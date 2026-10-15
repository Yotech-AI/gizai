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

/// What the project form is offered for the folder `path`: whether it is a git checkout, its branch, whether it has
/// changes, the files a worktree could copy, and its GitHub and Bitbucket links. A link is the first GitHub or Bitbucket
/// address among its remotes' URLs (origin first; each as git uses it, then as written in its git config, see
/// `remote_fetch_urls`), so a remote that your own `insteadOf` rule sends to a host alias still offers its repository.
pub fn repo_check(path: &Path) -> RepoCheck {
    if !is_repo(path) {
        return RepoCheck { is_git: false, branch: None, dirty: false, github: None, bitbucket: None, suggest_copy: vec![] };
    }
    let branch = git(path, &["branch", "--show-current"]).filter(|b| !b.is_empty());
    let dirty = git(path, &["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    let remotes = remote_fetch_urls(path);
    let link = |provider: &str| remotes.iter()
        .find_map(|(_, url)| gizai_core::repo_url::normalize(url).ok().flatten().filter(|r| r.provider == provider).map(|r| r.url));
    let (github, bitbucket) = (link("github"), link("bitbucket"));
    RepoCheck { is_git: true, branch, dirty, github, bitbucket, suggest_copy: gizai_agents::prepare::suggest_copy(path) }
}

/// The repository's remote for the project link `url` (your own name for it, e.g. `upstream`; origin first), if it has
/// one: a remote whose URL as written in its git config, or as git uses it after your own `insteadOf` rules, is the same
/// repository as the link in any of its forms (`remote_fetch_urls`). A fetch or push through it goes the way your own
/// `git fetch` does, rewrite included (a host alias of yours for another SSH key, say).
pub fn remote_for(path: &Path, url: &str) -> Option<String> {
    remote_fetch_urls(path).into_iter()
        .find(|(_, u)| gizai_core::repo_url::same_repo(u, url))
        .map(|(name, _)| name)
}

/// The repository's remotes, each with the URL it fetches from, origin first: as git uses it, after your own
/// `url.<base>.insteadOf` rules (`worktree::remotes`, what `git remote -v` shows), then as written in its git config
/// (`worktree::remote_urls`). When a rule sends a GitHub or Bitbucket address to a host alias of yours, only the URL as
/// written still says which repository it is: (name, URL).
fn remote_fetch_urls(path: &Path) -> Vec<(String, String)> {
    let mut written = worktree::remote_urls(path);
    // a remote with more than one URL fetches from its first
    let mut seen = std::collections::HashSet::new();
    written.retain(|(name, _)| seen.insert(name.clone()));
    let mut urls: Vec<(String, String)> = worktree::remotes(path).unwrap_or_default().into_iter().chain(written).collect();
    // a stable sort: each remote's URL as git uses it stays before its URL as written
    urls.sort_by_key(|(name, _)| name != "origin");
    urls
}

/// How long a card start's fetch of main may take.
pub const START_FETCH_LIMIT: Duration = Duration::from_secs(60);

/// Where a new card of `project` starts, as a ref of its repository `repo`: with a link (GitHub, Bitbucket or another git
/// URL), the project's main branch from the remote that matches the link in any of its forms (`remote_for`; else from the
/// link itself, `worktree::fetch_start`), fetched first within
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
