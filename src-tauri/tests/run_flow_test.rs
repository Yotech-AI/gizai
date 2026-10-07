use std::time::Duration;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git_repo(dir: &std::path::Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

#[tokio::test]
async fn fake_claude_run_moves_card_to_testing() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded");
    assert_eq!(s.outcome.as_deref(), Some("ready_for_testing"));
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
    let run = &gizai_core::runs::list_for_task(&st.db, &task).unwrap()[0];
    assert_eq!((run.cost_usd_micros, run.input_tokens), (420_000, 38_000));
    assert!(run.branch.as_deref().unwrap().starts_with("gizai/kade-1-"));
    assert!(std::path::Path::new(run.worktree_path.as_deref().unwrap()).join(".git").exists());
    assert_eq!(gizai_lib::runs::events_for(&st, &run.id).len(), 7, "6 log lines → 7 events, replayed from the log");
}

#[tokio::test]
async fn missing_claude_gives_a_readable_error_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let task = gizai_lib::test_task(&st, tmp.path().to_str().unwrap(), "backend");
    let err = gizai_lib::runs::run_once(&st, &task, None, Some("/nonexistent/claude".into())).await.unwrap_err();
    assert!(err.contains("Claude Code not found"), "{err}");
    assert!(gizai_core::runs::list_for_task(&st.db, &task).unwrap().is_empty(), "no run row created");
}

#[tokio::test]
async fn a_project_without_a_repo_is_refused_before_anything_starts() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let task = gizai_lib::test_task(&st, "", "backend");
    let err = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap_err();
    assert!(err.contains("git repository"), "{err}");
    assert!(gizai_core::runs::list_for_task(&st.db, &task).unwrap().is_empty());
}

#[tokio::test]
async fn stopping_a_run_cancels_it_without_moving_or_penalising_the_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, Some(FAKE.into()), "manual").await.unwrap();
    assert_eq!(gizai_lib::runs::live(&st).len(), 1);
    tokio::time::sleep(Duration::from_millis(300)).await;
    gizai_lib::runs::stop(&st, &run_id);
    let s = done.await.unwrap();
    assert_eq!(s.status, "cancelled");
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.fail_count), ("To do", 0));
    assert!(gizai_lib::runs::live(&st).is_empty());
}

#[tokio::test]
async fn the_concurrency_cap_refuses_extra_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "max_concurrent_runs", &1u32).unwrap();
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_core::tasks::update(&st.db, &st.you_id, &a, gizai_core::model::TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let (run_a, done_a) = gizai_lib::runs::start(&st, &a, None, Some(FAKE.into()), "manual").await.unwrap();
    let b = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let err = gizai_lib::runs::start(&st, &b, None, Some(FAKE.into()), "manual").await.map(|_| ()).unwrap_err();
    assert!(err.contains("already active"), "{err}");
    gizai_lib::runs::stop(&st, &run_a);
    done_a.await.unwrap();
}

#[tokio::test]
async fn a_due_heartbeat_starts_the_agents_next_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    let mut input = gizai_core::model::AgentInput { name: agent.name.clone(), role_key: agent.role_key.clone(), wakeup: "heartbeat".into(), heartbeat_minutes: Some(5), ..Default::default() };
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, input.clone()).unwrap();
    let started = gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms()).await;
    assert_eq!(started.len(), 1);
    started.into_iter().next().unwrap().await.unwrap();
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
    // not due again within its interval, and nothing happens while agents are paused
    assert!(gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms() + 60_000).await.is_empty());
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    input.heartbeat_minutes = Some(1);
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, input).unwrap();
    assert!(gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms() + 3_600_000).await.is_empty());
}

#[tokio::test]
async fn a_crashing_claude_shows_its_error_and_counts_as_a_failed_run() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { description_md: Some("FAKE_CRASH".into()), ..Default::default() }).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "failed");
    assert!(s.error.as_deref().unwrap().contains("unknown option '--frobnicate'"), "{:?}", s.error);
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.fail_count), ("To do", 1));
}

#[tokio::test]
async fn an_on_assign_builder_does_not_loop_on_its_own_testing_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput {
        name: agent.name.clone(), role_key: agent.role_key.clone(), wakeup: "on_assign".into(), ..Default::default() }).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(gizai_core::runs::list_for_task(&st.db, &task).unwrap().len(), 1, "no automatic re-run of the builder on Testing");
    assert!(gizai_lib::runs::live(&st).is_empty());
}

#[tokio::test]
async fn the_suggested_agent_is_the_assigned_one_before_routing() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let task = gizai_lib::test_task(&st, tmp.path().to_str().unwrap(), "backend");
    let backend = gizai_core::team::all_agents(&st.db).unwrap()[0].1.actor_id.clone();
    assert_eq!(gizai_lib::runs::suggest(&st, &task).as_deref(), Some(backend.as_str()));
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let fe = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, gizai_core::model::AgentInput { name: "Frontend Agent".into(), role_key: "frontend".into(), ..Default::default() }).unwrap();
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { assignee_id: Some(fe.clone()), ..Default::default() }).unwrap();
    assert_eq!(gizai_lib::runs::suggest(&st, &task).as_deref(), Some(fe.as_str()));
}

