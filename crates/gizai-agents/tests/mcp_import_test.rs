// Import from Claude Code (GA-39): which file holds a Claude Code's MCP servers, and what `read` makes of it. Everything
// happens in scratch config folders in a tempdir: never the real ~/.claude.json or ~/.claude.
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gizai_agents::mcp_import::{self, Found};
use serde_json::{Value, json};

/// Secret values in the scratch configs: none of them may show up in what Gizai shows or prints.
const SECRETS: [&str; 6] = ["ghp_SECRET_user_github", "BSA_SECRET_brave", "zz_SECRET_zed", "sntrys_SECRET_sentry", "pw_SECRET_pg", "ey_SECRET_cookie"];

/// What `claude mcp add` (Claude Code 2.1.289) leaves in its config file, among the many other keys it keeps there.
fn claude_json() -> Value {
    json!({
        "numStartups": 42,
        "installMethod": "native",
        "userID": "0f3c-user-id",
        "oauthAccount": { "accountUuid": "acc-1", "emailAddress": "jefsev@example.com", "organizationName": "Yotech" },
        "tipsHistory": { "new-user-warmup": 1 },
        "mcpServers": {
            "github": { "type": "http", "url": "https://api.githubcopilot.com/mcp/", "headers": { "Authorization": "Bearer ghp_SECRET_user_github" } },
            "linear": { "type": "sse", "url": "https://mcp.linear.app/sse" },
            // an older entry: no "type", a command
            "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"] },
            "brave": { "type": "stdio", "command": "npx", "args": ["-y", "@brave/brave-search-mcp-server"],
                       "env": { "ZED": "zz_SECRET_zed", "BRAVE_API_KEY": "BSA_SECRET_brave" } },
            "cookies": { "type": "streamable-http", "url": "https://mcp.example.com/mcp", "headers": { "Cookie": "ey_SECRET_cookie" } }
        },
        "projects": {
            "/home/jefsev/code/otus": {
                "allowedTools": [], "history": [{ "display": "fix the build", "pastedContents": {} }], "hasTrustDialogAccepted": true,
                "mcpServers": { "sentry": { "type": "http", "url": "https://mcp.sentry.dev/mcp", "headers": { "X-Api-Key": "sntrys_SECRET_sentry" } } }
            },
            "/home/jefsev/code/alpha": {
                "mcpServers": { "postgres": { "type": "stdio", "command": "/usr/local/bin/pg-mcp", "args": [],
                                              "env": { "DATABASE_URL": "postgres://u:pw_SECRET_pg@db/app" } } }
            },
            "/home/jefsev/code/empty": { "history": [], "mcpServers": {} }
        }
    })
}

fn write_json(path: &Path, v: &Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}

fn by_name<'a>(found: &'a [Found], name: &str) -> &'a Found {
    found.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} not found in {found:?}"))
}

fn pairs(kv: &[(&str, &str)]) -> Vec<(String, String)> {
    kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// The regular files in `dir` with their bytes and mtime (FIFOs and other special files are only listed, never opened).
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

fn mkfifo(path: &Path) {
    let ok = std::process::Command::new("mkfifo").arg(path).status().unwrap().success();
    assert!(ok, "mkfifo {}", path.display());
}

/// `read` on another thread: a reader that opens a FIFO blocks until a writer comes, so a hang means it opened one.
fn read_within(path: &Path, limit: Duration) -> Result<Vec<Found>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let p = path.to_path_buf();
    std::thread::spawn(move || { let _ = tx.send(mcp_import::read(&p)); });
    rx.recv_timeout(limit).unwrap_or_else(|_| panic!("read({}) didn't return within {limit:?}: it opened something that blocks (a FIFO)", path.display()))
}

// ---- which file ----

