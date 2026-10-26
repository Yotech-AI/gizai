use crate::db::{Db, Writer};
use crate::model::{Project, ProjectInput};
use crate::util::{clean, org_id};
use crate::{Error, Result, ids};
use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The first team, the oldest that isn't removed: a new project's when none is given, and the team of a project (and of
/// a card without a project) that has none.
pub(crate) const FIRST_TEAM: &str = "(SELECT id FROM teams WHERE deleted_at IS NULL ORDER BY created_at LIMIT 1)";

/// The project with its task counts and its AI usage since `?1` (the start of this month): the API cost of the runs on
/// its cards, archived ones included, and how many of them have an unknown cost (`usage`). Its team comes after
/// (`with_teams`).
fn select() -> String {
    let unknown = crate::usage::unknown_cost("u");
    format!("SELECT p.id, p.client_id, c.name, p.number, p.key, p.name, p.status, p.color, p.goal_md,
    r.local_path, coalesce(r.default_branch, 'main'), p.team_id, p.budget_amount_minor, p.budget_hours,
    (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.deleted_at IS NULL AND t.state_category NOT IN ('done','cancelled')),
    (SELECT count(*) FROM tasks t WHERE t.project_id = p.id AND t.deleted_at IS NULL AND t.state_category = 'done'),
    p.updated_at, NULLIF(r.remote_url, ''), p.worktree_copy_json, p.worktree_install, p.worktree_setup,
    (SELECT COALESCE(SUM(u.cost_usd_micros), 0) FROM runs u JOIN tasks ut ON ut.id = u.task_id WHERE ut.project_id = p.id AND u.created_at >= ?1),
    (SELECT count(*) FROM runs u JOIN tasks ut ON ut.id = u.task_id WHERE ut.project_id = p.id AND u.created_at >= ?1 AND {unknown}),
    p.lead_may_merge, p.screenshots_json, p.preview_command, p.preview_start_secs, p.test_runs_at_once
  FROM projects p
  LEFT JOIN clients c ON c.id = p.client_id
  LEFT JOIN repos r ON r.project_id = p.id AND r.deleted_at IS NULL")
}

/// GA-225: each project's team, as its cards go: its own, else the first team; and that team's name.
fn with_teams(c: &Connection, mut list: Vec<Project>) -> Result<Vec<Project>> {
    let teams: Vec<(String, String)> = {
        let mut st = c.prepare("SELECT id, name FROM teams WHERE deleted_at IS NULL ORDER BY created_at")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for p in &mut list {
        if p.team_id.is_none() {
            p.team_id = teams.first().map(|t| t.0.clone());
        }
        p.team_name = teams.iter().find(|t| p.team_id.as_ref() == Some(&t.0)).map(|t| t.1.clone());
    }
    Ok(list)
}

fn row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?, client_id: r.get(1)?, client_name: r.get(2)?, number: r.get(3)?, key: r.get(4)?, name: r.get(5)?,
        status: r.get(6)?, color: r.get(7)?, goal_md: r.get(8)?, repo_path: r.get(9)?, default_branch: r.get(10)?,
        team_id: r.get(11)?, budget_amount_minor: r.get(12)?, budget_hours: r.get(13)?, open_tasks: r.get(14)?,
        done_tasks: r.get(15)?, updated_at: r.get(16)?, repo_url: r.get(17)?,
        worktree_copy: serde_json::from_str(&r.get::<_, String>(18)?).unwrap_or_default(),
        worktree_install: r.get(19)?, worktree_setup: r.get(20)?,
        ai_cost_usd_micros: r.get(21)?, ai_unknown_cost_runs: r.get(22)?, lead_may_merge: r.get::<_, i64>(23)? != 0,
        screenshots: crate::screenshots::parse_project(r.get::<_, Option<String>>(24)?.as_deref()),
        preview_command: r.get(25)?,
        preview_start_secs: r.get::<_, Option<i64>>(26)?.and_then(|s| u32::try_from(s).ok())
            .filter(|s| crate::screenshots::START_SECS.contains(s)).unwrap_or(crate::screenshots::DEFAULT_START_SECS),
        test_runs_at_once: r.get::<_, Option<i64>>(27)?.and_then(|n| u32::try_from(n).ok()).filter(|n| *n >= 1),
        team_name: None,
    })
}

