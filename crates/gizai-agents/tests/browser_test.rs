// GA-55: the hidden browser agents test web pages with, Chrome DevTools MCP (crates/gizai-agents/src/browser.rs). Its
// command is always the pinned server, hidden (--headless) with a throwaway profile (--isolated), and never has an option
// that connects to a running browser or uses a real profile; Brave's program is refused; what it needs (Node, npx, Chrome
// or Chromium) is looked up on a fake PATH; and Stop and the time cap end the browser the server started in a process group
// of its own. The run's claude is fake-claude-browser.py and the server fake-npx-browser.sh: never the real npx, Chrome
// DevTools MCP or a browser.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_agents::browser;
use gizai_agents::claude::ClaudeArgs;
use gizai_agents::mcp_run::{self, RunServer};
use gizai_agents::process::{Caps, RunHandle, spawn};
use gizai_agents::stream::RunEvent;

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-browser.py");
const FAKE_NPX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-npx-browser.sh");

/// A committed fake, made runnable (its mode in git is 755; a checkout without it still runs).
fn executable(p: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    PathBuf::from(p)
}

/// A small script at `path` (made runnable).
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
    // Google Chrome where the server finds it itself, or Chromium in its usual place, counts on any PATH
    let system = ["/opt/google/chrome/chrome", "/usr/lib/chromium/chromium", "/usr/lib/chromium-browser/chromium-browser"]
        .into_iter().find(|p| Path::new(p).is_file());

    let n = browser::needs(&path(&new), "", &home);
    assert!(n.missing.is_empty(), "{:?}", n.missing);
    assert_eq!(n.node_version.as_deref(), Some("v24.1.0"));
    assert_eq!(n.npx, Some(new.join("npx").display().to_string()));
    if !Path::new("/opt/google/chrome/chrome").is_file() {
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

// ---- Stop and the time cap end the browser ----

fn stat(pid: u32) -> Option<(char, u32)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((f.first()?.chars().next()?, f.get(2)?.parse().ok()?))
}

fn ended(pid: u32) -> bool {
    stat(pid).is_none_or(|(state, _)| state == 'Z' || state == 'X')
}

/// (claude, server, browser) as the fakes wrote them; on drop, any still running (and ours by its command line) gets
/// SIGKILL, so a failing test leaves nothing behind.
struct Pids([u32; 3]);

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

async fn drain(h: &mut RunHandle) -> Vec<RunEvent> {
    let mut evs = vec![];
    while let Some(e) = h.events.recv().await {
        evs.push(e);
    }
    evs
}

/// Starts the fake claude with the browser's server in its MCP config, as a run gets it, and checks where each process is:
/// the server in the run's process group, the browser in one of its own.
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

fn alive(p: &Pids) -> Vec<(&'static str, u32)> {
    ["claude", "server", "browser"].into_iter().zip(p.0).filter(|(_, x)| !ended(*x)).collect()
}

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

#[tokio::test]
async fn the_time_cap_ends_claude_the_browsers_server_and_the_browser() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, pids) = start(tmp.path(), "int", Duration::from_secs(2)).await;
    let evs = tokio::time::timeout(Duration::from_secs(20), drain(&mut h)).await.expect("the time cap ended the run");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time")), "{evs:?}");
    assert!(all_ended(&pids).await, "still running after the time cap: {:?}", alive(&pids));
}

#[tokio::test]
async fn the_time_cap_ends_the_browser_of_a_server_that_closes_it_only_on_sigterm() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, pids) = start(tmp.path(), "term", Duration::from_secs(2)).await;
    let evs = tokio::time::timeout(Duration::from_secs(25), drain(&mut h)).await.expect("the time cap ended the run");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time")), "{evs:?}");
    assert!(all_ended(&pids).await, "still running after the time cap: {:?}", alive(&pids));
}
