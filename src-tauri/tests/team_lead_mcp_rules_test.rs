// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-39: the Team Lead's rules around MCP servers.
// - After a chat answer used a tool from outside Gizai (an MCP server of its own, the web, the browser), Gizai's tools
//   that act are refused for the rest of that answer, and work again in the next message.
// - The Team Lead can't switch an agent's MCP servers or their tools, and no tool of its adds, imports or signs in to one.
// - Quitting ends the MCP servers a chat answer started (the process side is in gizai-agents' mcp_cleanup_test).
// Tools are called the way the MCP server calls them; chat turns run a fake Claude Code (never the real one) that
// starts the real gizai-mcp shim.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::mcp_servers::{self as core_mcp, AgentServer, AgentTools, McpServer};
use gizai_core::model::*;
use gizai_core::{chat, projects, team};
use gizai_lib::mcp_servers::{SecretLine, ServerInput};
use gizai_lib::{AppState, chat as app_chat, mcp, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat-outside.py");
const FAKE_MCP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-mcp.py");
const OTUS_SECRET: &str = "otus-secret-value-123";
const LINEAR_SECRET: &str = "Bearer linear-secret-value-456";

/// The shim binary: target/debug/gizai-mcp (built here when missing).
fn shim() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/gizai-mcp"));
    BUILT.call_once(|| {
        if path.is_file() {
            return;
        }
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = std::process::Command::new(cargo).args(["build", "-q", "-p", "gizai-mcp"])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).status().map(|s| s.success()).unwrap_or(false);
        assert!(ok && path.is_file(), "could not build gizai-mcp");
    });
    path
}

struct T {
    st: AppState,
    lead: String,
    backend: String,
    otus: String,
    linear: String,
    _dir: tempfile::TempDir,
}

/// A Team Lead, a Backend Agent with Otus on (its delete_note tool off) and Linear off, and KADE-1.
fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(dir.path());
    st.mcp_shim = Some(shim());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let backend = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let otus = gizai_lib::mcp_servers::save(&st, ServerInput {
        server: McpServer { name: "otus".into(), transport: "stdio".into(), command: "/opt/otus/otus-mcp".into(), ..Default::default() },
        env: vec![SecretLine { name: "OTUS_TOKEN".into(), value: Some(OTUS_SECRET.into()) }], headers: vec![],
    }).unwrap().server.id;
    let linear = gizai_lib::mcp_servers::save(&st, ServerInput {
        server: McpServer { name: "linear".into(), transport: "http".into(), url: "https://mcp.linear.example/mcp".into(), ..Default::default() },
        env: vec![], headers: vec![SecretLine { name: "Authorization".into(), value: Some(LINEAR_SECRET.into()) }],
    }).unwrap().server.id;
    core_mcp::set_agent_tools(&st.db, &st.you_id, &backend, AgentTools { mcp: vec![
        AgentServer { server_id: otus.clone(), on: true, tools_off: vec!["delete_note".into()] },
        AgentServer { server_id: linear.clone(), on: false, tools_off: vec![] },
    ] }).unwrap();
    T { st, lead, backend, otus, linear, _dir: dir }
}

impl T {
    async fn call(&self, thread: Option<&str>, name: &str, args: Value) -> Result<Value, String> {
        tools::call_in(&self.st, &self.lead, thread, name, args).await
    }
    async fn ok(&self, thread: Option<&str>, name: &str, args: Value) -> Value {
        self.call(thread, name, args).await.unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }
    fn thread(&self, text: &str) -> String {
        let id = chat::create_thread(&self.st.db, &self.st.you_id, &self.lead, text).unwrap();
        chat::add_message(&self.st.db, chat::NewMessage { thread_id: id.clone(), role: "user".into(), author_id: Some(self.st.you_id.clone()),
            body_md: Some(text.into()), ..Default::default() }).unwrap();
        id
    }
    fn backend_tools(&self) -> AgentTools {
        core_mcp::agent_tools(&self.st.db, &self.backend).unwrap()
    }
    fn agent_names(&self) -> Vec<String> {
        team::all_agents(&self.st.db).unwrap().into_iter().map(|(_, m)| m.name).collect()
    }
    fn columns(&self) -> Vec<String> {
        let team_id = team::list(&self.st.db).unwrap()[0].id.clone();
        team::get(&self.st.db, &team_id).unwrap().states.into_iter().map(|s| s.name).collect()
    }
}

fn refused_after_outside(e: &str, name: &str) -> bool {
    e.starts_with(&format!("{name} is refused for the rest of this answer")) && e.contains("mcp__otus__search")
        && e.contains("confirm it in a new message")
}

// ---- what counts as outside ----

