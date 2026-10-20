// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-162: the first start's onboarding in the app. A new install (`open_state` on an empty data folder) waits for it;
// finishing saves the names and the agents' CLI, then starts the Team Lead's welcome chat on a (fake) Claude Code and
// returns its id, or None when no CLI can run the chat; skipping only ends it. GIZAI_SKIP_ONBOARDING is in
// onboarding_skip_env_test.rs: the environment belongs to the whole process.
#[path = "../support/data_lock.rs"]
mod data_lock;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gizai_core::onboarding::{OnboardingInput, WELCOME};
use gizai_core::{chat, clis, settings, team, users};
use gizai_lib::runs::Note;
use gizai_lib::{AppState, mcp, onboarding};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py");

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
    notes: Arc<Mutex<Vec<String>>>,
    _dir: tempfile::TempDir,
    _server: tokio::task::JoinHandle<()>,
}

/// A new install: `open_state` on an empty data folder, with Claude Code the fake one at `claude_bin`.
async fn new_install(claude_bin: &str) -> T {
    let dir = tempfile::tempdir().unwrap();
    let notes: Arc<Mutex<Vec<String>>> = Arc::default();
    let n2 = notes.clone();
    let mut st = gizai_lib::open_state(dir.path().join("data"), Arc::new(move |n: Note| {
        let s = match n {
            Note::RowsChanged(t) => format!("rows:{t}"),
            Note::ChatChanged => "chat-changed".into(),
            _ => "other".into(),
        };
        n2.lock().unwrap().push(s);
    })).unwrap();
    st.mcp_socket = st.data_dir.join("mcp.sock");
    st.mcp_shim = Some(shim());
    settings::set(&st.db, "claude_bin", &claude_bin.to_string()).unwrap();
    let server = mcp::start(&st).unwrap();
    T { st, notes, _dir: dir, _server: server }
}

fn input(you: &str, team: &str, cli: Option<&str>) -> OnboardingInput {
    OnboardingInput { you_name: you.into(), team_name: team.into(), agents_cli: cli.map(str::to_string) }
}

fn you(st: &AppState) -> (String, String) {
    let p = users::list(&st.db).unwrap().into_iter().find(|p| p.id == st.you_id).expect("you");
    (p.name, p.handle)
}

/// The first team's name and its agents as (name, adapter, permission mode), by name.
fn team_now(st: &AppState) -> (String, Vec<(String, String, String)>) {
    let t = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    let mut a: Vec<_> = t.members.into_iter().filter(|m| m.kind == "agent")
        .map(|m| (m.name, m.adapter.unwrap_or_default(), m.permission_mode.unwrap_or_default())).collect();
    a.sort();
    (t.name, a)
}

