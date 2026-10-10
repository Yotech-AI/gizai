//! GA-96 QA end to end: agents that share one memory folder. The Team Lead's get_agent, create_agent and update_agent
//! with shares_memory_with by agent name; and task runs on the fake Claude Code: a Backend Agent 2 run sharing the Backend
//! Agent's folder gets its notes and saves what it learned there with its name, and the Backend Agent's next run gets it.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};

use gizai_core::clis::Cli;
use gizai_core::memory::{self, Who};
use gizai_core::model::*;
use gizai_core::{clients, projects, runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, tools};
use serde_json::{Value, json};

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

// ---- The Team Lead's tools ----

struct T {
    st: AppState,
    lead: String,
    _dir: tempfile::TempDir,
}

/// A Team Lead with Chat on, and Claude Code's model list from the fake.
fn lead_setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    T { st, lead, _dir: dir }
}

impl T {
    async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        tools::call(&self.st, &self.lead, name, args).await
    }
    async fn ok(&self, name: &str, args: Value) -> Value {
        self.call(name, args).await.unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }
    async fn agent(&self, name: &str) -> Value {
        self.ok("get_agent", json!({"agent": name})).await["agent"].clone()
    }
    fn id(&self, name: &str) -> String {
        team::all_agents(&self.st.db).unwrap().into_iter().map(|(_, m)| m).find(|m| m.name == name).unwrap_or_else(|| panic!("no {name}")).actor_id
    }
    fn shares(&self, name: &str) -> Option<String> {
        team::agent(&self.st.db, &self.id(name)).unwrap().shares_memory_with
    }
}

#[tokio::test]
async fn get_agent_shows_the_setting_and_create_agent_and_update_agent_set_it_by_name_refusing_an_unknown_name_or_the_team_lead() {
    let t = lead_setup();
    // the tools say so
    for d in tools::catalog().iter().filter(|d| d.name == "create_agent" || d.name == "update_agent") {
        let p = &d.input_schema["properties"]["shares_memory_with"];
        assert_eq!(p["type"], "string", "{}: {p}", d.name);
        assert!(p["description"].as_str().unwrap().contains("by name"), "{}: {p}", d.name);
    }
    t.ok("create_agent", json!({"name": "Backend Agent", "role": "backend"})).await;
    let be = t.agent("Backend Agent").await;
    assert_eq!((be["shares_memory_with"].clone(), be["memory_folder"].clone(), be["shared_by"].clone(), be["use_memory"].clone()),
               (Value::Null, json!("Agents/Backend Agent/"), json!([]), json!(true)), "{be}");

    // create_agent: by name
    t.ok("create_agent", json!({"name": "Backend Agent 2", "role": "backend", "shares_memory_with": "Backend Agent"})).await;
    assert_eq!(t.shares("Backend Agent 2"), Some(t.id("Backend Agent")));
    let be2 = t.agent("Backend Agent 2").await;
    assert_eq!((be2["shares_memory_with"].clone(), be2["memory_folder"].clone()), (json!("Backend Agent"), json!("Agents/Backend Agent/")), "{be2}");
    assert_eq!(t.agent("Backend Agent").await["shared_by"], json!(["Backend Agent 2"]));
    assert!(memory::find(&t.st.db, "Agents/Backend Agent 2/Notes").unwrap().is_none(), "made in the group: no folder of its own");
    // an agent that shares stands for its owner
    t.ok("create_agent", json!({"name": "Backend Agent 3", "role": "backend", "shares_memory_with": "Backend Agent 2"})).await;
    assert_eq!(t.shares("Backend Agent 3"), Some(t.id("Backend Agent")));
    assert_eq!(t.agent("Backend Agent 3").await["shares_memory_with"], json!("Backend Agent"));
    // an unknown name, or the Team Lead, is refused and no agent is added
    let e = t.call("create_agent", json!({"name": "Ghost Agent", "role": "backend", "shares_memory_with": "Nobody"})).await.unwrap_err();
    assert!(e.contains("Nobody"), "{e}");
    let e = t.call("create_agent", json!({"name": "Lead's twin", "role": "backend", "shares_memory_with": "Team Lead"})).await.unwrap_err();
    assert!(e.contains("Team Lead"), "{e}");
    let names: Vec<String> = team::all_agents(&t.st.db).unwrap().into_iter().map(|(_, m)| m.name).collect();
    assert!(!names.iter().any(|n| n == "Ghost Agent" || n == "Lead's twin"), "{names:?}");

    // update_agent: an unknown name or the Team Lead changes nothing, not even the other fields given
    let e = t.call("update_agent", json!({"agent": "Backend Agent 2", "title": "Second account", "shares_memory_with": "Nobody"})).await.unwrap_err();
    assert!(e.contains("Nobody"), "{e}");
    let e = t.call("update_agent", json!({"agent": "Backend Agent 2", "title": "Second account", "shares_memory_with": "Team Lead"})).await.unwrap_err();
    assert!(e.contains("Team Lead"), "{e}");
    let m = team::agent(&t.st.db, &t.id("Backend Agent 2")).unwrap();
    assert_eq!((m.title.as_deref().unwrap_or(""), m.shares_memory_with), ("", Some(t.id("Backend Agent"))));
    // the Team Lead itself shares nothing
    assert!(t.call("update_agent", json!({"agent": "Team Lead", "shares_memory_with": "Backend Agent"})).await.is_err());
    let lead = t.agent("Team Lead").await;
    assert_eq!((lead["shares_memory_with"].clone(), lead["memory_folder"].clone()), (Value::Null, json!("Team Lead/")), "{lead}");
    // a field left out keeps it
    t.ok("update_agent", json!({"agent": "Backend Agent 2", "title": "Second account"})).await;
    assert_eq!(t.shares("Backend Agent 2"), Some(t.id("Backend Agent")));
    // "none": its own folder, with a fresh Notes
    t.ok("update_agent", json!({"agent": "Backend Agent 2", "shares_memory_with": "none"})).await;
    assert_eq!(t.shares("Backend Agent 2"), None);
    let be2 = t.agent("Backend Agent 2").await;
    assert_eq!((be2["shares_memory_with"].clone(), be2["memory_folder"].clone()), (Value::Null, json!("Agents/Backend Agent 2/")), "{be2}");
    assert!(memory::find(&t.st.db, "Agents/Backend Agent 2/Notes").unwrap().is_some());
    assert_eq!(t.agent("Backend Agent").await["shared_by"], json!(["Backend Agent 3"]));
    // and back, by name
    t.ok("update_agent", json!({"agent": "Backend Agent 2", "shares_memory_with": "Backend Agent"})).await;
    assert_eq!(t.shares("Backend Agent 2"), Some(t.id("Backend Agent")));
    assert!(memory::find(&t.st.db, "Agents/Backend Agent 2/Notes").unwrap().is_none(), "its template-only Notes went into the owner's");
}

