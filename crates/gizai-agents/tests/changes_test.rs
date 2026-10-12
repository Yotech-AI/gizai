//! GA-106: a card's changes from local git, on temporary repositories. The file list of a worktree (its commits,
//! uncommitted and untracked files, a rename, a binary) and of a branch alone, measured from the merge base with main;
//! a run's range; each file's texts and git's -U0 hunks, which agree with `git diff --numstat`; files over 1 MB;
//! `.gizai-tmp/`; the range saved at a merge commit, a squash and a rebase merge; a card merged before ranges were
//! saved; a pull request's diff; the refs, which no push sends; and the types as contract C1 has them.
use std::path::{Path, PathBuf};
use std::process::Command;

use gizai_agents::changes::{self, CardChanges, ChangeSource, DiffHunk, FileChange, FileDiff, FileStatus, MAX_TEXT};
use serde_json::json;

/// git in `dir` as a fixed person, whatever this computer's git config says about line ends and signing.
fn git_out(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
        .args(args).current_dir(dir).output().unwrap()
}

/// git's output as it is (not trimmed): a file's text.
fn git_raw(dir: &Path, args: &[&str]) -> String {
    let o = git_out(dir, args);
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn git(dir: &Path, args: &[&str]) -> String {
    git_raw(dir, args).trim_end().to_string()
}

fn write(dir: &Path, path: &str, bytes: impl AsRef<[u8]>) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// Commits everything in `dir` and returns the commit's id.
fn commit(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

/// "line 1\n" … "line n\n".
fn lines(prefix: &str, n: usize) -> String {
    (1..=n).map(|i| format!("{prefix} {i}\n")).collect()
}

fn new_repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    repo
}

/// What the screen gets of a file: (path, old path, status, (additions, deletions) or None for a binary, uncommitted).
type Row = (String, Option<String>, FileStatus, Option<(u64, u64)>, bool);

fn rows(files: &[FileChange]) -> Vec<Row> {
    files.iter().map(|f| {
        assert_eq!(f.binary, f.additions.is_none() && f.deletions.is_none(), "{f:?}: a binary, and only a binary, has no counts");
        let counts = f.additions.zip(f.deletions);
        (f.path.clone(), f.old_path.clone(), f.status, counts, f.uncommitted)
    }).collect()
}

fn row(path: &str, old: Option<&str>, status: FileStatus, counts: Option<(u64, u64)>, uncommitted: bool) -> Row {
    (path.to_string(), old.map(String::from), status, counts, uncommitted)
}

/// `git diff --numstat -M <range> -- [<old>] <path>` for one file: (added, deleted), None for a binary's "-".
fn numstat(dir: &Path, range: &[&str], old: Option<&str>, path: &str) -> Option<(u64, u64)> {
    let mut args = vec!["diff", "--numstat", "-M"];
    args.extend(range);
    args.push("--");
    args.extend(old);
    args.push(path);
    let out = git(dir, &args);
    numstat_line(&out, path)
}

/// git's numstat of a file it doesn't track: as new, against nothing (`git diff --no-index`, which exits with 1 for a
/// difference).
fn numstat_new(dir: &Path, path: &str) -> Option<(u64, u64)> {
    let o = git_out(dir, &["diff", "--no-index", "--numstat", "--", "/dev/null", path]);
    assert_eq!(o.status.code(), Some(1), "{}", String::from_utf8_lossy(&o.stderr));
    numstat_line(&String::from_utf8_lossy(&o.stdout), path)
}

fn numstat_line(out: &str, path: &str) -> Option<(u64, u64)> {
    let line = out.lines().next().unwrap_or_else(|| panic!("no numstat for {path}"));
    let mut f = line.split('\t');
    let (a, d) = (f.next().unwrap(), f.next().unwrap());
    a.parse().ok().zip(d.parse().ok())
}

/// What git's -U0 hunks of a file add up to: (lines added, lines removed).
fn hunk_totals(d: &FileDiff) -> (u64, u64) {
    (d.hunks.iter().map(|h| h.new_lines).sum(), d.hunks.iter().map(|h| h.old_lines).sum())
}

fn hunk(old_start: u64, old_lines: u64, new_start: u64, new_lines: u64) -> DiffHunk {
    DiffHunk { old_start, old_lines, new_start, new_lines }
}

const BRANCH: &str = "gizai/ga-1-x";

struct Card {
    repo: PathBuf,
    wt: PathBuf,
    /// main when the card started.
    start: String,
    /// its latest commit.
    tip: String,
}

/// A card in its own worktree (branch gizai/ga-1-x from main) with two commits: a changed line and two new ones, a
/// rename with a change (into a folder, with a space and an "é"), a binary, a deleted file, a CRLF file's change and a
/// committed file in .gizai-tmp/; then a new file without a last newline. Not committed yet: a change on top of a
/// committed one, a change of its own, a staged new file, two new files git doesn't track (one binary), an ignored
/// file and a scratch file in .gizai-tmp/.
fn card_in_worktree(tmp: &Path) -> Card {
    let repo = new_repo(tmp);
    write(&repo, "a.txt", lines("line", 10));
    write(&repo, "old.txt", lines("keep", 20));
    write(&repo, "keep.txt", "one\ntwo\nthree\n");
    write(&repo, "gone.txt", "bye\nbye\nbye\n");
    write(&repo, "crlf.txt", "a\r\nb\r\nc\r\n");
    write(&repo, ".gitignore", "*.log\n");
    let start = commit(&repo, "start");
    let wt = tmp.join("wt");
    git(&repo, &["worktree", "add", "-q", "-b", BRANCH, wt.to_str().unwrap(), "main"]);

    write(&wt, "a.txt", lines("line", 12).replace("line 3\n", "LINE 3\n"));
    std::fs::create_dir_all(wt.join("new dir")).unwrap();
    git(&wt, &["mv", "old.txt", "new dir/new é.txt"]);
    write(&wt, "new dir/new é.txt", lines("keep", 20).replace("keep 7\n", "kept 7\n"));
    write(&wt, "bin.png", [0x89u8, b'P', b'N', b'G', 0, 1, 2, 0, 255]);
    std::fs::remove_file(wt.join("gone.txt")).unwrap();
    write(&wt, "crlf.txt", "a\r\nB\r\nc\r\n");
    write(&wt, ".gizai-tmp/committed.txt", "kept out\n");
    commit(&wt, "first");
    write(&wt, "src/lib.rs", "fn x() {}");
    let tip = commit(&wt, "second");

    write(&wt, "a.txt", lines("line", 13).replace("line 3\n", "LINE 3\n"));
    write(&wt, "keep.txt", "one\nTWO\nthree\nfour\n");
    write(&wt, "staged.txt", "staged\n");
    git(&wt, &["add", "staged.txt"]);
    write(&wt, "notes.md", "# Notes\n\nfirst");
    write(&wt, "photo.bin", [0u8, 159, 146, 150]);
    write(&wt, "debug.log", "ignored\n");
    write(&wt, ".gizai-tmp/scratch.txt", "scratch\n");
    Card { repo, wt, start, tip }
}

/// The worktree's changes as the screen lists them.
fn worktree_rows() -> Vec<Row> {
    use FileStatus::*;
    vec![
        row("a.txt", None, Modified, Some((4, 1)), true),
        row("bin.png", None, Added, None, false),
        row("crlf.txt", None, Modified, Some((1, 1)), false),
        row("gone.txt", None, Deleted, Some((0, 3)), false),
        row("keep.txt", None, Modified, Some((2, 1)), true),
        row("new dir/new é.txt", Some("old.txt"), Renamed, Some((1, 1)), false),
        row("notes.md", None, Added, Some((3, 0)), true),
        row("photo.bin", None, Added, None, true),
        row("src/lib.rs", None, Added, Some((1, 0)), false),
        row("staged.txt", None, Added, Some((1, 0)), true),
    ]
}

/// The branch's changes (its commits only).
fn branch_rows() -> Vec<Row> {
    use FileStatus::*;
    vec![
        row("a.txt", None, Modified, Some((3, 1)), false),
        row("bin.png", None, Added, None, false),
        row("crlf.txt", None, Modified, Some((1, 1)), false),
        row("gone.txt", None, Deleted, Some((0, 3)), false),
        row("new dir/new é.txt", Some("old.txt"), Renamed, Some((1, 1)), false),
        row("src/lib.rs", None, Added, Some((1, 0)), false),
    ]
}

#[test]
fn a_worktree_shows_its_commits_uncommitted_and_untracked_files_a_rename_and_a_binary() {
    let tmp = tempfile::tempdir().unwrap();
    let card = card_in_worktree(tmp.path());
    let base = changes::merge_base(&card.wt, "HEAD", "main").unwrap();
    assert_eq!(base, card.start);
    let files = changes::of_worktree(&card.wt, &base).unwrap();
    let all = CardChanges::new(ChangeSource::Worktree, Some(base.clone()), None, "origin/main", files, None);
    // in path order, the ignored file and everything in .gizai-tmp/ left out
    assert_eq!(rows(&all.files), worktree_rows());
    assert_eq!((all.additions, all.deletions), (13, 7), "the totals of the files with counts");

    // each count is git's own: numstat against the merge base, or against nothing for a file git doesn't track
    for f in &all.files {
        let want = if f.uncommitted && f.status == FileStatus::Added && f.path != "staged.txt" {
            numstat_new(&card.wt, &f.path)
        } else {
            numstat(&card.wt, &[&base], f.old_path.as_deref(), &f.path)
        };
        assert_eq!(f.additions.zip(f.deletions), want, "{}", f.path);
    }
}

#[test]
fn each_files_texts_and_hunks_agree_with_git_diff_numstat() {
    let tmp = tempfile::tempdir().unwrap();
    let card = card_in_worktree(tmp.path());
    let base = card.start.clone();
    let files = changes::of_worktree(&card.wt, &base).unwrap();
    assert_eq!(files.len(), 10);
    // the worktree as it is on disk
    for f in &files {
        let d = changes::file_diff(&card.repo, Some(&card.wt), &f.path, f.old_path.as_deref(), &base, None).unwrap();
        assert_eq!((d.path.as_str(), d.binary, d.too_large, d.patch.as_deref()), (f.path.as_str(), f.binary, false, None), "{}", f.path);
        let before = f.old_path.as_deref().unwrap_or(&f.path);
        let old = (f.status != FileStatus::Added).then(|| git_raw(&card.repo, &["show", &format!("{base}:{before}")]));
        let new = (f.status != FileStatus::Deleted).then(|| std::fs::read(card.wt.join(&f.path)).unwrap());
        assert_eq!((d.old_size, d.new_size), (old.as_ref().map_or(0, |t| t.len() as u64), new.as_ref().map_or(0, |b| b.len() as u64)), "{}", f.path);
        if f.binary {
            assert_eq!((d.old_text, d.new_text, d.hunks), (None, None, vec![]), "{}: no texts or hunks for a binary", f.path);
        } else {
            assert_eq!(hunk_totals(&d), f.additions.zip(f.deletions).unwrap(), "{}: hunks {:?}", f.path, d.hunks);
            assert_eq!(d.old_text, old, "{}", f.path);
            assert_eq!(d.new_text, new.map(|b| String::from_utf8(b).unwrap()), "{}", f.path);
        }
    }
    let diff = |path: &str, old: Option<&str>, to: Option<&str>| {
        let wt = to.is_none().then_some(card.wt.as_path());
        changes::file_diff(&card.repo, wt, path, old, &base, to).unwrap()
    };
    // git's hunks as -U0 prints them: a side of 0 lines starts at the line before (0 at the top)
    assert_eq!(diff("a.txt", None, None).hunks, [hunk(3, 1, 3, 1), hunk(10, 0, 11, 3)]);
    assert_eq!(diff("gone.txt", None, None).hunks, [hunk(1, 3, 0, 0)]);
    assert_eq!(diff("notes.md", None, None).hunks, [hunk(0, 0, 1, 3)], "a new file git doesn't track: all new");
    assert_eq!(diff("staged.txt", None, None).hunks, [hunk(0, 0, 1, 1)]);
    assert_eq!(diff("new dir/new é.txt", Some("old.txt"), None).hunks, [hunk(7, 1, 7, 1)]);
    assert_eq!(diff("src/lib.rs", None, None).hunks, [hunk(0, 0, 1, 1)], "a last line without its newline counts");
    let crlf = diff("crlf.txt", None, None);
    assert_eq!((crlf.crlf, crlf.hunks, crlf.new_text.as_deref()), (true, vec![hunk(2, 1, 2, 1)], Some("a\r\nB\r\nc\r\n")));
    assert!(!diff("a.txt", None, None).crlf);

    // between two commits (the branch's tip): the file as committed, without what isn't committed yet
    let tip = card.tip.as_str();
    let a = diff("a.txt", None, Some(tip));
    assert_eq!(a.hunks, [hunk(3, 1, 3, 1), hunk(10, 0, 11, 2)]);
    assert_eq!(a.new_text, Some(git_raw(&card.repo, &["show", &format!("{tip}:a.txt")])));
    assert_eq!(hunk_totals(&a), numstat(&card.repo, &[&base, tip], None, "a.txt").unwrap());
    let renamed = diff("new dir/new é.txt", Some("old.txt"), Some(tip));
    assert_eq!((renamed.hunks, renamed.old_text.map(|t| t.len())), (vec![hunk(7, 1, 7, 1)], Some(lines("keep", 20).len())));
}

#[test]
fn commits_main_got_since_the_card_started_dont_show() {
    let tmp = tempfile::tempdir().unwrap();
    let card = card_in_worktree(tmp.path());
    // main moves on after the card started: a new file and a change to a file the card changed too
    write(&card.repo, "main-only.txt", "from main\n");
    write(&card.repo, "a.txt", lines("line", 10).replace("line 1\n", "LINE ONE\n"));
    let main_tip = commit(&card.repo, "main moves on");

    let base = changes::merge_base(&card.wt, "HEAD", "main").unwrap();
    assert_eq!(base, card.start, "where the card started, not main's tip");
    let files = changes::of_worktree(&card.wt, &base).unwrap();
    let all = CardChanges::new(ChangeSource::Worktree, Some(base.clone()), None, "main", files, None);
    assert_eq!(rows(&all.files), worktree_rows(), "main's commit isn't the card's");
    let a = changes::file_diff(&card.repo, Some(&card.wt), "a.txt", None, &base, None).unwrap();
    assert_eq!(a.hunks, [hunk(3, 1, 3, 1), hunk(10, 0, 11, 3)]);
    // what the merge base avoids: against main's tip, main's own commit would show as removed by the card
    let against_tip = changes::of_worktree(&card.wt, &main_tip).unwrap();
    assert!(against_tip.iter().any(|f| f.path == "main-only.txt" && f.status == FileStatus::Deleted), "{against_tip:?}");
}

#[test]
fn a_card_with_only_its_branch_shows_its_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let card = card_in_worktree(tmp.path());
    git(&card.repo, &["worktree", "remove", "--force", card.wt.to_str().unwrap()]);
    assert!(!card.wt.exists());
    let tip = changes::commit_of(&card.repo, &format!("refs/heads/{BRANCH}")).unwrap();
    assert_eq!(tip, card.tip);
    let base = changes::merge_base(&card.repo, &tip, "main").unwrap();
    let files = changes::between(&card.repo, &base, &tip).unwrap();
    assert_eq!(rows(&files), branch_rows(), "its commits only; .gizai-tmp/ left out");
    for f in &files {
        assert_eq!(f.additions.zip(f.deletions), numstat(&card.repo, &[&base, &tip], f.old_path.as_deref(), &f.path), "{}", f.path);
        let d = changes::file_diff(&card.repo, None, &f.path, f.old_path.as_deref(), &base, Some(&tip)).unwrap();
        if !f.binary {
            assert_eq!(hunk_totals(&d), f.additions.zip(f.deletions).unwrap(), "{}: {:?}", f.path, d.hunks);
        }
    }
    // the worktree is gone: a file as it is on disk can't be read
    assert!(changes::file_diff(&card.repo, None, "a.txt", None, &base, None).is_err());
}

