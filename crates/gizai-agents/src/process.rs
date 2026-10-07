//! Runs an agent's CLI (`claude`, `codex`, `gemini`, …) as a child in its own process group, streams its events,
//! enforces the caps and stops it.
//! Signals only ever go to that process group (never pid 0 or 1), so nothing else on the machine is touched.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Notify, mpsc};

use crate::AgentError;
use crate::chat_stream::ChatEvent;
use crate::claude::ClaudeArgs;
use crate::cli;
use crate::stream::RunEvent;

/// A program to start for one run or chat turn.
#[derive(Debug, Clone, Default)]
pub struct Exec {
    pub bin: PathBuf,
    pub args: Vec<String>,
    /// Set on top of Gizai's own environment (another account's config folder, a PATH).
    pub env: Vec<(String, String)>,
    /// Written to stdin, which is then closed. Linux caps one argument at 128 KiB, so prompts go here when the CLI
    /// reads them there.
    pub stdin: String,
}

#[derive(Debug, Clone, Copy)]
pub struct Caps {
    pub max_time: Duration,
    /// Tool calls, each one counted (a message can make several).
    pub max_tool_calls: u32,
}

#[derive(Clone)]
pub struct StopHandle {
    notify: Arc<Notify>,
    /// The run's process group (its leader is the `claude` Gizai started).
    group: u32,
}

impl StopHandle {
    /// SIGINT to the run's process group, SIGTERM after 5 s, SIGKILL after 10 s.
    pub fn stop(&self) {
        self.notify.notify_one();
    }

    /// SIGKILL to the run's process group at once: Gizai is exiting and can't wait for a stop.
    pub fn kill(&self) {
        signal_group(self.group, libc::SIGKILL);
    }
}

/// The events a run's stdout turns into: task runs read `RunEvent`s, chat turns `ChatEvent`s.
pub trait StreamEvent: Clone + Send + 'static {
    fn parse(line: &str) -> Vec<Self>;
    /// An assistant message that calls a tool (counted against the turn cap).
    fn is_tool_call(&self) -> bool;
    /// Gizai's own markers: `cap_exceeded:tools`, `cap_exceeded:time`, `exit:<code>`.
    fn other(raw: String) -> Self;
}

impl StreamEvent for RunEvent {
    fn parse(line: &str) -> Vec<Self> { crate::stream::parse_line(line) }
    fn is_tool_call(&self) -> bool { matches!(self, RunEvent::ToolUse { .. }) }
    fn other(raw: String) -> Self { RunEvent::Other { raw_type: raw } }
}

impl StreamEvent for ChatEvent {
    fn parse(line: &str) -> Vec<Self> { crate::chat_stream::parse_line(line) }
    fn is_tool_call(&self) -> bool { matches!(self, ChatEvent::ToolUse { .. }) }
    fn other(raw: String) -> Self { ChatEvent::Other { raw_type: raw } }
}

/// Reads a run's output line by line; `finish` gives the events still due when the output ends.
pub trait LineParser<E>: Send + 'static {
    fn line(&mut self, line: &str) -> Vec<E>;
    fn finish(&mut self) -> Vec<E> {
        vec![]
    }
}

/// Claude Code's stream-json, one line at a time (task runs and chat turns).
pub struct ClaudeLines;

impl<E: StreamEvent> LineParser<E> for ClaudeLines {
    fn line(&mut self, line: &str) -> Vec<E> { E::parse(line) }
}

impl LineParser<RunEvent> for cli::Parser {
    fn line(&mut self, line: &str) -> Vec<RunEvent> { cli::Parser::line(self, line) }
    fn finish(&mut self) -> Vec<RunEvent> { cli::Parser::finish(self) }
}

pub struct RunHandle<E = RunEvent> {
    pub pid: u32,
    /// Closed after the final `other("exit:<code>")` event.
    pub events: mpsc::Receiver<E>,
    pub stop: StopHandle,
}

fn signal_group(pgid: u32, sig: i32) {
    if pgid > 1 {
        // SAFETY: plain syscall; a negative pid addresses the process group we created for this run.
        unsafe { libc::kill(-(pgid as i32), sig); }
    }
}

/// Whether a process is still in the run's process group (one that ended counts until it has been reaped).
fn group_alive(pgid: u32) -> bool {
    // SAFETY: signal 0 only checks that the group exists; nothing is sent.
    pgid > 1 && unsafe { libc::kill(-(pgid as i32), 0) } == 0
}

