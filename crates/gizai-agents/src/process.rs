//! Runs an agent's CLI (`claude`, `codex`, `gemini`, …) as a child in its own process group (a Job Object on Windows,
//! see `os::Tree`), streams its events, enforces the caps, ends it when it doesn't exit once the agent is done (GA-221)
//! and stops it.
//! Gizai's own markers come among the CLI's events (`StreamEvent::other`): `cap_exceeded:tools` and `cap_exceeded:time`
//! at a cap, `ended_after_result` when the CLI stayed quiet for `Caps::after_result` after the agent's result,
//! `oom_kill:<n>` when Linux ended a process in the run at the memory limit, `leftover:<pid> <name>` for each process
//! ended that the run left in its scope, and last `exit:<code>`.
//! Signals only ever go to that process group (never pid 0 or 1), so nothing else on the machine is touched; in the
//! slice of the agent runs (Linux), also to what the run left running in its own scope (GA-211).
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Notify, mpsc};
use tokio::time::Instant;

use crate::AgentError;
use crate::chat_stream::ChatEvent;
use crate::claude::ClaudeArgs;
use crate::cli;
use crate::os::{self, End, Tree};
use crate::run_memory::{Leftover, OomWatch, Scope, Slice};
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
    /// Linux: the slice of Gizai's agent runs (GA-202) to start it in, in a scope of its own (`Slice::command`), so the
    /// memory limit for agent runs covers it and what it starts. None: outside it, as before.
    pub slice: Option<Slice>,
}

#[derive(Debug, Clone, Copy)]
pub struct Caps {
    pub max_time: Duration,
    /// Tool calls, each one counted (a message can make several).
    pub max_tool_calls: u32,
    /// GA-221: how long the CLI may stay quiet once the agent is done (`StreamEvent::is_result`) before Gizai ends it as
    /// Stop does, with an `ENDED_AFTER_RESULT` marker. Something the agent left running in the background (a server, a
    /// watcher) can keep a CLI from exiting. `AFTER_RESULT` for task runs and chat turns.
    pub after_result: Duration,
}

/// `Caps::after_result` for task runs and chat turns.
pub const AFTER_RESULT: Duration = Duration::from_secs(3 * 60);

/// The marker for a CLI Gizai ended because it stayed quiet for `Caps::after_result` once the agent was done (GA-221).
/// Before the run's `LEFTOVER` markers and its `exit:`.
pub const ENDED_AFTER_RESULT: &str = "ended_after_result";

#[derive(Clone)]
pub struct StopHandle {
    notify: Arc<Notify>,
    /// The run's process group (its leader is the `claude` Gizai started); on Windows its job, which stays open while
    /// a clone of this handle lives.
    tree: Tree,
    /// Linux, in the slice of the agent runs: the run's scope (GA-211).
    scope: Option<Scope>,
}

impl StopHandle {
    /// SIGINT to the run's process group, SIGTERM after 5 s, SIGKILL after 10 s. On Windows the run's job ends at once
    /// (Windows has no signal Gizai can send a CLI, see `os::End`).
    pub fn stop(&self) {
        self.notify.notify_one();
    }

    /// SIGKILL to the run's process group at once (on Windows its job ends), and in the slice to everything in the run's
    /// scope (GA-211): Gizai is exiting and can't wait for a stop.
    pub fn kill(&self) {
        self.tree.end(End::Kill);
        if let Some(s) = &self.scope {
            s.kill();
        }
    }
}

/// The marker for a process Linux ended in a run at the memory limit for agent runs (GA-202), followed by how many it
/// ended in that run so far: `oom_kill:1`.
pub const OOM_KILL: &str = "oom_kill:";

/// The marker for a process the run left running in its scope outside its process group, which Gizai ended at the
/// run's end (GA-211, `Scope::end_leftovers`), followed by its pid and name: `leftover:12345 chromium`. One per process,
/// before `exit:`.
pub const LEFTOVER: &str = "leftover:";

/// The process a `LEFTOVER` marker names.
pub fn leftover(raw: &str) -> Option<Leftover> {
    let (pid, name) = raw.strip_prefix(LEFTOVER)?.split_once(' ')?;
    Some(Leftover { pid: pid.parse().ok()?, name: name.to_string() })
}

