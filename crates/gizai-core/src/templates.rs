//! Team templates (GA-226): a team's setup saved as a file, `<team name>.gizai-team.json`, and a new team made from one
//! or from a built-in template. The file is format 1, described field by field in `docs/team-template-format.md`.
//! - Export (`export`, `check_export`, `save`): the team's columns, agents (their settings, instructions and allowed
//!   commands), labels and branches. Never people, an agent's folders, MCP server settings, a CLI's id or account, the
//!   agents' status, cards or spend. `check_export` lists each place that looks private; export never changes them: the
//!   person decides.
//! - Built-in templates (`builtins`, `builtin`): the Software team, made from the seed when it is asked for, so it never
//!   drifts from a new install's first team.
//! - The check (`preview`): reads a template and says what an import would make (`Plan`), or why it is refused. It
//!   writes nothing.
//! - Import (`import`): makes what the plan says in one write, so a failure leaves nothing behind. Existing teams,
//!   agents, labels and the Chat agent stay as they are.
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::clis::{self, Cli};
use crate::db::{Db, Writer};
use crate::mcp_servers::{self, AgentServer, CliTools, McpServer};
use crate::model::AgentInput;
use crate::team::{self, Branch};
use crate::{Error, Result, columns, ids, memory, seed, sortkey, util};

/// A template file's `format`.
pub const FORMAT: &str = "gizai-team";
/// The format this Gizai writes, and the newest it reads.
pub const FORMAT_VERSION: i64 = 1;
/// A template file's name ends in this: `<team name>.gizai-team.json`.
pub const FILE_SUFFIX: &str = ".gizai-team.json";
/// The most agents and columns a template may have.
pub const MAX_AGENTS: usize = 50;
pub const MAX_COLUMNS: usize = 30;
/// The most characters an agent's instructions may have.
pub const MAX_INSTRUCTIONS: usize = 100_000;
/// The biggest template file: 2 MB.
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Why a file of a newer format is refused, word for word.
pub const NEWER: &str = "This template needs a newer version of the app.";
/// The built-in Software team's key.
pub const SOFTWARE: &str = "software";
/// The permission modes that skip every check, which a template never gives: Claude Code's, Codex's and Gemini's. An
/// agent gets its CLI's first mode instead.
pub const NEVER_TAKEN: [&str; 3] = ["bypassPermissions", "danger-full-access", "yolo"];

// ---- the file ----

/// A team template, format 1: the file, field by field (`docs/team-template-format.md`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateFile {
    /// Always `gizai-team`.
    pub format: String,
    /// 1. A file with a higher one is refused (`NEWER`).
    pub format_version: i64,
    /// The Gizai version that wrote it.
    #[serde(default)]
    pub app_version: String,
    /// The team's name.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description_md: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// The organisation chart's branches, in order (left out: the default ones).
    #[serde(default = "team::default_branches")]
    pub branches: Vec<Branch>,
    /// Every label, with its colour (labels belong to the whole organisation).
    #[serde(default)]
    pub labels: Vec<FileLabel>,
    /// The board's columns, in board order.
    #[serde(default)]
    pub columns: Vec<FileColumn>,
    /// The team's agents (archived ones left out).
    #[serde(default)]
    pub agents: Vec<FileAgent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileLabel {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// A column. `next` and `agents` are names within the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileColumn {
    pub name: String,
    /// Its category, one of `team::CATEGORIES`.
    pub kind: String,
    /// Its agents pick up its cards by themselves.
    #[serde(default)]
    pub auto: bool,
    /// The column its cards go to next.
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub wip_limit: Option<i64>,
    #[serde(default)]
    pub color: Option<String>,
    /// The agents on it, in order.
    #[serde(default)]
    pub agents: Vec<String>,
}

/// An agent: its settings, never its folders, its CLI's id or account, its status, memory notes, runs or spend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileAgent {
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub title: Option<String>,
    /// Its CLI's kind: claude_code, codex, gemini or other.
    #[serde(default = "claude_code_kind")]
    pub cli: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub instructions_md: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default = "one")]
    pub cards_at_once: i64,
    #[serde(default, serialize_with = "dollars")]
    pub monthly_budget_usd: Option<f64>,
    /// It answers on the Chat page.
    #[serde(default)]
    pub chat: bool,
    #[serde(default)]
    pub board_check_minutes: Option<i64>,
    #[serde(default = "yes")]
    pub use_memory: bool,
    /// The agent (a name within the file) whose memory folder it shares.
    #[serde(default)]
    pub shares_memory_with: Option<String>,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub web_fetch: bool,
    #[serde(default)]
    pub fetch_domains: Vec<String>,
    #[serde(default)]
    pub builtin_tools: Vec<String>,
    #[serde(default)]
    pub slash_commands: bool,
    /// The built-in browser (Chrome DevTools MCP) is on.
    #[serde(default)]
    pub browser: bool,
    #[serde(default)]
    pub insecure_certs: bool,
    /// Take screenshots of what it built; left out: its role's default.
    #[serde(default)]
    pub screenshots: Option<bool>,
    /// The MCP servers switched on for it, by their names in Settings → MCP, never their settings.
    #[serde(default)]
    pub mcp_servers: Vec<FileServer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileServer {
    pub name: String,
    /// Its tools switched off for the agent.
    #[serde(default)]
    pub tools_off: Vec<String>,
}

fn claude_code_kind() -> String {
    clis::CLAUDE_CODE.into()
}

fn one() -> i64 {
    1
}

fn yes() -> bool {
    true
}

/// A budget in dollars, a whole one without decimals (25, not 25.0).
fn dollars<S: serde::Serializer>(usd: &Option<f64>, s: S) -> std::result::Result<S::Ok, S::Error> {
    match usd {
        Some(d) if d.fract() == 0.0 && d.abs() < 1e15 => s.serialize_i64(*d as i64),
        Some(d) => s.serialize_f64(*d),
        None => s.serialize_none(),
    }
}

// ---- what the commands return ----

/// Where a template comes from: one of a built-in's key, the path of a file the person picked, or the file's text.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Source {
    pub builtin: Option<String>,
    pub path: Option<String>,
    pub json: Option<String>,
}

/// A place in a team's template that looks private, for the person to decide on before sharing the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    /// The agent whose field it is in, by its name in the file; None for the team's own fields.
    pub agent: Option<String>,
    /// The file's field, like instructions_md.
    pub field: String,
    /// email | path | url | secret
    pub kind: String,
    /// What looks private, as the file has it (at most 60 characters).
    pub excerpt: String,
}

/// A built-in template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Builtin {
    pub key: String,
    pub name: String,
    pub description: String,
}

