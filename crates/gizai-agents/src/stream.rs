//! Claude Code `--output-format stream-json` lines → a few event kinds the Run panel shows.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunEvent {
    Init { session_id: String, model: String },
    Text { text: String },
    ToolUse { name: String, summary: String },
    ToolResult { is_error: bool, preview: String },
    Result { is_error: bool, subtype: String, text: String, cost_usd: Option<f64>, input_tokens: i64, output_tokens: i64, num_turns: i64 },
    Other { raw_type: String },
    /// A note from Gizai in the run log, such as a folder the run goes without (`cli::note_line`).
    Note { text: String },
    /// A tool call the CLI refused because it needed an approval nobody can give in a headless run: the tool, what it
    /// asked for (the command, the file) and why, when the CLI said. Claude Code lists them in its result line
    /// (`permission_denials`, without a reason); `cli::Parser` also shows each one as it happens, with its reason.
    Refused { tool: String, input: String, reason: String },
}

fn s(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// At most `n` characters, never splitting one.
pub(crate) fn cut(text: &str, n: usize) -> String {
    match text.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text.to_string(),
    }
}

/// What a refused tool call asked for: its command, file, URL or query, else its input, at most 300 characters.
pub fn refused_input(input: &Value) -> String {
    for k in ["command", "file_path", "path", "notebook_path", "url", "query", "pattern"] {
        if let Some(v) = input.get(k).and_then(Value::as_str) {
            return cut(v, 300);
        }
    }
    cut(&input.to_string(), 300)
}

/// The refused tool calls in a Claude Code result line (`permission_denials`: tool_name, tool_use_id, tool_input).
fn refusals(v: &Value) -> Vec<RunEvent> {
    v.get("permission_denials").and_then(Value::as_array).into_iter().flatten()
        .map(|d| RunEvent::Refused { tool: s(&d["tool_name"]), input: refused_input(&d["tool_input"]), reason: String::new() })
        .collect()
}

/// What a tool call is about: its file, command, pattern or URL, else the start of its input.
pub(crate) fn summarize(input: &Value) -> String {
    for k in ["file_path", "path", "command", "pattern", "url", "notebook_path", "description"] {
        if let Some(v) = input.get(k).and_then(Value::as_str) {
            return cut(v, 120);
        }
    }
    cut(&input.to_string(), 120)
}

fn result_text(content: &Value) -> String {
    match content {
        Value::String(t) => t.clone(),
        Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// One line of stream-json → zero or more events. Blank → none; broken JSON → `Other{invalid}`; never panics.
pub fn parse_line(line: &str) -> Vec<RunEvent> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return vec![RunEvent::Other { raw_type: "invalid".into() }];
    };
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("unknown");
    let blocks = || v.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
    match ty {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            vec![RunEvent::Init { session_id: s(&v["session_id"]), model: s(&v["model"]) }]
        }
        "assistant" => {
            let out: Vec<RunEvent> = blocks().iter().map(|b| match b.get("type").and_then(Value::as_str).unwrap_or("") {
                "text" => RunEvent::Text { text: s(&b["text"]) },
                "tool_use" => RunEvent::ToolUse { name: s(&b["name"]), summary: summarize(&b["input"]) },
                other => RunEvent::Other { raw_type: if other.is_empty() { "assistant".into() } else { other.into() } },
            }).collect();
            if out.is_empty() { vec![RunEvent::Other { raw_type: "assistant".into() }] } else { out }
        }
        "user" => {
            let out: Vec<RunEvent> = blocks().iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                .map(|b| RunEvent::ToolResult {
                    is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                    preview: cut(&result_text(&b["content"]), 200),
                }).collect();
            if out.is_empty() { vec![RunEvent::Other { raw_type: "user".into() }] } else { out }
        }
        "result" => {
            let subtype = s(&v["subtype"]);
            let u = &v["usage"];
            let n = |k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
            // What was refused, then the result.
            let mut out = refusals(&v);
            out.push(RunEvent::Result {
                is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(subtype != "success"),
                text: s(&v["result"]),
                cost_usd: v.get("total_cost_usd").and_then(Value::as_f64),
                // Cached input is still input: count it, so token totals match what the run used.
                input_tokens: n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens"),
                output_tokens: n("output_tokens"),
                num_turns: v.get("num_turns").and_then(Value::as_i64).unwrap_or(0),
                subtype,
            });
            out
        }
        "gizai_note" => vec![RunEvent::Note { text: s(&v["text"]) }],
        other => vec![RunEvent::Other { raw_type: other.into() }],
    }
}
