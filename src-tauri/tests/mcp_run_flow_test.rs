// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-39: what a task run and the Team Lead's chat get of an agent's MCP servers, end to end with fake Claude Codes
// (crates/gizai-agents/tests/fake-claude-mcp-run.sh and fake-claude-mcp-chat.py keep their argv and a copy of the MCP
// config while the run lives), never the real one; secrets live in the in-memory keychain of `test_state`.
use std::path::{Path, PathBuf};
use std::time::Duration;

use gizai_agents::mcp_run::UNTRUSTED;
use gizai_agents::oauth::Saved;
use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::mcp_servers::{self as core_mcp, AgentServer, AgentTools, McpServer, ToolList};
use gizai_core::model::{AgentInput, ProjectInput};
use gizai_lib::AppState;
use gizai_lib::mcp_servers::{self as app_mcp, SecretLine, ServerInput};
use serde_json::{Value, json};

const FAKE_RUN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-mcp-run.sh");
const FAKE_CHAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-mcp-chat.py");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
/// An init line with gizai connected, otus failed, docs needs-auth and wiki connected.
const INIT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fixtures/run-mcp-init.jsonl");

const OTUS_SECRET: &str = "otus-env-SECRET-7f3a";
const DOCS_HEADER: &str = "docs-header-SECRET-91bc";
const DOCS_TOKEN: &str = "docs-token-SECRET-c0de";
const DOCS_REFRESH: &str = "docs-refresh-SECRET-d00d";
const JIRA_HEADER: &str = "jira-header-SECRET-5e5e";

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &AppState, name: &str, kind: &str, command: &str, env: &[String]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.to_vec(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent of `test_task` on the CLI `cli` and returns its id.
fn put_agent_on(st: &AppState, cli: &str) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..Default::default() }).unwrap();
    agent.actor_id
}

/// A card for the backend agent on a fake Claude Code that keeps what it got in `out`.
struct RunSetup {
    tmp: tempfile::TempDir,
    st: AppState,
    task: String,
    agent: String,
    out: PathBuf,
}

/// `init`: the fake's init line names MCP servers (fixtures/run-mcp-init.jsonl).
fn run_setup(init: bool) -> RunSetup {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    // never the real claude, whatever the agent runs on
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_RUN.to_string()).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let out = tmp.path().join("fake-out");
    let mut env = vec![format!("FAKE_MCP_OUT={}", out.display())];
    if init {
        env.push(format!("FAKE_MCP_INIT={INIT}"));
    }
    let cli = add_cli(&st, "Claude Code (mcp)", "claude_code", FAKE_RUN, &env);
    let agent = put_agent_on(&st, &cli);
    RunSetup { tmp, st, task, agent, out }
}

fn lines(v: &[(&str, Option<&str>)]) -> Vec<SecretLine> {
    v.iter().map(|(n, val)| SecretLine { name: n.to_string(), value: val.map(str::to_string) }).collect()
}

/// Adds a command server with its environment lines (a value None: none in the keychain) and returns its id.
fn add_command(st: &AppState, name: &str, command: &str, args: &[&str], env: &[(&str, Option<&str>)]) -> String {
    let server = McpServer { name: name.into(), transport: "stdio".into(), command: command.into(), args: args.iter().map(|a| a.to_string()).collect(), ..Default::default() };
    app_mcp::save(st, ServerInput { server, env: lines(env), headers: vec![] }).unwrap().server.id
}

/// Adds an address server with its header lines and returns its id.
fn add_address(st: &AppState, name: &str, url: &str, headers: &[(&str, Option<&str>)]) -> String {
    let server = McpServer { name: name.into(), transport: "http".into(), url: url.into(), ..Default::default() };
    app_mcp::save(st, ServerInput { server, env: vec![], headers: lines(headers) }).unwrap().server.id
}

/// Signs the server in: tokens in the keychain that last a day, so no refresh happens.
fn sign_in(st: &AppState, id: &str, url: &str, token: &str) {
    st.tokens.save(id, &Saved {
        access_token: token.into(), refresh_token: Some(DOCS_REFRESH.into()), expires_at: Some(gizai_core::ids::now_ms() + 24 * 3_600_000),
        client_id: "gizai-test".into(), token_endpoint: "https://auth.example.test/token".into(), revocation_endpoint: None,
        resource: url.into(), issuer: "https://auth.example.test".into(), scope: None,
    }).unwrap();
}

