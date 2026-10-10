//! An agent's folders (agent form → Permissions → Folders): folders besides its card's worktree that its file tools may
//! read ("read") or read and change ("change"). They limit the file tools, not the commands an agent may run. Only you
//! change the list, in the form: the Team Lead's `create_agent` and `update_agent` leave it alone.
//! - Refused, with a reason: `/`, your home folder and the folders above it, Gizai's data folder, and the folders that
//!   hold keys or logins (`~/.ssh`, `~/.gnupg`, `~/.config`, …), with what is inside them and the folders above them.
//! - A "read and change" folder that is a project's main checkout (or holds one, or is inside one) gets a warning:
//!   agents normally never touch it.
//! - At run time a missing folder, or one refused by then (say through a link), is skipped with a note (`for_run`).
//! - The Team Lead only reads its folders in chat. "Read and change" lets it update one with `update_checkout` after
//!   you said yes in the chat (`may_update`).
//! - On Windows the same with its paths: `C:\` is the whole disk, `~\` your profile folder, either slash separates
//!   folders, and names are compared in any case.
use std::borrow::Cow;
use std::path::{Component, MAIN_SEPARATOR, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result};

/// One folder in an agent's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    /// An absolute path; `~/` is your home folder when you type it, and it is saved expanded.
    pub path: String,
    /// "read", or "change": read and change.
    pub access: String,
}

impl Folder {
    /// Read and change.
    pub fn change(&self) -> bool {
        self.access == "change"
    }
}

/// The access levels as saved: "read", and "change" for read and change.
pub const ACCESS: [&str; 2] = ["read", "change"];
/// The most folders one agent has.
pub const MAX: usize = 20;

/// Folders in your home folder that hold keys, tokens or logins. Never given to an agent, nor anything inside them or a
/// folder above them. A first folder whose name starts with `.claude` counts too (`~/.claude-2`, a second account).
#[cfg(not(any(windows, target_os = "macos")))]
pub const KEY_FOLDERS: [&str; 15] = [".ssh", ".gnupg", ".config", ".aws", ".azure", ".kube", ".docker", ".password-store", ".pki",
                                     ".local/share/keyrings", ".mozilla", ".claude", ".codex", ".gemini", ".terraform.d"];
/// macOS: Linux's, and the keychains.
#[cfg(target_os = "macos")]
pub const KEY_FOLDERS: [&str; 16] = [".ssh", ".gnupg", ".config", ".aws", ".azure", ".kube", ".docker", ".password-store", ".pki",
                                     ".local/share/keyrings", ".mozilla", ".claude", ".codex", ".gemini", ".terraform.d",
                                     "Library/Keychains"];
/// Windows, in your profile folder: the same tools' folders, AppData\Roaming (Gizai's data, and gh's and git's logins),
/// Windows' own credentials and Edge (AppData\Local\Microsoft), and Chrome's profile (AppData\Local\Google).
#[cfg(windows)]
pub const KEY_FOLDERS: [&str; 13] = [".ssh", ".gnupg", ".config", ".aws", ".azure", ".kube", ".docker", ".claude", ".codex", ".gemini",
                                     r"AppData\Roaming", r"AppData\Local\Microsoft", r"AppData\Local\Google"];

/// Characters a folder's path can't have: Claude Code reads them in a rule as a pattern or a separator, and Gemini
/// splits its folders at a comma. On Windows a backslash separates folders, so it is fine there.
#[cfg(not(windows))]
const ODD: [char; 8] = ['(', ')', '[', ']', '*', '?', '\\', ','];
#[cfg(windows)]
const ODD: [char; 7] = ['(', ')', '[', ']', '*', '?', ','];
/// The odd characters as the refusal names them.
#[cfg(not(windows))]
const ODD_SAID: &str = "( ) [ ] * ? , or \\";
#[cfg(windows)]
const ODD_SAID: &str = "( ) [ ] * ? or ,";
/// What separates the folders of a path you type: `/`, and on Windows `\` too.
const SEPARATORS: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
/// What the refusal of a path that isn't a full one gives as examples.
const FULL_PATH: &str = if cfg!(windows) { r"give the full path, like ~\Herd\shared or D:\data" } else { "give the full path, like ~/Herd/shared or /srv/data" };

