//! List tools (mcp_client) against fake MCP servers: a python stdio server (tests/fake-mcp-server.py) and a std-only
//! HTTP server (tests/support/fake_mcp_http.rs); and what each tool does in plain words (mcp_tools).
use std::path::Path;
use std::time::{Duration, Instant};

use gizai_agents::mcp_client::{self, ListError, Target, Transport, list_tools, redact};
use gizai_agents::mcp_tools::{self, Hints, describe, describe_all};
use serde_json::{Value, json};

#[path = "support/fake_mcp_http.rs"]
mod fake_http;

const SERVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-mcp-server.py");
const SECRET: &str = "sk-live-NOTES-4f9a1c77e2";
const TIMEOUT: Duration = Duration::from_secs(30);

/// The fake stdio server in `mode`, with NOTES_KEY (its secret) in its environment and asked for with --env.
fn stdio(mode: &str, extra_env: &[(&str, &str)]) -> Target {
    let mut env = vec![("NOTES_KEY".to_string(), SECRET.to_string())];
    env.extend(extra_env.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    Target {
        transport: Transport::Stdio,
        command: "python3".into(),
        args: vec![SERVER.into(), mode.into(), "--env".into(), "NOTES_KEY".into()],
        env,
        ..Default::default()
    }
}

fn names(tools: &[Value]) -> Vec<&str> {
    tools.iter().map(|t| t["name"].as_str().unwrap()).collect()
}

/// Whether the process runs (a zombie or a missing /proc entry counts as gone).
fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => s.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next()).is_some_and(|st| st != "Z" && st != "X"),
        Err(_) => false,
    }
}

fn read_pid(path: &Path) -> u32 {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(s) = std::fs::read_to_string(path)
            && let Ok(pid) = s.trim().parse()
        {
            return pid;
        }
        assert!(Instant::now() < until, "no pid in {}", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Ends a process this test started (through the fake server) if it still runs, and says whether it did.
fn end_if_alive(pid: u32) -> bool {
    let was = alive(pid);
    if was {
        let _ = std::process::Command::new("kill").arg("-KILL").arg(pid.to_string()).status();
    }
    was
}

fn failed(e: ListError) -> String {
    match e {
        ListError::Failed(text) => text,
        other => panic!("expected a plain failure, got {other:?}"),
    }
}

// ---- stdio ----

#[test]
fn stdio_lists_every_tool_with_the_servers_name_and_version() {
    let l = list_tools(&stdio("hints", &[("FAKE_MCP_VERSION", "2.4.0")]), TIMEOUT).expect("listing");
    assert_eq!(l.server_name, "fake-mcp");
    // FAKE_MCP_VERSION came from the env lines: the environment reaches the server.
    assert_eq!(l.server_version, "2.4.0");
    assert_eq!(l.protocol_version, mcp_client::PROTOCOL_VERSION);
    assert_eq!(names(&l.tools), ["read_notes", "send_mail", "tag_note"]);
    assert_eq!(l.tools[0]["description"], "Reads the notes that match a query.");
}

#[test]
fn stdio_server_without_its_env_value_fails_with_its_own_words() {
    let mut t = stdio("hints", &[]);
    t.env.clear();
    let text = failed(list_tools(&t, TIMEOUT).unwrap_err());
    assert!(text.contains("The server ended before it answered (exit code 4)"), "{text}");
    assert!(text.contains("NOTES_KEY is not set"), "{text}");
}

#[test]
fn stdio_reads_every_page_of_tools_list() {
    let l = list_tools(&stdio("paged", &[]), TIMEOUT).expect("listing");
    assert_eq!(names(&l.tools), ["p1_a", "p1_b", "p2_a", "p3_a"]);
}

#[test]
fn stdio_server_is_ended_after_list_tools() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("server.pid");
    list_tools(&stdio("hints", &[("FAKE_MCP_PIDFILE", pidfile.to_str().unwrap())]), TIMEOUT).expect("listing");
    let pid = read_pid(&pidfile);
    assert!(!end_if_alive(pid), "the server (pid {pid}) still runs after List tools");
}

#[test]
fn stdio_server_is_ended_after_a_failed_list_tools() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("server.pid");
    let err = list_tools(&stdio("rpcerror", &[("FAKE_MCP_PIDFILE", pidfile.to_str().unwrap())]), TIMEOUT).unwrap_err();
    assert!(matches!(err, ListError::Failed(_)), "{err:?}");
    let pid = read_pid(&pidfile);
    assert!(!end_if_alive(pid), "the server (pid {pid}) still runs after a failed List tools");
}

