//! The Team Lead's tools: Gizai's clients, projects, tasks, agents, docs, files and inbox, served over MCP.
//! Each tool takes human references (KADE-12, "Kade portal", "Backend Agent"), calls gizai-core as the agent
//! (so activity shows who did it) and tells open screens what changed, exactly like the UI's commands.
mod read;
mod resolve;
mod write;

use serde_json::{Map, Value, json};

use crate::AppState;
use crate::runs::Note;
pub use gizai_mcp::ToolDef;

/// The MCP server's view of the tools for one agent (the token's owner), in one chat thread (the token's scope).
pub struct GizaiTools {
    pub st: AppState,
    pub actor: String,
    pub thread: Option<String>,
}

impl gizai_mcp::Tools for GizaiTools {
    fn list(&self) -> Vec<ToolDef> {
        catalog()
    }
    async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        call_in(&self.st, &self.actor, self.thread.as_deref(), name, args).await
    }
}

/// Runs one tool as `actor`, outside any chat thread. Errors are plain sentences for the model to act on.
pub async fn call(st: &AppState, actor: &str, name: &str, args: Value) -> Result<Value, String> {
    call_in(st, actor, None, name, args).await
}

/// Runs one tool as `actor` for a chat thread (what the user said there widens `attach_file`).
pub async fn call_in(st: &AppState, actor: &str, thread: Option<&str>, name: &str, args: Value) -> Result<Value, String> {
    let a = Args(match args {
        Value::Object(m) => m,
        Value::Null => Map::new(),
        _ => return Err("arguments must be an object".into()),
    });
    let cx = Cx { st, actor, thread };
    match name {
        "get_overview" => read::overview(&cx),
        "read_inbox" => read::inbox(&cx),
        "list_clients" => read::list_clients(&cx, &a),
        "get_client" => read::get_client(&cx, &a),
        "list_projects" => read::list_projects(&cx, &a),
        "get_project" => read::get_project(&cx, &a),
        "list_tasks" => read::list_tasks(&cx, &a),
        "get_task" => read::get_task(&cx, &a),
        "list_agents" => read::list_agents(&cx),
        "get_agent" => read::get_agent(&cx, &a),
        "list_docs" => read::list_docs(&cx, &a),
        "read_doc" => read::read_doc(&cx, &a),
        "list_people" => read::list_people(&cx),
        "get_workflow" => read::workflow(&cx),
        "create_client" => write::create_client(&cx, &a),
        "update_client" => write::update_client(&cx, &a),
        "save_contact" => write::save_contact(&cx, &a),
        "create_project" => write::create_project(&cx, &a),
        "update_project" => write::update_project(&cx, &a),
        "create_task" => write::create_task(&cx, &a),
        "update_task" => write::update_task(&cx, &a),
        "move_task" => write::move_task(&cx, &a),
        "comment_on_task" => write::comment(&cx, &a),
        "create_agent" => write::create_agent(&cx, &a).await,
        "update_agent" => write::update_agent(&cx, &a).await,
        "set_agent_status" => write::set_agent_status(&cx, &a),
        "add_routing_rule" => write::add_rule(&cx, &a),
        "start_agent_run" => write::start_run(&cx, &a).await,
        "stop_agent_run" => write::stop_run(&cx, &a),
        "create_doc" => write::create_doc(&cx, &a),
        "write_doc" => write::write_doc(&cx, &a),
        "attach_file" => write::attach_file(&cx, &a).await,
        "add_person" => write::add_person(&cx, &a),
        other => Err(format!("unknown tool {other}")),
    }
}

/// What every tool gets: the app and the acting agent.
pub(crate) struct Cx<'a> {
    pub st: &'a AppState,
    pub actor: &'a str,
    /// The chat thread the call comes from, if any.
    pub thread: Option<&'a str>,
}

impl Cx<'_> {
    pub fn db(&self) -> &gizai_core::db::Db {
        &self.st.db
    }
    pub fn changed(&self, table: &'static str) {
        (self.st.notify)(Note::RowsChanged(table));
    }
    /// After a card changes, an agent that wakes up on assignment may start on it.
    pub fn wake(&self, task_id: &str) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let (st, id) = (self.st.clone(), task_id.to_string());
        tokio::spawn(async move { crate::runs::dispatch(&st, &id).await; });
    }
}

pub(crate) fn err(e: gizai_core::Error) -> String {
    match e {
        gizai_core::Error::NotFound(what) => format!("{what} was not found"),
        other => other.to_string(),
    }
}

/// The tool's arguments, with the readers every tool uses.
pub(crate) struct Args(Map<String, Value>);

