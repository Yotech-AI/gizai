//! GA-191 in the app, with fake CLIs and local git repositories only: a run's events go to the window in batches (at most
//! one `run-events` message every 50 ms per run, none lost or doubled, also for a page opened mid-run, the last within
//! 50 ms of the run's end), `run_events` gives the events after a number, waiting cards start side by side within the
//! run limits, a copy from the linked folder holds the folder's lock and an install doesn't, `runs::pull` and
//! `board::tick` wait for the database off the async runtime's threads, the start logs each part's time once, and no
//! command but the six named ones runs on the main thread.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use gizai_core::model::*;
use gizai_core::{projects, runs as core_runs, settings, tasks, team};
use gizai_agents::stream::RunEvent;
use gizai_lib::runs::{self, Note, SeqEvent};
use gizai_lib::AppState;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fixtures");

fn git(dir: &Path, args: &[&str]) {
    assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}");
}

/// A main checkout that ignores vendor/ (copied into a new worktree, not committed).
fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), "vendor/\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    std::fs::create_dir(repo.join("vendor")).unwrap();
    std::fs::write(repo.join("vendor/autoload.php"), "<?php\n").unwrap();
    repo
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (ETXTBSY).
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// A fake `claude` that streams `n` text events at `per_second`, evenly spaced, then finishes like run-ok.
fn streaming_fake(dir: &Path, n: usize, per_second: u32) -> String {
    let path = dir.join(format!("streaming-claude-{n}.py"));
    write_script(&path, &format!(r#"#!/usr/bin/env python3
import json, sys, time
fx = '{FIXTURES}'
if '--input-format' in sys.argv:
    sys.stdin.readline()
    sys.stdout.write(open(fx + '/models-init.jsonl').read()); sys.stdout.flush()
    sys.stdin.read(); sys.exit(0)
sys.stdin.read()
print(json.dumps({{"type": "system", "subtype": "init", "session_id": "S1", "model": "claude-opus-5-5", "tools": ["Read"]}}), flush=True)
start = time.monotonic()
for i in range({n}):
    wait = start + i / {per_second} - time.monotonic()
    if wait > 0:
        time.sleep(wait)
    print(json.dumps({{"type": "assistant", "message": {{"role": "assistant", "content": [{{"type": "text", "text": "line %d" % i}}]}}, "session_id": "S1"}}), flush=True)
sys.stdout.write(''.join(open(fx + '/run-ok.jsonl').readlines()[-2:])); sys.stdout.flush()
"#));
    path.to_string_lossy().into_owned()
}

/// A temp folder by its real path, the way git and Gizai report it.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    tempfile::tempdir_in(base).unwrap()
}

/// The `run-events` messages the window got: when, for which run, and the batch.
type Batches = Arc<Mutex<Vec<(Instant, String, Vec<SeqEvent>)>>>;

fn listening(st: &mut AppState) -> Batches {
    let got: Batches = Arc::default();
    let g = got.clone();
    st.notify = Arc::new(move |n| {
        if let Note::RunEvents { run_id, events } = n {
            g.lock().unwrap().push((Instant::now(), run_id, events));
        }
    });
    got
}

fn seqs(events: &[SeqEvent]) -> Vec<u64> {
    events.iter().map(|e| e.seq).collect()
}

fn json(events: &[SeqEvent]) -> Vec<serde_json::Value> {
    events.iter().map(|e| serde_json::to_value(&e.event).unwrap()).collect()
}

async fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---- streams in batches ----

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_run_streaming_200_events_a_second_sends_at_most_20_messages_a_second_none_lost_or_doubled_also_for_a_page_opened_mid_run() {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let got = listening(&mut st);
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let bin = streaming_fake(tmp.path(), 400, 200);

    let (run_id, done) = runs::start(&st, &task, None, Some(bin), "manual").await.unwrap();
    // a page opened mid-run: it listens from now on, then asks for what came before (the Run panel's mergeEvents)
    until("the run streams", 30, || got.lock().unwrap().iter().filter(|b| b.1 == run_id).map(|b| b.2.len()).sum::<usize>() > 100).await;
    let heard_from = got.lock().unwrap().len();
    let history = runs::events_after(&st, &run_id, None);
    let summary = tokio::time::timeout(Duration::from_secs(60), done).await.unwrap().unwrap();
    let ended = Instant::now();
    assert_eq!(summary.status, "succeeded", "{summary:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let all: Vec<_> = got.lock().unwrap().iter().filter(|b| b.1 == run_id).cloned().collect();

    // none lost or doubled, in seq order: every number once, as the run's log has them
    let events: Vec<SeqEvent> = all.iter().flat_map(|b| b.2.clone()).collect();
    let total = events.len() as u64;
    assert!(total >= 400, "{total}");
    assert_eq!(seqs(&events), (0..total).collect::<Vec<_>>(), "each event once, in order");
    for b in &all {
        assert!(seqs(&b.2).windows(2).all(|w| w[0] < w[1]), "a batch is in seq order");
    }
    // after the run, run_events re-reads its log, which has every event but Gizai's own exit:<code> marker (as on main)
    let logged = runs::events_after(&st, &run_id, None);
    let without_exit: Vec<SeqEvent> = events.iter().filter(|e| !matches!(&e.event, RunEvent::Other { raw_type } if raw_type.starts_with("exit:"))).cloned().collect();
    assert_eq!(without_exit.len() + 1, events.len(), "one exit marker, at the end");
    assert_eq!(json(&without_exit), json(&logged), "what the window got is what run_events gives after the run");

    // at most 20 messages in any second (one per 50 ms), and really batched
    let times: Vec<Instant> = all.iter().map(|b| b.0).collect();
    let busiest = times.iter().map(|t| times.iter().filter(|u| **u >= *t && **u < *t + Duration::from_secs(1)).count()).max().unwrap();
    assert!(busiest <= 20, "{busiest} run-events messages in one second");
    let gaps: Vec<Duration> = times.windows(2).map(|w| w[1] - w[0]).collect();
    let streamed = times.last().unwrap().duration_since(times[0]);
    let rate = total as f64 / streamed.as_secs_f64();
    println!("GA-191 streams: {total} events in {} messages over {streamed:?} ({rate:.0} events/s), busiest second {busiest}, \
              shortest gap {:?}", all.len(), gaps.iter().min().unwrap());
    assert!(rate >= 150.0, "the fake streamed {rate:.0} events/s");
    assert!(all.len() as u64 * 5 <= total, "{} messages for {total} events", all.len());

    // the page opened mid-run: its history plus what it heard is every event once
    assert!(!history.is_empty() && (history.len() as u64) < total, "opened mid-run: {} of {total}", history.len());
    let heard: Vec<SeqEvent> = got.lock().unwrap()[heard_from..].iter().filter(|b| b.1 == run_id).flat_map(|b| b.2.clone()).collect();
    let mut merged: HashMap<u64, serde_json::Value> = HashMap::new();
    for e in history.iter().chain(heard.iter()) {
        let v = serde_json::to_value(&e.event).unwrap();
        if let Some(seen) = merged.insert(e.seq, v.clone()) {
            assert_eq!(seen, v, "one seq, one event");
        }
    }
    assert_eq!(merged.keys().copied().collect::<BTreeSet<_>>(), (0..total).collect::<BTreeSet<_>>(), "nothing lost between the history and the batches");

    // the last batch within 50 ms of the run's end
    let last = *times.last().unwrap();
    let after_end = last.saturating_duration_since(ended);
    println!("GA-191 streams: the last batch came {after_end:?} after the run's end");
    assert!(after_end <= Duration::from_millis(50), "the last batch came {after_end:?} after the run's end");
    assert_eq!(gizai_lib::stream::EVERY, Duration::from_millis(50));
    assert_eq!(gizai_lib::stream::queued(&st), (0, 0), "nothing left waiting");
}

#[tokio::test]
async fn run_events_after_a_number_returns_only_the_events_after_it_at_most_the_last_500() {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let got = listening(&mut st);
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let bin = streaming_fake(tmp.path(), 600, 3000);
    let (run_id, done) = runs::start(&st, &task, None, Some(bin), "manual").await.unwrap();
    // while it is live: only the events after the number asked
    until("the run streams", 30, || got.lock().unwrap().iter().map(|b| b.2.len()).sum::<usize>() > 50).await;
    let live = runs::events_after(&st, &run_id, Some(20));
    assert!(!live.is_empty() && live.iter().all(|e| e.seq > 20), "{:?}", seqs(&live).first());
    tokio::time::timeout(Duration::from_secs(60), done).await.unwrap().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let total = got.lock().unwrap().iter().map(|b| b.2.len()).sum::<usize>() as u64;
    assert!(total > 600, "{total}");

    // after the run its log is read: every event but Gizai's own exit:<code> marker
    let total = runs::events_for(&st, &run_id).len() as u64;
    let all = runs::events_after(&st, &run_id, None);
    assert_eq!(seqs(&all), (total - 500..total).collect::<Vec<_>>(), "all of them, at most the last 500");
    assert_eq!(seqs(&runs::events_after(&st, &run_id, Some(total - 11))), (total - 10..total).collect::<Vec<_>>());
    assert!(runs::events_after(&st, &run_id, Some(total - 1)).is_empty(), "nothing after the last");
    assert_eq!(seqs(&runs::events_after(&st, &run_id, Some(5))), (total - 500..total).collect::<Vec<_>>(), "after 5: still the last 500");
}

// ---- waiting cards start side by side ----

struct App {
    st: AppState,
    repo: PathBuf,
    project: String,
    _tmp: tempfile::TempDir,
}

/// Project KADE with a local repository, New worktrees copying vendor/ and running `setup`, the usual fake `claude`.
fn app(setup: &str) -> App {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let repo = git_repo(tmp.path());
    let project = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(),
        repo_path: Some(repo.to_string_lossy().into_owned()), ..Default::default() }).unwrap();
    let p = projects::get(&st.db, &project).unwrap();
    projects::update(&st.db, &st.you_id, &project, ProjectInput { name: p.name.clone(), key: p.key.clone(), repo_path: p.repo_path.clone(),
        default_branch: Some(p.default_branch.clone()), worktree_copy: Some(vec!["vendor/".into()]), worktree_install: Some(false),
        worktree_setup: Some(setup.into()), ..Default::default() }).unwrap();
    App { st, repo, project, _tmp: tmp }
}

