//! GA-86 QA: the Team Lead merges pull requests, in core. Migration 0015 on a schema 14 database (the switch is there,
//! off, for every existing project), only a person sets Team Lead may merge, and what counts as a release under way.
use gizai_core::db::{self, Db};
use gizai_core::model::*;
use gizai_core::{projects, pulls, seed, tasks, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

/// A genuine schema 14 database file (migrations 0001..0014) with a person and two projects, as the previous release
/// left it.
fn v14_db(dir: &std::path::Path) -> std::path::PathBuf {
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
        include_str!("../migrations/0014_memory.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 1, "one schema step back");
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch("
        INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
        INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active');
        INSERT INTO projects (id, created_at, updated_at, org_id, number, key, name) VALUES ('p1', 1, 1, 'org', '2026-001', 'KADE', 'Kade'),
          ('p2', 2, 2, 'org', '2026-002', 'SHOP', 'Shop');
        INSERT INTO devices (id, created_at, name, is_self) VALUES ('dev', 1, 'test', 1);").unwrap();
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 14, "the fixture is schema 14");
    path
}

#[test]
fn migration_0015_adds_the_switch_off_for_every_existing_project_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = v14_db(dir.path());
    let db = Db::open(&path).unwrap();
    assert_eq!(db::SCHEMA_VERSION, 15);
    let (v, broken): (i64, i64) = db.read(|c| Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?))).unwrap();
    assert_eq!((v, broken), (15, 0), "schema 15, foreign keys intact");
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(snaps.len() == 1 && snaps[0].starts_with("gizai-before-v15-"), "{snaps:?}");
    // the column: NOT NULL, 0 by default, 0 for both old projects
    let (notnull, default): (i64, Option<String>) = db.read(|c| Ok(c.query_row(
        "SELECT \"notnull\", dflt_value FROM pragma_table_info('projects') WHERE name = 'lead_may_merge'", [], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!((notnull, default.as_deref()), (1, Some("0")));
    let mut all: Vec<(String, bool)> = projects::list(&db).unwrap().into_iter().map(|p| (p.key, p.lead_may_merge)).collect();
    all.sort();
    assert_eq!(all, [("KADE".to_string(), false), ("SHOP".to_string(), false)]);
    // the backup is the schema 14 database, without the column
    let old = Connection::open(dir.path().join("backups").join(&snaps[0])).unwrap();
    let (old_v, has): (i64, i64) = (old.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap(),
        old.query_row("SELECT count(*) FROM pragma_table_info('projects') WHERE name = 'lead_may_merge'", [], |r| r.get(0)).unwrap());
    assert_eq!((old_v, has), (14, 0));
    // a second open changes nothing and makes no new backup
    drop(db);
    let db = Db::open(&path).unwrap();
    assert!(projects::list(&db).unwrap().iter().all(|p| !p.lead_may_merge));
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
}

struct F {
    db: Db,
    you: String,
    team: String,
    project: String,
}

fn setup() -> F {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    F { you: s.you_id, team: s.team_id, project, db }
}

impl F {
    fn agent(&self, name: &str, role: &str) -> String {
        team::add_agent(&self.db, &self.you, &self.team, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap()
    }
    fn col(&self, category: &str) -> String {
        team::get(&self.db, &self.team).unwrap().states.into_iter().find(|s| s.category == category).unwrap().id
    }
    fn card(&self, project: &str, title: &str, category: &str, assignee: Option<&str>) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: project.into(), title: title.into(), state_id: Some(self.col(category)),
            assignee_id: assignee.map(Into::into), ..Default::default() }).unwrap()
    }
    fn input(&self, on: Option<bool>) -> ProjectInput {
        ProjectInput { name: "Kade portal".into(), key: "KADE".into(), lead_may_merge: on, ..Default::default() }
    }
    fn on(&self) -> bool {
        projects::get(&self.db, &self.project).unwrap().lead_may_merge
    }
}

