//! GA-49 QA: the column decides who works a card. Each column has agents, Auto or Manual, and a next column; labels
//! never route. End to end with the fake `claude` (and a fake gh for the merge), never the real ones.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_core::columns::{self, ColumnInput};
use gizai_core::model::*;
use gizai_core::{runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, runs, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fixtures");
const LINK: &str = "https://github.com/acme/shop";
const DEFAULT_COLUMNS: [&str; 7] = ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done"];

// ---- helpers ----

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"]);
    repo
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake `claude` like the usual one whose run answers what a marker in its prompt says (the agent's instructions):
/// FAKE_ANSWER=qa_pass or FAKE_ANSWER=deployed; else ready_for_testing.
fn answering_fake(dir: &Path) -> String {
    let path = dir.join("answering-claude.sh");
    write_script(&path, &format!(r#"#!/usr/bin/env bash
fx='{FIXTURES}'
case " $* " in *" --input-format stream-json "*)
  IFS= read -r _r; cat "$fx/models-init.jsonl"; cat > /dev/null; exit 0 ;;
esac
prompt="$(cat)"
out=ready_for_testing
case "$prompt" in
  *FAKE_ANSWER=qa_pass*) out=qa_pass ;;
  *FAKE_ANSWER=deployed*) out=deployed ;;
esac
sed "s/ready_for_testing/$out/g" "$fx/run-ok.jsonl"
exit 0
"#));
    path.to_string_lossy().into_owned()
}

struct App {
    st: AppState,
    project: String,
    tmp: tempfile::TempDir,
}

/// Project KADE ("Kade portal") with a local git repository, the usual fake `claude`. No agents yet.
fn app() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let repo = git_repo(tmp.path());
    let project = gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(),
        repo_path: Some(repo.to_string_lossy().into_owned()), ..Default::default() }).unwrap();
    App { st, project, tmp }
}

impl App {
    fn team(&self) -> team::Team {
        team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap()
    }
    fn state(&self, name: &str) -> String {
        self.team().states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
    }
    fn add_agent(&self, input: AgentInput) -> String {
        team::add_agent(&self.st.db, &self.st.you_id, &self.team().id, input).unwrap()
    }
    fn agent(&self, name: &str, role: &str, max_runs: i64) -> String {
        self.add_agent(AgentInput { name: name.into(), role_key: role.into(), max_runs: Some(max_runs), ..Default::default() })
    }
    fn lead(&self) -> String {
        self.add_agent(AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() })
    }
    fn card(&self, title: &str, column: &str, description: &str, assignee: Option<&str>) -> String {
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: self.project.clone(), title: title.into(),
            description_md: description.into(), state_id: Some(self.state(column)), assignee_id: assignee.map(String::from), ..Default::default() }).unwrap()
    }
    /// What the app does on a drag (commands::move_task): move the card, then wake the queue (`runs::dispatch`).
    async fn drag(&self, task: &str, column: &str) -> Option<String> {
        tasks::move_to(&self.st.db, &self.st.you_id, task, &self.state(column), "").unwrap();
        runs::dispatch(&self.st, task).await
    }
    /// What the app does on an assignment (commands::update_task).
    async fn assign(&self, task: &str, agent: &str) -> Option<String> {
        tasks::update(&self.st.db, &self.st.you_id, task, TaskPatch { assignee_id: Some(agent.into()), ..Default::default() }).unwrap();
        runs::dispatch(&self.st, task).await
    }
    fn set_col(&self, name: &str, input: ColumnInput) {
        columns::set_column(&self.st.db, &self.st.you_id, &self.state(name), input).unwrap();
    }
    fn task(&self, t: &str) -> Task { tasks::get(&self.st.db, t).unwrap() }
    fn column(&self, t: &str) -> String { self.task(t).state_name }
    fn ident(&self, t: &str) -> String { self.task(t).identifier }
    fn runs_of(&self, t: &str) -> Vec<Run> { core_runs::list_for_task(&self.st.db, t).unwrap() }
    fn runs_by(&self, agent: &str) -> Vec<Run> { core_runs::list_for_agent(&self.st.db, agent, 100).unwrap() }
    fn live_run(&self, t: &str) -> Option<runs::LiveRun> { runs::live(&self.st).into_iter().find(|l| l.task_id == t) }
    async fn tool(&self, lead: &str, name: &str, args: Value) -> Result<Value, String> {
        tools::call(&self.st, lead, name, args).await
    }
    async fn stop_everything(&self) {
        runs::stop_all(&self.st, Duration::from_secs(12)).await;
    }
}

async fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn names(v: &Value) -> Vec<String> {
    v["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect()
}

fn col<'a>(v: &'a Value, name: &str) -> &'a Value {
    v["columns"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap_or_else(|| panic!("no column {name} in {v}"))
}

