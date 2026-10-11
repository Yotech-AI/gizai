//! The folders the Editor may open (GA-107), and Settings → General → Editor folders.
//! - The projects' linked folders; every folder directly in an Editor folder (`editor_roots`, `~/Herd` until you set
//!   your own), so `~/Herd/kade-portal` opens without being a project first; and the cards' worktrees,
//!   `<data>/worktrees/<KEY>/<identifier>`.
//! - Never with the refusals agent folders get (`folders`): `/`, your home folder and the folders above it, the folders
//!   that hold keys or logins, and Gizai's data folder apart from the card worktrees in it, so not the Team Lead's copies
//!   either. A folder that leads somewhere else through a link is checked where it leads too.
//! - A card's worktree is read-only while an agent works on the card.
//! The app asks again on every call (`find`), so a folder the Editor may no longer open stops at once. What is inside a
//! folder (its files, git status, search) is gizai-agents' `editor`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::folders::{self, Places};
use crate::{Error, Result, settings};

/// The setting that holds the Editor folders.
pub const ROOTS: &str = "editor_roots";
/// The Editor folders until you set your own.
pub const DEFAULT_ROOTS: [&str; 1] = ["~/Herd"];

/// What to pick instead, when an Editor folder is refused.
const ROOT_WANTED: &str = "the folder that holds your projects";
/// What to pick instead, when a folder to open is refused.
const FOLDER_WANTED: &str = "a project's own folder";
/// The refusal of an Editor folder that isn't a full path.
const FULL_PATH: &str = if cfg!(windows) { r"give the full path, like ~\Herd or D:\Projects" } else { "give the full path, like ~/Herd or /srv/projects" };

/// Where a folder the Editor may open comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A project's linked folder.
    Project,
    /// A folder directly in one of the Editor folders.
    Root,
    /// A card's worktree.
    Card,
}

/// A folder the Editor may open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorFolder {
    /// The folder's canonical path (links followed): how the screen names the folder in every other call.
    pub id: String,
    /// As shown, `~` for the home folder.
    pub path: String,
    /// Its own name, or the card's identifier.
    pub name: String,
    pub kind: Kind,
    /// The project whose linked folder it is, or whose card's worktree.
    pub project_id: Option<String>,
    /// The card whose worktree it is.
    pub task_id: Option<String>,
    /// A card an agent is working on now: nothing in it may change.
    pub read_only: bool,
    /// Why, as the screen says it: "Backend Agent is working on GA-12".
    pub read_only_why: Option<String>,
}

/// The Editor folders as saved (written the way Settings shows them, `~/Herd`), or `DEFAULT_ROOTS` until you set your
/// own. An empty list stays empty.
pub fn roots(db: &Db) -> Vec<String> {
    settings::get::<Vec<String>>(db, ROOTS).ok().flatten().unwrap_or_else(|| DEFAULT_ROOTS.map(String::from).to_vec())
}

/// The Editor folders as they are saved: without empty lines, each one written the way Settings shows it (`~/Herd` for
/// a folder in your home folder); or why one can't be used, with `folders`' refusals. A folder that isn't there yet may
/// be saved: it has no folders to open until it is.
pub fn clean_roots(list: &[String], places: &Places) -> Result<Vec<String>> {
    let mut out = vec![];
    let mut seen: Vec<PathBuf> = vec![];
    for raw in list.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
        let bad = |why: &str| Error::Invalid(format!("Editor folders: {raw} can't be used: {why}"));
        let p = folders::normalize(raw, &places.home).ok_or_else(|| bad(FULL_PATH))?;
        if let Some(why) = refusal(&p, places, ROOT_WANTED) {
            return Err(bad(&why));
        }
        if p.is_file() {
            return Err(bad("that's a file: pick a folder"));
        }
        if seen.contains(&p) {
            return Err(bad("it's in the list twice"));
        }
        out.push(places.show(&p));
        seen.push(p);
    }
    Ok(out)
}

/// Checks the Editor folders (`clean_roots`) and saves them; returns them as saved.
pub fn set_roots(db: &Db, list: &[String], places: &Places) -> Result<Vec<String>> {
    let roots = clean_roots(list, places)?;
    settings::set(db, ROOTS, &roots)?;
    Ok(roots)
}

/// Every folder the Editor may open now: the projects' linked folders, then the folders in the Editor folders, then the
/// cards' worktrees (in `worktrees`, Gizai's `<data>/worktrees`), each group by name. A folder shows once: as a project's
/// when it is one. A folder that isn't there, or that is refused, is left out. `working`: the cards an agent works on
/// now, with that agent's name; their worktrees are read-only.
pub fn folders(db: &Db, places: &Places, worktrees: &Path, working: &HashMap<String, String>) -> Result<Vec<EditorFolder>> {
    let mut out: Vec<EditorFolder> = vec![];
    let mut taken: HashSet<String> = HashSet::new();
    let mut projects: Vec<EditorFolder> = linked(db)?.into_iter().filter_map(|(project_id, raw)| {
        let p = folders::normalize(&raw, &places.home)?;
        let (id, name) = usable(&p, places)?;
        Some(EditorFolder { id, path: places.show(&p), name, kind: Kind::Project, project_id: Some(project_id), task_id: None,
                            read_only: false, read_only_why: None })
    }).collect();
    by_name(&mut projects);
    let mut in_roots = vec![];
    for root in roots(db).iter().filter_map(|r| folders::normalize(r, &places.home)) {
        if refusal(&root, places, ROOT_WANTED).is_some() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for e in entries.flatten() {
            // a hidden folder (.cache, .idea) is no project
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let p = root.join(e.file_name());
            if let Some((id, name)) = usable(&p, places) {
                in_roots.push(EditorFolder { id, path: places.show(&p), name, kind: Kind::Root, project_id: None, task_id: None,
                                             read_only: false, read_only_why: None });
            }
        }
    }
    by_name(&mut in_roots);
    let mut cards = card_worktrees(db, worktrees)?.into_iter().map(|(c, p, id)| {
        let why = working.get(&c.task_id).map(|agent| format!("{agent} is working on {}", c.identifier));
        EditorFolder { id, path: places.show(&p), name: c.identifier, kind: Kind::Card, project_id: Some(c.project_id),
                       task_id: Some(c.task_id), read_only: why.is_some(), read_only_why: why }
    }).collect::<Vec<_>>();
    cards.sort_by_cached_key(|f| identifier_order(&f.name));
    for f in projects.into_iter().chain(in_roots).chain(cards) {
        if taken.insert(f.id.clone()) {
            out.push(f);
        }
    }
    Ok(out)
}

