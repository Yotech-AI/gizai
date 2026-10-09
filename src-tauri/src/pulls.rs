//! Review on GitHub. Open pull request (a card in Review) pushes the card's branch over SSH with your keys, or over
//! HTTPS with gh's login (Settings → GitHub → Push over), and opens its pull request with your GitHub CLI (gh). The PR
//! check follows the pull requests of cards in Review, and of any open card whose pull request isn't merged yet: every
//! two minutes, when a run moves a card to Review, and when you open such a card. A merge on GitHub moves its card to
//! Deploy (Done for a team without a Deploy column), where nothing starts by itself, and removes its worktree. Usable
//! without a Tauri app (tests): the UI hears about changes through `AppState::notify`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gizai_agents::connection::PushOver;
use gizai_agents::github::{self, PullRequest};
use gizai_agents::{AgentError, worktree};
use gizai_core::model::Task;
use gizai_core::pulls::{self, PrCard};
use gizai_core::{settings, tasks};
use serde::Serialize;

use crate::AppState;
use crate::runs::Note;

/// How often the PR check asks GitHub.
pub const CHECK_EVERY: Duration = Duration::from_secs(120);

/// The longest card text a pull request description takes (GitHub's limit is 65,536 characters).
const MAX_BODY: usize = 60_000;

#[derive(Default)]
pub struct PullChecks {
    /// Cards whose pull request is being opened or checked right now: one at a time per card.
    busy: Mutex<HashSet<String>>,
    /// The last problem per card ("" for gh itself), so the log says each one once.
    errors: Mutex<HashMap<String, String>>,
}

/// A card's pull request, as the UI shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullInfo {
    pub url: String,
    pub number: Option<u64>,
    /// open, draft, merged or closed
    pub state: String,
    /// Something worth knowing (uncommitted changes left out, what a merge did to the worktree), else None.
    pub note: Option<String>,
}

/// What one PR check did with a card.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub task_id: String,
    pub pull: Option<PullInfo>,
    /// The column a merge moved the card to.
    pub moved_to: Option<String>,
    /// Whether anything on the card changed.
    pub changed: bool,
}

/// Held while Gizai opens or checks a card's pull request.
struct Busy {
    checks: Arc<PullChecks>,
    task_id: String,
}

impl Busy {
    fn take(st: &AppState, task_id: &str) -> Option<Busy> {
        st.pulls.busy.lock().unwrap().insert(task_id.to_string())
            .then(|| Busy { checks: st.pulls.clone(), task_id: task_id.to_string() })
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.checks.busy.lock().unwrap().remove(&self.task_id);
    }
}

/// A problem in the background, logged once until it changes or goes away.
fn log_once(st: &AppState, key: &str, msg: &str) {
    let mut errors = st.pulls.errors.lock().unwrap();
    if errors.get(key).map(String::as_str) != Some(msg) {
        eprintln!("gizai: {msg}");
        errors.insert(key.to_string(), msg.to_string());
    }
}

fn forget_error(st: &AppState, key: &str) {
    st.pulls.errors.lock().unwrap().remove(key);
}

/// git's own words, without the "git:" prefix.
fn plain(e: AgentError) -> String {
    match e {
        AgentError::Git(m) => m,
        other => other.to_string(),
    }
}