#[test]
fn only_a_person_sets_team_lead_may_merge_and_other_changes_keep_it() {
    let f = setup();
    assert!(!f.on(), "off by default");
    let lead = f.agent("Team Lead", "lead");
    let be = f.agent("Backend Agent", "backend");
    for agent in [&lead, &be] {
        for on in [true, false] {
            let e = projects::update(&f.db, agent, &f.project, f.input(Some(on))).unwrap_err().to_string();
            assert!(e.contains("Team Lead may merge is switched only by a person") && e.contains("Nothing changed"), "{e}");
        }
        let e = projects::create(&f.db, agent, ProjectInput { name: "Shop".into(), key: "SHOP".into(), lead_may_merge: Some(false),
            ..Default::default() }).unwrap_err().to_string();
        assert!(e.contains("switched only by a person"), "{e}");
    }
    assert!(!f.on());
    assert!(projects::list(&f.db).unwrap().iter().all(|p| p.key != "SHOP"), "nothing made");
    // a refused write changes nothing else either
    assert_eq!(projects::get(&f.db, &f.project).unwrap().name, "Kade portal");
    // you switch it on; an agent's other changes (None) keep it on
    projects::update(&f.db, &f.you, &f.project, f.input(Some(true))).unwrap();
    assert!(f.on());
    projects::update(&f.db, &lead, &f.project, ProjectInput { name: "Kade portal 2".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let p = projects::get(&f.db, &f.project).unwrap();
    assert_eq!((p.name.as_str(), p.lead_may_merge), ("Kade portal 2", true));
    // and your own other changes keep it too
    projects::update(&f.db, &f.you, &f.project, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    assert!(f.on());
    projects::update(&f.db, &f.you, &f.project, f.input(Some(false))).unwrap();
    assert!(!f.on());
    // a new project of yours may start with it on
    let shop = projects::create(&f.db, &f.you, ProjectInput { name: "Shop".into(), key: "SHOP".into(), lead_may_merge: Some(true), ..Default::default() }).unwrap();
    assert!(projects::get(&f.db, &shop).unwrap().lead_may_merge);
}

#[test]
fn a_release_under_way_is_a_card_of_the_project_in_deploy_assigned_to_a_devops_agent() {
    let f = setup();
    assert_eq!(pulls::release_under_way(&f.db, &f.project).unwrap(), None);
    let ops = f.agent("DevOps Agent", "devops");
    let be = f.agent("Backend Agent", "backend");
    // a devops agent's card elsewhere than Deploy, a Deploy card of another agent or of you, or no one's: no release
    f.card(&f.project, "Set up CI", "ready", Some(&ops));
    f.card(&f.project, "Export invoices", "deploy", Some(&be));
    f.card(&f.project, "Import contacts", "deploy", Some(&f.you));
    f.card(&f.project, "Rename the page", "deploy", None);
    assert_eq!(pulls::release_under_way(&f.db, &f.project).unwrap(), None);
    // another project's release isn't this project's
    let shop = projects::create(&f.db, &f.you, ProjectInput { name: "Shop".into(), key: "SHOP".into(), ..Default::default() }).unwrap();
    let other = f.card(&shop, "Release Shop v2.0.0", "deploy", Some(&ops));
    assert_eq!(pulls::release_under_way(&f.db, &f.project).unwrap(), None);
    assert_eq!(pulls::release_under_way(&f.db, &shop).unwrap(), Some(("SHOP-1".into(), "DevOps Agent".into())));
    // this project's release card
    let release = f.card(&f.project, "Release Kade v1.2.0", "deploy", Some(&ops));
    let id = tasks::get(&f.db, &release).unwrap().identifier;
    assert_eq!(pulls::release_under_way(&f.db, &f.project).unwrap(), Some((id, "DevOps Agent".into())));
    // released (Done), and Done then archived: none any more
    tasks::move_to(&f.db, &f.you, &release, &f.col("done"), "").unwrap();
    assert_eq!(pulls::release_under_way(&f.db, &f.project).unwrap(), None);
    tasks::move_to(&f.db, &f.you, &other, &f.col("done"), "").unwrap();
    tasks::archive(&f.db, &f.you, &other).unwrap();
    assert_eq!(pulls::release_under_way(&f.db, &shop).unwrap(), None);
}
