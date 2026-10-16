// GA-188: the streaming fakes the benchmark harness runs with. FAKE_STREAM <rate>x<seconds> in fake-claude.sh's prompt
// prints that many events a second (a text, a tool call, its result, again) between run-ok's init line and its last two
// lines; FAKE_CHAT_STREAM in fake-claude-chat.py's prompt makes the last answer about 20 KB, in pieces of 20 to 40
// characters, one every 10 ms. Without the marker both behave as before. Linux and macOS only: bash, perl and python3.
#![cfg(unix)]
use gizai_agents::{chat_stream::{self, ChatEvent}, claude::ClaudeArgs, process::{spawn, Caps}, stream::{self, RunEvent}};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TESTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");

/// Runs a fake with the prompt on stdin: each stdout line with the time it came, since the start, and the exit code.
fn run_fake(program: &str, args: &[&str], prompt: &str) -> (Vec<(Duration, String)>, i32) {
    let mut child = Command::new(program).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().unwrap();
    let t0 = Instant::now();
    child.stdin.take().unwrap().write_all(prompt.as_bytes()).unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap()).lines().map(|l| (t0.elapsed(), l.unwrap())).collect();
    (lines, child.wait().unwrap().code().unwrap_or(-1))
}

fn run_ok() -> Vec<String> {
    std::fs::read_to_string(format!("{TESTS}/fixtures/run-ok.jsonl")).unwrap().lines().map(String::from).collect()
}

#[test]
fn fake_stream_prints_the_set_rate_for_the_set_time_between_run_oks_first_and_last_lines() {
    let (lines, code) = run_fake(&format!("{TESTS}/fake-claude.sh"), &[], "Do the card.\nFAKE_STREAM 50x2\n");
    assert_eq!(code, 0);
    let ok = run_ok();
    assert_eq!(lines.len(), 1 + 100 + 2, "init, 50 a second for 2 s, and run-ok's last two lines");
    assert_eq!(lines[0].1, ok[0]);
    assert_eq!(lines[101].1, ok[ok.len() - 2]);
    assert_eq!(lines[102].1, ok[ok.len() - 1]);
    let events = &lines[1..101];
    // a text, a tool call and its result, again: every line JSON that Gizai's run parser reads as such
    for (i, (_, line)) in events.iter().enumerate() {
        serde_json::from_str::<Value>(line).unwrap_or_else(|e| panic!("line {i} isn't JSON ({e}): {line}"));
        let parsed = stream::parse_line(line);
        let want = match i % 3 { 0 => matches!(parsed[..], [RunEvent::Text { .. }]), 1 => matches!(parsed[..], [RunEvent::ToolUse { .. }]),
                                 _ => matches!(parsed[..], [RunEvent::ToolResult { is_error: false, .. }]) };
        assert!(want, "event {i}: {parsed:?}");
    }
    assert!(events[3].1.contains("Step 2") && events[4].1.contains("tu-stream-2"), "each round has its own number");
    // 50 a second: never faster (each event waits for its own moment from the start), and not much slower
    let span = events[99].0 - events[0].0;
    assert!(span >= Duration::from_millis(1950), "100 events at 50 a second take 1.98 s; took {span:?}");
    assert!(span < Duration::from_millis(3000), "100 events at 50 a second took {span:?}");
    let first_second = events.iter().filter(|(at, _)| *at - events[0].0 < Duration::from_secs(1)).count();
    assert!((48..=52).contains(&first_second), "{first_second} events in the first second");
    // run-ok's last lines come once the time is up
    assert!(lines[101].0 - events[0].0 >= Duration::from_secs(2));
}

#[test]
fn without_fake_stream_the_fake_prints_run_ok_as_before() {
    for prompt in ["Do the card.", "FAKE_STREAM", "FAKE_STREAM 0x5", "FAKE_STREAM fast"] {
        let (lines, code) = run_fake(&format!("{TESTS}/fake-claude.sh"), &[], prompt);
        assert_eq!(code, 0, "{prompt}");
        assert_eq!(lines.into_iter().map(|(_, l)| l).collect::<Vec<_>>(), run_ok(), "{prompt}");
    }
}