#[test]
fn outside_tools_are_all_but_read_glob_grep_and_gizais_own() {
    for inside in ["Read", "Glob", "Grep", "mcp__gizai__get_overview", "mcp__gizai__set_agent_status", "mcp__gizai__x"] {
        assert!(!app_chat::is_outside_tool(inside), "{inside} is not outside");
    }
    for outside in ["mcp__otus__list", "mcp__otus__search", "WebSearch", "WebFetch", "mcp__chrome-devtools__navigate_page", "mcp__chrome-devtools__x",
                    "Bash", "Edit", "Write", "Task", "mcp__gizai", "mcp__gizaiX__get_overview", "mcp__linear__create_issue", "read", "mcp__Gizai__get_overview"] {
        assert!(app_chat::is_outside_tool(outside), "{outside} is outside");
    }
}

/// Claude Code names a server's tools mcp__<server>__<tool>. A server called "gizai_" or "gizai__notes" would have tools
/// named mcp__gizai___x or mcp__gizai__notes__x, which pass for Gizai's own: either such a name can't be saved, or its
/// tools count as outside.
#[test]
fn a_server_whose_tools_would_pass_for_gizais_own_is_refused_or_counts_as_outside() {
    let t = setup();
    let mut passing = vec![];
    for name in ["gizai_", "gizai__notes", "gizai___x"] {
        let saved = core_mcp::save(&t.st.db, McpServer { name: name.into(), transport: "stdio".into(), command: "/opt/x".into(), ..Default::default() });
        let tool = format!("mcp__{name}__search");
        if saved.is_ok() && !app_chat::is_outside_tool(&tool) {
            passing.push(format!("{name:?} (its tool {tool})"));
        }
    }
    assert!(passing.is_empty(), "servers saved whose tools count as Gizai's own, so using them doesn't stop the acting tools: {}", passing.join(", "));
}

