//! Read-only git checks for the project screen, which remote of a repository is its GitHub or Bitbucket repository, and
//! where a new card of a project starts.
use gizai_agents::{AgentError, worktree};
use gizai_core::model::Project;
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoCheck {
    pub is_git: bool,
    pub branch: Option<String>,
    pub dirty: bool,
    /// The repository's GitHub remote as a link (origin first), offered for the project's GitHub field.
    pub github: Option<String>,
    /// The repository's Bitbucket remote as a link (origin first): https://bitbucket.org/<workspace>/<repository>.
    pub bitbucket: Option<String>,
    /// Paths a new worktree could copy from it, offered for the project's copy list (.env, vendor/, node_modules/, target/).
    pub suggest_copy: Vec<String>,
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let out = gizai_agents::os::command("git").arg("-C").arg(path).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether `path` is a folder inside a git checkout.
pub fn is_repo(path: &Path) -> bool {
    path.is_dir() && git(path, &["rev-parse", "--is-inside-work-tree"]).as_deref() == Some("true")
}

pub fn repo_check(path: &Path) -> RepoCheck {
    if !is_repo(path) {
        return RepoCheck { is_git: false, branch: None, dirty: false, github: None, bitbucket: None, suggest_copy: vec![] };
    }
    let branch = git(path, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let dirty = git(path, &["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    let mut remotes: Vec<(String, String)> = git(path, &["remote", "-v"]).unwrap_or_default().lines()
        .filter_map(|l| { let p: Vec<&str> = l.split_whitespace().collect(); (p.len() == 3 && p[2] == "(fetch)").then(|| (p[0].to_string(), p[1].to_string())) })
        .collect();
    remotes.sort_by_key(|(name, _)| name != "origin");
    let link = |provider: &str| remotes.iter()
        .find_map(|(_, url)| gizai_core::repo_url::normalize(url).ok().flatten().filter(|r| r.provider == provider).map(|r| r.url));
    let (github, bitbucket) = (link("github"), link("bitbucket"));
    RepoCheck { is_git: true, branch, dirty, github, bitbucket, suggest_copy: gizai_agents::prepare::suggest_copy(path) }
}

/// The repository's remote for the project link `url` (your own name for it, e.g. `upstream`; origin first), if it has one.
pub fn remote_for(path: &Path, url: &str) -> Option<String> {
    gizai_agents::worktree::remotes(path).unwrap_or_default().into_iter()
        .filter(|(_, u)| gizai_core::repo_url::same_repo(u, url))
        .min_by_key(|(name, _)| name != "origin")
        .map(|(name, _)| name)
}

/// How long a card start's fetch of main may take.
pub const START_FETCH_LIMIT: Duration = Duration::from_secs(60);

/// Where a new card of `project` starts, as a ref of its repository `repo`: with a link (GitHub, Bitbucket or another git
/// URL), the project's main branch from the remote that matches the link in any of its forms (else from the link
/// itself, `worktree::fetch_start`), fetched first within
/// `fetch` (None: as last fetched); without a link, the local default branch. Card starts and the Team Lead's copies
/// (`code`) both start here, so the two can't drift apart. Blocking.
pub fn start_point(project: &Project, repo: &Path, fetch: Option<Duration>) -> Result<String, AgentError> {
    let Some(url) = project.repo_url.as_deref() else { return Ok(project.default_branch.clone()) };
    let remote = remote_for(repo, url);
    match fetch {
        Some(limit) => worktree::fetch_start(repo, remote.as_deref(), url, &project.default_branch, limit),
        None => Ok(worktree::start_ref(remote.as_deref(), &project.default_branch)),
    }
}