fn agents_on(v: &Value, name: &str) -> Vec<String> {
    col(v, name)["agents"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()
}

// ---- A. Auto start ----

#[tokio::test]
async fn a1_a_card_dragged_into_an_auto_column_starts_with_an_idle_agent_and_waits_while_its_agents_are_at_their_cards_at_once() {
    let a = app();
    let be = a.agent("Backend Agent", "backend", 1); // lands on To do and In progress
    let c1 = a.card("First", "Backlog", "FAKE_HANG", None);
    let c2 = a.card("Second", "Backlog", "FAKE_HANG", None);
    assert!(runs::pull(&a.st).await.is_empty(), "nothing starts in Backlog");

    // dragged into To do (Auto): the idle Backend Agent starts it
    assert_eq!(a.drag(&c1, "To do").await, Some(a.ident(&c1)));
    let r1 = a.live_run(&c1).expect("c1 runs");
    assert_eq!(r1.agent_id, be);
    assert_eq!(a.column(&c1), "In progress", "a start in To do moves the card to its next column");

    // its only agent is at its cards at once (1): the second card waits
    assert_eq!(a.drag(&c2, "To do").await, None);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c2).is_empty(), "c2 waits");

    // a slot frees up: the second card starts
    runs::stop(&a.st, &r1.run_id);
    until("c2 starts once c1's run ended", 20, || a.live_run(&c2).is_some()).await;
    assert_eq!(a.live_run(&c2).unwrap().agent_id, be);
    a.stop_everything().await;
}

#[tokio::test]
async fn a2_with_runs_at_once_full_a_dragged_card_waits_and_starts_when_a_run_ends() {
    let a = app();
    settings::set(&a.st.db, "max_concurrent_runs", &1u32).unwrap();
    let be = a.agent("Backend Agent", "backend", 2);
    let c1 = a.card("First", "Backlog", "FAKE_HANG", None);
    let c2 = a.card("Second", "Backlog", "FAKE_HANG", None);
    assert_eq!(a.drag(&c1, "In progress").await, Some(a.ident(&c1)));
    let r1 = a.live_run(&c1).expect("c1 runs");
    assert_eq!(a.drag(&c2, "In progress").await, None, "Runs at once (1) is full");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c2).is_empty());
    runs::stop(&a.st, &r1.run_id);
    until("c2 starts once a run slot is free", 20, || a.live_run(&c2).is_some()).await;
    assert_eq!(a.live_run(&c2).unwrap().agent_id, be);
    a.stop_everything().await;
}

// ---- B. An assigned card ----

#[tokio::test]
async fn b_a_card_assigned_to_an_agent_in_an_auto_column_is_started_only_by_that_agent_even_off_the_column() {
    let a = app();
    let be = a.agent("Backend Agent", "backend", 1); // on To do and In progress
    let fe = a.agent("Frontend Agent", "frontend", 1);
    // the Frontend Agent is on no column at all
    for c in ["To do", "In progress"] {
        columns::remove_agent(&a.st.db, &a.st.you_id, &a.state(c), &fe).unwrap();
    }
    assert!(columns::of_agent(&a.st.db, &fe).unwrap().is_empty());
    team::set_agent_status(&a.st.db, &a.st.you_id, &fe, "paused").unwrap();

    let c = a.card("Assigned", "Backlog", "FAKE_HANG", None);
    a.assign(&c, &fe).await;
    assert_eq!(a.drag(&c, "To do").await, None, "its agent is paused, and the idle Backend Agent on To do doesn't take it");
    assert!(runs::pull(&a.st).await.is_empty());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c).is_empty());
    assert!(a.runs_by(&be).is_empty(), "the Backend Agent never started it");

    // its agent is active again: it starts the card, although it isn't on To do
    team::set_agent_status(&a.st.db, &a.st.you_id, &fe, "active").unwrap();
    runs::resume_pull(&a.st, &fe);
    let started = runs::pull(&a.st).await;
    assert_eq!(started.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>(), vec![c.clone()]);
    assert_eq!(a.live_run(&c).unwrap().agent_id, fe);
    assert!(a.runs_by(&be).is_empty());
    a.stop_everything().await;
}

// ---- C. Manual column ----

#[tokio::test]
async fn c1_in_a_manual_column_nothing_starts_by_itself() {
    let a = app();
    let be = a.agent("Backend Agent", "backend", 1);
    a.set_col("To do", ColumnInput { auto: Some(false), ..Default::default() });
    let c = a.card("Manual", "Backlog", "FAKE_HANG", None);
    assert_eq!(a.drag(&c, "To do").await, None, "a drag");
    assert_eq!(a.assign(&c, &be).await, None, "an assignment");
    assert!(runs::pull(&a.st).await.is_empty(), "the queue");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c).is_empty());
    assert_eq!(a.column(&c), "To do");
}

