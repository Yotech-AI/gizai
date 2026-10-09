//! Settings → MCP servers and the agent form's Tools: the servers with their secrets in the keychain, List tools, the
//! import from Claude Code, sign-in, the switches per agent, and what a run of an agent gets (`for_run`).
//! Secret values (environment lines, header lines, sign-in tokens) never leave this module except into the keychain and
//! a run's own MCP config file: views show their names only.
use std::time::Duration;

use gizai_agents::mcp_client::{self, ListError, Target, Transport};
use gizai_agents::mcp_run::{self, RunServer};
use gizai_agents::mcp_tools::{self, ToolView};
use gizai_agents::oauth::{self, TokenProblem};
use gizai_agents::{mcp_import, secrets::Keychain};
use gizai_core::mcp_servers::{self as core_mcp, AgentTools, LastRun, McpServer};
use gizai_core::team::Member;
use serde::{Deserialize, Serialize};

use crate::AppState;

/// How long List tools waits: an `npx -y` server downloads its package the first time.
pub const LIST_TIMEOUT: Duration = Duration::from_secs(120);
/// How long a sign-in waits for the browser to come back.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// A secret line from the form: its value only when typed in now (None keeps the one in the keychain).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SecretLine {
    pub name: String,
    pub value: Option<String>,
}

/// A server as the form saves it: the server, and its environment and header lines with any new values.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerInput {
    pub server: McpServer,
    pub env: Vec<SecretLine>,
    pub headers: Vec<SecretLine>,
}

/// What List tools found last, each tool in plain words.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    pub server_name: String,
    pub server_version: String,
    pub listed_at: i64,
    pub tools: Vec<ToolView>,
}

/// A server as Settings shows it: names, never values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerView {
    #[serde(flatten)]
    pub server: McpServer,
    /// signed_in | needs_sign_in | "" (no sign-in asked for so far)
    pub sign_in: String,
    /// What went wrong last (List tools, sign-in, a refresh), in plain words.
    pub problem: Option<String>,
    /// Environment or header lines whose value isn't in the keychain.
    pub missing: Vec<String>,
    pub listed: Option<Listed>,
    /// The agents that have it on.
    pub used_by: Vec<String>,
}

fn env_key(id: &str, name: &str) -> String {
    format!("mcp/{id}/env/{name}")
}

fn header_key(id: &str, name: &str) -> String {
    format!("mcp/{id}/header/{name}")
}

fn keychain_problem(e: String) -> String {
    format!("The keychain couldn't be used: {e}")
}

/// The secret values of a server's lines from the keychain: (name, value) found, and the names missing.
fn secrets(k: &dyn Keychain, s: &McpServer) -> Result<(Vec<(String, String)>, Vec<(String, String)>, Vec<String>), String> {
    let (mut env, mut headers, mut missing) = (vec![], vec![], vec![]);
    for n in &s.env_names {
        match k.get(&env_key(&s.id, n)).map_err(keychain_problem)? {
            Some(v) => env.push((n.clone(), v)),
            None => missing.push(n.clone()),
        }
    }
    for n in &s.header_names {
        match k.get(&header_key(&s.id, n)).map_err(keychain_problem)? {
            Some(v) => headers.push((n.clone(), v)),
            None => missing.push(n.clone()),
        }
    }
    Ok((env, headers, missing))
}

fn view(st: &AppState, s: McpServer, lists: &std::collections::BTreeMap<String, core_mcp::ToolList>, agents: &[Member]) -> ServerView {
    let cached = lists.get(&s.id);
    let (missing, mut problem) = match secrets(st.keychain.as_ref(), &s) {
        Ok((_, _, m)) => (m, None),
        Err(e) => (vec![], Some(e)),
    };
    let signed_in = s.transport != "stdio" && st.tokens.load(&s.id).ok().flatten().is_some();
    let sign_in = if signed_in { "signed_in" } else if cached.is_some_and(|c| c.needs_sign_in) { "needs_sign_in" } else { "" };
    problem = problem.or_else(|| cached.and_then(|c| c.problem.clone()));
    let listed = cached.filter(|c| c.listed_at > 0).map(|c| Listed {
        server_name: c.server_name.clone(), server_version: c.server_version.clone(), listed_at: c.listed_at, tools: mcp_tools::describe_all(&c.tools),
    });
    let used_by = agents.iter().filter(|a| a.tools.servers_on().any(|o| o.server_id == s.id)).map(|a| a.name.clone()).collect();
    ServerView { server: s, sign_in: sign_in.into(), problem, missing, listed, used_by }
}

