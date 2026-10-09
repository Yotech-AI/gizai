//! Bitbucket Cloud through its REST API (https://api.bitbucket.org/2.0), with your Atlassian account's email and an API
//! token: who the token belongs to, a branch's pull requests, and opening one. Bitbucket has no CLI like gh, so this is
//! Gizai's own small client. Pushes go over SSH with your own keys (`connection::PushOver::Bitbucket`).
//!
//! The login is kept in your keychain (`secrets`, under `bitbucket/login`), never in Gizai's database, a log or an error
//! text; it goes to Bitbucket only in a request's Authorization header, never in a command line. Tests point the API at
//! a local server with GIZAI_BITBUCKET_API.
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::connection::Problem;
use crate::github::{Commit, PullRequest};
use crate::secrets::Keychain;

/// Bitbucket Cloud's REST API.
pub const API: &str = "https://api.bitbucket.org/2.0";

/// The scopes Gizai's API token needs: who it belongs to (`/user`), and reading and opening pull requests.
pub const SCOPES: [&str; 3] = ["read:user:bitbucket", "read:pullrequest:bitbucket", "write:pullrequest:bitbucket"];

/// Where the login is saved in the keychain.
pub const KEY: &str = "bitbucket/login";

/// How long one request may take.
const LIMIT: Duration = Duration::from_secs(30);
/// The largest answer Gizai reads.
const BODY_LIMIT: u64 = 8 << 20;
/// The longest pull request title Gizai sends (Bitbucket keeps 255 characters).
const MAX_TITLE: usize = 255;

/// Where Bitbucket's API is: GIZAI_BITBUCKET_API (a local server, in tests), else `API`.
pub fn api() -> String {
    match std::env::var("GIZAI_BITBUCKET_API") {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_string(),
        _ => API.to_string(),
    }
}

/// Your Bitbucket login: your Atlassian account's email and an API token.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Login {
    pub email: String,
    pub token: String,
}

impl std::fmt::Debug for Login {
    // never the token
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Login").field("email", &self.email).field("token", &"…").finish()
    }
}

impl Login {
    /// HTTP Basic with the email and the token, as Bitbucket's API takes an API token.
    fn authorization(&self) -> String {
        format!("Basic {}", base64(format!("{}:{}", self.email, self.token).as_bytes()))
    }
}

/// The login saved in the keychain; None when there is none.
pub fn saved_login(keychain: &dyn Keychain) -> Result<Option<Login>, String> {
    match keychain.get(KEY)? {
        None => Ok(None),
        Some(text) => serde_json::from_str::<Login>(&text).map(Some)
            .map_err(|_| "The Bitbucket login in the keychain can't be read: log in to Bitbucket again (Settings → Bitbucket)".to_string()),
    }
}

/// Saves the login in the keychain, in place of the one there.
pub fn save_login(keychain: &dyn Keychain, login: &Login) -> Result<(), String> {
    let text = serde_json::to_string(login).map_err(|_| "The Bitbucket login can't be written".to_string())?;
    keychain.set(KEY, &text)
}

/// Removes the login from the keychain (Ok when there was none).
pub fn remove_login(keychain: &dyn Keychain) -> Result<(), String> {
    keychain.delete(KEY)
}

/// Who the login belongs to, as Bitbucket names the account ("Jeffrey Sevinga (jefsev)"), or why Bitbucket didn't say,
/// with what to do.
pub fn account(login: &Login) -> Result<String, Problem> {
    let answer = request(login, "GET", "/user", &[], None).map_err(|e| Problem::new(e, "Check your internet connection, then try again."))?;
    match answer.status {
        200..=299 => {
            let doc: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
            let field = |k: &str| doc.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from);
            let name = field("display_name");
            let handle = field("username").or_else(|| field("nickname"));
            Ok(match (name, handle) {
                (Some(n), Some(h)) if !n.eq_ignore_ascii_case(&h) => format!("{n} ({h})"),
                (Some(n), _) => n,
                (None, Some(h)) => h,
                (None, None) => login.email.clone(),
            })
        }
        401 => Err(Problem::new("Bitbucket refused the email and API token",
                                "Check that the email is your Atlassian account's and that the token hasn't expired or been revoked, or make a new API token.")),
        403 => Err(Problem::new("The API token may not read your Bitbucket account", format!("Make a token with the scopes {}.", SCOPES.join(", ")))),
        status => Err(Problem::new(refused(status, &answer.body, None), "Try again in a moment.")),
    }
}

