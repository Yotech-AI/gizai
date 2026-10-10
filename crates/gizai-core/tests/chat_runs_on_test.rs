// GA-50 in the record: each chat's own Runs on, the hand-over a new session gets, the queue of messages sent while the
// Team Lead answers, and three of GA-23's fixes (a failed resumed turn's cost, the interrupted note, the schema step).
use gizai_core::chat::{self, NewMessage, Totals};
use gizai_core::clis::{self, Cli};
use gizai_core::model::*;
use gizai_core::{db, db::Db, runs, seed, team};
use serde_json::json;

fn setup() -> (Db, seed::SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    (db, s)
}

fn lead(db: &Db, s: &seed::SeedIds) -> String {
    team::add_agent(db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap()
}

/// Settings → Coding CLIs with a second Claude Code account and Codex. Returns (Claude Code 2's id, Codex's id).
fn two_more_clis(db: &Db) -> (String, String) {
    let all = clis::save(db, vec![
        Cli { name: "Claude Code 2".into(), kind: "claude_code".into(), command: "claude".into(), env: vec!["CLAUDE_CONFIG_DIR=/tmp/acct-2".into()], ..Default::default() },
        Cli { name: "Codex".into(), kind: "codex".into(), command: "codex".into(), ..Default::default() },
    ]).unwrap();
    let id = |n: &str| all.iter().find(|c| c.name == n).unwrap().id.clone();
    (id("Claude Code 2"), id("Codex"))
}

fn say(db: &Db, thread: &str, role: &str, text: &str) -> String {
    chat::add_message(db, NewMessage { thread_id: thread.into(), role: role.into(), body_md: Some(text.into()), ..Default::default() }).unwrap().id
}

// Runs on per chat

#[test]
fn a_chat_without_its_own_pick_follows_the_team_leads_runs_on() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, _) = two_more_clis(&db);
    let th = chat::get_thread(&db, &chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap()).unwrap();
    assert_eq!(th.cli, None);
    assert_eq!(chat::runs_on(&db, &th, None).unwrap().id, clis::CLAUDE_CODE, "the Team Lead's Runs on, here the built-in Claude Code");
    assert_eq!(chat::runs_on(&db, &th, Some(&cc2)).unwrap().id, cc2, "the agent form moved the Team Lead to Claude Code 2");
}

#[test]
fn a_picked_cli_is_saved_on_the_chat_and_kept_after_reopening() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, _) = two_more_clis(&db);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    let th = chat::set_cli(&db, &s.you_id, &id, Some(&cc2)).unwrap();
    assert_eq!(th.cli.as_deref(), Some(cc2.as_str()));
    let again = chat::get_thread(&db, &id).unwrap();
    assert_eq!(again.cli.as_deref(), Some(cc2.as_str()));
    assert_eq!(chat::runs_on(&db, &again, None).unwrap().name, "Claude Code 2", "its own pick wins over the Team Lead's");
    assert_eq!(chat::list_threads(&db).unwrap()[0].cli.as_deref(), Some(cc2.as_str()), "the chat list carries it too");
    // None: it follows the Team Lead again
    assert_eq!(chat::set_cli(&db, &s.you_id, &id, None).unwrap().cli, None);
    assert_eq!(chat::set_cli(&db, &s.you_id, &id, Some("  ")).unwrap().cli, None, "empty is no pick");
}

#[test]
fn a_new_chat_can_start_on_its_own_cli() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, codex) = two_more_clis(&db);
    let id = chat::create_thread_on(&db, &s.you_id, &lead, "Hi", Some(&cc2)).unwrap();
    assert_eq!(chat::get_thread(&db, &id).unwrap().cli.as_deref(), Some(cc2.as_str()));
    assert!(chat::create_thread_on(&db, &s.you_id, &lead, "Hi", Some(&codex)).is_err());
}

#[test]
fn only_a_claude_code_cli_from_settings_can_be_picked() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, codex) = two_more_clis(&db);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    chat::set_cli(&db, &s.you_id, &id, Some(&cc2)).unwrap();
    let e = chat::set_cli(&db, &s.you_id, &id, Some(&codex)).unwrap_err().to_string();
    assert!(e.contains("Codex can't run the chat: the chat runs on Claude Code only"), "{e}");
    let e = chat::set_cli(&db, &s.you_id, &id, Some("nope")).unwrap_err().to_string();
    assert!(e.contains("there is no coding CLI with id nope"), "{e}");
    assert_eq!(chat::get_thread(&db, &id).unwrap().cli.as_deref(), Some(cc2.as_str()), "a refused pick leaves the chat alone");
    assert!(chat::set_cli(&db, &s.you_id, "no-such-chat", Some(&cc2)).is_err());
    assert_eq!(clis::chat_problem(&clis::get(&db, &codex).unwrap()).as_deref(), Some("the chat runs on Claude Code only"));
    assert_eq!(clis::chat_problem(&clis::get(&db, &cc2).unwrap()), None);
    assert_eq!(clis::chat_problem(&clis::builtin(&db)), None);
}

