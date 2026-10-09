// GA-55: the coding CLIs' own tools (agent form → Tools). Gizai's catalog per CLI, merged with what Claude Code reports in
// its init line (never only one of them), the Web switches on each CLI's command line, and Ask Claude Code again with a
// fake Claude Code that isn't logged in (fake-claude-tools.sh), never the real one.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_agents::cli::{self, CliSpec, Kind, TaskRun, WebTools};
use gizai_agents::tool_catalog::{self, how};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
const FAKE_TOOLS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake-claude-tools.sh");

/// A committed fake, made runnable (its mode in git is 755; a checkout without it still runs).
fn executable(p: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    PathBuf::from(p)
}

fn spec(kind: Kind) -> CliSpec {
    CliSpec { kind, bin: "/bin/fake".into(), env: vec![], args: String::new() }
}

fn web(search: bool, fetch: bool, domains: &[&str]) -> WebTools {
    WebTools { search, fetch, fetch_domains: domains.iter().map(|d| d.to_string()).collect() }
}

fn args_of(kind: Kind, w: WebTools) -> Vec<String> {
    cli::task_exec(&spec(kind), &TaskRun { prompt: "Do the card".into(), session_id: "S-1".into(), allowed_tools: vec!["Bash(git status:*)".into()],
                                          web: w, ..Default::default() }).args
}

/// The values after `flag`, up to the next `--` option.
fn values(argv: &[String], flag: &str) -> Vec<String> {
    let Some(at) = argv.iter().position(|a| a == flag) else { return vec![] };
    argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect()
}

/// `-c` values, in order.
fn configs(args: &[String]) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == "-c").map(|w| w[1].clone()).collect()
}

fn no_chrome(args: &[String]) {
    assert!(!args.iter().any(|a| a == "--chrome" || a.starts_with("--chrome=") || a == "--chrome-native-host"), "Claude in Chrome is never used: {args:?}");
}

// ---- the Web switches on the command line ----

#[test]
fn a_claude_code_run_gets_websearch_and_webfetch_allowed_only_when_on_and_never_chrome() {
    let off = args_of(Kind::ClaudeCode, WebTools::default());
    assert_eq!(values(&off, "--allowedTools"), ["Bash(git status:*)"], "off by default: {off:?}");
    no_chrome(&off);
    for (w, want) in [
        (web(true, false, &[]), vec!["WebSearch"]),
        (web(false, true, &[]), vec!["WebFetch"]),
        (web(true, true, &[]), vec!["WebSearch", "WebFetch"]),
        (web(false, true, &["docs.rs", "*.laravel.com"]), vec!["WebFetch(domain:docs.rs)", "WebFetch(domain:*.laravel.com)"]),
        (web(true, true, &["docs.rs"]), vec!["WebSearch", "WebFetch(domain:docs.rs)"]),
        // a domain list with fetching off gives nothing
        (web(false, false, &["docs.rs"]), vec![]),
    ] {
        let a = args_of(Kind::ClaudeCode, w.clone());
        let allowed = values(&a, "--allowedTools");
        assert_eq!(allowed.first().map(String::as_str), Some("Bash(git status:*)"), "the agent's own commands stay: {a:?}");
        assert_eq!(&allowed[1..], want.as_slice(), "{w:?}: {a:?}");
        no_chrome(&a);
        assert!(!a.iter().any(|x| x == "--tools"), "a task run has no --tools list: {a:?}");
    }
}

#[test]
fn chat_gets_the_web_tool_names_for_its_tools_list_only_when_on() {
    assert!(tool_catalog::claude_web_names(false, false).is_empty());
    assert_eq!(tool_catalog::claude_web_names(true, false), ["WebSearch"]);
    assert_eq!(tool_catalog::claude_web_names(false, true), ["WebFetch"]);
    assert_eq!(tool_catalog::claude_web_names(true, true), ["WebSearch", "WebFetch"]);
    assert!(tool_catalog::claude_web_rules(false, false, &["docs.rs".into()]).is_empty());
}