/// Both halves of the fix on their own: such names are refused with the reason, ordinary ones with a single _ or - still
/// work, and a tool name that only looks like Gizai's own counts as outside even if such a server were there.
#[test]
fn server_names_with_two_underscores_or_a_trailing_one_are_refused_and_their_tools_count_as_outside() {
    let t = setup();
    for name in ["gizai_", "gizai__notes", "gizai___x", "otus__x", "otus_", "a__b", "__otus", "Otus__Notes"] {
        let e = core_mcp::save(&t.st.db, McpServer { name: name.into(), transport: "stdio".into(), command: "/opt/x".into(), ..Default::default() })
            .expect_err(name).to_string();
        assert!(e.contains("can't have two _ in a row or end with _") && e.contains(&format!("\"{name}\"")), "{name}: {e}");
        assert!(!core_mcp::clear_in_tool_names(name), "{name}");
    }
    for name in ["notes", "my_server", "otus-os", "a_b_c", "x-_y", "gizai-notes", "otus2"] {
        core_mcp::save(&t.st.db, McpServer { name: name.into(), transport: "stdio".into(), command: "/opt/x".into(), ..Default::default() })
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
    let names: Vec<String> = core_mcp::list(&t.st.db).unwrap().into_iter().map(|s| s.name).collect();
    assert!(!names.iter().any(|n| n.contains("__") || n.ends_with('_')), "{names:?}");
    // editing a saved server into such a name is refused too
    let mut otus = core_mcp::list(&t.st.db).unwrap().into_iter().find(|s| s.id == t.otus).unwrap();
    otus.name = "otus__x".into();
    assert!(core_mcp::save(&t.st.db, otus).is_err());
    for outside in ["mcp__gizai__", "mcp__gizai___x", "mcp__gizai____x", "mcp__gizai__notes__search", "mcp__gizai__notes__", "mcp__gizai_x__y"] {
        assert!(app_chat::is_outside_tool(outside), "{outside} is outside");
    }
    for inside in ["mcp__gizai__create_task", "mcp__gizai__start_agent_run", "mcp__gizai__get_overview"] {
        assert!(!app_chat::is_outside_tool(inside), "{inside} is Gizai's own");
    }
}

// ---- after an outside tool, in that answer ----

#[tokio::test]
async fn after_an_outside_tool_the_acting_tools_are_refused_for_the_rest_of_the_answer_and_change_nothing() {
    let t = setup();
    t.ok(None, "create_task", json!({"project": "KADE", "title": "Export"})).await;
    let f = t._dir.path().join("brief.txt");
    std::fs::write(&f, "the brief").unwrap();
    let thread = t.thread(&format!("Look up the release notes in Otus and attach {} to KADE-1", f.display()));
    gizai_lib::chat::mark_outside(&t.st, &thread, "mcp__otus__search");
    // a second outside tool in the same answer: the first one is the one named
    gizai_lib::chat::mark_outside(&t.st, &thread, "WebFetch");
    assert_eq!(app_chat::used_outside(&t.st, &thread).as_deref(), Some("mcp__otus__search"));
    let before = (t.agent_names(), t.columns(), team::agent(&t.st.db, &t.backend).unwrap());
    let calls: [(&str, Value); 10] = [
        ("start_agent_run", json!({"task": "KADE-1", "agent": "Backend Agent"})),
        ("continue_agent_run", json!({"task": "KADE-1"})),
        ("create_agent", json!({"name": "Otus Agent", "role": "backend"})),
        ("update_agent", json!({"agent": "Backend Agent", "title": "Steered", "model": "opus"})),
        ("set_agent_status", json!({"agent": "Backend Agent", "status": "paused"})),
        ("add_column", json!({"name": "Otus", "after": "Review", "kind": "work"})),
        ("set_column", json!({"column": "Testing", "auto": true, "new_name": "Steered"})),
        ("attach_file", json!({"path": f.display().to_string(), "task": "KADE-1"})),
        ("update_checkout", json!({"project": "KADE"})),
        // GA-86: merging a pull request
        ("merge_pull_request", json!({"task": "KADE-1"})),
    ];
    assert_eq!(calls.iter().map(|(n, _)| *n).collect::<Vec<_>>(), tools::NOT_AFTER_OUTSIDE, "every tool in the list is tried");
    for (name, args) in calls {
        let t0 = Instant::now();
        let e = t.call(Some(&thread), name, args).await.expect_err(name);
        assert!(refused_after_outside(&e, name), "{name}: {e}");
        assert!(t0.elapsed() < Duration::from_millis(140), "{name}: refused at once once marked, took {:?}", t0.elapsed());
    }
    let after = (t.agent_names(), t.columns(), team::agent(&t.st.db, &t.backend).unwrap());
    assert_eq!(before.0, after.0, "no agent was added");
    assert_eq!(before.1, after.1, "no column was added or renamed");
    assert_eq!((after.2.status.as_str(), after.2.title.clone(), after.2.model.clone()), (before.2.status.as_str(), before.2.title.clone(), before.2.model.clone()));
    let task_id = t.ok(None, "get_task", json!({"task": "KADE-1"})).await["task"]["id"].as_str().unwrap().to_string();
    assert!(gizai_core::files::list(&t.st.db, "task", &task_id).unwrap().is_empty(), "nothing attached");
    assert!(gizai_core::runs::list_for_agent(&t.st.db, &t.backend, 10).unwrap().is_empty(), "no run started");
}

#[tokio::test]
async fn after_an_outside_tool_reading_tools_and_tools_not_in_the_list_still_work_in_that_answer() {
    let t = setup();
    t.ok(None, "create_task", json!({"project": "KADE", "title": "Export"})).await;
    let thread = t.thread("What does Otus say about KADE-1?");
    gizai_lib::chat::mark_outside(&t.st, &thread, "mcp__otus__search");
    for (name, args) in [("get_overview", json!({})), ("read_inbox", json!({})), ("list_agents", json!({})), ("get_agent", json!({"agent": "Backend Agent"})),
                         ("get_task", json!({"task": "KADE-1"})), ("get_workflow", json!({})), ("list_tasks", json!({})), ("check_board", json!({})),
                         ("comment_on_task", json!({"task": "KADE-1", "body_md": "Otus says the export is due Friday."}))] {
        t.call(Some(&thread), name, args).await.unwrap_or_else(|e| panic!("{name} should still work after an outside tool: {e}"));
    }
    let task = t.ok(None, "get_task", json!({"task": "KADE-1"})).await;
    assert!(task.to_string().contains("due Friday"), "the comment was added: {task}");
}

#[tokio::test]
async fn the_mark_holds_only_in_its_own_thread_and_never_for_calls_without_a_thread() {
    let t = setup();
    let marked = t.thread("Search Otus");
    let other = t.thread("Pause the Backend Agent");
    gizai_lib::chat::mark_outside(&t.st, &marked, "mcp__otus__search");
    assert!(app_chat::used_outside(&t.st, &other).is_none());
    let e = t.call(Some(&marked), "set_agent_status", json!({"agent": "Backend Agent", "status": "paused"})).await.unwrap_err();
    assert!(refused_after_outside(&e, "set_agent_status"), "{e}");
    // another chat's answer
    t.ok(Some(&other), "set_agent_status", json!({"agent": "Backend Agent", "status": "paused"})).await;
    assert_eq!(team::agent(&t.st.db, &t.backend).unwrap().status, "paused");
    t.ok(Some(&other), "add_column", json!({"name": "Design review", "after": "Review", "kind": "review"})).await;
    // no chat at all (a board check's or the UI's own calls)
    t.ok(None, "set_agent_status", json!({"agent": "Backend Agent", "status": "active"})).await;
    t.ok(None, "create_agent", json!({"name": "Docs Agent", "role": "docs"})).await;
    t.ok(None, "update_agent", json!({"agent": "Docs Agent", "title": "Writer"})).await;
    t.ok(None, "set_column", json!({"column": "Design review", "new_name": "UX review"})).await;
    assert_eq!(team::agent(&t.st.db, &t.backend).unwrap().status, "active");
    assert!(t.columns().contains(&"UX review".to_string()), "{:?}", t.columns());
}

// ---- an agent's MCP servers: only the user switches them ----

#[tokio::test]
async fn create_and_update_agent_refuse_mcp_servers_and_their_tool_switches_and_change_nothing() {
    let t = setup();
    let before = t.backend_tools();
    assert_eq!(before.servers_on().count(), 1);
    let linear_on = json!([{"serverId": t.linear, "on": true, "toolsOff": []}]);
    let attempts = [
        ("mcp_servers", json!([{"name": "linear", "on": true}])),
        ("mcp_servers", json!(["linear"])),
        ("mcp", json!({"mcp": linear_on})),
        ("mcp", linear_on.clone()),
        ("tools", json!({"otus": {"delete_note": true}})),
        ("tools", json!("mcp__otus__delete_note")),
        ("mcp_tools", json!({"otus": {"tools_off": []}})),
        ("mcp_tools", json!(false)),
    ];
    for (key, value) in &attempts {
        let mut args = json!({"agent": "Backend Agent", "title": "Changed"});
        args[*key] = value.clone();
        let e = t.call(None, "update_agent", args).await.expect_err(key);
        assert!(e.contains("MCP servers") && e.contains("agent form"), "update_agent {key}: {e}");
        let mut args = json!({"name": "Otus Agent", "role": "backend"});
        args[*key] = value.clone();
        let e = t.call(None, "create_agent", args).await.expect_err(key);
        assert!(e.contains("MCP servers") && e.contains("agent form"), "create_agent {key}: {e}");
    }
    assert_eq!(t.backend_tools(), before, "the Backend Agent's switches are as they were");
    assert!(team::agent(&t.st.db, &t.backend).unwrap().title.is_none(), "a refused update changes nothing else either");
    assert!(!t.agent_names().contains(&"Otus Agent".to_string()), "no agent was created");
    // also in a chat thread, without an outside tool
    let thread = t.thread("Turn Linear on for the Backend Agent");
    let e = t.call(Some(&thread), "update_agent", json!({"agent": "Backend Agent", "mcp_servers": ["linear"]})).await.unwrap_err();
    assert!(e.contains("MCP servers"), "{e}");
    assert_eq!(t.backend_tools(), before);
}

/// Keys a model might try instead: they aren't in the tool's schema, so they must not reach the agent's switches either.
#[tokio::test]
async fn other_keys_a_model_might_send_dont_change_an_agents_mcp_servers() {
    let t = setup();
    let before = t.backend_tools();
    let raw = serde_json::to_string(&AgentTools { mcp: vec![AgentServer { server_id: t.linear.clone(), on: true, tools_off: vec![] }] }).unwrap();
    let mut outcomes = vec![];
    for (key, value) in [
        ("mcpServers", json!({"linear": {"type": "http", "url": "https://mcp.linear.example/mcp"}})),
        ("mcp_extra_json", json!(raw)),
        ("mcpExtraJson", json!(raw)),
        ("mcp_config", json!({"mcpServers": {"evil": {"command": "/tmp/evil"}}})),
        ("servers", json!(["linear"])),
        ("tools_off", json!([])),
        ("toolsOff", json!([])),
        ("enabled_mcp_servers", json!(["linear"])),
        ("disallowed_tools", json!([])),
    ] {
        let mut args = json!({"agent": "Backend Agent"});
        args[key] = value.clone();
        let r = t.call(None, "update_agent", args).await;
        outcomes.push(format!("{key}: {}", match &r { Ok(_) => "accepted, ignored".to_string(), Err(e) => format!("refused: {e}") }));
        assert_eq!(t.backend_tools(), before, "update_agent with {key} changed the switches");
        let mut args = json!({"name": format!("Agent {key}"), "role": "backend"});
        args[key] = value;
        let r = t.call(None, "create_agent", args).await;
        if let Ok(v) = r {
            let id = v["link"]["id"].as_str().unwrap().to_string();
            assert_eq!(core_mcp::agent_tools(&t.st.db, &id).unwrap(), AgentTools::default(), "create_agent with {key} has a server on");
        }
    }
    eprintln!("other keys on update_agent:\n  {}", outcomes.join("\n  "));
}

/// allowed_tools is the agent's commands list (the chat may set it): naming an MCP server or tool there gives a run
/// nothing, because a server that's off isn't in the run's MCP config, and a tool switched off is refused outright.
#[tokio::test]
async fn allowed_tools_naming_a_server_that_is_off_or_a_tool_switched_off_gives_a_run_nothing() {
    let t = setup();
    let before = t.backend_tools();
    let r = t.call(None, "update_agent", json!({"agent": "Backend Agent", "allowed_tools": ["Bash(npm test:*)", "mcp__linear", "mcp__otus__delete_note", "mcp__otus"]})).await;
    eprintln!("update_agent with allowed_tools naming MCP tools: {}", match &r { Ok(_) => "accepted".to_string(), Err(e) => format!("refused: {e}") });
    assert_eq!(t.backend_tools(), before, "the switches are as they were");
    let agent = team::agent(&t.st.db, &t.backend).unwrap();
    let (servers, notes) = gizai_lib::mcp_servers::for_run(&t.st, &agent, Duration::from_secs(3600));
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["otus"], "Linear is off, so no run gets it");
    let config = gizai_agents::mcp_run::config(vec![], &servers);
    assert!(config["mcpServers"].get("linear").is_none(), "{config}");
    let (_allowed, refused) = gizai_agents::mcp_run::permissions(&servers);
    assert!(refused.contains(&"mcp__otus__delete_note".to_string()), "a tool switched off is refused (--disallowedTools): {refused:?}");
}

