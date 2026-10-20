//! Backups (GA-216): snapshots of Gizai's data in a backups folder, in format 1 (docs/backup-format.md). Gizai Cloud's
//! backups will be written in the same format, so one restore reads both. A snapshot is the database, copied while Gizai
//! runs (`VACUUM INTO`) and compressed with zstd, and the blobs it names, stored once by content and shared by all
//! snapshots. Its manifest says what it holds, and how far into `changes` it goes: where a sync continues.
//!
//! Settings name the folder (a USB disk, or a folder Dropbox or Syncthing keeps), how often (daily, weekly or only by
//! hand), On quit and how many to keep. Several computers may share one folder: each writes and removes only its own
//! snapshots, and a blob goes only when no manifest names it and it hasn't been modified for 7 days. No Tauri: the
//! schedule, the quit and the events are src-tauri's `backups`.
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::{self, Db};
use crate::settings::{get_in, set_in};
use crate::{Error, Result, files, ids};

/// The file that makes a folder a backups folder…
pub const MARKER: &str = "gizai-backups.json";
/// …and what it says.
pub const MARKER_JSON: &str = r#"{"format":"gizai-backups","format_version":1}"#;
/// The backups folder Gizai makes inside a chosen folder that isn't one itself.
pub const FOLDER_NAME: &str = "Gizai backups";
/// The format this Gizai writes and reads: the marker's and each manifest's `format_version`.
pub const FORMAT_VERSION: i64 = 1;
/// A manifest's `format`.
pub const MANIFEST_FORMAT: &str = "gizai-backup";
/// The marker's `format`.
const FOLDER_FORMAT: &str = "gizai-backups";
/// zstd's level for the database.
pub const ZSTD_LEVEL: i32 = 3;
/// Keep: how many of this computer's daily, weekly, manual and quit snapshots stay, 1 to KEEP_MAX.
pub const KEEP_DEFAULT: i64 = 10;
pub const KEEP_MAX: i64 = 100;
pub const HOUR_MS: i64 = 60 * 60 * 1000;
pub const DAY_MS: i64 = 24 * HOUR_MS;
pub const WEEK_MS: i64 = 7 * DAY_MS;
/// A `.part` file of this computer's this old was left by a snapshot that was interrupted: the next snapshot removes it.
pub const PART_AGE_MS: i64 = HOUR_MS;
/// A blob no manifest names goes once it hasn't been modified for this long: a snapshot another computer is making may
/// have found it (it sets the blob's modified time to now) before that snapshot's manifest is written.
pub const BLOB_AGE_MS: i64 = 7 * DAY_MS;
/// A scheduled snapshot counts as due this much before its day (week) is up: the hourly check comes a moment before the
/// 24 hours are up each day, and would otherwise move the daily snapshot an hour later every day.
pub const EARLY_MS: i64 = 5 * 60 * 1000;

/// This computer's settings (a restore keeps them): the chosen folder (null: backups are off)…
pub const FOLDER_KEY: &str = "backup_folder";
/// …"daily", "weekly" or "manual"…
pub const EVERY_KEY: &str = "backup_every";
/// …whether Quit Gizai completely backs up first…
pub const ON_QUIT_KEY: &str = "backup_on_quit";
/// …and Keep.
pub const KEEP_KEY: &str = "backup_keep";
/// The last snapshot that failed: when and why (`LastError`); null once one has been made since.
pub const LAST_ERROR_KEY: &str = "backup_last_error";

/// The reasons Keep counts (and removes past Keep): all but before-restore.
const KEPT_REASONS: [&str; 4] = ["daily", "weekly", "manual", "quit"];

/// The backups folder's README.txt.
const README: &str = "Gizai backups

This folder holds snapshots of Gizai's data, made by Gizai on each computer that backs up here: its database and the
files it stores (attachments and screenshots). Gizai makes them on a schedule, when you press Back up now, and when
you quit Gizai if you asked it to.

snapshots/   one snapshot is two files: <name>.json says what it holds, <name>.db.zst is its database (zstd)
files/       every stored file, once, named by its content (SHA-256), shared by all snapshots

Several computers can back up to this folder: each adds and removes only its own snapshots. Don't rename or change
the files here. They are not encrypted: whoever can open this folder can read them.

Restore in Gizai: Settings → General → Backups.
";

/// Why a snapshot was made: its name and its manifest say so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Daily,
    Weekly,
    /// Back up now.
    Manual,
    /// Quit Gizai completely, with On quit on.
    Quit,
    /// Before a restore (GA-218): Keep neither counts nor removes it.
    BeforeRestore,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Daily => "daily",
            Reason::Weekly => "weekly",
            Reason::Manual => "manual",
            Reason::Quit => "quit",
            Reason::BeforeRestore => "before-restore",
        }
    }
}

/// How often a snapshot is made on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Every {
    #[default]
    Daily,
    Weekly,
    /// Only Back up now (and On quit).
    Manual,
}

impl Every {
    /// How old this computer's newest snapshot is when the next one is due; None: never on its own.
    pub fn interval_ms(self) -> Option<i64> {
        match self {
            Every::Daily => Some(DAY_MS),
            Every::Weekly => Some(WEEK_MS),
            Every::Manual => None,
        }
    }

    /// The reason a scheduled snapshot has.
    pub fn reason(self) -> Option<Reason> {
        match self {
            Every::Daily => Some(Reason::Daily),
            Every::Weekly => Some(Reason::Weekly),
            Every::Manual => None,
        }
    }
}

