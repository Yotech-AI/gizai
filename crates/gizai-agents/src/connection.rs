//! How Gizai reaches GitHub, and the checks behind Settings → GitHub.
//! - A push goes over SSH with your keys (the default) or over HTTPS with the GitHub CLI's login, set for that one git
//!   command only (`PushOver`). A failure says what to check, in plain words (`push_problem`).
//! - The checks: gh's version and the account it is logged in as, ssh to git@github.com in batch mode, and gh's own
//!   login in the browser (`GhLogin`).
//!
//! Gizai never stores a token or password and never asks for one: it uses gh's login and your SSH keys.
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::github;

/// How a card's branch goes to GitHub (Settings → GitHub → Push over).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushOver {
    /// SSH with your own keys, the default: an https://github.com/ address goes to git@github.com: instead.
    Ssh,
    /// HTTPS with the GitHub CLI's login (`gh auth git-credential`) as git's only credential helper: a git@github.com:
    /// address goes to https://github.com/ instead.
    Https { gh: PathBuf },
}

/// How a GitHub address starts, over HTTPS and over SSH.
const HTTPS: [&str; 4] = ["https://github.com/", "http://github.com/", "https://www.github.com/", "http://www.github.com/"];
const SSH: [&str; 2] = ["git@github.com:", "ssh://git@github.com/"];

impl PushOver {
    /// The `-c` settings for one git command; they change no git config and no remote. git takes the longest rewrite
    /// that matches an address, so a more specific one in your own config (a mirror, a test's local repository) wins.
    pub fn git_config(&self) -> Vec<String> {
        match self {
            PushOver::Ssh => HTTPS.iter().map(|from| format!("url.git@github.com:.insteadOf={from}")).collect(),
            PushOver::Https { gh } => {
                let mut c: Vec<String> = SSH.iter().chain(&HTTPS[1..]).map(|from| format!("url.https://github.com/.insteadOf={from}")).collect();
                // the empty helper first: none of your own helpers runs (or asks), only gh's login is used
                c.push("credential.helper=".into());
                c.push(format!("credential.helper=!{} auth git-credential", sh_quote(&gh.to_string_lossy())));
                c
            }
        }
    }
}

/// What went wrong talking to GitHub, and what to do about it, in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub what: String,
    /// What to do, when Gizai knows.
    pub fix: Option<String>,
}

impl Problem {
    pub fn new(what: impl Into<String>, fix: impl Into<String>) -> Problem {
        Problem { what: what.into(), fix: Some(fix.into()) }
    }

    pub fn plain(what: impl Into<String>) -> Problem {
        Problem { what: what.into(), fix: None }
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.fix {
            Some(fix) => write!(f, "{}. {fix}", self.what),
            None => write!(f, "{}", self.what),
        }
    }
}

const OR_HTTPS: &str = "Or push over HTTPS with gh's login (Settings → GitHub).";

/// What a failed push (or its dry run) means, from everything git said: what to check, in plain words. Else git's own
/// words.
pub fn push_problem(said: &str, over: &PushOver) -> Problem {
    let has = |s: &str| said.contains(s);
    if let Some(p) = ssh_problem(said) {
        return p;
    }
    if has("Repository not found") {
        return Problem::new("GitHub has no repository at this address that your account can see",
                            "Check the project's GitHub link, and that your GitHub account has access to the repository.");
    }
    if let Some(who) = denied_to(said) {
        return Problem::new(format!("Your GitHub account {who} can't push to this repository"),
                            "Ask for write access to it, or use an account that has it.");
    }
    let no_login = ["could not read Username", "could not read Password", "terminal prompts disabled"].iter().any(|s| has(s));
    let refused = has("Authentication failed") || has("Invalid username or token");
    if no_login || refused {
        return match over {
            PushOver::Https { .. } if refused => Problem::new("GitHub refused gh's login for HTTPS",
                                                              "Use Log in with GitHub again in Settings → GitHub, or run gh auth login in a terminal."),
            PushOver::Https { .. } => not_logged_in_for_https(),
            PushOver::Ssh => Problem::new("git tried HTTPS, and has no login for it",
                                          "Check where this repository pushes to (git remote -v), or push over HTTPS with gh's login (Settings → GitHub)."),
        };
    }
    if has("refusing to allow an OAuth App to create or update workflow") {
        return Problem::new("gh's login may not change GitHub Actions workflows",
                            "Run gh auth refresh -s workflow in a terminal, or push over SSH (Settings → GitHub).");
    }
    if has("[rejected]") || has("non-fast-forward") || has("(fetch first)") {
        return Problem::new("The branch on GitHub has commits this one doesn't",
                            "Gizai never forces a push: merge GitHub's copy into the branch first.");
    }
    if ["Could not resolve host", "Failed to connect to", "Couldn't connect to server", "Connection timed out", "Network is unreachable"].iter().any(|s| has(s)) {
        return Problem::new("Can't reach github.com", "Check your internet connection, then try again.");
    }
    if has("src refspec") && has("does not match any") {
        return Problem::new("The repository has no commits yet", "Gizai tries a push with its latest commit: commit something first.");
    }
    Problem::plain(last_words(said))
}