/// The folder the screen opened as `id` (an `EditorFolder::id`), when the Editor may open it now; else why not, in a
/// plain sentence. Every Editor command asks this first.
pub fn find(db: &Db, places: &Places, worktrees: &Path, working: &HashMap<String, String>, id: &str) -> std::result::Result<EditorFolder, String> {
    if let Some(f) = folders(db, places, worktrees, working).map_err(|e| e.to_string())?.into_iter().find(|f| f.id == id) {
        return Ok(f);
    }
    let Some(p) = folders::normalize(id, &places.home).filter(|_| !id.trim().is_empty()) else {
        return Err("Pick one of the Editor's folders to open".into());
    };
    let shown = places.show(&p);
    if let Some(why) = refusal(&p, places, FOLDER_WANTED) {
        return Err(format!("The Editor can't open {shown}: {why}"));
    }
    if !p.is_dir() {
        return Err(format!("The Editor can't open {shown}: it isn't there any more"));
    }
    Err(format!("The Editor can't open {shown}: it isn't a project's folder, a folder in one of the Editor folders (Settings → General), \
                 or a card's worktree"))
}

/// Why the Editor can't use the folder `p` (absolute, normalized), with `folders`' refusals and `wanted` as what to pick
/// instead: checked as it is written and, when a link leads elsewhere, where it leads. None when it can.
fn refusal(p: &Path, places: &Places, wanted: &str) -> Option<String> {
    if let Some(why) = folders::refusal_in(p, places, wanted) {
        return Some(why);
    }
    let real = folders::on_disk(p)?;
    if real != p
        && let Some(why) = folders::refusal_in(&real, places, wanted)
    {
        return Some(format!("it leads to {}: {why}", places.show(&real)));
    }
    None
}

/// A project's folder or one in an Editor folder that the Editor may open: (its canonical path, its own name). None
/// when it isn't a folder on this disk, or is refused.
fn usable(p: &Path, places: &Places) -> Option<(String, String)> {
    if !p.is_dir() || refusal(p, places, FOLDER_WANTED).is_some() {
        return None;
    }
    let real = folders::on_disk(p)?;
    let name = p.file_name()?.to_string_lossy().to_string();
    Some((real.display().to_string(), name))
}

/// The projects with a linked folder: (project id, the folder as saved).
fn linked(db: &Db) -> Result<Vec<(String, String)>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT p.id, r.local_path FROM projects p JOIN repos r ON r.project_id = p.id AND r.deleted_at IS NULL
             WHERE p.deleted_at IS NULL AND coalesce(r.local_path, '') <> ''")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// A card with a branch, which may have a worktree.
struct Card {
    task_id: String,
    identifier: String,
    project_id: String,
}

/// The cards whose worktree is there, in `worktrees/<KEY>/<identifier>` as a run makes it: (the card, the worktree, its
/// canonical path). Archived cards are left out, and so is a worktree that leads out of `worktrees` through a link.
fn card_worktrees(db: &Db, worktrees: &Path) -> Result<Vec<(Card, PathBuf, String)>> {
    let Some(base) = folders::on_disk(worktrees) else { return Ok(vec![]) };
    let cards: Vec<(Card, String)> = db.read(|c| {
        let mut st = c.prepare(
            "SELECT t.id, t.identifier, t.project_id, p.key FROM tasks t JOIN projects p ON p.id = t.project_id
             WHERE t.deleted_at IS NULL AND p.deleted_at IS NULL AND coalesce(t.branch, '') <> ''")?;
        Ok(st.query_map([], |r| Ok((Card { task_id: r.get(0)?, identifier: r.get(1)?, project_id: r.get(2)? }, r.get(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    Ok(cards.into_iter().filter_map(|(card, key)| {
        let p = worktrees.join(key).join(&card.identifier);
        if !p.is_dir() {
            return None;
        }
        let real = folders::on_disk(&p).filter(|r| r.starts_with(&base) && *r != base)?;
        Some((card, p, real.display().to_string()))
    }).collect())
}

/// Sorts folders by name, in any case.
fn by_name(list: &mut [EditorFolder]) {
    list.sort_by_cached_key(|f| (f.name.to_lowercase(), f.name.clone()));
}

/// A card identifier's place in a sorted list: by key, then by number (GA-9 before GA-10).
fn identifier_order(identifier: &str) -> (String, u64, String) {
    match identifier.rsplit_once('-') {
        Some((key, n)) if n.parse::<u64>().is_ok() => (key.to_string(), n.parse().unwrap_or(0), identifier.to_string()),
        _ => (identifier.to_string(), 0, identifier.to_string()),
    }
}
