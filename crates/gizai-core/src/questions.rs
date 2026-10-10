//! Agents ask the Team Lead before a person (GA-70). When a task agent ends its run with `needs_decision` and the step is
//! on (Settings → Runs → Ask the Team Lead first), the card goes to the Team Lead first: it stays on hold, but out of the
//! Inbox, while the Team Lead's run on the question (trigger `question`) reads it, looks in memory and either answers (the
//! answer goes on the card and the agent's session continues with it) or escalates (a comment with why, the options and
//! its advice, and the card lands in the Inbox). Any error, a timeout or a run without an answer escalates too.
//!
//! What the Team Lead did is kept with the question: under `lead` in the asking run's `outcome_json` (state `asking`,
//! `answering` while Gizai continues the agent, then `answered`, `escalated` or `dropped` when the card moved on without
//! it). The Team Lead's own run is a run without a card, stored with trigger `approval` (the runs table's CHECK takes no
//! new trigger without rebuilding the whole table) and read as `question`, its `outcome_json` naming the run that asked.
//! So the runs table needs no new column.
//!
//! Limits, so there are no loops: one Team Lead attempt per question, two per card, and the question right after a Team
//! Lead answer goes to the person. A `run_for_me` request, a gate's hold (QA bounces, an answer the role can't give, a
//! deploy outside Deploy) and a failed push never go to the Team Lead.
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, Writer};
use crate::memory::{self, Saved, Who};
use crate::{Error, Result, clis, comments, ids, runs, settings, team, util};

/// The Team Lead takes at most this many questions per card.
pub const MAX_PER_CARD: i64 = 2;
/// The trigger a Team Lead's run on a question reports (`Run::trigger`).
pub const TRIGGER: &str = "question";
/// How it is stored: the runs table's CHECK has no `question`, and `approval` was never used (see `runs::reported_trigger`).
pub(crate) const STORED_TRIGGER: &str = "approval";
/// Settings → Runs → Ask the Team Lead first.
const SETTING: &str = "ask_lead_first";
/// An answer or a reason is kept on the question at most this long (the comment has it in full).
const KEEP_CHARS: usize = 600;

/// Whether task agents ask the Team Lead before a person (Settings → Runs): on unless switched off.
pub fn enabled(db: &Db) -> bool {
    settings::get::<bool>(db, SETTING).ok().flatten().unwrap_or(true)
}

/// Switches the step on or off for every agent (Settings → Runs).
pub fn set_enabled(db: &Db, on: bool) -> Result<()> {
    settings::set(db, SETTING, &on)
}

/// What the Team Lead did with a run's question, as the card's Runs tab shows it (`Run::lead`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeadAnswer {
    /// `asking` (the Team Lead looks at it now), `answering` (it answered, and Gizai continues the agent), `answered`,
    /// `escalated` (it asked you: the Inbox) or `dropped` (the card moved on before it was done).
    pub state: String,
    /// The Team Lead.
    pub lead_id: Option<String>,
    /// The Team Lead's run on the question, once it started.
    pub run_id: Option<String>,
    /// Escalated: why you decide.
    pub reason: Option<String>,
    /// Answered: what it answered (the start; the comment has it all).
    pub answer: Option<String>,
    /// The memory note the answer was saved in.
    pub note: Option<String>,
    /// What the Team Lead's run on the question cost (it counts toward the Team Lead's budget).
    pub cost_usd_micros: i64,
    /// You answered after it escalated, and Gizai saved your answer in memory.
    pub learned: bool,
}

/// `lead` in a run's `outcome_json`, as stored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    state: String,
    lead: Option<String>,
    at: i64,
    run: Option<String>,
    reason: Option<String>,
    answer: Option<String>,
    note: Option<String>,
    escalated_at: Option<i64>,
    learned: bool,
}

/// The question's record (`outcome_json.lead`, as JSON text) and the Team Lead's run's cost as `Run::lead`.
pub(crate) fn lead_of(json: Option<String>, cost: Option<i64>) -> Option<LeadAnswer> {
    let s: Stored = serde_json::from_str(&json?).ok()?;
    Some(LeadAnswer { state: s.state, lead_id: s.lead, run_id: s.run, reason: s.reason, answer: s.answer, note: s.note,
                      cost_usd_micros: cost.unwrap_or(0), learned: s.learned })
}

