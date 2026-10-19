// Settings → Coding CLIs (GA-3): Claude Code plus the CLIs you add (Codex, Gemini, other programs, more accounts), and
// which permission modes and effort levels an agent on each kind may have.
use gizai_core::clis::{self, Cli};
use gizai_core::model::AgentInput;
use gizai_core::{db::Db, seed::ensure_seed, settings, team};

fn setup() -> (Db, gizai_core::seed::SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    (db, s)
}

fn cli(name: &str, kind: &str, command: &str) -> Cli {
    Cli { name: name.into(), kind: kind.into(), command: command.into(), ..Default::default() }
}

/// Saves these CLIs and returns their ids by name.
fn add(db: &Db, list: Vec<Cli>) -> std::collections::HashMap<String, String> {
    clis::save(db, list).unwrap().into_iter().map(|c| (c.name, c.id)).collect()
}

fn agent(name: &str, adapter: &str) -> AgentInput {
    AgentInput { name: name.into(), role_key: "backend".into(), adapter: adapter.into(), ..Default::default() }
}

#[test]
fn claude_code_is_built_in_and_its_program_is_the_claude_bin_setting() {
    let (db, _) = setup();
    let all = clis::list(&db).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].id.as_str(), all[0].name.as_str(), all[0].kind.as_str(), all[0].command.as_str()), ("claude_code", "Claude Code", "claude_code", ""));
    settings::set(&db, "claude_bin", &"/opt/claude".to_string()).unwrap();
    assert_eq!(clis::get(&db, "").unwrap().command, "/opt/claude", "empty means the built-in Claude Code");
    assert_eq!(clis::get(&db, " claude_code ").unwrap().command, "/opt/claude");
    let e = clis::get(&db, "nope").unwrap_err().to_string();
    assert!(e.contains("no coding CLI with id nope"), "{e}");
}

