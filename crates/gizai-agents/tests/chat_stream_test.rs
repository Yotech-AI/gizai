use gizai_agents::chat_stream::{ChatEvent, UsageLimit, parse_line, usage_limit};
use serde_json::json;

fn events() -> Vec<ChatEvent> {
    include_str!("fixtures/chat-ok.jsonl").lines().flat_map(parse_line).collect()
}

#[test]
fn init_carries_the_session_and_the_gizai_server_status() {
    assert_eq!(events()[0], ChatEvent::Init { session_id: "C1".into(), model: "claude-haiku".into(), mcp_status: Some("connected".into()) });
}

#[test]
fn text_streams_as_a_block_start_and_deltas_then_the_finished_text() {
    let evs: Vec<ChatEvent> = events().into_iter().filter(|e| !matches!(e, ChatEvent::Other { .. })).collect();
    assert_eq!(evs[1], ChatEvent::BlockStart);
    assert_eq!(evs[2], ChatEvent::Delta { text: "Sure, ".into() });
    assert_eq!(evs[3], ChatEvent::Delta { text: "on it.".into() });
    assert_eq!(evs[4], ChatEvent::Text { text: "Sure, on it.".into() });
}

#[test]
fn thinking_deltas_are_not_text() {
    assert_eq!(events().iter().filter(|e| matches!(e, ChatEvent::Delta { .. })).count(), 2);
}

#[test]
fn tool_calls_keep_their_id_name_and_input() {
    let evs = events();
    assert!(evs.contains(&ChatEvent::ToolUse { id: "toolu_1".into(), name: "mcp__gizai__create_task".into(),
        input: json!({"project": "KADE", "title": "Export invoices"}) }));
    assert!(evs.contains(&ChatEvent::ToolResult { tool_use_id: "toolu_1".into(), is_error: false, text: "{\"ok\":true,\"identifier\":\"KADE-4\"}".into() }));
}

