// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
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
async fn a_second_message_while_working_is_queued() {
    // GA-50: it used to be refused ("still answering"); now it waits in the chat's queue.
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG".into(), None).await.unwrap();
    for _ in 0..100 {
        if app_chat::live(&t.st).iter().any(|l| l.thread_id == thread && !l.run_id.is_empty()) { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let (same, queued) = app_chat::send(&t.st, Some(thread.clone()), "another".into(), None).await.unwrap();
    assert_eq!(same, thread);
    assert_eq!(queued.await.unwrap().status, "queued");
    assert_eq!(chat::queue(&t.st.db, &thread).unwrap().iter().map(|q| q.body_md.as_str()).collect::<Vec<_>>(), ["another"]);
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

/// A git repository with one tracked file and an ignored .env, in the test's folder.
fn git_repo(t: &T, name: &str) -> PathBuf {
    let repo = t._dir.path().join(name);
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(&repo).status().unwrap().success(), "git {args:?}");
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(repo.join("README.md"), "# Kade\n").unwrap();
    git(&["add", "."]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    std::fs::write(repo.join(".env"), "APP_KEY=secret\n").unwrap();
    repo
}

#[tokio::test]
async fn the_agent_sees_its_instructions_and_its_copies_of_the_code_not_the_linked_folders() {
    let t = setup().await;
    let repo = git_repo(&t, "kade-repo");
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
    let copy = t.st.data_dir.join("code/KADE");
    let dirs: Vec<&String> = argv.iter().enumerate().filter(|(_, a)| *a == "--add-dir").map(|(i, _)| &argv[i + 1]).collect();
    assert_eq!(dirs, [&copy.display().to_string()], "the Team Lead's copy, not the linked folder");
    assert!(!argv.contains(&repo.display().to_string()), "the linked folder isn't passed");
    assert!(!argv.contains(&plain.display().to_string()), "a folder without .git is not opened to the Team Lead");
    assert!(copy.join("README.md").is_file() && !copy.join(".env").exists(), "tracked files only");
}

#[tokio::test]
async fn the_system_prompt_names_each_copy_and_stays_the_same_while_the_commit_line_goes_only_into_the_turns_prompt() {
    let t = setup().await;
    let repo = git_repo(&t, "kade-repo");
    let p = projects::list(&t.st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(&t.st.db, &t.st.you_id, &p.id, ProjectInput { name: p.name, key: p.key, repo_path: Some(repo.display().to_string()), ..Default::default() }).unwrap();
    let (thread, s) = t.turn(None, "what changed in KADE?").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let (_, s) = t.turn(Some(thread.clone()), "and since then?").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = t.calls();
    let sys = |i: usize| -> String {
        let argv: Vec<String> = serde_json::from_value(calls[i]["argv"].clone()).unwrap();
        argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1].clone()
    };
    assert_eq!(sys(0), sys(1), "the same system prompt two turns in a row (the prompt cache keeps working)");
    assert!(sys(0).contains(&format!("- KADE: {}", t.st.data_dir.join("code/KADE").display())), "{}", sys(0));
    assert!(!sys(0).contains("linked repositories"), "{}", sys(0));
    assert!(sys(0).contains("ask Jeffrey once in this chat") || sys(0).contains("once in this chat, when that project comes up"), "{}", sys(0));
    assert!(sys(0).contains("Call update_checkout only after a yes in this chat") && sys(0).contains("never update a folder without that yes"), "{}", sys(0));
    let head = String::from_utf8(std::process::Command::new("git").args(["rev-parse", "--short=7", "main"]).current_dir(&repo).output().unwrap().stdout).unwrap();
    for (i, text) in [(0, "what changed in KADE?"), (1, "and since then?")] {
        let prompt = calls[i]["prompt"].as_str().unwrap();
        assert!(prompt.starts_with(&format!("[Gizai: Your copies of the code: KADE {} (", head.trim())), "{prompt}");
        assert!(prompt.ends_with(text), "{prompt}");
    }
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    assert!(msgs.iter().all(|m| !m.body_md.as_deref().unwrap_or("").contains("[Gizai:")), "the line isn't saved as a chat message");
    assert_eq!(msgs.iter().filter(|m| m.role == "user").map(|m| m.body_md.clone().unwrap()).collect::<Vec<_>>(), ["what changed in KADE?", "and since then?"]);
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

#[tokio::test]
async fn the_team_lead_gets_its_folders_in_chat_but_only_reads_them() {
    // GA-45: its folders from the agent form, read only in chat whatever they are set to; a missing one is left out.
    let t = setup().await;
    let (shared, out, gone) = (t._dir.path().join("shared"), t._dir.path().join("out"), t._dir.path().join("gone"));
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let folder = |p: &std::path::Path, access: &str| gizai_core::folders::Folder { path: p.display().to_string(), access: access.into() };
    let m = team::agent(&t.st.db, &t.lead).unwrap();
    team::update_agent(&t.st.db, &t.st.you_id, &t.lead, AgentInput { name: m.name, role_key: m.role_key, chat_enabled: Some(true),
        folders: Some(vec![folder(&shared, "read"), folder(&out, "change"), folder(&gone, "read")]), ..Default::default() }).unwrap();
    t.turn(None, "hello").await;
    let argv: Vec<String> = serde_json::from_value(t.calls()[0]["argv"].clone()).unwrap();
    let real = |p: &std::path::Path| p.canonicalize().unwrap().display().to_string();
    let at = argv.iter().position(|a| a == "--add-dir").expect("its folders");
    let dirs: Vec<&String> = argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).collect();
    assert_eq!(dirs, [&real(&shared), &real(&out)], "the missing folder is left out");
    let tools = &argv[argv.iter().position(|a| a == "--tools").unwrap() + 1];
    assert_eq!(tools, "Read,Glob,Grep", "no tool that writes, so a read and change folder is read only too");
    assert!(!argv.iter().any(|a| a == "--disallowedTools" || a.contains("Edit") || a.contains("Write(")), "{argv:?}");
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains("## Your folders") && sys.contains("you never change files in them"), "{sys}");
    assert!(sys.contains(&format!("- {} (read)\n", real(&shared))) && sys.contains(&format!("- {} (read and change)\n", real(&out))), "{sys}");
    assert!(!sys.contains(&gone.display().to_string()), "{sys}");
}

// GA-50: Runs on per chat, the hand-over when a chat moves to another account, the usage limit, and the queue.

/// A second Claude Code account in Settings (its own CLAUDE_CONFIG_DIR in the test's folder, which the fake only
/// reads), with usage left (the fake's limit doesn't apply to it). Returns its id and its folder.
fn second_account(t: &T) -> (String, String) {
    let dir = t.st.data_dir.join("acct-2").display().to_string();
    let all = gizai_core::clis::save(&t.st.db, vec![gizai_core::clis::Cli {
        name: "Claude Code 2".into(), kind: "claude_code".into(), command: FAKE.into(),
        env: vec![format!("CLAUDE_CONFIG_DIR={dir}"), "FAKE_HAS_USAGE=1".into()], ..Default::default() }]).unwrap();
    (all.into_iter().find(|c| c.name == "Claude Code 2").unwrap().id, dir)
}

fn argv_of(call: &Value) -> Vec<String> {
    serde_json::from_value(call["argv"].clone()).unwrap()
}

fn notes_of(t: &T, thread: &str) -> Vec<String> {
    chat::messages(&t.st.db, thread).unwrap().into_iter().filter(|m| m.role == "system").map(|m| m.body_md.unwrap_or_default()).collect()
}

/// Waits until the fake is waiting for fake-go (FAKE_CHAT_WAIT), so a Stop or a queued message lands during the answer.
async fn until_waiting(t: &T) {
    let f = t.st.data_dir.join("chat/fake-waiting");
    for _ in 0..200 {
        if f.exists() { return; }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the fake never started waiting");
}

fn go(t: &T) {
    std::fs::write(t.st.data_dir.join("chat/fake-go"), "").unwrap();
}

fn chat_runs(t: &T, thread: &str) -> Vec<(String, String)> {
    t.st.db.read(|c| {
        let mut st = c.prepare("SELECT status, COALESCE(adapter, '') FROM runs WHERE chat_thread_id=?1 ORDER BY created_at, rowid")?;
        Ok(st.query_map([thread], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<(String, String)>, _>>()?)
    }).unwrap()
}

#[tokio::test]
async fn a_chat_moved_to_another_account_starts_a_new_session_there_with_the_hand_over_and_moves_back_the_same_way() {
    let t = setup().await;
    let (cc2, acct2) = second_account(&t);
    let home = std::env::var("CLAUDE_CONFIG_DIR").unwrap_or_default();
    let (thread, s) = t.turn(None, "Remember this: the release is called Tulip.").await;
    assert_eq!(s.status, "succeeded");
    let s1 = chat::get_thread(&t.st.db, &thread).unwrap().session_id.unwrap();
    assert_eq!(chat::get_thread(&t.st.db, &thread).unwrap().session_cli.as_deref(), Some("claude_code"));

    // Runs on under the text box → Claude Code 2: saved on the chat, the Team Lead keeps its own.
    let lead_cli = team::agent(&t.st.db, &t.lead).unwrap().adapter;
    assert_eq!(app_chat::set_cli(&t.st, &thread, Some(&cc2)).unwrap().cli.as_deref(), Some(cc2.as_str()));
    assert_eq!(chat::get_thread(&t.st.db, &thread).unwrap().cli.as_deref(), Some(cc2.as_str()), "kept after reopening");
    assert_eq!(team::agent(&t.st.db, &t.lead).unwrap().adapter, lead_cli, "board checks and task runs keep the agent's Runs on");

    let (_, s) = t.turn(Some(thread.clone()), "What is the release called?").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = t.calls();
    assert_eq!(calls.len(), 2, "no failed resume first");
    let argv = argv_of(&calls[1]);
    assert!(argv.contains(&"--session-id".to_string()) && !argv.contains(&"--resume".to_string()), "a new session: {argv:?}");
    assert_eq!(calls[1]["account"], acct2.as_str(), "on Claude Code 2's account");
    let prompt = calls[1]["prompt"].as_str().unwrap();
    assert!(prompt.contains("This chat moved to Claude Code 2, in a new session."), "{prompt}");
    assert!(prompt.contains("User: Remember this: the release is called Tulip.") && prompt.contains("You: Here is the overview."), "{prompt}");
    assert!(prompt.contains("(You called get_overview; its result began: {"), "the tool call, with the start of its result: {prompt}");
    assert!(prompt.ends_with("(New message:)\nWhat is the release called?"), "{prompt}");
    let th = chat::get_thread(&t.st.db, &thread).unwrap();
    assert_ne!(th.session_id.as_deref(), Some(s1.as_str()));
    assert_eq!(th.session_cli.as_deref(), Some(cc2.as_str()));
    // the note sits where the switch happened, before the message that went there
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    let note = msgs.iter().position(|m| m.role == "system").unwrap();
    assert_eq!(msgs[note].body_md.as_deref(), Some("Now on Claude Code 2. The conversation so far was handed over."));
    assert_eq!(msgs[note].meta.as_ref().unwrap()["kind"], "switch");
    assert_eq!(msgs[note + 1].body_md.as_deref(), Some("What is the release called?"));
    // every turn records the CLI it ran on
    assert_eq!(chat_runs(&t, &thread), [("succeeded".to_string(), "claude_code".to_string()), ("succeeded".into(), cc2.clone())]);

    // The next message on Claude Code 2 resumes its session, without a note.
    t.turn(Some(thread.clone()), "Thanks").await;
    let calls = t.calls();
    let argv = argv_of(&calls[2]);
    assert_eq!(argv[argv.iter().position(|a| a == "--resume").expect("resumed") + 1], th.session_id.clone().unwrap());
    assert_eq!(calls[2]["account"], acct2.as_str());
    assert_eq!(notes_of(&t, &thread).len(), 1);

    // Back to the Team Lead's Runs on: a new session on the first account, with the hand-over, and a note.
    assert_eq!(app_chat::set_cli(&t.st, &thread, None).unwrap().cli, None);
    let (_, s) = t.turn(Some(thread.clone()), "And the one before?").await;
    assert_eq!(s.status, "succeeded");
    let calls = t.calls();
    assert_eq!(calls.len(), 4, "no failed resume when switching back either");
    assert!(!argv_of(&calls[3]).contains(&"--resume".to_string()));
    assert_eq!(calls[3]["account"], home.as_str());
    let prompt = calls[3]["prompt"].as_str().unwrap();
    assert!(prompt.contains("This chat moved to Claude Code, in a new session.") && prompt.contains("User: What is the release called?")
        && prompt.contains("User: Remember this: the release is called Tulip."), "{prompt}");
    assert_eq!(notes_of(&t, &thread).last().map(String::as_str), Some("Now on Claude Code. The conversation so far was handed over."));
    assert_eq!(chat_runs(&t, &thread).last().unwrap().1, "claude_code");
    assert!(!std::path::Path::new(&acct2).exists(), "nothing is written in an account's folder");
}

#[tokio::test]
async fn a_changed_runs_on_in_the_agent_form_moves_a_chat_without_its_own_pick() {
    let t = setup().await;
    let (cc2, acct2) = second_account(&t);
    let (thread, _) = t.turn(None, "First question").await;
    let m = team::agent(&t.st.db, &t.lead).unwrap();
    team::update_agent(&t.st.db, &t.st.you_id, &t.lead, AgentInput { name: m.name, role_key: m.role_key, adapter: cc2.clone(), ..Default::default() }).unwrap();
    let (_, s) = t.turn(Some(thread.clone()), "Second question").await;
    assert_eq!(s.status, "succeeded");
    let calls = t.calls();
    assert_eq!(calls.len(), 2, "no failed resume first");
    assert!(!argv_of(&calls[1]).contains(&"--resume".to_string()));
    assert_eq!(calls[1]["account"], acct2.as_str());
    assert!(calls[1]["prompt"].as_str().unwrap().contains("User: First question"));
    assert_eq!(notes_of(&t, &thread), ["Now on Claude Code 2. The conversation so far was handed over."]);
    assert_eq!(chat::get_thread(&t.st.db, &thread).unwrap().cli, None, "the chat still follows the Team Lead");
}

#[tokio::test]
async fn a_session_that_cant_be_resumed_gets_the_same_hand_over() {
    let t = setup().await;
    let (thread, _) = t.turn(None, "First question").await;
    t.turn(Some(thread.clone()), "FAKE_LOST_SESSION second question").await;
    let prompt = t.calls()[2]["prompt"].as_str().unwrap().to_string();
    assert!(prompt.contains("This chat's earlier session could not be resumed, so this is a new one.") && prompt.contains("User: First question")
        && prompt.contains("(You called get_overview; its result began:"), "{prompt}");
}

#[tokio::test]
async fn a_usage_limit_says_so_and_answer_on_another_account_sends_the_message_again_there() {
    let t = setup().await;
    let (cc2, acct2) = second_account(&t);
    let (thread, s) = t.turn(None, "FAKE_CHAT_LIMIT Will you answer?").await;
    assert_eq!(s.status, "failed");
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    let note = msgs.last().unwrap();
    assert_eq!(note.role, "system");
    assert_eq!(note.body_md.as_deref(), Some("Claude Code has hit its weekly limit, so the Team Lead couldn't answer. It resets Oct 9, 5pm (Europe/Amsterdam)."));
    let meta = note.meta.clone().unwrap();
    assert_eq!((meta["kind"].as_str(), meta["cli"].as_str(), meta["limit"].as_str()), (Some("limit"), Some("claude_code"), Some("weekly limit")));
    assert_eq!(meta["messageIds"], serde_json::json!([msgs[0].id]));

    let done = app_chat::answer_on(&t.st, &thread, &cc2, &note.id, None).await.unwrap();
    let s = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.unwrap().unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = t.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1]["account"], acct2.as_str());
    assert!(!argv_of(&calls[1]).contains(&"--resume".to_string()));
    assert!(calls[1]["prompt"].as_str().unwrap().ends_with("FAKE_CHAT_LIMIT Will you answer?"));
    assert_eq!(chat::get_thread(&t.st.db, &thread).unwrap().cli.as_deref(), Some(cc2.as_str()), "the chat runs on Claude Code 2 now");
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    assert_eq!(msgs.iter().filter(|m| m.role == "user").count(), 1, "the same message goes again, not a copy");
    assert!(notes_of(&t, &thread).last().unwrap().starts_with("Now on Claude Code 2."));
    // A failure that isn't a limit stays the usual error, without Answer on.
    let (other, s) = t.turn(None, "FAKE_CHAT_CRASH").await;
    assert_eq!(s.status, "failed");
    let last = chat::messages(&t.st.db, &other).unwrap().pop().unwrap();
    assert!(last.body_md.unwrap().starts_with("The Team Lead couldn't answer:") && last.meta.is_none());
}

#[tokio::test]
async fn two_messages_queued_during_an_answer_go_as_one_turn_after_it_and_stay_separate_messages() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_WAIT first".into(), None).await.unwrap();
    until_waiting(&t).await;
    for text in ["second", "third", "fourth"] {
        let (_, h) = app_chat::send(&t.st, Some(thread.clone()), text.into(), None).await.unwrap();
        assert_eq!(h.await.unwrap().status, "queued");
    }
    let q = chat::queue(&t.st.db, &thread).unwrap();
    assert_eq!(q.iter().map(|q| q.body_md.as_str()).collect::<Vec<_>>(), ["second", "third", "fourth"]);
    assert!(q.iter().all(|q| !q.held));
    // editable and removable until they go
    app_chat::edit_queued(&t.st, &q[1].id, "third, changed").unwrap();
    app_chat::remove_queued(&t.st, &q[2].id).unwrap();
    assert!(app_chat::set_cli(&t.st, &thread, Some("claude_code")).is_err(), "Runs on waits until the answer is done");
    assert_eq!(chat::messages(&t.st.db, &thread).unwrap().iter().filter(|m| m.role == "user").count(), 1);

    go(&t);
    let s = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.expect("both answers done").unwrap();
    assert_eq!(s.status, "succeeded");
    let calls = t.calls();
    assert_eq!(calls.len(), 2, "one turn for both");
    let first_session = argv_of(&calls[0])[argv_of(&calls[0]).iter().position(|a| a == "--session-id").unwrap() + 1].clone();
    let argv = argv_of(&calls[1]);
    assert_eq!(argv[argv.iter().position(|a| a == "--resume").expect("the same session") + 1], first_session);
    assert!(calls[1]["prompt"].as_str().unwrap().ends_with("(2 messages, written while you answered, oldest first:)\n\nsecond\n\nthird, changed"),
            "{}", calls[1]["prompt"]);
    let users: Vec<String> = chat::messages(&t.st.db, &thread).unwrap().into_iter().filter(|m| m.role == "user").map(|m| m.body_md.unwrap()).collect();
    assert_eq!(users, ["FAKE_CHAT_WAIT first", "second", "third, changed"]);
    assert_eq!(t.roles(&thread), ["user", "agent", "tool", "agent", "user", "user", "agent", "tool", "agent"]);
    assert!(chat::queue(&t.st.db, &thread).unwrap().is_empty());
    assert!(app_chat::live(&t.st).is_empty());
}

#[tokio::test]
async fn after_a_stopped_answer_the_queue_waits_until_send_now() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_WAIT plan three tasks".into(), None).await.unwrap();
    until_waiting(&t).await;
    app_chat::send(&t.st, Some(thread.clone()), "only two, please".into(), None).await.unwrap();
    app_chat::stop(&t.st, &thread);
    let s = tokio::time::timeout(std::time::Duration::from_secs(15), done).await.unwrap().unwrap();
    assert_eq!(s.status, "cancelled");
    assert_eq!(t.calls().len(), 1, "nothing went by itself");
    let q = chat::queue(&t.st.db, &thread).unwrap();
    assert!(q.len() == 1 && q[0].held, "{q:?}");
    assert_eq!(notes_of(&t, &thread), ["Stopped."]);

    let done = app_chat::send_queue(&t.st, &thread, None).await.unwrap();
    assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(30), done).await.unwrap().unwrap().status, "succeeded");
    assert!(t.calls()[1]["prompt"].as_str().unwrap().ends_with("only two, please"));
    assert!(chat::queue(&t.st.db, &thread).unwrap().is_empty());
}

