//! Memory for agent runs (GA-202, docs/PLATFORMS.md): one limit for all of a Gizai's agent runs together, Settings →
//! Runs. On Linux with a systemd user session every run, chat answer, board check and worktree preparation starts in a
//! scope of its own inside one slice per data folder (`slice_name`), whose MemoryMax is the limit and MemoryHigh 90% of
//! it: the runs are slowed down first, and only at the limit does Linux end a process, one inside the slice, never
//! Gizai. On every system the start check reads what the runs use and what the computer has free (`Reading`).
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::os;

/// A gigabyte as Settings → Runs counts it: 2³⁰ bytes, as systemd's `G` and `free -g` count.
pub const GB: u64 = 1 << 30;

/// Where Linux shows the unified cgroup hierarchy (cgroup v2).
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// How long systemctl or a slice's first scope may take.
const QUICK: Duration = Duration::from_secs(15);

/// This computer's memory in bytes: MemTotal in /proc/meminfo on Linux, hw.memsize on macOS, the physical memory
/// Windows reports (GlobalMemoryStatusEx). None when the system doesn't say.
pub fn total() -> Option<u64> {
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| meminfo_value(&t, "MemTotal"));
    #[cfg(target_os = "macos")]
    return sysctl_u64("hw.memsize");
    #[cfg(windows)]
    return memory_status().map(|m| m.ullTotalPhys);
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    None
}

/// The memory this computer has free for new work now, in bytes: MemAvailable in /proc/meminfo on Linux; on macOS the
/// share of hw.memsize its memory pressure counts as available (kern.memorystatus_level); on Windows the available
/// physical memory (GlobalMemoryStatusEx). None when the system doesn't say.
pub fn available() -> Option<u64> {
    #[cfg(target_os = "linux")]
    return std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| meminfo_value(&t, "MemAvailable"));
    #[cfg(target_os = "macos")]
    return mac_available();
    #[cfg(windows)]
    return memory_status().map(|m| m.ullAvailPhys);
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    None
}

/// A value in /proc/meminfo's text, in bytes ("MemAvailable:   48729820 kB"). Plain text work, the same on every system.
pub fn meminfo_value(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let mut words = l.strip_prefix(key)?.strip_prefix(':')?.split_whitespace();
        let n: u64 = words.next()?.parse().ok()?;
        Some(if words.next() == Some("kB") { n.saturating_mul(1024) } else { n })
    })
}

/// A value in a cgroup's memory.stat or memory.events text ("anon 123456", "oom_kill 1"). Plain text work.
pub fn stat_value(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once(' ')?;
        if k == key { v.trim().parse().ok() } else { None }
    })
}

/// macOS: a 64-bit sysctl (hw.memsize).
#[cfg(target_os = "macos")]
fn sysctl_u64(name: &str) -> Option<u64> {
    let name = std::ffi::CString::new(name).ok()?;
    let mut value: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: sysctlbyname writes at most `size` bytes into `value` and sets `size` to how many it wrote.
    let ok = unsafe { libc::sysctlbyname(name.as_ptr(), (&raw mut value).cast(), &mut size, std::ptr::null_mut(), 0) } == 0;
    (ok && size == std::mem::size_of::<u64>()).then_some(value)
}

/// macOS: what is available, from the share of memory the kernel's memory pressure counts as available (0 to 100).
#[cfg(target_os = "macos")]
fn mac_available() -> Option<u64> {
    let name = std::ffi::CString::new("kern.memorystatus_level").ok()?;
    let mut level: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    // SAFETY: sysctlbyname writes at most `size` bytes into `level` and sets `size` to how many it wrote.
    let ok = unsafe { libc::sysctlbyname(name.as_ptr(), (&raw mut level).cast(), &mut size, std::ptr::null_mut(), 0) } == 0;
    if !ok || size != std::mem::size_of::<libc::c_int>() {
        return None;
    }
    Some(total()? / 100 * level.clamp(0, 100) as u64)
}

/// Windows: the system's memory status.
#[cfg(windows)]
fn memory_status() -> Option<windows_sys::Win32::System::SystemInformation::MEMORYSTATUSEX> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: a plain call with a buffer of the size its dwLength gives.
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status)
}

