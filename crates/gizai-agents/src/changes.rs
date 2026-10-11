//! A card's changes, from local git (GA-106): which files it changed with their counts, and each file's texts before and
//! after with git's own hunks between them (`git diff -U0`), so the Changes view, its file list and GitHub agree.
//!
//! The app picks where a card's changes come from (`src-tauri/src/changes.rs`), in this order: its worktree against its
//! merge base with main (`of_worktree`, uncommitted and untracked files included), else its branch (`between`), else the
//! range saved at its merge (`saved_range`), else its merge commit on main against its first parent
//! (`find_merge_commit`, `between`), else the pull request's diff from GitHub or Bitbucket (`parse_patch`).
//!
//! Git runs as a command, like everywhere in Gizai, and only reads: never the index, and no optional lock
//! (`GIT_OPTIONAL_LOCKS=0`), so an agent at work in the worktree isn't disturbed. Paths are literal (no wildcards). The
//! one write is at a merge: the card's range in two refs of the repository (`save_range`), which Gizai's pushes never
//! send (they push `refs/heads/<branch>` only). Gizai's temp folder (`.gizai-tmp/`) never shows.
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::Path;
use std::process::Stdio;

use serde::Serialize;

use crate::AgentError;
use crate::worktree::TEMP_DIR;

/// The most of a file's text a side `file_diff` sends (1 MB): a bigger file comes without its texts and hunks.
pub const MAX_TEXT: u64 = 1 << 20;
/// git's own test for a binary file: a NUL byte in its first 8,000 bytes.
const BINARY_PEEK: usize = 8000;
/// Bigger files are binary to git's diff (the default of `core.bigFileThreshold`, 512 MB).
const BIG_FILE: u64 = 512 << 20;
/// Where the ranges of merged cards are kept: `refs/gizai/merged/<identifier>/base` and `/head`.
pub const MERGED_REFS: &str = "refs/gizai/merged";
/// The diff options every count and hunk is made with: no external diff program and no text conversion, so they are
/// git's own counts of the files as they are, whatever the user's git config says.
const PLAIN: [&str; 3] = ["--no-color", "--no-ext-diff", "--no-textconv"];

/// Where a card's changes come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeSource {
    /// Its worktree as it is on disk, uncommitted changes included, against its merge base with main.
    Worktree,
    /// Its branch (its worktree is gone), against its merge base with main.
    Branch,
    /// The range saved at its merge, or its merge commit against the commit's first parent.
    Merge,
    /// GitHub's or Bitbucket's diff of its pull request, without the files' texts.
    Pull,
    /// One run's commits.
    Run,
    /// Nothing to show (see the note).
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// A file that became a link, or the other way round.
    Typechange,
}

/// One changed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// Renamed or copied from.
    pub old_path: Option<String>,
    pub status: FileStatus,
    /// None for a binary file.
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub binary: bool,
    /// Not committed yet (worktree only); an untracked file is added and uncommitted.
    pub uncommitted: bool,
}

/// A card's changed files with their counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardChanges {
    pub source: ChangeSource,
    /// The commit the changes are measured from: the merge base, the saved base, or the merge commit's first parent.
    pub from: Option<String>,
    /// The commit they end at; None for the worktree as it is on disk.
    pub to: Option<String>,
    /// "origin/main", or "main" for a project without a link.
    pub base_label: String,
    pub additions: u64,
    pub deletions: u64,
    /// In path order.
    pub files: Vec<FileChange>,
    /// Why there is nothing to show, or where the changes come from when not the worktree.
    pub note: Option<String>,
}

impl CardChanges {
    /// The changes in `files`, put in path order, with their totals.
    pub fn new(source: ChangeSource, from: Option<String>, to: Option<String>, base_label: &str, mut files: Vec<FileChange>,
               note: Option<String>) -> CardChanges {
        files.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.old_path.cmp(&b.old_path)));
        let additions = files.iter().filter_map(|f| f.additions).sum();
        let deletions = files.iter().filter_map(|f| f.deletions).sum();
        CardChanges { source, from, to, base_label: base_label.to_string(), additions, deletions, files, note }
    }

    /// Nothing to show, and why.
    pub fn none(base_label: &str, note: &str) -> CardChanges {
        CardChanges::new(ChangeSource::None, None, None, base_label, vec![], Some(note.to_string()))
    }
}

