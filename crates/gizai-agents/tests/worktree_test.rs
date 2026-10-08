use gizai_agents::worktree;
use std::process::Command;

/// The time limit a card start gives `fetch_start`.
const MINUTE: std::time::Duration = std::time::Duration::from_secs(60);

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
    let start = worktree::fetch_start(&local, Some("acme-labs"), bare.to_str().unwrap(), "main", MINUTE).unwrap();
    assert_eq!(start, "refs/remotes/acme-labs/main");
    let wt = worktree::ensure(&local, &tmp.path().join("wt"), "SH-1", "Wireframes", &start).unwrap();
    assert_eq!(rev(&wt.path, "HEAD"), rev(&bare, "main"));
    let upstream = Command::new("git").args(["rev-parse", "--abbrev-ref", "@{upstream}"]).current_dir(&wt.path).output().unwrap();
    assert!(!upstream.status.success(), "no upstream, so a push can't land on main by accident");
    assert_eq!(worktree::behind(&wt.path, &start).unwrap(), 0);
    // GitHub moves on: the card's branch is now one commit behind
    push_new_commit(&src, &bare, "later on github");
    let start = worktree::fetch_start(&local, Some("acme-labs"), bare.to_str().unwrap(), "main", MINUTE).unwrap();
    assert_eq!(worktree::behind(&wt.path, &start).unwrap(), 1);
}

#[test]
fn without_a_matching_remote_the_link_is_fetched_into_a_hidden_ref() {
    let tmp = tempfile::tempdir().unwrap();
    let (_src, bare, _local) = github_and_stale_clone(tmp.path());
    let plain = repo(&tmp.path().join("p").tap_mkdir());
    let start = worktree::fetch_start(&plain, None, bare.to_str().unwrap(), "main", MINUTE).unwrap();
    assert_eq!(start, "refs/gizai/base/main");
    assert_eq!(rev(&plain, &start), rev(&bare, "main"));
    let branches = String::from_utf8(Command::new("git").args(["branch", "-a"]).current_dir(&plain).output().unwrap().stdout).unwrap();
    assert!(!branches.contains("gizai/base"), "not in your branch list: {branches}");
}

#[test]
fn an_unreachable_repository_is_a_clear_error() {
    let tmp = tempfile::tempdir().unwrap();
    let local = repo(tmp.path());
    let e = worktree::fetch_start(&local, None, "/nonexistent/github.git", "main", MINUTE).unwrap_err().to_string();
    assert!(e.contains("Couldn't fetch main from /nonexistent/github.git"), "{e}");
}

trait TapMkdir { fn tap_mkdir(self) -> Self; }
impl TapMkdir for std::path::PathBuf { fn tap_mkdir(self) -> Self { std::fs::create_dir_all(&self).unwrap(); self } }

// ---- Review on GitHub: pushing a card's branch, and the clean-up after its merge ----

fn commit_file(dir: &std::path::Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", name]);
}

