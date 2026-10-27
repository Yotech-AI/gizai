//! Restore (GA-218): a snapshot in format 1 (`backups`, docs/backup-format.md) comes back as this computer's data. The
//! app runs it in the background (src-tauri's `restore`), in three steps:
//!
//! 1. *Check* (`check`): the manifest is format 1's and no newer Gizai's (its `format_version` and `schema_version`),
//!    the database decompresses, matches its sha256, passes `PRAGMA integrity_check` and has the manifest's
//!    `user_version`, and every blob the manifest names is in the backups folder with its size and hash.
//! 2. *Save today's data*: a `before-restore` snapshot (the app's `backups::back_up`).
//! 3. *Stage* (`stage`): the checked database as `<data>/restore/gizai.db`, the blobs `<data>/files/` lacks in
//!    `<data>/restore/files/`, and `<data>/restore/restore.json` (`Staged`), written last: from then on the restore is
//!    ready, and waits for a restart. Today's data keeps working until then.
//!
//! At the next start, `open` puts the staged database in today's place before `Db::open` reads it, keeps this
//! computer's identity (its `devices` row) and its own settings (`scope`), pauses the agents and notes where the data came
//! back from (`last_restore`). A restored database that can't be opened goes to `<data>/restore/failed/`, and Gizai
//! starts with today's. Nothing in the keychain is restored or removed. No Tauri.
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::backups::{self, BackupEntry, Hashing, LastChange, Manifest};
use crate::db::{self, Db};
use crate::{Error, Result, files, ids, scope, settings};

/// The data folder's folder for a restore: what is staged, and `failed/`.
pub const DIR: &str = "restore";
/// The staged restore (`Staged`): a restore is ready once this file is there.
pub const STAGED: &str = "restore.json";
/// Inside `DIR`: the restored databases that couldn't be opened.
pub const FAILED: &str = "failed";
/// The staged database, inside `DIR`…
const STAGED_DB: &str = "gizai.db";
/// …and while it is decompressed and checked.
const CHECKED_DB: &str = "gizai.db.part";

/// A restore's step, as `check` and `stage` say them (`Step::text`: the `step` of `restore-changed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The manifest is read and checked.
    Snapshot,
    /// The database is decompressed and checked.
    Database,
    /// The blobs the manifest names are checked in the backups folder: so many of so many.
    CheckFiles(usize, usize),
    /// The checked database goes into `restore/`.
    CopyDatabase,
    /// The blobs the data folder lacks are copied into `restore/files/`: so many of so many.
    CopyFiles(usize, usize),
}

impl Step {
    pub fn text(self) -> String {
        match self {
            Step::Snapshot => "Checking the snapshot".into(),
            Step::Database => "Checking the database".into(),
            Step::CheckFiles(n, of) => format!("Checking files ({n} of {of})"),
            Step::CopyDatabase => "Copying the database".into(),
            Step::CopyFiles(n, of) => format!("Copying files ({n} of {of})"),
        }
    }
}

/// The before-restore snapshot of today's data: its name, when it was made and its backups folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub name: String,
    pub made_at: i64,
    pub folder: String,
}

/// `<data>/restore/restore.json`: a restore that is ready, waiting for the next start.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Staged {
    /// "ready".
    pub state: String,
    /// The backups folder it came from.
    pub folder: String,
    /// The snapshot's manifest, as it was checked.
    pub manifest: Manifest,
    /// The before-restore snapshot of today's data.
    pub saved_today: Saved,
    /// When it was staged.
    pub staged_at: i64,
    /// Set at the start, just before today's database moves to `backups/`: the name it gets there. A start that finds
    /// it set knows the swap was interrupted, and where it was (`swap`).
    pub swapping: Option<String>,
}

