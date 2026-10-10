//! The connection to GitHub without GitHub: Push over's one-command git settings (checked with git's own URL
//! expansion and credential helpers), plain words for a failed push, gh's status and login with a fake gh, and a push
//! over HTTPS to a local server that refuses every login. Nothing here runs ssh, so nothing can reach github.com.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::connection::{self, GhLogin, Problem, PushOver};
use gizai_agents::worktree;

const LIMIT: Duration = Duration::from_secs(10);
const TOKEN: &str = "gho_FAKEtoken0123456789";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// git with Push over's settings for this one command, the way Gizai runs it.
fn git_over(dir: &Path, over: &PushOver, args: &[&str], stdin: &str) -> String {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir);
    for c in over.git_config() {
        cmd.arg("-c").arg(c);
    }
    let mut child = cmd.args(args).env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (a leaked
/// handle would make running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake gh in `dir`. Each call's arguments go to dir/gh-args. Logged in when dir/logged-in holds an account name.
/// `auth login` shows a one-time code and a link, then waits until dir/code-entered holds the account to log in as
/// (or ends with dir/login.err when that exists). `auth git-credential get` answers with a token when logged in.
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d=$(dirname "$0")
echo "$*" >> "$d/gh-args"
case "$*" in
  --version) echo "gh version 2.62.0 (2024-11-14)"; echo "https://github.com/cli/cli/releases/tag/v2.62.0"; exit 0 ;;
  "auth status"*)
    if [ -f "$d/logged-in" ]; then
      echo "github.com"
      echo "  ✓ Logged in to github.com account $(cat "$d/logged-in") (keyring)"
      echo "  - Active account: true"
      echo "  - Token: gho_************************************"
      exit 0
    fi
    echo "You are not logged into any GitHub hosts. To log in, run: gh auth login" >&2; exit 1 ;;
  "auth login"*)
    if [ -f "$d/login.err" ]; then cat "$d/login.err" >&2; exit 1; fi
    echo "! First copy your one-time code: 4F2A-9C1B" >&2
    echo "Open this URL to continue in your web browser: https://github.com/login/device" >&2
    while [ ! -f "$d/code-entered" ]; do sleep 0.1; done
    echo "✓ Authentication complete." >&2
    cp "$d/code-entered" "$d/logged-in"
    echo "✓ Logged in as $(cat "$d/logged-in")" >&2
    exit 0 ;;
  "auth git-credential"*)
    cat > "$d/credential-asked"
    [ -f "$d/logged-in" ] || exit 1
    printf 'protocol=http\nhost=github.com\nusername=x-access-token\npassword={TOKEN}\n'; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#));
    gh
}

