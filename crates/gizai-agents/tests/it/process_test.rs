// Linux and macOS only: process groups and signals, with shell scripts as fake CLIs. Windows: os_test.rs.
#![cfg(unix)]
use gizai_agents::{chat_stream::ChatEvent, claude::ClaudeArgs, process::{spawn, Caps, AFTER_RESULT, ENDED_AFTER_RESULT, StreamEvent},
                   stream::RunEvent};
use std::time::{Duration, Instant};

fn args(prompt: &str) -> ClaudeArgs {
    ClaudeArgs { bin: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude.sh").into(), prompt: prompt.into(),
                 session_id: "S".into(), permission_mode: "acceptEdits".into(), ..Default::default() }
}

async fn drain(h: &mut gizai_agents::process::RunHandle) -> Vec<RunEvent> {
    let mut evs = vec![];
    while let Some(e) = h.events.recv().await { evs.push(e); }
    evs
}

#[tokio::test]
async fn streams_events_and_writes_the_log() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("r.jsonl");
    let mut h = spawn(&args("p"), tmp.path(), &log, Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { .. })));
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:0"));
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 6);
    assert!(std::fs::read_to_string(tmp.path().join("r.stderr.log")).unwrap().contains("fake claude done"));
}

#[tokio::test]
async fn stop_ends_a_hanging_run() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("hang"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let _init = h.events.recv().await;
    let t0 = Instant::now();
    h.stop.stop();
    let evs = drain(&mut h).await;
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:130"), "{evs:?}");
    assert!(t0.elapsed() < Duration::from_secs(2), "SIGINT should end it at once; took {:?}", t0.elapsed());
}

#[tokio::test]
async fn stop_escalates_to_sigterm_when_sigint_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("stubborn"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let _init = h.events.recv().await;
    let t0 = Instant::now();
    h.stop.stop();
    let evs = drain(&mut h).await;
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:signal-15"), "{evs:?}");
    let took = t0.elapsed();
    assert!(took >= Duration::from_millis(4500) && took < Duration::from_secs(9), "took {took:?}");
}

#[tokio::test]
async fn the_tool_call_cap_stops_the_run_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    // the fixture has two tool calls; a cap of 1 is exceeded on the second
    let mut h = spawn(&args("p"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 1, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:tools")), "{evs:?}");
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type.starts_with("exit:")));
}

#[tokio::test]
async fn the_time_cap_stops_the_run_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("hang"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(1), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time")), "{evs:?}");
}

#[tokio::test]
async fn every_tool_call_counts_even_several_in_one_message() {
    let tmp = tempfile::tempdir().unwrap();
    // the fixture has 2 tool calls: a cap of 2 is not exceeded
    let mut h = spawn(&args("p"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 2, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(!evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type.starts_with("cap_exceeded"))), "{evs:?}");
}

#[tokio::test]
async fn a_background_child_holding_stdout_does_not_hang_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("orphan"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:0"));
    assert!(t0.elapsed() < Duration::from_secs(8), "took {:?}", t0.elapsed());
}

/// Whether a process group is gone within `within` (a process that ended counts until it has been reaped).
async fn group_gone(pgid: u32, within: Duration) -> bool {
    let t0 = Instant::now();
    // SAFETY: signal 0 only checks that the group exists; nothing is sent.
    while unsafe { libc::kill(-(pgid as i32), 0) } == 0 {
        if t0.elapsed() > within { return false; }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    true
}

#[tokio::test]
async fn kill_ends_the_run_group_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("stubborn"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let _init = h.events.recv().await;
    let t0 = Instant::now();
    h.stop.kill();
    let evs = drain(&mut h).await;
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:signal-9"), "{evs:?}");
    assert!(t0.elapsed() < Duration::from_secs(2), "SIGKILL doesn't wait for Stop's grace; took {:?}", t0.elapsed());
    assert!(group_gone(h.pid, Duration::from_secs(1)).await, "the stubborn agent and its sleep are gone");
}

#[tokio::test]
async fn a_leftover_that_ignores_sigterm_gets_sigkill_after_3s() {
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("leftover"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:0"), "{evs:?}");
    let took = t0.elapsed();
    assert!(took >= Duration::from_millis(2900) && took < Duration::from_secs(6), "SIGTERM, then SIGKILL 3 s later; took {took:?}");
    assert!(group_gone(h.pid, Duration::from_secs(1)).await, "the leftover that ignored SIGTERM doesn't run on");
}

#[test]
fn a_missing_binary_is_a_spawn_error() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let a = ClaudeArgs { bin: "/nonexistent/claude".into(), ..args("p") };
    let r = rt.block_on(async { spawn::<RunEvent>(&a, tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 1, after_result: AFTER_RESULT }).map(|_| ()) });
    assert!(matches!(r, Err(gizai_agents::AgentError::Spawn(_))));
}

