// GA-216 QA: backups in the app (src-tauri/src/backups.rs, contract BK). Back up now and GA-218's `back_up`, one at a
// time; `backups-changed` at the start, each step, the end and a failure, and when the settings change; a failure kept
// as the last error with no manifest left; the schedule (daily, weekly, a folder that can't be reached); Quit Gizai
// completely with On quit on, and a signal, which doesn't back up; `list_backups`; and no keychain value in the folder.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use gizai_core::backups::{self as core, BackupSettings, BackupStatus, Every, Reason, Step};
use gizai_core::ids;
use gizai_lib::backups as app;
use gizai_lib::{AppState, runs};

const HOUR: i64 = 60 * 60 * 1000;
const DAY: i64 = 24 * HOUR;

/// Holds a snapshot at its start (`backups-changed` with it running) until opened: so a test can ask for another
/// meanwhile.
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

/// What `backups-changed` sent, in order (and the gate it passes while a snapshot runs).
struct Heard {
    got: Arc<Mutex<Vec<BackupStatus>>>,
    gate: Arc<Gate>,
}

impl Heard {
    fn all(&self) -> Vec<BackupStatus> {
        self.got.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.got.lock().unwrap().clear();
    }
}

fn listen(st: &mut AppState) -> Heard {
    let got: Arc<Mutex<Vec<BackupStatus>>> = Arc::default();
    let gate: Arc<Gate> = Arc::default();
    let (seen, g) = (got.clone(), gate.clone());
    st.notify = Arc::new(move |n| {
        if let runs::Note::BackupsChanged(s) = n {
            let running = s.running.is_some();
            seen.lock().unwrap().push(s);
            if running {
                g.pass();
            }
        }
    });
    Heard { got, gate }
}

/// Gizai on new data in `tmp`, and an empty folder `usb` beside it.
fn gizai(tmp: &Path) -> (AppState, PathBuf) {
    let st = gizai_lib::test_state(tmp);
    let usb = tmp.join("usb");
    fs::create_dir_all(&usb).unwrap();
    (st, usb)
}

fn settings(folder: &Path) -> BackupSettings {
    BackupSettings { folder: Some(folder.to_string_lossy().into_owned()), ..Default::default() }
}

/// Waits up to 30 s for `ok`.
fn until<T>(what: &str, mut ok: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(t) = ok() {
            return t;
        }
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The manifests in the backups folder `chosen` stands for.
fn manifests(chosen: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(core::backups_dir(chosen).join("snapshots")).map(|rd| {
        rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".json")).collect()
    }).unwrap_or_default();
    v.sort();
    v
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() { files_under(&p, out) } else { out.push(p) }
    }
}

fn attach(st: &AppState, name: &str, bytes: &[u8]) -> String {
    let blob = gizai_core::files::store_bytes(&st.data_dir, name, bytes).unwrap();
    st.db.write(Some(&st.you_id), |w| gizai_core::files::insert(w, Some(&st.you_id), "actor", &st.you_id, &blob)).unwrap();
    blob.sha256
}

// ---- making one ----