/// What an import makes: the team's name, its columns and agents, the labels it adds, and notes on what it changed from
/// the template.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub team_name: String,
    pub columns: Vec<PlanColumn>,
    pub agents: Vec<PlanAgent>,
    pub labels_added: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanColumn {
    pub name: String,
    pub kind: String,
    pub auto: bool,
    pub next: Option<String>,
    /// The agents on it, by their new names.
    pub agents: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanAgent {
    /// Its name in the new team.
    pub name: String,
    /// Its name in the template.
    pub file_name: String,
    pub role: String,
    /// The CLI it runs on (Settings → Coding CLIs).
    pub cli_name: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub permission_mode: String,
    /// Its allowed commands beyond its role's defaults (`seed::role_tools`).
    pub extra_tools: Vec<String>,
    /// MCP servers the template switches on for it that Settings → MCP doesn't have: they stay off.
    pub missing_servers: Vec<String>,
    /// Active; false: paused (an agent from a file).
    pub active: bool,
}

// ---- export ----

/// The team as a template, format 1: what `save` writes. Archived agents are left out; paused ones go in as they are.
pub fn export(db: &Db, team_id: &str) -> Result<TemplateFile> {
    db.read(|c| export_in(c, team_id))
}

fn export_in(c: &Connection, team_id: &str) -> Result<TemplateFile> {
    let t = team::get_in(c, team_id)?;
    let (description_md, color): (Option<String>, Option<String>) =
        c.query_row("SELECT description_md, color FROM teams WHERE id=?1", [team_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    // The agents in the order they joined the team.
    let joined: Vec<String> = {
        let mut st = c.prepare("SELECT actor_id FROM team_members WHERE team_id=?1 AND deleted_at IS NULL ORDER BY created_at")?;
        st.query_map([team_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut agents: Vec<&team::Member> = t.members.iter().filter(|m| m.kind == "agent" && m.status != "archived").collect();
    agents.sort_by_key(|m| joined.iter().position(|j| *j == m.actor_id).unwrap_or(usize::MAX));
    // The file links agents by name: a name another agent has already gets a number.
    let mut names: Vec<String> = vec![];
    for m in &agents {
        let base = m.name.trim().to_string();
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|x| same(x, &name)) {
            name = format!("{base} {n}");
            n += 1;
        }
        names.push(name);
    }
    let name_of = |id: &str| agents.iter().position(|m| m.actor_id == id).map(|i| names[i].clone());
    let clis = clis::list_in(c)?;
    let servers = mcp_servers::list_in(c)?;
    let file_agents = agents.iter().zip(&names).map(|(m, name)| {
        let adapter = m.adapter.as_deref().map(str::trim).filter(|a| !a.is_empty()).unwrap_or(clis::CLAUDE_CODE);
        let cli = clis.iter().find(|x| x.id == adapter).map(|x| x.kind.clone()).unwrap_or_else(claude_code_kind);
        let mcp = m.tools.servers_on().filter(|s| s.server_id != mcp_servers::BROWSER)
            .filter_map(|s| servers.iter().find(|x| x.id == s.server_id).map(|x| FileServer { name: x.name.clone(), tools_off: s.tools_off.clone() }))
            .collect();
        FileAgent {
            name: name.clone(), role: m.role_key.clone(), title: m.title.clone(), cli, model: m.model.clone(), effort: m.effort.clone(),
            permission_mode: Some(m.permission_mode.clone().unwrap_or_default()), instructions_md: m.instructions_md.clone(),
            allowed_tools: m.allowed_tools.clone(), cards_at_once: m.max_runs, monthly_budget_usd: m.budget_usd_micros.map(|b| b as f64 / 1e6),
            chat: m.chat_enabled, board_check_minutes: m.board_check_minutes, use_memory: m.use_memory,
            shares_memory_with: m.shares_memory_with.as_deref().filter(|o| *o != m.actor_id).and_then(name_of),
            web_search: m.cli_tools.web_search, web_fetch: m.cli_tools.web_fetch, fetch_domains: m.cli_tools.fetch_domains.clone(),
            builtin_tools: m.cli_tools.builtin.clone(), slash_commands: m.cli_tools.slash_commands, browser: m.tools.browser_on(),
            insecure_certs: m.cli_tools.insecure_certs, screenshots: Some(m.cli_tools.screenshots), mcp_servers: mcp,
        }
    }).collect();
    let file_columns = t.states.iter().map(|s| FileColumn {
        name: s.name.clone(), kind: s.category.clone(), auto: s.auto,
        next: s.next_state_id.as_deref().and_then(|n| t.states.iter().find(|x| x.id == n)).map(|x| x.name.clone()),
        wip_limit: s.wip_limit, color: s.color.clone(),
        agents: s.agent_ids.iter().filter_map(|a| name_of(a)).collect(),
    }).collect();
    Ok(TemplateFile {
        format: FORMAT.into(), format_version: FORMAT_VERSION, app_version: env!("CARGO_PKG_VERSION").into(), name: t.name.clone(),
        description_md, color, branches: t.branches.clone(),
        labels: t.labels.iter().map(|l| FileLabel { name: l.name.clone(), color: l.color.clone() }).collect(),
        columns: file_columns, agents: file_agents,
    })
}

/// Export's check: each place in the team's template that looks private (`warnings`). Writes nothing.
pub fn check_export(db: &Db, team_id: &str) -> Result<Vec<Warning>> {
    Ok(warnings(&export(db, team_id)?))
}

/// Writes the team to `path` as a template, format 1: indented JSON in UTF-8. The text is as the team has it: export
/// never hides what `check_export` finds.
pub fn save(db: &Db, team_id: &str, path: &Path) -> Result<()> {
    let mut text = serde_json::to_string_pretty(&export(db, team_id)?)?;
    text.push('\n');
    std::fs::write(path, text).map_err(|e| Error::Invalid(format!("Gizai can't write {}: {e}.", path.display())))
}

/// The places in a template that look private, in the file's order: an email address, a local path, a URL with a query
/// string or with a user and password, and anything that looks like a key or token (`scan`). Each once per field.
pub fn warnings(f: &TemplateFile) -> Vec<Warning> {
    let mut out: Vec<Warning> = vec![];
    let mut add = |agent: Option<&str>, field: &str, text: &str| {
        for (kind, excerpt) in scan(text) {
            let w = Warning { agent: agent.map(String::from), field: field.into(), kind: kind.into(), excerpt };
            if !out.contains(&w) {
                out.push(w);
            }
        }
    };
    add(None, "name", &f.name);
    if let Some(d) = &f.description_md {
        add(None, "description_md", d);
    }
    for b in &f.branches {
        for text in [&b.key, &b.name].into_iter().chain(&b.roles) {
            add(None, "branches", text);
        }
    }
    for l in &f.labels {
        add(None, "labels", &l.name);
    }
    for c in &f.columns {
        add(None, "columns", &c.name);
    }
    for a in &f.agents {
        let who = Some(a.name.as_str());
        add(who, "name", &a.name);
        for (field, text) in [("title", &a.title), ("model", &a.model), ("instructions_md", &a.instructions_md)] {
            if let Some(text) = text {
                add(who, field, text);
            }
        }
        for t in &a.allowed_tools {
            add(who, "allowed_tools", t);
        }
        for d in &a.fetch_domains {
            add(who, "fetch_domains", d);
        }
        for t in &a.builtin_tools {
            add(who, "builtin_tools", t);
        }
        for s in &a.mcp_servers {
            for text in std::iter::once(&s.name).chain(&s.tools_off) {
                add(who, "mcp_servers", text);
            }
        }
    }
    out
}

// ---- what looks private ----

/// What looks private in `text`, in the order it comes, as (kind, excerpt): `email` (an email address), `path` (a local
/// path: /home/…, /Users/…, C:\…, ~/…), `url` (a URL with a query string, or with a user and password) and `secret`
/// (sk-…, ghp_…, github_pat_…, xox…, AKIA…, a private key, or a run of 32 or more hex or base64 characters that looks
/// random). A URL that is flagged holds what is in it: its password or a token in its query isn't flagged again.
pub fn scan(text: &str) -> Vec<(&'static str, String)> {
    let b = text.as_bytes();
    let mut found: Vec<(usize, usize, &'static str)> = vec![];
    let add = |spans: Vec<(usize, usize)>, kind: &'static str, found: &mut Vec<(usize, usize, &'static str)>| {
        for (s, e) in spans {
            if !found.iter().any(|(fs, fe, _)| s < *fe && *fs < e) {
                found.push((s, e, kind));
            }
        }
    };
    let private_urls = urls(b).into_iter().filter(|(s, e)| url_is_private(&text[*s..*e])).collect();
    add(private_urls, "url", &mut found);
    add(tokens(b), "secret", &mut found);
    add(emails(b), "email", &mut found);
    add(paths(b), "path", &mut found);
    add(runs(b), "secret", &mut found);
    found.sort_by_key(|(s, _, _)| *s);
    // Every span starts at an ASCII character and ends after one, so it is cut on a character boundary.
    found.into_iter().map(|(s, e, kind)| (kind, excerpt(&text[s..e]))).collect()
}

/// At most 60 characters of what was found.
fn excerpt(s: &str) -> String {
    if s.chars().count() <= 60 {
        return s.to_string();
    }
    s.chars().take(59).collect::<String>() + "…"
}

fn find(b: &[u8], from: usize, what: &[u8]) -> Option<usize> {
    if from >= b.len() {
        return None;
    }
    b[from..].windows(what.len()).position(|w| w == what).map(|p| from + p)
}

/// Each URL: a scheme (a letter, then letters, digits, `+`, `.` and `-`) and `://`, up to a space, a quote or a bracket,
/// without the punctuation after it.
fn urls(b: &[u8]) -> Vec<(usize, usize)> {
    let scheme = |c: u8| c.is_ascii_alphanumeric() || b"+.-".contains(&c);
    let in_url = |c: u8| c.is_ascii_graphic() && !b"\"'<>`()[]|\\^".contains(&c);
    let mut out = vec![];
    let mut from = 0;
    while let Some(p) = find(b, from, b"://") {
        let mut s = p;
        while s > 0 && scheme(b[s - 1]) {
            s -= 1;
        }
        while s < p && !b[s].is_ascii_alphabetic() {
            s += 1;
        }
        let mut e = p + 3;
        while e < b.len() && in_url(b[e]) {
            e += 1;
        }
        while e > p + 3 && b".,;:!?".contains(&b[e - 1]) {
            e -= 1;
        }
        if s < p && e > p + 3 {
            out.push((s, e));
        }
        from = e.max(p + 3);
    }
    out
}

/// A URL with a query string, or with a user and password (`https://me:secret@host`).
fn url_is_private(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let host = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let password = host.rsplit_once('@').and_then(|(user, _)| user.split_once(':')).is_some_and(|(u, p)| !u.is_empty() && !p.is_empty());
    let query = rest.split('#').next().and_then(|r| r.split_once('?')).is_some_and(|(_, q)| !q.is_empty());
    password || query
}

/// Keys and tokens by how they start: sk-… (and Stripe's sk_live_…), ghp_… and GitHub's other kinds, github_pat_…,
/// glpat-…, xox?-… (Slack), AKIA… (AWS), and a private key's first line.
fn tokens(b: &[u8]) -> Vec<(usize, usize)> {
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'-';
    // The end of a token whose body starts at `at` and takes `ok` characters, when it has at least `min` of them.
    let body = |at: usize, ok: fn(u8) -> bool, min: usize| -> Option<usize> {
        let mut e = at;
        while e < b.len() && ok(b[e]) {
            e += 1;
        }
        (e - at >= min).then_some(e)
    };
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let r = &b[i..];
        let at_start = i == 0 || !word(b[i - 1]);
        let end = if !at_start {
            None
        } else if r.starts_with(b"sk-") {
            body(i + 3, |c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-', 8)
        } else if r.starts_with(b"sk_live_") || r.starts_with(b"sk_test_") || r.starts_with(b"rk_live_") {
            body(i + 8, |c| c.is_ascii_alphanumeric(), 8)
        } else if r.starts_with(b"github_pat_") {
            body(i + 11, |c| c.is_ascii_alphanumeric() || c == b'_', 8)
        } else if r.len() > 4 && r.starts_with(b"gh") && b"pousr".contains(&r[2]) && r[3] == b'_' {
            body(i + 4, |c| c.is_ascii_alphanumeric(), 8)
        } else if r.starts_with(b"glpat-") {
            body(i + 6, |c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-', 8)
        } else if r.len() > 5 && r.starts_with(b"xox") && r[3].is_ascii_lowercase() && r[4] == b'-' {
            body(i + 5, |c| c.is_ascii_alphanumeric() || c == b'-', 8)
        } else if r.starts_with(b"AKIA") {
            body(i + 4, |c| c.is_ascii_uppercase() || c.is_ascii_digit(), 12)
        } else if r.starts_with(b"-----BEGIN ") {
            let line = r.iter().position(|c| *c == b'\n').unwrap_or(r.len());
            find(&r[..line], 0, b"PRIVATE KEY-----").map(|k| i + k + "PRIVATE KEY-----".len())
        } else {
            None
        };
        match end {
            Some(e) => {
                out.push((i, e));
                i = e;
            }
            None => i += 1,
        }
    }
    out
}

/// Email addresses: name@domain.tld, the top-level domain two letters or more.
fn emails(b: &[u8]) -> Vec<(usize, usize)> {
    let local = |c: u8| c.is_ascii_alphanumeric() || b"._%+-".contains(&c);
    let domain = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-';
    let mut out = vec![];
    for at in (0..b.len()).filter(|i| b[*i] == b'@') {
        let mut s = at;
        while s > 0 && local(b[s - 1]) {
            s -= 1;
        }
        while s < at && b[s] == b'.' {
            s += 1;
        }
        let mut e = at + 1;
        while e < b.len() && domain(b[e]) {
            e += 1;
        }
        while e > at + 1 && (b[e - 1] == b'.' || b[e - 1] == b'-') {
            e -= 1;
        }
        let host = &b[at + 1..e];
        let Some(dot) = host.iter().rposition(|c| *c == b'.') else { continue };
        let tld = &host[dot + 1..];
        if s < at && dot > 0 && tld.len() >= 2 && tld.iter().all(u8::is_ascii_alphabetic) {
            out.push((s, e));
        }
    }
    out
}

/// Local paths: /home/…, /Users/…, ~/… and a Windows drive's C:\…, each where a word starts and with a name after it.
fn paths(b: &[u8]) -> Vec<(usize, usize)> {
    let starts = |i: usize| i == 0 || b[i - 1].is_ascii_whitespace() || b"\"'`([{<=,;:|".contains(&b[i - 1]);
    let name = |c: u8| c.is_ascii_alphanumeric() || b"._-$%~{\\".contains(&c);
    let in_path = |c: u8| c.is_ascii_graphic() && !b"\"'`)]}>,;|<".contains(&c);
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let r = &b[i..];
        let prefix = if !starts(i) {
            0
        } else if r.starts_with(b"/home/") {
            6
        } else if r.starts_with(b"/Users/") {
            7
        } else if r.starts_with(b"~/") {
            2
        } else if r.len() > 3 && r[0].is_ascii_alphabetic() && r[1] == b':' && r[2] == b'\\' {
            3
        } else {
            0
        };
        if prefix == 0 || !b.get(i + prefix).is_some_and(|c| name(*c)) {
            i += 1;
            continue;
        }
        let mut e = i + prefix;
        while e < b.len() && in_path(b[e]) {
            e += 1;
        }
        while e > i + prefix && b".:".contains(&b[e - 1]) {
            e -= 1;
        }
        out.push((i, e));
        i = e;
    }
    out
}

/// Runs of 32 or more hex characters (with a digit and a letter), and of base64 characters that look random: with two
/// or more each of capitals, small letters and digits, and changing between those often (a long word, a path or a
/// constant's name rarely does).
fn runs(b: &[u8]) -> Vec<(usize, usize)> {
    let mut out = vec![];
    let each_run = |ok: fn(u8) -> bool, out: &mut Vec<(usize, usize)>, keep: &dyn Fn(&[u8]) -> bool| {
        let mut i = 0;
        while i < b.len() {
            if !ok(b[i]) {
                i += 1;
                continue;
            }
            let s = i;
            while i < b.len() && ok(b[i]) {
                i += 1;
            }
            if keep(&b[s..i]) && !out.iter().any(|(fs, fe)| s < *fe && *fs < i) {
                out.push((s, i));
            }
        }
    };
    each_run(|c| c.is_ascii_hexdigit(), &mut out, &|run| {
        run.len() >= 32 && run.iter().any(u8::is_ascii_digit) && run.iter().any(u8::is_ascii_alphabetic)
    });
    each_run(|c| c.is_ascii_alphanumeric() || b"+/=_-".contains(&c), &mut out, &|run| {
        let classes: Vec<u8> = run.iter().filter_map(|c| match c {
            b'A'..=b'Z' => Some(0),
            b'a'..=b'z' => Some(1),
            b'0'..=b'9' => Some(2),
            _ => None,
        }).collect();
        let count = |k: u8| classes.iter().filter(|c| **c == k).count();
        let changes = classes.windows(2).filter(|w| w[0] != w[1]).count();
        classes.len() >= 32 && count(0) >= 2 && count(1) >= 2 && count(2) >= 2 && changes * 5 >= (classes.len() - 1) * 2
    });
    out
}

// ---- built-in templates ----

/// The built-in templates.
pub fn builtins() -> Vec<Builtin> {
    vec![Builtin {
        key: SOFTWARE.into(),
        name: "Software team".into(),
        description: "The five agents a new install gets, and the seven usual columns.".into(),
    }]
}

/// The built-in template with this key, as a file. The Software team is a new install's first team, made from the seed
/// now: its columns (`seed::DEFAULT_COLUMNS`, each linked to the next but Backlog and Done), labels (`DEFAULT_LABELS`)
/// and agents (`DEFAULT_AGENTS`, with their role's instructions and allowed commands, on Claude Code with acceptEdits),
/// each agent on the columns a new agent of its role lands on (`columns::role_columns`).
pub fn builtin(key: &str) -> Option<TemplateFile> {
    if key != SOFTWARE {
        return None;
    }
    let cols = &seed::DEFAULT_COLUMNS;
    let next: Vec<Option<usize>> = cols.iter().enumerate()
        .map(|(i, (_, category, _))| (*category != "backlog" && *category != "done" && i + 1 < cols.len()).then_some(i + 1))
        .collect();
    let shape: Vec<(&str, Option<usize>)> = cols.iter().zip(&next).map(|((_, category, _), n)| (*category, *n)).collect();
    let mut on: Vec<Vec<String>> = vec![vec![]; cols.len()];
    let agents = seed::DEFAULT_AGENTS.iter().map(|(name, role)| {
        for i in columns::role_columns(&shape, role) {
            on[i].push(name.to_string());
        }
        FileAgent {
            name: name.to_string(), role: role.to_string(), title: None, cli: claude_code_kind(), model: None, effort: None,
            permission_mode: Some("acceptEdits".into()), instructions_md: Some(seed::role_template(role)), allowed_tools: seed::role_tools(role),
            cards_at_once: 1, monthly_budget_usd: None, chat: *role == "lead", board_check_minutes: None, use_memory: true,
            shares_memory_with: None, web_search: false, web_fetch: false, fetch_domains: vec![], builtin_tools: vec![], slash_commands: false,
            browser: false, insecure_certs: false, screenshots: Some(mcp_servers::screenshots_default(role)), mcp_servers: vec![],
        }
    }).collect();
    let columns = cols.iter().enumerate().map(|(i, (name, category, auto))| FileColumn {
        name: name.to_string(), kind: category.to_string(), auto: *auto, next: next[i].map(|n| cols[n].0.to_string()),
        wip_limit: None, color: None, agents: on[i].clone(),
    }).collect();
    Some(TemplateFile {
        format: FORMAT.into(), format_version: FORMAT_VERSION, app_version: env!("CARGO_PKG_VERSION").into(), name: "Software team".into(),
        description_md: None, color: None, branches: team::default_branches(),
        labels: seed::DEFAULT_LABELS.iter().map(|(name, color)| FileLabel { name: name.to_string(), color: Some(color.to_string()) }).collect(),
        columns, agents,
    })
}

// ---- reading and checking a template ----

/// The template a source holds, read and checked (`check`), and whether it is a built-in one. A refusal is an
/// `Error::Invalid` that says why.
fn read_source(source: &Source) -> Result<(TemplateFile, bool)> {
    let given = [source.builtin.is_some(), source.path.is_some(), source.json.is_some()].iter().filter(|g| **g).count();
    if given != 1 {
        return Err(Error::Invalid("Give one template: a built-in one, a file or the file's text.".into()));
    }
    let (file, builtin) = if let Some(key) = &source.builtin {
        let file = builtin(key.trim()).ok_or_else(|| Error::Invalid(format!("There is no built-in team template called {}.", key.trim())))?;
        (file, true)
    } else if let Some(path) = &source.path {
        (parse(&read_file(Path::new(path))?)?, false)
    } else {
        let text = source.json.as_deref().unwrap_or_default();
        if text.len() as u64 > MAX_BYTES {
            return Err(too_big());
        }
        (parse(text)?, false)
    };
    check(&file)?;
    Ok((file, builtin))
}

fn too_big() -> Error {
    Error::Invalid("This file is too big: a team template is at most 2 MB.".into())
}

/// A template file's text: at most 2 MB, in UTF-8.
fn read_file(path: &Path) -> Result<String> {
    let cant = |e: std::io::Error| Error::Invalid(format!("Gizai can't read {}: {e}.", path.display()));
    let file = std::fs::File::open(path).map_err(cant)?;
    if file.metadata().map_err(cant)?.is_dir() {
        return Err(Error::Invalid(format!("Gizai can't read {}: it's a folder.", path.display())));
    }
    let mut bytes = vec![];
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(cant)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(too_big());
    }
    String::from_utf8(bytes).map_err(|_| Error::Invalid("This file isn't JSON: it isn't UTF-8 text.".into()))
}

/// The file's text as a template: JSON, of format gizai-team, version 1.
fn parse(text: &str) -> Result<TemplateFile> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let v: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(format!("This file isn't JSON: {e}.")))?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(Error::Invalid("This file isn't a Gizai team template.".into()));
    }
    match v.get("format_version").and_then(Value::as_f64) {
        Some(n) if n > FORMAT_VERSION as f64 => return Err(Error::Invalid(NEWER.into())),
        Some(n) if n >= 1.0 => {}
        _ => return Err(Error::Invalid("This team template has no format_version: Gizai reads format 1.".into())),
    }
    serde_json::from_value(v).map_err(|e| Error::Invalid(format!("This team template can't be read: {e}.")))
}

/// Names are the same, ignoring case and the spaces around them.
fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// The column's next column, by name; None when it has none.
fn next_of(c: &FileColumn) -> Option<&str> {
    c.next.as_deref().map(str::trim).filter(|n| !n.is_empty())
}

/// The names of the agents on the column (empty ones left out).
fn agents_of(c: &FileColumn) -> impl Iterator<Item = &str> {
    c.agents.iter().map(|a| a.trim()).filter(|a| !a.is_empty())
}

/// 120345 → "120,345".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    out
}

