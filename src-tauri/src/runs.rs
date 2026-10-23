//! Runs agents for tasks on their coding CLI (Claude Code, Codex, Gemini, others): start (worktree, prompt, process), follow the stream, finish with
//! the gates, stop, and the queue that starts the cards of Auto columns. Usable without a Tauri app (tests): everything the
//! UI needs to hear goes through `AppState::notify`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gizai_agents::cli::{self as agent_cli, Kind, RunFolder, TaskRun};
use gizai_agents::prepare::{self as prep, Prepare};
use gizai_agents::process::{self, Caps, StopHandle, StreamEvent};
use gizai_agents::prompt::{self, BaseInfo, RunLimits, TaskContext};
use gizai_agents::stream::RunEvent;
use gizai_agents::{outcome, worktree};
use gizai_core::model::{Outcome, Project, Refusal, Task, TaskPatch};
use gizai_core::{comments, housekeeping, ids, projects, runs as core_runs, settings, tasks, team, workflow};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Per-run limits Gizai enforces itself (Claude Code has no turn cap in print mode); Settings → Runs changes them.
pub const DEFAULT_MAX_RUN_MINUTES: u64 = 90;
pub const DEFAULT_MAX_RUN_TOOL_CALLS: u32 = 200;
/// The same per answer of the Team Lead (a chat answer, a board check, its look at an agent's question), which is short
/// work; Settings → Runs changes them too (Minutes and Tool calls per chat answer).
pub const DEFAULT_MAX_CHAT_MINUTES: u64 = 15;
pub const DEFAULT_MAX_CHAT_TOOL_CALLS: u32 = 60;
pub const DEFAULT_MAX_CONCURRENT: u32 = 3;
const BUFFER: usize = 500;
/// Why a run or chat answer ended when Gizai quit (logging out and SIGTERM included), and the note a chat shows.
pub const STOPPED_BY_QUIT: &str = "Stopped because Gizai quit.";

/// Used when an agent has no allowed commands of its own. New agents start with their role's list, which builds on it
/// (`gizai_core::seed::role_tools`).
pub use gizai_core::seed::DEFAULT_TOOLS;

