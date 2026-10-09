//! The Team Lead's board check, in code (no model): what on the board needs attention. Answered cards (a person's
//! comment after a "needs a decision" hold), other held cards, cards in an Auto column no agent will start (with why)
//! and cards in In progress whose run stopped part-way. Only To do, In progress and Testing count (cards in a Manual
//! column wait for Run by design), and paused, done and archived projects are left out. The Team Lead's scheduled check and
//! the `check_board` tool both use it, so they see the same thing.
//!
//! What each check saw is kept on its run (`runs.findings_json`): a finding is new when the Team Lead hasn't seen it
//! yet, or its card changed since (a comment, a run, a move, the hold), not counting the Team Lead's own changes.
use std::collections::HashMap;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::{Error, Result, ids, runs, team, workflow};

/// An agent with a free slot takes a waiting card within this long (the queue pulls every minute).
pub const FREE_SLOT_GRACE_MS: i64 = 2 * 60_000;
/// "Runs at once" (Settings) full for this long is a finding.
pub const RUNS_FULL_MS: i64 = 60 * 60_000;
/// Failed checks in a row that pause the check.
pub const MAX_CHECK_FAILURES: i64 = 3;
/// Why a cancelled run ended when Gizai quit (the app's `STOPPED_BY_QUIT`).
const STOPPED_BY_QUIT: &str = "Stopped because Gizai quit.";

/// What the check needs to know beyond the database.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub now: i64,
    /// Agents whose pull is paused (after a start that couldn't work or a failed run), with why.
    pub pull_paused: HashMap<String, String>,
    /// Settings → agents paused.
    pub agents_paused: bool,
    /// Settings → Runs at once.
    pub max_concurrent: i64,
    /// The Team Lead: its own comments and changes don't make a finding new.
    pub lead_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastRun {
    pub id: String,
    pub agent: String,
    pub status: String,
    /// What started it: `nudge` is a Continue (a person's or the Team Lead's), `result_nudge` Gizai's own nudge after a
    /// run ended without its result line.
    pub trigger: String,
    pub outcome: Option<String>,
    pub error: Option<String>,
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// answered | held | waiting | stopped
    pub kind: String,
    /// Why, as a code. held: the hold. waiting: no_agents, testing_off, agents_paused, paused, budget, pull_paused,
    /// stopped_run, free_slot or runs_full. stopped: limit, failed, no_result, stopped (by a person) or quit.
    pub code: String,
    pub task_id: String,
    /// The card's identifier, like GA-12.
    pub task: String,
    pub title: String,
    pub column: String,
    /// Since when (Unix ms): the answer, the hold, the card's last change, the run's end.
    pub since: i64,
    /// The agent involved: the one that asked, ran or would start it.
    pub agent_id: Option<String>,
    pub agent: Option<String>,
    /// What is wrong, in a sentence.
    pub reason: String,
    pub hold: Option<String>,
    pub hold_reason: Option<String>,
    /// Answered: the person's comment.
    pub answer: Option<String>,
    pub last_run: Option<LastRun>,
    /// The card's last change not made by the Team Lead (`changes.seq`).
    pub stamp: i64,
    /// Held: the commands its last run asks the user to run ("Run this for me", GA-31; `Task::run_for_me`). The user
    /// runs them and presses Done, continue in the Inbox.
    #[serde(default)]
    pub run_for_me: Vec<String>,
}

impl Finding {
    /// The same finding in another check: the same card, kind and code.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.task_id, self.kind, self.code)
    }
}

/// A finding a check saw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    pub key: String,
    pub stamp: i64,
}

pub fn seen_of(findings: &[Finding]) -> Vec<Seen> {
    findings.iter().map(|f| Seen { key: f.key(), stamp: f.stamp }).collect()
}

struct Agent {
    name: String,
    status: String,
    max_runs: i64,
    over_budget: Option<String>,
    /// Its card runs at work now.
    busy: i64,
    /// When its last card run ended (a slot freed up).
    free_since: i64,
}

struct Card {
    id: String,
    identifier: String,
    title: String,
    column: String,
    category: String,
    hold: Option<String>,
    hold_reason: Option<String>,
    hold_at: i64,
    created_at: i64,
    run_for_me: Vec<String>,
}

fn usd(micros: i64) -> String {
    format!("${:.2}", micros as f64 / 1e6)
}

