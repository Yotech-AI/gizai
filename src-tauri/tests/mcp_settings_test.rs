//! Settings → MCP servers end to end without the OS keychain: add, edit and remove a command server and an address
//! server, their values in a keychain in memory (never in SQLite, the views or the Team Lead's tools), the name rules,
//! and List tools against fake servers (crates/gizai-agents/tests/fake-mcp-server.py and a local HTTP server).
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gizai_agents::secrets::{Keychain, MemoryKeychain};
use gizai_core::mcp_servers::{self as core_mcp, AgentServer, AgentTools, McpServer};
use gizai_core::model::AgentInput;
use gizai_lib::mcp_servers::{self as mcp, SecretLine, ServerInput, ServerView};
use gizai_lib::{AppState, tools};
use serde_json::json;

#[path = "../../crates/gizai-agents/tests/support/fake_mcp_http.rs"]
mod fake_http;

const SERVER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-mcp-server.py");
const ENV_SECRET: &str = "sk-live-SETTINGS-91d3e7a2b4";
const REGION_SECRET: &str = "region-SECRET-eu-west-55";
const HEADER_SECRET: &str = "hdr-SECRET-x-api-0c6f1e";
const TOKEN_SECRET: &str = "tok-SECRET-bearer-8a2d4c";
const ALL_SECRETS: [&str; 4] = [ENV_SECRET, REGION_SECRET, HEADER_SECRET, TOKEN_SECRET];

struct T {
    st: AppState,
    /// The keychain the state uses, to see its keys.
    kc: Arc<MemoryKeychain>,
    _dir: tempfile::TempDir,
}

fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(dir.path());
    let kc = Arc::new(MemoryKeychain::default());
    st.keychain = kc.clone();
    st.tokens = Arc::new(gizai_agents::oauth::TokenStore::new(kc.clone()));
    T { st, kc, _dir: dir }
}

fn line(name: &str, value: Option<&str>) -> SecretLine {
    SecretLine { name: name.into(), value: value.map(str::to_string) }
}

/// The fake stdio server in `mode`, needing NOTES_KEY; NOTES_KEY and REGION have values.
fn command_server(name: &str, mode: &str) -> ServerInput {
    ServerInput {
        server: McpServer {
            name: name.into(), transport: "stdio".into(), command: "python3".into(),
            args: vec![SERVER.into(), mode.into(), "--env".into(), "NOTES_KEY".into()], ..Default::default()
        },
        env: vec![line("NOTES_KEY", Some(ENV_SECRET)), line("REGION", Some(REGION_SECRET))],
        headers: vec![],
    }
}

fn address_server(name: &str, url: &str) -> ServerInput {
    ServerInput {
        server: McpServer { name: name.into(), transport: "http".into(), url: url.into(), ..Default::default() },
        env: vec![],
        headers: vec![line("X-Api-Key", Some(HEADER_SECRET)), line("Authorization", Some(&format!("Bearer {TOKEN_SECRET}")))],
    }
}

/// The same server again for an edit: its id, and the lines given.
fn edit(v: &ServerView, env: Vec<SecretLine>, headers: Vec<SecretLine>) -> ServerInput {
    ServerInput { server: v.server.clone(), env, headers }
}

fn env_value(t: &T, id: &str, name: &str) -> Option<String> {
    t.kc.get(&format!("mcp/{id}/env/{name}")).unwrap()
}

fn header_value(t: &T, id: &str, name: &str) -> Option<String> {
    t.kc.get(&format!("mcp/{id}/header/{name}")).unwrap()
}

/// The keychain's keys (MemoryKeychain's Debug shows only keys).
fn keychain_keys(t: &T) -> String {
    format!("{:?}", t.kc)
}

