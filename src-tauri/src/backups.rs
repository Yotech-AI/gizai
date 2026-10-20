//! Backups (GA-216, contract BK): the app's side of `gizai_core::backups`. One snapshot at a time, off the main thread,
//! for Back up now, the schedule (daily or weekly: checked 2 minutes after Gizai starts, then every hour), On quit (Quit
//! Gizai completely backs up first, waiting at most 5 minutes) and GA-218's before-restore snapshot (`back_up`). Each
//! start, step, end and failure, and each change of the settings, sends `backups-changed` with the status.
//!
//! Only Quit Gizai completely backs up on quit: the tray's and Settings' Quit, and Cmd+Q on macOS (`quit_completely`).
//! A signal (logging out, shutting down), the restart after an update or a restore, and macOS's and Windows' own ends
//! (straight to `RunEvent::Exit`) don't.
//!
//! Usable without a Tauri app (tests): the UI hears about changes through `AppState::notify`.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use gizai_core::backups::{self as core, BackupEntry, BackupSettings, BackupStatus, LastError, Made, Reason, Running, Step};
use gizai_core::ids;
use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::runs::Note;

/// The schedule's first check, after Gizai starts…
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(2 * 60);
/// …and then how often it checks.
pub const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// How long quitting waits for its snapshot, and for one that runs already, at most.
pub const QUIT_WAIT: Duration = Duration::from_secs(5 * 60);

/// Nothing asked about the quit's snapshot.
const QUIT_NONE: u8 = 0;
/// Quit Gizai completely asked to quit: with On quit on, Gizai backs up before it exits.
const QUIT_ASKED: u8 = 1;
/// The quit's snapshot runs.
const QUIT_BACKING_UP: u8 = 2;
/// Gizai exits without (another) snapshot: the quit's has run, or a signal or a restart quits.
const QUIT_NOW: u8 = 3;

