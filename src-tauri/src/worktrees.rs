//! The worktrees of finished cards (Done or Cancelled). Settings → Data lists them with their disk use and uncommitted
//! files, and removes them (the same removal as after a merged pull request: `worktree::remove_card_worktree`), and a
//! new card of the same project takes one over, so its build stays warm (`take_over`).
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gizai_agents::worktree::{self, UncommittedFile};
use gizai_core::model::{Project, Task};
use gizai_core::worktrees::{self as core_worktrees, FinishedCard};
use serde::Serialize;

use crate::AppState;
use crate::runs::Note;

/// Above this disk use a taken-over worktree's build folder (target/) is emptied, and the card starts with a cold build
/// (`take_over_plan`): 20 GB.
pub const BUILD_LIMIT: u64 = 20 * 1024 * 1024 * 1024;

/// How many of a worktree's uncommitted files Settings → Data lists (`OldWorktree::changes`); it says how many more.
pub const SHOWN_CHANGES: usize = 10;

/// A finished card's worktree, as Settings → Data shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldWorktree {
    pub task_id: String,
    pub identifier: String,
    pub title: String,
    /// done or cancelled
    pub category: String,
    pub project_name: String,
    pub branch: String,
    pub path: String,
    /// The disk space its files take, in bytes, as du counts it.
    pub bytes: u64,
    /// Files with uncommitted changes, untracked ones included: git keeps such a worktree when asked to remove it.
    pub uncommitted: usize,
    /// The first of those files (at most `SHOWN_CHANGES`), in git status's order: each one's path and whether it is new,
    /// changed or deleted.
    pub changes: Vec<UncommittedFile>,
    /// An agent is working in it right now.
    pub live: bool,
}

/// What removing one card's worktree did.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedWorktree {
    pub task_id: String,
    pub identifier: String,
    pub removed: bool,
    /// What happened ("removed its worktree and deleted branch gizai/gz-1-x"), or why it stayed.
    pub note: String,
}

/// What a taken-over worktree keeps of the finished card (all other untracked and ignored files go): the project's
/// copied paths, and the dependency and build folders Gizai knows (vendor/, node_modules/, target/).
pub fn keep_on_reuse(project: &Project) -> Vec<String> {
    let mut keep = project.worktree_copy.clone();
    for k in ["vendor/", "node_modules/", "target/"] {
        if !keep.iter().any(|p| p.trim_end_matches('/') == k.trim_end_matches('/')) {
            keep.push(k.to_string());
        }
    }
    keep
}

/// What a card keeps when it takes over a finished card's worktree (`take_over_plan`).
#[derive(Debug, Clone, PartialEq)]
pub struct TakeOver {
    /// The paths it keeps, relative to the worktree (`keep_on_reuse`, without target/ when that is emptied).
    pub keep: Vec<String>,
    /// The disk use of the build folder (target/) it empties, in bytes: set when that is over the limit.
    pub emptied: Option<u64>,
}

/// What a card keeps when it takes over the finished card's worktree at `from`: `keep_on_reuse`'s paths, its build
/// folder target/ only while that takes at most `limit` bytes (`BUILD_LIMIT`; tests give a small one). A build folder
/// grows card after card, so above the limit target/ goes with the rest (`worktree::reuse`'s `git clean -dx`) and the
/// card starts with a cold build. A target/ that is a link stays: emptying it would free nothing here.
pub fn take_over_plan(project: &Project, from: &Path, limit: u64) -> TakeOver {
    let keep = keep_on_reuse(project);
    let target = from.join("target");
    let bytes = if std::fs::symlink_metadata(&target).is_ok_and(|m| m.is_dir()) { disk_use(&target) } else { 0 };
    if bytes <= limit {
        return TakeOver { keep, emptied: None };
    }
    // as `worktree::reuse` reads the paths: "target", "./target" and "/target/" are all the build folder, and a folder in
    // it (a copied "target/debug") would keep most of it
    let keep = keep.into_iter().filter(|k| {
        let k = k.trim().trim_start_matches("./").trim_matches('/');
        k != "target" && !k.starts_with("target/")
    }).collect();
    TakeOver { keep, emptied: Some(bytes) }
}

