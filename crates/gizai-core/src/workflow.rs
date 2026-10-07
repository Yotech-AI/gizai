//! The Software-team workflow: who picks a card up (routing), what a run's verdict does to the card
//! (gates), and what an agent should work on next (heartbeats). Agents never move a card to Done.
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, Writer};
use crate::model::Outcome;
use crate::{Error, Result, comments, ids, runs, tasks};

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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    /// The column the card moved to.
    pub moved_to: Option<String>,
    /// The hold the card got.
    pub hold: Option<String>,
}

struct TaskRow {
    team_id: String,
    state_id: String,
    category: String,
    hold: Option<String>,
    pinned: Option<String>,
    implementer: Option<String>,
    owner_person: Option<String>,
    created_by: Option<String>,
    bounce_count: i64,
    fail_count: i64,
}

fn task_row(c: &Connection, task_id: &str) -> Result<TaskRow> {
    c.query_row(
        "SELECT s.team_id, t.state_id, s.category, t.hold, t.pinned_actor_id, t.implementer_actor_id, t.owner_person_id, t.created_by,
                t.bounce_count, t.fail_count
         FROM tasks t JOIN workflow_states s ON s.id = t.state_id WHERE t.id=?1 AND t.deleted_at IS NULL",
        [task_id],
        |r| Ok(TaskRow { team_id: r.get(0)?, state_id: r.get(1)?, category: r.get(2)?, hold: r.get(3)?, pinned: r.get(4)?,
                         implementer: r.get(5)?, owner_person: r.get(6)?, created_by: r.get(7)?, bounce_count: r.get(8)?, fail_count: r.get(9)? }),
    ).optional()?.ok_or_else(|| Error::NotFound(format!("task {task_id}")))
}

/// The agent's role in the team, if it is an active agent member.
fn agent_role(c: &Connection, team_id: &str, actor_id: &str) -> Result<Option<String>> {
    Ok(c.query_row(
        "SELECT m.role_key FROM team_members m JOIN actors a ON a.id = m.actor_id
         WHERE m.team_id=?1 AND m.actor_id=?2 AND m.deleted_at IS NULL AND a.kind='agent' AND a.status='active' AND a.deleted_at IS NULL",
        rusqlite::params![team_id, actor_id], |r| r.get(0)).optional()?)
}

fn is_idle(c: &Connection, actor_id: &str) -> Result<bool> {
    let busy: i64 = c.query_row(
        "SELECT count(*) FROM runs WHERE agent_actor_id=?1 AND status IN ('queued','running','waiting_approval')", [actor_id], |r| r.get(0))?;
    let max: i64 = c.query_row("SELECT max_concurrent_runs FROM agent_configs WHERE actor_id=?1", [actor_id], |r| r.get(0)).optional()?.unwrap_or(1);
    Ok(busy < max.max(1))
}

/// The role a card goes to now. Review, Done, Backlog and Cancelled cards go to nobody (Review is the human
/// gate). Otherwise a column rule on its column wins; in To do and In progress the label rule with the lowest
/// priority number comes next; in Testing the column's owner role (qa). Label rules never send a Testing card
/// back to its builder.
fn routed_role(c: &Connection, task_id: &str, t: &TaskRow) -> Result<Option<String>> {
    if !matches!(t.category.as_str(), "ready" | "in_progress" | "testing") {
        return Ok(None);
    }
    if let Some(role) = c.query_row(
        "SELECT target_role FROM routing_rules WHERE team_id=?1 AND kind='column' AND match_state_id=?2 AND enabled=1 AND deleted_at IS NULL
           AND target_role IS NOT NULL ORDER BY priority, created_at LIMIT 1",
        rusqlite::params![t.team_id, t.state_id], |r| r.get::<_, String>(0)).optional()? {
        return Ok(Some(role));
    }
    if t.category == "testing" {
        let owner: Option<String> = c.query_row("SELECT owner_role FROM workflow_states WHERE id=?1", [&t.state_id], |r| r.get(0))?;
        return Ok(owner.filter(|o| o != "implementer" && o != "human"));
    }
    Ok(c.query_row(
        "SELECT r.target_role FROM routing_rules r JOIN task_labels tl ON tl.label_id = r.match_label_id AND tl.task_id=?2
         WHERE r.team_id=?1 AND r.kind='label' AND r.enabled=1 AND r.deleted_at IS NULL AND r.target_role IS NOT NULL
         ORDER BY r.priority, r.created_at LIMIT 1",
        rusqlite::params![t.team_id, task_id], |r| r.get::<_, String>(0)).optional()?)
}

