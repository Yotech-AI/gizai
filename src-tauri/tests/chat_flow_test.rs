// Chat turns end to end: Gizai starts a (fake) Claude Code, which starts the real gizai-mcp shim, which calls
// Gizai's tools over the socket; the conversation, the run and the session are recorded.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gizai_core::chat;
use gizai_core::model::*;
use gizai_core::{projects, runs as core_runs, tasks, team};
use gizai_lib::runs::Note;
use gizai_lib::{AppState, chat as app_chat, mcp};
use serde_json::Value;

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
    lead: String,
    notes: Arc<Mutex<Vec<String>>>,
    _dir: tempfile::TempDir,
    _server: tokio::task::JoinHandle<()>,
}

async fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let notes: Arc<Mutex<Vec<String>>> = Arc::default();
    let n2 = notes.clone();
    let mut st = gizai_lib::open_state(dir.path().join("data"), Arc::new(move |n: Note| {
        let s = match n {
            Note::Chat { thread_id: _, event } => format!("chat:{}", serde_json::to_value(&event).unwrap()["kind"].as_str().unwrap()),
            Note::ChatChanged => "chat-changed".into(),
            Note::RowsChanged(t) => format!("rows:{t}"),
            _ => "other".into(),
        };
        n2.lock().unwrap().push(s);
    })).unwrap();
    st.mcp_socket = st.data_dir.join("mcp.sock");
    st.mcp_shim = Some(shim());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let server = mcp::start(&st).unwrap();
    T { st, lead, notes, _dir: dir, _server: server }
}

impl T {
    async fn turn(&self, thread: Option<String>, text: &str) -> (String, app_chat::TurnSummary) {
        let (id, done) = app_chat::send(&self.st, thread, text.into(), None).await.unwrap();
        let summary = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        (id, summary)
    }
    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-calls.jsonl")).unwrap_or_default()
            .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
    fn roles(&self, thread: &str) -> Vec<String> {
        chat::messages(&self.st.db, thread).unwrap().into_iter().map(|m| m.role).collect()
    }
}

#[tokio::test]
async fn a_chat_turn_runs_tools_and_saves_the_conversation() {
    let t = setup().await;
    let (thread, s) = t.turn(None, "create task Chat probe task in KADE").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    // the tool really ran, as the Team Lead
    let made = tasks::list(&t.st.db, &TaskFilter::default()).unwrap().into_iter().find(|x| x.title == "Chat probe task").expect("task created");
    assert_eq!(made.identifier, "KADE-1");
    assert_eq!(tasks::activity(&t.st.db, &made.id).unwrap()[0].actor_name.as_deref(), Some("Team Lead"));
    // the conversation
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    assert_eq!(msgs.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(), ["user", "agent", "tool", "agent"]);
    assert_eq!(msgs[0].body_md.as_deref(), Some("create task Chat probe task in KADE"));
    assert_eq!(msgs[1].body_md.as_deref(), Some("Sure, on it."));
    assert_eq!(msgs[2].tool_name.as_deref(), Some("mcp__gizai__create_task"));
    let tool = msgs[2].tool.as_ref().unwrap();
    assert_eq!(tool["isError"], false);
    let result: Value = serde_json::from_str(tool["result"].as_str().unwrap()).unwrap();
    assert_eq!(result["link"]["page"], "task");
    assert_eq!(msgs[3].body_md.as_deref(), Some("Done: KADE-1."));
    // the run and the session
    let run = core_runs::get(&t.st.db, &s.run_id).unwrap();
    assert_eq!((run.trigger.as_str(), run.status.as_str(), run.task_id.as_deref(), run.cost_usd_micros), ("chat", "succeeded", None, 10_000));
    assert!(run.outcome.is_none(), "a chat turn has no task outcome");
    let th = chat::get_thread(&t.st.db, &thread).unwrap();
    assert!(th.session_id.is_some());
    assert_eq!(th.title, "create task Chat probe task in KADE");
    // the flags Claude Code got
    let argv: Vec<String> = serde_json::from_value(t.calls()[0]["argv"].clone()).unwrap();
    for f in ["--restricted", "--include-partial-messages", "--strict-mcp-config", "--session-id"] {
        assert!(argv.contains(&f.to_string()), "missing {f}: {argv:?}");
    }
    assert_eq!(argv[argv.len() - 2..], ["--allowedTools", "mcp__gizai"]);
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains("Team Lead") && sys.contains("GIZAI_RESULT"), "{sys}");
    // the UI heard about it as it happened
    let notes = t.notes.lock().unwrap().clone();
    assert!(notes.iter().any(|n| n == "chat:delta"), "{notes:?}");
    assert!(notes.iter().any(|n| n == "chat:message"), "{notes:?}");
    assert!(notes.iter().filter(|n| *n == "chat-changed").count() >= 2, "{notes:?}");
    assert!(notes.iter().any(|n| n == "rows:tasks"), "{notes:?}");
}