#[test]
fn the_config_file_is_claude_json_in_the_accounts_config_folder_else_in_home() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let second = home.join(".claude-2");
    std::fs::create_dir_all(&second).unwrap();
    // A second account (CLAUDE_CONFIG_DIR): its own folder, whether the file is there yet or not.
    assert_eq!(mcp_import::config_file(Some(&second), &home), second.join(".claude.json"));
    // The built-in one without CLAUDE_CONFIG_DIR: ~/.claude.json, not ~/.claude/.claude.json.
    assert_eq!(mcp_import::config_file(None, &home), home.join(".claude.json"));
    // An empty CLAUDE_CONFIG_DIR counts as none.
    assert_eq!(mcp_import::config_file(Some(Path::new("")), &home), home.join(".claude.json"));
}

#[test]
fn an_old_style_config_json_in_the_config_folder_is_the_one_when_it_is_there() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let second = home.join(".claude-2");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    std::fs::write(home.join(".claude.json"), "{}").unwrap();
    std::fs::write(second.join(".claude.json"), "{}").unwrap();
    std::fs::write(home.join(".claude/.config.json"), "{}").unwrap();
    assert_eq!(mcp_import::config_file(None, &home), home.join(".claude/.config.json"));
    // The built-in one's old file doesn't count for a second account, and the other way round.
    assert_eq!(mcp_import::config_file(Some(&second), &home), second.join(".claude.json"));
    std::fs::write(second.join(".config.json"), "{}").unwrap();
    assert_eq!(mcp_import::config_file(Some(&second), &home), second.join(".config.json"));
}

// ---- what read finds ----

#[test]
fn read_finds_user_scope_then_local_scope_servers_with_their_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(".claude.json");
    write_json(&file, &claude_json());
    let found = mcp_import::read(&file).unwrap();
    let order: Vec<(&str, &str, Option<&str>)> = found.iter().map(|f| (f.scope.as_str(), f.name.as_str(), f.folder.as_deref())).collect();
    assert_eq!(order, [
        ("user", "brave", None), ("user", "cookies", None), ("user", "filesystem", None), ("user", "github", None), ("user", "linear", None),
        ("local", "postgres", Some("/home/jefsev/code/alpha")), ("local", "sentry", Some("/home/jefsev/code/otus")),
    ]);

    let brave = by_name(&found, "brave");
    assert_eq!((brave.transport.as_str(), brave.command.as_str(), brave.url.as_str()), ("stdio", "npx", ""));
    assert_eq!(brave.args, ["-y", "@brave/brave-search-mcp-server"]);
    // sorted by name, values kept for the import itself
    assert_eq!(brave.env, pairs(&[("BRAVE_API_KEY", "BSA_SECRET_brave"), ("ZED", "zz_SECRET_zed")]));
    assert!(brave.headers.is_empty());

    let fs = by_name(&found, "filesystem");
    assert_eq!((fs.transport.as_str(), fs.command.as_str()), ("stdio", "npx"), "an entry without a type but with a command is stdio");
    assert_eq!(fs.args, ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
    assert!(fs.env.is_empty());

    let gh = by_name(&found, "github");
    assert_eq!((gh.transport.as_str(), gh.url.as_str(), gh.command.as_str()), ("http", "https://api.githubcopilot.com/mcp/", ""));
    assert!(gh.args.is_empty() && gh.env.is_empty());
    assert_eq!(gh.headers, pairs(&[("Authorization", "Bearer ghp_SECRET_user_github")]));

    let linear = by_name(&found, "linear");
    assert_eq!((linear.transport.as_str(), linear.url.as_str()), ("sse", "https://mcp.linear.app/sse"));
    assert!(linear.headers.is_empty());

    assert_eq!(by_name(&found, "cookies").transport, "http", "streamable-http is Claude Code's other name for http");

    let sentry = by_name(&found, "sentry");
    assert_eq!((sentry.scope.as_str(), sentry.folder.as_deref(), sentry.transport.as_str()), ("local", Some("/home/jefsev/code/otus"), "http"));
    assert_eq!(sentry.headers, pairs(&[("X-Api-Key", "sntrys_SECRET_sentry")]));

    let pg = by_name(&found, "postgres");
    assert_eq!((pg.command.as_str(), pg.args.len()), ("/usr/local/bin/pg-mcp", 0));
    assert_eq!(pg.env, pairs(&[("DATABASE_URL", "postgres://u:pw_SECRET_pg@db/app")]));
}