/// Finds `gh` the way a login shell would, then in the usual install places. Saves what it finds.
pub fn detect_gh(st: &AppState) -> Option<String> {
    let from_shell = std::process::Command::new("bash").args(["-lc", "command -v gh"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|p| !p.is_empty() && crate::runs::executable(Path::new(p)));
    let home = std::env::var("HOME").unwrap_or_default();
    let found = from_shell.or_else(|| {
        [".local/bin/gh", ".local/share/mise/shims/gh", "bin/gh"].iter().map(|rel| format!("{home}/{rel}"))
            .chain(["/usr/local/bin/gh", "/usr/bin/gh", "/home/linuxbrew/.linuxbrew/bin/gh", "/snap/bin/gh"].map(String::from))
            .find(|p| crate::runs::executable(Path::new(p)))
    });
    if let Some(p) = &found {
        let _ = settings::set(&st.db, "gh_bin", p);
    }
    found
}

/// The saved GitHub CLI; found (and saved) only when none was ever saved. A saved one that has gone missing is
/// reported, never silently replaced by another.
pub(crate) fn gh_bin(st: &AppState) -> Result<PathBuf, String> {
    let bin = match settings::get::<String>(&st.db, "gh_bin").ok().flatten() {
        Some(saved) => Some(saved),
        None => detect_gh(st),
    };
    match bin {
        Some(b) if crate::runs::executable(Path::new(&b)) => Ok(PathBuf::from(b)),
        Some(b) => Err(format!("The GitHub CLI isn't at {b} any more: fix its path in Settings")),
        None => Err("The GitHub CLI (gh) isn't installed: install it from cli.github.com and log in with gh auth login, or set its path in Settings".into()),
    }
}

/// `gh_bin` off the async threads: finding gh can start a login shell.
async fn gh(st: &AppState) -> Result<PathBuf, String> {
    let st = st.clone();
    tokio::task::spawn_blocking(move || gh_bin(&st)).await.map_err(|e| e.to_string())?
}

/// The card's pull request as Gizai last saw it.
fn known(card: &PrCard) -> Option<PullInfo> {
    card.pr_url.clone().map(|url| PullInfo {
        number: github::pull_number(&url), state: card.pr_state.clone().unwrap_or_else(|| "open".into()), url, note: None,
    })
}

fn is_live(st: &AppState, task_id: &str) -> bool {
    crate::runs::live(st).iter().any(|r| r.task_id == task_id)
}

/// Open pull request, for a card in Review: pushes the card's branch to GitHub (through your remote for the project's
/// GitHub link, else to the link itself) over SSH with your keys or over HTTPS with gh's login, as Settings → GitHub
/// says, then opens a pull request into the project's main branch with gh, titled and described after the card. A
/// branch that already has an open pull request (an agent opened one) keeps it, and the push adds any new commits to
/// it. A failed push records nothing.
pub async fn open(st: &AppState, task_id: &str) -> Result<PullInfo, String> {
    let card = pulls::card(&st.db, task_id).map_err(|e| e.to_string())?;
    if card.category != "review" {
        return Err(format!("{} isn't in Review: a pull request is opened from Review", card.identifier));
    }
    if is_live(st, task_id) {
        return Err(format!("An agent is working on {}: open the pull request when its run has ended", card.identifier));
    }
    let gh = gh(st).await?;
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let _busy = Busy::take(st, task_id).ok_or_else(|| format!("Gizai is already busy with {}'s pull request", card.identifier))?;
    let (title, body) = (format!("{}: {}", task.identifier, task.title), body(&task));
    let (c, over) = (card.clone(), crate::github::push_over(st, &gh));
    let (info, created) = tokio::task::spawn_blocking(move || open_blocking(&gh, &c, &title, &body, &over)).await.map_err(|e| e.to_string())??;
    // one an agent opened earlier is noted as seen by Gizai, not as opened by you
    pulls::record(&st.db, created.then_some(st.you_id.as_str()), task_id, &info.url, &info.state, created).map_err(|e| e.to_string())?;
    (st.notify)(Note::RowsChanged("tasks"));
    Ok(info)
}

/// Pushes and opens (or keeps the open pull request); returns it, and whether Gizai opened it now.
fn open_blocking(gh: &Path, card: &PrCard, title: &str, body: &str, over: &PushOver) -> Result<(PullInfo, bool), String> {
    let repo = Path::new(&card.repo_path);
    if worktree::rev_parse(repo, &format!("refs/heads/{}", card.branch)).is_err() {
        return Err(format!("{}'s branch {} isn't in {} any more", card.identifier, card.branch, card.repo_path));
    }
    // the same place a run fetches main from
    let to = crate::git::remote_for(repo, &card.repo_url).unwrap_or_else(|| card.repo_url.clone());
    worktree::push_branch_over(repo, &to, &card.branch, over).map_err(plain)?;
    let open = github::pulls_for_branch(gh, repo, &card.repo, &card.branch)?.into_iter().find(|p| p.state == "OPEN");
    let (url, state, created) = match open {
        Some(p) => (p.url.clone(), p.state().to_string(), false),
        None => (github::create_pull(gh, repo, &card.repo, &card.branch, &card.default_branch, title, body)?, "open".to_string(), true),
    };
    let left_out = worktree::worktree_of(repo, &card.branch).ok().flatten()
        .and_then(|wt| worktree::uncommitted(&wt).ok())
        .filter(|n| *n > 0);
    let note = left_out.map(|n| format!("Its worktree has {n} uncommitted {}, which the pull request doesn't have",
                                        if n == 1 { "change" } else { "changes" }));
    Ok((PullInfo { number: github::pull_number(&url), url, state, note }, created))
}

/// The pull request's description: the card's description and acceptance criteria, and the card it comes from.
fn body(t: &Task) -> String {
    let mut b = String::new();
    let description = t.description_md.trim();
    if !description.is_empty() {
        b.push_str(description);
        b.push_str("\n\n");
    }
    if let Some(a) = t.acceptance_md.as_deref().map(str::trim).filter(|a| !a.is_empty()) {
        b.push_str("## Acceptance criteria\n\n");
        b.push_str(a);
        b.push_str("\n\n");
    }
    if b.chars().count() > MAX_BODY {
        b = b.chars().take(MAX_BODY).collect::<String>() + "…\n\n";
    }
    b.push_str(&format!("From Gizai card {}.", t.identifier));
    b
}

/// Check now (the page of a card the PR check follows): asks GitHub about the card's pull request at once, so one an
/// agent opened shows straight away and a merge moves the card. Returns the card's pull request, if it has one.
pub async fn check(st: &AppState, task_id: &str) -> Result<Option<PullInfo>, String> {
    let card = pulls::card(&st.db, task_id).map_err(|e| e.to_string())?;
    if !card.followed() || is_live(st, task_id) {
        return Ok(known(&card));
    }
    let gh = gh(st).await?;
    let Some(_busy) = Busy::take(st, task_id) else { return Ok(known(&card)) };
    let (st2, c) = (st.clone(), card.clone());
    let checked = tokio::task::spawn_blocking(move || check_blocking(&st2, &gh, &c)).await.map_err(|e| e.to_string())??;
    forget_error(st, task_id);
    if checked.changed {
        (st.notify)(Note::RowsChanged("tasks"));
    }
    Ok(checked.pull)
}

/// Checks the card's pull request in the background (a run just moved the card to Review). Nothing for a card
/// without a GitHub link or branch.
pub fn check_soon(st: &AppState, task_id: &str) {
    if pulls::card(&st.db, task_id).is_err() {
        return;
    }
    let (st, id) = (st.clone(), task_id.to_string());
    tokio::spawn(async move {
        if let Err(e) = check(&st, &id).await {
            log_once(&st, &id, &format!("checking a pull request failed: {e}"));
        }
    });
}

/// One round of the PR check: every card it follows (`PrCard::followed`) that no agent is working on. Without such
/// cards it never starts gh.
pub async fn check_all(st: &AppState) -> Vec<Checked> {
    let cards: Vec<PrCard> = pulls::to_check(&st.db).unwrap_or_default().into_iter().filter(|c| !is_live(st, &c.task_id)).collect();
    if cards.is_empty() {
        return vec![];
    }
    let gh = match gh(st).await {
        Ok(g) => { forget_error(st, ""); g }
        Err(e) => { log_once(st, "", &format!("the pull request check can't run: {e}")); return vec![]; }
    };
    let mut out = vec![];
    for card in cards {
        let Some(_busy) = Busy::take(st, &card.task_id) else { continue };
        let (st2, gh2, c) = (st.clone(), gh.clone(), card.clone());
        match tokio::task::spawn_blocking(move || check_blocking(&st2, &gh2, &c)).await {
            Ok(Ok(checked)) => {
                forget_error(st, &card.task_id);
                if checked.changed {
                    (st.notify)(Note::RowsChanged("tasks"));
                }
                // A merge moved the card on: on an Auto column its agents pick it up; on a Manual one nothing starts.
                if checked.moved_to.is_some() {
                    let st2 = st.clone();
                    tokio::spawn(async move { crate::runs::pull(&st2).await; });
                }
                out.push(checked);
            }
            Ok(Err(e)) => log_once(st, &card.task_id, &format!("checking the pull request of {} failed: {e}", card.identifier)),
            Err(e) => log_once(st, &card.task_id, &format!("checking the pull request of {} failed: {e}", card.identifier)),
        }
    }
    out
}

/// Asks GitHub for the card's pull requests and brings the card up to date with the one to show (an open one first,
/// else the one Gizai follows, else the newest). A merge moves a Review card to Review's next column (`pulls::merged`) and
/// cleans up, once: when Gizai
/// followed that pull request, or when it has the branch's latest commit (so a card reopened after an earlier merge
/// stays where it is).
fn check_blocking(st: &AppState, gh: &Path, card: &PrCard) -> Result<Checked, String> {
    let repo = Path::new(&card.repo_path);
    let prs = github::pulls_for_branch(gh, repo, &card.repo, &card.branch)?;
    let followed = card.pr_url.as_deref().and_then(|u| prs.iter().find(|p| p.url == u));
    let mut out = Checked { task_id: card.task_id.clone(), pull: known(card), moved_to: None, changed: false };
    let Some(pr) = prs.iter().find(|p| p.state == "OPEN").or(followed).or(prs.first()) else { return Ok(out) };
    let state = pr.state();
    let mut info = PullInfo { url: pr.url.clone(), number: Some(pr.number), state: state.to_string(), note: None };
    let seen = card.pr_url.as_deref() == Some(pr.url.as_str());
    if state == "merged" && !(seen && card.pr_state.as_deref() == Some("merged")) {
        let tip = worktree::rev_parse(repo, &format!("refs/heads/{}", card.branch)).ok();
        if seen || tip.as_deref().is_some_and(|t| pr.contains(t)) {
            out.moved_to = pulls::merged(&st.db, &st.you_id, &card.task_id, &pr.url).map_err(|e| e.to_string())?;
            let sentence = clean_up(st, card, pr, tip.as_deref());
            if !sentence.is_empty() {
                let _ = pulls::note_cleanup(&st.db, &card.task_id, &sentence);
                info.note = Some(sentence);
            }
            out.changed = true;
            out.pull = Some(info);
            return Ok(out);
        }
    }
    out.changed = pulls::record(&st.db, None, &card.task_id, &pr.url, state, false).map_err(|e| e.to_string())?;
    out.pull = Some(info);
    Ok(out)
}

/// After a merge: removes the card's worktree (`worktree::remove_card_worktree`: only one of Gizai's, never by
/// force, so one with uncommitted changes stays) and deletes its branch when the pull request has its latest commit.
/// Returns what it did as one sentence for the card's activity ("" when there was nothing to clean up).
fn clean_up(st: &AppState, card: &PrCard, pr: &PullRequest, tip: Option<&str>) -> String {
    let all_in = tip.is_some_and(|t| pr.contains(t));
    let removal = worktree::remove_card_worktree(Path::new(&card.repo_path), &st.data_dir.join("worktrees"), &card.branch, all_in);
    let mut said = removal.phrases(&card.branch);
    if tip.is_some() && !all_in && removal.kept.is_none() {
        said.push(format!("kept branch {}, which has commits the pull request doesn't", card.branch));
    }
    match said.len() {
        0 => String::new(),
        1 => format!("{} after the merge", said[0]),
        n => format!("{} and {} after the merge", said[..n - 1].join(", "), said[n - 1]),
    }
}
