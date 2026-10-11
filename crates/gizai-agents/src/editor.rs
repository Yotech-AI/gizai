//! What is inside a folder open in the Editor (GA-107): one folder level at a time, a file to read and to save, new
//! files and folders, rename and delete, Go to file, search, git status and the files' versions; and Open in VS Code,
//! Zed or PhpStorm. The app first checks that the Editor may open the folder at all (`gizai_core::editor`); every path
//! in it then goes through the path gate (`gate`).
//! - A path is relative to the folder, with `/` between folders: never a full path, never `..`, never any `.git` part,
//!   and never through a link that leads out of the folder or into a `.git`.
//! - Read-only: everything in a card's worktree while an agent works on it (`Folder::read_only`), the files in a folder
//!   git ignores (node_modules/, vendor/…), files over 1 MB, binary files, files that aren't UTF-8 and files that are
//!   read-only on disk. Binary files and files over 5 MB come without their text.
//! - Saving writes the new text to a file next to the old one and renames it over the old one, with the old one's
//!   permissions, line endings (CRLF) and byte order mark. It refuses when the file changed on disk since it was read:
//!   its version (size, times) isn't the one the screen read any more.
//! - git runs as a command, as everywhere in Gizai, without optional locks: the Editor's look at git status never keeps
//!   an agent's git in the same worktree from the index. Pathspecs are literal, so `app/[id]` is that folder.
use std::collections::{BTreeMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{os, worktree};

/// Files over this size are read-only in the Editor.
pub const READ_ONLY_OVER: u64 = 1024 * 1024;
/// Files over this size come without their text.
pub const TEXT_UP_TO: u64 = 5 * 1024 * 1024;
/// Search stops at this many hits (`SearchResult::truncated`)…
pub const MAX_HITS: usize = 2000;
/// …or after this long.
pub const SEARCH_LIMIT: Duration = Duration::from_secs(10);
/// Go to file lists at most this many files.
pub const MAX_FILES: usize = 50_000;
/// `FolderGit::files` holds at most this many files (`uncommitted` counts them all).
pub const MAX_CHANGED: usize = 10_000;
/// The line in the repository's `info/exclude` that keeps PhpStorm's project files (`.idea/`) out of git status.
pub const IDEA_EXCLUDE: &str = "/.idea/";

/// `OpenedFile::read_only_why` for a file over 5 MB…
pub const OVER_5_MB: &str = "Over 5 MB";
/// …over 1 MB…
pub const OVER_1_MB: &str = "Over 1 MB";
/// …a binary file…
pub const BINARY: &str = "A binary file";
/// …a file in a folder git ignores…
pub const IGNORED: &str = "Ignored by git";
/// …a file that isn't UTF-8…
pub const NOT_UTF8: &str = "Not UTF-8 text";
/// …and a file that is read-only on disk. A card's worktree while an agent works on it says who (`Folder::read_only`).
pub const READ_ONLY_ON_DISK: &str = "Read-only on disk";

/// How much of a file is looked at for a NUL byte: git's own test for a binary file.
const SNIFF: usize = 8000;
/// A search hit's line is cut after this many characters.
const HIT_TEXT: usize = 1000;
/// How long one of the Editor's git commands may take (search has its own limit).
const GIT_LIMIT: Duration = Duration::from_secs(30);
/// UTF-8's byte order mark.
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// The path gate's refusals.
const FULL_PATH: &str = "Give a path inside the folder, like src/main.rs, not a full path";
const UP: &str = "A path can't go up out of the folder (..)";
const GIT: &str = "The Editor never opens or changes git's own files (.git)";

/// A folder open in the Editor (`gizai_core::editor::EditorFolder`).
#[derive(Debug, Clone, Copy)]
pub struct Folder<'a> {
    /// Where it is: its canonical path (`EditorFolder::id`).
    pub dir: &'a Path,
    /// Why nothing in it may change now ("Backend Agent is working on GA-12"); None when it may.
    pub read_only: Option<&'a str>,
}

/// A file's git status, as the tree and the tabs show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GitMark {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// One file or folder in a folder level (`list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    pub name: String,
    /// Its path in the opened folder, with `/`.
    pub path: String,
    pub dir: bool,
    /// In bytes; 0 for a folder.
    pub size: u64,
    /// git ignores it: greyed; read-only inside an ignored folder.
    pub ignored: bool,
    /// For a folder: "modified" when something inside it changed.
    pub git: Option<GitMark>,
}

/// A file as the Editor opens it (`read`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedFile {
    pub path: String,
    /// With `\n` for every line ending and without the byte order mark; None when binary or over 5 MB.
    pub text: Option<String>,
    pub binary: bool,
    /// Over 5 MB.
    pub too_large: bool,
    pub read_only: bool,
    /// "Over 1 MB", "Ignored by git", "Backend Agent is working on GA-12".
    pub read_only_why: Option<String>,
    /// Its lines end in CRLF (its first line does): saving keeps that.
    pub crlf: bool,
    /// It starts with a byte order mark: saving keeps that.
    pub bom: bool,
    /// What `write` checks.
    pub version: String,
}

/// One line search found (`search`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    /// From 1.
    pub line: u32,
    /// Where the first match on the line starts, from 1, in UTF-16 code units (as the screen counts).
    pub column: u32,
    /// The line, cut after 1,000 characters.
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// Search stopped before the end: at 2,000 hits, or after 10 seconds.
    pub truncated: bool,
}

/// What git status says about a folder (`status`): local, nothing fetched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderGit {
    /// None when it isn't a git repository, or its HEAD is detached.
    pub branch: Option<String>,
    /// Each changed file (untracked ones one by one) by its path in the folder.
    pub files: BTreeMap<String, GitMark>,
    /// How many files changed.
    pub uncommitted: usize,
}

/// How `search` matches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Find {
    pub case_sensitive: bool,
    /// Only whole words.
    pub whole_word: bool,
    /// The query is a regular expression (Perl's, else POSIX's extended ones when git has no PCRE).
    pub regex: bool,
}