#[test]
fn a_pick_removed_from_settings_falls_back_to_the_team_leads_runs_on() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, _) = two_more_clis(&db);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    chat::set_cli(&db, &s.you_id, &id, Some(&cc2)).unwrap();
    clis::save(&db, vec![]).unwrap();
    let th = chat::get_thread(&db, &id).unwrap();
    assert_eq!(chat::runs_on(&db, &th, None).unwrap().id, clis::CLAUDE_CODE);
}

#[test]
fn picking_a_cli_for_a_chat_leaves_the_team_leads_own_runs_on_alone() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, _) = two_more_clis(&db);
    let before = team::agent(&db, &lead).unwrap().adapter;
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    chat::set_cli(&db, &s.you_id, &id, Some(&cc2)).unwrap();
    assert_eq!(team::agent(&db, &lead).unwrap().adapter, before, "board checks and task runs keep the agent's Runs on");
    // a chat turn records the CLI it ran on; without one, the agent's own
    let r = runs::create_chat_on(&db, &lead, &id, Some(&cc2), "S", "/tmp/lead", "/tmp/r.jsonl").unwrap();
    assert_eq!(runs::get(&db, &r).unwrap().adapter.as_deref(), Some(cc2.as_str()));
    let r = runs::create_chat(&db, &lead, &id, "S", "/tmp/lead", "/tmp/r.jsonl").unwrap();
    assert_eq!(runs::get(&db, &r).unwrap().adapter, before);
}

#[test]
fn a_session_remembers_the_cli_whose_account_holds_it() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let (cc2, _) = two_more_clis(&db);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    chat::record_session_on(&db, &id, "S2", &cc2, Totals { cost_usd_micros: 5, input_tokens: 6, output_tokens: 7 }).unwrap();
    let th = chat::get_thread(&db, &id).unwrap();
    assert_eq!((th.session_id.as_deref(), th.session_cli.as_deref()), (Some("S2"), Some(cc2.as_str())));
    chat::reset_session(&db, &id).unwrap();
    let th = chat::get_thread(&db, &id).unwrap();
    assert_eq!((th.session_id, th.session_cli), (None, None));
}

// The hand-over

#[test]
fn the_hand_over_holds_the_whole_chat_oldest_first_with_each_tool_calls_name_target_and_result() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    // more than the 20 messages the old retry kept
    for i in 1..=15 {
        say(&db, &id, "user", &format!("Question {i}"));
        say(&db, &id, "agent", &format!("Answer {i}"));
    }
    let tool = chat::add_message(&db, NewMessage { thread_id: id.clone(), role: "tool".into(), tool_name: Some("mcp__gizai__get_task".into()),
        tool: Some(json!({"id": "t1", "input": {"task": "GA-12", "limit": 5}})), ..Default::default() }).unwrap();
    chat::set_tool_result(&db, &tool.id, &format!("{{\"title\": \"Export invoices\", \"body\": \"{}\"}}", "x".repeat(1000)), false).unwrap();
    let bad = chat::add_message(&db, NewMessage { thread_id: id.clone(), role: "tool".into(), tool_name: Some("mcp__gizai__get_doc".into()),
        tool: Some(json!({"id": "t2", "input": {"doc": "Requirements"}})), ..Default::default() }).unwrap();
    chat::set_tool_result(&db, &bad.id, "no doc called Requirements", true).unwrap();
    chat::add_message(&db, NewMessage { thread_id: id.clone(), role: "tool".into(), tool_name: Some("Read".into()),
        tool: Some(json!({"id": "t3", "input": {"file_path": "/code/KADE/src/main.rs"}})), ..Default::default() }).unwrap();
    say(&db, &id, "system", "Stopped.");
    say(&db, &id, "agent", "Last answer");

    let h = chat::handover(&chat::messages(&db, &id).unwrap(), chat::HANDOVER_CAP);
    assert!(h.starts_with("User: Question 1\n\nYou: Answer 1\n\nUser: Question 2"), "{h}");
    assert!(!h.contains("left out"), "nothing was cut: {h}");
    let first = h.find("Question 1\n").unwrap();
    assert!(first < h.find("Question 15").unwrap() && h.find("Question 15").unwrap() < h.find("get_task").unwrap());
    // name, what it was called on, and the start of the result (not all of it)
    let line = h.lines().find(|l| l.contains("get_task")).unwrap();
    assert!(line.starts_with("(You called get_task (") && line.contains("task: GA-12") && line.contains("limit: 5"), "{line}");
    assert!(line.contains("its result began: {\"title\": \"Export invoices\""), "{line}");
    assert!(line.contains('…') && line.len() < 700, "only the start of the result: {} chars", line.len());
    let line = h.lines().find(|l| l.contains("get_doc")).unwrap();
    assert!(line.contains("doc: Requirements") && line.contains("it failed: no doc called Requirements"), "{line}");
    let line = h.lines().find(|l| l.contains("You called Read")).unwrap();
    assert!(line.contains("file_path: /code/KADE/src/main.rs") && line.contains("it had no result"), "{line}");
    assert!(!h.contains("Stopped."), "Gizai's own notes are left out");
    assert!(h.ends_with("You: Last answer"), "{h}");
}

