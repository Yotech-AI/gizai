// GA-55: the hidden browser agents test web pages with, Chrome DevTools MCP (crates/gizai-agents/src/browser.rs). Its
// command is always the pinned server, hidden (--headless) with a throwaway profile (--isolated), and never has an option
// that connects to a running browser or uses a real profile; Brave's program is refused; what it needs (Node, npx, Chrome
// or Chromium) is looked up on a fake PATH; and Stop and the time cap end the browser the server started in a process group
// of its own. The run's claude is fake-claude-browser.py and the server fake-npx-browser.sh: never the real npx, Chrome
// DevTools MCP or a browser.
// The command, the options and the versions are checked on every system. The browser program, what it needs and Stop and
// the time cap only on Linux and macOS: they run shell or Python scripts as fake programs, which Windows can't start.
// GA-79, Windows only: Node and npx found as node.exe and npx.cmd, Google Chrome in its three Windows places, the program
// you set with a drive letter or `~\`, and the run's entry started through `cmd /c` with paths that have spaces (a node
// fake behind an npx.cmd, like os_test's).
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::{Duration, Instant};

use gizai_agents::browser;
#[cfg(unix)]
use gizai_agents::claude::ClaudeArgs;
use gizai_agents::mcp_run;
#[cfg(unix)]
use gizai_agents::mcp_run::RunServer;
#[cfg(unix)]
use gizai_agents::process::{Caps, RunHandle, spawn};
#[cfg(unix)]
use gizai_agents::stream::RunEvent;

#[cfg(unix)]
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-browser.py");
#[cfg(unix)]
const FAKE_NPX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-npx-browser.sh");

/// A committed fake, made runnable (its mode in git is 755; a checkout without it still runs).
#[cfg(unix)]
fn executable(p: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    PathBuf::from(p)
}

/// A small script at `path` (made runnable).
#[cfg(unix)]
fn script(path: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

// ---- the command ----

#[test]
fn the_command_is_the_pinned_server_always_headless_and_isolated_with_nothing_sent_to_google() {
    let a = browser::args("1.10.1", None, false);
    assert_eq!(a, s(&["-y", "chrome-devtools-mcp@1.10.1", "--headless", "--isolated", "--no-usage-statistics", "--no-performance-crux"]));
    browser::safe(&a).unwrap();
    let a = browser::args("1.10.1", Some("/usr/bin/chromium"), true);
    assert_eq!(&a[6..], s(&["--executablePath=/usr/bin/chromium", "--acceptInsecureCerts"]));
    browser::safe(&a).unwrap();
    assert!(!browser::args("1.10.1", Some("/usr/bin/chromium"), false).contains(&"--acceptInsecureCerts".to_string()), "off by default");
    // the program is one argument: a "program" that looks like an option can never become one
    let a = browser::args("1.10.1", Some("--browserUrl=http://127.0.0.1:9222"), false);
    assert_eq!(a.last().map(String::as_str), Some("--executablePath=--browserUrl=http://127.0.0.1:9222"));
    assert!(!a.iter().any(|x| x.starts_with("--browserUrl")));
    assert_eq!(browser::args("1.10.1", Some(""), false).len(), 6, "an empty program adds nothing");
    let env = browser::env(OsStr::new("/usr/bin:/bin"));
    for want in [("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS", "1"), ("CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS", "1"), ("PATH", "/usr/bin:/bin")] {
        assert!(env.contains(&(want.0.to_string(), want.1.to_string())), "{want:?} missing: {env:?}");
    }
}

#[test]
fn safe_refuses_every_option_that_connects_to_a_running_browser_or_uses_a_real_profile() {
    let base = browser::args("1.10.1", None, false);
    for bad in ["--browserUrl=http://127.0.0.1:9222", "--browserUrl", "--browser-url=http://x", "-u", "--wsEndpoint=ws://127.0.0.1:9222/devtools/browser/x",
                "--ws-endpoint", "-w", "--wsHeaders={}", "--autoConnect", "--auto-connect", "--userDataDir=/home/jefsev/.config/BraveSoftware",
                "--user-data-dir=/home/jefsev/.config/chromium", "--channel=stable", "--config=x.json", "--chromeArg=--user-data-dir=/x",
                "--chrome-arg=--remote-debugging-port=9222", "--headless=false", "--no-headless", "--no-isolated", "--isolated=false"] {
        let mut a = base.clone();
        a.push(bad.into());
        let e = browser::safe(&a).expect_err(bad);
        assert!(!e.is_empty(), "{bad}");
    }
    for must in ["--headless", "--isolated"] {
        let a: Vec<String> = base.iter().filter(|x| *x != must).cloned().collect();
        assert!(browser::safe(&a).unwrap_err().contains(must), "{must}");
    }
    for card in ["--browserUrl", "--wsEndpoint", "--autoConnect", "--userDataDir"] {
        assert!(browser::FORBIDDEN.contains(&card), "{card}");
    }
    for v in ["latest", "^1.10.1", "1.10", "1.x"] {
        let e = browser::safe(&browser::args(v, None, false)).unwrap_err();
        assert!(e.contains("never latest"), "{v}: {e}");
    }
}

#[test]
fn only_an_exact_version_is_taken() {
    for ok in ["1.10.1", "0.0.1", "12.3.456", "1.11.0-beta.2"] {
        assert!(browser::exact_version(ok), "{ok}");
    }
    for bad in ["latest", "", "1.10", "^1.10.1", "~1.10.1", "1.10.x", ">=1.0.0", "1.10.1 || 2.0.0", "1.10.1-", "v1.10.1", "1.10.1 ", "next"] {
        assert!(!browser::exact_version(bad), "{bad:?}");
    }
}

// ---- the browser program ----

// Linux and macOS only: shell scripts as the browser programs, and a Unix symlink.
#[cfg(unix)]
#[test]
fn brave_is_never_the_browser_by_its_path_or_where_a_link_leads() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().display().to_string();
    let chromium = script(&tmp.path().join("bin/chromium"), "exit 0");
    let brave = script(&tmp.path().join("opt/brave.com/brave/brave"), "exit 0");
    let link = tmp.path().join("bin/my-browser");
    std::os::unix::fs::symlink(&brave, &link).unwrap();
    for b in ["/usr/bin/brave", "/usr/bin/brave-browser", "/opt/brave.com/brave/brave", "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"] {
        let e = browser::check_program(b, &home).unwrap_err();
        assert!(e.contains("Brave"), "{b}: {e}");
    }
    assert!(browser::check_program(&brave.display().to_string(), &home).unwrap_err().contains("Brave"));
    assert!(browser::check_program(&link.display().to_string(), &home).unwrap_err().contains("Brave"), "a link to Brave is Brave");
    assert!(browser::check_program("chromium", &home).unwrap_err().contains("full path"));
    assert!(browser::check_program(&tmp.path().join("bin/gone").display().to_string(), &home).unwrap_err().contains("isn't a program"));
    let plain = tmp.path().join("bin/not-runnable");
    std::fs::write(&plain, "x").unwrap();
    assert!(browser::check_program(&plain.display().to_string(), &home).is_err(), "a file that can't run");
    assert_eq!(browser::check_program(&format!(" {} ", chromium.display()), &home).unwrap(), chromium);
    assert_eq!(browser::check_program("~/bin/chromium", &home).unwrap(), chromium, "~ is the home folder");
    assert!(browser::program_for_run(&brave.display().to_string(), &home, OsStr::new("")).unwrap_err().contains("Brave"));
    assert_eq!(browser::program_for_run(&chromium.display().to_string(), &home, OsStr::new("")).unwrap(), Some(chromium.display().to_string()));
}

