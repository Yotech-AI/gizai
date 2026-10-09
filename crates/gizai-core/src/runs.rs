//! Agent runs: the bookkeeping around one agent CLI process (`claude`, `codex`, …) working one task.
use rusqlite::OptionalExtension;

use crate::db::{Db, Writer};
use crate::model::{DayStat, Outcome, Refusal, Run};
use crate::{Error, Result, ids, util};

/// A run claims its task for this long; a crashed Gizai can't block a task forever.
pub const LEASE_MS: i64 = 50 * 60 * 1000;
const ACTIVE: &str = "('queued','running','waiting_approval')";

const COLS: &str = "r.id, r.agent_actor_id, a.name, r.task_id, r.role_key, r.trigger, r.status, r.outcome, r.summary_md, r.created_at,
                    r.started_at, r.ended_at, COALESCE(r.cost_usd_micros,0), COALESCE(r.input_tokens,0), COALESCE(r.output_tokens,0),
                    r.branch, r.worktree_path, r.session_id, r.error, r.log_path, r.pid, r.base_sha, r.adapter, r.head_sha,
                    COALESCE((SELECT f.refused_json FROM run_refusals f WHERE f.run_id = r.id), '[]'), r.nudged,
                    json_extract(r.outcome_json, '$.run_for_me')";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Run> {
    let nudged = r.get::<_, i64>(25)? != 0;
    Ok(Run {
        id: r.get(0)?, agent_id: r.get(1)?, agent_name: r.get(2)?, task_id: r.get(3)?, role_key: r.get(4)?, trigger: reported_trigger(r.get(5)?, nudged),
        status: r.get(6)?, outcome: r.get(7)?, summary_md: r.get(8)?, created_at: r.get(9)?, started_at: r.get(10)?,
        ended_at: r.get(11)?, cost_usd_micros: r.get(12)?, input_tokens: r.get(13)?, output_tokens: r.get(14)?,
        branch: r.get(15)?, worktree_path: r.get(16)?, session_id: r.get(17)?, error: r.get(18)?, log_path: r.get(19)?, pid: r.get(20)?,
        base_sha: r.get(21)?, adapter: r.get(22)?, head_sha: r.get(23)?,
        refused: serde_json::from_str(&r.get::<_, String>(24)?).unwrap_or_default(),
        nudged,
        run_for_me: commands_of(r.get(26)?),
    })
}

/// The trigger a run reports (`Run::trigger`): Gizai's nudge is stored as `nudge` with `nudged` set, and reported as
/// `result_nudge` (`RESULT_NUDGE`), apart from a Continue. Older nudges (GA-54) read the same way.
pub(crate) fn reported_trigger(stored: String, nudged: bool) -> String {
    if nudged && stored == "nudge" { RESULT_NUDGE.to_string() } else { stored }
}

/// The commands of a verdict's `run_for_me` (`outcome_json`, see `workflow::apply_outcome_with`), as JSON text; none
/// when it has none or isn't a list of strings.
pub(crate) fn commands_of(json: Option<String>) -> Vec<String> {
    json.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()).unwrap_or_default()
}

/// A routed run (the dispatcher picked the agent). See `create_with_trigger`.
#[allow(clippy::too_many_arguments)]
pub fn create(db: &Db, agent_id: &str, task_id: &str, role_key: &str, session_id: &str, cwd: &str, worktree: &str, branch: &str, log_path: &str) -> Result<String> {
    create_with_trigger(db, agent_id, task_id, role_key, "routed", session_id, cwd, worktree, branch, log_path)
}

/// Records a queued run and claims the task for it in the same write. Fails when another run holds an
/// unexpired claim on the task.
#[allow(clippy::too_many_arguments)]
pub fn create_with_trigger(db: &Db, agent_id: &str, task_id: &str, role_key: &str, trigger: &str, session_id: &str, cwd: &str,
                           worktree: &str, branch: &str, log_path: &str) -> Result<String> {
    create_run(db, agent_id, task_id, role_key, trigger, false, session_id, cwd, worktree, branch, log_path)
}

