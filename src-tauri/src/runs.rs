//! Runs agents for tasks on their coding CLI (Claude Code, Codex, Gemini, others): start (worktree, prompt, process), follow the stream, finish with
//! the gates, stop, and the queue that starts the cards of Auto columns. Usable without a Tauri app (tests): everything the
//! UI needs to hear goes through `AppState::notify`.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gizai_agents::cli::{self as agent_cli, Kind, RunFolder, TaskRun};
use gizai_agents::prepare::{self as prep, Prepare};
use gizai_agents::process::{self, Caps, StopHandle};
use gizai_agents::prompt::{self, BaseInfo, RunLimits, TaskContext};
use gizai_agents::stream::RunEvent;
use gizai_agents::{outcome, worktree};
use gizai_core::model::{Outcome, Project, Refusal, Task, TaskPatch};
use gizai_core::{comments, ids, projects, runs as core_runs, settings, tasks, team, workflow};
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Per-run limits Gizai enforces itself (Claude Code has no turn cap in print mode); Settings → Runs changes them.
pub const DEFAULT_MAX_RUN_MINUTES: u64 = 90;
pub const DEFAULT_MAX_RUN_TOOL_CALLS: u32 = 200;
pub const DEFAULT_MAX_CONCURRENT: u32 = 3;
const BUFFER: usize = 500;
/// Why a run or chat answer ended when Gizai quit (logging out and SIGTERM included), and the note a chat shows.
pub const STOPPED_BY_QUIT: &str = "Stopped because Gizai quit.";

/// Used when an agent has no allowed commands of its own; new agents start with the same list (`src/lib/agents.ts`).
/// The read-only helpers near the end are the ones agents use in pipes; `sleep` lets an agent wait in the foreground
/// (for CI, a release or a deploy) between checks, as "How this run works" tells it.
pub const DEFAULT_TOOLS: [&str; 29] = [
    "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git add:*)", "Bash(git commit:*)", "Bash(git merge:*)", "Bash(npm:*)", "Bash(npx:*)", "Bash(composer:*)",
    "Bash(php:*)", "Bash(./vendor/bin/*)", "Bash(cargo:*)", "Bash(pytest:*)", "Bash(ls:*)", "Bash(cat:*)", "Bash(rg:*)",
    "Bash(head:*)", "Bash(tail:*)", "Bash(wc:*)", "Bash(sort:*)", "Bash(uniq:*)", "Bash(cut:*)", "Bash(diff:*)", "Bash(grep:*)", "Bash(jq:*)",
    "Bash(pwd:*)", "Bash(which:*)", "Bash(tree:*)", "Bash(sleep:*)",
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
    /// Agents that stopped taking cards from the queue (`pull`) after a start that couldn't work or a run
    /// that failed, with why (the first failure's). It is asked before every start, and only the failure that paused the
    /// agent holds its card: a card started just before goes back to waiting when it fails the same way, so one missing
    /// login holds one card, not the whole queue. The pause ends when a person starts the agent (Run or Continue), edits
    /// it or sets it active on the Team page, or when Gizai starts again.
    pull_paused: Mutex<HashMap<String, String>>,
    /// One pull of the queue at a time: two runs ending together don't give an agent more cards than it may work on.
    pulling: tokio::sync::Mutex<()>,
}