#[tokio::test]
async fn a_fake_stream_run_goes_through_gizais_run_and_stop_ends_it_part_way() {
    let args = |prompt: &str| ClaudeArgs { bin: format!("{TESTS}/fake-claude.sh").into(), prompt: prompt.into(), session_id: "S".into(),
                                          permission_mode: "acceptEdits".into(), ..Default::default() };
    let caps = Caps { max_time: Duration::from_secs(60), max_tool_calls: 2000 };
    // the whole stream: every event and run-ok's result, then Gizai's exit marker
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("FAKE_STREAM 60x1"), tmp.path(), &tmp.path().join("r.jsonl"), caps).unwrap();
    let mut evs = vec![];
    while let Some(e) = h.events.recv().await { evs.push(e); }
    assert_eq!(evs.iter().filter(|e| matches!(e, RunEvent::ToolUse { .. })).count(), 20, "{evs:?}");
    assert_eq!(evs.iter().filter(|e| matches!(e, RunEvent::ToolResult { .. })).count(), 20);
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { is_error: false, .. })));
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:0"), "{:?}", evs.last());
    assert_eq!(std::fs::read_to_string(tmp.path().join("r.jsonl")).unwrap().lines().count(), 1 + 60 + 2);
    // Stop part-way: SIGINT ends it at once, with no result
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("FAKE_STREAM 50x20"), tmp.path(), &tmp.path().join("r.jsonl"), caps).unwrap();
    let mut evs = vec![];
    while evs.len() < 10 { evs.push(h.events.recv().await.unwrap()); }
    let t0 = Instant::now();
    h.stop.stop();
    while let Some(e) = h.events.recv().await { evs.push(e); }
    assert!(t0.elapsed() < Duration::from_secs(2), "SIGINT should end it at once; took {:?}", t0.elapsed());
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:130"), "{:?}", evs.last());
    assert!(!evs.iter().any(|e| matches!(e, RunEvent::Result { .. })));
    assert!(evs.len() < 200, "it stopped part-way: {} events", evs.len());
}

/// The chat fake with a stand-in for Gizai's MCP server (fake-mcp-server.py: it answers initialize and tools/list, and
/// tools/call with an error, which the fake takes as "no answer").
fn run_chat_fake(dir: &Path, prompt: &str) -> (Vec<(Duration, String)>, i32) {
    let config = dir.join("mcp.json");
    let server = serde_json::json!({ "mcpServers": { "gizai": { "command": "python3", "args": [format!("{TESTS}/fake-mcp-server.py"), "hints"] } } });
    std::fs::write(&config, server.to_string()).unwrap();
    let script = format!("{TESTS}/fake-claude-chat.py");
    run_fake("python3", &[&script, "--session-id", "C1", "--mcp-config", config.to_str().unwrap()], prompt)
}

/// The deltas of the chat's last text block, each with the time it came, and its finished text.
fn last_answer(lines: &[(Duration, String)]) -> (Vec<(Duration, String)>, String) {
    let mut deltas = vec![];
    let mut text = String::new();
    for (at, line) in lines {
        for e in chat_stream::parse_line(line) {
            match e {
                ChatEvent::BlockStart => deltas.clear(),
                ChatEvent::Delta { text } => deltas.push((*at, text)),
                ChatEvent::Text { text: t } => text = t,
                _ => {}
            }
        }
    }
    (deltas, text)
}

fn result_text(lines: &[(Duration, String)]) -> String {
    let last: Value = serde_json::from_str(&lines.last().unwrap().1).unwrap();
    assert_eq!(last["type"], "result");
    assert_eq!(last["is_error"], false);
    last["result"].as_str().unwrap().to_string()
}

#[test]
fn fake_chat_stream_answers_about_20_kb_in_small_pieces_one_every_10_ms() {
    let tmp = tempfile::tempdir().unwrap();
    let (lines, code) = run_chat_fake(tmp.path(), "How does KADE stand? FAKE_CHAT_STREAM");
    assert_eq!(code, 0);
    let (deltas, text) = last_answer(&lines);
    assert_eq!(deltas.iter().map(|(_, d)| d.as_str()).collect::<String>(), text, "the pieces make the answer");
    assert_eq!(result_text(&lines), text);
    assert!((20_000..22_000).contains(&text.len()), "about 20 KB: {} bytes", text.len());
    for want in ["# Where KADE stands", "\n## ", "\n- **Done:**", "\n```ts\n", "\n| Card | State |"] {
        assert!(text.contains(want), "a long Team Lead answer has {want:?}");
    }
    // pieces of 20 to 40 characters (the last one may be shorter)
    for (i, (_, d)) in deltas[..deltas.len() - 1].iter().enumerate() {
        assert!((20..=40).contains(&d.chars().count()), "piece {i} has {} characters", d.chars().count());
    }
    // one every 10 ms, from the first piece on: never faster, and not much slower
    let n = deltas.len();
    assert!(n > 500, "{n} pieces");
    let span = deltas[n - 1].0 - deltas[0].0;
    let want = Duration::from_millis(10 * (n as u64 - 1));
    assert!(span >= want - Duration::from_millis(20), "{n} pieces at 10 ms take {want:?}; took {span:?}");
    assert!(span < want * 3 / 2, "{n} pieces at 10 ms take {want:?}; took {span:?}");
    // the same answer every run
    let (again, _) = run_chat_fake(tmp.path(), "How does KADE stand? FAKE_CHAT_STREAM");
    assert_eq!(last_answer(&again).1, text);
}

#[test]
fn without_fake_chat_stream_the_chat_fake_answers_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let (lines, code) = run_chat_fake(tmp.path(), "How does KADE stand?");
    assert_eq!(code, 0);
    let (deltas, text) = last_answer(&lines);
    assert_eq!(text, "Here is the overview.");
    assert_eq!(deltas.len(), 2, "two halves, as before: {deltas:?}");
    assert_eq!(result_text(&lines), "Here is the overview.");
}
