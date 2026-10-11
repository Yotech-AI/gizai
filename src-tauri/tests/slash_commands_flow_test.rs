// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-93: an agent's Slash commands and skills switch (agent form → Tools → Built-in tools), end to end with fake Claude
// Codes, never the real one. It is off for every agent until you switch it on. On, a Claude Code agent's new, nudged and
// continued runs go without --disable-slash-commands and get SlashCommand and Skill; off, the command line is as before,
// whatever the agent's allowed commands say. Only you switch it: the Team Lead's create_agent and update_agent refuse it
// and change nothing, get_agent shows it, and the Team Lead's chat never gets slash commands. Codex and Gemini agents
// can't have it, and say why.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::mcp_servers::{self as core_mcp, CliTools};
use gizai_core::model::{AgentInput, ProjectInput, Run, TaskPatch};
use gizai_lib::mcp_servers as app_mcp;
use gizai_lib::{AppState, chat as app_chat, tools};
use serde_json::{Value, json};

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const FAKE_CHAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-mcp-chat.py");

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

fn add_cli(st: &AppState, name: &str, kind: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent on the CLI `cli` with these allowed commands (a plain form save) and returns its id.
fn put_agent_on(st: &AppState, cli: &str, allowed: &[&str]) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), allowed_tools: allowed.iter().map(|a| a.to_string()).collect(), ..Default::default() }).unwrap();
    agent.actor_id
}

