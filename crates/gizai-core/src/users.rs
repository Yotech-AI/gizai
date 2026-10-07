use crate::db::Db;
use crate::model::Person;
use crate::seed::handle_for;
use crate::util::org_id;
use crate::{Error, Result, ids};

pub fn list(db: &Db) -> Result<Vec<Person>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT a.id, a.name, a.handle, a.title, a.email,
               (SELECT count(*) FROM tasks t WHERE t.assignee_actor_id = a.id AND t.deleted_at IS NULL
                  AND t.state_category NOT IN ('done','cancelled'))
             FROM actors a WHERE a.kind='person' AND a.deleted_at IS NULL ORDER BY a.name COLLATE NOCASE",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Person { id: r.get(0)?, name: r.get(1)?, handle: r.get(2)?, title: r.get(3)?, email: r.get(4)?, open_tasks: r.get(5)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn create(db: &Db, actor: &str, name: &str, email: Option<&str>) -> Result<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(Error::Invalid("name is required".into()));
    }
    db.write(Some(actor), |w| {
        let base = handle_for(&name);
        let mut handle = base.clone();
        let mut n = 2;
        while w.conn().query_row("SELECT count(*) FROM actors WHERE handle=?1", [&handle], |r| r.get::<_, i64>(0))? > 0 {
            handle = format!("{base}-{n}");
            n += 1;
        }
        let id = ids::new_id();
        let org = org_id(w.conn())?;
        let email = email.map(str::trim).filter(|e| !e.is_empty());
        w.conn().execute(
            "INSERT INTO actors(id, created_at, updated_at, created_by, updated_by, org_id, kind, name, handle, email)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, 'person', ?5, ?6, ?7)",
            rusqlite::params![id, ids::now_ms(), actor, org, name, handle, email],
        )?;
        w.insert("actors", &id, serde_json::json!({"kind": "person", "name": name, "handle": handle, "email": email}))?;
        Ok(id)
    })
}
