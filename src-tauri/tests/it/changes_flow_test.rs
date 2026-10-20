//! GA-106: a card's Changes through the app, as contract C1's commands get them (`changes::of_card`, `file_of_card`,
//! `of_run`): a card without a branch, its worktree against its merge base with origin/main as last fetched, its branch
//! once the worktree is gone, the range the PR check saves when it sees a merge commit, a squash or a rebase merge
//! (also offline, and with the merge seen offline), a card merged before ranges were saved, the pull request's diff from
//! gh as the last resort, and a run's commits. "GitHub" is local bare repositories and gh a fake, as in pull_flow_test,
//! so nothing here reaches the real GitHub.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;

use gizai_lib::changes::{self, ChangeSource, DiffHunk, FileStatus};
use gizai_lib::pulls;
use serde_json::json;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const LINK: &str = "https://github.com/acme/shop";
const ID: &[&str] = &["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false"];

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(ID).args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

/// "GitHub" (bare repositories under tmp/github) and the project's local clone of https://github.com/acme/shop. The
/// clone's git config sends https://github.com/acme/ there and allows no transport but local files.
fn github(tmp: &Path) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    std::fs::write(src.join("readme.md"), "Shop\n").unwrap();
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-q", "-m", "init"]);
    let bare = tmp.join("github/acme/shop");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", "-c", &rewrite(tmp), "-c", "protocol.allow=never", "-c", "protocol.file.allow=always",
               "-c", "user.email=t@t", "-c", "user.name=t", LINK, local.to_str().unwrap()]);
    (bare, local)
}

fn rewrite(tmp: &Path) -> String {
    format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display())
}

/// GitHub out of reach: the local clone no longer sends https://github.com/acme/ to the bare repositories, and allows
/// no https, so a fetch fails at once without the network.
fn offline(tmp: &Path, local: &Path) {
    git(local, &["config", "--remove-section", &format!("url.{}/", tmp.join("github/acme").display())]);
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake gh: each call's arguments go to dir/gh-args; `pr list` answers with dir/list.json, `pr create` with
/// dir/create.out, `pr view` with dir/view.json, `pr diff` with dir/diff.out.
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d='{d}'
echo "$*" >> "$d/gh-args"
case "$1 $2" in
  "pr list") [ -f "$d/list.json" ] && cat "$d/list.json"; exit 0 ;;
  "pr create") cat > /dev/null; cat "$d/create.out"; exit 0 ;;
  "pr view") cat "$d/view.json"; exit 0 ;;
  "pr diff") cat "$d/diff.out"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#, d = dir.display()));
    gh
}

fn gh_calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("gh-args")).unwrap_or_default().lines().map(String::from).collect()
}

/// gh's answer to `pr list` for pull request #7 from the card's branch, with its merge commit once merged (the light
/// fields only, GA-214: no `commits`).
fn list(c: &Card, state: &str, head: &str, merge: Option<&str>) {
    let pr = json!([{"number": 7, "url": format!("{LINK}/pull/7"), "state": state, "isDraft": false, "headRefName": c.branch,
                     "headRefOid": head, "mergeCommit": merge.map(|m| json!({"oid": m}))}]);
    std::fs::write(c.gh.join("list.json"), pr.to_string()).unwrap();
}

struct Card { st: gizai_lib::AppState, task: String, bare: PathBuf, local: PathBuf, wt: PathBuf, branch: String, gh: PathBuf,
              /// main when the card started.
              start: String }

fn link_project(st: &gizai_lib::AppState, url: Option<&str>) {
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(url.unwrap_or_default().to_string()), default_branch: Some("main".into()),
        ..Default::default() }).unwrap();
}

