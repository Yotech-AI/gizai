//! The crate's integration tests as one test program (GA-213): every file in this folder is a module, declared below.
//! Each file in tests/ is a test program of its own that links the crate and all its dependencies again, so a new test
//! file goes here, with its `mod` line. The tests of all these files run side by side in one process: a file whose
//! tests change process-wide state (`std::env::set_var`, `set_current_dir`) or count on it stays in tests/, alone.
//! Paths in `include_str!` and `#[path]` are relative to this folder.

mod protocol_test;
mod shim_test;

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
