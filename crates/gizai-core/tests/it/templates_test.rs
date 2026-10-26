//! GA-226 QA: team templates in core (`gizai_core::templates`, docs/team-template-format.md). Export (`save` writes format
//! 1 with every column and agent field, and nothing private: no folders, MCP server settings, CLI ids, people, cards or
//! status; archived agents left out, paused ones kept); `check_export`'s warnings (email, path, url, secret, naming the
//! agent; a clean team gives none); the built-in Software team equals a new install's first team; export then import from
//! the saved path; every refusal of the check (the version one word for word) makes nothing; names (a taken team name gets
//! a number, a taken agent name the team's in brackets); the CLI (a kind that isn't set up runs on Claude Code with
//! acceptEdits, word for word), permission modes and effort; `extraTools`; MCP servers; Chat; paused from a file, active
//! from the built-in; shared memory folders; labels; an import that fails partway leaves nothing behind; and a team made
//! from the built-in can be a project's team (GA-225).
use std::path::Path;

use gizai_core::clis::{self, Cli};
use gizai_core::db::Db;
use gizai_core::mcp_servers::{self, AgentServer, AgentTools, CliTools, McpServer};
use gizai_core::model::{ProjectInput, TaskInput};
use gizai_core::seed::{self, SeedIds};
use gizai_core::team::{self, Member, Team};
use gizai_core::templates::{self, FileAgent, FileColumn, FileLabel, FileServer, Plan, PlanAgent, Source, TemplateFile};
use gizai_core::{memory, projects, tasks};
use serde_json::{Value, json};

/// The fields a team template has, in the order format 1 writes them.
const TEAM_FIELDS: [&str; 10] = ["format", "format_version", "app_version", "name", "description_md", "color", "branches", "labels", "columns", "agents"];
const COLUMN_FIELDS: [&str; 7] = ["name", "kind", "auto", "next", "wip_limit", "color", "agents"];
/// Every agent field the card lists, and nothing else.
const AGENT_FIELDS: [&str; 24] = [
    "name", "role", "title", "cli", "model", "effort", "permission_mode", "instructions_md", "allowed_tools", "cards_at_once",
    "monthly_budget_usd", "chat", "board_check_minutes", "use_memory", "shares_memory_with", "web_search", "web_fetch", "fetch_domains",
    "builtin_tools", "slash_commands", "browser", "insecure_certs", "screenshots", "mcp_servers",
];
/// The tables an import writes to: a refusal or a failed import leaves each as it was.
const TABLES: [&str; 10] = ["teams", "team_members", "actors", "agent_configs", "workflow_states", "column_agents", "labels", "docs", "changes", "runs"];

// ---- set-up ----

/// A new install: you, the first team with its seven columns, the four labels and the five agents (the Team Lead with
/// Chat), all on the built-in Claude Code.
fn install() -> (Db, SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed_with_agents(&db, "Jeffrey Testperson", || None).unwrap();
    (db, s)
}

/// An install without agents (no Chat agent yet).
fn bare() -> (Db, SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey Testperson").unwrap();
    (db, s)
}

fn sql(db: &Db, q: &str, params: &[&dyn rusqlite::ToSql]) {
    db.write(None, |w| {
        w.conn().execute(q, params)?;
        Ok(())
    }).unwrap();
}

fn count(db: &Db, q: &str) -> i64 {
    db.read(|c| Ok(c.query_row(q, [], |r| r.get(0))?)).unwrap()
}

/// How many rows each table an import writes to has.
fn counts(db: &Db) -> Vec<(&'static str, i64)> {
    TABLES.iter().map(|t| (*t, count(db, &format!("SELECT count(*) FROM {t}")))).collect()
}

fn member<'a>(t: &'a Team, name: &str) -> &'a Member {
    t.members.iter().find(|m| m.kind == "agent" && m.name == name).unwrap_or_else(|| panic!("no agent {name} in {}", t.name))
}

fn state_id(t: &Team, name: &str) -> String {
    t.states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
}

fn agent_names_on(t: &Team, column: &str) -> Vec<String> {
    let s = t.states.iter().find(|s| s.name == column).unwrap();
    s.agent_ids.iter().map(|id| t.members.iter().find(|m| m.actor_id == *id).unwrap().name.clone()).collect()
}

fn team_named(db: &Db, name: &str) -> Option<String> {
    team::list(db).unwrap().into_iter().find(|t| t.name == name).map(|t| t.id)
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// An object's field names, sorted (serde_json may keep them in order or sort them, as the workspace's features have it).
fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}

fn sorted(v: &[&str]) -> Vec<String> {
    let mut k = strs(v);
    k.sort();
    k
}

fn builtin() -> Source {
    Source { builtin: Some("software".into()), ..Default::default() }
}

fn text(t: &str) -> Source {
    Source { json: Some(t.to_string()), ..Default::default() }
}

fn file(f: &TemplateFile) -> Source {
    text(&serde_json::to_string(f).unwrap())
}

fn path(p: &Path) -> Source {
    Source { path: Some(p.to_string_lossy().into_owned()), ..Default::default() }
}

fn agent(name: &str, role: &str) -> FileAgent {
    FileAgent {
        name: name.into(), role: role.into(), title: None, cli: "claude_code".into(), model: None, effort: None,
        permission_mode: Some("acceptEdits".into()), instructions_md: Some(format!("You are {name}.")), allowed_tools: seed::role_tools(role),
        cards_at_once: 1, monthly_budget_usd: None, chat: false, board_check_minutes: None, use_memory: true, shares_memory_with: None,
        web_search: false, web_fetch: false, fetch_domains: vec![], builtin_tools: vec![], slash_commands: false, browser: false,
        insecure_certs: false, screenshots: None, mcp_servers: vec![],
    }
}

fn column(name: &str, kind: &str, auto: bool, next: Option<&str>, agents: &[&str]) -> FileColumn {
    FileColumn { name: name.into(), kind: kind.into(), auto, next: next.map(String::from), wip_limit: None, color: None, agents: strs(agents) }
}

/// A small template: Backlog, Build (Auto → Check, the Builder), Check (Auto → Done, the Checker) and Done; a label
/// "infra".
fn small(name: &str) -> TemplateFile {
    TemplateFile {
        format: "gizai-team".into(), format_version: 1, app_version: "0.14.0".into(), name: name.into(), description_md: Some("A small team.".into()),
        color: Some("#123456".into()), branches: team::default_branches(),
        labels: vec![FileLabel { name: "infra".into(), color: Some("#00aa00".into()) }],
        columns: vec![
            column("Backlog", "backlog", false, None, &[]),
            column("Build", "in_progress", true, Some("Check"), &["Builder"]),
            column("Check", "testing", true, Some("Done"), &["Checker"]),
            column("Done", "done", false, None, &[]),
        ],
        agents: vec![agent("Builder", "backend"), agent("Checker", "qa")],
    }
}

fn plan_agent<'a>(p: &'a Plan, file_name: &str) -> &'a PlanAgent {
    p.agents.iter().find(|a| a.file_name == file_name).unwrap_or_else(|| panic!("no {file_name} in the plan"))
}