/// Settings → MCP servers.
pub fn list(st: &AppState) -> Result<Vec<ServerView>, String> {
    let servers = core_mcp::list(&st.db).map_err(|e| e.to_string())?;
    let lists = core_mcp::tool_lists(&st.db).map_err(|e| e.to_string())?;
    let agents: Vec<Member> = gizai_core::team::all_agents(&st.db).map_err(|e| e.to_string())?.into_iter().map(|(_, m)| m).collect();
    Ok(servers.into_iter().map(|s| view(st, s, &lists, &agents)).collect())
}

pub fn get(st: &AppState, id: &str) -> Result<ServerView, String> {
    list(st)?.into_iter().find(|v| v.server.id == id).ok_or_else(|| format!("there is no MCP server with id {id}"))
}

/// Adds or saves a server: the names in the settings, the values typed in now in the keychain. A line taken out loses its
/// value from the keychain too.
pub fn save(st: &AppState, input: ServerInput) -> Result<ServerView, String> {
    let k = st.keychain.as_ref();
    let mut s = input.server;
    s.env_names = input.env.iter().map(|l| l.name.trim().to_string()).filter(|n| !n.is_empty()).collect();
    s.header_names = input.headers.iter().map(|l| l.name.trim().to_string()).filter(|n| !n.is_empty()).collect();
    let old = core_mcp::list(&st.db).map_err(|e| e.to_string())?.into_iter().find(|o| !s.id.is_empty() && o.id == s.id);
    let saved = core_mcp::save(&st.db, s).map_err(|e| e.to_string())?;
    let id = saved.id.clone();
    // Only the lines the server keeps: a command has no headers and an address no environment.
    for (lines, kept, key) in [(&input.env, &saved.env_names, env_key as fn(&str, &str) -> String), (&input.headers, &saved.header_names, header_key)] {
        for l in lines.iter().filter(|l| kept.iter().any(|n| n == l.name.trim())) {
            if let Some(v) = l.value.as_deref().filter(|v| !v.is_empty()) {
                k.set(&key(&id, l.name.trim()), v).map_err(keychain_problem)?;
            }
        }
    }
    if let Some(old) = old {
        for n in old.env_names.iter().filter(|n| !saved.env_names.contains(n)) {
            k.delete(&env_key(&id, n)).map_err(keychain_problem)?;
        }
        for n in old.header_names.iter().filter(|n| !saved.header_names.contains(n)) {
            k.delete(&header_key(&id, n)).map_err(keychain_problem)?;
        }
        // Another address or command is another server: what List tools found no longer holds.
        if old.url != saved.url || old.command != saved.command || old.args != saved.args || old.transport != saved.transport {
            let _ = core_mcp::change_tool_list(&st.db, &id, |t| *t = Default::default());
        }
    }
    get(st, &id)
}

/// Removes a server: signs it out (revoking where the server offers it), forgets its secrets, and takes it from the list.
pub fn remove(st: &AppState, id: &str) -> Result<(), String> {
    let s = core_mcp::get(&st.db, id).map_err(|e| e.to_string())?;
    if st.tokens.load(id).ok().flatten().is_some() {
        st.tokens.sign_out(id)?;
    }
    for n in &s.env_names {
        st.keychain.delete(&env_key(id, n)).map_err(keychain_problem)?;
    }
    for n in &s.header_names {
        st.keychain.delete(&header_key(id, n)).map_err(keychain_problem)?;
    }
    core_mcp::remove(&st.db, id).map_err(|e| e.to_string())?;
    Ok(())
}

