//! What a call of a tool in `NOT_AFTER_OUTSIDE` would do, for a pending action (GA-102): one plain line, what it is
//! about and what changes, and the text it saves or adds. Worked out when you look at it, from the board and memory as
//! they are then; a call that can't work as things are now says why (`Preview::warning`). Reads only.
use gizai_core::memory;
use gizai_core::pending::{DiffLine, line_diff};
use serde::Serialize;
use serde_json::{Map, Value};

use super::{Args, Cx, err, resolve, short};

/// What a pending call does.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// One plain line about what it does: "Save a new version of the note Lessons/Otus".
    pub line: String,
    /// What it is about and what changes, in order.
    pub fields: Vec<Field>,
    /// The text it saves or adds (a note's, a continued run's note, an agent's instructions), exactly as given.
    pub text: Option<String>,
    /// Against the current version, line by line: a note memory_write saves over, an agent's instructions update_agent
    /// replaces.
    pub diff: Option<Vec<DiffLine>>,
    /// Why approving it fails as things are now (its usual message comes then), when Gizai can tell.
    pub warning: Option<String>,
}

/// A label and the value the call gives; `from`: the value now, for a value that changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub label: String,
    pub value: String,
    pub from: Option<String>,
}

fn field(label: &str, value: impl Into<String>) -> Field {
    Field { label: label.into(), value: value.into(), from: None }
}

fn change(label: &str, from: impl Into<String>, value: impl Into<String>) -> Field {
    Field { label: label.into(), value: value.into(), from: Some(from.into()) }
}

/// "—" for nothing.
fn or_none(v: Option<String>) -> String {
    v.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "—".into())
}

fn card(t: &gizai_core::model::Task) -> String {
    format!("{} {}", t.identifier, short(&t.title, 60))
}

/// The pending call `tool` with `args`, as the Team Lead `actor` in the chat `thread` would make it.
pub(crate) fn preview(st: &crate::AppState, actor: &str, thread: &str, tool: &str, args: &Value) -> Preview {
    let a = Args(args.as_object().cloned().unwrap_or_default());
    let cx = Cx { st, actor, thread: Some(thread), check: None, from: None };
    let p = match tool {
        "memory_write" => memory_write(&cx, &a),
        "memory_append" => memory_append(&cx, &a),
        "memory_move" => memory_move(&cx, &a),
        "start_agent_run" => start_run(&cx, &a),
        "continue_agent_run" => continue_run(&cx, &a),
        "create_agent" => create_agent(&cx, &a),
        "update_agent" => update_agent(&cx, &a),
        "set_agent_status" => set_agent_status(&cx, &a),
        "add_column" => add_column(&cx, &a),
        "set_column" => set_column(&cx, &a),
        "attach_file" => attach_file(&cx, &a),
        "update_checkout" => update_checkout(&cx, &a),
        "merge_pull_request" => merge_pull_request(&cx, &a),
        _ => Ok(Preview { line: format!("Call {tool}"), fields: plain_fields(&a.0), ..Default::default() }),
    };
    // A call whose own arguments are wrong still shows what it was called with, and why it fails.
    p.unwrap_or_else(|why| Preview { line: format!("Call {tool}"), fields: plain_fields(&a.0), warning: Some(why), ..Default::default() })
}

/// Every argument as given, for a call Gizai can't say more about.
fn plain_fields(args: &Map<String, Value>) -> Vec<Field> {
    args.iter().map(|(k, v)| field(&k.replace('_', " "), match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    })).collect()
}

/// The note `r` (path, title or id) as the Team Lead sees it, if it exists.
fn note(cx: &Cx, r: &str) -> Option<memory::Note> {
    let who = super::memory::who(cx).ok()?;
    memory::get(cx.db(), &who, r).ok()
}

/// The note memory_write would save over, found as it finds it: by its path (case ignored, `.md` allowed) or its id, never
/// by its title.
fn note_at(cx: &Cx, r: &str) -> Option<memory::Note> {
    let p = r.trim().trim_matches('/');
    let p = p.strip_suffix(".md").unwrap_or(p);
    memory::find(cx.db(), p).ok().flatten().or_else(|| note(cx, r).filter(|n| n.id == r.trim()))
}

