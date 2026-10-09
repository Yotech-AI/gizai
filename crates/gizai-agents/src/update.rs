//! Updates from GitHub Releases. The app decides when (src-tauri/src/update.rs); this is how.
//! - The release check asks GitHub for the latest release with curl: no login, and nothing is sent about you or your
//!   work. GitHub answers `/releases/latest` with the newest release that is neither a draft nor a pre-release.
//! - An update builds that release from source. A shallow fetch of its tag goes into a folder of its own, which is kept
//!   so the next build is quicker. Then the release's own installer builds it (`install.sh --build-only`) and installs
//!   it (`install.sh --skip-build`), so every release builds and installs the way its own version needs. On Windows the
//!   installer is `install.ps1` (`-BuildOnly`, then `-SkipBuild`), run with Windows PowerShell.
//! - Every command writes to the update's log and never prompts. Stop ends it with everything it started (its process
//!   group; a Job Object on Windows, see `os::Tree`).
use std::ffi::OsStr;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::connection::{self, Problem};
use crate::os::{self, End, Tree};

/// Gizai's own repository: its releases and their source (GIZAI_REPO changes it, as it does for install.sh).
pub const REPO: &str = "https://github.com/Yotech-AI/gizai.git";

/// How long getting a release's source may take.
pub const SOURCE_LIMIT: Duration = Duration::from_secs(15 * 60);
/// How long a build may take (the first one builds every dependency).
pub const BUILD_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);
/// How long the install may take.
pub const INSTALL_LIMIT: Duration = Duration::from_secs(10 * 60);

/// A release's version, MAJOR.MINOR.PATCH, as in its tag ("v0.1.6") and in Cargo.toml ("0.1.6").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// "0.1.6" or "v0.1.6". None for anything else ("0.2.0-beta.1", "0.2", "latest").
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim();
        let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
        let parts: Vec<&str> = s.split('.').collect();
        let num = |p: &str| (!p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit())).then(|| p.parse::<u64>().ok()).flatten();
        match parts.as_slice() {
            [a, b, c] => Some(Version(num(a)?, num(b)?, num(c)?)),
            _ => None,
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Whether `version` is newer than `than`. Both are MAJOR.MINOR.PATCH; anything else is never newer.
pub fn is_newer(version: &str, than: &str) -> bool {
    matches!((Version::parse(version), Version::parse(than)), (Some(a), Some(b)) if a > b)
}

/// A release on GitHub.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// "0.1.6"
    pub version: String,
    /// The tag it was made from: "v0.1.6".
    pub tag: String,
    /// Its title on GitHub.
    #[serde(default)]
    pub name: Option<String>,
    /// Its page on GitHub, with the notes.
    #[serde(default)]
    pub url: Option<String>,
    /// When it was published, as GitHub says it (ISO 8601).
    #[serde(default)]
    pub published_at: Option<String>,
    /// Its notes (Markdown), cut at 20,000 characters.
    #[serde(default)]
    pub notes: Option<String>,
}

const NOTES_CHARS: usize = 20_000;

/// GitHub's answer for a release (GET /repos/{owner}/{repo}/releases/latest).
#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// Reads GitHub's answer for a release. A draft, a pre-release, or a tag that isn't vMAJOR.MINOR.PATCH is no release to
/// update to.
pub fn parse_release(json: &str) -> Result<Release, Problem> {
    let r: GithubRelease = serde_json::from_str(json).map_err(|e| Problem::plain(format!("GitHub gave an answer Gizai can't read ({e})")))?;
    if r.draft || r.prerelease {
        let kind = if r.draft { "a draft" } else { "a pre-release" };
        return Err(Problem::plain(format!("The latest release, {}, is {kind}", r.tag_name)));
    }
    let version = Version::parse(&r.tag_name)
        .ok_or_else(|| Problem::plain(format!("The latest release's tag, {}, isn't a version like v1.2.3", r.tag_name)))?;
    let notes = r.body.map(|b| b.trim().to_string()).filter(|b| !b.is_empty()).map(|b| {
        if b.chars().count() > NOTES_CHARS { b.chars().take(NOTES_CHARS).collect::<String>() + "…" } else { b }
    });
    Ok(Release {
        version: version.to_string(),
        tag: r.tag_name.trim().to_string(),
        name: r.name.filter(|n| !n.trim().is_empty()),
        url: r.html_url,
        published_at: r.published_at,
        notes,
    })
}

