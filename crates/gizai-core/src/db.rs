//! One SQLite connection owned by the app. Every write runs in one IMMEDIATE transaction
//! and appends to `changes` (the activity feed, audit log and future sync oplog).
use crate::{ids, Result};
use rusqlite::{Connection, Transaction, TransactionBehavior};
use rusqlite_migration::{M, Migrations};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

pub const SCHEMA_VERSION: i64 = 12;

pub struct Db {
    conn: Mutex<Connection>,
    device_id: String,
    counter: AtomicU32,
    /// The folder the database file is in (Gizai's data folder); None in memory.
    dir: Option<PathBuf>,
}

fn migrations() -> Migrations<'static> {
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
        M::up(include_str!("../migrations/0010_run_refusals.sql")),
        M::up(include_str!("../migrations/0011_column_agents.sql")),
        M::up(include_str!("../migrations/0012_chat_runs_on.sql")),
    ])
}

/// Snapshots kept per folder; older ones are removed.
const KEEP_SNAPSHOTS: usize = 20;

/// "20261007-111502-123" for a snapshot's name, in local time (the TZ variable, else the system's time zone): reads like
/// a date on your clock, and sorts like time (except in the hour a clock goes back).
pub fn stamp(ms: i64) -> String {
    let ms = ms + local_offset_secs(ms) * 1000;
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}-{:03}", rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000)
}

unsafe extern "C" {
    /// POSIX: the C library reads TZ again (the libc crate has no binding for it on Linux).
    fn tzset();
}

/// How far local time is ahead of UTC at `ms`, in seconds (summer time included); 0 when the C library can't say.
fn local_offset_secs(ms: i64) -> i64 {
    let t = ms.div_euclid(1000) as libc::time_t;
    // SAFETY: tzset only updates the C library's time zone; localtime_r writes nothing but `tm`, which is ours.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        tzset();
        if libc::localtime_r(&t, &mut tm).is_null() { 0 } else { tm.tm_gmtoff as i64 }
    }
}

/// A complete, consistent copy of the database at `src` (`VACUUM INTO`, safe while Gizai has it open) as
/// `dest_dir/gizai-<label>-<time>.db`. Keeps the newest 20 snapshots in that folder. Never migrates `src`.
pub fn snapshot(src: &Path, dest_dir: &Path, label: &str) -> Result<std::path::PathBuf> {
    std::fs::create_dir_all(dest_dir)?;
    let mut dest = dest_dir.join(format!("gizai-{label}-{}.db", stamp(ids::now_ms())));
    let mut n = 2;
    while dest.exists() {
        dest = dest_dir.join(format!("gizai-{label}-{}-{n}.db", stamp(ids::now_ms())));
        n += 1;
    }
    let conn = Connection::open(src)?;
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy().as_ref()])?;
    drop(conn);
    let mut snaps: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(dest_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| { let n = e.file_name().to_string_lossy().into_owned(); n.starts_with("gizai-") && n.ends_with(".db") })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    snaps.sort();
    if snaps.len() > KEEP_SNAPSHOTS {
        for (_, old) in &snaps[..snaps.len() - KEEP_SNAPSHOTS] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(dest)
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        // A database made by an older Gizai is copied before its schema is upgraded (backups/ next to it).
        if path.exists() {
            let version: i64 = Connection::open(path)?.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version > 0 && version < SCHEMA_VERSION {
                let dir = path.parent().unwrap_or(Path::new(".")).join("backups");
                snapshot(path, &dir, &format!("before-v{SCHEMA_VERSION}"))?;
            }
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let mut db = Self::init(conn)?;
        db.dir = path.parent().map(|d| std::path::absolute(d).unwrap_or_else(|_| d.to_path_buf()));
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    /// The folder the database file is in: Gizai's data folder (None for a database in memory).
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    fn init(mut conn: Connection) -> Result<Db> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // Foreign keys are off while the schema is upgraded: a migration that changes a CHECK rebuilds its table (SQLite's
        // way), and the old table's rows must be dropped while other tables still point at them. The rebuild keeps every id.
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        migrations().to_latest(&mut conn)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let device_id: Option<String> = conn
            .query_row("SELECT id FROM devices WHERE is_self = 1 LIMIT 1", [], |r| r.get(0))
            .ok();
        let device_id = match device_id {
            Some(d) => d,
            None => {
                let id = ids::new_id();
                let name = std::env::var("HOSTNAME").unwrap_or_else(|_| "this machine".into());
                conn.execute(
                    "INSERT INTO devices(id, created_at, name, is_self) VALUES (?1, ?2, ?3, 1)",
                    rusqlite::params![id, ids::now_ms(), name],
                )?;
                id
            }
        };
        Ok(Db { conn: Mutex::new(conn), device_id, counter: AtomicU32::new(0), dir: None })
    }

    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        f(&conn)
    }

    pub fn write<T>(&self, actor: Option<&str>, f: impl FnOnce(&mut Writer) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut w = Writer {
            tx,
            actor: actor.map(str::to_string),
            run_id: None,
            device_id: &self.device_id,
            counter: &self.counter,
        };
        let out = f(&mut w)?;
        w.tx.commit()?;
        Ok(out)
    }
}

pub struct Writer<'a> {
    tx: Transaction<'a>,
    actor: Option<String>,
    run_id: Option<String>,
    device_id: &'a str,
    counter: &'a AtomicU32,
}

impl Writer<'_> {
    pub fn conn(&self) -> &Transaction<'_> {
        &self.tx
    }

    pub fn actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }

    /// Attribute the following changes to an agent run.
    pub fn set_run(&mut self, run_id: &str) {
        self.run_id = Some(run_id.to_string());
    }

    pub fn insert(&self, table: &str, row_id: &str, diff: serde_json::Value) -> Result<()> {
        self.change(table, row_id, "insert", Some(diff))
    }

    pub fn update(&self, table: &str, row_id: &str, diff: serde_json::Value) -> Result<()> {
        self.change(table, row_id, "update", Some(diff))
    }

    pub fn delete(&self, table: &str, row_id: &str) -> Result<()> {
        self.change(table, row_id, "delete", None)
    }

    fn change(&self, table: &str, row_id: &str, op: &str, diff: Option<serde_json::Value>) -> Result<()> {
        let now = ids::now_ms();
        let n = self.counter.fetch_add(1, Ordering::SeqCst) % 10_000;
        let hlc = format!("{now}-{n:04}-{}", &self.device_id[..8.min(self.device_id.len())]);
        self.tx.execute(
            "INSERT INTO changes(id, hlc, device_id, actor_id, run_id, table_name, row_id, op, diff_json, schema_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                ids::new_id(), hlc, self.device_id, self.actor, self.run_id, table, row_id, op,
                diff.map(|d| d.to_string()), SCHEMA_VERSION
            ],
        )?;
        Ok(())
    }
}
