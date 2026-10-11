//! A card's changes (GA-106, contract C1): which files a card changed with their counts (`card_changes`), each file's
//! texts and git's hunks (`card_file_diff`), and one run's commits (`run_changes`). They come from local git
//! (`gizai_agents::changes`), in this order:
//! 1. its worktree, uncommitted changes included, against its merge base with main as last fetched (the ref card starts
//!    use), so the commits main got since the card started don't show;
//! 2. its branch, when its worktree is gone;
//! 3. the range saved at its merge (`save_at_merge`): its branch as its pull request had it, after a merge commit, a
//!    squash or a rebase alike, also once its branch is gone, offline;
//! 4. its merge commit on main against the commit's first parent (a card merged before Gizai saved ranges), found on
//!    main by its message, else asked of GitHub (gh) or Bitbucket;
//! 5. its pull request's diff from GitHub or Bitbucket, without the files' texts.
//!
//! Only 4 and 5 can ask the network, and only for a card whose own git has nothing left. Blocking work runs off the
//! async threads.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::bitbucket::{self, Login};
use gizai_agents::changes::{self as git_changes, MAX_TEXT};
pub use gizai_agents::changes::{CardChanges, ChangeSource, DiffHunk, FileChange, FileDiff, FileStatus};
use gizai_agents::github::{self, PullRequest};
use gizai_agents::worktree;
use gizai_core::model::{Project, Task};
use gizai_core::pulls::PrCard;
use gizai_core::{projects, runs as core_runs, tasks};
use tauri::State;

use crate::AppState;

/// What a card without a branch shows.
const NO_BRANCH: &str = "No changes yet: the first run makes this card's branch.";

/// A card's changed files with their counts: its worktree (uncommitted changes included) against its merge base with
/// main, else its branch, else the range saved at its merge, else its merge commit on main, else the pull request's
/// diff. Each file's texts and hunks come from `card_file_diff`.
#[tauri::command]
pub async fn card_changes(st: State<'_, AppState>, task_id: String) -> Result<CardChanges, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || of_card(&st, &task_id)).await.map_err(|e| e.to_string())?
}

/// One changed file: its text at `from` and at `to` (None: the worktree as it is on disk), and git's hunks between them.
/// An empty `from` is the pull request's diff (source "pull"): this file's part of it.
#[tauri::command]
pub async fn card_file_diff(st: State<'_, AppState>, task_id: String, path: String, old_path: Option<String>,
                            from: String, to: Option<String>) -> Result<FileDiff, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || file_of_card(&st, &task_id, &path, old_path.as_deref(), &from, to.as_deref()))
        .await.map_err(|e| e.to_string())?
}

/// The changes one run committed: from the commit it started at to the one it ended at.
#[tauri::command]
pub async fn run_changes(st: State<'_, AppState>, run_id: String) -> Result<CardChanges, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || of_run(&st, &run_id)).await.map_err(|e| e.to_string())?
}

// ---- a card ----