/// Where `needle` shows up in Gizai's data: any file in the data folder (the database, its WAL, anything else) as
/// bytes, and any column of any table, as text.
fn found_in_data(t: &T, needle: &str) -> Vec<String> {
    let mut out = vec![];
    fn walk(dir: &Path, needle: &[u8], out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, needle, out);
            } else if let Ok(bytes) = std::fs::read(&p)
                && bytes.windows(needle.len()).any(|w| w == needle)
            {
                out.push(format!("file {}", p.display()));
            }
        }
    }
    walk(&t.st.data_dir, needle.as_bytes(), &mut out);
    let rows: Vec<String> = t.st.db.read(|c| {
        let mut hits = vec![];
        let tables: Vec<String> = c.prepare("SELECT name FROM sqlite_master WHERE type='table'")?
            .query_map([], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>()?;
        for table in tables {
            let cols: Vec<String> = c.prepare("SELECT name FROM pragma_table_info(?1)")?
                .query_map([&table], |r| r.get::<_, String>(0))?.collect::<Result<_, _>>()?;
            for col in cols {
                let n: i64 = c.query_row(&format!("SELECT count(*) FROM \"{table}\" WHERE instr(CAST(\"{col}\" AS TEXT), ?1) > 0"), [needle], |r| r.get(0))?;
                if n > 0 {
                    hits.push(format!("{table}.{col} ({n} rows)"));
                }
            }
        }
        Ok(hits)
    }).unwrap();
    out.extend(rows);
    out
}

fn assert_no_secret_in_data(t: &T, secrets: &[&str]) {
    for s in secrets {
        let found = found_in_data(t, s);
        assert!(found.is_empty(), "the secret {s} is in Gizai's data: {found:?}");
    }
}

fn assert_no_secret_in_json(what: &str, json: &str, secrets: &[&str]) {
    for s in secrets {
        assert!(!json.contains(s), "{what} shows the secret {s}: {json}");
    }
}

fn wait_pid(path: &Path) -> u32 {
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

#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(s) => s.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next()).is_some_and(|st| st != "Z" && st != "X"),
        Err(_) => false,
    }
}

/// macOS, which has no /proc: the same from `ps`.
#[cfg(not(target_os = "linux"))]
fn alive(pid: u32) -> bool {
    let out = std::process::Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).output();
    out.ok().and_then(|o| String::from_utf8_lossy(&o.stdout).trim().chars().next()).is_some_and(|st| st != 'Z')
}

fn add_agent(t: &T, name: &str, role: &str) -> String {
    let team_id = gizai_core::team::list(&t.st.db).unwrap()[0].id.clone();
    gizai_core::team::add_agent(&t.st.db, &t.st.you_id, &team_id, AgentInput {
        name: name.into(), role_key: role.into(), chat_enabled: Some(role == "lead"), ..Default::default() }).unwrap()
}

// ---- add, edit, remove (AC1, AC2) ----

#[test]
fn adding_a_command_and_an_address_server_keeps_their_values_in_the_keychain_not_in_sqlite() {
    let t = setup();
    let cmd = mcp::save(&t.st, command_server("notes", "hints")).expect("command server");
    let web = mcp::save(&t.st, address_server("docs", "http://127.0.0.1:9/mcp")).expect("address server");

    assert!(!cmd.server.id.is_empty() && cmd.server.id != web.server.id);
    assert_eq!(cmd.server.transport, "stdio");
    assert_eq!(cmd.server.command, "python3");
    assert_eq!(cmd.server.args, [SERVER, "hints", "--env", "NOTES_KEY"]);
    assert_eq!(cmd.server.env_names, ["NOTES_KEY", "REGION"]);
    assert!(cmd.server.header_names.is_empty());
    assert!(cmd.missing.is_empty(), "{:?}", cmd.missing);
    assert_eq!(web.server.transport, "http");
    assert_eq!(web.server.url, "http://127.0.0.1:9/mcp");
    assert_eq!(web.server.header_names, ["X-Api-Key", "Authorization"]);
    assert!(web.server.env_names.is_empty());
    assert!(web.missing.is_empty(), "{:?}", web.missing);

    // The values are in the keychain, under the server's id.
    assert_eq!(env_value(&t, &cmd.server.id, "NOTES_KEY").as_deref(), Some(ENV_SECRET));
    assert_eq!(env_value(&t, &cmd.server.id, "REGION").as_deref(), Some(REGION_SECRET));
    assert_eq!(header_value(&t, &web.server.id, "X-Api-Key").as_deref(), Some(HEADER_SECRET));
    assert_eq!(header_value(&t, &web.server.id, "Authorization"), Some(format!("Bearer {TOKEN_SECRET}")));

    // SQLite (and every file in the data folder) has the names, never the values.
    assert!(!found_in_data(&t, "NOTES_KEY").is_empty(), "the scan should find the names (it found nothing at all)");
    assert!(!found_in_data(&t, "X-Api-Key").is_empty());
    assert_no_secret_in_data(&t, &ALL_SECRETS);

    // What the UI gets: names only.
    let list = mcp::list(&t.st).unwrap();
    assert_eq!(list.len(), 2);
    for (what, v) in [("save (command)", serde_json::to_string(&cmd)), ("save (address)", serde_json::to_string(&web)),
                      ("list", serde_json::to_string(&list)), ("get", serde_json::to_string(&mcp::get(&t.st, &cmd.server.id).unwrap()))] {
        let v = v.unwrap();
        assert!(v.contains("NOTES_KEY") || v.contains("X-Api-Key"), "{what}: {v}");
        assert_no_secret_in_json(what, &v, &ALL_SECRETS);
    }
}