/// memory_write and memory_append: which note a call is for, so a newer pending call for the same note replaces the
/// older one: its id when it exists, else its path in lower case.
pub(crate) fn note_key(st: &crate::AppState, actor: &str, thread: &str, tool: &str, args: &Map<String, Value>) -> Option<String> {
    let key = match tool {
        "memory_write" => "path",
        "memory_append" => "note",
        _ => return None,
    };
    let r = args.get(key)?.as_str()?.trim();
    let cx = Cx { st, actor, thread: Some(thread), check: None, from: None };
    let found =if tool == "memory_write" { note_at(&cx, r) } else { note(&cx, r) };
    Some(match found {
        Some(n) => n.id,
        None => {
            let p = r.trim_matches('/');
            p.strip_suffix(".md").unwrap_or(p).to_lowercase()
        }
    })
}

fn memory_write(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let path = a.req("path")?;
    let body = a.text("body_md").unwrap_or_default();
    let version = a.int("version")?;
    Ok(match note_at(cx, &path) {
        Some(n) => {
            let warning = match version {
                None | Some(0) => Some(format!("{} exists and the call gives no version, so approving fails and nothing is saved.", n.path)),
                Some(v) if v != n.current_version => Some(format!("{} changed since the Team Lead read it (version {v}; it is at version {} now), \
                                                                    so approving fails and nothing is saved.", n.path, n.current_version)),
                _ => None,
            };
            Preview { line: format!("Save a new version of the note {}", n.path),
                      fields: vec![field("Note", n.path.clone()), change("Version", n.current_version.to_string(), (n.current_version + 1).to_string())],
                      diff: Some(line_diff(&n.body_md, &body)), text: Some(body), warning }
        }
        None => Preview { line: format!("Make the note {path}"), fields: vec![field("Note", path.clone())], text: Some(body), ..Default::default() },
    })
}

fn memory_append(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let r = a.req("note")?;
    let heading = a.opt("heading");
    let (line, path) = match note(cx, &r) {
        Some(n) => (format!("Add to the note {}", n.path), n.path),
        None => (format!("Make the note {r} with this text"), r),
    };
    let mut fields = vec![field("Note", path)];
    fields.push(field("Where", match heading {
        Some(h) => format!("At the end of the section {h}"),
        None => "At the end of the note".into(),
    }));
    Ok(Preview { line, fields, text: Some(a.text("text").unwrap_or_default()), ..Default::default() })
}

fn memory_move(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let r = a.req("note")?;
    let to = a.req("to")?;
    let copy = a.flag("copy").unwrap_or(false);
    let (from, warning) = match note(cx, &r) {
        Some(n) => (n.path, None),
        None => (r.clone(), Some(format!("There is no note {r}, so approving fails."))),
    };
    // A folder (ending in /) keeps the note's title.
    let to = if to.ends_with('/') { format!("{to}{}", memory::title_of(&from)) } else { to };
    let verb = if copy { "Copy" } else { "Move" };
    Ok(Preview { line: format!("{verb} the note {from} to {to}"), fields: vec![field("From", from), field("To", to)], warning, ..Default::default() })
}

fn start_run(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    // (who starts, as the Agent row shows it, why it fails)
    let (who, shown, warning) = match a.opt("agent") {
        Some(n) => {
            let name = resolve::agent(cx, &n)?.name;
            (name.clone(), name, None)
        }
        None => match crate::runs::suggest(cx.st, &t.id) {
            Some(id) => {
                let name = gizai_core::team::agent(cx.db(), &id).map_err(err)?.name;
                (name.clone(), format!("{name} (the card's agent)"), None)
            }
            None if t.hold.is_some() => ("an agent".to_string(), "none chosen".to_string(),
                                        Some(format!("{} is on hold, so approving fails unless the hold is cleared first.", t.identifier))),
            None => ("an agent".to_string(), "none".to_string(),
                     Some(format!("No agent is assigned to {} or on its column, so approving fails.", t.identifier))),
        },
    };
    Ok(Preview { line: format!("Start {who} on {}", t.identifier),
                 fields: vec![field("Card", card(&t)), field("Column", t.state_name.clone()), field("Agent", shown)], warning, ..Default::default() })
}

fn continue_run(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let last = gizai_core::runs::list_for_task(cx.db(), &t.id).map_err(err)?.into_iter().next();
    let note = a.opt("note");
    let mut fields = vec![field("Card", card(&t))];
    let (line, warning) = match &last {
        Some(r) => {
            fields.push(field("Agent", r.agent_name.clone()));
            fields.push(field("Its last run", format!("{}{}", r.status, r.outcome.as_ref().map(|o| format!(", {o}")).unwrap_or_default())));
            (format!("Continue {}'s run on {}", r.agent_name, t.identifier), None)
        }
        None => (format!("Continue the run on {}", t.identifier), Some(format!("{} has no run to continue, so approving fails.", t.identifier))),
    };
    if note.is_none() {
        fields.push(field("Note", "none"));
    }
    Ok(Preview { line, fields, text: note, warning, ..Default::default() })
}

