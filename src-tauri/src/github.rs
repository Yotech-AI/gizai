//! Settings → GitHub: whether Gizai can use GitHub and how it pushes.
//! - The status: the GitHub CLI (found, its version), the account it is logged in as, and Push over (SSH with your
//!   keys, the default, or HTTPS with gh's login).
//! - Check connection: gh, its login, ssh to git@github.com, and for each project with a GitHub link whether you can
//!   push to it (a dry run). Each check says what to do when it fails.
//! - Log in with GitHub: gh's own login in the browser (`gh auth login --web`); the app shows its one-time code and
//!   link, and the status follows once you have entered the code on GitHub.
//!
//! Gizai never stores a token or password: it uses gh's login and your SSH keys.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gizai_agents::connection::{self, GhLogin, Problem, PushOver};
use gizai_agents::worktree;
use gizai_core::model::Project;
use gizai_core::{projects, repo_url, settings};
use serde::Serialize;

use crate::AppState;

/// The setting that says how pushes go.
const PUSH_OVER: &str = "github_push_over";

/// The ways a push can go, as Settings keeps them.
pub const PUSH_OVER_NAMES: [&str; 2] = ["ssh", "https"];

/// How long one check may take (gh, ssh, a dry-run push).
const CHECK_LIMIT: Duration = Duration::from_secs(30);
/// How long gh may take to show its one-time code…
const CODE_LIMIT: Duration = Duration::from_secs(30);
/// …and how long the code may wait to be entered (GitHub's codes last 15 minutes).
const LOGIN_LIMIT: Duration = Duration::from_secs(16 * 60);

/// Settings → GitHub → Push over: "ssh" (the default) or "https".
pub fn push_over_name(st: &AppState) -> String {
    match settings::get::<String>(&st.db, PUSH_OVER).ok().flatten() {
        Some(name) if PUSH_OVER_NAMES.contains(&name.as_str()) => name,
        _ => "ssh".into(),
    }
}

pub(crate) fn save_push_over(st: &AppState, name: &str) -> Result<(), String> {
    if !PUSH_OVER_NAMES.contains(&name) {
        return Err(format!("pushes go over ssh or https, not {name:?}"));
    }
    settings::set(&st.db, PUSH_OVER, &name).map_err(|e| e.to_string())
}

/// How a push goes now; over HTTPS with the login of `gh` (the GitHub CLI Gizai uses).
pub(crate) fn push_over(st: &AppState, gh: &Path) -> PushOver {
    match push_over_name(st).as_str() {
        "https" => PushOver::Https { gh: gh.to_path_buf() },
        _ => PushOver::Ssh,
    }
}

/// The GitHub CLI Gizai uses (see `pulls::gh_bin`), or why there is none, with where to get it.
fn gh(st: &AppState) -> Result<PathBuf, Problem> {
    crate::pulls::gh_bin(st).map_err(|_| match settings::get::<String>(&st.db, "gh_bin").ok().flatten() {
        Some(saved) => Problem::new(format!("Not found at {saved}"), "Install the GitHub CLI from cli.github.com, or fix its path."),
        None => Problem::new("Not found", "Install the GitHub CLI from cli.github.com, or set its path."),
    })
}

/// What Settings → GitHub shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The GitHub CLI Gizai uses, when it is there.
    pub gh_path: Option<String>,
    /// Its version ("2.62.0").
    pub gh_version: Option<String>,
    /// Why gh can't be used (not found, won't run), with what to do.
    pub gh_problem: Option<Problem>,
    /// The account gh is logged in as on github.com.
    pub account: Option<String>,
    /// Why there is none (not logged in, a login GitHub refuses), with what to do. None while gh itself is missing.
    pub account_problem: Option<Problem>,
    /// How pushes go: "ssh" or "https".
    pub push_over: String,
    /// The login in the browser that waits for its code to be entered (Log in with GitHub), if one does.
    pub login: Option<LoginCode>,
    /// The command that logs gh in from a terminal (when gh is there).
    pub login_command: Option<String>,
}

/// Whether Gizai can use GitHub now: gh, its version and login (gh asks GitHub), and how pushes go.
pub async fn status(st: &AppState) -> Status {
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || status_blocking(&st2)).await.unwrap_or_else(|e| Status {
        gh_problem: Some(Problem::plain(e.to_string())), push_over: push_over_name(st), ..Default::default()
    })
}

fn status_blocking(st: &AppState) -> Status {
    let mut out = Status { push_over: push_over_name(st), login: login_now(st), ..Default::default() };
    let gh = match gh(st) {
        Ok(gh) => gh,
        Err(p) => {
            out.gh_problem = Some(p);
            return out;
        }
    };
    out.gh_path = Some(gh.display().to_string());
    out.login_command = Some(connection::login_command(&gh));
    match connection::gh_version(&gh, CHECK_LIMIT) {
        Ok(v) => out.gh_version = Some(v),
        Err(p) => {
            out.gh_problem = Some(p);
            return out;
        }
    }
    match connection::gh_account(&gh, CHECK_LIMIT) {
        Ok(who) => out.account = Some(who),
        Err(p) => out.account_problem = Some(p),
    }
    out
}