#[test]
fn editing_keeps_an_empty_value_replaces_a_typed_one_and_deletes_a_removed_line() {
    let t = setup();
    let v = mcp::save(&t.st, command_server("notes", "hints")).unwrap();
    let id = v.server.id.clone();

    // Left empty (None, or "" from an empty field): the stored values stay.
    let v = mcp::save(&t.st, edit(&v, vec![line("NOTES_KEY", None), line("REGION", Some(""))], vec![])).unwrap();
    assert_eq!(v.server.id, id);
    assert_eq!(env_value(&t, &id, "NOTES_KEY").as_deref(), Some(ENV_SECRET));
    assert_eq!(env_value(&t, &id, "REGION").as_deref(), Some(REGION_SECRET));
    assert!(v.missing.is_empty());

    // Typed in: replaced. Taken out: its keychain entry is gone. A new line without a value is missing.
    let new_value = "sk-live-REPLACED-0a1b2c3d4e";
    let v = mcp::save(&t.st, edit(&v, vec![line("NOTES_KEY", Some(new_value)), line("NEW_LINE", None)], vec![])).unwrap();
    assert_eq!(v.server.env_names, ["NOTES_KEY", "NEW_LINE"]);
    assert_eq!(env_value(&t, &id, "NOTES_KEY").as_deref(), Some(new_value));
    assert_eq!(env_value(&t, &id, "REGION"), None);
    assert_eq!(v.missing, ["NEW_LINE"]);

    // A rename keeps the values (they are kept by id).
    let mut renamed = edit(&v, vec![line("NOTES_KEY", None)], vec![]);
    renamed.server.name = "notes-2".into();
    let v = mcp::save(&t.st, renamed).unwrap();
    assert_eq!((v.server.id.as_str(), v.server.name.as_str()), (id.as_str(), "notes-2"));
    assert_eq!(env_value(&t, &id, "NOTES_KEY").as_deref(), Some(new_value));
    assert_eq!(env_value(&t, &id, "NEW_LINE"), None);

    // The same for an address server's header lines.
    let w = mcp::save(&t.st, address_server("docs", "http://127.0.0.1:9/mcp")).unwrap();
    let wid = w.server.id.clone();
    let w = mcp::save(&t.st, edit(&w, vec![], vec![line("X-Api-Key", Some(""))])).unwrap();
    assert_eq!(w.server.header_names, ["X-Api-Key"]);
    assert_eq!(header_value(&t, &wid, "X-Api-Key").as_deref(), Some(HEADER_SECRET));
    assert_eq!(header_value(&t, &wid, "Authorization"), None);

    assert_no_secret_in_data(&t, &[ENV_SECRET, REGION_SECRET, HEADER_SECRET, TOKEN_SECRET, new_value]);
    let json = serde_json::to_string(&mcp::list(&t.st).unwrap()).unwrap();
    assert_no_secret_in_json("list", &json, &[ENV_SECRET, REGION_SECRET, HEADER_SECRET, TOKEN_SECRET, new_value]);
}