/// What the UI hears about.
pub enum Note {
    RowsChanged(&'static str),
    RunsChanged,
    /// A run's events since its last message, in `seq` order: at most one message every 50 ms per run (`stream`).
    RunEvents { run_id: String, events: Vec<SeqEvent> },
    /// What happened in a chat turn since its last message (text written, a tool called, a message saved): at most one
    /// message every 50 ms per chat, pieces of text one after the other joined (`stream`).
    ChatEvents { thread_id: String, events: Vec<crate::chat::ChatUiEvent> },
    /// A chat turn started or ended.
    ChatChanged,
    /// The release check or an update moved on (see `update`).
    UpdateChanged,
    /// The Editor's Update of a folder ended (see `editor`): the `editor-updated` event.
    EditorUpdated(gizai_agents::behind::EditorUpdated),
    /// A terminal opened, was renamed or closed, its shell ended or its busy program changed (see `terminal`): the
    /// `terminals-changed` event, with every open terminal.
    TerminalsChanged(Vec<gizai_agents::terminal::TerminalSession>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeqEvent {
    pub seq: u64,
    pub event: RunEvent,
}

struct Live {
    task_id: String,
    agent_id: String,
    stop: StopHandle,
    events: Vec<SeqEvent>,
    next_seq: u64,
}

#[derive(Default)]
pub struct RunManager {
    live: Mutex<HashMap<String, Live>>,
    /// Each Claude Code CLI's model list (by CLI id), asked once and kept for a while.
    models: Mutex<HashMap<String, (Instant, Vec<gizai_agents::models::ModelOption>)>>,
    stopped: Mutex<HashSet<String>>,
    /// Set while Gizai quits: no new runs start.
    closing: AtomicBool,
    /// Cards whose run is starting (claimed, their worktree being made and prepared): one start at a time per card. With
    /// the agent whose run it will be ("" until the start has chosen it), counted with the live runs against the run
    /// limits (`under_way`); None for a person's start by hand (GA-111), which isn't a run.
    starting: std::sync::Arc<Mutex<HashMap<String, Option<String>>>>,
    /// Agents that stopped taking cards from the queue (`pull`) after a start that couldn't work or a run
    /// that failed, with why (the first failure's). It is asked before every start, and only the failure that paused the
    /// agent holds its card: a card started just before goes back to waiting when it fails the same way, so one missing
    /// login holds one card, not the whole queue. The pause ends when a person starts the agent (Run or Continue), edits
    /// it or sets it active on the Team page, or when Gizai starts again.
    pull_paused: Mutex<HashMap<String, String>>,
    /// One pull of the queue at a time: two runs ending together don't give an agent more cards than it may work on.
    pulling: tokio::sync::Mutex<()>,
    /// Since when the start check has been waiting for memory, and why (GA-202, `run_memory::wait_reason`).
    pub(crate) memory_wait: Mutex<Option<(i64, String)>>,
    /// The runs' previews (GA-182): each run's block of ports, and its preview's tree while it runs.
    pub(crate) previews: crate::screenshots::Previews,
    /// GA-221: how long a run's or a chat answer's CLI may stay quiet once the agent is done before Gizai ends it
    /// (`Caps::after_result`). None: `process::AFTER_RESULT`; a test shortens it (`set_after_result`).
    after_result: Mutex<Option<Duration>>,
}

impl RunManager {
    /// The CLIs changed: ask for model lists again.
    pub fn forget_models(&self) {
        self.models.lock().unwrap().clear();
    }

    /// GA-221: `Caps::after_result` for the runs and chat answers that start from now on.
    pub fn after_result(&self) -> Duration {
        self.after_result.lock().unwrap().unwrap_or(process::AFTER_RESULT)
    }

    /// Tests: another wait than `process::AFTER_RESULT` for the runs and chat answers that start from now on.
    pub fn set_after_result(&self, wait: Duration) {
        *self.after_result.lock().unwrap() = Some(wait);
    }
}

/// Why the agent's pull is paused, if it is (see `RunManager::pull_paused`).
pub fn pull_paused(st: &AppState, agent_id: &str) -> Option<String> {
    st.runs.pull_paused.lock().unwrap().get(agent_id).cloned()
}

/// Every agent whose pull is paused, with why (the board check).
pub fn pull_paused_all(st: &AppState) -> HashMap<String, String> {
    st.runs.pull_paused.lock().unwrap().clone()
}

/// A person started, edited or reactivated the agent: it takes cards from the queue again.
pub fn resume_pull(st: &AppState, agent_id: &str) {
    st.runs.pull_paused.lock().unwrap().remove(agent_id);
}

/// Pauses the agent's pull. True when this paused it; false when an earlier failure already had (its reason stays).
fn pause_pull(st: &AppState, agent_id: &str, why: &str) -> bool {
    let mut paused = st.runs.pull_paused.lock().unwrap();
    if paused.contains_key(agent_id) {
        return false;
    }
    eprintln!("gizai: agent {agent_id} stops taking cards until you start or edit it: {why}");
    paused.insert(agent_id.to_string(), why.to_string());
    true
}

/// GA-212: why a run on the card waits for its project's Test runs at once (project settings), if it does: the card is in
/// a Testing column, and as many of the project's other cards in a Testing column have a run live or still starting.
fn test_runs_full(st: &AppState, task_id: &str) -> Option<String> {
    // The cards still starting first, then the live ones: a run is live before its card stops starting, so a start that
    // goes live meanwhile is in one of the two.
    let mut under_way = starting(st);
    under_way.extend(live(st).into_iter().map(|l| l.task_id));
    match workflow::test_runs_full(&st.db, task_id, &under_way) {
        Ok(full) => full.map(|f| f.message()),
        Err(e) => {
            eprintln!("gizai: checking Test runs at once for card {task_id} failed: {e}");
            None
        }
    }
}

/// Whether the queue may start another card for the agent now: Gizai isn't quitting, agents aren't
/// paused in Settings and the agent's pull isn't paused. Asked again before each start.
fn takes_cards(st: &AppState, agent_id: &str) -> bool {
    !is_closing(st) && !settings::get::<bool>(&st.db, "agents_paused").ok().flatten().unwrap_or(false) && pull_paused(st, agent_id).is_none()
}

/// Held while a card's run starts: the card is claimed (`RunManager::starting`).
pub(crate) struct Starting {
    cards: std::sync::Arc<Mutex<HashMap<String, Option<String>>>>,
    task_id: String,
}

impl Starting {
    /// Claims the card when nothing starts on it yet: for a run of `agent` (Some("") while the start still chooses it), or
    /// None for a person's start by hand.
    fn take(st: &AppState, task_id: &str, agent: Option<&str>) -> Option<Starting> {
        let cards = st.runs.starting.clone();
        let taken = {
            let mut claimed = cards.lock().unwrap();
            !claimed.contains_key(task_id) && claimed.insert(task_id.to_string(), agent.map(str::to_string)).is_none()
        };
        taken.then(|| Starting { cards, task_id: task_id.to_string() })
    }

    /// The agent whose run it is, once the start has chosen it.
    fn for_agent(&self, agent: &str) {
        if let Some(a) = self.cards.lock().unwrap().get_mut(&self.task_id) {
            *a = Some(agent.to_string());
        }
    }
}

impl Drop for Starting {
    fn drop(&mut self) {
        self.cards.lock().unwrap().remove(&self.task_id);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRun {
    pub run_id: String,
    pub task_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub outcome: Option<String>,
    pub cost_usd_micros: i64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub claude_bin: Option<String>,
    /// Read-only.
    #[serde(default)]
    pub data_dir: String,
    pub max_concurrent_runs: u32,
    /// Memory for agent runs (GA-202), in GB: all agent runs together. Half of this computer's memory until someone sets
    /// it (`run_memory::default_gb`); 2 GB up to the computer's memory. None (left out) keeps what was saved.
    #[serde(default)]
    pub runs_memory_gb: Option<u32>,
    /// Read-only: this computer's memory in GB (0 when the system doesn't say), for the hint under it.
    #[serde(default)]
    pub computer_memory_gb: u32,
    /// Read-only: whether the system enforces Memory for agent runs (Linux with a systemd user session). Elsewhere Gizai
    /// only waits for free memory before it starts a run.
    #[serde(default)]
    pub runs_memory_enforced: bool,
    #[serde(default)]
    pub agents_paused: bool,
    /// Claude Code stops a run once it has spent this much; None = no limit.
    #[serde(default)]
    pub max_run_usd: Option<f64>,
    /// Gizai stops a run after this long…
    #[serde(default = "default_minutes")]
    pub max_run_minutes: u64,
    /// …or after this many tool calls.
    #[serde(default = "default_tool_calls")]
    pub max_run_tool_calls: u32,
    /// Gizai stops an answer of the Team Lead (a chat answer, a board check, its look at an agent's question) after this
    /// long…
    #[serde(default = "default_chat_minutes")]
    pub max_chat_minutes: u64,
    /// …or after this many tool calls.
    #[serde(default = "default_chat_tool_calls")]
    pub max_chat_tool_calls: u32,
    /// The GitHub CLI (gh) that opens and follows pull requests with your GitHub login; None = found when needed.
    #[serde(default)]
    pub gh_bin: Option<String>,
    /// How Open pull request and Push branch reach GitHub: "ssh" (your SSH keys, the default) or "https" (gh's login).
    #[serde(default = "default_push_over")]
    pub push_over: String,
    /// Desktop notifications, a switch per kind (Settings → Notifications); all on by default.
    #[serde(default)]
    pub notifications: crate::notifications::Switches,
    /// Settings → General → Editor folders (GA-107): the Editor may open every folder in these. `~/Herd` until you set
    /// your own (`gizai_core::editor::roots`).
    #[serde(default = "default_editor_roots")]
    pub editor_roots: Vec<String>,
    /// Settings → Agents and runs → Screenshots (GA-182, contract CP): Screenshots per run, 12 (1–30). Each of these is
    /// always given by `get_settings`; None (left out) keeps what was saved.
    #[serde(default)]
    pub screenshots_per_run: Option<u32>,
    /// Largest screenshot in MB, 10 (1–50).
    #[serde(default)]
    pub screenshot_max_mb: Option<u32>,
    /// Keep per card: a card's newest sets, 3 (1–20).
    #[serde(default)]
    pub screenshots_keep_sets: Option<u32>,
    /// Delete a finished card's screenshots after this many days in Done or Cancelled, 30 (0–365; 0 keeps them).
    #[serde(default)]
    pub screenshots_keep_days: Option<u32>,
    /// Preview ports, 47100 to 47999 (1024–65535): at least ten for each run at once.
    #[serde(default)]
    pub preview_port_from: Option<u32>,
    #[serde(default)]
    pub preview_port_to: Option<u32>,
}

fn default_minutes() -> u64 { DEFAULT_MAX_RUN_MINUTES }
fn default_editor_roots() -> Vec<String> { gizai_core::editor::DEFAULT_ROOTS.map(String::from).to_vec() }
fn default_tool_calls() -> u32 { DEFAULT_MAX_RUN_TOOL_CALLS }
fn default_chat_minutes() -> u64 { DEFAULT_MAX_CHAT_MINUTES }
fn default_chat_tool_calls() -> u32 { DEFAULT_MAX_CHAT_TOOL_CALLS }
fn default_push_over() -> String { "ssh".into() }

pub fn get_settings(st: &AppState) -> Settings {
    let shots = gizai_core::screenshots::limits(&st.db);
    Settings {
        claude_bin: settings::get(&st.db, "claude_bin").ok().flatten(),
        data_dir: st.data_dir.display().to_string(),
        max_concurrent_runs: settings::get(&st.db, "max_concurrent_runs").ok().flatten().unwrap_or(DEFAULT_MAX_CONCURRENT),
        runs_memory_gb: Some(crate::run_memory::setting_gb(st)),
        computer_memory_gb: crate::run_memory::computer_gb().unwrap_or(0),
        runs_memory_enforced: st.run_memory.enforced(),
        agents_paused: settings::get(&st.db, "agents_paused").ok().flatten().unwrap_or(false),
        max_run_usd: settings::get(&st.db, "max_run_usd").ok().flatten(),
        max_run_minutes: settings::get(&st.db, "max_run_minutes").ok().flatten().unwrap_or(DEFAULT_MAX_RUN_MINUTES),
        max_run_tool_calls: settings::get(&st.db, "max_run_tool_calls").ok().flatten().unwrap_or(DEFAULT_MAX_RUN_TOOL_CALLS),
        max_chat_minutes: settings::get(&st.db, "max_chat_minutes").ok().flatten().unwrap_or(DEFAULT_MAX_CHAT_MINUTES),
        max_chat_tool_calls: settings::get(&st.db, "max_chat_tool_calls").ok().flatten().unwrap_or(DEFAULT_MAX_CHAT_TOOL_CALLS),
        gh_bin: settings::get(&st.db, "gh_bin").ok().flatten(),
        push_over: crate::github::push_over_name(st),
        notifications: crate::notifications::switches(&st.db),
        editor_roots: gizai_core::editor::roots(&st.db),
        screenshots_per_run: Some(shots.per_run),
        screenshot_max_mb: Some(shots.max_mb),
        screenshots_keep_sets: Some(shots.keep_sets),
        screenshots_keep_days: Some(shots.keep_days),
        preview_port_from: Some(shots.port_from),
        preview_port_to: Some(shots.port_to),
    }
}

pub fn save_settings(st: &AppState, s: &Settings) -> Result<(), String> {
    if !(1..=20).contains(&s.max_concurrent_runs) {
        return Err("allow between 1 and 20 runs at once".into());
    }
    if let Some(gb) = s.runs_memory_gb {
        crate::run_memory::check(gb)?;
    }
    if matches!(s.max_run_usd, Some(v) if v <= 0.0) {
        return Err("a spending limit must be more than $0".into());
    }
    if !(5..=480).contains(&s.max_run_minutes) {
        return Err("a run may last between 5 and 480 minutes".into());
    }
    if !(20..=2000).contains(&s.max_run_tool_calls) {
        return Err("a run may make between 20 and 2000 tool calls".into());
    }
    if !(5..=240).contains(&s.max_chat_minutes) {
        return Err("a chat answer may last between 5 and 240 minutes".into());
    }
    if !(20..=500).contains(&s.max_chat_tool_calls) {
        return Err("a chat answer may make between 20 and 500 tool calls".into());
    }
    if !crate::github::PUSH_OVER_NAMES.contains(&s.push_over.as_str()) {
        return Err("pushes go over ssh or https".into());
    }
    let editor_roots = gizai_core::editor::clean_roots(&s.editor_roots, &gizai_core::folders::Places::without_checkouts(&st.db)).map_err(|e| e.to_string())?;
    // Settings → Agents and runs → Screenshots (GA-182): a value left out keeps what was saved; one outside its range is
    // refused with why, and nothing changes.
    let saved = gizai_core::screenshots::limits(&st.db);
    let shots = gizai_core::screenshots::Limits {
        per_run: s.screenshots_per_run.unwrap_or(saved.per_run), max_mb: s.screenshot_max_mb.unwrap_or(saved.max_mb),
        keep_sets: s.screenshots_keep_sets.unwrap_or(saved.keep_sets), keep_days: s.screenshots_keep_days.unwrap_or(saved.keep_days),
        port_from: s.preview_port_from.unwrap_or(saved.port_from), port_to: s.preview_port_to.unwrap_or(saved.port_to),
    };
    gizai_core::screenshots::check_limits(&shots, s.max_concurrent_runs)?;
    let bin = s.claude_bin.as_ref().map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
    settings::set(&st.db, "claude_bin", &bin).map_err(|e| e.to_string())?;
    let gh = s.gh_bin.as_ref().map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
    settings::set(&st.db, "gh_bin", &gh).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_concurrent_runs", &s.max_concurrent_runs).map_err(|e| e.to_string())?;
    settings::set(&st.db, "agents_paused", &s.agents_paused).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_run_usd", &s.max_run_usd).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_run_minutes", &s.max_run_minutes).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_run_tool_calls", &s.max_run_tool_calls).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_chat_minutes", &s.max_chat_minutes).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_chat_tool_calls", &s.max_chat_tool_calls).map_err(|e| e.to_string())?;
    crate::github::save_push_over(st, &s.push_over)?;
    crate::notifications::save_switches(&st.db, &s.notifications)?;
    settings::set(&st.db, gizai_core::editor::ROOTS, &editor_roots).map_err(|e| e.to_string())?;
    gizai_core::screenshots::save_limits(&st.db, &shots, s.max_concurrent_runs).map_err(|e| e.to_string())?;
    if let Some(gb) = s.runs_memory_gb {
        crate::run_memory::save(st, gb)?;
    }
    Ok(())
}

/// Whether `p` is a program this system runs (`gizai_agents::os::executable`): a file with an execute bit on Linux and
/// macOS, an `.exe`, `.cmd`… file on Windows.
pub(crate) fn executable(p: &Path) -> bool {
    gizai_agents::os::executable(p)
}

/// The program at the path `p` (a saved Claude Code or gh), when this system runs it. On Windows a path without its
/// extension is tried with PATHEXT's (`…\claude` → `…\claude.exe`).
pub(crate) fn program_at(p: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        return gizai_agents::os::find_in(p, &std::env::var_os("PATH").unwrap_or_default());
    }
    executable(Path::new(p)).then(|| PathBuf::from(p))
}

/// Finds `claude` the way a login shell would, then in the usual install places. Saves what it finds.
pub fn detect_claude(st: &AppState) -> Option<String> {
    let found = find_claude();
    if let Some(p) = &found {
        let _ = settings::set(&st.db, "claude_bin", p);
    }
    found
}

/// `detect_claude` without saving: where `claude` is installed, if it is. Linux: the way a login shell finds it, then
/// the usual install places. macOS: on PATH (Gizai took your login shell's at start, `shell_path`), then the usual
/// places. Windows: on PATH (`claude.exe`, or npm's `claude.cmd`), then where the native installer and npm put it.
pub fn find_claude() -> Option<String> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    if cfg!(windows) {
        let home = gizai_core::clis::home();
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).filter(|a| a.is_absolute());
        let usual = [(!home.is_empty()).then(|| Path::new(&home).join(".local").join("bin").join("claude.exe")),
                     appdata.map(|a| a.join("npm").join("claude.cmd"))];
        return gizai_agents::os::find_in("claude", &path)
            .or_else(|| usual.into_iter().flatten().find(|p| executable(p)))
            .map(|p| p.display().to_string());
    }
    if cfg!(target_os = "macos") {
        let home = gizai_core::clis::home();
        return gizai_agents::os::find_in("claude", &path).map(|p| p.display().to_string()).or_else(|| {
            [".local/bin/claude", ".claude/local/claude"].iter().map(|rel| format!("{home}/{rel}"))
                .chain(["/opt/homebrew/bin/claude".to_string(), "/usr/local/bin/claude".to_string()])
                .find(|p| executable(Path::new(p)))
        });
    }
    let from_shell = std::process::Command::new("bash").args(["-lc", "command -v claude"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|p| !p.is_empty() && executable(Path::new(p)));
    let home = std::env::var("HOME").unwrap_or_default();
    from_shell.or_else(|| {
        [".local/bin/claude", ".claude/local/claude", ".local/share/mise/installs/claude/latest/claude", ".npm-global/bin/claude"]
            .iter().map(|rel| format!("{home}/{rel}"))
            .chain(["/usr/local/bin/claude".to_string(), "/usr/bin/claude".to_string()])
            .find(|p| executable(Path::new(p)))
    })
}

/// The saved Claude Code path, `~` expanded as for the other CLIs; detected only when none was ever saved (a saved path
/// that has gone missing is reported, never silently replaced by another binary).
pub(crate) fn claude_bin(st: &AppState, bin_override: Option<String>) -> Result<PathBuf, String> {
    let bin = match bin_override {
        Some(b) => Some(b),
        None => match settings::get::<String>(&st.db, "claude_bin").ok().flatten() {
            Some(saved) => Some(saved),
            None => detect_claude(st),
        },
    };
    bin.map(|b| gizai_core::clis::expand_home(b.trim(), &gizai_core::clis::home())).as_deref().and_then(program_at)
        .ok_or_else(|| "Claude Code not found: set its path in Settings".into())
}

/// How long Claude Code's model list is kept before it is asked again.
const MODELS_TTL: Duration = Duration::from_secs(30 * 60);

/// The models this user's Claude Code offers (its /model list), kept for half an hour; `refresh` asks again.
pub async fn models(st: &AppState, refresh: bool) -> Result<Vec<gizai_agents::models::ModelOption>, String> {
    models_for(st, None, refresh).await
}

/// The same for one Claude Code CLI (another account can offer other models); None = the built-in Claude Code. Other
/// kinds of CLI have no list Gizai can ask for: empty.
pub async fn models_for(st: &AppState, cli_id: Option<&str>, refresh: bool) -> Result<Vec<gizai_agents::models::ModelOption>, String> {
    let cli = crate::clis::of_agent(st, cli_id)?;
    if cli.kind != "claude_code" {
        return Ok(vec![]);
    }
    if !refresh {
        if let Some((at, list)) = st.runs.models.lock().unwrap().get(&cli.id) {
            if at.elapsed() < MODELS_TTL {
                return Ok(list.clone());
            }
        }
    }
    let spec = crate::clis::spec(st, &cli, None)?;
    let cwd = st.data_dir.clone();
    let list = gizai_agents::models::fetch_models_with_env(&spec.bin, &cwd, &spec.env).await.map_err(|e| e.to_string())?;
    st.runs.models.lock().unwrap().insert(cli.id.clone(), (Instant::now(), list.clone()));
    Ok(list)
}

/// An agent works on the card now: a live run, or a run still getting its worktree ready. (One lock at a time.)
pub fn working_on(st: &AppState, task_id: &str) -> bool {
    let starting = st.runs.starting.lock().unwrap().contains_key(task_id);
    starting || st.runs.live.lock().unwrap().values().any(|l| l.task_id == task_id)
}

/// The cards whose run is getting its worktree ready now (with `live`, the cards of `working_on`).
pub fn starting(st: &AppState) -> Vec<String> {
    st.runs.starting.lock().unwrap().keys().cloned().collect()
}

/// The cards with a run under way (GA-191): live, or starting (claimed for a run, its worktree being made ready), of
/// `agent` or of every agent, each card once, without `except`. What the run limits count: since cards start side by
/// side, a claimed card holds its place before its run is live. A person's start by hand isn't a run.
fn under_way(st: &AppState, agent: Option<&str>, except: Option<&str>) -> usize {
    let mut cards: HashSet<String> = st.runs.live.lock().unwrap().values()
        .filter(|l| agent.is_none_or(|a| l.agent_id == a)).map(|l| l.task_id.clone()).collect();
    let starting: Vec<String> = st.runs.starting.lock().unwrap().iter()
        .filter(|(_, by)| by.as_deref().is_some_and(|by| agent.is_none_or(|a| a == by))).map(|(t, _)| t.clone()).collect();
    cards.extend(starting);
    if let Some(e) = except {
        cards.remove(e);
    }
    cards.len()
}

pub fn live(st: &AppState) -> Vec<LiveRun> {
    st.runs.live.lock().unwrap().iter()
        .map(|(id, l)| LiveRun { run_id: id.clone(), task_id: l.task_id.clone(), agent_id: l.agent_id.clone() })
        .collect()
}

/// The run's events: from memory while it is live (its last `BUFFER`), else re-read from its log. When housekeeping
/// removed the log (the run ended more than 30 days ago), a note says so.
pub fn events_for(st: &AppState, run_id: &str) -> Vec<SeqEvent> {
    let live = st.runs.live.lock().unwrap().get(run_id).map(|l| l.events.clone());
    live.unwrap_or_else(|| logged_events(st, run_id))
}

/// The window's `run_events` (GA-191): the run's events numbered after `after_seq` (all of them for None), at most the
/// last `BUFFER`, so a page that has a run's events up to a number asks only for the rest.
pub fn events_after(st: &AppState, run_id: &str, after_seq: Option<u64>) -> Vec<SeqEvent> {
    let mut events = events_for(st, run_id);
    if let Some(a) = after_seq {
        events.retain(|e| e.seq > a);
    }
    let cut = events.len().saturating_sub(BUFFER);
    events.split_off(cut)
}

/// A run's events as its log has them (a run that isn't live), numbered from 0.
fn logged_events(st: &AppState, run_id: &str) -> Vec<SeqEvent> {
    let Ok(run) = core_runs::get(&st.db, run_id) else { return vec![] };
    let events = match std::fs::read_to_string(&run.log_path) {
        // The limits Claude Code reported are for the Usage page, not the Run panel.
        Ok(text) => agent_cli::parse_log(&text).into_iter().filter(|e| !matches!(e, RunEvent::Limits { .. })).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound
            && run.ended_at.unwrap_or(run.created_at) < ids::now_ms() - housekeeping::KEEP_LOGS_MS => {
            vec![RunEvent::Note { text: format!("Gizai keeps run logs for {} days, so this run's output is gone. Its summary, cost and commits stay.",
                                                housekeeping::KEEP_LOG_DAYS) }]
        }
        Err(_) => vec![],
    };
    events.into_iter().enumerate().map(|(i, event)| SeqEvent { seq: i as u64, event }).collect()
}

pub fn stop(st: &AppState, run_id: &str) {
    st.runs.stopped.lock().unwrap().insert(run_id.to_string());
    if let Some(l) = st.runs.live.lock().unwrap().get(run_id) {
        l.stop.stop();
    }
}

/// Which agent Run starts when none is picked: the card's agent assignee, else the first agent on its column
/// (`workflow::run_agent`). Without either, the message says to put an agent on the column.
fn choose_agent(st: &AppState, task_id: &str) -> Result<(String, String), String> {
    let t = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    if t.hold.is_some() {
        return Err(format!("{} is on hold: clear the hold first", t.identifier));
    }
    workflow::run_agent(&st.db, task_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No agent works {}: drag an agent onto {} on the Team page, or pick an agent", t.identifier, t.state_name))
}

/// Quitting: no new runs, Stop every live one, and wait until they have ended (at most `wait`). Returns
/// how many were live. Stop escalates to SIGKILL after 10 s, so 12 s is enough.
pub async fn stop_all(st: &AppState, wait: Duration) -> usize {
    st.runs.closing.store(true, Ordering::SeqCst);
    let ids: Vec<String> = st.runs.live.lock().unwrap().keys().cloned().collect();
    for id in &ids {
        stop(st, id);
    }
    let t0 = Instant::now();
    while !st.runs.live.lock().unwrap().is_empty() && t0.elapsed() < wait {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    ids.len()
}

/// Gizai exits with runs still live (asked to quit again while they were being stopped, or they didn't end in time):
/// ends their process groups at once (SIGKILL). Each is recorded as stopped because Gizai quit as soon as it has
/// ended. Returns how many were live.
pub fn kill_all(st: &AppState) -> usize {
    mark_closing(st);
    let live: Vec<StopHandle> = st.runs.live.lock().unwrap().values().map(|l| l.stop.clone()).collect();
    for s in &live {
        s.kill();
    }
    // their previews too (GA-182): a preview never outlives its run
    crate::screenshots::end_previews(&st.runs.previews);
    live.len()
}

/// Gizai is quitting: no new runs or chat answers start, and those that end from now on were stopped because Gizai
/// quit.
pub fn mark_closing(st: &AppState) {
    st.runs.closing.store(true, Ordering::SeqCst);
}

/// Gizai isn't quitting after all (Stay, when quitting asked about unsaved text: see `unsaved`): runs and chat answers
/// start again.
pub fn unmark_closing(st: &AppState) {
    st.runs.closing.store(false, Ordering::SeqCst);
}

pub fn is_closing(st: &AppState) -> bool {
    st.runs.closing.load(Ordering::SeqCst)
}

/// The agent a Run without a chosen agent would start (the assigned agent, else the first agent on the card's column).
pub fn suggest(st: &AppState, task_id: &str) -> Option<String> {
    choose_agent(st, task_id).ok().map(|(a, _)| a)
}

/// Why a run didn't start: a start that can't work (Claude Code missing, a wrong model, no repository, …), a card already
/// put on hold for it (preparing its worktree failed), a start that only has to wait (the run limit, a paused agent, the
/// agent's budget, …), or anything else.
enum StartError {
    Card(String),
    Held(String),
    Wait(String),
    Other(String),
}

impl StartError {
    fn message(self) -> String {
        match self { StartError::Card(m) | StartError::Held(m) | StartError::Wait(m) | StartError::Other(m) => m }
    }
}

impl From<String> for StartError {
    fn from(s: String) -> Self { StartError::Other(s) }
}

impl From<&str> for StartError {
    fn from(s: &str) -> Self { StartError::Other(s.to_string()) }
}

/// Starts a run and returns its id plus a handle that resolves when it has finished and its verdict is applied.
pub async fn start(st: &AppState, task_id: &str, agent_id: Option<String>, bin_override: Option<String>, trigger: &str)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    let (run_id, done) = start_inner(st, task_id, agent_id, bin_override, trigger, None, None, &mut None).await.map_err(StartError::message)?;
    if trigger == "manual" && let Ok(r) = core_runs::get(&st.db, &run_id) {
        resume_pull(st, &r.agent_id);
    }
    Ok((run_id, done))
}

/// A session to resume instead of starting one: Continue.
struct Resume {
    session: String,
    /// The CLI that ran the session: only it (that program, that account) can resume it.
    cli: gizai_core::clis::Cli,
    /// Why the run being continued stopped, told to the agent.
    reason: String,
    /// It ended asking for a decision: what was written on the card since, told to the agent instead.
    answer: Option<String>,
    /// Gizai's own nudge (`nudge_for`): the run ended without a result, and the agent hears `prompt::nudge_prompt`.
    nudge: bool,
    /// Continue with a message (GA-31): the note of whoever continued it, told to the agent next to `reason` (or the
    /// answer) and saved on the card as their comment once the run is recorded.
    note: Option<ContinueNote>,
}

/// A note for a continued run (GA-31): from you (the text box next to Continue, or Done, continue) or from the Team Lead
/// (`continue_agent_run`'s `note`).
struct ContinueNote {
    /// Who wrote it: the comment's author.
    by: String,
    /// Their name, as the prompt says it.
    by_name: String,
    text: String,
}

impl ContinueNote {
    /// The note of `by`, None when it is empty.
    fn of(st: &AppState, by: &str, text: Option<String>) -> Option<ContinueNote> {
        let text = text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())?;
        Some(ContinueNote { by: by.to_string(), by_name: actor_name(st, by), text })
    }
}

/// An actor's name (a person or an agent), for a prompt or a comment.
pub(crate) fn actor_name(st: &AppState, actor_id: &str) -> String {
    team::agent(&st.db, actor_id).map(|m| m.name).ok()
        .or_else(|| gizai_core::users::list(&st.db).ok()?.into_iter().find(|p| p.id == actor_id).map(|p| p.name))
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "The user".into())
}

