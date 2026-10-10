//! gizai-mcp: the stdio MCP server Claude Code starts for a chat turn. It only connects stdio to the running
//! Gizai: dials the user-only socket in $GIZAI_SOCKET (a named pipe on Windows), sends the turn's token
//! ($GIZAI_TOKEN) as its first line, then copies bytes both ways. Gizai answers the MCP requests. It exits when
//! Gizai closes the connection: normally after Claude Code closed stdin, otherwise (a refused token, Gizai
//! quitting) with 1.
//!
//! On Windows a named pipe can't be half-closed, so the shim can't tell Gizai it is done writing: when Claude Code
//! closes stdin, the shim stops writing and passes on what Gizai still answers until Gizai closes the pipe or stays
//! quiet for half a second, then exits with 0 (Claude Code is no longer waiting for answers by then). The pipe is
//! read and written with tokio's overlapped I/O: blocking reads and writes on one pipe handle from two threads wait
//! for each other. And the token goes only to a pipe served by the shim's own user, in case another program took the
//! name before Gizai started.
use std::ffi::OsStr;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::process::ExitCode;
#[cfg(unix)]
use std::sync::Arc;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};

fn fail(msg: &str) -> ExitCode {
    eprintln!("gizai-mcp: {msg}");
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let (Some(socket), Some(token)) = (std::env::var_os("GIZAI_SOCKET"), std::env::var("GIZAI_TOKEN").ok()) else {
        return fail("GIZAI_SOCKET and GIZAI_TOKEN must be set (Gizai sets them for each chat turn)");
    };
    run(&socket, &token)
}

#[cfg(unix)]
fn run(socket: &OsStr, token: &str) -> ExitCode {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e) => return fail(&format!("can't reach Gizai at {} ({e}); is Gizai running?", std::path::Path::new(socket).display())),
    };
    if let Err(e) = stream.write_all(gizai_mcp::hello_line(token).as_bytes()) {
        return fail(&format!("can't talk to Gizai: {e}"));
    }
    let mut to_gizai = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => return fail(&format!("socket: {e}")),
    };

    // Stdin → socket; at end of input, tell Gizai we are done writing (it then answers what's left and closes).
    let stdin_done = Arc::new(AtomicBool::new(false));
    {
        let done = stdin_done.clone();
        std::thread::spawn(move || {
            let mut input = std::io::stdin().lock();
            let mut buf = [0u8; 1 << 14];
            loop {
                match input.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if to_gizai.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
            let _ = to_gizai.shutdown(std::net::Shutdown::Write);
        });
    }

    // Socket → stdout, flushed per read so every answer reaches Claude Code at once. Keep the last line, to
    // repeat Gizai's reason when it closes the connection on us.
    let mut out = std::io::stdout().lock();
    let mut buf = [0u8; 1 << 14];
    let mut tail: Vec<u8> = Vec::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > 4096 {
                    tail.drain(..tail.len() - 4096);
                }
                if out.write_all(&buf[..n]).and_then(|_| out.flush()).is_err() {
                    break;
                }
            }
        }
    }
    // Claude Code may close stdin a moment after Gizai answered its last request: give that a short grace.
    for _ in 0..15 {
        if stdin_done.load(Ordering::SeqCst) {
            return ExitCode::SUCCESS;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    closed(&tail)
}

/// Windows: the same over Gizai's named pipe (see the top of this file).
#[cfg(windows)]
fn run(socket: &OsStr, token: &str) -> ExitCode {
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt.block_on(over_pipe(socket, token)),
        Err(e) => fail(&format!("can't start: {e}")),
    }
}

/// Windows: how long Gizai may stay quiet after Claude Code closed stdin before the shim ends.
#[cfg(windows)]
const QUIET: std::time::Duration = std::time::Duration::from_millis(500);

#[cfg(windows)]
async fn over_pipe(socket: &OsStr, token: &str) -> ExitCode {
    use std::os::windows::io::AsRawHandle;
    use std::time::{Duration, Instant};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::ClientOptions;
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;

    // Every instance busy (Gizai is just letting another connection in): try again for a few seconds.
    let deadline = Instant::now() + Duration::from_secs(5);
    let pipe = loop {
        match ClientOptions::new().open(socket) {
            Ok(pipe) => break pipe,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(e) => return fail(&format!("can't reach Gizai at {} ({e}); is Gizai running?", std::path::Path::new(socket).display())),
        }
    };
    // Another program could have made a pipe by this name before Gizai started: the token goes only to our own user.
    // SAFETY: the handle is open while `pipe` lives.
    let server = unsafe { gizai_mcp::pipe::server_user(pipe.as_raw_handle()) };
    let ours = match (server, gizai_mcp::pipe::User::current()) {
        (Ok(server), Ok(me)) => server == me,
        _ => false,
    };
    if !ours {
        return fail(&format!("{} is not served by your Gizai (another user's program holds it)", std::path::Path::new(socket).display()));
    }
    let (mut from_gizai, mut to_gizai) = tokio::io::split(pipe);
    if let Err(e) = to_gizai.write_all(gizai_mcp::hello_line(token).as_bytes()).await {
        return fail(&format!("can't talk to Gizai: {e}"));
    }

    // Stdin → pipe. Stdin is read on a thread of its own (that read can't be cancelled; the thread ends with the shim)
    // and written to the pipe by a task, so a slow write never holds up Gizai's answers. The task ends with true when
    // Claude Code closed stdin, false when Gizai stopped taking input.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        let mut buf = [0u8; 1 << 14];
        loop {
            match input.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let mut writer = tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            if to_gizai.write_all(&chunk).await.is_err() {
                return false;
            }
        }
        true
    });

    // Pipe → stdout, flushed per read, keeping the last line for Gizai's reason, as on Unix.
    let mut out = std::io::stdout().lock();
    let mut buf = vec![0u8; 1 << 14];
    let mut tail: Vec<u8> = Vec::new();
    let mut writing = true;
    let mut stdin_closed = false;
    loop {
        let read = if stdin_closed {
            match tokio::time::timeout(QUIET, from_gizai.read(&mut buf)).await {
                Ok(read) => read,
                // Claude Code is done and Gizai has nothing more to say.
                Err(_) => return ExitCode::SUCCESS,
            }
        } else if writing {
            tokio::select! {
                read = from_gizai.read(&mut buf) => read,
                ended = &mut writer => {
                    writing = false;
                    stdin_closed = ended.unwrap_or(false);
                    continue;
                }
            }
        } else {
            from_gizai.read(&mut buf).await
        };
        match read {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tail.extend_from_slice(&buf[..n]);
                if tail.len() > 4096 {
                    tail.drain(..tail.len() - 4096);
                }
                if out.write_all(&buf[..n]).and_then(|_| out.flush()).is_err() {
                    break;
                }
            }
        }
    }
    // Claude Code may close stdin a moment after Gizai answered its last request: give that a short grace.
    if stdin_closed || (writing && matches!(tokio::time::timeout(Duration::from_millis(300), &mut writer).await, Ok(Ok(true)))) {
        return ExitCode::SUCCESS;
    }
    closed(&tail)
}

/// Gizai closed the connection while Claude Code still had stdin open: exit with 1, repeating Gizai's reason when its
/// last line gave one.
fn closed(tail: &[u8]) -> ExitCode {
    let last = String::from_utf8_lossy(tail).lines().last().unwrap_or_default().to_string();
    let reason = serde_json::from_str::<serde_json::Value>(&last).ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
    fail(&match reason {
        Some(r) => format!("Gizai closed the connection: {r}"),
        None => "Gizai closed the connection".to_string(),
    })
}
