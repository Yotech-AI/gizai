//! GA-191 in the core: reads go on beside a write and beside a long read (the read-only connections of `Db::read`), the
//! window's lists leave out what the board doesn't show (`tasks::list_lean`, `team::get_lean`) while the full ones keep
//! it, the rewritten queries read through an index and return the rows the old ones did, and `PRAGMA optimize` runs when
//! the database opens and in the daily housekeeping.
use gizai_core::board::{self, Context};
use gizai_core::chat::{self, NewMessage};
use gizai_core::db::{self, Db};
use gizai_core::model::*;
use gizai_core::pending::{self, NewPending};
use gizai_core::{comments, housekeeping, ids, projects, runs, seed, tasks, team};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const TASKS_RS: &str = include_str!("../src/tasks.rs");
const CHAT_RS: &str = include_str!("../src/chat.rs");
const BOARD_RS: &str = include_str!("../src/board.rs");
const DB_RS: &str = include_str!("../src/db.rs");
const HOUSEKEEPING_RS: &str = include_str!("../src/housekeeping.rs");

fn file_db(dir: &tempfile::TempDir) -> Arc<Db> {
    Arc::new(Db::open(&dir.path().join("gizai.db")).unwrap())
}

fn name_of(db: &Db, project: &str) -> String {
    db.read(|c| Ok(c.query_row("SELECT name FROM projects WHERE id = ?1", [project], |r| r.get(0))?)).unwrap()
}

/// `f` on a thread of its own; its answer, or None when it took longer than `limit`.
fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> (Option<T>, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let h = std::thread::spawn(move || { let _ = tx.send(f()); });
    (rx.recv_timeout(limit).ok(), h)
}

// ---- reads beside writes ----

#[test]
fn a_read_finishes_while_another_thread_holds_a_write_transaction_open_and_sees_only_what_was_committed() {
    let dir = tempfile::tempdir().unwrap();
    let db = file_db(&dir);
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let (open, done) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
    let writer = {
        let (db, open, done, you, p) = (db.clone(), open.clone(), done.clone(), s.you_id.clone(), p.clone());
        std::thread::spawn(move || db.write(Some(&you), |w| {
            w.conn().execute("UPDATE projects SET name = 'Renamed' WHERE id = ?1", [&p])?;
            open.wait(); // the write transaction is open, its change not committed
            done.wait(); // until the read below has had its chance
            Ok(())
        }).unwrap())
    };
    open.wait();
    let t0 = Instant::now();
    let (db2, p2) = (db.clone(), p.clone());
    let (read, reader) = within(Duration::from_secs(3), move || name_of(&db2, &p2));
    let took = t0.elapsed();
    done.wait();
    writer.join().unwrap();
    reader.join().unwrap();
    assert_eq!(read.as_deref(), Some("Kade portal"), "the read finished while the write was open ({took:?}), and saw no uncommitted change");
    assert!(took < Duration::from_secs(1), "{took:?}");
    assert_eq!(name_of(&db, &p), "Renamed", "the write committed afterwards");
}