#[tokio::test]
async fn after_a_failed_answer_the_queue_waits_and_can_be_removed() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_WAIT FAKE_CHAT_FAIL".into(), None).await.unwrap();
    until_waiting(&t).await;
    app_chat::send(&t.st, Some(thread.clone()), "and then?".into(), None).await.unwrap();
    go(&t);
    let s = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.unwrap().unwrap();
    assert_eq!(s.status, "failed");
    assert_eq!(t.calls().len(), 1);
    let q = chat::queue(&t.st.db, &thread).unwrap();
    assert!(q.len() == 1 && q[0].held);
    app_chat::remove_queued(&t.st, &q[0].id).unwrap();
    assert!(chat::queue(&t.st.db, &thread).unwrap().is_empty());
    assert!(app_chat::send_queue(&t.st, &thread, None).await.is_err(), "nothing left to send");
}

#[tokio::test]
async fn stop_pressed_as_the_answer_finishes_doesnt_mark_it_cancelled_and_the_queue_goes() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_WAIT FAKE_IGNORE_STOP".into(), None).await.unwrap();
    until_waiting(&t).await;
    app_chat::send(&t.st, Some(thread.clone()), "next one".into(), None).await.unwrap();
    // Stop reaches Claude Code just as it finishes its answer anyway.
    app_chat::stop(&t.st, &thread);
    go(&t);
    let s = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.unwrap().unwrap();
    assert_eq!(s.status, "succeeded");
    assert_eq!(chat_runs(&t, &thread).iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["succeeded", "succeeded"], "the finished answer, then the queued one");
    assert!(!notes_of(&t, &thread).contains(&"Stopped.".to_string()));
    assert!(t.calls()[1]["prompt"].as_str().unwrap().ends_with("next one"));
}