#[test]
fn back_up_makes_a_snapshot_in_the_folder_in_settings_and_says_so_at_its_start_each_step_and_its_end() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let heard = listen(&mut st);
    let saved = app::save_settings(&st, settings(&usb)).unwrap();
    assert_eq!(saved.settings, settings(&usb));
    assert_eq!(saved.folder.as_deref(), Some(usb.join("Gizai backups").to_string_lossy().as_ref()));
    let got = heard.all();
    assert_eq!(got.len(), 1, "saving the settings says so");
    assert_eq!(got[0].settings.folder, saved.settings.folder);
    heard.clear();

    let sha = attach(&st, "plan.pdf", b"the plan");
    let made = app::back_up(&st, None, Reason::Manual).unwrap();
    let dir = usb.join("Gizai backups");
    assert_eq!(made.dir, dir);
    assert!(dir.join("gizai-backups.json").is_file() && dir.join("README.txt").is_file());
    assert_eq!(manifests(&usb), [format!("{}.json", made.manifest.name)]);
    assert!(dir.join("snapshots").join(format!("{}.db.zst", made.manifest.name)).is_file());
    assert!(dir.join("files").join(&sha[..2]).join(&sha).is_file());
    assert_eq!(made.manifest.reason, "manual");
    assert_eq!(made.manifest.device.id, st.db.device_id());

    let got = heard.all();
    let steps: Vec<Option<(Reason, String)>> = got.iter().map(|s| s.running.as_ref().map(|r| (r.reason, r.step.clone()))).collect();
    let run = |step: &str| Some((Reason::Manual, step.to_string()));
    assert_eq!(steps, [run("database"), run("files"), run("compress"), run("manifest"), run("keep"), None],
               "started, each step, ended");
    let end = got.last().unwrap();
    assert_eq!(end.last.as_ref().map(|l| l.name.clone()), Some(made.manifest.name.clone()));
    assert_eq!(end.last_error, None);
    assert_eq!(end.next, Some(made.manifest.made_at + DAY), "the next daily one is due a day later");
    assert!(app::status(&st).running.is_none());
}

#[test]
fn a_failed_snapshot_leaves_no_manifest_keeps_the_last_error_and_says_so() {
    for (step, word) in [(Step::Database, "database"), (Step::Files, "files"), (Step::Compress, "compress"), (Step::Manifest, "manifest")] {
        let tmp = tempfile::tempdir().unwrap();
        let (mut st, usb) = gizai(tmp.path());
        attach(&st, "a.txt", b"one file");
        let heard = listen(&mut st);
        app::save_settings(&st, settings(&usb)).unwrap();
        heard.clear();
        st.backups.fail_next_at(step);
        let before = ids::now_ms();
        let err = app::back_up(&st, None, Reason::Manual).unwrap_err();
        assert!(err.contains(&format!("at its {word} step")), "{err}");
        assert!(manifests(&usb).is_empty(), "at {word}");
        let mut all = vec![];
        files_under(&usb, &mut all);
        files_under(&st.data_dir.join("backups"), &mut all);
        assert!(!all.iter().any(|p| p.to_string_lossy().ends_with(".part") || p.to_string_lossy().contains(".snapshot-")),
                "at {word}: {all:?}");
        let s = app::status(&st);
        let e = s.last_error.expect("the last error");
        assert_eq!(e.message, err);
        assert!(e.at >= before);
        assert_eq!((s.running, s.last), (None, None));
        let end = heard.all().pop().unwrap();
        assert!(end.running.is_none() && end.last_error.is_some(), "backups-changed says it failed: {end:?}");
        assert_eq!(heard.all()[0].running.as_ref().map(|r| r.reason), Some(Reason::Manual), "and that it started");

        // the next one works, and clears the error
        app::back_up(&st, None, Reason::Manual).unwrap();
        let s = app::status(&st);
        assert_eq!(s.last_error, None);
        assert!(s.last.is_some());
    }
}

#[test]
fn back_up_now_starts_a_manual_snapshot_in_the_background() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    let err = app::back_up_now(&st).unwrap_err();
    assert_eq!(err, "Choose a folder for backups first");
    app::save_settings(&st, settings(&usb)).unwrap();
    let s = app::back_up_now(&st).unwrap();
    assert_eq!(s.running.as_ref().map(|r| r.reason), Some(Reason::Manual), "it answers with the snapshot running");
    let last = until("the snapshot is made", || {
        let s = app::status(&st);
        if s.running.is_none() { s.last } else { None }
    });
    assert!(last.name.starts_with("gizai-manual-"), "{}", last.name);
    assert_eq!(manifests(&usb), [format!("{}.json", last.name)]);
}

