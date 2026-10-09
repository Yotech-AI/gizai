//! GA-59: pushes to Bitbucket Cloud without Bitbucket. A fake ssh as your own GIT_SSH_COMMAND (kept, as Gizai keeps one
//! you set) logs its arguments and serves "Bitbucket" from local bare repositories, only to git@bitbucket.org, so
//! nothing here reaches bitbucket.org or github.com. Every test in this file that runs git or ssh runs that fake ssh.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use gizai_agents::connection::{self, Problem, PushOver};
use gizai_agents::worktree;

const LIMIT: Duration = Duration::from_secs(20);

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// The fake ssh's folder: base/ssh-args (one line per call) and "Bitbucket" at base/bitbucket/<workspace>/<repo>(.git)
/// (found with or without .git). It serves only git@bitbucket.org. A file fake-ssh-mode in a bare repository makes ssh
/// fail for it: publickey or hostkey. `ssh -T git@bitbucket.org` answers the way base/t-mode says: ok, ok-exit1,
/// loggedin, publickey or hostkey.
fn fake_ssh() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let base = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap().keep();
        let ssh = base.join("bin/ssh");
        std::fs::create_dir_all(ssh.parent().unwrap()).unwrap();
        write_script(&ssh, &format!(r#"#!/bin/sh
base='{base}'
[ "$1" = "-G" ] && exit 1
echo "$*" >> "$base/ssh-args"
cmd=""
host=""
for a in "$@"; do
  case "$a" in
    git-receive-pack*|git-upload-pack*) cmd="$a" ;;
    git@bitbucket.org) host="bitbucket" ;;
  esac
done
if [ "$host" != "bitbucket" ]; then
  echo "fake ssh: only git@bitbucket.org is served here, not: $*" >&2; exit 255
fi
shell="You can use git to connect to Bitbucket. Shell access is disabled"
if [ -z "$cmd" ]; then
  case "$(cat "$base/t-mode" 2>/dev/null)" in
    ok) echo "authenticated via ssh key."; echo; echo "$shell"; exit 0 ;;
    ok-exit1) echo "authenticated via ssh key."; echo; echo "$shell"; exit 1 ;;
    loggedin) echo "logged in as jefsev."; echo; echo "$shell"; exit 0 ;;
    publickey) echo "git@bitbucket.org: Permission denied (publickey)." >&2; exit 255 ;;
    hostkey) echo "Host key verification failed." >&2; exit 255 ;;
  esac
  echo "fake ssh: no t-mode" >&2; exit 255
fi
path="${{cmd#* }}"
path="$(printf '%s' "$path" | tr -d "'")"
path="${{path#/}}"
repo="$base/bitbucket/$path"
[ -d "$repo" ] || repo="$repo.git"
case "$(cat "$repo/fake-ssh-mode" 2>/dev/null)" in
  publickey) echo "git@bitbucket.org: Permission denied (publickey)." >&2; exit 255 ;;
  hostkey) echo "Host key verification failed." >&2; exit 255 ;;
esac
verb="${{cmd%% *}}"
exec git "${{verb#git-}}" "$repo"
"#, base = base.display()));
        // SAFETY: set once, before this test binary starts any process (every test that does calls fake_ssh first)
        unsafe { std::env::set_var("GIT_SSH_COMMAND", &ssh) };
        base
    })
}