/// What Gizai starts or calls for a server, secrets filled in; a signed-in server gets an access token valid for at least
/// `min_valid` (refreshed when needed).
fn target(st: &AppState, s: &McpServer, min_valid: Duration) -> Result<Target, String> {
    let (env, headers, missing) = secrets(st.keychain.as_ref(), s)?;
    if let Some(n) = missing.first() {
        return Err(format!("{}'s value for {n} isn't in the keychain: enter it again in Settings → MCP servers", s.name));
    }
    let transport = Transport::parse(&s.transport).unwrap_or(Transport::Stdio);
    let home = std::env::var("HOME").unwrap_or_default();
    let t = Target {
        transport, command: gizai_core::clis::expand_home(&s.command, &home), args: s.args.clone(), env, cwd: None,
        url: s.url.clone(), headers,
    };
    if transport != Transport::Stdio && st.tokens.load(&s.id).ok().flatten().is_some() {
        let token = st.tokens.access_token(&s.id, min_valid).map_err(|p| token_problem(st, s, p))?;
        return Ok(with_bearer(t, &token));
    }
    Ok(t)
}

/// The token in a target's `Authorization: Bearer …` header, if it has one.
fn bearer(t: &Target) -> Option<String> {
    t.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("authorization"))
        .and_then(|(_, v)| v.trim().strip_prefix("Bearer ").map(|tok| tok.trim().to_string()))
}

/// The target with `token` as its bearer token.
fn with_bearer(mut t: Target, token: &str) -> Target {
    t.headers.retain(|(n, _)| !n.eq_ignore_ascii_case("authorization"));
    t.headers.push(("Authorization".into(), format!("Bearer {token}")));
    t
}

/// A token that can't be had, in plain words; a refused refresh also marks the server as needing sign-in.
fn token_problem(st: &AppState, s: &McpServer, p: TokenProblem) -> String {
    match p {
        TokenProblem::SignedOut => format!("{}: signed out. Sign in again in Settings → MCP servers.", s.name),
        TokenProblem::SignInAgain(why) => {
            let msg = format!("{}: the sign-in was refused ({why}). Sign in again in Settings → MCP servers.", s.name);
            let _ = st.tokens.forget(&s.id);
            let _ = core_mcp::change_tool_list(&st.db, &s.id, |t| { t.needs_sign_in = true; t.problem = Some(msg.clone()); });
            msg
        }
        TokenProblem::Failed(why) => format!("{}: couldn't refresh its sign-in: {why}", s.name),
    }
}

/// List tools: starts or calls the server, asks for its tools, stops it, and keeps what it found (by the server's version).
pub fn list_tools(st: &AppState, id: &str) -> Result<ServerView, String> {
    let s = core_mcp::get(&st.db, id).map_err(|e| e.to_string())?;
    let t = match target(st, &s, Duration::from_secs(5 * 60)) {
        Ok(t) => t,
        Err(e) => {
            let _ = core_mcp::change_tool_list(&st.db, id, |c| c.problem = Some(e.clone()));
            return Err(e);
        }
    };
    let mut listed = mcp_client::list_tools(&t, LIST_TIMEOUT);
    // A signed-in server refused a token that looked valid (another run may just have renewed it, or the server dropped
    // it): renew it once, then try again. A refused renewal means signing in again.
    if let (Err(ListError::NeedsSignIn { .. }), Some(used)) = (&listed, bearer(&t)) {
        if st.tokens.load(id).ok().flatten().is_some() {
            match st.tokens.renewed_token(id, &used, Duration::from_secs(5 * 60)) {
                Ok(token) => listed = mcp_client::list_tools(&with_bearer(t, &token), LIST_TIMEOUT),
                Err(TokenProblem::Failed(why)) => {
                    let why = format!("{}: couldn't refresh its sign-in: {why}", s.name);
                    let _ = core_mcp::change_tool_list(&st.db, id, |c| c.problem = Some(why.clone()));
                    return Err(why);
                }
                Err(_) => {}
            }
        }
    }
    match listed {
        Ok(l) => {
            core_mcp::set_tool_list(&st.db, id, &core_mcp::ToolList {
                server_name: l.server_name, server_version: l.server_version, listed_at: gizai_core::ids::now_ms(), tools: l.tools,
                needs_sign_in: false, www_authenticate: String::new(), problem: None,
            }).map_err(|e| e.to_string())?;
        }
        Err(ListError::NeedsSignIn { www_authenticate }) => {
            let _ = st.tokens.forget(id);
            core_mcp::change_tool_list(&st.db, id, |c| {
                c.needs_sign_in = true;
                c.www_authenticate = www_authenticate;
                c.problem = Some(format!("{} needs sign-in: Sign in, then List tools again.", s.name));
            }).map_err(|e| e.to_string())?;
        }
        Err(ListError::Failed(why)) => {
            core_mcp::change_tool_list(&st.db, id, |c| c.problem = Some(why)).map_err(|e| e.to_string())?;
        }
    }
    get(st, id)
}

