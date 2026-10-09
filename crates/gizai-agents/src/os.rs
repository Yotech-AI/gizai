//! What differs between Linux, macOS and Windows when Gizai starts a program: finding it, and starting it without a
//! console window (docs/PLATFORMS.md). On Linux and macOS these are the plain std calls.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    // npm's cmd-shim ends with: "%_prog%"  "%dp0%\node_modules\<package>\<script>.js" %*
    let start = text.rfind("\"%dp0%\\")? + "\"%dp0%\\".len();
    let rel = &text[start..start + text[start..].find('"')?];
    if !(rel.ends_with(".js") || rel.ends_with(".cjs") || rel.ends_with(".mjs")) {
        return None;
    }
    let script = dir.join(rel);
    if !script.is_file() {
        return None;
    }
    let local = dir.join("node.exe");
    let node = if local.is_file() { local } else { find_in("node", &std::env::var_os("PATH").unwrap_or_default())? };
    Some((node, script))
}

/// The bash that runs shell commands (a worktree's prepare commands): `bash` on PATH on Linux and macOS. On Windows Git
/// Bash, which Claude Code needs too: CLAUDE_CODE_GIT_BASH_PATH, else the one next to git on PATH, else Git for
/// Windows' usual places. Never `C:\Windows\System32\bash.exe`: that one starts WSL, which Gizai doesn't use.
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
        // <Git>\cmd\git.exe, <Git>\bin\git.exe or <Git>\mingw64\bin\git.exe: bash is <Git>\bin\bash.exe
        let from_git = find_in("git", path).and_then(|git| {
            git.ancestors().skip(2).take(2).map(|root| root.join("bin").join("bash.exe")).find(|b| b.is_file())
        });
        from_git.or_else(|| {
            ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"].iter()
                .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
                .flat_map(|base| [base.join("Git").join("bin").join("bash.exe"), base.join("Programs").join("Git").join("bin").join("bash.exe")])
                .find(|b| b.is_file())
        })
    }
}