#[test]
fn only_one_snapshot_runs_at_a_time() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let heard = listen(&mut st);
    app::save_settings(&st, settings(&usb)).unwrap();
    heard.gate.close();
    let st2 = st.clone();
    let first = std::thread::spawn(move || app::back_up(&st2, None, Reason::Daily));
    until("the first snapshot started", || heard.gate.held.load(Ordering::SeqCst).then_some(()));
    let s = app::status(&st);
    assert_eq!(s.running.as_ref().map(|r| (r.reason, r.step.as_str())), Some((Reason::Daily, "database")));
    assert!(s.running.as_ref().unwrap().started_at > 0);
    assert_eq!(app::back_up_now(&st).unwrap_err(), "A backup is running already");
    assert_eq!(app::back_up(&st, None, Reason::Manual).unwrap_err(), "A backup is running already");
    assert!(app::check_schedule(&st, ids::now_ms() + 30 * DAY).is_none(), "the schedule passes while one runs");
    heard.gate.open();
    let made = first.join().unwrap().unwrap();
    assert_eq!(manifests(&usb), [format!("{}.json", made.manifest.name)], "only the first one was made");
    assert!(app::back_up_now(&st).is_ok(), "and once it has ended, another can start");
    until("the second snapshot ended", || (app::status(&st).running.is_none() && manifests(&usb).len() == 2).then_some(()));
}

#[test]
fn back_up_now_is_refused_while_the_folder_cant_be_reached_saying_why() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    app::save_settings(&st, settings(&usb)).unwrap();
    fs::rename(&usb, tmp.path().join("unplugged")).unwrap();
    let err = app::back_up_now(&st).unwrap_err();
    assert!(err.contains("can't be found"), "{err}");
    assert!(!usb.exists());
    assert!(app::status(&st).running.is_none());
}

#[test]
fn ga218s_before_restore_snapshot_goes_into_a_folder_of_its_own_and_keep_leaves_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    let own = tmp.path().join("restore-from");
    fs::create_dir_all(&own).unwrap();
    // no folder in Settings: before-restore needs none
    let made = app::back_up(&st, Some(&own), Reason::BeforeRestore).unwrap();
    assert!(made.manifest.name.starts_with("gizai-before-restore-"), "{}", made.manifest.name);
    assert_eq!(made.dir, own.join("Gizai backups"));
    app::save_settings(&st, BackupSettings { keep: 1, ..settings(&own) }).unwrap();
    app::back_up(&st, None, Reason::Manual).unwrap();
    app::back_up(&st, None, Reason::Manual).unwrap();
    let left = manifests(&own);
    assert_eq!(left.len(), 2, "the newest manual one and the before-restore one: {left:?}");
    assert!(left.contains(&format!("{}.json", made.manifest.name)));
    assert!(manifests(&usb).is_empty());
}

// ---- the schedule ----

#[test]
fn the_schedule_checks_2_minutes_after_start_then_every_hour_by_the_wall_clock() {
    assert_eq!(app::FIRST_CHECK_AFTER, Duration::from_secs(120));
    assert_eq!(app::CHECK_EVERY, Duration::from_secs(3600));
    assert!(app::TICK <= Duration::from_secs(5 * 60));
    let t = 1_792_573_322_123;
    assert!(app::check_due(None, t), "the first check");
    assert!(!app::check_due(Some(t), t + 59 * 60 * 1000));
    assert!(app::check_due(Some(t), t + HOUR));
    assert!(app::check_due(Some(t), t + 5 * HOUR), "after the computer slept");
    assert!(app::check_due(Some(t), t - 1000), "the clock went back");
    assert_eq!(app::QUIT_WAIT, Duration::from_secs(5 * 60));
}