impl App {
    fn agent(&self, name: &str, max_runs: i64) -> String {
        let team = team::list(&self.st.db).unwrap()[0].id.clone();
        team::add_agent(&self.st.db, &self.st.you_id, &team, AgentInput { name: name.into(), role_key: "backend".into(), max_runs: Some(max_runs),
            ..Default::default() }).unwrap()
    }
    /// A card waiting in To do, which hangs once its run starts.
    fn card(&self, title: &str) -> String {
        let team = team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap();
        let todo = team.states.iter().find(|s| s.name == "To do").unwrap().id.clone();
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: self.project.clone(), title: title.into(),
            description_md: "FAKE_HANG".into(), state_id: Some(todo), ..Default::default() }).unwrap()
    }
    fn runs_of(&self, task: &str) -> usize {
        core_runs::list_for_task(&self.st.db, task).unwrap().len()
    }
    fn worktree(&self, task: &str) -> PathBuf {
        self.st.data_dir.join("worktrees/KADE").join(tasks::get(&self.st.db, task).unwrap().identifier)
    }
}

/// The cards under way now: live or starting, each once.
fn under_way(st: &AppState) -> BTreeSet<String> {
    let mut cards: BTreeSet<String> = runs::live(st).into_iter().map(|l| l.task_id).collect();
    cards.extend(runs::starting(st));
    cards
}

