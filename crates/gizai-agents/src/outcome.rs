//! The agent's verdict: the last `GIZAI_RESULT: {json}` line of its final message.
use serde::{Deserialize, Deserializer, Serialize};

pub const OUTCOMES: [&str; 5] = ["ready_for_testing", "qa_pass", "qa_fail", "needs_decision", "deployed"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub outcome: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub issues: Vec<String>,
    /// "Run this for me" (GA-31): on a `needs_decision`, the commands the agent may not run (sudo, an install, one its
    /// list refuses) that it asks the user to run, each exactly as it is to be typed. Optional: older result lines have
    /// none. A single command as a string counts as a list of one; anything else, and blank entries, are left out.
    #[serde(default, deserialize_with = "commands", skip_serializing_if = "Vec::is_empty")]
    pub run_for_me: Vec<String>,
}

/// `run_for_me` as the agent wrote it: a list of commands, or one command as a string. A value of another kind
/// doesn't spoil the result line; it only asks for nothing.
fn commands<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let list = match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => vec![s],
        serde_json::Value::Array(items) => items.into_iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => vec![],
    };
    Ok(list.into_iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect())
}

/// None when there is no result line, the last one isn't valid JSON, or its outcome is unknown.
pub fn parse(final_text: &str) -> Option<Outcome> {
    let line = final_text.lines().rev().map(str::trim_start).find(|l| l.starts_with("GIZAI_RESULT:"))?;
    let o: Outcome = serde_json::from_str(line["GIZAI_RESULT:".len()..].trim()).ok()?;
    OUTCOMES.contains(&o.outcome.as_str()).then_some(o)
}

/// At most this many `learned` lines are kept from one result.
pub const MAX_LEARNED: usize = 20;

/// Memory (GA-19): the optional `learned` list on the last result line that `parse` accepts, the short lines the agent
/// wants kept in its own notes. A single string counts as a list of one; blank entries, other values and a result line
/// without the list give nothing. Apart from `Outcome`, so a result line parses as it always did.
pub fn learned(final_text: &str) -> Vec<String> {
    if parse(final_text).is_none() {
        return vec![];
    }
    let Some(line) = final_text.lines().rev().map(str::trim_start).find(|l| l.starts_with("GIZAI_RESULT:")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line["GIZAI_RESULT:".len()..].trim()) else { return vec![] };
    let list = match v.get("learned") {
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        Some(serde_json::Value::Array(items)) => items.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        _ => vec![],
    };
    list.into_iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).take(MAX_LEARNED).collect()
}
