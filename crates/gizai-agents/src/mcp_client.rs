//! List tools: Gizai starts an MCP server (stdio) or calls it (Streamable HTTP, or the older HTTP+SSE), says hello
//! (`initialize`) and reads every page of `tools/list`. Blocking code: callers run it with `spawn_blocking`.
//! A server Gizai starts runs in a process group of its own, which is always ended afterwards (SIGTERM, SIGKILL after
//! 3 s; never pid 0 or 1; on Windows a Job Object, ended at once, see `os::Tree`), and no error shows the values of the
//! server's environment or headers.
use std::collections::{HashSet, VecDeque};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Serialize;
use serde_json::{Value, json};
use ureq::RequestExt;
use ureq::http::{HeaderName, HeaderValue, Method, Response, StatusCode, Uri};

use crate::cli::plain;
use crate::mcp_tools::PARAM_ORDER;
use crate::os::{self, End, Tree};
use crate::stream::cut;

/// The MCP version Gizai asks for; the server answers with the one it speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// What one listing reads at most: pages of `tools/list`, and tools in all.
const MAX_PAGES: u64 = 50;
const MAX_TOOLS: usize = 2000;
/// The longest message Gizai reads (one stdout line, one HTTP answer, one event).
const MAX_MESSAGE: u64 = 64 * 1024 * 1024;
/// How many lines of a server's own output an error shows.
const TAIL_LINES: usize = 20;
/// How long a server gets to end after SIGTERM before its group gets SIGKILL.
const END_GRACE: Duration = Duration::from_secs(3);
/// Redirects an HTTP server may send Gizai on, within its own site.
const MAX_REDIRECTS: usize = 5;

/// How Gizai reaches a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    /// A program Gizai starts, talking newline-delimited JSON-RPC on its stdin and stdout.
    #[default]
    Stdio,
    /// Streamable HTTP: every message is a POST to one address.
    Http,
    /// The older HTTP+SSE transport: one event stream (GET), and POSTs to the address it gives.
    Sse,
}

impl Transport {
    pub fn parse(s: &str) -> Option<Transport> {
        match s {
            "stdio" => Some(Transport::Stdio),
            "http" => Some(Transport::Http),
            "sse" => Some(Transport::Sse),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Transport::Stdio => "stdio",
            Transport::Http => "http",
            Transport::Sse => "sse",
        }
    }
}

/// A server as Gizai starts or calls it, with its secret values filled in. `Debug` shows only the names of its
/// environment and headers.
#[derive(Clone, Default)]
pub struct Target {
    pub transport: Transport,
    /// stdio: the program, its arguments, and the environment set on top of Gizai's own.
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// stdio: the folder it starts in (else Gizai's own).
    pub cwd: Option<PathBuf>,
    /// http and sse: where the server is, and the headers sent with every request (a signed-in server's OAuth access
    /// token comes in as an Authorization header).
    pub url: String,
    pub headers: Vec<(String, String)>,
}

impl std::fmt::Debug for Target {
    /// Like a derived one, but with only the names of the env and header values: those are often keys or tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = |kv: &[(String, String)]| kv.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        f.debug_struct("Target")
            .field("transport", &self.transport)
            .field("command", &self.command)
            .field("args", &self.args)
            .field("env", &names(&self.env))
            .field("cwd", &self.cwd)
            .field("url", &self.url)
            .field("headers", &names(&self.headers))
            .finish()
    }
}

impl Target {
    /// The values an error must never show: every environment and header value, and a header's token without its
    /// scheme too ("Bearer abc…" → also "abc…").
    pub fn secrets(&self) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for (_, v) in self.env.iter().chain(&self.headers) {
            out.push(v.clone());
            out.push(v.trim().to_string());
        }
        for (_, v) in &self.headers {
            if let Some((_, token)) = v.trim().split_once(' ') {
                out.push(token.trim().to_string());
            }
        }
        out.sort();
        out.dedup();
        out
    }
}

/// What a server said about itself, and its tools.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub server_name: String,
    pub server_version: String,
    /// The MCP version the server chose.
    pub protocol_version: String,
    /// The `tools/list` entries as the server sent them, from every page, each with the order of its parameters added
    /// (`mcp_tools::PARAM_ORDER`).
    pub tools: Vec<Value>,
}

/// Why a listing failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ListError {
    /// HTTP 401 (or 403 with WWW-Authenticate: Bearer): the raw WWW-Authenticate header value ("" when absent).
    #[error("The server asks you to sign in.")]
    NeedsSignIn { www_authenticate: String },
    /// Anything else, in plain words, with every secret value (env values, header values) replaced by "•••".
    #[error("{0}")]
    Failed(String),
}

fn failed(text: impl Into<String>) -> ListError {
    ListError::Failed(text.into())
}

/// `text` with every secret in it shown as "•••". Secrets shorter than 4 characters are left alone (they would hide
/// ordinary words); longer ones go first, so a secret that holds another is hidden whole.
pub fn redact(text: &str, secrets: &[&str]) -> String {
    let mut secrets: Vec<&str> = secrets.iter().copied().filter(|s| s.chars().count() >= 4).collect();
    secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut out = text.to_string();
    for s in secrets {
        if out.contains(s) {
            out = out.replace(s, "•••");
        }
    }
    out
}