/// Where the release check asks for the latest release of the GitHub repository owner/name.
pub fn latest_release_url(owner: &str, name: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{name}/releases/latest")
}

/// Asks `url` for the latest release with curl, which follows redirects (to https only) and gives up after `limit`.
/// The answer is GitHub's (see `latest_release_url`); an http:// or file:// URL works too (a test's fake release).
/// Ok(None): there is no release yet.
pub fn latest_release(url: &str, user_agent: &str, limit: Duration) -> Result<Option<Release>, Problem> {
    let mut cmd = os::command("curl");
    // -q first: your ~/.curlrc plays no part
    cmd.args(["-q", "--silent", "--show-error", "--location", "--proto", "=https,http,file", "--proto-redir", "=https"])
        .args(["--max-time", &limit.as_secs().max(1).to_string()])
        .args(["--header", "Accept: application/vnd.github+json", "--header", "X-GitHub-Api-Version: 2022-11-28"])
        .args(["--user-agent", user_agent, "--write-out", "\n%{http_code}", "--url", url]);
    let (ok, out, err) = crate::github::run(cmd, None, limit + Duration::from_secs(5)).map_err(|e| match e.kind() {
        ErrorKind::NotFound => Problem::new("curl isn't installed", "Install curl with your package manager: Gizai asks GitHub for new releases with it."),
        ErrorKind::TimedOut => connection::no_answer(limit),
        _ => Problem::plain(format!("can't run curl: {e}")),
    })?;
    if !ok {
        return Err(curl_problem(&err, url, limit));
    }
    // the answer, then the HTTP status curl adds (000 for a file)
    let (body, status) = out.rsplit_once('\n').unwrap_or(("", out.as_str()));
    match status.trim() {
        "200" | "000" => parse_release(body).map(Some),
        "404" => Ok(None),
        "403" | "429" if body.contains("rate limit") => {
            Err(Problem::new("GitHub turned the check away: too many requests from your network this hour", "Try again later."))
        }
        status => Err(Problem::plain(format!("GitHub answered the release check with HTTP {status}"))),
    }
}

/// What curl's error means ("curl: (6) Could not resolve host: api.github.com"), in plain words.
fn curl_problem(said: &str, url: &str, limit: Duration) -> Problem {
    let line = said.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("curl failed");
    let code = line.strip_prefix("curl: (").and_then(|r| r.split(')').next()).and_then(|c| c.parse::<u32>().ok());
    match code {
        Some(28) => connection::no_answer(limit),
        Some(5 | 6 | 7 | 35 | 52 | 55 | 56) => Problem::new("Can't reach GitHub", "Check your internet connection, then try again."),
        Some(37) => Problem::plain(format!("Can't read {url}")),
        _ => Problem::plain(line.trim_start_matches("curl: ").to_string()),
    }
}

