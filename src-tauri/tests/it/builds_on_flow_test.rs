//! GA-228 QA, end to end with a fake `claude`: Builds on.
//! - The queue: B builds on A, both in To do, and the Backend Agent has free slots: A and another card C start, B doesn't
//!   and keeps its place (no move, no hold). Once a person moves A to Deploy, B starts by itself; the same once the Team
//!   Lead moves A to Done, and once a run's end moves A on into Deploy. An archived card counts; a cancelled one never.
//! - Builds on holds a card in Testing (the QA Agent) and in a Research column of the team's own the same way.
//! - Run, Continue and the Team Lead's start_agent_run and continue_agent_run on B don't start it and say why word for
//!   word, for one card and for two; once the cards are merged, Run starts it.
//! - The run prompt lists the cards a card builds on after its description.
//! - The Team Lead's tools: create_task and update_task set builds_on (an empty list clears it; the refusals word for
//!   word), get_task shows builds_on and needed_by, list_tasks waits_for, check_board builds_on_blocked.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::columns::{self, ColumnInput};
use gizai_core::model::*;
use gizai_core::{builds_on, projects, runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, runs, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fixtures");

fn git_repo(dir: &Path, name: &str) -> PathBuf {
    let repo = dir.join(name);
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = std::process::Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake `claude`: a prompt with FAKE_SAVE:<name> is kept in dir/prompt-<name>; one with FAKE_UNTIL:<name> works until
/// the file dir/done-<name> is there (at most 60 s), then ends with qa_pass; any other prompt goes to the usual fake
/// (FAKE_HANG hangs until Stop).
fn gate_fake(dir: &Path) -> String {
    let path = dir.join("gate-claude.sh");
    write_script(&path, &format!(r#"#!/usr/bin/env bash
case " $* " in *" --input-format stream-json "*) exec '{FAKE}' "$@" ;; esac
prompt="$(cat)"
if [[ "$prompt" =~ FAKE_SAVE:([a-z0-9]+) ]]; then
  printf '%s' "$prompt" > '{d}/prompt-'"${{BASH_REMATCH[1]}}"
fi
if [[ "$prompt" =~ FAKE_UNTIL:([a-z0-9]+) ]]; then
  gate='{d}/done-'"${{BASH_REMATCH[1]}}"
  for _ in $(seq 1 1200); do [ -f "$gate" ] && break; sleep 0.05; done
  sed "s/ready_for_testing/qa_pass/g" '{FIXTURES}/run-ok.jsonl'
  exit 0
fi
printf '%s' "$prompt" | exec '{FAKE}' "$@"
"#, d = dir.display()));
    path.to_string_lossy().into_owned()
}

struct App { st: AppState, fake: String, ga: String, kade: String, be: String, tmp: tempfile::TempDir }

/// Project GA ("Giz AI") and KADE ("Kade"), each with a local git repository; the Backend Agent with three cards at once
/// (To do and In progress, Auto); "Runs at once" 10; a Cancelled column after Done.
fn app() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let fake = gate_fake(tmp.path());
    settings::set(&st.db, "claude_bin", &fake).unwrap();
    settings::set(&st.db, "max_concurrent_runs", &10u32).unwrap();
    let repo = |name: &str| Some(git_repo(tmp.path(), name).to_string_lossy().into_owned());
    let ga = projects::create(&st.db, &st.you_id, ProjectInput { name: "Giz AI".into(), key: "GA".into(), repo_path: repo("ga"), ..Default::default() }).unwrap();
    let kade = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), repo_path: repo("kade"), ..Default::default() }).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let be = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), max_runs: Some(3),
        ..Default::default() }).unwrap();
    let a = App { st, fake, ga, kade, be, tmp };
    team::add_state(&a.st.db, &a.st.you_id, &team_id, "Cancelled", &a.state("Done"), "cancelled").unwrap();
    a
}