/// The agent settings create_agent and update_agent take, by argument, with their labels (instructions come as text).
const AGENT_SETTINGS: [(&str, &str); 11] = [
    ("name", "Name"), ("role", "Role"), ("title", "Job title"), ("runs_on", "Runs on"), ("model", "Model"), ("effort", "Effort"),
    ("permission_mode", "Permission mode"), ("cards_at_once", "Cards at once"), ("monthly_budget_usd", "Monthly budget"),
    ("board_check_minutes", "Board check every"), ("shares_memory_with", "Shares memory with"),
];

/// The agent settings a call gives, as (argument, label, value), the way the agent form says them.
fn agent_fields(cx: &Cx, a: &Args) -> Vec<(&'static str, &'static str, String)> {
    let mut out = vec![];
    for (key, label) in AGENT_SETTINGS {
        let Some(v) = a.text(key) else { continue };
        let v = match key {
            "monthly_budget_usd" if !v.is_empty() => format!("${v}"),
            "board_check_minutes" if !v.is_empty() => if v == "0" { "off".into() } else { format!("{v} minutes") },
            // A coding CLI by its name, as Settings lists it.
            "runs_on" => gizai_core::clis::list(cx.db()).ok().and_then(|all| all.into_iter().find(|c| c.id == v || c.name.eq_ignore_ascii_case(&v)))
                .map(|c| c.name).unwrap_or(v),
            "shares_memory_with" if v.is_empty() || ["none", "own", "own folder", "its own folder"].contains(&v.to_lowercase().as_str()) =>
                "none (its own folder)".into(),
            _ => v,
        };
        out.push((key, label, v));
    }
    if let Some(list) = a.list("allowed_tools") {
        out.push(("allowed_tools", "Allowed commands", if list.is_empty() { "Gizai's default list".into() } else { list.join(", ") }));
    }
    out
}

fn create_agent(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let name = a.req("name")?;
    let role = a.req("role")?;
    // GA-225: the team it joins, which must be named when there is more than one.
    let t = resolve::team_arg(cx, a)?;
    let mut fields: Vec<Field> = team_field(cx, &t).into_iter().collect();
    fields.extend(agent_fields(cx, a).into_iter().map(|(_, label, v)| field(label, if v.is_empty() { "—".into() } else { v })));
    Ok(Preview { line: format!("Add the agent {name} ({role})"), fields, text: a.opt("instructions_md"), ..Default::default() })
}

/// The agent's setting `key` now, as the call would give it.
fn agent_now(cx: &Cx, m: &gizai_core::team::Member, key: &str) -> String {
    let name_of = |id: &str| gizai_core::team::agent(cx.db(), id).map(|x| x.name).unwrap_or_else(|_| id.to_string());
    match key {
        "name" => m.name.clone(),
        "role" => m.role_key.clone(),
        "title" => or_none(m.title.clone()),
        "runs_on" => crate::clis::of_agent(cx.st, m.adapter.as_deref()).map(|c| c.name).unwrap_or_else(|_| or_none(m.adapter.clone())),
        "model" => or_none(m.model.clone()),
        "effort" => or_none(m.effort.clone()),
        "permission_mode" => or_none(m.permission_mode.clone()),
        "cards_at_once" => m.max_runs.to_string(),
        "monthly_budget_usd" => m.budget_usd_micros.map(|b| format!("${}", b as f64 / 1e6)).unwrap_or_else(|| "—".into()),
        "board_check_minutes" => m.board_check_minutes.filter(|n| *n > 0).map(|n| format!("{n} minutes")).unwrap_or_else(|| "off".into()),
        "shares_memory_with" => m.shares_memory_with.as_deref().map(name_of).unwrap_or_else(|| "none (its own folder)".into()),
        "allowed_tools" => if m.allowed_tools.is_empty() { "Gizai's default list".into() } else { m.allowed_tools.join(", ") },
        _ => String::new(),
    }
}