// ---- export ----

#[test]
fn save_writes_format_1_with_every_column_and_every_agent_field() {
    let (db, s) = install();
    let t = team::get(&db, &s.team_id).unwrap();
    let (be, fe, qa) = (member(&t, "Backend Agent").actor_id.clone(), member(&t, "Frontend Agent").actor_id.clone(), member(&t, "QA Agent").actor_id.clone());
    sql(&db, "UPDATE workflow_states SET wip_limit=3, color='#336699' WHERE id=?1", &[&state_id(&t, "To do")]);
    sql(&db, "UPDATE actors SET title='Rust lead' WHERE id=?1", &[&be]);
    sql(&db, "UPDATE agent_configs SET model='claude-opus-4-1', effort='high', budget_usd_micros=25500000, max_concurrent_runs=3, use_memory=0
              WHERE actor_id=?1", &[&be]);
    sql(&db, "UPDATE agent_configs SET budget_usd_micros=25000000 WHERE actor_id=?1", &[&qa]);
    let figma = mcp_servers::save(&db, McpServer { name: "figma".into(), transport: "stdio".into(), command: "figma-mcp".into(), ..Default::default() }).unwrap();
    mcp_servers::set_agent_tools(&db, &s.you_id, &fe, AgentTools { mcp: vec![
        AgentServer { server_id: figma.id.clone(), on: true, tools_off: strs(&["get_image"]) },
        AgentServer { server_id: mcp_servers::BROWSER.into(), on: true, tools_off: vec![] },
    ] }).unwrap();
    mcp_servers::set_cli_tools(&db, &s.you_id, &fe, CliTools {
        web_search: true, web_fetch: true, fetch_domains: strs(&["docs.rs"]), insecure_certs: true, builtin: strs(&["NotebookEdit"]), slash_commands: true,
    }).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("My team.gizai-team.json");
    templates::save(&db, &s.team_id, &p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    assert!(text.starts_with("{\n  \"format\": \"gizai-team\",\n  \"format_version\": 1,\n"), "indented JSON:\n{}", &text[..120]);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(keys(&v), sorted(&TEAM_FIELDS));
    let at: Vec<usize> = TEAM_FIELDS.iter().map(|f| text.find(&format!("\n  \"{f}\": ")).unwrap_or_else(|| panic!("{f}"))).collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "the team's fields, in order: {at:?}");
    assert_eq!(v["app_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(v["name"], t.name);

    // Every column, in board order, with kind, Auto, next (a name), WIP limit, colour and its agents (names, in order).
    for c in v["columns"].as_array().unwrap() {
        assert_eq!(keys(c), sorted(&COLUMN_FIELDS));
    }
    let col = |name: &str, kind: &str, auto: bool, next: Option<&str>, wip: Option<i64>, color: Option<&str>, agents: &[&str]|
        json!({"name": name, "kind": kind, "auto": auto, "next": next, "wip_limit": wip, "color": color, "agents": agents});
    assert_eq!(v["columns"], json!([
        col("Backlog", "backlog", false, None, None, None, &[]),
        col("To do", "ready", true, Some("In progress"), Some(3), Some("#336699"), &["Backend Agent", "Frontend Agent"]),
        col("In progress", "in_progress", true, Some("Testing"), None, None, &["Backend Agent", "Frontend Agent"]),
        col("Testing", "testing", true, Some("Review"), None, None, &["QA Agent"]),
        col("Review", "review", false, Some("Deploy"), None, None, &[]),
        col("Deploy", "deploy", false, Some("Done"), None, None, &["DevOps Agent"]),
        col("Done", "done", false, None, None, None, &[]),
    ]));

    // Every agent field the card lists, and nothing else.
    let agents = v["agents"].as_array().unwrap();
    assert_eq!(agents.iter().map(|a| a["name"].as_str().unwrap()).collect::<Vec<_>>(),
               ["Team Lead", "Backend Agent", "Frontend Agent", "QA Agent", "DevOps Agent"], "in the order they joined");
    for a in agents {
        assert_eq!(keys(a), sorted(&AGENT_FIELDS), "{}", a["name"]);
    }
    let a = |name: &str| agents.iter().find(|a| a["name"] == name).unwrap();
    let backend = a("Backend Agent");
    assert_eq!((&backend["role"], &backend["title"], &backend["cli"], &backend["model"], &backend["effort"], &backend["permission_mode"]),
               (&json!("backend"), &json!("Rust lead"), &json!("claude_code"), &json!("claude-opus-4-1"), &json!("high"), &json!("acceptEdits")));
    assert_eq!(backend["instructions_md"], seed::role_template("backend"));
    assert_eq!(backend["allowed_tools"], json!(seed::role_tools("backend")));
    assert_eq!((&backend["cards_at_once"], &backend["monthly_budget_usd"], &backend["use_memory"]), (&json!(3), &json!(25.5), &json!(false)));
    assert!(text.contains("\"monthly_budget_usd\": 25.5,") && text.contains("\"monthly_budget_usd\": 25,"), "25.5 as 25.5, 25 as 25");
    assert_eq!(a("QA Agent")["monthly_budget_usd"], json!(25));
    assert_eq!((&a("Team Lead")["chat"], &backend["chat"]), (&json!(true), &json!(false)));
    let frontend = a("Frontend Agent");
    assert_eq!(frontend["mcp_servers"], json!([{"name": "figma", "tools_off": ["get_image"]}]), "by name; the browser is its own switch");
    assert_eq!((&frontend["browser"], &frontend["web_search"], &frontend["web_fetch"], &frontend["fetch_domains"]),
               (&json!(true), &json!(true), &json!(true), &json!(["docs.rs"])));
    assert_eq!((&frontend["builtin_tools"], &frontend["slash_commands"], &frontend["insecure_certs"]), (&json!(["NotebookEdit"]), &json!(true), &json!(true)));
    assert_eq!((&backend["browser"], &backend["mcp_servers"], &backend["shares_memory_with"]), (&json!(false), &json!([]), &Value::Null));

    // The labels and branches as the team has them; save writes what export makes.
    assert_eq!(v["labels"], json!([{"name": "backend", "color": "#f29a4a"}, {"name": "bug", "color": "#f2706b"},
                                   {"name": "frontend", "color": "#7b9bff"}, {"name": "qa", "color": "#a98bfa"}]));
    assert_eq!(v["branches"], serde_json::to_value(team::default_branches()).unwrap());
    assert_eq!(serde_json::from_str::<TemplateFile>(&text).unwrap(), templates::export(&db, &s.team_id).unwrap());
}

#[test]
fn the_file_holds_no_folders_server_settings_cli_ids_people_cards_or_status() {
    let (db, s) = install();
    let t = team::get(&db, &s.team_id).unwrap();
    let (be, fe, qa, ops) = (member(&t, "Backend Agent").actor_id.clone(), member(&t, "Frontend Agent").actor_id.clone(),
                             member(&t, "QA Agent").actor_id.clone(), member(&t, "DevOps Agent").actor_id.clone());
    // A folder, an MCP server with its settings, a second Claude Code account, a card, a memory note and a paused agent.
    sql(&db, "UPDATE agent_configs SET folders_json=?2 WHERE actor_id=?1",
        &[&be, &r#"[{"path":"/srv/private-folder-q7","access":"change"}]"#]);
    let figma = mcp_servers::save(&db, McpServer {
        name: "figma".into(), transport: "stdio".into(), command: "/opt/private-server-q7/bin/figma".into(), args: strs(&["--private-arg-q7"]),
        env_names: strs(&["PRIVATE_ENV_Q7"]), ..Default::default()
    }).unwrap();
    mcp_servers::set_agent_tools(&db, &s.you_id, &be, AgentTools { mcp: vec![AgentServer { server_id: figma.id.clone(), on: true, tools_off: vec![] }] }).unwrap();
    let second = clis::save(&db, vec![Cli { name: "Claude Code (second account)".into(), kind: "claude_code".into(), command: "/opt/private-claude-q7".into(),
                                            env: strs(&["CLAUDE_CONFIG_DIR=/srv/private-config-q7"]), ..Default::default() }]).unwrap()
        .into_iter().find(|c| c.kind == "claude_code" && c.id != clis::CLAUDE_CODE).unwrap();
    sql(&db, "UPDATE agent_configs SET adapter=?2 WHERE actor_id=?1", &[&qa, &second.id]);
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Private project q7".into(), key: "PQS".into(), ..Default::default() }).unwrap();
    tasks::create(&db, &s.you_id, TaskInput { project_id: project, title: "Private card q7".into(), ..Default::default() }).unwrap();
    memory::append_or_make(&db, &memory::Who::Agent(be.clone()), &memory::agent_notes_path("Backend Agent"), "A private note q7", &[], None).unwrap();
    team::set_agent_status(&db, &s.you_id, &ops, "paused").unwrap();
    sql(&db, "UPDATE actors SET status='archived' WHERE id=?1", &[&fe]);

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("t.gizai-team.json");
    templates::save(&db, &s.team_id, &p).unwrap();
    let text = std::fs::read_to_string(&p).unwrap();
    for private in ["private-folder-q7", "private-server-q7", "private-arg-q7", "PRIVATE_ENV_Q7", figma.id.as_str(), second.id.as_str(),
                    "second account", "private-claude-q7", "private-config-q7", "Jeffrey Testperson", s.you_id.as_str(), "Private project q7",
                    "Private card q7", "A private note q7", "paused", "active", "archived", be.as_str(), "\"status\"", "\"folders\"", "\"adapter\""] {
        assert!(!text.contains(private), "{private} is in the file:\n{text}");
    }
    let f: TemplateFile = serde_json::from_str(&text).unwrap();
    let qa_file = f.agents.iter().find(|a| a.name == "QA Agent").unwrap();
    assert_eq!(qa_file.cli, "claude_code", "the CLI's kind, not its id or account");
    assert_eq!(f.agents.iter().find(|a| a.name == "Backend Agent").unwrap().mcp_servers, [FileServer { name: "figma".into(), tools_off: vec![] }]);
    // The archived Frontend Agent is left out, from the agents and from every column; the paused DevOps Agent is in.
    assert_eq!(f.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Team Lead", "Backend Agent", "QA Agent", "DevOps Agent"]);
    assert!(f.columns.iter().all(|c| !c.agents.iter().any(|a| a == "Frontend Agent")), "{:?}", f.columns);
    assert_eq!(f.columns.iter().find(|c| c.name == "Deploy").unwrap().agents, ["DevOps Agent"]);
}

#[test]
fn check_export_names_each_private_place_and_a_clean_team_gives_none() {
    let (db, s) = install();
    assert_eq!(templates::check_export(&db, &s.team_id).unwrap(), vec![], "a new install's team is clean");
    let be = member(&team::get(&db, &s.team_id).unwrap(), "Backend Agent").actor_id.clone();
    let set = |instructions: &str| sql(&db, "UPDATE agent_configs SET instructions_md=?2 WHERE actor_id=?1", &[&be, &instructions]);

    // Text that only looks technical gives none.
    set("Push to git@github.com:owner/repo.git, read https://docs.rs/serde and src/components/MarkdownEditor.tsx, run /usr/bin/env, \
         keep SCREAMING_SNAKE_CASE_CONSTANT_NAMES_THAT_ARE_LONG as they are.");
    assert_eq!(templates::check_export(&db, &s.team_id).unwrap(), vec![]);

    // The card's four, each once, with the agent and the field.
    set("Mail jef@example.com, see /home/jef/x, call https://me:pw@api.example.com/x with sk-ant-api03-abcdefgh1234.");
    let before = counts(&db);
    let w = templates::check_export(&db, &s.team_id).unwrap();
    let found: Vec<(Option<&str>, &str, &str, &str)> =
        w.iter().map(|w| (w.agent.as_deref(), w.field.as_str(), w.kind.as_str(), w.excerpt.as_str())).collect();
    let be_ = Some("Backend Agent");
    assert_eq!(found, [
        (be_, "instructions_md", "email", "jef@example.com"),
        (be_, "instructions_md", "path", "/home/jef/x"),
        (be_, "instructions_md", "url", "https://me:pw@api.example.com/x"),
        (be_, "instructions_md", "secret", "sk-ant-api03-abcdefgh1234"),
    ]);
    assert_eq!(counts(&db), before, "the check writes nothing");
    assert!(templates::export(&db, &s.team_id).unwrap().agents[1].instructions_md.as_deref().unwrap().contains("jef@example.com"),
            "export never hides what it finds");

    // The other paths, URLs and tokens the card names.
    for (text, kind, excerpt) in [
        ("notes in ~/notes/today.md", "path", "~/notes/today.md"),
        ("/Users/jef/Code", "path", "/Users/jef/Code"),
        (r"the file C:\Users\jef\key.txt here", "path", r"C:\Users\jef\key.txt"),
        ("open https://api.example.com/x?token=1.", "url", "https://api.example.com/x?token=1"),
        ("ghp_abcdefghij1234567890", "secret", "ghp_abcdefghij1234567890"),
        ("github_pat_11ABCDEFG0123456789_abcdef", "secret", "github_pat_11ABCDEFG0123456789_abcdef"),
        ("xoxb-1234567890-abcdefgh", "secret", "xoxb-1234567890-abcdefgh"),
        ("AKIAABCDEFGHIJKLMNOP", "secret", "AKIAABCDEFGHIJKLMNOP"),
        ("hash 0123456789abcdef0123456789abcdef01234567 here", "secret", "0123456789abcdef0123456789abcdef01234567"),
        ("token aB3dE5fG7hJ9kL2mN4pQ6rS8tU0vW1xY3z here", "secret", "aB3dE5fG7hJ9kL2mN4pQ6rS8tU0vW1xY3z"),
    ] {
        set(text);
        let w = templates::check_export(&db, &s.team_id).unwrap();
        let found: Vec<(&str, &str)> = w.iter().map(|w| (w.kind.as_str(), w.excerpt.as_str())).collect();
        assert_eq!(found, [(kind, excerpt)], "{text}");
        assert_eq!((w[0].agent.as_deref(), w[0].field.as_str()), (be_, "instructions_md"));
    }

    // A place in the team's own fields names no agent.
    set("Clean.");
    sql(&db, "UPDATE workflow_states SET name='Ask jef@example.com' WHERE id=?1", &[&state_id(&team::get(&db, &s.team_id).unwrap(), "Review")]);
    let w = templates::check_export(&db, &s.team_id).unwrap();
    assert_eq!(w.iter().map(|w| (w.agent.clone(), w.field.as_str(), w.kind.as_str())).collect::<Vec<_>>(), [(None, "columns", "email")]);
}

// ---- the built-in template ----

#[test]
fn the_builtin_software_team_is_a_new_installs_first_team() {
    assert_eq!(serde_json::to_value(templates::builtins()).unwrap(), json!([{"key": "software", "name": "Software team",
        "description": "The five agents a new install gets, and the seven usual columns."}]));
    assert!(templates::builtin("nope").is_none());

    let (db, s) = install();
    let first = templates::export(&db, &s.team_id).unwrap();
    let b = templates::builtin("software").unwrap();
    assert_eq!((b.format.as_str(), b.format_version, b.name.as_str()), ("gizai-team", 1, "Software team"));
    assert_eq!(b.agents, first.agents, "the same agents, instructions, commands and settings");
    assert_eq!(b.columns, first.columns, "the same columns, with their agents");
    assert_eq!((b.labels, b.branches), (first.labels, first.branches));
    assert_eq!(b.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), seed::DEFAULT_AGENTS.iter().map(|(n, _)| *n).collect::<Vec<_>>());
    assert_eq!(b.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), seed::DEFAULT_COLUMNS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>());
}

#[test]
fn the_builtin_comes_in_active_on_its_columns_and_the_team_lead_keeps_chat() {
    let (db, s) = install();
    let lead = team::chat_agent(&db).unwrap().unwrap();
    sql(&db, "UPDATE actors SET name='Master Chief' WHERE id=?1", &[&lead.actor_id]);
    let first_before = serde_json::to_value(team::get(&db, &s.team_id).unwrap()).unwrap();
    let labels_before = gizai_core::labels::list(&db).unwrap().into_iter().map(|l| (l.name, l.color)).collect::<Vec<_>>();

    let plan = templates::preview(&db, &builtin()).unwrap();
    assert_eq!(plan.team_name, "Software team");
    assert!(plan.agents.iter().all(|a| a.active), "a built-in's agents are active");
    assert_eq!(plan.agents.iter().map(|a| (a.name.as_str(), a.file_name.as_str())).collect::<Vec<_>>(), [
        ("Team Lead", "Team Lead"),
        ("Backend Agent (Software team)", "Backend Agent"),
        ("Frontend Agent (Software team)", "Frontend Agent"),
        ("QA Agent (Software team)", "QA Agent"),
        ("DevOps Agent (Software team)", "DevOps Agent"),
    ], "the Team Lead's name is free once yours is Master Chief");
    assert_eq!(plan.notes, ["Your Team Lead stays Master Chief."]);
    assert_eq!(plan.labels_added, Vec::<String>::new());
    assert!(plan.agents.iter().all(|a| a.extra_tools.is_empty() && a.missing_servers.is_empty() && a.cli_name == "Claude Code"
                                      && a.permission_mode == "acceptEdits"));

    let id = templates::import(&db, &s.you_id, &builtin(), "Software team").unwrap();
    let t = team::get(&db, &id).unwrap();
    assert_eq!(t.name, "Software team");
    assert_eq!(t.states.iter().map(|c| (c.name.as_str(), c.category.as_str(), c.auto)).collect::<Vec<_>>(),
               seed::DEFAULT_COLUMNS.iter().map(|(n, k, a)| (*n, *k, *a)).collect::<Vec<_>>());
    assert_eq!(agent_names_on(&t, "To do"), ["Backend Agent (Software team)", "Frontend Agent (Software team)"]);
    assert_eq!(agent_names_on(&t, "Testing"), ["QA Agent (Software team)"]);
    assert_eq!(agent_names_on(&t, "Deploy"), ["DevOps Agent (Software team)"]);
    for (name, role) in seed::DEFAULT_AGENTS {
        let m = member(&t, &if name == "Team Lead" { name.to_string() } else { format!("{name} (Software team)") });
        assert_eq!(m.status, "active", "{name}");
        assert_eq!(m.instructions_md.as_deref(), Some(seed::role_template(role).as_str()), "{name}");
        assert_eq!(m.allowed_tools, seed::role_tools(role), "{name}");
        assert!(!m.chat_enabled, "{name}: Master Chief keeps Chat");
        assert_eq!(m.board_check_minutes, None);
    }
    assert_eq!(team::chat_agent(&db).unwrap().unwrap().actor_id, lead.actor_id, "the Team Lead keeps Chat");
    assert!(t.members.iter().any(|m| m.kind == "person" && m.actor_id == s.you_id && m.role_key == "reviewer"), "you are its reviewer");
    assert_eq!(serde_json::to_value(team::get(&db, &s.team_id).unwrap()).unwrap(), first_before, "the first team doesn't change");
    assert_eq!(gizai_core::labels::list(&db).unwrap().into_iter().map(|l| (l.name, l.color)).collect::<Vec<_>>(), labels_before);
    assert_eq!(count(&db, "SELECT count(*) FROM runs"), 0, "nothing starts by itself");
}

#[test]
fn without_a_chat_agent_the_builtins_team_lead_gets_chat() {
    let (db, s) = bare();
    let plan = templates::preview(&db, &builtin()).unwrap();
    assert_eq!(plan.notes, Vec::<String>::new());
    assert_eq!(plan.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), seed::DEFAULT_AGENTS.iter().map(|(n, _)| *n).collect::<Vec<_>>());
    let id = templates::import(&db, &s.you_id, &builtin(), &plan.team_name).unwrap();
    let chat = team::chat_agent(&db).unwrap().unwrap();
    assert_eq!(chat.name, "Team Lead");
    assert_eq!(member(&team::get(&db, &id).unwrap(), "Team Lead").actor_id, chat.actor_id);
}

#[test]
fn a_team_made_from_the_builtin_can_be_a_projects_team() {
    // GA-225: a project picks its team. A team made from a template is a second team like any other.
    let (db, s) = install();
    std::thread::sleep(std::time::Duration::from_millis(5)); // teams are ordered by when they were made
    let id = templates::import(&db, &s.you_id, &builtin(), "Software team 2").unwrap();
    let made = team::get(&db, &id).unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Template project".into(), key: "TPL".into(),
        team_id: Some(id.clone()), ..Default::default() }).unwrap();

    let p = projects::get(&db, &project).unwrap();
    assert_eq!((p.team_id.as_deref(), p.team_name.as_deref()), (Some(id.as_str()), Some("Software team 2")), "projects::get returns that team");

    let card = tasks::create(&db, &s.you_id, TaskInput { project_id: project.clone(), title: "First card".into(), ..Default::default() }).unwrap();
    let (state, category): (String, String) = db.read(|c| Ok(c.query_row("SELECT state_id, state_category FROM tasks WHERE id=?1", [&card],
        |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!(state, made.states[0].id, "the new card goes to that team's first column");
    assert_eq!((made.states[0].name.as_str(), category.as_str()), ("Backlog", "backlog"));
    assert!(team::get(&db, &s.team_id).unwrap().states.iter().all(|c| c.id != state), "not the first team's Backlog");
    let other = projects::create(&db, &s.you_id, ProjectInput { name: "Default project".into(), key: "DEF".into(), ..Default::default() }).unwrap();
    assert_eq!(projects::get(&db, &other).unwrap().team_id.as_deref(), Some(s.team_id.as_str()), "without a team: the first team, as before");
}

// ---- export, then import ----

#[test]
fn export_then_import_from_the_saved_path_makes_the_same_team_and_the_first_one_stays() {
    let (db, s) = install();
    let t = team::get(&db, &s.team_id).unwrap();
    let be = member(&t, "Backend Agent").actor_id.clone();
    sql(&db, "UPDATE workflow_states SET wip_limit=2, color='#ff8800' WHERE id=?1", &[&state_id(&t, "In progress")]);
    sql(&db, "UPDATE agent_configs SET model='claude-sonnet-4-5', effort='max', budget_usd_micros=12340000, max_concurrent_runs=2,
              allowed_tools_json=?2 WHERE actor_id=?1", &[&be, &serde_json::to_string(&[seed::role_tools("backend"), strs(&["Bash(terraform plan:*)"])].concat()).unwrap()]);
    sql(&db, "UPDATE teams SET description_md='Builds Gizai.', color='#abcdef' WHERE id=?1", &[&s.team_id]);
    team::add_branch(&db, &s.you_id, &s.team_id, "Docs").unwrap();
    let before = serde_json::to_value(team::get(&db, &s.team_id).unwrap()).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join(format!("{}.gizai-team.json", t.name));
    templates::save(&db, &s.team_id, &p).unwrap();
    let plan = templates::preview(&db, &path(&p)).unwrap();
    assert_eq!(plan.team_name, format!("{} 2", t.name), "the team's name is taken: it gets a number");
    assert_eq!(plan_agent(&plan, "Backend Agent").extra_tools, ["Bash(terraform plan:*)"]);
    let id = templates::import(&db, &s.you_id, &path(&p), "Copy").unwrap();

    assert_eq!(serde_json::to_value(team::get(&db, &s.team_id).unwrap()).unwrap(), before, "the original team doesn't change");
    let (a, mut b) = (templates::export(&db, &s.team_id).unwrap(), templates::export(&db, &id).unwrap());
    assert_eq!(b.name, "Copy");
    // Names aside: the copies are "<name> (Copy)".
    let plain = |n: &mut String| *n = n.trim_end_matches(" (Copy)").to_string();
    b.agents.iter_mut().for_each(|x| plain(&mut x.name));
    b.columns.iter_mut().for_each(|c| c.agents.iter_mut().for_each(plain));
    assert_eq!(b.columns, a.columns, "the same columns: order, kind, Auto, next, WIP limit, colour and agents");
    assert_eq!((&b.description_md, &b.color, &b.branches, &b.labels), (&a.description_md, &a.color, &a.branches, &a.labels));
    // The same agents, but the Team Lead: the first team's keeps Chat.
    let mut want = a.agents.clone();
    want[0].chat = false;
    assert_eq!(b.agents, want);
    assert_eq!(plan.notes, ["Your Team Lead stays Team Lead."]);

    let copy = team::get(&db, &id).unwrap();
    assert!(copy.members.iter().filter(|m| m.kind == "agent").all(|m| m.status == "paused"), "agents from a file come in paused");
    // Each agent has its memory folder, as team::add_agent makes it (the Team Lead keeps its notes in Team Lead/).
    for name in ["Backend Agent (Copy)", "Frontend Agent (Copy)", "QA Agent (Copy)", "DevOps Agent (Copy)"] {
        assert!(memory::find(&db, &memory::agent_notes_path(name)).unwrap().is_some(), "{name}'s Notes");
    }
}

// ---- refusals ----

#[test]
fn each_refusal_says_why_and_makes_nothing() {
    let (db, s) = install();
    let dir = tempfile::tempdir().unwrap();
    let ok = small("Small team");
    let with = |f: &dyn Fn(&mut TemplateFile)| {
        let mut t = ok.clone();
        f(&mut t);
        file(&t)
    };
    let big_text = format!("{{\"format\":\"gizai-team\",\"format_version\":1,\"name\":\"Big\",\"description_md\":\"{}\"}}", "x".repeat(2 * 1024 * 1024));
    let big = dir.path().join("big.gizai-team.json");
    std::fs::write(&big, &big_text).unwrap();
    let mut v = serde_json::to_value(&ok).unwrap();
    v["format_version"] = json!(2);
    let newer = v.to_string();
    v["format_version"] = json!(1);
    v["format"] = json!("gizai-backup");
    let other_format = v.to_string();

    let cases: Vec<(&str, Source, &str)> = vec![
        ("no file", path(&dir.path().join("missing.gizai-team.json")), "can't read"),
        ("a folder", path(dir.path()), "can't read"),
        ("not JSON", text("{ this isn't json"), "isn't JSON"),
        ("another format", text(&other_format), "isn't a Gizai team template"),
        ("a newer format", text(&newer), "This template needs a newer version of the app."),
        ("no columns", with(&|t| t.columns.clear()), "no columns"),
        ("51 agents", with(&|t| t.agents.extend((0..49).map(|i| agent(&format!("Agent {i}"), "backend")))), "51 agents"),
        ("31 columns", with(&|t| t.columns.extend((0..27).map(|i| column(&format!("Column {i}"), "in_progress", false, None, &[])))), "31 columns"),
        ("long instructions", with(&|t| t.agents[0].instructions_md = Some("é".repeat(100_001))), "instructions"),
        ("a file over 2 MB", path(&big), "2 MB"),
        ("text over 2 MB", text(&big_text), "2 MB"),
        ("an unknown kind", with(&|t| t.columns[1].kind = "sideways".into()), "unknown kind"),
        ("two columns, one name", with(&|t| t.columns[2].name = "BUILD".into()), "two columns called"),
        ("two agents, one name", with(&|t| { t.agents[1].name = "builder".into(); t.columns[2].agents = strs(&["builder"]); }), "two agents called"),
        ("a next that isn't there", with(&|t| t.columns[1].next = Some("Nowhere".into())), "Nowhere"),
        ("an agent that isn't there", with(&|t| t.columns[1].agents.push("Ghost".into())), "Ghost"),
        ("agents on Backlog", with(&|t| t.columns[0].agents = strs(&["Builder"])), "takes no agents"),
        ("agents on Done", with(&|t| t.columns[3].agents = strs(&["Checker"])), "takes no agents"),
        ("agents on Review", with(&|t| t.columns.insert(3, column("Review", "review", false, Some("Done"), &["Checker"]))), "takes no agents"),
        ("agents on Cancelled", with(&|t| t.columns.push(column("Cancelled", "cancelled", false, None, &["Checker"]))), "takes no agents"),
        ("Auto on Review", with(&|t| t.columns.insert(3, column("Review", "review", true, Some("Done"), &[]))), "can't be Auto"),
        ("Auto on Backlog", with(&|t| { t.columns[0].auto = true; t.columns[0].next = Some("Build".into()); }), "can't be Auto"),
        ("Auto without next", with(&|t| t.columns[2].next = None), "without a next column"),
    ];
    assert!(templates::preview(&db, &file(&ok)).is_ok(), "the template the cases start from is fine");
    let before = counts(&db);
    for (case, source, why) in cases {
        let e = templates::preview(&db, &source).expect_err(case).to_string();
        assert!(e.contains(why), "{case}: {e}");
        let e = templates::import(&db, &s.you_id, &source, "Refused").expect_err(case).to_string();
        assert!(e.contains(why), "{case}, import: {e}");
        assert_eq!(counts(&db), before, "{case} makes nothing");
    }
    // The version's message, word for word.
    assert_eq!(templates::preview(&db, &text(&newer)).unwrap_err().to_string(), "This template needs a newer version of the app.");
    // Within the limits is fine: 50 agents, 30 columns, instructions of 100,000 characters, format_version 1.0.
    let edge = with(&|t| {
        t.agents.extend((0..48).map(|i| agent(&format!("Agent {i}"), "backend")));
        t.columns.extend((0..26).map(|i| column(&format!("Column {i}"), "in_progress", false, None, &[])));
        t.agents[0].instructions_md = Some("é".repeat(100_000));
    });
    assert_eq!(templates::preview(&db, &edge).unwrap().agents.len(), 50);
    assert!(templates::preview(&db, &text(&newer.replace("\"format_version\":2", "\"format_version\":1.0"))).is_ok());
}

// ---- names ----

#[test]
fn a_taken_team_name_gets_a_number_and_a_taken_agent_name_the_teams_name_in_brackets() {
    let (db, s) = install();
    team::add_team(&db, &s.you_id, "software TEAM").unwrap();
    let plan = templates::preview(&db, &builtin()).unwrap();
    assert_eq!(plan.team_name, "Software team 2", "case ignored");
    assert_eq!(plan_agent(&plan, "Backend Agent").name, "Backend Agent (Software team 2)");
    team::add_team(&db, &s.you_id, "Software team 2").unwrap();
    assert_eq!(templates::preview(&db, &builtin()).unwrap().team_name, "Software team 3");

    // Import takes the name passed in; an agent's name is unique ignoring case, the file's name stays in fileName.
    let mut f = small("Small team");
    f.agents[0].name = "BACKEND AGENT".into();
    f.columns[1].agents = strs(&["BACKEND AGENT"]);
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan.team_name, "Small team");
    assert_eq!((plan.agents[0].name.as_str(), plan.agents[0].file_name.as_str()), ("BACKEND AGENT (Small team)", "BACKEND AGENT"));
    assert_eq!((plan.agents[1].name.as_str(), plan.agents[1].file_name.as_str()), ("Checker", "Checker"));
    assert_eq!(plan.columns[1].agents, ["BACKEND AGENT (Small team)"], "the plan's columns use the new names");
    let id = templates::import(&db, &s.you_id, &file(&f), "Crew").unwrap();
    let t = team::get(&db, &id).unwrap();
    assert_eq!(t.name, "Crew");
    assert_eq!(agent_names_on(&t, "Build"), ["BACKEND AGENT (Crew)"]);
    // A second import of the same file: those names are taken now too.
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan.team_name, "Small team");
    assert_eq!(plan.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["BACKEND AGENT (Small team)", "Checker (Small team)"]);
}

// ---- the CLI, permission modes and effort ----

#[test]
fn an_agent_whose_cli_kind_isnt_set_up_runs_on_claude_code_with_accept_edits() {
    let (db, s) = install();
    let mut f = small("Small team");
    f.agents[0].cli = "codex".into();
    f.agents[0].model = Some("gpt-5-codex".into());
    f.agents[0].effort = Some("minimal".into());
    f.agents[0].permission_mode = Some("read-only".into());
    f.agents[1].cli = "gemini".into();
    f.agents[1].permission_mode = Some("plan".into());
    f.agents[1].model = Some("gemini-2.5-pro".into());
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan.notes, ["No Codex CLI is set up: Builder runs on Claude Code.", "No Gemini CLI is set up: Checker runs on Claude Code."]);
    for a in &plan.agents {
        assert_eq!((a.cli_name.as_str(), a.permission_mode.as_str(), a.model.as_deref(), a.effort.as_deref()), ("Claude Code", "acceptEdits", None, None),
                   "{}", a.name);
    }
    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    for name in ["Builder", "Checker"] {
        let m = member(&t, name);
        assert_eq!((m.adapter.as_deref().unwrap_or(""), m.permission_mode.as_deref(), m.model.as_deref(), m.effort.as_deref()),
                   (clis::CLAUDE_CODE, Some("acceptEdits"), None, None), "{name}");
    }
}

#[test]
fn modes_the_cli_doesnt_offer_and_the_ones_that_skip_every_check_become_its_first_mode() {
    let (db, s) = install();
    let saved = clis::save(&db, vec![
        Cli { name: "My Codex".into(), kind: "codex".into(), command: "codex".into(), ..Default::default() },
        Cli { name: "My Gemini".into(), kind: "gemini".into(), command: "gemini".into(), ..Default::default() },
    ]).unwrap();
    let id_of = |name: &str| saved.iter().find(|c| c.name == name).unwrap().id.clone();
    let mut f = small("Small team");
    let mk = |name: &str, cli: &str, mode: &str, effort: Option<&str>| {
        let mut a = agent(name, "backend");
        a.cli = cli.into();
        a.permission_mode = Some(mode.into());
        a.effort = effort.map(String::from);
        a
    };
    f.agents.extend([
        mk("Claude Bypass", "claude_code", "bypassPermissions", Some("high")),
        mk("Claude Odd", "claude_code", "workspace-write", Some("minimal")),
        mk("Claude Plan", "claude_code", "plan", None),
        mk("Codex Full", "codex", "danger-full-access", Some("minimal")),
        mk("Codex Read", "codex", "read-only", Some("max")),
        mk("Gemini Yolo", "gemini", "yolo", Some("high")),
        mk("Gemini Plan", "gemini", "plan", None),
    ]);
    let plan = templates::preview(&db, &file(&f)).unwrap();
    let got = |n: &str| {
        let a = plan_agent(&plan, n);
        (a.cli_name.as_str(), a.permission_mode.as_str(), a.effort.as_deref())
    };
    assert_eq!(got("Claude Bypass"), ("Claude Code", "acceptEdits", Some("high")));
    assert_eq!(got("Claude Odd"), ("Claude Code", "acceptEdits", None), "minimal is Codex's: empty");
    assert_eq!(got("Claude Plan"), ("Claude Code", "plan", None), "a mode the CLI offers is kept");
    assert_eq!(got("Codex Full"), ("My Codex", "workspace-write", Some("minimal")));
    assert_eq!(got("Codex Read"), ("My Codex", "read-only", None), "max is Claude Code's: empty");
    assert_eq!(got("Gemini Yolo"), ("My Gemini", "auto_edit", None), "Gemini takes no effort");
    assert_eq!(got("Gemini Plan"), ("My Gemini", "plan", None));
    // Each change is noted, with the agent and the mode it gets; a kept mode isn't.
    for (name, first) in [("Claude Bypass", "acceptEdits"), ("Claude Odd", "acceptEdits"), ("Codex Full", "workspace-write"), ("Gemini Yolo", "auto_edit")] {
        assert!(plan.notes.iter().any(|n| n.contains(name) && n.contains(first)), "{name}: {:?}", plan.notes);
    }
    for name in ["Claude Plan", "Codex Read", "Gemini Plan", "Builder", "Checker"] {
        assert!(!plan.notes.iter().any(|n| n.contains(name)), "{name}: {:?}", plan.notes);
    }
    for never in ["bypassPermissions", "danger-full-access", "yolo"] {
        assert!(plan.agents.iter().all(|a| a.permission_mode != never));
    }

    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    let m = |n: &str| member(&t, n);
    assert_eq!((m("Codex Full").adapter.clone().unwrap(), m("Codex Full").permission_mode.as_deref(), m("Codex Full").effort.as_deref()),
               (id_of("My Codex"), Some("workspace-write"), Some("minimal")), "on the first CLI of its kind");
    assert_eq!((m("Gemini Yolo").adapter.clone().unwrap(), m("Gemini Yolo").permission_mode.as_deref()), (id_of("My Gemini"), Some("auto_edit")));
    assert_eq!((m("Claude Bypass").permission_mode.as_deref(), m("Claude Bypass").effort.as_deref()), (Some("acceptEdits"), Some("high")));
    assert_eq!(m("Claude Plan").permission_mode.as_deref(), Some("plan"));
}

// ---- allowed commands, MCP servers, Chat ----

#[test]
fn extra_tools_lists_exactly_the_commands_beyond_the_roles_defaults() {
    let (db, s) = install();
    let mut f = small("Small team");
    f.agents[0].allowed_tools = [seed::role_tools("backend"), strs(&["Bash(terraform plan:*)", "Bash(make:*)"])].concat();
    f.agents[1].allowed_tools = seed::role_tools("qa")[..3].to_vec();
    let mut lead = agent("Lead", "lead");
    lead.allowed_tools = strs(&["Bash(git status:*)", "Bash(gh pr merge:*)"]);
    f.agents.push(lead);
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan_agent(&plan, "Builder").extra_tools, ["Bash(terraform plan:*)", "Bash(make:*)"]);
    assert_eq!(plan_agent(&plan, "Checker").extra_tools, Vec::<String>::new(), "fewer than the defaults: nothing extra");
    let lead_extra: Vec<String> = ["Bash(git status:*)", "Bash(gh pr merge:*)"].iter().filter(|t| !seed::role_tools("lead").contains(&t.to_string()))
        .map(|t| t.to_string()).collect();
    assert_eq!(plan_agent(&plan, "Lead").extra_tools, lead_extra, "against the lead's own defaults");
    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    assert_eq!(member(&t, "Builder").allowed_tools, f.agents[0].allowed_tools, "the commands are kept");
    assert_eq!(member(&t, "Checker").allowed_tools, f.agents[1].allowed_tools);
}

#[test]
fn a_known_mcp_server_is_switched_on_and_an_unknown_one_is_missing_and_stays_off() {
    let (db, s) = install();
    let figma = mcp_servers::save(&db, McpServer { name: "Figma".into(), transport: "stdio".into(), command: "figma-mcp".into(), ..Default::default() }).unwrap();
    let other = mcp_servers::save(&db, McpServer { name: "sentry".into(), transport: "stdio".into(), command: "sentry-mcp".into(), ..Default::default() }).unwrap();
    let mut f = small("Small team");
    f.agents[0].mcp_servers = vec![FileServer { name: "figma".into(), tools_off: strs(&["get_image"]) }, FileServer { name: "linear".into(), tools_off: vec![] }];
    f.agents[0].browser = true;
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan_agent(&plan, "Builder").missing_servers, ["linear"]);
    assert_eq!(plan_agent(&plan, "Checker").missing_servers, Vec::<String>::new());
    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    let on: Vec<(String, Vec<String>)> = member(&t, "Builder").tools.servers_on().map(|s| (s.server_id.clone(), s.tools_off.clone())).collect();
    assert!(on.contains(&(figma.id.clone(), strs(&["get_image"]))), "matched ignoring case: {on:?}");
    assert!(on.iter().any(|(id, _)| id == mcp_servers::BROWSER), "the browser: {on:?}");
    assert_eq!(on.len(), 2, "linear isn't there and sentry wasn't asked for: {on:?}");
    assert!(!on.iter().any(|(id, _)| *id == other.id));
    assert_eq!(member(&t, "Checker").tools.servers_on().count(), 0);
}

