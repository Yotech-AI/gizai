//! GA-180: migration 0018's indexes. A card's comments and activity, a chat's messages and runs, and the release of a
//! run's claim read through an index now, not the whole table: EXPLAIN QUERY PLAN on the code's own SQL shows a SEARCH,
//! no SCAN. So do the board check's, the Usage page's and the project list's reads that the indexes are for. A database
//! the previous version left gets the eight indexes after a backup, every row kept, and opening it again changes nothing.
//! And the reads give what they gave: comments oldest first, a chat's messages in the order written, a run's end frees
//! its own card.
use gizai_core::chat::{self, NewMessage};
use gizai_core::db::{self, Db};
use gizai_core::model::*;
use gizai_core::{comments, projects, runs, seed, tasks, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use std::path::Path;

const COMMENTS_RS: &str = include_str!("../../src/comments.rs");
const TASKS_RS: &str = include_str!("../../src/tasks.rs");
const CHAT_RS: &str = include_str!("../../src/chat.rs");
const FILES_RS: &str = include_str!("../../src/files.rs");
const RUNS_RS: &str = include_str!("../../src/runs.rs");
const BOARD_RS: &str = include_str!("../../src/board.rs");
const PULLS_RS: &str = include_str!("../../src/pulls.rs");
const USAGE_RS: &str = include_str!("../../src/usage.rs");
const PROJECTS_RS: &str = include_str!("../../src/projects.rs");

/// 0018's indexes: name, table, columns, and the rows it keeps (its WHERE) when not all.
const INDEXES: [(&str, &str, &[&str], Option<&str>); 8] = [
    ("comments_task", "comments", &["task_id", "created_at", "id"], None),
    ("chat_messages_thread", "chat_messages", &["thread_id"], None),
    ("runs_chat", "runs", &["chat_thread_id", "created_at"], Some("chat_thread_id IS NOT NULL")),
    ("tasks_claimed", "tasks", &["claimed_by_run_id"], Some("claimed_by_run_id IS NOT NULL")),
    ("runs_active", "runs", &["status"], Some("status IN ('queued','running','waiting_approval')")),
    ("runs_agent_created", "runs", &["agent_actor_id", "created_at"], None),
    ("runs_created", "runs", &["created_at"], None),
    ("tasks_project", "tasks", &["project_id", "deleted_at", "state_category"], None),
];

// ---- the code's own queries, copied (`in_source` checks that the code still has them) ----

/// comments::list: a card's comments, on its page and the last ones in a run's prompt.
const COMMENTS_LIST: &str = "SELECT m.id, m.author_actor_id, a.name, a.kind, m.body_md, m.run_id, m.created_at
             FROM comments m JOIN actors a ON a.id = m.author_actor_id
             WHERE m.task_id = ?1 AND m.deleted_at IS NULL ORDER BY m.created_at, m.id";

/// tasks::activity: a card's activity, its own changes and its comments'.
const ACTIVITY: &str = "SELECT ch.hlc, a.name, ch.table_name, ch.op, ch.diff_json FROM changes ch LEFT JOIN actors a ON a.id = ch.actor_id
             WHERE ch.row_id = ?1 OR ch.row_id IN (SELECT id FROM comments WHERE task_id = ?1) ORDER BY ch.seq";

/// chat::messages: MSG_SELECT, then the chat's own.
const MSG_SELECT: &str = "SELECT m.id, m.thread_id, m.role, m.author_actor_id, a.name, m.body_md, m.run_id, m.tool_name, m.tool_json, m.created_at, m.meta_json
    FROM chat_messages m LEFT JOIN actors a ON a.id = m.author_actor_id";
const MESSAGES: &str = "{MSG_SELECT} WHERE m.thread_id=?1 AND m.deleted_at IS NULL ORDER BY m.rowid";

/// chat::messages: the files of a chat's messages, in one look (`{}`: files::ROW_COLS).
const MESSAGE_FILES: &str = "SELECT owner_id, {} FROM files WHERE owner_type = ?1 AND deleted_at IS NULL
               AND owner_id IN (SELECT id FROM chat_messages WHERE thread_id = ?2) ORDER BY created_at, rowid";
const FILE_COLS: &str = "id, name, mime, size_bytes, sha256, created_at";

/// runs::release_claim: at every run's end, and for the runs a restart finds.
const RELEASE_CLAIM: &str = "UPDATE tasks SET claimed_by_run_id=NULL, lease_expires_at=NULL WHERE claimed_by_run_id=?1";

/// A chat's runs (its answers) in order. No query reads them yet (the app follows a chat's answer in memory, and a
/// message carries its run's id); this is the read runs_chat is ready for.
const CHAT_RUNS: &str = "SELECT id FROM runs WHERE chat_thread_id = ?1 ORDER BY created_at";

/// White space as one space, as SQLite reads a query.
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Each of `literals` is a string literal in `src`, white space aside: the plans here are of the code's own queries, so a
/// query that changes fails here until its copy follows it.
fn in_source(file: &str, src: &str, literals: &[&str]) {
    let src = squash(src);
    for l in literals {
        assert!(src.contains(&format!("\"{}\"", squash(l))), "{file} no longer has this query; copy the new one into indexes_test.rs:\n{l}");
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

/// The plan reads `alias` through `index`: `SEARCH alias USING [COVERING ]INDEX index (…)`.
fn searches(plan: &[String], alias: &str, index: &str) -> bool {
    plan.iter().any(|s| s.starts_with(&format!("SEARCH {alias} USING ")) && s.contains(&format!("INDEX {index} (")))
}

/// The steps that read one of `aliases` row by row: `SCAN alias`, through an index or not.
fn scans<'a>(plan: &'a [String], aliases: &[&str]) -> Vec<&'a String> {
    plan.iter().filter(|s| aliases.iter().any(|a| {
        let scan = format!("SCAN {a}");
        s.as_str() == scan || s.starts_with(&format!("{scan} "))
    })).collect()
}

fn no_scan(plan: &[String]) -> bool {
    !plan.iter().any(|s| s.starts_with("SCAN "))
}

fn sorts(plan: &[String]) -> bool {
    plan.iter().any(|s| s.contains("TEMP B-TREE"))
}

// ---- the plans the card asks for ----

#[test]
fn a_cards_comments_search_comments_task_and_come_in_their_order_with_no_sort() {
    in_source("comments.rs (comments::list)", COMMENTS_RS, &[COMMENTS_LIST]);
    let p = plan(&Db::open_in_memory().unwrap(), COMMENTS_LIST);
    assert!(searches(&p, "m", "comments_task"), "{p:?}");
    assert!(no_scan(&p) && !sorts(&p), "no scan, and the index gives the order (created_at, id): {p:?}");
}

#[test]
fn a_cards_activity_finds_its_comments_by_comments_task() {
    in_source("tasks.rs (tasks::activity)", TASKS_RS, &[ACTIVITY]);
    let p = plan(&Db::open_in_memory().unwrap(), ACTIVITY);
    assert!(searches(&p, "comments", "comments_task") && searches(&p, "ch", "changes_row"), "{p:?}");
    assert!(no_scan(&p), "{p:?}");
}

#[test]
fn a_chats_messages_and_their_files_search_chat_messages_thread_in_the_order_written() {
    in_source("chat.rs (chat::messages)", CHAT_RS, &[MSG_SELECT, MESSAGES, MESSAGE_FILES]);
    in_source("files.rs (files::ROW_COLS)", FILES_RS, &[FILE_COLS]);
    let db = Db::open_in_memory().unwrap();
    let p = plan(&db, &MESSAGES.replace("{MSG_SELECT}", MSG_SELECT));
    assert!(searches(&p, "m", "chat_messages_thread"), "{p:?}");
    assert!(no_scan(&p) && !sorts(&p), "no scan, and the index keeps rowid order: {p:?}");
    let p = plan(&db, &MESSAGE_FILES.replace("{}", FILE_COLS));
    assert!(searches(&p, "chat_messages", "chat_messages_thread") && searches(&p, "files", "files_owner"), "{p:?}");
    assert!(no_scan(&p), "{p:?}");
}

#[test]
fn a_chats_runs_search_runs_chat_in_order() {
    let p = plan(&Db::open_in_memory().unwrap(), CHAT_RUNS);
    assert!(searches(&p, "runs", "runs_chat"), "{p:?}");
    assert!(no_scan(&p) && !sorts(&p), "{p:?}");
}

#[test]
fn the_release_of_a_runs_claim_searches_tasks_claimed() {
    in_source("runs.rs (runs::release_claim)", RUNS_RS, &[RELEASE_CLAIM]);
    let p = plan(&Db::open_in_memory().unwrap(), RELEASE_CLAIM);
    assert!(searches(&p, "tasks", "tasks_claimed"), "{p:?}");
    assert!(no_scan(&p), "{p:?}");
}

// ---- the other reads the indexes are for ----

#[test]
fn the_board_checks_reads_search_their_indexes() {
    // the busy runs per agent, a card's last change, a person's answer after a hold, what the last check saw
    let busy = "SELECT agent_actor_id, count(*), max(COALESCE(started_at, created_at)) FROM runs
                                WHERE task_id IS NOT NULL AND status IN ('queued','running','waiting_approval') GROUP BY agent_actor_id";
    // GA-191: a card's last change is the newest of three lookups, each through its index
    let not_lead = "(?2 IS NULL OR ch.actor_id IS NULL OR ch.actor_id <> ?2)";
    let stamp_own = "SELECT ch.seq, ch.hlc FROM changes ch WHERE ch.row_id = ?1 AND {NOT_LEAD} ORDER BY ch.seq DESC LIMIT 1";
    let stamp_comments = "SELECT ch.seq, ch.hlc FROM comments m JOIN changes ch ON ch.row_id = m.id WHERE m.task_id = ?1 AND {NOT_LEAD}
                 ORDER BY ch.seq DESC LIMIT 1";
    let stamp_runs = "SELECT ch.seq, ch.hlc FROM runs r JOIN changes ch ON ch.row_id = r.id WHERE r.task_id = ?1 AND {NOT_LEAD}
                 ORDER BY ch.seq DESC LIMIT 1";
    let answer = "SELECT c.body_md, c.created_at FROM comments c JOIN actors a ON a.id = c.author_actor_id
                         WHERE c.task_id=?1 AND c.deleted_at IS NULL AND a.kind='person' AND c.created_at > ?2
                         ORDER BY c.created_at DESC, c.rowid DESC LIMIT 1";
    let seen = "SELECT findings_json FROM runs WHERE agent_actor_id=?1 AND trigger='board_check' AND status='succeeded'
             ORDER BY created_at DESC, rowid DESC LIMIT 1";
    in_source("board.rs", BOARD_RS, &[busy, not_lead, stamp_own, stamp_comments, stamp_runs, answer, seen]);
    // an agent's spend this month (its budget), and the runs a restart finds (runs::ACTIVE)
    let spend = "SELECT COALESCE(SUM(cost_usd_micros), 0) FROM runs WHERE agent_actor_id=?1 AND created_at >= ?2";
    let active = "('queued','running','waiting_approval')";
    let recover = "SELECT id, trigger, chat_thread_id FROM runs WHERE status IN {ACTIVE} ORDER BY created_at";
    in_source("runs.rs", RUNS_RS, &[spend, active, recover]);
    // a release under way in the project
    let release = "SELECT t.identifier, a.name FROM tasks t
         JOIN actors a ON a.id = t.assignee_actor_id AND a.kind = 'agent' AND a.deleted_at IS NULL
         WHERE t.project_id = ?1 AND t.deleted_at IS NULL AND t.state_category = 'deploy'
           AND EXISTS (SELECT 1 FROM team_members m WHERE m.actor_id = a.id AND m.deleted_at IS NULL AND m.role_key = 'devops')
         ORDER BY t.sort_key, t.created_at LIMIT 1";
    in_source("pulls.rs (pulls::release_in)", PULLS_RS, &[release]);

    let db = Db::open_in_memory().unwrap();
    // what it is, its SQL, the table (as named there) it reads by the index, the index, and the tables it may not scan
    let reads: [(&str, String, &str, &str, &[&str]); 9] = [
        ("busy runs", busy.to_string(), "runs", "runs_active", &["runs"]),
        ("a card's own last change", stamp_own.replace("{NOT_LEAD}", not_lead), "ch", "changes_row", &["ch"]),
        ("its comments' last change", stamp_comments.replace("{NOT_LEAD}", not_lead), "m", "comments_task", &["ch", "m"]),
        ("its runs' last change", stamp_runs.replace("{NOT_LEAD}", not_lead), "r", "runs_task", &["ch", "r"]),
        ("an answer after a hold", answer.to_string(), "c", "comments_task", &["c"]),
        ("what the last check saw", seen.to_string(), "runs", "runs_agent_created", &["runs"]),
        ("an agent's spend", spend.to_string(), "runs", "runs_agent_created", &["runs"]),
        ("the runs a restart finds", recover.replace("{ACTIVE}", active), "runs", "runs_active", &["runs"]),
        ("a release under way", release.to_string(), "t", "tasks_project", &["t"]),
    ];
    for (name, sql, alias, index, tables) in reads {
        let p = plan(&db, &sql);
        assert!(searches(&p, alias, index), "{name}: {p:?}");
        assert!(scans(&p, tables).is_empty(), "{name}: {p:?}");
    }
}

#[test]
fn the_usage_pages_four_reads_search_runs_created() {
    let unknown = "(COALESCE({alias}.cost_usd_micros, 0) = 0 AND COALESCE({alias}.input_tokens, 0) + COALESCE({alias}.output_tokens, 0) > 0)";
    let sums = "count(*), COALESCE(SUM(r.trigger = 'chat'), 0), COALESCE(SUM(r.input_tokens), 0), COALESCE(SUM(r.output_tokens), 0),
                        COALESCE(SUM(r.cost_usd_micros), 0), COALESCE(SUM({unknown}), 0)";
    let period = "r.created_at >= ?1 AND r.created_at < ?2";
    let total = "SELECT {sums} FROM runs r WHERE {period}";
    let days = "SELECT (r.created_at - ?1) / {DAY_MS}, {sums} FROM runs r WHERE {period} GROUP BY 1";
    let agents = "SELECT r.agent_actor_id, COALESCE(a.name, 'A removed agent'),
                    COALESCE((SELECT m.role_key FROM team_members m WHERE m.actor_id = r.agent_actor_id ORDER BY m.is_lead DESC LIMIT 1), MAX(r.role_key)),
                    {sums}
             FROM runs r LEFT JOIN actors a ON a.id = r.agent_actor_id
             WHERE {period} GROUP BY r.agent_actor_id";
    let projects = "SELECT r.task_id IS NULL, t.project_id, COALESCE(p.number, ''), COALESCE(p.key, ''), COALESCE(p.name, 'A removed project'), p.color,
                    {sums}
             FROM runs r LEFT JOIN tasks t ON t.id = r.task_id LEFT JOIN projects p ON p.id = t.project_id
             WHERE {period} GROUP BY r.task_id IS NULL, t.project_id";
    in_source("usage.rs (usage::summary)", USAGE_RS, &[unknown, sums, period, total, days, agents, projects]);
    let sums = sums.replace("{unknown}", &unknown.replace("{alias}", "r"));
    let db = Db::open_in_memory().unwrap();
    for (name, sql) in [("total", total), ("per day", days), ("per agent", agents), ("per project", projects)] {
        let p = plan(&db, &sql.replace("{sums}", &sums).replace("{period}", period).replace("{DAY_MS}", "86400000"));
        assert!(searches(&p, "r", "runs_created"), "{name}: {p:?}");
        assert!(scans(&p, &["r", "t"]).is_empty(), "{name}: {p:?}");
    }
}

#[test]
fn the_project_lists_counts_and_runs_this_month_search_tasks_project() {
    let select = "SELECT p.id, p.client_id, c.name, p.number, p.key, p.name, p.status, p.color, p.goal_md,
    r.local_path, coalesce(r.default_branch, 'main'), p.team_id, p.budget_amount_minor, p.budget_hours,
    (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.deleted_at IS NULL AND t.state_category NOT IN ('done','cancelled')),
    (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.deleted_at IS NULL AND t.state_category = 'done'),
    p.updated_at, NULLIF(r.remote_url, ''), p.worktree_copy_json, p.worktree_install, p.worktree_setup,
    (SELECT COALESCE(SUM(u.cost_usd_micros), 0) FROM runs u JOIN tasks ut ON ut.id = u.task_id WHERE ut.project_id = p.id AND u.created_at >= ?1),
    (SELECT count(*) FROM runs u JOIN tasks ut ON ut.id = u.task_id WHERE ut.project_id = p.id AND u.created_at >= ?1 AND {unknown}),
    p.lead_may_merge, p.screenshots_json, p.preview_command, p.preview_start_secs, p.test_runs_at_once
  FROM projects p
  LEFT JOIN clients c ON c.id = p.client_id
  LEFT JOIN repos r ON r.project_id = p.id AND r.deleted_at IS NULL";
    in_source("projects.rs (projects::select)", PROJECTS_RS, &[select]);
    let unknown = "(COALESCE(u.cost_usd_micros, 0) = 0 AND COALESCE(u.input_tokens, 0) + COALESCE(u.output_tokens, 0) > 0)";
    let p = plan(&Db::open_in_memory().unwrap(), &select.replace("{unknown}", unknown));
    assert!(searches(&p, "t", "tasks_project") && searches(&p, "ut", "tasks_project"), "{p:?}");
    assert_eq!(p.iter().filter(|s| s.contains("INDEX tasks_project (")).count(), 4, "both counts and both run sums: {p:?}");
    assert!(scans(&p, &["t", "ut", "u"]).is_empty(), "{p:?}");
}

// ---- the schema ----

/// 0018's indexes as the database has them: on their table, with their columns, and their WHERE when they keep some rows.
fn check_indexes(c: &Connection) {
    for (name, table, columns, rows) in INDEXES {
        let (on, sql): (String, String) = c.query_row("SELECT tbl_name, sql FROM sqlite_master WHERE type = 'index' AND name = ?1", [name],
            |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or_else(|e| panic!("{name}: {e}"));
        let cols: Vec<String> = c.prepare(&format!("SELECT name FROM pragma_index_info('{name}') ORDER BY seqno")).unwrap()
            .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        assert_eq!((on.as_str(), cols), (table, columns.iter().map(|s| s.to_string()).collect::<Vec<_>>()), "{name}");
        match rows {
            Some(w) => assert!(sql.ends_with(&format!(" WHERE {w}")), "{name} keeps only the rows WHERE {w}: {sql}"),
            None => assert!(!sql.contains(" WHERE "), "{name} keeps every row: {sql}"),
        }
    }
}

#[test]
fn a_new_database_has_the_eight_indexes() {
    assert_eq!(db::SCHEMA_VERSION, 21);
    Db::open_in_memory().unwrap().read(|c| {
        check_indexes(c);
        Ok(())
    }).unwrap();
}

/// Migrations 0001..0017 on a new file at `path`: a genuine schema 17 database, as a version before GA-180 leaves it.
fn v17_db(path: &Path) -> Connection {
    let all = [
        include_str!("../../migrations/0001_init.sql"), include_str!("../../migrations/0002_agents.sql"), include_str!("../../migrations/0003_chat.sql"),
        include_str!("../../migrations/0004_effort.sql"), include_str!("../../migrations/0005_pull_requests.sql"),
        include_str!("../../migrations/0006_worktree_prepare.sql"), include_str!("../../migrations/0007_card_flow.sql"),
        include_str!("../../migrations/0008_board_check.sql"), include_str!("../../migrations/0009_agent_folders.sql"),
        include_str!("../../migrations/0010_run_refusals.sql"), include_str!("../../migrations/0011_column_agents.sql"),
        include_str!("../../migrations/0012_chat_runs_on.sql"), include_str!("../../migrations/0013_bitbucket.sql"),
        include_str!("../../migrations/0014_memory.sql"), include_str!("../../migrations/0015_lead_merges.sql"),
        include_str!("../../migrations/0016_memory_groups.sql"), include_str!("../../migrations/0017_pending_actions.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 4, "four schema steps back (0018, and GA-182's 0019, GA-212's 0020 and GA-228's 0021 after it)");
    let mut c = Connection::open(path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c
}

/// GA-182's 0019 columns and GA-212's 0020 column: NULL in every row an upgrade keeps.
const COLUMNS_0019_0020: [(&str, &str); 9] = [("files", "caption"), ("files", "width"), ("files", "height"), ("runs", "screenshots_note"),
    ("tasks", "screenshots"), ("projects", "screenshots_json"), ("projects", "preview_command"), ("projects", "preview_start_secs"),
    ("projects", "test_runs_at_once")];

/// Undoes GA-228's 0021: no Builds on table.
const UNDO_0021: &str = "DROP INDEX task_builds_on_needed_by; DROP TABLE task_builds_on;";

/// Undoes GA-212's 0020: no Test runs at once on projects.
const UNDO_0020: &str = "ALTER TABLE projects DROP COLUMN test_runs_at_once;";

/// Undoes GA-182's 0019: no screenshot columns on files, runs, tasks and projects, and no files_run index.
const UNDO_0019: &str = "DROP INDEX files_run; ALTER TABLE files DROP COLUMN caption; ALTER TABLE files DROP COLUMN width;
    ALTER TABLE files DROP COLUMN height; ALTER TABLE runs DROP COLUMN screenshots_note; ALTER TABLE tasks DROP COLUMN screenshots;
    ALTER TABLE projects DROP COLUMN screenshots_json; ALTER TABLE projects DROP COLUMN preview_command;
    ALTER TABLE projects DROP COLUMN preview_start_secs;";

/// As all_rows, without 0019's and 0020's columns, each checked NULL in every row first: an upgraded database's rows, to
/// compare with those of the schema before it.
fn rows_without_0019_0020(c: &Connection) -> std::collections::BTreeMap<String, Vec<String>> {
    for (t, col) in COLUMNS_0019_0020 {
        let set: i64 = c.query_row(&format!("SELECT count(*) FROM {t} WHERE {col} IS NOT NULL"), [], |r| r.get(0)).unwrap();
        assert_eq!(set, 0, "{t}.{col} is NULL in every kept row");
    }
    // GA-228's 0021 adds task_builds_on, empty after an upgrade
    let builds_on: i64 = c.query_row("SELECT count(*) FROM task_builds_on", [], |r| r.get(0)).unwrap();
    assert_eq!(builds_on, 0, "no card builds on another after an upgrade");
    let tables: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> 'task_builds_on'").unwrap()
        .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    tables.into_iter().map(|t| {
        let cols: Vec<String> = c.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid").unwrap()
            .query_map([&t], |r| r.get(0)).unwrap().map(Result::unwrap)
            .filter(|n: &String| !COLUMNS_0019_0020.contains(&(t.as_str(), n.as_str()))).collect();
        let list = cols.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
        let mut s = c.prepare(&format!("SELECT {list} FROM \"{t}\"")).unwrap();
        let n = s.column_count();
        let mut rows: Vec<String> = s.query_map([], |r| Ok((0..n).map(|i| format!("{:?}", r.get::<_, rusqlite::types::Value>(i).unwrap()))
            .collect::<Vec<_>>().join(" | "))).unwrap().map(Result::unwrap).collect();
        rows.sort();
        (t, rows)
    }).collect()
}

/// Every row of every table, sorted, as text: what an upgrade must keep.
fn all_rows(c: &Connection) -> std::collections::BTreeMap<String, Vec<String>> {
    let tables: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'").unwrap()
        .query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    tables.into_iter().map(|t| {
        let mut s = c.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = s.column_count();
        let mut rows: Vec<String> = s.query_map([], |r| Ok((0..n).map(|i| format!("{:?}", r.get::<_, rusqlite::types::Value>(i).unwrap()))
            .collect::<Vec<_>>().join(" | "))).unwrap().map(Result::unwrap).collect();
        rows.sort();
        (t, rows)
    }).collect()
}

/// The schema as SQLite keeps it: every table, index, trigger and view with its SQL.
/// The schema, without SQLite's own statistics (GA-191: PRAGMA optimize at open may add sqlite_stat1 and sqlite_stat4).
fn schema(c: &Connection) -> Vec<(String, String, Option<String>)> {
    c.prepare("SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_stat%' ORDER BY type, name").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
}

fn version(c: &Connection) -> i64 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

/// The run that holds the card's claim, if one does.
fn claimed(db: &Db, task_id: &str) -> Option<String> {
    db.read(|c| Ok(c.query_row("SELECT claimed_by_run_id FROM tasks WHERE id = ?1", [task_id], |r| r.get(0))?)).unwrap()
}

struct Work {
    you: String,
    backend: String,
    /// A card with three comments, the second deleted, and a run that ended.
    card: String,
    the_deleted: String,
    /// A card whose run holds its claim.
    other: String,
    claim: String,
    /// The Team Lead's chat: two messages and a chat run.
    thread: String,
}

/// A day's work, made through the app.
fn work(db: &Db) -> Work {
    let s = seed::ensure_seed(db, "Jeffrey").unwrap();
    let you = s.you_id.clone();
    let p = projects::create(db, &you, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let card = tasks::create(db, &you, TaskInput { project_id: p.clone(), title: "Export invoices".into(), ..Default::default() }).unwrap();
    let other = tasks::create(db, &you, TaskInput { project_id: p.clone(), title: "Import clients".into(), ..Default::default() }).unwrap();
    let backend = team::add_agent(db, &you, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(),
        ..Default::default() }).unwrap();
    let ended = runs::create(db, &backend, &card, "backend", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/r1.jsonl").unwrap();
    runs::finish(db, &ended, "succeeded", None, 1200, 10, 20, None).unwrap();
    let claim = runs::create(db, &backend, &other, "backend", "S", "/tmp", "/tmp", "gizai/kade-2", "/tmp/r2.jsonl").unwrap();
    comments::add(db, &you, &card, "Use the KADE template.", None).unwrap();
    let the_deleted = comments::add(db, &backend, &card, "Done, see the branch.", Some(ended.as_str())).unwrap();
    comments::add(db, &you, &card, "Thanks.", None).unwrap();
    // GA-191: reads of a database file are read-only, so this goes through the writer
    db.write(None, |w| Ok(w.conn().execute("UPDATE comments SET deleted_at = 9 WHERE id = ?1", [&the_deleted])?)).unwrap();
    let lead = team::add_agent(db, &you, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    let thread = chat::create_thread(db, &you, &lead, "Plan the invoices export").unwrap();
    chat::add_message(db, NewMessage { thread_id: thread.clone(), role: "user".into(), author_id: Some(you.clone()),
        body_md: Some("Plan the invoices export".into()), ..Default::default() }).unwrap();
    let turn = runs::create_chat(db, &lead, &thread, "S", "/tmp", "/tmp/c.jsonl").unwrap();
    chat::add_message(db, NewMessage { thread_id: thread.clone(), role: "agent".into(), author_id: Some(lead.clone()),
        body_md: Some("I made KADE-1.".into()), run_id: Some(turn.clone()), ..Default::default() }).unwrap();
    runs::finish_chat(db, &turn, "succeeded", 300, 5, 7, None).unwrap();
    Work { you, backend, card, the_deleted, other, claim, thread }
}

#[test]
fn a_v17_database_full_of_work_gets_the_indexes_after_a_backup_every_row_kept_and_a_second_open_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let db = Db::open(&path).unwrap();
    let w = work(&db);
    drop(db);
    // four steps back to the database a version before GA-180 leaves (schema 17): 0018 only adds indexes, GA-182's 0019 only columns
    // and an index, GA-212's 0020 only a column, and GA-228's 0021 only a table and its index
    let c = Connection::open(&path).unwrap();
    let drops: String = INDEXES.iter().map(|(name, ..)| format!("DROP INDEX {name}; ")).collect();
    c.execute_batch(&format!("{UNDO_0021} {UNDO_0020} {UNDO_0019} {drops}PRAGMA user_version = 17;")).unwrap();
    let other = tempfile::tempdir().unwrap();
    let genuine = v17_db(&other.path().join("gizai.db"));
    assert_eq!(schema(&c), schema(&genuine), "the step back is exactly schema 17 (migrations 0001..0017)");
    let before = all_rows(&c);
    for t in ["tasks", "comments", "chat_messages", "runs", "changes"] {
        assert!(before.get(t).is_some_and(|rows| !rows.is_empty()), "{t} has rows before the upgrade");
    }
    drop(c);

    // Gizai opens it
    let db = Db::open(&path).unwrap();
    let (v, broken, ok): (i64, i64, String) = db.read(|c| Ok((version(c),
        c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?,
        c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?))).unwrap();
    assert_eq!((v, broken, ok.as_str()), (21, 0, "ok"), "schema 21, foreign keys and the file (indexes too) intact");
    let with_0019_0020 = v17_db(&other.path().join("with-0019-0020.db"));
    with_0019_0020.execute_batch(include_str!("../../migrations/0019_screenshots.sql")).unwrap();
    with_0019_0020.execute_batch(include_str!("../../migrations/0020_test_runs_at_once.sql")).unwrap();
    with_0019_0020.execute_batch(include_str!("../../migrations/0021_builds_on.sql")).unwrap();
    db.read(|c| {
        check_indexes(c);
        let mut rest = schema(c);
        rest.retain(|(_, name, _)| !INDEXES.iter().any(|(n, ..)| name.as_str() == *n));
        assert_eq!(rest, schema(&with_0019_0020), "the eight indexes and nothing else are new, besides 0019's columns and index, 0020's column and 0021's table");
        let after = rows_without_0019_0020(c);
        assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>(), "no table lost or added");
        for (t, rows) in &before {
            assert!(&after[t] == rows, "{t}: every row kept as it was ({} before, {} after)", rows.len(), after[t].len());
        }
        Ok(())
    }).unwrap();
    // the backup made before the upgrade is that schema 17 database, with the same rows
    let snaps: Vec<_> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().path()).collect();
    assert!(snaps.len() == 1 && snaps[0].file_name().unwrap().to_string_lossy().starts_with("gizai-before-v21-"), "{snaps:?}");
    let old = Connection::open(&snaps[0]).unwrap();
    assert_eq!(version(&old), 17, "the backup is the schema 17 database");
    assert!(!schema(&old).iter().any(|(_, name, _)| INDEXES.iter().any(|(n, ..)| name.as_str() == *n)), "with none of 0018's indexes");
    assert!(all_rows(&old) == before, "the backup holds every row");
    drop(old);
    drop(db);

    // opening it again changes nothing and makes no new backup
    let (schema_1, rows_1) = {
        let c = Connection::open(&path).unwrap();
        (schema(&c), all_rows(&c))
    };
    let db = Db::open(&path).unwrap();
    let (schema_2, rows_2, v) = db.read(|c| Ok((schema(c), all_rows(c), version(c)))).unwrap();
    assert_eq!((schema_2, v), (schema_1, 21), "the same schema");
    assert!(rows_2 == rows_1, "the same rows");
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1, "no new backup");

    // and the work reads as before
    let listed: Vec<String> = comments::list(&db, &w.card).unwrap().into_iter().map(|m| m.body_md).collect();
    assert_eq!(listed, ["Use the KADE template.", "Thanks."], "the live comments, oldest first");
    let comment_changes = tasks::activity(&db, &w.card).unwrap().into_iter().filter(|e| e.table == "comments").count();
    assert_eq!(comment_changes, 3, "the activity keeps the deleted comment's change");
    let bodies: Vec<Option<String>> = chat::messages(&db, &w.thread).unwrap().into_iter().map(|m| m.body_md).collect();
    assert_eq!(bodies, [Some("Plan the invoices export".to_string()), Some("I made KADE-1.".to_string())]);
    assert_eq!(claimed(&db, &w.other), Some(w.claim.clone()));
    runs::finish(&db, &w.claim, "succeeded", None, 0, 0, 0, None).unwrap();
    assert_eq!(claimed(&db, &w.other), None, "the run's end frees its card");
}

#[test]
fn a_database_with_a_comments_task_index_of_its_own_still_opens_and_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    v17_db(&path).execute_batch("CREATE INDEX comments_task ON comments(task_id);").unwrap();
    let db = Db::open(&path).unwrap();
    let (v, sql, others): (i64, String, i64) = db.read(|c| Ok((version(c),
        c.query_row("SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'comments_task'", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name IN ('chat_messages_thread', 'runs_chat', 'tasks_claimed',
                     'runs_active', 'runs_agent_created', 'runs_created', 'tasks_project')", [], |r| r.get(0))?))).unwrap();
    assert_eq!((v, sql.as_str(), others), (21, "CREATE INDEX comments_task ON comments(task_id)", 7), "its own comments_task kept, the seven others added");
    let p = plan(&db, COMMENTS_LIST);
    assert!(searches(&p, "m", "comments_task") && no_scan(&p), "a card's comments still go through it: {p:?}");
}

// ---- what the reads give stays the same ----

#[test]
fn a_cards_comments_still_come_oldest_first_by_id_at_one_moment_and_a_deleted_one_stays_out_of_the_list_not_the_activity() {
    let db = Db::open_in_memory().unwrap();
    let w = work(&db);
    // three more at one moment, earlier than the others
    let same: Vec<String> = (0..3).map(|i| comments::add(&db, &w.you, &w.card, &format!("At one moment, {i}."), None).unwrap()).collect();
    db.read(|c| {
        for id in &same {
            c.execute("UPDATE comments SET created_at = 5 WHERE id = ?1", [id])?;
        }
        Ok(())
    }).unwrap();
    let listed: Vec<(i64, String)> = comments::list(&db, &w.card).unwrap().into_iter().map(|m| (m.created_at, m.id)).collect();
    let mut expected: Vec<(i64, String)> = db.read(|c| {
        let mut st = c.prepare("SELECT created_at, id FROM comments WHERE task_id = ?1 AND deleted_at IS NULL")?;
        let all = st.query_map([&w.card], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(all)
    }).unwrap();
    expected.sort();
    assert_eq!(listed, expected, "by time, then by id");
    assert_eq!(listed.len(), 5, "five live comments");
    let mut first: Vec<String> = listed[..3].iter().map(|(at, id)| { assert_eq!(*at, 5); id.clone() }).collect();
    first.sort();
    let mut same = same;
    same.sort();
    assert_eq!(first, same, "the three at one moment first, by id");
    assert!(!listed.iter().any(|(_, id)| id == &w.the_deleted), "the deleted comment stays out of the list");
    let comment_changes = tasks::activity(&db, &w.card).unwrap().into_iter().filter(|e| e.table == "comments").count();
    assert_eq!(comment_changes, 6, "every comment's change is in the activity, the deleted one's too");
}

#[test]
fn a_chats_messages_still_come_in_the_order_written() {
    let db = Db::open_in_memory().unwrap();
    let w = work(&db);
    // a third message dated before the others (a clock that went back): the chat still reads in the order written
    let late = chat::add_message(&db, NewMessage { thread_id: w.thread.clone(), role: "user".into(), author_id: Some(w.you.clone()),
        body_md: Some("And the credit notes?".into()), ..Default::default() }).unwrap();
    db.read(|c| Ok(c.execute("UPDATE chat_messages SET created_at = 1 WHERE id = ?1", [&late.id])?)).unwrap();
    let bodies: Vec<Option<String>> = chat::messages(&db, &w.thread).unwrap().into_iter().map(|m| m.body_md).collect();
    assert_eq!(bodies, [Some("Plan the invoices export".to_string()), Some("I made KADE-1.".to_string()), Some("And the credit notes?".to_string())]);
}

#[test]
fn a_runs_end_frees_its_own_card_only_and_a_restart_frees_the_runs_it_left() {
    let db = Db::open_in_memory().unwrap();
    let w = work(&db);
    assert_eq!((claimed(&db, &w.card), claimed(&db, &w.other)), (None, Some(w.claim.clone())), "the ended run freed its card");
    let again = runs::create(&db, &w.backend, &w.card, "backend", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/r3.jsonl").unwrap();
    runs::finish(&db, &w.claim, "failed", None, 0, 0, 0, Some("exit 1")).unwrap();
    assert_eq!((claimed(&db, &w.card), claimed(&db, &w.other)), (Some(again.clone()), None), "a run's end frees its own card, not another's");
    assert_eq!(runs::recover_interrupted(&db).unwrap(), 1);
    assert_eq!(claimed(&db, &w.card), None, "a restart fails the run it left and frees its card");
    assert_eq!(runs::get(&db, &again).unwrap().status, "failed");
}
