use crate::db::{Db, Writer};
use crate::model::{ChangeEntry, Label, Task, TaskFilter, TaskInput, TaskPatch};
use crate::sortkey::key_after;
use crate::util::org_id;
use crate::{Error, Result, ids};
use rusqlite::{Connection, OptionalExtension, Row};
use std::collections::HashMap;

const COLS: &str = "t.id, t.identifier, t.project_id, p.name, p.color, t.title, {desc}, t.acceptance_md, t.state_id, s.name,
    s.category, t.priority, t.assignee_actor_id, a.name, a.kind, t.hold, t.hold_reason, t.bounce_count, t.fail_count,
    t.sort_key, t.branch, t.created_at, t.updated_at, t.pr_url, t.pr_state, t.testing, t.deleted_at,
    CASE WHEN t.deleted_at IS NOT NULL THEN (SELECT x.name FROM changes ch JOIN actors x ON x.id = ch.actor_id
      WHERE ch.row_id = t.id AND ch.table_name = 'tasks' AND ch.op = 'delete' ORDER BY ch.seq DESC LIMIT 1) END, t.hold_at,
    CASE WHEN t.hold IS NOT NULL THEN (SELECT json_extract(r.outcome_json, '$.run_for_me') FROM runs r
      WHERE r.task_id = t.id AND r.deleted_at IS NULL ORDER BY r.created_at DESC, r.id DESC LIMIT 1) END
  FROM tasks t JOIN workflow_states s ON s.id = t.state_id
  LEFT JOIN projects p ON p.id = t.project_id
  LEFT JOIN actors a ON a.id = t.assignee_actor_id";

fn select(full: bool) -> String {
    format!("SELECT {}", COLS.replace("{desc}", if full { "t.description_md" } else { "''" }))
}

fn row(r: &Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?, identifier: r.get(1)?, project_id: r.get(2)?, project_name: r.get(3)?, project_color: r.get(4)?,
        title: r.get(5)?, description_md: r.get(6)?, acceptance_md: r.get(7)?, state_id: r.get(8)?, state_name: r.get(9)?,
        state_category: r.get(10)?, priority: r.get(11)?, assignee_id: r.get(12)?, assignee_name: r.get(13)?,
        assignee_kind: r.get(14)?, labels: vec![], hold: r.get(15)?, hold_reason: r.get(16)?, hold_at: r.get(28)?, bounce_count: r.get(17)?,
        fail_count: r.get(18)?, sort_key: r.get(19)?, branch: r.get(20)?, created_at: r.get(21)?, updated_at: r.get(22)?,
        pr_url: r.get(23)?, pr_state: r.get(24)?, testing: r.get::<_, i64>(25)? != 0, archived_at: r.get(26)?, archived_by: r.get(27)?,
        run_for_me: crate::runs::commands_of(r.get(29)?),
    })
}

fn labels_for(c: &Connection, task_ids: &[String]) -> Result<HashMap<String, Vec<Label>>> {
    let mut out: HashMap<String, Vec<Label>> = HashMap::new();
    if task_ids.is_empty() {
        return Ok(out);
    }
    let mut st = c.prepare(
        "SELECT tl.task_id, l.id, l.name, l.color FROM task_labels tl JOIN labels l ON l.id = tl.label_id
         WHERE tl.deleted_at IS NULL ORDER BY l.name",
    )?;
    let wanted: std::collections::HashSet<&String> = task_ids.iter().collect();
    for r in st.query_map([], |r| Ok((r.get::<_, String>(0)?, Label { id: r.get(1)?, name: r.get(2)?, color: r.get(3)? })))? {
        let (tid, l) = r?;
        if wanted.contains(&tid) {
            out.entry(tid).or_default().push(l);
        }
    }
    Ok(out)
}

/// Lists omit the description (it can be large); `get` returns it.
pub fn list(db: &Db, filter: &TaskFilter) -> Result<Vec<Task>> {
    db.read(|c| {
        let mut sql = format!("{} WHERE t.deleted_at IS NULL", select(false));
        if filter.project_id.is_some() {
            sql.push_str(" AND t.project_id = ?1");
        }
        if filter.open_only {
            sql.push_str(" AND t.state_category NOT IN ('done','cancelled')");
        }
        sql.push_str(" ORDER BY s.sort_key, t.sort_key");
        let mut st = c.prepare(&sql)?;
        let tasks: Vec<Task> = match &filter.project_id {
            Some(p) => st.query_map([p], row)?.collect::<rusqlite::Result<_>>()?,
            None => st.query_map([], row)?.collect::<rusqlite::Result<_>>()?,
        };
        with_labels(c, tasks)
    })
}