/// The slice for the agent runs of the Gizai whose data folder is `data_dir`: `gizai-runs-` and the first 8 hex digits
/// of the SHA-256 of the folder's path (the same folder however it is spelled, as the data folder's key in
/// `src-tauri/src/lib.rs`), so a test Gizai with its own data folder never touches the slice of the Gizai its agents run
/// in. The dashes put it in `gizai.slice` and `gizai-runs.slice`, which systemd makes when it is first used.
pub fn slice_name(data_dir: &Path) -> String {
    use sha2::{Digest, Sha256};
    let canon = data_dir.canonicalize().unwrap_or_else(|_| data_dir.to_path_buf());
    let hash = Sha256::digest(canon.to_string_lossy().as_bytes());
    let hex: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("gizai-runs-{hex}.slice")
}

/// The slice's MemoryMax and MemoryHigh (90% of it) for a limit of `max` bytes, both in whole 4 KiB pages, as the kernel
/// keeps them: memory.max and memory.high read back these numbers.
pub fn limits_for(max: u64) -> (u64, u64) {
    const PAGE: u64 = 4096;
    let max = max / PAGE * PAGE;
    (max, max / 10 * 9 / PAGE * PAGE)
}

/// A Gizai's slice for its agent runs (Linux with a systemd user session), from `Slice::set_up`.
#[derive(Debug, Clone)]
pub struct Slice {
    name: String,
    /// Its cgroup folder: /sys/fs/cgroup/user.slice/user-<uid>.slice/user@<uid>.service/gizai.slice/gizai-runs.slice/<name>.
    dir: PathBuf,
    /// systemd-run and systemctl, found on Gizai's PATH when the slice was set up.
    systemd_run: PathBuf,
    systemctl: PathBuf,
    /// The scopes get OOMPolicy=continue (systemd 253 and newer): when Linux ends a process at the limit, the rest of its
    /// run goes on. Older systemd has no OOM policy for scopes and lets them run on anyway.
    oom_continue: bool,
}