fn ssh_calls() -> Vec<String> {
    std::fs::read_to_string(fake_ssh().join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// Whether the fake ssh was asked to receive a push into `path` (as git sends it) on git@bitbucket.org.
fn pushed_over_ssh(path: &str) -> bool {
    ssh_calls().iter().any(|c| c.contains("git@bitbucket.org") && c.contains(&format!("git-receive-pack '{path}'")))
}

/// "Bitbucket" with a bare repository at base/bitbucket/<bare> behind the fake ssh, and a local clone of it whose remote
/// origin is `origin`, the way a project links to Bitbucket. The clone allows no https, so a push that didn't go over
/// ssh fails. Its branch card has a commit Bitbucket doesn't.
fn bitbucket(tmp: &Path, bare: &str, origin: &str) -> (PathBuf, PathBuf) {
    let base = fake_ssh();
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "master"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = base.join("bitbucket").join(bare);
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&local, &["remote", "set-url", "origin", origin]);
    git(&local, &["config", "protocol.https.allow", "never"]);
    git(&local, &["checkout", "-q", "-b", "card"]);
    commit(&local, "the card's work");
    git(&local, &["checkout", "-q", "master"]);
    (bare, local)
}

fn commit(local: &Path, message: &str) {
    git(local, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", message]);
}

/// The clone's own git config, as a file and as git lists it.
fn config(local: &Path) -> (String, String) {
    (std::fs::read_to_string(local.join(".git/config")).unwrap(), git(local, &["config", "--list", "--local"]))
}

fn no_github(text: &str) {
    assert!(!text.to_lowercase().contains("github"), "GitHub in a Bitbucket problem: {text}");
}

// ---- pushes go over SSH to git@bitbucket.org ----

#[test]
fn a_push_to_a_bitbucket_https_link_goes_over_ssh_to_git_at_bitbucket_org_and_changes_no_config() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = bitbucket(tmp.path(), "acme/shop.git", "https://bitbucket.org/acme/shop");
    let before = config(&local);
    // through the remote
    worktree::push_branch_over(&local, "origin", "card", &PushOver::Bitbucket).unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]), "the branch is on Bitbucket");
    assert!(pushed_over_ssh("acme/shop"), "{:?}", ssh_calls());
    // to the link itself
    git(&local, &["checkout", "-q", "card"]);
    commit(&local, "more");
    worktree::push_branch_over(&local, "https://bitbucket.org/acme/shop", "card", &PushOver::Bitbucket).unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]), "the newer commit is on Bitbucket");
    // nothing changed in the repository's config or remotes
    assert_eq!(config(&local), before);
    assert_eq!(git(&local, &["config", "--get", "remote.origin.url"]), "https://bitbucket.org/acme/shop");
    assert!(!config(&local).1.to_lowercase().contains("insteadof"), "{}", config(&local).1);
    // without PushOver::Bitbucket the link stays https, which this clone doesn't allow: the rewrite is Bitbucket's
    commit(&local, "and more");
    let e = worktree::push_branch_over(&local, "origin", "card", &PushOver::Ssh).unwrap_err().to_string();
    assert!(e.contains("transport 'https' not allowed"), "{e}");
    assert_ne!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]));
}

#[test]
fn a_link_with_a_user_name_in_it_goes_over_ssh_too() {
    let tmp = tempfile::tempdir().unwrap();
    // what Bitbucket's Clone button gives
    let (bare, local) = bitbucket(tmp.path(), "acme/web.git", "https://jefsev@bitbucket.org/acme/web.git");
    let before = config(&local);
    worktree::push_branch_over(&local, "origin", "card", &PushOver::Bitbucket).unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]), "the branch is on Bitbucket");
    assert!(pushed_over_ssh("acme/web.git"), "{:?}", ssh_calls());
    // and to that link itself
    git(&local, &["checkout", "-q", "card"]);
    commit(&local, "more");
    worktree::push_branch_over(&local, "https://jefsev@bitbucket.org/acme/web.git", "card", &PushOver::Bitbucket).unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]));
    assert_eq!(config(&local), before, "no git config or remote changed");
    assert_eq!(git(&local, &["config", "--get", "remote.origin.url"]), "https://jefsev@bitbucket.org/acme/web.git");
}

#[test]
fn a_push_url_with_a_user_name_goes_over_ssh_too() {
    let tmp = tempfile::tempdir().unwrap();
    // fetches over ssh from a repository that isn't there, pushes to the user@ link: only the push URL can work
    let (bare, local) = bitbucket(tmp.path(), "acme/pushurl.git", "git@bitbucket.org:acme/elsewhere.git");
    git(&local, &["remote", "set-url", "--push", "origin", "https://tjitske@bitbucket.org/acme/pushurl.git"]);
    let before = config(&local);
    worktree::push_branch_over(&local, "origin", "card", &PushOver::Bitbucket).unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]), "the branch is on Bitbucket");
    assert!(pushed_over_ssh("acme/pushurl.git"), "{:?}", ssh_calls());
    assert_eq!(config(&local), before, "no git config or remote changed");
    assert_eq!(git(&local, &["config", "--get", "remote.origin.pushurl"]), "https://tjitske@bitbucket.org/acme/pushurl.git");
    assert_eq!(git(&local, &["config", "--get", "remote.origin.url"]), "git@bitbucket.org:acme/elsewhere.git");
}

#[test]
fn a_link_with_a_password_in_it_is_left_alone_and_its_password_never_shows() {
    let link = "https://jefsev:s3cretPASS@bitbucket.org/acme/pw.git";
    // no rule for it: its password never goes in a command line
    let rules = PushOver::Bitbucket.git_config_for(&[link.to_string()]);
    assert!(rules.iter().all(|r| !r.contains("s3cretPASS") && !r.contains("jefsev")), "{rules:?}");
    assert!(PushOver::Bitbucket.git_config_for(&["https://jefsev@bitbucket.org/acme/web.git".to_string()])
        .contains(&"url.git@bitbucket.org:.insteadOf=https://jefsev@bitbucket.org/".to_string()), "a user name alone gets one");

    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = bitbucket(tmp.path(), "acme/pw.git", link);
    let e = worktree::push_branch_over(&local, "origin", "card", &PushOver::Bitbucket).unwrap_err().to_string();
    assert!(e.contains("Couldn't push card to origin: transport 'https' not allowed"), "it stayed https, a plain message: {e}");
    assert!(!e.contains("s3cretPASS"), "{e}");
    let p = worktree::can_push(&local, "origin", &PushOver::Bitbucket, LIMIT).unwrap_err();
    assert!(!p.to_string().contains("s3cretPASS") && !format!("{p:?}").contains("s3cretPASS"), "{p:?}");
    assert!(!ssh_calls().iter().any(|c| c.contains("acme/pw")), "not over ssh: {:?}", ssh_calls());
    assert!(!has_branch(&bare, "card"), "nothing pushed");
}