/// What a folder is checked against.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// Your home folder.
    pub home: PathBuf,
    /// Gizai's data folders: this Gizai's, and the installed one's.
    pub data: Vec<PathBuf>,
    /// The projects' main checkouts: (project name, folder).
    pub checkouts: Vec<(String, PathBuf)>,
}

impl Places {
    /// Your home folder ($HOME; on Windows your profile folder, %USERPROFILE%), Gizai's data folders (the database's
    /// folder, and where the installed Gizai keeps its data: `$XDG_DATA_HOME/gizai` or `~/.local/share/gizai`, on macOS
    /// also `~/Library/Application Support/Gizai`, on Windows `%APPDATA%\Gizai`) and the projects' main checkouts.
    pub fn of(db: &Db) -> Places {
        let home = if cfg!(windows) { std::env::home_dir() } else { std::env::var_os("HOME").map(PathBuf::from) };
        let home = home.filter(|h| h.is_absolute()).unwrap_or_default();
        let mut data: Vec<PathBuf> = db.dir().map(Path::to_path_buf).into_iter().collect();
        if let Some(x) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|x| x.is_absolute()) {
            data.push(x.join("gizai"));
        }
        if !cfg!(windows) && !home.as_os_str().is_empty() {
            data.push(home.join(".local/share/gizai"));
        }
        if cfg!(target_os = "macos") && !home.as_os_str().is_empty() {
            data.push(home.join("Library/Application Support/Gizai"));
        }
        if cfg!(windows) {
            data.extend(std::env::var_os("APPDATA").map(PathBuf::from).filter(|a| a.is_absolute()).map(|a| a.join("Gizai")));
        }
        let checkouts = crate::projects::list(db).unwrap_or_default().into_iter()
            .filter_map(|p| Some((p.name, PathBuf::from(p.repo_path.filter(|r| !r.trim().is_empty())?))))
            .collect();
        Places { home, data, checkouts }
    }

    /// `~/…` (on Windows `~\…`) for a path in your home folder, else the path.
    pub fn show(&self, p: &Path) -> String {
        match p.strip_prefix(&self.home) {
            Ok(rest) if !self.home.as_os_str().is_empty() => format!("~{MAIN_SEPARATOR}{}", rest.display()),
            _ => p.display().to_string(),
        }
    }
}

/// What the form shows next to one folder: why it is refused, or a warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCheck {
    /// The path as it is saved (absolute, `~/` expanded); the path as typed when it isn't one.
    pub path: String,
    pub error: Option<String>,
    pub warning: Option<String>,
}

/// `~/x` → `<home>/x`, with `.` and `..` worked out by name and no slash at the end. None: not an absolute path. On
/// Windows `~\x` too, either slash separates folders and the drive is written one way: `c:/x` and `\\?\C:\x` (as
/// `canonicalize` gives it) are both `C:\x`.
pub fn normalize(raw: &str, home: &Path) -> Option<PathBuf> {
    let raw = raw.trim();
    let p = match raw.strip_prefix('~') {
        Some("") => home.to_path_buf(),
        Some(rest) if rest.starts_with(SEPARATORS) && !home.as_os_str().is_empty() => home.join(rest.trim_start_matches(SEPARATORS)),
        _ => PathBuf::from(raw),
    };
    if !p.is_absolute() {
        return None;
    }
    let mut out = if cfg!(windows) { PathBuf::new() } else { PathBuf::from("/") };
    for c in p.components() {
        match c {
            Component::ParentDir => { out.pop(); }
            Component::Normal(n) => out.push(n),
            // Windows only: the drive or share, then the root folder on it.
            Component::Prefix(pre) => out.push(plain_prefix(pre)),
            Component::RootDir if cfg!(windows) => out.push(std::path::MAIN_SEPARATOR_STR),
            _ => {}
        }
    }
    Some(out)
}

/// Windows: a path's drive or share, written one way: `C:` for `c:` and `\\?\C:`, `\\server\share` for
/// `\\?\UNC\server\share`; another kind (`\\.\pipe`…) as it is.
fn plain_prefix(pre: std::path::PrefixComponent<'_>) -> std::ffi::OsString {
    use std::path::Prefix;
    match pre.kind() {
        Prefix::Disk(d) | Prefix::VerbatimDisk(d) => format!("{}:", d.to_ascii_uppercase() as char).into(),
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) =>
            format!(r"\\{}\{}", server.to_string_lossy(), share.to_string_lossy()).into(),
        _ => pre.as_os_str().to_os_string(),
    }
}