fn has_branch(repo: &std::path::Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

#[test]
fn pushing_a_cards_branch_puts_its_commits_on_github_and_never_forces() {
    let tmp = tempfile::tempdir().unwrap();
    let (_src, bare, local) = github_and_stale_clone(tmp.path());
    let wt = worktree::ensure(&local, &tmp.path().join("wt"), "SH-1", "Export", "main").unwrap();
    commit_file(&wt.path, "export.csv", "a,b\n");
    worktree::push_branch(&local, "acme-labs", &wt.branch).unwrap();
    assert_eq!(rev(&bare, &wt.branch), rev(&wt.path, "HEAD"), "through the remote's name");
    commit_file(&wt.path, "more.csv", "c\n");
    worktree::push_branch(&local, bare.to_str().unwrap(), &wt.branch).unwrap();
    assert_eq!(rev(&bare, &wt.branch), rev(&wt.path, "HEAD"), "or to the link itself");
    // rewritten history is refused, not forced over GitHub's copy
    let on_github = rev(&bare, &wt.branch);
    git(&wt.path, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--amend", "-m", "rewritten"]);
    let e = worktree::push_branch(&local, bare.to_str().unwrap(), &wt.branch).unwrap_err().to_string();
    assert!(e.contains(&format!("Couldn't push {} to {}", wt.branch, bare.display())), "{e}");
    assert_eq!(rev(&bare, &wt.branch), on_github);
    // nowhere to push to
    let e = worktree::push_branch(&local, "/nonexistent/github.git", &wt.branch).unwrap_err().to_string();
    assert!(e.contains("Couldn't push"), "{e}");
}

#[test]
fn a_clean_card_worktree_is_removed_and_its_branch_deleted_when_asked() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let base = tmp.path().join("worktrees");
    let a = worktree::ensure(&repo, &base.join("SH"), "SH-1", "Export", "main").unwrap();
    commit_file(&a.path, "a.txt", "a");
    let r = worktree::remove_card_worktree(&repo, &base, &a.branch, true);
    assert_eq!((r.removed.as_deref(), r.kept.is_none(), r.branch_deleted, r.branch_kept.is_none()), (Some(a.path.as_path()), true, true, true), "{r:?}");
    assert!(!a.path.exists() && !has_branch(&repo, &a.branch));
    assert_eq!(r.phrases(&a.branch), ["removed its worktree".to_string(), format!("deleted branch {}", a.branch)]);
    // without delete_branch the branch stays, and nothing says it was refused
    let b = worktree::ensure(&repo, &base.join("SH"), "SH-2", "Import", "main").unwrap();
    let r = worktree::remove_card_worktree(&repo, &base, &b.branch, false);
    assert!(r.removed.is_some() && !r.branch_deleted && r.branch_kept.is_none(), "{r:?}");
    assert!(!b.path.exists() && has_branch(&repo, &b.branch));
    assert_eq!(r.phrases(&b.branch), ["removed its worktree"]);
    // a branch no worktree has checked out any more is just deleted
    let r = worktree::remove_card_worktree(&repo, &base, &b.branch, true);
    assert!(r.removed.is_none() && r.branch_deleted, "{r:?}");
    assert!(!has_branch(&repo, &b.branch));
}

#[test]
fn a_card_worktree_with_uncommitted_work_is_kept_with_its_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let base = tmp.path().join("worktrees");
    let a = worktree::ensure(&repo, &base, "SH-1", "Export", "main").unwrap();
    assert_eq!(worktree::uncommitted(&a.path).unwrap(), 0);
    std::fs::write(a.path.join("notes.txt"), "not committed").unwrap();
    assert_eq!(worktree::uncommitted(&a.path).unwrap(), 1, "an untracked file counts");
    let r = worktree::remove_card_worktree(&repo, &base, &a.branch, true);
    assert_eq!(r.kept, Some((a.path.clone(), "it has uncommitted changes".to_string())), "{r:?}");
    assert!(r.removed.is_none() && !r.branch_deleted);
    assert!(a.path.join("notes.txt").exists() && has_branch(&repo, &a.branch), "nothing lost");
    assert_eq!(r.phrases(&a.branch), [format!("kept its worktree {} (it has uncommitted changes)", a.path.display())]);
}

#[test]
fn only_gizais_own_worktrees_are_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let base = tmp.path().join("worktrees");
    std::fs::create_dir_all(&base).unwrap();
    // your own checkout of the branch, next to the repository
    let mine = tmp.path().join("mine");
    git(&repo, &["worktree", "add", "-q", "-b", "gizai/sh-1-export", mine.to_str().unwrap()]);
    let r = worktree::remove_card_worktree(&repo, &base, "gizai/sh-1-export", true);
    assert_eq!(r.kept, Some((mine.clone(), "it isn't one of Gizai's worktrees".to_string())), "{r:?}");
    assert!(mine.exists() && has_branch(&repo, "gizai/sh-1-export") && !r.branch_deleted);
    // the main checkout itself on the branch
    let r = worktree::remove_card_worktree(&repo, &base, "main", true);
    assert!(r.kept.is_some() && r.removed.is_none() && !r.branch_deleted, "{r:?}");
    assert!(repo.join(".git").exists() && has_branch(&repo, "main"));
    // the worktrees folder itself is not "inside" it
    let r = worktree::remove_card_worktree(&repo, &repo, "main", false);
    assert!(r.kept.is_some() && r.removed.is_none(), "{r:?}");
}

#[test]
fn a_card_worktree_whose_folder_is_gone_only_leaves_gits_list() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let base = tmp.path().join("worktrees");
    let a = worktree::ensure(&repo, &base.join("SH"), "SH-1", "Export", "main").unwrap();
    std::fs::remove_dir_all(&a.path).unwrap();
    let r = worktree::remove_card_worktree(&repo, &base, &a.branch, true);
    assert!(r.removed.is_some() && r.branch_deleted, "{r:?}");
    assert_eq!(worktree::worktree_of(&repo, &a.branch).unwrap(), None);
    assert!(!has_branch(&repo, &a.branch));
    // a branch that doesn't exist says why it stayed
    let r = worktree::remove_card_worktree(&repo, &base, "gizai/nope", true);
    assert!(!r.branch_deleted && r.branch_kept.is_some(), "{r:?}");
}

