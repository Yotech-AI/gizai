use gizai_agents::{claude::ClaudeArgs, outcome, stream::{parse_line, RunEvent}};

fn args() -> ClaudeArgs {
    ClaudeArgs { bin: "/x/claude".into(), prompt: "hi".into(), session_id: "S".into(), permission_mode: "acceptEdits".into(),
                 allowed_tools: vec!["Bash(npm test:*)".into()], ..Default::default() }
}

#[test]
fn parses_a_normal_run() {
    let evs: Vec<RunEvent> = include_str!("fixtures/run-ok.jsonl").lines().flat_map(parse_line).collect();
    assert!(matches!(&evs[0], RunEvent::Init { session_id, .. } if session_id == "S1"));
    assert!(evs.iter().any(|e| matches!(e, RunEvent::ToolUse { name, summary } if name == "Read" && summary == "app/Exports/InvoiceExporter.php")));
    assert!(evs.iter().any(|e| matches!(e, RunEvent::ToolUse { name, summary } if name == "Bash" && summary.contains("artisan test"))));
    assert!(evs.iter().any(|e| matches!(e, RunEvent::ToolResult { is_error: false, preview } if preview.starts_with("<?php"))));
    let last = evs.last().unwrap();
    match last { RunEvent::Result { cost_usd, input_tokens, num_turns, is_error, text, .. } => {
        assert_eq!((*cost_usd, *input_tokens, *num_turns, *is_error), (Some(0.42), 38000, 7, false));
        let o = outcome::parse(text).unwrap();
        assert_eq!(o.outcome, "ready_for_testing");
    } other => panic!("{other:?}") }
}

#[test]
fn survives_unknown_and_broken_lines() {
    let evs: Vec<RunEvent> = include_str!("fixtures/run-weird.jsonl").lines().flat_map(parse_line).collect();
    assert_eq!(evs.len(), 3);
    assert!(evs.iter().all(|e| matches!(e, RunEvent::Other { .. })));
    assert!(matches!(&evs[2], RunEvent::Other { raw_type } if raw_type == "thinking"));
}

#[test]
fn outcome_rejects_unknown_values_and_takes_the_last_line() {
    assert!(outcome::parse("GIZAI_RESULT: {\"outcome\":\"merged\",\"summary\":\"x\"}").is_none());
    let t = "GIZAI_RESULT: {\"outcome\":\"qa_fail\",\"summary\":\"a\",\"issues\":[\"1. pin overlaps\"]}\nmore\nGIZAI_RESULT: {\"outcome\":\"qa_pass\",\"summary\":\"b\"}";
    assert_eq!(outcome::parse(t).unwrap().outcome, "qa_pass");
    assert!(outcome::parse("GIZAI_RESULT: not json").is_none());
    assert!(outcome::parse("no result line").is_none());
}

#[test]
fn argv_contains_the_safety_flags() {
    let a = args().argv();
    for f in ["-p", "--output-format", "stream-json", "--verbose", "--session-id", "--strict-mcp-config", "--setting-sources"] {
        assert!(a.contains(&f.to_string()), "missing {f}");
    }
    let pos = a.iter().position(|x| x == "--allowedTools").unwrap();
    assert_eq!(a[pos + 1], "Bash(npm test:*)");
    assert!(!a.contains(&"--max-budget-usd".to_string()));
    // the prompt goes in on stdin (argv is capped at 128 KiB per argument on Linux)
    assert!(!a.contains(&"hi".to_string()) && !a.contains(&"--".to_string()));
}

#[test]
fn argv_adds_budget_model_and_system_prompt_when_set() {
    let a = ClaudeArgs { max_budget_usd: Some(2.5), model: Some("sonnet".into()), append_system_prompt: Some("Be brief.".into()), allowed_tools: vec![], ..args() }.argv();
    let after = |f: &str| a[a.iter().position(|x| x == f).unwrap() + 1].clone();
    assert_eq!(after("--max-budget-usd"), "2.50");
    assert_eq!(after("--model"), "sonnet");
    assert_eq!(after("--append-system-prompt"), "Be brief.");
    assert!(!a.contains(&"--allowedTools".to_string()));
}

#[test]
fn argv_for_a_task_run_is_unchanged() {
    let a = args().argv();
    let after = |f: &str| a[a.iter().position(|x| x == f).unwrap() + 1].clone();
    assert_eq!(after("--session-id"), "S");
    assert_eq!(after("--setting-sources"), "user");
    for f in ["--resume", "--restricted", "--include-partial-messages", "--mcp-config", "--tools", "--permission-prompts", "--add-dir", "--no-session-persistence"] {
        assert!(!a.contains(&f.to_string()), "unexpected {f}");
    }
}

