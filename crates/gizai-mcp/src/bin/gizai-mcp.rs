//! gizai-mcp: the stdio MCP server Claude Code starts for a chat turn. It only connects stdio to the running
//! Gizai: dials the user-only socket in $GIZAI_SOCKET, sends the turn's token ($GIZAI_TOKEN) as its first
//! line, then copies bytes both ways. Gizai answers the MCP requests. It exits when Gizai closes the
//! connection: normally after Claude Code closed stdin, otherwise (a refused token, Gizai quitting) with 1.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn fail(msg: &str) -> ExitCode {
    eprintln!("gizai-mcp: {msg}");
    ExitCode::from(1)
}

fn main() -> ExitCode {
    let (Some(socket), Some(token)) = (std::env::var_os("GIZAI_SOCKET"), std::env::var("GIZAI_TOKEN").ok()) else {
        return fail("GIZAI_SOCKET and GIZAI_TOKEN must be set (Gizai sets them for each chat turn)");
    };
    let mut stream = match UnixStream::connect(&socket) {
        Ok(s) => s,
        Err(e) => return fail(&format!("can't reach Gizai at {} ({e}); is Gizai running?", std::path::Path::new(&socket).display())),
    };
    if let Err(e) = stream.write_all(gizai_mcp::hello_line(&token).as_bytes()) {
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
    let last = String::from_utf8_lossy(&tail).lines().last().unwrap_or_default().to_string();
    let reason = serde_json::from_str::<serde_json::Value>(&last).ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string));
    fail(&match reason {
        Some(r) => format!("Gizai closed the connection: {r}"),
        None => "Gizai closed the connection".to_string(),
    })
}
