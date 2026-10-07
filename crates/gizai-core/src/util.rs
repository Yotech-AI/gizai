use crate::Result;
use rusqlite::Connection;

pub fn org_id(c: &Connection) -> Result<String> {
    Ok(c.query_row("SELECT id FROM orgs WHERE deleted_at IS NULL ORDER BY created_at LIMIT 1", [], |r| r.get(0))?)
}

/// Trim; empty becomes None.
pub fn clean(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// `base`, or `base-2`, `base-3`… whichever no actor uses yet.
pub fn unique_handle(c: &Connection, base: &str) -> Result<String> {
    let mut handle = base.to_string();
    let mut n = 2;
    while c.query_row("SELECT count(*) FROM actors WHERE handle=?1", [&handle], |r| r.get::<_, i64>(0))? > 0 {
        handle = format!("{base}-{n}");
        n += 1;
    }
    Ok(handle)
}
