//! GA-202: a Gizai on a Linux without a systemd user session (another init system, a container): its start sets up no
//! slice, runs start as today, outside any slice, Settings → Runs says the limit isn't enforced (so the page says Gizai
//! only waits for free memory), and the start check still applies. One test in a binary of its own: it takes
//! XDG_RUNTIME_DIR out of the test process's environment.
#![cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

use gizai_agents::run_memory::{GB, Reading};
use gizai_lib::runs;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn cgroup_in(file: &Path) -> PathBuf {
    let text = std::fs::read_to_string(file).unwrap();
    Path::new("/sys/fs/cgroup").join(text.lines().find_map(|l| l.strip_prefix("0::")).unwrap().trim().trim_start_matches('/'))
}

#[tokio::test]
async fn without_a_systemd_user_session_runs_start_as_today_and_settings_says_only_the_start_check_applies() {
    // SAFETY: the only test in this binary, before it starts anything that reads the environment.
    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    // as Gizai's start does
    gizai_lib::run_memory::start(&st);
    assert!(st.run_memory.slice().is_none(), "no slice without a user session");
    assert!(!st.run_memory.enforced());
    let s = runs::get_settings(&st);
    assert!(!s.runs_memory_enforced, "Settings → Runs: only the start check applies");
    assert_eq!(s.runs_memory_gb, Some(gizai_lib::run_memory::default_gb()));

    // a run starts as today, where Gizai runs (here: the test's own cgroup)
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    let out = tmp.path().join("cgroup");
    let bin = tmp.path().join("claude.sh");
    let script = format!("#!/usr/bin/env bash\ncat /proc/$$/cgroup > '{}'\nexec '{FAKE}' \"$@\"\n", out.display());
    assert!(std::process::Command::new("sh").args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod +x \"$2\"", "sh", &script])
        .arg(&bin).status().unwrap().success());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let summary = runs::run_once(&st, &task, None, Some(bin.to_string_lossy().into_owned())).await.unwrap();
    assert_eq!(summary.status, "succeeded", "{summary:?}");
    let own = cgroup_in(Path::new("/proc/self/cgroup"));
    assert_eq!(cgroup_in(&out), own, "the run ran where Gizai runs, as before");

    // the start check still applies: the runs' memory can't be read, the computer's free memory can
    assert_eq!(st.run_memory.reading().used, None);
    assert!(st.run_memory.reading().available.is_some());
    st.run_memory.fake_reading(Some(Reading { used: None, available: Some(GB) }));
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let err = runs::start(&st, &task, None, Some(FAKE.into()), "manual").await.unwrap_err();
    assert_eq!(err, "Waiting for memory: this computer has 1.0 GB free, less than 2 GB (Settings → Runs)");
    // and a new limit is saved as usual
    runs::save_settings(&st, &runs::Settings { runs_memory_gb: Some(4), ..runs::get_settings(&st) }).unwrap();
    assert_eq!((runs::get_settings(&st).runs_memory_gb, gizai_lib::run_memory::limit(&st)), (Some(4), 4 * GB));
}