/// Continue: resumes a stopped run's session in its worktree, as a new run of the same agent on the same CLI. Only the
/// card's latest run continues, and only one that stopped part-way. A hold on the card is cleared: you asked for
/// the work (clearing also resets its failure count).
pub async fn continue_run(st: &AppState, run_id: &str, bin_override: Option<String>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    continue_inner(st, run_id, bin_override, false, None).await
}

/// Continue with a message (GA-31): `continue_run` with your note for the agent (the text box next to Continue), like
/// "use the existing CSV writer". The note goes into the continued run's prompt, next to why the run stopped, and is saved
/// on the card as your comment. None or an empty note: a plain Continue.
pub async fn continue_with_note(st: &AppState, run_id: &str, note: Option<String>, bin_override: Option<String>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    let note = ContinueNote::of(st, &st.you_id, note);
    continue_inner(st, run_id, bin_override, false, note).await
}

/// The Team Lead's Continue after an answer (`continue_agent_run`): also a run that ended asking for a decision
/// (`needs_decision`) resumes, told what was written on the card since it ended.
pub async fn continue_answered(st: &AppState, run_id: &str) -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    continue_inner(st, run_id, None, true, None).await
}

/// `continue_answered` with the Team Lead's note (`continue_agent_run`'s `note`), from `lead`: saved on the card as its
/// comment and told to the agent like a person's note. On a run that asked for a decision, the note counts as an answer
/// written on the card.
pub async fn continue_answered_with_note(st: &AppState, run_id: &str, lead: &str, note: Option<String>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    let note = ContinueNote::of(st, lead, note);
    continue_inner(st, run_id, None, true, note).await
}

/// "Run this for me", Done, continue (GA-31): you ran the commands the card's latest run asked you to run (`run_for_me`
/// on its `needs_decision` result). That run continues, like the Team Lead's Continue after an answer, with your note that
/// the commands were run (saved on the card as your comment), so the agent checks that they worked and carries on. A
/// hold on the card is cleared.
pub async fn continue_after_run_for_me(st: &AppState, task_id: &str) -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    let last = core_runs::list_for_task(&st.db, task_id).map_err(|e| e.to_string())?.into_iter().next()
        .ok_or("this card has no run to continue")?;
    if last.outcome.as_deref() != Some("needs_decision") || last.run_for_me.is_empty() {
        return Err("its last run didn't ask you to run anything: Continue or Run starts the agent".into());
    }
    let note = ContinueNote::of(st, &st.you_id, Some(ran_for_me_note(&last.run_for_me)));
    continue_inner(st, &last.id, None, true, note).await
}

/// Done, continue's note: the commands, and that they were run.
pub fn ran_for_me_note(commands: &[String]) -> String {
    let (what, worked) = if commands.len() == 1 { ("the command", "it worked") } else { ("the commands", "they worked") };
    format!("Done: I ran {what} you asked me to run.\n\n```sh\n{}\n```\n\nCheck that {worked}, then carry on.", commands.join("\n"))
}

/// The Team Lead answered the question the run `run_id` ended with (GA-70, `ask_lead`): that run continues with the answer
/// as the Team Lead's note, like `continue_answered_with_note`. The error says whether the start only has to wait (true:
/// "Runs at once" is full, the agent is paused or over its budget, the card's run is starting) or can't work (false).
pub(crate) async fn continue_for_question(st: &AppState, run_id: &str, lead: &str, answer: &str)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), (bool, String)> {
    let note = ContinueNote::of(st, lead, Some(answer.to_string()));
    continue_with(st, run_id, None, true, note).await.map_err(|e| match e {
        StartError::Wait(m) => (true, m),
        other => (false, other.message()),
    })
}

async fn continue_inner(st: &AppState, run_id: &str, bin_override: Option<String>, answered: bool, note: Option<ContinueNote>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    continue_with(st, run_id, bin_override, answered, note).await.map_err(StartError::message)
}

async fn continue_with(st: &AppState, run_id: &str, bin_override: Option<String>, answered: bool, note: Option<ContinueNote>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), StartError> {
    let run = core_runs::get(&st.db, run_id).map_err(|e| e.to_string())?;
    let task_id = run.task_id.clone().ok_or("only a card's run can continue")?;
    let latest = core_runs::list_for_task(&st.db, &task_id).map_err(|e| e.to_string())?.into_iter().next();
    if latest.as_ref().map(|r| r.id.as_str()) != Some(run_id) {
        return Err("only the card's latest run can continue".into());
    }
    let asked = answered && run.status == "succeeded" && run.outcome.as_deref() == Some("needs_decision");
    let stopped = matches!(run.status.as_str(), "timed_out" | "failed" | "cancelled")
        || (run.status == "succeeded" && run.outcome.as_deref() == Some("no_result")) || asked;
    if !stopped {
        return Err("this run finished; Run starts a new one".into());
    }
    let answer = if asked {
        let since = run.ended_at.unwrap_or(run.created_at);
        // The Team Lead's comment that asked you (GA-70) is told to the agent with the rest, but it is no answer.
        let asking = run.lead.as_ref().and_then(|l| l.comment_id.clone());
        let said: Vec<(bool, String)> = comments::list(&st.db, &task_id).unwrap_or_default().into_iter()
            .filter(|c| c.created_at > since && c.run_id.as_deref() != Some(run_id))
            .map(|c| (asking.as_deref() != Some(c.id.as_str()), format!("{}: {}", c.author_name, c.body_md.trim())))
            .collect();
        // A note is an answer too: it is saved on the card with the run (`start_inner`).
        if !said.iter().any(|(answer, _)| *answer) && note.is_none() {
            return Err("nobody has answered on the card since this run asked for a decision".into());
        }
        let lines: Vec<String> = said.into_iter().map(|(_, l)| l).collect();
        Some(lines[lines.len().saturating_sub(5)..].join("\n\n"))
    } else {
        None
    };
    let cli = crate::clis::of_agent(st, run.adapter.as_deref())?;
    if !Kind::parse(&cli.kind).is_some_and(Kind::can_resume) {
        return Err(format!("{} can't continue a run: Run starts the card fresh", cli.name).into());
    }
    let session = run.session_id.clone().filter(|s| !s.is_empty()).ok_or_else(|| format!("this run has no {} session to continue", cli.name))?;
    if !run.worktree_path.as_deref().is_some_and(|p| Path::new(p).is_dir()) {
        return Err("its worktree is gone; Run starts the card fresh".into());
    }
    let reason = run.error.clone().filter(|e| !e.trim().is_empty()).unwrap_or_else(|| "it ended without a result".into());
    let started = start_inner(st, &task_id, Some(run.agent_id.clone()), bin_override, "manual",
                              Some(Resume { session, cli, reason, answer, nudge: false, note }), None, &mut None).await?;
    resume_pull(st, &run.agent_id);
    if tasks::get(&st.db, &task_id).is_ok_and(|t| t.hold.is_some()) {
        let patch = gizai_core::model::TaskPatch { hold: Some(String::new()), ..Default::default() };
        if tasks::update(&st.db, &st.you_id, &task_id, patch).is_ok() {
            (st.notify)(Note::RowsChanged("tasks"));
        }
    }
    Ok(started)
}

/// A start nobody is watching (the queue, Gizai's nudge with the session to resume): a start that can't work puts the
/// card on hold "blocked" with the reason (it stays where it was, and its failure count doesn't change), so Jeffrey sees
/// why, and the agent stops taking cards until a person starts or edits it. When another card's failure paused the agent
/// while this card was starting, the card only waits again: one failure holds one card. A start that only has to wait
/// leaves the card waiting for the next pull.
///
/// `claim` and `checked` are the queue's (`pull`): the card it claimed, and what the agent's next card waits for. That
/// one is told once this start is past its quick checks, or else once this start has ended, a pause it caused included,
/// so the agent's best card is the one a start that can't work holds.
async fn start_background(st: &AppState, task_id: &str, agent_id: &str, trigger: &str, resume: Option<Resume>, claim: Option<Starting>,
                          checked: Option<tokio::sync::oneshot::Sender<()>>) -> Option<tokio::task::JoinHandle<RunSummary>> {
    let mut checked = checked;
    let started = start_inner(st, task_id, Some(agent_id.to_string()), None, trigger, resume, claim, &mut checked).await;
    let out = match started {
        Ok((_, done)) => Some(done),
        Err(StartError::Card(reason)) => {
            if pause_pull(st, agent_id, &reason) {
                hold_card(st, agent_id, task_id, &reason);
            } else {
                eprintln!("gizai: {trigger} start of an agent failed while its pull was paused, the card waits: {reason}");
            }
            None
        }
        Err(StartError::Held(reason)) => { pause_pull(st, agent_id, &reason); None }
        Err(StartError::Wait(_)) => None,
        Err(StartError::Other(reason)) => { eprintln!("gizai: {trigger} start of an agent failed: {reason}"); None }
    };
    drop(checked);
    out
}

