//! The Editor's commands (GA-107; contract C2 with the Editor page, F2): the folders it may open
//! (`gizai_core::editor`), what is in them (`gizai_agents::editor`), and Open in VS Code, Zed or PhpStorm.
//! - Every command checks again that the Editor may open the folder (a project's linked folder, a folder in the Editor
//!   folders, a card's worktree), so one that may not any more stops at once.
//! - A card's worktree is read-only while an agent works on the card: a live run, or a run getting its worktree ready.
//! - The commands run off the window's thread: git and the disk can take a moment.
use std::collections::HashMap;
use std::path::Path;

use gizai_agents::editor::{self as inside, AppId, DirEntry, EditorApp, Find, FolderGit, OpenedFile, SearchResult};
use gizai_core::editor::{self as core_editor, EditorFolder, Kind};
use gizai_core::folders::Places;
use tauri::State;

use crate::AppState;

type R<T> = Result<T, String>;

/// The cards an agent works on now, with that agent's name ("An agent" while its run is still getting ready).
fn working(st: &AppState) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for r in crate::runs::live(st) {
        let name = gizai_core::team::agent(&st.db, &r.agent_id).map(|m| m.name).unwrap_or_else(|_| "An agent".into());
        out.insert(r.task_id, name);
    }
    for task_id in crate::runs::starting(st) {
        out.entry(task_id).or_insert_with(|| "An agent".into());
    }
    out
}

/// The folders the Editor may open now: projects' linked folders, the folders in the Editor folders, card worktrees.
pub fn folders(st: &AppState) -> R<Vec<EditorFolder>> {
    core_editor::folders(&st.db, &Places::of(&st.db), &st.data_dir.join("worktrees"), &working(st)).map_err(|e| e.to_string())
}

/// The folder `id` (an `EditorFolder::id`), when the Editor may open it now; else why not.
pub fn folder(st: &AppState, id: &str) -> R<EditorFolder> {
    core_editor::find(&st.db, &Places::of(&st.db), &st.data_dir.join("worktrees"), &working(st), id)
}

/// The folder as `gizai_agents::editor` takes it: where it is, and why nothing in it may change now.
fn place(f: &EditorFolder) -> inside::Folder<'_> {
    let why = f.read_only.then(|| f.read_only_why.as_deref().unwrap_or("an agent is working on it"));
    inside::Folder { dir: Path::new(&f.id), read_only: why }
}

/// One folder level of `folder`, folders first; `dir` "" for the top.
pub fn list(st: &AppState, folder: &str, dir: &str) -> R<Vec<DirEntry>> {
    inside::list(Path::new(&self::folder(st, folder)?.id), dir)
}

pub fn read(st: &AppState, folder: &str, path: &str) -> R<OpenedFile> {
    inside::read(place(&self::folder(st, folder)?), path)
}

/// Saves `text` as the file `path`; refused with "changed on disk" when its version isn't `version` any more. Returns
/// the new version.
pub fn write(st: &AppState, folder: &str, path: &str, text: &str, version: &str) -> R<String> {
    inside::write(place(&self::folder(st, folder)?), path, text, version)
}

/// A new empty file at `path`, or a new folder (`dir`).
pub fn create(st: &AppState, folder: &str, path: &str, dir: bool) -> R<()> {
    inside::create(place(&self::folder(st, folder)?), path, dir)
}

pub fn rename(st: &AppState, folder: &str, from: &str, to: &str) -> R<()> {
    inside::rename(place(&self::folder(st, folder)?), from, to)
}

pub fn delete(st: &AppState, folder: &str, path: &str) -> R<()> {
    inside::delete(place(&self::folder(st, folder)?), path)
}

/// Go to file: tracked and untracked files git doesn't ignore.
pub fn files(st: &AppState, folder: &str) -> R<Vec<String>> {
    inside::files(Path::new(&self::folder(st, folder)?.id))
}

/// Search in the folder with `git grep`: at most 2,000 hits, at most 10 seconds.
pub fn search(st: &AppState, folder: &str, query: &str, case_sensitive: bool, whole_word: bool, regex: bool) -> R<SearchResult> {
    inside::search(Path::new(&self::folder(st, folder)?.id), query, Find { case_sensitive, whole_word, regex })
}

/// The branch and each changed file's git status (local, no fetch).
pub fn git(st: &AppState, folder: &str) -> R<FolderGit> {
    Ok(inside::status(Path::new(&self::folder(st, folder)?.id)))
}

/// The current version of each path ("" for a file that is gone).
pub fn versions(st: &AppState, folder: &str, paths: &[String]) -> R<Vec<String>> {
    Ok(inside::versions(Path::new(&self::folder(st, folder)?.id), paths))
}

