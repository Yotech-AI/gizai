// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-62 end to end: the subscription limits a (fake) coding CLI reports in a task run or a chat turn are kept for the
// coding CLI entry that run used, and only for it; Codex's are read from its own session log after the run. The fakes
// are small wrappers around crates/gizai-agents/tests/fake-claude.sh, fake-claude-chat.py and fake-cli.sh, never the
// real CLIs.
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gizai_agents::stream::RunEvent;
use gizai_core::clis::{self, Cli};
use gizai_core::limits::{self, CliLimits};
use gizai_core::model::AgentInput;
use gizai_lib::AppState;

const AGENTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests");
const CODEX_LOG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-core/tests/fixtures/limits-codex-rollout.jsonl");

/// What Claude Code account 1 heard of its limits (session 42%, weekly 18%, Fable 5%).
const EVENT_1: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1791565200,"rateLimitType":"five_hour","utilization":0.42,"unifiedWindows":{"five_hour":{"utilization":0.42,"resetsAt":1791565200},"seven_day":{"utilization":0.18,"resetsAt":1791882000},"seven_day_overage_included":{"utilization":0.05,"resetsAt":1791968400}}},"session_id":"S"}"#;
/// And account 2 (session 91% with a warning, weekly 35%, no Fable window).
const EVENT_2: &str = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1791561600,"rateLimitType":"five_hour","utilization":0.91,"unifiedWindows":{"five_hour":{"utilization":0.91,"resetsAt":1791561600},"seven_day":{"utilization":0.35,"resetsAt":1792054800}}},"session_id":"S"}"#;

fn script(dir: &Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/usr/bin/env bash\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_string_lossy().into_owned()
}

/// A fake Claude Code that writes `event` right after the first line of `fake`'s stream.
fn claude_with(dir: &Path, name: &str, fake: &str, event: &str) -> String {
    script(dir, name, &format!("set -o pipefail\n'{fake}' \"$@\" | {{ IFS= read -r first; printf '%s\\n' \"$first\"; printf '%s\\n' '{event}'; cat; }}"))
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &AppState, name: &str, kind: &str, command: &str, env: &[String]) -> String {
    let mut list: Vec<Cli> = clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.to_vec(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// The agent with role `role` (made by `test_task`), moved to the CLI `cli`.
fn put_agent_on(st: &AppState, role: &str, cli: &str) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == role).unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: role.into(), adapter: cli.into(),
        ..Default::default() }).unwrap();
    agent.actor_id
}

fn block(st: &AppState, cli: &str) -> CliLimits {
    gizai_lib::limits::subscription(st).unwrap().into_iter().find(|b| b.cli_id == cli).unwrap_or_else(|| panic!("no block for {cli}"))
}

/// (key, used %, run) of each limit with a reading.
fn numbers(b: &CliLimits) -> Vec<(String, Option<f64>, String)> {
    b.limits.iter().filter_map(|l| l.reading.as_ref().map(|r| (l.key.clone(), r.used_percent.map(|u| (u * 10.0).round() / 10.0), r.run_id.clone().unwrap_or_default())))
        .collect()
}

fn state(dir: &Path) -> AppState {
    let st = gizai_lib::test_state(dir);
    // Nothing starts on its own between the runs these tests start.
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    st
}

