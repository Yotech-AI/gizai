//! The worktrees of finished cards (Done or Cancelled). Settings → Data lists them with their disk use and removes them
//! (the same removal as after a merged pull request: `worktree::remove_card_worktree`), and a new card of the same
//! project takes one over, so its build stays warm (`runs`).
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gizai_agents::worktree;
use gizai_core::model::Project;
use gizai_core::worktrees::{self as core_worktrees, FinishedCard};
use serde::Serialize;

use crate::AppState;
use crate::runs::Note;

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

/// The worktrees a new card of `project_id` can take over, the most recently finished card's first: a Done or
/// Cancelled card's, in the project's part of Gizai's worktree folder (`base_dir`), with no agent working in it and
/// nothing uncommitted. A card that isn't finished never gives its worktree.
pub(crate) fn reusable(st: &AppState, project_id: &str, base_dir: &Path) -> Vec<PathBuf> {
    let Ok(cards) = core_worktrees::finished(&st.db, Some(project_id)) else { return vec![] };
    let live = live_cards(st);
    with_worktree(st, cards).into_iter()
        .filter(|(c, wt)| !live.contains(&c.task_id) && worktree::is_inside(wt, base_dir) && worktree::uncommitted(wt).is_ok_and(|n| n == 0))
        .map(|(_, wt)| wt)
        .collect()
}

/// Settings → Data: the worktrees of Done and Cancelled cards with their disk use, the most recently finished first.
/// Counting walks every folder, so call it off the async threads.
pub fn list(st: &AppState) -> Result<Vec<OldWorktree>, String> {
    let cards = core_worktrees::finished(&st.db, None).map_err(|e| e.to_string())?;
    let live = live_cards(st);
    Ok(with_worktree(st, cards).into_iter().map(|(c, wt)| OldWorktree {
        bytes: disk_use(&wt), uncommitted: worktree::uncommitted(&wt).unwrap_or(0), live: live.contains(&c.task_id),
        path: wt.display().to_string(), task_id: c.task_id, identifier: c.identifier, title: c.title, category: c.category,
        project_name: c.project_name, branch: c.branch,
    }).collect())
}

/// Removes the worktrees of these finished cards (Settings → Data, after you confirmed), never by force: one with
/// uncommitted changes stays, and so does one an agent is working in. A card's branch is deleted only when the
/// project's main branch has all its commits. Each card's activity says what happened.
pub fn remove(st: &AppState, task_ids: &[String]) -> Result<Vec<RemovedWorktree>, String> {
    let wanted: HashSet<&str> = task_ids.iter().map(String::as_str).collect();
    let cards: Vec<FinishedCard> = core_worktrees::finished(&st.db, None).map_err(|e| e.to_string())?
        .into_iter().filter(|c| wanted.contains(c.task_id.as_str())).collect();
    let live = live_cards(st);
    let dir = st.data_dir.join("worktrees");
    let mut out = vec![];
    for (c, _) in with_worktree(st, cards) {
        if live.contains(&c.task_id) {
            out.push(RemovedWorktree { task_id: c.task_id, identifier: c.identifier, removed: false,
                                       note: "kept its worktree: an agent is working in it".into() });
            continue;
        }
        let repo = Path::new(&c.repo_path);
        let merged = worktree::merged(repo, &c.branch, &c.default_branch);
        let removal = worktree::remove_card_worktree(repo, &dir, &c.branch, merged);
        let mut said = removal.phrases(&c.branch);
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

/// "a, b and c".
fn sentence(parts: &[String]) -> String {
    match parts.len() {
        0 => String::new(),
        1 => parts[0].clone(),
        n => format!("{} and {}", parts[..n - 1].join(", "), parts[n - 1]),
    }
}

/// The disk space the files in `dir` take, as du counts it (their blocks), without following symlinks.
pub fn disk_use(dir: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let mut total = 0;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            total += m.blocks() * 512;
            if m.is_dir() {
                todo.push(e.path());
            }
        }
    }
    total
}
