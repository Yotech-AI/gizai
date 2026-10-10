//! GA-49 QA: migration 0011 (column agents; GA-48's 0010 runs before it) on a genuine schema 9 database, as a v0.2.0 user has it.
//! The database is built by running migrations 0001..0009 only (not by rolling 0010 and 0011 back), filled with
//! v0.2.0-style rows using raw SQL, then opened with `Db::open` like the app does.
use gizai_core::db::{self, Db};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use std::collections::{BTreeMap, BTreeSet};
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

/// v0.2.0 data. Team A "Software": the full 0.2.0 board (with an extra In progress column and two removed columns),
/// label and column routing rules. Team B "Studio": no Deploy column, Testing owned by 'design', an agent that is also
/// in team A, a member removed from the team. Team C "Quiet": only a manual builder and a paused QA agent.
const DATA: &str = r#"
INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');

INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
  ('you',         1, 1, 'org', 'person', 'Tjitske',     'tjitske',     'active'),
  ('lead',        1, 1, 'org', 'agent',  'Master Chief','chief',       'active'),
  ('backend',     1, 1, 'org', 'agent',  'Backend',     'backend',     'active'),
  ('frontend',    1, 1, 'org', 'agent',  'Frontend',    'frontend',    'active'),
  ('design',      1, 1, 'org', 'agent',  'Design',      'design',      'active'),
  ('qa',          1, 1, 'org', 'agent',  'QA',          'qa',          'active'),
  ('devops',      1, 1, 'org', 'agent',  'DevOps',      'devops',      'active'),
  ('devops2',     1, 1, 'org', 'agent',  'Release bot', 'release',     'active'),
  ('backend2',    1, 1, 'org', 'agent',  'Backend 2',   'backend2',    'paused'),
  ('archived',    1, 1, 'org', 'agent',  'Old frontend','oldfront',    'archived'),
  ('docs',        1, 1, 'org', 'agent',  'Docs',        'docs',        'active'),
  ('sec',         1, 1, 'org', 'agent',  'Security',    'sec',         'active'),
  ('illustrator', 1, 1, 'org', 'agent',  'Illustrator', 'illustrator', 'active'),
  ('slowpoke',    1, 1, 'org', 'agent',  'Slowpoke',    'slowpoke',    'active'),
  ('qa_paused',   1, 1, 'org', 'agent',  'QA paused',   'qapaused',    'paused');
INSERT INTO actors (id, created_at, updated_at, deleted_at, org_id, kind, name, handle, status) VALUES
  ('gone',        1, 1, 70, 'org', 'agent',  'Removed agent','gone',     'active');

INSERT INTO agent_configs (actor_id, created_at, updated_at, adapter, wakeup) VALUES
  ('lead', 1, 1, 'claude_code', 'heartbeat'),
  ('backend', 1, 1, 'claude_code', 'on_assign'),
  ('frontend', 1, 1, 'claude_code', 'heartbeat'),
  ('design', 1, 1, 'claude_code', 'manual'),
  ('qa', 1, 1, 'claude_code', 'on_assign'),
  ('devops', 1, 1, 'claude_code', 'manual'),
  ('devops2', 1, 1, 'claude_code', 'on_assign'),
  ('backend2', 1, 1, 'claude_code', 'on_assign'),
  ('archived', 1, 1, 'claude_code', 'on_assign'),
  ('docs', 1, 1, 'claude_code', 'on_assign'),
  ('sec', 1, 1, 'claude_code', 'on_assign'),
  ('illustrator', 1, 1, 'claude_code', 'on_assign'),
  ('slowpoke', 1, 1, 'claude_code', 'manual'),
  ('qa_paused', 1, 1, 'claude_code', 'on_assign'),
  ('gone', 1, 1, 'claude_code', 'on_assign');

