// The MCP socket: a turn's token opens Gizai's tools; anything else is turned away.
use std::path::Path;

use gizai_core::model::AgentInput;
use gizai_core::{team, tokens};
use gizai_lib::{AppState, mcp, tools};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

fn lead(st: &AppState) -> String {
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap()
}

async fn session(path: &Path, hello: &str, requests: &[Value]) -> Vec<Value> {
    let stream = UnixStream::connect(path).await.unwrap();
    let (r, mut w) = stream.into_split();
    w.write_all(hello.as_bytes()).await.unwrap();
    for q in requests {
        w.write_all(format!("{q}\n").as_bytes()).await.unwrap();
    }
    w.shutdown().await.unwrap();
    let mut lines = BufReader::new(r).lines();
    let mut out = vec![];
    while let Ok(Ok(Some(l))) = tokio::time::timeout(std::time::Duration::from_secs(5), lines.next_line()).await {
        out.push(serde_json::from_str(&l).unwrap());
    }
    out
}

#[test]
fn socket_path_depends_on_the_data_dir() {
    let run = Path::new("/run/user/1000");
    let a = mcp::socket_path_in(Some(run), Path::new("/home/x/.local/share/gizai"));
    let b = mcp::socket_path_in(Some(run), Path::new("/home/x/Herd/gizai/.devdata/uitest"));
    assert_ne!(a, b);
    assert!(a.starts_with("/run/user/1000/gizai") && b.starts_with("/run/user/1000/gizai"), "{a:?} {b:?}");
    assert_eq!(a, mcp::socket_path_in(Some(run), Path::new("/home/x/.local/share/gizai")));
    assert!(a.to_string_lossy().len() < 100);
    assert_eq!(mcp::socket_path_in(None, Path::new("/d")), Path::new("/d/mcp.sock"));
}

#[tokio::test]
async fn a_valid_token_gets_tools() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let lead = lead(&st);
    let _server = mcp::start(&st).unwrap();
    let tok = tokens::mint(&st.db, &lead, json!({"chat": "T"}), 60_000).unwrap();
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line(&tok), &[
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "get_overview", "arguments": {}}}),
    ]).await;
    assert_eq!(out.len(), 3, "{out:?}");
    assert_eq!(out[0]["result"]["serverInfo"]["name"], "gizai");
    assert_eq!(out[1]["result"]["tools"].as_array().unwrap().len(), tools::catalog().len());
    assert_eq!(out[2]["result"]["isError"], false, "{}", out[2]);
    let overview: Value = serde_json::from_str(out[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(overview["agents"][0]["name"], "Team Lead");
}

#[tokio::test]
async fn a_bad_or_revoked_token_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let lead = lead(&st);
    let _server = mcp::start(&st).unwrap();
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line("nope"), &[ping.clone()]).await;
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0]["error"].as_str().unwrap().contains("token"));
    let tok = tokens::mint(&st.db, &lead, json!({}), 60_000).unwrap();
    tokens::revoke(&st.db, &tok).unwrap();
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line(&tok), &[ping.clone()]).await;
    assert!(out[0]["error"].is_string(), "{out:?}");
    let out = session(&st.mcp_socket, "{\"hello\": true}\n", &[ping]).await;
    assert!(out[0]["error"].is_string(), "{out:?}");
}

#[tokio::test]
async fn a_stale_socket_file_is_replaced_and_kept_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let lead = lead(&st);
    drop(std::os::unix::net::UnixListener::bind(&st.mcp_socket).unwrap()); // leaves the file behind
    assert!(st.mcp_socket.exists());
    let _server = mcp::start(&st).unwrap();
    let mode = std::fs::metadata(&st.mcp_socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let tok = tokens::mint(&st.db, &lead, json!({}), 60_000).unwrap();
    let out = session(&st.mcp_socket, &gizai_mcp::hello_line(&tok), &[json!({"jsonrpc": "2.0", "id": 9, "method": "ping"})]).await;
    assert_eq!(out[0]["id"], 9);
}

#[tokio::test]
async fn a_regular_file_in_the_way_is_not_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    std::fs::write(&st.mcp_socket, "not a socket").unwrap();
    assert!(mcp::start(&st).is_err());
    assert_eq!(std::fs::read_to_string(&st.mcp_socket).unwrap(), "not a socket");
}

#[test]
fn a_long_data_dir_still_gets_a_usable_socket_path() {
    let long = Path::new("/tmp").join("x".repeat(120));
    let p = mcp::socket_path_in(None, &long);
    assert!(p.to_string_lossy().len() < 100, "{p:?}");
    assert_ne!(p, mcp::socket_path_in(None, &Path::new("/tmp").join("y".repeat(120))), "still one per data dir");
    let deep = Path::new("/run/user/1000").join("z".repeat(120));
    assert!(mcp::socket_path_in(Some(&deep), Path::new("/d")).to_string_lossy().len() < 100);
}

#[tokio::test]
async fn start_binds_a_shortened_path_for_a_long_data_dir() {
    let dir = tempfile::tempdir().unwrap();
    let st0 = gizai_lib::test_state(dir.path());
    let mut st = st0.clone();
    st.mcp_socket = mcp::socket_path_in(None, &dir.path().join("w".repeat(110)));
    let _server = mcp::start(&st).unwrap();
    assert!(UnixStream::connect(&st.mcp_socket).await.is_ok());
    mcp::remove_socket(&st);
    let _ = std::fs::remove_dir(st.mcp_socket.parent().unwrap()); // only when empty
}