/// One hunk as git prints it with -U0: "@@ -oldStart,oldLines +newStart,newLines @@". Lines count from 1; when a side
/// has 0 lines, its start is the line before the change (0 at the top), as in git.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u64,
    pub old_lines: u64,
    pub new_start: u64,
    pub new_lines: u64,
}

/// One changed file: its texts on both sides and git's hunks between them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// None when added, binary or too large.
    pub old_text: Option<String>,
    /// None when deleted, binary or too large.
    pub new_text: Option<String>,
    /// git's hunks between the two texts, in order; empty when binary, too large or from a pull request's diff.
    pub hunks: Vec<DiffHunk>,
    /// Only from a pull request's diff: this file's part of it.
    pub patch: Option<String>,
    pub binary: bool,
    /// Over 1 MB on a side (`MAX_TEXT`).
    pub too_large: bool,
    pub old_size: u64,
    pub new_size: u64,
    /// Its lines end with CRLF.
    pub crlf: bool,
}

// ---- git ----

/// git in `dir`, reading only: literal paths, and no optional lock (git status and diff otherwise refresh the index).
fn git_bytes(dir: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>, AgentError> {
    let mut cmd = crate::os::command("git");
    cmd.arg("-C").arg(dir).args(args).env("GIT_LITERAL_PATHSPECS", "1").env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() }).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AgentError::Git(format!("can't run git: {e}")))?;
    if let (Some(text), Some(mut pipe)) = (input, child.stdin.take()) {
        pipe.write_all(text).map_err(|e| AgentError::Git(format!("can't talk to git: {e}")))?;
    }
    let out = child.wait_with_output().map_err(|e| AgentError::Git(format!("can't run git: {e}")))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(AgentError::Git(String::from_utf8_lossy(&out.stderr).trim().to_string()))
    }
}

