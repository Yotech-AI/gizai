//! The Editor's commands (GA-107; contract C2 with the Editor page, F2): the folders it may open
//! (`gizai_core::editor`), what is in them (`gizai_agents::editor`), and Open in VS Code, Zed or PhpStorm. And a
//! person's card (GA-111; contract C4 with the card bar, F4): Open in editor, Commit, Push and Hand over (`hand`).
//! - Every command checks again that the Editor may open the folder (a project's linked folder, a folder in the Editor
//!   folders, a card's worktree), so one that may not any more stops at once.
//! - A card's worktree is read-only while an agent works on the card: a live run, or a run getting its worktree ready.
//! - The commands run off the window's thread: git and the disk can take a moment.
//!
//! Behind main (GA-110; contract C3 with the Editor's behind bar, F3): how a folder stands against main, Fetch and
//! Update (`gizai_agents::behind`).
//! - Main is where the project's cards start (`git::start_point`) for a project's folder or a card's worktree, and the
//!   origin remote's default branch for another folder. Fetch fetches it as a card start does (at most 60 s), but not
//!   again within a minute of a fetch that worked: the Editor's, a card start's, the Team Lead's copies' or your own.
//! - Update runs in the background: `checkout`'s checks without switching branch, then its fast-forward, and an install
//!   only for a lock file that changed; never the setup command, never while a card's worktree is prepared from the
//!   folder (`code::folder_lock`), one update per folder at a time (`code::begin_update`, shared with the Team Lead's).
//!   Its end goes to the screen as `editor-updated`.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use gizai_agents::behind::{self, End, Fetched, FolderMain, Main, Named};
use gizai_agents::checkout;
use gizai_agents::editor::{self as inside, AppId, DirEntry, EditorApp, Find, FolderGit, OpenedFile, SearchResult};
use gizai_agents::worktree::Commit;
use gizai_core::editor::{self as core_editor, EditorFolder, Kind};
use gizai_core::folders::Places;
use gizai_core::model::Project;
use tauri::State;

use crate::AppState;
use crate::hand::HandCard;
use crate::runs::Note;

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
    core_editor::folders(&st.db, &Places::without_checkouts(&st.db), &st.data_dir.join("worktrees"), &working(st)).map_err(|e| e.to_string())
}

/// The folder `id` (an `EditorFolder::id`), when the Editor may open it now; else why not.
pub fn folder(st: &AppState, id: &str) -> R<EditorFolder> {
    core_editor::find(&st.db, &Places::without_checkouts(&st.db), &st.data_dir.join("worktrees"), &working(st), id)
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
    inside::apps(&crate::runs::command_path(), &Places::without_checkouts(&st.db).home)
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
    let program = inside::find_app(id, &search, &Places::without_checkouts(&st.db).home)
        .ok_or_else(|| format!("Gizai can't find {} on this computer: it looks for {} on PATH", id.name(), id.programs().join(", ")))?;
    if id == AppId::PhpStorm && f.kind == Kind::Card {
        inside::keep_idea_out(dir)?;
    }
    inside::open_in(id, &program, dir, file.as_deref(), line, &search)
}

// ---- behind main (GA-110) ----

/// The Editor's fetches of main, by repository (its git folder, so a project's folder and its cards' worktrees share
/// one): one at a time, and what they found.
#[derive(Default)]
pub struct Fetches {
    repos: Mutex<HashMap<PathBuf, Arc<Repo>>>,
}

#[derive(Default)]
struct Repo {
    /// Held while main is fetched: a second Fetch waits for the first, and takes what it found.
    busy: Mutex<()>,
    seen: Mutex<Seen>,
}

/// What a repository's fetches found.
#[derive(Debug, Clone, Default)]
struct Seen {
    fetched: Fetched,
    /// When the last fetch ended, whether it worked or not.
    ended: Option<Instant>,
}

impl Fetches {
    fn repo(&self, git_folder: &Path) -> Arc<Repo> {
        self.repos.lock().unwrap().entry(git_folder.to_path_buf()).or_default().clone()
    }
}

/// The folder as Update's words name it.
fn named(f: &EditorFolder) -> Named<'_> {
    Named { dir: Path::new(&f.id), name: &f.name, card: f.kind == Kind::Card }
}