/// An editor Gizai can open a folder in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppId {
    VsCode,
    Zed,
    PhpStorm,
}

/// An editor found on this computer (`apps`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorApp {
    pub id: AppId,
    pub name: String,
}

impl AppId {
    /// In the order the Open in menu lists them.
    pub const ALL: [AppId; 3] = [AppId::VsCode, AppId::Zed, AppId::PhpStorm];

    /// The editor whose id the screen sent ("vscode", "zed", "phpstorm"); None for another.
    pub fn parse(id: &str) -> Option<AppId> {
        AppId::ALL.into_iter().find(|a| a.id() == id)
    }

    pub fn id(self) -> &'static str {
        match self {
            AppId::VsCode => "vscode",
            AppId::Zed => "zed",
            AppId::PhpStorm => "phpstorm",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AppId::VsCode => "VS Code",
            AppId::Zed => "Zed",
            AppId::PhpStorm => "PhpStorm",
        }
    }

    /// The names its command goes by, the usual one first: `code`; `zed`, and `zeditor` or `zedit` as Linux
    /// distributions name Zed's; `phpstorm` (JetBrains Toolbox's script, and packages), `phpstorm.sh` (in PhpStorm's own
    /// bin folder) and `phpstorm64` (Windows' `phpstorm64.exe`). On Windows each with `.cmd` or `.exe` (`os::find_in`).
    pub fn programs(self) -> &'static [&'static str] {
        match self {
            AppId::VsCode => &["code"],
            AppId::Zed => &["zed", "zeditor", "zedit"],
            AppId::PhpStorm => &["phpstorm", "phpstorm.sh", "phpstorm64"],
        }
    }
}

// ---- the path gate ----

/// The parts of `path`, a path in a folder with `/` between folders, or why the Editor refuses it: a full path, `..`,
/// any `.git` part (in any case, also as `.git.`), or a control character. `.` and empty parts drop out, so "" and "."
/// are the folder itself. `windows`: with Windows' rules too, whatever system this is (so they can be tested
/// anywhere): `\` separates folders as well, a drive (`C:`) or `:` anywhere is refused, a part of only dots and spaces
/// (`.. `, which Windows reads as `..`) too, and so are the device names (CON, NUL, COM1, LPT1…).
pub fn split_path(path: &str, windows: bool) -> Result<Vec<&str>, String> {
    if path.chars().any(char::is_control) {
        return Err("A path can't have a control character, such as a line break, in it".into());
    }
    let seps: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    let drive = windows && path.len() >= 2 && path.as_bytes()[0].is_ascii_alphabetic() && path.as_bytes()[1] == b':';
    if path.starts_with(seps) || drive {
        return Err(FULL_PATH.into());
    }
    let mut parts = vec![];
    for part in path.split(seps).filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." || (windows && part.trim_end_matches(['.', ' ']).is_empty()) {
            return Err(UP.into());
        }
        if is_git(part) {
            return Err(GIT.into());
        }
        if windows && part.contains(':') {
            return Err(format!("A path can't have a : in it on Windows ({part})"));
        }
        if windows && is_device(part) {
            return Err(format!("{part} is the name of a device on Windows, not of a file"));
        }
        parts.push(part);
    }
    Ok(parts)
}

/// The path gate: where `path` (in the folder at `dir`, with `/`) is on disk, links followed, or why the Editor refuses
/// it (`split_path`, and a link that leads out of the folder, into a `.git`, or to nothing). What isn't there yet
/// follows as written.
pub fn gate(dir: &Path, path: &str) -> Result<PathBuf, String> {
    resolve(dir, path, true)
}

/// `path` in the folder at `dir` as written (`dir` joined with its parts, no links followed), after the path gate:
/// what an editor is given to open.
pub fn joined(dir: &Path, path: &str) -> Result<PathBuf, String> {
    gate(dir, path)?;
    Ok(split_path(path, cfg!(windows))?.into_iter().fold(dir.to_path_buf(), |p, part| p.join(part)))
}

/// Whether a path's part names git's folder: `.git` in any case, also with the dots and spaces Windows drops from the
/// end of a name (`.git.`).
fn is_git(part: &str) -> bool {
    part.trim_end_matches(['.', ' ']).eq_ignore_ascii_case(".git")
}

/// Whether `part` is a name Windows keeps for a device (CON, PRN, AUX, NUL, COM1–9, LPT1–9), in any case and with any
/// extension.
fn is_device(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ').to_ascii_uppercase();
    let numbered = stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && matches!(stem.as_bytes()[3], b'1'..=b'9');
    numbered || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
}

/// Where `path` (in the folder at `dir`) is on disk, after the path gate. The parts that are there are followed through
/// their links and checked as they are on disk (also Windows' short names: `GIT~1` is `.git`'s); what isn't there yet
/// follows as written. `follow`: the last part too (to read or save a file); without it the last part is the entry
/// itself, a link as the link (to rename or delete it).
fn resolve(dir: &Path, path: &str, follow: bool) -> Result<PathBuf, String> {
    let parts = split_path(path, cfg!(windows))?;
    let root = dir.canonicalize().map_err(|_| format!("The folder {} isn't there any more", dir.display()))?;
    let (through, last) = match parts.split_last() {
        Some((last, through)) if !follow => (through, Some(*last)),
        _ => (&parts[..], None),
    };
    let mut here = root.clone();
    let mut there = 0;
    for part in through {
        let next = here.join(part);
        if fs::symlink_metadata(&next).is_err() {
            break;
        }
        here = next;
        there += 1;
    }
    let mut out = if there == 0 {
        root.clone()
    } else {
        let real = here.canonicalize().map_err(|_| format!("{path} goes through a link to something that isn't there"))?;
        inside(&root, &real, path)?;
        real
    };
    for part in &through[there..] {
        out.push(part);
    }
    if let Some(last) = last {
        out.push(last);
        if fs::symlink_metadata(&out).is_ok_and(|m| !m.file_type().is_symlink()) {
            let real = out.canonicalize().map_err(|e| format!("Gizai can't read {path}: {e}"))?;
            inside(&root, &real, path)?;
        }
    }
    Ok(out)
}