/// What `task_id`'s Changes show, from the first of the five places (see the top of this file) that has them. Blocking.
pub fn of_card(st: &AppState, task_id: &str) -> Result<CardChanges, String> {
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let project = project_of(st, &task);
    let Some(branch) = task.branch.clone().filter(|b| !b.trim().is_empty()) else {
        let label = match &project {
            Some(p) => match repo_of(&task, p) {
                Ok(repo) => Main::of(p, &repo).label,
                Err(_) if p.repo_url.is_some() => format!("origin/{}", p.default_branch),
                Err(_) => p.default_branch.clone(),
            },
            None => "main".to_string(),
        };
        return Ok(CardChanges::none(&label, NO_BRANCH));
    };
    let project = project.ok_or_else(|| format!("Gizai can't show what {} changed: it has no project", task.identifier))?;
    let repo = repo_of(&task, &project)?;
    let main = Main::of(&project, &repo);
    let unreadable = |e: gizai_agents::AgentError| format!("Gizai couldn't read what {} changed: {}", task.identifier, crate::pulls::plain(e));

    // 1. its worktree, as it is on disk
    if let Some(wt) = worktree::worktree_of(&repo, &branch).ok().flatten().filter(|w| w.is_dir()) {
        let tip = git_changes::commit_of(&wt, "HEAD")
            .ok_or_else(|| format!("Gizai couldn't read what {} changed: its worktree {} has no commit", task.identifier, wt.display()))?;
        let base = main.base(&repo, &tip, &branch)?;
        let files = git_changes::of_worktree(&wt, &base).map_err(unreadable)?;
        let note = files.is_empty().then(|| no_changes(&main.label));
        return Ok(CardChanges::new(ChangeSource::Worktree, Some(base), None, &main.label, files, note));
    }
    // 2. its branch, once its worktree is gone
    if let Some(tip) = git_changes::commit_of(&repo, &format!("refs/heads/{branch}")) {
        let base = main.base(&repo, &tip, &branch)?;
        let files = git_changes::between(&repo, &base, &tip).map_err(unreadable)?;
        let note = if files.is_empty() { no_changes(&main.label) } else { format!("From its branch {branch}: its worktree was removed.") };
        return Ok(CardChanges::new(ChangeSource::Branch, Some(base), Some(tip), &main.label, files, Some(note)));
    }
    // 3. the range saved at its merge
    let number = task.pr_url.as_deref().and_then(github::pull_number);
    let into = &project.default_branch;
    if let Some((base, head)) = git_changes::saved_range(&repo, &task.identifier) {
        let files = git_changes::between(&repo, &base, &head).map_err(unreadable)?;
        let note = match number {
            Some(n) => format!("As merged into {into} (pull request #{n})."),
            None => format!("As merged into {into}."),
        };
        return Ok(CardChanges::new(ChangeSource::Merge, Some(base), Some(head), &main.label, files, Some(note)));
    }
    // 4. its merge commit on main, then 5. its pull request's diff
    let card = gizai_core::pulls::card(&st.db, task_id).ok();
    // what GitHub or Bitbucket said when asking failed: it would say the same again
    let mut asked: Option<String> = None;
    if let Some(n) = number {
        if task.pr_state.as_deref() == Some("merged") {
            let mut merge = main.commit.as_deref().and_then(|m| git_changes::find_merge_commit(&repo, m, n, &branch));
            if merge.is_none() && let Some(card) = &card {
                match Host::of(st, card).and_then(|h| h.merge_commit(card, n)) {
                    Ok(id) => merge = id.and_then(|id| git_changes::commit_of(&repo, &id)),
                    Err(e) => asked = Some(e),
                }
            }
            if let Some(m) = merge && let Some(parent) = git_changes::commit_of(&repo, &format!("{m}^1")) {
                let files = git_changes::between(&repo, &parent, &m).map_err(unreadable)?;
                let note = format!("From the merge commit {} on {into}.", &m[..m.len().min(7)]);
                return Ok(CardChanges::new(ChangeSource::Merge, Some(parent), Some(m), &main.label, files, Some(note)));
            }
        }
        if asked.is_none() && let Some(card) = &card {
            match pull_files(st, card, n) {
                Ok(files) => {
                    let host = gizai_core::repo_url::provider_name(&card.provider).unwrap_or("GitHub");
                    let note = format!("From {host}'s diff of pull request #{n}, without the files' texts.");
                    let files = files.iter().map(|(f, _)| f.clone()).collect();
                    return Ok(CardChanges::new(ChangeSource::Pull, None, None, &main.label, files, Some(note)));
                }
                Err(e) => asked = Some(e),
            }
        }
    }
    let why = match (card.as_ref().map(|c| c.provider.as_str()), number) {
        (Some("bitbucket"), Some(_)) => format!("Gizai can't find what {} changed: fetch main, or log in to Bitbucket (Settings → Bitbucket)", task.identifier),
        (Some(_), Some(_)) => format!("Gizai can't find what {} changed: fetch main, or log gh in (Settings → GitHub)", task.identifier),
        _ => format!("Gizai can't find what {} changed: its branch {branch} isn't in {} any more, and it has no pull request to read it from",
                     task.identifier, repo.display()),
    };
    Err(match asked {
        Some(e) => format!("{why}. {e}"),
        None => why,
    })
}

