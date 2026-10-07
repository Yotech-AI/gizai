// Agents on Codex, Gemini, another coding CLI or a second account (GA-3), run end to end with a fake CLI
// (crates/gizai-agents/tests/fake-cli.sh), never the real ones.
use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, TaskPatch};

const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git_repo(dir: &std::path::Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str, env: &[&str], args: &str) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(),
                    args: args.into(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// The backend agent of `test_task`, moved to the CLI `cli`.
fn put_agent_on(st: &gizai_lib::AppState, cli: &str, extra: AgentInput) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..extra }).unwrap();
    agent.actor_id
}

fn describe(st: &gizai_lib::AppState, task: &str, text: &str) {
    gizai_core::tasks::update(&st.db, &st.you_id, task, TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

fn stderr_of(run: &gizai_core::model::Run) -> String {
    std::fs::read_to_string(std::path::Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

#[tokio::test]
async fn a_codex_agent_runs_codex_exec_and_moves_the_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let codex = add_cli(&st, "Codex (2nd account)", "codex", FAKE_CLI, &["FAKE_KIND=codex", "FAKE_ACCOUNT=~/.codex-2"], "");
    put_agent_on(&st, &codex, AgentInput { model: Some("gpt-5-codex".into()), effort: Some("high".into()), ..Default::default() });

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None));
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!(run.adapter.as_deref(), Some(codex.as_str()), "the run records its CLI");
    assert_eq!(run.session_id.as_deref(), Some("019a-fake-thread"), "Codex's thread id is the session Continue resumes");
    assert_eq!((run.cost_usd_micros, run.input_tokens, run.output_tokens), (0, 1500, 250), "Codex reports tokens, not cost");

    let err = stderr_of(&run);
    let common = std::fs::canonicalize(repo.join(".git")).unwrap();
    for want in ["argv: exec --json -m gpt-5-codex -c model_reasoning_effort=\"high\" -c approval_policy=\"never\" -c sandbox_mode=\"workspace-write\"",
                 "sandbox_workspace_write.network_access=true", &format!("sandbox_workspace_write.writable_roots=[\"{}\"] -", common.display())] {
        assert!(err.contains(want), "{want} missing in {err}");
    }
    let home = std::env::var("HOME").unwrap();
    assert!(err.contains(&format!("account: {home}/.codex-2")), "the CLI's environment, ~ expanded: {err}");
    assert!(!err.contains("prompt chars: 0"), "the prompt came on stdin: {err}");

    // the log says which CLI wrote it, so the Run panel reads it back the same way
    let log = std::fs::read_to_string(&run.log_path).unwrap();
    assert!(log.lines().next().unwrap().contains("\"gizai_cli\""), "{log}");
    let evs: Vec<_> = gizai_lib::runs::events_for(&st, &run.id).into_iter().map(|e| e.event).collect();
    use gizai_agents::stream::RunEvent;
    assert_eq!(evs[0], RunEvent::Init { session_id: "019a-fake-thread".into(), model: String::new() });
    assert_eq!(evs.iter().filter(|e| matches!(e, RunEvent::ToolUse { .. })).count(), 2, "{evs:?}");
    assert!(matches!(evs.iter().rev().find(|e| matches!(e, RunEvent::Result { .. })), Some(RunEvent::Result { is_error: false, .. })), "{evs:?}");
}

#[tokio::test]
async fn continue_resumes_codexs_thread() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let codex = add_cli(&st, "Codex", "codex", FAKE_CLI, &["FAKE_KIND=codex"], "");
    put_agent_on(&st, &codex, AgentInput::default());
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let first = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(first.status, "timed_out", "{:?}", first.error);
    assert_eq!(first.session_id.as_deref(), Some("019a-fake-thread"));
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &200u32).unwrap();

    let (id, done) = gizai_lib::runs::continue_run(&st, &first.id, None).await.unwrap();
    done.await.unwrap();
    let next = gizai_core::runs::get(&st.db, &id).unwrap();
    assert_eq!((next.trigger.as_str(), next.status.as_str()), ("nudge", "succeeded"));
    assert_eq!(next.worktree_path, first.worktree_path);
    let err = stderr_of(&next);
    assert!(err.contains("argv: exec resume --json") && err.contains("019a-fake-thread -"), "{err}");
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
}

#[tokio::test]
async fn continue_after_moving_the_agent_to_another_cli_does_not_resume_on_the_wrong_cli() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let codex = add_cli(&st, "Codex", "codex", FAKE_CLI, &["FAKE_KIND=codex"], "");
    let other = add_cli(&st, "Plain", "other", FAKE_CLI, &["FAKE_KIND=other"], "");
    put_agent_on(&st, &codex, AgentInput::default());
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let first = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(first.status, "timed_out");
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &200u32).unwrap();
    // the agent now runs on a CLI without sessions; the stopped run's session is Codex's
    put_agent_on(&st, &other, AgentInput::default());
    match gizai_lib::runs::continue_run(&st, &first.id, None).await {
        Err(e) => assert!(!e.is_empty()),
        Ok((id, done)) => {
            done.await.unwrap();
            let next = gizai_core::runs::get(&st.db, &id).unwrap();
            assert_eq!(next.adapter.as_deref(), Some(codex.as_str()),
                       "Continue resumed a Codex thread on {:?}; that CLI got only the 'continue' prompt, without the card: {}",
                       next.adapter, stderr_of(&next));
        }
    }
}

