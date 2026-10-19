//! GA-214: the PR check asks git first. `worktree::ls_remote` (what a remote has at some refs, asked the way a push
//! goes) and `worktree::is_ancestor` (whether a merged pull request has the branch's latest commit, without asking
//! GitHub for its commits). "GitHub" is a local bare repository, so nothing here reaches GitHub.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use gizai_agents::connection::PushOver;
use gizai_agents::worktree;

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false"]).args(args)
        .current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, subject: &str) -> String {
    git(dir, &["commit", "-q", "--allow-empty", "-m", subject]);
    git(dir, &["rev-parse", "HEAD"])
}

/// "GitHub" (a bare repository with main) and a clone of it with `origin` there.
fn repos(tmp: &Path) -> (PathBuf, PathBuf) {
    let bare = tmp.join("shop.git");
    std::fs::create_dir(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let local = tmp.join("local");
    std::fs::create_dir(&local).unwrap();
    git(&local, &["init", "-q", "-b", "main"]);
    commit(&local, "init");
    git(&local, &["remote", "add", "origin", &bare.to_string_lossy()]);
    git(&local, &["push", "-q", "origin", "main"]);
    (bare, local)
}

fn refs(list: &[&str]) -> Vec<String> {
    list.iter().map(|r| r.to_string()).collect()
}

const LIMIT: Duration = Duration::from_secs(30);

#[test]
fn ls_remote_lists_main_the_branches_and_the_pull_request_refs_asked_for_in_ref_order() {
    let tmp = tempfile::tempdir().unwrap();
    let (bare, local) = repos(tmp.path());
    let main = git(&local, &["rev-parse", "HEAD"]);
    git(&local, &["checkout", "-q", "-b", "gizai/kade-1-x"]);
    let card = commit(&local, "card");
    git(&local, &["push", "-q", "origin", "gizai/kade-1-x"]);
    git(&local, &["checkout", "-q", "-b", "gizai/kade-2-y"]);
    let other = commit(&local, "other card");
    git(&local, &["push", "-q", "origin", "gizai/kade-2-y"]);
    // a pull request opened on GitHub: its ref
    git(&bare, &["update-ref", "refs/pull/9/head", &card]);
    let asked = refs(&["refs/heads/main", "refs/heads/gizai/kade-1-x", "refs/heads/gizai/kade-3-not-pushed", "refs/pull/*/head"]);
    let got = worktree::ls_remote(&local, "origin", &asked, &PushOver::Ssh, LIMIT).unwrap();
    assert_eq!(got, [
        ("refs/heads/gizai/kade-1-x".to_string(), card.clone()),
        ("refs/heads/main".to_string(), main.clone()),
        ("refs/pull/9/head".to_string(), card.clone()),
    ], "what was asked for and is there, sorted; not kade-2-y, and nothing for a branch GitHub doesn't have");
    assert!(!got.iter().any(|(_, sha)| *sha == other));
    // the same answer asked again; a remote given as an address answers the same
    assert_eq!(worktree::ls_remote(&local, "origin", &asked, &PushOver::Ssh, LIMIT).unwrap(), got);
    assert_eq!(worktree::ls_remote(&local, &bare.to_string_lossy(), &asked, &PushOver::Ssh, LIMIT).unwrap(), got);
    // a push, a merge on main and a new pull request each change it
    git(&local, &["checkout", "-q", "gizai/kade-1-x"]);
    let pushed = commit(&local, "fix");
    git(&local, &["push", "-q", "origin", "gizai/kade-1-x"]);
    let after_push = worktree::ls_remote(&local, "origin", &asked, &PushOver::Ssh, LIMIT).unwrap();
    assert_ne!(after_push, got);
    assert!(after_push.contains(&("refs/heads/gizai/kade-1-x".to_string(), pushed.clone())), "{after_push:?}");
    git(&bare, &["update-ref", "refs/heads/main", &pushed]);
    let after_merge = worktree::ls_remote(&local, "origin", &asked, &PushOver::Ssh, LIMIT).unwrap();
    assert!(after_merge.contains(&("refs/heads/main".to_string(), pushed.clone())), "{after_merge:?}");
    git(&bare, &["update-ref", "refs/pull/10/head", &other]);
    let after_open = worktree::ls_remote(&local, "origin", &asked, &PushOver::Ssh, LIMIT).unwrap();
    assert_eq!(after_open.len(), after_merge.len() + 1, "{after_open:?}");
    assert!(after_open.contains(&("refs/pull/10/head".to_string(), other.clone())), "{after_open:?}");
}

#[test]
fn ls_remote_says_plainly_when_the_remote_cant_be_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let (_bare, local) = repos(tmp.path());
    let gone = tmp.path().join("gone.git");
    git(&local, &["remote", "set-url", "origin", &gone.to_string_lossy()]);
    let e = worktree::ls_remote(&local, "origin", &refs(&["refs/heads/main"]), &PushOver::Ssh, LIMIT).unwrap_err().to_string();
    assert!(e.contains("Couldn't ask origin for its branches"), "{e}");
}

#[test]
fn is_ancestor_tells_whether_a_commit_comes_before_another_when_this_repository_has_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (_bare, local) = repos(tmp.path());
    let first = git(&local, &["rev-parse", "HEAD"]);
    let tip = commit(&local, "card");
    let later = commit(&local, "pushed on GitHub after the card's tip");
    assert_eq!(worktree::is_ancestor(&local, &tip, &later), Some(true), "the tip comes before the pull request's head");
    assert_eq!(worktree::is_ancestor(&local, &tip, &tip), Some(true), "the tip is the head");
    assert_eq!(worktree::is_ancestor(&local, &later, &first), Some(false), "a newer tip isn't in an older head");
    git(&local, &["checkout", "-q", "-b", "side", &first]);
    let side = commit(&local, "side");
    assert_eq!(worktree::is_ancestor(&local, &tip, &side), Some(false), "another line of work");
    // git can't tell: a head this repository doesn't have, or no commit id at all
    let unknown = "f".repeat(40);
    assert_eq!(worktree::is_ancestor(&local, &tip, &unknown), None);
    assert_eq!(worktree::is_ancestor(&local, &tip, ""), None);
    assert_eq!(worktree::is_ancestor(&local, "", &later), None);
    assert_eq!(worktree::is_ancestor(&local, &tip, "main; rm -rf"), None, "only commit ids reach git");
}
