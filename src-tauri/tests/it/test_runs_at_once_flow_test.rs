//! GA-212 QA, end to end with a fake `claude`: a project's Test runs at once.
//! - The queue: project GA ("Giz AI", Test runs at once 1) has two cards in Testing; the QA Agent (cards at once 3)
//!   starts one, passes over the other and starts project KADE's (no limit) meanwhile; the second GA card starts by
//!   itself once the first one's test run has ended. With the setting empty, both GA cards start at once, as today.
//!   With Test runs at once 2 and three GA cards in Testing, one pull starts two side by side and the third waits (GA-191).
//! - Two GA cards that start at the same moment (by hand, or the queue and a hand) never both start.
//! - Run, Continue and the Team Lead's start_agent_run and continue_agent_run on the second card don't start it, and say
//!   why word for word; with a limit of 2 they name both cards in a test run.
//! - Runs on GA's cards in To do or In progress neither count nor wait.
//! - The Team Lead's get_project and list_projects show it, update_project sets it and 0 clears it.
//! - The board check: the waiting card is a test_runs_full finding after an hour.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::columns::{self, ColumnInput};
use gizai_core::model::*;
use gizai_core::{projects, runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, runs, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fixtures");
const ONE: &str = "Giz AI tests 1 card at a time (project settings): wait for GA-1 to finish testing, or raise the limit on the project page.";

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

/// A fake `claude`: a prompt with FAKE_UNTIL:<name> (a card's description) works until the file dir/done-<name> is
/// there (at most 60 s), then ends with qa_pass; any other prompt goes to the usual fake (FAKE_HANG hangs until Stop).
fn gate_fake(dir: &Path) -> String {
    let path = dir.join("gate-claude.sh");
    write_script(&path, &format!(r#"#!/usr/bin/env bash
case " $* " in *" --input-format stream-json "*) exec '{FAKE}' "$@" ;; esac
prompt="$(cat)"
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

struct App { st: AppState, fake: String, ga: String, kade: String, qa: String, tmp: tempfile::TempDir }

/// Project GA ("Giz AI") with Test runs at once `limit` and project KADE ("Kade") without one, each with a local git
/// repository; the QA Agent with three cards at once on Testing (Auto); "Runs at once" 10.
fn app(limit: Option<i64>) -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let fake = gate_fake(tmp.path());
    settings::set(&st.db, "claude_bin", &fake).unwrap();
    settings::set(&st.db, "max_concurrent_runs", &10u32).unwrap();
    let repo = |name: &str| Some(git_repo(tmp.path(), name).to_string_lossy().into_owned());
    let ga = projects::create(&st.db, &st.you_id, ProjectInput { name: "Giz AI".into(), key: "GA".into(), repo_path: repo("ga"),
        test_runs_at_once: limit, ..Default::default() }).unwrap();
    let kade = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), repo_path: repo("kade"),
        ..Default::default() }).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let qa = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), max_runs: Some(3),
        ..Default::default() }).unwrap();
    App { st, fake, ga, kade, qa, tmp }
}

