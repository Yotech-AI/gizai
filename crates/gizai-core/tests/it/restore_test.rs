//! GA-218 QA: restore in the core (`gizai_core::restore`, `scope`). The checks and what each refuses (a newer format or
//! schema, a missing or changed database file, a failed integrity check, a database of another version than its manifest
//! says, a missing or changed blob), with nothing staged after a refusal; the staging; and the swap at the next start
//! (`restore::open`): the snapshot's rows and blobs, today's database in `backups/`, the device rule for this computer's
//! own snapshot and another computer's, this computer's settings against shared ones, the agents paused, `last_restore`,
//! an older schema upgraded, a restored database that can't be opened or can't take this computer's settings, cancel,
//! and a start with nothing staged.
use std::fs;
use std::path::{Path, PathBuf};

use gizai_core::backups::{self as b, Manifest, Reason};
use gizai_core::db::{self, Db};
use gizai_core::restore::{self, RestoreResult, Saved, Staged, Step};
use gizai_core::{files, ids, scope, seed, settings};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

// ---- set-up ----

/// A Gizai data folder with its database open, as the app has it.
struct Data {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    dir: PathBuf,
    db: Db,
    org: String,
}

impl Data {
    fn device(&self) -> String {
        self.db.device_id().to_string()
    }

    /// A stored file with these bytes, attached to you.
    fn attach(&self, name: &str, bytes: &[u8]) -> String {
        let you = seed::ensure_seed(&self.db, "Jeffrey").unwrap().you_id;
        let blob = files::store_bytes(&self.dir, name, bytes).unwrap();
        self.db.write(Some(&you), |w| files::insert(w, Some(&you), "actor", &you, &blob)).unwrap();
        blob.sha256
    }

    /// A snapshot (GA-216's format 1) in chosen folder `chosen`.
    fn snapshot(&self, chosen: &Path, reason: Reason) -> Manifest {
        b::make(&self.dir, self.db.device_id(), chosen, reason, 10, &mut |_| Ok(())).unwrap().manifest
    }

    /// The hashes the `files` rows name, sorted.
    fn files_rows(&self) -> Vec<String> {
        self.db.read(|c| {
            let mut st = c.prepare("SELECT sha256 FROM files ORDER BY sha256")?;
            let rows = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
            Ok(rows)
        }).unwrap()
    }

    fn set(&self, key: &str, value: Value) {
        settings::set(&self.db, key, &value).unwrap();
    }

    fn get(&self, key: &str) -> Option<Value> {
        settings::get::<Value>(&self.db, key).unwrap()
    }

    /// A setting of an organisation (`org_id` not '').
    fn set_org(&self, org: &str, key: &str, value: Value) {
        self.db.write(None, |w| {
            w.conn().execute("INSERT OR REPLACE INTO settings(key, org_id, value_json, updated_at) VALUES (?1, ?2, ?3, 1)",
                             params![key, org, value.to_string()])?;
            Ok(())
        }).unwrap();
    }

    fn get_org(&self, org: &str, key: &str) -> Option<Value> {
        self.db.read(|c| {
            let mut st = c.prepare("SELECT value_json FROM settings WHERE key = ?1 AND org_id = ?2")?;
            let v = st.query_map(params![key, org], |r| r.get::<_, String>(0))?.next().transpose()?;
            Ok(v)
        }).unwrap().map(|s| serde_json::from_str(&s).unwrap())
    }

    /// Every `devices` row: (id, name, created_at, is_self), by id.
    fn devices(&self) -> Vec<(String, String, i64, i64)> {
        self.db.read(|c| {
            let mut st = c.prepare("SELECT id, name, created_at, is_self FROM devices ORDER BY id")?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        }).unwrap()
    }

    /// Gizai quits and starts again on the same data folder (`open_data` calls `restore::open`).
    fn restart(self) -> (Data, Option<RestoreResult>) {
        let Data { _tmp, root, dir, db, org } = self;
        drop(db);
        let (db, result) = restore::open(&dir).unwrap();
        (Data { _tmp, root, dir, db, org }, result)
    }
}