/// Refuses `real` (a path as it is on disk) outside the folder `root`, or in its `.git`.
fn inside(root: &Path, real: &Path, path: &str) -> Result<(), String> {
    let rel = real.strip_prefix(root).map_err(|_| format!("{path} leads out of the folder through a link"))?;
    if rel.components().any(|c| is_git(&c.as_os_str().to_string_lossy())) {
        return Err(GIT.into());
    }
    Ok(())
}

// ---- reading and changing ----

/// Why a file is read-only in the Editor.
enum Lock {
    /// The whole folder: a card an agent works on.
    Folder(String),
    TooLarge,
    Binary,
    OverMb,
    Ignored,
    NotUtf8,
    OnDisk,
}

impl Lock {
    /// As `OpenedFile::read_only_why` says it.
    fn why(&self) -> String {
        match self {
            Lock::Folder(why) => why.clone(),
            Lock::TooLarge => OVER_5_MB.into(),
            Lock::Binary => BINARY.into(),
            Lock::OverMb => OVER_1_MB.into(),
            Lock::Ignored => IGNORED.into(),
            Lock::NotUtf8 => NOT_UTF8.into(),
            Lock::OnDisk => READ_ONLY_ON_DISK.into(),
        }
    }

    /// Why a change to `rel` is refused.
    fn refusal(&self, rel: &str) -> String {
        match self {
            Lock::Folder(why) => format!("The Editor can't change {rel} now: {why}"),
            Lock::TooLarge => format!("{rel} is over 5 MB, too big for the Editor"),
            Lock::Binary => format!("{rel} is a binary file: the Editor only changes text"),
            Lock::OverMb => format!("{rel} is over 1 MB, so the Editor keeps it read-only"),
            Lock::Ignored => format!("{rel} is in a folder git ignores, so the Editor keeps it read-only"),
            Lock::NotUtf8 => format!("{rel} isn't UTF-8 text: saving it here would change it"),
            Lock::OnDisk => format!("{rel} is read-only on disk"),
        }
    }
}

/// One folder level of the folder at `dir`: `sub` is a folder in it ("" for the top). Folders first, then files, each
/// by name in any case; `.git` never shows. Each with what git ignores and what changed.
pub fn list(dir: &Path, sub: &str) -> Result<Vec<DirEntry>, String> {
    let rel = split_path(sub, cfg!(windows))?.join("/");
    let real = gate(dir, sub)?;
    if !real.is_dir() {
        return Err(match (rel.is_empty(), real.exists()) {
            (true, _) => "The folder isn't there any more".into(),
            (false, true) => format!("{rel} is a file, not a folder"),
            (false, false) => format!("{rel} isn't there any more"),
        });
    }
    let entries = fs::read_dir(&real).map_err(|e| format!("Gizai can't read {}: {e}", if rel.is_empty() { "the folder" } else { &rel }))?;
    let mut out = vec![];
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if is_git(&name) {
            continue;
        }
        let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        // a link counts as what it leads to
        let meta = fs::metadata(e.path()).ok();
        let is_dir = meta.as_ref().is_some_and(Metadata::is_dir);
        let size = meta.as_ref().filter(|m| m.is_file()).map_or(0, Metadata::len);
        out.push(DirEntry { name, path, dir: is_dir, size, ignored: false, git: None });
    }
    let ignored = ignored(dir, &out.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    let changed: BTreeMap<String, GitMark> = changes(dir, &rel).map(|(_, files)| files.into_iter().collect()).unwrap_or_default();
    for e in &mut out {
        e.ignored = ignored.contains(&e.path);
        e.git = if e.dir {
            let inner = format!("{}/", e.path);
            changed.range(inner.clone()..).next().filter(|(k, _)| k.starts_with(&inner)).map(|_| GitMark::Modified)
        } else {
            changed.get(&e.path).copied()
        };
    }
    out.sort_by_cached_key(|e| (!e.dir, e.name.to_lowercase(), e.name.clone()));
    Ok(out)
}

/// The file at `path` in the folder, as the Editor opens it: its text, unless it is binary or over 5 MB; read-only, and
/// why, when it may not change (`Lock`); its line endings and byte order mark; and its version, which `write` checks.
pub fn read(f: Folder, path: &str) -> Result<OpenedFile, String> {
    let parts = split_path(path, cfg!(windows))?;
    let rel = parts.join("/");
    if rel.is_empty() {
        return Err("Pick a file to open".into());
    }
    let real = gate(f.dir, path)?;
    let meta = fs::metadata(&real).map_err(|e| unreadable(&rel, e))?;
    if meta.is_dir() {
        return Err(format!("{rel} is a folder"));
    }
    let mut opened = OpenedFile { path: rel.clone(), text: None, binary: false, too_large: false, read_only: false, read_only_why: None,
                                  crlf: false, bom: false, version: version_of(&meta) };
    let mut lock = f.read_only.map(|why| Lock::Folder(why.to_string()));
    let file = fs::File::open(&real).map_err(|e| unreadable(&rel, e))?;
    // of a file over 5 MB only what tells whether it is binary; never more than 5 MB, also when it grew since
    let up_to = if meta.len() > TEXT_UP_TO { SNIFF as u64 } else { TEXT_UP_TO + 1 };
    let mut bytes = Vec::with_capacity(meta.len().min(up_to) as usize);
    file.take(up_to).read_to_end(&mut bytes).map_err(|e| unreadable(&rel, e))?;
    opened.binary = is_binary(&bytes);
    if meta.len() > TEXT_UP_TO || bytes.len() as u64 > TEXT_UP_TO {
        opened.too_large = true;
        lock.get_or_insert(Lock::TooLarge);
    } else if opened.binary {
        lock.get_or_insert(Lock::Binary);
    } else {
        let (text, bom, crlf, utf8) = decode(&bytes);
        (opened.text, opened.bom, opened.crlf) = (Some(text), bom, crlf);
        if lock.is_none() {
            lock = if bytes.len() as u64 > READ_ONLY_OVER {
                Some(Lock::OverMb)
            } else if in_ignored_folder(f.dir, &parts) {
                Some(Lock::Ignored)
            } else if !utf8 {
                Some(Lock::NotUtf8)
            } else if meta.permissions().readonly() {
                Some(Lock::OnDisk)
            } else {
                None
            };
        }
    }
    if let Some(l) = lock {
        opened.read_only = true;
        opened.read_only_why = Some(l.why());
    }
    Ok(opened)
}

