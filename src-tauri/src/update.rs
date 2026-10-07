//! Updates: Settings → Updates, and the notice above Company in the sidebar.
//! - The release check asks GitHub for the latest release: 20 seconds after Gizai starts when one is due, then every
//!   six hours while "Check for new releases" is on, and whenever you press Check now. What it found is kept, so the
//!   notice shows from the start.
//! - A release newer than this Gizai is offered as "Update to <version>". The update gets its source and builds it in
//!   the background while Gizai stays usable. Then it backs up your data, installs the release with its own
//!   install.sh, and offers a restart. A step that fails leaves the installed Gizai as it was and says why; the log in
//!   <data folder>/update has everything.
//! - Only a Gizai that install.sh installed (<prefix>/lib/gizai/gizai) updates itself. A dev or test build that runs
//!   from anywhere else never installs over anything.
//!
//! Usable without a Tauri app (tests): the UI hears about changes through `AppState::notify`.
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use gizai_agents::connection::Problem;
use gizai_agents::update::{self as up, Release, Stop, UpdateFailed};
use gizai_core::{ids, repo_url, settings};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::runs::Note;

/// This Gizai's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How often the release check asks GitHub while it is on…
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// …and an hour after a check that failed (no internet, GitHub busy).
pub const RETRY_AFTER: Duration = Duration::from_secs(60 * 60);
/// The automatic check waits this long after Gizai starts, so starting stays quick.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(20);
/// How often Gizai looks whether a check is due.
pub const TICK: Duration = Duration::from_secs(10 * 60);
/// How long one check may take.
const CHECK_LIMIT: Duration = Duration::from_secs(30);

/// Settings → Updates → Check for new releases (on unless switched off).
const AUTO_CHECK: &str = "update_auto_check";
/// What the last check found (`LastCheck`).
const LAST_CHECK: &str = "update_last_check";

/// Where releases and their source come from.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The git repository a release's tag is fetched from: GIZAI_REPO, else Gizai's own.
    pub repo: String,
    /// What the release check reads: GIZAI_RELEASES_URL, else GitHub's latest release of `repo`. None: `repo` isn't on
    /// GitHub and no URL was given.
    pub releases_url: Option<String>,
}

impl Source {
    /// From GIZAI_REPO and GIZAI_RELEASES_URL (as install.sh reads GIZAI_REPO).
    pub fn from_env() -> Source {
        let var = |name: &str| std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Source::new(&var("GIZAI_REPO").unwrap_or_else(|| up::REPO.to_string()), var("GIZAI_RELEASES_URL"))
    }

    pub fn new(repo: &str, releases_url: Option<String>) -> Source {
        let releases_url = releases_url.or_else(|| {
            let link = repo_url::normalize(repo).ok().flatten().filter(|l| l.provider == "github")?;
            Some(up::latest_release_url(link.owner.as_deref()?, link.name.as_deref()?))
        });
        Source { repo: repo.to_string(), releases_url }
    }
}

/// The release check and the update, while Gizai runs.
pub struct Updates {
    inner: Mutex<Inner>,
}

struct Inner {
    source: Source,
    /// The program that runs: its install prefix is where an update installs.
    exe: Option<PathBuf>,
    /// A check runs now.
    checking: bool,
    /// The version the installed Gizai says it is, as the last check found it.
    installed: Option<String>,
    /// The update that runs now, or the last one since Gizai started.
    job: Option<Job>,
}

struct Job {
    status: UpdateJob,
    stop: Stop,
}

impl Default for Updates {
    fn default() -> Self {
        Updates::new(Source::from_env(), std::env::current_exe().ok())
    }
}

impl Updates {
    pub fn new(source: Source, exe: Option<PathBuf>) -> Updates {
        Updates { inner: Mutex::new(Inner { source, exe, checking: false, installed: None, job: None }) }
    }

    /// For tests: where releases come from.
    pub fn set_source(&self, source: Source) {
        self.lock().source = source;
    }

