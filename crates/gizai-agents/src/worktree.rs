//! One git worktree per task, on its own branch, so agents never touch the main checkout.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::AgentError;

#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    pub created: bool,
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

fn git(repo: &Path, args: &[&str]) -> Result<String, AgentError> {
    let out = Command::new("git").arg("-C").arg(repo).args(args).output()
        .map_err(|e| AgentError::Git(format!("can't run git: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(AgentError::Git(String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

/// (path, branch) for every worktree of the repo.
fn worktrees(repo: &Path) -> Result<Vec<(PathBuf, Option<String>)>, AgentError> {
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

fn same(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

/// The task's worktree at `base_dir/<identifier>` on branch `gizai/<identifier>-<slug>`: reused when it
/// already exists, else created (on the existing branch if there is one, or a new branch off `default_branch`).
pub fn ensure(repo: &Path, base_dir: &Path, identifier: &str, title: &str, default_branch: &str) -> Result<Worktree, AgentError> {
    match git(repo, &["rev-parse", "--is-inside-work-tree"]) {
        Ok(s) if s == "true" => {}
        _ => return Err(AgentError::NotGitRepo(repo.to_path_buf())),
    }
    let path = base_dir.join(identifier);
    let s = slug(title, 40);
    let branch = if s.is_empty() { format!("gizai/{}", identifier.to_lowercase()) } else { format!("gizai/{}-{s}", identifier.to_lowercase()) };
    let list = worktrees(repo)?;

    if path.exists() {
        return match list.iter().find(|(p, _)| same(p, &path)) {
            Some((_, b)) => Ok(Worktree { path, branch: b.clone().unwrap_or(branch), created: false }),
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
    Ok(Worktree { path, branch, created: true })
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

/// Fetches `branch` of the project's repository and returns the ref to start from: through `remote` when the
/// repository has one for it (`refs/remotes/<remote>/<branch>`), else from `url` into a ref git's branch list
/// doesn't show (`refs/gizai/base/<branch>`). Never asks for a password; gives up after a minute.
pub fn fetch_start(repo: &Path, remote: Option<&str>, url: &str, branch: &str) -> Result<String, AgentError> {
    let (from, start) = match remote {
        Some(r) => (r, format!("refs/remotes/{r}/{branch}")),
        None => (url, format!("refs/gizai/base/{branch}")),
    };
    let spec = format!("+refs/heads/{branch}:{start}");
    let mut last = String::new();
    // a run starting at the same moment may hold the ref's lock: one retry
    for attempt in 0..2 {
        match git_quiet(repo, &["fetch", "--quiet", "--no-tags", from, &spec], Duration::from_secs(60)) {
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

/// Pushes the local `branch` to the branch of the same name at `to` (a remote's name, or a URL) with the user's own
/// git login (ssh keys, credential helpers). Never forces and never asks for a password; gives up after two minutes.
pub fn push_branch(repo: &Path, to: &str, branch: &str) -> Result<(), AgentError> {
    let spec = format!("refs/heads/{branch}:refs/heads/{branch}");
    git_quiet(repo, &["push", "--quiet", to, &spec], Duration::from_secs(120))
        .map(|_| ())
        .map_err(|e| AgentError::Git(format!("Couldn't push {branch} to {to}: {e}")))
}

/// The worktree that has `branch` checked out, if one does.
pub fn worktree_of(repo: &Path, branch: &str) -> Result<Option<PathBuf>, AgentError> {
    Ok(worktrees(repo)?.into_iter().find(|(_, b)| b.as_deref() == Some(branch)).map(|(p, _)| p))
}

/// Removes a worktree, never by force: git refuses one with uncommitted changes or untracked files, and says so.
pub fn remove_worktree(repo: &Path, path: &Path) -> Result<(), AgentError> {
    git(repo, &["worktree", "remove", &path.to_string_lossy()]).map(|_| ())
}

/// Deletes a local branch (git refuses one that is checked out somewhere).
pub fn delete_branch(repo: &Path, branch: &str) -> Result<(), AgentError> {
    git(repo, &["branch", "-D", branch]).map(|_| ())
}

/// How many files in the checkout at `dir` have uncommitted changes (untracked ones included).
pub fn uncommitted(dir: &Path) -> Result<usize, AgentError> {
    Ok(git(dir, &["status", "--porcelain"])?.lines().filter(|l| !l.trim().is_empty()).count())
}

/// git without prompts (no terminal, ssh in batch mode unless you set your own ssh command), ended after `limit`.
fn git_quiet(repo: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args).env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    let mut child = cmd.spawn().map_err(|e| format!("can't run git: {e}"))?;
    let until = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(50)),
            _ => { let _ = child.kill(); let _ = child.wait(); return Err(format!("no answer within {} s", limit.as_secs())); }
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("git failed").to_string())
    }
}
