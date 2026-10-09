//! MCP servers (Settings → MCP servers): one list for all agents, each switched on per agent with a switch per tool
//! (`AgentTools`, in the agent's `mcp_extra_json`). A server is a command (stdio) or an address (http, sse). Its
//! environment values, header values and sign-in tokens are secrets: they live in the OS keychain, and this list keeps
//! only their names. Only you change any of it, in Settings and the agent form: the Team Lead's tools can't.
use std::collections::BTreeMap;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;
use crate::{Error, Result, ids, settings};

const KEY: &str = "mcp_servers";
const TOOLS_KEY: &str = "mcp_tools";
const LAST_RUN_KEY: &str = "mcp_last_run";
/// Names Gizai keeps for itself: its own server in chat, and the browser that comes later.
pub const TAKEN: [&str; 2] = ["gizai", "chrome-devtools"];
pub const TRANSPORTS: [&str; 3] = ["stdio", "http", "sse"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct McpServer {
    /// Empty for a new one (saving gives it an id). The keychain keeps its secrets under this id, so a rename keeps them.
    pub id: String,
    /// Letters, digits, `-` and `_`: its tools are `mcp__<name>__<tool>`.
    pub name: String,
    /// stdio | http | sse
    pub transport: String,
    /// stdio: the program and its arguments.
    pub command: String,
    pub args: Vec<String>,
    /// stdio: the names of its environment lines (the values are in the keychain).
    pub env_names: Vec<String>,
    /// http and sse: its address.
    pub url: String,
    /// http and sse: the names of its header lines (the values are in the keychain).
    pub header_names: Vec<String>,
    /// Sign-in: a client id you entered, for a server that doesn't let Gizai register itself. Not a secret: Gizai signs in
    /// as a public client.
    pub client_id: String,
    /// Where it came from, like "Claude Code, local scope: /home/me/code/otus"; empty when added by hand.
    pub source: String,
}

/// The agent's own tool settings (agent form → Tools). Everything is off until you switch it on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentTools {
    /// The MCP servers it has a switch for, by server id; one that isn't here is off.
    pub mcp: Vec<AgentServer>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentServer {
    pub server_id: String,
    pub on: bool,
    /// Its tools switched off, by name; none = all of them on.
    pub tools_off: Vec<String>,
}

impl AgentTools {
    /// The servers switched on, with the tools switched off of each.
    pub fn servers_on(&self) -> impl Iterator<Item = &AgentServer> {
        self.mcp.iter().filter(|s| s.on)
    }
}

/// What List tools found last: the server's name and version as it gave them, and its tools as it listed them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolList {
    pub server_name: String,
    pub server_version: String,
    pub listed_at: i64,
    pub tools: Vec<Value>,
    /// It answered that it needs sign-in (HTTP 401), with its WWW-Authenticate header for the sign-in to start from.
    pub needs_sign_in: bool,
    pub www_authenticate: String,
    /// Why the last List tools or sign-in failed, in plain words (never with a secret in it).
    pub problem: Option<String>,
}

/// A server's state in an agent's last run, as Claude Code's init line gave it (connected, failed, needs-auth, …).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LastRun {
    pub status: String,
    pub at: i64,
}

pub fn list(db: &Db) -> Result<Vec<McpServer>> {
    Ok(settings::get::<Vec<McpServer>>(db, KEY)?.unwrap_or_default())
}

pub fn get(db: &Db, id: &str) -> Result<McpServer> {
    list(db)?.into_iter().find(|s| s.id == id).ok_or_else(|| Error::NotFound(format!("MCP server {id}")))
}

/// Letters, digits, `-` and `_`, at most 64.
pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Whether Claude Code can tell where the name ends in its tool names, `mcp__<name>__<tool>`: it splits them at each `__`,
/// so "gizai__notes" or "gizai_" would pass for the gizai server, and their tools for Gizai's own.
pub fn clear_in_tool_names(n: &str) -> bool {
    !n.contains("__") && !n.ends_with('_')
}

/// Whether `name` is free for a new server (or for the one with `own_id`), and if not, why, in plain words.
pub fn name_problem(servers: &[McpServer], name: &str, own_id: &str) -> Option<String> {
    let name = name.trim();
    if !valid_name(name) {
        return Some(format!("an MCP server's name takes letters, digits, - and _ (at most 64), not \"{name}\""));
    }
    if !clear_in_tool_names(name) {
        return Some(format!("an MCP server's name can't have two _ in a row or end with _, not \"{name}\": its tools are called \
                             mcp__<name>__<tool>, and Claude Code reads __ as where the name ends"));
    }
    if TAKEN.iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return Some(format!("{name} is Gizai's own: give the server another name"));
    }
    if servers.iter().any(|s| s.id != own_id && s.name.eq_ignore_ascii_case(name)) {
        return Some(format!("there is already an MCP server called {name}: give this one another name"));
    }
    None
}

