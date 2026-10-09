//! What an MCP server's tool does, in plain words: its name, title, description and parameters (from `inputSchema`),
//! what the server says about it (`annotations`), and a risk. A hint the server didn't send follows the MCP spec's
//! default (2025-06-18): not read-only, may delete or overwrite, not safe to repeat, reaches outside services. So a
//! tool without hints counts as high risk, and the view says which hints the server sent.
use std::collections::HashSet;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::stream::cut;

/// The longest description or parameter description a view keeps.
const MAX_TEXT: usize = 2000;
/// The most parameters a view lists.
const MAX_PARAMS: usize = 100;
/// Where List tools keeps the order of a tool's parameters (the keys of `inputSchema.properties`) as the server wrote
/// them, since serde_json's maps sort their keys. Not MCP: `mcp_client` adds it to each tool it lists.
pub const PARAM_ORDER: &str = "x-gizai-param-order";

/// One parameter of a tool.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    pub name: String,
    /// Its JSON type, like "string" or "number | null"; "any" when the schema doesn't say.
    pub ty: String,
    pub required: bool,
    pub description: String,
}

/// What the server says about a tool, its defaults filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hints {
    /// It only reads (`readOnlyHint`; default false).
    pub read_only: bool,
    /// It may delete or overwrite (`destructiveHint`; default true, and only when it isn't read-only).
    pub destructive: bool,
    /// Calling it again with the same input changes nothing more (`idempotentHint`; default false).
    pub idempotent: bool,
    /// It reaches services outside the server, like the web (`openWorldHint`; default true).
    pub open_world: bool,
}

/// A tool as Settings and the agent form show it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolView {
    pub name: String,
    pub title: Option<String>,
    pub description: String,
    pub params: Vec<Param>,
    pub hints: Hints,
    /// The hints the server sent, like ["readOnlyHint"]; the others follow the MCP defaults.
    pub hints_sent: Vec<String>,
    /// low (only reads) | medium (changes things, never deletes or overwrites) | high (may delete or overwrite)
    pub risk: String,
    /// One line: what it may do, like "Only reads." or "May delete or overwrite. Reaches outside services."
    pub summary: String,
    /// Which hints the server sent, and what is assumed for the rest.
    pub notes: Vec<String>,
}

const HINTS: [&str; 4] = ["readOnlyHint", "destructiveHint", "idempotentHint", "openWorldHint"];

/// One `tools/list` entry in plain words.
pub fn describe(tool: &Value) -> ToolView {
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
    let annotations = tool.get("annotations").and_then(Value::as_object);
    let hint = |key: &str| annotations.and_then(|a| a.get(key)).and_then(Value::as_bool);
    let hints_sent: Vec<String> = HINTS.iter().filter(|k| hint(k).is_some()).map(|k| k.to_string()).collect();
    let read_only = hint("readOnlyHint").unwrap_or(false);
    let hints = Hints {
        read_only,
        // The spec only gives these meaning for a tool that isn't read-only.
        destructive: !read_only && hint("destructiveHint").unwrap_or(true),
        idempotent: read_only || hint("idempotentHint").unwrap_or(false),
        open_world: hint("openWorldHint").unwrap_or(true),
    };
    let risk = if hints.read_only { "low" } else if hints.destructive { "high" } else { "medium" };
    let title = Some(text(tool.get("title"))).filter(|t| !t.is_empty())
        .or_else(|| Some(text(annotations.and_then(|a| a.get("title")))).filter(|t| !t.is_empty()));
    ToolView {
        name: text(tool.get("name")),
        title,
        description: cut(&text(tool.get("description")), MAX_TEXT),
        params: params(tool.get("inputSchema"), tool.get(PARAM_ORDER)),
        hints,
        notes: notes(&hints_sent),
        hints_sent,
        risk: risk.to_string(),
        summary: summary(&hints),
    }
}