// ---- what it needs ----

// Linux and macOS only: shell scripts as Node, npx and the browser, and a Unix symlink.
#[cfg(unix)]
#[test]
fn what_the_browser_needs_is_found_on_path_or_named_with_what_to_install() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().display().to_string();
    let new = tmp.path().join("new");
    script(&new.join("node"), "echo v24.1.0");
    script(&new.join("npx"), "exit 0");
    let chromium = script(&new.join("chromium"), "exit 0");
    let old = tmp.path().join("old");
    script(&old.join("node"), "echo v18.20.4");
    script(&old.join("npx"), "exit 0");
    let no_npx = tmp.path().join("no-npx");
    script(&no_npx.join("node"), "echo v22.12.0");
    let brave_only = tmp.path().join("brave-only");
    let brave = script(&tmp.path().join("opt/brave.com/brave/brave-browser"), "exit 0");
    std::fs::create_dir_all(&brave_only).unwrap();
    std::os::unix::fs::symlink(&brave, brave_only.join("chromium")).unwrap();
    let path = |d: &Path| d.as_os_str().to_owned();
    // Google Chrome where the server finds it itself, or Chromium in its usual place, counts on any PATH: Linux's places
    // and macOS's, in browser.rs's order (a macOS runner has Google Chrome in /Applications)
    let chrome = ["/opt/google/chrome/chrome", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
    let system = chrome.into_iter()
        .chain(["/Applications/Chromium.app/Contents/MacOS/Chromium", "/usr/lib/chromium/chromium", "/usr/lib/chromium-browser/chromium-browser"])
        .find(|p| Path::new(p).is_file());

    let n = browser::needs(&path(&new), "", &home);
    assert!(n.missing.is_empty(), "{:?}", n.missing);
    assert_eq!(n.node_version.as_deref(), Some("v24.1.0"));
    assert_eq!(n.npx, Some(new.join("npx").display().to_string()));
    if !chrome.iter().any(|p| Path::new(p).is_file()) {
        assert_eq!((n.browser.clone(), n.browser_name.as_deref()), (Some(chromium.display().to_string()), Some("Chromium")));
    }

    let n = browser::needs(&path(&old), "", &home);
    assert!(n.missing.iter().any(|m| m.contains("Node v18.20.4 is too old") && m.contains("Node 20.19 or newer")), "{:?}", n.missing);

    let n = browser::needs(&path(&no_npx), "", &home);
    assert!(n.missing.iter().any(|m| m.starts_with("npx isn't found")), "{:?}", n.missing);

    let n = browser::needs(&path(&tmp.path().join("empty")), "", &home);
    assert!(n.missing.iter().any(|m| m.starts_with("Node isn't found: install Node 20.19 or newer")), "{:?}", n.missing);
    assert!(n.node.is_none() && n.npx.is_none());
    if system.is_none() {
        assert!(n.missing.iter().any(|m| m.contains("No Google Chrome or Chromium found")), "{:?}", n.missing);
    }

    // a "chromium" on PATH that leads to Brave never counts
    let found = browser::find_browser(&path(&brave_only));
    assert!(found.as_ref().is_none_or(|f| !f.path.to_string_lossy().to_lowercase().contains("brave") && Some(f.path.to_str().unwrap()) == system),
            "{found:?}");

    // the program set in Settings: Brave's is named as the problem; another is used as it is
    let n = browser::needs(&path(&new), "/usr/bin/brave", &home);
    assert!(n.missing.iter().any(|m| m.contains("Brave")), "{:?}", n.missing);
    let n = browser::needs(&path(&new), &chromium.display().to_string(), &home);
    assert_eq!((n.browser.clone(), n.browser_name.as_deref()), (Some(chromium.display().to_string()), Some("The program you set")));
    assert!(n.missing.is_empty(), "{:?}", n.missing);
}

