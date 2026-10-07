use gizai_agents::{claude::ClaudeArgs, process::{spawn, Caps}, stream::RunEvent};
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
    let mut h = spawn(&args("p"), tmp.path(), &log, Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Result { .. })));
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type == "exit:0"));
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 6);
    assert!(std::fs::read_to_string(tmp.path().join("r.stderr.log")).unwrap().contains("fake claude done"));
}

#[tokio::test]
async fn stop_ends_a_hanging_run() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("hang"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
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
    let mut h = spawn(&args("stubborn"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
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
    let mut h = spawn(&args("p"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 1 }).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:tools")), "{evs:?}");
    assert!(matches!(evs.last(), Some(RunEvent::Other { raw_type }) if raw_type.starts_with("exit:")));
}

#[tokio::test]
async fn the_time_cap_stops_the_run_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = spawn(&args("hang"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(1), max_tool_calls: 80 }).unwrap();
    let evs = drain(&mut h).await;
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type == "cap_exceeded:time")), "{evs:?}");
}

#[tokio::test]
async fn every_tool_call_counts_even_several_in_one_message() {
    let tmp = tempfile::tempdir().unwrap();
    // the fixture has 2 tool calls: a cap of 2 is not exceeded
    let mut h = spawn(&args("p"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 2 }).unwrap();
    let evs = drain(&mut h).await;
    assert!(!evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type.starts_with("cap_exceeded"))), "{evs:?}");
}

#[tokio::test]
async fn a_background_child_holding_stdout_does_not_hang_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let t0 = Instant::now();
    let mut h = spawn(&args("orphan"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
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
    let mut h = spawn(&args("stubborn"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
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
    let mut h = spawn(&args("leftover"), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
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
    let r = rt.block_on(async { spawn::<RunEvent>(&a, tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 1 }).map(|_| ()) });
    assert!(matches!(r, Err(gizai_agents::AgentError::Spawn(_))));
}

#[tokio::test]
async fn a_huge_prompt_goes_in_on_stdin() {
    let tmp = tempfile::tempdir().unwrap();
    let big = "x".repeat(200_000);
    let mut h = spawn(&args(&big), tmp.path(), &tmp.path().join("r.jsonl"), Caps { max_time: Duration::from_secs(60), max_tool_calls: 80 }).unwrap();
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
