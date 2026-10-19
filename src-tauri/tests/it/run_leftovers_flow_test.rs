//! GA-211 end to end with the fake `claude`: what a run leaves running in its scope ends with it.
//! - Linux with a systemd user session: a run whose CLI leaves a child in a session of its own (`setsid sleep 600`) ends
//!   it at its end, after Stop too, and the run's log names each process once ("Ended 1 process this run left running:
//!   sleep (pid 12345)."); a run that left nothing gets no note. Quitting leaves no process in the slice. A start after
//!   a crash ends what the previous Gizai left in its own slice, and never touches another data folder's slice.
//! - Without the slice (a test Gizai that didn't enable it), a run ends as today: no note, and nothing outside its
//!   process group is touched.
//!
//! Each test's slice is named after its own data folder, never the slice of the Gizai these tests run in (QA's own run
//! is in that one), and is stopped and reverted when the test ends. Without a systemd user session (CI runners) the
//! slice tests say so and pass; GIZAI_TEST_SLICE=1 makes that a failure.
// Linux only: the slice, `setsid` and /proc.
#![cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_agents::run_memory::{self as mem, GB, Leftover, Slice};
use gizai_agents::stream::RunEvent;
use gizai_lib::{AppState, runs};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// A script in `dir` that runs `before` (bash) and then `exec`s `then` with its arguments, its stdin kept.
fn wrapper(dir: &Path, name: &str, before: &str, then: &str) -> String {
    let path = dir.join(name);
    let script = format!("#!/usr/bin/env bash\n{before}\nexec '{then}' \"$@\"\n");
    // written by a child, so this process never holds the script open for writing (ETXTBSY in other tests' children)
    assert!(std::process::Command::new("sh").args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod +x \"$2\"", "sh", &script])
        .arg(&path).status().unwrap().success());
    path.to_string_lossy().into_owned()
}

/// bash that starts `sleep 600` in a session of its own (`setsid`, as Puppeteer starts Chrome detached), which first
/// writes its pid to `file`; with `stubborn` it ignores SIGTERM. Its output stays the CLI's.
fn setsid_sleep(file: &Path, stubborn: bool) -> String {
    let trap = if stubborn { "trap \"\" TERM; " } else { "" };
    format!("setsid bash -c '{trap}echo $$ > \"{}\"; exec sleep 600' &", file.display())
}

/// The test Gizai's slice, stopped and reverted when the test ends.
struct Own(Slice);

impl Drop for Own {
    fn drop(&mut self) {
        let _ = self.0.remove();
    }
}

/// The test Gizai's own slice, set up now (as Gizai's start does, for its own data folder). None where this Linux has
/// no systemd user session.
fn own_slice(st: &AppState) -> Option<Own> {
    st.run_memory.enable(&mem::slice_name(&st.data_dir), 2 * GB);
    match st.run_memory.slice() {
        Some(s) => {
            assert!(!cgroup_of(std::process::id()).starts_with(s.dir()), "never the slice the tests run in");
            Some(Own(s))
        }
        None => {
            assert!(std::env::var_os("GIZAI_TEST_SLICE").is_none(), "GIZAI_TEST_SLICE is set, but there is no slice (see stderr)");
            eprintln!("skipped: no slice for agent runs on this Linux");
            None
        }
    }
}

fn cgroup_of(pid: u32) -> PathBuf {
    let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap_or_default();
    Path::new("/sys/fs/cgroup").join(text.lines().find_map(|l| l.strip_prefix("0::")).unwrap_or("").trim().trim_start_matches('/'))
}

/// Whether `pid` still runs: a zombie has ended.
fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| s.rfind(')').is_some_and(|at| !s[at + 2..].starts_with(['Z', 'X'])))
}

fn comm(pid: u32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string()
}

/// The processes still in the slice, in any of its scopes.
fn procs_in(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_to_string(dir.join("cgroup.procs")).unwrap_or_default().lines().map(String::from).collect();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            out.extend(procs_in(&e.path()));
        }
    }
    out
}