#[test]
fn node_versions_the_pinned_server_takes() {
    for ok in ["v20.19.0", "v20.20.1", "v22.12.0", "v23.0.0", "v24.1.0", "26.0.0\n"] {
        assert!(browser::node_ok(ok), "{ok}");
    }
    for bad in ["v20.18.9", "v21.7.3", "v22.11.0", "v18.20.4", "", "garbage"] {
        assert!(!browser::node_ok(bad), "{bad}");
    }
}

// ---- GA-79: Linux and macOS as before, Windows' own ----

#[test]
fn the_full_path_message_gives_this_systems_example() {
    #[cfg(not(windows))]
    {
        assert_eq!(browser::EXAMPLE_PROGRAM, "/usr/bin/chromium");
        // word for word what it said before GA-79
        assert_eq!(browser::check_program(" chromium ", "").unwrap_err(), "give the browser program as a full path, like /usr/bin/chromium: not \"chromium\"");
    }
    #[cfg(windows)]
    {
        assert_eq!(browser::EXAMPLE_PROGRAM, r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        assert_eq!(browser::check_program(" chrome.exe ", "").unwrap_err(),
                   r#"give the browser program as a full path, like C:\Program Files\Google\Chrome\Application\chrome.exe: not "chrome.exe""#);
    }
}

/// The browser's entry in a run's config. On Linux and macOS it is `mcp_run::stdio`'s, npx by its path with the server's
/// arguments and environment lines, as before GA-79, also with spaces in the path. On Windows too when npx isn't a batch
/// file in a folder (a bare `npx`, an npx.exe).
#[test]
fn the_runs_entry_is_npx_by_its_path_on_linux_and_macos() {
    let args = browser::args("1.10.1", Some("/opt/Google Chrome/chrome"), true);
    let env = browser::env(OsStr::new("/usr/local/bin:/usr/bin"));
    #[cfg(not(windows))]
    for npx in ["/usr/bin/npx", "/home/me/.nvm/versions/node/v24.1.0/bin/npx", "/opt/Program Files/nodejs/npx"] {
        let e = browser::entry(npx, &args, &env);
        assert_eq!(e, mcp_run::stdio(npx, &args, &env), "{npx}");
        assert_eq!(e, serde_json::json!({"type": "stdio", "command": npx, "args": args, "env": {
            "CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS": "1", "CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS": "1", "PATH": "/usr/local/bin:/usr/bin"}}));
    }
    #[cfg(windows)]
    for npx in ["npx", r"C:\tools\npx.exe"] {
        assert_eq!(browser::entry(npx, &args, &env), mcp_run::stdio(npx, &args, &env), "{npx}");
    }
}

// ---- Windows (GA-79) ----
// Node and npx as Node's installer puts them, Google Chrome in its three places, the program you set, and the run's
// entry started through cmd. Programs are empty `.exe` files (os::executable goes by PATHEXT), never a real browser; npx
// is an npx.cmd like Node's own that runs the node fake of os_test.

#[cfg(windows)]
const FAKE_NODE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-node.cjs");

/// Node's own npx.cmd (Node 22), without its look for a global npm: node.exe next to it, else the one on PATH, runs
/// node_modules\npm\bin\npx-cli.js with the arguments as cmd got them.
#[cfg(windows)]
const NPX_CMD: &str = "@ECHO OFF\r\n\r\nSETLOCAL\r\n\r\nSET \"NODE_EXE=%~dp0\\node.exe\"\r\nIF NOT EXIST \"%NODE_EXE%\" (\r\n  SET \"NODE_EXE=node\"\r\n)\r\n\r\nSET \"NPX_CLI_JS=%~dp0\\node_modules\\npm\\bin\\npx-cli.js\"\r\n\r\n\"%NODE_EXE%\" \"%NPX_CLI_JS%\" %*\r\n";

/// The variables `find_browser` reads for Google Chrome's places, in its order.
#[cfg(windows)]
const PLACE_VARS: [&str; 3] = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"];

/// The tests that read or change Chrome's places (`PLACE_VARS`) take turns.
#[cfg(windows)]
static PLACES: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(windows)]
fn turn() -> std::sync::MutexGuard<'static, ()> {
    PLACES.lock().unwrap_or_else(|e| e.into_inner())
}

