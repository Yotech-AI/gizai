//! GA-179: on macOS and Windows `runs::command_path` starts no login shell and keeps nothing: macOS took your login
//! shell's PATH at start (`shell_path`), and a Windows app gets the full PATH. Linux's kept PATH is
//! `login_path_kept_test.rs`. The count of login shells belongs to the whole process, so this file has a single test.
#![cfg(any(windows, target_os = "macos"))]
use gizai_lib::runs;

#[test]
fn command_path_is_gizais_own_path_and_starts_no_login_shell() {
    let own = std::env::var_os("PATH").unwrap_or_default();
    for i in 1..=10 {
        assert_eq!(runs::command_path(), own, "call {i}");
    }
    assert_eq!(runs::fresh_command_path(), own, "a fresh read is Gizai's own PATH too");
    assert_eq!(runs::login_shells(), 0, "no login shell");
}
