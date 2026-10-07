use crate::db::Db;
use crate::model::{Project, ProjectInput};
use crate::util::{clean, org_id};
use crate::{Error, Result, ids};
use rusqlite::{OptionalExtension, Row};

const SELECT: &str = "SELECT p.id, p.client_id, c.name, p.number, p.key, p.name, p.status, p.color, p.goal_md,
    r.local_path, coalesce(r.default_branch, 'main'), p.team_id, p.budget_amount_minor, p.budget_hours,
    (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.deleted_at IS NULL AND t.state_category NOT IN ('done','cancelled')),
    (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.deleted_at IS NULL AND t.state_category = 'done'),
    p.updated_at, NULLIF(r.remote_url, '')
  FROM projects p
  LEFT JOIN clients c ON c.id = p.client_id
  LEFT JOIN repos r ON r.project_id = p.id AND r.deleted_at IS NULL";

fn row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?, client_id: r.get(1)?, client_name: r.get(2)?, number: r.get(3)?, key: r.get(4)?, name: r.get(5)?,
        status: r.get(6)?, color: r.get(7)?, goal_md: r.get(8)?, repo_path: r.get(9)?, default_branch: r.get(10)?,
        team_id: r.get(11)?, budget_amount_minor: r.get(12)?, budget_hours: r.get(13)?, open_tasks: r.get(14)?,
        done_tasks: r.get(15)?, updated_at: r.get(16)?, repo_url: r.get(17)?,
    })
}

pub fn list(db: &Db) -> Result<Vec<Project>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("{SELECT} WHERE p.deleted_at IS NULL ORDER BY p.number DESC"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn get(db: &Db, id: &str) -> Result<Project> {
    db.read(|c| {
        c.query_row(&format!("{SELECT} WHERE p.id = ?1"), [id], row)
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("project {id}")))
    })
}

/// Unix ms → calendar year (proleptic Gregorian, UTC).
pub fn year_of(ms: i64) -> i64 {
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + if m <= 2 { 1 } else { 0 }
}

/// A project key from its name, like the project form suggests: "Kade Logistics portal" → "KLP"; one word →
/// its first 4 letters ("Webshop" → "WEBS"). Letters outside A–Z separate words. Starts with a letter.
pub fn suggest_key(name: &str) -> String {
    let upper = name.to_uppercase();
    let words: Vec<&str> = upper.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let mut k: String = match words.len() {
        0 => return "PRJ".into(),
        1 => words[0].chars().take(4).collect(),
        _ => words.iter().filter_map(|w| w.chars().next()).take(6).collect(),
    };
    if !k.starts_with(|c: char| c.is_ascii_alphabetic()) {
        k = format!("P{}", k.chars().take(5).collect::<String>());
    }
    if k.len() < 2 {
        k.push('P');
    }
    k
}

fn normalise_key(key: &str) -> Result<String> {
    let k = key.trim().to_uppercase();
    if !(2..=6).contains(&k.len()) || !k.chars().all(|c| c.is_ascii_alphanumeric()) || !k.chars().next().unwrap().is_ascii_alphabetic() {
        return Err(Error::Invalid("project key must be 2–6 letters or digits, starting with a letter".into()));
    }
    Ok(k)
}

fn validate_status(s: &Option<String>) -> Result<()> {
    if let Some(s) = clean(s) {
        if !["planned", "active", "paused", "done", "archived"].contains(&s.as_str()) {
            return Err(Error::Invalid("unknown project status".into()));
        }
    }
    Ok(())
}