/// Chrome's three places pointed at a test's folders while it holds its turn, and put back as they were when it ends,
/// also when it fails.
#[cfg(windows)]
struct Places {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _turn: std::sync::MutexGuard<'static, ()>,
}

#[cfg(windows)]
impl Places {
    fn at(dirs: &[PathBuf; 3]) -> Places {
        let t = turn();
        let saved = PLACE_VARS.iter().map(|k| (*k, std::env::var_os(k))).collect();
        for (k, d) in PLACE_VARS.iter().zip(dirs) {
            // SAFETY: only the tests in this file read these variables, and they take turns (PLACES).
            unsafe { std::env::set_var(k, d) };
        }
        Places { saved, _turn: t }
    }
}

#[cfg(windows)]
impl Drop for Places {
    fn drop(&mut self) {
        for (k, v) in &self.saved {
            // SAFETY: as in `at`; the turn is let go only after this (fields drop after `drop`).
            unsafe {
                match v {
                    Some(v) => std::env::set_var(k, v),
                    None => std::env::remove_var(k),
                }
            }
        }
    }
}

/// An empty file at `p` as a program.
#[cfg(windows)]
fn program(p: &Path) -> PathBuf {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, "").unwrap();
    p.to_path_buf()
}

/// npx in `dir` as Node's installer puts it there: npx.cmd (`NPX_CMD`), whose npx-cli.js is the node fake, and an `npx`
/// without an extension (Node's script for Git Bash), which Windows can't run.
#[cfg(windows)]
fn install_npx(dir: &Path) -> PathBuf {
    let cli = dir.join(r"node_modules\npm\bin\npx-cli.js");
    std::fs::create_dir_all(cli.parent().unwrap()).unwrap();
    std::fs::write(&cli, format!("require({});\n", serde_json::to_string(FAKE_NODE).unwrap())).unwrap();
    std::fs::write(dir.join("npx"), "#!/usr/bin/env bash\nnode \"$(dirname \"$0\")/node_modules/npm/bin/npx-cli.js\" \"$@\"\n").unwrap();
    std::fs::write(dir.join("npx.cmd"), NPX_CMD).unwrap();
    dir.join("npx.cmd")
}

