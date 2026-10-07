//! Serde models shared with the UI (camelCase on the wire).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub id: String,
    pub name: String,
    pub legal_name: Option<String>,
    pub kind: String,
    pub vat_number: Option<String>,
    pub coc_number: Option<String>,
    pub iban: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub street: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub currency: String,
    pub payment_terms_days: Option<i64>,
    pub status: String,
    pub notes_md: Option<String>,
    pub main_contact: Option<String>,
    pub open_tasks: i64,
    pub projects: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClientInput {
    pub name: String,
    pub legal_name: Option<String>,
    pub kind: Option<String>,
    pub vat_number: Option<String>,
    pub coc_number: Option<String>,
    pub iban: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub street: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub payment_terms_days: Option<i64>,
    pub status: Option<String>,
    pub notes_md: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Contact {
    pub id: String,
    pub client_id: String,
    pub name: String,
    pub role: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub title: Option<String>,
    pub email: Option<String>,
    pub open_tasks: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub client_id: Option<String>,
    pub client_name: Option<String>,
    pub number: String,
    pub key: String,
    pub name: String,
    pub status: String,
    pub color: Option<String>,
    pub goal_md: Option<String>,
    pub repo_path: Option<String>,
    /// The repository on GitHub (https://github.com/owner/name) or another git URL; new cards start from its branch.
    pub repo_url: Option<String>,
    pub default_branch: String,
    pub team_id: Option<String>,
    pub budget_amount_minor: Option<i64>,
    pub budget_hours: Option<f64>,
    pub open_tasks: i64,
    pub done_tasks: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectInput {
    pub client_id: Option<String>,
    pub name: String,
    pub key: String,
    pub status: Option<String>,
    pub goal_md: Option<String>,
    pub repo_path: Option<String>,
    /// GitHub link in any usual form, or another git URL; empty removes it.
    pub repo_url: Option<String>,
    pub default_branch: Option<String>,
    pub color: Option<String>,
    pub budget_amount_minor: Option<i64>,
    pub budget_hours: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub identifier: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub project_color: Option<String>,
    pub title: String,
    pub description_md: String,
    pub acceptance_md: Option<String>,
    pub state_id: String,
    pub state_name: String,
    pub state_category: String,
    pub priority: i64,
    pub assignee_id: Option<String>,
    pub assignee_name: Option<String>,
    pub assignee_kind: Option<String>,
    pub labels: Vec<Label>,
    pub hold: Option<String>,
    pub hold_reason: Option<String>,
    pub bounce_count: i64,
    pub fail_count: i64,
    pub sort_key: String,
    pub branch: Option<String>,
    /// The card's pull request on GitHub, and its state as Gizai last saw it: open, draft, merged or closed.
    pub pr_url: Option<String>,
    pub pr_state: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskInput {
    pub project_id: String,
    pub title: String,
    pub description_md: String,
    pub acceptance_md: Option<String>,
    pub state_id: Option<String>,
    pub priority: i64,
    pub assignee_id: Option<String>,
    pub label_ids: Vec<String>,
}

/// Every field is optional; for optional columns an empty string clears the value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description_md: Option<String>,
    pub acceptance_md: Option<String>,
    pub priority: Option<i64>,
    pub assignee_id: Option<String>,
    pub pinned_actor_id: Option<String>,
    pub due_on: Option<String>,
    pub hold: Option<String>,
    pub hold_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TaskFilter {
    pub project_id: Option<String>,
    pub open_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub author_kind: String,
    pub body_md: String,
    pub run_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub at: i64,
    pub actor_name: Option<String>,
    pub table: String,
    pub op: String,
    pub diff: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub id: String,
    pub project_id: Option<String>,
    pub title: String,
    /// Empty in lists; filled by `docs::get`.
    pub body_md: String,
    pub current_version: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocVersion {
    pub version: i64,
    pub author_name: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRow {
    pub id: String,
    pub name: String,
    pub mime: Option<String>,
    pub size_bytes: i64,
    pub sha256: String,
    pub created_at: i64,
}

/// An agent's verdict (same shape as gizai-agents' `outcome::Outcome`; the app converts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    pub outcome: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub issues: Vec<String>,
}

/// New or changed agent. Empty strings take the defaults: adapter "claude_code", permission mode
/// "acceptEdits", wake-up "manual"; instructions None = the role's template (on create) or unchanged (on update).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentInput {
    pub name: String,
    pub role_key: String,
    pub title: Option<String>,
    pub adapter: String,
    pub model: Option<String>,
    pub instructions_md: Option<String>,
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    /// "manual" | "on_assign" | "heartbeat"
    pub wakeup: String,
    pub heartbeat_minutes: Option<i64>,
    pub budget_usd_micros: Option<i64>,
    /// Answers on the Chat page (at most one agent does). None: off for a new agent, unchanged on update.
    pub chat_enabled: Option<bool>,
    /// Claude Code `--effort` (low, medium, high, xhigh, max); None or empty = Claude Code's default.
    pub effort: Option<String>,
    /// Cards it works on at once (1–10), each in its own worktree. None: 1 for a new agent, unchanged on update.
    pub max_runs: Option<i64>,
}

/// "When a card has label <match_name>" or "enters column <match_name>" → the first idle agent with `target_role`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleInput {
    /// "label" | "column"
    pub kind: String,
    pub match_name: String,
    pub target_role: String,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub task_id: Option<String>,
    pub role_key: Option<String>,
    pub trigger: String,
    pub status: String,
    pub outcome: Option<String>,
    pub summary_md: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub cost_usd_micros: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub session_id: Option<String>,
    pub error: Option<String>,
    pub log_path: String,
    /// The claude process (its process group leader) while the run is running.
    pub pid: Option<i64>,
    /// The commit its worktree was at when it started.
    pub base_sha: Option<String>,
}

/// An agent's runs on one UTC day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub day_start: i64,
    pub succeeded: i64,
    /// failed or timed out
    pub failed: i64,
    /// cancelled, queued or running
    pub other: i64,
}
