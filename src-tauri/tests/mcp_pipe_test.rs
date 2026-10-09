//! GA-51, Windows: the Team Lead's chat reaches Gizai's tools through a named pipe (mcp_socket_test.rs is the same for
//! the Unix socket on Linux and macOS): one per user and data folder, a turn's token opens the tools, anything else is
//! turned away, a second Gizai can't take the name, and the gizai-mcp shim carries a turn over it.
#![cfg(windows)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use gizai_core::model::AgentInput;
use gizai_core::{team, tokens};
use gizai_lib::{AppState, mcp, tools};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;

fn lead(st: &AppState) -> String {
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap()
}

/// Sends the hello line and `requests` over the pipe and reads `expect` answers (a pipe can't be half-closed, so the
/// client can't say it is done writing).
async fn session(pipe: &Path, hello: &str, requests: &[Value], expect: usize) -> Vec<Value> {
    let client = ClientOptions::new().open(pipe).unwrap();
    let (r, mut w) = tokio::io::split(client);
    w.write_all(hello.as_bytes()).await.unwrap();
    for q in requests {
        w.write_all(format!("{q}\n").as_bytes()).await.unwrap();
    }
    w.flush().await.unwrap();
    let mut lines = BufReader::new(r).lines();
    let mut out = vec![];
    while out.len() < expect {
        match tokio::time::timeout(Duration::from_secs(10), lines.next_line()).await {
            Ok(Ok(Some(l))) => out.push(serde_json::from_str(&l).unwrap()),
            _ => break,
        }
    }
    out
}

fn initialize_and_overview() -> Vec<Value> {
    vec![
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "get_overview", "arguments": {}}}),
    ]
}

fn assert_tools_answered(out: &[Value]) {
    assert_eq!(out.len(), 3, "{out:?}");
    assert_eq!(out[0]["result"]["serverInfo"]["name"], "gizai");
    assert_eq!(out[1]["result"]["tools"].as_array().unwrap().len(), tools::catalog().len());
    assert_eq!(out[2]["result"]["isError"], false, "{}", out[2]);
    let overview: Value = serde_json::from_str(out[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(overview["agents"][0]["name"], "Team Lead");
}

#[test]
fn the_pipe_is_named_per_user_and_data_folder() {
    let a = mcp::socket_path(Path::new(r"C:\Users\x\AppData\Roaming\Gizai"));
    let b = mcp::socket_path(Path::new(r"C:\Users\x\gizai\.devdata\uitest"));
    assert_ne!(a, b);
    assert_eq!(a, mcp::socket_path(Path::new(r"C:\Users\x\AppData\Roaming\Gizai")));
    for p in [&a, &b] {
        assert!(p.to_string_lossy().starts_with(r"\\.\pipe\gizai-"), "{p:?}");
    }
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    assert_eq!(st.mcp_socket, mcp::socket_path(&st.data_dir), "a test's pipe is its own data folder's");
}

#[tokio::test]
async fn a_valid_token_gets_tools_over_the_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let lead = lead(&st);
    let _server = mcp::start(&st).unwrap();
    let tok = tokens::mint(&st.db, &lead, json!({"chat": "T"}), 60_000).unwrap();
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line(&tok), &initialize_and_overview(), 3).await;
    assert_tools_answered(&out);
}

#[tokio::test]
async fn a_bad_or_revoked_token_is_refused_over_the_pipe() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let lead = lead(&st);
    let _server = mcp::start(&st).unwrap();
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line("nope"), &[ping.clone()], 2).await;
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0]["error"].as_str().unwrap().contains("token"), "{out:?}");
    let tok = tokens::mint(&st.db, &lead, json!({}), 60_000).unwrap();
    tokens::revoke(&st.db, &tok).unwrap();
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line(&tok), &[ping], 2).await;
    assert!(out[0]["error"].is_string(), "{out:?}");
}

#[tokio::test]
async fn a_second_server_cant_take_a_pipe_name_that_is_in_use() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let _server = mcp::start(&st).unwrap();
    let e = mcp::start(&st).unwrap_err();
    assert!(e.to_string().contains("taken by another program"), "{e}");
}

/// The gizai-mcp shim next to the test binaries (`cargo test --workspace` builds it; CI builds it first).
fn shim() -> PathBuf {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), r"\..\target\debug\gizai-mcp.exe"));
    assert!(path.is_file(), "build gizai-mcp first: {}", path.display());
    path
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shim_carries_a_chat_turn_over_the_pipe_as_claude_code_starts_it() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let lead = lead(&st);
    let _server = mcp::start(&st).unwrap();
    let tok = tokens::mint(&st.db, &lead, json!({"chat": "T"}), 60_000).unwrap();
    let pipe = st.mcp_socket.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new(shim()).env("GIZAI_SOCKET", &pipe).env("GIZAI_TOKEN", &tok)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        for q in initialize_and_overview() {
            writeln!(stdin, "{q}").unwrap();
        }
        // Claude Code closes stdin when the turn is over; the shim passes on what Gizai still answers, then exits
        std::thread::sleep(Duration::from_secs(2));
        drop(stdin);
        child.wait_with_output().unwrap()
    }).await.unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}\n{}", String::from_utf8_lossy(&out.stderr));
    let answers: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_tools_answered(&answers);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_shim_with_a_wrong_token_gets_no_tools_and_exits_with_1() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let _server = mcp::start(&st).unwrap();
    let pipe = st.mcp_socket.clone();
    let out = tokio::task::spawn_blocking(move || {
        let mut child = std::process::Command::new(shim()).env("GIZAI_SOCKET", &pipe).env("GIZAI_TOKEN", "nope")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let _ = writeln!(stdin, "{}", json!({"jsonrpc": "2.0", "id": 1, "method": "ping"}));
        std::thread::sleep(Duration::from_secs(1));
        drop(stdin);
        child.wait_with_output().unwrap()
    }).await.unwrap();
    assert!(!out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("\"result\""), "{}", String::from_utf8_lossy(&out.stdout));
}