/// A card without a worktree (`task`) takes over the worktree of a finished card of its project, so its build stays
/// warm: the first of `reusable`'s that `worktree::reuse` can move to `base_dir` (the project's part of Gizai's worktree
/// folder) and switch to the card's branch, made from `start` when it has none yet. A build folder over `limit`
/// (`BUILD_LIMIT`) is emptied (`take_over_plan`), and the card's activity says so. None when there is none to take over.
/// Measuring the build folder walks it, so call it off the async threads.
pub fn take_over(st: &AppState, project: &Project, task: &Task, repo: &Path, base_dir: &Path, start: &str, limit: u64)
    -> Option<worktree::Worktree> {
    for (card, from) in reusable(st, &project.id, base_dir) {
        let plan = take_over_plan(project, &from, limit);
        match worktree::reuse(repo, &from, base_dir, &task.identifier, &task.title, start, &plan.keep) {
            Ok(wt) => {
                // GA-210: the finished card's runs' temp folder is named after its old place; the new card gets its own
                if let Err(e) = worktree::remove_run_temp(&st.temp_base, &from) {
                    eprintln!("gizai: couldn't remove the temp folder of {}'s worktree: {e}", card.identifier);
                }
                if let Some(bytes) = plan.emptied {
                    let said = format!("took over {}'s worktree and emptied its build folder (target/, {}): it was over {}",
                                       card.identifier, size(bytes), size(limit));
                    let _ = gizai_core::pulls::note_cleanup(&st.db, &task.id, &said);
                    (st.notify)(Note::RowsChanged("tasks"));
                }
                return Some(wt);
            }
            Err(e) => eprintln!("gizai: {} couldn't take over the worktree {}: {e}", task.identifier, from.display()),
        }
    }
    None
}

/// The finished cards whose branch is checked out in a worktree inside Gizai's worktree folder, with that worktree.
fn with_worktree(st: &AppState, cards: Vec<FinishedCard>) -> Vec<(FinishedCard, PathBuf)> {
    let dir = st.data_dir.join("worktrees");
    let mut by_repo: HashMap<String, HashMap<String, PathBuf>> = HashMap::new();
    cards.into_iter().filter_map(|c| {
        let branches = by_repo.entry(c.repo_path.clone())
            .or_insert_with(|| worktree::checked_out(Path::new(&c.repo_path)).unwrap_or_default());
        let wt = branches.get(&c.branch)?.clone();
        (wt.is_dir() && worktree::is_inside(&wt, &dir)).then_some((c, wt))
    }).collect()
}

fn live_cards(st: &AppState) -> HashSet<String> {
    crate::runs::live(st).into_iter().map(|r| r.task_id).collect()
}

/// The worktrees a new card of `project_id` can take over, with their cards, the most recently finished card's first: a
/// Done or Cancelled card's, in the project's part of Gizai's worktree folder (`base_dir`), with no agent working in it
/// and nothing uncommitted. A card that isn't finished never gives its worktree.
fn reusable(st: &AppState, project_id: &str, base_dir: &Path) -> Vec<(FinishedCard, PathBuf)> {
    let Ok(cards) = core_worktrees::finished(&st.db, Some(project_id)) else { return vec![] };
    let live = live_cards(st);
    with_worktree(st, cards).into_iter()
        .filter(|(c, wt)| !live.contains(&c.task_id) && worktree::is_inside(wt, base_dir) && worktree::uncommitted(wt).is_ok_and(|n| n == 0))
        .collect()
}

/// Settings → Data: the worktrees of Done and Cancelled cards with their disk use and uncommitted files, the most
/// recently finished first. Counting walks every folder, so call it off the async threads.
pub fn list(st: &AppState) -> Result<Vec<OldWorktree>, String> {
    let cards = core_worktrees::finished(&st.db, None).map_err(|e| e.to_string())?;
    let live = live_cards(st);
    Ok(with_worktree(st, cards).into_iter().map(|(c, wt)| {
        let files = worktree::uncommitted_files(&wt).unwrap_or_default();
        OldWorktree {
            bytes: disk_use(&wt), uncommitted: files.len(), changes: files.into_iter().take(SHOWN_CHANGES).collect(),
            live: live.contains(&c.task_id), path: wt.display().to_string(), task_id: c.task_id, identifier: c.identifier,
            title: c.title, category: c.category, project_name: c.project_name, branch: c.branch,
        }
    }).collect())
}

/// Removes the worktrees of these finished cards (Settings → Data, after you confirmed; Remove all too), never by force:
/// one with uncommitted changes stays, and so does one an agent is working in. A card's branch is deleted only when the
/// project's main branch has all its commits. Each card's activity says what happened.
pub fn remove(st: &AppState, task_ids: &[String]) -> Result<Vec<RemovedWorktree>, String> {
    remove_cards(st, task_ids, false)
}

/// Remove anyway (Settings → Data, after you confirmed), for a finished card's worktree with uncommitted changes: they go
/// into git's stash first, as "<identifier> leftover, removed in Settings" (`worktree::stash_all`), so nothing is lost.
/// Then the worktree is removed as `remove` does it, its branch only when main has all its commits. When the stash
/// fails, nothing is removed and the note says why. One an agent is working in stays.
pub fn remove_anyway(st: &AppState, task_id: &str) -> Result<RemovedWorktree, String> {
    let mut out = remove_cards(st, &[task_id.to_string()], true)?;
    out.pop().ok_or_else(|| format!("{task_id} has no worktree of a Done or Cancelled card"))
}