/// Refuses a template a team can't be made from, saying why: no columns, more than 50 agents or 30 columns,
/// instructions over 100,000 characters, an unknown kind, two columns or agents with one name (ignoring case), a next
/// column or a column's agent that isn't in it, agents or Auto on a backlog, review, done or cancelled column, and an Auto
/// column without a next one.
fn check(f: &TemplateFile) -> Result<()> {
    let no = |why: String| Err(Error::Invalid(why));
    if f.name.trim().is_empty() {
        return no("This team template has no name.".into());
    }
    if f.columns.is_empty() {
        return no("This team template has no columns: a team needs at least one.".into());
    }
    if f.agents.len() > MAX_AGENTS {
        return no(format!("This team template has {} agents: a template has at most {MAX_AGENTS}.", f.agents.len()));
    }
    if f.columns.len() > MAX_COLUMNS {
        return no(format!("This team template has {} columns: a template has at most {MAX_COLUMNS}.", f.columns.len()));
    }
    for a in &f.agents {
        let n = a.instructions_md.as_deref().map_or(0, |t| t.chars().count());
        if n > MAX_INSTRUCTIONS {
            return no(format!("{}'s instructions are {} characters: an agent's instructions are at most {}.", a.name.trim(), thousands(n),
                              thousands(MAX_INSTRUCTIONS)));
        }
    }
    for (i, c) in f.columns.iter().enumerate() {
        if c.name.trim().is_empty() {
            return no("A column in this team template has no name.".into());
        }
        let kind = c.kind.trim().to_lowercase();
        if !team::CATEGORIES.contains(&kind.as_str()) {
            return no(format!("The column {} has an unknown kind, {}: a column's kind is {}.", c.name.trim(), c.kind.trim(),
                              team::CATEGORIES.join(", ")));
        }
        if f.columns[..i].iter().any(|o| same(&o.name, &c.name)) {
            return no(format!("This team template has two columns called {}.", c.name.trim()));
        }
    }
    for (i, a) in f.agents.iter().enumerate() {
        if a.name.trim().is_empty() {
            return no("An agent in this team template has no name.".into());
        }
        if f.agents[..i].iter().any(|o| same(&o.name, &a.name)) {
            return no(format!("This team template has two agents called {}.", a.name.trim()));
        }
        let role = team::role_key(&a.role);
        if role.is_empty() || role.len() > 32 {
            return no(format!("{} has no role: give it one of at most 32 characters, like backend or qa.", a.name.trim()));
        }
    }
    for c in &f.columns {
        let name = c.name.trim();
        if let Some(n) = next_of(c) {
            match f.columns.iter().find(|o| same(&o.name, n)) {
                None => return no(format!("{name} goes next to {n}, which isn't in this team template.")),
                Some(o) if same(&o.name, name) => return no(format!("{name} goes next to itself: its cards need another column to go to.")),
                Some(_) => {}
            }
        }
        if let Some(a) = agents_of(c).find(|a| !f.agents.iter().any(|x| same(&x.name, a))) {
            return no(format!("{a} is on {name}, but isn't in this team template."));
        }
        let kind = c.kind.trim().to_lowercase();
        if !columns::takes_agents(&kind) {
            if agents_of(c).next().is_some() {
                return no(format!("{name} is a {kind} column, which takes no agents."));
            }
            if c.auto {
                return no(format!("{name} is a {kind} column, which can't be Auto."));
            }
        }
        if c.auto && next_of(c).is_none() {
            return no(format!("{name} is Auto without a next column: an Auto column needs one, so a finished card isn't picked up again."));
        }
    }
    Ok(())
}