/// The trigger of Gizai's nudge (GA-31), so the Runs list tells it from a Continue (`nudge`): what `Run::trigger` says
/// for it. In the database it is a `nudge` with `nudged` set (`reported_trigger`): the runs table's CHECK takes no new
/// trigger without rebuilding the whole table, and `nudged` already says it.
pub const RESULT_NUDGE: &str = "result_nudge";

/// Gizai's nudge (GA-54): a run that continues, by itself, a run that ended without a result, in the same session.
/// Recorded like `create_with_trigger` with trigger `nudge` and marked `nudged`, so it is never nudged in turn: when it
/// ends without a result too, the card goes on hold (`workflow::apply_outcome`). It reads as trigger `result_nudge`
/// (`RESULT_NUDGE`); a person's or the Team Lead's Continue reads as `nudge`.
#[allow(clippy::too_many_arguments)]
pub fn create_nudge(db: &Db, agent_id: &str, task_id: &str, role_key: &str, session_id: &str, cwd: &str, worktree: &str, branch: &str,
                    log_path: &str) -> Result<String> {
    create_run(db, agent_id, task_id, role_key, "nudge", true, session_id, cwd, worktree, branch, log_path)
}

#[allow(clippy::too_many_arguments)]
fn create_run(db: &Db, agent_id: &str, task_id: &str, role_key: &str, trigger: &str, nudged: bool, session_id: &str, cwd: &str,
              worktree: &str, branch: &str, log_path: &str) -> Result<String> {
    db.write(Some(agent_id), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        crate::tasks::not_archived(c, task_id)?;
        let claim: Option<(Option<String>, Option<i64>)> = c.query_row(
            "SELECT claimed_by_run_id, lease_expires_at FROM tasks WHERE id=?1 AND deleted_at IS NULL", [task_id],
            |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        let Some((claimed, lease)) = claim else { return Err(Error::NotFound(format!("task {task_id}"))) };
        if let Some(other) = claimed {
            let active: i64 = c.query_row(&format!("SELECT count(*) FROM runs WHERE id=?1 AND status IN {ACTIVE}"), [&other], |r| r.get(0))?;
            if active > 0 && lease.unwrap_or(0) > now {
                return Err(Error::Invalid("task already has an active run".into()));
            }
        }
        let (adapter, model): (String, Option<String>) = c.query_row(
            "SELECT adapter, model FROM agent_configs WHERE actor_id=?1", [agent_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?.ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;
        let id = ids::new_id();
        c.execute(
            "INSERT INTO runs(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, task_id, trigger, role_key, adapter, model,
                              status, cwd, worktree_path, branch, session_id, log_path, nudged)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?3, ?5, ?6, ?7, ?8, ?9, 'queued', ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![id, now, agent_id, util::org_id(c)?, task_id, trigger, role_key, adapter, model, cwd, worktree, branch, session_id, log_path,
                              nudged as i64],
        )?;
        c.execute("UPDATE tasks SET claimed_by_run_id=?2, lease_expires_at=?3, branch=COALESCE(branch, ?4) WHERE id=?1",
                  rusqlite::params![task_id, id, now + LEASE_MS, branch])?;
        let mut diff = serde_json::json!({"task_id": task_id, "agent": agent_id, "role": role_key, "trigger": trigger});
        if nudged {
            diff["nudged"] = serde_json::json!(true);
        }
        w.insert("runs", &id, diff)?;
        Ok(id)
    })
}

/// Records a queued chat turn of the Team Lead: no task, the thread instead. It runs on the agent's own CLI.
pub fn create_chat(db: &Db, agent_id: &str, thread_id: &str, session_id: &str, cwd: &str, log_path: &str) -> Result<String> {
    create_chat_on(db, agent_id, thread_id, None, session_id, cwd, log_path)
}

/// `create_chat` for a turn on `cli` (the chat's Runs on, a coding CLI's id), recorded as the run's adapter; None: the
/// agent's own CLI.
pub fn create_chat_on(db: &Db, agent_id: &str, thread_id: &str, cli: Option<&str>, session_id: &str, cwd: &str, log_path: &str) -> Result<String> {
    db.write(Some(agent_id), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let (adapter, model): (String, Option<String>) = c.query_row(
            "SELECT adapter, model FROM agent_configs WHERE actor_id=?1", [agent_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?.ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;
        let adapter = cli.map(str::to_string).unwrap_or(adapter);
        let id = ids::new_id();
        c.execute(
            "INSERT INTO runs(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, chat_thread_id, trigger, role_key, adapter, model,
                              status, cwd, worktree_path, session_id, log_path)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?3, ?5, 'chat', 'lead', ?6, ?7, 'queued', ?8, ?8, ?9, ?10)",
            rusqlite::params![id, now, agent_id, util::org_id(c)?, thread_id, adapter, model, cwd, session_id, log_path],
        )?;
        w.insert("runs", &id, serde_json::json!({"chat": thread_id, "agent": agent_id, "trigger": "chat"}))?;
        Ok(id)
    })
}

