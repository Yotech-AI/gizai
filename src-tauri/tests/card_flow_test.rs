//! GA-32 end to end with the fake `claude`: To do is a queue by priority, a start moves the card to In progress, a start
//! that can't work holds one card, the Testing switch, a DevOps run that never goes to QA, and the Deploy column where only
//! a person's Run starts an agent.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::model::*;
use gizai_lib::{AppState, runs};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// A fake `claude` like the usual one whose run ends with `outcome` instead of ready_for_testing.
fn fake_answering(dir: &Path, outcome: &str) -> String {
    let d = dir.join(format!("fake-{outcome}"));
    std::fs::create_dir_all(d.join("fixtures")).unwrap();
    let src = Path::new(FAKE).parent().unwrap().join("fixtures");
    let run = std::fs::read_to_string(src.join("run-ok.jsonl")).unwrap().replace("ready_for_testing", outcome);
    std::fs::write(d.join("fixtures/run-ok.jsonl"), run).unwrap();
    std::fs::copy(src.join("models-init.jsonl"), d.join("fixtures/models-init.jsonl")).unwrap();
    // copied by a child, so this process never holds the script open for writing (ETXTBSY in other tests' children)
    assert!(std::process::Command::new("cp").arg(FAKE).arg(d.join("fake-claude.sh")).status().unwrap().success());
    d.join("fake-claude.sh").to_string_lossy().into_owned()
}

/// A fake `claude` that waits `secs` before it does what the usual one does, so several runs are under way when the
/// first one fails.
fn slow_fake(dir: &Path, secs: f32) -> String {
    let path = dir.join("slow-claude.sh");
    let script = format!("#!/usr/bin/env bash\nsleep {secs}\nexec {FAKE} \"$@\"\n");
    // written by a child, so this process never holds the script open for writing (ETXTBSY in other tests' children)
    assert!(std::process::Command::new("sh").args(["-c", "printf '%s' \"$1\" > \"$2\" && chmod +x \"$2\"", "sh", &script])
        .arg(&path).status().unwrap().success());
    path.to_string_lossy().into_owned()
}

struct App { st: AppState, repo: PathBuf, tmp: tempfile::TempDir }

fn app() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let repo = git_repo(tmp.path());
    App { st, repo, tmp }
}

