//! Tools that change things. Each one returns `{ok, …, link}` so the chat can show a card that opens the item,
//! and tells open screens what changed. Updates merge: only the fields given change.
use gizai_core::model::*;
use gizai_core::{clients, comments, docs, files, projects, tasks, team, users};
use serde_json::{Value, json};

use super::{Args, Cx, err, resolve, short};

fn link(page: &str, id: &str, label: &str) -> Value {
    json!({"page": page, "id": id, "label": label})
}

fn task_json(t: &Task) -> Value {
    json!({"id": t.id, "identifier": t.identifier, "title": t.title, "column": t.state_name, "assignee": t.assignee_name,
           "labels": t.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), "hold": t.hold, "testing": t.testing})
}

fn task_done(cx: &Cx, id: &str, what: &str) -> Result<Value, String> {
    let t = tasks::get(cx.db(), id).map_err(err)?;
    cx.changed("tasks");
    cx.wake(&t.id);
    Ok(json!({"ok": true, "done": what, "task": task_json(&t), "link": link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60)))}))
}

/// For an update: the given value (empty clears), else the current one.
fn keep(a: &Args, k: &str, current: &Option<String>) -> Option<String> {
    match a.text(k) {
        Some(v) if v.is_empty() => None,
        Some(v) => Some(v),
        None => current.clone(),
    }
}

/// The argument a project's repository link is in: `repository`, or `github`, its old name.
fn repository_arg(a: &Args) -> &'static str {
    if a.text("repository").is_some() { "repository" } else { "github" }
}

// ---- guards: what the chat may not do, whatever it is asked ----

/// The Team Lead reads text other people and agents wrote, so it can't hand an agent unlimited powers: no
/// bypassPermissions, no "any command" (bare Bash). People set those in the agent form.
fn guard_agent_powers(a: &Args) -> Result<(), String> {
    // Claude Code's bypassPermissions, Codex without its sandbox, Gemini's yolo: each lets an agent run anything.
    if let Some(m @ ("bypassPermissions" | "danger-full-access" | "yolo")) = a.opt("permission_mode").as_deref() {
        return Err(format!("{m} can't be set from chat: it lets an agent run anything. Set it yourself in the agent form if you really want it."));
    }
    let allowed = a.list("allowed_tools").unwrap_or_default();
    for t in &allowed {
        let t = t.trim().to_lowercase().replace(' ', "");
        if t == "bash" || t == "bash(*)" || t == "bash(:*)" || t == "bash(*:*)" {
            return Err("an agent can't be allowed to run any command from chat; name the commands, like Bash(npm test:*) or Bash(git commit:*)".into());
        }
    }
    // Only commands: web search, fetching pages and the CLI's other tools are the user's switches (agent form → Tools), and a
    // rule for another tool, like Read(//…), could reach past the worktree.
    if let Some(t) = allowed.iter().find(|t| !is_command(t)) {
        return Err(format!("allowed_tools takes only commands, like Bash(npm test:*); {t} isn't one. Web search, fetching pages, the browser, \
                            MCP servers and the CLI's other tools can't be switched on from chat: only the user does that, in the agent form → \
                            Tools. Nothing changed."));
    }
    // MCP servers and their tools: only the user switches them, in the agent form → Tools.
    for k in ["mcp_servers", "mcp", "tools", "mcp_tools"] {
        if a.0.get(k).is_some_and(|v| !v.is_null()) {
            return Err("an agent's MCP servers and their tools can't be switched from chat: only the user does that, in the agent form → Tools".into());
        }
    }
    // Web search, fetching pages, the browser and the CLI's built-in tools: the same, only the user (agent form → Tools).
    for k in ["web_search", "web_fetch", "fetch_domains", "web", "browser", "insecure_certs", "builtin_tools", "builtin", "cli_tools"] {
        if a.0.get(k).is_some_and(|v| !v.is_null()) {
            return Err("an agent's web search, fetching pages, browser and built-in tools can't be switched from chat: only the user does that, \
                        in the agent form → Tools. Nothing changed.".into());
        }
    }
    // The folders an agent may read or change: only the user sets them, in the agent form.
    if a.0.get("folders").is_some_and(|v| !v.is_null()) {
        return Err("an agent's folders can't be changed from chat: the user sets them in the agent form (Team page → the agent → Permissions → Folders). Nothing changed.".into());
    }
    Ok(())
}

/// Team Lead may merge (GA-86) is the user's switch, set in the app: create_project and update_project never set it.
fn guard_lead_may_merge(a: &Args) -> Result<(), String> {
    for k in ["lead_may_merge", "team_lead_may_merge", "leadMayMerge", "may_merge"] {
        if a.0.get(k).is_some_and(|v| !v.is_null()) {
            return Err("Team Lead may merge can't be switched from chat: only the user switches it, in the app (the project page → Edit). \
                        Nothing changed.".into());
        }
    }
    Ok(())
}