// ---- a failed push says what to check, in Bitbucket's terms ----

#[test]
fn a_key_or_host_key_bitbucket_refuses_is_said_in_bitbucket_terms_and_pushes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = bitbucket(tmp.path(), "acme/locked.git", "https://bitbucket.org/acme/locked");
    std::fs::write(bare.join("fake-ssh-mode"), "publickey").unwrap();
    let e = worktree::push_branch_over(&local, "origin", "card", &PushOver::Bitbucket).unwrap_err().to_string();
    assert!(e.contains("Couldn't push card to origin: Bitbucket didn't accept your SSH key."), "{e}");
    assert!(e.contains("Bitbucket → Personal settings → SSH keys"), "{e}");
    no_github(&e);
    std::fs::write(bare.join("fake-ssh-mode"), "hostkey").unwrap();
    let e = worktree::push_branch_over(&local, "origin", "card", &PushOver::Bitbucket).unwrap_err().to_string();
    assert!(e.contains("This computer doesn't trust Bitbucket's SSH host key yet. Run ssh -T git@bitbucket.org once in a terminal"), "{e}");
    no_github(&e);
    assert!(pushed_over_ssh("acme/locked"), "{:?}", ssh_calls());
    assert!(!has_branch(&bare, "card"), "nothing pushed");
}

#[test]
fn a_failed_push_to_bitbucket_says_what_to_check_in_bitbucket_terms() {
    let bb = PushOver::Bitbucket;
    let tail = "fatal: Could not read from remote repository.\n\nPlease make sure you have the correct access rights\nand the repository exists.\n";
    let cases = [
        (format!("git@bitbucket.org: Permission denied (publickey).\n{tail}"), "Bitbucket didn't accept your SSH key"),
        (format!("Host key verification failed.\n{tail}"), "This computer doesn't trust Bitbucket's SSH host key yet"),
        (format!("conq: repository does not exist.\n{tail}"), "Bitbucket has no repository at this address that your account can see"),
        (format!("remote: Repository not found\n{tail}"), "Bitbucket has no repository at this address that your account can see"),
        (format!("conq: repository access denied. access via a deployment key is read-only.\n{tail}"), "Your Bitbucket account can't push to this repository"),
        (" ! [rejected]        card -> card (fetch first)\nerror: failed to push some refs to 'bitbucket.org:acme/shop.git'\n".to_string(),
         "The branch on Bitbucket has commits this one doesn't"),
        ("fatal: unable to access 'https://bitbucket.org/acme/shop.git/': Could not resolve host: bitbucket.org\n".to_string(), "Can't reach bitbucket.org"),
        (format!("ssh: Could not resolve hostname bitbucket.org: Name or service not known\n{tail}"), "Can't reach bitbucket.org over SSH"),
        ("fatal: could not read Username for 'https://bitbucket.org': terminal prompts disabled\n".to_string(), "git tried HTTPS, and has no login for it"),
        ("error: cannot run ssh: No such file or directory\nfatal: unable to fork\n".to_string(), "ssh isn't installed"),
        ("error: src refspec card does not match any\nerror: failed to push some refs to 'bitbucket.org:acme/shop.git'\n".to_string(),
         "The repository has no commits yet"),
    ];
    for (said, what) in &cases {
        let p = connection::push_problem(said, &bb);
        assert_eq!(p.what, *what, "{said}");
        assert!(p.fix.is_some(), "a fix for: {said}");
        no_github(&p.to_string());
    }
    let fix = |said: &str| connection::push_problem(said, &bb).fix.unwrap();
    let key = fix(&cases[0].0);
    assert!(key.contains("Bitbucket → Personal settings → SSH keys") && key.contains("ssh-add"), "{key}");
    assert!(fix(&cases[1].0).contains("ssh -T git@bitbucket.org"));
    assert!(fix(&cases[2].0).contains("Bitbucket link"));
    let rejected = fix(&cases[5].0);
    assert!(rejected.contains("never forces") && rejected.contains("Bitbucket's copy"), "{rejected}");
    assert!(fix(&cases[8].0).contains("over SSH"));
    assert!(fix(&cases[9].0).contains("Bitbucket over SSH"));
    // anything else: git's own telling line
    let p = connection::push_problem("fatal: 'nowhere' does not appear to be a git repository\nfatal: Could not read from remote repository.\n", &bb);
    assert_eq!(p, Problem::plain("'nowhere' does not appear to be a git repository"));
    // no answer in time
    assert_eq!(connection::push_no_answer(&bb, Duration::from_secs(2)).to_string(),
               "Bitbucket gave no answer within 2 seconds. Check your internet connection, then try again.");
}

