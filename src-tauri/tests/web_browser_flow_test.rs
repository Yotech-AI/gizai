// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-55: what a task run and the Team Lead's chat get of an agent's Web switches, the hidden browser (Chrome DevTools MCP)
// and the CLI's built-in tools, end to end with fake Claude Codes, a fake npx and a stand-in browser program that never
// runs: never the real Claude Code, npx, Chrome DevTools MCP, a browser or a browser profile. Also Settings → MCP
// servers' built-in browser, the Team Lead's rules, Codex and Gemini, and Stop, the tool cap and quitting ending the
// browser the server started in a process group of its own.
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gizai_agents::mcp_run::UNTRUSTED;
use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::mcp_servers::{self as core_mcp, AgentServer, AgentTools, BrowserEntry, CliTools};
use gizai_core::model::{AgentInput, ProjectInput, TaskPatch};
use gizai_lib::mcp_servers as app_mcp;
use gizai_lib::{AppState, chat as app_chat, mcp, tools};
use serde_json::{Value, json};

const FAKE_RUN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-mcp-run.sh");
const FAKE_CHAT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-mcp-chat.py");
const FAKE_OUTSIDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat-outside.py");
const FAKE_BROWSER_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-browser.py");
const FAKE_NPX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-npx-browser.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
/// An init line naming WebSearch, an MCP tool and FancyNewTool, a tool Gizai's catalog doesn't know.
const TOOLS_INIT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fixtures/run-tools-init.jsonl");

