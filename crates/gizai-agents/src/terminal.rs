//! Terminals (GA-133): your shell in a pseudo-terminal, as many as you open, kept in memory while Gizai runs and never
//! written anywhere: no file, database row, log, Memory note or card gets what a terminal shows or is sent
//! (docs/research/terminal.md §3.2, §5.5 and §5.6). Gizai's window is their only user; no agent or Team Lead tool
//! reaches them. The app checks the folder first (src-tauri/src/terminal.rs).
//! - A terminal is portable-pty's pseudo-terminal (ConPTY on Windows) with a shell in it (`os::user_shell`), started in
//!   an Editor folder with the environment it is given (`environment`).
//! - Output: a thread per terminal reads up to 64 KB at a time and keeps the last 256 KB; another sends what came to the
//!   window's listener (`attach`), at most one message every 8 ms. The window acknowledges what it has written (`ack`):
//!   with more than 500,000 bytes not acknowledged, reading pauses until fewer than 200,000 are, and the program waits
//!   in its write meanwhile, as in any terminal. A new listener replaces the last one and first gets the bytes kept.
//! - Input goes to a thread of the terminal's own, so the window never waits on a program that doesn't read. On Windows
//!   that thread also resizes and closes the pseudo console: ConPTY's calls can wait on the terminal.
//! - `busy`: the program in the foreground when it isn't the shell, looked at about once a second.
//! - A shell that ends by itself with exit code 0 leaves the list; any other end stays, as `ended`, until it is closed.
//! - `close` ends the shell and everything it started. Linux and macOS: SIGHUP to the foreground process group, the
//!   shell's and the session's other groups (background jobs), then the pseudo-terminal closes, and after 3 s SIGKILL to
//!   every process still in the terminal's session (`nohup` programs). Windows: the terminal's Job Object ends at once.
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use portable_pty::ChildKiller;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};

use crate::os;

/// One read from a terminal, at most.
const READ: usize = 64 * 1024;
/// The output kept for a listener that attaches later (after the window reloaded, say).
const KEEP: usize = 256 * 1024;
/// At most one message to the listener in this time.
const EVERY: Duration = Duration::from_millis(8);
/// Reading pauses with more than this many bytes sent and not acknowledged…
const PAUSE_ABOVE: u64 = 500_000;
/// …and goes on with fewer than this.
const GO_ON_BELOW: u64 = 200_000;
/// How often `busy` is looked at.
const BUSY_EVERY: Duration = Duration::from_secs(1);
/// Linux and macOS: how long `close` waits after SIGHUP before SIGKILL.
const KILL_AFTER: Duration = Duration::from_secs(3);
/// The longest name, in characters.
const NAME_MAX: usize = 40;
/// The most columns or rows (ConPTY takes a 16-bit signed size).
const SIZE_MAX: u16 = 4096;

/// An open terminal as the window shows it (contract CT).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    /// For as long as Gizai runs.
    pub id: String,
    /// "gizai", "gizai 2", "GA-123", or the name given.
    pub name: String,
    /// The Editor folder's id it started in (`EditorFolder::id`).
    pub folder: String,
    /// As shown, `~` for the home folder.
    pub folder_path: String,
    /// The card, for a card's worktree.
    pub task_id: Option<String>,
    /// "zsh", "bash", "sh", "pwsh", "powershell", "cmd".
    pub shell: String,
    /// The program in the foreground, when it isn't the shell ("npm", "cargo").
    pub busy: Option<String>,
    /// Set when the shell ended other than with exit code 0 (that one leaves the list).
    pub ended: Option<TerminalEnd>,
    pub cols: u16,
    pub rows: u16,
    /// Ms since 1970.
    pub opened_at: i64,
}

/// How a terminal's shell ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalEnd {
    /// Its exit code; None when a signal ended it.
    pub code: Option<i32>,
    /// Ms since 1970.
    pub at: i64,
    /// What ended it, as a sentence: "sh ended with exit code 3", "zsh was ended by SIGKILL".
    pub why: Option<String>,
}

/// What `Terminals::open` starts.
#[derive(Debug, Clone)]
pub struct Start {
    /// Where the shell starts: an Editor folder's id, its canonical path.
    pub folder: String,
    /// The folder as shown, `~` for the home folder.
    pub folder_path: String,
    /// The folder's own name, or the card's identifier: the default name.
    pub folder_name: String,
    /// The card, for a card's worktree.
    pub task_id: Option<String>,
    /// The shell and its arguments (`os::user_shell`; tests start /bin/sh or cmd.exe).
    pub program: PathBuf,
    pub args: Vec<OsString>,
    /// The whole environment the shell gets (`environment`).
    pub env: Vec<(OsString, OsString)>,
    /// The size; 0 for either takes 80 × 24.
    pub cols: u16,
    pub rows: u16,
    /// None or empty: the default name.
    pub name: Option<String>,
}

/// The window's listener for a terminal's output: it takes one message of bytes, and says false when it is gone.
pub type Output = Box<dyn FnMut(Vec<u8>) -> bool + Send>;