#[tokio::test]
async fn the_second_turn_resumes_the_session_and_counts_only_its_own_cost() {
    let t = setup().await;
    let (thread, _) = t.turn(None, "What is going on?").await;
    let session = chat::get_thread(&t.st.db, &thread).unwrap().session_id.unwrap();
    let (again, s) = t.turn(Some(thread.clone()), "And now?").await;
    assert_eq!(again, thread);
    assert_eq!(s.status, "succeeded", "{s:?}");
    let argv: Vec<String> = serde_json::from_value(t.calls()[1]["argv"].clone()).unwrap();
    let at = argv.iter().position(|a| a == "--resume").expect("resumed");
    assert_eq!(argv[at + 1], session);
    assert_eq!(core_runs::get(&t.st.db, &s.run_id).unwrap().cost_usd_micros, 10_000, "0.02 cumulative − 0.01 before");
    let th = chat::get_thread(&t.st.db, &thread).unwrap();
    assert_eq!(th.cost_usd_micros, 20_000);
    assert_eq!(t.roles(&thread), ["user", "agent", "tool", "agent", "user", "agent", "tool", "agent"]);
}

#[tokio::test]
async fn a_lost_session_starts_again_with_the_recent_messages() {
    let t = setup().await;
    let (thread, _) = t.turn(None, "First question").await;
    let old = chat::get_thread(&t.st.db, &thread).unwrap().session_id.unwrap();
    let (_, s) = t.turn(Some(thread.clone()), "FAKE_LOST_SESSION second question").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = t.calls();
    assert_eq!(calls.len(), 3, "first turn, the failed resume, exactly one retry");
    let retry: Vec<String> = serde_json::from_value(calls[2]["argv"].clone()).unwrap();
    assert!(retry.contains(&"--session-id".to_string()) && !retry.contains(&"--resume".to_string()));
    let prompt = calls[2]["prompt"].as_str().unwrap();
    assert!(prompt.contains("First question") && prompt.contains("Here is the overview.") && prompt.ends_with("FAKE_LOST_SESSION second question"), "{prompt}");
    let new = chat::get_thread(&t.st.db, &thread).unwrap().session_id.unwrap();
    assert_ne!(new, old);
    assert!(!t.roles(&thread).contains(&"system".to_string()), "a recovered turn shows no error");
}

#[tokio::test]
async fn chat_needs_an_active_chat_agent() {
    let t = setup().await;
    team::set_agent_status(&t.st.db, &t.st.you_id, &t.lead, "paused").unwrap();
    let e = app_chat::send(&t.st, None, "hi".into(), None).await.unwrap_err();
    assert!(e.contains("paused"), "{e}");
    let m = team::agent(&t.st.db, &t.lead).unwrap();
    team::update_agent(&t.st.db, &t.st.you_id, &t.lead, AgentInput { name: m.name, role_key: m.role_key, chat_enabled: Some(false), ..Default::default() }).unwrap();
    let e = app_chat::send(&t.st, None, "hi".into(), None).await.unwrap_err();
    assert!(e.contains("Set up the Team Lead"), "{e}");
    assert!(chat::list_threads(&t.st.db).unwrap().is_empty(), "nothing is created when chat can't start");
    let e = app_chat::send(&t.st, None, "   ".into(), None).await.unwrap_err();
    assert!(!e.is_empty());
}