#[tokio::test]
async fn get_agent_shows_the_switches_and_never_a_secret() {
    let t = setup();
    let a = t.ok(None, "get_agent", json!({"agent": "Backend Agent"})).await;
    let mut servers = a["agent"]["mcp_servers"].as_array().unwrap().clone();
    servers.sort_by_key(|s| s["name"].as_str().unwrap().to_string());
    assert_eq!(servers, vec![
        json!({"name": "linear", "on": false, "tools_off": []}),
        json!({"name": "otus", "on": true, "tools_off": ["delete_note"]}),
    ]);
    // the Team Lead sees no switches of its own, and the list holds names only
    let lead = t.ok(None, "get_agent", json!({"agent": "Team Lead"})).await;
    assert_eq!(lead["agent"]["mcp_servers"], json!([]));
    for v in [&a, &lead, &t.ok(None, "list_agents", json!({})).await, &t.ok(None, "get_overview", json!({})).await] {
        let s = v.to_string();
        for secret in [OTUS_SECRET, LINEAR_SECRET, "linear-secret-value-456", "/opt/otus/otus-mcp", &t.otus, &t.linear] {
            assert!(!s.contains(secret), "{secret} shows in {s}");
        }
    }
}

#[test]
fn no_team_lead_tool_adds_imports_or_signs_in_to_an_mcp_server() {
    for t in tools::catalog() {
        let name = t.name.to_lowercase();
        for word in ["mcp", "import", "sign_in", "signin", "login", "log_in", "oauth", "server", "keychain", "secret", "token"] {
            assert!(!name.contains(word), "{} looks like an MCP server tool ({word})", t.name);
        }
        let d = t.description.to_lowercase();
        let props: Vec<String> = t.input_schema["properties"].as_object().unwrap().keys().map(|k| k.to_lowercase()).collect();
        for p in &props {
            assert!(!p.contains("mcp") && !p.contains("server") && !p.contains("oauth") && !p.contains("secret") && p != "tools",
                    "{} takes {p}", t.name);
        }
        if d.contains("mcp") {
            // only to say it can't: update_agent's description
            assert!(t.name == "update_agent" && d.contains("set only by the user"), "{}: {}", t.name, t.description);
        }
    }
}