#[test]
fn a_runs_range_shows_only_the_commits_it_made() {
    let tmp = tempfile::tempdir().unwrap();
    let card = card_in_worktree(tmp.path());
    // a run that started at the card's first commit and made the second
    let started = git(&card.wt, &["rev-parse", "HEAD~1"]);
    let files = changes::between(&card.repo, &started, &card.tip).unwrap();
    assert_eq!(rows(&files), [row("src/lib.rs", None, FileStatus::Added, Some((1, 0)), false)]);
    assert!(changes::between(&card.repo, &card.tip, &card.tip).unwrap().is_empty(), "a run without commits");
    // commit ids only: never a ref, a range or an option for git
    for bad in ["HEAD", "main..HEAD", "--output=x", ""] {
        assert!(changes::between(&card.repo, bad, &card.tip).is_err(), "{bad}");
        assert!(changes::between(&card.repo, &started, bad).is_err(), "{bad}");
    }
}

#[test]
fn a_file_over_1_mb_comes_too_large_without_its_text() {
    assert_eq!(MAX_TEXT, 1_000_000, "1 MB a side is 1,000,000 bytes");
    let tmp = tempfile::tempdir().unwrap();
    let repo = new_repo(tmp.path());
    let line = format!("{}\n", "x".repeat(99)); // 100 bytes
    write(&repo, "big.txt", line.repeat(10_001)); // 1,000,100 bytes
    write(&repo, "exact.txt", line.repeat(10_000)); // 1,000,000 bytes: still shown
    let start = commit(&repo, "start");
    git(&repo, &["checkout", "-q", "-b", "card"]);
    write(&repo, "big.txt", line.repeat(10_002));
    write(&repo, "exact.txt", line.repeat(9_999) + &format!("{}\n", "y".repeat(99)));
    let tip = commit(&repo, "card");
    write(&repo, "huge.txt", line.repeat(10_000) + "z"); // not committed: 1,000,001 bytes

    let big = changes::file_diff(&repo, None, "big.txt", None, &start, Some(&tip)).unwrap();
    assert_eq!((big.too_large, big.old_text, big.new_text, big.hunks, big.old_size, big.new_size, big.binary),
               (true, None, None, vec![], 1_000_100, 1_000_200, false));
    let exact = changes::file_diff(&repo, None, "exact.txt", None, &start, Some(&tip)).unwrap();
    assert_eq!((exact.too_large, exact.old_size, exact.new_size, exact.hunks), (false, 1_000_000, 1_000_000, vec![hunk(10_000, 1, 10_000, 1)]));
    assert_eq!(exact.new_text.map(|t| t.len()), Some(1_000_000));
    // on disk, as a worktree has it
    let huge = changes::file_diff(&repo, Some(&repo), "huge.txt", None, &tip, None).unwrap();
    assert_eq!((huge.too_large, huge.old_text, huge.new_text, huge.hunks, huge.new_size), (true, None, None, vec![], 1_000_001));
    let big_on_disk = changes::file_diff(&repo, Some(&repo), "big.txt", None, &start, None).unwrap();
    assert_eq!((big_on_disk.too_large, big_on_disk.new_text), (true, None));
    // the list still counts them
    let files = changes::of_worktree(&repo, &start).unwrap();
    assert_eq!(rows(&files), [
        row("big.txt", None, FileStatus::Modified, Some((1, 0)), false),
        row("exact.txt", None, FileStatus::Modified, Some((1, 1)), false),
        row("huge.txt", None, FileStatus::Added, Some((10_001, 0)), true),
    ]);
}

