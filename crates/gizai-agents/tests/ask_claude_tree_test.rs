//! GA-78: Ask Claude Code again (`tool_catalog::ask_claude`) on every system. It starts Claude Code as an `os::Tree` (a
//! process group on Linux and macOS, a Job Object on Windows) and ends it with everything it started: SIGTERM, then
//! SIGKILL after 3 seconds for one that ignores SIGTERM (on Windows ending the job ends it at once). The fake Claude Code
//! is a node script (tests/fake-claude-tools-node.cjs), installed the way npm installs one on each system, so these tests
//! run on all three; never the real one.
//! GA-80: what Claude Code gets from Gizai's environment (on Windows Windows' own variables, with its home and temp folders
//! in the scratch folder; never a key), and the last line it wrote to stderr in the error when it gives no list.
use std::collections::BTreeMap;
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

/// The error when Claude Code gives no list.
const NO_LIST: &str = "Claude Code gave no list of its tools (is it installed and up to date?)";

/// Windows' own variables `ask_claude` passes on from Gizai's environment on Windows, when set.
const WINDOWS_VARS: [&str; 12] = ["SystemRoot", "SystemDrive", "windir", "ComSpec", "PATHEXT", "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData", "CLAUDE_CODE_GIT_BASH_PATH"];

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

/// A variable's name as the system compares it: Windows doesn't tell `Path` from `PATH`.
fn name(k: &str) -> String {
    if cfg!(windows) { k.to_ascii_uppercase() } else { k.to_string() }
}

/// The variables the fake got (mode `env` writes them into $HOME/env.json), by `name`.
fn env_seen(scratch: &Path) -> BTreeMap<String, String> {
    let text = std::fs::read_to_string(scratch.join("home").join("env.json")).expect("the fake wrote what it got");
    let got: BTreeMap<String, String> = serde_json::from_str(&text).unwrap();
    got.into_iter().map(|(k, v)| (name(&k), v)).collect()
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

#[tokio::test]
async fn claude_code_gets_windows_own_variables_and_its_home_and_temp_in_the_scratch_folder_on_windows_and_never_a_key() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let scratch = scratch(tmp.path(), "env");
    // SAFETY: only ask_claude reads these, in this file. The key must never reach Claude Code; Git Bash's path only on
    // Windows.
    unsafe {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-GA80-NOT-FOR-THE-FAKE");
        std::env::set_var("CLAUDE_CODE_GIT_BASH_PATH", r"C:\GA80\Git\bin\bash.exe");
    }
    let tools = tool_catalog::ask_claude(&bin, &scratch, &path()).await.unwrap();
    assert_eq!(tools, TOOLS, "Claude Code started and gave its list");
    let got = env_seen(&scratch);
    let get = |k: &str| got.get(&name(k)).cloned();
    assert_eq!(get("HOME").map(PathBuf::from), Some(scratch.join("home")));
    assert_eq!(get("CLAUDE_CONFIG_DIR").map(PathBuf::from), Some(scratch.join("config")), "never ~/.claude or the agents' account");
    assert_eq!(get("TMPDIR").map(PathBuf::from), Some(scratch.join("tmp")));
    assert_eq!(get("LANG").as_deref(), Some("C.UTF-8"));
    assert_eq!(get("PATH").map(OsString::from), Some(path()));
    for k in ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_AUTH_TOKEN"] {
        assert_eq!(get(k), None, "{k} reached Claude Code: it could log in and spend");
    }
    #[cfg(windows)]
    {
        assert!(get("SystemRoot").is_some_and(|v| !v.is_empty()), "a Windows program doesn't start without SystemRoot");
        for k in WINDOWS_VARS {
            assert_eq!(get(k), std::env::var(k).ok(), "{k}: Gizai's own, when set");
        }
        assert_eq!(get("USERPROFILE").map(PathBuf::from), Some(scratch.join("home")), "the home folder on Windows: never your own .claude");
        assert_eq!(get("TEMP").map(PathBuf::from), Some(scratch.join("tmp")));
        assert_eq!(get("TMP").map(PathBuf::from), Some(scratch.join("tmp")));
    }
    #[cfg(unix)]
    for k in WINDOWS_VARS.into_iter().chain(["USERPROFILE", "TEMP", "TMP"]) {
        assert_eq!(get(k), None, "{k}: Linux and macOS get no more than before");
    }
    // nothing else of this test's environment reaches it (on Linux and macOS the shell that starts node,
    // fake-claude-tools-node.sh, sets PWD, SHLVL and _ itself, and macOS __CF_USER_TEXT_ENCODING)
    let mut passed: Vec<String> = ["PATH", "HOME", "CLAUDE_CONFIG_DIR", "TMPDIR", "LANG"].map(name).to_vec();
    if cfg!(windows) {
        passed.extend(WINDOWS_VARS.into_iter().chain(["USERPROFILE", "TEMP", "TMP"]).map(name));
    }
    let own = ["PWD", "OLDPWD", "SHLVL", "_", "__CF_USER_TEXT_ENCODING"];
    let leaked: Vec<String> = std::env::vars_os().map(|(k, _)| name(&k.to_string_lossy()))
        .filter(|k| !k.starts_with('=') && !passed.contains(k) && !own.contains(&k.as_str()) && got.contains_key(k))
        .collect();
    assert!(leaked.is_empty(), "these reached Claude Code from Gizai's environment: {leaked:?}");
}

#[tokio::test]
async fn without_a_list_the_error_ends_with_the_last_line_claude_code_wrote_to_stderr_cut_to_300_characters() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    // its last non-empty line, written after another one with Windows line ends and followed by blank lines
    let e = tool_catalog::ask_claude(&bin, &scratch(&tmp.path().join("a"), "stderr"), &path()).await.unwrap_err();
    assert_eq!(e, format!("{NO_LIST}: Error: GA80 the last thing Claude Code said"));
    // a long last line: its first 300 characters (not bytes: é takes two)
    let e = tool_catalog::ask_claude(&bin, &scratch(&tmp.path().join("b"), "stderr-long"), &path()).await.unwrap_err();
    let said = e.strip_prefix(&format!("{NO_LIST}: ")).unwrap_or_else(|| panic!("{e}"));
    assert_eq!(said.chars().count(), 300, "{said}");
    assert_eq!(said, format!("Error: {}", "é".repeat(293)));
}

#[tokio::test]
async fn without_a_list_and_with_nothing_on_stderr_the_error_is_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let e = tool_catalog::ask_claude(&bin, &scratch(&tmp.path().join("a"), "quiet"), &path()).await.unwrap_err();
    assert_eq!(e, NO_LIST);
    // only blank lines count as nothing
    let e = tool_catalog::ask_claude(&bin, &scratch(&tmp.path().join("b"), "blank"), &path()).await.unwrap_err();
    assert_eq!(e, NO_LIST);
}