/// The events a run's stdout turns into: task runs read `RunEvent`s, chat turns `ChatEvent`s.
pub trait StreamEvent: Clone + Send + 'static {
    fn parse(line: &str) -> Vec<Self>;
    /// An assistant message that calls a tool (counted against the turn cap).
    fn is_tool_call(&self) -> bool;
    /// The agent is done (GA-221): a `Result` (Claude Code's result line, or what `cli::Parser` makes of another CLI's
    /// end), or a text whose GIZAI_RESULT line `outcome::parse` accepts, as Claude Code may hold its result line back
    /// while something the run started still runs in the background. From then on `Caps::after_result` counts; a tool
    /// call after it means the agent went on.
    fn is_result(&self) -> bool;
    /// Gizai's own markers: `cap_exceeded:tools`, `cap_exceeded:time`, `exit:<code>`, `ended_after_result` when Gizai
    /// ended a CLI that stayed quiet for `Caps::after_result` once the agent was done (`ENDED_AFTER_RESULT`),
    /// `oom_kill:<n>` when Linux ended a process in the run at the memory limit for agent runs (the n-th in this run,
    /// `OOM_KILL`), and `leftover:<pid> <name>` for each process Gizai ended that the run left running in its scope
    /// (`LEFTOVER`).
    fn other(raw: String) -> Self;
}

impl StreamEvent for RunEvent {
    fn parse(line: &str) -> Vec<Self> { crate::stream::parse_line(line) }
    fn is_tool_call(&self) -> bool { matches!(self, RunEvent::ToolUse { .. }) }
    fn is_result(&self) -> bool {
        matches!(self, RunEvent::Result { .. }) || matches!(self, RunEvent::Text { text } if crate::outcome::parse(text).is_some())
    }
    fn other(raw: String) -> Self { RunEvent::Other { raw_type: raw } }
}

impl StreamEvent for ChatEvent {
    fn parse(line: &str) -> Vec<Self> { crate::chat_stream::parse_line(line) }
    fn is_tool_call(&self) -> bool { matches!(self, ChatEvent::ToolUse { .. }) }
    fn is_result(&self) -> bool {
        matches!(self, ChatEvent::Result { .. }) || matches!(self, ChatEvent::Text { text } if crate::outcome::parse(text).is_some())
    }
    fn other(raw: String) -> Self { ChatEvent::Other { raw_type: raw } }
}

/// GA-221: whether the agent is done after `evs`, when it was `done` before them: a result says it is, a tool call after
/// it that it went on.
fn done_after<E: StreamEvent>(done: bool, evs: &[E]) -> bool {
    evs.iter().fold(done, |d, e| if e.is_tool_call() { false } else if e.is_result() { true } else { d })
}

/// GA-221: resolves once the agent is done and its CLI has been quiet for `after`. `quiet` says since when it has been
/// quiet while the agent is done (each line starts that again), None while it isn't; the reader drops it when the output
/// ends, and as it stood then it holds: no line can come any more.
async fn quiet_after_result(mut quiet: tokio::sync::watch::Receiver<Option<Instant>>, after: Duration) {
    loop {
        let since = *quiet.borrow_and_update();
        let open = match since {
            Some(t) => tokio::select! {
                _ = sleep_after(t, after) => return,
                r = quiet.changed() => r.is_ok(),
            },
            None => quiet.changed().await.is_ok(),
        };
        if !open {
            match since {
                Some(t) => return sleep_after(t, after).await,
                None => return std::future::pending().await,
            }
        }
    }
}