/// VS Code, Zed and PhpStorm, those found on this computer under any of their names: on the PATH the coding CLIs get
/// (`runs::command_path`), in JetBrains Toolbox's scripts folder, and on macOS in /Applications.
pub fn apps(st: &AppState) -> Vec<EditorApp> {
    inside::apps(&crate::runs::command_path(), &Places::of(&st.db).home)
}

/// Opens `folder` in the editor `app` ("vscode", "zed" or "phpstorm"), or the file `path` in it, at `line` when given.
/// Before PhpStorm opens a card's worktree, `/.idea/` goes into the repository's `info/exclude`, so the project files
/// it adds never become part of the card.
pub fn open_in(st: &AppState, app: &str, folder: &str, path: Option<&str>, line: Option<u32>) -> R<()> {
    let id = AppId::parse(app).ok_or_else(|| format!("Gizai can't open a folder in {app}: pick VS Code, Zed or PhpStorm"))?;
    let f = self::folder(st, folder)?;
    let dir = Path::new(&f.id);
    let file = path.filter(|p| !p.trim().is_empty()).map(|p| inside::joined(dir, p)).transpose()?;
    let search = crate::runs::command_path();
    let program = inside::find_app(id, &search, &Places::of(&st.db).home)
        .ok_or_else(|| format!("Gizai can't find {} on this computer: it looks for {} on PATH", id.name(), id.programs().join(", ")))?;
    if id == AppId::PhpStorm && f.kind == Kind::Card {
        inside::keep_idea_out(dir)?;
    }
    inside::open_in(id, &program, dir, file.as_deref(), line, &search)
}

/// `f` on a thread that may wait for git and the disk, off the window's.
async fn off<T: Send + 'static>(st: &AppState, f: impl FnOnce(&AppState) -> R<T> + Send + 'static) -> R<T> {
    let st = st.clone();
    tauri::async_runtime::spawn_blocking(move || f(&st)).await.map_err(|e| e.to_string())?
}

// ---- the commands, word for word as contract C2 names them ----

#[tauri::command]
pub async fn editor_folders(st: State<'_, AppState>) -> R<Vec<EditorFolder>> {
    off(&st, folders).await
}

#[tauri::command]
pub async fn editor_list(st: State<'_, AppState>, folder: String, dir: String) -> R<Vec<DirEntry>> {
    off(&st, move |st| list(st, &folder, &dir)).await
}

#[tauri::command]
pub async fn editor_read(st: State<'_, AppState>, folder: String, path: String) -> R<OpenedFile> {
    off(&st, move |st| read(st, &folder, &path)).await
}

#[tauri::command]
pub async fn editor_write(st: State<'_, AppState>, folder: String, path: String, text: String, version: String) -> R<String> {
    off(&st, move |st| write(st, &folder, &path, &text, &version)).await
}

#[tauri::command]
pub async fn editor_create(st: State<'_, AppState>, folder: String, path: String, dir: bool) -> R<()> {
    off(&st, move |st| create(st, &folder, &path, dir)).await
}

#[tauri::command]
pub async fn editor_rename(st: State<'_, AppState>, folder: String, from: String, to: String) -> R<()> {
    off(&st, move |st| rename(st, &folder, &from, &to)).await
}

#[tauri::command]
pub async fn editor_delete(st: State<'_, AppState>, folder: String, path: String) -> R<()> {
    off(&st, move |st| delete(st, &folder, &path)).await
}

#[tauri::command]
pub async fn editor_files(st: State<'_, AppState>, folder: String) -> R<Vec<String>> {
    off(&st, move |st| files(st, &folder)).await
}

#[tauri::command]
pub async fn editor_search(st: State<'_, AppState>, folder: String, query: String, case_sensitive: bool, whole_word: bool, regex: bool)
    -> R<SearchResult> {
    off(&st, move |st| search(st, &folder, &query, case_sensitive, whole_word, regex)).await
}

#[tauri::command]
pub async fn editor_git(st: State<'_, AppState>, folder: String) -> R<FolderGit> {
    off(&st, move |st| git(st, &folder)).await
}

#[tauri::command]
pub async fn editor_versions(st: State<'_, AppState>, folder: String, paths: Vec<String>) -> R<Vec<String>> {
    off(&st, move |st| versions(st, &folder, &paths)).await
}

#[tauri::command]
pub async fn editor_apps(st: State<'_, AppState>) -> R<Vec<EditorApp>> {
    off(&st, |st| Ok(apps(st))).await
}

#[tauri::command]
pub async fn open_in_app(st: State<'_, AppState>, app: String, folder: String, path: Option<String>, line: Option<u32>) -> R<()> {
    off(&st, move |st| open_in(st, &app, &folder, path.as_deref(), line)).await
}