#[test]
fn stdio_servers_child_in_its_own_process_group_is_gone_after_list_tools() {
    let dir = tempfile::tempdir().unwrap();
    let (pidfile, childfile) = (dir.path().join("server.pid"), dir.path().join("child.pid"));
    list_tools(&stdio("hints", &[("FAKE_MCP_PIDFILE", pidfile.to_str().unwrap()), ("FAKE_MCP_CHILD_PIDFILE", childfile.to_str().unwrap())]), TIMEOUT)
        .expect("listing");
    let (pid, child) = (read_pid(&pidfile), read_pid(&childfile));
    assert_ne!(pid, child);
    let (server_left, child_left) = (end_if_alive(pid), end_if_alive(child));
    assert!(!server_left, "the server (pid {pid}) still runs after List tools");
    assert!(!child_left, "the server's setsid child (pid {child}) still runs after List tools");
}

#[test]
fn stdio_server_that_ignores_sigterm_is_killed_after_the_grace_time() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("server.pid");
    let start = Instant::now();
    let l = list_tools(&stdio("ignoreterm", &[("FAKE_MCP_PIDFILE", pidfile.to_str().unwrap())]), TIMEOUT).expect("listing");
    let took = start.elapsed();
    assert_eq!(l.tools.len(), 3);
    let pid = read_pid(&pidfile);
    assert!(!end_if_alive(pid), "the server that ignores SIGTERM (pid {pid}) still runs after List tools");
    assert!(took >= Duration::from_millis(2500) && took < Duration::from_secs(10), "took {took:?}");
}

#[test]
fn stdio_server_that_fails_at_start_shows_its_exit_code_and_last_lines_with_the_secret_as_dots() {
    let text = failed(list_tools(&stdio("fail", &[]), TIMEOUT).unwrap_err());
    assert!(text.starts_with("The server ended before it answered (exit code 3)"), "{text}");
    // Both its stdout line that isn't JSON and its stderr line are in the error, each with the value hidden.
    assert!(text.contains("starting with NOTES_KEY=•••"), "{text}");
    assert!(text.contains("error: the server refused the key (NOTES_KEY=•••)"), "{text}");
    assert!(!text.contains(SECRET), "{text}");
}

#[test]
fn stdio_json_rpc_error_shows_the_servers_message_with_the_secret_as_dots() {
    let text = failed(list_tools(&stdio("rpcerror", &[]), TIMEOUT).unwrap_err());
    assert_eq!(text, "The server answered with an error: bad credentials: NOTES_KEY=•••");
}

#[test]
fn stdio_server_that_never_answers_times_out_with_its_last_lines_and_no_secret() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("server.pid");
    let start = Instant::now();
    let text = failed(list_tools(&stdio("hang", &[("FAKE_MCP_PIDFILE", pidfile.to_str().unwrap())]), Duration::from_secs(2)).unwrap_err());
    assert!(start.elapsed() < Duration::from_secs(8), "{:?}", start.elapsed());
    assert!(text.starts_with("The server didn't answer within 2 s."), "{text}");
    assert!(text.contains("connecting with NOTES_KEY=•••"), "{text}");
    assert!(!text.contains(SECRET), "{text}");
    let pid = read_pid(&pidfile);
    assert!(!end_if_alive(pid), "the server (pid {pid}) still runs after it timed out");
}

