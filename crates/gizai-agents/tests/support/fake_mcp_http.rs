//! A fake MCP server over HTTP for List tools tests, on 127.0.0.1 with std only: Streamable HTTP (POST /mcp) or the
//! older HTTP+SSE (GET /sse, then POST /messages). It keeps every request it got, so a test can check the headers
//! that arrived. One request per connection (it answers with Connection: close).
#![allow(dead_code)]
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Streamable HTTP: the hello as JSON with an Mcp-Session-Id, tools/list as an event stream.
    Ok,
    /// Every POST gets 500 with the values of its X-Api-Key and Authorization headers in the body.
    EchoError,
    /// Every POST gets 401 with a Bearer challenge.
    NeedsSignIn,
    /// The older HTTP+SSE transport.
    Sse,
}

/// A request the server got.
#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The JSON-RPC method of a POST ("" for anything else).
    pub fn rpc(&self) -> String {
        serde_json::from_str::<Value>(&self.body).ok().and_then(|v| v.get("method").and_then(Value::as_str).map(str::to_string)).unwrap_or_default()
    }
}

pub struct FakeHttp {
    pub base: String,
    pub mode: Mode,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl FakeHttp {
    /// The address to give Gizai: /mcp, or /sse for the older transport.
    pub fn url(&self) -> String {
        format!("{}{}", self.base, if self.mode == Mode::Sse { "/sse" } else { "/mcp" })
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

/// The tools it lists: one that only reads (readOnlyHint only), one without hints.
pub fn tools() -> Value {
    json!([
        {"name": "search", "description": "Searches the docs.", "annotations": {"readOnlyHint": true},
         "inputSchema": {"type": "object", "properties": {"q": {"type": "string", "description": "What to find"}}, "required": ["q"]}},
        {"name": "publish", "description": "Publishes a page.", "inputSchema": {"type": "object", "properties": {"page": {"type": "string"}}}},
    ])
}

fn rpc_answer(req: &Value) -> Option<Value> {
    let id = req.get("id").filter(|v| !v.is_null())?.clone();
    let method = req.get("method").and_then(Value::as_str).unwrap_or_default();
    Some(match method {
        "initialize" => json!({"jsonrpc": "2.0", "id": id, "result": {
            "protocolVersion": "2025-06-18", "capabilities": {"tools": {}}, "serverInfo": {"name": "fake-http", "version": "3.1.4"}}}),
        "tools/list" => json!({"jsonrpc": "2.0", "id": id, "result": {"tools": tools()}}),
        _ => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}}),
    })
}

pub fn start(mode: Mode) -> FakeHttp {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let stream_tx: Arc<Mutex<Option<mpsc::Sender<String>>>> = Arc::new(Mutex::new(None));
    let (seen2, base2) = (seen.clone(), base.clone());
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(conn) = conn else { continue };
            let (seen, stream_tx, base) = (seen2.clone(), stream_tx.clone(), base2.clone());
            thread::spawn(move || handle(conn, mode, &seen, &stream_tx, &base));
        }
    });
    FakeHttp { base, mode, seen }
}

fn read_request(conn: &TcpStream) -> Option<Seen> {
    let mut r = BufReader::new(conn.try_clone().ok()?);
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut headers = vec![];
    loop {
        let mut h = String::new();
        if r.read_line(&mut h).ok()? == 0 {
            break;
        }
        let h = h.trim_end_matches(['\r', '\n']);
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let get = |n: &str| headers.iter().find(|(k, _): &&(String, String)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v.clone());
    let mut body = Vec::new();
    if let Some(len) = get("content-length").and_then(|v| v.parse::<usize>().ok()) {
        body.resize(len, 0);
        r.read_exact(&mut body).ok()?;
    } else if get("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        loop {
            let mut size = String::new();
            r.read_line(&mut size).ok()?;
            let n = usize::from_str_radix(size.trim(), 16).ok()?;
            let mut chunk = vec![0; n + 2];
            r.read_exact(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
    }
    Some(Seen { method, path, headers, body: String::from_utf8_lossy(&body).into_owned() })
}

fn respond(conn: &mut TcpStream, status: &str, headers: &[(&str, &str)], body: &str) {
    let mut out = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    let _ = conn.write_all(out.as_bytes());
    let _ = conn.flush();
}

fn chunk(conn: &mut TcpStream, data: &str) -> std::io::Result<()> {
    conn.write_all(format!("{:x}\r\n{data}\r\n", data.len()).as_bytes())?;
    conn.flush()
}

fn handle(mut conn: TcpStream, mode: Mode, seen: &Mutex<Vec<Seen>>, stream_tx: &Mutex<Option<mpsc::Sender<String>>>, base: &str) {
    let Some(req) = read_request(&conn) else { return };
    seen.lock().unwrap().push(req.clone());
    let msg: Value = serde_json::from_str(&req.body).unwrap_or(Value::Null);
    match mode {
        Mode::EchoError => {
            let body = format!("Invalid API key: {} (authorization: {})", req.header("x-api-key").unwrap_or("none"),
                               req.header("authorization").unwrap_or("none"));
            respond(&mut conn, "500 Internal Server Error", &[("Content-Type", "text/plain")], &body);
        }
        Mode::NeedsSignIn => {
            let challenge = format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\"");
            respond(&mut conn, "401 Unauthorized", &[("WWW-Authenticate", &challenge)], "");
        }
        Mode::Ok => {
            if req.method == "DELETE" {
                return respond(&mut conn, "200 OK", &[], "");
            }
            let Some(answer) = rpc_answer(&msg) else { return respond(&mut conn, "202 Accepted", &[], "") };
            if req.rpc() == "tools/list" {
                // As an event stream: a log notification first, then the answer.
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
                let note = json!({"jsonrpc": "2.0", "method": "notifications/message", "params": {"level": "info", "data": "listing"}});
                let _ = conn.write_all(head.as_bytes());
                let _ = chunk(&mut conn, &format!("event: message\ndata: {note}\n\n"));
                let _ = chunk(&mut conn, &format!("event: message\ndata: {answer}\n\n"));
                let _ = chunk(&mut conn, "");
            } else {
                respond(&mut conn, "200 OK", &[("Content-Type", "application/json"), ("Mcp-Session-Id", "sess-42")], &answer.to_string());
            }
        }
        Mode::Sse => {
            if req.method == "GET" {
                let (tx, rx) = mpsc::channel::<String>();
                *stream_tx.lock().unwrap() = Some(tx);
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nTransfer-Encoding: chunked\r\n\r\n";
                if conn.write_all(head.as_bytes()).is_err() || chunk(&mut conn, "event: endpoint\ndata: /messages?session=7\n\n").is_err() {
                    return;
                }
                while let Ok(m) = rx.recv_timeout(Duration::from_secs(30)) {
                    if chunk(&mut conn, &format!("event: message\ndata: {m}\n\n")).is_err() {
                        return;
                    }
                }
                return;
            }
            if let Some(answer) = rpc_answer(&msg)
                && let Some(tx) = stream_tx.lock().unwrap().as_ref()
            {
                let _ = tx.send(answer.to_string());
            }
            respond(&mut conn, "202 Accepted", &[], "");
        }
    }
}