/// A path as `refusal` compares it: on Windows in lower case, since a name there is the same in any case.
fn fold(p: &Path) -> Cow<'_, Path> {
    if cfg!(windows) {
        return Cow::Owned(PathBuf::from(p.as_os_str().to_string_lossy().to_lowercase()));
    }
    Cow::Borrowed(p)
}

/// The folder as it is on disk (links followed), or None when it isn't there. On Windows written as `normalize` writes
/// it, without the `\\?\` that `canonicalize` puts before it, as the coding CLIs take it.
fn on_disk(p: &Path) -> Option<PathBuf> {
    let real = p.canonicalize().ok()?;
    if cfg!(windows) {
        return normalize(&real.to_string_lossy(), Path::new(""));
    }
    Some(real)
}

/// Why the folder `p` (absolute, normalized) can't be an agent's, or None.
pub fn refusal(p: &Path, places: &Places) -> Option<String> {
    let (said, p, home) = (p, &*fold(p), &*fold(&places.home));
    let has_home = !home.as_os_str().is_empty();
    if p.parent().is_none() {
        return Some(format!("that's the whole disk ({}): pick the folder the agent needs", said.display()));
    }
    if has_home && p == home {
        return Some("that's your home folder: pick the folder the agent needs inside it".into());
    }
    if has_home && home.starts_with(p) {
        return Some(format!("it holds your home folder ({}): pick the folder the agent needs", places.home.display()));
    }
    for d in &places.data {
        let folded = fold(d);
        if p.starts_with(&folded) {
            return Some(format!("that's Gizai's data folder ({}), or inside it", places.show(d)));
        }
        if folded.starts_with(p) {
            return Some(format!("it holds Gizai's data folder ({})", places.show(d)));
        }
    }
    let rest = p.strip_prefix(home).ok().filter(|_| has_home)?;
    let first = rest.components().next().and_then(|c| c.as_os_str().to_str()).unwrap_or_default();
    let key = KEY_FOLDERS.iter().map(Path::new).find(|k| rest.starts_with(fold(k)) || fold(k).starts_with(rest))
        .map(|k| k.display().to_string())
        .or_else(|| first.starts_with(".claude").then(|| first.to_string()))?;
    Some(format!("it holds keys or logins (~{MAIN_SEPARATOR}{key})"))
}

/// The folder `raw` normalized, or why it can't be an agent's: checked as typed, and as it is on disk when a link
/// leads elsewhere.
fn refused(raw: &str, places: &Places) -> std::result::Result<PathBuf, String> {
    if raw.trim().is_empty() {
        return Err("give the folder's path".into());
    }
    if raw.chars().any(char::is_control) || raw.contains(ODD) {
        return Err(format!("Gizai can't pass a path with {ODD_SAID} to the coding CLIs: rename the folder or pick another"));
    }
    let p = normalize(raw, &places.home).ok_or_else(|| FULL_PATH.to_string())?;
    if let Some(why) = refusal(&p, places) {
        return Err(why);
    }
    if let Some(real) = on_disk(&p) {
        if real != p {
            if let Some(why) = refusal(&real, places) {
                return Err(format!("it leads to {}: {why}", places.show(&real)));
            }
        }
    }
    Ok(p)
}

/// "Read and change" in a project's main checkout: a warning, since agents normally never touch it.
fn checkout_warning(p: &Path, places: &Places) -> Option<String> {
    let real = |p: &Path| on_disk(p).unwrap_or_else(|| p.to_path_buf());
    let p = real(p);
    places.checkouts.iter().find_map(|(name, c)| {
        let c = real(c);
        let how = if p == c { "This is" } else if p.starts_with(&c) { "This is inside" } else if c.starts_with(&p) { "This holds" } else { return None };
        Some(format!("{how} {name}'s main checkout: agents normally never touch it, since they work in a worktree of their own"))
    })
}