#[test]
fn a_daily_snapshot_is_due_24_hours_after_this_computers_newest_good_one_and_a_weekly_one_7_days() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    assert!(app::check_schedule(&st, ids::now_ms()).is_none(), "backups are off without a folder");
    app::save_settings(&st, settings(&usb)).unwrap();
    // none yet: the first is due at once
    let first = app::check_schedule(&st, ids::now_ms()).expect("due").unwrap().manifest;
    assert_eq!(first.reason, "daily");
    let t = first.made_at;
    assert_eq!(app::status(&st).next, Some(t + DAY));
    assert!(app::check_schedule(&st, t + HOUR).is_none());
    assert!(app::check_schedule(&st, t + 23 * HOUR).is_none());
    let second = app::check_schedule(&st, t + DAY).expect("due a day later").unwrap().manifest;
    assert_eq!(second.reason, "daily");
    assert_eq!(manifests(&usb).len(), 2);

    app::save_settings(&st, BackupSettings { every: Every::Weekly, ..settings(&usb) }).unwrap();
    let t = second.made_at;
    assert_eq!(app::status(&st).next, Some(t + 7 * DAY));
    assert!(app::check_schedule(&st, t + DAY).is_none());
    assert!(app::check_schedule(&st, t + 6 * DAY + 23 * HOUR).is_none());
    let weekly = app::check_schedule(&st, t + 7 * DAY).expect("due a week later").unwrap().manifest;
    assert_eq!(weekly.reason, "weekly");

    app::save_settings(&st, BackupSettings { every: Every::Manual, ..settings(&usb) }).unwrap();
    assert!(app::check_schedule(&st, t + 365 * DAY).is_none(), "manual: never on its own");
    assert_eq!(app::status(&st).next, None);
    assert_eq!(manifests(&usb).len(), 3);
}

#[test]
fn a_manual_or_quit_snapshot_counts_for_the_schedule_and_a_before_restore_one_doesnt() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    app::save_settings(&st, settings(&usb)).unwrap();
    let manual = app::back_up(&st, None, Reason::Manual).unwrap().manifest;
    assert!(app::check_schedule(&st, manual.made_at + 23 * HOUR).is_none(), "a manual one is this computer's newest good one");

    let tmp2 = tempfile::tempdir().unwrap();
    let (st2, usb2) = gizai(tmp2.path());
    app::save_settings(&st2, settings(&usb2)).unwrap();
    app::back_up(&st2, None, Reason::BeforeRestore).unwrap();
    let made = app::check_schedule(&st2, ids::now_ms()).expect("due: before-restore doesn't count").unwrap();
    assert_eq!(made.manifest.reason, "daily");
}

#[test]
fn nothing_is_made_while_the_folder_cant_be_reached_and_folder_problem_says_why() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    app::save_settings(&st, settings(&usb)).unwrap();
    let away = tmp.path().join("unplugged");
    fs::rename(&usb, &away).unwrap();
    assert!(app::check_schedule(&st, ids::now_ms()).is_none(), "skipped quietly");
    assert!(app::check_schedule(&st, ids::now_ms() + 30 * DAY).is_none());
    assert!(!usb.exists(), "nothing was made at the folder's place");
    let s = app::status(&st);
    let why = s.folder_problem.expect("folderProblem");
    assert!(why.contains("can't be found") && why.contains("plugged in"), "{why}");
    assert_eq!((s.last_error, s.next, s.last), (None, None, None), "no error kept for a skipped check");
    assert!(app::list(&st, None).is_err(), "list_backups says the folder is missing");

    // a folder that is a file now
    fs::write(&usb, "a file").unwrap();
    assert!(app::check_schedule(&st, ids::now_ms()).is_none());
    let why = app::status(&st).folder_problem.unwrap();
    assert!(why.contains("is a file, not a folder"), "{why}");
    fs::remove_file(&usb).unwrap();

    // plugged in again: the next check makes one
    fs::rename(&away, &usb).unwrap();
    assert_eq!(app::status(&st).folder_problem, None);
    assert!(app::check_schedule(&st, ids::now_ms()).expect("due").is_ok());
    assert_eq!(manifests(&usb).len(), 1);
}

// ---- quitting ----

/// Asks to quit as `RunEvent::ExitRequested` does once agents have stopped; the exit hears which snapshots are in the
/// folder when it comes. Returns whether Gizai waited, and the snapshots at exit (None: no exit within 60 s).
fn quit(st: &AppState, usb: &Path) -> (bool, Option<Vec<String>>) {
    let (tx, rx) = mpsc::channel();
    let usb2 = usb.to_path_buf();
    let waited = app::on_exit_request(st, move || {
        let _ = tx.send(manifests(&usb2));
    });
    (waited, if waited { rx.recv_timeout(Duration::from_secs(60)).ok() } else { None })
}

