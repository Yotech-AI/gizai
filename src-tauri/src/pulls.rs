//! Review on GitHub or Bitbucket. Open pull request (a card in Review) pushes the card's branch and opens its pull
//! request: on GitHub over SSH with your keys, or over HTTPS with gh's login (Settings → GitHub → Push over), with your
//! GitHub CLI (gh); on Bitbucket over SSH with your keys, with Bitbucket's API and your login (Settings → Bitbucket).
//! The end of every run pushes the card's branch to the same place, the same way (`push_to`, `push_over_for`), and an
//! agent's own push in a run goes the same way too (`run_git_config`).
//! The PR check follows the pull requests of cards in Review, and of any open card whose pull request isn't merged yet.
//! Every two minutes it asks git (one `ls-remote` per repository), and GitHub or Bitbucket only when something moved
//! there, and at least every ten minutes: then one light gh call per GitHub repository (`check_all`). When a run moves a
//! card to Review, and when you open such a card, it asks GitHub or Bitbucket at once (`check`). Every gh call shares
//! GitHub's API limit (5,000 points an hour) with each agent's gh; one light call costs a point. A merge moves its card
//! to Deploy (Done for a team without a Deploy column), where nothing starts by itself, and removes its worktree; first
//! it saves the card's range for its Changes (`changes::save_at_merge`). Usable without a Tauri app (tests): the UI
//! hears about changes through `AppState::notify`.
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::bitbucket::{self, Login};
use gizai_agents::connection::PushOver;
use gizai_agents::github::{self, PullRequest};
use gizai_agents::{AgentError, worktree};
use gizai_core::model::Task;
use gizai_core::pulls::{self, PrCard};
use gizai_core::{settings, tasks};
use serde::Serialize;

use crate::AppState;
use crate::runs::Note;

/// How often the PR check runs: it asks git every round, and GitHub and Bitbucket when git shows something moved.
pub const CHECK_EVERY: Duration = Duration::from_secs(120);

/// How often the PR check asks GitHub or Bitbucket about a repository at least, when git shows nothing moved there:
/// closing, reopening or marking a pull request ready moves no ref.
pub const ASK_AT_LEAST_EVERY: Duration = Duration::from_secs(10 * 60);

/// A round this much short of `ASK_AT_LEAST_EVERY` counts as on time: the rounds' own timing shifts a little.
const ON_TIME: Duration = Duration::from_secs(30);

/// How long the PR check's `git ls-remote` of a repository may take.
const ASK_GIT_LIMIT: Duration = Duration::from_secs(30);

/// GitHub's refs of its pull requests' latest commits: a new one is a pull request opened.
const PULL_REFS: &str = "refs/pull/*/head";

/// The longest card text a pull request description takes (GitHub's limit is 65,536 characters; Bitbucket takes more).
const MAX_BODY: usize = 60_000;

/// What reaches a card's pull requests: your GitHub CLI, or Bitbucket's API with your login.
#[derive(Clone)]
enum Via {
    GitHub(PathBuf),
    Bitbucket(Login),
}

impl Via {
    /// The pull requests from the card's branch, in any state, newest first.
    fn pulls_for_branch(&self, card: &PrCard) -> Result<Vec<PullRequest>, String> {
        match self {
            Via::GitHub(gh) => github::pulls_for_branch(gh, Path::new(&card.repo_path), &card.repo, &card.branch),
            Via::Bitbucket(login) => bitbucket::pulls_for_branch(login, &card.repo, &card.branch),
        }
    }

    /// Opens a pull request from the card's branch into the project's main branch; returns its link.
    fn create_pull(&self, card: &PrCard, title: &str, body: &str) -> Result<String, String> {
        match self {
            Via::GitHub(gh) => github::create_pull(gh, Path::new(&card.repo_path), &card.repo, &card.branch, &card.default_branch, title, body),
            Via::Bitbucket(login) => bitbucket::create_pull(login, &card.repo, &card.branch, &card.default_branch, title, body),
        }
    }

