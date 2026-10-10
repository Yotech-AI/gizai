//! The Software-team workflow: who picks a card up (its column: the agents on it, Auto or Manual), what a run's verdict
//! does to the card (the gates, through the column's next column), and what an agent should work on next (the queue).
//! Labels never route. A card assigned to an agent is started only by that agent. Nothing starts by itself in a Manual
//! column; only a person's Run does.
use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, Writer};
use crate::model::Outcome;
use crate::{Error, Result, columns, comments, ids, runs, tasks};

/// "1. Pin overlaps" / "2) x" / "3.Bad" → the text without its list marker; "404 on /login" and
/// "3.5 s slow" keep their numbers.
fn strip_list_marker(s: &str) -> &str {
    let t = s.trim();
    let digits = t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 { return t; }
    let rest = &t[digits..];
    match rest.chars().next() {
        Some('.') | Some(')') if !rest[1..].starts_with(|c: char| c.is_ascii_digit()) => rest[1..].trim_start(),
        _ => t,
    }
}

pub const MAX_BOUNCES: i64 = 3;
pub const MAX_FAILS: i64 = 3;
/// The hold's reason when Gizai's nudge (GA-54) also ended without a result.
pub const NUDGE_STALLED: &str = "Its run ended without a GIZAI_RESULT line twice: Gizai continued it once, and it ended without one again. \
Its last message is in the Runs tab.";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    /// The column the card moved to.
    pub moved_to: Option<String>,
    /// The hold the card got.
    pub hold: Option<String>,
    /// The Team Lead took the card's question (GA-70, `questions::hand_over_in`): its id. The card stays on hold, out of
    /// the Inbox, while the Team Lead answers it or asks you.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<String>,
}

struct TaskRow {
    team_id: String,
    state_id: String,
    column: String,
    category: String,
    /// Its column is Auto.
    auto: bool,
    /// Its column's next column.
    next: Option<String>,
    pinned: Option<String>,
    implementer: Option<String>,
    /// Its assignee when that is an agent.
    agent_assignee: Option<String>,
    /// Its assignee when that is a person.
    person_assignee: Option<String>,
    owner_person: Option<String>,
    created_by: Option<String>,
    bounce_count: i64,
    fail_count: i64,
    /// The Testing switch: off, the card skips Testing columns.
    testing: bool,
}

fn task_row(c: &Connection, task_id: &str) -> Result<TaskRow> {
    c.query_row(
        "SELECT s.team_id, t.state_id, s.name, s.category, s.auto, s.next_state_id, t.pinned_actor_id, t.implementer_actor_id,
                CASE WHEN x.kind = 'agent' THEN x.id END, CASE WHEN x.kind = 'person' THEN x.id END,
                t.owner_person_id, t.created_by, t.bounce_count, t.fail_count, t.testing
         FROM tasks t JOIN workflow_states s ON s.id = t.state_id
         LEFT JOIN actors x ON x.id = t.assignee_actor_id AND x.deleted_at IS NULL
         WHERE t.id=?1 AND t.deleted_at IS NULL",
        [task_id],
        |r| Ok(TaskRow { team_id: r.get(0)?, state_id: r.get(1)?, column: r.get(2)?, category: r.get(3)?, auto: r.get::<_, i64>(4)? != 0,
                         next: r.get(5)?, pinned: r.get(6)?, implementer: r.get(7)?, agent_assignee: r.get(8)?,
                         person_assignee: r.get(9)?, owner_person: r.get(10)?, created_by: r.get(11)?, bounce_count: r.get(12)?,
                         fail_count: r.get(13)?, testing: r.get::<_, i64>(14)? != 0 }),
    ).optional()?.ok_or_else(|| Error::NotFound(format!("task {task_id}")))
}

