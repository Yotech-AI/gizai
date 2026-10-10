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
    /// `rate_limit_event`: what Claude Code last heard of the account's subscription limits (`rate_limit_info`, as it wrote
    /// it), kept for the turn's coding CLI (`gizai_core::limits`).
    Limits { info: Value },
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
        "rate_limit_event" if v.get("rate_limit_info").is_some_and(Value::is_object) => vec![ChatEvent::Limits { info: v["rate_limit_info"].clone() }],
        other => vec![ChatEvent::Other { raw_type: other.into() }],
    }
}

/// An answer that failed because the Claude Code account hit a usage limit.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageLimit {
    /// Which limit, in plain words: "session limit", "weekly limit", "Opus limit", "Sonnet limit" or "usage limit".
    pub limit: String,
    /// When it resets, as Claude Code says it ("3pm (Europe/Amsterdam)", "Oct 9, 5pm (Europe/Amsterdam)"); None when it doesn't.
    pub resets: Option<String>,
    /// When it resets (Unix ms), when Claude Code gives a time stamp instead ("Claude AI usage limit reached|1751230800").
    pub resets_at: Option<i64>,
}

/// The separators Claude Code puts between the parts of a notice: "·" (2.x) and "∙" (1.x).
const SEPARATORS: [&str; 3] = [" · ", " ∙ ", " • "];

fn first_part(s: &str) -> &str {
    let end = SEPARATORS.iter().filter_map(|sep| s.find(sep)).chain(s.find('\n')).min().unwrap_or(s.len());
    &s[..end]
}

/// "resets 3pm (Europe/Amsterdam)" anywhere in `s`: the time, as written.
fn resets_in(s: &str) -> Option<String> {
    let i = s.find("resets ")?;
    let when = first_part(&s[i + "resets ".len()..]).trim().trim_end_matches('.').trim();
    (!when.is_empty()).then(|| when.to_string())
}

/// A limit's name in plain words, from how Claude Code names it ("session limit", "5-hour limit", "Opus weekly limit",
/// "Fable limit").
fn limit_name(name: &str) -> String {
    let n = name.to_lowercase();
    if n.contains("session") || n.contains("5-hour") || n.contains("five-hour") {
        "session limit".into()
    } else if n.contains("fable") {
        "Fable limit".into()
    } else if n.contains("opus") {
        "Opus limit".into()
    } else if n.contains("sonnet") {
        "Sonnet limit".into()
    } else if n.contains("weekly") || n.contains("week") {
        "weekly limit".into()
    } else {
        "usage limit".into()
    }
}

/// Claude Code's text for a subscription that hit its usage limit, which a failed answer reports as its result: what
/// 2.1.289 writes ("You've hit your session limit · resets 3pm (Europe/Amsterdam)", likewise the weekly, Opus and Sonnet
/// limits, " · progress saved" after it; "You've reached your Fable limit." for the Fable model's own weekly limit) and
/// what older versions wrote ("Claude AI usage limit reached|1751230800", "5-hour limit reached ∙ resets 3pm", "Weekly
/// limit reached ∙ resets Oct 9, 5pm"). None for any other text, also for spend limits, budgets and fast mode: those
/// aren't solved by another account.
pub fn usage_limit(text: &str) -> Option<UsageLimit> {
    let text = text.trim();
    // ASCII only, so its byte positions are the text's.
    let lower = text.to_ascii_lowercase();
    // 2.x: "You've hit your <name> · resets <when>", "You've reached your <name>. …".
    for lead in ["you've hit your ", "you’ve hit your ", "you have hit your ", "you've reached your ", "you’ve reached your "] {
        if let Some(i) = lower.find(lead) {
            let rest = &text[i + lead.len()..];
            let part = first_part(rest);
            let name = part.split(". ").next().unwrap_or(part).trim().trim_end_matches('.');
            let n = name.to_lowercase();
            let usage = n.ends_with("limit") && !["fast", "spend", "credit", "budget", "monthly"].iter().any(|w| n.contains(w));
            if usage {
                return Some(UsageLimit { limit: limit_name(name), resets: resets_in(rest), resets_at: None });
            }
        }
    }
    // 1.x: "Claude AI usage limit reached|<unix seconds>".
    if let Some(i) = lower.find("usage limit reached|") {
        let digits: String = text[i + "usage limit reached|".len()..].chars().take_while(char::is_ascii_digit).collect();
        let at = digits.parse::<i64>().ok().map(|n| if n < 100_000_000_000 { n * 1000 } else { n });
        return Some(UsageLimit { limit: "usage limit".into(), resets: None, resets_at: at });
    }
    // 1.x after the weekly limits came: "<name> limit reached ∙ resets <when>".
    if let Some(i) = lower.find("limit reached") {
        let start = lower[..i].rfind(['\n', '.', '·', '∙']).map(|j| j + lower[j..].chars().next().map_or(1, char::len_utf8)).unwrap_or(0);
        let name = text[start..i + "limit".len()].trim();
        let n = name.to_lowercase();
        let usage = ["usage", "session", "5-hour", "weekly", "opus", "sonnet"].iter().any(|w| n.contains(w))
            && !["fast", "spend", "credit", "budget", "context", "monthly"].iter().any(|w| n.contains(w));
        if usage {
            return Some(UsageLimit { limit: limit_name(name), resets: resets_in(&text[i..]), resets_at: None });
        }
    }
    None
}
