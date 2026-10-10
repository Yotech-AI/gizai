//! Tools that only look: compact JSON the model can scan, never whole tables of text.
use std::collections::BTreeMap;

use gizai_core::model::{FileRow, Task, TaskFilter};
use gizai_core::{clients, comments, docs, files, ids, projects, runs, tasks, team, users};
use serde_json::{Value, json};

use super::{Args, Cx, err, resolve, ymd};

fn task_line(t: &Task) -> Value {
    let mut v = json!({
        "task": t.identifier, "title": t.title, "project": t.project_name, "column": t.state_name, "priority": t.priority,
        "assignee": t.assignee_name, "labels": t.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), "hold": t.hold,
        "testing": t.testing,
    });
    if let Some(at) = t.archived_at {
        v["archived"] = json!(true);
        v["archived_on"] = json!(ymd(at));
        v["archived_by"] = json!(t.archived_by);
    }
    v
}

fn file_lines(list: Vec<FileRow>) -> Vec<Value> {
    list.into_iter().map(|f| json!({"name": f.name, "size_bytes": f.size_bytes, "added": ymd(f.created_at)})).collect()
}

fn usd(micros: i64) -> f64 {
    (micros as f64 / 10_000.0).round() / 100.0
}

pub(crate) fn overview(cx: &Cx) -> Result<Value, String> {
    let db = cx.db();
    let you = users::list(db).map_err(err)?.into_iter().find(|p| p.id == cx.st.you_id);
    let clients = clients::list(db).map_err(err)?;
    let projects = projects::list(db).map_err(err)?;
    let all = tasks::list(db, &TaskFilter::default()).map_err(err)?;
    let mut by_column: BTreeMap<String, i64> = BTreeMap::new();
    for t in &all {
        *by_column.entry(t.state_name.clone()).or_default() += 1;
    }
    let live = crate::runs::live(cx.st);
    let agents: Vec<Value> = team::all_agents(db).map_err(err)?.into_iter().map(|(_, m)| json!({
        "name": m.name, "role": m.role_key, "status": m.status, "working": live.iter().any(|r| r.agent_id == m.actor_id), "chat": m.chat_enabled,
    })).collect();
    Ok(json!({
        "today": ymd(ids::now_ms()),
        "you": you.map(|p| json!({"name": p.name, "email": p.email})),
        "clients": clients.len(),
        "projects": projects.iter().filter(|p| p.status == "active").take(30).map(|p| json!({
            "key": p.key, "name": p.name, "client": p.client_name, "open_tasks": p.open_tasks, "repo": p.repo_path.is_some()})).collect::<Vec<_>>(),
        "tasks_by_column": by_column,
        "inbox": tasks::needs_you(db, &cx.st.you_id).map_err(err)?.len(),
        "team_lead_chats": lead_chats(cx)?,
        "agents": agents,
        "runs_working_now": live.len(),
    }))
}

/// The chats the Team Lead started that wait for the user's answer, newest first.
fn lead_chats(cx: &Cx) -> Result<Vec<Value>, String> {
    Ok(gizai_core::chat::waiting_lead_chats(cx.db()).map_err(err)?.into_iter().map(|t| json!({
        "title": t.title, "kind": t.kind, "tasks": t.tasks, "since": ymd(t.updated_at),
    })).collect())
}

pub(crate) fn inbox(cx: &Cx) -> Result<Value, String> {
    let items: Vec<Value> = tasks::needs_you(cx.db(), &cx.st.you_id).map_err(err)?.iter().map(|t| json!({
        "task": t.identifier, "title": t.title, "project": t.project_name, "column": t.state_name,
        "why": if t.hold.is_some() && !t.with_lead { "on hold" } else { "waiting for your review" },
        "hold": t.hold, "reason": t.hold_reason, "assignee": t.assignee_name,
    })).collect();
    Ok(json!({"count": items.len(), "items": items, "team_lead_chats": lead_chats(cx)?}))
}

pub(crate) fn list_clients(cx: &Cx, a: &Args) -> Result<Value, String> {
    let status = a.opt("status");
    let list: Vec<Value> = clients::list(cx.db()).map_err(err)?.into_iter()
        .filter(|c| status.as_ref().is_none_or(|s| &c.status == s))
        .map(|c| json!({"id": c.id, "name": c.name, "kind": c.kind, "city": c.city, "email": c.email, "main_contact": c.main_contact,
                        "status": c.status, "projects": c.projects, "open_tasks": c.open_tasks}))
        .collect();
    Ok(json!({"count": list.len(), "clients": list}))
}