/// One line of Check connection.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    /// What was checked: "GitHub CLI", "Account", "SSH", or a project's name.
    pub name: String,
    /// "ok", "failed", or "skipped" (not needed, or it needs something that failed).
    pub result: String,
    /// The result in plain words.
    pub text: String,
    /// For a failure: what to do.
    pub fix: Option<String>,
    /// For a project: its id and repository ("owner/name" on GitHub, "workspace/repository" on Bitbucket).
    pub project_id: Option<String>,
    pub repo: Option<String>,
}

impl Check {
    pub(crate) fn ok(name: &str, text: impl Into<String>) -> Check {
        Check { name: name.into(), result: "ok".into(), text: text.into(), fix: None, project_id: None, repo: None }
    }

    pub(crate) fn failed(name: &str, p: Problem) -> Check {
        Check { name: name.into(), result: "failed".into(), text: p.what, fix: p.fix, project_id: None, repo: None }
    }

    pub(crate) fn skipped(name: &str, text: impl Into<String>) -> Check {
        Check { name: name.into(), result: "skipped".into(), text: text.into(), fix: None, project_id: None, repo: None }
    }
}

/// What Check connection found.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCheck {
    /// Whether every check that matters passed (skipped ones don't count).
    pub ok: bool,
    /// How pushes went: "ssh" or "https".
    pub push_over: String,
    pub checks: Vec<Check>,
}

/// Check connection: the GitHub CLI, the account it is logged in as, ssh to git@github.com in batch mode (when pushes
/// go over SSH), and for each project with a GitHub link whether you can push to it, the way Push branch would (a dry
/// run: nothing is sent). Never asks for anything, and changes nothing. Bitbucket's is `bitbucket::check`.
pub async fn check(st: &AppState) -> ConnectionCheck {
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || check_blocking(&st2)).await.unwrap_or_else(|e| ConnectionCheck {
        ok: false, push_over: push_over_name(st), checks: vec![Check::failed("Check connection", Problem::plain(e.to_string()))],
    })
}

fn check_blocking(st: &AppState) -> ConnectionCheck {
    let over_name = push_over_name(st);
    let mut checks = vec![];
    let gh = gh(st).and_then(|gh| connection::gh_version(&gh, CHECK_LIMIT).map(|v| (gh, v)));
    let account = match &gh {
        Ok((path, version)) => {
            checks.push(Check::ok("GitHub CLI", format!("gh {version} at {}", path.display())));
            match connection::gh_account(path, CHECK_LIMIT) {
                Ok(who) => {
                    checks.push(Check::ok("Account", format!("Logged in to GitHub as {who}")));
                    Some(who)
                }
                Err(p) => {
                    checks.push(Check::failed("Account", p));
                    None
                }
            }
        }
        Err(p) => {
            checks.push(Check::failed("GitHub CLI", p.clone()));
            checks.push(Check::skipped("Account", "Needs the GitHub CLI"));
            None
        }
    };
    let over = match (over_name.as_str(), &gh) {
        ("https", Ok((path, _))) => Some(PushOver::Https { gh: path.clone() }),
        ("https", Err(_)) => None,
        _ => Some(PushOver::Ssh),
    };
    if over == Some(PushOver::Ssh) {
        checks.push(match connection::ssh_check(CHECK_LIMIT) {
            Ok(who) => {
                let gh_too = account.as_deref().filter(|a| !a.eq_ignore_ascii_case(&who)).map(|a| format!(" (gh is logged in as {a})")).unwrap_or_default();
                Check::ok("SSH", format!("git@github.com accepts your SSH key, as {who}{gh_too}"))
            }
            Err(p) => Check::failed("SSH", p),
        });
    } else {
        checks.push(Check::skipped("SSH", "Not used: pushes go over HTTPS with gh's login"));
    }
    checks.extend(project_checks(st, "github", over.as_ref()));
    ConnectionCheck { ok: !checks.iter().any(|c| c.result == "failed"), push_over: over_name, checks }
}

/// How many projects are checked at the same time (GitHub and Bitbucket may drop a burst of SSH connections).
const PROJECTS_AT_ONCE: usize = 4;