#[test]
fn a_read_finishes_while_another_thread_holds_a_long_read_and_a_write_goes_on_beside_every_reader() {
    let dir = tempfile::tempdir().unwrap();
    let db = file_db(&dir);
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    // one long read (a board check), and the window's read beside it
    let (open, done) = (Arc::new(Barrier::new(2)), Arc::new(Barrier::new(2)));
    let long = {
        let (db, open, done) = (db.clone(), open.clone(), done.clone());
        std::thread::spawn(move || db.read(|c| {
            let n: i64 = c.query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))?;
            open.wait();
            done.wait();
            Ok(n)
        }).unwrap())
    };
    open.wait();
    let t0 = Instant::now();
    let (db2, p2) = (db.clone(), p.clone());
    let (read, reader) = within(Duration::from_secs(3), move || name_of(&db2, &p2));
    let took = t0.elapsed();
    done.wait();
    long.join().unwrap();
    reader.join().unwrap();
    assert_eq!(read.as_deref(), Some("Kade portal"), "the read finished beside the long one ({took:?})");
    assert!(took < Duration::from_secs(1), "{took:?}");

    // every reader busy with a long read: a write still goes, and a read that began before it doesn't see it halfway
    let open = Arc::new(Barrier::new(db::READERS + 1));
    let done = Arc::new(Barrier::new(db::READERS + 1));
    let longs: Vec<_> = (0..db::READERS).map(|_| {
        let (db, open, done, p) = (db.clone(), open.clone(), done.clone(), p.clone());
        std::thread::spawn(move || db.read(|c| {
            let before: String = c.query_row("SELECT name FROM projects WHERE id = ?1", [&p], |r| r.get(0))?;
            open.wait();
            done.wait();
            let after: String = c.query_row("SELECT name FROM projects WHERE id = ?1", [&p], |r| r.get(0))?;
            Ok((before, after))
        }).unwrap())
    }).collect();
    open.wait();
    let (db2, you, p2) = (db.clone(), s.you_id.clone(), p.clone());
    let (wrote, writer) = within(Duration::from_secs(3), move || db2.write(Some(&you), |w| {
        w.conn().execute("UPDATE projects SET name = 'Renamed' WHERE id = ?1", [&p2])?;
        Ok(())
    }).is_ok());
    done.wait();
    writer.join().unwrap();
    assert_eq!(wrote, Some(true), "the write committed while {} long reads were open", db::READERS);
    for l in longs {
        assert_eq!(l.join().unwrap(), ("Kade portal".to_string(), "Kade portal".to_string()), "one read is one snapshot");
    }
    assert_eq!(name_of(&db, &p), "Renamed");
}

#[test]
fn reads_on_a_database_file_are_read_only_and_an_in_memory_database_reads_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let db = file_db(&dir);
    seed::ensure_seed(&db, "Jeffrey").unwrap();
    let e = db.read(|c| Ok(c.execute("UPDATE projects SET name = 'x'", [])?)).unwrap_err().to_string();
    assert!(e.contains("readonly") || e.contains("read-only") || e.contains("read only"), "{e}");
    assert_eq!(db.read(|c| Ok(c.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i64>(0))?)).unwrap(), 1, "as the writer has it");
    let mem = Db::open_in_memory().unwrap();
    seed::ensure_seed(&mem, "Jeffrey").unwrap();
    assert!(mem.read(|c| Ok(c.query_row("SELECT count(*) FROM actors", [], |r| r.get::<_, i64>(0))?)).unwrap() > 0);
}

// ---- lighter lists ----

#[test]
fn the_lean_lists_leave_out_acceptance_and_instructions_and_everything_else_stays_the_same() {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = team::get(&db, &s.team_id).unwrap();
    let label = t.labels.iter().find(|l| l.name == "backend").unwrap().id.clone();
    let card = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: "Export invoices".into(), description_md: "As CSV.".into(),
        acceptance_md: Some("- [ ] A header row".into()), label_ids: vec![label], ..Default::default() }).unwrap();
    tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: "Import clients".into(), ..Default::default() }).unwrap();
    let agent = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(),
        instructions_md: Some("Write the test first.".into()), ..Default::default() }).unwrap();

    for filter in [TaskFilter::default(), TaskFilter { project_id: Some(p.clone()), open_only: true }] {
        let lean = tasks::list_lean(&db, &filter).unwrap();
        let full = tasks::list(&db, &filter).unwrap();
        assert_eq!(lean.len(), 2);
        assert!(lean.iter().all(|t| t.acceptance_md.is_none()), "list_tasks: acceptanceMd null");
        assert_eq!(full.iter().find(|t| t.id == card).unwrap().acceptance_md.as_deref(), Some("- [ ] A header row"), "tasks::list keeps it");
        let without = |ts: &[Task]| ts.iter().map(|t| { let mut v = serde_json::to_value(t).unwrap(); v["acceptanceMd"] = serde_json::Value::Null; v })
            .collect::<Vec<_>>();
        assert_eq!(without(&lean), without(&full), "the rest of each card is the same");
    }
    assert_eq!(tasks::get(&db, &card).unwrap().acceptance_md.as_deref(), Some("- [ ] A header row"), "get_task still fills it");

    let lean = team::get_lean(&db, &s.team_id).unwrap();
    let full = team::get(&db, &s.team_id).unwrap();
    assert!(lean.members.iter().all(|m| m.instructions_md.is_none()), "get_team: instructionsMd null");
    assert_eq!(full.members.iter().find(|m| m.actor_id == agent).unwrap().instructions_md.as_deref(), Some("Write the test first."), "team::get keeps them");
    assert_eq!(team::agent(&db, &agent).unwrap().instructions_md.as_deref(), Some("Write the test first."), "get_agent still fills them");
    let mut a = serde_json::to_value(&lean).unwrap();
    let mut b = serde_json::to_value(&full).unwrap();
    for m in a["members"].as_array_mut().unwrap().iter_mut().chain(b["members"].as_array_mut().unwrap().iter_mut()) {
        m["instructionsMd"] = serde_json::Value::Null;
    }
    assert_eq!(a, b, "the rest of the team is the same");
}

