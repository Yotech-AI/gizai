//! The Team Lead's own read-only copies of the projects' code, at `<data dir>/code/<KEY>` (`gizai_agents::copies`): one
//! per active project with a git repository, at the commit a new card of it starts from (`git::start_point`).
//! - Before each chat turn every copy is refreshed, in parallel: the project is fetched (unless it was fetched less
//!   than a minute ago), and the copy moves when that commit moved (else nothing in it is touched). One refresh per
//!   project at a time. A turn waits at most 10 seconds: a slow or failed refresh leaves the last good copy, and the
//!   turn's prompt says so, while the refresh finishes in the background.
//! - At start-up and before each turn, the copies of projects that no longer get one (paused, archived, unlinked) or
//!   that point to another repository now are removed. Nothing outside `<data dir>/code/` is ever removed.
//! - At start-up the missing copies are made in the background, so the first answer rarely waits.
//!
//! With each refresh Gizai also looks at the project's linked folder (your own checkout, where a new card's worktree
//! copies vendor/ and node_modules/ from), locally and without a fetch (`gizai_agents::checkout::status`). When its
//! dependencies are behind main, the turn's line gets a note: on a chat's first turn, and again when it changes. The
//! Team Lead then offers to update the folder, and `update_checkout` does that in the background once you said yes
//! (`start_update`); never at the same time as a card's preparation copying from the same folder (`folder_lock`).
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::AgentError;
use gizai_agents::checkout;
use gizai_agents::copies::{self, At};
use gizai_core::model::Project;
use gizai_core::projects;

use crate::AppState;

/// How long a chat turn waits for the copies.
pub const TURN_WAIT: Duration = Duration::from_secs(10);
/// A project fetched more recently than this isn't fetched again for a turn.
pub const FETCH_EVERY: Duration = Duration::from_secs(60);
/// How long a copy's fetch may take; it goes on in the background once the turn has started.
const FETCH_LIMIT: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct Copies {
    /// Project key → its copy.
    slots: Mutex<HashMap<String, Arc<Slot>>>,
    /// Chat thread → what its turns' lines have told the Team Lead.
    heard: Mutex<HashMap<String, Heard>>,
    /// Linked folder (as it is on disk) → held while it is updated, or while a card's worktree is prepared from it.
    folders: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
    /// Linked folders an update runs in.
    updating: Mutex<HashSet<PathBuf>>,
    /// `FETCH_EVERY` and `TURN_WAIT`, unless a test changed them (`set_timing`).
    timing: Mutex<Option<(Duration, Duration)>>,
}

/// What one chat has been told.
#[derive(Debug, Default)]
struct Heard {
    /// The note about each project's linked folder it got last (project key → note).
    notes: HashMap<String, String>,
    /// The ends of updates started in it, still to mention in its next turn's line.
    results: Vec<String>,
}

#[derive(Default)]
struct Slot {
    /// Held while the copy is refreshed or removed: one at a time per project (two chats can answer at once).
    busy: tokio::sync::Mutex<()>,
    last: Mutex<Last>,
}

/// What the last refresh of a copy found.
#[derive(Debug, Clone, Default)]
struct Last {
    /// When the project was last fetched, or a fetch was tried.
    fetched: Option<Instant>,
    /// Why that fetch failed.
    fetch_failed: Option<String>,
    /// Why the copy couldn't be made or moved.
    failed: Option<String>,
    /// How the project's linked folder stands: a note when its dependencies are behind main, None when they aren't;
    /// unknown (not looked at, or git failed) when outer None.
    folder: Option<Option<String>>,
}

impl Copies {
    fn slot(&self, key: &str) -> Arc<Slot> {
        self.slots.lock().unwrap().entry(key.to_string()).or_default().clone()
    }

    /// For tests: how long a fetch counts as fresh (else `FETCH_EVERY`), and how long a turn waits (else `TURN_WAIT`).
    #[doc(hidden)]
    pub fn set_timing(&self, fetch_every: Duration, turn_wait: Duration) {
        *self.timing.lock().unwrap() = Some((fetch_every, turn_wait));
    }

    fn timing(&self) -> (Duration, Duration) {
        self.timing.lock().unwrap().unwrap_or((FETCH_EVERY, TURN_WAIT))
    }
}

