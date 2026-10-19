//! GA-56: `worktree::push_new_commits`, Gizai's push after a run. "The remote" is a local bare repository, so nothing
//! here reaches GitHub; a pre-receive hook in it writes down every push that reaches it.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gizai_agents::connection::PushOver;
use gizai_agents::worktree::{self, Pushed};

const BRANCH: &str = "gizai/kade-1-export";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, subject: &str) -> String {
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", subject]);
    git(dir, &["rev-parse", "HEAD"])
}

fn write_script(path: &Path, script: &str) {
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

struct Repos { _tmp: tempfile::TempDir, bare: PathBuf, repo: PathBuf, pushes: PathBuf }

impl Repos {
    /// The remote's address, as `push_new_commits` gets it (a URL; here a path).
    fn to(&self) -> String { self.bare.to_string_lossy().to_string() }
    fn remote_tip(&self) -> Option<String> {
        let o = Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{BRANCH}")]).current_dir(&self.bare).output().unwrap();
        o.status.success().then(|| String::from_utf8(o.stdout).unwrap().trim().to_string())
    }
    fn pushes(&self) -> usize { std::fs::read_to_string(&self.pushes).unwrap_or_default().lines().count() }
    fn push(&self) -> Result<Pushed, String> {
        worktree::push_new_commits(&self.repo, &self.to(), BRANCH, "refs/remotes/origin/main", &PushOver::Ssh).map_err(|e| e.to_string())
    }
}

/// A remote with main (one commit), and a clone of it on the card's branch, which starts at main as fetched.
fn repos() -> Repos {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    commit(&src, "init");
    let bare = tmp.path().join("remote.git");
    git(tmp.path(), &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    git(&bare, &["config", "core.hooksPath", bare.join("hooks").to_str().unwrap()]);
    let pushes = tmp.path().join("pushes");
    write_script(&bare.join("hooks/pre-receive"), &format!("#!/bin/sh\ncat >> '{}'\n", pushes.display()));
    let repo = tmp.path().join("repo");
    git(tmp.path(), &["clone", "-q", bare.to_str().unwrap(), repo.to_str().unwrap()]);
    git(&repo, &["checkout", "-q", "-b", BRANCH, "origin/main"]);
    Repos { _tmp: tmp, bare, repo, pushes }
}

#[test]
fn a_branch_without_commits_of_its_own_goes_nowhere() {
    let r = repos();
    assert_eq!(r.push(), Ok(Pushed::Nothing));
    assert_eq!((r.remote_tip(), r.pushes()), (None, 0));
}

#[test]
fn a_new_branch_goes_with_all_its_commits_and_counts_only_its_own() {
    let r = repos();
    commit(&r.repo, "one");
    let tip = commit(&r.repo, "two");
    assert_eq!(r.push(), Ok(Pushed::Commits(2)), "main's commit isn't counted");
    assert_eq!(r.remote_tip(), Some(tip));
    assert_eq!(r.pushes(), 1);
}

#[test]
fn nothing_goes_when_the_remote_has_the_tip_and_only_what_it_lacks_is_counted() {
    let r = repos();
    commit(&r.repo, "one");
    git(&r.repo, &["push", "-q", "origin", BRANCH]); // the agent's own push
    assert_eq!(r.push(), Ok(Pushed::Nothing));
    assert_eq!(r.pushes(), 1, "only the agent's: no push when the remote has everything");

    commit(&r.repo, "two");
    let tip = commit(&r.repo, "three");
    assert_eq!(r.push(), Ok(Pushed::Commits(2)), "the commit the remote had isn't counted");
    assert_eq!((r.remote_tip(), r.pushes()), (Some(tip), 2));
}

#[test]
fn a_remote_branch_with_commits_this_repository_never_saw_is_never_forced() {
    let r = repos();
    commit(&r.repo, "mine");
    // someone else pushed to the branch meanwhile
    let other = r.bare.parent().unwrap().join("other");
    git(r.bare.parent().unwrap(), &["clone", "-q", &r.to(), other.to_str().unwrap()]);
    let theirs = commit(&other, "theirs");
    git(&other, &["push", "-q", "origin", &format!("HEAD:refs/heads/{BRANCH}")]);

    let e = r.push().unwrap_err();
    assert!(e.contains(&format!("Couldn't push {BRANCH} to {}", r.to())) && e.contains("The branch on GitHub has commits this one doesn't"), "{e}");
    assert!(e.contains("Gizai never forces a push"), "{e}");
    assert_eq!(r.remote_tip(), Some(theirs), "their commit stays");
}

#[test]
fn a_rewritten_branch_is_not_forced_over_the_remotes_copy_either() {
    let r = repos();
    let first = commit(&r.repo, "first");
    git(&r.repo, &["push", "-q", "origin", BRANCH]);
    // the branch was rewritten after its push: the remote's copy is a commit this repository knows, not an ancestor
    git(&r.repo, &["reset", "-q", "--hard", "origin/main"]);
    commit(&r.repo, "rewritten");
    let e = r.push().unwrap_err();
    assert!(e.contains("The branch on GitHub has commits this one doesn't"), "{e}");
    assert_eq!(r.remote_tip(), Some(first));
}

#[test]
fn a_branch_that_is_gone_says_so() {
    let r = repos();
    let e = worktree::push_new_commits(&r.repo, &r.to(), "gizai/no-such-branch", "refs/remotes/origin/main", &PushOver::Ssh).unwrap_err().to_string();
    assert!(e.contains("Couldn't push gizai/no-such-branch: it isn't in"), "{e}");
    assert_eq!(r.pushes(), 0);
}

#[test]
fn a_remote_that_cant_be_reached_says_why_and_pushes_nothing() {
    let r = repos();
    commit(&r.repo, "one");
    let gone = r.bare.parent().unwrap().join("no-remote-here.git");
    let e = worktree::push_new_commits(&r.repo, gone.to_str().unwrap(), BRANCH, "refs/remotes/origin/main", &PushOver::Ssh).unwrap_err().to_string();
    assert!(e.contains(&format!("Couldn't push {BRANCH} to {}", gone.display())), "{e}");
    assert_eq!(r.pushes(), 0);
}