#[tokio::test]
async fn a_runs_rate_limit_event_counts_for_its_own_claude_code_account_only_and_stays_out_of_the_run_panel() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state(tmp.path());
    let repo = git_repo(tmp.path());
    let fake = format!("{AGENTS}/fake-claude.sh");
    gizai_core::settings::set(&st.db, "claude_bin", &claude_with(tmp.path(), "claude-1", &fake, EVENT_1)).unwrap();
    let cc2 = add_cli(&st, "Claude Code 2", "claude_code", &claude_with(tmp.path(), "claude-2", &fake, EVENT_2),
                      &[format!("CLAUDE_CONFIG_DIR={}/acct-2", tmp.path().display())]);
    let t1 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let t2 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "frontend");
    put_agent_on(&st, "frontend", &cc2);

    let s1 = gizai_lib::runs::run_once(&st, &t1, None, None).await.unwrap();
    let s2 = gizai_lib::runs::run_once(&st, &t2, None, None).await.unwrap();
    assert_eq!((s1.status.as_str(), s2.status.as_str()), ("succeeded", "succeeded"), "{:?} {:?}", s1.error, s2.error);
    assert_eq!(gizai_core::runs::get(&st.db, &s2.run_id).unwrap().adapter.as_deref(), Some(cc2.as_str()), "the run records its entry's id");

    let cc = block(&st, clis::CLAUDE_CODE);
    assert_eq!(numbers(&cc), vec![(limits::FIVE_HOUR.into(), Some(42.0), s1.run_id.clone()), (limits::SEVEN_DAY.into(), Some(18.0), s1.run_id.clone()),
                                  (limits::FABLE.into(), Some(5.0), s1.run_id.clone())]);
    let two = block(&st, &cc2);
    assert_eq!(numbers(&two), vec![(limits::FIVE_HOUR.into(), Some(91.0), s2.run_id.clone()), (limits::SEVEN_DAY.into(), Some(35.0), s2.run_id.clone())]);
    assert_eq!(two.limits[0].reading.as_ref().unwrap().status.as_deref(), Some("allowed_warning"));
    assert_eq!(two.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Frontend Agent"]);
    assert_eq!(cc.agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Backend Agent"]);

    // The run's log has the line; the Run panel doesn't show it.
    let run = gizai_core::runs::get(&st.db, &s1.run_id).unwrap();
    assert!(std::fs::read_to_string(&run.log_path).unwrap().contains("rate_limit_event"));
    let evs: Vec<RunEvent> = gizai_lib::runs::events_for(&st, &s1.run_id).into_iter().map(|e| e.event).collect();
    assert!(!evs.iter().any(|e| matches!(e, RunEvent::Limits { .. })), "{evs:?}");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { .. })), "{evs:?}");
}

#[tokio::test]
async fn a_codex_run_reads_its_own_threads_session_log_in_its_account_folder_after_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state(tmp.path());
    let repo = git_repo(tmp.path());
    let home = tmp.path().join("codex-home");
    // Another thread's log in the same folder, with other numbers: not this run's.
    let day = home.join("sessions/2026/10/09");
    std::fs::create_dir_all(&day).unwrap();
    std::fs::write(day.join("rollout-2026-10-09T10-00-00-019a-other-thread.jsonl"),
                   r#"{"timestamp":"2026-10-09T15:00:00.000Z","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":99.0,"window_minutes":300,"resets_at":1791565200}}}}"#).unwrap();
    // Codex writes its log while it runs; fake-cli.sh's thread is 019a-fake-thread.
    let codex_bin = script(tmp.path(), "codex", &format!(
        "mkdir -p \"$CODEX_HOME/sessions/2026/10/09\"\ncat '{CODEX_LOG}' > \"$CODEX_HOME/sessions/2026/10/09/rollout-2026-10-09T13-58-00-019a-fake-thread.jsonl\"\nexec bash '{AGENTS}/fake-cli.sh' \"$@\""));
    let codex = add_cli(&st, "Codex", "codex", &codex_bin, &["FAKE_KIND=codex".into(), format!("CODEX_HOME={}", home.display())]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, "backend", &codex);

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let b = block(&st, &codex);
    assert_eq!(numbers(&b), vec![(limits::PRIMARY.into(), Some(23.5), s.run_id.clone()), (limits::SECONDARY.into(), Some(41.0), s.run_id.clone())]);
    assert_eq!(b.limits.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["5-hour limit", "Weekly limit"]);
    // The account's folder, with the home folder as ~ (a temp folder may be inside it).
    let user_home = std::env::var("HOME").unwrap();
    let want = match home.strip_prefix(&user_home) { Ok(rest) => format!("~/{}", rest.display()), Err(_) => home.display().to_string() };
    assert_eq!(b.account_dir.as_deref(), Some(want.as_str()));
    assert_eq!(numbers(&block(&st, clis::CLAUDE_CODE)), vec![], "nothing for Claude Code");
    // Gizai only read Codex's folder.
    let mut names: Vec<String> = std::fs::read_dir(&day).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["rollout-2026-10-09T10-00-00-019a-other-thread.jsonl", "rollout-2026-10-09T13-58-00-019a-fake-thread.jsonl"]);
}

