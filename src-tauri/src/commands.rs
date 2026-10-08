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

/// After a card changes, agents that wake up when a card is routed or assigned to them take their next cards.
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
    resume(&st, &actor_id);
    Ok(())
}
#[tauri::command]
pub fn set_agent_status(app: AppHandle, st: State<AppState>, actor_id: String, status: String) -> R<()> {
    team::set_agent_status(&st.db, &st.you_id, &actor_id, &status).map_err(e)?;
    changed(&app, "actors");
    resume(&st, &actor_id);
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
/// Adds a column after `after_id` (`team::add_state`).
#[tauri::command]
pub fn add_state(app: AppHandle, st: State<AppState>, team_id: String, name: String, after_id: String, category: String,
                 owner_role: Option<String>) -> R<String> {
    let id = team::add_state(&st.db, &st.you_id, &team_id, &name, &after_id, &category, owner_role.as_deref()).map_err(e)?;
    changed(&app, "workflow_states");
    Ok(id)
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
/// The commits a finished run made, oldest first (the Runs tab).
#[tauri::command]
pub async fn run_commits(st: State<'_, AppState>, run_id: String) -> R<Vec<gizai_agents::worktree::Commit>> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || runs::commits(&st, &run_id)).await.map_err(|err| err.to_string())?
}
#[tauri::command]
pub fn live_runs(st: State<AppState>) -> Vec<runs::LiveRun> { runs::live(&st) }
/// Who a Run without a chosen agent would start now (assigned agent first, then routing).
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
