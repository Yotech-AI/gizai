pub mod chat;
mod commands;
pub mod git;
pub mod mcp;
pub mod pulls;
mod quit;
pub mod runs;
pub mod tools;
pub mod worktrees;

use gizai_core::db::Db;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Db>,
    pub you_id: String,
    pub data_dir: PathBuf,
    pub runs: Arc<runs::RunManager>,
    /// Where the MCP server for chat turns listens (see `mcp::socket_path`).
    pub mcp_socket: PathBuf,
    /// The `gizai-mcp` shim Claude Code starts in a chat turn (None: not found next to Gizai).
    pub mcp_shim: Option<PathBuf>,
    pub chat: Arc<chat::ChatManager>,
    /// Pull requests being opened or checked on GitHub (see `pulls`).
    pub pulls: Arc<pulls::PullChecks>,
    /// Held while this Gizai runs: one Gizai per data folder (see `lock_data_dir`).
    pub _lock: Arc<std::fs::File>,
    /// Tells the UI what changed (rows, runs, live run events). A no-op in tests.
    pub notify: Arc<dyn Fn(runs::Note) + Send + Sync>,
}

#[derive(Serialize)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub selftest: bool,
    pub you_id: String,
    pub start_route: Option<String>,
    /// Which extra self-test to run (GIZAI_SELFTEST_MODE), e.g. "run".
    pub selftest_mode: Option<String>,
    /// Set when this Gizai runs on data other than the usual folder (a dev build, a test): see `data_label`.
    pub data_label: Option<String>,
}

#[tauri::command]
fn app_info(st: tauri::State<AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: st.data_dir.display().to_string(),
        selftest: std::env::var("GIZAI_SELFTEST").is_ok(),
        you_id: st.you_id.clone(),
        start_route: std::env::var("GIZAI_ROUTE").ok().filter(|r| !r.is_empty()),
        selftest_mode: std::env::var("GIZAI_SELFTEST_MODE").ok().filter(|m| !m.is_empty()),
        data_label: data_label(&st.data_dir),
    }
}

/// Writes the UI's self-test report (used by scripts/smoke-cage.sh). No-op unless GIZAI_SELFTEST is set.
#[tauri::command]
fn selftest_report(json: String) -> Result<(), String> {
    match std::env::var("GIZAI_SELFTEST") {
        Ok(path) => std::fs::write(path, json).map_err(|e| e.to_string()),
        Err(_) => Ok(()),
    }
}

#[tauri::command]
fn exit_app(app: AppHandle, code: i32) {
    app.exit(code);
}

/// The data folder: GIZAI_DATA_DIR, else the usual one.
pub fn data_dir() -> PathBuf {
    match std::env::var("GIZAI_DATA_DIR") {
        Ok(d) if !d.is_empty() => d.into(),
        _ => default_data_dir(),
    }
}

/// `$XDG_DATA_HOME/gizai`, else `~/.local/share/gizai`: the data of the Gizai you use.
pub fn default_data_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".local/share"));
    base.join("gizai")
}

/// 12 hex characters naming a data folder (the same folder however it is spelled).
pub fn data_dir_key(dir: &std::path::Path) -> String {
    let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    gizai_core::tokens::sha256_hex(&canon.to_string_lossy())[..12].to_string()
}

/// The D-Bus name that keeps Gizai to one window per data folder, so a test Gizai with its own data can run
/// next to the one you use.
pub fn instance_id(dir: &std::path::Path) -> String {
    format!("ai.gizai.app.d{}", data_dir_key(dir))
}

/// A short name shown in the window title and sidebar when Gizai runs on data other than the usual folder.
pub fn data_label(dir: &std::path::Path) -> Option<String> {
    if dir == default_data_dir() {
        return None;
    }
    Some(dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string()))
}

fn display_name() -> String {
    let user = std::env::var("USER").unwrap_or_else(|_| "You".into());
    let mut c = user.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "You".into(),
    }
}

/// One Gizai per data folder: a second one would treat the first one's running agents as left behind and stop
/// them, and take over its chat socket. An exclusive lock on `<dir>/gizai.lock`, released when Gizai exits.
fn lock_data_dir(dir: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::unix::io::AsRawFd;
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("gizai.lock"))
        .map_err(|e| format!("can't open the lock in {}: {e}", dir.display()))?;
    // SAFETY: flock on a file descriptor we own.
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!("Gizai is already running with the data in {}", dir.display()));
    }
    Ok(f)
}