/// What List tools found: the tools by name.
fn listed(st: &AppState, id: &str, tools: &[&str]) {
    core_mcp::set_tool_list(&st.db, id, &ToolList {
        server_name: "fake".into(), server_version: "1.0.0".into(), listed_at: gizai_core::ids::now_ms(),
        tools: tools.iter().map(|t| json!({"name": t, "description": format!("{t} things"), "inputSchema": {"type": "object"}})).collect(),
        ..Default::default()
    }).unwrap();
}

/// The agent form's switches: (server id, on, its tools off).
fn switch(st: &AppState, agent: &str, servers: &[(&str, bool, &[&str])]) {
    let mcp = servers.iter().map(|(id, on, off)| AgentServer { server_id: id.to_string(), on: *on, tools_off: off.iter().map(|t| t.to_string()).collect() }).collect();
    app_mcp::save_agent(st, agent, AgentTools { mcp }).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The fake's arguments, one per line.
fn argv_of(out: &Path) -> Vec<String> {
    read(&out.join("argv")).lines().map(str::to_string).collect()
}

/// The values after `flag`, up to the next `--` option.
fn values(argv: &[String], flag: &str) -> Vec<String> {
    let Some(at) = argv.iter().position(|a| a == flag) else { return vec![] };
    argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect()
}

fn has(list: &[String], s: &str) -> bool {
    list.iter().any(|x| x == s)
}

/// The run's events as the Run panel shows them (live, else read back from its log).
fn events(st: &AppState, run_id: &str) -> Vec<RunEvent> {
    gizai_lib::runs::events_for(st, run_id).into_iter().map(|e| e.event).collect()
}

fn notes(evs: &[RunEvent]) -> Vec<String> {
    evs.iter().filter_map(|e| match e { RunEvent::Note { text } => Some(text.clone()), _ => None }).collect()
}

/// Gizai's own note lines at the top of the run's log.
fn log_notes(log: &str) -> Vec<String> {
    log.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v["type"] == "gizai_note").map(|v| v["text"].as_str().unwrap().to_string()).collect()
}

/// Every file under `dir` (sockets and the like left out) that holds `needle`.
fn files_holding(dir: &Path, needle: &str, skip: &[&str]) -> Vec<PathBuf> {
    let mut hits = vec![];
    let Ok(entries) = std::fs::read_dir(dir) else { return hits };
    for e in entries.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            hits.extend(files_holding(&p, needle, skip));
        } else if ft.is_file() && !skip.iter().any(|s| p.file_name().is_some_and(|n| n == *s)) {
            let bytes = std::fs::read(&p).unwrap_or_default();
            if bytes.windows(needle.len()).any(|w| w == needle.as_bytes()) {
                hits.push(p);
            }
        }
    }
    hits
}

fn config_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir).map(|r| r.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".mcp.json")).collect())
        .unwrap_or_default()
}

// ---- task runs ----