pub(crate) fn get_client(cx: &Cx, a: &Args) -> Result<Value, String> {
    let c = resolve::client(cx, &a.req("client")?)?;
    let contacts: Vec<Value> = clients::contacts(cx.db(), &c.id).map_err(err)?.into_iter()
        .map(|k| json!({"name": k.name, "role": k.role, "email": k.email, "phone": k.phone, "primary": k.is_primary})).collect();
    let projects: Vec<Value> = projects::list(cx.db()).map_err(err)?.into_iter().filter(|p| p.client_id.as_deref() == Some(&c.id))
        .map(|p| json!({"key": p.key, "name": p.name, "status": p.status, "open_tasks": p.open_tasks})).collect();
    let files = file_lines(files::list(cx.db(), "client", &c.id).map_err(err)?);
    Ok(json!({"client": {
        "id": c.id, "name": c.name, "legal_name": c.legal_name, "kind": c.kind, "status": c.status, "email": c.email, "phone": c.phone,
        "website": c.website, "street": c.street, "postal_code": c.postal_code, "city": c.city, "country": c.country,
        "vat_number": c.vat_number, "coc_number": c.coc_number, "iban": c.iban, "payment_terms_days": c.payment_terms_days, "notes_md": c.notes_md,
    }, "contacts": contacts, "projects": projects, "files": files}))
}

pub(crate) fn list_projects(cx: &Cx, a: &Args) -> Result<Value, String> {
    let client = match a.opt("client") { Some(c) => Some(resolve::client(cx, &c)?.id), None => None };
    let status = a.opt("status");
    let list: Vec<Value> = projects::list(cx.db()).map_err(err)?.into_iter()
        .filter(|p| client.as_ref().is_none_or(|c| p.client_id.as_ref() == Some(c)))
        .filter(|p| status.as_ref().is_none_or(|s| &p.status == s))
        .map(|p| json!({"id": p.id, "key": p.key, "name": p.name, "client": p.client_name, "status": p.status, "repo_path": p.repo_path,
                        "open_tasks": p.open_tasks, "done_tasks": p.done_tasks}))
        .collect();
    Ok(json!({"count": list.len(), "projects": list}))
}

pub(crate) fn get_project(cx: &Cx, a: &Args) -> Result<Value, String> {
    let p = resolve::project(cx, &a.req("project")?)?;
    let mut by_column: BTreeMap<String, i64> = BTreeMap::new();
    for t in tasks::list(cx.db(), &TaskFilter { project_id: Some(p.id.clone()), open_only: false }).map_err(err)? {
        *by_column.entry(t.state_name).or_default() += 1;
    }
    let docs: Vec<Value> = docs::list(cx.db(), &p.id).map_err(err)?.into_iter()
        .map(|d| json!({"title": d.title, "version": d.current_version, "updated": ymd(d.updated_at)})).collect();
    Ok(json!({"project": {
        "id": p.id, "key": p.key, "number": p.number, "name": p.name, "client": p.client_name, "status": p.status, "goal_md": p.goal_md,
        "repo_path": p.repo_path, "default_branch": p.default_branch, "color": p.color,
        "repository": p.repo_url, "provider": super::provider(p.repo_url.as_deref()),
    }, "tasks_by_column": by_column, "docs": docs, "files": file_lines(files::list(cx.db(), "project", &p.id).map_err(err)?)}))
}

pub(crate) fn list_tasks(cx: &Cx, a: &Args) -> Result<Value, String> {
    let project = match a.opt("project") { Some(p) => Some(resolve::project(cx, &p)?), None => None };
    let include_done = a.flag("include_done").unwrap_or(false);
    let mut list = if a.flag("archived").unwrap_or(false) {
        tasks::archived(cx.db(), project.as_ref().map(|p| p.id.as_str())).map_err(err)?
    } else {
        tasks::list(cx.db(), &TaskFilter { project_id: project.as_ref().map(|p| p.id.clone()), open_only: !include_done }).map_err(err)?
    };
    if let Some(c) = a.opt("column") {
        let col = resolve::column(&resolve::team_of(cx, project.as_ref())?, &c)?;
        list.retain(|t| t.state_name.eq_ignore_ascii_case(&col.name));
    }
    match a.opt("assignee").as_deref() {
        None => {}
        Some(n) if n.eq_ignore_ascii_case("none") => list.retain(|t| t.assignee_id.is_none()),
        Some(n) => {
            let who = resolve::assignee(cx, n)?;
            list.retain(|t| t.assignee_id.as_deref() == Some(&who));
        }
    }
    if let Some(l) = a.opt("label") {
        list.retain(|t| t.labels.iter().any(|x| x.name.eq_ignore_ascii_case(&l)));
    }
    if let Some(q) = a.opt("text") {
        let q = q.to_lowercase();
        list.retain(|t| t.title.to_lowercase().contains(&q) || t.identifier.to_lowercase() == q);
    }
    let limit = a.int("limit")?.unwrap_or(50).clamp(1, 200) as usize;
    let shown: Vec<Value> = list.iter().take(limit).map(task_line).collect();
    Ok(json!({"count": list.len(), "shown": shown.len(), "tasks": shown}))
}