fn wait_for(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "waited {secs} s for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

async fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "waited {secs} s for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The pid a leftover wrote to `file`, once it runs sleep.
async fn pid_in(file: &Path) -> u32 {
    until(&format!("{} to be written", file.display()), 30, || std::fs::read_to_string(file).is_ok_and(|t| t.ends_with('\n'))).await;
    let pid: u32 = std::fs::read_to_string(file).unwrap().trim().parse().unwrap();
    until(&format!("{pid} to run sleep"), 10, || comm(pid) == "sleep" || !alive(pid)).await;
    pid
}

fn notes(st: &AppState, run_id: &str) -> Vec<String> {
    runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

/// The run's notes about processes Gizai ended.
fn ended_notes(st: &AppState, run_id: &str) -> Vec<String> {
    notes(st, run_id).into_iter().filter(|n| n.starts_with("Ended ") && n.contains("left running")).collect()
}

struct App { st: AppState, repo: PathBuf, tmp: tempfile::TempDir }

fn app() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    // only the runs a test starts
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    let repo = git_repo(tmp.path());
    App { st, repo, tmp }
}

impl App {
    fn card(&self, description: &str) -> String {
        let t = gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend");
        gizai_core::tasks::update(&self.st.db, &self.st.you_id, &t, gizai_core::model::TaskPatch { description_md: Some(description.into()),
            ..Default::default() }).unwrap();
        t
    }
    /// The fake Claude Code, after it ran `before`.
    fn cli(&self, name: &str, before: &str) -> String {
        wrapper(self.tmp.path(), name, before, FAKE)
    }
}

#[tokio::test]
async fn the_runs_log_names_each_process_it_left_running_once_and_a_run_that_left_nothing_gets_no_note() {
    let a = app();
    let Some(slice) = own_slice(&a.st) else { return };
    let dir = a.tmp.path();

    // one child in a session of its own
    let left = dir.join("one");
    let bin = a.cli("one.sh", &setsid_sleep(&left, false));
    let s = runs::run_once(&a.st, &a.card(""), None, Some(bin)).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let pid = pid_in(&left).await;
    assert!(!alive(pid), "the child the run left ({pid}) still runs");
    assert_eq!(ended_notes(&a.st, &s.run_id), [format!("Ended 1 process this run left running: sleep (pid {pid}).")]);

    // two, one of which ignores SIGTERM: one note, each process once
    let (l1, l2) = (dir.join("two-a"), dir.join("two-b"));
    let bin = a.cli("two.sh", &format!("{}\n{}", setsid_sleep(&l1, false), setsid_sleep(&l2, true)));
    let s = runs::run_once(&a.st, &a.card(""), None, Some(bin)).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let (p1, p2) = (pid_in(&l1).await, pid_in(&l2).await);
    assert!(!alive(p1) && !alive(p2));
    let said = ended_notes(&a.st, &s.run_id);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with("Ended 2 processes this run left running: ") && said[0].ends_with(")."), "{said:?}");
    for p in [p1, p2] {
        assert_eq!(said[0].matches(&format!("sleep (pid {p})")).count(), 1, "{said:?}");
    }
    // it is in the run's log too, where Show output reads it later
    let run = gizai_core::runs::get(&a.st.db, &s.run_id).unwrap();
    assert!(std::fs::read_to_string(&run.log_path).unwrap().contains("Ended 2 processes this run left running"));

    // a run that left nothing: no note
    let s = runs::run_once(&a.st, &a.card(""), None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert_eq!(ended_notes(&a.st, &s.run_id), Vec::<String>::new(), "{:?}", notes(&a.st, &s.run_id));

    // Stop: the child ends within 5 s, and the note says so
    let left = dir.join("stopped");
    let bin = a.cli("stopped.sh", &setsid_sleep(&left, false));
    let (run, done) = runs::start(&a.st, &a.card("FAKE_HANG"), None, Some(bin), "manual").await.unwrap();
    let pid = pid_in(&left).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let t0 = Instant::now();
    runs::stop(&a.st, &run);
    until("the child of a stopped run ends", 5, || !alive(pid)).await;
    assert!(t0.elapsed() < Duration::from_secs(5));
    let s = tokio::time::timeout(Duration::from_secs(20), done).await.expect("the run ended").unwrap();
    assert_eq!(s.status, "cancelled");
    assert_eq!(ended_notes(&a.st, &run), [format!("Ended 1 process this run left running: sleep (pid {pid}).")]);
    until("nothing is left in the slice", 5, || procs_in(slice.0.dir()).is_empty()).await;
}

#[tokio::test]
async fn after_gizai_quits_no_process_is_left_in_its_slice() {
    let a = app();
    let Some(slice) = own_slice(&a.st) else { return };
    let s = &slice.0;
    let dir = a.tmp.path();
    // a run at work, whose CLI left a child in a session of its own that ignores SIGTERM
    let left = dir.join("left");
    let bin = a.cli("hang.sh", &setsid_sleep(&left, true));
    let (run, done) = runs::start(&a.st, &a.card("FAKE_HANG"), None, Some(bin.clone()), "manual").await.unwrap();
    let pid = pid_in(&left).await;
    // and something else in the slice, in a scope of its own, that no run ends (as a run's CLI that was slow to end)
    let mut cmd = s.command_for("bash".as_ref(), None).unwrap();
    let mut stray = cmd.args(["-c", "trap '' TERM; exec sleep 600"]).stdin(std::process::Stdio::null()).spawn().unwrap();
    wait_for("the stray process runs", 10, || comm(stray.id()) == "sleep");
    assert_eq!(cgroup_of(stray.id()).parent(), Some(s.dir()));
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Gizai quits, as `quit` does: its agents are stopped, those still at work end at once, then the slice is emptied
    assert_eq!(runs::stop_all(&a.st, Duration::from_secs(12)).await, 1);
    let quit = done.await.unwrap();
    assert_eq!((quit.status.as_str(), quit.error.as_deref()), ("cancelled", Some(runs::STOPPED_BY_QUIT)));
    assert!(!alive(pid), "the run's own leftover ended at its end");
    assert_eq!(ended_notes(&a.st, &run), [format!("Ended 1 process this run left running: sleep (pid {pid}).")]);
    assert_eq!(runs::kill_all(&a.st), 0);
    let ended = gizai_lib::run_memory::end_leftovers(&a.st);
    assert_eq!(ended, vec![Leftover { pid: stray.id(), name: "sleep".into() }]);
    stray.wait().unwrap();
    assert!(procs_in(s.dir()).is_empty(), "{:?}", procs_in(s.dir()));
    // nothing more to end
    assert_eq!(gizai_lib::run_memory::end_leftovers(&a.st), vec![]);
}

#[tokio::test]
async fn gizai_exiting_at_once_kills_what_a_run_left_in_its_scope_too() {
    // asked to quit again while its runs were being stopped: `quit::end_agents` kills them (`runs::kill_all`), with
    // everything in their scopes (cgroup.kill), then the slice is emptied
    let a = app();
    let Some(slice) = own_slice(&a.st) else { return };
    let left = a.tmp.path().join("left");
    let bin = a.cli("hang.sh", &setsid_sleep(&left, true));
    let (_run, done) = runs::start(&a.st, &a.card("FAKE_HANG"), None, Some(bin), "manual").await.unwrap();
    let pid = pid_in(&left).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs::kill_all(&a.st), 1);
    until("the run's leftover is killed at once", 2, || !alive(pid)).await;
    let s = tokio::time::timeout(Duration::from_secs(10), done).await.expect("the run ended").unwrap();
    assert_eq!((s.status.as_str(), s.error.as_deref()), ("cancelled", Some(runs::STOPPED_BY_QUIT)));
    gizai_lib::run_memory::end_leftovers(&a.st);
    until("nothing is left in the slice", 5, || procs_in(slice.0.dir()).is_empty()).await;
}

