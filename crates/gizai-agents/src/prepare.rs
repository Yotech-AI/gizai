//! Gets a card's new worktree ready before its agent starts, in this order: paths copied from the main checkout (a warm
//! build, the .env), dependencies that are still missing installed, then the project's setup command. Commands never
//! prompt (stdin is closed) and are ended, with everything they started, after a time limit.
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long one install or setup command may take.
pub const COMMAND_LIMIT: Duration = Duration::from_secs(20 * 60);
/// How long copying one path may take (a copy without reflinks, of a large target/, is slow).
const COPY_LIMIT: Duration = Duration::from_secs(30 * 60);
/// How much of a failed command's output is kept (the end of it).
const TAIL_CHARS: usize = 1500;

/// A project's way of preparing a new worktree (project form → New worktrees).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prepare {
    /// Files and folders copied from the main checkout with `cp --reflink=auto`, relative to it (".env",
    /// "node_modules/"). A path the main checkout doesn't have, or the worktree already has, is skipped.
    pub copy: Vec<String>,
    /// Install what is still missing: `composer install` for a composer.json without vendor/, and `npm ci` (with a
    /// package-lock.json; else `npm install`, which writes no lock file) for a package.json without node_modules/.
    pub install: bool,
    /// A command run with bash in the worktree after the install (a migration, a build).
    pub setup: Option<String>,
}

/// What preparing did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prepared {
    /// The paths it copied.
    pub copied: Vec<String>,
    /// The paths it skipped: the main checkout doesn't have them, or the worktree already does.
    pub skipped: Vec<String>,
    /// Copies that failed, and why. Nothing of them is left, so the install still fills in a dependency folder.
    pub failed: Vec<(String, String)>,
    /// The install commands it ran ("composer install", "npm ci").
    pub installed: Vec<String>,
    /// Whether it ran the setup command.
    pub setup: bool,
}

impl Prepared {
    /// What it did, in one line ("copied .env, node_modules/; ran npm ci and the setup command"); "" for nothing.
    pub fn summary(&self) -> String {
        let mut said = vec![];
        if !self.copied.is_empty() {
            said.push(format!("copied {}", self.copied.join(", ")));
        }
        for (path, why) in &self.failed {
            said.push(format!("couldn't copy {path} ({why})"));
        }
        let mut ran: Vec<String> = self.installed.clone();
        if self.setup {
            ran.push("the setup command".into());
        }
        if !ran.is_empty() {
            said.push(format!("ran {}", ran.join(" and ")));
        }
        said.join("; ")
    }
}

/// A command that failed while preparing.
#[derive(Debug, Clone, PartialEq)]
pub struct PrepareFailed {
    /// The command, as you'd type it ("composer install", or the setup command).
    pub command: String,
    /// What went wrong: "failed (exit code 1)", "couldn't start: composer isn't installed", "was stopped after 20 minutes".
    pub why: String,
    /// The end of its output.
    pub output: String,
}

impl std::fmt::Display for PrepareFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.command, self.why)?;
        if !self.output.is_empty() {
            write!(f, ":\n{}", self.output)?;
        }
        Ok(())
    }
}

/// A dependency folder the install looks after.
struct Dep {
    manifest: &'static str,
    folder: &'static str,
    /// Lock files: a dependency folder whose lock file changed is installed again in a reused worktree.
    locks: &'static [&'static str],
}

const DEPS: [Dep; 2] = [
    Dep { manifest: "composer.json", folder: "vendor", locks: &["composer.lock"] },
    Dep { manifest: "package.json", folder: "node_modules", locks: &["package-lock.json", "npm-shrinkwrap.json"] },
];

/// Prepares the worktree `wt` of the repository whose main checkout is `main`: copies, then installs, then the setup
/// command; the first command that fails stops it. `since` is the commit a reused worktree's dependency folders were
/// installed for: a folder whose lock file (else its composer.json or package.json) changed since then is installed
/// again. `path` is the PATH the commands get (None: Gizai's own).
pub fn prepare(main: &Path, wt: &Path, p: &Prepare, since: Option<&str>, path: Option<&OsStr>) -> Result<Prepared, PrepareFailed> {
    let mut out = Prepared::default();
    for entry in &p.copy {
        let rel = entry.trim().trim_start_matches("./").trim_end_matches('/');
        let unsafe_path = rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|c| c == ".." || c == ".git");
        let (from, to) = (main.join(rel), wt.join(rel));
        if unsafe_path || std::fs::symlink_metadata(&from).is_err() || std::fs::symlink_metadata(&to).is_ok() {
            out.skipped.push(entry.clone());
            continue;
        }
        match copy(&from, &to, path) {
            Ok(()) => out.copied.push(entry.clone()),
            Err(why) => {
                // a half copy would pass for a dependency folder that is already there
                let _ = if to.is_dir() { std::fs::remove_dir_all(&to) } else { std::fs::remove_file(&to) };
                out.failed.push((entry.clone(), why));
            }
        }
    }
    if p.install {
        for dep in &DEPS {
            if !wt.join(dep.manifest).is_file() {
                continue;
            }
            let missing = !wt.join(dep.folder).is_dir();
            if !missing && !since.is_some_and(|s| lock_changed(wt, s, dep)) {
                continue;
            }
            let (shown, program, args) = install_command(wt, dep);
            run(&shown, program, args, wt, path, COMMAND_LIMIT)?;
            out.installed.push(shown);
        }
    }
    if let Some(cmd) = p.setup.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        run(cmd, "bash", &["-c", cmd], wt, path, COMMAND_LIMIT)?;
        out.setup = true;
    }
    Ok(out)
}