/// Starts a run's config entry the way Claude Code starts a stdio server: its command with its arguments (Rust's Command,
/// like Node's spawn, puts quotes around each argument with a space and leaves the others as they are) and its env lines
/// over the environment, in the run's folder. The node fake writes the arguments it got to `out`.
#[cfg(windows)]
fn start_entry(e: &serde_json::Value, cwd: &Path, out: &Path) -> std::process::Output {
    let mut cmd = std::process::Command::new(e["command"].as_str().unwrap());
    cmd.args(e["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()));
    for (k, v) in e["env"].as_object().unwrap() {
        cmd.env(k, v.as_str().unwrap());
    }
    cmd.env("FAKE_ARGS_OUT", out).current_dir(cwd).stdin(std::process::Stdio::null()).output().unwrap()
}

#[cfg(windows)]
fn args_seen(out: &Path) -> Vec<String> {
    serde_json::from_str(&std::fs::read_to_string(out).expect("the fake wrote its arguments")).unwrap()
}

/// Windows: on the runner's own PATH (Node 22 from actions/setup-node), Node is node.exe with its version and npx is
/// npx.cmd, and nothing is missing about either.
#[cfg(windows)]
#[test]
fn needs_finds_node_exe_and_npx_cmd_on_the_path_with_nodes_version() {
    let _t = turn();
    let n = browser::needs(&std::env::var_os("PATH").unwrap_or_default(), "", "");
    let lower = |s: &Option<String>| s.clone().unwrap_or_default().to_lowercase();
    assert!(lower(&n.node).ends_with(r"\node.exe"), "{n:?}");
    assert!(lower(&n.npx).ends_with(r"\npx.cmd"), "{n:?}");
    let v = n.node_version.clone().unwrap_or_default();
    assert!(v.starts_with('v') && browser::node_ok(&v), "Node's version: {n:?}");
    assert!(!n.missing.iter().any(|m| m.contains("Node") || m.contains("npx")), "{:?}", n.missing);
}

/// Windows: in a folder like the one Node's installer makes (C:\Program Files\nodejs), npx is npx.cmd, not the `npx`
/// without an extension next to it; node.exe (a copy of the runner's) gives its version.
#[cfg(windows)]
#[test]
fn in_nodes_own_folder_npx_is_npx_cmd_and_node_exe_gives_its_version() {
    let tmp = tempfile::tempdir().unwrap();
    let nodejs = tmp.path().join(r"Program Files\nodejs");
    install_npx(&nodejs);
    let real = gizai_agents::os::find_in("node", &std::env::var_os("PATH").unwrap_or_default()).expect("node on PATH");
    std::fs::copy(&real, nodejs.join("node.exe")).unwrap();
    let path = nodejs.as_os_str();
    assert_eq!(browser::on_path("node", path), Some(nodejs.join("node.exe")));
    assert_eq!(browser::on_path("npx", path), Some(nodejs.join("npx.cmd")));
    let _t = turn();
    let n = browser::needs(path, "", "");
    assert_eq!((n.node.clone(), n.npx.clone()), (Some(nodejs.join("node.exe").display().to_string()), Some(nodejs.join("npx.cmd").display().to_string())));
    let v = n.node_version.clone().unwrap_or_default();
    assert!(v.starts_with('v') && browser::node_ok(&v), "Node's version: {n:?}");
    assert!(!n.missing.iter().any(|m| m.contains("Node") || m.contains("npx")), "{:?}", n.missing);
    // only the one without an extension: not found
    std::fs::remove_file(nodejs.join("npx.cmd")).unwrap();
    assert_eq!(browser::on_path("npx", path), None);
    assert!(browser::needs(path, "", "").missing.iter().any(|m| m.starts_with("npx isn't found")));
}

/// Windows: Google Chrome in each of its places, `Google\Chrome\Application\chrome.exe` under %ProgramFiles%,
/// %ProgramFiles(x86)% and %LOCALAPPDATA% (an install for one user), with by_itself: chrome-devtools-mcp 1.10.1's
/// Puppeteer looks in all three, so a run passes no --executablePath. On PATH Chrome or Chromium is found too, and passed.
/// Brave never counts, in its own places or on PATH.
#[cfg(windows)]
#[test]
fn find_browser_finds_chrome_in_its_three_windows_places_and_never_brave() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = ["Program Files", "Program Files (x86)", r"Users\me\AppData\Local"].map(|d| tmp.path().join(d));
    for d in &dirs {
        std::fs::create_dir_all(d).unwrap();
    }
    let _places = Places::at(&dirs);
    let chrome = |d: &Path| d.join(r"Google\Chrome\Application\chrome.exe");
    let found = |p: &Path, name: &'static str, by_itself: bool| Some(browser::Found { path: p.to_path_buf(), name, by_itself });
    let none = OsStr::new("");
    assert_eq!(browser::find_browser(none), None, "nothing installed");

    // Brave in its own places, and its folder on PATH with a chrome.exe in it too
    for d in &dirs {
        program(&d.join(r"BraveSoftware\Brave-Browser\Application\brave.exe"));
    }
    let brave_dir = dirs[0].join(r"BraveSoftware\Brave-Browser\Application");
    program(&brave_dir.join("chrome.exe"));
    assert_eq!(browser::find_browser(brave_dir.as_os_str()), None);
    assert!(browser::needs(brave_dir.as_os_str(), "", "").missing.iter().any(|m| m.contains("No Google Chrome or Chromium found")));

    // each place on its own
    for (d, var) in dirs.iter().zip(PLACE_VARS) {
        let c = program(&chrome(d));
        assert_eq!(browser::find_browser(brave_dir.as_os_str()), found(&c, "Google Chrome", true), "%{var}%");
        assert_eq!(browser::program_for_run("", "", none), Ok(None), "%{var}%: the server finds it by itself");
        std::fs::remove_file(&c).unwrap();
    }
    // all three: %ProgramFiles% first, then %ProgramFiles(x86)%, then %LOCALAPPDATA%; before Chrome on PATH
    let bin = tmp.path().join("bin");
    let on_path = program(&bin.join("chrome.exe"));
    for d in dirs.iter().rev() {
        let c = program(&chrome(d));
        assert_eq!(browser::find_browser(bin.as_os_str()), found(&c, "Google Chrome", true));
    }
    let n = browser::needs(bin.as_os_str(), "", "");
    assert_eq!((n.browser.clone(), n.browser_name.as_deref()), (Some(chrome(&dirs[0]).display().to_string()), Some("Google Chrome")));

    // none in its places: Chrome on PATH, then Chromium on PATH, by their paths
    for d in &dirs {
        std::fs::remove_file(chrome(d)).unwrap();
    }
    assert_eq!(browser::find_browser(bin.as_os_str()), found(&on_path, "Google Chrome", false));
    assert_eq!(browser::program_for_run("", "", bin.as_os_str()), Ok(Some(on_path.display().to_string())));
    std::fs::remove_file(&on_path).unwrap();
    let chromium = program(&bin.join("chromium.exe"));
    assert_eq!(browser::find_browser(bin.as_os_str()), found(&chromium, "Chromium", false));
    assert_eq!(browser::program_for_run("", "", bin.as_os_str()), Ok(Some(chromium.display().to_string())));
}

/// Windows: the program you set is a full path with a drive letter, or `~\…` or `~/…` in your profile folder; a path
/// without a drive letter, a missing file and Brave's are refused, and the message's example is Chrome's Windows path.
#[cfg(windows)]
#[test]
fn check_program_takes_a_path_with_a_drive_letter_or_tilde_and_refuses_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().display().to_string(); // the profile folder, C:\Users\<you>
    let chrome = program(&tmp.path().join(r"AppData\Local\Google\Chrome\Application\chrome.exe"));
    let full = chrome.display().to_string();
    assert!(full.as_bytes().get(1) == Some(&b':'), "a drive letter: {full}");
    for ok in [full.clone(), format!("  {full}  "), r"~\AppData\Local\Google\Chrome\Application\chrome.exe".into(),
               "~/AppData/Local/Google/Chrome/Application/chrome.exe".into()] {
        assert_eq!(browser::check_program(&ok, &home), Ok(chrome.clone()), "{ok}");
        assert_eq!(browser::program_for_run(&ok, &home, OsStr::new("")), Ok(Some(full.clone())), "{ok}");
    }
    let _t = turn();
    let n = browser::needs(OsStr::new(""), r"~\AppData\Local\Google\Chrome\Application\chrome.exe", &home);
    assert_eq!((n.browser.clone(), n.browser_name.as_deref()), (Some(full.clone()), Some("The program you set")));

    for rel in ["chrome.exe", r"Google\Chrome\Application\chrome.exe", r"\Program Files\Google\Chrome\Application\chrome.exe", "C:chrome.exe",
                "/usr/bin/chromium"] {
        let e = browser::check_program(rel, &home).unwrap_err();
        assert_eq!(e, format!(r#"give the browser program as a full path, like C:\Program Files\Google\Chrome\Application\chrome.exe: not "{rel}""#));
    }
    let gone = tmp.path().join(r"Program Files\Google\Chrome\Application\chrome.exe");
    assert!(browser::check_program(&gone.display().to_string(), &home).unwrap_err().contains("isn't a program"), "a missing file");
    let notes = program(&tmp.path().join("notes.txt"));
    assert!(browser::check_program(&notes.display().to_string(), &home).unwrap_err().contains("isn't a program"), "not a program");
    let brave = program(&tmp.path().join(r"AppData\Local\BraveSoftware\Brave-Browser\Application\brave.exe"));
    for b in [brave.display().to_string(), r"~\AppData\Local\BraveSoftware\Brave-Browser\Application\brave.exe".into(),
              r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe".into()] {
        assert!(browser::check_program(&b, &home).unwrap_err().contains("Brave"), "{b}");
    }
    assert!(browser::needs(OsStr::new(""), &brave.display().to_string(), &home).missing.iter().any(|m| m.contains("Brave")));
}

/// Windows: the browser's entry in a run's config with npx and Chrome both under C:\Program Files. Claude Code puts
/// quotes around each argument with a space, and cmd strips the first and the last quote of the text after /c when it
/// starts with one and has more than two (`cmd /?`): `cmd /c "C:\Program Files\nodejs\npx.cmd" …
/// "--executablePath=C:\Program Files\…\chrome.exe"` would run `C:\Program`. The entry starts npx by its name, so the
/// text after /c starts with `npx`, not a quote, and cmd keeps every quote; cmd finds npx.cmd in the folder put first on
/// the PATH line, and `--executablePath=…` stays one argument.
#[cfg(windows)]
#[test]
fn the_runs_entry_gives_npx_by_its_name_through_cmd_with_its_folder_first_on_path() {
    let npx = r"C:\Program Files\nodejs\npx.cmd";
    let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
    let args = browser::args("1.10.1", Some(chrome), false);
    let env = browser::env(OsStr::new(r"C:\Windows\system32;C:\Windows"));
    let e = browser::entry(npx, &args, &env);
    let want: Vec<String> = ["/c", "npx"].map(String::from).into_iter().chain(args.iter().cloned()).collect();
    assert_eq!((&e["type"], &e["command"]), (&serde_json::json!("stdio"), &serde_json::json!("cmd")), "{e}");
    assert_eq!(e["args"], serde_json::json!(want));
    assert_eq!(e["args"].as_array().unwrap().last(), Some(&serde_json::json!(format!("--executablePath={chrome}"))), "one argument");
    assert!(!e["args"].as_array().unwrap().iter().any(|a| a.as_str().unwrap().contains(r"Program Files\nodejs")), "npx by its name: {e}");
    assert_eq!(e["env"]["PATH"], r"C:\Program Files\nodejs;C:\Windows\system32;C:\Windows");
    for (k, v) in [("NoDefaultCurrentDirectoryInExePath", "1"), ("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS", "1"), ("CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS", "1")] {
        assert_eq!(e["env"][k], v, "{e}");
    }
    // as the run's config file has it
    let server = mcp_run::RunServer { name: "chrome-devtools".into(), entry: e.clone(), tools_off: vec![], known_tools: vec![] };
    assert_eq!(mcp_run::config(vec![], &[server])["mcpServers"]["chrome-devtools"], e);
    // an uppercase extension is a batch file too, and an env without a PATH line gets one
    let e = browser::entry(r"C:\Program Files\nodejs\NPX.CMD", &args, &[]);
    assert_eq!((&e["command"], &e["args"][1], &e["env"]["PATH"]), (&serde_json::json!("cmd"), &serde_json::json!("NPX"), &serde_json::json!(r"C:\Program Files\nodejs")));
}

/// Windows: the entry started as Claude Code starts it, with npx.cmd and Chrome in folders with spaces: npx gets every
/// argument unchanged, `--executablePath=…` as one. The entry as it was before GA-79, npx by its path, doesn't start at
/// all (cmd runs `…\Program`): that shows cmd's quote rule is at work here.
#[cfg(windows)]
#[test]
fn the_runs_entry_starts_through_cmd_with_npx_and_chrome_in_folders_with_spaces() {
    let tmp = tempfile::tempdir().unwrap();
    let npx = install_npx(&tmp.path().join(r"Program Files\nodejs"));
    let chrome = program(&tmp.path().join(r"Program Files\Google\Chrome\Application\chrome.exe"));
    let args = browser::args("1.10.1", Some(&chrome.display().to_string()), true);
    let env = browser::env(&std::env::var_os("PATH").unwrap_or_default());
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();

    let out = tmp.path().join("args.json");
    let done = start_entry(&browser::entry(&npx.display().to_string(), &args, &env), &project, &out);
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    let seen = args_seen(&out);
    assert_eq!(seen, args);
    assert!(seen.contains(&format!("--executablePath={}", chrome.display())), "{seen:?}");

    let before = tmp.path().join("before.json");
    let done = start_entry(&mcp_run::stdio(&npx.display().to_string(), &args, &env), &project, &before);
    assert!(!done.status.success() && !before.exists(), "npx by its path through cmd /c started: {}", String::from_utf8_lossy(&done.stderr));

    // List tools starts npx.cmd by its path through os::command (as mcp_client does), with Rust's own batch-file quoting:
    // that copes with the spaces too
    let listed = tmp.path().join("listed.json");
    let done = gizai_agents::os::command(&npx).args(&args).envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str()))).env("FAKE_ARGS_OUT", &listed)
        .stdin(std::process::Stdio::null()).output().unwrap();
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    assert_eq!(args_seen(&listed), args);
}