/// This computer's backup settings (contract BK's `BackupSettings`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSettings {
    /// The chosen folder; None: backups are off.
    pub folder: Option<String>,
    pub every: Every,
    /// Quit Gizai completely backs up first.
    pub on_quit: bool,
    /// How many of this computer's snapshots stay, 1 to 100.
    pub keep: i64,
}

impl Default for BackupSettings {
    fn default() -> Self {
        BackupSettings { folder: None, every: Every::Daily, on_quit: false, keep: KEEP_DEFAULT }
    }
}

/// A snapshot's manifest, `snapshots/<name>.json`: the snapshot exists once it does. Format 1, field by field in
/// docs/backup-format.md, in the card's order. Read leniently (a field it lacks reads as empty), so one from a newer
/// Gizai still lists, as not restorable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    /// "gizai-backup".
    pub format: String,
    pub format_version: i64,
    pub name: String,
    pub reason: String,
    /// When the database was copied (ms).
    pub made_at: i64,
    pub app_version: String,
    /// The copied database's `PRAGMA user_version`.
    pub schema_version: i64,
    pub device: Device,
    pub db: DbFile,
    /// The copied database's newest `changes` row: where a sync continues. None when it has none.
    pub last_change: Option<LastChange>,
    /// Every blob the copied database names, once, by hash.
    pub files: Vec<FileEntry>,
    /// The hashes among `files` the snapshot couldn't store: neither the data folder nor the backups folder had them.
    pub files_missing: Vec<String>,
}

/// The computer that made a snapshot: its `devices` row.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Device {
    pub id: String,
    pub name: String,
}

/// A snapshot's database file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbFile {
    /// `snapshots/<name>.db.zst`, from the backups folder.
    pub path: String,
    /// "zstd".
    pub compression: String,
    /// The compressed file's size.
    pub bytes: i64,
    /// The uncompressed database's sha256, 64 lowercase hex.
    pub sha256: String,
    /// The uncompressed database's size.
    pub db_bytes: i64,
}

/// A `changes` row: its `seq` (this database's order), `id` and `hlc` (the same on every computer).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LastChange {
    pub seq: i64,
    pub id: String,
    pub hlc: String,
}

/// A blob a snapshot names: `files/<sha256[..2]>/<sha256>` in the backups folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileEntry {
    pub sha256: String,
    /// Its size; for a missing one, the size the database gives it (0 when it gives none).
    pub bytes: i64,
}

/// One snapshot in a backups folder, as `list` gives it (contract BK's `BackupEntry`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub name: String,
    pub reason: String,
    pub made_at: i64,
    pub device_id: String,
    pub device_name: String,
    /// This computer made it (the same `devices.id`).
    pub this_computer: bool,
    pub app_version: String,
    pub schema_version: i64,
    /// The database's size, uncompressed (the manifest's `db.db_bytes`).
    pub db_bytes: i64,
    /// How many blobs it names, their size together, and how many of them the folder's `files/` doesn't have now.
    pub files_count: i64,
    pub files_bytes: i64,
    pub files_missing: i64,
    /// Whether this Gizai can restore it; `problem` says why not.
    pub restorable: bool,
    pub problem: Option<String>,
}

/// A snapshot that runs now.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub reason: Reason,
    pub started_at: i64,
    /// The step it is at (`Step::as_str`).
    pub step: String,
}

/// This computer's newest good snapshot (`newest`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Last {
    pub name: String,
    pub made_at: i64,
    /// Its compressed database file's size (the blobs are shared by all snapshots).
    pub bytes: i64,
}

/// The last snapshot that failed: when and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastError {
    pub at: i64,
    pub message: String,
}

/// What Settings → General → Backups shows (contract BK's `BackupStatus`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub settings: BackupSettings,
    /// The backups folder in use (`backups_dir` of the chosen one), None when none is chosen.
    pub folder: Option<String>,
    /// Why it can't be used now (`folder_problem`).
    pub folder_problem: Option<String>,
    pub running: Option<Running>,
    /// This computer's newest good snapshot in it.
    pub last: Option<Last>,
    pub last_error: Option<LastError>,
    /// When the next scheduled snapshot is due (`next_due`); None when none is.
    pub next: Option<i64>,
}

/// A snapshot's steps, in order: `make` says each one as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The database is copied into the data folder's `backups/`.
    Database,
    /// The blobs it names go into the folder's `files/`.
    Files,
    /// The copy is compressed into `snapshots/<name>.db.zst`.
    Compress,
    /// The manifest is written: from then on the snapshot exists.
    Manifest,
    /// This computer's snapshots past Keep go, then the blobs no manifest names.
    Keep,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Step::Database => "database",
            Step::Files => "files",
            Step::Compress => "compress",
            Step::Manifest => "manifest",
            Step::Keep => "keep",
        }
    }
}

/// A snapshot `make` made.
#[derive(Debug, Clone)]
pub struct Made {
    /// The backups folder it is in.
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// Blobs copied into the folder now…
    pub copied: usize,
    /// …and blobs it had already (their modified time is now).
    pub found: usize,
    /// What Keep removed after it.
    pub pruned: Pruned,
}

/// What Keep removed: snapshots past Keep, and blobs no manifest names any more.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pruned {
    pub snapshots: usize,
    pub blobs: usize,
}

// ---- settings ----