/// What a chat turn gets from the copies.
#[derive(Debug, Clone, Default)]
pub struct ForTurn {
    /// The copies there are, for `--add-dir`.
    pub dirs: Vec<String>,
    /// The line the turn's prompt starts with (empty: none): the commit and date each copy shows, and which couldn't
    /// be refreshed and why. It isn't saved as a chat message.
    pub line: String,
}

/// `<data dir>/code`: Gizai's folder of copies.
pub fn root(st: &AppState) -> PathBuf {
    st.data_dir.join("code")
}

/// Where the copy of the project with this key lives.
pub fn dir_of(st: &AppState, key: &str) -> PathBuf {
    root(st).join(key)
}

/// The projects the Team Lead gets a copy of: active ones with a git repository on this disk, by key.
pub fn projects_with_code(st: &AppState) -> Vec<Project> {
    let mut list: Vec<Project> = projects::list(&st.db).unwrap_or_default().into_iter()
        .filter(|p| p.status == "active")
        .filter(|p| p.repo_path.as_deref().is_some_and(|r| Path::new(r).is_dir() && Path::new(r).join(".git").exists()))
        .collect();
    list.sort_by(|a, b| a.key.cmp(&b.key));
    list
}

/// The copies there are now, of the projects that get one: the folders the Team Lead may read and attach files from.
pub fn dirs(st: &AppState) -> Vec<PathBuf> {
    projects_with_code(st).iter().map(|p| dir_of(st, &p.key)).filter(|d| d.join(".git").exists()).collect()
}

/// A card start just fetched the project: its copy needn't fetch again for a minute.
pub fn fetched(st: &AppState, key: &str) {
    let slot = st.code.slot(key);
    let mut last = slot.last.lock().unwrap();
    last.fetched = Some(Instant::now());
    last.fetch_failed = None;
}

/// At start-up: removes the copies that no longer belong, then makes or refreshes every copy (run it in the background).
pub async fn startup(st: &AppState) {
    let list = projects_with_code(st);
    tidy(st, &list).await;
    let jobs: Vec<_> = list.into_iter().map(|p| tokio::spawn(refresh(st.clone(), p))).collect();
    for j in jobs {
        let _ = j.await;
    }
}

/// Before a turn in the chat `thread_id`: removes the copies that no longer belong, refreshes the others in parallel
/// and waits for them at most `TURN_WAIT`. The line also carries the notes about linked folders this chat hasn't had
/// yet (or that changed), and the ends of updates started in it.
pub async fn before_turn(st: &AppState, thread_id: &str) -> ForTurn {
    let list = projects_with_code(st);
    tidy(st, &list).await;
    let wait = st.code.timing().1;
    let deadline = tokio::time::Instant::now() + wait;
    let jobs: Vec<_> = list.iter().map(|p| tokio::spawn(refresh(st.clone(), p.clone()))).collect();
    let mut found = vec![];
    for (p, job) in list.into_iter().zip(jobs) {
        match tokio::time::timeout_at(deadline, job).await {
            Ok(Ok(last)) => found.push((p, last, false)),
            // still running (it finishes in the background), or it panicked: the last good copy
            _ => {
                let last = st.code.slot(&p.key).last.lock().unwrap().clone();
                found.push((p, last, true));
            }
        }
    }
    let dirs: Vec<PathBuf> = found.iter().map(|(p, _, _)| dir_of(st, &p.key)).collect();
    let ats: Vec<Option<At>> = tokio::task::spawn_blocking(move || dirs.iter().map(|d| copies::at(d)).collect())
        .await.unwrap_or_default();
    let mut out = ForTurn::default();
    let mut said = vec![];
    let mut notes = vec![];
    let mut heard = st.code.heard.lock().unwrap();
    let heard = heard.entry(thread_id.to_string()).or_default();
    for ((p, last, late), at) in found.iter().zip(ats.into_iter().chain(std::iter::repeat(None))) {
        let dir = dir_of(st, &p.key);
        if at.is_some() && dir.join(".git").exists() {
            out.dirs.push(dir.display().to_string());
        }
        said.push(copy_said(&p.key, at.as_ref(), last, late.then_some(wait)));
        // a chat hears about a linked folder on its first turn and when that changes, not on every turn
        match &last.folder {
            Some(Some(note)) if heard.notes.get(&p.key) != Some(note) => {
                notes.push(note.clone());
                heard.notes.insert(p.key.clone(), note.clone());
            }
            Some(None) if heard.notes.remove(&p.key).is_some() => {
                notes.push(format!("{}'s linked folder isn't outdated any more.", p.key));
            }
            _ => {}
        }
    }
    let mut parts = vec![];
    if !said.is_empty() {
        parts.push(format!("Your copies of the code: {}.", said.join("; ")));
    }
    parts.extend(notes);
    parts.append(&mut heard.results);
    if !parts.is_empty() {
        out.line = format!("[Gizai: {}]", parts.join(" "));
    }
    out
}