#[tokio::test]
async fn c2_run_in_a_manual_column_takes_the_chosen_agent_else_the_assignee_else_the_columns_first_agent_else_says_to_put_one_on() {
    let a = app();
    let be = a.agent("Backend Agent", "backend", 1);
    let fe = a.agent("Frontend Agent", "frontend", 1);
    let ux = a.agent("Design Agent", "design", 1);
    for c in ["To do", "In progress"] {
        columns::remove_agent(&a.st.db, &a.st.you_id, &a.state(c), &ux).unwrap();
    }
    a.set_col("To do", ColumnInput { auto: Some(false), agent_ids: Some(vec![be.clone(), fe.clone()]), ..Default::default() });
    let agent_of = |s: &runs::RunSummary| core_runs::get(&a.st.db, &s.run_id).unwrap().agent_id;

    // the chosen agent (the card is assigned to another)
    let c1 = a.card("Chosen", "To do", "", Some(&be));
    let s = runs::run_once(&a.st, &c1, Some(ux.clone()), None).await.unwrap();
    assert_eq!(agent_of(&s), ux, "the agent picked");

    // the card's agent assignee, also when it isn't the column's first agent
    let c2 = a.card("Assigned", "To do", "", Some(&fe));
    assert_eq!(runs::suggest(&a.st, &c2).as_deref(), Some(fe.as_str()));
    let s = runs::run_once(&a.st, &c2, None, None).await.unwrap();
    assert_eq!(agent_of(&s), fe, "the assignee");

    // else the column's first agent
    let c3 = a.card("Unassigned", "To do", "", None);
    assert_eq!(runs::suggest(&a.st, &c3).as_deref(), Some(be.as_str()));
    let s = runs::run_once(&a.st, &c3, None, None).await.unwrap();
    assert_eq!(agent_of(&s), be, "the column's first agent");

    // a person as assignee doesn't count: still the column's first agent
    let c4 = a.card("Mine", "To do", "", Some(&a.st.you_id.clone()));
    assert_eq!(runs::suggest(&a.st, &c4).as_deref(), Some(be.as_str()));

    // nobody on the column: an error that says to put an agent on it (Team page)
    a.set_col("To do", ColumnInput { agent_ids: Some(vec![]), ..Default::default() });
    let c5 = a.card("Nobody", "To do", "", None);
    assert_eq!(runs::suggest(&a.st, &c5), None);
    let err = runs::run_once(&a.st, &c5, None, None).await.unwrap_err();
    assert!(err.contains("Team page") && err.contains("To do") && err.contains("agent"), "{err}");
    assert!(a.runs_of(&c5).is_empty());
}

// ---- D. Old wake-up settings; no worker heartbeat ----

#[tokio::test]
async fn d1_an_agents_old_manual_wakeup_changes_nothing_on_an_auto_column() {
    let a = app();
    let be = a.add_agent(AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), wakeup: "manual".into(), ..Default::default() });
    assert_eq!(team::agent(&a.st.db, &be).unwrap().wakeup.as_deref(), Some("manual"));
    let c = a.card("Wake", "Backlog", "FAKE_HANG", None);
    assert_eq!(a.drag(&c, "To do").await, Some(a.ident(&c)), "the Auto column starts it");
    assert_eq!(a.live_run(&c).unwrap().agent_id, be);
    a.stop_everything().await;
}

#[tokio::test]
async fn d2_there_is_no_worker_heartbeat_an_agent_with_an_old_heartbeat_on_a_manual_column_never_starts() {
    let a = app();
    let be = a.add_agent(AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), wakeup: "heartbeat".into(),
                                      heartbeat_minutes: Some(1), ..Default::default() });
    let lead = a.add_agent(AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() });
    a.set_col("To do", ColumnInput { auto: Some(false), ..Default::default() });
    let c = a.card("Beat", "To do", "FAKE_HANG", Some(&be));
    // what the app's minute tick does (runs::pull, then the Team Lead's board check), long after the old heartbeat
    for minutes in [1, 5, 60] {
        assert!(runs::pull(&a.st).await.is_empty());
        let _ = gizai_lib::board::tick(&a.st, gizai_core::ids::now_ms() + minutes * 60_000).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c).is_empty());
    assert!(a.runs_by(&be).is_empty());
    assert!(a.runs_by(&lead).is_empty());
    assert_eq!(team::agent(&a.st.db, &be).unwrap().last_heartbeat_at, None, "no heartbeat was recorded for the worker");
}

// ---- E. Off the column, paused ----

#[tokio::test]
async fn e_an_agent_taken_off_a_column_or_paused_gets_no_new_cards_there() {
    let a = app();
    let be = a.agent("Backend Agent", "backend", 1);
    // taken off To do (the × on the Team page)
    columns::remove_agent(&a.st.db, &a.st.you_id, &a.state("To do"), &be).unwrap();
    let c1 = a.card("Off", "Backlog", "FAKE_HANG", None);
    assert_eq!(a.drag(&c1, "To do").await, None);
    assert!(runs::pull(&a.st).await.is_empty());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c1).is_empty());
    // put back on: it starts the card (the check above isn't vacuous)
    columns::add_agent(&a.st.db, &a.st.you_id, &a.state("To do"), &be).unwrap();
    let started = runs::pull(&a.st).await;
    assert_eq!(started.len(), 1);
    let r = a.live_run(&c1).expect("c1 runs");
    runs::stop(&a.st, &r.run_id);
    until("c1's run ended", 20, || runs::live(&a.st).is_empty()).await;

    // paused: no new cards in In progress, where it still is
    team::set_agent_status(&a.st.db, &a.st.you_id, &be, "paused").unwrap();
    runs::resume_pull(&a.st, &be);
    let c2 = a.card("Paused", "Backlog", "FAKE_HANG", None);
    assert_eq!(a.drag(&c2, "In progress").await, None);
    assert!(runs::pull(&a.st).await.is_empty());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&c2).is_empty());
    a.stop_everything().await;
}