INSERT INTO teams (id, created_at, updated_at, org_id, name, lead_actor_id) VALUES
  ('team-a', 1, 1, 'org', 'Software', 'lead'),
  ('team-b', 2, 2, 'org', 'Studio', NULL),
  ('team-c', 3, 3, 'org', 'Quiet', NULL);

INSERT INTO team_members (team_id, actor_id, role_key, is_lead, created_at, deleted_at) VALUES
  ('team-a', 'you',       'reviewer', 0, 101, NULL),
  ('team-a', 'lead',      'lead',     1, 102, NULL),
  ('team-a', 'backend',   'backend',  0, 103, NULL),
  ('team-a', 'frontend',  'frontend', 0, 104, NULL),
  ('team-a', 'design',    'design',   0, 105, NULL),
  ('team-a', 'qa',        'qa',       0, 106, NULL),
  ('team-a', 'devops',    'devops',   0, 107, NULL),
  ('team-a', 'devops2',   'devops',   0, 108, NULL),
  ('team-a', 'backend2',  'backend',  0, 109, NULL),
  ('team-a', 'archived',  'frontend', 0, 110, NULL),
  ('team-a', 'docs',      'docs',     0, 111, NULL),
  ('team-a', 'sec',       'security', 0, 112, NULL),
  ('team-a', 'gone',      'backend',  0, 113, NULL),
  ('team-b', 'you',       'reviewer', 0, 201, NULL),
  ('team-b', 'backend',   'frontend', 0, 202, NULL),
  ('team-b', 'frontend',  'frontend', 0, 203, 250),
  ('team-b', 'illustrator','design',  0, 204, NULL),
  ('team-c', 'slowpoke',  'backend',  0, 301, NULL),
  ('team-c', 'qa_paused', 'qa',       0, 302, NULL);

INSERT INTO workflow_states (id, created_at, updated_at, deleted_at, team_id, name, category, owner_role, sort_key) VALUES
  ('a-backlog', 1, 1, NULL, 'team-a', 'Backlog',     'backlog',     NULL,          'a0'),
  ('a-todo',    1, 1, NULL, 'team-a', 'To do',       'ready',       'implementer', 'a1'),
  ('a-oldprog', 1, 1, 50,   'team-a', 'Old work',    'in_progress', 'implementer', 'a15'),
  ('a-prog',    1, 1, NULL, 'team-a', 'In progress', 'in_progress', 'implementer', 'a2'),
  ('a-prog2',   1, 1, NULL, 'team-a', 'Pairing',     'in_progress', 'implementer', 'a3'),
  ('a-oldqa',   1, 1, 50,   'team-a', 'Old QA',      'testing',     'qa',          'a35'),
  ('a-test',    1, 1, NULL, 'team-a', 'Testing',     'testing',     'qa',          'a4'),
  ('a-review',  1, 1, NULL, 'team-a', 'Review',      'review',      'human',       'a5'),
  ('a-deploy',  1, 1, NULL, 'team-a', 'Deploy',      'deploy',      NULL,          'a6'),
  ('a-done',    1, 1, NULL, 'team-a', 'Done',        'done',        NULL,          'a7'),
  ('a-cancel',  1, 1, NULL, 'team-a', 'Cancelled',   'cancelled',   NULL,          'a8'),
  ('b-backlog', 1, 1, NULL, 'team-b', 'Backlog',     'backlog',     NULL,          'a0'),
  ('b-todo',    1, 1, NULL, 'team-b', 'To do',       'ready',       'implementer', 'a1'),
  ('b-prog',    1, 1, NULL, 'team-b', 'In progress', 'in_progress', 'implementer', 'a2'),
  ('b-test',    1, 1, NULL, 'team-b', 'Design check','testing',     'design',      'a3'),
  ('b-review',  1, 1, NULL, 'team-b', 'Review',      'review',      'human',       'a4'),
  ('b-done',    1, 1, NULL, 'team-b', 'Done',        'done',        NULL,          'a5'),
  ('c-todo',    1, 1, NULL, 'team-c', 'To do',       'ready',       'implementer', 'a1'),
  ('c-prog',    1, 1, NULL, 'team-c', 'In progress', 'in_progress', 'implementer', 'a2'),
  ('c-test',    1, 1, NULL, 'team-c', 'Testing',     'testing',     'qa',          'a3'),
  ('c-review',  1, 1, NULL, 'team-c', 'Review',      'review',      'human',       'a4'),
  ('c-done',    1, 1, NULL, 'team-c', 'Done',        'done',        NULL,          'a5');