#[test]
fn with_a_chat_agent_there_the_files_chat_agent_comes_without_chat_or_the_board_check() {
    let (db, s) = install();
    let lead = team::chat_agent(&db).unwrap().unwrap();
    sql(&db, "UPDATE actors SET name='Master Chief' WHERE id=?1", &[&lead.actor_id]);
    let mut f = small("Small team");
    let mut boss = agent("Boss", "lead");
    boss.chat = true;
    boss.board_check_minutes = Some(30);
    f.agents.push(boss.clone());
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan.notes, ["Your Team Lead stays Master Chief."]);
    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    let b = member(&t, "Boss");
    assert_eq!((b.chat_enabled, b.board_check_minutes), (false, None));
    let chat = team::chat_agent(&db).unwrap().unwrap();
    assert_eq!((chat.actor_id.as_str(), chat.name.as_str()), (lead.actor_id.as_str(), "Master Chief"), "the Team Lead keeps Chat");
    assert_eq!(team::agent(&db, &lead.actor_id).unwrap().board_check_minutes, lead.board_check_minutes);

    // Without a Chat agent, the file's comes with Chat and its board check.
    let (db, s) = bare();
    let mut f = small("Small team");
    f.agents.push(boss);
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan.notes, Vec::<String>::new());
    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    let b = member(&t, "Boss");
    assert_eq!((b.chat_enabled, b.board_check_minutes), (true, Some(30)));
}