fn git_text(dir: &Path, args: &[&str]) -> Result<String, AgentError> {
    git_bytes(dir, args, None).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Whether `id` is a commit id (4 to 64 hex digits): never an option, a range or a ref for git.
pub fn is_commit_id(id: &str) -> bool {
    (4..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn check_id(id: &str) -> Result<(), AgentError> {
    if is_commit_id(id) { Ok(()) } else { Err(AgentError::Git(format!("{id:?} is not a commit id"))) }
}

/// The full id of commit `rev` (a ref, or a commit id short or full) in `repo`, when it has it.
pub fn commit_of(repo: &Path, rev: &str) -> Option<String> {
    if rev.is_empty() || rev.starts_with('-') {
        return None;
    }
    let id = git_text(repo, &["rev-parse", "--verify", "--quiet", &format!("{rev}^{{commit}}")]).ok()?;
    let id = id.trim();
    is_commit_id(id).then(|| id.to_string())
}

/// Where commits `a` and `b` meet (`git merge-base`), when they do.
pub fn merge_base(repo: &Path, a: &str, b: &str) -> Option<String> {
    if [a, b].iter().any(|r| r.is_empty() || r.starts_with('-')) {
        return None;
    }
    let id = git_text(repo, &["merge-base", a, b]).ok()?;
    let id = id.trim();
    is_commit_id(id).then(|| id.to_string())
}

/// Whether `path` is in Gizai's temp folder (`.gizai-tmp/`), which never shows.
fn in_temp(path: &str) -> bool {
    path.strip_prefix(TEMP_DIR).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

// ---- the file list ----

/// The files of `git diff --raw --numstat -z` output, in git's order: each with its status from the raw part and its
/// counts from the numstat part (None and binary for git's "-"). Gizai's temp folder is left out.
pub fn parse_diff(out: &str) -> Vec<FileChange> {
    let tokens: Vec<&str> = out.split('\0').collect();
    let mut files: Vec<FileChange> = vec![];
    let mut counts: HashMap<&str, (Option<u64>, Option<u64>)> = HashMap::new();
    let mut i = 0;
    let next = |i: &mut usize| {
        let t = tokens.get(*i).copied().unwrap_or_default();
        *i += 1;
        t
    };
    while i < tokens.len() {
        let t = next(&mut i);
        if t.is_empty() {
            continue;
        }
        if let Some(raw) = t.strip_prefix(':') {
            // ":<old mode> <new mode> <old id> <new id> <status>", then its path, or both paths of a rename or copy
            let status = raw.split(' ').nth(4).unwrap_or("M");
            let letter = status.chars().next().unwrap_or('M');
            let first = next(&mut i);
            let (old_path, path) = if matches!(letter, 'R' | 'C') { (Some(first.to_string()), next(&mut i)) } else { (None, first) };
            let status = match letter {
                'A' => FileStatus::Added,
                'D' => FileStatus::Deleted,
                'R' => FileStatus::Renamed,
                'C' => FileStatus::Copied,
                'T' => FileStatus::Typechange,
                _ => FileStatus::Modified,
            };
            files.push(FileChange { path: path.to_string(), old_path, status, additions: Some(0), deletions: Some(0), binary: false, uncommitted: false });
        } else {
            // "<added>\t<deleted>\t<path>", or "<added>\t<deleted>\t" and then both paths of a rename or copy
            let mut parts = t.splitn(3, '\t');
            let (added, deleted, rest) = (parts.next().unwrap_or_default(), parts.next().unwrap_or_default(), parts.next());
            let path = match rest {
                None => continue,
                Some("") => {
                    next(&mut i);
                    next(&mut i)
                }
                Some(p) => p,
            };
            counts.insert(path, (added.parse().ok(), deleted.parse().ok()));
        }
    }
    for f in &mut files {
        if let Some((added, deleted)) = counts.get(f.path.as_str()) {
            f.binary = added.is_none() || deleted.is_none();
            (f.additions, f.deletions) = if f.binary { (None, None) } else { (*added, *deleted) };
        }
    }
    files.retain(|f| !in_temp(&f.path) && !f.old_path.as_deref().is_some_and(in_temp));
    files
}

/// The changes between commits `from` and `to`: a branch against its merge base, the range saved at a merge, a merge
/// commit against its first parent, or a run's start and end. Renames count as renames (`-M`).
pub fn between(repo: &Path, from: &str, to: &str) -> Result<Vec<FileChange>, AgentError> {
    check_id(from)?;
    check_id(to)?;
    let mut args = vec!["diff", "--raw", "--numstat", "-z", "-M"];
    args.extend(PLAIN);
    args.extend([from, to, "--"]);
    Ok(parse_diff(&git_text(repo, &args)?))
}

/// The changes of the worktree at `wt` since commit `from` (its merge base with main): its commits and what isn't
/// committed yet, the files git doesn't track yet (and doesn't ignore) as added. Each file with changes that aren't
/// committed yet says so.
pub fn of_worktree(wt: &Path, from: &str) -> Result<Vec<FileChange>, AgentError> {
    check_id(from)?;
    let mut args = vec!["diff", "--raw", "--numstat", "-z", "-M"];
    args.extend(PLAIN);
    args.extend([from, "--"]);
    let mut files = parse_diff(&git_text(wt, &args)?);
    // what differs from the worktree's own latest commit isn't committed yet
    let touched: HashSet<String> = git_text(wt, &["diff", "--name-only", "-z", "--no-renames", "--no-ext-diff", "HEAD", "--"])?
        .split('\0').filter(|p| !p.is_empty()).map(String::from).collect();
    for f in &mut files {
        f.uncommitted = touched.contains(&f.path) || f.old_path.as_ref().is_some_and(|o| touched.contains(o));
    }
    let listed: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    for path in untracked(wt)? {
        // a folder that is a repository of its own ("dir/"); a file the diff has already (taken out of git, still here)
        if path.ends_with('/') || in_temp(&path) || listed.contains(&path) {
            continue;
        }
        let lines = count_new(&wt.join(&path));
        files.push(FileChange { path, old_path: None, status: FileStatus::Added, additions: lines, deletions: lines.map(|_| 0),
                                binary: lines.is_none(), uncommitted: true });
    }
    Ok(files)
}

/// The files in the worktree at `wt` that git doesn't track and doesn't ignore.
fn untracked(wt: &Path) -> Result<Vec<String>, AgentError> {
    Ok(git_text(wt, &["ls-files", "--others", "--exclude-standard", "-z"])?.split('\0').filter(|p| !p.is_empty()).map(String::from).collect())
}

/// What git's numstat counts for a new file on disk: its lines, or None when it is binary (a NUL byte in its first
/// 8,000 bytes, or bigger than 512 MB) or can't be read. A link counts as its target, one line, as git keeps it.
fn count_new(path: &Path) -> Option<u64> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        return Some(1);
    }
    if !meta.is_file() || meta.len() > BIG_FILE {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 64 * 1024];
    let (mut lines, mut seen, mut last) = (0u64, 0usize, b'\n');
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        if seen < BINARY_PEEK && chunk[..n.min(BINARY_PEEK - seen)].contains(&0) {
            return None;
        }
        seen += n;
        lines += chunk.iter().filter(|b| **b == b'\n').count() as u64;
        last = chunk[n - 1];
    }
    // a last line without its newline still counts
    Some(if last == b'\n' { lines } else { lines + 1 })
}

/// How many lines `bytes` has, as git counts them (a last line without its newline too).
fn line_count(bytes: &[u8]) -> u64 {
    let n = bytes.iter().filter(|b| **b == b'\n').count() as u64;
    if bytes.last().is_some_and(|b| *b != b'\n') { n + 1 } else { n }
}

// ---- one file ----

/// One side of a file.
struct Side {
    /// None when it is bigger than `MAX_TEXT`.
    bytes: Option<Vec<u8>>,
    size: u64,
    /// A NUL byte in its first 8,000 bytes: binary to git.
    nul: bool,
    /// A submodule's commit, not a file of this repository.
    submodule: bool,
}

impl Side {
    /// A file's whole content.
    fn of(bytes: Vec<u8>) -> Side {
        Side { nul: has_nul(&bytes), size: bytes.len() as u64, bytes: Some(bytes), submodule: false }
    }
}

/// A path of the repository as git gives it: relative, with `/`, and never out of the folder (`..`) or into `.git`.
fn check_path(path: &str) -> Result<(), AgentError> {
    let parts: Vec<&str> = if cfg!(windows) { path.split(['/', '\\']).collect() } else { path.split('/').collect() };
    let drive = cfg!(windows) && path.as_bytes().get(1) == Some(&b':');
    if path.is_empty() || path.contains('\0') || path.starts_with('/') || drive
        || parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.eq_ignore_ascii_case(".git"))
    {
        return Err(AgentError::Git(format!("{path:?} isn't a path in the repository")));
    }
    Ok(())
}

