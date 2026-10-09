//! GA-35 end to end with the fake `claude`: the Team Lead's board check on its heartbeat (only new findings start
//! it, one check at a time, what skips it, three failures pause it), the check run itself, the tools check_board,
//! start_chat and continue_agent_run, what a check may not do, waiting Team Lead chats in read_inbox and
//! get_overview, and your reply in a Team Lead chat (its preface).
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::model::*;
use gizai_core::{board as core_board, chat, comments, ids, projects, runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, board, chat as app_chat, mcp, runs, tools};
use serde_json::{Value, json};

const CHAT_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py");
const RUN_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
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
    // Without a new install's five agents (GA-63): the checks here count on To do having none, and one Team Lead.
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
    /// A card in To do (Auto) with no agent on the column: a waiting finding at once. Its title reaches the check's prompt.
    fn loose(&self, title: &str) -> String {
        let team = team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap();
        let todo = team.states.iter().find(|s| s.category == "ready").unwrap().id.clone();
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: self.project.clone(), title: title.into(), state_id: Some(todo), ..Default::default() }).unwrap()
    }
    fn checks(&self) -> Vec<Run> {
        core_runs::list_for_agent(&self.st.db, &self.lead, 100).unwrap().into_iter().filter(|r| r.trigger == "board_check").collect()
    }
    fn lead(&self) -> team::Member { team::agent(&self.st.db, &self.lead).unwrap() }
    fn calls(&self) -> Vec<Value> {
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-calls.jsonl")).unwrap_or_default()
            .lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
    async fn beat(&self, now: i64) -> Option<app_chat::CheckSummary> {
        let h = board::tick(&self.st, now).await?;
        Some(tokio::time::timeout(Duration::from_secs(30), h).await.expect("the check finished").unwrap())
    }
    async fn tool(&self, name: &str, args: Value) -> Result<Value, String> {
        tools::call(&self.st, &self.lead, name, args).await
    }
    fn set_lead(&self, f: impl FnOnce(&mut AgentInput)) {
        let m = self.lead();
        let mut i = AgentInput { name: m.name, role_key: m.role_key, wakeup: m.wakeup.unwrap_or_default(), budget_usd_micros: m.budget_usd_micros, ..Default::default() };
        f(&mut i);
        team::update_agent(&self.st.db, &self.st.you_id, &self.lead, i).unwrap();
    }
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(20), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---- The heartbeat ----

#[tokio::test]
async fn a_beat_with_new_findings_starts_one_check_run_and_a_beat_with_nothing_new_starts_nothing() {
    let t = setup().await;
    let t0 = ids::now_ms();
    // an empty board: nothing starts, nothing is spent
    assert!(t.beat(t0).await.is_none());
    assert!(t.checks().is_empty());
    assert_eq!(t.lead().board_checked_at, Some(t0), "it looked");
    // a card in To do with no agent on it; the interval hasn't passed yet
    let card = t.loose("Export invoices");
    assert!(t.beat(t0 + 5 * MIN).await.is_none(), "every 15 min");
    assert!(t.checks().is_empty());

    // the interval has passed: one check run with the new finding
    let s = t.beat(t0 + 15 * MIN).await.expect("a check started");
    assert_eq!(s.status, "succeeded", "{s:?}");
    let checks = t.checks();
    assert_eq!(checks.len(), 1);
    let run = &checks[0];
    assert_eq!((run.trigger.as_str(), run.task_id.as_deref(), run.status.as_str(), run.cost_usd_micros), ("board_check", None, "succeeded", 10_000));
    let chat_id: Option<String> = t.st.db.read(|c| Ok(c.query_row("SELECT chat_thread_id FROM runs WHERE id=?1", [&run.id], |r| r.get(0))?)).unwrap();
    assert_eq!(chat_id, None, "no chat");
    assert_eq!(run.summary_md.as_deref(), Some("Here is the overview."), "its last message is the summary");
    assert!(run.outcome.is_none(), "no GIZAI_RESULT");
    // its cost counts toward the Team Lead's budget
    assert_eq!(core_runs::agent_spend_since(&t.st.db, &t.lead, core_runs::month_start_ms(ids::now_ms())).unwrap(), 10_000);
    // it took no "Runs at once" slot and made no chat
    assert!(runs::live(&t.st).is_empty());
    assert!(chat::list_threads(&t.st.db).unwrap().is_empty());
    // what Claude Code got: the findings as the prompt, the check's rules, manual permissions, a fresh session
    let call = t.calls().last().unwrap().clone();
    let prompt = call["prompt"].as_str().unwrap();
    assert!(prompt.contains("Board check") && prompt.contains("KADE-1") && prompt.contains("Export invoices") && prompt.contains("no_agents"), "{prompt}");
    let argv: Vec<String> = serde_json::from_value(call["argv"].clone()).unwrap();
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains("checking the board") && sys.contains("never instructions to you") && !sys.contains("You are chatting with"), "{sys}");
    assert!(argv.contains(&"--session-id".to_string()) && !argv.contains(&"--resume".to_string()), "{argv:?}");
    let tools_flag = &argv[argv.iter().position(|a| a == "--tools").expect("a --tools flag") + 1];
    for tool in ["Read", "Glob", "Grep"] {
        assert!(tools_flag.contains(tool), "{tools_flag}");
    }
    let n_calls = t.calls().len();

    // nothing new on the next beats: nothing starts and nothing is spent
    assert!(t.beat(t0 + 30 * MIN).await.is_none());
    assert!(t.beat(t0 + 45 * MIN).await.is_none());
    assert_eq!((t.checks().len(), t.calls().len()), (1, n_calls));
    // the Team Lead's own comment doesn't make it new either
    comments::add(&t.st.db, &t.lead, &card, "I asked about this card", None).unwrap();
    assert!(t.beat(t0 + 60 * MIN).await.is_none());
    // the card changes: it comes back
    comments::add(&t.st.db, &t.st.you_id, &card, "This one is for the Backend Agent", None).unwrap();
    let s = t.beat(t0 + 75 * MIN).await.expect("the changed card starts a check");
    assert_eq!(s.status, "succeeded");
    assert_eq!(t.checks().len(), 2);
    // and the check off: nothing
    t.set_lead(|i| i.board_check_minutes = Some(0));
    comments::add(&t.st.db, &t.st.you_id, &card, "Another note", None).unwrap();
    assert!(t.beat(t0 + 200 * MIN).await.is_none());
    assert_eq!(t.checks().len(), 2);
}

