//! Thin command layer: every command calls gizai-core and maps errors to strings.
//! Write commands emit `rows-changed` so open screens refresh.
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

/// A person edited or reactivated the agent: it takes cards from the queue again (`runs::pull_paused`).
fn resume(st: &State<AppState>, agent_id: &str) {
    crate::runs::resume_pull(st, agent_id);
    let st = st.inner().clone();
    tauri::async_runtime::spawn(async move { crate::runs::pull(&st).await; });
}

/// After a card changes, the agents take their next cards (a card that landed in an Auto column, or was assigned).
fn wake(st: &State<AppState>, task_id: &str) {
    let (st, id) = (st.inner().clone(), task_id.to_string());
    tauri::async_runtime::spawn(async move { crate::runs::dispatch(&st, &id).await; });
}

/// A column changed (its agents, Auto, its cards): the agents take the cards that wait for them now.
fn pull_soon(st: &State<AppState>) {
    let st = st.inner().clone();
    tauri::async_runtime::spawn(async move { crate::runs::pull(&st).await; });
}

// ---- clients and users ----
#[tauri::command]
pub fn list_clients(st: State<AppState>) -> R<Vec<Client>> { clients::list(&st.db).map_err(e) }
#[tauri::command]
pub fn get_client(st: State<AppState>, id: String) -> R<Client> { clients::get(&st.db, &id).map_err(e) }
#[tauri::command]
pub fn save_client(app: AppHandle, st: State<AppState>, id: Option<String>, input: ClientInput) -> R<String> {
    let out = match id {
        Some(id) => clients::update(&st.db, &st.you_id, &id, input).map(|_| id),
        None => clients::create(&st.db, &st.you_id, input),
    }.map_err(e)?;
    changed(&app, "clients");
    Ok(out)
}
#[tauri::command]
pub fn archive_client(app: AppHandle, st: State<AppState>, id: String) -> R<()> {
    clients::archive(&st.db, &st.you_id, &id).map_err(e)?;
    changed(&app, "clients");
    Ok(())
}
#[tauri::command]
pub fn list_contacts(st: State<AppState>, client_id: String) -> R<Vec<Contact>> { clients::contacts(&st.db, &client_id).map_err(e) }
#[tauri::command]
pub fn save_contact(app: AppHandle, st: State<AppState>, contact: Contact) -> R<String> {
    let id = clients::upsert_contact(&st.db, &st.you_id, contact).map_err(e)?;
    changed(&app, "contacts");
    Ok(id)
}
#[tauri::command]
pub fn remove_contact(app: AppHandle, st: State<AppState>, id: String) -> R<()> {
    clients::remove_contact(&st.db, &st.you_id, &id).map_err(e)?;
    changed(&app, "contacts");
    Ok(())
}
#[tauri::command]
pub fn list_users(st: State<AppState>) -> R<Vec<Person>> { users::list(&st.db).map_err(e) }
#[tauri::command]
pub fn add_user(app: AppHandle, st: State<AppState>, name: String, email: Option<String>) -> R<String> {
    let id = users::create(&st.db, &st.you_id, &name, email.as_deref()).map_err(e)?;
    changed(&app, "actors");
    Ok(id)
}

// ---- projects ----
#[tauri::command]
pub fn list_projects(st: State<AppState>) -> R<Vec<Project>> { projects::list(&st.db).map_err(e) }
#[tauri::command]
pub fn get_project(st: State<AppState>, id: String) -> R<Project> { projects::get(&st.db, &id).map_err(e) }
#[tauri::command]
pub fn save_project(app: AppHandle, st: State<AppState>, id: Option<String>, input: ProjectInput) -> R<String> {
    let out = match id {
        Some(id) => projects::update(&st.db, &st.you_id, &id, input).map(|_| id),
        None => projects::create(&st.db, &st.you_id, input),
    }.map_err(e)?;
    changed(&app, "projects");
    Ok(out)
}