/// This computer's backup settings: the defaults (no folder, daily, On quit off, Keep 10) where none is saved.
pub fn settings(db: &Db) -> BackupSettings {
    db.read(|c| {
        let d = BackupSettings::default();
        Ok(BackupSettings {
            folder: get_in::<Option<String>>(c, FOLDER_KEY)?.flatten().filter(|f| !f.trim().is_empty()),
            every: get_in(c, EVERY_KEY)?.unwrap_or(d.every),
            on_quit: get_in(c, ON_QUIT_KEY)?.unwrap_or(d.on_quit),
            keep: get_in::<i64>(c, KEEP_KEY)?.filter(|k| (1..=KEEP_MAX).contains(k)).unwrap_or(d.keep),
        })
    })
    .unwrap_or_default()
}

/// Saves the settings once they check out: Keep is 1 to 100, and a folder is a full path to a folder Gizai can write in
/// (`folder_problem`). An empty folder is none: backups are off. Choosing another folder clears the last error, which
/// was the old folder's. Returns them as saved.
pub fn save_settings(db: &Db, s: BackupSettings) -> Result<BackupSettings> {
    if !(1..=KEEP_MAX).contains(&s.keep) {
        return Err(Error::Invalid(format!("Keep must be 1 to {KEEP_MAX} snapshots, not {}", s.keep)));
    }
    let folder = s.folder.map(|f| f.trim().to_string()).filter(|f| !f.is_empty());
    if let Some(f) = &folder {
        if !Path::new(f).is_absolute() {
            return Err(Error::Invalid(format!("The backups folder needs a full path, not \"{f}\"")));
        }
        if let Some(problem) = folder_problem(Path::new(f)) {
            return Err(Error::Invalid(problem));
        }
    }
    let saved = BackupSettings { folder, ..s };
    db.write(None, |w| {
        let before = get_in::<Option<String>>(w.conn(), FOLDER_KEY)?.flatten();
        set_in(w, FOLDER_KEY, &saved.folder)?;
        set_in(w, EVERY_KEY, &saved.every)?;
        set_in(w, ON_QUIT_KEY, &saved.on_quit)?;
        set_in(w, KEEP_KEY, &saved.keep)?;
        if before != saved.folder {
            set_in(w, LAST_ERROR_KEY, &None::<LastError>)?;
        }
        Ok(())
    })?;
    Ok(saved)
}

/// The last snapshot that failed, unless one has been made since.
pub fn last_error(db: &Db) -> Option<LastError> {
    crate::settings::get::<Option<LastError>>(db, LAST_ERROR_KEY).ok().flatten().flatten()
}

/// Keeps a failure as the last error, or clears it (None) after a snapshot that worked.
pub fn set_last_error(db: &Db, error: Option<&LastError>) -> Result<()> {
    crate::settings::set(db, LAST_ERROR_KEY, &error)
}

// ---- the folder ----

/// The backups folder a chosen folder stands for: the folder itself when it holds `gizai-backups.json`, else its
/// `Gizai backups` (made by the first snapshot). Settings' folder, `list`'s and a restore's are read the same way.
pub fn backups_dir(chosen: &Path) -> PathBuf {
    if chosen.join(MARKER).is_file() { chosen.to_path_buf() } else { chosen.join(FOLDER_NAME) }
}

/// Why chosen folder `chosen` can't take snapshots now, in plain words; None when it can. It is missing (a USB disk that
/// isn't plugged in), it isn't a folder, Gizai can't write in it (or in its backups folder, when it has one), or a newer
/// Gizai made its backups folder. Writes and removes a small file to find out.
pub fn folder_problem(chosen: &Path) -> Option<String> {
    if let Some(why) = not_a_folder(chosen) {
        return Some(why);
    }
    let dir = backups_dir(chosen);
    let write_in = match fs::metadata(&dir) {
        Ok(m) if m.is_dir() => {
            if let Err(why) = check_marker(&dir) {
                return Some(why);
            }
            dir
        }
        Ok(_) => return Some(format!("{} is a file, not a folder", dir.display())),
        // the first snapshot makes it
        Err(_) => chosen.to_path_buf(),
    };
    let probe = write_in.join(format!(".gizai-write-check-{}", ids::new_id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            None
        }
        Err(e) => Some(format!("Gizai can't write in {}: {e}", write_in.display())),
    }
}

/// Why `path` isn't a folder that can be read: missing, or a file. None when it is one.
fn not_a_folder(path: &Path) -> Option<String> {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Some(format!("{} can't be found. Is its disk plugged in?", path.display())),
        Err(e) => Some(format!("{} can't be read: {e}", path.display())),
        Ok(m) if !m.is_dir() => Some(format!("{} is a file, not a folder", path.display())),
        Ok(_) => None,
    }
}

/// Whether backups folder `dir` takes this Gizai's snapshots: its `gizai-backups.json`, when it has one, is format 1's.
fn check_marker(dir: &Path) -> std::result::Result<(), String> {
    #[derive(Deserialize)]
    struct Marker {
        #[serde(default)]
        format: String,
        #[serde(default)]
        format_version: i64,
    }
    let path = dir.join(MARKER);
    let raw = match fs::read(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("{} can't be read: {e}", path.display())),
    };
    match serde_json::from_slice::<Marker>(&raw) {
        Ok(m) if m.format == FOLDER_FORMAT && m.format_version > FORMAT_VERSION => Err(format!(
            "A newer Gizai made the backups folder {} (format {}): update Gizai to back up there", dir.display(), m.format_version)),
        Ok(m) if m.format == FOLDER_FORMAT => Ok(()),
        _ => Err(format!("{} doesn't say its folder is a Gizai backups folder", path.display())),
    }
}

