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
const BROWSER_KEY: &str = "mcp_browser";
const SEEN_KEY: &str = "cli_tools_seen";
const ASKED_KEY: &str = "cli_tools_asked";
/// Names Gizai keeps for itself: its own server in chat, and the browser (Chrome DevTools MCP, built in).
pub const TAKEN: [&str; 2] = ["gizai", "chrome-devtools"];
/// The built-in browser's id and name: an agent's switch for it is `AgentServer { server_id: BROWSER, … }`, and its tools
/// are `mcp__chrome-devtools__<tool>`.
pub const BROWSER: &str = "chrome-devtools";
/// The Chrome DevTools MCP version a new install starts with: an exact version, never `latest`.
pub const BROWSER_VERSION: &str = "1.10.1";
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
    /// The servers switched on, with the tools switched off of each (the built-in browser among them).
    pub fn servers_on(&self) -> impl Iterator<Item = &AgentServer> {
        self.mcp.iter().filter(|s| s.on)
    }

    /// Whether the built-in browser (Chrome DevTools MCP) is on.
    pub fn browser_on(&self) -> bool {
        self.servers_on().any(|s| s.server_id == BROWSER)
    }
}

/// The CLI's own tools the agent has on (agent form → Tools → Web and Built-in tools), and the browser's one option. Kept
/// next to the MCP switches, under `cli` in the agent's `mcp_extra_json`. Everything is off until you switch it on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CliTools {
    /// Web search: Claude Code's `WebSearch`, Codex's `web_search`.
    pub web_search: bool,
    /// Fetching pages: Claude Code's `WebFetch`, Gemini's `web_fetch`.
    pub web_fetch: bool,
    /// Only these domains for fetching (Claude Code: `WebFetch(domain:…)`); none = any page.
    pub fetch_domains: Vec<String>,
    /// The browser accepts self-signed and expired certificates (local `.test` sites): `--acceptInsecureCerts`.
    pub insecure_certs: bool,
    /// The CLI's other tools switched on, by name: allowed in its task runs (a tool the catalog doesn't know among them).
    pub builtin: Vec<String>,
    /// Slash commands and skills (Built-in tools), on Claude Code only: its task runs go without `--disable-slash-commands`
    /// and get `SlashCommand` and `Skill`. Never in chat.
    pub slash_commands: bool,
}

impl CliTools {
    /// Whether anything that reads from the web is on (search, fetch); the browser is on `AgentTools`.
    pub fn web_on(&self) -> bool {
        self.web_search || self.web_fetch
    }
}

/// The built-in browser in Settings → MCP servers: Chrome DevTools MCP, always hidden (`--headless`) with a throwaway profile
/// (`--isolated`). Only its version and the browser program can change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BrowserEntry {
    /// An exact version of the `chrome-devtools-mcp` package, like 1.10.1.
    pub version: String,
    /// The browser program (`--executablePath`); empty = Google Chrome, else Chromium, as Gizai finds them.
    pub program: String,
}

impl Default for BrowserEntry {
    fn default() -> Self {
        BrowserEntry { version: BROWSER_VERSION.into(), program: String::new() }
    }
}

/// The tools a coding CLI reported: Claude Code's init line in an agent's last run or chat turn, or Ask Claude Code again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SeenTools {
    /// The coding CLI entry (Settings → Coding CLIs) that reported them.
    pub cli_id: String,
    /// Its built-in tools, as it named them (MCP servers' tools left out).
    pub tools: Vec<String>,
    pub at: i64,
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

/// The CLI tools in `mcp_extra_json` (its `cli`); anything unreadable is "nothing on".
pub fn parse_cli_tools(raw: Option<&str>) -> CliTools {
    raw.and_then(|j| serde_json::from_str::<Value>(j).ok()).and_then(|v| v.get("cli").cloned())
        .and_then(|c| serde_json::from_value(c).ok()).unwrap_or_default()
}

/// Why an agent on `kind` can't have MCP servers or the browser; None on Claude Code.
pub fn mcp_not_on(kind: &str) -> Option<&'static str> {
    match kind {
        "claude_code" => None,
        "codex" => Some("Codex takes MCP servers per run, but Gizai hasn't checked yet that a headless codex exec may call their tools: \
                         MCP servers and the browser stay off for Codex agents for now."),
        "gemini" => Some("Gemini takes MCP servers only from its settings files, and Gizai never writes in ~/.gemini or the worktree: \
                          MCP servers and the browser stay off for Gemini agents."),
        _ => Some("This CLI runs with its own settings: Gizai can't give it MCP servers or the browser."),
    }
}

