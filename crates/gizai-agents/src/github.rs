//! Pull requests through the user's own GitHub CLI (`gh`, logged in as them): find a branch's pull requests and open
//! one. Never prompts: a missing login is an error, never a question.
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// A pull request as `gh pr list --json` gives it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    /// OPEN, CLOSED or MERGED
    pub state: String,
    #[serde(default)]
    pub is_draft: bool,
    /// Its latest commit.
    #[serde(default)]
    pub head_ref_oid: String,
    #[serde(default)]
    pub commits: Vec<Commit>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Commit {
    pub oid: String,
}

impl PullRequest {
    /// Gizai's name for its state: open, draft, merged or closed.
    pub fn state(&self) -> &'static str {
        match self.state.as_str() {
            "MERGED" => "merged",
            "CLOSED" => "closed",
            _ if self.is_draft => "draft",
            _ => "open",
        }
    }

    /// Whether commit `sha` is in it (its latest commit or an earlier one).
    pub fn contains(&self, sha: &str) -> bool {
        !sha.is_empty() && (self.head_ref_oid == sha || self.commits.iter().any(|c| c.oid == sha))
    }
}

/// The fields Gizai asks gh for.
pub const FIELDS: &str = "number,url,state,isDraft,headRefOid,commits";
const LIMIT: Duration = Duration::from_secs(60);

/// The pull requests from `branch` in `repo` ("owner/name"), in any state, newest first. `dir` is where gh runs
/// (the project's repository).
pub fn pulls_for_branch(gh: &Path, dir: &Path, repo: &str, branch: &str) -> Result<Vec<PullRequest>, String> {
    let out = run_gh(gh, dir, &["pr", "list", "--repo", repo, "--head", branch, "--state", "all", "--limit", "20", "--json", FIELDS], None)?;
    if out.trim().is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_str(&out).map_err(|e| format!("gh gave an answer Gizai can't read ({e})"))
}

/// Opens a pull request from `branch` into `base` and returns its link; when the branch already has an open one, its
/// link. The body goes in on stdin, so its length and quotes never matter.
pub fn create_pull(gh: &Path, dir: &Path, repo: &str, branch: &str, base: &str, title: &str, body: &str) -> Result<String, String> {
    let args = ["pr", "create", "--repo", repo, "--head", branch, "--base", base, "--title", title, "--body-file", "-"];
    match run_gh(gh, dir, &args, Some(body)) {
        Ok(out) => pull_url(&out).ok_or_else(|| format!("gh didn't say which pull request it opened: {}", out.trim())),
        Err(e) if e.contains("already exists") => pull_url(&e).ok_or(e),
        Err(e) => Err(e),
    }
}

/// The last pull request link (https://github.com/<owner>/<name>/pull/<number>) in `text`.
pub fn pull_url(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '(' | ')'))
        .filter_map(|w| {
            let w = w.trim_end_matches(['.', ',', ';', ':']);
            let rest = w.strip_prefix("https://github.com/")?;
            let parts: Vec<&str> = rest.split('/').collect();
            (parts.len() == 4 && !parts[0].is_empty() && !parts[1].is_empty() && parts[2] == "pull" && parts[3].parse::<u64>().is_ok())
                .then(|| w.to_string())
        })
        .last()
}

/// The number at the end of a pull request link.
pub fn pull_number(url: &str) -> Option<u64> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

/// Runs gh without prompts, colours or update checks, and ends it after a minute. The error is gh's own message,
/// with a plain one for a missing gh or login.
fn run_gh(gh: &Path, dir: &Path, args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new(gh);
    cmd.args(args).current_dir(dir)
        .env("GH_PROMPT_DISABLED", "1").env("GH_NO_UPDATE_NOTIFIER", "1").env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")
        .env("GH_SPINNER_DISABLED", "1").env("NO_COLOR", "1").env("CLICOLOR", "0").env("GIT_TERMINAL_PROMPT", "0");
    let (ok, out, err) = run(cmd, stdin, LIMIT).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("GitHub CLI not found at {}: install gh, or set its path in Settings", gh.display()),
        std::io::ErrorKind::TimedOut => format!("gh gave no answer within {} s", LIMIT.as_secs()),
        _ => format!("can't run gh: {e}"),
    })?;
    if ok {
        return Ok(out);
    }
    let msg = err.trim();
    if msg.contains("gh auth login") {
        return Err("The GitHub CLI isn't logged in: run gh auth login in a terminal".into());
    }
    let msg: String = if msg.is_empty() { "gh failed".into() } else { msg.chars().take(600).collect() };
    Err(msg)
}

/// Runs `cmd` with `stdin` and returns (succeeded, stdout, stderr). Both outputs are read while it runs (a long
/// answer can't fill a pipe and hang it); after `limit` it is killed.
fn run(mut cmd: Command, stdin: Option<&str>, limit: Duration) -> std::io::Result<(bool, String, String)> {
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let text = text.to_string();
        std::thread::spawn(move || { let _ = pipe.write_all(text.as_bytes()); });
    }
    let read = |pipe: Option<Box<dyn Read + Send>>| std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut p) = pipe { let _ = p.read_to_string(&mut s); }
        s
    });
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let until = Instant::now() + limit;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "no answer"));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok((status.success(), out.join().unwrap_or_default(), err.join().unwrap_or_default()))
}
