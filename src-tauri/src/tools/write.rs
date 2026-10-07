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
           "labels": t.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(), "hold": t.hold})
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

// ---- guards: what the chat may not do, whatever it is asked ----

/// The Team Lead reads text other people and agents wrote, so it can't hand an agent unlimited powers: no
/// bypassPermissions, no "any command" (bare Bash). People set those in the agent form.
fn guard_agent_powers(a: &Args) -> Result<(), String> {
    if a.opt("permission_mode").as_deref() == Some("bypassPermissions") {
        return Err("bypassPermissions can't be set from chat: it lets an agent run anything. Set it yourself in the agent form if you really want it.".into());
    }
    for t in a.list("allowed_tools").unwrap_or_default() {
        let t = t.trim().to_lowercase().replace(' ', "");
        if t == "bash" || t == "bash(*)" || t == "bash(:*)" || t == "bash(*:*)" {
            return Err("an agent can't be allowed to run any command from chat; name the commands, like Bash(npm test:*) or Bash(git commit:*)".into());
        }
    }
    Ok(())
}

/// A repository path from chat must be a git repository (it has a .git folder or file), and never `/`, your
/// home folder or a folder above it: agents work there and the Team Lead may read it.
fn checked_repo(raw: &str) -> Result<String, String> {
    let bad = || format!("{raw} is not a git repository: give the folder that holds the project's .git");
    let p = std::path::Path::new(raw).canonicalize().map_err(|_| bad())?;
    let home = std::env::var("HOME").ok().and_then(|h| std::path::Path::new(&h).canonicalize().ok());
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
    let name = a.req("name")?;
    let repo_path = a.opt("repo_path").map(|r| checked_repo(&r)).transpose()?;
    let taken: Vec<String> = projects::list(cx.db()).map_err(err)?.into_iter().map(|p| p.key).collect();
    let key = match a.opt("key") {
        Some(k) => k.to_uppercase(),
        None => {
            let base = projects::suggest_key(&name);
            let mut k = base.clone();
            let mut n = 2;
            while taken.contains(&k) {
                k = format!("{}{n}", &base[..base.len().min(5)]);
                n += 1;
            }
            k
        }
    };
    let client_id = match a.opt("client") { Some(c) => Some(resolve::client(cx, &c)?.id), None => None };
    let id = projects::create(cx.db(), cx.actor, ProjectInput {
        client_id, name, key, status: a.opt("status"), goal_md: a.opt("goal_md"), repo_path,
        default_branch: a.opt("default_branch"), color: a.opt("color"), repo_url: a.opt("github"), ..Default::default()
    }).map_err(err)?;
    cx.changed("projects");
    let p = projects::get(cx.db(), &id).map_err(err)?;
    let label = format!("{} ({})", p.name, p.key);
    Ok(json!({"ok": true, "project": {"id": p.id, "key": p.key, "name": p.name, "number": p.number}, "link": link("project", &p.id, &label)}))
}

pub(crate) fn update_project(cx: &Cx, a: &Args) -> Result<Value, String> {
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
        repo_url: keep(a, "github", &cur.repo_url), ..Default::default()
    }).map_err(err)?;
    cx.changed("projects");
    let p = projects::get(cx.db(), &cur.id).map_err(err)?;
    let label = format!("{} ({})", p.name, p.key);
    Ok(json!({"ok": true, "project": {"id": p.id, "key": p.key, "name": p.name, "status": p.status, "repo_path": p.repo_path, "github": p.repo_url}, "link": link("project", &p.id, &label)}))
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
        state_id, priority: a.int("priority")?.unwrap_or(0), assignee_id, label_ids,
    }).map_err(err)?;
    task_done(cx, &id, "created")
}

pub(crate) fn update_task(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let mut patch = TaskPatch { title: a.opt("title"), description_md: a.text("description_md"), acceptance_md: a.text("acceptance_md"),
                                priority: a.int("priority")?, ..Default::default() };
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
        && patch.assignee_id.is_none() && patch.hold.is_none() && patch.hold_reason.is_none() && labels.is_none();
    if nothing {
        return Err("nothing to change: give a field to update".into());
    }
    tasks::update(cx.db(), cx.actor, &t.id, patch).map_err(err)?;
    if let Some(l) = labels {
        let team = resolve::team_of(cx, t.project_id.as_ref().and_then(|p| projects::get(cx.db(), p).ok()).as_ref())?;
        tasks::set_labels(cx.db(), cx.actor, &t.id, resolve::labels(&team, &l)?).map_err(err)?;
    }
    task_done(cx, &t.id, "updated")
}

pub(crate) fn move_task(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let project = t.project_id.as_ref().and_then(|p| projects::get(cx.db(), p).ok());
    let col = resolve::column(&resolve::team_of(cx, project.as_ref())?, &a.req("column")?)?;
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
    Ok(json!({"ok": true, "done": what, "agent": {"id": m.actor_id, "name": m.name, "role": m.role_key, "status": m.status, "wakeup": m.wakeup},
              "link": link("agent", &m.actor_id, &m.name)}))
}