/// Until `after` has passed since `t`; for ever when that is too far off to tell.
async fn sleep_after(t: Instant, after: Duration) {
    match t.checked_add(after) {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Ends the run's process group as Stop does: SIGINT, SIGTERM after 5 s, SIGKILL after 10 s; on Windows its job at once
/// (see `os::End`). Returns how the CLI exited.
async fn end_group(tree: &Tree, child: &mut tokio::process::Child) -> std::io::Result<std::process::ExitStatus> {
    tree.end(End::Interrupt);
    match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        Ok(s) => s,
        Err(_) => {
            tree.end(End::Terminate);
            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                Ok(s) => s,
                Err(_) => { tree.end(End::Kill); child.wait().await }
            }
        }
    }
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

/// For a run a previous Gizai left behind: ends (SIGTERM) the process group led by `pid`, but only when `pid` still
/// leads its own group and still works in `expected_cwd` (the run's worktree), so a reused pid can never hit anything
/// else. Linux reads that in /proc, macOS asks the kernel (`getpgid`, `proc_pidinfo`). Returns whether it signalled.
/// Windows: always false, as nothing is left there: the previous Gizai's Job Objects ended its agents when it exited.
pub fn end_orphan_group(pid: u32, expected_cwd: &Path) -> bool {
    end_orphan(pid, expected_cwd, false)
}

/// As `end_orphan_group`, for a group whose leader may have changed folder inside the worktree: a run's preview (GA-182),
/// whose Preview command may start with `cd web && …`. `pid` must work in `worktree` or in a folder inside it.
pub fn end_orphan_group_within(pid: u32, worktree: &Path) -> bool {
    end_orphan(pid, worktree, true)
}

fn end_orphan(pid: u32, expected_cwd: &Path, or_inside: bool) -> bool {
    if pid <= 1 {
        return false;
    }
    #[cfg(unix)]
    {
        let Some(cwd) = group_leader_cwd(pid) else { return false };
        match (cwd.canonicalize(), expected_cwd.canonicalize()) {
            (Ok(a), Ok(b)) if a == b || (or_inside && a.starts_with(&b)) => {}
            _ => return false,
        }
        Tree::led_by(pid).end(End::Terminate);
        true
    }
    #[cfg(windows)]
    {
        let _ = (expected_cwd, or_inside);
        false
    }
}

/// The folder `pid` works in, when it leads its own process group: from /proc/<pid>/stat and /proc/<pid>/cwd.
#[cfg(target_os = "linux")]
fn group_leader_cwd(pid: u32) -> Option<PathBuf> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // After "pid (comm) " come: state, ppid, pgrp, …
    let close = stat.rfind(')')?;
    let fields: Vec<&str> = stat[close + 1..].split_whitespace().collect();
    if fields.get(2).and_then(|p| p.parse::<u32>().ok()) != Some(pid) {
        return None;
    }
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

/// macOS, which has no /proc: the folder `pid` works in, when it leads its own process group (getpgid), from the
/// kernel's proc_pidinfo (PROC_PIDVNODEPATHINFO).
#[cfg(target_os = "macos")]
fn group_leader_cwd(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let pid = libc::pid_t::try_from(pid).ok()?;
    // SAFETY: plain syscalls; proc_pidinfo writes at most `size` bytes into `info`, a plain C struct.
    let info = unsafe {
        if libc::getpgid(pid) != pid {
            return None;
        }
        let mut info: libc::proc_vnodepathinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
        if libc::proc_pidinfo(pid, libc::PROC_PIDVNODEPATHINFO, 0, (&raw mut info).cast(), size) != size {
            return None;
        }
        info
    };
    // a C string in a char[MAXPATHLEN], which libc declares as [[c_char; 32]; 32]
    let path: Vec<u8> = info.pvi_cdir.vip_path.iter().flatten().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    (!path.is_empty()).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(&path)))
}

/// Claude Code. Must be called inside a Tokio runtime.
pub fn spawn<E: StreamEvent>(args: &ClaudeArgs, cwd: &Path, log_path: &Path, caps: Caps) -> Result<RunHandle<E>, AgentError> {
    spawn_exec(&args.exec(), cwd, log_path, caps, ClaudeLines)
}