/// Starts a sign-in: finds the server's authorization server, registers Gizai there (or uses the entered client id) and
/// gives the address to open, with the sign-in waiting for its answer.
pub fn begin_sign_in(st: &AppState, id: &str) -> Result<oauth::Pending, String> {
    let s = core_mcp::get(&st.db, id).map_err(|e| e.to_string())?;
    if s.transport == "stdio" {
        return Err(format!("{} is a command: only a server with an address signs in", s.name));
    }
    let www = core_mcp::tool_lists(&st.db).map_err(|e| e.to_string())?.get(id).map(|c| c.www_authenticate.clone()).filter(|w| !w.is_empty());
    let d = oauth::discover(&s.url, www.as_deref())?;
    let client_id = Some(s.client_id.trim()).filter(|c| !c.is_empty());
    oauth::begin(&d, client_id)
}

/// Waits for the browser's answer and keeps the sign-in in the keychain.
pub fn finish_sign_in(st: &AppState, id: &str, pending: oauth::Pending, timeout: Duration) -> Result<ServerView, String> {
    let saved = match oauth::finish(pending, timeout) {
        Ok(s) => s,
        Err(e) => {
            let _ = core_mcp::change_tool_list(&st.db, id, |c| c.problem = Some(e.clone()));
            return Err(e);
        }
    };
    st.tokens.save(id, &saved)?;
    core_mcp::change_tool_list(&st.db, id, |c| { c.needs_sign_in = false; c.problem = None; }).map_err(|e| e.to_string())?;
    get(st, id)
}

/// The whole sign-in: `open` gets the address of the sign-in page (the app opens it in your default browser; a test reads it).
pub fn sign_in(st: &AppState, id: &str, open: impl FnOnce(&str) -> Result<(), String>, timeout: Duration) -> Result<ServerView, String> {
    let pending = begin_sign_in(st, id).inspect_err(|e| { let _ = core_mcp::change_tool_list(&st.db, id, |c| c.problem = Some(e.clone())); })?;
    open(&pending.authorize_url)?;
    finish_sign_in(st, id, pending, timeout)
}

/// Sign out: revokes where the server offers it and forgets the tokens.
pub fn sign_out(st: &AppState, id: &str) -> Result<ServerView, String> {
    st.tokens.sign_out(id)?;
    core_mcp::change_tool_list(&st.db, id, |c| { c.needs_sign_in = true; c.problem = None; }).map_err(|e| e.to_string())?;
    get(st, id)
}

// ---- import from Claude Code ----

/// A server found in a Claude Code's config file, as the import shows it: names of its lines, never their values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// Picks it: `<cli id>|<scope>|<folder>|<name>`.
    pub key: String,
    pub name: String,
    /// The Claude Code it is in (its name in Settings → Coding CLIs).
    pub account: String,
    pub scope: String,
    pub folder: Option<String>,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    pub env_names: Vec<String>,
    pub header_names: Vec<String>,
    /// The same command or address is in the list already.
    pub already: bool,
    /// Why its name can't be used as it is (taken, or Gizai's own): the import asks for another.
    pub clash: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scan {
    pub servers: Vec<Candidate>,
    /// Config files that couldn't be read, in plain words.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pick {
    pub key: String,
    /// The name it gets here (another one when its own clashes).
    pub name: String,
}