    /// For tests: the program that runs (an update installs into its prefix, `<prefix>/lib/gizai/gizai`).
    pub fn set_exe(&self, exe: Option<PathBuf>) {
        self.lock().exe = exe;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// What the last release check found. Kept in settings, so the notice shows from the start.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCheck {
    /// When (ms).
    pub at: i64,
    /// The latest release on GitHub. None: there is none yet, or no check has worked yet.
    pub latest: Option<Release>,
    /// Why the check didn't work (the release found before it stays).
    pub problem: Option<Problem>,
}

/// An update: the step it is at, or how it ended.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJob {
    /// The version it installs.
    pub version: String,
    /// While it runs: "source" (getting the source), "build", "backup", "install". When it has ended: "installed",
    /// "failed" or "stopped".
    pub step: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// Its log: every command and what it said.
    pub log: String,
    /// The backup of your data it made before installing.
    pub backup: Option<String>,
    /// Why it failed, in plain words. For an installed update: what the installer said went wrong after the new
    /// version was in place (the desktop entry, the icons).
    pub problem: Option<String>,
    /// The end of what the failed command said.
    pub output: Option<String>,
    /// While it runs and when it failed: whether the Gizai installed before is still in place, as it was. It is, unless
    /// the install step failed partway.
    pub unchanged: bool,
}

impl UpdateJob {
    pub fn running(&self) -> bool {
        matches!(self.step.as_str(), "source" | "build" | "backup" | "install")
    }
}

/// What Settings → Updates and the sidebar show.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// This Gizai's version.
    pub current: String,
    /// Check for new releases: on (the default) or off.
    pub auto_check: bool,
    /// A check runs now.
    pub checking: bool,
    /// When the last check ran (ms), and what it found.
    pub checked_at: Option<i64>,
    pub latest: Option<Release>,
    pub problem: Option<Problem>,
    /// The latest release, when it is newer than this Gizai.
    pub available: Option<Release>,
    /// The version that is installed already when it is newer than this Gizai (installed by an update, or by install.sh
    /// meanwhile): a restart starts it.
    pub installed: Option<String>,
    /// Where an update installs (the prefix this Gizai was installed into)…
    pub install_to: Option<String>,
    /// …or why this Gizai can't update itself.
    pub cannot_install: Option<String>,
    /// The update that runs now, or the last one since Gizai started.
    pub job: Option<UpdateJob>,
    /// Where releases come from.
    pub repo: String,
}

fn changed(st: &AppState) {
    (st.notify)(Note::UpdateChanged);
}

/// Check for new releases: on unless switched off.
pub fn auto_check(st: &AppState) -> bool {
    settings::get(&st.db, AUTO_CHECK).ok().flatten().unwrap_or(true)
}

pub fn set_auto_check(st: &AppState, on: bool) -> Result<UpdateStatus, String> {
    settings::set(&st.db, AUTO_CHECK, &on).map_err(|e| e.to_string())?;
    changed(st);
    Ok(status(st))
}

pub fn last_check(st: &AppState) -> Option<LastCheck> {
    settings::get(&st.db, LAST_CHECK).ok().flatten()
}

/// Where an update installs: the prefix install.sh installed this Gizai into. Else why it can't update itself.
fn install_prefix(inner: &Inner) -> Result<PathBuf, String> {
    let Some(exe) = &inner.exe else {
        return Err("Gizai can't tell where it runs from, so it can't update itself. Install the new version with ./install.sh.".into());
    };
    up::install_prefix(exe).ok_or_else(|| {
        format!("This Gizai runs from {}, not from an install, so it doesn't update itself. Update its checkout with git, or install with ./install.sh.",
                exe.display())
    })
}