fn data() -> Data {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let dir = root.join("data");
    fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join("gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    Data { _tmp: tmp, root, dir, db, org: s.org_id }
}

/// An empty folder the user chose (a USB disk).
fn chosen(d: &Data, name: &str) -> PathBuf {
    let p = d.root.join(name);
    fs::create_dir_all(&p).unwrap();
    p
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Every file under `dir`, relative to it with `/`, sorted.
fn tree(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = vec![];
    if dir.exists() {
        walk(dir, dir, &mut out);
    }
    out.sort();
    out
}

/// What the app does before the restart: check snapshot `name` in chosen folder `from`, save today's data (a
/// before-restore snapshot in `save_to`), and stage it. Returns the staged restore and the before-restore snapshot.
fn stage(d: &Data, from: &Path, name: &str, save_to: &Path) -> (Staged, Manifest) {
    let dir = b::backups_dir(from);
    let m = restore::check(&d.dir, &dir, name, &mut |_| Ok(())).unwrap();
    let saved = d.snapshot(save_to, Reason::BeforeRestore);
    let s = Saved { name: saved.name.clone(), made_at: saved.made_at, folder: b::backups_dir(save_to).to_string_lossy().into_owned() };
    let staged = restore::stage(&d.dir, &dir, &m, &s, &mut |_| Ok(())).unwrap();
    (staged, saved)
}

/// Writes a file a snapshot made read-only: removed first.
fn replace(path: &Path, bytes: &[u8]) {
    let _ = fs::remove_file(path);
    fs::write(path, bytes).unwrap();
}

fn edit_manifest(dir: &Path, name: &str, f: impl FnOnce(&mut Value)) {
    let p = dir.join("snapshots").join(format!("{name}.json"));
    let mut v: Value = serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    f(&mut v);
    replace(&p, &serde_json::to_vec_pretty(&v).unwrap());
}

/// Changes snapshot `name`'s database (backups folder `dir`) with `f`, compresses it again and makes its manifest match
/// (`db.sha256`, `db.bytes`, `db.db_bytes`, and `schema_version` as its `user_version`): it passes the sha256 check.
fn repack(dir: &Path, name: &str, f: impl FnOnce(&Connection)) {
    let tmp = dir.join(format!("{name}.qa.db"));
    b::unpack_db(dir, name, &tmp).unwrap();
    let version: i64 = {
        let c = Connection::open(&tmp).unwrap();
        f(&c);
        c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
    };
    let raw = fs::read(&tmp).unwrap();
    fs::remove_file(&tmp).unwrap();
    let zst = zstd::encode_all(&raw[..], 3).unwrap();
    replace(&dir.join("snapshots").join(format!("{name}.db.zst")), &zst);
    edit_manifest(dir, name, |v| {
        v["db"]["sha256"] = json!(sha_hex(&raw));
        v["db"]["bytes"] = json!(zst.len());
        v["db"]["db_bytes"] = json!(raw.len());
        v["schema_version"] = json!(version);
    });
}

fn assert_nothing_staged(d: &Data) {
    assert!(restore::staged(&d.dir).is_none(), "nothing is staged");
    let left = tree(&restore::dir(&d.dir));
    assert!(left.is_empty(), "nothing is left in restore/: {left:?}");
}

/// The check of snapshot `name` in chosen folder `from`, refused: its reason. Nothing is staged after it.
fn refused(d: &Data, from: &Path, name: &str) -> String {
    let e = restore::check(&d.dir, &b::backups_dir(from), name, &mut |_| Ok(())).expect_err("the check refuses it");
    assert_nothing_staged(d);
    e.to_string()
}

/// The files in the data folder's `backups/` whose name starts with `start`.
fn backups_named(d: &Data, start: &str) -> Vec<String> {
    tree(&d.dir.join("backups")).into_iter().filter(|f| f.starts_with(start)).collect()
}

fn is_stamped(name: &str, start: &str) -> bool {
    // gizai-before-restore-20261027-142233-123.db (`db::stamp`)
    let Some(stamp) = name.strip_prefix(start).and_then(|s| s.strip_suffix(".db")) else { return false };
    let b = stamp.as_bytes();
    b.len() == 19 && b[8] == b'-' && b[15] == b'-' && b.iter().enumerate().all(|(i, c)| [8, 15].contains(&i) || c.is_ascii_digit())
}

// ---- this computer's settings (scope.rs) ----

#[test]
fn this_computers_settings_are_the_cards_list_and_every_other_and_every_orgs_setting_is_shared() {
    for key in ["claude_bin", "gh_bin", "clis", "mcp_servers", "mcp_tools", "mcp_last_run", "mcp_browser", "cli_tools_seen",
                "cli_tools_asked", "editor_roots", "github_push_over", "claude_memory_imports", "runs_memory_gb",
                "max_concurrent_runs", "agents_paused", "subscription_limits", "onboarding", "update_auto_check",
                "update_last_check", "notify_hold", "notify_waiting", "notify_lead_asks", "notify_lead_answered",
                "notify_something_new", b::FOLDER_KEY, b::EVERY_KEY, b::ON_QUIT_KEY, b::KEEP_KEY, "last_restore"] {
        assert!(scope::this_computer(key), "{key} is this computer's");
        assert!(!scope::shared(key, ""), "app-wide {key} isn't shared");
        assert!(scope::shared(key, "org-1"), "an org's {key} is shared");
    }
    assert_eq!(scope::LAST_RESTORE, "last_restore");
    for key in ["max_run_minutes", "max_run_usd", "max_chat_minutes", "memory_on", "ask_lead_first", "learned_by_project",
                "screenshots_per_run", "seeded", "a_setting_nobody_listed", "notify", "xnotify_hold"] {
        assert!(!scope::this_computer(key), "{key} is shared");
        assert!(scope::shared(key, ""), "{key} is shared");
    }
}

// ---- the steps ----

#[test]
fn the_check_and_the_staging_say_each_step_in_order() {
    let d = data();
    let (a, bb) = (d.attach("a.txt", b"first file"), d.attach("b.txt", b"second file"));
    let usb = chosen(&d, "usb");
    let m = d.snapshot(&usb, Reason::Manual);
    // the data folder lacks one of the two: only that one is copied
    let lacking = files::blob_path(&d.dir, &bb);
    fs::remove_file(&lacking).unwrap();
    let dir = b::backups_dir(&usb);
    let mut heard = vec![];
    let checked = restore::check(&d.dir, &dir, &m.name, &mut |s| { heard.push(s); Ok(()) }).unwrap();
    assert_eq!(checked.name, m.name);
    assert_eq!(heard, [Step::Snapshot, Step::Database, Step::CheckFiles(0, 2), Step::CheckFiles(1, 2), Step::CheckFiles(2, 2)]);
    let mut heard = vec![];
    let saved = Saved { name: "gizai-before-restore-x".into(), made_at: 1, folder: dir.to_string_lossy().into_owned() };
    restore::stage(&d.dir, &dir, &checked, &saved, &mut |s| { heard.push(s); Ok(()) }).unwrap();
    assert_eq!(heard, [Step::CopyDatabase, Step::CopyFiles(0, 1), Step::CopyFiles(1, 1)]);
    // the staged folder: the database, the one blob the data folder lacks, and restore.json
    let staged = tree(&restore::dir(&d.dir));
    assert_eq!(staged, vec![format!("files/{}/{}", &bb[..2], bb), "gizai.db".to_string(), "restore.json".to_string()], "{a}");
    // the window's words for them
    assert_eq!(Step::Database.text(), "Checking the database");
    assert_eq!(Step::CopyFiles(120, 300).text(), "Copying files (120 of 300)");
}

#[test]
fn a_step_that_stops_the_check_or_the_staging_leaves_nothing_staged() {
    let d = data();
    let sha = d.attach("a.txt", b"first file");
    let usb = chosen(&d, "usb");
    let m = d.snapshot(&usb, Reason::Manual);
    fs::remove_file(files::blob_path(&d.dir, &sha)).unwrap();
    let dir = b::backups_dir(&usb);
    let stop = |s: Step| if matches!(s, Step::CheckFiles(..) | Step::CopyFiles(..)) { Err(gizai_core::Error::Invalid("Cancelled".into())) } else { Ok(()) };
    let e = restore::check(&d.dir, &dir, &m.name, &mut |s| stop(s)).unwrap_err();
    assert_eq!(e.to_string(), "Cancelled");
    assert_nothing_staged(&d);
    let checked = restore::check(&d.dir, &dir, &m.name, &mut |_| Ok(())).unwrap();
    let saved = Saved { name: "x".into(), made_at: 1, folder: String::new() };
    let e = restore::stage(&d.dir, &dir, &checked, &saved, &mut |s| stop(s)).unwrap_err();
    assert_eq!(e.to_string(), "Cancelled");
    assert_nothing_staged(&d);
}

// ---- the checks refuse ----

#[test]
fn a_snapshot_of_a_newer_format_or_schema_is_refused_and_says_to_update_first() {
    let d = data();
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    edit_manifest(&dir, &m.name, |v| v["schema_version"] = json!(db::SCHEMA_VERSION + 1));
    assert_eq!(refused(&d, &usb, &m.name), format!("Made by a newer Gizai (database version {}). Update Gizai first.", db::SCHEMA_VERSION + 1));
    edit_manifest(&dir, &m.name, |v| { v["schema_version"] = json!(db::SCHEMA_VERSION); v["format_version"] = json!(2); });
    let why = refused(&d, &usb, &m.name);
    assert!(why.starts_with("Made by a newer Gizai") && why.ends_with("Update Gizai first."), "{why}");
    // format 1 and this schema pass
    edit_manifest(&dir, &m.name, |v| v["format_version"] = json!(1));
    restore::check(&d.dir, &dir, &m.name, &mut |_| Ok(())).unwrap();
    restore::remove_checked(&d.dir);
    // not a snapshot's manifest, and no such snapshot
    edit_manifest(&dir, &m.name, |v| v["format"] = json!("something-else"));
    assert!(refused(&d, &usb, &m.name).contains("isn't a snapshot's manifest"));
    assert!(refused(&d, &usb, "gizai-manual-19990101-000000-000-aaaaaaaa").contains("There is no snapshot"));
    assert!(refused(&d, &usb, "../gizai").contains("isn't a snapshot's name"));
}

#[test]
fn a_missing_or_changed_database_file_is_refused() {
    let d = data();
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    let zst = dir.join("snapshots").join(format!("{}.db.zst", m.name));
    let good = fs::read(&zst).unwrap();
    fs::remove_file(&zst).unwrap();
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("The snapshot's database file is missing"), "{why}");
    // another database in its place (zstd, but not the one the manifest names)
    replace(&zst, &zstd::encode_all(&b"SQLite format 3\0 but another one"[..], 3).unwrap());
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("has changed since the snapshot was made"), "{why}");
    // a file that isn't zstd at all
    replace(&zst, b"not a zstd file");
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("The snapshot's database file"), "{why}");
    // the snapshot's own file passes again
    replace(&zst, &good);
    restore::check(&d.dir, &dir, &m.name, &mut |_| Ok(())).unwrap();
}

