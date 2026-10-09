//! Where Gizai finds the coding CLIs when it starts from the Dock or Finder on macOS: such an app gets a short PATH
//! (/usr/bin:/bin:/usr/sbin:/sbin), not the one your shell sets up. See docs/PLATFORMS.md.
use std::ffi::OsString;
use std::path::PathBuf;

/// macOS: adds the folders your login shell puts on PATH to Gizai's own PATH, once, before anything else starts.
/// Elsewhere it does nothing (Linux adds them per command in `runs::command_path`; Windows apps get the full PATH).
pub fn adopt_login_path() {
    #[cfg(target_os = "macos")]
    {
        if let Some(login) = login_path() {
            let path = with_login_path(std::env::var_os("PATH").unwrap_or_default(), &login);
            // SAFETY: main calls this first thing, before Tauri, Tokio or any other thread of Gizai's starts, and the
            // thread that read the login shell has ended, so nothing reads the environment while it changes.
            unsafe { std::env::set_var("PATH", path) };
        }
    }
}

/// Gizai's own PATH, followed by the folders of `login` it doesn't have yet.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn with_login_path(own: OsString, login: &str) -> OsString {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&own).collect();
    for d in std::env::split_paths(login) {
        if !d.as_os_str().is_empty() && !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    std::env::join_paths(dirs).unwrap_or(own)
}

/// The PATH your login shell sets up: $SHELL (else zsh) as an interactive login shell, in a session of its own so it
/// leaves alone the terminal Gizai may have been started from. None when it fails or prints no PATH, or when it takes
/// more than 5 seconds (it is ended then).
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn login_path() -> Option<String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Duration;

    // Marks around the PATH in the shell's output, so whatever its profile prints is left out.
    const START: &str = "__GIZAI_PATH_START__";
    const END: &str = "__GIZAI_PATH_END__";
    unsafe extern "C" {
        fn setsid() -> i32;
    }
    let shell = std::env::var("SHELL").ok().filter(|s| s.starts_with('/')).unwrap_or_else(|| "/bin/zsh".into());
    let script = format!("printf '%s%s%s' '{START}' \"$PATH\" '{END}'");
    let mut cmd = Command::new(&shell);
    cmd.args(["-l", "-i", "-c", &script]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and only takes the shell out of the terminal's session; it touches nothing of
    // Gizai's.
    unsafe {
        cmd.pre_exec(|| {
            setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn().ok()?;
    let mut out = child.stdout.take()?;
    // Read until the end mark, not until the pipe closes: something the profile starts in the background may keep it open.
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let (mut text, mut buf) = (Vec::new(), [0u8; 4096]);
        while let Ok(n) = out.read(&mut buf) {
            if n == 0 {
                break;
            }
            text.extend_from_slice(&buf[..n]);
            if text.windows(END.len()).any(|w| w == END.as_bytes()) {
                break;
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&text).into_owned());
    });
    let text = rx.recv_timeout(Duration::from_secs(5)).ok();
    // It has printed the PATH or taken too long: either way it is done.
    let _ = child.kill();
    let _ = child.wait();
    let text = text?;
    let _ = reader.join();
    let start = text.find(START)? + START.len();
    let len = text[start..].find(END)?;
    Some(text[start..start + len].to_string()).filter(|p| !p.trim().is_empty())
}
