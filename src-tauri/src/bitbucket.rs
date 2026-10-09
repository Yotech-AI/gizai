//! Settings → Bitbucket: whether Gizai can use Bitbucket Cloud.
//! - The status: the login saved in your keychain (your Atlassian account's email and an API token) and the account it
//!   belongs to, as Bitbucket says.
//! - Save login: asks Bitbucket first and keeps only a login it accepts; Remove login.
//! - Check connection: the token's account, ssh to git@bitbucket.org, and for each project with a Bitbucket link
//!   whether you can push to it (a dry run). Each check says what to do when it fails.
//!
//! Pushes go over SSH with your keys. The login lives only in the keychain: never in Gizai's database, a log, an error
//! or a command line.
use std::time::Duration;

use gizai_agents::bitbucket::{self, Login, SCOPES};
use gizai_agents::connection::{self, Problem, PushOver};
use gizai_agents::secrets::NOT_RUNNING;
use serde::Serialize;

use crate::AppState;
use crate::github::{Check, ConnectionCheck};

/// How long the SSH check may take.
const CHECK_LIMIT: Duration = Duration::from_secs(30);

/// What Settings → Bitbucket shows.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BitbucketStatus {
    /// The email of the login in the keychain.
    pub email: Option<String>,
    /// Whether an API token is saved with it.
    pub has_token: bool,
    /// Who the token belongs to, as Bitbucket names the account.
    pub account: Option<String>,
    /// Why there is no account (no login, Bitbucket refused it or couldn't be reached, the keychain is locked), with
    /// what to do.
    pub account_problem: Option<Problem>,
}

/// Where to make an API token, and the scopes it needs.
fn make_a_token() -> String {
    format!("Make an API token with scopes for Bitbucket (Atlassian account → Security → API tokens → Create API token with scopes) \
             with the scopes {}, and save it here with your Atlassian account's email.", SCOPES.join(", "))
}

fn no_login() -> Problem {
    Problem::new("No Bitbucket login yet", make_a_token())
}

/// The keychain couldn't be read, with what to do when its words don't say.
fn keychain_problem(e: String) -> Problem {
    if e == NOT_RUNNING || e.contains("then try again") {
        Problem::plain(e)
    } else {
        Problem::new(e, "Unlock your keychain, then try again.")
    }
}

/// Your Bitbucket login from the keychain, or why there is none, in plain words. Blocking (the keychain may ask its
/// service).
pub(crate) fn login(st: &AppState) -> Result<Login, String> {
    match bitbucket::saved_login(st.keychain.as_ref()) {
        Ok(Some(login)) => Ok(login),
        Ok(None) => Err("Log in to Bitbucket first (Settings → Bitbucket): Gizai opens and follows its pull requests with your email and an API token".into()),
        Err(e) => Err(e),
    }
}

/// Whether Gizai can use Bitbucket now: the login in the keychain and the account it belongs to (Bitbucket is asked).
pub async fn status(st: &AppState) -> BitbucketStatus {
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || status_blocking(&st2)).await
        .unwrap_or_else(|e| BitbucketStatus { account_problem: Some(Problem::plain(e.to_string())), ..Default::default() })
}

fn status_blocking(st: &AppState) -> BitbucketStatus {
    let login = match bitbucket::saved_login(st.keychain.as_ref()) {
        Ok(Some(login)) => login,
        Ok(None) => return BitbucketStatus { account_problem: Some(no_login()), ..Default::default() },
        Err(e) => return BitbucketStatus { account_problem: Some(keychain_problem(e)), ..Default::default() },
    };
    let mut out = BitbucketStatus { email: Some(login.email.clone()), has_token: !login.token.is_empty(), ..Default::default() };
    match bitbucket::account(&login) {
        Ok(who) => out.account = Some(who),
        Err(p) => out.account_problem = Some(p),
    }
    out
}

/// Save login: asks Bitbucket who `email` and `token` belong to (`/user`), and only when it accepts them keeps them in
/// the keychain, in place of the login there. Otherwise nothing is saved, and the error says why in plain words.
pub async fn save_login(st: &AppState, email: String, token: String) -> Result<BitbucketStatus, String> {
    let (email, token) = (email.trim().to_string(), token.trim().to_string());
    if email.is_empty() || !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Err("Fill in your Atlassian account's email: the one you log in to Bitbucket with".into());
    }
    if token.is_empty() {
        return Err(format!("Fill in an API token. {}", make_a_token()));
    }
    if token.chars().any(char::is_whitespace) {
        return Err("The API token has a space or line break in it: copy it again".into());
    }
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || {
        let login = Login { email, token };
        let account = bitbucket::account(&login).map_err(|p| format!("{p} Nothing was saved."))?;
        bitbucket::save_login(st2.keychain.as_ref(), &login).map_err(|e| format!("Bitbucket accepted the login, but it couldn't be saved: {e}"))?;
        Ok(BitbucketStatus { email: Some(login.email), has_token: true, account: Some(account), account_problem: None })
    }).await.map_err(|e| e.to_string())?
}

/// Remove login: takes the Bitbucket login out of the keychain. Pull requests on Bitbucket then wait for a new one.
pub async fn remove_login(st: &AppState) -> Result<BitbucketStatus, String> {
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || bitbucket::remove_login(st2.keychain.as_ref())).await.map_err(|e| e.to_string())?
        .map_err(|e| format!("The Bitbucket login couldn't be removed: {e}"))?;
    Ok(status(st).await)
}

/// Check connection: the account the API token belongs to, ssh to git@bitbucket.org in batch mode, and for each project
/// with a Bitbucket link whether you can push to it, the way Open pull request would (a dry run over SSH: nothing is
/// sent). Never asks for anything, and changes nothing.
pub async fn check(st: &AppState) -> ConnectionCheck {
    let st2 = st.clone();
    tokio::task::spawn_blocking(move || check_blocking(&st2)).await.unwrap_or_else(|e| ConnectionCheck {
        ok: false, push_over: "ssh".into(), checks: vec![Check::failed("Check connection", Problem::plain(e.to_string()))],
    })
}

fn check_blocking(st: &AppState) -> ConnectionCheck {
    let mut checks = vec![match bitbucket::saved_login(st.keychain.as_ref()) {
        Ok(Some(login)) => match bitbucket::account(&login) {
            Ok(who) => Check::ok("Account", format!("Logged in to Bitbucket as {who}")),
            Err(p) => Check::failed("Account", p),
        },
        Ok(None) => Check::failed("Account", no_login()),
        Err(e) => Check::failed("Account", keychain_problem(e)),
    }];
    checks.push(match connection::bitbucket_ssh_check(CHECK_LIMIT) {
        Ok(Some(who)) => Check::ok("SSH", format!("git@bitbucket.org accepts your SSH key, as {who}")),
        Ok(None) => Check::ok("SSH", "git@bitbucket.org accepts your SSH key"),
        Err(p) => Check::failed("SSH", p),
    });
    checks.extend(crate::github::project_checks(st, "bitbucket", Some(&PushOver::Bitbucket)));
    ConnectionCheck { ok: !checks.iter().any(|c| c.result == "failed"), push_over: "ssh".into(), checks }
}