// ---- paused or active, memory folders, labels ----

#[test]
fn agents_from_a_file_come_in_paused_and_nothing_starts() {
    let (db, s) = install();
    let plan = templates::preview(&db, &file(&small("Small team"))).unwrap();
    assert!(plan.agents.iter().all(|a| !a.active));
    let id = templates::import(&db, &s.you_id, &file(&small("Small team")), "Small team").unwrap();
    let t = team::get(&db, &id).unwrap();
    assert_eq!(t.members.iter().filter(|m| m.kind == "agent").map(|m| m.status.as_str()).collect::<Vec<_>>(), ["paused", "paused"]);
    assert_eq!(agent_names_on(&t, "Build"), ["Builder"]);
    assert_eq!(agent_names_on(&t, "Check"), ["Checker"]);
    assert_eq!(t.states.iter().map(|c| (c.name.as_str(), c.auto, c.next_state_id.clone())).collect::<Vec<_>>(), [
        ("Backlog", false, None), ("Build", true, Some(state_id(&t, "Check"))), ("Check", true, Some(state_id(&t, "Done"))), ("Done", false, None),
    ]);
    assert_eq!(count(&db, "SELECT count(*) FROM runs"), 0);
    assert_eq!(count(&db, &format!("SELECT count(*) FROM tasks t JOIN workflow_states s ON s.id = t.state_id WHERE s.team_id = '{id}'")), 0);
}

