//! GA-212 QA: Test runs at once in core. Migration 0020 on a schema 19 database (every project keeps no limit of its own,
//! after a backup); the project setting (1 to 20, 0 clears it, left out keeps it, anything else refused with why, and
//! nothing changes) and its JSON for the app; `workflow::test_runs_full`, the check the queue and a start by hand make
//! (only the project's other cards in a Testing column count, cards in other columns and other projects' cards never
//! count or wait, the message word for word with the cards by number); and the board check's `test_runs_full`: a card
//! held up only by it is waiting after an hour, not before, with the project and its limit in the reason.
use std::path::Path;

use gizai_core::board::{self, Context, Finding};
use gizai_core::db::{self, Db};
use gizai_core::workflow::{self, TestRunsFull};
use gizai_core::{ids, model::*, projects, runs, seed, tasks, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

const HOUR: i64 = 60 * 60_000;
const ONE: &str = "Giz AI tests 1 card at a time (project settings): wait for GA-1 to finish testing, or raise the limit on the project page.";

// ---- set-up ----

struct B { db: Db, you: String, team: String, qa: String, be: String, lead: String, a: String, b: String }

/// Project A, "Giz AI" (GA), and project B, "Kade" (KADE), neither with a limit yet; a QA Agent with three cards at once
/// (on Testing), a Backend Agent (on To do and In progress) and a Team Lead.
fn board_() -> B {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let a = projects::create(&db, &s.you_id, ProjectInput { name: "Giz AI".into(), key: "GA".into(), ..Default::default() }).unwrap();
    let b = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let agent = |name: &str, role: &str, max_runs: i64| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), wakeup: "on_assign".into(), max_runs: Some(max_runs), chat_enabled: Some(role == "lead"),
        ..Default::default() }).unwrap();
    let qa = agent("QA Agent", "qa", 3);
    let be = agent("Backend Agent", "backend", 3);
    let lead = agent("Team Lead", "lead", 1);
    gizai_core::questions::set_enabled(&db, false).unwrap();
    B { db, you: s.you_id, team: s.team_id, qa, be, lead, a, b }
}

impl B {
    fn state(&self, name: &str) -> String {
        team::get(&self.db, &self.team).unwrap().states.into_iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id
    }
    fn card(&self, project: &str, column: &str) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: project.to_string(), title: format!("A card in {column}"),
            state_id: Some(self.state(column)), ..Default::default() }).unwrap()
    }
    fn ident(&self, t: &str) -> String { tasks::get(&self.db, t).unwrap().identifier }
    fn limit(&self, project: &str, n: i64) {
        let p = projects::get(&self.db, project).unwrap();
        projects::update(&self.db, &self.you, project, ProjectInput { name: p.name, key: p.key, test_runs_at_once: Some(n), ..Default::default() }).unwrap();
    }
    fn full(&self, task: &str, under_way: &[&String]) -> Option<TestRunsFull> {
        let under_way: Vec<String> = under_way.iter().map(|s| s.to_string()).collect();
        workflow::test_runs_full(&self.db, task, &under_way).unwrap()
    }
    /// A run at work (queued) by `agent` on `task`.
    fn run(&self, agent: &str, task: &str) -> String {
        runs::create(&self.db, agent, task, "qa", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap()
    }
    fn cx_at(&self, now: i64) -> Context {
        Context { now, max_concurrent: 10, lead_id: Some(self.lead.clone()), ..Default::default() }
    }
    fn check_at(&self, now: i64) -> Vec<Finding> {
        board::check(&self.db, &self.cx_at(now)).unwrap()
    }
    fn of(&self, all: &[Finding], task: &str) -> Vec<(String, String)> {
        all.iter().filter(|f| f.task_id == task).map(|f| (f.kind.clone(), f.code.clone())).collect()
    }
    /// When the card last changed, as the board check counts it (its last change, else its creation).
    fn changed(&self, task: &str) -> i64 {
        let t = tasks::get(&self.db, task).unwrap();
        t.updated_at.max(t.created_at)
    }
}

fn waiting(code: &str) -> Vec<(String, String)> {
    vec![("waiting".to_string(), code.to_string())]
}

// ---- 1. migration 0020 ----