impl App {
    fn team(&self) -> team::Team { team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap() }
    fn state(&self, name: &str) -> String {
        self.team().states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
    }
    fn card_titled(&self, project: &str, column: &str, title: &str, description: &str, priority: i64) -> String {
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: project.to_string(), title: title.into(),
            description_md: description.into(), state_id: Some(self.state(column)), priority, ..Default::default() }).unwrap()
    }
    fn card(&self, column: &str, description: &str, priority: i64) -> String {
        self.card_titled(&self.ga, column, &format!("A card in {column}"), description, priority)
    }
    fn agent(&self, name: &str, role: &str, max_runs: i64) -> String {
        team::add_agent(&self.st.db, &self.st.you_id, &self.team().id, AgentInput { name: name.into(), role_key: role.into(), max_runs: Some(max_runs),
            chat_enabled: Some(role == "lead"), ..Default::default() }).unwrap()
    }
    fn builds_on(&self, t: &str, on: &[&String]) {
        builds_on::set(&self.st.db, &self.st.you_id, t, &on.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap();
    }
    /// A person's move, as the board's drag does it (`commands::move_task`): the move, then the agents take their next cards.
    async fn person_moves(&self, t: &str, column: &str) {
        tasks::move_to(&self.st.db, &self.st.you_id, t, &self.state(column), "").unwrap();
        runs::dispatch(&self.st, t).await;
    }
    fn release(&self, name: &str) { std::fs::write(self.tmp.path().join(format!("done-{name}")), "").unwrap(); }
    fn task(&self, t: &str) -> Task { tasks::get(&self.st.db, t).unwrap() }
    fn ident(&self, t: &str) -> String { self.task(t).identifier }
    fn runs_of(&self, t: &str) -> Vec<Run> { core_runs::list_for_task(&self.st.db, t).unwrap() }
    fn live_cards(&self) -> Vec<String> {
        let mut v: Vec<String> = runs::live(&self.st).into_iter().map(|l| l.task_id).collect();
        v.sort();
        v
    }
    fn is_live(&self, t: &str) -> bool { runs::live(&self.st).iter().any(|l| l.task_id == t) }
    fn live_run(&self, t: &str) -> String {
        runs::live(&self.st).into_iter().find(|l| l.task_id == t).unwrap_or_else(|| panic!("{t} has no live run")).run_id
    }
    async fn run(&self, t: &str) -> Result<String, String> {
        runs::start(&self.st, t, None, Some(self.fake.clone()), "manual").await.map(|(id, _)| id)
    }
    async fn stop(&self, t: &str) {
        let r = self.live_run(t);
        runs::stop(&self.st, &r);
        until("the run stops", 30, || !runs::live(&self.st).iter().any(|l| l.run_id == r)).await;
    }
    async fn pull(&self) -> Vec<String> { runs::pull(&self.st).await.into_iter().map(|(t, _)| t).collect() }
    async fn stop_everything(&self) {
        runs::stop_all(&self.st, Duration::from_secs(12)).await;
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

async fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn one(card: &str, on: &str) -> String {
    format!("{card} builds on {on}, which isn't merged yet. It starts by itself once {on} is merged; to start it now, take {on} off its Builds on.")
}

// ---- the queue ----

#[tokio::test]
async fn b_waits_while_a_and_another_card_start_and_starts_by_itself_once_a_person_moves_a_to_deploy() {
    let a = app();
    let ca = a.card("To do", "FAKE_HANG", 2);
    let cb = a.card("To do", "FAKE_HANG", 1); // urgent: first in line, but for Builds on
    let cc = a.card("To do", "FAKE_HANG", 3);
    a.builds_on(&cb, &[&ca]);
    assert_eq!((a.ident(&ca), a.ident(&cb), a.ident(&cc)), ("GA-1".to_string(), "GA-2".to_string(), "GA-3".to_string()));

    assert_eq!(a.pull().await, [ca.clone(), cc.clone()], "GA-1 and GA-3 start, GA-2 is passed over");
    assert_eq!(a.live_cards(), sorted(vec![ca.clone(), cc.clone()]));
    assert!(a.pull().await.is_empty(), "the next pull starts nothing either, with a slot still free");
    let t = a.task(&cb);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.waits_for.clone(), a.runs_of(&cb).len()), ("To do", None, vec!["GA-1".to_string()], 0),
               "it only waits: no move, no hold, no run");

    // GA-1's agent is done with it, and a person drags it to Deploy (merged): GA-2 starts, nobody moved it
    a.stop(&ca).await;
    a.person_moves(&ca, "Deploy").await;
    until("GA-2 starts by itself", 30, || a.is_live(&cb)).await;
    let t = a.task(&cb);
    assert_eq!((t.state_name.as_str(), t.waits_for.len()), ("In progress", 0), "its run took it on, as any start does");
    assert_eq!(a.live_cards(), sorted(vec![cb.clone(), cc.clone()]), "GA-1 doesn't start in Deploy");
    a.stop_everything().await;
}