// ---- tasks ----
#[tauri::command]
pub fn list_tasks(st: State<AppState>, filter: TaskFilter) -> R<Vec<Task>> { tasks::list(&st.db, &filter).map_err(e) }
#[tauri::command]
pub fn get_task(st: State<AppState>, id: String) -> R<Task> { tasks::get(&st.db, &id).map_err(e) }
#[tauri::command]
pub fn create_task(app: AppHandle, st: State<AppState>, input: TaskInput) -> R<String> {
    let id = tasks::create(&st.db, &st.you_id, input).map_err(e)?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(id)
}
#[tauri::command]
pub fn update_task(app: AppHandle, st: State<AppState>, id: String, patch: TaskPatch) -> R<()> {
    tasks::update(&st.db, &st.you_id, &id, patch).map_err(e)?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
#[tauri::command]
pub fn move_task(app: AppHandle, st: State<AppState>, id: String, state_id: String, sort_key: String) -> R<()> {
    tasks::move_to(&st.db, &st.you_id, &id, &state_id, &sort_key).map_err(e)?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
#[tauri::command]
pub fn set_task_labels(app: AppHandle, st: State<AppState>, id: String, label_ids: Vec<String>) -> R<()> {
    tasks::set_labels(&st.db, &st.you_id, &id, label_ids).map_err(e)?;
    changed(&app, "tasks");
    wake(&st, &id);
    Ok(())
}
#[tauri::command]
pub fn list_archived_tasks(st: State<AppState>, project_id: Option<String>) -> R<Vec<Task>> {
    tasks::archived(&st.db, project_id.as_deref().filter(|p| !p.is_empty())).map_err(e)
}
/// Archives a card in Done. Refused while an agent works on it, its run's worktree being made included.
#[tauri::command]
pub fn archive_task(app: AppHandle, st: State<AppState>, id: String) -> R<()> {
    if crate::runs::working_on(&st, &id) {
        return Err("An agent is working on this card".into());
    }
    tasks::archive(&st.db, &st.you_id, &id).map_err(e)?;
    changed(&app, "tasks");
    Ok(())
}
#[tauri::command]
pub fn restore_task(app: AppHandle, st: State<AppState>, id: String) -> R<()> {
    tasks::restore(&st.db, &st.you_id, &id).map_err(e)?;
    changed(&app, "tasks");
    Ok(())
}
#[tauri::command]
pub fn task_activity(st: State<AppState>, task_id: String) -> R<Vec<ChangeEntry>> { tasks::activity(&st.db, &task_id).map_err(e) }
#[tauri::command]
pub fn list_comments(st: State<AppState>, task_id: String) -> R<Vec<Comment>> { comments::list(&st.db, &task_id).map_err(e) }
#[tauri::command]
pub fn add_comment(app: AppHandle, st: State<AppState>, task_id: String, body_md: String) -> R<String> {
    let id = comments::add(&st.db, &st.you_id, &task_id, &body_md, None).map_err(e)?;
    changed(&app, "comments");
    Ok(id)
}

// ---- teams ----
#[tauri::command]
pub fn list_teams(st: State<AppState>) -> R<Vec<team::TeamSummary>> { team::list(&st.db).map_err(e) }
#[tauri::command]
pub fn get_team(st: State<AppState>, id: Option<String>) -> R<team::Team> {
    let id = match id {
        Some(i) => i,
        None => team::list(&st.db).map_err(e)?.first().map(|t| t.id.clone()).ok_or("no team")?,
    };
    team::get(&st.db, &id).map_err(e)
}

// ---- git ----
#[tauri::command]
pub fn check_repo(path: String) -> crate::git::RepoCheck { crate::git::repo_check(std::path::Path::new(&path)) }

// ---- docs ----
#[tauri::command]
pub fn list_docs(st: State<AppState>, project_id: String) -> R<Vec<Doc>> { docs::list(&st.db, &project_id).map_err(e) }
#[tauri::command]
pub fn get_doc(st: State<AppState>, id: String) -> R<Doc> { docs::get(&st.db, &id).map_err(e) }
#[tauri::command]
pub fn create_doc(app: AppHandle, st: State<AppState>, project_id: String, title: String) -> R<String> {
    let id = docs::create(&st.db, &st.you_id, &project_id, &title).map_err(e)?;
    changed(&app, "docs");
    Ok(id)
}
#[tauri::command]
pub fn save_doc(app: AppHandle, st: State<AppState>, id: String, body_md: String, base_version: i64) -> R<i64> {
    let v = docs::save(&st.db, &st.you_id, &id, &body_md, base_version).map_err(e)?;
    changed(&app, "docs");
    Ok(v)
}
#[tauri::command]
pub fn rename_doc(app: AppHandle, st: State<AppState>, id: String, title: String) -> R<()> {
    docs::rename(&st.db, &st.you_id, &id, &title).map_err(e)?;
    changed(&app, "docs");
    Ok(())
}

// ---- memory (GA-19; the Memory page is GA-68) ----
/// An agent's own notes in Memory: the Team Lead's `Team Lead/Notes` (made the first time), another agent's
/// `Agents/<name>/Notes`; None when it has none.
#[tauri::command]
pub fn agent_notes(app: AppHandle, st: State<AppState>, agent_id: String) -> R<Option<gizai_core::memory::Note>> {
    use gizai_core::memory::{self, Who};
    let agent = gizai_core::team::agent(&st.db, &agent_id).map_err(e)?;
    let you = Who::Person(st.you_id.clone());
    if agent.is_lead || agent.chat_enabled {
        let you_name = gizai_core::users::list(&st.db).ok().and_then(|l| l.into_iter().find(|p| p.id == st.you_id)).map(|p| p.name)
            .unwrap_or_else(|| "the user".into());
        let made = memory::find(&st.db, &memory::lead_notes_path()).map_err(e)?.is_none();
        let id = memory::ensure_lead_notes(&st.db, &agent_id, &you_name).map_err(e)?;
        if made {
            changed(&app, "docs");
        }
        return memory::get(&st.db, &you, &id).map(Some).map_err(e);
    }
    Ok(memory::list(&st.db, &you).map_err(e)?.into_iter()
        .find(|n| n.owner_id.as_deref() == Some(agent_id.as_str()) && n.title().eq_ignore_ascii_case(memory::NOTES)))
}
/// Memory for every agent (Settings → Runs): on unless switched off.
#[tauri::command]
pub fn memory_enabled(st: State<AppState>) -> bool { gizai_core::memory::enabled(&st.db) }
#[tauri::command]
pub fn set_memory_enabled(app: AppHandle, st: State<AppState>, on: bool) -> R<()> {
    gizai_core::memory::set_enabled(&st.db, on).map_err(e)?;
    changed(&app, "settings");
    Ok(())
}

#[tauri::command]
pub fn doc_versions(st: State<AppState>, id: String) -> R<Vec<DocVersion>> { docs::versions(&st.db, &id).map_err(e) }
#[tauri::command]
pub fn doc_version_body(st: State<AppState>, id: String, version: i64) -> R<String> { docs::version_body(&st.db, &id, version).map_err(e) }

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
pub fn list_files(st: State<AppState>, owner_type: String, owner_id: String) -> R<Vec<FileRow>> { files::list(&st.db, &owner_type, &owner_id).map_err(e) }
#[tauri::command]
pub fn remove_file(app: AppHandle, st: State<AppState>, id: String) -> R<()> {
    files::remove(&st.db, &st.you_id, &id).map_err(e)?;
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
pub fn add_team(app: AppHandle, st: State<AppState>, name: String) -> R<String> {
    let id = team::add_team(&st.db, &st.you_id, &name).map_err(e)?;
    changed(&app, "teams");
    Ok(id)
}
#[tauri::command]
pub fn add_agent(app: AppHandle, st: State<AppState>, team_id: String, input: AgentInput) -> R<String> {
    let id = team::add_agent(&st.db, &st.you_id, &team_id, input).map_err(e)?;
    changed(&app, "actors");
    // It landed on its role's usual columns: it may take their waiting cards.
    changed(&app, "workflow_states");
    pull_soon(&st);
    Ok(id)
}
#[tauri::command]
pub fn update_agent(app: AppHandle, st: State<AppState>, actor_id: String, input: AgentInput) -> R<()> {
    team::update_agent(&st.db, &st.you_id, &actor_id, input).map_err(e)?;
    changed(&app, "actors");
    resume(&st, &actor_id);
    Ok(())
}
/// The agent form's Folders: why each folder is refused, or a warning (a project's main checkout).
#[tauri::command]
pub fn check_agent_folders(st: State<AppState>, folders: Vec<gizai_core::folders::Folder>) -> Vec<gizai_core::folders::FolderCheck> {
    crate::folders::check(&st, &folders)
}
#[tauri::command]
pub fn set_agent_status(app: AppHandle, st: State<AppState>, actor_id: String, status: String) -> R<()> {
    team::set_agent_status(&st.db, &st.you_id, &actor_id, &status).map_err(e)?;
    changed(&app, "actors");
    resume(&st, &actor_id);
    Ok(())
}
/// Adds a column after `after_id` (`team::add_state`), Manual and without agents. `category` is a kind in plain words
/// (waiting, work, testing, review, deploy, done, backlog) or a category.
#[tauri::command]
pub fn add_state(app: AppHandle, st: State<AppState>, team_id: String, name: String, after_id: String, category: String) -> R<String> {
    let cat = gizai_core::columns::category_of(&category)
        .ok_or_else(|| format!("a column is Waiting, Work, Testing, Review, Deploy, Done or Backlog, not {category}"))?;
    let id = team::add_state(&st.db, &st.you_id, &team_id, &name, &after_id, cat).map_err(e)?;
    changed(&app, "workflow_states");
    Ok(id)
}
/// Sets up a column: its agents, Auto or Manual, its next column, its name and its place (`columns::set_column`).
#[tauri::command]
pub fn set_column(app: AppHandle, st: State<AppState>, state_id: String, input: gizai_core::columns::ColumnInput) -> R<()> {
    gizai_core::columns::set_column(&st.db, &st.you_id, &state_id, input).map_err(e)?;
    changed(&app, "workflow_states");
    pull_soon(&st);
    Ok(())
}
/// Puts an agent on a column (dragged from the organisation chart, or "+ Agent").
#[tauri::command]
pub fn add_column_agent(app: AppHandle, st: State<AppState>, state_id: String, agent_id: String) -> R<()> {
    gizai_core::columns::add_agent(&st.db, &st.you_id, &state_id, &agent_id).map_err(e)?;
    changed(&app, "workflow_states");
    pull_soon(&st);
    Ok(())
}
/// Takes an agent off a column (×). Its running cards finish as usual.
#[tauri::command]
pub fn remove_column_agent(app: AppHandle, st: State<AppState>, state_id: String, agent_id: String) -> R<()> {
    gizai_core::columns::remove_agent(&st.db, &st.you_id, &state_id, &agent_id).map_err(e)?;
    changed(&app, "workflow_states");
    Ok(())
}
/// What removing a column does, for its confirm (`columns::removal`).
#[tauri::command]
pub fn column_removal(st: State<AppState>, state_id: String) -> R<gizai_core::columns::Removal> {
    gizai_core::columns::removal(&st.db, &state_id).map_err(e)
}
/// Removes a column, moving its cards to `target_id` (`columns::remove_state`). On an Auto column, its agents pick them up.
#[tauri::command]
pub fn remove_state(app: AppHandle, st: State<AppState>, state_id: String, target_id: String) -> R<()> {
    gizai_core::columns::remove_state(&st.db, &st.you_id, &state_id, &target_id).map_err(e)?;
    changed(&app, "workflow_states");
    changed(&app, "tasks");
    pull_soon(&st);
    Ok(())
}
/// Every label with the number of cards that carry it (Team → Labels).
#[tauri::command]
pub fn list_labels(st: State<AppState>) -> R<Vec<gizai_core::labels::LabelInfo>> { gizai_core::labels::list(&st.db).map_err(e) }
/// Creates a label (`id` None) or renames or recolours one; returns its id. Names are unique, ignoring case.
#[tauri::command]
pub fn save_label(app: AppHandle, st: State<AppState>, id: Option<String>, name: String, color: Option<String>) -> R<String> {
    let id = gizai_core::labels::save(&st.db, &st.you_id, id.as_deref(), &name, color.as_deref()).map_err(e)?;
    changed(&app, "labels");
    Ok(id)
}
/// Removes a label from every card; returns how many cards carried it.
#[tauri::command]
pub fn remove_label(app: AppHandle, st: State<AppState>, id: String) -> R<i64> {
    let n = gizai_core::labels::remove(&st.db, &st.you_id, &id).map_err(e)?;
    changed(&app, "labels");
    changed(&app, "tasks");
    Ok(n)
}
/// Adds a branch to the team's organisation chart; returns the branches.
#[tauri::command]
pub fn add_branch(app: AppHandle, st: State<AppState>, team_id: String, name: String) -> R<Vec<team::Branch>> {
    let out = team::add_branch(&st.db, &st.you_id, &team_id, &name).map_err(e)?;
    changed(&app, "teams");
    Ok(out)
}
/// Removes a branch without agents from the team's organisation chart; returns the branches.
#[tauri::command]
pub fn remove_branch(app: AppHandle, st: State<AppState>, team_id: String, key: String) -> R<Vec<team::Branch>> {
    let out = team::remove_branch(&st.db, &st.you_id, &team_id, &key).map_err(e)?;
    changed(&app, "teams");
    Ok(out)
}
#[tauri::command]
pub fn rename_state(app: AppHandle, st: State<AppState>, state_id: String, name: String) -> R<()> {
    team::rename_state(&st.db, &st.you_id, &state_id, &name).map_err(e)?;
    changed(&app, "workflow_states");
    Ok(())
}
/// The starting instructions for a role, to prefill the agent form.
#[tauri::command]
pub fn role_template(role: String) -> String { gizai_core::seed::role_template(&team::role_key(&role)) }
/// The allowed commands a role starts with, to prefill the agent form.
#[tauri::command]
pub fn role_tools(role: String) -> Vec<String> { gizai_core::seed::role_tools(&team::role_key(&role)) }

// ---- agent runs ----
use crate::runs;

#[tauri::command]
pub async fn detect_claude(st: State<'_, AppState>) -> R<Option<String>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runs::detect_claude(&st)).await.map_err(|err| err.to_string())
}
#[tauri::command]
pub fn get_settings(st: State<AppState>) -> runs::Settings { runs::get_settings(&st) }
#[tauri::command]
pub fn save_settings(app: AppHandle, st: State<AppState>, settings: runs::Settings) -> R<()> {
    runs::save_settings(&st, &settings)?;
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
#[tauri::command]
pub fn stop_run(st: State<AppState>, run_id: String) { runs::stop(&st, &run_id) }
#[tauri::command]
pub fn list_runs(st: State<AppState>, task_id: String) -> R<Vec<Run>> { gizai_core::runs::list_for_task(&st.db, &task_id).map_err(e) }
#[tauri::command]
pub fn run_events(st: State<AppState>, run_id: String) -> Vec<runs::SeqEvent> { runs::events_for(&st, &run_id) }
/// The commits a finished run made, oldest first (the Runs tab).
#[tauri::command]
pub async fn run_commits(st: State<'_, AppState>, run_id: String) -> R<Vec<gizai_agents::worktree::Commit>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runs::commits(&st, &run_id)).await.map_err(|err| err.to_string())?
}
#[tauri::command]
pub fn live_runs(st: State<AppState>) -> Vec<runs::LiveRun> { runs::live(&st) }
/// Who a Run without a chosen agent would start now (the assigned agent, else the first agent on the card's column).
#[tauri::command]
pub fn suggest_agent(st: State<AppState>, task_id: String) -> Option<String> { runs::suggest(&st, &task_id) }

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
pub fn get_agent(st: State<AppState>, id: String) -> R<team::Member> { team::agent(&st.db, &id).map_err(e) }

/// An agent's runs per day for the last `days` days (agent page charts), and its recent runs.
#[tauri::command]
pub fn agent_stats(st: State<AppState>, id: String, days: i64) -> R<Vec<DayStat>> {
    gizai_core::runs::daily_stats(&st.db, &id, days.clamp(1, 90), gizai_core::ids::now_ms()).map_err(e)
}
#[tauri::command]
pub fn agent_runs(st: State<AppState>, id: String, limit: i64) -> R<Vec<Run>> { gizai_core::runs::list_for_agent(&st.db, &id, limit.clamp(1, 200)).map_err(e) }
/// The card this agent would pick up next (Run on the agent page).
#[tauri::command]
pub fn agent_next_task(st: State<AppState>, id: String) -> R<Option<String>> { gizai_core::workflow::next_task_for(&st.db, &id).map_err(e) }

/// The Usage page: the tokens and API cost of the runs and chat turns in `period` (today, 7d, 30d or month), in total, per
/// day, per agent and per project.
#[tauri::command]
pub fn usage_summary(st: State<AppState>, period: String) -> R<gizai_core::usage::Usage> {
    gizai_core::usage::for_period(&st.db, &period, gizai_core::ids::now_ms()).map_err(e)
}

/// The Usage page's Subscription tab: per coding CLI, the newest reading of each of its limits and the agents on it.
#[tauri::command]
pub fn subscription_limits(st: State<AppState>) -> R<Vec<gizai_core::limits::CliLimits>> {
    crate::limits::subscription(&st)
}

// ---- chat with the Team Lead ----
use crate::chat;

/// The chats, newest activity first: the `limit` newest (Chat → Recent), or all of them (the Inbox).
#[tauri::command]
pub fn list_chat_threads(st: State<AppState>, limit: Option<usize>) -> R<Vec<gizai_core::chat::ChatThread>> {
    match limit {
        Some(n) => gizai_core::chat::recent_threads(&st.db, n),
        None => gizai_core::chat::list_threads(&st.db),
    }.map_err(e)
}
/// One chat, also one older than those in Recent (opened from the Archive).
#[tauri::command]
pub fn get_chat_thread(st: State<AppState>, thread_id: String) -> R<gizai_core::chat::ChatThread> { gizai_core::chat::get_thread(&st.db, &thread_id).map_err(e) }
/// Chat → Archive: the chats whose title or messages (yours and the Team Lead's) hold `query`; all of them for an empty one.
#[tauri::command]
pub fn search_chat_threads(st: State<AppState>, query: String) -> R<Vec<gizai_core::chat::ThreadHit>> { gizai_core::chat::search_threads(&st.db, &query).map_err(e) }
#[tauri::command]
pub fn chat_messages(st: State<AppState>, thread_id: String) -> R<Vec<gizai_core::chat::ChatMessage>> { gizai_core::chat::messages(&st.db, &thread_id).map_err(e) }
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
pub fn item_id(st: State<AppState>, kind: String, key: String) -> R<String> {
    match kind.as_str() {
        "task" => tasks::get(&st.db, &key).map(|t| t.id).or_else(|_| tasks::id_of(&st.db, &key)).map_err(e),
        "project" => match projects::get(&st.db, &key) {
            Ok(p) => Ok(p.id),
            Err(_) => projects::list(&st.db).map_err(e)?.into_iter().find(|p| p.key.eq_ignore_ascii_case(&key)).map(|p| p.id)
                .ok_or_else(|| format!("No project with the key {key}")),
        },
        _ => Ok(key),
    }
}
/// The chat's queued messages: they wait while the Team Lead answers.
#[tauri::command]
pub fn chat_queue(st: State<AppState>, thread_id: String) -> R<Vec<gizai_core::chat::QueuedMessage>> { chat::queue(&st, &thread_id) }
#[tauri::command]
pub fn edit_queued_chat(st: State<AppState>, id: String, text: String) -> R<gizai_core::chat::QueuedMessage> { chat::edit_queued(&st, &id, &text) }
#[tauri::command]
pub fn remove_queued_chat(st: State<AppState>, id: String) -> R<()> { chat::remove_queued(&st, &id) }
/// Send now: the chat's queued messages go together (after the answer being written, if there is one).
#[tauri::command]
pub async fn send_chat_queue(st: State<'_, AppState>, thread_id: String) -> R<()> { chat::send_queue(&st, &thread_id, None).await.map(|_| ()) }
/// Runs on under the text box: the chat's next answers run on `cli` (None: on the Team Lead's Runs on).
#[tauri::command]
pub fn set_chat_cli(st: State<AppState>, thread_id: String, cli: Option<String>) -> R<gizai_core::chat::ChatThread> { chat::set_cli(&st, &thread_id, cli.as_deref()) }
/// Answer on <CLI> under a usage-limit note: the chat moves to `cli` and the message goes again there.
#[tauri::command]
pub async fn answer_chat_on(st: State<'_, AppState>, thread_id: String, cli: String, note_id: String) -> R<()> {
    chat::answer_on(&st, &thread_id, &cli, &note_id, None).await.map(|_| ())
}
/// The coding CLIs Runs on lists under the chat's text box, with why one can't run the chat.
#[tauri::command]
pub fn chat_clis(st: State<AppState>) -> R<Vec<chat::ChatCli>> { chat::clis(&st) }
#[tauri::command]
pub fn stop_chat(st: State<AppState>, thread_id: String) { chat::stop(&st, &thread_id) }
/// Chat answers being written right now, with their text so far.
#[tauri::command]
pub fn chat_live(st: State<AppState>) -> Vec<chat::ChatStatus> { chat::live(&st) }
/// The agent that answers on the Chat page, if any.
#[tauri::command]
pub fn chat_agent(st: State<AppState>) -> R<Option<team::Member>> { team::chat_agent(&st.db).map_err(e) }
/// × on a Team Lead chat in the Inbox: it no longer waits for you (it stays in Chat → Recent).
#[tauri::command]
pub fn dismiss_chat(app: AppHandle, st: State<AppState>, thread_id: String) -> R<()> {
    gizai_core::chat::dismiss(&st.db, &st.you_id, &thread_id).map_err(e)?;
    changed(&app, "chat_threads");
    Ok(())
}

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
#[tauri::command]
pub fn github_login_cancel(st: State<AppState>) { crate::github::login_cancel(&st) }

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

// ---- updates (Settings → Updates, the notice above Company) ----
#[tauri::command]
pub fn update_status(st: State<AppState>) -> crate::update::UpdateStatus { crate::update::status(&st) }
/// Asks GitHub for the latest release now.
#[tauri::command]
pub async fn check_for_updates(st: State<'_, AppState>) -> R<crate::update::UpdateStatus> { Ok(crate::update::check(&st).await) }
#[tauri::command]
pub fn set_update_auto_check(st: State<AppState>, on: bool) -> R<crate::update::UpdateStatus> { crate::update::set_auto_check(&st, on) }
/// Builds and installs `version` in the background; returns at once.
#[tauri::command]
pub fn start_update(st: State<AppState>, version: String) -> R<crate::update::UpdateStatus> { crate::update::start(&st, &version) }
#[tauri::command]
pub fn stop_update(st: State<AppState>) -> crate::update::UpdateStatus { crate::update::stop(&st) }
/// Quits and starts the Gizai an update installed: agents at work are stopped first, as when you quit.
#[tauri::command]
pub fn restart_gizai(app: AppHandle, st: State<AppState>) -> R<()> {
    let mut cmd = crate::update::restart_command(&st)?;
    cmd.spawn().map_err(|e| format!("Couldn't restart Gizai: {e}"))?;
    app.exit(0);
    Ok(())
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

/// Saves the agent's Web and Built-in tool switches. Only you: no Team Lead tool calls this.
#[tauri::command]
pub async fn save_agent_cli_tools(app: AppHandle, st: State<'_, AppState>, agent_id: String, tools: gizai_core::mcp_servers::CliTools)
    -> R<gizai_core::mcp_servers::CliTools> {
    let st = st.inner().clone();
    let out = blocking(move || crate::mcp_servers::save_cli_tools(&st, &agent_id, tools)).await?;
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
