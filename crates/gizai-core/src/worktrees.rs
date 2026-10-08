//! The finished cards (Done or Cancelled) that may still have a worktree: Settings → Data lists and removes those
//! worktrees, and a new card of the same project can take one over. Finding and removing them on disk is the app's
//! (gizai-agents' `worktree`).
use crate::Result;
use crate::db::Db;

/// A Done or Cancelled card with a branch, in a project linked to a local repository.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishedCard {
    pub task_id: String,
    pub identifier: String,
    pub title: String,
    /// done or cancelled
    pub category: String,
    pub branch: String,
    pub project_id: String,
    pub project_name: String,
    pub project_key: String,
    /// The project's main checkout.
    pub repo_path: String,
    /// The branch new cards start from.
    pub default_branch: String,
    /// When it was finished (else last changed), Unix ms.
    pub finished_at: i64,
}

/// The finished cards that have a branch, of one project or of all; the most recently finished first. Archived cards
/// count too (they were archived from Done), so archiving never leaves a worktree behind for good.
pub fn finished(db: &Db, project_id: Option<&str>) -> Result<Vec<FinishedCard>> {
    db.read(|c| {
        let mut st = c.prepare(
            "SELECT t.id, t.identifier, t.title, t.state_category, t.branch, p.id, p.name, p.key, r.local_path,
                    coalesce(r.default_branch, 'main'), coalesce(t.completed_at, t.updated_at)
             FROM tasks t
             JOIN projects p ON p.id = t.project_id
             JOIN repos r ON r.project_id = p.id AND r.deleted_at IS NULL
             WHERE t.state_category IN ('done', 'cancelled')
               AND coalesce(t.branch, '') <> '' AND coalesce(r.local_path, '') <> ''
               AND (?1 IS NULL OR p.id = ?1)
             ORDER BY coalesce(t.completed_at, t.updated_at) DESC, t.identifier")?;
        let rows = st.query_map([project_id], |r| Ok(FinishedCard {
            task_id: r.get(0)?, identifier: r.get(1)?, title: r.get(2)?, category: r.get(3)?, branch: r.get(4)?,
            project_id: r.get(5)?, project_name: r.get(6)?, project_key: r.get(7)?, repo_path: r.get(8)?,
            default_branch: r.get(9)?, finished_at: r.get(10)?,
        }))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}
