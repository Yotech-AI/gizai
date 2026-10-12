//! Terminals (GA-133; contract CT with the terminal's screens, TF0 to TF2): your shell in one of the Editor's folders.
//! The sessions are `gizai_agents::terminal`'s, in memory in `AppState.terminals`: no table, no file, no log.
//! - A terminal starts only where the Editor may open a folder (`editor::find`, with its sentence when it may not), and
//!   not in a card's worktree an agent works on now. One opened before the agent started stays open.
//! - Your shell (`os::user_shell`) gets Gizai's environment with the PATH runs get (`runs::command_path`).
//! - Every change goes to the window as `terminals-changed`, with the whole list.
//! - Gizai quitting ends every terminal (`end_all`, from `quit`); closing the window keeps them running.
//! - These are the window's commands only: no MCP tool and no agent or Team Lead tool reaches a terminal.
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use gizai_agents::terminal::{self, Changed, Output, Start, TerminalSession};
use tauri::State;

use crate::AppState;
use crate::runs::Note;

type R<T> = Result<T, String>;

/// Starts your shell in the Editor folder `folder` (an `EditorFolder::id`) at `cols` × `rows`. `name` None or empty:
/// the folder's name, numbered when another terminal has it.
pub fn open(st: &AppState, folder: &str, cols: u16, rows: u16, name: Option<&str>) -> R<TerminalSession> {
    open_with(st, folder, cols, rows, name, None)
}

/// `open` with the program to start and its arguments; None: your shell. Tests start /bin/sh or cmd.exe.
pub fn open_with(st: &AppState, folder: &str, cols: u16, rows: u16, name: Option<&str>, program: Option<(PathBuf, Vec<OsString>)>)
    -> R<TerminalSession> {
    let f = crate::editor::folder(st, folder)?;
    if f.read_only {
        let why = f.read_only_why.as_deref().unwrap_or("An agent is working on it");
        return Err(format!("{why}: a terminal can start there when the run ends"));
    }
    let path = crate::runs::command_path();
    let (program, args) = program.unwrap_or_else(|| gizai_agents::os::user_shell(&path));
    let env = terminal::environment(std::env::vars_os(), &path, env!("CARGO_PKG_VERSION"));
    let start = Start { folder: f.id, folder_path: f.path, folder_name: f.name, task_id: f.task_id, program, args, env, cols, rows,
                        name: name.map(str::to_string) };
    st.terminals.open(start, changed(st))
}

/// Sends the terminal's output to `output`: what it kept first, then what comes.
pub fn attach(st: &AppState, id: &str, output: Output) -> R<()> {
    st.terminals.attach(id, output)
}

/// Gizai quits: every terminal ends (SIGHUP, the pseudo-terminal closed, SIGKILL after 3 s; on Windows the Job Object).
pub fn end_all(st: &AppState) {
    st.terminals.close_all();
}

/// Tells the window the list, through `notify` as it is now.
fn changed(st: &AppState) -> Changed {
    let notify = st.notify.clone();
    Arc::new(move |list| notify(Note::TerminalsChanged(list)))
}

// ---- the commands, word for word as contract CT names them ----

#[tauri::command]
pub fn terminal_list(st: State<'_, AppState>) -> Vec<TerminalSession> {
    st.terminals.list()
}

#[tauri::command]
pub async fn terminal_open(st: State<'_, AppState>, folder: String, cols: u16, rows: u16, name: Option<String>) -> Result<TerminalSession, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || open(&st, &folder, cols, rows, name.as_deref())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn terminal_attach(st: State<'_, AppState>, id: String, output: tauri::ipc::Channel<tauri::ipc::Response>) -> Result<(), String> {
    attach(&st, &id, Box::new(move |bytes| output.send(tauri::ipc::Response::new(bytes)).is_ok()))
}

#[tauri::command]
pub fn terminal_input(st: State<'_, AppState>, id: String, data: String, binary: bool) -> Result<(), String> {
    st.terminals.input(&id, &data, binary)
}

#[tauri::command]
pub fn terminal_resize(st: State<'_, AppState>, id: String, cols: u16, rows: u16) -> Result<(), String> {
    st.terminals.resize(&id, cols, rows)
}

#[tauri::command]
pub fn terminal_ack(st: State<'_, AppState>, id: String, bytes: u32) -> Result<(), String> {
    st.terminals.ack(&id, bytes)
}

#[tauri::command]
pub fn terminal_rename(st: State<'_, AppState>, id: String, name: String) -> Result<TerminalSession, String> {
    st.terminals.rename(&id, &name)
}

#[tauri::command]
pub async fn terminal_close(st: State<'_, AppState>, id: String) -> Result<(), String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || st.terminals.close(&id)).await.map_err(|e| e.to_string())?
}