/// The pull requests from `branch` in `repo` ("workspace/repository"), in any state, newest first, as Gizai keeps them
/// for GitHub's: OPEN is open (or a draft), MERGED merged, DECLINED and SUPERSEDED closed. Bitbucket gives a pull
/// request's latest commit as a short hash; a merged one also gets its commits (full hashes), so `PullRequest::contains`
/// knows the branch's latest commit either way.
pub fn pulls_for_branch(login: &Login, repo: &str, branch: &str) -> Result<Vec<PullRequest>, String> {
    let q = format!("source.branch.name = \"{}\"", branch.replace('\\', "\\\\").replace('"', "\\\""));
    let query = [("q", q.as_str()), ("state", "OPEN"), ("state", "MERGED"), ("state", "DECLINED"), ("state", "SUPERSEDED"),
                 ("sort", "-created_on"), ("pagelen", "20")];
    let answer = request(login, "GET", &format!("/repositories/{repo}/pullrequests"), &query, None)?;
    if !(200..300).contains(&answer.status) {
        return Err(refused(answer.status, &answer.body, Some(repo)));
    }
    let doc: Value = serde_json::from_str(&answer.body).map_err(|e| format!("Bitbucket gave an answer Gizai can't read ({e})"))?;
    let mut pulls: Vec<PullRequest> = doc.get("values").and_then(Value::as_array).map(|v| v.iter().filter_map(pull).collect()).unwrap_or_default();
    for p in pulls.iter_mut().filter(|p| p.state == "MERGED") {
        // without them, the short hash of its latest commit still counts
        p.commits = commits(login, repo, p.number).unwrap_or_default();
    }
    Ok(pulls)
}

/// A pull request from Bitbucket's answer, in Gizai's terms.
fn pull(v: &Value) -> Option<PullRequest> {
    let number = v.get("id")?.as_u64()?;
    let url = v.pointer("/links/html/href")?.as_str()?.to_string();
    let draft = v.get("draft").and_then(Value::as_bool).unwrap_or(false);
    let (state, is_draft) = match v.get("state").and_then(Value::as_str).unwrap_or("OPEN") {
        "MERGED" => ("MERGED", false),
        "DECLINED" | "SUPERSEDED" => ("CLOSED", false),
        "DRAFT" => ("OPEN", true),
        _ => ("OPEN", draft),
    };
    let head = v.pointer("/source/commit/hash").and_then(Value::as_str).unwrap_or_default().to_string();
    Some(PullRequest { number, url, state: state.into(), is_draft, head_ref_oid: head, commits: vec![] })
}

/// The commits of pull request `number` (its first 100).
fn commits(login: &Login, repo: &str, number: u64) -> Result<Vec<Commit>, String> {
    let answer = request(login, "GET", &format!("/repositories/{repo}/pullrequests/{number}/commits"), &[("pagelen", "100")], None)?;
    if !(200..300).contains(&answer.status) {
        return Err(refused(answer.status, &answer.body, Some(repo)));
    }
    let doc: Value = serde_json::from_str(&answer.body).map_err(|e| format!("Bitbucket gave an answer Gizai can't read ({e})"))?;
    Ok(doc.get("values").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|c| Some(Commit { oid: c.get("hash")?.as_str()?.to_string() })).collect())
}

/// Opens a pull request from `branch` into `base` in `repo` ("workspace/repository") and returns its link.
pub fn create_pull(login: &Login, repo: &str, branch: &str, base: &str, title: &str, body: &str) -> Result<String, String> {
    let title: String = title.chars().take(MAX_TITLE).collect();
    let payload = json!({
        "title": title,
        "description": body,
        "source": {"branch": {"name": branch}},
        "destination": {"branch": {"name": base}},
    });
    let answer = request(login, "POST", &format!("/repositories/{repo}/pullrequests"), &[], Some(&payload))?;
    if !(200..300).contains(&answer.status) {
        return Err(refused(answer.status, &answer.body, Some(repo)));
    }
    let doc: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
    doc.pointer("/links/html/href").and_then(Value::as_str).map(String::from)
        .ok_or_else(|| "Bitbucket didn't say which pull request it opened".to_string())
}

