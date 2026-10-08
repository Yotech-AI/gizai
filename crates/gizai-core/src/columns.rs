//! A team's columns, as set up in Team → Workflow: the agents on each (in order), Auto or Manual, the column its cards
//! go to next, the board order, and removing a column. The column decides who works its cards (`workflow`); labels
//! never do. Backlog, Review, Done and Cancelled columns take no agents and are never Auto, and an Auto column always
//! has a next column, so a finished card isn't picked up again in the same column.
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::{Db, Writer};
use crate::{Error, Result, ids, sortkey};

/// Categories whose columns take no agents and are never Auto: new and finished cards, and your review.
pub const NO_AGENTS: [&str; 4] = ["backlog", "review", "done", "cancelled"];

/// Whether a column of this category takes agents (and may be Auto).
pub fn takes_agents(category: &str) -> bool {
    !NO_AGENTS.contains(&category)
}

/// The kinds Add column offers, in plain words, with their categories.
pub const KINDS: [(&str, &str); 7] = [
    ("waiting", "ready"), ("work", "in_progress"), ("testing", "testing"), ("review", "review"), ("deploy", "deploy"), ("done", "done"),
    ("backlog", "backlog"),
];

/// "Work" or "in_progress" or "To do" → the category; None when it is neither a kind nor a category.
pub fn category_of(kind: &str) -> Option<&'static str> {
    let k = kind.trim().to_lowercase().replace([' ', '-'], "_");
    let k = match k.as_str() { "to_do" | "todo" | "ready" => "waiting", "in_progress" | "doing" => "work", other => other };
    KINDS.iter().find(|(name, cat)| *name == k || *cat == k).map(|(_, cat)| *cat)
        .or_else(|| crate::team::CATEGORIES.iter().find(|c| **c == k).copied())
}

/// One column, as stored.
#[derive(Debug, Clone)]
pub(crate) struct Col {
    pub team_id: String,
    pub name: String,
    pub category: String,
    pub auto: bool,
    pub next: Option<String>,
    pub sort_key: String,
}

pub(crate) fn col(c: &Connection, state_id: &str) -> Result<Col> {
    c.query_row(
        "SELECT team_id, name, category, auto, next_state_id, sort_key FROM workflow_states WHERE id=?1 AND deleted_at IS NULL",
        [state_id],
        |r| Ok(Col { team_id: r.get(0)?, name: r.get(1)?, category: r.get(2)?, auto: r.get::<_, i64>(3)? != 0,
                     next: r.get(4)?, sort_key: r.get(5)? }),
    ).optional()?.ok_or_else(|| Error::NotFound(format!("column {state_id}")))
}