// ---- Task runs ----

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &AppState, name: &str, kind: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts agent `agent` on CLI `cli` (Shares memory with left as it is).
fn put_on(st: &AppState, agent: &str, cli: &str) {
    let m = team::agent(&st.db, agent).unwrap();
    team::update_agent(&st.db, &st.you_id, agent, AgentInput { name: m.name, role_key: m.role_key, adapter: cli.into(), ..Default::default() }).unwrap();
}

/// The prompt the fake CLI was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &Run) -> String {
    let err = std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

/// The Memory section of an agent's run prompt, up to the run's rules.
fn section_of(prompt: &str) -> String {
    let at = prompt.find("\n## Memory\n\nNotes from Gizai's Memory for this card").unwrap_or_else(|| panic!("no Memory section: {prompt}"));
    let end = prompt[at..].find("\n## How this run works").expect("the run's rules after it") + at;
    prompt[at..end].to_string()
}

fn today() -> String {
    gizai_lib::tools::ymd(gizai_core::ids::now_ms())
}

#[tokio::test]
async fn a_backend_agent_2_run_gets_the_backend_agents_notes_saves_what_it_learned_there_with_its_name_and_the_backend_agents_next_run_gets_it() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let acme = clients::create(&st.db, &st.you_id, ClientInput { name: "Acme".into(), ..Default::default() }).unwrap();
    let globex = clients::create(&st.db, &st.you_id, ClientInput { name: "Globex".into(), ..Default::default() }).unwrap();
    let kade = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), client_id: Some(acme),
        repo_path: Some(repo.to_string_lossy().into()), ..Default::default() }).unwrap();
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Globex portal".into(), key: "GX".into(), client_id: Some(globex), ..Default::default() }).unwrap();
    let team = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    for s in &team.states {
        gizai_core::columns::set_column(&st.db, &st.you_id, &s.id, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    }
    let add = |name: &str, with: Option<String>| team::add_agent(&st.db, &st.you_id, &team.id, AgentInput {
        name: name.into(), role_key: "backend".into(), shares_memory_with: with, ..Default::default() }).unwrap();
    let be = add("Backend Agent", None);
    let be2 = add("Backend Agent 2", Some(be.clone()));
    let cli = add_cli(&st, "Claude Code (prints its prompt)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    put_on(&st, &be, &cli);
    put_on(&st, &be2, &cli);
    assert_eq!(team::agent(&st.db, &be2).unwrap().shares_memory_with, Some(be.clone()), "putting it on a CLI kept the setting");
    memory::learned(&st.db, &be, "KADE-0", &["The Backend Agent's own lesson.".into()], None, &today()).unwrap();
    // a note about another client in the group's folder never reaches a KADE run
    let you = Who::Person(st.you_id.clone());
    memory::write(&st.db, &you, "Agents/Backend Agent/Globex gotchas", "---\nclient: Globex\n---\nGlobex needs a VPN.", None, None).unwrap();
    let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
    let card = |title: &str, description: &str, agent: &str| tasks::create(&st.db, &st.you_id, TaskInput { project_id: kade.clone(), title: title.into(),
        description_md: description.into(), state_id: Some(todo.clone()), assignee_id: Some(agent.to_string()), ..Default::default() }).unwrap();
    let run = |task: String| {
        let st = &st;
        async move {
            let s = gizai_lib::runs::run_once(st, &task, None, None).await.unwrap();
            assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None));
            core_runs::get(&st.db, &s.run_id).unwrap()
        }
    };
    let notes = || memory::find(&st.db, "Agents/Backend Agent/Notes").unwrap().unwrap();
    let before = notes();

    // the Backend Agent 2's run: the Backend Agent's notes are its own, first
    let task = card("Export invoices as CSV", "Export invoices as CSV. FAKE_LEARNED", &be2);
    let id = tasks::get(&st.db, &task).unwrap().identifier;
    let r2 = run(task).await;
    assert_eq!(r2.agent_id, be2);
    let prompt = prompt_of(&r2);
    let section = section_of(&prompt);
    assert!(section.contains(&format!("### Agents/Backend Agent/Notes (version {})", before.current_version)), "{section}");
    assert!(section.contains("The Backend Agent's own lesson."), "{section}");
    assert!(!prompt.contains("Globex"), "never another client's note, also not from the group's folder: {prompt}");
    assert_eq!(r2.memory.first().map(|g| g.path.as_str()), Some("Agents/Backend Agent/Notes"), "{:?}", r2.memory);
    // what it learned is in the Backend Agent's Notes, with its name, written by it on that run
    let n = notes();
    let day = today();
    assert!(n.body_md.ends_with(&format!("- {day} ({id}, Backend Agent 2): Excel NL needs semicolons in a CSV.\n\
        - {day} ({id}, Backend Agent 2): The exporter tests need --filter=InvoiceExport.\n")), "{}", n.body_md);
    assert_eq!(n.current_version, before.current_version + 1);
    let (author, run_on): (String, Option<String>) = st.db.read(|c| Ok(c.query_row(
        &format!("SELECT author_actor_id, run_id FROM doc_versions WHERE doc_id = ?1 AND version = {}", n.current_version), [&n.id],
        |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!((author, run_on), (be2.clone(), Some(r2.id.clone())));
    assert!(memory::find(&st.db, "Agents/Backend Agent 2/Notes").unwrap().is_none(), "no folder of its own");

    // the Backend Agent's next run gets those lines
    let r1 = run(card("Import invoices", "Import invoices.", &be)).await;
    assert_eq!(r1.agent_id, be);
    let section = section_of(&prompt_of(&r1));
    assert!(section.contains(&format!("### Agents/Backend Agent/Notes (version {})", n.current_version)), "{section}");
    assert!(section.contains(&format!("({id}, Backend Agent 2): Excel NL needs semicolons in a CSV.")), "{section}");
    assert!(!section.contains("Globex"), "{section}");
}