/// Told the whole list after an open, a rename, a close, a shell's end and a change of `busy`.
pub type Changed = Arc<dyn Fn(Vec<TerminalSession>) + Send + Sync>;

/// The environment of a terminal's shell: `own` (Gizai's) with `path` as PATH (the PATH runs get), TERM=xterm-256color,
/// COLORTERM=truecolor, TERM_PROGRAM=gizai and TERM_PROGRAM_VERSION=`version`, and without the GIZAI_* variables, which
/// are meant for Gizai itself (GIZAI_DATA_DIR). On macOS also LANG=en_US.UTF-8 when none of LANG, LC_ALL and LC_CTYPE
/// is set: an app started from the Dock has none, and programs then print ? for anything outside ASCII.
pub fn environment(own: impl IntoIterator<Item = (OsString, OsString)>, path: &OsStr, version: &str) -> Vec<(OsString, OsString)> {
    let mut set: Vec<(&str, OsString)> = vec![
        ("TERM", "xterm-256color".into()),
        ("COLORTERM", "truecolor".into()),
        ("TERM_PROGRAM", "gizai".into()),
        ("TERM_PROGRAM_VERSION", version.into()),
    ];
    if !path.is_empty() {
        set.push(("PATH", path.to_os_string()));
    }
    let mut env: Vec<(OsString, OsString)> = own.into_iter()
        .filter(|(k, _)| !k.to_string_lossy().to_ascii_uppercase().starts_with("GIZAI_") && !set.iter().any(|(s, _)| same_key(k, s)))
        .collect();
    let has_locale = ["LANG", "LC_ALL", "LC_CTYPE"].iter().any(|l| env.iter().any(|(k, v)| same_key(k, l) && !v.is_empty()));
    if cfg!(target_os = "macos") && !has_locale {
        env.retain(|(k, _)| !same_key(k, "LANG"));
        set.push(("LANG", "en_US.UTF-8".into()));
    }
    env.extend(set.into_iter().map(|(k, v)| (OsString::from(k), v)));
    env
}

/// Whether the variable `k` is `name` (on Windows in any case).
fn same_key(k: &OsStr, name: &str) -> bool {
    if cfg!(windows) { k.to_string_lossy().eq_ignore_ascii_case(name) } else { k == name }
}

/// The open terminals, in the order they were opened.
#[derive(Default)]
pub struct Terminals {
    list: Arc<List>,
}

#[derive(Default)]
struct List {
    sessions: Mutex<Vec<Arc<Session>>>,
    /// Held while a list is told, so the lists go out in the order they were made.
    telling: Mutex<()>,
}

impl List {
    fn views(&self) -> Vec<TerminalSession> {
        lock(&self.sessions).iter().map(|s| s.view()).collect()
    }

    fn remove(&self, id: &str) -> Option<Arc<Session>> {
        let mut sessions = lock(&self.sessions);
        let at = sessions.iter().position(|s| s.id == id)?;
        Some(sessions.remove(at))
    }
}

impl Terminals {
    /// Every open terminal, in the order they were opened.
    pub fn list(&self) -> Vec<TerminalSession> {
        self.list.views()
    }

