//! Teams: members (people and agents with a role), board columns and routing rules.
use crate::db::Db;
use crate::model::{AgentInput, Label, RuleInput};
use crate::{Error, Result, ids, util};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub actor_id: String,
    pub name: String,
    pub kind: String,
    pub role_key: String,
    pub title: Option<String>,
    pub adapter: Option<String>,
    pub instructions_md: Option<String>,
    pub handle: String,
    /// active | paused | archived
    pub status: String,
    pub is_lead: bool,
    // Agent settings (None / empty for people)
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub allowed_tools: Vec<String>,
    pub wakeup: Option<String>,
    pub heartbeat_minutes: Option<i64>,
    pub budget_usd_micros: Option<i64>,
    pub last_heartbeat_at: Option<i64>,
    /// Answers on the Chat page.
    pub chat_enabled: bool,
    /// Claude Code `--effort`; None = Claude Code's default.
    pub effort: Option<String>,
    /// Cards it works on at once.
    pub max_runs: i64,
    /// Folders besides its worktree its file tools may read, or read and change (`folders`).
    pub folders: Vec<crate::folders::Folder>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowState {
    pub id: String,
    pub name: String,
    pub category: String,
    pub owner_role: Option<String>,
    pub wip_limit: Option<i64>,
    pub color: Option<String>,
    pub sort_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRule {
    pub id: String,
    pub kind: String,
    pub match_label_id: Option<String>,
    pub match_state_id: Option<String>,
    pub target_role: Option<String>,
    pub target_actor_id: Option<String>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub name: String,
    pub members: Vec<Member>,
    pub states: Vec<WorkflowState>,
    pub labels: Vec<Label>,
    pub rules: Vec<RoutingRule>,
}

const MEMBER_SELECT: &str = "SELECT a.id, a.name, a.kind, m.role_key, a.title, g.adapter, g.instructions_md, a.handle, a.status, m.is_lead,
        g.model, g.permission_mode, g.allowed_tools_json, g.wakeup, g.heartbeat_minutes, g.budget_usd_micros, g.last_heartbeat_at, m.team_id,
        COALESCE(g.chat_enabled, 0), g.effort, COALESCE(g.max_concurrent_runs, 1), g.folders_json
     FROM team_members m JOIN actors a ON a.id = m.actor_id LEFT JOIN agent_configs g ON g.actor_id = a.id";

fn member_row(r: &rusqlite::Row) -> rusqlite::Result<Member> {
    let tools: Option<String> = r.get(12)?;
    let folders: Option<String> = r.get(21)?;
    Ok(Member { actor_id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, role_key: r.get(3)?, title: r.get(4)?,
                adapter: r.get(5)?, instructions_md: r.get(6)?, handle: r.get(7)?, status: r.get(8)?,
                is_lead: r.get::<_, i64>(9)? != 0, model: r.get(10)?, permission_mode: r.get(11)?,
                allowed_tools: tools.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default(),
                wakeup: r.get(13)?, heartbeat_minutes: r.get(14)?, budget_usd_micros: r.get(15)?, last_heartbeat_at: r.get(16)?,
                chat_enabled: r.get::<_, i64>(18)? != 0, effort: r.get(19)?, max_runs: r.get(20)?,
                folders: folders.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default() })
}

/// Every agent of every team, with its team id (for the heartbeat scheduler).
pub fn all_agents(db: &Db) -> Result<Vec<(String, Member)>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("{MEMBER_SELECT} WHERE a.kind='agent' AND m.deleted_at IS NULL AND a.deleted_at IS NULL ORDER BY m.created_at"))?;
        Ok(st.query_map([], |r| Ok((r.get::<_, String>(17)?, member_row(r)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// The agent that answers on the Chat page, if one has Chat turned on.
pub fn chat_agent(db: &Db) -> Result<Option<Member>> {
    db.read(|c| {
        Ok(c.query_row(&format!("{MEMBER_SELECT} WHERE a.kind='agent' AND g.chat_enabled=1 AND m.deleted_at IS NULL AND a.deleted_at IS NULL
                                 ORDER BY g.updated_at DESC LIMIT 1"), [], member_row).optional()?)
    })
}

/// One agent's settings.
pub fn agent(db: &Db, agent_id: &str) -> Result<Member> {
    db.read(|c| {
        c.query_row(&format!("{MEMBER_SELECT} WHERE a.id=?1 AND a.kind='agent' AND m.deleted_at IS NULL AND a.deleted_at IS NULL LIMIT 1"), [agent_id], member_row)
            .optional()?.ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))
    })
}

pub fn list(db: &Db) -> Result<Vec<TeamSummary>> {
    db.read(|c| {
        let mut st = c.prepare("SELECT id, name FROM teams WHERE deleted_at IS NULL ORDER BY created_at")?;
        Ok(st.query_map([], |r| Ok(TeamSummary { id: r.get(0)?, name: r.get(1)? }))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn get(db: &Db, id: &str) -> Result<Team> {
    db.read(|c| get_in(c, id))
}

pub(crate) fn get_in(c: &Connection, id: &str) -> Result<Team> {
    let name: String = c
        .query_row("SELECT name FROM teams WHERE id=?1 AND deleted_at IS NULL", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("team {id}")))?;
    let mut st = c.prepare(&format!("{MEMBER_SELECT} WHERE m.team_id = ?1 AND m.deleted_at IS NULL AND a.deleted_at IS NULL
         ORDER BY a.kind DESC, a.name COLLATE NOCASE"))?;
    let members = st.query_map([id], member_row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut st = c.prepare(
        "SELECT id, name, category, owner_role, wip_limit, color, sort_key FROM workflow_states
         WHERE team_id = ?1 AND deleted_at IS NULL ORDER BY sort_key",
    )?;
    let states = st
        .query_map([id], |r| {
            Ok(WorkflowState { id: r.get(0)?, name: r.get(1)?, category: r.get(2)?, owner_role: r.get(3)?,
                               wip_limit: r.get(4)?, color: r.get(5)?, sort_key: r.get(6)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut st = c.prepare("SELECT id, name, color FROM labels WHERE deleted_at IS NULL ORDER BY name")?;
    let labels = st
        .query_map([], |r| Ok(Label { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut st = c.prepare(
        "SELECT id, kind, match_label_id, match_state_id, target_role, target_actor_id, priority, enabled FROM routing_rules
         WHERE team_id = ?1 AND deleted_at IS NULL ORDER BY priority, created_at",
    )?;
    let rules = st
        .query_map([id], |r| {
            Ok(RoutingRule { id: r.get(0)?, kind: r.get(1)?, match_label_id: r.get(2)?, match_state_id: r.get(3)?,
                             target_role: r.get(4)?, target_actor_id: r.get(5)?, priority: r.get(6)?,
                             enabled: r.get::<_, i64>(7)? != 0 })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Team { id: id.to_string(), name, members, states, labels, rules })
}

// ---- agents, rules, columns ----

/// The roles the agent form offers (any other key is allowed too).
pub const ROLES: [&str; 6] = ["lead", "frontend", "backend", "design", "qa", "devops"];

/// Claude Code 2.1 permission modes (Codex and Gemini have their own, see `clis::permission_modes`).
pub const PERMISSION_MODES: [&str; 6] = ["acceptEdits", "auto", "bypassPermissions", "manual", "dontAsk", "plan"];
pub const WAKEUPS: [&str; 3] = ["manual", "on_assign", "heartbeat"];
/// Claude Code `--effort` levels, lowest first.
pub const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// "Front End " → "front-end"
pub fn role_key(raw: &str) -> String {
    raw.trim().to_lowercase().split_whitespace().collect::<Vec<_>>().join("-")
}

struct CleanAgent {
    name: String, role: String, title: Option<String>, adapter: String, model: Option<String>, permission_mode: String,
    tools_json: String, wakeup: String, heartbeat_minutes: Option<i64>, budget: Option<i64>, effort: Option<String>,
    /// The kind of its CLI: claude_code, codex, gemini or other.
    kind: String,
    /// Its folders as saved; None: unchanged (none for a new agent).
    folders_json: Option<String>,
}

fn clean_agent(db: &Db, i: &AgentInput) -> Result<CleanAgent> {
    let name = i.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("give the agent a name".into()));
    }
    let role = role_key(&i.role_key);
    if role.is_empty() || role.len() > 32 {
        return Err(Error::Invalid("give the agent a role, like frontend, backend or qa".into()));
    }
    // The CLI it runs on (Settings → Coding CLIs); its kind decides the permission modes and effort levels.
    let cli = crate::clis::get(db, &i.adapter)?;
    let (adapter, kind) = (cli.id, cli.kind);
    let modes = crate::clis::permission_modes(&kind);
    let permission_mode = match (i.permission_mode.trim(), modes.first()) {
        (_, None) => String::new(),
        ("", Some(first)) => first.to_string(),
        (m, Some(_)) => m.to_string(),
    };
    if !modes.is_empty() && !modes.contains(&permission_mode.as_str()) {
        return Err(Error::Invalid(format!("unknown permission mode {permission_mode} for {}: use {}", cli.name, modes.join(", "))));
    }
    if i.chat_enabled == Some(true) && kind != "claude_code" {
        return Err(Error::Invalid(format!("Chat runs on Claude Code: give the agent a Claude Code CLI to turn Chat on, not {}", cli.name)));
    }
    let wakeup = if i.wakeup.trim().is_empty() { "manual".to_string() } else { i.wakeup.trim().to_string() };
    if !WAKEUPS.contains(&wakeup.as_str()) {
        return Err(Error::Invalid(format!("unknown wake-up {wakeup}")));
    }
    if wakeup == "heartbeat" && !matches!(i.heartbeat_minutes, Some(1..=1440)) {
        return Err(Error::Invalid("a heartbeat needs an interval between 1 and 1440 minutes".into()));
    }
    if matches!(i.budget_usd_micros, Some(b) if b < 0) {
        return Err(Error::Invalid("a budget can't be negative".into()));
    }
    let effort = util::clean(&i.effort).map(|e| e.to_lowercase());
    if let Some(e) = &effort {
        let levels = crate::clis::efforts(&kind);
        if levels.is_empty() {
            return Err(Error::Invalid(format!("{} takes no effort level: leave effort empty", cli.name)));
        }
        if !levels.contains(&e.as_str()) {
            return Err(Error::Invalid(format!("effort is {}, not {e}", levels.join(", "))));
        }
    }
    if matches!(i.max_runs, Some(n) if !(1..=10).contains(&n)) {
        return Err(Error::Invalid("an agent works on between 1 and 10 cards at once".into()));
    }
    let tools: Vec<String> = i.allowed_tools.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    let folders_json = match &i.folders {
        Some(list) => Some(serde_json::to_string(&crate::folders::clean(list, &crate::folders::Places::of(db))?)?),
        None => None,
    };
    Ok(CleanAgent {
        name, role, title: util::clean(&i.title), adapter, model: util::clean(&i.model), permission_mode,
        tools_json: serde_json::to_string(&tools)?, wakeup,
        heartbeat_minutes: if i.wakeup.trim() == "heartbeat" { i.heartbeat_minutes } else { i.heartbeat_minutes.filter(|m| *m > 0) },
        budget: i.budget_usd_micros, effort, kind, folders_json,
    })
}

/// Creates the agent (an actor reporting to `actor`), its settings and its team membership.
pub fn add_agent(db: &Db, actor: &str, team_id: &str, input: AgentInput) -> Result<String> {
    let a = clean_agent(db, &input)?;
    let instructions = input.instructions_md.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| crate::seed::role_template(&a.role));
    db.write(Some(actor), |w| {
        let c = w.conn();
        if c.query_row("SELECT count(*) FROM teams WHERE id=?1 AND deleted_at IS NULL", [team_id], |r| r.get::<_, i64>(0))? == 0 {
            return Err(Error::NotFound(format!("team {team_id}")));
        }
        let now = ids::now_ms();
        let id = ids::new_id();
        let handle = util::unique_handle(c, &crate::seed::handle_for(&a.name))?;
        c.execute(
            "INSERT INTO actors(id, created_at, updated_at, created_by, updated_by, org_id, kind, name, handle, title, reports_to_id)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, 'agent', ?5, ?6, ?7, ?3)",
            rusqlite::params![id, now, actor, util::org_id(c)?, a.name, handle, a.title],
        )?;
        c.execute(
            "INSERT INTO agent_configs(actor_id, created_at, updated_at, adapter, model, instructions_md, permission_mode, allowed_tools_json,
                                       wakeup, heartbeat_minutes, budget_usd_micros, effort, max_concurrent_runs, folders_json)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            rusqlite::params![id, now, a.adapter, a.model, instructions, a.permission_mode, a.tools_json, a.wakeup, a.heartbeat_minutes, a.budget, a.effort,
                              input.max_runs.unwrap_or(1), a.folders_json.as_deref().unwrap_or("[]")],
        )?;
        c.execute(
            "INSERT INTO team_members(team_id, actor_id, role_key, is_lead, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![team_id, id, a.role, (a.role == "lead") as i64, now],
        )?;
        w.insert("actors", &id, serde_json::json!({"kind": "agent", "name": a.name, "handle": handle}))?;
        w.insert("agent_configs", &id, serde_json::json!({"adapter": a.adapter, "role": a.role, "wakeup": a.wakeup, "heartbeat_minutes": a.heartbeat_minutes}))?;
        w.insert("team_members", &format!("{team_id}:{id}"), serde_json::json!({"role_key": a.role}))?;
        if input.chat_enabled == Some(true) {
            set_chat_in(w, &id, now)?;
        }
        Ok(id)
    })
}

/// Saves an agent's settings. `instructions_md: None` keeps the current instructions.
pub fn update_agent(db: &Db, actor: &str, actor_id: &str, input: AgentInput) -> Result<()> {
    let a = clean_agent(db, &input)?;
    let instructions = input.instructions_md.clone();
    db.write(Some(actor), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        if a.kind != "claude_code" && input.chat_enabled.is_none() {
            let chat: i64 = c.query_row("SELECT COALESCE(chat_enabled, 0) FROM agent_configs WHERE actor_id=?1", [actor_id], |r| r.get(0))
                .optional()?.unwrap_or(0);
            if chat != 0 {
                return Err(Error::Invalid("Chat runs on Claude Code: turn Chat off for this agent, or keep it on a Claude Code CLI".into()));
            }
        }
        let n = c.execute(
            "UPDATE actors SET name=?2, title=?3, updated_at=?4, updated_by=?5, version=version+1 WHERE id=?1 AND kind='agent' AND deleted_at IS NULL",
            rusqlite::params![actor_id, a.name, a.title, now, actor],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("agent {actor_id}")));
        }
        c.execute(
            "UPDATE agent_configs SET adapter=?2, model=?3, instructions_md=COALESCE(?4, instructions_md), permission_mode=?5, allowed_tools_json=?6,
                    wakeup=?7, heartbeat_minutes=?8, budget_usd_micros=?9, updated_at=?10, effort=?11,
                    max_concurrent_runs=COALESCE(?12, max_concurrent_runs), folders_json=COALESCE(?13, folders_json), version=version+1 WHERE actor_id=?1",
            rusqlite::params![actor_id, a.adapter, a.model, instructions, a.permission_mode, a.tools_json, a.wakeup, a.heartbeat_minutes, a.budget, now, a.effort,
                              input.max_runs, a.folders_json],
        )?;
        c.execute("UPDATE team_members SET role_key=?2, is_lead=?3 WHERE actor_id=?1 AND deleted_at IS NULL",
                  rusqlite::params![actor_id, a.role, (a.role == "lead") as i64])?;
        match input.chat_enabled {
            Some(true) => set_chat_in(w, actor_id, now)?,
            Some(false) => { c.execute("UPDATE agent_configs SET chat_enabled=0 WHERE actor_id=?1", [actor_id])?; }
            None => {}
        }
        w.update("agent_configs", actor_id, serde_json::json!({"name": a.name, "role": a.role, "adapter": a.adapter, "permission_mode": a.permission_mode,
            "wakeup": a.wakeup, "heartbeat_minutes": a.heartbeat_minutes, "model": a.model, "effort": a.effort, "max_runs": input.max_runs, "instructions_changed": instructions.is_some(),
            "folders_changed": a.folders_json.is_some()}))
    })
}

/// Gives `agent_id` the Chat page and takes it from every other agent (there is at most one chat agent).
fn set_chat_in(w: &crate::db::Writer, agent_id: &str, now: i64) -> Result<()> {
    let c = w.conn();
    let others: Vec<String> = {
        let mut st = c.prepare("SELECT actor_id FROM agent_configs WHERE chat_enabled=1 AND actor_id<>?1")?;
        st.query_map([agent_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    c.execute("UPDATE agent_configs SET chat_enabled=0, updated_at=?2 WHERE chat_enabled=1 AND actor_id<>?1", rusqlite::params![agent_id, now])?;
    c.execute("UPDATE agent_configs SET chat_enabled=1, updated_at=?2 WHERE actor_id=?1", rusqlite::params![agent_id, now])?;
    for o in others {
        w.update("agent_configs", &o, serde_json::json!({"chat_enabled": false}))?;
    }
    w.update("agent_configs", agent_id, serde_json::json!({"chat_enabled": true}))
}

pub fn add_rule(db: &Db, actor: &str, team_id: &str, input: RuleInput) -> Result<String> {
    let target = role_key(&input.target_role);
    if target.is_empty() {
        return Err(Error::Invalid("pick the role the rule sends cards to".into()));
    }
    let name = input.match_name.trim().to_string();
    db.write(Some(actor), |w| {
        let c = w.conn();
        let (label, state): (Option<String>, Option<String>) = match input.kind.as_str() {
            "label" => (Some(c.query_row("SELECT id FROM labels WHERE name=?1 COLLATE NOCASE AND deleted_at IS NULL", [&name], |r| r.get(0))
                .optional()?.ok_or_else(|| Error::Invalid(format!("there is no label called {name}")))?), None),
            "column" => {
                let (id, category): (String, String) = c.query_row(
                    "SELECT id, category FROM workflow_states WHERE team_id=?1 AND name=?2 COLLATE NOCASE AND deleted_at IS NULL",
                    rusqlite::params![team_id, name], |r| Ok((r.get(0)?, r.get(1)?)))
                    .optional()?.ok_or_else(|| Error::Invalid(format!("this team has no column called {name}")))?;
                if category == "deploy" {
                    return Err(Error::Invalid(format!("{name} is a Deploy column: no agent starts there by itself. Press Run on a card to start the DevOps Agent")));
                }
                (None, Some(id))
            }
            other => return Err(Error::Invalid(format!("a rule matches a label or a column, not {other}"))),
        };
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute(
            "INSERT INTO routing_rules(id, created_at, updated_at, created_by, updated_by, team_id, kind, match_label_id, match_state_id, target_role, priority)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, now, actor, team_id, input.kind, label, state, target, input.priority],
        )?;
        w.insert("routing_rules", &id, serde_json::json!({"kind": input.kind, "match": name, "target_role": target, "priority": input.priority}))?;
        Ok(id)
    })
}

pub fn delete_rule(db: &Db, actor: &str, rule_id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE routing_rules SET deleted_at=?2, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![rule_id, now, actor],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("rule {rule_id}")));
        }
        w.delete("routing_rules", rule_id)
    })
}

/// Column names are free; the gates key off the column's category, so renaming is always safe.
pub fn rename_state(db: &Db, actor: &str, state_id: &str, name: &str) -> Result<()> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("a column needs a name".into()));
    }
    db.write(Some(actor), |w| {
        let c = w.conn();
        let team: String = c.query_row("SELECT team_id FROM workflow_states WHERE id=?1 AND deleted_at IS NULL", [state_id], |r| r.get(0))
            .optional()?.ok_or_else(|| Error::NotFound(format!("column {state_id}")))?;
        let clash: i64 = c.query_row(
            "SELECT count(*) FROM workflow_states WHERE team_id=?1 AND id<>?2 AND name=?3 COLLATE NOCASE AND deleted_at IS NULL",
            rusqlite::params![team, state_id, name], |r| r.get(0))?;
        if clash > 0 {
            return Err(Error::Invalid(format!("this team already has a column called {name}")));
        }
        c.execute("UPDATE workflow_states SET name=?2, updated_at=?3, updated_by=?4, version=version+1 WHERE id=?1",
                  rusqlite::params![state_id, name, ids::now_ms(), actor])?;
        w.update("workflow_states", state_id, serde_json::json!({"name": name}))
    })
}

/// The column categories, in board order. The gates key off them; `deploy` (merged, not deployed yet) is never routed.
pub const CATEGORIES: [&str; 8] = ["backlog", "ready", "in_progress", "testing", "review", "deploy", "done", "cancelled"];

/// Adds a column to the team's board, right after the column `after_id`. `owner_role`: who works it (None = nobody, a
/// role, or "human" = you); a Deploy column is always worked by you. Names are unique per team. Returns its id.
pub fn add_state(db: &Db, actor: &str, team_id: &str, name: &str, after_id: &str, category: &str, owner_role: Option<&str>) -> Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("a column needs a name".into()));
    }
    if !CATEGORIES.contains(&category) {
        return Err(Error::Invalid(format!("a column's category is one of {}, not {category}", CATEGORIES.join(", "))));
    }
    let owner = if category == "deploy" {
        Some("human".to_string())
    } else {
        owner_role.map(|o| if o.trim() == "human" { "human".to_string() } else { role_key(o) }).filter(|o| !o.is_empty())
    };
    db.write(Some(actor), |w| {
        let c = w.conn();
        let after: Option<String> = c.query_row("SELECT sort_key FROM workflow_states WHERE id=?1 AND team_id=?2 AND deleted_at IS NULL",
                                                rusqlite::params![after_id, team_id], |r| r.get(0)).optional()?;
        let after = after.ok_or_else(|| Error::Invalid("pick the column it goes after".into()))?;
        let clash: i64 = c.query_row("SELECT count(*) FROM workflow_states WHERE team_id=?1 AND name=?2 COLLATE NOCASE AND deleted_at IS NULL",
                                     rusqlite::params![team_id, name], |r| r.get(0))?;
        if clash > 0 {
            return Err(Error::Invalid(format!("this team already has a column called {name}")));
        }
        let next: Option<String> = c.query_row(
            "SELECT min(sort_key) FROM workflow_states WHERE team_id=?1 AND sort_key > ?2 AND deleted_at IS NULL",
            rusqlite::params![team_id, after], |r| r.get(0))?;
        let sort_key = crate::sortkey::key_between(Some(&after), next.as_deref());
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute(
            "INSERT INTO workflow_states(id, created_at, updated_at, created_by, updated_by, team_id, name, category, owner_role, sort_key)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![id, now, actor, team_id, name, category, owner, sort_key],
        )?;
        w.insert("workflow_states", &id, serde_json::json!({"name": name, "category": category, "owner_role": owner}))?;
        Ok(id)
    })
}

/// A new team with the six default columns and `actor` as its reviewer. No agents: Jeffrey adds them.
pub fn add_team(db: &Db, actor: &str, name: &str) -> Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("give the team a name".into()));
    }
    db.write(Some(actor), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute("INSERT INTO teams(id, created_at, updated_at, created_by, updated_by, org_id, name) VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5)",
                  rusqlite::params![id, now, actor, util::org_id(c)?, name])?;
        w.insert("teams", &id, serde_json::json!({"name": name}))?;
        c.execute("INSERT INTO team_members(team_id, actor_id, role_key, created_at) VALUES (?1, ?2, 'reviewer', ?3)",
                  rusqlite::params![id, actor, now])?;
        w.insert("team_members", &format!("{id}:{actor}"), serde_json::json!({"role_key": "reviewer"}))?;
        crate::seed::insert_default_columns(w, &id, now)?;
        Ok(id)
    })
}

/// Pause an agent (no heartbeats, no new runs) or make it active again.
pub fn set_agent_status(db: &Db, actor: &str, agent_id: &str, status: &str) -> Result<()> {
    if !["active", "paused"].contains(&status) {
        return Err(Error::Invalid(format!("an agent is active or paused, not {status}")));
    }
    db.write(Some(actor), |w| {
        let n = w.conn().execute(
            "UPDATE actors SET status=?2, updated_at=?3, updated_by=?4, version=version+1 WHERE id=?1 AND kind='agent' AND deleted_at IS NULL",
            rusqlite::params![agent_id, status, ids::now_ms(), actor])?;
        if n == 0 {
            return Err(Error::NotFound(format!("agent {agent_id}")));
        }
        w.update("actors", agent_id, serde_json::json!({"status": status}))
    })
}

/// Heartbeat bookkeeping: when the agent last woke up.
pub fn touch_heartbeat(db: &Db, agent_id: &str, at: i64) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE agent_configs SET last_heartbeat_at=?2 WHERE actor_id=?1", rusqlite::params![agent_id, at])?;
        Ok(())
    })
}