/// Each Claude Code in Settings → Coding CLIs with its config file: the built-in one (Gizai's own CLAUDE_CONFIG_DIR, else
/// `~/.claude.json`) and the accounts with their own CLAUDE_CONFIG_DIR.
fn claude_configs(st: &AppState) -> Result<Vec<(gizai_core::clis::Cli, std::path::PathBuf)>, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut out = vec![];
    for cli in gizai_core::clis::list(&st.db).map_err(|e| e.to_string())?.into_iter().filter(|c| c.kind == "claude_code") {
        let dir = gizai_core::clis::env_pairs(&cli, &home).into_iter().find(|(k, _)| k == "CLAUDE_CONFIG_DIR").map(|(_, v)| v)
            .or_else(|| (cli.id == gizai_core::clis::CLAUDE_CODE).then(|| std::env::var("CLAUDE_CONFIG_DIR").ok()).flatten())
            .filter(|d| !d.trim().is_empty())
            .map(|d| std::path::PathBuf::from(gizai_core::clis::expand_home(&d, &home)));
        let file = mcp_import::config_file(dir.as_deref(), std::path::Path::new(&home));
        out.push((cli, file));
    }
    Ok(out)
}

fn found_all(st: &AppState) -> Result<(Vec<(gizai_core::clis::Cli, mcp_import::Found)>, Vec<String>), String> {
    let (mut found, mut problems) = (vec![], vec![]);
    for (cli, file) in claude_configs(st)? {
        match mcp_import::read(&file) {
            Ok(list) => found.extend(list.into_iter().map(|f| (cli.clone(), f))),
            Err(e) => problems.push(format!("{}: {e}", cli.name)),
        }
    }
    Ok((found, problems))
}

fn key_of(cli: &gizai_core::clis::Cli, f: &mcp_import::Found) -> String {
    format!("{}|{}|{}|{}", cli.id, f.scope, f.folder.clone().unwrap_or_default(), f.name)
}

/// Reads (never writes) every Claude Code's config file for its MCP servers. Starts no server.
pub fn scan(st: &AppState) -> Result<Scan, String> {
    let (found, problems) = found_all(st)?;
    let servers = core_mcp::list(&st.db).map_err(|e| e.to_string())?;
    let out = found.iter().map(|(cli, f)| {
        let already = servers.iter().any(|s| s.transport == f.transport
            && if f.transport == "stdio" { s.command == f.command && s.args == f.args } else { s.url == f.url });
        Candidate {
            key: key_of(cli, f), name: f.name.clone(), account: cli.name.clone(), scope: f.scope.clone(), folder: f.folder.clone(),
            transport: f.transport.clone(), command: f.command.clone(), args: f.args.clone(), url: f.url.clone(),
            env_names: f.env.iter().map(|(k, _)| k.clone()).collect(), header_names: f.headers.iter().map(|(k, _)| k.clone()).collect(),
            already, clash: core_mcp::name_problem(&servers, &f.name, ""),
        }
    }).collect();
    Ok(Scan { servers: out, problems })
}

/// Imports the picked servers as normal entries: a copy (later changes in Claude Code don't follow), its values in the
/// keychain. A name that clashes must be changed first.
pub fn import(st: &AppState, picks: Vec<Pick>) -> Result<Vec<ServerView>, String> {
    let (found, _) = found_all(st)?;
    let mut ids = vec![];
    for p in picks {
        let Some((cli, f)) = found.iter().find(|(c, f)| key_of(c, f) == p.key) else {
            return Err("that server isn't in Claude Code's config any more: Import again to see the list as it is now".into());
        };
        let name = if p.name.trim().is_empty() { f.name.clone() } else { p.name.trim().to_string() };
        let source = match &f.folder {
            Some(dir) => format!("{}, {} scope: {dir}", cli.name, f.scope),
            None => format!("{}, {} scope", cli.name, f.scope),
        };
        let input = ServerInput {
            server: McpServer { name, transport: f.transport.clone(), command: f.command.clone(), args: f.args.clone(), url: f.url.clone(), source, ..Default::default() },
            env: f.env.iter().map(|(k, v)| SecretLine { name: k.clone(), value: Some(v.clone()) }).collect(),
            headers: f.headers.iter().map(|(k, v)| SecretLine { name: k.clone(), value: Some(v.clone()) }).collect(),
        };
        ids.push(save(st, input)?.server.id);
    }
    Ok(list(st)?.into_iter().filter(|v| ids.contains(&v.server.id)).collect())
}