#[tokio::test]
async fn b_starts_by_itself_once_the_team_lead_moves_a_to_done_an_archived_card_counts_and_a_cancelled_one_never() {
    let a = app();
    let lead = a.agent("Team Lead", "lead", 1);
    let ca = a.card("Review", "FAKE_HANG", 0);
    let cb = a.card("To do", "FAKE_HANG", 0);
    a.builds_on(&cb, &[&ca]);
    assert!(a.pull().await.is_empty(), "GA-1 is in Review: GA-2 waits");
    let r = tools::call(&a.st, &lead, "move_task", json!({"task": "GA-1", "column": "Done"})).await.unwrap();
    assert_eq!(r["task"]["column"], "Done", "{r}");
    until("GA-2 starts by itself", 30, || a.is_live(&cb)).await;
    a.stop(&cb).await;

    // archived from Done: it still counts
    let archived = a.card("Done", "", 0);
    tasks::archive(&a.st.db, &a.st.you_id, &archived).unwrap();
    let cd = a.card("To do", "FAKE_HANG", 0);
    a.builds_on(&cd, &[&archived]);
    assert!(a.task(&cd).waits_for.is_empty());
    assert_eq!(a.pull().await, [cd.clone()]);

    // cancelled: never
    let cancelled = a.card("Cancelled", "", 0);
    let ce = a.card("To do", "FAKE_HANG", 0);
    a.builds_on(&ce, &[&cancelled]);
    assert!(a.pull().await.is_empty());
    assert_eq!(a.task(&ce).waits_for, [a.ident(&cancelled)]);
    assert_eq!(a.run(&ce).await.unwrap_err(), one(&a.ident(&ce), &a.ident(&cancelled)));
    a.stop_everything().await;
}

#[tokio::test]
async fn once_a_runs_end_moves_a_on_into_deploy_b_starts_by_itself() {
    let a = app();
    // a team without a Review step: QA's pass moves a card from Testing straight to Deploy
    columns::set_column(&a.st.db, &a.st.you_id, &a.state("Testing"), ColumnInput { next_state_id: Some(a.state("Deploy")), ..Default::default() }).unwrap();
    let qa = a.agent("QA Agent", "qa", 1);
    let ca = a.card("Testing", "FAKE_UNTIL:a", 0);
    let cb = a.card("To do", "FAKE_HANG", 0);
    a.builds_on(&cb, &[&ca]);
    assert_eq!(a.pull().await, [ca.clone()], "QA tests GA-1; GA-2 waits");
    assert_eq!(runs::live(&a.st)[0].agent_id, qa);
    a.release("a");
    until("GA-2 starts by itself", 30, || a.is_live(&cb)).await;
    assert_eq!(a.task(&ca).state_name, "Deploy", "QA passed GA-1");
    assert_eq!(runs::live(&a.st).into_iter().find(|l| l.task_id == cb).unwrap().agent_id, a.be);
    a.stop_everything().await;
}