// ---- Taking over a finished card's worktree (GA-30) ----

fn worktree_list(repo: &std::path::Path) -> String {
    String::from_utf8(Command::new("git").args(["worktree", "list", "--porcelain"]).current_dir(repo).output().unwrap().stdout).unwrap()
}

/// A repository that ignores the folders a warm worktree keeps, with a finished card's worktree that has a commit
/// of its own, build output (target/, node_modules/, .env) and an ignored file of the card that isn't on the keep list.
fn finished_card_worktree(tmp: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf, worktree::Worktree) {
    let repo = repo(tmp);
    commit_file(&repo, ".gitignore", ".env\ntarget/\nnode_modules/\n*.log\n");
    let base = tmp.join("wt");
    let a = worktree::ensure(&repo, &base, "KADE-1", "Old card", "main").unwrap();
    commit_file(&a.path, "old-card.txt", "done\n");
    std::fs::create_dir_all(a.path.join("target/debug")).unwrap();
    std::fs::write(a.path.join("target/debug/warm.rlib"), "compiled").unwrap();
    std::fs::create_dir_all(a.path.join("node_modules/left-pad")).unwrap();
    std::fs::write(a.path.join(".env"), "APP_KEY=1\n").unwrap();
    std::fs::write(a.path.join("debug.log"), "the old card's log\n").unwrap();
    (repo, base, a)
}

#[test]
fn a_new_card_takes_over_a_finished_cards_worktree_on_its_own_clean_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, base, a) = finished_card_worktree(tmp.path());
    let old_head = rev(&a.path, "HEAD");
    let keep = [".env".to_string(), "target/".into(), "node_modules/".into()];
    let b = worktree::reuse(&repo, &a.path, &base, "KADE-2", "Next card", "main", &keep).unwrap();

    assert_eq!(b.path, base.join("KADE-2"));
    assert_eq!(b.branch, "gizai/kade-2-next-card");
    assert!(b.created, "a taken-over worktree counts as new for the card: it is prepared");
    assert_eq!(b.reused, Some(worktree::Reused { from: a.path.clone(), head: old_head }));
    assert!(!a.path.exists(), "the finished card's folder moved");
    let list = worktree_list(&repo);
    assert!(list.contains(&format!("worktree {}", b.path.display())) && !list.contains(&format!("worktree {}\n", a.path.display())), "{list}");
    // its own branch, from main: nothing of the finished card is left
    assert_eq!(rev(&b.path, "HEAD"), rev(&repo, "main"));
    assert_eq!(String::from_utf8(Command::new("git").args(["branch", "--show-current"]).current_dir(&b.path).output().unwrap().stdout).unwrap().trim(), b.branch);
    assert!(!b.path.join("old-card.txt").exists(), "the finished card's commit isn't on the new branch");
    assert!(!b.path.join("debug.log").exists(), "ignored files that aren't kept go");
    assert_eq!(worktree::uncommitted(&b.path).unwrap(), 0);
    // the warm folders stay
    assert_eq!(std::fs::read_to_string(b.path.join("target/debug/warm.rlib")).unwrap(), "compiled");
    assert!(b.path.join("node_modules/left-pad").is_dir());
    assert_eq!(std::fs::read_to_string(b.path.join(".env")).unwrap(), "APP_KEY=1\n");
    // the finished card's branch, with its work, is still there
    assert!(has_branch(&repo, &a.branch));
}

#[test]
fn a_card_that_already_has_a_branch_takes_over_on_that_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, base, a) = finished_card_worktree(tmp.path());
    git(&repo, &["branch", "gizai/kade-2-next-card", "main"]);
    commit_file(&repo, "main-only.txt", "x\n");
    let b = worktree::reuse(&repo, &a.path, &base, "KADE-2", "Next card", "main", &[]).unwrap();
    assert_eq!(rev(&b.path, "HEAD"), rev(&repo, "gizai/kade-2-next-card"), "its own branch as it was, not main");
    assert!(!b.path.join("target").exists(), "nothing kept when the keep list is empty");
}

