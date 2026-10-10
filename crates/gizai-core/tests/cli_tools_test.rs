// GA-55: where an agent's own CLI tools are kept (agent form → Tools → Web, Browser, Built-in tools): next to its MCP
// switches in mcp_extra_json, each leaving the other alone, off until switched on; the built-in browser's Settings entry
// (pinned, only an exact version); and the tools Claude Code reported, per agent (last run) and per CLI (when asked).
use gizai_core::db::Db;
use gizai_core::mcp_servers::{self as m, AgentServer, AgentTools, BrowserEntry, CliTools, McpServer};
use gizai_core::model::AgentInput;
use gizai_core::{seed, team};

fn setup() -> (Db, String, String, String) {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let id = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    (db, s.you_id, s.team_id, id)
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn everything_is_off_for_a_new_agent_and_when_the_json_is_unreadable() {
    let (db, _, _, id) = setup();
    let none = CliTools::default();
    assert!(!none.web_search && !none.web_fetch && !none.insecure_certs && none.fetch_domains.is_empty() && none.builtin.is_empty() && !none.web_on());
    assert_eq!(m::agent_cli_tools(&db, &id).unwrap(), none);
    assert_eq!(team::agent(&db, &id).unwrap().cli_tools, none);
    assert!(!m::agent_tools(&db, &id).unwrap().browser_on(), "the browser is off too");
    for raw in [None, Some(""), Some("not json"), Some(r#"{"mcp":[]}"#), Some(r#"{"cli":5}"#), Some(r#"[1,2]"#)] {
        assert_eq!(m::parse_cli_tools(raw), none, "{raw:?}");
    }
    assert!(m::agent_cli_tools(&db, "no-such-agent").is_err());
}

#[test]
fn the_web_and_built_in_switches_are_saved_cleaned_next_to_the_mcp_switches_and_each_leaves_the_other_alone() {
    let (db, you, _, id) = setup();
    let otus = m::save(&db, McpServer { name: "otus".into(), transport: "stdio".into(), command: "/opt/otus/otus-mcp".into(), ..Default::default() }).unwrap();
    let mcp = vec![AgentServer { server_id: otus.id.clone(), on: true, tools_off: strs(&["delete_note"]) },
                   AgentServer { server_id: m::BROWSER.into(), on: true, tools_off: vec![] }];
    m::set_agent_tools(&db, &you, &id, AgentTools { mcp: mcp.clone() }).unwrap();

    let saved = m::set_cli_tools(&db, &you, &id, CliTools {
        web_search: true, web_fetch: true, insecure_certs: true,
        fetch_domains: strs(&[" https://Docs.rs/ ", "docs.rs", "*.laravel.com", "", "http://kade.test"]),
        builtin: strs(&["FancyNewTool", " FancyNewTool ", "AnotherTool", ""]),
    }).unwrap();
    assert_eq!(saved.fetch_domains, ["docs.rs", "*.laravel.com", "kade.test"], "lower case, no scheme or slash, each once");
    assert_eq!(saved.builtin, ["AnotherTool", "FancyNewTool"]);
    assert!(saved.web_on());
    assert_eq!(m::agent_cli_tools(&db, &id).unwrap(), saved);
    assert_eq!(team::agent(&db, &id).unwrap().cli_tools, saved);
    let tools = m::agent_tools(&db, &id).unwrap();
    assert_eq!(tools.mcp, mcp, "the MCP switches are as they were");
    assert!(tools.browser_on());

    // the MCP switches saved after it leave the CLI tools alone
    m::set_agent_tools(&db, &you, &id, AgentTools { mcp: vec![] }).unwrap();
    assert_eq!(m::agent_cli_tools(&db, &id).unwrap(), saved);
    assert!(!m::agent_tools(&db, &id).unwrap().browser_on());
    // and everything off again
    assert_eq!(m::set_cli_tools(&db, &you, &id, CliTools::default()).unwrap(), CliTools::default());
    assert_eq!(team::agent(&db, &id).unwrap().cli_tools, CliTools::default());
}

#[test]
fn a_domain_or_tool_name_that_cant_be_used_is_refused_and_nothing_changes() {
    let (db, you, _, id) = setup();
    let before = m::set_cli_tools(&db, &you, &id, CliTools { web_fetch: true, fetch_domains: strs(&["docs.rs"]), ..Default::default() }).unwrap();
    for bad in ["docs.rs/api", "localhost", "docs rs", "docs.rs:8080", "*.", ".docs.rs", "docs.rs.", "https://", "user@docs.rs", "docs.rs?x=1"] {
        let e = m::set_cli_tools(&db, &you, &id, CliTools { web_fetch: true, fetch_domains: strs(&[bad]), ..Default::default() }).unwrap_err().to_string();
        assert!(e.contains("isn't a domain"), "{bad}: {e}");
    }
    for bad in ["mcp__chrome-devtools", "mcp__otus__search", "Bash(npm:*)", "WebFetch(domain:docs.rs)", "bad name", &"x".repeat(65)] {
        let e = m::set_cli_tools(&db, &you, &id, CliTools { builtin: strs(&[bad]), ..Default::default() }).unwrap_err().to_string();
        assert!(e.contains("isn't the name"), "{bad}: {e}");
    }
    assert_eq!(m::agent_cli_tools(&db, &id).unwrap(), before, "nothing changed");
    assert!(m::set_cli_tools(&db, &you, "no-such-agent", CliTools::default()).is_err());
}

#[test]
fn the_browser_is_a_switch_of_its_own_and_no_mcp_server_can_take_its_name() {
    let (db, you, _, id) = setup();
    assert!(m::TAKEN.contains(&m::BROWSER));
    assert!(m::save(&db, McpServer { name: "chrome-devtools".into(), transport: "stdio".into(), command: "/opt/x".into(), ..Default::default() }).is_err());
    let kept = m::set_agent_tools(&db, &you, &id, AgentTools { mcp: vec![
        AgentServer { server_id: m::BROWSER.into(), on: true, tools_off: strs(&["evaluate_script", " "]) },
        AgentServer { server_id: "no-such-server".into(), on: true, tools_off: vec![] },
    ] }).unwrap();
    assert_eq!(kept.mcp, [AgentServer { server_id: m::BROWSER.into(), on: true, tools_off: strs(&["evaluate_script"]) }], "the browser is known; a server not in the list is dropped");
}

#[test]
fn an_agent_on_codex_or_gemini_cant_have_the_browser_on_and_says_why() {
    let (db, you, team_id, _) = setup();
    for kind in ["codex", "gemini"] {
        let mut clis = gizai_core::clis::list(&db).unwrap();
        clis.push(gizai_core::clis::Cli { name: format!("My {kind}"), kind: kind.into(), command: kind.into(), ..Default::default() });
        let cli = gizai_core::clis::save(&db, clis).unwrap().into_iter().find(|c| c.kind == kind).unwrap();
        let id = team::add_agent(&db, &you, &team_id, AgentInput { name: format!("{kind} agent"), role_key: "frontend".into(), adapter: cli.id.clone(), ..Default::default() }).unwrap();
        let e = m::set_agent_tools(&db, &you, &id, AgentTools { mcp: vec![AgentServer { server_id: m::BROWSER.into(), on: true, tools_off: vec![] }] }).unwrap_err().to_string();
        assert!(e.contains("browser") && e.contains(&cli.name), "{kind}: {e}");
        assert!(!m::agent_tools(&db, &id).unwrap().browser_on());
        let why = m::mcp_not_on(kind).unwrap();
        assert!(why.contains("the browser"), "{kind}: {why}");
    }
    assert_eq!(m::mcp_not_on("claude_code"), None);
    assert!(m::mcp_not_on("other").is_some());
}

#[test]
fn the_built_in_browser_starts_pinned_and_takes_only_an_exact_version() {
    let (db, ..) = setup();
    let b = m::browser(&db).unwrap();
    assert_eq!(b, BrowserEntry { version: m::BROWSER_VERSION.into(), program: String::new() });
    assert_eq!(m::BROWSER_VERSION, "1.10.1");
    assert!(m::exact_version(&b.version));
    for bad in ["latest", "^1.10.1", "~1.10.1", "1.10", "", "next", "1.x", ">=1.0.0"] {
        let e = m::set_browser(&db, BrowserEntry { version: bad.into(), program: String::new() }).unwrap_err().to_string();
        assert!(e.contains("exact version"), "{bad}: {e}");
    }
    assert_eq!(m::browser(&db).unwrap(), b, "nothing changed");
    let saved = m::set_browser(&db, BrowserEntry { version: " v1.11.0 ".into(), program: " /usr/bin/chromium ".into() }).unwrap();
    assert_eq!(saved, BrowserEntry { version: "1.11.0".into(), program: "/usr/bin/chromium".into() });
    assert_eq!(m::browser(&db).unwrap(), saved);
}

#[test]
fn the_tools_claude_code_reported_are_kept_per_agent_and_per_cli_without_mcp_tools() {
    let (db, you, team_id, id) = setup();
    let other = team::add_agent(&db, &you, &team_id, AgentInput { name: "Frontend Agent".into(), role_key: "frontend".into(), ..Default::default() }).unwrap();
    assert!(m::seen_tools(&db).unwrap().is_empty() && m::asked_tools(&db).unwrap().is_empty());
    m::set_seen_tools(&db, &id, "claude-code", &strs(&["Read", "WebSearch", "mcp__chrome-devtools__click", "Read", "FancyNewTool", "bad name"])).unwrap();
    m::set_seen_tools(&db, &other, "claude-code", &strs(&["Bash"])).unwrap();
    let seen = m::seen_tools(&db).unwrap();
    let mine = &seen[&id];
    assert_eq!((mine.cli_id.as_str(), mine.tools.clone()), ("claude-code", strs(&["Read", "WebSearch", "FancyNewTool"])));
    assert!(mine.at > 0);
    assert_eq!(seen[&other].tools, ["Bash"], "kept apart per agent");
    // the next run replaces it
    m::set_seen_tools(&db, &id, "claude-code-2", &strs(&["Read"])).unwrap();
    let seen = m::seen_tools(&db).unwrap();
    assert_eq!((seen[&id].cli_id.as_str(), seen[&id].tools.clone()), ("claude-code-2", strs(&["Read"])));
    assert_eq!(seen[&other].tools, ["Bash"]);
    // asked per CLI, apart from the runs
    m::set_asked_tools(&db, "claude-code", &strs(&["Task", "mcp__gizai__get_overview", "WebFetch"])).unwrap();
    assert_eq!(m::asked_tools(&db).unwrap()["claude-code"].tools, ["Task", "WebFetch"]);
    assert_eq!(m::seen_tools(&db).unwrap().len(), 2);
}