#[test]
fn a_database_that_fails_its_integrity_check_or_isnt_the_manifests_version_is_refused() {
    let d = data();
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    // its version isn't the manifest's
    edit_manifest(&dir, &m.name, |v| v["schema_version"] = json!(db::SCHEMA_VERSION - 1));
    assert_eq!(refused(&d, &usb, &m.name),
               format!("The snapshot's database is version {}, not {} as its manifest says", db::SCHEMA_VERSION, db::SCHEMA_VERSION - 1));
    // damaged: two indexes swap their trees, so rows are missing from them (its sha256 matches the manifest)
    repack(&dir, &m.name, |c| {
        c.execute_batch("CREATE TABLE qa_t(a INTEGER, b TEXT); CREATE INDEX qa_a ON qa_t(a); CREATE INDEX qa_b ON qa_t(b);
                         INSERT INTO qa_t VALUES (1, 'z'), (2, 'y'), (3, 'x');").unwrap();
        let root = |n: &str| c.query_row("SELECT rootpage FROM sqlite_schema WHERE name = ?1", [n], |r| r.get::<_, i64>(0)).unwrap();
        let (ra, rb) = (root("qa_a"), root("qa_b"));
        c.execute_batch("PRAGMA writable_schema = ON").unwrap();
        c.execute("UPDATE sqlite_schema SET rootpage = ?1 WHERE name = 'qa_a'", [rb]).unwrap();
        c.execute("UPDATE sqlite_schema SET rootpage = ?1 WHERE name = 'qa_b'", [ra]).unwrap();
        c.execute_batch("PRAGMA writable_schema = OFF").unwrap();
    });
    let why = refused(&d, &usb, &m.name);
    assert!(why.starts_with("The snapshot's database is damaged"), "{why}");
}

#[test]
fn a_missing_or_changed_blob_is_refused() {
    let d = data();
    let sha = d.attach("a.txt", b"the first file");
    let other = d.attach("b.txt", b"the second file");
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    let blob = files::blob_path(&dir, &sha);
    let good = fs::read(&blob).unwrap();
    // missing
    fs::remove_file(&blob).unwrap();
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("1 file of the snapshot is missing from"), "{why}");
    fs::remove_file(files::blob_path(&dir, &other)).unwrap();
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("2 files of the snapshot are missing from"), "{why}");
    fs::write(files::blob_path(&dir, &other), b"the second file").unwrap();
    // changed, the same size
    let mut same_size = good.clone();
    same_size[0] ^= 1;
    fs::write(&blob, &same_size).unwrap();
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("has changed since the snapshot was made"), "{why}");
    // changed, another size
    replace(&blob, b"longer than the file it replaces");
    let why = refused(&d, &usb, &m.name);
    assert!(why.contains("has changed since the snapshot was made"), "{why}");
    // the snapshot's own blob passes again
    replace(&blob, &good);
    restore::check(&d.dir, &dir, &m.name, &mut |_| Ok(())).unwrap();
}