impl Slice {
    /// Sets the slice `name` up for agent runs with a limit of `max` bytes: its MemoryMax and MemoryHigh (90%) through
    /// your user's systemd (`systemctl --user set-property --runtime`, which lasts until you log out; Gizai sets them
    /// at each start), then a first scope in it (`systemd-run --user --scope`) that says where the slice's cgroup is,
    /// and the kernel's memory.max there. Err says why it can't be: no systemd (another init system, a container), no
    /// user session, no cgroup v2, the memory controller off. Linux only; elsewhere always Err.
    pub fn set_up(name: &str, max: u64) -> Result<Slice, String> {
        #[cfg(target_os = "linux")]
        {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let find = |p: &str| os::find_in(p, &path).ok_or_else(|| format!("{p} isn't installed, so this Linux has no systemd"));
            let (systemd_run, systemctl) = (find("systemd-run")?, find("systemctl")?);
            if std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|d| d.is_empty()) {
                return Err("there is no systemd user session (XDG_RUNTIME_DIR isn't set)".into());
            }
            let mut slice = Slice { name: name.to_string(), dir: PathBuf::new(), systemd_run, systemctl, oom_continue: true };
            slice.set_properties(max)?;
            // systemd 253 and newer take an OOM policy for a scope; older ones refuse the property
            let cgroup = match slice.probe() {
                Ok(c) => c,
                Err(_) => {
                    slice.oom_continue = false;
                    slice.probe()?
                }
            };
            slice.dir = slice_dir(&cgroup, name).ok_or_else(|| format!("a scope in {name} started in {cgroup} instead"))?;
            slice.check(max)?;
            Ok(slice)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (name, max);
            Err("only Linux enforces a memory limit for agent runs".into())
        }
    }

    /// Its unit name, `gizai-runs-<8 hex digits>.slice`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its cgroup folder under /sys/fs/cgroup.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A new limit of `max` bytes: MemoryMax and MemoryHigh (90%) change together, at once, for the runs at work too.
    pub fn set_limit(&self, max: u64) -> Result<(), String> {
        self.set_properties(max)?;
        self.check(max)
    }

    /// What the slice's processes use now, without the file cache: `anon` in its memory.stat. The cache fills up to the
    /// limit and is handed back when needed, so it doesn't count.
    pub fn used(&self) -> Option<u64> {
        stat_value(&std::fs::read_to_string(self.dir.join("memory.stat")).ok()?, "anon")
    }

    /// Its MemoryMax and MemoryHigh as the kernel has them (memory.max, memory.high), in bytes; None for `max` (none)
    /// or when they can't be read.
    pub fn limits(&self) -> (Option<u64>, Option<u64>) {
        let read = |f: &str| std::fs::read_to_string(self.dir.join(f)).ok().and_then(|t| t.trim().parse().ok());
        (read("memory.max"), read("memory.high"))
    }

    /// A Command that starts `program` in a scope of its own in this slice: `systemd-run --user --scope`, which runs it
    /// in the same process once the scope is made, so its pid, process group, priority, environment, folder and open
    /// files stay as they are. Add the program's arguments to it.
    pub fn command(&self, program: &OsStr) -> Command {
        let mut cmd = os::command(&self.systemd_run);
        cmd.args(["--user", "--scope", "--collect", "--quiet"]).arg(format!("--slice={}", self.name));
        if self.oom_continue {
            cmd.args(["-p", "OOMPolicy=continue"]);
        }
        cmd.arg("--").arg(program);
        cmd
    }

    /// `command` for `program` (a name, found on `path`, else on Gizai's own PATH; or a path), or None when it can't be
    /// found: then it starts as before, outside the slice, and fails the usual way.
    pub fn command_for(&self, program: &OsStr, path: Option<&OsStr>) -> Option<Command> {
        let own = std::env::var_os("PATH").unwrap_or_default();
        let found = os::find_in(&program.to_string_lossy(), path.unwrap_or(own.as_os_str()))?;
        Some(self.command(found.as_os_str()))
    }

    /// Tests: stops the slice, with everything still running in it, and removes the limits Gizai set on it, so a test's
    /// own slice leaves nothing behind. Never for the slice of a Gizai in use.
    #[doc(hidden)]
    pub fn remove(&self) -> Result<(), String> {
        quick(os::command(&self.systemctl).args(["--user", "stop", &self.name]))?;
        quick(os::command(&self.systemctl).args(["--user", "revert", &self.name])).map(|_| ())
    }

    /// MemoryMax and MemoryHigh, in one call.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn set_properties(&self, max: u64) -> Result<(), String> {
        let (max, high) = limits_for(max);
        let mut cmd = os::command(&self.systemctl);
        cmd.args(["--user", "set-property", "--runtime", &self.name, &format!("MemoryMax={max}"), &format!("MemoryHigh={high}")]);
        quick(&mut cmd).map(|_| ()).map_err(|e| format!("systemctl --user set-property {} failed: {e}", self.name))
    }

    /// Starts `cat /proc/self/cgroup` in a scope of this slice: the cgroup it ran in, cgroup v2's `0::` line.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn probe(&self) -> Result<String, String> {
        let cat = os::find_in("cat", &std::env::var_os("PATH").unwrap_or_default()).ok_or("cat isn't installed")?;
        let out = quick(self.command(cat.as_os_str()).arg("/proc/self/cgroup"))
            .map_err(|e| format!("systemd-run --user --scope couldn't start a scope in {}: {e}", self.name))?;
        String::from_utf8_lossy(&out).lines().find_map(|l| l.strip_prefix("0::")).map(|p| p.trim().to_string())
            .ok_or_else(|| "this Linux has no unified cgroup hierarchy (cgroup v2)".to_string())
    }

    /// The kernel keeps `max` as the slice's memory.max.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn check(&self, max: u64) -> Result<(), String> {
        let file = self.dir.join("memory.max");
        let text = std::fs::read_to_string(&file)
            .map_err(|_| format!("{} has no memory.max: the memory controller is off for it", self.name))?;
        // a kernel with bigger pages than 4 KiB rounds the limit down to its own pages
        match text.trim().parse::<u64>() {
            Ok(v) if v.abs_diff(limits_for(max).0) < 64 * 1024 => Ok(()),
            _ => Err(format!("systemd didn't give {} its MemoryMax: its memory.max is {}", self.name, text.trim())),
        }
    }

    /// The cgroup folder of the scope the process `pid` runs in, once it is inside this slice.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    fn scope_of(&self, pid: u32) -> Option<PathBuf> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()?;
        let at = Path::new(CGROUP_ROOT).join(text.lines().find_map(|l| l.strip_prefix("0::"))?.trim().trim_start_matches('/'));
        (at.starts_with(&self.dir) && at != self.dir).then_some(at)
    }
}

