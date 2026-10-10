// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
//! GA-85 QA: every Claude Code that Gizai starts (a task run, a chat answer, a board check) gets
//! CLAUDE_CODE_DISABLE_AUTO_MEMORY=1, Claude Code's own switch for its memory folder, also on a second account whose
//! environment lines say otherwise; Codex and Gemini don't. The fakes write what they got (fake-claude.sh and
//! fake-cli.sh on stderr as `auto memory: …`, fake-claude-chat.py as `auto_memory` in fake-calls.jsonl), never the real
//! CLIs.
use std::path::PathBuf;
use std::time::Duration;

use gizai_core::clis::Cli;
use gizai_core::model::*;
use gizai_core::{projects, settings, tasks, team};
use gizai_lib::{AppState, board, chat as app_chat, mcp};
use serde_json::Value;

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const CHAT_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py");
const MIN: i64 = 60_000;

fn git_repo(dir: &std::path::Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &AppState, name: &str, kind: &str, command: &str, env: &[String]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.to_vec(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Moves agent `agent` to the CLI `cli`.
fn put_on(st: &AppState, agent: &str, cli: &str) {
    let m = team::agent(&st.db, agent).unwrap();
    team::update_agent(&st.db, &st.you_id, agent, AgentInput { name: m.name, role_key: m.role_key, adapter: cli.into(), ..Default::default() }).unwrap();
}

fn stderr_of(st: &AppState, run_id: &str) -> String {
    let run = gizai_core::runs::get(&st.db, run_id).unwrap();
    std::fs::read_to_string(std::path::Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

/// A second Claude Code account whose own environment lines try to turn its memory back on (the fake only reads its
/// folder in the test's own folder).
fn second_claude(st: &AppState, command: &str) -> String {
    let dir = st.data_dir.join("acct-2").display().to_string();
    add_cli(st, "Claude Code 2", "claude_code", command,
            &[format!("CLAUDE_CONFIG_DIR={dir}"), "CLAUDE_CODE_DISABLE_AUTO_MEMORY=0".into(), "FAKE_HAS_USAGE=1".into()])
}

#[test]
fn the_switch_is_claude_codes_own_variable() {
    assert_eq!(gizai_agents::claude::AUTO_MEMORY_OFF, "CLAUDE_CODE_DISABLE_AUTO_MEMORY");
    // These tests only mean something when the test itself doesn't pass the switch on to the fakes.
    assert_ne!(std::env::var("CLAUDE_CODE_DISABLE_AUTO_MEMORY").ok().as_deref(), Some("1"), "unset it to run these tests");
}

#[tokio::test]
async fn a_task_run_starts_claude_code_with_its_own_memory_off_also_on_another_account() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    // the built-in Claude Code
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE_CLAUDE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let err = stderr_of(&st, &s.run_id);
    assert!(err.contains("auto memory: 1\n"), "{err}");
    assert!(err.contains(r#"--settings {"disableAllHooks":true}"#), "the hooks stay off too: {err}");

    // a second account that says CLAUDE_CODE_DISABLE_AUTO_MEMORY=0 in its own lines: Gizai's switch comes after them
    let cc2 = second_claude(&st, FAKE_CLAUDE);
    let task2 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let (_, agent) = team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    put_on(&st, &agent.actor_id, &cc2);
    let s = gizai_lib::runs::run_once(&st, &task2, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert_eq!(gizai_core::runs::get(&st.db, &s.run_id).unwrap().adapter.as_deref(), Some(cc2.as_str()));
    let err = stderr_of(&st, &s.run_id);
    assert!(err.contains("auto memory: 1\n"), "{err}");
}

#[tokio::test]
async fn codex_and_gemini_runs_get_no_such_switch() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let (_, agent) = {
        gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap()
    };
    for kind in ["codex", "gemini"] {
        let cli = add_cli(&st, kind, kind, FAKE_CLI, &[format!("FAKE_KIND={kind}")]);
        put_on(&st, &agent.actor_id, &cli);
        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{kind}: {s:?}");
        let err = stderr_of(&st, &s.run_id);
        assert!(err.contains("auto memory: unset\n"), "{kind}: {err}");
    }
}

// ---- The Team Lead: chat answers and board checks ----

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
    async fn turn(&self, text: &str) {
        let (_, done) = app_chat::send(&self.st, None, text.into(), None).await.unwrap();
        let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        assert_eq!(s.status, "succeeded", "{s:?}");
    }
    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-calls.jsonl")).unwrap_or_default()
            .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
    /// Starts a board check (a card in To do), waits for it, and returns the fake's record of it.
    async fn check(&self, at: i64) -> Value {
        let team = team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap();
        let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: self.project.clone(), title: format!("Card {at}"),
            state_id: Some(todo), ..Default::default() }).unwrap();
        let n = self.calls().len();
        let h = board::tick(&self.st, at).await.expect("a check started");
        let s = tokio::time::timeout(Duration::from_secs(30), h).await.expect("the check finished").unwrap();
        assert_eq!(s.status, "succeeded", "{s:?}");
        let calls = self.calls();
        assert_eq!(calls.len(), n + 1, "one call for the check");
        let call = calls.last().unwrap().clone();
        assert!(call["prompt"].as_str().unwrap_or_default().contains("checking the board")
            || call["argv"].to_string().contains("checking the board"), "a board check: {call}");
        call
    }
}

#[tokio::test]
async fn a_chat_answer_and_a_board_check_start_claude_code_with_its_own_memory_off_also_on_another_account() {
    let t = setup().await;
    t.turn("What is going on?").await;
    let calls = t.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["auto_memory"], "1", "a chat answer: {}", calls[0]);

    let t0 = gizai_core::ids::now_ms();
    assert!(board::tick(&t.st, t0).await.is_none(), "an empty board starts nothing");
    let call = t.check(t0 + 15 * MIN).await;
    assert_eq!(call["auto_memory"], "1", "a board check: {call}");
    let argv: Vec<String> = serde_json::from_value(call["argv"].clone()).unwrap();
    assert!(argv.contains(&"--no-session-persistence".to_string()), "a board check's flags: {argv:?}");

    // the Team Lead on a second account whose lines say CLAUDE_CODE_DISABLE_AUTO_MEMORY=0: both still off
    let cc2 = second_claude(&t.st, CHAT_FAKE);
    put_on(&t.st, &t.lead, &cc2);
    t.turn("And now?").await;
    let call = t.calls().last().unwrap().clone();
    assert_eq!(call["account"], t.st.data_dir.join("acct-2").display().to_string(), "on the second account: {call}");
    assert_eq!(call["auto_memory"], "1", "a chat answer on another account: {call}");
    let call = t.check(t0 + 45 * MIN).await;
    assert_eq!(call["account"], t.st.data_dir.join("acct-2").display().to_string(), "on the second account: {call}");
    assert_eq!(call["auto_memory"], "1", "a board check on another account: {call}");
}
