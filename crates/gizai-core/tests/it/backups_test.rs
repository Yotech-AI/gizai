//! GA-216 QA: backups in format 1 (docs/backup-format.md), in the core. A snapshot in a backups folder (`make`): the
//! folder's layout, the name, the manifest field by field, the database it holds (sha256, integrity, the rows it had
//! when it started), the blobs it names (stored once, a second snapshot copies none and touches what it finds), an
//! interrupted one, the `.part` files an old one left, Keep (`prune`, with another computer's snapshots and a
//! before-restore one beside), the blob sweep after 7 days, `list` and why a snapshot isn't restorable, the settings,
//! the schedule's sums, and what never goes into the folder.
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use gizai_core::backups::{self as b, BackupSettings, Every, Reason, Step};
use gizai_core::db::{self, Db};
use gizai_core::{Error, files, ids, seed};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

const HOUR: i64 = 60 * 60 * 1000;
const DAY: i64 = 24 * HOUR;

// ---- set-up ----

/// A Gizai data folder with its database open, as the app has it while a snapshot is made.
struct Data {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    dir: PathBuf,
    db: Db,
    you: String,
    org: String,
}

impl Data {
    fn device(&self) -> String {
        self.db.device_id().to_string()
    }

    fn tag(&self) -> String {
        let d = self.device();
        d[d.len() - 8..].to_string()
    }

    /// A snapshot in `chosen` with this reason and Keep, every step let through.
    fn make(&self, chosen: &Path, reason: Reason, keep: i64) -> b::Made {
        b::make(&self.dir, self.db.device_id(), chosen, reason, keep, &mut |_| Ok(())).unwrap()
    }

    /// A stored file with these bytes, attached to you (a `files` row naming its blob).
    fn attach(&self, name: &str, bytes: &[u8]) -> String {
        let blob = files::store_bytes(&self.dir, name, bytes).unwrap();
        self.db.write(Some(&self.you), |w| files::insert(w, Some(&self.you), "actor", &self.you, &blob)).unwrap();
        blob.sha256
    }