#[test]
fn a_hand_over_longer_than_the_cap_drops_the_oldest_messages_first_and_says_so() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    for i in 1..=60 {
        say(&db, &id, if i % 2 == 1 { "user" } else { "agent" }, &format!("Message {i:02} {}", "word ".repeat(200)));
    }
    let msgs = chat::messages(&db, &id).unwrap();
    let h = chat::handover(&msgs, chat::HANDOVER_CAP);
    let first = h.lines().next().unwrap();
    assert!(first.starts_with("(The ") && first.ends_with(" oldest messages are left out, to keep this short.)"), "{first}");
    let dropped: usize = first.trim_start_matches("(The ").split(' ').next().unwrap().parse().unwrap();
    assert!(dropped > 0 && dropped < 60);
    assert!(!h.contains("Message 01 ") && !h.contains(&format!("Message {dropped:02} ")), "the oldest go");
    assert!(h.contains(&format!("Message {:02} ", dropped + 1)) && h.contains("Message 60 "), "the newest stay");
    assert!(h.chars().count() <= chat::HANDOVER_CAP + first.chars().count() + 2, "{} chars", h.chars().count());
    // a smaller cap keeps fewer; one message is "message is"
    let small = chat::handover(&msgs[58..], 1500);
    assert!(small.starts_with("(The 1 oldest message is left out, to keep this short.)") && small.contains("Message 60 "), "{small}");
    assert_eq!(chat::handover(&[], chat::HANDOVER_CAP), "");
}

#[test]
fn messages_that_go_together_are_joined_oldest_first_and_one_goes_as_it_is() {
    assert_eq!(chat::joined(&["Only one".into()]), "Only one");
    assert_eq!(chat::joined(&["First".into(), "Second".into()]), "(2 messages, written while you answered, oldest first:)\n\nFirst\n\nSecond");
}

// The queue

#[test]
fn queued_messages_keep_their_order_can_be_edited_or_removed_and_go_as_separate_messages() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    say(&db, &id, "user", "Hi");
    let a = chat::enqueue(&db, &s.you_id, &id, "  First  ").unwrap();
    let b = chat::enqueue(&db, &s.you_id, &id, "Second").unwrap();
    let c = chat::enqueue(&db, &s.you_id, &id, "Third").unwrap();
    assert!(!a.held && a.body_md == "First");
    assert_eq!(chat::queue(&db, &id).unwrap().iter().map(|q| q.body_md.as_str()).collect::<Vec<_>>(), ["First", "Second", "Third"]);
    assert_eq!(chat::edit_queued(&db, &s.you_id, &b.id, "Second, changed").unwrap().body_md, "Second, changed");
    chat::remove_queued(&db, &s.you_id, &c.id).unwrap();
    assert!(chat::queue_ready(&db, &id).unwrap());
    assert!(chat::enqueue(&db, &s.you_id, &id, "   ").is_err(), "an empty message isn't queued");
    assert!(chat::enqueue(&db, &s.you_id, "no-such-chat", "x").is_err());
    assert_eq!(chat::messages(&db, &id).unwrap().len(), 1, "queued messages aren't in the chat yet");

    let sent = chat::send_queued(&db, &id, false).unwrap();
    assert_eq!(sent.iter().map(|m| (m.role.as_str(), m.body_md.as_deref().unwrap())).collect::<Vec<_>>(), [("user", "First"), ("user", "Second, changed")]);
    assert!(sent.iter().all(|m| m.author_id.as_deref() == Some(s.you_id.as_str())));
    assert!(chat::queue(&db, &id).unwrap().is_empty());
    assert_eq!(chat::messages(&db, &id).unwrap().len(), 3, "each its own message in the chat");
    // once gone, it can't be changed or removed
    let e = chat::edit_queued(&db, &s.you_id, &a.id, "late").unwrap_err().to_string();
    assert!(e.contains("gone already"), "{e}");
    assert!(chat::remove_queued(&db, &s.you_id, &a.id).unwrap_err().to_string().contains("gone already"));
    assert!(chat::send_queued(&db, &id, false).unwrap().is_empty());
}