#[test]
fn a_path_outside_the_repository_or_in_git_is_never_read() {
    let tmp = tempfile::tempdir().unwrap();
    let card = card_in_worktree(tmp.path());
    let base = card.start.as_str();
    for bad in ["../secret.txt", "/etc/passwd", "a/../../x", ".git/config", "src/.GIT/x", "", "./a.txt", "a//b"] {
        assert!(changes::file_diff(&card.repo, Some(&card.wt), bad, None, base, None).is_err(), "{bad}");
        assert!(changes::file_diff(&card.repo, Some(&card.wt), "a.txt", Some(bad), base, None).is_err() || bad.is_empty(), "old path {bad}");
    }
    for bad in ["HEAD", "main", "--output=x", "a..b", ""] {
        assert!(changes::file_diff(&card.repo, Some(&card.wt), "a.txt", None, bad, None).is_err(), "from {bad}");
        assert!(changes::file_diff(&card.repo, Some(&card.wt), "a.txt", None, base, Some(bad)).is_err(), "to {bad}");
    }
    // a link in the worktree that leads out of it is never followed
    #[cfg(unix)]
    {
        let outside = tmp.path().join("outside");
        write(&outside, "secret.txt", "secret\n");
        std::os::unix::fs::symlink(&outside, card.wt.join("out")).unwrap();
        let d = changes::file_diff(&card.repo, Some(&card.wt), "out/secret.txt", None, base, None).unwrap();
        assert_eq!((d.old_text, d.new_text, d.hunks), (None, None, vec![]));
    }
}