// ---- the rewritten queries: their plans ----

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Each of `literals` is a string literal in `src`, white space aside: the plans below are of the code's own queries.
fn in_source(file: &str, src: &str, literals: &[&str]) {
    let src = squash(src);
    for l in literals {
        assert!(src.contains(&squash(l)), "{file} no longer has this query; copy the new one into ga191_core_test.rs:\n{l}");
    }
}

/// The steps of `sql`'s plan (EXPLAIN QUERY PLAN), every parameter NULL.
fn plan(db: &Db, sql: &str) -> Vec<String> {
    db.read(|c| {
        let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
        let nulls = vec![rusqlite::types::Null; st.parameter_count()];
        let steps = st.query_map(rusqlite::params_from_iter(nulls), |r| r.get::<_, String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(steps)
    }).unwrap()
}

/// The plan reads `alias` through an index or its primary key: `SEARCH alias USING …`.
fn searches(plan: &[String], alias: &str) -> bool {
    plan.iter().any(|s| s.starts_with(&format!("SEARCH {alias} USING ")))
}

/// The steps that read one of `aliases` row by row.
fn scans<'a>(plan: &'a [String], aliases: &[&str]) -> Vec<&'a String> {
    plan.iter().filter(|s| aliases.iter().any(|a| s.as_str() == format!("SCAN {a}") || s.starts_with(&format!("SCAN {a} ")))).collect()
}

const LABELS: &str = "SELECT tl.task_id, l.id, l.name, l.color FROM json_each(?1) j
         JOIN task_labels tl ON tl.task_id = j.value AND tl.deleted_at IS NULL
         JOIN labels l ON l.id = tl.label_id ORDER BY l.name";
const CHAT_CARDS: &str = "SELECT t.id, t.identifier FROM json_each(?1) j JOIN tasks t ON t.id = j.value AND t.deleted_at IS NULL";
const NOT_LEAD: &str = "(?2 IS NULL OR ch.actor_id IS NULL OR ch.actor_id <> ?2)";
const STAMP_OWN: &str = "SELECT ch.seq, ch.hlc FROM changes ch WHERE ch.row_id = ?1 AND {NOT_LEAD} ORDER BY ch.seq DESC LIMIT 1";
const STAMP_COMMENTS: &str = "SELECT ch.seq, ch.hlc FROM comments m JOIN changes ch ON ch.row_id = m.id WHERE m.task_id = ?1 AND {NOT_LEAD}
                 ORDER BY ch.seq DESC LIMIT 1";
const STAMP_RUNS: &str = "SELECT ch.seq, ch.hlc FROM runs r JOIN changes ch ON ch.row_id = r.id WHERE r.task_id = ?1 AND {NOT_LEAD}
                 ORDER BY ch.seq DESC LIMIT 1";