/// Starts or calls the server, says hello and reads all its tools, in at most `timeout` (callers give 120 s: an
/// `npx -y` server downloads its package the first time). A server Gizai started is ended before this returns, which
/// may take up to 3 s more when it ignores SIGTERM. Blocking: call it from `spawn_blocking`.
pub fn list_tools(t: &Target, timeout: Duration) -> Result<Listing, ListError> {
    let limit = Limit::new(timeout);
    let res = match t.transport {
        Transport::Stdio => list_stdio(t, limit),
        Transport::Http => list_http(t, limit),
        Transport::Sse => list_sse(t, limit),
    };
    res.map_err(|e| match e {
        ListError::Failed(text) => {
            let secrets = t.secrets();
            ListError::Failed(redact(&text, &secrets.iter().map(String::as_str).collect::<Vec<_>>()))
        }
        other => other,
    })
}

/// A listing's deadline, and the time it was given (for the error).
#[derive(Debug, Clone, Copy)]
struct Limit {
    deadline: Instant,
    timeout: Duration,
}

impl Limit {
    fn new(timeout: Duration) -> Limit {
        let now = Instant::now();
        // A timeout too long to add (Duration::MAX) is as good as a year.
        let deadline = now.checked_add(timeout).unwrap_or_else(|| now + Duration::from_secs(365 * 86_400));
        Limit { deadline, timeout }
    }

    fn left(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    fn error(&self) -> ListError {
        failed(format!("The server didn't answer within {}.", secs(self.timeout)))
    }
}

/// "120 s", or "0.5 s" for a short one.
fn secs(d: Duration) -> String {
    if d.subsec_millis() == 0 || d.as_secs() >= 10 { format!("{} s", d.as_secs()) } else { format!("{:.1} s", d.as_secs_f64()) }
}

// ---- JSON-RPC, the same over every transport ----

/// A JSON-RPC error the server answered with.
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn failed(self) -> ListError {
        let m = self.message.trim();
        if m.is_empty() {
            failed(format!("The server answered with an error (code {}).", self.code))
        } else {
            failed(format!("The server answered with an error: {}", cut(m, 500)))
        }
    }
}

/// The answer to one request: its result, or the error the server gave.
type Answer = Result<Value, RpcError>;

fn request(id: u64, method: &str, params: Option<Value>) -> Value {
    let mut m = json!({"jsonrpc": "2.0", "id": id, "method": method});
    if let (Some(p), Some(obj)) = (params, m.as_object_mut()) {
        obj.insert("params".into(), p);
    }
    m
}

fn notification(method: &str) -> Value {
    json!({"jsonrpc": "2.0", "method": method})
}

/// What a message from the server means while Gizai waits for the answer to its request `id`.
enum Seen {
    Answer(Answer),
    /// A request from the server, with Gizai's reply to send back.
    Reply(Value),
    /// A notification, an answer to something else.
    Other,
}

fn seen(mut msg: Value, id: u64) -> Seen {
    let method = msg.get("method").and_then(Value::as_str).map(str::to_string);
    let msg_id = msg.get("id").cloned().unwrap_or(Value::Null);
    match method {
        Some(method) if !msg_id.is_null() => Seen::Reply(reply(&msg_id, &method)),
        Some(_) => Seen::Other,
        None if same_id(&msg_id, id) => Seen::Answer(match msg.get("error").filter(|e| !e.is_null()) {
            Some(e) => Err(RpcError { code: e.get("code").and_then(Value::as_i64).unwrap_or(0), message: text_at(e, "message") }),
            None => Ok(msg.get_mut("result").map(Value::take).unwrap_or(Value::Null)),
        }),
        None => Seen::Other,
    }
}

fn same_id(v: &Value, id: u64) -> bool {
    v.as_u64() == Some(id) || v.as_str().is_some_and(|s| s.trim() == id.to_string())
}

/// Gizai's reply to a request from the server: an empty result for `ping`; Gizai offers nothing else (no sampling, no
/// roots, no elicitation), so anything else is "method not found".
fn reply(id: &Value, method: &str) -> Value {
    if method == "ping" {
        json!({"jsonrpc": "2.0", "id": id, "result": {}})
    } else {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "Method not found"}})
    }
}

/// One message, or each message of a batch.
fn messages(v: Value) -> Vec<Value> {
    match v {
        Value::Array(list) => list,
        other => vec![other],
    }
}

/// A message, or a batch, from the server. serde_json's maps sort their keys, so each tool of a `tools/list` answer also
/// gets the order its parameters were written in, read from the text (`PARAM_ORDER`): the form lists them that way.
fn parse_json(text: &str) -> serde_json::Result<Value> {
    let mut v: Value = serde_json::from_str(text)?;
    let lists_tools = |m: &Value| m.pointer("/result/tools").is_some_and(Value::is_array);
    let any = match &v {
        Value::Array(batch) => batch.iter().any(lists_tools),
        one => lists_tools(one),
    };
    if any && let Ok(shape) = serde_json::from_str::<Shape>(text) {
        mark_order(&mut v, &shape);
    }
    Ok(v)
}