/// Bitbucket's answer: its status and body.
struct Answer {
    status: u16,
    body: String,
}

/// One request to the API, as the login: HTTP errors are answers, a request that doesn't get one is an error in plain
/// words. Never follows a redirect (it would take the login along).
fn request(login: &Login, method: &str, path: &str, query: &[(&str, &str)], body: Option<&Value>) -> Result<Answer, String> {
    let base = api();
    let local = is_local(&base);
    if !base.starts_with("https://") && !local {
        return Err(format!("{base} isn't secure (https): Gizai sends the Bitbucket login only over https"));
    }
    let mut config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(LIMIT))
        .max_redirects(0)
        .user_agent(format!("Gizai/{}", env!("CARGO_PKG_VERSION")));
    if local {
        config = config.proxy(None);
    }
    let agent: ureq::Agent = config.build().into();
    let url = format!("{base}{path}");
    let sent = match (method, body) {
        ("POST", Some(b)) => agent.post(&url).header("Authorization", &login.authorization()).header("Accept", "application/json").send_json(b),
        _ => query.iter()
            .fold(agent.get(&url), |r, (k, v)| r.query(k, v))
            .header("Authorization", &login.authorization()).header("Accept", "application/json").call(),
    };
    let mut response = sent.map_err(|e| unreachable(&base, e))?;
    let status = response.status().as_u16();
    let body = response.body_mut().with_config().limit(BODY_LIMIT).lossy_utf8(true).read_to_string().map_err(|e| unreachable(&base, e))?;
    Ok(Answer { status, body })
}

/// The API on this computer (a test's local server).
fn is_local(base: &str) -> bool {
    let rest = base.strip_prefix("http://").or_else(|| base.strip_prefix("https://")).unwrap_or(base);
    let host = rest.split('/').next().unwrap_or(rest);
    let host = if host.starts_with('[') { host.split(']').next().map(|h| format!("{h}]")).unwrap_or_default() } else { host.split(':').next().unwrap_or(host).to_string() };
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "[::1]")
}

/// A request that got no answer, in plain words (never with the login).
fn unreachable(base: &str, e: ureq::Error) -> String {
    let host = base.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("Bitbucket");
    let why = match &e {
        ureq::Error::Timeout(_) => format!("no answer within {} seconds", LIMIT.as_secs()),
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => format!("no answer within {} seconds", LIMIT.as_secs()),
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused => "it refused the connection".to_string(),
        ureq::Error::HostNotFound => "the address wasn't found".to_string(),
        ureq::Error::ConnectionFailed => "the connection failed".to_string(),
        ureq::Error::BodyExceedsLimit(_) => "its answer was too large".to_string(),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => "the secure connection (TLS) failed".to_string(),
        other => other.to_string(),
    };
    format!("Can't reach Bitbucket ({host}): {why}")
}

/// What an answer other than OK means, in plain words, with Bitbucket's own message when it gave one.
fn refused(status: u16, body: &str, repo: Option<&str>) -> String {
    let said = serde_json::from_str::<Value>(body).ok()
        .and_then(|d| d.pointer("/error/message").and_then(Value::as_str).map(|m| m.trim().chars().take(300).collect::<String>()))
        .filter(|m| !m.is_empty());
    let repo = repo.map(|r| format!(" {r}")).unwrap_or_default();
    match status {
        401 => "Bitbucket refused the login (your email and API token): log in to Bitbucket again in Settings → Bitbucket".to_string(),
        403 => format!("Bitbucket refused{}: the API token needs the scopes {}, and your account access to the repository{repo}",
                       said.map(|s| format!(" ({s})")).unwrap_or_default(), SCOPES.join(", ")),
        404 => format!("Bitbucket has no repository{repo} that your account can see: check the project's Bitbucket link"),
        429 => "Bitbucket asks Gizai to slow down (too many requests): try again in a minute".to_string(),
        _ => format!("Bitbucket answered {status}{}", said.map(|s| format!(": {s}")).unwrap_or_default()),
    }
}

/// Standard base64 with padding (RFC 4648 §4).
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            out.push(if i <= chunk.len() { char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]) } else { '=' });
        }
    }
    out
}