/// What ssh said went wrong, in plain words.
fn ssh_problem(said: &str) -> Option<Problem> {
    let has = |s: &str| said.contains(s);
    if has("Permission denied (publickey") {
        Some(Problem::new("GitHub didn't accept your SSH key",
                          format!("Add your public key to your GitHub account (github.com/settings/keys), or load it into ssh-agent with ssh-add. {OR_HTTPS}")))
    } else if has("Host key verification failed") {
        Some(Problem::new("This computer doesn't trust GitHub's SSH host key yet",
                          "Run ssh -T git@github.com once in a terminal and answer yes."))
    } else if has("ssh: not found") || has("ssh: command not found") || has("cannot run ssh") {
        Some(ssh_missing())
    } else if ["ssh: connect to host", "Could not resolve hostname", "Connection closed by", "kex_exchange_identification", "Connection reset by"].iter().any(|s| has(s)) {
        Some(Problem::new("Can't reach github.com over SSH",
                          format!("Check your internet connection, and that no firewall blocks SSH (port 22). {OR_HTTPS}")))
    } else {
        None
    }
}

fn ssh_missing() -> Problem {
    Problem::new("ssh isn't installed", format!("Install OpenSSH. {OR_HTTPS}"))
}

fn not_logged_in_for_https() -> Problem {
    Problem::new("The GitHub CLI isn't logged in, so git has no login for HTTPS",
                 "Use Log in with GitHub in Settings → GitHub, or run gh auth login in a terminal.")
}

/// GitHub gave no answer within `limit`.
pub fn no_answer(limit: Duration) -> Problem {
    Problem::new(format!("GitHub gave no answer within {}", span(limit)), "Check your internet connection, then try again.")
}

/// "2 minutes", "30 seconds".
fn span(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        60 => "a minute".into(),
        s if s > 60 && s % 60 == 0 => format!("{} minutes", s / 60),
        s => format!("{s} seconds"),
    }
}

/// "octocat" in "Permission to acme/shop.git denied to octocat."
fn denied_to(said: &str) -> Option<String> {
    let line = said.lines().find(|l| l.contains("Permission to ") && l.contains(" denied to "))?;
    let who = line.split(" denied to ").nth(1)?.trim().trim_end_matches('.');
    (!who.is_empty()).then(|| who.to_string())
}

/// git's (or ssh's) most telling line: its first error that isn't a generic one, else its last line; without the
/// "fatal:" and "error:" prefixes.
fn last_words(said: &str) -> String {
    const GENERIC: [&str; 4] = ["failed to push some refs", "Could not read from remote repository", "the remote end hung up", "unable to access"];
    let tidy = |l: &str| {
        let l = l.trim();
        let l = l.strip_prefix("remote: ").unwrap_or(l);
        ["fatal: ", "error: ", "ERROR: "].iter().fold(l, |l, p| l.strip_prefix(p).unwrap_or(l)).trim().to_string()
    };
    let lines: Vec<&str> = said.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("hint:")).collect();
    let error = lines.iter().find(|l| {
        ["fatal:", "error:", "ERROR:", "remote: error:"].iter().any(|p| l.starts_with(p)) && !GENERIC.iter().any(|g| l.contains(g))
    });
    match error.or(lines.last()) {
        Some(l) => tidy(l),
        None => "git failed".into(),
    }
}

/// `s` as one shell word.
fn sh_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@%,".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Whether ssh reaches GitHub with your keys, in batch mode so it never asks: `ssh -T git@github.com`, which GitHub
/// answers with "Hi <login>! You've successfully authenticated, …". Ok: the account your key belongs to. It runs your
/// own GIT_SSH_COMMAND when you set one, the way git does for a push.
pub fn ssh_check(limit: Duration) -> Result<String, Problem> {
    let mut cmd = match std::env::var("GIT_SSH_COMMAND") {
        Ok(own) if !own.trim().is_empty() => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(format!("{own} \"$@\"")).arg(&own);
            c
        }
        _ => {
            let mut c = Command::new("ssh");
            c.args(["-o", "BatchMode=yes"]);
            c
        }
    };
    cmd.args(["-T", "git@github.com"]).env("SSH_ASKPASS_REQUIRE", "never");
    let (_, out, err) = github::run(cmd, None, limit).map_err(|e| match e.kind() {
        ErrorKind::NotFound => ssh_missing(),
        ErrorKind::TimedOut => no_answer(limit),
        _ => Problem::plain(format!("can't run ssh: {e}")),
    })?;
    let said = format!("{err}\n{out}");
    // GitHub ends the session with exit code 1 even when the key is accepted, so its words decide
    if let Some(who) = said.lines().filter(|l| l.contains("successfully authenticated")).find_map(|l| l.trim().strip_prefix("Hi ")?.split('!').next()) {
        return Ok(who.trim().to_string());
    }
    Err(ssh_problem(&said).unwrap_or_else(|| Problem::plain(last_words(&said))))
}