/// `path` at commit `rev`: None when it isn't there (or is a folder). A submodule reads as git shows it in a diff.
fn blob_side(repo: &Path, rev: &str, path: &str) -> Result<Option<Side>, AgentError> {
    let listed = git_text(repo, &["ls-tree", "-z", "-l", "--full-tree", rev, "--", path])?;
    // "<mode> <type> <id> <size>\t<path>"
    let Some((info, _)) = listed.split('\0').filter_map(|e| e.split_once('\t')).find(|(_, p)| *p == path) else { return Ok(None) };
    let fields: Vec<&str> = info.split_whitespace().collect();
    let (Some(kind), Some(id)) = (fields.get(1).copied(), fields.get(2).copied()) else { return Ok(None) };
    match kind {
        "blob" => {
            let size: u64 = fields.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            if size > MAX_TEXT {
                return Ok(Some(Side { bytes: None, size, nul: peek_blob(repo, id), submodule: false }));
            }
            let bytes = git_bytes(repo, &["cat-file", "blob", id], None)?;
            Ok(Some(Side::of(bytes)))
        }
        "commit" => {
            let text = format!("Subproject commit {id}\n").into_bytes();
            Ok(Some(Side { submodule: true, ..Side::of(text) }))
        }
        _ => Ok(None),
    }
}

/// `path` in the worktree at `wt` as it is on disk: None when it isn't there (or is a folder). A link reads as its
/// target, as git keeps it; a path through a linked folder that leads out of the worktree is never read.
fn disk_side(wt: &Path, path: &str) -> Result<Option<Side>, AgentError> {
    let full = wt.join(path);
    let inside = match (full.parent().map(Path::canonicalize), wt.canonicalize()) {
        (Some(Ok(dir)), Ok(root)) => dir.starts_with(&root),
        _ => false,
    };
    if !inside {
        return Ok(None);
    }
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full)?.to_string_lossy().replace('\\', "/").into_bytes();
        return Ok(Some(Side::of(target)));
    }
    if !meta.is_file() {
        return Ok(None);
    }
    if meta.len() > MAX_TEXT {
        let mut head = vec![0u8; BINARY_PEEK];
        let got = std::fs::File::open(&full).and_then(|mut f| read_up_to(&mut f, &mut head)).unwrap_or(0);
        return Ok(Some(Side { bytes: None, size: meta.len(), nul: has_nul(&head[..got]), submodule: false }));
    }
    let bytes = std::fs::read(&full)?;
    Ok(Some(Side::of(bytes)))
}