/// A command for an agent's allowed commands: `Bash(…)` with something in it.
fn is_command(t: &str) -> bool {
    t.trim().strip_prefix("Bash(").and_then(|r| r.strip_suffix(')')).is_some_and(|inner| !inner.trim().is_empty())
}

/// A repository path from chat must be a git repository (it has a .git folder or file), and never `/`, your
/// home folder or a folder above it: agents work there and the Team Lead may read it.
fn checked_repo(raw: &str) -> Result<String, String> {
    let bad = || format!("{raw} is not a git repository: give the folder that holds the project's .git");
    let p = std::path::Path::new(raw).canonicalize().map_err(|_| bad())?;
    let home = std::path::Path::new(&gizai_core::clis::home()).canonicalize().ok();
    if p.parent().is_none() || home.as_ref().is_some_and(|h| h.starts_with(&p)) || !p.join(".git").exists() {
        return Err(bad());
    }
    Ok(p.display().to_string())
}

/// The model and effort must be ones Claude Code offers (when its list can be read): a wrong name like
/// "opus 5.5" would only fail at the agent's next run.
async fn check_model(cx: &Cx<'_>, model: Option<&str>, effort: Option<&str>) -> Result<(), String> {
    let Ok(list) = crate::runs::models(cx.st, false).await else { return Ok(()) };
    let wanted = model.unwrap_or("default");
    let Some(m) = list.iter().find(|m| m.value.eq_ignore_ascii_case(wanted) || m.resolved_model.as_deref().is_some_and(|r| r.eq_ignore_ascii_case(wanted))) else {
        let names: Vec<String> = list.iter().map(|m| match &m.resolved_model { Some(r) => format!("{} ({r})", m.value), None => m.value.clone() }).collect();
        return Err(format!("Claude Code has no model called \"{wanted}\". Use one of: {}.", names.join(", ")));
    };
    if let Some(e) = effort {
        if !m.effort_levels.iter().any(|l| l == e) {
            return Err(if m.effort_levels.is_empty() {
                format!("{} takes no effort level; leave effort empty", m.display_name)
            } else {
                format!("{} takes effort {}, not {e}", m.display_name, m.effort_levels.join(", "))
            });
        }
    }
    Ok(())
}

// ---- clients ----

fn client_input(a: &Args, cur: Option<&Client>) -> Result<ClientInput, String> {
    let c = |k: &str, v: Option<&Option<String>>| match v { Some(cur) => keep(a, k, cur), None => a.opt(k) };
    Ok(ClientInput {
        name: a.opt("name").or_else(|| cur.map(|c| c.name.clone())).unwrap_or_default(),
        legal_name: c("legal_name", cur.map(|x| &x.legal_name)),
        kind: a.opt("kind").or_else(|| cur.map(|x| x.kind.clone())),
        vat_number: c("vat_number", cur.map(|x| &x.vat_number)),
        coc_number: c("coc_number", cur.map(|x| &x.coc_number)),
        iban: c("iban", cur.map(|x| &x.iban)),
        email: c("email", cur.map(|x| &x.email)),
        phone: c("phone", cur.map(|x| &x.phone)),
        website: c("website", cur.map(|x| &x.website)),
        street: c("street", cur.map(|x| &x.street)),
        postal_code: c("postal_code", cur.map(|x| &x.postal_code)),
        city: c("city", cur.map(|x| &x.city)),
        country: a.opt("country").or_else(|| cur.and_then(|x| x.country.clone())),
        payment_terms_days: a.int("payment_terms_days")?.or_else(|| cur.and_then(|x| x.payment_terms_days)),
        status: a.opt("status").or_else(|| cur.map(|x| x.status.clone())),
        notes_md: c("notes_md", cur.map(|x| &x.notes_md)),
    })
}

pub(crate) fn create_client(cx: &Cx, a: &Args) -> Result<Value, String> {
    a.req("name")?;
    let id = clients::create(cx.db(), cx.actor, client_input(a, None)?).map_err(err)?;
    cx.changed("clients");
    let c = clients::get(cx.db(), &id).map_err(err)?;
    Ok(json!({"ok": true, "client": {"id": c.id, "name": c.name}, "link": link("client", &c.id, &c.name)}))
}

pub(crate) fn update_client(cx: &Cx, a: &Args) -> Result<Value, String> {
    let cur = resolve::client(cx, &a.req("client")?)?;
    clients::update(cx.db(), cx.actor, &cur.id, client_input(a, Some(&cur))?).map_err(err)?;
    cx.changed("clients");
    let c = clients::get(cx.db(), &cur.id).map_err(err)?;
    Ok(json!({"ok": true, "client": {"id": c.id, "name": c.name, "city": c.city, "status": c.status}, "link": link("client", &c.id, &c.name)}))
}

