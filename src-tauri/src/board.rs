//! The Team Lead's board check on its heartbeat. Once a minute (`lib.rs`), when its interval has passed, Gizai looks
//! at the board in code (`gizai_core::board::check`); only findings the Team Lead hasn't seen yet start the model, in
//! a check run of its own (`chat::start_check`). Nothing new: nothing starts and nothing is spent.
use gizai_core::board::{self as core_board, Context, Finding};
use gizai_core::{ids, runs as core_runs, settings, team};
use serde_json::{Value, json};

use crate::AppState;
use crate::chat::{self, CheckSummary};

/// What the check knows beyond the database: the agents' paused pulls, Settings, the Team Lead.
pub fn context(st: &AppState, now: i64) -> Context {
    let lead = team::chat_agent(&st.db).ok().flatten().map(|m| m.actor_id);
    let s = crate::runs::get_settings(st);
    Context {
        now, pull_paused: crate::runs::pull_paused_all(st), agents_paused: s.agents_paused, max_concurrent: s.max_concurrent_runs as i64, lead_id: lead,
    }
}

/// Everything on the board that needs attention now.
pub fn findings(st: &AppState, now: i64) -> Result<Vec<Finding>, String> {
    core_board::check(&st.db, &context(st, now)).map_err(|e| e.to_string())
}

/// "2026-10-08 14:05 UTC".
pub(crate) fn when(ms: i64) -> String {
    let rem = ms.rem_euclid(86_400_000);
    format!("{} {:02}:{:02} UTC", crate::tools::ymd(ms), rem / 3_600_000, rem / 60_000 % 60)
}

fn finding_json(f: &Finding) -> Value {
    let mut v = json!({
        "kind": f.kind, "why": f.code, "task": f.task, "title": f.title, "column": f.column, "since": when(f.since), "agent": f.agent,
        "reason": f.reason, "hold": f.hold, "hold_reason": f.hold_reason, "answer": f.answer,
        "last_run": f.last_run.as_ref().map(|r| json!({"agent": r.agent, "status": r.status, "trigger": r.trigger, "outcome": r.outcome,
                                                       "error": r.error, "ended": r.ended_at.map(when)})),
    });
    // "Run this for me" (GA-31): what the agent asks the user to run, shown in their Inbox with Done, continue.
    if !f.run_for_me.is_empty() {
        v["run_for_me"] = json!(f.run_for_me);
    }
    v
}

/// The agents' slots: per agent its cards at once, the cards it runs now and whether its pull is paused (and why), and
/// "Runs at once" (Settings) with what is free.
pub fn slots_json(st: &AppState) -> Value {
    let live = crate::runs::live(st);
    let s = crate::runs::get_settings(st);
    let ident = |id: &str| gizai_core::tasks::get(&st.db, id).map(|t| t.identifier).unwrap_or_default();
    let agents: Vec<Value> = team::all_agents(&st.db).unwrap_or_default().into_iter().map(|(_, m)| {
        let mine: Vec<String> = live.iter().filter(|r| r.agent_id == m.actor_id).map(|r| ident(&r.task_id)).collect();
        json!({
            "name": m.name, "role": m.role_key, "status": m.status, "columns": gizai_core::columns::of_agent(&st.db, &m.actor_id).unwrap_or_default(),
            "cards_at_once": m.max_runs, "working_on": mine,
            "free_slots": (m.max_runs.max(1) - mine.len() as i64).max(0), "pull_paused": crate::runs::pull_paused(st, &m.actor_id),
        })
    }).collect();
    json!({
        "runs_at_once": {"max": s.max_concurrent_runs, "working": live.len(), "free": (s.max_concurrent_runs as usize).saturating_sub(live.len())},
        "agents_paused": s.agents_paused,
        "agents": agents,
    })
}

/// The `check_board` tool: the findings, then the agents' slots.
pub fn check_json(st: &AppState) -> Result<Value, String> {
    let all = findings(st, ids::now_ms())?;
    let mut v = slots_json(st);
    v["findings"] = Value::Array(all.iter().map(finding_json).collect());
    v["count"] = json!(all.len());
    Ok(v)
}

/// The check run's message: the new findings, how many it saw before, and the free slots.
pub fn check_prompt(st: &AppState, all: &[Finding], new: &[Finding]) -> String {
    let new_json: Vec<Value> = new.iter().map(finding_json).collect();
    format!(
        "Board check, {now}. {n} new finding(s) since your last check, to handle by your rules:\n\n```json\n{new}\n```\n\n\
         {seen} other finding(s) you saw before are unchanged (check_board lists everything).\n\n\
         The free slots now (never start more runs than these allow):\n\n```json\n{slots}\n```",
        now = when(ids::now_ms()), n = new.len(), new = serde_json::to_string_pretty(&new_json).unwrap_or_default(),
        seen = all.len() - new.len(), slots = serde_json::to_string_pretty(&slots_json(st)).unwrap_or_default(),
    )
}

/// Why a check may not run now: the Team Lead is paused or over its budget, agents are paused in Settings, Gizai is
/// quitting, or a check is still running.
fn blocked(st: &AppState, agent: &team::Member) -> Option<String> {
    if agent.status != "active" {
        return Some(format!("{} is paused", agent.name));
    }
    if let Some(budget) = agent.budget_usd_micros {
        let spent = core_runs::agent_spend_since(&st.db, &agent.actor_id, core_runs::month_start_ms(ids::now_ms())).unwrap_or(0);
        if spent >= budget {
            return Some(format!("{} has used its monthly budget", agent.name));
        }
    }
    if settings::get::<bool>(&st.db, "agents_paused").ok().flatten().unwrap_or(false) {
        return Some("agents are paused in Settings".into());
    }
    if crate::runs::is_closing(st) {
        return Some("Gizai is quitting".into());
    }
    if chat::checking(st) {
        return Some("a board check is still running".into());
    }
    None
}

/// One beat: when the Team Lead's check is on, not paused after failures and its interval has passed, Gizai looks at the
/// board, and new findings start one check run. Returns its handle when one started.
pub async fn tick(st: &AppState, now: i64) -> Option<tokio::task::JoinHandle<CheckSummary>> {
    let agent = team::chat_agent(&st.db).ok().flatten()?;
    let every = agent.board_check_minutes.filter(|m| *m > 0)? * 60_000;
    if agent.board_check_paused.is_some() || matches!(agent.board_checked_at, Some(last) if now - last < every) {
        return None;
    }
    if blocked(st, &agent).is_some() {
        return None;
    }
    let _ = core_board::touch(&st.db, &agent.actor_id, now);
    let all = findings(st, now).map_err(|e| eprintln!("gizai: the board check failed: {e}")).ok()?;
    let new = core_board::new_findings(&st.db, &agent.actor_id, &all).ok()?;
    if new.is_empty() {
        return None;
    }
    let prompt = check_prompt(st, &all, &new);
    match chat::start_check(st, agent, prompt, core_board::seen_of(&all)) {
        Ok(done) => Some(done),
        Err(e) => { eprintln!("gizai: the Team Lead's board check couldn't start: {e}"); None }
    }
}