/// Puts the card on hold "blocked" with the reason, as `actor` (the agent that couldn't start), so Jeffrey sees why.
fn hold_card(st: &AppState, actor: &str, task_id: &str, reason: &str) {
    let patch = TaskPatch { hold: Some("blocked".into()), hold_reason: Some(reason.to_string()), ..Default::default() };
    if tasks::update(&st.db, actor, task_id, patch).is_ok() {
        (st.notify)(Note::RowsChanged("tasks"));
    }
}

/// Where the project's main branch is fetched from, as the run prompt says it: "GitHub", "Bitbucket", or "the project's
/// repository" for another git URL.
fn fetched_from(project: &Project) -> &'static str {
    project.repo_url.as_deref()
        .and_then(|u| gizai_core::repo_url::normalize(u).ok().flatten())
        .and_then(|l| gizai_core::repo_url::provider_name(&l.provider))
        .unwrap_or("the project's repository")
}

/// The card's worktree: its own when it has one; else a finished card's worktree of the same project, taken over so
/// its build stays warm (`worktrees::take_over`, which empties a build folder over `worktrees::BUILD_LIMIT`); else a new
/// one. A new or taken-over worktree is noted as still to prepare. Off the async threads: a take-over measures the
/// finished card's build folder, and can delete a big one.
async fn open_worktree(st: &AppState, project: &Project, task: &Task, repo: &Path, start: &str) -> Result<worktree::Worktree, StartError> {
    let (st, project, task, repo, start) = (st.clone(), project.clone(), task.clone(), repo.to_path_buf(), start.to_string());
    tokio::task::spawn_blocking(move || open_worktree_now(&st, &project, &task, &repo, &start))
        .await.map_err(|e| StartError::Other(e.to_string()))?
}

fn open_worktree_now(st: &AppState, project: &Project, task: &Task, repo: &Path, start: &str) -> Result<worktree::Worktree, StartError> {
    let base_dir = st.data_dir.join("worktrees").join(&project.key);
    let has_own = base_dir.join(&task.identifier).exists()
        || worktree::worktree_of(repo, &worktree::branch_name(&task.identifier, &task.title)).ok().flatten().is_some();
    let taken = if has_own { None } else {
        crate::worktrees::take_over(st, project, task, repo, &base_dir, start, crate::worktrees::BUILD_LIMIT)
    };
    let wt = match taken {
        Some(w) => w,
        None => worktree::ensure(repo, &base_dir, &task.identifier, &task.title, start).map_err(|e| StartError::Card(e.to_string()))?,
    };
    if wt.created {
        worktree::mark_unprepared(&wt.path, wt.reused.as_ref().map(|r| r.head.as_str())).map_err(|e| StartError::Card(e.to_string()))?;
    }
    Ok(wt)
}

/// Prepares a new or taken-over worktree before the agent starts: the project's copies, the install of what is still
/// missing, then its setup command (`gizai_agents::prepare`). A command that fails puts the card on hold "blocked"
/// with its output, as `holder` (the agent that was to start); no run starts, so it doesn't count as a failed run, and
/// the next start prepares again. Without `holder` (a person's start by hand, GA-111) the card gets no hold: the error
/// says why.
async fn prepare_worktree(st: &AppState, holder: Option<&str>, task: &Task, project: &Project, repo: &Path, wt: &worktree::Worktree,
                          todo: worktree::Unprepared) -> Result<(), StartError> {
    let plan = Prepare { copy: project.worktree_copy.clone(), install: project.worktree_install, setup: project.worktree_setup.clone() };
    // The copies from the project's own folder hold its lock: not while it is being updated from chat
    // (`code::start_update`) or the Editor, and the other way round, so no card copies a half-installed node_modules/.
    // Copies go side by side (a read lock). The install and the setup command run in the worktree only, so they go
    // without it (GA-191): one card's long install doesn't hold up an update or another card's copy.
    let copied = {
        let folder = crate::code::folder_lock(st, repo);
        let _copying = folder.read().await;
        let (main, dir, plan, memory) = (repo.to_path_buf(), wt.path.clone(), plan.clone(), st.run_memory.clone());
        tokio::task::spawn_blocking(move || {
            let path = command_path();
            // GA-202: in the slice of the agent runs, under their memory limit (Linux)
            prep::copy_in(&main, &dir, &plan, Some(path.as_os_str()), memory.slice().as_ref())
        }).await.map_err(|e| StartError::Other(e.to_string()))?
    };
    let (dir, memory) = (wt.path.clone(), st.run_memory.clone());
    let done = tokio::task::spawn_blocking(move || {
        let path = command_path();
        prep::install_in(&dir, &plan, todo.since.as_deref(), Some(path.as_os_str()), memory.slice().as_ref(), copied)
    }).await.map_err(|e| StartError::Other(e.to_string()))?;
    match done {
        Ok(did) => {
            worktree::mark_prepared(&wt.path).map_err(|e| StartError::Card(e.to_string()))?;
            let what = did.summary();
            if !what.is_empty() {
                eprintln!("gizai: prepared {}'s worktree: {what}", task.identifier);
            }
            Ok(())
        }
        Err(failed) => {
            let reason = format!("Preparing its worktree failed: {failed}");
            match holder {
                Some(agent_id) => {
                    hold_card(st, agent_id, &task.id, &reason);
                    Err(StartError::Held(reason))
                }
                None => Err(StartError::Card(reason)),
            }
        }
    }
}

/// How long `command_path` keeps your login shell's PATH (Linux): a burst of calls (a chat page, a run's start, the
/// Editor's apps) starts one login shell.
pub const LOGIN_PATH_TTL: Duration = Duration::from_secs(60);

/// Your login shell's PATH and when it was read (Linux), kept for `LOGIN_PATH_TTL`.
static LOGIN_PATH: Mutex<Option<(Instant, String)>> = Mutex::new(None);

/// The login shells `login_path` started (Linux).
static LOGIN_SHELLS: AtomicUsize = AtomicUsize::new(0);

/// The PATH for the commands that prepare a worktree (and the coding CLIs Settings finds and runs start): on Linux
/// Gizai's own, then the folders your login shell adds (started from the app launcher, Gizai often lacks ~/.local/bin,
/// mise or nvm), as read in the last minute (`LOGIN_PATH_TTL`). On macOS and Windows Gizai's own: macOS took your login
/// shell's at start (`shell_path`), and a Windows app gets the full PATH.
pub fn command_path() -> std::ffi::OsString {
    path_with_login(false)
}

/// `command_path` with your login shell's PATH read now, for what looks for a program installed a moment ago (Settings →
/// Coding CLIs finding the CLIs and saving them); the calls after it get this PATH.
pub fn fresh_command_path() -> std::ffi::OsString {
    path_with_login(true)
}

fn path_with_login(fresh: bool) -> std::ffi::OsString {
    let own = std::env::var_os("PATH").unwrap_or_default();
    if cfg!(any(windows, target_os = "macos")) {
        return own;
    }
    crate::shell_path::with_login_path(own, &login_path(fresh))
}

/// Your login shell's PATH (Linux): the one read less than `LOGIN_PATH_TTL` ago, else (or when `fresh`) read now with
/// `bash -lc`. One read at a time: a call while another reads waits for it, and gets what it read.
fn login_path(fresh: bool) -> String {
    let mut kept = LOGIN_PATH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, path)) = kept.as_ref() && !fresh && at.elapsed() < LOGIN_PATH_TTL {
        return path.clone();
    }
    let at = Instant::now();
    LOGIN_SHELLS.fetch_add(1, Ordering::SeqCst);
    let path = std::process::Command::new("bash").args(["-lc", "printf '\\n%s' \"$PATH\""]).stdin(std::process::Stdio::null()).output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().last().map(str::to_string))
        .unwrap_or_default();
    *kept = Some((at, path.clone()));
    path
}

/// For tests: how many login shells `command_path` and `fresh_command_path` started in this process (Linux; none on
/// macOS and Windows).
#[doc(hidden)]
pub fn login_shells() -> usize {
    LOGIN_SHELLS.load(Ordering::SeqCst)
}

/// For tests: the kept login PATH as if it was read `by` earlier, so a test needn't wait a minute for it to go stale.
#[doc(hidden)]
pub fn age_login_path(by: Duration) {
    let mut kept = LOGIN_PATH.lock().unwrap_or_else(|e| e.into_inner());
    *kept = kept.take().and_then(|(at, path)| Some((at.checked_sub(by)?, path)));
}