#[tokio::test]
async fn a_run_gets_the_servers_switched_on_in_its_own_0600_mcp_config_with_their_secrets_and_it_is_gone_after_the_run() {
    let t = run_setup(false);
    let st = &t.st;
    let otus = add_command(st, "otus", "npx", &["-y", "@otus/mcp"], &[("OTUS_TOKEN", Some(OTUS_SECRET))]);
    let docs = add_address(st, "docs", "https://docs.example.test/mcp", &[("X-Api-Key", Some(DOCS_HEADER))]);
    sign_in(st, &docs, "https://docs.example.test/mcp", DOCS_TOKEN);
    let notes_srv = add_command(st, "notes", "notes-mcp", &[], &[]);
    add_command(st, "jira", "jira-mcp", &[], &[]);
    switch(st, &t.agent, &[(&otus, true, &[]), (&docs, true, &[]), (&notes_srv, false, &[])]);

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let argv = argv_of(&t.out);
    assert!(has(&argv, "--strict-mcp-config"), "only the servers in its config: {argv:?}");
    let path = st.data_dir.join("runs").join(format!("{}.mcp.json", s.run_id));
    assert_eq!(values(&argv, "--mcp-config"), [path.display().to_string()], "{argv:?}");
    assert_eq!(read(&t.out.join("mcp.mode")).trim(), "600", "only its owner reads it while the run lives");
    assert!(!path.exists(), "the config is deleted when the run ends");
    assert!(config_files(&st.data_dir.join("runs")).is_empty(), "{:?}", config_files(&st.data_dir.join("runs")));

    let config: Value = serde_json::from_str(&read(&t.out.join("mcp.json"))).unwrap();
    let all = config["mcpServers"].as_object().unwrap();
    assert_eq!(all.keys().collect::<Vec<_>>(), ["docs", "otus"], "a server off or not switched on is absent: {config}");
    assert_eq!(all["otus"], json!({"type": "stdio", "command": "npx", "args": ["-y", "@otus/mcp"], "env": {"OTUS_TOKEN": OTUS_SECRET}}));
    assert_eq!(all["docs"], json!({"type": "http", "url": "https://docs.example.test/mcp",
                                    "headers": {"X-Api-Key": DOCS_HEADER, "Authorization": format!("Bearer {DOCS_TOKEN}")}}),
               "a signed-in server gets its access token in an Authorization header");

    // an agent with an outside server on gets the UNTRUSTED line, once, at the end of its prompt
    let prompt = read(&t.out.join("prompt"));
    assert_eq!(prompt.matches(UNTRUSTED).count(), 1, "{prompt}");
    assert!(prompt.trim_end().ends_with(UNTRUSTED), "{prompt}");
    assert!(notes(&events(st, &s.run_id)).iter().all(|n| !n.contains("Left out")), "nothing was left out");
}

#[tokio::test]
async fn the_runs_mcp_config_is_there_for_its_owner_only_while_the_run_lives_and_gone_after_a_stop() {
    let t = run_setup(false);
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[("OTUS_TOKEN", Some(OTUS_SECRET))]);
    switch(st, &t.agent, &[(&otus, true, &[])]);
    gizai_core::tasks::update(&st.db, &st.you_id, &t.task, gizai_core::model::TaskPatch { description_md: Some("FAKE_MCP_HANG".into()), ..Default::default() }).unwrap();
    let (run_id, done) = gizai_lib::runs::start(st, &t.task, None, None, "manual").await.unwrap();
    let path = st.data_dir.join("runs").join(format!("{run_id}.mcp.json"));
    let t0 = std::time::Instant::now();
    while !t.out.join("mcp.json").exists() {
        assert!(t0.elapsed() < Duration::from_secs(15), "the fake never started");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(&path).expect("the config is there while the run lives");
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    assert!(read(&path).contains(OTUS_SECRET));
    // the only file Gizai keeps that holds the secret is this config (the fake's own copy is outside Gizai's folder)
    assert_eq!(files_holding(&st.data_dir, OTUS_SECRET, &[]), [path.clone()]);
    gizai_lib::runs::stop(st, &run_id);
    let s = tokio::time::timeout(Duration::from_secs(20), done).await.expect("the run ended").unwrap();
    assert_eq!(s.status, "cancelled", "{:?}", s.error);
    assert!(!path.exists(), "a stopped run's config is deleted too");
}

