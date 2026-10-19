//! GA-54: a run that ends normally without its GIZAI_RESULT line is continued once by itself, in the same session, with
//! the nudge message; when that run also ends without one, the card is held stalled and no third run starts. No nudge
//! after Stop, a limit, a failed run or Gizai quitting, nor when a start isn't allowed now. Runs use the fake Claude Code
//! (FAKE_NO_RESULT makes it end its message waiting for CI, FAKE_GATE holds it until a file exists), never the real one.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, Run, TaskPatch};

const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent on the CLI `cli` and returns its id.
fn put_agent_on(st: &gizai_lib::AppState, cli: &str, extra: AgentInput) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..extra }).unwrap();
    agent.actor_id
}

fn describe(st: &gizai_lib::AppState, task: &str, text: &str) {
    gizai_core::tasks::update(&st.db, &st.you_id, task, TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

/// Makes In progress Manual: a run that ends there without a result (or at a limit) would otherwise be started afresh by
/// the queue, which isn't the nudge these tests look for.
fn in_progress_manual(st: &gizai_lib::AppState) {
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let state = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &state, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
}

fn stderr_of(run: &Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

/// The prompt the fake CLI was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &Run) -> String {
    let err = stderr_of(run);
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

/// The card's runs, oldest first.
fn runs_of(st: &gizai_lib::AppState, task: &str) -> Vec<Run> {
    let mut runs = gizai_core::runs::list_for_task(&st.db, task).unwrap();
    runs.sort_by_key(|r| r.created_at);
    runs
}

/// Waits until the card has `n` runs and none of them is live.
async fn wait_for_runs(st: &gizai_lib::AppState, task: &str, n: usize) -> Vec<Run> {
    let t0 = Instant::now();
    loop {
        let runs = runs_of(st, task);
        if runs.len() >= n && gizai_lib::runs::live(st).iter().all(|l| l.task_id != task)
            && runs.iter().all(|r| !matches!(r.status.as_str(), "queued" | "running" | "waiting_approval")) {
            return runs;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "{n} runs expected: {:?}", runs.iter().map(|r| (&r.trigger, &r.status)).collect::<Vec<_>>());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Gives a nudge time to start (it would start at once), then says that none did: the card has only its one run.
async fn assert_no_nudge(st: &gizai_lib::AppState, task: &str, why: &str) -> Run {
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let runs = runs_of(st, task);
    assert_eq!(runs.len(), 1, "{why}: no nudge: {:?}", runs.iter().map(|r| (&r.trigger, &r.status)).collect::<Vec<_>>());
    assert!(gizai_lib::runs::live(st).iter().all(|l| l.task_id != task), "{why}: nothing live on the card");
    runs.into_iter().next().unwrap()
}

/// A run that ended normally without a result, where nothing else changed: the card stays in In progress, not held,
/// with one failure counted.
fn assert_left_as_it_was(st: &gizai_lib::AppState, task: &str, run: &Run, why: &str) {
    assert_eq!((run.status.as_str(), run.outcome.as_deref(), run.nudged), ("succeeded", Some("no_result"), false), "{why}: {:?}", run.error);
    let t = gizai_core::tasks::get(&st.db, task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.fail_count), ("In progress", None, 1), "{why}");
}

fn touch(p: &Path) {
    std::fs::write(p, "").unwrap();
}

#[tokio::test]
async fn a_run_without_its_result_is_continued_once_in_its_session_then_the_card_is_held_stalled() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    // every run of this CLI ends its message waiting for CI, the nudged one too
    let cli = add_cli(&st, "Claude Code (waits)", "claude_code", FAKE_CLAUDE, &["FAKE_NO_RESULT=1", "FAKE_TEMP=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &cli, AgentInput::default());
    describe(&st, &task, "Release Otus v1.11.0.");

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", None), "{:?}", s.error);
    let runs = wait_for_runs(&st, &task, 2).await;
    assert_eq!(runs.len(), 2);
    let (first, nudged) = (&runs[0], &runs[1]);
    assert_eq!((first.trigger.as_str(), first.outcome.as_deref(), first.nudged), ("manual", Some("no_result"), false));
    // GA-31: the nudge has its own trigger in the Runs list (a Continue is `nudge`)
    assert_eq!((nudged.trigger.as_str(), nudged.status.as_str(), nudged.outcome.as_deref(), nudged.nudged),
               ("result_nudge", "succeeded", Some("no_result"), true), "{:?}", nudged.error);
    // the same session, worktree, branch and agent, resumed
    let session = first.session_id.clone().unwrap();
    assert_eq!(nudged.session_id.as_deref(), Some(session.as_str()));
    assert_eq!((&nudged.worktree_path, &nudged.branch, &nudged.agent_id), (&first.worktree_path, &first.branch, &first.agent_id));
    assert!(stderr_of(nudged).contains(&format!("--resume {session}")), "{}", stderr_of(nudged));
    assert!(!stderr_of(first).contains("--resume"), "{}", stderr_of(first));
    assert_ne!(nudged.log_path, first.log_path, "each run shows only its own output");

    // the nudge message, then the limits and How this run works with the waiting rule; not the whole task again
    let p = prompt_of(nudged);
    for want in ["Your run ended without your GIZAI_RESULT line, and nothing wakes you up later.", "If you were waiting for something",
                 "check it in the foreground now", "End with your GIZAI_RESULT line.", "## Limits of this run", "## How this run works",
                 "Ending your message ends the run", "`<check>; sleep 45`", "`sleep`"] {
        assert!(p.contains(want), "{want} missing in the nudge's prompt: {p}");
    }
    assert!(p.find("## Limits of this run").unwrap() < p.find("## How this run works").unwrap(), "{p}");
    assert!(!p.contains("Release Otus v1.11.0."), "the short nudge prompt, not the task: {p}");
    // the first run's prompt has the rule too, and sleep is in Gizai's default list
    let p1 = prompt_of(first);
    assert!(p1.contains("Release Otus v1.11.0.") && p1.contains("Ending your message ends the run") && p1.contains("`sleep`"), "{p1}");

    // the second run in a row without a result: held stalled with the reason, in the Inbox
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.hold_reason.as_deref()),
               ("In progress", Some("stalled"), Some(gizai_core::workflow::NUDGE_STALLED)));
    assert!(gizai_core::tasks::needs_you(&st.db, &st.you_id).unwrap().iter().any(|n| n.identifier == t.identifier), "in the Inbox");
    // and no third run: not by itself, not by the queue
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(gizai_lib::runs::pull(&st).await.is_empty());
    assert_eq!(runs_of(&st, &task).len(), 2, "no third run");
    assert!(gizai_lib::runs::live(&st).is_empty());
}

#[tokio::test]
async fn a_nudged_run_that_ends_with_its_result_moves_the_card_on() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let cli = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &cli, AgentInput::default());
    // only the first prompt (the task's) ends without a result; the nudge's doesn't have the card's words
    describe(&st, &task, "FAKE_NO_RESULT");
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let runs = wait_for_runs(&st, &task, 2).await;
    assert_eq!(runs.iter().map(|r| (r.trigger.as_str(), r.outcome.as_deref().unwrap_or(""), r.nudged)).collect::<Vec<_>>(),
               [("manual", "no_result", false), ("result_nudge", "ready_for_testing", true)]);
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("Testing", None));
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert_eq!(runs_of(&st, &task).len(), 2);
}

