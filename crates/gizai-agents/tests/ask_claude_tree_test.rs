//! GA-78: Ask Claude Code again (`tool_catalog::ask_claude`) on every system. It starts Claude Code as an `os::Tree` (a
//! process group on Linux and macOS, a Job Object on Windows) and ends it with everything it started: SIGTERM, then
//! SIGKILL after 3 seconds for one that ignores SIGTERM (on Windows ending the job ends it at once). The fake Claude Code
//! is a node script (tests/fake-claude-tools-node.cjs), installed the way npm installs one on each system, so these tests
//! run on all three; never the real one.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_agents::tool_catalog;

#[cfg_attr(unix, allow(dead_code))]
const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-tools-node.cjs");

/// npm's own `.cmd` shim (its cmd-shim, npm 11) for a package's node script, as in os_test.rs.
#[cfg(windows)]
const NPM_CMD: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\fake-claude\\cli.js\" %*\r\n";

/// What the fake reports: its built-in tools, without its MCP tool.
const TOOLS: [&str; 5] = ["Task", "Bash", "Read", "WebSearch", "WebFetch"];

/// The fake installed in `dir` as npm would: on Windows `claude.cmd`, npm's shim for a script in node_modules. On Linux
/// and macOS the committed program tests/fake-claude-tools-node.sh, which runs the script with node.
fn install_fake(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    #[cfg(windows)]
    {
        let script = dir.join(r"node_modules\fake-claude\cli.js");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, format!("require({});\n", serde_json::to_string(FAKE).unwrap())).unwrap();
        let shim = dir.join("claude.cmd");
        std::fs::write(&shim, NPM_CMD).unwrap();
        shim
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-tools-node.sh"));
        // committed with 755; set again in case a checkout lost it (no write, so it can't make the file busy)
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }
}

/// The scratch folder `ask_claude` gets, with the fake's mode in its home.
fn scratch(tmp: &Path, mode: &str) -> PathBuf {
    let scratch = tmp.join("scratch");
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    std::fs::write(scratch.join("home").join("mode"), mode).unwrap();
    scratch
}

/// The PATH the fake gets: this test's own, where node is.
fn path() -> OsString {
    std::env::var_os("PATH").unwrap_or_default()
}

/// Whether whatever writes the time into `beat` every 100 ms has stopped: the file stays the same for a second.
async fn beat_stopped(beat: &Path) -> bool {
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = std::fs::read_to_string(beat).unwrap_or_default();
    tokio::time::sleep(Duration::from_millis(1000)).await;
    before == std::fs::read_to_string(beat).unwrap_or_default()
}

#[tokio::test]
async fn ask_claude_reads_the_tools_of_a_claude_code_without_a_login_on_every_system() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let scratch = scratch(tmp.path(), "");
    let tools = tool_catalog::ask_claude(&bin, &scratch, &path()).await.unwrap();
    assert_eq!(tools, TOOLS, "its built-in tools, MCP tools left out");
}

#[tokio::test]
async fn ask_claude_ends_a_claude_code_left_running_with_what_it_started_on_every_system() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let scratch = scratch(tmp.path(), "hang");
    let t0 = Instant::now();
    let tools = tool_catalog::ask_claude(&bin, &scratch, &path()).await.unwrap();
    assert_eq!(tools, TOOLS);
    assert!(t0.elapsed() < Duration::from_secs(10), "it doesn't wait for Claude Code to exit by itself: {:?}", t0.elapsed());
    let beat = scratch.join("home").join("beat");
    assert!(beat.is_file(), "the fake's child ran before its init line");
    assert!(beat_stopped(&beat).await, "what Claude Code started still runs");
}

#[tokio::test]
async fn a_claude_code_that_ignores_sigterm_is_killed_with_what_it_started_after_three_seconds_and_on_windows_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let scratch = scratch(tmp.path(), "stubborn");
    let t0 = Instant::now();
    let tools = tool_catalog::ask_claude(&bin, &scratch, &path()).await.unwrap();
    let took = t0.elapsed();
    assert_eq!(tools, TOOLS);
    // Linux and macOS: SIGTERM first, and SIGKILL only when it is still there 3 seconds later
    #[cfg(unix)]
    assert!(took >= Duration::from_secs(3) && took < Duration::from_secs(15), "SIGTERM, then SIGKILL after 3 s: {took:?}");
    // Windows has no gentle step: the job ends at once (`os::End`)
    #[cfg(windows)]
    assert!(took < Duration::from_secs(10), "{took:?}");
    let beat = scratch.join("home").join("beat");
    assert!(beat.is_file(), "the fake's child ran before its init line");
    assert!(beat_stopped(&beat).await, "what Claude Code started still runs");
}
