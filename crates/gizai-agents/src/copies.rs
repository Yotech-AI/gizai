//! The Team Lead's read-only copies of the projects' code (`<data dir>/code/<KEY>`): a worktree of the project's
//! repository without a branch, at the commit a new card starts from. It shares the repository's objects (no clone, no
//! second download) and holds only tracked files: no vendor/, node_modules/, target/ or .env. Gizai never prepares it
//! (no copies, installs or setup), and the card worktree features don't see it: it lives outside Gizai's worktree folder.
use std::path::{Path, PathBuf};

use crate::AgentError;
use crate::worktree::{git, same, worktrees};

/// The commit a copy shows.
#[derive(Debug, Clone, PartialEq)]
pub struct At {
    pub sha: String,
    /// Its date (the committer's), as 2026-10-07.
    pub date: String,
}

impl At {
    /// The first 7 characters of its id.
    pub fn short(&self) -> &str {
        &self.sha[..self.sha.len().min(7)]
    }
}

/// The repository's shared git folder, as it is on disk: the same for its main checkout and every worktree.
pub fn git_folder(dir: &Path) -> Option<PathBuf> {
    let common = git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok()?;
    PathBuf::from(common).canonicalize().ok()
}

/// Whether `dir` is a checkout (the top of one, not a folder inside it) of the repository whose main checkout is `repo`.
pub fn is_checkout_of(dir: &Path, repo: &Path) -> bool {
    let top = git(dir, &["rev-parse", "--show-toplevel"]).ok();
    top.is_some_and(|t| same(Path::new(&t), dir)) && git_folder(dir).is_some_and(|g| Some(g) == git_folder(repo))
}

/// Makes or moves the copy at `dir` of the repository `repo` to `rev` (a ref or a commit id):
/// - missing: made, as a worktree without a branch (`git worktree add --detach`);
/// - at another commit: moved to `rev`, and anything changed in it thrown away (it is Gizai's own);
/// - already at `rev`: left alone, not a file touched.
///
/// A folder at `dir` that isn't a copy of `repo` is an error: `remove` it first. Returns the commit the copy shows and
/// whether it changed.
pub fn sync(repo: &Path, dir: &Path, rev: &str) -> Result<(At, bool), AgentError> {
    let sha = git(repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")])
        .map_err(|_| AgentError::Git(format!("{rev} isn't in {}", repo.display())))?;
    let changed = if std::fs::symlink_metadata(dir).is_ok() {
        if !is_checkout_of(dir, repo) {
            return Err(AgentError::Git(format!("{} is not a copy of {}", dir.display(), repo.display())));
        }
        if git(dir, &["rev-parse", "--verify", "--quiet", "HEAD"]).ok().as_deref() == Some(sha.as_str()) {
            false
        } else {
            git(dir, &["checkout", "--quiet", "--force", "--detach", &sha])?;
            git(dir, &["clean", "-d", "-x", "--force", "--quiet"])?;
            true
        }
    } else {
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let path = dir.to_string_lossy();
        let add = || git(repo, &["worktree", "add", "--quiet", "--detach", &path, &sha]);
        if add().is_err() {
            // a copy whose folder was deleted by hand is still in git's list
            git(repo, &["worktree", "prune"])?;
            add()?;
        }
        true
    };
    let date = git(repo, &["log", "-1", "--format=%cs", &sha]).unwrap_or_default();
    Ok((At { sha, date }, changed))
}

/// The commit the copy at `dir` shows, when it is a checkout.
pub fn at(dir: &Path) -> Option<At> {
    let out = git(dir, &["log", "-1", "--format=%H %cs", "HEAD"]).ok()?;
    let (sha, date) = out.split_once(' ')?;
    Some(At { sha: sha.to_string(), date: date.to_string() })
}

/// Removes the copy at `dir`, and only one directly inside `root` (Gizai's folder of copies): `git worktree remove
/// --force` from its repository's main checkout, then `git worktree prune` there. What git can't remove (its
/// repository is gone, or it isn't a worktree) is deleted as a folder; a symlink only loses the link.
pub fn remove(dir: &Path, root: &Path) -> Result<(), AgentError> {
    let parent = dir.parent().and_then(|p| p.canonicalize().ok());
    if dir.file_name().is_none() || parent.is_none() || parent != root.canonicalize().ok() {
        return Err(AgentError::Git(format!("{} is not one of Gizai's copies", dir.display())));
    }
    let meta = match std::fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if !meta.is_dir() {
        std::fs::remove_file(dir)?;
        return Ok(());
    }
    let top = git(dir, &["rev-parse", "--show-toplevel"]).ok().is_some_and(|t| same(Path::new(&t), dir));
    let main = top.then(|| worktrees(dir).ok()).flatten()
        .and_then(|list| list.into_iter().next()).map(|(p, _)| p)
        .filter(|m| !same(m, dir));
    if let Some(m) = &main {
        let _ = git(m, &["worktree", "remove", "--force", &dir.to_string_lossy()]);
    }
    if std::fs::symlink_metadata(dir).is_ok() {
        std::fs::remove_dir_all(dir)?;
    }
    if let Some(m) = &main {
        let _ = git(m, &["worktree", "prune"]);
    }
    Ok(())
}