fn stored_in(c: &Connection, run_id: &str) -> Result<Option<Stored>> {
    let json: Option<Option<String>> = c.query_row("SELECT json_extract(outcome_json, '$.lead') FROM runs WHERE id=?1", [run_id], |r| r.get(0)).optional()?;
    Ok(json.flatten().and_then(|j| serde_json::from_str(&j).ok()))
}

fn save_stored(w: &Writer, run_id: &str, s: &Stored) -> Result<()> {
    w.conn().execute("UPDATE runs SET outcome_json = json_set(COALESCE(outcome_json, '{}'), '$.lead', json(?2)), updated_at=?3 WHERE id=?1",
                     rusqlite::params![run_id, serde_json::to_string(s)?, ids::now_ms()])?;
    w.update("runs", run_id, serde_json::json!({"lead": s.state}))
}

fn cut(s: &str, n: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

/// The Team Lead takes the question a task agent's run (`run_id`, on `task_id`) ended with, when: the run asks nobody to
/// run commands (`run_for_me`), its agent isn't the Team Lead, the step is on, agents aren't paused in Settings, the Team
/// Lead (the agent with Chat on) is active, runs on a Claude Code CLI and is under its monthly budget, the card had fewer
/// than `MAX_PER_CARD` Team Lead attempts, and the card's question before this one wasn't answered by the Team Lead.
/// Records the hand-over on the run (state `asking`) and returns the Team Lead's id; None: the question goes to the Inbox
/// as before. Called by `workflow::apply_outcome_with` inside its write, after the hold is set.
pub(crate) fn hand_over_in(w: &Writer, run_id: &str, task_id: &str, agent_id: &str, role: &str, run_for_me: &[String]) -> Result<Option<String>> {
    let c = w.conn();
    if !run_for_me.is_empty() || role == "lead" {
        return Ok(None);
    }
    if !settings::get_in::<bool>(c, SETTING)?.unwrap_or(true) || settings::get_in::<bool>(c, "agents_paused")?.unwrap_or(false) {
        return Ok(None);
    }
    let Some(lead) = team::chat_agent_in(c)? else { return Ok(None) };
    if lead.actor_id == agent_id || lead.status != "active" {
        return Ok(None);
    }
    if clis::kind_in(c, lead.adapter.as_deref().unwrap_or_default())?.as_deref() != Some("claude_code") {
        return Ok(None);
    }
    // An answer continues the agent's session: an Other CLI can't resume one.
    let asked_on: Option<String> = c.query_row("SELECT adapter FROM runs WHERE id=?1", [run_id], |r| r.get(0)).optional()?;
    if clis::kind_in(c, asked_on.as_deref().unwrap_or_default())?.as_deref() == Some("other") {
        return Ok(None);
    }
    if let Some(budget) = lead.budget_usd_micros {
        let spent: i64 = c.query_row("SELECT COALESCE(SUM(cost_usd_micros), 0) FROM runs WHERE agent_actor_id=?1 AND created_at >= ?2",
                                     rusqlite::params![lead.actor_id, runs::month_start_ms(ids::now_ms())], |r| r.get(0))?;
        if spent >= budget {
            return Ok(None);
        }
    }
    let tries: i64 = c.query_row(
        "SELECT count(*) FROM runs WHERE task_id=?1 AND id<>?2 AND json_extract(outcome_json, '$.lead.lead') IS NOT NULL",
        rusqlite::params![task_id, run_id], |r| r.get(0))?;
    if tries >= MAX_PER_CARD {
        return Ok(None);
    }
    // The question right after a Team Lead answer goes to the person: no loops.
    let before: Option<Option<String>> = c.query_row(
        "SELECT json_extract(outcome_json, '$.lead.state') FROM runs WHERE task_id=?1 AND id<>?2 AND outcome='needs_decision' AND deleted_at IS NULL
         ORDER BY created_at DESC, id DESC LIMIT 1", rusqlite::params![task_id, run_id], |r| r.get(0)).optional()?;
    if matches!(before.flatten().as_deref(), Some("answering" | "answered")) {
        return Ok(None);
    }
    save_stored(w, run_id, &Stored { state: "asking".into(), lead: Some(lead.actor_id.clone()), at: ids::now_ms(), ..Default::default() })?;
    Ok(Some(lead.actor_id))
}

/// A run's question, for the Team Lead's run on it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    /// The run that asked.
    pub run_id: String,
    pub task_id: String,
    /// Like KADE-12.
    pub identifier: String,
    pub title: String,
    pub description_md: String,
    pub acceptance_md: Option<String>,
    /// The card's column now.
    pub column: String,
    /// Its project's key and name, and its client's name.
    pub project_key: Option<String>,
    pub project_name: Option<String>,
    pub client_name: Option<String>,
    pub agent_id: String,
    pub agent_name: String,
    pub role: String,
    /// What the agent wrote: its summary, then its numbered issues.
    pub text: String,
    /// What the Team Lead did with it so far.
    pub lead: LeadAnswer,
}

/// The question the run `run_id` asked, with its card.
pub fn question(db: &Db, run_id: &str) -> Result<Question> {
    db.read(|c| {
        let q = c.query_row(
            "SELECT r.id, t.id, t.identifier, t.title, t.description_md, t.acceptance_md, s.name, p.key, p.name, cl.name, r.agent_actor_id, a.name,
                    COALESCE(r.role_key, ''), r.outcome_json, json_extract(r.outcome_json, '$.lead')
             FROM runs r JOIN tasks t ON t.id = r.task_id JOIN workflow_states s ON s.id = t.state_id JOIN actors a ON a.id = r.agent_actor_id
             LEFT JOIN projects p ON p.id = t.project_id LEFT JOIN clients cl ON cl.id = p.client_id
             WHERE r.id = ?1", [run_id], |r| {
                let verdict: Option<String> = r.get(13)?;
                let o: Option<crate::model::Outcome> = verdict.and_then(|j| serde_json::from_str(&j).ok());
                let mut text = o.as_ref().map(|o| o.summary.trim().to_string()).unwrap_or_default();
                for (i, issue) in o.map(|o| o.issues).unwrap_or_default().iter().enumerate() {
                    text.push_str(&format!("{}{}. {}", if i == 0 && !text.is_empty() { "\n\n" } else { "\n" }, i + 1, issue.trim()));
                }
                Ok(Question {
                    run_id: r.get(0)?, task_id: r.get(1)?, identifier: r.get(2)?, title: r.get(3)?, description_md: r.get(4)?, acceptance_md: r.get(5)?,
                    column: r.get(6)?, project_key: r.get(7)?, project_name: r.get(8)?, client_name: r.get(9)?, agent_id: r.get(10)?,
                    agent_name: r.get(11)?, role: r.get(12)?, text: text.trim().to_string(), lead: lead_of(r.get(14)?, None).unwrap_or_default(),
                })
            }).optional()?;
        q.ok_or_else(|| Error::NotFound(format!("the question of run {run_id}")))
    })
}