impl Args {
    /// A string that must be there and not empty.
    pub fn req(&self, k: &str) -> Result<String, String> {
        self.text(k).filter(|s| !s.is_empty()).ok_or_else(|| format!("missing \"{k}\""))
    }
    /// A string, trimmed; Some("") when given empty (an update clears it), None when absent.
    pub fn text(&self, k: &str) -> Option<String> {
        match self.0.get(k)? {
            Value::String(s) => Some(s.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            Value::Null => None,
            other => Some(other.to_string()),
        }
    }
    /// A string that is there and not empty.
    pub fn opt(&self, k: &str) -> Option<String> {
        self.text(k).filter(|s| !s.is_empty())
    }
    pub fn int(&self, k: &str) -> Result<Option<i64>, String> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f.round() as i64)).map(Some).ok_or_else(|| format!("\"{k}\" must be a whole number")),
            Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
            Some(Value::String(s)) => s.trim().parse().map(Some).map_err(|_| format!("\"{k}\" must be a whole number")),
            _ => Err(format!("\"{k}\" must be a whole number")),
        }
    }
    pub fn num(&self, k: &str) -> Result<Option<f64>, String> {
        match self.0.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
            Some(Value::String(s)) => s.trim().replace(',', ".").parse().map(Some).map_err(|_| format!("\"{k}\" must be a number")),
            _ => Err(format!("\"{k}\" must be a number")),
        }
    }
    pub fn flag(&self, k: &str) -> Option<bool> {
        match self.0.get(k)? {
            Value::Bool(b) => Some(*b),
            Value::String(s) => Some(matches!(s.trim().to_lowercase().as_str(), "true" | "yes" | "1")),
            _ => None,
        }
    }
    /// A list of strings: a JSON array, or one comma-separated string.
    pub fn list(&self, k: &str) -> Option<Vec<String>> {
        let items: Vec<String> = match self.0.get(k)? {
            Value::Array(v) => v.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_string())).collect(),
            Value::String(s) => s.split(',').map(|x| x.trim().to_string()).collect(),
            _ => return None,
        };
        Some(items.into_iter().filter(|s| !s.is_empty()).collect())
    }
}

/// A JSON Schema for a tool's arguments. Each property: (name, type, description); type is string,
/// integer, number, boolean, string[] or `enum:a|b|c`.
fn schema(props: &[(&str, &str, &str)], required: &[&str]) -> Value {
    let mut p = Map::new();
    for (name, ty, desc) in props {
        let v = match *ty {
            "string[]" => json!({"type": "array", "items": {"type": "string"}, "description": desc}),
            t if t.starts_with("enum:") => json!({"type": "string", "enum": t[5..].split('|').collect::<Vec<_>>(), "description": desc}),
            t => json!({"type": t, "description": desc}),
        };
        p.insert(name.to_string(), v);
    }
    json!({"type": "object", "properties": p, "required": required})
}

fn tool(name: &str, description: &str, props: &[(&str, &str, &str)], required: &[&str]) -> ToolDef {
    let read_only = name.starts_with("get_") || name.starts_with("list_") || name.starts_with("read_");
    ToolDef { name: name.into(), description: description.into(), input_schema: schema(props, required), read_only }
}

const TASK: (&str, &str, &str) = ("task", "string", "The task's identifier, like KADE-12 (or its id)");
const PROJECT: (&str, &str, &str) = ("project", "string", "The project's key (KADE), name or id");
const CLIENT: (&str, &str, &str) = ("client", "string", "The client's name or id");
const AGENT: (&str, &str, &str) = ("agent", "string", "The agent's name or id");

const CLIENT_FIELDS: [(&str, &str, &str); 14] = [
    ("legal_name", "string", "Registered name"), ("kind", "enum:company|person", "A company or a private person"),
    ("email", "string", "Email"), ("phone", "string", "Phone"), ("website", "string", "Website"),
    ("street", "string", "Street and number"), ("postal_code", "string", "Postal code"), ("city", "string", "City"),
    ("country", "string", "Country code, like NL"), ("vat_number", "string", "VAT number"), ("coc_number", "string", "Chamber of Commerce number"),
    ("iban", "string", "IBAN"), ("payment_terms_days", "integer", "Payment terms in days"), ("notes_md", "string", "Notes (Markdown)"),
];

const AGENT_FIELDS: [(&str, &str, &str); 10] = [
    ("model", "string", "A model Claude Code offers: an alias (default, opus, sonnet, haiku, fable) or its full id (claude-opus-5-5); empty = Claude Code's default"),
    ("effort", "enum:low|medium|high|xhigh|max", "How hard the model thinks (Claude Code --effort); empty = Claude Code's default. Higher costs more."),
    ("instructions_md", "string", "Instructions sent with every run (Markdown). Omit on create to use the role's template; it must end by asking for the GIZAI_RESULT line"),
    ("wakeup", "enum:manual|on_assign|heartbeat", "When it starts work by itself: manual (only Run), on_assign, or heartbeat"),
    ("heartbeat_minutes", "integer", "With wakeup heartbeat: every how many minutes it looks for its next card (1–1440)"),
    ("cards_at_once", "integer", "How many cards it works on at the same time, each in its own git worktree (1–10, default 1)"),
    ("permission_mode", "enum:acceptEdits|dontAsk|auto|plan|manual|bypassPermissions", "Claude Code permission mode for task runs (acceptEdits is the usual)"),
    ("allowed_tools", "string[]", "Commands it may run without asking, like Bash(npm test:*); empty = Gizai's default list"),
    ("monthly_budget_usd", "number", "Monthly spending cap in dollars; empty = no cap"),
    ("title", "string", "Job title shown on the team page"),
];