#[test]
fn shares_memory_with_a_name_in_the_file_or_its_own_folder_noted() {
    let (db, s) = install();
    let mut f = small("Small team");
    f.agents[1].shares_memory_with = Some("builder".into());
    let mut stray = agent("Stray", "backend");
    stray.shares_memory_with = Some("Nobody".into());
    f.agents.push(stray);
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert!(plan.notes.iter().any(|n| n.contains("Nobody") && n.contains("Stray") && n.contains("own memory folder")), "{:?}", plan.notes);
    let t = team::get(&db, &templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap()).unwrap();
    let (builder, checker, stray) = (member(&t, "Builder"), member(&t, "Checker"), member(&t, "Stray"));
    assert_eq!(memory::folder_owner(&db, &checker.actor_id).unwrap(), builder.actor_id);
    assert_eq!(memory::folder_owner(&db, &stray.actor_id).unwrap(), stray.actor_id);
    assert!(memory::find(&db, &memory::agent_notes_path("Builder")).unwrap().is_some());
    assert!(memory::find(&db, &memory::agent_notes_path("Stray")).unwrap().is_some());
    assert!(memory::find(&db, &memory::agent_notes_path("Checker")).unwrap().is_none(), "it shares the Builder's folder");
    assert_eq!(templates::export(&db, &t.id).unwrap().agents[1].shares_memory_with.as_deref(), Some("Builder"));
}

