//! From what the model writes ("KADE-12", "kade portal", "backend agent", "me") to one row. An id or an exact
//! name wins; then a unique prefix; then a unique partial match. More than one candidate is never guessed:
//! the error lists them. Nothing found: the error says what exists.
use gizai_core::model::{Client, Doc, Project, Task, TaskFilter};
use gizai_core::team::{Member, Team, WorkflowState};
use gizai_core::{clients, docs, projects, tasks, team, users};

use super::{Cx, err};

pub(crate) struct Cand {
    pub id: String,
    pub names: Vec<String>,
    pub show: String,
}

/// The index of the one candidate `r` names. `what`/`plural` word the errors; `hint` says how to be exact.
pub(crate) fn pick(what: &str, plural: &str, hint: &str, r: &str, cands: &[Cand]) -> Result<usize, String> {
    let r = r.trim();
    if r.is_empty() {
        return Err(format!("say which {what}"));
    }
    if let Some(i) = cands.iter().position(|c| c.id == r) {
        return Ok(i);
    }
    let q = r.to_lowercase();
    let tiers: [&dyn Fn(&str) -> bool; 3] = [&|n: &str| n == q, &|n: &str| n.starts_with(&q), &|n: &str| n.contains(&q)];
    for t in tiers {
        let hits: Vec<usize> = (0..cands.len()).filter(|&i| cands[i].names.iter().any(|n| t(&n.to_lowercase()))).collect();
        match hits.len() {
            0 => continue,
            1 => return Ok(hits[0]),
            n => {
                let shown: Vec<&str> = hits.iter().take(10).map(|&i| cands[i].show.as_str()).collect();
                return Err(format!("\"{r}\" matches {n} {plural}: {}. Use the {hint}.", shown.join(", ")));
            }
        }
    }
    if cands.is_empty() {
        return Err(format!("No {what} called \"{r}\": there are no {plural} yet."));
    }
    let mut names: Vec<&str> = cands.iter().take(12).map(|c| c.show.as_str()).collect();
    if cands.len() > 12 {
        names.push("…");
    }
    Err(format!("No {what} called \"{r}\". {}{}: {}.", plural[..1].to_uppercase(), &plural[1..], names.join(", ")))
}

pub(crate) fn client(cx: &Cx, r: &str) -> Result<Client, String> {
    let mut list = clients::list(cx.db()).map_err(err)?;
    let cands: Vec<Cand> = list.iter().map(|c| Cand { id: c.id.clone(), names: vec![c.name.clone()], show: c.name.clone() }).collect();
    let i = pick("client", "clients", "full name", r, &cands)?;
    Ok(list.swap_remove(i))
}

/// A project by id, exact key or exact name (ignoring case), else by the start or a part of its name. One rule for keys
/// and names: when the reference is one project's key and another project's name, it is ambiguous and the error lists
/// both, with their ids. A key is exact or nothing: "KA" never picks KADE by prefix.
pub(crate) fn project(cx: &Cx, r: &str) -> Result<Project, String> {
    let mut list = projects::list(cx.db()).map_err(err)?;
    let q = r.trim();
    if let Some(i) = list.iter().position(|p| p.id == q) {
        return Ok(list.swap_remove(i));
    }
    let exact: Vec<usize> = (0..list.len()).filter(|&i| list[i].key.eq_ignore_ascii_case(q) || list[i].name.eq_ignore_ascii_case(q)).collect();
    match exact.len() {
        0 => {}
        1 => return Ok(list.swap_remove(exact[0])),
        n => {
            let shown: Vec<String> = exact.iter().map(|&i| format!("{} (key {}, id {})", list[i].name, list[i].key, list[i].id)).collect();
            return Err(format!("\"{q}\" matches {n} projects: {}. Use the id of the one you mean.", shown.join("; ")));
        }
    }
    let cands: Vec<Cand> = list.iter().map(|p| Cand { id: p.id.clone(), names: vec![p.name.clone()], show: format!("{} ({})", p.name, p.key) }).collect();
    let i = pick("project", "projects", "key", r, &cands)?;
    Ok(list.swap_remove(i))
}

pub(crate) fn task(cx: &Cx, r: &str) -> Result<Task, String> {
    let r = r.trim();
    let all = tasks::list(cx.db(), &TaskFilter::default()).map_err(err)?;
    if let Some(t) = all.iter().find(|t| t.id == r || t.identifier.eq_ignore_ascii_case(r)) {
        return tasks::get(cx.db(), &t.id).map_err(err);
    }
    // An archived card is found by its identifier (or id) only, never by its title.
    if let Some(t) = tasks::archived(cx.db(), None).map_err(err)?.into_iter().find(|t| t.id == r || t.identifier.eq_ignore_ascii_case(r)) {
        return tasks::get(cx.db(), &t.id).map_err(err);
    }
    // Looks like an identifier: say so plainly instead of matching titles.
    let looks_like_id = r.split_once('-').is_some_and(|(k, n)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric()) && n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty());
    if looks_like_id {
        return Err(format!("No task {}: check the identifier with list_tasks.", r.to_uppercase()));
    }
    let cands: Vec<Cand> = all.iter().map(|t| Cand { id: t.id.clone(), names: vec![t.title.clone()], show: format!("{} {}", t.identifier, super::short(&t.title, 40)) }).collect();
    let i = pick("task", "tasks", "identifier, like KADE-12", r, &cands)?;
    tasks::get(cx.db(), &all[i].id).map_err(err)
}

