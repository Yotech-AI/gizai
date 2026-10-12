//! A person's card (GA-111; contract C4 with the card bar and Open in editor, F4; the commands are in `editor`). A small
//! card can go to a person who does it by hand, in the Editor or in their own editor, then hands it over like an agent:
//! - Open in editor (`start`), on a card assigned to you: its worktree made and prepared as for an agent's run, the card
//!   pinned to you so no agent takes it, you its implementer, moved on from To do as you (`runs::start_by_hand`).
//! - The card bar: where it is (`card`), Commit… (`commit`, every change), Push (`push`, as Gizai's push after a run).
//! - Hand over (`over`): refuses uncommitted changes, pushes, moves the card on as a builder's `ready_for_testing` does
//!   (Testing, or Review with its Testing switch off), takes the pin off so QA takes it, and comments on the card. A QA
//!   fail brings it back to you, pinned again (`gizai_core::workflow::apply_outcome`).
//!
//! Each refuses, in a plain sentence, a card that isn't yours by hand, and Commit, Push and Hand over refuse while an
//! agent works on the card. Blocking work (git, the remote) runs off the async threads.
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Mutex;

use gizai_agents::worktree::{self, Commit, Pushed};
use gizai_core::model::{Project, Task};
use gizai_core::repo_url::{self, RepoUrl};
use gizai_core::{projects, tasks, workflow};
use serde::Serialize;

use crate::AppState;
use crate::runs::Note;

type R<T> = Result<T, String>;

/// Where a person's card is (contract C4's `HandCard`): its worktree as the Editor names it (an `EditorFolder::id`), its
/// branch, how many files have uncommitted changes (as the Editor's git status counts them), and how many of its commits
/// a push would still send, as far as this computer knows (0 for a project without a link to push to).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandCard {
    pub task_id: String,
    pub folder: String,
    pub branch: String,
    pub changed: u32,
    pub unpushed: u32,
}

/// A card that is yours by hand, as the commands need it.
struct Hand {
    task: Task,
    project: Project,
    repo: PathBuf,
    /// Its worktree, where a run makes it: `<data>/worktrees/<KEY>/<identifier>`.
    wt: PathBuf,
    branch: String,
}

impl Hand {
    /// The project's link to push to (GitHub, Bitbucket or another git URL), if it has one.
    fn link(&self) -> Option<RepoUrl> {
        self.project.repo_url.as_deref().and_then(|u| repo_url::normalize(u).ok().flatten())
    }

    /// The main branch the card started from, as last fetched (`git::start_point`).
    fn base(&self) -> R<String> {
        crate::git::start_point(&self.project, &self.repo, None).map_err(crate::pulls::plain)
    }

    /// Files with uncommitted changes in its worktree, untracked ones each, as the Editor counts them.
    fn changed(&self) -> usize {
        gizai_agents::editor::status(&self.wt).uncommitted
    }
}

/// The card `task_id` when it is yours by hand (pinned to you, `workflow::by_hand_check`), with its worktree there on the
/// card's branch; with `idle`, also not while an agent works on it or its run starts. Else why not, in a plain sentence.
fn hand_of(st: &AppState, task_id: &str, idle: bool) -> R<Hand> {
    workflow::by_hand_check(&st.db, &st.you_id, task_id, idle).map_err(|e| e.to_string())?;
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    if idle && crate::runs::working_on(st, task_id) {
        return Err(format!("An agent is working on {}: wait for its run to end", task.identifier));
    }
    let project = projects::get(&st.db, task.project_id.as_deref().unwrap_or_default()).map_err(|e| e.to_string())?;
    let repo = project.repo_path.clone().filter(|p| !p.trim().is_empty()).map(PathBuf::from)
        .ok_or_else(|| format!("Link a git repository to {} first (project page → Edit)", project.name))?;
    let wt = st.data_dir.join("worktrees").join(&project.key).join(&task.identifier);
    let gone = || format!("{}'s worktree isn't there any more: open it in the editor again to make it", task.identifier);
    let Some(branch) = task.branch.clone().filter(|b| !b.trim().is_empty()) else { return Err(gone()) };
    if !wt.join(".git").exists() {
        return Err(gone());
    }
    match worktree::branch_of(&wt) {
        Some(b) if b == branch => Ok(Hand { task, project, repo, wt, branch }),
        Some(b) => Err(format!("{}'s worktree is on branch {b}, not on the card's branch {branch}: switch it back first", task.identifier)),
        None => Err(format!("{}'s worktree isn't on a branch: switch it back to {branch} first", task.identifier)),
    }
}