// ---- F. The Team Lead's tools ----

#[tokio::test]
async fn f1_get_workflow_lists_the_columns_in_order_with_agents_auto_or_manual_and_next_plus_the_labels_and_no_rules() {
    let a = app();
    let lead = a.lead();
    a.agent("Backend Agent", "backend", 1);
    a.agent("Frontend Agent", "frontend", 1);
    a.agent("QA Agent", "qa", 1);
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(names(&v), DEFAULT_COLUMNS, "{v}");
    assert_eq!(agents_on(&v, "To do"), ["Backend Agent", "Frontend Agent"]);
    assert_eq!(agents_on(&v, "Testing"), ["QA Agent"]);
    assert_eq!((col(&v, "To do")["start"].as_str(), col(&v, "To do")["next"].as_str()), (Some("auto"), Some("In progress")));
    assert_eq!((col(&v, "Testing")["start"].as_str(), col(&v, "Testing")["next"].as_str()), (Some("auto"), Some("Review")));
    assert_eq!((col(&v, "Review")["next"].as_str(), col(&v, "Deploy")["start"].as_str(), col(&v, "Deploy")["next"].as_str()),
               (Some("Deploy"), Some("manual"), Some("Done")));
    let labels: Vec<&str> = v["labels"].as_array().unwrap().iter().map(|l| l.as_str().unwrap()).collect();
    for l in ["frontend", "backend", "qa", "bug"] {
        assert!(labels.contains(&l), "{labels:?}");
    }
    let text = v.to_string().to_lowercase();
    assert!(!text.contains("rule") && !text.contains("routing"), "no routing rules: {v}");
}

#[tokio::test]
async fn f2_set_column_by_name_sets_agents_auto_next_name_and_place() {
    let a = app();
    let lead = a.lead();
    a.agent("Backend Agent", "backend", 1);
    a.agent("Frontend Agent", "frontend", 1);
    a.tool(&lead, "set_column", json!({"column": "testing", "agents": ["Frontend Agent", "Backend Agent"], "auto": false, "next": "Deploy"})).await.unwrap();
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(agents_on(&v, "Testing"), ["Frontend Agent", "Backend Agent"]);
    assert_eq!((col(&v, "Testing")["start"].as_str(), col(&v, "Testing")["next"].as_str()), (Some("manual"), Some("Deploy")));

    a.tool(&lead, "set_column", json!({"column": "Testing", "new_name": "QA check"})).await.unwrap();
    a.tool(&lead, "set_column", json!({"column": "QA check", "after": "To do"})).await.unwrap();
    a.tool(&lead, "set_column", json!({"column": "QA check", "auto": true})).await.unwrap();
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(names(&v), ["Backlog", "To do", "QA check", "In progress", "Review", "Deploy", "Done"]);
    assert_eq!((col(&v, "QA check")["start"].as_str(), col(&v, "QA check")["next"].as_str()), (Some("auto"), Some("Deploy")));
    assert_eq!(agents_on(&v, "QA check"), ["Frontend Agent", "Backend Agent"]);
    // an empty list takes them all off
    a.tool(&lead, "set_column", json!({"column": "QA check", "agents": []})).await.unwrap();
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert!(agents_on(&v, "QA check").is_empty());
}

#[tokio::test]
async fn f3_set_column_refuses_agents_on_people_columns_a_self_link_auto_without_next_and_unknown_names_each_with_a_short_reason() {
    let a = app();
    let lead = a.lead();
    a.agent("Backend Agent", "backend", 1);
    a.tool(&lead, "add_column", json!({"name": "Cancelled", "after": "Done", "kind": "cancelled"})).await.unwrap();
    let before = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    let short = |e: &str| { assert!(!e.trim().is_empty() && e.chars().count() < 300, "a short reason: {e}"); };

    for c in ["Backlog", "Review", "Done", "Cancelled"] {
        let e = a.tool(&lead, "set_column", json!({"column": c, "agents": ["Backend Agent"]})).await.unwrap_err();
        assert!(e.contains("takes no agents") && e.contains(c), "{c}: {e}");
        short(&e);
    }
    let e = a.tool(&lead, "set_column", json!({"column": "In progress", "next": "In progress"})).await.unwrap_err();
    assert!(e.contains("itself"), "{e}");
    short(&e);
    let e = a.tool(&lead, "set_column", json!({"column": "Deploy", "next": "none", "auto": true})).await.unwrap_err();
    assert!(e.contains("Auto") && e.contains("next column"), "{e}");
    short(&e);
    let e = a.tool(&lead, "set_column", json!({"column": "Nope", "auto": false})).await.unwrap_err();
    assert!(e.contains("No column called \"Nope\"") && e.contains("To do"), "{e}");
    short(&e);
    let e = a.tool(&lead, "set_column", json!({"column": "To do", "agents": ["Nobody Agent"]})).await.unwrap_err();
    assert!(e.contains("Nobody Agent"), "{e}");
    short(&e);
    let e = a.tool(&lead, "set_column", json!({"column": "To do", "next": "Nowhere"})).await.unwrap_err();
    assert!(e.contains("No column called \"Nowhere\""), "{e}");
    short(&e);
    let e = a.tool(&lead, "set_column", json!({"column": "To do", "after": "Nowhere"})).await.unwrap_err();
    assert!(e.contains("No column called \"Nowhere\""), "{e}");
    let after = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(before["columns"], after["columns"], "nothing changed");
}