#[tokio::test]
async fn builds_on_holds_a_card_in_testing_or_in_a_research_column_the_same_way() {
    let a = app();
    a.agent("QA Agent", "qa", 3);
    // Research: a work column of the team's own before In progress, on Auto, with a Research agent on it
    let research = team::add_state(&a.st.db, &a.st.you_id, &a.team().id, "Research", &a.state("To do"), "in_progress").unwrap();
    let ra = a.agent("Research Agent", "backend", 3);
    columns::set_column(&a.st.db, &a.st.you_id, &research, ColumnInput { agent_ids: Some(vec![ra.clone()]), auto: Some(true),
        next_state_id: Some(a.state("In progress")), ..Default::default() }).unwrap();
    let base = a.card("Review", "", 0);
    let t = a.card("Testing", "FAKE_HANG", 0);
    let r = a.card("Research", "FAKE_HANG", 0);
    a.builds_on(&t, &[&base]);
    a.builds_on(&r, &[&base]);
    assert!(a.pull().await.is_empty(), "neither starts");
    for (card, column) in [(&t, "Testing"), (&r, "Research")] {
        let x = a.task(card);
        assert_eq!((x.state_name.as_str(), x.hold.as_deref(), a.runs_of(card).len()), (column, None, 0));
        assert_eq!(a.run(card).await.unwrap_err(), one(&x.identifier, "GA-1"));
    }
    a.person_moves(&base, "Deploy").await;
    until("both start by themselves", 30, || a.is_live(&t) && a.is_live(&r)).await;
    assert_eq!(runs::live(&a.st).into_iter().find(|l| l.task_id == r).unwrap().agent_id, ra);
    a.stop_everything().await;
}

// ---- a start by hand ----

#[tokio::test]
async fn run_continue_start_agent_run_and_continue_agent_run_refuse_word_for_word_for_one_card_and_for_two() {
    let a = app();
    let lead = a.agent("Team Lead", "lead", 1);
    let a1 = a.card("Review", "", 0);
    let a2 = a.card("Review", "", 0);
    let b = a.card("To do", "FAKE_HANG", 0);
    // GA-3 had a run that was stopped part-way, so Continue has one to resume
    let r = a.run(&b).await.unwrap();
    a.stop(&b).await;
    a.builds_on(&b, &[&a1]);

    let refusals = |want: String| {
        let (st, fake, lead, r, b) = (a.st.clone(), a.fake.clone(), lead.clone(), r.clone(), b.clone());
        async move {
            let task = gizai_core::tasks::get(&st.db, &b).unwrap().identifier;
            assert_eq!(runs::start(&st, &b, None, Some(fake.clone()), "manual").await.map(|(id, _)| id).unwrap_err(), want, "Run");
            assert_eq!(runs::continue_run(&st, &r, Some(fake.clone())).await.map(|(id, _)| id).unwrap_err(), want, "Continue");
            assert_eq!(runs::continue_with_note(&st, &r, Some("Use the new templates".into()), Some(fake)).await.map(|(id, _)| id).unwrap_err(),
                       want, "Continue with a note");
            assert_eq!(tools::call(&st, &lead, "start_agent_run", json!({"task": task})).await.unwrap_err(), want, "start_agent_run");
            assert_eq!(tools::call(&st, &lead, "start_agent_run", json!({"task": task, "agent": "Backend Agent"})).await.unwrap_err(), want);
            assert_eq!(tools::call(&st, &lead, "continue_agent_run", json!({"task": task})).await.unwrap_err(), want, "continue_agent_run");
        }
    };
    refusals("GA-3 builds on GA-1, which isn't merged yet. It starts by itself once GA-1 is merged; to start it now, take GA-1 off its Builds on.".into()).await;
    a.builds_on(&b, &[&a2, &a1]);
    refusals("GA-3 builds on GA-1 and GA-2, which aren't merged yet. It starts by itself once they are merged; to start it now, take them off its Builds on.".into()).await;
    // nothing started: GA-3 still has its one stopped run, where that run left it (In progress), not on hold
    let t = a.task(&b);
    assert_eq!((a.runs_of(&b).len(), t.state_name.as_str(), t.hold.as_deref()), (1, "In progress", None));
    assert!(runs::live(&a.st).is_empty());

    // GA-1 merged: only GA-2 is named
    a.person_moves(&a1, "Done").await;
    refusals(one("GA-3", "GA-2")).await;
    // both merged: Continue starts it
    tasks::move_to(&a.st.db, &a.st.you_id, &a2, &a.state("Deploy"), "").unwrap();
    runs::continue_run(&a.st, &r, Some(a.fake.clone())).await.unwrap();
    assert_eq!(a.live_cards(), [b.clone()]);
    a.stop_everything().await;
}