#[tokio::test]
async fn a_servers_tools_all_on_allow_the_server_some_off_allow_the_rest_by_full_name_and_refuse_those_off_and_a_server_off_is_absent() {
    let t = run_setup(false);
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[]);
    listed(st, &otus, &["search", "delete_all"]);
    let docs = add_command(st, "docs", "docs-mcp", &[], &[]);
    listed(st, &docs, &["search", "read_page", "delete_page"]);
    let notes_srv = add_command(st, "notes", "notes-mcp", &[], &[]);
    listed(st, &notes_srv, &["write_note"]);
    switch(st, &t.agent, &[(&otus, true, &[]), (&docs, true, &["delete_page"]), (&notes_srv, false, &[])]);

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let argv = argv_of(&t.out);
    let allowed = values(&argv, "--allowedTools");
    let refused = values(&argv, "--disallowedTools");
    for want in ["mcp__otus", "mcp__docs__search", "mcp__docs__read_page", "Bash(git status:*)"] {
        assert!(has(&allowed, want), "{want} not allowed: {allowed:?}");
    }
    for not in ["mcp__docs", "mcp__docs__delete_page", "mcp__otus__delete_all"] {
        assert!(!has(&allowed, not), "{not} allowed: {allowed:?}");
    }
    assert_eq!(refused, ["mcp__docs__delete_page"], "{argv:?}");
    assert!(!argv.iter().any(|a| a.contains("notes")), "a server switched off is nowhere: {argv:?}");
    let config: Value = serde_json::from_str(&read(&t.out.join("mcp.json"))).unwrap();
    assert_eq!(config["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["docs", "otus"], "{config}");
}

#[tokio::test]
async fn a_server_with_a_secret_missing_from_the_keychain_or_signed_out_is_left_out_and_the_run_says_which_and_why() {
    let t = run_setup(false);
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[("OTUS_TOKEN", None)]);
    let docs = add_address(st, "docs", "https://docs.example.test/mcp", &[]);
    app_mcp::sign_out(st, &docs).unwrap();
    let wiki = add_command(st, "wiki", "wiki-mcp", &[], &[]);
    switch(st, &t.agent, &[(&otus, true, &[]), (&docs, true, &[]), (&wiki, true, &[])]);

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "the run still starts and finishes: {:?}", s.error);
    let config: Value = serde_json::from_str(&read(&t.out.join("mcp.json"))).unwrap();
    assert_eq!(config["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["wiki"], "{config}");
    let allowed = values(&argv_of(&t.out), "--allowedTools");
    assert!(has(&allowed, "mcp__wiki") && !allowed.iter().any(|a| a.starts_with("mcp__otus") || a.starts_with("mcp__docs")), "{allowed:?}");

    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let in_log = log_notes(&read(Path::new(&run.log_path)));
    let shown = notes(&events(st, &s.run_id));
    for list in [&in_log, &shown] {
        let otus_note = list.iter().find(|n| n.starts_with("Left out otus: ")).unwrap_or_else(|| panic!("no note for otus: {list:?}"));
        assert!(otus_note.contains("OTUS_TOKEN") && otus_note.contains("keychain") && otus_note.contains("Settings → MCP servers"), "{otus_note}");
        assert!(list.contains(&"Left out docs: signed out. Sign in again in Settings → MCP servers.".to_string()), "{list:?}");
        assert!(!list.iter().any(|n| n.contains("wiki")), "{list:?}");
    }
}

/// A list saved before the name rule can still hold a server called gizai__notes or otus_: its tools would be named
/// mcp__gizai__notes__x, which passes for Gizai's own. The run leaves it out and says once to rename it.
#[tokio::test]
async fn a_server_saved_under_a_name_the_list_no_longer_takes_is_left_out_and_the_run_says_to_rename_it() {
    let t = run_setup(false);
    let st = &t.st;
    let wiki = add_command(st, "wiki", "wiki-mcp", &[], &[]);
    let mut all = core_mcp::list(&st.db).unwrap();
    for (id, name) in [("old-notes", "gizai__notes"), ("old-otus", "otus_")] {
        all.push(McpServer { id: id.into(), name: name.into(), transport: "stdio".into(), command: "notes-mcp".into(), ..Default::default() });
    }
    // straight into the settings table, as an older Gizai could have saved it
    gizai_core::settings::set(&st.db, "mcp_servers", &all).unwrap();
    switch(st, &t.agent, &[(&wiki, true, &[]), ("old-notes", true, &[]), ("old-otus", true, &[])]);

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let config: Value = serde_json::from_str(&read(&t.out.join("mcp.json"))).unwrap();
    assert_eq!(config["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["wiki"], "{config}");
    let argv = argv_of(&t.out);
    let allowed = values(&argv, "--allowedTools");
    assert!(!allowed.iter().chain(values(&argv, "--disallowedTools").iter()).any(|a| a.contains("notes") || a.starts_with("mcp__otus")), "{allowed:?}");
    let shown = notes(&events(st, &s.run_id));
    for name in ["gizai__notes", "otus_"] {
        let mine: Vec<&String> = shown.iter().filter(|n| n.starts_with(&format!("Left out {name}: "))).collect();
        assert_eq!(mine.len(), 1, "one note for {name}: {shown:?}");
        assert!(mine[0].starts_with(&format!("Left out {name}: rename it in Settings → MCP servers")), "{}", mine[0]);
        assert_eq!(mine[0].to_lowercase().matches("another name").count() + mine[0].to_lowercase().matches("rename").count(), 1,
                   "says once to rename it: {}", mine[0]);
    }
}

#[tokio::test]
async fn a_run_whose_servers_are_all_left_out_goes_without_an_mcp_config_or_the_untrusted_line() {
    let t = run_setup(false);
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[("OTUS_TOKEN", None)]);
    switch(st, &t.agent, &[(&otus, true, &[])]);
    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let argv = argv_of(&t.out);
    assert!(has(&argv, "--strict-mcp-config") && !has(&argv, "--mcp-config"), "{argv:?}");
    assert!(!t.out.join("mcp.json").exists());
    assert!(!read(&t.out.join("prompt")).contains(UNTRUSTED));
    assert!(notes(&events(st, &s.run_id)).iter().any(|n| n.starts_with("Left out otus: ")), "{:?}", events(st, &s.run_id));
}

#[tokio::test]
async fn everything_is_off_by_default_a_fresh_agent_shows_every_server_off_and_its_run_gets_no_mcp_config() {
    let t = run_setup(false);
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[("OTUS_TOKEN", Some(OTUS_SECRET))]);
    listed(st, &otus, &["search"]);
    add_address(st, "docs", "https://docs.example.test/mcp", &[]);
    // a newly added agent has nothing on either
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let fresh = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Fresh Agent".into(), role_key: "frontend".into(), ..Default::default() }).unwrap();
    assert_eq!(core_mcp::agent_tools(&st.db, &fresh).unwrap(), AgentTools::default());
    for agent in [&t.agent, &fresh] {
        let v = app_mcp::agent_view(st, agent).unwrap();
        assert_eq!(v.disabled, None, "a Claude Code agent can have MCP servers");
        assert_eq!(v.servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["otus", "docs"]);
        assert!(v.servers.iter().all(|s| !s.on && s.tools_off.is_empty() && s.last_run.is_none()), "{:?}", v.servers);
        assert_eq!(v.warning, None, "no npm warning while nothing is on");
    }

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let argv = argv_of(&t.out);
    assert!(has(&argv, "--strict-mcp-config"), "not even the user's own MCP servers: {argv:?}");
    assert!(!has(&argv, "--mcp-config") && !t.out.join("mcp.json").exists(), "{argv:?}");
    assert!(!argv.iter().any(|a| a.starts_with("mcp__")), "{argv:?}");
    assert!(!read(&t.out.join("prompt")).contains(UNTRUSTED));
    assert!(config_files(&st.data_dir.join("runs")).is_empty());
}

#[tokio::test]
async fn a_server_the_init_line_shows_failed_is_named_in_the_run_and_the_agent_form_shows_each_servers_state_in_the_last_run() {
    let t = run_setup(true);
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[]);
    let docs = add_address(st, "docs", "https://docs.example.test/mcp", &[]);
    let wiki = add_command(st, "wiki", "wiki-mcp", &[], &[]);
    add_command(st, "jira", "jira-mcp", &[], &[]);
    switch(st, &t.agent, &[(&otus, true, &[]), (&docs, true, &[]), (&wiki, true, &[])]);

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    // read back from the log, as the Run panel shows a finished run
    let evs = events(st, &s.run_id);
    let said = notes(&evs);
    assert!(said.contains(&"otus: failed to connect, so this run goes without it. Settings → MCP servers → List tools shows why.".to_string()), "{said:?}");
    assert!(said.iter().any(|n| n.starts_with("docs: needs sign-in")), "{said:?}");
    assert!(!said.iter().any(|n| n.starts_with("wiki") || n.starts_with("gizai")), "{said:?}");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::McpServers { servers } if servers.len() == 3)), "{evs:?}");

    let v = app_mcp::agent_view(st, &t.agent).unwrap();
    let state = |name: &str| v.servers.iter().find(|s| s.name == name).unwrap().last_run.clone();
    assert_eq!(state("otus").map(|l| l.status), Some("failed".into()));
    assert_eq!(state("docs").map(|l| l.status), Some("needs-auth".into()));
    assert_eq!(state("wiki").map(|l| l.status), Some("connected".into()));
    assert_eq!(state("jira"), None, "not in the run");
    assert!(state("otus").unwrap().at > 0);
    let all = core_mcp::last_runs(&st.db).unwrap();
    assert!(!all[&t.agent].contains_key("gizai"), "Gizai's own server isn't kept: {all:?}");
}

