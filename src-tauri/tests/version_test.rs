// A release bumps the version in Cargo.toml (workspace), package.json and
// src-tauri/tauri.conf.json, and in both lock files (docs/RELEASING.md).
// This test fails when one of them is left behind.
use std::path::{Path, PathBuf};

fn root() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf() }

fn json(file: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(root().join(file)).unwrap()).unwrap()
}

fn cargo_lock_version(lock: &str, name: &str) -> Option<String> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line == format!("name = \"{name}\"") {
            return lines.next()?.strip_prefix("version = \"")?.strip_suffix('"').map(String::from);
        }
    }
    None
}

#[test]
fn every_version_file_says_the_workspace_version() {
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(json("src-tauri/tauri.conf.json")["version"], version, "src-tauri/tauri.conf.json");
    assert_eq!(json("package.json")["version"], version, "package.json");
    let npm_lock = json("package-lock.json");
    assert_eq!(npm_lock["version"], version, "package-lock.json");
    assert_eq!(npm_lock["packages"][""]["version"], version, "package-lock.json, the root package");
    let cargo_lock = std::fs::read_to_string(root().join("Cargo.lock")).unwrap();
    for name in ["gizai", "gizai-agents", "gizai-core", "gizai-mcp"] {
        assert_eq!(cargo_lock_version(&cargo_lock, name).as_deref(), Some(version), "Cargo.lock, {name}");
    }
}

#[test]
fn the_binary_reports_the_workspace_version() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_gizai")).arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("gizai {}", env!("CARGO_PKG_VERSION")));
}
