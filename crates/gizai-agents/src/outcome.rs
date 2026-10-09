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
