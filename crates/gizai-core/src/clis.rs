//! The coding CLIs agents run on (Settings → Coding CLIs): Claude Code, and the ones you add, such as Codex, Gemini,
//! any other program, or a second account of one with its own environment (CLAUDE_CONFIG_DIR, CODEX_HOME). An agent's
//! `adapter` is the id of its CLI.
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result, ids, settings};

/// The built-in Claude Code, whose program is the `claude_bin` setting. Agents made before CLIs could be picked run on it.
pub const CLAUDE_CODE: &str = "claude_code";
/// The kinds of CLI Gizai knows how to start and read.
pub const KINDS: [&str; 4] = ["claude_code", "codex", "gemini", "other"];
/// Codex's `model_reasoning_effort` levels, lowest first.
pub const CODEX_EFFORTS: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];
/// Codex sandboxes; the first is the default.
pub const CODEX_MODES: [&str; 3] = ["workspace-write", "read-only", "danger-full-access"];
/// Gemini approval modes; the first is the default.
pub const GEMINI_MODES: [&str; 4] = ["auto_edit", "yolo", "plan", "default"];
const KEY: &str = "clis";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Cli {
    /// "claude_code" for the built-in Claude Code; empty for a new one (saving gives it an id).
    pub id: String,
    pub name: String,
    /// claude_code | codex | gemini | other
    pub kind: String,
    /// The program: a path, or a name found in your login shell's PATH. For the built-in Claude Code, empty = found when needed.
    pub command: String,
    /// NAME=value, e.g. CLAUDE_CONFIG_DIR=~/.claude-2; `~` and `$HOME` are expanded when it runs.
    pub env: Vec<String>,
    /// Other only: its arguments. `{prompt}` is the prompt (without it, the prompt goes in on stdin), `{model}` the agent's model.
    pub args: String,
}

/// The built-in Claude Code.
pub fn builtin(db: &Db) -> Cli {
    Cli { command: settings::get::<String>(db, "claude_bin").ok().flatten().unwrap_or_default(), ..claude_code() }
}

/// The built-in Claude Code without its program (the `claude_bin` setting).
pub(crate) fn claude_code() -> Cli {
    Cli { id: CLAUDE_CODE.into(), name: "Claude Code".into(), kind: "claude_code".into(), ..Default::default() }
}

/// Claude Code first, then the CLIs added in Settings.
pub fn list(db: &Db) -> Result<Vec<Cli>> {
    let mut out = vec![builtin(db)];
    out.extend(settings::get::<Vec<Cli>>(db, KEY)?.unwrap_or_default());
    Ok(out)
}

/// The kind of the CLI with this id (empty: the built-in Claude Code), on a connection that is open already (inside a
/// write); None when there is no such CLI.
pub(crate) fn kind_in(c: &rusqlite::Connection, id: &str) -> Result<Option<String>> {
    let id = if id.trim().is_empty() { CLAUDE_CODE } else { id.trim() };
    if id == CLAUDE_CODE {
        return Ok(Some("claude_code".into()));
    }
    let added = settings::get_in::<Vec<Cli>>(c, KEY)?.unwrap_or_default();
    Ok(added.into_iter().find(|x| x.id == id).map(|x| x.kind))
}

/// The CLI with this id; empty means the built-in Claude Code.
pub fn get(db: &Db, id: &str) -> Result<Cli> {
    let id = if id.trim().is_empty() { CLAUDE_CODE } else { id.trim() };
    list(db)?.into_iter().find(|c| c.id == id)
        .ok_or_else(|| Error::Invalid(format!("there is no coding CLI with id {id}: Settings → Coding CLIs lists them")))
}

fn valid_env_name(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn clean(c: Cli) -> Result<Cli> {
    let name = c.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(Error::Invalid("give the CLI a name of at most 60 characters, like Codex or Claude Code (2nd account)".into()));
    }
    let kind = c.kind.trim().to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(Error::Invalid(format!("a CLI is Claude Code, Codex, Gemini or Other, not {kind}")));
    }
    let command = c.command.trim().to_string();
    if command.is_empty() {
        return Err(Error::Invalid(format!("give {name} its program, like codex or /usr/local/bin/codex")));
    }
    let mut env = vec![];
    for line in c.env.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        match line.split_once('=') {
            Some((k, _)) if valid_env_name(k.trim()) => env.push(line.to_string()),
            _ => return Err(Error::Invalid(format!("{name}'s environment takes NAME=value lines, not {line}"))),
        }
    }
    let args = if kind == "other" { c.args.trim().to_string() } else { String::new() };
    let id = if c.id.trim().is_empty() { ids::new_id() } else { c.id.trim().to_string() };
    Ok(Cli { id, name, kind, command, env, args })
}