/// Test runs at once (GA-212): how many of a project's cards may be in a test run at the same time.
pub const TEST_RUNS_AT_ONCE: std::ops::RangeInclusive<i64> = 1..=20;

/// The Test runs at once of `input` to save, checked: None keeps what the project has, 0 removes it, 1 to 20 sets it.
fn test_runs_field(input: &ProjectInput) -> Result<Option<i64>> {
    match input.test_runs_at_once {
        Some(n) if n != 0 && !TEST_RUNS_AT_ONCE.contains(&n) => Err(Error::Invalid(format!(
            "Test runs at once must be from {} to {}, or empty for no limit.", TEST_RUNS_AT_ONCE.start(), TEST_RUNS_AT_ONCE.end()))),
        n => Ok(n),
    }
}

/// Team Lead may merge (GA-86) is a person's switch, set in the app: a write by anyone else (an agent, Gizai) that gives
/// it is refused, and nothing changes.
fn person_sets_lead_may_merge(c: &rusqlite::Connection, actor: &str, input: &ProjectInput) -> Result<()> {
    if input.lead_may_merge.is_none() {
        return Ok(());
    }
    let kind: Option<String> = c.query_row("SELECT kind FROM actors WHERE id=?1 AND deleted_at IS NULL", [actor], |r| r.get(0)).optional()?;
    if kind.as_deref() != Some("person") {
        return Err(Error::Invalid("Team Lead may merge is switched only by a person, in the app (the project page → Edit). Nothing changed.".into()));
    }
    Ok(())
}

/// The Preview command runs on your computer as you, like the setup command: only a person sets it, in the app (the project
/// form). A write by anyone else (an agent, Gizai) that sets it is refused, and nothing changes.
fn person_sets_preview(c: &rusqlite::Connection, actor: &str, input: &ProjectInput) -> Result<()> {
    if input.preview_command.is_none() {
        return Ok(());
    }
    let kind: Option<String> = c.query_row("SELECT kind FROM actors WHERE id=?1 AND deleted_at IS NULL", [actor], |r| r.get(0)).optional()?;
    if kind.as_deref() != Some("person") {
        return Err(Error::Invalid("The Preview command is set only by a person, in the app (the project page → Edit). Nothing changed.".into()));
    }
    Ok(())
}

/// The screenshot and preview settings of `input` to save (GA-182), checked: None keeps what the project has. The
/// screenshots as JSON, the Preview command trimmed ("" removes it), the Start time.
fn preview_fields(input: &ProjectInput) -> Result<(Option<String>, Option<String>, Option<u32>)> {
    let shots = match &input.screenshots {
        Some(s) => Some(serde_json::to_string(&crate::screenshots::check_project(s)?)?),
        None => None,
    };
    if let Some(secs) = input.preview_start_secs {
        crate::screenshots::check_start_secs(secs)?;
    }
    Ok((shots, input.preview_command.as_ref().map(|c| c.trim().to_string()), input.preview_start_secs))
}

/// The start of this month (UTC), from which a project's AI usage counts.
fn month_start() -> i64 {
    crate::runs::month_start_ms(ids::now_ms())
}

pub fn list(db: &Db) -> Result<Vec<Project>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("{} WHERE p.deleted_at IS NULL ORDER BY p.number DESC", select()))?;
        let list = st.query_map([month_start()], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
        with_teams(c, list)
    })
}

