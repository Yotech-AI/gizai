// Linux and macOS only: the shim's Unix socket. Windows has a named pipe instead.
#![cfg(unix)]
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};

const SHIM: &str = env!("CARGO_BIN_EXE_gizai-mcp");

#[test]
fn pipes_stdio_to_the_socket_after_saying_hello() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        // A broken shim must fail the test, not hang it.
        let t0 = std::time::Instant::now();
        let stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && t0.elapsed().as_secs() < 10 => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(e) => panic!("the shim never connected: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut hello = String::new();
        reader.read_line(&mut hello).unwrap();
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        let mut w = stream;
        w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n").unwrap();
        (hello, request)
    });
    let mut child = Command::new(SHIM)
        .env("GIZAI_SOCKET", &sock).env("GIZAI_TOKEN", "tok-123")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n").unwrap();
    let (hello, request) = server.join().unwrap();
    assert_eq!(gizai_mcp::parse_hello(&hello).as_deref(), Some("tok-123"));
    assert!(request.contains("\"ping\""));
    let mut out = String::new();
    child.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    assert_eq!(out, "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n");
    assert!(child.wait().unwrap().success());
}

#[test]
fn says_why_when_gizai_is_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let out = Command::new(SHIM)
        .env("GIZAI_SOCKET", dir.path().join("missing.sock")).env("GIZAI_TOKEN", "x")
        .stdin(Stdio::null()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("gizai-mcp:"), "{err}");
    assert!(err.contains("Gizai"), "{err}");
}

#[test]
fn needs_its_environment() {
    let out = Command::new(SHIM).env_remove("GIZAI_SOCKET").env_remove("GIZAI_TOKEN").stdin(Stdio::null()).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("GIZAI_SOCKET"));
}

#[test]
fn exits_when_gizai_closes_the_connection_even_with_stdin_open() {
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("t.sock");
    let listener = UnixListener::bind(&sock).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut w = stream.try_clone().unwrap();
        let mut hello = String::new();
        BufReader::new(stream).read_line(&mut hello).unwrap();
        w.write_all(b"{\"error\":\"this token is not valid\"}\n").unwrap();
        // dropping both halves closes the connection
    });
    let mut child = Command::new(SHIM)
        .env("GIZAI_SOCKET", &sock).env("GIZAI_TOKEN", "bad")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().unwrap();
    let _keep_stdin_open = child.stdin.take().unwrap();
    server.join().unwrap();
    let t0 = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() { break s; }
        if t0.elapsed().as_secs() > 5 { child.kill().unwrap(); panic!("the shim kept running after Gizai closed the connection"); }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(1));
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(err.contains("Gizai closed the connection"), "{err}");
}