#[tokio::test]
async fn quitting_stops_every_live_run_first() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let (_run, _done) = gizai_lib::runs::start(&st, &task, None, Some(FAKE.into()), "manual").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(gizai_lib::runs::stop_all(&st, Duration::from_secs(10)).await, 1);
    assert!(gizai_lib::runs::live(&st).is_empty());
    assert_eq!(gizai_core::runs::list_for_task(&st.db, &task).unwrap()[0].status, "cancelled");
}

#[tokio::test]
async fn a_card_that_cannot_start_in_the_background_goes_on_hold_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let task = gizai_lib::test_task(&st, "", "backend"); // project without a repository
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput {
        name: agent.name.clone(), role_key: agent.role_key.clone(), wakeup: "heartbeat".into(), heartbeat_minutes: Some(5), ..Default::default() }).unwrap();
    assert!(gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms()).await.is_empty());
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!(t.hold.as_deref(), Some("blocked"));
    assert!(t.hold_reason.as_deref().unwrap_or("").contains("git repository"), "{:?}", t.hold_reason);
    assert_eq!(gizai_core::workflow::next_task_for(&st.db, &agent.actor_id).unwrap(), None, "the agent no longer retries this card");
}

#[tokio::test]
async fn a_missing_claude_does_not_put_cards_on_hold() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &"/nonexistent/claude".to_string()).unwrap();
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput {
        name: agent.name.clone(), role_key: agent.role_key.clone(), wakeup: "heartbeat".into(), heartbeat_minutes: Some(5), ..Default::default() }).unwrap();
    let _ = gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms()).await;
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().hold, None);
}

#[tokio::test]
async fn an_agent_over_its_monthly_budget_does_not_start() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput {
        name: agent.name.clone(), role_key: agent.role_key.clone(), budget_usd_micros: Some(500_000), ..Default::default() }).unwrap();
    gizai_lib::runs::run_once(&st, &task, Some(agent.actor_id.clone()), Some(FAKE.into())).await.unwrap(); // $0.42
    gizai_lib::runs::run_once(&st, &task, Some(agent.actor_id.clone()), Some(FAKE.into())).await.unwrap(); // $0.84 ≥ $0.50
    let err = gizai_lib::runs::run_once(&st, &task, Some(agent.actor_id.clone()), Some(FAKE.into())).await.unwrap_err();
    assert!(err.contains("monthly budget"), "{err}");
}

#[tokio::test]
async fn task_agents_run_without_the_users_hooks_and_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let stderr = std::fs::read_to_string(std::path::Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    assert!(stderr.contains(r#"--settings {"disableAllHooks":true}"#), "{stderr}");
    assert!(stderr.contains("--disable-slash-commands"), "{stderr}");
    assert!(stderr.contains("--setting-sources user"), "{stderr}");
}

#[tokio::test]
async fn a_claude_that_is_not_logged_in_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { description_md: Some("FAKE_NOT_LOGGED_IN".into()), ..Default::default() }).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "failed");
    assert!(s.error.as_deref().unwrap_or("").contains("Not logged in"), "{:?}", s.error);
}

#[tokio::test]
async fn an_agents_model_and_effort_reach_claude_code() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        model: Some("opus".into()), effort: Some("max".into()), ..Default::default() }).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let stderr = std::fs::read_to_string(std::path::Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    assert!(stderr.contains("--model opus") && stderr.contains("--effort max"), "{stderr}");
}

#[tokio::test]
async fn claude_codes_model_list_is_fetched_and_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let models = gizai_lib::runs::models(&st, false).await.unwrap();
    assert_eq!(models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(), ["default", "opus", "fable", "sonnet", "haiku"]);
    // kept: a second ask doesn't start Claude Code again, even if it is gone now
    gizai_core::settings::set(&st.db, "claude_bin", &"/nonexistent/claude".to_string()).unwrap();
    assert_eq!(gizai_lib::runs::models(&st, false).await.unwrap().len(), 5);
    assert!(gizai_lib::runs::models(&st, true).await.is_err(), "refresh asks again");
}

#[tokio::test]
async fn run_limits_are_settings_with_sane_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let s = gizai_lib::runs::get_settings(&st);
    assert_eq!((s.max_run_minutes, s.max_run_tool_calls), (90, 200));
    let mut next = s.clone();
    next.max_run_minutes = 90;
    next.max_run_tool_calls = 400;
    gizai_lib::runs::save_settings(&st, &next).unwrap();
    let s = gizai_lib::runs::get_settings(&st);
    assert_eq!((s.max_run_minutes, s.max_run_tool_calls), (90, 400));
    let many = gizai_lib::runs::Settings { max_concurrent_runs: 20, ..s.clone() };
    assert!(gizai_lib::runs::save_settings(&st, &many).is_ok(), "20 runs at once in total");
    assert!(gizai_lib::runs::save_settings(&st, &gizai_lib::runs::Settings { max_concurrent_runs: 21, ..s.clone() }).is_err());
    for (m, t) in [(0, 200), (60, 5), (2000, 200), (60, 100_000)] {
        let bad = gizai_lib::runs::Settings { max_run_minutes: m, max_run_tool_calls: t, ..s.clone() };
        assert!(gizai_lib::runs::save_settings(&st, &bad).is_err(), "{m} min / {t} calls accepted");
    }
}