/// One file of `task_id`'s changes: its texts at `from` and at `to` (None: the card's worktree as it is on disk), at most
/// 1 MB a side, and git's hunks between them. An empty `from` is its pull request's diff: this file's part of it. Blocking.
pub fn file_of_card(st: &AppState, task_id: &str, path: &str, old_path: Option<&str>, from: &str, to: Option<&str>)
    -> Result<FileDiff, String> {
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let (from, to) = (from.trim(), to.map(str::trim).filter(|t| !t.is_empty()));
    if from.is_empty() {
        return pull_file(st, &task, path, old_path);
    }
    let project = project_of(st, &task).ok_or_else(|| format!("Gizai can't show what {} changed: it has no project", task.identifier))?;
    let repo = repo_of(&task, &project)?;
    let wt = match to {
        Some(_) => None,
        None => {
            let branch = task.branch.as_deref().filter(|b| !b.trim().is_empty()).unwrap_or("-");
            let wt = worktree::worktree_of(&repo, branch).ok().flatten().filter(|w| w.is_dir())
                .ok_or_else(|| format!("{}'s worktree is gone: open its changes again", task.identifier))?;
            Some(wt)
        }
    };
    git_changes::file_diff(&repo, wt.as_deref(), path, old_path, from, to)
        .map_err(|e| format!("Gizai can't show {path}: {}", crate::pulls::plain(e)))
}

/// What a pull request's diff has of `path`: its part of the diff (none over 1 MB), without the file's texts.
fn pull_file(st: &AppState, task: &Task, path: &str, old_path: Option<&str>) -> Result<FileDiff, String> {
    let number = task.pr_url.as_deref().and_then(github::pull_number)
        .ok_or_else(|| format!("{} has no pull request to read {path} from", task.identifier))?;
    let card = gizai_core::pulls::card(&st.db, &task.id).map_err(|e| e.to_string())?;
    let files = pull_files(st, &card, number)?;
    let found = files.iter().find(|(f, _)| f.path == path && (old_path.is_none() || f.old_path.as_deref() == old_path))
        .or_else(|| files.iter().find(|(f, _)| f.path == path))
        .ok_or_else(|| format!("{path} isn't in the diff of pull request #{number}"))?;
    let (file, part) = (&found.0, &found.1);
    let too_large = part.len() as u64 > MAX_TEXT;
    Ok(FileDiff { path: file.path.clone(), old_text: None, new_text: None, hunks: vec![], patch: (!too_large).then(|| part.clone()),
                  binary: file.binary, too_large, old_size: 0, new_size: 0, crlf: part.contains("\r\n") })
}

// ---- a run ----

/// The changes one run committed, from the commit its worktree was at when it started to the one it ended at. Blocking.
pub fn of_run(st: &AppState, run_id: &str) -> Result<CardChanges, String> {
    let run = core_runs::get(&st.db, run_id).map_err(|e| e.to_string())?;
    let task = tasks::get(&st.db, run.task_id.as_deref().unwrap_or_default()).map_err(|_| "This run wasn't on a card".to_string())?;
    let (Some(base), Some(head)) = (run.base_sha.clone(), run.head_sha.clone()) else {
        if crate::runs::live(st).iter().any(|r| r.run_id == run_id) {
            return Err(format!("{}'s run is still going: its changes show once it has ended", run.agent_name));
        }
        return Err("Gizai didn't save where this run started and ended".into());
    };
    let project = project_of(st, &task).ok_or_else(|| format!("Gizai can't show this run's changes: {} has no project", task.identifier))?;
    // its commits are in the project's repository, which its worktree shares
    let repo = match repo_of(&task, &project) {
        Ok(repo) => repo,
        Err(e) => run.worktree_path.as_deref().map(PathBuf::from).filter(|w| w.join(".git").exists()).ok_or(e)?,
    };
    let label = Main::of(&project, &repo).label;
    if base == head {
        return Ok(CardChanges::new(ChangeSource::Run, Some(base), Some(head), &label, vec![], Some("This run made no commits.".into())));
    }
    let files = git_changes::between(&repo, &base, &head)
        .map_err(|e| format!("Gizai can't read this run's changes: {}", crate::pulls::plain(e)))?;
    let note = match worktree::commits(&repo, &base, &head).map(|c| c.len()).unwrap_or(0) {
        0 => "From where this run started to where it ended.".to_string(),
        1 => "The commit this run made.".to_string(),
        n => format!("The {n} commits this run made."),
    };
    Ok(CardChanges::new(ChangeSource::Run, Some(base), Some(head), &label, files, Some(note)))
}

