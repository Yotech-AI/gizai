use gizai_agents::worktree;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success()); }

fn repo(tmp: &std::path::Path) -> std::path::PathBuf {
    let repo = tmp.join("repo"); std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    repo
}

#[test]
fn creates_then_reuses_a_worktree_and_rejects_non_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let base = tmp.path().join("wt");
    let a = worktree::ensure(&repo, &base, "KADE-41", "Export invoices as CSV!", "main").unwrap();
    assert!(a.created && a.path.join(".git").exists());
    assert_eq!(a.branch, "gizai/kade-41-export-invoices-as-csv");
    let b = worktree::ensure(&repo, &base, "KADE-41", "Export invoices as CSV!", "main").unwrap();
    assert!(!b.created);
    assert_eq!(b.branch, a.branch);
    let not = tmp.path().join("plain"); std::fs::create_dir(&not).unwrap();
    assert!(matches!(worktree::ensure(&not, &base, "X-1", "t", "main"), Err(gizai_agents::AgentError::NotGitRepo(_))));
}

#[test]
fn reuses_an_existing_branch_after_the_worktree_was_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let base = tmp.path().join("wt");
    let a = worktree::ensure(&repo, &base, "GFW-7", "Mollie webhook", "main").unwrap();
    git(&repo, &["worktree", "remove", "--force", a.path.to_str().unwrap()]);
    let b = worktree::ensure(&repo, &base, "GFW-7", "Mollie webhook", "main").unwrap();
    assert!(b.created);
    assert_eq!(b.branch, "gizai/gfw-7-mollie-webhook");
}

#[test]
fn slugs_are_short_ascii_and_tidy() {
    assert_eq!(worktree::slug("  Café über   Straße -- naïve!! ", 40), "cafe-uber-strasse-naive");
    assert_eq!(worktree::slug("A very long title that goes on and on and on forever", 20), "a-very-long-title-th");
    assert_eq!(worktree::slug("!!!", 40), "");
}

#[test]
fn a_bad_default_branch_is_a_git_error() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let r = worktree::ensure(&repo, &tmp.path().join("wt"), "X-2", "t", "develop");
    assert!(matches!(r, Err(gizai_agents::AgentError::Git(_))), "{r:?}");
}

fn rev(dir: &std::path::Path, r: &str) -> String {
    String::from_utf8(Command::new("git").args(["rev-parse", r]).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

/// "GitHub" (a bare repository), and a local clone that has fallen behind it, its remote named like Jeffrey's.
fn github_and_stale_clone(tmp: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let src = repo(tmp);
    let bare = tmp.join("github.git");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", "-o", "acme-labs", bare.to_str().unwrap(), local.to_str().unwrap()]);
    push_new_commit(&src, &bare, "on github");
    (src, bare, local)
}

fn push_new_commit(src: &std::path::Path, bare: &std::path::Path, msg: &str) {
    git(src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", msg]);
    git(src, &["push", "-q", bare.to_str().unwrap(), "main"]);
}

#[test]
fn a_new_card_starts_from_main_fetched_from_github_not_the_stale_local_main() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, bare, local) = github_and_stale_clone(tmp.path());
    assert_ne!(rev(&local, "main"), rev(&bare, "main"), "the local main is behind");
    assert_eq!(worktree::remotes(&local).unwrap(), vec![("acme-labs".to_string(), bare.to_string_lossy().to_string())]);
    let start = worktree::fetch_start(&local, Some("acme-labs"), bare.to_str().unwrap(), "main").unwrap();
    assert_eq!(start, "refs/remotes/acme-labs/main");
    let wt = worktree::ensure(&local, &tmp.path().join("wt"), "SH-1", "Wireframes", &start).unwrap();
    assert_eq!(rev(&wt.path, "HEAD"), rev(&bare, "main"));
    let upstream = Command::new("git").args(["rev-parse", "--abbrev-ref", "@{upstream}"]).current_dir(&wt.path).output().unwrap();
    assert!(!upstream.status.success(), "no upstream, so a push can't land on main by accident");
    assert_eq!(worktree::behind(&wt.path, &start).unwrap(), 0);
    // GitHub moves on: the card's branch is now one commit behind
    push_new_commit(&src, &bare, "later on github");
    let start = worktree::fetch_start(&local, Some("acme-labs"), bare.to_str().unwrap(), "main").unwrap();
    assert_eq!(worktree::behind(&wt.path, &start).unwrap(), 1);
}

#[test]
fn without_a_matching_remote_the_link_is_fetched_into_a_hidden_ref() {
    let tmp = tempfile::tempdir().unwrap();
    let (_src, bare, _local) = github_and_stale_clone(tmp.path());
    let plain = repo(&tmp.path().join("p").tap_mkdir());
    let start = worktree::fetch_start(&plain, None, bare.to_str().unwrap(), "main").unwrap();
    assert_eq!(start, "refs/gizai/base/main");
    assert_eq!(rev(&plain, &start), rev(&bare, "main"));
    let branches = String::from_utf8(Command::new("git").args(["branch", "-a"]).current_dir(&plain).output().unwrap().stdout).unwrap();
    assert!(!branches.contains("gizai/base"), "not in your branch list: {branches}");
}

#[test]
fn an_unreachable_repository_is_a_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let local = repo(tmp.path());
    let e = worktree::fetch_start(&local, None, "/nonexistent/github.git", "main").unwrap_err().to_string();
    assert!(e.contains("Couldn't fetch main from /nonexistent/github.git"), "{e}");
}

trait TapMkdir { fn tap_mkdir(self) -> Self; }
impl TapMkdir for std::path::PathBuf { fn tap_mkdir(self) -> Self { std::fs::create_dir_all(&self).unwrap(); self } }