/// Why gh couldn't be run, with what to do.
fn gh_failed(gh: &Path, e: std::io::Error, limit: Duration) -> Problem {
    match e.kind() {
        ErrorKind::NotFound => Problem::new(format!("The GitHub CLI isn't at {}", gh.display()),
                                            "Install it from cli.github.com, or set its path in Settings → GitHub."),
        ErrorKind::TimedOut => Problem::new(format!("gh gave no answer within {}", span(limit)), "Check your internet connection, then try again."),
        _ => Problem::plain(format!("can't run gh: {e}")),
    }
}

/// The GitHub CLI's version ("2.62.0"), or why it doesn't run.
pub fn gh_version(gh: &Path, limit: Duration) -> Result<String, Problem> {
    let mut cmd = github::gh_command(gh);
    cmd.arg("--version");
    let (ok, out, err) = github::run(cmd, None, limit).map_err(|e| gh_failed(gh, e, limit))?;
    if !ok {
        return Err(Problem::plain(last_words(&format!("{out}\n{err}"))));
    }
    let first = out.lines().next().unwrap_or("").trim();
    let version = first.strip_prefix("gh version ").and_then(|v| v.split_whitespace().next()).unwrap_or(first);
    Ok(version.to_string())
}

fn not_logged_in() -> Problem {
    Problem::new("The GitHub CLI isn't logged in", "Use Log in with GitHub, or run gh auth login in a terminal.")
}

/// The account the GitHub CLI is logged in as on github.com (its active one), or why there is none, with what to do.
/// `gh auth status` asks GitHub whether the login still works; Gizai never reads its token.
pub fn gh_account(gh: &Path, limit: Duration) -> Result<String, Problem> {
    let mut cmd = github::gh_command(gh);
    cmd.args(["auth", "status", "--hostname", "github.com"]);
    let (_, out, err) = github::run(cmd, None, limit).map_err(|e| gh_failed(gh, e, limit))?;
    account_from_status(&format!("{out}\n{err}"))
}

/// The active account in an answer of `gh auth status` (gh lists it first), or why there is none.
pub fn account_from_status(said: &str) -> Result<String, Problem> {
    for line in said.lines().map(str::trim) {
        if let Some(who) = word_after(line, "Logged in to github.com account ").or_else(|| word_after(line, "Logged in to github.com as ")) {
            return Ok(who);
        }
        // gh says this when GitHub refuses its token, and also when it can't reach GitHub
        if line.contains("Failed to log in to github.com") {
            let who = word_after(line, "Failed to log in to github.com account ").map(|w| format!(" as {w}")).unwrap_or_default();
            return Err(Problem::new(format!("gh's login{who} doesn't work: GitHub refused it, or couldn't be reached"),
                                    "Check your internet connection. If that's fine, use Log in with GitHub again, or run gh auth login in a terminal."));
        }
        if line.contains("Timeout trying to log in to github.com") {
            return Err(Problem::new("GitHub didn't answer gh in time", "Check your internet connection, then try again."));
        }
    }
    if said.contains("error connecting to") {
        return Err(Problem::new("gh can't reach GitHub", "Check your internet connection, then try again."));
    }
    if said.contains("not logged in") || said.contains("gh auth login") {
        return Err(not_logged_in());
    }
    Err(Problem::plain(last_words(said)))
}

/// The word after `prefix` in `line`, without trailing punctuation.
fn word_after(line: &str, prefix: &str) -> Option<String> {
    let rest = &line[line.find(prefix)? + prefix.len()..];
    let word = rest.split_whitespace().next()?.trim_end_matches(['.', ',', '!', ':']);
    (!word.is_empty()).then(|| word.to_string())
}

/// Where you enter gh's one-time code, when gh doesn't say.
pub const DEVICE_URL: &str = "https://github.com/login/device";

/// The command that logs gh in from a terminal, for when the login in the app can't work.
pub fn login_command(gh: &Path) -> String {
    let on_path = gh.parent().is_some_and(|dir| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir)));
    let bin = if on_path && gh.file_name().is_some_and(|n| n == "gh") { "gh".to_string() } else { sh_quote(&gh.to_string_lossy()) };
    format!("{bin} auth login --hostname github.com --web")
}