/// Every test here starts the browser's server through GIZAI_NPX, the fake npx: set once, the same for all of them,
/// before any of them starts a run. The committed fakes are made runnable too.
fn fakes() {
    static SET: OnceLock<()> = OnceLock::new();
    SET.get_or_init(|| {
        use std::os::unix::fs::PermissionsExt;
        for f in [FAKE_NPX, FAKE_BROWSER_CLAUDE] {
            std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // SAFETY: set once (OnceLock), before any test in this file goes on, to the same value for all of them.
        unsafe { std::env::set_var("GIZAI_NPX", FAKE_NPX) };
    });
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// A stand-in browser program (a script nothing here starts): the program Settings passes with --executablePath.
fn stand_in(path: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

fn add_cli(st: &AppState, name: &str, kind: &str, command: &str, env: &[String]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.to_vec(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent on the CLI `cli` (its other settings as a plain form save) and returns its id.
fn put_agent_on(st: &AppState, cli: &str) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..Default::default() }).unwrap();
    agent.actor_id
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The values after `flag`, up to the next `--` option.
fn values(argv: &[String], flag: &str) -> Vec<String> {
    let Some(at) = argv.iter().position(|a| a == flag) else { return vec![] };
    argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect()
}

fn notes(st: &AppState, run_id: &str) -> Vec<String> {
    gizai_lib::runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

/// Never Claude in Chrome, and never an option that connects the browser to a running one or a real profile.
fn never_a_real_browser(argv: &[String], config: Option<&Value>) {
    assert!(!argv.iter().any(|a| a == "--chrome" || a.starts_with("--chrome=")), "--chrome is never passed: {argv:?}");
    let Some(c) = config else { return };
    for (name, s) in c["mcpServers"].as_object().unwrap() {
        for a in s["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str()) {
            let opt = a.split('=').next().unwrap();
            assert!(!gizai_agents::browser::FORBIDDEN.contains(&opt), "{name} got {a}");
        }
    }
}

// ---- task runs ----

struct RunSetup {
    tmp: tempfile::TempDir,
    st: AppState,
    agent: String,
    cli: String,
    repo: PathBuf,
    out: PathBuf,
    program: PathBuf,
}

/// What one run's fake Claude Code got.
struct Got {
    run_id: String,
    argv: Vec<String>,
    prompt: String,
    config: Option<Value>,
}

impl Got {
    fn allowed(&self) -> Vec<String> {
        values(&self.argv, "--allowedTools")
    }
    fn browser(&self) -> Option<&Value> {
        self.config.as_ref().and_then(|c| c["mcpServers"].get("chrome-devtools"))
    }
}

/// The backend agent on a fake Claude Code that keeps what it got (fake-claude-mcp-run.sh); the browser's program is a
/// stand-in. `init`: the fake's init line comes from that file.
fn run_setup(init: Option<&str>) -> RunSetup {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_RUN.to_string()).unwrap();
    // only the runs a test starts
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let out = tmp.path().join("fake-out");
    let mut env = vec![format!("FAKE_MCP_OUT={}", out.display())];
    if let Some(i) = init {
        env.push(format!("FAKE_MCP_INIT={i}"));
    }
    let cli = add_cli(&st, "Claude Code (web)", "claude_code", FAKE_RUN, &env);
    let agent = put_agent_on(&st, &cli);
    let program = stand_in(&tmp.path().join("bin/chromium"));
    app_mcp::save_browser(&st, BrowserEntry { version: "1.10.1".into(), program: program.display().to_string() }).unwrap();
    RunSetup { tmp, st, agent, cli, repo, out, program }
}

impl RunSetup {
    fn task(&self) -> String {
        gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend")
    }
    fn got(&self, run_id: &str) -> Got {
        let config = self.out.join("mcp.json").is_file().then(|| serde_json::from_str(&read(&self.out.join("mcp.json"))).unwrap());
        Got { run_id: run_id.into(), argv: read(&self.out.join("argv")).lines().map(str::to_string).collect(), prompt: read(&self.out.join("prompt")), config }
    }
    /// Runs a new card and returns what its Claude Code got.
    async fn run(&self) -> Got {
        let _ = std::fs::remove_dir_all(&self.out);
        let s = gizai_lib::runs::run_once(&self.st, &self.task(), None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{:?}", s.error);
        self.got(&s.run_id)
    }
    fn web(&self, t: CliTools) {
        app_mcp::save_cli_tools(&self.st, &self.agent, t).unwrap();
    }
    fn browser(&self, on: bool) {
        let mcp = if on { vec![AgentServer { server_id: core_mcp::BROWSER.into(), on: true, tools_off: vec![] }] } else { vec![] };
        app_mcp::save_agent(&self.st, &self.agent, AgentTools { mcp }).unwrap();
    }
}

fn web_tools(allowed: &[String]) -> Vec<String> {
    allowed.iter().filter(|a| a.starts_with("WebSearch") || a.starts_with("WebFetch")).cloned().collect()
}

#[tokio::test]
async fn a_claude_code_run_gets_websearch_and_webfetch_allowed_only_when_switched_on_and_never_chrome() {
    let t = run_setup(None);
    // off: nothing from the web, no untrusted line, no MCP config
    let g = t.run().await;
    assert!(web_tools(&g.allowed()).is_empty(), "{:?}", g.allowed());
    assert!(!g.prompt.contains(UNTRUSTED));
    assert!(values(&g.argv, "--mcp-config").is_empty(), "{:?}", g.argv);
    never_a_real_browser(&g.argv, None);

    for (tools, want) in [
        (CliTools { web_search: true, ..Default::default() }, vec!["WebSearch"]),
        (CliTools { web_fetch: true, ..Default::default() }, vec!["WebFetch"]),
        (CliTools { web_search: true, web_fetch: true, ..Default::default() }, vec!["WebSearch", "WebFetch"]),
        (CliTools { web_fetch: true, fetch_domains: vec!["https://Docs.rs/".into(), "*.laravel.com".into()], ..Default::default() },
         vec!["WebFetch(domain:docs.rs)", "WebFetch(domain:*.laravel.com)"]),
    ] {
        t.web(tools.clone());
        let g = t.run().await;
        assert_eq!(web_tools(&g.allowed()), want, "{tools:?}: {:?}", g.argv);
        assert!(g.allowed().iter().any(|a| a.starts_with("Bash(")), "the agent's commands stay: {:?}", g.allowed());
        // web pages and search results are data: the prompt says so, once, at the end
        assert_eq!(g.prompt.matches(UNTRUSTED).count(), 1, "{tools:?}");
        assert!(g.prompt.trim_end().ends_with(UNTRUSTED));
        never_a_real_browser(&g.argv, None);
    }
    // a plain form save (here: the agent moved to the same CLI again) keeps the switches
    put_agent_on(&t.st, &t.cli);
    assert_eq!(web_tools(&t.run().await.allowed()), ["WebFetch(domain:docs.rs)", "WebFetch(domain:*.laravel.com)"]);
    // and off again
    t.web(CliTools::default());
    let g = t.run().await;
    assert!(web_tools(&g.allowed()).is_empty() && !g.prompt.contains(UNTRUSTED), "{:?}", g.allowed());
}

#[tokio::test]
async fn the_browser_on_gives_a_run_the_chrome_devtools_server_hidden_and_throwaway_and_off_neither() {
    let t = run_setup(None);
    t.browser(true);
    let g = t.run().await;
    let b = g.browser().unwrap_or_else(|| panic!("no chrome-devtools in {:?}", g.config));
    let args: Vec<&str> = b["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    let program = format!("--executablePath={}", t.program.display());
    assert_eq!(b["command"], FAKE_NPX, "npx (GIZAI_NPX here) starts the pinned server");
    assert_eq!(args, ["-y", "chrome-devtools-mcp@1.10.1", "--headless", "--isolated", "--no-usage-statistics", "--no-performance-crux", program.as_str()]);
    assert_eq!(b["env"]["CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS"], "1");
    assert_eq!(b["env"]["CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS"], "1");
    assert!(g.allowed().contains(&"mcp__chrome-devtools".to_string()), "{:?}", g.allowed());
    assert!(g.argv.iter().any(|a| a == "--strict-mcp-config"));
    assert_eq!(g.prompt.matches(UNTRUSTED).count(), 1, "browser pages are data");
    never_a_real_browser(&g.argv, g.config.as_ref());
    assert!(!notes(&t.st, &g.run_id).iter().any(|n| n.contains("Left out")), "{:?}", notes(&t.st, &g.run_id));

    // self-signed certificates: only with the agent's switch
    t.web(CliTools { insecure_certs: true, ..Default::default() });
    let g = t.run().await;
    assert!(g.browser().unwrap()["args"].as_array().unwrap().iter().any(|a| a == "--acceptInsecureCerts"));

    // off: no server, nothing allowed, no untrusted line
    t.browser(false);
    let g = t.run().await;
    assert!(g.browser().is_none() && g.config.is_none(), "{:?}", g.config);
    assert!(!g.allowed().iter().any(|a| a.starts_with("mcp__chrome-devtools")), "{:?}", g.allowed());
    assert!(!g.prompt.contains(UNTRUSTED));
    never_a_real_browser(&g.argv, None);
}

#[tokio::test]
async fn a_browser_program_that_leads_to_brave_is_left_out_of_the_run_with_a_note() {
    let t = run_setup(None);
    t.browser(true);
    // saved behind Settings' back (the form refuses it): the run checks the program again
    let brave = stand_in(&t.tmp.path().join("opt/brave.com/brave/brave-browser"));
    core_mcp::set_browser(&t.st.db, BrowserEntry { version: "1.10.1".into(), program: brave.display().to_string() }).unwrap();
    let g = t.run().await;
    assert!(g.browser().is_none(), "{:?}", g.config);
    assert!(!g.allowed().iter().any(|a| a.starts_with("mcp__chrome-devtools")));
    let said = notes(&t.st, &g.run_id);
    assert!(said.iter().any(|n| n.contains("Left out the browser") && n.contains("Brave")), "{said:?}");
    // a version saved behind its back that isn't exact is refused the same way
    core_mcp::set_browser(&t.st.db, BrowserEntry { version: "1.10.1".into(), program: t.program.display().to_string() }).unwrap();
    t.st.db.write(None, |w| {
        w.conn().execute("UPDATE settings SET value_json=?1 WHERE key='mcp_browser'", [json!({"version": "latest", "program": ""}).to_string()])?;
        Ok(())
    }).unwrap();
    let g = t.run().await;
    assert!(g.browser().is_none(), "{:?}", g.config);
    assert!(notes(&t.st, &g.run_id).iter().any(|n| n.contains("Left out the browser")), "{:?}", notes(&t.st, &g.run_id));
}

#[tokio::test]
async fn a_continued_run_gets_the_same_web_tools_and_browser_as_a_fresh_one() {
    let t = run_setup(None);
    t.browser(true);
    t.web(CliTools { web_search: true, web_fetch: true, fetch_domains: vec!["docs.rs".into()], ..Default::default() });
    let task = t.task();
    gizai_core::tasks::update(&t.st.db, &t.st.you_id, &task, TaskPatch { description_md: Some("FAKE_MCP_HANG".into()), ..Default::default() }).unwrap();
    let _ = std::fs::remove_dir_all(&t.out);
    let (run_id, done) = gizai_lib::runs::start(&t.st, &task, None, None, "manual").await.unwrap();
    let t0 = Instant::now();
    while !t.out.join("prompt").exists() {
        assert!(t0.elapsed() < Duration::from_secs(15), "the fake never started");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let fresh = t.got(&run_id);
    gizai_lib::runs::stop(&t.st, &run_id);
    let s = tokio::time::timeout(Duration::from_secs(20), done).await.expect("stopped").unwrap();
    assert_eq!(s.status, "cancelled", "{:?}", s.error);

    gizai_core::tasks::update(&t.st.db, &t.st.you_id, &task, TaskPatch { description_md: Some("Export the invoices".into()), ..Default::default() }).unwrap();
    let _ = std::fs::remove_dir_all(&t.out);
    let (id, done) = gizai_lib::runs::continue_run(&t.st, &run_id, None).await.unwrap();
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("continued run ended").unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let cont = t.got(&id);
    assert!(cont.argv.iter().any(|a| a == "--resume"), "{:?}", cont.argv);
    assert_eq!(web_tools(&cont.allowed()), web_tools(&fresh.allowed()));
    assert_eq!(web_tools(&cont.allowed()), ["WebSearch", "WebFetch(domain:docs.rs)"]);
    assert!(cont.allowed().contains(&"mcp__chrome-devtools".to_string()));
    assert_eq!(cont.browser(), fresh.browser(), "the same browser server");
    assert!(cont.browser().is_some());
    assert!(cont.prompt.contains(UNTRUSTED));
}

// ---- Settings → MCP servers: the built-in browser ----

#[tokio::test]
async fn the_built_in_browser_entry_is_pinned_and_only_its_version_and_program_change_never_brave_or_latest() {
    let t = run_setup(None);
    let st = &t.st;
    let v = app_mcp::browser_view(st).unwrap();
    assert_eq!((v.id.as_str(), v.version.as_str()), ("chrome-devtools", "1.10.1"));
    assert!(v.command.starts_with("npx -y chrome-devtools-mcp@1.10.1 --headless --isolated --no-usage-statistics --no-performance-crux"), "{}", v.command);
    assert!(v.command.ends_with(&format!("--executablePath={}", t.program.display())), "{}", v.command);
    assert_eq!(v.needs.browser.as_deref(), Some(t.program.to_str().unwrap()));

    let brave = stand_in(&t.tmp.path().join("opt/brave.com/brave/brave"));
    let link = t.tmp.path().join("bin/browser");
    std::os::unix::fs::symlink(&brave, &link).unwrap();
    for (version, program, why) in [
        ("1.10.1", "/usr/bin/brave".to_string(), "Brave"), ("1.10.1", brave.display().to_string(), "Brave"), ("1.10.1", link.display().to_string(), "Brave"),
        ("1.10.1", "chromium".to_string(), "full path"), ("latest", String::new(), "exact version"), ("^1.10.1", String::new(), "exact version"),
        ("1.10", String::new(), "exact version"),
    ] {
        let e = app_mcp::save_browser(st, BrowserEntry { version: version.into(), program: program.clone() }).unwrap_err();
        assert!(e.contains(why), "{version} {program}: {e}");
    }
    let v = app_mcp::browser_view(st).unwrap();
    assert_eq!((v.version.as_str(), v.program.as_str()), ("1.10.1", t.program.to_str().unwrap()), "nothing changed");

    let other = stand_in(&t.tmp.path().join("bin/chromium-dev"));
    let v = app_mcp::save_browser(st, BrowserEntry { version: "1.11.0".into(), program: other.display().to_string() }).unwrap();
    assert_eq!(v.version, "1.11.0");
    assert!(v.command.starts_with("npx -y chrome-devtools-mcp@1.11.0 --headless --isolated "), "{}", v.command);
    assert!(v.command.contains(&format!("--executablePath={}", other.display())));
    // the agents that have it on
    t.browser(true);
    assert_eq!(app_mcp::browser_view(st).unwrap().used_by, ["Backend Agent"]);
}

// ---- what the CLI reports, and the built-in tools ----

#[tokio::test]
async fn the_tools_a_run_reports_are_shown_as_seen_in_the_last_run_and_one_the_catalog_doesnt_know_shows_under_other_off() {
    let t = run_setup(Some(TOOLS_INIT));
    let st = &t.st;
    let before = app_mcp::tools_view(st, Some(&t.agent), &t.cli).unwrap();
    assert!(before.builtin.source.contains("after its first run"), "{}", before.builtin.source);
    assert!(before.builtin.can_ask);
    assert!(!before.builtin.tools.iter().any(|x| x.reported));

    let g = t.run().await;
    let seen = core_mcp::seen_tools(&st.db).unwrap();
    assert_eq!(seen[&t.agent].tools, ["Read", "Edit", "Bash", "WebSearch", "FancyNewTool"], "MCP tools left out");
    assert_eq!(seen[&t.agent].cli_id, t.cli);
    let v = app_mcp::tools_view(st, Some(&t.agent), &t.cli).unwrap();
    assert!(v.builtin.source.contains("reported in this agent's last run"), "{}", v.builtin.source);
    let get = |id: &str| v.builtin.tools.iter().find(|x| x.id == id).unwrap_or_else(|| panic!("{id} missing"));
    assert!(get("Read").reported && get("WebSearch").reported && !get("Glob").reported);
    let fancy = get("FancyNewTool");
    assert_eq!((fancy.group.as_str(), fancy.risk.as_str(), fancy.how.as_str()), ("other", "unknown", "switch"));
    assert!(!v.saved.as_ref().unwrap().builtin.contains(&"FancyNewTool".to_string()), "off until you switch it on");
    assert!(!g.allowed().contains(&"FancyNewTool".to_string()));
    // another agent, or the same one on another CLI, doesn't see this agent's list
    let new = app_mcp::tools_view(st, None, &t.cli).unwrap();
    assert!(new.saved.is_none() && !new.builtin.tools.iter().any(|x| x.id == "FancyNewTool"));

    // switched on: allowed in its task runs; a catalog tool with no switch of its own is refused
    app_mcp::save_cli_tools(st, &t.agent, CliTools { builtin: vec!["FancyNewTool".into()], ..Default::default() }).unwrap();
    assert!(t.run().await.allowed().contains(&"FancyNewTool".to_string()));
    for id in ["Skill", "Bash", "WebSearch", "Read"] {
        let e = app_mcp::save_cli_tools(st, &t.agent, CliTools { builtin: vec![id.into()], ..Default::default() }).unwrap_err();
        assert!(e.contains("can't be switched on"), "{id}: {e}");
    }
}

// ---- Codex and Gemini ----

#[tokio::test]
async fn codex_and_gemini_agents_get_only_what_their_cli_takes_and_the_rest_is_refused_with_why() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_RUN.to_string()).unwrap();
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let browser_on = AgentTools { mcp: vec![AgentServer { server_id: core_mcp::BROWSER.into(), on: true, tools_off: vec![] }] };
    for (name, kind, ok, refused, flag) in [
        ("Codex", "codex", CliTools { web_search: true, ..Default::default() }, CliTools { web_fetch: true, ..Default::default() }, r#"web_search="live""#),
        ("Gemini", "gemini", CliTools { web_fetch: true, ..Default::default() }, CliTools { web_search: true, ..Default::default() }, "--allowed-tools=web_fetch"),
    ] {
        let cli = add_cli(&st, name, kind, FAKE_CLI, &[format!("FAKE_KIND={kind}")]);
        let agent = put_agent_on(&st, &cli);
        let v = app_mcp::tools_view(&st, Some(&agent), &cli).unwrap();
        assert!(v.browser.disabled.as_deref().is_some_and(|w| w.contains(name)), "{name}: {:?}", v.browser.disabled);
        assert!(v.builtin.source.starts_with("From Gizai's catalog"), "{name}: {}", v.builtin.source);
        assert!(!v.builtin.can_ask);
        let e = app_mcp::save_cli_tools(&st, &agent, refused.clone()).unwrap_err();
        assert!(e.contains(name), "{name}: {e}");
        let e = app_mcp::save_cli_tools(&st, &agent, CliTools { web_fetch: true, fetch_domains: vec!["docs.rs".into()], ..Default::default() }).unwrap_err();
        assert!(e.contains(name), "{name}: domains: {e}");
        assert!(app_mcp::save_agent(&st, &agent, browser_on.clone()).is_err(), "{name}: no browser yet");
        app_mcp::save_cli_tools(&st, &agent, ok.clone()).unwrap();

        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{name}: {:?}", s.error);
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        let stderr = read(&Path::new(&run.log_path).with_extension("stderr.log"));
        let argv = stderr.lines().find_map(|l| l.strip_prefix("argv: ")).unwrap_or_else(|| panic!("{name}: no argv in {stderr}"));
        assert!(argv.contains(flag), "{name}: {argv}");
        assert!(!argv.contains("mcp") && !argv.contains("chrome-devtools") && !argv.contains("--chrome"), "{name}: {argv}");
        // nothing written into the worktree for the CLI's settings
        let wt = PathBuf::from(run.worktree_path.unwrap());
        for d in [".codex", ".gemini", ".mcp.json"] {
            assert!(!wt.join(d).exists(), "{name}: {d} in the worktree");
        }
        // off again for the next CLI
        app_mcp::save_cli_tools(&st, &agent, CliTools::default()).unwrap();
    }
}

/// Gemini searches the web in every run by its own policy (the form shows Web search as always on), so search results reach
/// every Gemini agent: its prompt says that content is data, as for an agent with web search switched on.
#[tokio::test]
async fn a_gemini_agent_searches_the_web_in_every_run_so_its_prompt_says_web_content_is_data() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_RUN.to_string()).unwrap();
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let cli = add_cli(&st, "Gemini", "gemini", FAKE_CLI, &["FAKE_KIND=gemini".into(), "FAKE_TEMP=1".into()]);
    let agent = put_agent_on(&st, &cli);
    let v = app_mcp::tools_view(&st, Some(&agent), &cli).unwrap();
    assert_eq!(v.builtin.tools.iter().find(|x| x.id == "google_web_search").map(|x| x.how.as_str()), Some("always"));
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let stderr = read(&Path::new(&run.log_path).with_extension("stderr.log"));
    let prompt = stderr.split("prompt>>").nth(1).and_then(|p| p.split("<<prompt").next()).unwrap_or_else(|| panic!("no prompt in {stderr}"));
    assert!(prompt.contains(UNTRUSTED), "a Gemini agent gets search results in every run, but its prompt doesn't say they are data");
}

// ---- the Team Lead's chat ----

struct ChatSetup {
    _tmp: tempfile::TempDir,
    st: AppState,
    lead: String,
}

fn chat_setup() -> ChatSetup {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    // The fake starts no MCP server: the helper only has to be there.
    st.mcp_shim = Some(PathBuf::from(FAKE_CHAT));
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CHAT.to_string()).unwrap();
    gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    ChatSetup { _tmp: tmp, st, lead }
}

impl ChatSetup {
    /// One chat turn; what its Claude Code got: {argv, prompt, path, mode, config}.
    async fn turn(&self, text: &str) -> Value {
        let (_, done) = app_chat::send(&self.st, None, text.into(), None).await.unwrap();
        let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        assert_eq!(s.status, "succeeded", "{s:?}");
        std::fs::read_to_string(self.st.data_dir.join("chat/fake-mcp-calls.jsonl")).unwrap().lines().last().map(|l| serde_json::from_str(l).unwrap()).unwrap()
    }
}

fn argv_in(call: &Value) -> Vec<String> {
    call["argv"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()
}

fn system_prompt(argv: &[String]) -> String {
    argv[argv.iter().position(|a| a == "--append-system-prompt").unwrap() + 1].clone()
}

#[tokio::test]
async fn the_team_leads_chat_keeps_read_glob_grep_and_gets_the_web_tools_only_when_on() {
    let t = chat_setup();
    let st = &t.st;
    let argv = argv_in(&t.turn("What is on the board?").await);
    assert_eq!(values(&argv, "--tools"), ["Read,Glob,Grep"], "off: only the tools that read");
    assert!(web_tools(&values(&argv, "--allowedTools")).is_empty(), "{argv:?}");
    assert!(!system_prompt(&argv).contains(UNTRUSTED));
    never_a_real_browser(&argv, None);

    for (tools, list, allowed) in [
        (CliTools { web_search: true, ..Default::default() }, "Read,Glob,Grep,WebSearch", vec!["WebSearch"]),
        (CliTools { web_fetch: true, fetch_domains: vec!["docs.rs".into()], ..Default::default() }, "Read,Glob,Grep,WebFetch", vec!["WebFetch(domain:docs.rs)"]),
        (CliTools { web_search: true, web_fetch: true, builtin: vec!["FancyNewTool".into()], ..Default::default() }, "Read,Glob,Grep,WebSearch,WebFetch",
         vec!["WebSearch", "WebFetch"]),
    ] {
        app_mcp::save_cli_tools(st, &t.lead, tools.clone()).unwrap();
        let argv = argv_in(&t.turn("Look up the Laravel docs").await);
        assert_eq!(values(&argv, "--tools"), [list], "{tools:?}");
        let allow = values(&argv, "--allowedTools");
        assert_eq!(web_tools(&allow), allowed, "{tools:?}: {allow:?}");
        assert!(!allow.contains(&"FancyNewTool".to_string()), "built-in tools are for task runs only: {allow:?}");
        assert!(system_prompt(&argv).contains(UNTRUSTED), "web content is data");
        never_a_real_browser(&argv, None);
    }
    app_mcp::save_cli_tools(st, &t.lead, CliTools::default()).unwrap();
    let argv = argv_in(&t.turn("And now?").await);
    assert_eq!(values(&argv, "--tools"), ["Read,Glob,Grep"]);
    assert!(web_tools(&values(&argv, "--allowedTools")).is_empty());
}

#[tokio::test]
async fn the_team_leads_chat_gets_the_browser_next_to_gizai_when_it_is_on() {
    let t = chat_setup();
    let st = &t.st;
    let program = stand_in(&t._tmp.path().join("bin/chromium"));
    app_mcp::save_browser(st, BrowserEntry { version: "1.10.1".into(), program: program.display().to_string() }).unwrap();
    app_mcp::save_agent(st, &t.lead, AgentTools { mcp: vec![AgentServer { server_id: core_mcp::BROWSER.into(), on: true, tools_off: vec![] }] }).unwrap();
    let call = t.turn("Check the login page of kade.test").await;
    let argv = argv_in(&call);
    // the fake keeps the config's mcpServers
    let config = json!({"mcpServers": call["config"].clone()});
    let servers = config["mcpServers"].as_object().unwrap_or_else(|| panic!("{call}"));
    assert!(servers.contains_key("gizai") && servers.contains_key("chrome-devtools"), "{config}");
    let args: Vec<&str> = servers["chrome-devtools"]["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    assert!(args.contains(&"--headless") && args.contains(&"--isolated"), "{args:?}");
    assert!(values(&argv, "--allowedTools").contains(&"mcp__chrome-devtools".to_string()), "{argv:?}");
    assert_eq!(values(&argv, "--tools"), ["Read,Glob,Grep"], "the browser comes as an MCP server, not a built-in tool");
    assert!(system_prompt(&argv).contains(UNTRUSTED));
    never_a_real_browser(&argv, Some(&config));
}

// ---- the Team Lead's rules ----

/// The shim binary: target/debug/gizai-mcp (built here when missing).
fn shim() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/gizai-mcp"));
    BUILT.call_once(|| {
        if path.is_file() {
            return;
        }
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = std::process::Command::new(cargo).args(["build", "-q", "-p", "gizai-mcp"])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).status().map(|s| s.success()).unwrap_or(false);
        assert!(ok && path.is_file(), "could not build gizai-mcp");
    });
    path
}

struct Lead {
    _tmp: tempfile::TempDir,
    st: AppState,
    lead: String,
    backend: String,
}

fn lead_setup() -> Lead {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    st.mcp_shim = Some(shim());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_OUTSIDE.to_string()).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let backend = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    gizai_core::projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    Lead { _tmp: tmp, st, lead, backend }
}

#[tokio::test]
async fn after_a_chat_answer_used_a_web_tool_or_the_browser_the_acting_tools_are_refused_as_after_an_outside_mcp_tool() {
    let t = lead_setup();
    let st = &t.st;
    let _server = mcp::start(st).unwrap();
    for tool in ["WebSearch", "WebFetch", "mcp__chrome-devtools__navigate_page"] {
        assert!(app_chat::is_outside_tool(tool), "{tool}");
        let pause = r#"CALL set_agent_status {"agent": "Backend Agent", "status": "paused"}"#;
        let (thread, done) = app_chat::send(st, None, format!("OUTSIDE OUTSIDE_TOOL={tool} Look it up, then {pause}"), None).await.unwrap();
        let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
        assert_eq!(s.status, "succeeded", "{tool}: {s:?}");
        let msgs: Vec<(String, bool, String)> = gizai_core::chat::messages(&st.db, &thread).unwrap().into_iter().filter(|m| m.role == "tool").map(|m| {
            let x = m.tool.unwrap_or_default();
            (m.tool_name.unwrap_or_default(), x["isError"].as_bool().unwrap_or(false), x["result"].as_str().unwrap_or_default().to_string())
        }).collect();
        assert_eq!(msgs.iter().map(|m| m.0.as_str()).collect::<Vec<_>>(), [tool, "mcp__gizai__set_agent_status"], "{msgs:?}");
        let (_, is_error, text) = &msgs[1];
        assert!(*is_error && text.starts_with("set_agent_status is refused for the rest of this answer") && text.contains(tool)
                && text.contains("confirm it in a new message"), "{tool}: {msgs:?}");
        assert_eq!(gizai_core::team::agent(&st.db, &t.backend).unwrap().status, "active", "{tool}: nothing paused");
        assert_eq!(app_chat::used_outside(st, &thread).as_deref(), Some(tool));
    }
}

#[tokio::test]
async fn create_agent_and_update_agent_cant_change_the_web_browser_or_built_in_tool_switches() {
    let t = lead_setup();
    let st = &t.st;
    let saved = app_mcp::save_cli_tools(st, &t.backend, CliTools { web_fetch: true, fetch_domains: vec!["docs.rs".into()], ..Default::default() }).unwrap();
    let before_mcp = core_mcp::agent_tools(&st.db, &t.backend).unwrap();
    let lead = t.lead.as_str();
    let call = |name: &'static str, args: Value| async move { tools::call_in(st, lead, None, name, args).await };
    for (key, value) in [("web_search", json!(true)), ("web_fetch", json!(false)), ("fetch_domains", json!(["evil.example"])), ("web", json!({"search": true})),
                         ("browser", json!(true)), ("insecure_certs", json!(true)), ("builtin_tools", json!(["FancyNewTool"])), ("builtin", json!(["Skill"])),
                         ("cli_tools", json!({"webSearch": true}))] {
        let e = call("update_agent", json!({"agent": "Backend Agent", key: value.clone()})).await.unwrap_err();
        assert!(e.contains("only the user") && e.contains("Nothing changed"), "update_agent {key}: {e}");
        let e = call("create_agent", json!({"name": format!("Web {key}"), "role": "qa", key: value})).await.unwrap_err();
        assert!(e.contains("only the user"), "create_agent {key}: {e}");
    }
    assert_eq!(core_mcp::agent_cli_tools(&st.db, &t.backend).unwrap(), saved, "the switches are as they were");
    assert_eq!(core_mcp::agent_tools(&st.db, &t.backend).unwrap(), before_mcp);
    assert!(!gizai_core::team::all_agents(&st.db).unwrap().iter().any(|(_, m)| m.name.starts_with("Web ")), "no agent was added");
    // a plain update_agent (another field) keeps them
    call("update_agent", json!({"agent": "Backend Agent", "name": "Backend Agent 2"})).await.unwrap();
    assert_eq!(core_mcp::agent_cli_tools(&st.db, &t.backend).unwrap(), saved);
    // get_agent shows them
    let v = call("get_agent", json!({"agent": "Backend Agent 2"})).await.unwrap()["agent"].clone();
    assert_eq!((v["web_search"].clone(), v["web_fetch"].clone(), v["fetch_domains"].clone(), v["browser"].clone()),
               (json!(false), json!(true), json!(["docs.rs"]), json!(false)), "{v}");
    // the tools' own descriptions say so, and none takes such a field
    let catalog = tools::catalog();
    let update = catalog.iter().find(|d| d.name == "update_agent").unwrap();
    assert!(update.description.contains("web search, fetching pages, the browser"), "{}", update.description);
    for d in catalog.iter().filter(|d| d.name == "update_agent" || d.name == "create_agent") {
        let props: Vec<&String> = d.input_schema["properties"].as_object().unwrap().keys().collect();
        assert!(!props.iter().any(|p| p.contains("web") || p.contains("browser") || p.contains("builtin") || p.contains("cert")), "{}: {props:?}", d.name);
    }
}

/// The Team Lead may set an agent's allowed commands (update_agent's allowed_tools), and a task run passes them to
/// --allowedTools. Naming WebSearch, WebFetch or a built-in tool there must not give a run what only the user's switches
/// give: either update_agent refuses it, or the run doesn't get it.
#[tokio::test]
async fn the_team_lead_cant_give_an_agent_web_tools_through_its_allowed_commands_either() {
    let t = run_setup(None);
    let st = &t.st;
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let r = tools::call_in(st, &lead, None, "update_agent", json!({"agent": "Backend Agent",
        "allowed_tools": ["Bash(git status:*)", "WebSearch", "WebFetch", "WebFetch(domain:evil.example)", "FancyNewTool"]})).await;
    eprintln!("update_agent with web tools in allowed_tools: {}", match &r { Ok(_) => "accepted".to_string(), Err(e) => format!("refused: {e}") });
    assert_eq!(core_mcp::agent_cli_tools(&st.db, &t.agent).unwrap(), CliTools::default(), "the switches are still off");
    let g = t.run().await;
    let allowed = g.allowed();
    assert!(web_tools(&allowed).is_empty() && !allowed.contains(&"FancyNewTool".to_string()),
            "a run got web or built-in tools the user never switched on, through the Team Lead's allowed_tools: {allowed:?}");
}

/// update_agent and create_agent take only commands (`Bash(…)`) in allowed_tools: any other entry is refused, and nothing
/// changes; a list of commands, or none (the role's list), still works.
#[tokio::test]
async fn the_team_leads_allowed_tools_take_only_commands_and_anything_else_changes_nothing() {
    let t = lead_setup();
    let st = &t.st;
    let lead = t.lead.as_str();
    let call = |name: &'static str, args: Value| async move { tools::call_in(st, lead, None, name, args).await };
    let before = gizai_core::team::agent(&st.db, &t.backend).unwrap().allowed_tools;
    let agents = gizai_core::team::all_agents(&st.db).unwrap().len();
    for bad in ["WebSearch", "WebFetch", "WebFetch(domain:evil.example)", "FancyNewTool", "Skill", "Read", "Read(//home/**)", "Edit(//etc/**)",
                "mcp__chrome-devtools", "mcp__otus__get_card", "bash(git status:*)", "Bash()", "Bash( )", "Bash(git status:*"] {
        let list = json!(["Bash(git status:*)", bad]);
        let e = call("update_agent", json!({"agent": "Backend Agent", "allowed_tools": list.clone()})).await.unwrap_err();
        assert!(e.contains("allowed_tools takes only commands") && e.contains(bad) && e.contains("Nothing changed"), "update_agent {bad}: {e}");
        assert_eq!(gizai_core::team::agent(&st.db, &t.backend).unwrap().allowed_tools, before, "{bad}: the list is as it was");
        let e = call("create_agent", json!({"name": "Web QA", "role": "qa", "allowed_tools": list})).await.unwrap_err();
        assert!(e.contains("allowed_tools takes only commands"), "create_agent {bad}: {e}");
    }
    assert_eq!(gizai_core::team::all_agents(&st.db).unwrap().len(), agents, "no agent was added");
    // any command is still refused with its own reason
    let e = call("update_agent", json!({"agent": "Backend Agent", "allowed_tools": ["Bash"]})).await.unwrap_err();
    assert!(e.contains("can't be allowed to run any command"), "{e}");
    // commands only: saved
    call("update_agent", json!({"agent": "Backend Agent", "allowed_tools": ["Bash(git status:*)", " Bash(npm test:*) "]})).await.unwrap();
    assert!(gizai_core::team::agent(&st.db, &t.backend).unwrap().allowed_tools.iter().any(|a| a.contains("npm test")));
    // no list: the role's, which is commands only, so a run leaves nothing of it out
    call("create_agent", json!({"name": "Web QA", "role": "qa"})).await.unwrap();
    for role in ["lead", "backend", "frontend", "qa", "devops", "design"] {
        let left: Vec<String> = gizai_core::seed::role_tools(role).into_iter().filter(|t| gizai_agents::tool_catalog::only_by_switch(t)).collect();
        assert!(left.is_empty(), "{role}: {left:?}");
    }
}

/// A run takes web search, fetching pages and the CLI's other tools only from their switches: an allowed commands entry
/// for them (an old list, or a person typing in the form) is left out with a note, the rest of the list stays, and the
/// switches still give their own tools, once.
#[tokio::test]
async fn a_run_leaves_web_and_built_in_tools_out_of_the_allowed_commands_and_the_switches_still_give_them() {
    let t = run_setup(None);
    let typed = ["Bash(git status:*)", "Bash(npm test:*)", "Read(//srv/docs/**)", "Edit", "mcp__otus__get_card", "WebSearch",
                 "WebFetch(domain:evil.example)", "FancyNewTool", "Skill", "AskUserQuestion"];
    let left_out = ["WebSearch", "WebFetch(domain:evil.example)", "FancyNewTool", "Skill", "AskUserQuestion"];
    let (_, agent) = gizai_core::team::all_agents(&t.st.db).unwrap().into_iter().find(|(_, m)| m.actor_id == t.agent).unwrap();
    gizai_core::team::update_agent(&t.st.db, &t.st.you_id, &t.agent, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: t.cli.clone(), allowed_tools: typed.map(String::from).to_vec(), ..Default::default() }).unwrap();

    let g = t.run().await;
    let allowed = g.allowed();
    for kept in ["Bash(git status:*)", "Bash(npm test:*)", "Read(//srv/docs/**)", "Edit", "mcp__otus__get_card"] {
        assert!(allowed.iter().any(|a| a == kept), "{kept} stays: {allowed:?}");
    }
    for l in left_out {
        assert!(!allowed.iter().any(|a| a == l), "{l} is left out: {allowed:?}");
    }
    assert!(web_tools(&allowed).is_empty(), "{allowed:?}");
    assert!(!g.prompt.contains(UNTRUSTED), "nothing from the web is on");
    assert!(!g.prompt.contains("FancyNewTool") && !g.prompt.contains("evil.example"), "the prompt's commands are the run's: {}", g.prompt);
    let n = notes(&t.st, &g.run_id);
    let note = n.iter().find(|x| x.starts_with("Left out of")).unwrap_or_else(|| panic!("no note: {n:?}"));
    for l in left_out {
        assert!(note.contains(l), "{l} named in {note}");
    }
    assert!(!note.contains("Bash(") && !note.contains("mcp__"), "{note}");

    // the switches give their own tools, once each; the typed domain list stays out
    app_mcp::save_cli_tools(&t.st, &t.agent, CliTools { web_search: true, builtin: vec!["FancyNewTool".into()], ..Default::default() }).unwrap();
    let g = t.run().await;
    let allowed = g.allowed();
    assert_eq!(web_tools(&allowed), ["WebSearch"], "{allowed:?}");
    assert_eq!(allowed.iter().filter(|a| *a == "FancyNewTool").count(), 1, "{allowed:?}");
    assert!(!allowed.iter().any(|a| a == "Skill" || a == "AskUserQuestion"), "{allowed:?}");
    assert_eq!(g.prompt.matches(UNTRUSTED).count(), 1);
    never_a_real_browser(&g.argv, g.config.as_ref());
}