/// Saves `text` as the file at `path`, and returns its new version. Refused with "changed on disk" when the file's
/// version isn't `version` any more (`version` "": a file that isn't there yet, which this makes), and when it is
/// read-only (`Lock`). The text's line endings become the file's (CRLF when its first line ends so), and its byte order
/// mark stays. Atomic: the text goes to a new file next to it, which is renamed over it with its permissions.
pub fn write(f: Folder, path: &str, text: &str, version: &str) -> Result<String, String> {
    let parts = split_path(path, cfg!(windows))?;
    let rel = parts.join("/");
    if rel.is_empty() {
        return Err("Pick a file to save".into());
    }
    if let Some(why) = f.read_only {
        return Err(Lock::Folder(why.into()).refusal(&rel));
    }
    let real = gate(f.dir, path)?;
    let changed = format!("{rel} changed on disk since you opened it");
    let (crlf, bom, permissions) = match fs::metadata(&real) {
        Ok(m) if m.is_dir() => return Err(format!("{rel} is a folder")),
        Ok(m) => {
            if version_of(&m) != version {
                return Err(changed);
            }
            if m.len() > READ_ONLY_OVER {
                return Err(Lock::OverMb.refusal(&rel));
            }
            if m.permissions().readonly() {
                return Err(Lock::OnDisk.refusal(&rel));
            }
            let bytes = fs::read(&real).map_err(|e| unreadable(&rel, e))?;
            if is_binary(&bytes) {
                return Err(Lock::Binary.refusal(&rel));
            }
            let (bom, crlf, utf8) = endings(&bytes);
            if !utf8 {
                return Err(Lock::NotUtf8.refusal(&rel));
            }
            (crlf, bom, Some(m.permissions()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if !version.is_empty() {
                return Err(format!("{changed}: it isn't there any more"));
            }
            (false, false, None)
        }
        Err(e) => return Err(unreadable(&rel, e)),
    };
    if in_ignored_folder(f.dir, &parts) {
        return Err(Lock::Ignored.refusal(&rel));
    }
    if let Some(parent) = real.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Gizai couldn't save {rel}: {e}"))?;
    }
    replace(&real, &encode(text, crlf, bom), permissions).map_err(|e| format!("Gizai couldn't save {rel}: {e}"))?;
    fs::metadata(&real).map(|m| version_of(&m)).map_err(|e| unreadable(&rel, e))
}

/// A new empty file at `path`, or a new folder (`dir`), with the folders above it that aren't there yet. Refused when
/// something is there already, in a read-only folder and in a folder git ignores.
pub fn create(f: Folder, path: &str, dir: bool) -> Result<(), String> {
    let (parts, rel, real) = entry(f, path, "make")?;
    if fs::symlink_metadata(&real).is_ok() {
        return Err(format!("{rel} is there already"));
    }
    if in_ignored_folder(f.dir, &parts) {
        return Err(Lock::Ignored.refusal(&rel));
    }
    let made = real.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| {
        if dir { fs::create_dir(&real) } else { fs::OpenOptions::new().write(true).create_new(true).open(&real).map(|_| ()) }
    });
    made.map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => format!("{rel} is there already"),
        _ => format!("Gizai couldn't make {rel}: {e}"),
    })
}

/// Renames or moves the file or folder at `from` to `to` (a link as the link), making the folders above `to` that
/// aren't there yet. Refused when something else is at `to` (a new case of the same name is fine), for a folder into
/// itself, in a read-only folder and in a folder git ignores.
pub fn rename(f: Folder, from: &str, to: &str) -> Result<(), String> {
    let (from_parts, from_rel, from_real) = entry(f, from, "rename")?;
    let (to_parts, to_rel, to_real) = entry(f, to, "rename")?;
    let meta = fs::symlink_metadata(&from_real).map_err(|e| unreadable(&from_rel, e))?;
    if from_rel == to_rel {
        return Ok(());
    }
    if meta.is_dir() && to_rel.starts_with(&format!("{from_rel}/")) {
        return Err(format!("{from_rel} can't move into itself"));
    }
    if fs::symlink_metadata(&to_real).is_ok() && !same_entry(&from_real, &to_real) {
        return Err(format!("{to_rel} is there already"));
    }
    for (parts, rel) in [(&from_parts, &from_rel), (&to_parts, &to_rel)] {
        if in_ignored_folder(f.dir, parts) {
            return Err(Lock::Ignored.refusal(rel));
        }
    }
    to_real.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::rename(&from_real, &to_real))
        .map_err(|e| format!("Gizai couldn't rename {from_rel} to {to_rel}: {e}"))
}

/// Deletes the file or folder at `path`, a folder with everything in it, a link as the link (never what it leads to).
/// Refused for the folder itself, in a read-only folder and in a folder git ignores.
pub fn delete(f: Folder, path: &str) -> Result<(), String> {
    let (parts, rel, real) = entry(f, path, "delete")?;
    let meta = fs::symlink_metadata(&real).map_err(|e| unreadable(&rel, e))?;
    if in_ignored_folder(f.dir, &parts) {
        return Err(Lock::Ignored.refusal(&rel));
    }
    let done = if meta.file_type().is_symlink() {
        // on Windows a link to a folder goes as a folder
        fs::remove_file(&real).or_else(|e| fs::remove_dir(&real).map_err(|_| e))
    } else if meta.is_dir() {
        fs::remove_dir_all(&real)
    } else {
        fs::remove_file(&real)
    };
    done.map_err(|e| format!("Gizai couldn't delete {rel}: {e}"))
}