#[tokio::test]
async fn at_start_after_a_crash_a_leftover_in_its_own_slice_ends_and_a_process_in_another_data_folders_slice_keeps_running() {
    let tmp = tempfile::tempdir().unwrap();
    // two data folders, as test_state names them: the crashed Gizai's, and another Gizai's
    let (ours, theirs) = (tmp.path().join("ours"), tmp.path().join("theirs"));
    for d in [&ours, &theirs] {
        std::fs::create_dir_all(d.join("data")).unwrap();
    }
    let set_up = |d: &Path| match Slice::set_up(&mem::slice_name(&d.join("data")), 2 * GB) {
        Ok(s) => Some(Own(s)),
        Err(why) => {
            assert!(std::env::var_os("GIZAI_TEST_SLICE").is_none(), "GIZAI_TEST_SLICE is set, but there is no slice: {why}");
            eprintln!("skipped: no slice for agent runs on this Linux: {why}");
            None
        }
    };
    let Some(our_slice) = set_up(&ours) else { return };
    let Some(their_slice) = set_up(&theirs) else { return };
    // what the crashed Gizai left in its slice: a run's CLI, and a child it started in a session of its own that
    // ignores SIGTERM; and another Gizai's run in its own slice
    let start = |s: &Slice, script: &str| {
        let mut cmd = s.command_for("bash".as_ref(), None).unwrap();
        let child = cmd.args(["-c", script]).stdin(std::process::Stdio::null()).spawn().unwrap();
        wait_for("it runs sleep", 10, || comm(child.id()) == "sleep");
        child
    };
    let left = tmp.path().join("left");
    let mut cli = start(&our_slice.0, &format!("{}\nexec sleep 600", setsid_sleep(&left, true)));
    let leftover = pid_in(&left).await;
    let mut other = start(&their_slice.0, "exec sleep 600");

    // Gizai starts on its data folder, as after a crash
    let t0 = Instant::now();
    let st = gizai_lib::test_state(&ours);
    assert!(t0.elapsed() >= Duration::from_millis(2_900), "the one that ignores SIGTERM got SIGKILL after the grace: {:?}", t0.elapsed());
    cli.wait().unwrap();
    assert!(!alive(cli.id()) && !alive(leftover), "what the crashed Gizai left is gone once it has started");
    assert!(procs_in(our_slice.0.dir()).is_empty(), "{:?}", procs_in(our_slice.0.dir()));
    assert!(alive(other.id()), "another data folder's slice keeps its process");
    assert_eq!(cgroup_of(other.id()).parent(), Some(their_slice.0.dir()));
    // and a second start finds nothing more to end
    assert_eq!(gizai_lib::run_memory::end_leftovers_at_start(&st.data_dir), vec![]);
    drop(their_slice);
    other.wait().unwrap();
}

#[tokio::test]
async fn without_the_slice_a_run_ends_as_today_with_no_note() {
    // a test Gizai doesn't enable the slice: its runs start and end as before GA-211, only their process group ends
    let a = app();
    assert!(a.st.run_memory.slice().is_none());
    let left = a.tmp.path().join("left");
    let bin = a.cli("one.sh", &setsid_sleep(&left, false));
    let s = runs::run_once(&a.st, &a.card(""), None, Some(bin)).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let pid = pid_in(&left).await;
    assert_eq!(ended_notes(&a.st, &s.run_id), Vec::<String>::new());
    assert_eq!(gizai_lib::run_memory::end_leftovers(&a.st), vec![], "no slice to empty when it quits");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let still = alive(pid);
    // the exact pid the test's own fake started, still that sleep
    if still && comm(pid) == "sleep" {
        // SAFETY: a signal to that one process only
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }
    assert!(still, "outside the slice nothing ends it, as before");
}