fn with<const N: usize>(head: &[(&'static str, &'static str, &'static str)], rest: [(&'static str, &'static str, &'static str); N]) -> Vec<(&'static str, &'static str, &'static str)> {
    head.iter().copied().chain(rest).collect()
}

/// Every tool with its description and argument schema.
pub fn catalog() -> Vec<ToolDef> {
    vec![
        tool("get_overview", "A summary of the organisation: clients, active projects, tasks per column, how many items wait in the inbox, the agents and who is working now. Start here.", &[], &[]),
        tool("read_inbox", "What needs the user: open tasks on hold (an agent or a gate needs a person) and tasks waiting in Review for them.", &[], &[]),
        tool("list_clients", "All clients with city, main contact and counts of projects and open tasks.", &[("status", "enum:lead|active|inactive", "Only clients with this status")], &[]),
        tool("get_client", "One client with every field, its contacts, projects and files.", &[CLIENT], &["client"]),
        tool("list_projects", "Projects with key, client, status, linked repository and task counts.", &[("client", "string", "Only this client's projects"), ("status", "enum:planned|active|paused|done|archived", "Only projects with this status")], &[]),
        tool("get_project", "One project with its goal, repository, tasks per column, docs and files.", &[PROJECT], &["project"]),
        tool("list_tasks", "Tasks, newest columns first. Done and cancelled tasks are left out unless include_done is true.",
             &[("project", "string", "Project key, name or id"), ("column", "string", "Column name, like To do or In progress"),
               ("assignee", "string", "A person or agent name; \"none\" for unassigned; \"me\" for the user"), ("label", "string", "Label name"),
               ("text", "string", "Words in the title"), ("include_done", "boolean", "Include done and cancelled tasks"), ("limit", "integer", "At most this many (default 50)")], &[]),
        tool("get_task", "One task with its description, acceptance criteria, labels, hold, latest comments, agent runs and files.", &[TASK], &["task"]),
        tool("list_agents", "The team's agents: role, status, wake-up, model, whether they work right now, spend this month and budget.", &[], &[]),
        tool("get_agent", "One agent's settings, instructions and recent runs.", &[AGENT], &["agent"]),
        tool("list_docs", "A project's docs (title, version, last change).", &[PROJECT], &["project"]),
        tool("read_doc", "A doc's current text (Markdown) and version.", &[("doc", "string", "The doc's title or id"), ("project", "string", "Project key or name, when titles repeat")], &["doc"]),
        tool("list_people", "The people in Gizai (the user and colleagues), with open task counts.", &[], &[]),
        tool("get_workflow", "The board's columns (with who works each), the labels and the routing rules that hand cards to agents.", &[], &[]),
        tool("create_client", "Adds a client.", &with(&[("name", "string", "Client name")], CLIENT_FIELDS), &["name"]),
        tool("update_client", "Changes a client. Only the fields given change; an empty string clears a field.",
             &with(&[CLIENT, ("name", "string", "New name"), ("status", "enum:lead|active|inactive", "Status")], CLIENT_FIELDS), &["client"]),
        tool("save_contact", "Adds a contact person to a client, or changes one (name it in contact).",
             &[CLIENT, ("name", "string", "Contact's name"), ("contact", "string", "An existing contact's name or id, to change it"),
               ("role", "string", "Job title"), ("email", "string", "Email"), ("phone", "string", "Phone"), ("is_primary", "boolean", "The client's main contact")], &["client", "name"]),
        tool("create_project", "Adds a project. The key (2–6 letters, used in task ids like KADE-12) is suggested from the name when omitted.",
             &[("name", "string", "Project name"), ("key", "string", "2–6 letters or digits, starting with a letter"), CLIENT,
               ("goal_md", "string", "The goal (Markdown)"), ("repo_path", "string", "Absolute path of the local git repository agents work in"),
               ("default_branch", "string", "The repository's main branch (default main)"),
               ("github", "string", "The repository on GitHub (https://github.com/owner/name); new cards then start from its main branch"),
               ("color", "string", "A colour like #7b9bff"),
               ("status", "enum:planned|active|paused|done|archived", "Status (default active)")], &["name"]),
        tool("update_project", "Changes a project. Only the fields given change; an empty string clears a field. The key can't change.",
             &[PROJECT, ("name", "string", "New name"), CLIENT, ("goal_md", "string", "The goal (Markdown)"),
               ("repo_path", "string", "Absolute path of the local git repository"), ("default_branch", "string", "Main branch"),
               ("github", "string", "The repository on GitHub (https://github.com/owner/name)"),
               ("color", "string", "A colour like #7b9bff"), ("status", "enum:planned|active|paused|done|archived", "Status")], &["project"]),
        tool("create_task", "Adds a task to a project. Labels frontend or backend let routing hand it to the matching agent.",
             &[PROJECT, ("title", "string", "Short title"), ("description_md", "string", "What to do and why (Markdown)"),
               ("acceptance_md", "string", "Acceptance criteria, as a Markdown checklist"), ("column", "string", "Column name (default the first, Backlog)"),
               ("priority", "integer", "0 none, 1 urgent, 2 high, 3 medium, 4 low"), ("assignee", "string", "A person or agent name; \"me\" for the user"),
               ("labels", "string[]", "Label names, like backend or bug")], &["project", "title"]),
        tool("update_task", "Changes a task's fields, labels, assignee or hold. Only what is given changes.",
             &[TASK, ("title", "string", "New title"), ("description_md", "string", "Description (Markdown)"), ("acceptance_md", "string", "Acceptance criteria (Markdown)"),
               ("priority", "integer", "0 none, 1 urgent, 2 high, 3 medium, 4 low"), ("assignee", "string", "A person or agent name; \"none\" to unassign"),
               ("labels", "string[]", "The full new set of label names"), ("hold", "enum:needs_decision|blocked|stalled", "Put it on hold"),
               ("hold_reason", "string", "Why it is on hold"), ("clear_hold", "boolean", "Take it off hold")], &["task"]),
        tool("move_task", "Moves a task to another column (to the bottom of that column).", &[TASK, ("column", "string", "Column name, like In progress")], &["task", "column"]),
        tool("comment_on_task", "Adds a comment to a task, as you.", &[TASK, ("body_md", "string", "The comment (Markdown)")], &["task", "body_md"]),
        tool("create_agent", "Adds a Claude Code agent to the team. It starts from the role's instructions unless instructions_md is given.",
             &with(&[("name", "string", "Agent name, like Frontend Agent"), ("role", "string", "Role key: lead, frontend, backend, design, qa, devops or your own")], AGENT_FIELDS), &["name", "role"]),
        tool("update_agent", "Changes an agent's settings. Only the fields given change.",
             &with(&[AGENT, ("name", "string", "New name"), ("role", "string", "Role key")], AGENT_FIELDS), &["agent"]),
        tool("set_agent_status", "Pauses an agent (no heartbeats, no new runs) or makes it active again.", &[AGENT, ("status", "enum:active|paused", "active or paused")], &["agent", "status"]),
        tool("add_routing_rule", "Adds a routing rule: a card with a label, or entering a column, goes to the first idle agent with a role.",
             &[("kind", "enum:label|column", "Match a label or a column"), ("match", "string", "The label or column name"),
               ("role", "string", "The role that takes the card"), ("priority", "integer", "Lower wins (default 10)")], &["kind", "match", "role"]),
        tool("start_agent_run", "Starts an agent on a task now (the given agent, else the assigned or routed one). The project needs a linked git repository.",
             &[TASK, ("agent", "string", "Agent name; omit to use the assigned or routed agent")], &["task"]),
        tool("stop_agent_run", "Stops the agent working on a task right now.", &[TASK], &["task"]),
        tool("create_doc", "Adds a doc to a project, optionally with its first text.", &[PROJECT, ("title", "string", "Doc title"), ("body_md", "string", "Text (Markdown)")], &["project", "title"]),
        tool("write_doc", "Saves new text for a doc as a new version (the whole text, not a diff).",
             &[("doc", "string", "The doc's title or id"), ("body_md", "string", "The complete new text (Markdown)"), ("project", "string", "Project key or name, when titles repeat")], &["doc", "body_md"]),
        tool("attach_file", "Copies a local file (absolute path, or ~/…) into Gizai and attaches it to one task, project or client.",
             &[("path", "string", "Absolute path of the file"), ("task", "string", "Task identifier"), ("project", "string", "Project key or name"), ("client", "string", "Client name")], &["path"]),
        tool("add_person", "Adds a person (a colleague or reviewer) to Gizai.", &[("name", "string", "Full name"), ("email", "string", "Email")], &["name"]),
    ]
}

/// A short label for links: at most `n` characters.
pub(crate) fn short(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

/// Unix ms → "2026-10-07" (UTC).
pub fn ymd(ms: i64) -> String {
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}