/// Gemini's line about web content holds for a continued run too, once, and only Gemini gets it with every switch off.
#[tokio::test]
async fn a_continued_gemini_run_says_web_content_is_data_once_and_codex_with_search_off_doesnt() {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_RUN.to_string()).unwrap();
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let prompt_of = |run_id: &str| {
        let run = gizai_core::runs::get(&st.db, run_id).unwrap();
        let stderr = read(&Path::new(&run.log_path).with_extension("stderr.log"));
        stderr.split("prompt>>").nth(1).and_then(|p| p.split("<<prompt").next()).unwrap_or_else(|| panic!("no prompt in {stderr}")).to_string()
    };

    let codex = add_cli(&st, "Codex", "codex", FAKE_CLI, &["FAKE_KIND=codex".into(), "FAKE_TEMP=1".into()]);
    put_agent_on(&st, &codex);
    let s = gizai_lib::runs::run_once(&st, &gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend"), None, None).await.unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    assert!(!prompt_of(&s.run_id).contains(UNTRUSTED), "Codex with web search off reaches no web content");

    let gemini = add_cli(&st, "Gemini", "gemini", FAKE_CLI, &["FAKE_KIND=gemini".into(), "FAKE_TEMP=1".into()]);
    put_agent_on(&st, &gemini);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    // the fake ends without a result line, so the run can be continued
    gizai_core::tasks::update(&st.db, &st.you_id, &task, TaskPatch { description_md: Some("FAKE_NO_RESULT".into()), ..Default::default() }).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(prompt_of(&s.run_id).matches(UNTRUSTED).count(), 1, "a fresh Gemini run");
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!((run.status.as_str(), run.outcome.as_deref()), ("succeeded", Some("no_result")), "the fake ended without a result");
    let (id, done) = gizai_lib::runs::continue_run(&st, &s.run_id, None).await.unwrap();
    let c = tokio::time::timeout(Duration::from_secs(30), done).await.expect("continued run ended").unwrap();
    assert_eq!(c.status, "succeeded", "{:?}", c.error);
    let run = gizai_core::runs::get(&st.db, &id).unwrap();
    assert!(read(&Path::new(&run.log_path).with_extension("stderr.log")).contains("--resume"), "a continued run");
    assert_eq!(prompt_of(&id).matches(UNTRUSTED).count(), 1, "a continued Gemini run");
}

