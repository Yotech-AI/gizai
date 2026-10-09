//! GA-58 QA (release v0.3.0): a v0.2.0 database with chats, upgraded in one step from schema 9 to 12, as the update
//! does it for someone who used the Team Lead chat on v0.2.0. GA-48's 0010 (Refused in this run), GA-49's 0011 (column
//! agents) and GA-50's 0012 (a chat's Runs on and its queue) run together here. ga49_migration_test covers the board on
//! the same path; this covers the chats and the runs. The database is built by running migrations 0001..0009 only (not
//! by rolling 0010..0012 back), filled with v0.2.0-style rows using raw SQL, then opened with `Db::open` like the app does.
use gizai_core::chat;
use gizai_core::db::{self, Db};
use gizai_core::model::Refusal;
use gizai_core::runs;
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use std::path::{Path, PathBuf};

fn v9_migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("../migrations/0001_init.sql")),
        M::up(include_str!("../migrations/0002_agents.sql")),
        M::up(include_str!("../migrations/0003_chat.sql")),
        M::up(include_str!("../migrations/0004_effort.sql")),
        M::up(include_str!("../migrations/0005_pull_requests.sql")),
        M::up(include_str!("../migrations/0006_worktree_prepare.sql")),
        M::up(include_str!("../migrations/0007_card_flow.sql")),
        M::up(include_str!("../migrations/0008_board_check.sql")),
        M::up(include_str!("../migrations/0009_agent_folders.sql")),
    ])
}

/// v0.2.0 chats.
/// - 'plans': your chat with the Team Lead. Its first session S1 ran on claude_code; the current one, S2, had a turn on
///   claude_code and then a later one on acct-2. A card run (no chat) that also says S2 and is newer still must not count.
/// - 'fresh': a chat whose first answer never came (no session).
/// - 'ask': a question the Team Lead started during a board check, with a session but no chat turn in it.
const DATA: &str = r#"
INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
  ('you',  1, 1, 'org', 'person', 'Tjitske',      'tjitske', 'active'),
  ('lead', 1, 1, 'org', 'agent',  'Master Chief', 'chief',   'active');
INSERT INTO agent_configs (actor_id, created_at, updated_at, adapter, wakeup, chat_enabled, board_check_minutes) VALUES
  ('lead', 1, 1, 'claude_code', 'heartbeat', 1, 30);

INSERT INTO chat_threads (id, created_at, updated_at, created_by, org_id, agent_actor_id, title, session_id, cost_usd_micros, input_tokens, output_tokens) VALUES
  ('plans', 10, 40, 'you', 'org', 'lead', 'Plans for the week', 'S2', 52000, 1200, 340),
  ('fresh', 50, 50, 'you', 'org', 'lead', 'No answer yet', NULL, 0, 0, 0);
INSERT INTO chat_threads (id, created_at, updated_at, created_by, org_id, agent_actor_id, title, session_id, kind, task_ids_json) VALUES
  ('ask', 60, 60, 'lead', 'org', 'lead', 'Which branch?', 'S9', 'question', '[]');

INSERT INTO runs (id, created_at, updated_at, org_id, agent_actor_id, chat_thread_id, trigger, adapter, status, session_id, log_path, cost_usd_micros) VALUES
  ('r1', 11, 11, 'org', 'lead', 'plans', 'chat', 'claude_code', 'succeeded', 'S1', '/data/runs/r1.jsonl', 20000),
  ('r2', 20, 20, 'org', 'lead', 'plans', 'chat', 'claude_code', 'succeeded', 'S2', '/data/runs/r2.jsonl', 12000),
  ('r3', 30, 30, 'org', 'lead', 'plans', 'chat', 'acct-2',      'succeeded', 'S2', '/data/runs/r3.jsonl', 20000),
  ('r9', 59, 59, 'org', 'lead', NULL,    'board_check', 'claude_code', 'succeeded', 'S9', '/data/runs/r9.jsonl', 3000),
  ('rx', 70, 70, 'org', 'lead', NULL,    'manual', 'gemini', 'failed', 'S2', '/data/runs/rx.jsonl', 0);