INSERT INTO labels (id, created_at, updated_at, org_id, name) VALUES
  ('l-docs', 1, 1, 'org', 'docs'), ('l-hotfix', 1, 1, 'org', 'hotfix'), ('l-infra', 1, 1, 'org', 'infra'), ('l-wip', 1, 1, 'org', 'wip');

INSERT INTO routing_rules (id, created_at, updated_at, deleted_at, team_id, kind, match_label_id, match_state_id, target_role, priority, enabled) VALUES
  ('r-docs',     1, 1, NULL, 'team-a', 'label',  'l-docs',   NULL,       'docs',     100, 1),
  ('r-hotfix',   1, 1, NULL, 'team-a', 'label',  'l-hotfix', NULL,       'devops',   100, 1),
  ('r-infra',    1, 1, NULL, 'team-a', 'label',  'l-infra',  NULL,       'devops',   90,  1),
  ('r-wip-off',  1, 1, NULL, 'team-a', 'label',  'l-wip',    NULL,       'lead',     100, 0),
  ('r-prog-qa',  1, 1, NULL, 'team-a', 'column', NULL,       'a-prog',   'qa',       100, 1),
  ('r-test-sec', 1, 1, NULL, 'team-a', 'column', NULL,       'a-test',   'security', 100, 1),
  ('r-test-qa',  1, 1, NULL, 'team-a', 'column', NULL,       'a-test',   'qa',       50,  1),
  ('r-oldqa',    1, 1, NULL, 'team-a', 'column', NULL,       'a-oldqa',  'security', 100, 1),
  ('r-review',   1, 1, NULL, 'team-a', 'column', NULL,       'a-review', 'frontend', 100, 1),
  ('r-deploy',   1, 1, NULL, 'team-a', 'column', NULL,       'a-deploy', 'frontend', 100, 1),
  ('r-todo-del', 1, 1, 60,   'team-a', 'column', NULL,       'a-todo',   'lead',     100, 1);

INSERT INTO tasks (id, created_at, updated_at, org_id, identifier, title, state_id, state_category, sort_key, assignee_actor_id, implementer_actor_id) VALUES
  ('t1',  1, 1, 'org', 'YT-1',  'In the backlog',   'a-backlog', 'backlog',     'a0', NULL,      NULL),
  ('t2',  2, 2, 'org', 'YT-2',  'Waiting in To do', 'a-todo',    'ready',       'a0', NULL,      NULL),
  ('t3',  3, 3, 'org', 'YT-3',  'Being built',      'a-prog',    'in_progress', 'a0', 'backend', 'backend'),
  ('t4',  4, 4, 'org', 'YT-4',  'Being tested',     'a-test',    'testing',     'a0', NULL,      'backend'),
  ('t5',  5, 5, 'org', 'YT-5',  'Your review',      'a-review',  'review',      'a0', 'you',     'backend'),
  ('t6',  6, 6, 'org', 'YT-6',  'To ship',          'a-deploy',  'deploy',      'a0', NULL,      'frontend'),
  ('t7',  7, 7, 'org', 'YT-7',  'Shipped',          'a-done',    'done',        'a0', NULL,      'frontend'),
  ('t8',  8, 8, 'org', 'YT-8',  'In a removed col', 'a-oldqa',   'testing',     'a0', NULL,      NULL),
  ('t9',  9, 9, 'org', 'YT-9',  'Studio card',      'b-todo',    'ready',       'a0', NULL,      NULL),
  ('t10', 10, 10, 'org', 'YT-10', 'Quiet card',     'c-todo',    'ready',       'a0', NULL,      NULL);