pub(crate) fn save_contact(cx: &Cx, a: &Args) -> Result<Value, String> {
    let c = resolve::client(cx, &a.req("client")?)?;
    let existing = clients::contacts(cx.db(), &c.id).map_err(err)?;
    let cur = match a.opt("contact") {
        Some(r) => {
            let cands: Vec<resolve::Cand> = existing.iter().map(|k| resolve::Cand { id: k.id.clone(), names: vec![k.name.clone()], show: k.name.clone() }).collect();
            Some(existing[resolve::pick("contact", "contacts", "full name", &r, &cands)?].clone())
        }
        None => None,
    };
    let contact = Contact {
        id: cur.as_ref().map(|k| k.id.clone()).unwrap_or_default(),
        client_id: c.id.clone(),
        name: a.req("name")?,
        role: keep(a, "role", &cur.as_ref().and_then(|k| k.role.clone())),
        email: keep(a, "email", &cur.as_ref().and_then(|k| k.email.clone())),
        phone: keep(a, "phone", &cur.as_ref().and_then(|k| k.phone.clone())),
        is_primary: a.flag("is_primary").unwrap_or(cur.as_ref().is_some_and(|k| k.is_primary) || existing.is_empty()),
    };
    let id = clients::upsert_contact(cx.db(), cx.actor, contact).map_err(err)?;
    cx.changed("contacts");
    Ok(json!({"ok": true, "contact": {"id": id, "name": a.req("name")?}, "link": link("client", &c.id, &c.name)}))
}

// ---- projects ----

pub(crate) fn create_project(cx: &Cx, a: &Args) -> Result<Value, String> {
    guard_lead_may_merge(a)?;
    let name = a.req("name")?;
    let repo_path = a.opt("repo_path").map(|r| checked_repo(&r)).transpose()?;
    let key = match a.opt("key") {
        Some(k) => k.to_uppercase(),
        None => projects::unused_key(cx.db(), &name).map_err(err)?,
    };
    let client_id = match a.opt("client") { Some(c) => Some(resolve::client(cx, &c)?.id), None => None };
    let id = projects::create(cx.db(), cx.actor, ProjectInput {
        client_id, name, key, status: a.opt("status"), goal_md: a.opt("goal_md"), repo_path,
        default_branch: a.opt("default_branch"), color: a.opt("color"), repo_url: a.opt(repository_arg(a)), ..Default::default()
    }).map_err(err)?;
    cx.changed("projects");
    let p = projects::get(cx.db(), &id).map_err(err)?;
    let label = format!("{} ({})", p.name, p.key);
    Ok(json!({"ok": true, "project": {"id": p.id, "key": p.key, "name": p.name, "number": p.number}, "link": link("project", &p.id, &label)}))
}

pub(crate) fn update_project(cx: &Cx, a: &Args) -> Result<Value, String> {
    guard_lead_may_merge(a)?;
    let cur = resolve::project(cx, &a.req("project")?)?;
    let repo_path = match a.text("repo_path") {
        Some(r) if r.is_empty() => None,
        Some(r) => Some(checked_repo(&r)?),
        None => cur.repo_path.clone(),
    };
    let client_id = match a.text("client") {
        Some(c) if c.is_empty() => None,
        Some(c) => Some(resolve::client(cx, &c)?.id),
        None => cur.client_id.clone(),
    };
    projects::update(cx.db(), cx.actor, &cur.id, ProjectInput {
        client_id, name: a.opt("name").unwrap_or_else(|| cur.name.clone()), key: cur.key.clone(),
        status: a.opt("status").or(Some(cur.status.clone())), goal_md: keep(a, "goal_md", &cur.goal_md),
        repo_path, default_branch: a.opt("default_branch").or(Some(cur.default_branch.clone())),
        color: keep(a, "color", &cur.color), budget_amount_minor: cur.budget_amount_minor, budget_hours: cur.budget_hours,
        repo_url: keep(a, repository_arg(a), &cur.repo_url), ..Default::default()
    }).map_err(err)?;
    cx.changed("projects");
    let p = projects::get(cx.db(), &cur.id).map_err(err)?;
    let label = format!("{} ({})", p.name, p.key);
    Ok(json!({"ok": true, "project": {"id": p.id, "key": p.key, "name": p.name, "status": p.status, "repo_path": p.repo_path,
              "repository": p.repo_url, "provider": super::provider(p.repo_url.as_deref())}, "link": link("project", &p.id, &label)}))
}

// ---- tasks ----

pub(crate) fn create_task(cx: &Cx, a: &Args) -> Result<Value, String> {
    let p = resolve::project(cx, &a.req("project")?)?;
    let title = a.req("title")?;
    let team = resolve::team_of(cx, Some(&p))?;
    let state_id = match a.opt("column") { Some(c) => Some(resolve::column(&team, &c)?.id), None => None };
    let label_ids = match a.list("labels") { Some(l) => resolve::labels(&team, &l)?, None => vec![] };
    let assignee_id = match a.opt("assignee") {
        Some(n) if !n.eq_ignore_ascii_case("none") => Some(resolve::assignee(cx, &n)?),
        _ => None,
    };
    let id = tasks::create(cx.db(), cx.actor, TaskInput {
        project_id: p.id.clone(), title, description_md: a.opt("description_md").unwrap_or_default(), acceptance_md: a.opt("acceptance_md"),
        state_id, priority: a.int("priority")?.unwrap_or(0), assignee_id, label_ids, testing: Some(a.flag("testing").unwrap_or(true)),
    }).map_err(err)?;
    task_done(cx, &id, "created")
}

