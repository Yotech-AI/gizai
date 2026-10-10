// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-41, end to end with the fake Claude Code: files added to a chat message (the + button or a drop) are stored with
// the message, copied into the Team Lead's own folder (its working folder in chat, which its file tools read) and named
// in the turn's prompt with that path; the Team Lead can read them and attach one to a task when asked. In a new chat
// and an existing one, and for a message queued while it answers. A file that can't be added stops the send.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gizai_core::model::*;
use gizai_core::{chat, files, projects, tasks, team};
use gizai_lib::runs::Note;
use gizai_lib::{AppState, chat as app_chat, mcp, tools};
use serde_json::{Value, json};

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
    task: String,
    dir: tempfile::TempDir,
    _server: tokio::task::JoinHandle<()>,
}

async fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::open_state(dir.path().join("data"), Arc::new(|_: Note| {})).unwrap();
    st.mcp_socket = st.data_dir.join("mcp.sock");
    st.mcp_shim = Some(shim());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let p = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let task = tasks::create(&st.db, &st.you_id, TaskInput { project_id: p, title: "Spec review".into(), ..Default::default() }).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let server = mcp::start(&st).unwrap();
    T { st, lead, task, dir, _server: server }
}

impl T {
    /// A file on this computer, outside Gizai's folder (like ~/Downloads).
    fn file(&self, name: &str, body: &str) -> String {
        let d = self.dir.path().join("downloads");
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join(name);
        std::fs::write(&p, body).unwrap();
        p.display().to_string()
    }
    async fn send(&self, thread: Option<String>, text: &str, files: Vec<String>) -> (String, app_chat::TurnSummary) {
        let (id, done) = app_chat::send_with_files(&self.st, thread, text.into(), None, files, None).await.unwrap();
        let s = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        (id, s)
    }
    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-calls.jsonl")).unwrap_or_default()
            .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
    fn last_answer(&self, thread: &str) -> String {
        chat::messages(&self.st.db, thread).unwrap().into_iter().filter(|m| m.role == "agent").last().and_then(|m| m.body_md).unwrap_or_default()
    }
}

fn names(fs: &[FileRow]) -> Vec<&str> {
    fs.iter().map(|f| f.name.as_str()).collect()
}

#[tokio::test]
async fn a_new_chat_carries_the_files_and_the_team_lead_reads_its_copies_in_its_own_folder() {
    let t = setup().await;
    let (spec, notes) = (t.file("spec.md", "the spec"), t.file("notes 1.txt", "some notes"));
    let (thread, s) = t.send(None, "FAKE_READ_FILES What do these say?", vec![spec.clone(), notes.clone()]).await;
    assert_eq!(s.status, "succeeded", "{s:?}");

    // saved with the message, also when the chat is read again
    let msgs = chat::messages(&t.st.db, &thread).unwrap();
    assert_eq!(msgs[0].body_md.as_deref(), Some("FAKE_READ_FILES What do these say?"));
    assert_eq!(names(&msgs[0].files), ["spec.md", "notes 1.txt"]);

    // a copy of each in the Team Lead's folder, which is its working folder in chat
    let lead_dir = t.st.data_dir.join("lead");
    let copies: Vec<PathBuf> = msgs[0].files.iter().map(|f| app_chat::lead_file_path(&t.st, f)).collect();
    for (c, body) in copies.iter().zip(["the spec", "some notes"]) {
        assert!(c.starts_with(&lead_dir), "{}", c.display());
        assert_eq!(std::fs::read_to_string(c).unwrap(), body);
    }
    let call = &t.calls()[0];
    assert_eq!(Path::new(call["cwd"].as_str().unwrap()).canonicalize().unwrap(), lead_dir.canonicalize().unwrap());
    let argv: Vec<String> = serde_json::from_value(call["argv"].clone()).unwrap();
    assert!(argv.contains(&"--restricted".to_string()), "{argv:?}");

    // the prompt names each file and where to read it; not the path it was added from
    let prompt = call["prompt"].as_str().unwrap();
    assert!(prompt.contains("FAKE_READ_FILES What do these say?\n\n(Files added to this message, copied into your folder so you can read them; attach_file takes these paths:)"),
            "{prompt}");
    assert!(prompt.contains(&format!("\n- spec.md: {}", copies[0].display())), "{prompt}");
    assert!(prompt.contains(&format!("\n- notes 1.txt: {}", copies[1].display())), "{prompt}");
    assert!(!prompt.contains(&spec), "{prompt}");
    // and it read them there
    assert_eq!(t.last_answer(&thread), "Read: the spec | some notes");
    // the system prompt says what gizai: links and added files are
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains("[GA-12 - Fix the login](gizai:task/GA-12)") && sys.contains("attach_file takes those paths"), "{sys}");
}

