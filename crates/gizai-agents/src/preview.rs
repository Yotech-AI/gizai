//! A project's preview (GA-182): before an agent that takes screenshots starts, Gizai serves the card's branch by running
//! the project's Preview command in its worktree on a port it picked, and ends it with the run (docs/research/
//! screenshots.md §3.5). It runs as the setup command does (`prepare`): with `bash -c` (Git Bash on Windows), stdin
//! closed, in a tree of its own (`os::spawn_tree`), here with its output in `.gizai-tmp/preview/preview.log` and without
//! `CLAUDECODE` in its environment (Astro detaches its server when it sees Claude Code). It gets `GIZAI_PREVIEW_PORT`,
//! `PORT` (the same), `GIZAI_PREVIEW_URL` and `GIZAI_PREVIEW_DIR`; the agent's own commands get the three
//! `GIZAI_PREVIEW_*` too (`agent_env`), not `PORT`.
use std::ffi::OsStr;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use crate::os::{self, End, Tree};
use crate::run_memory::Slice;

/// The preview's folder in the worktree's `.gizai-tmp`: its log, and room for a throwaway database
/// (`GIZAI_PREVIEW_DIR`).
pub const DIR: &str = "preview";
/// Its output, in that folder.
pub const LOG: &str = "preview.log";
/// A run's preview gets this many ports: its own and the nine after it (a second server, an API, Vite).
pub const PORTS: u16 = 10;
/// How long the preview gets to end after SIGTERM, before SIGKILL.
pub const END_GRACE: Duration = Duration::from_secs(3);
/// How often `wait_ready` knocks on its port.
const KNOCK_EVERY: Duration = Duration::from_millis(250);
/// The most of the log's end the prompt and the run's output quote.
const TAIL_LINES: usize = 10;

/// The address a preview on `port` is served at: `http://127.0.0.1:<port>`.
pub fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

/// The preview's folder in `worktree`: `<worktree>/.gizai-tmp/preview` (emptied with `.gizai-tmp` when the run ends).
pub fn dir(worktree: &Path) -> PathBuf {
    worktree.join(crate::worktree::TEMP_DIR).join(DIR)
}

/// The preview's log in `worktree`: `<worktree>/.gizai-tmp/preview/preview.log`.
pub fn log_path(worktree: &Path) -> PathBuf {
    dir(worktree).join(LOG)
}

/// What the agent's own commands get, so a project script it runs (to load the demo data again) finds the preview:
/// `GIZAI_PREVIEW_PORT`, `GIZAI_PREVIEW_URL` and `GIZAI_PREVIEW_DIR`. Not `PORT`: that one is the preview command's.
pub fn agent_env(port: u16, worktree: &Path) -> Vec<(String, String)> {
    vec![
        ("GIZAI_PREVIEW_PORT".into(), port.to_string()),
        ("GIZAI_PREVIEW_URL".into(), url(port)),
        ("GIZAI_PREVIEW_DIR".into(), dir(worktree).display().to_string()),
    ]
}

/// What the Preview command gets: `agent_env`, and `PORT` with the same port, as Claude Desktop's previews have it.
pub fn command_env(port: u16, worktree: &Path) -> Vec<(String, String)> {
    let mut env = agent_env(port, worktree);
    env.push(("PORT".into(), port.to_string()));
    env
}

/// The first port of a free block of ten (`PORTS`) in `from..=to`, for a run's preview: the block overlaps none of
/// `taken` (the blocks of the other live runs, by their first port), and its first port can be bound on 127.0.0.1 now.
/// None when no block is free.
pub fn free_port(from: u16, to: u16, taken: &[u16]) -> Option<u16> {
    let mut start = u32::from(from);
    while start + u32::from(PORTS) - 1 <= u32::from(to) {
        let port = start as u16;
        let clash = taken.iter().any(|&t| u32::from(t).abs_diff(start) < u32::from(PORTS));
        if !clash && TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok() {
            return Some(port);
        }
        start += u32::from(PORTS);
    }
    None
}

/// Whether something answers on `port` of 127.0.0.1 now: a connection is taken within a second.
pub fn answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(1)).is_ok()
}

/// A preview Gizai started, with everything it started in turn (`os::Tree`). `stop` ends it; dropped without that, it is
/// killed at once, so a preview never outlives its run.
#[derive(Debug)]
pub struct Preview {
    child: Option<Child>,
    tree: Tree,
    /// The port it was given (`GIZAI_PREVIEW_PORT`).
    pub port: u16,
    /// Its output: `<worktree>/.gizai-tmp/preview/preview.log`.
    pub log: PathBuf,
}