#[tokio::test]
async fn f4_add_column_takes_kind_agents_auto_and_next() {
    let a = app();
    let lead = a.lead();
    a.agent("Frontend Agent", "frontend", 1);
    a.agent("Backend Agent", "backend", 1);
    a.tool(&lead, "add_column", json!({"name": "Design", "after": "To do", "kind": "work", "agents": ["Frontend Agent"], "auto": true, "next": "In progress"}))
        .await.unwrap();
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(names(&v), ["Backlog", "To do", "Design", "In progress", "Testing", "Review", "Deploy", "Done"]);
    let d = col(&v, "Design");
    assert_eq!((d["kind"].as_str(), d["start"].as_str(), d["next"].as_str()), (Some("work"), Some("auto"), Some("In progress")));
    assert_eq!(agents_on(&v, "Design"), ["Frontend Agent"]);
    // refused setups add nothing
    let e = a.tool(&lead, "add_column", json!({"name": "Staging", "after": "Deploy", "kind": "review", "agents": ["Backend Agent"]})).await.unwrap_err();
    assert!(e.contains("no agents"), "{e}");
    let e = a.tool(&lead, "add_column", json!({"name": "Polish", "after": "Deploy", "kind": "work", "auto": true})).await.unwrap_err();
    assert!(e.contains("next column"), "{e}");
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(names(&v).len(), 8, "{v}");
}

#[tokio::test]
async fn f5_save_label_adds_renames_and_recolours_a_label() {
    let a = app();
    let lead = a.lead();
    a.tool(&lead, "save_label", json!({"name": "Must have", "color": "#FF0000"})).await.unwrap();
    let find = |n: &str| gizai_core::labels::list(&a.st.db).unwrap().into_iter().find(|l| l.name == n);
    assert_eq!(find("Must have").unwrap().color.as_deref(), Some("#ff0000"));
    a.tool(&lead, "save_label", json!({"name": "Plain"})).await.unwrap();
    assert_eq!(find("Plain").unwrap().color, None, "the colour is optional");
    a.tool(&lead, "save_label", json!({"label": "must have", "name": "Should have", "color": "#00ff00"})).await.unwrap();
    assert!(find("Must have").is_none());
    assert_eq!(find("Should have").unwrap().color.as_deref(), Some("#00ff00"));
    a.tool(&lead, "save_label", json!({"label": "Should have", "color": "#0000ff"})).await.unwrap();
    assert_eq!(find("Should have").unwrap().color.as_deref(), Some("#0000ff"), "recoloured, name kept");
    // the new label is in get_workflow's labels
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert!(v["labels"].as_array().unwrap().iter().any(|l| l == "Should have"), "{v}");
}

#[tokio::test]
async fn f6_create_task_and_update_task_refuse_an_unknown_label() {
    let a = app();
    let lead = a.lead();
    let e = a.tool(&lead, "create_task", json!({"project": "KADE", "title": "Export", "labels": ["Nope"]})).await.unwrap_err();
    assert!(e.contains("there is no label Nope; add it with save_label"), "{e}");
    assert!(tasks::list(&a.st.db, &TaskFilter::default()).unwrap().is_empty(), "no card was made");
    let t = a.card("Export", "Backlog", "", None);
    let e = a.tool(&lead, "update_task", json!({"task": a.ident(&t), "labels": ["bug", "Nope"]})).await.unwrap_err();
    assert!(e.contains("there is no label Nope; add it with save_label"), "{e}");
    assert!(a.task(&t).labels.is_empty());
    // a known label works
    let ok = a.tool(&lead, "create_task", json!({"project": "KADE", "title": "Bugfix", "labels": ["bug"]})).await.unwrap();
    assert_eq!(ok["task"]["labels"], json!(["bug"]));
}

#[tokio::test]
async fn f7_add_routing_rule_is_gone() {
    let a = app();
    let lead = a.lead();
    let e = a.tool(&lead, "add_routing_rule", json!({"label": "bug", "agent": "Backend Agent"})).await.unwrap_err();
    assert!(e.contains("unknown tool"), "{e}");
    let all: Vec<String> = tools::catalog().into_iter().map(|t| t.name).collect();
    assert!(!all.iter().any(|n| n.contains("rout") || n.contains("rule")), "{all:?}");
    for n in ["get_workflow", "set_column", "add_column", "save_label"] {
        assert!(all.iter().any(|x| x == n), "{n} in {all:?}");
    }
}