/// The agent's role in its team, if it is an agent that isn't archived or deleted.
fn role_of(c: &Connection, actor_id: &str) -> Result<Option<String>> {
    Ok(c.query_row(
        "SELECT m.role_key FROM team_members m JOIN actors a ON a.id = m.actor_id
         WHERE m.actor_id=?1 AND m.deleted_at IS NULL AND a.kind='agent' AND a.status <> 'archived' AND a.deleted_at IS NULL
         ORDER BY m.created_at LIMIT 1",
        [actor_id], |r| r.get(0)).optional()?)
}

fn is_active(c: &Connection, actor_id: &str) -> Result<bool> {
    Ok(c.query_row("SELECT count(*) FROM actors WHERE id=?1 AND kind='agent' AND status='active' AND deleted_at IS NULL",
                   [actor_id], |r| r.get::<_, i64>(0))? > 0)
}

fn is_idle(c: &Connection, actor_id: &str) -> Result<bool> {
    let busy: i64 = c.query_row(
        "SELECT count(*) FROM runs WHERE agent_actor_id=?1 AND status IN ('queued','running','waiting_approval')", [actor_id], |r| r.get(0))?;
    let max: i64 = c.query_row("SELECT max_concurrent_runs FROM agent_configs WHERE actor_id=?1", [actor_id], |r| r.get(0)).optional()?.unwrap_or(1);
    Ok(busy < max.max(1))
}

/// The column a start moves a card to: a To do-type column's next column; from Backlog (a person's Run), the team's
/// first In progress column, as 0.2.0 did. None: the card stays.
fn start_target(c: &Connection, state_id: &str) -> Result<Option<String>> {
    let Ok(col) = columns::col(c, state_id) else { return Ok(None) };
    Ok(match col.category.as_str() {
        "ready" => col.next,
        "backlog" => columns::first_of(c, &col.team_id, "in_progress")?.map(|(id, _)| id),
        _ => None,
    })
}

/// Who Run starts on a card when the person picks nobody: the agent it is pinned or assigned to, else the first agent on
/// its column (an idle, active one first). A Backlog card has no agents of its own: the agents of the column Run moves
/// it to. None: nobody (the message says to put an agent on the column).
pub fn run_agent(db: &Db, task_id: &str) -> Result<Option<(String, String)>> {
    db.read(|c| {
        let t = task_row(c, task_id)?;
        for a in t.pinned.iter().chain(t.agent_assignee.iter()) {
            if let Some(role) = role_of(c, a)? {
                return Ok(Some((a.clone(), role)));
            }
        }
        let mut agents = columns::agents_of(c, &t.state_id)?;
        if agents.is_empty() && t.category == "backlog" && let Some(to) = start_target(c, &t.state_id)? {
            agents = columns::agents_of(c, &to)?;
        }
        let mut best: Option<String> = None;
        for a in &agents {
            if is_active(c, a)? && is_idle(c, a)? {
                best = Some(a.clone());
                break;
            }
        }
        let best = best.or_else(|| agents.first().cloned());
        Ok(match best {
            Some(a) => role_of(c, &a)?.map(|role| (a, role)),
            None => None,
        })
    })
}

/// The name of a card's column (for messages).
pub fn column_name(db: &Db, task_id: &str) -> Result<String> {
    db.read(|c| Ok(task_row(c, task_id)?.column))
}

/// The card this agent should work on next (the Agent page), if it is idle: the first of `waiting_for`.
pub fn next_task_for(db: &Db, agent_id: &str) -> Result<Option<String>> {
    let idle = db.read(|c| is_idle(c, agent_id))?;
    if !idle {
        return Ok(None);
    }
    Ok(waiting_for(db, agent_id)?.into_iter().next())
}