// ---- the restore at the next start ----

#[test]
fn a_snapshot_of_this_computer_restores_its_rows_and_blobs_and_todays_database_goes_to_backups() {
    let d = data();
    let a = d.attach("a.txt", b"in the snapshot");
    d.set("qa_shared", json!("snapshot"));
    let usb = chosen(&d, "usb");
    let m = d.snapshot(&usb, Reason::Manual);
    let devices = d.devices();
    // today, after the snapshot: another file, a shared setting changed, and the snapshot's blob gone from files/
    let today = d.attach("b.txt", b"only today");
    d.set("qa_shared", json!("today"));
    fs::remove_file(files::blob_path(&d.dir, &a)).unwrap();
    let (staged, saved) = stage(&d, &usb, &m.name, &usb);
    assert_eq!(staged.state, "ready");
    assert_eq!(saved.reason, "before-restore");
    assert!(restore::staged(&d.dir).is_some());
    // today's data keeps working until the restart
    let mut both = vec![a.clone(), today.clone()];
    both.sort();
    assert_eq!(d.files_rows(), both);
    assert!(!files::blob_path(&d.dir, &a).exists(), "nothing goes into files/ before the restart");

    let started = ids::now_ms();
    let (d, result) = d.restart();
    let result = result.expect("a result");
    assert!(result.ok, "{result:?}");
    assert_eq!(result.snapshot, m.name);
    assert!(result.at >= started);
    assert!(result.message.contains(&m.name) && result.message.contains(&saved.name), "{}", result.message);
    // the snapshot's rows and blobs
    assert_eq!(d.files_rows(), vec![a.clone()]);
    assert_eq!(fs::read(files::blob_path(&d.dir, &a)).unwrap(), b"in the snapshot");
    assert_eq!(d.get("qa_shared"), Some(json!("snapshot")));
    // this computer's own snapshot leaves devices as it was
    assert_eq!(d.devices(), devices);
    // today's database is backups/gizai-before-restore-<stamp>.db, with today's rows
    let before = backups_named(&d, "gizai-before-restore-");
    assert_eq!(before.len(), 1, "{before:?}");
    assert!(is_stamped(&before[0], "gizai-before-restore-"), "{before:?}");
    let old = Connection::open(d.dir.join("backups").join(&before[0])).unwrap();
    let n: i64 = old.query_row("SELECT count(*) FROM files WHERE sha256 = ?1", [&today], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "today's rows are in the copy");
    // restore/ is gone, and the result waits for the window until it is dismissed
    assert!(!restore::dir(&d.dir).exists());
    assert_eq!(restore::result(&d.db), Some(result));
    restore::dismiss_result(&d.db).unwrap();
    assert_eq!(restore::result(&d.db), None);
}

