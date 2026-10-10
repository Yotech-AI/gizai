//! GA-94 end to end with a fake `claude` (never the real one): an answer of the Team Lead (a chat answer, a board check)
//! stops at Minutes and Tool calls per chat answer in Settings → Runs, 15 and 60 unless changed, and says which limit
//! stopped it with the numbers that applied. An answer reads them as it starts: a change counts from the next answer.
//! Task runs keep the run limits. The fake makes as many tool calls as `FAKE_CALLS=<n>` in its prompt says, then answers;
//! `FAKE_GATE` waits for a fake-go file first (it writes fake-waiting), `FAKE_HANG` hangs until it is stopped.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_core::model::*;
use gizai_core::{chat, ids, projects, runs as core_runs, settings, tasks, team};
use gizai_lib::runs::{self, Settings};
use gizai_lib::{AppState, board, chat as app_chat, mcp};
use serde_json::json;

const RUN_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const MIN: i64 = 60_000;

/// The Team Lead's Claude Code: an init line, the tool calls `FAKE_CALLS=<n>` asks for, then a short answer.
const LEAD_FAKE: &str = r##"#!/usr/bin/env bash
here="$(cd "$(dirname "$0")" && pwd)"
sid=none
prev=""
for a in "$@"; do
  case "$prev" in --session-id|--resume) sid="$a" ;; esac
  prev="$a"
done
prompt="$(cat)"
trap 'exit 130' INT TERM
printf '{"type":"system","subtype":"init","session_id":"%s","model":"fake-model","tools":["Read"]}\n' "$sid"
case "$prompt" in *FAKE_GATE*)
  : > "$here/fake-waiting"
  n=0
  while [ ! -e "$here/fake-go" ] && [ $n -lt 300 ]; do sleep 0.1; n=$((n+1)); done ;;
esac
case "$prompt" in *FAKE_HANG*) sleep 60 & wait $!; exit 0 ;; esac
calls="$(printf '%s' "$prompt" | sed -n 's/.*FAKE_CALLS=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
calls="${calls:-0}"
n=0
while [ $n -lt "$calls" ]; do
  printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t%s","name":"Read","input":{"file_path":"/dev/null"}}]},"session_id":"%s"}\n' "$n" "$sid"
  n=$((n+1))
done
printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done after %s tool calls."}]},"session_id":"%s"}\n' "$calls" "$sid"
printf '{"type":"result","subtype":"success","is_error":false,"result":"Done after %s tool calls.","total_cost_usd":0.01,"num_turns":2,"session_id":"%s","usage":{"input_tokens":1000,"output_tokens":100}}\n' "$calls" "$sid"
"##;

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

/// Writes an executable script through a child `sh`, so this process never holds it open for writing ("Text file busy").
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| assert!(Command::new("git").args(args).current_dir(&repo).status().unwrap().success(), "git {args:?}");
    git(&["init", "-q", "-b", "main"]);
    git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"]);
    repo
}

struct T {
    st: AppState,
    lead: String,
    project: String,
    /// The fake's folder: fake-waiting and fake-go are here.
    fake_dir: PathBuf,
    _tmp: tempfile::TempDir,
    _server: tokio::task::JoinHandle<()>,
}

/// A Team Lead with Chat on that checks the board every 15 minutes, project KADE, the fake above as Claude Code.
async fn setup() -> T {
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    st.mcp_shim = Some(shim());
    let fake_dir = tmp.path().join("lead-cli");
    std::fs::create_dir_all(&fake_dir).unwrap();
    let fake = fake_dir.join("claude");
    write_script(&fake, LEAD_FAKE);
    settings::set(&st.db, "claude_bin", &fake.to_string_lossy().to_string()).unwrap();
    let project = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), board_check_minutes: Some(15), ..Default::default() }).unwrap();
    let server = mcp::start(&st).unwrap();
    T { st, lead, project, fake_dir, _tmp: tmp, _server: server }
}