/// Migrations 0001..0019 on a new file at `path`: a genuine schema 19 database, as the version before GA-212 leaves it.
fn v19_db(path: &Path) -> Connection {
    let all = [
        include_str!("../../migrations/0001_init.sql"), include_str!("../../migrations/0002_agents.sql"), include_str!("../../migrations/0003_chat.sql"),
        include_str!("../../migrations/0004_effort.sql"), include_str!("../../migrations/0005_pull_requests.sql"),
        include_str!("../../migrations/0006_worktree_prepare.sql"), include_str!("../../migrations/0007_card_flow.sql"),
        include_str!("../../migrations/0008_board_check.sql"), include_str!("../../migrations/0009_agent_folders.sql"),
        include_str!("../../migrations/0010_run_refusals.sql"), include_str!("../../migrations/0011_column_agents.sql"),
        include_str!("../../migrations/0012_chat_runs_on.sql"), include_str!("../../migrations/0013_bitbucket.sql"),
        include_str!("../../migrations/0014_memory.sql"), include_str!("../../migrations/0015_lead_merges.sql"),
        include_str!("../../migrations/0016_memory_groups.sql"), include_str!("../../migrations/0017_pending_actions.sql"),
        include_str!("../../migrations/0018_indexes.sql"), include_str!("../../migrations/0019_screenshots.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 1, "one schema step back");
    let mut c = Connection::open(path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c
}

/// Every table, index and trigger with its SQL, sorted.
fn schema(c: &Connection) -> Vec<(String, String, Option<String>)> {
    let mut v: Vec<_> = c.prepare("SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    v.sort();
    v
}

fn version(c: &Connection) -> i64 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

#[test]
fn a_schema_19_database_gets_test_runs_at_once_empty_for_every_project_after_a_backup() {
    assert_eq!(db::SCHEMA_VERSION, 20, "0020 is the latest migration");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let (kade, shop) = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let kade = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
        let shop = projects::create(&db, &s.you_id, ProjectInput { name: "Shop".into(), key: "SHOP".into(), worktree_setup: Some("make".into()),
            ..Default::default() }).unwrap();
        (kade, shop)
    };
    // one step back to the database the version before GA-212 leaves (schema 19): 0020 only adds the column
    let c = Connection::open(&path).unwrap();
    c.execute_batch("ALTER TABLE projects DROP COLUMN test_runs_at_once; PRAGMA user_version = 19;").unwrap();
    let other = tempfile::tempdir().unwrap();
    let genuine = v19_db(&other.path().join("gizai.db"));
    assert_eq!(schema(&c), schema(&genuine), "the step back is exactly schema 19 (migrations 0001..0019)");
    let before: Vec<(String, String, Option<String>)> = c.prepare("SELECT id, name, worktree_setup FROM projects ORDER BY name").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    drop(c);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(version(c), 20, "0020 ran");
        let col: (String, i64, Option<String>) = c.query_row(
            "SELECT type, \"notnull\", dflt_value FROM pragma_table_info('projects') WHERE name = 'test_runs_at_once'", [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        assert_eq!(col, ("INTEGER".to_string(), 0, None), "an optional whole number, empty by default");
        let set: i64 = c.query_row("SELECT count(*) FROM projects WHERE test_runs_at_once IS NOT NULL", [], |r| r.get(0))?;
        assert_eq!(set, 0, "no project has a limit of its own");
        let after: Vec<(String, String, Option<String>)> = c.prepare("SELECT id, name, worktree_setup FROM projects ORDER BY name")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?;
        assert_eq!(after, before, "the projects are kept as they were");
        let mut rest = schema(c);
        rest.retain(|(kind, name, _)| !(kind == "table" && name == "projects"));
        let mut old = schema(&genuine);
        old.retain(|(kind, name, _)| !(kind == "table" && name == "projects"));
        assert_eq!(rest, old, "nothing else changed");
        Ok(())
    }).unwrap();
    for p in [&kade, &shop] {
        assert_eq!(projects::get(&db, p).unwrap().test_runs_at_once, None, "works as before: no limit of its own");
    }
    assert!(projects::list(&db).unwrap().iter().all(|p| p.test_runs_at_once.is_none()));
    // the backup made before the upgrade is the schema 19 database, without the column
    let snaps: Vec<_> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().path()).collect();
    assert!(snaps.len() == 1 && snaps[0].file_name().unwrap().to_string_lossy().starts_with("gizai-before-v20-"), "{snaps:?}");
    let old = Connection::open(&snaps[0]).unwrap();
    assert_eq!(version(&old), 19, "the backup is the schema 19 database");
    assert_eq!(schema(&old), schema(&genuine));
    // opening it again changes nothing and makes no new backup
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.read(|c| Ok(version(c))).unwrap(), 20);
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1, "no new backup");
}

// ---- 2. the project setting ----