#[tokio::test]
async fn a_claude_code_run_that_hit_a_limit_is_a_reading_and_a_successful_runs_text_never_is() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state(tmp.path());
    let repo = git_repo(tmp.path());
    // Claude Code's answer when the account hit its weekly limit (GA-50's fixture), as a task run.
    let hit = script(tmp.path(), "claude-hit", &format!("cat > /dev/null\ncat '{AGENTS}/fixtures/chat-limit.jsonl'\nexit 1"));
    gizai_core::settings::set(&st.db, "claude_bin", &hit).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_ne!(s.status, "succeeded");
    let weekly = block(&st, clis::CLAUDE_CODE).limits.into_iter().find(|l| l.key == limits::SEVEN_DAY).unwrap().reading.expect("a reading");
    assert_eq!((weekly.used_percent, weekly.status.as_deref(), weekly.resets_text.as_deref(), weekly.run_id.as_deref()),
               (None, Some("rejected"), Some("Oct 9, 5pm (Europe/Amsterdam)"), Some(s.run_id.as_str())), "reached, no number made up");

    // An agent that quotes a limit in a successful answer hit nothing.
    let quote = script(tmp.path(), "claude-quote", concat!(
        "cat > /dev/null\n",
        "echo '{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"S-q\",\"model\":\"m\",\"tools\":[],\"mcp_servers\":[]}'\n",
        "echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"The log said: You have hit your session limit · resets 3pm\\nGIZAI_RESULT: {\\\"outcome\\\":\\\"ready_for_testing\\\",\\\"summary\\\":\\\"s\\\",\\\"issues\\\":[]}\",\"total_cost_usd\":0.01,\"usage\":{\"input_tokens\":10,\"output_tokens\":2},\"num_turns\":1,\"session_id\":\"S-q\"}'"));
    gizai_core::settings::set(&st.db, "claude_bin", &quote).unwrap();
    let task2 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s2 = gizai_lib::runs::run_once(&st, &task2, None, None).await.unwrap();
    assert_eq!(s2.status, "succeeded", "{:?}", s2.error);
    let session = block(&st, clis::CLAUDE_CODE).limits.into_iter().find(|l| l.key == limits::FIVE_HOUR).unwrap();
    assert_eq!(session.reading, None);
}

// Chat turns

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

async fn turn(st: &AppState, thread: Option<String>, text: &str, cli: Option<String>) -> (String, gizai_lib::chat::TurnSummary) {
    let (id, done) = gizai_lib::chat::send_on(st, thread, text.into(), cli, None).await.unwrap();
    (id, tokio::time::timeout(std::time::Duration::from_secs(30), done).await.expect("turn finished").unwrap())
}

#[tokio::test]
async fn a_chat_turn_keeps_the_limits_for_the_account_it_ran_on_and_a_limit_it_hit() {
    let dir = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::open_state(dir.path().join("data"), Arc::new(|_| {})).unwrap();
    st.mcp_socket = st.data_dir.join("mcp.sock");
    st.mcp_shim = Some(shim());
    let chat_fake = format!("{AGENTS}/fake-claude-chat.py");
    gizai_core::settings::set(&st.db, "claude_bin", &chat_fake).unwrap();
    let cc2 = add_cli(&st, "Claude Code 2", "claude_code", &claude_with(dir.path(), "claude-2", &chat_fake, EVENT_2),
                      &[format!("CLAUDE_CONFIG_DIR={}/acct-2", dir.path().display())]);
    gizai_core::projects::create(&st.db, &st.you_id, gizai_core::model::ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    let _server = gizai_lib::mcp::start(&st).unwrap();

    // A chat that runs on Claude Code 2 (Runs on under the text box), while the Team Lead is on Claude Code.
    let (_, s) = turn(&st, None, "What is next?", Some(cc2.clone())).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    assert_eq!(gizai_core::runs::get(&st.db, &s.run_id).unwrap().adapter.as_deref(), Some(cc2.as_str()));
    assert_eq!(numbers(&block(&st, &cc2)), vec![(limits::FIVE_HOUR.into(), Some(91.0), s.run_id.clone()), (limits::SEVEN_DAY.into(), Some(35.0), s.run_id.clone())]);
    assert_eq!(numbers(&block(&st, clis::CLAUDE_CODE)), vec![], "the Team Lead's own account reported nothing");
    let two = block(&st, &cc2);
    assert_eq!((two.chats, two.lead_chat), (1, false));
    assert!(block(&st, clis::CLAUDE_CODE).lead_chat, "the Team Lead's chat runs on Claude Code");

    // A turn on the Team Lead's account that hit the weekly limit.
    let (_, hit) = turn(&st, None, "FAKE_CHAT_LIMIT plan the week", None).await;
    assert_ne!(hit.status, "succeeded");
    let weekly = block(&st, clis::CLAUDE_CODE).limits.into_iter().find(|l| l.key == limits::SEVEN_DAY).unwrap().reading.expect("a reading");
    assert_eq!((weekly.used_percent, weekly.status.as_deref(), weekly.run_id.as_deref()), (None, Some("rejected"), Some(hit.run_id.as_str())));
    assert_eq!(block(&st, &cc2).limits[1].reading.as_ref().unwrap().used_percent, Some(35.0), "Claude Code 2's weekly number is its own");
}