    /// Starts `start.program` in a new pseudo-terminal in `start.folder` and adds it to the list; `changed` hears of it
    /// and of everything that happens to it later. Nothing starts when the pseudo-terminal or the shell can't.
    pub fn open(&self, start: Start, changed: Changed) -> Result<TerminalSession, String> {
        let shell = shell_name(&start.program);
        let (cols, rows) = (size(start.cols, 80), size(start.rows, 24));
        let opened = std::panic::catch_unwind(|| native_pty_system().openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }))
            // portable-pty panics on a Windows without ConPTY
            .map_err(|_| format!("Gizai couldn't open a terminal{}", if cfg!(windows) { ": it needs Windows 10 version 1809 or newer" } else { "" }))?;
        let pty = opened.map_err(|e| format!("Gizai couldn't open a terminal: {e}"))?;
        let failed = format!("Couldn't start {shell} in {}", start.folder_path);
        let not_started = |e: &dyn std::fmt::Display| format!("{failed}: {e}");
        #[cfg(unix)]
        let (reader, writer, wake) = {
            let (reader, writer) = own_ends(pty.master.as_ref()).map_err(|e| not_started(&e))?;
            (reader, writer, Wake::new().map_err(|e| not_started(&e))?)
        };
        #[cfg(windows)]
        let (reader, writer) = (pty.master.try_clone_reader().map_err(|e| not_started(&e))?, pty.master.take_writer().map_err(|e| not_started(&e))?);
        let mut cmd = CommandBuilder::new(&start.program);
        cmd.args(&start.args);
        cmd.cwd(&start.folder);
        cmd.env_clear();
        for (k, v) in &start.env {
            cmd.env(k, v);
        }
        #[cfg_attr(unix, allow(unused_mut))]
        let mut child = pty.slave.spawn_command(cmd).map_err(|e| not_started(&e))?;
        // Only what runs in the terminal holds its other end now, so reading it ends when they are gone.
        drop(pty.slave);
        let Some(pid) = child.process_id() else {
            return Err(not_started(&"it ended at once"));
        };
        // Windows: the shell goes into a Job Object of its own right away, so everything it starts is in it too.
        #[cfg(windows)]
        let job = {
            let added = os::Job::new().and_then(|job| {
                let process = child.as_raw_handle().ok_or_else(|| io::Error::other("it has no handle"))?;
                job.add(process as _).map(|()| job)
            });
            match added {
                Ok(job) => job,
                Err(e) => {
                    let _ = child.kill();
                    return Err(not_started(&format!("Windows didn't let Gizai keep track of it ({e})")));
                }
            }
        };
        let (todo, todo_rx) = mpsc::channel();
        let session = Arc::new(Session {
            id: new_id(),
            folder: start.folder,
            folder_path: start.folder_path,
            folder_name: start.folder_name,
            task_id: start.task_id,
            shell,
            opened_at: now_ms(),
            pid,
            state: Mutex::new(State { name: String::new(), busy: None, ended: None, cols, rows, closing: false }),
            out: Mutex::new(Out::default()),
            out_moved: Condvar::new(),
            pty: Mutex::new(Some(pty.master)),
            todo: Mutex::new(Some(todo)),
            done: Mutex::new(false),
            done_moved: Condvar::new(),
            reaped: AtomicBool::new(false),
            #[cfg(unix)]
            wake,
            #[cfg(windows)]
            job,
            list: Arc::downgrade(&self.list),
            changed,
        });
        let threads = [
            thread("read", { let s = session.clone(); move || s.read_loop(reader) }),
            thread("send", { let s = session.clone(); move || s.send_loop() }),
            thread("write", { let s = session.clone(); move || s.write_loop(todo_rx, writer) }),
        ];
        if let Some(Err(e)) = threads.into_iter().find(Result::is_err) {
            session.begin_end();
            session.finish_end(Instant::now() + KILL_AFTER);
            return Err(not_started(&e));
        }
        // Its name and its place in the list, one terminal at a time, so two can't get the same default name.
        {
            let mut sessions = lock(&self.list.sessions);
            let name = match start.name.as_deref().map(clean_name).filter(|n| !n.is_empty()) {
                Some(n) => n,
                None => default_name(&session.folder_name, &sessions.iter().map(|s| s.name()).collect::<Vec<_>>()),
            };
            lock(&session.state).name = name;
            sessions.push(session.clone());
        }
        // Started once it is in the list: a shell that ends at once leaves the list again.
        if let Err(e) = thread("wait", { let s = session.clone(); move || s.wait_loop(child) }) {
            self.list.remove(&session.id);
            session.begin_end();
            session.finish_end(Instant::now() + KILL_AFTER);
            return Err(not_started(&e));
        }
        session.tell();
        Ok(session.view())
    }

    /// Sends the terminal's output to `output`: first what it kept (at most 256 KB, from the start of a line once it
    /// has dropped older output), then everything new, in order. It replaces the listener attached before.
    pub fn attach(&self, id: &str, output: Output) -> Result<(), String> {
        let s = self.find(id)?;
        let mut out = lock(&s.out);
        out.generation += 1;
        out.listener = Some(output);
        out.attached = true;
        out.pending = kept(&out.kept, out.wrapped);
        out.unacked = out.pending.len() as u64;
        out.paused = out.unacked > PAUSE_ABOVE;
        s.out_moved.notify_all();
        Ok(())
    }

    /// Types `data` into the terminal: as UTF-8, or with `binary` one byte per character (xterm.js's onBinary, as
    /// Node's "binary" encoding takes it). Returns at once; the terminal's own thread writes it as the program reads.
    pub fn input(&self, id: &str, data: &str, binary: bool) -> Result<(), String> {
        let s = self.find(id)?;
        if lock(&s.state).ended.is_some() || data.is_empty() {
            return Ok(());
        }
        let bytes = if binary { data.chars().map(|c| c as u32 as u8).collect() } else { data.as_bytes().to_vec() };
        s.todo(Todo::Input(bytes));
        Ok(())
    }

    /// The window's place for the terminal has this size now. 0 columns or rows (a place not measured yet) keeps the
    /// size it has. On Windows it returns at once: the terminal's own thread resizes the pseudo console.
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let s = self.find(id)?;
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        let (cols, rows) = (cols.min(SIZE_MAX), rows.min(SIZE_MAX));
        {
            let mut st = lock(&s.state);
            st.cols = cols;
            st.rows = rows;
        }
        let to = PtySize { rows, cols, pixel_width: 0, pixel_height: 0 };
        #[cfg(unix)]
        if let Some(master) = lock(&s.pty).as_ref() {
            master.resize(to).map_err(|e| format!("Gizai couldn't resize the terminal: {e}"))?;
        }
        #[cfg(windows)]
        s.todo(Todo::Resize(to));
        Ok(())
    }

    /// The window has written `bytes` more of the terminal's output: reading goes on once fewer than 200,000 are left
    /// unacknowledged.
    pub fn ack(&self, id: &str, bytes: u32) -> Result<(), String> {
        let s = self.find(id)?;
        let mut out = lock(&s.out);
        out.unacked = out.unacked.saturating_sub(u64::from(bytes));
        if out.paused && out.unacked < GO_ON_BELOW {
            out.paused = false;
            s.out_moved.notify_all();
        }
        Ok(())
    }

    /// Renames the terminal: the name without spaces around it, at most 40 characters; an empty one gives the default
    /// back (the folder's name, numbered when another terminal has it).
    pub fn rename(&self, id: &str, name: &str) -> Result<TerminalSession, String> {
        let s = {
            let sessions = lock(&self.list.sessions);
            let s = sessions.iter().find(|s| s.id == id).cloned().ok_or_else(closed)?;
            let mut name = clean_name(name);
            if name.is_empty() {
                name = default_name(&s.folder_name, &sessions.iter().filter(|o| o.id != id).map(|o| o.name()).collect::<Vec<_>>());
            }
            lock(&s.state).name = name;
            s
        };
        s.tell();
        Ok(s.view())
    }

    /// Ends the terminal `id`, its shell and everything it started, and returns when they are gone: on Linux and macOS
    /// within about 3 s (see `Session::begin_end`), on Windows at once. One that is closed already is fine.
    pub fn close(&self, id: &str) -> Result<(), String> {
        let Some(s) = self.list.remove(id) else { return Ok(()) };
        let deadline = Instant::now() + KILL_AFTER;
        s.begin_end();
        s.tell();
        s.finish_end(deadline);
        Ok(())
    }

    /// Gizai quits: every terminal ends as with `close`, all at the same time (on Linux and macOS within about 3 s).
    pub fn close_all(&self) {
        let all = std::mem::take(&mut *lock(&self.list.sessions));
        let deadline = Instant::now() + KILL_AFTER;
        for s in &all {
            s.begin_end();
        }
        if let Some(s) = all.first() {
            s.tell();
        }
        for s in &all {
            s.finish_end(deadline);
        }
    }

    fn find(&self, id: &str) -> Result<Arc<Session>, String> {
        lock(&self.list.sessions).iter().find(|s| s.id == id).cloned().ok_or_else(closed)
    }
}