/// Cards an Open in editor, Commit, Push or Hand over works on now.
static BUSY: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Held while an Open in editor, Commit, Push or Hand over works on a card: one at a time per card, so a second click
/// is refused instead of running twice.
struct Busy(String);

impl Busy {
    fn take(task_id: &str, identifier: &str) -> R<Busy> {
        if BUSY.lock().unwrap_or_else(|e| e.into_inner()).insert(task_id.to_string()) {
            Ok(Busy(task_id.to_string()))
        } else {
            Err(format!("Gizai is still busy with {identifier} for you: wait a moment"))
        }
    }

    fn of(h: &Hand) -> R<Busy> {
        Busy::take(&h.task.id, &h.task.identifier)
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.0);
    }
}

/// `f` on a thread that may wait for git and the remote, off the async threads.
async fn off<T: Send + 'static>(st: &AppState, f: impl FnOnce(&AppState) -> R<T> + Send + 'static) -> R<T> {
    let st = st.clone();
    tokio::task::spawn_blocking(move || f(&st)).await.map_err(|e| e.to_string())?
}

/// "1 commit", "2 commits".
fn commits_word(n: u32) -> String {
    format!("{n} {}", if n == 1 { "commit" } else { "commits" })
}

/// "1 uncommitted change", "2 uncommitted changes".
fn changes_word(n: usize) -> String {
    format!("{n} uncommitted {}", if n == 1 { "change" } else { "changes" })
}

/// Open in editor (contract C4's `start_by_hand`), on a card assigned to you: `runs::start_by_hand` (its worktree made and
/// prepared as for an agent, the card pinned to you, you its implementer, moved on from To do as you). Returns where it is.
pub async fn start(st: &AppState, task_id: &str) -> R<HandCard> {
    let identifier = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?.identifier;
    let busy = Busy::take(task_id, &identifier)?;
    crate::runs::start_by_hand(st, task_id).await?;
    drop(busy);
    card(st, task_id).await
}

/// Where your card by hand is (contract C4's `hand_card`): `HandCard`. Local only: nothing is fetched or asked.
pub async fn card(st: &AppState, task_id: &str) -> R<HandCard> {
    let id = task_id.to_string();
    off(st, move |st| card_now(st, &id)).await
}

fn card_now(st: &AppState, task_id: &str) -> R<HandCard> {
    let h = hand_of(st, task_id, false)?;
    let gone = || format!("{}'s worktree isn't there any more: open it in the editor again to make it", h.task.identifier);
    let shown = gizai_core::folders::on_disk(&h.wt).ok_or_else(gone)?;
    let folder = crate::editor::folder(st, &shown.display().to_string())?.id;
    let unpushed = match h.link() {
        // the remote's copy of the branch as a push through your remote for the link last left it
        Some(link) => {
            let pushed = crate::git::remote_for(&h.repo, &link.url).map(|r| format!("refs/remotes/{r}/{}", h.branch));
            worktree::unpushed(&h.repo, &h.branch, &h.base()?, pushed.as_deref()).map_err(crate::pulls::plain)?
        }
        None => 0,
    };
    Ok(HandCard { task_id: h.task.id.clone(), folder, changed: h.changed() as u32, branch: h.branch, unpushed })
}

/// Commit…, on your card by hand (contract C4's `hand_commit`): every change in its worktree, untracked files included,
/// committed with `message` (`worktree::commit_all`; the repository's own hooks run). Returns the commit. Refused while an
/// agent works on the card, without a message, and with nothing to commit.
pub async fn commit(st: &AppState, task_id: &str, message: &str) -> R<Commit> {
    let (id, message) = (task_id.to_string(), message.to_string());
    off(st, move |st| {
        let h = hand_of(st, &id, true)?;
        let _busy = Busy::of(&h)?;
        worktree::commit_all(&h.wt, &message).map_err(crate::pulls::plain)
    }).await
}