/// The prefix the installer installed the Gizai at `exe` into. None for a Gizai that runs from anywhere else (a build in
/// a checkout's target/, a test build): an update never installs there. The layouts:
/// - Linux (install.sh): `<prefix>/lib/gizai/gizai`.
/// - macOS (install.sh): the same, a link into `<prefix>/lib/gizai/Gizai.app`; started from the Dock or Finder Gizai is
///   `<prefix>/lib/gizai/Gizai.app/Contents/MacOS/gizai`. `exe` is resolved first (std's `current_exe` doesn't follow
///   links on macOS), and both give `<prefix>`.
/// - Windows (install.ps1): `<prefix>\gizai.exe`, in a folder that holds install.ps1's marker `gizai-installed.txt` (a
///   build in target\release has none).
pub fn install_prefix(exe: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let dir = exe.parent()?;
        let installed = exe.file_name()?.eq_ignore_ascii_case("gizai.exe") && dir.join("gizai-installed.txt").is_file();
        installed.then(|| dir.to_path_buf())
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let exe = &mac_exe(exe);
        let lib_gizai = exe.parent()?;
        let lib = lib_gizai.parent()?;
        let named = exe.file_name()? == "gizai" && lib_gizai.file_name()? == "gizai" && lib.file_name()? == "lib";
        named.then(|| lib.parent().map(Path::to_path_buf)).flatten()
    }
}

/// macOS: `exe` with its links resolved, and Gizai.app's own binary (`<dir>/Gizai.app/Contents/MacOS/gizai`) taken as
/// the `<dir>/gizai` that install.sh links to it.
#[cfg(target_os = "macos")]
fn mac_exe(exe: &Path) -> PathBuf {
    let exe = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let in_app = exe.file_name().is_some_and(|n| n == "gizai") && exe.parent().is_some_and(|d| d.ends_with("Gizai.app/Contents/MacOS"));
    let link = exe.ancestors().nth(3).filter(|_| in_app).map(|app| app.with_file_name("gizai"));
    link.unwrap_or(exe)
}

/// The Gizai binary installed into `prefix`: `<prefix>/lib/gizai/gizai` (on macOS a link into Gizai.app, which
/// install.sh keeps), on Windows `<prefix>\gizai.exe`.
pub fn installed_exe(prefix: &Path) -> PathBuf {
    if cfg!(windows) { prefix.join("gizai.exe") } else { prefix.join("lib/gizai/gizai") }
}

/// The version the Gizai installed into `prefix` says it is: "gizai 0.1.6" from `gizai --version` gives "0.1.6".
pub fn installed_version(prefix: &Path) -> Option<String> {
    let mut cmd = os::command(installed_exe(prefix));
    cmd.arg("--version");
    let (ok, out, _) = crate::github::run(cmd, None, Duration::from_secs(10)).ok()?;
    let word = out.split_whitespace().nth(1)?;
    (ok && Version::parse(word).is_some()).then(|| word.to_string())
}

/// The version a source says it is: its Cargo.toml's [workspace.package] version.
pub fn source_version(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
            continue;
        }
        if in_package && let Some(rest) = line.strip_prefix("version") && let Some(value) = rest.trim_start().strip_prefix('=') {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

/// Stops an update's commands: the one that runs now ends with everything it started, and no other one starts.
#[derive(Clone, Default)]
pub struct Stop {
    asked: Arc<AtomicBool>,
    /// The process group (on Windows the job) of the command that runs now (None: none).
    tree: Arc<Mutex<Option<Tree>>>,
}

impl Stop {
    /// SIGTERM to the process group of the command that runs now (SIGKILL follows after 5 seconds). On Windows its job
    /// ends at once (see `os::End`).
    pub fn stop(&self) {
        self.asked.store(true, Ordering::SeqCst);
        if let Some(tree) = self.tree.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            tree.end(End::Terminate);
        }
    }

    pub fn asked(&self) -> bool {
        self.asked.load(Ordering::SeqCst)
    }

    /// The command that runs now (None: none).
    fn now(&self, tree: Option<Tree>) {
        *self.tree.lock().unwrap_or_else(|p| p.into_inner()) = tree;
    }
}

/// An update's log: what every command said, and Gizai's own lines between them.
#[derive(Clone)]
pub struct Log(Arc<Mutex<std::fs::File>>);