/// For a run a previous Gizai left behind: ends (SIGTERM) the process group led by `pid`, but only when
/// /proc shows that `pid` still leads its own group and still works in `expected_cwd` (the run's worktree),
/// so a reused pid can never hit anything else. Returns whether it signalled.
pub fn end_orphan_group(pid: u32, expected_cwd: &Path) -> bool {
    if pid <= 1 {
        return false;
    }
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
    // After "pid (comm) " come: state, ppid, pgrp, …
    let Some(close) = stat.rfind(')') else { return false };
    let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    if fields.get(2).and_then(|p| p.parse::<u32>().ok()) != Some(pid) {
        return false;
    }
    let Ok(cwd) = std::fs::read_link(format!("/proc/{pid}/cwd")) else { return false };
    match (cwd.canonicalize(), expected_cwd.canonicalize()) {
        (Ok(a), Ok(b)) if a == b => {}
        _ => return false,
    }
    signal_group(pid, libc::SIGTERM);
    true
}

/// Claude Code. Must be called inside a Tokio runtime.
pub fn spawn<E: StreamEvent>(args: &ClaudeArgs, cwd: &Path, log_path: &Path, caps: Caps) -> Result<RunHandle<E>, AgentError> {
    spawn_exec(&args.exec(), cwd, log_path, caps, ClaudeLines)
}

/// Any CLI, its output read by `parser`. Must be called inside a Tokio runtime.
pub fn spawn_exec<E: StreamEvent, P: LineParser<E>>(exec: &Exec, cwd: &Path, log_path: &Path, caps: Caps, mut parser: P)
    -> Result<RunHandle<E>, AgentError> {
    let stderr = std::fs::File::create(log_path.with_extension("stderr.log"))?;
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(log_path)?;
    let mut child = Command::new(&exec.bin)
        .args(&exec.args)
        .envs(exec.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(stderr)
        .process_group(0)
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| AgentError::Spawn(format!("{}: {e}", exec.bin.display())))?;
    let pid = child.id().ok_or_else(|| AgentError::Spawn("the process exited at once".into()))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    // The prompt goes in on stdin, then stdin closes so the CLI starts.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let prompt = exec.stdin.clone().into_bytes();
    tokio::spawn(async move {
        let _ = stdin.write_all(&prompt).await;
        let _ = stdin.shutdown().await;
    });

    let (tx, rx) = mpsc::channel::<E>(512);
    let stop = Arc::new(Notify::new());

    // Reader: log every line, forward its events, count tool-calling turns.
    let mut reader = {
        let tx = tx.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut calls = 0u32;
            let mut capped = false;
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = writeln!(log, "{line}");
                let evs = parser.line(&line);
                calls += evs.iter().filter(|e| E::is_tool_call(e)).count() as u32;
                for e in evs {
                    let _ = tx.send(e).await;
                }
                if calls > caps.max_tool_calls && !capped {
                    capped = true;
                    let _ = tx.send(E::other("cap_exceeded:tools".into())).await;
                    stop.notify_one();
                }
            }
            for e in parser.finish() {
                let _ = tx.send(e).await;
            }
        })
    };

    let reader_abort = reader.abort_handle();

    // Timer: the time cap.
    let timer = {
        let tx = tx.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            tokio::time::sleep(caps.max_time).await;
            let _ = tx.send(E::other("cap_exceeded:time".into())).await;
            stop.notify_one();
        })
    };

    // Waiter: owns the child (so its pid can't be reused while we may still signal it), escalates on stop,
    // cleans up whatever the run left in its group, then reports the exit and closes the channel.
    {
        let stop = stop.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                s = child.wait() => s,
                _ = stop.notified() => {
                    signal_group(pid, libc::SIGINT);
                    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                        Ok(s) => s,
                        Err(_) => {
                            signal_group(pid, libc::SIGTERM);
                            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                                Ok(s) => s,
                                Err(_) => { signal_group(pid, libc::SIGKILL); child.wait().await }
                            }
                        }
                    }
                }
            };
            timer.abort();
            // Background processes the agent started (a dev server, a watcher) would outlive the run
            // and keep stdout open; end them with the run. One that ignores SIGTERM gets SIGKILL after 3 s.
            signal_group(pid, libc::SIGTERM);
            let ended = tokio::time::timeout(Duration::from_secs(3), async {
                let _ = (&mut reader).await;
                while group_alive(pid) {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await;
            if ended.is_err() {
                signal_group(pid, libc::SIGKILL);
                reader_abort.abort(); // a dropped JoinHandle doesn't stop the task; it would keep the channel open
            }
            let code = match status {
                Ok(s) => match s.code() {
                    Some(c) => c.to_string(),
                    None => {
                        use std::os::unix::process::ExitStatusExt;
                        format!("signal-{}", s.signal().unwrap_or(0))
                    }
                },
                Err(e) => format!("unknown ({e})"),
            };
            let _ = tx.send(E::other(format!("exit:{code}"))).await;
        });
    }

    Ok(RunHandle { pid, events: rx, stop: StopHandle { notify: stop, group: pid } })
}