#[tokio::test]
async fn a_second_message_while_working_is_refused() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG".into(), None).await.unwrap();
    for _ in 0..100 {
        if app_chat::live(&t.st).iter().any(|l| l.thread_id == thread && !l.run_id.is_empty()) { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let e = app_chat::send(&t.st, Some(thread.clone()), "another".into(), None).await.unwrap_err();
    assert!(e.contains("still answering"), "{e}");
    // another thread is fine meanwhile
    let (_, other) = t.turn(None, "separate chat").await;
    assert_eq!(other.status, "succeeded");
    app_chat::stop(&t.st, &thread);
    let s = tokio::time::timeout(std::time::Duration::from_secs(15), done).await.unwrap().unwrap();
    assert_eq!(s.status, "cancelled");
}

#[tokio::test]
async fn stopping_a_turn_cancels_it_and_says_so() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG".into(), None).await.unwrap();
    let mut draft = String::new();
    for _ in 0..100 {
        if let Some(l) = app_chat::live(&t.st).into_iter().find(|l| l.thread_id == thread) { draft = l.draft; }
        if !draft.is_empty() { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(draft, "Thinking about ", "the live draft is visible while it streams");
    app_chat::stop(&t.st, &thread);
    let s = tokio::time::timeout(std::time::Duration::from_secs(15), done).await.unwrap().unwrap();
    assert_eq!(s.status, "cancelled");
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    let last = msgs.last().unwrap();
    assert_eq!((last.role.as_str(), last.body_md.as_deref()), ("system", Some("Stopped.")));
    assert!(msgs.iter().any(|m| m.role == "agent" && m.body_md.as_deref() == Some("Thinking about")), "the half-written text is kept");
    assert!(app_chat::live(&t.st).is_empty());
}

#[tokio::test]
async fn a_crashing_claude_leaves_a_system_message_with_its_error() {
    let t = setup().await;
    let (thread, s) = t.turn(None, "FAKE_CHAT_CRASH").await;
    assert_eq!(s.status, "failed");
    let last = chat::messages(&t.st.db, &thread).unwrap().pop().unwrap();
    assert_eq!(last.role, "system");
    assert!(last.body_md.unwrap().contains("the fake was told to crash"));
    assert!(chat::get_thread(&t.st.db, &thread).unwrap().session_id.is_none(), "a session that never started isn't resumed");
}

#[tokio::test]
async fn the_turn_token_is_revoked_and_its_config_removed_afterwards() {
    let t = setup().await;
    t.turn(None, "hello").await;
    let open: i64 = t.st.db.read(|c| Ok(c.query_row("SELECT count(*) FROM api_tokens WHERE revoked_at IS NULL", [], |r| r.get(0))?)).unwrap();
    assert_eq!(open, 0);
    let leftovers: Vec<String> = std::fs::read_dir(t.st.data_dir.join("chat")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".mcp.json")).collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[tokio::test]
async fn the_agent_sees_its_instructions_and_the_linked_repos() {
    let t = setup().await;
    let repo = t._dir.path().join("kade-repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let plain = t._dir.path().join("not-a-repo");
    std::fs::create_dir_all(&plain).unwrap();
    let other = projects::create(&t.st.db, &t.st.you_id, ProjectInput { name: "Other".into(), key: "OTH".into(), repo_path: Some(plain.display().to_string()), ..Default::default() }).unwrap();
    let _ = other;
    let p = projects::list(&t.st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(&t.st.db, &t.st.you_id, &p.id, ProjectInput { name: p.name, key: p.key, repo_path: Some(repo.display().to_string()), ..Default::default() }).unwrap();
    let m = team::agent(&t.st.db, &t.lead).unwrap();
    team::update_agent(&t.st.db, &t.st.you_id, &t.lead, AgentInput { name: m.name, role_key: m.role_key, instructions_md: Some("Always answer in Dutch.".into()), ..Default::default() }).unwrap();
    t.turn(None, "hello").await;
    let argv: Vec<String> = serde_json::from_value(t.calls()[0]["argv"].clone()).unwrap();
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains("Always answer in Dutch."), "{sys}");
    let at = argv.iter().position(|a| a == "--add-dir").expect("repo dirs");
    assert_eq!(argv[at + 1], repo.display().to_string());
    assert!(!argv.contains(&plain.display().to_string()), "a folder without .git is not opened to the Team Lead");
}

#[tokio::test]
async fn stray_turn_configs_are_removed_at_start_up() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(data.join("chat")).unwrap();
    std::fs::write(data.join("chat/old.mcp.json"), "{}").unwrap();
    std::fs::write(data.join("chat/old.jsonl"), "").unwrap();
    let _st = gizai_lib::open_state(data.clone(), Arc::new(|_| {})).unwrap();
    assert!(!data.join("chat/old.mcp.json").exists());
    assert!(data.join("chat/old.jsonl").exists(), "logs stay");
}

#[tokio::test]
async fn a_claude_that_is_not_logged_in_says_so_once() {
    let t = setup().await;
    let (thread, s) = t.turn(None, "FAKE_NOT_LOGGED_IN hello").await;
    assert_eq!(s.status, "failed");
    assert!(s.error.as_deref().unwrap_or("").contains("Not logged in"), "{:?}", s.error);
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    let last = msgs.last().unwrap();
    assert_eq!(last.role, "system");
    assert!(last.body_md.as_deref().unwrap().contains("Not logged in"), "{:?}", last.body_md);
    assert!(!msgs.iter().any(|m| m.role == "agent"), "Claude Code's synthetic text is not shown as the Team Lead's answer");
}

#[tokio::test]
async fn quitting_stops_a_chat_answer_and_says_why() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG".into(), None).await.unwrap();
    for _ in 0..100 {
        if app_chat::live(&t.st).iter().any(|l| l.thread_id == thread && !l.draft.is_empty()) { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(app_chat::stop_all(&t.st, std::time::Duration::from_secs(12)).await, 1);
    let s = done.await.unwrap();
    assert_eq!(s.status, "cancelled");
    let last = chat::messages(&t.st.db, &thread).unwrap().pop().unwrap();
    assert_eq!(last.body_md.as_deref(), Some("Stopped because Gizai quit."));
    assert!(app_chat::send(&t.st, Some(thread), "more".into(), None).await.is_err(), "no new turns while quitting");
}

/// A hanging answer that has begun to write, and the process group of its Claude Code.
async fn hanging_answer(t: &T) -> (String, tokio::task::JoinHandle<app_chat::TurnSummary>, i32) {
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG".into(), None).await.unwrap();
    let mut run_id = String::new();
    for _ in 0..100 {
        if let Some(l) = app_chat::live(&t.st).into_iter().find(|l| l.thread_id == thread && !l.draft.is_empty()) { run_id = l.run_id; break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let pid = core_runs::get(&t.st.db, &run_id).unwrap().pid.expect("the answer's pid") as i32;
    (thread, done, pid)
}

/// Whether a process group is gone within a second (a process that ended counts until it has been reaped).
async fn group_gone(pgid: i32) -> bool {
    for _ in 0..50 {
        // SAFETY: signal 0 only checks that the group exists; nothing is sent.
        if unsafe { libc::kill(-pgid, 0) } != 0 { return true; }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn as_gizai_exits_kill_all_ends_a_chat_answer_at_once_and_says_why() {
    let t = setup().await;
    let (thread, done, pid) = hanging_answer(&t).await;
    assert_eq!(app_chat::kill_all(&t.st), 1);
    let s = tokio::time::timeout(std::time::Duration::from_secs(3), done).await.expect("ended at once").unwrap();
    assert_eq!((s.status.as_str(), s.error.as_deref()), ("cancelled", Some("Stopped because Gizai quit.")));
    assert_eq!(core_runs::get(&t.st.db, &s.run_id).unwrap().error.as_deref(), Some("Stopped because Gizai quit."));
    let last = chat::messages(&t.st.db, &thread).unwrap().pop().unwrap();
    assert_eq!((last.role.as_str(), last.body_md.as_deref()), ("system", Some("Stopped because Gizai quit.")));
    assert!(app_chat::live(&t.st).is_empty());
    assert!(group_gone(pid).await, "no Claude Code left");
}

#[tokio::test]
async fn an_answer_whose_claude_code_ends_while_gizai_quits_was_stopped_by_the_quit() {
    // Logging out: systemd sends SIGTERM to Claude Code as well as to Gizai.
    let t = setup().await;
    let (thread, done, pid) = hanging_answer(&t).await;
    gizai_lib::runs::mark_closing(&t.st);
    unsafe { libc::kill(-pid, libc::SIGTERM) };
    let s = tokio::time::timeout(std::time::Duration::from_secs(8), done).await.expect("answer ended").unwrap();
    assert_eq!((s.status.as_str(), s.error.as_deref()), ("cancelled", Some("Stopped because Gizai quit.")));
    let last = chat::messages(&t.st.db, &thread).unwrap().pop().unwrap();
    assert_eq!((last.role.as_str(), last.body_md.as_deref()), ("system", Some("Stopped because Gizai quit.")));
    assert_eq!(core_runs::list_for_agent(&t.st.db, &t.lead, 10).unwrap().len(), 1, "not retried in a new session while quitting");
    assert!(group_gone(pid).await);
}

#[tokio::test]
async fn a_lost_session_whose_retry_also_fails_keeps_the_old_session() {
    let t = setup().await;
    let (thread, _) = t.turn(None, "First question").await;
    let s1 = chat::get_thread(&t.st.db, &thread).unwrap().session_id.unwrap();
    let (_, s) = t.turn(Some(thread.clone()), "FAKE_CHAT_CRASH now").await; // resume fails, the fresh retry fails too
    assert_eq!(s.status, "failed");
    assert_eq!(chat::get_thread(&t.st.db, &thread).unwrap().session_id.as_deref(), Some(s1.as_str()), "the conversation's session survives");
    t.turn(Some(thread.clone()), "Third").await;
    let argv: Vec<String> = serde_json::from_value(t.calls().last().unwrap()["argv"].clone()).unwrap();
    let at = argv.iter().position(|a| a == "--resume").expect("the next turn resumes");
    assert_eq!(argv[at + 1], s1);
}

#[tokio::test]
async fn a_stopped_first_answer_is_resumed_next_time() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG plan three tasks".into(), None).await.unwrap();
    for _ in 0..100 {
        if app_chat::live(&t.st).iter().any(|l| l.thread_id == thread && !l.draft.is_empty()) { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    app_chat::stop(&t.st, &thread);
    done.await.unwrap();
    let first: Vec<String> = serde_json::from_value(t.calls()[0]["argv"].clone()).unwrap();
    let s1 = first[first.iter().position(|a| a == "--session-id").unwrap() + 1].clone();
    t.turn(Some(thread.clone()), "OK, only the first two").await;
    let argv: Vec<String> = serde_json::from_value(t.calls()[1]["argv"].clone()).unwrap();
    let at = argv.iter().position(|a| a == "--resume").expect("the stopped session is resumed");
    assert_eq!(argv[at + 1], s1);
}

#[tokio::test]
async fn a_turn_after_a_crashed_first_turn_carries_the_earlier_messages() {
    let t = setup().await;
    let (thread, s) = t.turn(None, "FAKE_CHAT_CRASH plan the Kade portal").await;
    assert_eq!(s.status, "failed");
    t.turn(Some(thread), "try again please").await;
    let prompt = t.calls().last().unwrap()["prompt"].as_str().unwrap().to_string();
    assert!(prompt.contains("plan the Kade portal") && prompt.ends_with("try again please"), "{prompt}");
}