/// Saves the CLIs added in Settings (the built-in Claude Code is left out: its program is a setting of its own). New ones
/// get an id. Refuses two CLIs with one name, and a list without a CLI that agents still run on. Returns the full list.
pub fn save(db: &Db, clis: Vec<Cli>) -> Result<Vec<Cli>> {
    let out = clean_all(clis)?;
    let old = list(db)?;
    let agents: Vec<(String, String)> = db.read(|c| {
        let mut st = c.prepare("SELECT g.adapter, a.name FROM agent_configs g JOIN actors a ON a.id = g.actor_id WHERE a.deleted_at IS NULL ORDER BY a.name")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    for gone in old.iter().filter(|o| o.id != CLAUDE_CODE && !out.iter().any(|c| c.id == o.id)) {
        let names: Vec<&str> = agents.iter().filter(|(a, _)| *a == gone.id).map(|(_, n)| n.as_str()).collect();
        if !names.is_empty() {
            return Err(Error::Invalid(format!("{} runs on {}: give it another CLI first (its settings → Runs on)", names.join(", "), gone.name)));
        }
    }
    settings::set(db, KEY, &out)?;
    list(db)
}

/// Saves the CLIs added in Settings inside a write that is open already, as `save` does, but without asking which agents
/// run on the ones that go: a new install's first start (`seed::ensure_seed_with_agents`), which has no agents yet.
/// Returns the saved CLIs (without the built-in Claude Code).
pub(crate) fn save_in(w: &crate::db::Writer, clis: Vec<Cli>) -> Result<Vec<Cli>> {
    let out = clean_all(clis)?;
    settings::set_in(w, KEY, &out)?;
    Ok(out)
}

/// The CLIs added in Settings, cleaned, the built-in Claude Code left out; two with one name are refused.
fn clean_all(clis: Vec<Cli>) -> Result<Vec<Cli>> {
    let mut out: Vec<Cli> = vec![];
    for c in clis.into_iter().filter(|c| c.id != CLAUDE_CODE) {
        let c = clean(c)?;
        if c.name.eq_ignore_ascii_case("Claude Code") || out.iter().any(|o| o.name.eq_ignore_ascii_case(&c.name)) {
            return Err(Error::Invalid(format!("there is already a CLI called {}: give each its own name", c.name)));
        }
        if out.iter().any(|o| o.id == c.id) {
            return Err(Error::Invalid(format!("two CLIs have the id {}", c.id)));
        }
        out.push(c);
    }
    Ok(out)
}

/// Why a CLI can't run the Team Lead's chat, or None when it can: chat needs Claude Code's MCP and stream support, so it
/// runs on the Claude Code entries (the built-in one and other accounts of it).
pub fn chat_problem(cli: &Cli) -> Option<String> {
    (cli.kind != "claude_code").then(|| "the chat runs on Claude Code only".to_string())
}

/// The permission modes an agent on this kind of CLI may have, the default first; none for Other.
pub fn permission_modes(kind: &str) -> &'static [&'static str] {
    match kind {
        "codex" => &CODEX_MODES,
        "gemini" => &GEMINI_MODES,
        "other" => &[],
        _ => &crate::team::PERMISSION_MODES,
    }
}

/// The effort levels an agent on this kind of CLI may have; none for Gemini and Other.
pub fn efforts(kind: &str) -> &'static [&'static str] {
    match kind {
        "claude_code" => &crate::team::EFFORTS,
        "codex" => &CODEX_EFFORTS,
        _ => &[],
    }
}

/// Your home folder, as `~` and `$HOME` in the CLIs' settings mean it: $HOME on Linux and macOS (empty when it isn't
/// set), your profile folder (%USERPROFILE%) on Windows.
pub fn home() -> String {
    if cfg!(windows) {
        return std::env::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    }
    std::env::var("HOME").unwrap_or_default()
}

/// `~` and `~/…` at the start (on Windows `~\…` too), and `$HOME` or `${HOME}` anywhere, as `home`.
pub fn expand_home(s: &str, home: &str) -> String {
    let s = s.replace("${HOME}", home).replace("$HOME", home);
    if s == "~" {
        home.to_string()
    } else if let Some(rest) = s.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else if let Some(rest) = s.strip_prefix("~\\").filter(|_| cfg!(windows)) {
        format!("{home}\\{rest}")
    } else {
        s
    }
}

/// A CLI's environment lines as (name, value), with the home folder expanded in each value.
pub fn env_pairs(cli: &Cli, home: &str) -> Vec<(String, String)> {
    cli.env.iter().filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), expand_home(v.trim().trim_matches('"').trim_matches('\''), home)))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}