#[tokio::test]
async fn a_huge_prompt_goes_in_on_stdin() {
    let tmp = tempfile::tempdir().unwrap();
    let big = "x".repeat(200_000);
    let mut h = spawn(&args(&big), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: AFTER_RESULT }).unwrap();
    let evs = drain(&mut h).await;
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:0"), "{evs:?}");
    assert!(std::fs::read_to_string(tmp.path().join("r.stderr.log")).unwrap().contains("prompt chars: 200000"));
}

#[test]
fn an_orphaned_run_group_is_ended_only_when_it_is_really_ours() {
    use std::os::unix::process::CommandExt;
    let here = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new("sleep").arg("30").current_dir(here.path()).process_group(0).spawn().unwrap();
    let pid = child.id();
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(!gizai_agents::process::end_orphan_group(pid, elsewhere.path()), "a different working folder: not ours");
    assert!(child.try_wait().unwrap().is_none(), "still running");
    assert!(gizai_agents::process::end_orphan_group(pid, here.path()));
    let status = child.wait().unwrap();
    assert!(!status.success());
    assert!(!gizai_agents::process::end_orphan_group(pid, here.path()), "gone now");
    assert!(!gizai_agents::process::end_orphan_group(1, std::path::Path::new("/")), "never pid 1");
}

// GA-221: a CLI that doesn't exit once the agent is done (something the run started still runs in the background) is
// ended as Stop ends it once it has been quiet for `Caps::after_result`, with `ended_after_result` before its `exit:`.
// The fake's modes for it are listed at the top of fake-claude.sh.

fn after(wait: Duration) -> Caps {
    Caps { max_time: Duration::from_secs(60), max_tool_calls: 80, after_result: wait }
}

fn at(evs: &[RunEvent], marker: &str) -> Option<usize> {
    evs.iter().position(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == marker))
}

fn exit_of(evs: &[RunEvent]) -> &str {
    match evs.last() {
        Some(RunEvent::Other { raw_type }) if raw_type.starts_with("exit:") => raw_type,
        _ => panic!("the last event isn't exit: {evs:?}"),
    }
}

#[tokio::test]
async fn a_cli_that_stays_quiet_after_its_result_line_is_ended_after_after_result_with_the_marker_then_exit() {
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("FAKE_RESULT_THEN_HANG"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(2))).unwrap();
    let evs = drain(&mut h).await;
    let took = t0.elapsed();
    let result = evs.iter().position(|e| matches!(e, RunEvent::Result { is_error: false, .. })).expect("its result line");
    let ended = at(&evs, ENDED_AFTER_RESULT).expect("ended_after_result");
    assert!(result < ended && ended == evs.len() - 2, "after its result, right before exit: {evs:?}");
    assert_eq!(evs.iter().filter(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == ENDED_AFTER_RESULT)).count(), 1);
    assert_eq!(exit_of(&evs), "exit:130", "SIGINT ended it, as Stop's first step");
    assert!(took >= Duration::from_secs(2) && took < Duration::from_secs(2 + 10), "within after_result plus 10 s; took {took:?}");
    assert!(group_gone(h.pid, Duration::from_secs(1)).await, "its process group is gone");
}

#[tokio::test]
async fn a_cli_whose_last_text_has_a_gizai_result_line_and_no_result_line_is_ended_the_same_way() {
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("FAKE_TEXT_THEN_HANG"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(2))).unwrap();
    let evs = drain(&mut h).await;
    let took = t0.elapsed();
    assert!(!evs.iter().any(|e| matches!(e, RunEvent::Result { .. })), "no result line: {evs:?}");
    let text = evs.iter().rposition(|e| matches!(e, RunEvent::Text { text } if text.contains("GIZAI_RESULT:"))).expect("its final text");
    let ended = at(&evs, ENDED_AFTER_RESULT).expect("ended_after_result");
    assert!(text < ended && ended == evs.len() - 2, "{evs:?}");
    assert_eq!(exit_of(&evs), "exit:130");
    assert!(took >= Duration::from_secs(2) && took < Duration::from_secs(2 + 10), "took {took:?}");
}