fn gh_calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("gh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// A repository with one commit on main and one on `branch`.
fn repo(tmp: &Path, branch: &str) -> PathBuf {
    let r = tmp.join("repo");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    git(&r, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    git(&r, &["branch", branch]);
    r
}

// ---- Push over: the -c settings for one git command ----

#[test]
fn over_ssh_a_github_https_address_goes_to_git_at_github_com() {
    let tmp = tempfile::tempdir().unwrap();
    let r = repo(tmp.path(), "card");
    for from in ["https://github.com/acme/shop", "https://github.com/acme/shop.git", "http://github.com/acme/shop", "https://www.github.com/acme/shop"] {
        assert_eq!(git_over(&r, &PushOver::Ssh, &["ls-remote", "--get-url", from], ""), from.replace(&from[..from.find("acme").unwrap()], "git@github.com:"), "{from}");
    }
    // an ssh address and another host stay as they are
    assert_eq!(git_over(&r, &PushOver::Ssh, &["ls-remote", "--get-url", "git@github.com:acme/shop.git"], ""), "git@github.com:acme/shop.git");
    assert_eq!(git_over(&r, &PushOver::Ssh, &["ls-remote", "--get-url", "https://gitlab.com/acme/shop"], ""), "https://gitlab.com/acme/shop");
    // a remote with an https URL pushes over ssh, and keeps its URL
    git(&r, &["remote", "add", "origin", "https://github.com/acme/shop"]);
    assert_eq!(git_over(&r, &PushOver::Ssh, &["ls-remote", "--get-url", "origin"], ""), "git@github.com:acme/shop");
    assert_eq!(git(&r, &["remote", "get-url", "origin"]), "https://github.com/acme/shop");
}

#[test]
fn a_longer_rewrite_in_your_own_config_wins_over_the_ssh_one() {
    // what the GA-17 tests rely on: https://github.com/acme/ goes to a local folder
    let tmp = tempfile::tempdir().unwrap();
    let r = repo(tmp.path(), "card");
    git(&r, &["config", "url./srv/mirror/acme/.insteadOf", "https://github.com/acme/"]);
    assert_eq!(git_over(&r, &PushOver::Ssh, &["ls-remote", "--get-url", "https://github.com/acme/shop"], ""), "/srv/mirror/acme/shop");
    assert_eq!(git_over(&r, &PushOver::Ssh, &["ls-remote", "--get-url", "https://github.com/other/app"], ""), "git@github.com:other/app");
}

#[test]
fn over_https_an_ssh_address_goes_to_https_and_only_ghs_login_is_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let r = repo(tmp.path(), "card");
    let gh = fake_gh(&tmp.path().join("gh"));
    let over = PushOver::Https { gh: gh.clone() };
    assert_eq!(git_over(&r, &over, &["ls-remote", "--get-url", "git@github.com:acme/shop.git"], ""), "https://github.com/acme/shop.git");
    assert_eq!(git_over(&r, &over, &["ls-remote", "--get-url", "ssh://git@github.com/acme/shop.git"], ""), "https://github.com/acme/shop.git");
    assert_eq!(git_over(&r, &over, &["ls-remote", "--get-url", "https://github.com/acme/shop"], ""), "https://github.com/acme/shop");
    // your own credential helper never runs: the empty helper clears it, then gh's login answers
    let own = tmp.path().join("own-helper");
    write_script(&own, &format!("#!/bin/sh\necho \"$*\" >> '{}/own-helper-ran'\n", tmp.path().display()));
    git(&r, &["config", "credential.helper", &format!("!{}", own.display())]);
    std::fs::write(gh.parent().unwrap().join("logged-in"), "octocat").unwrap();
    let filled = git_over(&r, &over, &["credential", "fill"], "protocol=https\nhost=github.com\npath=acme/shop.git\n\n");
    assert!(filled.contains(&format!("password={TOKEN}")), "{filled}");
    assert!(gh_calls(gh.parent().unwrap()).contains(&"auth git-credential get".to_string()), "{:?}", gh_calls(gh.parent().unwrap()));
    assert!(!tmp.path().join("own-helper-ran").exists(), "your own helper isn't asked");
}

#[test]
fn a_gh_path_with_spaces_or_quotes_still_runs_as_the_credential_helper() {
    let tmp = tempfile::tempdir().unwrap();
    let r = repo(tmp.path(), "card");
    let gh = fake_gh(&tmp.path().join("Jeff's tools/g h"));
    std::fs::write(gh.parent().unwrap().join("logged-in"), "octocat").unwrap();
    let filled = git_over(&r, &PushOver::Https { gh: gh.clone() }, &["credential", "fill"], "protocol=https\nhost=github.com\n\n");
    assert!(filled.contains(&format!("password={TOKEN}")), "{filled}");
}

#[test]
fn neither_way_changes_git_config_or_remotes() {
    let tmp = tempfile::tempdir().unwrap();
    let r = repo(tmp.path(), "card");
    git(&r, &["remote", "add", "origin", "https://github.com/acme/shop"]);
    let config = std::fs::read_to_string(r.join(".git/config")).unwrap();
    let gh = fake_gh(&tmp.path().join("gh"));
    for over in [PushOver::Ssh, PushOver::Https { gh }] {
        // a dry run to a remote that doesn't exist fails, and leaves the config as it was
        let _ = worktree::can_push(&r, &tmp.path().join("nowhere.git").display().to_string(), &over, LIMIT);
        assert_eq!(std::fs::read_to_string(r.join(".git/config")).unwrap(), config, "{over:?}");
    }
}

// ---- a failed push says what to check ----

#[test]
fn a_failed_push_says_what_to_check_in_plain_words() {
    let gh = PushOver::Https { gh: "/usr/bin/gh".into() };
    let ssh = PushOver::Ssh;
    let said = |p: Problem| p.to_string();
    // the key isn't on the account, or isn't loaded
    let p = connection::push_problem("git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n", &ssh);
    assert_eq!(p.what, "GitHub didn't accept your SSH key");
    assert!(said(p.clone()).contains("github.com/settings/keys") && said(p).contains("ssh-add"));
    // the host key isn't trusted
    let p = connection::push_problem("Host key verification failed.\nfatal: Could not read from remote repository.\n", &ssh);
    assert_eq!(p.what, "This computer doesn't trust GitHub's SSH host key yet");
    assert!(p.fix.unwrap().contains("ssh -T git@github.com"));
    // ssh missing
    for missing in ["error: cannot run ssh: No such file or directory\nfatal: unable to fork", "sh: 1: ssh: not found", "sh: ssh: command not found"] {
        assert_eq!(connection::push_problem(missing, &ssh).what, "ssh isn't installed", "{missing}");
    }
    // gh not logged in, over HTTPS
    let p = connection::push_problem("fatal: could not read Username for 'https://github.com': terminal prompts disabled\n", &gh);
    assert_eq!(p.what, "The GitHub CLI isn't logged in, so git has no login for HTTPS");
    assert!(p.fix.unwrap().contains("Log in with GitHub"));
    // a login GitHub refuses, over HTTPS
    let p = connection::push_problem("remote: Invalid username or token.\nfatal: Authentication failed for 'https://github.com/acme/shop.git/'\n", &gh);
    assert_eq!(p.what, "GitHub refused gh's login for HTTPS");
    // over SSH, an https remote without a login
    let p = connection::push_problem("fatal: could not read Username for 'https://github.com': terminal prompts disabled\n", &ssh);
    assert_eq!(p.what, "git tried HTTPS, and has no login for it");
    // no answer in time
    let p = connection::no_answer(Duration::from_secs(120));
    assert_eq!(said(p), "GitHub gave no answer within 2 minutes. Check your internet connection, then try again.");
    // GitHub can't be reached
    let p = connection::push_problem("ssh: connect to host github.com port 22: Connection timed out\nfatal: Could not read from remote repository.\n", &ssh);
    assert_eq!(p.what, "Can't reach github.com over SSH");
    let p = connection::push_problem("fatal: unable to access 'https://github.com/acme/shop.git/': Could not resolve host: github.com\n", &gh);
    assert_eq!(p.what, "Can't reach github.com");
    // no access, a missing repository, a branch that moved on
    let p = connection::push_problem("ERROR: Permission to acme/shop.git denied to octocat.\nfatal: Could not read from remote repository.\n", &ssh);
    assert_eq!(p.what, "Your GitHub account octocat can't push to this repository");
    let p = connection::push_problem("ERROR: Repository not found.\nfatal: Could not read from remote repository.\n", &ssh);
    assert!(p.what.contains("no repository at this address"), "{p:?}");
    let p = connection::push_problem(" ! [rejected]        card -> card (fetch first)\nerror: failed to push some refs to 'git@github.com:acme/shop.git'\n", &ssh);
    assert!(p.fix.unwrap().contains("never forces"));
    // anything else: git's own telling line, without its prefix
    let p = connection::push_problem("fatal: 'nowhere' does not appear to be a git repository\nfatal: Could not read from remote repository.\n", &ssh);
    assert_eq!(p, Problem::plain("'nowhere' does not appear to be a git repository"));
}

// ---- the GitHub CLI: version, account, login ----

#[test]
fn gh_says_its_version_and_whether_it_is_logged_in() {
    let tmp = tempfile::tempdir().unwrap();
    let gh = fake_gh(&tmp.path().join("gh"));
    assert_eq!(connection::gh_version(&gh, LIMIT), Ok("2.62.0".into()));
    let p = connection::gh_account(&gh, LIMIT).unwrap_err();
    assert_eq!(p.what, "The GitHub CLI isn't logged in");
    assert!(p.fix.unwrap().contains("Log in with GitHub"));
    std::fs::write(tmp.path().join("gh/logged-in"), "octocat").unwrap();
    assert_eq!(connection::gh_account(&gh, LIMIT), Ok("octocat".into()));
    assert!(gh_calls(&tmp.path().join("gh")).contains(&"auth status --hostname github.com".to_string()));
    // gh missing: where to get it
    let p = connection::gh_version(Path::new("/nonexistent/gh"), LIMIT).unwrap_err();
    assert_eq!(p.what, "The GitHub CLI isn't at /nonexistent/gh");
    assert!(p.fix.unwrap().contains("cli.github.com"));
}

#[test]
fn reads_the_account_from_each_kind_of_gh_auth_status() {
    assert_eq!(connection::account_from_status("github.com\n  ✓ Logged in to github.com account octocat (keyring)\n  - Active account: true\n"), Ok("octocat".into()));
    assert_eq!(connection::account_from_status("github.com\n  ✓ Logged in to github.com as hubot (oauth_token)\n"), Ok("hubot".into()));
    let p = connection::account_from_status("github.com\n  X Failed to log in to github.com account octocat (keyring)\n  - The token in keyring is invalid.\n").unwrap_err();
    assert!(p.what.contains("as octocat") && p.what.contains("doesn't work"), "{p:?}");
    let p = connection::account_from_status("You are not logged into any GitHub hosts. To log in, run: gh auth login\n").unwrap_err();
    assert_eq!(p.what, "The GitHub CLI isn't logged in");
    let p = connection::account_from_status("error connecting to api.github.com\ncheck your internet connection or https://githubstatus.com\n").unwrap_err();
    assert_eq!(p.what, "gh can't reach GitHub");
}

#[test]
fn the_terminal_command_names_gh_by_its_path_unless_it_is_on_path() {
    assert_eq!(connection::login_command(Path::new("/opt/my tools/gh")), "'/opt/my tools/gh' auth login --hostname github.com --web");
    let on_path = std::env::split_paths(&std::env::var_os("PATH").unwrap()).next().unwrap().join("gh");
    assert_eq!(connection::login_command(&on_path), "gh auth login --hostname github.com --web");
}

#[test]
fn log_in_with_github_shows_ghs_code_and_link_and_ends_with_the_account() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("gh");
    let gh = fake_gh(&dir);
    let login = GhLogin::start(&gh, LIMIT).unwrap();
    assert_eq!((login.code.as_str(), login.url.as_str()), ("4F2A-9C1B", "https://github.com/login/device"));
    assert!(gh_calls(&dir).iter().any(|l| l.starts_with("auth login --web --hostname github.com")), "{:?}", gh_calls(&dir));
    std::fs::write(dir.join("code-entered"), "octocat").unwrap(); // you enter the code on GitHub
    assert_eq!(login.wait(LIMIT, &AtomicBool::new(false)), Ok(Some("octocat".into())));
    assert_eq!(connection::gh_account(&gh, LIMIT), Ok("octocat".into()));
}