/// Starts the project's Preview `command` in `worktree` on `port`, as described above. `path`: the PATH for it, as for
/// the setup command (`prepare`); `slice`: the agent runs' slice to start it in (Linux, GA-202). Err says why it couldn't
/// start, in plain words.
pub fn start(command: &str, worktree: &Path, port: u16, path: Option<&OsStr>, slice: Option<&Slice>) -> Result<Preview, String> {
    let folder = dir(worktree);
    std::fs::create_dir_all(&folder).map_err(|e| format!("couldn't make its folder {}: {e}", folder.display()))?;
    let log = log_path(worktree);
    let out = std::fs::File::create(&log).map_err(|e| format!("couldn't write its log {}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| format!("couldn't write its log {}: {e}", log.display()))?;
    let own = std::env::var_os("PATH").unwrap_or_default();
    let bash = os::git_bash(path.unwrap_or(&own)).ok_or("it runs in Git Bash, and Git for Windows isn't installed")?;
    let mut cmd = match slice.and_then(|s| s.command_for(bash.as_os_str(), path)) {
        Some(c) => c,
        None => os::command(&bash),
    };
    cmd.arg("-c").arg(command).current_dir(worktree)
        .stdin(Stdio::null()).stdout(Stdio::from(out)).stderr(Stdio::from(err))
        .env_remove("CLAUDECODE").env("NO_COLOR", "1");
    for (k, v) in command_env(port, worktree) {
        cmd.env(k, v);
    }
    if let Some(p) = path {
        cmd.env("PATH", p);
    }
    let (child, tree) = os::spawn_tree(&mut cmd, false).map_err(|e| format!("couldn't start: {e}"))?;
    Ok(Preview { child: Some(child), tree, port, log })
}

impl Preview {
    /// The id of the process it started (bash), which leads its process group on Linux and macOS.
    pub fn pid(&self) -> u32 {
        self.child.as_ref().map(Child::id).unwrap_or(0)
    }

    /// Its whole tree, to end from elsewhere (Gizai quitting at once).
    pub fn tree(&self) -> Tree {
        self.tree.clone()
    }

    /// Whether anything of it still runs. The program Gizai started is waited for here once it has ended.
    pub fn running(&mut self) -> bool {
        if let Some(c) = self.child.as_mut() {
            let _ = c.try_wait();
        }
        self.tree.alive()
    }

    /// Waits until its port answers, at most `limit`, knocking every quarter of a second; `give_up` is asked between
    /// knocks (Gizai quitting). Err says why not, as the card's set and the prompt say it: "It didn't answer on port 47130
    /// in 120 s", "It ended before it answered (exit code 1)".
    pub async fn wait_ready(&mut self, limit: Duration, give_up: impl Fn() -> bool) -> Result<(), String> {
        let t0 = Instant::now();
        loop {
            let port = self.port;
            if tokio::task::spawn_blocking(move || answers(port)).await.unwrap_or(false) {
                return Ok(());
            }
            if !self.running() {
                let code = self.child.as_mut().and_then(|c| c.try_wait().ok().flatten()).and_then(|s| s.code());
                return Err(match code {
                    Some(code) => format!("It ended before it answered on port {port} (exit code {code})"),
                    None => format!("It ended before it answered on port {port}"),
                });
            }
            if give_up() {
                return Err("Gizai is quitting".into());
            }
            if t0.elapsed() >= limit {
                return Err(format!("It didn't answer on port {port} in {} s", limit.as_secs()));
            }
            tokio::time::sleep(KNOCK_EVERY).await;
        }
    }

    /// Ends its whole tree: SIGTERM, then SIGKILL to what still runs after `END_GRACE` (on Windows its Job Object ends
    /// everything at once), and waits until nothing of it runs (a few seconds at most).
    pub async fn stop(mut self) {
        self.tree.end(End::Terminate);
        let t0 = Instant::now();
        while self.running() && t0.elapsed() < END_GRACE {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        if self.running() {
            self.tree.end(End::Kill);
            let t1 = Instant::now();
            while self.running() && t1.elapsed() < Duration::from_secs(2) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.try_wait();
        }
    }
}

impl Drop for Preview {
    /// Not stopped (a start that went wrong after it started): killed at once, and waited for on a thread of its own.
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            self.tree.end(End::Kill);
            std::thread::spawn(move || {
                let _ = c.wait();
            });
        }
    }
}

/// The last lines of the preview's log in `worktree` (at most ten, without terminal colours), for the prompt and the
/// run's output when it didn't answer; "" when it wrote nothing.
pub fn log_tail(worktree: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(log_path(worktree)) else { return String::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(16 * 1024)));
    let mut bytes = vec![];
    let _ = f.read_to_end(&mut bytes);
    let text = crate::prepare::tail(&String::from_utf8_lossy(&bytes));
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}