pub fn get(db: &Db, id: &str) -> Result<Project> {
    db.read(|c| {
        let p = c.query_row(&format!("{} WHERE p.id = ?2", select()), rusqlite::params![month_start(), id], row)
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("project {id}")))?;
        Ok(with_teams(c, vec![p])?.remove(0))
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

/// A key for a new project called `name` that none of `taken` is: `suggest_key(name)`, else that key with a number (2, 3,
/// …) for which its last letters make room, so it is never longer than 6 characters: "ABCDEF" → "ABCDE2" … "ABCDE9",
/// "ABCD10".
pub fn free_key(name: &str, taken: &[String]) -> String {
    let base = suggest_key(name);
    let is_taken = |k: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(k));
    let mut k = base.clone();
    let mut n = 2;
    // At most 5 digits, so a letter still starts the key.
    while is_taken(&k) && n < 100_000 {
        let num = n.to_string();
        k = format!("{}{num}", &base[..base.len().min(6 - num.len())]);
        n += 1;
    }
    k
}

/// `free_key` for a new project: none of the projects has the key yet (`create` refuses one that is used).
pub fn unused_key(db: &Db, name: &str) -> Result<String> {
    let taken = db.read(|c| {
        let mut st = c.prepare("SELECT key FROM projects")?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?)
    })?;
    Ok(free_key(name, &taken))
}

fn normalise_key(key: &str) -> Result<String> {
    let k = key.trim().to_uppercase();
    if !(2..=6).contains(&k.len()) || !k.chars().all(|c| c.is_ascii_alphanumeric()) || !k.chars().next().unwrap().is_ascii_alphabetic() {
        return Err(Error::Invalid("project key must be 2–6 letters or digits, starting with a letter".into()));
    }
    Ok(k)
}

/// The paths a new worktree copies from the main checkout, tidied: one per entry, relative to the repository ("./" and
/// spaces dropped, a folder's "/" kept), each once. A path outside the repository, or its .git, is refused.
pub fn clean_copy_paths(paths: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = vec![];
    for raw in paths {
        let mut p = raw.trim();
        while let Some(rest) = p.strip_prefix("./") {
            p = rest.trim_start_matches('/');
        }
        if p.is_empty() || p == "." {
            continue;
        }
        let parts: Vec<&str> = p.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
        if p.starts_with('/') || p.starts_with('~') || parts.is_empty() || parts.contains(&"..") || parts[0] == ".git" {
            return Err(Error::Invalid(format!("copy paths are relative to the repository, like .env or node_modules/: {p} isn't one")));
        }
        let tidy = parts.join("/") + if p.ends_with('/') { "/" } else { "" };
        if !out.iter().any(|o| o.trim_end_matches('/') == tidy.trim_end_matches('/')) {
            out.push(tidy);
        }
    }
    Ok(out)
}

/// The worktree settings of `input` to save: None keeps what the project has.
fn worktree_fields(input: &ProjectInput) -> Result<(Option<String>, Option<bool>, Option<String>)> {
    let copy = match &input.worktree_copy {
        Some(list) => Some(serde_json::to_string(&clean_copy_paths(list)?)?),
        None => None,
    };
    let setup = input.worktree_setup.as_ref().map(|c| c.trim().to_string());
    Ok((copy, input.worktree_install, setup))
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
    let (copy, install, setup) = worktree_fields(&input)?;
    let (shots, preview, start_secs) = preview_fields(&input)?;
    let test_runs = test_runs_field(&input)?;
    db.write(Some(actor), |w| {
        let c = w.conn();
        person_sets_lead_may_merge(c, actor, &input)?;
        person_sets_preview(c, actor, &input)?;
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
        // The team given (GA-225), else the first team.
        let team: Option<String> = match clean(&input.team_id) {
            Some(t) => Some(team_named(c, &t)?.0),
            None => c.query_row(&format!("SELECT {FIRST_TEAM}"), [], |r| r.get(0))?,
        };
        let id = ids::new_id();
        c.execute(
            "INSERT INTO projects(id, created_at, updated_at, created_by, updated_by, org_id, client_id, number, key, name, status,
               color, goal_md, team_id, budget_amount_minor, budget_hours, worktree_copy_json, worktree_install, worktree_setup, lead_may_merge,
               screenshots_json, preview_command, preview_start_secs, test_runs_at_once)
             VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7, ?8, coalesce(?9, 'active'), ?10, ?11, ?12, ?13, ?14,
               coalesce(?15, '[]'), coalesce(?16, 1), NULLIF(?17, ''), coalesce(?18, 0), ?19, NULLIF(?20, ''), ?21, NULLIF(?22, 0))",
            rusqlite::params![id, now, actor, org, clean(&input.client_id), number, key, name, clean(&input.status),
                              clean(&input.color), clean(&input.goal_md), team, input.budget_amount_minor, input.budget_hours,
                              copy, install, setup, input.lead_may_merge, shots, preview, start_secs, test_runs],
        )?;
        w.insert("projects", &id, serde_json::to_value(&input)?)?;
        upsert_repo(w, actor, &id, &input, now)?;
        Ok(id)
    })
}