/// The note about a linked folder whose dependencies are behind main, for the turn's line: the folder, its branch, how
/// many commits it is behind, which dependencies are behind, and what an update would do.
fn folder_note(key: &str, dir: &Path, default_branch: &str, s: &checkout::Status) -> String {
    let plural = |n: u32| if n == 1 { "" } else { "s" };
    let deps: Vec<String> = s.deps.iter().map(checkout::DepBehind::said).collect();
    let on = match &s.branch {
        Some(b) => format!("It is on branch {b}"),
        None => "It has no branch checked out".to_string(),
    };
    let switch = match &s.branch {
        Some(b) if b == default_branch => String::new(),
        Some(b) => format!("switch to {default_branch} ({b} stays as it is), "),
        None => format!("switch to {default_branch}, "),
    };
    let installs: Vec<&str> = s.deps.iter().map(|d| checkout::install_name(&d.folder)).collect();
    let update = match s.moves {
        Some(n) => format!("An update would {switch}move {default_branch} {n} commit{} and run {}.", plural(n), installs.join(" and ")),
        None => format!("Its {default_branch} branch has commits main doesn't (or isn't there), so it can't be updated with a fast-forward."),
    };
    format!("{key}'s linked folder {} is outdated: {}. {on}, {} commit{} behind main. {update}",
            dir.display(), deps.join(", "), s.behind, plural(s.behind))
}

/// One copy in the turn's line: "GA 319bf0b (2026-10-07)", and why it couldn't be refreshed or made. `late`: the
/// refresh was still running when the turn stopped waiting for it (after this long).
fn copy_said(key: &str, at: Option<&At>, last: &Last, late: Option<Duration>) -> String {
    let why = if let Some(waited) = late {
        Some(format!("couldn't be refreshed: still fetching after {} s", waited.as_secs()))
    } else {
        last.failed.as_deref().or(last.fetch_failed.as_deref()).map(|w| format!("couldn't be refreshed: {}", one_line(w)))
    };
    match (at, why) {
        (Some(at), None) => format!("{key} {} ({})", at.short(), at.date),
        (Some(at), Some(why)) => format!("{key} {} ({}; {why})", at.short(), at.date),
        (None, Some(why)) => format!("{key} has no copy ({why})"),
        (None, None) => format!("{key} has no copy yet"),
    }
}

/// A message for one line: its last line, at most 200 characters.
fn one_line(s: &str) -> String {
    let last = s.trim().lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("unknown error");
    match last.char_indices().nth(200) {
        Some((i, _)) => format!("{}…", &last[..i]),
        None => last.to_string(),
    }
}

/// git's message without the "git: " in front.
fn plain(e: AgentError) -> String {
    match e {
        AgentError::Git(m) => m,
        other => other.to_string(),
    }
}