#[tokio::test]
async fn a_tool_call_after_the_gizai_result_text_means_the_agent_went_on_and_the_cli_exits_as_usual() {
    // the text with the GIZAI_RESULT line, a tool call, 3 s of quiet (more than the 1 s wait), then the rest and exit 0
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("FAKE_TEXT_TOOL_THEN_GO_ON"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(1))).unwrap();
    let evs = drain(&mut h).await;
    assert!(t0.elapsed() >= Duration::from_secs(3), "it went on to the end: {:?}", t0.elapsed());
    let text = evs.iter().position(|e| matches!(e, RunEvent::Text { text } if text.contains("GIZAI_RESULT:"))).expect("the text");
    let tool = evs.iter().position(|e| matches!(e, RunEvent::ToolUse { .. })).expect("the tool call");
    assert!(text < tool, "{evs:?}");
    assert!(at(&evs, ENDED_AFTER_RESULT).is_none(), "{evs:?}");
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { is_error: false, .. })), "its result came: {evs:?}");
    assert_eq!(exit_of(&evs), "exit:0");
}

#[tokio::test]
async fn each_line_after_the_result_starts_the_quiet_time_again() {
    // after its result line, a line every 0.5 s for 3 s: with a 1 s wait it ends about 1 s after the last of them
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("FAKE_RESULT_THEN_TRICKLE"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(1))).unwrap();
    let evs = drain(&mut h).await;
    let took = t0.elapsed();
    assert!(at(&evs, ENDED_AFTER_RESULT).is_some(), "{evs:?}");
    assert_eq!(exit_of(&evs), "exit:130");
    assert!(took >= Duration::from_millis(3500) && took < Duration::from_secs(12), "not 1 s after the result line; took {took:?}");
}

#[tokio::test]
async fn a_cli_that_exits_right_after_its_result_gives_the_same_events_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let mut short = spawn(&args("p"), tmp.path(), &tmp.path().join("a.jsonl"), after(Duration::from_secs(1))).unwrap();
    let mut usual = spawn(&args("p"), tmp.path(), &tmp.path().join("b.jsonl"), after(AFTER_RESULT)).unwrap();
    let (a, b) = (drain(&mut short).await, drain(&mut usual).await);
    assert_eq!(a, b);
    assert!(at(&a, ENDED_AFTER_RESULT).is_none(), "{a:?}");
    assert_eq!(exit_of(&a), "exit:0");
    assert_eq!(a.len(), 8, "the fixture's 7 events and exit:0: {a:?}");
}

#[tokio::test]
async fn stop_ends_a_run_at_once_while_the_wait_is_armed() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("FAKE_RESULT_THEN_HANG"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(60))).unwrap();
    let mut evs = vec![];
    while let Some(e) = tokio::time::timeout(Duration::from_secs(10), h.events.recv()).await.expect("its result line") {
        let done = matches!(e, RunEvent::Result { .. });
        evs.push(e);
        if done { break; }
    }
    let t0 = Instant::now();
    h.stop.stop();
    evs.extend(drain(&mut h).await);
    assert!(t0.elapsed() < Duration::from_secs(2), "SIGINT at once; took {:?}", t0.elapsed());
    assert!(at(&evs, ENDED_AFTER_RESULT).is_none(), "Stop ended it, not the wait: {evs:?}");
    assert_eq!(exit_of(&evs), "exit:130");
}

#[tokio::test]
async fn a_cli_that_ignores_sigint_after_its_result_gets_sigterm_5_s_later_as_with_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("FAKE_RESULT_THEN_STUBBORN"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(1))).unwrap();
    let evs = drain(&mut h).await;
    let took = t0.elapsed();
    assert!(at(&evs, ENDED_AFTER_RESULT).is_some(), "{evs:?}");
    assert_eq!(exit_of(&evs), "exit:signal-15");
    assert!(took >= Duration::from_millis(5500) && took < Duration::from_secs(1 + 10), "1 s, then SIGTERM 5 s after SIGINT; took {took:?}");
}