fn with_labels(c: &Connection, mut tasks: Vec<Task>) -> Result<Vec<Task>> {
    let ids: Vec<String> = tasks.iter().map(|t| t.id.clone()).collect();
    let mut labels = labels_for(c, &ids)?;
    for t in &mut tasks {
        t.labels = labels.remove(&t.id).unwrap_or_default();
    }
    Ok(tasks)
}

/// The archived cards (the bin), of one project or of all: the most recently archived first. `list` leaves them out.
pub fn archived(db: &Db, project_id: Option<&str>) -> Result<Vec<Task>> {
    db.read(|c| {
        let mut st = c.prepare(&format!(
            "{} WHERE t.deleted_at IS NOT NULL AND (?1 IS NULL OR t.project_id = ?1) ORDER BY t.deleted_at DESC, t.identifier", select(false)))?;
        let tasks: Vec<Task> = st.query_map([project_id], row)?.collect::<rusqlite::Result<_>>()?;
        with_labels(c, tasks)
    })
}

/// The Inbox: open cards on hold (an agent or a gate needs a person) and cards waiting for `you` in Review or Deploy
/// (merged, not deployed yet). Same rule as the UI's `needsYou`.
pub fn needs_you(db: &Db, you_id: &str) -> Result<Vec<Task>> {
    Ok(list(db, &TaskFilter { open_only: true, ..Default::default() })?
        .into_iter()
        .filter(|t| t.hold.is_some() || (matches!(t.state_category.as_str(), "review" | "deploy") && t.assignee_id.as_deref() == Some(you_id)))
        .collect())
}

pub fn get(db: &Db, id: &str) -> Result<Task> {
    db.read(|c| get_in(c, id))
}

/// A card's id from its identifier (GA-12, case ignored), archived cards included.
pub fn id_of(db: &Db, identifier: &str) -> Result<String> {
    db.read(|c| {
        c.query_row("SELECT id FROM tasks WHERE identifier = ?1 COLLATE NOCASE", [identifier.trim()], |r| r.get(0))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("task {identifier}")))
    })
}

pub(crate) fn get_in(c: &Connection, id: &str) -> Result<Task> {
    let mut t = c
        .query_row(&format!("{} WHERE t.id = ?1", select(true)), [id], row)
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("task {id}")))?;
    t.labels = labels_for(c, &[t.id.clone()])?.remove(&t.id).unwrap_or_default();
    Ok(t)
}

/// An archived card is read-only until it's restored: moving, editing, commenting and starting a run are refused.
pub(crate) fn not_archived(c: &Connection, id: &str) -> Result<()> {
    let row: Option<(String, Option<i64>)> = c
        .query_row("SELECT identifier, deleted_at FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    match row {
        Some((identifier, Some(_))) => Err(Error::Invalid(format!("{identifier} is archived: restore it first"))),
        _ => Ok(()),
    }
}

/// Archives a card in Done (a soft delete): it leaves the board, the list, the Inbox, the queue and every count. Its
/// column, comments, runs, files, branch and identifier stay. A card outside Done, or one an agent is working on, is
/// refused. The activity says who archived it (a `delete` change, which `archived_by` reads).
pub fn archive(db: &Db, actor: &str, id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let c = w.conn();
        let (identifier, column, category, deleted): (String, String, String, Option<i64>) = c
            .query_row(
                "SELECT t.identifier, s.name, s.category, t.deleted_at FROM tasks t JOIN workflow_states s ON s.id = t.state_id WHERE t.id=?1",
                [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;
        if deleted.is_some() {
            return Err(Error::Invalid(format!("{identifier} is already archived")));
        }
        if category != "done" {
            return Err(Error::Invalid(format!("Only a card in Done can be archived, and {identifier} is in {column}")));
        }
        let live: i64 = c.query_row(
            "SELECT count(*) FROM runs WHERE task_id=?1 AND status IN ('queued','running','waiting_approval')", [id], |r| r.get(0))?;
        if live > 0 {
            return Err(Error::Invalid("An agent is working on this card".into()));
        }
        let now = ids::now_ms();
        c.execute("UPDATE tasks SET deleted_at=?2, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1",
                  rusqlite::params![id, now, actor])?;
        w.delete("tasks", id)
    })
}