#[test]
fn a_project_saves_test_runs_at_once_from_1_to_20_0_clears_it_left_out_keeps_it_and_another_value_is_refused() {
    let b = board_();
    let get = |p: &str| projects::get(&b.db, p).unwrap().test_runs_at_once;
    let input = |name: &str, key: &str, n: Option<i64>| ProjectInput { name: name.into(), key: key.into(), test_runs_at_once: n, ..Default::default() };
    let why = "Test runs at once must be from 1 to 20, or empty for no limit.";

    // a new project: none (no limit of its own) unless given, 1 and 20 are the bounds, 0 is none
    assert_eq!(get(&b.a), None, "the default");
    let p1 = projects::create(&b.db, &b.you, input("One", "ONE", Some(1))).unwrap();
    let p20 = projects::create(&b.db, &b.you, input("Twenty", "TWEN", Some(20))).unwrap();
    let p0 = projects::create(&b.db, &b.you, input("Zero", "ZERO", Some(0))).unwrap();
    assert_eq!((get(&p1), get(&p20), get(&p0)), (Some(1), Some(20), None));
    let count = || projects::list(&b.db).unwrap().len();
    let n = count();
    for bad in [21, -1, 100] {
        let e = projects::create(&b.db, &b.you, input("Bad", "BAD", Some(bad))).unwrap_err();
        assert_eq!(e.to_string(), why, "{bad}");
    }
    assert_eq!(count(), n, "no project made");

    // an existing project: set, kept when left out (another field changed), changed, cleared with 0
    b.limit(&b.a, 2);
    assert_eq!(get(&b.a), Some(2));
    projects::update(&b.db, &b.you, &b.a, ProjectInput { name: "Giz AI app".into(), key: "GA".into(), ..Default::default() }).unwrap();
    assert_eq!((get(&b.a), projects::get(&b.db, &b.a).unwrap().name.as_str()), (Some(2), "Giz AI app"), "left out keeps it");
    b.limit(&b.a, 5);
    assert_eq!(get(&b.a), Some(5));
    for bad in [21, -3] {
        let e = projects::update(&b.db, &b.you, &b.a, input("Renamed", "GA", Some(bad))).unwrap_err();
        assert_eq!(e.to_string(), why, "{bad}");
    }
    let p = projects::get(&b.db, &b.a).unwrap();
    assert_eq!((p.test_runs_at_once, p.name.as_str()), (Some(5), "Giz AI app"), "a refused save changes nothing");
    b.limit(&b.a, 0);
    assert_eq!(get(&b.a), None, "0 clears it");
    assert_eq!(get(&b.b), None, "another project is untouched");
}

#[test]
fn the_app_gets_test_runs_at_once_as_a_number_or_null_and_sends_it_as_test_runs_at_once() {
    let b = board_();
    b.limit(&b.a, 3);
    let a = serde_json::to_value(projects::get(&b.db, &b.a).unwrap()).unwrap();
    let k = serde_json::to_value(projects::get(&b.db, &b.b).unwrap()).unwrap();
    assert_eq!((&a["testRunsAtOnce"], &k["testRunsAtOnce"]), (&serde_json::json!(3), &serde_json::Value::Null));
    // the form's save: a number, 0 for an empty field, or left out
    let i: ProjectInput = serde_json::from_value(serde_json::json!({"name": "Giz AI", "key": "GA", "testRunsAtOnce": 0})).unwrap();
    assert_eq!(i.test_runs_at_once, Some(0));
    let i: ProjectInput = serde_json::from_value(serde_json::json!({"name": "Giz AI", "key": "GA", "testRunsAtOnce": 4})).unwrap();
    assert_eq!(i.test_runs_at_once, Some(4));
    let i: ProjectInput = serde_json::from_value(serde_json::json!({"name": "Giz AI", "key": "GA", "testRunsAtOnce": null})).unwrap();
    assert_eq!(i.test_runs_at_once, None);
    // an older Project without the field reads as no limit
    let mut old = k.clone();
    old.as_object_mut().unwrap().remove("testRunsAtOnce");
    assert_eq!(serde_json::from_value::<Project>(old).unwrap().test_runs_at_once, None);
}

// ---- 3. workflow::test_runs_full ----

