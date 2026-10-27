// GA-218 QA: restore in the app (src-tauri/src/restore.rs, contract BR). `start_restore` checks, saves today's data (a
// before-restore snapshot in the folder in Settings, else in the folder restored from, else it stops and says what to
// do) and stages, with `restore-changed` at each step; the status as the window reads it; refused while a backup or
// another restore runs; cancel, of a ready restore and of one that runs; the start after it (`open_data`), with the
// result until it is dismissed; and `restart_to_restore`'s command, tested as the update's restart is, while
// `restart_gizai` still refuses without an update.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gizai_core::backups::{self as core, BackupSettings, Reason};
use gizai_core::restore::RestoreStatus;
use gizai_lib::backups as app;
use gizai_lib::restore;
use gizai_lib::{AppState, runs};

/// Holds the thread that passes it while closed: so a test can ask for something while a restore or a snapshot runs.
#[derive(Default)]
struct Gate {
    closed: Mutex<bool>,
    opened: Condvar,
    held: AtomicBool,
}

impl Gate {
    fn close(&self) {
        *self.closed.lock().unwrap() = true;
    }

    fn open(&self) {
        *self.closed.lock().unwrap() = false;
        self.opened.notify_all();
    }

    fn pass(&self) {
        let mut closed = self.closed.lock().unwrap();
        while *closed {
            self.held.store(true, Ordering::SeqCst);
            closed = self.opened.wait(closed).unwrap();
        }
    }
}

/// What `restore-changed` sent, in order; the gates a restore's thread and a snapshot's pass; and, at the first
/// `staging`, the manifests in folder `watch`.
#[derive(Default)]
struct Heard {
    got: Mutex<Vec<RestoreStatus>>,
    restore_gate: Gate,
    backup_gate: Gate,
    watch: Mutex<Option<PathBuf>>,
    at_staging: Mutex<Option<Vec<String>>>,
}

impl Heard {
    fn all(&self) -> Vec<RestoreStatus> {
        self.got.lock().unwrap().clone()
    }
}

fn listen(st: &mut AppState) -> Arc<Heard> {
    let heard: Arc<Heard> = Arc::default();
    let h = heard.clone();
    st.notify = Arc::new(move |n| match n {
        runs::Note::RestoreChanged(s) => {
            if s.state == "staging" {
                let mut at = h.at_staging.lock().unwrap();
                if at.is_none() {
                    *at = h.watch.lock().unwrap().as_deref().map(manifests);
                }
            }
            h.got.lock().unwrap().push(s);
            if std::thread::current().name() == Some("gizai-restore") {
                h.restore_gate.pass();
            }
        }
        runs::Note::BackupsChanged(s) if s.running.is_some() => h.backup_gate.pass(),
        _ => {}
    });
    heard
}

/// Gizai on new data in `tmp`, and a folder `usb` beside it.
fn gizai(tmp: &Path) -> (AppState, PathBuf) {
    let st = gizai_lib::test_state(tmp);
    let usb = tmp.join("usb");
    fs::create_dir_all(&usb).unwrap();
    (st, usb)
}

fn settings(folder: &Path) -> BackupSettings {
    BackupSettings { folder: Some(folder.to_string_lossy().into_owned()), ..Default::default() }
}

/// Waits up to 60 s for `ok`.
fn until<T>(what: &str, mut ok: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(t) = ok() {
            return t;
        }
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Waits until the restore has ended (ready, failed or none), and returns its status.
fn ended(st: &AppState) -> RestoreStatus {
    until("the restore ended", || {
        let s = restore::status(st);
        matches!(s.state.as_str(), "ready" | "failed" | "none").then_some(s)
    })
}

/// The manifests in the backups folder chosen folder `chosen` stands for.
fn manifests(chosen: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(core::backups_dir(chosen).join("snapshots")).map(|rd| {
        rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".json")).collect()
    }).unwrap_or_default();
    v.sort();
    v
}

