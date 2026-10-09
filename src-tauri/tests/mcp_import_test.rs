// Settings → MCP servers → Import from Claude Code (GA-39): scan() and import() over every Claude Code in Settings →
// Coding CLIs. HOME is the process's, so the tests here take turns (ENV): each points HOME at its own scratch folder and
// removes CLAUDE_CONFIG_DIR before anything reads them. Nothing here reads the real ~/.claude.json, ~/.claude or the OS
// keychain (test_state keeps the keychain in memory), starts `claude`, or opens a browser.
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use gizai_core::clis::{self, Cli};
use gizai_core::mcp_servers::McpServer;
use gizai_lib::AppState;
use gizai_lib::mcp_servers::{self as mcp, Candidate, Pick, Scan, SecretLine, ServerInput, ServerView};
use serde_json::{Value, json};

static ENV: Mutex<()> = Mutex::new(());

const SECOND: &str = "Claude Code (2nd account)";
const OTUS: &str = "/home/jefsev/code/otus";
const ALPHA: &str = "/home/jefsev/code/alpha";
/// Every secret value in the scratch configs.
const SECRETS: [&str; 8] = ["ghp_SECRET_builtin", "BSA_SECRET_brave", "sntrys_SECRET_sentry", "gz_SECRET_gizai",
                            "ghp_SECRET_second", "pw_SECRET_pg", "lin_SECRET_linear", "cd_SECRET_chrome"];

struct T {
    st: AppState,
    home: PathBuf,
    dir: tempfile::TempDir,
    _env: MutexGuard<'static, ()>,
}

fn setup() -> T {
    let env = ENV.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    // SAFETY: the tests in this binary take turns (ENV), so no other thread reads the environment meanwhile.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::remove_var("CLAUDE_CONFIG_DIR");
        std::env::remove_var("GIZAI_FAKE_KEYCHAIN");
    }
    assert!(home.starts_with(std::env::temp_dir()), "scratch HOME {} isn't in the temp folder", home.display());
    assert_eq!(std::env::var("HOME").unwrap(), home.display().to_string());
    let st = gizai_lib::test_state(&dir.path().join("gizai"));
    T { st, home, dir, _env: env }
}