pub fn status(st: &AppState) -> UpdateStatus {
    let (last, auto) = (last_check(st), auto_check(st));
    let inner = st.updates.lock();
    let latest = last.as_ref().and_then(|l| l.latest.clone());
    let (install_to, cannot_install) = match install_prefix(&inner) {
        Ok(p) => (Some(p.display().to_string()), None),
        Err(why) => (None, Some(why)),
    };
    UpdateStatus {
        current: VERSION.into(),
        auto_check: auto,
        checking: inner.checking,
        checked_at: last.as_ref().map(|l| l.at),
        available: latest.clone().filter(|r| up::is_newer(&r.version, VERSION)),
        latest,
        problem: last.and_then(|l| l.problem),
        installed: inner.installed.clone().filter(|v| up::is_newer(v, VERSION)),
        install_to,
        cannot_install,
        job: inner.job.as_ref().map(|j| j.status.clone()),
        repo: inner.source.repo.clone(),
    }
}

/// Asks GitHub for the latest release now and keeps what it found. When a check runs already, returns at once (that
/// one tells the UI when it is done). A check that doesn't work keeps the release found before.
pub async fn check(st: &AppState) -> UpdateStatus {
    let (url, prefix) = {
        let mut inner = st.updates.lock();
        if inner.checking {
            drop(inner);
            return status(st);
        }
        inner.checking = true;
        (inner.source.releases_url.clone(), install_prefix(&inner).ok())
    };
    let running = Checking(&st.updates);
    changed(st);
    let found = tokio::task::spawn_blocking(move || {
        let latest = match url {
            Some(url) => up::latest_release(&url, &format!("gizai/{VERSION}"), CHECK_LIMIT),
            None => Err(Problem::new("Gizai doesn't know where this repository keeps its releases",
                                     "Set GIZAI_RELEASES_URL, or GIZAI_REPO to a repository on GitHub.")),
        };
        // a release that is installed already only needs a restart
        let installed = match (&latest, &prefix) {
            (Ok(Some(r)), Some(prefix)) if up::is_newer(&r.version, VERSION) => up::installed_version(prefix),
            _ => None,
        };
        (latest, installed)
    }).await;
    let (latest, installed) = found.unwrap_or_else(|e| (Err(Problem::plain(e.to_string())), None));
    let now = ids::now_ms();
    let last = match latest {
        Ok(latest) => LastCheck { at: now, latest, problem: None },
        Err(p) => LastCheck { at: now, latest: last_check(st).and_then(|l| l.latest), problem: Some(p) },
    };
    if let Err(e) = settings::set(&st.db, LAST_CHECK, &last) {
        eprintln!("gizai: can't keep the release check's result: {e}");
    }
    if installed.is_some() || last.problem.is_none() {
        st.updates.lock().installed = installed;
    }
    drop(running);
    changed(st);
    status(st)
}

/// Held while a check runs: `checking` goes back to false however the check ends.
struct Checking<'a>(&'a Updates);

impl Drop for Checking<'_> {
    fn drop(&mut self) {
        self.0.lock().checking = false;
    }
}

/// The automatic check: asks GitHub when Check for new releases is on and the last check is six hours old (an hour,
/// when it didn't work). Returns whether it asked.
pub async fn tick(st: &AppState, now: i64) -> bool {
    if !auto_check(st) {
        return false;
    }
    let due = last_check(st).is_none_or(|l| {
        let wait = if l.problem.is_some() { RETRY_AFTER } else { CHECK_EVERY };
        l.at > now || now - l.at >= wait.as_millis() as i64
    });
    if due {
        check(st).await;
    }
    due
}

