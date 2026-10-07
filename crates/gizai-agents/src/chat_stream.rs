//! Claude Code `stream-json` lines with `--include-partial-messages` → what the Chat page needs: text as it is
//! written (deltas), finished text blocks, tool calls with their id and full input, tool results, the end.
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatEvent {
    /// `mcp_status`: the status Claude Code reports for the `gizai` MCP server ("connected", "failed", …).
    Init { session_id: String, model: String, mcp_status: Option<String> },
    /// A new text block starts: the text written so far belongs to a finished block.
    BlockStart,
    Delta { text: String },
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, is_error: bool, text: String },
    Result { is_error: bool, subtype: String, text: String, cost_usd: Option<f64>, input_tokens: i64, output_tokens: i64, num_turns: i64 },
    Other { raw_type: String },
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(t) => t.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// One line → zero or more events. Blank → none; broken JSON → `Other{invalid}`; never panics.
pub fn parse_line(line: &str) -> Vec<ChatEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![ChatEvent::Other { raw_type: "invalid".into() }];
    };
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("unknown");
    let blocks = || v.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
    match ty {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            let mcp_status = v.get("mcp_servers").and_then(Value::as_array)
                .and_then(|list| list.iter().find(|m| m.get("name").and_then(Value::as_str) == Some("gizai")))
                .and_then(|m| m.get("status").and_then(Value::as_str)).map(str::to_string);
            vec![ChatEvent::Init { session_id: s(&v["session_id"]), model: s(&v["model"]), mcp_status }]
        }
        "stream_event" => {
            let e = &v["event"];
            match e.get("type").and_then(Value::as_str) {
                Some("content_block_start") if e.pointer("/content_block/type").and_then(Value::as_str) == Some("text") => vec![ChatEvent::BlockStart],
                Some("content_block_delta") if e.pointer("/delta/type").and_then(Value::as_str) == Some("text_delta") => {
                    vec![ChatEvent::Delta { text: s(&e["delta"]["text"]) }]
                }
                _ => vec![],
            }
        }
        // Claude Code writes some of its own notices ("Not logged in") as a synthetic assistant message: not an answer.
        "assistant" if v.pointer("/message/model").and_then(Value::as_str) == Some("<synthetic>") => vec![ChatEvent::Other { raw_type: "synthetic".into() }],
        "assistant" => blocks().iter().filter_map(|b| match b.get("type").and_then(Value::as_str) {
            Some("text") => Some(ChatEvent::Text { text: s(&b["text"]) }),
            Some("tool_use") => Some(ChatEvent::ToolUse { id: s(&b["id"]), name: s(&b["name"]), input: b.get("input").cloned().unwrap_or(Value::Null) }),
            _ => None,
        }).collect(),
        "user" => blocks().iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            .map(|b| ChatEvent::ToolResult {
                tool_use_id: s(&b["tool_use_id"]),
                is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                text: result_text(&b["content"]),
            }).collect(),
        "result" => {
            let subtype = s(&v["subtype"]);
            let u = &v["usage"];
            let n = |k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
            vec![ChatEvent::Result {
                is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(subtype != "success"),
                text: s(&v["result"]),
                cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
                input_tokens: n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens"),
                output_tokens: n("output_tokens"),
                num_turns: v.get("num_turns").and_then(Value::as_i64).unwrap_or(0),
                subtype,
            }]
        }
        other => vec![ChatEvent::Other { raw_type: other.into() }],
    }
}