/// Any CLI, its output read by `parser`. Must be called inside a Tokio runtime. With `exec.slice` it starts in a scope
/// of its own in that slice, the events get an `OOM_KILL` marker each time Linux ends a process in it at the limit, and
/// at its end what it left in its scope ends too, each process with a `LEFTOVER` marker.
pub fn spawn_exec<E: StreamEvent, P: LineParser<E>>(exec: &Exec, cwd: &Path, log_path: &Path, caps: Caps, mut parser: P)
    -> Result<RunHandle<E>, AgentError> {
    let stderr = std::fs::File::create(log_path.with_extension("stderr.log"))?;
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(log_path)?;
    // found on the PATH the CLI gets; a CLI that can't be found starts as before, so it fails the usual way
    let path = exec.env.iter().rev().find(|(k, _)| k == "PATH").map(|(_, v)| std::ffi::OsString::from(v));
    let scoped = exec.slice.as_ref().and_then(|s| Some((s.scoped_for(exec.bin.as_os_str(), path.as_deref())?, s)));
    let (mut cmd, slice, scope) = match scoped {
        Some(((c, scope), s)) => (tokio::process::Command::from(c), Some(s), Some(scope)),
        None => (os::tokio_command(&exec.bin), None, None),
    };
    cmd.args(&exec.args)
        .envs(exec.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(stderr)
        .kill_on_drop(false);
    let (mut child, tree) = os::spawn_tree_tokio(&mut cmd).map_err(|e| AgentError::Spawn(format!("{}: {e}", exec.bin.display())))?;
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
    // GA-221: since when the CLI has been quiet while the agent is done, from the reader to the waiter.
    let (quiet, quiet_rx) = tokio::sync::watch::channel::<Option<Instant>>(None);

    // Linux, in the slice: each process Linux ends in the run's scope at the memory limit, as it happens.
    let watch = slice.map(|s| {
        let tx = tx.clone();
        OomWatch::start(s, pid, move |n| {
            let _ = tx.blocking_send(E::other(format!("{OOM_KILL}{n}")));
        })
    });

    // Reader: log every line, forward its events, count tool-calling turns, and tell the waiter when the agent is done
    // and since when the CLI has been quiet (GA-221).
    let mut reader = {
        let tx = tx.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut calls = 0u32;
            let mut capped = false;
            let mut done = false;
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = writeln!(log, "{line}");
                let evs = parser.line(&line);
                calls += evs.iter().filter(|e| E::is_tool_call(e)).count() as u32;
                let was = done;
                done = done_after(done, &evs);
                for e in evs {
                    let _ = tx.send(e).await;
                }
                // While the agent is done, each line starts the quiet time again.
                if done || was {
                    quiet.send_replace(done.then(Instant::now));
                }
                if calls > caps.max_tool_calls && !capped {
                    capped = true;
                    let _ = tx.send(E::other("cap_exceeded:tools".into())).await;
                    stop.notify_one();
                }
            }
            // What is still due when the output ends is no line from the CLI: it only says whether the agent is done.
            let evs = parser.finish();
            let was = done;
            done = done_after(done, &evs);
            for e in evs {
                let _ = tx.send(e).await;
            }
            if done != was {
                quiet.send_replace(done.then(Instant::now));
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

    // Waiter: owns the child (so its pid can't be reused while we may still signal it), escalates on stop, ends the run
    // the same way when its CLI stays quiet once the agent is done (GA-221), cleans up whatever the run left in its
    // group, then in its scope, then reports the exit and closes the channel.
    // On Windows every step ends the run's job at once (see `os::End`).
    {
        let stop = stop.clone();
        let tree = tree.clone();
        let scope = scope.clone();
        tokio::spawn(async move {
            let status = tokio::select! {
                s = child.wait() => s,
                _ = stop.notified() => end_group(&tree, &mut child).await,
                _ = quiet_after_result(quiet_rx, caps.after_result) => {
                    // The agent is done: the time cap doesn't stop this run as well.
                    timer.abort();
                    let _ = tx.send(E::other(ENDED_AFTER_RESULT.into())).await;
                    end_group(&tree, &mut child).await
                }
            };
            // The last look at the run's scope, while what the run left in it may still be there.
            let watched = watch.map(|w| tokio::task::spawn_blocking(move || w.finish()));
            timer.abort();
            // Background processes the agent started (a dev server, a watcher) would outlive the run
            // and keep stdout open; end them with the run. One that ignores SIGTERM gets SIGKILL after 3 s.
            tree.end(End::Terminate);
            let ended = tokio::time::timeout(Duration::from_secs(3), async {
                let _ = (&mut reader).await;
                while tree.alive() {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }).await;
            if ended.is_err() {
                tree.end(End::Kill);
                reader_abort.abort(); // a dropped JoinHandle doesn't stop the task; it would keep the channel open
            }
            let code = match status {
                Ok(s) => match s.code() {
                    Some(c) => c.to_string(),
                    #[cfg(unix)]
                    None => {
                        use std::os::unix::process::ExitStatusExt;
                        format!("signal-{}", s.signal().unwrap_or(0))
                    }
                    // Windows has no signals: every exit has a code (1 when Gizai ended the job)
                    #[cfg(not(unix))]
                    None => "unknown".to_string(),
                },
                Err(e) => format!("unknown ({e})"),
            };
            if let Some(w) = watched {
                let _ = w.await;
            }
            // GA-211: then what the run left in its scope outside its group (a browser started detached, `setsid`),
            // which would outlive it there: SIGTERM, SIGKILL 3 s later. The run hears of each process.
            if let Some(scope) = scope {
                let left = tokio::task::spawn_blocking(move || scope.end_leftovers(Some(pid))).await.unwrap_or_default();
                for l in left {
                    let _ = tx.send(E::other(format!("{LEFTOVER}{} {}", l.pid, l.name))).await;
                }
            }
            let _ = tx.send(E::other(format!("exit:{code}"))).await;
        });
    }

    Ok(RunHandle { pid, events: rx, stop: StopHandle { notify: stop, tree, scope } })
}