#[tokio::test]
async fn taking_the_card_off_its_builds_on_lets_run_start_it() {
    let a = app();
    let a1 = a.card("Review", "", 0);
    let b = a.card("To do", "FAKE_HANG", 0);
    a.builds_on(&b, &[&a1]);
    assert_eq!(a.run(&b).await.unwrap_err(), one("GA-2", "GA-1"));
    builds_on::remove(&a.st.db, &a.st.you_id, &b, &a1).unwrap();
    a.run(&b).await.unwrap();
    assert!(a.is_live(&b));
    a.stop_everything().await;
}

// ---- the run prompt ----

#[tokio::test]
async fn the_run_prompt_lists_the_cards_it_builds_on_after_its_description() {
    let a = app();
    let base = a.card_titled(&a.ga, "Deploy", "Team templates (backend)", "", 0);
    let other = a.card_titled(&a.kade, "Done", "Shared fonts.", "", 0);
    let b = a.card_titled(&a.ga, "To do", "Team templates screens", "Build the screens. FAKE_SAVE:b FAKE_HANG", 0);
    a.builds_on(&b, &[&other, &base]);
    let plain = a.card_titled(&a.ga, "To do", "No builds on", "FAKE_SAVE:plain FAKE_HANG", 0);
    assert_eq!(sorted(a.pull().await), sorted(vec![b.clone(), plain.clone()]));
    let file = |name: &str| a.tmp.path().join(format!("prompt-{name}"));
    until("both prompts are saved", 30, || file("b").exists() && file("plain").exists()).await;
    let p = std::fs::read_to_string(file("b")).unwrap();
    assert!(p.contains("Build the screens. FAKE_SAVE:b FAKE_HANG\n\nBuilds on GA-1: Team templates (backend), merged.\n\
                        Builds on KADE-1: Shared fonts, merged.\n\n## Acceptance criteria"), "{p}");
    let p = std::fs::read_to_string(file("plain")).unwrap();
    assert!(!p.contains("Builds on GA-"), "a card without Builds on gets no such line: {p}");
    a.stop_everything().await;
}

// ---- the Team Lead's tools ----

fn task_in<'a>(list: &'a Value, id: &str) -> &'a Value {
    list["tasks"].as_array().unwrap_or_else(|| panic!("no tasks: {list}")).iter().find(|t| t["task"] == id).unwrap_or_else(|| panic!("no {id}: {list}"))
}