/// Makes backups folder `dir` ready for a snapshot, its chosen folder being there: the folder itself, its
/// `gizai-backups.json` and README.txt (each written once), `snapshots/` and `files/`.
fn prepare(dir: &Path, tag: &str) -> Result<()> {
    match fs::create_dir(dir) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e.into()),
        _ => {}
    }
    check_marker(dir).map_err(Error::Invalid)?;
    for sub in ["snapshots", "files"] {
        match fs::create_dir(dir.join(sub)) {
            Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e.into()),
            _ => {}
        }
    }
    write_new(&dir.join(MARKER), format!("{MARKER_JSON}\n").as_bytes(), tag)?;
    write_new(&dir.join("README.txt"), README.as_bytes(), tag)?;
    Ok(())
}

/// Writes `bytes` to `path` unless it is there already (another computer may have written it): through a `.part` file,
/// so nobody reads half of it.
fn write_new(path: &Path, bytes: &[u8], tag: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let part = part_of(path, tag);
    write_synced(&part, bytes)?;
    fs::rename(&part, path)?;
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// `<file>.<device>.part`: one of this computer's files on its way in (a blob, the marker, README.txt).
fn part_of(path: &Path, tag: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{tag}.part"));
    path.with_file_name(name)
}

// ---- making a snapshot ----

/// The device part of a snapshot's name: the last 8 characters of this computer's `devices.id` (a UUIDv7, whose first
/// characters are its creation time), letters and digits only.
pub fn device_tag(device_id: &str) -> String {
    let chars: Vec<char> = device_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let tag: String = chars[chars.len().saturating_sub(8)..].iter().collect();
    if tag.is_empty() { "device".into() } else { tag }
}

/// A snapshot's name: `gizai-<reason>-<stamp>-<device>`, its stamp `db::stamp` (local time), its device `device_tag`.
pub fn snapshot_name(reason: Reason, made_at: i64, tag: &str) -> String {
    format!("gizai-{}-{}-{tag}", reason.as_str(), db::stamp(made_at))
}

/// Makes a snapshot of the data in `data_dir` (its `gizai.db`, and the blobs it names in `files/`) in the backups folder
/// chosen folder `chosen` stands for (`backups_dir`), with this reason; then Keep (`prune`, with `keep`). Blocks until it
/// is done, so run it off the main thread, and one at a time. Gizai stays usable meanwhile.
///
/// In the order docs/backup-format.md gives, so an interrupted snapshot leaves no manifest, only `.part` files (and its
/// temporary copy), which a later one removes once they are an hour old. A failure removes what this one wrote and
/// hasn't finished, and its temporary copy. `step` hears each step as it starts; an error from it stops the snapshot
/// there, as a failure would (at Keep the snapshot is made already, and Keep goes on).
pub fn make(data_dir: &Path, chosen: &Path, reason: Reason, keep: i64, step: &mut dyn FnMut(Step) -> Result<()>) -> Result<Made> {
    let src = data_dir.join("gizai.db");
    if !src.is_file() {
        return Err(Error::Invalid(format!("no Gizai data in {}", data_dir.display())));
    }
    if let Some(problem) = folder_problem(chosen) {
        return Err(Error::Invalid(problem));
    }
    let dir = backups_dir(chosen);
    let conn = Connection::open(&src)?;
    conn.busy_timeout(Duration::from_secs(10))?;
    let device = conn
        .query_row("SELECT id, name FROM devices WHERE is_self = 1 ORDER BY created_at LIMIT 1", [], |r| {
            Ok(Device { id: r.get(0)?, name: r.get(1)? })
        })
        .optional()?
        .ok_or_else(|| Error::Invalid("this computer has no device row in the database".into()))?;
    let tag = device_tag(&device.id);
    prepare(&dir, &tag)?;
    remove_old_parts(&dir, data_dir, &tag, ids::now_ms());
    let snaps = dir.join("snapshots");
    let (name, made_at) = free_name(&snaps, reason, &tag)?;
    let temp_dir = data_dir.join("backups");
    fs::create_dir_all(&temp_dir)?;
    let copy = temp_dir.join(format!(".snapshot-{name}.db"));
    let mut undo = Unfinished::default();
    for end in ["", "-wal", "-shm", "-journal"] {
        undo.temp.push(temp_dir.join(format!(".snapshot-{name}.db{end}")));
    }

    // 1. A consistent copy, while Gizai goes on (as db::snapshot makes one). It becomes one file whatever its journal
    // mode: a restore needs nothing beside it.
    step(Step::Database)?;
    conn.execute("VACUUM INTO ?1", [copy.to_string_lossy().as_ref()])?;
    drop(conn);
    let (schema_version, last_change, named) = {
        let c = Connection::open(&copy)?;
        c.pragma_update(None, "journal_mode", "DELETE")?;
        let schema_version: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let last_change = c
            .query_row("SELECT seq, id, hlc FROM changes ORDER BY seq DESC LIMIT 1", [], |r| {
                Ok(LastChange { seq: r.get(0)?, id: r.get(1)?, hlc: r.get(2)? })
            })
            .optional()?;
        (schema_version, last_change, named_blobs(&c)?)
    };

    // 2. Each blob the copy names, once: one the folder has already stays (used now), one it hasn't is copied.
    step(Step::Files)?;
    let (mut copied, mut found) = (0, 0);
    let mut listed = Vec::with_capacity(named.len());
    let mut missing = Vec::new();
    for (sha, db_size) in &named {
        let dest = files::blob_path(&dir, sha);
        if let Ok(m) = fs::symlink_metadata(&dest)
            && m.is_file()
        {
            // a blob no manifest names goes once it hasn't been modified for 7 days: this one is in use
            let _ = files::touch(&dest);
            listed.push(FileEntry { sha256: sha.clone(), bytes: m.len() as i64 });
            found += 1;
            continue;
        }
        match copy_blob(&files::blob_path(data_dir, sha), &dest, sha, &tag, &mut undo)? {
            Some(bytes) => {
                listed.push(FileEntry { sha256: sha.clone(), bytes: bytes as i64 });
                copied += 1;
            }
            None => {
                listed.push(FileEntry { sha256: sha.clone(), bytes: db_size.unwrap_or(0) });
                missing.push(sha.clone());
            }
        }
    }

    // 3. The database, compressed, its hash and size taken as it is read.
    step(Step::Compress)?;
    let zst = snaps.join(format!("{name}.db.zst"));
    let zst_part = snaps.join(format!("{name}.db.zst.part"));
    undo.parts.push(zst_part.clone());
    let size = fs::metadata(&copy)?.len();
    let mut input = Hashing::new(fs::File::open(&copy)?);
    {
        let mut out = fs::File::create(&zst_part)?;
        zstd::stream::compress_sized_into(&mut input, &mut out, ZSTD_LEVEL, size)?;
        out.sync_all()?;
    }
    let (db_sha, db_bytes) = input.sum();
    let bytes = fs::metadata(&zst_part)?.len();
    fs::rename(&zst_part, &zst)?;
    undo.parts.push(zst.clone());

    // 4. The manifest: from now on the snapshot exists.
    step(Step::Manifest)?;
    let manifest = Manifest {
        format: MANIFEST_FORMAT.into(),
        format_version: FORMAT_VERSION,
        name: name.clone(),
        reason: reason.as_str().into(),
        made_at,
        app_version: env!("CARGO_PKG_VERSION").into(),
        schema_version,
        device,
        db: DbFile {
            path: format!("snapshots/{name}.db.zst"),
            compression: "zstd".into(),
            bytes: bytes as i64,
            sha256: db_sha,
            db_bytes: db_bytes as i64,
        },
        last_change,
        files: listed,
        files_missing: missing,
    };
    let json_part = snaps.join(format!("{name}.json.part"));
    undo.parts.push(json_part.clone());
    let mut json = serde_json::to_vec_pretty(&manifest)?;
    json.push(b'\n');
    write_synced(&json_part, &json)?;
    fs::rename(&json_part, snaps.join(format!("{name}.json")))?;
    // made: nothing of it goes now but the temporary copy
    undo.done = true;
    drop(undo);

    let _ = step(Step::Keep);
    let pruned = prune(&dir, &manifest.device.id, keep, ids::now_ms()).unwrap_or_else(|e| {
        eprintln!("gizai: removing old backups in {} failed: {e}", dir.display());
        Pruned::default()
    });
    Ok(Made { dir, manifest, copied, found, pruned })
}

/// What a snapshot has written and not finished (`parts`: removed when it fails, also by a panic), and its temporary
/// copy (`temp`: removed either way).
#[derive(Default)]
struct Unfinished {
    parts: Vec<PathBuf>,
    temp: Vec<PathBuf>,
    done: bool,
}

impl Drop for Unfinished {
    fn drop(&mut self) {
        if !self.done {
            for p in &self.parts {
                let _ = fs::remove_file(p);
            }
        }
        for p in &self.temp {
            let _ = fs::remove_file(p);
        }
    }
}

/// A name no snapshot in `snaps` has, nor one being made: the time is now, a millisecond later when it is taken.
fn free_name(snaps: &Path, reason: Reason, tag: &str) -> Result<(String, i64)> {
    for _ in 0..1000 {
        let at = ids::now_ms();
        let name = snapshot_name(reason, at, tag);
        if !["json", "json.part", "db.zst", "db.zst.part"].iter().any(|ext| snaps.join(format!("{name}.{ext}")).exists()) {
            return Ok((name, at));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Err(Error::Invalid("no free name for a snapshot".into()))
}

/// The blobs a database names, each once, by hash (lowercase), with the size its `files` rows give (None for an
/// artifact's): the three columns `screenshots::sweep_blobs` reads. A value that isn't a sha256 names no blob.
fn named_blobs(c: &Connection) -> Result<BTreeMap<String, Option<i64>>> {
    let mut named: BTreeMap<String, Option<i64>> = BTreeMap::new();
    for sql in ["SELECT sha256, size_bytes FROM files", "SELECT sha256, NULL FROM artifact_versions",
                "SELECT thumbnail_sha256, NULL FROM artifact_versions WHERE thumbnail_sha256 IS NOT NULL"] {
        let mut st = c.prepare(sql)?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))?;
        for row in rows {
            let (sha, size) = row?;
            let sha = sha.to_ascii_lowercase();
            if is_hex(&sha, 64) {
                let known = named.entry(sha).or_insert(None);
                *known = (*known).max(size);
            }
        }
    }
    Ok(named)
}

/// Copies blob `sha` from `src` (in the data folder) to `dest` (in the backups folder): as `<dest>.<device>.part`,
/// hashed while it is copied, then renamed. Its size; None when the data folder doesn't have it, or has something else
/// under its name.
fn copy_blob(src: &Path, dest: &Path, sha: &str, tag: &str, undo: &mut Unfinished) -> Result<Option<u64>> {
    let Ok(mut from) = fs::File::open(src) else { return Ok(None) };
    if !from.metadata().is_ok_and(|m| m.is_file()) {
        return Ok(None);
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = part_of(dest, tag);
    undo.parts.push(part.clone());
    let mut out = Hashing::new(fs::File::create(&part)?);
    io::copy(&mut from, &mut out)?;
    out.inner.sync_all()?;
    let (hash, size) = out.sum();
    if hash != sha {
        fs::remove_file(&part)?;
        return Ok(None);
    }
    fs::rename(&part, dest)?;
    undo.parts.retain(|p| p != &part);
    Ok(Some(size))
}

/// Reads or writes through to `inner`, hashing (sha256) and counting what passes.
struct Hashing<T> {
    inner: T,
    hash: Sha256,
    bytes: u64,
}

impl<T> Hashing<T> {
    fn new(inner: T) -> Self {
        Hashing { inner, hash: Sha256::new(), bytes: 0 }
    }

    /// The sha256 of what passed (64 lowercase hex), and its size.
    fn sum(self) -> (String, u64) {
        (format!("{:x}", self.hash.finalize()), self.bytes)
    }
}

impl<T: Read> Read for Hashing<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hash.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
}

impl<T: Write> Write for Hashing<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hash.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// `s` is `len` lowercase hex digits (a blob's name, its folder's).
fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// How long ago (ms) `t` was, at `now_ms`.
fn age_ms(t: SystemTime, now_ms: i64) -> i64 {
    now_ms - t.duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// The entries of folder `dir`: their names and paths (none when it can't be read).
fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect())
        .unwrap_or_default()
}