/// `remove`, with `stash`: Remove anyway's stash of uncommitted changes first.
fn remove_cards(st: &AppState, task_ids: &[String], stash: bool) -> Result<Vec<RemovedWorktree>, String> {
    let wanted: HashSet<&str> = task_ids.iter().map(String::as_str).collect();
    let cards: Vec<FinishedCard> = core_worktrees::finished(&st.db, None).map_err(|e| e.to_string())?
        .into_iter().filter(|c| wanted.contains(c.task_id.as_str())).collect();
    let live = live_cards(st);
    let dir = st.data_dir.join("worktrees");
    let mut out = vec![];
    for (c, wt) in with_worktree(st, cards) {
        if live.contains(&c.task_id) {
            out.push(RemovedWorktree { task_id: c.task_id, identifier: c.identifier, removed: false,
                                       note: "kept its worktree: an agent is working in it".into() });
            continue;
        }
        let mut said = vec![];
        let changes = if stash { worktree::uncommitted(&wt).unwrap_or(0) } else { 0 };
        if changes > 0 {
            let message = stash_message(&c.identifier);
            if let Err(e) = worktree::stash_all(&wt, &message) {
                let why = match e { gizai_agents::AgentError::Git(m) => m, other => other.to_string() };
                out.push(RemovedWorktree { task_id: c.task_id, identifier: c.identifier, removed: false,
                                           note: format!("kept its worktree: git couldn't put its uncommitted {} in the stash, so \
                                                          nothing was removed ({})", plural(changes, "change"), why.trim_end_matches('.')) });
                continue;
            }
            said.push(format!("put its {changes} uncommitted {} in git's stash as \"{message}\"", plural(changes, "change")));
        }
        let repo = Path::new(&c.repo_path);
        // main here, or main as last fetched from GitHub (a run fetches it into one of these)
        let mains = [c.default_branch.clone(), format!("refs/gizai/base/{}", c.default_branch), format!("refs/remotes/origin/{}", c.default_branch)];
        let merged = mains.iter().any(|m| worktree::merged(repo, &c.branch, m));
        let removal = worktree::remove_card_worktree_with(repo, &dir, &st.temp_base, &c.branch, merged);
        said.extend(removal.phrases(&c.branch));
        if !merged && removal.kept.is_none() {
            said.push(format!("kept branch {}, which has commits {} doesn't", c.branch, c.default_branch));
        }
        let note = sentence(&said);
        let _ = gizai_core::pulls::note_cleanup(&st.db, &c.task_id, &format!("{note} in Settings → Data"));
        out.push(RemovedWorktree { task_id: c.task_id, identifier: c.identifier, removed: removal.removed.is_some(), note });
    }
    for id in task_ids {
        if !out.iter().any(|r| &r.task_id == id) {
            let identifier = gizai_core::tasks::get(&st.db, id).map(|t| t.identifier).unwrap_or_else(|_| id.clone());
            out.push(RemovedWorktree { task_id: id.clone(), identifier, removed: false,
                                       note: "has no worktree of a Done or Cancelled card".into() });
        }
    }
    (st.notify)(Note::RowsChanged("tasks"));
    Ok(out)
}

/// The message Remove anyway gives the stash entry of a card's uncommitted changes, as `git stash list` shows it.
pub fn stash_message(identifier: &str) -> String {
    format!("{identifier} leftover, removed in Settings")
}

/// "1 change", "3 changes".
fn plural(n: usize, word: &str) -> String {
    if n == 1 { word.to_string() } else { format!("{word}s") }
}

/// "a, b and c".
fn sentence(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

/// A disk use as people read it, with at most one decimal: "23.4 GB", "20 GB", "512 KB".
pub fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let (mut v, mut unit) = (bytes as f64 / 1024.0, 0);
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    let shown = format!("{v:.1}");
    format!("{} {}", shown.strip_suffix(".0").unwrap_or(&shown), UNITS[unit])
}

/// The disk space the files in `dir` take, as du counts it (their blocks), without following symlinks. On Windows, which
/// has no block count here, the sum of the files' sizes.
pub fn disk_use(dir: &Path) -> u64 {
    let mut total = 0;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            total += used(&m);
            if m.is_dir() {
                todo.push(e.path());
            }
        }
    }
    total
}

/// What one entry takes on disk: its blocks.
#[cfg(unix)]
fn used(m: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.blocks() * 512
}

/// What one entry takes on disk: its size (a folder counts 0).
#[cfg(windows)]
fn used(m: &std::fs::Metadata) -> u64 {
    if m.is_dir() { 0 } else { m.len() }
}