fn valid_env_name(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn valid_header_name(n: &str) -> bool {
    !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c))
}

fn names(list: &[String], valid: fn(&str) -> bool, what: &str, server: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for n in list.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        if !valid(n) {
            return Err(Error::Invalid(format!("{server}: \"{n}\" can't be the name of {what}")));
        }
        if out.iter().any(|o| o.eq_ignore_ascii_case(n)) {
            return Err(Error::Invalid(format!("{server} has {what} {n} twice")));
        }
        out.push(n.to_string());
    }
    Ok(out)
}

/// Checks a server and fills in what is left to its kind (a new one gets an id).
pub fn clean(servers: &[McpServer], s: McpServer) -> Result<McpServer> {
    let id = if s.id.trim().is_empty() { ids::new_id() } else { s.id.trim().to_string() };
    let name = s.name.trim().to_string();
    if let Some(why) = name_problem(servers, &name, &id) {
        return Err(Error::Invalid(why));
    }
    let transport = if s.transport.trim().is_empty() { "stdio".to_string() } else { s.transport.trim().to_lowercase() };
    if !TRANSPORTS.contains(&transport.as_str()) {
        return Err(Error::Invalid(format!("an MCP server is a command (stdio) or an address (http or sse), not {transport}")));
    }
    let mut out = McpServer { id, name: name.clone(), transport: transport.clone(), client_id: s.client_id.trim().to_string(), source: s.source.trim().to_string(), ..Default::default() };
    if transport == "stdio" {
        out.command = s.command.trim().to_string();
        if out.command.is_empty() {
            return Err(Error::Invalid(format!("give {name} its command, like npx -y @acme/mcp-server or /usr/local/bin/my-server")));
        }
        out.args = s.args.into_iter().filter(|a| !a.is_empty()).collect();
        out.env_names = names(&s.env_names, valid_env_name, "an environment line", &name)?;
    } else {
        out.url = s.url.trim().to_string();
        let ok = (out.url.starts_with("https://") || out.url.starts_with("http://")) && out.url.len() > "https://".len();
        if !ok {
            return Err(Error::Invalid(format!("give {name} its address, starting with https://")));
        }
        out.header_names = names(&s.header_names, valid_header_name, "a header", &name)?;
    }
    Ok(out)
}

/// Adds the server or saves the one with its id; returns it as saved.
pub fn save(db: &Db, s: McpServer) -> Result<McpServer> {
    let mut all = list(db)?;
    let s = clean(&all, s)?;
    match all.iter_mut().find(|o| o.id == s.id) {
        Some(o) => *o = s.clone(),
        None => all.push(s.clone()),
    }
    settings::set(db, KEY, &all)?;
    Ok(s)
}

/// Removes the server from the list and its tools from the cache; agents that had it just don't get it any more.
pub fn remove(db: &Db, id: &str) -> Result<McpServer> {
    let mut all = list(db)?;
    let Some(i) = all.iter().position(|s| s.id == id) else { return Err(Error::NotFound(format!("MCP server {id}"))) };
    let gone = all.remove(i);
    settings::set(db, KEY, &all)?;
    let mut cache = tool_lists(db)?;
    if cache.remove(id).is_some() {
        settings::set(db, TOOLS_KEY, &cache)?;
    }
    Ok(gone)
}

/// The tools List tools found last, per server id.
pub fn tool_lists(db: &Db) -> Result<BTreeMap<String, ToolList>> {
    Ok(settings::get::<BTreeMap<String, ToolList>>(db, TOOLS_KEY)?.unwrap_or_default())
}

pub fn set_tool_list(db: &Db, server_id: &str, list: &ToolList) -> Result<()> {
    change_tool_list(db, server_id, |t| *t = list.clone())
}

/// Changes what is kept for the server (its tools, whether it needs sign-in, its last problem) in one write.
pub fn change_tool_list(db: &Db, server_id: &str, f: impl FnOnce(&mut ToolList)) -> Result<()> {
    db.write(None, |w| {
        let c = w.conn();
        let raw: Option<String> = c.query_row("SELECT value_json FROM settings WHERE key=?1 AND org_id=''", [TOOLS_KEY], |r| r.get(0)).optional()?;
        let mut all: BTreeMap<String, ToolList> = raw.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        f(all.entry(server_id.to_string()).or_default());
        c.execute("INSERT INTO settings(key, org_id, value_json, updated_at) VALUES (?1, '', ?2, ?3)
                   ON CONFLICT(key, org_id) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
                  rusqlite::params![TOOLS_KEY, serde_json::to_string(&all)?, ids::now_ms()])?;
        Ok(())
    })
}