/// The entry at `path` that a new file, rename or delete is about: its parts, its path as the screen names it, and
/// where it is (a link as the link). Refused for the folder itself, and in a read-only folder. `doing`: "make",
/// "rename" or "delete".
fn entry<'p>(f: Folder, path: &'p str, doing: &str) -> Result<(Vec<&'p str>, String, PathBuf), String> {
    let parts = split_path(path, cfg!(windows))?;
    let rel = parts.join("/");
    if rel.is_empty() {
        return Err(format!("Pick a file or folder in the folder to {doing}"));
    }
    if let Some(why) = f.read_only {
        return Err(Lock::Folder(why.into()).refusal(&rel));
    }
    let real = resolve(f.dir, path, false)?;
    Ok((parts, rel, real))
}

/// Whether `a` and `b` are the same entry: a new case of a name on a disk that doesn't tell cases apart.
fn same_entry(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        matches!((fs::symlink_metadata(a), fs::symlink_metadata(b)), (Ok(x), Ok(y)) if x.dev() == y.dev() && x.ino() == y.ino())
    }
    #[cfg(not(unix))]
    {
        matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
    }
}

/// The current version of each of `paths` in the folder at `dir`: "" for a file that is gone (or refused by now).
pub fn versions(dir: &Path, paths: &[String]) -> Vec<String> {
    paths.iter().map(|p| {
        gate(dir, p).ok().and_then(|real| fs::metadata(real).ok()).filter(Metadata::is_file).map(|m| version_of(&m)).unwrap_or_default()
    }).collect()
}

/// A file's version: its size and modification time, and on Linux and macOS its inode and change time too (which
/// nothing can set back), so any change by anyone gives another version.
fn version_of(m: &Metadata) -> String {
    let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let changed = i128::from(m.ctime()) * 1_000_000_000 + i128::from(m.ctime_nsec());
        format!("{:x}-{modified:x}-{changed:x}-{:x}", m.len(), m.ino())
    }
    #[cfg(not(unix))]
    {
        format!("{:x}-{modified:x}", m.len())
    }
}

/// git's own test for a binary file: a NUL byte in its first 8,000 bytes.
fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(SNIFF)].contains(&0)
}

/// A file's bytes as the Editor shows them: without the byte order mark, with `\n` for every line ending. Also whether
/// it had a byte order mark, whether its first line ends in CRLF, and whether it was UTF-8 (when not, what isn't shows
/// as �).
fn decode(bytes: &[u8]) -> (String, bool, bool, bool) {
    let (bom, crlf, utf8) = endings(bytes);
    let body = if bom { &bytes[BOM.len()..] } else { bytes };
    (String::from_utf8_lossy(body).replace("\r\n", "\n"), bom, crlf, utf8)
}

/// Whether the file's bytes start with a byte order mark, whether its first line ends in CRLF, and whether it is UTF-8.
fn endings(bytes: &[u8]) -> (bool, bool, bool) {
    let bom = bytes.starts_with(BOM);
    let body = if bom { &bytes[BOM.len()..] } else { bytes };
    let crlf = body.iter().position(|b| *b == b'\n').is_some_and(|i| i > 0 && body[i - 1] == b'\r');
    (bom, crlf, std::str::from_utf8(body).is_ok())
}

/// `text` as the file's bytes: its line endings all `\n`, or all CRLF with `crlf`, and the byte order mark first with
/// `bom` (one the text starts with doesn't count twice).
fn encode(text: &str, crlf: bool, bom: bool) -> Vec<u8> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let text = if crlf { text.replace('\n', "\r\n") } else { text };
    let mut out = Vec::with_capacity(text.len() + BOM.len());
    if bom {
        out.extend_from_slice(BOM);
    }
    out.extend_from_slice(text.as_bytes());
    out
}

/// Writes `bytes` to a new file next to `target` and renames it over `target`, so whoever reads `target` sees the old
/// file or the new one, never half of one. The new file gets `permissions` (the old one's: a script stays executable).
fn replace(target: &Path, bytes: &[u8], permissions: Option<fs::Permissions>) -> std::io::Result<()> {
    let dir = target.parent().ok_or_else(|| std::io::Error::other("it has no folder"))?;
    let name = target.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut tag = [0u8; 6];
    getrandom::fill(&mut tag).map_err(|e| std::io::Error::other(e.to_string()))?;
    let tag: String = tag.iter().map(|b| format!("{b:02x}")).collect();
    let temp = dir.join(format!(".{name}.{tag}.gizai-save"));
    let written = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Some(p) = permissions {
            fs::set_permissions(&temp, p)?;
        }
        fs::rename(&temp, target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// Why a file couldn't be read, in a sentence.
fn unreadable(rel: &str, e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => format!("{rel} isn't there any more"),
        _ => format!("Gizai can't read {rel}: {e}"),
    }
}

// ---- git ----

/// git in the folder `dir`: without optional locks (an agent's git in the same worktree never finds the index locked by
/// the Editor) and with literal pathspecs.
fn git_command(dir: &Path, args: &[&str]) -> Command {
    let mut cmd = os::command("git");
    cmd.arg("-C").arg(dir).args(args).env("GIT_OPTIONAL_LOCKS", "0").env("GIT_LITERAL_PATHSPECS", "1");
    cmd
}

/// `git_command` with `input` on its stdin, ended after `GIT_LIMIT`: (succeeded, stdout, stderr).
fn git(dir: &Path, args: &[&str], input: Option<Vec<u8>>) -> Result<(bool, Vec<u8>, String), String> {
    let mut child = git_command(dir, args).stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("Gizai can't run git: {e}"))?;
    if let (Some(bytes), Some(mut pipe)) = (input, child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        });
    }
    let (out, err) = (drain(child.stdout.take()), drain(child.stderr.take()));
    let until = Instant::now() + GIT_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(5)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("git gave no answer within {} s", GIT_LIMIT.as_secs()));
            }
            Err(e) => return Err(format!("Gizai lost track of git: {e}")),
        }
    };
    let said = String::from_utf8_lossy(&err.join().unwrap_or_default()).trim().to_string();
    Ok((status.success(), out.join().unwrap_or_default(), said))
}