fn has_nul(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PEEK)].contains(&0)
}

/// Reads until `buf` is full or the stream ends; returns how much it read.
fn read_up_to(from: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match from.read(&mut buf[got..])? {
            0 => break,
            n => got += n,
        }
    }
    Ok(got)
}

/// Whether the blob `id` (too big to read whole) has a NUL byte in its first 8,000 bytes.
fn peek_blob(repo: &Path, id: &str) -> bool {
    let child = crate::os::command("git").arg("-C").arg(repo).args(["cat-file", "blob", id])
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let Ok(mut child) = child else { return false };
    let mut head = vec![0u8; BINARY_PEEK];
    let got = child.stdout.as_mut().and_then(|out| read_up_to(out, &mut head).ok()).unwrap_or(0);
    let _ = child.kill();
    let _ = child.wait();
    has_nul(&head[..got])
}

/// git's hunks in `git diff -U0` output (its `@@ -a,b +c,d @@` lines), and whether git called a file in it binary. When
/// it holds two or more files (a rename git no longer sees as one, or a file that became a link), they become one hunk
/// over both sides.
pub fn parse_hunks(out: &str) -> (Vec<DiffHunk>, bool) {
    let (mut hunks, mut binary, mut files) = (vec![], false, 0);
    for line in out.lines() {
        if line.starts_with("diff --git ") {
            files += 1;
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            binary = true;
        } else if let Some(h) = hunk_header(line) {
            hunks.push(h);
        }
    }
    if files > 1 && !hunks.is_empty() {
        let old_lines: u64 = hunks.iter().map(|h| h.old_lines).sum();
        let new_lines: u64 = hunks.iter().map(|h| h.new_lines).sum();
        hunks = vec![DiffHunk { old_start: u64::from(old_lines > 0), old_lines, new_start: u64::from(new_lines > 0), new_lines }];
    }
    (hunks, binary)
}

/// "@@ -a[,b] +c[,d] @@ …": a count left out is 1.
fn hunk_header(line: &str) -> Option<DiffHunk> {
    let (ranges, _) = line.strip_prefix("@@ -")?.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let range = |r: &str| -> Option<(u64, u64)> {
        match r.split_once(',') {
            Some((start, n)) => Some((start.parse().ok()?, n.parse().ok()?)),
            None => Some((r.parse().ok()?, 1)),
        }
    };
    let ((old_start, old_lines), (new_start, new_lines)) = (range(old)?, range(new)?);
    Some(DiffHunk { old_start, old_lines, new_start, new_lines })
}

