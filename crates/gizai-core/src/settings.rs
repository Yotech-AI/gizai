//! App-wide settings as JSON values (Claude Code path, run limits, pause switch).
use rusqlite::OptionalExtension;
use serde::{Serialize, de::DeserializeOwned};

use crate::db::Db;
use crate::{Result, ids};

pub fn get<T: DeserializeOwned>(db: &Db, key: &str) -> Result<Option<T>> {
    db.read(|c| get_in(c, key))
}

/// `get` on a connection that is open already (inside a write).
pub(crate) fn get_in<T: DeserializeOwned>(c: &rusqlite::Connection, key: &str) -> Result<Option<T>> {
    let raw: Option<String> = c.query_row("SELECT value_json FROM settings WHERE key=?1 AND org_id=''", [key], |r| r.get(0)).optional()?;
    Ok(match raw { Some(j) => serde_json::from_str(&j).ok(), None => None })
}

pub fn set<T: Serialize>(db: &Db, key: &str, value: &T) -> Result<()> {
    db.write(None, |w| set_in(w, key, value))
}

/// `set` inside a write that is open already.
pub(crate) fn set_in<T: Serialize>(w: &crate::db::Writer, key: &str, value: &T) -> Result<()> {
    let json = serde_json::to_string(value)?;
    w.conn().execute(
        "INSERT INTO settings(key, org_id, value_json, updated_at) VALUES (?1, '', ?2, ?3)
         ON CONFLICT(key, org_id) DO UPDATE SET value_json=excluded.value_json, updated_at=excluded.updated_at",
        rusqlite::params![key, json, ids::now_ms()])?;
    Ok(())
}
