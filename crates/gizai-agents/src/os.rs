//! What differs between Linux, macOS and Windows when Gizai starts a program: finding it, starting it without a
//! console window, and ending it with everything it started (`Tree`) (docs/PLATFORMS.md). On Linux and macOS these are
//! the plain std calls and process groups.
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(windows)]
use std::sync::Arc;

/// Windows: a console program started by Gizai (a window app) gets no console window of its own.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A Command for `program` (a name or a path) that works on every system. On Windows no console window opens, and an
/// npm `.cmd` shim (Gemini, Codex, an npm-installed Claude Code) runs as `node <its script>`, so arguments with spaces
/// and quotes reach the CLI unchanged; another `.cmd` or `.bat` goes through Rust's own batch-file quoting.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let program = program.as_ref();
        let found = if Path::new(program).components().count() > 1 {
            Some(PathBuf::from(program)).filter(|p| p.is_file()).or_else(|| with_extension(Path::new(program)))
        } else {
            find_in(&program.to_string_lossy(), &std::env::var_os("PATH").unwrap_or_default())
        };
        let mut cmd = match found.as_deref().and_then(npm_shim) {
            Some((node, script)) => {
                let mut c = Command::new(node);
                c.arg(script);
                c
            }
            None => Command::new(found.as_deref().map(Path::as_os_str).unwrap_or(program)),
        };
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    Command::new(program)
}

/// `command` for Tokio.
pub fn tokio_command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    tokio::process::Command::from(command(program))
}

/// Whether `p` is a program this system runs: a file with an execute bit on Linux and macOS; on Windows a file whose
/// extension is in PATHEXT (`.exe`, `.cmd`, `.bat`, `.com`).
pub fn executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(windows)]
    {
        let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy()).to_ascii_lowercase());
        p.is_file() && ext.is_some_and(|e| path_exts().contains(&e))
    }
}

/// Where `name` (a program's name, or a path) is found: a path as it is when it is a program, else the first folder in
/// `path` that has it. On Windows a name without an extension is tried with each PATHEXT extension (`claude` →
/// `claude.exe`, `gemini` → `gemini.cmd`).
pub fn find_in(name: &str, path: &OsStr) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let p = Path::new(name);
    if p.components().count() > 1 || p.is_absolute() {
        if executable(p) {
            return Some(p.to_path_buf());
        }
        #[cfg(windows)]
        return with_extension(p);
        #[cfg(not(windows))]
        return None;
    }
    std::env::split_paths(path).filter(|d| !d.as_os_str().is_empty()).find_map(|d| {
        let candidate = d.join(name);
        if executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        return with_extension(&candidate);
        #[cfg(not(windows))]
        None
    })
}

/// Windows: `p` with the first PATHEXT extension that makes it a file.
#[cfg(windows)]
fn with_extension(p: &Path) -> Option<PathBuf> {
    path_exts().iter().map(|e| PathBuf::from(format!("{}{e}", p.display()))).find(|c| c.is_file())
}

/// Windows: the extensions that make a program, lower case with their dot (PATHEXT, else the usual four).
#[cfg(windows)]
fn path_exts() -> Vec<String> {
    let own = std::env::var("PATHEXT").unwrap_or_default();
    let exts: Vec<String> = own.split(';').map(|e| e.trim().to_ascii_lowercase()).filter(|e| e.starts_with('.') && e.len() > 1).collect();
    if exts.is_empty() { [".com", ".exe", ".bat", ".cmd"].map(String::from).to_vec() } else { exts }
}

/// The script an npm `.cmd` shim runs, read from the shim's text: its path relative to the shim's folder, like
/// `node_modules\@google\gemini-cli\dist\index.js`. None for any other text. Plain text work, the same on every system
/// (so it can be tested anywhere); `npm_shim` uses it on Windows.
pub fn npm_shim_script(cmd_text: &str) -> Option<&str> {
    // npm's cmd-shim ends with: "%_prog%"  "%dp0%\node_modules\<package>\<script>.js" %*
    let start = cmd_text.rfind("\"%dp0%\\")? + "\"%dp0%\\".len();
    let rel = &cmd_text[start..start + cmd_text[start..].find('"')?];
    (rel.ends_with(".js") || rel.ends_with(".cjs") || rel.ends_with(".mjs")).then_some(rel)
}

/// Windows: for an npm `.cmd` shim, the node it runs (the `node.exe` next to it, else node on PATH) and its script.
/// None for any other file.
#[cfg(windows)]
pub fn npm_shim(cmd_file: &Path) -> Option<(PathBuf, PathBuf)> {
    let ext = cmd_file.extension()?.to_string_lossy().to_ascii_lowercase();
    if ext != "cmd" {
        return None;
    }
    let dir = cmd_file.parent()?;
    let text = std::fs::read_to_string(cmd_file).ok().filter(|t| t.len() < 16 * 1024)?;
    let script = dir.join(npm_shim_script(&text)?);
    if !script.is_file() {
        return None;
    }
    let local = dir.join("node.exe");
    let node = if local.is_file() { local } else { find_in("node", &std::env::var_os("PATH").unwrap_or_default())? };
    Some((node, script))
}