#[test]
fn a_command_that_isnt_there_says_so_in_plain_words() {
    let t = Target { command: "/nonexistent/gizai-fake-mcp".into(), ..Default::default() };
    let text = failed(list_tools(&t, TIMEOUT).unwrap_err());
    assert_eq!(text, "Couldn't start /nonexistent/gizai-fake-mcp: not found. Give the full path, or a program in your PATH.");
    let empty = failed(list_tools(&Target::default(), TIMEOUT).unwrap_err());
    assert_eq!(empty, "Give the command that starts the server.");
}

#[test]
fn a_targets_debug_shows_the_names_of_its_env_and_headers_never_their_values() {
    let mut t = stdio("hints", &[]);
    t.headers = vec![("Authorization".into(), "Bearer tok-ABCDEF-123456".into())];
    let shown = format!("{t:?}");
    assert!(shown.contains("NOTES_KEY") && shown.contains("Authorization"), "{shown}");
    assert!(!shown.contains(SECRET) && !shown.contains("tok-ABCDEF-123456"), "{shown}");
}

#[test]
fn redact_hides_secrets_longest_first_and_leaves_short_ones() {
    assert_eq!(redact("key abcdef and abcdefgh", &["abcdef", "abcdefgh"]), "key ••• and •••");
    // Under 4 characters would hide ordinary words.
    assert_eq!(redact("the cat sat", &["cat"]), "the cat sat");
    let t = Target { headers: vec![("Authorization".into(), "Bearer tok-ABCDEF-123456".into())], ..Default::default() };
    // The token without its scheme is a secret too.
    assert!(t.secrets().contains(&"tok-ABCDEF-123456".to_string()), "{:?}", t.secrets().len());
}

// ---- Streamable HTTP and HTTP+SSE ----

const API_KEY: &str = "key-HTTP-77c1d9e0aa";
const TOKEN: &str = "tok-BEARER-5b2e8f90c3";

fn http(server: &fake_http::FakeHttp, transport: Transport) -> Target {
    Target {
        transport,
        url: server.url(),
        headers: vec![("X-Api-Key".into(), API_KEY.into()), ("Authorization".into(), format!("Bearer {TOKEN}"))],
        ..Default::default()
    }
}

#[test]
fn http_lists_tools_and_sends_the_header_lines_with_every_request() {
    let server = fake_http::start(fake_http::Mode::Ok);
    let l = list_tools(&http(&server, Transport::Http), TIMEOUT).expect("listing");
    assert_eq!((l.server_name.as_str(), l.server_version.as_str()), ("fake-http", "3.1.4"));
    assert_eq!(names(&l.tools), ["search", "publish"]);
    let seen = server.seen();
    let rpcs: Vec<String> = seen.iter().filter(|s| s.method == "POST").map(|s| s.rpc()).collect();
    assert_eq!(rpcs, ["initialize", "notifications/initialized", "tools/list"]);
    for s in &seen {
        assert_eq!(s.header("x-api-key"), Some(API_KEY), "{} {}", s.method, s.rpc());
        assert_eq!(s.header("authorization"), Some(format!("Bearer {TOKEN}").as_str()), "{} {}", s.method, s.rpc());
    }
    // After the hello: the session and the protocol version go along, and the session is ended with a DELETE.
    let list = seen.iter().find(|s| s.rpc() == "tools/list").unwrap();
    assert_eq!(list.header("mcp-session-id"), Some("sess-42"));
    assert_eq!(list.header("mcp-protocol-version"), Some("2025-06-18"));
    let delete = seen.iter().find(|s| s.method == "DELETE").expect("the session is ended");
    assert_eq!(delete.header("mcp-session-id"), Some("sess-42"));
}

#[test]
fn http_error_body_that_echoes_the_header_values_shows_dots() {
    let server = fake_http::start(fake_http::Mode::EchoError);
    let text = failed(list_tools(&http(&server, Transport::Http), TIMEOUT).unwrap_err());
    assert!(text.starts_with("The server answered 500 Internal Server Error: Invalid API key: •••"), "{text}");
    assert!(!text.contains(API_KEY) && !text.contains(TOKEN), "{text}");
}