/// Update to `version`, the newer release the check found: get its source, build it, back up your data and install it,
/// in the background. Returns at once; the steps show in `status` (the UI hears of each one). An update that runs
/// already is left to run.
pub fn start(st: &AppState, version: &str) -> Result<UpdateStatus, String> {
    let now = status(st);
    let release = now.available.clone().filter(|r| r.version == version)
        .ok_or_else(|| format!("Gizai knows of no newer release {version}. Check for updates first."))?;
    let (prefix, repo, stop) = {
        let mut inner = st.updates.lock();
        if inner.job.as_ref().is_some_and(|j| j.status.running()) {
            drop(inner);
            return Ok(status(st));
        }
        let prefix = install_prefix(&inner)?;
        let stop = Stop::default();
        let log = st.data_dir.join("update").join("update.log");
        inner.job = Some(Job {
            status: UpdateJob { version: release.version.clone(), step: "source".into(), started_at: ids::now_ms(), ended_at: None,
                                log: log.display().to_string(), backup: None, problem: None, output: None, unchanged: true },
            stop: stop.clone(),
        });
        (prefix, inner.source.repo.clone(), stop)
    };
    changed(st);
    let st2 = st.clone();
    std::thread::spawn(move || {
        let ended = run(&st2, &release, &repo, &prefix, &stop);
        finish(&st2, ended);
    });
    Ok(status(st))
}

/// How an update ended well: the backup it made, and what the installer said went wrong after the new version was in
/// place, if anything.
struct Done {
    backup: String,
    note: Option<String>,
}

/// How an update didn't work.
struct Failed {
    failed: UpdateFailed,
    /// Stopped (Stop, or Gizai quit) while it got the source or built.
    stopped: bool,
    /// The Gizai installed before is still in place, as it was.
    unchanged: bool,
}

impl From<UpdateFailed> for Failed {
    fn from(failed: UpdateFailed) -> Failed {
        Failed { failed, stopped: false, unchanged: true }
    }
}

/// The update's steps, one after the other; the first that fails ends it.
fn run(st: &AppState, release: &Release, repo: &str, prefix: &std::path::Path, stop: &Stop) -> Result<Done, Failed> {
    let dir = st.data_dir.join("update");
    let log = up::Log::create(&dir.join("update.log"))
        .map_err(|e| UpdateFailed::plain(format!("Can't write the update's log in {}: {e}", dir.display())))?;
    // a data folder given as a relative path works too: the installer runs in the source folder
    let source = dir.canonicalize().unwrap_or(dir).join("source");
    let version = &release.version;
    log.say(&format!("== Updating Gizai {VERSION} to {version}: the release {} from {repo}, installed into {}", release.tag, prefix.display()));
    let path = crate::runs::command_path();
    let built = (|| {
        up::get_source(repo, &release.tag, &source, &log, stop)?;
        match up::source_version(&source) {
            Some(v) if &v == version => {}
            said => {
                return Err(UpdateFailed::plain(format!("The source of {} says it is version {}, not {version}, so Gizai doesn't install it",
                                                       release.tag, said.as_deref().unwrap_or("unknown"))));
            }
        }
        step(st, "build", None);
        log.say(&format!("== Building {version}"));
        up::build(&source, Some(&path), &log, stop)?;
        if stop.asked() {
            return Err(UpdateFailed::plain("Stopped"));
        }
        Ok(())
    })();
    if let Err(failed) = built {
        return Err(Failed { failed, stopped: stop.asked(), unchanged: true });
    }
    // from here on Stop doesn't apply: the backup and the install take seconds
    step(st, "backup", None);
    log.say("== Backing up your data");
    let backup = crate::backup_data_dir(&st.data_dir, "before-update")
        .map_err(|e| UpdateFailed::plain(format!("Couldn't back up your data ({e}), so nothing was installed")))?
        .display().to_string();
    log.say(&format!("Backed up your data to {backup}"));
    step(st, "install", Some(&backup));
    log.say(&format!("== Installing {version} into {}", prefix.display()));
    let installed = up::install(&source, prefix, Some(&path), &log, &Stop::default());
    // what is in place now decides, whatever the installer said
    let now = up::installed_version(prefix);
    match (installed, now) {
        (installed, Some(v)) if &v == version => {
            let note = installed.err().map(|f| format!("{version} is installed, but the installer said: {}", f.what));
            log.say(&format!("== Installed {version}. Restart Gizai to use it."));
            Ok(Done { backup, note })
        }
        (Ok(()), said) => Err(Failed {
            failed: UpdateFailed::plain(format!("The installer finished, but the installed Gizai says it is version {}",
                                                said.as_deref().unwrap_or("unknown"))),
            stopped: false,
            unchanged: said.as_deref() == Some(VERSION),
        }),
        (Err(failed), said) => Err(Failed { failed, stopped: false, unchanged: said.as_deref() == Some(VERSION) }),
    }
}

