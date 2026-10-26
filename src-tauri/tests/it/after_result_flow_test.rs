//! GA-221 end to end with the fake `claude`: a run whose CLI doesn't exit after the agent's final message, as Claude Code
//! while something the run started still runs in the background, is ended once it has been quiet for `after_result` (a
//! few seconds here, `set_after_result`; 3 minutes in Gizai) and keeps its result: succeeded, its outcome applied (the
//! card moves), its Learned lines saved, and a note that says why. An error result or a cap that comes first ends it as
//! before, a CLI that exits on its own changes nothing, and Stop still ends it at once. The leftovers note:
//! run_leftovers_flow_test.rs; a chat answer: chat_flow_test.rs.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_agents::stream::RunEvent;
use gizai_core::runs as core_runs;
use gizai_lib::{AppState, runs};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

/// The note on a run Gizai ended after the agent's result, word for word.
const NOTE: &str = "Claude Code hadn't exited 3 minutes after the agent's final message, so Gizai ended it and kept the result. \
                    Something the run started was probably still running in the background, such as a server.";

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

struct App { st: AppState, repo: PathBuf, _tmp: tempfile::TempDir }

/// A test Gizai whose runs' CLIs may stay quiet for `wait` once the agent is done (None: Gizai's own 3 minutes). Only the
/// runs a test starts run.
fn app(wait: Option<Duration>) -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    if let Some(w) = wait {
        st.runs.set_after_result(w);
    }
    let repo = git_repo(tmp.path());
    App { st, repo, _tmp: tmp }
}

impl App {
    fn card(&self, description: &str) -> String {
        let t = gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend");
        gizai_core::tasks::update(&self.st.db, &self.st.you_id, &t, gizai_core::model::TaskPatch { description_md: Some(description.into()),
            ..Default::default() }).unwrap();
        t
    }
    fn column(&self, task: &str) -> String {
        gizai_core::tasks::get(&self.st.db, task).unwrap().state_name
    }
}

fn notes(st: &AppState, run_id: &str) -> Vec<String> {
    runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

#[tokio::test]
async fn a_run_whose_cli_stays_quiet_after_its_result_succeeds_with_its_outcome_its_learned_lines_and_the_note() {
    let a = app(Some(Duration::from_secs(2)));
    let task = a.card("Export invoices as CSV. FAKE_LEARNED FAKE_RESULT_THEN_HANG");
    let t0 = Instant::now();
    let s = tokio::time::timeout(Duration::from_secs(60), runs::run_once(&a.st, &task, None, None)).await.expect("ended well before 3 minutes").unwrap();
    assert!(t0.elapsed() >= Duration::from_secs(2), "{:?}", t0.elapsed());
    assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None), "{s:?}");
    assert_eq!(a.column(&task), "Testing", "the card moves as for any finished run");
    let run = core_runs::get(&a.st.db, &s.run_id).unwrap();
    assert_eq!((run.cost_usd_micros, run.input_tokens, run.output_tokens), (420_000, 38_000, 4_100), "from its result line");
    let said = notes(&a.st, &s.run_id);
    assert_eq!(said.iter().filter(|n| *n == NOTE).count(), 1, "{said:?}");
    // in the run's log too, where Show output reads it later
    assert!(std::fs::read_to_string(&run.log_path).unwrap().contains(NOTE));
    // its Learned lines: in the agent's note for the card's project (GA-172)
    let kade = gizai_core::memory::find(&a.st.db, "Agents/Backend Agent/KADE").unwrap().expect("its Learned lines were saved");
    for line in ["Excel NL needs semicolons in a CSV.", "The exporter tests need --filter=InvoiceExport."] {
        assert!(kade.body_md.contains(line), "{}", kade.body_md);
    }
    assert!(runs::live(&a.st).is_empty());
}