#[test]
fn http_401_asks_for_sign_in_with_the_servers_challenge() {
    let server = fake_http::start(fake_http::Mode::NeedsSignIn);
    match list_tools(&http(&server, Transport::Http), TIMEOUT).unwrap_err() {
        ListError::NeedsSignIn { www_authenticate } => {
            assert!(www_authenticate.starts_with("Bearer resource_metadata="), "{www_authenticate}");
        }
        other => panic!("expected NeedsSignIn, got {other:?}"),
    }
}

#[test]
fn http_server_that_isnt_there_says_so_without_its_path() {
    // A port nothing listens on: bind one, then let it go.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let t = Target { transport: Transport::Http, url: format!("http://127.0.0.1:{port}/mcp?key={API_KEY}"), ..Default::default() };
    let text = failed(list_tools(&t, TIMEOUT).unwrap_err());
    assert_eq!(text, "Couldn't reach 127.0.0.1: nothing answers there (connection refused).");
}

#[test]
fn sse_lists_tools_over_the_older_transport_with_the_header_lines() {
    let server = fake_http::start(fake_http::Mode::Sse);
    let l = list_tools(&http(&server, Transport::Sse), TIMEOUT).expect("listing");
    assert_eq!(names(&l.tools), ["search", "publish"]);
    let seen = server.seen();
    assert_eq!(seen[0].method, "GET");
    assert!(seen.iter().skip(1).all(|s| s.method == "POST" && s.path == "/messages?session=7"), "{seen:?}");
    for s in &seen {
        assert_eq!(s.header("x-api-key"), Some(API_KEY), "{} {}", s.method, s.path);
    }
}

// ---- what each tool does (AC7) ----

fn tool<'a>(views: &'a [mcp_tools::ToolView], name: &str) -> &'a mcp_tools::ToolView {
    views.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("no tool {name}"))
}

#[test]
fn list_tools_from_a_server_that_sends_some_hints_shows_each_tool_in_plain_words_with_a_risk() {
    let l = list_tools(&stdio("hints", &[]), TIMEOUT).expect("listing");
    let views = describe_all(&l.tools);
    assert_eq!(views.len(), 3);

    // Otus OS style: only readOnlyHint true, no title.
    let read = tool(&views, "read_notes");
    assert_eq!(read.title, None);
    assert_eq!(read.description, "Reads the notes that match a query.");
    assert_eq!(read.risk, "low");
    assert_eq!(read.hints_sent, ["readOnlyHint"]);
    assert!(read.hints.read_only && !read.hints.destructive);
    assert!(read.hints.open_world, "openWorldHint defaults to true");
    assert_eq!(read.summary, "Only reads. Reaches outside services.");
    assert!(read.notes.iter().any(|n| n.contains("MCP defaults")), "{:?}", read.notes);
    // (Their order: see parameters_are_listed_in_the_schemas_order.)
    let mut p: Vec<(&str, &str, bool, &str)> = read.params.iter().map(|p| (p.name.as_str(), p.ty.as_str(), p.required, p.description.as_str())).collect();
    p.sort();
    assert_eq!(p, [("limit", "integer", false, "How many notes at most"), ("query", "string", true, "Words to look for"), ("tags", "array of string", false, "")]);

    // Otus OS style: only readOnlyHint false; the rest follow the defaults (destructive, not idempotent, open world).
    let send = tool(&views, "send_mail");
    assert_eq!(send.risk, "high");
    assert_eq!(send.hints_sent, ["readOnlyHint"]);
    assert_eq!(send.hints, Hints { read_only: false, destructive: true, idempotent: false, open_world: true });
    assert_eq!(send.summary, "May delete or overwrite. Reaches outside services.");
    let mut types: Vec<(&str, &str)> = send.params.iter().map(|p| (p.name.as_str(), p.ty.as_str())).collect();
    types.sort();
    assert_eq!(types, [("body", "string | null"), ("priority", "one of low, high"), ("to", "string")]);

    // All four hints sent, and a title in its annotations.
    let tag = tool(&views, "tag_note");
    assert_eq!(tag.title.as_deref(), Some("Tag a note"));
    assert_eq!(tag.risk, "medium");
    assert_eq!(tag.hints_sent, ["readOnlyHint", "destructiveHint", "idempotentHint", "openWorldHint"]);
    assert_eq!(tag.hints, Hints { read_only: false, destructive: false, idempotent: true, open_world: false });
    assert_eq!(tag.summary, "Changes things, but doesn't delete or overwrite. Safe to repeat.");
    assert!(!tag.notes.iter().any(|n| n.contains("didn't say")), "{:?}", tag.notes);
}