    /// Whether merged pull request `pr` has `tip`, the branch's latest commit: when it is its latest commit, or this
    /// repository has its latest commit and `tip` comes before it (git). Only when this repository doesn't have that
    /// commit does GitHub list its commits (one `gh pr view <n> --json commits`). A merged Bitbucket pull request comes
    /// with its commits (`bitbucket::pulls_for_branch`).
    fn has_tip(&self, card: &PrCard, pr: &PullRequest, tip: &str) -> Result<bool, String> {
        let Via::GitHub(gh) = self else { return Ok(pr.contains(tip)) };
        if pr.contains(tip) {
            return Ok(true);
        }
        let repo = Path::new(&card.repo_path);
        if let Some(has) = worktree::is_ancestor(repo, tip, &pr.head_ref_oid) {
            return Ok(has);
        }
        let commits = github::pull_commits(gh, repo, &card.repo, pr.number)
            .map_err(|e| format!("gh couldn't list the commits of pull request #{}: {e}", pr.number))?;
        Ok(PullRequest { commits, ..pr.clone() }.contains(tip))
    }
}

/// What reaches `card`'s pull requests: gh for a GitHub card, your Bitbucket login for a Bitbucket one.
async fn via(st: &AppState, card: &PrCard) -> Result<Via, String> {
    match card.provider.as_str() {
        "bitbucket" => bitbucket_login(st).await.map(Via::Bitbucket),
        _ => gh(st).await.map(Via::GitHub),
    }
}

/// Your Bitbucket login from the keychain, off the async threads (the keychain may ask its service).
async fn bitbucket_login(st: &AppState) -> Result<Login, String> {
    let st = st.clone();
    tokio::task::spawn_blocking(move || crate::bitbucket::login(&st)).await.map_err(|e| e.to_string())?
}

#[derive(Default)]
pub struct PullChecks {
    /// Cards whose pull request is being opened or checked right now: one at a time per card.
    busy: Mutex<HashSet<String>>,
    /// The last problem per card ("" for gh itself, a repository's key for its own), so the log says each one once.
    errors: Mutex<HashMap<String, String>>,
    /// Per repository (`by_repository`'s key): what the PR check last asked GitHub or Bitbucket about it with. Kept in
    /// memory only: after a restart the first round asks.
    asked: Mutex<HashMap<String, Asked>>,
}