/// Removes what this computer's interrupted snapshots left, once it is more than an hour old: their `.part` files in
/// backups folder `dir` (`snapshots/<name>.db.zst.part` and `<name>.json.part`, a blob's `<sha256>.<device>.part`, the
/// marker's and README's), a database file whose manifest never came (or went first, as Keep removes them), and their
/// temporary copies in the data folder's `backups/`. Never another computer's: their names end in its own device.
fn remove_old_parts(dir: &Path, data_dir: &Path, tag: &str, now_ms: i64) {
    let old = |p: &Path| fs::symlink_metadata(p).and_then(|m| m.modified()).is_ok_and(|t| age_ms(t, now_ms) > PART_AGE_MS);
    let mine = format!("-{tag}");
    let part = format!(".{tag}.part");
    let snaps = dir.join("snapshots");
    for (name, path) in entries(&snaps) {
        let gone = match name.strip_suffix(".db.zst.part").or_else(|| name.strip_suffix(".json.part")) {
            Some(stem) => stem.ends_with(&mine),
            None => name.strip_suffix(".db.zst").is_some_and(|stem| {
                stem.ends_with(&mine)
                    && !snaps.join(format!("{stem}.json")).exists()
                    && !snaps.join(format!("{stem}.json.part")).exists()
            }),
        };
        if gone && old(&path) {
            let _ = fs::remove_file(&path);
        }
    }
    for (name, path) in entries(dir) {
        if name.ends_with(&part) && old(&path) {
            let _ = fs::remove_file(&path);
        }
    }
    for (prefix, sub) in entries(&dir.join("files")) {
        if !is_hex(&prefix, 2) {
            continue;
        }
        for (name, path) in entries(&sub) {
            if name.ends_with(&part) && old(&path) {
                let _ = fs::remove_file(&path);
            }
        }
    }
    for (name, path) in entries(&data_dir.join("backups")) {
        if name.starts_with(".snapshot-gizai-") && name.contains(&mine) && old(&path) {
            let _ = fs::remove_file(&path);
        }
    }
}