// ---- per agent ----

/// A server in the agent form's Tools section.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentServerView {
    pub server_id: String,
    pub name: String,
    pub transport: String,
    pub on: bool,
    pub tools_off: Vec<String>,
    pub sign_in: String,
    /// For a signed-in server: its tools act as you in that service.
    pub acts_as_you: Option<String>,
    /// Its state in the agent's last run (connected, failed, needs-auth), if it was in one.
    pub last_run: Option<LastRun>,
    pub tools: Vec<ToolView>,
    /// One line: how many tools, what they may do, the highest risk.
    pub summary: String,
    pub risk: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMcpView {
    /// Why the agent can't have MCP servers (its CLI isn't Claude Code yet); None = it can.
    pub disabled: Option<String>,
    /// MCP servers on together with npm or npx commands allowed.
    pub warning: Option<String>,
    pub servers: Vec<AgentServerView>,
}

/// The highest risk among the tools and a one-line summary of them.
fn summary(tools: &[ToolView]) -> (String, String) {
    if tools.is_empty() {
        return ("unknown".into(), "Its tools aren't listed yet: List tools in Settings → MCP servers shows them.".into());
    }
    let n = |r: &str| tools.iter().filter(|t| t.risk == r).count();
    let (low, med, high) = (n("low"), n("medium"), n("high"));
    let risk = if high > 0 { "high" } else if med > 0 { "medium" } else { "low" };
    let mut parts = vec![];
    if low > 0 { parts.push(format!("{low} only read")); }
    if med > 0 { parts.push(format!("{med} change things")); }
    if high > 0 { parts.push(format!("{high} may delete or overwrite")); }
    (risk.into(), format!("{} tool{}: {}. {} risk.", tools.len(), if tools.len() == 1 { "" } else { "s" }, parts.join(", "),
                          match risk { "high" => "High", "medium" => "Medium", _ => "Low" }))
}

/// The warning when an MCP server is on and the agent may run npm or npx: a server's answer could try to make it run code.
pub fn npm_warning(allowed_tools: &[String], any_on: bool) -> Option<String> {
    let risky = allowed_tools.iter().map(|t| t.replace(' ', "")).any(|t| t.starts_with("Bash(npm:") || t.starts_with("Bash(npx:") || t == "Bash(npm)" || t == "Bash(npx)");
    (any_on && risky).then(|| "This agent may run npm or npx, and an MCP server is on: a server's answer could try to make it run code. \
        Take npm and npx out of its commands, or switch the server off.".to_string())
}

pub fn agent_view(st: &AppState, agent_id: &str) -> Result<AgentMcpView, String> {
    let agent = gizai_core::team::agent(&st.db, agent_id).map_err(|e| e.to_string())?;
    let kind = crate::clis::of_agent(st, agent.adapter.as_deref()).map(|c| c.kind).unwrap_or_default();
    let disabled = (kind != "claude_code").then(|| "MCP servers work on Claude Code for now: Codex and Gemini come with GA-55.".to_string());
    let last = core_mcp::last_runs(&st.db).map_err(|e| e.to_string())?.remove(agent_id).unwrap_or_default();
    let servers = list(st)?.into_iter().map(|v| {
        let mine = agent.tools.mcp.iter().find(|o| o.server_id == v.server.id);
        let tools = v.listed.map(|l| l.tools).unwrap_or_default();
        let (risk, summary) = summary(&tools);
        AgentServerView {
            server_id: v.server.id.clone(), name: v.server.name.clone(), transport: v.server.transport.clone(),
            on: mine.is_some_and(|m| m.on), tools_off: mine.map(|m| m.tools_off.clone()).unwrap_or_default(),
            acts_as_you: (v.sign_in == "signed_in").then(|| format!("Signed in: its tools act as you in {}.", v.server.name)),
            sign_in: v.sign_in, last_run: last.get(&v.server.name).cloned(), tools, summary, risk,
        }
    }).collect::<Vec<_>>();
    let allowed = if agent.allowed_tools.is_empty() { crate::runs::DEFAULT_TOOLS.iter().map(|s| s.to_string()).collect() } else { agent.allowed_tools.clone() };
    let warning = npm_warning(&allowed, servers.iter().any(|s| s.on));
    Ok(AgentMcpView { disabled, warning, servers })
}