// ---- end to end: a chat answer that uses an outside tool ----

/// Gizai's MCP socket (test_state keeps it in the test's data folder), for the shim the fake starts.
fn with_socket(t: &T) -> tokio::task::JoinHandle<()> {
    mcp::start(&t.st).unwrap()
}

async fn turn(st: &AppState, thread: Option<String>, text: &str) -> (String, app_chat::TurnSummary) {
    let (id, done) = app_chat::send(st, thread, text.into(), None).await.unwrap();
    let summary = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
    (id, summary)
}

/// The tool messages of a thread: (tool name, isError, result text).
fn tool_messages(st: &AppState, thread: &str) -> Vec<(String, bool, String)> {
    chat::messages(&st.db, thread).unwrap().into_iter().filter(|m| m.role == "tool").map(|m| {
        let tool = m.tool.unwrap_or_default();
        (m.tool_name.unwrap_or_default(), tool["isError"].as_bool().unwrap_or(false), tool["result"].as_str().unwrap_or_default().to_string())
    }).collect()
}

fn state(t: &T) -> AppState {
    t.st.clone()
}

#[tokio::test]
async fn after_an_answer_used_an_outside_tool_set_agent_status_is_refused_and_the_next_message_can_use_it() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    let pause = r#"CALL set_agent_status {"agent": "Backend Agent", "status": "paused"}"#;
    let (thread, s) = turn(&st, None, &format!("OUTSIDE Look up the release notes in Otus, then {pause}")).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let msgs = tool_messages(&st, &thread);
    assert_eq!(msgs.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), ["mcp__otus__search", "mcp__gizai__set_agent_status"], "{msgs:?}");
    let (_, is_error, text) = &msgs[1];
    assert!(*is_error && refused_after_outside(text, "set_agent_status"), "the tool result is the refusal: {msgs:?}");
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "active", "nothing paused");
    // the user confirms in a new message: that answer uses no outside tool, and the same call works
    let (_, s) = turn(&st, Some(thread.clone()), &format!("Yes, pause it. {pause}")).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let msgs = tool_messages(&st, &thread);
    let last = msgs.last().unwrap();
    assert_eq!((last.0.as_str(), last.1), ("mcp__gizai__set_agent_status", false), "{msgs:?}");
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "paused");
    assert!(app_chat::used_outside(&st, &thread).is_none(), "the new message cleared the mark");
    // and a third message that uses an outside tool again is refused again
    let (_, s) = turn(&st, Some(thread.clone()), r#"OUTSIDE CALL add_column {"name": "Otus", "after": "Review", "kind": "work"}"#).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let last = tool_messages(&st, &thread).pop().unwrap();
    assert!(last.1 && refused_after_outside(&last.2, "add_column"), "{last:?}");
    assert!(!t.columns().contains(&"Otus".to_string()));
}