// ---- the plan ----

/// What is there already, for a plan.
struct Here {
    teams: Vec<String>,
    /// Every agent's memory folder name, in small letters: each new agent gets a folder of its own.
    agents: HashSet<String>,
    /// Settings → Coding CLIs, the built-in Claude Code first.
    clis: Vec<Cli>,
    servers: Vec<McpServer>,
    /// The name of the agent with Chat, when one has it.
    chat: Option<String>,
    /// The labels' names, in small letters.
    labels: HashSet<String>,
}

fn here_in(c: &Connection) -> Result<Here> {
    let names = |sql: &str| -> Result<Vec<String>> {
        let mut st = c.prepare(sql)?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?)
    };
    Ok(Here {
        teams: names("SELECT name FROM teams WHERE deleted_at IS NULL")?,
        agents: names("SELECT name FROM actors WHERE kind='agent' AND deleted_at IS NULL")?.iter().map(|n| folder_key(n)).collect(),
        clis: clis::list_in(c)?,
        servers: mcp_servers::list_in(c)?,
        chat: team::chat_agent_in(c)?.map(|m| m.name),
        labels: names("SELECT name FROM labels WHERE deleted_at IS NULL")?.iter().map(|n| n.trim().to_lowercase()).collect(),
    })
}

/// An agent's memory folder name, in small letters (`memory::folder_name`).
fn folder_key(name: &str) -> String {
    memory::folder_name(name).to_lowercase()
}