/// Gives each tool in `v` (a message or a batch) the order of its parameters in `shape`, the same text read in order.
fn mark_order(v: &mut Value, shape: &Shape) {
    if let (Value::Array(batch), Shape::Array(shapes)) = (&mut *v, shape) {
        batch.iter_mut().zip(shapes).for_each(|(m, s)| mark_order(m, s));
        return;
    }
    let Some(tools) = v.pointer_mut("/result/tools").and_then(Value::as_array_mut) else { return };
    let Some(Shape::Array(shapes)) = shape.get("result").and_then(|r| r.get("tools")) else { return };
    for (tool, s) in tools.iter_mut().zip(shapes) {
        if let (Some(tool), Some(Shape::Object(props))) = (tool.as_object_mut(), s.get("inputSchema").and_then(|i| i.get("properties"))) {
            tool.insert(PARAM_ORDER.into(), props.iter().map(|(k, _)| Value::String(k.clone())).collect());
        }
    }
}

/// A JSON value's shape, each object's keys in the order they were written.
enum Shape {
    Object(Vec<(String, Shape)>),
    Array(Vec<Shape>),
    Other,
}

impl Shape {
    /// An object's value at `key`: the last one, as serde_json keeps it.
    fn get(&self, key: &str) -> Option<&Shape> {
        match self {
            Shape::Object(entries) => entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for Shape {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Shape, D::Error> {
        struct Any;
        impl<'de> Visitor<'de> for Any {
            type Value = Shape;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("any JSON value")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut m: A) -> Result<Shape, A::Error> {
                let mut entries = vec![];
                while let Some(entry) = m.next_entry::<String, Shape>()? {
                    entries.push(entry);
                }
                Ok(Shape::Object(entries))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut s: A) -> Result<Shape, A::Error> {
                let mut items = vec![];
                while let Some(item) = s.next_element::<Shape>()? {
                    items.push(item);
                }
                Ok(Shape::Array(items))
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Shape, E> {
                Ok(Shape::Other)
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Shape, E> {
                Ok(Shape::Other)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Shape, E> {
                Ok(Shape::Other)
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Shape, E> {
                Ok(Shape::Other)
            }
            fn visit_str<E: de::Error>(self, _: &str) -> Result<Shape, E> {
                Ok(Shape::Other)
            }
            fn visit_unit<E: de::Error>(self) -> Result<Shape, E> {
                Ok(Shape::Other)
            }
        }
        d.deserialize_any(Any)
    }
}

fn text_at(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// One way of talking to a server.
trait Conn {
    /// Sends request `id` and waits for its answer.
    fn call(&mut self, id: u64, method: &str, params: Option<Value>) -> Result<Answer, ListError>;
    /// Sends a notification, which has no answer.
    fn notify(&mut self, method: &str) -> Result<(), ListError>;
    /// The protocol version the server chose (Streamable HTTP sends it along from then on).
    fn agreed(&mut self, _version: &str) {}
}

/// Hello, then every page of `tools/list`.
fn session(c: &mut impl Conn) -> Result<Listing, ListError> {
    let hello = json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": {"name": "Gizai", "version": env!("CARGO_PKG_VERSION")},
    });
    let init = c.call(1, "initialize", Some(hello))?.map_err(RpcError::failed)?;
    if !init.is_object() {
        return Err(failed("The server's answer to Gizai's hello isn't MCP."));
    }
    let protocol_version = text_at(&init, "protocolVersion");
    c.agreed(&protocol_version);
    c.notify("notifications/initialized")?;
    let has_tools = init.pointer("/capabilities/tools").is_some();

    let mut tools: Vec<Value> = vec![];
    let mut cursor: Option<String> = None;
    let mut cursors: HashSet<String> = HashSet::new();
    for page in 0..MAX_PAGES {
        let params = cursor.as_ref().map(|c| json!({"cursor": c}));
        let mut result = match c.call(2 + page, "tools/list", params)? {
            Ok(r) => r,
            // A server that offers no tools may not know the method at all.
            Err(e) if e.code == -32601 && !has_tools && page == 0 => break,
            Err(e) => return Err(e.failed()),
        };
        if let Some(Value::Array(list)) = result.get_mut("tools").map(Value::take) {
            tools.extend(list.into_iter().filter(Value::is_object));
        }
        if tools.len() >= MAX_TOOLS {
            tools.truncate(MAX_TOOLS);
            break;
        }
        match result.get("nextCursor").and_then(Value::as_str) {
            // A cursor seen before would go round in circles.
            Some(next) if !next.is_empty() && cursors.insert(next.to_string()) => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    let info = init.get("serverInfo").cloned().unwrap_or(Value::Null);
    Ok(Listing { server_name: text_at(&info, "name"), server_version: text_at(&info, "version"), protocol_version, tools })
}

// ---- stdio ----

fn list_stdio(t: &Target, limit: Limit) -> Result<Listing, ListError> {
    let mut server = Server::start(t, limit)?;
    session(&mut server)
    // Dropping `server` closes its stdin and ends its process group.
}

/// A server Gizai started, and the threads that write its stdin and read its stdout and stderr.
struct Server {
    child: Child,
    /// Its process group (its leader is the program Gizai started); on Windows its job.
    tree: Tree,
    /// Lines for its stdin; dropped to close it.
    input: Option<mpsc::Sender<Vec<u8>>>,
    /// Its JSON messages, or why its output can't be read; closed when its stdout ends.
    output: mpsc::Receiver<Result<Value, String>>,
    /// The last lines it printed: stderr, and stdout lines that aren't JSON.
    tail: Arc<Mutex<VecDeque<String>>>,
    /// Closed when its stderr has ended.
    stderr_done: mpsc::Receiver<()>,
    status: Option<ExitStatus>,
    ended: bool,
    limit: Limit,
}

impl Server {
    fn start(t: &Target, limit: Limit) -> Result<Server, ListError> {
        let command = t.command.trim();
        if command.is_empty() {
            return Err(failed("Give the command that starts the server."));
        }
        if let Some(dir) = &t.cwd
            && !dir.is_dir()
        {
            return Err(failed(format!("The folder {} isn't there.", dir.display())));
        }
        for (k, v) in &t.env {
            if k.is_empty() || k.contains(['=', '\0']) {
                return Err(failed(format!("\"{k}\" isn't a valid name for an environment variable.")));
            }
            if v.contains('\0') {
                return Err(failed(format!("The value of {k} isn't valid text.")));
            }
        }
        // os::command: `npx` is npx.cmd on Windows
        let mut cmd = os::command(command);
        cmd.args(&t.args)
            .envs(t.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &t.cwd {
            cmd.current_dir(dir);
        }
        let (mut child, tree) = os::spawn_tree(&mut cmd, false).map_err(|e| {
            failed(match e.kind() {
                io::ErrorKind::NotFound => format!("Couldn't start {command}: not found. Give the full path, or a program in your PATH."),
                io::ErrorKind::PermissionDenied => format!("Couldn't start {command}: it may not be run (permission denied)."),
                _ => format!("Couldn't start {command}: {e}."),
            })
        })?;
        let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>();
        let (out_tx, out_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        // From here on, dropping `server` ends the process group.
        let server = Server {
            child, tree, input: Some(in_tx), output: out_rx, tail: tail.clone(), stderr_done: done_rx, status: None, ended: false, limit,
        };
        let not_started = |e: io::Error| failed(format!("Couldn't start {command}: Gizai couldn't read its output ({e})."));
        if let Some(mut w) = stdin {
            thread::Builder::new().name("mcp-stdin".into()).spawn(move || {
                for line in in_rx {
                    if w.write_all(&line).and_then(|()| w.flush()).is_err() {
                        break;
                    }
                }
            }).map_err(not_started)?;
        }
        if let Some(r) = stdout {
            let tail = tail.clone();
            thread::Builder::new().name("mcp-stdout".into()).spawn(move || read_stdout(r, &out_tx, &tail)).map_err(not_started)?;
        }
        if let Some(r) = stderr {
            thread::Builder::new().name("mcp-stderr".into()).spawn(move || {
                read_stderr(r, &tail);
                drop(done_tx);
            }).map_err(not_started)?;
        }
        Ok(server)
    }

    fn send(&self, msg: &Value) {
        if let Some(input) = &self.input {
            let mut line = msg.to_string().into_bytes();
            line.push(b'\n');
            // A server that closed its stdin shows up as its output ending.
            let _ = input.send(line);
        }
    }

    fn tail(&self) -> Vec<String> {
        self.tail.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
    }

    fn timed_out(&self) -> ListError {
        match self.limit.error() {
            ListError::Failed(text) => failed(with_tail(&text, "Its last lines", &self.tail())),
            other => other,
        }
    }

    /// Its stdout ended before the answer came: its exit code and last lines.
    fn gone(&mut self) -> ListError {
        // A moment to exit by itself, so its own exit code shows.
        let until = Instant::now() + Duration::from_millis(500);
        while self.status.is_none() && Instant::now() < until {
            match self.child.try_wait() {
                Ok(Some(s)) => self.status = Some(s),
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(_) => break,
            }
        }
        self.end();
        // Its last stderr lines may still be on their way.
        let _ = self.stderr_done.recv_timeout(Duration::from_millis(500));
        let mut text = "The server ended before it answered".to_string();
        if let Some(code) = self.status.and_then(|s| s.code()).filter(|c| *c != 0) {
            text.push_str(&format!(" (exit code {code})"));
        }
        let tail = self.tail();
        if tail.is_empty() {
            text.push('.');
        } else if tail.len() == 1 {
            text.push_str(&format!(": {}", tail[0]));
        } else {
            text.push_str(&format!(":\n{}", tail.join("\n")));
        }
        failed(text)
    }

    /// Closes its stdin, sends SIGTERM to its process group, SIGKILL after 3 s when anything in it is still running,
    /// and reaps it. Runs once. On Windows its job ends at once (see `os::End`).
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        // The writer closes stdin once it has written what it had.
        self.input = None;
        if self.tree.alive() {
            self.tree.end(End::Terminate);
        }
        let start = Instant::now();
        loop {
            if self.status.is_none()
                && let Ok(Some(s)) = self.child.try_wait()
            {
                self.status = Some(s);
            }
            // The leader counts until it has been reaped (just above), its own children until they end.
            if !self.tree.alive() {
                break;
            }
            if start.elapsed() >= END_GRACE {
                self.tree.end(End::Kill);
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        if let Ok(s) = self.child.wait() {
            self.status.get_or_insert(s);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.end();
    }
}

impl Conn for Server {
    fn call(&mut self, id: u64, method: &str, params: Option<Value>) -> Result<Answer, ListError> {
        self.send(&request(id, method, params));
        loop {
            let left = self.limit.left();
            if left.is_zero() {
                return Err(self.timed_out());
            }
            match self.output.recv_timeout(left) {
                Ok(Ok(v)) => {
                    for msg in messages(v) {
                        match seen(msg, id) {
                            Seen::Answer(a) => return Ok(a),
                            Seen::Reply(r) => self.send(&r),
                            Seen::Other => {}
                        }
                    }
                }
                Ok(Err(e)) => return Err(failed(e)),
                Err(RecvTimeoutError::Timeout) => return Err(self.timed_out()),
                Err(RecvTimeoutError::Disconnected) => return Err(self.gone()),
            }
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), ListError> {
        self.send(&notification(method));
        Ok(())
    }
}

/// `text`, then the server's last lines under `label`.
fn with_tail(text: &str, label: &str, tail: &[String]) -> String {
    match tail {
        [] => text.to_string(),
        [one] => format!("{text} {label}: {one}"),
        many => format!("{text} {label}:\n{}", many.join("\n")),
    }
}

/// The server's stdout: each line that is JSON goes to `out`, any other line to the tail.
fn read_stdout(r: impl Read, out: &mpsc::Sender<Result<Value, String>>, tail: &Mutex<VecDeque<String>>) {
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match (&mut r).take(MAX_MESSAGE).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if buf.last() != Some(&b'\n') && buf.len() as u64 >= MAX_MESSAGE {
            let _ = out.send(Err(format!("The server sent a message over {} MB.", MAX_MESSAGE >> 20)));
            return;
        }
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_json(line) {
            Ok(v) if v.is_object() || v.is_array() => {
                if out.send(Ok(v)).is_err() {
                    return;
                }
            }
            _ => push_tail(tail, line),
        }
    }
}

/// The server's stderr, into the tail.
fn read_stderr(r: impl Read, tail: &Mutex<VecDeque<String>>) {
    let mut r = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match (&mut r).take(64 * 1024).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => push_tail(tail, &String::from_utf8_lossy(&buf)),
        }
    }
}

/// Keeps a line of the server's output (without colours, at most 300 characters), and only the last 20.
fn push_tail(tail: &Mutex<VecDeque<String>>, line: &str) {
    let line = plain(line.trim_end_matches(['\n', '\r']));
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
    t.push_back(cut(line, 300));
    while t.len() > TAIL_LINES {
        t.pop_front();
    }
}

// ---- http and sse ----

/// Headers Gizai sets itself; a server's own header with one of these names is left out.
const OWN_HEADERS: [&str; 6] = ["accept", "content-type", "content-length", "host", "mcp-session-id", "mcp-protocol-version"];

/// The HTTP side of the http and sse transports: one agent, the server's own headers, the deadline.
struct Web {
    agent: ureq::Agent,
    headers: Vec<(String, String)>,
    limit: Limit,
}

impl Web {
    fn new(t: &Target, limit: Limit) -> Result<Web, ListError> {
        let url = t.url.trim();
        if url.is_empty() {
            return Err(failed("Give the server's address (its URL)."));
        }
        let lower = url.to_ascii_lowercase();
        if !lower.starts_with("https://") && !lower.starts_with("http://") {
            return Err(failed("The server's address must start with https:// or http://."));
        }
        if url.parse::<Uri>().is_err() {
            return Err(failed(format!("{url} isn't a valid web address.")));
        }
        let mut headers = vec![];
        for (k, v) in &t.headers {
            let (k, v) = (k.trim(), v.trim());
            if k.is_empty() {
                continue;
            }
            if HeaderName::from_bytes(k.as_bytes()).is_err() {
                return Err(failed(format!("\"{k}\" isn't a valid header name.")));
            }
            if HeaderValue::from_str(v).is_err() {
                return Err(failed(format!("The value of the header {k} isn't valid: it must be one line of text.")));
            }
            if !OWN_HEADERS.contains(&k.to_ascii_lowercase().as_str()) {
                headers.push((k.to_string(), v.to_string()));
            }
        }
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // Redirects are followed by `send`, only within the server's own site and with the body sent again.
            .max_redirects(0)
            .timeout_global(Some(limit.timeout))
            .user_agent(format!("Gizai/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .new_agent();
        Ok(Web { agent, headers, limit })
    }

    /// Sends a request with the server's headers and `extra`, in what is left of `limit`, following up to 5 redirects
    /// within the same site. Returns the address that answered, and its answer, whatever its status.
    fn send(&self, limit: Limit, method: Method, url: &str, extra: &[(&str, &str)], body: Option<&str>)
        -> Result<(String, Response<ureq::Body>), ListError> {
        let mut url = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            let left = limit.left();
            if left.is_zero() {
                return Err(limit.error());
            }
            let mut b = ureq::http::Request::builder().method(method.clone()).uri(url.as_str());
            for (k, v) in &self.headers {
                b = b.header(k.as_str(), v.as_str());
            }
            for (k, v) in extra {
                b = b.header(*k, *v);
            }
            let sent = match body {
                Some(text) => b.body(text.to_string()).map(|r| r.with_agent(&self.agent).configure().timeout_global(Some(left)).run()),
                None => b.body(()).map(|r| r.with_agent(&self.agent).configure().timeout_global(Some(left)).run()),
            };
            let resp = match sent {
                Ok(Ok(resp)) => resp,
                Ok(Err(e)) => return Err(net_error(e, &url, limit)),
                Err(_) => return Err(failed(format!("{url} isn't a valid web address."))),
            };
            let location = resp.headers().get("location").and_then(|v| v.to_str().ok()).map(str::to_string);
            let (true, Some(location)) = (resp.status().is_redirection(), location) else { return Ok((url, resp)) };
            let next = resolve(&url, &location);
            if !same_site(&url, &next) {
                return Err(failed(format!("The server sent Gizai on to another site: {next}. Use that address instead.")));
            }
            url = next;
        }
        Err(failed("The server sent Gizai on too many redirects."))
    }

    fn post(&self, url: &str, extra: &[(&str, &str)], msg: &Value) -> Result<(String, Response<ureq::Body>), ListError> {
        self.send(self.limit, Method::POST, url, extra, Some(&msg.to_string()))
    }
}

/// A request that failed before any answer, in plain words.
fn net_error(e: ureq::Error, url: &str, limit: Limit) -> ListError {
    let host = host_of(url);
    match e {
        ureq::Error::Timeout(_) => limit.error(),
        ureq::Error::HostNotFound => failed(format!("Couldn't find {host}: check the address and your internet connection.")),
        ureq::Error::Io(e) => match e.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => limit.error(),
            io::ErrorKind::ConnectionRefused => failed(format!("Couldn't reach {host}: nothing answers there (connection refused).")),
            _ => failed(format!("Couldn't reach {host}: {e}.")),
        },
        ureq::Error::BadUri(_) | ureq::Error::Http(_) => failed(format!("{url} isn't a valid web address.")),
        ureq::Error::BodyExceedsLimit(_) => failed(format!("The server sent an answer over {} MB.", MAX_MESSAGE >> 20)),
        ureq::Error::Tls(_) => failed(format!("Couldn't make a secure connection to {host}: {e}.")),
        e if e.to_string().starts_with("rustls") => failed(format!("Couldn't make a secure connection to {host}: {e}.")),
        e => failed(format!("Couldn't reach {host}: {e}.")),
    }
}

/// Reading an answer that had started failed: the time ran out, or the connection broke.
fn read_error(e: &io::Error, limit: Limit) -> ListError {
    let inner = e.get_ref().and_then(|x| x.downcast_ref::<ureq::Error>());
    let timed_out = matches!(inner, Some(ureq::Error::Timeout(_))) || matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock);
    if timed_out || limit.left().is_zero() {
        return limit.error();
    }
    if matches!(inner, Some(ureq::Error::BodyExceedsLimit(_))) || e.kind() == io::ErrorKind::InvalidData {
        return failed(format!("The server sent an answer over {} MB.", MAX_MESSAGE >> 20));
    }
    failed(format!("The connection to the server broke: {e}."))
}

/// A 2xx answer as it is. 401 (or 403 with a Bearer challenge) asks for a sign-in; any other status is an error with
/// the start of what the server said.
fn check(mut resp: Response<ureq::Body>) -> Result<Response<ureq::Body>, ListError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let challenge = resp.headers().get_all("www-authenticate").iter().filter_map(|v| v.to_str().ok()).collect::<Vec<_>>().join(", ");
    let bearer = challenge.to_ascii_lowercase().split(',').any(|c| c.trim_start().starts_with("bearer"));
    if status == StatusCode::UNAUTHORIZED || (status == StatusCode::FORBIDDEN && bearer) {
        return Err(ListError::NeedsSignIn { www_authenticate: challenge });
    }
    let mut body = Vec::new();
    let _ = resp.body_mut().as_reader().take(4096).read_to_end(&mut body);
    let body = String::from_utf8_lossy(&body).split_whitespace().collect::<Vec<_>>().join(" ");
    let what = format!("{} {}", status.as_u16(), status.canonical_reason().unwrap_or("")).trim_end().to_string();
    Err(failed(if body.is_empty() { format!("The server answered {what}.") } else { format!("The server answered {what}: {}", cut(&body, 200)) }))
}

fn is_mime(resp: &Response<ureq::Body>, mime: &str) -> bool {
    resp.body().mime_type().is_some_and(|m| m.trim().eq_ignore_ascii_case(mime))
}

/// A JSON answer: the message(s) in it, none when its body is empty.
fn read_json(resp: Response<ureq::Body>, url: &str, limit: Limit) -> Result<Vec<Value>, ListError> {
    let text = resp.into_body().into_with_config().limit(MAX_MESSAGE).lossy_utf8(true).read_to_string()
        .map_err(|e| net_error(e, url, limit))?;
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    parse_json(&text).map(messages).map_err(|_| {
        let start = text.split_whitespace().collect::<Vec<_>>().join(" ");
        failed(format!("The server's answer isn't JSON: {}", cut(&start, 200)))
    })
}

/// Reads the next server-sent event: its type ("message" when it has none) and its `data:` lines joined by "\n".
/// None when the stream has ended.
fn next_event(r: &mut impl BufRead) -> io::Result<Option<(String, String)>> {
    let mut kind = String::new();
    let mut data: Option<String> = None;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if r.by_ref().take(MAX_MESSAGE).read_until(b'\n', &mut buf)? == 0 {
            return Ok(data.map(|d| (event_kind(kind), d)));
        }
        let text = String::from_utf8_lossy(&buf);
        let line = text.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            match data.take() {
                Some(d) => return Ok(Some((event_kind(kind), d))),
                None => kind.clear(),
            }
            continue;
        }
        if line.starts_with(':') {
            continue; // a comment, e.g. a keep-alive
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => kind = value.to_string(),
            "data" => match &mut data {
                Some(d) => {
                    d.push('\n');
                    d.push_str(value);
                }
                None => data = Some(value.to_string()),
            },
            _ => {}
        }
        if data.as_ref().is_some_and(|d| d.len() as u64 > MAX_MESSAGE) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "an event too long to read"));
        }
    }
}

fn event_kind(kind: String) -> String {
    if kind.is_empty() { "message".into() } else { kind }
}

/// The server's messages in one event, none when its data isn't JSON.
fn event_messages(data: &str) -> Vec<Value> {
    parse_json(data).map(messages).unwrap_or_default()
}

/// Splits a URL into its origin ("https://host:port") and the rest ("/path?query").
fn split_url(url: &str) -> Option<(&str, &str)> {
    let at = url.find("://")? + 3;
    let end = url[at..].find(['/', '?', '#']).map_or(url.len(), |i| at + i);
    Some((&url[..end], &url[end..]))
}

/// `to` (an absolute address, or a relative one like `/messages?session=1`) as an absolute address, read from `base`.
fn resolve(base: &str, to: &str) -> String {
    let to = to.trim();
    let lower = to.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return to.to_string();
    }
    let Some((origin, rest)) = split_url(base) else { return to.to_string() };
    if let Some(after) = to.strip_prefix("//") {
        let scheme = origin.split("://").next().unwrap_or("https");
        return format!("{scheme}://{after}");
    }
    if to.starts_with('/') {
        return format!("{origin}{to}");
    }
    let path = rest.split(['?', '#']).next().unwrap_or("");
    if to.starts_with('?') {
        return format!("{origin}{path}{to}");
    }
    let dir = path.rfind('/').map_or("/", |i| &path[..=i]);
    format!("{origin}{dir}{to}")
}

/// Whether `to` is on the same site as `from`: the same host and port (https may take over from http).
fn same_site(from: &str, to: &str) -> bool {
    fn site(url: &str) -> Option<(String, String)> {
        let (origin, _) = split_url(url)?;
        let (scheme, authority) = origin.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        let host = authority.rsplit('@').next().unwrap_or(authority).to_ascii_lowercase();
        let default_port = if scheme == "https" { ":443" } else { ":80" };
        let host = host.strip_suffix(default_port).map(str::to_string).unwrap_or(host);
        Some((scheme, host))
    }
    match (site(from), site(to)) {
        (Some((s1, h1)), Some((s2, h2))) => h1 == h2 && (s1 == s2 || s2 == "https"),
        _ => false,
    }
}

/// The host of an address, for errors (never its path or query, where a key could be).
fn host_of(url: &str) -> String {
    url.parse::<Uri>().ok().and_then(|u| u.host().map(str::to_string)).unwrap_or_else(|| "the server".into())
}

// ---- Streamable HTTP ----

fn list_http(t: &Target, limit: Limit) -> Result<Listing, ListError> {
    let mut conn = Http { web: Web::new(t, limit)?, url: t.url.trim().to_string(), session: None, version: None };
    let res = session(&mut conn);
    conn.close();
    res
}

/// Streamable HTTP (MCP 2025-06-18): each message is a POST; an answer comes back as JSON or as an event stream.
struct Http {
    web: Web,
    url: String,
    /// The server's Mcp-Session-Id, once it gave one.
    session: Option<String>,
    /// The protocol version the server chose, sent along after the hello.
    version: Option<String>,
}

impl Http {
    fn post(&mut self, msg: &Value) -> Result<Response<ureq::Body>, ListError> {
        let mut extra = vec![("Content-Type", "application/json"), ("Accept", "application/json, text/event-stream")];
        if let Some(v) = &self.version {
            extra.push(("MCP-Protocol-Version", v.as_str()));
        }
        if let Some(s) = &self.session {
            extra.push(("Mcp-Session-Id", s.as_str()));
        }
        let (url, resp) = self.web.post(&self.url, &extra, msg)?;
        // Later messages go straight to where a redirect led.
        self.url = url;
        let resp = check(resp)?;
        if let Some(id) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()) {
            self.session = Some(id.to_string());
        }
        Ok(resp)
    }