/// Windows: cmd looks for a program in its current folder before the PATH, so an npx.cmd in the project a run works in
/// would start instead of npx. The entry's NoDefaultCurrentDirectoryInExePath=1 stops that: npx.cmd from npx's folder
/// starts. Without that line the project's would (cmd's own lookup, checked here too).
#[cfg(windows)]
#[test]
fn the_runs_entry_never_starts_an_npx_cmd_from_the_project_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let npx = install_npx(&tmp.path().join(r"Program Files\nodejs"));
    let args = browser::args("1.10.1", None, false);
    let env = browser::env(&std::env::var_os("PATH").unwrap_or_default());
    let project = tmp.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("npx.cmd"), "@ECHO OFF\r\necho project> \"%~dp0ran.txt\"\r\n").unwrap();

    let e = browser::entry(&npx.display().to_string(), &args, &env);
    let out = tmp.path().join("args.json");
    let done = start_entry(&e, &project, &out);
    assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
    assert!(!project.join("ran.txt").exists(), "the project's npx.cmd ran");
    assert_eq!(args_seen(&out), args);

    let mut without = e.clone();
    without["env"].as_object_mut().unwrap().remove("NoDefaultCurrentDirectoryInExePath");
    let out = tmp.path().join("without.json");
    start_entry(&without, &project, &out);
    assert!(project.join("ran.txt").exists() && !out.exists(), "without the line cmd starts the project's npx.cmd");
}