UPDATE orgs SET next_task_number = 11;

INSERT INTO task_labels (task_id, label_id, created_at) VALUES ('t2', 'l-docs', 1), ('t3', 'l-hotfix', 1);

INSERT INTO runs (id, created_at, updated_at, org_id, agent_actor_id, task_id, trigger, role_key, outcome, adapter, status, log_path, started_at, ended_at) VALUES
  ('run-1', 20, 20, 'org', 'backend', 't3', 'routed', 'backend', 'ready_for_testing', 'claude_code', 'succeeded', 'runs/run-1.jsonl', 20, 30),
  ('run-2', 40, 40, 'org', 'qa',      't4', 'routed', 'qa',      'qa_pass',           'claude_code', 'succeeded', 'runs/run-2.jsonl', 40, 50);

INSERT INTO comments (id, created_at, updated_at, task_id, author_actor_id, body_md, run_id) VALUES
  ('c1', 31, 31, 't3', 'backend', 'Built it.', 'run-1'),
  ('c2', 51, 51, 't4', 'qa',      'All criteria pass.', 'run-2'),
  ('c3', 60, 60, 't5', 'you',     'Looks good.', NULL);
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
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 9, "the fixture is schema 9");
    let fk: i64 = c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0)).unwrap();
    assert_eq!(fk, 0, "the fixture's rows are consistent");
    drop(c);
    let backups = dir.path().join("backups");
    Fixture { _dir: dir, path, backups }
}

fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

fn ids(c: &Connection, table: &str) -> Vec<String> {
    let mut st = c.prepare(&format!("SELECT id FROM {table} ORDER BY id")).unwrap();
    st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
}

fn has_table(c: &Connection, name: &str) -> bool {
    c.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1", [name], |r| r.get::<_, i64>(0)).unwrap() > 0
}

fn agents_on(c: &Connection, state: &str) -> BTreeSet<String> {
    let mut st = c.prepare("SELECT actor_id FROM column_agents WHERE state_id=?1").unwrap();
    st.query_map([state], |r| r.get(0)).unwrap().collect::<rusqlite::Result<BTreeSet<_>>>().unwrap()
}

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// (auto, next_state_id) per column id.
fn columns(c: &Connection) -> BTreeMap<String, (bool, Option<String>)> {
    let mut st = c.prepare("SELECT id, auto, next_state_id FROM workflow_states").unwrap();
    st.query_map([], |r| Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)? != 0, r.get::<_, Option<String>>(2)?))))
        .unwrap().collect::<rusqlite::Result<BTreeMap<_, _>>>().unwrap()
}

fn backup_names(f: &Fixture) -> Vec<String> {
    match std::fs::read_dir(&f.backups) {
        Ok(rd) => rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect(),
        Err(_) => vec![],
    }
}

/// Opens the v9 fixture like the app does and closes it again.
fn migrated() -> Fixture {
    let f = v9_db();
    drop(Db::open(&f.path).unwrap());
    f
}