#[test]
fn the_result_counts_cached_input() {
    match events().last().unwrap() {
        ChatEvent::Result { cost_usd, input_tokens, output_tokens, is_error, text, .. } => {
            assert_eq!((*cost_usd, *input_tokens, *output_tokens, *is_error), (Some(0.0123), 1000, 50, false));
            assert_eq!(text, "Done: KADE-4.");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_and_broken_lines_are_other() {
    assert!(matches!(&parse_line("not json")[..], [ChatEvent::Other { raw_type }] if raw_type == "invalid"));
    assert!(matches!(&parse_line("{\"type\":\"rate_limit_event\"}")[..], [ChatEvent::Other { raw_type }] if raw_type == "rate_limit_event"));
    assert!(parse_line("   ").is_empty());
}

#[test]
fn a_string_tool_result_is_its_text() {
    let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t9","content":"plain","is_error":true}]}}"#;
    assert_eq!(parse_line(line), vec![ChatEvent::ToolResult { tool_use_id: "t9".into(), is_error: true, text: "plain".into() }]);
}

#[test]
fn an_init_without_mcp_servers_has_no_status() {
    let line = r#"{"type":"system","subtype":"init","session_id":"X","model":"m"}"#;
    assert_eq!(parse_line(line), vec![ChatEvent::Init { session_id: "X".into(), model: "m".into(), mcp_status: None }]);
}

#[test]
fn claude_codes_own_synthetic_text_is_not_an_answer() {
    let line = r#"{"type":"assistant","message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"Not logged in · Please run /login"}]}}"#;
    assert_eq!(parse_line(line), vec![ChatEvent::Other { raw_type: "synthetic".into() }]);
}

// Usage limits (GA-50). The fixture is Claude Code 2.1.289's answer when the account hit its weekly limit; the text
// follows its own template, `You've hit your ${limit}${" · resets " + when}${" · progress saved"}`.

fn limit_result() -> String {
    let evs: Vec<ChatEvent> = include_str!("fixtures/chat-limit.jsonl").lines().flat_map(parse_line).collect();
    assert!(matches!(evs[1], ChatEvent::Other { ref raw_type } if raw_type == "synthetic"), "the limit text isn't an answer: {evs:?}");
    match evs.last().unwrap() {
        ChatEvent::Result { is_error: true, text, .. } => text.clone(),
        other => panic!("not an error result: {other:?}"),
    }
}

#[test]
fn the_fixtures_weekly_limit_is_a_usage_limit_with_its_reset_time() {
    let l = usage_limit(&limit_result()).expect("a usage limit");
    assert_eq!(l, UsageLimit { limit: "weekly limit".into(), resets: Some("Oct 9, 5pm (Europe/Amsterdam)".into()), resets_at: None });
}

#[test]
fn every_limit_claude_code_names_is_read_in_plain_words() {
    for (text, limit, resets) in [
        ("You've hit your session limit · resets 3pm (Europe/Amsterdam)", "session limit", Some("3pm (Europe/Amsterdam)")),
        ("You've hit your session limit · resets 3pm (Europe/Amsterdam) · progress saved", "session limit", Some("3pm (Europe/Amsterdam)")),
        ("You've hit your Opus limit · resets Oct 9, 5pm (Europe/Amsterdam)", "Opus limit", Some("Oct 9, 5pm (Europe/Amsterdam)")),
        ("You've hit your Sonnet limit · resets Oct 9, 5pm", "Sonnet limit", Some("Oct 9, 5pm")),
        ("You've hit your limit · resets 11am (UTC)", "usage limit", Some("11am (UTC)")),
        ("You've hit your usage limit", "usage limit", None),
        ("5-hour limit reached ∙ resets 3pm", "session limit", Some("3pm")),
        ("Weekly limit reached ∙ resets Oct 9, 5pm", "weekly limit", Some("Oct 9, 5pm")),
    ] {
        let l = usage_limit(text).unwrap_or_else(|| panic!("{text}"));
        assert_eq!((l.limit.as_str(), l.resets.as_deref()), (limit, resets), "{text}");
    }
    let old = usage_limit("Claude AI usage limit reached|1751230800").unwrap();
    assert_eq!((old.limit.as_str(), old.resets, old.resets_at), ("usage limit", None, Some(1_751_230_800_000)));
}

#[test]
fn other_failures_and_limits_another_account_doesnt_solve_are_not_usage_limits() {
    for text in [
        "Not logged in · Please run /login",
        "API Error: 500 the fake failed",
        "You've hit your fast limit",
        "You've hit your monthly spend limit · raise it at claude.ai/settings/usage",
        "You've hit your org's monthly usage limit · resets Nov 1",
        "You've hit your team's shared budget. Switch to another model",
        "Context limit reached · /compact or /clear to continue",
        "Fast limit reached and temporarily disabled · resets in 3m",
        "",
    ] {
        assert_eq!(usage_limit(text), None, "{text}");
    }
}

// GA-62: chat turns and board checks read the same rate_limit_event; and the Fable model's own limit, in both of Claude
// Code's wordings ("You've hit your Fable limit · resets …" and "You've reached your Fable limit."), is a usage limit.

#[test]
fn a_chat_turns_rate_limit_event_is_a_limits_event() {
    let evs: Vec<ChatEvent> = include_str!("fixtures/run-limits.jsonl").lines().flat_map(parse_line).collect();
    let infos: Vec<&serde_json::Value> = evs.iter().filter_map(|e| match e { ChatEvent::Limits { info } => Some(info), _ => None }).collect();
    assert_eq!(infos.len(), 1, "{evs:?}");
    assert_eq!(infos[0]["unifiedWindows"]["five_hour"]["resetsAt"], 1_791_565_200);
    assert!(matches!(&parse_line(r#"{"type":"rate_limit_event","rate_limit_info":[]}"#)[..], [ChatEvent::Other { raw_type }] if raw_type == "rate_limit_event"));
}

#[test]
fn the_fable_limit_is_a_usage_limit_in_both_wordings() {
    for (text, resets) in [
        ("You've reached your Fable limit.", None),
        ("You've reached your Fable limit. Switch to another model with /model to keep going.", None),
        ("You’ve reached your Fable limit.", None),
        ("You've hit your Fable limit · resets Oct 14, 9am (Europe/Amsterdam)", Some("Oct 14, 9am (Europe/Amsterdam)")),
    ] {
        let l = usage_limit(text).unwrap_or_else(|| panic!("{text}"));
        assert_eq!((l.limit.as_str(), l.resets.as_deref()), ("Fable limit", resets), "{text}");
    }
    // Reaching a spend limit is no usage limit, in the new wording either.
    assert_eq!(usage_limit("You've reached your monthly spend limit. Raise it at claude.ai/settings/usage"), None);
}