pub(crate) fn get_task(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let all = comments::list(cx.db(), &t.id).map_err(err)?;
    let skip = all.len().saturating_sub(20);
    let comments: Vec<Value> = all.into_iter().skip(skip).map(|c| json!({"author": c.author_name, "at": ymd(c.created_at), "body_md": c.body_md})).collect();
    let runs: Vec<Value> = runs::list_for_task(cx.db(), &t.id).map_err(err)?.into_iter().take(5).map(|r| json!({
        "agent": r.agent_name, "status": r.status, "outcome": r.outcome, "trigger": r.trigger, "at": ymd(r.created_at), "summary": r.summary_md, "error": r.error,
        "refused": r.refused,
    })).collect();
    Ok(json!({"task": {
        "id": t.id, "task": t.identifier, "title": t.title, "project": t.project_name, "column": t.state_name, "priority": t.priority,
        "assignee": t.assignee_name, "labels": t.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), "hold": t.hold,
        "hold_reason": t.hold_reason, "testing": t.testing, "description_md": t.description_md, "acceptance_md": t.acceptance_md, "branch": t.branch,
        // GA-70: its question is with you (the Team Lead's run on it answers or asks the user).
        "with_team_lead": t.with_lead,
        "created": ymd(t.created_at), "updated": ymd(t.updated_at),
        "archived": t.archived_at.is_some(), "archived_on": t.archived_at.map(ymd), "archived_by": t.archived_by,
    }, "comments": comments, "runs": runs, "files": file_lines(files::list(cx.db(), "task", &t.id).map_err(err)?)}))
}

fn agent_json(cx: &Cx, m: &team::Member, live: &[crate::runs::LiveRun]) -> Value {
    let spent = runs::agent_spend_since(cx.db(), &m.actor_id, runs::month_start_ms(ids::now_ms())).unwrap_or(0);
    json!({
        "id": m.actor_id, "name": m.name, "role": m.role_key, "title": m.title, "status": m.status,
        "columns": gizai_core::columns::of_agent(cx.db(), &m.actor_id).unwrap_or_default(), "model": m.model, "effort": m.effort, "cards_at_once": m.max_runs, "chat": m.chat_enabled,
        "working": live.iter().any(|r| r.agent_id == m.actor_id),
        "spent_this_month_usd": usd(spent), "monthly_budget_usd": m.budget_usd_micros.map(usd),
        "board_check_minutes": m.board_check_minutes, "board_check_paused": m.board_check_paused,
    })
}

pub(crate) fn list_agents(cx: &Cx) -> Result<Value, String> {
    let live = crate::runs::live(cx.st);
    let agents: Vec<Value> = team::all_agents(cx.db()).map_err(err)?.iter().map(|(_, m)| agent_json(cx, m, &live)).collect();
    Ok(json!({"count": agents.len(), "agents": agents}))
}

pub(crate) fn get_agent(cx: &Cx, a: &Args) -> Result<Value, String> {
    let m = resolve::agent(cx, &a.req("agent")?)?;
    let live = crate::runs::live(cx.st);
    let mut v = agent_json(cx, &m, &live);
    v["permission_mode"] = json!(m.permission_mode);
    v["allowed_tools"] = json!(m.allowed_tools);
    // Its MCP servers' switches, to show: only the user changes them (agent form → Tools).
    let servers = gizai_core::mcp_servers::list(cx.db()).unwrap_or_default();
    v["mcp_servers"] = json!(m.tools.mcp.iter().filter_map(|o| servers.iter().find(|s| s.id == o.server_id).map(|s| json!({
        "name": s.name, "on": o.on, "tools_off": o.tools_off,
    }))).collect::<Vec<_>>());
    // Its web tools, browser and built-in tools, to show: only the user switches them (agent form → Tools).
    let c = &m.cli_tools;
    v["web_search"] = json!(c.web_search);
    v["web_fetch"] = json!(c.web_fetch);
    v["fetch_domains"] = json!(c.fetch_domains);
    v["browser"] = json!(m.tools.browser_on());
    v["builtin_tools_on"] = json!(c.builtin);
    v["instructions_md"] = json!(m.instructions_md);
    let recent: Vec<Value> = runs::list_for_agent(cx.db(), &m.actor_id, 10).map_err(err)?.into_iter().map(|r| json!({
        "status": r.status, "outcome": r.outcome, "trigger": r.trigger, "at": ymd(r.created_at), "cost_usd": usd(r.cost_usd_micros), "error": r.error,
        "task": r.task_id.as_deref().and_then(|t| tasks::get(cx.db(), t).ok()).map(|t| t.identifier), "refused": r.refused,
    })).collect();
    Ok(json!({"agent": v, "recent_runs": recent}))
}