#[tokio::test]
async fn no_nudge_after_stop_a_tool_call_limit_or_a_failed_run() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let cli = add_cli(&st, "Claude Code (waits)", "claude_code", FAKE_CLAUDE, &["FAKE_NO_RESULT=1"]);
    // Stop
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &cli, AgentInput::default());
    in_progress_manual(&st);
    describe(&st, &task, "FAKE_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    gizai_lib::runs::stop(&st, &run_id);
    assert_eq!(done.await.unwrap().status, "cancelled");
    assert_eq!(assert_no_nudge(&st, &task, "Stop").await.status, "cancelled");

    // the tool-call limit (the no-result run makes two tool calls)
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.status, "timed_out", "{:?}", s.error);
    assert!(s.error.as_deref().unwrap().contains("tool calls"), "{:?}", s.error);
    assert_eq!(assert_no_nudge(&st, &task, "a limit").await.status, "timed_out");
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &200u32).unwrap();

    // a failed run
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    describe(&st, &task, "FAKE_CRASH");
    assert_eq!(gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap().status, "failed");
    assert_eq!(assert_no_nudge(&st, &task, "a failure").await.status, "failed");
}

#[tokio::test]
async fn no_nudge_when_gizai_quits() {
    // the run ends normally while Gizai is quitting
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let gate = tmp.path().join("gate");
    let cli = add_cli(&st, "Claude Code (waits)", "claude_code", FAKE_CLAUDE, &["FAKE_NO_RESULT=1", &format!("FAKE_GATE={}", gate.display())]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &cli, AgentInput::default());
    let (_, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    gizai_lib::runs::mark_closing(&st);
    touch(&gate);
    let s = done.await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", None), "{:?}", s.error);
    assert_no_nudge(&st, &task, "quitting").await;

    // Gizai quits while the run is live: stopped because Gizai quit
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let cli = add_cli(&st, "Claude Code (waits)", "claude_code", FAKE_CLAUDE, &["FAKE_NO_RESULT=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &cli, AgentInput::default());
    describe(&st, &task, "FAKE_HANG");
    let (_, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(gizai_lib::runs::stop_all(&st, Duration::from_secs(12)).await, 1);
    let s = done.await.unwrap();
    assert_eq!((s.status.as_str(), s.error.as_deref()), ("cancelled", Some(gizai_lib::runs::STOPPED_BY_QUIT)));
    assert_no_nudge(&st, &task, "quit").await;
}

/// A state with a card for the backend agent, on a fake Claude Code that ends without a result once `gate` exists.
struct Waits {
    _tmp: tempfile::TempDir,
    st: gizai_lib::AppState,
    repo: PathBuf,
    gate: PathBuf,
    agent: String,
    task: String,
}

fn waits(extra: AgentInput) -> Waits {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let gate = tmp.path().join("gate");
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let cli = add_cli(&st, "Claude Code (waits)", "claude_code", FAKE_CLAUDE, &["FAKE_NO_RESULT=1", &format!("FAKE_GATE={}", gate.display())]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let agent = put_agent_on(&st, &cli, extra);
    in_progress_manual(&st);
    Waits { _tmp: tmp, st, repo, gate, agent, task }
}

#[tokio::test]
async fn no_nudge_when_the_agent_is_paused_agents_are_paused_or_its_budget_is_used() {
    // the agent is paused while its run is live
    let w = waits(AgentInput::default());
    let task = w.task.clone();
    let (_, done) = gizai_lib::runs::start(&w.st, &task, None, None, "manual").await.unwrap();
    gizai_core::team::set_agent_status(&w.st.db, &w.st.you_id, &w.agent, "paused").unwrap();
    touch(&w.gate);
    done.await.unwrap();
    let run = assert_no_nudge(&w.st, &task, "a paused agent").await;
    assert_left_as_it_was(&w.st, &task, &run, "a paused agent");

    // agents are paused in Settings
    let w = waits(AgentInput::default());
    touch(&w.gate);
    gizai_core::settings::set(&w.st.db, "agents_paused", &true).unwrap();
    let task = w.task.clone();
    gizai_lib::runs::run_once(&w.st, &task, None, None).await.unwrap();
    let run = assert_no_nudge(&w.st, &task, "agents paused").await;
    assert_left_as_it_was(&w.st, &task, &run, "agents paused");

    // the run used the rest of the agent's monthly budget ($0.12 of $0.10)
    let w = waits(AgentInput { budget_usd_micros: Some(100_000), ..Default::default() });
    touch(&w.gate);
    let task = w.task.clone();
    gizai_lib::runs::run_once(&w.st, &task, None, None).await.unwrap();
    let run = assert_no_nudge(&w.st, &task, "over budget").await;
    assert_eq!(run.cost_usd_micros, 120_000);
    assert_left_as_it_was(&w.st, &task, &run, "over budget");
}

#[tokio::test]
async fn no_nudge_when_runs_at_once_or_the_agents_cards_at_once_are_full() {
    // Runs at once: another agent's run takes the last place while this one ends
    let w = waits(AgentInput::default());
    gizai_core::settings::set(&w.st.db, "max_concurrent_runs", &2u32).unwrap();
    let task = w.task.clone();
    let (_, done) = gizai_lib::runs::start(&w.st, &task, None, None, "manual").await.unwrap();
    let other = gizai_lib::test_task(&w.st, w.repo.to_str().unwrap(), "frontend");
    describe(&w.st, &other, "FAKE_HANG");
    let (other_run, other_done) = gizai_lib::runs::start(&w.st, &other, None, None, "manual").await.unwrap();
    gizai_core::settings::set(&w.st.db, "max_concurrent_runs", &1u32).unwrap();
    touch(&w.gate);
    done.await.unwrap();
    let run = assert_no_nudge(&w.st, &task, "Runs at once").await;
    assert_left_as_it_was(&w.st, &task, &run, "Runs at once");
    gizai_lib::runs::stop(&w.st, &other_run);
    other_done.await.unwrap();
    assert_no_nudge(&w.st, &task, "Runs at once, after the other run").await;

    // the agent's cards at once (1): its run on another card is still live
    let w = waits(AgentInput { max_runs: Some(1), ..Default::default() });
    let task = w.task.clone();
    let (_, done) = gizai_lib::runs::start(&w.st, &task, None, None, "manual").await.unwrap();
    let other = gizai_lib::test_task(&w.st, w.repo.to_str().unwrap(), "backend");
    describe(&w.st, &other, "FAKE_HANG");
    let (other_run, other_done) = gizai_lib::runs::start(&w.st, &other, None, None, "manual").await.unwrap();
    touch(&w.gate);
    done.await.unwrap();
    let run = assert_no_nudge(&w.st, &task, "cards at once").await;
    assert_left_as_it_was(&w.st, &task, &run, "cards at once");
    gizai_lib::runs::stop(&w.st, &other_run);
    other_done.await.unwrap();
}

#[tokio::test]
async fn a_cli_that_cant_continue_a_session_is_not_nudged() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let other = add_cli(&st, "OpenCode", "other", FAKE_CLI, &["FAKE_KIND=other"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &other, AgentInput::default());
    in_progress_manual(&st);
    describe(&st, &task, "FAKE_NO_RESULT");
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let run = assert_no_nudge(&st, &task, "another CLI").await;
    assert_left_as_it_was(&st, &task, &run, "another CLI");
}