// ---- at a merge ----

/// At a merge, before the PR check cleans up the card's worktree and branch: fetches main as a card start does, then saves
/// the card's range in two refs of the project's repository, `refs/gizai/merged/<identifier>/base` and `/head`
/// (`gizai_agents::changes::range_at_merge`): the card's latest commit the pull request had, and where it meets the merge
/// commit's first parent. That shows the whole card after a merge commit, a squash and a rebase alike, offline, once the
/// worktree and branch are gone. Offline or without the merge commit, main as last fetched counts; nothing is saved when
/// main had the card's commits already (its merge commit on main shows them then). The refs are never pushed. Never
/// fails the PR check: a problem goes to the log. Blocking.
pub(crate) fn save_at_merge(st: &AppState, card: &PrCard, pr: &PullRequest, tip: Option<&str>) {
    let repo = Path::new(&card.repo_path);
    let Some(project) = tasks::get(&st.db, &card.task_id).ok().and_then(|t| project_of(st, &t)) else { return };
    // main as last fetched before this merge was seen, then main as GitHub or Bitbucket has it now
    let before = crate::git::start_point(&project, repo, None).ok().and_then(|m| git_changes::commit_of(repo, &m));
    let after = match crate::git::start_point(&project, repo, Some(crate::git::START_FETCH_LIMIT)) {
        Ok(main) => {
            if project.repo_url.is_some() {
                crate::code::fetched(st, &project.key);
            }
            git_changes::commit_of(repo, &main)
        }
        Err(e) => {
            eprintln!("gizai: saving {}'s changes at its merge with main as last fetched: {e}", card.identifier);
            None
        }
    };
    // the pull request's latest commit when this repository has it, else the branch's own when the pull request has it
    let head = Some(pr.head_ref_oid.as_str()).filter(|h| git_changes::is_commit_id(h)).and_then(|h| git_changes::commit_of(repo, h))
        .or_else(|| tip.filter(|t| pr.contains(t)).and_then(|t| git_changes::commit_of(repo, t)));
    let Some(head) = head else {
        eprintln!("gizai: {}'s changes weren't saved at its merge: this repository doesn't have its pull request's latest commit", card.identifier);
        return;
    };
    let merge = pr.merge_commit.as_ref().map(|c| c.oid.as_str());
    let mains: Vec<&str> = [before.as_deref(), after.as_deref()].into_iter().flatten().collect();
    if let Some((base, head)) = git_changes::range_at_merge(repo, &head, merge, &mains)
        && let Err(e) = git_changes::save_range(repo, &card.identifier, &base, &head)
    {
        eprintln!("gizai: saving {}'s changes at its merge failed: {}", card.identifier, crate::pulls::plain(e));
    }
}

// ---- where things are ----

fn project_of(st: &AppState, task: &Task) -> Option<Project> {
    task.project_id.as_deref().filter(|p| !p.is_empty()).and_then(|id| projects::get(&st.db, id).ok())
}

/// The project's repository on this disk (its linked folder), or why there is none.
fn repo_of(task: &Task, project: &Project) -> Result<PathBuf, String> {
    let repo = project.repo_path.as_deref().map(str::trim).filter(|r| !r.is_empty())
        .ok_or_else(|| format!("Gizai can't show what {} changed: link a git repository to {} first (project page → Edit)", task.identifier, project.name))?;
    let repo = PathBuf::from(repo);
    if !crate::git::is_repo(&repo) {
        return Err(format!("Gizai can't show what {} changed: {} isn't a git repository any more", task.identifier, repo.display()));
    }
    Ok(repo)
}

/// "This card's branch has no changes against origin/main."
fn no_changes(label: &str) -> String {
    format!("This card's branch has no changes against {label}.")
}

/// Main as last fetched: the ref card starts use (`git::start_point`, no fetch), else the local default branch.
struct Main {
    /// How it shows on screen: "origin/main", or "main" for a project without a link.
    label: String,
    commit: Option<String>,
}