#[test]
fn a_restore_is_applied_once_and_a_start_with_nothing_staged_is_a_normal_one() {
    let d = data();
    let usb = chosen(&d, "usb");
    let m = d.snapshot(&usb, Reason::Manual);
    // nothing staged: a normal start, and no restore/ made
    let (d, result) = d.restart();
    assert_eq!(result, None);
    assert!(!restore::dir(&d.dir).exists());
    stage(&d, &usb, &m.name, &usb);
    let (d, result) = d.restart();
    assert!(result.unwrap().ok);
    let later = d.attach("c.txt", b"after the restore");
    let (d, result) = d.restart();
    assert_eq!(result, None, "the next start is a normal one");
    assert_eq!(d.files_rows(), vec![later]);
    assert_eq!(backups_named(&d, "gizai-before-restore-").len(), 1);
    // an earlier failure's restore/failed/ alone doesn't make a start restore anything
    fs::create_dir_all(restore::dir(&d.dir).join(restore::FAILED)).unwrap();
    let (d, result) = d.restart();
    assert_eq!(result, None);
    assert!(restore::dir(&d.dir).join(restore::FAILED).is_dir());
}

#[test]
fn another_computers_snapshot_keeps_this_computers_device_and_new_changes_carry_its_id() {
    let o = data();
    o.attach("theirs.txt", b"made on the other computer");
    let theirs = chosen(&o, "usb");
    let m = o.snapshot(&theirs, Reason::Manual);
    let o_devices = o.devices();
    assert_eq!(o_devices.len(), 1);
    assert_eq!(m.device.id, o.device());

    let d = data();
    let mine = d.devices();
    assert_eq!(mine.len(), 1);
    let me = d.device();
    assert_ne!(me, m.device.id);
    let own = chosen(&d, "mine");
    stage(&d, &theirs, &m.name, &own);
    let (d, result) = d.restart();
    let result = result.unwrap();
    assert!(result.ok, "{result:?}");
    assert_eq!(result.from_computer, m.device.name);
    // this computer's id with is_self = 1, the snapshot's computer with is_self = 0
    assert_eq!(d.device(), me);
    let mut want = vec![(mine[0].0.clone(), mine[0].1.clone(), mine[0].2, 1), (o_devices[0].0.clone(), o_devices[0].1.clone(), o_devices[0].2, 0)];
    want.sort();
    assert_eq!(d.devices(), want);
    // the snapshot's changes keep their computer; a new change carries this computer's id
    let old: Vec<String> = d.db.read(|c| {
        let mut st = c.prepare("SELECT DISTINCT device_id FROM changes")?;
        let v = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(v)
    }).unwrap();
    assert_eq!(old, vec![m.device.id.clone()]);
    d.db.write(None, |w| w.insert("qa_rows", "row-1", json!({"title": ["", "new"]}))).unwrap();
    let (device, hlc): (String, String) = d.db.read(|c| {
        Ok(c.query_row("SELECT device_id, hlc FROM changes ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
    }).unwrap();
    assert_eq!(device, me);
    assert!(hlc.ends_with(&me[..8]), "{hlc}");
}

#[test]
fn this_computers_settings_keep_todays_values_and_shared_ones_come_from_the_snapshot() {
    let o = data();
    o.set("claude_bin", json!("/snap/bin/claude"));
    o.set(b::FOLDER_KEY, json!("/snap/usb"));
    o.set("gh_bin", json!("/snap/bin/gh"));
    o.set("notify_hold", json!(true));
    o.set("agents_paused", json!(false));
    o.set("max_run_minutes", json!(45));
    o.set("qa_shared", json!("snapshot"));
    o.set_org(&o.org, "claude_bin", json!("org from the snapshot"));
    let theirs = chosen(&o, "usb");
    let m = o.snapshot(&theirs, Reason::Manual);
    assert!(m.last_change.is_some());

    let d = data();
    d.set("claude_bin", json!("/today/bin/claude"));
    d.set(b::FOLDER_KEY, json!("/today/usb"));
    d.set("notify_hold", json!(false));
    d.set("editor_roots", json!(["/today/code"]));
    d.set("agents_paused", json!(false));
    d.set("max_run_minutes", json!(90));
    d.set("qa_shared", json!("today"));
    d.set("qa_shared_only_today", json!(1));
    d.set_org(&d.org, "claude_bin", json!("org of today"));
    let own = chosen(&d, "mine");
    let (_, saved) = stage(&d, &theirs, &m.name, &own);
    let (d, result) = d.restart();
    let result = result.unwrap();
    assert!(result.ok, "{result:?}");
    // this computer's keep today's values; one today didn't have comes from the snapshot
    assert_eq!(d.get("claude_bin"), Some(json!("/today/bin/claude")));
    assert_eq!(d.get(b::FOLDER_KEY), Some(json!("/today/usb")));
    assert_eq!(d.get("notify_hold"), Some(json!(false)));
    assert_eq!(d.get("editor_roots"), Some(json!(["/today/code"])));
    assert_eq!(d.get("gh_bin"), Some(json!("/snap/bin/gh")));
    // shared ones, and every org's, come from the snapshot
    assert_eq!(d.get("max_run_minutes"), Some(json!(45)));
    assert_eq!(d.get("qa_shared"), Some(json!("snapshot")));
    assert_eq!(d.get("qa_shared_only_today"), None);
    assert_eq!(d.get_org(&o.org, "claude_bin"), Some(json!("org from the snapshot")));
    assert_eq!(d.get_org(&d.org, "claude_bin"), None);
    // the agents are paused, and last_restore says where the data came back from
    assert_eq!(d.get("agents_paused"), Some(json!(true)));
    let last = d.get("last_restore").expect("last_restore");
    assert_eq!(last["snapshot"], json!(m.name));
    assert_eq!(last["made_by"], json!(o.device()));
    assert_eq!(last["made_at"], json!(m.made_at));
    assert_eq!(last["last_change"], serde_json::to_value(&m.last_change).unwrap());
    assert_eq!(last["saved_today"], json!(saved.name));
    assert!(last["restored_at"].as_i64().unwrap() >= saved.made_at, "{last}");
    let typed = restore::last_restore(&d.db).unwrap();
    assert_eq!((typed.snapshot.as_str(), typed.made_by.as_str()), (m.name.as_str(), o.device().as_str()));
    // the copy of today's database holds today's values
    let before = backups_named(&d, "gizai-before-restore-");
    let old = Connection::open(d.dir.join("backups").join(&before[0])).unwrap();
    let v: String = old.query_row("SELECT value_json FROM settings WHERE key = 'qa_shared' AND org_id = ''", [], |r| r.get(0)).unwrap();
    assert_eq!(v, "\"today\"");
}

#[test]
fn a_snapshot_with_an_older_schema_is_upgraded_at_the_start_with_the_upgrades_copy_in_backups() {
    let d = data();
    let a = d.attach("a.txt", b"in the snapshot");
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    // the database as the Gizai before migration 21 made it
    repack(&dir, &m.name, |c| {
        c.execute_batch(&format!("DROP TABLE task_builds_on; PRAGMA user_version = {};", db::SCHEMA_VERSION - 1)).unwrap();
    });
    let older = b::read_manifest(&dir, &m.name).unwrap();
    assert_eq!(older.schema_version, db::SCHEMA_VERSION - 1);
    stage(&d, &usb, &m.name, &usb);
    let (d, result) = d.restart();
    assert!(result.unwrap().ok);
    assert_eq!(d.files_rows(), vec![a]);
    let (version, table): (i64, i64) = d.db.read(|c| {
        Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
            c.query_row("SELECT count(*) FROM sqlite_schema WHERE name = 'task_builds_on'", [], |r| r.get(0))?))
    }).unwrap();
    assert_eq!((version, table), (db::SCHEMA_VERSION, 1), "upgraded");
    let copies = backups_named(&d, &format!("gizai-before-v{}-", db::SCHEMA_VERSION));
    assert_eq!(copies.len(), 1, "Db::open's copy before the upgrade: {:?}", tree(&d.dir.join("backups")));
    let copy = Connection::open(d.dir.join("backups").join(&copies[0])).unwrap();
    assert_eq!(copy.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), db::SCHEMA_VERSION - 1);
    assert_eq!(backups_named(&d, "gizai-before-restore-").len(), 1);
}

