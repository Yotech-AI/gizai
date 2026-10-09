use gizai_core::db::{self, Db};
use gizai_core::{seed, users};
use rusqlite_migration::{M, Migrations};

/// A database file as the previous version left it: the migrations up to one schema step back (built up rather than
/// rolled back, because 0011, GA-49, links columns with a foreign key, so it can't be undone by dropping columns),
/// with one person in it.
fn previous_version(path: &std::path::Path) {
    let all = [
        include_str!("../migrations/0001_init.sql"), include_str!("../migrations/0002_agents.sql"), include_str!("../migrations/0003_chat.sql"),
        include_str!("../migrations/0004_effort.sql"), include_str!("../migrations/0005_pull_requests.sql"),
        include_str!("../migrations/0006_worktree_prepare.sql"), include_str!("../migrations/0007_card_flow.sql"),
        include_str!("../migrations/0008_board_check.sql"), include_str!("../migrations/0009_agent_folders.sql"),
        include_str!("../migrations/0010_run_refusals.sql"), include_str!("../migrations/0011_column_agents.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 1, "one schema step back");
    let mut c = rusqlite::Connection::open(path).unwrap();
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.execute_batch("INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
                     INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active');").unwrap();
}

fn people(path: &std::path::Path) -> Vec<String> {
    let c = rusqlite::Connection::open(path).unwrap();
    let mut st = c.prepare("SELECT name FROM actors WHERE kind='person' ORDER BY name").unwrap();
    st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()
}

#[test]
fn a_snapshot_is_a_complete_copy_taken_while_the_database_is_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let live = Db::open(&path).unwrap();
    let s = seed::ensure_seed(&live, "Jeffrey").unwrap();
    users::create(&live, &s.you_id, "Sanne Bakker", None).unwrap();
    let snap = db::snapshot(&path, &dir.path().join("backups"), "manual").unwrap();
    assert!(snap.file_name().unwrap().to_string_lossy().starts_with("gizai-manual-"), "{snap:?}");
    assert_eq!(people(&snap), ["Jeffrey", "Sanne Bakker"]);
    drop(live);
}

#[test]
fn only_the_newest_twenty_snapshots_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    seed::ensure_seed(&Db::open(&path).unwrap(), "Jeffrey").unwrap();
    let backups = dir.path().join("backups");
    for _ in 0..23 {
        db::snapshot(&path, &backups, "manual").unwrap();
    }
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 20);
}

#[test]
fn opening_an_older_database_snapshots_it_before_upgrading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    // Made by the previous version: one schema step back.
    previous_version(&path);
    let _db = Db::open(&path).unwrap();
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(snaps.len(), 1, "{snaps:?}");
    assert!(snaps[0].starts_with(&format!("gizai-before-v{}-", db::SCHEMA_VERSION)), "{snaps:?}");
    assert_eq!(people(&dir.path().join("backups").join(&snaps[0])), ["Jeffrey"], "the snapshot holds the old data");
    assert_eq!(people(&path), ["Jeffrey"], "and the upgrade kept it");
    // and a database that is already current gets no snapshot
    drop(_db);
    let _again = Db::open(&path).unwrap();
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
}