fn calls(st: &AppState) -> Vec<serde_json::Value> {
    std::fs::read_to_string(st.data_dir.join("chat/fake-calls.jsonl")).unwrap_or_default()
        .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[tokio::test]
async fn a_new_install_waits_until_the_onboarding_is_finished_and_the_welcome_chat_gets_its_answer() {
    let t = new_install(FAKE).await;
    assert!(onboarding::pending(&t.st), "a new install waits for the onboarding");
    // Claude Code can run the chat: the Team Lead's CLI is offered without a problem.
    let lead_cli = gizai_lib::chat::clis(&t.st).unwrap().into_iter().find(|c| c.id == clis::CLAUDE_CODE).unwrap();
    assert_eq!(lead_cli.problem, None);
    // A second CLI for the agents.
    clis::save(&t.st.db, vec![clis::Cli { name: "Codex".into(), kind: "codex".into(), command: "/opt/codex/bin/codex".into(), ..Default::default() }]).unwrap();
    let codex = clis::list(&t.st.db).unwrap().into_iter().find(|c| c.name == "Codex").unwrap().id;

    let (thread, answer) = onboarding::finish(&t.st, input(" Jeffrey Sevinga ", " YoTech ", Some(&codex))).await.unwrap()
        .expect("the welcome chat started");
    assert!(!onboarding::pending(&t.st), "finished: no longer waits");

    // The names, the handle and the agents' CLI.
    assert_eq!(you(&t.st), ("Jeffrey Sevinga".into(), "jeffrey-sevinga".into()));
    let (team_name, agents) = team_now(&t.st);
    assert_eq!(team_name, "YoTech");
    let mut want: Vec<(String, String, String)> = ["Backend Agent", "DevOps Agent", "Frontend Agent", "QA Agent"].iter()
        .map(|n| (n.to_string(), codex.clone(), "workspace-write".to_string())).collect();
    want.push(("Team Lead".into(), clis::CLAUDE_CODE.into(), "acceptEdits".into()));
    want.sort();
    assert_eq!(agents, want);

    // The chat: a new one, with the Team Lead, whose first message is yours and the welcome text word for word.
    let threads = chat::list_threads(&t.st.db).unwrap();
    assert_eq!(threads.iter().map(|th| th.id.as_str()).collect::<Vec<_>>(), [thread.as_str()], "one new chat");
    let lead = team::chat_agent(&t.st.db).unwrap().unwrap();
    assert_eq!(lead.name, "Team Lead");
    let th = chat::get_thread(&t.st.db, &thread).unwrap();
    assert_eq!(th.agent_id, lead.actor_id);
    let first = chat::messages(&t.st.db, &thread).unwrap().into_iter().next().expect("a first message");
    assert_eq!(first.role, "user");
    assert_eq!(first.author_id.as_deref(), Some(t.st.you_id.as_str()), "your message");
    assert_eq!(first.body_md.as_deref(), Some(WELCOME));

    // The Team Lead's answer runs to its end on the fake Claude Code, which got the welcome text.
    let summary = tokio::time::timeout(std::time::Duration::from_secs(30), answer).await.expect("the answer ended").unwrap();
    assert_eq!(summary.status, "succeeded", "{summary:?}");
    let roles: Vec<String> = chat::messages(&t.st.db, &thread).unwrap().into_iter().map(|m| m.role).collect();
    assert_eq!(roles[0], "user");
    assert!(roles.iter().skip(1).any(|r| r == "agent"), "the Team Lead answered: {roles:?}");
    let calls = calls(&t.st);
    assert_eq!(calls.len(), 1, "one turn");
    assert!(calls[0]["prompt"].as_str().unwrap().contains(WELCOME), "{}", calls[0]["prompt"]);

    // The UI heard about the new names.
    let notes = t.notes.lock().unwrap().clone();
    assert!(notes.iter().any(|n| n == "rows:actors"), "{notes:?}");
    assert!(notes.iter().any(|n| n == "rows:teams"), "{notes:?}");

    // Once is enough: nothing to set up, and no second chat.
    let e = onboarding::finish(&t.st, input("Someone", "Else", None)).await.unwrap_err();
    assert_eq!(e, "Nothing to set up");
    assert_eq!(chat::list_threads(&t.st.db).unwrap().len(), 1);
    assert_eq!(you(&t.st).0, "Jeffrey Sevinga");
}

#[tokio::test]
async fn without_a_cli_that_can_run_the_chat_it_saves_the_names_and_returns_none() {
    let t = new_install("/nowhere/claude").await;
    assert!(onboarding::pending(&t.st));
    let lead_cli = gizai_lib::chat::clis(&t.st).unwrap().into_iter().find(|c| c.id == clis::CLAUDE_CODE).unwrap();
    assert!(lead_cli.problem.is_some(), "Claude Code isn't found");

    let started = onboarding::finish(&t.st, input("Jeffrey", "YoTech", None)).await.unwrap();
    assert!(started.is_none(), "no chat");
    assert!(chat::list_threads(&t.st.db).unwrap().is_empty(), "no chat started");
    assert!(calls(&t.st).is_empty(), "no CLI ran");
    assert_eq!(you(&t.st), ("Jeffrey".into(), "jeffrey".into()));
    assert_eq!(team_now(&t.st).0, "YoTech");
    assert!(!onboarding::pending(&t.st));
}

#[tokio::test]
async fn a_failing_finish_changes_nothing_and_starts_no_chat() {
    let t = new_install(FAKE).await;
    let before = (you(&t.st), team_now(&t.st));
    for (i, want) in [
        (input("  ", "YoTech", None), "Your name can't be empty"),
        (input("Jeffrey", "", None), "The team's name can't be empty"),
        (input("Jeffrey", "YoTech", Some("gone")), "No coding CLI with the id gone"),
    ] {
        let e = onboarding::finish(&t.st, i.clone()).await.unwrap_err();
        assert_eq!(e, want, "{i:?}");
        assert_eq!((you(&t.st), team_now(&t.st)), before, "{i:?} changed something");
        assert!(onboarding::pending(&t.st), "{i:?}: still waits");
    }
    assert!(chat::list_threads(&t.st.db).unwrap().is_empty(), "no chat started");
    assert!(calls(&t.st).is_empty());
}

#[tokio::test]
async fn skipping_ends_the_onboarding_and_changes_nothing_else() {
    let t = new_install(FAKE).await;
    let before = (you(&t.st), team_now(&t.st), clis::list(&t.st.db).unwrap());
    onboarding::skip(&t.st).unwrap();
    assert!(!onboarding::pending(&t.st));
    assert_eq!((you(&t.st), team_now(&t.st), clis::list(&t.st.db).unwrap()), before);
    assert!(chat::list_threads(&t.st.db).unwrap().is_empty(), "no chat");
    assert!(t.notes.lock().unwrap().iter().any(|n| n == "rows:settings"));
    // Again: nothing waits, nothing happens.
    t.notes.lock().unwrap().clear();
    onboarding::skip(&t.st).unwrap();
    assert!(!onboarding::pending(&t.st));
    assert!(t.notes.lock().unwrap().is_empty(), "nothing changed, so nothing to tell");
    assert_eq!(onboarding::finish(&t.st, input("Jeffrey", "YoTech", None)).await.unwrap_err(), "Nothing to set up");
}

#[tokio::test]
async fn a_tests_gizai_and_a_database_seeded_before_never_wait() {
    let dir = tempfile::tempdir().unwrap();
    // A test's Gizai seeds with ensure_seed, as the demo data does.
    let st = gizai_lib::test_state(dir.path());
    assert!(!onboarding::pending(&st));
    let you_id = st.you_id.clone();
    drop(st);
    // That data, opened again as a new install's Gizai opens it: seeded before, so it doesn't wait.
    data_lock::released(&dir.path().join("data")).await;
    let st = gizai_lib::open_state(dir.path().join("data"), Arc::new(|_| {})).unwrap();
    assert_eq!(st.you_id, you_id);
    assert!(!onboarding::pending(&st));
    assert_eq!(onboarding::finish(&st, input("Jeffrey", "YoTech", None)).await.unwrap_err(), "Nothing to set up");
    drop(st);

    // A new install that wasn't finished still waits at its next start.
    let other = tempfile::tempdir().unwrap();
    let st = gizai_lib::open_state(other.path().join("data"), Arc::new(|_| {})).unwrap();
    assert!(onboarding::pending(&st));
    drop(st);
    data_lock::released(&other.path().join("data")).await;
    let st = gizai_lib::open_state(other.path().join("data"), Arc::new(|_| {})).unwrap();
    assert!(onboarding::pending(&st), "it still waits after a restart");
}