impl T {
    /// The built-in Claude Code's config file (no CLAUDE_CONFIG_DIR): ~/.claude.json in the scratch HOME.
    fn builtin_file(&self) -> PathBuf {
        let f = gizai_agents::mcp_import::config_file(None, &self.home);
        assert!(f.starts_with(self.dir.path()), "{}", f.display());
        f
    }
    fn second_file(&self) -> PathBuf {
        self.home.join(".claude-2/.claude.json")
    }
    /// A Claude Code account in Settings → Coding CLIs, as the Coding CLIs form saves it.
    fn add_cli(&self, name: &str, kind: &str, env: &[&str]) -> String {
        let mut list: Vec<Cli> = clis::list(&self.st.db).unwrap().into_iter().filter(|c| c.id != clis::CLAUDE_CODE).collect();
        list.push(Cli { name: name.into(), kind: kind.into(), command: "claude".into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
        clis::save(&self.st.db, list).unwrap().into_iter().find(|c| c.name == name).unwrap().id
    }
    /// The built-in Claude Code and a second account (CLAUDE_CONFIG_DIR=~/.claude-2), each with servers in its file, and
    /// a Codex (not a Claude Code: left out). Returns the second account's id.
    fn two_accounts(&self) -> String {
        write_json(&self.builtin_file(), &builtin_json());
        write_json(&self.second_file(), &second_json());
        let id = self.add_cli(SECOND, "claude_code", &["CLAUDE_CONFIG_DIR=~/.claude-2"]);
        self.add_cli("Codex", "codex", &["CODEX_HOME=~/.codex-2"]);
        println!("scratch config files: {} and {}", self.builtin_file().display(), self.second_file().display());
        id
    }
    fn scan(&self) -> Scan {
        mcp::scan(&self.st).unwrap()
    }
    fn key(&self, account: &str, name: &str) -> String {
        candidate(&self.scan(), account, name).key.clone()
    }
    fn add_by_hand(&self, server: McpServer, env: &[(&str, &str)]) -> ServerView {
        mcp::save(&self.st, ServerInput { server, env: env.iter().map(|(n, v)| SecretLine { name: n.to_string(), value: Some(v.to_string()) }).collect(), headers: vec![] }).unwrap()
    }
    fn secret(&self, id: &str, kind: &str, name: &str) -> Option<String> {
        self.st.keychain.get(&format!("mcp/{id}/{kind}/{name}")).unwrap()
    }
    /// The bytes of every file in Gizai's data folder (the SQLite database and its WAL).
    fn data_bytes(&self) -> Vec<u8> {
        let mut out = vec![];
        for (p, (_, bytes, _)) in snapshot(&self.st.data_dir) {
            if let Some(b) = bytes {
                assert!(!p.starts_with(&self.home));
                out.extend(b);
            }
        }
        out
    }
}

fn builtin_json() -> Value {
    json!({
        "numStartups": 12, "userID": "u-1", "oauthAccount": { "emailAddress": "jefsev@example.com" },
        "mcpServers": {
            "github": { "type": "http", "url": "https://api.githubcopilot.com/mcp/", "headers": { "Authorization": "Bearer ghp_SECRET_builtin" } },
            "brave": { "type": "stdio", "command": "npx", "args": ["-y", "@brave/brave-search-mcp-server"], "env": { "BRAVE_API_KEY": "BSA_SECRET_brave" } },
            "gizai": { "command": "node", "args": ["/opt/gizai-mcp.js"], "env": { "GIZAI_TOKEN": "gz_SECRET_gizai" } }
        },
        "projects": {
            OTUS: { "history": [{ "display": "hi" }], "allowedTools": [],
                    "mcpServers": { "sentry": { "type": "http", "url": "https://mcp.sentry.dev/mcp", "headers": { "X-Api-Key": "sntrys_SECRET_sentry" } } } }
        }
    })
}

fn second_json() -> Value {
    json!({
        "numStartups": 3, "oauthAccount": { "emailAddress": "jefsev+2@example.com" },
        "mcpServers": {
            "github": { "type": "http", "url": "https://api.githubcopilot.com/mcp/", "headers": { "Authorization": "Bearer ghp_SECRET_second" } },
            "linear": { "type": "sse", "url": "https://mcp.linear.app/sse", "headers": { "Authorization": "Bearer lin_SECRET_linear" } },
            "postgres": { "type": "stdio", "command": "/usr/local/bin/pg-mcp", "args": ["--read-only"], "env": { "DATABASE_URL": "postgres://u:pw_SECRET_pg@db/app" } }
        },
        "projects": {
            ALPHA: { "mcpServers": { "chrome-devtools": { "type": "stdio", "command": "npx", "args": ["chrome-devtools-mcp@latest"], "env": { "CHROME_TOKEN": "cd_SECRET_chrome" } } } }
        }
    })
}

fn write_json(path: &Path, v: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}

fn candidate<'a>(scan: &'a Scan, account: &str, name: &str) -> &'a Candidate {
    scan.servers.iter().find(|c| c.account == account && c.name == name)
        .unwrap_or_else(|| panic!("no {name} from {account} in {:#?}", scan.servers))
}

fn pick(key: &str, name: &str) -> Pick {
    Pick { key: key.into(), name: name.into() }
}

/// Regular files under `dir` with bytes and mtime; FIFOs and other special files only listed, never opened.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (String, Option<Vec<u8>>, Option<SystemTime>)> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            let m = std::fs::symlink_metadata(&p).unwrap();
            if m.is_dir() {
                todo.push(p.clone());
                out.insert(p, ("dir".into(), None, None));
            } else if m.is_file() && m.permissions().mode() & 0o400 != 0 {
                out.insert(p.clone(), ("file".into(), Some(std::fs::read(&p).unwrap()), Some(m.modified().unwrap())));
            } else {
                out.insert(p, (format!("{:?}", m.file_type()), None, Some(m.modified().unwrap())));
            }
        }
    }
    out
}

