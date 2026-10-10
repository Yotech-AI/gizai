//! Project documents: Markdown with an append-only version history. Memory notes are docs too (kind `memory`, see
//! `memory`): a save keeps their links and refuses secrets, and a project's list never shows them.
use crate::db::{Db, Writer};
use crate::model::{Doc, DocVersion};
use crate::{Error, Result, ids, util};

fn doc_from(r: &rusqlite::Row, with_body: bool) -> rusqlite::Result<Doc> {
    Ok(Doc {
        id: r.get(0)?,
        project_id: r.get(1)?,
        title: r.get(2)?,
        body_md: if with_body { r.get(3)? } else { String::new() },
        current_version: r.get(4)?,
        updated_at: r.get(5)?,
        kind: r.get(6)?,
        path: r.get(7)?,
    })
}

const COLS: &str = "id, project_id, title, body_md, current_version, updated_at, kind, path";

pub fn list(db: &Db, project_id: &str) -> Result<Vec<Doc>> {
    db.read(|c| {
        let mut st = c.prepare(&format!(
            "SELECT {COLS} FROM docs WHERE project_id = ?1 AND kind = 'doc' AND deleted_at IS NULL ORDER BY updated_at DESC, title"
        ))?;
        let rows = st.query_map([project_id], |r| doc_from(r, false))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn get(db: &Db, id: &str) -> Result<Doc> {
    db.read(|c| {
        c.query_row(&format!("SELECT {COLS} FROM docs WHERE id = ?1 AND deleted_at IS NULL"), [id], |r| doc_from(r, true))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("doc {id}")),
                e => e.into(),
            })
    })
}

fn clean_title(title: &str) -> Result<String> {
    let t = title.trim();
    if t.is_empty() {
        return Err(Error::Invalid("give the doc a title".into()));
    }
    Ok(t.to_string())
}

/// New empty doc (version 1) in a project.
pub fn create(db: &Db, actor: &str, project_id: &str, title: &str) -> Result<String> {
    let title = clean_title(title)?;
    db.write(Some(actor), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute(
            "INSERT INTO docs(id, created_at, updated_at, created_by, updated_by, org_id, project_id, title, body_md, current_version)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, '', 1)",
            rusqlite::params![id, now, actor, util::org_id(c)?, project_id, title],
        )?;
        c.execute(
            "INSERT INTO doc_versions(id, created_at, doc_id, version, body_md, author_actor_id) VALUES (?1, ?2, ?3, 1, '', ?4)",
            rusqlite::params![ids::new_id(), now, id, actor],
        )?;
        w.insert("docs", &id, serde_json::json!({"title": title, "project_id": project_id}))?;
        Ok(id)
    })
}

/// Saves a new version. `base_version` is the version the editor started from; if someone saved in
/// between, nothing is written and the caller gets an error. Returns the new version number.
pub fn save(db: &Db, actor: &str, id: &str, body_md: &str, base_version: i64) -> Result<i64> {
    db.write(Some(actor), |w| save_in(w, actor, id, body_md, base_version, None))
}

/// `save` inside a write that is open. `run_id`: the agent run that wrote it, kept on the version. Every save rebuilds
/// the doc's links (`doc_links`); a memory note with a secret in it is refused.
pub(crate) fn save_in(w: &Writer, actor: &str, id: &str, body_md: &str, base_version: i64, run_id: Option<&str>) -> Result<i64> {
    let c = w.conn();
    let (current, kind, path): (i64, String, Option<String>) = c
        .query_row("SELECT current_version, kind, path FROM docs WHERE id = ?1 AND deleted_at IS NULL", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("doc {id}")),
            e => e.into(),
        })?;
    let memory = kind == "memory";
    if current != base_version {
        return Err(Error::Invalid(if memory {
            format!("{} changed since it was read (it is at version {current} now, not {base_version}): read it again and save your change \
                     on top of that version", path.unwrap_or_default())
        } else {
            "doc changed since you opened it".into()
        }));
    }
    if memory {
        crate::memory::refuse_secrets(body_md)?;
    }
    let next = current + 1;
    let now = ids::now_ms();
    c.execute(
        "UPDATE docs SET body_md = ?2, current_version = ?3, updated_at = ?4, updated_by = ?5, version = version + 1 WHERE id = ?1",
        rusqlite::params![id, body_md, next, now, actor],
    )?;
    c.execute(
        "INSERT INTO doc_versions(id, created_at, doc_id, version, body_md, author_actor_id, run_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![ids::new_id(), now, id, next, body_md, actor, run_id],
    )?;
    crate::memory::index_links(w, id, body_md)?;
    w.update("docs", id, serde_json::json!({"version": next, "chars": body_md.chars().count()}))?;
    Ok(next)
}

/// A new title. A memory note keeps its folder and moves (`memory`): the links to it follow.
pub fn rename(db: &Db, actor: &str, id: &str, title: &str) -> Result<()> {
    let title = clean_title(title)?;
    db.write(Some(actor), |w| {
        let kind: Option<String> = w.conn().query_row("SELECT kind FROM docs WHERE id = ?1 AND deleted_at IS NULL", [id], |r| r.get(0))
            .map(Some).or_else(|e| if matches!(e, rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(e) })?;
        if kind.as_deref() == Some("memory") {
            return crate::memory::retitle_in(w, actor, id, &title);
        }
        let n = w.conn().execute(
            "UPDATE docs SET title = ?2, updated_at = ?3, updated_by = ?4, version = version + 1 WHERE id = ?1 AND deleted_at IS NULL",
            rusqlite::params![id, title, ids::now_ms(), actor],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("doc {id}")));
        }
        w.update("docs", id, serde_json::json!({"title": title}))
    })
}

/// Newest first.
pub fn versions(db: &Db, id: &str) -> Result<Vec<DocVersion>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT v.version, a.name, v.created_at FROM doc_versions v LEFT JOIN actors a ON a.id = v.author_actor_id
             WHERE v.doc_id = ?1 ORDER BY v.version DESC",
        )?;
        let rows = st.query_map([id], |r| Ok(DocVersion { version: r.get(0)?, author_name: r.get(1)?, created_at: r.get(2)? }))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn version_body(db: &Db, id: &str, version: i64) -> Result<String> {
    db.read(|c| {
        c.query_row("SELECT body_md FROM doc_versions WHERE doc_id = ?1 AND version = ?2", rusqlite::params![id, version], |r| r.get(0))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("doc {id} version {version}")),
                e => e.into(),
            })
    })
}
