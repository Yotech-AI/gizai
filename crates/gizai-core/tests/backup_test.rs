use gizai_core::db::{self, Db};
use gizai_core::{seed, users};

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
    seed::ensure_seed(&Db::open(&path).unwrap(), "Jeffrey").unwrap();
    // Pretend it was made by the previous version: one schema step back (0009 added the agents' folders).
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch(&format!("ALTER TABLE agent_configs DROP COLUMN folders_json; PRAGMA user_version = {};", db::SCHEMA_VERSION - 1)).unwrap();
    drop(c);
    let _db = Db::open(&path).unwrap();
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(snaps.len(), 1, "{snaps:?}");
    assert!(snaps[0].starts_with(&format!("gizai-before-v{}-", db::SCHEMA_VERSION)), "{snaps:?}");
    // and a database that is already current gets no snapshot
    drop(_db);
    let _again = Db::open(&path).unwrap();
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
}