    /// Ends the session on the server when it gave one (DELETE), within 3 s; the answer doesn't matter.
    fn close(&self) {
        let Some(session) = &self.session else { return };
        let mut extra = vec![("Mcp-Session-Id", session.as_str())];
        if let Some(v) = &self.version {
            extra.push(("MCP-Protocol-Version", v.as_str()));
        }
        let _ = self.web.send(Limit::new(Duration::from_secs(3)), Method::DELETE, &self.url, &extra, None);
    }
}

impl Conn for Http {
    fn call(&mut self, id: u64, method: &str, params: Option<Value>) -> Result<Answer, ListError> {
        let resp = self.post(&request(id, method, params))?;
        let limit = self.web.limit;
        if !is_mime(&resp, "text/event-stream") {
            let url = self.url.clone();
            for msg in read_json(resp, &url, limit)? {
                match seen(msg, id) {
                    Seen::Answer(a) => return Ok(a),
                    Seen::Reply(r) => {
                        let _ = self.post(&r);
                    }
                    Seen::Other => {}
                }
            }
            return Err(failed("The server answered without a result."));
        }
        let mut events = BufReader::new(resp.into_body().into_reader());
        loop {
            match next_event(&mut events) {
                Ok(Some((_, data))) => {
                    for msg in event_messages(&data) {
                        match seen(msg, id) {
                            Seen::Answer(a) => return Ok(a),
                            // Its own request in the middle of the answer: the reply is a POST of its own.
                            Seen::Reply(r) => {
                                let _ = self.post(&r);
                            }
                            Seen::Other => {}
                        }
                    }
                }
                Ok(None) => return Err(failed("The server ended its answer without a result.")),
                Err(e) => return Err(read_error(&e, limit)),
            }
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), ListError> {
        // 202 is the usual answer. Only a sign-in matters here: anything else shows in the next request.
        match self.post(&notification(method)) {
            Err(e @ ListError::NeedsSignIn { .. }) => Err(e),
            _ => Ok(()),
        }
    }

    fn agreed(&mut self, version: &str) {
        if !version.is_empty() && HeaderValue::from_str(version).is_ok() {
            self.version = Some(version.to_string());
        }
    }
}

// ---- HTTP+SSE (the older transport) ----

fn list_sse(t: &Target, limit: Limit) -> Result<Listing, ListError> {
    let web = Web::new(t, limit)?;
    let accept = [("Accept", "text/event-stream"), ("Cache-Control", "no-cache")];
    let (url, resp) = web.send(limit, Method::GET, t.url.trim(), &accept, None)?;
    let resp = check(resp)?;
    if !is_mime(&resp, "text/event-stream") {
        return Err(failed("The server's address doesn't give an event stream: it may use HTTP rather than SSE."));
    }
    let mut events = BufReader::new(resp.into_body().into_reader());
    let endpoint = loop {
        match next_event(&mut events) {
            Ok(Some((kind, data))) if kind == "endpoint" => break resolve(&url, data.trim()),
            Ok(Some(_)) => {}
            Ok(None) => return Err(failed("The server's event stream ended before it said where to send messages.")),
            Err(e) => return Err(read_error(&e, limit)),
        }
    };
    // The headers (a key, a token) only ever go to the server's own site.
    if !same_site(&url, &endpoint) {
        return Err(failed(format!("The server wants its messages sent to another site ({}), which Gizai doesn't do.", host_of(&endpoint))));
    }
    session(&mut Sse { web, endpoint, events })
    // Dropping the stream closes it, which ends the session.
}

/// HTTP+SSE (MCP 2024-11-05): answers come as `message` events on one long GET; messages go by POST to the address the
/// stream's `endpoint` event gave.
struct Sse {
    web: Web,
    endpoint: String,
    events: BufReader<ureq::BodyReader<'static>>,
}

impl Sse {
    fn post(&self, msg: &Value) -> Result<Response<ureq::Body>, ListError> {
        let (_, resp) = self.web.post(&self.endpoint, &[("Content-Type", "application/json")], msg)?;
        check(resp)
    }
}

impl Conn for Sse {
    fn call(&mut self, id: u64, method: &str, params: Option<Value>) -> Result<Answer, ListError> {
        let limit = self.web.limit;
        let resp = self.post(&request(id, method, params))?;
        // The answer comes on the stream (the POST gets 202); a few servers put it in the POST's answer too.
        if is_mime(&resp, "application/json") {
            for msg in read_json(resp, &self.endpoint, limit).unwrap_or_default() {
                if let Seen::Answer(a) = seen(msg, id) {
                    return Ok(a);
                }
            }
        }
        loop {
            match next_event(&mut self.events) {
                Ok(Some((kind, data))) if kind == "message" => {
                    for msg in event_messages(&data) {
                        match seen(msg, id) {
                            Seen::Answer(a) => return Ok(a),
                            Seen::Reply(r) => {
                                let _ = self.post(&r);
                            }
                            Seen::Other => {}
                        }
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => return Err(failed("The server's event stream ended before it answered.")),
                Err(e) => return Err(read_error(&e, limit)),
            }
        }
    }

    fn notify(&mut self, method: &str) -> Result<(), ListError> {
        match self.post(&notification(method)) {
            Err(e @ ListError::NeedsSignIn { .. }) => Err(e),
            _ => Ok(()),
        }
    }
}