/// The slice `name`'s cgroup folder, from the cgroup of a scope in it ("/user.slice/…/gizai-runs-1a2b3c4d.slice/run-…").
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn slice_dir(cgroup: &str, name: &str) -> Option<PathBuf> {
    let at = cgroup.find(&format!("/{name}/"))?;
    Some(Path::new(CGROUP_ROOT).join(cgroup[..at + 1 + name.len()].trim_start_matches('/')))
}

/// Runs `cmd` (systemctl, a slice's first scope) without input, within `QUICK`: what it printed, or why it failed (the
/// last line it wrote to stderr).
fn quick(cmd: &mut Command) -> Result<Vec<u8>, String> {
    use std::io::Read;
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let until = Instant::now() + QUICK;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("it didn't answer within {} seconds", QUICK.as_secs()));
            }
        }
    };
    let (mut out, mut err) = (vec![], String::new());
    if let Some(mut o) = child.stdout.take() {
        let _ = o.read_to_end(&mut out);
    }
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    if status.success() {
        return Ok(out);
    }
    let said = err.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or_default().to_string();
    Err(if said.is_empty() { format!("it ended with {status}") } else { said })
}

/// Watches the scope a process started in through `Slice::command` for processes Linux ended at the slice's limit: the
/// `oom_kill` count in the scope's memory.events, which the kernel raises for each process it ends there. `on_kill`
/// hears each rise with the count so far, as it happens (inotify; a look every 250 ms without it). Elsewhere it watches
/// nothing.
pub struct OomWatch {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<u64>>,
}

impl OomWatch {
    /// Watches the scope of `pid`, a process just started with `slice`'s `command`.
    pub fn start(slice: &Slice, pid: u32, on_kill: impl FnMut(u64) + Send + 'static) -> OomWatch {
        let stop = Arc::new(AtomicBool::new(false));
        #[cfg(target_os = "linux")]
        let thread = {
            let (slice, stopped) = (slice.clone(), stop.clone());
            std::thread::Builder::new().name("gizai-oom-watch".into()).spawn(move || watch(&slice, pid, &stopped, on_kill)).ok()
        };
        #[cfg(not(target_os = "linux"))]
        let thread = {
            let _ = (slice, pid, on_kill);
            None
        };
        OomWatch { stop, thread }
    }

    /// Stops watching after a last look, and returns how many processes Linux ended in the scope (0 when it never
    /// found the scope). Blocks for at most 250 ms.
    pub fn finish(mut self) -> u64 {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().and_then(|t| t.join().ok()).unwrap_or(0)
    }
}

impl Drop for OomWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// The watch's thread: finds the scope of `pid` (systemd-run moves it there before the program runs; at most 10 s),
/// then reads its memory.events whenever it changes, until `stop` or until the scope is gone.
#[cfg(target_os = "linux")]
fn watch(slice: &Slice, pid: u32, stop: &AtomicBool, mut on_kill: impl FnMut(u64)) -> u64 {
    let until = Instant::now() + Duration::from_secs(10);
    let scope = loop {
        if let Some(s) = slice.scope_of(pid) {
            break s;
        }
        if stop.load(Ordering::SeqCst) || Instant::now() > until || !Path::new(&format!("/proc/{pid}")).exists() {
            return 0;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let events = scope.join("memory.events");
    let changes = Inotify::watch(&events);
    let mut seen = 0;
    loop {
        let last = stop.load(Ordering::SeqCst);
        match std::fs::read_to_string(&events).ok().and_then(|t| stat_value(&t, "oom_kill")) {
            Some(n) if n > seen => {
                seen = n;
                on_kill(n);
            }
            Some(_) => {}
            // the scope is gone: everything in it has ended
            None => break,
        }
        if last {
            break;
        }
        match &changes {
            Some(c) => c.wait(Duration::from_millis(250)),
            None => std::thread::sleep(Duration::from_millis(250)),
        }
    }
    seen
}

/// Linux: an inotify watch on one file. The kernel raises IN_MODIFY on a cgroup's memory.events when a count in it
/// changes.
#[cfg(target_os = "linux")]
struct Inotify(libc::c_int);

#[cfg(target_os = "linux")]
impl Inotify {
    fn watch(file: &Path) -> Option<Inotify> {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(file.as_os_str().as_bytes()).ok()?;
        // SAFETY: plain syscalls with a NUL-terminated path; the descriptor belongs to the Inotify, which closes it.
        unsafe {
            let fd = libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK);
            if fd < 0 {
                return None;
            }
            let me = Inotify(fd);
            (libc::inotify_add_watch(fd, path.as_ptr(), libc::IN_MODIFY) >= 0).then_some(me)
        }
    }

    /// Waits until the file changes or `timeout` passes, and takes the events.
    fn wait(&self, timeout: Duration) {
        let mut fds = libc::pollfd { fd: self.0, events: libc::POLLIN, revents: 0 };
        let mut buf = [0u8; 1024];
        // SAFETY: poll and read on the Inotify's own descriptor, with buffers of the sizes given.
        unsafe {
            if libc::poll(&mut fds, 1, timeout.as_millis() as libc::c_int) > 0 {
                while libc::read(self.0, buf.as_mut_ptr().cast(), buf.len()) > 0 {}
            }
        }
    }
}

#[cfg(target_os = "linux")]
impl Drop for Inotify {
    fn drop(&mut self) {
        // SAFETY: the descriptor is the Inotify's own and is closed once.
        unsafe {
            libc::close(self.0);
        }
    }
}

/// What the start check reads, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reading {
    /// What the agent runs use together: on Linux with the slice its memory.stat `anon`; None where Gizai can't tell.
    pub used: Option<u64>,
    /// What the computer has free (`available`).
    pub available: Option<u64>,
}