/// Everything on the board that needs attention now, answered cards first, then held, stopped and waiting ones.
pub fn check(db: &Db, cx: &Context) -> Result<Vec<Finding>> {
    let month = runs::month_start_ms(cx.now);
    let mut agents: HashMap<String, Agent> = HashMap::new();
    for (_, m) in team::all_agents(db)? {
        let over_budget = match m.budget_usd_micros {
            Some(b) => {
                let spent = runs::agent_spend_since(db, &m.actor_id, month)?;
                (spent >= b).then(|| format!("{} has used its monthly budget ({} of {})", m.name, usd(spent), usd(b)))
            }
            None => None,
        };
        agents.insert(m.actor_id.clone(), Agent {
            name: m.name, status: m.status, max_runs: m.max_runs.max(1), over_budget, busy: 0, free_since: 0,
        });
    }
    db.read(|c| {
        // Card runs at work, per agent and in all ("Runs at once"), and since when every slot has been taken.
        let mut st = c.prepare("SELECT agent_actor_id, count(*), max(COALESCE(started_at, created_at)) FROM runs
                                WHERE task_id IS NOT NULL AND status IN ('queued','running','waiting_approval') GROUP BY agent_actor_id")?;
        let (mut all, mut full_since) = (0i64, 0i64);
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))? {
            let (agent, n, latest) = row?;
            all += n;
            full_since = full_since.max(latest);
            if let Some(a) = agents.get_mut(&agent) {
                a.busy = n;
            }
        }
        let mut st = c.prepare("SELECT agent_actor_id, max(ended_at) FROM runs WHERE task_id IS NOT NULL AND ended_at IS NOT NULL GROUP BY agent_actor_id")?;
        for row in st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))? {
            let (agent, ended) = row?;
            if let Some(a) = agents.get_mut(&agent) {
                a.free_since = ended.unwrap_or(0);
            }
        }
        let runs_full = cx.max_concurrent > 0 && all >= cx.max_concurrent;

        let mut st = c.prepare(
            "SELECT t.id, t.identifier, t.title, s.name, s.category, t.hold, t.hold_reason, COALESCE(t.hold_at, t.updated_at), t.created_at,
                    CASE WHEN t.hold IS NOT NULL THEN (SELECT json_extract(r.outcome_json, '$.run_for_me') FROM runs r
                      WHERE r.task_id = t.id AND r.deleted_at IS NULL ORDER BY r.created_at DESC, r.id DESC LIMIT 1) END
             FROM tasks t JOIN workflow_states s ON s.id = t.state_id LEFT JOIN projects p ON p.id = t.project_id
             WHERE t.deleted_at IS NULL AND s.category IN ('ready','in_progress','testing')
               AND (p.id IS NULL OR (p.deleted_at IS NULL AND p.status NOT IN ('archived','done','paused')))
             ORDER BY s.sort_key, t.sort_key, t.created_at")?;
        let cards = st.query_map([], |r| Ok(Card {
            id: r.get(0)?, identifier: r.get(1)?, title: r.get(2)?, column: r.get(3)?, category: r.get(4)?, hold: r.get(5)?,
            hold_reason: r.get(6)?, hold_at: r.get(7)?, created_at: r.get(8)?, run_for_me: runs::commands_of(r.get(9)?),
        }))?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut out = vec![];
        for card in cards {
            let active: i64 = c.query_row("SELECT count(*) FROM runs WHERE task_id=?1 AND status IN ('queued','running','waiting_approval')",
                                          [&card.id], |r| r.get(0))?;
            if active > 0 {
                continue;
            }
            let (stamp, changed_at) = stamp(c, &card.id, cx.lead_id.as_deref())?;
            let last = last_run(c, &card.id)?;
            let base = |kind: &str, code: &str, since: i64, agent: Option<(String, String)>, reason: String| Finding {
                kind: kind.into(), code: code.into(), task_id: card.id.clone(), task: card.identifier.clone(), title: card.title.clone(),
                column: card.column.clone(), since, agent_id: agent.as_ref().map(|a| a.0.clone()), agent: agent.map(|a| a.1), reason,
                hold: card.hold.clone(), hold_reason: card.hold_reason.clone(), answer: None,
                last_run: last.as_ref().map(|(r, _)| r.clone()), stamp, run_for_me: card.run_for_me.clone(),
            };
            let last_agent = last.as_ref().map(|(r, a)| (a.clone(), r.agent.clone()));

            if let Some(hold) = &card.hold {
                // A person's comment after a "needs a decision" hold may answer it.
                let answer: Option<(String, i64)> = if hold == "needs_decision" {
                    c.query_row(
                        "SELECT c.body_md, c.created_at FROM comments c JOIN actors a ON a.id = c.author_actor_id
                         WHERE c.task_id=?1 AND c.deleted_at IS NULL AND a.kind='person' AND c.created_at > ?2
                         ORDER BY c.created_at DESC, c.rowid DESC LIMIT 1",
                        rusqlite::params![card.id, card.hold_at], |r| Ok((r.get(0)?, r.get(1)?))).optional()?
                } else {
                    None
                };
                let f = match answer {
                    Some((body, at)) => Finding {
                        answer: Some(body),
                        ..base("answered", "answered", at, last_agent, "A person answered after the card was put on hold".into())
                    },
                    None => {
                        let why = card.hold_reason.clone().filter(|r| !r.trim().is_empty()).unwrap_or_else(|| "no reason given".into());
                        base("held", hold, card.hold_at, last_agent, format!("On hold ({hold}): {why}"))
                    }
                };
                out.push(f);
                continue;
            }

            // In progress with a run that ended part-way.
            if card.category == "in_progress" && let Some((r, agent_id)) = &last {
                let stopped = match (r.status.as_str(), r.outcome.as_deref()) {
                    ("timed_out", _) => Some(("limit", format!("Its run stopped at a limit: {}", r.error.clone().unwrap_or_default()))),
                    ("cancelled", _) if r.error.as_deref() == Some(STOPPED_BY_QUIT) => Some(("quit", "Its run stopped because Gizai quit".to_string())),
                    ("cancelled", _) => Some(("stopped", "A person stopped its run".to_string())),
                    ("failed", _) => Some(("failed", format!("Its run failed: {}", r.error.clone().unwrap_or_else(|| "no error saved".into())))),
                    ("succeeded", Some("no_result")) => Some(("no_result", "Its run ended without a result".to_string())),
                    _ => None,
                };
                if let Some((code, reason)) = stopped {
                    out.push(base("stopped", code, r.ended_at.unwrap_or(changed_at), Some((agent_id.clone(), r.agent.clone())), reason));
                    continue;
                }
            }

            // Waiting: no agent will start it.
            let since = changed_at.max(card.created_at);
            if let Some((r, agent_id)) = &last && r.status == "cancelled" {
                let reason = if r.error.as_deref() == Some(STOPPED_BY_QUIT) { "Its last run stopped because Gizai quit" } else { "A person stopped its last run" };
                out.push(base("waiting", "stopped_run", since, Some((agent_id.clone(), r.agent.clone())),
                              format!("{reason}: the queue doesn't start it again until Run or Continue")));
                continue;
            }
            let route = workflow::route(c, &card.id)?;
            if !route.auto {
                // A Manual column: only a person's Run starts its cards.
                continue;
            }
            if route.testing_off {
                out.push(base("waiting", "testing_off", since, None,
                              format!("Its Testing switch is off, so no QA run starts in {}: move it on, or turn Testing on", route.column)));
                continue;
            }
            if route.agents.is_empty() {
                let f = match &route.person {
                    Some(p) => {
                        let name: String = c.query_row("SELECT name FROM actors WHERE id=?1", [p], |r| r.get(0)).unwrap_or_default();
                        base("waiting", "no_agents", since, None,
                             format!("It is assigned to {name}, a person, and no agent is on {}: put one on it on the Team page", route.column))
                    }
                    None => base("waiting", "no_agents", since, None, format!("No agent is on {}: put one on it on the Team page", route.column)),
                };
                out.push(f);
                continue;
            }
            let mut first_blocker: Option<(String, String, String)> = None;
            let mut free: Option<(String, i64)> = None;
            let mut waits_only = false;
            for id in &route.agents {
                let Some(a) = agents.get(id) else { continue };
                let blocker = if cx.agents_paused {
                    Some(("agents_paused", "Agents are paused in Settings".to_string()))
                } else if a.status != "active" {
                    Some(("paused", format!("{} is paused", a.name)))
                } else if let Some(b) = &a.over_budget {
                    Some(("budget", b.clone()))
                } else if let Some(why) = cx.pull_paused.get(id) {
                    Some(("pull_paused", format!("{} stopped taking cards: {why}", a.name)))
                } else {
                    None
                };
                match blocker {
                    Some((code, reason)) => {
                        if first_blocker.is_none() {
                            first_blocker = Some((id.clone(), code.to_string(), reason));
                        }
                    }
                    None if a.busy >= a.max_runs => waits_only = true,
                    None => {
                        let grace = FREE_SLOT_GRACE_MS;
                        let free_for = cx.now - a.free_since.max(since);
                        if free_for > grace {
                            free.get_or_insert((id.clone(), free_for));
                        } else {
                            waits_only = true;
                        }
                    }
                }
            }
            let agent_of = |id: &str| Some((id.to_string(), agents.get(id).map(|a| a.name.clone()).unwrap_or_default()));
            if let Some((id, free_for)) = free {
                if runs_full {
                    if cx.now - full_since > RUNS_FULL_MS {
                        out.push(base("waiting", "runs_full", full_since, agent_of(&id),
                                      format!("\"Runs at once\" (Settings) has been full ({} runs) for over an hour", cx.max_concurrent)));
                    }
                } else {
                    let name = agents.get(&id).map(|a| a.name.clone()).unwrap_or_default();
                    out.push(base("waiting", "free_slot", since, agent_of(&id),
                                  format!("{name} has had a free slot for {} min and didn't take it", free_for / 60_000)));
                }
            } else if !waits_only && let Some((id, code, reason)) = first_blocker {
                out.push(base("waiting", &code, since, agent_of(&id), reason));
            } else if !waits_only {
                out.push(base("waiting", "no_agents", since, None, "The agent it is assigned or pinned to is no longer on the team".into()));
            }
        }
        let order = |k: &str| match k { "answered" => 0, "held" => 1, "stopped" => 2, _ => 3 };
        out.sort_by_key(|f| order(&f.kind));
        Ok(out)
    })
}