pub(crate) fn agent(cx: &Cx, r: &str) -> Result<Member, String> {
    let mut list: Vec<Member> = team::all_agents(cx.db()).map_err(err)?.into_iter().map(|(_, m)| m).collect();
    let cands: Vec<Cand> = list.iter().map(|m| Cand { id: m.actor_id.clone(), names: vec![m.name.clone(), m.handle.clone()], show: m.name.clone() }).collect();
    let i = pick("agent", "agents", "full name", r, &cands)?;
    Ok(list.swap_remove(i))
}

/// A person or an agent to assign: "me"/"you"/"I" is the user. Returns the actor id.
pub(crate) fn assignee(cx: &Cx, r: &str) -> Result<String, String> {
    if ["me", "you", "i", "myself", "user"].contains(&r.trim().to_lowercase().as_str()) {
        return Ok(cx.st.you_id.clone());
    }
    let mut cands: Vec<Cand> = users::list(cx.db()).map_err(err)?.into_iter()
        .map(|p| Cand { id: p.id, names: vec![p.name.clone(), p.handle], show: p.name }).collect();
    cands.extend(team::all_agents(cx.db()).map_err(err)?.into_iter()
        .map(|(_, m)| Cand { id: m.actor_id, names: vec![m.name.clone(), m.handle], show: m.name }));
    let i = pick("person or agent", "people and agents", "full name", r, &cands)?;
    Ok(cands.swap_remove(i).id)
}

/// The team a project's tasks live in (its own, else the first team).
pub(crate) fn team_of(cx: &Cx, project: Option<&Project>) -> Result<Team, String> {
    let id = match project.and_then(|p| p.team_id.clone()) {
        Some(t) => t,
        None => team::list(cx.db()).map_err(err)?.first().map(|t| t.id.clone()).ok_or("there is no team")?,
    };
    team::get(cx.db(), &id).map_err(err)
}

/// A column by its name, ignoring case ("to do", "in_progress" for In progress). An unknown name is an error that lists
/// the team's columns: never a guess by category.
pub(crate) fn column(team: &Team, r: &str) -> Result<WorkflowState, String> {
    let q = r.trim().to_lowercase().replace('_', " ");
    let found = team.states.iter().find(|s| s.name.to_lowercase() == q);
    found.cloned().ok_or_else(|| {
        let names: Vec<&str> = team.states.iter().map(|s| s.name.as_str()).collect();
        format!("No column called \"{}\". Columns: {}.", r.trim(), names.join(", "))
    })
}

/// Label names → ids (exact names, ignoring case: labels are few and short). Any existing label; an unknown one is an
/// error that says how to add it.
pub(crate) fn labels(team: &Team, names: &[String]) -> Result<Vec<String>, String> {
    names.iter().map(|n| {
        team.labels.iter().find(|l| l.name.eq_ignore_ascii_case(n.trim())).map(|l| l.id.clone()).ok_or_else(|| {
            let all: Vec<&str> = team.labels.iter().map(|l| l.name.as_str()).collect();
            let known = if all.is_empty() { "There are no labels yet.".to_string() } else { format!("Labels: {}.", all.join(", ")) };
            format!("there is no label {}; add it with save_label. {known}", n.trim())
        })
    }).collect()
}

/// A doc by id or title, in one project or across all of them.
pub(crate) fn doc(cx: &Cx, r: &str, project: Option<&Project>) -> Result<Doc, String> {
    let projects: Vec<Project> = match project {
        Some(p) => vec![p.clone()],
        None => projects::list(cx.db()).map_err(err)?,
    };
    let mut all: Vec<(Doc, String)> = vec![];
    for p in &projects {
        for d in docs::list(cx.db(), &p.id).map_err(err)? {
            all.push((d, p.key.clone()));
        }
    }
    let cands: Vec<Cand> = all.iter().map(|(d, key)| Cand { id: d.id.clone(), names: vec![d.title.clone()], show: format!("{} ({key})", d.title) }).collect();
    let i = pick("doc", "docs", "title and the project", r, &cands)?;
    docs::get(cx.db(), &all[i].0.id).map_err(err)
}