impl Main {
    fn of(project: &Project, repo: &Path) -> Main {
        let start = crate::git::start_point(project, repo, None).unwrap_or_else(|_| project.default_branch.clone());
        let local = format!("refs/heads/{}", project.default_branch);
        for rev in [start.as_str(), local.as_str()] {
            if let Some(commit) = git_changes::commit_of(repo, rev) {
                return Main { label: label_of(rev, &project.default_branch), commit: Some(commit) };
            }
        }
        Main { label: label_of(&start, &project.default_branch), commit: None }
    }

    /// Where `tip` (the card's latest commit) meets main: what its changes are measured from.
    fn base(&self, repo: &Path, tip: &str, branch: &str) -> Result<String, String> {
        let main = self.commit.as_deref()
            .ok_or_else(|| format!("Gizai can't find {} in {}: fetch it (git fetch), then try again", self.label, repo.display()))?;
        git_changes::merge_base(repo, tip, main).ok_or_else(|| format!("{branch} has no commit in common with {}", self.label))
    }
}

/// A main ref as the screen names it: a remote's ("origin/main"), the link's that card starts fetch into Gizai's own ref
/// ("origin/main" too), or the local branch ("main").
fn label_of(rev: &str, default_branch: &str) -> String {
    if let Some(remote) = rev.strip_prefix("refs/remotes/") {
        return remote.to_string();
    }
    if rev.starts_with("refs/gizai/base/") {
        return format!("origin/{default_branch}");
    }
    rev.strip_prefix("refs/heads/").unwrap_or(rev).to_string()
}

// ---- GitHub and Bitbucket ----

/// What reaches a card's pull request: gh for a GitHub card, your Bitbucket login for a Bitbucket one.
enum Host {
    GitHub(PathBuf),
    Bitbucket(Login),
}

impl Host {
    /// Finding gh can start a login shell, and the keychain may ask its service: blocking.
    fn of(st: &AppState, card: &PrCard) -> Result<Host, String> {
        match card.provider.as_str() {
            "bitbucket" => crate::bitbucket::login(st).map(Host::Bitbucket),
            _ => crate::pulls::gh_bin(st).map(Host::GitHub),
        }
    }

    fn merge_commit(&self, card: &PrCard, number: u64) -> Result<Option<String>, String> {
        match self {
            Host::GitHub(gh) => github::merge_commit(gh, Path::new(&card.repo_path), &card.repo, number),
            Host::Bitbucket(login) => bitbucket::merge_commit(login, &card.repo, number),
        }
    }

    fn diff(&self, card: &PrCard, number: u64) -> Result<String, String> {
        match self {
            Host::GitHub(gh) => github::pull_diff(gh, Path::new(&card.repo_path), &card.repo, number),
            Host::Bitbucket(login) => bitbucket::pull_diff(login, &card.repo, number),
        }
    }
}

/// How long a fetched pull request diff is kept, and how many.
const KEEP_DIFF: Duration = Duration::from_secs(10 * 60);
const KEEP_DIFFS: usize = 8;

/// The pull request diffs the Changes view fetched (source "pull"), split per file, so that each file's part doesn't ask
/// GitHub or Bitbucket again: the last few, for ten minutes.
#[derive(Default)]
pub struct PullDiffs(Mutex<Vec<PullDiff>>);

struct PullDiff {
    task_id: String,
    number: u64,
    at: Instant,
    files: Arc<Vec<(FileChange, String)>>,
}

/// Pull request `number`'s diff of `card`, split per file: the one fetched in the last ten minutes, else fetched now.
fn pull_files(st: &AppState, card: &PrCard, number: u64) -> Result<Arc<Vec<(FileChange, String)>>, String> {
    {
        let mut kept = st.changes.0.lock().unwrap();
        kept.retain(|d| d.at.elapsed() < KEEP_DIFF);
        if let Some(d) = kept.iter().find(|d| d.task_id == card.task_id && d.number == number) {
            return Ok(d.files.clone());
        }
    }
    let patch = Host::of(st, card)?.diff(card, number)?;
    let files = Arc::new(git_changes::parse_patch(&patch));
    let mut kept = st.changes.0.lock().unwrap();
    kept.retain(|d| d.task_id != card.task_id);
    kept.push(PullDiff { task_id: card.task_id.clone(), number, at: Instant::now(), files: files.clone() });
    if kept.len() > KEEP_DIFFS {
        kept.remove(0);
    }
    Ok(files)
}