// ---- G. GA-24 fixes ----

#[tokio::test]
async fn g1_update_task_with_a_good_title_and_a_bad_label_changes_nothing() {
    let a = app();
    let lead = a.lead();
    let t = a.card("Old title", "Backlog", "", None);
    let e = a.tool(&lead, "update_task", json!({"task": a.ident(&t), "title": "New title", "labels": ["Nope"]})).await.unwrap_err();
    assert!(e.contains("there is no label Nope"), "{e}");
    assert_eq!(a.task(&t).title, "Old title");
}

#[tokio::test]
async fn g2_a_tool_result_over_4000_characters_reaches_the_chat_as_valid_json_that_says_it_was_cut() {
    let a = app();
    let mut st = a.st.clone();
    a.lead();
    // the chat needs the gizai-mcp shim to exist; this fake never starts it
    st.mcp_shim = Some(PathBuf::from(FAKE));
    let fake = a.tmp.path().join("long-chat.py");
    write_script(&fake, &format!(r#"#!/usr/bin/env python3
import json, sys
argv = sys.argv[1:]
if "--input-format" in argv:
    sys.stdin.readline()
    sys.stdout.write(open("{FIXTURES}/models-init.jsonl").read()); sys.stdout.flush(); sys.stdin.read(); sys.exit(0)
def flag(n):
    return argv[argv.index(n) + 1] if n in argv and argv.index(n) + 1 < len(argv) else None
sid = flag("--resume") or flag("--session-id") or "none"
sys.stdin.read()
def out(o): print(json.dumps(o), flush=True)
out({{"type": "system", "subtype": "init", "session_id": sid, "model": "fake-model", "tools": ["mcp__gizai__list_tasks"], "mcp_servers": [{{"name": "gizai", "status": "connected"}}]}})
out({{"type": "assistant", "message": {{"role": "assistant", "content": [{{"type": "tool_use", "id": "toolu_1", "name": "mcp__gizai__list_tasks", "input": {{}}}}]}}, "session_id": sid}})
big = json.dumps({{"count": 300, "tasks": [{{"identifier": "KADE-%d" % i, "title": "Export every invoice as one CSV file " * 3, "column": "To do"}} for i in range(300)]}})
out({{"type": "user", "message": {{"role": "user", "content": [{{"type": "tool_result", "tool_use_id": "toolu_1", "content": [{{"type": "text", "text": big}}], "is_error": False}}]}}, "session_id": sid}})
out({{"type": "assistant", "message": {{"role": "assistant", "content": [{{"type": "text", "text": "Here they are."}}]}}, "session_id": sid}})
out({{"type": "result", "subtype": "success", "is_error": False, "result": "Here they are.", "total_cost_usd": 0.01, "num_turns": 2, "session_id": sid, "usage": {{"input_tokens": 10, "output_tokens": 5}}}})
"#));
    let (thread, done) = gizai_lib::chat::send(&st, None, "list every task".into(), Some(fake.to_string_lossy().into_owned())).await.unwrap();
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("the turn ended").unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let msgs = gizai_core::chat::messages(&st.db, &thread).unwrap();
    let tool = msgs.iter().find(|m| m.role == "tool").expect("a tool message");
    let result = tool.tool.as_ref().and_then(|t| t["result"].as_str()).unwrap_or_else(|| panic!("a result: {:?}", tool.tool));
    assert!(result.chars().count() <= 4000, "cut to 4000: {}", result.chars().count());
    let v: Value = serde_json::from_str(result).unwrap_or_else(|e| panic!("valid JSON ({e}): {result}"));
    assert!(v["cut"].as_str().is_some_and(|c| c.contains("cut")), "a note that it was cut: {v}");
}

#[tokio::test]
async fn g3_a_project_reference_that_is_one_projects_key_and_anothers_name_is_an_error_listing_both() {
    let a = app();
    let lead = a.lead();
    let kd = gizai_core::projects::create(&a.st.db, &a.st.you_id, ProjectInput { name: "Kade".into(), key: "KD".into(), ..Default::default() }).unwrap();
    for tool in ["get_project", "create_task"] {
        let e = a.tool(&lead, tool, json!({"project": "Kade", "title": "X"})).await.unwrap_err();
        assert!(e.contains("Kade portal") && e.contains("KADE") && e.contains("KD") && e.contains(&a.project) && e.contains(&kd), "{tool}: {e}");
    }
    assert!(tasks::list(&a.st.db, &TaskFilter::default()).unwrap().is_empty());
    // the key alone, or the full name, is exact
    let ok = a.tool(&lead, "get_project", json!({"project": "Kade portal"})).await.unwrap();
    assert!(ok.to_string().contains("KADE"), "{ok}");
}

#[tokio::test]
async fn g4_an_unknown_column_is_an_error_listing_the_teams_columns() {
    let a = app();
    let lead = a.lead();
    let e = a.tool(&lead, "create_task", json!({"project": "KADE", "title": "X", "column": "Nope"})).await.unwrap_err();
    assert!(e.contains("No column called \"Nope\"") && e.contains(&DEFAULT_COLUMNS.join(", ")), "{e}");
    assert!(tasks::list(&a.st.db, &TaskFilter::default()).unwrap().is_empty());
    let t = a.card("Card", "Backlog", "", None);
    let e = a.tool(&lead, "move_task", json!({"task": a.ident(&t), "column": "Nope"})).await.unwrap_err();
    assert!(e.contains("No column called \"Nope\"") && e.contains(&DEFAULT_COLUMNS.join(", ")), "{e}");
    assert_eq!(a.column(&t), "Backlog");
}

// ---- H. Done when ----

/// "GitHub" (bare repositories under tmp/github) and a local clone of https://github.com/acme/shop that can only reach it.
fn github(tmp: &Path) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = tmp.join("github/acme/shop");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    let rewrite = format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display());
    git(tmp, &["clone", "-q", "-c", &rewrite, "-c", "protocol.allow=never", "-c", "protocol.file.allow=always",
               "-c", "user.email=t@t", "-c", "user.name=t", LINK, local.to_str().unwrap()]);
    (bare, local)
}

