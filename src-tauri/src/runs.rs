//! Runs agents for tasks on their coding CLI (Claude Code, Codex, Gemini, others): start (worktree, prompt, process), follow the stream, finish with
//! the gates, stop, heartbeats and on-assign dispatch. Usable without a Tauri app (tests): everything the
//! UI needs to hear goes through `AppState::notify`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gizai_agents::cli::{self as agent_cli, Kind, TaskRun};
use gizai_agents::prepare::{self as prep, Prepare};
use gizai_agents::process::{self, Caps, StopHandle};
use gizai_agents::prompt::{self, BaseInfo, RunLimits, TaskContext};
use gizai_agents::stream::RunEvent;
use gizai_agents::{outcome, worktree};
use gizai_core::model::{Outcome, Project, Task, TaskPatch};
use gizai_core::{comments, ids, projects, runs as core_runs, settings, tasks, team, workflow};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Per-run limits Gizai enforces itself (Claude Code has no turn cap in print mode); Settings → Runs changes them.
pub const DEFAULT_MAX_RUN_MINUTES: u64 = 90;
pub const DEFAULT_MAX_RUN_TOOL_CALLS: u32 = 200;
pub const DEFAULT_MAX_CONCURRENT: u32 = 3;
const BUFFER: usize = 500;

/// Used when an agent has no allowed commands of its own.
pub const DEFAULT_TOOLS: [&str; 16] = [
    "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git add:*)", "Bash(git commit:*)", "Bash(git merge:*)", "Bash(npm:*)", "Bash(npx:*)", "Bash(composer:*)",
    "Bash(php:*)", "Bash(./vendor/bin/*)", "Bash(cargo:*)", "Bash(pytest:*)", "Bash(ls:*)", "Bash(cat:*)", "Bash(rg:*)",
];