/// Refreshes the copy of `p` (see the module doc) and returns what it found.
async fn refresh(st: AppState, p: Project) -> Last {
    let slot = st.code.slot(&p.key);
    let _busy = slot.busy.lock().await;
    let mut last = slot.last.lock().unwrap().clone();
    let fetch = match last.fetched {
        Some(t) if t.elapsed() < st.code.timing().0 => None,
        _ if p.repo_url.is_some() => Some(FETCH_LIMIT),
        _ => None,
    };
    let dir = dir_of(&st, &p.key);
    // a folder being updated is half-way (an install removes node_modules/ first): it is looked at again next time
    let look = p.repo_path.as_deref().map(Path::new).and_then(|r| r.canonicalize().ok())
        .is_some_and(|r| !st.code.updating.lock().unwrap().contains(&r));
    match tokio::task::spawn_blocking(move || refresh_blocking(&p, &dir, fetch, look)).await {
        Ok(r) => {
            if fetch.is_some() {
                last.fetched = Some(Instant::now());
                last.fetch_failed = r.fetch_failed;
            }
            last.failed = r.failed;
            last.folder = r.folder;
        }
        Err(e) => last.failed = Some(e.to_string()),
    }
    *slot.last.lock().unwrap() = last.clone();
    last
}

/// What `refresh_blocking` found.
struct Refreshed {
    fetch_failed: Option<String>,
    failed: Option<String>,
    folder: Option<Option<String>>,
}

/// Fetches when `fetch` gives a time limit, then makes or moves the copy at `dir`, and with `look` looks at the
/// project's linked folder (locally). A failed fetch still moves the copy to main as last fetched.
fn refresh_blocking(p: &Project, dir: &Path, fetch: Option<Duration>, look: bool) -> Refreshed {
    let repo = PathBuf::from(p.repo_path.clone().unwrap_or_default());
    let mut fetch_failed = None;
    if let Some(limit) = fetch
        && let Err(e) = crate::git::start_point(p, &repo, Some(limit))
    {
        fetch_failed = Some(plain(e));
    }
    let start = crate::git::start_point(p, &repo, None);
    let failed = start.as_ref().map_err(|e| e.to_string()).and_then(|s| copies::sync(&repo, dir, s).map_err(plain)).err();
    let folder = start.ok().filter(|_| look).and_then(|s| checkout::status(&repo, &p.default_branch, &s).ok())
        .map(|s| s.outdated().then(|| folder_note(&p.key, &repo, &p.default_branch, &s)));
    Refreshed { fetch_failed, failed, folder }
}

/// Removes the copies in `<data dir>/code/` that no longer belong: of a project that isn't in `keep` (paused, archived,
/// unlinked, its repository gone), or that isn't a copy of its project's repository (it points to another one now, or
/// making it failed half-way). A copy being refreshed right now is left for the next time.
async fn tidy(st: &AppState, keep: &[Project]) {
    let root = root(st);
    let Ok(entries) = std::fs::read_dir(&root) else { return };
    for e in entries.flatten() {
        let key = e.file_name().to_string_lossy().to_string();
        let slot = st.code.slot(&key);
        let Ok(_busy) = slot.busy.try_lock() else { continue };
        let repo = keep.iter().find(|p| p.key == key).and_then(|p| p.repo_path.clone());
        let (dir, root2) = (e.path(), root.clone());
        let removed = tokio::task::spawn_blocking(move || {
            if repo.is_some_and(|r| copies::is_checkout_of(&dir, Path::new(&r))) {
                return false;
            }
            match copies::remove(&dir, &root2) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("gizai: couldn't remove the Team Lead's copy {}: {e}", dir.display());
                    false
                }
            }
        }).await.unwrap_or(false);
        if removed {
            *slot.last.lock().unwrap() = Last::default();
        }
    }
}

/// Held while a project's linked folder `folder` is updated (`start_update`), or while a card's new worktree is prepared
/// from it (`runs`): the two wait for each other, so no card copies a half-installed node_modules/.
pub fn folder_lock(st: &AppState, folder: &Path) -> Arc<tokio::sync::Mutex<()>> {
    let key = folder.canonicalize().unwrap_or_else(|_| folder.to_path_buf());
    st.code.folders.lock().unwrap().entry(key).or_default().clone()
}

/// No longer running when dropped.
struct Updating {
    st: AppState,
    folder: PathBuf,
}

impl Drop for Updating {
    fn drop(&mut self) {
        self.st.code.updating.lock().unwrap().remove(&self.folder);
    }
}