/// Each folder of the list as the form shows it: the reason it's refused, or a warning (it isn't there now, or it is a
/// read and change folder in a project's main checkout). A folder that isn't there may still be saved.
pub fn check(list: &[Folder], places: &Places) -> Vec<FolderCheck> {
    let paths: Vec<Option<PathBuf>> = list.iter().map(|f| normalize(&f.path, &places.home)).collect();
    let mut out: Vec<FolderCheck> = vec![];
    for (i, f) in list.iter().enumerate() {
        let mut c = FolderCheck { path: f.path.trim().to_string(), error: None, warning: None };
        let p = match refused(&f.path, places) {
            Ok(p) => p,
            Err(why) => {
                c.error = Some(why);
                out.push(c);
                continue;
            }
        };
        c.path = p.display().to_string();
        // Claude Code's rule for a read folder would keep a read and change folder inside it read only too.
        let inside_read = list.iter().zip(&paths).enumerate()
            .find(|(j, (o, op))| *j != i && f.change() && !o.change() && op.as_ref().is_some_and(|op| p.starts_with(op) && *op != p))
            .and_then(|(_, (_, op))| op.clone());
        c.error = if !ACCESS.contains(&f.access.as_str()) {
            Some(format!("pick read or read and change, not {}", f.access))
        } else if p.is_file() {
            Some("that's a file: pick a folder".into())
        } else if i >= MAX {
            Some(format!("an agent has at most {MAX} folders"))
        } else if paths[..i].iter().any(|o| o.as_ref() == Some(&p)) {
            Some("it's in the list twice".into())
        } else if let Some(o) = inside_read {
            Some(format!("it's inside {}, which is set to read, so it would stay read only: set that one to read and change, or remove one of them",
                         places.show(&o)))
        } else {
            None
        };
        if c.error.is_none() {
            c.warning = if !p.exists() {
                Some("It isn't there now: runs go without it until it is".into())
            } else if f.change() {
                checkout_warning(&p, places)
            } else {
                None
            };
        }
        out.push(c);
    }
    out
}

/// The list as it is saved (paths absolute, `~/` expanded), or why it can't be: the first refused folder and the
/// reason.
pub fn clean(list: &[Folder], places: &Places) -> Result<Vec<Folder>> {
    let list: Vec<Folder> = list.iter().filter(|f| !f.path.trim().is_empty()).cloned().collect();
    let checks = check(&list, places);
    if let Some(c) = checks.iter().find(|c| c.error.is_some()) {
        return Err(Error::Invalid(format!("Folders: {} can't be used: {}", c.path, c.error.clone().unwrap_or_default())));
    }
    Ok(list.into_iter().zip(checks).map(|(f, c)| Folder { path: c.path, access: f.access }).collect())
}

/// The folders a run gets: each one as it is on disk (links followed). A folder that is missing, or that is refused by
/// now, is left out, with a note for the run log.
pub fn for_run(list: &[Folder], places: &Places) -> (Vec<Folder>, Vec<String>) {
    let mut out = vec![];
    let mut notes = vec![];
    for f in list {
        match refused(&f.path, places) {
            Err(why) => notes.push(format!("Skipped the folder {}: {why}.", f.path)),
            Ok(p) if !p.is_dir() => notes.push(format!("Skipped the folder {}: it's missing, so this run goes without it.", f.path)),
            Ok(p) => {
                let real = on_disk(&p).unwrap_or(p).display().to_string();
                if !out.iter().any(|o: &Folder| o.path == real) {
                    out.push(Folder { path: real, access: f.access.clone() });
                }
            }
        }
    }
    (out, notes)
}

/// Whether the Team Lead may update `folder` (a project's linked folder) with `update_checkout`, after you said yes in
/// the chat. Not when the closest folder of its list that holds it is set to read; yes when that one is set to read and
/// change, and when no folder of its list holds it.
pub fn may_update(list: &[Folder], folder: &Path, places: &Places) -> std::result::Result<(), String> {
    let real = |p: &Path| on_disk(p).unwrap_or_else(|| p.to_path_buf());
    let target = real(folder);
    let closest = list.iter()
        .filter_map(|f| Some((real(&normalize(&f.path, &places.home)?), f)))
        .filter(|(p, _)| target.starts_with(p))
        .max_by_key(|(p, _)| p.components().count());
    match closest {
        Some((p, f)) if !f.change() => Err(format!(
            "{} is set to read in the Team Lead's folders (Team page → Team Lead → Folders): only read and change lets update_checkout update it, so nothing changed",
            places.show(&p))),
        _ => Ok(()),
    }
}
