//! First start: the organisation, the user, one team with the seven workflow columns and four labels, and on a new
//! install (`ensure_seed_with_agents`) five agents ready to work. Each role's starting instructions (`role_template`)
//! and allowed commands (`role_tools`).
use crate::clis::Cli;
use crate::db::{Db, Writer};
use crate::model::AgentInput;
use crate::{ids, Result};
use rusqlite::OptionalExtension;
use serde_json::json;

#[derive(Debug, Clone, PartialEq)]
pub struct SeedIds {
    pub org_id: String,
    pub you_id: String,
    pub team_id: String,
}

/// (name, category, auto) in board order. Each links to the next one, except Backlog and Done.
pub const DEFAULT_COLUMNS: [(&str, &str, bool); 7] = [
    ("Backlog", "backlog", false),
    ("To do", "ready", true),
    ("In progress", "in_progress", true),
    ("Testing", "testing", true),
    ("Review", "review", false),
    ("Deploy", "deploy", false),
    ("Done", "done", false),
];

pub const DEFAULT_LABELS: [(&str, &str); 4] =
    [("frontend", "#7b9bff"), ("backend", "#f29a4a"), ("qa", "#a98bfa"), ("bug", "#f2706b")];

/// The agents a new install starts with, as (name, role), in this order: the Team Lead (with Chat), the two builders,
/// QA and DevOps. Each lands on its role's usual columns (`columns::place_new_agent`).
pub const DEFAULT_AGENTS: [(&str, &str); 5] =
    [("Team Lead", "lead"), ("Backend Agent", "backend"), ("Frontend Agent", "frontend"), ("QA Agent", "qa"), ("DevOps Agent", "devops")];

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

/// Seeds an empty database (the organisation, you, a team with its columns, the labels) without agents; a database
/// seeded before is left as it is. Tests and the demo data start here; a new install starts with
/// `ensure_seed_with_agents`.
pub fn ensure_seed(db: &Db, you_name: &str) -> Result<SeedIds> {
    match seeded(db)? {
        Some(ids) => Ok(ids),
        None => db.write(None, |w| seed_in(w, you_name)),
    }
}

/// A new install's first start: `ensure_seed` plus the five agents of `DEFAULT_AGENTS`, in the same write. Each gets its
/// role's instructions and allowed commands, its CLI's own model and effort, acceptEdits, one card at once, no budget,
/// no MCP servers and no board check; the Team Lead gets Chat.
///
/// `codex` is asked only on that first start, for Codex when Claude Code isn't installed but Codex is. Codex is then
/// added under Settings → Coding CLIs and the Backend, Frontend, QA and DevOps agents run on it, in its own sandbox
/// (workspace-write: Codex has no acceptEdits); the Team Lead stays on Claude Code, where chat runs. A database seeded
/// before gets nothing new.
pub fn ensure_seed_with_agents(db: &Db, you_name: &str, codex: impl FnOnce() -> Option<Cli>) -> Result<SeedIds> {
    if let Some(ids) = seeded(db)? {
        return Ok(ids);
    }
    let codex = codex();
    db.write(None, |w| {
        let ids = seed_in(w, you_name)?;
        add_default_agents(w, &ids, codex)?;
        Ok(ids)
    })
}