const PENDING_FILTER: &str = "id IN (SELECT thread_id FROM pending_actions WHERE status = 'pending')";
const WAITING_FILTER: &str = "kind IS NOT NULL AND answered_at IS NULL AND dismissed_at IS NULL";
const THREADS_WHERE: &str = "FROM chat_threads WHERE deleted_at IS NULL AND ({filter}) ORDER BY updated_at DESC, rowid DESC LIMIT ?1";
const INBOX_WHERE: &str = "WHERE t.deleted_at IS NULL AND t.state_category NOT IN ('done','cancelled')
               AND ((t.hold IS NOT NULL AND COALESCE({WITH_LEAD}, 0) = 0) OR (s.category IN ('review','deploy') AND t.assignee_actor_id = ?1))
             ORDER BY s.sort_key, t.sort_key";

/// The old queries, as main had them before GA-191: the rows to compare with.
const OLD_LABELS: &str = "SELECT tl.task_id, l.id, l.name, l.color FROM task_labels tl JOIN labels l ON l.id = tl.label_id
         WHERE tl.deleted_at IS NULL ORDER BY l.name";
const OLD_STAMP: &str = "SELECT ch.seq, ch.hlc FROM changes ch
         WHERE (ch.row_id = ?1 OR ch.row_id IN (SELECT id FROM comments WHERE task_id = ?1) OR ch.row_id IN (SELECT id FROM runs WHERE task_id = ?1))
           AND (?2 IS NULL OR ch.actor_id IS NULL OR ch.actor_id <> ?2)
         ORDER BY ch.seq DESC LIMIT 1";

#[test]
fn each_rewritten_query_reads_through_an_index() {
    in_source("tasks.rs (labels_for)", TASKS_RS, &[LABELS]);
    in_source("chat.rs (with_tasks)", CHAT_RS, &[CHAT_CARDS]);
    in_source("chat.rs (threads_where)", CHAT_RS, &[PENDING_FILTER, WAITING_FILTER, THREADS_WHERE]);
    in_source("board.rs (stamp)", BOARD_RS, &[NOT_LEAD, STAMP_OWN, STAMP_COMMENTS, STAMP_RUNS]);
    in_source("tasks.rs (needs_you)", TASKS_RS, &[INBOX_WHERE]);
    let db = Db::open_in_memory().unwrap();

    let p = plan(&db, LABELS);
    assert!(searches(&p, "tl") && searches(&p, "l"), "a list's labels: by card, then by label: {p:?}");
    assert!(scans(&p, &["tl", "l"]).is_empty(), "{p:?}");
    let p = plan(&db, CHAT_CARDS);
    assert!(searches(&p, "t") && scans(&p, &["t"]).is_empty(), "the chats' cards by id: {p:?}");
    for (what, sql, table) in [("its own changes", STAMP_OWN, "ch"), ("its comments' changes", STAMP_COMMENTS, "m"), ("its runs' changes", STAMP_RUNS, "r")] {
        let p = plan(&db, &sql.replace("{NOT_LEAD}", NOT_LEAD));
        assert!(p.iter().any(|s| s.starts_with(&format!("SEARCH {table} USING ")) && s.contains("INDEX")), "a card's last change, {what}: {p:?}");
        assert!(p.iter().any(|s| s.starts_with("SEARCH ch USING ") && s.contains("changes_row")), "{what}: changes by changes_row: {p:?}");
        assert!(scans(&p, &["ch", "m", "r", "changes", "comments", "runs"]).is_empty(), "{what}: {p:?}");
    }
    // the chats with calls that wait for approval: the pending calls through their index, the chats by id
    let p = plan(&db, &format!("SELECT id {}", THREADS_WHERE.replace("{filter}", PENDING_FILTER)));
    assert!(p.iter().any(|s| s.contains("pending_actions USING COVERING INDEX pending_actions_thread")), "the calls from their index only: {p:?}");
    assert!(searches(&p, "chat_threads"), "the chats by id: {p:?}");
    println!("plans: chats_with_pending {p:?}");
    println!("plans: waiting_lead_chats {:?}", plan(&db, &format!("SELECT id {}", THREADS_WHERE.replace("{filter}", WAITING_FILTER))));
    let with_lead = "CASE WHEN t.hold = 'needs_decision' THEN (SELECT json_extract(r.outcome_json, '$.lead.state') IN ('asking','answering') FROM runs r
      WHERE r.task_id = t.id AND r.deleted_at IS NULL AND json_extract(r.outcome_json, '$.lead') IS NOT NULL
      ORDER BY r.created_at DESC, r.id DESC LIMIT 1) END";
    in_source("tasks.rs (WITH_LEAD)", TASKS_RS, &[with_lead]);
    let inbox = format!("SELECT t.id FROM tasks t JOIN workflow_states s ON s.id = t.state_id {}", INBOX_WHERE.replace("{WITH_LEAD}", with_lead));
    let p = plan(&db, &inbox);
    assert!(p.iter().any(|s| s.contains("USING INDEX runs_task")), "the Inbox: a held card's latest run through runs_task: {p:?}");
    println!("plans: needs_you {p:?}");
}