INSERT INTO chat_messages (id, created_at, thread_id, role, author_actor_id, body_md, run_id, tool_name, tool_json) VALUES
  ('m1', 10, 'plans', 'user',   'you',  'What is on this week?', NULL, NULL, NULL),
  ('m2', 11, 'plans', 'agent',  'lead', 'Three cards.',          'r1', NULL, NULL),
  ('m3', 12, 'plans', 'tool',   'lead', NULL,                    'r1', 'list_tasks', '{"id":"tu1","input":{}}'),
  ('m4', 25, 'plans', 'system', NULL,   'Started a new session.', NULL, NULL, NULL),
  ('m5', 30, 'plans', 'agent',  'lead', 'Done.',                 'r3', NULL, NULL),
  ('m6', 50, 'fresh', 'user',   'you',  'Hello?',                NULL, NULL, NULL),
  ('m7', 60, 'ask',   'agent',  'lead', 'Main or production?',   'r9', NULL, NULL);
"#;

struct Fixture {
    _dir: tempfile::TempDir,
    path: PathBuf,
    backups: PathBuf,
}

/// A genuine schema 9 database file (what v0.2.0 left on disk), filled with DATA.
fn v9_db() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let mut c = Connection::open(&path).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    v9_migrations().to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch(DATA).unwrap();
    c.execute("INSERT INTO devices(id, created_at, name, is_self) VALUES ('dev', 1, 'test', 1)", []).unwrap();
    assert_eq!(version(&c), 9, "the fixture is schema 9");
    assert_eq!(broken_keys(&c), 0, "the fixture's rows are consistent");
    drop(c);
    let backups = dir.path().join("backups");
    Fixture { _dir: dir, path, backups }
}

fn version(c: &Connection) -> i64 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

fn broken_keys(c: &Connection) -> i64 {
    c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap()
}

fn backup_names(dir: &Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => vec![],
    }
}

#[test]
fn a_v020_database_with_chats_upgrades_to_schema_12_in_one_step() {
    let f = v9_db();
    let db = Db::open(&f.path).unwrap();
    let (v, broken) = db.read(|c| Ok((version(c), broken_keys(c)))).unwrap();
    assert_eq!((v, broken), (db::SCHEMA_VERSION, 0), "schema 12, foreign keys intact");
    assert_eq!(db::SCHEMA_VERSION, 12, "v0.3.0 ships schema 12; a new migration needs its own release check");

    let names = backup_names(&f.backups);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].starts_with("gizai-before-v12-") && names[0].ends_with(".db"), "{names:?}");
    let old = Connection::open(f.backups.join(&names[0])).unwrap();
    let (old_v, old_threads, old_messages): (i64, i64, i64) = (version(&old),
        old.query_row("SELECT count(*) FROM chat_threads", [], |r| r.get(0)).unwrap(),
        old.query_row("SELECT count(*) FROM chat_messages", [], |r| r.get(0)).unwrap());
    assert_eq!((old_v, old_threads, old_messages), (9, 3, 7), "the backup is the v0.2.0 database as it was");
}

