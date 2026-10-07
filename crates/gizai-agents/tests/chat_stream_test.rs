use gizai_agents::chat_stream::{ChatEvent, parse_line};
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