#[test]
fn a_login_can_be_cancelled_and_one_that_cannot_work_says_what_to_run() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("gh");
    let gh = fake_gh(&dir);
    let login = GhLogin::start(&gh, LIMIT).unwrap();
    let started = Instant::now();
    assert_eq!(login.wait(LIMIT, &AtomicBool::new(true)), Err(Problem::plain("Login cancelled")));
    assert!(started.elapsed() < Duration::from_secs(3));
    // the code isn't entered in time
    let login = GhLogin::start(&gh, LIMIT).unwrap();
    let p = login.wait(Duration::from_millis(300), &AtomicBool::new(false)).unwrap_err();
    assert!(p.what.starts_with("The code wasn't entered within"), "{p:?}");
    assert!(!dir.join("logged-in").exists());
    // gh can't log in this way: why, and the command to run instead
    std::fs::write(dir.join("login.err"), "The value of the GH_TOKEN environment variable is being used for authentication.\n").unwrap();
    let p = GhLogin::start(&gh, LIMIT).err().unwrap();
    assert!(p.what.contains("GH_TOKEN"), "{p:?}");
    assert!(p.fix.unwrap().contains("auth login --hostname github.com --web"));
    std::fs::write(dir.join("login.err"), "something went wrong\n").unwrap();
    let p = GhLogin::start(&gh, LIMIT).err().unwrap();
    assert_eq!(p.what, "gh's login didn't work: something went wrong");
    assert!(p.fix.unwrap().starts_with("Run "));
}