/// Restores an archived card: back at the bottom of its Done column, as it was. The activity says who restored it.
pub fn restore(db: &Db, actor: &str, id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let c = w.conn();
        let (identifier, state_id, deleted): (String, String, Option<i64>) = c
            .query_row("SELECT identifier, state_id, deleted_at FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;
        if deleted.is_none() {
            return Err(Error::Invalid(format!("{identifier} isn't archived")));
        }
        let key = key_after(last_key(c, &state_id)?.as_deref());
        c.execute("UPDATE tasks SET deleted_at=NULL, sort_key=?2, updated_at=?3, updated_by=?4, version=version+1 WHERE id=?1",
                  rusqlite::params![id, key, ids::now_ms(), actor])?;
        w.update("tasks", id, serde_json::json!({"archived": false}))
    })
}

fn team_of_project(c: &Connection, project_id: &str) -> Result<(String, Option<String>, String)> {
    let (key, client, team): (String, Option<String>, Option<String>) = c
        .query_row("SELECT key, client_id, team_id FROM projects WHERE id=?1 AND deleted_at IS NULL", [project_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("project {project_id}")))?;
    let team = match team {
        Some(t) => t,
        None => c.query_row("SELECT id FROM teams WHERE deleted_at IS NULL ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
    };
    Ok((key, client, team))
}

fn state_in_team(c: &Connection, team_id: &str, state_id: &str) -> Result<(String, String)> {
    c.query_row(
        "SELECT name, category FROM workflow_states WHERE id=?1 AND team_id=?2 AND deleted_at IS NULL",
        [state_id, team_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| Error::Invalid("that column doesn't belong to this project's team".into()))
}

fn last_key(c: &Connection, state_id: &str) -> Result<Option<String>> {
    Ok(c.query_row("SELECT max(sort_key) FROM tasks WHERE state_id=?1 AND deleted_at IS NULL", [state_id], |r| r.get(0))?)
}

fn actor_kind(c: &Connection, id: &str) -> Result<Option<String>> {
    Ok(c.query_row("SELECT kind FROM actors WHERE id=?1 AND deleted_at IS NULL", [id], |r| r.get(0)).optional()?)
}

pub fn create(db: &Db, actor: &str, input: TaskInput) -> Result<String> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(Error::Invalid("task title is required".into()));
    }
    db.write(Some(actor), |w| {
        let c = w.conn();
        let (key, client, team) = team_of_project(c, &input.project_id)?;
        let state_id = match &input.state_id {
            Some(s) if !s.is_empty() => s.clone(),
            _ => c.query_row(
                "SELECT id FROM workflow_states WHERE team_id=?1 AND deleted_at IS NULL ORDER BY sort_key LIMIT 1", [&team], |r| r.get(0))?,
        };
        let (_, category) = state_in_team(c, &team, &state_id)?;
        let n: i64 = c.query_row(
            "UPDATE projects SET next_task_number = next_task_number + 1 WHERE id=?1 RETURNING next_task_number - 1",
            [&input.project_id], |r| r.get(0))?;
        let identifier = format!("{key}-{n}");
        let sort_key = key_after(last_key(c, &state_id)?.as_deref());
        let owner = if actor_kind(c, actor)?.as_deref() == Some("person") { Some(actor.to_string()) } else { None };
        let assignee = input.assignee_id.clone().filter(|a| !a.is_empty());
        if let Some(a) = &assignee {
            actor_kind(c, a)?.ok_or_else(|| Error::Invalid("unknown assignee".into()))?;
        }
        let id = ids::new_id();
        let now = ids::now_ms();
        c.execute(
            "INSERT INTO tasks(id, created_at, updated_at, created_by, updated_by, org_id, project_id, client_id, identifier, title,
               description_md, acceptance_md, state_id, state_category, owner_person_id, priority, assignee_actor_id,
               creator_actor_id, sort_key, started_at, testing)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?3, ?16,
               CASE WHEN ?12 = 'in_progress' THEN ?2 END, ?17)",
            rusqlite::params![id, now, actor, org_id(c)?, input.project_id, client, identifier, title, input.description_md,
                              input.acceptance_md.as_ref().filter(|a| !a.trim().is_empty()), state_id, category, owner,
                              input.priority.clamp(0, 4), assignee, sort_key, input.testing.unwrap_or(true)],
        )?;
        w.insert("tasks", &id, serde_json::json!({"identifier": identifier, "title": title}))?;
        if !input.label_ids.is_empty() {
            set_labels_in(w, &id, &input.label_ids)?;
        }
        Ok(id)
    })
}

pub fn update(db: &Db, actor: &str, id: &str, patch: TaskPatch) -> Result<()> {
    update_with_labels(db, actor, id, patch, None)
}