pub(crate) fn update_task(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let mut patch = TaskPatch { title: a.opt("title"), description_md: a.text("description_md"), acceptance_md: a.text("acceptance_md"),
                                priority: a.int("priority")?, testing: a.flag("testing"), ..Default::default() };
    if let Some(n) = a.text("assignee") {
        patch.assignee_id = Some(if n.is_empty() || n.eq_ignore_ascii_case("none") { String::new() } else { resolve::assignee(cx, &n)? });
    }
    if a.flag("clear_hold") == Some(true) {
        patch.hold = Some(String::new());
    } else if let Some(h) = a.opt("hold") {
        patch.hold = Some(h);
        patch.hold_reason = a.opt("hold_reason");
    } else if let Some(r) = a.text("hold_reason") {
        patch.hold_reason = Some(r);
    }
    let labels = a.list("labels");
    let nothing = patch.title.is_none() && patch.description_md.is_none() && patch.acceptance_md.is_none() && patch.priority.is_none()
        && patch.testing.is_none() && patch.assignee_id.is_none() && patch.hold.is_none() && patch.hold_reason.is_none() && labels.is_none();
    if nothing {
        return Err("nothing to change: give a field to update".into());
    }
    // Everything given is checked before anything changes: a bad label (or hold, title, …) changes nothing.
    let label_ids = match labels {
        Some(l) => Some(resolve::labels(&resolve::team_of(cx, t.project_id.as_ref().and_then(|p| projects::get(cx.db(), p).ok()).as_ref())?, &l)?),
        None => None,
    };
    tasks::update_with_labels(cx.db(), cx.actor, &t.id, patch, label_ids).map_err(err)?;
    task_done(cx, &t.id, "updated")
}

pub(crate) fn move_task(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let project = t.project_id.as_ref().and_then(|p| projects::get(cx.db(), p).ok());
    let col = resolve::column(&resolve::team_of(cx, project.as_ref())?, &a.req("column")?)?;
    if cx.check.is_some() && matches!(col.category.as_str(), "review" | "deploy" | "done") {
        return Err(format!("A board check never moves a card to {}: ask the user in a chat (start_chat) instead.", col.name));
    }
    tasks::move_to(cx.db(), cx.actor, &t.id, &col.id, "").map_err(err)?;
    task_done(cx, &t.id, "moved")
}

pub(crate) fn comment(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let body = a.req("body_md")?;
    comments::add(cx.db(), cx.actor, &t.id, &body, None).map_err(err)?;
    cx.changed("comments");
    Ok(json!({"ok": true, "done": "commented", "link": link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60)))}))
}

// ---- agents ----

fn budget(a: &Args, cur: Option<i64>) -> Result<Option<i64>, String> {
    match a.text("monthly_budget_usd") {
        Some(s) if s.is_empty() => Ok(None),
        Some(_) => Ok(a.num("monthly_budget_usd")?.map(|d| (d * 1_000_000.0).round() as i64)),
        None => Ok(cur),
    }
}

fn agent_result(cx: &Cx, id: &str, what: &str) -> Result<Value, String> {
    cx.changed("actors");
    let m = team::agent(cx.db(), id).map_err(err)?;
    let columns = gizai_core::columns::of_agent(cx.db(), &m.actor_id).unwrap_or_default();
    Ok(json!({"ok": true, "done": what, "agent": {"id": m.actor_id, "name": m.name, "role": m.role_key, "status": m.status, "columns": columns},
              "link": link("agent", &m.actor_id, &m.name)}))
}

/// The coding CLI from `runs_on` (its name or id), as its id; None when not given. An Other CLI runs with its own
/// permissions (whatever its arguments allow), so only you can pick one, in the agent form.
fn runs_on(cx: &Cx<'_>, a: &Args) -> Result<Option<gizai_core::clis::Cli>, String> {
    let Some(want) = a.opt("runs_on") else { return Ok(None) };
    let all = gizai_core::clis::list(cx.db()).map_err(err)?;
    let Some(c) = all.iter().find(|c| c.id == want.trim() || c.name.eq_ignore_ascii_case(want.trim())) else {
        let names: Vec<&str> = all.iter().map(|c| c.name.as_str()).collect();
        return Err(format!("there is no coding CLI called \"{want}\". Use one of: {}.", names.join(", ")));
    };
    if c.kind == "other" {
        return Err(format!("{} runs with its own permissions, so it can't be picked from chat: set it yourself in the agent's settings (Runs on).", c.name));
    }
    Ok(Some(c.clone()))
}