// ---- the rewritten queries: the same rows as before ----

struct W { db: Db, you: String, project: String, team: String, lead: String, be: String }

fn work() -> W {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let agent = |name: &str, role: &str| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), chat_enabled: Some(role == "lead"), ..Default::default() }).unwrap();
    let be = agent("Backend Agent", "backend");
    let lead = agent("Team Lead", "lead");
    gizai_core::questions::set_enabled(&db, false).unwrap();
    W { db, you: s.you_id, project, team: s.team_id, lead, be }
}

impl W {
    fn state(&self, name: &str) -> String {
        team::get(&self.db, &self.team).unwrap().states.into_iter().find(|s| s.name == name).unwrap().id
    }
    fn label(&self, name: &str) -> String {
        team::get(&self.db, &self.team).unwrap().labels.into_iter().find(|l| l.name == name).unwrap().id
    }
    fn card(&self, column: &str, labels: &[&str]) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: self.project.clone(), title: format!("A card in {column}"),
            state_id: Some(self.state(column)), label_ids: labels.iter().map(|l| self.label(l)).collect(), ..Default::default() }).unwrap()
    }
    fn hold(&self, task: &str, hold: &str) {
        tasks::update(&self.db, &self.you, task, TaskPatch { hold: Some(hold.into()), hold_reason: Some("Why".into()), ..Default::default() }).unwrap();
    }
}

fn tick() {
    std::thread::sleep(Duration::from_millis(3));
}

#[test]
fn a_lists_labels_are_the_listed_cards_own_as_before() {
    let w = work();
    let a = w.card("To do", &["backend", "frontend"]);
    let b = w.card("Backlog", &["qa"]);
    let c = w.card("To do", &[]);
    let archived = w.card("Done", &["backend"]);
    tasks::archive(&w.db, &w.you, &archived).unwrap();
    // a label taken off a card
    let frontend = w.label("frontend");
    w.db.write(Some(&w.you), |x| { x.conn().execute("UPDATE task_labels SET deleted_at = 1 WHERE task_id = ?1 AND label_id = ?2", [&a, &frontend])?; Ok(()) }).unwrap();

    let old: Vec<(String, Label)> = w.db.read(|c| {
        let mut st = c.prepare(OLD_LABELS)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, Label { id: r.get(1)?, name: r.get(2)?, color: r.get(3)? })))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }).unwrap();
    let mut expected: HashMap<String, Vec<String>> = HashMap::new();
    for (t, l) in &old {
        expected.entry(t.clone()).or_default().push(l.name.clone());
    }
    for list in [tasks::list(&w.db, &TaskFilter::default()).unwrap(), tasks::list_lean(&w.db, &TaskFilter::default()).unwrap()] {
        assert_eq!(list.len(), 3, "the archived card isn't listed");
        for t in &list {
            let names: Vec<String> = t.labels.iter().map(|l| l.name.clone()).collect();
            assert_eq!(names, expected.get(&t.id).cloned().unwrap_or_default(), "{}", t.identifier);
            assert_eq!(names, tasks::get(&w.db, &t.id).unwrap().labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>());
        }
    }
    assert_eq!(tasks::get(&w.db, &a).unwrap().labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["backend"]);
    assert_eq!(tasks::get(&w.db, &b).unwrap().labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["qa"]);
    assert!(tasks::get(&w.db, &c).unwrap().labels.is_empty());
    assert_eq!(tasks::archived(&w.db, None).unwrap()[0].labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["backend"]);
}

