//! GA-202 end to end with the fake `claude`: Memory for agent runs.
//! - Every system: with a fake reading over 80% of the limit, or under 2 GB free, no run starts (the queue, Run and
//!   Continue); the card waits with the reason, unheld, and starts by itself once memory frees up.
//! - Linux with a systemd user session: a run, a worktree's setup command, a chat answer and a board check each run in a
//!   scope in the test Gizai's own slice (`/proc/<pid>/cgroup`), the test's own process doesn't; the slice's MemoryMax and
//!   MemoryHigh follow the setting without a restart; at a 1 GB limit a run's command that asks for 2 GB is ended, another
//!   run in the slice and the test keep running, and the card's activity names the limit; Stop, Continue and quitting work
//!   as before. Each test's slice is named after its own data folder and is stopped and reverted when the test ends.
//!   Without a systemd user session (CI runners) these say so and pass; GIZAI_TEST_SLICE=1 makes that a failure.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_agents::run_memory::{GB, Reading};
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

/// A script in `dir` that runs `before` (bash) and then `exec`s `then` with its arguments, its stdin kept.
fn wrapper(dir: &Path, name: &str, before: &str, then: &str) -> String {
    let path = dir.join(name);
    let script = format!("#!/usr/bin/env bash\n{before}\nexec '{then}' \"$@\"\n");
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
    fn card(&self, description: &str) -> String {
        let t = gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend");
        gizai_core::tasks::update(&self.st.db, &self.st.you_id, &t, TaskPatch { priority: Some(2), description_md: Some(description.into()), ..Default::default() }).unwrap();
        t
    }
    fn task(&self, t: &str) -> Task { gizai_core::tasks::get(&self.st.db, t).unwrap() }
    fn runs_of(&self, t: &str) -> Vec<Run> { gizai_core::runs::list_for_task(&self.st.db, t).unwrap() }
    fn live_cards(&self) -> Vec<String> {
        let mut v: Vec<String> = runs::live(&self.st).into_iter().map(|l| l.task_id).collect();
        v.sort();
        v
    }
    fn backend(&self) -> String {
        gizai_core::team::all_agents(&self.st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap().1.actor_id
    }
    /// Backend Agent takes cards by itself, `n` at once.
    fn takes_cards(&self, n: i64) {
        let id = self.backend();
        let m = gizai_core::team::agent(&self.st.db, &id).unwrap();
        gizai_core::team::update_agent(&self.st.db, &self.st.you_id, &id, AgentInput { name: m.name, role_key: m.role_key, wakeup: "on_assign".into(),
            max_runs: Some(n), ..Default::default() }).unwrap();
    }
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(30), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---- Every system: the start check ----

#[tokio::test]
async fn with_a_fake_reading_over_80_percent_or_under_2_gb_free_no_run_starts_the_card_waits_with_the_reason_and_starts_by_itself_once_memory_frees_up() {
    let a = app();
    gizai_core::settings::set(&a.st.db, gizai_lib::run_memory::KEY, &10u32).unwrap();
    let first = a.card("FAKE_HANG");
    a.takes_cards(3);
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [first.clone()], "memory is fine: the first card starts");

    // the runs use 8.5 of their 10 GB: no run starts
    let used = Reading { used: Some(85 * GB / 10), available: Some(40 * GB) };
    a.st.run_memory.fake_reading(Some(used));
    let waiting = a.card("FAKE_HANG");
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [first.clone()], "the queue starts nothing");
    let t = a.task(&waiting);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), a.runs_of(&waiting).len()), ("To do", None, 0), "it only has to wait");
    let why = "Waiting for memory: agent runs use 8.5 of 10 GB (Settings → Runs)";
    assert_eq!(runs::start(&a.st, &waiting, None, Some(FAKE.into()), "manual").await.unwrap_err(), why, "Run waits with the reason");
    assert_eq!((a.task(&waiting).state_name.as_str(), a.runs_of(&waiting).len()), ("To do", 0));
    // the Team Lead's slots say why, and after an hour of it the board check has a finding
    assert_eq!(gizai_lib::board::slots_json(&a.st)["waiting_for_memory"], serde_json::json!(why));
    let later = gizai_core::ids::now_ms() + 61 * 60_000;
    let found: Vec<(String, String, String)> = gizai_lib::board::findings(&a.st, later).unwrap().into_iter().map(|f| (f.task_id, f.code, f.reason)).collect();
    assert!(found.iter().any(|(t, code, reason)| t == &waiting && code == "memory"
                && reason == "Gizai has been waiting for memory for over an hour: agent runs use 8.5 of 10 GB (Settings → Runs)"), "{found:?}");

    // exactly 80% is no reason to wait; over it is
    a.st.run_memory.fake_reading(Some(Reading { used: Some(8 * GB), available: Some(40 * GB) }));
    assert_eq!(gizai_lib::run_memory::wait_reason(&a.st), None);
    a.st.run_memory.fake_reading(Some(Reading { used: Some(8 * GB + 1), available: Some(40 * GB) }));
    assert!(gizai_lib::run_memory::wait_reason(&a.st).is_some());

    // the computer has 1.5 GB free: no run starts either
    a.st.run_memory.fake_reading(Some(Reading { used: Some(GB), available: Some(3 * GB / 2) }));
    runs::pull(&a.st).await;
    assert_eq!(a.live_cards(), [first.clone()]);
    let why = "Waiting for memory: this computer has 1.5 GB free, less than 2 GB (Settings → Runs)";
    assert_eq!(runs::start(&a.st, &waiting, None, Some(FAKE.into()), "manual").await.unwrap_err(), why);
    assert_eq!(gizai_lib::board::slots_json(&a.st)["waiting_for_memory"], serde_json::json!(why));
    // exactly 2 GB free is enough
    a.st.run_memory.fake_reading(Some(Reading { used: Some(GB), available: Some(2 * GB) }));
    assert_eq!(gizai_lib::run_memory::wait_reason(&a.st), None);
    a.st.run_memory.fake_reading(Some(used));

    // memory frees up: the next time the queue looks (here: when the first run ends), the waiting card starts by itself
    a.st.run_memory.fake_reading(None);
    let stopped = runs::live(&a.st).remove(0);
    runs::stop(&a.st, &stopped.run_id);
    until("the waiting card starts by itself", || a.live_cards() == [waiting.clone()]).await;
    assert_eq!(a.task(&waiting).state_name, "In progress");
    assert_eq!(gizai_lib::board::slots_json(&a.st)["waiting_for_memory"], serde_json::Value::Null);

    // Continue waits too
    let live = runs::live(&a.st).remove(0);
    runs::stop(&a.st, &live.run_id);
    until("the run stops", || runs::live(&a.st).is_empty()).await;
    a.st.run_memory.fake_reading(Some(used));
    let err = runs::continue_run(&a.st, &live.run_id, Some(FAKE.into())).await.unwrap_err();
    assert_eq!(err, "Waiting for memory: agent runs use 8.5 of 10 GB (Settings → Runs)");
    assert!(runs::live(&a.st).is_empty());
    a.st.run_memory.fake_reading(None);
    runs::stop_all(&a.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_test_gizai_never_waits_for_the_test_machines_memory_and_runs_as_before() {
    // test_state doesn't start the slice or read the memory: no fake reading, no wait, and the run starts as today
    let a = app();
    assert_eq!(gizai_lib::run_memory::wait_reason(&a.st), None);
    assert!(a.st.run_memory.slice().is_none());
    let t = a.card("");
    let s = runs::run_once(&a.st, &t, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded");
}

// ---- Linux with a systemd user session: the test Gizai's own slice ----

/// The test Gizai's slice, set up now with a limit of `gb` (as Gizai's start does, for the test's own data folder), and
/// stopped and reverted when the test ends. None where this Linux has no systemd user session.
#[cfg(target_os = "linux")]
struct Own(gizai_agents::run_memory::Slice);

#[cfg(target_os = "linux")]
impl Drop for Own {
    fn drop(&mut self) {
        let _ = self.0.remove();
    }
}

#[cfg(target_os = "linux")]
fn own_slice(st: &AppState, limit: u64) -> Option<Own> {
    st.run_memory.enable(&gizai_agents::run_memory::slice_name(&st.data_dir), limit);
    match st.run_memory.slice() {
        Some(s) => Some(Own(s)),
        None => {
            assert!(std::env::var_os("GIZAI_TEST_SLICE").is_none(), "GIZAI_TEST_SLICE is set, but there is no slice (see stderr)");
            eprintln!("skipped: no slice for agent runs on this Linux");
            None
        }
    }
}

/// The cgroup folders in a file of `/proc/<pid>/cgroup` copies (the `0::` lines).
#[cfg(target_os = "linux")]
fn cgroups_in(file: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(file).unwrap_or_default().lines().filter_map(|l| l.strip_prefix("0::"))
        .map(|p| Path::new("/sys/fs/cgroup").join(p.trim().trim_start_matches('/'))).collect()
}

#[cfg(target_os = "linux")]
fn cgroup_of(pid: u32) -> PathBuf {
    cgroups_in(Path::new(&format!("/proc/{pid}/cgroup"))).remove(0)
}

#[cfg(target_os = "linux")]
fn in_a_scope_of(at: &Path, slice: &gizai_agents::run_memory::Slice) -> bool {
    at.parent() == Some(slice.dir()) && at.extension().is_some_and(|e| e == "scope")
}

/// The processes still in the slice, in any of its scopes.
#[cfg(target_os = "linux")]
fn procs_in(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_to_string(dir.join("cgroup.procs")).unwrap_or_default().lines().map(String::from).collect();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().is_dir() {
            out.extend(procs_in(&e.path()));
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| !s[s.rfind(')').unwrap() + 2..].starts_with('Z'))
}

/// The shim binary: target/debug/gizai-mcp (built by `cargo test --workspace`; built here when missing).
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_run_its_worktrees_setup_command_a_chat_answer_and_a_board_check_each_run_in_a_scope_in_the_slice_and_gizai_doesnt() {
    const CHAT_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py");
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    let Some(slice) = own_slice(&st, 2 * GB) else { return };
    let s = &slice.0;
    assert_eq!(s.name(), gizai_agents::run_memory::slice_name(&st.data_dir), "the test Gizai's own slice");
    assert!(st.run_memory.enforced());
    let record = |what: &str| (tmp.path().join(format!("{what}-cgroups")), format!("cat /proc/self/cgroup >> '{}'", tmp.path().join(format!("{what}-cgroups")).display()));

    // a run, after its worktree's setup command
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let (setup_file, setup_cmd) = record("setup");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), repo_path: p.repo_path.clone(),
        default_branch: Some(p.default_branch.clone()), worktree_setup: Some(setup_cmd), ..Default::default() }).unwrap();
    let (run_file, run_cmd) = record("run");
    let run_bin = wrapper(tmp.path(), "run-claude.sh", &run_cmd, FAKE);
    let summary = runs::run_once(&st, &task, None, Some(run_bin)).await.unwrap();
    assert_eq!(summary.status, "succeeded", "{summary:?}");
    let at = cgroups_in(&setup_file);
    assert!(at.len() == 1 && in_a_scope_of(&at[0], s), "the setup command: {at:?}");
    let at = cgroups_in(&run_file);
    assert!(at.len() == 1 && in_a_scope_of(&at[0], s), "the run: {at:?}");

    // a chat answer and a board check of the Team Lead
    let (chat_file, chat_cmd) = record("chat");
    let chat_bin = wrapper(tmp.path(), "chat-claude.sh", &chat_cmd, CHAT_FAKE);
    gizai_core::settings::set(&st.db, "claude_bin", &chat_bin).unwrap();
    st.mcp_shim = Some(shim());
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), board_check_minutes: Some(15), ..Default::default() }).unwrap();
    let _server = gizai_lib::mcp::start(&st).unwrap();
    let (_, done) = gizai_lib::chat::send(&st, None, "How are the cards?".into(), None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), done).await.expect("the answer finished").unwrap();
    let at = cgroups_in(&chat_file);
    assert!(at.len() == 1 && in_a_scope_of(&at[0], s), "the chat answer: {at:?}");

    // a card on hold is a finding: the board check starts
    gizai_core::tasks::update(&st.db, &st.you_id, &task, TaskPatch { hold: Some("blocked".into()), hold_reason: Some("Needs a key".into()), ..Default::default() }).unwrap();
    let t0 = gizai_core::ids::now_ms();
    let check = match gizai_lib::board::tick(&st, t0).await {
        Some(h) => h,
        None => gizai_lib::board::tick(&st, t0 + 16 * 60_000).await.expect("a board check started"),
    };
    let checked = tokio::time::timeout(Duration::from_secs(30), check).await.expect("the check finished").unwrap();
    assert_eq!(checked.status, "succeeded", "{checked:?}");
    let at = cgroups_in(&chat_file);
    assert!(at.len() == 2 && in_a_scope_of(&at[1], s), "the board check: {at:?}");
    assert_ne!(at[0], at[1], "a scope of its own each");

    // Gizai's own process (here the test's) stays outside the slice
    assert!(!cgroup_of(std::process::id()).starts_with(s.dir()));
    // and nothing is left in the slice
    assert_eq!(procs_in(s.dir()), Vec::<String>::new());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn in_the_slice_a_setup_command_with_shell_variables_works_as_it_does_outside_it() {
    // Must stay the same: what runs in the slice gets the same arguments as before. A setup command goes to bash -c.
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    std::fs::write(repo.join("app.ini.example"), "key=\n").unwrap();
    std::process::Command::new("git").args(["add", "app.ini.example"]).current_dir(&repo).status().unwrap();
    std::process::Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "example"]).current_dir(&repo).status().unwrap();
    // a usual setup command: every *.example copied to its name without .example, and bash's own pid
    let setup = "for f in *.example; do cp \"$f\" \"${f%.example}\"; done && echo \"pid $$\" > setup.out";
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), repo_path: p.repo_path.clone(),
        default_branch: Some(p.default_branch.clone()), worktree_setup: Some(setup.into()), ..Default::default() }).unwrap();
    let prepared = |summary: &runs::RunSummary| {
        assert_eq!(summary.status, "succeeded", "{summary:?}");
        let wt = PathBuf::from(gizai_core::runs::get(&st.db, &summary.run_id).unwrap().worktree_path.unwrap());
        assert_eq!(std::fs::read_to_string(wt.join("app.ini")).unwrap(), "key=\n");
        let out = std::fs::read_to_string(wt.join("setup.out")).unwrap();
        assert!(out.strip_prefix("pid ").is_some_and(|p| p.trim().parse::<u32>().is_ok()), "{out}");
    };
    // outside the slice, as before GA-202
    prepared(&runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap());

    // in the slice: the same
    let Some(_slice) = own_slice(&st, 2 * GB) else { return };
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let summary = runs::run_once(&st, &task, None, Some(FAKE.into())).await
        .unwrap_or_else(|e| panic!("the setup command works outside the slice, but here: {e}"));
    prepared(&summary);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn in_the_slice_the_team_leads_instructions_with_dollar_signs_reach_claude_code_as_written() {
    // Must stay the same: the Team Lead's instructions (written by you) go to Claude Code's --append-system-prompt, and
    // reach it unchanged in the slice as outside it.
    const CHAT_FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py");
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    let Some(_slice) = own_slice(&st, 2 * GB) else { return };
    let instructions = "Run `php artisan test --filter=${TEST}` before you say a card is done; a card under $$5 is small.";
    gizai_core::settings::set(&st.db, "claude_bin", &CHAT_FAKE.to_string()).unwrap();
    st.mcp_shim = Some(shim());
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        instructions_md: Some(instructions.into()), ..Default::default() }).unwrap();
    let _server = gizai_lib::mcp::start(&st).unwrap();
    let (_, done) = gizai_lib::chat::send(&st, None, "How are the cards?".into(), None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), done).await.expect("the answer finished").unwrap();
    let call: serde_json::Value = std::fs::read_to_string(st.data_dir.join("chat/fake-calls.jsonl")).unwrap().lines().last()
        .map(|l| serde_json::from_str(l).unwrap()).unwrap();
    let argv: Vec<String> = serde_json::from_value(call["argv"].clone()).unwrap();
    let sys = &argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
    assert!(sys.contains(instructions), "{sys}");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn in_the_slice_a_cards_text_with_dollar_signs_reaches_a_cli_that_takes_the_prompt_as_an_argument_as_written() {
    // Must stay the same: another coding CLI (OpenCode, for example) gets the prompt, with the card's text, as an
    // argument ({prompt}); in the slice it reaches it as written, `$` and `%` included, as outside it.
    const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
    let a = app();
    let Some(slice) = own_slice(&a.st, 2 * GB) else { return };
    let text = "Run `php artisan test --filter=${TEST}` in $HOME/app; a card under $$5 is small, cost: $5; 100% done, %h %u stay.";
    let task = a.card(text);
    let cgroups = a.tmp.path().join("cli-cgroups");
    let bin = wrapper(a.tmp.path(), "opencode.sh", &format!("cat /proc/self/cgroup >> '{}'", cgroups.display()), FAKE_CLI);
    let mut list = gizai_core::clis::list(&a.st.db).unwrap();
    // FAKE_TEMP=1: the fake writes the whole prompt it got to stderr
    list.push(gizai_core::clis::Cli { name: "OpenCode".into(), kind: "other".into(), command: bin,
        env: vec!["FAKE_KIND=other".into(), "FAKE_TEMP=1".into()], args: "run {prompt}".into(), ..Default::default() });
    let cli = gizai_lib::clis::save(&a.st, list).unwrap().into_iter().find(|c| c.cli.name == "OpenCode").unwrap().cli.id;
    let backend = a.backend();
    let m = gizai_core::team::agent(&a.st.db, &backend).unwrap();
    gizai_core::team::update_agent(&a.st.db, &a.st.you_id, &backend, AgentInput { name: m.name, role_key: m.role_key, adapter: cli,
        ..Default::default() }).unwrap();

    let summary = runs::run_once(&a.st, &task, None, None).await.unwrap();
    assert_eq!(summary.status, "succeeded", "{summary:?}");
    let at = cgroups_in(&cgroups);
    assert!(at.len() == 1 && in_a_scope_of(&at[0], &slice.0), "the CLI ran in the slice: {at:?}");
    let run = gizai_core::runs::get(&a.st.db, &summary.run_id).unwrap();
    let err = std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    assert!(err.contains("argv: run ") && err.contains("prompt chars: 0"), "the prompt came as an argument: {err}");
    assert!(err.contains(text), "the card's text as written: {err}");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn the_slices_memory_max_is_the_setting_and_memory_high_90_percent_and_saving_a_new_value_changes_both_without_a_restart() {
    use gizai_agents::run_memory::limits_for;
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, gizai_lib::run_memory::KEY, &3u32).unwrap();
    // as Gizai's start does: the slice for its own data folder, with the setting, set up on a thread of its own
    gizai_lib::run_memory::start(&st);
    let Some(slice) = st.run_memory.slice().map(Own) else {
        assert!(std::env::var_os("GIZAI_TEST_SLICE").is_none(), "GIZAI_TEST_SLICE is set, but there is no slice");
        return;
    };
    let s = &slice.0;
    assert_eq!(s.name(), gizai_agents::run_memory::slice_name(&st.data_dir));
    assert_eq!(s.limits(), (Some(3 * GB), Some(limits_for(3 * GB).1)));
    let settings = runs::get_settings(&st);
    assert_eq!((settings.runs_memory_gb, settings.runs_memory_enforced), (Some(3), true), "Settings → Runs says it is enforced");

    runs::save_settings(&st, &runs::Settings { runs_memory_gb: Some(5), ..settings.clone() }).unwrap();
    assert_eq!(s.limits(), (Some(5 * GB), Some(limits_for(5 * GB).1)), "both change at once");
    assert_eq!(gizai_lib::run_memory::limit(&st), 5 * GB);
    // a refused value changes nothing
    assert!(runs::save_settings(&st, &runs::Settings { runs_memory_gb: Some(1), ..settings }).is_err());
    assert_eq!(s.limits(), (Some(5 * GB), Some(limits_for(5 * GB).1)));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_test_gizai_with_its_own_data_folder_uses_its_own_slice() {
    let (one, two) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (a, b) = (gizai_lib::test_state(one.path()), gizai_lib::test_state(two.path()));
    let Some(sa) = own_slice(&a, 2 * GB) else { return };
    let Some(sb) = own_slice(&b, 3 * GB) else { return };
    assert_ne!(sa.0.name(), sb.0.name());
    assert_ne!(sa.0.dir(), sb.0.dir());
    // each keeps its own limit
    assert_eq!((sa.0.limits().0, sb.0.limits().0), (Some(2 * GB), Some(3 * GB)));
    // and neither is the slice of the Gizai these tests run in (if it runs one): its processes aren't in them
    let own = cgroup_of(std::process::id());
    assert!(!own.starts_with(sa.0.dir()) && !own.starts_with(sb.0.dir()));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn at_a_1_gb_limit_a_runs_command_that_asks_for_2_gb_is_ended_another_run_and_the_test_keep_running_and_the_activity_names_the_limit() {
    let a = app();
    let Some(slice) = own_slice(&a.st, GB) else { return };
    let s = &slice.0;
    // another run in the slice, which keeps working
    let pid_file = a.tmp.path().join("other-pid");
    let other_bin = wrapper(a.tmp.path(), "other-claude.sh", &format!("echo $$ > '{}'", pid_file.display()), FAKE);
    let other = a.card("FAKE_HANG");
    let (other_run, other_done) = runs::start(&a.st, &other, None, Some(other_bin), "manual").await.unwrap();
    until("the other run started", || std::fs::read_to_string(&pid_file).is_ok_and(|p| p.ends_with('\n'))).await;
    let other_pid: u32 = std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
    assert!(in_a_scope_of(&cgroup_of(other_pid), s));

    // a run whose build asks for 2 GB; Claude Code itself goes on after the failed command, and finishes
    let big_bin = wrapper(a.tmp.path(), "big-claude.sh", "python3 -c \"b = b'x' * (2 << 30)\"\necho \"build: $?\" >&2", FAKE);
    let big = a.card("");
    let (big_run, big_done) = runs::start(&a.st, &big, None, Some(big_bin), "manual").await.unwrap();
    let summary = tokio::time::timeout(Duration::from_secs(120), big_done).await.expect("the run finished").unwrap();
    assert_eq!(summary.status, "succeeded", "{summary:?}");
    let run = gizai_core::runs::get(&a.st.db, &big_run).unwrap();
    let stderr = std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    assert!(stderr.contains("build: 137"), "Linux ended the build (SIGKILL): {stderr}");

    // the card's activity says so, as Gizai, with the limit and where to change it
    let said = "saw Linux end a process in Backend Agent's run: the agent runs reached their memory limit of 1 GB together \
                (Settings → Runs → Memory for agent runs)";
    let activity = gizai_core::tasks::activity(&a.st.db, &big).unwrap();
    let lines: Vec<&ChangeEntry> = activity.iter().filter(|e| e.diff.get("cleanup").and_then(|c| c.as_str()) == Some(said)).collect();
    assert_eq!(lines.len(), 1, "once: {activity:?}");
    // and the run's own output, at its end
    let note = "Linux ended a process in this run: Gizai's agent runs reached their memory limit of 1 GB together. \
                Change it in Settings → Runs → Memory for agent runs.";
    let notes: Vec<String> = runs::events_for(&a.st, &big_run).into_iter().filter_map(|e| match e.event {
        gizai_agents::stream::RunEvent::Note { text } => Some(text),
        _ => None,
    }).collect();
    assert!(notes.iter().any(|n| n == note), "{notes:?}");
    // the other card's activity doesn't
    assert!(!gizai_core::tasks::activity(&a.st.db, &other).unwrap().iter().any(|e| e.diff.get("cleanup").is_some()));

    // the other run and the test keep running
    assert!(alive(other_pid), "the other run's process keeps running");
    assert_eq!(a.live_cards(), [other.clone()]);
    runs::stop(&a.st, &other_run);
    let s2 = other_done.await.unwrap();
    assert_eq!(s2.status, "cancelled");
    assert!(!gizai_core::tasks::activity(&a.st.db, &other).unwrap().iter().any(|e| e.diff.get("cleanup").is_some()));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn stop_continue_and_quitting_work_as_before_in_the_slice_and_leave_nothing_running_in_it() {
    let a = app();
    let Some(slice) = own_slice(&a.st, 2 * GB) else { return };
    let s = &slice.0;
    let pids = a.tmp.path().join("pids");
    // FAKE_GATE: a continued run (its prompt has no FAKE_HANG) waits until it is stopped too
    let bin = wrapper(a.tmp.path(), "claude.sh", &format!("echo $$ >> '{}'\nexport FAKE_GATE='{}'", pids.display(), a.tmp.path().join("never").display()), FAKE);
    let read_pids = || -> Vec<u32> { std::fs::read_to_string(&pids).unwrap_or_default().lines().filter_map(|l| l.trim().parse().ok()).collect() };
    let t = a.card("FAKE_HANG");

    // Stop
    let (run, done) = runs::start(&a.st, &t, None, Some(bin.clone()), "manual").await.unwrap();
    until("the run started", || read_pids().len() == 1).await;
    let first = read_pids()[0];
    assert!(in_a_scope_of(&cgroup_of(first), s));
    let leads_its_group = std::fs::read_to_string(format!("/proc/{first}/stat")).unwrap();
    let pgrp: u32 = leads_its_group[leads_its_group.rfind(')').unwrap() + 2..].split_whitespace().nth(2).unwrap().parse().unwrap();
    assert_eq!(pgrp, first, "its own process group, as before");
    tokio::time::sleep(Duration::from_millis(300)).await;
    runs::stop(&a.st, &run);
    let stopped = done.await.unwrap();
    assert_eq!(stopped.status, "cancelled");
    until("its process is gone", || !alive(first)).await;
    until("nothing is left in the slice", || procs_in(s.dir()).is_empty()).await;

    // Continue resumes it, in the slice again
    let (again, done) = runs::continue_run(&a.st, &run, Some(bin)).await.unwrap();
    until("the continued run started", || read_pids().len() == 2).await;
    let second = read_pids()[1];
    assert!(in_a_scope_of(&cgroup_of(second), s));
    let resumed = gizai_core::runs::get(&a.st.db, &again).unwrap();
    assert_eq!(resumed.session_id, gizai_core::runs::get(&a.st.db, &run).unwrap().session_id, "the same session");

    // quitting: every run is stopped and recorded as stopped because Gizai quit, none outlives it
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs::stop_all(&a.st, Duration::from_secs(12)).await, 1);
    let quit = done.await.unwrap();
    assert_eq!((quit.status.as_str(), quit.error.as_deref()), ("cancelled", Some(runs::STOPPED_BY_QUIT)));
    until("its process is gone", || !alive(second)).await;
    until("nothing is left in the slice", || procs_in(s.dir()).is_empty()).await;
}