fn before_restore(chosen: &Path) -> Vec<String> {
    manifests(chosen).into_iter().filter(|n| n.starts_with("gizai-before-restore-")).collect()
}

fn attach(st: &AppState, name: &str, bytes: &[u8]) -> String {
    let blob = gizai_core::files::store_bytes(&st.data_dir, name, bytes).unwrap();
    st.db.write(Some(&st.you_id), |w| gizai_core::files::insert(w, Some(&st.you_id), "actor", &st.you_id, &blob)).unwrap();
    blob.sha256
}

fn files_rows(st: &AppState) -> Vec<String> {
    st.db.read(|c| {
        let mut s = c.prepare("SELECT sha256 FROM files ORDER BY sha256")?;
        let v = s.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok(v)
    }).unwrap()
}

/// The states `restore-changed` went through, each once in a row.
fn states(heard: &[RestoreStatus]) -> Vec<String> {
    let mut v: Vec<String> = heard.iter().map(|s| s.state.clone()).collect();
    v.dedup();
    v
}

fn steps(heard: &[RestoreStatus]) -> Vec<String> {
    let mut v: Vec<String> = heard.iter().filter_map(|s| s.step.clone()).collect();
    v.dedup();
    v
}

fn nothing_staged(st: &AppState) {
    assert!(gizai_core::restore::staged(&st.data_dir).is_none(), "nothing is staged");
    let rdir = gizai_core::restore::dir(&st.data_dir);
    let left: Vec<_> = fs::read_dir(&rdir).map(|rd| rd.flatten().map(|e| e.file_name()).collect()).unwrap_or_default();
    assert!(left.is_empty(), "nothing is left in restore/: {left:?}");
}

// ---- a restore, from start to the start after it ----

