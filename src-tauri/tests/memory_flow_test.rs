//! GA-19 QA end to end: memory in the Team Lead's chat answers (old threads and new ones), its board checks and its task
//! runs; the memory tools as the Team Lead; the Memory section and `learned` in task runs on Claude Code, Codex, Gemini
//! and Other (the fake CLIs, never the real ones); the notes a run was given; the Use memory switches; and agents made
//! before memory getting their folder at start-up.
use std::path::{Path, PathBuf};
use std::time::Duration;

use gizai_core::clis::Cli;
use gizai_core::memory::{self, Who};
use gizai_core::model::*;
use gizai_core::{chat, clients, docs, projects, runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, board, chat as app_chat, mcp, tools};
use serde_json::{Value, json};

const CHAT_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const MIN: i64 = 60_000;

/// The shim binary: target/debug/gizai-mcp (built by `cargo test --workspace`; built here when missing).
fn shim() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/gizai-mcp"));
    BUILT.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = std::process::Command::new(cargo).args(["build", "-q", "-p", "gizai-mcp"])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).status().map(|s| s.success()).unwrap_or(false);
        assert!(ok && path.is_file(), "could not build gizai-mcp");
    });
    path
}

// ---- The Team Lead: chat, board check, tools ----

struct T {
    st: AppState,
    lead: String,
    project: String,
    _tmp: tempfile::TempDir,
    _server: tokio::task::JoinHandle<()>,
}

/// A Team Lead with Chat on that checks the board every 15 minutes, project KADE, the fake chat `claude`.
async fn setup() -> T {
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    st.mcp_shim = Some(shim());
    settings::set(&st.db, "claude_bin", &CHAT_FAKE.to_string()).unwrap();
    let project = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), board_check_minutes: Some(15), ..Default::default() }).unwrap();
    let server = mcp::start(&st).unwrap();
    T { st, lead, project, _tmp: tmp, _server: server }
}

impl T {
    async fn turn(&self, thread: Option<String>, text: &str) -> (String, app_chat::TurnSummary) {
        let (id, done) = app_chat::send(&self.st, thread, text.into(), None).await.unwrap();
        let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        assert_eq!(s.status, "succeeded", "{s:?}");
        (id, s)
    }
    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-calls.jsonl")).unwrap_or_default()
            .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
    /// The system prompt (--append-system-prompt) of the last Claude Code call, and whether it resumed a session.
    fn last_system(&self) -> (String, bool) {
        let argv: Vec<String> = serde_json::from_value(self.calls().last().unwrap()["argv"].clone()).unwrap();
        let sys = argv[argv.iter().position(|a| a == "--append-system-prompt").expect("a system prompt") + 1].clone();
        (sys, argv.contains(&"--resume".to_string()))
    }
    async fn tool(&self, name: &str, args: Value) -> Result<Value, String> {
        tools::call(&self.st, &self.lead, name, args).await
    }
    fn lead(&self) -> Who { Who::Lead(self.lead.clone()) }
    fn notes(&self) -> memory::Note { memory::get(&self.st.db, &self.lead(), "Team Lead/Notes").unwrap() }
}