/// The key is fixed after creation (task identifiers depend on it). Another team (`team_id`, GA-225) moves the project
/// and all its cards there in the same write (`move_cards`), refused while one of its cards has a run live (`move_refused`).
pub fn update(db: &Db, actor: &str, id: &str, input: ProjectInput) -> Result<()> {
    update_with(db, actor, id, input, &[])
}

/// `update`, where `under_way` are the cards the app has a run live or starting on (`runs::working_on`): a move to
/// another team is refused while one of them is the project's, as for a live run in the runs table.
pub fn update_with(db: &Db, actor: &str, id: &str, input: ProjectInput, under_way: &[String]) -> Result<()> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(Error::Invalid("project name is required".into()));
    }
    validate_status(&input.status)?;
    let (copy, install, setup) = worktree_fields(&input)?;
    let (shots, preview, start_secs) = preview_fields(&input)?;
    let test_runs = test_runs_field(&input)?;
    db.write(Some(actor), |w| {
        person_sets_lead_may_merge(w.conn(), actor, &input)?;
        person_sets_preview(w.conn(), actor, &input)?;
        // GA-225: another team moves it. Checked before anything changes, so a refusal changes nothing.
        let to = match clean(&input.team_id) {
            Some(t) => Some(team_named(w.conn(), &t)?),
            None => None,
        };
        let moving = match to {
            Some(to) => {
                let (saved_name, from) = name_and_team(w.conn(), id)?;
                if from.0 == to.0 {
                    None
                } else {
                    move_refused(w.conn(), id, &saved_name, under_way)?;
                    Some((from, to))
                }
            }
            None => None,
        };
        let now = ids::now_ms();
        let n = w.conn().execute(
            "UPDATE projects SET client_id=?2, name=?3, status=coalesce(?4, status), color=?5, goal_md=?6,
               budget_amount_minor=?7, budget_hours=?8, updated_at=?9, updated_by=?10, version=version+1,
               worktree_copy_json=coalesce(?11, worktree_copy_json), worktree_install=coalesce(?12, worktree_install),
               worktree_setup=CASE WHEN ?13 IS NULL THEN worktree_setup ELSE NULLIF(?13, '') END,
               lead_may_merge=coalesce(?14, lead_may_merge), screenshots_json=coalesce(?15, screenshots_json),
               preview_command=CASE WHEN ?16 IS NULL THEN preview_command ELSE NULLIF(?16, '') END,
               preview_start_secs=coalesce(?17, preview_start_secs),
               test_runs_at_once=CASE WHEN ?18 IS NULL THEN test_runs_at_once ELSE NULLIF(?18, 0) END,
               team_id=coalesce(?19, team_id)
             WHERE id=?1 AND deleted_at IS NULL",
            rusqlite::params![id, clean(&input.client_id), name, clean(&input.status), clean(&input.color),
                              clean(&input.goal_md), input.budget_amount_minor, input.budget_hours, now, actor,
                              copy, install, setup, input.lead_may_merge, shots, preview, start_secs, test_runs,
                              moving.as_ref().map(|(_, to)| to.0.clone())],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("project {id}")));
        }
        let mut diff = serde_json::to_value(&input)?;
        if let Some((from, to)) = &moving {
            let cards = move_cards(w, actor, id, from, to, now)?;
            diff["team"] = serde_json::json!([from.1, to.1]);
            diff["cardsMoved"] = serde_json::json!(cards);
        }
        w.update("projects", id, diff)?;
        upsert_repo(w, actor, id, &input, now)
    })
}

