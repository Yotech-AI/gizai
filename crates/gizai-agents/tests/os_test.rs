//! GA-51: starting a coding CLI on every system (`gizai_agents::os`): finding it (an execute bit on Linux and macOS,
//! PATHEXT on Windows), npm's `.cmd` shims, arguments that reach the CLI unchanged, and ending it with everything it
//! started (its process group on Linux and macOS, its Job Object on Windows). The fake CLI is a node script
//! (tests/fake-claude-node.cjs), installed the way npm installs one on each system, so these tests run on all three.
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use gizai_agents::claude::ClaudeArgs;
use gizai_agents::os::{self, End};
use gizai_agents::process::{Caps, RunHandle, spawn};
use gizai_agents::stream::RunEvent;

#[cfg_attr(unix, allow(dead_code))]
const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-node.cjs");

/// npm's own `.cmd` shims (its cmd-shim, npm 11), for a package's node script (Gemini) and for a program of its own
/// (Claude Code 2.1's claude.exe).
const GEMINI_CMD: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@google\\gemini-cli\\bundle\\gemini.js\" %*\r\n";
const CLAUDE_CMD: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\r\n";

/// Arguments a shell or cmd.exe would change: spaces, quotes, backslashes, line breaks, `%VAR%`, cmd's `& | < > ^`.
fn tricky() -> Vec<String> {
    ["two words", r#"a "quoted" word"#, r"ends in a backslash\", r"\\server\share\", "100%", "%PATH%", "a&b|c<d>e^f",
     "line one\nline two", "", "ünïcødé ✓", "--flag=a b"].map(String::from).to_vec()
}

fn node() -> PathBuf {
    os::find_in("node", &std::env::var_os("PATH").unwrap_or_default()).expect("node on PATH")
}

/// The fake CLI installed in `dir` as npm would: on Windows `claude.cmd`, npm's shim for a script in node_modules. On
/// Linux and macOS the committed program tests/fake-claude-node.sh, which runs the script with node: a script written
/// here and started at once can fail with "Text file busy" when another test forks meanwhile.
fn install_fake(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    #[cfg(windows)]
    {
        let script = dir.join(r"node_modules\fake-claude\cli.js");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, format!("require({});\n", serde_json::to_string(FAKE).unwrap())).unwrap();
        let shim = dir.join("claude.cmd");
        std::fs::write(&shim, GEMINI_CMD.replace(r"@google\gemini-cli\bundle\gemini.js", r"fake-claude\cli.js")).unwrap();
        shim
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-node.sh"));
        // committed with 755; set again in case a checkout lost it (no write, so it can't make the file busy)
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }
}

fn args_seen(out: &Path) -> Vec<String> {
    serde_json::from_str(&std::fs::read_to_string(out).expect("the fake wrote its arguments")).unwrap()
}