impl Log {
    /// A new log at `path`; an older one there is replaced.
    pub fn create(path: &Path) -> std::io::Result<Log> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Ok(Log(Arc::new(Mutex::new(std::fs::File::create(path)?))))
    }

    /// One line of Gizai's own.
    pub fn say(&self, line: &str) {
        let _ = writeln!(self.0.lock().unwrap_or_else(|p| p.into_inner()), "{line}");
    }

    fn write(&self, bytes: &[u8]) {
        let _ = self.0.lock().unwrap_or_else(|p| p.into_inner()).write_all(bytes);
    }
}

/// Why a step of an update didn't work, in plain words, with the end of what its command said (all of it is in the
/// log).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpdateFailed {
    pub what: String,
    pub output: String,
}

impl UpdateFailed {
    pub fn plain(what: impl Into<String>) -> UpdateFailed {
        UpdateFailed { what: what.into(), output: String::new() }
    }
}

impl std::fmt::Display for UpdateFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.what)?;
        if !self.output.is_empty() {
            write!(f, ":\n{}", self.output)?;
        }
        Ok(())
    }
}

/// Gets the source of release `tag` from `repo` into `dir`, a folder of its own (made when missing): a shallow fetch of
/// that tag alone, checked out as it is. Everything else in the folder goes, except the build's target/, so the next
/// build is quicker. git only ever works in `dir`'s own repository, never in one around it.
pub fn get_source(repo: &str, tag: &str, dir: &Path, log: &Log, stop: &Stop) -> Result<(), UpdateFailed> {
    std::fs::create_dir_all(dir).map_err(|e| UpdateFailed::plain(format!("Can't make {}: {e}", dir.display())))?;
    let dir = &dir.canonicalize().map_err(|e| UpdateFailed::plain(format!("Can't find {}: {e}", dir.display())))?;
    if !own_repo(dir) {
        // a .git cut short (Stop, a full disk) is made again
        let dot_git = dir.join(".git");
        let _ = if dot_git.is_dir() { std::fs::remove_dir_all(&dot_git) } else { std::fs::remove_file(&dot_git) };
        run("git init", git(dir, &["init", "--quiet"]), log, SOURCE_LIMIT, stop, false)?;
    }
    let refspec = format!("+refs/tags/{tag}:refs/tags/{tag}");
    run(&format!("git fetch --depth 1 {repo} {refspec}"), git(dir, &["fetch", "--depth", "1", "--no-tags", "--force", "--", repo, &refspec]),
        log, SOURCE_LIMIT, stop, false)
        .map_err(|f| fetch_problem(f, repo, tag))?;
    run("git checkout", git(dir, &["checkout", "--quiet", "--force", "--detach", &format!("refs/tags/{tag}")]), log, SOURCE_LIMIT, stop, false)?;
    run("git clean", git(dir, &["clean", "-ffdxq", "--exclude=/target/"]), log, SOURCE_LIMIT, stop, false)?;
    Ok(())
}

/// git in `dir` that never looks for a repository above it, so a broken one here can't send it into a checkout around
/// the data folder.
fn git(dir: &Path, args: &[&str]) -> Command {
    let mut c = os::command("git");
    c.arg("-C").arg(dir).args(args);
    if let Some(parent) = dir.parent() {
        c.env("GIT_CEILING_DIRECTORIES", parent);
    }
    c
}

/// Whether `dir` (canonical) holds a git repository of its own that works.
fn own_repo(dir: &Path) -> bool {
    let Ok(out) = git(dir, &["rev-parse", "--show-toplevel"]).stdin(Stdio::null()).stderr(Stdio::null()).output() else { return false };
    out.status.success() && Path::new(String::from_utf8_lossy(&out.stdout).trim()).canonicalize().ok().as_deref() == Some(dir)
}