/// Reads a pipe to its end on a thread of its own.
fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut all = vec![];
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut all);
        }
        all
    })
}

/// The fields of git's `-z` output.
fn fields(out: &[u8]) -> impl Iterator<Item = &[u8]> {
    out.split(|b| *b == 0).filter(|f| !f.is_empty())
}

/// git's last line, without its "fatal: ".
fn last_line(said: &str) -> String {
    let line = said.lines().last().unwrap_or("git failed").trim();
    line.strip_prefix("fatal: ").or_else(|| line.strip_prefix("error: ")).unwrap_or(line).to_string()
}

/// Where the folder `dir` is in its git repository: "" at its top, "app/" in a folder of it; None outside a repository.
fn prefix(dir: &Path) -> Option<String> {
    match git(dir, &["rev-parse", "--show-prefix"], None) {
        Ok((true, out, _)) => Some(String::from_utf8_lossy(&out).trim_end_matches(['\r', '\n']).to_string()),
        _ => None,
    }
}

/// Which of `paths` (in the folder `dir`, with `/`) git ignores: what `.gitignore`, `info/exclude` and your own
/// excludes name, and everything in a folder they name; never a tracked file. None outside a repository.
fn ignored(dir: &Path, paths: &[String]) -> HashSet<String> {
    if paths.is_empty() {
        return HashSet::new();
    }
    let mut input = vec![];
    for p in paths {
        input.extend_from_slice(p.as_bytes());
        input.push(0);
    }
    // exit code 1: none of them; past a path git refuses (one beyond a link) what it said so far still counts
    match git(dir, &["check-ignore", "-z", "--stdin"], Some(input)) {
        Ok((_, out, _)) => fields(&out).map(|p| String::from_utf8_lossy(p).into_owned()).collect(),
        Err(_) => HashSet::new(),
    }
}

/// Whether one of the folders above `parts` (a path in the folder `dir`) is ignored by git: the files in it are
/// read-only, and nothing in it is made, renamed or deleted.
fn in_ignored_folder(dir: &Path, parts: &[&str]) -> bool {
    let above: Vec<String> = (1..parts.len()).map(|n| parts[..n].join("/")).collect();
    !ignored(dir, &above).is_empty()
}

/// The branch (None when detached), and each changed file with its mark by its path in the folder.
type Changes = (Option<String>, Vec<(String, GitMark)>);

/// What git status says about the folder `dir`, or its folder `sub` ("" for all of it): the branch, and each changed
/// file, untracked ones one by one. None outside a repository.
fn changes(dir: &Path, sub: &str) -> Option<Changes> {
    let prefix = prefix(dir)?;
    let spec = if sub.is_empty() { "." } else { sub };
    match git(dir, &["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all", "--", spec], None) {
        Ok((true, out, _)) => Some(parse_status(&out, &prefix)),
        _ => None,
    }
}

/// `git status --porcelain=v2 -z --branch`'s branch and files, their paths without `prefix` (where the folder is in
/// the repository; git gives paths from its top).
fn parse_status(out: &[u8], prefix: &str) -> Changes {
    let mut branch = None;
    let mut files = vec![];
    let mut all = fields(out);
    while let Some(field) = all.next() {
        let line = String::from_utf8_lossy(field);
        let words = |n: usize| line.splitn(n, ' ').collect::<Vec<_>>();
        let (mark, path) = match line.as_bytes()[0] {
            b'#' => {
                if let Some(head) = line.strip_prefix("# branch.head ") {
                    branch = (head != "(detached)").then(|| head.to_string());
                }
                continue;
            }
            // 1 XY sub mH mI mW hH hI path
            b'1' => match words(9)[..] {
                [_, xy, _, _, _, _, _, _, path] => (changed(xy), path.to_string()),
                _ => continue,
            },
            // 2 XY sub mH mI mW hH hI Xscore path, then the path it had
            b'2' => {
                let _was = all.next();
                match words(10)[..] {
                    [_, xy, _, _, _, _, _, _, _, path] => (moved(xy), path.to_string()),
                    _ => continue,
                }
            }
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            b'u' => match words(11)[..] {
                [.., path] => (GitMark::Conflicted, path.to_string()),
                _ => continue,
            },
            b'?' => (GitMark::Untracked, line[2.min(line.len())..].to_string()),
            _ => continue,
        };
        if let Some(p) = path.strip_prefix(prefix).filter(|p| !p.is_empty()) {
            files.push((p.to_string(), mark));
        }
    }
    (branch, files)
}

/// A changed file's mark from its XY (staged, then not staged).
fn changed(xy: &str) -> GitMark {
    if xy.contains('D') {
        GitMark::Deleted
    } else if xy.contains('A') {
        GitMark::Added
    } else if xy.contains('R') {
        GitMark::Renamed
    } else {
        GitMark::Modified
    }
}

/// A renamed (R) or copied (C) file's mark.
fn moved(xy: &str) -> GitMark {
    if xy.contains('D') {
        GitMark::Deleted
    } else if xy.contains('R') {
        GitMark::Renamed
    } else {
        GitMark::Added
    }
}

/// The branch of the folder at `dir`, and each changed file's mark (local: nothing is fetched). Empty outside a git
/// repository.
pub fn status(dir: &Path) -> FolderGit {
    let Some((branch, list)) = changes(dir, "") else { return FolderGit::default() };
    let all: BTreeMap<String, GitMark> = list.into_iter().collect();
    let uncommitted = all.len();
    let files = if uncommitted > MAX_CHANGED { all.into_iter().take(MAX_CHANGED).collect() } else { all };
    FolderGit { branch, files, uncommitted }
}

