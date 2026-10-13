// Linux and macOS only: the fake Claude Code is a shell script, which Windows can't start.
#![cfg(unix)]
// A run starts Claude Code from a saved path that begins with ~, as Settings → Coding CLIs takes it (GA-149, checked
// for 0.11.0 in GA-160): cli_home_paths_test shows the path, this one runs it. HOME is the process's: this binary has
// one test.
use std::os::unix::fs::PermissionsExt;

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git_repo(dir: &std::path::Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

#[tokio::test]
async fn a_claude_code_path_saved_with_a_tilde_runs_the_program_in_your_home_folder() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let bin = home.join(".local").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    // ~/.local/bin/claude says it ran, then is the fake Claude Code
    let claude = bin.join("claude");
    std::fs::write(&claude, format!("#!/usr/bin/env bash\necho \"ran: ~/.local/bin/claude\" >&2\nexec \"{FAKE_CLAUDE}\" \"$@\"\n")).unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    // SAFETY: the only test in this binary, so no other thread reads the environment meanwhile.
    unsafe { std::env::set_var("HOME", &home) };
    let st = gizai_lib::test_state(&dir.path().join("gizai"));
    let repo = git_repo(dir.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");

    // saved as typed in Settings, with spaces around it; no bin override, so the run takes the saved path
    gizai_core::settings::set(&st.db, "claude_bin", &" ~/.local/bin/claude ".to_string()).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None));
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let err = std::fs::read_to_string(std::path::Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    assert!(err.contains("ran: ~/.local/bin/claude"), "the run started the program in the home folder: {err}");

    // a ~ path that isn't there is refused before a run starts, never replaced by another Claude Code
    gizai_core::settings::set(&st.db, "claude_bin", &"~/.local/bin/no-claude".to_string()).unwrap();
    let other = gizai_lib::test_task(&st, repo.to_str().unwrap(), "frontend");
    let e = gizai_lib::runs::run_once(&st, &other, None, None).await.unwrap_err();
    assert!(e.contains("Claude Code not found"), "{e}");
    assert!(gizai_core::runs::list_for_task(&st.db, &other).unwrap().is_empty(), "no run row created");
}
