//! Claude Code's own MCP servers, to import into Gizai. `claude mcp add` writes them to the global config file of that
//! Claude Code (`config_file`): user scope servers under `mcpServers`, local scope ones under
//! `projects.<folder>.mcpServers`, each as `{"type": "stdio", "command", "args", "env"}` or
//! `{"type": "http" | "sse", "url", "headers"}`. Gizai only reads that one file: never `.credentials.json` (where a
//! server's OAuth tokens can be), and it writes nothing and starts no server.
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

/// One MCP server found in a Claude Code config file. Values are kept for the import itself; what Gizai shows uses
/// only the names (so `Debug` leaves the env and header values out).
#[derive(Clone, PartialEq, Serialize)]
pub struct Found {
    pub name: String,
    /// "user" | "local"
    pub scope: String,
    /// The project folder, for local scope, as Claude Code keys it: in a git repository, its root (for a worktree, the
    /// repository's main checkout).
    pub folder: Option<String>,
    /// "stdio" | "http" | "sse"
    pub transport: String,
    /// stdio: the program and its arguments.
    pub command: String,
    pub args: Vec<String>,
    /// http and sse: where the server is.
    pub url: String,
    /// stdio: set on top of the environment, sorted by name.
    pub env: Vec<(String, String)>,
    /// http and sse: sent with every request (e.g. Authorization), sorted by name.
    pub headers: Vec<(String, String)>,
}

impl std::fmt::Debug for Found {
    /// Like a derived one, but env and headers show only their names: the values are often keys or tokens.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names = |kv: &[(String, String)]| kv.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        f.debug_struct("Found")
            .field("name", &self.name)
            .field("scope", &self.scope)
            .field("folder", &self.folder)
            .field("transport", &self.transport)
            .field("command", &self.command)
            .field("args", &self.args)
            .field("url", &self.url)
            .field("env", &names(&self.env))
            .field("headers", &names(&self.headers))
            .finish()
    }
}

/// The global config file of a Claude Code, the one `claude mcp add` writes: `<config_dir>/.claude.json` for an account
/// with its own config folder (CLAUDE_CONFIG_DIR), else `<home>/.claude.json`. As in Claude Code, an old-style
/// `.config.json` in its config folder (`<config_dir>`, else `<home>/.claude`) is the one when it is there.
pub fn config_file(config_dir: Option<&Path>, home: &Path) -> PathBuf {
    let config_dir = config_dir.filter(|d| !d.as_os_str().is_empty());
    let old = config_dir.map(Path::to_path_buf).unwrap_or_else(|| home.join(".claude")).join(".config.json");
    if old.exists() {
        return old;
    }
    config_dir.unwrap_or(home).join(".claude.json")
}

/// The MCP servers in a Claude Code config file (`config_file`): user scope first, then local scope per project folder
/// (folders sorted), each by name. A missing or empty file has none. An entry Gizai can't use (no command or URL, a
/// value of the wrong kind, another transport such as `ws`) is left out, and the rest still come. Reads only `path`
/// (never `.credentials.json`), writes nothing and starts no server.
pub fn read(path: &Path) -> Result<Vec<Found>, String> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(format!("Couldn't read {}: {e}", path.display())),
    };
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes);
    if text.iter().all(u8::is_ascii_whitespace) {
        return Ok(vec![]);
    }
    let v: Value = serde_json::from_slice(text).map_err(|_| format!("Couldn't read {}: it isn't valid JSON", path.display()))?;
    let Some(top) = v.as_object() else {
        return Err(format!("Couldn't read {}: it isn't a Claude Code config file", path.display()));
    };
    let mut out = servers(top, "user", None);
    let mut folders: Vec<(&String, &Map<String, Value>)> = top.get("projects").and_then(Value::as_object).into_iter().flatten()
        .filter_map(|(folder, p)| Some((folder, p.as_object()?))).collect();
    folders.sort_by(|a, b| a.0.cmp(b.0));
    for (folder, p) in folders {
        out.extend(servers(p, "local", Some(folder)));
    }
    Ok(out)
}

/// The servers under `mcpServers` in `parent`, by name.
fn servers(parent: &Map<String, Value>, scope: &str, folder: Option<&str>) -> Vec<Found> {
    let mut out: Vec<Found> = parent.get("mcpServers").and_then(Value::as_object).into_iter().flatten()
        .filter_map(|(name, v)| server(name, v, scope, folder)).collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// One entry, if Gizai can run it. Without a `type`, one with a `command` is stdio and one with a `url` is http;
/// "streamable-http" is Claude Code's other name for http.
fn server(name: &str, v: &Value, scope: &str, folder: Option<&str>) -> Option<Found> {
    let o = v.as_object()?;
    let has = |k: &str| o.get(k).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
    let transport = match o.get("type") {
        None | Some(Value::Null) if has("command") => "stdio",
        None | Some(Value::Null) if has("url") => "http",
        Some(Value::String(t)) => match t.as_str() {
            "stdio" => "stdio",
            "http" | "streamable-http" => "http",
            "sse" => "sse",
            _ => return None,
        },
        _ => return None,
    };
    let stdio = transport == "stdio";
    if name.trim().is_empty() || !has(if stdio { "command" } else { "url" }) {
        return None;
    }
    let text = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    Some(Found {
        name: name.to_string(),
        scope: scope.to_string(),
        folder: folder.map(str::to_string),
        transport: transport.to_string(),
        command: if stdio { text("command") } else { String::new() },
        args: if stdio { strings(o.get("args"))? } else { vec![] },
        url: if stdio { String::new() } else { text("url") },
        env: if stdio { pairs(o.get("env"))? } else { vec![] },
        headers: if stdio { vec![] } else { pairs(o.get("headers"))? },
    })
}

/// A list of strings. Missing or null is an empty list; anything else but strings makes the entry unreadable.
fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    match v {
        None | Some(Value::Null) => Some(vec![]),
        Some(Value::Array(a)) => a.iter().map(|x| x.as_str().map(str::to_string)).collect(),
        Some(_) => None,
    }
}

/// An object of strings as (name, value) pairs, sorted by name. Missing or null is none; anything else but strings
/// makes the entry unreadable.
fn pairs(v: Option<&Value>) -> Option<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = match v {
        None | Some(Value::Null) => vec![],
        Some(Value::Object(m)) => m.iter().map(|(k, x)| x.as_str().map(|s| (k.clone(), s.to_string()))).collect::<Option<_>>()?,
        Some(_) => return None,
    };
    out.sort();
    Some(out)
}