    /// An artifact with one version: its folder's hash `sha` and its thumbnail `thumb` (both name a blob).
    fn artifact(&self, sha: &str, thumb: Option<&str>) {
        let (a, v, now) = (ids::new_id(), ids::new_id(), ids::now_ms());
        self.db.write(None, |w| {
            w.conn().execute("INSERT INTO artifacts (id, created_at, updated_at, org_id, title) VALUES (?1, ?2, ?2, ?3, 'Mock-up')",
                             rusqlite::params![a, now, self.org])?;
            w.conn().execute("INSERT INTO artifact_versions (id, created_at, artifact_id, version, sha256, size_bytes, thumbnail_sha256)
                              VALUES (?1, ?2, ?3, 1, ?4, 10, ?5)", rusqlite::params![v, now, a, sha, thumb])?;
            Ok(())
        }).unwrap();
    }

    fn sql(&self, sql: &str) {
        self.db.write(None, |w| { w.conn().execute_batch(sql)?; Ok(()) }).unwrap();
    }
}

fn data() -> Data {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let dir = root.join("data");
    fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&dir.join("gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    Data { _tmp: tmp, root, dir, db, you: s.you_id, org: s.org_id }
}

/// A second computer: its own data folder and database, so its own `devices.id`.
fn other_computer() -> Data {
    data()
}

/// An empty folder the user chose (a USB disk, a Dropbox folder).
fn chosen(d: &Data, name: &str) -> PathBuf {
    let p = d.root.join(name);
    fs::create_dir_all(&p).unwrap();
    p
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Every file under `dir`, as paths relative to it with `/`, sorted.
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

fn manifests_in(dir: &Path) -> Vec<String> {
    tree(&dir.join("snapshots")).into_iter().filter(|f| f.ends_with(".json")).collect()
}

fn parts_in(dir: &Path) -> Vec<String> {
    tree(dir).into_iter().filter(|f| f.ends_with(".part")).collect()
}

/// The temporary copies a snapshot keeps in the data folder's `backups/`.
fn temp_copies(d: &Data) -> Vec<String> {
    tree(&d.dir.join("backups")).into_iter().filter(|f| f.starts_with(".snapshot-")).collect()
}

fn set_mtime(path: &Path, ms: i64) {
    File::options().write(true).open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_millis(ms as u64)).unwrap();
}

fn mtime_ms(path: &Path) -> i64 {
    fs::metadata(path).unwrap().modified().unwrap().duration_since(UNIX_EPOCH).unwrap().as_millis() as i64
}

fn write_at(path: &Path, bytes: &[u8], ms: i64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    set_mtime(path, ms);
}

fn raw_manifest(dir: &Path, name: &str) -> serde_json::Value {
    serde_json::from_slice(&fs::read(dir.join("snapshots").join(format!("{name}.json"))).unwrap()).unwrap()
}

fn edit_manifest(dir: &Path, name: &str, edit: impl FnOnce(&mut serde_json::Value)) {
    let mut v = raw_manifest(dir, name);
    edit(&mut v);
    fs::write(dir.join("snapshots").join(format!("{name}.json")), serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

/// Each table's row count.
fn counts(c: &Connection) -> Vec<(String, i64)> {
    let tables: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
    tables.into_iter().map(|t| {
        let n: i64 = c.query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get(0)).unwrap();
        (t, n)
    }).collect()
}

// ---- the folder ----

#[test]
fn back_up_makes_a_gizai_backups_folder_in_the_chosen_folder_with_the_layout_of_format_1() {
    let d = data();
    let a = d.attach("plan.pdf", b"the plan");
    let shot = d.attach("shot.png", b"a screenshot");
    let usb = chosen(&d, "usb");
    let made = d.make(&usb, Reason::Manual, 10);

    let dir = usb.join("Gizai backups");
    assert_eq!(made.dir, dir);
    assert_eq!(b::backups_dir(&usb), dir);
    let name = &made.manifest.name;
    let mut want = vec![
        "README.txt".to_string(),
        format!("files/{}/{a}", &a[..2]),
        format!("files/{}/{shot}", &shot[..2]),
        "gizai-backups.json".to_string(),
        format!("snapshots/{name}.db.zst"),
        format!("snapshots/{name}.json"),
    ];
    want.sort();
    assert_eq!(tree(&dir), want, "the folder holds the marker, README.txt, the snapshot's two files and its blobs");
    assert_eq!(tree(&usb).len(), want.len(), "nothing else in the chosen folder");
    assert_eq!(fs::read_to_string(dir.join("gizai-backups.json")).unwrap().trim(), r#"{"format":"gizai-backups","format_version":1}"#);
    let readme = fs::read_to_string(dir.join("README.txt")).unwrap();
    assert!(readme.contains("Restore in Gizai: Settings → General → Backups."), "{readme}");
    assert_eq!(fs::read(dir.join("files").join(&a[..2]).join(&a)).unwrap(), b"the plan", "a blob is the file as it is");
    assert!(temp_copies(&d).is_empty(), "the temporary copy is gone: {:?}", temp_copies(&d));
}

#[test]
fn a_chosen_folder_that_holds_gizai_backups_json_is_the_backups_folder_itself() {
    let d = data();
    let first = chosen(&d, "usb");
    let made = d.make(&first, Reason::Manual, 10);
    // the user then chooses the backups folder itself (or a copy of it on another disk)
    let made2 = d.make(&made.dir, Reason::Manual, 10);
    assert_eq!(made2.dir, made.dir);
    assert!(!made.dir.join("Gizai backups").exists(), "no Gizai backups inside a backups folder");
    assert_eq!(manifests_in(&made.dir).len(), 2);

    // a folder with only the marker in it is one too
    let other = chosen(&d, "dropbox");
    fs::write(other.join("gizai-backups.json"), r#"{"format":"gizai-backups","format_version":1}"#).unwrap();
    let made3 = d.make(&other, Reason::Manual, 10);
    assert_eq!(made3.dir, other);
    assert!(other.join("snapshots").join(format!("{}.json", made3.manifest.name)).is_file());
    assert!(other.join("README.txt").is_file(), "README.txt is written when it is missing");
    assert_eq!(b::backups_dir(&other), other);
}

#[test]
fn a_backups_folder_a_newer_gizai_made_takes_no_snapshot_and_says_why() {
    let d = data();
    let usb = chosen(&d, "usb");
    fs::create_dir_all(usb.join("Gizai backups")).unwrap();
    fs::write(usb.join("Gizai backups/gizai-backups.json"), r#"{"format":"gizai-backups","format_version":2}"#).unwrap();
    let why = b::folder_problem(&usb).expect("a problem");
    assert!(why.contains("newer Gizai"), "{why}");
    let err = b::make(&d.dir, d.db.device_id(), &usb, Reason::Manual, 10, &mut |_| Ok(())).unwrap_err().to_string();
    assert!(err.contains("newer Gizai"), "{err}");
    assert!(!usb.join("Gizai backups/snapshots").exists());
}

// ---- the name and the manifest ----

#[test]
fn a_snapshot_is_named_after_its_reason_its_local_time_and_the_last_8_characters_of_this_computer() {
    let d = data();
    let usb = chosen(&d, "usb");
    for reason in [Reason::Daily, Reason::Weekly, Reason::Manual, Reason::Quit, Reason::BeforeRestore] {
        let m = d.make(&usb, reason, 100).manifest;
        let want = format!("gizai-{}-{}-{}", reason.as_str(), db::stamp(m.made_at), d.tag());
        assert_eq!(m.name, want);
        assert_eq!(m.reason, reason.as_str());
    }
    assert_eq!(b::device_tag("0199f1c2-3d4e-7a00-8b11-123456789abc"), "56789abc");
    assert_eq!(b::snapshot_name(Reason::BeforeRestore, 0, "9f3c2a1b"), format!("gizai-before-restore-{}-9f3c2a1b", db::stamp(0)));
}

#[test]
fn the_manifest_has_every_field_of_format_1_in_order() {
    let d = data();
    let a = d.attach("a.txt", b"alpha");
    let thumb_bytes = b"thumbnail png";
    let thumb = files::store_bytes(&d.dir, "thumb.png", thumb_bytes).unwrap().sha256;
    // an artifact's folder hash, which no blob in the data folder has
    let absent = sha_hex(b"an artifact folder manifest");
    d.artifact(&absent, Some(&thumb));
    let usb = chosen(&d, "usb");
    let before = ids::now_ms();
    let made = d.make(&usb, Reason::Daily, 10);
    let after = ids::now_ms();
    let name = made.manifest.name.clone();

    let text = fs::read_to_string(made.dir.join("snapshots").join(format!("{name}.json"))).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    let fields = ["format", "format_version", "name", "reason", "made_at", "app_version", "schema_version", "device", "db",
                  "last_change", "files", "files_missing"];
    assert_eq!(keys, fields.iter().copied().collect(), "{text}");
    // in the card's order, as docs/backup-format.md says
    let at: Vec<usize> = fields.iter().map(|f| text.find(&format!("\n  \"{f}\":")).unwrap_or_else(|| panic!("{f} in {text}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "fields out of order: {text}");

    assert_eq!(v["format"], "gizai-backup");
    assert_eq!(v["format_version"], 1);
    assert_eq!(v["name"], name.as_str());
    assert_eq!(v["reason"], "daily");
    let made_at = v["made_at"].as_i64().unwrap();
    assert!((before..=after).contains(&made_at), "{made_at} not in {before}..{after}");
    assert_eq!(v["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["schema_version"], db::SCHEMA_VERSION);
    let dev_name: String = d.db.read(|c| Ok(c.query_row("SELECT name FROM devices WHERE is_self = 1", [], |r| r.get(0))?)).unwrap();
    assert_eq!(v["device"], serde_json::json!({"id": d.device(), "name": dev_name}));
    let dbf = v["db"].as_object().unwrap();
    assert_eq!(dbf.keys().map(|k| k.as_str()).collect::<BTreeSet<_>>(), ["bytes", "compression", "db_bytes", "path", "sha256"].into());
    assert_eq!(v["db"]["path"], format!("snapshots/{name}.db.zst"));
    assert_eq!(v["db"]["compression"], "zstd");
    let zst = made.dir.join(v["db"]["path"].as_str().unwrap());
    assert_eq!(v["db"]["bytes"].as_u64().unwrap(), fs::metadata(&zst).unwrap().len(), "bytes: the compressed file's size");
    let lc = v["last_change"].as_object().unwrap();
    assert_eq!(lc.keys().map(|k| k.as_str()).collect::<BTreeSet<_>>(), ["hlc", "id", "seq"].into());

    // files: each blob the database names, once, with its size; the one the data folder lacks is in files_missing too
    let mut want = vec![(a.clone(), 5), (thumb.clone(), thumb_bytes.len() as i64), (absent.clone(), 0)];
    want.sort();
    let got: Vec<(String, i64)> = v["files"].as_array().unwrap().iter()
        .map(|f| (f["sha256"].as_str().unwrap().to_string(), f["bytes"].as_i64().unwrap())).collect();
    assert_eq!(got, want);
    for f in v["files"].as_array().unwrap() {
        assert_eq!(f.as_object().unwrap().len(), 2, "sha256 and bytes: {f}");
        let s = f["sha256"].as_str().unwrap();
        assert!(s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "64 lowercase hex: {s}");
    }
    assert_eq!(v["files_missing"], serde_json::json!([absent]));
    assert!(made.dir.join("files").join(&thumb[..2]).join(&thumb).is_file(), "the thumbnail's blob is stored");
    assert!(!made.dir.join("files").join(&absent[..2]).join(&absent).exists());
    // what the snapshot reads back is the same
    assert_eq!(b::read_manifest(&made.dir, &name).unwrap(), made.manifest);
}

#[test]
fn last_change_is_null_when_the_copied_database_has_no_changes() {
    let d = data();
    d.sql("DELETE FROM changes");
    let usb = chosen(&d, "usb");
    let made = d.make(&usb, Reason::Manual, 10);
    assert_eq!(made.manifest.last_change, None);
    assert!(raw_manifest(&made.dir, &made.manifest.name)["last_change"].is_null());
}

#[test]
fn the_database_unpacks_to_its_sha256_passes_integrity_check_and_has_the_rows_it_had_when_the_snapshot_started() {
    let d = data();
    for i in 0..30 {
        d.attach(&format!("f{i}.txt"), format!("file number {i}").as_bytes());
    }
    let usb = chosen(&d, "usb");
    let live_before = d.db.read(|c| Ok(counts(c))).unwrap();
    // Gizai goes on while the snapshot is made: a file added after the copy isn't in it
    let mut wrote = false;
    let made = b::make(&d.dir, d.db.device_id(), &usb, Reason::Manual, 10, &mut |step| {
        if step == Step::Files && !wrote {
            wrote = true;
            d.attach("later.txt", b"added while the snapshot ran");
        }
        Ok(())
    }).unwrap();
    assert!(wrote);
    let m = &made.manifest;

    let out = d.root.join("unpacked.db");
    let read = b::unpack_db(&made.dir, &m.name, &out).unwrap();
    assert_eq!(&read, m);
    let bytes = fs::read(&out).unwrap();
    assert_eq!(sha_hex(&bytes), m.db.sha256, "db.sha256 is the decompressed database's");
    assert_eq!(bytes.len() as i64, m.db.db_bytes, "db_bytes is the decompressed database's size");
    // and decompressing it by hand with any zstd gives the same
    let by_hand = zstd::decode_all(File::open(made.dir.join(&m.db.path)).unwrap()).unwrap();
    assert_eq!(by_hand, bytes);

    let c = Connection::open(&out).unwrap();
    let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
    assert_eq!(ok, "ok");
    let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "delete", "one file, nothing beside it");
    assert_eq!(counts(&c), live_before, "the rows Gizai had when the snapshot started");
    let later: i64 = c.query_row("SELECT count(*) FROM files WHERE name = 'later.txt'", [], |r| r.get(0)).unwrap();
    assert_eq!(later, 0);
    let live_later: i64 = d.db.read(|c| Ok(c.query_row("SELECT count(*) FROM files WHERE name = 'later.txt'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(live_later, 1, "Gizai could write while the snapshot ran");

    // last_change is the copy's newest change, not the live database's
    let copy_last: (i64, String, String) = c.query_row("SELECT seq, id, hlc FROM changes ORDER BY seq DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    let lc = m.last_change.clone().unwrap();
    assert_eq!((lc.seq, lc.id, lc.hlc), copy_last);
    let live_max: i64 = d.db.read(|c| Ok(c.query_row("SELECT max(seq) FROM changes", [], |r| r.get(0))?)).unwrap();
    assert!(live_max > copy_last.0, "the live database went on: {live_max} > {}", copy_last.0);
    // and the blob added later isn't in the snapshot
    assert_eq!(m.files.len(), 30);
    drop(c);

    // a database file that doesn't match its manifest isn't unpacked
    edit_manifest(&made.dir, &m.name, |v| v["db"]["sha256"] = serde_json::json!("0".repeat(64)));
    let out2 = d.root.join("unpacked2.db");
    let err = b::unpack_db(&made.dir, &m.name, &out2).unwrap_err().to_string();
    assert!(err.contains("doesn't match"), "{err}");
    assert!(!out2.exists(), "a failed unpack leaves nothing");
    assert!(b::read_manifest(&made.dir, "../../etc").is_err(), "a name that leaves snapshots/ is refused");
}

#[test]
fn files_lists_each_blob_the_copied_database_names_once() {
    let d = data();
    let same = d.attach("one.png", b"the same picture");
    d.attach("two.png", b"the same picture");
    d.artifact(&same, Some(&same));
    // a removed file's blob is kept in the data folder, and named
    let gone = d.attach("removed.txt", b"removed later");
    d.sql(&format!("UPDATE files SET deleted_at = 1 WHERE sha256 = '{gone}'"));
    // a blob in the data folder that no row names isn't
    let orphan = files::store_bytes(&d.dir, "orphan.bin", b"nobody names me").unwrap().sha256;
    let usb = chosen(&d, "usb");
    let made = d.make(&usb, Reason::Manual, 10);
    let mut want = vec![same.clone(), gone.clone()];
    want.sort();
    let got: Vec<String> = made.manifest.files.iter().map(|f| f.sha256.clone()).collect();
    assert_eq!(got, want, "each once, sorted by hash");
    assert!(made.manifest.files_missing.is_empty());
    assert_eq!(made.copied, 2);
    assert!(!made.dir.join("files").join(&orphan[..2]).join(&orphan).exists(), "only the blobs the database names");
}

#[test]
fn a_blob_the_data_folder_lacks_is_in_files_missing_and_the_snapshot_is_still_made() {
    let d = data();
    let sha = d.attach("lost.pdf", b"will be lost");
    let kept = d.attach("kept.pdf", b"is still there");
    let blob = files::blob_path(&d.dir, &sha);
    let mut perm = fs::metadata(&blob).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    fs::set_permissions(&blob, perm).unwrap();
    fs::remove_file(&blob).unwrap();
    let usb = chosen(&d, "usb");
    let made = d.make(&usb, Reason::Manual, 10);
    assert_eq!(made.manifest.files_missing, vec![sha.clone()]);
    let lost = made.manifest.files.iter().find(|f| f.sha256 == sha).unwrap();
    assert_eq!(lost.bytes, 12, "a missing one has the size its files row gives");
    assert!(made.dir.join("files").join(&kept[..2]).join(&kept).is_file());
    let e = b::list(&usb, d.db.device_id()).unwrap().remove(0);
    assert!(!e.restorable);
    assert_eq!(e.problem.as_deref(), Some("1 file is missing"));
    assert_eq!(e.files_missing, 1);
}

// ---- a second snapshot ----

#[test]
fn a_second_snapshot_copies_no_blob_the_folder_has_and_sets_its_modified_time_to_now() {
    let d = data();
    let shas: Vec<String> = (0..3).map(|i| d.attach(&format!("f{i}.bin"), format!("blob {i}").as_bytes())).collect();
    let usb = chosen(&d, "usb");
    let first = d.make(&usb, Reason::Manual, 10);
    assert_eq!((first.copied, first.found), (3, 0));
    let paths: Vec<PathBuf> = shas.iter().map(|s| first.dir.join("files").join(&s[..2]).join(s)).collect();
    let long_ago = ids::now_ms() - 3 * DAY;
    for p in &paths {
        set_mtime(p, long_ago);
    }
    #[cfg(unix)]
    let inodes: Vec<u64> = paths.iter().map(|p| std::os::unix::fs::MetadataExt::ino(&fs::metadata(p).unwrap())).collect();
    let new = d.attach("new.bin", b"a new one");

    let before = ids::now_ms();
    let second = d.make(&usb, Reason::Manual, 10);
    assert_eq!((second.copied, second.found), (1, 3), "only the new blob is copied");
    for p in &paths {
        let t = mtime_ms(p);
        assert!(t >= before - 2000, "a found blob's modified time is now: {t} < {before}");
    }
    #[cfg(unix)]
    {
        let after: Vec<u64> = paths.iter().map(|p| std::os::unix::fs::MetadataExt::ino(&fs::metadata(p).unwrap())).collect();
        assert_eq!(after, inodes, "the found blobs are the same files, not copies");
    }
    assert!(first.dir.join("files").join(&new[..2]).join(&new).is_file());
    assert_eq!(second.manifest.files.len(), 4);
}

#[test]
fn a_blob_another_computer_stored_is_found_not_copied_again() {
    let a = data();
    let b2 = other_computer();
    let usb = chosen(&a, "usb");
    a.attach("shared.png", b"both computers have this");
    b2.attach("shared.png", b"both computers have this");
    assert_eq!(a.make(&usb, Reason::Manual, 10).copied, 1);
    let made = b2.make(&usb, Reason::Manual, 10);
    assert_eq!((made.copied, made.found), (0, 1));
}

// ---- interrupted ----

#[test]
fn a_snapshot_that_fails_at_any_step_leaves_no_manifest_no_part_file_and_no_temporary_copy() {
    for at in [Step::Database, Step::Files, Step::Compress, Step::Manifest] {
        let d = data();
        for i in 0..3 {
            d.attach(&format!("f{i}.bin"), format!("blob {i}").as_bytes());
        }
        let usb = chosen(&d, "usb");
        let err = b::make(&d.dir, d.db.device_id(), &usb, Reason::Manual, 10, &mut |step| {
            if step == at { Err(Error::Invalid(format!("the disk is full at {}", step.as_str()))) } else { Ok(()) }
        }).unwrap_err();
        assert!(err.to_string().contains("the disk is full"), "{err}");
        let dir = usb.join("Gizai backups");
        assert!(manifests_in(&dir).is_empty(), "at {at:?}: {:?}", tree(&dir));
        assert!(parts_in(&dir).is_empty(), "at {at:?}: {:?}", parts_in(&dir));
        assert!(!tree(&dir.join("snapshots")).iter().any(|f| f.ends_with(".db.zst")), "at {at:?}: no database without its manifest");
        assert!(temp_copies(&d).is_empty(), "at {at:?}: {:?}", temp_copies(&d));
        assert!(b::list(&usb, d.db.device_id()).unwrap().is_empty());
        assert!(b::newest(&dir, d.db.device_id(), ids::now_ms()).is_none());
        // and the next one works
        let made = d.make(&usb, Reason::Manual, 10);
        assert_eq!(manifests_in(&dir), [format!("{}.json", made.manifest.name)]);
    }
}

#[test]
fn a_later_snapshot_removes_this_computers_part_files_once_they_are_an_hour_old() {
    let d = data();
    let other = other_computer();
    let usb = chosen(&d, "usb");
    let dir = d.make(&usb, Reason::Manual, 10).dir;
    let now = ids::now_ms();
    let (old, young) = (now - 2 * HOUR, now - 30 * 60 * 1000);
    let (me, them) = (d.tag(), other.tag());
    let snaps = dir.join("snapshots");
    let blob = sha_hex(b"half a blob");
    let young_blob = sha_hex(b"half a blob, being copied now");
    let stem = |tag: &str, n: u32| format!("gizai-manual-20261020-10000{n}-000-{tag}");
    // what an interrupted snapshot of this computer left, two hours ago
    let gone = [
        snaps.join(format!("{}.db.zst.part", stem(&me, 1))),
        snaps.join(format!("{}.json.part", stem(&me, 2))),
        snaps.join(format!("{}.db.zst", stem(&me, 3))),   // a database whose manifest never came
        dir.join("files").join(&blob[..2]).join(format!("{blob}.{me}.part")),
        dir.join(format!("README.txt.{me}.part")),
        d.dir.join("backups").join(format!(".snapshot-{}.db", stem(&me, 4))),
    ];
    // what stays: this computer's half an hour old, another computer's, and a database whose manifest is there
    let stay = [
        snaps.join(format!("{}.json.part", stem(&me, 5))),
        dir.join("files").join(&young_blob[..2]).join(format!("{young_blob}.{me}.part")),
        snaps.join(format!("{}.db.zst.part", stem(&them, 6))),
        snaps.join(format!("{}.json.part", stem(&them, 7))),
        dir.join("files").join(&blob[..2]).join(format!("{blob}.{them}.part")),
    ];
    for p in &gone {
        write_at(p, b"{}", old);
    }
    for (i, p) in stay.iter().enumerate() {
        write_at(p, b"{}", if i < 2 { young } else { old });
    }
    let made = d.make(&usb, Reason::Manual, 10);
    for p in &gone {
        assert!(!p.exists(), "{} is gone", p.display());
    }
    for p in &stay {
        assert!(p.exists(), "{} stays", p.display());
    }
    assert!(snaps.join(format!("{}.db.zst", made.manifest.name)).is_file());
    assert_eq!(manifests_in(&dir).len(), 2, "the good snapshots stay");
}

// ---- Keep and the blob sweep ----

#[test]
fn keep_3_with_5_of_this_computers_snapshots_leaves_the_newest_3_and_others_stay() {
    let a = data();
    let other = other_computer();
    let usb = chosen(&a, "usb");
    // a blob only the first two snapshots name: its file row goes for good after them
    let lone = a.attach("only-early.txt", b"named by the first two");
    a.attach("always.txt", b"named by all");
    let mut mine = vec![];
    for i in 0..2 {
        mine.push(a.make(&usb, if i == 0 { Reason::Daily } else { Reason::Quit }, 10).manifest.name);
    }
    a.sql(&format!("DELETE FROM files WHERE sha256 = '{lone}'"));
    let theirs: Vec<String> = (0..2).map(|_| other.make(&usb, Reason::Daily, 10).manifest.name).collect();
    let restore = a.make(&usb, Reason::BeforeRestore, 10).manifest.name;
    for _ in 0..2 {
        mine.push(a.make(&usb, Reason::Weekly, 10).manifest.name);
    }
    // the fifth, with Keep 3
    let fifth = a.make(&usb, Reason::Manual, 3);
    mine.push(fifth.manifest.name.clone());
    assert_eq!(fifth.pruned.snapshots, 2);
    assert_eq!(fifth.pruned.blobs, 0, "the blob only the removed ones named is too young to go");
    let dir = fifth.dir.clone();
    let snaps = dir.join("snapshots");
    for n in &mine[..2] {
        assert!(!snaps.join(format!("{n}.json")).exists() && !snaps.join(format!("{n}.db.zst")).exists(), "{n} went");
    }
    for n in mine[2..].iter().chain(&theirs).chain([&restore]) {
        assert!(snaps.join(format!("{n}.json")).is_file() && snaps.join(format!("{n}.db.zst")).is_file(), "{n} stays");
    }
    let lone_path = dir.join("files").join(&lone[..2]).join(&lone);
    assert!(lone_path.is_file());

    // another snapshot with Keep 3 removes none of the others: before-restore and the other computer's don't count
    let sixth = a.make(&usb, Reason::Manual, 3);
    assert_eq!(sixth.pruned.snapshots, 1);
    assert!(snaps.join(format!("{restore}.json")).is_file());
    assert!(theirs.iter().all(|n| snaps.join(format!("{n}.json")).is_file()));

    // the blob goes once it hasn't been modified for 7 days, not before; the named ones stay
    let now = ids::now_ms();
    assert_eq!(b::prune(&dir, a.db.device_id(), 3, now + 7 * DAY - 60_000).unwrap().blobs, 0);
    assert!(lone_path.is_file(), "not before 7 days");
    let p = b::prune(&dir, a.db.device_id(), 3, now + 7 * DAY + 60_000).unwrap();
    assert_eq!(p, b::Pruned { snapshots: 0, blobs: 1 });
    assert!(!lone_path.exists(), "gone at 7 days");
    let named: BTreeSet<String> = manifests_in(&dir).iter()
        .flat_map(|m| b::read_manifest(&dir, m.trim_end_matches(".json")).unwrap().files.into_iter().map(|f| f.sha256)).collect();
    for s in &named {
        assert!(dir.join("files").join(&s[..2]).join(s).is_file(), "a named blob stays: {s}");
    }
}

#[test]
fn a_blob_only_another_computers_unfinished_manifest_names_stays() {
    let a = data();
    let other = other_computer();
    let usb = chosen(&a, "usb");
    let made = a.make(&usb, Reason::Manual, 10);
    let dir = made.dir;
    let sha = sha_hex(b"the other computer found this one");
    let blob = dir.join("files").join(&sha[..2]).join(&sha);
    let now = ids::now_ms();
    write_at(&blob, b"the other computer found this one", now - 30 * DAY);
    // the other computer's snapshot runs: its manifest is still a .part
    let part = dir.join("snapshots").join(format!("gizai-manual-20261021-090000-000-{}.json.part", other.tag()));
    fs::write(&part, serde_json::to_vec(&serde_json::json!({"format": "gizai-backup", "files": [{"sha256": sha, "bytes": 33}]})).unwrap()).unwrap();
    assert_eq!(b::prune(&dir, a.db.device_id(), 10, now).unwrap().blobs, 0);
    assert!(blob.is_file(), ".part manifests count");
    fs::remove_file(&part).unwrap();
    assert_eq!(b::prune(&dir, a.db.device_id(), 10, now).unwrap().blobs, 1);
    assert!(!blob.exists());
}

#[test]
fn keep_counts_a_snapshot_dated_after_now_as_the_oldest() {
    let d = data();
    let usb = chosen(&d, "usb");
    let ahead = d.make(&usb, Reason::Manual, 10).manifest.name;
    edit_manifest(&usb.join("Gizai backups"), &ahead, |v| v["made_at"] = serde_json::json!(ids::now_ms() + 365 * DAY));
    let made = d.make(&usb, Reason::Manual, 1);
    assert_eq!(made.pruned.snapshots, 1);
    let left = manifests_in(&made.dir);
    assert_eq!(left, [format!("{}.json", made.manifest.name)], "the one just made stays");
}

// ---- list ----

#[test]
fn list_shows_every_computers_snapshots_newest_first_and_whether_each_can_be_restored() {
    let a = data();
    let other = other_computer();
    let usb = chosen(&a, "usb");
    a.attach("a.txt", b"one");
    let mine = a.make(&usb, Reason::Manual, 10).manifest;
    let theirs = other.make(&usb, Reason::Daily, 10).manifest;
    let newer = a.make(&usb, Reason::Quit, 10).manifest;

    let list = b::list(&usb, a.db.device_id()).unwrap();
    assert_eq!(list.iter().map(|e| e.name.clone()).collect::<Vec<_>>(), [newer.name.clone(), theirs.name.clone(), mine.name.clone()]);
    let t = &list[1];
    assert!(!t.this_computer);
    assert_eq!(t.device_id, other.device());
    assert_eq!(t.reason, "daily");
    assert!(t.restorable, "{:?}", t.problem);
    let m = &list[2];
    assert!(m.this_computer);
    assert_eq!((m.made_at, m.app_version.as_str(), m.schema_version), (mine.made_at, env!("CARGO_PKG_VERSION"), db::SCHEMA_VERSION));
    assert_eq!((m.db_bytes, m.files_count, m.files_bytes, m.files_missing), (mine.db.db_bytes, 1, 3, 0));
    assert!(m.restorable && m.problem.is_none());
    // the other computer sees them the other way round; the backups folder itself lists the same
    let theirs_view = b::list(&usb.join("Gizai backups"), other.db.device_id()).unwrap();
    assert_eq!(theirs_view.iter().filter(|e| e.this_computer).map(|e| e.name.clone()).collect::<Vec<_>>(), [theirs.name.clone()]);

    // a newer schema, a newer format, a missing database file, missing files
    let dir = usb.join("Gizai backups");
    edit_manifest(&dir, &newer.name, |v| v["schema_version"] = serde_json::json!(99));
    edit_manifest(&dir, &theirs.name, |v| v["format_version"] = serde_json::json!(2));
    let list = b::list(&usb, a.db.device_id()).unwrap();
    for e in &list[..2] {
        assert!(!e.restorable, "{}", e.name);
        let why = e.problem.as_deref().unwrap();
        assert!(why.contains("A newer Gizai made it"), "{why}");
    }
    assert_eq!(list[0].schema_version, 99);
    fs::remove_file(dir.join("snapshots").join(format!("{}.db.zst", mine.name))).unwrap();
    let e = b::list(&usb, a.db.device_id()).unwrap().into_iter().find(|e| e.name == mine.name).unwrap();
    assert_eq!((e.restorable, e.problem.as_deref()), (false, Some("Its database file is missing")));
}

#[test]
fn list_counts_the_files_missing_from_the_folder_now() {
    let d = data();
    let shas: Vec<String> = (0..4).map(|i| d.attach(&format!("{i}.txt"), format!("file {i}").as_bytes())).collect();
    let usb = chosen(&d, "usb");
    let dir = d.make(&usb, Reason::Manual, 10).dir;
    for s in &shas[..3] {
        fs::remove_file(dir.join("files").join(&s[..2]).join(s)).unwrap();
    }
    let e = b::list(&usb, d.db.device_id()).unwrap().remove(0);
    assert_eq!((e.files_count, e.files_missing), (4, 3));
    assert_eq!((e.restorable, e.problem.as_deref()), (false, Some("3 files are missing")));
}

#[test]
fn list_reads_a_manifest_a_newer_gizai_wrote_in_another_shape_as_not_restorable() {
    let d = data();
    let usb = chosen(&d, "usb");
    let dir = d.make(&usb, Reason::Manual, 10).dir;
    let name = format!("gizai-daily-20261021-090200-123-{}", "9f3c2a1b");
    fs::write(dir.join("snapshots").join(format!("{name}.json")), serde_json::to_vec(&serde_json::json!({
        "format": "gizai-backup", "format_version": 2, "name": name, "reason": "daily", "made_at": 1, "app_version": "2.0.0",
        "schema_version": 40, "device": {"id": "someone-else", "name": "laptop"}, "db": "now a string", "files": {"a": 1},
    })).unwrap()).unwrap();
    let list = b::list(&usb, d.db.device_id()).unwrap();
    assert_eq!(list.len(), 2);
    let e = list.iter().find(|e| e.name == name).unwrap();
    assert!(!e.this_computer && !e.restorable);
    assert_eq!(e.problem.as_deref(), Some("A newer Gizai made it (2.0.0): update Gizai to restore it"));
    assert_eq!(e.device_name, "laptop");
}

#[test]
fn list_refuses_a_folder_that_is_missing_and_lists_none_in_an_empty_one() {
    let d = data();
    let err = b::list(&d.root.join("unplugged"), d.db.device_id()).unwrap_err().to_string();
    assert!(err.contains("can't be found"), "{err}");
    let empty = chosen(&d, "empty");
    assert!(b::list(&empty, d.db.device_id()).unwrap().is_empty());
}

// ---- the schedule ----

#[test]
fn a_daily_or_weekly_snapshot_is_due_a_day_or_a_week_after_the_newest_good_one() {
    let t = 1_792_573_322_123;
    assert!(b::is_due(Every::Daily, None, t), "the first one is due at once");
    assert_eq!(b::next_due(Every::Daily, None, t), Some(t));
    assert_eq!(b::next_due(Every::Daily, Some(t), t + HOUR), Some(t + DAY));
    assert_eq!(b::next_due(Every::Weekly, Some(t), t + HOUR), Some(t + 7 * DAY));
    assert_eq!(b::next_due(Every::Manual, Some(t), t + HOUR), None);
    assert!(!b::is_due(Every::Daily, Some(t), t + 23 * HOUR));
    assert!(b::is_due(Every::Daily, Some(t), t + DAY));
    assert!(b::is_due(Every::Daily, Some(t), t + DAY - 5 * 60 * 1000), "up to 5 minutes early counts");
    assert!(!b::is_due(Every::Daily, Some(t), t + DAY - 6 * 60 * 1000));
    assert!(!b::is_due(Every::Weekly, Some(t), t + 6 * DAY + 23 * HOUR));
    assert!(b::is_due(Every::Weekly, Some(t), t + 7 * DAY));
    assert!(!b::is_due(Every::Manual, None, t), "never on its own");
    assert!(!b::is_due(Every::Manual, Some(t), t + 30 * DAY));
}

#[test]
fn the_newest_good_snapshot_is_this_computers_not_before_restore_not_dated_ahead_with_its_database() {
    let a = data();
    let other = other_computer();
    let usb = chosen(&a, "usb");
    let dir = b::backups_dir(&usb);
    let good = a.make(&usb, Reason::Daily, 10).manifest;
    other.make(&usb, Reason::Daily, 10);
    a.make(&usb, Reason::BeforeRestore, 10);
    let now = ids::now_ms();
    let n = b::newest(&dir, a.db.device_id(), now).unwrap();
    assert_eq!((n.name.as_str(), n.made_at, n.bytes), (good.name.as_str(), good.made_at, good.db.bytes));
    // one made while the clock was ahead doesn't count
    let ahead = a.make(&usb, Reason::Manual, 10).manifest;
    edit_manifest(&dir, &ahead.name, |v| v["made_at"] = serde_json::json!(now + 30 * DAY));
    assert_eq!(b::newest(&dir, a.db.device_id(), ids::now_ms()).unwrap().name, good.name);
    // one whose database file is gone doesn't either
    fs::remove_file(dir.join("snapshots").join(format!("{}.db.zst", good.name))).unwrap();
    assert!(b::newest(&dir, a.db.device_id(), ids::now_ms()).is_none());
}

// ---- the settings ----

#[test]
fn the_backup_settings_default_to_no_folder_daily_on_quit_off_and_keep_10() {
    let d = data();
    let s = b::settings(&d.db);
    assert_eq!(s, BackupSettings { folder: None, every: Every::Daily, on_quit: false, keep: 10 });
    assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({"folder": null, "every": "daily", "onQuit": false, "keep": 10}));
    let st = b::status(&d.db, None, ids::now_ms());
    assert_eq!((st.folder, st.folder_problem, st.running, st.last, st.last_error, st.next), (None, None, None, None, None, None));
}

#[test]
fn save_settings_keeps_them_under_this_computers_keys() {
    let d = data();
    let usb = chosen(&d, "usb");
    let f = usb.to_string_lossy().into_owned();
    let saved = b::save_settings(&d.db, BackupSettings { folder: Some(f.clone()), every: Every::Weekly, on_quit: true, keep: 100 }).unwrap();
    assert_eq!(saved, BackupSettings { folder: Some(f.clone()), every: Every::Weekly, on_quit: true, keep: 100 });
    assert_eq!(b::settings(&d.db), saved);
    use gizai_core::settings::get;
    assert_eq!(get::<Option<String>>(&d.db, "backup_folder").unwrap(), Some(Some(f.clone())));
    assert_eq!(get::<String>(&d.db, "backup_every").unwrap().as_deref(), Some("weekly"));
    assert_eq!(get::<bool>(&d.db, "backup_on_quit").unwrap(), Some(true));
    assert_eq!(get::<i64>(&d.db, "backup_keep").unwrap(), Some(100));
    for every in [Every::Daily, Every::Manual] {
        b::save_settings(&d.db, BackupSettings { every, keep: 1, ..saved.clone() }).unwrap();
        assert_eq!(b::settings(&d.db).every, every);
    }
    // no folder: backups are off
    let off = b::save_settings(&d.db, BackupSettings { folder: None, ..saved.clone() }).unwrap();
    assert_eq!(off.folder, None);
    assert_eq!(get::<Option<String>>(&d.db, "backup_folder").unwrap(), Some(None));
    assert_eq!(b::status(&d.db, None, ids::now_ms()).folder, None);
}

#[test]
fn save_settings_refuses_keep_0_and_101_saying_why() {
    let d = data();
    for keep in [0, 101, -1] {
        let err = b::save_settings(&d.db, BackupSettings { keep, ..Default::default() }).unwrap_err().to_string();
        assert!(err.contains("Keep must be 1 to 100"), "{keep}: {err}");
    }
    assert_eq!(b::settings(&d.db).keep, 10, "nothing was saved");
    for keep in [1, 100] {
        assert_eq!(b::save_settings(&d.db, BackupSettings { keep, ..Default::default() }).unwrap().keep, keep);
    }
}

#[test]
fn save_settings_refuses_a_folder_that_is_missing_a_file_or_not_a_full_path_saying_why() {
    let d = data();
    let s = |f: &str| BackupSettings { folder: Some(f.to_string()), ..Default::default() };
    let missing = d.root.join("unplugged").to_string_lossy().into_owned();
    let err = b::save_settings(&d.db, s(&missing)).unwrap_err().to_string();
    assert!(err.contains("can't be found"), "{err}");
    let file = d.root.join("a-file.txt");
    fs::write(&file, "not a folder").unwrap();
    let err = b::save_settings(&d.db, s(&file.to_string_lossy())).unwrap_err().to_string();
    assert!(err.contains("is a file, not a folder"), "{err}");
    let err = b::save_settings(&d.db, s("relative/folder")).unwrap_err().to_string();
    assert!(err.contains("full path"), "{err}");
    assert_eq!(b::settings(&d.db).folder, None, "nothing was saved");
}

#[cfg(unix)]
#[test]
fn save_settings_refuses_a_folder_gizai_cant_write_in_saying_why() {
    use std::os::unix::fs::PermissionsExt;
    let d = data();
    let ro = chosen(&d, "read-only");
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::write(ro.join("probe"), b"").is_ok() {
        // root writes anywhere: nothing to check here
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let why = b::folder_problem(&ro).expect("a problem");
    assert!(why.contains("can't write in"), "{why}");
    let err = b::save_settings(&d.db, BackupSettings { folder: Some(ro.to_string_lossy().into()), ..Default::default() }).unwrap_err().to_string();
    assert!(err.contains("can't write in"), "{err}");
    let err = b::make(&d.dir, d.db.device_id(), &ro, Reason::Manual, 10, &mut |_| Ok(())).unwrap_err().to_string();
    assert!(err.contains("can't write in"), "{err}");
    assert!(tree(&ro).is_empty(), "the probe file didn't stay: {:?}", tree(&ro));
    fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn the_status_says_why_the_folder_cant_be_used_and_shows_no_last_or_next_then() {
    let d = data();
    let usb = chosen(&d, "usb");
    b::save_settings(&d.db, BackupSettings { folder: Some(usb.to_string_lossy().into()), ..Default::default() }).unwrap();
    let made = d.make(&usb, Reason::Manual, 10).manifest;
    let now = ids::now_ms();
    let st = b::status(&d.db, None, now);
    assert_eq!(st.folder.as_deref(), Some(usb.join("Gizai backups").to_string_lossy().as_ref()));
    assert_eq!(st.folder_problem, None);
    assert_eq!(st.last.as_ref().map(|l| l.name.clone()), Some(made.name.clone()));
    assert_eq!(st.next, Some(made.made_at + DAY));
    let v = serde_json::to_value(&st).unwrap();
    let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    assert_eq!(keys, ["folder", "folderProblem", "last", "lastError", "next", "running", "settings"].into(), "contract BK's BackupStatus");
    assert_eq!(v["last"].as_object().unwrap().keys().map(|k| k.as_str()).collect::<BTreeSet<_>>(), ["bytes", "madeAt", "name"].into());

    // the USB disk is unplugged
    fs::rename(&usb, d.root.join("elsewhere")).unwrap();
    let st = b::status(&d.db, None, ids::now_ms());
    let why = st.folder_problem.expect("a problem");
    assert!(why.contains("can't be found") && why.contains("plugged in"), "{why}");
    assert_eq!((st.last, st.next), (None, None));
    // the other settings can still change: the saved folder isn't checked again
    let s = b::save_settings(&d.db, BackupSettings { keep: 5, ..b::settings(&d.db) }).unwrap();
    assert_eq!(s.keep, 5);
}

#[test]
fn a_backup_entry_has_contract_bks_fields() {
    let d = data();
    let usb = chosen(&d, "usb");
    d.make(&usb, Reason::Manual, 10);
    let e = b::list(&usb, d.db.device_id()).unwrap().remove(0);
    let v = serde_json::to_value(&e).unwrap();
    let keys: BTreeSet<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    assert_eq!(keys, ["name", "reason", "madeAt", "deviceId", "deviceName", "thisComputer", "appVersion", "schemaVersion", "dbBytes",
                      "filesCount", "filesBytes", "filesMissing", "restorable", "problem"].into());
    assert!(v["problem"].is_null());
}

// ---- what never goes into the folder ----

#[test]
fn no_keychain_value_worktree_log_code_copy_or_update_file_ends_up_in_the_folder() {
    let d = data();
    d.attach("real.txt", b"a real attachment");
    let secrets = [
        ("keychain.json", "KEYCHAIN-VALUE-7d1f"),
        ("worktrees/GA/GA-1/src/main.rs", "WORKTREE-CODE-91ab"),
        ("worktrees/GA/GA-1/.git", "WORKTREE-GIT-0c2e"),
        ("runs/0199-run.jsonl", "RUN-LOG-55aa"),
        ("runs/0199-run.stderr.log", "RUN-STDERR-55ab"),
        ("chat/0199-chat.jsonl", "CHAT-LOG-66bb"),
        ("code/GA/README.md", "CODE-COPY-77cc"),
        ("update/update.log", "UPDATE-LOG-88dd"),
        ("update/gizai-0.14.0.tar.gz", "UPDATE-BUNDLE-99ee"),
    ];
    for (path, marker) in secrets {
        let p = d.dir.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, marker).unwrap();
    }
    let usb = chosen(&d, "usb");
    let made = d.make(&usb, Reason::Manual, 10);
    let name = &made.manifest.name;
    for f in tree(&usb) {
        let ok = f == "Gizai backups/gizai-backups.json" || f == "Gizai backups/README.txt"
            || f == format!("Gizai backups/snapshots/{name}.json") || f == format!("Gizai backups/snapshots/{name}.db.zst")
            || f.strip_prefix("Gizai backups/files/").is_some_and(|rest| rest.len() == 67 && rest.as_bytes()[2] == b'/');
        assert!(ok, "only the database and its blobs: {f}");
        let mut bytes = fs::read(usb.join(&f)).unwrap();
        if f.ends_with(".db.zst") {
            bytes = zstd::decode_all(&bytes[..]).unwrap();
        }
        let text = String::from_utf8_lossy(&bytes);
        for (path, marker) in secrets {
            assert!(!text.contains(marker), "{path}'s content is in {f}");
        }
    }
    assert_eq!(made.manifest.files.len(), 1);
}

// ---- db::snapshot stays as it was ----

#[test]
fn db_snapshot_still_names_its_copy_by_label_and_time_in_the_data_folders_backups() {
    let d = data();
    let usb = chosen(&d, "usb");
    d.make(&usb, Reason::Manual, 10);
    let snap = db::snapshot(&d.dir.join("gizai.db"), &d.dir.join("backups"), "manual").unwrap();
    let name = snap.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("gizai-manual-") && name.ends_with(".db"), "{name}");
    assert_eq!(snap.parent().unwrap(), d.dir.join("backups"));
    let left = tree(&d.dir.join("backups"));
    assert_eq!(left, [name], "a backups-folder snapshot leaves nothing in the data folder's backups/");
}