#[test]
fn only_the_projects_other_cards_in_a_testing_column_count_and_only_its_testing_cards_wait() {
    let b = board_();
    let a1 = b.card(&b.a, "Testing");
    let a2 = b.card(&b.a, "Testing");
    let a_todo = b.card(&b.a, "To do");
    let a_prog = b.card(&b.a, "In progress");
    let b1 = b.card(&b.b, "Testing");
    let b2 = b.card(&b.b, "Testing");
    assert_eq!((b.ident(&a1), b.ident(&a2), b.ident(&b1)), ("GA-1".to_string(), "GA-2".to_string(), "KADE-1".to_string()));

    // no setting: nothing ever waits for it
    assert_eq!(b.full(&a2, &[&a1, &a_todo, &b1]), None, "a project without the setting works as today");
    b.limit(&b.a, 1);
    assert_eq!(b.full(&a2, &[]), None, "nothing under way");
    let full = b.full(&a2, &[&a1]).expect("GA-1 in a test run fills GA's 1");
    assert_eq!(full, TestRunsFull { project: "Giz AI".into(), limit: 1, cards: vec!["GA-1".into()] });
    assert_eq!(full.message(), ONE, "word for word");
    assert_eq!(b.full(&a1, &[&a1]), None, "a card never waits for itself (a Continue on it)");
    assert_eq!(b.full(&a2, &[&a2, &a1]).map(|f| f.cards), Some(vec!["GA-1".to_string()]), "nor counts itself");
    // runs on A's cards in To do or In progress, and on B's cards in Testing, don't count
    assert_eq!(b.full(&a2, &[&a_todo, &a_prog, &b1, &b2]), None);
    // A's cards in To do or In progress never wait, also while A1 is in a test run
    assert_eq!(b.full(&a_todo, &[&a1, &a2]), None);
    assert_eq!(b.full(&a_prog, &[&a1, &a2]), None);
    // B has no limit: its Testing cards never wait for A's
    assert_eq!(b.full(&b2, &[&a1, &a2, &b1]), None);
    // a card in a test run that leaves Testing no longer counts
    tasks::move_to(&b.db, &b.you, &a1, &b.state("Review"), "").unwrap();
    assert_eq!(b.full(&a2, &[&a1]), None, "GA-1 is in Review now");
    tasks::move_to(&b.db, &b.you, &a1, &b.state("Testing"), "").unwrap();
    assert!(b.full(&a2, &[&a1]).is_some());
    // a renamed Testing column is still a column of kind testing
    gizai_core::columns::set_column(&b.db, &b.you, &b.state("Testing"), gizai_core::columns::ColumnInput { name: Some("QA check".into()),
        ..Default::default() }).unwrap();
    assert_eq!(b.full(&a2, &[&a1]).map(|f| f.message()), Some(ONE.to_string()));
    // cleared: no wait at all
    b.limit(&b.a, 0);
    assert_eq!(b.full(&a2, &[&a1]), None);
}

#[test]
fn with_a_higher_limit_it_waits_once_that_many_are_in_a_test_run_and_names_them_by_number() {
    let b = board_();
    let cards: Vec<String> = (0..12).map(|_| b.card(&b.a, "Testing")).collect();
    let card = |n: usize| &cards[n - 1];
    assert_eq!(b.ident(card(12)), "GA-12");
    b.limit(&b.a, 2);
    assert_eq!(b.full(card(2), &[card(1)]), None, "one of two");
    let full = b.full(card(2), &[card(12), card(3)]).expect("two of two");
    assert_eq!((full.limit, full.cards.clone()), (2, vec!["GA-3".to_string(), "GA-12".to_string()]), "by number: GA-3 before GA-12");
    assert_eq!(full.message(),
               "Giz AI tests 2 cards at a time (project settings): wait for GA-3 or GA-12 to finish testing, or raise the limit on the project page.");
    b.limit(&b.a, 3);
    assert_eq!(b.full(card(2), &[card(12), card(3)]), None);
    assert_eq!(b.full(card(2), &[card(12), card(10), card(3)]).unwrap().message(),
               "Giz AI tests 3 cards at a time (project settings): wait for GA-3, GA-10 or GA-12 to finish testing, or raise the limit on the project page.");
    // more than the limit under way (a limit lowered meanwhile): it still waits, and names them all
    b.limit(&b.a, 1);
    assert_eq!(b.full(card(2), &[card(1), card(11)]).unwrap().message(),
               "Giz AI tests 1 card at a time (project settings): wait for GA-1 or GA-11 to finish testing, or raise the limit on the project page.");
}

// ---- 4. the board check ----