#[test]
fn the_chat_lists_card_numbers_and_the_inboxs_chats_are_the_ones_the_old_lookups_gave() {
    let w = work();
    let a = w.card("To do", &[]);
    let b = w.card("To do", &[]);
    let gone = w.card("To do", &[]);
    let ident = |t: &str| tasks::get(&w.db, t).unwrap().identifier;
    // a question about two cards, one archived since (its number drops out, as before); an approval about one card
    let (q, _) = chat::start_lead_chat(&w.db, &w.lead, "Which format?", "question", &[b.clone(), gone.clone()], "CSV or JSON?", None).unwrap();
    tick();
    let (ap, _) = chat::start_lead_chat(&w.db, &w.lead, "Merge?", "approval", &[a.clone()], "May I merge?", None).unwrap();
    tick();
    let (answered, _) = chat::start_lead_chat(&w.db, &w.lead, "Answered", "question", &[w.card("To do", &[])], "Ok?", None).unwrap();
    chat::add_message(&w.db, NewMessage { thread_id: answered.clone(), role: "user".into(), author_id: Some(w.you.clone()),
        body_md: Some("Yes".into()), ..Default::default() }).unwrap();
    tick();
    let (dismissed, _) = chat::start_lead_chat(&w.db, &w.lead, "Dismissed", "question", &[w.card("To do", &[])], "Ok?", None).unwrap();
    chat::dismiss(&w.db, &w.you, &dismissed).unwrap();
    tick();
    let own = chat::create_thread(&w.db, &w.you, &w.lead, "Plan the export").unwrap();
    tasks::move_to(&w.db, &w.you, &gone, &w.state("Done"), "").unwrap();
    tasks::archive(&w.db, &w.you, &gone).unwrap();
    // a call of the Team Lead's that waits for your approval in your own chat, and one decided already in the question
    let keep = |thread: &str| pending::keep(&w.db, NewPending { thread_id: thread.into(), run_id: None, actor_id: w.lead.clone(), tool: "memory_write".into(),
        args: serde_json::json!({"path": "Notes", "content": "x"}), outside_tool: String::new(), note_key: Some("notes".into()) }).unwrap();
    keep(&own);
    let decided = keep(&q);
    pending::discard(&w.db, &w.you, &decided.id, "No").unwrap();

    let all = chat::list_threads(&w.db).unwrap();
    let by_id = |id: &str| all.iter().find(|t| t.id == id).unwrap().clone();
    // the old lookups: one card at a time, a deleted card left out
    for t in &all {
        let old: Vec<String> = t.task_ids.iter().filter_map(|id| w.db.read(|c| Ok(c.query_row(
            "SELECT identifier FROM tasks WHERE id=?1 AND deleted_at IS NULL", [id], |r| r.get::<_, String>(0)).ok())).unwrap()).collect();
        assert_eq!(t.tasks, old, "chat {}", t.title);
    }
    assert_eq!(by_id(&q).tasks, [ident(&b)]);
    assert_eq!(by_id(&ap).tasks, [ident(&a)]);

    let ids = |v: Vec<chat::ChatThread>| v.into_iter().map(|t| t.id).collect::<Vec<_>>();
    let old_waiting = ids(all.iter().filter(|t| t.waiting).cloned().collect());
    let old_pending = ids(all.iter().filter(|t| t.pending_actions > 0).cloned().collect());
    assert_eq!(ids(chat::waiting_lead_chats(&w.db).unwrap()), old_waiting);
    assert_eq!(old_waiting, [ap.clone(), q.clone()], "newest first; the answered and the dismissed ones don't wait");
    assert_eq!(ids(chat::chats_with_pending(&w.db).unwrap()), old_pending);
    assert_eq!(old_pending, [own.clone()]);
    let full = |v: Vec<chat::ChatThread>| v.into_iter().map(|t| serde_json::to_value(t).unwrap()).collect::<Vec<_>>();
    assert_eq!(full(chat::waiting_lead_chats(&w.db).unwrap()), full(all.iter().filter(|t| t.waiting).cloned().collect()), "whole rows, the same");
    assert_eq!(full(chat::chats_with_pending(&w.db).unwrap()), full(all.iter().filter(|t| t.pending_actions > 0).cloned().collect()));
}