/// Watches the cards under way every 20 ms until `stop`: the most at once, and when each card's run went live.
fn watch(a: &App, stop: Arc<AtomicBool>) -> tokio::task::JoinHandle<(usize, HashMap<String, Instant>)> {
    let st = a.st.clone();
    tokio::spawn(async move {
        let (mut most, mut live_at) = (0, HashMap::new());
        while !stop.load(Ordering::SeqCst) {
            most = most.max(under_way(&st).len());
            for l in runs::live(&st) {
                live_at.entry(l.task_id).or_insert_with(Instant::now);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        (most, live_at)
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_waiting_cards_each_with_a_5_second_install_start_within_a_second_of_each_other_and_none_starts_twice() {
    let a = app("sleep 5");
    a.agent("Backend Agent", 2);
    let (c1, c2) = (a.card("First"), a.card("Second"));
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = watch(&a, stop.clone());
    let t0 = Instant::now();
    // three pulls at once (a card that changed, a run's end, the minute's): each card is claimed once
    let (p1, p2, p3) = tokio::join!(runs::pull(&a.st), runs::pull(&a.st), runs::pull(&a.st));
    let pulled = t0.elapsed();
    until("both cards run", 30, || runs::live(&a.st).len() == 2).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    stop.store(true, Ordering::SeqCst);
    let (most, live_at) = watcher.await.unwrap();
    let mut started: Vec<String> = p1.iter().chain(&p2).chain(&p3).map(|(t, _)| t.clone()).collect();
    started.sort();
    let mut both = vec![c1.clone(), c2.clone()];
    both.sort();
    assert_eq!(started, both, "each card started by one pull");
    assert_eq!((a.runs_of(&c1), a.runs_of(&c2)), (1, 1), "no card started twice");
    assert_eq!(most, 2);
    let (t1, t2) = (live_at[&c1], live_at[&c2]);
    let apart = if t1 > t2 { t1 - t2 } else { t2 - t1 };
    println!("GA-191 side by side: the two cards went live {apart:?} apart, the pulls took {pulled:?}");
    assert!(apart < Duration::from_secs(1), "the cards started {apart:?} apart");
    assert!(pulled < Duration::from_secs(9), "the installs ran side by side, not one after the other: {pulled:?}");
    runs::stop_all(&a.st, Duration::from_secs(12)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn with_cards_starting_side_by_side_runs_at_once_and_an_agents_cards_at_once_still_hold() {
    // Runs at once 1: one card under way, also while it is still starting
    let a = app("sleep 2");
    settings::set(&a.st.db, "max_concurrent_runs", &1u32).unwrap();
    a.agent("Backend Agent", 3);
    let cards = [a.card("First"), a.card("Second"), a.card("Third")];
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = watch(&a, stop.clone());
    let (p1, p2) = tokio::join!(runs::pull(&a.st), runs::pull(&a.st));
    assert_eq!(p1.len() + p2.len(), 1, "one card started");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs::pull(&a.st).await.len(), 0, "Runs at once is full");
    stop.store(true, Ordering::SeqCst);
    let (most, _) = watcher.await.unwrap();
    assert_eq!(most, 1, "never more than Runs at once under way");
    assert_eq!(cards.iter().map(|c| a.runs_of(c)).sum::<usize>(), 1);
    runs::stop_all(&a.st, Duration::from_secs(12)).await;

    // an agent with one card at once, Runs at once 5: one of its cards under way at a time; another agent's card goes beside it
    let b = app("sleep 2");
    settings::set(&b.st.db, "max_concurrent_runs", &5u32).unwrap();
    let one = b.agent("Backend Agent", 1);
    let cards = [b.card("First"), b.card("Second")];
    let stop = Arc::new(AtomicBool::new(false));
    let watcher = watch(&b, stop.clone());
    let (p1, p2) = tokio::join!(runs::pull(&b.st), runs::pull(&b.st));
    assert_eq!(p1.len() + p2.len(), 1, "one card of the agent with one card at once");
    stop.store(true, Ordering::SeqCst);
    let (most, _) = watcher.await.unwrap();
    assert_eq!(most, 1);
    assert_eq!(cards.iter().map(|c| b.runs_of(c)).sum::<usize>(), 1);
    assert!(runs::live(&b.st).iter().all(|l| l.agent_id == one));
    runs::stop_all(&b.st, Duration::from_secs(12)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_copy_from_the_linked_folder_holds_the_folders_lock_and_the_install_does_not() {
    let tmp_marker = real_tempdir();
    let marker = tmp_marker.path().join("installing");
    let a = app(&format!("touch '{}'; sleep 3", marker.display()));
    a.agent("Backend Agent", 1);
    let card = a.card("First");
    let lock = gizai_lib::code::folder_lock(&a.st, &a.repo);
    // the folder is being updated (an update holds it): the card's copy waits
    let held = lock.clone().write_owned().await;
    let st = a.st.clone();
    let pull = tokio::spawn(async move { runs::pull(&st).await.len() });
    until("the card's worktree is made", 30, || a.worktree(&card).join(".git").exists()).await;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert!(!a.worktree(&card).join("vendor").exists(), "the copy waits for the folder's update");
    assert!(!marker.exists(), "and so does the install after it");
    drop(held);
    // copied, and the install (the setup command) runs: the folder's lock is free meanwhile
    until("the install runs", 30, || marker.exists()).await;
    assert!(a.worktree(&card).join("vendor/autoload.php").is_file(), "copied first");
    assert!(runs::live(&a.st).is_empty(), "still installing");
    assert!(lock.try_write().is_ok(), "an install doesn't hold the folder's lock");
    assert_eq!(tokio::time::timeout(Duration::from_secs(30), pull).await.unwrap().unwrap(), 1);
    assert_eq!(runs::live(&a.st).len(), 1);
    runs::stop_all(&a.st, Duration::from_secs(12)).await;
}

// ---- off the async runtime's threads ----

#[test]
fn pull_and_the_board_tick_wait_for_the_database_off_the_async_runtimes_threads() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = team::add_agent(&st.db, &st.you_id, &team::list(&st.db).unwrap()[0].id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), board_check_minutes: Some(5), ..Default::default() }).unwrap();
    assert!(!lead.is_empty());
    // every read-only connection busy with a long read (at most 5 s, so a wrong build fails instead of hanging)
    let release = Arc::new(AtomicBool::new(false));
    let open = Arc::new(Barrier::new(gizai_core::db::READERS + 1));
    let readers: Vec<_> = (0..gizai_core::db::READERS).map(|_| {
        let (st, release, open) = (st.clone(), release.clone(), open.clone());
        std::thread::spawn(move || st.db.read(|_| {
            open.wait();
            let t0 = Instant::now();
            while !release.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(())
        }).unwrap())
    }).collect();
    open.wait();
    // a runtime with one thread: when pull or tick waited for the database on it, nothing else on it could go on
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let (s1, s2) = (st.clone(), st.clone());
        let pull = tokio::spawn(async move { runs::pull(&s1).await.len() });
        let tick = tokio::spawn(async move { gizai_lib::board::tick(&s2, gizai_core::ids::now_ms()).await.is_some() });
        let t0 = Instant::now();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let slept = t0.elapsed();
        assert!(slept < Duration::from_millis(1000), "the runtime's thread was held for {slept:?}");
        assert!(!pull.is_finished() && !tick.is_finished(), "both wait for a reader");
        release.store(true, Ordering::SeqCst);
        assert_eq!(tokio::time::timeout(Duration::from_secs(10), pull).await.unwrap().unwrap(), 0);
        tokio::time::timeout(Duration::from_secs(20), tick).await.unwrap().unwrap();
    });
    for r in readers {
        r.join().unwrap();
    }
}

// ---- a timed start ----

#[test]
fn the_start_logs_each_parts_time_once() {
    // the child: opens the data once, with its log on stderr
    if let Ok(dir) = std::env::var("GIZAI_GA191_OPEN") {
        let _st = gizai_lib::test_state(Path::new(&dir));
        return;
    }
    let tmp = real_tempdir();
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["the_start_logs_each_parts_time_once", "--exact", "--nocapture", "--test-threads=1"])
        .env("GIZAI_GA191_OPEN", tmp.path()).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let lines: Vec<&str> = stderr.lines().filter(|l| l.contains("the start took")).collect();
    assert_eq!(lines.len(), 1, "once: {stderr}");
    let line = lines[0];
    println!("GA-191 start: {line}");
    let ms = |after: &str| -> Option<u64> {
        let rest = &line[line.find(after)? + after.len()..];
        rest.split(" ms").next()?.trim().parse().ok()
    };
    assert!(ms("the start took ").is_some(), "{line}");
    for part in ["the lock ", "the database ", "the seed ", "the recovery ", "the housekeeping "] {
        assert!(ms(part).is_some(), "{part}in {line}");
        assert_eq!(line.matches(part).count(), 1, "{part}once: {line}");
    }
}

// ---- the commands ----

/// A Tauri command as written: its name, whether it is async, and its body (from `#[tauri::command]` to the next one).
fn commands(src: &str) -> Vec<(String, bool, String)> {
    src.match_indices("#[tauri::command]").map(|(at, _)| {
        let rest = &src[at..];
        let fn_at = rest.find("fn ").unwrap();
        let name: String = rest[fn_at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let end = rest[1..].find("#[tauri::command]").map_or(rest.len(), |e| e + 1);
        (name, rest[..fn_at].contains("async"), rest[..end].to_string())
    }).collect()
}

fn rs_files(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push((p.clone(), std::fs::read_to_string(&p).unwrap()));
        }
    }
}

#[test]
fn only_the_six_named_commands_are_not_async_each_says_why_and_the_window_gets_the_lean_lists_and_the_batches() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![];
    rs_files(&src, &mut files);
    let mut sync: Vec<String> = vec![];
    let mut all = 0;
    for (_, text) in &files {
        for (name, is_async, _) in commands(text) {
            all += 1;
            if !is_async {
                sync.push(name);
            }
        }
    }
    sync.sort();
    assert!(all > 100, "{all} commands");
    assert_eq!(sync, ["close_answer", "exit_app", "set_unsaved_edits", "terminal_ack", "terminal_input", "terminal_resize"]);
    // each says why it stays on the main thread
    let text = |name: &str| files.iter().find(|(p, _)| p.ends_with(name)).unwrap().1.clone();
    let before = |file: &str, f: &str| { let t = text(file); let at = t.find(&format!("fn {f}(")).unwrap(); t[at.saturating_sub(900)..at].to_string() };
    for (file, f) in [("lib.rs", "exit_app"), ("unsaved.rs", "set_unsaved_edits"), ("unsaved.rs", "close_answer")] {
        assert!(before(file, f).contains("On the main thread") || before(file, f).contains("on the main thread"), "{f} says why");
    }
    assert!(before("terminal.rs", "terminal_input").contains("order") && before("terminal.rs", "terminal_input").contains("no database or file work"));

    let cmds = text("commands.rs");
    let body = |f: &str| commands(&cmds).into_iter().find(|(n, ..)| n == f).unwrap_or_else(|| panic!("no command {f}")).2;
    assert!(body("list_tasks").contains("tasks::list_lean(") && !body("list_tasks").contains("tasks::list("), "list_tasks: acceptanceMd null");
    assert!(body("get_team").contains("team::get_lean(") && !body("get_team").contains("team::get("), "get_team: instructionsMd null");
    assert!(body("get_task").contains("tasks::get("), "get_task keeps them");
    assert!(body("get_agent").contains("team::agent("), "get_agent keeps them");
    assert!(body("run_events").contains("after_seq: Option<u64>") && body("run_events").contains("events_after("));
    // the agents' and the Team Lead's tools keep the full rows
    let tools = text("tools/read.rs") + &text("tools/resolve.rs");
    assert!(!tools.contains("list_lean") && !tools.contains("get_lean"), "the tools read tasks::list and team::get");
    assert!(text("tools/read.rs").contains("tasks::list(cx.db()"));
    // the window's two events, and the old ones gone
    let lib = text("lib.rs");
    assert!(lib.contains("app.emit(\"run-events\"") && lib.contains("app.emit(\"chat-events\""));
    assert!(!lib.contains("\"run-event\"") && !lib.contains("\"chat-event\""));
}

#[tokio::test]
async fn the_agents_get_task_tool_still_gives_the_acceptance_criteria() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = team::add_agent(&st.db, &st.you_id, &team::list(&st.db).unwrap()[0].id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), ..Default::default() }).unwrap();
    let task = gizai_lib::test_task(&st, "", "backend");
    tasks::update(&st.db, &st.you_id, &task, TaskPatch { acceptance_md: Some("- [ ] A header row".into()), ..Default::default() }).unwrap();
    let ident = tasks::get(&st.db, &task).unwrap().identifier;
    let got = gizai_lib::tools::call(&st, &lead, "get_task", serde_json::json!({"task": ident})).await.unwrap();
    assert_eq!(got["task"]["acceptance_md"], "- [ ] A header row");
    let list = gizai_lib::tools::call(&st, &lead, "list_tasks", serde_json::json!({})).await.unwrap();
    assert_eq!(list["tasks"][0]["task"], ident.as_str(), "{list}");
}