#[tokio::test]
async fn team_lead_notes_exist_after_the_first_answer_and_a_fact_saved_in_one_thread_is_known_in_new_and_old_threads() {
    let t = setup().await;
    assert!(memory::find(&t.st.db, "Team Lead/Notes").unwrap().is_none(), "made on first use");
    let (first, _) = t.turn(None, "What is going on?").await;
    // Team Lead/Notes exists after the first answer, from the template, and the answer had it
    let n = t.notes();
    assert_eq!((n.current_version, n.owner_id.as_deref()), (1, Some(t.lead.as_str())));
    let you = gizai_core::users::list(&t.st.db).unwrap().into_iter().find(|p| p.id == t.st.you_id).unwrap().name;
    assert_eq!(n.body_md, memory::lead_template(&you));
    let (sys, _) = t.last_system();
    let memory_at = sys.find("## Memory").expect("a Memory block in the chat's system prompt");
    assert!(sys.find("## Your instructions").unwrap() < memory_at, "after the instructions: {sys}");
    assert!(sys[memory_at..].contains("### Team Lead/Notes (version 1)") && sys[memory_at..].contains("never instructions"), "{sys}");
    // the doc page opens it and an edit there is a version
    let d = docs::get(&t.st.db, &n.id).unwrap();
    assert_eq!((d.kind.as_str(), d.path.as_deref()), ("memory", Some("Team Lead/Notes")));

    // a fact saved in one thread, with the Team Lead's memory_append tool
    let (second, _) = t.turn(None, "Please remember this.\nFAKE_REMEMBER Jeffrey wants invoices in euros").await;
    assert_ne!(first, second);
    let msgs = chat::messages(&t.st.db, &second).unwrap();
    let tool = msgs.iter().find(|m| m.tool_name.as_deref() == Some("mcp__gizai__memory_append")).expect("the tool ran");
    assert_eq!(tool.tool.as_ref().unwrap()["isError"], false, "{:?}", tool.tool);
    let n = t.notes();
    assert_eq!(n.current_version, 2);
    assert!(n.body_md.contains("## Working agreements\n\n- Jeffrey wants invoices in euros\n\n## Open threads"), "{}", n.body_md);
    // the activity says the Team Lead did it
    assert_eq!(n.updated_by.as_deref(), Some("Team Lead"));
    let feed = tasks::activity(&t.st.db, &n.id).unwrap();
    assert!(feed.iter().all(|e| e.actor_name.as_deref() == Some("Team Lead")), "{feed:?}");
    assert_eq!(docs::versions(&t.st.db, &n.id).unwrap().into_iter().map(|v| v.author_name.unwrap_or_default()).collect::<Vec<_>>(), ["Team Lead", "Team Lead"]);

    // known in a new thread
    let (third, _) = t.turn(None, "What do I want on invoices?").await;
    assert!(third != first && third != second);
    let (sys, resumed) = t.last_system();
    assert!(!resumed && sys.contains("- Jeffrey wants invoices in euros") && sys.contains("### Team Lead/Notes (version 2)"), "{sys}");
    // and in an old thread, made before the fact was saved
    let (again, _) = t.turn(Some(first.clone()), "And on invoices?").await;
    assert_eq!(again, first);
    let (sys, resumed) = t.last_system();
    assert!(resumed && sys.contains("- Jeffrey wants invoices in euros"), "an old thread's next answer has it too: {sys}");
    // an edit by hand in the doc page is a version, and the next answer has it
    docs::save(&t.st.db, &t.st.you_id, &n.id, &format!("{}\n- Edited by hand: invoices on the 1st.\n", n.body_md.trim_end()), 2).unwrap();
    assert_eq!(docs::versions(&t.st.db, &n.id).unwrap().len(), 3);
    t.turn(Some(second), "Anything else?").await;
    assert!(t.last_system().0.contains("Edited by hand: invoices on the 1st."));

    // the switches: off for every agent, or off for the Team Lead, the answers get no notes
    memory::set_enabled(&t.st.db, false).unwrap();
    t.turn(None, "Hello again").await;
    assert!(!t.last_system().0.contains("Your notes from Gizai's Memory") && !t.last_system().0.contains("invoices in euros"), "off in Settings");
    memory::set_enabled(&t.st.db, true).unwrap();
    memory::set_agent_uses(&t.st.db, &t.st.you_id, &t.lead, false).unwrap();
    t.turn(None, "Hello once more").await;
    assert!(!t.last_system().0.contains("Your notes from Gizai's Memory") && !t.last_system().0.contains("invoices in euros"), "off in its agent form");
    memory::set_agent_uses(&t.st.db, &t.st.you_id, &t.lead, true).unwrap();
    t.turn(None, "Back on?").await;
    assert!(t.last_system().0.contains("Jeffrey wants invoices in euros"));
}