#[test]
fn a_restore_checks_saves_todays_data_in_the_settings_folder_stages_and_the_next_start_has_the_snapshots_data() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let own = tmp.path().join("own");
    fs::create_dir_all(&own).unwrap();
    let a = attach(&st, "a.txt", b"in the snapshot");
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    app::save_settings(&st, settings(&own)).unwrap();
    // today, after it: another file, and the snapshot's blob gone from the data folder (so it is copied)
    let today = attach(&st, "b.txt", b"only today");
    fs::remove_file(gizai_core::files::blob_path(&st.data_dir, &a)).unwrap();
    let heard = listen(&mut st);
    *heard.watch.lock().unwrap() = Some(own.clone());
    assert_eq!(restore::status(&st).state, "none");

    let asked = Instant::now();
    let started = restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    assert!(asked.elapsed() < Duration::from_secs(2), "start returns at once");
    assert_eq!(started.state, "checking");
    assert_eq!(started.folder.as_deref(), Some(core::backups_dir(&usb).to_string_lossy().as_ref()));
    let done = ended(&st);
    assert_eq!(done.state, "ready", "{done:?}");
    assert_eq!(done.error, None);
    assert_eq!(done.step, None);
    assert_eq!(done.snapshot.as_ref().map(|e| e.name.as_str()), Some(snap.name.as_str()));
    // today's data was saved in the folder in Settings, before anything was staged
    let saved = before_restore(&own);
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert!(before_restore(&usb).is_empty(), "not in the folder restored from");
    let saved_name = saved[0].trim_end_matches(".json").to_string();
    assert_eq!(done.saved_today.as_deref(), Some(saved_name.as_str()));
    let at_staging = heard.at_staging.lock().unwrap().clone().expect("a staging step was heard");
    assert!(at_staging.contains(&saved[0]), "made before staging: {at_staging:?}");
    // restore-changed at each step
    let all = heard.all();
    assert_eq!(states(&all), ["checking", "saving", "staging", "ready"]);
    let said = steps(&all);
    for step in ["Checking the snapshot", "Checking the database", "Checking files (1 of 1)", "Saving today's data",
                 "Copying the database", "Copying files (1 of 1)"] {
        assert!(said.iter().any(|s| s == step), "restore-changed said {step:?}: {said:?}");
    }
    assert_eq!(all.last().unwrap(), &done);
    assert!(all.iter().skip_while(|s| s.state != "staging").all(|s| s.saved_today.as_deref() == Some(saved_name.as_str())));
    // the window's JSON is contract BR's
    let json = serde_json::to_value(&done).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["error", "folder", "result", "savedToday", "snapshot", "state", "step"]);
    // today's data keeps working until the restart
    let mut both = vec![a.clone(), today.clone()];
    both.sort();
    assert_eq!(files_rows(&st), both);
    assert_eq!(restore::status(&st).state, "ready");

    // the restart: Gizai quits and starts again on the same data
    let data = st.data_dir.clone();
    drop(st);
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(st.data_dir, data);
    assert_eq!(files_rows(&st), vec![a.clone()]);
    assert!(gizai_core::files::blob_path(&st.data_dir, &a).is_file());
    let before: Vec<String> = fs::read_dir(st.data_dir.join("backups")).unwrap().flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("gizai-before-restore-")).collect();
    assert_eq!(before.len(), 1, "{before:?}");
    let after = restore::status(&st);
    assert_eq!(after.state, "none");
    let result = after.result.clone().expect("the result waits for the window");
    assert!(result.ok, "{result:?}");
    assert_eq!(result.snapshot, snap.name);
    assert_eq!(result.from_computer, snap.device.name);
    let json = serde_json::to_value(&result).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["at", "fromComputer", "message", "ok", "snapshot"]);
    assert_eq!(gizai_core::settings::get::<bool>(&st.db, "agents_paused").unwrap(), Some(true));
    // until it is dismissed (and that is sent too)
    let mut st = st;
    let heard = listen(&mut st);
    let dismissed = restore::dismiss_result(&st).unwrap();
    assert_eq!(dismissed.result, None);
    assert_eq!(restore::status(&st).result, None);
    assert_eq!(heard.all().last().map(|s| s.result.is_none()), Some(true));
    // and the start after that is a normal one
    drop(st);
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(restore::status(&st).result, None);
    assert_eq!(files_rows(&st), vec![a]);
}

#[test]
fn without_a_folder_in_settings_todays_data_is_saved_in_the_folder_restored_from() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    assert_eq!(core::settings(&st.db).folder, None);
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    let done = ended(&st);
    assert_eq!(done.state, "ready", "{done:?}");
    let saved = before_restore(&usb);
    assert_eq!(saved.len(), 1, "{:?}", manifests(&usb));
    assert_eq!(done.saved_today.map(|s| format!("{s}.json")), Some(saved[0].clone()));
}

#[cfg(unix)]
#[test]
fn when_neither_folder_can_be_written_nothing_is_staged_and_the_reason_says_what_to_do() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    // the folder in Settings is a USB disk that isn't plugged in; the folder restored from is read-only
    gizai_core::settings::set(&st.db, core::FOLDER_KEY, &tmp.path().join("unplugged").to_string_lossy().into_owned()).unwrap();
    let dir = core::backups_dir(&usb);
    let lock = |mode: u32| {
        for p in [usb.clone(), dir.clone(), dir.join("snapshots"), dir.join("files")] {
            fs::set_permissions(&p, fs::Permissions::from_mode(mode)).unwrap();
        }
    };
    lock(0o555);
    let heard = listen(&mut st);
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    let done = ended(&st);
    lock(0o755);
    assert_eq!(done.state, "failed", "{done:?}");
    let why = done.error.clone().unwrap();
    assert!(why.starts_with("Gizai can't save today's data first: ") && why.ends_with(". Choose a backups folder in Settings → General → Backups."),
            "{why}");
    assert_eq!(done.saved_today, None);
    nothing_staged(&st);
    assert!(before_restore(&usb).is_empty());
    assert_eq!(states(&heard.all()), ["checking", "saving", "failed"]);
    assert_eq!(heard.all().last().unwrap().error.as_deref(), Some(why.as_str()));
}