/// The bash that runs shell commands (a worktree's prepare commands, your GIT_SSH_COMMAND): `bash` on PATH on Linux and
/// macOS. On Windows Git Bash, found the way Claude Code 2.1 finds it, so both use the same one: CLAUDE_CODE_GIT_BASH_PATH
/// when that file exists, then `%ProgramFiles%\Git\bin\bash.exe`, `%ProgramFiles(x86)%\Git\bin\bash.exe`, the one next
/// to git on `path` (`<git>\..\..\bin\bash.exe`), and last, where Claude Code doesn't look, Git for Windows' install for
/// one user (`%LOCALAPPDATA%\Programs\Git\bin\bash.exe`). Never `C:\Windows\System32\bash.exe`: that one starts WSL,
/// which Gizai doesn't use.
pub fn git_bash(path: &OsStr) -> Option<PathBuf> {
    #[cfg(not(windows))]
    {
        let _ = path;
        Some(PathBuf::from("bash"))
    }
    #[cfg(windows)]
    {
        if let Some(p) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from).filter(|p| p.is_file()) {
            return Some(p);
        }
        let in_git = |git_root: PathBuf| Some(git_root.join("bin").join("bash.exe")).filter(|b| b.is_file());
        let program_files = |var: &str, usual: &str| std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(usual)).join("Git");
        in_git(program_files("ProgramFiles", r"C:\Program Files"))
            .or_else(|| in_git(program_files("ProgramFiles(x86)", r"C:\Program Files (x86)")))
            // <Git>\cmd\git.exe or <Git>\bin\git.exe: bash is <Git>\bin\bash.exe
            .or_else(|| find_in("git", path).and_then(|git| git.parent()?.parent().map(Path::to_path_buf)).and_then(in_git))
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Programs").join("Git")).and_then(in_git))
    }
}

/// How hard `Tree::end` asks. On Linux and macOS a signal to the tree's process group. On Windows each one ends the
/// whole job at once: Gizai is a window app without a console, and Windows has no signal such an app can send a console
/// program (Ctrl+C only reaches the programs of the console it is pressed in), so there is no gentle step there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    /// SIGINT, what Ctrl+C sends: a CLI stops its turn and exits.
    Interrupt,
    /// SIGTERM.
    Terminate,
    /// SIGKILL, which no program can ignore.
    Kill,
}

/// A program Gizai started with everything it starts in turn, checked on and ended as one (`spawn_tree`).
/// - Linux and macOS: the process group the program leads. Signals only ever go to that group, never to pid 0 or 1.
/// - Windows: a Job Object. Every process the program starts is in it too and can't leave it, and the job ends what
///   still runs in it when its last handle closes (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`): when the last clone of the
///   `Tree` drops, or when Gizai exits or crashes, so nothing outlives Gizai. Keep a clone for as long as the program is
///   tracked; dropping the last one after it ended is what ends its leftovers.
#[derive(Clone, Debug)]
pub struct Tree {
    #[cfg(unix)]
    group: u32,
    #[cfg(windows)]
    job: Arc<Job>,
}

impl Tree {
    /// Linux and macOS: the process group `pid` leads (a run a previous Gizai started).
    #[cfg(unix)]
    pub fn led_by(pid: u32) -> Tree {
        Tree { group: pid }
    }

    /// Asks everything in the tree to end (see `End`), and returns at once.
    pub fn end(&self, how: End) {
        #[cfg(unix)]
        {
            if let Ok(g) = i32::try_from(self.group)
                && g > 1
            {
                let sig = match how {
                    End::Interrupt => libc::SIGINT,
                    End::Terminate => libc::SIGTERM,
                    End::Kill => libc::SIGKILL,
                };
                // SAFETY: plain syscall; a negative pid addresses the process group started for this program.
                unsafe {
                    libc::kill(-g, sig);
                }
            }
        }
        #[cfg(windows)]
        {
            let _ = how;
            self.job.terminate();
        }
    }

    /// Whether anything in the tree still runs. On Linux and macOS the program Gizai started counts until it has been
    /// waited for (reaped).
    pub fn alive(&self) -> bool {
        #[cfg(unix)]
        return match i32::try_from(self.group) {
            // SAFETY: signal 0 only checks that the group exists; nothing is sent.
            Ok(g) if g > 1 => unsafe { libc::kill(-g, 0) == 0 },
            _ => false,
        };
        #[cfg(windows)]
        return self.job.active() > 0;
    }
}