/// Why nothing in the folder may change now (an agent works on the card); None when it may.
fn read_only(f: &EditorFolder) -> Option<&str> {
    f.read_only.then(|| f.read_only_why.as_deref().unwrap_or("An agent is working on it"))
}

/// Where the folder `f`'s main is, and the project whose cards start there: for a project's folder or a card's
/// worktree, where the project's cards start (`git::start_point`, as last fetched); for another folder, its origin
/// remote's default branch. Err: why it has none, as the sentence after "Update can't run:".
fn main_of(st: &AppState, f: &EditorFolder) -> Result<(Main, Option<Project>), String> {
    let dir = Path::new(&f.id);
    let Some(project_id) = f.project_id.as_deref() else {
        return behind::origin_main(dir).map(|m| (m, None))
            .ok_or_else(|| format!("{} has no origin remote: Gizai can't tell whether it's behind.", f.name));
    };
    let p = gizai_core::projects::get(&st.db, project_id).map_err(|e| e.to_string())?;
    if p.repo_url.as_deref().is_none_or(|u| u.trim().is_empty()) {
        return Err(format!("{} has no link to GitHub, Bitbucket or another git URL, so Gizai can't tell whether {} is behind. \
                            Add one on the project's page (Edit).", p.name, f.name));
    }
    let rev = crate::git::start_point(&p, dir, None).map_err(crate::pulls::plain)?;
    let label = crate::changes::label_of(&rev, &p.default_branch);
    Ok((Main { rev, label, branch: p.default_branch.clone() }, Some(p)))
}

/// What Gizai saw of its fetches of main in the repository of `dir`.
fn seen(st: &AppState, dir: &Path) -> Fetched {
    gizai_agents::copies::git_folder(dir).map(|r| st.editor.repo(&r).seen.lock().unwrap().fetched.clone()).unwrap_or_default()
}

/// Update's plan for the folder `f` (`behind::plan`), or why it can't run.
fn plan_of(st: &AppState, f: &EditorFolder) -> Result<checkout::Plan, String> {
    let main = main_of(st, f);
    behind::plan(named(f), main.as_ref().map(|(m, _)| m).map_err(String::as_str), read_only(f))
}

/// How the folder stands against main as last fetched (local only).
pub fn main(st: &AppState, folder: &str) -> R<FolderMain> {
    let f = self::folder(st, folder)?;
    let dir = Path::new(&f.id);
    let main = main_of(st, &f);
    let updating = crate::code::updating(st, dir);
    Ok(behind::stand(named(&f), main.as_ref().map(|(m, _)| m).map_err(String::as_str), &seen(st, dir), updating.as_deref(),
                     read_only(&f)))
}

/// Fetches main the way a card start does (at most 60 s), then says how the folder stands (`main`). Not again within a
/// minute of a fetch that worked (the Editor's, a card start's, the Team Lead's copies' or your own); a Fetch that
/// waited for another one of the same repository takes what that one found. A folder without a main (outside git,
/// without a remote) has nothing to fetch.
pub fn fetch(st: &AppState, folder: &str) -> R<FolderMain> {
    let f = self::folder(st, folder)?;
    let dir = Path::new(&f.id);
    if let Ok((main, project)) = main_of(st, &f)
        && behind::is_git(dir)
        && let Some(repo) = gizai_agents::copies::git_folder(dir)
    {
        let slot = st.editor.repo(&repo);
        let asked = Instant::now();
        let _busy = slot.busy.lock().unwrap();
        let seen = slot.seen.lock().unwrap().clone();
        let waited = seen.ended.is_some_and(|t| t >= asked);
        let last = [seen.fetched.ok_at, behind::fetched_at(dir, &main.branch)].into_iter().flatten().max();
        let fresh = last.is_some_and(|t| t.elapsed().is_ok_and(|age| age < crate::code::fetch_every(st)));
        if !waited && !fresh {
            let limit = crate::git::START_FETCH_LIMIT;
            let fetched = match &project {
                Some(p) => crate::git::start_point(p, dir, Some(limit)).map(drop).map_err(crate::pulls::plain),
                None => behind::fetch_origin_main(dir, limit).map(drop),
            };
            let mut s = slot.seen.lock().unwrap();
            match fetched {
                Ok(()) => {
                    s.fetched = Fetched { ok_at: Some(SystemTime::now()), failed: None };
                    // the Team Lead's copy of the project needn't fetch again for a minute either
                    if let Some(p) = &project {
                        crate::code::fetched(st, &p.key);
                    }
                }
                Err(why) => s.fetched.failed = Some((SystemTime::now(), why)),
            }
            s.ended = Some(Instant::now());
        }
    }
    main(st, folder)
}