#[test]
fn the_inbox_has_the_cards_the_old_filter_kept_in_the_same_order() {
    let w = work();
    let held = w.card("To do", &["backend"]);
    w.hold(&held, "blocked");
    let review_mine = w.card("Review", &[]);
    tasks::update(&w.db, &w.you, &review_mine, TaskPatch { assignee_id: Some(w.you.clone()), ..Default::default() }).unwrap();
    let review_agents = w.card("Review", &[]);
    tasks::update(&w.db, &w.you, &review_agents, TaskPatch { assignee_id: Some(w.be.clone()), ..Default::default() }).unwrap();
    let deploy_mine = w.card("Deploy", &[]);
    tasks::update(&w.db, &w.you, &deploy_mine, TaskPatch { assignee_id: Some(w.you.clone()), ..Default::default() }).unwrap();
    w.card("To do", &[]);
    let done_held = w.card("Done", &[]);
    w.hold(&done_held, "blocked");
    // a question that is with the Team Lead (GA-70): not yours yet
    let with_lead = w.card("To do", &[]);
    w.hold(&with_lead, "needs_decision");
    let r = runs::create(&w.db, &w.be, &with_lead, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
    w.db.write(None, |x| { x.conn().execute("UPDATE runs SET outcome_json = '{\"lead\":{\"state\":\"asking\"}}' WHERE id = ?1", [&r])?; Ok(()) }).unwrap();
    // one whose Team Lead answered already: yours again
    let lead_done = w.card("To do", &[]);
    w.hold(&lead_done, "needs_decision");
    let r = runs::create(&w.db, &w.be, &lead_done, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r2.jsonl").unwrap();
    w.db.write(None, |x| { x.conn().execute("UPDATE runs SET outcome_json = '{\"lead\":{\"state\":\"answered\"}}' WHERE id = ?1", [&r])?; Ok(()) }).unwrap();

    let old: Vec<Task> = tasks::list(&w.db, &TaskFilter { open_only: true, ..Default::default() }).unwrap().into_iter()
        .filter(|t| (t.hold.is_some() && !t.with_lead) || (matches!(t.state_category.as_str(), "review" | "deploy") && t.assignee_id.as_deref() == Some(w.you.as_str())))
        .collect();
    let new = tasks::needs_you(&w.db, &w.you).unwrap();
    let json = |v: &[Task]| v.iter().map(|t| serde_json::to_value(t).unwrap()).collect::<Vec<_>>();
    assert_eq!(json(&new), json(&old), "the same cards, whole, in the same order");
    let mut got: Vec<&str> = new.iter().map(|t| t.id.as_str()).collect();
    got.sort();
    let mut want = vec![held.as_str(), review_mine.as_str(), deploy_mine.as_str(), lead_done.as_str()];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn a_cards_last_change_is_the_one_the_old_query_found() {
    let w = work();
    let old = |task: &str| -> i64 {
        w.db.read(|c| Ok(c.query_row(OLD_STAMP, rusqlite::params![task, w.lead], |r| r.get::<_, i64>(0)).unwrap_or(0))).unwrap()
    };
    let column = w.state("To do");
    gizai_core::columns::remove_agent(&w.db, &w.you, &column, &w.be).ok();
    // only its own changes
    let own = w.card("To do", &[]);
    w.hold(&own, "blocked");
    tick();
    // a comment of yours after them
    let commented = w.card("To do", &[]);
    w.hold(&commented, "blocked");
    tick();
    comments::add(&w.db, &w.you, &commented, "Use the template.", None).unwrap();
    tick();
    // a run after them, then the Team Lead's comment and its change to the card, which don't count
    let ran = w.card("To do", &[]);
    w.hold(&ran, "blocked");
    tick();
    let r = runs::create(&w.db, &w.be, &ran, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
    runs::finish(&w.db, &r, "failed", None, 0, 0, 0, Some("exit 1")).unwrap();
    tick();
    comments::add(&w.db, &w.lead, &ran, "Looking at it.", None).unwrap();
    tasks::update(&w.db, &w.lead, &ran, TaskPatch { priority: Some(2), ..Default::default() }).unwrap();

    let findings = board::check(&w.db, &Context { now: ids::now_ms(), max_concurrent: 3, lead_id: Some(w.lead.clone()), ..Default::default() }).unwrap();
    for card in [&own, &commented, &ran] {
        let f = findings.iter().find(|f| &f.task_id == card).unwrap_or_else(|| panic!("no finding for {card}: {findings:?}"));
        assert!(f.stamp > 0);
        assert_eq!(f.stamp, old(card), "{}", f.task);
    }
    let seq_of = |row: &str| -> i64 { w.db.read(|c| Ok(c.query_row("SELECT max(seq) FROM changes WHERE row_id = ?1", [row], |r| r.get(0))?)).unwrap() };
    assert_eq!(findings.iter().find(|f| f.task_id == ran).unwrap().stamp, seq_of(&r), "the run's change, not the Team Lead's later ones");
}

// ---- PRAGMA optimize ----

fn stats(db: &Db) -> Vec<(String, String)> {
    db.read(|c| {
        let exists: i64 = c.query_row("SELECT count(*) FROM sqlite_master WHERE name = 'sqlite_stat1'", [], |r| r.get(0))?;
        if exists == 0 {
            return Ok(vec![]);
        }
        let mut st = c.prepare("SELECT tbl, stat FROM sqlite_stat1 WHERE tbl = 'tasks' ORDER BY idx")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }).unwrap()
}

fn add_cards(db: &Db, you: &str, project: &str, n: usize) {
    db.write(Some(you), |w| {
        for _ in 0..n {
            let id = ids::new_id();
            w.conn().execute("INSERT INTO tasks (id, created_at, updated_at, org_id, project_id, identifier, title, state_id, state_category, sort_key)
                SELECT ?1, 1, 1, org_id, ?2, 'X-' || ?1, 'Card', state_id, state_category, 'a' FROM tasks LIMIT 1",
                rusqlite::params![id, project])?;
        }
        Ok(())
    }).unwrap();
}

#[test]
fn pragma_optimize_runs_when_the_database_opens_and_in_the_housekeeping() {
    in_source("db.rs (Db::open)", DB_RS, &["PRAGMA optimize=0x10002"]);
    in_source("housekeeping.rs (run)", HOUSEKEEPING_RS, &["db.optimize()"]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let db = Db::open(&path).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: "First".into(), ..Default::default() }).unwrap();
    add_cards(&db, &s.you_id, &p, 400);
    assert!(stats(&db).is_empty(), "no statistics before a PRAGMA optimize: {:?}", stats(&db));
    drop(db);

    // opened again: the open gathered the statistics of the cards
    let db = Db::open(&path).unwrap();
    let at_open = stats(&db);
    assert!(!at_open.is_empty(), "PRAGMA optimize at open analysed tasks");
    assert!(at_open.iter().any(|(_, stat)| stat.starts_with("401")), "{at_open:?}");

    // far more cards since: the housekeeping gathers them again
    add_cards(&db, &s.you_id, &p, 12_000);
    // the writer looks cards up by their column and project, as a write does (reads go to the read-only connections)
    db.write(None, |w| { w.conn().query_row("SELECT count(*) FROM tasks WHERE project_id = ?1 AND deleted_at IS NULL AND state_category = 'ready'", [&p], |r| r.get::<_, i64>(0))?; Ok(()) }).unwrap();
    assert_eq!(stats(&db), at_open, "nothing changes the statistics but an optimize");
    housekeeping::run(&db, dir.path(), ids::now_ms()).unwrap();
    let after = stats(&db);
    assert_ne!(after, at_open, "PRAGMA optimize in the housekeeping analysed tasks again: {after:?}");
}