#[test]
fn a_restored_database_that_cant_be_opened_goes_to_restore_failed_and_gizai_starts_with_todays_data() {
    let d = data();
    d.attach("a.txt", b"in the snapshot");
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    // it says the version before 21 but has 21's table: the upgrade fails
    repack(&dir, &m.name, |c| c.execute_batch(&format!("PRAGMA user_version = {};", db::SCHEMA_VERSION - 1)).unwrap());
    d.attach("b.txt", b"only today");
    d.set("agents_paused", json!(false));
    let rows = d.files_rows();
    let devices = d.devices();
    stage(&d, &usb, &m.name, &usb);
    let (d, result) = d.restart();
    let result = result.expect("a result");
    assert!(!result.ok);
    assert_eq!(result.snapshot, m.name);
    assert!(result.message.contains(&format!("couldn't open the restored data of {}, so it started with today's data", m.name)), "{}", result.message);
    // today's data, as it was
    assert_eq!(d.files_rows(), rows);
    assert_eq!(d.devices(), devices);
    assert_eq!(d.get("agents_paused"), Some(json!(false)));
    assert_eq!(d.get("last_restore"), None);
    // the failed one is in restore/failed/, and nothing else stays in restore/
    let failed = restore::dir(&d.dir).join(restore::FAILED).join(format!("{}.db", m.name));
    assert!(failed.is_file(), "{:?}", tree(&restore::dir(&d.dir)));
    assert_eq!(tree(&restore::dir(&d.dir)), vec![format!("failed/{}.db", m.name)]);
    assert!(result.message.contains(&failed.display().to_string()), "{}", result.message);
    assert!(backups_named(&d, "gizai-before-restore-").is_empty(), "today's database came back from backups/");
    assert_eq!(restore::result(&d.db), Some(result));
    // the next start is a normal one
    let (d, again) = d.restart();
    assert_eq!(again, None);
    assert_eq!(d.files_rows(), rows);
}