/// The update moved on to `name`.
fn step(st: &AppState, name: &str, backup: Option<&str>) {
    if let Some(job) = st.updates.lock().job.as_mut() {
        job.status.step = name.into();
        if let Some(b) = backup {
            job.status.backup = Some(b.into());
        }
    }
    changed(st);
}

fn finish(st: &AppState, ended: Result<Done, Failed>) {
    {
        let mut inner = st.updates.lock();
        let mut installed = None;
        if let Some(job) = inner.job.as_mut() {
            let s = &mut job.status;
            s.ended_at = Some(ids::now_ms());
            match ended {
                Ok(done) => {
                    s.step = "installed".into();
                    s.backup = Some(done.backup);
                    s.problem = done.note;
                    s.unchanged = false;
                    installed = Some(s.version.clone());
                }
                Err(Failed { failed, stopped, unchanged }) => {
                    let said = if stopped { "Stopped".to_string() } else { failed.what.clone() };
                    if let Ok(mut log) = std::fs::OpenOptions::new().append(true).open(&s.log) {
                        let _ = writeln!(log, "== {said}");
                    }
                    s.step = if stopped { "stopped" } else { "failed" }.into();
                    s.problem = (!stopped).then_some(failed.what);
                    s.output = Some(failed.output).filter(|o| !o.trim().is_empty());
                    s.unchanged = unchanged;
                }
            }
        }
        if installed.is_some() {
            inner.installed = installed;
        }
    }
    changed(st);
}

/// Stops the update while it gets the source or builds; the installed Gizai stays as it is. The backup and the install
/// take seconds, and always finish.
pub fn stop(st: &AppState) -> UpdateStatus {
    if let Some(job) = st.updates.lock().job.as_ref().filter(|j| matches!(j.status.step.as_str(), "source" | "build")) {
        job.stop.stop();
    }
    status(st)
}

/// When Gizai quits: an update that gets its source or builds stops with it (the source and build so far are kept).
pub fn on_exit(st: &AppState) {
    stop(st);
}

/// The restart after an update: a small shell in a session of its own waits until this Gizai has quit (at most ten
/// minutes), then starts the installed one with its launcher. The caller quits Gizai the usual way next, which stops
/// agents at work first.
pub fn restart_command(st: &AppState) -> Result<std::process::Command, String> {
    let inner = st.updates.lock();
    let ready = inner.job.as_ref().is_some_and(|j| j.status.step == "installed")
        || inner.installed.as_deref().is_some_and(|v| up::is_newer(v, VERSION));
    if !ready {
        return Err("No newer Gizai is installed yet: update first".into());
    }
    let lib = install_prefix(&inner)?.join("lib/gizai");
    let launcher = [lib.join("gizai-launch"), lib.join("gizai")].into_iter().find(|p| crate::runs::executable(p))
        .ok_or_else(|| format!("No Gizai is installed in {}", lib.display()))?;
    // gone: no /proc entry, or a zombie its parent hasn't collected yet
    const WAIT_THEN_START: &str = r#"i=0
while [ "$i" -lt 3000 ]; do
  s="$(cat "/proc/$1/stat" 2>/dev/null)" || break
  case "${s##*) }" in Z*|X*) break ;; esac
  sleep 0.2; i=$((i + 1))
done
[ "$i" -lt 3000 ] && exec "$2""#;
    let mut cmd = std::process::Command::new("sh");
    cmd.args(["-c", WAIT_THEN_START, "gizai-restart"]).arg(std::process::id().to_string()).arg(&launcher)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    // SAFETY: setsid(2) is a plain syscall, safe to make between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    Ok(cmd)
}