/// Records the end of a chat turn: like `finish`, but a chat turn has no task outcome.
pub fn finish_chat(db: &Db, run_id: &str, status: &str, cost_usd_micros: i64, input_tokens: i64, output_tokens: i64, error: Option<&str>) -> Result<()> {
    if !["succeeded", "failed", "cancelled", "timed_out"].contains(&status) {
        return Err(Error::Invalid(format!("a run can't finish as {status}")));
    }
    db.write(None, |w| {
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE runs SET status=?2, cost_usd_micros=?3, input_tokens=?4, output_tokens=?5, error=?6, ended_at=?7, updated_at=?7, version=version+1
             WHERE id=?1 AND trigger='chat'",
            rusqlite::params![run_id, status, cost_usd_micros, input_tokens, output_tokens, error, now])?;
        if n == 0 {
            return Err(Error::NotFound(format!("chat run {run_id}")));
        }
        w.update("runs", run_id, serde_json::json!({"status": status, "cost_usd_micros": cost_usd_micros}))
    })
}

/// The session the CLI started, when the CLI picks it (Codex names its thread once it runs).
pub fn set_session(db: &Db, run_id: &str, session_id: &str) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE runs SET session_id=?2 WHERE id=?1", rusqlite::params![run_id, session_id])?;
        Ok(())
    })
}

/// The commit the run's worktree was at when it started.
pub fn set_base_sha(db: &Db, run_id: &str, sha: &str) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE runs SET base_sha=?2 WHERE id=?1", rusqlite::params![run_id, sha])?;
        Ok(())
    })
}

/// The commit the run's worktree was at when it ended: with `base_sha`, it gives the commits the run made.
pub fn set_head_sha(db: &Db, run_id: &str, sha: &str) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE runs SET head_sha=?2 WHERE id=?1", rusqlite::params![run_id, sha])?;
        Ok(())
    })
}

/// The tool calls the run's CLI refused (Refused in this run), in the order it reported them (`run_refusals`).
pub fn set_refused(db: &Db, run_id: &str, refused: &[Refusal]) -> Result<()> {
    db.write(None, |w| {
        let c = w.conn();
        let n: i64 = c.query_row("SELECT count(*) FROM runs WHERE id=?1", [run_id], |r| r.get(0))?;
        if n == 0 {
            return Err(Error::NotFound(format!("run {run_id}")));
        }
        c.execute("INSERT INTO run_refusals(run_id, refused_json) VALUES (?1, ?2)
                   ON CONFLICT(run_id) DO UPDATE SET refused_json = excluded.refused_json",
                  rusqlite::params![run_id, serde_json::to_string(refused)?])?;
        Ok(())
    })
}

