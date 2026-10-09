//! Labels are tags for people, such as Must have and Could have: they never start, route or assign anything. They
//! belong to the whole organisation, and their names are unique, ignoring case.
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::db::Db;
use crate::model::Label;
use crate::{Error, Result, ids, util};

/// A label with the number of cards that carry it (archived ones left out).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LabelInfo {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub cards: i64,
}

pub fn list(db: &Db) -> Result<Vec<LabelInfo>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT l.id, l.name, l.color,
                    (SELECT count(*) FROM task_labels tl JOIN tasks t ON t.id = tl.task_id
                      WHERE tl.label_id = l.id AND tl.deleted_at IS NULL AND t.deleted_at IS NULL)
             FROM labels l WHERE l.deleted_at IS NULL ORDER BY l.name COLLATE NOCASE")?;
        Ok(st.query_map([], |r| Ok(LabelInfo { id: r.get(0)?, name: r.get(1)?, color: r.get(2)?, cards: r.get(3)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// The label with this name, ignoring case.
pub(crate) fn by_name(c: &Connection, name: &str) -> Result<Option<Label>> {
    Ok(c.query_row("SELECT id, name, color FROM labels WHERE name=?1 COLLATE NOCASE AND deleted_at IS NULL",
                   [name.trim()], |r| Ok(Label { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? })).optional()?)
}

pub fn find(db: &Db, name: &str) -> Result<Option<Label>> {
    db.read(|c| by_name(c, name))
}

fn clean_color(color: Option<&str>) -> Result<Option<String>> {
    let Some(c) = color.map(str::trim).filter(|c| !c.is_empty()) else { return Ok(None) };
    let hex = c.strip_prefix('#').unwrap_or(c);
    if !matches!(hex.len(), 3 | 6) || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(Error::Invalid(format!("a label's colour is a hex colour like #7b9bff, not {c}")));
    }
    Ok(Some(format!("#{}", hex.to_lowercase())))
}

/// Creates a label (`id` None) or renames or recolours one. `color` None keeps the current colour (a new label gets
/// none). A name another label already has, ignoring case, is refused. Returns the label's id.
pub fn save(db: &Db, actor: &str, id: Option<&str>, name: &str, color: Option<&str>) -> Result<String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("give the label a name".into()));
    }
    if name.chars().count() > 40 {
        return Err(Error::Invalid("a label's name is at most 40 characters".into()));
    }
    let color = clean_color(color)?;
    db.write(Some(actor), |w| {
        let c = w.conn();
        if let Some(other) = by_name(c, &name)? && Some(other.id.as_str()) != id {
            return Err(Error::Invalid(format!("there already is a label called {}", other.name)));
        }
        let now = ids::now_ms();
        match id {
            None => {
                let id = ids::new_id();
                c.execute("INSERT INTO labels(id, created_at, updated_at, created_by, updated_by, org_id, name, color) VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6)",
                          rusqlite::params![id, now, actor, util::org_id(c)?, name, color])?;
                w.insert("labels", &id, json!({"name": name, "color": color}))?;
                Ok(id)
            }
            Some(id) => {
                let n = c.execute(
                    "UPDATE labels SET name=?2, color=COALESCE(?3, color), updated_at=?4, updated_by=?5, version=version+1 WHERE id=?1 AND deleted_at IS NULL",
                    rusqlite::params![id, name, color, now, actor])?;
                if n == 0 {
                    return Err(Error::NotFound(format!("label {id}")));
                }
                w.update("labels", id, json!({"name": name, "color": color}))?;
                Ok(id.to_string())
            }
        }
    })
}

/// Removes a label: every card loses it. Returns how many cards carried it (archived ones included).
pub fn remove(db: &Db, actor: &str, id: &str) -> Result<i64> {
    db.write(Some(actor), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let n = c.execute("UPDATE labels SET deleted_at=?2, updated_at=?2, updated_by=?3, version=version+1 WHERE id=?1 AND deleted_at IS NULL",
                          rusqlite::params![id, now, actor])?;
        if n == 0 {
            return Err(Error::NotFound(format!("label {id}")));
        }
        let cards = c.execute("DELETE FROM task_labels WHERE label_id=?1", [id])? as i64;
        w.delete("labels", id)?;
        Ok(cards)
    })
}
