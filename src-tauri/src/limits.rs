//! Subscription limits (GA-62): what the coding CLIs report of their accounts' limits, kept per CLI as runs and chat turns
//! go (`gizai_core::limits`), and the Usage page's Subscription tab. A reading counts for the CLI the run recorded only.
use gizai_agents::chat_stream::UsageLimit;
use gizai_agents::cli::Kind;
use gizai_core::limits::{self as core_limits, Reading};
use gizai_core::{clis as core_clis, ids, runs as core_runs};
use serde_json::Value;

use crate::AppState;
use crate::runs::Note;

fn home() -> String {
    core_clis::home()
}

/// Gizai's own environment, where a CLI without its own CLAUDE_CONFIG_DIR or CODEX_HOME line gets them.
fn inherited(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn keep(st: &AppState, run_id: &str, readings: &[Reading]) {
    if readings.is_empty() {
        return;
    }
    match core_limits::record_for_run(&st.db, run_id, readings) {
        Ok(true) => (st.notify)(Note::RowsChanged("subscription_limits")),
        Ok(false) => {}
        Err(e) => eprintln!("gizai: keeping the limits run {run_id} reported failed: {e}"),
    }
}

/// Claude Code's `rate_limit_event` in run or chat turn `run_id` (its `rate_limit_info`).
pub fn from_claude(st: &AppState, run_id: &str, info: &Value) {
    keep(st, run_id, &core_limits::claude_info(info, ids::now_ms()));
}

/// A limit run or chat turn `run_id` hit, as Claude Code said it in words (`chat_stream::usage_limit`).
pub fn hit(st: &AppState, run_id: &str, limit: &UsageLimit) {
    keep(st, run_id, &[core_limits::hit(&limit.limit, limit.resets.as_deref(), limit.resets_at, ids::now_ms())]);
}

/// After a task run on `kind`: Codex's limits from the run's own session log (its output has none); a Claude Code run that
/// failed says in `failed_text` (its result, else the end of its stderr) when it hit a limit.
pub async fn after_run(st: &AppState, run_id: &str, kind: Kind, failed_text: Option<&str>) {
    match kind {
        Kind::ClaudeCode => {
            if let Some(limit) = failed_text.and_then(gizai_agents::chat_stream::usage_limit) {
                hit(st, run_id, &limit);
            }
        }
        Kind::Codex => {
            let Ok(run) = core_runs::get(&st.db, run_id) else { return };
            let (Some(thread), Ok(cli)) = (run.session_id.clone(), core_clis::get(&st.db, run.adapter.as_deref().unwrap_or_default())) else { return };
            let Some(dir) = core_limits::account_dir(&cli, &home(), &inherited) else { return };
            let readings = tokio::task::spawn_blocking(move || core_limits::codex_limits(&dir, &thread)).await.unwrap_or_default();
            keep(st, run_id, &readings);
        }
        Kind::Gemini | Kind::Other => {}
    }
}

/// The Usage page's Subscription tab: a block per coding CLI with its limits and the agents on it.
pub fn subscription(st: &AppState) -> Result<Vec<core_limits::CliLimits>, String> {
    core_limits::subscription(&st.db, &home(), &inherited).map_err(|e| e.to_string())
}