/// The seed's ids, once the database is seeded.
fn seeded(db: &Db) -> Result<Option<SeedIds>> {
    db.read(|c| {
        let seeded: Option<String> =
            c.query_row("SELECT value_json FROM settings WHERE key='seeded' AND org_id=''", [], |r| r.get(0)).optional()?;
        if seeded.is_none() {
            return Ok(None);
        }
        Ok(Some(SeedIds {
            org_id: c.query_row("SELECT id FROM orgs ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
            you_id: c.query_row("SELECT id FROM actors WHERE kind='person' ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
            team_id: c.query_row("SELECT id FROM teams ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
        }))
    })
}

fn seed_in(w: &Writer, you_name: &str) -> Result<SeedIds> {
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
}

/// The five agents of a new install (see `ensure_seed_with_agents`), reporting to you.
fn add_default_agents(w: &Writer, ids: &SeedIds, codex: Option<Cli>) -> Result<()> {
    let codex = match codex {
        Some(cli) => crate::clis::save_in(w, vec![cli])?.into_iter().next(),
        None => None,
    };
    for (name, role) in DEFAULT_AGENTS {
        // Chat runs only on Claude Code: the Team Lead stays there.
        let cli = match &codex {
            Some(c) if role != "lead" => c.clone(),
            _ => crate::clis::claude_code(),
        };
        let on_claude = cli.kind == "claude_code";
        crate::team::add_agent_in(w, &ids.you_id, &ids.team_id, cli, AgentInput {
            name: name.into(), role_key: role.into(), instructions_md: Some(role_template(role)), allowed_tools: role_tools(role),
            // Empty: the CLI's own sandbox (Codex has no acceptEdits).
            permission_mode: if on_claude { "acceptEdits".into() } else { String::new() },
            chat_enabled: (role == "lead").then_some(true), max_runs: Some(1),
            ..Default::default()
        })?;
    }
    Ok(())
}

/// The seven default columns, used by the seed and by "New team": Backlog, To do, In progress, Testing, Review, Deploy
/// and Done, each linked to the next (Backlog and Done unlinked), with To do, In progress and Testing on Auto.
pub fn insert_default_columns(w: &crate::db::Writer, team_id: &str, now: i64) -> Result<()> {
    let ids: Vec<String> = DEFAULT_COLUMNS.iter().map(|_| ids::new_id()).collect();
    // Inserted last to first, so each column's next one exists already.
    for (i, (name, category, auto)) in DEFAULT_COLUMNS.iter().enumerate().rev() {
        let next = (*category != "backlog" && *category != "done").then(|| ids.get(i + 1)).flatten();
        w.conn().execute(
            "INSERT INTO workflow_states(id, created_at, updated_at, team_id, name, category, sort_key, auto, next_state_id)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![ids[i], now, team_id, name, category, format!("a{i}"), *auto as i64, next],
        )?;
    }
    for (i, (name, category, auto)) in DEFAULT_COLUMNS.iter().enumerate() {
        w.insert("workflow_states", &ids[i], json!({"name": name, "category": category, "auto": auto}))?;
    }
    Ok(())
}

/// The Backend, Frontend, QA and DevOps Agents' instructions, word for word (`roles/`).
const BACKEND: &str = include_str!("../roles/backend.md");
const FRONTEND: &str = include_str!("../roles/frontend.md");
const QA: &str = include_str!("../roles/qa.md");
const DEVOPS: &str = include_str!("../roles/devops.md");

/// Starting instructions for an agent with this role (`lead`, `frontend`, `backend`, `design`, `qa`, `devops`, or any
/// other key). Prefills the agent form, is the fallback in the run prompt and is what a new install's agents get.
/// Design gets the Frontend Agent's text and any other role the Backend Agent's, each with its own name and job.
pub fn role_template(role: &str) -> String {
    if role == "lead" {
        let result = "When you finish, end your final message with exactly one line:\n\
             GIZAI_RESULT: {\"outcome\":\"<outcome>\",\"summary\":\"<one paragraph for the task comment>\",\"issues\":[]}\n";
        return format!(
            "You are the Team Lead in Gizai's Software team.\n\
             You talk with the user on Gizai's Chat page and run the team's work for them with Gizai's tools:\n\
             - Turn requests into clients, projects and tasks; give every task a clear description and acceptance criteria.\n\
             - Hand code work to the developer agents as tasks: a card in an Auto column is picked up by the agents on that column, and assigning an agent makes only that agent start it. Labels are tags for people; they don't route. Don't write code yourself.\n\
             - Set up and adjust agents when asked, and keep the board tidy.\n\
             - Say briefly what you changed, with task identifiers.\n\
             - Keep Gizai's Memory: save decisions with their reasons, the user's preferences and gotchas (memory_append to Team Lead/Notes, memory_write for a shared note), and move an agent's useful notes into a shared folder (memory_move). Don't copy what the repository or the board already say, and never store a secret. After you read something from outside Gizai (another MCP server, the web, the browser), propose the note and save it once the user has confirmed.\n\
             - Fold a DevOps agent's learned lines about a deploy (in its Agents/<name>/Notes, under Learned) into that project's Deployments/<KEY> note (type: deployment, project: <KEY>, applies_to: devops), or make the note from its summary when there is none yet.\n\
             - Look in memory (memory_search) before you ask the user.\n\
             When you are started on a task instead of in chat, plan it or split it into sub-tasks. Allowed outcomes: needs_decision.\n\
             {result}"
        );
    }
    match role {
        "backend" => BACKEND.to_string(),
        "frontend" => FRONTEND.to_string(),
        "qa" => QA.to_string(),
        "devops" => DEVOPS.to_string(),
        "design" => builder_as(FRONTEND, "Design", "You design the screens and flows a card asks for and build them as UI components and styles, \
             following the project's design system; explain your design decisions in the hand-over."),
        _ => {
            let mut c = role.chars();
            let name = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
            builder_as(BACKEND, &name, "You build what the card asks for.")
        }
    }
}

/// A builder's text (`BACKEND` or `FRONTEND`) for another role. Its first line names the agent and its job: "You are the
/// Backend Agent in Gizai's Software team. You build the server side of a card (…). You do not test …"; `name` and
/// `job` take their place.
fn builder_as(text: &str, name: &str, job: &str) -> String {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let after_job = first.splitn(3, ". ").nth(2).unwrap_or_default();
    format!("You are the {name} Agent in Gizai's Software team. {job} {after_job}\n{rest}")
}

/// Gizai's default allowed commands: an agent without a list of its own runs with these (`src-tauri`'s runs), and each
/// role's list starts from them (`role_tools`); the agent form has the same list (`src/lib/agents.ts`). The read-only
/// helpers near the end are the ones agents use in pipes; `sleep` lets an agent wait in the foreground (for CI, a release
/// or a deploy) between checks, as "How this run works" tells it.
pub const DEFAULT_TOOLS: [&str; 29] = [
    "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git add:*)", "Bash(git commit:*)", "Bash(git merge:*)", "Bash(npm:*)", "Bash(npx:*)", "Bash(composer:*)",
    "Bash(php:*)", "Bash(./vendor/bin/*)", "Bash(cargo:*)", "Bash(pytest:*)", "Bash(ls:*)", "Bash(cat:*)", "Bash(rg:*)",
    "Bash(head:*)", "Bash(tail:*)", "Bash(wc:*)", "Bash(sort:*)", "Bash(uniq:*)", "Bash(cut:*)", "Bash(diff:*)", "Bash(grep:*)", "Bash(jq:*)",
    "Bash(pwd:*)", "Bash(which:*)", "Bash(tree:*)", "Bash(sleep:*)",
];

/// What builders (every role but lead, qa and devops) may run on top of Gizai's default list: look around the repository,
/// push, pull and fetch their branch, and node, echo and printf.
const BUILDER_TOOLS: [&str; 11] = [
    "Bash(git show:*)", "Bash(git ls-files:*)", "Bash(git rev-parse:*)", "Bash(git branch --show-current:*)", "Bash(git remote -v:*)",
    "Bash(git push:*)", "Bash(git pull:*)", "Bash(git fetch:*)", "Bash(node:*)", "Bash(echo:*)", "Bash(printf:*)",
];

/// What QA may run on top of the builders' list: open and update the card's pull request on GitHub.
const QA_TOOLS: [&str; 4] = ["Bash(gh pr create:*)", "Bash(gh pr list:*)", "Bash(gh pr view:*)", "Bash(gh pr edit:*)"];

/// The DevOps Agent's own list: read the repository, GitHub's releases, runs and workflows, resolve and push a pull
/// request's merge, make a release, check that it compiles, and wait between checks.
const DEVOPS_TOOLS: [&str; 59] = [
    "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)", "Bash(git branch -r:*)", "Bash(git branch -a:*)",
    "Bash(git branch --show-current:*)", "Bash(git branch --contains:*)", "Bash(git tag:*)", "Bash(git describe:*)", "Bash(git rev-parse:*)",
    "Bash(git rev-list:*)", "Bash(git merge-base:*)", "Bash(git merge-tree:*)", "Bash(git ls-remote:*)", "Bash(git ls-files:*)", "Bash(git fetch:*)",
    "Bash(git pull:*)", "Bash(git clone:*)", "Bash(git remote -v:*)", "Bash(git remote get-url:*)", "Bash(git add:*)", "Bash(git commit:*)",
    "Bash(git merge:*)", "Bash(git checkout --ours:*)", "Bash(git checkout --theirs:*)", "Bash(git push:*)", "Bash(gh auth status:*)",
    "Bash(gh repo view:*)", "Bash(gh repo clone:*)", "Bash(gh pr create:*)", "Bash(gh pr list:*)", "Bash(gh pr view:*)", "Bash(gh pr checks:*)",
    "Bash(gh pr diff:*)", "Bash(gh pr merge:*)", "Bash(gh pr edit:*)", "Bash(gh pr update-branch:*)", "Bash(gh release create:*)",
    "Bash(gh release list:*)", "Bash(gh release view:*)", "Bash(gh run list:*)", "Bash(gh run view:*)", "Bash(gh workflow list:*)",
    "Bash(gh workflow view:*)", "Bash(gh variable list:*)", "Bash(gh variable get:*)", "Bash(gh secret list:*)",
    "Bash(npm install --package-lock-only:*)", "Bash(cargo update --workspace:*)", "Bash(cargo check:*)", "Bash(npm ci:*)", "Bash(npx tsc -b:*)",
    "Bash(ls:*)", "Bash(cat:*)", "Bash(rg:*)", "Bash(jq:*)", "Bash(sleep:*)", "Bash(date:*)",
];

/// The allowed commands an agent with this role starts with: in the agent form when its role is picked, from the Team
/// Lead's create_agent without a list, and on a new install's agents. Builders (Backend, Frontend, Design and any other
/// role) get Gizai's default list plus `BUILDER_TOOLS`, QA the builders' list plus `QA_TOOLS`, DevOps `DEVOPS_TOOLS` and
/// the Team Lead Gizai's default list. An agent's saved list is never changed by it.
pub fn role_tools(role: &str) -> Vec<String> {
    let list: Vec<&str> = match role {
        "lead" => DEFAULT_TOOLS.to_vec(),
        "devops" => DEVOPS_TOOLS.to_vec(),
        "qa" => DEFAULT_TOOLS.iter().chain(&BUILDER_TOOLS).chain(&QA_TOOLS).copied().collect(),
        _ => DEFAULT_TOOLS.iter().chain(&BUILDER_TOOLS).copied().collect(),
    };
    list.into_iter().map(String::from).collect()
}