impl App {
    fn team(&self) -> team::Team { team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap() }
    fn state(&self, name: &str) -> String {
        self.team().states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
    }
    fn card(&self, project: &str, column: &str, description: &str, priority: i64) -> String {
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: project.to_string(), title: format!("A card in {column}"),
            description_md: description.into(), state_id: Some(self.state(column)), priority, ..Default::default() }).unwrap()
    }
    fn agent(&self, name: &str, role: &str, max_runs: i64) -> String {
        team::add_agent(&self.st.db, &self.st.you_id, &self.team().id, AgentInput { name: name.into(), role_key: role.into(), max_runs: Some(max_runs),
            chat_enabled: Some(role == "lead"), ..Default::default() }).unwrap()
    }
    /// Testing is Manual: only Run starts its cards, so a run's end starts none of them.
    fn manual_testing(&self) {
        columns::set_column(&self.st.db, &self.st.you_id, &self.state("Testing"), ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    }
    /// What the project form saves: the project as it is, with Test runs at once `n` (0: empty).
    fn limit(&self, n: i64) {
        let p = projects::get(&self.st.db, &self.ga).unwrap();
        projects::update(&self.st.db, &self.st.you_id, &self.ga, ProjectInput { name: p.name, key: p.key, repo_path: p.repo_path,
            default_branch: Some(p.default_branch), test_runs_at_once: Some(n), ..Default::default() }).unwrap();
    }
    fn release(&self, name: &str) { std::fs::write(self.tmp.path().join(format!("done-{name}")), "").unwrap(); }
    fn task(&self, t: &str) -> Task { tasks::get(&self.st.db, t).unwrap() }
    fn column(&self, t: &str) -> String { self.task(t).state_name }
    fn runs_of(&self, t: &str) -> Vec<Run> { core_runs::list_for_task(&self.st.db, t).unwrap() }
    fn live_cards(&self) -> Vec<String> {
        let mut v: Vec<String> = runs::live(&self.st).into_iter().map(|l| l.task_id).collect();
        v.sort();
        v
    }
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

// ---- the queue ----

#[tokio::test]
async fn the_qa_agent_starts_one_of_a_limited_projects_testing_cards_another_projects_meanwhile_and_the_next_once_the_first_ended() {
    let a = app(Some(1));
    let a1 = a.card(&a.ga, "Testing", "FAKE_UNTIL:a1", 1);
    let a2 = a.card(&a.ga, "Testing", "FAKE_UNTIL:a2", 2);
    let b1 = a.card(&a.kade, "Testing", "FAKE_UNTIL:b1", 3);
    assert_eq!((a.task(&a1).identifier.as_str(), a.task(&a2).identifier.as_str()), ("GA-1", "GA-2"));

    // GA-1 first (urgent); GA-2 is next in line but waits, so the agent takes KADE's card instead
    let started: Vec<String> = runs::pull(&a.st).await.into_iter().map(|(t, _)| t).collect();
    assert_eq!(started, [a1.clone(), b1.clone()], "GA-1, then KADE-1: GA-2 is passed over");
    assert_eq!(a.live_cards(), sorted(vec![a1.clone(), b1.clone()]));
    assert!(a.runs_of(&a2).is_empty(), "GA-2 has no run");
    assert!(runs::pull(&a.st).await.is_empty(), "the next pull starts nothing either, with a slot still free");
    let t = a.task(&a2);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("Testing", None), "it only waits");

    // the board check: nothing yet, and a test_runs_full finding once it has waited over an hour
    let now = gizai_core::ids::now_ms();
    let of = |at: i64| -> Vec<(String, String, String)> { gizai_lib::board::findings(&a.st, at).unwrap().into_iter()
        .filter(|f| f.task_id == a2).map(|f| (f.kind, f.code, f.reason)).collect() };
    assert!(of(now + 3 * 60_000).is_empty(), "no free_slot finding: it waits for GA-1");
    assert_eq!(of(now + 61 * 60_000), [("waiting".to_string(), "test_runs_full".to_string(),
        "Giz AI tests 1 card at a time (project settings): it has waited over an hour for GA-1 to finish testing".to_string())]);

    // GA-1's test run ends (QA passed it): GA-2 starts by itself, while KADE-1 is still at work
    a.release("a1");
    until("GA-2 starts by itself", 30, || a.live_cards().contains(&a2)).await;
    assert_eq!(a.column(&a1), "Review", "GA-1 passed");
    assert_eq!(a.live_cards(), sorted(vec![a2.clone(), b1.clone()]));
    assert_eq!(a.runs_of(&b1).len(), 1, "KADE-1 kept its one run");
    a.release("a2");
    a.release("b1");
    until("both runs end", 30, || runs::live(&a.st).is_empty()).await;
    assert_eq!((a.column(&a2), a.column(&b1)), ("Review".to_string(), "Review".to_string()));
    a.stop_everything().await;
}

#[tokio::test]
async fn without_the_setting_both_cards_of_the_project_start_at_once_as_today() {
    let a = app(None);
    let a1 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    let a2 = a.card(&a.ga, "Testing", "FAKE_HANG", 2);
    let b1 = a.card(&a.kade, "Testing", "FAKE_HANG", 3);
    let started: Vec<String> = runs::pull(&a.st).await.into_iter().map(|(t, _)| t).collect();
    assert_eq!(started, [a1.clone(), a2.clone(), b1.clone()]);
    assert_eq!(a.live_cards(), sorted(vec![a1.clone(), a2.clone(), b1.clone()]));
    // and a Run by hand on a third one starts too
    let a3 = a.card(&a.ga, "Testing", "FAKE_HANG", 4);
    a.run(&a3).await.unwrap();
    assert!(a.live_cards().contains(&a3));
    assert!(gizai_lib::board::findings(&a.st, gizai_core::ids::now_ms() + 2 * 60 * 60_000).unwrap().iter().all(|f| f.code != "test_runs_full"));
    a.stop_everything().await;
}