fn update_agent(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let m = resolve::agent(cx, &a.req("agent")?)?;
    let mut fields = vec![];
    for (key, label, v) in agent_fields(cx, a) {
        let now = agent_now(cx, &m, key);
        let v = if v.is_empty() { "—".to_string() } else { v };
        if !now.eq_ignore_ascii_case(&v) {
            fields.push(change(label, now, v));
        }
    }
    let new_instructions = a.opt("instructions_md");
    let current = m.instructions_md.clone().unwrap_or_default();
    let instructions = new_instructions.filter(|i| i.trim() != current.trim());
    let warning = (fields.is_empty() && instructions.is_none()).then(|| format!("Nothing of {}'s settings would change.", m.name));
    if instructions.is_some() {
        fields.push(field("Instructions", "new text, below"));
    }
    Ok(Preview { line: format!("Change {}'s settings", m.name), fields,
                 diff: instructions.as_ref().map(|i| line_diff(&current, i)), text: instructions, warning })
}

fn set_agent_status(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let m = resolve::agent(cx, &a.req("agent")?)?;
    let status = a.req("status")?;
    let line = if status == "paused" { format!("Pause {}", m.name) } else { format!("Make {} active", m.name) };
    Ok(Preview { line, fields: vec![field("Agent", m.name.clone()), change("Status", m.status.clone(), status)], ..Default::default() })
}

/// "Auto" or "Manual".
fn mode(auto: bool) -> String {
    if auto { "Auto".into() } else { "Manual".into() }
}

/// Agent names as given, joined.
fn names(list: &[String]) -> String {
    if list.is_empty() { "none".into() } else { list.join(", ") }
}

/// GA-225: the team a call names, as a field, when there is more than one team to tell apart.
fn team_field(cx: &Cx, t: &gizai_core::team::Team) -> Option<Field> {
    let teams = gizai_core::team::list(cx.db()).unwrap_or_default();
    (teams.len() > 1).then(|| field("Team", t.name.clone()))
}

fn add_column(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let t = resolve::team_arg(cx, a)?;
    let name = a.req("name")?;
    let after = resolve::column(&t, &a.req("after")?)?;
    let kind = a.opt("kind").or_else(|| a.opt("category")).unwrap_or_default();
    let mut fields: Vec<Field> = team_field(cx, &t).into_iter().collect();
    fields.extend([field("Column", name.clone()), field("After", after.name.clone()), field("Kind", kind)]);
    if let Some(list) = a.list("agents") {
        fields.push(field("Agents", names(&list)));
    }
    fields.push(field("Agents pick up its cards", mode(a.flag("auto").unwrap_or(false))));
    if let Some(next) = a.opt("next") {
        fields.push(field("Next column", next));
    }
    Ok(Preview { line: format!("Add the column {name} after {}", after.name), fields, ..Default::default() })
}

fn set_column(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let t = resolve::team_arg(cx, a)?;
    let col = resolve::column(&t, &a.req("column")?)?;
    let column_name = |id: Option<&str>| id.and_then(|id| t.states.iter().find(|s| s.id == id)).map(|s| s.name.clone()).unwrap_or_else(|| "none".into());
    let mut fields: Vec<Field> = team_field(cx, &t).into_iter().collect();
    fields.push(field("Column", col.name.clone()));
    if let Some(n) = a.opt("new_name") {
        fields.push(change("Name", col.name.clone(), n));
    }
    if let Some(list) = a.list("agents") {
        let now: Vec<String> = col.agent_ids.iter().map(|id| gizai_core::team::agent(cx.db(), id).map(|m| m.name).unwrap_or_else(|_| id.clone())).collect();
        fields.push(change("Agents", names(&now), names(&list)));
    }
    if let Some(auto) = a.flag("auto") {
        fields.push(change("Agents pick up its cards", mode(col.auto), mode(auto)));
    }
    if let Some(next) = a.text("next") {
        let next = if next.is_empty() || next.eq_ignore_ascii_case("none") { "none".to_string() } else { next };
        fields.push(change("Next column", column_name(col.next_state_id.as_deref()), next));
    }
    if let Some(after) = a.opt("after") {
        fields.push(field("Moves to after", after));
    }
    Ok(Preview { line: format!("Change the column {}", col.name), fields, ..Default::default() })
}