#[tokio::test]
async fn a_run_stopped_by_a_limit_says_which_one() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(run.status, "timed_out");
    let err = run.error.unwrap_or_default();
    assert!(err.contains("1 tool calls") && err.contains("Settings"), "{err}");
}

#[tokio::test]
async fn a_heartbeat_starts_as_many_cards_as_the_agent_takes() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let tasks: Vec<String> = (0..3).map(|_| gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend")).collect();
    for t in &tasks {
        gizai_core::tasks::update(&st.db, &st.you_id, t, gizai_core::model::TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    }
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput {
        name: agent.name.clone(), role_key: agent.role_key.clone(), wakeup: "heartbeat".into(), heartbeat_minutes: Some(1),
        max_runs: Some(2), ..Default::default() }).unwrap();
    let started = gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms()).await;
    assert_eq!(started.len(), 2, "two cards at once");
    let live = gizai_lib::runs::live(&st);
    assert_eq!(live.len(), 2);
    assert_ne!(live[0].task_id, live[1].task_id, "each on its own card");
    // full: the next heartbeat starts nothing more
    assert!(gizai_lib::runs::heartbeat_tick(&st, gizai_core::ids::now_ms() + 120_000).await.is_empty());
    gizai_lib::runs::stop_all(&st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn continue_resumes_the_stopped_session_in_the_same_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let first = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(first.status, "timed_out");
    // a person put it on hold meanwhile; Continue clears that
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { hold: Some("stalled".into()), ..Default::default() }).unwrap();
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &200u32).unwrap();

    let (id, done) = gizai_lib::runs::continue_run(&st, &first.id, Some(FAKE.into())).await.unwrap();
    done.await.unwrap();
    let runs = gizai_core::runs::list_for_task(&st.db, &task).unwrap();
    let next = runs.iter().find(|r| r.id == id).unwrap();
    assert_eq!((next.trigger.as_str(), next.status.as_str()), ("nudge", "succeeded"));
    assert_eq!(next.session_id, first.session_id, "the same Claude Code session");
    assert_eq!(next.worktree_path, first.worktree_path);
    assert_ne!(next.log_path, first.log_path, "its own log");
    let argv = std::fs::read_to_string(std::path::Path::new(&next.log_path).with_extension("stderr.log")).unwrap();
    assert!(argv.contains(&format!("--resume {}", first.session_id.clone().unwrap())), "{argv}");
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.hold.as_deref(), t.state_name.as_str()), (None, "Testing"));

    // only the latest run continues, and only one that didn't finish
    assert!(gizai_lib::runs::continue_run(&st, &first.id, Some(FAKE.into())).await.is_err());
    assert!(gizai_lib::runs::continue_run(&st, &id, Some(FAKE.into())).await.is_err());
}

/// "GitHub" (a bare repository) that is one commit ahead of the local clone the project links.
fn github_ahead_of_local(tmp: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let src = git_repo(tmp);
    let bare = tmp.join("github.git");
    let git = |dir: &std::path::Path, args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(dir).status().unwrap().success());
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.join("local");
    git(tmp, &["clone", "-q", "-o", "acme-labs", bare.to_str().unwrap(), local.to_str().unwrap()]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "on github"]);
    git(&src, &["push", "-q", bare.to_str().unwrap(), "HEAD:main"]);
    (bare, local)
}

fn link_github(st: &gizai_lib::AppState, url: &str) {
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(url.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
}

#[tokio::test]
async fn a_card_of_a_github_project_starts_from_main_fetched_from_github() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let (bare, local) = github_ahead_of_local(tmp.path());
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    link_github(&st, bare.to_str().unwrap());
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let github_main = String::from_utf8(std::process::Command::new("git").args(["rev-parse", "main"]).current_dir(&bare).output().unwrap().stdout).unwrap();
    assert_eq!(run.base_sha.as_deref(), Some(github_main.trim()), "recorded where it started");
    let head = String::from_utf8(std::process::Command::new("git").args(["rev-parse", "HEAD"]).current_dir(run.worktree_path.unwrap()).output().unwrap().stdout).unwrap();
    assert_eq!(head.trim(), github_main.trim(), "the worktree started from GitHub's main, not the stale local main");
}

#[tokio::test]
async fn an_unreachable_github_stops_the_start_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let (_bare, local) = github_ahead_of_local(tmp.path());
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    link_github(&st, "/nonexistent/github.git");
    let e = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap_err();
    assert!(e.contains("Couldn't fetch main"), "{e}");
}
