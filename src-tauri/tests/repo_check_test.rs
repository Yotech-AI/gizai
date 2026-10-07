use gizai_lib::git::repo_check;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success());
}

#[test]
fn reports_git_repos_branches_and_dirty_state() {
    let tmp = tempfile::tempdir().unwrap();
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert!(!repo_check(&plain).is_git);
    assert!(!repo_check(&tmp.path().join("missing")).is_git);

    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let clean = repo_check(&repo);
    assert!(clean.is_git && !clean.dirty);
    assert_eq!(clean.branch.as_deref(), Some("main"));
    std::fs::write(repo.join("new.txt"), "x").unwrap();
    assert!(repo_check(&repo).dirty);
}

#[test]
fn the_github_remote_is_offered_for_the_project_link() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("r");
    std::fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(&repo).status().unwrap().success());
    git(&["init", "-q", "-b", "main"]);
    assert_eq!(repo_check(&repo).github, None);
    git(&["remote", "add", "gitlab", "git@gitlab.com:team/app.git"]);
    git(&["remote", "add", "acme-labs", "git@github.com:acme-labs/shop-app.git"]);
    assert_eq!(repo_check(&repo).github.as_deref(), Some("https://github.com/acme-labs/shop-app"));
    git(&["remote", "add", "origin", "https://github.com/someone/shop-fork.git"]);
    assert_eq!(repo_check(&repo).github.as_deref(), Some("https://github.com/someone/shop-fork"), "origin first");
}