#[test]
fn list_tools_from_a_server_that_sends_no_hints_follows_the_mcp_defaults() {
    let l = list_tools(&stdio("nohints", &[]), TIMEOUT).expect("listing");
    let views = describe_all(&l.tools);
    assert_eq!(views.len(), 2);
    for v in &views {
        assert!(v.hints_sent.is_empty(), "{}", v.name);
        // readOnlyHint false, destructiveHint true, idempotentHint false, openWorldHint true
        assert_eq!(v.hints, Hints { read_only: false, destructive: true, idempotent: false, open_world: true }, "{}", v.name);
        assert_eq!(v.risk, "high", "{}", v.name);
        assert_eq!(v.summary, "May delete or overwrite. Reaches outside services.");
        assert_eq!(v.notes.len(), 1);
        assert!(v.notes[0].starts_with("The server sent no hints about this tool"), "{:?}", v.notes);
    }
    let lookup = tool(&views, "lookup");
    assert_eq!(lookup.description, "Looks something up.");
    assert_eq!(lookup.params[0].name, "q");
    assert!(!lookup.params[0].required);
}

#[test]
fn describe_says_which_hints_were_sent_and_assumes_the_defaults_for_the_rest() {
    let v = describe(&json!({"name": "rm", "annotations": {"destructiveHint": false}}));
    assert_eq!(v.hints_sent, ["destructiveHint"]);
    assert_eq!(v.hints, Hints { read_only: false, destructive: false, idempotent: false, open_world: true });
    assert_eq!(v.risk, "medium");
    assert_eq!(v.notes.len(), 2, "{:?}", v.notes);
    assert!(v.notes[1].contains("only reads") && v.notes[1].contains("safe to repeat") && v.notes[1].contains("reaches outside services"), "{:?}", v.notes);

    // Hints that aren't true or false count as not sent.
    let v = describe(&json!({"name": "odd", "annotations": {"readOnlyHint": "yes"}}));
    assert!(v.hints_sent.is_empty());
    assert_eq!(v.risk, "high");

    // No inputSchema: no parameters; a parameter without a type is "any".
    assert!(describe(&json!({"name": "bare"})).params.is_empty());
    let v = describe(&json!({"name": "x", "inputSchema": {"type": "object", "properties": {"v": {}, "w": {"anyOf": [{"type": "string"}, {"type": "number"}]}}}}));
    assert_eq!(v.params.iter().map(|p| p.ty.as_str()).collect::<Vec<_>>(), ["any", "string | number"]);
}

/// mcp_tools::params says "in the schema's order"; the form shows them in that order, as the server wrote them.
#[test]
fn parameters_are_listed_in_the_schemas_order() {
    let l = list_tools(&stdio("hints", &[]), TIMEOUT).expect("listing");
    let views = describe_all(&l.tools);
    let order = |name: &str| tool(&views, name).params.iter().map(|p| p.name.clone()).collect::<Vec<_>>();
    assert_eq!(order("read_notes"), ["query", "limit", "tags"]);
    assert_eq!(order("send_mail"), ["to", "body", "priority"]);
}

#[test]
fn describe_all_leaves_out_tools_without_a_name() {
    let views = describe_all(&[json!({"description": "nameless"}), json!({"name": "  "}), json!({"name": "ok"})]);
    assert_eq!(views.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["ok"]);
}
