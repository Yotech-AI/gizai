//! Restore (GA-218, contract BR): the app's side of `gizai_core::restore`. `start` runs a restore on a thread of its
//! own: it checks the snapshot, saves today's data (a `before-restore` snapshot, `backups::back_up_within`) and stages
//! the snapshot in `<data>/restore/`. Then it is ready, and waits for a restart: `restart_command` quits Gizai the usual
//! way (agents at work are stopped first) and starts it again, and the start (`open_data`, `gizai_core::restore::open`)
//! swaps it in before the database opens. Each start, step, end, failure and cancel, and dismissing the result, sends
//! `restore-changed` with the status.
//!
//! One restore at a time, and none while a backup runs. Usable without a Tauri app (tests): the UI hears about changes
//! through `AppState::notify`.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use gizai_core::backups::{self as backups_core, BackupEntry, Reason};
use gizai_core::restore::{self as core, RestoreStatus, Saved, Step};

use crate::AppState;
use crate::runs::Note;

/// How long saving today's data waits for a snapshot that runs already (a scheduled one that started meanwhile).
pub const SAVE_WAIT: Duration = Duration::from_secs(5 * 60);
/// The counts of files checked and copied go out in `restore-changed` at most this often; every other step at once.
pub const COUNT_EVERY: Duration = Duration::from_millis(100);
/// How long `cancel` waits for a restore that runs to stop.
const CANCEL_WAIT: Duration = Duration::from_secs(2);

/// The step while today's data is saved.
pub const SAVING_STEP: &str = "Saving today's data";

/// The restore that runs now, or the last one since Gizai started (ready, or failed).
#[derive(Default)]
pub struct Restores {
    now: Mutex<Now>,
    /// Set by `cancel` while a restore runs: it stops at its next step.
    cancel: AtomicBool,
}

#[derive(Debug, Clone, Default)]
struct Now {
    /// "none" (the default), "checking", "saving", "staging", "ready" or "failed".
    state: &'static str,
    folder: Option<String>,
    snapshot: Option<BackupEntry>,
    step: Option<String>,
    error: Option<String>,
    saved_today: Option<String>,
}