// ---- a merged card ----

#[derive(Debug, Clone, Copy)]
enum How {
    MergeCommit,
    Squash,
    Rebase,
}

struct Merged {
    repo: PathBuf,
    /// main when the card started.
    start: String,
    /// the card's latest commit, as its pull request had it.
    tip: String,
    /// main just before the merge.
    before: String,
    /// the commit main is at after the merge: the merge commit, the squashed commit, or the last rebased one.
    merge: String,
}

/// Card branch "card" with two commits from `start`, merged into main as GitHub does `how` (with `message` for a merge
/// commit or a squash), after main got a commit of its own.
fn merged(tmp: &Path, how: How, message: &str) -> Merged {
    let repo = new_repo(tmp);
    write(&repo, "a.txt", lines("line", 10));
    write(&repo, "b.txt", "b\n");
    let start = commit(&repo, "start");
    git(&repo, &["checkout", "-q", "-b", "card"]);
    write(&repo, "card1.txt", "one\n");
    commit(&repo, "card one");
    write(&repo, "a.txt", lines("line", 10).replace("line 5\n", "LINE 5\n"));
    write(&repo, "card2.txt", "two\ntwo\n");
    let tip = commit(&repo, "card two");
    git(&repo, &["checkout", "-q", "main"]);
    write(&repo, "main.txt", "main\n");
    write(&repo, "b.txt", "B\n");
    let before = commit(&repo, "main's own");
    match how {
        How::MergeCommit => {
            git(&repo, &["merge", "-q", "--no-ff", "card", "-m", message]);
        }
        How::Squash => {
            git(&repo, &["merge", "-q", "--squash", "card"]);
            git(&repo, &["commit", "-q", "-m", message]);
        }
        How::Rebase => {
            git(&repo, &["checkout", "-q", "-b", "rebased", "card"]);
            git(&repo, &["rebase", "-q", "main"]);
            git(&repo, &["checkout", "-q", "main"]);
            git(&repo, &["merge", "-q", "--ff-only", "rebased"]);
            git(&repo, &["branch", "-q", "-D", "rebased"]);
        }
    }
    let merge = git(&repo, &["rev-parse", "HEAD"]);
    assert_ne!(merge, tip);
    Merged { repo, start, tip, before, merge }
}