/// Waits (at most 20 s) until `file` exists.
fn wait_for(file: &Path) {
    let until = Instant::now() + Duration::from_secs(20);
    while !file.is_file() {
        assert!(Instant::now() < until, "{} never appeared", file.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether whatever writes the time into `beat` every 100 ms has stopped: the file stays the same for a second.
fn beat_stopped(beat: &Path) -> bool {
    std::thread::sleep(Duration::from_millis(300));
    let before = std::fs::read_to_string(beat).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(1000));
    before == std::fs::read_to_string(beat).unwrap_or_default()
}

async fn drain(h: &mut RunHandle) -> Vec<RunEvent> {
    let mut evs = vec![];
    while let Some(e) = h.events.recv().await {
        evs.push(e);
    }
    evs
}

fn caps() -> Caps {
    Caps { max_time: Duration::from_secs(120), max_tool_calls: 80 }
}

fn exit_of(evs: &[RunEvent]) -> String {
    match evs.last() {
        Some(RunEvent::Other { raw_type }) if raw_type.starts_with("exit:") => raw_type.clone(),
        other => panic!("the run should end with its exit, not {other:?} ({evs:?})"),
    }
}

#[test]
fn npm_shims_name_the_script_or_the_program_they_run() {
    assert_eq!(os::npm_shim_script(GEMINI_CMD), Some(r"node_modules\@google\gemini-cli\bundle\gemini.js"));
    assert_eq!(os::npm_shim_script(CLAUDE_CMD), Some(r"node_modules\@anthropic-ai\claude-code\bin\claude.exe"));
    // not npm's: a batch file of your own, a shim for a shell script, nothing
    assert_eq!(os::npm_shim_script("@echo off\r\nnode \"%~dp0\\cli.js\" %*\r\n"), None);
    assert_eq!(os::npm_shim_script("@ECHO off\r\n\"%_prog%\"  \"%dp0%\\node_modules\\tool\\bin\\run.sh\" %*\r\n"), None);
    assert_eq!(os::npm_shim_script(""), None);
}

#[cfg(unix)]
#[test]
fn a_program_is_found_by_its_execute_bit_on_the_path_or_by_its_full_path() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    for (dir, name, mode) in [(&a, "notes", 0o644), (&b, "notes", 0o755), (&b, "claude", 0o755)] {
        std::fs::write(dir.join(name), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(mode)).unwrap();
    }
    let path = std::env::join_paths([&a, &b]).unwrap();
    assert_eq!(os::find_in("claude", &path), Some(b.join("claude")));
    assert_eq!(os::find_in("notes", &path), Some(b.join("notes")), "a file without an execute bit isn't a program");
    assert_eq!(os::find_in(&b.join("claude").display().to_string(), "".as_ref()), Some(b.join("claude")), "a full path");
    assert_eq!(os::find_in(&a.join("notes").display().to_string(), &path), None);
    assert_eq!(os::find_in("gemini", &path), None);
    assert_eq!(os::find_in("", &path), None);
}

#[cfg(windows)]
#[test]
fn a_program_is_found_with_pathext_so_claude_is_claude_exe_and_gemini_is_gemini_cmd() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    for f in [a.join("claude.exe"), a.join("readme.txt"), b.join("gemini.cmd"), b.join("claude.cmd"), b.join("codex.exe")] {
        std::fs::write(f, "").unwrap();
    }
    let path = std::env::join_paths([&a, &b]).unwrap();
    assert_eq!(os::find_in("claude", &path), Some(a.join("claude.exe")), "the first folder on PATH wins");
    assert_eq!(os::find_in("gemini", &path), Some(b.join("gemini.cmd")));
    assert_eq!(os::find_in("codex", &path), Some(b.join("codex.exe")));
    assert_eq!(os::find_in("claude.cmd", &path), Some(b.join("claude.cmd")), "a name with its extension");
    assert_eq!(os::find_in("readme", &path), None, ".txt isn't in PATHEXT");
    assert_eq!(os::find_in("readme.txt", &path), None);
    // a full path, with or without its extension (Settings → Coding CLIs)
    assert_eq!(os::find_in(&b.join("gemini").display().to_string(), "".as_ref()), Some(b.join("gemini.cmd")));
    assert_eq!(os::find_in(&b.join("gemini.cmd").display().to_string(), "".as_ref()), Some(b.join("gemini.cmd")));
    assert!(os::executable(&a.join("claude.exe")) && !os::executable(&a.join("readme.txt")));
}

#[test]
fn arguments_with_spaces_quotes_and_line_breaks_reach_the_cli_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let out = tmp.path().join("args.json");
    let done = os::command(&bin).args(tricky()).env("FAKE_ARGS_OUT", &out).stdin(Stdio::null()).output().unwrap();
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    assert_eq!(args_seen(&out), tricky());
    assert!(String::from_utf8_lossy(&done.stdout).contains(r#""subtype":"init""#));
}

/// Windows: an npm-installed Claude Code 2.1, whose shim runs its own claude.exe (here a copy of node.exe, which runs
/// the fake as its first argument).
#[cfg(windows)]
#[test]
fn an_npm_shim_for_a_program_runs_that_program_with_the_arguments_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("npm");
    let exe = dir.join(r"node_modules\@anthropic-ai\claude-code\bin\claude.exe");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::copy(node(), &exe).unwrap();
    std::fs::write(dir.join("claude.cmd"), CLAUDE_CMD).unwrap();
    let out = tmp.path().join("args.json");
    let done = os::command(dir.join("claude.cmd")).arg(FAKE).args(tricky()).env("FAKE_ARGS_OUT", &out).stdin(Stdio::null())
        .output().unwrap();
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    assert_eq!(args_seen(&out), tricky());
}

/// Windows: a batch file that isn't npm's goes through Rust's own batch-file quoting (CVE-2024-24576), which refuses an
/// argument it can't pass safely rather than let cmd.exe read it.
#[cfg(windows)]
#[test]
fn another_batch_file_runs_but_an_argument_with_a_line_break_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let bat = tmp.path().join("tool.cmd");
    std::fs::write(&bat, "@echo off\r\nexit /b 0\r\n").unwrap();
    let ok = os::command(&bat).args(["two words", "a&b"]).stdin(Stdio::null()).output().unwrap();
    assert!(ok.status.success());
    let refused = os::command(&bat).arg("line one\nline two").stdin(Stdio::null()).output();
    assert!(refused.is_err(), "{refused:?}");
}