impl App {
    /// A backend card in To do with `priority`; its description tells the fake what to do (FAKE_HANG, …).
    fn card(&self, priority: i64, description: &str) -> String {
        let t = gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend");
        self.patch(&t, TaskPatch { priority: Some(priority), description_md: Some(description.into()), ..Default::default() });
        t
    }
    fn patch(&self, t: &str, patch: TaskPatch) {
        gizai_core::tasks::update(&self.st.db, &self.st.you_id, t, patch).unwrap();
    }
    /// The card without an assignee, as cards were before GA-49 (test_task now assigns its agent).
    fn unassign(&self, t: &str) {
        self.patch(t, TaskPatch { assignee_id: Some(String::new()), ..Default::default() });
    }
    fn team(&self) -> gizai_core::team::Team {
        gizai_core::team::get(&self.st.db, &gizai_core::team::list(&self.st.db).unwrap()[0].id).unwrap()
    }
    fn state(&self, name: &str) -> String {
        self.team().states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
    }
    fn to(&self, t: &str, column: &str) {
        gizai_core::tasks::move_to(&self.st.db, &self.st.you_id, t, &self.state(column), "").unwrap();
    }
    fn task(&self, t: &str) -> Task { gizai_core::tasks::get(&self.st.db, t).unwrap() }
    fn column(&self, t: &str) -> String { self.task(t).state_name }
    fn runs_of(&self, t: &str) -> Vec<gizai_core::model::Run> { gizai_core::runs::list_for_task(&self.st.db, t).unwrap() }
    fn agent(&self, name: &str) -> String {
        gizai_core::team::all_agents(&self.st.db).unwrap().into_iter().find(|(_, m)| m.name == name).unwrap_or_else(|| panic!("no {name}")).1.actor_id
    }
    /// Adds an agent (or changes it): role, wake-up, cards at once and model.
    fn agent_with(&self, name: &str, role: &str, wakeup: &str, max_runs: i64, model: Option<&str>) -> String {
        let input = AgentInput { name: name.into(), role_key: role.into(), wakeup: wakeup.into(), heartbeat_minutes: Some(1), max_runs: Some(max_runs),
                                 model: model.map(String::from), ..Default::default() };
        match gizai_core::team::all_agents(&self.st.db).unwrap().into_iter().find(|(_, m)| m.name == name) {
            Some((_, m)) => { gizai_core::team::update_agent(&self.st.db, &self.st.you_id, &m.actor_id, input).unwrap(); m.actor_id }
            None => gizai_core::team::add_agent(&self.st.db, &self.st.you_id, &self.team().id, input).unwrap(),
        }
    }
    fn live_cards(&self) -> Vec<String> {
        let mut v: Vec<String> = runs::live(&self.st).into_iter().map(|l| l.task_id).collect();
        v.sort();
        v
    }
    fn implementer(&self, t: &str) -> Option<String> {
        self.st.db.read(|c| Ok(c.query_row("SELECT implementer_actor_id FROM tasks WHERE id=?1", [t], |r| r.get(0))?)).unwrap()
    }
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(20), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> { v.sort(); v }

// ---- Part 1: the queue ----

#[tokio::test]
async fn a_run_moves_its_to_do_or_backlog_card_to_in_progress_as_the_agent_and_testing_cards_stay() {
    let a = app();
    let todo = a.card(0, "FAKE_HANG");
    let (run, done) = runs::start(&a.st, &todo, None, Some(FAKE.into()), "manual").await.unwrap();
    assert_eq!(a.column(&todo), "In progress", "at once, while it runs");
    let moves = gizai_core::tasks::activity(&a.st.db, &todo).unwrap();
    assert!(moves.iter().any(|e| e.actor_name.as_deref() == Some("Backend Agent") && e.diff == serde_json::json!({"column": ["To do", "In progress"]})),
            "{moves:?}");
    runs::stop(&a.st, &run);
    done.await.unwrap();
    // a person's Run on a Backlog card
    let backlog = a.card(0, "FAKE_HANG");
    a.to(&backlog, "Backlog");
    let be = a.agent("Backend Agent"); // nothing routes a Backlog card: the person picks the agent
    let (run, done) = runs::start(&a.st, &backlog, Some(be), Some(FAKE.into()), "manual").await.unwrap();
    assert_eq!(a.column(&backlog), "In progress");
    runs::stop(&a.st, &run);
    done.await.unwrap();
    // QA's run on a Testing card: it stays in Testing
    let qa = a.agent_with("QA Agent", "qa", "manual", 3, None);
    let testing = a.card(0, "FAKE_HANG");
    a.to(&testing, "Testing");
    let (run, done) = runs::start(&a.st, &testing, Some(qa), Some(FAKE.into()), "manual").await.unwrap();
    assert_eq!(a.column(&testing), "Testing");
    runs::stop(&a.st, &run);
    done.await.unwrap();
    assert_eq!(a.column(&testing), "Testing");
}

#[tokio::test]
async fn continue_on_a_card_dragged_back_to_to_do_moves_it_to_in_progress_as_the_agent() {
    let a = app();
    let t = a.card(0, "FAKE_HANG");
    let (run, done) = runs::start(&a.st, &t, None, Some(FAKE.into()), "manual").await.unwrap();
    runs::stop(&a.st, &run);
    done.await.unwrap();
    a.to(&t, "To do");
    let (_, done) = runs::continue_run(&a.st, &run, Some(FAKE.into())).await.unwrap();
    assert_eq!(a.column(&t), "In progress", "at once, while it runs");
    let moves: Vec<_> = gizai_core::tasks::activity(&a.st.db, &t).unwrap().into_iter()
        .filter(|e| e.diff == serde_json::json!({"column": ["To do", "In progress"]})).collect();
    assert_eq!(moves.iter().filter(|e| e.actor_name.as_deref() == Some("Backend Agent")).count(), 2, "Run and Continue: {moves:?}");
    done.await.unwrap();
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn the_queue_starts_the_best_waiting_card_each_time_a_slot_frees_up_and_never_restarts_a_stopped_one() {
    // Done when: Backend Agent at 3 runs and 5 more cards of mixed priority in To do.
    let a = app();
    let prios = [0, 4, 1, 3, 2, 2, 4, 0];
    let cards: Vec<String> = prios.iter().map(|p| a.card(*p, "FAKE_HANG")).collect();
    a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    // the card dragged in last (no priority) doesn't jump the queue
    assert_eq!(runs::dispatch(&a.st, &cards[7]).await, None);
    let first = sorted(vec![cards[2].clone(), cards[4].clone(), cards[5].clone()]);
    assert_eq!(a.live_cards(), first, "urgent and the two high cards");
    for c in &first {
        assert_eq!(a.column(c), "In progress");
    }
    for i in [0, 1, 3, 6, 7] {
        assert_eq!((a.column(&cards[i]).as_str(), a.runs_of(&cards[i]).len()), ("To do", 0), "card {i} waits");
    }
    // a 4th card dragged in while the agent is full waits too, and isn't held
    assert_eq!(runs::dispatch(&a.st, &cards[3]).await, None);
    assert_eq!((a.column(&cards[3]).as_str(), a.task(&cards[3]).hold), ("To do", None));
    // each time a run ends, the best waiting card moves to In progress and starts: medium, low, low, none, none
    for next in [3usize, 1, 6, 0, 7] {
        let stopped = runs::live(&a.st).remove(0);
        runs::stop(&a.st, &stopped.run_id);
        until(&format!("card {next} starts"), || a.live_cards().contains(&cards[next])).await;
        assert_eq!(a.column(&cards[next]), "In progress");
        assert_eq!(runs::live(&a.st).len(), 3, "never more than its cards at once");
        let t = a.task(&stopped.task_id);
        assert_eq!((t.state_name.as_str(), t.fail_count), ("In progress", 0));
        assert_eq!(a.runs_of(&stopped.task_id).len(), 1, "a stopped card isn't started again");
    }
    // nothing waits any more: a run that ends starts nothing
    let stopped = runs::live(&a.st).remove(0);
    runs::stop(&a.st, &stopped.run_id);
    until("the stopped run ends", || runs::live(&a.st).len() == 2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(runs::live(&a.st).len(), 2);
    assert!(cards.iter().all(|c| a.runs_of(c).len() == 1), "every card ran once");
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn the_runs_at_once_limit_makes_cards_wait_without_holding_them() {
    let a = app();
    gizai_core::settings::set(&a.st.db, "max_concurrent_runs", &1u32).unwrap();
    let cards: Vec<String> = (0..2).map(|_| a.card(2, "FAKE_HANG")).collect();
    a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [cards[0].clone()]);
    let t = a.task(&cards[1]);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("To do", None), "it only has to wait");
    let stopped = runs::live(&a.st).remove(0);
    runs::stop(&a.st, &stopped.run_id);
    until("the waiting card starts", || a.live_cards() == [cards[1].clone()]).await;
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn qa_takes_testing_cards_by_priority_as_its_slot_frees_up() {
    let a = app();
    let low = a.card(4, "FAKE_HANG");
    let urgent = a.card(1, "FAKE_HANG");
    for t in [&low, &urgent] {
        a.to(t, "Testing");
    }
    a.agent_with("QA Agent", "qa", "on_assign", 1, None);
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [urgent.clone()]);
    assert_eq!(a.column(&urgent), "Testing", "QA's card stays in Testing");
    let stopped = runs::live(&a.st).remove(0);
    runs::stop(&a.st, &stopped.run_id);
    until("the low card starts", || a.live_cards() == [low.clone()]).await;
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_wrong_model_holds_the_first_card_it_tries_and_the_others_stay_in_to_do() {
    let a = app();
    let high = a.card(2, "FAKE_HANG");
    let urgent = a.card(1, "FAKE_HANG");
    let low = a.card(4, "FAKE_HANG");
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 3, Some("opuss"));
    assert!(runs::pull(&a.st).await.is_empty());
    let t = a.task(&urgent);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count), ("To do", Some("blocked"), 0));
    assert!(t.hold_reason.as_deref().unwrap_or("").contains("no model called opuss"), "{:?}", t.hold_reason);
    for other in [&high, &low] {
        let t = a.task(other);
        assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count), ("To do", None, 0));
        assert!(a.runs_of(other).is_empty());
    }
    assert!(runs::pull_paused(&a.st, &be).is_some());
    assert!(runs::pull(&a.st).await.is_empty(), "the agent takes no more cards");
    assert_eq!(a.task(&high).hold, None);
    // a person's Run says what is wrong
    let err = runs::start(&a.st, &high, Some(be.clone()), None, "manual").await.map(|_| ()).unwrap_err();
    assert!(err.contains("no model called opuss"), "{err}");
    // the person fixes the model (editing the agent ends the pause): the best card that isn't held starts
    a.agent_with("Backend Agent", "backend", "on_assign", 1, Some("opus"));
    runs::resume_pull(&a.st, &be);
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [high.clone()]);
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_claude_that_is_not_logged_in_holds_one_card_and_the_others_stay_in_to_do() {
    let a = app();
    let cards: Vec<String> = [1, 2, 3, 4].iter().map(|p| a.card(*p, "FAKE_NOT_LOGGED_IN")).collect();
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    runs::pull(&a.st).await;
    until("the runs end", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let t = a.task(&cards[0]);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count), ("To do", Some("blocked"), 0), "back where it was, blocked");
    assert!(t.hold_reason.as_deref().unwrap_or("").contains("Not logged in"), "{:?}", t.hold_reason);
    assert!(runs::pull_paused(&a.st, &be).is_some());
    let held: Vec<usize> = (0..4).filter(|i| a.task(&cards[*i]).hold.is_some()).collect();
    assert_eq!(held, [0], "one missing login holds one card");
    for c in &cards[1..] {
        let t = a.task(c);
        assert_eq!((t.state_name.as_str(), t.fail_count), ("To do", 0));
    }
}