#[test]
fn a_card_held_up_only_by_test_runs_at_once_is_waiting_test_runs_full_after_an_hour_and_not_before() {
    let b = board_();
    b.limit(&b.a, 1);
    let a1 = b.card(&b.a, "Testing");
    let a2 = b.card(&b.a, "Testing");
    b.run(&b.qa, &a1);
    let t0 = ids::now_ms().max(b.changed(&a2));
    // QA has two free slots, but A2 waits for A1's test run: no free_slot, nor anything else, within the hour
    for later in [3 * 60_000, 30 * 60_000, HOUR - 60_000] {
        assert!(b.of(&b.check_at(t0 + later), &a2).is_empty(), "{} min: {:?}", later / 60_000, b.check_at(t0 + later));
    }
    let all = b.check_at(t0 + HOUR + 60_000);
    assert_eq!(b.of(&all, &a2), waiting("test_runs_full"));
    let f = all.iter().find(|f| f.task_id == a2).unwrap();
    assert_eq!(f.reason, "Giz AI tests 1 card at a time (project settings): it has waited over an hour for GA-1 to finish testing");
    assert_eq!((f.agent_id.as_deref(), f.agent.as_deref(), f.task.as_str()), (Some(b.qa.as_str()), Some("QA Agent"), "GA-2"));
    assert!(b.of(&all, &a1).is_empty(), "the card in its test run is no finding");

    // without the limit the same card is QA's unused free slot, as before
    b.limit(&b.a, 0);
    assert_eq!(b.of(&b.check_at(t0 + 3 * 60_000), &a2), waiting("free_slot"));
}

#[test]
fn the_hour_counts_from_the_later_of_the_cards_last_change_and_the_start_of_the_test_run_it_waits_for() {
    let b = board_();
    b.limit(&b.a, 1);
    let a1 = b.card(&b.a, "Testing");
    let a2 = b.card(&b.a, "Testing");
    let r = b.run(&b.qa, &a1);
    // A1's run started half an hour after A2 last changed (another run on it before ended then)
    let t0 = ids::now_ms().max(b.changed(&a2));
    b.db.read(|c| Ok(c.execute("UPDATE runs SET started_at = ?2 WHERE id = ?1", rusqlite::params![r, t0 + 30 * 60_000])?)).unwrap();
    assert!(b.of(&b.check_at(t0 + HOUR + 60_000), &a2).is_empty(), "only 31 minutes since that run started");
    let all = b.check_at(t0 + 90 * 60_000 + 60_000);
    assert_eq!(b.of(&all, &a2), waiting("test_runs_full"));
    assert_eq!(all.iter().find(|f| f.task_id == a2).unwrap().since, t0 + 30 * 60_000, "since that run started");
}

#[test]
fn runs_on_other_columns_or_projects_never_hold_a_card_up_and_runs_at_once_full_comes_first() {
    let b = board_();
    b.limit(&b.a, 1);
    let a2 = b.card(&b.a, "Testing");
    // A's cards in In progress and B's card in Testing are at work: they don't fill A's Test runs at once
    let prog = b.card(&b.a, "In progress");
    b.run(&b.be, &prog);
    let b1 = b.card(&b.b, "Testing");
    b.run(&b.qa, &b1);
    let t0 = ids::now_ms().max(b.changed(&a2));
    assert_eq!(b.of(&b.check_at(t0 + 3 * 60_000), &a2), waiting("free_slot"), "QA's free slot, as before");
    assert_eq!(b.of(&b.check_at(t0 + 2 * HOUR), &a2), waiting("free_slot"));
    // a card of A in To do never waits for A's test runs
    let a1 = b.card(&b.a, "Testing");
    b.run(&b.qa, &a1);
    let todo = b.card(&b.a, "To do");
    let t1 = ids::now_ms().max(b.changed(&todo));
    assert_eq!(b.of(&b.check_at(t1 + 2 * HOUR), &todo), waiting("free_slot"), "the Backend Agent's free slot");
    assert_eq!(b.of(&b.check_at(t1 + 2 * HOUR), &a2), waiting("test_runs_full"), "A2 waits for A1 now");
    // "Runs at once" full is the finding first, like the wait for memory: they hold every card
    let mut cx = b.cx_at(t1 + 2 * HOUR);
    cx.max_concurrent = 3;
    assert_eq!(b.of(&board::check(&b.db, &cx).unwrap(), &a2), waiting("runs_full"));
    let mut cx = b.cx_at(t1 + 2 * HOUR);
    cx.memory_wait = Some((t1, "Waiting for memory: agent runs use 8.5 of 10 GB (Settings → Runs)".into()));
    assert_eq!(b.of(&board::check(&b.db, &cx).unwrap(), &a2), waiting("memory"));
}