/// Starts `cmd` as a `Tree` of its own: it and everything it starts. `low`: at low CPU priority, which what it starts
/// inherits (nice 10 on Linux and macOS, below normal on Windows).
///
/// Windows: the program is in its job before any of its code runs. It starts suspended, goes into the job, and then its
/// first thread is resumed (std keeps no handle to that thread, so a Toolhelp32 snapshot of the system's threads finds
/// it). Putting it in the job after a normal start would leave a moment in which something it starts escapes the job.
/// This sets the command's creation flags, which replaces the ones it had: no console window (see `command`) is set
/// again.
pub fn spawn_tree(cmd: &mut Command, low: bool) -> io::Result<(std::process::Child, Tree)> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        if low {
            // SAFETY: nice(2) is a plain syscall, safe to make between fork and exec.
            unsafe {
                cmd.pre_exec(|| {
                    libc::nice(10);
                    Ok(())
                });
            }
        }
        let child = cmd.spawn()?;
        let group = child.id();
        Ok((child, Tree { group }))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        let job = Job::new()?;
        cmd.creation_flags(start_flags(low));
        let mut child = cmd.spawn()?;
        match job.adopt(child.as_raw_handle(), child.id()) {
            Ok(()) => Ok((child, Tree { job: Arc::new(job) })),
            Err(e) => {
                // still suspended, or running outside the job: it goes either way
                if child.kill().is_ok() {
                    let _ = child.wait();
                }
                Err(e)
            }
        }
    }
}

/// `spawn_tree` for Tokio, at normal priority. Must be called inside a Tokio runtime.
pub fn spawn_tree_tokio(cmd: &mut tokio::process::Command) -> io::Result<(tokio::process::Child, Tree)> {
    #[cfg(unix)]
    {
        cmd.process_group(0);
        let child = cmd.spawn()?;
        // None only for a child that has been waited for
        let group = child.id().unwrap_or(0);
        Ok((child, Tree { group }))
    }
    #[cfg(windows)]
    {
        let job = Job::new()?;
        cmd.creation_flags(start_flags(false));
        let mut child = cmd.spawn()?;
        let adopted = match (child.raw_handle(), child.id()) {
            (Some(process), Some(pid)) => job.adopt(process, pid),
            _ => Err(io::Error::other("it ended at once")),
        };
        match adopted {
            Ok(()) => Ok((child, Tree { job: Arc::new(job) })),
            Err(e) => {
                let _ = child.start_kill();
                Err(e)
            }
        }
    }
}

/// Windows: the exit code of the programs in a job Gizai ends (there is no signal to report instead).
#[cfg(windows)]
const ENDED: u32 = 1;

/// Windows: the creation flags of a program started as a `Tree`.
#[cfg(windows)]
fn start_flags(low: bool) -> u32 {
    use windows_sys::Win32::System::Threading::{BELOW_NORMAL_PRIORITY_CLASS, CREATE_SUSPENDED};
    CREATE_NO_WINDOW | CREATE_SUSPENDED | if low { BELOW_NORMAL_PRIORITY_CLASS } else { 0 }
}

/// Windows: a Job Object's handle, closed on drop, which ends what still runs in the job.
#[cfg(windows)]
#[derive(Debug)]
struct Job(windows_sys::Win32::Foundation::HANDLE);

// SAFETY: a kernel handle may be used, and closed, on any thread.
#[cfg(windows)]
unsafe impl Send for Job {}
#[cfg(windows)]
unsafe impl Sync for Job {}

#[cfg(windows)]
impl Job {
    /// A new job that ends everything in it when its last handle closes. Its handle isn't inherited, so only Gizai holds
    /// it, and it allows no breakaway: a program in it can't start anything outside it.
    fn new() -> io::Result<Job> {
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };
        // SAFETY: plain calls with valid pointers; from here on the handle belongs to the Job, which closes it.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Job(handle);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
            if SetInformationJobObject(job.0, JobObjectExtendedLimitInformation, (&raw const limits).cast(), size) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }
    }

    /// Puts the suspended process `process` (id `pid`) in the job, then resumes its threads.
    fn adopt(&self, process: windows_sys::Win32::Foundation::HANDLE, pid: u32) -> io::Result<()> {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next};
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
        use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
        // SAFETY: plain calls with valid handles and pointers; every handle opened here is closed here.
        unsafe {
            if AssignProcessToJobObject(self.0, process) == 0 {
                return Err(io::Error::last_os_error());
            }
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let mut entry = THREADENTRY32 { dwSize: std::mem::size_of::<THREADENTRY32>() as u32, ..Default::default() };
            let mut resumed = 0;
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if entry.th32OwnerProcessID == pid {
                    let thread = OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID);
                    if !thread.is_null() {
                        if ResumeThread(thread) != u32::MAX {
                            resumed += 1;
                        }
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
            if resumed == 0 {
                return Err(io::Error::other("Windows didn't let Gizai resume the program it started"));
            }
            Ok(())
        }
    }

    /// How many processes run in the job now (0 when Windows doesn't say).
    fn active(&self) -> u32 {
        use windows_sys::Win32::System::JobObjects::{JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation, QueryInformationJobObject};
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        let size = std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32;
        // SAFETY: a plain call with a valid handle and a buffer of the size given.
        let ok = unsafe { QueryInformationJobObject(self.0, JobObjectBasicAccountingInformation, (&raw mut info).cast(), size, std::ptr::null_mut()) };
        if ok == 0 { 0 } else { info.ActiveProcesses }
    }

    /// Ends every process in the job.
    fn terminate(&self) {
        // SAFETY: a plain call with a valid handle.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, ENDED);
        }
    }
}

#[cfg(windows)]
impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is the Job's own and is closed once. With KILL_ON_JOB_CLOSE this ends what still runs in it.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