#[tokio::test]
async fn a_message_of_files_only_starts_a_chat_named_after_them_and_an_existing_chat_takes_files_too() {
    let t = setup().await;
    let (thread, s) = t.send(None, "  ", vec![t.file("a.txt", "A"), t.file("b.txt", "B")]).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert_eq!(chat::get_thread(&t.st.db, &thread).unwrap().title, "a.txt, b.txt");
    let prompt = t.calls()[0]["prompt"].as_str().unwrap().to_string();
    assert!(prompt.contains("(Files added to this message") && prompt.contains("\n- a.txt: ") && prompt.contains("\n- b.txt: "), "{prompt}");

    // the next message in the same chat, with a file of its own: only that one is named in its turn
    let (again, s) = t.send(Some(thread.clone()), "FAKE_READ_FILES and this one", vec![t.file("c.txt", "C")]).await;
    assert_eq!((again.as_str(), s.status.as_str()), (thread.as_str(), "succeeded"));
    let prompt = t.calls()[1]["prompt"].as_str().unwrap().to_string();
    assert!(prompt.contains("\n- c.txt: ") && !prompt.contains("\n- a.txt: "), "{prompt}");
    assert_eq!(t.last_answer(&thread), "Read: C");
    let users: Vec<Vec<String>> = chat::messages(&t.st.db, &thread).unwrap().into_iter().filter(|m| m.role == "user")
        .map(|m| m.files.into_iter().map(|f| f.name).collect()).collect();
    assert_eq!(users, [vec!["a.txt", "b.txt"], vec!["c.txt"]]);
    // no text and no files is still refused
    assert!(app_chat::send_with_files(&t.st, Some(thread), " ".into(), None, vec![], None).await.is_err());
}

#[tokio::test]
async fn a_folder_or_a_missing_file_stops_the_send_and_says_which() {
    let t = setup().await;
    let folder = t.dir.path().join("photos");
    std::fs::create_dir_all(&folder).unwrap();
    let e = app_chat::send_with_files(&t.st, None, "see".into(), None, vec![t.file("ok.txt", "ok"), folder.display().to_string()], None).await
        .err().expect("refused");
    assert!(e.contains("photos is a folder, not a file"), "{e}");
    assert!(chat::list_threads(&t.st.db).unwrap().is_empty(), "no chat was started");
    let e = app_chat::send_with_files(&t.st, None, "see".into(), None, vec![t.dir.path().join("gone.pdf").display().to_string()], None).await
        .err().expect("refused");
    assert!(e.contains("gone.pdf"), "{e}");
    assert!(t.calls().is_empty(), "Claude Code never started");
}

#[tokio::test]
async fn a_message_queued_during_an_answer_keeps_its_files_and_the_next_turn_names_them() {
    let t = setup().await;
    let (thread, done) = app_chat::send_with_files(&t.st, None, "FAKE_CHAT_WAIT first".into(), None, vec![], None).await.unwrap();
    let waiting = t.st.data_dir.join("chat/fake-waiting");
    for _ in 0..200 {
        if waiting.exists() { break; }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(waiting.exists(), "the fake never started waiting");
    let (_, h) = app_chat::send_with_files(&t.st, Some(thread.clone()), "FAKE_READ_FILES meanwhile".into(), None, vec![t.file("late.txt", "late")], None)
        .await.unwrap();
    assert_eq!(h.await.unwrap().status, "queued");
    let q = chat::queue(&t.st.db, &thread).unwrap();
    assert_eq!((q[0].body_md.as_str(), names(&q[0].files)), ("FAKE_READ_FILES meanwhile", vec!["late.txt"]));

    std::fs::write(t.st.data_dir.join("chat/fake-go"), "").unwrap();
    let s = tokio::time::timeout(std::time::Duration::from_secs(30), done).await.expect("both answers done").unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = t.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls[0]["prompt"].as_str().unwrap().contains("Files added"), "{}", calls[0]["prompt"]);
    assert!(calls[1]["prompt"].as_str().unwrap().contains("\n- late.txt: "), "{}", calls[1]["prompt"]);
    assert_eq!(t.last_answer(&thread), "Read: late");
    let sent = chat::messages(&t.st.db, &thread).unwrap().into_iter().filter(|m| m.role == "user").collect::<Vec<_>>();
    assert_eq!(names(&sent[1].files), ["late.txt"], "the sent message shows its files");
    assert!(chat::queue(&t.st.db, &thread).unwrap().is_empty());
}