/// The answer for a terminal that isn't in the list (any more).
fn closed() -> String {
    "That terminal is closed".into()
}

/// One terminal: its shell, its pseudo-terminal and the threads that read, send, write and wait for it.
struct Session {
    id: String,
    folder: String,
    folder_path: String,
    folder_name: String,
    task_id: Option<String>,
    shell: String,
    opened_at: i64,
    /// The shell's process id; on Linux and macOS also the id of its process group and of its session.
    pid: u32,
    state: Mutex<State>,
    out: Mutex<Out>,
    /// Wakes the sender (output to send, a new listener, closing) and the reader (room to read again, closing).
    out_moved: Condvar,
    /// The pseudo-terminal; None once it is closed.
    pty: Mutex<Option<Box<dyn MasterPty + Send>>>,
    /// The writer's queue; None once the terminal closes.
    todo: Mutex<Option<mpsc::Sender<Todo>>>,
    /// Set when the shell may be reaped: the terminal was closed, or left the list.
    done: Mutex<bool>,
    done_moved: Condvar,
    /// The shell was reaped: its id may be another process's now, so nothing is sent to it any more.
    reaped: AtomicBool,
    /// Linux and macOS: stops the reader and the writer when the terminal closes.
    #[cfg(unix)]
    wake: Wake,
    /// Windows: the shell and everything it starts.
    #[cfg(windows)]
    job: os::Job,
    list: Weak<List>,
    changed: Changed,
}

struct State {
    name: String,
    busy: Option<String>,
    ended: Option<TerminalEnd>,
    cols: u16,
    rows: u16,
    /// Being closed, or leaving the list after `exit`.
    closing: bool,
}

#[derive(Default)]
struct Out {
    /// The last 256 KB, for a listener that attaches later.
    kept: VecDeque<u8>,
    /// Older output was dropped from `kept`.
    wrapped: bool,
    /// Read and not sent yet.
    pending: Vec<u8>,
    /// The listener; out of here while the sender calls it.
    listener: Option<Output>,
    /// A listener is attached (also while the sender calls it): what is read goes to `pending`.
    attached: bool,
    /// Counts the listeners attached, so a send to one that was replaced meanwhile doesn't bring it back.
    generation: u64,
    /// Sent, or about to be, and not acknowledged.
    unacked: u64,
    paused: bool,
    last_sent: Option<Instant>,
    closing: bool,
    /// Windows: ConPTY's first cursor-position request has come.
    #[cfg(windows)]
    asked_cursor: bool,
}

/// Work for the terminal's own thread.
enum Todo {
    Input(Vec<u8>),
    #[cfg(windows)]
    Resize(PtySize),
    /// Windows: close the pseudo console.
    #[cfg(windows)]
    Close,
}