/// A Gizai's memory limit for its agent runs, and the slice that enforces it (Linux). Nothing is enforced until `enable`,
/// which Gizai's start does: a test's Gizai keeps its processes outside systemd unless the test enables it.
#[derive(Default)]
pub struct RunsMemory {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// The slice's name and the limit in bytes, from `enable` and `set_limit`.
    wanted: Option<(String, u64)>,
    /// What setting the slice up gave, once tried: the slice, or why there is none.
    made: Option<Result<Slice, String>>,
    /// Tests: readings that stand in for the real ones.
    fake: Option<Reading>,
}

impl RunsMemory {
    /// From now on, the processes `slice` hands out start in the slice `name`, with a limit of `limit` bytes. The slice is
    /// set up at the first `slice` call.
    pub fn enable(&self, name: &str, limit: u64) {
        let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        s.wanted = Some((name.to_string(), limit));
        s.made = None;
    }

    /// The slice to start agent processes in, set up at the first call after `enable` (Linux: systemctl and a first
    /// scope, well under a second). None when it isn't enabled, or can't be made: then they start as before, and Gizai
    /// logged why, once.
    pub fn slice(&self) -> Option<Slice> {
        let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if s.made.is_none()
            && let Some((name, limit)) = s.wanted.clone()
        {
            let made = Slice::set_up(&name, limit);
            match &made {
                Ok(slice) => eprintln!("gizai: agent runs start in {} ({}), with a memory limit of {:.1} GB together",
                                       slice.name(), slice.dir().display(), limit as f64 / GB as f64),
                Err(why) => eprintln!("gizai: agent runs start without a memory limit: {why}. Gizai only waits for free memory \
                                       before it starts a run (Settings → Runs → Memory for agent runs)."),
            }
            s.made = Some(made);
        }
        s.made.as_ref()?.as_ref().ok().cloned()
    }

    /// Whether the limit is enforced now: the slice is set up.
    pub fn enforced(&self) -> bool {
        matches!(self.inner.lock().unwrap_or_else(|e| e.into_inner()).made, Some(Ok(_)))
    }

    /// The limit in bytes, once enabled.
    pub fn limit(&self) -> Option<u64> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).wanted.as_ref().map(|(_, l)| *l)
    }

    /// A new limit of `limit` bytes. A slice that is set up gets it at once, MemoryMax and MemoryHigh together.
    pub fn set_limit(&self, limit: u64) -> Result<(), String> {
        let mut s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some((_, l)) = s.wanted.as_mut() else { return Ok(()) };
        *l = limit;
        match &s.made {
            Some(Ok(slice)) => slice.set_limit(limit),
            _ => Ok(()),
        }
    }

    /// The start check's readings now: what the runs in the slice use (when it is set up) and what the computer has free.
    pub fn reading(&self) -> Reading {
        let slice = {
            let s = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(f) = s.fake {
                return f;
            }
            s.made.as_ref().and_then(|m| m.as_ref().ok()).cloned()
        };
        Reading { used: slice.and_then(|s| s.used()), available: available() }
    }

    /// Tests: readings that stand in for the real ones; None goes back to the real ones.
    #[doc(hidden)]
    pub fn fake_reading(&self, reading: Option<Reading>) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).fake = reading;
    }
}