pub fn create(db: &Db, actor: &str, input: ProjectInput) -> Result<String> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("project name is required".into()));
    }
    let key = normalise_key(&input.key)?;
    validate_status(&input.status)?;
    db.write(Some(actor), |w| {
        let c = w.conn();
        let org = org_id(c)?;
        if c.query_row("SELECT count(*) FROM projects WHERE org_id=?1 AND key=?2", [&org, &key], |r| r.get::<_, i64>(0))? > 0 {
            return Err(Error::Invalid(format!("project key {key} is already used")));
        }
        let now = ids::now_ms();
        let year = year_of(now);
        let mut seq: i64 = c.query_row(
            "SELECT count(*) FROM projects WHERE org_id=?1 AND number LIKE ?2", rusqlite::params![org, format!("{year}-%")],
            |r| r.get::<_, i64>(0))? + 1;
        let mut number = format!("{year}-{seq:03}");
        while c.query_row("SELECT count(*) FROM projects WHERE org_id=?1 AND number=?2", [&org, &number], |r| r.get::<_, i64>(0))? > 0 {
            seq += 1;
            number = format!("{year}-{seq:03}");
        }
        let team: Option<String> = c.query_row("SELECT id FROM teams WHERE deleted_at IS NULL ORDER BY created_at LIMIT 1", [], |r| r.get(0)).optional()?;
        let id = ids::new_id();
        c.execute(
            "INSERT INTO projects(id, created_at, updated_at, created_by, updated_by, org_id, client_id, number, key, name, status,
               color, goal_md, team_id, budget_amount_minor, budget_hours)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, coalesce(?9, 'active'), ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![id, now, actor, org, clean(&input.client_id), number, key, name, clean(&input.status),
                              clean(&input.color), clean(&input.goal_md), team, input.budget_amount_minor, input.budget_hours],
        )?;
        w.insert("projects", &id, serde_json::to_value(&input)?)?;
        upsert_repo(w, actor, &id, &input, now)?;
        Ok(id)
    })
}

/// The key is fixed after creation (task identifiers depend on it).
pub fn update(db: &Db, actor: &str, id: &str, input: ProjectInput) -> Result<()> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("project name is required".into()));
    }
    validate_status(&input.status)?;
    db.write(Some(actor), |w| {
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE projects SET client_id=?2, name=?3, status=coalesce(?4, status), color=?5, goal_md=?6,
               budget_amount_minor=?7, budget_hours=?8, updated_at=?9, updated_by=?10, version=version+1
             WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![id, clean(&input.client_id), name, clean(&input.status), clean(&input.color),
                              clean(&input.goal_md), input.budget_amount_minor, input.budget_hours, now, actor],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("project {id}")));
        }
        w.update("projects", id, serde_json::to_value(&input)?)?;
        upsert_repo(w, actor, id, &input, now)
    })
}

fn upsert_repo(w: &crate::db::Writer, actor: &str, project_id: &str, input: &ProjectInput, now: i64) -> Result<()> {
    let path = clean(&input.repo_path);
    let branch = clean(&input.default_branch).unwrap_or_else(|| "main".into());
    let link = crate::repo_url::normalize(input.repo_url.as_deref().unwrap_or(""))?;
    if link.is_some() && path.is_none() {
        return Err(Error::Invalid("choose the project's local git repository too: agents work in a checkout of it".into()));
    }
    let (url, provider, owner, name) = match &link {
        Some(l) => (l.url.clone(), l.provider.clone(), l.owner.clone(), l.name.clone()),
        None => (String::new(), "git".to_string(), None, None),
    };
    let existing: Option<String> = w.conn()
        .query_row("SELECT id FROM repos WHERE project_id=?1 AND deleted_at IS NULL", [project_id], |r| r.get(0))
        .optional()?;
    match (existing, path) {
        (Some(rid), Some(p)) => {
            w.conn().execute(
                "UPDATE repos SET local_path=?2, default_branch=?3, updated_at=?4, updated_by=?5, remote_url=?6, provider=?7, owner=?8, name=?9,
                        version=version+1 WHERE id=?1",
                rusqlite::params![rid, p, branch, now, actor, url, provider, owner, name],
            )?;
            w.update("repos", &rid, serde_json::json!({"local_path": p, "default_branch": branch, "remote_url": url}))
        }
        (Some(rid), None) => {
            w.conn().execute("UPDATE repos SET deleted_at=?2, updated_at=?2 WHERE id=?1", rusqlite::params![rid, now])?;
            w.delete("repos", &rid)
        }
        (None, Some(p)) => {
            let rid = ids::new_id();
            w.conn().execute(
                "INSERT INTO repos(id, created_at, updated_at, created_by, updated_by, project_id, provider, remote_url, default_branch, local_path, owner, name)
                 VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?7, ?8, ?5, ?6, ?9, ?10)",
                rusqlite::params![rid, now, actor, project_id, branch, p, provider, url, owner, name],
            )?;
            w.insert("repos", &rid, serde_json::json!({"local_path": p, "default_branch": branch, "remote_url": url}))
        }
        (None, None) => Ok(()),
    }
}