#[test]
fn argv_for_a_chat_turn() {
    let a = ClaudeArgs {
        bin: "/x/claude".into(), prompt: "hi".into(), session_id: "S".into(), permission_mode: "manual".into(),
        allowed_tools: vec!["mcp__gizai".into()], resume: true, mcp_config: Some("/d/chat/r.mcp.json".into()), partial_messages: true,
        restricted: true, tools: Some(vec!["Read".into(), "Glob".into(), "Grep".into()]), permission_prompts_none: true,
        add_dirs: vec!["/a".into(), "/b".into()], no_session_persistence: true, ..Default::default()
    }.argv();
    let after = |f: &str| a[a.iter().position(|x| x == f).unwrap_or_else(|| panic!("missing {f}")) + 1].clone();
    assert_eq!(after("--resume"), "S");
    assert!(!a.contains(&"--session-id".to_string()));
    assert!(a.contains(&"--include-partial-messages".to_string()));
    assert!(a.contains(&"--restricted".to_string()));
    assert!(!a.contains(&"--setting-sources".to_string()));
    assert!(a.contains(&"--strict-mcp-config".to_string()));
    assert_eq!(after("--mcp-config"), "/d/chat/r.mcp.json");
    assert_eq!(after("--tools"), "Read,Glob,Grep");
    assert_eq!(after("--permission-prompts"), "none");
    assert_eq!(after("--permission-mode"), "manual");
    let dirs = a.iter().position(|x| x == "--add-dir").unwrap();
    assert_eq!(&a[dirs + 1..dirs + 3], ["/a", "/b"]);
    assert!(a.contains(&"--no-session-persistence".to_string()));
    // variadic options must not swallow each other: the allow-list comes last
    assert_eq!(&a[a.len() - 2..], ["--allowedTools", "mcp__gizai"]);
}

#[test]
fn argv_can_switch_off_hooks_and_skills() {
    let a = ClaudeArgs { disable_hooks: true, disable_skills: true, ..args() }.argv();
    let after = |f: &str| a[a.iter().position(|x| x == f).unwrap_or_else(|| panic!("missing {f}")) + 1].clone();
    assert_eq!(after("--settings"), r#"{"disableAllHooks":true}"#);
    assert!(a.contains(&"--disable-slash-commands".to_string()));
    assert!(!args().argv().contains(&"--settings".to_string()));
}

#[test]
fn argv_passes_the_effort_level() {
    let a = ClaudeArgs { effort: Some("xhigh".into()), ..args() }.argv();
    assert_eq!(a[a.iter().position(|x| x == "--effort").unwrap() + 1], "xhigh");
    assert!(!args().argv().contains(&"--effort".to_string()));
}

// GA-31, "Run this for me": a needs_decision result line may name the commands the agent asks the user to run.
#[test]
fn a_result_line_may_ask_the_user_to_run_commands_and_older_lines_still_parse() {
    // the fixture the fake Claude Code ends with for FAKE_RUN_FOR_ME: each command exactly as written, quotes and all
    let evs: Vec<RunEvent> = include_str!("fixtures/run-for-me.jsonl").lines().flat_map(parse_line).collect();
    let RunEvent::Result { text, .. } = evs.last().unwrap() else { panic!("{evs:?}") };
    let o = outcome::parse(text).unwrap();
    assert_eq!((o.outcome.as_str(), o.issues.len()), ("needs_decision", 0));
    assert_eq!(o.run_for_me, ["sudo pacman -S libayatana-appindicator",
        "echo \"fs.inotify.max_user_watches=524288\" | sudo tee /etc/sysctl.d/40-watches.conf && sudo sysctl --system"]);
    // older result lines have none, and parse as before
    for line in ["GIZAI_RESULT: {\"outcome\":\"needs_decision\",\"summary\":\"CSV or JSON?\",\"issues\":[\"Which format?\"]}",
                 "GIZAI_RESULT: {\"outcome\":\"qa_pass\",\"summary\":\"ok\"}", "GIZAI_RESULT: {\"outcome\":\"ready_for_testing\"}"] {
        let o = outcome::parse(line).unwrap_or_else(|| panic!("{line}"));
        assert!(o.run_for_me.is_empty(), "{line}");
    }
    let RunEvent::Result { text, .. } = include_str!("fixtures/run-asks.jsonl").lines().flat_map(parse_line).last().unwrap() else { panic!() };
    let o = outcome::parse(&text).unwrap();
    assert_eq!((o.outcome.as_str(), o.summary.as_str(), o.run_for_me.len()), ("needs_decision", "CSV or JSON for the export?", 0));
}

#[test]
fn run_for_me_takes_one_command_as_a_string_drops_blank_entries_and_never_spoils_the_line() {
    let p = |rfm: &str| outcome::parse(&format!("GIZAI_RESULT: {{\"outcome\":\"needs_decision\",\"summary\":\"s\",\"run_for_me\":{rfm}}}"))
        .unwrap_or_else(|| panic!("run_for_me {rfm} spoiled the line"));
    assert_eq!(p("\"sudo pacman -S libayatana-appindicator\"").run_for_me, ["sudo pacman -S libayatana-appindicator"]);
    assert_eq!(p("[\"  sudo make install  \", \"\", \"   \", \"npm i -g pnpm\"]").run_for_me, ["sudo make install", "npm i -g pnpm"], "trimmed, blanks out");
    assert_eq!(p("[\"a\", 3, null, {\"cmd\":\"b\"}, \"c\"]").run_for_me, ["a", "c"], "only the strings");
    for odd in ["42", "true", "null", "{\"cmd\":\"x\"}", "[]", "\"\""] {
        assert!(p(odd).run_for_me.is_empty(), "{odd}");
    }
    // the verdict as Gizai saves it again: no run_for_me key without commands
    let o = p("[]");
    assert!(!serde_json::to_string(&o).unwrap().contains("run_for_me"));
    assert!(serde_json::to_string(&p("[\"x\"]")).unwrap().contains("\"run_for_me\":[\"x\"]"));
}