#[tokio::test]
async fn a_crashing_cli_pauses_the_queue_instead_of_running_through_it() {
    let a = app();
    let cards: Vec<String> = [1, 2, 3, 4, 4].iter().map(|p| a.card(*p, "FAKE_CRASH")).collect();
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 1, None);
    runs::pull(&a.st).await;
    until("the run ends", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(runs::pull_paused(&a.st, &be).is_some());
    let t = a.task(&cards[0]);
    assert_eq!((t.state_name.as_str(), t.fail_count), ("In progress", 1), "a failed run, as before");
    for c in &cards[1..] {
        assert_eq!((a.column(c).as_str(), a.runs_of(c).len()), ("To do", 0));
    }
    // a person's Run ends the pause
    gizai_core::settings::set(&a.st.db, "claude_bin", &FAKE.to_string()).unwrap();
    a.patch(&cards[1], TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() });
    let (_run, _done) = runs::start(&a.st, &cards[1], Some(be.clone()), None, "manual").await.unwrap();
    assert!(runs::pull_paused(&a.st, &be).is_none());
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_crashing_cli_with_three_cards_at_once_does_not_run_through_the_queue() {
    let a = app();
    let cards: Vec<String> = [1, 1, 2, 2, 3, 3, 4, 4].iter().map(|p| a.card(*p, "FAKE_CRASH")).collect();
    a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    runs::pull(&a.st).await;
    until("the runs end", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let ran: Vec<usize> = (0..cards.len()).filter(|i| !a.runs_of(&cards[*i]).is_empty()).collect();
    assert!(ran.len() <= 3, "at most its cards at once fail before the pull stops; these cards ran: {ran:?}");
    let waiting = cards.iter().filter(|c| a.column(c) == "To do" && a.runs_of(c).is_empty()).count();
    assert!(waiting >= 5, "the rest wait in To do; cards that ran: {ran:?}");
}

#[tokio::test]
async fn a_missing_login_with_three_runs_under_way_holds_one_card_and_the_others_wait_again() {
    let a = app();
    gizai_core::settings::set(&a.st.db, "claude_bin", &slow_fake(a.tmp.path(), 1.5)).unwrap();
    let cards: Vec<String> = [1, 2, 3, 4, 0].iter().map(|p| a.card(*p, "FAKE_NOT_LOGGED_IN")).collect();
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    let started = runs::pull(&a.st).await;
    assert_eq!(started.len(), 3, "three runs are under way before the first one fails");
    for (_, done) in started {
        done.await.unwrap();
    }
    until("the runs end", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(runs::pull_paused(&a.st, &be).is_some());
    let held: Vec<usize> = (0..5).filter(|i| a.task(&cards[*i]).hold.is_some()).collect();
    assert_eq!(held.len(), 1, "one missing login holds one card; held: {held:?}");
    let h = a.task(&cards[held[0]]);
    assert_eq!(h.hold.as_deref(), Some("blocked"));
    assert!(h.hold_reason.as_deref().unwrap_or("").contains("Not logged in"), "{:?}", h.hold_reason);
    for (i, c) in cards.iter().enumerate() {
        let t = a.task(c);
        assert_eq!((t.state_name.as_str(), t.fail_count), ("To do", 0), "card {i}: back in To do, not a failed run");
    }
    let ran: Vec<usize> = (0..5).filter(|i| !a.runs_of(&cards[*i]).is_empty()).collect();
    assert_eq!(ran, [0, 1, 2], "the three best cards were tried, once each");
    for i in &ran {
        let r = a.runs_of(&cards[*i]);
        assert_eq!((r.len(), r[0].status.as_str()), (1, "failed"), "card {i}");
    }
    assert!(runs::pull(&a.st).await.is_empty(), "the agent takes no more cards");
    // the person logs in and edits the agent: the cards that only went back to waiting are taken again, by priority
    gizai_core::settings::set(&a.st.db, "claude_bin", &FAKE.to_string()).unwrap();
    for c in &cards {
        a.patch(c, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() });
    }
    runs::resume_pull(&a.st, &be);
    runs::pull(&a.st).await;
    let best: Vec<String> = (0..5).filter(|i| !held.contains(i)).take(3).map(|i| cards[i].clone()).collect();
    assert_eq!(a.live_cards(), sorted(best));
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_crash_with_three_runs_under_way_fails_those_three_and_the_rest_wait_in_to_do() {
    let a = app();
    gizai_core::settings::set(&a.st.db, "claude_bin", &slow_fake(a.tmp.path(), 1.5)).unwrap();
    let cards: Vec<String> = [1, 1, 2, 2, 3, 3, 4, 0].iter().map(|p| a.card(*p, "FAKE_CRASH")).collect();
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    let started = runs::pull(&a.st).await;
    assert_eq!(started.len(), 3);
    for (_, done) in started {
        done.await.unwrap();
    }
    // the pulls the runs' ends start
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(runs::live(&a.st).is_empty(), "nothing else started");
    assert!(runs::pull_paused(&a.st, &be).is_some());
    for (i, c) in cards.iter().enumerate() {
        let t = a.task(c);
        let want = if i < 3 { ("In progress", 1, 1) } else { ("To do", 0, 0) };
        assert_eq!((t.state_name.as_str(), t.fail_count, a.runs_of(c).len()), want, "card {i}");
        assert_eq!(t.hold, None, "card {i}");
    }
}

#[tokio::test]
async fn an_agent_set_to_heartbeat_without_a_login_holds_one_card_and_the_next_pull_starts_nothing() {
    // GA-49: the worker heartbeat is gone and an agent's wake-up no longer matters: the queue takes its cards.
    let a = app();
    gizai_core::settings::set(&a.st.db, "claude_bin", &slow_fake(a.tmp.path(), 1.5)).unwrap();
    let cards: Vec<String> = [1, 2, 3, 4].iter().map(|p| a.card(*p, "FAKE_NOT_LOGGED_IN")).collect();
    let be = a.agent_with("Backend Agent", "backend", "heartbeat", 3, None);
    let started = runs::pull(&a.st).await;
    assert_eq!(started.len(), 3, "three runs are under way before the first one fails");
    for (_, done) in started {
        done.await.unwrap();
    }
    until("the runs end", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(runs::pull_paused(&a.st, &be).is_some());
    let held: Vec<usize> = (0..4).filter(|i| a.task(&cards[*i]).hold.is_some()).collect();
    assert_eq!(held.len(), 1, "one missing login holds one card; held: {held:?}");
    for (i, c) in cards.iter().enumerate() {
        let t = a.task(c);
        assert_eq!((t.state_name.as_str(), t.fail_count), ("To do", 0), "card {i}");
    }
    assert!(runs::pull(&a.st).await.is_empty(), "the next pull starts nothing");
    assert!(a.runs_of(&cards[3]).is_empty());
}

#[tokio::test]
async fn a_persons_run_without_a_login_holds_its_card_where_it_was_also_while_the_pull_is_paused() {
    let a = app();
    let cards: Vec<String> = [1, 2, 3].iter().map(|p| a.card(*p, "FAKE_NOT_LOGGED_IN")).collect();
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 1, None);
    runs::pull(&a.st).await;
    until("the run ends", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(a.task(&cards[0]).hold.as_deref(), Some("blocked"));
    assert!(runs::pull_paused(&a.st, &be).is_some());
    // a person's Run on a To do card, and on a Backlog card: each says what is wrong and is held where it was
    a.to(&cards[2], "Backlog");
    for (c, column) in [(&cards[1], "To do"), (&cards[2], "Backlog")] {
        let s = runs::run_once(&a.st, c, Some(be.clone()), None).await.unwrap();
        assert_eq!(s.status, "failed");
        assert!(s.error.as_deref().unwrap_or("").contains("Not logged in"), "{:?}", s.error);
        let t = a.task(c);
        assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count), (column, Some("blocked"), 0));
        assert!(t.hold_reason.as_deref().unwrap_or("").contains("Not logged in"), "{:?}", t.hold_reason);
        assert!(runs::pull_paused(&a.st, &be).is_some(), "the agent stops taking cards again");
    }
}

#[tokio::test]
async fn a_card_dragged_into_to_do_while_its_agent_is_paused_waits_unheld_until_the_team_lead_fixes_the_agent() {
    let a = app();
    let first = a.card(2, "FAKE_HANG");
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 3, Some("opuss"));
    let lead = a.agent_with("Team Lead", "lead", "manual", 1, None);
    assert!(runs::pull(&a.st).await.is_empty());
    assert_eq!(a.task(&first).hold.as_deref(), Some("blocked"));
    assert!(runs::pull_paused(&a.st, &be).is_some());
    // an urgent card dragged in now (a drag wakes the queue) only waits: no run, no hold, no failure
    let fresh = a.card(1, "FAKE_HANG");
    assert_eq!(runs::dispatch(&a.st, &fresh).await, None);
    let t = a.task(&fresh);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count), ("To do", None, 0));
    assert!(a.runs_of(&fresh).is_empty());
    // the Team Lead fixes the model from chat: the pause ends and the next pull (at most a minute) takes the waiting card
    gizai_lib::tools::call(&a.st, &lead, "update_agent", serde_json::json!({"agent": "Backend Agent", "model": "opus"})).await.unwrap();
    assert!(runs::pull_paused(&a.st, &be).is_none());
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [fresh.clone()], "the held card stays held");
    assert_eq!(a.column(&fresh), "In progress");
    assert_eq!((a.column(&first).as_str(), a.task(&first).hold.as_deref()), ("To do", Some("blocked")));
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn agents_paused_in_settings_or_a_paused_agent_keep_the_queue_waiting_without_holds() {
    let a = app();
    let cards: Vec<String> = [1, 2].iter().map(|p| a.card(*p, "FAKE_HANG")).collect();
    let be = a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    gizai_core::settings::set(&a.st.db, "agents_paused", &true).unwrap();
    assert_eq!(runs::dispatch(&a.st, &cards[0]).await, None);
    assert!(runs::pull(&a.st).await.is_empty());
    gizai_core::settings::set(&a.st.db, "agents_paused", &false).unwrap();
    gizai_core::team::set_agent_status(&a.st.db, &a.st.you_id, &be, "paused").unwrap();
    assert!(runs::pull(&a.st).await.is_empty());
    for c in &cards {
        let t = a.task(c);
        assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count, a.runs_of(c).len()), ("To do", None, 0, 0));
    }
    assert!(runs::pull_paused(&a.st, &be).is_none(), "only waiting: the agent's pull isn't paused");
    gizai_core::team::set_agent_status(&a.st.db, &a.st.you_id, &be, "active").unwrap();
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), sorted(cards.clone()));
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_crash_on_an_agent_set_to_heartbeat_fails_at_most_its_cards_at_once_and_the_next_pull_starts_nothing() {
    // GA-49: the worker heartbeat is gone and an agent's wake-up no longer matters: the queue takes its cards.
    let a = app();
    gizai_core::settings::set(&a.st.db, "claude_bin", &slow_fake(a.tmp.path(), 1.0)).unwrap();
    let cards: Vec<String> = [1, 2, 3, 4, 0, 0].iter().map(|p| a.card(*p, "FAKE_CRASH")).collect();
    let be = a.agent_with("Backend Agent", "backend", "heartbeat", 3, None);
    let lead = a.agent_with("Team Lead", "lead", "manual", 1, None);
    let started = runs::pull(&a.st).await;
    assert_eq!(started.len(), 3);
    for (_, done) in started {
        done.await.unwrap();
    }
    until("the runs end", || runs::live(&a.st).is_empty()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(runs::pull_paused(&a.st, &be).is_some());
    for (i, c) in cards.iter().enumerate() {
        let t = a.task(c);
        let want = if i < 3 { ("In progress", 1, 1) } else { ("To do", 0, 0) };
        assert_eq!((t.state_name.as_str(), t.fail_count, a.runs_of(c).len()), want, "card {i}");
        assert_eq!(t.hold, None, "card {i}");
    }
    assert!(runs::pull(&a.st).await.is_empty(), "the next pull starts nothing");
    // the Team Lead sets the agent active again: the pause ends
    gizai_lib::tools::call(&a.st, &lead, "set_agent_status", serde_json::json!({"agent": "Backend Agent", "status": "active"})).await.unwrap();
    assert!(runs::pull_paused(&a.st, &be).is_none());
}

// ---- Part 2: the Testing switch ----

#[tokio::test]
async fn a_small_fix_with_testing_off_ends_in_review_and_with_testing_on_qa_starts() {
    let a = app();
    let fix = a.card(2, "Fix the typo");
    a.patch(&fix, TaskPatch { testing: Some(false), ..Default::default() });
    let qa = a.agent_with("QA Agent", "qa", "on_assign", 3, None);
    a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    runs::dispatch(&a.st, &fix).await.expect("picked up from To do");
    until("the run ends", || a.runs_of(&fix).iter().all(|r| r.status != "running" && r.status != "queued")).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let t = a.task(&fix);
    assert_eq!((t.state_name.as_str(), t.assignee_id.as_deref()), ("Review", Some(a.st.you_id.as_str())));
    let ran = a.runs_of(&fix);
    assert_eq!(ran.len(), 1, "no QA run");
    assert_eq!(ran[0].outcome.as_deref(), Some("ready_for_testing"));
    assert!(gizai_core::comments::list(&a.st.db, &fix).unwrap().iter().any(|c| c.body_md.contains("Exporter added")));
    // the same with Testing on: Testing, and the QA Agent starts
    let full = a.card(2, "Export");
    runs::dispatch(&a.st, &full).await.expect("picked up from To do");
    until("QA starts", || a.runs_of(&full).iter().any(|r| r.agent_id == qa)).await;
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

// ---- 3.4 and Part 3: DevOps runs and the Deploy column ----

#[tokio::test]
async fn nothing_starts_on_a_deploy_card_even_for_a_devops_agent_on_it_whatever_its_wake_up() {
    // GA-49: Deploy is in the seed, Manual, with the DevOps Agent on it (its role's usual column).
    let a = app();
    let t = a.card(1, "FAKE_HANG");
    let ops = a.agent_with("DevOps Agent", "devops", "on_assign", 3, None);
    a.agent_with("Backend Agent", "backend", "on_assign", 3, None);
    a.to(&t, "Deploy");
    a.patch(&t, TaskPatch { assignee_id: Some(ops.clone()), ..Default::default() });
    assert_eq!(runs::dispatch(&a.st, &t).await, None, "a drag or an assignment starts nothing");
    assert!(runs::pull(&a.st).await.is_empty(), "the queue skips it");
    a.agent_with("DevOps Agent", "devops", "heartbeat", 3, None);
    assert!(runs::pull(&a.st).await.is_empty(), "and when its wake-up is heartbeat");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.runs_of(&t).is_empty());
    assert_eq!(a.column(&t), "Deploy");
}

#[tokio::test]
async fn run_on_a_deploy_card_starts_the_devops_agent_and_the_card_stays_in_deploy() {
    let a = app();
    let t = a.card(0, "FAKE_HANG");
    a.unassign(&t);
    a.to(&t, "Deploy");
    // without an agent on Deploy, the message says to put one on it
    let err = runs::start(&a.st, &t, None, Some(FAKE.into()), "manual").await.map(|_| ()).unwrap_err();
    assert!(err.contains("drag an agent onto Deploy") && err.contains("Team page"), "{err}");
    let ops = a.agent_with("DevOps Agent", "devops", "manual", 3, None);
    assert_eq!(runs::suggest(&a.st, &t).as_deref(), Some(ops.as_str()));
    let (run, done) = runs::start(&a.st, &t, None, Some(FAKE.into()), "manual").await.unwrap();
    assert_eq!(gizai_core::runs::get(&a.st.db, &run).unwrap().agent_id, ops);
    assert_eq!(a.column(&t), "Deploy", "it stays in Deploy while it runs");
    runs::stop(&a.st, &run);
    done.await.unwrap();
    assert_eq!(a.column(&t), "Deploy");
    // the card's agent assignee goes first, or the agent the person picks
    let be = a.agent("Backend Agent");
    a.patch(&t, TaskPatch { assignee_id: Some(be.clone()), ..Default::default() });
    assert_eq!(runs::suggest(&a.st, &t).as_deref(), Some(be.as_str()));
}

#[tokio::test]
async fn the_ga_40_case_a_devops_run_on_a_review_card_stays_in_review_and_deployed_ends_in_done() {
    let a = app();
    let t = a.card(0, "Export");
    let ops = a.agent_with("DevOps Agent", "devops", "manual", 3, None);
    let be = a.agent("Backend Agent");
    runs::run_once(&a.st, &t, Some(be.clone()), Some(FAKE.into())).await.unwrap(); // built: Testing
    a.to(&t, "Review"); // QA passed it
    a.patch(&t, TaskPatch { assignee_id: Some(a.st.you_id.clone()), ..Default::default() });
    let qa = a.agent_with("QA Agent", "qa", "on_assign", 3, None);
    // fix the pull request's merge conflicts
    let s = runs::run_once(&a.st, &t, Some(ops.clone()), Some(FAKE.into())).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("ready_for_testing"));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let task = a.task(&t);
    assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref()), ("Review", Some(a.st.you_id.as_str())));
    assert!(!a.runs_of(&t).iter().any(|r| r.agent_id == qa), "the QA Agent doesn't start");
    assert_eq!(a.implementer(&t).as_deref(), Some(be.as_str()), "the implementer stays the Backend Agent");
    // the pull request is merged: Deploy, and nothing starts
    gizai_core::pulls::merged(&a.st.db, &a.st.you_id, &t, "https://github.com/acme/kade/pull/12").unwrap();
    assert_eq!(a.column(&t), "Deploy");
    assert!(runs::pull(&a.st).await.is_empty());
    // Run with the DevOps Agent: deployed moves it to Done
    let s = runs::run_once(&a.st, &t, None, Some(fake_answering(a.tmp.path(), "deployed"))).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("deployed"));
    assert_eq!(gizai_core::runs::get(&a.st.db, &s.run_id).unwrap().agent_id, ops);
    assert_eq!(a.column(&t), "Done");
}