/// What a failed fetch of a release's tag means, in plain words.
fn fetch_problem(f: UpdateFailed, repo: &str, tag: &str) -> UpdateFailed {
    let has = |s: &str| f.output.contains(s);
    let what = if has("couldn't find remote ref") {
        format!("{repo} has no tag {tag}. Check for updates again: the release may have been removed")
    } else if ["Could not resolve host", "Failed to connect", "Couldn't connect to server", "Connection timed out", "Network is unreachable"].iter().any(|s| has(s)) {
        "Can't reach GitHub. Check your internet connection, then try again".to_string()
    } else if has("Repository not found") || has("does not appear to be a git repository") {
        format!("Gizai can't find its repository at {repo}")
    } else {
        return f;
    };
    UpdateFailed { what, output: f.output }
}

/// The release's installer, and its two steps (build, install) as given to it and as the log shows them.
#[cfg(not(windows))]
const INSTALLER: &str = "install.sh";
#[cfg(not(windows))]
const STEPS: [(&str, &str); 2] = [("--build-only", "./install.sh --build-only"), ("--skip-build", "./install.sh --skip-build")];
#[cfg(windows)]
const INSTALLER: &str = "install.ps1";
#[cfg(windows)]
const STEPS: [(&str, &str); 2] = [("-BuildOnly", r".\install.ps1 -BuildOnly"), ("-SkipBuild", r".\install.ps1 -SkipBuild")];

/// Builds the release in `dir` with its own installer, `install.sh --build-only` (`install.ps1 -BuildOnly` on Windows),
/// at low CPU priority so Gizai stays quick meanwhile. `path` is the PATH the build gets (None: Gizai's own).
pub fn build(dir: &Path, path: Option<&OsStr>, log: &Log, stop: &Stop) -> Result<(), UpdateFailed> {
    let mut cmd = installer(dir, path)?;
    let (arg, shown) = STEPS[0];
    cmd.arg(arg);
    run(shown, cmd, log, BUILD_LIMIT, stop, true).map(|_| ())
}

/// Installs what `build` made into `prefix` with the release's installer, `install.sh --skip-build`. Its desktop entry
/// and icons go under the prefix too (`<prefix>/share`: ~/.local/share for the usual install), so an install anywhere
/// else, a test's scratch folder, never changes the launcher entry you use. The installer backs up the data folder
/// there (`<prefix>/share/gizai`) with the new build first, and installs nothing when it can't.
/// On Windows `install.ps1 -SkipBuild` gets the prefix only: XDG_DATA_HOME means nothing there, and the data folder
/// install.ps1 backs up is `%APPDATA%\Gizai`.
pub fn install(dir: &Path, prefix: &Path, path: Option<&OsStr>, log: &Log, stop: &Stop) -> Result<(), UpdateFailed> {
    let mut cmd = installer(dir, path)?;
    let (arg, shown) = STEPS[1];
    cmd.arg(arg).env("GIZAI_PREFIX", prefix);
    #[cfg(not(windows))]
    cmd.env("XDG_DATA_HOME", prefix.join("share"));
    run(shown, cmd, log, INSTALL_LIMIT, stop, false).map(|_| ())
}

/// The release's installer, run in its source, building into the source's own target/: install.sh with bash, on
/// Windows install.ps1 with Windows PowerShell (`-ExecutionPolicy Bypass`, as the release's own script isn't signed;
/// `-NonInteractive`, so a question fails instead of waiting).
fn installer(dir: &Path, path: Option<&OsStr>) -> Result<Command, UpdateFailed> {
    let script = dir.join(INSTALLER);
    if !script.is_file() {
        return Err(UpdateFailed::plain(format!("The release has no {INSTALLER}, so Gizai can't build it")));
    }
    #[cfg(not(windows))]
    let mut cmd = os::command("bash");
    #[cfg(windows)]
    let mut cmd = os::command("powershell.exe");
    #[cfg(windows)]
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]);
    cmd.arg(script).current_dir(dir)
        .env("CARGO_TARGET_DIR", dir.join("target")).env("TAURI_TELEMETRY_DISABLED", "1")
        .env_remove("GIZAI_REPO").env_remove("GIZAI_BRANCH").env_remove("GIZAI_PREFIX").env_remove("GIZAI_DATA_DIR");
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    Ok(cmd)
}