impl T {
    /// Settings → Runs → Save settings with these chat answer limits, the rest as it is.
    fn chat_limits(&self, minutes: u64, tool_calls: u32) {
        let s = runs::get_settings(&self.st);
        runs::save_settings(&self.st, &Settings { max_chat_minutes: minutes, max_chat_tool_calls: tool_calls, ..s }).unwrap();
    }
    async fn answer(&self, thread: Option<String>, text: &str) -> (String, app_chat::TurnSummary) {
        let (id, done) = app_chat::send(&self.st, thread, text.into(), None).await.unwrap();
        (id, finished(done).await)
    }
    async fn until_waiting(&self) {
        let t0 = Instant::now();
        while !self.fake_dir.join("fake-waiting").exists() {
            assert!(t0.elapsed() < Duration::from_secs(20), "the fake never started waiting");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    fn go(&self) {
        std::fs::write(self.fake_dir.join("fake-go"), "").unwrap();
    }
    fn gate_closed(&self) {
        let _ = std::fs::remove_file(self.fake_dir.join("fake-go"));
        let _ = std::fs::remove_file(self.fake_dir.join("fake-waiting"));
    }
    /// The chat's notes (system messages), oldest first.
    fn notes(&self, thread: &str) -> Vec<String> {
        chat::messages(&self.st.db, thread).unwrap().into_iter().filter(|m| m.role == "system").map(|m| m.body_md.unwrap_or_default()).collect()
    }
    fn checks(&self) -> Vec<Run> {
        core_runs::list_for_agent(&self.st.db, &self.lead, 100).unwrap().into_iter().filter(|r| r.trigger == "board_check").collect()
    }
}

async fn finished(done: tokio::task::JoinHandle<app_chat::TurnSummary>) -> app_chat::TurnSummary {
    tokio::time::timeout(Duration::from_secs(30), done).await.expect("the answer finished").unwrap()
}

fn tool_limit(minutes: u64, calls: u32) -> String {
    format!("it stopped at the tool-call limit ({minutes} min or {calls} tool calls per chat answer, Settings → Runs)")
}

// ---- Settings → Runs ----

#[tokio::test]
async fn chat_answer_limits_are_15_minutes_and_60_tool_calls_unless_changed_and_stay_after_a_restart() {
    let tmp = tempfile::tempdir().unwrap();
    {
        let st = gizai_lib::test_state(tmp.path());
        let s = runs::get_settings(&st);
        assert_eq!((s.max_chat_minutes, s.max_chat_tool_calls), (15, 60), "today's values");
        assert_eq!((s.max_run_minutes, s.max_run_tool_calls), (90, 200), "the run limits are their own");
        // what the UI gets and sends (src/types.ts Settings)
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!((&v["maxChatMinutes"], &v["maxChatToolCalls"]), (&json!(15), &json!(60)), "{v}");
        let mut sent = v.clone();
        sent["maxChatMinutes"] = json!(120);
        sent["maxChatToolCalls"] = json!(300);
        let sent: Settings = serde_json::from_value(sent).unwrap();
        runs::save_settings(&st, &sent).unwrap();
        let s = runs::get_settings(&st);
        assert_eq!((s.max_chat_minutes, s.max_chat_tool_calls, s.max_run_minutes, s.max_run_tool_calls), (120, 300, 90, 200));
    }
    // Gizai starts again on the same data folder
    let st = gizai_lib::test_state(tmp.path());
    let s = runs::get_settings(&st);
    assert_eq!((s.max_chat_minutes, s.max_chat_tool_calls), (120, 300), "saved");
}

#[tokio::test]
async fn chat_answer_limits_outside_5_to_240_minutes_or_20_to_500_tool_calls_are_refused_with_the_range_and_nothing_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let before = runs::get_settings(&st);
    let minutes = "a chat answer may last between 5 and 240 minutes";
    let calls = "a chat answer may make between 20 and 500 tool calls";
    for (m, c, why) in [(4, 60, minutes), (241, 60, minutes), (0, 60, minutes), (15, 19, calls), (15, 501, calls), (15, 0, calls)] {
        // with other changes in the same save: none of them is kept either
        let bad = Settings { max_chat_minutes: m, max_chat_tool_calls: c, max_run_minutes: 45, max_concurrent_runs: 7, agents_paused: true, ..before.clone() };
        assert_eq!(runs::save_settings(&st, &bad).unwrap_err(), why, "{m} min / {c} calls");
        assert_eq!(serde_json::to_value(runs::get_settings(&st)).unwrap(), serde_json::to_value(&before).unwrap(), "{m} min / {c} calls changed something");
    }
    // the ends of the ranges are allowed
    for (m, c) in [(5, 20), (240, 500)] {
        runs::save_settings(&st, &Settings { max_chat_minutes: m, max_chat_tool_calls: c, ..before.clone() }).unwrap();
        let s = runs::get_settings(&st);
        assert_eq!((s.max_chat_minutes, s.max_chat_tool_calls), (m, c));
    }
}

// ---- A chat answer ----

#[tokio::test]
async fn a_chat_answer_stops_at_the_tool_calls_in_settings_and_the_chat_says_so_with_the_set_numbers() {
    let t = setup().await;
    // the defaults: 60 tool calls
    let (thread, s) = t.answer(None, "FAKE_CALLS=61 sort the memory").await;
    assert_eq!(s.status, "timed_out", "{s:?}");
    assert_eq!(s.error.as_deref(), Some(tool_limit(15, 60).as_str()));
    assert_eq!(t.notes(&thread), [format!("The Team Lead couldn't answer: {}", tool_limit(15, 60))]);

    // 20 tool calls and 30 minutes: 20 calls are fine, the 21st stops the answer
    t.chat_limits(30, 20);
    let (thread, s) = t.answer(None, "FAKE_CALLS=20 set up the cards").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert!(t.notes(&thread).is_empty(), "{:?}", t.notes(&thread));
    let (again, s) = t.answer(Some(thread.clone()), "FAKE_CALLS=21 and the next ones").await;
    assert_eq!(again, thread);
    assert_eq!(s.status, "timed_out", "{s:?}");
    assert_eq!(s.error.as_deref(), Some(tool_limit(30, 20).as_str()));
    assert_eq!(t.notes(&thread), [format!("The Team Lead couldn't answer: {}", tool_limit(30, 20))]);
    // the run says the same
    let run = core_runs::get(&t.st.db, &s.run_id).unwrap();
    assert_eq!((run.trigger.as_str(), run.status.as_str(), run.error.as_deref()), ("chat", "timed_out", Some(tool_limit(30, 20).as_str())));

    // 500 tool calls: a long answer goes on past the old 60
    t.chat_limits(15, 500);
    let (_, s) = t.answer(Some(thread.clone()), "FAKE_CALLS=120 sort the memory").await;
    assert_eq!(s.status, "succeeded", "{s:?}");
}

#[tokio::test]
async fn a_chat_answer_stops_at_the_time_limit_when_that_comes_first_and_says_so() {
    let t = setup().await;
    t.chat_limits(15, 500);
    // Settings refuses less than 5 minutes; 0 straight in the database reaches the time limit at once
    settings::set(&t.st.db, "max_chat_minutes", &0u64).unwrap();
    let t0 = Instant::now();
    let (thread, s) = t.answer(None, "FAKE_HANG think for a long time").await;
    assert!(t0.elapsed() < Duration::from_secs(20), "stopped by the 0 minutes, not by 15: {:?}", t0.elapsed());
    assert_eq!(s.status, "timed_out", "{s:?}");
    let why = "it stopped at the time limit (0 min or 500 tool calls per chat answer, Settings → Runs)";
    assert_eq!(s.error.as_deref(), Some(why));
    assert_eq!(t.notes(&thread), [format!("The Team Lead couldn't answer: {why}")]);
}

#[tokio::test]
async fn a_changed_limit_counts_from_the_next_answer_and_an_answer_already_running_keeps_its_own() {
    let t = setup().await;
    // starts with 20 tool calls; while it runs, Settings goes to 500
    t.chat_limits(15, 20);
    let (thread, done) = app_chat::send(&t.st, None, "FAKE_GATE FAKE_CALLS=30 sort the memory".into(), None).await.unwrap();
    t.until_waiting().await;
    t.chat_limits(240, 500);
    t.go();
    let s = finished(done).await;
    assert_eq!(s.status, "timed_out", "{s:?}");
    assert_eq!(s.error.as_deref(), Some(tool_limit(15, 20).as_str()), "the limits it started with");
    // the next answer has the new ones
    let (_, s) = t.answer(Some(thread.clone()), "FAKE_CALLS=30 go on").await;
    assert_eq!(s.status, "succeeded", "{s:?}");

    // the other way: starts with 500; while it runs, Settings goes to 20
    t.gate_closed();
    let (_, done) = app_chat::send(&t.st, Some(thread.clone()), "FAKE_GATE FAKE_CALLS=30 and the rest".into(), None).await.unwrap();
    t.until_waiting().await;
    t.chat_limits(240, 20);
    t.go();
    assert_eq!(finished(done).await.status, "succeeded", "not cut off by the new 20");
    let (_, s) = t.answer(Some(thread), "FAKE_CALLS=30 once more").await;
    assert_eq!(s.status, "timed_out", "{s:?}");
    assert_eq!(s.error.as_deref(), Some(tool_limit(240, 20).as_str()));
}

#[tokio::test]
async fn a_chat_answers_token_for_gizais_tools_lives_for_its_time_limit_and_a_quarter_of_an_hour() {
    let t = setup().await;
    for (minutes, ttl) in [(15u64, 30 * MIN), (120, 135 * MIN)] {
        t.chat_limits(minutes, 60);
        t.gate_closed();
        let (_, done) = app_chat::send(&t.st, None, "FAKE_GATE FAKE_CALLS=1 a long one".into(), None).await.unwrap();
        t.until_waiting().await;
        let lives: Vec<i64> = t.st.db.read(|c| {
            let mut q = c.prepare("SELECT expires_at - created_at FROM api_tokens WHERE revoked_at IS NULL AND scopes_json LIKE '%\"chat\"%'")?;
            let rows = q.query_map([], |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
            Ok(rows)
        }).unwrap();
        assert_eq!(lives, [ttl], "{minutes} min");
        t.go();
        assert_eq!(finished(done).await.status, "succeeded");
    }
}

// ---- A board check ----

#[tokio::test]
async fn a_board_check_stops_at_the_chat_answer_limits_in_settings() {
    let t = setup().await;
    t.chat_limits(15, 20);
    let t0 = ids::now_ms();
    assert!(board::tick(&t.st, t0).await.is_none(), "an empty board");
    // a card in To do with no agent on it: a finding; its title reaches the check's prompt
    let todo = team::get(&t.st.db, &team::list(&t.st.db).unwrap()[0].id).unwrap().states.into_iter().find(|s| s.category == "ready").unwrap().id;
    tasks::create(&t.st.db, &t.st.you_id, TaskInput { project_id: t.project.clone(), title: "Export invoices FAKE_CALLS=30".into(),
                                                       state_id: Some(todo), ..Default::default() }).unwrap();
    let h = board::tick(&t.st, t0 + 15 * MIN).await.expect("a check started");
    let s = tokio::time::timeout(Duration::from_secs(30), h).await.expect("the check finished").unwrap();
    assert_eq!(s.status, "timed_out", "{s:?}");
    assert_eq!(s.error.as_deref(), Some(tool_limit(15, 20).as_str()));
    let run = t.checks().remove(0);
    assert_eq!((run.status.as_str(), run.error.as_deref()), ("timed_out", Some(tool_limit(15, 20).as_str())));
}

// ---- Task runs ----

/// Makes In progress Manual, so a run stopped at a limit isn't started again by the queue before the test reads it.
fn in_progress_manual(st: &AppState) {
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let state = team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &state, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
}

#[tokio::test]
async fn task_runs_keep_the_run_limits_whatever_the_chat_answer_limits_are() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    settings::set(&st.db, "claude_bin", &RUN_FAKE.to_string()).unwrap();
    in_progress_manual(&st);
    // chat answers as short as can be (0 minutes, 1 tool call, straight in the database): a task run doesn't notice
    settings::set(&st.db, "max_chat_minutes", &0u64).unwrap();
    settings::set(&st.db, "max_chat_tool_calls", &1u32).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(RUN_FAKE.into())).await.unwrap();
    let run = core_runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(run.status, "succeeded", "{:?}", run.error);

    // the run limit of 1 tool call stops a run, with chat answers at their highest
    let mut s = runs::get_settings(&st);
    s.max_chat_minutes = 240;
    s.max_chat_tool_calls = 500;
    runs::save_settings(&st, &s).unwrap();
    settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(RUN_FAKE.into())).await.unwrap();
    let run = core_runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(run.status, "timed_out");
    let err = run.error.unwrap_or_default();
    assert!(err.contains("1 tool calls") && !err.contains("chat answer"), "{err}");
}