#[tokio::test]
async fn a_gizai_call_that_comes_in_just_before_the_stream_shows_the_outside_tool_is_refused_too() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    let (thread, s) = turn(&st, None, r#"OUTSIDE_LATE CALL set_agent_status {"agent": "Backend Agent", "status": "paused"}"#).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let msgs = tool_messages(&st, &thread);
    let gizai = msgs.iter().find(|m| m.0 == "mcp__gizai__set_agent_status").unwrap_or_else(|| panic!("{msgs:?}"));
    assert!(gizai.1 && refused_after_outside(&gizai.2, "set_agent_status"), "{msgs:?}");
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "active");
}

#[tokio::test]
async fn an_answer_with_only_gizais_own_tools_can_act() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    let (thread, s) = turn(&st, None, r#"CALL set_agent_status {"agent": "Backend Agent", "status": "paused"}"#).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert_eq!(tool_messages(&st, &thread).pop().map(|m| m.1), Some(false));
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "paused");
}

// ---- GA-77: an acting call in a chat answer waits until the answer's stream shows it, at most tools::SHOWN_WAIT ----

const PAUSE: &str = r#"CALL set_agent_status {"agent": "Backend Agent", "status": "paused"}"#;

/// The answer's last text. The fake says there how long Gizai took to answer its Gizai call, and, with NO_SHOW, what
/// Gizai answered (no tool message holds a call the stream never showed).
fn last_answer(st: &AppState, thread: &str) -> String {
    chat::messages(&st.db, thread).unwrap().into_iter().filter(|m| m.role == "agent").filter_map(|m| m.body_md).next_back().unwrap_or_default()
}

/// How long the fake's Gizai call took: "(the call took N ms)" in the answer's last text.
fn call_took(st: &AppState, thread: &str) -> Duration {
    let text = last_answer(st, thread);
    let ms = text.rsplit_once("(the call took ").and_then(|(_, r)| r.split_once(" ms)")).and_then(|(n, _)| n.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("no call time in {text:?}"));
    Duration::from_millis(ms)
}

fn refused_unseen(e: &str, name: &str) -> bool {
    e.contains(&format!("{name} is refused this time: Gizai couldn't see this call in your answer in time"))
        && e.contains("Nothing changed: call it again")
}