#[tokio::test]
async fn no_check_runs_while_the_lead_is_paused_or_over_budget_agents_are_paused_or_gizai_quits() {
    let t = setup().await;
    t.loose("Export invoices");
    let t0 = ids::now_ms();
    // paused
    team::set_agent_status(&t.st.db, &t.st.you_id, &t.lead, "paused").unwrap();
    assert!(t.beat(t0).await.is_none());
    team::set_agent_status(&t.st.db, &t.st.you_id, &t.lead, "active").unwrap();
    // over its budget: $0.01 of $0.005 spent this month
    t.set_lead(|i| i.budget_usd_micros = Some(5_000));
    let r = core_board::create_run(&t.st.db, &t.lead, "S", "/tmp", "/tmp/c.jsonl", &[]).unwrap();
    core_board::finish_run(&t.st.db, &r, "failed", 10_000, 0, 0, Some("x"), None).unwrap();
    assert!(t.beat(t0 + 15 * MIN).await.is_none());
    t.set_lead(|i| i.budget_usd_micros = None);
    // agents paused in Settings
    settings::set(&t.st.db, "agents_paused", &true).unwrap();
    assert!(t.beat(t0 + 30 * MIN).await.is_none());
    settings::set(&t.st.db, "agents_paused", &false).unwrap();
    assert_eq!(t.checks().len(), 1, "only the one made by hand");
    assert!(t.calls().is_empty(), "Claude Code never started");
    // Gizai quits
    runs::mark_closing(&t.st);
    assert!(t.beat(t0 + 45 * MIN).await.is_none());
    assert!(t.calls().is_empty());
}