/// The plan's notes, each once, in order.
#[derive(Default)]
struct Notes(Vec<String>);

impl Notes {
    fn push(&mut self, note: String) {
        if !self.0.contains(&note) {
            self.0.push(note);
        }
    }
}

/// A plan, with what the import makes from it.
struct Made {
    plan: Plan,
    template: String,
    description_md: Option<String>,
    color: Option<String>,
    /// None: the default branches (`team::default_branches`), as a new team has them.
    branches: Option<Vec<Branch>>,
    columns: Vec<MadeColumn>,
    labels: Vec<(String, Option<String>)>,
    agents: Vec<MadeAgent>,
}

struct MadeColumn {
    name: String,
    kind: String,
    auto: bool,
    next: Option<usize>,
    wip_limit: Option<i64>,
    color: Option<String>,
    agents: Vec<usize>,
}

struct MadeAgent {
    cli: Cli,
    input: AgentInput,
    /// Its `mcp_extra_json`: its MCP servers, its CLI's own tools and its Take screenshots; None: nothing on, its role's
    /// screenshots.
    extra: Option<String>,
    /// The agent whose memory folder it shares.
    shares: Option<usize>,
}

/// The team's name: the template's, with a number when a team has it already ("Software team 2").
fn unique_team_name(name: &str, teams: &[String]) -> String {
    let taken = |n: &str| teams.iter().any(|t| same(t, n));
    if !taken(name) {
        return name.to_string();
    }
    (2..).map(|n| format!("{name} {n}")).find(|n| !taken(n)).unwrap_or_default()
}