/// A snapshot of the data in `dir` (its database, consistent even while Gizai runs) in `dir/backups`, named
/// `gizai-<label>-<time>.db`.
pub fn backup_data_dir(dir: &std::path::Path, label: &str) -> Result<PathBuf, String> {
    if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("a backup name is letters, digits and dashes, not {label:?}"));
    }
    let db = dir.join("gizai.db");
    if !db.exists() {
        return Err(format!("no Gizai data in {}", dir.display()));
    }
    gizai_core::db::snapshot(&db, &dir.join("backups"), label).map_err(|e| e.to_string())
}

/// Opens (and on first start seeds) the database in `dir`. Runs a previous Gizai left behind are marked interrupted.
pub fn open_state(dir: PathBuf, notify: Arc<dyn Fn(runs::Note) + Send + Sync>) -> Result<AppState, String> {
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let lock = lock_data_dir(&dir)?;
    let db = Db::open(&dir.join("gizai.db")).map_err(|e| e.to_string())?;
    let seed = gizai_core::seed::ensure_seed(&db, &display_name()).map_err(|e| e.to_string())?;
    // Runs a previous Gizai left running: end their claude process groups (only when /proc proves they are
    // ours), then mark them interrupted and release their cards.
    for r in gizai_core::runs::active(&db).unwrap_or_default() {
        if let (Some(pid), Some(wt)) = (r.pid, r.worktree_path.as_deref()) {
            gizai_agents::process::end_orphan_group(pid as u32, std::path::Path::new(wt));
        }
    }
    let _ = gizai_core::runs::recover_interrupted(&db);
    chat::remove_stray_configs(&dir);
    let mcp_socket = mcp::socket_path(&dir);
    Ok(AppState { db: Arc::new(db), you_id: seed.you_id, data_dir: dir, runs: Arc::new(runs::RunManager::default()), mcp_socket,
                  mcp_shim: mcp::shim_bin(), chat: Arc::new(chat::ChatManager::default()), pulls: Arc::new(pulls::PullChecks::default()),
                  _lock: Arc::new(lock), notify })
}

fn ui_notifier(app: AppHandle) -> Arc<dyn Fn(runs::Note) + Send + Sync> {
    Arc::new(move |note| {
        let _ = match note {
            runs::Note::RowsChanged(table) => app.emit("rows-changed", serde_json::json!({ "table": table })),
            runs::Note::RunsChanged => app.emit("runs-changed", ()),
            runs::Note::Event { run_id, seq, event } => app.emit("run-event", serde_json::json!({ "runId": run_id, "seq": seq, "event": event })),
            runs::Note::Chat { thread_id, event } => {
                let mut v = serde_json::to_value(&event).unwrap_or_default();
                v["threadId"] = serde_json::Value::String(thread_id);
                app.emit("chat-event", v)
            }
            runs::Note::ChatChanged => app.emit("chat-changed", ()),
        };
    })
}

#[doc(hidden)]
pub fn test_state(dir: &std::path::Path) -> AppState {
    let mut st = open_state(dir.join("data"), Arc::new(|_| {})).expect("test state");
    // Tests keep their socket in their own folder, never in the real runtime dir.
    st.mcp_socket = st.data_dir.join("mcp.sock");
    st
}

/// A task in To do, labelled `label`, in project KADE (repo `repo`), with a "<Label> Agent" and a rule
/// label → role. Reuses the project, agent and rule on later calls.
#[doc(hidden)]
pub fn test_task(st: &AppState, repo: &str, label: &str) -> String {
    use gizai_core::model::*;
    let db = &st.db;
    let project = match gizai_core::projects::list(db).unwrap().into_iter().find(|p| p.key == "KADE") {
        Some(p) => p.id,
        None => gizai_core::projects::create(db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(),
            repo_path: Some(repo.to_string()).filter(|r| !r.is_empty()), ..Default::default() }).unwrap(),
    };
    let team_id = gizai_core::team::list(db).unwrap()[0].id.clone();
    let team = gizai_core::team::get(db, &team_id).unwrap();
    if !team.members.iter().any(|m| m.kind == "agent" && m.role_key == label) {
        let name = format!("{}{} Agent", label[..1].to_uppercase(), &label[1..]);
        gizai_core::team::add_agent(db, &st.you_id, &team_id, AgentInput { name, role_key: label.into(), ..Default::default() }).unwrap();
        gizai_core::team::add_rule(db, &st.you_id, &team_id, RuleInput { kind: "label".into(), match_name: label.into(), target_role: label.into(), priority: 10 }).unwrap();
    }
    let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
    let lbl = team.labels.iter().find(|l| l.name == label).unwrap().id.clone();
    let t = gizai_core::tasks::create(db, &st.you_id, TaskInput { project_id: project, title: "Export invoices as CSV".into(), state_id: Some(todo),
        label_ids: vec![lbl], ..Default::default() }).unwrap();
    t
}