#[test]
fn a_snapshot_the_check_refuses_fails_with_its_reason_and_stages_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    let manifest = core::backups_dir(&usb).join("snapshots").join(format!("{}.json", snap.name));
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    v["schema_version"] = serde_json::json!(gizai_core::db::SCHEMA_VERSION + 1);
    let _ = fs::remove_file(&manifest);
    fs::write(&manifest, serde_json::to_vec(&v).unwrap()).unwrap();
    let heard = listen(&mut st);
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    let done = ended(&st);
    assert_eq!(done.state, "failed");
    assert_eq!(done.error, Some(format!("Made by a newer Gizai (database version {}). Update Gizai first.", gizai_core::db::SCHEMA_VERSION + 1)));
    nothing_staged(&st);
    assert!(before_restore(&usb).is_empty(), "today's data isn't saved for a snapshot that can't be restored");
    assert_eq!(states(&heard.all()).last().map(String::as_str), Some("failed"));
    // a restart_to_restore has nothing to restore
    assert_eq!(restore::restart_command(&st).unwrap_err(), "No restore is ready: restore a snapshot first");
}

// ---- one at a time ----

#[test]
fn start_restore_is_refused_while_a_backup_runs_and_while_another_restore_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    app::save_settings(&st, settings(&usb)).unwrap();
    let heard = listen(&mut st);
    // a backup runs (on a thread of its own: its start is sent on the thread that asks for it)
    heard.backup_gate.close();
    let st2 = st.clone();
    let backup = std::thread::spawn(move || app::back_up(&st2, None, Reason::Daily));
    until("the snapshot started", || heard.backup_gate.held.load(Ordering::SeqCst).then_some(()));
    assert_eq!(restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap_err(), "A backup is running: restore once it is done");
    assert_eq!(restore::status(&st).state, "none");
    heard.backup_gate.open();
    backup.join().unwrap().unwrap();
    assert!(st.backups.running().is_none());
    // a restore runs
    heard.restore_gate.close();
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    until("the restore's thread is held", || heard.restore_gate.held.load(Ordering::SeqCst).then_some(()));
    assert_eq!(restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap_err(), "A restore is running already");
    heard.restore_gate.open();
    assert_eq!(ended(&st).state, "ready");
}

// ---- cancel ----

#[test]
fn cancel_removes_a_ready_restore_keeps_the_before_restore_snapshot_and_the_next_start_is_a_normal_one() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    let today = attach(&st, "b.txt", b"only today");
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    assert_eq!(ended(&st).state, "ready");
    let heard = listen(&mut st);
    let cancelled = restore::cancel(&st).unwrap();
    assert_eq!(cancelled.state, "none");
    assert_eq!(heard.all().last().map(|s| s.state.as_str()), Some("none"), "restore-changed says so");
    nothing_staged(&st);
    assert_eq!(before_restore(&usb).len(), 1, "the before-restore snapshot stays");
    assert_eq!(restore::restart_command(&st).unwrap_err(), "No restore is ready: restore a snapshot first");
    drop(st);
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(restore::status(&st).result, None);
    assert_eq!(files_rows(&st), vec![today]);
}

#[test]
fn cancel_while_a_restore_runs_stops_it_and_stages_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    let heard = listen(&mut st);
    heard.restore_gate.close();
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    until("the restore's thread is held", || heard.restore_gate.held.load(Ordering::SeqCst).then_some(()));
    // cancel waits a moment for it; it is held, so it stops once let go
    let _ = restore::cancel(&st).unwrap();
    heard.restore_gate.open();
    let done = until("the restore stopped", || {
        let s = restore::status(&st);
        (s.state == "none").then_some(s)
    });
    assert_eq!(done.error, None);
    nothing_staged(&st);
    assert!(before_restore(&usb).is_empty(), "it stopped before saving today's data");
    assert_eq!(heard.all().last().map(|s| s.state.as_str()), Some("none"));
}