#[tokio::test]
async fn after_a_restart_a_queued_message_waits_and_an_answer_cut_off_by_a_crash_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let (thread, run) = {
        let st = gizai_lib::open_state(data.clone(), Arc::new(|_| {})).unwrap();
        let team_id = team::list(&st.db).unwrap()[0].id.clone();
        let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
            name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
        let thread = chat::create_thread(&st.db, &st.you_id, &lead, "First question").unwrap();
        chat::add_message(&st.db, chat::NewMessage { thread_id: thread.clone(), role: "user".into(), body_md: Some("First question".into()), ..Default::default() }).unwrap();
        chat::enqueue(&st.db, &st.you_id, &thread, "queued before the crash").unwrap();
        let run = core_runs::create_chat(&st.db, &lead, &thread, "S", "/tmp/lead", "/tmp/r.jsonl").unwrap();
        core_runs::set_running(&st.db, &run, 4242).unwrap();
        (thread, run)
        // Gizai crashes here: nothing else is recorded.
    };
    let again = gizai_lib::open_state(data, Arc::new(|_| {})).unwrap();
    let q = chat::queue(&again.db, &thread).unwrap();
    assert!(q.len() == 1 && q[0].held, "it waits for Send now: {q:?}");
    let last = chat::messages(&again.db, &thread).unwrap().pop().unwrap();
    assert_eq!((last.role.as_str(), last.body_md.as_deref()), ("system", Some(chat::INTERRUPTED)));
    assert_eq!(core_runs::get(&again.db, &run).unwrap().status, "failed");
}

#[tokio::test]
async fn the_live_snapshot_numbers_the_text_it_holds() {
    let t = setup().await;
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_CHAT_HANG".into(), None).await.unwrap();
    let mut live = None;
    for _ in 0..100 {
        live = app_chat::live(&t.st).into_iter().find(|l| l.thread_id == thread && !l.draft.is_empty());
        if live.is_some() { break; }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let live = live.expect("a draft");
    assert_eq!(live.draft, "Thinking about ");
    assert!(live.seq > 0, "the snapshot says which change it holds, so the Chat page adds only the later ones");
    app_chat::stop(&t.st, &thread);
    done.await.unwrap();
}