/// Records a queued run of the Team Lead on the question `asked_run_id`: no card and no chat (so the card's latest run
/// stays the one that asked, which the answer continues), trigger `question`, role lead, on the Team Lead's own CLI. The
/// question remembers it.
pub fn create_run(db: &Db, lead_id: &str, asked_run_id: &str, session_id: &str, cwd: &str, log_path: &str) -> Result<String> {
    db.write(Some(lead_id), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let task: Option<String> = c.query_row("SELECT task_id FROM runs WHERE id=?1", [asked_run_id], |r| r.get(0)).optional()?.flatten();
        let task = task.ok_or_else(|| Error::NotFound(format!("the question of run {asked_run_id}")))?;
        let (adapter, model): (String, Option<String>) = c.query_row(
            "SELECT adapter, model FROM agent_configs WHERE actor_id=?1", [lead_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?.ok_or_else(|| Error::NotFound(format!("agent {lead_id}")))?;
        let id = ids::new_id();
        c.execute(
            "INSERT INTO runs(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, trigger, role_key, adapter, model,
                              status, cwd, worktree_path, session_id, log_path, outcome_json)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?3, ?5, 'lead', ?6, ?7, 'queued', ?8, ?8, ?9, ?10, ?11)",
            rusqlite::params![id, now, lead_id, util::org_id(c)?, STORED_TRIGGER, adapter, model, cwd, session_id, log_path,
                              serde_json::json!({"asked": asked_run_id, "task": task}).to_string()],
        )?;
        w.insert("runs", &id, serde_json::json!({"agent": lead_id, "trigger": TRIGGER, "asked": asked_run_id}))?;
        if let Some(mut s) = stored_in(c, asked_run_id)? {
            s.run = Some(id.clone());
            save_stored(w, asked_run_id, &s)?;
        }
        Ok(id)
    })
}