/// GA-191 claims a pull's cards first and starts them side by side: the cards claimed in that pull count against Test
/// runs at once, and the claimed ones, each starting while the other is, don't make each other wait.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_pull_claims_as_many_of_the_projects_testing_cards_as_its_test_runs_at_once_and_starts_them_side_by_side() {
    let a = app(Some(2));
    let a1 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    let a2 = a.card(&a.ga, "Testing", "FAKE_HANG", 2);
    let a3 = a.card(&a.ga, "Testing", "FAKE_HANG", 3);
    let started: Vec<String> = runs::pull(&a.st).await.into_iter().map(|(t, _)| t).collect();
    assert_eq!(started, [a1.clone(), a2.clone()], "GA-1 and GA-2 start, GA-3 waits, with a slot of the QA Agent's still free");
    assert_eq!(a.live_cards(), sorted(vec![a1.clone(), a2.clone()]));
    let t = a.task(&a3);
    assert_eq!((a.runs_of(&a3).len(), t.state_name.as_str(), t.hold.as_deref()), (0, "Testing", None), "GA-3 only waits");
    assert!(runs::pull(&a.st).await.is_empty(), "the next pull starts nothing either");
    assert_eq!(a.run(&a3).await.unwrap_err(),
               "Giz AI tests 2 cards at a time (project settings): wait for GA-1 or GA-2 to finish testing, or raise the limit on the project page.");
    a.stop_everything().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_cards_of_the_project_that_start_at_the_same_moment_never_both_start() {
    let a = app(Some(1));
    a.manual_testing();
    let a1 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    let a2 = a.card(&a.ga, "Testing", "FAKE_HANG", 2);
    let ga_live = |a: &App| a.live_cards().into_iter().filter(|t| t == &a1 || t == &a2).count();
    let mut one_started = 0;
    for round in 0..6 {
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let go = |t: String| {
            let (st, fake, barrier) = (a.st.clone(), a.fake.clone(), barrier.clone());
            tokio::spawn(async move {
                barrier.wait().await;
                runs::start(&st, &t, None, Some(fake), "manual").await.map(|(id, _)| id)
            })
        };
        let (h1, h2) = (go(a1.clone()), go(a2.clone()));
        let (r1, r2) = (h1.await.unwrap(), h2.await.unwrap());
        let ok = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
        assert!(ok <= 1, "round {round}: both started: {r1:?} {r2:?}");
        assert!(ga_live(&a) <= 1, "round {round}: {:?}", a.live_cards());
        for (r, other) in [(&r1, "GA-2"), (&r2, "GA-1")] {
            if let Err(e) = r {
                assert!(e.starts_with("Giz AI tests 1 card at a time (project settings): wait for GA-")
                        && e.ends_with(" to finish testing, or raise the limit on the project page."), "round {round}: {e} ({other})");
            }
        }
        one_started += ok;
        for t in a.live_cards() {
            a.stop(&t).await;
        }
    }
    assert!(one_started > 0, "in six rounds one of them started at least once");

    // the queue and a Run by hand at the same moment: Testing is Auto again, and two fresh cards wait in it
    columns::set_column(&a.st.db, &a.st.you_id, &a.state("Testing"), ColumnInput { auto: Some(true), ..Default::default() }).unwrap();
    let a3 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    let a4 = a.card(&a.ga, "Testing", "FAKE_HANG", 2);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
    let pull = {
        let (st, barrier) = (a.st.clone(), barrier.clone());
        tokio::spawn(async move { barrier.wait().await; runs::pull(&st).await.into_iter().map(|(t, _)| t).collect::<Vec<_>>() })
    };
    let hand = {
        let (st, fake, barrier, t) = (a.st.clone(), a.fake.clone(), barrier.clone(), a4.clone());
        tokio::spawn(async move { barrier.wait().await; runs::start(&st, &t, None, Some(fake), "manual").await.map(|(id, _)| id) })
    };
    let (pulled, by_hand) = (pull.await.unwrap(), hand.await.unwrap());
    let live: Vec<String> = a.live_cards().into_iter().filter(|t| t == &a3 || t == &a4).collect();
    assert!(live.len() <= 1, "never both: pulled {pulled:?}, by hand {by_hand:?}, live {live:?}");
    a.stop_everything().await;
}

// ---- a start by hand ----