/// The agent's CLI kind and its entry's name.
fn agent_cli(db: &Db, agent_id: &str) -> Result<crate::clis::Cli> {
    let adapter: Option<String> = db.read(|c| Ok(c.query_row("SELECT adapter FROM agent_configs WHERE actor_id=?1", [agent_id], |r| r.get(0)).optional()?))?
        .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;
    crate::clis::get(db, adapter.as_deref().unwrap_or_default())
}

/// Writes one key of the agent's `mcp_extra_json` (`mcp` or `cli`), keeping the other, with `change` for the audit log.
fn write_extra(db: &Db, actor: &str, agent_id: &str, key: &str, value: Value, change: Value) -> Result<()> {
    db.write(Some(actor), |w| {
        let raw: Option<Option<String>> = w.conn().query_row("SELECT mcp_extra_json FROM agent_configs WHERE actor_id=?1", [agent_id], |r| r.get(0)).optional()?;
        let Some(raw) = raw else { return Err(Error::NotFound(format!("agent {agent_id}"))) };
        let mut all = raw.and_then(|j| serde_json::from_str::<Value>(&j).ok()).filter(Value::is_object).unwrap_or_else(|| serde_json::json!({}));
        all[key] = value;
        w.conn().execute("UPDATE agent_configs SET mcp_extra_json=?2, updated_at=?3, version=version+1 WHERE actor_id=?1",
                         rusqlite::params![agent_id, all.to_string(), ids::now_ms()])?;
        w.update("agent_configs", agent_id, change)
    })
}

/// Saves the agent's tool settings (agent form → Tools; never the Team Lead). Servers that aren't in the list (or the
/// built-in browser) are dropped; an agent on Codex, Gemini or another CLI can't have one on (`mcp_not_on`).
pub fn set_agent_tools(db: &Db, actor: &str, agent_id: &str, tools: AgentTools) -> Result<AgentTools> {
    let servers = list(db)?;
    let mut clean = AgentTools::default();
    for s in tools.mcp {
        let known = s.server_id == BROWSER || servers.iter().any(|o| o.id == s.server_id);
        if !known || clean.mcp.iter().any(|o| o.server_id == s.server_id) {
            continue;
        }
        let mut off: Vec<String> = s.tools_off.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
        off.sort();
        off.dedup();
        clean.mcp.push(AgentServer { server_id: s.server_id, on: s.on, tools_off: off });
    }
    let cli = agent_cli(db, agent_id)?;
    if mcp_not_on(&cli.kind).is_some() && clean.servers_on().next().is_some() {
        return Err(Error::Invalid(format!("MCP servers and the browser work on Claude Code for now: {} runs on {}, which Gizai hasn't checked them with",
                                          agent_name(db, agent_id)?, cli.name)));
    }
    let on: Vec<&str> = clean.servers_on().map(|s| s.server_id.as_str()).collect();
    write_extra(db, actor, agent_id, "mcp", serde_json::to_value(&clean.mcp)?, serde_json::json!({"mcp_servers_on": on}))?;
    Ok(clean)
}

/// A domain for `WebFetch(domain:…)`: a host name like docs.rs or *.example.com, without a scheme, path or port.
pub fn clean_domain(raw: &str) -> Option<String> {
    let d = raw.trim().trim_end_matches('/').to_lowercase();
    let d = d.strip_prefix("https://").or_else(|| d.strip_prefix("http://")).unwrap_or(&d).to_string();
    let host = d.strip_prefix("*.").unwrap_or(&d);
    let ok = !host.is_empty() && host.len() <= 253 && host.contains('.') && !host.starts_with('.') && !host.ends_with('.')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    ok.then_some(d)
}

/// A built-in tool's name as a CLI reports it: letters, digits, `_` and `-`. MCP tools (`mcp__…`) are switched per server.
pub fn valid_tool_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && !n.starts_with("mcp__") && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Saves the agent's CLI tools (agent form → Tools → Web and Built-in tools; never the Team Lead). Domains and names are
/// cleaned; one that can't be used is refused with why.
pub fn set_cli_tools(db: &Db, actor: &str, agent_id: &str, tools: CliTools) -> Result<CliTools> {
    let mut clean = CliTools { web_search: tools.web_search, web_fetch: tools.web_fetch, insecure_certs: tools.insecure_certs,
                               slash_commands: tools.slash_commands, ..Default::default() };
    for raw in tools.fetch_domains.iter().filter(|d| !d.trim().is_empty()) {
        let d = clean_domain(raw).ok_or_else(|| Error::Invalid(format!("\"{}\" isn't a domain: write it like docs.rs or *.example.com", raw.trim())))?;
        if !clean.fetch_domains.contains(&d) {
            clean.fetch_domains.push(d);
        }
    }
    for raw in tools.builtin.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !valid_tool_name(raw) {
            return Err(Error::Invalid(format!("\"{raw}\" isn't the name of one of the CLI's own tools")));
        }
        if !clean.builtin.iter().any(|t| t == raw) {
            clean.builtin.push(raw.to_string());
        }
    }
    clean.builtin.sort();
    agent_cli(db, agent_id)?;
    let change = serde_json::json!({"web_search": clean.web_search, "web_fetch": clean.web_fetch, "fetch_domains": clean.fetch_domains,
                                    "insecure_certs": clean.insecure_certs, "builtin_tools_on": clean.builtin,
                                    "slash_commands": clean.slash_commands});
    write_extra(db, actor, agent_id, "cli", serde_json::to_value(&clean)?, change)?;
    Ok(clean)
}