#[tokio::test]
async fn create_task_and_update_task_set_builds_on_an_empty_list_clears_it_and_the_refusals_are_word_for_word() {
    let a = app();
    let lead = a.agent("Team Lead", "lead", 1);
    let call = |name: &'static str, args: Value| { let (st, lead) = (a.st.clone(), lead.clone()); async move { tools::call(&st, &lead, name, args).await } };
    let g1 = a.card("Review", "", 0); // GA-1, not merged yet
    let _g2 = a.card("Review", "", 0); // GA-2
    let k1 = a.card_titled(&a.kade, "Backlog", "Kade base", "", 0); // KADE-1
    let _ = (&g1, &k1);

    // create_task: builds on GA-1 and KADE-1 (another project's card) from the start, so no agent takes it
    let r = call("create_task", json!({"project": "GA", "title": "Screens", "column": "To do", "builds_on": ["KADE-1", "ga-1"]})).await.unwrap();
    assert_eq!((r["task"]["identifier"].as_str(), r["task"]["builds_on"].clone(), r["task"]["waits_for"].clone()),
               (Some("GA-3"), json!(["GA-1", "KADE-1"]), json!(["GA-1", "KADE-1"])), "{r}");
    let g3 = tasks::id_of(&a.st.db, "GA-3").unwrap();
    assert!(a.pull().await.is_empty(), "it waits");
    // to Backlog, so no agent takes it once its Builds on is cleared below
    tasks::move_to(&a.st.db, &a.st.you_id, &g3, &a.state("Backlog"), "").unwrap();
    // a card that doesn't exist, more than 20: refused, no card made
    let count = || tasks::list(&a.st.db, &TaskFilter::default()).unwrap().len();
    let n = count();
    assert_eq!(call("create_task", json!({"project": "GA", "title": "X", "builds_on": ["GA-99"]})).await.unwrap_err(),
               "No task GA-99: check the identifier with list_tasks. Nothing changed.");
    let many: Vec<String> = (0..21).map(|i| { let t = a.card("Backlog", "", 0); let _ = i; a.ident(&t) }).collect();
    let n = n + 21;
    let e = call("create_task", json!({"project": "GA", "title": "X", "builds_on": many})).await.unwrap_err();
    assert!(e.contains("can't build on more than 20 cards"), "{e}");
    assert_eq!(count(), n, "no card made");
    // exactly 20 is fine
    let r = call("create_task", json!({"project": "GA", "title": "Twenty", "column": "Backlog", "builds_on": many[..20]})).await.unwrap();
    assert_eq!(r["task"]["builds_on"].as_array().unwrap().len(), 20, "{r}");

    // update_task: the refusals word for word, and nothing changes
    assert_eq!(call("update_task", json!({"task": "GA-3", "builds_on": ["GA-3"]})).await.unwrap_err(), "GA-3 can't build on itself.");
    assert_eq!(call("update_task", json!({"task": "GA-1", "builds_on": ["GA-3"], "title": "Renamed"})).await.unwrap_err(),
               "GA-1 can't build on GA-3: GA-3 already builds on GA-1.");
    assert_eq!(a.task(&g1).title, "A card in Review", "nothing changed");
    call("update_task", json!({"task": "GA-2", "builds_on": ["GA-3"]})).await.unwrap();
    assert_eq!(call("update_task", json!({"task": "GA-1", "builds_on": ["GA-2"]})).await.unwrap_err(),
               "GA-1 can't build on GA-2: GA-2 already builds on GA-1, through GA-3.");
    let e = call("update_task", json!({"task": "GA-3", "builds_on": many})).await.unwrap_err();
    assert!(e.contains("can't build on more than 20 cards"), "{e}");
    assert_eq!(a.task(&g3).waits_for, ["GA-1", "KADE-1"], "nothing changed");

    // the full new list; an empty list clears it
    let r = call("update_task", json!({"task": "GA-3", "builds_on": ["KADE-1"]})).await.unwrap();
    assert_eq!((r["task"]["builds_on"].clone(), r["task"]["waits_for"].clone()), (json!(["KADE-1"]), json!(["KADE-1"])), "{r}");
    let r = call("update_task", json!({"task": "GA-3", "builds_on": []})).await.unwrap();
    assert!(r["task"].get("builds_on").is_none() && r["task"].get("waits_for").is_none(), "{r}");
    assert!(builds_on::of(&a.st.db, &g3).unwrap().builds_on.is_empty());
    // left out keeps it
    call("update_task", json!({"task": "GA-3", "builds_on": ["GA-1"]})).await.unwrap();
    call("update_task", json!({"task": "GA-3", "title": "Screens for templates"})).await.unwrap();
    assert_eq!(a.task(&g3).waits_for, ["GA-1"]);

    // the catalog offers it
    let cat = tools::catalog();
    for name in ["create_task", "update_task"] {
        let p = &cat.iter().find(|t| t.name == name).unwrap().input_schema["properties"]["builds_on"];
        assert_eq!((p["type"].as_str(), p["items"]["type"].as_str()), (Some("array"), Some("string")), "{name}: {p}");
    }
}

