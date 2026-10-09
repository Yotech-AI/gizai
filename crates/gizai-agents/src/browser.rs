//! The hidden browser agents test web pages with: Chrome DevTools MCP, Google's MCP server for coding agents, as a built-in
//! entry in Settings → MCP servers. It starts its own Chrome, always hidden (`--headless`) with a throwaway profile
//! (`--isolated`): never on your screen, never your browser, profile or logins. Gizai installs nothing: `npx` fetches the
//! pinned server, and the server starts the browser.
//!
//! Checked with chrome-devtools-mcp 1.10.1 (`--help` and its code, without starting a browser):
//! - it starts Chrome only at the first tool call (List tools starts none), through Puppeteer, in a process group of its
//!   own (`detached`), and ends it on SIGINT, SIGTERM, SIGHUP and when its own process exits;
//! - `--no-usage-statistics` keeps it from sending usage statistics (and from starting its detached telemetry watchdog),
//!   `--no-performance-crux` from sending page addresses of performance traces to Google's CrUX API, and
//!   `CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS` from starting a detached process that asks npm for its latest version;
//! - `--browserUrl`, `--wsEndpoint` and `--autoConnect` connect to a running browser, and `--userDataDir` and `--channel`
//!   pick a real profile or install: Gizai never passes them (`safe`).
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The npm package.
pub const PACKAGE: &str = "chrome-devtools-mcp";
/// What Node the pinned version needs (its package.json `engines`: ^20.19.0 || ^22.12.0 || >=23).
pub const NODE_NEEDED: &str = "Node 20.19 or newer (22.12 or newer on Node 22)";

/// Options that would connect to a running browser, use a real profile, read a config file or pass Chrome other
/// arguments: never in the command, also not as `--option=value`.
pub const FORBIDDEN: [&str; 22] = [
    "--browserUrl", "--browser-url", "-u", "--wsEndpoint", "--ws-endpoint", "-w", "--wsHeaders", "--ws-headers", "--autoConnect",
    "--auto-connect", "--userDataDir", "--user-data-dir", "--channel", "--config", "--chromeArg", "--chrome-arg",
    "--ignoreDefaultChromeArg", "--ignore-default-chrome-arg", "--proxyServer", "--proxy-server", "--categoryExtensions", "--category-extensions",
];

/// The server's arguments after `npx`: the pinned package, always hidden with a throwaway profile, nothing sent to
/// Google, and the browser program when Gizai picked one (`--executablePath=…`, one argument, so a path can never pass for
/// an option). `insecure_certs`: accept self-signed and expired certificates (local `.test` sites).
pub fn args(version: &str, program: Option<&str>, insecure_certs: bool) -> Vec<String> {
    let mut a: Vec<String> = vec!["-y".into(), format!("{PACKAGE}@{version}"), "--headless".into(), "--isolated".into(),
                                  "--no-usage-statistics".into(), "--no-performance-crux".into()];
    if let Some(p) = program.filter(|p| !p.is_empty()) {
        a.push(format!("--executablePath={p}"));
    }
    if insecure_certs {
        a.push("--acceptInsecureCerts".into());
    }
    a
}

/// The server's environment lines: no update check (a detached process), no usage statistics, and a PATH with `node` on
/// it (`npx` is a Node script).
pub fn env(path: &OsStr) -> Vec<(String, String)> {
    vec![("CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS".into(), "1".into()), ("CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS".into(), "1".into()),
         ("PATH".into(), path.to_string_lossy().into_owned())]
}

/// Checks a command line for the browser: `--headless` and `--isolated` there, no option that connects to a running
/// browser or uses a real profile, and only an exact version of the package. Why not, in plain words.
pub fn safe(args: &[String]) -> Result<(), String> {
    for must in ["--headless", "--isolated"] {
        if !args.iter().any(|a| a == must) {
            return Err(format!("the browser always starts with {must}"));
        }
    }
    for a in args {
        let opt = a.split('=').next().unwrap_or(a);
        if FORBIDDEN.contains(&opt) || opt == "--headless=false" || a == "--no-headless" || a == "--no-isolated" || a.starts_with("--headless=")
            || a.starts_with("--isolated=") {
            return Err(format!("{opt} isn't allowed for the browser: it never connects to a running browser or uses a real profile"));
        }
    }
    match args.iter().find(|a| a.starts_with(&format!("{PACKAGE}@"))) {
        Some(p) if exact_version(&p[PACKAGE.len() + 1..]) => Ok(()),
        _ => Err(format!("the browser runs an exact version of {PACKAGE}, never latest")),
    }
}