#[test]
fn after_a_stopped_or_failed_answer_the_queue_waits_for_send_now() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    chat::enqueue(&db, &s.you_id, &id, "One").unwrap();
    chat::enqueue(&db, &s.you_id, &id, "Two").unwrap();
    assert_eq!(chat::hold_queue(&db, &id).unwrap(), 2);
    assert!(chat::queue(&db, &id).unwrap().iter().all(|q| q.held));
    assert!(!chat::queue_ready(&db, &id).unwrap(), "nothing goes by itself");
    assert!(chat::send_queued(&db, &id, false).unwrap().is_empty());
    // a message queued later, during the next answer, goes by itself; Send now takes the waiting ones too
    chat::enqueue(&db, &s.you_id, &id, "Three").unwrap();
    assert!(chat::queue_ready(&db, &id).unwrap());
    let sent = chat::send_queued(&db, &id, true).unwrap();
    assert_eq!(sent.iter().map(|m| m.body_md.as_deref().unwrap()).collect::<Vec<_>>(), ["One", "Two", "Three"]);
    // Send now while an answer is being written: the waiting ones go when it is done
    chat::enqueue(&db, &s.you_id, &id, "Four").unwrap();
    chat::hold_queue(&db, &id).unwrap();
    assert_eq!(chat::release_queue(&db, &id).unwrap(), 1);
    assert!(chat::queue_ready(&db, &id).unwrap());
}

#[test]
fn the_queue_is_saved_with_the_chat_and_waits_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let (you, a, b) = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let lead = lead(&db, &s);
        let a = chat::create_thread(&db, &s.you_id, &lead, "A").unwrap();
        let b = chat::create_thread(&db, &s.you_id, &lead, "B").unwrap();
        chat::enqueue(&db, &s.you_id, &a, "for A").unwrap();
        chat::enqueue(&db, &s.you_id, &b, "for B").unwrap();
        (s.you_id, a, b)
    };
    let db = Db::open(&path).unwrap();
    assert_eq!(chat::queue(&db, &a).unwrap()[0].body_md, "for A", "still there");
    assert_eq!(chat::hold_all_queues(&db).unwrap(), 2);
    for t in [&a, &b] {
        assert!(chat::queue(&db, t).unwrap().iter().all(|q| q.held), "it waits, as after a failure");
    }
    assert!(chat::enqueue(&db, &you, &a, "more").is_ok());
}

// GA-23's fixes in the record

#[test]
fn a_failed_resumed_turn_that_reports_its_cost_without_tokens_is_counted_once() {
    // The session had cost 0.02 with 2000/200 tokens; the failed resume reports 0.03 cumulative and no tokens.
    let prev = Totals { cost_usd_micros: 20_000, input_tokens: 2000, output_tokens: 200 };
    let now = Totals { cost_usd_micros: 30_000, input_tokens: 0, output_tokens: 0 };
    assert_eq!(chat::turn_cost(prev, now).cost_usd_micros, 10_000, "only this turn's 0.01, not the session's 0.03 again");
    assert_eq!(now.at_least(prev), Totals { cost_usd_micros: 30_000, input_tokens: 2000, output_tokens: 200 }, "the session's totals only grow");
}