/// The whole card: both its commits, nothing of main's own.
fn the_card() -> Vec<Row> {
    vec![
        row("a.txt", None, FileStatus::Modified, Some((1, 1)), false),
        row("card1.txt", None, FileStatus::Added, Some((1, 0)), false),
        row("card2.txt", None, FileStatus::Added, Some((2, 0)), false),
    ]
}

const HOWS: [(How, &str); 3] =
    [(How::MergeCommit, "Merge pull request #7 from acme/card"), (How::Squash, "Card's two things (#7)"), (How::Rebase, "")];

#[test]
fn a_merge_commit_a_squash_and_a_rebase_each_save_a_range_that_shows_the_whole_card_once_the_branch_is_gone() {
    for (how, message) in HOWS {
        let tmp = tempfile::tempdir().unwrap();
        let m = merged(tmp.path(), how, message);
        let want = Some((m.start.clone(), m.tip.clone()));
        // GitHub's merge commit (mergeCommit), Bitbucket's short hash of it, and main fetched at the merge
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, Some(&m.merge), &[&m.before]), want, "{how:?}");
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, Some(&m.merge[..12]), &[&m.before]), want, "{how:?}: a short hash");
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, Some(&m.merge), &[&m.merge, &m.before]), want, "{how:?}");
        let (base, head) = want.clone().unwrap();
        assert_eq!(changes::saved_range(&m.repo, "GA-7"), None, "nothing saved yet");
        changes::save_range(&m.repo, "GA-7", &base, &head).unwrap();

        // its branch is deleted and git cleans up: after a squash or a rebase only the saved refs keep its commits
        git(&m.repo, &["branch", "-q", "-D", "card"]);
        git(&m.repo, &["reflog", "expire", "--expire=now", "--all"]);
        git(&m.repo, &["gc", "-q", "--prune=now"]);
        assert_eq!(git(&m.repo, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/gizai/"]),
                   format!("refs/gizai/merged/GA-7/base {}\nrefs/gizai/merged/GA-7/head {}", m.start, m.tip), "{how:?}");
        assert_eq!(changes::saved_range(&m.repo, "GA-7"), want, "{how:?}");
        // no remote at all: offline
        assert_eq!(git(&m.repo, &["remote"]), "");
        let files = changes::between(&m.repo, &base, &head).unwrap();
        assert_eq!(rows(&files), the_card(), "{how:?}");
        let a = changes::file_diff(&m.repo, None, "a.txt", None, &base, Some(&head)).unwrap();
        assert_eq!((a.hunks, a.old_text), (vec![hunk(5, 1, 5, 1)], Some(lines("line", 10))), "{how:?}");
        // saved again (a card merged twice): the new range takes the old one's place
        changes::save_range(&m.repo, "GA-7", &m.before, &m.merge).unwrap();
        assert_eq!(changes::saved_range(&m.repo, "GA-7"), Some((m.before.clone(), m.merge.clone())));
    }
}