/// Saves the agent's switches (agent form → Tools). Only you: no Team Lead tool calls this.
pub fn save_agent(st: &AppState, agent_id: &str, tools: AgentTools) -> Result<AgentMcpView, String> {
    core_mcp::set_agent_tools(&st.db, &st.you_id, agent_id, tools).map_err(|e| e.to_string())?;
    agent_view(st, agent_id)
}

// ---- runs ----

/// The agent's MCP servers for one Claude Code run or chat turn, and a plain note for each one left out (signed out, a
/// refused refresh, a secret missing from the keychain). A signed-in server's token must last `min_valid` (the run's time
/// cap), else it is refreshed first.
pub fn for_run(st: &AppState, agent: &Member, min_valid: Duration) -> (Vec<RunServer>, Vec<String>) {
    let (mut out, mut notes) = (vec![], vec![]);
    let on: Vec<_> = agent.tools.servers_on().collect();
    if on.is_empty() {
        return (out, notes);
    }
    let servers = core_mcp::list(&st.db).unwrap_or_default();
    let lists = core_mcp::tool_lists(&st.db).unwrap_or_default();
    for a in on {
        let Some(s) = servers.iter().find(|s| s.id == a.server_id) else { continue };
        // A name the list doesn't take (only in a list saved before its rules): its tools could pass for Gizai's own.
        if let Some(why) = core_mcp::name_problem(&[], &s.name, &s.id) {
            notes.push(format!("Left out {}: {why}. Give it another name in Settings → MCP servers.", s.name));
            continue;
        }
        let cached = lists.get(&s.id);
        let signed_in = s.transport != "stdio" && st.tokens.load(&s.id).ok().flatten().is_some();
        if !signed_in && cached.is_some_and(|c| c.needs_sign_in) {
            notes.push(format!("Left out {}: signed out. Sign in again in Settings → MCP servers.", s.name));
            continue;
        }
        let t = match target(st, s, min_valid) {
            Ok(t) => t,
            Err(e) => {
                let prefix = format!("{}: ", s.name);
                notes.push(format!("Left out {}: {}", s.name, e.strip_prefix(&prefix).unwrap_or(&e)));
                continue;
            }
        };
        let entry = match t.transport {
            Transport::Stdio => mcp_run::stdio(&t.command, &t.args, &t.env),
            other => mcp_run::remote(other.key(), &t.url, &t.headers),
        };
        let known_tools = cached.map(|c| c.tools.iter().filter_map(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string)).collect()).unwrap_or_default();
        out.push(RunServer { name: s.name.clone(), entry, tools_off: a.tools_off.clone(), known_tools });
    }
    (out, notes)
}

/// Keeps each server's state from a run's init line on the agent (its state in the last run).
pub fn record_states(st: &AppState, agent_id: &str, states: &[gizai_agents::stream::McpState]) {
    let pairs: Vec<(String, String)> = states.iter().map(|s| (s.name.clone(), s.status.clone())).collect();
    if let Err(e) = core_mcp::set_last_run(&st.db, agent_id, &pairs) {
        eprintln!("gizai: couldn't keep the MCP servers' states of {agent_id}'s run: {e}");
    }
}