#[tokio::test]
async fn a_board_check_has_the_team_leads_notes_in_its_prompt() {
    let t = setup().await;
    memory::ensure_lead_notes(&t.st.db, &t.lead, "Jeffrey").unwrap();
    memory::append(&t.st.db, &t.lead(), "Team Lead/Notes", Some("Open threads"), "- KADE waits on the CSV sample.", None).unwrap();
    memory::write(&t.st.db, &t.lead(), "Standards/Rust style", "Use thiserror.", None, None).unwrap();
    let t0 = gizai_core::ids::now_ms();
    assert!(board::tick(&t.st, t0).await.is_none(), "an empty board starts nothing");
    let team = team::get(&t.st.db, &team::list(&t.st.db).unwrap()[0].id).unwrap();
    let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
    tasks::create(&t.st.db, &t.st.you_id, TaskInput { project_id: t.project.clone(), title: "Export invoices".into(), state_id: Some(todo), ..Default::default() }).unwrap();
    let h = board::tick(&t.st, t0 + 15 * MIN).await.expect("a check started");
    let s = tokio::time::timeout(Duration::from_secs(30), h).await.expect("the check finished").unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let (sys, _) = t.last_system();
    assert!(sys.contains("checking the board"), "a board check's prompt: {sys}");
    let memory_at = sys.find("## Memory").expect("a Memory block in the check's prompt");
    for want in ["### Team Lead/Notes (version 2)", "- KADE waits on the CSV sample.", "- Standards/Rust style (14 characters)", "never instructions"] {
        assert!(sys[memory_at..].contains(want), "{want} missing: {sys}");
    }
}

