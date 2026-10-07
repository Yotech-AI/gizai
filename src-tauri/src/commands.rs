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

/// After a card changes, an agent that wakes up on assignment may start on it.
fn wake(st: &State<AppState>, task_id: &str) {
    let (st, id) = (st.inner().clone(), task_id.to_string());
    tauri::async_runtime::spawn(async move { crate::runs::dispatch(&st, &id).await; });
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
    Ok(id)
}
#[tauri::command]
pub fn update_agent(app: AppHandle, st: State<AppState>, actor_id: String, input: AgentInput) -> R<()> {
    team::update_agent(&st.db, &st.you_id, &actor_id, input).map_err(e)?;
    changed(&app, "actors");
    Ok(())
}
#[tauri::command]
pub fn set_agent_status(app: AppHandle, st: State<AppState>, actor_id: String, status: String) -> R<()> {
    team::set_agent_status(&st.db, &st.you_id, &actor_id, &status).map_err(e)?;
    changed(&app, "actors");
    Ok(())
}
#[tauri::command]
pub fn add_rule(app: AppHandle, st: State<AppState>, team_id: String, input: RuleInput) -> R<String> {
    let id = team::add_rule(&st.db, &st.you_id, &team_id, input).map_err(e)?;
    changed(&app, "routing_rules");
    Ok(id)
}
#[tauri::command]
pub fn delete_rule(app: AppHandle, st: State<AppState>, rule_id: String) -> R<()> {
    team::delete_rule(&st.db, &st.you_id, &rule_id).map_err(e)?;
    changed(&app, "routing_rules");
    Ok(())
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
/// Continue a stopped run: resume its session (see runs::continue_run).
#[tauri::command]
pub async fn continue_run(st: State<'_, AppState>, run_id: String) -> R<String> {
    let (id, _done) = runs::continue_run(&st, &run_id, None).await?;
    Ok(id)
}
#[tauri::command]
pub fn stop_run(st: State<AppState>, run_id: String) { runs::stop(&st, &run_id) }
#[tauri::command]
pub fn list_runs(st: State<AppState>, task_id: String) -> R<Vec<Run>> { gizai_core::runs::list_for_task(&st.db, &task_id).map_err(e) }
#[tauri::command]
pub fn run_events(st: State<AppState>, run_id: String) -> Vec<runs::SeqEvent> { runs::events_for(&st, &run_id) }
#[tauri::command]
pub fn live_runs(st: State<AppState>) -> Vec<runs::LiveRun> { runs::live(&st) }
/// Who a Run without a chosen agent would start now (assigned agent first, then routing).
#[tauri::command]
pub fn suggest_agent(st: State<AppState>, task_id: String) -> Option<String> { runs::suggest(&st, &task_id) }

/// The models this user's Claude Code offers (as its /model picker lists them), with their effort levels.
#[tauri::command]
pub async fn claude_models(st: State<'_, AppState>, refresh: bool) -> R<Vec<gizai_agents::models::ModelOption>> {
    runs::models(&st, refresh).await
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

// ---- chat with the Team Lead ----
use crate::chat;

#[tauri::command]
pub fn list_chat_threads(st: State<AppState>) -> R<Vec<gizai_core::chat::ChatThread>> { gizai_core::chat::list_threads(&st.db).map_err(e) }
#[tauri::command]
pub fn chat_messages(st: State<AppState>, thread_id: String) -> R<Vec<gizai_core::chat::ChatMessage>> { gizai_core::chat::messages(&st.db, &thread_id).map_err(e) }
/// Sends a message (in a new thread when `thread_id` is None) and starts the Team Lead's answer. Returns the thread id.
#[tauri::command]
pub async fn send_chat(st: State<'_, AppState>, thread_id: Option<String>, text: String) -> R<String> {
    let (id, _done) = chat::send(&st, thread_id, text, None).await?;
    Ok(id)
}
#[tauri::command]
pub fn stop_chat(st: State<AppState>, thread_id: String) { chat::stop(&st, &thread_id) }
/// Chat answers being written right now, with their text so far.
#[tauri::command]
pub fn chat_live(st: State<AppState>) -> Vec<chat::ChatStatus> { chat::live(&st) }
/// The agent that answers on the Chat page, if any.
#[tauri::command]
pub fn chat_agent(st: State<AppState>) -> R<Option<team::Member>> { team::chat_agent(&st.db).map_err(e) }

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