#[tokio::test]
async fn an_agent_on_codex_or_gemini_shows_why_mcp_is_off_and_its_run_goes_without_its_servers_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_RUN.to_string()).unwrap();
    let otus = add_command(&st, "otus", "otus-mcp", &[], &[("OTUS_TOKEN", Some(OTUS_SECRET))]);
    for (name, kind) in [("Codex", "codex"), ("Gemini", "gemini")] {
        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        // switched on while it ran on Claude Code, then moved to another CLI
        let agent = put_agent_on(&st, "claude_code");
        switch(&st, &agent, &[(&otus, true, &[])]);
        let cli = add_cli(&st, name, kind, FAKE_CLI, &[format!("FAKE_KIND={kind}")]);
        put_agent_on(&st, &cli);
        let v = app_mcp::agent_view(&st, &agent).unwrap();
        // GA-55: each CLI says why it can't have MCP servers or the browser (the same words as the agent form's).
        assert_eq!(v.disabled.as_deref(), core_mcp::mcp_not_on(kind), "{name}");
        let why = v.disabled.clone().unwrap_or_default();
        assert!(why.contains(name) && why.contains("the browser") && !why.contains("GA-55"), "{name}: {why}");
        let err = app_mcp::save_agent(&st, &agent, AgentTools { mcp: vec![AgentServer { server_id: otus.clone(), on: true, tools_off: vec![] }] }).unwrap_err();
        assert!(err.contains("Claude Code") && err.contains(name), "{name}: {err}");

        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{name}: {:?}", s.error);
        let said = notes(&events(&st, &s.run_id));
        assert!(said.contains(&format!("Backend Agent runs on {name}: MCP servers work on Claude Code for now, so this run goes without them.")),
                "{name}: {said:?}");
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        let stderr = read(&Path::new(&run.log_path).with_extension("stderr.log"));
        let argv = stderr.lines().find_map(|l| l.strip_prefix("argv: ")).unwrap_or_else(|| panic!("{name}: no argv in {stderr}"));
        assert!(!argv.contains("mcp") && !stderr.contains(OTUS_SECRET), "{name}: {stderr}");
        assert!(config_files(&st.data_dir.join("runs")).is_empty(), "{name}");
        // back on Claude Code with nothing on, for the next CLI
        switch(&st, &agent, &[]);
    }
}