// ---- reading a folder ----

/// A manifest in a backups folder: the snapshot's name (its file's, without `.json`), and whether it read whole as format
/// 1's. One that didn't (a newer Gizai's, or a damaged one) is read field by field (`field_by_field`).
struct Found {
    name: String,
    manifest: Manifest,
    whole: bool,
}

/// The manifests in backups folder `dir` (`snapshots/*.json`) that are a snapshot's (`format` "gizai-backup"), newest
/// first. Only one computer's when `tag` is given: their names end in its device.
fn manifests(dir: &Path, tag: Option<&str>) -> Vec<Found> {
    let ending = tag.map(|t| format!("-{t}"));
    let mut out: Vec<Found> = entries(&dir.join("snapshots"))
        .into_iter()
        .filter_map(|(name, path)| {
            let stem = name.strip_suffix(".json")?;
            if ending.as_ref().is_some_and(|e| !stem.ends_with(e.as_str())) {
                return None;
            }
            let raw = fs::read(&path).ok()?;
            let (manifest, whole) = match serde_json::from_slice::<Manifest>(&raw) {
                Ok(m) => (m, true),
                Err(_) => (field_by_field(&serde_json::from_slice(&raw).ok()?), false),
            };
            (manifest.format == MANIFEST_FORMAT).then(|| Found { name: stem.to_string(), manifest, whole })
        })
        .collect();
    out.sort_by(|a, b| (b.manifest.made_at, &b.name).cmp(&(a.manifest.made_at, &a.name)));
    out
}

/// A manifest that doesn't read whole as format 1's, field by field: each field that has format 1's shape, the others
/// empty. So a snapshot a newer Gizai made still lists (as one this Gizai can't restore).
fn field_by_field(v: &serde_json::Value) -> Manifest {
    fn field<T: serde::de::DeserializeOwned + Default>(v: &serde_json::Value, key: &str) -> T {
        v.get(key).and_then(|x| T::deserialize(x).ok()).unwrap_or_default()
    }
    Manifest {
        format: field(v, "format"),
        format_version: field(v, "format_version"),
        name: field(v, "name"),
        reason: field(v, "reason"),
        made_at: field(v, "made_at"),
        app_version: field(v, "app_version"),
        schema_version: field(v, "schema_version"),
        device: field(v, "device"),
        db: field(v, "db"),
        last_change: field(v, "last_change"),
        files: field(v, "files"),
        files_missing: field(v, "files_missing"),
    }
}

