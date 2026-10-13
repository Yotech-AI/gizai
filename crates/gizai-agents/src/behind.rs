//! Behind main, for a folder open in the Editor (GA-110; contract C3 with the Editor's behind bar, F3): how the folder
//! stands against main (`stand`: local only, nothing is fetched), where main is for a folder that isn't a project's
//! (its origin remote's default branch: `origin_main`, fetched with `fetch_origin_main`), when main was last fetched
//! (`fetched_at`), and Update: what it will do or why it can't (`plan`), and how it ended (`ended`).
//! - Update is the update of a project's checkout (`checkout`), with its rules: a fast-forward of the default branch to
//!   main and nothing else (no merge commit, reset or force; git's hooks off); never over uncommitted changes in
//!   tracked files, during a merge, rebase, cherry-pick or revert, or when the default branch has commits main
//!   doesn't. In the Editor it never switches branch, and it installs (composer install, npm ci) only for a lock file
//!   the fast-forward changes; never the setup command.
//! - The words are D2's design (§5): the folder by its name, and a refusal as the sentence after "Update can't run:".
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::AgentError;
use crate::checkout::{self, Plan, Refusal};
use crate::worktree::{self, git};

/// How a folder stands against main (contract C3's `FolderMain`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderMain {
    /// How the screen names main: "origin/main". "" when the folder has no main to compare with: it isn't a git
    /// repository, or there is nowhere to fetch main from (`update_says` says which).
    pub main_label: String,
    /// When main was last fetched, in ms since 1970; None: never, as far as Gizai can tell.
    pub fetched_at: Option<u64>,
    /// Why Gizai's last fetch of main failed, while no fetch has worked since.
    pub fetch_error: Option<String>,
    /// The branch the folder is on; None on no branch (a detached HEAD), or outside git.
    pub branch: Option<String>,
    /// It is on the default branch, the one Update moves.
    pub on_default: bool,
    /// Commits main has that the folder doesn't.
    pub behind: u32,
    /// Commits the folder has that main doesn't.
    pub ahead: u32,
    /// Files changed and not committed, untracked ones too (as the Editor's git status counts them).
    pub uncommitted: usize,
    /// Update can run now.
    pub can_update: bool,
    /// What Update will do, in the plan's words ("move main 3 commits to 319bf0b and run npm ci"); else why it can't,
    /// as the sentence after "Update can't run:"; while an update runs, what it does.
    pub update_says: String,
    /// An update of the folder runs now: from the Editor, or from the Team Lead's chat.
    pub updating: bool,
}

/// How an update of a folder ended, for the screen (contract C3's `EditorUpdated`, the `editor-updated` event).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorUpdated {
    /// The folder (its `EditorFolder` id).
    pub folder: String,
    pub ok: bool,
    /// What it did ("Moved main 3 commits to 319bf0b and ran npm ci."), or why it stopped or didn't start.
    pub said: String,
    /// What git and the installs printed, for Show output; "" when nothing.
    pub output: String,
}

/// Where a folder's main is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Main {
    /// The ref: "refs/remotes/origin/main".
    pub rev: String,
    /// As the screen names it: "origin/main".
    pub label: String,
    /// The default branch: the one fetched, and the one Update moves to main ("main").
    pub branch: String,
}

/// A folder open in the Editor, as Update's words name it.
#[derive(Debug, Clone, Copy)]
pub struct Named<'a> {
    /// Where it is (its `EditorFolder` id).
    pub dir: &'a Path,
    /// Its name: "kade-portal", or a card's identifier.
    pub name: &'a str,
    /// A card's worktree, which is always on the card's own branch.
    pub card: bool,
}

/// What Gizai saw of its own fetches of a repository's main.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fetched {
    /// When a fetch last worked.
    pub ok_at: Option<SystemTime>,
    /// When a fetch last failed, and why: a sentence ("Couldn't fetch main from origin: …").
    pub failed: Option<(SystemTime, String)>,
}