#[test]
fn added_clis_get_ids_are_trimmed_and_come_after_claude_code() {
    let (db, _) = setup();
    let saved = clis::save(&db, vec![
        Cli { name: " Codex ".into(), kind: "codex".into(), command: " codex ".into(), env: vec!["".into(), " CODEX_HOME=~/.codex-2 ".into()], args: "ignored".into(), ..Default::default() },
        Cli { name: "Claude Code (2nd account)".into(), kind: "claude_code".into(), command: "claude".into(), env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..Default::default() },
        Cli { name: "OpenCode".into(), kind: "other".into(), command: "opencode".into(), args: " run -m {model} {prompt} ".into(), ..Default::default() },
    ]).unwrap();
    assert_eq!(saved.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Claude Code", "Codex", "Claude Code (2nd account)", "OpenCode"]);
    let codex = &saved[1];
    assert!(!codex.id.is_empty() && codex.id != "claude_code");
    assert_eq!((codex.command.as_str(), codex.env.clone(), codex.args.as_str()), ("codex", vec!["CODEX_HOME=~/.codex-2".to_string()], ""),
               "only Other keeps arguments");
    assert_eq!(saved[3].args, "run -m {model} {prompt}");
    assert_eq!(clis::list(&db).unwrap(), saved, "kept in the settings");
    assert_eq!(clis::get(&db, &saved[2].id).unwrap().env, ["CLAUDE_CONFIG_DIR=~/.claude-2"]);

    // saving again keeps the ids; the built-in Claude Code in the list is left out (its program is a setting)
    let again = clis::save(&db, saved.clone()).unwrap();
    assert_eq!(again, saved);
    assert_eq!(settings::get::<String>(&db, "claude_bin").unwrap(), None);
}

#[test]
fn bad_clis_are_refused_with_the_reason() {
    let (db, _) = setup();
    let bad = [
        (cli("", "codex", "codex"), "give the CLI a name"),
        (cli(&"x".repeat(61), "codex", "codex"), "at most 60"),
        (cli("Cursor", "cursor", "cursor-agent"), "Claude Code, Codex, Gemini or Other, not cursor"),
        (cli("Codex", "codex", "  "), "give Codex its program"),
        (Cli { env: vec!["no equals sign".into()], ..cli("Codex", "codex", "codex") }, "NAME=value lines, not no equals sign"),
        (Cli { env: vec!["1BAD=x".into()], ..cli("Codex", "codex", "codex") }, "NAME=value"),
        (Cli { env: vec!["=x".into()], ..cli("Codex", "codex", "codex") }, "NAME=value"),
        (cli("claude code", "claude_code", "claude"), "already a CLI called claude code"),
    ];
    for (c, want) in bad {
        let e = clis::save(&db, vec![c.clone()]).unwrap_err().to_string();
        assert!(e.contains(want), "{c:?}: {e}");
    }
    let e = clis::save(&db, vec![cli("Codex", "codex", "codex"), cli("CODEX", "codex", "codex")]).unwrap_err().to_string();
    assert!(e.contains("already a CLI called CODEX"), "{e}");
    let e = clis::save(&db, vec![Cli { id: "x1".into(), ..cli("A", "codex", "codex") }, Cli { id: "x1".into(), ..cli("B", "gemini", "gemini") }]).unwrap_err().to_string();
    assert!(e.contains("two CLIs have the id x1"), "{e}");
    assert_eq!(clis::list(&db).unwrap().len(), 1, "nothing saved");
}

#[test]
fn a_cli_that_agents_run_on_cannot_be_removed() {
    let (db, s) = setup();
    let ids = add(&db, vec![cli("Codex", "codex", "codex"), cli("Gemini", "gemini", "gemini")]);
    let a = team::add_agent(&db, &s.you_id, &s.team_id, agent("Backend Agent", &ids["Codex"])).unwrap();
    let qa = team::add_agent(&db, &s.you_id, &s.team_id, agent("QA Agent", &ids["Codex"])).unwrap();
    let keep_gemini = vec![Cli { id: ids["Gemini"].clone(), ..cli("Gemini", "gemini", "gemini") }];
    let e = clis::save(&db, keep_gemini.clone()).unwrap_err().to_string();
    assert!(e.contains("Backend Agent, QA Agent runs on Codex"), "{e}");
    assert_eq!(clis::list(&db).unwrap().len(), 3);
    // once its agents run on something else, it can go
    team::update_agent(&db, &s.you_id, &a, agent("Backend Agent", "claude_code")).unwrap();
    let e = clis::save(&db, keep_gemini.clone()).unwrap_err().to_string();
    assert!(e.contains("QA Agent runs on Codex"), "{e}");
    team::update_agent(&db, &s.you_id, &qa, agent("QA Agent", &ids["Gemini"])).unwrap();
    let left = clis::save(&db, keep_gemini).unwrap();
    assert_eq!(left.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Claude Code", "Gemini"]);
}

#[test]
fn each_kind_has_its_own_permission_modes_and_efforts() {
    assert_eq!(clis::permission_modes("claude_code")[0], "acceptEdits");
    assert_eq!(clis::permission_modes("codex"), ["workspace-write", "read-only", "danger-full-access"]);
    assert_eq!(clis::permission_modes("gemini"), ["auto_edit", "yolo", "plan", "default"]);
    assert!(clis::permission_modes("other").is_empty());
    assert_eq!(clis::efforts("claude_code"), ["low", "medium", "high", "xhigh", "max"]);
    assert_eq!(clis::efforts("codex"), ["minimal", "low", "medium", "high", "xhigh"]);
    assert!(clis::efforts("gemini").is_empty() && clis::efforts("other").is_empty());
}

#[test]
fn agents_run_on_any_cli_with_that_clis_modes_and_efforts() {
    let (db, s) = setup();
    let ids = add(&db, vec![cli("Codex", "codex", "codex"), cli("Gemini", "gemini", "gemini"), cli("Crush", "other", "crush"),
                            Cli { env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..cli("Claude 2", "claude_code", "claude") }]);
    let add_agent = |i: AgentInput| team::add_agent(&db, &s.you_id, &s.team_id, i);

    let codex = add_agent(agent("Codex Agent", &ids["Codex"])).unwrap();
    let m = team::agent(&db, &codex).unwrap();
    assert_eq!((m.adapter.as_deref(), m.permission_mode.as_deref()), (Some(ids["Codex"].as_str()), Some("workspace-write")), "the CLI's first mode is the default");
    let gemini = add_agent(agent("Gemini Agent", &ids["Gemini"])).unwrap();
    assert_eq!(team::agent(&db, &gemini).unwrap().permission_mode.as_deref(), Some("auto_edit"));
    let other = add_agent(AgentInput { permission_mode: "acceptEdits".into(), ..agent("Crush Agent", &ids["Crush"]) }).unwrap();
    assert_eq!(team::agent(&db, &other).unwrap().permission_mode.as_deref(), Some(""), "an Other CLI has no modes");
    let second = add_agent(AgentInput { effort: Some("max".into()), ..agent("Claude 2 Agent", &ids["Claude 2"]) }).unwrap();
    let m = team::agent(&db, &second).unwrap();
    assert_eq!((m.permission_mode.as_deref(), m.effort.as_deref()), (Some("acceptEdits"), Some("max")), "a second Claude Code account is Claude Code");
    let builtin = add_agent(agent("Plain Agent", "")).unwrap();
    assert_eq!(team::agent(&db, &builtin).unwrap().adapter.as_deref(), Some("claude_code"), "no CLI: Claude Code");

    let refused = [
        (AgentInput { permission_mode: "acceptEdits".into(), ..agent("A", &ids["Codex"]) }, "unknown permission mode acceptEdits for Codex: use workspace-write, read-only, danger-full-access"),
        (AgentInput { permission_mode: "workspace-write".into(), ..agent("B", "claude_code") }, "unknown permission mode workspace-write for Claude Code"),
        (AgentInput { effort: Some("max".into()), ..agent("C", &ids["Codex"]) }, "effort is minimal, low, medium, high, xhigh, not max"),
        (AgentInput { effort: Some("high".into()), ..agent("D", &ids["Gemini"]) }, "Gemini takes no effort level"),
        (AgentInput { effort: Some("high".into()), ..agent("E", &ids["Crush"]) }, "Crush takes no effort level"),
        (agent("F", "gone"), "no coding CLI with id gone"),
    ];
    for (i, want) in refused {
        let e = add_agent(i).unwrap_err().to_string();
        assert!(e.contains(want), "{want}: {e}");
    }
    let ok = add_agent(AgentInput { effort: Some("Minimal".into()), ..agent("G", &ids["Codex"]) }).unwrap();
    assert_eq!(team::agent(&db, &ok).unwrap().effort.as_deref(), Some("minimal"));
}

#[test]
fn moving_an_agent_to_another_cli_is_saved() {
    let (db, s) = setup();
    let ids = add(&db, vec![cli("Codex", "codex", "codex")]);
    let id = team::add_agent(&db, &s.you_id, &s.team_id, agent("Backend Agent", "")).unwrap();
    team::update_agent(&db, &s.you_id, &id, AgentInput { permission_mode: "read-only".into(), effort: Some("xhigh".into()), ..agent("Backend Agent", &ids["Codex"]) }).unwrap();
    let m = team::agent(&db, &id).unwrap();
    assert_eq!((m.adapter.as_deref(), m.permission_mode.as_deref(), m.effort.as_deref()), (Some(ids["Codex"].as_str()), Some("read-only"), Some("xhigh")));
    team::update_agent(&db, &s.you_id, &id, agent("Backend Agent", "claude_code")).unwrap();
    let m = team::agent(&db, &id).unwrap();
    assert_eq!((m.adapter.as_deref(), m.permission_mode.as_deref()), (Some("claude_code"), Some("acceptEdits")));
}

#[test]
fn chat_stays_on_claude_code() {
    let (db, s) = setup();
    let ids = add(&db, vec![cli("Codex", "codex", "codex"), Cli { env: vec!["CLAUDE_CONFIG_DIR=/x".into()], ..cli("Claude 2", "claude_code", "claude") }]);
    let e = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { chat_enabled: Some(true), ..agent("Lead", &ids["Codex"]) }).unwrap_err().to_string();
    assert!(e.contains("Chat runs on Claude Code"), "{e}");
    // a second Claude Code account can be the Team Lead
    let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { chat_enabled: Some(true), role_key: "lead".into(), ..agent("Team Lead", &ids["Claude 2"]) }).unwrap();
    assert!(team::agent(&db, &lead).unwrap().chat_enabled);
    // moving the Team Lead to Codex while its Chat is on is refused; turning Chat off in the same save is fine
    let e = team::update_agent(&db, &s.you_id, &lead, AgentInput { role_key: "lead".into(), ..agent("Team Lead", &ids["Codex"]) }).unwrap_err().to_string();
    assert!(e.contains("turn Chat off"), "{e}");
    assert_eq!(team::agent(&db, &lead).unwrap().adapter.as_deref(), Some(ids["Claude 2"].as_str()), "unchanged");
    team::update_agent(&db, &s.you_id, &lead, AgentInput { role_key: "lead".into(), chat_enabled: Some(false), ..agent("Team Lead", &ids["Codex"]) }).unwrap();
    let m = team::agent(&db, &lead).unwrap();
    assert_eq!((m.adapter.as_deref(), m.chat_enabled), (Some(ids["Codex"].as_str()), false));
}

#[test]
fn environment_lines_expand_the_home_folder() {
    assert_eq!(clis::expand_home("~", "/home/u"), "/home/u");
    assert_eq!(clis::expand_home("~/.claude-2", "/home/u"), "/home/u/.claude-2");
    assert_eq!(clis::expand_home("$HOME/a:${HOME}/b", "/home/u"), "/home/u/a:/home/u/b");
    assert_eq!(clis::expand_home("/abs/~x", "/home/u"), "/abs/~x", "only a leading ~");
    let c = Cli { env: vec!["CLAUDE_CONFIG_DIR = \"~/.claude-2\"".into(), "A='x=y'".into(), "B=".into()], ..cli("C", "claude_code", "claude") };
    assert_eq!(clis::env_pairs(&c, "/home/u"), [("CLAUDE_CONFIG_DIR".to_string(), "/home/u/.claude-2".to_string()),
        ("A".to_string(), "x=y".to_string()), ("B".to_string(), String::new())]);
}