/// The agents' new names: the template's, except where an agent has one already (ignoring case, or another agent's
/// memory folder would have its name): then the team's name in brackets, "Backend Agent (Software team)", with a number
/// when that is taken too.
fn agent_names(f: &TemplateFile, team_name: &str, taken: &HashSet<String>) -> Vec<String> {
    let mut used = taken.clone();
    let mut names: Vec<Option<String>> = f.agents.iter().map(|a| used.insert(folder_key(&a.name)).then(|| a.name.trim().to_string())).collect();
    for (i, a) in f.agents.iter().enumerate() {
        if names[i].is_some() {
            continue;
        }
        let base = format!("{} ({team_name})", a.name.trim());
        let mut name = base.clone();
        let mut n = 2;
        while !used.insert(folder_key(&name)) {
            name = format!("{base} {n}");
            n += 1;
        }
        names[i] = Some(name);
    }
    names.into_iter().map(Option::unwrap_or_default).collect()
}

/// A CLI kind as Settings → Coding CLIs names it.
fn kind_name(kind: &str) -> &str {
    match kind {
        "claude_code" => "Claude Code",
        "codex" => "Codex",
        "gemini" => "Gemini",
        "other" => "Other",
        k => k,
    }
}

/// The agent's permission mode on `cli`: the template's when the CLI offers it, else the CLI's first mode, noted. The
/// modes that skip every check (`NEVER_TAKEN`) are never taken.
fn pick_mode(wanted: &str, cli: &Cli, name: &str, notes: &mut Notes) -> String {
    let modes = clis::permission_modes(&cli.kind);
    let Some(first) = modes.first() else {
        if !wanted.is_empty() {
            notes.push(format!("{} takes no permission mode: {name} runs with its own settings.", cli.name));
        }
        return String::new();
    };
    if wanted.is_empty() {
        return first.to_string();
    }
    if NEVER_TAKEN.iter().any(|m| m.eq_ignore_ascii_case(wanted)) {
        notes.push(format!("{wanted} is never taken from a template: {name} runs with {first}."));
        return first.to_string();
    }
    match modes.iter().find(|m| m.eq_ignore_ascii_case(wanted)) {
        Some(m) => m.to_string(),
        None => {
            notes.push(format!("{} doesn't offer the permission mode {wanted}: {name} runs with {first}.", cli.name));
            first.to_string()
        }
    }
}

/// Each once, trimmed, empty ones left out, in order.
fn tidy(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for t in list.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        if !out.iter().any(|o| o == t) {
            out.push(t.to_string());
        }
    }
    out
}