#[test]
fn with_on_quit_on_quit_gizai_completely_makes_a_quit_snapshot_before_gizai_exits() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    app::save_settings(&st, BackupSettings { on_quit: true, ..settings(&usb) }).unwrap();
    st.backups.quit_completely();
    let (waited, at_exit) = quit(&st, &usb);
    assert!(waited, "Gizai waits for the quit's snapshot");
    let at_exit = at_exit.expect("Gizai exits");
    assert_eq!(at_exit.len(), 1, "{at_exit:?}");
    assert!(at_exit[0].starts_with("gizai-quit-"), "{at_exit:?}");
    assert!(runs::is_closing(&st), "no run starts meanwhile");
    // and a request after that quits at once, without another
    assert!(!app::on_exit_request(&st, || {}));
    assert_eq!(manifests(&usb).len(), 1);
}

#[test]
fn a_signal_or_any_other_quit_doesnt_back_up() {
    // a signal (logging out, shutting down), the restart after an update
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    app::save_settings(&st, BackupSettings { on_quit: true, ..settings(&usb) }).unwrap();
    st.backups.quit_without_backup();
    assert!(!app::on_exit_request(&st, || panic!("no wait")), "Gizai quits at once");
    // a signal after Quit Gizai completely asked, before the backup began
    let tmp2 = tempfile::tempdir().unwrap();
    let (st2, usb2) = gizai(tmp2.path());
    app::save_settings(&st2, BackupSettings { on_quit: true, ..settings(&usb2) }).unwrap();
    st2.backups.quit_completely();
    st2.backups.quit_without_backup();
    assert!(!app::on_exit_request(&st2, || {}));
    // a quit no Quit Gizai completely asked for
    let tmp3 = tempfile::tempdir().unwrap();
    let (st3, usb3) = gizai(tmp3.path());
    app::save_settings(&st3, BackupSettings { on_quit: true, ..settings(&usb3) }).unwrap();
    assert!(!app::on_exit_request(&st3, || {}));
    // Quit Gizai completely, then Stay (the page asked about unsaved text)
    let tmp4 = tempfile::tempdir().unwrap();
    let (st4, usb4) = gizai(tmp4.path());
    app::save_settings(&st4, BackupSettings { on_quit: true, ..settings(&usb4) }).unwrap();
    st4.backups.quit_completely();
    st4.backups.quit_called_off();
    assert!(!app::on_exit_request(&st4, || {}));
    std::thread::sleep(Duration::from_millis(300));
    for u in [&usb, &usb2, &usb3, &usb4] {
        assert!(manifests(u).is_empty(), "no snapshot in {}", u.display());
    }
}

#[test]
fn on_quit_is_off_by_default_and_then_quit_gizai_completely_exits_without_a_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    assert!(!core::settings(&st.db).on_quit, "off by default");
    app::save_settings(&st, settings(&usb)).unwrap();
    assert!(!core::settings(&st.db).on_quit);
    st.backups.quit_completely();
    let (waited, at_exit) = quit(&st, &usb);
    if waited {
        assert_eq!(at_exit.expect("Gizai exits at once"), Vec::<String>::new());
    }
    assert!(manifests(&usb).is_empty());
}

#[test]
fn a_second_request_while_the_quit_backs_up_quits_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let heard = listen(&mut st);
    app::save_settings(&st, BackupSettings { on_quit: true, ..settings(&usb) }).unwrap();
    heard.gate.close();
    st.backups.quit_completely();
    let (tx, rx) = mpsc::channel();
    assert!(app::on_exit_request(&st, move || { let _ = tx.send(()); }));
    until("the quit's snapshot started", || heard.gate.held.load(Ordering::SeqCst).then_some(()));
    assert_eq!(app::status(&st).running.map(|r| r.reason), Some(Reason::Quit));
    assert!(!app::on_exit_request(&st, || {}), "a second Quit quits at once");
    heard.gate.open();
    rx.recv_timeout(Duration::from_secs(60)).expect("the first request's exit comes too");
}

// ---- listing and the settings ----