/// `claim`: the card as the queue claimed it (`pull`); None: the start claims it. `checked`: told once the start is past
/// its quick checks (the agent, its CLI and model, the card's project), before the fetch and the worktree; the queue's
/// next card of the same agent waits for that (`pull`).
async fn start_inner(st: &AppState, task_id: &str, agent_id: Option<String>, bin_override: Option<String>, trigger: &str, resume: Option<Resume>,
                     claim: Option<Starting>, checked: &mut Option<tokio::sync::oneshot::Sender<()>>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), StartError> {
    if st.runs.closing.load(Ordering::SeqCst) {
        return Err(StartError::Wait("Gizai is quitting".into()));
    }
    let _starting = match claim {
        Some(c) => c,
        None => Starting::take(st, task_id, Some(agent_id.as_deref().unwrap_or_default()))
            .ok_or_else(|| StartError::Wait("This card's run is already starting: Gizai is getting its worktree ready".into()))?,
    };
    if let Ok(t) = tasks::get(&st.db, task_id) && t.archived_at.is_some() {
        return Err(StartError::Other(format!("{} is archived: restore it first", t.identifier)));
    }
    let max = get_settings(st).max_concurrent_runs;
    // the other cards under way count, those still starting too: cards start side by side (GA-191)
    if under_way(st, None, Some(task_id)) as u32 >= max {
        return Err(StartError::Wait(format!("{max} runs are already active; wait for one to finish or raise the limit in Settings")));
    }
    // GA-202: no new run while the runs use most of their memory limit or the computer has little free.
    if let Some(why) = crate::run_memory::wait_reason(st) {
        return Err(StartError::Wait(why));
    }
    // GA-212: nor a test run while its project's other cards fill its Test runs at once, those still starting too.
    if let Some(why) = test_runs_full(st, task_id) {
        return Err(StartError::Wait(why));
    }
    let (agent_id, role) = match agent_id {
        Some(a) => { let m = team::agent(&st.db, &a).map_err(|e| e.to_string())?; (m.actor_id, m.role_key) }
        None => choose_agent(st, task_id)?,
    };
    _starting.for_agent(&agent_id);
    let agent = team::agent(&st.db, &agent_id).map_err(|e| e.to_string())?;
    if agent.status != "active" {
        return Err(StartError::Wait(format!("{} is paused", agent.name)));
    }
    if let Some(budget) = agent.budget_usd_micros {
        let spent = core_runs::agent_spend_since(&st.db, &agent_id, core_runs::month_start_ms(ids::now_ms())).unwrap_or(0);
        if spent >= budget {
            return Err(StartError::Wait(format!("{} has used its monthly budget (${:.2} of ${:.2}); raise it on the Team page",
                                                agent.name, spent as f64 / 1e6, budget as f64 / 1e6)));
        }
    }
    let cli = crate::clis::of_agent(st, agent.adapter.as_deref()).map_err(StartError::Card)?;
    // The agent may have moved to another CLI since the run being continued: its session isn't there.
    if let Some(r) = resume.as_ref().filter(|r| r.cli.id != cli.id) {
        return Err(format!("{} now runs on {}, and this run was on {}: Run starts the card fresh on {}",
                           agent.name, cli.name, r.cli.name, cli.name).into());
    }
    // finding the CLI's program can start your login shell (GA-191: off the async runtime)
    let spec = {
        let (st2, cli2, bin) = (st.clone(), cli.clone(), bin_override.clone());
        tokio::task::spawn_blocking(move || crate::clis::spec(&st2, &cli2, bin)).await.map_err(|e| StartError::Other(e.to_string()))?
            .map_err(StartError::Card)?
    };
    if let Some(model) = agent.model.as_deref().map(str::trim).filter(|m| !m.is_empty() && *m != "default") {
        check_model(st, &cli, &spec, bin_override.is_some(), model, &agent.name).await?;
    }
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let project = projects::get(&st.db, task.project_id.as_deref().unwrap_or_default()).map_err(|e| StartError::Card(e.to_string()))?;
    let repo = project.repo_path.clone().filter(|p| !p.trim().is_empty())
        .ok_or_else(|| StartError::Card(format!("Link a git repository to {} first (project page → Edit)", project.name)))?;
    // Past the quick checks: the queue's next card of this agent may start too (`pull`).
    if let Some(tx) = checked.take() {
        let _ = tx.send(());
    }
    // With a link (GitHub, Bitbucket or another git URL), a card starts from the main branch just fetched from it, and
    // a card that already has a branch hears how far that main has moved on.
    let start = {
        let (p, dir) = (project.clone(), PathBuf::from(&repo));
        tokio::task::spawn_blocking(move || crate::git::start_point(&p, &dir, Some(crate::git::START_FETCH_LIMIT)))
            .await.map_err(|e| StartError::Other(e.to_string()))?.map_err(|e| StartError::Card(e.to_string()))?
    };
    if project.repo_url.is_some() {
        crate::code::fetched(st, &project.key);
    }
    // A card the queue or Gizai's nudge starts waits when another of the agent's cards failed meanwhile: asked before its
    // worktree is made and again before its process spawns.
    let queued = matches!(trigger, "assigned" | core_runs::RESULT_NUDGE);
    let wait_if_paused = || match pull_paused(st, &agent_id) {
        Some(why) if queued => Err(StartError::Wait(format!("{} stopped taking cards: {why}", agent.name))),
        _ => Ok(()),
    };
    wait_if_paused()?;
    let wt = open_worktree(st, &project, &task, Path::new(&repo), &start).await?;
    if let Some(todo) = worktree::unprepared(&wt.path) {
        prepare_worktree(st, Some(&agent_id), &task, &project, Path::new(&repo), &wt, todo).await?;
    }
    let behind = if project.repo_url.is_none() || wt.created { 0 } else {
        let (dir, from) = (wt.path.clone(), start.clone());
        tokio::task::spawn_blocking(move || worktree::behind(&dir, &from).unwrap_or(0)).await.unwrap_or(0)
    };
    let base = project.repo_url.as_ref().map(|_| BaseInfo {
        from: start.strip_prefix("refs/remotes/").or_else(|| start.strip_prefix("refs/")).unwrap_or(&start).to_string(),
        behind,
    });

    // Gizai may have begun to quit while the worktree was made ready.
    if is_closing(st) {
        return Err(StartError::Other("Gizai is quitting".into()));
    }
    wait_if_paused()?;
    let session = resume.as_ref().map(|r| r.session.clone()).unwrap_or_else(ids::new_id);
    let log_dir = st.data_dir.join("runs");
    std::fs::create_dir_all(&log_dir).map_err(|e| StartError::Other(e.to_string()))?;
    // A continued session gets a log of its own: each run shows only its own output.
    let log_path = match &resume {
        Some(_) => { let tag = ids::new_id(); log_dir.join(format!("{session}-{}.jsonl", &tag[tag.len() - 8..])) }
        None => log_dir.join(format!("{session}.jsonl")),
    };
    let wt_path = wt.path.to_string_lossy().to_string();
    // A Continue (a person's, the Team Lead's, Done, continue) is recorded as `nudge`. Gizai's own nudge is too, marked
    // nudged (`create_nudge`), and reads as `result_nudge`.
    let db_trigger = if resume.is_some() { "nudge" } else { trigger };
    let log = log_path.to_string_lossy();
    let run_id = if resume.as_ref().is_some_and(|r| r.nudge) {
        core_runs::create_nudge(&st.db, &agent_id, task_id, &role, &session, &wt_path, &wt_path, &wt.branch, &log)
    } else {
        core_runs::create_with_trigger(&st.db, &agent_id, task_id, &role, db_trigger, &session, &wt_path, &wt_path, &wt.branch, &log)
    }.map_err(|e| StartError::Other(e.to_string()))?;
    // The note of whoever continued it goes on the card as their comment, now that the run is recorded: a start that
    // didn't get this far leaves no comment behind.
    if let Some(n) = resume.as_ref().and_then(|r| r.note.as_ref()) {
        match comments::add(&st.db, &n.by, task_id, &n.text, None) {
            Ok(_) => (st.notify)(Note::RowsChanged("comments")),
            Err(e) => eprintln!("gizai: saving the note for run {run_id} as a comment failed: {e}"),
        }
    }
    // GA-70: the card's last question went to you from the Team Lead, and you answered on the card: the Team Lead keeps
    // your answer in memory, linked to the card.
    crate::ask_lead::learn(st, task_id, &run_id);

    let head = { let dir = wt.path.clone(); tokio::task::spawn_blocking(move || worktree::rev_parse(&dir, "HEAD").ok()).await.ok().flatten() };
    if let Some(sha) = head {
        let _ = core_runs::set_base_sha(&st.db, &run_id, &sha);
    }

    let recent: Vec<(String, String)> = comments::list(&st.db, task_id).unwrap_or_default().into_iter().rev().take(10).rev()
        .map(|c| (c.author_name, c.body_md)).collect();
    let limits = { let s = get_settings(st); RunLimits { minutes: s.max_run_minutes, tool_calls: s.max_run_tool_calls } };
    let ctx = TaskContext {
        identifier: task.identifier.clone(), title: task.title.clone(), description_md: task.description_md.clone(),
        acceptance_md: task.acceptance_md.clone().unwrap_or_default(), recent_comments: recent, role: role.clone(),
        qa_issues: if role == "qa" { vec![] } else { core_runs::last_qa_issues(&st.db, task_id).unwrap_or_default() },
        limits: Some(limits), base, project_goal_md: project.goal_md.clone().unwrap_or_default(),
    };
    let instructions = agent.instructions_md.clone().filter(|i| !i.trim().is_empty()).unwrap_or_else(|| gizai_core::seed::role_template(&role));
    // The agent's folders (agent form → Folders): one that is missing, or refused by now, is skipped and the run log
    // says so, as it says which ones this CLI can't be given.
    let (folders, mut notes) = gizai_core::folders::for_run(&agent.folders, &gizai_core::folders::Places::of(&st.db));
    let folders: Vec<RunFolder> = folders.into_iter().map(|f| RunFolder { change: f.change(), path: f.path }).collect();
    notes.extend(agent_cli::folders_left_out(spec.kind, &folders));
    // The worktree's own `.gizai-tmp`, out of git status and empty: the screenshots and the preview go there (GA-182).
    let own_temp = worktree::prepare_temp(&wt.path);
    // GA-210: the run's temp folder is short and outside the worktree, Gizai's data folder and any git repository, and
    // empty: every CLI gets it as TMPDIR, TMP and TEMP, and may read and change it. When it can't be used the run gets
    // `.gizai-tmp`, as before; without that either it still starts. Its log says why.
    let run_temp = worktree::prepare_run_temp(&st.temp_base, &wt.path);
    let temp_dir = match (&run_temp, &own_temp) {
        (Ok(d), _) => Some(d.to_string_lossy().to_string()),
        (Err(why), Ok(own)) => {
            notes.push(format!("Couldn't use a short temp folder for this run: {why}. TMPDIR, TMP and TEMP point at {} instead, where the \
                                browser may not start (its socket path can be too long).", own.display()));
            Some(own.to_string_lossy().to_string())
        }
        (Err(why), Err(e)) => {
            notes.push(format!("Couldn't make this run's temp folder: {why}; nor {}: {e}. TMPDIR, TMP and TEMP stay as they were.",
                               wt.path.join(worktree::TEMP_DIR).display()));
            None
        }
    };
    if let (Ok(_), Err(e)) = (&run_temp, &own_temp) {
        eprintln!("gizai: couldn't make {} for run {run_id}: {e}", wt.path.join(worktree::TEMP_DIR).display());
    }
    let run_temp_used = run_temp.is_ok();
    // GA-92: the agent's own `git push` to the project's repository goes the way Gizai's push after the run goes, through
    // git settings in the run's environment only (every CLI, new and continued runs). Over HTTPS without gh the run
    // starts without them, and its log says why.
    let (st2, p2, dir) = (st.clone(), project.clone(), wt.path.clone());
    let git_config = match tokio::task::spawn_blocking(move || crate::pulls::run_git_config(&st2, &p2, &dir)).await {
        Ok(Ok(c)) => c,
        Ok(Err(e)) => {
            notes.push(format!("Couldn't set up this run's own git push over HTTPS with gh's login (Settings → GitHub): {e}. A git \
                                push in this run goes as the repository's remote says."));
            vec![]
        }
        Err(e) => {
            eprintln!("gizai: setting up the own git push of run {run_id} failed: {e}");
            vec![]
        }
    };
    let allowed_tools: Vec<String> =
        if agent.allowed_tools.is_empty() { DEFAULT_TOOLS.iter().map(|s| s.to_string()).collect() } else { agent.allowed_tools.clone() };
    // Web search, fetching pages and the CLI's other tools come only from their switches (agent form → Tools), never from the
    // allowed commands, whoever put them there: with a switch off, the tool is absent.
    let (allowed_tools, by_switch): (Vec<String>, Vec<String>) =
        allowed_tools.into_iter().partition(|t| !gizai_agents::tool_catalog::only_by_switch(t));
    if !by_switch.is_empty() {
        notes.push(format!("Left out of {}'s allowed commands: {}. Web search, fetching pages and the CLI's other tools come only from \
                            their switches (agent form → Tools).", agent.name, by_switch.join(", ")));
    }
    let permission_mode = agent.permission_mode.clone().unwrap_or_default();
    // "How this run works" ends every task prompt, new and continued.
    let rules = prompt::RunRules {
        kind: spec.kind, mode: permission_mode.clone(), allowed_tools: allowed_tools.clone(),
        folders: folders.iter().map(|f| f.path.clone()).collect(), temp_dir: temp_dir.clone(),
    };
    // The agent's MCP servers (agent form → Tools), on Claude Code for now: each in this run's own MCP config, its tools
    // allowed or refused by their switches. One it can't use (signed out, a refused refresh, a secret missing from the
    // keychain) is left out, and the log says why.
    let mut allowed_tools = allowed_tools;
    let (mut mcp_config, mut mcp_refused) = (None, vec![]);
    // GA-182: the browser is in this run, with its take_screenshot on (a run without it takes no screenshots)
    let mut browser_in_run = false;
    // The servers Claude Code signs in to: a needs-auth in the init line then says to sign Claude Code in, not Gizai.
    let mut claude_code_signs_in: Vec<String> = vec![];
    if agent.tools.servers_on().next().is_some() {
        if spec.kind == Kind::ClaudeCode {
            let (st2, a2, cap) = (st.clone(), agent.clone(), std::time::Duration::from_secs(limits.minutes * 60));
            let (servers, left_out) = tokio::task::spawn_blocking(move || crate::mcp_servers::for_run(&st2, &a2, cap)).await.unwrap_or_default();
            notes.extend(left_out);
            if !servers.is_empty() {
                let path = log_dir.join(format!("{run_id}.mcp.json"));
                match gizai_agents::mcp_run::write_config(&path, &gizai_agents::mcp_run::config(vec![], &servers)) {
                    Ok(()) => {
                        browser_in_run = servers.iter().any(|s| s.name == gizai_core::mcp_servers::BROWSER
                                                                && !s.tools_off.iter().any(|t| t == "take_screenshot"));
                        let (allow, refuse) = gizai_agents::mcp_run::permissions(&servers);
                        allowed_tools.extend(allow);
                        mcp_refused = refuse;
                        mcp_config = Some(path);
                        claude_code_signs_in = crate::mcp_servers::claude_code_names(st, &servers);
                    }
                    Err(e) => notes.push(format!("Couldn't write this run's MCP config, so it goes without its MCP servers: {e}")),
                }
            }
        } else {
            notes.push(format!("{} runs on {}: MCP servers work on Claude Code for now, so this run goes without them.", agent.name, cli.name));
        }
    }
    // The agent's Web switches and the CLI's built-in tools it has on (agent form → Tools), as far as its CLI takes them.
    let (web, web_notes) = web_for_run(&agent, spec.kind, &cli.name);
    notes.extend(web_notes);
    if spec.kind == Kind::ClaudeCode {
        allowed_tools.extend(agent.cli_tools.builtin.iter().filter(|t| gizai_agents::tool_catalog::switchable(spec.kind, t)).cloned());
    }
    // Slash commands and skills (agent form → Tools → Built-in tools): on Claude Code only, new, continued and nudged runs
    // alike. On another CLI the run goes without them, and its log says why.
    let no_slash = gizai_agents::tool_catalog::slash_support(spec.kind);
    if agent.cli_tools.slash_commands && let Some(why) = no_slash {
        notes.push(format!("{} runs on {}, so this run goes without slash commands and skills: {why}", agent.name, cli.name));
    }
    let slash_commands = agent.cli_tools.slash_commands && no_slash.is_none();
    // Content from outside (an MCP server's answers, web pages, search results, the browser) is data: the prompt says so. A
    // CLI that searches the web in every run (Gemini) gets it in every run.
    let untrusted = mcp_config.is_some() || web.search || web.fetch || gizai_agents::tool_catalog::web_in_every_run(spec.kind);
    let note = resume.as_ref().and_then(|r| r.note.as_ref()).map(|n| prompt::Note { from: n.by_name.clone(), text: n.text.clone() });
    // Memory (GA-19): the agent's own notes and the shared notes for this card, as a section after the task; the Runs tab
    // lists them. A continued run has them in its session already.
    let memory = if resume.is_none() { crate::memory::run_block(st, &agent, &role, &project) } else { Default::default() };
    if !memory.given.is_empty() && let Err(e) = gizai_core::memory::record_given(&st.db, &run_id, &memory.given) {
        eprintln!("gizai: saving which notes run {run_id} was given failed: {e}");
    }
    // Screenshots (GA-182): a run whose agent has the browser and takes screenshots, on a card (or project) with them on,
    // in a project with a Preview command, gets the Screenshots section, and its preview is served first. Any other run
    // starts as before.
    let mut shots = if spec.kind == Kind::ClaudeCode && crate::screenshots::gets_section(st, browser_in_run, &agent_id, &role, &task, &project) {
        Some(crate::screenshots::start(st, &run_id, &role, &project, &wt.path).await)
    } else {
        None
    };
    if let Some(n) = shots.as_mut().and_then(|s| s.note.take()) {
        notes.push(n);
    }
    let base_prompt = prompt::with_rules(&prompt::with_screenshots(&match &resume {
        Some(Resume { nudge: true, .. }) => prompt::nudge_prompt(Some(limits)),
        Some(Resume { answer: Some(a), .. }) => prompt::answered_prompt_with(a, note.as_ref(), Some(limits)),
        Some(r) => prompt::continue_prompt_with(&r.reason, note.as_ref(), Some(limits)),
        None => prompt::with_memory(&prompt::build_from(&ctx, &instructions, fetched_from(&project)), &memory.text),
    }, shots.as_ref().map(|s| &s.section)), &rules);
    let writable_dirs: Vec<String> = if spec.kind == Kind::Codex {
        let dir = wt.path.clone();
        tokio::task::spawn_blocking(move || git_common_dir(&dir)).await.ok().flatten().into_iter().collect()
    } else {
        vec![]
    };
    let run = TaskRun {
        session_id: session.clone(), resume: resume.is_some(),
        prompt: if untrusted { format!("{base_prompt}\n\n{}", gizai_agents::mcp_run::UNTRUSTED) } else { base_prompt },
        permission_mode, allowed_tools, mcp_config: mcp_config.clone(), disallowed_tools: mcp_refused, web, slash_commands,
        model: agent.model.clone(), max_budget_usd: get_settings(st).max_run_usd, effort: agent.effort.clone(),
        // A worktree's commits go to the repository's git folder, outside the worktree: Codex's sandbox must be able to write it.
        writable_dirs,
        folders, temp_dir, git_config,
    };
    // GA-202: the CLI and everything it starts run in the slice of the agent runs, under their memory limit (Linux).
    let mut exec = process::Exec { slice: st.run_memory.slice(), ..agent_cli::task_exec(&spec, &run) };
    // GA-182: the agent's own commands find the preview (GIZAI_PREVIEW_PORT, _URL and _DIR; not PORT)
    if let Some(s) = &shots {
        exec.env.extend(s.env.iter().cloned());
    }
    // The log says which CLI wrote it, so it can be read again after the run (Claude Code's needs no header), then which
    // MCP servers Claude Code signs in to, then Gizai's notes.
    let head: String = (spec.kind != Kind::ClaudeCode).then(|| agent_cli::log_header(spec.kind)).into_iter()
        .chain((!claude_code_signs_in.is_empty()).then(|| agent_cli::claude_code_line(&claude_code_signs_in)))
        .chain(notes.iter().map(|n| agent_cli::note_line(n)))
        .map(|l| format!("{l}\n")).collect();
    let drop_config = || if let Some(p) = &mcp_config { let _ = std::fs::remove_file(p); };
    if !head.is_empty() {
        if let Err(e) = std::fs::write(&log_path, head) {
            drop_config();
            let _ = core_runs::finish(&st.db, &run_id, "failed", None, 0, 0, 0, Some(&e.to_string()));
            return Err(StartError::Other(e.to_string()));
        }
    }
    let mut parser = agent_cli::Parser::new(spec.kind);
    parser.claude_code_signs_in(claude_code_signs_in);
    let mut handle = match process::spawn_exec::<RunEvent, _>(&exec, &wt.path, &log_path,
        Caps { max_time: std::time::Duration::from_secs(limits.minutes * 60), max_tool_calls: limits.tool_calls, after_result: st.runs.after_result() },
        parser) {
        Ok(h) => h,
        Err(e) => {
            drop_config();
            let msg = e.to_string();
            record_head(&st.db, &run_id, &wt.path);
            let _ = core_runs::finish(&st.db, &run_id, "failed", None, 0, 0, 0, Some(&msg));
            (st.notify)(Note::RowsChanged("runs"));
            // The CLI couldn't be started: a start that can't work, not a failed run.
            return Err(StartError::Card(msg));
        }
    };
    let _ = core_runs::set_running(&st.db, &run_id, handle.pid);
    // The Run panel shows the notes first, as the log has them.
    let noted: Vec<SeqEvent> = notes.into_iter().enumerate().map(|(i, text)| SeqEvent { seq: i as u64, event: RunEvent::Note { text } }).collect();
    st.runs.live.lock().unwrap().insert(run_id.clone(), Live {
        task_id: task_id.to_string(), agent_id: agent_id.clone(), stop: handle.stop.clone(), next_seq: noted.len() as u64, events: noted,
    });
    // The process runs: a card in a To do-type column moves to its next column (from Backlog, for a person's Run, to In
    // progress), as the agent. No dispatch.
    let moved_from = match workflow::move_on_start(&st.db, &agent_id, task_id) {
        Ok(from) => from,
        Err(e) => { eprintln!("gizai: moving {} to In progress failed: {e}", task.identifier); None }
    };
    if moved_from.is_some() {
        (st.notify)(Note::RowsChanged("tasks"));
    }
    // Quitting began just now and may have missed this run: it stops with the others.
    if is_closing(st) {
        stop(st, &run_id);
    }
    (st.notify)(Note::RunsChanged);
    (st.notify)(Note::RowsChanged("runs"));

    let st2 = st.clone();
    let rid = run_id.clone();
    let tid = task_id.to_string();
    let cli_name = cli.name.clone();
    let kind = spec.kind;
    let dir = wt.path.clone();
    let (run_agent, run_config) = (agent_id.clone(), mcp_config.clone());
    let (seen_log, seen_cli) = (log_path.clone(), cli.id.clone());
    let agent_name = agent.name.clone();
    let shots_agent = agent_id.clone();
    let done = tokio::spawn(async move {
        let mut result: Option<RunEvent> = None;
        let mut capped: Option<String> = None;
        let mut exit = String::new();
        let mut tools = 0usize;
        let mut refused: Vec<Refusal> = vec![];
        // GA-202: the processes Linux ended in this run at the memory limit for agent runs
        let mut ended_at_limit = 0u64;
        // GA-211: the processes Gizai ended at the run's end that it left running in its scope
        let mut left: Vec<gizai_agents::run_memory::Leftover> = vec![];
        // GA-221: whether Gizai ended the CLI because it stayed quiet once the agent was done, and the agent's last text
        // that said so (with its GIZAI_RESULT line), its result when the CLI gave none
        let (mut after_result, mut done_text) = (false, None);
        while let Some(ev) = handle.events.recv().await {
            // What Claude Code heard of the account's limits: kept for this run's CLI (the Usage page's Subscription tab),
            // not shown in the Run panel.
            if let RunEvent::Limits { info } = &ev {
                crate::limits::from_claude(&st2, &rid, info);
                continue;
            }
            // Linux ended a process in the run at the memory limit: the card's activity says so at once, the run's own
            // note at its end.
            if let RunEvent::Other { raw_type } = &ev && let Some(n) = raw_type.strip_prefix(process::OOM_KILL) {
                if ended_at_limit == 0 {
                    crate::run_memory::ended_on_card(&st2, &tid, &agent_name);
                }
                ended_at_limit = n.parse().unwrap_or(ended_at_limit + 1).max(ended_at_limit);
                continue;
            }
            // A process the run left running in its scope, ended: the run's note at its end names it.
            if let RunEvent::Other { raw_type } = &ev && let Some(l) = process::leftover(raw_type) {
                left.push(l);
                continue;
            }
            match &ev {
                RunEvent::ToolUse { .. } => tools += 1,
                RunEvent::Text { text } if ev.is_result() => done_text = Some(text.clone()),
                RunEvent::Refused { tool, input, reason } => refused.push(Refusal { tool: tool.clone(), input: input.clone(), reason: reason.clone() }),
                // Codex names its session itself: Continue resumes that one.
                RunEvent::Init { session_id, .. } if !session_id.is_empty() && *session_id != session => {
                    let _ = core_runs::set_session(&st2.db, &rid, session_id);
                }
                RunEvent::Result { .. } => result = Some(ev.clone()),
                RunEvent::McpServers { servers } => crate::mcp_servers::record_states(&st2, &run_agent, servers),
                // A cap, or Gizai's end once the agent was done: whichever came first says how the run ended.
                RunEvent::Other { raw_type } if raw_type.starts_with("cap_exceeded") && capped.is_none() && !after_result => capped = Some(raw_type.clone()),
                RunEvent::Other { raw_type } if raw_type == process::ENDED_AFTER_RESULT && capped.is_none() => after_result = true,
                RunEvent::Other { raw_type } if raw_type.starts_with("exit:") => exit = raw_type.clone(),
                _ => {}
            }
            push_event(&st2, &rid, ev);
        }
        let capped = capped.map(|c| if c.ends_with(":time") {
            format!("stopped at the time limit of {} minutes per run (Settings → Runs)", limits.minutes)
        } else {
            format!("stopped at the limit of {} tool calls per run (Settings → Runs)", limits.tool_calls)
        });
        // GA-221: that text is the run's result only when Gizai ended the CLI once the agent was done and no result came.
        let done_text = done_text.filter(|_| after_result && result.is_none());
        if ended_at_limit > 0 {
            note_run(&st2, &rid, &crate::run_memory::ended_note(&st2, ended_at_limit));
        }
        // The run's process group has ended, whatever way (finished, Stop, a limit): its throwaway files go, its MCP
        // config (with its servers' secrets) first.
        if let Some(p) = &run_config {
            let _ = std::fs::remove_file(p);
        }
        // The tools Claude Code's init line named: the agent's "seen in the last run" (agent form → Tools → Built-in tools).
        if kind == Kind::ClaudeCode {
            crate::mcp_servers::record_seen_tools(&st2, &run_agent, &seen_cli, &seen_log);
        }
        // GA-182: its preview's whole tree ends, then its screenshots go onto the card, before its .gizai-tmp is emptied.
        // A run Gizai's quitting stopped collects nothing (as `finish_run` tells such a run).
        let mut shots_note = None;
        if let Some(run_shots) = shots {
            let finished = (matches!(&result, Some(RunEvent::Result { is_error: false, .. })) || done_text.is_some())
                && (exit == "exit:0" || after_result);
            let quit = is_closing(&st2) && (st2.runs.stopped.lock().unwrap().contains(&rid) || !finished);
            let (text, preview_left) = crate::screenshots::finish(&st2, &rid, &tid, &shots_agent, &dir, run_shots, !quit).await;
            left.extend(preview_left);
            shots_note = text;
        }
        // GA-211: what the run left running in its scope (and its preview in the preview's), which Gizai ended: each
        // process once. Nothing when it left nothing.
        if !left.is_empty() {
            note_run(&st2, &rid, &crate::run_memory::leftovers_note(&left));
        }
        if let Some(text) = shots_note {
            note_run(&st2, &rid, &text);
        }
        if let Err(e) = worktree::empty_temp(&dir) {
            eprintln!("gizai: couldn't empty the .gizai-tmp folder of run {rid}: {e}");
        }
        if run_temp_used && let Err(e) = worktree::empty_run_temp(&st2.temp_base, &dir) {
            eprintln!("gizai: couldn't empty the temp folder of run {rid}: {e}");
        }
        // The account's limits as the run left them: Codex's from its session log, or the limit a Claude Code run that
        // failed says it hit (its result, else its stderr). Never from a successful run's text: an agent may quote one.
        let failed_text = match &result {
            _ if kind != Kind::ClaudeCode => None,
            Some(RunEvent::Result { is_error: true, text, .. }) => Some(text.clone()),
            Some(_) => None,
            // GA-221: Gizai ended it after the agent's final text: a run that finished
            None if done_text.is_some() => None,
            None => Some(stderr_tail(&rid, &st2)),
        };
        crate::limits::after_run(&st2, &rid, kind, failed_text.as_deref()).await;
        let ran = Ran { result, capped, exit, tools, moved_from, queued, refused, after_result, done_text };
        finish_run(&st2, &rid, &tid, &dir, ran, &cli_name).await
    });
    Ok((run_id, done))
}