#[tokio::test]
async fn one_check_at_a_time_and_a_check_neither_blocks_chat_nor_takes_a_runs_at_once_slot() {
    let t = setup().await;
    t.loose("FAKE_CHAT_HANG while checking");
    let t0 = ids::now_ms();
    let h = board::tick(&t.st, t0).await.expect("a check started");
    until("the check runs", || t.checks().iter().any(|r| r.status == "running")).await;
    assert!(app_chat::checking(&t.st));
    // the next beat while it runs: nothing starts
    t.loose("Another card");
    assert!(board::tick(&t.st, t0 + 15 * MIN).await.is_none());
    assert_eq!(t.checks().len(), 1);
    // chat answers meanwhile
    let (thread, done) = app_chat::send(&t.st, None, "What is on the board?".into(), None).await.unwrap();
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.unwrap().unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert_eq!(chat::messages(&t.st.db, &thread).unwrap().last().unwrap().body_md.as_deref(), Some("Here is the overview."));
    // it takes no "Runs at once" slot
    assert!(runs::live(&t.st).is_empty());
    let board = t.tool("check_board", json!({})).await.unwrap();
    assert_eq!(board["runs_at_once"]["working"], 0, "{board}");
    // stopped: the next check may run
    app_chat::stop_all(&t.st, Duration::from_secs(10)).await;
    let s = tokio::time::timeout(Duration::from_secs(20), h).await.unwrap().unwrap();
    assert_ne!(s.status, "succeeded");
    assert!(!app_chat::checking(&t.st));
}

#[tokio::test]
async fn three_failed_checks_in_a_row_pause_the_check_until_the_agent_is_changed() {
    let t = setup().await;
    t.loose("FAKE_CHAT_CRASH on every check");
    let t0 = ids::now_ms();
    for i in 0..3 {
        let s = t.beat(t0 + i * 15 * MIN).await.expect("a failed check leaves its findings new, so the next beat checks again");
        assert_eq!(s.status, "failed", "{s:?}");
        assert!(s.error.as_deref().unwrap_or_default().contains("crash"), "{s:?}");
        assert_eq!(s.paused.is_some(), i == 2, "{i}: {s:?}");
    }
    let why = t.lead().board_check_paused.expect("the check is paused");
    assert!(why.contains("3 board checks in a row failed"), "{why}");
    // the Agent page (get_agent) says so
    let a = t.tool("get_agent", json!({"agent": "Team Lead"})).await.unwrap();
    assert_eq!(a["agent"]["board_check_paused"].as_str(), Some(why.as_str()), "{a}");
    // paused: no more checks
    assert!(t.beat(t0 + 45 * MIN).await.is_none());
    assert_eq!(t.checks().len(), 3);
    assert!(t.checks().iter().all(|r| r.status == "failed"));
    // changed: it checks again
    t.set_lead(|_| {});
    assert!(t.lead().board_check_paused.is_none());
    assert_eq!(t.beat(t0 + 60 * MIN).await.expect("a check").status, "failed");
}

// ---- Tools ----

#[tokio::test]
async fn check_board_gives_the_same_findings_as_the_heartbeat_with_the_agents_slots() {
    let t = setup().await;
    t.loose("Export invoices");
    let held = t.loose("Import contacts");
    tasks::update(&t.st.db, &t.st.you_id, &held, TaskPatch { hold: Some("blocked".into()), hold_reason: Some("The API key is missing".into()), ..Default::default() }).unwrap();
    let v = t.tool("check_board", json!({})).await.unwrap();
    let mine: Vec<(String, String, String)> = board::findings(&t.st, ids::now_ms()).unwrap().into_iter().map(|f| (f.task, f.kind, f.code)).collect();
    let tool: Vec<(String, String, String)> = v["findings"].as_array().unwrap().iter()
        .map(|f| (f["task"].as_str().unwrap().into(), f["kind"].as_str().unwrap().into(), f["why"].as_str().unwrap().into())).collect();
    assert_eq!(tool, mine);
    assert_eq!(tool, [("KADE-2".to_string(), "held".to_string(), "blocked".to_string()), ("KADE-1".into(), "waiting".into(), "no_agents".into())]);
    assert_eq!(v["count"], 2);
    assert_eq!(v["findings"][0]["hold_reason"], "The API key is missing");
    let lead = v["agents"].as_array().unwrap().iter().find(|a| a["name"] == "Team Lead").unwrap();
    assert_eq!((lead["cards_at_once"].as_i64(), lead["free_slots"].as_i64(), lead["working_on"].as_array().unwrap().len()), (Some(1), Some(1), 0));
    assert!(lead["pull_paused"].is_null());
    assert!(v["runs_at_once"]["max"].as_i64().unwrap() >= 1);
    // check_board only reads
    assert!(tools::catalog().iter().find(|c| c.name == "check_board").unwrap().read_only);
}

