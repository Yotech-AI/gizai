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

    /// Whether commit `sha` is in it (its latest commit or an earlier one). A short hash counts when `sha` starts with
    /// it (7 characters or more): Bitbucket gives a pull request's latest commit that way.
    pub fn contains(&self, sha: &str) -> bool {
        let is = |oid: &str| oid == sha || (oid.len() >= 7 && sha.starts_with(oid));
        !sha.is_empty() && (is(&self.head_ref_oid) || self.commits.iter().any(|c| is(&c.oid)))
    }
}

/// The fields Gizai asks gh for.
pub const FIELDS: &str = "number,url,state,isDraft,headRefOid,commits";
const LIMIT: Duration = Duration::from_secs(60);

/// What the Team Lead's merge_pull_request (GA-86) checks before a merge, as `gh pr view --json` gives it.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PullDetails {
    pub number: u64,
    pub url: String,
    /// OPEN, CLOSED or MERGED
    pub state: String,
    pub is_draft: bool,
    /// From a fork: another repository's branch.
    pub is_cross_repository: bool,
    pub head_ref_name: String,
    /// Its latest commit.
    pub head_ref_oid: String,
    /// The branch it goes into.
    pub base_ref_name: String,
    /// MERGEABLE, CONFLICTING or UNKNOWN (GitHub is still working it out).
    pub mergeable: String,
    /// CLEAN, UNSTABLE, BLOCKED, BEHIND, DIRTY, DRAFT, HAS_HOOKS or UNKNOWN.
    pub merge_state_status: String,
    /// Every check on its latest commit: GitHub Actions jobs and other check runs, and commit statuses.
    pub status_check_rollup: Vec<CheckItem>,
}

/// One check on a pull request: a check run (status and conclusion) or a commit status (state).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CheckItem {
    /// CheckRun or StatusContext
    #[serde(rename = "__typename")]
    pub kind: String,
    /// A check run's name.
    pub name: String,
    /// A commit status's name.
    pub context: String,
    pub workflow_name: String,
    /// A check run's: QUEUED, IN_PROGRESS, COMPLETED, WAITING, PENDING or REQUESTED.
    pub status: String,
    /// A completed check run's: SUCCESS, FAILURE, NEUTRAL, CANCELLED, SKIPPED, TIMED_OUT, ACTION_REQUIRED, STALE, …
    pub conclusion: String,
    /// A commit status's: SUCCESS, PENDING, EXPECTED, ERROR or FAILURE.
    pub state: String,
}

/// How a check stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    /// Succeeded; a skipped or neutral check run counts too, as GitHub's own merge rules count it.
    Passed,
    /// Still queued or running.
    Running,
    Failed,
}

impl CheckItem {
    /// Its name as GitHub shows it: "CI / ubuntu-24.04" for a job, else the check's or the status's name.
    pub fn label(&self) -> String {
        let name = if self.name.is_empty() { &self.context } else { &self.name };
        match self.workflow_name.as_str() {
            "" => name.clone(),
            w => format!("{w} / {name}"),
        }
    }

    pub fn standing(&self) -> CheckState {
        if self.kind == "StatusContext" || (self.status.is_empty() && !self.state.is_empty()) {
            return match self.state.as_str() {
                "SUCCESS" => CheckState::Passed,
                "PENDING" | "EXPECTED" | "" => CheckState::Running,
                _ => CheckState::Failed,
            };
        }
        if self.status != "COMPLETED" {
            return CheckState::Running;
        }
        match self.conclusion.as_str() {
            "SUCCESS" | "NEUTRAL" | "SKIPPED" => CheckState::Passed,
            _ => CheckState::Failed,
        }
    }
}

/// `v` without its null fields, at every level: a field gh gives as null reads as missing (its default).
fn without_nulls(v: serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(m) => m.into_iter().filter(|(_, x)| !x.is_null()).map(|(k, x)| (k, without_nulls(x))).collect(),
        serde_json::Value::Array(a) => a.into_iter().map(without_nulls).collect(),
        other => other,
    }
}

/// The fields of `PullDetails`.
pub const DETAIL_FIELDS: &str =
    "number,url,state,isDraft,isCrossRepository,headRefName,headRefOid,baseRefName,mergeable,mergeStateStatus,statusCheckRollup";

/// Pull request `number` of `repo` ("owner/name") as GitHub has it now, with its checks. `dir` is where gh runs.
pub fn pull_details(gh: &Path, dir: &Path, repo: &str, number: u64) -> Result<PullDetails, String> {
    let n = number.to_string();
    let out = run_gh(gh, dir, &["pr", "view", &n, "--repo", repo, "--json", DETAIL_FIELDS], None)?;
    let v: serde_json::Value = serde_json::from_str(&out).map_err(|e| format!("gh gave an answer Gizai can't read ({e})"))?;
    serde_json::from_value(without_nulls(v)).map_err(|e| format!("gh gave an answer Gizai can't read ({e})"))
}

/// Merges pull request `number` of `repo` with a merge commit (never squash or rebase), only while `head` is still its
/// latest commit (GitHub refuses it otherwise). Never with admin rights or auto-merge, and no branch is deleted.
/// Returns gh's own words.
pub fn merge_pull(gh: &Path, dir: &Path, repo: &str, number: u64, head: &str) -> Result<String, String> {
    let n = number.to_string();
    let out = run_gh(gh, dir, &["pr", "merge", &n, "--repo", repo, "--merge", "--match-head-commit", head], None)?;
    Ok(out.trim().to_string())
}

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
        .next_back()
}

/// The number at the end of a pull request link.
pub fn pull_number(url: &str) -> Option<u64> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

/// gh without prompts, colours or update checks.
pub(crate) fn gh_command(gh: &Path) -> Command {
    let mut cmd = crate::os::command(gh);
    cmd.env("GH_PROMPT_DISABLED", "1").env("GH_NO_UPDATE_NOTIFIER", "1").env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")
        .env("GH_SPINNER_DISABLED", "1").env("NO_COLOR", "1").env("CLICOLOR", "0").env("GIT_TERMINAL_PROMPT", "0");
    cmd
}

/// Runs gh without prompts, colours or update checks, and ends it after a minute. The error is gh's own message,
/// with a plain one for a missing gh or login.
fn run_gh(gh: &Path, dir: &Path, args: &[&str], stdin: Option<&str>) -> Result<String, String> {
    let mut cmd = gh_command(gh);
    cmd.args(args).current_dir(dir);
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
/// answer can't fill a pipe and hang it); after `limit` it is killed (an error of kind `TimedOut`).
pub(crate) fn run(mut cmd: Command, stdin: Option<&str>, limit: Duration) -> std::io::Result<(bool, String, String)> {
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