// ---- Stop and the time cap end the browser ----
// Linux and macOS only: Python and shell scripts as fake programs, Unix process groups and signals.

/// A process's state letter and process group from /proc (None once it is gone).
#[cfg(target_os = "linux")]
fn stat(pid: u32) -> Option<(char, u32)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((f.first()?.chars().next()?, f.get(2)?.parse().ok()?))
}

/// macOS, which has no /proc: the same from `ps`.
#[cfg(all(unix, not(target_os = "linux")))]
fn stat(pid: u32) -> Option<(char, u32)> {
    let out = std::process::Command::new("ps").args(["-o", "stat=,pgid=", "-p", &pid.to_string()]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut f = text.split_whitespace();
    Some((f.next()?.chars().next()?, f.next()?.parse().ok()?))
}

#[cfg(unix)]
fn ended(pid: u32) -> bool {
    stat(pid).is_none_or(|(state, _)| state == 'Z' || state == 'X')
}

/// (claude, server, browser) as the fakes wrote them; on drop, any still running (and ours by its command line) gets
/// SIGKILL, so a failing test leaves nothing behind.
#[cfg(unix)]
struct Pids([u32; 3]);

#[cfg(unix)]
impl Drop for Pids {
    fn drop(&mut self) {
        for pid in self.0 {
            let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).map(|b| String::from_utf8_lossy(&b).replace('\0', " ")).unwrap_or_default();
            if pid > 1 && !ended(pid) && (cmd.contains("sleep") || cmd.contains("fake-mcp-server") || cmd.contains("fake-claude-browser")) {
                // SAFETY: a plain kill of a process this test started (checked by its command line just above).
                unsafe { libc::kill(pid as i32, libc::SIGKILL); }
            }
        }
    }
}

