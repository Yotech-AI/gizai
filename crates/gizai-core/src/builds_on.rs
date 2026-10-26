//! Builds on (GA-228): a card that needs another card's code builds on it, and waits until that card is merged: in a
//! column of kind deploy or done (an archived card came from Done, so it counts; a cancelled card never does). The queue
//! passes over a card that waits (`workflow::waiting_for`) without moving or holding it, so it keeps its place and is
//! taken at the next pull once the last card it builds on is merged; a start by hand is refused with `waits_message`.
//! Any card of the organisation may be named, another project's too. A card builds on at most `MAX` cards, never on
//! itself, and never on a card that already builds on it, directly or through other cards (`set_in`).
use std::collections::{HashMap, HashSet, VecDeque};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::{Db, Writer};
use crate::{Error, Result, ids, tasks};

/// The most cards one card builds on.
pub const MAX: usize = 20;

/// The cards `t` waits for, as a JSON array of their identifiers: those it builds on whose column isn't of kind deploy
/// or done (`Task::waits_for`, from the cards' select).
pub(crate) const WAITS_FOR_JSON: &str = "(SELECT json_group_array(bt.identifier) FROM task_builds_on bo
      JOIN tasks bt ON bt.id = bo.builds_on_id JOIN workflow_states bs ON bs.id = bt.state_id
      WHERE bo.task_id = t.id AND bs.category NOT IN ('deploy','done'))";

/// SQL: the card `t` waits for a card it builds on that isn't merged yet (the queue's WHERE, `workflow::waiting_for`).
pub(crate) const WAITS: &str = "EXISTS (SELECT 1 FROM task_builds_on bo JOIN tasks bt ON bt.id = bo.builds_on_id
      JOIN workflow_states bs ON bs.id = bt.state_id WHERE bo.task_id = t.id AND bs.category NOT IN ('deploy','done'))";

/// A card one builds on, or one that builds on it (Needed by), as the card page, the board check, the run prompt and the
/// Team Lead's tools show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Linked {
    pub id: String,
    /// Like GA-226.
    pub identifier: String,
    pub title: String,
    pub project_id: Option<String>,
    /// Its column's name, and its kind (category).
    pub column: String,
    pub category: String,
    /// In a column of kind deploy or done (also archived, from Done): its code is on main.
    pub merged: bool,
    pub hold: Option<String>,
    pub archived: bool,
}

/// A card's Builds on and Needed by (the card page, `get_task`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildsOn {
    /// The cards it builds on, by number.
    pub builds_on: Vec<Linked>,
    /// The cards that build on it, by number.
    pub needed_by: Vec<Linked>,
}

/// The card's Builds on and Needed by.
pub fn of(db: &Db, task_id: &str) -> Result<BuildsOn> {
    db.read(|c| {
        identifier_of(c, task_id)?;
        Ok(BuildsOn { builds_on: linked(c, task_id, false)?, needed_by: linked(c, task_id, true)? })
    })
}

/// The cards the card waits for: those it builds on that aren't merged yet, by number. Empty: it waits for none.
pub fn waits_for(db: &Db, task_id: &str) -> Result<Vec<Linked>> {
    db.read(|c| waits_for_in(c, task_id))
}

/// `waits_for` on an open connection (the board check reads it in its own read).
pub(crate) fn waits_for_in(c: &Connection, task_id: &str) -> Result<Vec<Linked>> {
    Ok(linked(c, task_id, false)?.into_iter().filter(|l| !l.merged).collect())
}