#[tokio::test]
async fn start_chat_keeps_one_waiting_chat_per_card_and_read_inbox_and_get_overview_list_them() {
    let t = setup().await;
    t.loose("Export invoices");
    t.loose("Import contacts");
    let a = t.tool("start_chat", json!({"title": "KADE-1: who builds it?", "kind": "question", "tasks": ["KADE-1"],
        "body_md": "Nothing routes KADE-1. I recommend the backend label."})).await.unwrap();
    assert_eq!((a["ok"].as_bool(), a["done"].as_str(), a["link"]["page"].as_str()), (Some(true), Some("chat started"), Some("chat")));
    let id = a["chat"]["id"].as_str().unwrap().to_string();
    assert_eq!(a["link"]["id"], id.as_str());
    // a second message about KADE-1 goes into that chat
    let b = t.tool("start_chat", json!({"title": "Also KADE-2", "kind": "approval", "tasks": ["KADE-2", "kade-1"], "body_md": "KADE-2 too."})).await.unwrap();
    assert_eq!((b["chat"]["id"].as_str(), b["done"].as_str()), (Some(id.as_str()), Some("added to the chat that already waits for this card")));
    let msgs = chat::messages(&t.st.db, &id).unwrap();
    assert_eq!(msgs.iter().map(|m| (m.role.as_str(), m.author_id.as_deref())).collect::<Vec<_>>(), [("agent", Some(t.lead.as_str())); 2]);
    // the inbox and the overview list it
    let inbox = t.tool("read_inbox", json!({})).await.unwrap();
    let chats = inbox["team_lead_chats"].as_array().unwrap();
    assert_eq!(chats.len(), 1, "{inbox}");
    assert_eq!((chats[0]["title"].as_str(), chats[0]["kind"].as_str()), (Some("KADE-1: who builds it?"), Some("question")));
    assert_eq!(chats[0]["tasks"], json!(["KADE-1", "KADE-2"]));
    assert!(chats[0]["since"].is_string());
    let overview = t.tool("get_overview", json!({})).await.unwrap();
    assert_eq!(overview["team_lead_chats"].as_array().unwrap().len(), 1, "{overview}");
    // wrong arguments say so
    assert!(t.tool("start_chat", json!({"title": "x", "kind": "fyi", "tasks": ["KADE-1"], "body_md": "x"})).await.unwrap_err().contains("question or an approval"));
    assert!(t.tool("start_chat", json!({"title": "x", "kind": "question", "tasks": [], "body_md": "x"})).await.is_err());
    assert!(t.tool("start_chat", json!({"title": "x", "kind": "question", "tasks": ["KADE-99"], "body_md": "x"})).await.is_err());
    // your answer takes it out of the inbox
    chat::add_message(&t.st.db, chat::NewMessage { thread_id: id.clone(), role: "user".into(), author_id: Some(t.st.you_id.clone()),
        body_md: Some("Yes".into()), ..Default::default() }).unwrap();
    assert!(t.tool("read_inbox", json!({})).await.unwrap()["team_lead_chats"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn in_a_check_attach_file_agent_settings_and_moves_to_review_deploy_or_done_are_refused() {
    let t = setup().await;
    t.loose("Export invoices");
    let run = core_board::create_run(&t.st.db, &t.lead, "S", "/tmp", "/tmp/c.jsonl", &[]).unwrap();
    let check = |name: &'static str, args: Value| { let (st, lead, run) = (t.st.clone(), t.lead.clone(), run.clone());
        async move { tools::call_check(&st, &lead, &run, name, args).await } };
    let e = check("attach_file", json!({"path": "/etc/hostname", "task": "KADE-1"})).await.unwrap_err();
    assert!(e.contains("nobody named a file"), "{e}");
    for (name, args) in [("update_agent", json!({"agent": "Team Lead", "board_check_minutes": 5})), ("set_agent_status", json!({"agent": "Team Lead", "status": "paused"})),
                         ("create_agent", json!({"name": "X", "role": "backend"}))] {
        let e = check(name, args).await.unwrap_err();
        assert!(e.contains("can't be used in a board check"), "{name}: {e}");
    }
    for column in ["Review", "Done"] {
        let e = check("move_task", json!({"task": "KADE-1", "column": column})).await.unwrap_err();
        assert!(e.contains("never moves a card"), "{column}: {e}");
    }
    assert_eq!(tasks::get(&t.st.db, &tasks::list(&t.st.db, &TaskFilter::default()).unwrap()[0].id).unwrap().state_name, "To do");
    // reading and other changes work
    assert!(check("check_board", json!({})).await.is_ok());
    assert!(check("move_task", json!({"task": "KADE-1", "column": "Backlog"})).await.is_ok());
    // a start_chat in a check links its message to the check's run
    let c = check("start_chat", json!({"title": "KADE-1?", "kind": "question", "tasks": ["KADE-1"], "body_md": "Who builds it?"})).await.unwrap();
    let msgs = chat::messages(&t.st.db, c["chat"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(msgs[0].run_id.as_deref(), Some(run.as_str()));
    // outside a check update_agent sets the board check, and get_agent and list_agents show it
    t.tool("update_agent", json!({"agent": "Team Lead", "board_check_minutes": 30})).await.unwrap();
    assert_eq!(t.tool("get_agent", json!({"agent": "Team Lead"})).await.unwrap()["agent"]["board_check_minutes"], 30);
    let list = t.tool("list_agents", json!({})).await.unwrap();
    let lead = list["agents"].as_array().unwrap().iter().find(|a| a["name"] == "Team Lead").unwrap().clone();
    assert_eq!(lead["board_check_minutes"], 30, "{list}");
}

// ---- Your reply in a Team Lead chat ----

#[tokio::test]
async fn your_reply_answers_a_team_lead_chat_and_its_prompt_says_the_lead_started_it_with_the_cards_now() {
    let t = setup().await;
    let card = t.loose("Export invoices");
    tasks::update(&t.st.db, &t.st.you_id, &card, TaskPatch { hold: Some("needs_decision".into()), hold_reason: Some("CSV or JSON?".into()), ..Default::default() }).unwrap();
    let c = t.tool("start_chat", json!({"title": "KADE-1: export format", "kind": "approval", "tasks": ["KADE-1"],
        "body_md": "The Backend Agent asks CSV or JSON. I recommend JSON."})).await.unwrap();
    let id = c["chat"]["id"].as_str().unwrap().to_string();
    assert!(chat::get_thread(&t.st.db, &id).unwrap().waiting);
    let (same, done) = app_chat::send(&t.st, Some(id.clone()), "Go with JSON".into(), None).await.unwrap();
    assert_eq!(same, id);
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.unwrap().unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    // it left the Inbox as soon as you wrote
    let th = chat::get_thread(&t.st.db, &id).unwrap();
    assert!(!th.waiting && th.answered_at.is_some());
    assert_eq!(th.kind.as_deref(), Some("approval"), "still a Team Lead chat (the Team Lead label in Recent)");
    // the prompt: the Team Lead started it during a board check, the cards as they are now, the chat so far, then your message
    let prompt = t.calls().last().unwrap()["prompt"].as_str().unwrap().to_string();
    assert!(prompt.contains("You started this chat during a board check"), "{prompt}");
    assert!(!prompt.contains("could not be resumed"), "{prompt}");
    assert!(prompt.contains("KADE-1") && prompt.contains("To do") && prompt.contains("needs_decision") && prompt.contains("CSV or JSON?"), "{prompt}");
    assert!(prompt.contains("write the decision on the card as a comment"), "{prompt}");
    assert!(prompt.contains("I recommend JSON") && prompt.ends_with("Go with JSON"), "{prompt}");
}

// ---- continue_agent_run ----

/// A fake `claude` whose runs end with needs_decision, writing each prompt it gets to prompts.txt.
fn asking_fake(dir: &Path) -> String {
    let d = dir.join("fake-asks");
    std::fs::create_dir_all(d.join("fixtures")).unwrap();
    let src = Path::new(RUN_FAKE).parent().unwrap().join("fixtures");
    let run = std::fs::read_to_string(src.join("run-ok.jsonl")).unwrap().replace("ready_for_testing", "needs_decision");
    std::fs::write(d.join("fixtures/run-ok.jsonl"), run).unwrap();
    std::fs::copy(src.join("models-init.jsonl"), d.join("fixtures/models-init.jsonl")).unwrap();
    // copied and written by a child, so this process never holds a script open for writing (ETXTBSY)
    assert!(std::process::Command::new("cp").arg(RUN_FAKE).arg(d.join("fake-claude.sh")).status().unwrap().success());
    let wrapper = d.join("asks.sh");
    let script = format!("#!/usr/bin/env bash\np=\"$(cat)\"\nprintf '%s\\n----\\n' \"$p\" >> {prompts}\nprintf '%s' \"$p\" | exec {fake} \"$@\"\n",
                         prompts = d.join("prompts.txt").display(), fake = d.join("fake-claude.sh").display());
    assert!(std::process::Command::new("sh").args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod +x \"$2\"", "sh", &script])
        .arg(&wrapper).status().unwrap().success());
    wrapper.to_string_lossy().into_owned()
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

#[tokio::test]
async fn continue_agent_run_resumes_a_run_that_asked_a_decision_once_it_is_answered_on_the_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let fake = asking_fake(tmp.path());
    settings::set(&st.db, "claude_bin", &fake).unwrap();
    let repo = git_repo(tmp.path());
    let card = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    let call = |name: &'static str, args: Value| { let (st, lead) = (st.clone(), lead.clone()); async move { tools::call(&st, &lead, name, args).await } };

    // no run yet
    assert!(call("continue_agent_run", json!({"task": "KADE-1"})).await.unwrap_err().contains("no run to continue"));
    let (first, done) = runs::start(&st, &card, None, None, "manual").await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), done).await.unwrap().unwrap();
    let t = tasks::get(&st.db, &card).unwrap();
    assert_eq!(t.hold.as_deref(), Some("needs_decision"), "the run asked a decision");
    // not answered yet: refused
    let e = call("continue_agent_run", json!({"task": "KADE-1"})).await.unwrap_err();
    assert!(e.contains("nobody has answered"), "{e}");
    // you answer on the card; the Team Lead continues the run
    comments::add(&st.db, &st.you_id, &card, "Use JSON, please", None).unwrap();
    let r = call("continue_agent_run", json!({"task": "KADE-1"})).await.unwrap();
    assert_eq!(r["done"], "continued", "{r}");
    let second = r["run"]["id"].as_str().unwrap().to_string();
    assert_ne!(second, first);
    until("the continued run ends", || runs::live(&st).is_empty() && core_runs::get(&st.db, &second).unwrap().ended_at.is_some()).await;
    let prompts = std::fs::read_to_string(tmp.path().join("fake-asks/prompts.txt")).unwrap();
    let last = prompts.trim_end().trim_end_matches("----").rsplit("\n----\n").next().unwrap().to_string();
    assert!(last.contains("ended asking for a decision") && last.contains("Use JSON, please"), "{last}");
    let resumed = core_runs::get(&st.db, &second).unwrap();
    let first_run = core_runs::get(&st.db, &first).unwrap();
    assert_eq!(resumed.session_id, first_run.session_id, "the same session, resumed");
}