#[tokio::test]
async fn run_continue_start_agent_run_and_continue_agent_run_on_the_second_card_dont_start_it_and_say_why() {
    let a = app(Some(1));
    a.manual_testing();
    let lead = a.agent("Team Lead", "lead", 1);
    let a1 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    let a2 = a.card(&a.ga, "Testing", "FAKE_HANG", 2);
    // GA-2 had a run that was stopped part-way, so Continue has one to resume
    let r2 = a.run(&a2).await.unwrap();
    a.stop(&a2).await;
    // GA-1 is in its test run
    a.run(&a1).await.unwrap();

    assert_eq!(a.run(&a2).await.unwrap_err(), ONE, "Run");
    assert_eq!(runs::continue_run(&a.st, &r2, Some(a.fake.clone())).await.map(|(id, _)| id).unwrap_err(), ONE, "Continue");
    assert_eq!(runs::continue_with_note(&a.st, &r2, Some("Check the CSV export".into()), Some(a.fake.clone())).await.map(|(id, _)| id).unwrap_err(),
               ONE, "Continue with a note");
    assert_eq!(tools::call(&a.st, &lead, "start_agent_run", json!({"task": "GA-2"})).await.unwrap_err(), ONE, "start_agent_run");
    assert_eq!(tools::call(&a.st, &lead, "start_agent_run", json!({"task": "GA-2", "agent": "QA Agent"})).await.unwrap_err(), ONE);
    assert_eq!(tools::call(&a.st, &lead, "continue_agent_run", json!({"task": "GA-2"})).await.unwrap_err(), ONE, "continue_agent_run");
    // nothing started: GA-2 still has its one stopped run, in Testing, not on hold
    let t = a.task(&a2);
    assert_eq!((a.runs_of(&a2).len(), t.state_name.as_str(), t.hold.as_deref()), (1, "Testing", None));
    assert_eq!(a.live_cards(), [a1.clone()]);

    // a limit of 2: GA-3 starts next to GA-1, and GA-2 waits for both, named by number
    a.limit(2);
    let a3 = a.card(&a.ga, "Testing", "FAKE_HANG", 3);
    a.run(&a3).await.unwrap();
    let two = "Giz AI tests 2 cards at a time (project settings): wait for GA-1 or GA-3 to finish testing, or raise the limit on the project page.";
    assert_eq!(a.run(&a2).await.unwrap_err(), two);
    assert_eq!(tools::call(&a.st, &lead, "start_agent_run", json!({"task": "GA-2"})).await.unwrap_err(), two);
    assert_eq!(tools::call(&a.st, &lead, "continue_agent_run", json!({"task": "GA-2"})).await.unwrap_err(), two);

    // once GA-1's run has ended, Continue on GA-2 starts it
    a.stop(&a1).await;
    runs::continue_run(&a.st, &r2, Some(a.fake.clone())).await.unwrap();
    assert_eq!(a.live_cards(), sorted(vec![a2.clone(), a3.clone()]));
    // and a limit cleared lets a fourth start
    let a4 = a.card(&a.ga, "Testing", "FAKE_HANG", 4);
    assert!(a.run(&a4).await.unwrap_err().starts_with("Giz AI tests 2 cards at a time"));
    a.limit(0);
    a.run(&a4).await.unwrap();
    a.stop_everything().await;
}

#[tokio::test]
async fn runs_on_the_projects_cards_in_to_do_or_in_progress_neither_count_nor_wait() {
    let a = app(Some(1));
    let be = a.agent("Backend Agent", "backend", 4);
    let todo = a.card(&a.ga, "To do", "FAKE_HANG", 1);
    let prog = a.card(&a.ga, "In progress", "FAKE_HANG", 1);
    let started: Vec<String> = runs::pull(&a.st).await.into_iter().map(|(t, _)| t).collect();
    assert_eq!(sorted(started), sorted(vec![todo.clone(), prog.clone()]), "the Backend Agent starts both");
    assert!(runs::live(&a.st).iter().all(|l| l.agent_id == be));
    // they don't fill GA's Test runs at once: its Testing card starts
    let a1 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    let started: Vec<String> = runs::pull(&a.st).await.into_iter().map(|(t, _)| t).collect();
    assert_eq!(started, [a1.clone()]);
    assert_eq!(a.live_run(&a1), runs::live(&a.st).into_iter().find(|l| l.agent_id == a.qa).unwrap().run_id, "QA tests it");
    // while GA-… in Testing is in its test run, GA's cards in To do and In progress still start, by the queue and by hand
    let prog2 = a.card(&a.ga, "In progress", "FAKE_HANG", 1);
    let started: Vec<String> = runs::pull(&a.st).await.into_iter().map(|(t, _)| t).collect();
    assert_eq!(started, [prog2.clone()]);
    let todo2 = a.card(&a.ga, "To do", "FAKE_HANG", 1);
    a.run(&todo2).await.unwrap();
    assert!(a.live_cards().contains(&todo2));
    // and only a Testing card waits
    let a2 = a.card(&a.ga, "Testing", "FAKE_HANG", 1);
    assert!(runs::pull(&a.st).await.is_empty());
    let e = a.run(&a2).await.unwrap_err();
    let first = a.task(&a1).identifier;
    assert_eq!(e, format!("Giz AI tests 1 card at a time (project settings): wait for {first} to finish testing, or raise the limit on the project page."));
    a.stop_everything().await;
}