/// gh's own login in the browser, run without a terminal (`gh auth login --web`). gh shows a one-time code and a
/// link, waits until you enter the code on GitHub, then keeps its login where it always does (your keyring). Gizai
/// only shows the code and the link. It asks for gh's usual access plus `workflow`, so a push over HTTPS may include
/// GitHub Actions workflows.
pub struct GhLogin {
    gh: PathBuf,
    child: Child,
    /// The one-time code to enter on GitHub.
    pub code: String,
    /// Where to enter it.
    pub url: String,
    lines: Receiver<String>,
    said: Vec<String>,
}

impl GhLogin {
    /// Starts gh's login and returns once gh shows its one-time code (at most `limit`).
    pub fn start(gh: &Path, limit: Duration) -> Result<GhLogin, Problem> {
        let mut cmd = github::gh_command(gh);
        cmd.args(["auth", "login", "--web", "--hostname", "github.com", "--scopes", "workflow"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| gh_failed(gh, e, limit))?;
        let (tx, lines) = mpsc::channel();
        let pipes: [Option<Box<dyn Read + Send>>; 2] = [child.stdout.take().map(|p| Box::new(p) as _), child.stderr.take().map(|p| Box::new(p) as _)];
        for pipe in pipes.into_iter().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let (mut code, mut url, mut said) = (None::<String>, None::<String>, vec![]);
        let until = Instant::now() + limit;
        let mut code_at = None;
        loop {
            if code.is_some() && (url.is_some() || code_at.is_some_and(|at: Instant| at.elapsed() > Duration::from_secs(2))) {
                break;
            }
            match lines.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    if code.is_none() {
                        code = word_after(&line, "one-time code: ");
                        code_at = code.as_ref().map(|_| Instant::now());
                    }
                    if url.is_none() {
                        url = line.split_whitespace().find(|w| w.starts_with("https://github.com/")).map(|w| w.trim_end_matches(['.', ',']).to_string());
                    }
                    said.push(line);
                }
                Err(RecvTimeoutError::Timeout) if Instant::now() < until => {}
                Err(RecvTimeoutError::Timeout) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Problem::new(format!("gh showed no one-time code within {}", span(limit)),
                                            format!("Run {} in a terminal instead.", login_command(gh))));
                }
                // gh ended before it showed a code
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = child.wait();
                    return Err(login_problem(gh, &said.join("\n")));
                }
            }
        }
        let code = code.unwrap_or_default();
        Ok(GhLogin { gh: gh.to_path_buf(), child, code, url: url.unwrap_or_else(|| DEVICE_URL.into()), lines, said })
    }

    /// Waits until gh is done: you entered the code on GitHub (Ok, with the account gh says it logged in as), gh gave
    /// up, `limit` passed or `stop` was set (gh is ended).
    pub fn wait(mut self, limit: Duration, stop: &AtomicBool) -> Result<Option<String>, Problem> {
        let until = Instant::now() + limit;
        let status = loop {
            match self.child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if stop.load(Ordering::Relaxed) => {
                    self.end();
                    return Err(Problem::plain("Login cancelled"));
                }
                Ok(None) if Instant::now() >= until => {
                    self.end();
                    return Err(Problem::new(format!("The code wasn't entered within {}", span(limit)), "Log in again."));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => return Err(Problem::plain(format!("can't follow gh's login: {e}"))),
            }
        };
        // what gh said last (its pipes close as it ends)
        let deadline = Instant::now() + Duration::from_secs(2);
        while let Ok(line) = self.lines.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            self.said.push(line);
        }
        let said = self.said.join("\n");
        if status.success() {
            Ok(word_after(&said, "Logged in as "))
        } else {
            Err(login_problem(&self.gh, &said))
        }
    }

    fn end(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Why gh's login didn't work, with what to do instead.
fn login_problem(gh: &Path, said: &str) -> Problem {
    let command = login_command(gh);
    if let Some(var) = ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN"].iter().find(|v| said.contains(&format!("{v} environment variable"))) {
        return Problem::new(format!("gh uses the token in {var}, so it can't log in another way"),
                            format!("Remove {var} from the environment Gizai starts in, or log in with {command} in a terminal."));
    }
    if said.contains("expired") {
        return Problem::new("The code expired before it was entered", "Log in again.");
    }
    if said.contains("error connecting to") {
        return Problem::new("gh can't reach GitHub", "Check your internet connection, then try again.");
    }
    let words = said.lines().map(str::trim).rfind(|l| !l.is_empty() && !l.contains("one-time code") && !l.starts_with("Open this URL"));
    match words {
        Some(w) => Problem::new(format!("gh's login didn't work: {}", w.trim_start_matches(['!', 'X', '✓', ' '])), format!("Run {command} in a terminal instead.")),
        None => Problem::new("gh's login ended without saying why", format!("Run {command} in a terminal instead.")),
    }
}