#[test]
fn every_column_card_run_agent_and_comment_is_kept() {
    let f = v9_db();
    let tables = ["workflow_states", "tasks", "runs", "actors", "comments", "labels", "teams", "agent_configs"];
    let c = raw(&f.path);
    let before: Vec<Vec<String>> = tables.iter().map(|t| {
        if *t == "agent_configs" { let mut st = c.prepare("SELECT actor_id FROM agent_configs ORDER BY actor_id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap() } else { ids(&c, t) }
    }).collect();
    let members_before: i64 = c.query_row("SELECT count(*) FROM team_members", [], |r| r.get(0)).unwrap();
    let task_cols_before: Vec<(String, String)> = c.prepare("SELECT id, state_id FROM tasks ORDER BY id").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    let removed_before: Vec<String> = c.prepare("SELECT id FROM workflow_states WHERE deleted_at IS NOT NULL ORDER BY id").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    drop(c);

    drop(Db::open(&f.path).unwrap());

    let c = raw(&f.path);
    for (t, b) in tables.iter().zip(&before) {
        let after = if *t == "agent_configs" { let mut st = c.prepare("SELECT actor_id FROM agent_configs ORDER BY actor_id").unwrap();
            st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap() } else { ids(&c, t) };
        assert_eq!(&after, b, "{t} keeps every row");
    }
    assert_eq!(c.query_row("SELECT count(*) FROM team_members", [], |r| r.get::<_, i64>(0)).unwrap(), members_before);
    let task_cols_after: Vec<(String, String)> = c.prepare("SELECT id, state_id FROM tasks ORDER BY id").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(task_cols_after, task_cols_before, "every card stays in its column");
    let removed_after: Vec<String> = c.prepare("SELECT id FROM workflow_states WHERE deleted_at IS NOT NULL ORDER BY id").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    assert_eq!(removed_after, removed_before, "removed columns stay removed (and kept)");
    assert_eq!(c.query_row("SELECT count(*) FROM task_labels", [], |r| r.get::<_, i64>(0)).unwrap(), 2, "labels stay on cards");
    // the new links point at real columns and agents
    for t in ["workflow_states", "column_agents"] {
        let n: i64 = c.query_row(&format!("SELECT count(*) FROM pragma_foreign_key_check('{t}')"), [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "foreign keys of {t} hold");
    }
}

#[test]
fn next_columns_follow_020_moves() {
    let f = migrated();
    let cols = columns(&raw(&f.path));
    let next = |id: &str| cols[id].1.clone();
    // Team A (with Deploy, an extra In progress column and removed columns)
    assert_eq!(next("a-todo").as_deref(), Some("a-prog"), "To do → the first (not removed) In progress column");
    assert_eq!(next("a-prog").as_deref(), Some("a-test"), "In progress → the first (not removed) Testing column");
    assert_eq!(next("a-prog2").as_deref(), Some("a-test"), "every In progress column → the first Testing column");
    assert_eq!(next("a-test").as_deref(), Some("a-review"), "Testing → the first Review column");
    assert_eq!(next("a-review").as_deref(), Some("a-deploy"), "Review → the first Deploy column");
    assert_eq!(next("a-deploy").as_deref(), Some("a-done"), "Deploy → the first Done column");
    for none in ["a-backlog", "a-done", "a-cancel", "a-oldprog", "a-oldqa"] {
        assert_eq!(next(none), None, "{none} gets no next column");
    }
    // Team B has no Deploy column: Review → Done
    assert_eq!(next("b-review").as_deref(), Some("b-done"), "Review without Deploy → the first Done column");
    assert_eq!(next("b-todo").as_deref(), Some("b-prog"));
    assert_eq!(next("b-prog").as_deref(), Some("b-test"));
    assert_eq!(next("b-test").as_deref(), Some("b-review"));
    assert_eq!(next("b-backlog"), None);
    assert_eq!(next("b-done"), None);
    // next columns never cross teams
    let c = raw(&f.path);
    let crossing: i64 = c.query_row("SELECT count(*) FROM workflow_states s JOIN workflow_states n ON n.id = s.next_state_id
                                     WHERE n.team_id <> s.team_id", [], |r| r.get(0)).unwrap();
    assert_eq!(crossing, 0, "a next column is in the same team");
}

#[test]
fn builders_and_routed_roles_go_on_to_do_and_in_progress() {
    let f = migrated();
    let c = raw(&f.path);
    // builders (not lead/qa/devops), paused included, archived not, manual-wakeup 'design' not (these columns are Auto);
    // + devops2 (label rules hotfix/infra → devops; devops itself is manual so stays off).
    let builders = ["backend", "frontend", "backend2", "docs", "sec", "devops2"];
    assert_eq!(agents_on(&c, "a-todo"), set(&builders), "To do");
    assert_eq!(agents_on(&c, "a-prog2"), set(&builders), "Pairing (second In progress column)");
    // + qa by the column rule In progress → qa
    let mut prog = builders.to_vec();
    prog.push("qa");
    assert_eq!(agents_on(&c, "a-prog"), set(&prog), "In progress (column rule → qa)");
    // the lead never: not by role, not by a disabled label rule, not by a removed column rule
    for s in ["a-todo", "a-prog", "a-prog2", "a-test", "a-deploy"] {
        assert!(!agents_on(&c, s).contains("lead"), "the Team Lead isn't on {s}");
        assert!(!agents_on(&c, s).contains("archived"), "an archived agent isn't on {s}");
        assert!(!agents_on(&c, s).contains("gone"), "a removed (deleted_at) agent isn't on {s}");
        assert!(!agents_on(&c, s).contains("you"), "a person isn't on {s}");
    }
}

#[test]
fn testing_gets_qa_and_column_rule_roles_deploy_gets_devops() {
    let f = migrated();
    let c = raw(&f.path);
    assert_eq!(agents_on(&c, "a-test"), set(&["qa", "sec"]), "Testing: QA (owner role) + security (column rule)");
    assert_eq!(agents_on(&c, "a-deploy"), set(&["devops", "devops2"]), "Deploy: every DevOps agent (Deploy is Manual, so manual ones too); a column rule on Deploy adds nobody");
    for none in ["a-backlog", "a-review", "a-done", "a-cancel", "a-oldprog", "a-oldqa"] {
        assert_eq!(agents_on(&c, none), set(&[]), "{none} gets no agents (incl. a column rule on Review / a removed column)");
    }
    // no duplicate (state, actor) rows can exist: the primary key holds; also no rows for another team's columns
    let foreign: i64 = c.query_row(
        "SELECT count(*) FROM column_agents ca JOIN workflow_states s ON s.id = ca.state_id
          WHERE NOT EXISTS (SELECT 1 FROM team_members m WHERE m.team_id = s.team_id AND m.actor_id = ca.actor_id AND m.deleted_at IS NULL)",
        [], |r| r.get(0)).unwrap();
    assert_eq!(foreign, 0, "every agent on a column is a (current) member of that column's team");
}

#[test]
fn second_team_owner_role_two_teams_and_removed_member() {
    let f = migrated();
    let c = raw(&f.path);
    // 'backend' is in team A (backend) and team B (frontend); 'frontend' left team B (deleted_at)
    assert_eq!(agents_on(&c, "b-todo"), set(&["backend", "illustrator"]), "Studio To do");
    assert_eq!(agents_on(&c, "b-prog"), set(&["backend", "illustrator"]), "Studio In progress");
    assert_eq!(agents_on(&c, "b-test"), set(&["illustrator"]), "Testing owned by 'design' gets the design agent");
    assert_eq!(agents_on(&c, "b-review"), set(&[]));
    let cols = columns(&c);
    assert!(cols["b-todo"].0 && cols["b-prog"].0 && cols["b-test"].0, "Studio To do, In progress and Testing are Auto: {cols:?}");
}

#[test]
fn manual_wakeup_agents_stay_off_auto_columns_paused_agents_are_placed() {
    let f = migrated();
    let c = raw(&f.path);
    let cols = columns(&c);
    // Team A: design (manual) is a builder but To do / In progress are Auto
    for s in ["a-todo", "a-prog", "a-prog2", "a-test"] {
        assert!(cols[s].0, "{s} is Auto");
        assert!(!agents_on(&c, s).contains("design"), "manual 'design' isn't on Auto {s}");
        assert!(!agents_on(&c, s).contains("devops"), "manual 'devops' isn't on Auto {s}");
    }
    assert!(agents_on(&c, "a-todo").contains("backend2"), "paused backend2 is placed like an active one");
    // Team C: the only builder is manual → To do and In progress stay Manual and keep it (it starts on Run)
    assert_eq!(agents_on(&c, "c-todo"), set(&["slowpoke"]), "manual builder on a Manual To do");
    assert_eq!(agents_on(&c, "c-prog"), set(&["slowpoke"]), "manual builder on a Manual In progress");
    assert!(!cols["c-todo"].0, "To do with only a manual agent is Manual");
    assert!(!cols["c-prog"].0, "In progress with only a manual agent is Manual");
    // a paused QA agent on on_assign still makes Testing Auto, like an active one
    assert_eq!(agents_on(&c, "c-test"), set(&["qa_paused"]), "paused QA on Testing");
    assert!(cols["c-test"].0, "Testing with a paused on_assign QA is Auto");
}

#[test]
fn auto_only_on_to_do_in_progress_and_testing_with_a_next_column() {
    let f = migrated();
    let c = raw(&f.path);
    let cols = columns(&c);
    for (id, (auto, next)) in &cols {
        if *auto {
            assert!(next.is_some(), "Auto column {id} has a next column");
        }
    }
    for manual in ["a-backlog", "a-review", "a-deploy", "a-done", "a-cancel", "a-oldprog", "a-oldqa", "b-backlog", "b-review", "b-done",
                   "c-review", "c-done"] {
        assert!(!cols[manual].0, "{manual} is Manual");
    }
    for auto in ["a-todo", "a-prog", "a-prog2", "a-test", "b-todo", "b-prog", "b-test", "c-test"] {
        assert!(cols[auto].0, "{auto} is Auto");
    }
    // an Auto column holds no agent on manual wake-up
    let bad: i64 = c.query_row(
        "SELECT count(*) FROM column_agents ca JOIN workflow_states s ON s.id = ca.state_id JOIN agent_configs g ON g.actor_id = ca.actor_id
          WHERE s.auto = 1 AND g.wakeup = 'manual'", [], |r| r.get(0)).unwrap();
    assert_eq!(bad, 0, "no manual-wakeup agent on an Auto column");
}

#[test]
fn routing_rules_are_gone_and_schema_is_current() {
    let f = migrated();
    let c = raw(&f.path);
    assert!(!has_table(&c, "routing_rules"), "routing_rules is dropped");
    assert!(has_table(&c, "column_agents"));
    assert!(!has_table(&c, "ga49_workers"), "the temp helper table is gone");
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, db::SCHEMA_VERSION);
    assert_eq!(v, 14, "0012 here, then GA-59's 0013 (Bitbucket links) and GA-19's 0014 (memory)");
    let branches: i64 = c.query_row("SELECT count(*) FROM teams WHERE branches_json IS NOT NULL", [], |r| r.get(0)).unwrap();
    assert_eq!(branches, 0, "teams keep the default branches (NULL)");
}

#[test]
fn a_before_v14_backup_is_made_and_still_opens_as_schema_9() {
    let f = migrated();
    let snaps = backup_names(&f);
    assert_eq!(snaps.len(), 1, "{snaps:?}");
    assert!(snaps[0].starts_with("gizai-before-v14-") && snaps[0].ends_with(".db"), "{snaps:?}");
    let b = raw(&f.backups.join(&snaps[0]));
    let v: i64 = b.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 9, "the backup is the schema 9 database");
    assert!(has_table(&b, "routing_rules"));
    assert!(!has_table(&b, "column_agents"));
    let rules: i64 = b.query_row("SELECT count(*) FROM routing_rules", [], |r| r.get(0)).unwrap();
    assert_eq!(rules, 11, "the backup keeps every routing rule");
    let tasks: i64 = b.query_row("SELECT count(*) FROM tasks", [], |r| r.get(0)).unwrap();
    assert_eq!(tasks, 10);
    let has_auto: i64 = b.query_row("SELECT count(*) FROM pragma_table_info('workflow_states') WHERE name='auto'", [], |r| r.get(0)).unwrap();
    assert_eq!(has_auto, 0, "the backup's columns have no auto field");
}

#[test]
fn opening_the_migrated_database_again_changes_nothing() {
    let f = migrated();
    let snapshot = |c: &Connection| {
        let cols = columns(c);
        let mut st = c.prepare("SELECT state_id, actor_id, sort_key, created_at FROM column_agents ORDER BY state_id, actor_id").unwrap();
        let agents = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        let counts: Vec<i64> = ["tasks", "runs", "comments", "actors", "workflow_states", "team_members", "changes"].iter()
            .map(|t| c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0)).unwrap()).collect();
        (cols, agents, counts)
    };
    let first = snapshot(&raw(&f.path));
    std::thread::sleep(std::time::Duration::from_millis(5));
    drop(Db::open(&f.path).unwrap());
    let second = snapshot(&raw(&f.path));
    assert_eq!(first, second, "a second open changes nothing");
    assert_eq!(backup_names(&f).len(), 1, "and makes no new backup");
    let v: i64 = raw(&f.path).query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 14);
}