// ---- restart_to_restore ----

/// Writes an executable script through a child sh, so this process never holds it open for writing ("Text file busy").
#[cfg(unix)]
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

#[cfg(unix)]
#[test]
fn restart_to_restore_starts_the_same_gizai_again_without_an_update_and_restart_gizai_still_refuses() {
    use std::process::Command;
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path().canonicalize().unwrap();
    let (st, usb) = gizai(&t);
    let snap = app::back_up(&st, Some(&usb), Reason::Manual).unwrap().manifest;
    // an installed Gizai with its launcher, as the update's tests have it
    let prefix = t.join("home/.local");
    write_script(&prefix.join("lib/gizai/gizai"), "#!/bin/sh\nexit 0\n");
    write_script(&prefix.join("lib/gizai/gizai-launch"), "#!/bin/sh\nexec \"$(dirname \"$0\")/gizai\" \"$@\"\n");
    st.updates.set_exe(Some(prefix.join("lib/gizai/gizai")));
    assert_eq!(restore::restart_command(&st).unwrap_err(), "No restore is ready: restore a snapshot first");
    restore::start(&st, &usb.to_string_lossy(), &snap.name).unwrap();
    assert_eq!(ended(&st).state, "ready");

    let args = |cmd: &Command| -> Vec<String> { cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect() };
    // installed: a shell that waits for this Gizai to quit, then starts the installed one with its launcher
    let restart = restore::restart_command(&st).unwrap();
    assert_eq!(restart.get_program(), "sh");
    let a = args(&restart);
    assert_eq!(a.last().map(String::as_str), Some(prefix.join("lib/gizai/gizai-launch").to_string_lossy().as_ref()), "{a:?}");
    assert!(a.contains(&std::process::id().to_string()), "it waits for this process: {a:?}");
    assert_eq!(gizai_lib::update::restart_command(&st).unwrap_err(), "No newer Gizai is installed yet: update first");
    // not installed (a dev build): the program that runs now
    let me = std::env::current_exe().unwrap();
    st.updates.set_exe(Some(me.clone()));
    let restart = restore::restart_command(&st).unwrap();
    assert_eq!(restart.get_program(), "sh");
    let a = args(&restart);
    assert_eq!(a.last().map(String::as_str), Some(me.to_string_lossy().as_ref()), "{a:?}");
    assert!(a.contains(&std::process::id().to_string()), "{a:?}");
    assert!(gizai_lib::update::restart_command(&st).is_err(), "restart_gizai still refuses without an update");
    // where it runs from is unknown: it says so
    st.updates.set_exe(None);
    assert!(restore::restart_command(&st).unwrap_err().contains("can't start itself again"));

    // the shell itself: it waits until the process is gone (here a child that sleeps a second), then starts the program
    st.updates.set_exe(Some(me));
    let restart = restore::restart_command(&st).unwrap();
    let mut a = args(&restart);
    let marker = t.join("started");
    let program = t.join("fake-gizai");
    write_script(&program, &format!("#!/bin/sh\necho started > '{}'\n", marker.display()));
    let mut child = Command::new("sleep").arg("1").spawn().unwrap();
    let pid = std::process::id().to_string();
    let at = a.iter().position(|x| *x == pid).unwrap();
    a[at] = child.id().to_string();
    *a.last_mut().unwrap() = program.to_string_lossy().into_owned();
    let mut sh = Command::new("sh").args(&a).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "it waits while the process runs");
    child.wait().unwrap();
    until("the program started after the process quit", || marker.exists().then_some(()));
    sh.wait().unwrap();
}