#[tokio::test]
async fn a_run_streams_its_events_and_ends_with_its_exit_on_every_system() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let log = tmp.path().join("r.jsonl");
    let args = ClaudeArgs { bin, prompt: "p".into(), session_id: "S".into(), permission_mode: "acceptEdits".into(),
                            append_system_prompt: Some("Line one.\nLine \"two\", with 100% of %PATH%.".into()), ..Default::default() };
    let mut h = spawn(&args, tmp.path(), &log, caps()).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { .. })), "{evs:?}");
    assert_eq!(exit_of(&evs), "exit:0");
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 6);
    assert!(std::fs::read_to_string(tmp.path().join("r.stderr.log")).unwrap().contains("fake claude done"));
}

#[tokio::test]
async fn stop_ends_the_cli_and_the_server_it_left_running() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let beat = tmp.path().join("beat");
    let args = ClaudeArgs { bin, prompt: "hang".into(), session_id: "S".into(), permission_mode: "acceptEdits".into(),
                            env: vec![("FAKE_BEAT".into(), beat.display().to_string())], ..Default::default() };
    let mut h = spawn(&args, tmp.path(), &tmp.path().join("r.jsonl"), caps()).unwrap();
    let _init = h.events.recv().await;
    wait_for(&beat);
    let t0 = Instant::now();
    h.stop.stop();
    let evs = drain(&mut h).await;
    exit_of(&evs);
    assert!(t0.elapsed() < Duration::from_secs(9), "took {:?}", t0.elapsed());
    assert!(beat_stopped(&beat), "the server the run started still runs");
}

/// Windows: a child started detached (as `npm run dev` in the background, or a daemon) is still in the run's job, and
/// ends with it. (On Linux and macOS a detached child leaves the process group with setsid, so this is Windows only.)
#[cfg(windows)]
#[tokio::test]
async fn stop_ends_a_detached_child_too() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let beat = tmp.path().join("beat");
    let args = ClaudeArgs { bin, prompt: "hang detached".into(), session_id: "S".into(), permission_mode: "acceptEdits".into(),
                            env: vec![("FAKE_BEAT".into(), beat.display().to_string())], ..Default::default() };
    let mut h = spawn(&args, tmp.path(), &tmp.path().join("r.jsonl"), caps()).unwrap();
    let _init = h.events.recv().await;
    wait_for(&beat);
    h.stop.stop();
    let evs = drain(&mut h).await;
    exit_of(&evs);
    assert!(beat_stopped(&beat), "the detached child still runs");
}

#[tokio::test]
async fn kill_when_gizai_quits_ends_the_cli_and_the_server_it_left_running() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = install_fake(&tmp.path().join("bin"));
    let beat = tmp.path().join("beat");
    let args = ClaudeArgs { bin, prompt: "hang".into(), session_id: "S".into(), permission_mode: "acceptEdits".into(),
                            env: vec![("FAKE_BEAT".into(), beat.display().to_string())], ..Default::default() };
    let mut h = spawn(&args, tmp.path(), &tmp.path().join("r.jsonl"), caps()).unwrap();
    let _init = h.events.recv().await;
    wait_for(&beat);
    let t0 = Instant::now();
    h.stop.kill();
    let evs = drain(&mut h).await;
    exit_of(&evs);
    assert!(t0.elapsed() < Duration::from_secs(5), "kill is at once; took {:?}", t0.elapsed());
    assert!(beat_stopped(&beat));
}

#[test]
fn a_tree_ends_with_everything_in_it_and_then_says_it_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let beat = tmp.path().join("beat");
    let script = "require('child_process').spawn(process.execPath, ['-e', \
        'setInterval(() => require(\"fs\").writeFileSync(process.env.FAKE_BEAT, String(Date.now())), 100)'], { stdio: 'ignore' }); \
        setInterval(() => {}, 1000)";
    let mut cmd = os::command(node());
    cmd.args(["-e", script]).env("FAKE_BEAT", &beat).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // at low priority, as an update builds (nice 10, below normal on Windows)
    let (mut child, tree) = os::spawn_tree(&mut cmd, true).unwrap();
    wait_for(&beat);
    assert!(tree.alive());
    tree.end(End::Kill);
    child.wait().unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while tree.alive() {
        assert!(Instant::now() < until, "something in the tree still runs");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(beat_stopped(&beat), "the child in the tree still runs");
}