/// Snapshot `name`'s manifest in backups folder `dir`.
pub fn read_manifest(dir: &Path, name: &str) -> Result<Manifest> {
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        return Err(Error::Invalid(format!("\"{name}\" isn't a snapshot's name")));
    }
    let raw = fs::read(dir.join("snapshots").join(format!("{name}.json"))).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => Error::NotFound(format!("snapshot {name}")),
        _ => e.into(),
    })?;
    let m: Manifest = serde_json::from_slice(&raw)?;
    if m.format != MANIFEST_FORMAT {
        return Err(Error::Invalid(format!("{name}.json isn't a snapshot's manifest")));
    }
    Ok(m)
}

/// Decompresses snapshot `name`'s database from backups folder `dir` into `dest` (replaced) and checks it against its
/// manifest (sha256 and size): where a restore starts (GA-218). Returns the manifest. On an error `dest` is removed.
pub fn unpack_db(dir: &Path, name: &str, dest: &Path) -> Result<Manifest> {
    let m = read_manifest(dir, name)?;
    let src = dir.join("snapshots").join(format!("{name}.db.zst"));
    let unpacked = (|| -> Result<(String, u64)> {
        let input = fs::File::open(&src).map_err(|e| Error::Invalid(format!("{name}'s database file can't be read: {e}")))?;
        let mut out = Hashing::new(fs::File::create(dest)?);
        zstd::stream::copy_decode(input, &mut out)?;
        out.inner.sync_all()?;
        Ok(out.sum())
    })();
    match unpacked {
        Ok((sha, size)) if sha == m.db.sha256 && size as i64 == m.db.db_bytes => Ok(m),
        Ok(_) => {
            let _ = fs::remove_file(dest);
            Err(Error::Invalid(format!("{name}'s database doesn't match its manifest")))
        }
        Err(e) => {
            let _ = fs::remove_file(dest);
            Err(e)
        }
    }
}

/// The blobs backups folder `dir` holds in `files/`.
fn blobs_in(dir: &Path) -> HashSet<String> {
    let mut out = HashSet::new();
    for (prefix, sub) in entries(&dir.join("files")) {
        if !is_hex(&prefix, 2) {
            continue;
        }
        let Ok(rd) = fs::read_dir(&sub) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if is_hex(&name, 64) && name.starts_with(&prefix) && e.file_type().is_ok_and(|t| t.is_file()) {
                out.insert(name);
            }
        }
    }
    out
}

/// The snapshots in the backups folder chosen folder `chosen` stands for (`backups_dir`), newest first, each with whether
/// this Gizai can restore it and why not: a newer Gizai made it (its `format_version` or `schema_version` is higher), its
/// manifest doesn't read whole, its database file is missing, or files are missing ("3 files are missing", counted in
/// `files/` now). `device_id` is
/// this computer's. An error when `chosen` is missing or not a folder; no snapshots when it holds none.
pub fn list(chosen: &Path, device_id: &str) -> Result<Vec<BackupEntry>> {
    if let Some(why) = not_a_folder(chosen) {
        return Err(Error::Invalid(why));
    }
    let dir = backups_dir(chosen);
    let found = manifests(&dir, None);
    let present = if found.iter().any(|f| !f.manifest.files.is_empty()) { blobs_in(&dir) } else { HashSet::new() };
    Ok(found.into_iter().map(|f| entry(&dir, f, device_id, &present)).collect())
}

fn entry(dir: &Path, found: Found, device_id: &str, present: &HashSet<String>) -> BackupEntry {
    let Found { name, manifest: m, whole } = found;
    let files_missing = m.files.iter().filter(|f| !present.contains(&f.sha256.to_ascii_lowercase())).count() as i64;
    let problem = if m.format_version > FORMAT_VERSION || m.schema_version > db::SCHEMA_VERSION {
        Some(match m.app_version.as_str() {
            "" => "A newer Gizai made it: update Gizai to restore it".to_string(),
            v => format!("A newer Gizai made it ({v}): update Gizai to restore it"),
        })
    } else if !whole {
        Some("Its manifest can't be read".to_string())
    } else if !dir.join("snapshots").join(format!("{name}.db.zst")).is_file() {
        Some("Its database file is missing".to_string())
    } else if files_missing > 0 {
        Some(if files_missing == 1 { "1 file is missing".to_string() } else { format!("{files_missing} files are missing") })
    } else {
        None
    };
    BackupEntry {
        name,
        this_computer: m.device.id == device_id,
        reason: m.reason,
        made_at: m.made_at,
        device_id: m.device.id,
        device_name: m.device.name,
        app_version: m.app_version,
        schema_version: m.schema_version,
        db_bytes: m.db.db_bytes,
        files_count: m.files.len() as i64,
        files_bytes: m.files.iter().map(|f| f.bytes).sum(),
        files_missing,
        restorable: problem.is_none(),
        problem,
    }
}