/// How a shell ended.
enum Exit {
    Code(i32),
    /// Linux and macOS: the signal that ended it (Windows has no signals).
    #[cfg_attr(windows, allow(dead_code))]
    Signal(String),
}

#[cfg(unix)]
type Reader = std::fs::File;
#[cfg(windows)]
type Reader = Box<dyn Read + Send>;
#[cfg(unix)]
type Writer = std::fs::File;
#[cfg(windows)]
type Writer = Box<dyn Write + Send>;

impl Session {
    fn view(&self) -> TerminalSession {
        let st = lock(&self.state);
        TerminalSession {
            id: self.id.clone(),
            name: st.name.clone(),
            folder: self.folder.clone(),
            folder_path: self.folder_path.clone(),
            task_id: self.task_id.clone(),
            shell: self.shell.clone(),
            busy: st.busy.clone(),
            ended: st.ended.clone(),
            cols: st.cols,
            rows: st.rows,
            opened_at: self.opened_at,
        }
    }

    fn name(&self) -> String {
        lock(&self.state).name.clone()
    }

    /// Tells the window the whole list.
    fn tell(&self) {
        let Some(list) = self.list.upgrade() else { return };
        let _in_order = lock(&list.telling);
        (self.changed)(list.views());
    }

    fn todo(&self, t: Todo) {
        if let Some(queue) = lock(&self.todo).as_ref() {
            let _ = queue.send(t);
        }
    }

    // ---- reading ----

    fn read_loop(self: Arc<Self>, mut from: Reader) {
        let mut buf = vec![0u8; READ];
        while self.room_to_read() {
            #[cfg(unix)]
            if !self.wake.until_ready(std::os::fd::AsRawFd::as_raw_fd(&from), libc::POLLIN) {
                break;
            }
            match from.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.took_in(&buf[..n]),
                Err(e) if matches!(e.kind(), io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => {}
                // Linux: EIO once nothing has the terminal open any more
                Err(_) => break,
            }
        }
    }

    /// Waits while reading is paused. False once the terminal closes: on Linux and macOS reading stops then. On Windows
    /// it goes on, dropping what it reads, so closing the pseudo console isn't kept waiting for its last output.
    fn room_to_read(&self) -> bool {
        let mut out = lock(&self.out);
        while out.paused && !out.closing {
            out = wait(&self.out_moved, out);
        }
        !(cfg!(unix) && out.closing)
    }

    /// What was read: kept, and sent when a listener is attached.
    fn took_in(&self, bytes: &[u8]) {
        let mut out = lock(&self.out);
        if out.closing {
            return;
        }
        #[cfg(windows)]
        let answered = self.answer_cursor(&mut out, bytes);
        #[cfg(windows)]
        let bytes = &answered[..];
        out.kept.extend(bytes);
        let over = out.kept.len().saturating_sub(KEEP);
        if over > 0 {
            out.kept.drain(..over);
            out.wrapped = true;
        }
        if out.attached && !bytes.is_empty() {
            out.pending.extend_from_slice(bytes);
            out.unacked += bytes.len() as u64;
            if out.unacked > PAUSE_ABOVE {
                out.paused = true;
            }
            self.out_moved.notify_all();
        }
    }