#[test]
fn the_same_words_from_a_push_to_github_still_speak_github() {
    let ssh = PushOver::Ssh;
    let p = connection::push_problem("git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.\n", &ssh);
    assert_eq!(p.what, "GitHub didn't accept your SSH key");
    assert!(p.fix.unwrap().contains("github.com/settings/keys"));
    // even with Bitbucket's own words in it, a push over PushOver::Ssh is GitHub's
    let p = connection::push_problem("git@bitbucket.org: Permission denied (publickey).\n", &ssh);
    assert_eq!(p.what, "GitHub didn't accept your SSH key");
    let p = connection::push_problem("Host key verification failed.\n", &ssh);
    assert!(p.fix.unwrap().contains("ssh -T git@github.com"));
    let p = connection::push_problem("ERROR: Repository not found.\n", &ssh);
    assert_eq!(p.what, "GitHub has no repository at this address that your account can see");
    assert_eq!(connection::push_no_answer(&ssh, Duration::from_secs(2)).what, "GitHub gave no answer within 2 seconds");
}

// ---- Check connection: a dry run, and ssh -T git@bitbucket.org ----

#[test]
fn can_push_to_bitbucket_is_a_dry_run_that_sends_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = bitbucket(tmp.path(), "acme/check.git", "https://bitbucket.org/acme/check");
    let before = config(&local);
    worktree::can_push(&local, "origin", &PushOver::Bitbucket, LIMIT).unwrap();
    assert!(pushed_over_ssh("acme/check"), "{:?}", ssh_calls());
    assert!(!has_branch(&bare, "gizai-connection-check") && !has_branch(&bare, "card"), "nothing was pushed");
    assert_eq!(config(&local), before);
    // a key Bitbucket refuses: in its terms
    std::fs::write(bare.join("fake-ssh-mode"), "publickey").unwrap();
    let p = worktree::can_push(&local, "origin", &PushOver::Bitbucket, LIMIT).unwrap_err();
    assert_eq!(p.what, "Bitbucket didn't accept your SSH key");
    assert!(p.fix.as_deref().is_some_and(|f| f.contains("Bitbucket → Personal settings → SSH keys")), "{p:?}");
    no_github(&p.to_string());
    assert!(!has_branch(&bare, "gizai-connection-check"));
}

/// One test at a time may use base/t-mode.
static T_MODE: Mutex<()> = Mutex::new(());

fn t_mode(mode: &str) {
    std::fs::write(fake_ssh().join("t-mode"), mode).unwrap();
}

#[test]
fn the_bitbucket_ssh_check_goes_by_bitbucket_words_not_the_exit_code() {
    let _one = T_MODE.lock().unwrap_or_else(|e| e.into_inner());
    t_mode("ok");
    assert_eq!(connection::bitbucket_ssh_check(LIMIT), Ok(None));
    t_mode("ok-exit1");
    assert_eq!(connection::bitbucket_ssh_check(LIMIT), Ok(None), "accepted although ssh ended with 1");
    t_mode("loggedin");
    assert_eq!(connection::bitbucket_ssh_check(LIMIT), Ok(Some("jefsev".into())));
    assert!(ssh_calls().iter().any(|c| c == "-T git@bitbucket.org"), "{:?}", ssh_calls());
}

#[test]
fn the_bitbucket_ssh_check_says_where_the_key_goes_and_how_to_trust_the_host() {
    let _one = T_MODE.lock().unwrap_or_else(|e| e.into_inner());
    t_mode("publickey");
    let p = connection::bitbucket_ssh_check(LIMIT).unwrap_err();
    assert_eq!(p.what, "Bitbucket didn't accept your SSH key");
    assert!(p.fix.as_deref().is_some_and(|f| f.contains("Personal settings → SSH keys")), "{p:?}");
    no_github(&p.to_string());
    t_mode("hostkey");
    let p = connection::bitbucket_ssh_check(LIMIT).unwrap_err();
    assert_eq!(p.what, "This computer doesn't trust Bitbucket's SSH host key yet");
    assert!(p.fix.as_deref().is_some_and(|f| f.contains("ssh -T git@bitbucket.org")), "{p:?}");
    no_github(&p.to_string());
    assert!(ssh_calls().iter().any(|c| c == "-T git@bitbucket.org"), "{:?}", ssh_calls());
}