/// The agents on a column, in order (archived and deleted agents left out; paused ones kept).
pub(crate) fn agents_of(c: &Connection, state_id: &str) -> Result<Vec<String>> {
    let mut st = c.prepare(
        "SELECT ca.actor_id FROM column_agents ca JOIN actors a ON a.id = ca.actor_id
         WHERE ca.state_id=?1 AND a.kind='agent' AND a.deleted_at IS NULL AND a.status <> 'archived' ORDER BY ca.sort_key, ca.created_at")?;
    Ok(st.query_map([state_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The first column of a category in the team, in board order: (id, name).
pub(crate) fn first_of(c: &Connection, team_id: &str, category: &str) -> Result<Option<(String, String)>> {
    Ok(c.query_row(
        "SELECT id, name FROM workflow_states WHERE team_id=?1 AND category=?2 AND deleted_at IS NULL ORDER BY sort_key LIMIT 1",
        rusqlite::params![team_id, category], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
}

/// Changes to a column; only what is given changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColumnInput {
    /// A new name, unique in the team (ignoring case).
    pub name: Option<String>,
    /// The full list of agents on it, in order.
    pub agent_ids: Option<Vec<String>>,
    /// Auto (its agents pick up its cards) or Manual (only Run).
    pub auto: Option<bool>,
    /// The column its cards go to next; "" = none.
    pub next_state_id: Option<String>,
    /// Move it right after this column; "" = to the front.
    pub after_id: Option<String>,
}

fn is_team_agent(c: &Connection, team_id: &str, actor_id: &str) -> Result<bool> {
    Ok(c.query_row(
        "SELECT count(*) FROM team_members m JOIN actors a ON a.id = m.actor_id
         WHERE m.team_id=?1 AND m.actor_id=?2 AND m.deleted_at IS NULL AND a.kind='agent' AND a.deleted_at IS NULL AND a.status <> 'archived'",
        rusqlite::params![team_id, actor_id], |r| r.get::<_, i64>(0))? > 0)
}

fn name_free(c: &Connection, team_id: &str, except: Option<&str>, name: &str) -> Result<()> {
    let clash: i64 = c.query_row(
        "SELECT count(*) FROM workflow_states WHERE team_id=?1 AND id IS NOT ?2 AND name=?3 COLLATE NOCASE AND deleted_at IS NULL",
        rusqlite::params![team_id, except, name], |r| r.get(0))?;
    if clash > 0 {
        return Err(Error::Invalid(format!("this team already has a column called {name}")));
    }
    Ok(())
}

/// A sort key that puts a column right after `after` (None: at the front), among the team's other columns.
fn key_after_column(c: &Connection, team_id: &str, moving: Option<&str>, after: Option<&str>) -> Result<String> {
    let prev: Option<String> = match after {
        Some(a) => Some(c.query_row("SELECT sort_key FROM workflow_states WHERE id=?1 AND team_id=?2 AND deleted_at IS NULL",
                                    rusqlite::params![a, team_id], |r| r.get(0)).optional()?
                         .ok_or_else(|| Error::Invalid("pick a column of this team for it to go after".into()))?),
        None => None,
    };
    let next: Option<String> = c.query_row(
        "SELECT min(sort_key) FROM workflow_states WHERE team_id=?1 AND deleted_at IS NULL AND id IS NOT ?2 AND (?3 IS NULL OR sort_key > ?3)",
        rusqlite::params![team_id, moving, prev], |r| r.get(0))?;
    Ok(sortkey::key_between(prev.as_deref(), next.as_deref()))
}

fn write_agents(w: &Writer, state_id: &str, agent_ids: &[String]) -> Result<()> {
    let c = w.conn();
    c.execute("DELETE FROM column_agents WHERE state_id=?1", [state_id])?;
    let now = ids::now_ms();
    let mut key: Option<String> = None;
    for a in agent_ids {
        let k = sortkey::key_after(key.as_deref());
        c.execute("INSERT OR IGNORE INTO column_agents(state_id, actor_id, sort_key, created_at) VALUES (?1, ?2, ?3, ?4)",
                  rusqlite::params![state_id, a, k, now])?;
        key = Some(k);
    }
    Ok(())
}

/// Sets up a column: its agents, Auto or Manual, its next column, its name and its place. Refuses agents (and Auto) on
/// Backlog, Review, Done and Cancelled columns, a column linked to itself or to another team's column, Auto without a
/// next column, an agent that isn't on the team and a name the team already uses.
pub fn set_column(db: &Db, actor: &str, state_id: &str, input: ColumnInput) -> Result<()> {
    db.write(Some(actor), |w| set_column_in(w, actor, state_id, &input))
}

pub(crate) fn set_column_in(w: &Writer, actor: &str, state_id: &str, input: &ColumnInput) -> Result<()> {
    let c = w.conn();
    let col = col(c, state_id)?;
    let mut diff = serde_json::Map::new();
    let name = match input.name.as_deref().map(str::trim) {
        Some("") => return Err(Error::Invalid("a column needs a name".into())),
        Some(n) if n != col.name => {
            name_free(c, &col.team_id, Some(state_id), n)?;
            diff.insert("name".into(), json!(n));
            n.to_string()
        }
        _ => col.name.clone(),
    };
    let agents = match &input.agent_ids {
        Some(list) => {
            let mut clean: Vec<String> = vec![];
            for a in list.iter().map(|a| a.trim()).filter(|a| !a.is_empty()) {
                if !is_team_agent(c, &col.team_id, a)? {
                    return Err(Error::Invalid("only an agent of this team can be put on its columns".into()));
                }
                if !clean.iter().any(|x| x == a) {
                    clean.push(a.to_string());
                }
            }
            if !clean.is_empty() && !takes_agents(&col.category) {
                return Err(Error::Invalid(format!(
                    "{name} takes no agents: Backlog, Review, Done and Cancelled columns are for people (new, reviewed and finished cards)")));
            }
            Some(clean)
        }
        None => None,
    };
    let next = match input.next_state_id.as_deref().map(str::trim) {
        None => col.next.clone(),
        Some("") => None,
        Some(n) => Some(n.to_string()),
    };
    if let Some(n) = next.as_deref().filter(|n| Some(*n) != col.next.as_deref()) {
        if n == state_id {
            return Err(Error::Invalid(format!("{name} can't be linked to itself: pick the column its cards go to next")));
        }
        let ok: i64 = c.query_row("SELECT count(*) FROM workflow_states WHERE id=?1 AND team_id=?2 AND deleted_at IS NULL",
                                  rusqlite::params![n, col.team_id], |r| r.get(0))?;
        if ok == 0 {
            return Err(Error::Invalid("the next column must be another column of this team".into()));
        }
    }
    let auto = input.auto.unwrap_or(col.auto);
    if auto && !takes_agents(&col.category) {
        return Err(Error::Invalid(format!("{name} can't be Auto: Backlog, Review, Done and Cancelled columns take no agents")));
    }
    if auto && next.is_none() {
        return Err(Error::Invalid(format!("an Auto column needs a next column, so a finished card isn't picked up again in {name}: pick one first")));
    }
    let sort_key = match input.after_id.as_deref().map(str::trim) {
        None => col.sort_key.clone(),
        Some("") => key_after_column(c, &col.team_id, Some(state_id), None)?,
        Some(after) if after == state_id => col.sort_key.clone(),
        Some(after) => key_after_column(c, &col.team_id, Some(state_id), Some(after))?,
    };
    if auto != col.auto {
        diff.insert("auto".into(), json!(auto));
    }
    if next != col.next {
        diff.insert("next".into(), json!(next));
    }
    if sort_key != col.sort_key {
        diff.insert("moved".into(), json!(true));
    }
    if let Some(list) = &agents {
        let now = agents_of(c, state_id)?;
        if &now != list {
            write_agents(w, state_id, list)?;
            diff.insert("agents".into(), json!(list));
        }
    }
    if diff.is_empty() {
        return Ok(());
    }
    c.execute("UPDATE workflow_states SET name=?2, auto=?3, next_state_id=?4, sort_key=?5, updated_at=?6, updated_by=?7, version=version+1 WHERE id=?1",
              rusqlite::params![state_id, name, auto as i64, next, sort_key, ids::now_ms(), actor])?;
    w.update("workflow_states", state_id, serde_json::Value::Object(diff))
}

/// Puts an agent on a column (at the end), when it isn't there yet.
pub fn add_agent(db: &Db, actor: &str, state_id: &str, agent_id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let mut list = agents_of(w.conn(), state_id)?;
        if list.iter().any(|a| a == agent_id) {
            return Ok(());
        }
        list.push(agent_id.to_string());
        set_column_in(w, actor, state_id, &ColumnInput { agent_ids: Some(list), ..Default::default() })
    })
}

/// Takes an agent off a column. Its running cards finish as usual; it gets no new cards from this column.
pub fn remove_agent(db: &Db, actor: &str, state_id: &str, agent_id: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        col(w.conn(), state_id)?;
        let n = w.conn().execute("DELETE FROM column_agents WHERE state_id=?1 AND actor_id=?2", rusqlite::params![state_id, agent_id])?;
        if n > 0 {
            w.update("workflow_states", state_id, json!({"agentOff": agent_id}))?;
        }
        Ok(())
    })
}

/// Puts an agent on the columns of `team_id` its role usually works, at the end of each: builders (every role but lead,
/// qa and devops) the first To do column and the Work column it links to (else the first In progress column), QA the
/// first Testing column, DevOps the first Deploy column, the Team Lead none.
pub(crate) fn place_new_agent(w: &Writer, team_id: &str, agent_id: &str, role: &str) -> Result<()> {
    let c = w.conn();
    let mut cols: Vec<String> = vec![];
    match role {
        "lead" => {}
        "qa" => cols.extend(first_of(c, team_id, "testing")?.map(|(id, _)| id)),
        "devops" => cols.extend(first_of(c, team_id, "deploy")?.map(|(id, _)| id)),
        _ => {
            if let Some((todo, _)) = first_of(c, team_id, "ready")? {
                let linked: Option<String> = c.query_row(
                    "SELECT n.id FROM workflow_states s JOIN workflow_states n ON n.id = s.next_state_id
                     WHERE s.id=?1 AND n.deleted_at IS NULL AND n.category='in_progress'", [&todo], |r| r.get(0)).optional()?;
                cols.push(todo);
                cols.extend(linked);
            }
            if cols.len() < 2 {
                cols.extend(first_of(c, team_id, "in_progress")?.map(|(id, _)| id));
            }
        }
    }
    let now = ids::now_ms();
    for s in cols {
        let last: Option<String> = c.query_row("SELECT max(sort_key) FROM column_agents WHERE state_id=?1", [&s], |r| r.get(0))?;
        c.execute("INSERT OR IGNORE INTO column_agents(state_id, actor_id, sort_key, created_at) VALUES (?1, ?2, ?3, ?4)",
                  rusqlite::params![s, agent_id, sortkey::key_after(last.as_deref()), now])?;
    }
    Ok(())
}

/// What removing a column does, for its confirm: the cards it holds (archived ones included), where they go by
/// default (the column before it), the columns that would lose their link (and turn Manual if they were Auto), and why
/// it can't be removed, if it can't.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Removal {
    pub cards: i64,
    /// Of those, archived.
    pub archived: i64,
    /// The column the cards go to unless another is picked: the one before it (else the one after it).
    pub default_target: Option<String>,
    /// Columns that link to it and now link to its next column.
    pub relinked: Vec<String>,
    /// Columns that would be left with no next column (or linked to themselves): they lose the link and turn Manual.
    pub unlinked: Vec<String>,
    /// Why it can't be removed now; None = it can.
    pub blocked: Option<String>,
}

pub fn removal(db: &Db, state_id: &str) -> Result<Removal> {
    db.read(|c| removal_in(c, state_id))
}

fn removal_in(c: &Connection, state_id: &str) -> Result<Removal> {
    let col = col(c, state_id)?;
    let (cards, archived): (i64, i64) = c.query_row(
        "SELECT count(*), count(deleted_at) FROM tasks WHERE state_id=?1", [state_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let default_target: Option<String> = c.query_row(
        "SELECT id FROM workflow_states WHERE team_id=?1 AND deleted_at IS NULL AND id<>?2
         ORDER BY CASE WHEN sort_key < ?3 THEN 0 ELSE 1 END, CASE WHEN sort_key < ?3 THEN sort_key END DESC, sort_key LIMIT 1",
        rusqlite::params![col.team_id, state_id, col.sort_key], |r| r.get(0)).optional()?;
    let mut relinked = vec![];
    let mut unlinked = vec![];
    let mut st = c.prepare("SELECT id, name FROM workflow_states WHERE team_id=?1 AND next_state_id=?2 AND deleted_at IS NULL AND id<>?2 ORDER BY sort_key")?;
    for row in st.query_map(rusqlite::params![col.team_id, state_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (id, name) = row?;
        if col.next.as_deref().is_some_and(|n| n != id && n != state_id) { relinked.push(name) } else { unlinked.push(name) }
    }
    let same: i64 = c.query_row("SELECT count(*) FROM workflow_states WHERE team_id=?1 AND category=?2 AND deleted_at IS NULL",
                                rusqlite::params![col.team_id, col.category], |r| r.get(0))?;
    let working: i64 = c.query_row(
        "SELECT count(*) FROM runs r JOIN tasks t ON t.id = r.task_id WHERE t.state_id=?1 AND r.status IN ('queued','running','waiting_approval')",
        [state_id], |r| r.get(0))?;
    let blocked = if col.category == "backlog" && same <= 1 {
        Some(format!("{} is the team's last Backlog column: new cards need it", col.name))
    } else if col.category == "done" && same <= 1 {
        Some(format!("{} is the team's last Done column: finished cards need it", col.name))
    } else if working > 0 {
        Some(format!("An agent is working on a card in {}: wait for the run to end or stop it", col.name))
    } else if default_target.is_none() {
        Some(format!("{} is the team's only column", col.name))
    } else {
        None
    };
    Ok(Removal { cards, archived, default_target, relinked, unlinked, blocked })
}

/// Removes a column: its cards (archived ones included) move to `target_id` keeping their hold, assignee and Testing
/// switch, the columns that linked to it link to its next column instead (or lose the link and turn Manual when that
/// would leave them with none or linked to themselves), its agents come off it, and it is marked deleted, so old
/// activity, runs and comments keep its name and the name is free again. Returns the cards that moved (not the
/// archived ones), so the caller can let the target column's agents pick them up.
pub fn remove_state(db: &Db, actor: &str, state_id: &str, target_id: &str) -> Result<Vec<String>> {
    db.write(Some(actor), |w| {
        let c = w.conn();
        let col = col(c, state_id)?;
        let r = removal_in(c, state_id)?;
        if let Some(why) = r.blocked {
            return Err(Error::Invalid(why));
        }
        if target_id == state_id {
            return Err(Error::Invalid(format!("pick another column for the cards of {}", col.name)));
        }
        let target = self::col(c, target_id).ok().filter(|t| t.team_id == col.team_id)
            .ok_or_else(|| Error::Invalid("the cards go to another column of this team: pick one".into()))?;
        let now = ids::now_ms();
        // The cards, in their order, at the end of the target column.
        let cards: Vec<(String, bool)> = {
            let mut st = c.prepare("SELECT id, deleted_at IS NOT NULL FROM tasks WHERE state_id=?1 ORDER BY sort_key, created_at")?;
            st.query_map([state_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut moved = vec![];
        let mut last: Option<String> = c.query_row("SELECT max(sort_key) FROM tasks WHERE state_id=?1", [target_id], |r| r.get(0))?;
        for (id, archived) in cards {
            let key = sortkey::key_after(last.as_deref());
            c.execute(
                "UPDATE tasks SET state_id=?2, state_category=?3, sort_key=?4,
                   started_at = coalesce(started_at, CASE WHEN ?3 = 'in_progress' THEN ?5 END),
                   completed_at = CASE WHEN ?3 = 'done' THEN coalesce(completed_at, ?5) ELSE NULL END,
                   updated_at=?5, updated_by=?6, version=version+1 WHERE id=?1",
                rusqlite::params![id, target_id, target.category, key, now, actor])?;
            w.update("tasks", &id, json!({"column": [col.name, target.name], "columnRemoved": col.name}))?;
            last = Some(key);
            if !archived {
                moved.push(id);
            }
        }
        // Links to it are bridged to its next column.
        let linking: Vec<(String, bool)> = {
            let mut st = c.prepare("SELECT id, auto FROM workflow_states WHERE team_id=?1 AND next_state_id=?2 AND deleted_at IS NULL AND id<>?2")?;
            st.query_map(rusqlite::params![col.team_id, state_id], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? != 0)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, auto) in linking {
            let bridged = col.next.clone().filter(|n| *n != id && n != state_id);
            let auto = auto && bridged.is_some();
            c.execute("UPDATE workflow_states SET next_state_id=?2, auto=?3, updated_at=?4, updated_by=?5, version=version+1 WHERE id=?1",
                      rusqlite::params![id, bridged, auto as i64, now, actor])?;
            w.update("workflow_states", &id, json!({"next": bridged, "auto": auto, "columnRemoved": col.name}))?;
        }
        c.execute("DELETE FROM column_agents WHERE state_id=?1", [state_id])?;
        c.execute("UPDATE workflow_states SET deleted_at=?2, auto=0, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1",
                  rusqlite::params![state_id, now, actor])?;
        w.delete("workflow_states", state_id)?;
        Ok(moved)
    })
}

/// Adds a column after `after_id` (`team::add_state`) and sets it up in the same write.
pub(crate) fn add_in(w: &Writer, actor: &str, team_id: &str, name: &str, after_id: &str, category: &str) -> Result<String> {
    let c = w.conn();
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("a column needs a name".into()));
    }
    if !crate::team::CATEGORIES.contains(&category) {
        return Err(Error::Invalid(format!("a column is Waiting, Work, Testing, Review, Deploy, Done or Backlog, not {category}")));
    }
    name_free(c, team_id, None, name)?;
    let sort_key = key_after_column(c, team_id, None, Some(after_id))?;
    let now = ids::now_ms();
    let id = ids::new_id();
    c.execute(
        "INSERT INTO workflow_states(id, created_at, updated_at, created_by, updated_by, team_id, name, category, sort_key)
         VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, now, actor, team_id, name, category, sort_key],
    )?;
    w.insert("workflow_states", &id, json!({"name": name, "category": category}))?;
    Ok(id)
}