/// The cards this active agent may start now, best first (the queue). Only cards in Auto columns: those assigned or
/// pinned to it, wherever they are, and the cards of the columns it is on that aren't assigned or pinned to another
/// agent (a person as assignee doesn't block). Held, claimed and paused-project cards are skipped, and so is a card
/// whose last run was stopped (by a person, or because Gizai quit: Run or Continue starts it again) and a card with
/// its Testing switch off in a Testing column. Best means: priority urgent → low with none last, then cards assigned to
/// this agent, then column and card order.
pub fn waiting_for(db: &Db, agent_id: &str) -> Result<Vec<String>> {
    db.read(|c| {
        if !is_active(c, agent_id)? {
            return Ok(vec![]);
        }
        let mut st = c.prepare(
            "SELECT t.id FROM tasks t JOIN workflow_states s ON s.id = t.state_id
             LEFT JOIN projects p ON p.id = t.project_id
             LEFT JOIN actors x ON x.id = t.assignee_actor_id AND x.deleted_at IS NULL
             WHERE t.deleted_at IS NULL AND t.hold IS NULL AND s.deleted_at IS NULL AND s.auto = 1
               AND s.category NOT IN ('backlog','review','done','cancelled')
               AND NOT (s.category = 'testing' AND t.testing = 0)
               AND (p.id IS NULL OR p.status NOT IN ('archived','done','paused'))
               AND (t.claimed_by_run_id IS NULL OR t.lease_expires_at IS NULL OR t.lease_expires_at < ?2
                    OR NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = t.claimed_by_run_id AND r.status IN ('queued','running','waiting_approval')))
               AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.task_id = t.id AND r.status = 'cancelled'
                               AND r.created_at = (SELECT max(created_at) FROM runs WHERE task_id = t.id))
               AND (
                 t.pinned_actor_id = ?1
                 OR (t.pinned_actor_id IS NULL AND x.kind = 'agent' AND x.id = ?1)
                 OR (t.pinned_actor_id IS NULL AND (x.id IS NULL OR x.kind = 'person')
                     AND EXISTS (SELECT 1 FROM column_agents ca WHERE ca.state_id = s.id AND ca.actor_id = ?1))
               )
             ORDER BY CASE WHEN t.priority BETWEEN 1 AND 4 THEN t.priority ELSE 5 END,
                      CASE WHEN t.assignee_actor_id = ?1 OR t.pinned_actor_id = ?1 THEN 0 ELSE 1 END, s.sort_key, t.sort_key, t.created_at")?;
        Ok(st.query_map(rusqlite::params![agent_id, ids::now_ms()], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// Who would start a card (the board check): the agents that could take it (the pinned agent, else the agent it is
/// assigned to, else the agents on its column when that is Auto, its builder first; paused ones included, so the check
/// can say why the card waits), the person it is assigned to, if any, and its column.
pub(crate) struct Route {
    pub agents: Vec<String>,
    pub person: Option<String>,
    pub column: String,
    /// Its column is Auto.
    pub auto: bool,
    /// Its Testing switch is off and it is in a Testing column: no QA run starts.
    pub testing_off: bool,
}

pub(crate) fn route(c: &Connection, task_id: &str) -> Result<Route> {
    let t = task_row(c, task_id)?;
    let base = Route { agents: vec![], person: t.person_assignee.clone(), column: t.column.clone(), auto: t.auto,
                       testing_off: t.category == "testing" && !t.testing };
    if let Some(a) = t.pinned.clone().or(t.agent_assignee.clone()) {
        return Ok(Route { agents: vec![a], ..base });
    }
    if !t.auto {
        return Ok(base);
    }
    let mut agents = columns::agents_of(c, &t.state_id)?;
    if let Some(i) = t.implementer.as_ref().and_then(|imp| agents.iter().position(|a| a == imp)) {
        let imp = agents.remove(i);
        agents.insert(0, imp);
    }
    Ok(Route { agents, ..base })
}

/// A run has really started on the card: a card in a To do-type column moves to that column's next column at once (from
/// Backlog, when a person pressed Run on it, to the first In progress column), as the agent. Cards in other columns
/// stay while the agent works. Returns the column it came from, so a run that can't start working can put it back.
pub fn move_on_start(db: &Db, agent_id: &str, task_id: &str) -> Result<Option<String>> {
    db.write(Some(agent_id), |w| {
        let t = task_row(w.conn(), task_id)?;
        let Some(to) = start_target(w.conn(), &t.state_id)? else { return Ok(None) };
        if to == t.state_id {
            return Ok(None);
        }
        tasks::move_in(w, agent_id, task_id, &to, None)?;
        Ok(Some(t.state_id))
    })
}

/// A run couldn't start working (a missing login, …): the card goes back to the column it came from (`move_on_start`),
/// unless someone moved it in the meantime.
pub fn put_back(db: &Db, agent_id: &str, task_id: &str, state_id: &str) -> Result<()> {
    db.write(Some(agent_id), |w| {
        let t = task_row(w.conn(), task_id)?;
        if t.state_id != state_id && start_target(w.conn(), state_id)?.as_deref() == Some(t.state_id.as_str()) {
            tasks::move_in(w, agent_id, task_id, state_id, None)?;
        }
        Ok(())
    })
}

/// A run that couldn't start working: it isn't a failed run. Puts the card on hold "blocked" with the error, records
/// the run as without a result and releases its claim; `fail_count` stays as it was.
pub fn hold_unstarted(db: &Db, run_id: &str, reason: &str) -> Result<()> {
    unstarted(db, run_id, Some(reason))
}

/// The same without the hold: the card only waits again. For a card the queue started before another card's failure
/// paused its agent: one failure holds one card.
pub fn release_unstarted(db: &Db, run_id: &str) -> Result<()> {
    unstarted(db, run_id, None)
}

fn unstarted(db: &Db, run_id: &str, hold: Option<&str>) -> Result<()> {
    let run = runs::get(db, run_id)?;
    let task_id = run.task_id.clone().ok_or_else(|| Error::Invalid("this run has no task".into()))?;
    db.write(Some(&run.agent_id), |w| {
        w.conn().execute("UPDATE runs SET outcome=COALESCE(outcome, 'no_result'), updated_at=?2 WHERE id=?1", rusqlite::params![run_id, ids::now_ms()])?;
        runs::release_claim(w, run_id)?;
        match hold {
            Some(reason) => set_hold(w, &task_id, "blocked", reason),
            None => Ok(()),
        }
    })
}

/// A column's (id, name, category), if it isn't removed.
fn column_info(c: &Connection, state_id: &str) -> Result<Option<(String, String, String)>> {
    Ok(c.query_row("SELECT id, name, category FROM workflow_states WHERE id=?1 AND deleted_at IS NULL", [state_id],
                   |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?)
}

/// Where a done answer sends a card: its column's next column, past Testing columns when its Testing switch is off.
fn onward(c: &Connection, t: &TaskRow) -> Result<Option<(String, String, String)>> {
    let mut next = t.next.clone();
    let mut seen = HashSet::new();
    while let Some(id) = next {
        if !seen.insert(id.clone()) {
            return Ok(None);
        }
        let Some((name, category, after)): Option<(String, String, Option<String>)> = c.query_row(
            "SELECT name, category, next_state_id FROM workflow_states WHERE id=?1 AND deleted_at IS NULL", [&id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()? else { return Ok(None) };
        if category == "testing" && !t.testing {
            next = after;
            continue;
        }
        return Ok(Some((id, name, category)));
    }
    Ok(None)
}

/// The column a card that failed its test goes back to: the column it came from (the column before in its history,
/// when that is a column agents work), else the first column linked to this one, else the first In progress column.
fn came_from(c: &Connection, task_id: &str, t: &TaskRow) -> Result<Option<(String, String, String)>> {
    let mut st = c.prepare("SELECT diff_json FROM changes WHERE table_name='tasks' AND row_id=?1 AND diff_json LIKE '%\"column\":%' ORDER BY seq DESC")?;
    let diffs = st.query_map([task_id], |r| r.get::<_, Option<String>>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for d in diffs.into_iter().flatten() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&d) else { continue };
        let Some([from, to]) = v.get("column").and_then(|x| x.as_array()).and_then(|a| <&[serde_json::Value; 2]>::try_from(a.as_slice()).ok()) else { continue };
        if to.as_str() != Some(t.column.as_str()) {
            continue;
        }
        let from: Option<(String, String, String)> = c.query_row(
            "SELECT id, name, category FROM workflow_states WHERE team_id=?1 AND name=?2 AND deleted_at IS NULL AND id<>?3",
            rusqlite::params![t.team_id, from.as_str().unwrap_or_default(), t.state_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
        if let Some(f) = from.filter(|f| columns::takes_agents(&f.2)) {
            return Ok(Some(f));
        }
        break;
    }
    let linked: Option<(String, String, String)> = c.query_row(
        "SELECT id, name, category FROM workflow_states WHERE team_id=?1 AND next_state_id=?2 AND deleted_at IS NULL AND id<>?2 ORDER BY sort_key LIMIT 1",
        rusqlite::params![t.team_id, t.state_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    if linked.is_some() {
        return Ok(linked);
    }
    Ok(match columns::first_of(c, &t.team_id, "in_progress")? {
        Some((id, _)) => column_info(c, &id)?,
        None => None,
    })
}

fn allowed(role: &str, outcome: &str) -> bool {
    match role {
        "qa" => matches!(outcome, "qa_pass" | "qa_fail" | "needs_decision"),
        "lead" => outcome == "needs_decision",
        "devops" => matches!(outcome, "ready_for_testing" | "deployed" | "needs_decision"),
        _ => matches!(outcome, "ready_for_testing" | "needs_decision"),
    }
}

/// The person who reviews the card: its owner or creator, otherwise the first person.
fn reviewer(c: &Connection, t: &TaskRow) -> Result<Option<String>> {
    Ok(match t.owner_person.clone().or(t.created_by.clone()) {
        Some(p) => Some(p),
        None => c.query_row("SELECT id FROM actors WHERE kind='person' AND deleted_at IS NULL ORDER BY created_at LIMIT 1", [], |r| r.get(0)).optional()?,
    })
}

pub(crate) fn set_hold(w: &Writer, task_id: &str, hold: &str, reason: &str) -> Result<()> {
    w.conn().execute("UPDATE tasks SET hold=?2, hold_reason=?3, hold_at=?4, updated_at=?4, version=version+1 WHERE id=?1",
                     rusqlite::params![task_id, hold, reason, ids::now_ms()])?;
    w.update("tasks", task_id, serde_json::json!({"hold": hold, "holdReason": reason}))
}

fn set_fields(w: &Writer, task_id: &str, sql_set: &str, params: &[&dyn rusqlite::ToSql], diff: serde_json::Value) -> Result<()> {
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&task_id];
    all.extend_from_slice(params);
    w.conn().execute(&format!("UPDATE tasks SET {sql_set}, version=version+1 WHERE id=?1"), all.as_slice())?;
    w.update("tasks", task_id, diff)
}

/// Applies a finished run's verdict to its card, posts the agent's summary as a comment, records the outcome on the run
/// and releases the card's claim. The card's column decides where it goes:
/// - the role's done answer (`ready_for_testing` for builders, `qa_pass` for QA, `deployed` for DevOps on a Deploy card)
///   moves it to its column's next column, past Testing columns when its Testing switch is off; without a next column
///   it stays. A card a run moves into Review is assigned to the person who reviews; one it moves elsewhere loses its
///   agent assignee, so the next column's agents pick it up.
/// - `qa_fail` sends it back to the column it came from, to the agent that built it; three bounces hold it.
/// - `needs_decision` holds it where it is. The Team Lead looks at the question first when it can (GA-70,
///   `questions::hand_over_in`, then `GateResult::lead`): the card stays out of the Inbox until it answers or asks you.
/// Cards a person moved to Backlog, Done or Cancelled while the agent worked stay where they are, and a Deploy card
/// moves only on `deployed`. GA-32's DevOps rules hold: a DevOps run never sends a card to QA (`ready_for_testing`
/// outside Deploy ends in Review) and doesn't make the DevOps Agent the card's implementer, and `deployed` outside a
/// Deploy column holds the card. A builder's `ready_for_testing` on a card in Testing leaves it there for QA, and on a
/// card in Review it goes to Testing (with the switch on), as in 0.2.0.
pub fn apply_outcome(db: &Db, run_id: &str, outcome: Option<&Outcome>) -> Result<GateResult> {
    apply_outcome_with(db, run_id, outcome, &[])
}

/// `apply_outcome` for a result line that may also name `run_for_me` (GA-31): on a `needs_decision`, the commands the
/// agent asks you to run for it. They are kept with the verdict (`Run::run_for_me`), and the held card shows them
/// (`Task::run_for_me`) until a run continues it. On another outcome they are left out.
pub fn apply_outcome_with(db: &Db, run_id: &str, outcome: Option<&Outcome>, run_for_me: &[String]) -> Result<GateResult> {
    let run = runs::get(db, run_id)?;
    let task_id = run.task_id.clone().ok_or_else(|| Error::Invalid("this run has no task".into()))?;
    let agent = run.agent_id.clone();
    let role = run.role_key.clone().unwrap_or_default();
    db.write(Some(&agent), |w| {
        let c = w.conn();
        let t = task_row(c, &task_id)?;
        let mut g = GateResult::default();
        record_verdict(w, run_id, &agent, &task_id, outcome, run_for_me)?;

        let moved_by_hand = matches!(t.category.as_str(), "backlog" | "done" | "cancelled");
        // Moves the card to `to` (None: it stays), as the agent: into Review it is assigned to the person who reviews,
        // elsewhere it loses its agent assignee. `implementer` makes the agent the card's builder.
        let go = |to: Option<(String, String, String)>, implementer: bool, g: &mut GateResult| -> Result<()> {
            if implementer {
                set_fields(w, &task_id, "implementer_actor_id=?2", &[&agent], serde_json::json!({"implementer": agent}))?;
            }
            let Some((sid, name, category)) = to else { return Ok(()) };
            if sid != t.state_id {
                tasks::move_in(w, &agent, &task_id, &sid, None)?;
            }
            if category == "review" {
                let you = reviewer(c, &t)?;
                set_fields(w, &task_id, "assignee_actor_id=?2", &[&you], serde_json::json!({"assigneeId": you}))?;
            } else if sid != t.state_id && t.agent_assignee.is_some() {
                set_fields(w, &task_id, "assignee_actor_id=NULL", &[], serde_json::json!({"assigneeId": null}))?;
            }
            g.moved_to = Some(name);
            Ok(())
        };
        let first_review = || -> Result<Option<(String, String, String)>> {
            Ok(columns::first_of(c, &t.team_id, "review")?.map(|(id, name)| (id, name, "review".to_string())))
        };

        match outcome {
            Some(o) if !allowed(&role, &o.outcome) => {
                let reason = format!("The {role} agent answered {}, which its role can't use", o.outcome);
                set_hold(w, &task_id, "needs_decision", &reason)?;
                g.hold = Some("needs_decision".into());
            }
            Some(o) => match o.outcome.as_str() {
                "ready_for_testing" | "qa_pass" if moved_by_hand || t.category == "deploy" => {}
                // A DevOps run (merge conflicts, a release, …) never goes to QA: the card waits in Review for its person.
                "ready_for_testing" if role == "devops" => go(first_review()?, false, &mut g)?,
                // A builder on a card in Testing: QA tests it there.
                "ready_for_testing" if t.category == "testing" && t.testing => {
                    go(None, true, &mut g)?;
                    if t.agent_assignee.is_some() {
                        set_fields(w, &task_id, "assignee_actor_id=NULL", &[], serde_json::json!({"assigneeId": null}))?;
                    }
                }
                // A builder on a card in Review (Run by a person): QA tests it again, or it stays for your review.
                "ready_for_testing" if t.category == "review" => {
                    let testing = if t.testing { columns::first_of(c, &t.team_id, "testing")?.map(|(id, name)| (id, name, "testing".to_string())) } else { None };
                    match testing {
                        Some(to) => go(Some(to), true, &mut g)?,
                        None => go(Some((t.state_id.clone(), t.column.clone(), t.category.clone())), true, &mut g)?,
                    }
                }
                "ready_for_testing" => go(onward(c, &t)?, true, &mut g)?,
                "qa_pass" => go(onward(c, &t)?, false, &mut g)?,
                // Released or deployed on a Deploy card (a person pressed Run on it, or its column is Auto): on to its next column.
                "deployed" if t.category == "deploy" => go(onward(c, &t)?, false, &mut g)?,
                "deployed" if moved_by_hand => {}
                "deployed" => {
                    // Otherwise the agent assigned to the card would pick it up again and deploy twice.
                    set_hold(w, &task_id, "needs_decision", "The devops agent answered deployed on a card that isn't in Deploy")?;
                    g.hold = Some("needs_decision".into());
                }
                "qa_fail" => {
                    // A card a person moved to Backlog, Done or Cancelled stays, and so does a Deploy card (merged).
                    if !moved_by_hand && t.category != "deploy" {
                        let back = came_from(c, &task_id, &t)?;
                        if let Some((sid, name, _)) = back {
                            if sid != t.state_id {
                                tasks::move_in(w, &agent, &task_id, &sid, None)?;
                            }
                            g.moved_to = Some(name);
                        }
                    }
                    let bounces = t.bounce_count + 1;
                    set_fields(w, &task_id, "bounce_count=?2, assignee_actor_id=COALESCE(implementer_actor_id, assignee_actor_id)", &[&bounces],
                               serde_json::json!({"bounceCount": bounces}))?;
                    if bounces >= MAX_BOUNCES {
                        set_hold(w, &task_id, "needs_decision", &format!("{MAX_BOUNCES} QA bounces"))?;
                        g.hold = Some("needs_decision".into());
                    }
                }
                _ => {
                    let reason = match o.summary.trim() {
                        "" if !run_for_me.is_empty() => "The agent asks you to run commands for it".to_string(),
                        "" => "The agent needs a decision".to_string(),
                        s => s.to_string(),
                    };
                    set_hold(w, &task_id, "needs_decision", &reason)?;
                    g.hold = Some("needs_decision".into());
                    // GA-70: the Team Lead looks at the question first when it can; the card stays out of the Inbox meanwhile.
                    if o.outcome == "needs_decision" {
                        g.lead = crate::questions::hand_over_in(w, run_id, &task_id, &agent, &role, run_for_me)?;
                    }
                }
            },
            None => {
                let fails = t.fail_count + 1;
                set_fields(w, &task_id, "fail_count=?2", &[&fails], serde_json::json!({"failCount": fails}))?;
                // Gizai's nudge ended normally without a result too: no third run, the card waits for a person. A nudge
                // that failed or hit a limit counts as usual.
                let ended_normally = matches!(run.status.as_str(), "succeeded" | "queued" | "running" | "waiting_approval");
                if run.nudged && ended_normally {
                    set_hold(w, &task_id, "stalled", NUDGE_STALLED)?;
                    g.hold = Some("stalled".into());
                } else if fails >= MAX_FAILS {
                    let last: Option<String> = w.conn().query_row("SELECT error FROM runs WHERE id=?1", [run_id], |r| r.get(0)).ok().flatten();
                    let reason = match last.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
                        Some(e) => format!("{MAX_FAILS} runs ended without a result. The last one: {e}"),
                        None => format!("{MAX_FAILS} runs ended without a result"),
                    };
                    set_hold(w, &task_id, "stalled", &reason)?;
                    g.hold = Some("stalled".into());
                }
            }
        }
        Ok(g)
    })
}

/// Gizai couldn't push the card's branch after the run (GA-56); `reason` says why, in plain words. The run's verdict is
/// recorded and the agent's summary posted, as `apply_outcome` does, but the card stays where it is, whatever the verdict,
/// on hold "blocked" with the reason, so it shows in the Inbox: the next agent wouldn't find the branch. Nothing else
/// changes: no column, assignee, implementer, bounce or failure count.
pub fn hold_unpushed(db: &Db, run_id: &str, outcome: Option<&Outcome>, reason: &str) -> Result<GateResult> {
    hold_unpushed_with(db, run_id, outcome, &[], reason)
}

/// `hold_unpushed` for a result line that may also name `run_for_me` (see `apply_outcome_with`): the commands are kept
/// with the verdict as well, so the card held for the push shows them too.
pub fn hold_unpushed_with(db: &Db, run_id: &str, outcome: Option<&Outcome>, run_for_me: &[String], reason: &str) -> Result<GateResult> {
    let run = runs::get(db, run_id)?;
    let task_id = run.task_id.clone().ok_or_else(|| Error::Invalid("this run has no task".into()))?;
    db.write(Some(&run.agent_id), |w| {
        task_row(w.conn(), &task_id)?;
        record_verdict(w, run_id, &run.agent_id, &task_id, outcome, run_for_me)?;
        set_hold(w, &task_id, "blocked", reason)?;
        Ok(GateResult { moved_to: None, hold: Some("blocked".into()), lead: None })
    })
}

/// Records a finished run's verdict on the run (a run still marked active is finished here), releases the card's claim
/// and posts the agent's summary, with QA's issues, as a comment on the card. A `needs_decision` keeps the commands it
/// asks you to run (`run_for_me`) in its `outcome_json`.
fn record_verdict(w: &Writer, run_id: &str, agent: &str, task_id: &str, outcome: Option<&Outcome>, run_for_me: &[String]) -> Result<()> {
    let label = outcome.map(|o| o.outcome.clone()).unwrap_or_else(|| "no_result".into());
    let json = match outcome {
        Some(o) => {
            let mut v = serde_json::to_value(o)?;
            if o.outcome == "needs_decision" && !run_for_me.is_empty() {
                v["run_for_me"] = serde_json::json!(run_for_me);
            }
            Some(v.to_string())
        }
        None => None,
    };
    w.conn().execute(
        "UPDATE runs SET outcome=CASE WHEN ?2='no_result' AND outcome='error' THEN outcome ELSE ?2 END, outcome_json=?3, summary_md=?4,
                status=CASE WHEN status IN ('queued','running','waiting_approval') THEN 'succeeded' ELSE status END,
                ended_at=COALESCE(ended_at, ?5), updated_at=?5 WHERE id=?1",
        rusqlite::params![run_id, label, json, outcome.map(|o| o.summary.clone()), ids::now_ms()])?;
    runs::release_claim(w, run_id)?;
    if let Some(o) = outcome {
        let mut body = o.summary.trim().to_string();
        if !o.issues.is_empty() {
            if !body.is_empty() { body.push_str("\n\n"); }
            for (i, issue) in o.issues.iter().enumerate() {
                body.push_str(&format!("{}. {}\n", i + 1, strip_list_marker(issue)));
            }
        }
        if !body.trim().is_empty() {
            comments::add_in(w, agent, task_id, body.trim_end(), Some(run_id))?;
        }
    }
    Ok(())
}

/// A merged pull request: the column a card in Review goes to (Review's next column; without one, the team's first
/// Deploy column, else Done, as in 0.2.0) and a card elsewhere goes to (the first Deploy column, else Done). Cards in
/// Deploy, Done or Cancelled stay.
pub(crate) fn merge_target(c: &Connection, task_id: &str) -> Result<Option<(String, String)>> {
    let t = task_row(c, task_id)?;
    if matches!(t.category.as_str(), "deploy" | "done" | "cancelled") {
        return Ok(None);
    }
    if t.category == "review" && let Some(next) = t.next.as_deref() && let Some((id, name, _)) = column_info(c, next)? {
        return Ok(Some((id, name)));
    }
    Ok(c.query_row("SELECT id, name FROM workflow_states WHERE team_id=?1 AND category IN ('deploy','done') AND deleted_at IS NULL
                    ORDER BY category = 'done', sort_key LIMIT 1", [&t.team_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
}