impl Now {
    fn running(&self) -> bool {
        matches!(self.state, "checking" | "saving" | "staging")
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// What a restore is doing (contract BR's `RestoreStatus`), with the last restore's result until it is dismissed. A
/// restore staged on disk that this Gizai didn't stage reads as ready.
pub fn status(st: &AppState) -> RestoreStatus {
    let now = lock(&st.restores.now).clone();
    let mut out = RestoreStatus {
        state: if now.state.is_empty() { "none" } else { now.state }.into(),
        folder: now.folder,
        snapshot: now.snapshot,
        step: now.step,
        error: now.error,
        saved_today: now.saved_today,
        result: core::result(&st.db),
    };
    if out.state == "none"
        && let Some(staged) = core::staged(&st.data_dir)
    {
        out.state = "ready".into();
        out.snapshot = entry(st, Path::new(&staged.folder), &staged.manifest.name);
        out.folder = Some(staged.folder);
        out.saved_today = Some(staged.saved_today.name);
    }
    out
}

/// Sends `restore-changed` with the status.
fn changed(st: &AppState) {
    (st.notify)(Note::RestoreChanged(status(st)));
}

/// Snapshot `name` in the backups folder chosen folder `chosen` stands for, as Settings lists it.
fn entry(st: &AppState, chosen: &Path, name: &str) -> Option<BackupEntry> {
    backups_core::list(chosen, st.db.device_id()).ok()?.into_iter().find(|e| e.name == name)
}

/// Starts restoring snapshot `name` from chosen folder `folder` (read as `backups::backups_dir` says; empty: the folder
/// in Settings) on a thread of its own, and returns the status at once, with it checking. Refused while a backup or
/// another restore runs. A restore that is ready already goes: this one takes its place. Each step, and how it ends
/// (ready, or failed with why), goes out in `restore-changed`.
pub fn start(st: &AppState, folder: &str, name: &str) -> Result<RestoreStatus, String> {
    if st.backups.running().is_some() {
        return Err("A backup is running: restore once it is done".into());
    }
    let chosen = match folder.trim() {
        "" => backups_core::settings(&st.db).folder.map(PathBuf::from).unwrap_or_default(),
        f => PathBuf::from(f),
    };
    {
        let mut now = lock(&st.restores.now);
        if now.running() {
            return Err("A restore is running already".into());
        }
        st.restores.cancel.store(false, Ordering::SeqCst);
        *now = Now {
            state: "checking",
            folder: (!chosen.as_os_str().is_empty()).then(|| backups_core::backups_dir(&chosen).to_string_lossy().into_owned()),
            step: Some(Step::Snapshot.text()),
            ..Now::default()
        };
    }
    // the restore staged before, and what one that stopped left
    let _ = core::cancel(&st.data_dir);
    let out = status(st);
    (st.notify)(Note::RestoreChanged(out.clone()));
    let st2 = st.clone();
    let name = name.to_string();
    let spawned = std::thread::Builder::new().name("gizai-restore".into()).spawn(move || run(&st2, &chosen, &name));
    if let Err(e) = spawned {
        end(st, Err(format!("Couldn't start the restore: {e}")));
        return Err(format!("Couldn't start the restore: {e}"));
    }
    Ok(out)
}

/// The restore, on its own thread: its three steps, then how it ended.
fn run(st: &AppState, chosen: &Path, name: &str) {
    let out = restore(st, chosen, name);
    end(st, out);
}

/// Ends the restore that runs: ready, failed with why, or, when it was cancelled, none (and nothing staged). Says so.
fn end(st: &AppState, out: Result<(), String>) {
    {
        let mut now = lock(&st.restores.now);
        let cancelled = st.restores.cancel.swap(false, Ordering::SeqCst);
        if cancelled || out.is_err() {
            let _ = core::cancel(&st.data_dir);
        }
        match out {
            _ if cancelled => *now = Now::default(),
            Ok(()) => {
                now.state = "ready";
                now.step = None;
            }
            Err(why) => {
                eprintln!("gizai: restoring failed: {why}");
                now.state = "failed";
                now.step = None;
                now.error = Some(why);
            }
        }
    }
    changed(st);
}

fn restore(st: &AppState, chosen: &Path, name: &str) -> Result<(), String> {
    if chosen.as_os_str().is_empty() {
        return Err("Choose the folder to restore from".into());
    }
    let dir = backups_core::backups_dir(chosen);
    // as Settings lists it (also to say which computer made it); the check says what is wrong with it
    backups_core::list(chosen, st.db.device_id()).map_err(|e| e.to_string())?;
    let snapshot = entry(st, chosen, name);
    lock(&st.restores.now).snapshot = snapshot;
    changed(st);
    let mut said = None;
    // 1. the checks
    let m = core::check(&st.data_dir, &dir, name, &mut |s| step(st, s, &mut said)).map_err(|e| e.to_string())?;
    stop_if_cancelled(st)?;
    // 2. today's data, saved
    set_state(st, "saving", Some(SAVING_STEP.into()));
    let saved = match save_today(st, chosen) {
        Ok(saved) => saved,
        Err(why) => {
            core::remove_checked(&st.data_dir);
            return Err(why);
        }
    };
    lock(&st.restores.now).saved_today = Some(saved.name.clone());
    stop_if_cancelled(st)?;
    // 3. staged
    set_state(st, "staging", Some(Step::CopyDatabase.text()));
    core::stage(&st.data_dir, &dir, &m, &saved, &mut |s| step(st, s, &mut said)).map_err(|e| e.to_string())?;
    stop_if_cancelled(st)
}

/// A restore that was cancelled stops here.
fn stop_if_cancelled(st: &AppState) -> Result<(), String> {
    if st.restores.cancel.load(Ordering::SeqCst) { Err("Cancelled".into()) } else { Ok(()) }
}

fn set_state(st: &AppState, state: &'static str, step: Option<String>) {
    {
        let mut now = lock(&st.restores.now);
        now.state = state;
        now.step = step;
    }
    changed(st);
}

/// A step of the check or the staging: it goes out in `restore-changed` (the counts in between at most every
/// COUNT_EVERY, `said` being when the last went). A restore that was cancelled stops here.
fn step(st: &AppState, s: Step, said: &mut Option<Instant>) -> gizai_core::Result<()> {
    if st.restores.cancel.load(Ordering::SeqCst) {
        return Err(gizai_core::Error::Invalid("Cancelled".into()));
    }
    let text = s.text();
    let between = matches!(s, Step::CheckFiles(n, of) | Step::CopyFiles(n, of) if n > 0 && n < of);
    {
        let mut now = lock(&st.restores.now);
        if now.step.as_deref() == Some(text.as_str()) {
            return Ok(());
        }
        now.step = Some(text);
    }
    if !between || said.is_none_or(|t| t.elapsed() >= COUNT_EVERY) {
        *said = Some(Instant::now());
        changed(st);
    }
    Ok(())
}

/// Saves today's data before a restore: a `before-restore` snapshot in the backups folder in Settings, or, when none is
/// set (or it can't be written now), in the folder restored from (`chosen`). It waits for a snapshot that runs already,
/// at most SAVE_WAIT. When neither can be written, says why and what to do.
fn save_today(st: &AppState, chosen: &Path) -> Result<Saved, String> {
    let until = Some(Instant::now() + SAVE_WAIT);
    let mut folders: Vec<PathBuf> = backups_core::settings(&st.db).folder.map(PathBuf::from).into_iter().collect();
    if !folders.iter().any(|f| backups_core::backups_dir(f) == backups_core::backups_dir(chosen)) {
        folders.push(chosen.to_path_buf());
    }
    let mut why = Vec::new();
    for folder in folders {
        match crate::backups::back_up_within(st, Some(&folder), Reason::BeforeRestore, until) {
            Ok(made) => {
                return Ok(Saved {
                    name: made.manifest.name,
                    made_at: made.manifest.made_at,
                    folder: made.dir.to_string_lossy().into_owned(),
                });
            }
            Err(e) => why.push(e),
        }
    }
    Err(format!("Gizai can't save today's data first: {}. Choose a backups folder in Settings → General → Backups.", why.join("; ")))
}

/// Removes the staged restore (`<data>/restore/`, apart from `restore/failed/`); the before-restore snapshot stays in its
/// folder. A restore that runs stops at its next step (this waits a moment for it), and nothing of it stays staged.
/// Afterwards the state is none, and the next start is a normal one.
pub fn cancel(st: &AppState) -> Result<RestoreStatus, String> {
    if lock(&st.restores.now).running() {
        st.restores.cancel.store(true, Ordering::SeqCst);
        let until = Instant::now() + CANCEL_WAIT;
        while lock(&st.restores.now).running() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        return Ok(status(st));
    }
    core::cancel(&st.data_dir).map_err(|e| format!("Couldn't remove the staged restore: {e}"))?;
    *lock(&st.restores.now) = Now::default();
    let out = status(st);
    (st.notify)(Note::RestoreChanged(out.clone()));
    Ok(out)
}

/// Dismisses the last restore's result (`result` is null from then on).
pub fn dismiss_result(st: &AppState) -> Result<RestoreStatus, String> {
    core::dismiss_result(&st.db).map_err(|e| e.to_string())?;
    let out = status(st);
    (st.notify)(Note::RestoreChanged(out.clone()));
    Ok(out)
}

/// The restart that applies a staged restore (`restart_to_restore`): `update::start_again_command`, a shell that waits
/// until this Gizai has quit, then starts the same Gizai again. Refused while no restore is ready. The caller spawns
/// it, then quits Gizai the usual way.
pub fn restart_command(st: &AppState) -> Result<std::process::Command, String> {
    if core::staged(&st.data_dir).is_none() {
        return Err("No restore is ready: restore a snapshot first".into());
    }
    crate::update::start_again_command(st)
}