/// Every tool of a listing, those without a name left out.
pub fn describe_all(tools: &[Value]) -> Vec<ToolView> {
    tools.iter().map(describe).filter(|t| !t.name.is_empty()).collect()
}

/// What the tool may do, in one line.
fn summary(h: &Hints) -> String {
    let mut out = vec![if h.read_only {
        "Only reads."
    } else if h.destructive {
        "May delete or overwrite."
    } else {
        "Changes things, but doesn't delete or overwrite."
    }];
    if h.open_world {
        out.push("Reaches outside services.");
    }
    if !h.read_only && h.idempotent {
        out.push("Safe to repeat.");
    }
    out.join(" ")
}

/// Which hints the server sent, and that the rest follow the MCP defaults.
fn notes(sent: &[String]) -> Vec<String> {
    let plain = |k: &str| match k {
        "readOnlyHint" => "only reads",
        "destructiveHint" => "may delete or overwrite",
        "idempotentHint" => "safe to repeat",
        _ => "reaches outside services",
    };
    if sent.is_empty() {
        return vec!["The server sent no hints about this tool, so Gizai assumes the MCP defaults: it may change, delete or overwrite \
                     things, and reach outside services.".to_string()];
    }
    let said: Vec<&str> = sent.iter().map(|k| plain(k)).collect();
    let mut out = vec![format!("The server said whether it {}.", said.join(", "))];
    let missing: Vec<&str> = HINTS.iter().filter(|k| !sent.iter().any(|s| s == *k)).map(|k| plain(k)).collect();
    if !missing.is_empty() {
        out.push(format!("It didn't say whether it {}: Gizai assumes the MCP defaults for those.", missing.join(", ")));
    }
    out
}

/// The parameters in an `inputSchema` (`properties`, `required`), in the order the server wrote them (`order`, from
/// `PARAM_ORDER`); by name when that order isn't known.
fn params(schema: Option<&Value>, order: Option<&Value>) -> Vec<Param> {
    let Some(schema) = schema.and_then(Value::as_object) else { return vec![] };
    let required: Vec<&str> = schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
    let Some(props) = schema.get("properties").and_then(Value::as_object) else { return vec![] };
    let written = order.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
    let mut seen = HashSet::new();
    written.chain(props.keys().map(String::as_str)).filter(|n| props.contains_key(*n) && seen.insert(*n)).take(MAX_PARAMS).map(|name| {
        let p = &props[name];
        Param {
            name: name.to_string(),
            ty: type_of(p.as_object()),
            required: required.contains(&name),
            description: cut(p.get("description").and_then(Value::as_str).map(str::trim).unwrap_or_default(), MAX_TEXT),
        }
    }).collect()
}

/// A property's type in a few words: "string", "string | null", "one of a, b", "array of string", or "any".
fn type_of(p: Option<&Map<String, Value>>) -> String {
    let Some(p) = p else { return "any".into() };
    if let Some(values) = p.get("enum").and_then(Value::as_array) {
        let shown: Vec<String> = values.iter().take(10).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).collect();
        return format!("one of {}{}", shown.join(", "), if values.len() > 10 { ", …" } else { "" });
    }
    match p.get("type") {
        Some(Value::String(t)) if t == "array" => match p.get("items").and_then(Value::as_object) {
            Some(items) if items.contains_key("type") || items.contains_key("enum") => format!("array of {}", type_of(Some(items))),
            _ => "array".into(),
        },
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(ts)) => {
            let all: Vec<&str> = ts.iter().filter_map(Value::as_str).collect();
            if all.is_empty() { "any".into() } else { all.join(" | ") }
        }
        _ => {
            let options: Vec<String> = ["anyOf", "oneOf"].iter().filter_map(|k| p.get(*k).and_then(Value::as_array)).flatten()
                .map(|o| type_of(o.as_object())).collect();
            if options.is_empty() { "any".into() } else { options.join(" | ") }
        }
    }
}
