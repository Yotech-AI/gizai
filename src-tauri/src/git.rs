//! Read-only git checks for the project screen, and which remote of a repository is its GitHub repository.
use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoCheck {
    pub is_git: bool,
    pub branch: Option<String>,
    pub dirty: bool,
    /// The repository's GitHub remote as a link (origin first), offered for the project's GitHub field.
    pub github: Option<String>,
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(path).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether `path` is a folder inside a git checkout.
pub fn is_repo(path: &Path) -> bool {
    path.is_dir() && git(path, &["rev-parse", "--is-inside-work-tree"]).as_deref() == Some("true")
}

pub fn repo_check(path: &Path) -> RepoCheck {
    if !is_repo(path) {
        return RepoCheck { is_git: false, branch: None, dirty: false, github: None };
    }
    let branch = git(path, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let dirty = git(path, &["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    let mut remotes: Vec<(String, String)> = git(path, &["remote", "-v"]).unwrap_or_default().lines()
        .filter_map(|l| { let p: Vec<&str> = l.split_whitespace().collect(); (p.len() == 3 && p[2] == "(fetch)").then(|| (p[0].to_string(), p[1].to_string())) })
        .collect();
    remotes.sort_by_key(|(name, _)| name != "origin");
    let github = remotes.iter().find_map(|(_, url)| gizai_core::repo_url::normalize(url).ok().flatten().filter(|r| r.provider == "github").map(|r| r.url));
    RepoCheck { is_git: true, branch, dirty, github }
}

/// The repository's remote for the project link `url` (your own name for it, e.g. `upstream`; origin first), if it has one.
pub fn remote_for(path: &Path, url: &str) -> Option<String> {
    gizai_agents::worktree::remotes(path).unwrap_or_default().into_iter()
        .filter(|(_, u)| gizai_core::repo_url::same_repo(u, url))
        .min_by_key(|(name, _)| name != "origin")
        .map(|(name, _)| name)
}