/// What the UI hears about.
pub enum Note {
    RowsChanged(&'static str),
    RunsChanged,
    Event { run_id: String, seq: u64, event: RunEvent },
    /// Something happened in a chat turn (text written, a tool called, a message saved).
    Chat { thread_id: String, event: crate::chat::ChatUiEvent },
    /// A chat turn started or ended.
    ChatChanged,
    /// The release check or an update moved on (see `update`).
    UpdateChanged,
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
    /// Cards whose run is starting (their worktree being made and prepared): one start at a time per card.
    starting: std::sync::Arc<Mutex<HashSet<String>>>,
}

impl RunManager {
    /// The CLIs changed: ask for model lists again.
    pub fn forget_models(&self) {
        self.models.lock().unwrap().clear();
    }
}

/// Held while a card's run starts.
struct Starting {
    cards: std::sync::Arc<Mutex<HashSet<String>>>,
    task_id: String,
}

impl Starting {
    fn take(st: &AppState, task_id: &str) -> Option<Starting> {
        let cards = st.runs.starting.clone();
        let taken = cards.lock().unwrap().insert(task_id.to_string());
        taken.then(|| Starting { cards, task_id: task_id.to_string() })
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
    /// The GitHub CLI (gh) that opens and follows pull requests with your GitHub login; None = found when needed.
    #[serde(default)]
    pub gh_bin: Option<String>,
    /// How Open pull request and Push branch reach GitHub: "ssh" (your SSH keys, the default) or "https" (gh's login).
    #[serde(default = "default_push_over")]
    pub push_over: String,
}

fn default_minutes() -> u64 { DEFAULT_MAX_RUN_MINUTES }
fn default_tool_calls() -> u32 { DEFAULT_MAX_RUN_TOOL_CALLS }
fn default_push_over() -> String { "ssh".into() }

pub fn get_settings(st: &AppState) -> Settings {
    Settings {
        claude_bin: settings::get(&st.db, "claude_bin").ok().flatten(),
        data_dir: st.data_dir.display().to_string(),
        max_concurrent_runs: settings::get(&st.db, "max_concurrent_runs").ok().flatten().unwrap_or(DEFAULT_MAX_CONCURRENT),
        agents_paused: settings::get(&st.db, "agents_paused").ok().flatten().unwrap_or(false),
        max_run_usd: settings::get(&st.db, "max_run_usd").ok().flatten(),
        max_run_minutes: settings::get(&st.db, "max_run_minutes").ok().flatten().unwrap_or(DEFAULT_MAX_RUN_MINUTES),
        max_run_tool_calls: settings::get(&st.db, "max_run_tool_calls").ok().flatten().unwrap_or(DEFAULT_MAX_RUN_TOOL_CALLS),
        gh_bin: settings::get(&st.db, "gh_bin").ok().flatten(),
        push_over: crate::github::push_over_name(st),
    }
}

pub fn save_settings(st: &AppState, s: &Settings) -> Result<(), String> {
    if !(1..=20).contains(&s.max_concurrent_runs) {
        return Err("allow between 1 and 20 runs at once".into());
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
    if !crate::github::PUSH_OVER_NAMES.contains(&s.push_over.as_str()) {
        return Err("pushes go over ssh or https".into());
    }
    let bin = s.claude_bin.as_ref().map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
    settings::set(&st.db, "claude_bin", &bin).map_err(|e| e.to_string())?;
    let gh = s.gh_bin.as_ref().map(|b| b.trim().to_string()).filter(|b| !b.is_empty());
    settings::set(&st.db, "gh_bin", &gh).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_concurrent_runs", &s.max_concurrent_runs).map_err(|e| e.to_string())?;
    settings::set(&st.db, "agents_paused", &s.agents_paused).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_run_usd", &s.max_run_usd).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_run_minutes", &s.max_run_minutes).map_err(|e| e.to_string())?;
    settings::set(&st.db, "max_run_tool_calls", &s.max_run_tool_calls).map_err(|e| e.to_string())?;
    crate::github::save_push_over(st, &s.push_over)?;
    Ok(())
}

pub(crate) fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Finds `claude` the way a login shell would, then in the usual install places. Saves what it finds.
pub fn detect_claude(st: &AppState) -> Option<String> {
    let from_shell = std::process::Command::new("bash").args(["-lc", "command -v claude"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|p| !p.is_empty() && executable(Path::new(p)));
    let home = std::env::var("HOME").unwrap_or_default();
    let found = from_shell.or_else(|| {
        [".local/bin/claude", ".claude/local/claude", ".local/share/mise/installs/claude/latest/claude", ".npm-global/bin/claude"]
            .iter().map(|rel| format!("{home}/{rel}"))
            .chain(["/usr/local/bin/claude".to_string(), "/usr/bin/claude".to_string()])
            .find(|p| executable(Path::new(p)))
    });
    if let Some(p) = &found {
        let _ = settings::set(&st.db, "claude_bin", p);
    }
    found
}

/// The saved Claude Code path; detected only when none was ever saved (a saved path that has gone missing is
/// reported, never silently replaced by another binary).
pub(crate) fn claude_bin(st: &AppState, bin_override: Option<String>) -> Result<PathBuf, String> {
    let bin = match bin_override {
        Some(b) => Some(b),
        None => match settings::get::<String>(&st.db, "claude_bin").ok().flatten() {
            Some(saved) => Some(saved),
            None => detect_claude(st),
        },
    };
    match bin {
        Some(b) if executable(Path::new(&b)) => Ok(PathBuf::from(b)),
        _ => Err("Claude Code not found: set its path in Settings".into()),
    }
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

pub fn live(st: &AppState) -> Vec<LiveRun> {
    st.runs.live.lock().unwrap().iter()
        .map(|(id, l)| LiveRun { run_id: id.clone(), task_id: l.task_id.clone(), agent_id: l.agent_id.clone() })
        .collect()
}

/// The run's events: from memory while it is live, else re-read from its log.
pub fn events_for(st: &AppState, run_id: &str) -> Vec<SeqEvent> {
    if let Some(l) = st.runs.live.lock().unwrap().get(run_id) {
        return l.events.clone();
    }
    let Ok(run) = core_runs::get(&st.db, run_id) else { return vec![] };
    let text = std::fs::read_to_string(&run.log_path).unwrap_or_default();
    agent_cli::parse_log(&text).into_iter().enumerate().map(|(i, event)| SeqEvent { seq: i as u64, event }).collect()
}

pub fn stop(st: &AppState, run_id: &str) {
    st.runs.stopped.lock().unwrap().insert(run_id.to_string());
    if let Some(l) = st.runs.live.lock().unwrap().get(run_id) {
        l.stop.stop();
    }
}

/// Which agent works the card when none is given: an assigned agent, else routing.
fn choose_agent(st: &AppState, task_id: &str) -> Result<(String, String), String> {
    let t = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    if t.hold.is_some() {
        return Err(format!("{} is on hold: clear the hold first", t.identifier));
    }
    if let (Some(a), Some("agent")) = (&t.assignee_id, t.assignee_kind.as_deref()) {
        if let Ok(m) = team::agent(&st.db, a) {
            return Ok((m.actor_id, m.role_key));
        }
    }
    workflow::pick_agent(&st.db, task_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("No agent picks {} up: add a routing rule on the Team page, or pick an agent", t.identifier))
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

/// Gizai is quitting: no new runs or chat answers start.
pub fn mark_closing(st: &AppState) {
    st.runs.closing.store(true, Ordering::SeqCst);
}

pub fn is_closing(st: &AppState) -> bool {
    st.runs.closing.load(Ordering::SeqCst)
}

/// The agent a Run without a chosen agent would start (assigned agent first, then routing).
pub fn suggest(st: &AppState, task_id: &str) -> Option<String> {
    choose_agent(st, task_id).ok().map(|(a, _)| a)
}

/// Why a run didn't start: a problem with this card (its project or repository), a card already put on hold for it
/// (preparing its worktree failed), or anything else (Claude Code missing, the run limit, a paused agent, …).
enum StartError {
    Card(String),
    Held(String),
    Other(String),
}

impl From<String> for StartError {
    fn from(s: String) -> Self { StartError::Other(s) }
}

/// Starts a run and returns its id plus a handle that resolves when it has finished and its verdict is applied.
pub async fn start(st: &AppState, task_id: &str, agent_id: Option<String>, bin_override: Option<String>, trigger: &str)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    start_inner(st, task_id, agent_id, bin_override, trigger, None).await
        .map_err(|e| match e { StartError::Card(m) | StartError::Held(m) | StartError::Other(m) => m })
}

/// A session to resume instead of starting one: Continue.
struct Resume {
    session: String,
    /// Why the run being continued stopped, told to the agent.
    reason: String,
}

/// Continue: resumes a stopped run's Claude Code session in its worktree, as a new run of the same agent. Only the
/// card's latest run continues, and only one that stopped part-way. A hold on the card is cleared: you asked for
/// the work (clearing also resets its failure count).
pub async fn continue_run(st: &AppState, run_id: &str, bin_override: Option<String>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    let run = core_runs::get(&st.db, run_id).map_err(|e| e.to_string())?;
    let task_id = run.task_id.clone().ok_or("only a card's run can continue")?;
    let latest = core_runs::list_for_task(&st.db, &task_id).map_err(|e| e.to_string())?.into_iter().next();
    if latest.as_ref().map(|r| r.id.as_str()) != Some(run_id) {
        return Err("only the card's latest run can continue".into());
    }
    let stopped = matches!(run.status.as_str(), "timed_out" | "failed" | "cancelled")
        || (run.status == "succeeded" && run.outcome.as_deref() == Some("no_result"));
    if !stopped {
        return Err("this run finished; Run starts a new one".into());
    }
    let cli = crate::clis::of_agent(st, run.adapter.as_deref())?;
    if !Kind::parse(&cli.kind).is_some_and(Kind::can_resume) {
        return Err(format!("{} can't continue a run: Run starts the card fresh", cli.name));
    }
    let session = run.session_id.clone().filter(|s| !s.is_empty()).ok_or_else(|| format!("this run has no {} session to continue", cli.name))?;
    if !run.worktree_path.as_deref().is_some_and(|p| Path::new(p).is_dir()) {
        return Err("its worktree is gone; Run starts the card fresh".into());
    }
    let reason = run.error.clone().filter(|e| !e.trim().is_empty()).unwrap_or_else(|| "it ended without a result".into());
    let started = start_inner(st, &task_id, Some(run.agent_id.clone()), bin_override, "manual", Some(Resume { session, reason })).await
        .map_err(|e| match e { StartError::Card(m) | StartError::Held(m) | StartError::Other(m) => m })?;
    if tasks::get(&st.db, &task_id).is_ok_and(|t| t.hold.is_some()) {
        let patch = gizai_core::model::TaskPatch { hold: Some(String::new()), ..Default::default() };
        if tasks::update(&st.db, &st.you_id, &task_id, patch).is_ok() {
            (st.notify)(Note::RowsChanged("tasks"));
        }
    }
    Ok(started)
}

/// A start nobody is watching (heartbeat, on-assign): when the card itself is the problem, put it on hold
/// "blocked" with the reason, so Jeffrey sees why and the agent moves on to its next card.
async fn start_background(st: &AppState, task_id: &str, agent_id: &str, trigger: &str) -> Option<tokio::task::JoinHandle<RunSummary>> {
    match start_inner(st, task_id, Some(agent_id.to_string()), None, trigger, None).await {
        Ok((_, done)) => Some(done),
        Err(StartError::Card(reason)) => {
            hold_card(st, agent_id, task_id, &reason);
            None
        }
        Err(StartError::Held(_)) => None,
        Err(StartError::Other(reason)) => { eprintln!("gizai: {trigger} start of an agent failed: {reason}"); None }
    }
}

/// Puts the card on hold "blocked" with the reason, as `actor` (the agent that couldn't start), so Jeffrey sees why.
fn hold_card(st: &AppState, actor: &str, task_id: &str, reason: &str) {
    let patch = TaskPatch { hold: Some("blocked".into()), hold_reason: Some(reason.to_string()), ..Default::default() };
    if tasks::update(&st.db, actor, task_id, patch).is_ok() {
        (st.notify)(Note::RowsChanged("tasks"));
    }
}

/// The card's worktree: its own when it has one; else a finished card's worktree of the same project, taken over so
/// its build stays warm (`worktree::reuse`, see `worktrees::reusable`); else a new one. A new or taken-over worktree is
/// noted as still to prepare.
fn open_worktree(st: &AppState, project: &Project, task: &Task, repo: &Path, start: &str) -> Result<worktree::Worktree, StartError> {
    let base_dir = st.data_dir.join("worktrees").join(&project.key);
    let has_own = base_dir.join(&task.identifier).exists()
        || worktree::worktree_of(repo, &worktree::branch_name(&task.identifier, &task.title)).ok().flatten().is_some();
    let mut wt = None;
    if !has_own {
        for from in crate::worktrees::reusable(st, &project.id, &base_dir) {
            match worktree::reuse(repo, &from, &base_dir, &task.identifier, &task.title, start, &crate::worktrees::keep_on_reuse(project)) {
                Ok(w) => {
                    wt = Some(w);
                    break;
                }
                Err(e) => eprintln!("gizai: {} couldn't take over the worktree {}: {e}", task.identifier, from.display()),
            }
        }
    }
    let wt = match wt {
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
/// with its output; no run starts, so it doesn't count as a failed run, and the next start prepares again.
async fn prepare_worktree(st: &AppState, agent_id: &str, task: &Task, project: &Project, repo: &Path, wt: &worktree::Worktree,
                          todo: worktree::Unprepared) -> Result<(), StartError> {
    let plan = Prepare { copy: project.worktree_copy.clone(), install: project.worktree_install, setup: project.worktree_setup.clone() };
    let (main, dir) = (repo.to_path_buf(), wt.path.clone());
    let done = tokio::task::spawn_blocking(move || {
        let path = command_path();
        prep::prepare(&main, &dir, &plan, todo.since.as_deref(), Some(path.as_os_str()))
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
            hold_card(st, agent_id, &task.id, &reason);
            Err(StartError::Held(reason))
        }
    }
}

/// The PATH for the commands that prepare a worktree: Gizai's own, then the folders your login shell adds (started
/// from the app launcher, Gizai often lacks ~/.local/bin, mise or nvm).
pub(crate) fn command_path() -> std::ffi::OsString {
    let own = std::env::var_os("PATH").unwrap_or_default();
    let login = std::process::Command::new("bash").args(["-lc", "printf '\\n%s' \"$PATH\""]).stdin(std::process::Stdio::null()).output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8_lossy(&o.stdout).lines().last().map(str::to_string))
        .unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&own).collect();
    for d in std::env::split_paths(&login) {
        if !d.as_os_str().is_empty() && !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    std::env::join_paths(dirs).unwrap_or(own)
}

async fn start_inner(st: &AppState, task_id: &str, agent_id: Option<String>, bin_override: Option<String>, trigger: &str, resume: Option<Resume>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), StartError> {
    if st.runs.closing.load(Ordering::SeqCst) {
        return Err(StartError::Other("Gizai is quitting".into()));
    }
    let _starting = Starting::take(st, task_id)
        .ok_or_else(|| StartError::Other("This card's run is already starting: Gizai is getting its worktree ready".into()))?;
    let max = get_settings(st).max_concurrent_runs;
    if st.runs.live.lock().unwrap().len() as u32 >= max {
        return Err(format!("{max} runs are already active; wait for one to finish or raise the limit in Settings").into());
    }
    let (agent_id, role) = match agent_id {
        Some(a) => { let m = team::agent(&st.db, &a).map_err(|e| e.to_string())?; (m.actor_id, m.role_key) }
        None => choose_agent(st, task_id)?,
    };
    let agent = team::agent(&st.db, &agent_id).map_err(|e| e.to_string())?;
    if agent.status != "active" {
        return Err(format!("{} is paused", agent.name).into());
    }
    if let Some(budget) = agent.budget_usd_micros {
        let spent = core_runs::agent_spend_since(&st.db, &agent_id, core_runs::month_start_ms(ids::now_ms())).unwrap_or(0);
        if spent >= budget {
            return Err(format!("{} has used its monthly budget (${:.2} of ${:.2}); raise it on the Team page",
                               agent.name, spent as f64 / 1e6, budget as f64 / 1e6).into());
        }
    }
    let cli = crate::clis::of_agent(st, agent.adapter.as_deref())?;
    let spec = crate::clis::spec(st, &cli, bin_override)?;
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let project = projects::get(&st.db, task.project_id.as_deref().unwrap_or_default()).map_err(|e| StartError::Card(e.to_string()))?;
    let repo = project.repo_path.clone().filter(|p| !p.trim().is_empty())
        .ok_or_else(|| StartError::Card(format!("Link a git repository to {} first (project page → Edit)", project.name)))?;
    // With a GitHub link, a card starts from the main branch just fetched from GitHub, and a card that already has
    // a branch hears how far that main has moved on.
    let start = match project.repo_url.clone() {
        Some(url) => {
            let (dir, branch) = (PathBuf::from(&repo), project.default_branch.clone());
            tokio::task::spawn_blocking(move || {
                let remote = crate::git::remote_for(&dir, &url);
                worktree::fetch_start(&dir, remote.as_deref(), &url, &branch)
            }).await.map_err(|e| StartError::Other(e.to_string()))?.map_err(|e| StartError::Card(e.to_string()))?
        }
        None => project.default_branch.clone(),
    };
    let wt = open_worktree(st, &project, &task, Path::new(&repo), &start)?;
    if let Some(todo) = worktree::unprepared(&wt.path) {
        prepare_worktree(st, &agent_id, &task, &project, Path::new(&repo), &wt, todo).await?;
    }
    let base = project.repo_url.as_ref().map(|_| BaseInfo {
        from: start.strip_prefix("refs/remotes/").or_else(|| start.strip_prefix("refs/")).unwrap_or(&start).to_string(),
        behind: if wt.created { 0 } else { worktree::behind(&wt.path, &start).unwrap_or(0) },
    });

    let session = resume.as_ref().map(|r| r.session.clone()).unwrap_or_else(ids::new_id);
    let log_dir = st.data_dir.join("runs");
    std::fs::create_dir_all(&log_dir).map_err(|e| StartError::Other(e.to_string()))?;
    // A continued session gets a log of its own: each run shows only its own output.
    let log_path = match &resume {
        Some(_) => { let tag = ids::new_id(); log_dir.join(format!("{session}-{}.jsonl", &tag[tag.len() - 8..])) }
        None => log_dir.join(format!("{session}.jsonl")),
    };
    let wt_path = wt.path.to_string_lossy().to_string();
    let db_trigger = if resume.is_some() { "nudge" } else if trigger == "heartbeat" { "routed" } else { trigger };
    let run_id = core_runs::create_with_trigger(&st.db, &agent_id, task_id, &role, db_trigger, &session, &wt_path, &wt_path, &wt.branch,
                                                &log_path.to_string_lossy()).map_err(|e| StartError::Other(e.to_string()))?;

    if let Ok(sha) = worktree::rev_parse(&wt.path, "HEAD") {
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
    let run = TaskRun {
        session_id: session.clone(), resume: resume.is_some(),
        prompt: match &resume { Some(r) => prompt::continue_prompt(&r.reason, Some(limits)), None => prompt::build(&ctx, &instructions) },
        permission_mode: agent.permission_mode.clone().unwrap_or_default(),
        allowed_tools: if agent.allowed_tools.is_empty() { DEFAULT_TOOLS.iter().map(|s| s.to_string()).collect() } else { agent.allowed_tools.clone() },
        model: agent.model.clone(), max_budget_usd: get_settings(st).max_run_usd, effort: agent.effort.clone(),
        // A worktree's commits go to the repository's git folder, outside the worktree: Codex's sandbox must be able to write it.
        writable_dirs: if spec.kind == Kind::Codex { git_common_dir(&wt.path).into_iter().collect() } else { vec![] },
    };
    let exec = agent_cli::task_exec(&spec, &run);
    if spec.kind != Kind::ClaudeCode {
        // The log says which CLI wrote it, so it can be read again after the run.
        if let Err(e) = std::fs::write(&log_path, format!("{}\n", agent_cli::log_header(spec.kind))) {
            let _ = core_runs::finish(&st.db, &run_id, "failed", None, 0, 0, 0, Some(&e.to_string()));
            return Err(StartError::Other(e.to_string()));
        }
    }
    let mut handle = match process::spawn_exec::<RunEvent, _>(&exec, &wt.path, &log_path,
        Caps { max_time: std::time::Duration::from_secs(limits.minutes * 60), max_tool_calls: limits.tool_calls }, agent_cli::Parser::new(spec.kind)) {
        Ok(h) => h,
        Err(e) => {
            let msg = e.to_string();
            let _ = core_runs::finish(&st.db, &run_id, "failed", None, 0, 0, 0, Some(&msg));
            (st.notify)(Note::RowsChanged("runs"));
            return Err(StartError::Other(msg));
        }
    };
    let _ = core_runs::set_running(&st.db, &run_id, handle.pid);
    st.runs.live.lock().unwrap().insert(run_id.clone(), Live {
        task_id: task_id.to_string(), agent_id: agent_id.clone(), stop: handle.stop.clone(), events: vec![], next_seq: 0,
    });
    (st.notify)(Note::RunsChanged);
    (st.notify)(Note::RowsChanged("runs"));

    let st2 = st.clone();
    let rid = run_id.clone();
    let tid = task_id.to_string();
    let cli_name = cli.name.clone();
    let done = tokio::spawn(async move {
        let mut result: Option<RunEvent> = None;
        let mut capped: Option<String> = None;
        let mut exit = String::new();
        while let Some(ev) = handle.events.recv().await {
            match &ev {
                // Codex names its session itself: Continue resumes that one.
                RunEvent::Init { session_id, .. } if !session_id.is_empty() && *session_id != session => {
                    let _ = core_runs::set_session(&st2.db, &rid, session_id);
                }
                RunEvent::Result { .. } => result = Some(ev.clone()),
                RunEvent::Other { raw_type } if raw_type.starts_with("cap_exceeded") && capped.is_none() => capped = Some(raw_type.clone()),
                RunEvent::Other { raw_type } if raw_type.starts_with("exit:") => exit = raw_type.clone(),
                _ => {}
            }
            let seq = {
                let mut live = st2.runs.live.lock().unwrap();
                let Some(l) = live.get_mut(&rid) else { continue };
                let seq = l.next_seq;
                l.next_seq += 1;
                l.events.push(SeqEvent { seq, event: ev.clone() });
                if l.events.len() > BUFFER { l.events.remove(0); }
                seq
            };
            (st2.notify)(Note::Event { run_id: rid.clone(), seq, event: ev });
        }
        let capped = capped.map(|c| if c.ends_with(":time") {
            format!("stopped at the time limit of {} minutes per run (Settings → Runs)", limits.minutes)
        } else {
            format!("stopped at the limit of {} tool calls per run (Settings → Runs)", limits.tool_calls)
        });
        finish_run(&st2, &rid, &tid, result, capped, &exit, &cli_name).await
    });
    Ok((run_id, done))
}

/// The repository's shared git folder for a worktree (where its commits go), as an absolute path.
fn git_common_dir(wt: &Path) -> Option<String> {
    let out = std::process::Command::new("git").args(["rev-parse", "--path-format=absolute", "--git-common-dir"]).current_dir(wt)
        .stdin(std::process::Stdio::null()).output().ok().filter(|o| o.status.success())?;
    let dir = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!dir.is_empty()).then_some(dir)
}

/// `capped`: why Gizai stopped the run at a limit, if it did. `cli`: the CLI's name, for the reason a run failed.
async fn finish_run(st: &AppState, run_id: &str, task_id: &str, result: Option<RunEvent>, capped: Option<String>, exit: &str, cli: &str) -> RunSummary {
    let cancelled = st.runs.stopped.lock().unwrap().remove(run_id);
    let (cost, input, output, text, ok) = match &result {
        Some(RunEvent::Result { cost_usd, input_tokens, output_tokens, text, is_error, .. }) =>
            ((cost_usd.unwrap_or(0.0) * 1_000_000.0).round() as i64, *input_tokens, *output_tokens, text.clone(), !is_error),
        _ => (0, 0, 0, String::new(), false),
    };
    let verdict: Option<Outcome> = outcome::parse(&text).map(|o| Outcome { outcome: o.outcome, summary: o.summary, issues: o.issues });
    let status = if cancelled { "cancelled" } else if capped.is_some() { "timed_out" } else if ok && exit == "exit:0" { "succeeded" } else { "failed" };
    let error = match status {
        "cancelled" => Some("stopped".to_string()),
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
    let _ = core_runs::finish(&st.db, run_id, status, verdict.as_ref(), cost, input, output, error.as_deref());
    let mut moved = false;
    if !cancelled {
        match workflow::apply_outcome(&st.db, run_id, if status == "succeeded" { verdict.as_ref() } else { None }) {
            Ok(g) => moved = g.moved_to.is_some(),
            Err(e) => eprintln!("gizai: applying the outcome of run {run_id} failed: {e}"),
        }
    }
    st.runs.live.lock().unwrap().remove(run_id);
    (st.notify)(Note::RowsChanged("tasks"));
    (st.notify)(Note::RowsChanged("comments"));
    (st.notify)(Note::RunsChanged);
    // The card may now belong to another agent (Testing → QA). Only when the gate moved it: a card that
    // stayed put would otherwise restart the same agent straight away.
    if moved {
        let st2 = st.clone();
        let tid = task_id.to_string();
        tokio::spawn(async move { dispatch(&st2, &tid).await; });
        // In Review: a pull request the agent opened shows on the card at once, not at the next PR check.
        if tasks::get(&st.db, task_id).is_ok_and(|t| t.state_category == "review") {
            crate::pulls::check_soon(st, task_id);
        }
    }
    RunSummary { run_id: run_id.to_string(), status: status.into(), outcome: verdict.map(|v| v.outcome), cost_usd_micros: cost, error }
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

/// On-assign wake-up: if the card now belongs to an agent that wakes up when assigned, start it.
/// Boxed because a finishing run dispatches the next one (start → finish → dispatch → start).
pub fn dispatch<'a>(st: &'a AppState, task_id: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + 'a>> {
    Box::pin(dispatch_inner(st, task_id))
}

async fn dispatch_inner(st: &AppState, task_id: &str) -> Option<String> {
    if get_settings(st).agents_paused {
        return None;
    }
    let t = tasks::get(&st.db, task_id).ok()?;
    if t.hold.is_some() || !matches!(t.state_category.as_str(), "ready" | "in_progress" | "testing") {
        return None;
    }
    let (agent_id, _) = choose_agent(st, task_id).ok()?;
    let agent = team::agent(&st.db, &agent_id).ok()?;
    if agent.wakeup.as_deref() != Some("on_assign") || agent.status != "active" {
        return None;
    }
    {
        let live = st.runs.live.lock().unwrap();
        let mine = live.values().filter(|l| l.agent_id == agent_id).count() as i64;
        if live.values().any(|l| l.task_id == task_id) || mine >= agent.max_runs.max(1) {
            return None;
        }
    }
    start_background(st, task_id, &agent_id, "assigned").await.map(|_| t.identifier.clone())
}

/// One heartbeat round: every active heartbeat agent whose interval has passed wakes up, and starts its
/// next cards (up to its cards-at-once) if it has them and the run limit allows. Returns the started runs' completion handles.
pub async fn heartbeat_tick(st: &AppState, now: i64) -> Vec<tokio::task::JoinHandle<RunSummary>> {
    let mut started = vec![];
    if get_settings(st).agents_paused {
        return started;
    }
    let mut woke = false;
    for (_, a) in team::all_agents(&st.db).unwrap_or_default() {
        if a.status != "active" || a.wakeup.as_deref() != Some("heartbeat") {
            continue;
        }
        let every = a.heartbeat_minutes.unwrap_or(0).max(1) * 60_000;
        if matches!(a.last_heartbeat_at, Some(last) if now - last < every) {
            continue;
        }
        let _ = team::touch_heartbeat(&st.db, &a.actor_id, now);
        woke = true;
        // Up to the agent's cards-at-once, each on its own card (next_task_for skips claimed cards).
        let busy = st.runs.live.lock().unwrap().values().filter(|l| l.agent_id == a.actor_id).count() as i64;
        for _ in busy..a.max_runs.max(1) {
            let Ok(Some(task)) = workflow::next_task_for(&st.db, &a.actor_id) else { break };
            match start_background(st, &task, &a.actor_id, "heartbeat").await {
                Some(done) => started.push(done),
                None => break,
            }
        }
    }
    if woke {
        (st.notify)(Note::RowsChanged("actors"));
    }
    started
}
