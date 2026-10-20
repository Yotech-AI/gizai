//! The crate's integration tests as one test program (GA-213): every file in this folder is a module, declared below.
//! Each file in tests/ is a test program of its own that links the crate and all its dependencies again, so a new test
//! file goes here, with its `mod` line. The tests of all these files run side by side in one process: a file whose
//! tests change process-wide state (`std::env::set_var`, `set_current_dir`) or count on it stays in tests/, alone.
//! Paths in `include_str!` and `#[path]` are relative to this folder.
//! run_memory_flow_test.rs stays in tests/ too: its run pushes a command past a 1 GB slice's limit, which Linux throttles
//! for a minute or more before it ends it, and side by side with the tests here it once ran past its 120 s limit.

mod agent_runs_test;
mod app_state_test;
mod archive_flow_test;
mod ask_lead_flow_test;
mod backups_flow_test;
mod board_flow_test;
mod card_flow_test;
mod changes_flow_test;
mod chat_files_flow_test;
mod chat_flow_test;
mod chat_limits_test;
mod claude_memory_switch_test;
mod cli_run_flow_test;
mod commands_off_main_thread_test;
mod editor_flow_test;
mod folders_flow_test;
mod ga191_perf_test;
mod ga31_flow_test;
mod ga49_flow_test;
mod hand_flow_test;
mod housekeeping_flow_test;
mod lead_files_test;
mod lead_merge_test;
mod limits_flow_test;
mod login_path_other_systems_test;
mod mcp_pipe_test;
mod mcp_run_flow_test;
mod mcp_settings_test;
mod mcp_sign_in_test;
mod mcp_socket_test;
mod memory_flow_test;
mod memory_groups_flow_test;
mod memory_per_project_flow_test;
mod notifications_test;
mod nudge_flow_test;
mod onboarding_flow_test;
mod own_push_test;
mod pending_actions_test;
mod pr_check_rounds_test;
mod pull_flow_test;
mod push_after_run_test;
mod repo_check_test;
mod run_commits_test;
mod run_flow_test;
mod run_leftovers_flow_test;
mod run_memory_settings_test;
mod screenshots_settings_test;
mod slash_commands_flow_test;
mod team_lead_mcp_rules_test;
mod terminal_flow_test;
mod test_runs_at_once_flow_test;
mod tools_test;
mod update_flow_test;
mod version_test;
mod worktree_flow_test;

/// Cargo builds only the files declared above, so a file in tests/it/ without its `mod` line would never run.
#[test]
fn every_file_in_tests_it_is_declared() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("it");
    let main = std::fs::read_to_string(dir.join("main.rs")).unwrap();
    let declared: Vec<&str> = main.lines().filter_map(|l| l.strip_prefix("mod ")?.strip_suffix(';')).collect();
    let mut missing: Vec<String> = std::fs::read_dir(&dir).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|f| f.strip_suffix(".rs").is_some_and(|m| m != "main" && !declared.contains(&m)))
        .collect();
    missing.sort();
    assert!(missing.is_empty(), "tests/it/main.rs doesn't declare {missing:?}: add `mod <name>;` for each, or its tests never run");
}