pub fn set_running(db: &Db, run_id: &str, pid: u32) -> Result<()> {
    db.write(None, |w| {
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE runs SET status='running', pid=?2, started_at=?3, updated_at=?3, version=version+1 WHERE id=?1 AND status='queued'",
            rusqlite::params![run_id, pid as i64, now])?;
        if n == 0 {
            return Err(Error::Invalid(format!("run {run_id} is not queued")));
        }
        w.update("runs", run_id, serde_json::json!({"status": "running"}))
    })
}

/// Records the end of a run (status succeeded, failed, cancelled or timed_out) and releases the task's claim.
#[allow(clippy::too_many_arguments)]
pub fn finish(db: &Db, run_id: &str, status: &str, outcome: Option<&Outcome>, cost_usd_micros: i64, input_tokens: i64,
              output_tokens: i64, error: Option<&str>) -> Result<()> {
    if !["succeeded", "failed", "cancelled", "timed_out"].contains(&status) {
        return Err(Error::Invalid(format!("a run can't finish as {status}")));
    }
    db.write(None, |w| finish_in(w, run_id, status, outcome, cost_usd_micros, input_tokens, output_tokens, error))
}

#[allow(clippy::too_many_arguments)]
fn finish_in(w: &Writer, run_id: &str, status: &str, outcome: Option<&Outcome>, cost: i64, input: i64, output: i64, error: Option<&str>) -> Result<()> {
    let c = w.conn();
    let now = ids::now_ms();
    let label = match outcome {
        Some(o) => o.outcome.clone(),
        None if status == "succeeded" => "no_result".into(),
        None => "error".into(),
    };
    let n = c.execute(
        "UPDATE runs SET status=?2, outcome=?3, outcome_json=?4, summary_md=?5, cost_usd_micros=?6, input_tokens=?7, output_tokens=?8,
                error=?9, ended_at=?10, updated_at=?10, version=version+1 WHERE id=?1",
        rusqlite::params![run_id, status, label, outcome.map(serde_json::to_string).transpose()?, outcome.map(|o| o.summary.clone()),
                          cost, input, output, error, now])?;
    if n == 0 {
        return Err(Error::NotFound(format!("run {run_id}")));
    }
    release_claim(w, run_id)?;
    w.update("runs", run_id, serde_json::json!({"status": status, "outcome": label, "cost_usd_micros": cost}))
}

pub(crate) fn release_claim(w: &Writer, run_id: &str) -> Result<()> {
    w.conn().execute("UPDATE tasks SET claimed_by_run_id=NULL, lease_expires_at=NULL WHERE claimed_by_run_id=?1", [run_id])?;
    Ok(())
}

pub fn get(db: &Db, run_id: &str) -> Result<Run> {
    db.read(|c| {
        c.query_row(&format!("SELECT {COLS} FROM runs r JOIN actors a ON a.id = r.agent_actor_id WHERE r.id=?1"), [run_id], row)
            .optional()?.ok_or_else(|| Error::NotFound(format!("run {run_id}")))
    })
}