/// This computer's (`device_id`) newest good snapshot in backups folder `dir`: its manifest reads, its database file is
/// there, and it wasn't made before a restore. Daily and weekly snapshots are due a day or a week after it.
pub fn newest(dir: &Path, device_id: &str) -> Option<Last> {
    let snaps = dir.join("snapshots");
    manifests(dir, Some(&device_tag(device_id)))
        .into_iter()
        .find(|f| {
            f.whole
                && f.manifest.device.id == device_id
                && f.manifest.reason != Reason::BeforeRestore.as_str()
                && snaps.join(format!("{}.db.zst", f.name)).is_file()
        })
        .map(|f| Last { name: f.name, made_at: f.manifest.made_at, bytes: f.manifest.db.bytes })
}

// ---- the schedule ----

/// When this computer's next scheduled snapshot is due: a day (a week) after its newest good one, or now when there is
/// none yet. None when Every is manual.
pub fn next_due(every: Every, newest_made_at: Option<i64>, now_ms: i64) -> Option<i64> {
    let interval = every.interval_ms()?;
    Some(newest_made_at.map_or(now_ms, |at| at + interval))
}

/// Whether a scheduled snapshot is due at `now_ms`: from `next_due`, or up to EARLY_MS before it, so the hourly check
/// doesn't push the daily snapshot an hour later each day.
pub fn is_due(every: Every, newest_made_at: Option<i64>, now_ms: i64) -> bool {
    next_due(every, newest_made_at, now_ms).is_some_and(|at| now_ms >= at - EARLY_MS)
}

/// What Settings → General → Backups shows: the settings, the backups folder in use and why it can't be used now, this
/// computer's newest good snapshot there and when the next scheduled one is due (both None while the folder can't be
/// used), and the last error. `running` is the snapshot that runs now, which the app keeps.
pub fn status(db: &Db, running: Option<Running>, now_ms: i64) -> BackupStatus {
    let settings = settings(db);
    let mut out = BackupStatus {
        folder: None,
        folder_problem: None,
        running,
        last: None,
        last_error: last_error(db),
        next: None,
        settings,
    };
    if let Some(chosen) = out.settings.folder.as_deref().map(Path::new) {
        let dir = backups_dir(chosen);
        out.folder = Some(dir.to_string_lossy().into_owned());
        out.folder_problem = folder_problem(chosen);
        if out.folder_problem.is_none() {
            out.last = newest(&dir, db.device_id());
            out.next = next_due(out.settings.every, out.last.as_ref().map(|l| l.made_at), now_ms);
        }
    }
    out
}

// ---- Keep ----

/// Keep, after each snapshot: this computer's (`device_id`) daily, weekly, manual and quit snapshots in backups folder
/// `dir` past the newest `keep` go, each manifest first, then its database. Never another computer's, never a
/// before-restore one. Then each blob in `files/` that no manifest names (`.part` ones included) goes, once it hasn't
/// been modified for 7 days: the blobs a snapshot another computer is making has found stay.
pub fn prune(dir: &Path, device_id: &str, keep: i64, now_ms: i64) -> Result<Pruned> {
    let keep = keep.clamp(1, KEEP_MAX) as usize;
    let snaps = dir.join("snapshots");
    // only manifests that read whole: one that doesn't may not be what it seems
    let mine: Vec<String> = manifests(dir, None)
        .into_iter()
        .filter(|f| f.whole && f.manifest.device.id == device_id && KEPT_REASONS.contains(&f.manifest.reason.as_str()))
        .map(|f| f.name)
        .collect();
    let mut pruned = Pruned::default();
    for name in mine.iter().skip(keep) {
        if fs::remove_file(snaps.join(format!("{name}.json"))).is_ok() {
            let _ = fs::remove_file(snaps.join(format!("{name}.db.zst")));
            pruned.snapshots += 1;
        }
    }
    pruned.blobs = sweep(dir, now_ms);
    Ok(pruned)
}

/// Removes the blobs in backups folder `dir` that no manifest names (`.part` ones included) and that haven't been
/// modified for 7 days (BLOB_AGE_MS). A manifest that can't be read keeps them all this time (it may name any), unless
/// it is a `.part` one more than an hour old, which a snapshot that stopped left. Returns how many went.
fn sweep(dir: &Path, now_ms: i64) -> usize {
    #[derive(Deserialize)]
    struct Names {
        #[serde(default)]
        files: Vec<Name>,
    }
    #[derive(Deserialize)]
    struct Name {
        #[serde(default)]
        sha256: String,
    }
    let Ok(rd) = fs::read_dir(dir.join("snapshots")) else { return 0 };
    let mut named: HashSet<String> = HashSet::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let part = name.ends_with(".json.part");
        if !part && !name.ends_with(".json") {
            continue;
        }
        match fs::read(e.path()).ok().and_then(|raw| serde_json::from_slice::<Names>(&raw).ok()) {
            Some(n) => named.extend(n.files.into_iter().map(|f| f.sha256.to_ascii_lowercase())),
            None if part && e.metadata().and_then(|m| m.modified()).is_ok_and(|t| age_ms(t, now_ms) > PART_AGE_MS) => {}
            None => return 0,
        }
    }
    let mut gone = 0;
    for (prefix, sub) in entries(&dir.join("files")) {
        if !is_hex(&prefix, 2) {
            continue;
        }
        let Ok(rd) = fs::read_dir(&sub) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // read_dir's file_type doesn't follow a link
            if !is_hex(&name, 64) || !name.starts_with(&prefix) || named.contains(&name) || !e.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| age_ms(t, now_ms) >= BLOB_AGE_MS) && fs::remove_file(e.path()).is_ok() {
                gone += 1;
            }
        }
    }
    gone
}