/// Go to file: the files of the folder at `dir`, tracked and untracked, that git doesn't ignore; those that are gone
/// left out; sorted, at most `MAX_FILES`. Outside a git repository every file in it (not `.git`).
pub fn files(dir: &Path) -> Result<Vec<String>, String> {
    if prefix(dir).is_none() {
        return Ok(walk(dir));
    }
    let (ok, out, said) = git(dir, &["ls-files", "--cached", "--others", "--exclude-standard", "-z"], None)?;
    if !ok {
        return Err(format!("git couldn't list the files: {}", last_line(&said)));
    }
    let mut seen = HashSet::new();
    let mut list = vec![];
    // a file with a merge conflict shows once per side
    for p in fields(&out).map(|p| String::from_utf8_lossy(p).into_owned()) {
        if p.split('/').any(is_git) || !seen.insert(p.clone()) {
            continue;
        }
        if fs::metadata(dir.join(&p)).is_ok_and(|m| !m.is_dir()) {
            list.push(p);
            if list.len() >= MAX_FILES {
                break;
            }
        }
    }
    list.sort();
    Ok(list)
}

/// Every file in the folder at `dir` and the folders in it, for a folder outside git: not `.git`, and a link to a folder
/// isn't followed. Sorted, at most `MAX_FILES`.
fn walk(dir: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut todo = vec![(dir.to_path_buf(), String::new())];
    while let Some((here, rel)) = todo.pop() {
        let Ok(entries) = fs::read_dir(&here) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if is_git(&name) {
                continue;
            }
            let path = if rel.is_empty() { name } else { format!("{rel}/{name}") };
            match e.file_type() {
                Ok(t) if t.is_dir() => todo.push((e.path(), path)),
                Ok(t) if t.is_file() || fs::metadata(e.path()).is_ok_and(|m| m.is_file()) => {
                    out.push(path);
                    if out.len() >= MAX_FILES {
                        out.sort();
                        return out;
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

// ---- search ----

/// Searches the folder at `dir` with `git grep`: tracked and untracked files git doesn't ignore (outside a repository
/// every file), binary files left out. Stops at `MAX_HITS` and after `SEARCH_LIMIT`, `truncated` then. Nothing for an
/// empty query.
pub fn search(dir: &Path, query: &str, how: Find) -> Result<SearchResult, String> {
    search_within(dir, query, how, MAX_HITS, SEARCH_LIMIT)
}

/// `search`, stopping at `max_hits` and after `limit`.
pub fn search_within(dir: &Path, query: &str, how: Find, max_hits: usize, limit: Duration) -> Result<SearchResult, String> {
    if query.is_empty() {
        return Ok(SearchResult::default());
    }
    if query.contains(['\n', '\r', '\0']) {
        return Err("Search looks within one line at a time: leave out the line break".into());
    }
    let until = Instant::now() + limit;
    let repo = prefix(dir).is_some();
    let found = match grep(dir, query, how, repo, if how.regex { "-P" } else { "-F" }, max_hits, until) {
        // git without PCRE: POSIX's extended regular expressions
        Err(Grep::NoPcre) => grep(dir, query, how, repo, "-E", max_hits, until),
        found => found,
    };
    found.map_err(|e| match e {
        Grep::Said(said) if how.regex => format!("Search couldn't use that regular expression: {said}"),
        Grep::Said(said) => format!("Search failed: {said}"),
        Grep::NoPcre => "Search failed: git can't use that regular expression".into(),
    })
}

/// Why `grep` failed.
enum Grep {
    /// This git has no Perl-compatible regular expressions.
    NoPcre,
    /// git's own words.
    Said(String),
}

/// One `git grep` (`kind`: `-F` for plain text, `-P` or `-E` for a regular expression), read as it goes, until
/// `max_hits` or `until`.
fn grep(dir: &Path, query: &str, how: Find, repo: bool, kind: &str, max_hits: usize, until: Instant) -> Result<SearchResult, Grep> {
    let mut args = vec!["grep", "-z", "-n", "--column", "-I", "--no-color"];
    // outside a repository the .gitignore files there still count
    args.extend(if repo { &["--untracked"][..] } else { &["--no-index", "--exclude-standard"][..] });
    if !how.case_sensitive {
        args.push("-i");
    }
    if how.whole_word {
        args.push("-w");
    }
    args.extend([kind, "-e", query]);
    let mut child = git_command(dir, &args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .map_err(|e| Grep::Said(format!("Gizai can't run git: {e}")))?;
    let err = drain(child.stderr.take());
    // a few hits ahead at most: git waits for the rest, and is ended once there are enough
    let (tx, rx) = mpsc::sync_channel(256);
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(out).split(b'\n') {
                let Ok(line) = line else { break };
                if let Some(hit) = parse_hit(&line)
                    && tx.send(hit).is_err()
                {
                    break;
                }
            }
        });
    }
    let mut found = SearchResult::default();
    loop {
        match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
            Ok(hit) if found.hits.len() < max_hits => found.hits.push(hit),
            // one hit more than may be shown, or out of time
            Ok(_) | Err(RecvTimeoutError::Timeout) => {
                found.truncated = true;
                break;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    if found.truncated {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| Grep::Said(e.to_string()))?;
    let said = String::from_utf8_lossy(&err.join().unwrap_or_default()).trim().to_string();
    // exit code 1: nothing found
    if !found.truncated && !status.success() && status.code() != Some(1) {
        // "cannot use Perl-compatible regexes when not compiled with USE_LIBPCRE" (the build flag's name isn't translated)
        if kind == "-P" && said.contains("USE_LIBPCRE") {
            return Err(Grep::NoPcre);
        }
        return Err(Grep::Said(last_line(&said)));
    }
    Ok(found)
}

/// One line of `git grep -z -n --column`: path, line, column (its first match's byte, from 1) and text, NUL between
/// them.
fn parse_hit(raw: &[u8]) -> Option<SearchHit> {
    let at = raw.iter().position(|b| *b == 0)?;
    let (path, rest) = (&raw[..at], &raw[at + 1..]);
    let (line, rest) = number(rest)?;
    let (byte, text) = number(rest)?;
    let text = text.strip_suffix(b"\r").unwrap_or(text);
    let before = &text[..(byte as usize).saturating_sub(1).min(text.len())];
    let column = String::from_utf8_lossy(before).encode_utf16().count() + 1;
    let mut shown = String::from_utf8_lossy(text).into_owned();
    if let Some((cut, _)) = shown.char_indices().nth(HIT_TEXT) {
        shown.truncate(cut);
    }
    Some(SearchHit { path: String::from_utf8_lossy(path).into_owned(), line: u32::try_from(line).ok()?, column: u32::try_from(column).ok()?, text: shown })
}

/// The number `b` starts with, and what follows the separator after it (NUL with `-z`, else `:`).
fn number(b: &[u8]) -> Option<(u64, &[u8])> {
    let n = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if n == 0 || n >= b.len() || !matches!(b[n], 0 | b':') {
        return None;
    }
    Some((std::str::from_utf8(&b[..n]).ok()?.parse().ok()?, &b[n + 1..]))
}

// ---- Open in VS Code, Zed or PhpStorm ----

/// The editors found on this computer, in `AppId::ALL`'s order (`find_app`).
pub fn apps(path: &OsStr, home: &Path) -> Vec<EditorApp> {
    AppId::ALL.into_iter().filter(|id| find_app(*id, path, home).is_some())
        .map(|id| EditorApp { id, name: id.name().to_string() }).collect()
}

/// Where the editor `id`'s command is: the first of its names (`AppId::programs`) in a folder of `path` (a PATH), then
/// in the places its installers use that aren't always on PATH: JetBrains Toolbox's scripts folder (`toolbox_scripts`),
/// and on macOS the command inside the app in /Applications or ~/Applications. A `zed` that is ZFS's event daemon
/// (Arch has both by that name) doesn't count.
pub fn find_app(id: AppId, path: &OsStr, home: &Path) -> Option<PathBuf> {
    let dirs: Vec<PathBuf> = std::env::split_paths(path).filter(|d| !d.as_os_str().is_empty()).collect();
    for name in id.programs() {
        for d in &dirs {
            if let Some(found) = os::find_in(name, d.as_os_str())
                && !(*name == "zed" && zfs_daemon(&found))
            {
                return Some(found);
            }
        }
    }
    other_places(id, home).into_iter().find(|p| os::executable(p))
}

/// Whether the `zed` at `p` is ZFS's event daemon: `zpool` is in the same folder.
fn zfs_daemon(p: &Path) -> bool {
    p.parent().is_some_and(|d| d.join("zpool").exists())
}

/// Where an editor's command often is without being on PATH.
fn other_places(id: AppId, home: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    if id == AppId::PhpStorm {
        out.push(toolbox_scripts(home).join(if cfg!(windows) { "phpstorm.cmd" } else { "phpstorm" }));
    }
    if cfg!(target_os = "macos") {
        let inside = match id {
            AppId::VsCode => "Visual Studio Code.app/Contents/Resources/app/bin/code",
            AppId::Zed => "Zed.app/Contents/MacOS/cli",
            AppId::PhpStorm => "PhpStorm.app/Contents/MacOS/phpstorm",
        };
        out.push(Path::new("/Applications").join(inside));
        out.push(home.join("Applications").join(inside));
    }
    out
}

/// The folder where the JetBrains Toolbox App puts the scripts that start its IDEs (its Settings → Tools → Shell
/// scripts location), as it is by default.
pub fn toolbox_scripts(home: &Path) -> PathBuf {
    if cfg!(windows) {
        home.join(r"AppData\Local\JetBrains\Toolbox\scripts")
    } else if cfg!(target_os = "macos") {
        home.join("Library/Application Support/JetBrains/Toolbox/scripts")
    } else {
        home.join(".local/share/JetBrains/Toolbox/scripts")
    }
}

/// The arguments that open the folder `dir` in the editor `id`, or the file `file` in it, at `line` when given (from
/// 1): VS Code `<dir> -g <file>:<line>`, Zed `<dir> <file>:<line>`, PhpStorm `<dir> --line <line> <file>`. The folder
/// comes first, so the file opens in its folder's window.
pub fn open_args(id: AppId, dir: &Path, file: Option<&Path>, line: Option<u32>) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![dir.into()];
    let Some(file) = file else { return args };
    let at = |l: u32| {
        let mut s = file.as_os_str().to_os_string();
        s.push(format!(":{l}"));
        s
    };
    match (id, line.filter(|l| *l > 0)) {
        (AppId::VsCode, Some(l)) => args.extend(["-g".into(), at(l)]),
        (AppId::Zed, Some(l)) => args.push(at(l)),
        (AppId::PhpStorm, Some(l)) => args.extend(["--line".into(), l.to_string().into(), file.into()]),
        (_, None) => args.push(file.into()),
    }
    args
}

/// Starts the editor `id` at `program` (`find_app`) with `open_args`, with `path` as its PATH, and leaves it: Gizai
/// doesn't wait for it, and on Linux and macOS it has a process group of its own, so it doesn't end with Gizai.
pub fn open_in(id: AppId, program: &Path, dir: &Path, file: Option<&Path>, line: Option<u32>, path: &OsStr) -> Result<(), String> {
    let mut cmd = os::command(program);
    cmd.args(open_args(id, dir, file, line)).current_dir(dir).env("PATH", path)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Gizai couldn't start {}: {e}", id.name()))?;
    // reaped when it ends: PhpStorm's own script runs as long as PhpStorm does
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Puts `/.idea/` in the `info/exclude` of the repository the folder at `dir` is a checkout of (shared by all its
/// checkouts and worktrees), so the project files PhpStorm adds never show in git status, nor in a card's changes.
/// `.gitignore` is never touched.
pub fn keep_idea_out(dir: &Path) -> Result<(), String> {
    let common = worktree::git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .map_err(|e| format!("Gizai couldn't find the git folder of {}: {e}", dir.display()))?;
    worktree::exclude(Path::new(&common), IDEA_EXCLUDE).map_err(|e| format!("Gizai couldn't keep .idea/ out of git status: {e}"))
}