#[cfg(unix)]
async fn read_pids(dir: &Path) -> Pids {
    let t0 = Instant::now();
    let read = |n: &str| std::fs::read_to_string(dir.join(n)).ok().and_then(|s| s.trim().parse::<u32>().ok());
    loop {
        if let (Some(a), Some(b), Some(c)) = (read("claude.pid"), read("server.pid"), read("helper.pid")) {
            return Pids([a, b, c]);
        }
        assert!(t0.elapsed() < Duration::from_secs(15), "the fake claude and the browser's server didn't start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[cfg(unix)]
async fn drain(h: &mut RunHandle) -> Vec<RunEvent> {
    let mut evs = vec![];
    while let Some(e) = h.events.recv().await {
        evs.push(e);
    }
    evs
}

/// Starts the fake claude with the browser's server in its MCP config, as a run gets it, and checks where each process is:
/// the server in the run's process group, the browser in one of its own.
#[cfg(unix)]
async fn start(tmp: &Path, mode: &str, max_time: Duration) -> (RunHandle, Pids) {
    let pids_dir = tmp.join("pids");
    let config = tmp.join("run.mcp.json");
    let args = browser::args("1.10.1", Some("/usr/bin/chromium"), false);
    browser::safe(&args).unwrap();
    let server = RunServer { name: "chrome-devtools".into(), entry: mcp_run::stdio(&executable(FAKE_NPX).display().to_string(), &args,
                             &browser::env(&std::env::var_os("PATH").unwrap_or_default())), tools_off: vec![], known_tools: vec![] };
    mcp_run::write_config(&config, &mcp_run::config(vec![], &[server])).unwrap();
    let claude = ClaudeArgs { bin: executable(FAKE_CLAUDE), prompt: "Test the login page".into(), session_id: "S".into(), permission_mode: "acceptEdits".into(),
                              mcp_config: Some(config), env: vec![("FAKE_BROWSER_PIDS".into(), pids_dir.display().to_string()), ("FAKE_BROWSER_MODE".into(), mode.into())],
                              ..Default::default() };
    let mut h = spawn(&claude, tmp, &tmp.join("r.jsonl"), Caps { max_time, max_tool_calls: 80 }).unwrap();
    let pids = read_pids(&pids_dir).await;
    let first = tokio::time::timeout(Duration::from_secs(10), h.events.recv()).await.expect("init line");
    assert!(first.is_some_and(|e| !matches!(e, RunEvent::Other { ref raw_type } if raw_type.starts_with("exit:"))), "the fake ended at once");
    let [claude, server, helper] = pids.0;
    assert_eq!(claude, h.pid);
    assert_eq!(stat(server).map(|s| s.1), Some(h.pid), "the server is in the run's process group, like Claude Code's");
    assert!(stat(helper).is_some_and(|s| s.1 != h.pid), "the browser is in a process group of its own, like Puppeteer's Chrome");
    // the server got the browser's command line, hidden and throwaway, and nothing goes to Google
    let argv: Vec<String> = std::fs::read_to_string(pids_dir.join("npx.argv")).unwrap().lines().map(str::to_string).collect();
    assert_eq!(argv, args);
    let env = std::fs::read_to_string(pids_dir.join("npx.env")).unwrap();
    assert!(env.contains("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS=1") && env.contains("CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS=1"), "{env}");
    (h, pids)
}

#[cfg(unix)]
async fn all_ended(p: &Pids) -> bool {
    let t0 = Instant::now();
    while !p.0.iter().all(|x| ended(*x)) {
        if t0.elapsed() > Duration::from_secs(5) {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    true
}

#[cfg(unix)]
fn alive(p: &Pids) -> Vec<(&'static str, u32)> {
    ["claude", "server", "browser"].into_iter().zip(p.0).filter(|(_, x)| !ended(*x)).collect()
}

#[cfg(unix)]
#[tokio::test]
async fn stop_ends_claude_the_browsers_server_and_the_browser_it_started_in_its_own_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, pids) = start(tmp.path(), "int", Duration::from_secs(60)).await;
    h.stop.stop();
    tokio::time::timeout(Duration::from_secs(15), drain(&mut h)).await.expect("the run ended");
    assert!(all_ended(&pids).await, "still running after Stop: {:?}", alive(&pids));
    let log = std::fs::read_to_string(tmp.path().join("pids/server.log")).unwrap_or_default();
    assert!(log.contains("ended helper on INT"), "the server closed its browser on SIGINT: {log:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn stop_ends_the_browser_of_a_server_that_closes_it_only_on_sigterm() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, pids) = start(tmp.path(), "term", Duration::from_secs(60)).await;
    h.stop.stop();
    tokio::time::timeout(Duration::from_secs(20), drain(&mut h)).await.expect("the run ended");
    assert!(all_ended(&pids).await, "still running after Stop: {:?}", alive(&pids));
    let log = std::fs::read_to_string(tmp.path().join("pids/server.log")).unwrap_or_default();
    assert!(log.contains("ended helper on TERM"), "{log:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn the_time_cap_ends_claude_the_browsers_server_and_the_browser() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, pids) = start(tmp.path(), "int", Duration::from_secs(2)).await;
    let evs = tokio::time::timeout(Duration::from_secs(20), drain(&mut h)).await.expect("the time cap ended the run");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time")), "{evs:?}");
    assert!(all_ended(&pids).await, "still running after the time cap: {:?}", alive(&pids));
}

#[cfg(unix)]
#[tokio::test]
async fn the_time_cap_ends_the_browser_of_a_server_that_closes_it_only_on_sigterm() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, pids) = start(tmp.path(), "term", Duration::from_secs(2)).await;
    let evs = tokio::time::timeout(Duration::from_secs(25), drain(&mut h)).await.expect("the time cap ended the run");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time")), "{evs:?}");
    assert!(all_ended(&pids).await, "still running after the time cap: {:?}", alive(&pids));
}
