//! The agent's verdict: the last `GIZAI_RESULT: {json}` line of its final message.
use serde::{Deserialize, Serialize};

pub const OUTCOMES: [&str; 4] = ["ready_for_testing", "qa_pass", "qa_fail", "needs_decision"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub outcome: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub issues: Vec<String>,
}

/// None when there is no result line, the last one isn't valid JSON, or its outcome is unknown.
pub fn parse(final_text: &str) -> Option<Outcome> {
    let line = final_text.lines().rev().map(str::trim_start).find(|l| l.starts_with("GIZAI_RESULT:"))?;
    let o: Outcome = serde_json::from_str(line["GIZAI_RESULT:".len()..].trim()).ok()?;
    OUTCOMES.contains(&o.outcome.as_str()).then_some(o)
}