/// Card KADE-1 of a project linked to https://github.com/acme/shop after an agent's run (its worktree and branch), with
/// two commits: invoices.csv, then report.txt and a change to readme.md. gh is the fake.
async fn worked_card(tmp: &Path) -> Card {
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = github(tmp);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    link_project(&st, Some(LINK));
    let gh = tmp.join("gh");
    gizai_core::settings::set(&st.db, "gh_bin", &fake_gh(&gh).to_string_lossy().to_string()).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let (wt, branch) = (PathBuf::from(run.worktree_path.unwrap()), run.branch.unwrap());
    let start = git(&wt, &["rev-parse", "HEAD"]);
    std::fs::write(wt.join("invoices.csv"), "id,total\n1,10\n").unwrap();
    commit(&wt, "invoices");
    std::fs::write(wt.join("report.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(wt.join("readme.md"), "Shop\nNow with invoices\n").unwrap();
    commit(&wt, "report");
    Card { st, task, bare, local, wt, branch, gh, start }
}

fn commit(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// What the card changed in its commits: (path, status, additions, deletions, uncommitted).
fn the_card() -> Vec<(String, FileStatus, Option<u64>, Option<u64>, bool)> {
    vec![("invoices.csv".into(), FileStatus::Added, Some(2), Some(0), false), ("readme.md".into(), FileStatus::Modified, Some(1), Some(0), false),
         ("report.txt".into(), FileStatus::Added, Some(3), Some(0), false)]
}

fn files_of(c: &changes::CardChanges) -> Vec<(String, FileStatus, Option<u64>, Option<u64>, bool)> {
    c.files.iter().map(|f| (f.path.clone(), f.status, f.additions, f.deletions, f.uncommitted)).collect()
}

/// Another clone of GitHub's repository, where "GitHub" merges and main moves on.
fn elsewhere(tmp: &Path, bare: &Path) -> PathBuf {
    let other = tmp.join("other");
    if !other.exists() {
        git(tmp, &["clone", "-q", bare.to_str().unwrap(), other.to_str().unwrap()]);
    }
    git(&other, &["fetch", "-q", "origin"]);
    other
}

/// main gets a commit of its own on GitHub: a new file.
fn main_moves_on(tmp: &Path, bare: &Path) -> String {
    let other = elsewhere(tmp, bare);
    git(&other, &["checkout", "-q", "main"]);
    git(&other, &["merge", "-q", "--ff-only", "origin/main"]);
    std::fs::write(other.join("main.txt"), "from main\n").unwrap();
    let id = commit(&other, "main's own");
    git(&other, &["push", "-q", "origin", "main"]);
    id
}

fn to_review(c: &Card) {
    let team = gizai_core::team::get(&c.st.db, &gizai_core::team::list(&c.st.db).unwrap()[0].id).unwrap();
    let id = |category: &str| team.states.iter().find(|s| s.category == category).unwrap().id.clone();
    // Review's next column is Done, for this test
    gizai_core::columns::set_column(&c.st.db, &c.st.you_id, &id("review"),
        gizai_core::columns::ColumnInput { next_state_id: Some(id("done")), ..Default::default() }).unwrap();
    gizai_core::tasks::move_to(&c.st.db, &c.st.you_id, &c.task, &id("review"), "").unwrap();
}

/// The card in Review with pull request #7 (its branch pushed to GitHub).
async fn in_review(c: &Card) {
    to_review(c);
    std::fs::write(c.gh.join("create.out"), format!("{LINK}/pull/7\n")).unwrap();
    pulls::open(&c.st, &c.task).await.unwrap();
}

#[derive(Debug, Clone, Copy)]
enum How {
    MergeCommit,
    Squash,
    Rebase,
}

/// GitHub merges pull request #7 `how` (with `message` for a merge commit or a squash) after main got a commit of its
/// own; returns the commit main is at after it.
fn merge_on_github(tmp: &Path, c: &Card, how: How, message: &str) -> String {
    main_moves_on(tmp, &c.bare);
    let other = elsewhere(tmp, &c.bare);
    let card = format!("origin/{}", c.branch);
    match how {
        How::MergeCommit => {
            git(&other, &["merge", "-q", "--no-ff", &card, "-m", message]);
        }
        How::Squash => {
            git(&other, &["merge", "-q", "--squash", &card]);
            git(&other, &["commit", "-q", "-m", message]);
        }
        How::Rebase => {
            git(&other, &["checkout", "-q", "-b", "rebased", &card]);
            git(&other, &["rebase", "-q", "main"]);
            git(&other, &["checkout", "-q", "main"]);
            git(&other, &["merge", "-q", "--ff-only", "rebased"]);
        }
    }
    git(&other, &["push", "-q", "origin", "main"]);
    git(&other, &["rev-parse", "HEAD"])
}

fn task(c: &Card) -> gizai_core::model::Task { gizai_core::tasks::get(&c.st.db, &c.task).unwrap() }

#[tokio::test]
async fn a_card_without_a_branch_has_no_changes_yet() {
    // a new card of a project where cards have started (so main was fetched)
    let tmp = real_tempdir();
    let c = worked_card(tmp.path()).await;
    let st = &c.st;
    let t = gizai_lib::test_task(st, c.local.to_str().unwrap(), "backend");
    let got = changes::of_card(st, &t).unwrap();
    assert_eq!(serde_json::to_value(&got).unwrap(), json!({"source": "none", "from": null, "to": null, "baseLabel": "origin/main",
        "additions": 0, "deletions": 0, "files": [], "note": "No changes yet: the first run makes this card's branch."}));
    // a project without a link compares with its own main
    link_project(st, None);
    let got = changes::of_card(st, &t).unwrap();
    assert_eq!((got.source, got.base_label.as_str()), (ChangeSource::None, "main"));
}

#[tokio::test]
async fn a_cards_worktree_shows_its_commits_and_whats_not_committed_since_origin_main_as_last_fetched() {
    let tmp = real_tempdir();
    let c = worked_card(tmp.path()).await;
    std::fs::write(c.wt.join("report.txt"), "one\nTWO\nthree\nfour\n").unwrap(); // not committed yet
    std::fs::write(c.wt.join("notes.md"), "a\nb\n").unwrap(); // not tracked yet
    std::fs::create_dir_all(c.wt.join(".gizai-tmp")).unwrap();
    std::fs::write(c.wt.join(".gizai-tmp/scratch.txt"), "scratch\n").unwrap();
    // main moves on at GitHub after the card started, and is fetched: its commit isn't the card's
    main_moves_on(tmp.path(), &c.bare);
    git(&c.local, &["fetch", "-q", "origin"]);

    let got = changes::of_card(&c.st, &c.task).unwrap();
    assert_eq!(serde_json::to_value(&got).unwrap(), json!({"source": "worktree", "from": c.start, "to": null, "baseLabel": "origin/main",
        "additions": 9, "deletions": 0, "note": null, "files": [
            {"path": "invoices.csv", "oldPath": null, "status": "added", "additions": 2, "deletions": 0, "binary": false, "uncommitted": false},
            {"path": "notes.md", "oldPath": null, "status": "added", "additions": 2, "deletions": 0, "binary": false, "uncommitted": true},
            {"path": "readme.md", "oldPath": null, "status": "modified", "additions": 1, "deletions": 0, "binary": false, "uncommitted": false},
            {"path": "report.txt", "oldPath": null, "status": "added", "additions": 4, "deletions": 0, "binary": false, "uncommitted": true},
        ]}));
    // one file's texts and git's hunks: as on disk, and as committed
    let from = got.from.clone().unwrap();
    let report = changes::file_of_card(&c.st, &c.task, "report.txt", None, &from, None).unwrap();
    assert_eq!((report.old_text, report.new_text.as_deref(), report.hunks.clone()),
               (None, Some("one\nTWO\nthree\nfour\n"), vec![DiffHunk { old_start: 0, old_lines: 0, new_start: 1, new_lines: 4 }]));
    let tip = git(&c.wt, &["rev-parse", "HEAD"]);
    let readme = changes::file_of_card(&c.st, &c.task, "readme.md", None, &from, Some(&tip)).unwrap();
    assert_eq!((readme.old_text.as_deref(), readme.new_text.as_deref(), readme.hunks),
               (Some("Shop\n"), Some("Shop\nNow with invoices\n"), vec![DiffHunk { old_start: 1, old_lines: 0, new_start: 2, new_lines: 1 }]));
    // errors are plain sentences, and nothing outside the repository is read
    let e = changes::file_of_card(&c.st, &c.task, "../secret.txt", None, &from, None).unwrap_err();
    assert!(e.starts_with("Gizai can't show ../secret.txt:"), "{e}");
}

#[tokio::test]
async fn a_card_whose_worktree_is_gone_shows_its_branch() {
    let tmp = real_tempdir();
    let c = worked_card(tmp.path()).await;
    std::fs::write(c.wt.join("notes.md"), "not committed\n").unwrap();
    git(&c.local, &["worktree", "remove", "--force", c.wt.to_str().unwrap()]);
    let tip = git(&c.local, &["rev-parse", &c.branch]);
    let got = changes::of_card(&c.st, &c.task).unwrap();
    assert_eq!((got.source, got.from.as_deref(), got.to.as_deref(), got.base_label.as_str()),
               (ChangeSource::Branch, Some(c.start.as_str()), Some(tip.as_str()), "origin/main"));
    assert_eq!(files_of(&got), the_card(), "its commits only");
    assert_eq!(got.note, Some(format!("From its branch {}: its worktree was removed.", c.branch)));
    let e = changes::file_of_card(&c.st, &c.task, "report.txt", None, &c.start, None).unwrap_err();
    assert_eq!(e, "KADE-1's worktree is gone: open its changes again");
    let report = changes::file_of_card(&c.st, &c.task, "report.txt", None, &c.start, Some(&tip)).unwrap();
    assert_eq!(report.new_text.as_deref(), Some("one\ntwo\nthree\n"));
}

#[tokio::test]
async fn a_merge_commit_a_squash_and_a_rebase_merge_each_save_the_whole_card_which_shows_offline_once_its_branch_is_gone() {
    for (how, message) in [(How::MergeCommit, "Merge pull request #7 from acme/x"), (How::Squash, "KADE-1: Export invoices as CSV (#7)"),
                           (How::Rebase, "")] {
        let tmp = real_tempdir();
        let c = worked_card(tmp.path()).await;
        let tip = git(&c.wt, &["rev-parse", "HEAD"]);
        in_review(&c).await;
        let merge = merge_on_github(tmp.path(), &c, how, message);
        list(&c, "MERGED", &tip, Some(&merge));
        // the PR check: fetches main, saves the range, then removes the worktree and the branch
        let checked = pulls::check_all(&c.st).await;
        assert_eq!(checked[0].moved_to.as_deref(), Some("Done"), "{how:?}");
        assert!(!c.wt.exists() && !has_branch(&c.local, &c.branch), "{how:?}: worktree and branch gone");
        // (origin is written as the link, so main comes through origin, which the clone's rewrite sends to the bare
        // repository; GA-203)
        assert_eq!(git(&c.local, &["rev-parse", "refs/remotes/origin/main"]), merge, "{how:?}: main fetched as a card start does");
        assert!(!Command::new("git").args(["rev-parse", "--verify", "--quiet", "refs/gizai/base/main"]).current_dir(&c.local).output().unwrap()
            .status.success(), "{how:?}: not fetched from the link");
        assert_eq!(git(&c.local, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/gizai/merged/"]),
                   format!("refs/gizai/merged/KADE-1/base {}\nrefs/gizai/merged/KADE-1/head {tip}", c.start), "{how:?}");

        // offline, gh gone, and git's clean-up done: the saved range still shows the whole card
        offline(tmp.path(), &c.local);
        gizai_core::settings::set(&c.st.db, "gh_bin", &"/nonexistent/gh".to_string()).unwrap();
        git(&c.local, &["reflog", "expire", "--expire=now", "--all"]);
        git(&c.local, &["gc", "-q", "--prune=now"]);
        let got = changes::of_card(&c.st, &c.task).unwrap();
        assert_eq!((got.source, got.from.as_deref(), got.to.as_deref()), (ChangeSource::Merge, Some(c.start.as_str()), Some(tip.as_str())), "{how:?}");
        assert_eq!(files_of(&got), the_card(), "{how:?}: both commits, nothing of main's own");
        assert_eq!(got.note.as_deref(), Some("As merged into main (pull request #7)."));
        let report = changes::file_of_card(&c.st, &c.task, "report.txt", None, &c.start, Some(&tip)).unwrap();
        assert_eq!(report.new_text.as_deref(), Some("one\ntwo\nthree\n"), "{how:?}");
        // the refs stay on this computer
        let remote = git(tmp.path(), &["ls-remote", c.bare.to_str().unwrap()]);
        assert!(remote.contains("refs/heads/main") && !remote.contains("refs/gizai"), "{how:?}: {remote}");
        assert!(!gh_calls(&c.gh).iter().any(|l| l.starts_with("pr view") || l.starts_with("pr diff")), "{:?}", gh_calls(&c.gh));
    }
}

#[tokio::test]
async fn a_merge_seen_offline_saves_the_range_from_main_as_last_fetched() {
    for (how, message) in [(How::Squash, "KADE-1: Export invoices as CSV (#7)"), (How::Rebase, "")] {
        let tmp = real_tempdir();
        let c = worked_card(tmp.path()).await;
        let tip = git(&c.wt, &["rev-parse", "HEAD"]);
        in_review(&c).await;
        let merge = merge_on_github(tmp.path(), &c, how, message);
        list(&c, "MERGED", &tip, Some(&merge));
        // GitHub can't be fetched at the merge, so this repository doesn't have the merge commit
        offline(tmp.path(), &c.local);
        let checked = pulls::check_all(&c.st).await;
        assert_eq!(checked[0].moved_to.as_deref(), Some("Done"), "{how:?}");
        assert!(!c.wt.exists() && !has_branch(&c.local, &c.branch), "{how:?}: worktree and branch gone");
        let got = changes::of_card(&c.st, &c.task).unwrap();
        assert_eq!((got.source, got.from.as_deref(), got.to.as_deref()), (ChangeSource::Merge, Some(c.start.as_str()), Some(tip.as_str())), "{how:?}");
        assert_eq!(files_of(&got), the_card(), "{how:?}");
    }
}

#[tokio::test]
async fn a_card_merged_before_ranges_were_saved_shows_its_merge_commit_against_its_first_parent() {
    // found on main by GitHub's message (a merge commit, a squash), or asked of gh when its message says nothing
    for (how, message, asks_gh) in [(How::MergeCommit, "Merge pull request #7 from acme/{branch}", false),
                                     (How::Squash, "KADE-1: Export invoices as CSV (#7)", false),
                                     (How::MergeCommit, "Bring in the invoices", true)] {
        let tmp = real_tempdir();
        let c = worked_card(tmp.path()).await;
        let tip = git(&c.wt, &["rev-parse", "HEAD"]);
        in_review(&c).await;
        let merge = merge_on_github(tmp.path(), &c, how, &message.replace("{branch}", &c.branch));
        list(&c, "MERGED", &tip, Some(&merge));
        pulls::check_all(&c.st).await;
        assert_eq!(task(&c).pr_state.as_deref(), Some("merged"));
        // as before GA-106: no saved range
        git(&c.local, &["update-ref", "-d", "refs/gizai/merged/KADE-1/base"]);
        git(&c.local, &["update-ref", "-d", "refs/gizai/merged/KADE-1/head"]);
        std::fs::write(c.gh.join("view.json"), json!({"mergeCommit": {"oid": merge}}).to_string()).unwrap();
        let got = changes::of_card(&c.st, &c.task).unwrap();
        let parent = git(&c.local, &["rev-parse", &format!("{merge}^1")]);
        assert_eq!((got.source, got.from.as_deref(), got.to.as_deref()), (ChangeSource::Merge, Some(parent.as_str()), Some(merge.as_str())), "{message}");
        assert_eq!(files_of(&got), the_card(), "{message}: what the pull request brought into main");
        assert_eq!(got.note, Some(format!("From the merge commit {} on main.", &merge[..7])));
        let asked = gh_calls(&c.gh).iter().any(|l| l == "pr view 7 --repo acme/shop --json mergeCommit");
        assert_eq!(asked, asks_gh, "{message}: {:?}", gh_calls(&c.gh));
    }
}

#[tokio::test]
async fn with_nothing_left_in_git_the_pull_requests_diff_from_gh_shows_without_the_files_texts() {
    let tmp = real_tempdir();
    let c = worked_card(tmp.path()).await;
    in_review(&c).await;
    let diff = git(&c.wt, &["diff", "--no-color", &c.start, "HEAD"]) + "\n";
    std::fs::write(c.gh.join("diff.out"), &diff).unwrap();
    git(&c.local, &["worktree", "remove", "--force", c.wt.to_str().unwrap()]);
    git(&c.local, &["branch", "-q", "-D", &c.branch]);

    let got = changes::of_card(&c.st, &c.task).unwrap();
    assert_eq!((got.source, got.from.as_deref(), got.to.as_deref()), (ChangeSource::Pull, None, None));
    assert_eq!(files_of(&got), the_card());
    assert_eq!(got.note.as_deref(), Some("From GitHub's diff of pull request #7, without the files' texts."));
    let report = changes::file_of_card(&c.st, &c.task, "report.txt", None, "", None).unwrap();
    assert_eq!((report.old_text, report.new_text, report.hunks, report.too_large), (None, None, vec![], false));
    let part = report.patch.unwrap();
    assert!(part.starts_with("diff --git a/report.txt b/report.txt\n") && part.contains("+three\n") && !part.contains("invoices"), "{part}");
    // asked once, with the files as they are
    let asked: Vec<String> = gh_calls(&c.gh).into_iter().filter(|l| l.starts_with("pr diff")).collect();
    assert_eq!(asked, ["pr diff 7 --repo acme/shop --color never --allow-escape-sequences"]);
    let e = changes::file_of_card(&c.st, &c.task, "nope.txt", None, "", None).unwrap_err();
    assert_eq!(e, "nope.txt isn't in the diff of pull request #7");

    // gh can't be reached either: a plain sentence that says what to do
    let tmp2 = real_tempdir();
    let c2 = worked_card(tmp2.path()).await;
    in_review(&c2).await;
    git(&c2.local, &["worktree", "remove", "--force", c2.wt.to_str().unwrap()]);
    git(&c2.local, &["branch", "-q", "-D", &c2.branch]);
    gizai_core::settings::set(&c2.st.db, "gh_bin", &"/nonexistent/gh".to_string()).unwrap();
    let e = changes::of_card(&c2.st, &c2.task).unwrap_err();
    assert!(e.starts_with("Gizai can't find what KADE-1 changed: fetch main, or log gh in (Settings → GitHub). "), "{e}");
}

#[tokio::test]
async fn a_runs_changes_are_the_commits_it_made() {
    let tmp = real_tempdir();
    let c = worked_card(tmp.path()).await;
    let run = gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().remove(0);
    // the fake agent made no commits: the two above came after its run ended
    let got = changes::of_run(&c.st, &run.id).unwrap();
    assert_eq!((got.source, got.files.len(), got.note.as_deref(), got.base_label.as_str()),
               (ChangeSource::Run, 0, Some("This run made no commits."), "origin/main"));
    // as if the run had made them: from the commit it started at to the one it ended at
    let head = git(&c.wt, &["rev-parse", "HEAD"]);
    gizai_core::runs::set_head_sha(&c.st.db, &run.id, &head).unwrap();
    let got = changes::of_run(&c.st, &run.id).unwrap();
    assert_eq!((got.source, got.from.as_deref(), got.to.as_deref()), (ChangeSource::Run, Some(c.start.as_str()), Some(head.as_str())));
    assert_eq!(files_of(&got), the_card());
    assert_eq!(got.note.as_deref(), Some("The 2 commits this run made."));
    // a run that started after the first: only its own commit
    gizai_core::runs::set_base_sha(&c.st.db, &run.id, &git(&c.wt, &["rev-parse", "HEAD~1"])).unwrap();
    let got = changes::of_run(&c.st, &run.id).unwrap();
    assert_eq!(files_of(&got), [("readme.md".to_string(), FileStatus::Modified, Some(1), Some(0), false),
                                ("report.txt".to_string(), FileStatus::Added, Some(3), Some(0), false)]);
    assert_eq!(got.note.as_deref(), Some("The commit this run made."));
    assert!(changes::of_run(&c.st, "no-such-run").is_err());
}

/// A temp folder by its real path, the way git and Gizai report it (on macOS /var/folders is /private/var/folders).
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    tempfile::tempdir_in(base).unwrap()
}