#[test]
fn names_take_letters_digits_dash_and_underscore_and_gizais_own_and_duplicates_are_refused() {
    let t = setup();
    let first = mcp::save(&t.st, command_server("Notes", "hints")).unwrap();
    let keys_before = keychain_keys(&t);

    for bad in ["my server", "notes.v2", "ünicode", "", "  ", "a/b", "semi;colon", &"x".repeat(65)] {
        let e = mcp::save(&t.st, command_server(bad, "hints")).expect_err(bad);
        assert!(e.contains("letters, digits, - and _"), "{bad:?}: {e}");
    }
    for own in ["gizai", "GIZAI", "chrome-devtools", "Chrome-DevTools"] {
        let e = mcp::save(&t.st, command_server(own, "hints")).expect_err(own);
        assert!(e.contains("is Gizai's own"), "{own}: {e}");
    }
    for dup in ["Notes", "notes", "NOTES"] {
        let e = mcp::save(&t.st, command_server(dup, "hints")).expect_err(dup);
        assert!(e.contains("there is already an MCP server called"), "{dup}: {e}");
    }
    // A refused save keeps nothing: not in the list, not in the keychain.
    assert_eq!(mcp::list(&t.st).unwrap().len(), 1);
    assert_eq!(keychain_keys(&t), keys_before);

    // Its own name is fine when it is saved again; letters, digits, - and _ (up to 64) are fine.
    mcp::save(&t.st, edit(&first, vec![line("NOTES_KEY", None)], vec![])).expect("saving it under its own name");
    for good in ["my-server_2", "A1", &"y".repeat(64)] {
        mcp::save(&t.st, command_server(good, "hints")).unwrap_or_else(|e| panic!("{good}: {e}"));
    }
    assert_eq!(mcp::list(&t.st).unwrap().len(), 4);
    assert!(core_mcp::valid_name("a-b_C9") && !core_mcp::valid_name("a b"));
    assert_eq!(core_mcp::TAKEN, ["gizai", "chrome-devtools"]);
}

#[test]
fn removing_a_server_deletes_all_its_secrets_and_its_cached_tools() {
    let t = setup();
    let cmd = mcp::save(&t.st, command_server("notes", "hints")).unwrap();
    let web = mcp::save(&t.st, address_server("docs", "http://127.0.0.1:9/mcp")).unwrap();
    let listed = mcp::list_tools(&t.st, &cmd.server.id).unwrap();
    assert!(listed.listed.is_some(), "{:?}", listed.problem);

    mcp::remove(&t.st, &cmd.server.id).unwrap();
    let keys = keychain_keys(&t);
    assert!(!keys.contains(&cmd.server.id), "the removed server's secrets are still in the keychain: {keys}");
    assert!(keys.contains(&web.server.id), "{keys}");
    assert!(!core_mcp::tool_lists(&t.st.db).unwrap().contains_key(&cmd.server.id));

    mcp::remove(&t.st, &web.server.id).unwrap();
    assert_eq!(keychain_keys(&t), "MemoryKeychain { keys: [] }");
    assert!(mcp::list(&t.st).unwrap().is_empty());
    assert!(mcp::remove(&t.st, &web.server.id).is_err(), "removing it twice");
}

// ---- List tools (AC1, AC2, AC7) ----

#[test]
fn list_tools_shows_a_servers_tools_before_any_agent_has_it_on_and_stops_it() {
    let t = setup();
    let agent = add_agent(&t, "Dev Agent", "dev");
    let pids = tempfile::tempdir().unwrap();
    let pidfile = pids.path().join("server.pid");
    let mut input = command_server("notes", "hints");
    input.env.push(line("FAKE_MCP_PIDFILE", Some(pidfile.to_str().unwrap())));
    let v = mcp::save(&t.st, input).unwrap();
    assert!(v.listed.is_none());
    assert!(v.used_by.is_empty());

    let v = mcp::list_tools(&t.st, &v.server.id).expect("list tools");
    assert_eq!(v.problem, None);
    assert!(v.used_by.is_empty(), "no agent has it on: {:?}", v.used_by);
    let l = v.listed.as_ref().expect("its tools");
    assert_eq!((l.server_name.as_str(), l.server_version.as_str()), ("fake-mcp", "1.0.0"));
    assert!(l.listed_at > 0);
    let shown: Vec<(&str, &str, &str)> = l.tools.iter().map(|t| (t.name.as_str(), t.description.as_str(), t.risk.as_str())).collect();
    assert_eq!(shown, [("read_notes", "Reads the notes that match a query.", "low"), ("send_mail", "Sends an email.", "high"),
                       ("tag_note", "Adds a tag to a note.", "medium")]);
    assert_eq!(l.tools[0].hints_sent, ["readOnlyHint"]);
    assert_no_secret_in_json("list tools", &serde_json::to_string(&v).unwrap(), &ALL_SECRETS);

    // The server it started is ended.
    let pid = wait_pid(&pidfile);
    assert!(!alive(pid), "the server (pid {pid}) still runs after List tools");

    // The agent form shows the tools with the server still off.
    let form = mcp::agent_view(&t.st, &agent).unwrap();
    let s = form.servers.iter().find(|s| s.server_id == v.server.id).unwrap();
    assert!(!s.on);
    assert_eq!(s.tools.len(), 3);
    assert_eq!(s.risk, "high");
    assert_eq!(s.summary, "3 tools: 1 only read, 1 change things, 1 may delete or overwrite. High risk.");

    // Kept: showing it again starts nothing.
    std::fs::remove_file(&pidfile).unwrap();
    let again = mcp::get(&t.st, &v.server.id).unwrap();
    assert_eq!(again.listed.as_ref().map(|l| l.tools.len()), Some(3));
    std::thread::sleep(Duration::from_millis(300));
    assert!(!pidfile.exists(), "showing the server started it again");
}

