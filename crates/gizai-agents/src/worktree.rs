//! One git worktree per task, on its own branch, so agents never touch the main checkout.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use crate::connection::{self, Problem, PushOver};
use crate::{AgentError, github};

#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    /// Made (or taken over) just now, for this card.
    pub created: bool,
    /// Set when it is a finished card's worktree taken over (`reuse`).
    pub reused: Option<Reused>,
}

/// Where a taken-over worktree comes from.
#[derive(Debug, Clone, PartialEq)]
pub struct Reused {
    /// Its folder before it moved.
    pub from: PathBuf,
    /// The commit it had checked out (the finished card's): its dependency folders were installed for that one.
    pub head: String,
}

/// Lowercase ASCII letters, digits and single dashes, at most `max` characters. Common accents are
/// folded ("Café" → "cafe", "Straße" → "strasse").
pub fn slug(title: &str, max: usize) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(char::to_lowercase) {
        let folded: &str = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a", 'ç' => "c", 'è' | 'é' | 'ê' | 'ë' => "e",
            'ì' | 'í' | 'î' | 'ï' => "i", 'ñ' => "n", 'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
            'ù' | 'ú' | 'û' | 'ü' => "u", 'ý' | 'ÿ' => "y", 'ß' => "ss",
            c if c.is_ascii_alphanumeric() => { out.push(c); continue; }
            _ => "-",
        };
        out.push_str(folded);
    }
    let mut tidy = String::new();
    for part in out.split('-').filter(|p| !p.is_empty()) {
        if !tidy.is_empty() { tidy.push('-'); }
        tidy.push_str(part);
    }
    tidy.chars().take(max).collect::<String>().trim_end_matches('-').to_string()
}

pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<String, AgentError> {
    let out = Command::new("git").arg("-C").arg(repo).args(args).output()
        .map_err(|e| AgentError::Git(format!("can't run git: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(AgentError::Git(String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

/// (path, branch) for every worktree of the repo, its main checkout first.
pub(crate) fn worktrees(repo: &Path) -> Result<Vec<(PathBuf, Option<String>)>, AgentError> {
    let mut out = vec![];
    let mut cur: Option<PathBuf> = None;
    for line in git(repo, &["worktree", "list", "--porcelain"])?.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            if let Some(prev) = cur.take() { out.push((prev, None)); }
            cur = Some(PathBuf::from(p));
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            if let Some(p) = cur.take() { out.push((p, Some(b.to_string()))); }
        }
    }
    if let Some(p) = cur { out.push((p, None)); }
    Ok(out)
}

pub(crate) fn same(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// The branch a card works on: `gizai/<identifier>-<slug of its title>`.
pub fn branch_name(identifier: &str, title: &str) -> String {
    let s = slug(title, 40);
    if s.is_empty() { format!("gizai/{}", identifier.to_lowercase()) } else { format!("gizai/{}-{s}", identifier.to_lowercase()) }
}

/// The task's worktree at `base_dir/<identifier>` on branch `gizai/<identifier>-<slug>`: reused when it
/// already exists, else created (on the existing branch if there is one, or a new branch off `default_branch`).
pub fn ensure(repo: &Path, base_dir: &Path, identifier: &str, title: &str, default_branch: &str) -> Result<Worktree, AgentError> {
    match git(repo, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(s) if s == "true" => {}
        _ => return Err(AgentError::NotGitRepo(repo.to_path_buf())),
    }
    let path = base_dir.join(identifier);
    let branch = branch_name(identifier, title);
    let list = worktrees(repo)?;

    if path.exists() {
        return match list.iter().find(|(p, _)| same(p, &path)) {
            Some((_, b)) => Ok(Worktree { path, branch: b.clone().unwrap_or(branch), created: false, reused: None }),
            None => Err(AgentError::Git(format!("{} exists but is not a worktree of this repository", path.display()))),
        };
    }
    std::fs::create_dir_all(base_dir)?;
    let path_s = path.to_string_lossy().to_string();
    let exists = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok();
    if exists {
        if let Some((p, _)) = list.iter().find(|(_, b)| b.as_deref() == Some(branch.as_str())) {
            return Err(AgentError::Git(format!("branch {branch} is already checked out at {}", p.display())));
        }
        git(repo, &["worktree", "add", &path_s, &branch])?;
    } else {
        // --no-track: a branch started from a remote's main must not push to it
        git(repo, &["worktree", "add", "--no-track", "-b", &branch, &path_s, default_branch])?;
    }
    Ok(Worktree { path, branch, created: true, reused: None })
}

/// A card without a worktree takes over `from`, the worktree of a finished card, so its build stays warm. The folder
/// moves to `base_dir/<identifier>` and switches to the card's branch (made from `start` when the card has none yet).
/// Then nothing of the finished card is left (`git clean -dx`: untracked and ignored files go) except the paths in
/// `keep`, relative to the worktree: the copied folders and the installed dependencies, such as node_modules/ and
/// target/. Refuses a worktree with uncommitted changes or untracked files, and puts it back when switching fails.
pub fn reuse(repo: &Path, from: &Path, base_dir: &Path, identifier: &str, title: &str, start: &str, keep: &[String]) -> Result<Worktree, AgentError> {
    let path = base_dir.join(identifier);
    let branch = branch_name(identifier, title);
    if path.exists() {
        return Err(AgentError::Git(format!("{} already exists", path.display())));
    }
    let list = worktrees(repo)?;
    if let Some((p, _)) = list.iter().find(|(_, b)| b.as_deref() == Some(branch.as_str())) {
        return Err(AgentError::Git(format!("branch {branch} is already checked out at {}", p.display())));
    }
    if !list.iter().any(|(p, _)| same(p, from)) {
        return Err(AgentError::Git(format!("{} is not a worktree of this repository", from.display())));
    }
    if uncommitted(from)? > 0 {
        return Err(AgentError::Git(format!("{} has uncommitted changes", from.display())));
    }
    let head = git(from, &["rev-parse", "HEAD"])?;
    std::fs::create_dir_all(base_dir)?;
    git(repo, &["worktree", "move", &from.to_string_lossy(), &path.to_string_lossy()])?;
    let switched = if git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok() {
        git(&path, &["checkout", "--quiet", &branch])
    } else {
        // --no-track: a branch started from a remote's main must not push to it
        git(&path, &["checkout", "--quiet", "--no-track", "-b", &branch, start])
    };
    if let Err(e) = switched {
        let _ = git(repo, &["worktree", "move", &path.to_string_lossy(), &from.to_string_lossy()]);
        return Err(e);
    }
    let mut args: Vec<String> = ["clean", "-d", "-x", "--force", "--quiet"].map(String::from).to_vec();
    for k in keep {
        let k = k.trim().trim_start_matches("./").trim_matches('/');
        if !k.is_empty() {
            args.extend(["-e".to_string(), format!("/{k}")]);
        }
    }
    git(&path, &args.iter().map(String::as_str).collect::<Vec<_>>())?;
    Ok(Worktree { path, branch, created: true, reused: Some(Reused { from: from.to_path_buf(), head }) })
}

/// Gizai's note that a worktree still has to be prepared, in the worktree's own git folder (.git/worktrees/<name>):
/// out of git status's sight, and gone with the worktree.
const UNPREPARED: &str = "gizai-unprepared";

/// A worktree that still has to be prepared (see `mark_unprepared`).
#[derive(Debug, Clone, PartialEq)]
pub struct Unprepared {
    /// The commit its dependency folders were installed for, when it was taken over (`Reused::head`).
    pub since: Option<String>,
}

/// Notes that the worktree at `wt` still has to be prepared: it was just made or taken over. The note stays until
/// `mark_prepared`, so a preparation that failed runs again at the next start.
pub fn mark_unprepared(wt: &Path, since: Option<&str>) -> Result<(), AgentError> {
    let dir = PathBuf::from(git(wt, &["rev-parse", "--absolute-git-dir"])?);
    std::fs::write(dir.join(UNPREPARED), since.unwrap_or_default())?;
    Ok(())
}

/// Whether the worktree at `wt` still has to be prepared.
pub fn unprepared(wt: &Path) -> Option<Unprepared> {
    let dir = PathBuf::from(git(wt, &["rev-parse", "--absolute-git-dir"]).ok()?);
    let since = std::fs::read_to_string(dir.join(UNPREPARED)).ok()?;
    let since = since.trim();
    Some(Unprepared { since: (!since.is_empty()).then(|| since.to_string()) })
}

pub fn mark_prepared(wt: &Path) -> Result<(), AgentError> {
    let dir = PathBuf::from(git(wt, &["rev-parse", "--absolute-git-dir"])?);
    match std::fs::remove_file(dir.join(UNPREPARED)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// The folder at a card's worktree root where its runs keep throwaway files: TMPDIR, TMP and TEMP point there, so
/// `mktemp`, PHP, Node and Python put their temp files where the agent may write.
pub const TEMP_DIR: &str = ".gizai-tmp";
/// The line in the repository's `info/exclude` that keeps the temp folder out of git status.
pub const TEMP_EXCLUDE: &str = "/.gizai-tmp/";

/// One look-and-add of the exclude line at a time: runs of the same repository can start together.
static EXCLUDING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Gets the worktree's temp folder (`<wt>/.gizai-tmp`) ready for a run and returns it. First `/.gizai-tmp/` goes into
/// the repository's `info/exclude` (`exclude_temp`), then the folder is made, empty: a run Gizai couldn't follow to its
/// end (Gizai quit or crashed) can have left files in it. Something else by that name, such as a link, is removed,
/// never followed.
pub fn prepare_temp(wt: &Path) -> Result<PathBuf, AgentError> {
    let common = PathBuf::from(git(wt, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?);
    exclude_temp(&common)?;
    let dir = wt.join(TEMP_DIR);
    if std::fs::symlink_metadata(&dir).is_ok_and(|m| !m.is_dir()) {
        std::fs::remove_file(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    empty_temp(wt)?;
    Ok(dir)
}

/// Adds `/.gizai-tmp/` to `info/exclude` in the repository's common git folder `common` (shared by the main checkout
/// and every worktree, so none of them shows the folder), unless a line already says it. `.gitignore` is never touched.
pub fn exclude_temp(common: &Path) -> Result<(), AgentError> {
    let _one = EXCLUDING.lock().unwrap_or_else(|e| e.into_inner());
    let info = common.join("info");
    std::fs::create_dir_all(&info)?;
    let file = info.join("exclude");
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    if text.lines().any(|l| l.trim() == TEMP_EXCLUDE) {
        return Ok(());
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&file)?;
    let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    write!(f, "{sep}{TEMP_EXCLUDE}\n")?;
    Ok(())
}

/// Empties the worktree's temp folder once a run has ended, however it ended (finished, Stop, a limit). The folder
/// stays and goes with the worktree. Links in it are removed, never followed; one link or file in the folder's place
/// is removed. Goes on past what it can't remove, and returns the first error.
pub fn empty_temp(wt: &Path) -> Result<(), AgentError> {
    let dir = wt.join(TEMP_DIR);
    match std::fs::symlink_metadata(&dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
        Ok(m) if !m.is_dir() => return Ok(std::fs::remove_file(&dir)?),
        Ok(_) => {}
    }
    let mut first: Option<std::io::Error> = None;
    for entry in std::fs::read_dir(&dir)? {
        let done = entry.and_then(|e| {
            let path = e.path();
            if e.file_type()?.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) }
        });
        if let Err(e) = done {
            first.get_or_insert(e);
        }
    }
    first.map_or(Ok(()), |e| Err(e.into()))
}

/// The environment that points a run's temp files at `dir`: TMPDIR, TMP and TEMP.
pub fn temp_env(dir: &Path) -> Vec<(String, String)> {
    ["TMPDIR", "TMP", "TEMP"].iter().map(|k| (k.to_string(), dir.display().to_string())).collect()
}

/// Whether `path` is inside `dir` (Gizai's worktree folder, or one project's part of it), as they are on disk.
pub fn is_inside(path: &Path, dir: &Path) -> bool {
    inside(path, dir)
}

/// Whether every commit of `branch` is in `into` (a branch or ref), so deleting the branch loses nothing.
pub fn merged(repo: &Path, branch: &str, into: &str) -> bool {
    git(repo, &["merge-base", "--is-ancestor", &format!("refs/heads/{branch}"), into]).is_ok()
}

/// The repository's remotes: (name, fetch URL).
pub fn remotes(repo: &Path) -> Result<Vec<(String, String)>, AgentError> {
    let mut out = vec![];
    for line in git(repo, &["remote", "-v"])?.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(name), Some(url), Some("(fetch)")) = (parts.next(), parts.next(), parts.next()) {
            out.push((name.to_string(), url.to_string()));
        }
    }
    Ok(out)
}

/// The ref `fetch_start` fetches `branch` into: `refs/remotes/<remote>/<branch>` through the repository's remote, else
/// a ref git's branch list doesn't show (`refs/gizai/base/<branch>`).
pub fn start_ref(remote: Option<&str>, branch: &str) -> String {
    match remote {
        Some(r) => format!("refs/remotes/{r}/{branch}"),
        None => format!("refs/gizai/base/{branch}"),
    }
}

/// Fetches `branch` of the project's repository and returns the ref to start from (`start_ref`): through `remote` when
/// the repository has one for it, else from `url`. Never asks for a password; each try gives up after `limit`.
pub fn fetch_start(repo: &Path, remote: Option<&str>, url: &str, branch: &str, limit: Duration) -> Result<String, AgentError> {
    let from = remote.unwrap_or(url);
    let start = start_ref(remote, branch);
    let spec = format!("+refs/heads/{branch}:{start}");
    let mut last = String::new();
    // a run starting at the same moment may hold the ref's lock: one retry
    for attempt in 0..2 {
        match git_quiet(repo, &["fetch", "--quiet", "--no-tags", from, &spec], limit) {
            Ok(_) => return Ok(start),
            Err(e) => last = e,
        }
        if attempt == 0 { std::thread::sleep(Duration::from_millis(500)); }
    }
    Err(AgentError::Git(format!("Couldn't fetch {branch} from {from}: {last}")))
}

/// How many commits `start` has that the checkout in `dir` doesn't.
pub fn behind(dir: &Path, start: &str) -> Result<u32, AgentError> {
    git(dir, &["rev-list", "--count", &format!("HEAD..{start}")])?.trim().parse().map_err(|_| AgentError::Git("rev-list gave no count".into()))
}

pub fn rev_parse(dir: &Path, rev: &str) -> Result<String, AgentError> {
    git(dir, &["rev-parse", "--verify", "--quiet", rev])
}

/// A commit: its id and the first line of its message.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

/// The commits after `base` up to `head` (two commit ids), oldest first, as a pull request lists them. Only the
/// branch's own line counts (first parents): merging main in is one commit, not all of main's.
pub fn commits(dir: &Path, base: &str, head: &str) -> Result<Vec<Commit>, AgentError> {
    for id in [base, head] {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AgentError::Git(format!("{id:?} is not a commit id")));
        }
    }
    let out = git(dir, &["log", "--first-parent", "--reverse", "--format=%H%x1f%s", &format!("{base}..{head}")])?;
    Ok(out.lines().filter_map(|l| l.split_once('\x1f'))
        .map(|(sha, subject)| Commit { sha: sha.to_string(), subject: subject.to_string() }).collect())
}

/// How long a push may take.
const PUSH_LIMIT: Duration = Duration::from_secs(120);

/// Pushes the local `branch` to the branch of the same name at `to` (a remote's name, or a URL) over SSH with your
/// keys, the default (see `push_branch_over`).
pub fn push_branch(repo: &Path, to: &str, branch: &str) -> Result<(), AgentError> {
    push_branch_over(repo, to, branch, &PushOver::Ssh)
}

/// Pushes the local `branch` to the branch of the same name at `to` (a remote's name, or a URL): over SSH with your
/// keys, or over HTTPS with gh's login, or to Bitbucket over SSH (`over`, set for this one command: no git config or
/// remote changes). Never forces and never asks for anything; gives up after two minutes. A failure says what to
/// check, in plain words.
pub fn push_branch_over(repo: &Path, to: &str, branch: &str, over: &PushOver) -> Result<(), AgentError> {
    let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
    quiet(repo, &push_config(repo, to, over), &["push", "--quiet", to, &spec], PUSH_LIMIT)
        .map(|_| ())
        .map_err(|e| AgentError::Git(format!("Couldn't push {branch} to {to}: {}", e.problem(over))))
}

/// Whether you can push to `to` the way `push_branch_over` would, without pushing anything: a dry run of the
/// repository's latest commit to a new branch. It reaches GitHub (or Bitbucket) and needs write access, but sends
/// nothing and runs no hooks.
pub fn can_push(repo: &Path, to: &str, over: &PushOver, limit: Duration) -> Result<(), Problem> {
    let args = ["push", "--dry-run", "--quiet", "--no-verify", to, "HEAD:refs/heads/gizai-connection-check"];
    quiet(repo, &push_config(repo, to, over), &args, limit).map(|_| ()).map_err(|e| e.problem(over))
}

/// `over`'s settings for one push to `to`: for Bitbucket with the addresses `to` pushes to (`PushOver::git_config_for`),
/// so an address with a user name in it goes over SSH too.
fn push_config(repo: &Path, to: &str, over: &PushOver) -> Vec<String> {
    match over {
        PushOver::Bitbucket => over.git_config_for(&push_addresses(repo, to)),
        _ => over.git_config(),
    }
}

/// The addresses a push to `to` goes to, as written: `to` itself (an address), or the push addresses of the remote it
/// names (`remote.<name>.pushurl`, else its `url`).
fn push_addresses(repo: &Path, to: &str) -> Vec<String> {
    let mut out = vec![to.to_string()];
    for key in ["pushurl", "url"] {
        let found: Vec<String> = git(repo, &["config", "--get-all", &format!("remote.{to}.{key}")]).unwrap_or_default()
            .lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
        if !found.is_empty() {
            out.extend(found);
            break;
        }
    }
    out
}

/// Every branch that is checked out in a worktree of the repository, with that worktree.
pub fn checked_out(repo: &Path) -> Result<std::collections::HashMap<String, PathBuf>, AgentError> {
    Ok(worktrees(repo)?.into_iter().filter_map(|(p, b)| Some((b?, p))).collect())
}

/// The worktree that has `branch` checked out, if one does.
pub fn worktree_of(repo: &Path, branch: &str) -> Result<Option<PathBuf>, AgentError> {
    Ok(worktrees(repo)?.into_iter().find(|(_, b)| b.as_deref() == Some(branch)).map(|(p, _)| p))
}

/// Removes a worktree, never by force: git refuses one with uncommitted changes or untracked files, and says so. Its
/// temp folder goes with it: `info/exclude` makes it ignored, not untracked (`exclude_temp`).
pub fn remove_worktree(repo: &Path, path: &Path) -> Result<(), AgentError> {
    git(repo, &["worktree", "remove", &path.to_string_lossy()]).map(|_| ())
}

/// Deletes a local branch (git refuses one that is checked out somewhere).
pub fn delete_branch(repo: &Path, branch: &str) -> Result<(), AgentError> {
    git(repo, &["branch", "-D", branch]).map(|_| ())
}

/// What `remove_card_worktree` did with a card's worktree and branch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Removal {
    /// The worktree it removed.
    pub removed: Option<PathBuf>,
    /// The worktree it left in place, and why: it isn't one of Gizai's, or git refused (uncommitted changes).
    pub kept: Option<(PathBuf, String)>,
    /// Whether it deleted the branch.
    pub branch_deleted: bool,
    /// Why the branch stayed, when it was asked to delete it.
    pub branch_kept: Option<String>,
}

impl Removal {
    /// What it did, as short phrases for one sentence ("removed its worktree", "deleted branch gizai/gz-1-x").
    pub fn phrases(&self, branch: &str) -> Vec<String> {
        let mut out = vec![];
        if self.removed.is_some() {
            out.push("removed its worktree".to_string());
        }
        if let Some((path, why)) = &self.kept {
            out.push(format!("kept its worktree {} ({why})", path.display()));
        }
        if self.branch_deleted {
            out.push(format!("deleted branch {branch}"));
        }
        if let Some(why) = &self.branch_kept {
            out.push(format!("kept branch {branch} ({why})"));
        }
        out
    }
}

/// Removes a card's worktree and, with `delete_branch`, its branch. One function for every clean-up: after a card's
/// pull request is merged, and Settings → Data for the worktrees of finished cards.
/// - The worktree is the one that has `branch` checked out. Only one inside `worktrees_dir` (Gizai's own folder) is
///   removed, and never by force: git keeps one with uncommitted changes or untracked files. One whose folder is
///   already gone only leaves git's list.
/// - The branch goes only when no worktree has it checked out any more (a kept worktree keeps it). `git branch -D`
///   also deletes commits that aren't merged anywhere, so the caller decides whether that is fine.
pub fn remove_card_worktree(repo: &Path, worktrees_dir: &Path, branch: &str, delete_branch: bool) -> Removal {
    let mut out = Removal::default();
    let checked_out = match worktree_of(repo, branch) {
        Ok(None) => false,
        Ok(Some(wt)) if !inside(&wt, worktrees_dir) => {
            out.kept = Some((wt, "it isn't one of Gizai's worktrees".into()));
            true
        }
        Ok(Some(wt)) => match remove_worktree(repo, &wt) {
            Ok(()) => {
                out.removed = Some(wt);
                false
            }
            Err(e) => {
                out.kept = Some((wt, refusal(e)));
                true
            }
        },
        Err(e) => {
            if delete_branch {
                out.branch_kept = Some(refusal(e));
            }
            return out;
        }
    };
    // a kept worktree keeps its branch (and `kept` says why)
    if delete_branch && !checked_out {
        match self::delete_branch(repo, branch) {
            Ok(()) => out.branch_deleted = true,
            Err(e) => out.branch_kept = Some(refusal(e)),
        }
    }
    out
}

/// git's reason, in plain words where Gizai knows them.
fn refusal(e: AgentError) -> String {
    let msg = match e {
        AgentError::Git(m) => m,
        other => other.to_string(),
    };
    if msg.contains("modified or untracked files") {
        return "it has uncommitted changes".into();
    }
    let msg = msg.lines().last().unwrap_or("git failed").trim();
    msg.strip_prefix("fatal: ").or_else(|| msg.strip_prefix("error: ")).unwrap_or(msg).to_string()
}

/// Whether `path` is inside `dir`, as they are on disk. A folder that is gone counts by the nearest folder above it
/// that is still there.
fn inside(path: &Path, dir: &Path) -> bool {
    let Ok(dir) = dir.canonicalize() else { return false };
    let (mut here, mut rest) = (path.to_path_buf(), vec![]);
    while !here.exists() {
        match (here.file_name(), here.parent()) {
            (Some(name), Some(up)) => {
                rest.push(name.to_os_string());
                here = up.to_path_buf();
            }
            _ => return false,
        }
    }
    let Ok(mut real) = here.canonicalize() else { return false };
    for name in rest.into_iter().rev() {
        real.push(name);
    }
    real != dir && real.starts_with(&dir)
}

/// How many files in the checkout at `dir` have uncommitted changes (untracked ones included).
pub fn uncommitted(dir: &Path) -> Result<usize, AgentError> {
    Ok(git(dir, &["status", "--porcelain"])?.lines().filter(|l| !l.trim().is_empty()).count())
}

/// Why a git command that talks to a remote failed.
enum Failure {
    /// Everything git said.
    Said(String),
    NoAnswer(Duration),
    CantRun(String),
}

impl Failure {
    /// git's own last line.
    fn last_line(self) -> String {
        match self {
            Failure::Said(said) => said.trim().lines().last().unwrap_or("git failed").to_string(),
            Failure::NoAnswer(limit) => format!("no answer within {} s", limit.as_secs()),
            Failure::CantRun(e) => e,
        }
    }

    /// What it means for a push, in plain words.
    fn problem(self, over: &PushOver) -> Problem {
        match self {
            Failure::Said(said) => connection::push_problem(&said, over),
            Failure::NoAnswer(limit) => connection::push_no_answer(over, limit),
            Failure::CantRun(e) => Problem::plain(e),
        }
    }
}

/// git without prompts, ended after `limit`: no terminal and no askpass program, ssh in batch mode (unless you set your
/// own ssh command), and `config` as `-c` settings for this one command.
fn quiet(repo: &Path, config: &[String], args: &[&str], limit: Duration) -> Result<String, Failure> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo);
    for c in config {
        cmd.arg("-c").arg(c);
    }
    // an empty GIT_ASKPASS turns off every askpass program for git (GIT_ASKPASS, core.askPass, SSH_ASKPASS)
    cmd.args(args).env("GIT_TERMINAL_PROMPT", "0").env("GIT_ASKPASS", "").env("SSH_ASKPASS_REQUIRE", "never");
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    match github::run(cmd, None, limit) {
        Ok((true, out, _)) => Ok(out.trim_end().to_string()),
        Ok((false, _, err)) => Err(Failure::Said(err)),
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Err(Failure::NoAnswer(limit)),
        Err(e) => Err(Failure::CantRun(format!("can't run git: {e}"))),
    }
}

/// git without prompts (see `quiet`); an error is git's last line.
fn git_quiet(repo: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    quiet(repo, &[], args, limit).map_err(Failure::last_line)
}