#[tokio::test]
async fn secrets_never_reach_the_run_log_its_notes_the_run_row_or_the_database() {
    let t = run_setup(true);
    let st = &t.st;
    let otus = add_command(st, "otus", "npx", &["-y", "@otus/mcp"], &[("OTUS_TOKEN", Some(OTUS_SECRET))]);
    let docs = add_address(st, "docs", "https://docs.example.test/mcp", &[("X-Api-Key", Some(DOCS_HEADER))]);
    sign_in(st, &docs, "https://docs.example.test/mcp", DOCS_TOKEN);
    // left out (signed out) though its header value is in the keychain: its note must not hold it either
    let jira = add_address(st, "jira", "https://jira.example.test/mcp", &[("X-Jira-Key", Some(JIRA_HEADER))]);
    app_mcp::sign_out(st, &jira).unwrap();
    switch(st, &t.agent, &[(&otus, true, &[]), (&docs, true, &[]), (&jira, true, &[])]);

    let s = gizai_lib::runs::run_once(st, &t.task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    // the fake did get them (else this test would prove nothing)
    let config = read(&t.out.join("mcp.json"));
    for secret in [OTUS_SECRET, DOCS_HEADER, DOCS_TOKEN] {
        assert!(config.contains(secret), "{secret} missing in the run's config: {config}");
    }
    assert!(!config.contains(JIRA_HEADER) && !config.contains(DOCS_REFRESH), "{config}");

    let evs = serde_json::to_string(&events(st, &s.run_id)).unwrap();
    assert!(evs.contains("Left out jira"), "{evs}");
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let row = format!("{run:?} {}", serde_json::to_string(&run).unwrap());
    let summary = format!("{s:?}");
    for secret in [OTUS_SECRET, DOCS_HEADER, DOCS_TOKEN, DOCS_REFRESH, JIRA_HEADER] {
        assert!(!evs.contains(secret), "{secret} in the run's events");
        assert!(!row.contains(secret), "{secret} in the run row");
        assert!(!summary.contains(secret), "{secret} in the run summary");
        // the log, its stderr, the database and its WAL, the worktrees: every file Gizai keeps
        let hits = files_holding(&st.data_dir, secret, &[]);
        assert!(hits.is_empty(), "{secret} in {hits:?}");
        let hits = files_holding(&t.tmp.path().join("repo"), secret, &[]);
        assert!(hits.is_empty(), "{secret} in {hits:?}");
    }
}

#[test]
fn mcp_configs_a_crashed_gizai_left_behind_are_removed_at_start_up() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    for dir in ["runs", "chat"] {
        std::fs::create_dir_all(data.join(dir)).unwrap();
        std::fs::write(data.join(dir).join("old-run.mcp.json"), format!("{{\"secret\":\"{OTUS_SECRET}\"}}")).unwrap();
        std::fs::write(data.join(dir).join("old-run.jsonl"), "{}\n").unwrap();
    }
    let st = gizai_lib::test_state(tmp.path());
    for dir in ["runs", "chat"] {
        assert!(!st.data_dir.join(dir).join("old-run.mcp.json").exists(), "{dir}: the config with its secrets is gone");
        assert!(st.data_dir.join(dir).join("old-run.jsonl").exists(), "{dir}: the log stays");
    }
}