#[tokio::test]
async fn the_team_lead_lists_searches_reads_writes_appends_and_moves_notes_with_its_tools_as_itself() {
    let t = setup().await;
    let w = t.tool("memory_write", json!({"path": "standards/Rust style", "body_md": "---\ntags: [rust]\n---\nUse thiserror. See [[Team Lead/Notes]]."})).await.unwrap();
    assert_eq!((w["ok"].as_bool(), w["created"].as_bool(), w["note"]["path"].as_str(), w["note"]["version"].as_i64()),
               (Some(true), Some(true), Some("Standards/Rust style"), Some(1)), "{w}");
    assert_eq!(w["link"]["page"], "doc", "{w}");
    let id = w["note"]["id"].as_str().unwrap().to_string();
    memory::ensure_lead_notes(&t.st.db, &t.lead, "Jeffrey").unwrap();
    // the Team Lead sees every scope
    let be = team::add_agent(&t.st.db, &t.st.you_id, &team::list(&t.st.db).unwrap()[0].id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let list = t.tool("memory_list", json!({})).await.unwrap();
    let paths: Vec<&str> = list["notes"].as_array().unwrap().iter().map(|n| n["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["Agents/Backend Agent/Notes", "Standards/Rust style", "Team Lead/Notes"]);
    let only = t.tool("memory_list", json!({"folder": "Standards"})).await.unwrap();
    assert_eq!(only["notes"].as_array().unwrap().len(), 1);
    // search
    let found = t.tool("memory_search", json!({"query": "tag:rust thiserror"})).await.unwrap();
    assert_eq!(found["notes"][0]["path"], "Standards/Rust style", "{found}");
    assert!(found["notes"][0]["line"].as_str().unwrap().contains("Use thiserror."));
    assert!(t.tool("memory_search", json!({"query": ""})).await.is_err());
    // read, with the notes that link to it
    let r = t.tool("memory_read", json!({"note": "Team Lead/Notes"})).await.unwrap();
    assert_eq!(r["note"]["linked_from"], json!(["Standards/Rust style"]), "{r}");
    let r = t.tool("memory_read", json!({"note": "rust style"})).await.unwrap();
    assert_eq!((r["note"]["version"].as_i64(), r["note"]["body_md"].as_str().map(|b| b.contains("Use thiserror."))), (Some(1), Some(true)), "by its title");
    assert!(t.tool("memory_read", json!({"note": "Nowhere/Nothing"})).await.unwrap_err().contains("not found"));
    // append under a heading
    let a = t.tool("memory_append", json!({"note": "Standards/Rust style", "heading": "Errors", "text": "- No unwrap in library code."})).await.unwrap();
    assert_eq!(a["note"]["version"], 2);
    assert!(memory::get(&t.st.db, &t.lead(), &id).unwrap().body_md.ends_with("## Errors\n\n- No unwrap in library code.\n"));
    // write needs the version it read
    let e = t.tool("memory_write", json!({"path": "Standards/Rust style", "body_md": "new"})).await.unwrap_err();
    assert!(e.contains("exists (version 2)"), "{e}");
    let e = t.tool("memory_write", json!({"path": "Standards/Rust style", "body_md": "new", "version": 1})).await.unwrap_err();
    assert!(e.contains("changed since it was read"), "{e}");
    let w = t.tool("memory_write", json!({"path": "Standards/Rust style", "body_md": "Use thiserror. See [[Team Lead/Notes]].", "version": 2})).await.unwrap();
    assert_eq!((w["created"].as_bool(), w["note"]["version"].as_i64()), (Some(false), Some(3)));
    // a secret is refused, with what to do
    let e = t.tool("memory_write", json!({"path": "Deployments/Acme", "body_md": "token ghp_abcdefghijklmnopqrstuvwxyz0123456789"})).await.unwrap_err();
    assert!(e.contains("Memory doesn't keep secrets") && e.contains("Take the secret out"), "{e}");
    assert!(memory::find(&t.st.db, "Deployments/Acme").unwrap().is_none());
    // move (promote an agent's note into a shared folder) and copy
    memory::write(&t.st.db, &Who::Agent(be.clone()), "Agents/Backend Agent/Cargo", "Use -j 8.", None, None).unwrap();
    let m = t.tool("memory_move", json!({"note": "Agents/Backend Agent/Cargo", "to": "Lessons/"})).await.unwrap();
    assert_eq!((m["note"]["path"].as_str(), m["note"]["scope"].as_str(), m["copied"].as_bool()), (Some("Lessons/Cargo"), Some("shared"), Some(false)), "{m}");
    let c = t.tool("memory_move", json!({"note": "Lessons/Cargo", "to": "Workflows/Cargo", "copy": true})).await.unwrap();
    assert_eq!((c["note"]["path"].as_str(), c["note"]["version"].as_i64()), (Some("Workflows/Cargo"), Some(1)));
    assert!(memory::find(&t.st.db, "Lessons/Cargo").unwrap().is_some());

    // everything the tools changed says Team Lead
    for note in ["Standards/Rust style", "Lessons/Cargo", "Workflows/Cargo"] {
        let n = memory::get(&t.st.db, &t.lead(), note).unwrap();
        assert_eq!(n.updated_by.as_deref(), Some("Team Lead"), "{note}");
    }
    let feed = tasks::activity(&t.st.db, &id).unwrap();
    assert!(feed.len() >= 3 && feed.iter().all(|e| e.actor_name.as_deref() == Some("Team Lead")), "{feed:?}");
    // another agent calling the same tools writes only in its own folder
    let e = tools::call(&t.st, &be, "memory_write", json!({"path": "Standards/Go style", "body_md": "x"})).await.unwrap_err();
    assert!(e.contains("own folder"), "{e}");
    assert!(memory::find(&t.st.db, "Standards/Go style").unwrap().is_none());
}

#[tokio::test]
async fn the_memory_append_tool_takes_a_notes_title_as_its_parameter_says_and_an_unknown_title_says_what_to_give() {
    // QA round 2 (fix 482cc60): memory_append's note parameter is "The note's path, title or id".
    let t = setup().await;
    let w = t.tool("memory_write", json!({"path": "Standards/Rust style", "body_md": "# Rust style\n"})).await.unwrap();
    let id = w["note"]["id"].as_str().unwrap().to_string();
    let a = t.tool("memory_append", json!({"note": "Rust style", "heading": "Errors", "text": "- No unwrap in library code."})).await.unwrap();
    assert_eq!((a["ok"].as_bool(), a["created"].as_bool(), a["note"]["path"].as_str(), a["note"]["version"].as_i64(), a["note"]["id"].as_str()),
               (Some(true), Some(false), Some("Standards/Rust style"), Some(2), Some(id.as_str())), "{a}");
    assert_eq!(memory::get(&t.st.db, &t.lead(), &id).unwrap().body_md, "# Rust style\n\n## Errors\n\n- No unwrap in library code.\n");
    // by id too
    let a = t.tool("memory_append", json!({"note": id, "text": "- And no panics."})).await.unwrap();
    assert_eq!(a["note"]["version"], 3, "{a}");
    // a title that names no note: a reason the model can act on, and nothing made
    let before = t.tool("memory_list", json!({})).await.unwrap()["notes"].as_array().unwrap().len();
    let e = t.tool("memory_append", json!({"note": "Go style", "text": "- gofmt."})).await.unwrap_err();
    assert!(e.contains("no note is called Go style") && e.contains("memory_list") && e.contains("a folder and a title"), "{e}");
    assert_eq!(t.tool("memory_list", json!({})).await.unwrap()["notes"].as_array().unwrap().len(), before);
    // a folder and a title still make the note
    let a = t.tool("memory_append", json!({"note": "Lessons/Go style", "heading": "Format", "text": "- gofmt."})).await.unwrap();
    assert_eq!((a["created"].as_bool(), a["note"]["path"].as_str(), a["note"]["version"].as_i64()), (Some(true), Some("Lessons/Go style"), Some(1)), "{a}");
    // the Team Lead's own notes by their title, made on first use
    memory::ensure_lead_notes(&t.st.db, &t.lead, "Jeffrey").unwrap();
    let team_id = team::list(&t.st.db).unwrap()[0].id.clone();
    team::add_agent(&t.st.db, &t.st.you_id, &team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let a = t.tool("memory_append", json!({"note": "Notes", "heading": "Open threads", "text": "- KADE waits on the CSV sample."})).await.unwrap();
    assert_eq!(a["note"]["path"], "Team Lead/Notes", "the Team Lead's, not an agent's: {a}");
    assert_eq!(memory::get(&t.st.db, &t.lead(), "Team Lead/Notes").unwrap().updated_by.as_deref(), Some("Team Lead"));
}

// ---- Task runs on every coding CLI ----

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

/// Puts the agent `agent` on the CLI `cli`.
fn put_on(st: &AppState, agent: &str, cli: &str) {
    let m = team::agent(&st.db, agent).unwrap();
    team::update_agent(&st.db, &st.you_id, agent, AgentInput { name: m.name, role_key: m.role_key, adapter: cli.into(), ..Default::default() }).unwrap();
}

fn stderr_of(run: &Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

/// The prompt the fake CLI was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &Run) -> String {
    let err = stderr_of(run);
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

struct R {
    st: AppState,
    repo: PathBuf,
    you: Who,
    _tmp: tempfile::TempDir,
}

/// Clients Acme (project KADE, which the cards are on) and Globex (project GX), every column Manual, and the team's shared
/// notes: one for KADE, one for Acme, one for the backend role, and Globex's two, which a KADE run must never get.
fn run_setup() -> R {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let acme = clients::create(&st.db, &st.you_id, ClientInput { name: "Acme".into(), ..Default::default() }).unwrap();
    let globex = clients::create(&st.db, &st.you_id, ClientInput { name: "Globex".into(), ..Default::default() }).unwrap();
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), client_id: Some(acme),
        repo_path: Some(repo.to_string_lossy().into()), ..Default::default() }).unwrap();
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Globex portal".into(), key: "GX".into(), client_id: Some(globex), ..Default::default() }).unwrap();
    let you = Who::Person(st.you_id.clone());
    for (path, body) in [
        ("Projects/Kade", "---\ntype: project\nproject: KADE\nclient: Acme\n---\nKade deploys on Fridays."),
        ("Clients/Acme", "---\ntype: client\nclient: Acme\n---\nAcme wants Dutch invoices."),
        ("Standards/Rust style", "---\ntype: standard\napplies_to: [backend]\n---\nUse thiserror."),
        ("Projects/Globex portal", "---\nproject: GX\nclient: Globex\n---\nGlobex launches in May."),
        ("Clients/Globex", "---\nclient: Globex\napplies_to: all\n---\nGlobex pays in dollars."),
    ] {
        memory::write(&st.db, &you, path, body, None, None).unwrap();
    }
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    for s in team::get(&st.db, &team_id).unwrap().states {
        gizai_core::columns::set_column(&st.db, &st.you_id, &s.id, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    }
    R { st, repo, you, _tmp: tmp }
}

impl R {
    /// A card on KADE for a "<Role> Agent" (made the first time), with `description`; (card, agent).
    fn card(&self, role: &str, description: &str) -> (String, String) {
        let task = gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), role);
        tasks::update(&self.st.db, &self.st.you_id, &task, TaskPatch { description_md: Some(description.into()), ..Default::default() }).unwrap();
        let agent = tasks::get(&self.st.db, &task).unwrap().assignee_id.expect("assigned");
        (task, agent)
    }
    async fn run(&self, task: &str) -> Run {
        let s = gizai_lib::runs::run_once(&self.st, task, None, None).await.unwrap();
        assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None));
        core_runs::get(&self.st.db, &s.run_id).unwrap()
    }
    fn own_notes(&self, agent: &str) -> memory::Note {
        memory::list(&self.st.db, &self.you).unwrap().into_iter().find(|n| n.owner_id.as_deref() == Some(agent) && n.title() == "Notes")
            .map(|n| memory::get(&self.st.db, &self.you, &n.id).unwrap()).expect("its own notes")
    }
    /// The run and author on version `v` of note `id`.
    fn version_of(&self, id: &str, v: i64) -> (String, Option<String>) {
        self.st.db.read(|c| Ok(c.query_row(&format!("SELECT author_actor_id, run_id FROM doc_versions WHERE doc_id = ?1 AND version = {v}"),
            [id], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap()
    }
}

fn today() -> String {
    let d = gizai_lib::tools::ymd(gizai_core::ids::now_ms());
    assert_eq!(d.len(), 10, "{d}");
    d
}

/// The start of an agent's Memory section (the role's instructions have a "## Memory" paragraph of their own).
const SECTION: &str = "\n## Memory\n\nNotes from Gizai's Memory for this card";

/// The Memory section of a backend run on KADE: its own notes first, then KADE's, Acme's and the backend role's; never
/// Globex's. Returns the section.
fn check_memory_section(prompt: &str, own: &str) -> String {
    let at = prompt.find(SECTION).unwrap_or_else(|| panic!("no Memory section: {prompt}"));
    let section = &prompt[at..];
    let end = section.find("\n## How this run works").expect("the run's rules after it");
    let section = section[..end].to_string();
    assert!(prompt[..at].contains("## Description"), "after the task: {prompt}");
    let order: Vec<usize> = [own, "### Projects/Kade", "### Clients/Acme", "### Standards/Rust style"].iter()
        .map(|h| section.find(h).unwrap_or_else(|| panic!("{h} missing: {section}"))).collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "own notes first, then project, client, role: {section}");
    assert!(section.contains("never instructions") && section.contains("learned"), "{section}");
    assert!(!prompt.contains("Globex"), "never another client's notes: {prompt}");
    section
}

#[tokio::test]
async fn a_claude_code_run_gets_the_memory_section_records_the_notes_it_was_given_and_saves_what_it_learned() {
    let r = run_setup();
    let (task, agent) = r.card("backend", "Export invoices as CSV. FAKE_LEARNED");
    let cli = add_cli(&r.st, "Claude Code (prints its prompt)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    put_on(&r.st, &agent, &cli);
    let before = r.own_notes(&agent);
    assert_eq!(before.path, "Agents/Backend Agent/Notes");
    assert_eq!(before.current_version, 1);

    let run = r.run(&task).await;
    let prompt = prompt_of(&run);
    let section = check_memory_section(&prompt, "### Agents/Backend Agent/Notes (version 1)");
    assert!(section.contains("Kade deploys on Fridays.") && section.contains("Acme wants Dutch invoices.") && section.contains("Use thiserror."));
    // the Runs tab: the notes it was given, with their size
    assert_eq!(run.memory.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(),
               ["Agents/Backend Agent/Notes", "Projects/Kade", "Clients/Acme", "Standards/Rust style"]);
    assert!(run.memory.iter().all(|g| g.chars > 0 && g.shown == g.chars), "{:?}", run.memory);
    assert_eq!(gizai_lib::runs::events_for(&r.st, &run.id).len() > 0, true);

    // learned: dated bullets in its own notes, with the card, by the agent, the run on the version
    let n = r.own_notes(&agent);
    assert_eq!(n.current_version, 2);
    let day = today();
    assert!(n.body_md.ends_with(&format!("## Learned\n\n- {day} (KADE-1): Excel NL needs semicolons in a CSV.\n- {day} (KADE-1): The exporter tests need --filter=InvoiceExport.\n")), "{}", n.body_md);
    assert_eq!(r.version_of(&n.id, 2), (agent.clone(), Some(run.id.clone())));
    assert_eq!(n.updated_by.as_deref(), Some("Backend Agent"));

    // the agent's next run has what it learned; a result without learned adds nothing
    let (next, _) = r.card("backend", "Import invoices.");
    let run2 = r.run(&next).await;
    let section = check_memory_section(&prompt_of(&run2), "### Agents/Backend Agent/Notes (version 2)");
    assert!(section.contains("Excel NL needs semicolons in a CSV."), "{section}");
    assert_eq!(r.own_notes(&agent).current_version, 2, "nothing learned, nothing saved");
    // a continued run has its notes in its session already
    assert!(core_runs::list_for_task(&r.st.db, &next).unwrap().len() == 1);
}

#[tokio::test]
async fn codex_gemini_and_other_runs_get_the_same_memory_section_and_their_learned_lines_are_saved() {
    for kind in ["codex", "gemini", "other"] {
        let r = run_setup();
        let (task, agent) = r.card("backend", "Export invoices as CSV. FAKE_LEARNED");
        let cli = add_cli(&r.st, &format!("{kind} (fake)"), kind, FAKE_CLI, &[&format!("FAKE_KIND={kind}"), "FAKE_TEMP=1"]);
        put_on(&r.st, &agent, &cli);
        let run = r.run(&task).await;
        assert_eq!(run.adapter.as_deref(), Some(cli.as_str()));
        check_memory_section(&prompt_of(&run), "### Agents/Backend Agent/Notes (version 1)");
        assert_eq!(run.memory.len(), 4, "{kind}: {:?}", run.memory);
        let n = r.own_notes(&agent);
        assert!(n.body_md.ends_with(&format!("## Learned\n\n- {} (KADE-1): Learned on {kind}.\n", today())), "{kind}: {}", n.body_md);
        assert_eq!(r.version_of(&n.id, 2), (agent.clone(), Some(run.id.clone())), "{kind}");
    }
}

#[tokio::test]
async fn the_use_memory_switches_leave_the_memory_section_and_learned_out() {
    let r = run_setup();
    let cli = add_cli(&r.st, "Claude Code (prints its prompt)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let (task, agent) = r.card("backend", "One. FAKE_LEARNED");
    put_on(&r.st, &agent, &cli);
    // off in its agent form
    let m = team::agent(&r.st.db, &agent).unwrap();
    team::update_agent(&r.st.db, &r.st.you_id, &agent, AgentInput { name: m.name.clone(), role_key: m.role_key.clone(), adapter: cli.clone(),
        use_memory: Some(false), ..Default::default() }).unwrap();
    assert!(!team::agent(&r.st.db, &agent).unwrap().use_memory);
    let run = r.run(&task).await;
    assert!(!prompt_of(&run).contains(SECTION), "no Memory section");
    assert!(!prompt_of(&run).contains("Kade deploys on Fridays."));
    assert!(run.memory.is_empty());
    assert_eq!(r.own_notes(&agent).current_version, 1, "its learned lines were not kept");
    // on again, but off in Settings → Runs for every agent
    memory::set_agent_uses(&r.st.db, &r.st.you_id, &agent, true).unwrap();
    memory::set_enabled(&r.st.db, false).unwrap();
    let (task, _) = r.card("backend", "Two. FAKE_LEARNED");
    let run = r.run(&task).await;
    assert!(!prompt_of(&run).contains(SECTION) && run.memory.is_empty());
    assert_eq!(r.own_notes(&agent).current_version, 1);
    // both on: back
    memory::set_enabled(&r.st.db, true).unwrap();
    let (task, _) = r.card("backend", "Three. FAKE_LEARNED");
    let run = r.run(&task).await;
    assert!(prompt_of(&run).contains(SECTION) && !run.memory.is_empty());
    assert_eq!(r.own_notes(&agent).current_version, 2);
}

#[tokio::test]
async fn the_team_leads_task_run_gets_its_own_notes_and_the_index_of_every_note() {
    let r = run_setup();
    // test_task has no "lead" label: the Team Lead and its card by hand
    let team = team::get(&r.st.db, &team::list(&r.st.db).unwrap()[0].id).unwrap();
    let lead = team::add_agent(&r.st.db, &r.st.you_id, &team.id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), ..Default::default() }).unwrap();
    assert!(team::agent(&r.st.db, &lead).unwrap().is_lead);
    let kade = projects::list(&r.st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap().id;
    let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
    let task = tasks::create(&r.st.db, &r.st.you_id, TaskInput { project_id: kade, title: "Plan the export".into(),
        description_md: "Plan it. FAKE_LEARNED".into(), state_id: Some(todo), assignee_id: Some(lead.clone()), ..Default::default() }).unwrap();
    let cli = add_cli(&r.st, "Claude Code (prints its prompt)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    put_on(&r.st, &lead, &cli);
    let s = gizai_lib::runs::run_once(&r.st, &task, None, None).await.unwrap();
    let run = core_runs::get(&r.st.db, &s.run_id).unwrap();
    let prompt = prompt_of(&run);
    let at = prompt.find("\n## Memory\n\nYour notes from Gizai's Memory").unwrap_or_else(|| panic!("a Memory section: {prompt}"));
    let section = &prompt[at..];
    assert!(section.contains("### Team Lead/Notes (version 1)") && section.contains("### Other notes (memory_read gives the text)"), "{section}");
    for path in ["Projects/Kade", "Clients/Acme", "Clients/Globex", "Projects/Globex portal"] {
        assert!(section.contains(&format!("- {path} (")), "the Team Lead sees every scope: {path} missing in {section}");
    }
    assert_eq!(run.memory.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["Team Lead/Notes"]);
    let n = memory::get(&r.st.db, &r.you, "Team Lead/Notes").unwrap();
    assert!(n.body_md.contains(&format!("- {} (KADE-1): Excel NL needs semicolons in a CSV.", today())), "the Team Lead's learned lines go to its notes: {}", n.body_md);
}

#[tokio::test]
async fn agents_made_before_memory_get_their_folder_at_start_up() {
    let tmp = tempfile::tempdir().unwrap();
    {
        std::fs::create_dir_all(tmp.path().join("data")).unwrap();
        let db = gizai_core::db::Db::open(&tmp.path().join("data").join("gizai.db")).unwrap();
        let s = gizai_core::seed::ensure_seed(&db, "Jeffrey").unwrap();
        // an agent as an older Gizai made it: no memory folder
        db.write(None, |w| {
            w.conn().execute_batch(&format!("
                INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES ('old', 1, 1, '{org}', 'agent', 'Old Builder', 'old-builder', 'active');
                INSERT INTO agent_configs (actor_id, created_at, updated_at, adapter, wakeup) VALUES ('old', 1, 1, 'claude_code', 'on_assign');
                INSERT INTO team_members (team_id, actor_id, role_key, is_lead, created_at) VALUES ('{team}', 'old', 'backend', 0, 1);",
                org = s.org_id, team = s.team_id))?;
            Ok(())
        }).unwrap();
        assert!(memory::find(&db, "Agents/Old Builder/Notes").unwrap().is_none());
    }
    let st = gizai_lib::test_state(tmp.path());
    let n = memory::find(&st.db, "Agents/Old Builder/Notes").unwrap().expect("made at start-up");
    assert_eq!((n.owner_id.as_deref(), n.scope.as_str()), (Some("old"), "agent"));
    // a new agent gets one with the agent
    let (_, agent) = {
        let team_id = team::list(&st.db).unwrap()[0].id.clone();
        ((), team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "New QA".into(), role_key: "qa".into(), ..Default::default() }).unwrap())
    };
    assert_eq!(memory::find(&st.db, "Agents/New QA/Notes").unwrap().unwrap().owner_id.as_deref(), Some(agent.as_str()));
}