/// Records the end of the Team Lead's run on a question: its cost (it counts toward the Team Lead's budget), its last
/// message and, when it failed, why.
#[allow(clippy::too_many_arguments)]
pub fn finish_run(db: &Db, run_id: &str, status: &str, cost_usd_micros: i64, input_tokens: i64, output_tokens: i64, error: Option<&str>,
                  summary: Option<&str>) -> Result<()> {
    if !["succeeded", "failed", "cancelled", "timed_out"].contains(&status) {
        return Err(Error::Invalid(format!("a run can't finish as {status}")));
    }
    db.write(None, |w| {
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE runs SET status=?2, cost_usd_micros=?3, input_tokens=?4, output_tokens=?5, error=?6, summary_md=?7, ended_at=?8, updated_at=?8,
                    version=version+1 WHERE id=?1 AND trigger=?9",
            rusqlite::params![run_id, status, cost_usd_micros, input_tokens, output_tokens, error, summary, now, STORED_TRIGGER])?;
        if n == 0 {
            return Err(Error::NotFound(format!("the Team Lead's run {run_id}")));
        }
        w.update("runs", run_id, serde_json::json!({"status": status, "cost_usd_micros": cost_usd_micros}))
    })
}

/// The card still waits for the Team Lead on this question: the question is `asking` or `answering` and the card is still
/// on hold for a decision (a person may have cleared the hold, or moved the card on, meanwhile).
fn waiting_in(c: &Connection, asked_run_id: &str) -> Result<Option<(Stored, String)>> {
    let Some(s) = stored_in(c, asked_run_id)? else { return Ok(None) };
    if !matches!(s.state.as_str(), "asking" | "answering") {
        return Ok(None);
    }
    let card: Option<(String, Option<String>)> = c.query_row(
        "SELECT t.id, t.hold FROM runs r JOIN tasks t ON t.id = r.task_id WHERE r.id=?1 AND t.deleted_at IS NULL", [asked_run_id],
        |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    Ok(match card {
        Some((task, Some(hold))) if hold == "needs_decision" => Some((s, task)),
        _ => None,
    })
}

/// Whether the card still waits for the Team Lead on the question `asked_run_id` (see `waiting_in`).
pub fn waiting(db: &Db, asked_run_id: &str) -> Result<bool> {
    db.read(|c| Ok(waiting_in(c, asked_run_id)?.is_some()))
}

/// The Team Lead answered `answer`: the question is `answering` while Gizai continues the agent with it (the card stays
/// out of the Inbox meanwhile). False when the card no longer waits for it (the question is `dropped`).
pub fn answering(db: &Db, asked_run_id: &str, answer: &str, note: Option<&str>) -> Result<bool> {
    db.write(None, |w| {
        let Some((mut s, _)) = waiting_in(w.conn(), asked_run_id)? else {
            drop_in(w, asked_run_id)?;
            return Ok(false);
        };
        s.state = "answering".into();
        s.answer = Some(cut(answer, KEEP_CHARS));
        s.note = note.map(str::to_string);
        save_stored(w, asked_run_id, &s)?;
        Ok(true)
    })
}

/// The agent carries on with the Team Lead's answer: the question is `answered`.
pub fn answered(db: &Db, asked_run_id: &str) -> Result<()> {
    db.write(None, |w| {
        let Some(mut s) = stored_in(w.conn(), asked_run_id)? else { return Ok(()) };
        s.state = "answered".into();
        save_stored(w, asked_run_id, &s)
    })
}

/// The card moved on before the Team Lead was done (a person cleared the hold, or started a run): the question no longer
/// waits for it.
fn drop_in(w: &Writer, asked_run_id: &str) -> Result<()> {
    match stored_in(w.conn(), asked_run_id)? {
        Some(mut s) if matches!(s.state.as_str(), "asking" | "answering") => {
            s.state = "dropped".into();
            save_stored(w, asked_run_id, &s)
        }
        _ => Ok(()),
    }
}

/// The Team Lead asks you: its comment (`comment`, "Needs <you>: why, the options, my advice") goes on the card as the
/// Team Lead (with its run, when there is one), and the card stays on hold for a decision with the reason "Team Lead
/// escalated to you: <reason>", now in the Inbox (the hold's time is now, so it notifies as new). The question is
/// `escalated`. False when the card no longer waited for the Team Lead: nothing is posted or held.
pub fn escalate(db: &Db, asked_run_id: &str, lead_run_id: Option<&str>, reason: &str, comment: &str) -> Result<bool> {
    let lead = db.read(|c| Ok(stored_in(c, asked_run_id)?.and_then(|s| s.lead)))?;
    db.write(lead.as_deref(), |w| {
        let Some((mut s, task)) = waiting_in(w.conn(), asked_run_id)? else {
            drop_in(w, asked_run_id)?;
            return Ok(false);
        };
        let reason = cut(reason, KEEP_CHARS);
        if let (Some(lead), false) = (s.lead.as_deref(), comment.trim().is_empty()) {
            comments::add_in(w, lead, &task, comment.trim(), lead_run_id)?;
        }
        crate::workflow::set_hold(w, &task, "needs_decision", &format!("Team Lead escalated to you: {reason}"))?;
        s.state = "escalated".into();
        s.reason = Some(reason);
        s.escalated_at = Some(ids::now_ms());
        save_stored(w, asked_run_id, &s)?;
        Ok(true)
    })
}

/// A title as a note's title can have it: without `* " \ / < > : | ? # ^ [ ]`, on one line, at most 80 characters.
fn note_title(s: &str) -> String {
    let t: String = s.chars().map(|ch| if "*\"\\/<>:|?#^[]".contains(ch) || ch.is_control() { ' ' } else { ch }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(80).collect();
    if t.trim().is_empty() { "General".into() } else { t.trim().to_string() }
}

/// Where answers to a project's questions go when the Team Lead names no note: `Decisions/<project name>` (`General`
/// without a project), with properties that give it to the agents on that project's cards.
pub fn decisions_path(project_name: Option<&str>) -> String {
    format!("Decisions/{}", note_title(project_name.unwrap_or("General")))
}

/// Saves an answer to the question `asked_run_id` in memory as the Team Lead, linked to the card (its identifier is in
/// the text): `text` in the note at `path` when the Team Lead named one (a shared folder, like `Decisions/Exports` or
/// `Standards/CSV`), else, or when that can't be saved, a dated line in the project's decisions note
/// (`decisions_path`): `- 2026-10-10 (KADE-12, answered by <by>): <the question> → <the answer>`. `run_id`: the run kept
/// on the note's version. `day`: today, like 2026-10-10.
#[allow(clippy::too_many_arguments)]
pub fn remember(db: &Db, lead_id: &str, asked_run_id: &str, run_id: Option<&str>, by: &str, answer: &str, path: Option<&str>, text: Option<&str>,
                day: &str) -> Result<Saved> {
    let q = question(db, asked_run_id)?;
    let who = Who::Lead(lead_id.to_string());
    if let (Some(path), Some(text)) = (path.map(str::trim).filter(|p| !p.is_empty()), text.map(str::trim).filter(|t| !t.is_empty())) {
        let shared = memory::SHARED_FOLDERS.iter().any(|f| path.split('/').next().is_some_and(|top| top.eq_ignore_ascii_case(f)));
        let line = if text.contains(&q.identifier) { text.to_string() } else { format!("{text} ({})", q.identifier) };
        if shared && let Ok(saved) = memory::append(db, &who, path, None, &format!("- {day}: {line}"), run_id) {
            return Ok(saved);
        }
    }
    let asked = cut(&q.text.lines().find(|l| !l.trim().is_empty()).unwrap_or(&q.title).replace('\n', " "), 200);
    let line = format!("- {day} ({}, answered by {by}): {asked} → {}", q.identifier, cut(&answer.split_whitespace().collect::<Vec<_>>().join(" "), 400));
    let path = decisions_path(q.project_name.as_deref());
    match memory::find(db, &path)? {
        Some(_) => memory::append(db, &who, &path, None, &line, run_id),
        None => {
            let mut props = String::from("---\ntype: decision\n");
            if let Some(key) = &q.project_key {
                props.push_str(&format!("project: {key}\n"));
            }
            let what = q.project_name.as_deref().unwrap_or("cards without a project");
            let body = format!("{props}---\n# {}\n\nAnswers to the agents' questions on {what}, kept by the Team Lead.\n\n{line}\n", memory::title_of(&path));
            memory::write(db, &who, &path, &body, None, run_id)
        }
    }
}

/// Records where the answer to the question `asked_run_id` was saved (`remember`).
pub fn noted(db: &Db, asked_run_id: &str, note: &str) -> Result<()> {
    db.write(None, |w| {
        let Some(mut s) = stored_in(w.conn(), asked_run_id)? else { return Ok(()) };
        s.note = Some(note.to_string());
        save_stored(w, asked_run_id, &s)
    })
}

/// Learn from your answer: a run `new_run_id` starts on the card after its last run's question was escalated to you, and
/// people wrote on the card since. What they wrote is saved in memory as the Team Lead (`remember`), linked to the card,
/// once per question. Returns the note it went to; None when there was nothing to learn.
pub fn learn_from_person(db: &Db, task_id: &str, new_run_id: &str, day: &str) -> Result<Option<Saved>> {
    let found: Option<(String, Option<String>)> = db.read(|c| Ok(c.query_row(
        "SELECT id, json_extract(outcome_json, '$.lead') FROM runs WHERE task_id=?1 AND id<>?2 AND deleted_at IS NULL
         ORDER BY created_at DESC, id DESC LIMIT 1", rusqlite::params![task_id, new_run_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?))?;
    let Some((asked, Some(json))) = found else { return Ok(None) };
    let Ok(s) = serde_json::from_str::<Stored>(&json) else { return Ok(None) };
    if s.state != "escalated" || s.learned {
        return Ok(None);
    }
    let since = s.escalated_at.unwrap_or(s.at);
    let said: Vec<crate::model::Comment> = comments::list(db, task_id)?.into_iter()
        .filter(|c| c.author_kind == "person" && c.created_at >= since && !c.body_md.trim().is_empty()).collect();
    if said.is_empty() {
        return Ok(None);
    }
    let lead = match s.lead.clone().filter(|l| team::agent(db, l).is_ok()) {
        Some(l) => l,
        None => match team::chat_agent(db)? {
            Some(m) => m.actor_id,
            None => return Ok(None),
        },
    };
    let by = said.last().map(|c| c.author_name.clone()).unwrap_or_default();
    let answer = said.iter().map(|c| c.body_md.trim().to_string()).collect::<Vec<_>>().join(" / ");
    let saved = remember(db, &lead, &asked, None, &by, &answer, None, None, day)?;
    db.write(None, |w| {
        let Some(mut s) = stored_in(w.conn(), &asked)? else { return Ok(()) };
        s.learned = true;
        s.note = Some(saved.path.clone());
        save_stored(w, &asked, &s)
    })?;
    Ok(Some(saved))
}

/// The questions that waited for the Team Lead when Gizai stopped (its run is gone with it): each goes to you, with a
/// comment that says so. Run at start-up, after `runs::recover_interrupted`. Returns how many there were.
pub fn recover(db: &Db) -> Result<usize> {
    let waiting: Vec<(String, Option<String>)> = db.read(|c| {
        let mut st = c.prepare("SELECT id, json_extract(outcome_json, '$.lead.run') FROM runs
                                WHERE json_extract(outcome_json, '$.lead.state') IN ('asking','answering') AND deleted_at IS NULL")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    let mut n = 0;
    for (asked, lead_run) in waiting {
        if escalate(db, &asked, lead_run.as_deref(), "Gizai stopped before it had answered",
                    "Gizai stopped before I had answered this question, so it is yours to decide.")? {
            n += 1;
        }
    }
    Ok(n)
}