/// How an update of a folder ended (`ended`).
#[derive(Debug, Clone)]
pub enum End {
    /// It ran to the end: the plan it carried out, and what `checkout::update` did.
    Done(Plan, checkout::Updated),
    /// It stopped part-way.
    Stopped(Plan, Box<checkout::Stopped>),
    /// The checks refused it when its turn came: why, as the sentence after "Update can't run:".
    Refused(String),
}

/// Whether `dir` is a folder in a git repository's work tree.
pub fn is_git(dir: &Path) -> bool {
    dir.is_dir() && git(dir, &["rev-parse", "--is-inside-work-tree"]).is_ok_and(|o| o.trim() == "true")
}

/// The branch origin's HEAD points to, as the repository of `dir` last heard (`refs/remotes/origin/HEAD`, which git
/// clone sets); None when it hasn't heard.
fn origin_head(dir: &Path) -> Option<String> {
    let head = git(dir, &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"]).ok()?;
    head.trim().strip_prefix("refs/remotes/origin/").filter(|b| !b.is_empty()).map(str::to_string)
}

/// Main for a folder that isn't a project's: its origin remote's default branch as last fetched
/// (`refs/remotes/origin/<branch>`). The branch is the one origin's HEAD points to; a repository that hasn't heard
/// which that is (until `fetch_origin_main` asks origin) takes main, else master, whichever it has from origin. None
/// without an origin remote.
pub fn origin_main(dir: &Path) -> Option<Main> {
    git(dir, &["config", "--get", "remote.origin.url"]).ok()?;
    let branch = origin_head(dir)
        .or_else(|| ["main", "master"].into_iter().find(|b| has(dir, &format!("refs/remotes/origin/{b}"))).map(str::to_string))
        .unwrap_or_else(|| "main".to_string());
    Some(Main { rev: format!("refs/remotes/origin/{branch}"), label: format!("origin/{branch}"), branch })
}

/// Fetches main for a folder that isn't a project's: its origin remote's default branch, into
/// `refs/remotes/origin/<branch>`, the way a card start fetches (`worktree::fetch_start`): never asking for a password,
/// and giving up after `limit` in all. When the repository hasn't heard which branch that is, origin is asked first and
/// the answer kept as `refs/remotes/origin/HEAD` (as git clone keeps it), so the next look, which is local, knows it.
/// An error is a sentence ("Couldn't fetch main from origin: …").
pub fn fetch_origin_main(dir: &Path, limit: Duration) -> Result<Main, String> {
    let until = Instant::now() + limit;
    let known = origin_head(dir);
    let branch = match &known {
        Some(b) => b.clone(),
        None => {
            let guess = origin_main(dir).map(|m| m.branch).unwrap_or_else(|| "main".to_string());
            worktree::remote_head(dir, "origin", limit).map_err(|e| format!("Couldn't fetch {guess} from origin: {e}"))?.unwrap_or(guess)
        }
    };
    let left = until.saturating_duration_since(Instant::now()).max(Duration::from_secs(1));
    let rev = worktree::fetch_start(dir, Some("origin"), "origin", &branch, left).map_err(said)?;
    if known.is_none() {
        let _ = git(dir, &["remote", "set-head", "origin", &branch]);
    }
    Ok(Main { rev, label: format!("origin/{branch}"), branch })
}

/// When main's `branch` was last fetched into the repository of `dir`, by Gizai or by you: when git last wrote
/// FETCH_HEAD with that branch in it, in the folder's own git folder or in the repository's main one (where card
/// starts fetch). None when it can't tell.
pub fn fetched_at(dir: &Path, branch: &str) -> Option<SystemTime> {
    let want = format!("\tbranch '{branch}' of ");
    let paths = git(dir, &["rev-parse", "--path-format=absolute", "--git-path", "FETCH_HEAD", "--git-common-dir"]).ok()?;
    let mut lines = paths.lines();
    let own = lines.next().map(PathBuf::from);
    let common = lines.next().map(|c| Path::new(c).join("FETCH_HEAD"));
    [own, common].into_iter().flatten()
        .filter(|f| std::fs::read_to_string(f).is_ok_and(|text| text.contains(&want)))
        .filter_map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
        .max()
}

/// How many commits `main` (a ref) has that HEAD doesn't, and HEAD has that main doesn't: (behind, ahead). (0, 0) when
/// either isn't there.
pub fn counts(dir: &Path, main: &str) -> (u32, u32) {
    let Ok(out) = git(dir, &["rev-list", "--left-right", "--count", &format!("{main}...HEAD"), "--"]) else { return (0, 0) };
    let mut n = out.split_whitespace().map(|w| w.parse().unwrap_or(0));
    (n.next().unwrap_or(0), n.next().unwrap_or(0))
}

/// Whether `rev` names a commit in the repository of `dir`.
fn has(dir: &Path, rev: &str) -> bool {
    git(dir, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")]).is_ok()
}

/// How the folder `f` stands against `main` (local only: nothing is fetched). `main`: where main is, or why the folder
/// has none (the sentence after "Update can't run:"). `seen`: what Gizai saw of its own fetches of main in this
/// repository. `updating`: what the update running in the folder does, when one runs. `read_only`: why nothing in the
/// folder may change now ("Backend Agent is working on GA-12").
pub fn stand(f: Named, main: Result<&Main, &str>, seen: &Fetched, updating: Option<&str>, read_only: Option<&str>) -> FolderMain {
    if !is_git(f.dir) {
        return FolderMain { update_says: not_git(f), ..Default::default() };
    }
    let status = crate::editor::status(f.dir);
    let mut out = FolderMain { branch: status.branch, uncommitted: status.uncommitted, ..Default::default() };
    let main = match main {
        Ok(m) => m,
        Err(why) => {
            out.update_says = why.to_string();
            return out;
        }
    };
    out.main_label = main.label.clone();
    out.on_default = out.branch.as_deref() == Some(main.branch.as_str());
    let at = [seen.ok_at, fetched_at(f.dir, &main.branch)].into_iter().flatten().max();
    out.fetched_at = at.and_then(ms);
    out.fetch_error = seen.failed.as_ref().filter(|(when, _)| at.is_none_or(|a| a < *when)).map(|(_, why)| why.clone());
    if has(f.dir, &main.rev) {
        (out.behind, out.ahead) = counts(f.dir, &main.rev);
    }
    out.update_says = match updating {
        Some(doing) => {
            out.updating = true;
            doing.to_string()
        }
        None => match plan(f, Ok(main), read_only) {
            Ok(p) => {
                out.can_update = true;
                p.said()
            }
            Err(why) => why,
        },
    };
    out
}

/// Update's plan for the folder `f`: what it will do (`Plan::said`), or why it can't, as the sentence after "Update
/// can't run:". It can't outside git, without a main (`main` says why), when main hasn't been fetched, while
/// `read_only` says nothing in the folder may change, when `checkout::check` refuses without switching branch, and when
/// there is nothing to move (the folder is up to date). It installs only for a lock file the fast-forward changes
/// (`Plan::only_changed_locks`). Nothing changes here.
pub fn plan(f: Named, main: Result<&Main, &str>, read_only: Option<&str>) -> Result<Plan, String> {
    if !is_git(f.dir) {
        return Err(not_git(f));
    }
    let main = main.map_err(str::to_string)?;
    if !has(f.dir, &main.rev) {
        return Err(refused(f, main, &Refusal::NotFetched));
    }
    if let Some(why) = read_only {
        return Err(full_stop(why));
    }
    // never a switch, never a stash: uncommitted changes in tracked files keep it from running
    let plan = checkout::check(f.dir, f.dir, &main.branch, &main.rev, false, false).map_err(|r| refused(f, main, &r))?;
    if plan.commits == 0 {
        return Err(format!("{} is up to date with {}.", f.name, main.label));
    }
    Ok(plan.only_changed_locks(f.dir))
}

/// "kade-portal isn't a git repository, …".
fn not_git(f: Named) -> String {
    format!("{} isn't a git repository, so there's no main to compare it with.", f.name)
}

/// `checkout::check`'s refusal as the Editor says it, after "Update can't run:" (D2's design, §5). The Editor never
/// switches branch, so on another branch it says to switch first, in a terminal or your own editor; a card's worktree
/// is always on the card's own branch.
pub fn refused(f: Named, main: &Main, r: &Refusal) -> String {
    let (name, default, label) = (f.name, main.branch.as_str(), main.label.as_str());
    let yourself = "in a terminal or your own editor";
    match r {
        Refusal::NotACheckout => format!("{name} isn't the top folder of its git repository, and Update only runs there."),
        Refusal::InProgress(what) => format!("{what} is in progress in {name}. Finish or abort it first."),
        Refusal::Uncommitted(files) => format!("{name} has uncommitted changes in {}. Commit or stash them first.", files_said(files)),
        Refusal::OtherBranch(Some(b)) if f.card => {
            format!("{name} is a card's worktree, on the card's branch {b}: Update only brings a folder on {default} up to date.")
        }
        Refusal::OtherBranch(Some(b)) => format!("{name} is on branch {b}, not {default}. Switch to {default} first, {yourself}."),
        Refusal::OtherBranch(None) => format!("{name} has no branch checked out. Switch to {default} first, {yourself}."),
        Refusal::NotFetched => format!("{label} hasn't been fetched into {name} yet. Fetch it first."),
        Refusal::NoDefaultBranch => format!("{name} has no {default} branch."),
        Refusal::Ahead(n) => format!("{default} in {name} has {n} commit{} that {label} doesn't, so a fast-forward can't bring it up to \
                                     date. Push or merge it first, {yourself}.", if *n == 1 { "" } else { "s" }),
        Refusal::Git(said) => {
            let last = said.trim().lines().last().unwrap_or("it failed").trim();
            full_stop(&format!("git failed in {name}: {}", last.strip_prefix("fatal: ").unwrap_or(last)))
        }
    }
}

/// How the update of the folder `name` (its `EditorFolder` id `folder`) ended, for the `editor-updated` event: what it
/// did ("Moved main 3 commits to 319bf0b and ran npm ci."), else why it stopped part-way and what it did before, or why
/// it didn't start (D2's design, §5); with what git and the installs printed.
pub fn ended(folder: &str, name: &str, end: End) -> EditorUpdated {
    let (ok, said, output) = match end {
        End::Done(plan, u) => {
            let mut did = vec![moved(&plan)];
            if !u.installed.is_empty() {
                did.push(format!("ran {}", u.installed.join(" and ")));
            }
            (true, capitalized(&full_stop(&list(&did))), u.printed)
        }
        End::Stopped(plan, s) => {
            let mut did = vec![];
            if s.moved {
                did.push(moved(&plan));
            }
            if !s.installed.is_empty() {
                did.push(format!("ran {}", s.installed.join(" and ")));
            }
            let before = if did.is_empty() { String::new() } else { format!(" Before that it {}.", list(&did)) };
            (false, format!("The update of {name} stopped: {}{before}", full_stop(&s.why)), s.printed)
        }
        End::Refused(why) => (false, format!("The update of {name} didn't start: {}", full_stop(&why)), String::new()),
    };
    EditorUpdated { folder: folder.to_string(), ok, said, output }
}

/// "moved main 3 commits to 319bf0b".
fn moved(plan: &Plan) -> String {
    let to = &plan.to[..plan.to.len().min(7)];
    match plan.commits {
        0 => format!("left {} at {to}", plan.default_branch),
        n => format!("moved {} {n} commit{} to {to}", plan.default_branch, if n == 1 { "" } else { "s" }),
    }
}

/// The uncommitted files a refusal names: "a, b and c"; after ten, "and 5 more".
fn files_said(files: &[String]) -> String {
    if files.len() > 10 {
        format!("{} and {} more", files[..10].join(", "), files.len() - 10)
    } else {
        list(files)
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

/// The sentence with a full stop at its end.
fn full_stop(s: &str) -> String {
    let s = s.trim_end();
    if s.ends_with(['.', '!', '?']) { s.to_string() } else { format!("{s}.") }
}

/// The sentence with a capital letter first.
fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// ms since 1970.
fn ms(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// git's message without the "git: " in front.
fn said(e: AgentError) -> String {
    match e {
        AgentError::Git(m) => m,
        other => other.to_string(),
    }
}