pub(crate) async fn create_agent(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    guard_agent_powers(a)?;
    let cli = runs_on(cx, a)?;
    // Only Claude Code has a model list to check against; Codex and Gemini take their own model names.
    if cli.as_ref().is_none_or(|c| c.kind == "claude_code") {
        check_model(cx, a.opt("model").as_deref(), a.opt("effort").as_deref()).await?;
    }
    let team_id = resolve::team_of(cx, None)?.id;
    let (name, role) = (a.req("name")?, a.req("role")?);
    // No list given: its role's, as the agent form starts it.
    let allowed_tools = a.list("allowed_tools").filter(|l| !l.is_empty()).unwrap_or_else(|| gizai_core::seed::role_tools(&team::role_key(&role)));
    let id = team::add_agent(cx.db(), cx.actor, &team_id, AgentInput {
        name, role_key: role, title: a.opt("title"), adapter: cli.map(|c| c.id).unwrap_or_default(), model: a.opt("model"),
        instructions_md: a.opt("instructions_md"), permission_mode: a.opt("permission_mode").unwrap_or_default(),
        allowed_tools, wakeup: String::new(),
        heartbeat_minutes: None, budget_usd_micros: budget(a, None)?, chat_enabled: None, effort: a.opt("effort"),
        max_runs: a.int("cards_at_once")?, board_check_minutes: a.int("board_check_minutes")?, folders: None, use_memory: None,
    }).map_err(err)?;
    agent_result(cx, &id, "created")
}

pub(crate) async fn update_agent(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    guard_agent_powers(a)?;
    let m = resolve::agent(cx, &a.req("agent")?)?;
    // Moved to another kind of CLI: its model, effort and permission mode don't carry over unless given (the CLI's defaults).
    let kind_now = crate::clis::of_agent(cx.st, m.adapter.as_deref()).map(|c| c.kind).unwrap_or_default();
    let new_cli = runs_on(cx, a)?;
    let moved = new_cli.as_ref().is_some_and(|c| c.kind != kind_now);
    let model = if moved { a.opt("model") } else { keep(a, "model", &m.model) };
    let effort = if moved { a.opt("effort") } else { keep(a, "effort", &m.effort) };
    let permission_mode = a.opt("permission_mode").or(if moved { None } else { m.permission_mode.clone() }).unwrap_or_default();
    let adapter = match new_cli { Some(c) => Some(c.id), None => m.adapter.clone() };
    // Only Claude Code has a model list to check against; Codex, Gemini and other CLIs take their own model names.
    let on_claude = crate::clis::of_agent(cx.st, adapter.as_deref()).is_ok_and(|c| c.kind == "claude_code");
    if on_claude && (moved || a.text("model").is_some() || a.text("effort").is_some()) {
        check_model(cx, model.as_deref(), effort.as_deref()).await?;
    }
    team::update_agent(cx.db(), cx.actor, &m.actor_id, AgentInput {
        name: a.opt("name").unwrap_or(m.name.clone()), role_key: a.opt("role").unwrap_or(m.role_key.clone()),
        title: keep(a, "title", &m.title), adapter: adapter.unwrap_or_default(), model,
        instructions_md: a.opt("instructions_md"), permission_mode,
        allowed_tools: a.list("allowed_tools").unwrap_or(m.allowed_tools.clone()),
        wakeup: m.wakeup.clone().unwrap_or_default(),
        heartbeat_minutes: m.heartbeat_minutes,
        budget_usd_micros: budget(a, m.budget_usd_micros)?, chat_enabled: None, effort, max_runs: a.int("cards_at_once")?,
        board_check_minutes: a.int("board_check_minutes")?, folders: None, use_memory: None,
    }).map_err(err)?;
    crate::runs::resume_pull(cx.st, &m.actor_id);
    agent_result(cx, &m.actor_id, "updated")
}

pub(crate) fn set_agent_status(cx: &Cx, a: &Args) -> Result<Value, String> {
    let m = resolve::agent(cx, &a.req("agent")?)?;
    team::set_agent_status(cx.db(), cx.actor, &m.actor_id, &a.req("status")?).map_err(err)?;
    crate::runs::resume_pull(cx.st, &m.actor_id);
    agent_result(cx, &m.actor_id, "status changed")
}

/// The setup a tool gives for a column (agents by name, auto, next column by name, a new name, the column it goes after),
/// as a `ColumnInput`. "none" (or empty) as next clears it.
fn column_input(cx: &Cx, t: &team::Team, a: &Args, renaming: bool) -> Result<gizai_core::columns::ColumnInput, String> {
    let agent_ids = match a.list("agents") {
        Some(names) => Some(names.iter().map(|n| resolve::agent(cx, n).map(|m| m.actor_id)).collect::<Result<Vec<_>, String>>()?),
        None => None,
    };
    let next_state_id = match a.text("next") {
        Some(n) if n.is_empty() || n.eq_ignore_ascii_case("none") => Some(String::new()),
        Some(n) => Some(resolve::column(t, &n)?.id),
        None => None,
    };
    let after_id = match a.opt("after") { Some(n) => Some(resolve::column(t, &n)?.id), None => None };
    Ok(gizai_core::columns::ColumnInput {
        name: if renaming { a.opt("new_name") } else { None }, agent_ids, auto: a.flag("auto"), next_state_id, after_id,
    })
}