/// The snapshot that runs now, and the quit's snapshot.
#[derive(Default)]
pub struct Backups {
    running: Mutex<Option<Running>>,
    /// Woken when a snapshot ends.
    ended: Condvar,
    quit: AtomicU8,
    /// For tests: the next snapshot fails as it reaches this step.
    fail_at: Mutex<Option<Step>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Backups {
    /// The snapshot that runs now.
    pub fn running(&self) -> Option<Running> {
        lock(&self.running).clone()
    }

    /// Quit Gizai completely asked to quit (the tray's or Settings' Quit, Cmd+Q on macOS): with On quit on, Gizai backs
    /// up before it exits. In memory only: also on the main thread.
    pub fn quit_completely(&self) {
        let _ = self.quit.compare_exchange(QUIT_NONE, QUIT_ASKED, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Gizai quits without a snapshot: a signal (logging out, shutting down) or the restart after an update or a restore.
    pub fn quit_without_backup(&self) {
        self.quit.store(QUIT_NOW, Ordering::SeqCst);
    }

    /// Quitting was called off (Stay, when the page asked about its unsaved text): only another Quit Gizai completely
    /// backs up as it quits.
    pub fn quit_called_off(&self) {
        let q = self.quit.load(Ordering::SeqCst);
        if q != QUIT_BACKING_UP {
            let _ = self.quit.compare_exchange(q, QUIT_NONE, Ordering::SeqCst, Ordering::SeqCst);
        }
    }

    /// For tests: the next snapshot fails as it reaches `step`, as a full disk would make it fail there.
    #[doc(hidden)]
    pub fn fail_next_at(&self, step: Step) {
        *lock(&self.fail_at) = Some(step);
    }

    fn fails_at(&self, step: Step) -> bool {
        let mut f = lock(&self.fail_at);
        let fails = *f == Some(step);
        if fails {
            *f = None;
        }
        fails
    }
}

/// What Settings → General → Backups shows (contract BK's `BackupStatus`).
pub fn status(st: &AppState) -> BackupStatus {
    core::status(&st.db, st.backups.running(), ids::now_ms())
}

/// Sends `backups-changed` with the status.
fn changed(st: &AppState) {
    (st.notify)(Note::BackupsChanged(status(st)));
}

/// Saves the backup settings (refusing Keep outside 1 to 100 and a folder Gizai can't write in, saying why) and says so
/// in `backups-changed`. Returns the status.
pub fn save_settings(st: &AppState, settings: BackupSettings) -> Result<BackupStatus, String> {
    core::save_settings(&st.db, settings).map_err(|e| e.to_string())?;
    let out = status(st);
    (st.notify)(Note::BackupsChanged(out.clone()));
    Ok(out)
}

/// The snapshots in chosen folder `folder` (read as `core::backups_dir` says), newest first; None: the folder in
/// Settings (no snapshots when none is set).
pub fn list(st: &AppState, folder: Option<String>) -> Result<Vec<BackupEntry>, String> {
    let chosen = match folder.filter(|f| !f.trim().is_empty()).or_else(|| core::settings(&st.db).folder) {
        Some(f) => PathBuf::from(f),
        None => return Ok(vec![]),
    };
    core::list(&chosen, st.db.device_id()).map_err(|e| e.to_string())
}

/// Back up now: starts a `manual` snapshot on a thread of its own and returns the status at once, with it running.
/// Refused without a folder, while the folder can't be used (saying why) and while a snapshot runs.
pub fn back_up_now(st: &AppState) -> Result<BackupStatus, String> {
    let chosen = settings_folder(st)?;
    if let Some(problem) = core::folder_problem(&chosen) {
        return Err(problem);
    }
    let slot = claim(st, Reason::Manual, None)?;
    std::thread::Builder::new()
        .name("gizai-backup".into())
        .spawn(move || {
            let _ = run(slot, &chosen, Reason::Manual);
        })
        .map_err(|e| format!("Couldn't start the backup: {e}"))?;
    Ok(status(st))
}

/// Makes a snapshot now, on this thread, and returns once it is made: in chosen folder `folder` (read as
/// `core::backups_dir` says; None: the folder in Settings), with this reason, then Settings' Keep. One at a time: refused
/// while another runs. Its start, steps and end go out in `backups-changed`, and a failure is kept as the last error.
/// GA-218 makes its `before-restore` snapshot with it, in a folder of its own.
pub fn back_up(st: &AppState, folder: Option<&Path>, reason: Reason) -> Result<Made, String> {
    let chosen = match folder {
        Some(f) => f.to_path_buf(),
        None => settings_folder(st)?,
    };
    let slot = claim(st, reason, None)?;
    run(slot, &chosen, reason)
}

/// The schedule's check (FIRST_CHECK_AFTER Gizai starts, then every CHECK_EVERY): with Every daily or weekly, makes a
/// snapshot on this thread when this computer's newest good one in the folder is a day or a week old at `now_ms`
/// (`core::is_due`). Passes quietly while backups are off, the folder can't be used (a USB disk that isn't plugged in:
/// the status's `folderProblem` says why) or a snapshot runs. Returns what it made, or why that failed; None when it made
/// none.
pub fn check_schedule(st: &AppState, now_ms: i64) -> Option<Result<Made, String>> {
    let s = core::settings(&st.db);
    let (Some(folder), Some(reason)) = (s.folder, s.every.reason()) else { return None };
    let chosen = PathBuf::from(folder);
    if st.backups.running().is_some() || core::folder_problem(&chosen).is_some() {
        return None;
    }
    let newest = core::newest(&core::backups_dir(&chosen), st.db.device_id());
    if !core::is_due(s.every, newest.map(|n| n.made_at), now_ms) {
        return None;
    }
    let slot = claim(st, reason, None).ok()?;
    Some(run(slot, &chosen, reason))
}

/// A request to quit, once the page's unsaved text and the agents at work are settled (`RunEvent::ExitRequested`): when
/// Quit Gizai completely asked, a thread of its own reads On quit (no database work on the main thread) and, with it on,
/// makes the quit's snapshot; then it calls `exit`, at the latest after QUIT_WAIT (a snapshot still running then is left,
/// as an interrupted one). Returns true while Gizai waits for that thread (also when On quit turns out to be off: then
/// `exit` comes at once). A second request meanwhile quits at once.
pub fn on_exit_request(st: &AppState, exit: impl FnOnce() + Send + 'static) -> bool {
    match st.backups.quit.compare_exchange(QUIT_ASKED, QUIT_BACKING_UP, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {}
        Err(QUIT_BACKING_UP) => {
            st.backups.quit.store(QUIT_NOW, Ordering::SeqCst);
            return false;
        }
        Err(_) => return false,
    }
    let st2 = st.clone();
    let spawned = std::thread::Builder::new().name("gizai-quit-backup".into()).spawn(move || {
        quit_backup(&st2);
        st2.backups.quit.store(QUIT_NOW, Ordering::SeqCst);
        exit();
    });
    // without a thread there is no snapshot: Gizai quits now
    if spawned.is_err() {
        st.backups.quit.store(QUIT_NOW, Ordering::SeqCst);
        return false;
    }
    true
}

/// `on_exit_request` for the app (`run`'s exit): Gizai quits with `code` once the quit's snapshot has ended.
pub fn quit_first(app: &AppHandle, code: i32) -> bool {
    let Some(st) = app.try_state::<AppState>() else { return false };
    let app = app.clone();
    on_exit_request(st.inner(), move || app.exit(code))
}

/// The quit's snapshot, with On quit on and a folder that can be used now (one that can't is passed over quietly, as
/// the schedule does): it waits for a snapshot that runs already, then makes its own, within QUIT_WAIT in all. No run or
/// chat answer starts meanwhile: Gizai is quitting.
fn quit_backup(st: &AppState) {
    let s = core::settings(&st.db);
    let Some(folder) = s.folder.filter(|_| s.on_quit) else { return };
    let chosen = PathBuf::from(folder);
    if core::folder_problem(&chosen).is_some() {
        return;
    }
    crate::runs::mark_closing(st);
    let until = Instant::now() + QUIT_WAIT;
    let (done, ended) = std::sync::mpsc::channel();
    let st2 = st.clone();
    let spawned = std::thread::Builder::new().name("gizai-backup".into()).spawn(move || {
        if let Ok(slot) = claim(&st2, Reason::Quit, Some(until)) {
            let _ = run(slot, &chosen, Reason::Quit);
        }
        let _ = done.send(());
    });
    if spawned.is_ok() {
        let _ = ended.recv_timeout(until.saturating_duration_since(Instant::now()));
    }
}

/// The folder in Settings.
fn settings_folder(st: &AppState) -> Result<PathBuf, String> {
    core::settings(&st.db).folder.map(PathBuf::from).ok_or_else(|| "Choose a folder for backups first".to_string())
}

/// The one snapshot slot, taken for a snapshot: given back when dropped (also after a panic), which `backups-changed`
/// says.
struct Slot {
    st: AppState,
}

impl Drop for Slot {
    fn drop(&mut self) {
        *lock(&self.st.backups.running) = None;
        self.st.backups.ended.notify_all();
        changed(&self.st);
    }
}

/// Takes the slot for a snapshot with this reason, which `backups-changed` says. While one runs: refused at once
/// (`until` None), or waits for it to end until `until`.
fn claim(st: &AppState, reason: Reason, until: Option<Instant>) -> Result<Slot, String> {
    let busy = || "A backup is running already".to_string();
    let mut running = lock(&st.backups.running);
    while running.is_some() {
        let left = until.ok_or_else(busy)?.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(busy());
        }
        running = st.backups.ended.wait_timeout(running, left).unwrap_or_else(|p| p.into_inner()).0;
    }
    *running = Some(Running { reason, started_at: ids::now_ms(), step: Step::Database.as_str().into() });
    drop(running);
    changed(st);
    Ok(Slot { st: st.clone() })
}

/// Makes the snapshot `slot` was taken for, on this thread: each step goes out in `backups-changed`, a failure is kept
/// as the last error (and one that worked clears it), and the slot is given back at the end.
fn run(slot: Slot, chosen: &Path, reason: Reason) -> Result<Made, String> {
    let st = &slot.st;
    let keep = core::settings(&st.db).keep;
    let made = core::make(&st.data_dir, chosen, reason, keep, &mut |step| {
        if st.backups.fails_at(step) {
            return Err(gizai_core::Error::Invalid(format!("a test stopped the snapshot at its {} step", step.as_str())));
        }
        let moved = match lock(&st.backups.running).as_mut() {
            Some(r) if r.step != step.as_str() => {
                r.step = step.as_str().into();
                true
            }
            _ => false,
        };
        if moved {
            changed(st);
        }
        Ok(())
    });
    let out = match made {
        Ok(made) => {
            if core::last_error(&st.db).is_some() {
                let _ = core::set_last_error(&st.db, None);
            }
            Ok(made)
        }
        Err(e) => {
            let message = e.to_string();
            eprintln!("gizai: the {} backup in {} failed: {message}", reason.as_str(), chosen.display());
            let _ = core::set_last_error(&st.db, Some(&LastError { at: ids::now_ms(), message: message.clone() }));
            Err(message)
        }
    };
    drop(slot);
    out
}