// ---- the Team Lead's chat ----

struct ChatSetup {
    _tmp: tempfile::TempDir,
    st: AppState,
    lead: String,
}

fn chat_setup() -> ChatSetup {
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    // The fake starts no MCP server: the helper only has to be there.
    st.mcp_shim = Some(PathBuf::from(FAKE_CHAT));
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CHAT.to_string()).unwrap();
    gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    ChatSetup { _tmp: tmp, st, lead }
}

impl ChatSetup {
    async fn turn(&self, text: &str) -> (String, gizai_lib::chat::TurnSummary) {
        let (id, done) = gizai_lib::chat::send(&self.st, None, text.into(), None).await.unwrap();
        let summary = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        (id, summary)
    }

    /// What each turn's Claude Code got: {argv, prompt, path, mode, config}.
    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-mcp-calls.jsonl")).unwrap_or_default()
            .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    fn system_notes(&self, thread: &str) -> Vec<String> {
        gizai_core::chat::messages(&self.st.db, thread).unwrap().into_iter().filter(|m| m.role == "system").filter_map(|m| m.body_md).collect()
    }
}

fn argv_in(call: &Value) -> Vec<String> {
    serde_json::from_value(call["argv"].clone()).unwrap()
}

#[tokio::test]
async fn the_team_leads_chat_gets_its_servers_next_to_gizai_with_their_tools_allowed_or_refused_and_the_config_is_gone_after_the_turn() {
    let t = chat_setup();
    let st = &t.st;
    let otus = add_command(st, "otus", "npx", &["-y", "@otus/mcp"], &[("OTUS_TOKEN", Some(OTUS_SECRET))]);
    let docs = add_address(st, "docs", "https://docs.example.test/mcp", &[("X-Api-Key", Some(DOCS_HEADER))]);
    sign_in(st, &docs, "https://docs.example.test/mcp", DOCS_TOKEN);
    listed(st, &docs, &["search", "delete_page"]);
    let notes_srv = add_command(st, "notes", "notes-mcp", &[], &[]);
    switch(st, &t.lead, &[(&otus, true, &[]), (&docs, true, &["delete_page"]), (&notes_srv, false, &[])]);

    let (thread, s) = t.turn("What is on the board?").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = t.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let call = &calls[0];
    let path = PathBuf::from(call["path"].as_str().unwrap());
    assert_eq!(path, st.data_dir.join("chat").join(format!("{}.mcp.json", s.run_id)));
    assert_eq!(call["mode"], "600", "only its owner reads it while the turn lives");
    assert!(!path.exists(), "the config is deleted after the turn");

    let config = call["config"].as_object().unwrap();
    assert_eq!(config.keys().collect::<Vec<_>>(), ["docs", "gizai", "otus"], "{config:?}");
    assert_eq!(config["gizai"]["command"], FAKE_CHAT);
    assert!(config["gizai"]["env"]["GIZAI_TOKEN"].as_str().is_some_and(|t| !t.is_empty()), "{config:?}");
    assert_eq!(config["otus"], json!({"type": "stdio", "command": "npx", "args": ["-y", "@otus/mcp"], "env": {"OTUS_TOKEN": OTUS_SECRET}}));
    assert_eq!(config["docs"]["headers"], json!({"X-Api-Key": DOCS_HEADER, "Authorization": format!("Bearer {DOCS_TOKEN}")}));

    let argv = argv_in(call);
    assert!(has(&argv, "--strict-mcp-config"), "{argv:?}");
    assert_eq!(values(&argv, "--allowedTools"), ["mcp__gizai", "mcp__otus", "mcp__docs__search"]);
    assert_eq!(values(&argv, "--disallowedTools"), ["mcp__docs__delete_page"]);
    assert_eq!(values(&argv, "--tools"), ["Read,Glob,Grep"], "still only the tools that read");
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains(UNTRUSTED), "{sys}");
    assert!(t.system_notes(&thread).is_empty(), "nothing left out, all connected: {:?}", t.system_notes(&thread));
    // its secrets stay out of the chat, the run and every file Gizai keeps (the fake's own record left out)
    let msgs = format!("{:?}", gizai_core::chat::messages(&st.db, &thread).unwrap());
    for secret in [OTUS_SECRET, DOCS_HEADER, DOCS_TOKEN, DOCS_REFRESH] {
        assert!(!msgs.contains(secret), "{secret} in the chat");
        let hits = files_holding(&st.data_dir, secret, &["fake-mcp-calls.jsonl"]);
        assert!(hits.is_empty(), "{secret} in {hits:?}");
    }
}