/// A fake gh: `pr list` answers with dir/list.json; `pr create` answers with dir/create.out.
fn fake_gh(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let gh = dir.join("gh");
    write_script(&gh, &format!(r#"#!/bin/sh
d='{d}'
echo "$*" >> "$d/gh-args"
case "$1 $2" in
  "pr list") [ -f "$d/list.json" ] && cat "$d/list.json"; exit 0 ;;
  "pr create") cat > "$d/gh-stdin"; cat "$d/create.out"; exit 0 ;;
esac
echo "unknown command: $*" >&2; exit 1
"#, d = dir.display()));
    gh
}

#[tokio::test]
async fn h_done_when_a_card_goes_through_the_columns_set_up_by_the_team_lead_and_deploy_is_manual() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let (_bare, local) = github(tmp.path());
    let project = gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Shop".into(), key: "KADE".into(),
        repo_path: Some(local.to_string_lossy().into_owned()), repo_url: Some(LINK.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    let gh = tmp.path().join("gh");
    settings::set(&st.db, "gh_bin", &fake_gh(&gh).to_string_lossy().to_string()).unwrap();
    settings::set(&st.db, "claude_bin", &answering_fake(tmp.path())).unwrap();
    let a = App { st, project, tmp };
    let lead = a.lead();
    let be1 = a.agent("Backend Agent", "backend", 1);
    let be2 = a.agent("Backend Agent 2", "backend", 1);
    let qa1 = a.agent("QA Agent", "qa", 1);
    let qa2 = a.add_agent(AgentInput { name: "QA Agent 2".into(), role_key: "qa".into(),
        instructions_md: Some("Test the card. FAKE_ANSWER=qa_pass. End with the GIZAI_RESULT line.".into()), ..Default::default() });
    let ops1 = a.agent("DevOps Agent", "devops", 1);
    let ops2 = a.add_agent(AgentInput { name: "DevOps Agent 2".into(), role_key: "devops".into(),
        instructions_md: Some("Deploy the card. FAKE_ANSWER=deployed. End with the GIZAI_RESULT line.".into()), ..Default::default() });

    for args in [
        json!({"column": "To do", "agents": ["Backend Agent 2"], "auto": true, "next": "In progress"}),
        json!({"column": "In progress", "agents": ["Backend Agent 2"], "auto": true, "next": "Testing"}),
        json!({"column": "Testing", "agents": ["QA Agent 2"], "auto": true, "next": "Review"}),
        json!({"column": "Review", "next": "Deploy"}),
        json!({"column": "Deploy", "agents": ["DevOps Agent 2"], "auto": false, "next": "Done"}),
    ] {
        a.tool(&lead, "set_column", args.clone()).await.unwrap_or_else(|e| panic!("{args}: {e}"));
    }
    let v = a.tool(&lead, "get_workflow", json!({})).await.unwrap();
    assert_eq!(agents_on(&v, "To do"), ["Backend Agent 2"]);
    assert_eq!(agents_on(&v, "In progress"), ["Backend Agent 2"]);
    assert_eq!(agents_on(&v, "Testing"), ["QA Agent 2"]);
    assert_eq!(agents_on(&v, "Deploy"), ["DevOps Agent 2"]);
    assert_eq!((col(&v, "Deploy")["start"].as_str(), col(&v, "Deploy")["next"].as_str()), (Some("manual"), Some("Done")));
    let text = v.to_string();
    assert!(!text.contains("\"QA Agent\"") && !text.contains("\"DevOps Agent\""), "QA Agent and DevOps Agent are off: {v}");

    // a new card in To do: To do → In progress → Testing → Review by itself
    let c = a.card("Export invoices", "To do", "Download all invoices as one CSV file.", None);
    runs::dispatch(&a.st, &c).await;
    until("the card reaches Review and its runs have ended", 60, || a.column(&c) == "Review" && a.live_run(&c).is_none()).await;
    let rs = a.runs_of(&c);
    let seen: Vec<(String, Option<String>)> = rs.iter().rev().map(|r| (r.agent_id.clone(), r.outcome.clone())).collect();
    assert_eq!(seen, vec![(be2.clone(), Some("ready_for_testing".into())), (qa2.clone(), Some("qa_pass".into()))]);
    let moves: Vec<Value> = tasks::activity(&a.st.db, &c).unwrap().into_iter().filter_map(|e| e.diff.get("column").cloned()).collect();
    for m in [json!(["To do", "In progress"]), json!(["In progress", "Testing"]), json!(["Testing", "Review"])] {
        assert!(moves.contains(&m), "{m} in {moves:?}");
    }

    // the pull request is opened and merged (fake gh): Deploy, and nothing starts there (Manual)
    let wt = PathBuf::from(rs[0].worktree_path.clone().unwrap());
    std::fs::write(wt.join("invoices.csv"), "id\n").unwrap();
    git(&wt, &["add", "invoices.csv"]);
    git(&wt, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "invoices"]);
    std::fs::write(gh.join("create.out"), format!("{LINK}/pull/7\n")).unwrap();
    let t0 = Instant::now();
    loop {
        match gizai_lib::pulls::open(&a.st, &c).await {
            Ok(_) => break,
            Err(e) if e.contains("busy") && t0.elapsed() < Duration::from_secs(10) => tokio::time::sleep(Duration::from_millis(100)).await,
            Err(e) => panic!("open pull request: {e}"),
        }
    }
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let pr = json!([{"number": 7, "url": format!("{LINK}/pull/7"), "state": "MERGED", "isDraft": false, "headRefOid": head, "commits": [{"oid": head}]}]);
    std::fs::write(gh.join("list.json"), pr.to_string()).unwrap();
    let t0 = Instant::now();
    while a.column(&c) != "Deploy" {
        assert!(t0.elapsed() < Duration::from_secs(20), "the merge moved it to Deploy; it is in {}", a.column(&c));
        gizai_lib::pulls::check_all(&a.st).await;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(runs::pull(&a.st).await.is_empty());
    runs::dispatch(&a.st, &c).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(runs::live(&a.st).is_empty());
    assert_eq!(a.runs_of(&c).len(), 2, "nothing started in Deploy");
    assert!(a.runs_by(&ops2).is_empty());

    // Run on it: DevOps Agent 2 (the column's agent) answers deployed: Done
    let s = runs::run_once(&a.st, &c, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("deployed")), "{s:?}");
    assert_eq!(core_runs::get(&a.st.db, &s.run_id).unwrap().agent_id, ops2);
    assert_eq!(a.column(&c), "Done");

    tokio::time::sleep(Duration::from_millis(300)).await;
    for (name, id) in [("QA Agent", &qa1), ("DevOps Agent", &ops1), ("Backend Agent", &be1)] {
        assert!(a.runs_by(id).is_empty(), "{name} never got a run");
    }
}