/// The card's last change not made by `lead` (its own row, its comments and its runs): (`changes.seq`, Unix ms).
fn stamp(c: &Connection, task_id: &str, lead: Option<&str>) -> Result<(i64, i64)> {
    let row: Option<(i64, String)> = c.query_row(
        "SELECT ch.seq, ch.hlc FROM changes ch
         WHERE (ch.row_id = ?1 OR ch.row_id IN (SELECT id FROM comments WHERE task_id = ?1) OR ch.row_id IN (SELECT id FROM runs WHERE task_id = ?1))
           AND (?2 IS NULL OR ch.actor_id IS NULL OR ch.actor_id <> ?2)
         ORDER BY ch.seq DESC LIMIT 1",
        rusqlite::params![task_id, lead], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    Ok(match row {
        Some((seq, hlc)) => (seq, hlc.split('-').next().and_then(|s| s.parse().ok()).unwrap_or(0)),
        None => (0, 0),
    })
}

/// The card's latest run and its agent's id.
fn last_run(c: &Connection, task_id: &str) -> Result<Option<(LastRun, String)>> {
    Ok(c.query_row(
        "SELECT r.id, a.name, r.status, r.outcome, r.error, r.ended_at, r.agent_actor_id, r.trigger, r.nudged FROM runs r
         JOIN actors a ON a.id = r.agent_actor_id
         WHERE r.task_id=?1 AND r.deleted_at IS NULL ORDER BY r.created_at DESC, r.id DESC LIMIT 1",
        [task_id],
        |r| Ok((LastRun { id: r.get(0)?, agent: r.get(1)?, status: r.get(2)?, trigger: runs::reported_trigger(r.get(7)?, r.get::<_, i64>(8)? != 0),
                          outcome: r.get(3)?, error: r.get(4)?, ended_at: r.get(5)? }, r.get(6)?)),
    ).optional()?)
}

/// What the agent's last finished check saw (its latest succeeded `board_check` run); a failed check doesn't count, so
/// its findings stay new.
pub fn seen(db: &Db, agent_id: &str) -> Result<Vec<Seen>> {
    db.read(|c| {
        let json: Option<Option<String>> = c.query_row(
            "SELECT findings_json FROM runs WHERE agent_actor_id=?1 AND trigger='board_check' AND status='succeeded'
             ORDER BY created_at DESC, rowid DESC LIMIT 1", [agent_id], |r| r.get(0)).optional()?;
        Ok(json.flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
    })
}

/// The findings the agent hasn't seen yet, or whose card changed since it saw them.
pub fn new_findings(db: &Db, agent_id: &str, findings: &[Finding]) -> Result<Vec<Finding>> {
    let seen = seen(db, agent_id)?;
    Ok(findings.iter().filter(|f| !seen.iter().any(|s| s.key == f.key() && s.stamp >= f.stamp)).cloned().collect())
}

/// Records a queued check of the Team Lead: no card and no chat, and every finding it sees.
pub fn create_run(db: &Db, agent_id: &str, session_id: &str, cwd: &str, log_path: &str, saw: &[Seen]) -> Result<String> {
    let json = serde_json::to_string(saw)?;
    db.write(Some(agent_id), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let (adapter, model): (String, Option<String>) = c.query_row(
            "SELECT adapter, model FROM agent_configs WHERE actor_id=?1", [agent_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?.ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;
        let id = ids::new_id();
        c.execute(
            "INSERT INTO runs(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, trigger, role_key, adapter, model,
                              status, cwd, worktree_path, session_id, log_path, findings_json)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?3, 'board_check', 'lead', ?5, ?6, 'queued', ?7, ?7, ?8, ?9, ?10)",
            rusqlite::params![id, now, agent_id, crate::util::org_id(c)?, adapter, model, cwd, session_id, log_path, json],
        )?;
        w.insert("runs", &id, serde_json::json!({"agent": agent_id, "trigger": "board_check", "findings": saw.len()}))?;
        Ok(id)
    })
}

/// Records the end of a check: its cost (it counts toward the agent's budget), its summary (its last message) and,
/// for the pause, whether it succeeded. Three failed checks in a row pause the check (`board_check_paused`) until the
/// agent is changed or resumed. Returns why it paused, when this check paused it.
#[allow(clippy::too_many_arguments)]
pub fn finish_run(db: &Db, run_id: &str, status: &str, cost_usd_micros: i64, input_tokens: i64, output_tokens: i64, error: Option<&str>,
                  summary: Option<&str>) -> Result<Option<String>> {
    if !["succeeded", "failed", "cancelled", "timed_out"].contains(&status) {
        return Err(Error::Invalid(format!("a run can't finish as {status}")));
    }
    db.write(None, |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let agent: String = c.query_row("SELECT agent_actor_id FROM runs WHERE id=?1 AND trigger='board_check'", [run_id], |r| r.get(0))
            .optional()?.ok_or_else(|| Error::NotFound(format!("board check {run_id}")))?;
        c.execute(
            "UPDATE runs SET status=?2, cost_usd_micros=?3, input_tokens=?4, output_tokens=?5, error=?6, summary_md=?7, ended_at=?8, updated_at=?8,
                    version=version+1 WHERE id=?1",
            rusqlite::params![run_id, status, cost_usd_micros, input_tokens, output_tokens, error, summary, now])?;
        w.update("runs", run_id, serde_json::json!({"status": status, "cost_usd_micros": cost_usd_micros}))?;
        // A check stopped because Gizai quit is no failure, and no success either: its findings stay new.
        let paused = match status {
            "succeeded" => { c.execute("UPDATE agent_configs SET board_check_failures=0 WHERE actor_id=?1", [&agent])?; None }
            "cancelled" => None,
            _ => {
                let n: i64 = c.query_row("UPDATE agent_configs SET board_check_failures=board_check_failures+1 WHERE actor_id=?1 RETURNING board_check_failures",
                                         [&agent], |r| r.get(0))?;
                if n >= MAX_CHECK_FAILURES {
                    let why = format!("{MAX_CHECK_FAILURES} board checks in a row failed. The last one: {}",
                                      error.map(str::trim).filter(|e| !e.is_empty()).unwrap_or("no error saved"));
                    c.execute("UPDATE agent_configs SET board_check_paused=?2 WHERE actor_id=?1 AND board_check_paused IS NULL", rusqlite::params![agent, why])?;
                    w.update("agent_configs", &agent, serde_json::json!({"board_check_paused": why}))?;
                    Some(why)
                } else {
                    None
                }
            }
        };
        Ok(paused)
    })
}

/// When the Team Lead last looked at the board (the interval counts from here).
pub fn touch(db: &Db, agent_id: &str, at: i64) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE agent_configs SET board_checked_at=?2 WHERE actor_id=?1", rusqlite::params![agent_id, at])?;
        Ok(())
    })
}