#[tokio::test]
async fn attach_this_to_a_task_works_for_an_added_file_but_only_in_its_own_chat() {
    let t = setup().await;
    let ident = tasks::get(&t.st.db, &t.task).unwrap().identifier;
    let (thread, s) = t.send(None, &format!("FAKE_ATTACH_TO {ident} Attach this to {ident}"), vec![t.file("contract.pdf", "%PDF")]).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let tool = chat::messages(&t.st.db, &thread).unwrap().into_iter().find(|m| m.role == "tool").expect("a tool call");
    assert_eq!(tool.tool_name.as_deref(), Some("mcp__gizai__attach_file"));
    let call = tool.tool.unwrap();
    assert_eq!(call["isError"], false, "{call}");
    let on_task = files::list(&t.st.db, "task", &t.task).unwrap();
    assert_eq!(names(&on_task), ["contract.pdf"]);

    // the Team Lead's copy isn't named in another chat, nor in a task run: there it is refused
    let copy = app_chat::lead_file_path(&t.st, &chat::messages(&t.st.db, &thread).unwrap()[0].files[0]).display().to_string();
    let other = chat::create_thread(&t.st.db, &t.st.you_id, &t.lead, "Another chat").unwrap();
    let e = tools::call_in(&t.st, &t.lead, Some(&other), "attach_file", json!({"path": copy, "task": ident})).await.unwrap_err();
    assert!(e.contains("one you added to a message here"), "{e}");
    assert!(tools::call_in(&t.st, &t.lead, None, "attach_file", json!({"path": copy, "task": ident})).await.is_err());
    // and in its own chat it works again
    tools::call_in(&t.st, &t.lead, Some(&thread), "attach_file", json!({"path": copy, "task": ident})).await.unwrap();
    assert_eq!(files::list(&t.st.db, "task", &t.task).unwrap().len(), 2);
}

#[tokio::test]
async fn a_link_from_the_picker_reaches_the_team_lead_in_the_message_and_its_target_looks_the_item_up() {
    let t = setup().await;
    let ident = tasks::get(&t.st.db, &t.task).unwrap().identifier;
    let client = gizai_core::clients::create(&t.st.db, &t.st.you_id, ClientInput { name: "Spoorwegmuseum".into(), ..Default::default() }).unwrap();
    let text = format!("What about [{ident} - Spec review](gizai:task/{ident}), [Kade portal](gizai:project/KADE) and [Spoorwegmuseum](gizai:client/{client})?");
    let (thread, s) = t.send(None, &text, vec![]).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert_eq!(chat::messages(&t.st.db, &thread).unwrap()[0].body_md.as_deref(), Some(text.as_str()));
    let prompt = t.calls()[0]["prompt"].as_str().unwrap().to_string();
    assert!(prompt.ends_with(&text), "{prompt}");
    // what comes after the slash is what the gizai tools take
    let task = tools::call_in(&t.st, &t.lead, Some(&thread), "get_task", json!({"task": ident})).await.unwrap();
    assert_eq!(task["task"]["title"], "Spec review", "{task}");
    let project = tools::call_in(&t.st, &t.lead, Some(&thread), "get_project", json!({"project": "KADE"})).await.unwrap();
    assert_eq!(project["project"]["name"], "Kade portal", "{project}");
    let found = tools::call_in(&t.st, &t.lead, Some(&thread), "get_client", json!({"client": client})).await.unwrap();
    assert_eq!(found["client"]["name"], "Spoorwegmuseum", "{found}");
}