#[test]
fn a_restored_database_that_cant_take_this_computers_settings_leaves_todays_data_whole() {
    let d = data();
    let usb = chosen(&d, "usb");
    let dir = b::backups_dir(&usb);
    let m = d.snapshot(&usb, Reason::Manual);
    repack(&dir, &m.name, |c| c.execute_batch("DROP TABLE settings;").unwrap());
    let today = d.attach("b.txt", b"only today");
    stage(&d, &usb, &m.name, &usb);
    let (d, result) = d.restart();
    let result = result.unwrap();
    assert!(!result.ok);
    assert!(result.message.contains("so it started with today's data"), "{}", result.message);
    assert_eq!(d.files_rows(), vec![today]);
    assert!(restore::dir(&d.dir).join(restore::FAILED).join(format!("{}.db", m.name)).is_file());
    assert!(restore::staged(&d.dir).is_none());
    assert!(backups_named(&d, "gizai-before-restore-").is_empty());
}

#[test]
fn a_start_after_a_swap_that_was_interrupted_carries_on_from_where_it_was() {
    let d = data();
    let a = d.attach("a.txt", b"in the snapshot");
    let usb = chosen(&d, "usb");
    let m = d.snapshot(&usb, Reason::Manual);
    let today = d.attach("b.txt", b"only today");
    let (staged, _) = stage(&d, &usb, &m.name, &usb);
    let Data { _tmp, root, dir, db, org } = d;
    drop(db);
    // the computer went off just after restore.json noted the name and today's database moved to backups/
    let before = "gizai-before-restore-20261027-120000-000.db";
    fs::create_dir_all(dir.join("backups")).unwrap();
    fs::rename(dir.join("gizai.db"), dir.join("backups").join(before)).unwrap();
    let s = Staged { swapping: Some(before.into()), ..staged };
    replace(&restore::dir(&dir).join(restore::STAGED), &serde_json::to_vec(&s).unwrap());
    let (db, result) = restore::open(&dir).unwrap();
    let d = Data { _tmp, root, dir, db, org };
    let result = result.unwrap();
    assert!(result.ok, "{result:?}");
    assert_eq!(d.files_rows(), vec![a.clone()]);
    assert!(files::blob_path(&d.dir, &a).is_file());
    assert_eq!(backups_named(&d, "gizai-before-restore-"), vec![before.to_string()]);
    let old = Connection::open(d.dir.join("backups").join(before)).unwrap();
    assert_eq!(old.query_row("SELECT count(*) FROM files WHERE sha256 = ?1", [&today], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert!(!restore::dir(&d.dir).exists());
    // and it is applied once
    let (_, again) = d.restart();
    assert_eq!(again, None);
}

#[test]
fn after_cancel_nothing_is_staged_the_before_restore_snapshot_stays_and_the_next_start_is_a_normal_one() {
    let d = data();
    let usb = chosen(&d, "usb");
    let m = d.snapshot(&usb, Reason::Manual);
    let today = d.attach("b.txt", b"only today");
    let (_, saved) = stage(&d, &usb, &m.name, &usb);
    restore::cancel(&d.dir).unwrap();
    assert_nothing_staged(&d);
    assert!(!restore::dir(&d.dir).exists());
    let listed: Vec<String> = b::list(&usb, &d.device()).unwrap().into_iter().map(|e| e.name).collect();
    assert!(listed.contains(&saved.name), "the before-restore snapshot stays: {listed:?}");
    let (d, result) = d.restart();
    assert_eq!(result, None);
    assert_eq!(d.files_rows(), vec![today]);
    assert!(backups_named(&d, "gizai-before-restore-").is_empty());
}
