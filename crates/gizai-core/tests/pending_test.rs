//! GA-102: pending actions in core. A call of one of the Team Lead's tools that act, refused in a chat answer because that
//! answer used a tool from outside Gizai, is kept exactly as called until you approve it (once) or discard it; a newer
//! write for the same note replaces the older one; the Team Lead hears each decision once; and a chat with calls that
//! wait counts them. Also the line diff the previews show, the hand-over that keeps your decisions, and migration 0017
//! on a database the previous version left.
use gizai_core::chat::{self, NewMessage};
use gizai_core::db::{self, Db};
use gizai_core::model::*;
use gizai_core::pending::{self, DiffLine, NewPending, PendingAction};
use gizai_core::{seed, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use serde_json::{Value, json};

struct T {
    db: Db,
    you: String,
    lead: String,
    thread: String,
}

/// You, a Team Lead with Chat on, and a chat with it.
fn setup() -> T {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    let thread = chat::create_thread(&db, &s.you_id, &lead, "Read the release notes in Otus and remember them").unwrap();
    T { db, you: s.you_id, lead, thread }
}

impl T {
    fn keep_in(&self, thread: &str, tool: &str, args: Value, note_key: Option<&str>) -> PendingAction {
        pending::keep(&self.db, NewPending { thread_id: thread.into(), run_id: Some("run-1".into()), actor_id: self.lead.clone(), tool: tool.into(),
            args, outside_tool: "mcp__otus__search".into(), note_key: note_key.map(str::to_string) }).unwrap()
    }
    fn keep(&self, tool: &str, args: Value, note_key: Option<&str>) -> PendingAction {
        self.keep_in(&self.thread, tool, args, note_key)
    }
    fn status(&self, id: &str) -> String {
        pending::get(&self.db, id).unwrap().status
    }
    fn waiting(&self) -> Vec<String> {
        pending::waiting(&self.db, &self.thread).unwrap().into_iter().map(|a| a.id).collect()
    }
    fn count(&self, thread: &str) -> i64 {
        chat::get_thread(&self.db, thread).unwrap().pending_actions
    }
    fn approve(&self, id: &str, ok: bool, text: &str) -> PendingAction {
        pending::claim(&self.db, &self.you, id).unwrap();
        pending::finish(&self.db, id, ok, text).unwrap()
    }
}

fn err(r: gizai_core::Result<PendingAction>) -> String {
    r.expect_err("refused").to_string()
}

/// Decisions a few ms apart, so their order is their time's.
fn tick() {
    std::thread::sleep(std::time::Duration::from_millis(3));
}

// ---- keeping a call ----

#[test]
fn a_kept_call_is_stored_exactly_as_called_and_its_chat_counts_it_until_you_decide() {
    let t = setup();
    let other = chat::create_thread(&t.db, &t.you, &t.lead, "Another question").unwrap();
    let args = json!({"path": "Lessons/Otus", "body_md": "Release notes go out on Fridays.\n\n- Two lines.", "version": 3, "nested": {"a": [1, 2]}});
    let a = t.keep("memory_write", args.clone(), Some("lessons/otus"));
    let got = pending::get(&t.db, &a.id).unwrap();
    assert_eq!((got.thread_id.as_str(), got.run_id.as_deref(), got.actor_id.as_str(), got.tool.as_str(), got.outside_tool.as_str(), got.status.as_str()),
               (t.thread.as_str(), Some("run-1"), t.lead.as_str(), "memory_write", "mcp__otus__search", "pending"));
    assert_eq!(got.args, args, "the arguments exactly as called");
    assert!(got.created_at > 0 && got.decided_at.is_none() && got.result_md.is_none());
    assert_eq!(t.waiting(), [a.id.clone()]);
    // the chat counts it, in every way it is listed; the other chat has none
    assert_eq!((t.count(&t.thread), t.count(&other)), (1, 0));
    let listed = chat::list_threads(&t.db).unwrap();
    assert_eq!(listed.iter().find(|c| c.id == t.thread).unwrap().pending_actions, 1);
    let hits = chat::search_threads(&t.db, "release notes").unwrap();
    assert_eq!(hits.iter().map(|h| (h.thread.id.as_str(), h.thread.pending_actions)).collect::<Vec<_>>(), [(t.thread.as_str(), 1)]);
    assert_eq!(chat::chats_with_pending(&t.db).unwrap().into_iter().map(|c| c.id).collect::<Vec<_>>(), [t.thread.clone()]);
    // a call that is decided no longer counts
    t.approve(&a.id, true, "Approved: save the note Lessons/Otus. Saved as version 1.");
    assert_eq!(t.count(&t.thread), 0);
    assert!(chat::chats_with_pending(&t.db).unwrap().is_empty());
}

#[test]
fn a_call_for_a_chat_that_isnt_there_is_not_kept() {
    let t = setup();
    let r = pending::keep(&t.db, NewPending { thread_id: "no-such-chat".into(), actor_id: t.lead.clone(), tool: "memory_write".into(),
        args: json!({"path": "X", "body_md": "y"}), outside_tool: "WebFetch".into(), ..Default::default() });
    assert!(r.is_err(), "{r:?}");
}

#[test]
fn a_newer_write_for_the_same_note_replaces_the_older_one_and_every_other_call_stays() {
    let t = setup();
    let other = chat::create_thread(&t.db, &t.you, &t.lead, "Another chat").unwrap();
    let first = t.keep("memory_write", json!({"path": "Lessons/Otus", "body_md": "First."}), Some("lessons/otus"));
    let same_elsewhere = t.keep_in(&other, "memory_write", json!({"path": "Lessons/Otus", "body_md": "Elsewhere."}), Some("lessons/otus"));
    let other_note = t.keep("memory_write", json!({"path": "Lessons/Other", "body_md": "Other."}), Some("lessons/other"));
    let pause = t.keep("set_agent_status", json!({"agent": "Backend Agent", "status": "paused"}), None);
    let rename = t.keep("set_column", json!({"column": "Testing", "new_name": "QA"}), None);
    // a memory_move names the note too, but only writes replace each other
    let moved = t.keep("memory_move", json!({"note": "Lessons/Otus", "to": "Decisions/"}), Some("lessons/otus"));
    // memory_append for the same note: the newest write, so the first one goes
    tick();
    let append = t.keep("memory_append", json!({"note": "Lessons/Otus", "text": "- Newer."}), Some("lessons/otus"));
    assert_eq!(t.status(&first.id), "replaced");
    assert!(pending::get(&t.db, &first.id).unwrap().decided_at.is_some());
    assert_eq!(t.waiting(), [other_note.id.clone(), pause.id.clone(), rename.id.clone(), moved.id.clone(), append.id.clone()], "in the order tried");
    // and a memory_write after it replaces the append in its turn
    let last = t.keep("memory_write", json!({"path": "Lessons/Otus", "body_md": "Last."}), Some("lessons/otus"));
    assert_eq!(t.status(&append.id), "replaced");
    assert_eq!(t.waiting(), [other_note.id.clone(), pause.id, rename.id, moved.id, last.id]);
    assert_eq!(t.count(&t.thread), 5);
    // another chat's call for that note is its own
    assert_eq!(t.status(&same_elsewhere.id), "pending");
    // a replaced call can't be approved or discarded
    assert_eq!(err(pending::claim(&t.db, &t.you, &first.id)), "It no longer waits: a newer call of the Team Lead's took its place.");
    assert_eq!(err(pending::discard(&t.db, &t.you, &first.id, "x")), "It no longer waits: a newer call of the Team Lead's took its place.");
}

#[test]
fn the_same_call_made_again_while_it_waits_is_kept_once() {
    let t = setup();
    let args = json!({"agent": "Backend Agent", "status": "paused"});
    let a = t.keep("set_agent_status", args.clone(), None);
    let b = t.keep("set_agent_status", args.clone(), None);
    assert_eq!(a.id, b.id, "it is that one");
    // the same tool with other arguments is another call
    let c = t.keep("set_agent_status", json!({"agent": "Backend Agent", "status": "active"}), None);
    assert_ne!(c.id, a.id);
    assert_eq!(t.waiting(), [a.id.clone(), c.id]);
    // once decided, the same call made again is a new one
    pending::discard(&t.db, &t.you, &a.id, "Discarded: pause Backend Agent.").unwrap();
    let d = t.keep("set_agent_status", args, None);
    assert_ne!(d.id, a.id);
    assert_eq!(t.status(&d.id), "pending");
}

// ---- approve and discard ----

#[test]
fn approve_claims_a_call_once_so_a_second_approval_or_a_discard_is_refused() {
    let t = setup();
    let a = t.keep("memory_append", json!({"note": "Team Lead/Notes", "text": "- Fridays."}), Some("team lead/notes"));
    let claimed = pending::claim(&t.db, &t.you, &a.id).unwrap();
    assert_eq!(claimed.status, "running");
    assert!(claimed.decided_at.is_some());
    // while it runs: shown under the answer (running), no longer counted as waiting for you
    assert_eq!(t.waiting(), [a.id.clone()]);
    assert_eq!(t.count(&t.thread), 0);
    assert_eq!(err(pending::claim(&t.db, &t.you, &a.id)), "It is being approved already.");
    assert_eq!(err(pending::discard(&t.db, &t.you, &a.id, "x")), "It is being approved already.");
    assert_eq!(pending::running(&t.db).unwrap().into_iter().map(|r| r.id).collect::<Vec<_>>(), [a.id.clone()]);
    let done = pending::finish(&t.db, &a.id, true, "Approved: add to the note Team Lead/Notes. Saved as version 2.").unwrap();
    assert_eq!((done.status.as_str(), done.result_md.as_deref()), ("approved", Some("Approved: add to the note Team Lead/Notes. Saved as version 2.")));
    assert!(t.waiting().is_empty() && pending::running(&t.db).unwrap().is_empty());
    for e in [err(pending::claim(&t.db, &t.you, &a.id)), err(pending::finish(&t.db, &a.id, true, "again"))] {
        assert_eq!(e, "It was approved already, so it doesn't run again.");
    }
    assert_eq!(pending::get(&t.db, &a.id).unwrap().result_md.as_deref(), Some("Approved: add to the note Team Lead/Notes. Saved as version 2."),
               "a second finish changes nothing");
    // one that failed counts as approved too: it doesn't run again
    let b = t.keep("merge_pull_request", json!({"task": "KADE-1"}), None);
    let failed = t.approve(&b.id, false, "Approved: merge KADE-1's pull request. It failed: QA hasn't passed it");
    assert_eq!(failed.status, "failed");
    assert_eq!(err(pending::claim(&t.db, &t.you, &b.id)), "It was approved already, so it doesn't run again.");
    // a finish without a claim is refused
    let c = t.keep("add_column", json!({"name": "Otus", "after": "Review"}), None);
    assert!(pending::finish(&t.db, &c.id, true, "x").is_err());
    assert_eq!(t.status(&c.id), "pending");
    assert!(pending::claim(&t.db, &t.you, "no-such-action").is_err());
}

#[test]
fn discard_runs_nothing_and_a_discarded_call_cant_be_approved() {
    let t = setup();
    let a = t.keep("create_agent", json!({"name": "Otus Agent", "role": "backend"}), None);
    let d = pending::discard(&t.db, &t.you, &a.id, "Discarded: add the agent Otus Agent (backend).").unwrap();
    assert_eq!((d.status.as_str(), d.result_md.as_deref()), ("discarded", Some("Discarded: add the agent Otus Agent (backend).")));
    assert!(d.decided_at.is_some());
    let by: Option<String> = t.db.read(|c| Ok(c.query_row("SELECT decided_by FROM pending_actions WHERE id=?1", [&a.id], |r| r.get(0))?)).unwrap();
    assert_eq!(by.as_deref(), Some(t.you.as_str()), "you decided it");
    assert_eq!(err(pending::claim(&t.db, &t.you, &a.id)), "It was discarded already.");
    assert_eq!(err(pending::discard(&t.db, &t.you, &a.id, "again")), "It was discarded already.");
    assert!(t.waiting().is_empty() && t.count(&t.thread) == 0);
}

// ---- the Team Lead hears it ----

#[test]
fn the_team_lead_hears_each_decision_once_in_the_order_you_made_them() {
    let t = setup();
    let a = t.keep("memory_write", json!({"path": "A", "body_md": "a"}), Some("a"));
    let b = t.keep("memory_write", json!({"path": "B", "body_md": "b"}), Some("b"));
    let c = t.keep("memory_write", json!({"path": "C", "body_md": "c"}), Some("c"));
    let waits = t.keep("memory_write", json!({"path": "D", "body_md": "d"}), Some("d"));
    let running = t.keep("memory_write", json!({"path": "E", "body_md": "e"}), Some("e"));
    t.keep("memory_write", json!({"path": "A", "body_md": "a2"}), Some("f")); // another note: a stays
    pending::discard(&t.db, &t.you, &b.id, "Discarded: save the note B.").unwrap();
    tick();
    t.approve(&a.id, true, "Approved: save the note A. Saved as version 1.");
    tick();
    t.approve(&c.id, false, "Approved: save the note C. It failed: C changed");
    tick();
    pending::claim(&t.db, &t.you, &running.id).unwrap();
    let untold: Vec<(String, String)> = pending::untold(&t.db, &t.thread).unwrap().into_iter().map(|x| (x.id, x.status)).collect();
    assert_eq!(untold, [(b.id.clone(), "discarded".to_string()), (a.id.clone(), "approved".into()), (c.id.clone(), "failed".into())],
               "decided ones only, in the order decided; not {} (waits) nor the one still running", waits.id);
    pending::mark_told(&t.db, &[b.id.clone(), a.id.clone()]).unwrap();
    assert_eq!(pending::untold(&t.db, &t.thread).unwrap().into_iter().map(|x| x.id).collect::<Vec<_>>(), [c.id.clone()]);
    pending::mark_told(&t.db, &[]).unwrap();
    pending::mark_told(&t.db, &[c.id.clone(), b.id.clone()]).unwrap();
    assert!(pending::untold(&t.db, &t.thread).unwrap().is_empty());
    // another chat's decisions are its own
    let other = chat::create_thread(&t.db, &t.you, &t.lead, "Other").unwrap();
    let x = t.keep_in(&other, "add_column", json!({"name": "X", "after": "Review"}), None);
    pending::discard(&t.db, &t.you, &x.id, "Discarded: add the column X after Review.").unwrap();
    assert!(pending::untold(&t.db, &t.thread).unwrap().is_empty());
    assert_eq!(pending::untold(&t.db, &other).unwrap().len(), 1);
}

#[test]
fn the_same_call_made_and_run_later_replaces_the_one_that_waits_and_nothing_else() {
    let t = setup();
    let other = chat::create_thread(&t.db, &t.you, &t.lead, "Other").unwrap();
    let args = json!({"note": "Team Lead/Notes", "heading": "Working agreements", "text": "- Fridays."});
    let a = t.keep("memory_append", args.clone(), Some("n1"));
    let b = t.keep("memory_append", json!({"note": "Team Lead/Notes", "text": "- Something else."}), Some("n2"));
    let elsewhere = t.keep_in(&other, "memory_append", args.clone(), Some("n1"));
    // nothing waits for that call: nothing changes
    assert!(pending::made(&t.db, &t.thread, "memory_write", &args, "why").unwrap().is_empty());
    let gone = pending::made(&t.db, &t.thread, "memory_append", &args, "The Team Lead made the same call itself in a later answer.").unwrap();
    assert_eq!(gone.iter().map(|g| (g.id.as_str(), g.status.as_str())).collect::<Vec<_>>(), [(a.id.as_str(), "replaced")]);
    assert_eq!(gone[0].result_md.as_deref(), Some("The Team Lead made the same call itself in a later answer."));
    assert_eq!(t.waiting(), [b.id.clone()]);
    assert_eq!(pending::get(&t.db, &elsewhere.id).unwrap().status, "pending", "another chat's call stays");
    // a call already being approved isn't touched
    pending::claim(&t.db, &t.you, &b.id).unwrap();
    assert!(pending::made(&t.db, &t.thread, "memory_append", &json!({"note": "Team Lead/Notes", "text": "- Something else."}), "why").unwrap().is_empty());
    assert_eq!(t.status(&b.id), "running");
}

#[test]
fn running_lists_the_approved_calls_not_finished_in_every_chat() {
    let t = setup();
    let other = chat::create_thread(&t.db, &t.you, &t.lead, "Other").unwrap();
    let a = t.keep("update_checkout", json!({"project": "KADE"}), None);
    let b = t.keep_in(&other, "attach_file", json!({"path": "/tmp/x.txt", "task": "KADE-1"}), None);
    t.keep("set_column", json!({"column": "Testing", "auto": true}), None);
    pending::claim(&t.db, &t.you, &a.id).unwrap();
    pending::claim(&t.db, &t.you, &b.id).unwrap();
    assert_eq!(pending::running(&t.db).unwrap().into_iter().map(|r| r.id).collect::<Vec<_>>(), [a.id.clone(), b.id]);
    pending::finish(&t.db, &a.id, false, "Gizai closed while it ran").unwrap();
    assert_eq!(pending::running(&t.db).unwrap().len(), 1);
}

// ---- the diff a preview shows ----

fn diff(old: &str, new: &str) -> Vec<(&'static str, String)> {
    pending::line_diff(old, new).into_iter().map(|DiffLine { kind, text }| (kind, text)).collect()
}

fn lines(spec: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    spec.iter().map(|(k, t)| (*k, t.to_string())).collect()
}

#[test]
fn line_diff_shows_what_changes_line_by_line_with_each_removed_run_before_what_replaces_it() {
    assert_eq!(diff("a\nb\nc", "a\nb\nc"), lines(&[("same", "a"), ("same", "b"), ("same", "c")]));
    assert_eq!(diff("a\nb\nc", "a\nB\nc"), lines(&[("same", "a"), ("del", "b"), ("add", "B"), ("same", "c")]));
    assert_eq!(diff("a\nx\ny\nb", "a\np\nb"), lines(&[("same", "a"), ("del", "x"), ("del", "y"), ("add", "p"), ("same", "b")]));
    // a line both keep, in the middle of changes
    assert_eq!(diff("1\n2\n3\n4", "1\n3\n5"), lines(&[("same", "1"), ("del", "2"), ("same", "3"), ("del", "4"), ("add", "5")]));
    // added at the end, removed at the start
    assert_eq!(diff("# Notes\n", "# Notes\n\n- New."), lines(&[("same", "# Notes"), ("add", ""), ("add", "- New.")]));
    assert_eq!(diff("old\nkeep", "keep"), lines(&[("del", "old"), ("same", "keep")]));
    // from nothing, and to nothing
    assert_eq!(diff("", "one\ntwo"), lines(&[("add", "one"), ("add", "two")]));
    assert_eq!(diff("one\ntwo", ""), lines(&[("del", "one"), ("del", "two")]));
    assert!(diff("", "").is_empty());
    // every line of both texts is there, in order
    let (old, new) = ("a\nb\nc\nd\ne\nf", "a\nc\nX\nd\nf\ng");
    let d = diff(old, new);
    assert_eq!(d.iter().filter(|l| l.0 != "add").map(|l| l.1.as_str()).collect::<Vec<_>>().join("\n"), old);
    assert_eq!(d.iter().filter(|l| l.0 != "del").map(|l| l.1.as_str()).collect::<Vec<_>>().join("\n"), new);
}

#[test]
fn line_diff_of_two_long_texts_with_little_in_common_is_quick_and_still_complete() {
    let old: String = (0..3000).map(|i| format!("old line {i}\n")).collect();
    let new: String = (0..3000).map(|i| format!("new line {i}\n")).collect();
    let (old, new) = (format!("# Notes\n{old}end\n"), format!("# Notes\n{new}end\n"));
    let t0 = std::time::Instant::now();
    let d = pending::line_diff(&old, &new);
    assert!(t0.elapsed() < std::time::Duration::from_secs(5), "took {:?}", t0.elapsed());
    assert_eq!(d.first().map(|l| (l.kind, l.text.as_str())), Some(("same", "# Notes")));
    assert_eq!(d.last().map(|l| (l.kind, l.text.as_str())), Some(("same", "end")));
    assert_eq!((d.iter().filter(|l| l.kind == "del").count(), d.iter().filter(|l| l.kind == "add").count()), (3000, 3000));
    assert_eq!(d.len(), 6002);
}

// ---- the hand-over to a new session ----

#[test]
fn the_hand_over_keeps_your_decisions_and_leaves_gizais_other_notes_out() {
    let t = setup();
    let say = |role: &str, body: &str, meta: Option<Value>| {
        chat::add_message(&t.db, NewMessage { thread_id: t.thread.clone(), role: role.into(), body_md: Some(body.into()), meta, ..Default::default() }).unwrap();
    };
    say("user", "Read the release notes in Otus and remember them", None);
    say("agent", "I couldn't save it: approve the note below.", None);
    say("system", "Stopped.", None);
    say("system", "This chat moved to Codex.", Some(json!({"kind": "switch"})));
    say("system", "Approved: save the note Lessons/Otus. Saved as version 1.", Some(json!({"kind": "pending", "action": "a1", "status": "approved"})));
    say("system", "Discarded: pause Backend Agent.", Some(json!({"kind": chat::DECISION_NOTE, "action": "a2", "status": "discarded"})));
    let msgs = chat::messages(&t.db, &t.thread).unwrap();
    assert_eq!(msgs.iter().filter(|m| chat::is_decision_note(m)).count(), 2);
    let h = chat::handover(&msgs, 10_000);
    assert!(h.contains("User: Read the release notes in Otus") && h.contains("You: I couldn't save it"), "{h}");
    assert!(h.contains("(Gizai: Approved: save the note Lessons/Otus. Saved as version 1.)"), "{h}");
    assert!(h.contains("(Gizai: Discarded: pause Backend Agent.)"), "{h}");
    assert!(!h.contains("Stopped.") && !h.contains("moved to Codex"), "Gizai's other notes stay out: {h}");
    // a note of yours or the Team Lead's with that meta isn't a decision
    say("user", "kind pending", Some(json!({"kind": "pending"})));
    assert!(!chat::is_decision_note(chat::messages(&t.db, &t.thread).unwrap().last().unwrap()));
}

// ---- migration 0017 on the previous version's database ----

/// A genuine schema 16 database file (migrations 0001..0016) with a person, a Team Lead and one chat with a message.
fn v16_db(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("gizai.db");
    let mut c = Connection::open(&path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    let all = [
        include_str!("../migrations/0001_init.sql"), include_str!("../migrations/0002_agents.sql"), include_str!("../migrations/0003_chat.sql"),
        include_str!("../migrations/0004_effort.sql"), include_str!("../migrations/0005_pull_requests.sql"),
        include_str!("../migrations/0006_worktree_prepare.sql"), include_str!("../migrations/0007_card_flow.sql"),
        include_str!("../migrations/0008_board_check.sql"), include_str!("../migrations/0009_agent_folders.sql"),
        include_str!("../migrations/0010_run_refusals.sql"), include_str!("../migrations/0011_column_agents.sql"),
        include_str!("../migrations/0012_chat_runs_on.sql"), include_str!("../migrations/0013_bitbucket.sql"),
        include_str!("../migrations/0014_memory.sql"), include_str!("../migrations/0015_lead_merges.sql"),
        include_str!("../migrations/0016_memory_groups.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 1, "one schema step back");
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch("
        INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
        INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
          ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active'),
          ('lead', 1, 1, 'org', 'agent', 'Team Lead', 'lead', 'active');
        INSERT INTO agent_configs (actor_id, created_at, updated_at, adapter, wakeup, chat_enabled) VALUES ('lead', 1, 1, 'claude_code', 'manual', 1);
        INSERT INTO chat_threads (id, created_at, updated_at, created_by, org_id, agent_actor_id, title) VALUES ('chat', 5, 6, 'you', 'org', 'lead', 'Release notes');
        INSERT INTO chat_messages (id, created_at, thread_id, role, author_actor_id, body_md) VALUES ('m1', 5, 'chat', 'user', 'you', 'Read Otus');
        INSERT INTO devices (id, created_at, name, is_self) VALUES ('dev', 1, 'test', 1);").unwrap();
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 16, "the fixture is schema 16");
    path
}

#[test]
fn migration_0017_on_a_v16_database_adds_pending_actions_after_a_backup_and_every_chat_keeps_working() {
    let dir = tempfile::tempdir().unwrap();
    let path = v16_db(dir.path());
    let db = Db::open(&path).unwrap();
    assert_eq!(db::SCHEMA_VERSION, 17);
    let (v, broken): (i64, i64) = db.read(|c| Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?))).unwrap();
    assert_eq!((v, broken), (17, 0), "schema 17, foreign keys intact");
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(snaps.len() == 1 && snaps[0].starts_with("gizai-before-v17-"), "{snaps:?}");
    let old = Connection::open(dir.path().join("backups").join(&snaps[0])).unwrap();
    assert_eq!(old.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 16, "the backup is the schema 16 database");
    // the table, empty, with its index
    let (rows, index): (i64, i64) = db.read(|c| Ok((c.query_row("SELECT count(*) FROM pending_actions", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM sqlite_master WHERE type='index' AND name='pending_actions_thread'", [], |r| r.get(0))?))).unwrap();
    assert_eq!((rows, index), (0, 1));
    // the old chat lists as before, with nothing waiting, and a call can be kept in it
    let chat = chat::get_thread(&db, "chat").unwrap();
    assert_eq!((chat.title.as_str(), chat.pending_actions), ("Release notes", 0));
    assert_eq!(chat::messages(&db, "chat").unwrap().len(), 1);
    let a = pending::keep(&db, NewPending { thread_id: "chat".into(), actor_id: "lead".into(), tool: "memory_write".into(),
        args: json!({"path": "Lessons/Otus", "body_md": "Fridays."}), outside_tool: "WebFetch".into(), note_key: Some("lessons/otus".into()), ..Default::default() }).unwrap();
    assert_eq!(chat::get_thread(&db, "chat").unwrap().pending_actions, 1);
    // a status that isn't one of the six is refused
    assert!(db.read(|c| Ok(c.execute("UPDATE pending_actions SET status='maybe' WHERE id=?1", [&a.id])?)).is_err());
    drop(db);
    // opening it again changes nothing and makes no new backup
    let db = Db::open(&path).unwrap();
    assert_eq!(pending::get(&db, &a.id).unwrap().status, "pending");
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
}