/// What importing template `f` (checked) makes, with `here` as what is there already. `name`: the new team's name (None:
/// the template's, with a number when it is taken). `builtin`: its agents come in active, else paused.
fn plan(f: &TemplateFile, builtin: bool, name: Option<&str>, here: &Here) -> Result<Made> {
    let mut notes = Notes::default();
    let team_name = match name {
        Some(n) => n.trim().to_string(),
        None => unique_team_name(f.name.trim(), &here.teams),
    };
    let names = agent_names(f, &team_name, &here.agents);
    let agent_index = |n: &str| f.agents.iter().position(|a| same(&a.name, n));
    let column_index = |n: &str| f.columns.iter().position(|c| same(&c.name, n));

    let mut agents: Vec<MadeAgent> = vec![];
    let mut planned: Vec<PlanAgent> = vec![];
    let mut chat_given = false;
    for (a, name) in f.agents.iter().zip(&names) {
        let role = team::role_key(&a.role);
        let kind = match a.cli.trim().to_lowercase() {
            k if k.is_empty() => claude_code_kind(),
            k => k,
        };
        // The first CLI of its kind, else the built-in Claude Code with acceptEdits and no model or effort.
        let (cli, fallback) = match here.clis.iter().find(|c| c.kind == kind) {
            Some(c) => (c.clone(), false),
            None => {
                notes.push(format!("No {} CLI is set up: {name} runs on Claude Code.", kind_name(&kind)));
                (clis::claude_code(), true)
            }
        };
        let permission_mode = if fallback {
            "acceptEdits".to_string()
        } else {
            pick_mode(a.permission_mode.as_deref().unwrap_or_default().trim(), &cli, name, &mut notes)
        };
        let model = if fallback { None } else { util::clean(&a.model) };
        let effort = if fallback {
            None
        } else {
            util::clean(&a.effort).map(|e| e.to_lowercase()).filter(|e| clis::efforts(&cli.kind).contains(&e.as_str()))
        };
        let tools = tidy(&a.allowed_tools);
        let defaults = seed::role_tools(&role);
        let extra_tools: Vec<String> = tools.iter().filter(|t| !defaults.contains(t)).cloned().collect();

        // MCP servers by name (the built-in browser by its switch), only on Claude Code.
        let mut servers: Vec<AgentServer> = vec![];
        let mut missing: Vec<String> = vec![];
        for s in &a.mcp_servers {
            let n = s.name.trim();
            if n.is_empty() {
                continue;
            }
            let id = if n.eq_ignore_ascii_case(mcp_servers::BROWSER) {
                Some(mcp_servers::BROWSER.to_string())
            } else {
                here.servers.iter().find(|x| x.name.eq_ignore_ascii_case(n)).map(|x| x.id.clone())
            };
            match id {
                Some(id) if !servers.iter().any(|o| o.server_id == id) => {
                    let mut off = tidy(&s.tools_off);
                    off.sort();
                    servers.push(AgentServer { server_id: id, on: true, tools_off: off });
                }
                Some(_) => {}
                None if !missing.iter().any(|m| m.eq_ignore_ascii_case(n)) => missing.push(n.to_string()),
                None => {}
            }
        }
        if a.browser && !servers.iter().any(|o| o.server_id == mcp_servers::BROWSER) {
            servers.push(AgentServer { server_id: mcp_servers::BROWSER.into(), on: true, tools_off: vec![] });
        }
        if !servers.is_empty() && mcp_servers::mcp_not_on(&cli.kind).is_some() {
            notes.push(format!("MCP servers and the browser work on Claude Code for now: {name} runs on {}, so they stay off.", cli.name));
            servers.clear();
        }

        // Chat: at most one agent has it, on Claude Code, and the Team Lead you have keeps it.
        let mut chat = a.chat;
        let mut board_check = a.board_check_minutes.filter(|m| *m != 0);
        if chat {
            let why = if let Some(lead) = &here.chat {
                Some(format!("Your Team Lead stays {lead}."))
            } else if chat_given {
                Some(format!("Only one agent has Chat: {name} comes without it."))
            } else if cli.kind != "claude_code" {
                Some(format!("Chat runs on Claude Code: {name} comes without it."))
            } else {
                None
            };
            match why {
                Some(why) => {
                    notes.push(why);
                    chat = false;
                    board_check = None;
                }
                None => chat_given = true,
            }
        }
        if let Some(m) = board_check.filter(|m| !(5..=1440).contains(m)) {
            notes.push(format!("{name}'s board check every {m} minutes isn't between 5 and 1440 minutes: it's off."));
            board_check = None;
        }
        let max_runs = a.cards_at_once.clamp(1, 10);
        if max_runs != a.cards_at_once {
            notes.push(format!("An agent works on 1 to 10 cards at once: {name} works on {max_runs}."));
        }
        let budget = match a.monthly_budget_usd {
            Some(d) if d.is_finite() && d >= 0.0 => Some((d * 1e6).round() as i64),
            Some(_) => {
                notes.push(format!("{name}'s budget can't be negative: it has none."));
                None
            }
            None => None,
        };

        // Its CLI's own tools. A domain to fetch from that can't be used is left out; with none left, it fetches no pages.
        let mut domains: Vec<String> = vec![];
        for raw in a.fetch_domains.iter().filter(|d| !d.trim().is_empty()) {
            match mcp_servers::clean_domain(raw) {
                Some(d) if !domains.contains(&d) => domains.push(d),
                Some(_) => {}
                None => notes.push(format!("{} isn't a domain: {name} doesn't fetch from it.", raw.trim())),
            }
        }
        let mut web_fetch = a.web_fetch;
        if web_fetch && domains.is_empty() && a.fetch_domains.iter().any(|d| !d.trim().is_empty()) {
            notes.push(format!("{name} fetches web pages only from domains Gizai can't use: it fetches none."));
            web_fetch = false;
        }
        let mut builtin_tools = vec![];
        for t in tidy(&a.builtin_tools) {
            if mcp_servers::valid_tool_name(&t) {
                builtin_tools.push(t);
            } else {
                notes.push(format!("{t} isn't the name of a CLI's own tool: {name} comes without it."));
            }
        }
        builtin_tools.sort();
        let cli_tools = CliTools {
            web_search: a.web_search, web_fetch, fetch_domains: domains, insecure_certs: a.insecure_certs, builtin: builtin_tools,
            slash_commands: a.slash_commands,
        };
        let screenshots = a.screenshots.unwrap_or_else(|| mcp_servers::screenshots_default(&role));
        let mut extra = serde_json::Map::new();
        if !servers.is_empty() {
            extra.insert("mcp".into(), serde_json::to_value(&servers)?);
        }
        if cli_tools != CliTools::default() {
            extra.insert("cli".into(), serde_json::to_value(&cli_tools)?);
        }
        if screenshots != mcp_servers::screenshots_default(&role) {
            extra.insert("screenshots".into(), Value::Bool(screenshots));
        }

        planned.push(PlanAgent {
            name: name.clone(), file_name: a.name.trim().to_string(), role: role.clone(), cli_name: cli.name.clone(), model: model.clone(),
            effort: effort.clone(), permission_mode: permission_mode.clone(), extra_tools, missing_servers: missing, active: builtin,
        });
        agents.push(MadeAgent {
            input: AgentInput {
                name: name.clone(), role_key: role, title: util::clean(&a.title), adapter: cli.id.clone(), model, instructions_md: a.instructions_md.clone(),
                permission_mode, allowed_tools: tools, budget_usd_micros: budget, chat_enabled: chat.then_some(true), effort, max_runs: Some(max_runs),
                board_check_minutes: board_check, use_memory: Some(a.use_memory), ..Default::default()
            },
            cli,
            extra: (!extra.is_empty()).then(|| Value::Object(extra).to_string()),
            shares: None,
        });
    }

    // Memory folders: an agent shares the folder of the agent it names, as long as both are in the template and neither is
    // a Team Lead (which keeps its notes in Team Lead/); a chain ends at its first agent with a folder of its own.
    let lead: Vec<bool> = agents.iter().map(|a| a.input.role_key == "lead" || a.input.chat_enabled == Some(true)).collect();
    let mut shares: Vec<Option<usize>> = vec![None; f.agents.len()];
    for (i, a) in f.agents.iter().enumerate() {
        let Some(target) = a.shares_memory_with.as_deref().map(str::trim).filter(|s| !s.is_empty()) else { continue };
        match agent_index(target) {
            None => notes.push(format!("{target} isn't in this team template: {} gets its own memory folder.", names[i])),
            Some(j) if j == i => {}
            Some(_) if lead[i] => notes.push(format!("{} is a Team Lead, which keeps its notes in Team Lead/: it shares no memory folder.", names[i])),
            Some(j) if lead[j] => notes.push(format!("{} is a Team Lead, which keeps its notes in Team Lead/: {} gets its own memory folder.",
                                                     names[j], names[i])),
            Some(j) => shares[i] = Some(j),
        }
    }
    for i in 0..shares.len() {
        let mut seen = vec![i];
        while let Some(j) = shares[i] {
            match shares[j] {
                None => break,
                Some(k) if seen.contains(&k) => {
                    notes.push(format!("{}'s Shares memory with goes round in a circle: it gets its own memory folder.", names[i]));
                    shares[i] = None;
                }
                Some(k) => {
                    seen.push(j);
                    shares[i] = Some(k);
                }
            }
        }
    }
    for (a, s) in agents.iter_mut().zip(shares) {
        a.shares = s;
    }

    let columns: Vec<MadeColumn> = f.columns.iter().map(|c| {
        let mut on: Vec<usize> = vec![];
        for j in agents_of(c).filter_map(agent_index) {
            if !on.contains(&j) {
                on.push(j);
            }
        }
        MadeColumn {
            name: c.name.trim().to_string(), kind: c.kind.trim().to_lowercase(), auto: c.auto, next: next_of(c).and_then(column_index),
            wip_limit: c.wip_limit.filter(|w| *w > 0), color: util::clean(&c.color), agents: on,
        }
    }).collect();

    // Labels that aren't there yet (ignoring case); the ones there keep their colour.
    let mut have = here.labels.clone();
    let mut labels: Vec<(String, Option<String>)> = vec![];
    for l in &f.labels {
        let n = l.name.trim();
        if n.is_empty() || have.contains(&n.to_lowercase()) {
            continue;
        }
        if n.chars().count() > 40 {
            notes.push(format!("The label {n} is longer than 40 characters: it isn't added."));
            continue;
        }
        let color = crate::labels::clean_color(l.color.as_deref()).unwrap_or_else(|_| {
            notes.push(format!("The label {n} has no hex colour like #7b9bff: it's added without one."));
            None
        });
        have.insert(n.to_lowercase());
        labels.push((n.to_string(), color));
    }

    // Branches as the organisation chart keeps them: each with a key, a name and its roles' keys.
    let mut branches: Vec<Branch> = vec![];
    for b in &f.branches {
        let (key, name) = (team::role_key(&b.key), b.name.trim().to_string());
        if key.is_empty() || name.is_empty() || branches.iter().any(|o| o.key == key) {
            continue;
        }
        branches.push(Branch { key, name, roles: tidy(&b.roles.iter().map(|r| team::role_key(r)).collect::<Vec<_>>()) });
    }

    let plan = Plan {
        team_name,
        columns: columns.iter().map(|c| PlanColumn {
            name: c.name.clone(), kind: c.kind.clone(), auto: c.auto, next: c.next.map(|n| columns[n].name.clone()),
            agents: c.agents.iter().map(|j| names[*j].clone()).collect(),
        }).collect(),
        agents: planned,
        labels_added: labels.iter().map(|(n, _)| n.clone()).collect(),
        notes: notes.0,
    };
    Ok(Made {
        plan, template: f.name.trim().to_string(), description_md: util::clean(&f.description_md), color: util::clean(&f.color),
        branches: (branches != team::default_branches()).then_some(branches), columns, labels, agents,
    })
}