#[test]
fn codex_gets_live_web_search_only_when_on_and_disabled_otherwise() {
    for (w, want) in [(WebTools::default(), r#"web_search="disabled""#), (web(true, false, &[]), r#"web_search="live""#),
                      (web(false, true, &[]), r#"web_search="disabled""#), (web(true, true, &["docs.rs"]), r#"web_search="live""#)] {
        let a = args_of(Kind::Codex, w.clone());
        let web_lines: Vec<String> = configs(&a).into_iter().filter(|c| c.starts_with("web_search")).collect();
        assert_eq!(web_lines, [want], "{w:?}: {a:?}");
        assert!(!configs(&a).iter().any(|c| c.starts_with("mcp_servers")), "no MCP servers for Codex yet: {a:?}");
        assert!(!a.iter().any(|x| x.contains("WebFetch") || x.contains("web_fetch")), "Codex has no fetch tool: {a:?}");
    }
}

#[test]
fn gemini_gets_web_fetch_only_when_on_and_nothing_else_from_the_web_switches() {
    let off = args_of(Kind::Gemini, WebTools::default());
    assert!(!off.iter().any(|a| a.contains("web_fetch") || a.contains("google_web_search")), "{off:?}");
    let on = args_of(Kind::Gemini, web(false, true, &[]));
    assert_eq!(on.iter().filter(|a| *a == "--allowed-tools=web_fetch").count(), 1, "{on:?}");
    let search = args_of(Kind::Gemini, web(true, false, &[]));
    assert_eq!(search, off, "Gemini's web search is its own policy's: the switch changes nothing on its command line");
    assert!(!on.iter().any(|a| a.contains("mcp") || a.contains("settings")), "no MCP servers or settings files for Gemini: {on:?}");
}

#[test]
fn another_cli_gets_nothing_from_the_web_switches() {
    assert_eq!(args_of(Kind::Other, web(true, true, &["docs.rs"])), args_of(Kind::Other, WebTools::default()));
}

// ---- the catalog ----

#[test]
fn every_catalog_tool_says_in_one_line_what_it_allows_and_how_risky_it_is() {
    let groups = ["web", "browser", "files", "commands", "agents", "planning", "other"];
    let hows = [how::WEB, how::SWITCH, how::ALWAYS, how::ELSEWHERE, how::OFF];
    for kind in [Kind::ClaudeCode, Kind::Codex, Kind::Gemini] {
        let c = tool_catalog::catalog(kind);
        assert!(!c.is_empty(), "{kind:?}");
        let mut ids: Vec<&str> = c.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.len(), "{kind:?}: an id twice");
        for t in &c {
            let d = &t.description;
            assert!(!d.is_empty() && !d.contains('\n') && d.ends_with('.') && d.len() <= 160, "{kind:?} {}: one line: {d:?}", t.id);
            assert!(["low", "medium", "high"].contains(&t.risk.as_str()), "{kind:?} {}: {}", t.id, t.risk);
            assert!(groups.contains(&t.group.as_str()) && hows.contains(&t.how.as_str()), "{kind:?} {}: {} {}", t.id, t.group, t.how);
            assert!(!t.label.is_empty() && !t.reported, "{kind:?} {}", t.id);
            if [how::ALWAYS, how::ELSEWHERE, how::OFF].contains(&t.how.as_str()) {
                assert!(!t.note.is_empty() && !t.note.contains('\n'), "{kind:?} {}: says why or where", t.id);
            }
            // nothing the catalog knows gets a switch of its own: the Web switches and unknown reported tools only
            assert_ne!(t.how, how::SWITCH, "{kind:?} {}", t.id);
        }
    }
}

#[test]
fn the_claude_code_catalog_has_the_cards_tools_with_skills_and_slash_commands_off() {
    let c = tool_catalog::catalog(Kind::ClaudeCode);
    for id in ["Read", "Glob", "Grep", "Edit", "Write", "NotebookEdit", "Bash", "BashOutput", "KillShell", "WebSearch", "WebFetch", "Task", "Agent",
               "TodoWrite", "ExitPlanMode", "AskUserQuestion", "Skill", "SlashCommand"] {
        assert!(c.iter().any(|t| t.id == id), "{id} missing from the catalog");
    }
    let get = |id: &str| tool_catalog::find(Kind::ClaudeCode, id).unwrap();
    for id in ["WebSearch", "WebFetch"] {
        assert_eq!((get(id).group.as_str(), get(id).how.as_str()), ("web", how::WEB), "{id}");
    }
    assert_eq!(get("WebFetch").risk, "high");
    for id in ["Skill", "SlashCommand", "AskUserQuestion"] {
        assert_eq!(get(id).how, how::OFF, "{id}");
    }
    assert!(get("Skill").note.contains("--disable-slash-commands"));
    assert_eq!(get("Bash").risk, "high");
    assert!(tool_catalog::source_note(Kind::ClaudeCode).is_none(), "Claude Code's list is read from Claude Code");
}

#[test]
fn codex_and_gemini_lists_are_the_catalog_labelled_as_such_with_what_they_cant_take_off_and_why() {
    for kind in [Kind::Codex, Kind::Gemini] {
        let note = tool_catalog::source_note(kind).unwrap();
        assert!(note.starts_with("From Gizai's catalog"), "{kind:?}: {note}");
    }
    let codex = |id: &str| tool_catalog::find(Kind::Codex, id).unwrap();
    assert_eq!(codex("web_search").how, how::WEB);
    assert_eq!(codex("web_fetch").how, how::OFF);
    assert!(codex("web_fetch").note.contains("Codex"));
    let gemini = |id: &str| tool_catalog::find(Kind::Gemini, id).unwrap();
    assert_eq!(gemini("web_fetch").how, how::WEB);
    assert_eq!(gemini("google_web_search").how, how::ALWAYS, "Gemini searches by its own policy");
    assert!(gemini("google_web_search").note.contains("policy"));
    assert_eq!(gemini("save_memory").how, how::OFF, "it would write in ~/.gemini");
    assert!(tool_catalog::catalog(Kind::Other).is_empty());
}

#[test]
fn what_a_cli_cant_take_of_the_web_switches_is_disabled_with_the_reason() {
    assert_eq!(tool_catalog::web_support(Kind::ClaudeCode), (None, None, None));
    let (search, fetch, domains) = tool_catalog::web_support(Kind::Codex);
    assert!(search.is_none() && fetch.is_some_and(|w| w.contains("Codex")) && domains.is_some());
    let (search, fetch, domains) = tool_catalog::web_support(Kind::Gemini);
    assert!(search.is_some_and(|w| w.contains("Gemini")) && fetch.is_none() && domains.is_some_and(|w| w.contains("domains")));
    let (search, fetch, domains) = tool_catalog::web_support(Kind::Other);
    assert!(search.is_some() && fetch.is_some() && domains.is_some());
}

#[test]
fn a_run_leaves_web_and_built_in_tools_with_a_switch_out_of_the_allowed_commands_and_keeps_the_rest() {
    // only the switches give these: an allowed commands entry for them is left out of a run
    for t in ["WebSearch", "WebFetch", "WebFetch(domain:evil.example)", " WebFetch (domain:docs.rs)", "FancyNewTool", "FancyNewTool(x)",
              "Skill", "SlashCommand", "AskUserQuestion", "SendMessage", "CronCreate", "EnterWorktree", "websearch", "bash(git status:*)"] {
        assert!(tool_catalog::only_by_switch(t), "{t} is left out");
    }
    // commands, the CLI's always-on tools, what the permission mode gives and MCP tools stay
    for t in ["Bash(git status:*)", "Bash(npm test:*)", "Bash", "Read", "Read(//srv/docs/**)", "Glob", "Grep", "Edit", "Edit(//srv/docs/**)",
              "Write", "NotebookEdit", "ExitPlanMode", "TodoWrite", "Task", "mcp__otus__get_card", "mcp__chrome-devtools", "bad name!"] {
        assert!(!tool_catalog::only_by_switch(t), "{t} stays");
    }
}

#[test]
fn only_a_cli_that_searches_the_web_whatever_the_switches_counts_as_on_the_web_in_every_run() {
    assert!(tool_catalog::web_in_every_run(Kind::Gemini), "google_web_search is always on");
    for kind in [Kind::ClaudeCode, Kind::Codex, Kind::Other] {
        assert!(!tool_catalog::web_in_every_run(kind), "{kind:?}: its web tools come from the switches");
    }
}

// ---- what Claude Code reports ----

#[test]
fn the_init_line_of_a_run_or_a_chat_turn_gives_its_built_in_tools_without_mcp_tools() {
    let log = |n: &str| tool_catalog::log_tools(&Path::new(FIXTURES).join(n));
    assert_eq!(log("run-ok.jsonl"), Some(vec!["Read".into(), "Edit".into(), "Bash".into()]));
    assert_eq!(log("chat-ok.jsonl"), Some(vec!["Read".into()]), "mcp__gizai__create_task is an MCP tool: left out");
    assert_eq!(log("run-mcp-init.jsonl"), Some(vec!["Read".into(), "Edit".into(), "Bash".into()]), "mcp__wiki__search left out");
    // Gizai's own note lines come before the init line in a run's log
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("r.jsonl");
    let run_ok = std::fs::read_to_string(Path::new(FIXTURES).join("run-ok.jsonl")).unwrap();
    std::fs::write(&p, format!("{}\n{}\n{run_ok}", r#"{"type":"gizai_note","text":"Left out the browser: \"init\" isn't here"}"#,
                               r#"{"type":"system","subtype":"hook_started"}"#)).unwrap();
    assert_eq!(tool_catalog::log_tools(&p), Some(vec!["Read".into(), "Edit".into(), "Bash".into()]));
    std::fs::write(&p, "{\"type\":\"result\",\"subtype\":\"error\"}\nnot json\n").unwrap();
    assert_eq!(tool_catalog::log_tools(&p), None, "no init line, nothing seen");
    assert_eq!(tool_catalog::log_tools(&tmp.path().join("missing.jsonl")), None);
    assert_eq!(tool_catalog::init_tools(&serde_json::json!({"type": "system", "subtype": "status", "tools": ["Read"]})), None);
}

#[test]
fn the_list_is_the_catalog_marked_with_what_claude_code_reported_and_an_unknown_tool_under_other_off_with_risk_unknown() {
    let reported: Vec<String> = ["Read", "WebSearch", "FancyNewTool", "mcp__chrome-devtools__click", "bad name!", "FancyNewTool"].map(String::from).to_vec();
    let catalog = tool_catalog::catalog(Kind::ClaudeCode);
    let m = tool_catalog::merged(Kind::ClaudeCode, Some(&reported));
    assert_eq!(m.len(), catalog.len() + 1, "only FancyNewTool is added, once: {:?}", m.iter().map(|t| &t.id).collect::<Vec<_>>());
    let get = |id: &str| m.iter().find(|t| t.id == id).unwrap_or_else(|| panic!("{id} missing"));
    assert!(get("Read").reported && get("WebSearch").reported);
    assert!(!get("Glob").reported, "a catalog tool the CLI didn't name stays in the list, not marked");
    let fancy = get("FancyNewTool");
    assert_eq!((fancy.group.as_str(), fancy.risk.as_str(), fancy.how.as_str(), fancy.reported), ("other", "unknown", how::SWITCH, true));
    assert!(!fancy.description.is_empty() && !fancy.description.contains('\n'));
    assert_eq!(m.last().map(|t| t.id.as_str()), Some("FancyNewTool"), "after the catalog's own");
    assert!(tool_catalog::switchable(Kind::ClaudeCode, "FancyNewTool"));
    for id in ["Read", "Skill", "Bash", "WebSearch", "mcp__chrome-devtools", "bad name!"] {
        assert!(!tool_catalog::switchable(Kind::ClaudeCode, id), "{id}");
    }
    assert!(!tool_catalog::switchable(Kind::Codex, "FancyNewTool") && !tool_catalog::switchable(Kind::Gemini, "FancyNewTool"));
    // never asked: the catalog alone, nothing marked
    assert_eq!(tool_catalog::merged(Kind::ClaudeCode, None), catalog);
    // Codex and Gemini can't be asked: nothing is added to their catalog
    assert_eq!(tool_catalog::merged(Kind::Codex, Some(&reported)).len(), tool_catalog::catalog(Kind::Codex).len());
}

// ---- Ask Claude Code again ----

/// Ends (SIGKILL) whatever of these PIDs still runs, checked by its command line, on drop.
struct Leftovers(Vec<u32>);

impl Drop for Leftovers {
    fn drop(&mut self) {
        for &pid in &self.0 {
            let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).map(|b| String::from_utf8_lossy(&b).replace('\0', " ")).unwrap_or_default();
            if pid > 1 && !ended(pid) && (cmd.contains("sleep") || cmd.contains("fake-claude-tools")) {
                // SAFETY: a plain kill of a process this test's fake started (checked by its command line just above).
                unsafe { libc::kill(pid as i32, libc::SIGKILL); }
            }
        }
    }
}

fn ended(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(s) => s.rfind(')').and_then(|i| s[i + 1..].split_whitespace().next().map(|st| st == "Z" || st == "X")).unwrap_or(true),
    }
}

fn pid_in(p: &Path) -> u32 {
    std::fs::read_to_string(p).unwrap_or_default().trim().parse().unwrap_or(0)
}

#[tokio::test]
async fn ask_claude_reads_the_tools_from_a_start_without_a_login_in_a_scratch_config_and_home() {
    let tmp = tempfile::tempdir().unwrap();
    let scratch = tmp.path().join("scratch");
    // SAFETY: only this test reads it; it must not reach the fake (ask_claude starts it with an empty environment).
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-GA55-NOT-FOR-THE-FAKE") };
    let tools = tool_catalog::ask_claude(&executable(FAKE_TOOLS), &scratch, OsStr::new("/usr/bin:/bin")).await.unwrap();
    assert_eq!(tools, ["Task", "Bash", "Read", "WebSearch", "WebFetch", "FancyNewTool"], "its built-in tools, MCP tools left out");
    let seen = scratch.join("home/seen");
    let env = std::fs::read_to_string(seen.join("env")).unwrap();
    let line = |k: &str| env.lines().find_map(|l| l.strip_prefix(&format!("{k}="))).map(str::to_string);
    assert_eq!(line("HOME"), Some(scratch.join("home").display().to_string()));
    assert_eq!(line("CLAUDE_CONFIG_DIR"), Some(scratch.join("config").display().to_string()), "never ~/.claude or the agents' account");
    assert_eq!(line("PATH").as_deref(), Some("/usr/bin:/bin"));
    for k in ["ANTHROPIC_API_KEY", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_AUTH_TOKEN", "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT"] {
        assert_eq!(line(k), None, "{k} reached the fake: it could log in and spend");
    }
    let argv: Vec<String> = std::fs::read_to_string(seen.join("argv")).unwrap().lines().map(str::to_string).collect();
    for want in ["-p", "--no-session-persistence", "--strict-mcp-config", "--disable-slash-commands"] {
        assert!(argv.iter().any(|a| a == want), "{want} missing: {argv:?}");
    }
    no_chrome(&argv);
    assert!(!argv.iter().any(|a| a.contains("dangerously") || a == "bypassPermissions"), "{argv:?}");
    // the fake wrote only under the scratch folder it was given
    assert!(seen.join("prompt").is_file());
}

#[tokio::test]
async fn ask_claude_ends_a_claude_code_that_keeps_running_after_its_init_line_with_its_process_group() {
    let tmp = tempfile::tempdir().unwrap();
    let scratch = tmp.path().join("scratch");
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    std::fs::write(scratch.join("home/mode"), "hang").unwrap();
    let t0 = Instant::now();
    let tools = tool_catalog::ask_claude(&executable(FAKE_TOOLS), &scratch, OsStr::new("/usr/bin:/bin")).await.unwrap();
    assert!(tools.contains(&"WebFetch".to_string()), "{tools:?}");
    assert!(t0.elapsed() < Duration::from_secs(10), "it doesn't wait for Claude Code to exit by itself: {:?}", t0.elapsed());
    let seen = scratch.join("home/seen");
    let (claude, child) = (pid_in(&seen.join("pid")), pid_in(&seen.join("child.pid")));
    let _left = Leftovers(vec![claude, child]);
    assert!(claude > 1 && child > 1, "the fake wrote its PIDs");
    let t0 = Instant::now();
    while !(ended(claude) && ended(child)) && t0.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ended(claude) && ended(child), "still running: claude {} child {}", !ended(claude), !ended(child));
}

#[tokio::test]
async fn ask_claude_without_an_init_line_says_it_gave_no_list() {
    let tmp = tempfile::tempdir().unwrap();
    let scratch = tmp.path().join("scratch");
    std::fs::create_dir_all(scratch.join("home")).unwrap();
    std::fs::write(scratch.join("home/mode"), "none").unwrap();
    let e = tool_catalog::ask_claude(&executable(FAKE_TOOLS), &scratch, OsStr::new("/usr/bin:/bin")).await.unwrap_err();
    assert!(e.contains("no list of its tools"), "{e}");
    let e = tool_catalog::ask_claude(Path::new("/nonexistent/claude"), &scratch, OsStr::new("/usr/bin:/bin")).await.unwrap_err();
    assert!(e.contains("couldn't start"), "{e}");
}