/// A stream that is far behind the calls: shown a second after the Gizai call (not 50 ms), the outside tool in the same
/// message still counts. Also for calls that name their tool use's id in `_meta`, as Claude Code's do.
#[tokio::test]
async fn a_gizai_call_the_stream_shows_a_second_late_next_to_an_outside_tool_is_refused_and_changes_nothing() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    for (words, call) in [
        ("OUTSIDE_LATE LATE_MS=1000", PAUSE),
        ("OUTSIDE_LATE LATE_MS=1000 META", PAUSE),
        ("OUTSIDE_LATE LATE_MS=1000", r#"CALL add_column {"name": "Otus", "after": "Review", "kind": "work"}"#),
        ("OUTSIDE_LATE LATE_MS=1000 META", r#"CALL create_agent {"name": "Otus Agent", "role": "backend"}"#),
    ] {
        let (thread, s) = turn(&st, None, &format!("{words} {call}")).await;
        assert_eq!(s.status, "succeeded", "{words} {call}: {s:?}");
        let name = call.split_whitespace().nth(1).unwrap();
        let msgs = tool_messages(&st, &thread);
        let gizai = msgs.iter().find(|m| m.0 == format!("mcp__gizai__{name}")).unwrap_or_else(|| panic!("{words} {call}: {msgs:?}"));
        assert!(gizai.1 && refused_after_outside(&gizai.2, name), "{words} {call}: {msgs:?}");
        let took = call_took(&st, &thread);
        assert!(took >= Duration::from_millis(900), "{words} {call}: answered after {took:?}, before the stream showed the call");
    }
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "active", "nothing paused");
    assert!(!t.columns().contains(&"Otus".to_string()), "no column added: {:?}", t.columns());
    assert!(!t.agent_names().contains(&"Otus Agent".to_string()), "no agent created: {:?}", t.agent_names());
}

/// A call whose tool use the stream doesn't show within SHOWN_WAIT (not at all, or only under another id than the one
/// the call names) is refused with a reason the model can act on, and changes nothing.
#[tokio::test]
async fn a_call_the_stream_doesnt_show_in_time_is_refused_with_a_reason_and_changes_nothing() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    for words in ["NO_SHOW", "NO_SHOW META"] {
        let (thread, s) = turn(&st, None, &format!("{words} {PAUSE}")).await;
        assert_eq!(s.status, "succeeded", "{words}: {s:?}");
        let text = last_answer(&st, &thread);
        assert!(text.starts_with("It was refused.") && refused_unseen(&text, "set_agent_status"), "{words}: {text}");
        let took = call_took(&st, &thread);
        assert!(took >= tools::SHOWN_WAIT - Duration::from_millis(50) && took < tools::SHOWN_WAIT + Duration::from_secs(3),
                "{words}: refused after {took:?}, not after SHOWN_WAIT ({:?})", tools::SHOWN_WAIT);
        assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "active", "{words}: nothing paused");
    }
    // the stream shows the same tool with the same arguments, but the call names another tool use: not this call's
    let (thread, s) = turn(&st, None, &format!("GIZAI_LATE META META_ID=toolu_other {PAUSE}")).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let msgs = tool_messages(&st, &thread);
    let gizai = msgs.iter().find(|m| m.0 == "mcp__gizai__set_agent_status").unwrap_or_else(|| panic!("{msgs:?}"));
    assert!(gizai.1 && refused_unseen(&gizai.2, "set_agent_status"), "{msgs:?}");
    assert!(call_took(&st, &thread) >= tools::SHOWN_WAIT - Duration::from_millis(50), "{:?}", call_took(&st, &thread));
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "active", "nothing paused");
}

/// The answer ends (its stream closes) while a call still waits to be shown: the call never goes through.
#[tokio::test]
async fn a_call_still_waiting_when_the_answer_ends_never_goes_through() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    let (_thread, s) = turn(&st, None, &format!("NO_SHOW EXIT_EARLY {PAUSE}")).await;
    assert_ne!(s.status, "running", "{s:?}");
    tokio::time::sleep(tools::SHOWN_WAIT + Duration::from_secs(1)).await;
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "active", "nothing paused");
}

/// Without an outside tool, a call acts as soon as the stream shows it: at once when it was shown first, a moment later
/// when the call came first (parallel calls), with or without its tool use's id. Never anywhere near SHOWN_WAIT.
#[tokio::test]
async fn a_call_the_stream_shows_without_an_outside_tool_acts_without_a_wait_anyone_notices() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    for words in ["", "META", "GIZAI_LATE", "GIZAI_LATE META"] {
        t.ok(None, "set_agent_status", json!({"agent": "Backend Agent", "status": "active"})).await;
        let (thread, s) = turn(&st, None, &format!("{words} {PAUSE}")).await;
        assert_eq!(s.status, "succeeded", "{words}: {s:?}");
        let msgs = tool_messages(&st, &thread);
        assert_eq!(msgs.last().map(|m| (m.0.as_str(), m.1)), Some(("mcp__gizai__set_agent_status", false)), "{words}: {msgs:?}");
        assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "paused", "{words}");
        let took = call_took(&st, &thread);
        assert!(took < Duration::from_millis(1500), "{words}: the call took {took:?}");
    }
}

