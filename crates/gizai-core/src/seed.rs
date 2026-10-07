//! First start: the organisation, the user, one team with the six workflow columns and four labels.
//! No agents and no routing rules: Jeffrey sets those up himself (revision 2026-10-06).
use crate::db::Db;
use crate::{ids, Result};
use rusqlite::OptionalExtension;
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub struct SeedIds {
    pub org_id: String,
    pub you_id: String,
    pub team_id: String,
}

/// (name, category, owner_role) in board order.
pub const DEFAULT_COLUMNS: [(&str, &str, Option<&str>); 6] = [
    ("Backlog", "backlog", None),
    ("To do", "ready", Some("implementer")),
    ("In progress", "in_progress", Some("implementer")),
    ("Testing", "testing", Some("qa")),
    ("Review", "review", Some("human")),
    ("Done", "done", None),
];

pub const DEFAULT_LABELS: [(&str, &str); 4] =
    [("frontend", "#7b9bff"), ("backend", "#f29a4a"), ("qa", "#a98bfa"), ("bug", "#f2706b")];

pub fn handle_for(name: &str) -> String {
    let h: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if h.is_empty() { "user".into() } else { h }
}

pub fn ensure_seed(db: &Db, you_name: &str) -> Result<SeedIds> {
    let seeded: Option<String> = db.read(|c| {
        Ok(c.query_row("SELECT value_json FROM settings WHERE key='seeded' AND org_id=''", [], |r| r.get(0))
            .optional()?)
    })?;
    if seeded.is_some() {
        return db.read(|c| {
            Ok(SeedIds {
                org_id: c.query_row("SELECT id FROM orgs ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
                you_id: c.query_row(
                    "SELECT id FROM actors WHERE kind='person' ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
                team_id: c.query_row("SELECT id FROM teams ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
            })
        });
    }
    db.write(None, |w| {
        let now = ids::now_ms();
        let c = w.conn();
        let org_id = ids::new_id();
        c.execute(
            "INSERT INTO orgs(id, created_at, updated_at, name, key) VALUES (?1, ?2, ?2, 'My company', 'GZ')",
            rusqlite::params![org_id, now],
        )?;
        w.insert("orgs", &org_id, json!({"name": "My company", "key": "GZ"}))?;

        let you_id = ids::new_id();
        let name = you_name.trim();
        let name = if name.is_empty() { "You" } else { name };
        let handle = handle_for(name);
        w.conn().execute(
            "INSERT INTO actors(id, created_at, updated_at, org_id, kind, name, handle) VALUES (?1, ?2, ?2, ?3, 'person', ?4, ?5)",
            rusqlite::params![you_id, now, org_id, name, handle],
        )?;
        w.insert("actors", &you_id, json!({"kind": "person", "name": name, "handle": handle}))?;

        let team_id = ids::new_id();
        w.conn().execute(
            "INSERT INTO teams(id, created_at, updated_at, org_id, name) VALUES (?1, ?2, ?2, ?3, 'My team')",
            rusqlite::params![team_id, now, org_id],
        )?;
        w.insert("teams", &team_id, json!({"name": "My team"}))?;
        w.conn().execute(
            "INSERT INTO team_members(team_id, actor_id, role_key, created_at) VALUES (?1, ?2, 'reviewer', ?3)",
            rusqlite::params![team_id, you_id, now],
        )?;
        w.insert("team_members", &format!("{team_id}:{you_id}"), json!({"role_key": "reviewer"}))?;

        insert_default_columns(w, &team_id, now)?;

        for (label, color) in DEFAULT_LABELS {
            let id = ids::new_id();
            w.conn().execute(
                "INSERT INTO labels(id, created_at, updated_at, org_id, name, color) VALUES (?1, ?2, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, now, org_id, label, color],
            )?;
            w.insert("labels", &id, json!({"name": label, "color": color}))?;
        }

        w.conn().execute(
            "INSERT INTO settings(key, org_id, value_json, updated_at) VALUES ('seeded', '', 'true', ?1)",
            [now],
        )?;
        Ok(SeedIds { org_id, you_id, team_id })
    })
}

/// The six default columns, used by the seed and by "New team".
pub fn insert_default_columns(w: &crate::db::Writer, team_id: &str, now: i64) -> Result<()> {
    for (i, (name, category, owner)) in DEFAULT_COLUMNS.iter().enumerate() {
        let id = ids::new_id();
        let sort_key = format!("a{i}");
        w.conn().execute(
            "INSERT INTO workflow_states(id, created_at, updated_at, team_id, name, category, owner_role, sort_key)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, now, team_id, name, category, owner, sort_key],
        )?;
        w.insert("workflow_states", &id, json!({"name": name, "category": category, "owner_role": owner}))?;
    }
    Ok(())
}

/// Starting instructions for an agent with this role (`lead`, `frontend`, `backend`, `design`, `qa`,
/// `devops`, or any other key). Prefills the agent form and is the fallback in the run prompt.
pub fn role_template(role: &str) -> String {
    let result = "When you finish, end your final message with exactly one line:\n\
         GIZAI_RESULT: {\"outcome\":\"<outcome>\",\"summary\":\"<one paragraph for the task comment>\",\"issues\":[]}\n";
    if role == "lead" {
        return format!(
            "You are the Team Lead in Gizai's Software team.\n\
             You talk with the user on Gizai's Chat page and run the team's work for them with Gizai's tools:\n\
             - Turn requests into clients, projects and tasks; give every task a clear description and acceptance criteria.\n\
             - Hand code work to the developer agents as tasks (label it frontend or backend so routing picks it up, or assign an agent); don't write code yourself.\n\
             - Set up and adjust agents when asked, and keep the board tidy.\n\
             - Say briefly what you changed, with task identifiers.\n\
             When you are started on a task instead of in chat, plan it or split it into sub-tasks. Allowed outcomes: needs_decision.\n\
             {result}"
        );
    }
    let name = match role {
        "qa" => "QA".to_string(),
        "devops" => "DevOps".to_string(),
        _ => {
            let mut c = role.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    };
    let rules = match role {
        "qa" => "Do not change application code; you may add or fix tests only. Check each acceptance criterion by running the test suites. Allowed outcomes: qa_pass, qa_fail (list numbered issues), needs_decision.",
        "design" => "Design the screens and flows the task asks for and build them as UI components and styles, following the project's design system; explain your design decisions in the summary. Add or update tests for what you build and run the project's test command. Allowed outcomes: ready_for_testing, needs_decision.",
        "devops" => "Work on build, CI, packaging and deployment scripts. Never deploy, push or touch production servers: prepare the change and describe how to roll it out in the summary. Run the project's test command. Allowed outcomes: ready_for_testing, needs_decision.",
        _ => "Implement the task and add or update tests. Run the project's test command. Allowed outcomes: ready_for_testing, needs_decision.",
    };
    format!(
        "You are the {name} Agent in Gizai's Software team.\n\
         Inputs: the task (title, description, acceptance criteria) and recent comments, below.\n\
         Work only inside the current git worktree. Commit your work with clear messages. Never push, merge or change branches.\n\
         {rules}\n\
         {result}"
    )
}