fn attach_file(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let raw = a.req("path")?;
    let name = std::path::Path::new(&raw).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| raw.clone());
    let to = if let Some(r) = a.opt("task") {
        card(&resolve::task(cx, &r)?)
    } else if let Some(r) = a.opt("project") {
        let p = resolve::project(cx, &r)?;
        format!("the project {} ({})", p.name, p.key)
    } else if let Some(r) = a.opt("client") {
        format!("the client {}", resolve::client(cx, &r)?.name)
    } else {
        return Err("The call says nowhere to attach it (a task, project or client), so approving fails.".into());
    };
    let mut fields = vec![field("File", raw.clone())];
    // `~/…`, and on Windows `~\…` too, as attach_file reads it
    let expanded = match raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\").filter(|_| cfg!(windows))) {
        Some(rest) => std::path::PathBuf::from(gizai_core::clis::home()).join(rest),
        None => std::path::PathBuf::from(&raw),
    };
    let warning = match std::fs::metadata(&expanded) {
        Ok(m) if m.is_file() => { fields.push(field("Size", size(m.len()))); None }
        _ => Some(format!("Gizai can't read {raw} now, so approving fails.")),
    };
    fields.push(field("Goes to", to.clone()));
    Ok(Preview { line: format!("Attach {name} to {to}"), fields, warning, ..Default::default() })
}

/// 1536 → "1.5 KB".
fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} bytes"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f64 / 1024.0),
        b if b < 1024 * 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        b => format!("{:.1} GB", b as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

fn update_checkout(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let p = resolve::project(cx, &a.req("project")?)?;
    let folder = a.opt("folder").or_else(|| p.repo_path.clone()).unwrap_or_else(|| "none linked".into());
    let mut fields = vec![field("Project", format!("{} ({})", p.name, p.key)), field("Folder", folder)];
    if a.flag("stash").unwrap_or(false) {
        fields.push(field("Stash first", "yes: uncommitted changes in tracked files go to git's stash"));
    }
    if a.flag("switch").unwrap_or(false) {
        fields.push(field("Switch branch", format!("yes, to {} first", p.default_branch)));
    }
    Ok(Preview { line: format!("Update {}'s linked folder to {} as last fetched", p.key, p.default_branch), fields, ..Default::default() })
}

fn merge_pull_request(cx: &Cx, a: &Args) -> Result<Preview, String> {
    let t = resolve::task(cx, &a.req("task")?)?;
    let pr = match (&t.pr_url, &t.pr_state) {
        (Some(url), Some(state)) => format!("{url} ({state})"),
        (Some(url), None) => url.clone(),
        _ => "none Gizai knows of yet".into(),
    };
    Ok(Preview { line: format!("Merge {}'s pull request", t.identifier),
                 fields: vec![field("Card", card(&t)), field("Column", t.state_name.clone()), field("Pull request", pr)], ..Default::default() })
}

/// What an approved call did, from its result: a short sentence, like "Saved as version 4."
pub(crate) fn done(tool: &str, result: &Value) -> String {
    let s = |p: &str| result.pointer(p).and_then(Value::as_str).map(str::to_string);
    let n = |p: &str| result.pointer(p).and_then(Value::as_i64);
    match tool {
        "memory_write" | "memory_append" => match n("/note/version") {
            Some(v) => format!("Saved as version {v}."),
            None => "Saved.".into(),
        },
        "memory_move" => match (result.get("copied").and_then(Value::as_bool), s("/note/path")) {
            (Some(true), Some(p)) => format!("The copy is at {p}."),
            (_, Some(p)) => format!("It is at {p} now."),
            _ => "Done.".into(),
        },
        "start_agent_run" | "continue_agent_run" => match (s("/run/agent"), s("/run/branch")) {
            (Some(a), Some(b)) => format!("{a} is working on it, on branch {b}."),
            (Some(a), None) => format!("{a} is working on it."),
            _ => "The run started.".into(),
        },
        "create_agent" => format!("{} is on the team.", s("/agent/name").unwrap_or_else(|| "The agent".into())),
        "update_agent" => "Its settings are saved.".into(),
        "set_agent_status" => format!("{} is {} now.", s("/agent/name").unwrap_or_else(|| "The agent".into()), s("/agent/status").unwrap_or_default()),
        "add_column" | "set_column" => "The board's columns are saved.".into(),
        "attach_file" => format!("Attached {}.", s("/file/name").unwrap_or_else(|| "the file".into())),
        "update_checkout" => "The update started: its result comes as a message in this chat.".into(),
        "merge_pull_request" => match (n("/pull_request/number"), s("/card")) {
            // "moved to Deploy", or "still in Review: …"
            (Some(num), Some(card)) => format!("Merged pull request #{num}. The card {}{card}.", if card.starts_with("moved") { "" } else { "is " }),
            (Some(num), None) => format!("Merged pull request #{num}."),
            _ => "Merged.".into(),
        },
        _ => "Done.".into(),
    }
}