/// Push, on your card by hand (contract C4's `hand_push`): its branch goes where Gizai's push after a run sends it, the
/// same way (`runs::push_card`: where Open pull request pushes, over Settings' Push over, only commits the remote doesn't
/// have, never forced). Returns what it did ("Pushed gizai/ga-123-… (2 commits)."). Refused while an agent works on the
/// card, and for a project without a link to push to.
pub async fn push(st: &AppState, task_id: &str) -> R<String> {
    let id = task_id.to_string();
    off(st, move |st| {
        let h = hand_of(st, &id, true)?;
        let _busy = Busy::of(&h)?;
        let link = h.link().ok_or_else(|| {
            format!("{} has no GitHub, Bitbucket or git link to push to: add one to {} first (project page → Edit)", h.task.identifier, h.project.name)
        })?;
        let pushed = crate::runs::push_card(st, &h.project, &h.repo, &link, &h.branch)?;
        let mut said = match pushed {
            Pushed::Commits(n) => format!("Pushed {} ({}).", h.branch, commits_word(n)),
            Pushed::Nothing => {
                let to = repo_url::provider_name(&link.provider).unwrap_or("The project's repository");
                format!("Nothing to push: {to} has every commit of {} already.", h.branch)
            }
        };
        let left = h.changed();
        if left > 0 {
            said.push_str(&format!(" {} {} pushed: commit {} first.", changes_word(left), if left == 1 { "isn't" } else { "aren't" },
                                   if left == 1 { "it" } else { "them" }));
        }
        Ok(said)
    }).await
}

/// Hand over, on your card by hand (contract C4's `hand_over`). It refuses uncommitted changes, then pushes as `push` does
/// (when the project has a link to push to; a failed push changes nothing on the card), then moves the card on as a
/// builder's `ready_for_testing` does (Testing, or Review with its Testing switch off), takes the pin off so the next
/// column's agents take it, and comments "Handed over by <you>: 2 commits." with its commits (`workflow::hand_over`).
/// Then the queue runs, so QA starts on it now when its column is Auto. Returns the name of the column it is in.
pub async fn over(st: &AppState, task_id: &str) -> R<String> {
    let id = task_id.to_string();
    let column = off(st, move |st| over_now(st, &id)).await?;
    (st.notify)(Note::RowsChanged("tasks"));
    (st.notify)(Note::RowsChanged("comments"));
    let st2 = st.clone();
    tokio::spawn(async move { crate::runs::pull(&st2).await; });
    Ok(column)
}

fn over_now(st: &AppState, task_id: &str) -> R<String> {
    let h = hand_of(st, task_id, true)?;
    let _busy = Busy::of(&h)?;
    let left = h.changed();
    if left > 0 {
        return Err(format!("{} has {}: commit {} before you hand it over", h.task.identifier, changes_word(left),
                           if left == 1 { "it" } else { "them" }));
    }
    if let Some(link) = h.link() {
        crate::runs::push_card(st, &h.project, &h.repo, &link, &h.branch)?;
    }
    let comment = handed_over(&crate::runs::actor_name(st, &st.you_id), &own_commits(&h));
    workflow::hand_over(&st.db, &st.you_id, task_id, &comment).map_err(|e| e.to_string())
}

/// The card's own commits, oldest first: those of its branch that main, as last fetched, doesn't have (as a pull request
/// lists them). Empty when git can't say.
fn own_commits(h: &Hand) -> Vec<Commit> {
    let ids = h.base().ok()
        .and_then(|base| worktree::rev_parse(&h.repo, &format!("{base}^{{commit}}")).ok())
        .zip(worktree::rev_parse(&h.repo, &format!("refs/heads/{}", h.branch)).ok());
    ids.and_then(|(base, head)| worktree::commits(&h.repo, &base, &head).ok()).unwrap_or_default()
}

/// The most commits Hand over's comment lists.
const LISTED: usize = 20;

/// Hand over's comment: "Handed over by Jeffrey: 2 commits." and the commits, at most `LISTED`.
fn handed_over(name: &str, commits: &[Commit]) -> String {
    if commits.is_empty() {
        return format!("Handed over by {name}: no commits of its own.");
    }
    let mut s = format!("Handed over by {name}: {}.\n\n", commits_word(commits.len() as u32));
    for c in commits.iter().take(LISTED) {
        s.push_str(&format!("- {} {}\n", &c.sha[..c.sha.len().min(7)], c.subject));
    }
    if commits.len() > LISTED {
        s.push_str(&format!("- and {} more\n", commits.len() - LISTED));
    }
    s.trim_end().to_string()
}