fn set_mtime_in_past(path: &Path) -> SystemTime {
    let past = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options().write(true).open(path).unwrap().set_modified(past).unwrap();
    past
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

/// Runs `f` on another thread; panics when it doesn't return within `limit` (it opened a FIFO and waits for a writer).
fn within<R: Send + 'static>(what: &str, limit: Duration, f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || { let _ = tx.send(f()); });
    rx.recv_timeout(limit).unwrap_or_else(|_| panic!("{what} didn't return within {limit:?}: it opened .credentials.json (a FIFO here)"))
}

// ---- scan ----

#[test]
fn scan_lists_the_servers_of_every_claude_code_in_settings_with_account_scope_and_folder() {
    let t = setup();
    let second = t.two_accounts();
    let scan = t.scan();
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    let got: Vec<(&str, &str, &str, Option<&str>)> = scan.servers.iter()
        .map(|c| (c.account.as_str(), c.scope.as_str(), c.name.as_str(), c.folder.as_deref())).collect();
    assert_eq!(got, [
        ("Claude Code", "user", "brave", None), ("Claude Code", "user", "github", None), ("Claude Code", "user", "gizai", None),
        ("Claude Code", "local", "sentry", Some(OTUS)),
        (SECOND, "user", "github", None), (SECOND, "user", "linear", None), (SECOND, "user", "postgres", None),
        (SECOND, "local", "chrome-devtools", Some(ALPHA)),
    ], "the built-in Claude Code and the second account, the Codex left out");

    let brave = candidate(&scan, "Claude Code", "brave");
    assert_eq!(brave.key, "claude_code|user||brave");
    assert_eq!((brave.transport.as_str(), brave.command.as_str(), brave.url.as_str()), ("stdio", "npx", ""));
    assert_eq!(brave.args, ["-y", "@brave/brave-search-mcp-server"]);
    assert_eq!((brave.env_names.clone(), brave.header_names.len()), (vec!["BRAVE_API_KEY".to_string()], 0));
    assert!(!brave.already && brave.clash.is_none());

    let sentry = candidate(&scan, "Claude Code", "sentry");
    assert_eq!(sentry.key, format!("claude_code|local|{OTUS}|sentry"));
    assert_eq!((sentry.transport.as_str(), sentry.url.as_str(), sentry.command.as_str()), ("http", "https://mcp.sentry.dev/mcp", ""));
    assert_eq!(sentry.header_names, ["X-Api-Key"]);
    assert!(sentry.env_names.is_empty());

    let linear = candidate(&scan, SECOND, "linear");
    assert_eq!(linear.key, format!("{second}|user||linear"));
    assert_eq!((linear.transport.as_str(), linear.url.as_str()), ("sse", "https://mcp.linear.app/sse"));
    assert_eq!(linear.header_names, ["Authorization"]);

    let pg = candidate(&scan, SECOND, "postgres");
    assert_eq!((pg.command.as_str(), pg.args.clone()), ("/usr/local/bin/pg-mcp", vec!["--read-only".to_string()]));
    assert_eq!(pg.env_names, ["DATABASE_URL"]);

    let chrome = candidate(&scan, SECOND, "chrome-devtools");
    assert_eq!((chrome.scope.as_str(), chrome.folder.as_deref()), ("local", Some(ALPHA)));
    assert_eq!(chrome.key, format!("{second}|local|{ALPHA}|chrome-devtools"));
}

#[test]
fn scan_shows_the_names_of_env_and_header_lines_never_their_values() {
    let t = setup();
    t.two_accounts();
    let scan = t.scan();
    let text = serde_json::to_string(&scan).unwrap();
    for name in ["BRAVE_API_KEY", "X-Api-Key", "Authorization", "DATABASE_URL", "GIZAI_TOKEN", "CHROME_TOKEN"] {
        assert!(text.contains(name), "{name} missing: {text}");
    }
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["servers"][0]["envNames"], json!(["BRAVE_API_KEY"]), "the UI's field names: {text}");
    for shown in [text, format!("{scan:?}")] {
        for secret in SECRETS {
            assert!(!shown.contains(secret), "the scan shows the secret {secret}: {shown}");
        }
        assert!(!shown.contains("SECRET"), "{shown}");
    }
}