impl RunManager {
    /// The CLIs changed: ask for model lists again.
    pub fn forget_models(&self) {
        self.models.lock().unwrap().clear();
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

/// Whether the queue may start another card for the agent now: Gizai isn't quitting, agents aren't
/// paused in Settings and the agent's pull isn't paused. Asked again before each start.
fn takes_cards(st: &AppState, agent_id: &str) -> bool {
    !is_closing(st) && !settings::get::<bool>(&st.db, "agents_paused").ok().flatten().unwrap_or(false) && pull_paused(st, agent_id).is_none()
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

/// An agent works on the card now: a live run, or a run still getting its worktree ready.
pub fn working_on(st: &AppState, task_id: &str) -> bool {
    st.runs.starting.lock().unwrap().contains(task_id) || st.runs.live.lock().unwrap().values().any(|l| l.task_id == task_id)
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
    live.len()
}

/// Gizai is quitting: no new runs or chat answers start, and those that end from now on were stopped because Gizai
/// quit.
pub fn mark_closing(st: &AppState) {
    st.runs.closing.store(true, Ordering::SeqCst);
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

/// Starts a run and returns its id plus a handle that resolves when it has finished and its verdict is applied.
pub async fn start(st: &AppState, task_id: &str, agent_id: Option<String>, bin_override: Option<String>, trigger: &str)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    let (run_id, done) = start_inner(st, task_id, agent_id, bin_override, trigger, None).await.map_err(StartError::message)?;
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
}

/// Continue: resumes a stopped run's session in its worktree, as a new run of the same agent on the same CLI. Only the
/// card's latest run continues, and only one that stopped part-way. A hold on the card is cleared: you asked for
/// the work (clearing also resets its failure count).
pub async fn continue_run(st: &AppState, run_id: &str, bin_override: Option<String>)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    continue_inner(st, run_id, bin_override, false).await
}

/// The Team Lead's Continue after an answer (`continue_agent_run`): also a run that ended asking for a decision
/// (`needs_decision`) resumes, told what was written on the card since it ended.
pub async fn continue_answered(st: &AppState, run_id: &str) -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
    continue_inner(st, run_id, None, true).await
}

async fn continue_inner(st: &AppState, run_id: &str, bin_override: Option<String>, answered: bool)
    -> Result<(String, tokio::task::JoinHandle<RunSummary>), String> {
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
        let lines: Vec<String> = comments::list(&st.db, &task_id).unwrap_or_default().into_iter()
            .filter(|c| c.created_at > since && c.run_id.as_deref() != Some(run_id))
            .map(|c| format!("{}: {}", c.author_name, c.body_md.trim()))
            .collect();
        if lines.is_empty() {
            return Err("nobody has answered on the card since this run asked for a decision".into());
        }
        Some(lines[lines.len().saturating_sub(5)..].join("\n\n"))
    } else {
        None
    };
    let cli = crate::clis::of_agent(st, run.adapter.as_deref())?;
    if !Kind::parse(&cli.kind).is_some_and(Kind::can_resume) {
        return Err(format!("{} can't continue a run: Run starts the card fresh", cli.name));
    }
    let session = run.session_id.clone().filter(|s| !s.is_empty()).ok_or_else(|| format!("this run has no {} session to continue", cli.name))?;
    if !run.worktree_path.as_deref().is_some_and(|p| Path::new(p).is_dir()) {
        return Err("its worktree is gone; Run starts the card fresh".into());
    }
    let reason = run.error.clone().filter(|e| !e.trim().is_empty()).unwrap_or_else(|| "it ended without a result".into());
    let started = start_inner(st, &task_id, Some(run.agent_id.clone()), bin_override, "manual", Some(Resume { session, cli, reason, answer, nudge: false })).await
        .map_err(StartError::message)?;
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
async fn start_background(st: &AppState, task_id: &str, agent_id: &str, trigger: &str, resume: Option<Resume>)
    -> Option<tokio::task::JoinHandle<RunSummary>> {
    match start_inner(st, task_id, Some(agent_id.to_string()), None, trigger, resume).await {
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
    // Not while the project's own folder is being updated from chat (`code::start_update`), and the other way round:
    // no card copies a half-installed node_modules/.
    let folder = crate::code::folder_lock(st, repo);
    let _folder = folder.lock().await;
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
        return Err(StartError::Wait("Gizai is quitting".into()));
    }
    let _starting = Starting::take(st, task_id)
        .ok_or_else(|| StartError::Wait("This card's run is already starting: Gizai is getting its worktree ready".into()))?;
    if let Ok(t) = tasks::get(&st.db, task_id) && t.archived_at.is_some() {
        return Err(StartError::Other(format!("{} is archived: restore it first", t.identifier)));
    }
    let max = get_settings(st).max_concurrent_runs;
    if st.runs.live.lock().unwrap().len() as u32 >= max {
        return Err(StartError::Wait(format!("{max} runs are already active; wait for one to finish or raise the limit in Settings")));
    }
    let (agent_id, role) = match agent_id {
        Some(a) => { let m = team::agent(&st.db, &a).map_err(|e| e.to_string())?; (m.actor_id, m.role_key) }
        None => choose_agent(st, task_id)?,
    };
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
    let spec = crate::clis::spec(st, &cli, bin_override.clone()).map_err(StartError::Card)?;
    if let Some(model) = agent.model.as_deref().map(str::trim).filter(|m| !m.is_empty() && *m != "default") {
        check_model(st, &cli, &spec, bin_override.is_some(), model, &agent.name).await?;
    }
    let task = tasks::get(&st.db, task_id).map_err(|e| e.to_string())?;
    let project = projects::get(&st.db, task.project_id.as_deref().unwrap_or_default()).map_err(|e| StartError::Card(e.to_string()))?;
    let repo = project.repo_path.clone().filter(|p| !p.trim().is_empty())
        .ok_or_else(|| StartError::Card(format!("Link a git repository to {} first (project page → Edit)", project.name)))?;
    // With a GitHub link, a card starts from the main branch just fetched from GitHub, and a card that already has
    // a branch hears how far that main has moved on.
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
    let queued = matches!(trigger, "assigned" | "nudge");
    let wait_if_paused = || match pull_paused(st, &agent_id) {
        Some(why) if queued => Err(StartError::Wait(format!("{} stopped taking cards: {why}", agent.name))),
        _ => Ok(()),
    };
    wait_if_paused()?;
    let wt = open_worktree(st, &project, &task, Path::new(&repo), &start)?;
    if let Some(todo) = worktree::unprepared(&wt.path) {
        prepare_worktree(st, &agent_id, &task, &project, Path::new(&repo), &wt, todo).await?;
    }
    let base = project.repo_url.as_ref().map(|_| BaseInfo {
        from: start.strip_prefix("refs/remotes/").or_else(|| start.strip_prefix("refs/")).unwrap_or(&start).to_string(),
        behind: if wt.created { 0 } else { worktree::behind(&wt.path, &start).unwrap_or(0) },
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
    let db_trigger = if resume.is_some() { "nudge" } else { trigger };
    let log = log_path.to_string_lossy();
    let run_id = if resume.as_ref().is_some_and(|r| r.nudge) {
        core_runs::create_nudge(&st.db, &agent_id, task_id, &role, &session, &wt_path, &wt_path, &wt.branch, &log)
    } else {
        core_runs::create_with_trigger(&st.db, &agent_id, task_id, &role, db_trigger, &session, &wt_path, &wt_path, &wt.branch, &log)
    }.map_err(|e| StartError::Other(e.to_string()))?;

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
    // The agent's folders (agent form → Folders): one that is missing, or refused by now, is skipped and the run log
    // says so, as it says which ones this CLI can't be given.
    let (folders, mut notes) = gizai_core::folders::for_run(&agent.folders, &gizai_core::folders::Places::of(&st.db));
    let folders: Vec<RunFolder> = folders.into_iter().map(|f| RunFolder { change: f.change(), path: f.path }).collect();
    notes.extend(agent_cli::folders_left_out(spec.kind, &folders));
    // The run's own temp folder in its worktree, out of git status and empty: every CLI gets it as TMPDIR, TMP and TEMP.
    // Without it the run still starts, and its log says why.
    let temp_dir = match worktree::prepare_temp(&wt.path) {
        Ok(d) => Some(d.to_string_lossy().to_string()),
        Err(e) => {
            notes.push(format!("Couldn't make this run's temp folder {}: {e}. TMPDIR, TMP and TEMP stay as they were.",
                               wt.path.join(worktree::TEMP_DIR).display()));
            None
        }
    };
    let allowed_tools: Vec<String> =
        if agent.allowed_tools.is_empty() { DEFAULT_TOOLS.iter().map(|s| s.to_string()).collect() } else { agent.allowed_tools.clone() };
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
    if agent.tools.servers_on().next().is_some() {
        if spec.kind == Kind::ClaudeCode {
            let (st2, a2, cap) = (st.clone(), agent.clone(), std::time::Duration::from_secs(limits.minutes * 60));
            let (servers, left_out) = tokio::task::spawn_blocking(move || crate::mcp_servers::for_run(&st2, &a2, cap)).await.unwrap_or_default();
            notes.extend(left_out);
            if !servers.is_empty() {
                let path = log_dir.join(format!("{run_id}.mcp.json"));
                match gizai_agents::mcp_run::write_config(&path, &gizai_agents::mcp_run::config(vec![], &servers)) {
                    Ok(()) => {
                        let (allow, refuse) = gizai_agents::mcp_run::permissions(&servers);
                        allowed_tools.extend(allow);
                        mcp_refused = refuse;
                        mcp_config = Some(path);
                    }
                    Err(e) => notes.push(format!("Couldn't write this run's MCP config, so it goes without its MCP servers: {e}")),
                }
            }
        } else {
            notes.push(format!("{} runs on {}: MCP servers work on Claude Code for now, so this run goes without them.", agent.name, cli.name));
        }
    }
    let base_prompt = prompt::with_rules(&match &resume {
        Some(Resume { nudge: true, .. }) => prompt::nudge_prompt(Some(limits)),
        Some(Resume { answer: Some(a), .. }) => prompt::answered_prompt(a, Some(limits)),
        Some(r) => prompt::continue_prompt(&r.reason, Some(limits)),
        None => prompt::build(&ctx, &instructions),
    }, &rules);
    let run = TaskRun {
        session_id: session.clone(), resume: resume.is_some(),
        prompt: if mcp_config.is_some() { format!("{base_prompt}\n\n{}", gizai_agents::mcp_run::UNTRUSTED) } else { base_prompt },
        permission_mode, allowed_tools, mcp_config: mcp_config.clone(), disallowed_tools: mcp_refused,
        model: agent.model.clone(), max_budget_usd: get_settings(st).max_run_usd, effort: agent.effort.clone(),
        // A worktree's commits go to the repository's git folder, outside the worktree: Codex's sandbox must be able to write it.
        writable_dirs: if spec.kind == Kind::Codex { git_common_dir(&wt.path).into_iter().collect() } else { vec![] },
        folders, temp_dir,
    };
    let exec = agent_cli::task_exec(&spec, &run);
    // The log says which CLI wrote it, so it can be read again after the run (Claude Code's needs no header), then
    // Gizai's notes.
    let head: String = (spec.kind != Kind::ClaudeCode).then(|| agent_cli::log_header(spec.kind)).into_iter()
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
    let mut handle = match process::spawn_exec::<RunEvent, _>(&exec, &wt.path, &log_path,
        Caps { max_time: std::time::Duration::from_secs(limits.minutes * 60), max_tool_calls: limits.tool_calls }, agent_cli::Parser::new(spec.kind)) {
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
    let dir = wt.path.clone();
    let (run_agent, run_config) = (agent_id.clone(), mcp_config.clone());
    let done = tokio::spawn(async move {
        let mut result: Option<RunEvent> = None;
        let mut capped: Option<String> = None;
        let mut exit = String::new();
        let mut tools = 0usize;
        let mut refused: Vec<Refusal> = vec![];
        while let Some(ev) = handle.events.recv().await {
            match &ev {
                RunEvent::ToolUse { .. } => tools += 1,
                RunEvent::Refused { tool, input, reason } => refused.push(Refusal { tool: tool.clone(), input: input.clone(), reason: reason.clone() }),
                // Codex names its session itself: Continue resumes that one.
                RunEvent::Init { session_id, .. } if !session_id.is_empty() && *session_id != session => {
                    let _ = core_runs::set_session(&st2.db, &rid, session_id);
                }
                RunEvent::Result { .. } => result = Some(ev.clone()),
                RunEvent::McpServers { servers } => crate::mcp_servers::record_states(&st2, &run_agent, servers),
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
        // The run's process group has ended, whatever way (finished, Stop, a limit): its throwaway files go, its MCP
        // config (with its servers' secrets) first.
        if let Some(p) = &run_config {
            let _ = std::fs::remove_file(p);
        }
        if let Err(e) = worktree::empty_temp(&dir) {
            eprintln!("gizai: couldn't empty the temp folder of run {rid}: {e}");
        }
        finish_run(&st2, &rid, &tid, &dir, Ran { result, capped, exit, tools, moved_from, queued, refused }, &cli_name).await
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
struct Ran {
    result: Option<RunEvent>,
    capped: Option<String>,
    exit: String,
    tools: usize,
    moved_from: Option<String>,
    queued: bool,
    refused: Vec<Refusal>,
}

/// Claude Code's own words for a missing or expired login.
fn login_problem(error: &str) -> bool {
    let e = error.to_lowercase();
    ["/login", "not logged in", "invalid api key", "oauth token has expired", "authentication_error", "please log in"].iter().any(|m| e.contains(m))
}

/// `dir`: the run's worktree. `cli`: the CLI's name, for the reason a run failed.
async fn finish_run(st: &AppState, run_id: &str, task_id: &str, dir: &Path, ran: Ran, cli: &str) -> RunSummary {
    let Ran { result, capped, exit, tools, moved_from, queued, refused } = ran;
    let exit = exit.as_str();
    let stopped = st.runs.stopped.lock().unwrap().remove(run_id);
    let (cost, input, output, text, ok) = match &result {
        Some(RunEvent::Result { cost_usd, input_tokens, output_tokens, text, is_error, .. }) =>
            ((cost_usd.unwrap_or(0.0) * 1_000_000.0).round() as i64, *input_tokens, *output_tokens, text.clone(), !is_error),
        _ => (0, 0, 0, String::new(), false),
    };
    let verdict: Option<Outcome> = outcome::parse(&text).map(|o| Outcome { outcome: o.outcome, summary: o.summary, issues: o.issues });
    let finished = ok && exit == "exit:0";
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
    record_head(&st.db, run_id, dir);
    if !refused.is_empty() && let Err(e) = core_runs::set_refused(&st.db, run_id, &refused) {
        eprintln!("gizai: saving what run {run_id} was refused failed: {e}");
    }
    let _ = core_runs::finish(&st.db, run_id, status, verdict.as_ref(), cost, input, output, error.as_deref());
    let mut moved = false;
    let mut nudge = None;
    let agent = core_runs::get(&st.db, run_id).map(|r| r.agent_id).unwrap_or_default();
    // Claude Code couldn't start working (not logged in): not a failed run. The card goes back where it was, on hold;
    // one failure holds one card, so a card the queue started before another card's failure paused the agent only
    // waits again.
    let unstarted = status == "failed" && tools == 0 && error.as_deref().is_some_and(login_problem);
    if unstarted {
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
        match workflow::apply_outcome(&st.db, run_id, if status == "succeeded" { verdict.as_ref() } else { None }) {
            Ok(g) => moved = g.moved_to.is_some(),
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
    Some(Resume { session, cli, reason: "it ended without a result".into(), answer: None, nudge: true })
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
        let busy = st.runs.live.lock().unwrap().values().filter(|l| l.agent_id == agent_id).count() as i64;
        if busy >= agent.max_runs.max(1) {
            return;
        }
        // The start checks the rest: active, budget, Runs at once (a wait, so nothing changes).
        if start_background(st, task_id, agent_id, "nudge", Some(resume)).await.is_some() {
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
    let mut started = vec![];
    if get_settings(st).agents_paused || is_closing(st) {
        return started;
    }
    let _one = st.runs.pulling.lock().await;
    let max = get_settings(st).max_concurrent_runs as usize;
    for (_, a) in team::all_agents(&st.db).unwrap_or_default() {
        if a.status != "active" {
            continue;
        }
        // At most its cards at once per pull; a slot that frees up meanwhile is filled by the pull its run's end starts.
        // The pause is asked before each start: a card this pull started may have failed already, and a failing CLI
        // must not run through the queue.
        for _ in 0..a.max_runs.max(1) {
            if !takes_cards(st, &a.actor_id) {
                break;
            }
            let (mine, all) = {
                let live = st.runs.live.lock().unwrap();
                (live.values().filter(|l| l.agent_id == a.actor_id).count() as i64, live.len())
            };
            if mine >= a.max_runs.max(1) || all >= max {
                break;
            }
            // Read again each time: the cards this pull started are claimed now, and others may have moved or been held.
            let busy = |t: &String| st.runs.live.lock().unwrap().values().any(|l| &l.task_id == t) || st.runs.starting.lock().unwrap().contains(t);
            let Some(task) = workflow::waiting_for(&st.db, &a.actor_id).unwrap_or_default().into_iter().find(|t| !busy(t)) else { break };
            match start_background(st, &task, &a.actor_id, "assigned", None).await {
                Some(done) => started.push((task, done)),
                None => break,
            }
        }
    }
    started
}
