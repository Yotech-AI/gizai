//! Thin command layer: every command calls gizai-core and maps errors to strings.
//! Write commands emit `rows-changed` so open screens refresh.
//!
//! Every command is async and does its work off the main thread, which also runs the window (GA-191): the database, files
//! and processes on a thread that may wait (`off`), what only reads memory on the async runtime. The few left on the main
//! thread say why.
use crate::AppState;
use gizai_core::model::*;
use gizai_core::{clients, comments, docs, files, projects, tasks, team, users};
use tauri::{AppHandle, Emitter, State};

type R<T> = Result<T, String>;

fn e(err: gizai_core::Error) -> String {
    err.to_string()
}

fn changed(app: &AppHandle, table: &str) {
    let _ = app.emit("rows-changed", serde_json::json!({ "table": table }));
}

/// Runs `f` with the app's state on a thread that may wait (for SQL, the database's lock, a file or a process): never on
/// the main thread, so the window doesn't stand still meanwhile, and never on an async worker.
async fn off<T: Send + 'static>(st: &AppState, f: impl FnOnce(&AppState) -> R<T> + Send + 'static) -> R<T> {
    let st = st.clone();
    tauri::async_runtime::spawn_blocking(move || f(&st)).await.map_err(|err| err.to_string())?
}

/// A person edited or reactivated the agent: it takes cards from the queue again (`runs::pull_paused`).
fn resume(st: &AppState, agent_id: &str) {
    crate::runs::resume_pull(st, agent_id);
    let st = st.clone();
    tauri::async_runtime::spawn(async move { crate::runs::pull(&st).await; });
}

/// After a card changes, the agents take their next cards (a card that landed in an Auto column, or was assigned).
fn wake(st: &AppState, task_id: &str) {
    let (st, id) = (st.clone(), task_id.to_string());
    tauri::async_runtime::spawn(async move { crate::runs::dispatch(&st, &id).await; });
}

/// A column changed (its agents, Auto, its cards): the agents take the cards that wait for them now.
fn pull_soon(st: &AppState) {
    let st = st.clone();
    tauri::async_runtime::spawn(async move { crate::runs::pull(&st).await; });
}

