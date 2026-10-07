use gizai_mcp::{ToolDef, Tools, handle, hello_line, parse_hello, serve};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Fake;

impl Tools for Fake {
    fn list(&self) -> Vec<ToolDef> {
        vec![
            ToolDef { name: "echo".into(), description: "Returns its input.".into(), input_schema: json!({"type": "object"}), read_only: true },
            ToolDef { name: "boom".into(), description: "Always fails.".into(), input_schema: json!({"type": "object"}), read_only: false },
        ]
    }
    async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        match name {
            "echo" => Ok(args),
            "boom" => Err("it broke".into()),
            other => Err(format!("unknown tool {other}")),
        }
    }
}

fn req(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

async fn ask(msg: Value) -> Value {
    handle(&Fake, &msg).await.expect("a request gets an answer")
}

#[tokio::test]
async fn initialize_echoes_the_clients_protocol_version() {
    let r = ask(req(1, "initialize", json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "claude-code"}}))).await;
    assert_eq!(r["id"], 1);
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(r["result"]["serverInfo"]["name"], "gizai");
    assert!(r["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn initialize_without_a_version_gets_the_default() {
    let r = ask(req(1, "initialize", json!({}))).await;
    assert_eq!(r["result"]["protocolVersion"], gizai_mcp::DEFAULT_PROTOCOL);
}

#[tokio::test]
async fn tools_list_has_schemas_and_read_only_hints() {
    let r = ask(req(2, "tools/list", json!({}))).await;
    let tools = r["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "echo");
    assert_eq!(tools[0]["inputSchema"]["type"], "object");
    assert_eq!(tools[0]["annotations"]["readOnlyHint"], true);
    assert_eq!(tools[1]["annotations"]["readOnlyHint"], false);
}

#[tokio::test]
async fn tools_call_returns_text_content() {
    let r = ask(req(3, "tools/call", json!({"name": "echo", "arguments": {"a": 1, "b": "two"}}))).await;
    assert_eq!(r["result"]["isError"], false);
    assert_eq!(r["result"]["content"][0]["type"], "text");
    let back: Value = serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(back, json!({"a": 1, "b": "two"}));
}

#[tokio::test]
async fn a_call_without_arguments_gets_an_empty_object() {
    let r = ask(req(3, "tools/call", json!({"name": "echo"}))).await;
    assert_eq!(r["result"]["content"][0]["text"], "{}");
}

#[tokio::test]
async fn a_failing_tool_is_an_error_result_not_a_protocol_error() {
    let r = ask(req(4, "tools/call", json!({"name": "boom", "arguments": {}}))).await;
    assert!(r.get("error").is_none());
    assert_eq!(r["result"]["isError"], true);
    assert_eq!(r["result"]["content"][0]["text"], "it broke");
}

#[tokio::test]
async fn unknown_tool_is_an_error_result() {
    let r = ask(req(5, "tools/call", json!({"name": "nope", "arguments": {}}))).await;
    assert_eq!(r["result"]["isError"], true);
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("unknown tool"));
}

#[tokio::test]
async fn notifications_get_no_answer() {
    assert!(handle(&Fake, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await.is_none());
    assert!(handle(&Fake, &json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 3}})).await.is_none());
}

#[tokio::test]
async fn unknown_method_is_method_not_found() {
    let r = ask(req(6, "resources/list", json!({}))).await;
    assert_eq!(r["error"]["code"], -32601);
    assert_eq!(r["id"], 6);
}

#[tokio::test]
async fn ping_answers_empty() {
    let r = ask(req(7, "ping", json!({}))).await;
    assert_eq!(r["result"], json!({}));
}

#[tokio::test]
async fn string_ids_come_back_unchanged() {
    let r = ask(json!({"jsonrpc": "2.0", "id": "abc", "method": "ping"})).await;
    assert_eq!(r["id"], "abc");
}

#[tokio::test]
async fn serve_reads_lines_and_answers_each_request() {
    let (client, server) = tokio::io::duplex(1 << 16);
    let (sr, sw) = tokio::io::split(server);
    let task = tokio::spawn(async move { serve(BufReader::new(sr), sw, &Fake).await });
    let (cr, mut cw) = tokio::io::split(client);
    let mut lines = BufReader::new(cr).lines();
    let input = format!(
        "{}\n{}\n\n{}\nthis is not json\n",
        req(1, "ping", json!({})),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        req(2, "tools/call", json!({"name": "echo", "arguments": {"x": 1}})),
    );
    cw.write_all(input.as_bytes()).await.unwrap();
    let a: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let b: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    let c: Value = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(a["id"], 1);
    assert_eq!(b["id"], 2);
    assert_eq!(c["error"]["code"], -32700);
    assert!(c["id"].is_null());
    cw.shutdown().await.unwrap();
    drop(cw);
    task.await.unwrap().unwrap();
}

#[test]
fn hello_round_trips() {
    let line = hello_line("abc123");
    assert!(line.ends_with('\n'));
    assert_eq!(parse_hello(&line), Some("abc123".to_string()));
    assert_eq!(parse_hello("nope"), None);
    assert_eq!(parse_hello("{\"token\": 5}"), None);
}