// ---- GA-32's DevOps rules through a real run ----

#[tokio::test]
async fn ga32_a_devops_run_on_a_review_card_never_goes_to_qa_and_deployed_outside_deploy_holds_the_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &answering_fake(tmp.path())).unwrap();
    let repo = git_repo(tmp.path());
    let project = gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(),
        repo_path: Some(repo.to_string_lossy().into_owned()), ..Default::default() }).unwrap();
    let a = App { st, project, tmp };
    let qa = a.agent("QA Agent", "qa", 1);
    let ops = a.agent("DevOps Agent", "devops", 1);
    let ops2 = a.add_agent(AgentInput { name: "DevOps Agent 2".into(), role_key: "devops".into(),
        instructions_md: Some("FAKE_ANSWER=deployed".into()), ..Default::default() });
    let you = a.st.you_id.clone();

    // ready_for_testing from a DevOps run on a Review card: it stays in Review, for you, and QA doesn't start
    let c = a.card("Fix the merge conflicts", "Review", "", Some(&you));
    let s = runs::run_once(&a.st, &c, Some(ops.clone()), None).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("ready_for_testing"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let t = a.task(&c);
    assert_eq!((t.state_name.as_str(), t.assignee_id.as_deref(), t.hold.as_deref()), ("Review", Some(you.as_str()), None));
    assert!(a.runs_by(&qa).is_empty(), "QA didn't start");
    let implementer: Option<String> = a.st.db.read(|c2| Ok(c2.query_row("SELECT implementer_actor_id FROM tasks WHERE id=?1", [&c], |r| r.get(0))?)).unwrap();
    assert_eq!(implementer, None, "the DevOps Agent isn't the implementer");

    // deployed on a card that isn't in Deploy: held
    let c2 = a.card("Release it", "Review", "", Some(&you));
    let s = runs::run_once(&a.st, &c2, Some(ops2.clone()), None).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("deployed"));
    let t = a.task(&c2);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("Review", Some("needs_decision")));
}