/// The Web switches a run of `agent` on a CLI of `kind` gets, and a note for each it can't take (agent form → Tools → Web).
pub(crate) fn web_for_run(agent: &team::Member, kind: Kind, cli_name: &str) -> (agent_cli::WebTools, Vec<String>) {
    let t = &agent.cli_tools;
    let (search, fetch, domains) = gizai_agents::tool_catalog::web_support(kind);
    let mut notes = vec![];
    // Gemini searches the web by its own policy whatever the switch says (the form shows that): no note for it.
    let search_on = t.web_search && search.is_none();
    if t.web_search && kind != Kind::Gemini && let Some(why) = search {
        notes.push(format!("{} runs on {cli_name}, so this run goes without web search: {why}", agent.name));
    }
    // A domain list a CLI can't keep to leaves fetching out, rather than letting it fetch any page.
    let why_not_fetch = fetch.or(if t.fetch_domains.is_empty() { None } else { domains });
    let fetch_on = t.web_fetch && why_not_fetch.is_none();
    if t.web_fetch && let Some(why) = why_not_fetch {
        notes.push(format!("{} runs on {cli_name}, so this run goes without fetching pages: {why}", agent.name));
    }
    let web = agent_cli::WebTools { search: search_on, fetch: fetch_on, fetch_domains: if fetch_on { t.fetch_domains.clone() } else { vec![] } };
    (web, notes)
}

/// The repository's shared git folder for a worktree (where its commits go), as an absolute path.
fn git_common_dir(wt: &Path) -> Option<String> {
    let out = gizai_agents::os::command("git").args(["rev-parse", "--path-format=absolute", "--git-common-dir"]).current_dir(wt)
        .stdin(std::process::Stdio::null()).output().ok().filter(|o| o.status.success())?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!dir.is_empty()).then_some(dir)
}

/// Saves the commit the run's worktree `dir` ended at (`runs.head_sha`). With the one it started at, it gives the
/// commits the run made (`commits`). Only a worktree counts: never the commit of a repository around it.
pub(crate) fn record_head(db: &gizai_core::db::Db, run_id: &str, dir: &Path) {
    if !dir.join(".git").exists() {
        return;
    }
    if let Ok(sha) = worktree::rev_parse(dir, "HEAD") {
        let _ = core_runs::set_head_sha(db, run_id, &sha);
    }
}

/// The commits a finished run made, oldest first: from the commit it started at to the one it ended at, along its
/// branch's own line (`worktree::commits`). Read in its worktree, else in the project's repository (once the worktree
/// is gone: they share their commits).
pub fn commits(st: &AppState, run_id: &str) -> Result<Vec<worktree::Commit>, String> {
    let run = core_runs::get(&st.db, run_id).map_err(|e| e.to_string())?;
    let (Some(base), Some(head)) = (run.base_sha.as_deref(), run.head_sha.as_deref()) else {
        return Err("Gizai didn't save where this run started and ended".into());
    };
    if let Some(wt) = run.worktree_path.as_deref().map(Path::new).filter(|p| p.join(".git").exists())
        && let Ok(list) = worktree::commits(wt, base, head)
    {
        return Ok(list);
    }
    let repo = tasks::get(&st.db, run.task_id.as_deref().unwrap_or_default()).ok()
        .and_then(|t| projects::get(&st.db, t.project_id.as_deref().unwrap_or_default()).ok())
        .and_then(|p| p.repo_path).filter(|p| !p.trim().is_empty())
        .ok_or("Couldn't read its commits: its worktree is gone and its project has no git repository")?;
    worktree::commits(Path::new(&repo), base, head).map_err(|e| format!("Couldn't read its commits: {e}"))
}