#[test]
fn a_chat_answer_left_running_by_a_crash_gets_an_interrupted_note() {
    let (db, s) = setup();
    let lead = lead(&db, &s);
    let id = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    say(&db, &id, "user", "Hi");
    let r = runs::create_chat(&db, &lead, &id, "S", "/tmp/lead", "/tmp/r.jsonl").unwrap();
    runs::set_running(&db, &r, 4242).unwrap();
    let other = chat::create_thread(&db, &s.you_id, &lead, "Done").unwrap();
    let done = runs::create_chat(&db, &lead, &other, "S2", "/tmp/lead", "/tmp/r2.jsonl").unwrap();
    runs::finish_chat(&db, &done, "succeeded", 1, 1, 1, None).unwrap();

    assert_eq!(runs::recover_interrupted(&db).unwrap(), 1);
    let run = runs::get(&db, &r).unwrap();
    assert_eq!((run.status.as_str(), run.error.as_deref(), run.outcome.as_deref()), ("failed", Some("interrupted"), None));
    let last = chat::messages(&db, &id).unwrap().pop().unwrap();
    assert_eq!((last.role.as_str(), last.body_md.as_deref(), last.run_id.as_deref()), ("system", Some(chat::INTERRUPTED), Some(r.as_str())));
    assert!(chat::messages(&db, &other).unwrap().is_empty(), "a finished answer gets no note");
    assert_eq!(runs::recover_interrupted(&db).unwrap(), 0, "once");
}

#[test]
fn schema_10_gives_existing_sessions_the_cli_of_their_last_turn() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let (id, fresh) = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let lead = lead(&db, &s);
        let id = chat::create_thread(&db, &s.you_id, &lead, "Old chat").unwrap();
        runs::create_chat_on(&db, &lead, &id, Some("claude_code"), "OLD", "/tmp/lead", "/tmp/a.jsonl").unwrap();
        runs::create_chat_on(&db, &lead, &id, Some("acct-2"), "S1", "/tmp/lead", "/tmp/b.jsonl").unwrap();
        chat::record_session(&db, &id, "S1", Totals::default()).unwrap();
        let fresh = chat::create_thread(&db, &s.you_id, &lead, "No session yet").unwrap();
        say(&db, &id, "user", "Hi");
        (id, fresh)
    };
    // One schema step back: 0012 hadn't run (nor GA-19's 0014 and GA-86's 0015, which come after it).
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch(&format!("PRAGMA foreign_keys=OFF; BEGIN; ALTER TABLE projects DROP COLUMN lead_may_merge; {} COMMIT;", undo_0014())).unwrap();
    c.execute_batch("DROP TABLE chat_queue; ALTER TABLE chat_messages DROP COLUMN meta_json;
                     ALTER TABLE chat_threads DROP COLUMN session_cli; ALTER TABLE chat_threads DROP COLUMN cli; PRAGMA user_version = 11;").unwrap();
    drop(c);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.read(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?)).unwrap(), db::SCHEMA_VERSION);
    let th = chat::get_thread(&db, &id).unwrap();
    assert_eq!((th.cli, th.session_id.as_deref(), th.session_cli.as_deref()), (None, Some("S1"), Some("acct-2")), "the CLI of the turn that ran in S1");
    assert_eq!(chat::get_thread(&db, &fresh).unwrap().session_cli, None);
    assert_eq!(chat::messages(&db, &id).unwrap()[0].meta, None);
    assert!(chat::queue(&db, &id).unwrap().is_empty());
}

/// Undoes GA-19's 0014: docs rebuilt as 0001 made them (owner_actor_id has a foreign key, so it can't be dropped), its
/// memory notes and their versions and links gone, no use_memory or memory_json. Runs inside an open transaction.
fn undo_0014() -> String {
    let m1 = include_str!("../migrations/0001_init.sql");
    let start = m1.find("CREATE TABLE docs (").unwrap();
    let docs = m1[start..start + m1[start..].find(") STRICT;").unwrap() + ") STRICT;".len()].replacen("CREATE TABLE docs (", "CREATE TABLE docs_v13 (", 1);
    format!("DELETE FROM doc_links WHERE source_type = 'doc' AND source_id IN (SELECT id FROM docs WHERE kind = 'memory');
        DELETE FROM doc_versions WHERE doc_id IN (SELECT id FROM docs WHERE kind = 'memory');
        {docs}; INSERT INTO docs_v13 SELECT id, created_at, updated_at, deleted_at, version, created_by, updated_by, org_id, project_id, client_id,
        parent_id, title, body_md, mirror_path, current_version, sort_key FROM docs WHERE kind = 'doc';
        DROP TABLE docs; ALTER TABLE docs_v13 RENAME TO docs;
        ALTER TABLE agent_configs DROP COLUMN use_memory; ALTER TABLE runs DROP COLUMN memory_json;")
}