// ---- Stop, the tool cap and quitting end the browser ----

/// A process's state letter and process group from /proc (None once it is gone).
#[cfg(target_os = "linux")]
fn stat(pid: u32) -> Option<(char, u32)> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &s[s.rfind(')')? + 1..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some((f.first()?.chars().next()?, f.get(2)?.parse().ok()?))
}

/// macOS, which has no /proc: the same from `ps`.
#[cfg(not(target_os = "linux"))]
fn stat(pid: u32) -> Option<(char, u32)> {
    let out = std::process::Command::new("ps").args(["-o", "stat=,pgid=", "-p", &pid.to_string()]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut f = text.split_whitespace();
    Some((f.next()?.chars().next()?, f.next()?.parse().ok()?))
}

fn ended(pid: u32) -> bool {
    stat(pid).is_none_or(|(state, _)| state == 'Z' || state == 'X')
}

/// (claude, server, browser) as the fakes wrote them; on drop, any still running (ours by its command line) gets SIGKILL.
struct Pids([u32; 3]);

impl Drop for Pids {
    fn drop(&mut self) {
        for pid in self.0 {
            let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).map(|b| String::from_utf8_lossy(&b).replace('\0', " ")).unwrap_or_default();
            if pid > 1 && !ended(pid) && (cmd.contains("sleep") || cmd.contains("fake-mcp-server") || cmd.contains("fake-claude-browser")) {
                // SAFETY: a plain kill of a process this test's fakes started (checked by its command line just above).
                unsafe { libc::kill(pid as i32, libc::SIGKILL); }
            }
        }
    }
}