#[test]
fn an_entry_without_a_type_but_with_a_url_is_http() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(".claude.json");
    write_json(&file, &json!({ "mcpServers": { "old-remote": { "url": "https://old.example.com/mcp" } } }));
    let found = mcp_import::read(&file).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].transport.as_str(), found[0].url.as_str()), ("http", "https://old.example.com/mcp"));
}

#[test]
fn entries_gizai_cant_use_are_left_out_and_the_rest_still_come() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(".claude.json");
    write_json(&file, &json!({
        "mcpServers": {
            "ok": { "type": "stdio", "command": "uvx", "args": ["mcp-server-time"] },
            "websocket": { "type": "ws", "url": "wss://x.example.com/mcp" },
            "no-command": { "type": "stdio", "args": ["x"] },
            "blank-command": { "type": "stdio", "command": "  " },
            "no-url": { "type": "http", "headers": { "A": "b" } },
            "bad-args": { "type": "stdio", "command": "x", "args": "not a list" },
            "bad-env": { "type": "stdio", "command": "x", "env": { "N": 1 } },
            "not-an-object": "npx foo",
            "nothing": {}
        },
        "projects": { "/p": "not an object", "/q": { "mcpServers": [] }, "/r": { "mcpServers": { "fine": { "url": "https://r.example.com/mcp" } } } }
    }));
    let found = mcp_import::read(&file).unwrap();
    let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["ok", "fine"], "{found:?}");
}

#[test]
fn debug_of_found_shows_the_names_of_env_and_header_lines_but_never_their_values() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(".claude.json");
    write_json(&file, &claude_json());
    let found = mcp_import::read(&file).unwrap();
    for text in [format!("{found:?}"), format!("{found:#?}")] {
        for name in ["BRAVE_API_KEY", "ZED", "Authorization", "X-Api-Key", "DATABASE_URL", "Cookie"] {
            assert!(text.contains(name), "{name} missing from Debug: {text}");
        }
        for secret in SECRETS {
            assert!(!text.contains(secret), "Debug of Found printed the secret {secret}: {text}");
        }
    }
}

#[test]
fn a_missing_or_empty_config_file_has_no_servers_and_no_problem() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(mcp_import::read(&tmp.path().join("nope/.claude.json")).unwrap(), vec![]);
    let empty = tmp.path().join("empty.json");
    std::fs::write(&empty, "").unwrap();
    assert_eq!(mcp_import::read(&empty).unwrap(), vec![]);
    std::fs::write(&empty, " \n\t\n").unwrap();
    assert_eq!(mcp_import::read(&empty).unwrap(), vec![]);
    // A file Claude Code wrote without any MCP server
    std::fs::write(&empty, r#"{"numStartups": 3, "projects": {"/x": {"history": []}}}"#).unwrap();
    assert_eq!(mcp_import::read(&empty).unwrap(), vec![]);
    // with a byte order mark
    std::fs::write(&empty, b"\xEF\xBB\xBF{\"mcpServers\": {\"t\": {\"command\": \"uvx\"}}}").unwrap();
    assert_eq!(mcp_import::read(&empty).unwrap().len(), 1);
}

#[test]
fn a_malformed_config_file_is_a_plain_error_without_its_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(".claude.json");
    // cut off halfway, as a crashed write could leave it, with a secret before the cut
    std::fs::write(&file, r#"{"mcpServers": {"brave": {"command": "npx", "env": {"BRAVE_API_KEY": "BSA_SECRET_brave"}}, "x": "#).unwrap();
    let e = mcp_import::read(&file).unwrap_err();
    assert_eq!(e, format!("Couldn't read {}: it isn't valid JSON", file.display()));
    assert!(!e.contains("BSA_SECRET_brave"));

    std::fs::write(&file, r#"[{"mcpServers": {}}]"#).unwrap();
    let e = mcp_import::read(&file).unwrap_err();
    assert_eq!(e, format!("Couldn't read {}: it isn't a Claude Code config file", file.display()));
}

#[test]
fn a_config_file_that_cant_be_opened_is_a_plain_error() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join(".claude.json");
    write_json(&file, &claude_json());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let e = mcp_import::read(&file).unwrap_err();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(e.starts_with(&format!("Couldn't read {}: ", file.display())), "{e}");
    assert!(e.to_lowercase().contains("permission denied"), "{e}");
}

