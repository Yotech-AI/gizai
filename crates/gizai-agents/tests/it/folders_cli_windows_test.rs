//! GA-51, Windows: Claude Code's deny rules for an agent's read folders in the POSIX form Claude Code matches paths in
//! on Windows (folders_cli_test.rs has Linux's).
#![cfg(windows)]
use gizai_agents::cli::{self, RunFolder};

#[test]
fn read_folders_become_deny_rules_with_the_drive_as_a_lower_case_first_folder() {
    let folders = [
        RunFolder { path: r"C:\Users\me\shared".into(), change: false },
        RunFolder { path: r"D:\data\docs\".into(), change: false },
        RunFolder { path: r"C:\Users\me\My work".into(), change: false },
        RunFolder { path: r"C:\Users\me\out".into(), change: true },
    ];
    assert_eq!(cli::claude_read_only(&folders), [
        "Edit(//c/Users/me/shared/**)", "Write(//c/Users/me/shared/**)",
        "Edit(//d/data/docs/**)", "Write(//d/data/docs/**)",
        "Edit(//c/Users/me/My work/**)", "Write(//c/Users/me/My work/**)",
    ]);
}
