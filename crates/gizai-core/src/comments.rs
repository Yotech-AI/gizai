use crate::db::Db;
use crate::model::Comment;
use crate::{Error, Result, ids};

pub fn list(db: &Db, task_id: &str) -> Result<Vec<Comment>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT m.id, m.author_actor_id, a.name, a.kind, m.body_md, m.run_id, m.created_at
             FROM comments m JOIN actors a ON a.id = m.author_actor_id
             WHERE m.task_id = ?1 AND m.deleted_at IS NULL ORDER BY m.created_at, m.id",
        )?;
        let rows = st.query_map([task_id], |r| {
            Ok(Comment { id: r.get(0)?, author_id: r.get(1)?, author_name: r.get(2)?, author_kind: r.get(3)?,
                         body_md: r.get(4)?, run_id: r.get(5)?, created_at: r.get(6)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn add(db: &Db, actor: &str, task_id: &str, body_md: &str, run_id: Option<&str>) -> Result<String> {
    if body_md.trim().is_empty() {
        return Err(Error::Invalid("comment is empty".into()));
    }
    db.write(Some(actor), |w| {
        crate::tasks::not_archived(w.conn(), task_id)?;
        add_in(w, actor, task_id, body_md, run_id)
    })
}

pub(crate) fn add_in(w: &crate::db::Writer, actor: &str, task_id: &str, body_md: &str, run_id: Option<&str>) -> Result<String> {
    let now = ids::now_ms();
    let id = ids::new_id();
    w.conn().execute(
        "INSERT INTO comments(id, created_at, updated_at, created_by, updated_by, task_id, author_actor_id, body_md, run_id)
         VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?3, ?5, ?6)",
        rusqlite::params![id, now, actor, task_id, body_md, run_id],
    )?;
    w.conn().execute("UPDATE tasks SET updated_at=?2 WHERE id=?1", rusqlite::params![task_id, now])?;
    w.insert("comments", &id, serde_json::json!({"task_id": task_id, "chars": body_md.chars().count()}))?;
    Ok(id)
}