#[test]
fn list_tools_keeps_what_it_found_with_the_servers_version_until_the_command_or_address_changes() {
    let t = setup();
    let mut input = command_server("notes", "hints");
    input.env.push(line("FAKE_MCP_VERSION", Some("7.1.0")));
    let v = mcp::save(&t.st, input).unwrap();
    let v = mcp::list_tools(&t.st, &v.server.id).unwrap();
    assert_eq!(v.listed.as_ref().unwrap().server_version, "7.1.0");
    let listed_at = v.listed.as_ref().unwrap().listed_at;

    // Another value, the same command: what it found still holds.
    let v = mcp::save(&t.st, edit(&v, vec![line("NOTES_KEY", Some("sk-other-value-123456")), line("REGION", None), line("FAKE_MCP_VERSION", None)], vec![])).unwrap();
    assert_eq!(v.listed.as_ref().map(|l| l.listed_at), Some(listed_at));

    // Another version: List tools again keeps the new one.
    let v = mcp::save(&t.st, edit(&v, vec![line("NOTES_KEY", None), line("REGION", None), line("FAKE_MCP_VERSION", Some("7.2.0"))], vec![])).unwrap();
    let v = mcp::list_tools(&t.st, &v.server.id).unwrap();
    assert_eq!(v.listed.as_ref().unwrap().server_version, "7.2.0");

    // Other arguments: another server, nothing listed.
    let mut changed = edit(&v, vec![line("NOTES_KEY", None), line("REGION", None), line("FAKE_MCP_VERSION", None)], vec![]);
    changed.server.args = vec![SERVER.into(), "nohints".into(), "--env".into(), "NOTES_KEY".into()];
    let v = mcp::save(&t.st, changed).unwrap();
    assert!(v.listed.is_none(), "other arguments should clear the tool list");
    let v = mcp::list_tools(&t.st, &v.server.id).unwrap();
    assert_eq!(v.listed.as_ref().unwrap().tools.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["delete_everything", "lookup"]);

    // Another command clears it too.
    let mut changed = edit(&v, vec![line("NOTES_KEY", None)], vec![]);
    changed.server.command = "/usr/bin/python3".into();
    let v = mcp::save(&t.st, changed).unwrap();
    assert!(v.listed.is_none(), "another command should clear the tool list");

    // An address server: its header lines arrive, and another address clears what it found.
    let http = fake_http::start(fake_http::Mode::Ok);
    let w = mcp::save(&t.st, address_server("docs", &http.url())).unwrap();
    let w = mcp::list_tools(&t.st, &w.server.id).unwrap();
    assert_eq!(w.problem, None);
    assert_eq!(w.listed.as_ref().unwrap().tools.len(), 2);
    let seen = http.seen();
    assert!(!seen.is_empty());
    for s in &seen {
        assert_eq!(s.header("x-api-key"), Some(HEADER_SECRET));
        assert_eq!(s.header("authorization"), Some(format!("Bearer {TOKEN_SECRET}").as_str()));
    }
    let mut moved = edit(&w, vec![], vec![line("X-Api-Key", None), line("Authorization", None)]);
    moved.server.url = format!("{}/v2", http.url());
    let w = mcp::save(&t.st, moved).unwrap();
    assert!(w.listed.is_none(), "another address should clear the tool list");
    assert_eq!(header_value(&t, &w.server.id, "X-Api-Key").as_deref(), Some(HEADER_SECRET));
}