#[tokio::test]
async fn a_devops_run_on_a_deploy_card_that_needs_a_decision_stays_in_deploy_on_hold() {
    let a = app();
    let ops = a.agent_with("DevOps Agent", "devops", "manual", 3, None);
    let t = a.card(0, "Release");
    a.unassign(&t);
    a.to(&t, "Deploy");
    let s = runs::run_once(&a.st, &t, None, Some(fake_answering(a.tmp.path(), "needs_decision"))).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("needs_decision"));
    assert_eq!(gizai_core::runs::get(&a.st.db, &s.run_id).unwrap().agent_id, ops);
    let task = a.task(&t);
    assert_eq!((task.state_name.as_str(), task.hold.as_deref()), ("Deploy", Some("needs_decision")));
    // ready_for_testing (it only checked something): Deploy, no hold, no QA
    let t = a.card(0, "Check the release");
    a.unassign(&t);
    a.to(&t, "Deploy");
    a.agent_with("QA Agent", "qa", "on_assign", 3, None);
    let s = runs::run_once(&a.st, &t, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(gizai_core::runs::get(&a.st.db, &s.run_id).unwrap().agent_id, ops);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let task = a.task(&t);
    assert_eq!((task.state_name.as_str(), task.hold.as_deref(), a.runs_of(&t).len()), ("Deploy", None, 1));
}
