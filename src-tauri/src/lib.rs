pub mod bitbucket;
pub mod board;
pub mod chat;
pub mod clis;
pub mod code;
mod commands;
pub mod folders;
pub mod git;
pub mod github;
pub mod limits;
pub mod mcp;
pub mod mcp_servers;
pub mod notifications;
pub mod pulls;
mod quit;
pub mod runs;
pub mod shell_path;
pub mod tools;
pub mod update;
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
    /// The Team Lead's read-only copies of the projects' code (see `code`).
    pub code: Arc<code::Copies>,
    /// Pull requests being opened or checked on GitHub (see `pulls`).
    pub pulls: Arc<pulls::PullChecks>,
    /// Log in with GitHub, while gh waits for its code (see `github`).
    pub github: Arc<github::Logins>,
    /// The release check and the update (see `update`).
    pub updates: Arc<update::Updates>,
    /// Held while this Gizai runs: one Gizai per data folder (see `lock_data_dir`).
    pub _lock: Arc<std::fs::File>,
    /// Where MCP servers' secrets live: the OS keychain, or a file standing in for it (GIZAI_FAKE_KEYCHAIN) in tests.
    pub keychain: Arc<dyn gizai_agents::secrets::Keychain>,
    /// MCP servers' sign-ins in that keychain, refreshed one at a time per server.
    pub tokens: Arc<gizai_agents::oauth::TokenStore>,
    /// Tells the UI what changed (rows, runs, live run events). A no-op in tests.
    pub notify: Arc<dyn Fn(runs::Note) + Send + Sync>,
    /// Shows desktop notifications and says whether the window is out of sight (see `notifications`). Shows nothing
    /// until `run` sets the real one; tests set their own.
    pub desktop: notifications::Desktop,
    /// The Inbox as the notifications last saw it (see `notifications::watch`).
    pub inbox: Arc<notifications::Watch>,
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

/// Housekeeping (README → Your data): the chat tools' dead tokens, and the run and chat logs in `dir` older than 30 days,
/// go. When Gizai starts (`open_state`) and once a day while it runs.
pub fn housekeeping(db: &Db, dir: &std::path::Path) {
    match gizai_core::housekeeping::run(db, dir, gizai_core::ids::now_ms()) {
        Ok(p) if p.logs + p.tokens > 0 => eprintln!("gizai: housekeeping removed {} old logs and {} old tokens", p.logs, p.tokens),
        Ok(_) => {}
        Err(e) => eprintln!("gizai: housekeeping failed: {e}"),
    }
}

/// Opens (and on first start seeds) the database in `dir`. Runs a previous Gizai left behind are marked interrupted.
/// A first start is a new install: it gets the five default agents, on Codex when only Codex is installed
/// (`gizai_core::seed::ensure_seed_with_agents`).
pub fn open_state(dir: PathBuf, notify: Arc<dyn Fn(runs::Note) + Send + Sync>) -> Result<AppState, String> {
    open_data(dir, notify, true)
}

/// `open_state`, with or without the default agents on a first start.
fn open_data(dir: PathBuf, notify: Arc<dyn Fn(runs::Note) + Send + Sync>, default_agents: bool) -> Result<AppState, String> {
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let lock = lock_data_dir(&dir)?;
    let db = Db::open(&dir.join("gizai.db")).map_err(|e| e.to_string())?;
    let seed = if default_agents {
        gizai_core::seed::ensure_seed_with_agents(&db, &display_name(), clis::codex_at_first_start)
    } else {
        gizai_core::seed::ensure_seed(&db, &display_name())
    }.map_err(|e| e.to_string())?;
    // Runs a previous Gizai left running: end their claude process groups (only when /proc proves they are
    // ours), save where a card's run ended (a chat answer has no worktree of its own), then mark them interrupted
    // and release their cards.
    for r in gizai_core::runs::active(&db).unwrap_or_default() {
        if let (Some(pid), Some(wt)) = (r.pid, r.worktree_path.as_deref()) {
            gizai_agents::process::end_orphan_group(pid as u32, std::path::Path::new(wt));
        }
        if let (Some(_), Some(wt)) = (&r.task_id, r.worktree_path.as_deref()) {
            runs::record_head(&db, &r.id, std::path::Path::new(wt));
        }
    }
    let _ = gizai_core::runs::recover_interrupted(&db);
    // Messages queued in a chat wait for Send now: the answer they waited for is gone.
    let _ = gizai_core::chat::hold_all_queues(&db);
    chat::remove_stray_configs(&dir);
    housekeeping(&db, &dir);
    let mcp_socket = mcp::socket_path(&dir);
    let keychain = gizai_agents::secrets::from_env();
    let tokens = Arc::new(gizai_agents::oauth::TokenStore::new(keychain.clone()));
    Ok(AppState { keychain, tokens, db: Arc::new(db), you_id: seed.you_id, data_dir: dir, runs: Arc::new(runs::RunManager::default()), mcp_socket,
                  mcp_shim: mcp::shim_bin(), chat: Arc::new(chat::ChatManager::default()), code: Arc::new(code::Copies::default()), pulls: Arc::new(pulls::PullChecks::default()),
                  github: Arc::new(github::Logins::default()), updates: Arc::new(update::Updates::default()), _lock: Arc::new(lock), notify,
                  desktop: notifications::Desktop::silent(), inbox: Arc::new(notifications::Watch::default()) })
}