#[tokio::test]
async fn get_task_shows_builds_on_and_needed_by_list_tasks_waits_for_and_check_board_builds_on_blocked() {
    let a = app();
    let lead = a.agent("Team Lead", "lead", 1);
    let call = |name: &'static str, args: Value| { let (st, lead) = (a.st.clone(), lead.clone()); async move { tools::call(&st, &lead, name, args).await } };
    let base = a.card_titled(&a.ga, "Deploy", "Team templates (backend)", "", 0); // GA-1, merged
    let work = a.card_titled(&a.ga, "Review", "Fonts", "", 0); // GA-2, still in work (not merged)
    let screens = a.card_titled(&a.ga, "Backlog", "Screens", "", 0); // GA-3
    let plain = a.card_titled(&a.ga, "Backlog", "Plain", "", 0); // GA-4
    a.builds_on(&screens, &[&work, &base]);
    let _ = &plain;

    let r = call("get_task", json!({"task": "GA-3"})).await.unwrap();
    assert_eq!(r["task"]["builds_on"], json!([
        {"task": "GA-1", "title": "Team templates (backend)", "column": "Deploy", "merged": true},
        {"task": "GA-2", "title": "Fonts", "column": "Review", "merged": false},
    ]), "{r}");
    assert_eq!(r["task"]["needed_by"], json!([]), "{r}");
    let r = call("get_task", json!({"task": "GA-1"})).await.unwrap();
    assert_eq!((r["task"]["builds_on"].clone(), r["task"]["needed_by"].clone()),
               (json!([]), json!([{"task": "GA-3", "title": "Screens", "column": "Backlog"}])), "{r}");

    let list = call("list_tasks", json!({"project": "GA"})).await.unwrap();
    assert_eq!(task_in(&list, "GA-3")["waits_for"], json!(["GA-2"]), "only the cards not merged yet: {list}");
    assert!(task_in(&list, "GA-4").get("waits_for").is_none(), "a card that doesn't wait has none: {list}");

    // check_board: in an Auto column, waiting for a card still in work is not a finding; for one on hold or cancelled it is
    a.person_moves(&screens, "To do").await;
    let blocked = |v: &Value| -> Vec<(String, String)> { v["findings"].as_array().unwrap().iter()
        .filter(|f| f["task"] == "GA-3").map(|f| (f["why"].as_str().unwrap().to_string(), f["reason"].as_str().unwrap().to_string())).collect() };
    let v = call("check_board", json!({})).await.unwrap();
    assert!(blocked(&v).is_empty(), "{v}");
    tasks::update(&a.st.db, &a.st.you_id, &work, TaskPatch { hold: Some("needs_decision".into()), hold_reason: Some("Which fonts?".into()),
        ..Default::default() }).unwrap();
    let v = call("check_board", json!({})).await.unwrap();
    assert_eq!(blocked(&v), [("builds_on_blocked".to_string(), "GA-3 waits for GA-2, which is on hold: needs_decision".to_string())], "{v}");
    let r = call("get_task", json!({"task": "GA-3"})).await.unwrap();
    assert_eq!(r["task"]["builds_on"][1]["hold"], "needs_decision", "{r}");
    tasks::move_to(&a.st.db, &a.st.you_id, &work, &a.state("Cancelled"), "").unwrap();
    tasks::update(&a.st.db, &a.st.you_id, &work, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    let v = call("check_board", json!({})).await.unwrap();
    assert_eq!(blocked(&v), [("builds_on_blocked".to_string(), "GA-3 waits for GA-2, which is cancelled".to_string())], "{v}");
    a.stop_everything().await;
}
