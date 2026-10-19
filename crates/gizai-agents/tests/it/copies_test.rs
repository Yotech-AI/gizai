//! GA-44: the Team Lead's read-only copies of a project's code (`<data dir>/code/<KEY>`), detached worktrees of the
//! project's repository.
use gizai_agents::copies;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, file: &str, text: &str) -> String {
    std::fs::write(dir.join(file), text).unwrap();
    git(dir, &["add", file]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", file]);
    git_out(dir, &["rev-parse", "HEAD"])
}

/// A main checkout on a feature branch, with an ignored .env and an uncommitted change.
fn repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), ".env\nvendor/\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    commit(&repo, "README.md", "one\n");
    std::fs::write(repo.join(".env"), "APP_KEY=secret\n").unwrap();
    std::fs::create_dir(repo.join("vendor")).unwrap();
    git(&repo, &["switch", "-q", "-c", "feature/x"]);
    std::fs::write(repo.join("README.md"), "work in progress\n").unwrap();
    repo
}

fn mtimes(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.file_name() != ".git" {
            out.push((e.path(), e.metadata().unwrap().modified().unwrap()));
        }
    }
    out.sort();
    out
}

#[test]
fn a_copy_is_a_detached_worktree_at_main_with_tracked_files_only_and_no_branch() {
    let tmp = real_tempdir();
    let repo = repo(tmp.path());
    let branches = git_out(&repo, &["branch", "--list"]);
    let dir = tmp.path().join("data/code/KADE");
    let (at, changed) = copies::sync(&repo, &dir, "main").unwrap();
    assert!(changed);
    assert_eq!(at.sha, git_out(&repo, &["rev-parse", "main"]));
    assert_eq!(at.short().len(), 7);
    assert_eq!(at.date.len(), 10, "{}", at.date);
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), at.sha);
    assert_eq!(git_out(&dir, &["branch", "--show-current"]), "", "no branch checked out");
    assert_eq!(git_out(&repo, &["branch", "--list"]), branches, "no branch made");
    let listed = git_out(&repo, &["worktree", "list", "--porcelain"]);
    assert!(listed.contains(&format!("worktree {}\nHEAD {}\ndetached", git_path(&dir), at.sha)), "{listed}");
    assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "one\n", "main's file, not the uncommitted change");
    assert!(!dir.join(".env").exists() && !dir.join("vendor").exists(), "only tracked files");
    assert!(copies::is_checkout_of(&dir, &repo));
    assert_eq!(copies::at(&dir), Some(at));
    // the main checkout is as it was
    assert_eq!(git_out(&repo, &["branch", "--show-current"]), "feature/x");
    assert_eq!(std::fs::read_to_string(repo.join("README.md")).unwrap(), "work in progress\n");
}

#[test]
fn a_copy_moves_when_main_moved_and_is_left_alone_when_it_didnt() {
    let tmp = real_tempdir();
    let repo = repo(tmp.path());
    let dir = tmp.path().join("code/KADE");
    let (first, _) = copies::sync(&repo, &dir, "main").unwrap();
    // nothing moved: not a file touched, and what is in it stays
    std::fs::write(dir.join("scratch.txt"), "left here\n").unwrap();
    let before = mtimes(&dir);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let (again, changed) = copies::sync(&repo, &dir, "main").unwrap();
    assert!(!changed);
    assert_eq!(again, first);
    assert_eq!(mtimes(&dir), before, "no file in the copy touched");
    // main moves (in a worktree of its own, so the feature branch stays): the copy follows, and what was changed in it goes
    let other = tmp.path().join("on-main");
    git(&repo, &["worktree", "add", "-q", other.to_str().unwrap(), "main"]);
    let later = commit(&other, "NEW.md", "new\n");
    std::fs::write(dir.join("README.md"), "changed in the copy\n").unwrap();
    let (moved, changed) = copies::sync(&repo, &dir, "main").unwrap();
    assert!(changed);
    assert_eq!(moved.sha, later);
    assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "one\n");
    assert!(dir.join("NEW.md").is_file());
    assert!(!dir.join("scratch.txt").exists(), "thrown away: it is Gizai's own copy");
    assert_eq!(git_out(&repo, &["branch", "--show-current"]), "feature/x");
}