#[test]
fn labels_that_arent_there_are_added_and_the_ones_there_keep_their_colour() {
    let (db, s) = install();
    let mut f = small("Small team");
    f.labels = vec![
        FileLabel { name: "BACKEND".into(), color: Some("#000000".into()) },
        FileLabel { name: "infra".into(), color: Some("#00aa00".into()) },
        FileLabel { name: "docs".into(), color: None },
    ];
    let plan = templates::preview(&db, &file(&f)).unwrap();
    assert_eq!(plan.labels_added, ["infra", "docs"]);
    templates::import(&db, &s.you_id, &file(&f), "Small team").unwrap();
    let labels: Vec<(String, Option<String>)> = gizai_core::labels::list(&db).unwrap().into_iter().map(|l| (l.name, l.color)).collect();
    assert!(labels.contains(&("backend".into(), Some("#f29a4a".into()))), "{labels:?}");
    assert!(labels.contains(&("infra".into(), Some("#00aa00".into()))) && labels.contains(&("docs".into(), None)), "{labels:?}");
    assert_eq!(labels.len(), 6);
}

// ---- one write ----

#[test]
fn an_import_that_fails_partway_leaves_nothing_behind() {
    let (db, s) = install();
    // The last step of an import puts the agents on their columns: make it fail there, after the team, its columns, the
    // new label, the agents and their memory folders are made.
    db.write(None, |w| {
        w.conn().execute_batch("CREATE TRIGGER qa_import_fails BEFORE INSERT ON column_agents BEGIN SELECT RAISE(ABORT, 'qa: no column agents'); END;")?;
        Ok(())
    }).unwrap();
    let before = counts(&db);
    let e = templates::import(&db, &s.you_id, &file(&small("Small team")), "Small team").unwrap_err().to_string();
    assert!(e.contains("qa: no column agents"), "{e}");
    assert_eq!(counts(&db), before, "nothing is left behind");
    assert!(team_named(&db, "Small team").is_none());
    assert!(gizai_core::labels::find(&db, "infra").unwrap().is_none());
    assert!(memory::find(&db, &memory::agent_notes_path("Builder")).unwrap().is_none());

    db.write(None, |w| {
        w.conn().execute_batch("DROP TRIGGER qa_import_fails;")?;
        Ok(())
    }).unwrap();
    let id = templates::import(&db, &s.you_id, &file(&small("Small team")), "Small team").unwrap();
    assert_eq!(team_named(&db, "Small team"), Some(id));
}