pub(crate) async fn create_agent(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    guard_agent_powers(a)?;
    check_model(cx, a.opt("model").as_deref(), a.opt("effort").as_deref()).await?;
    let team_id = resolve::team_of(cx, None)?.id;
    let id = team::add_agent(cx.db(), cx.actor, &team_id, AgentInput {
        name: a.req("name")?, role_key: a.req("role")?, title: a.opt("title"), adapter: String::new(), model: a.opt("model"),
        instructions_md: a.opt("instructions_md"), permission_mode: a.opt("permission_mode").unwrap_or_default(),
        allowed_tools: a.list("allowed_tools").unwrap_or_default(), wakeup: a.opt("wakeup").unwrap_or_default(),
        heartbeat_minutes: a.int("heartbeat_minutes")?, budget_usd_micros: budget(a, None)?, chat_enabled: None, effort: a.opt("effort"),
        max_runs: a.int("cards_at_once")?,
    }).map_err(err)?;
    agent_result(cx, &id, "created")
}

pub(crate) async fn update_agent(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    guard_agent_powers(a)?;
    let m = resolve::agent(cx, &a.req("agent")?)?;
    let model = keep(a, "model", &m.model);
    let effort = keep(a, "effort", &m.effort);
    // Only Claude Code has a model list to check against; Codex, Gemini and other CLIs take their own model names.
    let on_claude = crate::clis::of_agent(cx.st, m.adapter.as_deref()).is_ok_and(|c| c.kind == "claude_code");
    if on_claude && (a.text("model").is_some() || a.text("effort").is_some()) {
        check_model(cx, model.as_deref(), effort.as_deref()).await?;
    }
    team::update_agent(cx.db(), cx.actor, &m.actor_id, AgentInput {
        name: a.opt("name").unwrap_or(m.name.clone()), role_key: a.opt("role").unwrap_or(m.role_key.clone()),
        title: keep(a, "title", &m.title), adapter: m.adapter.clone().unwrap_or_default(), model,
        instructions_md: a.opt("instructions_md"), permission_mode: a.opt("permission_mode").or(m.permission_mode.clone()).unwrap_or_default(),
        allowed_tools: a.list("allowed_tools").unwrap_or(m.allowed_tools.clone()),
        wakeup: a.opt("wakeup").or(m.wakeup.clone()).unwrap_or_default(),
        heartbeat_minutes: a.int("heartbeat_minutes")?.or(m.heartbeat_minutes),
        budget_usd_micros: budget(a, m.budget_usd_micros)?, chat_enabled: None, effort, max_runs: a.int("cards_at_once")?,
    }).map_err(err)?;
    agent_result(cx, &m.actor_id, "updated")
}

pub(crate) fn set_agent_status(cx: &Cx, a: &Args) -> Result<Value, String> {
    let m = resolve::agent(cx, &a.req("agent")?)?;
    team::set_agent_status(cx.db(), cx.actor, &m.actor_id, &a.req("status")?).map_err(err)?;
    agent_result(cx, &m.actor_id, "status changed")
}

pub(crate) fn add_rule(cx: &Cx, a: &Args) -> Result<Value, String> {
    let t = resolve::team_of(cx, None)?;
    team::add_rule(cx.db(), cx.actor, &t.id, RuleInput {
        kind: a.req("kind")?, match_name: a.req("match")?, target_role: a.req("role")?, priority: a.int("priority")?.unwrap_or(10),
    }).map_err(err)?;
    cx.changed("routing_rules");
    let t = resolve::team_of(cx, None)?;
    let rules: Vec<String> = t.rules.iter().map(|r| super::read::rule_sentence(r, &t)).collect();
    Ok(json!({"ok": true, "done": "rule added", "rules": rules, "link": {"page": "team", "id": t.id, "label": "Routing rules"}}))
}

pub(crate) async fn start_run(cx: &Cx<'_>, a: &Args) -> Result<Value, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let agent = match a.opt("agent") { Some(n) => Some(resolve::agent(cx, &n)?.actor_id), None => None };
    let (run_id, _done) = crate::runs::start(cx.st, &t.id, agent, None, "manual").await?;
    let run = gizai_core::runs::get(cx.db(), &run_id).map_err(err)?;
    Ok(json!({"ok": true, "done": "started", "run": {"id": run.id, "agent": run.agent_name, "branch": run.branch},
              "link": link("task", &t.id, &format!("{} {}", t.identifier, short(&t.title, 60)))}))
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
    let path = match raw.strip_prefix("~/") {
        Some(rest) => std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest),
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

/// Spec §8: the Team Lead attaches only a file the user named in this chat, or one inside a linked repository,
/// never something it found elsewhere on the disk (keys, credentials).
fn allowed_attachment(cx: &Cx, raw: &str, path: &std::path::Path) -> Result<(), String> {
    let real = path.canonicalize().map_err(|_| format!("can't read {raw}"))?;
    let in_repo = crate::chat::repo_dirs(cx.st).iter()
        .filter_map(|r| std::path::Path::new(r).canonicalize().ok())
        .any(|r| real.starts_with(&r));
    if in_repo {
        return Ok(());
    }
    let named = cx.thread.is_some_and(|t| {
        gizai_core::chat::messages(cx.db(), t).unwrap_or_default().iter()
            .filter(|m| m.role == "user")
            .filter_map(|m| m.body_md.as_deref())
            .any(|b| b.contains(raw) || b.contains(&*real.to_string_lossy()) || b.contains(&*path.to_string_lossy()))
    });
    if named {
        Ok(())
    } else {
        Err(format!("I can only attach a file you named in this chat or one inside a linked repository; {raw} is neither. Ask the user to give the path."))
    }
}

pub(crate) fn add_person(cx: &Cx, a: &Args) -> Result<Value, String> {
    let id = users::create(cx.db(), cx.actor, &a.req("name")?, a.opt("email").as_deref()).map_err(err)?;
    cx.changed("actors");
    Ok(json!({"ok": true, "person": {"id": id, "name": a.req("name")?}}))
}