/// Tools not in NOT_AFTER_OUTSIDE never wait for the stream, not even when it never shows them; nor do calls made
/// directly for a thread (not by an answer's Claude Code, so no stream shows them).
#[tokio::test]
async fn tools_not_in_the_list_and_direct_calls_dont_wait_for_the_stream() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    for call in [r#"CALL get_overview {}"#, r#"CALL create_task {"project": "KADE", "title": "Export"}"#] {
        let (thread, s) = turn(&st, None, &format!("NO_SHOW {call}")).await;
        assert_eq!(s.status, "succeeded", "{call}: {s:?}");
        let text = last_answer(&st, &thread);
        assert!(text.starts_with("Done."), "{call}: {text}");
        assert!(call_took(&st, &thread) < Duration::from_millis(1500), "{call}: {text}");
    }
    assert!(t.ok(None, "get_task", json!({"task": "KADE-1"})).await.to_string().contains("Export"), "the task was created");
    let thread = t.thread("Pause the Backend Agent and add a Design review column");
    let t0 = Instant::now();
    t.ok(Some(&thread), "set_agent_status", json!({"agent": "Backend Agent", "status": "paused"})).await;
    t.ok(Some(&thread), "add_column", json!({"name": "Design review", "after": "Review", "kind": "review"})).await;
    assert!(t0.elapsed() < Duration::from_millis(1500), "direct calls took {:?}", t0.elapsed());
    assert_eq!(team::agent(&st.db, &t.backend).unwrap().status, "paused");
    assert!(t.columns().contains(&"Design review".to_string()), "{:?}", t.columns());
}

// ---- quitting ends the MCP servers a chat answer started ----

#[cfg(target_os = "linux")]
fn ended(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(s) => s.rfind(')').and_then(|i| s[i + 1..].split_whitespace().next().map(|st| st == "Z" || st == "X")).unwrap_or(true),
    }
}

/// macOS, which has no /proc: the same from `ps`.
#[cfg(not(target_os = "linux"))]
fn ended(pid: u32) -> bool {
    let out = std::process::Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().chars().next()).is_none_or(|st| st == 'Z')
}

/// Kills (SIGKILL) whatever of the PIDs our fakes wrote still runs, checked by its command line, on drop.
struct Leftovers(Vec<u32>);

impl Drop for Leftovers {
    fn drop(&mut self) {
        for &pid in &self.0 {
            let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).map(|b| String::from_utf8_lossy(&b).replace('\0', " ")).unwrap_or_default();
            if pid > 1 && !ended(pid) && (cmd.contains("sleep") || cmd.contains("fake-mcp-server") || cmd.contains("fake-claude-mcp")) {
                // SAFETY: a plain kill of a process this test's fakes started (checked by its command line just above).
                unsafe { libc::kill(pid as i32, libc::SIGKILL); }
            }
        }
    }
}

async fn read_pids(dir: &Path) -> Leftovers {
    let t0 = Instant::now();
    let read = |n: &str| std::fs::read_to_string(dir.join(n)).ok().and_then(|s| s.trim().parse::<u32>().ok());
    loop {
        if let (Some(a), Some(b), Some(c)) = (read("claude.pid"), read("server.pid"), read("helper.pid")) {
            return Leftovers(vec![a, b, c]);
        }
        assert!(t0.elapsed() < Duration::from_secs(15), "the fake Claude Code and its MCP server didn't start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn quitting_ends_the_mcp_server_a_chat_answer_started_and_the_helper_it_started_in_its_own_group() {
    let t = setup();
    let st = state(&t);
    let _server = with_socket(&t);
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_MCP.to_string()).unwrap();
    let pids_dir = t._dir.path().join("pids");
    let (_thread, done) = app_chat::send(&st, None, format!("PIDS={} MODE=int", pids_dir.display()), None).await.unwrap();
    let pids = read_pids(&pids_dir).await;
    assert_eq!(app_chat::stop_all(&st, Duration::from_secs(12)).await, 1);
    let s = tokio::time::timeout(Duration::from_secs(15), done).await.expect("answer ended").unwrap();
    assert_eq!(s.status, "cancelled", "{s:?}");
    let t0 = Instant::now();
    while !pids.0.iter().all(|p| ended(*p)) && t0.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let alive: Vec<(&str, u32)> = ["claude", "server", "helper"].into_iter().zip(pids.0.iter().copied()).filter(|(_, p)| !ended(*p)).collect();
    assert!(alive.is_empty(), "still running after quitting: {alive:?}");
}