/// The file `path` (from `old_path` when it was renamed or copied) between commit `from` and commit `to`, or the
/// worktree `wt` as it is on disk (`to` None): both texts, at most 1 MB a side, and git's hunks between them
/// (`git diff -U0`, the same diff as the file list's counts). `repo` is any checkout of the repository: they share their
/// commits.
pub fn file_diff(repo: &Path, wt: Option<&Path>, path: &str, old_path: Option<&str>, from: &str, to: Option<&str>)
    -> Result<FileDiff, AgentError> {
    check_id(from)?;
    if let Some(t) = to {
        check_id(t)?;
    }
    check_path(path)?;
    let old_path = old_path.filter(|o| !o.is_empty() && *o != path);
    if let Some(o) = old_path {
        check_path(o)?;
    }
    let before = old_path.unwrap_or(path);
    let old = blob_side(repo, from, before)?;
    let (new, dir) = match (to, wt) {
        (Some(t), _) => (blob_side(repo, t, path)?, repo),
        (None, Some(wt)) => (disk_side(wt, path)?, wt),
        (None, None) => return Err(AgentError::Git("the worktree is gone".into())),
    };
    let size = |s: &Option<Side>| s.as_ref().map_or(0, |s| s.size);
    let (old_size, new_size) = (size(&old), size(&new));
    let too_large = old_size > MAX_TEXT || new_size > MAX_TEXT;
    let mut binary = old.as_ref().is_some_and(|s| s.nul) || new.as_ref().is_some_and(|s| s.nul);
    let mut hunks = vec![];
    if !too_large && (old.is_some() || new.is_some()) {
        let mut args = vec!["diff", "-U0"];
        args.extend(PLAIN);
        let (from_blob, to_blob);
        match (to, &old, &new) {
            // both sides in commits: the two files themselves, whatever git's rename detection says now
            (Some(t), Some(o), Some(n)) if !o.submodule && !n.submodule => {
                from_blob = format!("{from}:{before}");
                to_blob = format!("{t}:{path}");
                args.extend([from_blob.as_str(), to_blob.as_str()]);
            }
            (Some(t), _, _) => args.extend([from, t, "--", before, path]),
            (None, _, _) => args.extend(["-M", from, "--", before, path]),
        }
        let (got, git_binary) = parse_hunks(&git_text(dir, &args)?);
        binary |= git_binary;
        hunks = got;
        // a file git doesn't track yet isn't in its diff: it is all new
        if to.is_none() && old.is_none() && hunks.is_empty() && !binary
            && let Some(lines) = new.as_ref().and_then(|s| s.bytes.as_deref()).map(line_count).filter(|n| *n > 0)
        {
            hunks = vec![DiffHunk { old_start: 0, old_lines: 0, new_start: 1, new_lines: lines }];
        }
    }
    let text = |s: Option<Side>| (!binary && !too_large).then(|| s.and_then(|s| s.bytes)).flatten()
        .map(|b| String::from_utf8_lossy(&b).into_owned());
    let (old_text, new_text) = (text(old), text(new));
    let crlf = new_text.as_deref().or(old_text.as_deref()).is_some_and(|t| t.contains("\r\n"));
    if binary || too_large {
        hunks.clear();
    }
    Ok(FileDiff { path: path.to_string(), old_text, new_text, hunks, patch: None, binary, too_large, old_size, new_size, crlf })
}

// ---- a merged card ----

/// The two refs that keep a merged card's range: (base, head).
pub fn merged_refs(identifier: &str) -> (String, String) {
    (format!("{MERGED_REFS}/{identifier}/base"), format!("{MERGED_REFS}/{identifier}/head"))
}

