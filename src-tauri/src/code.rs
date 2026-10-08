//! The Team Lead's own read-only copies of the projects' code, at `<data dir>/code/<KEY>` (`gizai_agents::copies`): one
//! per active project with a git repository, at the commit a new card of it starts from (`git::start_point`).
//! - Before each chat turn every copy is refreshed, in parallel: the project is fetched (unless it was fetched less
//!   than a minute ago), and the copy moves when that commit moved (else nothing in it is touched). One refresh per
//!   project at a time. A turn waits at most 10 seconds: a slow or failed refresh leaves the last good copy, and the
//!   turn's prompt says so, while the refresh finishes in the background.
//! - At start-up and before each turn, the copies of projects that no longer get one (paused, archived, unlinked) or
//!   that point to another repository now are removed. Nothing outside `<data dir>/code/` is ever removed.
//! - At start-up the missing copies are made in the background, so the first answer rarely waits.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::AgentError;
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
}

impl Copies {
    fn slot(&self, key: &str) -> Arc<Slot> {
        self.slots.lock().unwrap().entry(key.to_string()).or_default().clone()
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

/// Before a chat turn: removes the copies that no longer belong, refreshes the others in parallel and waits for them at
/// most `TURN_WAIT`.
pub async fn before_turn(st: &AppState) -> ForTurn {
    let list = projects_with_code(st);
    tidy(st, &list).await;
    let deadline = tokio::time::Instant::now() + TURN_WAIT;
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
    for ((p, last, late), at) in found.iter().zip(ats.into_iter().chain(std::iter::repeat(None))) {
        let dir = dir_of(st, &p.key);
        if at.is_some() && dir.join(".git").exists() {
            out.dirs.push(dir.display().to_string());
        }
        said.push(copy_said(&p.key, at.as_ref(), last, *late));
    }
    if !said.is_empty() {
        out.line = format!("[Gizai: your copies of the code: {}.]", said.join("; "));
    }
    out
}

/// One copy in the turn's line: "GA 319bf0b (2026-10-07)", and why it couldn't be refreshed or made.
fn copy_said(key: &str, at: Option<&At>, last: &Last, late: bool) -> String {
    let why = if late {
        Some(format!("not refreshed: still at it after {} s", TURN_WAIT.as_secs()))
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
    let last = s.trim().lines().map(str::trim).filter(|l| !l.is_empty()).next_back().unwrap_or("unknown error");
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
        Some(t) if t.elapsed() < FETCH_EVERY => None,
        _ if p.repo_url.is_some() => Some(FETCH_LIMIT),
        _ => None,
    };
    let dir = dir_of(&st, &p.key);
    match tokio::task::spawn_blocking(move || refresh_blocking(&p, &dir, fetch)).await {
        Ok((fetch_failed, failed)) => {
            if fetch.is_some() {
                last.fetched = Some(Instant::now());
                last.fetch_failed = fetch_failed;
            }
            last.failed = failed;
        }
        Err(e) => last.failed = Some(e.to_string()),
    }
    *slot.last.lock().unwrap() = last.clone();
    last
}

/// Fetches when `fetch` gives a time limit, then makes or moves the copy at `dir`. Returns why the fetch failed, and
/// why the copy couldn't be made or moved. A failed fetch still moves the copy to main as last fetched.
fn refresh_blocking(p: &Project, dir: &Path, fetch: Option<Duration>) -> (Option<String>, Option<String>) {
    let repo = PathBuf::from(p.repo_path.clone().unwrap_or_default());
    let mut fetch_failed = None;
    if let Some(limit) = fetch
        && let Err(e) = crate::git::start_point(p, &repo, Some(limit))
    {
        fetch_failed = Some(plain(e));
    }
    let failed = crate::git::start_point(p, &repo, None)
        .and_then(|start| copies::sync(&repo, dir, &start))
        .err().map(plain);
    (fetch_failed, failed)
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