#[tokio::test]
async fn in_a_check_the_team_lead_never_starts_more_runs_than_the_agents_free_slots_allow() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &RUN_FAKE.to_string()).unwrap();
    let repo = git_repo(tmp.path());
    let busy = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    tasks::update(&st.db, &st.you_id, &busy, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    // the Backend Agent (one card at a time) works on KADE-1
    let (_, done) = runs::start(&st, &busy, None, None, "manual").await.unwrap();
    until("KADE-1 runs", || runs::live(&st).len() == 1).await;
    let check = core_board::create_run(&st.db, &lead, "S", "/tmp", "/tmp/c.jsonl", &[]).unwrap();
    let e = tools::call_check(&st, &lead, &check, "start_agent_run", json!({"task": "KADE-2", "agent": "Backend Agent"})).await.unwrap_err();
    assert!(e.contains("no free slot"), "{e}");
    // without an agent the busy one isn't picked at all (the start is refused before the slot check)
    assert!(tools::call_check(&st, &lead, &check, "start_agent_run", json!({"task": "KADE-2"})).await.is_err());
    assert_eq!(runs::live(&st).len(), 1, "nothing more started");
    assert!(core_runs::list_for_task(&st.db, &tasks::list(&st.db, &TaskFilter::default()).unwrap().iter().find(|t| t.identifier == "KADE-2").unwrap().id)
        .unwrap().is_empty());
    runs::stop_all(&st, Duration::from_secs(12)).await;
    let _ = tokio::time::timeout(Duration::from_secs(15), done).await;
}