fn workflow_done(cx: &Cx, what: &str) -> Result<Value, String> {
    cx.changed("workflow_states");
    let t = resolve::team_of(cx, None)?;
    if tokio::runtime::Handle::try_current().is_ok() {
        let st = cx.st.clone();
        tokio::spawn(async move { crate::runs::pull(&st).await; });
    }
    let columns: Vec<String> = t.states.iter().map(|s| format!("{}: {}", s.name, super::read::column_line(s, &t))).collect();
    Ok(json!({"ok": true, "done": what, "columns": columns, "link": {"page": "team", "id": t.id, "label": "Workflow"}}))
}

pub(crate) fn add_column(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::team_of(cx, None)?;
    let after = resolve::column(&t, &a.req("after")?)?;
    let kind = a.req("kind").or_else(|_| a.req("category"))?;
    let category = gizai_core::columns::category_of(&kind)
        .ok_or_else(|| format!("a column is waiting, work, testing, review, deploy, done or backlog, not {kind}"))?;
    let setup = column_input(cx, &t, a, false)?;
    let name = a.req("name")?;
    // Checked before the column is added: nothing changes when its setup is refused.
    if setup.agent_ids.as_ref().is_some_and(|l| !l.is_empty()) && !gizai_core::columns::takes_agents(category) {
        return Err(format!("a {kind} column takes no agents: Backlog, Review, Done and Cancelled columns are for people"));
    }
    if setup.auto == Some(true) && setup.next_state_id.as_deref().is_none_or(str::is_empty) {
        return Err("an Auto column needs a next column, so a finished card isn't picked up again: give next".into());
    }
    let id = team::add_state(cx.db(), cx.actor, &t.id, &name, &after.id, category).map_err(err)?;
    let setup = gizai_core::columns::ColumnInput { after_id: None, ..setup };
    if let Err(e) = gizai_core::columns::set_column(cx.db(), cx.actor, &id, setup) {
        cx.changed("workflow_states");
        return Err(format!("{name} was added after {}, but its setup was refused: {}", after.name, err(e)));
    }
    workflow_done(cx, "column added")
}

pub(crate) fn set_column(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::team_of(cx, None)?;
    let col = resolve::column(&t, &a.req("column")?)?;
    let input = column_input(cx, &t, a, true)?;
    let nothing = input.name.is_none() && input.agent_ids.is_none() && input.auto.is_none() && input.next_state_id.is_none() && input.after_id.is_none();
    if nothing {
        return Err("nothing to change: give agents, auto, next, new_name or after".into());
    }
    gizai_core::columns::set_column(cx.db(), cx.actor, &col.id, input).map_err(err)?;
    workflow_done(cx, "column changed")
}

pub(crate) fn save_label(cx: &Cx, a: &Args) -> Result<Value, String> {
    let current = match a.opt("label") {
        Some(n) => Some(gizai_core::labels::find(cx.db(), &n).map_err(err)?.ok_or_else(|| format!("there is no label {n}: leave label out to add a new one"))?),
        None => None,
    };
    let name = a.opt("name").or_else(|| current.as_ref().map(|l| l.name.clone())).ok_or("missing \"name\"")?;
    let id = gizai_core::labels::save(cx.db(), cx.actor, current.as_ref().map(|l| l.id.as_str()), &name, a.opt("color").as_deref()).map_err(err)?;
    cx.changed("labels");
    let labels: Vec<String> = gizai_core::labels::list(cx.db()).map_err(err)?.into_iter().map(|l| l.name).collect();
    Ok(json!({"ok": true, "done": if current.is_some() { "label changed" } else { "label added" }, "label": {"id": id, "name": name},
              "labels": labels, "link": {"page": "team", "id": resolve::team_of(cx, None)?.id, "label": "Labels"}}))
}

/// A board check never starts more runs than the agent's free slots allow ("Runs at once" is checked by every start).
fn check_free_slot(cx: &Cx, agent_id: Option<&str>) -> Result<(), String> {
    let Some(agent_id) = agent_id.filter(|_| cx.check.is_some()) else { return Ok(()) };
    let m = team::agent(cx.db(), agent_id).map_err(err)?;
    let busy = crate::runs::live(cx.st).iter().filter(|r| r.agent_id == m.actor_id).count() as i64;
    if busy >= m.max_runs.max(1) {
        return Err(format!("{} has no free slot ({busy} of {} cards at once): a board check doesn't start more. Leave the card waiting.", m.name, m.max_runs.max(1)));
    }
    Ok(())
}