fn ui_notifier(app: AppHandle) -> Arc<dyn Fn(runs::Note) + Send + Sync> {
    Arc::new(move |note| {
        // Tasks or chats changed: the desktop notifications look at the Inbox again.
        if matches!(note, runs::Note::RowsChanged("tasks" | "chat_threads") | runs::Note::ChatChanged)
            && let Some(st) = app.try_state::<AppState>()
        {
            notifications::poke(st.inner());
        }
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
            runs::Note::UpdateChanged => app.emit("update-changed", ()),
        };
    })
}

/// Brings the main window back: unminimised, shown and focused. The tray's Open Gizai, starting Gizai again, the Dock
/// icon on macOS and a click on a notification.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// The tray icon: top right in Omarchy's Waybar, in the menu bar on macOS. A click opens its menu (on Linux a click
/// always does): Open Gizai, and Quit Gizai completely, which goes through `app.exit` like `exit_app`, so agents at
/// work are stopped first. On Linux it needs libayatana-appindicator (or the older libappindicator); without it there is
/// no tray icon, and starting Gizai again brings the window back.
fn tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    #[cfg(target_os = "linux")]
    if !appindicator_found() {
        eprintln!("gizai: no tray icon: libayatana-appindicator isn't installed (Arch and Omarchy: libayatana-appindicator; Debian and Ubuntu: libayatana-appindicator3-1)");
        return Ok(());
    }
    let open = MenuItem::with_id(app, "open", "Open Gizai", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Gizai completely", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    TrayIconBuilder::with_id("gizai")
        .icon(tauri::include_image!("icons/64x64.png"))
        .tooltip("Gizai")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Whether the tray's library loads: the tray (libappindicator-sys) panics without it. It stays loaded; the tray loads
/// the same one.
#[cfg(target_os = "linux")]
fn appindicator_found() -> bool {
    ["libayatana-appindicator3.so.1", "libappindicator3.so.1"].iter().any(|name| {
        let Ok(name) = std::ffi::CString::new(*name) else { return false };
        // SAFETY: dlopen with a NUL-terminated name; the handle is kept, never closed.
        !unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) }.is_null()
    })
}

/// Gizai for a test, on the data in `dir/data`. Its first start adds no agents: a new install's five would take the
/// test's cards, and would start the real Claude Code when the test hasn't set a fake one. Tests add the agents they
/// need; `open_state` starts like a new install.
#[doc(hidden)]
pub fn test_state(dir: &std::path::Path) -> AppState {
    let mut st = open_data(dir.join("data"), Arc::new(|_| {}), false).expect("test state");
    // Tests keep their socket in their own folder, never in the real runtime dir.
    st.mcp_socket = st.data_dir.join("mcp.sock");
    // Never the real keychain in tests: one in memory, unless the test names a file for it.
    if std::env::var_os("GIZAI_FAKE_KEYCHAIN").is_none() {
        st.keychain = Arc::new(gizai_agents::secrets::MemoryKeychain::default());
        st.tokens = Arc::new(gizai_agents::oauth::TokenStore::new(st.keychain.clone()));
    }
    st
}

