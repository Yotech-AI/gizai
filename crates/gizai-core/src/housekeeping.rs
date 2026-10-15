//! Housekeeping: what Gizai no longer needs goes, so its data folder doesn't grow for ever (README → Your data). Gizai
//! does this when it starts and once a day while it runs: `run` (tokens and logs), then `clear_screenshots` (old
//! screenshots and the blobs no row uses).
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use crate::db::Db;
use crate::{Result, tokens};

/// How many days a run's log (or a chat answer's, or a board check's) is kept after the run last wrote to it, which is
/// when the run ended.
pub const KEEP_LOG_DAYS: i64 = 30;
pub const KEEP_LOGS_MS: i64 = KEEP_LOG_DAYS * 24 * 60 * 60 * 1000;

/// How often Gizai does its housekeeping while it runs.
pub const EVERY_MS: i64 = 24 * 60 * 60 * 1000;

/// The folders in Gizai's data folder that hold logs: task runs' (`runs/`), and chat answers' and board checks' (`chat/`).
pub const LOG_DIRS: [&str; 2] = ["runs", "chat"];

/// What `run` removed.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Pruned {
    pub tokens: usize,
    pub logs: usize,
}

/// Removes the chat tools' dead tokens (`tokens::prune`) and the old logs in the `LOG_DIRS` of `data_dir` (`prune_logs`).
/// The runs themselves stay, with their summary, cost and commits.
pub fn run(db: &Db, data_dir: &Path, now_ms: i64) -> Result<Pruned> {
    let tokens = tokens::prune(db, now_ms)?;
    let logs = LOG_DIRS.iter().map(|d| prune_logs(&data_dir.join(d), now_ms)).sum();
    Ok(Pruned { tokens, logs })
}

/// What `clear_screenshots` removed (GA-182).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Cleared {
    /// Runs whose screenshot set went.
    pub sets: usize,
    /// Their screenshots (file rows).
    pub screenshots: usize,
    /// Blobs in the data folder's `files/` that no row used any more (any kind of file).
    pub blobs: usize,
}

/// Clears old screenshots as Settings → Agents and runs → Screenshots says (`screenshots::limits`): each card keeps its
/// newest Keep per card sets, and a card in Done or Cancelled for longer than Delete after loses all of them (0 keeps
/// them), what people attached never (`screenshots::prune`). Then the blobs in `data_dir/files` that no row uses any
/// more go, never one a row still uses (`screenshots::sweep_blobs`).
pub fn clear_screenshots(db: &Db, data_dir: &Path, now_ms: i64) -> Result<Cleared> {
    let l = crate::screenshots::limits(db);
    let pruned = crate::screenshots::prune(db, now_ms, l.keep_sets, l.keep_days)?;
    let blobs = crate::screenshots::sweep_blobs(db, data_dir, now_ms)?;
    Ok(Cleared { sets: pruned.sets, screenshots: pruned.files, blobs })
}

/// Removes the logs in `dir` (files named `*.jsonl` or `*.log`: a run's output and its stderr) last written to more than
/// `KEEP_LOGS_MS` before `now_ms`. Everything else stays: other files (a live run's MCP config), folders, links, and a
/// file whose time can't be read. Returns how many went; a missing folder has none.
pub fn prune_logs(dir: &Path, now_ms: i64) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let before = UNIX_EPOCH + Duration::from_millis((now_ms - KEEP_LOGS_MS).max(0) as u64);
    let mut n = 0;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !(name.ends_with(".jsonl") || name.ends_with(".log")) {
            continue;
        }
        // Neither file_type nor metadata follows a link.
        let old = e.file_type().is_ok_and(|t| t.is_file()) && e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < before);
        if old && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}