#[test]
fn list_backups_lists_the_folder_in_settings_or_another_one_and_another_computers_snapshots_as_not_this_computers() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    assert!(app::list(&st, None).unwrap().is_empty(), "no folder in Settings: none");
    let tmp2 = tempfile::tempdir().unwrap();
    let (other, _) = gizai(tmp2.path());
    app::save_settings(&st, settings(&usb)).unwrap();
    let mine = app::back_up(&st, None, Reason::Manual).unwrap().manifest;
    let theirs = app::back_up(&other, Some(&usb), Reason::Daily).unwrap().manifest;
    let list = app::list(&st, None).unwrap();
    assert_eq!(list.iter().map(|e| (e.name.clone(), e.this_computer)).collect::<Vec<_>>(),
               [(theirs.name.clone(), false), (mine.name.clone(), true)], "newest first");
    assert_eq!(list[0].device_id, other.db.device_id());
    // another folder, given by its chosen folder or by the backups folder itself
    let elsewhere = tmp.path().join("old-disk");
    fs::create_dir_all(&elsewhere).unwrap();
    let old = app::back_up(&other, Some(&elsewhere), Reason::Weekly).unwrap().manifest;
    for f in [elsewhere.clone(), elsewhere.join("Gizai backups")] {
        let l = app::list(&st, Some(f.to_string_lossy().into_owned())).unwrap();
        assert_eq!(l.iter().map(|e| (e.name.clone(), e.this_computer)).collect::<Vec<_>>(), [(old.name.clone(), false)]);
    }
    // a newer schema can't be restored here, and says why
    let path = usb.join("Gizai backups/snapshots").join(format!("{}.json", theirs.name));
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    v["schema_version"] = serde_json::json!(99);
    fs::write(&path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
    let e = app::list(&st, None).unwrap().into_iter().find(|e| e.name == theirs.name).unwrap();
    assert!(!e.restorable);
    assert!(e.problem.as_deref().unwrap().contains("A newer Gizai made it"), "{:?}", e.problem);
}

#[test]
fn save_backup_settings_refuses_keep_0_and_101_and_a_folder_that_cant_be_written_saying_why() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut st, usb) = gizai(tmp.path());
    let heard = listen(&mut st);
    for keep in [0, 101] {
        let err = app::save_settings(&st, BackupSettings { keep, ..settings(&usb) }).unwrap_err();
        assert_eq!(err, format!("Keep must be 1 to 100 snapshots, not {keep}"));
    }
    let err = app::save_settings(&st, settings(&tmp.path().join("unplugged"))).unwrap_err();
    assert!(err.contains("can't be found"), "{err}");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let ro = tmp.path().join("read-only");
        fs::create_dir_all(&ro).unwrap();
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o555)).unwrap();
        if fs::write(ro.join("probe"), b"").is_err() {
            let err = app::save_settings(&st, settings(&ro)).unwrap_err();
            assert!(err.contains("can't write in"), "{err}");
        }
        fs::set_permissions(&ro, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert!(heard.all().is_empty(), "nothing changed");
    assert_eq!(core::settings(&st.db), BackupSettings::default());
    let s = app::save_settings(&st, BackupSettings { keep: 100, on_quit: true, every: Every::Weekly, ..settings(&usb) }).unwrap();
    assert_eq!((s.settings.keep, s.settings.on_quit, s.settings.every), (100, true, Every::Weekly));
    assert_eq!(heard.all().len(), 1);
}

// ---- what never goes into the folder ----

#[test]
fn no_keychain_value_ends_up_in_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, usb) = gizai(tmp.path());
    let secret = "KEYCHAIN-SECRET-3f9a17c2";
    st.keychain.set("mcp/srv/oauth", secret).unwrap();
    app::save_settings(&st, settings(&usb)).unwrap();
    let made = app::back_up(&st, None, Reason::Manual).unwrap();
    let unpacked = tmp.path().join("unpacked.db");
    core::unpack_db(&made.dir, &made.manifest.name, &unpacked).unwrap();
    let mut all = vec![unpacked];
    files_under(&usb, &mut all);
    for f in all {
        let bytes = fs::read(&f).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains(secret), "the keychain's value is in {}", f.display());
    }
}