// ---- read only: nothing written, no credentials, no server ----

#[test]
fn read_leaves_the_config_folder_byte_for_byte_as_it_was() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".claude-2");
    let file = dir.join(".claude.json");
    write_json(&file, &claude_json());
    std::fs::write(dir.join("settings.json"), r#"{"model": "opus"}"#).unwrap();
    // an mtime in the past, so a rewrite with the same bytes would still show
    let past = SystemTime::now() - Duration::from_secs(3600);
    std::fs::File::options().write(true).open(&file).unwrap().set_modified(past).unwrap();
    let before = snapshot(&dir);
    assert_eq!(mcp_import::read(&file).unwrap().len(), 7);
    assert_eq!(mcp_import::read(&file).unwrap().len(), 7);
    let after = snapshot(&dir);
    assert_eq!(before.keys().collect::<Vec<_>>(), after.keys().collect::<Vec<_>>(), "read added or removed files");
    assert!(before == after, "read changed a file's bytes or mtime");
    assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), past);
}

#[test]
fn read_doesnt_open_credentials_json_even_when_it_would_block() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(".claude-2");
    let file = dir.join(".claude.json");
    write_json(&file, &claude_json());
    // Opening a FIFO for reading blocks until a writer comes: if read() opened .credentials.json it would hang.
    mkfifo(&dir.join(".credentials.json"));
    let found = read_within(&file, Duration::from_secs(10)).unwrap();
    assert_eq!(found.len(), 7);
}

#[test]
fn read_works_with_an_unreadable_credentials_json_and_takes_nothing_from_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("home/.claude");
    let creds = dir.join(".credentials.json");
    let file = tmp.path().join("home/.claude.json");
    write_json(&file, &claude_json());
    write_json(&creds, &json!({ "claudeAiOauth": { "accessToken": "sk-ant-oat-CREDS_SECRET" },
                                "mcpOAuth": { "sentry|abc": { "accessToken": "mcp-oauth-CREDS_SECRET" } } }));
    std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o000)).unwrap();
    let found = mcp_import::read(&file);
    std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
    let found = found.expect("an unreadable .credentials.json doesn't stop the read");
    assert_eq!(found.len(), 7);
    let all = serde_json::to_string(&found).unwrap();
    assert!(!all.contains("CREDS_SECRET"), "{all}");
}

#[test]
fn read_starts_no_server_and_calls_no_address() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("started");
    let script = tmp.path().join("server.sh");
    std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let file = tmp.path().join(".claude.json");
    write_json(&file, &json!({
        "mcpServers": {
            "local": { "type": "stdio", "command": script.display().to_string(), "args": [] },
            "sh": { "command": "/bin/sh", "args": ["-c", format!("touch '{}'", marker.display())] },
            "remote": { "type": "http", "url": url },
            "stream": { "type": "sse", "url": url }
        },
        "projects": { "/p": { "mcpServers": { "local-too": { "type": "stdio", "command": script.display().to_string() } } } }
    }));
    assert_eq!(mcp_import::read(&file).unwrap().len(), 5);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "a server's command ran");
    match listener.accept() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Ok((_, from)) => panic!("read() called the server's address (a connection from {from})"),
        Err(e) => panic!("{e}"),
    }
}

/// The control for the FIFO check above: reading the FIFO itself does hang, so that check would catch a reader.
#[test]
#[should_panic(expected = "didn't return within")]
fn the_fifo_check_catches_a_read_that_opens_the_fifo() {
    let tmp = tempfile::tempdir().unwrap();
    let fifo = tmp.path().join(".credentials.json");
    mkfifo(&fifo);
    let _ = read_within(&fifo, Duration::from_millis(500));
}