#[test]
fn a_failing_server_shows_its_error_in_plain_words_with_its_secrets_as_dots() {
    let t = setup();
    // A command that prints its key and exits.
    let v = mcp::save(&t.st, command_server("notes", "fail")).unwrap();
    let v = mcp::list_tools(&t.st, &v.server.id).expect("the view, with the problem in it");
    let problem = v.problem.clone().expect("a problem");
    assert!(problem.starts_with("The server ended before it answered (exit code 3)"), "{problem}");
    assert!(problem.contains("NOTES_KEY=•••"), "{problem}");
    assert!(v.listed.is_none());
    assert_no_secret_in_json("the view", &serde_json::to_string(&v).unwrap(), &ALL_SECRETS);

    // An address whose error page echoes the header values.
    let http = fake_http::start(fake_http::Mode::EchoError);
    let w = mcp::save(&t.st, address_server("docs", &http.url())).unwrap();
    let w = mcp::list_tools(&t.st, &w.server.id).unwrap();
    let problem = w.problem.clone().expect("a problem");
    assert!(problem.starts_with("The server answered 500 Internal Server Error: Invalid API key: •••"), "{problem}");
    assert_no_secret_in_json("the view", &serde_json::to_string(&w).unwrap(), &ALL_SECRETS);

    // The problems are kept in SQLite: without the values.
    assert_no_secret_in_data(&t, &ALL_SECRETS);
    assert_no_secret_in_json("list", &serde_json::to_string(&mcp::list(&t.st).unwrap()).unwrap(), &ALL_SECRETS);
}

#[test]
fn list_tools_says_which_value_is_missing_from_the_keychain() {
    let t = setup();
    let mut input = command_server("notes", "hints");
    input.env = vec![line("NOTES_KEY", None)];
    let v = mcp::save(&t.st, input).unwrap();
    assert_eq!(v.missing, ["NOTES_KEY"]);
    let e = mcp::list_tools(&t.st, &v.server.id).unwrap_err();
    assert_eq!(e, "notes's value for NOTES_KEY isn't in the keychain: enter it again in Settings → MCP servers");
    assert_eq!(mcp::get(&t.st, &v.server.id).unwrap().problem.as_deref(), Some(e.as_str()));
}

// ---- the Team Lead's tools (AC2) ----

#[tokio::test]
async fn the_team_leads_tools_show_an_agents_servers_by_name_never_their_values() {
    let t = setup();
    let lead = add_agent(&t, "Team Lead", "lead");
    let dev = add_agent(&t, "Dev Agent", "dev");
    let cmd = mcp::save(&t.st, command_server("notes", "hints")).unwrap();
    let web = mcp::save(&t.st, address_server("docs", "http://127.0.0.1:9/mcp")).unwrap();
    mcp::list_tools(&t.st, &cmd.server.id).unwrap();
    mcp::save_agent(&t.st, &dev, AgentTools { mcp: vec![
        AgentServer { server_id: cmd.server.id.clone(), on: true, tools_off: vec!["send_mail".into()] },
        AgentServer { server_id: web.server.id.clone(), on: true, tools_off: vec![] },
    ] }).unwrap();

    let got = tools::call(&t.st, &lead, "get_agent", json!({"agent": "Dev Agent"})).await.unwrap();
    let servers = &got["agent"]["mcp_servers"];
    assert_eq!(servers.as_array().map(Vec::len), Some(2), "{got}");
    assert_eq!(servers[0]["name"], "notes");
    assert_eq!(servers[0]["on"], true);
    assert_eq!(servers[0]["tools_off"], json!(["send_mail"]));
    assert_no_secret_in_json("get_agent", &got.to_string(), &ALL_SECRETS);
    for name in ["list_agents", "get_overview"] {
        let out = tools::call(&t.st, &lead, name, json!({})).await.unwrap();
        assert_no_secret_in_json(name, &out.to_string(), &ALL_SECRETS);
    }
    // The agent form's view: names only too.
    assert_no_secret_in_json("agent form", &serde_json::to_string(&mcp::agent_view(&t.st, &dev).unwrap()).unwrap(), &ALL_SECRETS);
    assert_no_secret_in_data(&t, &ALL_SECRETS);
}