/// Starts Update in the background: checks first (`behind::plan`) and returns what it will do, or why not (nothing
/// changed). One update per folder at a time, the Team Lead's included. Its turn comes once no card's worktree is being
/// prepared from the folder (`code::folder_lock`); everything is checked again then, and `checkout::update` runs. Its
/// end goes to the screen as `editor-updated` (`Note::EditorUpdated`), when the folder no longer counts as updating.
pub async fn update(st: &AppState, folder: &str) -> R<String> {
    let st = st.clone();
    let (f, running, will) = {
        let (st, id) = (st.clone(), folder.to_string());
        tokio::task::spawn_blocking(move || -> R<_> {
            let f = self::folder(&st, &id)?;
            let running = crate::code::begin_update(&st, Path::new(&f.id))
                .ok_or_else(|| format!("An update of {} is already running.", f.name))?;
            let will = plan_of(&st, &f)?.said();
            running.says(&will);
            Ok((f, running, will))
        }).await.map_err(|e| e.to_string())??
    };
    tokio::spawn(async move {
        let lock = crate::code::folder_lock(&st, Path::new(&f.id));
        let _held = lock.lock().await;
        let (st2, id) = (st.clone(), f.id.clone());
        let end = tokio::task::spawn_blocking(move || update_now(&st2, &id)).await.unwrap_or_else(|e| End::Refused(e.to_string()));
        drop(running);
        (st.notify)(Note::EditorUpdated(behind::ended(&f.id, &f.name, end)));
    });
    Ok(will)
}

/// The update's turn: the folder may have changed while it waited, so the checks run again before `checkout::update`.
fn update_now(st: &AppState, id: &str) -> End {
    let plan = match self::folder(st, id).and_then(|f| plan_of(st, &f)) {
        Ok(p) => p,
        Err(why) => return End::Refused(why),
    };
    let path = crate::runs::command_path();
    match checkout::update(Path::new(id), &plan, Some(path.as_os_str())) {
        Ok(done) => End::Done(plan, done),
        Err(stopped) => End::Stopped(plan, stopped),
    }
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

// ---- contract C3's commands (GA-110) ----

#[tauri::command]
pub async fn editor_main(st: State<'_, AppState>, folder: String) -> R<FolderMain> {
    off(&st, move |st| main(st, &folder)).await
}

#[tauri::command]
pub async fn editor_fetch(st: State<'_, AppState>, folder: String) -> R<FolderMain> {
    off(&st, move |st| fetch(st, &folder)).await
}

#[tauri::command]
pub async fn editor_update(st: State<'_, AppState>, folder: String) -> R<String> {
    update(&st, &folder).await
}

// ---- a person's card, word for word as contract C4 names them (GA-111; the work is in `hand`) ----

#[tauri::command]
pub async fn start_by_hand(st: State<'_, AppState>, task_id: String) -> R<HandCard> {
    crate::hand::start(&st, &task_id).await
}

#[tauri::command]
pub async fn hand_card(st: State<'_, AppState>, task_id: String) -> R<HandCard> {
    crate::hand::card(&st, &task_id).await
}

#[tauri::command]
pub async fn hand_commit(st: State<'_, AppState>, task_id: String, message: String) -> R<Commit> {
    crate::hand::commit(&st, &task_id, &message).await
}

#[tauri::command]
pub async fn hand_push(st: State<'_, AppState>, task_id: String) -> R<String> {
    crate::hand::push(&st, &task_id).await
}

#[tauri::command]
pub async fn hand_over(st: State<'_, AppState>, task_id: String) -> R<String> {
    crate::hand::over(&st, &task_id).await
}