pub(crate) async fn start_run(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let agent = match a.opt("agent") { Some(n) => Some(resolve::agent(cx, &n)?.actor_id), None => None };
    check_free_slot(cx, agent.clone().or_else(|| crate::runs::suggest(cx.st, &t.id)).as_deref())?;
    let (run_id, _done) = crate::runs::start(cx.st, &t.id, agent, None, "manual").await?;
    let run = gizai_core::runs::get(cx.db(), &run_id).map_err(err)?;
    Ok(json!({"ok": true, "done": "started", "run": {"id": run.id, "agent": run.agent_name, "branch": run.branch},
              "link": link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60)))}))
}

/// Continue on the card's latest run, like the Continue button (`runs::continue_run`); a run that ended asking for a
/// decision continues too, with what was written on the card since (`runs::continue_answered`). A `note` goes to the
/// agent with it and on the card as the Team Lead's comment (`runs::continue_answered_with_note`, GA-31).
pub(crate) async fn continue_run(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let last = gizai_core::runs::list_for_task(cx.db(), &t.id).map_err(err)?.into_iter().next()
        .ok_or_else(|| format!("{} has no run to continue: start_agent_run starts one", t.identifier))?;
    check_free_slot(cx, Some(&last.agent_id))?;
    let note = a.opt("note").filter(|n| !n.trim().is_empty());
    let noted = note.is_some();
    let (run_id, _done) = crate::runs::continue_answered_with_note(cx.st, &last.id, cx.actor, note).await?;
    cx.changed("tasks");
    if noted {
        cx.changed("comments");
    }
    let run = gizai_core::runs::get(cx.db(), &run_id).map_err(err)?;
    Ok(json!({"ok": true, "done": "continued", "run": {"id": run.id, "agent": run.agent_name, "branch": run.branch},
              "link": link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60)))}))
}

// ---- chats the Team Lead starts ----

/// Asks the user in a chat that waits at the top of their Inbox (`chat::start_lead_chat`): one waiting chat per card.
pub(crate) fn start_chat(cx: &Cx, a: &Args) -> Result<Value, String> {
    let title = a.req("title")?;
    let kind = a.req("kind")?.to_lowercase();
    let body = a.req("body_md")?;
    let refs = a.list("tasks").unwrap_or_default();
    if refs.is_empty() {
        return Err("name the cards the chat is about (tasks), like [\"GA-12\"]".into());
    }
    let mut ids = vec![];
    for r in &refs {
        let t = resolve::task(cx, r)?;
        if !ids.contains(&t.id) {
            ids.push(t.id);
        }
    }
    let (id, new) = gizai_core::chat::start_lead_chat(cx.db(), cx.actor, &title, &kind, &ids, &body, cx.check).map_err(err)?;
    cx.changed("chat_threads");
    cx.changed("chat_messages");
    (cx.st.notify)(crate::runs::Note::ChatChanged);
    let t = gizai_core::chat::get_thread(cx.db(), &id).map_err(err)?;
    Ok(json!({"ok": true, "done": if new { "chat started" } else { "added to the chat that already waits for this card" },
              "chat": {"id": t.id, "title": t.title, "kind": t.kind, "tasks": t.tasks}, "link": link("chat", &t.id, &t.title)}))
}

pub(crate) fn stop_run(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let live: Vec<_> = crate::runs::live(cx.st).into_iter().filter(|r| r.task_id == t.id).collect();
    if live.is_empty() {
        return Err(format!("no agent is working on {} right now", t.identifier));
    }
    for r in &live {
        crate::runs::stop(cx.st, &r.run_id);
    }
    Ok(json!({"ok": true, "done": "stopping", "link": link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60)))}))
}

// ---- docs, files, people ----

pub(crate) fn create_doc(cx: &Cx, a: &Args) -> Result<Value, String> {
    let p = resolve::project(cx, &a.req("project")?)?;
    let title = a.req("title")?;
    let id = docs::create(cx.db(), cx.actor, &p.id, &title).map_err(err)?;
    if let Some(body) = a.opt("body_md") {
        docs::save(cx.db(), cx.actor, &id, &body, 1).map_err(err)?;
    }
    cx.changed("docs");
    let d = docs::get(cx.db(), &id).map_err(err)?;
    Ok(json!({"ok": true, "doc": {"id": d.id, "title": d.title, "version": d.current_version}, "link": link("doc", &d.id, &d.title)}))
}

pub(crate) fn write_doc(cx: &Cx, a: &Args) -> Result<Value, String> {
    let project = match a.opt("project") { Some(p) => Some(resolve::project(cx, &p)?), None => None };
    let d = resolve::doc(cx, &a.req("doc")?, project.as_ref())?;
    let body = a.text("body_md").ok_or("missing \"body_md\"")?;
    let v = docs::save(cx.db(), cx.actor, &d.id, &body, d.current_version).map_err(err)?;
    cx.changed("docs");
    Ok(json!({"ok": true, "doc": {"id": d.id, "title": d.title, "version": v}, "link": link("doc", &d.id, &d.title)}))
}