#[tokio::test]
async fn without_a_result_line_the_final_text_with_its_gizai_result_line_is_the_result() {
    let a = app(Some(Duration::from_secs(2)));
    let task = a.card("FAKE_TEXT_THEN_HANG");
    let s = tokio::time::timeout(Duration::from_secs(60), runs::run_once(&a.st, &task, None, None)).await.expect("ended").unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None), "{s:?}");
    assert_eq!(a.column(&task), "Testing");
    let run = core_runs::get(&a.st.db, &s.run_id).unwrap();
    assert_eq!((run.cost_usd_micros, run.input_tokens, run.output_tokens), (0, 0, 0), "no result line: no cost");
    assert_eq!(notes(&a.st, &s.run_id).iter().filter(|n| *n == NOTE).count(), 1, "{:?}", notes(&a.st, &s.run_id));
}

#[tokio::test]
async fn a_run_whose_result_is_an_error_fails_with_that_error_and_gets_no_note() {
    let a = app(Some(Duration::from_secs(1)));
    let task = a.card("FAKE_ERROR_THEN_HANG");
    let s = tokio::time::timeout(Duration::from_secs(60), runs::run_once(&a.st, &task, None, None)).await.expect("ended").unwrap();
    assert_eq!(s.status, "failed", "{s:?}");
    assert!(s.error.as_deref().is_some_and(|e| e.contains("API Error: 500 the fake failed")), "{s:?}");
    assert!(!notes(&a.st, &s.run_id).iter().any(|n| n == NOTE), "{:?}", notes(&a.st, &s.run_id));
    assert_eq!(a.column(&task), "In progress", "a failed run leaves the card where the start put it");
}

#[tokio::test]
async fn a_run_that_hits_the_tool_call_cap_first_is_timed_out_as_before() {
    let a = app(Some(Duration::from_secs(1)));
    gizai_core::settings::set(&a.st.db, "max_run_tool_calls", &1u32).unwrap();
    let task = a.card("FAKE_RESULT_THEN_HANG");
    let s = tokio::time::timeout(Duration::from_secs(60), runs::run_once(&a.st, &task, None, None)).await.expect("ended").unwrap();
    assert_eq!(s.status, "timed_out", "{s:?}");
    assert_eq!(s.error.as_deref(), Some("stopped at the limit of 1 tool calls per run (Settings → Runs)"));
    assert!(!notes(&a.st, &s.run_id).iter().any(|n| n == NOTE));
}

#[tokio::test]
async fn a_run_that_exits_right_after_its_result_changes_in_nothing() {
    // the same card with a 1 s wait and with Gizai's own 3 minutes: the same status, outcome, cost, events and notes
    let mut seen = vec![];
    for wait in [Some(Duration::from_secs(1)), None] {
        let a = app(wait);
        let task = a.card("Export invoices as CSV.");
        let s = runs::run_once(&a.st, &task, None, None).await.unwrap();
        assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None), "{s:?}");
        assert_eq!(a.column(&task), "Testing");
        let run = core_runs::get(&a.st.db, &s.run_id).unwrap();
        let events: Vec<RunEvent> = runs::events_for(&a.st, &s.run_id).into_iter().map(|e| e.event).collect();
        assert!(!notes(&a.st, &s.run_id).iter().any(|n| n == NOTE));
        seen.push((run.cost_usd_micros, run.input_tokens, events));
    }
    assert_eq!(seen[0], seen[1]);
    assert_eq!(seen[0].2.len(), 7, "6 log lines → 7 events, no note: {:?}", seen[0].2);
}

#[tokio::test]
async fn stop_ends_a_run_at_once_while_it_waits_after_the_result() {
    let a = app(Some(Duration::from_secs(60)));
    let task = a.card("FAKE_RESULT_THEN_HANG");
    let (run, done) = runs::start(&a.st, &task, None, None, "manual").await.unwrap();
    let t0 = Instant::now();
    while !runs::events_for(&a.st, &run).iter().any(|e| matches!(e.event, RunEvent::Result { .. })) {
        assert!(t0.elapsed() < Duration::from_secs(20), "its result line never came");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let t0 = Instant::now();
    runs::stop(&a.st, &run);
    let s = tokio::time::timeout(Duration::from_secs(20), done).await.expect("Stop ended it").unwrap();
    assert!(t0.elapsed() < Duration::from_secs(10), "not after the 60 s wait: {:?}", t0.elapsed());
    assert_eq!(s.status, "cancelled", "{s:?}");
    assert!(!notes(&a.st, &run).iter().any(|n| n == NOTE));
    assert_eq!(a.column(&task), "In progress");
}