#[test]
fn offline_or_without_the_merge_commit_main_as_last_fetched_gives_the_range() {
    for (how, message) in HOWS {
        let tmp = tempfile::tempdir().unwrap();
        let m = merged(tmp.path(), how, message);
        let want = Some((m.start.clone(), m.tip.clone()));
        // offline: no merge commit, main as it was last fetched (before the merge)
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, None, &[&m.before]), want, "{how:?}");
        // a merge commit this repository doesn't have (not fetched), or the tip itself (a fast-forward)
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, Some(&"ab".repeat(20)), &[&m.before]), want, "{how:?}");
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, Some(&m.tip), &[&m.before]), want, "{how:?}");
        // the first main that isn't the tip itself
        assert_eq!(changes::range_at_merge(&m.repo, &m.tip, None, &[&m.tip, &m.before]), want, "{how:?}");
    }
    // main had all of the card already: every way gives the tip itself, and nothing is to be saved
    let tmp = tempfile::tempdir().unwrap();
    let m = merged(tmp.path(), How::MergeCommit, "Merge pull request #7 from acme/card");
    assert_eq!(changes::range_at_merge(&m.repo, &m.tip, None, &[&m.merge]), None);
    assert_eq!(changes::range_at_merge(&m.repo, &m.tip, Some(&m.tip), &[&m.tip]), None);
    assert_eq!(changes::range_at_merge(&m.repo, &"ab".repeat(20), Some(&m.merge), &[&m.before]), None, "a tip this repository doesn't have");
    // refs are only made for a card's identifier
    for bad in ["", "-x", "GA 7", "GA/7", "../x"] {
        assert!(changes::save_range(&m.repo, bad, &m.start, &m.tip).is_err(), "{bad}");
        assert_eq!(changes::saved_range(&m.repo, bad), None);
    }
}