/// Runs `cmd` (`shown` in the log) in its own process group (a Job Object on Windows) without prompts, its output going
/// to the log. It is ended, with everything it started, after `limit` or when `stop` is asked. `low`: at low CPU
/// priority (nice 10; below normal on Windows). Ok: the end of its output.
fn run(shown: &str, mut cmd: Command, log: &Log, limit: Duration, stop: &Stop, low: bool) -> Result<String, UpdateFailed> {
    if stop.asked() {
        return Err(UpdateFailed::plain("Stopped"));
    }
    log.say(&format!("$ {shown}"));
    let program = cmd.get_program().to_string_lossy().to_string();
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0").env("NO_COLOR", "1").env("CARGO_TERM_COLOR", "never")
        .env("npm_config_color", "false").env("npm_config_update_notifier", "false").env("npm_config_fund", "false").env("npm_config_audit", "false");
    let (mut child, tree) = os::spawn_tree(&mut cmd, low).map_err(|e| match e.kind() {
        ErrorKind::NotFound => UpdateFailed::plain(format!("{program} isn't installed (or not on Gizai's PATH)")),
        _ => UpdateFailed::plain(format!("Couldn't start {shown}: {e}")),
    })?;
    stop.now(Some(tree.clone()));
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let pipes: Vec<Box<dyn Read + Send>> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ].into_iter().flatten().collect();
    let readers: Vec<_> = pipes.into_iter().map(|mut pipe| {
        let (output, log) = (output.clone(), log.clone());
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                log.write(&buf[..n]);
                let mut o = output.lock().unwrap_or_else(|p| p.into_inner());
                o.extend_from_slice(&buf[..n]);
                if o.len() > 256 * 1024 {
                    let cut = o.len() - 64 * 1024;
                    o.drain(..cut);
                }
            }
        })
    }).collect();
    let until = Instant::now() + limit;
    let (mut ending, mut timed_out) = (None::<Instant>, false);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {}
            Err(_) => break None,
        }
        if ending.is_none() && (stop.asked() || Instant::now() >= until) {
            timed_out = !stop.asked();
            tree.end(End::Terminate);
            ending = Some(Instant::now());
        }
        if ending.is_some_and(|at| at.elapsed() > Duration::from_secs(5)) {
            tree.end(End::Kill);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    stop.now(None);
    // what it left running in the background (a compiler cache's server) would hold its output open
    tree.end(End::Terminate);
    let quiet_by = Instant::now() + Duration::from_secs(3);
    while readers.iter().any(|r| !r.is_finished()) && Instant::now() < quiet_by {
        std::thread::sleep(Duration::from_millis(50));
    }
    if readers.iter().any(|r| !r.is_finished()) {
        tree.end(End::Kill);
        let quiet_by = Instant::now() + Duration::from_secs(2);
        while readers.iter().any(|r| !r.is_finished()) && Instant::now() < quiet_by {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    // a reader still busy belongs to something that left the group and keeps the pipe open: it is left behind
    for r in readers.into_iter().filter(|r| r.is_finished()) {
        let _ = r.join();
    }
    let text = crate::prepare::tail(&String::from_utf8_lossy(&output.lock().unwrap_or_else(|p| p.into_inner())));
    let failed = |what: String| UpdateFailed { what, output: text.clone() };
    match status {
        _ if ending.is_some() && !timed_out => Err(failed("Stopped".into())),
        _ if timed_out => Err(failed(format!("{shown} was stopped after {} minutes", limit.as_secs() / 60))),
        Some(s) if s.success() => Ok(text),
        Some(s) => Err(failed(match s.code() {
            Some(code) => format!("{shown} failed (exit code {code})"),
            None => format!("{shown} was ended by a signal"),
        })),
        None => Err(failed(format!("Gizai lost track of {shown}"))),
    }
}