#[test]
fn a_copy_whose_folder_was_deleted_by_hand_is_made_again() {
    let tmp = real_tempdir();
    let repo = repo(tmp.path());
    let dir = tmp.path().join("code/KADE");
    copies::sync(&repo, &dir, "main").unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    let (_, changed) = copies::sync(&repo, &dir, "main").unwrap();
    assert!(changed && dir.join("README.md").is_file());
}

#[test]
fn a_folder_that_isnt_a_copy_of_the_repository_is_not_synced_over() {
    let tmp = real_tempdir();
    let repo = repo(tmp.path());
    let other_tmp = tmp.path().join("other");
    std::fs::create_dir(&other_tmp).unwrap();
    let other = self::repo(&other_tmp);
    let dir = tmp.path().join("code/KADE");
    copies::sync(&other, &dir, "main").unwrap();
    let e = copies::sync(&repo, &dir, "main").unwrap_err().to_string();
    assert!(e.contains("is not a copy of"), "{e}");
    assert!(!copies::is_checkout_of(&dir, &repo));
    assert!(!copies::is_checkout_of(&repo.join("README.md"), &repo), "a file inside isn't a checkout");
    let e = copies::sync(&repo, &tmp.path().join("code/X"), "no-such-branch").unwrap_err().to_string();
    assert!(e.contains("no-such-branch isn't in"), "{e}");
}

#[test]
fn remove_takes_the_copy_out_of_gits_list_and_never_touches_anything_outside_the_folder_of_copies() {
    let tmp = real_tempdir();
    let repo = repo(tmp.path());
    let root = tmp.path().join("code");
    let dir = root.join("KADE");
    copies::sync(&repo, &dir, "main").unwrap();
    // only a folder directly inside the folder of copies
    for outside in [repo.clone(), tmp.path().join("code"), dir.join("README.md"), tmp.path().to_path_buf()] {
        assert!(copies::remove(&outside, &root).is_err(), "{}", outside.display());
    }
    assert!(repo.join(".env").is_file() && dir.is_dir());
    copies::remove(&dir, &root).unwrap();
    assert!(!dir.exists());
    assert!(!git_out(&repo, &["worktree", "list", "--porcelain"]).contains(dir.to_str().unwrap()), "pruned");
    assert_eq!(git_out(&repo, &["branch", "--show-current"]), "feature/x");
    assert_eq!(std::fs::read_to_string(repo.join("README.md")).unwrap(), "work in progress\n");
    copies::remove(&dir, &root).unwrap(); // gone already: fine
    // a link in the folder of copies loses only the link, never what it points to (a Unix symlink: one on Windows
    // needs Developer Mode or an administrator)
    #[cfg(unix)]
    {
        let keep = tmp.path().join("keep");
        std::fs::create_dir(&keep).unwrap();
        std::fs::write(keep.join("important.txt"), "mine\n").unwrap();
        std::os::unix::fs::symlink(&keep, root.join("LINK")).unwrap();
        copies::remove(&root.join("LINK"), &root).unwrap();
        assert!(!root.join("LINK").exists() && keep.join("important.txt").is_file());
    }
}

/// A temp folder by its real path, the way git and Gizai report it: on macOS /var/folders is /private/var/folders, and on
/// Windows TEMP can be a short name (RUNNER~1) that git gives in full. Without the \\?\ that canonicalize puts before a
/// Windows drive.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let base = std::path::PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    tempfile::tempdir_in(base).unwrap()
}

/// A path as `git worktree list` prints it: with / between folders, also on Windows.
fn git_path(p: &std::path::Path) -> String {
    p.display().to_string().replace('\\', "/")
}