/// `last_restore`, one of this computer's settings: where this computer's data starts, for a sync.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastRestore {
    /// The snapshot's name…
    pub snapshot: String,
    /// …the computer that made it (its `devices.id`)…
    pub made_by: String,
    /// …when (ms)…
    pub made_at: i64,
    /// …and its manifest's `last_change`.
    pub last_change: Option<LastChange>,
    pub restored_at: i64,
    /// The before-restore snapshot of the data it replaced.
    pub saved_today: String,
}

/// How the restore went at the start (contract BR's `result`), kept as `restore_result` until it is dismissed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub ok: bool,
    /// When (ms).
    pub at: i64,
    /// The snapshot's name.
    pub snapshot: String,
    /// The name of the computer that made it.
    pub from_computer: String,
    pub message: String,
}

/// What a restore is doing now, for the window (contract BR's `RestoreStatus`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreStatus {
    /// "none", "checking", "saving", "staging", "ready" or "failed".
    pub state: String,
    /// The backups folder it restores from.
    pub folder: Option<String>,
    pub snapshot: Option<BackupEntry>,
    /// The step it is at: "Checking the database", "Saving today's data", "Copying files (120 of 300)".
    pub step: Option<String>,
    /// Why it failed (state failed).
    pub error: Option<String>,
    /// The before-restore snapshot's name, once it is made.
    pub saved_today: Option<String>,
    /// How the last restore went at the start, until it is dismissed.
    pub result: Option<RestoreResult>,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

/// The data folder's restore folder.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join(DIR)
}

// ---- 1. the checks ----

/// Checks snapshot `name` in backups folder `dir` (a backups folder, as `backups::backups_dir` gives it), each part as
/// `step` hears: its manifest, its database (decompressed into `<data>/restore/gizai.db.part`, which stays for `stage`)
/// and every blob it names. Returns the manifest; an error says the first thing that is wrong, and leaves no database
/// behind. An error from `step` stops it there.
pub fn check(data_dir: &Path, dir: &Path, name: &str, step: &mut dyn FnMut(Step) -> Result<()>) -> Result<Manifest> {
    step(Step::Snapshot)?;
    let m = read_manifest(dir, name)?;
    step(Step::Database)?;
    let rdir = self::dir(data_dir);
    fs::create_dir_all(&rdir)?;
    let checked = rdir.join(CHECKED_DB);
    let out = check_db(dir, name, &m, &checked).and_then(|()| check_files(dir, &m, step));
    if out.is_err() {
        remove_checked(data_dir);
    }
    out.map(|()| m)
}

/// The manifest of snapshot `name`, refused when it isn't a snapshot's, a newer Gizai made it, or it doesn't read as
/// format 1's.
fn read_manifest(dir: &Path, name: &str) -> Result<Manifest> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(invalid(format!("\"{name}\" isn't a snapshot's name")));
    }
    let path = dir.join("snapshots").join(format!("{name}.json"));
    let raw = fs::read(&path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => invalid(format!("There is no snapshot {name} in {}", dir.display())),
        _ => invalid(format!("{} can't be read: {e}", path.display())),
    })?;
    let v: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| invalid(format!("{} can't be read: {e}", path.display())))?;
    if v.get("format").and_then(|f| f.as_str()) != Some(backups::MANIFEST_FORMAT) {
        return Err(invalid(format!("{} isn't a snapshot's manifest", path.display())));
    }
    let number = |key: &str| v.get(key).and_then(|x| x.as_i64()).unwrap_or(0);
    if number("format_version") > backups::FORMAT_VERSION {
        return Err(invalid(format!("Made by a newer Gizai (backup format {}). Update Gizai first.", number("format_version"))));
    }
    if number("schema_version") > db::SCHEMA_VERSION {
        return Err(invalid(format!("Made by a newer Gizai (database version {}). Update Gizai first.", number("schema_version"))));
    }
    serde_json::from_value(v).map_err(|e| invalid(format!("{} can't be read as a snapshot's manifest: {e}", path.display())))
}