/// `update` and, with `label_ids`, the card's new set of labels, in one write: when anything is refused (a bad label,
/// hold or title), nothing changes.
pub fn update_with_labels(db: &Db, actor: &str, id: &str, patch: TaskPatch, label_ids: Option<Vec<String>>) -> Result<()> {
    db.write(Some(actor), |w| {
        if let Some(l) = &label_ids {
            check_labels(w.conn(), l)?;
        }
        update_in(w, actor, id, &patch)?;
        match &label_ids {
            Some(l) => set_labels_in(w, id, l),
            None => Ok(()),
        }
    })
}

fn check_labels(c: &Connection, label_ids: &[String]) -> Result<()> {
    for l in label_ids {
        if c.query_row("SELECT count(*) FROM labels WHERE id=?1 AND deleted_at IS NULL", [l], |r| r.get::<_, i64>(0))? == 0 {
            return Err(Error::Invalid("unknown label".into()));
        }
    }
    Ok(())
}

fn update_in(w: &Writer, actor: &str, id: &str, patch: &TaskPatch) -> Result<()> {
    {
        let c = w.conn();
        not_archived(c, id)?;
        use rusqlite::types::Value as V;
        let mut cols: Vec<(&str, V)> = vec![];
        let opt = |s: &String| if s.trim().is_empty() { V::Null } else { V::Text(s.clone()) };
        if let Some(t) = &patch.title {
            let t = t.trim();
            if t.is_empty() {
                return Err(Error::Invalid("task title is required".into()));
            }
            cols.push(("title", V::Text(t.to_string())));
        }
        if let Some(d) = &patch.description_md {
            cols.push(("description_md", V::Text(d.clone())));
        }
        if let Some(a) = &patch.acceptance_md {
            cols.push(("acceptance_md", opt(a)));
        }
        if let Some(p) = patch.priority {
            if !(0..=4).contains(&p) {
                return Err(Error::Invalid("priority must be 0–4".into()));
            }
            cols.push(("priority", V::Integer(p)));
        }
        for (col, v) in [("assignee_actor_id", &patch.assignee_id), ("pinned_actor_id", &patch.pinned_actor_id)] {
            if let Some(a) = v {
                if !a.is_empty() && actor_kind(c, a)?.is_none() {
                    return Err(Error::Invalid("unknown person or agent".into()));
                }
                cols.push((col, opt(a)));
            }
        }
        if let Some(d) = &patch.due_on {
            cols.push(("due_on", opt(d)));
        }
        if let Some(on) = patch.testing {
            cols.push(("testing", V::Integer(on as i64)));
        }
        if let Some(h) = &patch.hold {
            if !h.is_empty() && !["needs_decision", "stalled", "merge_conflict", "waiting_approval", "rate_limited", "blocked"].contains(&h.as_str()) {
                return Err(Error::Invalid("unknown hold".into()));
            }
            cols.push(("hold", opt(h)));
            if h.is_empty() {
                // a person cleared it: the card gets fresh tries before it stalls again
                cols.push(("hold_reason", V::Null));
                cols.push(("fail_count", V::Integer(0)));
                cols.push(("hold_at", V::Null));
            } else {
                // a comment after this answers it (the Team Lead's board check)
                cols.push(("hold_at", V::Integer(ids::now_ms())));
            }
        }
        if let Some(r) = &patch.hold_reason {
            if patch.hold.as_deref() != Some("") {
                cols.push(("hold_reason", opt(r)));
            }
        }
        if cols.is_empty() {
            return not_archived(c, id).and_then(|_| {
                let n: i64 = c.query_row("SELECT count(*) FROM tasks WHERE id=?1 AND deleted_at IS NULL", [id], |r| r.get(0))?;
                if n == 0 { Err(Error::NotFound(format!("task {id}"))) } else { Ok(()) }
            });
        }
        cols.push(("updated_at", V::Integer(ids::now_ms())));
        cols.push(("updated_by", V::Text(actor.to_string())));
        let sets: Vec<String> = cols.iter().enumerate().map(|(i, (col, _))| format!("{col} = ?{}", i + 2)).collect();
        let sql = format!("UPDATE tasks SET {}, version = version + 1 WHERE id = ?1 AND deleted_at IS NULL", sets.join(", "));
        let mut params: Vec<V> = vec![V::Text(id.to_string())];
        params.extend(cols.into_iter().map(|(_, v)| v));
        let n = c.execute(&sql, rusqlite::params_from_iter(params))?;
        if n == 0 {
            return Err(Error::NotFound(format!("task {id}")));
        }
        w.update("tasks", id, serde_json::to_value(patch)?)
    }
}