/// Who should work this card now: (agent, role). A pin wins; then the routed role, where a card that came
/// back to In progress goes to the agent that built it; otherwise the first idle agent with that role.
/// None when the card is on hold, nothing routes it, or every matching agent is busy.
pub fn pick_agent(db: &Db, task_id: &str) -> Result<Option<(String, String)>> {
    db.read(|c| {
        let t = task_row(c, task_id)?;
        if t.hold.is_some() || matches!(t.category.as_str(), "done" | "cancelled" | "backlog" | "review") {
            return Ok(None);
        }
        if let Some(pin) = &t.pinned {
            return Ok(agent_role(c, &t.team_id, pin)?.map(|role| (pin.clone(), role)));
        }
        let Some(role) = routed_role(c, task_id, &t)? else { return Ok(None) };
        if t.category == "in_progress" {
            if let Some(imp) = &t.implementer {
                if agent_role(c, &t.team_id, imp)?.is_some() && is_idle(c, imp)? {
                    return Ok(Some((imp.clone(), role)));
                }
            }
        }
        let mut st = c.prepare(
            "SELECT m.actor_id FROM team_members m JOIN actors a ON a.id = m.actor_id
             WHERE m.team_id=?1 AND m.role_key=?2 AND m.deleted_at IS NULL AND a.kind='agent' AND a.status='active' AND a.deleted_at IS NULL
             ORDER BY m.created_at, a.name")?;
        let candidates = st.query_map(rusqlite::params![t.team_id, role], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for a in candidates {
            if is_idle(c, &a)? {
                return Ok(Some((a, role)));
            }
        }
        Ok(None)
    })
}

/// For heartbeats: the card this agent should work on next, if it is idle. Candidates are cards assigned
/// to it, cards in a column its role owns (column rule or the column's owner role), and To do or In progress
/// cards whose label routes to its role. Held, claimed, pinned-to-someone-else and assigned-to-someone-else cards are
/// skipped. Assigned cards come first, then the topmost card (lowest sort key).
pub fn next_task_for(db: &Db, agent_id: &str) -> Result<Option<String>> {
    db.read(|c| {
        let Some((team_id, role)): Option<(String, String)> = c.query_row(
            "SELECT m.team_id, m.role_key FROM team_members m JOIN actors a ON a.id = m.actor_id
             WHERE m.actor_id=?1 AND m.deleted_at IS NULL AND a.kind='agent' AND a.status='active' AND a.deleted_at IS NULL
             ORDER BY m.created_at LIMIT 1",
            [agent_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()? else { return Ok(None) };
        if !is_idle(c, agent_id)? {
            return Ok(None);
        }
        let now = ids::now_ms();
        let mut st = c.prepare(
            "SELECT t.id, CASE WHEN t.assignee_actor_id = ?2 THEN 0 ELSE 1 END AS rank
             FROM tasks t JOIN workflow_states s ON s.id = t.state_id
             LEFT JOIN projects p ON p.id = t.project_id
             WHERE s.team_id = ?1 AND t.deleted_at IS NULL AND t.hold IS NULL
               AND s.category IN ('ready','in_progress','testing')
               AND (p.id IS NULL OR p.status NOT IN ('archived','done','paused'))
               AND (t.claimed_by_run_id IS NULL OR t.lease_expires_at IS NULL OR t.lease_expires_at < ?4
                    OR NOT EXISTS (SELECT 1 FROM runs r WHERE r.id = t.claimed_by_run_id AND r.status IN ('queued','running','waiting_approval')))
               AND (t.pinned_actor_id IS NULL OR t.pinned_actor_id = ?2)
               AND (t.assignee_actor_id IS NULL OR t.assignee_actor_id = ?2)
               AND (
                 t.assignee_actor_id = ?2 OR t.pinned_actor_id = ?2
                 OR s.owner_role = ?3
                 OR EXISTS (SELECT 1 FROM routing_rules r WHERE r.team_id = ?1 AND r.kind='column' AND r.match_state_id = s.id
                            AND r.target_role = ?3 AND r.enabled=1 AND r.deleted_at IS NULL)
                 OR (s.category IN ('ready','in_progress') AND EXISTS (
                      SELECT 1 FROM routing_rules r JOIN task_labels tl ON tl.label_id = r.match_label_id AND tl.task_id = t.id
                      WHERE r.team_id = ?1 AND r.kind='label' AND r.target_role = ?3 AND r.enabled=1 AND r.deleted_at IS NULL))
               )
             ORDER BY rank, t.sort_key, t.created_at")?;
        let ids = st.query_map(rusqlite::params![team_id, agent_id, role, now], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Same answer as routing would give: skip cards that route to another role or are pinned elsewhere.
        for id in ids {
            let t = task_row(c, &id)?;
            let routed = routed_role(c, &id, &t)?;
            let assigned_here = c.query_row("SELECT assignee_actor_id = ?2 OR pinned_actor_id = ?2 FROM tasks WHERE id=?1",
                rusqlite::params![id, agent_id], |r| r.get::<_, Option<bool>>(0))?.unwrap_or(false);
            if assigned_here || routed.as_deref() == Some(role.as_str()) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    })
}

fn column_of(c: &Connection, team_id: &str, category: &str) -> Result<Option<(String, String)>> {
    Ok(c.query_row(
        "SELECT id, name FROM workflow_states WHERE team_id=?1 AND category=?2 AND deleted_at IS NULL ORDER BY sort_key LIMIT 1",
        rusqlite::params![team_id, category], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
}

fn allowed(role: &str, outcome: &str) -> bool {
    match role {
        "qa" => matches!(outcome, "qa_pass" | "qa_fail" | "needs_decision"),
        "lead" => outcome == "needs_decision",
        _ => matches!(outcome, "ready_for_testing" | "needs_decision"),
    }
}

fn set_hold(w: &Writer, task_id: &str, hold: &str, reason: &str) -> Result<()> {
    w.conn().execute("UPDATE tasks SET hold=?2, hold_reason=?3, updated_at=?4, version=version+1 WHERE id=?1",
                     rusqlite::params![task_id, hold, reason, ids::now_ms()])?;
    w.update("tasks", task_id, serde_json::json!({"hold": hold, "holdReason": reason}))
}

fn set_fields(w: &Writer, task_id: &str, sql_set: &str, params: &[&dyn rusqlite::ToSql], diff: serde_json::Value) -> Result<()> {
    let mut all: Vec<&dyn rusqlite::ToSql> = vec![&task_id];
    all.extend_from_slice(params);
    w.conn().execute(&format!("UPDATE tasks SET {sql_set}, version=version+1 WHERE id=?1"), all.as_slice())?;
    w.update("tasks", task_id, diff)
}

/// Applies a finished run's verdict to its card, posts the agent's summary as a comment, records the
/// outcome on the run and releases the card's claim. Cards a person moved to Backlog, Done or Cancelled
/// while the agent worked stay where they are.
pub fn apply_outcome(db: &Db, run_id: &str, outcome: Option<&Outcome>) -> Result<GateResult> {
    let run = runs::get(db, run_id)?;
    let task_id = run.task_id.clone().ok_or_else(|| Error::Invalid("this run has no task".into()))?;
    let agent = run.agent_id.clone();
    let role = run.role_key.clone().unwrap_or_default();
    db.write(Some(&agent), |w| {
        let c = w.conn();
        let t = task_row(c, &task_id)?;
        let mut g = GateResult::default();

        // Record the verdict on the run; a run still marked active is finished here.
        let label = outcome.map(|o| o.outcome.clone()).unwrap_or_else(|| "no_result".into());
        c.execute(
            "UPDATE runs SET outcome=CASE WHEN ?2='no_result' AND outcome='error' THEN outcome ELSE ?2 END, outcome_json=?3, summary_md=?4,
                    status=CASE WHEN status IN ('queued','running','waiting_approval') THEN 'succeeded' ELSE status END,
                    ended_at=COALESCE(ended_at, ?5), updated_at=?5 WHERE id=?1",
            rusqlite::params![run_id, label, outcome.map(serde_json::to_string).transpose()?, outcome.map(|o| o.summary.clone()), ids::now_ms()])?;
        runs::release_claim(w, run_id)?;

        // The agent's summary (and QA's issues) as a comment on the card.
        if let Some(o) = outcome {
            let mut body = o.summary.trim().to_string();
            if !o.issues.is_empty() {
                if !body.is_empty() { body.push_str("\n\n"); }
                for (i, issue) in o.issues.iter().enumerate() {
                    body.push_str(&format!("{}. {}\n", i + 1, strip_list_marker(issue)));
                }
            }
            if !body.trim().is_empty() {
                comments::add_in(w, &agent, &task_id, body.trim_end(), Some(run_id))?;
            }
        }

        let moved_by_hand = matches!(t.category.as_str(), "backlog" | "done" | "cancelled");
        let move_to = |category: &str, g: &mut GateResult| -> Result<()> {
            if moved_by_hand { return Ok(()); }
            if let Some((sid, name)) = column_of(c, &t.team_id, category)? {
                if sid != t.state_id {
                    tasks::move_in(w, &agent, &task_id, &sid, None)?;
                }
                g.moved_to = Some(name);
            }
            Ok(())
        };

        match outcome {
            Some(o) if !allowed(&role, &o.outcome) => {
                let reason = format!("The {role} agent answered {}, which its role can't use", o.outcome);
                set_hold(w, &task_id, "needs_decision", &reason)?;
                g.hold = Some("needs_decision".into());
            }
            Some(o) => match o.outcome.as_str() {
                "ready_for_testing" => {
                    move_to("testing", &mut g)?;
                    if !moved_by_hand {
                        set_fields(w, &task_id, "implementer_actor_id=?2, assignee_actor_id=NULL", &[&agent], serde_json::json!({"implementer": agent}))?;
                    }
                }
                "qa_pass" => {
                    move_to("review", &mut g)?;
                    if !moved_by_hand {
                        let you: Option<String> = match t.owner_person.clone().or(t.created_by.clone()) {
                            Some(p) => Some(p),
                            None => c.query_row("SELECT id FROM actors WHERE kind='person' AND deleted_at IS NULL ORDER BY created_at LIMIT 1", [], |r| r.get(0)).optional()?,
                        };
                        set_fields(w, &task_id, "assignee_actor_id=?2", &[&you], serde_json::json!({"assigneeId": you}))?;
                    }
                }
                "qa_fail" => {
                    move_to("in_progress", &mut g)?;
                    let bounces = t.bounce_count + 1;
                    set_fields(w, &task_id, "bounce_count=?2, assignee_actor_id=COALESCE(implementer_actor_id, assignee_actor_id)", &[&bounces],
                               serde_json::json!({"bounceCount": bounces}))?;
                    if bounces >= MAX_BOUNCES {
                        set_hold(w, &task_id, "needs_decision", &format!("{MAX_BOUNCES} QA bounces"))?;
                        g.hold = Some("needs_decision".into());
                    }
                }
                _ => {
                    let reason = if o.summary.trim().is_empty() { "The agent needs a decision".to_string() } else { o.summary.trim().to_string() };
                    set_hold(w, &task_id, "needs_decision", &reason)?;
                    g.hold = Some("needs_decision".into());
                }
            },
            None => {
                let fails = t.fail_count + 1;
                set_fields(w, &task_id, "fail_count=?2", &[&fails], serde_json::json!({"failCount": fails}))?;
                if fails >= MAX_FAILS {
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