pub fn run() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir); // so the instance id names the folder the same way from the first start
    tauri::Builder::default()
        // Starting Gizai again on the same data (the launcher, the gizai command) brings the open window forward.
        .plugin(tauri_plugin_single_instance::Builder::new()
            .dbus_id(instance_id(&dir))
            .callback(|app, _args, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            })
            .build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            if let (Some(label), Some(w)) = (data_label(&dir), app.get_webview_window("main")) {
                let _ = w.set_title(&format!("Gizai ({label} data)"));
            }
            let state = match open_state(dir.clone(), ui_notifier(app.handle().clone())) {
                Ok(s) => s,
                Err(e) => {
                    // e.g. another Gizai holds this data folder (from another login session)
                    eprintln!("gizai: {e}");
                    quit::end_web_content(app.handle());
                    std::process::exit(1);
                }
            };
            app.manage(state.clone());
            // After manage: quitting reads the state.
            quit::on_signals(app.handle());
            // The MCP server chat turns reach Gizai's tools through.
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = mcp::start(&st) {
                        eprintln!("gizai: the chat tools socket {} could not start: {e}", st.mcp_socket.display());
                    }
                });
            }
            // The pull request check: at start and every two minutes, cards in Review (and open pull requests) hear
            // what happened on GitHub; a merge moves its card to Done.
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let mut tick = tokio::time::interval(pulls::CHECK_EVERY);
                    loop {
                        tick.tick().await;
                        let _ = pulls::check_all(&st).await;
                    }
                });
            }
            // Heartbeats: once a minute, agents whose interval has passed look for their next card.
            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    tick.tick().await;
                    let _ = runs::heartbeat_tick(&state, gizai_core::ids::now_ms()).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info, selftest_report, exit_app,
            commands::list_clients, commands::get_client, commands::save_client, commands::archive_client,
            commands::list_contacts, commands::save_contact, commands::remove_contact, commands::list_users, commands::add_user,
            commands::list_projects, commands::get_project, commands::save_project,
            commands::list_tasks, commands::get_task, commands::create_task, commands::update_task, commands::move_task,
            commands::set_task_labels, commands::task_activity, commands::list_comments, commands::add_comment,
            commands::list_teams, commands::get_team, commands::check_repo,
            commands::list_docs, commands::get_doc, commands::create_doc, commands::save_doc, commands::rename_doc,
            commands::doc_versions, commands::doc_version_body,
            commands::add_files, commands::list_files, commands::remove_file, commands::open_file,
            commands::add_team, commands::add_agent, commands::update_agent, commands::set_agent_status,
            commands::add_rule, commands::delete_rule, commands::rename_state, commands::role_template,
            commands::detect_claude, commands::get_settings, commands::save_settings, commands::start_run, commands::continue_run, commands::stop_run,
            commands::list_runs, commands::run_events, commands::live_runs, commands::suggest_agent, commands::get_agent, commands::claude_models, commands::agent_stats, commands::agent_runs, commands::agent_next_task,
            commands::list_chat_threads, commands::chat_messages, commands::send_chat, commands::stop_chat, commands::chat_live, commands::chat_agent,
            commands::open_pull_request, commands::check_pull_request, commands::detect_gh,
            commands::list_old_worktrees, commands::remove_old_worktrees,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Gizai")
        .run(|app, event| {
            // Quitting with agents at work (runs or a chat answer): stop them first, then quit (a second request
            // quits at once).
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                let st = app.state::<AppState>().inner().clone();
                if !runs::is_closing(&st) && (!runs::live(&st).is_empty() || !chat::live(&st).is_empty()) {
                    api.prevent_exit();
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let (a, b) = tokio::join!(runs::stop_all(&st, std::time::Duration::from_secs(12)), chat::stop_all(&st, std::time::Duration::from_secs(12)));
                        let _ = (a, b);
                        app.exit(0);
                    });
                }
            }
            // Nothing keeps the window open, so a close request means it closes now and Gizai quits. WebKit's
            // page process ends before the window goes (see `quit`); Exit does the same for every other way out.
            if let tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::CloseRequested { .. }, .. } = &event
                && label == "main"
            {
                quit::end_web_content(app);
            }
            if let tauri::RunEvent::Exit = event {
                quit::end_web_content(app);
                mcp::remove_socket(app.state::<AppState>().inner());
            }
        });
}