#[test]
fn the_built_in_claude_code_reads_the_file_in_gizais_own_claude_config_dir() {
    let t = setup();
    let own = t.home.join(".claude-own");
    // Gizai itself runs with CLAUDE_CONFIG_DIR: the built-in Claude Code (which runs with Gizai's environment) uses that.
    unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &own) };
    write_json(&t.builtin_file(), &builtin_json());
    write_json(&own.join(".claude.json"), &json!({ "mcpServers": { "time": { "command": "uvx", "args": ["mcp-server-time"] } } }));
    let scan = t.scan();
    unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
    let got: Vec<(&str, &str)> = scan.servers.iter().map(|c| (c.account.as_str(), c.name.as_str())).collect();
    assert_eq!(got, [("Claude Code", "time")], "not the servers in ~/.claude.json");
    // without it, ~/.claude.json
    assert_eq!(t.scan().servers.len(), 4);
}

#[test]
fn a_second_account_whose_dir_is_written_with_home_is_found_too() {
    let t = setup();
    write_json(&t.home.join(".claude-3/.claude.json"), &json!({ "mcpServers": { "time": { "command": "uvx" } } }));
    t.add_cli("Claude Code (3rd)", "claude_code", &["CLAUDE_CONFIG_DIR=$HOME/.claude-3"]);
    let scan = t.scan();
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    assert_eq!(scan.servers.iter().map(|c| (c.account.as_str(), c.name.as_str())).collect::<Vec<_>>(), [("Claude Code (3rd)", "time")]);
}