    /// Windows: with INHERIT_CURSOR, ConPTY first asks where the cursor is (ESC [6n) and waits for the answer, and the
    /// shell, a resize and a close wait with it. The view answers when it is attached; before that, the first question
    /// is answered here (ESC [1;1R, the top left) and taken out of the output, so a view that attaches later doesn't
    /// answer it again.
    #[cfg(windows)]
    fn answer_cursor<'a>(&self, out: &mut Out, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        use std::borrow::Cow;
        const ASK: &[u8] = b"\x1b[6n";
        if out.asked_cursor {
            return Cow::Borrowed(bytes);
        }
        let Some(at) = bytes.windows(ASK.len()).position(|w| w == ASK) else { return Cow::Borrowed(bytes) };
        out.asked_cursor = true;
        if out.attached {
            return Cow::Borrowed(bytes);
        }
        self.todo(Todo::Input(b"\x1b[1;1R".to_vec()));
        let mut rest = bytes[..at].to_vec();
        rest.extend_from_slice(&bytes[at + ASK.len()..]);
        Cow::Owned(rest)
    }

    // ---- sending, and busy ----

    /// Sends what came to the listener, at most once every 8 ms, and looks at `busy` about once a second.
    fn send_loop(self: Arc<Self>) {
        let mut next_look = Instant::now() + BUSY_EVERY;
        let mut out = lock(&self.out);
        while !out.closing {
            let now = Instant::now();
            if now >= next_look {
                drop(out);
                self.look_at_busy();
                next_look = Instant::now() + BUSY_EVERY;
                out = lock(&self.out);
                continue;
            }
            let mut until = next_look;
            if !out.pending.is_empty() && out.listener.is_some() {
                let at = out.last_sent.map_or(now, |t| t + EVERY);
                if at <= now {
                    let bytes = std::mem::take(&mut out.pending);
                    let generation = out.generation;
                    let Some(mut listener) = out.listener.take() else { continue };
                    out.last_sent = Some(now);
                    // sent without the lock: a listener may acknowledge from its own call
                    drop(out);
                    let alive = listener(bytes);
                    out = lock(&self.out);
                    if out.generation == generation {
                        if alive {
                            out.listener = Some(listener);
                        } else {
                            // gone (the window reloaded): reading goes on, for the next listener
                            out.attached = false;
                            out.pending.clear();
                            out.unacked = 0;
                            out.paused = false;
                            self.out_moved.notify_all();
                        }
                    }
                    continue;
                }
                until = until.min(at);
            }
            out = wait_until(&self.out_moved, out, until);
        }
    }

    fn look_at_busy(&self) {
        if lock(&self.state).ended.is_some() || self.reaped.load(Ordering::SeqCst) {
            return;
        }
        let now = self.foreground();
        let changed = {
            let mut st = lock(&self.state);
            let changed = !st.closing && st.ended.is_none() && st.busy != now;
            if changed {
                st.busy = now;
            }
            changed
        };
        if changed {
            self.tell();
        }
    }

    /// Linux and macOS: the name of the terminal's foreground process group (tcgetpgrp) when it isn't the shell's: its
    /// leader's, else one of its processes'.
    #[cfg(unix)]
    fn foreground(&self) -> Option<String> {
        let group = lock(&self.pty).as_ref()?.process_group_leader()?;
        let group = u32::try_from(group).ok().filter(|&g| g > 1 && g != self.pid)?;
        os::process_name(group)
            .or_else(|| os::in_session(self.pid).into_iter().find(|&(_, g)| g == group).and_then(|(pid, _)| os::process_name(pid)))
    }

    /// Windows: the newest process in the terminal's Job Object other than the shell and the console host.
    #[cfg(windows)]
    fn foreground(&self) -> Option<String> {
        self.job.newest(self.pid)
    }

    // ---- writing ----

    fn write_loop(self: Arc<Self>, todo: mpsc::Receiver<Todo>, mut to: Writer) {
        while let Ok(t) = todo.recv() {
            match t {
                // a program that ended or doesn't read: what it isn't given is dropped
                Todo::Input(bytes) => {
                    let _ = self.write_all(&mut to, &bytes);
                }
                #[cfg(windows)]
                Todo::Resize(size) => {
                    if let Some(master) = lock(&self.pty).as_ref() {
                        let _ = master.resize(size);
                    }
                }
                #[cfg(windows)]
                Todo::Close => break,
            }
        }
        // Windows: the pseudo console closes here, off the window's thread: ClosePseudoConsole may wait until its last
        // output is read, which the reader goes on doing.
        #[cfg(windows)]
        {
            let pty = lock(&self.pty).take();
            drop(pty);
        }
        drop(to);
    }

    /// Linux and macOS: writes all of `bytes`, waiting while the terminal is full (the program doesn't read), until the
    /// terminal closes.
    #[cfg(unix)]
    fn write_all(&self, to: &mut Writer, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            match to.write(bytes) {
                Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if !self.wake.until_ready(std::os::fd::AsRawFd::as_raw_fd(to), libc::POLLOUT) {
                        return Err(io::ErrorKind::BrokenPipe.into());
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn write_all(&self, to: &mut Writer, bytes: &[u8]) -> io::Result<()> {
        to.write_all(bytes)?;
        to.flush()
    }

    // ---- the shell's end ----

    fn wait_loop(self: Arc<Self>, mut child: Box<dyn Child + Send + Sync>) {
        #[cfg(unix)]
        let end = match wait_unreaped(self.pid) {
            Ok(end) => Some(end),
            // it can't be waited for without reaping it: then nothing is sent to its id any more
            Err(_) => {
                let status = child.wait().ok();
                self.reaped.store(true, Ordering::SeqCst);
                status.map(|s| match s.signal() {
                    Some(sig) => Exit::Signal(sig.to_string()),
                    None => Exit::Code(s.exit_code() as i32),
                })
            }
        };
        #[cfg(windows)]
        let end = child.wait().ok().map(|s| Exit::Code(s.exit_code() as i32));
        self.shell_ended(end);
        self.until_done();
        // Linux and macOS: reaps the shell, kept until now so its id stays its session's (`os::in_session`)
        let _ = child.wait();
        self.reaped.store(true, Ordering::SeqCst);
    }

    /// The shell ended by itself: with exit code 0 the terminal leaves the list; any other end is kept as `ended`. One
    /// that is being closed is `close`'s.
    fn shell_ended(&self, end: Option<Exit>) {
        let leave = matches!(end, Some(Exit::Code(0)));
        {
            let mut st = lock(&self.state);
            if st.closing {
                return;
            }
            st.busy = None;
            if leave {
                st.closing = true;
            } else {
                let (code, why) = match end {
                    Some(Exit::Code(c)) => (Some(c), format!("{} ended with exit code {c}", self.shell)),
                    Some(Exit::Signal(sig)) => (None, format!("{} was ended by {sig}", self.shell)),
                    None => (None, format!("Gizai lost track of {}", self.shell)),
                };
                st.ended = Some(TerminalEnd { code, at: now_ms(), why: Some(why) });
            }
        }
        if leave {
            if let Some(list) = self.list.upgrade() {
                list.remove(&self.id);
            }
            self.shut();
            self.tell();
            self.set_done();
        } else {
            // Windows: the pseudo console has nothing more to show
            #[cfg(windows)]
            self.todo(Todo::Close);
            self.tell();
        }
    }

    // ---- closing ----

    /// The first step of `close`, which returns at once. Linux and macOS: SIGHUP (and SIGCONT, for a stopped job) to the
    /// terminal's foreground process group and the shell's, and to the session's other groups (background jobs); then
    /// the pseudo-terminal closes. Windows: TerminateJobObject ends the shell and everything in its job (not
    /// portable-pty's kill(), which reports success and failure the wrong way round), and the pseudo console closes.
    fn begin_end(&self) {
        {
            let mut st = lock(&self.state);
            st.closing = true;
            st.busy = None;
        }
        #[cfg(unix)]
        if !self.reaped.load(Ordering::SeqCst) {
            let front = lock(&self.pty).as_ref().and_then(|m| m.process_group_leader()).and_then(|g| u32::try_from(g).ok());
            let mut groups: Vec<u32> = front.into_iter().chain([self.pid]).collect();
            for (_, group) in os::in_session(self.pid) {
                if !groups.contains(&group) {
                    groups.push(group);
                }
            }
            for group in groups {
                os::send_signal(group, true, libc::SIGHUP);
                os::send_signal(group, true, libc::SIGCONT);
            }
        }
        #[cfg(windows)]
        self.job.terminate();
        self.shut();
    }

    /// The rest of `close`. Linux and macOS: waits until nothing runs in the terminal's session any more; at `deadline`
    /// SIGKILL goes to every process still in it (one that ignores SIGHUP, as `nohup` makes it), and it waits a moment
    /// more for them to go. Then the shell is reaped, so its id is gone too when `close` returns.
    fn finish_end(&self, deadline: Instant) {
        #[cfg(unix)]
        if !self.reaped.load(Ordering::SeqCst) {
            loop {
                let left = os::in_session(self.pid);
                if left.is_empty() {
                    break;
                }
                if Instant::now() >= deadline {
                    for (pid, _) in left {
                        os::send_signal(pid, false, libc::SIGKILL);
                    }
                    let gone_by = Instant::now() + Duration::from_secs(1);
                    while !os::in_session(self.pid).is_empty() && Instant::now() < gone_by {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.set_done();
        // Linux and macOS: the waiter reaps the shell now (at once: nothing runs in its session any more)
        #[cfg(unix)]
        {
            let reaped_by = Instant::now() + Duration::from_secs(1);
            while !self.reaped.load(Ordering::SeqCst) && Instant::now() < reaped_by {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        #[cfg(windows)]
        let _ = deadline;
    }

    /// Stops reading, sending and writing, drops what was kept, and closes the pseudo-terminal (on Windows on the
    /// terminal's own thread).
    fn shut(&self) {
        {
            let mut out = lock(&self.out);
            out.closing = true;
            out.listener = None;
            out.attached = false;
            out.pending = Vec::new();
            out.kept = VecDeque::new();
            self.out_moved.notify_all();
        }
        #[cfg(windows)]
        self.todo(Todo::Close);
        lock(&self.todo).take();
        #[cfg(unix)]
        {
            self.wake.wake();
            let pty = lock(&self.pty).take();
            drop(pty);
        }
    }

    fn set_done(&self) {
        *lock(&self.done) = true;
        self.done_moved.notify_all();
    }

    fn until_done(&self) {
        let mut done = lock(&self.done);
        while !*done {
            done = wait(&self.done_moved, done);
        }
    }
}

/// Linux and macOS: waits until the process `pid`, a child of Gizai's, ends, without reaping it: its id stays its own,
/// and its session's, until it is reaped.
#[cfg(unix)]
fn wait_unreaped(pid: u32) -> io::Result<Exit> {
    loop {
        // SAFETY: waitid writes into `info`, which we own; WNOWAIT leaves the child to be reaped later.
        let (r, info) = unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            (libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOWAIT), info)
        };
        if r == 0 {
            // SAFETY: si_status is set for a child that ended.
            let status = unsafe { info.si_status() };
            return Ok(if info.si_code == libc::CLD_EXITED { Exit::Code(status) } else { Exit::Signal(signal_name(status)) });
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::Interrupted {
            return Err(e);
        }
    }
}

/// Linux and macOS: a signal's name, as a sentence says it.
#[cfg(unix)]
fn signal_name(n: i32) -> String {
    let name = match n {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGABRT => "SIGABRT",
        libc::SIGKILL => "SIGKILL",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGPIPE => "SIGPIPE",
        libc::SIGTERM => "SIGTERM",
        _ => return format!("signal {n}"),
    };
    name.to_string()
}

/// Linux and macOS: a pipe that wakes, for good, what waits on a terminal: written once when the terminal closes, and
/// never read.
#[cfg(unix)]
struct Wake {
    read: io::PipeReader,
    write: io::PipeWriter,
    woken: AtomicBool,
}

#[cfg(unix)]
impl Wake {
    fn new() -> io::Result<Wake> {
        let (read, write) = io::pipe()?;
        Ok(Wake { read, write, woken: AtomicBool::new(false) })
    }

    fn wake(&self) {
        if !self.woken.swap(true, Ordering::SeqCst) {
            let _ = (&self.write).write(b"x");
        }
    }

    /// Waits until `fd` is ready for `events` (POLLIN, POLLOUT; or it hung up or failed, which the read or write then
    /// says): true; or until woken: false.
    fn until_ready(&self, fd: std::os::fd::RawFd, events: libc::c_short) -> bool {
        use std::os::fd::AsRawFd;
        loop {
            let mut fds = [libc::pollfd { fd, events, revents: 0 }, libc::pollfd { fd: self.read.as_raw_fd(), events: libc::POLLIN, revents: 0 }];
            // SAFETY: poll reads and writes the two entries, which live across the call.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
            if n < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return false;
            }
            if fds[1].revents != 0 {
                return false;
            }
            if fds[0].revents != 0 {
                return true;
            }
        }
    }
}

/// Linux and macOS: the reader's and the writer's own descriptors for the terminal's master side: non-blocking, so
/// they wait with poll and closing can stop them (`Wake`), and closed in programs Gizai starts.
#[cfg(unix)]
fn own_ends(master: &(dyn MasterPty + Send)) -> io::Result<(std::fs::File, std::fs::File)> {
    let fd = master.as_raw_fd().ok_or_else(|| io::Error::other("the terminal has no descriptor"))?;
    // SAFETY: plain fcntl calls on a descriptor the master keeps open.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok((copy_fd(fd)?, copy_fd(fd)?))
}

/// Linux and macOS: a copy of the descriptor `fd`, closed on exec, owned by the File returned.
#[cfg(unix)]
fn copy_fd(fd: std::os::fd::RawFd) -> io::Result<std::fs::File> {
    use std::os::fd::FromRawFd;
    // SAFETY: F_DUPFD_CLOEXEC makes a new descriptor, which only the File made here owns.
    unsafe {
        let copy = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0);
        if copy < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(std::fs::File::from_raw_fd(copy))
    }
}

/// What the terminal kept, to send first to a new listener: from the start of a line once older output was dropped (in
/// the first 4 KB), so it doesn't start in the middle of a character or an escape sequence.
fn kept(kept: &VecDeque<u8>, wrapped: bool) -> Vec<u8> {
    let mut bytes: Vec<u8> = kept.iter().copied().collect();
    if wrapped && let Some(nl) = bytes.iter().take(4096).position(|&b| b == b'\n') {
        bytes.drain(..=nl);
    }
    bytes
}

/// A name as kept: on one line, without spaces around it, at most 40 characters.
fn clean_name(name: &str) -> String {
    let line: String = name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    line.trim().chars().take(NAME_MAX).collect::<String>().trim_end().to_string()
}

/// The folder's name, or with the lowest number no open terminal has: "gizai", "gizai 2", "gizai 3".
fn default_name(base: &str, taken: &[String]) -> String {
    let base = if base.trim().is_empty() { "Terminal" } else { base };
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2u32..).map(|n| format!("{base} {n}")).find(|n| !taken.contains(n)).unwrap_or_else(|| base.to_string())
}

/// What the window calls the shell: "zsh", "bash", "pwsh", "cmd".
fn shell_name(program: &Path) -> String {
    let name = if cfg!(windows) { program.file_stem() } else { program.file_name() };
    let name = name.map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| program.display().to_string());
    if cfg!(windows) { name.to_ascii_lowercase() } else { name }
}

/// A size as the pseudo-terminal takes it: 0 is `otherwise`.
fn size(n: u16, otherwise: u16) -> u16 {
    if n == 0 { otherwise } else { n.min(SIZE_MAX) }
}

/// A terminal's id: 16 random hex characters.
fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        // never the same twice in one Gizai
        bytes = (now_ms() as u64 ^ NEXT.fetch_add(1, Ordering::SeqCst).rotate_left(40)).to_be_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A thread of the terminal's.
fn thread(what: &str, f: impl FnOnce() + Send + 'static) -> io::Result<()> {
    std::thread::Builder::new().name(format!("terminal {what}")).spawn(f).map(|_| ())
}

/// A lock that a thread which panicked while holding it doesn't break for the others.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn wait<'a, T>(cv: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    cv.wait(guard).unwrap_or_else(|e| e.into_inner())
}

fn wait_until<'a, T>(cv: &Condvar, guard: MutexGuard<'a, T>, until: Instant) -> MutexGuard<'a, T> {
    cv.wait_timeout(guard, until.saturating_duration_since(Instant::now())).unwrap_or_else(|e| e.into_inner()).0
}