#[tokio::test]
async fn a_chat_turn_names_a_server_that_did_not_connect_and_the_team_leads_form_shows_each_servers_state() {
    let t = chat_setup();
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[]);
    let docs = add_command(st, "docs", "docs-mcp", &[], &[]);
    switch(st, &t.lead, &[(&otus, true, &[]), (&docs, true, &[])]);
    let (thread, s) = t.turn("Look it up, mcp-status:docs=failed").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let said = t.system_notes(&thread);
    assert_eq!(said, ["docs: failed to connect, so this run goes without it. Settings → MCP servers → List tools shows why."], "{said:?}");
    let v = app_mcp::agent_view(st, &t.lead).unwrap();
    let state = |name: &str| v.servers.iter().find(|s| s.name == name).unwrap().last_run.clone().map(|l| l.status);
    assert_eq!(state("docs").as_deref(), Some("failed"));
    assert_eq!(state("otus").as_deref(), Some("connected"));
    assert!(!core_mcp::last_runs(&st.db).unwrap()[&t.lead].contains_key("gizai"));
}

#[tokio::test]
async fn a_chat_turn_leaves_out_a_server_whose_secret_is_missing_and_says_so_in_the_chat() {
    let t = chat_setup();
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[("OTUS_TOKEN", None)]);
    let wiki = add_command(st, "wiki", "wiki-mcp", &[], &[]);
    switch(st, &t.lead, &[(&otus, true, &[]), (&wiki, true, &[])]);
    let (thread, s) = t.turn("Anything new?").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let call = &t.calls()[0];
    assert_eq!(call["config"].as_object().unwrap().keys().collect::<Vec<_>>(), ["gizai", "wiki"]);
    assert_eq!(values(&argv_in(call), "--allowedTools"), ["mcp__gizai", "mcp__wiki"]);
    let said = t.system_notes(&thread);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].starts_with("Left out otus: ") && said[0].contains("OTUS_TOKEN") && said[0].contains("keychain"), "{said:?}");
}

#[tokio::test]
async fn a_team_lead_without_servers_on_gets_only_gizai_and_no_untrusted_line() {
    let t = chat_setup();
    let st = &t.st;
    let otus = add_command(st, "otus", "otus-mcp", &[], &[]);
    switch(st, &t.lead, &[(&otus, false, &[])]);
    let (_, s) = t.turn("Hello").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let call = &t.calls()[0];
    assert_eq!(call["config"].as_object().unwrap().keys().collect::<Vec<_>>(), ["gizai"]);
    let argv = argv_in(call);
    assert_eq!(values(&argv, "--allowedTools"), ["mcp__gizai"]);
    assert!(!has(&argv, "--disallowedTools"), "{argv:?}");
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(!sys.contains(UNTRUSTED), "{sys}");
    assert!(core_mcp::last_runs(&st.db).unwrap().get(&t.lead).is_none(), "no servers of its own: nothing kept");
}