// ---- a push over HTTPS, to a local server that refuses every login ----

/// A local "GitHub" over plain HTTP that answers every request with 401; it keeps each request's head.
fn refusing_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/acme/shop.git", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let heads = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && s.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            heads.lock().unwrap().push(String::from_utf8_lossy(&head).to_string());
            let _ = s.write_all(b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"GitHub\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    (url, seen)
}

#[test]
fn over_https_a_push_uses_ghs_login_never_asks_and_never_shows_the_token() {
    let tmp = tempfile::tempdir().unwrap();
    let r = repo(tmp.path(), "card");
    let dir = tmp.path().join("gh");
    let gh = fake_gh(&dir);
    // anything that would ask you: an askpass program, and your own credential helper
    let ask = tmp.path().join("askpass");
    write_script(&ask, &format!("#!/bin/sh\necho \"$*\" >> '{}/asked'\necho secret\n", tmp.path().display()));
    git(&r, &["config", "core.askPass", ask.to_str().unwrap()]);
    git(&r, &["config", "credential.helper", &format!("!{}", ask.display())]);
    let config = std::fs::read_to_string(r.join(".git/config")).unwrap();
    let (url, seen) = refusing_server();
    let over = PushOver::Https { gh };

    // gh isn't logged in: no login to give, and nothing asks for one
    let started = Instant::now();
    let e = worktree::push_branch_over(&r, &url, "card", &over).unwrap_err().to_string();
    assert_eq!(e, format!("git: Couldn't push card to {url}: The GitHub CLI isn't logged in, so git has no login for HTTPS. \
                           Use Log in with GitHub in Settings → GitHub, or run gh auth login in a terminal."));
    assert!(started.elapsed() < Duration::from_secs(20), "it never waits for an answer");
    assert!(dir.join("credential-asked").exists(), "gh's login was asked");
    assert!(!tmp.path().join("asked").exists(), "nothing asked for a username or password");

    // logged in, and GitHub refuses the login: gh's token was sent, and is never shown
    std::fs::write(dir.join("logged-in"), "octocat").unwrap();
    let e = worktree::push_branch_over(&r, &url, "card", &over).unwrap_err().to_string();
    assert!(e.contains("GitHub refused gh's login for HTTPS"), "{e}");
    assert!(!e.contains(TOKEN), "{e}");
    assert!(seen.lock().unwrap().iter().any(|h| h.contains("Authorization: Basic")), "the login went with the request");
    assert!(!tmp.path().join("asked").exists());
    // and the dry run of Check connection says the same
    let p = worktree::can_push(&r, &url, &over, LIMIT).unwrap_err();
    assert_eq!(p.what, "GitHub refused gh's login for HTTPS");
    assert_eq!(std::fs::read_to_string(r.join(".git/config")).unwrap(), config, "no git config changed");
}