/// The check: what importing the template makes, as a plan, or why it is refused (an `Error::Invalid` that says why).
/// Writes nothing.
pub fn preview(db: &Db, source: &Source) -> Result<Plan> {
    let (file, builtin) = read_source(source)?;
    db.read(|c| Ok(plan(&file, builtin, None, &here_in(c)?)?.plan))
}

// ---- import ----

/// Makes a new team called `name` from the template, as `preview` plans it, with `actor` (you) as its reviewer, and
/// returns its id. One write: an import that fails leaves nothing behind. The agents from a file come in paused, a
/// built-in's active; nothing starts by itself (the team has no cards yet).
pub fn import(db: &Db, actor: &str, source: &Source, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("give the team a name".into()));
    }
    let (file, builtin) = read_source(source)?;
    db.write(Some(actor), |w| {
        let made = plan(&file, builtin, Some(name), &here_in(w.conn())?)?;
        make_in(w, actor, &made)
    })
}

/// Makes the plan's team inside the import's write: the team (as `team::add_team` does, with its description, colour and
/// branches), its columns in order, the labels it adds, its agents (as `team::add_agent` makes them, on the template's
/// columns) and their shared memory folders.
fn make_in(w: &Writer, actor: &str, m: &Made) -> Result<String> {
    let c = w.conn();
    let now = ids::now_ms();
    let org = util::org_id(c)?;
    let team_id = ids::new_id();
    let branches = m.branches.as_ref().map(serde_json::to_string).transpose()?;
    c.execute("INSERT INTO teams(id, created_at, updated_at, created_by, updated_by, org_id, name, description_md, color, branches_json)
               VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8)",
              rusqlite::params![team_id, now, actor, org, m.plan.team_name, m.description_md, m.color, branches])?;
    w.insert("teams", &team_id, json!({"name": m.plan.team_name, "template": m.template}))?;
    c.execute("INSERT INTO team_members(team_id, actor_id, role_key, created_at) VALUES (?1, ?2, 'reviewer', ?3)",
              rusqlite::params![team_id, actor, now])?;
    w.insert("team_members", &format!("{team_id}:{actor}"), json!({"role_key": "reviewer"}))?;

    // The columns in order, then their next columns, once all of them are there.
    let column_ids: Vec<String> = m.columns.iter().map(|_| ids::new_id()).collect();
    let mut key: Option<String> = None;
    for (col, id) in m.columns.iter().zip(&column_ids) {
        let k = sortkey::key_after(key.as_deref());
        c.execute("INSERT INTO workflow_states(id, created_at, updated_at, created_by, updated_by, team_id, name, category, sort_key, auto, wip_limit, color)
                   VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                  rusqlite::params![id, now, actor, team_id, col.name, col.kind, k, col.auto as i64, col.wip_limit, col.color])?;
        w.insert("workflow_states", id, json!({"name": col.name, "category": col.kind, "auto": col.auto}))?;
        key = Some(k);
    }
    for (col, id) in m.columns.iter().zip(&column_ids) {
        if let Some(n) = col.next {
            c.execute("UPDATE workflow_states SET next_state_id=?2 WHERE id=?1", rusqlite::params![id, column_ids[n]])?;
        }
    }

    for (name, color) in &m.labels {
        let id = ids::new_id();
        c.execute("INSERT INTO labels(id, created_at, updated_at, created_by, updated_by, org_id, name, color) VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6)",
                  rusqlite::params![id, now, actor, org, name, color])?;
        w.insert("labels", &id, json!({"name": name, "color": color}))?;
    }

    // The agents, then the folders they share (each one's owner is there by then).
    let mut agent_ids: Vec<String> = vec![];
    for (a, p) in m.agents.iter().zip(&m.plan.agents) {
        agent_ids.push(team::add_template_agent_in(w, actor, &team_id, a.cli.clone(), &a.input, !p.active, a.extra.as_deref(), a.shares.is_none())?);
    }
    for (a, id) in m.agents.iter().zip(&agent_ids) {
        if let Some(owner) = a.shares {
            memory::set_shares_in(w, actor, id, Some(agent_ids[owner].as_str()))?;
        }
    }
    for (col, id) in m.columns.iter().zip(&column_ids) {
        if !col.agents.is_empty() {
            let list: Vec<String> = col.agents.iter().map(|j| agent_ids[*j].clone()).collect();
            columns::write_agents(w, id, &list)?;
            w.update("workflow_states", id, json!({"agents": list}))?;
        }
    }
    Ok(team_id)
}