impl Pids {
    fn alive(&self) -> Vec<(&'static str, u32)> {
        ["claude", "server", "browser"].into_iter().zip(self.0).filter(|(_, p)| !ended(*p)).collect()
    }
    async fn all_ended(&self) -> bool {
        let t0 = Instant::now();
        while !self.0.iter().all(|p| ended(*p)) {
            if t0.elapsed() > Duration::from_secs(5) {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        true
    }
}

/// A run of the backend agent with the browser on, on fake-claude-browser.py, which starts the browser's server from the
/// run's MCP config (the fake npx: its "browser" in a process group of its own). `description`: the card's, for the fake.
async fn browser_run(mode: &str, description: &str, tool_cap: u32) -> (RunSetup, String, tokio::task::JoinHandle<gizai_lib::runs::RunSummary>, Pids) {
    let t = run_setup(None);
    gizai_core::settings::set(&t.st.db, "max_run_tool_calls", &tool_cap).unwrap();
    let pids = t.tmp.path().join("pids");
    let cli = add_cli(&t.st, "Claude Code (browser)", "claude_code", FAKE_BROWSER_CLAUDE,
                      &[format!("FAKE_BROWSER_PIDS={}", pids.display()), format!("FAKE_BROWSER_MODE={mode}")]);
    put_agent_on(&t.st, &cli);
    t.browser(true);
    let task = t.task();
    gizai_core::tasks::update(&t.st.db, &t.st.you_id, &task, TaskPatch { description_md: Some(description.into()), ..Default::default() }).unwrap();
    let (run_id, done) = gizai_lib::runs::start(&t.st, &task, None, None, "manual").await.unwrap();
    let t0 = Instant::now();
    let read = |n: &str| std::fs::read_to_string(pids.join(n)).ok().and_then(|s| s.trim().parse::<u32>().ok());
    let p = loop {
        if let (Some(a), Some(b), Some(c)) = (read("claude.pid"), read("server.pid"), read("helper.pid")) {
            break Pids([a, b, c]);
        }
        assert!(t0.elapsed() < Duration::from_secs(20), "the fake claude and the browser's server didn't start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let [claude, server, browser] = p.0;
    assert_eq!(stat(server).map(|s| s.1), Some(claude), "the server is in the run's process group");
    assert!(stat(browser).is_some_and(|s| s.1 != claude), "the browser is in a process group of its own");
    let argv = read_lines(&pids.join("npx.argv"));
    assert!(argv.contains(&"--headless".to_string()) && argv.contains(&"--isolated".to_string()), "{argv:?}");
    (t, run_id, done, p)
}

fn read_lines(p: &Path) -> Vec<String> {
    read(p).lines().map(str::to_string).collect()
}

#[tokio::test]
async fn stop_ends_the_run_the_browsers_server_and_the_browser() {
    for mode in ["int", "term"] {
        let (t, run_id, done, p) = browser_run(mode, "Test the login page", 200).await;
        gizai_lib::runs::stop(&t.st, &run_id);
        let s = tokio::time::timeout(Duration::from_secs(25), done).await.expect("the run ended").unwrap();
        assert_eq!(s.status, "cancelled", "{mode}: {:?}", s.error);
        assert!(p.all_ended().await, "{mode}: still running after Stop: {:?}", p.alive());
    }
}

#[tokio::test]
async fn the_tool_cap_ends_the_run_the_browsers_server_and_the_browser() {
    // every click is a tool call: three of them against a cap of one
    let (t, run_id, done, p) = browser_run("term", "Click through the checkout TOOLCALLS=3", 1).await;
    let s = tokio::time::timeout(Duration::from_secs(25), done).await.expect("the cap ended the run").unwrap();
    assert_ne!(s.status, "succeeded", "{s:?}");
    assert!(p.all_ended().await, "still running after the cap: {:?} ({s:?})", p.alive());
    let evs: Vec<RunEvent> = gizai_lib::runs::events_for(&t.st, &run_id).into_iter().map(|e| e.event).collect();
    assert!(evs.iter().any(|e| matches!(e, RunEvent::Other { raw_type } if raw_type.starts_with("cap_exceeded"))) || s.error.is_some(), "{s:?}");
}

#[tokio::test]
async fn quitting_ends_the_run_the_browsers_server_and_the_browser() {
    let (t, _run_id, done, p) = browser_run("term", "Test the login page", 200).await;
    assert_eq!(gizai_lib::runs::stop_all(&t.st, Duration::from_secs(12)).await, 1);
    let s = tokio::time::timeout(Duration::from_secs(15), done).await.expect("the run ended").unwrap();
    assert_eq!(s.status, "cancelled", "{:?}", s.error);
    assert!(p.all_ended().await, "still running after quitting: {:?}", p.alive());
}