/// A task in To do, labelled `label` and assigned to a "<Label> Agent" (role `label`; a new agent lands on its role's
/// usual columns), in project KADE (repo `repo`). Labels don't route: the assignment makes that agent start it. Reuses
/// the project and the agent on later calls.
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
    let agent = match team.members.iter().find(|m| m.kind == "agent" && m.role_key == label) {
        Some(m) => m.actor_id.clone(),
        None => {
            let name = format!("{}{} Agent", label[..1].to_uppercase(), &label[1..]);
            gizai_core::team::add_agent(db, &st.you_id, &team_id, AgentInput { name, role_key: label.into(), ..Default::default() }).unwrap()
        }
    };
    let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
    let lbl = team.labels.iter().find(|l| l.name == label).unwrap().id.clone();
    let t = gizai_core::tasks::create(db, &st.you_id, TaskInput { project_id: project, title: "Export invoices as CSV".into(), state_id: Some(todo),
        label_ids: vec![lbl], assignee_id: Some(agent), ..Default::default() }).unwrap();
    t
}

pub fn run() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir); // so the instance id names the folder the same way from the first start
    tauri::Builder::default()
        // Starting Gizai again on the same data (the launcher, the gizai command) brings the open window forward, also
        // when it was closed (hidden): on a desktop without a tray, that is how the window comes back.
        .plugin(tauri_plugin_single_instance::Builder::new()
            .dbus_id(instance_id(&dir))
            .callback(|app, _args, _cwd| show_main(app))
            .build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            if let (Some(label), Some(w)) = (data_label(&dir), app.get_webview_window("main")) {
                let _ = w.set_title(&format!("Gizai ({label} data)"));
            }
            // Headless test and screenshot runs.
            let test_run = ["GIZAI_SELFTEST", "GIZAI_ROUTE"].iter().any(|v| std::env::var_os(v).is_some());
            let mut state = match open_state(dir.clone(), ui_notifier(app.handle().clone())) {
                Ok(s) => s,
                Err(e) => {
                    // e.g. another Gizai holds this data folder (from another login session)
                    eprintln!("gizai: {e}");
                    quit::end_web_content(app.handle());
                    std::process::exit(1);
                }
            };
            // Desktop notifications go to the desktop (a test run only writes them to stderr).
            state.desktop = notifications::real(app.handle(), test_run);
            app.manage(state.clone());
            // After manage: quitting reads the state.
            quit::on_signals(app.handle());
            // The tray icon, with Open Gizai and Quit Gizai completely: closing the window only hides it.
            if let Err(e) = tray(app) {
                eprintln!("gizai: the tray icon could not start: {e}");
            }
            // Desktop notifications: the Inbox is watched from now on (what is in it now counts as seen).
            tauri::async_runtime::spawn(notifications::watch(state.clone()));
            // The MCP server chat turns reach Gizai's tools through.
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = mcp::start(&st) {
                        eprintln!("gizai: the chat tools socket {} could not start: {e}", st.mcp_socket.display());
                    }
                });
            }
            // The Team Lead's copies of the projects' code: the missing ones are made now, so the first answer rarely
            // waits for them, and copies that no longer belong go.
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move { code::startup(&st).await });
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
            // The release check: 20 seconds after start, then every ten minutes, Gizai asks GitHub for the latest
            // release when a check is due (Check for new releases is on, and the last check is six hours old).
            // Headless test and screenshot runs don't ask GitHub, unless they point the check at a fake release.
            if !test_run || std::env::var_os("GIZAI_RELEASES_URL").is_some() {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(update::FIRST_CHECK_AFTER).await;
                    let mut tick = tokio::time::interval(update::TICK);
                    loop {
                        tick.tick().await;
                        update::tick(&st, gizai_core::ids::now_ms()).await;
                    }
                });
            }
            // Housekeeping once a day while Gizai runs (open_state did it at start). The hourly tick goes by the wall
            // clock, so the hours the computer slept count too.
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let hour = std::time::Duration::from_secs(60 * 60);
                    let mut last = gizai_core::ids::now_ms();
                    let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + hour, hour);
                    loop {
                        tick.tick().await;
                        let now = gizai_core::ids::now_ms();
                        if now - last >= gizai_core::housekeeping::EVERY_MS {
                            last = now;
                            let st = st.clone();
                            let _ = tokio::task::spawn_blocking(move || housekeeping(&st.db, &st.data_dir)).await;
                        }
                    }
                });
            }
            // Once a minute the agents take cards that had to wait (the run limit was full, Gizai just started), and the
            // Team Lead checks the board when its interval has passed; only new findings start it.
            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
                loop {
                    tick.tick().await;
                    let _ = runs::pull(&state).await;
                    let _ = board::tick(&state, gizai_core::ids::now_ms()).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info, selftest_report, exit_app,
            commands::list_mcp_servers, commands::save_mcp_server, commands::remove_mcp_server, commands::list_mcp_tools,
            commands::scan_claude_code_mcp, commands::import_mcp_servers, commands::mcp_sign_in, commands::mcp_sign_out,
            commands::agent_mcp, commands::save_agent_mcp,
            commands::list_clients, commands::get_client, commands::save_client, commands::archive_client,
            commands::list_contacts, commands::save_contact, commands::remove_contact, commands::list_users, commands::add_user,
            commands::list_projects, commands::get_project, commands::save_project,
            commands::list_tasks, commands::get_task, commands::create_task, commands::update_task, commands::move_task,
            commands::set_task_labels, commands::task_activity, commands::list_comments, commands::add_comment,
            commands::list_archived_tasks, commands::archive_task, commands::restore_task,
            commands::list_teams, commands::get_team, commands::check_repo,
            commands::list_docs, commands::get_doc, commands::create_doc, commands::save_doc, commands::rename_doc,
            commands::doc_versions, commands::doc_version_body,
            commands::add_files, commands::list_files, commands::remove_file, commands::open_file,
            commands::add_team, commands::add_agent, commands::update_agent, commands::set_agent_status, commands::check_agent_folders,
            commands::rename_state, commands::add_state, commands::set_column, commands::add_column_agent, commands::remove_column_agent,
            commands::column_removal, commands::remove_state, commands::list_labels, commands::save_label, commands::remove_label,
            commands::add_branch, commands::remove_branch, commands::role_template, commands::role_tools,
            commands::detect_claude, commands::get_settings, commands::save_settings, commands::start_run, commands::continue_run,
            commands::continue_after_run_for_me, commands::stop_run,
            commands::list_runs, commands::run_events, commands::run_commits, commands::live_runs, commands::suggest_agent, commands::get_agent, commands::claude_models, commands::list_clis, commands::save_clis, commands::find_clis, commands::agent_stats, commands::agent_runs, commands::agent_next_task,
            commands::usage_summary, commands::subscription_limits,
            commands::list_chat_threads, commands::get_chat_thread, commands::search_chat_threads, commands::chat_messages, commands::send_chat, commands::stop_chat, commands::chat_live, commands::chat_agent,
            commands::dismiss_chat, commands::chat_queue, commands::edit_queued_chat, commands::remove_queued_chat, commands::send_chat_queue,
            commands::check_files, commands::item_id,
            commands::set_chat_cli, commands::answer_chat_on, commands::chat_clis,
            commands::open_pull_request, commands::check_pull_request, commands::detect_gh,
            commands::github_status, commands::github_check, commands::github_login, commands::github_login_wait, commands::github_login_cancel,
            commands::bitbucket_status, commands::bitbucket_save_login, commands::bitbucket_remove_login, commands::bitbucket_check,
            commands::list_old_worktrees, commands::remove_old_worktrees,
            commands::update_status, commands::check_for_updates, commands::set_update_auto_check, commands::start_update,
            commands::stop_update, commands::restart_gizai,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Gizai")
        .run(|app, event| {
            // Quitting with agents at work (runs or a chat answer): stop them first, then quit (a second request
            // quits at once).
            if let tauri::RunEvent::ExitRequested { api, .. } = &event
                && quit::stop_agents_first(app)
            {
                api.prevent_exit();
            }
            // Closing the window (Super+W on Omarchy, its X button) only hides it: Gizai keeps running in the tray, with
            // its agents, heartbeats, chat and MCP socket, and the hidden window keeps its page. Quitting is the tray's or
            // Settings' Quit Gizai completely, Cmd+Q on macOS or a signal; Exit ends the page process (see `quit`).
            if let tauri::RunEvent::WindowEvent { label, event: tauri::WindowEvent::CloseRequested { api, .. }, .. } = &event
                && label == "main"
            {
                api.prevent_close();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            // macOS: clicking the Dock icon shows the window again.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                show_main(app);
            }
            if let tauri::RunEvent::Exit = event {
                quit::end_web_content(app);
                // agents still at work end now, so none outlives Gizai
                quit::end_agents(app.state::<AppState>().inner());
                mcp::remove_socket(app.state::<AppState>().inner());
                // an update that is building stops (its source and build so far are kept for the next one)
                update::on_exit(app.state::<AppState>().inner());
            }
        });
}
