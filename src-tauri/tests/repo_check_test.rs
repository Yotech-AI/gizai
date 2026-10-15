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

/// GA-203: your own insteadOf rule (in the repository's own config, never the global one) sends origin to an SSH host
/// alias of yours, so `git remote -v` shows the alias: the GitHub link still comes from origin's URL as written.
#[test]
fn a_github_remote_your_own_insteadof_rule_sends_to_a_host_alias_is_still_offered_origin_first() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("r");
    std::fs::create_dir(&repo).unwrap();
    let out = |args: &[&str]| {
        let o = Command::new("git").args(args).current_dir(&repo).output().unwrap();
        assert!(o.status.success(), "git {args:?}");
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    };
    out(&["init", "-q", "-b", "main"]);
    out(&["remote", "add", "origin", "git@github.com:acme/shop.git"]);
    out(&["config", "url.git@gh-work:acme/.insteadOf", "git@github.com:acme/"]);
    assert_eq!(out(&["remote", "get-url", "origin"]), "git@gh-work:acme/shop.git");
    let config = std::fs::read_to_string(repo.join(".git/config")).unwrap();
    let r = repo_check(&repo);
    assert_eq!((r.github.as_deref(), r.bitbucket.as_deref()), (Some("https://github.com/acme/shop"), None));
    assert_eq!(std::fs::read_to_string(repo.join(".git/config")).unwrap(), config, "no git config changed");

    // origin first: also before an upstream that no rule rewrites
    out(&["remote", "add", "upstream", "https://github.com/other/shop.git"]);
    assert_eq!(repo_check(&repo).github.as_deref(), Some("https://github.com/acme/shop"), "origin first");
    // a fork as origin, which isn't rewritten, wins over upstream
    out(&["remote", "set-url", "origin", "git@github.com:jefsev/shop.git"]);
    assert_eq!(repo_check(&repo).github.as_deref(), Some("https://github.com/jefsev/shop"));
}

/// A partial clone: `git remote -v` prints its filter after "(fetch)", and the remote is still offered.
#[test]
fn a_partial_clones_remote_is_offered() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("r");
    std::fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| assert!(Command::new("git").args(args).current_dir(&repo).status().unwrap().success());
    git(&["init", "-q", "-b", "main"]);
    git(&["remote", "add", "origin", "git@github.com:acme/shop.git"]);
    git(&["config", "remote.origin.promisor", "true"]);
    git(&["config", "remote.origin.partialclonefilter", "blob:none"]);
    let shown = String::from_utf8(Command::new("git").args(["remote", "-v"]).current_dir(&repo).output().unwrap().stdout).unwrap();
    assert!(shown.starts_with("origin\tgit@github.com:acme/shop.git (fetch) [blob:none]"), "{shown}");
    assert_eq!(repo_check(&repo).github.as_deref(), Some("https://github.com/acme/shop"));
}