/// The agent's tool settings.
pub fn agent_tools(db: &Db, agent_id: &str) -> Result<AgentTools> {
    db.read(|c| {
        let raw: Option<Option<String>> = c.query_row("SELECT mcp_extra_json FROM agent_configs WHERE actor_id=?1", [agent_id], |r| r.get(0)).optional()?;
        match raw {
            None => Err(Error::NotFound(format!("agent {agent_id}"))),
            Some(j) => Ok(parse_tools(j.as_deref())),
        }
    })
}

/// `mcp_extra_json` as saved; anything unreadable is "nothing on".
pub fn parse_tools(raw: Option<&str>) -> AgentTools {
    raw.and_then(|j| serde_json::from_str(j).ok()).unwrap_or_default()
}

/// Saves the agent's tool settings (agent form → Tools; never the Team Lead). Servers that aren't in the list are dropped;
/// an agent on Codex, Gemini or another CLI can't have one on yet.
pub fn set_agent_tools(db: &Db, actor: &str, agent_id: &str, tools: AgentTools) -> Result<AgentTools> {
    let servers = list(db)?;
    let mut clean = AgentTools::default();
    for s in tools.mcp {
        if !servers.iter().any(|o| o.id == s.server_id) || clean.mcp.iter().any(|o| o.server_id == s.server_id) {
            continue;
        }
        let mut off: Vec<String> = s.tools_off.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        off.sort();
        off.dedup();
        clean.mcp.push(AgentServer { server_id: s.server_id, on: s.on, tools_off: off });
    }
    let adapter: Option<String> = db.read(|c| Ok(c.query_row("SELECT adapter FROM agent_configs WHERE actor_id=?1", [agent_id], |r| r.get(0)).optional()?))?
        .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;
    let cli = crate::clis::get(db, adapter.as_deref().unwrap_or_default())?;
    if cli.kind != "claude_code" && clean.servers_on().next().is_some() {
        return Err(Error::Invalid(format!("MCP servers work on Claude Code for now: {} runs on {}, which gets them later (GA-55)", agent_name(db, agent_id)?, cli.name)));
    }
    let json = serde_json::to_string(&clean)?;
    db.write(Some(actor), |w| {
        let n = w.conn().execute("UPDATE agent_configs SET mcp_extra_json=?2, updated_at=?3, version=version+1 WHERE actor_id=?1",
                                 rusqlite::params![agent_id, json, ids::now_ms()])?;
        if n == 0 {
            return Err(Error::NotFound(format!("agent {agent_id}")));
        }
        let on: Vec<&str> = clean.servers_on().map(|s| s.server_id.as_str()).collect();
        w.update("agent_configs", agent_id, serde_json::json!({"mcp_servers_on": on}))
    })?;
    Ok(clean)
}

fn agent_name(db: &Db, agent_id: &str) -> Result<String> {
    db.read(|c| Ok(c.query_row("SELECT name FROM actors WHERE id=?1", [agent_id], |r| r.get(0)).optional()?.unwrap_or_default()))
}

/// Each server's state in each agent's last run, by agent id, then server name.
pub fn last_runs(db: &Db) -> Result<BTreeMap<String, BTreeMap<String, LastRun>>> {
    Ok(settings::get(db, LAST_RUN_KEY)?.unwrap_or_default())
}

/// Keeps what the init line of the agent's run said about its MCP servers (`gizai` itself left out).
pub fn set_last_run(db: &Db, agent_id: &str, states: &[(String, String)]) -> Result<()> {
    let now = ids::now_ms();
    db.write(None, |w| {
        let c = w.conn();
        let raw: Option<String> = c.query_row("SELECT value_json FROM settings WHERE key=?1 AND org_id=''", [LAST_RUN_KEY], |r| r.get(0)).optional()?;
        let mut all: BTreeMap<String, BTreeMap<String, LastRun>> = raw.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        let mine: BTreeMap<String, LastRun> = states.iter().filter(|(n, _)| n != "gizai")
            .map(|(n, s)| (n.clone(), LastRun { status: s.clone(), at: now })).collect();
        all.insert(agent_id.to_string(), mine);
        c.execute("INSERT INTO settings(key, org_id, value_json, updated_at) VALUES (?1, '', ?2, ?3)
                   ON CONFLICT(key, org_id) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
                  rusqlite::params![LAST_RUN_KEY, serde_json::to_string(&all)?, now])?;
        Ok(())
    })
}