// ---- the Team Lead's tools ----

fn project_in(list: &Value, key: &str) -> Value {
    list["projects"].as_array().unwrap().iter().find(|p| p["key"] == key).unwrap_or_else(|| panic!("no {key}: {list}")).clone()
}

#[tokio::test]
async fn get_project_and_list_projects_show_it_and_update_project_sets_it_and_0_clears_it() {
    let a = app(None);
    let lead = a.agent("Team Lead", "lead", 1);
    let call = |name: &'static str, args: Value| { let (st, lead) = (a.st.clone(), lead.clone()); async move { tools::call(&st, &lead, name, args).await } };
    let shown = |project: &'static str| async move {
        let p = call("get_project", json!({"project": project})).await.unwrap();
        let list = call("list_projects", json!({})).await.unwrap();
        (p["project"]["test_runs_at_once"].clone(), project_in(&list, project)["test_runs_at_once"].clone())
    };
    assert_eq!(shown("GA").await, (Value::Null, Value::Null), "none by default");
    assert_eq!(shown("KADE").await, (Value::Null, Value::Null));

    let r = call("update_project", json!({"project": "GA", "test_runs_at_once": 2})).await.unwrap();
    assert_eq!(r["project"]["test_runs_at_once"], 2, "{r}");
    assert_eq!(shown("GA").await, (json!(2), json!(2)));
    assert_eq!(shown("KADE").await, (Value::Null, Value::Null), "another project is untouched");
    assert_eq!(projects::get(&a.st.db, &a.ga).unwrap().test_runs_at_once, Some(2));
    // another change keeps it
    call("update_project", json!({"project": "GA", "name": "Giz AI app"})).await.unwrap();
    assert_eq!(shown("GA").await, (json!(2), json!(2)));
    // outside 1 to 20: refused, nothing changes
    for (bad, said) in [(json!(21), "21"), (json!(-1), "-1"), (json!("21"), "21"), (json!(100), "100")] {
        let e = call("update_project", json!({"project": "GA", "test_runs_at_once": bad, "name": "Renamed"})).await.unwrap_err();
        assert_eq!(e, format!("test_runs_at_once is a number from 1 to 20, or 0 to clear it, not {said}. Nothing changed."), "{bad}");
    }
    let e = call("update_project", json!({"project": "GA", "test_runs_at_once": "two"})).await.unwrap_err();
    assert_eq!(e, "\"test_runs_at_once\" must be a whole number");
    let p = projects::get(&a.st.db, &a.ga).unwrap();
    assert_eq!((p.test_runs_at_once, p.name.as_str()), (Some(2), "Giz AI app"), "nothing changed");
    // 20 is the most; 0 clears it, and so does an empty string
    call("update_project", json!({"project": "GA", "test_runs_at_once": 20})).await.unwrap();
    assert_eq!(shown("GA").await, (json!(20), json!(20)));
    let r = call("update_project", json!({"project": "GA", "test_runs_at_once": 0})).await.unwrap();
    assert_eq!(r["project"]["test_runs_at_once"], Value::Null, "{r}");
    assert_eq!(shown("GA").await, (Value::Null, Value::Null));
    call("update_project", json!({"project": "GA", "test_runs_at_once": "1"})).await.unwrap();
    assert_eq!(shown("GA").await, (json!(1), json!(1)));
    call("update_project", json!({"project": "GA", "test_runs_at_once": ""})).await.unwrap();
    assert_eq!(shown("GA").await, (Value::Null, Value::Null));

    // the catalog offers it
    let cat = tools::catalog();
    let def = |name: &str| cat.iter().find(|t| t.name == name).unwrap();
    let p = &def("update_project").input_schema["properties"]["test_runs_at_once"];
    assert_eq!(p["type"], "integer", "{p}");
    assert!(p["description"].as_str().unwrap().contains("1 to 20; 0 clears it"), "{p}");
    for name in ["get_project", "list_projects"] {
        assert!(def(name).description.contains("Test runs at once"), "{name}");
    }
}