#[test]
fn a_worktree_with_uncommitted_work_is_never_taken_over() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, base, a) = finished_card_worktree(tmp.path());
    std::fs::write(a.path.join("draft.md"), "unsaved\n").unwrap();
    let r = worktree::reuse(&repo, &a.path, &base, "KADE-2", "Next card", "main", &[]);
    assert!(matches!(&r, Err(gizai_agents::AgentError::Git(m)) if m.contains("uncommitted")), "{r:?}");
    assert!(a.path.join("draft.md").is_file() && !base.join("KADE-2").exists(), "left as it was");
    // nor a folder that isn't a worktree of this repository
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert!(worktree::reuse(&repo, &plain, &base, "KADE-3", "Other", "main", &[]).is_err());
}

#[test]
fn a_failed_switch_puts_the_worktree_back() {
    let tmp = tempfile::tempdir().unwrap();
    let (repo, base, a) = finished_card_worktree(tmp.path());
    let r = worktree::reuse(&repo, &a.path, &base, "KADE-2", "Next card", "no-such-branch", &[]);
    assert!(r.is_err(), "{r:?}");
    assert!(a.path.join("old-card.txt").is_file() && !base.join("KADE-2").exists());
    assert!(worktree_list(&repo).contains(&format!("worktree {}", a.path.display())));
}

#[test]
fn the_still_to_prepare_note_lives_out_of_git_status_sight() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let a = worktree::ensure(&repo, &tmp.path().join("wt"), "KADE-1", "Card", "main").unwrap();
    assert_eq!(worktree::unprepared(&a.path), None);
    worktree::mark_unprepared(&a.path, Some("abc123")).unwrap();
    assert_eq!(worktree::unprepared(&a.path), Some(worktree::Unprepared { since: Some("abc123".into()) }));
    assert_eq!(worktree::uncommitted(&a.path).unwrap(), 0);
    worktree::mark_unprepared(&a.path, None).unwrap();
    assert_eq!(worktree::unprepared(&a.path), Some(worktree::Unprepared { since: None }));
    worktree::mark_prepared(&a.path).unwrap();
    assert_eq!(worktree::unprepared(&a.path), None);
    worktree::mark_prepared(&a.path).unwrap();
}

// GA-14: the commits a run made, from the commit it started at to the one it ended at.

fn commit(dir: &std::path::Path, subject: &str) {
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", subject]);
}

#[test]
fn the_commits_between_two_ids_are_listed_oldest_first_with_their_subjects() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = worktree::ensure(&repo, &tmp.path().join("wt"), "KADE-14", "Record where a run ended", "main").unwrap();
    let base = rev(&wt.path, "HEAD");
    commit(&wt.path, "First change");
    commit(&wt.path, "Second change\n\nWith a body that is not its subject.");
    let head = rev(&wt.path, "HEAD");
    let list = worktree::commits(&wt.path, &base, &head).unwrap();
    assert_eq!(list.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["First change", "Second change"]);
    assert_eq!(list[1].sha, head, "full commit ids");
    assert_eq!(list[0].sha, rev(&wt.path, "HEAD~1"));
    assert_eq!(worktree::commits(&repo, &base, &head).unwrap(), list, "the main checkout shares the worktree's commits");
    assert!(worktree::commits(&wt.path, &head, &head).unwrap().is_empty(), "a run that didn't commit");
}

#[test]
fn merging_main_in_counts_as_one_commit_not_all_of_mains() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let wt = worktree::ensure(&repo, &tmp.path().join("wt"), "KADE-15", "Merge main", "main").unwrap();
    let base = rev(&wt.path, "HEAD");
    commit(&wt.path, "Own change");
    commit(&repo, "On main 1");
    commit(&repo, "On main 2");
    git(&wt.path, &["-c", "user.email=t@t", "-c", "user.name=t", "merge", "-q", "--no-ff", "-m", "Merge main", "main"]);
    let list = worktree::commits(&wt.path, &base, &rev(&wt.path, "HEAD")).unwrap();
    assert_eq!(list.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["Own change", "Merge main"]);
}

#[test]
fn only_commit_ids_are_read_never_options_or_names() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let head = rev(&repo, "HEAD");
    for bad in ["", "HEAD", "main", "--all", "-p", "abc def", "../x"] {
        assert!(matches!(worktree::commits(&repo, bad, &head), Err(gizai_agents::AgentError::Git(_))), "base {bad:?}");
        assert!(matches!(worktree::commits(&repo, &head, bad), Err(gizai_agents::AgentError::Git(_))), "head {bad:?}");
    }
    assert!(worktree::commits(&repo, &head, "0123456789abcdef0123456789abcdef01234567").is_err(), "an unknown commit is an error");
}
