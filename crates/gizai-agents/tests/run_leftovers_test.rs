//! GA-211: what a program leaves running in its scope in the agent runs' slice ends with it. A run's CLI, a preview and
//! a worktree's setup command that start a process in a group of its own (`setsid`, as Puppeteer starts Chrome detached)
//! leave it in their scope; at their end it gets SIGTERM, and SIGKILL 3 s later, and each process is named once. Quitting
//! ends whatever is still in the slice (`Slice::end_leftovers`), and so does a start after a crash (`end_leftovers_in`),
//! never in another data folder's slice. Without the slice nothing changes.
//!
//! Linux with a systemd user session: every test has a slice of its own, named after its own temporary folder, never the
//! slice of the Gizai the tests run in (QA's own run is in that one), and stopped and reverted when it ends. Where there
//! is no systemd user session (CI runners) the slice tests say so and pass; GIZAI_TEST_SLICE=1 makes that a failure.
use gizai_agents::run_memory::{self as mem, Leftover};

#[test]
fn the_text_names_each_process_once_and_counts_them() {
    let l = |pid, name: &str| Leftover { pid, name: name.into() };
    assert_eq!(mem::leftovers_text(&[l(12345, "chromium")], "this run"), "1 process this run left running: chromium (pid 12345)");
    // the same pid twice is one process
    assert_eq!(mem::leftovers_text(&[l(12345, "chromium"), l(12346, "node"), l(12345, "chromium")], "this run"),
               "2 processes this run left running: chromium (pid 12345), node (pid 12346)");
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    use gizai_agents::os::{self, End};
    use gizai_agents::process::{AFTER_RESULT, Caps, ClaudeLines, Exec, LEFTOVER, OOM_KILL, leftover, spawn_exec};
    use gizai_agents::run_memory::{self as mem, GB, LEFTOVER_GRACE, Leftover, Slice};
    use gizai_agents::stream::RunEvent;

    const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude.sh");

    /// The test's own slice, stopped and reverted when it goes.
    struct Own(Slice);

    impl Drop for Own {
        fn drop(&mut self) {
            let _ = self.0.remove();
        }
    }

    /// A slice of the test's own for the data folder `dir`; None where this Linux has no systemd user session.
    fn own_slice(dir: &Path, max: u64) -> Option<Own> {
        match Slice::set_up(&mem::slice_name(dir), max) {
            Ok(s) => {
                // never the slice the tests themselves run in
                assert!(!cgroup_of(std::process::id()).starts_with(s.dir()), "{}", s.dir().display());
                Some(Own(s))
            }
            Err(why) => {
                assert!(std::env::var_os("GIZAI_TEST_SLICE").is_none(), "GIZAI_TEST_SLICE is set, but there is no slice: {why}");
                eprintln!("skipped: no slice for agent runs on this Linux: {why}");
                None
            }
        }
    }

    fn cgroup_in(text: &str) -> PathBuf {
        Path::new("/sys/fs/cgroup").join(text.lines().find_map(|l| l.strip_prefix("0::")).unwrap().trim().trim_start_matches('/'))
    }

    fn cgroup_of(pid: u32) -> PathBuf {
        cgroup_in(&std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap())
    }

    /// `/proc/<pid>/stat`'s fields after the program's name: state, ppid, pgrp, session, …
    fn stat_fields(pid: u32) -> Vec<String> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        text.rfind(')').map(|at| text[at + 2..].split_whitespace().map(String::from).collect()).unwrap_or_default()
    }

    /// Whether `pid` still runs: a zombie has ended, only its parent's wait is missing.
    fn alive(pid: u32) -> bool {
        stat_fields(pid).first().is_some_and(|s| s != "Z" && s != "X")
    }

    fn comm(pid: u32) -> String {
        std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string()
    }

    fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
        let t0 = Instant::now();
        while !ok() {
            assert!(t0.elapsed() < Duration::from_secs(secs), "waited {secs} s for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The pid a leftover wrote to `file`, once it runs `name` (it wrote it, then exec'd).
    fn pid_in(file: &Path, name: &str) -> u32 {
        until(&format!("{} to be written", file.display()), 20, || std::fs::read_to_string(file).is_ok_and(|t| t.ends_with('\n')));
        let pid: u32 = std::fs::read_to_string(file).unwrap().trim().parse().unwrap();
        until(&format!("{pid} to run {name}"), 10, || comm(pid) == name || !alive(pid));
        pid
    }

    /// The processes still in the cgroup folder `dir` and below it.
    fn procs_in(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_to_string(dir.join("cgroup.procs")).unwrap_or_default().lines().map(String::from).collect();
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                out.extend(procs_in(&e.path()));
            }
        }
        out
    }

    /// A script `name` in `dir`, written by a child, so this process never holds it open for writing (ETXTBSY in other
    /// tests' children).
    fn script(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        assert!(std::process::Command::new("sh").args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod +x \"$2\"", "sh", text])
            .arg(&path).status().unwrap().success());
        path
    }

    /// bash that starts `sleep 600` in a session of its own (`setsid`), which writes its pid to `file` first. Its stdout
    /// and stderr stay those of whoever started it, as a browser started detached keeps them.
    fn setsid_sleep(file: &Path) -> String {
        format!("setsid bash -c 'echo $$ > \"{}\"; exec sleep 600' &", file.display())
    }

    /// A fake CLI that first writes the cgroup it runs in to `<dir>/cgroup`, runs `before`, and then is the fake Claude
    /// Code (`fake-claude.sh`).
    fn cli(dir: &Path, before: &str) -> PathBuf {
        script(dir, "cli.sh", &format!("#!/usr/bin/env bash\ncat /proc/$$/cgroup > '{}'\n{before}\nexec '{FAKE}' \"$@\"\n",
                                       dir.join("cgroup").display()))
    }

    fn others(evs: &[RunEvent]) -> Vec<String> {
        evs.iter().filter_map(|e| match e { RunEvent::Other { raw_type } => Some(raw_type.clone()), _ => None }).collect()
    }

    enum Ends {
        /// The CLI finishes by itself.
        Finished,
        /// Stop.
        Stop,
        /// The time cap.
        TimeLimit,
    }

    /// A run in `slice` (None: outside it) whose CLI leaves `setsid sleep 600` behind, ended as `how`. Returns its events,
    /// the leftover's pid, the run's scope (from the CLI's own /proc/<pid>/cgroup) and when the run ended.
    async fn run_leaving_a_child(tmp: &Path, slice: Option<&Slice>, how: Ends) -> (Vec<RunEvent>, u32, PathBuf, Instant) {
        let left = tmp.join("left");
        let bin = cli(tmp, &setsid_sleep(&left));
        let (stdin, max_time) = match how {
            Ends::Finished => ("p", 120),
            Ends::Stop => ("FAKE_HANG", 120),
            Ends::TimeLimit => ("FAKE_HANG", 2),
        };
        let exec = Exec { bin, args: vec!["-p".into()], env: vec![], stdin: stdin.into(), slice: slice.cloned() };
        let caps = Caps { max_time: Duration::from_secs(max_time), max_tool_calls: 80, after_result: AFTER_RESULT };
        let mut h = spawn_exec::<RunEvent, _>(&exec, tmp, &tmp.join("r.jsonl"), caps, ClaudeLines).unwrap();
        let pid = pid_in(&left, "sleep");
        // the scenario: the child runs in a session and process group of its own, outside the run's group
        let (f, me) = (stat_fields(pid), pid.to_string());
        assert_eq!((&f[2], &f[3]), (&me, &me), "its own group and session: {f:?}");
        assert_ne!(f[2], h.pid.to_string());
        let mut evs = vec![];
        let mut ended = None;
        if matches!(how, Ends::Stop) {
            let first = h.events.recv().await.expect("the CLI's first line");
            evs.push(first);
            tokio::time::sleep(Duration::from_millis(200)).await;
            ended = Some(Instant::now());
            h.stop.stop();
        }
        while let Some(e) = h.events.recv().await {
            match &e {
                // the CLI's last line, just before it exits
                RunEvent::Result { .. } if ended.is_none() => ended = Some(Instant::now()),
                RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time" => ended = Some(Instant::now()),
                _ => {}
            }
            evs.push(e);
        }
        let scope = cgroup_in(&std::fs::read_to_string(tmp.join("cgroup")).unwrap());
        (evs, pid, scope, ended.expect("the run ended"))
    }

    /// The leftover `pid` is gone, and so is the run's scope, within 5 s of the run's end; the run heard of it once, as
    /// `leftover:<pid> sleep`, before its exit.
    fn ended_with_the_run(evs: &[RunEvent], pid: u32, scope: &Path, slice: &Slice, ended: Instant, exit: &str) {
        assert_eq!(scope.parent(), Some(slice.dir()), "the CLI ran in a scope of its own in the slice: {}", scope.display());
        let five = ended + Duration::from_secs(5);
        while alive(pid) && Instant::now() < five {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!alive(pid), "the child it left ({pid}) still runs 5 s after the run's end");
        while scope.exists() && Instant::now() < five {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!scope.exists(), "the run's scope is still there 5 s after its end: {}", scope.display());
        let others = others(evs);
        let marked: Vec<&String> = others.iter().filter(|o| o.starts_with(LEFTOVER)).collect();
        assert_eq!(marked, [&format!("{LEFTOVER}{pid} sleep")], "named once: {others:?}");
        assert_eq!(leftover(marked[0]), Some(Leftover { pid, name: "sleep".into() }));
        assert_eq!(others.last().map(String::as_str), Some(exit), "the exit comes last: {others:?}");
    }

    #[tokio::test]
    async fn a_run_that_finishes_ends_the_child_it_left_in_a_session_of_its_own_within_5_s_and_its_scope_goes_too() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (evs, pid, scope, ended) = run_leaving_a_child(tmp.path(), Some(&s.0), Ends::Finished).await;
        assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { .. })), "{evs:?}");
        ended_with_the_run(&evs, pid, &scope, &s.0, ended, "exit:0");
        assert!(procs_in(s.0.dir()).is_empty(), "nothing is left in the slice: {:?}", procs_in(s.0.dir()));
    }

    #[tokio::test]
    async fn after_stop_the_child_a_run_left_is_ended_within_5_s_and_its_scope_goes_too() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (evs, pid, scope, stopped) = run_leaving_a_child(tmp.path(), Some(&s.0), Ends::Stop).await;
        ended_with_the_run(&evs, pid, &scope, &s.0, stopped, "exit:130");
    }

    #[tokio::test]
    async fn at_the_time_limit_the_child_a_run_left_is_ended_within_5_s_and_its_scope_goes_too() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (evs, pid, scope, capped) = run_leaving_a_child(tmp.path(), Some(&s.0), Ends::TimeLimit).await;
        assert!(others(&evs).iter().any(|o| o == "cap_exceeded:time"), "{:?}", others(&evs));
        ended_with_the_run(&evs, pid, &scope, &s.0, capped, "exit:130");
    }

    #[tokio::test]
    async fn a_run_that_leaves_nothing_hears_of_no_leftover() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let exec = Exec { bin: cli(tmp.path(), ""), args: vec!["-p".into()], env: vec![], stdin: "p".into(), slice: Some(s.0.clone()) };
        let mut h = spawn_exec::<RunEvent, _>(&exec, tmp.path(), &tmp.path().join("r.jsonl"),
                                              Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }, ClaudeLines).unwrap();
        let t0 = Instant::now();
        let mut evs = vec![];
        while let Some(e) = h.events.recv().await {
            evs.push(e);
        }
        let others = others(&evs);
        assert!(!others.iter().any(|o| o.starts_with(LEFTOVER)), "{others:?}");
        assert_eq!(others.last().map(String::as_str), Some("exit:0"));
        assert!(t0.elapsed() < Duration::from_secs(3), "an empty scope costs no wait: {:?}", t0.elapsed());
    }

    #[tokio::test]
    async fn a_runs_end_never_touches_another_run_at_work_in_the_same_slice_nor_what_that_run_left() {
        // agent runs share the slice: one run's end ends only what is in its own scope
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (one, two) = (tmp.path().join("one"), tmp.path().join("two"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        // the other run: at work, and it left a child in a session of its own too
        let other_left = two.join("left");
        let exec = Exec { bin: cli(&two, &setsid_sleep(&other_left)), args: vec!["-p".into()], env: vec![], stdin: "FAKE_HANG".into(),
                          slice: Some(s.0.clone()) };
        let mut other = spawn_exec::<RunEvent, _>(&exec, &two, &two.join("r.jsonl"),
                                                  Caps { max_time: Duration::from_secs(120), max_tool_calls: 80, after_result: AFTER_RESULT }, ClaudeLines).unwrap();
        let other_child = pid_in(&other_left, "sleep");
        let other_first = other.events.recv().await.expect("the other CLI's first line");

        // this run leaves a child and finishes: its child ends, in its own scope
        let (evs, pid, scope, ended) = run_leaving_a_child(&one, Some(&s.0), Ends::Finished).await;
        ended_with_the_run(&evs, pid, &scope, &s.0, ended, "exit:0");
        let other_scope = cgroup_in(&std::fs::read_to_string(two.join("cgroup")).unwrap());
        assert_ne!(other_scope, scope, "a scope each");
        assert_eq!(other_scope.parent(), Some(s.0.dir()));
        assert!(alive(other.pid), "the other run's CLI keeps running");
        assert!(alive(other_child), "and so does what the other run left, until that run ends");
        assert_eq!(cgroup_of(other_child), other_scope);

        // the other run's end ends its own leftover
        other.stop.stop();
        let mut other_evs = vec![other_first];
        while let Some(e) = other.events.recv().await {
            other_evs.push(e);
        }
        until("the other run's leftover ends", 5, || !alive(other_child));
        let others = others(&other_evs);
        let marked: Vec<&String> = others.iter().filter(|o| o.starts_with(LEFTOVER)).collect();
        assert_eq!(marked, [&format!("{LEFTOVER}{other_child} sleep")], "{others:?}");
        until("nothing is left in the slice", 5, || procs_in(s.0.dir()).is_empty());
    }

    #[test]
    fn a_leftover_that_ignores_sigterm_gets_it_first_and_sigkill_3_s_later_and_one_that_doesnt_ends_at_once() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (stubborn, plain, term_log) = (tmp.path().join("stubborn"), tmp.path().join("plain"), tmp.path().join("term.log"));
        // python3 that notes each SIGTERM and goes on; its pid is written once its handler is in place
        let py = "import os, signal, sys, time\n\
                  def term(*_):\n    with open(sys.argv[1], 'a') as f: f.write('TERM\\n')\n\
                  signal.signal(signal.SIGTERM, term)\n\
                  with open(sys.argv[2] + '.part', 'w') as f: f.write(str(os.getpid()) + '\\n')\n\
                  os.rename(sys.argv[2] + '.part', sys.argv[2])\n\
                  while True: time.sleep(600)\n";
        let py_file = script(tmp.path(), "stubborn.py", py);
        let program = format!("setsid python3 '{}' '{}' '{}' > /dev/null 2>&1 &\n{}\nuntil [ -s '{}' ] && [ -s '{}' ]; do sleep 0.05; done\n",
                              py_file.display(), term_log.display(), stubborn.display(), setsid_sleep(&plain), stubborn.display(), plain.display());
        let (mut cmd, scope) = s.0.scoped_for("bash".as_ref(), None).expect("bash is installed");
        cmd.arg("-c").arg(&program).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let (mut child, tree) = os::spawn_tree(&mut cmd, false).unwrap();
        let group = child.id();
        assert!(child.wait().unwrap().success());
        tree.end(End::Kill);
        let (stubborn, plain) = (pid_in(&stubborn, "python3"), pid_in(&plain, "sleep"));
        assert_eq!(scope.dir().parent(), Some(s.0.dir()));
        assert!(scope.dir().file_name().unwrap().to_string_lossy().starts_with("gizai-"), "{}", scope.dir().display());
        assert_eq!(cgroup_of(stubborn), scope.dir(), "the leftovers run in the program's scope");
        assert_eq!(cgroup_of(plain), scope.dir());

        let t0 = Instant::now();
        let ending = std::thread::spawn(move || scope.end_leftovers(Some(group)));
        // SIGTERM first: the plain one ends at once, the stubborn one notes it and runs on
        until("the plain leftover ends on SIGTERM", 1, || !alive(plain));
        until("the stubborn leftover hears SIGTERM", 1, || std::fs::read_to_string(&term_log).is_ok_and(|t| t.contains("TERM")));
        std::thread::sleep(Duration::from_millis(2_000).saturating_sub(t0.elapsed()));
        assert!(alive(stubborn), "SIGKILL waits {LEFTOVER_GRACE:?}");
        let left = ending.join().unwrap();
        let took = t0.elapsed();
        assert!(took >= Duration::from_millis(2_900) && took < Duration::from_secs(5), "SIGKILL 3 s after SIGTERM: {took:?}");
        until("the stubborn leftover ends", 1, || !alive(stubborn));
        assert_eq!(std::fs::read_to_string(&term_log).unwrap().matches("TERM").count(), 1, "one SIGTERM");
        // each once, as the kernel names it
        let mut got = left.clone();
        got.sort_by_key(|l| l.pid);
        let mut want = vec![Leftover { pid: stubborn, name: "python3".into() }, Leftover { pid: plain, name: "sleep".into() }];
        want.sort_by_key(|l| l.pid);
        assert_eq!(got, want, "{left:?}");
        assert!(procs_in(s.0.dir()).is_empty(), "{:?}", procs_in(s.0.dir()));
    }

    #[test]
    fn an_empty_scope_has_no_leftovers_and_costs_no_wait() {
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (mut cmd, scope) = s.0.scoped_for("true".as_ref(), None).unwrap();
        let (mut child, _tree) = os::spawn_tree(cmd.stdin(Stdio::null()), false).unwrap();
        let group = child.id();
        child.wait().unwrap();
        let t0 = Instant::now();
        assert_eq!(scope.end_leftovers(Some(group)), vec![]);
        assert!(t0.elapsed() < Duration::from_millis(500), "{:?}", t0.elapsed());
        // and a second look at a scope that is gone finds nothing either
        until("the scope goes", 5, || !scope.dir().exists());
        assert_eq!(scope.end_leftovers(None), vec![]);
    }

    #[tokio::test]
    async fn the_oom_count_stays_the_same_when_the_run_also_leaves_a_child() {
        // as `a_cli_started_in_the_slice_runs_there_and_its_events_say_when_linux_ended_a_process_in_it_at_the_limit`
        // (run_memory_test.rs), with a child left in a session of its own: Linux ended one process, and the run hears
        // `oom_kill:1` once; ending the leftover isn't counted
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), GB) else { return };
        let left = tmp.path().join("left");
        let bin = cli(tmp.path(), &format!("{}\npython3 -c \"b = b'x' * (2 << 30)\"\necho \"build: $?\" >&2", setsid_sleep(&left)));
        let exec = Exec { bin, args: vec!["-p".into()], env: vec![], stdin: "p".into(), slice: Some(s.0.clone()) };
        let mut h = spawn_exec::<RunEvent, _>(&exec, tmp.path(), &tmp.path().join("r.jsonl"),
                                              Caps { max_time: Duration::from_secs(120), max_tool_calls: 80, after_result: AFTER_RESULT }, ClaudeLines).unwrap();
        let mut evs = vec![];
        while let Some(e) = h.events.recv().await {
            evs.push(e);
        }
        let pid = pid_in(&left, "sleep");
        let others = others(&evs);
        let ooms: Vec<&String> = others.iter().filter(|o| o.starts_with(OOM_KILL)).collect();
        assert_eq!(ooms, [&format!("{OOM_KILL}1")], "{others:?}");
        assert!(others.contains(&format!("{LEFTOVER}{pid} sleep")), "{others:?}");
        assert_eq!(others.last().map(String::as_str), Some("exit:0"));
        assert!(!alive(pid));
        assert!(std::fs::read_to_string(tmp.path().join("r.stderr.log")).unwrap().contains("build: 137"), "the build was killed");
    }

    #[tokio::test]
    async fn a_preview_stopped_at_the_runs_end_ends_what_it_left_in_its_scope_and_says_so() {
        use gizai_agents::preview;
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(wt.join(gizai_agents::worktree::TEMP_DIR)).unwrap();
        let port = preview::free_port(49_800, 49_899, &[]).expect("a free block");
        let left = tmp.path().join("left");
        // a server it starts in a group of its own (its output to the preview's log), then the preview itself
        let p = preview::start(&format!("{} exec sleep 600", setsid_sleep(&left)), &wt, port, None, Some(&s.0)).unwrap();
        let pid = pid_in(&left, "sleep");
        let group = p.pid();
        assert_ne!(stat_fields(pid)[2], group.to_string(), "outside the preview's group");
        let scope = cgroup_of(pid);
        assert_eq!(scope.parent(), Some(s.0.dir()), "in the preview's scope: {}", scope.display());
        assert_eq!(cgroup_of(group), scope);
        let t0 = Instant::now();
        let ended = p.stop().await;
        assert_eq!(ended, vec![Leftover { pid, name: "sleep".into() }]);
        assert!(!alive(pid) && !alive(group));
        assert!(t0.elapsed() < Duration::from_secs(5), "{:?}", t0.elapsed());
        until("the preview's scope goes", 5, || !scope.exists());

        // one that left nothing returns nothing
        let p = preview::start("exec sleep 600", &wt, port, None, Some(&s.0)).unwrap();
        until("the preview runs", 10, || comm(p.pid()) == "sleep");
        assert_eq!(p.stop().await, vec![]);
    }

    #[tokio::test]
    async fn a_preview_dropped_without_stop_kills_what_it_left_in_its_scope_too() {
        use gizai_agents::preview;
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(wt.join(gizai_agents::worktree::TEMP_DIR)).unwrap();
        let port = preview::free_port(49_800, 49_899, &[]).expect("a free block");
        let left = tmp.path().join("left");
        let p = preview::start(&format!("{} exec sleep 600", setsid_sleep(&left)), &wt, port, None, Some(&s.0)).unwrap();
        let pid = pid_in(&left, "sleep");
        drop(p);
        until("the leftover of a dropped preview ends", 5, || !alive(pid));
    }

    #[test]
    fn a_setup_command_that_leaves_a_child_in_a_session_of_its_own_ends_it_and_doesnt_wait_for_it() {
        use gizai_agents::prepare::{Prepare, prepare_in};
        let tmp = tempfile::tempdir().unwrap();
        let Some(s) = own_slice(tmp.path(), 2 * GB) else { return };
        let (main, wt) = (tmp.path().join("main"), tmp.path().join("wt"));
        std::fs::create_dir_all(&main).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        // it keeps the setup command's output open, as a daemon that doesn't close it would
        let left = tmp.path().join("left");
        let p = Prepare { copy: vec![], install: false, setup: Some(setsid_sleep(&left)) };
        let t0 = Instant::now();
        prepare_in(&main, &wt, &p, None, None, Some(&s.0)).unwrap();
        assert!(t0.elapsed() < Duration::from_secs(10), "{:?}", t0.elapsed());
        let pid: u32 = std::fs::read_to_string(&left).unwrap().trim().parse().unwrap();
        until("the setup command's leftover ends", 5, || !alive(pid));
        until("nothing is left in the slice", 5, || procs_in(s.0.dir()).is_empty());
    }

    /// `sleep 600` in a scope of its own in `slice`, which (with `stubborn`) ignores SIGTERM; the test waits for it.
    fn sleep_in(slice: &Slice, stubborn: bool) -> std::process::Child {
        let mut cmd = slice.command_for("bash".as_ref(), None).unwrap();
        cmd.arg("-c").arg(if stubborn { "trap '' TERM; exec sleep 600" } else { "exec sleep 600" }).stdin(Stdio::null());
        let child = cmd.spawn().unwrap();
        until("it runs sleep", 10, || comm(child.id()) == "sleep");
        child
    }

    #[test]
    fn quitting_ends_everything_in_its_own_slice_in_every_scope_and_nothing_in_another_slice() {
        let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let Some(s) = own_slice(one.path(), 2 * GB) else { return };
        let Some(other) = own_slice(two.path(), 2 * GB) else { return };
        let (mut a, mut b) = (sleep_in(&s.0, false), sleep_in(&s.0, true));
        let mut c = sleep_in(&other.0, false);
        assert_ne!(cgroup_of(a.id()), cgroup_of(b.id()), "a scope each");
        let t0 = Instant::now();
        let mut ended = s.0.end_leftovers();
        ended.sort_by_key(|l| l.pid);
        let mut want = vec![Leftover { pid: a.id(), name: "sleep".into() }, Leftover { pid: b.id(), name: "sleep".into() }];
        want.sort_by_key(|l| l.pid);
        assert_eq!(ended, want);
        assert!(t0.elapsed() >= Duration::from_millis(2_900), "the stubborn one got SIGKILL after the grace: {:?}", t0.elapsed());
        a.wait().unwrap();
        b.wait().unwrap();
        assert!(procs_in(s.0.dir()).is_empty(), "{:?}", procs_in(s.0.dir()));
        assert!(alive(c.id()), "another data folder's slice is never touched");
        // a slice that is empty has nothing to end
        assert_eq!(s.0.end_leftovers(), vec![]);
        drop(other);
        c.wait().unwrap();
    }

    #[test]
    fn a_start_after_a_crash_ends_what_is_left_in_its_own_slice_by_name_and_never_in_another() {
        let (one, two, three) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let Some(s) = own_slice(one.path(), 2 * GB) else { return };
        let Some(other) = own_slice(two.path(), 2 * GB) else { return };
        let (mut a, mut b) = (sleep_in(&s.0, false), sleep_in(&s.0, true));
        let mut c = sleep_in(&other.0, false);
        // found by its name alone, where your user's systemd keeps it, without setting it up
        let mut ended = mem::end_leftovers_in(&mem::slice_name(one.path()));
        ended.sort_by_key(|l| l.pid);
        let mut want = vec![Leftover { pid: a.id(), name: "sleep".into() }, Leftover { pid: b.id(), name: "sleep".into() }];
        want.sort_by_key(|l| l.pid);
        assert_eq!(ended, want);
        a.wait().unwrap();
        b.wait().unwrap();
        assert!(alive(c.id()), "another data folder's slice keeps its process");
        // a data folder whose slice was never made has nothing to end
        assert_eq!(mem::end_leftovers_in(&mem::slice_name(three.path())), vec![]);
        assert!(alive(c.id()));
        drop(other);
        c.wait().unwrap();
    }

    #[tokio::test]
    async fn without_the_slice_a_run_ends_as_before_and_what_it_left_in_a_session_of_its_own_runs_on() {
        // today's behaviour, kept where there is no slice (macOS, Windows, no user session, GIZAI_RUNS_SLICE=off): only
        // the run's process group ends; the test ends the child it knows itself
        let tmp = tempfile::tempdir().unwrap();
        let (evs, pid, scope, _) = run_leaving_a_child(tmp.path(), None, Ends::Finished).await;
        let others = others(&evs);
        assert!(!others.iter().any(|o| o.starts_with(LEFTOVER)), "{others:?}");
        assert_eq!(others.last().map(String::as_str), Some("exit:0"));
        assert_eq!(scope, cgroup_of(std::process::id()), "it ran where the test runs");
        std::thread::sleep(Duration::from_millis(500));
        let still = alive(pid);
        // SAFETY: the exact pid the test's own fake started; it is still that sleep (checked just before)
        if still && comm(pid) == "sleep" {
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        }
        assert!(still, "outside the slice nothing ends it, as before");
    }
}