#[test]
fn a_source_holds_one_template_and_a_new_team_without_one_works_as_before() {
    let (db, s) = install();
    let two = Source { builtin: Some("software".into()), json: Some("{}".into()), ..Default::default() };
    assert!(templates::preview(&db, &two).is_err());
    assert!(templates::preview(&db, &Source::default()).is_err());
    assert!(templates::preview(&db, &Source { builtin: Some("nope".into()), ..Default::default() }).is_err());
    assert!(templates::import(&db, &s.you_id, &builtin(), "  ").is_err(), "a team needs a name");
    // The commands' JSON: one of builtin, path or json.
    let src: Source = serde_json::from_value(json!({"builtin": "software"})).unwrap();
    assert_eq!(templates::preview(&db, &src).unwrap().team_name, "Software team");
    // The plan as the screens get it (contract TT).
    let plan = serde_json::to_value(templates::preview(&db, &src).unwrap()).unwrap();
    assert_eq!(keys(&plan), sorted(&["teamName", "columns", "agents", "labelsAdded", "notes"]));
    assert_eq!(keys(&plan["columns"][0]), sorted(&["name", "kind", "auto", "next", "agents"]));
    assert_eq!(keys(&plan["agents"][0]),
               sorted(&["name", "fileName", "role", "cliName", "model", "effort", "permissionMode", "extraTools", "missingServers", "active"]));
    let w = serde_json::to_value(templates::Warning { agent: None, field: "name".into(), kind: "email".into(), excerpt: "a@b.cd".into() }).unwrap();
    assert_eq!(keys(&w), sorted(&["agent", "field", "kind", "excerpt"]));

    // A new team without a template: the seven columns and no agents, as before.
    let id = team::add_team(&db, &s.you_id, "Plain").unwrap();
    let t = team::get(&db, &id).unwrap();
    assert_eq!(t.states.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), seed::DEFAULT_COLUMNS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>());
    assert!(t.members.iter().all(|m| m.kind != "agent"));
}

#[test]
fn the_format_page_describes_every_field_of_format_1() {
    let doc = include_str!("../../../../docs/team-template-format.md");
    assert!(doc.contains("This template needs a newer version of the app."));
    assert!(doc.contains(".gizai-team.json"));
    for field in TEAM_FIELDS.iter().chain(&COLUMN_FIELDS).chain(&AGENT_FIELDS).chain(&["tools_off", "key", "roles"]) {
        assert!(doc.contains(&format!("`{field}`")), "docs/team-template-format.md doesn't name `{field}`");
    }
}