#[test]
fn a_config_file_that_cant_be_read_is_a_problem_and_the_other_accounts_still_come() {
    let t = setup();
    write_json(&t.builtin_file(), &builtin_json());
    std::fs::create_dir_all(t.home.join(".claude-2")).unwrap();
    std::fs::write(t.second_file(), r#"{"mcpServers": {"x": {"command": "npx", "env": {"K": "pw_SECRET_pg"}}"#).unwrap();
    t.add_cli(SECOND, "claude_code", &["CLAUDE_CONFIG_DIR=~/.claude-2"]);
    // an account that never added a server: no file yet, no problem
    t.add_cli("Claude Code (new)", "claude_code", &["CLAUDE_CONFIG_DIR=~/.claude-new"]);
    let scan = t.scan();
    assert_eq!(scan.problems, [format!("{SECOND}: Couldn't read {}: it isn't valid JSON", t.second_file().display())]);
    assert_eq!(scan.servers.len(), 4);
    assert!(scan.servers.iter().all(|c| c.account == "Claude Code"));
}

#[test]
fn a_server_already_in_the_list_is_marked_already() {
    let t = setup();
    t.two_accounts();
    // the same command and arguments under another name, and the same address
    t.add_by_hand(McpServer { name: "brave-search".into(), transport: "stdio".into(), command: "npx".into(),
                              args: vec!["-y".into(), "@brave/brave-search-mcp-server".into()], env_names: vec![], ..Default::default() }, &[("BRAVE_API_KEY", "other")]);
    t.add_by_hand(McpServer { name: "sentry-io".into(), transport: "http".into(), url: "https://mcp.sentry.dev/mcp".into(), ..Default::default() }, &[]);
    // the same command with other arguments is another server
    t.add_by_hand(McpServer { name: "pg-rw".into(), transport: "stdio".into(), command: "/usr/local/bin/pg-mcp".into(), ..Default::default() }, &[]);
    let scan = t.scan();
    let already: Vec<(&str, &str)> = scan.servers.iter().filter(|c| c.already).map(|c| (c.account.as_str(), c.name.as_str())).collect();
    assert_eq!(already, [("Claude Code", "brave"), ("Claude Code", "sentry")]);
    let clashing: Vec<&str> = scan.servers.iter().filter(|c| c.clash.is_some()).map(|c| c.name.as_str()).collect();
    assert_eq!(clashing, ["gizai", "chrome-devtools"], "the same server under another name: no clash");
}

#[test]
fn a_name_that_is_taken_or_gizais_own_has_a_clash() {
    let t = setup();
    t.two_accounts();
    t.add_by_hand(McpServer { name: "GitHub".into(), transport: "http".into(), url: "https://github.example.com/mcp".into(), ..Default::default() }, &[]);
    let scan = t.scan();
    let clash = |account: &str, name: &str| candidate(&scan, account, name).clash.clone();
    let taken = clash("Claude Code", "github").expect("github is taken (as GitHub)");
    assert!(taken.contains("already an MCP server called github"), "{taken}");
    assert!(clash(SECOND, "github").is_some());
    assert!(!candidate(&scan, "Claude Code", "github").already, "another address: not already");
    assert_eq!(clash("Claude Code", "gizai").as_deref(), Some("gizai is Gizai's own: give the server another name"));
    assert_eq!(clash(SECOND, "chrome-devtools").as_deref(), Some("chrome-devtools is Gizai's own: give the server another name"));
    for (account, name) in [("Claude Code", "brave"), ("Claude Code", "sentry"), (SECOND, "linear"), (SECOND, "postgres")] {
        assert_eq!(clash(account, name), None, "{name}");
    }
}

// ---- import ----

#[test]
fn import_makes_normal_entries_with_their_values_in_the_keychain_not_in_the_database() {
    let t = setup();
    t.two_accounts();
    let by_hand = t.add_by_hand(McpServer { name: "github".into(), transport: "http".into(), url: "https://github.example.com/mcp".into(), ..Default::default() }, &[]);
    let added = mcp::import(&t.st, vec![
        pick(&t.key("Claude Code", "brave"), "brave"),
        pick(&t.key("Claude Code", "sentry"), ""),
        pick(&t.key(SECOND, "github"), "github-work"),
        pick(&t.key(SECOND, "postgres"), " postgres "),
        pick(&t.key("Claude Code", "gizai"), "gizai-node"),
    ]).unwrap();
    let names: Vec<&str> = added.iter().map(|v| v.server.name.as_str()).collect();
    assert_eq!(names.len(), 5);
    let view = |name: &str| added.iter().find(|v| v.server.name == name).unwrap_or_else(|| panic!("{name} not imported: {names:?}"));

    let brave = view("brave");
    assert_eq!((brave.server.transport.as_str(), brave.server.command.as_str()), ("stdio", "npx"));
    assert_eq!(brave.server.args, ["-y", "@brave/brave-search-mcp-server"]);
    assert_eq!(brave.server.env_names, ["BRAVE_API_KEY"]);
    assert_eq!(brave.server.source, "Claude Code, user scope");
    assert!(brave.missing.is_empty() && brave.problem.is_none(), "{brave:?}");
    assert_eq!(t.secret(&brave.server.id, "env", "BRAVE_API_KEY").as_deref(), Some("BSA_SECRET_brave"));

    let sentry = view("sentry");
    assert_eq!((sentry.server.transport.as_str(), sentry.server.url.as_str()), ("http", "https://mcp.sentry.dev/mcp"));
    assert_eq!(sentry.server.header_names, ["X-Api-Key"]);
    assert_eq!(sentry.server.source, format!("Claude Code, local scope: {OTUS}"));
    assert_eq!(t.secret(&sentry.server.id, "header", "X-Api-Key").as_deref(), Some("sntrys_SECRET_sentry"));

    let gh = view("github-work");
    assert_eq!(gh.server.url, "https://api.githubcopilot.com/mcp/");
    assert_eq!(gh.server.source, format!("{SECOND}, user scope"));
    assert_eq!(t.secret(&gh.server.id, "header", "Authorization").as_deref(), Some("Bearer ghp_SECRET_second"), "the second account's token");

    let pg = view("postgres");
    assert_eq!(t.secret(&pg.server.id, "env", "DATABASE_URL").as_deref(), Some("postgres://u:pw_SECRET_pg@db/app"));
    assert_eq!(t.secret(&view("gizai-node").server.id, "env", "GIZAI_TOKEN").as_deref(), Some("gz_SECRET_gizai"));

    // normal entries: in Settings → MCP servers next to the one added by hand, which is unchanged
    let all = mcp::list(&t.st).unwrap();
    let mut listed: Vec<&str> = all.iter().map(|v| v.server.name.as_str()).collect();
    listed.sort();
    assert_eq!(listed, ["brave", "github", "github-work", "gizai-node", "postgres", "sentry"]);
    assert_eq!(mcp::get(&t.st, &by_hand.server.id).unwrap().server, by_hand.server);
    assert!(all.iter().all(|v| v.missing.is_empty()), "{all:?}");
    let shown = serde_json::to_string(&all).unwrap();
    assert!(!shown.contains("SECRET"), "{shown}");

    // the database has the names, never the values
    let db = t.data_bytes();
    assert!(contains(&db, "BRAVE_API_KEY") && contains(&db, "X-Api-Key") && contains(&db, "github-work"), "the check below would find text in the database");
    for secret in SECRETS {
        assert!(!contains(&db, secret), "{secret} is in Gizai's database");
    }

    // a new scan marks them already and their names taken
    let scan = t.scan();
    for (account, name) in [("Claude Code", "brave"), ("Claude Code", "sentry"), (SECOND, "github"), (SECOND, "postgres"), ("Claude Code", "gizai")] {
        assert!(candidate(&scan, account, name).already, "{account} {name}");
    }
    assert!(candidate(&scan, "Claude Code", "github").already, "the same address as github-work");
    assert!(candidate(&scan, "Claude Code", "brave").clash.is_some());
    assert!(!candidate(&scan, SECOND, "linear").already);
}

#[test]
fn importing_a_clashing_name_without_a_new_name_is_refused() {
    let t = setup();
    t.two_accounts();
    t.add_by_hand(McpServer { name: "github".into(), transport: "http".into(), url: "https://github.example.com/mcp".into(), ..Default::default() }, &[]);
    let refused = |key: String, name: &str, why: &str| {
        let e = mcp::import(&t.st, vec![pick(&key, name)]).unwrap_err();
        assert!(e.contains(why), "{name}: {e}");
        assert!(!e.contains("SECRET"), "{e}");
    };
    refused(t.key("Claude Code", "github"), "", "there is already an MCP server called github");
    refused(t.key("Claude Code", "github"), "github", "there is already an MCP server called github");
    refused(t.key("Claude Code", "github"), "GITHUB", "there is already an MCP server called GITHUB");
    refused(t.key("Claude Code", "gizai"), "", "gizai is Gizai's own");
    refused(t.key(SECOND, "chrome-devtools"), "", "chrome-devtools is Gizai's own");
    refused(t.key(SECOND, "chrome-devtools"), "Chrome-DevTools", "Gizai's own");
    refused(t.key("Claude Code", "brave"), "brave search", "letters, digits, - and _");
    let names: Vec<String> = mcp::list(&t.st).unwrap().into_iter().map(|v| v.server.name).collect();
    assert_eq!(names, ["github"], "nothing imported");
    // under a new name it works
    let added = mcp::import(&t.st, vec![pick(&t.key("Claude Code", "gizai"), "gizai-node"), pick(&t.key("Claude Code", "github"), "github-2")]).unwrap();
    assert_eq!(added.iter().map(|v| v.server.name.as_str()).collect::<Vec<_>>(), ["gizai-node", "github-2"]);
}

#[test]
fn the_same_name_in_two_accounts_is_imported_once_and_the_other_needs_another_name() {
    let t = setup();
    t.two_accounts();
    let (first, other) = (t.key("Claude Code", "github"), t.key(SECOND, "github"));
    let e = mcp::import(&t.st, vec![pick(&first, ""), pick(&other, "")]).unwrap_err();
    assert!(e.contains("there is already an MCP server called github"), "{e}");
    let other_name = mcp::import(&t.st, vec![pick(&other, "github-2")]).unwrap();
    let mut names: Vec<String> = mcp::list(&t.st).unwrap().into_iter().map(|v| v.server.name).collect();
    names.sort();
    assert_eq!(names, ["github", "github-2"]);
    assert_eq!(t.secret(&other_name[0].server.id, "header", "Authorization").as_deref(), Some("Bearer ghp_SECRET_second"));
}

#[test]
fn a_pick_that_isnt_in_claude_codes_config_any_more_is_refused() {
    let t = setup();
    t.two_accounts();
    let key = t.key(SECOND, "linear");
    let mut v = second_json();
    v["mcpServers"].as_object_mut().unwrap().remove("linear");
    write_json(&t.second_file(), &v);
    let e = mcp::import(&t.st, vec![pick(&key, "")]).unwrap_err();
    assert!(e.contains("isn't in Claude Code's config any more"), "{e}");
    let e = mcp::import(&t.st, vec![pick("made-up|user||linear", "")]).unwrap_err();
    assert!(e.contains("isn't in Claude Code's config any more"), "{e}");
    assert!(mcp::list(&t.st).unwrap().is_empty());
}

#[test]
fn an_import_is_a_copy_and_later_changes_in_claude_code_dont_follow() {
    let t = setup();
    t.two_accounts();
    let added = mcp::import(&t.st, vec![pick(&t.key("Claude Code", "brave"), ""), pick(&t.key("Claude Code", "sentry"), "")]).unwrap();
    let before: Vec<McpServer> = added.iter().map(|v| v.server.clone()).collect();
    let (brave, sentry) = (&before[0], &before[1]);
    // Claude Code changes them, then removes one
    let mut v = builtin_json();
    v["mcpServers"]["brave"] = json!({ "type": "stdio", "command": "bunx", "args": ["brave-v2"], "env": { "BRAVE_API_KEY": "NEW_brave", "EXTRA": "NEW_extra" } });
    v["projects"][OTUS]["mcpServers"]["sentry"] = json!({ "type": "sse", "url": "https://other.sentry.dev/sse", "headers": { "X-Api-Key": "NEW_sentry" } });
    write_json(&t.builtin_file(), &v);
    assert_eq!(mcp::get(&t.st, &brave.id).unwrap().server, *brave);
    assert_eq!(mcp::get(&t.st, &sentry.id).unwrap().server, *sentry);
    assert_eq!(t.secret(&brave.id, "env", "BRAVE_API_KEY").as_deref(), Some("BSA_SECRET_brave"));
    assert_eq!(t.secret(&brave.id, "env", "EXTRA"), None);
    assert_eq!(t.secret(&sentry.id, "header", "X-Api-Key").as_deref(), Some("sntrys_SECRET_sentry"));
    // the changed ones are other servers now
    let scan = t.scan();
    assert!(!candidate(&scan, "Claude Code", "brave").already && !candidate(&scan, "Claude Code", "sentry").already);
    v["mcpServers"].as_object_mut().unwrap().remove("brave");
    write_json(&t.builtin_file(), &v);
    assert_eq!(mcp::get(&t.st, &brave.id).unwrap().server, *brave, "removed in Claude Code: still here");
}

// ---- read only: nothing written, no credentials, no server ----

#[test]
fn scan_and_import_leave_claude_codes_files_byte_for_byte_as_they_were() {
    let t = setup();
    t.two_accounts();
    std::fs::create_dir_all(t.home.join(".claude")).unwrap();
    std::fs::write(t.home.join(".claude/settings.json"), "{}").unwrap();
    std::fs::write(t.home.join(".claude-2/settings.json"), r#"{"model":"opus"}"#).unwrap();
    let pasts = [set_mtime_in_past(&t.builtin_file()), set_mtime_in_past(&t.second_file())];
    let before = snapshot(&t.home);
    t.scan();
    mcp::import(&t.st, vec![pick(&t.key("Claude Code", "brave"), ""), pick(&t.key(SECOND, "linear"), ""),
                            pick(&t.key(SECOND, "chrome-devtools"), "chrome-2")]).unwrap();
    let _ = mcp::import(&t.st, vec![pick(&t.key("Claude Code", "gizai"), "")]).unwrap_err();
    t.scan();
    let after = snapshot(&t.home);
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>(), "files added or removed in Claude Code's folders");
    assert!(before == after, "a file in Claude Code's folders changed");
    assert_eq!(std::fs::metadata(t.builtin_file()).unwrap().modified().unwrap(), pasts[0]);
    assert_eq!(std::fs::metadata(t.second_file()).unwrap().modified().unwrap(), pasts[1]);
}

#[test]
fn scan_and_import_never_open_credentials_json() {
    let t = setup();
    t.two_accounts();
    // Claude Code keeps sign-ins (its own and MCP servers' OAuth tokens) in .credentials.json in its config folder:
    // ~/.claude for the built-in one. As FIFOs, opening either for reading would hang.
    std::fs::create_dir_all(t.home.join(".claude")).unwrap();
    for f in [t.home.join(".claude/.credentials.json"), t.home.join(".claude-2/.credentials.json")] {
        assert!(std::process::Command::new("mkfifo").arg(&f).status().unwrap().success());
    }
    let st = t.st.clone();
    let scan = within("scan", Duration::from_secs(10), move || mcp::scan(&st).unwrap());
    assert_eq!(scan.servers.len(), 8);
    let st = t.st.clone();
    let keys = vec![pick(&candidate(&scan, "Claude Code", "sentry").key, ""), pick(&candidate(&scan, SECOND, "linear").key, "")];
    let added = within("import", Duration::from_secs(10), move || mcp::import(&st, keys).unwrap());
    assert_eq!(added.len(), 2);
}

#[test]
fn scan_and_import_start_no_server_and_call_no_address() {
    let t = setup();
    let marker = t.dir.path().join("started");
    let script = t.dir.path().join("server.sh");
    std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    write_json(&t.builtin_file(), &json!({
        "mcpServers": {
            "local": { "type": "stdio", "command": script.display().to_string(), "env": { "K": "v" } },
            "sh": { "command": "/bin/sh", "args": ["-c", format!("touch '{}'", marker.display())] },
            "remote": { "type": "http", "url": url, "headers": { "Authorization": "Bearer x" } }
        },
        "projects": { OTUS: { "mcpServers": { "stream": { "type": "sse", "url": url } } } }
    }));
    let scan = t.scan();
    assert_eq!(scan.servers.len(), 4);
    let picks = scan.servers.iter().map(|c| pick(&c.key, "")).collect();
    assert_eq!(mcp::import(&t.st, picks).unwrap().len(), 4);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "a server's command ran");
    match listener.accept() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Ok((_, from)) => panic!("scan or import called the server's address (a connection from {from})"),
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn the_import_panel_says_what_isnt_in_claude_codes_files() {
    let ui = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/components/McpSettings.tsx")).unwrap();
    for must in ["Connectors from your claude.ai account and servers from plugins aren't in these files",
                 "Gizai doesn't change Claude Code's files, doesn't read its sign-ins and starts no server",
                 "Already in the list", "An import is a copy"] {
        assert!(ui.contains(must), "McpSettings.tsx lacks: {must}");
    }
}

#[test]
fn a_name_gizai_cant_use_has_a_clash_and_imports_under_another_name() {
    let t = setup();
    write_json(&t.builtin_file(), &json!({ "mcpServers": { "my.server": { "command": "uvx", "args": ["mcp-server-time"] } } }));
    let scan = t.scan();
    let c = candidate(&scan, "Claude Code", "my.server");
    assert!(c.clash.as_deref().is_some_and(|w| w.contains("letters, digits, - and _")), "{:?}", c.clash);
    let e = mcp::import(&t.st, vec![pick(&c.key, "")]).unwrap_err();
    assert!(e.contains("letters, digits, - and _"), "{e}");
    let added = mcp::import(&t.st, vec![pick(&c.key, "my-server")]).unwrap();
    assert_eq!((added[0].server.name.as_str(), added[0].server.command.as_str()), ("my-server", "uvx"));
}