/// The team `id`, checked: its id and name. A team that doesn't exist, or was removed, is refused.
fn team_named(c: &Connection, id: &str) -> Result<(String, String)> {
    c.query_row("SELECT id, name FROM teams WHERE id=?1 AND deleted_at IS NULL", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("team {id}")))
}

/// The project's name as saved, and its team (id and name): its own, else the first team.
fn name_and_team(c: &Connection, id: &str) -> Result<(String, (String, String))> {
    let (name, team): (String, Option<String>) = c
        .query_row(&format!("SELECT name, COALESCE(team_id, {FIRST_TEAM}) FROM projects WHERE id=?1 AND deleted_at IS NULL"), [id],
                   |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| Error::NotFound(format!("project {id}")))?;
    let team = team.ok_or_else(|| Error::Invalid("there is no team".into()))?;
    let team_name: String = c.query_row("SELECT name FROM teams WHERE id=?1", [&team], |r| r.get(0))?;
    Ok((name, (team, team_name)))
}

/// GA-225: a project doesn't move to another team while one of its cards has a run live or starting: one of `under_way`
/// (the app's cards with a run live or starting), or a run the runs table has as live. The refusal names the first of
/// them by number.
fn move_refused(c: &Connection, id: &str, name: &str, under_way: &[String]) -> Result<()> {
    let mut busy: Vec<String> = {
        let mut st = c.prepare(
            "SELECT t.identifier FROM tasks t WHERE t.project_id=?1
               AND (t.id IN (SELECT value FROM json_each(?2))
                    OR EXISTS (SELECT 1 FROM runs r WHERE r.task_id = t.id AND r.status IN ('queued','running','waiting_approval')))")?;
        st.query_map(rusqlite::params![id, serde_json::to_string(under_way)?], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    crate::builds_on::sort_identifiers(&mut busy);
    match busy.first() {
        Some(card) => Err(Error::Invalid(format!("Wait for {card}'s run to end, or stop it, before you move {name} to another team."))),
        None => Ok(()),
    }
}

/// GA-225: moves the project's cards, archived ones too, from team `from` to team `to` (each an id and a name): each to
/// `to`'s first column of the same category in board order, else its first Backlog column, else its first column, at
/// the end of that column and in the cards' board order. An assignee, pin or implementer that is an agent not on `to`
/// comes off; people stay. Every card's change goes in the changes log. Returns how many cards moved.
fn move_cards(w: &Writer, actor: &str, project_id: &str, from: &(String, String), to: &(String, String), now: i64) -> Result<i64> {
    let c = w.conn();
    let columns: Vec<(String, String, String)> = {
        let mut st = c.prepare("SELECT id, name, category FROM workflow_states WHERE team_id=?1 AND deleted_at IS NULL ORDER BY sort_key")?;
        st.query_map([&to.0], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let target = |category: &str| {
        columns.iter().find(|s| s.2 == category).or_else(|| columns.iter().find(|s| s.2 == "backlog")).or(columns.first())
    };
    // The cards in board order: by their column, then their place in it.
    let cards: Vec<(String, String, String, [Option<String>; 3])> = {
        let mut st = c.prepare(
            "SELECT t.id, s.name, s.category, t.assignee_actor_id, t.pinned_actor_id, t.implementer_actor_id
             FROM tasks t JOIN workflow_states s ON s.id = t.state_id WHERE t.project_id=?1 ORDER BY s.sort_key, t.sort_key, t.created_at")?;
        st.query_map([project_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, [r.get(3)?, r.get(4)?, r.get(5)?])))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    // An agent that isn't on the new team comes off; a person stays, whatever team they are on.
    let mut leaves: HashMap<String, bool> = HashMap::new();
    let mut last: HashMap<String, Option<String>> = HashMap::new();
    for (card, from_column, category, actors) in &cards {
        let (state, column, new_category) = target(category).ok_or_else(|| Error::Invalid(format!("{} has no columns", to.1)))?;
        let mut off = [false; 3];
        for (i, a) in actors.iter().enumerate() {
            let Some(a) = a else { continue };
            if !leaves.contains_key(a) {
                let gone: bool = c.query_row(
                    "SELECT EXISTS (SELECT 1 FROM actors x WHERE x.id=?1 AND x.kind='agent')
                       AND NOT EXISTS (SELECT 1 FROM team_members m WHERE m.team_id=?2 AND m.actor_id=?1 AND m.deleted_at IS NULL)",
                    [a, &to.0], |r| r.get(0))?;
                leaves.insert(a.clone(), gone);
            }
            off[i] = leaves[a];
        }
        if !last.contains_key(state) {
            let max: Option<String> = c.query_row("SELECT max(sort_key) FROM tasks WHERE state_id=?1", [state], |r| r.get(0))?;
            last.insert(state.clone(), max);
        }
        let key = crate::sortkey::key_after(last[state].as_deref());
        c.execute(
            "UPDATE tasks SET state_id=?2, state_category=?3, sort_key=?4,
               started_at = coalesce(started_at, CASE WHEN ?3 = 'in_progress' THEN ?5 END),
               completed_at = CASE WHEN ?3 = 'done' THEN coalesce(completed_at, ?5) ELSE NULL END,
               assignee_actor_id = CASE WHEN ?7 THEN NULL ELSE assignee_actor_id END,
               pinned_actor_id = CASE WHEN ?8 THEN NULL ELSE pinned_actor_id END,
               implementer_actor_id = CASE WHEN ?9 THEN NULL ELSE implementer_actor_id END,
               updated_at=?5, updated_by=?6, version=version+1 WHERE id=?1",
            rusqlite::params![card, state, new_category, key, now, actor, off[0], off[1], off[2]])?;
        let mut diff = serde_json::json!({"column": [from_column, column], "team": [from.1, to.1]});
        if off.iter().any(|o| *o) {
            diff["agentsOff"] = serde_json::json!(true);
        }
        w.update("tasks", card, diff)?;
        last.insert(state.clone(), Some(key));
    }
    Ok(cards.len() as i64)
}

/// GA-225: what moving a project to another team does, for the project form's question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMove {
    /// The project's name.
    pub project: String,
    /// The team it is in now, and the team it would move to.
    pub from: String,
    pub to: String,
    /// Its cards, archived ones included: all of them move.
    pub cards: i64,
}

/// What moving project `id` to team `team_id` would do (`TeamMove`); nothing changes.
pub fn team_move(db: &Db, id: &str, team_id: &str) -> Result<TeamMove> {
    db.read(|c| {
        let (project, from) = name_and_team(c, id)?;
        let to = team_named(c, team_id)?;
        let cards: i64 = c.query_row("SELECT count(*) FROM tasks WHERE project_id=?1", [id], |r| r.get(0))?;
        Ok(TeamMove { project, from: from.1, to: to.1, cards })
    })
}

/// The project's cards on the board (archived ones left out: no run starts on them), for the app to hold while the
/// project moves to another team.
pub fn card_ids(db: &Db, id: &str) -> Result<Vec<String>> {
    db.read(|c| {
        let mut st = c.prepare("SELECT id FROM tasks WHERE project_id=?1 AND deleted_at IS NULL")?;
        Ok(st.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
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