/// Newest first.
pub fn list_for_task(db: &Db, task_id: &str) -> Result<Vec<Run>> {
    db.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM runs r JOIN actors a ON a.id = r.agent_actor_id WHERE r.task_id=?1 AND r.deleted_at IS NULL ORDER BY r.created_at DESC, r.id DESC"))?;
        Ok(st.query_map([task_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// An agent's latest runs, newest first.
pub fn list_for_agent(db: &Db, agent_id: &str, limit: i64) -> Result<Vec<Run>> {
    db.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM runs r JOIN actors a ON a.id = r.agent_actor_id WHERE r.agent_actor_id=?1 AND r.deleted_at IS NULL ORDER BY r.created_at DESC, r.id DESC LIMIT ?2"))?;
        Ok(st.query_map(rusqlite::params![agent_id, limit], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// Runs that are queued or running, oldest first.
pub fn active(db: &Db) -> Result<Vec<Run>> {
    db.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM runs r JOIN actors a ON a.id = r.agent_actor_id WHERE r.status IN {ACTIVE} ORDER BY r.created_at"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// Midnight UTC on the first of the month `ms` falls in (Unix ms).
pub fn month_start_ms(ms: i64) -> i64 {
    // Howard Hinnant's civil_from_days / days_from_civil.
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    let (y, m) = (if m <= 2 { y - 1 } else { y }, if m > 2 { m - 3 } else { m + 9 });
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5; // day 1 of the month
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * 86_400_000
}

const DAY_MS: i64 = 86_400_000;

/// An agent's runs per UTC day for the last `days` days up to `now` (oldest first; days without runs are zero).
pub fn daily_stats(db: &Db, agent_id: &str, days: i64, now: i64) -> Result<Vec<DayStat>> {
    let today = now - now.rem_euclid(DAY_MS);
    let first = today - (days - 1) * DAY_MS;
    let mut out: Vec<DayStat> = (0..days).map(|i| DayStat { day_start: first + i * DAY_MS, succeeded: 0, failed: 0, other: 0 }).collect();
    db.read(|c| {
        let mut st = c.prepare("SELECT created_at, status FROM runs WHERE agent_actor_id=?1 AND created_at >= ?2 AND created_at < ?3")?;
        let rows = st.query_map(rusqlite::params![agent_id, first, today + DAY_MS], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (at, status) = row?;
            let d = &mut out[((at - first) / DAY_MS) as usize];
            match status.as_str() {
                "succeeded" => d.succeeded += 1,
                "failed" | "timed_out" => d.failed += 1,
                _ => d.other += 1,
            }
        }
        Ok(())
    })?;
    Ok(out)
}

/// What an agent's runs cost since `since_ms` (for its monthly budget).
pub fn agent_spend_since(db: &Db, agent_id: &str, since_ms: i64) -> Result<i64> {
    db.read(|c| Ok(c.query_row(
        "SELECT COALESCE(SUM(cost_usd_micros), 0) FROM runs WHERE agent_actor_id=?1 AND created_at >= ?2",
        rusqlite::params![agent_id, since_ms], |r| r.get(0))?))
}

/// The numbered issues of the task's latest QA verdict, if that verdict was a fail (for the builder's next prompt).
pub fn last_qa_issues(db: &Db, task_id: &str) -> Result<Vec<String>> {
    db.read(|c| {
        let last: Option<(String, Option<String>)> = c.query_row(
            "SELECT outcome, outcome_json FROM runs WHERE task_id=?1 AND outcome IN ('qa_pass','qa_fail') ORDER BY created_at DESC, id DESC LIMIT 1",
            [task_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        Ok(match last {
            Some((o, Some(json))) if o == "qa_fail" => serde_json::from_str::<Outcome>(&json).map(|x| x.issues).unwrap_or_default(),
            _ => vec![],
        })
    })
}

/// At start-up: runs left queued or running by a previous Gizai can't be followed any more. Marks them
/// failed ("interrupted") and releases their claims. A chat answer gets no outcome (it has none), and its chat a note
/// that it was interrupted. Returns how many there were.
pub fn recover_interrupted(db: &Db) -> Result<usize> {
    db.write(None, |w| {
        let runs: Vec<(String, String, Option<String>)> = {
            let mut st = w.conn().prepare(&format!("SELECT id, trigger, chat_thread_id FROM runs WHERE status IN {ACTIVE} ORDER BY created_at"))?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, trigger, thread) in &runs {
            if trigger != "chat" {
                finish_in(w, id, "failed", None, 0, 0, 0, Some("interrupted"))?;
                continue;
            }
            let now = ids::now_ms();
            w.conn().execute("UPDATE runs SET status='failed', error='interrupted', ended_at=?2, updated_at=?2, version=version+1 WHERE id=?1",
                             rusqlite::params![id, now])?;
            w.update("runs", id, serde_json::json!({"status": "failed"}))?;
            if let Some(thread) = thread {
                crate::chat::insert_message(w, &crate::chat::NewMessage { thread_id: thread.clone(), role: "system".into(),
                    body_md: Some(crate::chat::INTERRUPTED.into()), run_id: Some(id.clone()), ..Default::default() })?;
            }
        }
        Ok(runs.len())
    })
}