/// `update_checkout` from the chat `thread`: checks the linked folder of `project` (`checkout::plan`) and, when nothing
/// is in the way, starts its update in the background: to main as last fetched for the copies, switching to the
/// default branch first only with `switch`, then the installs that are needed, never the setup command. Returns what
/// it will do; or why it changes nothing. When the update has ended its result is posted in the chat as a system
/// message, and the chat's next turn hears it once.
pub async fn start_update(st: &AppState, thread: &str, project: &Project, switch: bool) -> Result<String, String> {
    let folder = project.repo_path.clone().filter(|r| !r.trim().is_empty())
        .ok_or_else(|| format!("{} has no linked folder", project.key))?;
    let folder = PathBuf::from(folder);
    let key = folder.canonicalize().unwrap_or_else(|_| folder.clone());
    if !st.code.updating.lock().unwrap().insert(key.clone()) {
        return Err(format!("an update of {} is already running: nothing changed", folder.display()));
    }
    let running = Updating { st: st.clone(), folder: key };
    let (p, dir) = (project.clone(), folder.clone());
    let will = tokio::task::spawn_blocking(move || plan(&p, &dir, switch)).await.map_err(|e| e.to_string())??;
    let (st2, thread, p, dir) = (st.clone(), thread.to_string(), project.clone(), folder.clone());
    tokio::spawn(async move {
        let _running = running;
        // after a card's preparation that is copying from the folder; the folder may have changed meanwhile, so the
        // checks run again
        let lock = folder_lock(&st2, &dir);
        let _held = lock.lock().await;
        let (p2, dir2) = (p.clone(), dir.clone());
        let ended = tokio::task::spawn_blocking(move || {
            let plan = plan(&p2, &dir2, switch).map_err(Ended::Refused)?;
            let path = crate::runs::command_path();
            checkout::update(&dir2, &plan, Some(path.as_os_str())).map_err(Ended::Stopped)
        }).await.unwrap_or_else(|e| Err(Ended::Refused(e.to_string())));
        let (message, line) = update_said(&p.key, &dir, &ended);
        crate::chat::system(&st2, &thread, &message);
        st2.code.heard.lock().unwrap().entry(thread.clone()).or_default().results.push(line);
    });
    Ok(will.said())
}

/// The checks before an update of `p`'s linked folder `dir`, against main as last fetched (`git::start_point`).
fn plan(p: &Project, dir: &Path, switch: bool) -> Result<checkout::Plan, String> {
    let main = crate::git::start_point(p, dir, None).map_err(plain)?;
    checkout::plan(dir, dir, &p.default_branch, &main, switch)
}

/// Why an update didn't end well: the checks refused it when it was its turn, or a step failed.
enum Ended {
    Refused(String),
    Stopped(checkout::Stopped),
}

/// The update's end: the chat's system message (with the end of the output when it stopped), and the short sentence
/// for the next turn's line.
fn update_said(key: &str, dir: &Path, ended: &Result<checkout::Updated, Ended>) -> (String, String) {
    let folder = format!("{key}'s linked folder {}", dir.display());
    match ended {
        Ok(u) => {
            let mut did = vec![];
            if let Some(from) = &u.switched_from {
                did.push(if from.is_empty() { "switched to the default branch".to_string() } else { format!("switched from {from} to the default branch") });
            }
            did.push(if u.from == u.to {
                format!("stayed at {} (already main's commit)", &u.to[..u.to.len().min(7)])
            } else {
                format!("moved from {} to {}", &u.from[..u.from.len().min(7)], &u.to[..u.to.len().min(7)])
            });
            if u.installed.is_empty() {
                did.push("ran no installs".into());
            } else {
                did.push(format!("ran {}", u.installed.join(" and ")));
            }
            let text = format!("Updated {folder}: {}.", list(&did));
            (text.clone(), format!("The update you started ended: {text}"))
        }
        Err(Ended::Refused(why)) => {
            let text = format!("The update of {folder} didn't start: {why}.");
            (text.clone(), text)
        }
        Err(Ended::Stopped(s)) => {
            let before = if s.done.is_empty() { String::new() } else { format!(" Before that it {}.", list(&s.done)) };
            let text = format!("The update of {folder} stopped: {}.{before}", s.why);
            let output = if s.output.trim().is_empty() { String::new() } else { format!("\n\n```\n{}\n```", s.output.trim()) };
            (format!("{text}{output}"), text)
        }
    }
}

/// "a, b and c".
fn list(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}