#[tokio::test]
async fn a_gemini_agent_runs_headless_with_gizais_session() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let gemini = add_cli(&st, "Gemini", "gemini", FAKE_CLI, &["FAKE_KIND=gemini"], "");
    put_agent_on(&st, &gemini, AgentInput { allowed_tools: vec!["Bash(cargo test:*)".into()], ..Default::default() });

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}", s.error);
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!((run.input_tokens, run.output_tokens, run.cost_usd_micros), (800, 200, 0));
    let err = stderr_of(&run);
    let started = run.session_id.clone().unwrap();
    for want in ["--output-format stream-json".to_string(), "--approval-mode auto_edit".into(), "--allowed-tools=run_shell_command(cargo test)".into()] {
        assert!(err.contains(&want), "{want} missing in {err}");
    }
    assert!(err.contains("--session-id"), "{err}");
    assert!(started == "gemini-own-id" || err.contains(&format!("--session-id {started}")), "the session Continue resumes is the one Gemini ran: {started} / {err}");
}

#[tokio::test]
async fn another_cli_gets_its_arguments_and_its_plain_text_end_moves_the_card() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let other = add_cli(&st, "OpenCode", "other", FAKE_CLI, &["FAKE_KIND=other"], "run -m {model} {prompt}");
    put_agent_on(&st, &other, AgentInput { permission_mode: "acceptEdits".into(), ..Default::default() });

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}", s.error);
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let err = stderr_of(&run);
    assert!(err.contains("argv: run ") && !err.contains("-m ") && err.contains("prompt chars: 0"),
            "no model: -m {{model}} is left out, and the prompt is an argument, not stdin: {err}");
    let evs: Vec<_> = gizai_lib::runs::events_for(&st, &run.id).into_iter().map(|e| e.event).collect();
    assert!(evs.contains(&gizai_agents::stream::RunEvent::Text { text: "Working on it".into() }), "colours are removed: {evs:?}");

    // without the result line the run has no result, and an Other CLI can't continue
    let task2 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    describe(&st, &task2, "FAKE_NO_RESULT");
    let s = gizai_lib::runs::run_once(&st, &task2, None, None).await.unwrap();
    assert_eq!(gizai_core::runs::get(&st.db, &s.run_id).unwrap().outcome.as_deref(), Some("no_result"), "{s:?}");
    let e = gizai_lib::runs::continue_run(&st, &s.run_id, None).await.map(|_| ()).unwrap_err();
    assert!(e.contains("OpenCode can't continue a run"), "{e}");
}

#[tokio::test]
async fn a_failing_or_missing_cli_is_named_in_the_error() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let codex = add_cli(&st, "Codex", "codex", FAKE_CLI, &["FAKE_KIND=codex"], "");
    put_agent_on(&st, &codex, AgentInput::default());
    describe(&st, &task, "FAKE_NOT_LOGGED_IN");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.status, "failed");
    assert_eq!(s.error.as_deref(), Some("Codex: Not logged in: run codex login"));

    let gone = add_cli(&st, "Gone", "gemini", "/nonexistent/gemini", &[], "");
    put_agent_on(&st, &gone, AgentInput::default());
    let task2 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let e = gizai_lib::runs::run_once(&st, &task2, None, None).await.unwrap_err();
    assert!(e.contains("Gone not found (/nonexistent/gemini)") && e.contains("Settings → Coding CLIs"), "{e}");
    assert!(gizai_core::runs::list_for_task(&st.db, &task2).unwrap().is_empty(), "no run row");
}

#[tokio::test]
async fn a_second_claude_code_account_runs_claude_code_with_its_own_config_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let claude2 = add_cli(&st, "Claude Code (2nd account)", "claude_code", FAKE_CLAUDE, &["CLAUDE_CONFIG_DIR=~/.claude-2"], "");
    put_agent_on(&st, &claude2, AgentInput { effort: Some("max".into()), ..Default::default() });
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")));
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!(run.adapter.as_deref(), Some(claude2.as_str()));
    assert_eq!(run.cost_usd_micros, 420_000, "Claude Code still reports cost");
    let log = std::fs::read_to_string(&run.log_path).unwrap();
    assert!(!log.contains("gizai_cli"), "Claude Code logs stay as they were");
    assert!(stderr_of(&run).contains("--effort max"));
    // its model list is asked from that account; Codex has none to ask
    let models = gizai_lib::runs::models_for(&st, Some(&claude2), false).await.unwrap();
    assert_eq!(models.len(), 5);
    let codex = add_cli(&st, "Codex", "codex", "/nonexistent/codex", &[], "");
    assert!(gizai_lib::runs::models_for(&st, Some(&codex), false).await.unwrap().is_empty());
}

#[test]
fn settings_list_each_cli_with_its_program_or_why_not() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    add_cli(&st, "Fake", "other", FAKE_CLI, &[], "");
    add_cli(&st, "Gone", "codex", "/nonexistent/codex", &[], "");
    let list = gizai_lib::clis::list(&st).unwrap();
    assert_eq!(list.iter().map(|c| c.cli.name.as_str()).collect::<Vec<_>>(), ["Claude Code", "Fake", "Gone"]);
    assert_eq!(list[0].path.as_deref(), Some(FAKE_CLAUDE));
    assert_eq!((list[1].path.as_deref(), list[1].problem.as_deref()), (Some(FAKE_CLI), None));
    assert_eq!((list[2].path.as_deref(), list[2].problem.as_deref()), (None, Some("/nonexistent/codex not found")));
    // a program by name is looked up on the PATH
    let dir = std::path::Path::new(FAKE_CLI).parent().unwrap();
    let found = gizai_lib::clis::resolve_program("fake-cli.sh", dir.as_os_str()).unwrap();
    assert_eq!(found, dir.join("fake-cli.sh"));
    assert_eq!(gizai_lib::clis::resolve_program("fake-cli.sh", std::ffi::OsStr::new("/nonexistent")), None);
}