fn describe(st: &AppState, task: &str, text: &str) {
    gizai_core::tasks::update(&st.db, &st.you_id, task, TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

fn stderr_of(run: &Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap_or_default()
}

/// The words of the fake's `argv:` line (it prints its arguments joined by spaces).
fn argv_of(run: &Run) -> Vec<String> {
    let err = stderr_of(run);
    let line = err.lines().find_map(|l| l.strip_prefix("argv: ")).unwrap_or_else(|| panic!("no argv in {err}"));
    line.split_whitespace().map(str::to_string).collect()
}

/// The words after `flag`, up to the next `--` option.
fn values(argv: &[String], flag: &str) -> Vec<String> {
    let Some(at) = argv.iter().position(|a| a == flag) else { return vec![] };
    argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect()
}

fn notes(st: &AppState, run_id: &str) -> Vec<String> {
    gizai_lib::runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

fn runs_of(st: &AppState, task: &str) -> Vec<Run> {
    let mut runs = gizai_core::runs::list_for_task(&st.db, task).unwrap();
    runs.sort_by_key(|r| r.created_at);
    runs
}

/// Waits until the card has `n` runs and none of them is live.
async fn wait_for_runs(st: &AppState, task: &str, n: usize) -> Vec<Run> {
    let t0 = Instant::now();
    loop {
        let runs = runs_of(st, task);
        if runs.len() >= n && gizai_lib::runs::live(st).iter().all(|l| l.task_id != task)
            && runs.iter().all(|r| !matches!(r.status.as_str(), "queued" | "running" | "waiting_approval")) {
            return runs;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "{n} runs expected: {:?}", runs.iter().map(|r| (&r.trigger, &r.status)).collect::<Vec<_>>());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

struct Setup {
    _tmp: tempfile::TempDir,
    st: AppState,
    repo: PathBuf,
    cli: String,
    agent: String,
}

/// The backend agent on a fake Claude Code (fake-claude.sh, which prints its arguments to the run's stderr log).
fn setup(allowed: &[&str]) -> Setup {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let cli = add_cli(&st, "Claude Code (slash)", "claude_code", FAKE_CLAUDE, &[]);
    let agent = put_agent_on(&st, &cli, allowed);
    Setup { _tmp: tmp, st, repo, cli, agent }
}

impl Setup {
    fn task(&self) -> String {
        gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend")
    }

    /// A new run whose card ends without its result and Gizai's nudge that follows it; then, on another card, a run
    /// stopped part-way and continued. Returns (what, run) for the new, nudged and continued runs.
    async fn three_runs(&self) -> Vec<(&'static str, Run)> {
        let st = &self.st;
        // the new run ends without its result line; the nudge's prompt doesn't have the card's words, so it ends with one
        let task = self.task();
        describe(st, &task, "FAKE_NO_RESULT");
        gizai_lib::runs::run_once(st, &task, None, None).await.unwrap();
        let runs = wait_for_runs(st, &task, 2).await;
        assert_eq!(runs.iter().map(|r| (r.trigger.as_str(), r.nudged)).collect::<Vec<_>>(), [("manual", false), ("result_nudge", true)]);
        let (first, nudged) = (runs[0].clone(), runs[1].clone());

        // only the runs this test starts from here on
        gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
        let task = self.task();
        describe(st, &task, "FAKE_HANG");
        let (run_id, done) = gizai_lib::runs::start(st, &task, None, None, "manual").await.unwrap();
        let t0 = Instant::now();
        while !stderr_of(&gizai_core::runs::get(&st.db, &run_id).unwrap()).contains("argv: ") {
            assert!(t0.elapsed() < Duration::from_secs(15), "the fake never started");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        gizai_lib::runs::stop(st, &run_id);
        let s = tokio::time::timeout(Duration::from_secs(20), done).await.expect("stopped").unwrap();
        assert_eq!(s.status, "cancelled", "{:?}", s.error);
        let (id, done) = gizai_lib::runs::continue_run(st, &run_id, None).await.unwrap();
        let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("continued run ended").unwrap();
        assert_eq!(s.status, "succeeded", "{:?}", s.error);
        let continued = gizai_core::runs::get(&st.db, &id).unwrap();
        vec![("new", first), ("nudged", nudged), ("continued", continued)]
    }
}

#[tokio::test]
async fn switched_on_a_claude_code_agents_new_nudged_and_continued_runs_get_slash_commands_and_skills() {
    // Skill typed into its allowed commands too: the switch gives it, once
    let t = setup(&["Bash(git status:*)", "Skill"]);
    assert!(!core_mcp::agent_cli_tools(&t.st.db, &t.agent).unwrap().slash_commands, "off until you switch it on");
    let saved = app_mcp::save_cli_tools(&t.st, &t.agent, CliTools { slash_commands: true, ..Default::default() }).unwrap();
    assert!(saved.slash_commands);
    for (what, run) in t.three_runs().await {
        let a = argv_of(&run);
        assert!(!a.iter().any(|x| x == "--disable-slash-commands"), "{what}: {a:?}");
        let allowed = values(&a, "--allowedTools");
        for id in ["SlashCommand", "Skill"] {
            assert_eq!(allowed.iter().filter(|x| *x == id).count(), 1, "{what}: {id} once: {allowed:?}");
        }
        assert!(allowed.iter().any(|x| x.starts_with("Bash(git")), "{what}: the agent's commands stay: {allowed:?}");
        // hooks stay off and settings come only from the user's
        assert_eq!(values(&a, "--settings"), [r#"{"disableAllHooks":true}"#], "{what}: {a:?}");
        assert_eq!(values(&a, "--setting-sources"), ["user"], "{what}: {a:?}");
        assert_eq!(a.iter().any(|x| x == "--resume"), what != "new", "{what}: {a:?}");
        assert!(!notes(&t.st, &run.id).iter().any(|n| n.contains("without slash commands")), "{what}");
    }
}

#[tokio::test]
async fn switched_off_runs_keep_disable_slash_commands_and_slash_tools_typed_into_the_allowed_commands_stay_out() {
    let t = setup(&["Bash(git status:*)", "SlashCommand", "Skill", "Skill(frontend-design)"]);
    for (what, run) in t.three_runs().await {
        let a = argv_of(&run);
        assert_eq!(a.iter().filter(|x| *x == "--disable-slash-commands").count(), 1, "{what}: {a:?}");
        assert!(!a.iter().any(|x| x.starts_with("SlashCommand") || x.starts_with("Skill")), "{what}: {a:?}");
        assert_eq!(values(&a, "--settings"), [r#"{"disableAllHooks":true}"#], "{what}: {a:?}");
        assert_eq!(values(&a, "--setting-sources"), ["user"], "{what}: {a:?}");
        let n = notes(&t.st, &run.id);
        let note = n.iter().find(|x| x.starts_with("Left out of")).unwrap_or_else(|| panic!("{what}: no note: {n:?}"));
        for l in ["SlashCommand", "Skill", "Skill(frontend-design)"] {
            assert!(note.contains(l), "{what}: {l} named in {note}");
        }
    }
    // switched on and off again: as before
    app_mcp::save_cli_tools(&t.st, &t.agent, CliTools { slash_commands: true, ..Default::default() }).unwrap();
    app_mcp::save_cli_tools(&t.st, &t.agent, CliTools::default()).unwrap();
    assert!(!core_mcp::agent_cli_tools(&t.st.db, &t.agent).unwrap().slash_commands);
}

#[tokio::test]
async fn the_form_shows_the_switch_on_claude_code_and_disabled_with_why_on_codex_and_gemini() {
    let t = setup(&[]);
    let st = &t.st;
    let v = app_mcp::tools_view(st, Some(&t.agent), &t.cli).unwrap();
    assert_eq!(v.builtin.slash, None, "Claude Code can have it");
    let slash: Vec<(&str, &str, &str)> = v.builtin.tools.iter().filter(|x| x.how == "slash").map(|x| (x.id.as_str(), x.label.as_str(), x.risk.as_str())).collect();
    assert_eq!(slash, [("SlashCommand", "Slash commands and skills", "medium"), ("Skill", "Slash commands and skills", "medium")]);
    assert!(!v.saved.as_ref().unwrap().slash_commands, "off for an existing agent");
    // a new agent's form (no agent yet) shows it too, off
    let new = app_mcp::tools_view(st, None, &t.cli).unwrap();
    assert_eq!(new.builtin.slash, None);
    assert!(new.saved.is_none());
    // a built-in tool switch can't give them
    let e = app_mcp::save_cli_tools(st, &t.agent, CliTools { builtin: vec!["Skill".into()], ..Default::default() }).unwrap_err();
    assert!(e.contains("Slash commands and skills"), "{e}");

    for (name, kind) in [("Codex", "codex"), ("Gemini", "gemini")] {
        let cli = add_cli(st, name, kind, FAKE_CLI, &[&format!("FAKE_KIND={kind}")]);
        put_agent_on(st, &cli, &[]);
        let v = app_mcp::tools_view(st, Some(&t.agent), &cli).unwrap();
        let why = v.builtin.slash.clone().unwrap_or_else(|| panic!("{name}: no reason"));
        assert!(why.contains(name), "{name}: {why}");
        let e = app_mcp::save_cli_tools(st, &t.agent, CliTools { slash_commands: true, ..Default::default() }).unwrap_err();
        assert!(e.contains(name) && e.contains(&why), "{name}: {e}");
        assert!(!core_mcp::agent_cli_tools(&st.db, &t.agent).unwrap().slash_commands, "{name}: nothing saved");
    }
}

#[tokio::test]
async fn an_agent_moved_to_codex_or_gemini_with_the_switch_on_runs_without_it_and_the_log_says_why() {
    let t = setup(&[]);
    let st = &t.st;
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    app_mcp::save_cli_tools(st, &t.agent, CliTools { slash_commands: true, ..Default::default() }).unwrap();
    for (name, kind) in [("Codex", "codex"), ("Gemini", "gemini")] {
        let cli = add_cli(st, name, kind, FAKE_CLI, &[&format!("FAKE_KIND={kind}")]);
        put_agent_on(st, &cli, &[]);
        assert!(core_mcp::agent_cli_tools(&st.db, &t.agent).unwrap().slash_commands, "{name}: kept from Claude Code");
        let s = gizai_lib::runs::run_once(st, &t.task(), None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{name}: {:?}", s.error);
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        let argv = stderr_of(&run).lines().find_map(|l| l.strip_prefix("argv: ").map(str::to_string)).unwrap_or_else(|| panic!("{name}: no argv"));
        assert!(!argv.contains("SlashCommand") && !argv.contains("Skill") && !argv.contains("activate_skill"), "{name}: {argv}");
        let n = notes(st, &s.run_id);
        assert!(n.iter().any(|x| x.contains(name) && x.contains("without slash commands and skills")), "{name}: {n:?}");
    }
}

// ---- the Team Lead ----

struct Lead {
    _tmp: tempfile::TempDir,
    st: AppState,
    lead: String,
    design: String,
}

fn lead_setup() -> Lead {
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    // the fake starts no MCP server: the helper only has to be there
    st.mcp_shim = Some(PathBuf::from(FAKE_CHAT));
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CHAT.to_string()).unwrap();
    gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let design = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Design Agent".into(), role_key: "design".into(), ..Default::default() }).unwrap();
    Lead { _tmp: tmp, st, lead, design }
}

#[tokio::test]
async fn create_agent_and_update_agent_cant_switch_slash_commands_and_skills_and_get_agent_shows_it() {
    let t = lead_setup();
    let st = &t.st;
    let lead = t.lead.as_str();
    let call = |name: &'static str, args: Value| async move { tools::call_in(st, lead, None, name, args).await };
    let agents = gizai_core::team::all_agents(&st.db).unwrap().len();
    // get_agent shows it off for an agent nobody switched it on for
    let v = call("get_agent", json!({"agent": "Design Agent"})).await.unwrap()["agent"].clone();
    assert_eq!(v["slash_commands"], json!(false), "{v}");

    for on in [true, false] {
        // switched on or off by you, the Team Lead can't change it either way
        app_mcp::save_cli_tools(st, &t.design, CliTools { slash_commands: on, ..Default::default() }).unwrap();
        for (key, value) in [("slash_commands", json!(!on)), ("slash_commands", json!(on)), ("slash_commands_and_skills", json!(!on)), ("skills", json!(!on)),
                             ("builtin", json!(["SlashCommand", "Skill"])), ("cli_tools", json!({"slashCommands": !on}))] {
            let e = call("update_agent", json!({"agent": "Design Agent", key: value.clone()})).await.unwrap_err();
            assert!(e.contains("only the user") && e.contains("Nothing changed"), "update_agent {key}={value}: {e}");
            let e = call("create_agent", json!({"name": "Slash Agent", "role": "design", key: value.clone()})).await.unwrap_err();
            assert!(e.contains("only the user"), "create_agent {key}={value}: {e}");
            assert_eq!(core_mcp::agent_cli_tools(&st.db, &t.design).unwrap().slash_commands, on, "{key}={value}: nothing changed");
        }
        // a spelling the tools don't know changes nothing either
        let _ = call("update_agent", json!({"agent": "Design Agent", "slashCommands": !on})).await;
        assert_eq!(core_mcp::agent_cli_tools(&st.db, &t.design).unwrap().slash_commands, on);
        // nor does a plain update_agent of another field
        call("update_agent", json!({"agent": "Design Agent", "name": "Design Agent"})).await.unwrap();
        assert_eq!(core_mcp::agent_cli_tools(&st.db, &t.design).unwrap().slash_commands, on);
        let v = call("get_agent", json!({"agent": "Design Agent"})).await.unwrap()["agent"].clone();
        assert_eq!(v["slash_commands"], json!(on), "{v}");
    }
    assert_eq!(gizai_core::team::all_agents(&st.db).unwrap().len(), agents, "no agent was added");
    // an agent the Team Lead adds starts with it off
    call("create_agent", json!({"name": "New Design Agent", "role": "design"})).await.unwrap();
    let v = call("get_agent", json!({"agent": "New Design Agent"})).await.unwrap()["agent"].clone();
    assert_eq!(v["slash_commands"], json!(false), "{v}");
    // the tools' descriptions say so, and neither takes such a field
    let catalog = tools::catalog();
    let update = catalog.iter().find(|d| d.name == "update_agent").unwrap();
    assert!(update.description.contains("slash commands and skills"), "{}", update.description);
    for d in catalog.iter().filter(|d| d.name == "update_agent" || d.name == "create_agent") {
        let props: Vec<&String> = d.input_schema["properties"].as_object().unwrap().keys().collect();
        assert!(!props.iter().any(|p| p.contains("slash") || p.contains("skill")), "{}: {props:?}", d.name);
    }
}

#[tokio::test]
async fn the_team_leads_chat_still_gets_disable_slash_commands_with_the_switch_on() {
    let t = lead_setup();
    let st = &t.st;
    // even switched on for the Team Lead itself: its chat never gets slash commands or skills
    app_mcp::save_cli_tools(st, &t.lead, CliTools { slash_commands: true, ..Default::default() }).unwrap();
    let (_, done) = app_chat::send(st, None, "What is on the board?".into(), None).await.unwrap();
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let calls = std::fs::read_to_string(st.data_dir.join("chat/fake-mcp-calls.jsonl")).unwrap();
    let call: Value = serde_json::from_str(calls.lines().last().unwrap()).unwrap();
    let argv: Vec<String> = call["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect();
    assert_eq!(argv.iter().filter(|a| *a == "--disable-slash-commands").count(), 1, "{argv:?}");
    assert_eq!(values(&argv, "--tools"), ["Read,Glob,Grep"], "{argv:?}");
    assert!(!argv.iter().any(|a| a.contains("SlashCommand") || a.contains("Skill")), "{argv:?}");
    assert_eq!(values(&argv, "--settings"), [r#"{"disableAllHooks":true}"#], "{argv:?}");
}