/// The agent's CLI tools.
pub fn agent_cli_tools(db: &Db, agent_id: &str) -> Result<CliTools> {
    db.read(|c| {
        let raw: Option<Option<String>> = c.query_row("SELECT mcp_extra_json FROM agent_configs WHERE actor_id=?1", [agent_id], |r| r.get(0)).optional()?;
        match raw {
            None => Err(Error::NotFound(format!("agent {agent_id}"))),
            Some(j) => Ok(parse_cli_tools(j.as_deref())),
        }
    })
}

/// The built-in browser as it is set (Settings → MCP servers).
pub fn browser(db: &Db) -> Result<BrowserEntry> {
    Ok(settings::get::<BrowserEntry>(db, BROWSER_KEY)?.unwrap_or_default())
}

/// An exact version like 1.10.1 (or 1.11.0-beta.2): never `latest`, a range or a tag.
pub fn exact_version(v: &str) -> bool {
    let (core, pre) = v.split_once('-').unwrap_or((v, ""));
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.len() <= 6 && p.chars().all(|c| c.is_ascii_digit()))
        && (v.split_once('-').is_none() || (!pre.is_empty() && pre.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')))
}

/// Saves the built-in browser's version and program; the program's own checks (a path, not Brave) are the app's.
pub fn set_browser(db: &Db, entry: BrowserEntry) -> Result<BrowserEntry> {
    let version = entry.version.trim().trim_start_matches('v').to_string();
    if !exact_version(&version) {
        return Err(Error::Invalid(format!("give Chrome DevTools MCP an exact version, like {BROWSER_VERSION}: not \"{}\"", entry.version.trim())));
    }
    let clean = BrowserEntry { version, program: entry.program.trim().to_string() };
    settings::set(db, BROWSER_KEY, &clean)?;
    Ok(clean)
}

/// The tools each agent's Claude Code reported in its last run or chat turn, by agent id.
pub fn seen_tools(db: &Db) -> Result<BTreeMap<String, SeenTools>> {
    Ok(settings::get(db, SEEN_KEY)?.unwrap_or_default())
}

/// The tools each coding CLI reported when you asked it (Ask Claude Code again), by CLI id.
pub fn asked_tools(db: &Db) -> Result<BTreeMap<String, SeenTools>> {
    Ok(settings::get(db, ASKED_KEY)?.unwrap_or_default())
}

/// Built-in tool names from a CLI's list: MCP servers' tools (`mcp__…`) left out, each once, in its order.
pub fn builtin_names(tools: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in tools.iter().map(|t| t.trim()).filter(|t| valid_tool_name(t)) {
        if !out.iter().any(|o| o == t) {
            out.push(t.to_string());
        }
    }
    out
}

fn keep_seen(db: &Db, key: &str, who: &str, seen: SeenTools) -> Result<()> {
    db.write(None, |w| {
        let c = w.conn();
        let raw: Option<String> = c.query_row("SELECT value_json FROM settings WHERE key=?1 AND org_id=''", [key], |r| r.get(0)).optional()?;
        let mut all: BTreeMap<String, SeenTools> = raw.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
        all.insert(who.to_string(), seen);
        c.execute("INSERT INTO settings(key, org_id, value_json, updated_at) VALUES (?1, '', ?2, ?3)
                   ON CONFLICT(key, org_id) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
                  rusqlite::params![key, serde_json::to_string(&all)?, ids::now_ms()])?;
        Ok(())
    })
}

/// Keeps the tools a run's or chat turn's init line named, as the agent's "seen in the last run".
pub fn set_seen_tools(db: &Db, agent_id: &str, cli_id: &str, tools: &[String]) -> Result<()> {
    keep_seen(db, SEEN_KEY, agent_id, SeenTools { cli_id: cli_id.into(), tools: builtin_names(tools), at: ids::now_ms() })
}

/// Keeps the tools a coding CLI listed when asked (Ask Claude Code again).
pub fn set_asked_tools(db: &Db, cli_id: &str, tools: &[String]) -> Result<()> {
    keep_seen(db, ASKED_KEY, cli_id, SeenTools { cli_id: cli_id.into(), tools: builtin_names(tools), at: ids::now_ms() })
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