#[tokio::test]
async fn a_board_check_reads_the_leads_copies_of_the_code_and_its_own_folders_never_the_linked_folder() {
    // GA-47: main's board check still passed the linked folders (repo_dirs, which GA-44 removed); after the merge it
    // gets what a chat turn gets: the Team Lead's copies of the code, then its own folders (GA-45), a missing one left out.
    let t = setup().await;
    let repo = git_repo(t._tmp.path());
    projects::update(&t.st.db, &t.st.you_id, &t.project, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), status: Some("active".into()),
        repo_path: Some(repo.display().to_string()), ..Default::default() }).unwrap();
    gizai_lib::code::before_turn(&t.st, "warm-up").await;
    let copy = t.st.data_dir.join("code/KADE");
    assert!(copy.join(".git").exists(), "the Team Lead's copy is there");
    let (shared, out, gone) = (t._tmp.path().join("shared"), t._tmp.path().join("out"), t._tmp.path().join("gone"));
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let folder = |p: &Path, access: &str| gizai_core::folders::Folder { path: p.display().to_string(), access: access.into() };
    t.set_lead(|i| i.folders = Some(vec![folder(&shared, "read"), folder(&out, "change"), folder(&gone, "read")]));
    assert_eq!(t.lead().board_check_minutes, Some(15), "saving its folders keeps its board check");

    let t0 = ids::now_ms();
    assert!(t.beat(t0).await.is_none());
    t.loose("Export invoices");
    let s = t.beat(t0 + 15 * MIN).await.expect("a check started");
    assert_eq!(s.status, "succeeded", "{s:?}");
    let argv: Vec<String> = serde_json::from_value(t.calls().last().unwrap()["argv"].clone()).unwrap();
    assert_eq!(argv.iter().filter(|a| *a == "--add-dir").count(), 1, "{argv:?}");
    let at = argv.iter().position(|a| a == "--add-dir").unwrap();
    let dirs: Vec<&String> = argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).collect();
    let real = |p: &Path| p.canonicalize().unwrap().display().to_string();
    assert_eq!(dirs, [&copy.display().to_string(), &real(&shared), &real(&out)], "its copy first, then its folders that are there");
    assert!(!argv.contains(&repo.display().to_string()) && !argv.contains(&real(&repo)), "never the linked folder: {argv:?}");
    let tools_flag = &argv[argv.iter().position(|a| a == "--tools").unwrap() + 1];
    assert_eq!(tools_flag, "Read,Glob,Grep", "no tool that writes, so a read and change folder is only read");
}