/// An exact version like 1.10.1 (or 1.11.0-beta.2): never `latest`, a range or a tag. The same rule as
/// `gizai_core::mcp_servers::exact_version`, which Settings saves with.
pub fn exact_version(v: &str) -> bool {
    let (core, pre) = v.split_once('-').unwrap_or((v, ""));
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 6 && p.chars().all(|c| c.is_ascii_digit()))
        && (v.split_once('-').is_none() || (!pre.is_empty() && pre.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')))
}

/// `~/…` with the home folder filled in.
fn expand_home(s: &str, home: &str) -> String {
    match s.strip_prefix("~/") {
        Some(rest) if !home.is_empty() => format!("{}/{rest}", home.trim_end_matches('/')),
        _ if s == "~" && !home.is_empty() => home.to_string(),
        _ => s.to_string(),
    }
}

/// Brave, by the path or the program it leads to: never the browser for an agent.
pub fn is_brave(path: &Path) -> bool {
    let brave = |p: &Path| p.to_string_lossy().to_lowercase().contains("brave");
    brave(path) || std::fs::canonicalize(path).is_ok_and(|c| brave(&c))
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// The browser program you set: a full path to a program that is there, and never Brave's.
pub fn check_program(raw: &str, home: &str) -> Result<PathBuf, String> {
    let p = expand_home(raw.trim(), home);
    if is_brave(Path::new(&p)) {
        return Err("Brave can't be the agents' browser: pick Google Chrome or Chromium".into());
    }
    if !p.starts_with('/') {
        return Err(format!("give the browser program as a full path, like /usr/bin/chromium: not \"{}\"", raw.trim()));
    }
    let path = PathBuf::from(&p);
    if !executable(&path) {
        return Err(format!("{p} isn't a program on this computer"));
    }
    Ok(path)
}

/// Google Chrome where Chrome DevTools MCP looks for it by itself (its stable channel).
const CHROME_HOME: [&str; 2] = ["/opt/google/chrome/chrome", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"];
const CHROME_NAMES: [&str; 3] = ["google-chrome-stable", "google-chrome", "chrome"];
const CHROMIUM: [&str; 3] = ["/Applications/Chromium.app/Contents/MacOS/Chromium", "/usr/lib/chromium/chromium", "/usr/lib/chromium-browser/chromium-browser"];
const CHROMIUM_NAMES: [&str; 2] = ["chromium", "chromium-browser"];

/// A browser Gizai found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    /// "Google Chrome" or "Chromium".
    pub name: &'static str,
    /// The server finds it by itself (Google Chrome where it looks); else Gizai passes its path.
    pub by_itself: bool,
}

/// Google Chrome, else Chromium, in the usual places and on `path` (Brave's never counts).
pub fn find_browser(path: &OsStr) -> Option<Found> {
    let on_path = |name: &str| std::env::split_paths(path).map(|d| d.join(name)).find(|p| executable(p) && !is_brave(p));
    if let Some(p) = CHROME_HOME.iter().map(PathBuf::from).find(|p| executable(p)) {
        return Some(Found { path: p, name: "Google Chrome", by_itself: true });
    }
    if let Some(p) = CHROME_NAMES.iter().find_map(|n| on_path(n)) {
        return Some(Found { path: p, name: "Google Chrome", by_itself: false });
    }
    CHROMIUM_NAMES.iter().find_map(|n| on_path(n)).or_else(|| CHROMIUM.iter().map(PathBuf::from).find(|p| executable(p) && !is_brave(p)))
        .map(|p| Found { path: p, name: "Chromium", by_itself: false })
}

/// Whether a Node version (`v24.21.0`) is one the pinned server takes: ^20.19.0 || ^22.12.0 || >=23.
pub fn node_ok(version: &str) -> bool {
    let v: Vec<u64> = version.trim().trim_start_matches('v').split('.').map(|p| p.parse().unwrap_or(0)).collect();
    let (major, minor) = (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0));
    major >= 23 || (major == 22 && minor >= 12) || (major == 20 && minor >= 19)
}

/// A program on `path`, by name.
pub fn on_path(name: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path).map(|d| d.join(name)).find(|p| executable(p))
}

/// What the browser needs and whether it is here: Node (its version), npx and a browser.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Needs {
    pub node: Option<String>,
    pub node_version: Option<String>,
    pub npx: Option<String>,
    /// The browser program a run gets: the one you set, else Google Chrome or Chromium as found.
    pub browser: Option<String>,
    pub browser_name: Option<String>,
    /// What is missing and what to install, in plain words; empty = ready.
    pub missing: Vec<String>,
}

/// Looks for Node (and asks its version), npx and the browser (`program`: the one set in Settings, if any).
pub fn needs(path: &OsStr, program: &str, home: &str) -> Needs {
    let mut n = Needs::default();
    let node = on_path("node", path);
    if let Some(node) = &node {
        n.node = Some(node.display().to_string());
        n.node_version = std::process::Command::new(node).arg("--version").env("PATH", path).stdin(std::process::Stdio::null())
            .output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    }
    match (&node, n.node_version.as_deref()) {
        (None, _) => n.missing.push(format!("Node isn't found: install {NODE_NEEDED} (it comes with npx).")),
        (Some(_), Some(v)) if !node_ok(v) => n.missing.push(format!("Node {v} is too old: install {NODE_NEEDED}.")),
        _ => {}
    }
    n.npx = on_path("npx", path).map(|p| p.display().to_string());
    if n.npx.is_none() && node.is_some() {
        n.missing.push("npx isn't found: it comes with npm, next to Node. Install npm.".into());
    }
    if program.trim().is_empty() {
        match find_browser(path) {
            Some(f) => { n.browser = Some(f.path.display().to_string()); n.browser_name = Some(f.name.into()); }
            None => n.missing.push("No Google Chrome or Chromium found: install one of them (Brave doesn't count).".into()),
        }
    } else {
        match check_program(program, home) {
            Ok(p) => { n.browser = Some(p.display().to_string()); n.browser_name = Some("The program you set".into()); }
            Err(e) => n.missing.push(format!("The browser program: {e}.")),
        }
    }
    n
}

/// The browser program a run passes to the server: the one set, else the one found unless the server finds it itself.
pub fn program_for_run(program: &str, home: &str, path: &OsStr) -> Result<Option<String>, String> {
    if !program.trim().is_empty() {
        return check_program(program, home).map(|p| Some(p.display().to_string()));
    }
    match find_browser(path) {
        Some(f) if f.by_itself => Ok(None),
        Some(f) => Ok(Some(f.path.display().to_string())),
        None => Err("no Google Chrome or Chromium found: install one of them".into()),
    }
}
