//! GA-211 with `GIZAI_RUNS_SLICE=off`: a start after a crash ends nothing in the slice, even one that is there, and
//! quitting has no slice to empty. One test in a binary of its own: it sets GIZAI_RUNS_SLICE in the test process's
//! environment. The slice is the test's own, named after its own temporary folder, set up before the switch is set.
#![cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use gizai_agents::run_memory::{self as mem, GB, RunsMemory, Slice};

#[test]
fn with_gizai_runs_slice_off_nothing_in_a_slice_is_ended() {
    let tmp = tempfile::tempdir().unwrap();
    let slice = match Slice::set_up(&mem::slice_name(tmp.path()), 2 * GB) {
        Ok(s) => s,
        Err(why) => {
            assert!(std::env::var_os("GIZAI_TEST_SLICE").is_none(), "GIZAI_TEST_SLICE is set, but there is no slice: {why}");
            eprintln!("skipped: no slice for agent runs on this Linux: {why}");
            return;
        }
    };
    assert!(!std::fs::read_to_string("/proc/self/cgroup").unwrap().contains(slice.name()));
    let mut cmd = slice.command_for("sleep".as_ref(), None).unwrap();
    let mut child = cmd.arg("600").stdin(std::process::Stdio::null()).spawn().unwrap();
    let t0 = Instant::now();
    while std::fs::read_to_string(format!("/proc/{}/comm", child.id())).unwrap_or_default().trim() != "sleep" {
        assert!(t0.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(20));
    }

    // SAFETY: the only test in this binary, before it starts anything that reads the environment again.
    unsafe { std::env::set_var("GIZAI_RUNS_SLICE", "off") };
    assert_eq!(mem::end_leftovers_in(slice.name()), vec![], "a start ends nothing");
    // a Gizai started with it off has no slice, so quitting has none to empty
    let m = RunsMemory::default();
    m.enable(slice.name(), 2 * GB);
    assert!(m.slice().is_none());
    assert!(m.made().is_none());
    std::thread::sleep(Duration::from_millis(300));
    assert!(child.try_wait().unwrap().is_none(), "the process in the slice keeps running");

    // SAFETY: as above
    unsafe { std::env::remove_var("GIZAI_RUNS_SLICE") };
    let _ = slice.remove();
    child.wait().unwrap();
}