pub(crate) fn list_docs(cx: &Cx, a: &Args) -> Result<Value, String> {
    let p = resolve::project(cx, &a.req("project")?)?;
    let list: Vec<Value> = docs::list(cx.db(), &p.id).map_err(err)?.into_iter()
        .map(|d| json!({"id": d.id, "title": d.title, "version": d.current_version, "updated": ymd(d.updated_at)})).collect();
    Ok(json!({"project": p.key, "docs": list}))
}

pub(crate) fn read_doc(cx: &Cx, a: &Args) -> Result<Value, String> {
    let project = match a.opt("project") { Some(p) => Some(resolve::project(cx, &p)?), None => None };
    let d = resolve::doc(cx, &a.req("doc")?, project.as_ref())?;
    let key = d.project_id.as_ref().and_then(|id| projects::get(cx.db(), id).ok()).map(|p| p.key);
    Ok(json!({"doc": {"id": d.id, "title": d.title, "project": key, "version": d.current_version, "updated": ymd(d.updated_at), "body_md": d.body_md}}))
}

pub(crate) fn list_people(cx: &Cx) -> Result<Value, String> {
    let people: Vec<Value> = users::list(cx.db()).map_err(err)?.into_iter().map(|p| json!({
        "id": p.id, "name": p.name, "email": p.email, "title": p.title, "open_tasks": p.open_tasks, "you": p.id == cx.st.you_id,
    })).collect();
    Ok(json!({"people": people}))
}

/// What a column does, in one line: "Auto: Backend Agent and Frontend Agent take cards by priority and move them to In
/// progress", "Manual: press Run on a card", "You review and merge; merged cards go to Deploy".
pub(crate) fn column_line(s: &team::WorkflowState, t: &team::Team) -> String {
    let next = s.next_state_id.as_ref().and_then(|n| t.states.iter().find(|x| &x.id == n)).map(|x| x.name.clone());
    let names: Vec<String> = s.agent_ids.iter().map(|id| t.members.iter().find(|m| &m.actor_id == id).map(|m| m.name.clone()).unwrap_or_else(|| "(removed agent)".into())).collect();
    let who = match names.len() {
        0 => String::new(),
        1 => names[0].clone(),
        n => format!("{} and {}", names[..n - 1].join(", "), names[n - 1]),
    };
    match s.category.as_str() {
        "backlog" => "New cards wait here; nothing starts by itself".into(),
        "review" => match next {
            Some(n) => format!("You review and merge; merged cards go to {n}"),
            None => "You review and merge; merged cards go to the first Deploy column, else Done".into(),
        },
        "done" | "cancelled" => "Finished cards; nothing starts here".into(),
        _ if s.auto && names.is_empty() => "Auto, but no agent is on it: put one on it, or its cards wait".into(),
        _ if s.auto => {
            let verb = if names.len() == 1 { "takes" } else { "take" };
            match next {
                Some(n) if s.category == "ready" => format!("Auto: {who} {verb} cards by priority and move them to {n}"),
                Some(n) => format!("Auto: {who} {verb} cards by priority; done, they go to {n}"),
                None => format!("Auto: {who} {verb} cards by priority"),
            }
        }
        _ => {
            let run = if who.is_empty() { "press Run on a card and pick an agent".to_string() } else { format!("press Run on a card to start {}", names[0]) };
            match next {
                Some(n) => format!("Manual: {run}; done, it goes to {n}"),
                None => format!("Manual: {run}"),
            }
        }
    }
}

pub(crate) fn workflow(cx: &Cx) -> Result<Value, String> {
    let t = resolve::team_of(cx, None)?;
    let kind = |c: &str| gizai_core::columns::KINDS.iter().find(|(_, cat)| *cat == c).map(|(k, _)| *k).unwrap_or(c).to_string();
    Ok(json!({
        "team": t.name,
        "columns": t.states.iter().map(|s| {
            let agents: Vec<String> = s.agent_ids.iter().filter_map(|id| t.members.iter().find(|m| &m.actor_id == id).map(|m| m.name.clone())).collect();
            let next = s.next_state_id.as_ref().and_then(|n| t.states.iter().find(|x| &x.id == n)).map(|x| x.name.clone());
            json!({"name": s.name, "kind": kind(&s.category), "agents": agents,
                   "start": if gizai_core::columns::takes_agents(&s.category) { if s.auto { "auto" } else { "manual" } } else { "none" },
                   "next": next, "what_happens": column_line(s, &t)})
        }).collect::<Vec<_>>(),
        "labels": t.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
    }))
}