/// What a project's copy list could start with, from its main checkout: .env when it has one, vendor/ for a
/// composer.json, node_modules/ for a package.json and target/ for a Cargo.toml.
pub fn suggest_copy(main: &Path) -> Vec<String> {
    let mut out = vec![];
    if main.join(".env").is_file() {
        out.push(".env".to_string());
    }
    for (manifest, folder) in [("composer.json", "vendor/"), ("package.json", "node_modules/"), ("Cargo.toml", "target/")] {
        if main.join(manifest).is_file() {
            out.push(folder.to_string());
        }
    }
    out
}

/// The install for `dep`: (as shown, program, arguments). Never prompts, and never rewrites a lock file.
fn install_command(wt: &Path, dep: &Dep) -> (String, &'static str, &'static [&'static str]) {
    if dep.folder == "vendor" {
        return ("composer install".into(), "composer", &["install", "--no-interaction", "--no-progress", "--no-ansi"]);
    }
    if dep.locks.iter().any(|l| wt.join(l).is_file()) {
        ("npm ci".into(), "npm", &["ci", "--no-audit", "--no-fund"])
    } else {
        ("npm install".into(), "npm", &["install", "--no-package-lock", "--no-audit", "--no-fund"])
    }
}

/// Whether `dep`'s lock file (or, without one, its manifest) differs between commit `since` and the worktree's HEAD.
fn lock_changed(wt: &Path, since: &str, dep: &Dep) -> bool {
    let blob = |rev: &str, file: &str| -> Option<String> {
        let out = Command::new("git").arg("-C").arg(wt).args(["rev-parse", "--verify", "--quiet", &format!("{rev}:{file}")]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let mut files: Vec<&str> = dep.locks.iter().copied().filter(|f| blob(since, f).is_some() || blob("HEAD", f).is_some()).collect();
    if files.is_empty() {
        files.push(dep.manifest);
    }
    files.iter().any(|f| blob(since, f) != blob("HEAD", f))
}

/// `cp --reflink=auto` (instant on btrfs and XFS, a plain copy elsewhere), keeping modes, times and symlinks.
fn copy(from: &Path, to: &Path, path: Option<&OsStr>) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let args = [OsStr::new("-a"), OsStr::new("--reflink=auto"), OsStr::new("--"), from.as_os_str(), to.as_os_str()];
    run("cp", "cp", &args, to.parent().unwrap_or(to), path, COPY_LIMIT).map(|_| ()).map_err(|f| {
        let detail = f.output.lines().last().unwrap_or_default().trim().to_string();
        if detail.is_empty() { f.why } else { detail }
    })
}

/// Runs `program args` in `dir` without prompts and returns the end of its output. After `limit` it is ended, with
/// everything it started (its own process group).
fn run<S: AsRef<OsStr>>(shown: &str, program: &str, args: &[S], dir: &Path, path: Option<&OsStr>, limit: Duration) -> Result<String, PrepareFailed> {
    use std::os::unix::process::CommandExt;
    let fail = |why: String, output: String| PrepareFailed { command: shown.to_string(), why, output };
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(dir).process_group(0)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .env("COMPOSER_NO_INTERACTION", "1").env("npm_config_yes", "true").env("npm_config_update_notifier", "false")
        .env("npm_config_fund", "false").env("npm_config_audit", "false").env("GIT_TERMINAL_PROMPT", "0").env("NO_COLOR", "1");
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => fail(format!("couldn't start: {program} isn't installed (or not on Gizai's PATH)"), String::new()),
        _ => fail(format!("couldn't start: {e}"), String::new()),
    })?;
    let pgid = child.id() as i32;
    let output = Arc::new(Mutex::new(Vec::<u8>::new()));
    let pipes: Vec<Box<dyn Read + Send>> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ].into_iter().flatten().collect();
    let readers: Vec<_> = pipes.into_iter().map(|mut pipe| {
        let output = output.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut o = output.lock().unwrap();
                o.extend_from_slice(&buf[..n]);
                if o.len() > 256 * 1024 {
                    let cut = o.len() - 64 * 1024;
                    o.drain(..cut);
                }
            }
        })
    }).collect();
    let until = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                unsafe { libc::kill(-pgid, libc::SIGKILL); }
                let _ = child.wait();
                break None;
            }
        }
    };
    // what it left running in the background would hold its output open
    unsafe { libc::kill(-pgid, libc::SIGKILL); }
    for r in readers {
        let _ = r.join();
    }
    let text = tail(&String::from_utf8_lossy(&output.lock().unwrap()));
    match status {
        Some(s) if s.success() => Ok(text),
        Some(s) => Err(fail(match s.code() {
            Some(code) => format!("failed (exit code {code})"),
            None => "was ended by a signal".into(),
        }, text)),
        None => Err(fail(format!("was stopped after {} minutes", limit.as_secs() / 60), text)),
    }
}

/// The end of a command's output, without terminal colours: at most `TAIL_CHARS` characters, from a line's start.
fn tail(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // ESC [ … letter
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(x) = chars.next() {
                    if x.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' {
            plain.push(c);
        }
    }
    let plain = plain.trim();
    let count = plain.chars().count();
    if count <= TAIL_CHARS {
        return plain.to_string();
    }
    let end: String = plain.chars().skip(count - TAIL_CHARS).collect();
    match end.find('\n') {
        Some(i) if i + 1 < end.len() => format!("…\n{}", &end[i + 1..]),
        _ => format!("…{end}"),
    }
}