/// For each project linked to `provider` ("github" or "bitbucket"; archived ones aside): whether you can push to it, a
/// few at a time.
pub(crate) fn project_checks(st: &AppState, provider: &str, over: Option<&PushOver>) -> Vec<Check> {
    let mut linked: Vec<(Project, String, String)> = projects::list(&st.db).unwrap_or_default().into_iter()
        .filter(|p| p.status != "archived")
        .filter_map(|p| {
            let link = repo_url::normalize(p.repo_url.as_deref()?).ok().flatten().filter(|l| l.provider == provider)?;
            let repo = link.full_name()?;
            Some((p, link.url, repo))
        })
        .collect();
    let mut out = vec![];
    while !linked.is_empty() {
        let batch: Vec<_> = linked.drain(..linked.len().min(PROJECTS_AT_ONCE)).map(|(p, link, repo)| {
            let over = over.cloned();
            std::thread::spawn(move || {
                let mut c = match project_check(&p, &link, over.as_ref()) {
                    Ok(()) => Check::ok(&p.name, format!("You can push to {repo}")),
                    Err(problem) => Check::failed(&p.name, problem),
                };
                (c.project_id, c.repo) = (Some(p.id), Some(repo));
                c
            })
        }).collect();
        out.extend(batch.into_iter().filter_map(|t| t.join().ok()));
    }
    out
}

fn project_check(p: &Project, link: &str, over: Option<&PushOver>) -> Result<(), Problem> {
    let Some(over) = over else {
        return Err(Problem::new("Needs the GitHub CLI to push over HTTPS", "Install the GitHub CLI, or push over SSH."));
    };
    let folder = p.repo_path.as_deref().map(str::trim).filter(|r| !r.is_empty());
    let Some(folder) = folder.map(Path::new).filter(|f| crate::git::is_repo(f)) else {
        return Err(Problem::new("No local git repository", "Set the project's git repository on its page (Edit)."));
    };
    // the same place Push branch pushes to
    let to = crate::git::remote_for(folder, link).unwrap_or_else(|| link.to_string());
    worktree::can_push(folder, &to, over, CHECK_LIMIT)
}

/// Log in with GitHub, while gh waits for its one-time code to be entered.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCode {
    /// The one-time code to enter on GitHub.
    pub code: String,
    /// Where to enter it.
    pub url: String,
}

/// gh's login in the browser: one at a time.
#[derive(Default)]
pub struct Logins {
    /// The login that waits for its code, and its stop switch.
    now: Mutex<Option<(LoginCode, Arc<AtomicBool>)>>,
    /// How the last one ended: the account gh logged in as, or why not.
    last: Mutex<Option<Result<Option<String>, Problem>>>,
    done: tokio::sync::Notify,
}

fn login_now(st: &AppState) -> Option<LoginCode> {
    st.github.now.lock().unwrap().as_ref().map(|(code, _)| code.clone())
}

/// Log in with GitHub: starts gh's own login in the browser and returns its one-time code and link once gh shows
/// them; gh keeps waiting in the background until the code is entered on GitHub (`login_wait`). A login that is
/// already waiting is returned as it is. When gh can't log in this way, the error says why and the command to run in
/// a terminal instead.
pub async fn login(st: &AppState) -> Result<LoginCode, String> {
    if let Some(code) = login_now(st) {
        return Ok(code);
    }
    let gh = gh(st).map_err(|_| "Log in with GitHub needs the GitHub CLI (gh): install it from cli.github.com, or set its path in Settings → GitHub".to_string())?;
    let started = tokio::task::spawn_blocking(move || GhLogin::start(&gh, CODE_LIMIT)).await.map_err(|e| e.to_string())?;
    let login = started.map_err(|p| p.to_string())?;
    let code = LoginCode { code: login.code.clone(), url: login.url.clone() };
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut now = st.github.now.lock().unwrap();
        if let Some((running, _)) = now.as_ref() {
            // another call started one meanwhile: keep that one
            let running = running.clone();
            drop(now);
            stop.store(true, Ordering::Relaxed);
            std::thread::spawn(move || { let _ = login.wait(Duration::ZERO, &stop); });
            return Ok(running);
        }
        *now = Some((code.clone(), stop.clone()));
        *st.github.last.lock().unwrap() = None;
    }
    // a thread, not a blocking task: a login nobody finishes must never hold up the runtime
    let st2 = st.clone();
    std::thread::spawn(move || {
        let ended = login.wait(LOGIN_LIMIT, &stop);
        *st2.github.last.lock().unwrap() = Some(ended);
        *st2.github.now.lock().unwrap() = None;
        st2.github.done.notify_waiters();
    });
    Ok(code)
}

/// Waits until the login in the browser has ended, and returns how: the account gh logged in as, or why it didn't.
/// None when no login was started.
pub async fn login_wait(st: &AppState) -> Option<Result<Option<String>, Problem>> {
    loop {
        let ended = st.github.done.notified();
        if st.github.now.lock().unwrap().is_none() {
            return st.github.last.lock().unwrap().clone();
        }
        ended.await;
    }
}

/// Cancels the login that waits for its code (gh is ended).
pub fn login_cancel(st: &AppState) {
    if let Some((_, stop)) = st.github.now.lock().unwrap().as_ref() {
        stop.store(true, Ordering::Relaxed);
    }
}
