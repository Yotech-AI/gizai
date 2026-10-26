//! The crate's integration tests as one test program (GA-213): every file in this folder is a module, declared below.
//! Each file in tests/ is a test program of its own that links the crate and all its dependencies again, so a new test
//! file goes here, with its `mod` line. The tests of all these files run side by side in one process: a file whose
//! tests change process-wide state (`std::env::set_var`, `set_current_dir`) or count on it stays in tests/, alone.
//! Paths in `include_str!` and `#[path]` are relative to this folder.

mod agents_test;
mod archive_test;
mod backup_test;
mod backups_test;
mod bitbucket_core_test;
mod board_test;
mod builds_on_test;
mod by_hand_test;
mod card_flow_test;
mod chat_archive_test;
mod chat_files_test;
mod chat_runs_on_test;
mod chat_test;
mod cli_tools_test;
mod clients_test;
mod clis_test;
mod db_test;
mod docs_files_test;
mod editor_test;
mod folders_test;
mod folders_windows_test;
mod ga191_core_test;
mod ga49_core_test;
mod ga49_migration_test;
mod housekeeping_test;
mod indexes_test;
mod lead_merges_core_test;
mod limits_test;
mod memory_groups_test;
mod memory_import_test;
mod memory_page_test;
mod memory_per_project_test;
mod memory_test;
mod onboarding_test;
mod pending_test;
mod project_key_test;
mod pulls_test;
mod questions_test;
mod release_030_upgrade_test;
mod repo_url_test;
mod role_template_test;
mod run_for_me_test;
mod run_support_test;
mod screenshots_test;
mod seed_agents_test;
mod sortkey_test;
mod tasks_test;
mod team_test;
mod templates_test;
mod test_runs_at_once_test;
mod usage_test;
mod workflow_test;
mod worktrees_test;

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