/// The agent's model must be one its Claude Code offers (its /model list, or a full model id): a wrong name can't work.
/// When the list can't be read, the run starts and Claude Code itself says what is wrong.
async fn check_model(st: &AppState, cli: &gizai_core::clis::Cli, spec: &agent_cli::CliSpec, own_bin: bool, model: &str, agent: &str)
    -> Result<(), StartError> {
    if spec.kind != Kind::ClaudeCode || model.starts_with("claude-") {
        return Ok(());
    }
    let list = if own_bin {
        gizai_agents::models::fetch_models_with_env(&spec.bin, &st.data_dir, &spec.env).await.map_err(|e| e.to_string())
    } else {
        models_for(st, Some(cli.id.as_str()), false).await
    };
    let Ok(list) = list else { return Ok(()) };
    let base = model.split('[').next().unwrap_or(model);
    if list.is_empty() || list.iter().any(|m| m.value == model || m.value == base || m.resolved_model.as_deref() == Some(model)) {
        return Ok(());
    }
    let names: Vec<&str> = list.iter().map(|m| m.value.as_str()).collect();
    Err(StartError::Card(format!("{} has no model called {model} (it offers {}): pick another model for {agent} on the Team page",
                                 cli.name, names.join(", "))))
}

/// What a run's process left behind. `capped`: why Gizai stopped the run at a limit, if it did. `tools`: how many tool
/// calls it made. `moved_from`: the column the card was in before the start moved it on (To do → its next column).
/// `queued`: the queue started it, not a person. `refused`: the tool calls its CLI refused (Refused in this run).
/// `after_result`: Gizai ended its CLI because it stayed quiet once the agent was done (GA-221,
/// `process::ENDED_AFTER_RESULT`); `done_text`: then, without a result line, the agent's last text with its GIZAI_RESULT
/// line, which stands for the result.
struct Ran {
    result: Option<RunEvent>,
    capped: Option<String>,
    exit: String,
    tools: usize,
    moved_from: Option<String>,
    queued: bool,
    refused: Vec<Refusal>,
    after_result: bool,
    done_text: Option<String>,
}

/// Claude Code's own words for a missing or expired login.
fn login_problem(error: &str) -> bool {
    let e = error.to_lowercase();
    ["/login", "not logged in", "invalid api key", "oauth token has expired", "authentication_error", "please log in"].iter().any(|m| e.contains(m))
}

/// `dir`: the run's worktree. `cli`: the CLI's name, for the reason a run failed.
async fn finish_run(st: &AppState, run_id: &str, task_id: &str, dir: &Path, ran: Ran, cli: &str) -> RunSummary {
    let Ran { result, capped, exit, tools, moved_from, queued, refused, after_result, done_text } = ran;
    let exit = exit.as_str();
    let stopped = st.runs.stopped.lock().unwrap().remove(run_id);
    let (cost, input, output, text, ok) = match (&result, done_text) {
        (Some(RunEvent::Result { cost_usd, input_tokens, output_tokens, text, is_error, .. }), _) =>
            ((cost_usd.unwrap_or(0.0) * 1_000_000.0).round() as i64, *input_tokens, *output_tokens, text.clone(), !is_error),
        // GA-221: Gizai ended the CLI after the agent's final text and no result line came: that text is the result.
        (_, Some(text)) if after_result => (0, 0, 0, text, true),
        _ => (0, 0, 0, String::new(), false),
    };
    let parsed = outcome::parse(&text);
    // Memory (GA-19): what the agent wants kept for its next runs, saved below once the run has ended.
    let learned = outcome::learned(&text);
    // "Run this for me" (GA-31): the commands a `needs_decision` asks you to run, kept with its verdict below.
    let run_for_me: Vec<String> = parsed.as_ref().filter(|o| o.outcome == "needs_decision").map(|o| o.run_for_me.clone()).unwrap_or_default();
    let verdict: Option<Outcome> = parsed.map(|o| Outcome { outcome: o.outcome, summary: o.summary, issues: o.issues });
    // GA-221: a run whose CLI Gizai ended once the agent was done has finished, whatever its exit code.
    let finished = ok && (exit == "exit:0" || after_result);
    // While Gizai quits, a run that didn't finish was stopped by the quit, also when its agent ended first: logging
    // out sends SIGTERM to the agents as well as to Gizai. It doesn't count as a failure.
    let quit = is_closing(st) && (stopped || !finished);
    let cancelled = stopped || quit;
    let status = if cancelled { "cancelled" } else if capped.is_some() { "timed_out" } else if finished { "succeeded" } else { "failed" };
    let error = match status {
        "cancelled" => Some(if quit { STOPPED_BY_QUIT } else { "stopped" }.to_string()),
        "timed_out" => capped.clone(),
        "failed" => Some(match &result {
            Some(RunEvent::Result { text, .. }) if !text.trim().is_empty() => format!("{cli}: {}", text.trim().chars().take(300).collect::<String>()),
            Some(RunEvent::Result { subtype, .. }) => format!("{cli} ended with {subtype}"),
            _ => {
                let tail = stderr_tail(run_id, st);
                format!("{cli} exited without a result ({}){}", exit.trim_start_matches("exit:"),
                        if tail.is_empty() { String::new() } else { format!(": {tail}") })
            }
        }),
        _ => None,
    };
    if status == "succeeded" && after_result {
        note_run(st, run_id, &after_result_note(cli));
    }
    record_head(&st.db, run_id, dir);
    let agent = core_runs::get(&st.db, run_id).map(|r| r.agent_id).unwrap_or_default();
    let unstarted = status == "failed" && tools == 0 && error.as_deref().is_some_and(login_problem);
    // Gizai pushes the card's branch itself (GA-56), before the run counts as ended and the card moves on, so the next
    // agent (QA, for its pull request) finds it on the remote, also when the agent's own push was refused. Not after a run
    // that couldn't start working, nor for a run Gizai's quitting stopped.
    let pushed = if quit || unstarted { None } else { push_after_run(st, run_id, dir).await };
    for text in pushed.as_ref().map(AfterRun::notes).unwrap_or_default() {
        note_run(st, run_id, &text);
    }
    let unpushed = pushed.and_then(|p| p.push.err());
    // A Stop pressed while Gizai pushed came after the run had ended: it changes nothing.
    st.runs.stopped.lock().unwrap().remove(run_id);
    if !refused.is_empty() && let Err(e) = core_runs::set_refused(&st.db, run_id, &refused) {
        eprintln!("gizai: saving what run {run_id} was refused failed: {e}");
    }
    let _ = core_runs::finish(&st.db, run_id, status, verdict.as_ref(), cost, input, output, error.as_deref());
    if verdict.is_some() && !learned.is_empty() {
        let card = gizai_core::tasks::get(&st.db, task_id).map(|t| t.identifier).unwrap_or_default();
        for text in crate::memory::save_learned(st, run_id, &agent, &card, &learned) {
            note_run(st, run_id, &text);
        }
    }
    let mut moved = false;
    let mut nudge = None;
    // Only a run that finished has a verdict for the gates, and only then do its commands to run count.
    let (gated, asks) = if status == "succeeded" { (verdict.as_ref(), run_for_me.as_slice()) } else { (None, &[][..]) };
    if let Some(reason) = &unpushed {
        // The push failed: the card stays where it is, on hold with the reason, whatever the run's answer (its summary is
        // still posted), so it shows in the Inbox. Nothing retries: the next run's end pushes again. Commands it asks you
        // to run are kept with its verdict, so the held card still shows them.
        if cancelled {
            hold_card(st, &agent, task_id, reason);
        } else if let Err(e) = workflow::hold_unpushed_with(&st.db, run_id, gated, asks, reason) {
            eprintln!("gizai: holding the card of run {run_id} failed: {e}");
        }
        if status == "failed" && !quit {
            pause_pull(st, &agent, error.as_deref().unwrap_or("its last run failed"));
        }
    } else if unstarted {
        // Claude Code couldn't start working (not logged in): not a failed run. The card goes back where it was, on hold;
        // one failure holds one card, so a card the queue started before another card's failure paused the agent only
        // waits again.
        let reason = error.clone().unwrap_or_default();
        let first = pause_pull(st, &agent, &reason);
        let done = if first || !queued { workflow::hold_unstarted(&st.db, run_id, &reason) } else { workflow::release_unstarted(&st.db, run_id) };
        if let Err(e) = done {
            eprintln!("gizai: holding the card of run {run_id} failed: {e}");
        }
        if let Some(from) = &moved_from {
            let _ = workflow::put_back(&st.db, &agent, task_id, from);
        }
    } else if !cancelled {
        match workflow::apply_outcome_with(&st.db, run_id, gated, asks) {
            Ok(g) => {
                moved = g.moved_to.is_some();
                // GA-70: the Team Lead took the question; it answers (and continues this session) or asks you.
                if let Some(lead) = &g.lead {
                    crate::ask_lead::start(st, run_id, lead);
                }
            }
            Err(e) => eprintln!("gizai: applying the outcome of run {run_id} failed: {e}"),
        }
        // It ended normally without its result line: Gizai continues it once (never after Stop, a limit or a failure).
        if status == "succeeded" && verdict.is_none() {
            nudge = nudge_for(st, run_id, task_id);
        }
        // A failing CLI must not run through the whole queue.
        if status == "failed" && !quit {
            pause_pull(st, &agent, error.as_deref().unwrap_or("its last run failed"));
        }
    }
    st.runs.live.lock().unwrap().remove(run_id);
    (st.notify)(Note::RowsChanged("tasks"));
    (st.notify)(Note::RowsChanged("comments"));
    (st.notify)(Note::RunsChanged);
    // A slot is free and the card may now belong to another agent (Testing → QA): agents take their next cards. A card
    // that was stopped or stayed put isn't restarted (`workflow::waiting_for`). Gizai's nudge goes first, so the queue
    // doesn't start the card afresh meanwhile.
    if !quit {
        let (st2, task, agent) = (st.clone(), task_id.to_string(), agent.clone());
        tokio::spawn(async move {
            if let Some(resume) = nudge {
                start_nudge(&st2, &task, &agent, resume).await;
            }
            pull(&st2).await;
        });
    }
    // In Review: a pull request the agent opened shows on the card at once, not at the next PR check.
    if moved && tasks::get(&st.db, task_id).is_ok_and(|t| t.state_category == "review") {
        crate::pulls::check_soon(st, task_id);
    }
    RunSummary { run_id: run_id.to_string(), status: status.into(), outcome: verdict.map(|v| v.outcome), cost_usd_micros: cost, error }
}

/// What Gizai's push after a run did (GA-56).
struct AfterRun {
    branch: String,
    /// What went, or why it couldn't, in plain words.
    push: Result<worktree::Pushed, String>,
    /// Files with uncommitted changes in the run's worktree: they stay there, Gizai pushes only commits.
    uncommitted: usize,
}

impl AfterRun {
    /// What the run's output says about it, a note each: what went ("Gizai pushed …", which the Run panel shows as good
    /// news) or why it couldn't, and the uncommitted changes left behind. Empty when nothing went and nothing was left.
    fn notes(&self) -> Vec<String> {
        let mut said: Vec<String> = match &self.push {
            Ok(worktree::Pushed::Commits(n)) => vec![format!("Gizai pushed {} ({n} {})", self.branch, if *n == 1 { "commit" } else { "commits" })],
            Ok(worktree::Pushed::Nothing) => vec![],
            Err(why) => vec![why.trim().to_string()],
        };
        if self.uncommitted > 0 {
            let n = self.uncommitted;
            said.push(format!("Its worktree has {n} uncommitted {}, which Gizai doesn't push", if n == 1 { "change" } else { "changes" }));
        }
        said.into_iter().map(|s| if s.ends_with('.') { s } else { s + "." }).collect()
    }
}

/// Gizai's push after a run (GA-56): the run's branch goes to the project's GitHub, Bitbucket or git link when it has
/// commits that remote doesn't have (`worktree::push_new_commits`). It goes where Open pull request pushes and over the
/// same Push over setting (`pulls::push_to`, `pulls::push_over_for`), never forced and tried once, so it doesn't matter
/// whether the agent's own push was refused. None: nothing to push to (no link or repository) or no branch.
async fn push_after_run(st: &AppState, run_id: &str, dir: &Path) -> Option<AfterRun> {
    let run = core_runs::get(&st.db, run_id).ok()?;
    let branch = run.branch.filter(|b| !b.trim().is_empty())?;
    let task = tasks::get(&st.db, run.task_id.as_deref()?).ok()?;
    let project = projects::get(&st.db, task.project_id.as_deref()?).ok()?;
    let repo = PathBuf::from(project.repo_path.clone().filter(|p| !p.trim().is_empty())?);
    let link = gizai_core::repo_url::normalize(project.repo_url.as_deref()?).ok().flatten()?;
    let (st2, dir) = (st.clone(), dir.to_path_buf());
    let after = tokio::task::spawn_blocking(move || {
        let push = push_card(&st2, &project, &repo, &link, &branch);
        AfterRun { uncommitted: worktree::uncommitted(&dir).unwrap_or(0), branch, push }
    }).await;
    match after {
        Ok(a) => {
            if let Err(why) = &a.push {
                eprintln!("gizai: after run {run_id}: {why}");
            }
            Some(a)
        }
        Err(e) => {
            eprintln!("gizai: pushing after run {run_id} failed: {e}");
            None
        }
    }
}