/// A move by hand (the board, the Team Lead's `move_task`). A person dragging a card on hold into To do or In progress
/// answers the hold: it comes off (with fresh tries), so the queue or the next board check starts the card. Gates move
/// cards with `move_in` and keep their holds.
pub fn move_to(db: &Db, actor: &str, id: &str, state_id: &str, sort_key: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        move_in(w, actor, id, state_id, Some(sort_key))?;
        let c = w.conn();
        let person = actor_kind(c, actor)?.as_deref() == Some("person");
        let (category, held): (String, bool) = c.query_row(
            "SELECT state_category, hold IS NOT NULL FROM tasks WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if person && held && matches!(category.as_str(), "ready" | "in_progress") {
            c.execute("UPDATE tasks SET hold=NULL, hold_reason=NULL, hold_at=NULL, fail_count=0, updated_at=?2, version=version+1 WHERE id=?1",
                      rusqlite::params![id, ids::now_ms()])?;
            w.update("tasks", id, serde_json::json!({"hold": null, "released": "moved by hand"}))?;
        }
        Ok(())
    })
}

/// Move inside an open write (used by the workflow gates). `sort_key` None appends to the column.
pub(crate) fn move_in(w: &Writer, actor: &str, id: &str, state_id: &str, sort_key: Option<&str>) -> Result<()> {
    let c = w.conn();
    not_archived(c, id)?;
    let (project, from): (Option<String>, String) = c
        .query_row(
            "SELECT t.project_id, s.name FROM tasks t JOIN workflow_states s ON s.id=t.state_id WHERE t.id=?1 AND t.deleted_at IS NULL",
            [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("task {id}")))?;
    let team = match project {
        Some(p) => team_of_project(c, &p)?.2,
        None => c.query_row("SELECT id FROM teams ORDER BY created_at LIMIT 1", [], |r| r.get(0))?,
    };
    let (name, category) = state_in_team(c, &team, state_id)?;
    let key = match sort_key {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => key_after(last_key(c, state_id)?.as_deref()),
    };
    let now = ids::now_ms();
    c.execute(
        "UPDATE tasks SET state_id=?2, state_category=?3, sort_key=?4,
           started_at = coalesce(started_at, CASE WHEN ?3 = 'in_progress' THEN ?5 END),
           completed_at = CASE WHEN ?3 = 'done' THEN ?5 ELSE NULL END,
           updated_at=?5, updated_by=?6, version=version+1
         WHERE id=?1",
        rusqlite::params![id, state_id, category, key, now, actor],
    )?;
    if from != name {
        w.update("tasks", id, serde_json::json!({"column": [from, name]}))?;
    }
    Ok(())
}

pub fn set_labels(db: &Db, actor: &str, id: &str, label_ids: Vec<String>) -> Result<()> {
    db.write(Some(actor), |w| {
        not_archived(w.conn(), id)?;
        set_labels_in(w, id, &label_ids)
    })
}

fn set_labels_in(w: &Writer, id: &str, label_ids: &[String]) -> Result<()> {
    let c = w.conn();
    let now = ids::now_ms();
    check_labels(c, label_ids)?;
    c.execute("DELETE FROM task_labels WHERE task_id=?1", [id])?;
    for l in label_ids {
        c.execute("INSERT OR IGNORE INTO task_labels(task_id, label_id, created_at) VALUES (?1, ?2, ?3)", rusqlite::params![id, l, now])?;
    }
    let names: Vec<String> = label_ids
        .iter()
        .map(|l| c.query_row("SELECT name FROM labels WHERE id=?1", [l], |r| r.get(0)))
        .collect::<rusqlite::Result<_>>()?;
    c.execute("UPDATE tasks SET updated_at=?2 WHERE id=?1", rusqlite::params![id, now])?;
    w.update("tasks", id, serde_json::json!({"labels": names}))
}

pub fn activity(db: &Db, task_id: &str) -> Result<Vec<ChangeEntry>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT ch.hlc, a.name, ch.table_name, ch.op, ch.diff_json FROM changes ch LEFT JOIN actors a ON a.id = ch.actor_id
             WHERE ch.row_id = ?1 OR ch.row_id IN (SELECT id FROM comments WHERE task_id = ?1) ORDER BY ch.seq",
        )?;
        let rows = st.query_map([task_id], |r| {
            let hlc: String = r.get(0)?;
            let diff: Option<String> = r.get(4)?;
            Ok(ChangeEntry {
                at: hlc.split('-').next().and_then(|s| s.parse().ok()).unwrap_or(0),
                actor_name: r.get(1)?,
                table: r.get(2)?,
                op: r.get(3)?,
                diff: diff.and_then(|d| serde_json::from_str(&d).ok()).unwrap_or(serde_json::Value::Null),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}
