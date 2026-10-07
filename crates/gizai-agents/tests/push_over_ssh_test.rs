//! Push branch over SSH, with a fake ssh as your own GIT_SSH_COMMAND (kept, as Gizai keeps one you set): it logs its
//! arguments and serves "GitHub" from local bare repositories, so nothing here reaches github.com. Every test in this
//! file runs with that fake ssh.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gizai_agents::connection::{self, PushOver};
use gizai_agents::worktree;

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

/// The fake ssh's folder: base/ssh-args (one line per call) and "GitHub" at base/github/<owner>/<name>.git (found
/// with or without .git, as GitHub does). A file
/// fake-ssh-mode in a bare repository makes ssh fail for it: publickey, hostkey or slow. `ssh -T git@github.com`
/// answers the way GitHub does for octocat's key.
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
for a in "$@"; do case "$a" in git-receive-pack*|git-upload-pack*) cmd="$a" ;; esac; done
if [ -z "$cmd" ]; then
  echo "Hi octocat! You've successfully authenticated, but GitHub does not provide shell access." >&2; exit 1
fi
repo="$base/github/$(printf '%s' "${{cmd#* }}" | tr -d "'" | sed 's|^/||')"
[ -d "$repo" ] || repo="$repo.git"
case "$(cat "$repo/fake-ssh-mode" 2>/dev/null)" in
  publickey) echo "git@github.com: Permission denied (publickey)." >&2; exit 255 ;;
  hostkey) echo "Host key verification failed." >&2; exit 255 ;;
  slow) sleep 5; exit 255 ;;
esac
verb="${{cmd%% *}}"
exec git "${{verb#git-}}" "$repo"
"#, base = base.display()));
        // SAFETY: set once, before this test binary starts any process (every test calls fake_ssh first)
        unsafe { std::env::set_var("GIT_SSH_COMMAND", &ssh) };
        base
    })
}

fn ssh_calls() -> Vec<String> {
    std::fs::read_to_string(fake_ssh().join("ssh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// "GitHub" at git@github.com:<repo>.git (a bare repository behind the fake ssh), and a local clone of it whose
/// remote origin is https://github.com/<repo>, the way a project links to GitHub. The clone allows no https, so a
/// push that didn't go over ssh fails.
fn github(tmp: &Path, repo: &str) -> (PathBuf, PathBuf) {
    let base = fake_ssh();
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = base.join(format!("github/{repo}.git"));
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&local, &["remote", "set-url", "origin", &format!("https://github.com/{repo}")]);
    git(&local, &["config", "protocol.https.allow", "never"]);
    git(&local, &["checkout", "-q", "-b", "card"]);
    git(&local, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "the card's work"]);
    git(&local, &["checkout", "-q", "main"]);
    (bare, local)
}

#[test]
fn push_branch_goes_over_ssh_to_git_at_github_com_even_from_an_https_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = github(tmp.path(), "owner/name");
    let config = std::fs::read_to_string(local.join(".git/config")).unwrap();
    // through the remote, and to the link itself
    worktree::push_branch(&local, "origin", "card").unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]), "the branch is on GitHub");
    assert!(ssh_calls().iter().any(|c| c.contains("git@github.com") && c.contains("git-receive-pack 'owner/name'")), "{:?}", ssh_calls());
    git(&local, &["checkout", "-q", "card"]);
    git(&local, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "more"]);
    worktree::push_branch(&local, "https://github.com/owner/name", "card").unwrap();
    assert_eq!(git(&bare, &["rev-parse", "card"]), git(&local, &["rev-parse", "card"]));
    // nothing changed in the repository's config or remotes
    assert_eq!(std::fs::read_to_string(local.join(".git/config")).unwrap(), config);
    assert_eq!(git(&local, &["remote", "get-url", "origin"]), "https://github.com/owner/name");
}

#[test]
fn ssh_check_runs_your_own_ssh_command_and_reads_the_account() {
    fake_ssh();
    assert_eq!(connection::ssh_check(Duration::from_secs(10)), Ok("octocat".into()));
    assert!(ssh_calls().iter().any(|c| c == "-T git@github.com"), "{:?}", ssh_calls());
}

#[test]
fn can_push_is_a_dry_run_that_sends_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = github(tmp.path(), "acme/dry");
    worktree::can_push(&local, "origin", &PushOver::Ssh, Duration::from_secs(10)).unwrap();
    assert!(ssh_calls().iter().any(|c| c.contains("git-receive-pack 'acme/dry'")), "{:?}", ssh_calls());
    assert!(!has_branch(&bare, "gizai-connection-check") && !has_branch(&bare, "card"), "nothing was pushed");
}

#[test]
fn a_failed_ssh_push_says_what_to_check_and_pushes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = github(tmp.path(), "acme/locked");
    std::fs::write(bare.join("fake-ssh-mode"), "publickey").unwrap();
    let e = worktree::push_branch(&local, "origin", "card").unwrap_err().to_string();
    assert!(e.contains("Couldn't push card to origin: GitHub didn't accept your SSH key. Add your public key to your GitHub account \
                        (github.com/settings/keys), or load it into ssh-agent with ssh-add."), "{e}");
    std::fs::write(bare.join("fake-ssh-mode"), "hostkey").unwrap();
    let e = worktree::push_branch(&local, "origin", "card").unwrap_err().to_string();
    assert!(e.contains("This computer doesn't trust GitHub's SSH host key yet. Run ssh -T git@github.com once in a terminal and answer yes."), "{e}");
    let p = worktree::can_push(&local, "origin", &PushOver::Ssh, Duration::from_secs(10)).unwrap_err();
    assert_eq!(p.what, "This computer doesn't trust GitHub's SSH host key yet");
    assert!(!has_branch(&bare, "card"), "nothing pushed");
}

#[test]
fn no_answer_in_time_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = github(tmp.path(), "acme/slow");
    std::fs::write(bare.join("fake-ssh-mode"), "slow").unwrap();
    let started = Instant::now();
    let p = worktree::can_push(&local, "origin", &PushOver::Ssh, Duration::from_secs(2)).unwrap_err();
    assert_eq!(p.to_string(), "GitHub gave no answer within 2 seconds. Check your internet connection, then try again.");
    assert!(started.elapsed() < Duration::from_secs(4), "{:?}", started.elapsed());
}