/// Gizai's push of a card's `branch` (GA-56), after a run and for a person's card (GA-111, `hand::push`): to the
/// project's link `link`, through your remote for it (where Open pull request pushes, `pulls::push_to`) and over the same
/// Push over setting (`pulls::push_over_for`), only when the branch has commits that remote doesn't have
/// (`worktree::push_new_commits`), never forced, tried once. Blocking: it talks to the remote.
pub(crate) fn push_card(st: &AppState, project: &Project, repo: &Path, link: &gizai_core::repo_url::RepoUrl, branch: &str)
    -> Result<worktree::Pushed, String> {
    let over = crate::pulls::push_over_for(st, &link.provider).map_err(|e| format!("Couldn't push {branch}: {e}"))?;
    // the main branch the card started from, as last fetched: the remote has its commits
    let base = crate::git::start_point(project, repo, None).map_err(crate::pulls::plain)?;
    worktree::push_new_commits(repo, &crate::pulls::push_to(repo, &link.url), branch, &base, &over).map_err(crate::pulls::plain)
}

/// GA-111: Open in editor, on a card assigned to you (contract C4's start). Its worktree is made, or taken over, and
/// prepared from main just fetched, as for an agent's run (`open_worktree`, `prepare_worktree`); meanwhile the card
/// counts as starting (`working_on`), so no agent's run starts on it. Then, in one write, it is pinned to you, you become
/// its implementer and it moves on from To do as you (`workflow::start_by_hand`). A preparation that fails puts no hold
/// on the card: the error says why, and the next Open in editor prepares again. Refused, in a plain sentence, while an
/// agent works on the card or its run starts, as `workflow::start_by_hand_check` says (an archived card, one not
/// assigned to you, one in Deploy, Done or Cancelled), and when the card's worktree is on another branch than the card's.
/// On a card that is yours by hand already it makes sure the worktree is there and ready. Returns the worktree.
pub async fn start_by_hand(st: &AppState, task_id: &str) -> Result<worktree::Worktree, String> {
    if is_closing(st) {
        return Err("Gizai is quitting".into());
    }
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let busy = || format!("An agent is working on {}: wait for its run to end", task.identifier);
    if working_on(st, task_id) {
        return Err(busy());
    }
    workflow::start_by_hand_check(&st.db, &st.you_id, task_id).map_err(|e| e.to_string())?;
    let _starting = Starting::take(st, task_id, None).ok_or_else(busy)?;
    let project = projects::get(&st.db, task.project_id.as_deref().unwrap_or_default()).map_err(|e| e.to_string())?;
    let repo = project.repo_path.clone().filter(|p| !p.trim().is_empty())
        .ok_or_else(|| format!("Link a git repository to {} first (project page → Edit)", project.name))?;
    let start = {
        let (p, dir) = (project.clone(), PathBuf::from(&repo));
        tokio::task::spawn_blocking(move || crate::git::start_point(&p, &dir, Some(crate::git::START_FETCH_LIMIT)))
            .await.map_err(|e| e.to_string())?.map_err(crate::pulls::plain)?
    };
    if project.repo_url.is_some() {
        crate::code::fetched(st, &project.key);
    }
    let wt = open_worktree(st, &project, &task, Path::new(&repo), &start).await.map_err(StartError::message)?;
    // Commit, Push and Hand over work on the card's branch (`hand`): its worktree must have it checked out.
    if let Some(b) = task.branch.as_deref().filter(|b| !b.trim().is_empty() && *b != wt.branch) {
        return Err(format!("{}'s worktree is on branch {}, not on the card's branch {b}: switch it back first", task.identifier, wt.branch));
    }
    if let Some(todo) = worktree::unprepared(&wt.path) {
        prepare_worktree(st, None, &task, &project, Path::new(&repo), &wt, todo).await.map_err(StartError::message)?;
    }
    workflow::start_by_hand(&st.db, &st.you_id, task_id, &wt.branch).map_err(|e| e.to_string())?;
    (st.notify)(Note::RowsChanged("tasks"));
    Ok(wt)
}

/// GA-221: the note on a run that finished after Gizai ended its CLI, `cli`, which hadn't exited once the agent was done.
fn after_result_note(cli: &str) -> String {
    format!("{cli} hadn't exited {} minutes after the agent's final message, so Gizai ended it and kept the result. Something the \
             run started was probably still running in the background, such as a server.", process::AFTER_RESULT.as_secs() / 60)
}

/// Gizai's note on a run that is ending: in its log, where Show output reads it later, and in the Run panel now.
fn note_run(st: &AppState, run_id: &str, text: &str) {
    if let Ok(run) = core_runs::get(&st.db, run_id) {
        use std::io::Write;
        let added = std::fs::OpenOptions::new().create(true).append(true).open(&run.log_path)
            .and_then(|mut log| writeln!(log, "{}", agent_cli::note_line(text)));
        if let Err(e) = added {
            eprintln!("gizai: couldn't add a note to the log of run {run_id}: {e}");
        }
    }
    push_event(st, run_id, RunEvent::Note { text: text.to_string() });
}

/// The live run's next event: numbered, kept for the Run panel (its last `BUFFER`) and queued for the window (`stream`),
/// all under the live runs' lock, so the window gets them in `seq` order. Nothing when the run isn't live.
fn push_event(st: &AppState, run_id: &str, event: RunEvent) {
    let mut live = st.runs.live.lock().unwrap();
    let Some(l) = live.get_mut(run_id) else { return };
    let seq = l.next_seq;
    l.next_seq += 1;
    l.events.push(SeqEvent { seq, event: event.clone() });
    if l.events.len() > BUFFER {
        l.events.remove(0);
    }
    crate::stream::run_event(st, run_id, SeqEvent { seq, event });
}

/// Gizai's nudge (GA-54): a run that ended normally without its GIZAI_RESULT line (succeeded, `no_result`) is continued
/// once, by itself, in the same session, like Continue. Often the agent ended its message to wait for something outside
/// the run (CI, a release, a deploy), and nothing would ever wake it up. What to resume, or None: the run was Gizai's
/// nudge itself (its card is held stalled instead, `workflow::apply_outcome`), the card is on hold (the third run
/// without a result holds it too), a person moved it to Backlog, Done or Cancelled, or its CLI has no session to resume
/// or its worktree is gone.
fn nudge_for(st: &AppState, run_id: &str, task_id: &str) -> Option<Resume> {
    let run = core_runs::get(&st.db, run_id).ok()?;
    if run.status != "succeeded" || run.outcome.as_deref() != Some("no_result") || run.nudged {
        return None;
    }
    let task = tasks::get(&st.db, task_id).ok()?;
    if task.hold.is_some() || task.archived_at.is_some() || matches!(task.state_category.as_str(), "backlog" | "done" | "cancelled") {
        return None;
    }
    let cli = crate::clis::of_agent(st, run.adapter.as_deref()).ok()?;
    if !Kind::parse(&cli.kind).is_some_and(Kind::can_resume) || !run.worktree_path.as_deref().is_some_and(|p| Path::new(p).is_dir()) {
        return None;
    }
    let session = run.session_id.filter(|s| !s.is_empty())?;
    Some(Resume { session, cli, reason: "it ended without a result".into(), answer: None, nudge: true, note: None })
}

/// Starts Gizai's nudge (`nudge_for`) only when a start is allowed now: Gizai isn't quitting, agents aren't paused in
/// Settings, the agent's pull isn't paused, the agent is active, within its budget and its cards at once, and "Runs at
/// once" has room. Otherwise nothing changes: the run stays without a result. Like the queue's start, one that can't
/// work holds the card "blocked" (`start_background`). Boxed, like `pull`: a finishing run starts it (start → finish →
/// nudge → start).
fn start_nudge<'a>(st: &'a AppState, task_id: &'a str, agent_id: &'a str, resume: Resume)
    -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if !takes_cards(st, agent_id) {
            return;
        }
        let Ok(agent) = team::agent(&st.db, agent_id) else { return };
        // its cards under way, those still starting too (GA-191)
        if under_way(st, Some(agent_id), None) as i64 >= agent.max_runs.max(1) {
            return;
        }
        // The start checks the rest: active, budget, Runs at once (a wait, so nothing changes). Its own trigger
        // (`result_nudge`), so the Runs list tells it from a Continue.
        if start_background(st, task_id, agent_id, core_runs::RESULT_NUDGE, Some(resume), None, None).await.is_some() {
            eprintln!("gizai: a run on card {task_id} ended without a result: continued it once");
        }
    })
}

/// The last lines the CLI wrote to stderr (argument errors, login problems), at most 400 characters.
fn stderr_tail(run_id: &str, st: &AppState) -> String {
    let Ok(run) = core_runs::get(&st.db, run_id) else { return String::new() };
    let text = std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" · ");
    let chars: Vec<char> = tail.chars().collect();
    if chars.len() > 400 { chars[chars.len() - 400..].iter().collect() } else { tail }
}

/// Starts a run and waits for it to finish (tests, and anything that wants the result).
pub async fn run_once(st: &AppState, task_id: &str, agent_id: Option<String>, bin_override: Option<String>) -> Result<RunSummary, String> {
    let (_, done) = start(st, task_id, agent_id, bin_override, "manual").await?;
    done.await.map_err(|e| e.to_string())
}

/// A card changed (it landed in a column or was assigned): the agents take their best waiting cards (`pull`), so a card
/// waits its turn by priority. Returns the card's
/// identifier when it was one of the cards started. Boxed because a finishing run pulls the next one (start → finish →
/// pull → start).
pub fn dispatch<'a>(st: &'a AppState, task_id: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + 'a>> {
    Box::pin(dispatch_inner(st, task_id))
}

async fn dispatch_inner(st: &AppState, task_id: &str) -> Option<String> {
    let started = pull(st).await;
    if !started.iter().any(|(t, _)| t == task_id) {
        return None;
    }
    tasks::get(&st.db, task_id).ok().map(|t| t.identifier)
}

/// The queue: each active agent whose pull isn't paused takes its best waiting cards (`workflow::waiting_for`: the cards
/// of the Auto columns it is on and the cards assigned to it in Auto columns, by priority) until it is at its cards at
/// once, the "Runs at once" limit is reached, or nothing waits. Its old wake-up setting changes nothing. Runs when a card
/// lands in a column, whenever a run ends, and once a minute. Returns the cards it started, with their runs' handles.
pub fn pull(st: &AppState) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<(String, tokio::task::JoinHandle<RunSummary>)>> + Send + '_>> {
    Box::pin(pull_inner(st))
}

async fn pull_inner(st: &AppState) -> Vec<(String, tokio::task::JoinHandle<RunSummary>)> {
    if blocking(st, |st| get_settings(st).agents_paused).await.unwrap_or(true) || is_closing(st) {
        return vec![];
    }
    // The lock covers choosing and claiming the cards only (GA-191): the next pull (a run's end, a card that changed, the
    // minute's) claims more while these start, and the claims count against the run limits (`under_way`).
    let claims = {
        let _one = st.runs.pulling.lock().await;
        blocking(st, claim_cards).await.unwrap_or_default()
    };
    // Each claimed card's start (the fetch, its worktree, the copies, the install) in a task of its own: one card's install
    // doesn't hold up the next card. An agent's cards pass their quick checks one after the other, best first, so a start
    // that can't work (a wrong model, no repository) holds the best card and the others go back to waiting, as before.
    let mut checked: HashMap<String, tokio::sync::oneshot::Receiver<()>> = HashMap::new();
    let starts: Vec<_> = claims.into_iter().map(|c| {
        let (tell, told) = tokio::sync::oneshot::channel();
        let before = checked.insert(c.agent.clone(), told);
        let st = st.clone();
        tokio::spawn(async move {
            if let Some(before) = before {
                let _ = before.await;
            }
            // a card this pull started may have failed already: a failing CLI must not run through the queue
            let agent = c.agent.clone();
            if !blocking(&st, move |st| takes_cards(st, &agent)).await.unwrap_or(false) {
                return None;
            }
            let task = c.task.clone();
            start_background(&st, &task, &c.agent, "assigned", None, Some(c.claim), Some(tell)).await.map(|done| (task, done))
        })
    }).collect();
    let mut started = vec![];
    for s in starts {
        if let Ok(Some(s)) = s.await {
            started.push(s);
        }
    }
    started
}

/// A card the queue claimed for an agent's run (`claim_cards`).
struct Claimed {
    task: String,
    agent: String,
    claim: Starting,
}

/// The queue's choice: each active agent whose pull isn't paused takes its best waiting cards nobody works on, until it
/// has its cards at once under way or "Runs at once" is reached, counting the runs under way and the cards claimed so far
/// (`under_way`). Each is claimed for its agent at once, so no other start takes it.
fn claim_cards(st: &AppState) -> Vec<Claimed> {
    let max = get_settings(st).max_concurrent_runs as usize;
    // GA-202: while memory is short no card starts; each start checks again (`start_inner`).
    if crate::run_memory::wait_reason(st).is_some() {
        return vec![];
    }
    let mut claimed = vec![];
    for (_, a) in team::all_agents(&st.db).unwrap_or_default() {
        if a.status != "active" || !takes_cards(st, &a.actor_id) {
            continue;
        }
        let slots = a.max_runs.max(1) as usize;
        while under_way(st, Some(&a.actor_id), None) < slots && under_way(st, None, None) < max {
            let live: HashSet<String> = st.runs.live.lock().unwrap().values().map(|l| l.task_id.clone()).collect();
            let waiting = workflow::waiting_for(&st.db, &a.actor_id).unwrap_or_default();
            // GA-212: a card in Testing waits while its project's Test runs at once is full, the cards claimed so far
            // counted (they are starting); the agent takes its next card.
            let Some((task, claim)) = waiting.into_iter().filter(|t| !live.contains(t) && test_runs_full(st, t).is_none())
                .find_map(|t| Starting::take(st, &t, Some(&a.actor_id)).map(|c| (t, c))) else { break };
            claimed.push(Claimed { task, agent: a.actor_id.clone(), claim });
        }
    }
    claimed
}

/// `f` with the state on a thread that may wait (for the database, /proc, git or a shell), not on the async runtime's
/// threads, which every async command and event also needs (GA-191). None when `f` panicked.
async fn blocking<T: Send + 'static>(st: &AppState, f: impl FnOnce(&AppState) -> T + Send + 'static) -> Option<T> {
    let st = st.clone();
    tokio::task::spawn_blocking(move || f(&st)).await.ok()
}