#[test]
fn every_chat_and_message_is_kept_and_a_session_gets_the_cli_of_its_last_chat_turn() {
    let f = v9_db();
    let db = Db::open(&f.path).unwrap();

    let plans = chat::get_thread(&db, "plans").unwrap();
    assert_eq!((plans.title.as_str(), plans.session_id.as_deref(), plans.created_by.as_deref()), ("Plans for the week", Some("S2"), Some("you")));
    assert_eq!((plans.cost_usd_micros, plans.input_tokens, plans.output_tokens), (52000, 1200, 340), "the session's totals are kept");
    assert_eq!(plans.cli, None, "no Runs on of its own: it follows the Team Lead's");
    assert_eq!(plans.session_cli.as_deref(), Some("acct-2"), "S2's last chat turn ran on acct-2 (not S1's turn, not the card run)");

    let fresh = chat::get_thread(&db, "fresh").unwrap();
    assert_eq!((fresh.session_id, fresh.session_cli, fresh.cli), (None, None, None));

    let ask = chat::get_thread(&db, "ask").unwrap();
    assert_eq!((ask.kind.as_deref(), ask.session_id.as_deref(), ask.session_cli), (Some("question"), Some("S9"), None),
               "a session with no chat turn in it has no CLI yet");

    let mut ids: Vec<String> = chat::list_threads(&db).unwrap().into_iter().map(|t| t.id).collect();
    ids.sort();
    assert_eq!(ids, ["ask", "fresh", "plans"]);

    let msgs = chat::messages(&db, "plans").unwrap();
    let got: Vec<(&str, &str, Option<&str>, Option<&str>)> = msgs.iter()
        .map(|m| (m.id.as_str(), m.role.as_str(), m.body_md.as_deref(), m.run_id.as_deref())).collect();
    assert_eq!(got, [("m1", "user", Some("What is on this week?"), None), ("m2", "agent", Some("Three cards."), Some("r1")),
                     ("m3", "tool", None, Some("r1")), ("m4", "system", Some("Started a new session."), None),
                     ("m5", "agent", Some("Done."), Some("r3"))]);
    assert_eq!(msgs[2].tool_name.as_deref(), Some("list_tasks"));
    assert!(msgs.iter().all(|m| m.meta.is_none()), "old notes carry nothing for the Chat page");
    assert_eq!(chat::messages(&db, "fresh").unwrap().len(), 1);
    assert_eq!(chat::messages(&db, "ask").unwrap().len(), 1);

    for t in ["plans", "fresh", "ask"] {
        assert!(chat::queue(&db, t).unwrap().is_empty(), "{t}: no queued messages");
    }
}

#[test]
fn old_runs_show_nothing_refused_and_the_new_chat_features_work_on_old_chats() {
    let f = v9_db();
    let db = Db::open(&f.path).unwrap();

    for r in ["r1", "r2", "r3", "r9", "rx"] {
        assert!(runs::get(&db, r).unwrap().refused.is_empty(), "{r}: a v0.2.0 run refused nothing");
    }
    let refused = vec![Refusal { tool: "Bash".into(), input: "git push".into(), reason: "needs approval".into() }];
    runs::set_refused(&db, "r3", &refused).unwrap();
    let r3 = runs::get(&db, "r3").unwrap();
    assert_eq!((r3.refused.len(), r3.refused[0].tool.as_str(), r3.adapter.as_deref()), (1, "Bash", Some("acct-2")));

    let q = chat::enqueue(&db, "you", "plans", "  And next week?  ").unwrap();
    assert_eq!((q.body_md.as_str(), q.held), ("And next week?", false));
    assert_eq!(chat::queue(&db, "plans").unwrap().len(), 1);

    let picked = chat::set_cli(&db, "you", "plans", Some("claude_code")).unwrap();
    assert_eq!((picked.cli.as_deref(), picked.session_cli.as_deref()), (Some("claude_code"), Some("acct-2")),
               "picking Runs on leaves the session's own CLI alone");
    assert_eq!(chat::set_cli(&db, "you", "plans", None).unwrap().cli, None, "back to the Team Lead's");
}

#[test]
fn opening_the_upgraded_database_again_changes_nothing() {
    let f = v9_db();
    drop(Db::open(&f.path).unwrap());
    let snapshot = |db: &Db| db.read(|c| {
        let mut st = c.prepare("SELECT id, IFNULL(session_cli, '-'), IFNULL(cli, '-') FROM chat_threads ORDER BY id")?;
        let threads = st.query_map([], |r| Ok(format!("{}:{}:{}", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let counts: (i64, i64, i64, i64) = c.query_row(
            "SELECT (SELECT count(*) FROM chat_messages), (SELECT count(*) FROM runs), (SELECT count(*) FROM run_refusals), (SELECT count(*) FROM chat_queue)",
            [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok((threads, counts))
    }).unwrap();
    let first = snapshot(&Db::open(&f.path).unwrap());
    assert_eq!(first.0, ["ask:-:-", "fresh:-:-", "plans:acct-2:-"]);
    assert_eq!(first.1, (7, 5, 0, 0));
    let second = snapshot(&Db::open(&f.path).unwrap());
    assert_eq!(first, second);
    assert_eq!(backup_names(&f.backups).len(), 1, "no second backup: the database is current");
}