/// Decompresses the snapshot's database into `dest` and checks it: its sha256 and size (`backups::unpack_db`),
/// `PRAGMA integrity_check`, and its `user_version` is the manifest's `schema_version`.
fn check_db(dir: &Path, name: &str, m: &Manifest, dest: &Path) -> Result<()> {
    let src = dir.join("snapshots").join(format!("{name}.db.zst"));
    if !src.is_file() {
        return Err(invalid(format!("The snapshot's database file is missing: {}", src.display())));
    }
    // the data folder takes it (a full disk says so as itself, not as a damaged snapshot)
    fs::write(dest, b"")?;
    backups::unpack_db(dir, name, dest).map_err(|e| match e {
        Error::Invalid(_) => invalid(format!("The snapshot's database file has changed since the snapshot was made: {} doesn't match its manifest",
                                             src.display())),
        e => invalid(format!("The snapshot's database file can't be decompressed: {e}")),
    })?;
    let damaged = |e: rusqlite::Error| invalid(format!("The snapshot's database is damaged: {e}"));
    let conn = Connection::open_with_flags(dest, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(damaged)?;
    let said: Vec<String> = (|| {
        let mut st = conn.prepare("PRAGMA integrity_check")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.take(5).collect::<rusqlite::Result<Vec<String>>>()
    })().map_err(damaged)?;
    if said != ["ok"] {
        return Err(invalid(format!("The snapshot's database is damaged: {}", said.join("; "))));
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(damaged)?;
    if version != m.schema_version {
        return Err(invalid(format!("The snapshot's database is version {version}, not {} as its manifest says", m.schema_version)));
    }
    Ok(())
}

/// Every blob the manifest names is in backups folder `dir`, with its size and hash: first all of them by size (quick, so
/// a missing one is found at once), then each one's hash.
fn check_files(dir: &Path, m: &Manifest, step: &mut dyn FnMut(Step) -> Result<()>) -> Result<()> {
    let total = m.files.len();
    let mut missing = 0;
    for f in &m.files {
        let sha = blob_name(&f.sha256)?;
        let path = files::blob_path(dir, &sha);
        match fs::metadata(&path) {
            Ok(md) if md.is_file() => {
                if md.len() as i64 != f.bytes {
                    return Err(changed(&path));
                }
            }
            _ => missing += 1,
        }
    }
    if missing > 0 {
        let what = if missing == 1 { "1 file of the snapshot is".to_string() } else { format!("{missing} files of the snapshot are") };
        return Err(invalid(format!("{what} missing from {}", dir.join("files").display())));
    }
    for (i, f) in m.files.iter().enumerate() {
        step(Step::CheckFiles(i, total))?;
        let sha = blob_name(&f.sha256)?;
        let path = files::blob_path(dir, &sha);
        let mut h = Hashing::new(io::sink());
        io::copy(&mut fs::File::open(&path).map_err(|e| invalid(format!("{} can't be read: {e}", path.display())))?, &mut h)?;
        if h.sum().0 != sha {
            return Err(changed(&path));
        }
    }
    if total > 0 {
        step(Step::CheckFiles(total, total))?;
    }
    Ok(())
}

/// A blob's name from a manifest's `sha256`: 64 lowercase hex, nothing that leaves `files/`.
fn blob_name(sha256: &str) -> Result<String> {
    let sha = sha256.to_ascii_lowercase();
    if !backups::is_hex(&sha, 64) {
        return Err(invalid(format!("The snapshot names a file \"{sha256}\" that isn't a file's hash")));
    }
    Ok(sha)
}

fn changed(path: &Path) -> Error {
    invalid(format!("{} has changed since the snapshot was made: it doesn't match the snapshot", path.display()))
}

/// Removes the database `check` decompressed, and the restore folder when nothing else is in it.
pub fn remove_checked(data_dir: &Path) {
    let rdir = dir(data_dir);
    let _ = fs::remove_file(rdir.join(CHECKED_DB));
    let _ = fs::remove_dir(&rdir);
}

// ---- 3. staging ----

/// Stages the restore `check` checked (`m`, from backups folder `dir`), after today's data was saved as `saved`: the
/// checked database becomes `<data>/restore/gizai.db`, the blobs `<data>/files/` lacks are copied from the backups folder
/// into `<data>/restore/files/` (each through a `.part` file, hashed while it is copied), and `restore.json` is written,
/// last. The blobs the data folder has already get their modified time set to now, so the housekeeping, which leaves
/// blobs changed in the last day alone, keeps them until the restart. On an error nothing stays staged.
pub fn stage(data_dir: &Path, dir: &Path, m: &Manifest, saved: &Saved, step: &mut dyn FnMut(Step) -> Result<()>) -> Result<Staged> {
    let out = stage_in(data_dir, dir, m, saved, step);
    if out.is_err() {
        let _ = cancel(data_dir);
    }
    out
}

fn stage_in(data_dir: &Path, dir: &Path, m: &Manifest, saved: &Saved, step: &mut dyn FnMut(Step) -> Result<()>) -> Result<Staged> {
    let rdir = self::dir(data_dir);
    step(Step::CopyDatabase)?;
    let checked = rdir.join(CHECKED_DB);
    if !checked.is_file() {
        return Err(invalid("The checked database is missing: check the snapshot again"));
    }
    backups::rename(&checked, &rdir.join(STAGED_DB))?;
    let mut lacking = Vec::new();
    for f in &m.files {
        let sha = blob_name(&f.sha256)?;
        let have = files::blob_path(data_dir, &sha);
        if fs::metadata(&have).is_ok_and(|md| md.is_file()) {
            let _ = files::touch(&have);
        } else if !lacking.contains(&sha) {
            lacking.push(sha);
        }
    }
    let total = lacking.len();
    for (i, sha) in lacking.iter().enumerate() {
        step(Step::CopyFiles(i, total))?;
        let src = files::blob_path(dir, sha);
        let dest = files::blob_path(&rdir, sha);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let part = dest.with_extension("part");
        let mut from = fs::File::open(&src).map_err(|e| invalid(format!("{} can't be read: {e}", src.display())))?;
        let mut out = Hashing::new(fs::File::create(&part)?);
        io::copy(&mut from, &mut out)?;
        out.inner.sync_all()?;
        if out.sum().0 != *sha {
            let _ = fs::remove_file(&part);
            return Err(changed(&src));
        }
        backups::rename(&part, &dest)?;
    }
    if total > 0 {
        step(Step::CopyFiles(total, total))?;
    }
    let staged = Staged {
        state: "ready".into(),
        folder: dir.to_string_lossy().into_owned(),
        manifest: m.clone(),
        saved_today: saved.clone(),
        staged_at: ids::now_ms(),
        swapping: None,
    };
    write_staged(&rdir, &staged)?;
    Ok(staged)
}

/// Writes `restore.json` whole: through a `.part` file.
fn write_staged(rdir: &Path, staged: &Staged) -> Result<()> {
    let part = rdir.join(format!("{STAGED}.part"));
    let mut json = serde_json::to_vec_pretty(staged)?;
    json.push(b'\n');
    backups::write_synced(&part, &json)?;
    backups::rename(&part, &rdir.join(STAGED))?;
    Ok(())
}

/// The restore that is ready in data folder `data_dir`, if one is.
pub fn staged(data_dir: &Path) -> Option<Staged> {
    read_staged(&dir(data_dir)).ok().flatten().filter(|s| s.state == "ready")
}

/// `restore.json` in restore folder `rdir`: None when there is none.
fn read_staged(rdir: &Path) -> Result<Option<Staged>> {
    match fs::read(rdir.join(STAGED)) {
        Ok(raw) => Ok(Some(serde_json::from_slice(&raw)?)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Removes the staged restore, or what a restore that stopped left: `restore.json` first (from then on nothing is
/// staged), then the database and the blobs. `restore/failed/` stays. The before-restore snapshot stays in its folder.
pub fn cancel(data_dir: &Path) -> Result<()> {
    let rdir = dir(data_dir);
    remove_file_if_there(&rdir.join(STAGED))?;
    let _ = fs::remove_file(rdir.join(format!("{STAGED}.part")));
    for (name, path) in backups::entries(&rdir) {
        if name == FAILED {
            continue;
        }
        let gone = if path.is_dir() { remove_tree(&path) } else { fs::remove_file(&path) };
        if let Err(e) = gone
            && e.kind() != io::ErrorKind::NotFound
        {
            return Err(e.into());
        }
    }
    let _ = fs::remove_dir(&rdir);
    Ok(())
}

fn remove_file_if_there(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Removes a folder and all in it; on Windows a read-only file (a blob) is made writable first.
fn remove_tree(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        #[cfg(windows)]
        Err(_) => {
            let mut all = Vec::new();
            files_in(path, &mut all);
            for f in all {
                if let Ok(md) = fs::metadata(&f) {
                    let mut perm = md.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    perm.set_readonly(false);
                    let _ = fs::set_permissions(&f, perm);
                }
            }
            fs::remove_dir_all(path)
        }
        #[cfg(not(windows))]
        Err(e) => Err(e),
    }
}

#[cfg(windows)]
fn files_in(dir: &Path, out: &mut Vec<PathBuf>) {
    for (_, p) in backups::entries(dir) {
        if p.is_dir() { files_in(&p, out) } else { out.push(p) }
    }
}

// ---- at the next start ----

/// Opens the database in data folder `data_dir` (`<data>/gizai.db`) as `Db::open` does. When a restore is ready there,
/// it is applied first (`swap`): today's database moves to `backups/gizai-before-restore-<stamp>.db`, the restored one
/// takes its place (with this computer's device, this computer's settings as they were today, the agents paused and
/// `last_restore`), the staged blobs go into `files/`, and `Db::open` upgrades an older schema as usual. When the
/// restored database can't be opened, it goes to `restore/failed/` and today's comes back. Either way `restore/` goes
/// (apart from `restore/failed/`), and the result is kept as `restore_result` in the database that opened, until
/// dismissed. Without a staged restore it is `Db::open`, nothing else.
pub fn open(data_dir: &Path) -> Result<(Db, Option<RestoreResult>)> {
    let path = data_dir.join("gizai.db");
    let rdir = dir(data_dir);
    if !rdir.join(STAGED).exists() {
        return Ok((Db::open(&path)?, None));
    }
    let now = ids::now_ms();
    let staged = match read_staged(&rdir) {
        Ok(Some(s)) if s.state == "ready" => s,
        found => {
            let why = match found {
                Err(e) => format!("its restore.json can't be read: {e}"),
                _ => "its restore.json doesn't say it is ready".to_string(),
            };
            let _ = cancel(data_dir);
            let result = RestoreResult { ok: false, at: now, snapshot: String::new(), from_computer: String::new(),
                message: format!("Gizai couldn't restore the snapshot it had ready ({why}), so it started with today's data.") };
            return opened_with(&path, result);
        }
    };
    let m = &staged.manifest;
    let failed = |why: String| RestoreResult { ok: false, at: now, snapshot: m.name.clone(), from_computer: m.device.name.clone(),
                                               message: why };
    let before = match swap(data_dir, &staged, now) {
        Ok(before) => before,
        Err(why) => {
            let aside = set_aside(&rdir, &rdir.join(STAGED_DB), &m.name);
            let _ = cancel(data_dir);
            let kept = aside.map(|p| format!(" The snapshot's database is in {}.", p.display())).unwrap_or_default();
            return opened_with(&path, failed(format!("Gizai couldn't restore {}, so it started with today's data: {why}.{kept}", m.name)));
        }
    };
    match Db::open(&path) {
        Ok(db) => {
            let _ = cancel(data_dir);
            let result = RestoreResult {
                ok: true,
                at: now,
                snapshot: m.name.clone(),
                from_computer: m.device.name.clone(),
                message: format!("Gizai restored {} from {}. Today's data was saved as {} first. The agents are paused: start them \
                                  again when you're ready.", m.name, computer(&m.device.name), staged.saved_today.name),
            };
            keep_result(&db, &result);
            Ok((db, Some(result)))
        }
        Err(e) => {
            // the restored database (and what its open left beside it) goes aside, then today's comes back
            let aside = set_aside(&rdir, &path, &m.name);
            for end in ["-wal", "-shm", "-journal"] {
                let _ = fs::remove_file(with_end(&path, end));
            }
            if let Err(back) = backups::rename(&before, &path) {
                return Err(invalid(format!("the restored data couldn't be opened ({e}), and today's couldn't be put back from {}: {back}",
                                           before.display())));
            }
            let _ = cancel(data_dir);
            let kept = aside.map(|p| format!(" The snapshot's database is in {}.", p.display())).unwrap_or_default();
            opened_with(&path, failed(format!("Gizai couldn't open the restored data of {}, so it started with today's data: {e}.{kept}", m.name)))
        }
    }
}

fn computer(name: &str) -> &str {
    if name.trim().is_empty() { "another computer" } else { name }
}

/// Opens today's database after a restore that didn't happen, with its result.
fn opened_with(path: &Path, result: RestoreResult) -> Result<(Db, Option<RestoreResult>)> {
    let db = Db::open(path)?;
    keep_result(&db, &result);
    Ok((db, Some(result)))
}

fn keep_result(db: &Db, result: &RestoreResult) {
    if let Err(e) = settings::set(db, scope::RESTORE_RESULT, &Some(result)) {
        eprintln!("gizai: keeping the restore's result failed: {e}");
    }
}

/// How the last restore went, until it is dismissed.
pub fn result(db: &Db) -> Option<RestoreResult> {
    settings::get::<Option<RestoreResult>>(db, scope::RESTORE_RESULT).ok().flatten().flatten()
}

/// Dismisses the last restore's result.
pub fn dismiss_result(db: &Db) -> Result<()> {
    settings::set(db, scope::RESTORE_RESULT, &None::<RestoreResult>)
}

/// What `last_restore` says: where this computer's data came back from.
pub fn last_restore(db: &Db) -> Option<LastRestore> {
    settings::get::<LastRestore>(db, scope::LAST_RESTORE).ok().flatten()
}

/// `<path><end>`: a database's `-wal`, `-shm` or `-journal`.
fn with_end(path: &Path, end: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(end);
    PathBuf::from(name)
}

/// Moves a restored database that won't do into `restore/failed/<snapshot>.db` (replacing an older one of the same
/// snapshot). Where it went, or None when it isn't there (or can't be moved: then it is removed).
fn set_aside(rdir: &Path, db_file: &Path, snapshot: &str) -> Option<PathBuf> {
    if !db_file.is_file() {
        return None;
    }
    let failed = rdir.join(FAILED);
    let dest = failed.join(format!("{}.db", if snapshot.is_empty() { "snapshot" } else { snapshot }));
    let moved = fs::create_dir_all(&failed).and_then(|()| backups::rename(db_file, &dest));
    match moved {
        Ok(()) => Some(dest),
        Err(e) => {
            eprintln!("gizai: moving the restored database {} to {} failed: {e}", db_file.display(), failed.display());
            let _ = fs::remove_file(db_file);
            None
        }
    }
}

/// This computer as today's database knows it: its `devices` row, and its own settings (`scope::this_computer`).
struct Today {
    device: (String, String, i64),
    settings: Vec<(String, String, i64)>,
}

/// Puts the staged database in today's place (step 3 of the card, in an order that leaves today's data whole whatever
/// fails): this computer's device and settings are read from today's database, whose WAL is folded back into it; the
/// staged database gets them; today's moves to `backups/gizai-before-restore-<stamp>.db`; the staged one takes its place;
/// the staged blobs go into `files/` (a blob that is there already stays). Returns where today's database went. On an
/// error today's database is in place, as it was.
///
/// `restore.json` says `swapping` (today's new name) just before today's database moves, so a start after a swap that
/// was interrupted there (the computer went off) carries on from where it was.
fn swap(data_dir: &Path, staged: &Staged, now: i64) -> std::result::Result<PathBuf, String> {
    let path = data_dir.join("gizai.db");
    let rdir = dir(data_dir);
    let restored = rdir.join(STAGED_DB);
    let backups_dir = data_dir.join("backups");
    if let Some(before) = staged.swapping.as_deref().map(|b| backups_dir.join(b))
        && before.is_file()
    {
        if !path.exists() && restored.is_file() {
            // today's had moved, the restored one hadn't yet
            if let Err(e) = backups::rename(&restored, &path) {
                let _ = backups::rename(&before, &path);
                return Err(format!("the restored database couldn't take today's place: {e}"));
            }
            move_blobs(&rdir, data_dir);
            return Ok(before);
        }
        if path.is_file() && !restored.exists() {
            // both had moved: only the blobs were left
            move_blobs(&rdir, data_dir);
            return Ok(before);
        }
    }
    if !path.is_file() {
        return Err(format!("today's database {} is missing", path.display()));
    }
    if !restored.is_file() {
        return Err("the staged database is missing".into());
    }
    let today = read_today(&path).map_err(|e| format!("today's database couldn't be read: {e}"))?;
    adopt(&restored, &today, staged, now).map_err(|e| format!("the restored database couldn't take this computer's settings: {e}"))?;
    fs::create_dir_all(&backups_dir).map_err(|e| format!("{} can't be made: {e}", backups_dir.display()))?;
    let before = free_before_name(&backups_dir, now);
    let name = before.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    write_staged(&rdir, &Staged { swapping: Some(name), ..staged.clone() }).map_err(|e| format!("restore.json can't be written: {e}"))?;
    backups::rename(&path, &before).map_err(|e| format!("today's database couldn't move to {}: {e}", before.display()))?;
    if let Err(e) = backups::rename(&restored, &path) {
        let _ = backups::rename(&before, &path);
        return Err(format!("the restored database couldn't take today's place: {e}"));
    }
    move_blobs(&rdir, data_dir);
    Ok(before)
}

/// `backups/gizai-before-restore-<stamp>.db`, a name no file has.
fn free_before_name(backups_dir: &Path, now: i64) -> PathBuf {
    let stamp = db::stamp(now);
    let mut p = backups_dir.join(format!("gizai-before-restore-{stamp}.db"));
    let mut n = 2;
    while p.exists() {
        p = backups_dir.join(format!("gizai-before-restore-{stamp}-{n}.db"));
        n += 1;
    }
    p
}

/// Reads this computer's device and its own settings from today's database at `path`, then folds its WAL back into it
/// (`PRAGMA wal_checkpoint(TRUNCATE)`): once this, the only connection, closes, the database is one file.
fn read_today(path: &Path) -> Result<Today> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    let device: Option<(String, String, i64)> = conn
        .query_row("SELECT id, name, created_at FROM devices WHERE is_self = 1 ORDER BY created_at LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    // a database without this computer (never: Db::init makes it) gets a new one, never the snapshot's
    let device = device.unwrap_or_else(|| {
        (ids::new_id(), std::env::var("HOSTNAME").unwrap_or_else(|_| "this machine".into()), ids::now_ms())
    });
    let settings = {
        let mut st = conn.prepare("SELECT key, value_json, updated_at FROM settings WHERE org_id = '' ORDER BY key")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?.into_iter().filter(|(k, _, _)| scope::this_computer(k)).collect()
    };
    let (busy, _, _): (i64, i64, i64) = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    if busy != 0 {
        return Err(invalid("its WAL couldn't be folded back into it (the database is busy)"));
    }
    drop(conn);
    let wal = with_end(path, "-wal");
    if fs::metadata(&wal).is_ok_and(|m| m.len() > 0) {
        return Err(invalid(format!("{} still holds changes", wal.display())));
    }
    for end in ["-wal", "-shm"] {
        let _ = fs::remove_file(with_end(path, end));
    }
    Ok(Today { device, settings })
}

/// The staged database at `restored` becomes this computer's, on a plain connection before `Db::init` reads it, in one
/// transaction:
/// - when its `is_self` device isn't this computer's, every row gets `is_self = 0` and this computer's row (added, or
///   updated) `is_self = 1`: the snapshot's computer stays in `devices` as another computer. This computer's own
///   snapshot leaves `devices` as it was;
/// - today's values of this computer's settings go over the restored ones (a restored one stays only where today had
///   none);
/// - `agents_paused` is true, and `last_restore` says where the data came back from.
fn adopt(restored: &Path, today: &Today, staged: &Staged, now: i64) -> Result<()> {
    let mut conn = Connection::open(restored)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    let tx = conn.transaction()?;
    let (id, name, created_at) = &today.device;
    let selves: Vec<String> = {
        let mut st = tx.prepare("SELECT id FROM devices WHERE is_self = 1")?;
        st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    if selves != [id.clone()] {
        tx.execute("UPDATE devices SET is_self = 0 WHERE is_self <> 0", [])?;
        tx.execute("INSERT INTO devices(id, created_at, name, is_self) VALUES (?1, ?2, ?3, 1)
                    ON CONFLICT(id) DO UPDATE SET is_self = 1, name = excluded.name, created_at = excluded.created_at",
                   params![id, created_at, name])?;
    }
    let set = |key: &str, json: &str, at: i64| {
        tx.execute("INSERT INTO settings(key, org_id, value_json, updated_at) VALUES (?1, '', ?2, ?3)
                    ON CONFLICT(key, org_id) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
                   params![key, json, at])
    };
    for (key, json, at) in &today.settings {
        set(key, json, *at)?;
    }
    set("agents_paused", "true", now)?;
    let m = &staged.manifest;
    let last = LastRestore {
        snapshot: m.name.clone(),
        made_by: m.device.id.clone(),
        made_at: m.made_at,
        last_change: m.last_change.clone(),
        restored_at: now,
        saved_today: staged.saved_today.name.clone(),
    };
    set(scope::LAST_RESTORE, &serde_json::to_string(&last)?, now)?;
    tx.commit()?;
    Ok(())
}

/// Moves the staged blobs (`restore/files/<xx>/<sha256>`) into the data folder's `files/`: a blob that is there
/// already stays, and the staged copy goes. Each one moved is made read-only, as the data folder's blobs are.
fn move_blobs(rdir: &Path, data_dir: &Path) {
    for (prefix, sub) in backups::entries(&rdir.join("files")) {
        if !backups::is_hex(&prefix, 2) {
            continue;
        }
        for (name, path) in backups::entries(&sub) {
            if !backups::is_hex(&name, 64) || !name.starts_with(&prefix) {
                continue;
            }
            let dest = files::blob_path(data_dir, &name);
            if fs::symlink_metadata(&dest).is_ok() {
                continue;
            }
            let moved = dest.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| backups::rename(&path, &dest));
            match moved {
                Ok(()) => {
                    if let Ok(md) = fs::metadata(&dest) {
                        let mut perm = md.permissions();
                        perm.set_readonly(true);
                        let _ = fs::set_permissions(&dest, perm);
                    }
                }
                Err(e) => eprintln!("gizai: moving the restored file {} into {} failed: {e}", path.display(), dest.display()),
            }
        }
    }
}