// ---- clients and users ----
#[tauri::command]
pub async fn list_clients(st: State<'_, AppState>) -> R<Vec<Client>> { off(&st, |st| clients::list(&st.db).map_err(e)).await }
#[tauri::command]
pub async fn get_client(st: State<'_, AppState>, id: String) -> R<Client> { off(&st, move |st| clients::get(&st.db, &id).map_err(e)).await }
#[tauri::command]
pub async fn save_client(app: AppHandle, st: State<'_, AppState>, id: Option<String>, input: ClientInput) -> R<String> {
    let out = off(&st, move |st| match id {
        Some(id) => clients::update(&st.db, &st.you_id, &id, input).map(|_| id),
        None => clients::create(&st.db, &st.you_id, input),
    }.map_err(e)).await?;
    changed(&app, "clients");
    Ok(out)
}
#[tauri::command]
pub async fn archive_client(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    off(&st, move |st| clients::archive(&st.db, &st.you_id, &id).map_err(e)).await?;
    changed(&app, "clients");
    Ok(())
}
#[tauri::command]
pub async fn list_contacts(st: State<'_, AppState>, client_id: String) -> R<Vec<Contact>> {
    off(&st, move |st| clients::contacts(&st.db, &client_id).map_err(e)).await
}
#[tauri::command]
pub async fn save_contact(app: AppHandle, st: State<'_, AppState>, contact: Contact) -> R<String> {
    let id = off(&st, move |st| clients::upsert_contact(&st.db, &st.you_id, contact).map_err(e)).await?;
    changed(&app, "contacts");
    Ok(id)
}
#[tauri::command]
pub async fn remove_contact(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    off(&st, move |st| clients::remove_contact(&st.db, &st.you_id, &id).map_err(e)).await?;
    changed(&app, "contacts");
    Ok(())
}
#[tauri::command]
pub async fn list_users(st: State<'_, AppState>) -> R<Vec<Person>> { off(&st, |st| users::list(&st.db).map_err(e)).await }
#[tauri::command]
pub async fn add_user(app: AppHandle, st: State<'_, AppState>, name: String, email: Option<String>) -> R<String> {
    let id = off(&st, move |st| users::create(&st.db, &st.you_id, &name, email.as_deref()).map_err(e)).await?;
    changed(&app, "actors");
    Ok(id)
}

// ---- projects ----
#[tauri::command]
pub async fn list_projects(st: State<'_, AppState>) -> R<Vec<Project>> { off(&st, |st| projects::list(&st.db).map_err(e)).await }
#[tauri::command]
pub async fn get_project(st: State<'_, AppState>, id: String) -> R<Project> { off(&st, move |st| projects::get(&st.db, &id).map_err(e)).await }
#[tauri::command]
pub async fn save_project(app: AppHandle, st: State<'_, AppState>, id: Option<String>, input: ProjectInput) -> R<String> {
    let out = off(&st, move |st| match id {
        Some(id) => projects::update(&st.db, &st.you_id, &id, input).map(|_| id),
        None => projects::create(&st.db, &st.you_id, input),
    }.map_err(e)).await?;
    changed(&app, "projects");
    Ok(out)
}

// ---- tasks ----
/// Without each card's acceptance criteria (null): only the card's own page shows them, and it asks `get_task` (GA-191).
#[tauri::command]
pub async fn list_tasks(st: State<'_, AppState>, filter: TaskFilter) -> R<Vec<Task>> {
    off(&st, move |st| tasks::list_lean(&st.db, &filter).map_err(e)).await
}
#[tauri::command]
pub async fn get_task(st: State<'_, AppState>, id: String) -> R<Task> { off(&st, move |st| tasks::get(&st.db, &id).map_err(e)).await }
#[tauri::command]
pub async fn create_task(app: AppHandle, st: State<'_, AppState>, input: TaskInput) -> R<String> {
    let id = off(&st, move |st| tasks::create(&st.db, &st.you_id, input).map_err(e)).await?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(id)
}
#[tauri::command]
pub async fn update_task(app: AppHandle, st: State<'_, AppState>, id: String, patch: TaskPatch) -> R<()> {
    let task = id.clone();
    off(&st, move |st| tasks::update(&st.db, &st.you_id, &task, patch).map_err(e)).await?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
#[tauri::command]
pub async fn move_task(app: AppHandle, st: State<'_, AppState>, id: String, state_id: String, sort_key: String) -> R<()> {
    let task = id.clone();
    off(&st, move |st| tasks::move_to(&st.db, &st.you_id, &task, &state_id, &sort_key).map_err(e)).await?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
#[tauri::command]
pub async fn set_task_labels(app: AppHandle, st: State<'_, AppState>, id: String, label_ids: Vec<String>) -> R<()> {
    let task = id.clone();
    off(&st, move |st| tasks::set_labels(&st.db, &st.you_id, &task, label_ids).map_err(e)).await?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
/// GA-228: the cards the card builds on and the cards that build on it (the card page's Builds on and Needed by).
#[tauri::command]
pub async fn get_builds_on(st: State<'_, AppState>, id: String) -> R<gizai_core::builds_on::BuildsOn> {
    off(&st, move |st| gizai_core::builds_on::of(&st.db, &id).map_err(e)).await
}
/// GA-228: the card builds on `on` (a card's identifier, like GA-226, or its id) too. A loop, more than 20 cards or the
/// card itself is refused with why.
#[tauri::command]
pub async fn add_builds_on(app: AppHandle, st: State<'_, AppState>, id: String, on: String) -> R<()> {
    let task = id.clone();
    off(&st, move |st| {
        let other = tasks::id_of(&st.db, &on).unwrap_or(on);
        gizai_core::builds_on::add(&st.db, &st.you_id, &task, &other).map_err(e)
    }).await?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
/// GA-228: the card no longer builds on `on`: when it waited only for that card, the queue may take it now.
#[tauri::command]
pub async fn remove_builds_on(app: AppHandle, st: State<'_, AppState>, id: String, on: String) -> R<()> {
    let task = id.clone();
    off(&st, move |st| gizai_core::builds_on::remove(&st.db, &st.you_id, &task, &on).map_err(e)).await?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
#[tauri::command]
pub async fn list_archived_tasks(st: State<'_, AppState>, project_id: Option<String>) -> R<Vec<Task>> {
    off(&st, move |st| tasks::archived(&st.db, project_id.as_deref().filter(|p| !p.is_empty())).map_err(e)).await
}
/// Archives a card in Done. Refused while an agent works on it, its run's worktree being made included.
#[tauri::command]
pub async fn archive_task(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    off(&st, move |st| {
        if crate::runs::working_on(st, &id) {
            return Err("An agent is working on this card".into());
        }
        tasks::archive(&st.db, &st.you_id, &id).map_err(e)
    }).await?;
    changed(&app, "tasks");
    Ok(())
}
#[tauri::command]
pub async fn restore_task(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    off(&st, move |st| tasks::restore(&st.db, &st.you_id, &id).map_err(e)).await?;
    changed(&app, "tasks");
    Ok(())
}
#[tauri::command]
pub async fn task_activity(st: State<'_, AppState>, task_id: String) -> R<Vec<ChangeEntry>> {
    off(&st, move |st| tasks::activity(&st.db, &task_id).map_err(e)).await
}
#[tauri::command]
pub async fn list_comments(st: State<'_, AppState>, task_id: String) -> R<Vec<Comment>> {
    off(&st, move |st| comments::list(&st.db, &task_id).map_err(e)).await
}
#[tauri::command]
pub async fn add_comment(app: AppHandle, st: State<'_, AppState>, task_id: String, body_md: String) -> R<String> {
    let id = off(&st, move |st| comments::add(&st.db, &st.you_id, &task_id, &body_md, None).map_err(e)).await?;
    changed(&app, "comments");
    Ok(id)
}

// ---- teams ----
#[tauri::command]
pub async fn list_teams(st: State<'_, AppState>) -> R<Vec<team::TeamSummary>> { off(&st, |st| team::list(&st.db).map_err(e)).await }
/// Without each agent's instructions (null): only an agent's page and form show them, and they ask `get_agent` (GA-191).
#[tauri::command]
pub async fn get_team(st: State<'_, AppState>, id: Option<String>) -> R<team::Team> {
    off(&st, move |st| {
        let id = match id {
            Some(i) => i,
            None => team::list(&st.db).map_err(e)?.first().map(|t| t.id.clone()).ok_or("no team")?,
        };
        team::get_lean(&st.db, &id).map_err(e)
    }).await
}

// ---- git ----
/// Runs up to four git processes (`git status` among them), so on a thread that may wait for them.
#[tauri::command]
pub async fn check_repo(path: String) -> R<crate::git::RepoCheck> {
    tauri::async_runtime::spawn_blocking(move || crate::git::repo_check(std::path::Path::new(&path))).await.map_err(|err| err.to_string())
}

// ---- docs ----
#[tauri::command]
pub async fn list_docs(st: State<'_, AppState>, project_id: String) -> R<Vec<Doc>> {
    off(&st, move |st| docs::list(&st.db, &project_id).map_err(e)).await
}
#[tauri::command]
pub async fn get_doc(st: State<'_, AppState>, id: String) -> R<Doc> { off(&st, move |st| docs::get(&st.db, &id).map_err(e)).await }
#[tauri::command]
pub async fn create_doc(app: AppHandle, st: State<'_, AppState>, project_id: String, title: String) -> R<String> {
    let id = off(&st, move |st| docs::create(&st.db, &st.you_id, &project_id, &title).map_err(e)).await?;
    changed(&app, "docs");
    Ok(id)
}
#[tauri::command]
pub async fn save_doc(app: AppHandle, st: State<'_, AppState>, id: String, body_md: String, base_version: i64) -> R<i64> {
    let v = off(&st, move |st| docs::save(&st.db, &st.you_id, &id, &body_md, base_version).map_err(e)).await?;
    changed(&app, "docs");
    Ok(v)
}
#[tauri::command]
pub async fn rename_doc(app: AppHandle, st: State<'_, AppState>, id: String, title: String) -> R<()> {
    off(&st, move |st| docs::rename(&st.db, &st.you_id, &id, &title).map_err(e)).await?;
    changed(&app, "docs");
    Ok(())
}

// ---- memory (GA-19; the Memory page is GA-68) ----
/// An agent's own notes in Memory: the Team Lead's `Team Lead/Notes` (made the first time), another agent's
/// `Agents/<name>/Notes`, or for an agent that shares another agent's folder (GA-96) that folder's; None when it has none.
#[tauri::command]
pub async fn agent_notes(app: AppHandle, st: State<'_, AppState>, agent_id: String) -> R<Option<gizai_core::memory::Note>> {
    use gizai_core::memory::{self, Who};
    let (note, made) = off(&st, move |st| {
        let agent = gizai_core::team::agent(&st.db, &agent_id).map_err(e)?;
        let you = Who::Person(st.you_id.clone());
        if agent.is_lead || agent.chat_enabled {
            let you_name = gizai_core::users::list(&st.db).ok().and_then(|l| l.into_iter().find(|p| p.id == st.you_id)).map(|p| p.name)
                .unwrap_or_else(|| "the user".into());
            let made = memory::find(&st.db, &memory::lead_notes_path()).map_err(e)?.is_none();
            let id = memory::ensure_lead_notes(&st.db, &agent_id, &you_name).map_err(e)?;
            return Ok((Some(memory::get(&st.db, &you, &id).map_err(e)?), made));
        }
        let owner = agent.shares_memory_with.unwrap_or(agent_id);
        let notes: Vec<memory::Note> = memory::list(&st.db, &you).map_err(e)?.into_iter()
            .filter(|n| n.owner_id.as_deref() == Some(owner.as_str()) && n.title().eq_ignore_ascii_case(memory::NOTES)).collect();
        // The one right in its folder first.
        Ok((notes.iter().find(|n| n.path.split('/').count() == 3).or(notes.first()).cloned(), false))
    }).await?;
    if made {
        changed(&app, "docs");
    }
    Ok(note)
}
/// The Memory page (GA-68) works as you, a person: you read and write every note.
fn as_you(st: &AppState) -> gizai_core::memory::Who { gizai_core::memory::Who::Person(st.you_id.clone()) }
/// Every note by path; with their text when `text` (the Memory page's tree, links, tags and previews).
#[tauri::command]
pub async fn memory_notes(st: State<'_, AppState>, text: bool) -> R<Vec<gizai_core::memory::Note>> {
    off(&st, move |st| {
        let you = as_you(st);
        if text { gizai_core::memory::list_with_text(&st.db, &you) } else { gizai_core::memory::list(&st.db, &you) }.map_err(e)
    }).await
}
/// A new note at `path` (folder and title) with `body_md` as its first version. Refused when the path is taken, in a
/// folder memory doesn't have, or with a secret in it.
#[tauri::command]
pub async fn memory_create(app: AppHandle, st: State<'_, AppState>, path: String, body_md: String) -> R<gizai_core::memory::Saved> {
    let saved = off(&st, move |st| {
        let you = as_you(st);
        if gizai_core::memory::find(&st.db, path.trim().trim_matches('/')).map_err(e)?.is_some() {
            return Err(format!("{} exists already", path.trim().trim_matches('/')));
        }
        gizai_core::memory::write(&st.db, &you, &path, &body_md, None, None).map_err(e)
    }).await?;
    changed(&app, "docs");
    Ok(saved)
}
/// Moves (renames) note `id` to `to`: a whole path, or a folder ending in `/` that keeps its title. The links to it follow.
#[tauri::command]
pub async fn memory_move(app: AppHandle, st: State<'_, AppState>, id: String, to: String) -> R<gizai_core::memory::Note> {
    let n = off(&st, move |st| gizai_core::memory::move_note(&st.db, &as_you(st), &id, &to, false).map_err(e)).await?;
    changed(&app, "docs");
    Ok(n)
}
/// Searches the notes: words, "phrases", path:Folder and tag:name (`memory::search`).
#[tauri::command]
pub async fn memory_search(st: State<'_, AppState>, query: String, limit: Option<usize>) -> R<Vec<gizai_core::memory::Hit>> {
    off(&st, move |st| gizai_core::memory::search(&st.db, &as_you(st), &query, limit.unwrap_or(50)).map_err(e)).await
}
/// The notes by their last saved version, newest first: who wrote each (a person, an agent, a run) and when.
#[tauri::command]
pub async fn memory_recent(st: State<'_, AppState>, limit: Option<usize>) -> R<Vec<gizai_core::memory::Change>> {
    off(&st, move |st| gizai_core::memory::recent(&st.db, &as_you(st), limit.unwrap_or(30)).map_err(e)).await
}
/// Memory for every agent (Settings → Runs): on unless switched off.
#[tauri::command]
pub async fn memory_enabled(st: State<'_, AppState>) -> R<bool> { off(&st, |st| Ok(gizai_core::memory::enabled(&st.db))).await }
#[tauri::command]
pub async fn set_memory_enabled(app: AppHandle, st: State<'_, AppState>, on: bool) -> R<()> {
    off(&st, move |st| gizai_core::memory::set_enabled(&st.db, on).map_err(e)).await?;
    changed(&app, "settings");
    Ok(())
}
/// Agents ask the Team Lead before you (Settings → Runs, GA-70): on unless switched off.
#[tauri::command]
pub async fn ask_lead_enabled(st: State<'_, AppState>) -> R<bool> { off(&st, |st| Ok(gizai_core::questions::enabled(&st.db))).await }
#[tauri::command]
pub async fn set_ask_lead_enabled(app: AppHandle, st: State<'_, AppState>, on: bool) -> R<()> {
    off(&st, move |st| gizai_core::questions::set_enabled(&st.db, on).map_err(e)).await?;
    changed(&app, "settings");
    Ok(())
}

#[tauri::command]
pub async fn doc_versions(st: State<'_, AppState>, id: String) -> R<Vec<DocVersion>> {
    off(&st, move |st| docs::versions(&st.db, &id).map_err(e)).await
}
#[tauri::command]
pub async fn doc_version_body(st: State<'_, AppState>, id: String, version: i64) -> R<String> {
    off(&st, move |st| docs::version_body(&st.db, &id, version).map_err(e)).await
}

// ---- files ----
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFilesResult {
    pub added: Vec<FileRow>,
    /// One plain sentence per file that could not be added.
    pub failed: Vec<String>,
}

/// Async so copying large files never blocks the window.
#[tauri::command]
pub async fn add_files(app: AppHandle, st: State<'_, AppState>, owner_type: String, owner_id: String, paths: Vec<String>) -> R<AddFilesResult> {
    let (db, you, dir) = (st.db.clone(), st.you_id.clone(), st.data_dir.clone());
    let out = tauri::async_runtime::spawn_blocking(move || {
        let mut res = AddFilesResult { added: vec![], failed: vec![] };
        for p in paths {
            match files::add_from_path(&db, &you, &dir, &owner_type, &owner_id, std::path::Path::new(&p)) {
                Ok(f) => res.added.push(f),
                Err(err) => res.failed.push(err.to_string()),
            }
        }
        res
    }).await.map_err(|err| err.to_string())?;
    if !out.added.is_empty() { changed(&app, "files"); }
    Ok(out)
}
#[tauri::command]
pub async fn list_files(st: State<'_, AppState>, owner_type: String, owner_id: String) -> R<Vec<FileRow>> {
    off(&st, move |st| files::list(&st.db, &owner_type, &owner_id).map_err(e)).await
}
#[tauri::command]
pub async fn remove_file(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    off(&st, move |st| files::remove(&st.db, &st.you_id, &id).map_err(e)).await?;
    changed(&app, "files");
    Ok(())
}
/// Opens a named copy of the file with the system's default app.
#[tauri::command]
pub async fn open_file(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let (db, dir) = (st.db.clone(), st.data_dir.clone());
    let path = tauri::async_runtime::spawn_blocking(move || {
        let f = files::get(&db, &id)?;
        files::materialize(&dir, &f)
    }).await.map_err(|err| err.to_string())?.map_err(e)?;
    app.opener().open_path(path.to_string_lossy(), None::<&str>).map_err(|err| err.to_string())
}

// ---- team setup ----
#[tauri::command]
pub async fn add_team(app: AppHandle, st: State<'_, AppState>, name: String) -> R<String> {
    let id = off(&st, move |st| team::add_team(&st.db, &st.you_id, &name).map_err(e)).await?;
    changed(&app, "teams");
    Ok(id)
}
#[tauri::command]
pub async fn add_agent(app: AppHandle, st: State<'_, AppState>, team_id: String, input: AgentInput) -> R<String> {
    let id = off(&st, move |st| team::add_agent(&st.db, &st.you_id, &team_id, input).map_err(e)).await?;
    changed(&app, "actors");
    // Its memory folder, or the one it shares (GA-96).
    changed(&app, "docs");
    // It landed on its role's usual columns: it may take their waiting cards.
    changed(&app, "workflow_states");
    pull_soon(&st);
    Ok(id)
}
#[tauri::command]
pub async fn update_agent(app: AppHandle, st: State<'_, AppState>, actor_id: String, input: AgentInput) -> R<()> {
    let agent = actor_id.clone();
    off(&st, move |st| team::update_agent(&st.db, &st.you_id, &agent, input).map_err(e)).await?;
    changed(&app, "actors");
    // A new name moves its memory folder, and joining or leaving a shared folder moves notes (GA-96).
    changed(&app, "docs");
    resume(&st, &actor_id);
    Ok(())
}
/// The agent form's Folders: why each folder is refused, or a warning (a project's main checkout).
#[tauri::command]
pub async fn check_agent_folders(st: State<'_, AppState>, folders: Vec<gizai_core::folders::Folder>) -> R<Vec<gizai_core::folders::FolderCheck>> {
    off(&st, move |st| Ok(crate::folders::check(st, &folders))).await
}
#[tauri::command]
pub async fn set_agent_status(app: AppHandle, st: State<'_, AppState>, actor_id: String, status: String) -> R<()> {
    let agent = actor_id.clone();
    off(&st, move |st| team::set_agent_status(&st.db, &st.you_id, &agent, &status).map_err(e)).await?;
    changed(&app, "actors");
    resume(&st, &actor_id);
    Ok(())
}
/// Adds a column after `after_id` (`team::add_state`), Manual and without agents. `category` is a kind in plain words
/// (waiting, work, testing, review, deploy, done, backlog) or a category.
#[tauri::command]
pub async fn add_state(app: AppHandle, st: State<'_, AppState>, team_id: String, name: String, after_id: String, category: String) -> R<String> {
    let cat = gizai_core::columns::category_of(&category)
        .ok_or_else(|| format!("a column is Waiting, Work, Testing, Review, Deploy, Done or Backlog, not {category}"))?;
    let id = off(&st, move |st| team::add_state(&st.db, &st.you_id, &team_id, &name, &after_id, cat).map_err(e)).await?;
    changed(&app, "workflow_states");
    Ok(id)
}
/// Sets up a column: its agents, Auto or Manual, its next column, its name and its place (`columns::set_column`).
#[tauri::command]
pub async fn set_column(app: AppHandle, st: State<'_, AppState>, state_id: String, input: gizai_core::columns::ColumnInput) -> R<()> {
    off(&st, move |st| gizai_core::columns::set_column(&st.db, &st.you_id, &state_id, input).map_err(e)).await?;
    changed(&app, "workflow_states");
    pull_soon(&st);
    Ok(())
}
/// Puts an agent on a column (dragged from the organisation chart, or "+ Agent").
#[tauri::command]
pub async fn add_column_agent(app: AppHandle, st: State<'_, AppState>, state_id: String, agent_id: String) -> R<()> {
    off(&st, move |st| gizai_core::columns::add_agent(&st.db, &st.you_id, &state_id, &agent_id).map_err(e)).await?;
    changed(&app, "workflow_states");
    pull_soon(&st);
    Ok(())
}
/// Takes an agent off a column (×). Its running cards finish as usual.
#[tauri::command]
pub async fn remove_column_agent(app: AppHandle, st: State<'_, AppState>, state_id: String, agent_id: String) -> R<()> {
    off(&st, move |st| gizai_core::columns::remove_agent(&st.db, &st.you_id, &state_id, &agent_id).map_err(e)).await?;
    changed(&app, "workflow_states");
    Ok(())
}
/// What removing a column does, for its confirm (`columns::removal`).
#[tauri::command]
pub async fn column_removal(st: State<'_, AppState>, state_id: String) -> R<gizai_core::columns::Removal> {
    off(&st, move |st| gizai_core::columns::removal(&st.db, &state_id).map_err(e)).await
}
/// Removes a column, moving its cards to `target_id` (`columns::remove_state`). On an Auto column, its agents pick them up.
#[tauri::command]
pub async fn remove_state(app: AppHandle, st: State<'_, AppState>, state_id: String, target_id: String) -> R<()> {
    off(&st, move |st| gizai_core::columns::remove_state(&st.db, &st.you_id, &state_id, &target_id).map_err(e)).await?;
    changed(&app, "workflow_states");
    changed(&app, "tasks");
    pull_soon(&st);
    Ok(())
}
/// Every label with the number of cards that carry it (Team → Labels).
#[tauri::command]
pub async fn list_labels(st: State<'_, AppState>) -> R<Vec<gizai_core::labels::LabelInfo>> {
    off(&st, |st| gizai_core::labels::list(&st.db).map_err(e)).await
}
/// Creates a label (`id` None) or renames or recolours one; returns its id. Names are unique, ignoring case.
#[tauri::command]
pub async fn save_label(app: AppHandle, st: State<'_, AppState>, id: Option<String>, name: String, color: Option<String>) -> R<String> {
    let id = off(&st, move |st| gizai_core::labels::save(&st.db, &st.you_id, id.as_deref(), &name, color.as_deref()).map_err(e)).await?;
    changed(&app, "labels");
    Ok(id)
}
/// Removes a label from every card; returns how many cards carried it.
#[tauri::command]
pub async fn remove_label(app: AppHandle, st: State<'_, AppState>, id: String) -> R<i64> {
    let n = off(&st, move |st| gizai_core::labels::remove(&st.db, &st.you_id, &id).map_err(e)).await?;
    changed(&app, "labels");
    changed(&app, "tasks");
    Ok(n)
}
/// Adds a branch to the team's organisation chart; returns the branches.
#[tauri::command]
pub async fn add_branch(app: AppHandle, st: State<'_, AppState>, team_id: String, name: String) -> R<Vec<team::Branch>> {
    let out = off(&st, move |st| team::add_branch(&st.db, &st.you_id, &team_id, &name).map_err(e)).await?;
    changed(&app, "teams");
    Ok(out)
}
/// Removes a branch without agents from the team's organisation chart; returns the branches.
#[tauri::command]
pub async fn remove_branch(app: AppHandle, st: State<'_, AppState>, team_id: String, key: String) -> R<Vec<team::Branch>> {
    let out = off(&st, move |st| team::remove_branch(&st.db, &st.you_id, &team_id, &key).map_err(e)).await?;
    changed(&app, "teams");
    Ok(out)
}
#[tauri::command]
pub async fn rename_state(app: AppHandle, st: State<'_, AppState>, state_id: String, name: String) -> R<()> {
    off(&st, move |st| team::rename_state(&st.db, &st.you_id, &state_id, &name).map_err(e)).await?;
    changed(&app, "workflow_states");
    Ok(())
}
/// The starting instructions for a role, to prefill the agent form. Memory only: on the async runtime.
#[tauri::command]
pub async fn role_template(role: String) -> String { gizai_core::seed::role_template(&team::role_key(&role)) }
/// The allowed commands a role starts with, to prefill the agent form. Memory only: on the async runtime.
#[tauri::command]
pub async fn role_tools(role: String) -> Vec<String> { gizai_core::seed::role_tools(&team::role_key(&role)) }

// ---- team templates (GA-226, docs/team-template-format.md) ----
use gizai_core::templates;

/// Export's check: each place in the team's template that looks private (an email, a local path, a URL with a query or
/// a password, a key or token). Writes nothing.
#[tauri::command]
pub async fn check_team_export(st: State<'_, AppState>, team_id: String) -> R<Vec<templates::Warning>> {
    off(&st, move |st| templates::check_export(&st.db, &team_id).map_err(e)).await
}
/// Writes the team to `path` (picked with the dialog's save) as a template, format 1.
#[tauri::command]
pub async fn save_team_template(st: State<'_, AppState>, team_id: String, path: String) -> R<()> {
    off(&st, move |st| templates::save(&st.db, &team_id, std::path::Path::new(&path)).map_err(e)).await
}
/// The built-in templates. Memory only: on the async runtime.
#[tauri::command]
pub async fn team_templates() -> Vec<templates::Builtin> { templates::builtins() }
/// The check: what importing the template makes; a refusal is an error that says why. Writes nothing.
#[tauri::command]
pub async fn preview_team_template(st: State<'_, AppState>, source: templates::Source) -> R<templates::Plan> {
    off(&st, move |st| templates::preview(&st.db, &source).map_err(e)).await
}
/// Makes a new team called `name` from the template; returns its id. Its agents come in paused from a file (active from a
/// built-in), and nothing starts by itself: the new team has no cards yet.
#[tauri::command]
pub async fn import_team_template(app: AppHandle, st: State<'_, AppState>, source: templates::Source, name: String) -> R<String> {
    let id = off(&st, move |st| templates::import(&st.db, &st.you_id, &source, &name).map_err(e)).await?;
    changed(&app, "teams");
    changed(&app, "actors");
    changed(&app, "workflow_states");
    changed(&app, "labels");
    // The agents' memory folders.
    changed(&app, "docs");
    Ok(id)
}

// ---- agent runs ----
use crate::runs;

#[tauri::command]
pub async fn detect_claude(st: State<'_, AppState>) -> R<Option<String>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runs::detect_claude(&st)).await.map_err(|err| err.to_string())
}
#[tauri::command]
pub async fn get_settings(st: State<'_, AppState>) -> R<runs::Settings> { off(&st, |st| Ok(runs::get_settings(st))).await }
#[tauri::command]
pub async fn save_settings(app: AppHandle, st: State<'_, AppState>, settings: runs::Settings) -> R<()> {
    off(&st, move |st| runs::save_settings(st, &settings)).await?;
    changed(&app, "settings");
    Ok(())
}
/// Starts an agent on the card now (the given agent, else the assigned or routed one). Returns the run id.
#[tauri::command]
pub async fn start_run(st: State<'_, AppState>, task_id: String, agent_id: Option<String>) -> R<String> {
    let (id, _done) = runs::start(&st, &task_id, agent_id, None, "manual").await?;
    Ok(id)
}
/// Continue a stopped run: resume its session (see runs::continue_run), with your note for the agent when you wrote one
/// (saved on the card as your comment, see runs::continue_with_note).
#[tauri::command]
pub async fn continue_run(st: State<'_, AppState>, run_id: String, note: Option<String>) -> R<String> {
    let (id, _done) = runs::continue_with_note(&st, &run_id, note, None).await?;
    Ok(id)
}
/// "Run this for me": Done, continue. You ran the commands the card's latest run asked you to run; that run continues
/// with a note that says so (see runs::continue_after_run_for_me). Returns the new run's id.
#[tauri::command]
pub async fn continue_after_run_for_me(st: State<'_, AppState>, task_id: String) -> R<String> {
    let (id, _done) = runs::continue_after_run_for_me(&st, &task_id).await?;
    Ok(id)
}
/// Memory only (the stop is a signal its run's own task acts on): on the async runtime.
#[tauri::command]
pub async fn stop_run(st: State<'_, AppState>, run_id: String) -> R<()> {
    runs::stop(&st, &run_id);
    Ok(())
}
#[tauri::command]
pub async fn list_runs(st: State<'_, AppState>, task_id: String) -> R<Vec<Run>> {
    off(&st, move |st| gizai_core::runs::list_for_task(&st.db, &task_id).map_err(e)).await
}
/// The run's events after `after_seq` (all of them for null), at most the last 500 (GA-191).
#[tauri::command]
pub async fn run_events(st: State<'_, AppState>, run_id: String, after_seq: Option<u64>) -> R<Vec<runs::SeqEvent>> {
    off(&st, move |st| Ok(runs::events_after(st, &run_id, after_seq))).await
}
/// The commits a finished run made, oldest first (the Runs tab).
#[tauri::command]
pub async fn run_commits(st: State<'_, AppState>, run_id: String) -> R<Vec<gizai_agents::worktree::Commit>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runs::commits(&st, &run_id)).await.map_err(|err| err.to_string())?
}
/// Memory only: on the async runtime.
#[tauri::command]
pub async fn live_runs(st: State<'_, AppState>) -> R<Vec<runs::LiveRun>> { Ok(runs::live(&st)) }
/// Who a Run without a chosen agent would start now (the assigned agent, else the first agent on the card's column).
#[tauri::command]
pub async fn suggest_agent(st: State<'_, AppState>, task_id: String) -> R<Option<String>> {
    off(&st, move |st| Ok(runs::suggest(st, &task_id))).await
}

/// The models this user's Claude Code offers (as its /model picker lists them), with their effort levels.
#[tauri::command]
pub async fn claude_models(st: State<'_, AppState>, refresh: bool, cli: Option<String>) -> R<Vec<gizai_agents::models::ModelOption>> {
    runs::models_for(&st, cli.as_deref(), refresh).await
}

/// Settings → Coding CLIs: every CLI agents can run on, with the program each would start.
#[tauri::command]
pub async fn list_clis(st: State<'_, AppState>) -> R<Vec<crate::clis::CliStatus>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::clis::list(&st)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_clis(app: AppHandle, st: State<'_, AppState>, clis: Vec<gizai_core::clis::Cli>) -> R<Vec<crate::clis::CliStatus>> {
    let st = st.inner().clone();
    let out = tauri::async_runtime::spawn_blocking(move || crate::clis::save(&st, clis)).await.map_err(|e| e.to_string())??;
    changed(&app, "settings");
    Ok(out)
}

/// The known coding CLIs installed here that aren't listed yet.
#[tauri::command]
pub async fn find_clis(st: State<'_, AppState>) -> R<Vec<gizai_core::clis::Cli>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::clis::find(&st)).await.map_err(|e| e.to_string())?
}

/// One agent's settings and membership.
#[tauri::command]
pub async fn get_agent(st: State<'_, AppState>, id: String) -> R<team::Member> { off(&st, move |st| team::agent(&st.db, &id).map_err(e)).await }

/// An agent's runs per day for the last `days` days (agent page charts), and its recent runs.
#[tauri::command]
pub async fn agent_stats(st: State<'_, AppState>, id: String, days: i64) -> R<Vec<DayStat>> {
    off(&st, move |st| gizai_core::runs::daily_stats(&st.db, &id, days.clamp(1, 90), gizai_core::ids::now_ms()).map_err(e)).await
}
#[tauri::command]
pub async fn agent_runs(st: State<'_, AppState>, id: String, limit: i64) -> R<Vec<Run>> {
    off(&st, move |st| gizai_core::runs::list_for_agent(&st.db, &id, limit.clamp(1, 200)).map_err(e)).await
}
/// The card this agent would pick up next (Run on the agent page).
#[tauri::command]
pub async fn agent_next_task(st: State<'_, AppState>, id: String) -> R<Option<String>> {
    off(&st, move |st| gizai_core::workflow::next_task_for(&st.db, &id).map_err(e)).await
}

/// The Usage page: the tokens and API cost of the runs and chat turns in `period` (today, 7d, 30d or month), in total, per
/// day, per agent and per project.
#[tauri::command]
pub async fn usage_summary(st: State<'_, AppState>, period: String) -> R<gizai_core::usage::Usage> {
    off(&st, move |st| gizai_core::usage::for_period(&st.db, &period, gizai_core::ids::now_ms()).map_err(e)).await
}

/// The Usage page's Subscription tab: per coding CLI, the newest reading of each of its limits and the agents on it.
#[tauri::command]
pub async fn subscription_limits(st: State<'_, AppState>) -> R<Vec<gizai_core::limits::CliLimits>> {
    off(&st, crate::limits::subscription).await
}

// ---- chat with the Team Lead ----
use crate::chat;

/// The chats, newest activity first: the `limit` newest (Chat → Recent), or all of them (the Inbox).
#[tauri::command]
pub async fn list_chat_threads(st: State<'_, AppState>, limit: Option<usize>) -> R<Vec<gizai_core::chat::ChatThread>> {
    off(&st, move |st| match limit {
        Some(n) => gizai_core::chat::recent_threads(&st.db, n),
        None => gizai_core::chat::list_threads(&st.db),
    }.map_err(e)).await
}
/// One chat, also one older than those in Recent (opened from the Archive).
#[tauri::command]
pub async fn get_chat_thread(st: State<'_, AppState>, thread_id: String) -> R<gizai_core::chat::ChatThread> {
    off(&st, move |st| gizai_core::chat::get_thread(&st.db, &thread_id).map_err(e)).await
}
/// Chat → Archive: the chats whose title or messages (yours and the Team Lead's) hold `query`; all of them for an empty one.
#[tauri::command]
pub async fn search_chat_threads(st: State<'_, AppState>, query: String) -> R<Vec<gizai_core::chat::ThreadHit>> {
    off(&st, move |st| gizai_core::chat::search_threads(&st.db, &query).map_err(e)).await
}
#[tauri::command]
pub async fn chat_messages(st: State<'_, AppState>, thread_id: String) -> R<Vec<gizai_core::chat::ChatMessage>> {
    off(&st, move |st| gizai_core::chat::messages(&st.db, &thread_id).map_err(e)).await
}
/// Sends a message (in a new thread when `thread_id` is None, which runs on `cli` when one was picked under the text box)
/// with the `files` added to it (paths; the text may be empty then) and starts the Team Lead's answer; while it is
/// answering in the thread, the message is queued with its files. Returns the thread id.
#[tauri::command]
pub async fn send_chat(st: State<'_, AppState>, thread_id: Option<String>, text: String, cli: Option<String>, files: Option<Vec<String>>) -> R<String> {
    let (id, _done) = chat::send_with_files(&st, thread_id, text, cli, files.unwrap_or_default(), None).await?;
    Ok(id)
}
/// Files picked or dropped for a chat message: the paths that can be added, and one plain sentence for each that can't
/// (a folder, larger than 1 GB, or it can't be read).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCheck {
    pub ok: Vec<String>,
    pub failed: Vec<String>,
}
#[tauri::command]
pub async fn check_files(paths: Vec<String>) -> R<FileCheck> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = FileCheck { ok: vec![], failed: vec![] };
        for p in paths {
            match files::check_path(std::path::Path::new(&p)) {
                Ok(_) => out.ok.push(p),
                Err(err) => out.failed.push(err.to_string()),
            }
        }
        out
    }).await.map_err(|err| err.to_string())
}
/// The id of the item a gizai: link names, for its page: a task by id or identifier (GA-12), a project by id or key (GA);
/// other kinds name their id already. Fails when it is gone.
#[tauri::command]
pub async fn item_id(st: State<'_, AppState>, kind: String, key: String) -> R<String> {
    off(&st, move |st| match kind.as_str() {
        "task" => tasks::get(&st.db, &key).map(|t| t.id).or_else(|_| tasks::id_of(&st.db, &key)).map_err(e),
        "project" => match projects::get(&st.db, &key) {
            Ok(p) => Ok(p.id),
            Err(_) => projects::list(&st.db).map_err(e)?.into_iter().find(|p| p.key.eq_ignore_ascii_case(&key)).map(|p| p.id)
                .ok_or_else(|| format!("No project with the key {key}")),
        },
        _ => Ok(key),
    }).await
}
/// The chat's queued messages: they wait while the Team Lead answers.
#[tauri::command]
pub async fn chat_queue(st: State<'_, AppState>, thread_id: String) -> R<Vec<gizai_core::chat::QueuedMessage>> {
    off(&st, move |st| chat::queue(st, &thread_id)).await
}
#[tauri::command]
pub async fn edit_queued_chat(st: State<'_, AppState>, id: String, text: String) -> R<gizai_core::chat::QueuedMessage> {
    off(&st, move |st| chat::edit_queued(st, &id, &text)).await
}
#[tauri::command]
pub async fn remove_queued_chat(st: State<'_, AppState>, id: String) -> R<()> { off(&st, move |st| chat::remove_queued(st, &id)).await }
/// Send now: the chat's queued messages go together (after the answer being written, if there is one).
#[tauri::command]
pub async fn send_chat_queue(st: State<'_, AppState>, thread_id: String) -> R<()> { chat::send_queue(&st, &thread_id, None).await.map(|_| ()) }
/// Runs on under the text box: the chat's next answers run on `cli` (None: on the Team Lead's Runs on).
#[tauri::command]
pub async fn set_chat_cli(st: State<'_, AppState>, thread_id: String, cli: Option<String>) -> R<gizai_core::chat::ChatThread> {
    off(&st, move |st| chat::set_cli(st, &thread_id, cli.as_deref())).await
}
/// Answer on <CLI> under a usage-limit note: the chat moves to `cli` and the message goes again there.
#[tauri::command]
pub async fn answer_chat_on(st: State<'_, AppState>, thread_id: String, cli: String, note_id: String) -> R<()> {
    chat::answer_on(&st, &thread_id, &cli, &note_id, None).await.map(|_| ())
}
/// The coding CLIs Runs on lists under the chat's text box, with why one can't run the chat. Off the main thread:
/// finding their programs can start your login shell.
#[tauri::command]
pub async fn chat_clis(st: State<'_, AppState>) -> R<Vec<chat::ChatCli>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || chat::clis(&st)).await.map_err(|e| e.to_string())?
}
/// Memory only (the stop is a signal its answer's own task acts on): on the async runtime.
#[tauri::command]
pub async fn stop_chat(st: State<'_, AppState>, thread_id: String) -> R<()> {
    chat::stop(&st, &thread_id);
    Ok(())
}
/// Chat answers being written right now, with their text so far. Memory only: on the async runtime.
#[tauri::command]
pub async fn chat_live(st: State<'_, AppState>) -> R<Vec<chat::ChatStatus>> { Ok(chat::live(&st)) }
/// The agent that answers on the Chat page, if any.
#[tauri::command]
pub async fn chat_agent(st: State<'_, AppState>) -> R<Option<team::Member>> { off(&st, |st| team::chat_agent(&st.db).map_err(e)).await }
/// × on a Team Lead chat in the Inbox: it no longer waits for you (it stays in Chat → Recent).
#[tauri::command]
pub async fn dismiss_chat(app: AppHandle, st: State<'_, AppState>, thread_id: String) -> R<()> {
    off(&st, move |st| gizai_core::chat::dismiss(&st.db, &st.you_id, &thread_id).map_err(e)).await?;
    changed(&app, "chat_threads");
    Ok(())
}
/// GA-102: the Team Lead's calls that wait for your approval in a chat (after a tool from outside Gizai), each with what
/// it would do now. Only these commands, for you, list, approve or discard them; no tool of the Team Lead's does.
#[tauri::command]
pub async fn pending_actions(st: State<'_, AppState>, thread_id: String) -> R<Vec<crate::pending::PendingView>> {
    off(&st, move |st| crate::pending::list(st, &thread_id)).await
}
/// Approve: runs exactly that call, once, with every other rule in force; the chat gets a note with what happened.
#[tauri::command]
pub async fn approve_pending_action(st: State<'_, AppState>, id: String) -> R<gizai_core::pending::PendingAction> { crate::pending::approve(&st, &id).await }
/// Approve all: the calls you saw, one after the other in the order given.
#[tauri::command]
pub async fn approve_pending_actions(st: State<'_, AppState>, ids: Vec<String>) -> R<Vec<gizai_core::pending::PendingAction>> {
    crate::pending::approve_all(&st, &ids).await
}
/// Discard: nothing runs; the chat gets a note with what was discarded.
#[tauri::command]
pub async fn discard_pending_action(st: State<'_, AppState>, id: String) -> R<gizai_core::pending::PendingAction> {
    off(&st, move |st| crate::pending::discard(st, &id)).await
}

// ---- first start (GA-162) ----
/// Whether the first start's onboarding waits: true on a new install until it is finished or skipped.
#[tauri::command]
pub async fn onboarding_pending(st: State<'_, AppState>) -> R<bool> { off(&st, |st| Ok(crate::onboarding::pending(st))).await }
/// Finishes the onboarding: your name, the team's and the agents' coding CLI, then the Team Lead's welcome chat. Returns
/// that chat's id; None when the Team Lead can't answer.
#[tauri::command]
pub async fn finish_onboarding(st: State<'_, AppState>, input: gizai_core::onboarding::OnboardingInput) -> R<Option<String>> {
    Ok(crate::onboarding::finish(&st, input).await?.map(|(thread_id, _answer)| thread_id))
}
/// Skips the onboarding: it no longer waits, and nothing else changes.
#[tauri::command]
pub async fn skip_onboarding(st: State<'_, AppState>) -> R<()> { off(&st, crate::onboarding::skip).await }

// ---- pull requests on GitHub ----
/// Open pull request (a card in Review): pushes the card's branch with your git login and opens its pull request with gh.
#[tauri::command]
pub async fn open_pull_request(st: State<'_, AppState>, task_id: String) -> R<crate::pulls::PullInfo> { crate::pulls::open(&st, &task_id).await }
/// What GitHub says about the card's pull request now (a merge moves the card to Done).
#[tauri::command]
pub async fn check_pull_request(st: State<'_, AppState>, task_id: String) -> R<Option<crate::pulls::PullInfo>> { crate::pulls::check(&st, &task_id).await }
#[tauri::command]
pub async fn detect_gh(st: State<'_, AppState>) -> R<Option<String>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::pulls::detect_gh(&st)).await.map_err(|err| err.to_string())
}

// ---- Settings → GitHub ----
/// Whether Gizai can use GitHub: the GitHub CLI, the account it is logged in as, and how pushes go.
#[tauri::command]
pub async fn github_status(st: State<'_, AppState>) -> R<crate::github::Status> { Ok(crate::github::status(&st).await) }
/// Check connection: gh, its login, ssh to GitHub, and whether you can push to each project with a GitHub link.
#[tauri::command]
pub async fn github_check(st: State<'_, AppState>) -> R<crate::github::ConnectionCheck> { Ok(crate::github::check(&st).await) }
/// Log in with GitHub: starts gh's login in the browser and returns its one-time code and link.
#[tauri::command]
pub async fn github_login(st: State<'_, AppState>) -> R<crate::github::LoginCode> { crate::github::login(&st).await }
/// Waits until the login in the browser has ended: the account gh logged in as, or why it didn't.
#[tauri::command]
pub async fn github_login_wait(st: State<'_, AppState>) -> R<Option<String>> {
    match crate::github::login_wait(&st).await {
        Some(Err(p)) => Err(p.to_string()),
        Some(Ok(who)) => Ok(who),
        None => Ok(None),
    }
}
/// Memory only (a flag the login's own task reads): on the async runtime.
#[tauri::command]
pub async fn github_login_cancel(st: State<'_, AppState>) -> R<()> {
    crate::github::login_cancel(&st);
    Ok(())
}

// ---- Settings → Bitbucket ----
/// Whether Gizai can use Bitbucket: the login in the keychain (email and API token) and the account it belongs to.
#[tauri::command]
pub async fn bitbucket_status(st: State<'_, AppState>) -> R<crate::bitbucket::BitbucketStatus> { Ok(crate::bitbucket::status(&st).await) }
/// Saves your Atlassian email and API token in the keychain, once Bitbucket has accepted them.
#[tauri::command]
pub async fn bitbucket_save_login(st: State<'_, AppState>, email: String, token: String) -> R<crate::bitbucket::BitbucketStatus> {
    crate::bitbucket::save_login(&st, email, token).await
}
/// Removes the Bitbucket login from the keychain.
#[tauri::command]
pub async fn bitbucket_remove_login(st: State<'_, AppState>) -> R<crate::bitbucket::BitbucketStatus> { crate::bitbucket::remove_login(&st).await }
/// Check connection: the token's account, ssh to Bitbucket, and whether you can push to each project with a Bitbucket link.
#[tauri::command]
pub async fn bitbucket_check(st: State<'_, AppState>) -> R<crate::github::ConnectionCheck> { Ok(crate::bitbucket::check(&st).await) }

// ---- worktrees of finished cards (Settings → Data) ----
/// The worktrees of Done and Cancelled cards, with their disk use.
#[tauri::command]
pub async fn list_old_worktrees(st: State<'_, AppState>) -> R<Vec<crate::worktrees::OldWorktree>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::worktrees::list(&st)).await.map_err(|err| err.to_string())?
}
/// Removes the worktrees of these finished cards (after you confirmed); says per card what happened.
#[tauri::command]
pub async fn remove_old_worktrees(st: State<'_, AppState>, task_ids: Vec<String>) -> R<Vec<crate::worktrees::RemovedWorktree>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::worktrees::remove(&st, &task_ids)).await.map_err(|err| err.to_string())?
}
/// Remove anyway: puts a finished card's uncommitted changes in git's stash, then removes its worktree (after you
/// confirmed); says what happened, or why nothing was removed.
#[tauri::command]
pub async fn remove_old_worktree_anyway(st: State<'_, AppState>, task_id: String) -> R<crate::worktrees::RemovedWorktree> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::worktrees::remove_anyway(&st, &task_id)).await.map_err(|err| err.to_string())?
}

// ---- updates (Settings → Updates, the notice above Company) ----
#[tauri::command]
pub async fn update_status(st: State<'_, AppState>) -> R<crate::update::UpdateStatus> { off(&st, |st| Ok(crate::update::status(st))).await }
/// Asks GitHub for the latest release now.
#[tauri::command]
pub async fn check_for_updates(st: State<'_, AppState>) -> R<crate::update::UpdateStatus> { Ok(crate::update::check(&st).await) }
#[tauri::command]
pub async fn set_update_auto_check(st: State<'_, AppState>, on: bool) -> R<crate::update::UpdateStatus> {
    off(&st, move |st| crate::update::set_auto_check(st, on)).await
}
/// Builds and installs `version` in the background; returns at once.
#[tauri::command]
pub async fn start_update(st: State<'_, AppState>, version: String) -> R<crate::update::UpdateStatus> {
    off(&st, move |st| crate::update::start(st, &version)).await
}
#[tauri::command]
pub async fn stop_update(st: State<'_, AppState>) -> R<crate::update::UpdateStatus> { off(&st, |st| Ok(crate::update::stop(st))).await }
/// Quits and starts the Gizai an update installed: agents at work are stopped first, as when you quit.
#[tauri::command]
pub async fn restart_gizai(app: AppHandle, st: State<'_, AppState>) -> R<()> {
    off(&st, |st| {
        let mut cmd = crate::update::restart_command(st)?;
        cmd.spawn().map(|_| ()).map_err(|e| format!("Couldn't restart Gizai: {e}"))
    }).await?;
    // the restart after an update doesn't back up on quit
    st.backups.quit_without_backup();
    app.exit(0);
    Ok(())
}

// ---- backups (GA-216, contract BK): snapshots in a backups folder, on a schedule ----
/// The backup settings, the backups folder in use and why it can't be used now, the snapshot that runs, this computer's
/// newest, the last error and when the next scheduled one is due.
#[tauri::command]
pub async fn backup_status(st: State<'_, AppState>) -> R<gizai_core::backups::BackupStatus> {
    off(&st, |st| Ok(crate::backups::status(st))).await
}
/// Saves the backup settings: refuses Keep outside 1 to 100 and a folder Gizai can't write in, saying why.
#[tauri::command]
pub async fn save_backup_settings(st: State<'_, AppState>, settings: gizai_core::backups::BackupSettings) -> R<gizai_core::backups::BackupStatus> {
    off(&st, move |st| crate::backups::save_settings(st, settings)).await
}
/// The snapshots in a folder, newest first; null: the folder in Settings.
#[tauri::command]
pub async fn list_backups(st: State<'_, AppState>, folder: Option<String>) -> R<Vec<gizai_core::backups::BackupEntry>> {
    off(&st, move |st| crate::backups::list(st, folder)).await
}
/// Back up now: starts a snapshot in the background; refused without a folder or while one runs.
#[tauri::command]
pub async fn back_up_now(st: State<'_, AppState>) -> R<gizai_core::backups::BackupStatus> {
    off(&st, crate::backups::back_up_now).await
}

// ---- MCP servers (Settings → MCP servers, agent form → Tools) ----

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> R<T> + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn list_mcp_servers(st: State<'_, AppState>) -> R<Vec<crate::mcp_servers::ServerView>> {
    let st = st.inner().clone();
    blocking(move || crate::mcp_servers::list(&st)).await
}

#[tauri::command]
pub async fn save_mcp_server(app: AppHandle, st: State<'_, AppState>, input: crate::mcp_servers::ServerInput) -> R<crate::mcp_servers::ServerView> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::save(&st, input)).await?;
    changed(&app, "settings");
    Ok(out)
}

#[tauri::command]
pub async fn remove_mcp_server(app: AppHandle, st: State<'_, AppState>, id: String) -> R<()> {
    let st = st.inner().clone();
    blocking(move || crate::mcp_servers::remove(&st, &id)).await?;
    changed(&app, "settings");
    Ok(())
}

/// List tools: starts or calls the server, lists its tools and stops it.
#[tauri::command]
pub async fn list_mcp_tools(app: AppHandle, st: State<'_, AppState>, id: String) -> R<crate::mcp_servers::ServerView> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::list_tools(&st, &id)).await;
    changed(&app, "settings");
    out
}

/// The MCP servers in each Claude Code's config file (read only; no server is started).
#[tauri::command]
pub async fn scan_claude_code_mcp(st: State<'_, AppState>) -> R<crate::mcp_servers::Scan> {
    let st = st.inner().clone();
    blocking(move || crate::mcp_servers::scan(&st)).await
}

#[tauri::command]
pub async fn import_mcp_servers(app: AppHandle, st: State<'_, AppState>, picks: Vec<crate::mcp_servers::Pick>) -> R<Vec<crate::mcp_servers::ServerView>> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::import(&st, picks)).await?;
    changed(&app, "settings");
    Ok(out)
}

/// Sign in: opens the server's sign-in page in your default browser (the system opener) and waits up to 10 minutes for it.
#[tauri::command]
pub async fn mcp_sign_in(app: AppHandle, st: State<'_, AppState>, id: String) -> R<crate::mcp_servers::ServerView> {
    use tauri_plugin_opener::OpenerExt;
    let st = st.inner().clone();
    let opener = app.clone();
    let out = blocking(move || crate::mcp_servers::sign_in(&st, &id, |url| {
        opener.opener().open_url(url, None::<&str>).map_err(|e| format!("Couldn't open your browser: {e}"))
    }, crate::mcp_servers::SIGN_IN_TIMEOUT)).await;
    changed(&app, "settings");
    out
}

#[tauri::command]
pub async fn mcp_sign_out(app: AppHandle, st: State<'_, AppState>, id: String) -> R<crate::mcp_servers::ServerView> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::sign_out(&st, &id)).await?;
    changed(&app, "settings");
    Ok(out)
}

/// The agent form's Tools section.
#[tauri::command]
pub async fn agent_mcp(st: State<'_, AppState>, agent_id: String) -> R<crate::mcp_servers::AgentMcpView> {
    let st = st.inner().clone();
    blocking(move || crate::mcp_servers::agent_view(&st, &agent_id)).await
}

#[tauri::command]
pub async fn save_agent_mcp(app: AppHandle, st: State<'_, AppState>, agent_id: String, tools: gizai_core::mcp_servers::AgentTools)
    -> R<crate::mcp_servers::AgentMcpView> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::save_agent(&st, &agent_id, tools)).await?;
    changed(&app, "team");
    Ok(out)
}

/// The agent form's Web, Browser and Built-in tools for the CLI picked in the form (an agent not added yet has no id).
#[tauri::command]
pub async fn agent_cli_tools(st: State<'_, AppState>, agent_id: Option<String>, cli_id: String) -> R<crate::mcp_servers::ToolsView> {
    let st = st.inner().clone();
    blocking(move || crate::mcp_servers::tools_view(&st, agent_id.as_deref(), &cli_id)).await
}

/// Saves the agent's Web and Built-in tool switches, and its Take screenshots of what it built (GA-182). Only you: no Team
/// Lead tool calls this.
#[tauri::command]
pub async fn save_agent_cli_tools(app: AppHandle, st: State<'_, AppState>, agent_id: String, tools: crate::mcp_servers::CliToolsInput)
    -> R<crate::mcp_servers::SavedCliTools> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::save_cli_tools_input(&st, &agent_id, tools)).await?;
    changed(&app, "team");
    Ok(out)
}

/// Ask Claude Code again: its tools, from a start without a login (nothing spent, nothing written in ~/.claude).
#[tauri::command]
pub async fn ask_cli_tools(st: State<'_, AppState>, cli_id: String) -> R<Vec<String>> {
    let st = st.inner().clone();
    crate::mcp_servers::ask_cli_tools(&st, &cli_id).await
}

/// Settings → MCP servers: the built-in browser (Chrome DevTools MCP).
#[tauri::command]
pub async fn browser_entry(st: State<'_, AppState>) -> R<crate::mcp_servers::BrowserView> {
    let st = st.inner().clone();
    blocking(move || crate::mcp_servers::browser_view(&st)).await
}

/// Saves the browser's version and program (the only parts that change).
#[tauri::command]
pub async fn save_browser_entry(app: AppHandle, st: State<'_, AppState>, entry: gizai_core::mcp_servers::BrowserEntry) -> R<crate::mcp_servers::BrowserView> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::save_browser(&st, entry)).await?;
    changed(&app, "settings");
    Ok(out)
}

/// List tools for the browser: starts its server (no browser yet), lists its tools and stops it.
#[tauri::command]
pub async fn list_browser_tools(app: AppHandle, st: State<'_, AppState>) -> R<crate::mcp_servers::BrowserView> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::list_browser_tools(&st)).await;
    changed(&app, "settings");
    out
}