#[test]
fn the_board_works_after_the_migration() {
    let f = v9_db();
    let db = Db::open(&f.path).unwrap();
    let team = gizai_core::team::get(&db, "team-a").unwrap();
    let names: Vec<&str> = team.states.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Backlog", "To do", "In progress", "Pairing", "Testing", "Review", "Deploy", "Done", "Cancelled"],
               "team::get lists the (not removed) columns in board order");
    let todo = team.states.iter().find(|s| s.id == "a-todo").unwrap();
    assert!(todo.auto);
    assert_eq!(todo.next_state_id.as_deref(), Some("a-prog"));
    assert_eq!(todo.agent_ids.iter().cloned().collect::<BTreeSet<_>>(),
               set(&["backend", "frontend", "backend2", "docs", "sec", "devops2"]));
    // the agents are in the order they joined the team
    assert_eq!(todo.agent_ids, ["backend", "frontend", "devops2", "backend2", "docs", "sec"], "joined order");
    let waiting = gizai_core::workflow::waiting_for(&db, "backend").unwrap();
    assert!(waiting.contains(&"t2".to_string()), "backend is offered the To do card: {waiting:?}");
    // the To do card in team B is offered to backend too (it is on that team's To do as well)
    assert!(waiting.contains(&"t9".to_string()), "backend is offered team B's To do card: {waiting:?}");
    // the manual builder of team C is on its To do, but the column is Manual: nothing starts it by itself
    let quiet = gizai_core::workflow::waiting_for(&db, "slowpoke").unwrap();
    assert!(quiet.is_empty(), "a manual agent waits for Run: {quiet:?}");
    // an archived agent is offered nothing
    assert!(gizai_core::workflow::waiting_for(&db, "archived").unwrap().is_empty());
    // the QA agent is offered the Testing card
    let qa = gizai_core::workflow::waiting_for(&db, "qa").unwrap();
    assert!(qa.contains(&"t4".to_string()), "qa is offered the Testing card: {qa:?}");
    let b = gizai_core::team::get(&db, "team-b").unwrap();
    assert_eq!(b.states.len(), 6);
}

#[test]
fn an_empty_schema_9_database_upgrades() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let mut c = Connection::open(&path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    v9_migrations().to_latest(&mut c).unwrap();
    drop(c);
    drop(Db::open(&path).unwrap());
    let c = raw(&path);
    assert_eq!(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), db::SCHEMA_VERSION);
    assert!(!has_table(&c, "routing_rules"));
    assert_eq!(c.query_row("SELECT count(*) FROM column_agents", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
}