#[tokio::test]
async fn a_time_cap_that_comes_first_ends_the_run_as_before_and_the_wait_stops_the_time_cap() {
    // the time cap at 1 s, the wait 3 s: the cap ends it, without the marker
    let tmp = tempfile::tempdir().unwrap();
    let caps = Caps { max_time: Duration::from_secs(1), max_tool_calls: 80, after_result: Duration::from_secs(3) };
    let mut h = spawn(&args("FAKE_RESULT_THEN_HANG"), tmp.path(), &tmp.path().join("a.jsonl"), caps).unwrap();
    let evs = drain(&mut h).await;
    assert!(at(&evs, "cap_exceeded:time").is_some() && at(&evs, ENDED_AFTER_RESULT).is_none(), "{evs:?}");
    assert_eq!(exit_of(&evs), "exit:130");
    // the wait at 1 s, the time cap at 3 s: the wait ends it, and the time cap doesn't come after
    let t0 = Instant::now();
    let caps = Caps { max_time: Duration::from_secs(3), max_tool_calls: 80, after_result: Duration::from_secs(1) };
    let mut h = spawn(&args("FAKE_RESULT_THEN_HANG"), tmp.path(), &tmp.path().join("b.jsonl"), caps).unwrap();
    let evs = drain(&mut h).await;
    assert!(at(&evs, ENDED_AFTER_RESULT).is_some() && at(&evs, "cap_exceeded:time").is_none(), "{evs:?}");
    assert!(t0.elapsed() < Duration::from_secs(3), "took {:?}", t0.elapsed());
}

#[tokio::test]
async fn a_tool_call_cap_that_comes_first_ends_the_run_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let caps = Caps { max_time: Duration::from_secs(60), max_tool_calls: 1, after_result: Duration::from_secs(1) };
    let mut h = spawn(&args("FAKE_RESULT_THEN_HANG"), tmp.path(), &tmp.path().join("r.jsonl"), caps).unwrap();
    let evs = drain(&mut h).await;
    assert!(at(&evs, "cap_exceeded:tools").is_some(), "{evs:?}");
    assert!(at(&evs, ENDED_AFTER_RESULT).is_none(), "{evs:?}");
}

#[tokio::test]
async fn a_chat_turns_cli_that_stays_quiet_after_its_result_is_ended_too() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn::<ChatEvent>(&args("FAKE_RESULT_THEN_HANG"), tmp.path(), &tmp.path().join("r.jsonl"), after(Duration::from_secs(1))).unwrap();
    let mut evs = vec![];
    while let Some(e) = h.events.recv().await { evs.push(e); }
    let ended = evs.iter().position(|e| matches!(e, ChatEvent::Other { raw_type } if raw_type == ENDED_AFTER_RESULT)).expect("the marker");
    assert!(evs.iter().position(|e| matches!(e, ChatEvent::Result { .. })).is_some_and(|r| r < ended), "{evs:?}");
    assert!(matches!(evs.last(), Some(ChatEvent::Other { raw_type }) if raw_type == "exit:130"), "{evs:?}");
}

#[test]
fn the_agent_is_done_at_a_result_or_a_text_with_a_result_line_that_parses() {
    let line = "Done.\nGIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"Exporter added.\",\"issues\":[]}";
    let texts = [
        (line, true),
        ("Done, all tests pass.", false),
        ("GIZAI_RESULT: {\"outcome\":\"ready_for_testing\"", false),
        ("GIZAI_RESULT: {\"outcome\":\"all_good\",\"summary\":\"s\",\"issues\":[]}", false),
    ];
    for (text, done) in texts {
        assert_eq!(RunEvent::Text { text: text.into() }.is_result(), done, "{text}");
        assert_eq!(ChatEvent::Text { text: text.into() }.is_result(), done, "{text}");
    }
    for is_error in [false, true] {
        let r = RunEvent::Result { is_error, subtype: "success".into(), text: String::new(), cost_usd: None, input_tokens: 0, output_tokens: 0, num_turns: 1 };
        assert!(r.is_result());
        let c = ChatEvent::Result { is_error, subtype: "success".into(), text: String::new(), cost_usd: None, input_tokens: 0, output_tokens: 0, num_turns: 1 };
        assert!(c.is_result());
    }
    assert!(!RunEvent::ToolUse { name: "Bash".into(), summary: line.into() }.is_result());
    assert!(!ChatEvent::ToolUse { id: "t".into(), name: "Bash".into(), input: serde_json::json!({}) }.is_result());
    assert!(!RunEvent::Other { raw_type: "exit:0".into() }.is_result());
}