pub(crate) async fn attach_file(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    let raw = a.req("path")?;
    // `~/…`, and on Windows `~\…` too
    let path = match raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\").filter(|_| cfg!(windows))) {
        Some(rest) => std::path::PathBuf::from(gizai_core::clis::home()).join(rest),
        None => std::path::PathBuf::from(&raw),
    };
    if !path.is_absolute() {
        return Err(format!("{raw} is not an absolute path"));
    }
    allowed_attachment(cx, &raw, &path)?;
    let (owner_type, owner_id, l) = if let Some(r) = a.opt("task") {
        let t = resolve::task(cx, &r)?;
        ("task", t.id.clone(), link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60))))
    } else if let Some(r) = a.opt("project") {
        let p = resolve::project(cx, &r)?;
        ("project", p.id.clone(), link("project", &p.id, &format!("{} ({})", p.name, p.key)))
    } else if let Some(r) = a.opt("client") {
        let c = resolve::client(cx, &r)?;
        ("client", c.id.clone(), link("client", &c.id, &c.name))
    } else {
        return Err("say where to attach it: a task, project or client".into());
    };
    let (db, actor, dir) = (cx.st.db.clone(), cx.actor.to_string(), cx.st.data_dir.clone());
    let oid = owner_id.clone();
    let f = tokio::task::spawn_blocking(move || files::add_from_path(&db, &actor, &dir, owner_type, &oid, &path))
        .await.map_err(|e| e.to_string())?.map_err(err)?;
    cx.changed("files");
    Ok(json!({"ok": true, "file": {"id": f.id, "name": f.name, "size_bytes": f.size_bytes}, "link": l}))
}

/// Spec §8: the Team Lead attaches only a file the user named in this chat, one the user added to a message in this chat
/// (its copy in the Team Lead's folder, `chat::lead_file_path`), or one inside its copies of the projects' code (`code`),
/// never something it found elsewhere on the disk (keys, credentials). The copies hold only tracked files, so a
/// repository's .env isn't among them.
fn allowed_attachment(cx: &Cx, raw: &str, path: &std::path::Path) -> Result<(), String> {
    let real = path.canonicalize().map_err(|_| format!("can't read {raw}"))?;
    let in_copy = crate::code::dirs(cx.st).iter()
        .filter_map(|d| d.canonicalize().ok())
        .any(|d| real.starts_with(&d));
    if in_copy {
        return Ok(());
    }
    let named = cx.thread.is_some_and(|t| {
        gizai_core::chat::messages(cx.db(), t).unwrap_or_default().iter()
            .filter(|m| m.role == "user")
            .filter_map(|m| m.body_md.as_deref())
            .any(|b| b.contains(raw) || b.contains(&*real.to_string_lossy()) || b.contains(&*path.to_string_lossy()))
    });
    let added = cx.thread.is_some_and(|t| {
        gizai_core::chat::thread_files(cx.db(), t).unwrap_or_default().iter()
            .filter_map(|f| crate::chat::lead_file_path(cx.st, f).canonicalize().ok())
            .any(|p| p == real)
    });
    if named || added {
        Ok(())
    } else {
        Err(format!("I can only attach a file you named in this chat, one you added to a message here, or one inside my copies of the projects' code; {raw} is none of these. Ask the user to give the path."))
    }
}

/// Chat only: starts the update of a project's linked folder to main (`code::start_update`), after the user said yes in
/// the chat. Only the project's linked folder can be updated; the result comes later as a system message in the chat.
pub(crate) async fn update_checkout(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    let Some(thread) = cx.thread else {
        return Err("update_checkout works only in chat, after the user said yes there".into());
    };
    let p = resolve::project(cx, &a.req("project")?)?;
    let linked = p.repo_path.clone().filter(|r| !r.trim().is_empty())
        .ok_or_else(|| format!("{} has no linked folder to update", p.key))?;
    if let Some(f) = a.opt("folder") {
        let real = |s: &str| std::path::Path::new(s).canonicalize().ok();
        if real(&f).is_none() || real(&f) != real(&linked) {
            return Err(format!("update_checkout only updates {}'s linked folder ({linked}); {f} isn't it, so nothing changed", p.key));
        }
    }
    // Not a folder that the Team Lead's own folders (agent form → Folders) set to read.
    crate::folders::lead_may_update(cx.st, std::path::Path::new(&linked))?;
    let will = crate::code::start_update(cx.st, thread, &p, a.flag("switch").unwrap_or(false)).await?;
    Ok(json!({"ok": true, "done": "started", "folder": linked, "will": will,
              "result": "comes as a message in this chat when the update ends",
              "link": link("project", &p.id, &format!("{} ({})", p.name, p.key))}))
}

pub(crate) fn add_person(cx: &Cx, a: &Args) -> Result<Value, String> {
    let id = users::create(cx.db(), cx.actor, &a.req("name")?, a.opt("email").as_deref()).map_err(err)?;
    cx.changed("actors");
    Ok(json!({"ok": true, "person": {"id": id, "name": a.req("name")?}}))
}