/// The cards `task_id` builds on, or with `needed_by` the cards that build on it, by number.
fn linked(c: &Connection, task_id: &str, needed_by: bool) -> Result<Vec<Linked>> {
    let (me, other) = if needed_by { ("builds_on_id", "task_id") } else { ("task_id", "builds_on_id") };
    let mut st = c.prepare_cached(&format!(
        "SELECT d.id, d.identifier, d.title, d.project_id, s.name, s.category, d.hold, d.deleted_at IS NOT NULL
         FROM task_builds_on b JOIN tasks d ON d.id = b.{other} JOIN workflow_states s ON s.id = d.state_id
         WHERE b.{me} = ?1"))?;
    let mut list: Vec<Linked> = st.query_map([task_id], |r| {
        let category: String = r.get(5)?;
        Ok(Linked {
            id: r.get(0)?, identifier: r.get(1)?, title: r.get(2)?, project_id: r.get(3)?, column: r.get(4)?,
            merged: merged(&category), category, hold: r.get(6)?, archived: r.get::<_, i64>(7)? != 0,
        })
    })?.collect::<rusqlite::Result<_>>()?;
    list.sort_by_key(|l| number_key(&l.identifier));
    Ok(list)
}

/// A card in a column of this kind is merged: Deploy (merged, not deployed yet) or Done.
pub fn merged(category: &str) -> bool {
    matches!(category, "deploy" | "done")
}

/// How identifiers sort "by number": by project key, then number (GA-99 before GA-188), then as written.
fn number_key(identifier: &str) -> (String, u64, String) {
    match identifier.rsplit_once('-') {
        Some((key, n)) => (key.to_uppercase(), n.parse().unwrap_or(u64::MAX), identifier.to_string()),
        None => (identifier.to_uppercase(), 0, identifier.to_string()),
    }
}

/// Sorts card identifiers by number (`number_key`).
pub(crate) fn sort_identifiers(list: &mut [String]) {
    list.sort_by_key(|i| number_key(i));
}

/// "GA-1", "GA-1 and GA-2", "GA-1, GA-2 and GA-3".
fn and_list(items: &[String]) -> String {
    match items.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// Why a start of `card` is refused while it waits for `waits` (identifiers, by number): Run, Continue, and the Team
/// Lead's start_agent_run and continue_agent_run. "GA-227 builds on GA-226, which isn't merged yet. It starts by itself
/// once GA-226 is merged; to start it now, take GA-226 off its Builds on."
pub fn waits_message(card: &str, waits: &[String]) -> String {
    match waits {
        [one] => format!("{card} builds on {one}, which isn't merged yet. It starts by itself once {one} is merged; to start it now, \
                          take {one} off its Builds on."),
        _ => format!("{card} builds on {}, which aren't merged yet. It starts by itself once they are merged; to start it now, take them \
                      off its Builds on.", and_list(waits)),
    }
}

/// Why a card it waits for holds it up for good, when one does (the board check's `builds_on_blocked`): it is cancelled,
/// or on hold. "GA-227 waits for GA-226, which is on hold: needs_decision". A card that is simply still in work, None.
pub fn blocked_reason(card: &str, waits: &[Linked]) -> Option<String> {
    let why: Vec<String> = waits.iter().filter(|l| !l.merged).filter_map(|l| {
        if l.category == "cancelled" {
            Some(format!("{}, which is cancelled", l.identifier))
        } else {
            l.hold.as_ref().map(|h| format!("{}, which is on hold: {h}", l.identifier))
        }
    }).collect();
    (!why.is_empty()).then(|| format!("{card} waits for {}", why.join(", and for ")))
}

fn identifier_of(c: &Connection, task_id: &str) -> Result<String> {
    c.query_row("SELECT identifier FROM tasks WHERE id=?1", [task_id], |r| r.get(0)).optional()?
        .ok_or_else(|| Error::NotFound(format!("task {task_id}")))
}

/// The ids of the cards `task_id` builds on now.
fn ids_of(c: &Connection, task_id: &str) -> Result<Vec<String>> {
    let mut st = c.prepare_cached("SELECT builds_on_id FROM task_builds_on WHERE task_id=?1")?;
    Ok(st.query_map([task_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}

/// The shortest chain from `from` to `to` over what each card builds on, [from, …, to]; None when `from` doesn't build on
/// `to`, directly or through other cards.
fn chain(c: &Connection, from: &str, to: &str) -> Result<Option<Vec<String>>> {
    let mut came: HashMap<String, String> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::from([from.to_string()]);
    let mut queue = VecDeque::from([from.to_string()]);
    while let Some(x) = queue.pop_front() {
        for y in ids_of(c, &x)? {
            if !seen.insert(y.clone()) {
                continue;
            }
            came.insert(y.clone(), x.clone());
            if y == to {
                let mut path = vec![y];
                while let Some(before) = path.last().and_then(|p| came.get(p)) {
                    path.push(before.clone());
                }
                path.reverse();
                return Ok(Some(path));
            }
            queue.push_back(y);
        }
    }
    Ok(None)
}

/// The cards `task_id` builds on become `on` (card ids), the full new list: an empty one clears it. See `set_in`.
pub fn set(db: &Db, actor: &str, task_id: &str, on: &[String]) -> Result<()> {
    db.write(Some(actor), |w| set_in(w, task_id, on).map(|_| ()))
}

/// The card builds on `other` too (the card page's Add). Nothing changes when it does already.
pub fn add(db: &Db, actor: &str, task_id: &str, other: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let mut on = ids_of(w.conn(), task_id)?;
        if !on.iter().any(|x| x == other) {
            on.push(other.to_string());
            set_in(w, task_id, &on)?;
        }
        Ok(())
    })
}

/// The card no longer builds on `other` (the card page's ×). Nothing changes when it didn't.
pub fn remove(db: &Db, actor: &str, task_id: &str, other: &str) -> Result<()> {
    db.write(Some(actor), |w| {
        let on = ids_of(w.conn(), task_id)?;
        if on.iter().any(|x| x == other) {
            let rest: Vec<String> = on.into_iter().filter(|x| x != other).collect();
            set_in(w, task_id, &rest)?;
        }
        Ok(())
    })
}

/// In an open write: the cards `task_id` builds on become `on` (card ids; a card named twice counts once), the full new
/// list. Refused, and nothing changes, for an archived card (read-only), a card of another organisation or none, a
/// card that would build on itself ("GA-227 can't build on itself."), more than `MAX` cards ("GA-227 can't build on more
/// than 20 cards."), and a loop: "GA-227 can't build on GA-225: GA-225 already builds on GA-227.", with ", through
/// GA-226" before the full stop when the loop goes through other cards. The card's activity notes the new list. Returns
/// whether anything changed.
pub(crate) fn set_in(w: &Writer, task_id: &str, on: &[String]) -> Result<bool> {
    let c = w.conn();
    tasks::not_archived(c, task_id)?;
    let me = identifier_of(c, task_id)?;
    let mut list: Vec<String> = vec![];
    for id in on.iter().map(|id| id.trim()).filter(|id| !id.is_empty()) {
        if !list.iter().any(|x| x == id) {
            list.push(id.to_string());
        }
    }
    if list.iter().any(|id| id == task_id) {
        return Err(Error::Invalid(format!("{me} can't build on itself.")));
    }
    if list.len() > MAX {
        return Err(Error::Invalid(format!("{me} can't build on more than {MAX} cards.")));
    }
    let mut names: HashMap<String, String> = HashMap::new();
    for id in &list {
        let name: Option<String> = c.query_row(
            "SELECT d.identifier FROM tasks d JOIN tasks t ON t.id = ?2 WHERE d.id = ?1 AND d.org_id = t.org_id",
            [id, task_id], |r| r.get(0)).optional()?;
        names.insert(id.clone(), name.ok_or_else(|| Error::NotFound(format!("task {id}")))?);
    }
    for id in &list {
        if let Some(path) = chain(c, id, task_id)? {
            let other = &names[id];
            let through: Vec<String> = path[1..path.len() - 1].iter().map(|x| identifier_of(c, x)).collect::<Result<_>>()?;
            let mut why = format!("{me} can't build on {other}: {other} already builds on {me}");
            if !through.is_empty() {
                why.push_str(&format!(", through {}", and_list(&through)));
            }
            why.push('.');
            return Err(Error::Invalid(why));
        }
    }
    let now_on = ids_of(c, task_id)?;
    let gone: Vec<&String> = now_on.iter().filter(|x| !list.contains(x)).collect();
    let new: Vec<&String> = list.iter().filter(|x| !now_on.contains(x)).collect();
    if gone.is_empty() && new.is_empty() {
        return Ok(false);
    }
    let now = ids::now_ms();
    for id in gone {
        c.execute("DELETE FROM task_builds_on WHERE task_id=?1 AND builds_on_id=?2", [task_id, id])?;
    }
    for id in new {
        c.execute("INSERT INTO task_builds_on(task_id, builds_on_id, created_at) VALUES (?1, ?2, ?3)", rusqlite::params![task_id, id, now])?;
    }
    c.execute("UPDATE tasks SET updated_at=?2, updated_by=COALESCE(?3, updated_by), version=version+1 WHERE id=?1",
              rusqlite::params![task_id, now, w.actor()])?;
    let mut shown: Vec<String> = list.iter().map(|id| names[id].clone()).collect();
    sort_identifiers(&mut shown);
    w.update("tasks", task_id, serde_json::json!({"buildsOn": shown}))?;
    Ok(true)
}