#[test]
fn a_card_merged_before_its_range_was_saved_shows_its_merge_commit_against_its_first_parent() {
    for (how, message) in [(How::MergeCommit, "Merge pull request #7 from acme/card"), (How::Squash, "Card's two things (#7)"),
                           (How::MergeCommit, "Merged in card (pull request #7)\n\nApproved-by: Jeffrey")] {
        let tmp = tempfile::tempdir().unwrap();
        let m = merged(tmp.path(), how, message);
        assert_eq!(changes::saved_range(&m.repo, "GA-7"), None);
        git(&m.repo, &["branch", "-q", "-D", "card"]);
        let found = changes::find_merge_commit(&m.repo, "main", 7, "card");
        assert_eq!(found.as_deref(), Some(m.merge.as_str()), "{message}");
        let parent = changes::commit_of(&m.repo, &format!("{}^1", m.merge)).unwrap();
        assert_eq!(parent, m.before, "its first parent: main just before the merge");
        assert_eq!(rows(&changes::between(&m.repo, &parent, &m.merge).unwrap()), the_card(), "{message}");
        // another pull request's number, or another branch's merge
        assert_eq!(changes::find_merge_commit(&m.repo, "main", 17, "card"), None, "{message}");
        if message.starts_with("Merge pull request") {
            assert_eq!(changes::find_merge_commit(&m.repo, "main", 7, "other"), None);
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let m = merged(tmp.path(), How::MergeCommit, "Merge pull request #70 from acme/card");
    assert_eq!(changes::find_merge_commit(&m.repo, "main", 7, "card"), None, "#70 isn't #7");
    assert_eq!(changes::find_merge_commit(&m.repo, "main", 70, "card").as_deref(), Some(m.merge.as_str()));
}

#[test]
fn the_saved_refs_are_never_pushed() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    git(tmp.path(), &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
    let m = merged(tmp.path(), How::Squash, "Card's two things (#7)");
    git(&m.repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    changes::save_range(&m.repo, "GA-7", &m.start, &m.tip).unwrap();
    // every way a card's work goes out: Gizai's own push of a branch, an agent's push of HEAD, and git's --all and --tags
    gizai_agents::worktree::push_branch(&m.repo, "origin", "card").unwrap();
    git(&m.repo, &["push", "-q", "origin", "HEAD"]);
    git(&m.repo, &["push", "-q", "--all", "origin"]);
    git(&m.repo, &["push", "-q", "--tags", "origin"]);
    let remote = git(tmp.path(), &["ls-remote", bare.to_str().unwrap()]);
    assert!(remote.contains("refs/heads/card") && remote.contains("refs/heads/main"), "{remote}");
    assert!(!remote.contains("refs/gizai"), "{remote}");
    let clone = tmp.path().join("clone");
    git(tmp.path(), &["clone", "-q", "--mirror", bare.to_str().unwrap(), clone.to_str().unwrap()]);
    assert_eq!(git(&clone, &["for-each-ref", "refs/gizai/"]), "", "nobody else gets them");
    assert_eq!(changes::saved_range(&m.repo, "GA-7"), Some((m.start.clone(), m.tip.clone())), "still here");
}

// ---- a pull request's diff ----

#[test]
fn a_pull_requests_diff_splits_into_its_files_with_the_counts_git_gives() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = new_repo(tmp.path());
    write(&repo, "a.sql", "select 1;\n-- old comment\nselect 2;\n");
    write(&repo, "old name.txt", lines("same", 20));
    write(&repo, "gone.txt", "one\ntwo\n");
    write(&repo, "run.sh", "echo hi\n");
    write(&repo, "a b/c.txt", "1\n");
    let start = commit(&repo, "start");
    // lines that look like a diff's own "---" and "+++", a rename with a change, a binary, a deleted file, a name git
    // quotes, a mode change, a folder named "a b", and a file in .gizai-tmp/
    write(&repo, "a.sql", "select 1;\n++ new line\nselect 2;\n");
    git(&repo, &["mv", "old name.txt", "new name.txt"]);
    write(&repo, "new name.txt", lines("same", 20).replace("same 3\n", "other 3\n"));
    write(&repo, "logo.png", [0x89u8, b'P', b'N', b'G', 0, 0, 1]);
    std::fs::remove_file(repo.join("gone.txt")).unwrap();
    write(&repo, "café.txt", "hi\n");
    write(&repo, "a b/c.txt", "2\n");
    write(&repo, ".gizai-tmp/x.txt", "tmp\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["update-index", "--chmod=+x", "run.sh"]);
    git(&repo, &["commit", "-q", "-m", "card"]);
    let tip = git(&repo, &["rev-parse", "HEAD"]);
    // as GitHub sends it (`gh pr diff`): git's diff, names with special characters in quotes
    let patch = git_raw(&repo, &["-c", "core.quotePath=true", "diff", "--no-color", "-M", &start, &tip]);
    assert!(patch.contains("\"b/caf\\303\\251.txt\"") && patch.contains("--- old comment") && patch.contains("+++ new line"), "{patch}");

    let parsed = changes::parse_patch(&patch);
    let from_patch: Vec<FileChange> = parsed.iter().map(|(f, _)| f.clone()).collect();
    let mut want = vec![
        row("a b/c.txt", None, FileStatus::Modified, Some((1, 1)), false),
        row("a.sql", None, FileStatus::Modified, Some((1, 1)), false),
        row("café.txt", None, FileStatus::Added, Some((1, 0)), false),
        row("gone.txt", None, FileStatus::Deleted, Some((0, 2)), false),
        row("logo.png", None, FileStatus::Added, None, false),
        row("new name.txt", Some("old name.txt"), FileStatus::Renamed, Some((1, 1)), false),
        row("run.sh", None, FileStatus::Modified, Some((0, 0)), false),
    ];
    let mut got = rows(&from_patch);
    got.sort_by(|a, b| a.0.cmp(&b.0));
    want.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got, want, "no .gizai-tmp/");
    // the same as local git says
    let mut local = rows(&changes::between(&repo, &start, &tip).unwrap());
    local.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got, local);
    // each file's own part of the diff
    for (f, part) in &parsed {
        assert!(part.starts_with("diff --git ") && patch.contains(part.as_str()), "{}", f.path);
        assert_eq!(part.matches("diff --git ").count(), 1, "{}", f.path);
    }
    let sql = &parsed.iter().find(|(f, _)| f.path == "a.sql").unwrap().1;
    assert!(sql.contains("--- old comment") && sql.contains("+++ new line"), "{sql}");
    assert!(changes::parse_patch("").is_empty());
}

// ---- contract C1 ----

#[test]
fn the_types_reach_the_screen_as_contract_c1_has_them() {
    let f = FileChange { path: "b.txt".into(), old_path: Some("a.txt".into()), status: FileStatus::Renamed, additions: Some(2),
                         deletions: Some(1), binary: false, uncommitted: true };
    let bin = FileChange { path: "a.png".into(), old_path: None, status: FileStatus::Added, additions: None, deletions: None, binary: true,
                           uncommitted: false };
    let c = CardChanges::new(ChangeSource::Worktree, Some("abc123".into()), None, "origin/main", vec![f, bin], None);
    assert_eq!(serde_json::to_value(&c).unwrap(), json!({
        "source": "worktree", "from": "abc123", "to": null, "baseLabel": "origin/main", "additions": 2, "deletions": 1,
        "files": [
            {"path": "a.png", "oldPath": null, "status": "added", "additions": null, "deletions": null, "binary": true, "uncommitted": false},
            {"path": "b.txt", "oldPath": "a.txt", "status": "renamed", "additions": 2, "deletions": 1, "binary": false, "uncommitted": true},
        ],
        "note": null,
    }));
    let none = CardChanges::none("main", "No changes yet: the first run makes this card's branch.");
    assert_eq!(serde_json::to_value(&none).unwrap(), json!({"source": "none", "from": null, "to": null, "baseLabel": "main", "additions": 0,
        "deletions": 0, "files": [], "note": "No changes yet: the first run makes this card's branch."}));
    for (s, name) in [(ChangeSource::Worktree, "worktree"), (ChangeSource::Branch, "branch"), (ChangeSource::Merge, "merge"),
                      (ChangeSource::Pull, "pull"), (ChangeSource::Run, "run"), (ChangeSource::None, "none")] {
        assert_eq!(serde_json::to_value(s).unwrap(), json!(name));
    }
    for (s, name) in [(FileStatus::Added, "added"), (FileStatus::Modified, "modified"), (FileStatus::Deleted, "deleted"),
                      (FileStatus::Renamed, "renamed"), (FileStatus::Copied, "copied"), (FileStatus::Typechange, "typechange")] {
        assert_eq!(serde_json::to_value(s).unwrap(), json!(name));
    }
    let d = FileDiff { path: "a.txt".into(), old_text: Some("a\n".into()), new_text: None, hunks: vec![hunk(1, 1, 0, 0)], patch: None,
                       binary: false, too_large: false, old_size: 2, new_size: 0, crlf: false };
    assert_eq!(serde_json::to_value(&d).unwrap(), json!({"path": "a.txt", "oldText": "a\n", "newText": null,
        "hunks": [{"oldStart": 1, "oldLines": 1, "newStart": 0, "newLines": 0}], "patch": null, "binary": false, "tooLarge": false,
        "oldSize": 2, "newSize": 0, "crlf": false}));
}