/// A card identifier that is safe in a ref name ("GA-12").
fn ref_safe(identifier: &str) -> bool {
    !identifier.is_empty() && !identifier.starts_with('-') && identifier.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The range saved at the card's merge (`save_range`), as commit ids (base, head), when this repository has it.
pub fn saved_range(repo: &Path, identifier: &str) -> Option<(String, String)> {
    if !ref_safe(identifier) {
        return None;
    }
    let (base, head) = merged_refs(identifier);
    Some((commit_of(repo, &base)?, commit_of(repo, &head)?))
}

/// Saves the card's range in its two refs (`merged_refs`), both at once, in place of any saved before. Only these two
/// refs change, and Gizai's pushes send only a card's branch, so they stay on this computer; they keep the card's
/// commits from git's clean-up once its branch is gone.
pub fn save_range(repo: &Path, identifier: &str, base: &str, head: &str) -> Result<(), AgentError> {
    if !ref_safe(identifier) {
        return Err(AgentError::Git(format!("{identifier:?} can't name a ref")));
    }
    check_id(base)?;
    check_id(head)?;
    let (base_ref, head_ref) = merged_refs(identifier);
    let todo = format!("update {base_ref} {base}\nupdate {head_ref} {head}\n");
    git_bytes(repo, &["update-ref", "--stdin"], Some(todo.as_bytes())).map(|_| ())
}

/// The card's range to save at its merge: (base, head), `head` being the card's latest commit that its pull request had.
/// - With its merge commit `merge` (an id, short or full, as GitHub or Bitbucket gives it) in this repository, and not
///   `head` itself: base is where `head` meets the merge commit's first parent. After a merge commit or a squash, that
///   parent is main just before the merge. After a rebase it is the rebased commit before the last one, which `head`
///   still meets where the card started, because rebased commits are copies and not ancestors of `head`.
/// - Else (offline, no merge commit, a fast-forward): where `head` meets main as last fetched (`mains`, tried in order),
///   the first that isn't `head` itself.
///
/// None when every way gives `head` itself (main had all of it already) or nothing: nothing is saved then.
pub fn range_at_merge(repo: &Path, head: &str, merge: Option<&str>, mains: &[&str]) -> Option<(String, String)> {
    let head = commit_of(repo, head)?;
    let by_merge = merge.filter(|m| is_commit_id(m)).and_then(|m| commit_of(repo, m)).filter(|m| *m != head)
        .and_then(|m| commit_of(repo, &format!("{m}^1")))
        .and_then(|parent| merge_base(repo, &head, &parent))
        .filter(|base| *base != head);
    let base = by_merge.or_else(|| mains.iter().filter_map(|m| merge_base(repo, &head, m)).find(|base| *base != head))?;
    Some((base, head))
}

/// The commit that merged pull request `number` (from `branch`) into main (`main`, a ref or a commit), found by the
/// message GitHub or Bitbucket gives it, on main's own line (first parents): a merge commit ("Merge pull request #12
/// from owner/branch", "Merged in branch (pull request #12)") or a squash ("Its title (#12)"). Local only.
pub fn find_merge_commit(repo: &Path, main: &str, number: u64, branch: &str) -> Option<String> {
    if main.is_empty() || main.starts_with('-') {
        return None;
    }
    let (pull, squash) = (format!("--grep=pull request #{number}"), format!("--grep=(#{number})"));
    let out = git_text(repo, &["log", "--first-parent", "--fixed-strings", &pull, &squash, "--format=%H%x1f%s", "-n", "100", main, "--"]).ok()?;
    let (github, bitbucket, squashed) =
        (format!("Merge pull request #{number} from "), format!("Merged in {branch} (pull request #{number})"), format!("(#{number})"));
    out.lines().filter_map(|l| l.split_once('\x1f')).find(|(_, subject)| {
        let subject = subject.trim();
        subject.strip_prefix(&github).is_some_and(|from| from == branch || from.ends_with(&format!("/{branch}")) || from.ends_with(&format!(":{branch}")))
            || subject.starts_with(&bitbucket)
            || subject.ends_with(&squashed)
    }).map(|(id, _)| id.to_string()).filter(|id| is_commit_id(id))
}

// ---- a pull request's diff ----

/// A pull request's diff (`gh pr diff`, or Bitbucket's), split per file: each file with its status and counts, and its
/// part of the diff. The files' texts aren't in it.
pub fn parse_patch(patch: &str) -> Vec<(FileChange, String)> {
    let mut sections: Vec<String> = vec![];
    for line in patch.split_inclusive('\n') {
        if line.starts_with("diff --git ") || sections.is_empty() {
            sections.push(String::new());
        }
        if let Some(s) = sections.last_mut() {
            s.push_str(line);
        }
    }
    sections.into_iter().filter(|s| s.starts_with("diff --git ")).filter_map(|s| Some((file_of_section(&s)?, s)))
        .filter(|(f, _)| !in_temp(&f.path)).collect()
}

/// One file's part of a diff, as a `FileChange`.
fn file_of_section(section: &str) -> Option<FileChange> {
    let mut lines = section.lines();
    let first = lines.next()?.strip_prefix("diff --git ")?;
    let (mut old, mut new) = (None::<String>, None::<String>);
    let (mut added, mut deleted, mut renamed, mut copied, mut binary) = (false, false, false, false, false);
    let (mut old_mode, mut new_mode) = (None::<String>, None::<String>);
    let (mut plus, mut minus, mut in_hunk) = (0u64, 0u64, false);
    for line in lines {
        if in_hunk {
            match line.as_bytes().first() {
                Some(b'+') => plus += 1,
                Some(b'-') => minus += 1,
                _ => {}
            }
            continue;
        }
        if line.starts_with("@@ ") {
            in_hunk = true;
        } else if let Some(m) = line.strip_prefix("new file mode ") {
            added = true;
            new_mode = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("deleted file mode ") {
            deleted = true;
            old_mode = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("old mode ") {
            old_mode = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("new mode ") {
            new_mode = Some(m.to_string());
        } else if let Some(p) = line.strip_prefix("rename from ") {
            renamed = true;
            old = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ") {
            new = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("copy from ") {
            copied = true;
            old = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("copy to ") {
            new = Some(unquote(p));
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            binary = true;
        } else if let Some(p) = line.strip_prefix("--- ") && old.is_none() && p != "/dev/null" {
            old = Some(strip_side(p, "a/"));
        } else if let Some(p) = line.strip_prefix("+++ ") && new.is_none() && p != "/dev/null" {
            new = Some(strip_side(p, "b/"));
        }
    }
    let (from_header, to_header) = header_paths(first);
    let path = new.clone().or_else(|| to_header.clone()).or_else(|| old.clone())?;
    let old_path = (renamed || copied).then(|| old.clone().or(from_header)).flatten().filter(|o| *o != path);
    let kind = |m: &Option<String>| m.as_deref().map(|m| m.chars().take(2).collect::<String>());
    let status = if added {
        FileStatus::Added
    } else if deleted {
        FileStatus::Deleted
    } else if renamed {
        FileStatus::Renamed
    } else if copied {
        FileStatus::Copied
    } else if old_mode.is_some() && new_mode.is_some() && kind(&old_mode) != kind(&new_mode) {
        FileStatus::Typechange
    } else {
        FileStatus::Modified
    };
    Some(FileChange { path, old_path, status, additions: (!binary).then_some(plus), deletions: (!binary).then_some(minus), binary,
                      uncommitted: false })
}

/// A path on a "---" or "+++" line: without its side's prefix ("a/", "b/") and the tab git adds after a name with a
/// space.
fn strip_side(p: &str, side: &str) -> String {
    let p = unquote(p.strip_suffix('\t').unwrap_or(p));
    p.strip_prefix(side).map(String::from).unwrap_or(p)
}

/// Both paths of a "diff --git a/x b/y" line. Without quotes the line is ambiguous when a path holds " b/": then both
/// paths are taken to be the same, as they are unless the file was renamed (which says so on lines of its own).
fn header_paths(rest: &str) -> (Option<String>, Option<String>) {
    if rest.starts_with('"') {
        let Some((first, after)) = split_quoted(rest) else { return (None, None) };
        let second = after.trim_start();
        let second = if second.starts_with('"') { split_quoted(second).map(|(s, _)| s) } else { Some(second.to_string()) };
        return (first.strip_prefix("a/").map(String::from), second.and_then(|s| s.strip_prefix("b/").map(String::from)));
    }
    // "a/<p> b/<p>"
    let n = rest.len();
    if n > 5 && (n - 5).is_multiple_of(2) {
        let half = (n - 5) / 2;
        if let (Some(a), Some(b)) = (rest.get(..2 + half), rest.get(2 + half + 3..))
            && rest.get(2 + half..2 + half + 3) == Some(" b/") && a.strip_prefix("a/") == Some(b)
        {
            return (Some(b.to_string()), Some(b.to_string()));
        }
    }
    match rest.split_once(" b/") {
        Some((a, b)) => (a.strip_prefix("a/").map(String::from), Some(b.to_string())),
        None => (None, None),
    }
}

/// The quoted name at the start of `s` and what follows it.
fn split_quoted(s: &str) -> Option<(String, &str)> {
    let body = s.strip_prefix('"')?;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        match c {
            '\\' if !escaped => escaped = true,
            '"' if !escaped => return Some((unquote(&s[..i + 2]), &body[i + 1..])),
            _ => escaped = false,
        }
    }
    None
}

/// A name as git writes it in a diff: as is, or in quotes with C escapes (`\"`, `\\`, `\t`, `\n`, octal bytes such as
/// `\303\251`) when it holds special or non-ASCII characters.
fn unquote(s: &str) -> String {
    let Some(body) = s.strip_prefix('"').and_then(|b| b.strip_suffix('"')) else { return s.to_string() };
    let (bytes, mut out, mut i) = (body.as_bytes(), Vec::new(), 0);
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let c = bytes[i + 1];
        i += 2;
        match c {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'0'..=b'7' => {
                let mut v = u32::from(c - b'0');
                for _ in 0..2 {
                    match bytes.get(i) {
                        Some(d @ b'0'..=b'7') => {
                            v = v * 8 + u32::from(d - b'0');
                            i += 1;
                        }
                        _ => break,
                    }
                }
                out.push(v as u8);
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
