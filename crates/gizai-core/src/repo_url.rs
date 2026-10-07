//! A project's repository link (GitHub, or any git URL): normalised for storage, and compared with the remotes of
//! a local repository.
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct RepoUrl {
    pub url: String,
    /// "github" or "git"
    pub provider: String,
    pub owner: Option<String>,
    pub name: Option<String>,
}

/// None for empty input. A GitHub link in any usual form (https, ssh, git@, "owner/name", a page inside the
/// repository) becomes https://github.com/<owner>/<name>; other git URLs and paths are kept as given.
pub fn normalize(input: &str) -> Result<Option<RepoUrl>> {
    let s = input.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if let Some((owner, name)) = github(s) {
        return Ok(Some(RepoUrl { url: format!("https://github.com/{owner}/{name}"), provider: "github".into(), owner: Some(owner), name: Some(name) }));
    }
    let git_url = ["https://", "http://", "ssh://", "git://", "file://", "/"].iter().any(|p| s.starts_with(p)) || is_scp(s);
    if s.contains("github.com") || s.chars().any(char::is_whitespace) || !git_url {
        return Err(Error::Invalid(format!("{s} isn't a GitHub link (https://github.com/owner/name) or a git URL")));
    }
    Ok(Some(RepoUrl { url: s.to_string(), provider: "git".into(), owner: None, name: None }))
}

/// Whether two links name the same repository (GitHub ignores case; ".git", a trailing "/" and "file://" don't count).
pub fn same_repo(a: &str, b: &str) -> bool {
    key(a) == key(b)
}

fn key(url: &str) -> String {
    let s = url.trim();
    if let Some((owner, name)) = github(s) {
        return format!("github.com/{owner}/{name}").to_lowercase();
    }
    let s = s.strip_prefix("file://").unwrap_or(s).trim_end_matches('/');
    s.strip_suffix(".git").unwrap_or(s).to_string()
}

/// user@host:path
fn is_scp(s: &str) -> bool {
    matches!(s.split_once(':'), Some((host, path)) if host.contains('@') && !host.contains('/') && !path.is_empty())
}

/// (owner, name) of a GitHub repository link.
fn github(s: &str) -> Option<(String, String)> {
    let rest = if let Some(r) = s.strip_prefix("git@github.com:").or_else(|| s.strip_prefix("ssh://git@github.com/")) {
        r
    } else {
        let r = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
        let r = r.strip_prefix("www.").unwrap_or(r);
        match r.strip_prefix("github.com/") {
            Some(r) => r,
            // the "owner/name" shorthand
            None if !s.contains(':') && !s.starts_with(['/', '.', '~']) && s.matches('/').count() == 1 => s,
            None => return None,
        }
    };
    let mut parts = rest.split('/').filter(|p| !p.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    let owner_ok = owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let name_ok = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    (owner_ok && name_ok).then(|| (owner.to_string(), name.to_string()))
}