/// What a round of the PR check asked GitHub or Bitbucket about a repository with.
struct Asked {
    /// The round's time.
    at: Instant,
    /// What git's ls-remote said just before; None when it couldn't say.
    refs: Option<Vec<(String, String)>>,
    /// The cards the call covered: (task id, branch).
    cards: BTreeSet<(String, String)>,
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
pub(crate) fn plain(e: AgentError) -> String {
    match e {
        AgentError::Git(m) => m,
        other => other.to_string(),
    }
}

/// Where a card's branch is pushed: through the repository's remote for the project's link `link` (your own name for it,
/// origin first), the same place a run fetches main from; else to the link itself. Open pull request and Gizai's push
/// after every run (`runs`) both push there.
pub(crate) fn push_to(repo: &Path, link: &str) -> String {
    crate::git::remote_for(repo, link).unwrap_or_else(|| link.to_string())
}

/// How a push to a project linked to `provider` ("github", "bitbucket" or "git") goes now: to GitHub over SSH with your
/// keys or over HTTPS with gh's login, as Settings → GitHub → Push over says (the same as Open pull request); to
/// Bitbucket over SSH with your keys; to another git URL as its address says (SSH's rules rewrite only GitHub's
/// addresses). Over HTTPS it needs gh, and finding gh can start a login shell: blocking.
pub(crate) fn push_over_for(st: &AppState, provider: &str) -> Result<PushOver, String> {
    match provider {
        "bitbucket" => Ok(PushOver::Bitbucket),
        "github" if crate::github::push_over_name(st) == "https" => gh_bin(st).map(|gh| crate::github::push_over(st, &gh)),
        _ => Ok(PushOver::Ssh),
    }
}

/// The git settings of a task run of `project` (GA-92), whose worktree is `wt`: the agent's own `git push` to the
/// project's repository goes the way Gizai's push after the run goes (`push_over_for`), and nothing else changes
/// (`PushOver::run_config`). The repository goes by the project's link and by each of its remotes that is the same
/// repository. None without a GitHub or Bitbucket link: a push to another git URL goes as its address says. Err: Push
/// over HTTPS without gh. Finding gh can start a login shell: blocking.
pub(crate) fn run_git_config(st: &AppState, project: &gizai_core::model::Project, wt: &Path) -> Result<Vec<(String, String)>, String> {
    let Some(link) = project.repo_url.as_deref().and_then(|u| gizai_core::repo_url::normalize(u).ok().flatten()) else { return Ok(vec![]) };
    let Some(full_name) = link.full_name() else { return Ok(vec![]) };
    let over = push_over_for(st, &link.provider)?;
    let remotes = worktree::remote_urls(wt).into_iter().map(|(_, url)| url).filter(|url| gizai_core::repo_url::same_repo(url, &link.url));
    let addresses: Vec<String> = std::iter::once(link.url.clone()).chain(remotes).collect();
    Ok(over.run_config(&full_name, &addresses, &worktree::own_rewrites(wt)))
}

/// Finds `gh` and saves what it finds: on Linux and macOS the way a login shell would, then in the usual install places;
/// on Windows on PATH (`gh.exe`), then where its installer puts it.
pub fn detect_gh(st: &AppState) -> Option<String> {
    let found = if cfg!(windows) {
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
        gizai_agents::os::find_in("gh", &std::env::var_os("PATH").unwrap_or_default())
            .or_else(|| Some(program_files.join("GitHub CLI").join("gh.exe")).filter(|p| crate::runs::executable(p)))
            .map(|p| p.display().to_string())
    } else {
        let from_shell = std::process::Command::new("bash").args(["-lc", "command -v gh"]).output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|p| !p.is_empty() && crate::runs::executable(Path::new(p)));
        let home = gizai_core::clis::home();
        from_shell.or_else(|| {
            [".local/bin/gh", ".local/share/mise/shims/gh", "bin/gh"].iter().map(|rel| format!("{home}/{rel}"))
                .chain(["/usr/local/bin/gh", "/usr/bin/gh", "/home/linuxbrew/.linuxbrew/bin/gh", "/snap/bin/gh"].map(String::from))
                .chain(cfg!(target_os = "macos").then(|| "/opt/homebrew/bin/gh".to_string()))
                .find(|p| crate::runs::executable(Path::new(p)))
        })
    };
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
        Some(b) => crate::runs::program_at(&b).ok_or_else(|| format!("The GitHub CLI isn't at {b} any more: fix its path in Settings")),
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

/// Open pull request, for a card in Review: pushes the card's branch (through your remote for the project's link, else
/// to the link itself), to GitHub over SSH with your keys or over HTTPS with gh's login as Settings → GitHub says, to
/// Bitbucket over SSH with your keys; then opens a pull request into the project's main branch (with gh, or Bitbucket's
/// API and your login), titled and described after the card. A branch that already has an open pull request (an agent
/// opened one) keeps it, and the push adds any new commits to it. A failed push records nothing.
pub async fn open(st: &AppState, task_id: &str) -> Result<PullInfo, String> {
    let card = pulls::card(&st.db, task_id).map_err(|e| e.to_string())?;
    if card.category != "review" {
        return Err(format!("{} isn't in Review: a pull request is opened from Review", card.identifier));
    }
    if is_live(st, task_id) {
        return Err(format!("An agent is working on {}: open the pull request when its run has ended", card.identifier));
    }
    let via = via(st, &card).await?;
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let _busy = Busy::take(st, task_id).ok_or_else(|| format!("Gizai is already busy with {}'s pull request", card.identifier))?;
    let (title, body) = (format!("{}: {}", task.identifier, task.title), body(&task));
    let over = match &via {
        Via::GitHub(gh) => crate::github::push_over(st, gh),
        Via::Bitbucket(_) => PushOver::Bitbucket,
    };
    let c = card.clone();
    let (info, created) = tokio::task::spawn_blocking(move || open_blocking(&via, &c, &title, &body, &over)).await.map_err(|e| e.to_string())??;
    // one an agent opened earlier is noted as seen by Gizai, not as opened by you
    pulls::record(&st.db, created.then_some(st.you_id.as_str()), task_id, &info.url, &info.state, created).map_err(|e| e.to_string())?;
    (st.notify)(Note::RowsChanged("tasks"));
    Ok(info)
}

/// Pushes and opens (or keeps the open pull request); returns it, and whether Gizai opened it now.
fn open_blocking(via: &Via, card: &PrCard, title: &str, body: &str, over: &PushOver) -> Result<(PullInfo, bool), String> {
    let repo = Path::new(&card.repo_path);
    if worktree::rev_parse(repo, &format!("refs/heads/{}", card.branch)).is_err() {
        return Err(format!("{}'s branch {} isn't in {} any more", card.identifier, card.branch, card.repo_path));
    }
    worktree::push_branch_over(repo, &push_to(repo, &card.repo_url), &card.branch, over).map_err(plain)?;
    let open = via.pulls_for_branch(card)?.into_iter().find(|p| p.state == "OPEN");
    let (url, state, created) = match open {
        Some(p) => (p.url.clone(), p.state().to_string(), false),
        None => (via.create_pull(card, title, body)?, "open".to_string(), true),
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

/// Check now (the page of a card the PR check follows): asks GitHub or Bitbucket about the card's pull request at once
/// (on GitHub one light call for its branch), without waiting for git, so one an agent opened shows straight away and a
/// merge moves the card. Returns the card's pull request, if it has one.
pub async fn check(st: &AppState, task_id: &str) -> Result<Option<PullInfo>, String> {
    let card = pulls::card(&st.db, task_id).map_err(|e| e.to_string())?;
    if !card.followed() || is_live(st, task_id) {
        return Ok(known(&card));
    }
    let via = via(st, &card).await?;
    let Some(_busy) = Busy::take(st, task_id) else { return Ok(known(&card)) };
    let (st2, c) = (st.clone(), card.clone());
    let checked = tokio::task::spawn_blocking(move || {
        let prs = via.pulls_for_branch(&c)?;
        check_blocking(&st2, &via, &c, &prs)
    }).await.map_err(|e| e.to_string())??;
    forget_error(st, task_id);
    if checked.changed {
        (st.notify)(Note::RowsChanged("tasks"));
    }
    Ok(checked.pull)
}

/// Checks the card's pull request in the background (a run just moved the card to Review). Nothing for a card
/// without a GitHub or Bitbucket link, or without a branch.
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

/// One round of the PR check, now (see `check_all_at`).
pub async fn check_all(st: &AppState) -> Vec<Checked> {
    check_all_at(st, Instant::now()).await
}

/// One round of the PR check as at `now` (a test's round ten minutes on): every card it follows (`PrCard::followed`)
/// that no agent is working on, a repository at a time. git first: one `git ls-remote` of the repository, for its main
/// branch, each card's branch and GitHub's pull request refs, asked the way Gizai's push after a run goes. GitHub or
/// Bitbucket is asked only when that answer differs from the one at the repository's last call (a merge moved main, an
/// agent pushed, a pull request was opened), a card is followed there that the last call didn't cover, the last call is
/// `ASK_AT_LEAST_EVERY` old, or git couldn't answer (the log says why, once). Then GitHub gets one light call for the
/// repository's newest pull requests (`github::pulls_of_repo`), and a card its own only when the pull request it follows
/// isn't among them; Bitbucket gets each card's own call, as before. What a round asked with is kept in memory, so after
/// a restart the first round asks. Without such cards it never starts gh or asks Bitbucket. gh is found only for GitHub
/// cards and the Bitbucket login read only for Bitbucket ones, once a round; without it, that side's cards wait for the
/// next round (the log says why, once).
pub async fn check_all_at(st: &AppState, now: Instant) -> Vec<Checked> {
    let cards: Vec<PrCard> = pulls::to_check(&st.db).unwrap_or_default().into_iter().filter(|c| !is_live(st, &c.task_id)).collect();
    if cards.is_empty() {
        return vec![];
    }
    // per provider: what reaches its pull requests this round (None: nothing does)
    let mut reach: HashMap<String, Option<Via>> = HashMap::new();
    let mut out = vec![];
    for (key, cards) in by_repository(cards) {
        let (provider, repo) = (cards[0].provider.clone(), cards[0].repo.clone());
        let via = match reach.get(&provider) {
            Some(via) => via.clone(),
            None => {
                let via = match provider.as_str() {
                    "bitbucket" => match bitbucket_login(st).await {
                        Ok(login) => { forget_error(st, "bitbucket"); Some(Via::Bitbucket(login)) }
                        Err(e) => { log_once(st, "bitbucket", &format!("the pull request check can't follow Bitbucket's pull requests: {e}")); None }
                    },
                    _ => match gh(st).await {
                        Ok(g) => { forget_error(st, ""); Some(Via::GitHub(g)) }
                        Err(e) => { log_once(st, "", &format!("the pull request check can't run: {e}")); None }
                    },
                };
                reach.insert(provider.clone(), via.clone());
                via
            }
        };
        let Some(via) = via else { continue };
        // git first
        let refs = ask_git(st, &cards).await;
        let git_key = format!("git {key}");
        match &refs {
            Ok(_) => forget_error(st, &git_key),
            Err(e) => {
                let host = gizai_core::repo_url::provider_name(&provider).unwrap_or("GitHub");
                log_once(st, &git_key, &format!("the pull request check can't ask git about {repo}, so it asks {host} every round: {e}"));
            }
        }
        let followed: BTreeSet<(String, String)> = cards.iter().map(|c| (c.task_id.clone(), c.branch.clone())).collect();
        if !due(st.pulls.asked.lock().unwrap().get(&key), &refs, &followed, now) {
            continue;
        }
        // GitHub: the repository's newest pull requests, in one light call
        let listed = match &via {
            Via::GitHub(gh) => {
                let (gh, dir, r) = (gh.clone(), PathBuf::from(&cards[0].repo_path), repo.clone());
                match tokio::task::spawn_blocking(move || github::pulls_of_repo(&gh, &dir, &r)).await.map_err(|e| e.to_string()).and_then(|l| l) {
                    Ok(list) => {
                        forget_error(st, &key);
                        Some(list)
                    }
                    Err(e) => {
                        log_once(st, &key, &format!("checking the pull requests of {repo} failed: {e}"));
                        continue;
                    }
                }
            }
            Via::Bitbucket(_) => None,
        };
        // the cards this call covers: those checked without a problem (one Open pull request or Check now holds is asked
        // about again next round)
        let mut covered = BTreeSet::new();
        for card in cards {
            let id = (card.task_id.clone(), card.branch.clone());
            let Some(_busy) = Busy::take(st, &card.task_id) else { continue };
            // its pull requests are those from its branch in the list, unless the one it follows is older than the list
            let (prs, own_call) = match &listed {
                Some(list) => (list.iter().filter(|p| p.head_ref_name == card.branch).cloned().collect(),
                               card.pr_url.as_deref().is_some_and(|u| !list.iter().any(|p| p.url == u))),
                None => (vec![], true),
            };
            let (st2, c, via) = (st.clone(), card.clone(), via.clone());
            let checked = tokio::task::spawn_blocking(move || {
                let prs = if own_call { via.pulls_for_branch(&c)? } else { prs };
                check_blocking(&st2, &via, &c, &prs)
            }).await.map_err(|e| e.to_string()).and_then(|c| c);
            match checked {
                Ok(checked) => {
                    covered.insert(id);
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
                Err(e) => log_once(st, &card.task_id, &format!("checking the pull request of {} failed: {e}", card.identifier)),
            }
        }
        st.pulls.asked.lock().unwrap().insert(key, Asked { at: now, refs: refs.ok(), cards: covered });
    }
    out
}

/// The cards by repository, in the order they come: (key, cards), the key being the provider and "owner/name".
fn by_repository(cards: Vec<PrCard>) -> Vec<(String, Vec<PrCard>)> {
    let mut groups: Vec<(String, Vec<PrCard>)> = vec![];
    for card in cards {
        let key = format!("{}:{}", card.provider, card.repo.to_lowercase());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, group)) => group.push(card),
            None => groups.push((key, vec![card])),
        }
    }
    groups
}

/// What git's `ls-remote` says of the repository of `cards` (they share it): its main branch, each card's branch and on
/// GitHub its pull request refs, as (ref, commit id) pairs. It asks the way Gizai's push after a run goes (`push_to`,
/// `push_over_for`), so a private repository answers too, and it never asks for anything.
async fn ask_git(st: &AppState, cards: &[PrCard]) -> Result<Vec<(String, String)>, String> {
    let (repo, link, provider) = (PathBuf::from(&cards[0].repo_path), cards[0].repo_url.clone(), cards[0].provider.clone());
    let mut refs: Vec<String> = cards.iter().flat_map(|c| [&c.default_branch, &c.branch]).map(|b| format!("refs/heads/{b}")).collect();
    // Bitbucket Cloud has no refs for its pull requests
    if provider == "github" {
        refs.push(PULL_REFS.to_string());
    }
    refs.sort();
    refs.dedup();
    let st = st.clone();
    tokio::task::spawn_blocking(move || {
        let over = push_over_for(&st, &provider)?;
        worktree::ls_remote(&repo, &push_to(&repo, &link), &refs, &over, ASK_GIT_LIMIT).map_err(plain)
    }).await.map_err(|e| e.to_string())?
}

/// Whether a round asks GitHub or Bitbucket about a repository, with git's answer `refs` and the cards it follows there
/// (`followed`): when it wasn't asked since Gizai started, git couldn't answer, git's answer differs from the one at its
/// last call, a card is followed that the last call didn't cover, or that call is `ASK_AT_LEAST_EVERY` old.
fn due(last: Option<&Asked>, refs: &Result<Vec<(String, String)>, String>, followed: &BTreeSet<(String, String)>, now: Instant) -> bool {
    let (Some(last), Ok(refs)) = (last, refs) else { return true };
    last.refs.as_ref() != Some(refs) || !followed.is_subset(&last.cards)
        || now.saturating_duration_since(last.at) + ON_TIME >= ASK_AT_LEAST_EVERY
}

/// Brings the card up to date with its pull requests `prs` (newest first, as GitHub or Bitbucket has them now) and the
/// one to show: an open one first, else the one Gizai follows, else the newest. A merge moves a Review card to Review's
/// next column (`pulls::merged`) and cleans up, once: when Gizai followed that pull request, or when it has the
/// branch's latest commit (so a card reopened after an earlier merge stays where it is). Whether it has that commit is
/// asked once (`Via::has_tip`), for the move, the card's Changes and the clean-up.
fn check_blocking(st: &AppState, via: &Via, card: &PrCard, prs: &[PullRequest]) -> Result<Checked, String> {
    let repo = Path::new(&card.repo_path);
    let followed = card.pr_url.as_deref().and_then(|u| prs.iter().find(|p| p.url == u));
    let mut out = Checked { task_id: card.task_id.clone(), pull: known(card), moved_to: None, changed: false };
    let Some(pr) = prs.iter().find(|p| p.state == "OPEN").or(followed).or(prs.first()) else { return Ok(out) };
    let state = pr.state();
    let mut info = PullInfo { url: pr.url.clone(), number: Some(pr.number), state: state.to_string(), note: None };
    let seen = card.pr_url.as_deref() == Some(pr.url.as_str());
    if state == "merged" && !(seen && card.pr_state.as_deref() == Some("merged")) {
        let tip = worktree::rev_parse(repo, &format!("refs/heads/{}", card.branch)).ok();
        let tip_in = match tip.as_deref() {
            Some(t) => via.has_tip(card, pr, t)?,
            None => false,
        };
        if seen || tip_in {
            out.moved_to = pulls::merged(&st.db, &st.you_id, &card.task_id, &pr.url).map_err(|e| e.to_string())?;
            // the card's range, so its Changes still show the whole card once its worktree and branch are gone (GA-106)
            crate::changes::save_at_merge(st, card, pr, tip.as_deref().filter(|_| tip_in));
            let sentence = clean_up(st, card, tip.as_deref(), tip_in);
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

/// After a merge: removes the card's worktree with its runs' temp folder (`worktree::remove_card_worktree_with`: only
/// one of Gizai's, never by force, so one with uncommitted changes stays) and deletes its branch when the pull request
/// has its latest commit `tip` (`tip_in`). Returns what it did as one sentence for the card's activity ("" when there was
/// nothing to clean up).
fn clean_up(st: &AppState, card: &PrCard, tip: Option<&str>, tip_in: bool) -> String {
    let removal = worktree::remove_card_worktree_with(Path::new(&card.repo_path), &st.data_dir.join("worktrees"), &st.temp_base,
                                                      &card.branch, tip_in);
    let mut said = removal.phrases(&card.branch);
    if tip.is_some() && !tip_in && removal.kept.is_none() {
        said.push(format!("kept branch {}, which has commits the pull request doesn't", card.branch));
    }
    match said.len() {
        0 => String::new(),
        1 => format!("{} after the merge", said[0]),
        n => format!("{} and {} after the merge", said[..n - 1].join(", "), said[n - 1]),
    }
}
