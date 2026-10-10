//! A card's pull request on GitHub or Bitbucket: its link and state as Gizai last saw them, which cards the PR check
//! follows, and what a merge does to its card (Deploy, or Done for a team without a Deploy column). Talking to GitHub
//! (gh) and Bitbucket (its REST API) is gizai-agents' `github` and `bitbucket`, git push its `worktree`; the app joins
//! them.
use rusqlite::OptionalExtension;
use serde_json::json;

use crate::db::Db;
use crate::{Error, Result, ids, projects, repo_url, tasks};

/// The states Gizai keeps for a pull request.
pub const STATES: [&str; 4] = ["open", "draft", "merged", "closed"];

/// The providers a card can have a pull request on.
pub const PROVIDERS: [&str; 2] = ["github", "bitbucket"];

/// A card that can have a pull request: it has a branch, and its project a local repository linked to GitHub or
/// Bitbucket.
#[derive(Debug, Clone, PartialEq)]
pub struct PrCard {
    pub task_id: String,
    pub identifier: String,
    pub title: String,
    pub category: String,
    pub branch: String,
    pub pr_url: Option<String>,
    pub pr_state: Option<String>,
    /// The project's local repository (its main checkout).
    pub repo_path: String,
    /// Where the repository is: "github" or "bitbucket".
    pub provider: String,
    /// The repository as "owner/name" (GitHub) or "workspace/repository" (Bitbucket).
    pub repo: String,
    /// Its link, https://github.com/owner/name or https://bitbucket.org/workspace/repository.
    pub repo_url: String,
    /// The branch pull requests go into.
    pub default_branch: String,
}

impl PrCard {
    /// Whether the PR check looks at this card: an open card (not Done or Cancelled) in Review, or one with a pull
    /// request that isn't merged yet.
    pub fn followed(&self) -> bool {
        !matches!(self.category.as_str(), "done" | "cancelled")
            && (self.category == "review" || (self.pr_url.is_some() && self.pr_state.as_deref() != Some("merged")))
    }
}

/// The card's pull request details, or why it can't have one: its project needs a GitHub or Bitbucket link (and so a
/// local repository), and the card a branch (an agent's first run makes it).
pub fn card(db: &Db, task_id: &str) -> Result<PrCard> {
    let t = tasks::get(db, task_id)?;
    let project_id = t.project_id.clone().ok_or_else(|| Error::Invalid(format!("{} has no project", t.identifier)))?;
    let p = projects::get(db, &project_id)?;
    let link = match p.repo_url.as_deref() {
        Some(u) => repo_url::normalize(u)?.filter(|l| PROVIDERS.contains(&l.provider.as_str())),
        None => None,
    };
    let (Some(link), Some(repo_path)) = (link, p.repo_path.clone().filter(|r| !r.trim().is_empty())) else {
        return Err(Error::Invalid(format!("Link {} to its GitHub repository first, or to its Bitbucket repository (project page → Edit)", p.name)));
    };
    let Some(repo) = link.full_name() else {
        return Err(Error::Invalid(format!("{} isn't a GitHub or Bitbucket repository link", link.url)));
    };
    let branch = t.branch.clone().filter(|b| !b.trim().is_empty())
        .ok_or_else(|| Error::Invalid(format!("{} has no branch yet: an agent makes one when it first works on the card", t.identifier)))?;
    Ok(PrCard {
        task_id: t.id, identifier: t.identifier, title: t.title, category: t.state_category, branch, pr_url: t.pr_url, pr_state: t.pr_state,
        repo_path, provider: link.provider, repo, repo_url: link.url, default_branch: p.default_branch,
    })
}

/// The cards the PR check looks at now (see `PrCard::followed`).
pub fn to_check(db: &Db) -> Result<Vec<PrCard>> {
    let ids: Vec<String> = db.read(|c| {
        let mut st = c.prepare(
            "SELECT t.id FROM tasks t
             JOIN projects p ON p.id = t.project_id AND p.deleted_at IS NULL
             JOIN repos r ON r.project_id = p.id AND r.deleted_at IS NULL
             WHERE t.deleted_at IS NULL AND coalesce(t.branch, '') <> ''
               AND r.provider IN ('github', 'bitbucket') AND t.state_category NOT IN ('done', 'cancelled')
             ORDER BY t.sort_key, t.created_at")?;
        Ok(st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    })?;
    Ok(ids.iter().filter_map(|id| card(db, id).ok()).filter(PrCard::followed).collect())
}

fn check_state(state: &str) -> Result<()> {
    if STATES.contains(&state) { Ok(()) } else { Err(Error::Invalid(format!("unknown pull request state {state}"))) }
}

/// Records the card's pull request: by `actor` (you, opening it), or by Gizai itself (None: the PR check). The
/// activity gets an entry only when the link or the state changed; `opened` marks the one opened from Gizai. Returns
/// whether anything changed.
pub fn record(db: &Db, actor: Option<&str>, task_id: &str, url: &str, state: &str, opened: bool) -> Result<bool> {
    check_state(state)?;
    db.write(actor, |w| {
        let c = w.conn();
        let (old_url, old_state): (Option<String>, Option<String>) = c
            .query_row("SELECT pr_url, pr_state FROM tasks WHERE id=?1 AND deleted_at IS NULL", [task_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;
        if old_url.as_deref() == Some(url) && old_state.as_deref() == Some(state) {
            return Ok(false);
        }
        c.execute("UPDATE tasks SET pr_url=?2, pr_state=?3, updated_at=?4, updated_by=coalesce(?5, updated_by), version=version+1 WHERE id=?1",
                  rusqlite::params![task_id, url, state, ids::now_ms(), actor])?;
        let mut diff = json!({"pullRequest": url, "prState": state});
        if opened {
            diff["opened"] = json!(true);
        }
        w.update("tasks", task_id, diff)?;
        Ok(true)
    })
}

/// GitHub or Bitbucket merged the card's pull request: records it and moves a card in Review to Review's next column (without one,
/// to the team's first Deploy column, else Done; a card in another open column goes there too). A card that is already
/// in Deploy, Done or Cancelled stays. On an Auto column its agents pick it up (the caller pulls the queue); on a
/// Manual one nothing starts. The activity says Gizai did it; `by` is the person it did it for (the card's
/// `updated_by`). Returns the column it moved to.
pub fn merged(db: &Db, by: &str, task_id: &str, url: &str) -> Result<Option<String>> {
    db.write(None, |w| {
        let c = w.conn();
        let exists: i64 = c.query_row("SELECT count(*) FROM tasks WHERE id=?1 AND deleted_at IS NULL", [task_id], |r| r.get(0))?;
        if exists == 0 {
            return Err(Error::NotFound(format!("task {task_id}")));
        }
        c.execute("UPDATE tasks SET pr_url=?2, pr_state='merged', updated_at=?3, updated_by=?4, version=version+1 WHERE id=?1",
                  rusqlite::params![task_id, url, ids::now_ms(), by])?;
        w.update("tasks", task_id, json!({"pullRequest": url, "prState": "merged"}))?;
        let Some((to_id, name)) = crate::workflow::merge_target(c, task_id)? else { return Ok(None) };
        tasks::move_in(w, by, task_id, &to_id, None)?;
        Ok(Some(name))
    })
}

/// Notes in the card's activity, as Gizai, what happened to its worktree and branch after the merge (one plain
/// sentence).
pub fn note_cleanup(db: &Db, task_id: &str, sentence: &str) -> Result<()> {
    db.write(None, |w| w.update("tasks", task_id, json!({"cleanup": sentence})))
}

/// Notes in the card's activity that `actor` (the Team Lead's merge_pull_request, GA-86) merged its pull request `url`,
/// with `head` as its latest commit. The card itself changes only when the PR check sees the merge (`merged`), as when
/// you merge.
pub fn note_merged_by(db: &Db, actor: &str, task_id: &str, url: &str, head: &str) -> Result<()> {
    db.write(Some(actor), |w| w.update("tasks", task_id, json!({"pullRequest": url, "merged": true, "head": head})))
}

/// A release under way in the project (GA-86): one of its cards in a Deploy column, assigned to an agent with role
/// devops (a release card, like "Release Gizai v0.6.0"). Main mustn't move under it, so the Team Lead doesn't merge
/// then. The card's identifier and the agent's name.
pub fn release_under_way(db: &Db, project_id: &str) -> Result<Option<(String, String)>> {
    db.read(|c| release_in(c, project_id))
}

/// `release_under_way` on an open connection (the board check reads it in its own read).
pub(crate) fn release_in(c: &rusqlite::Connection, project_id: &str) -> Result<Option<(String, String)>> {
    Ok(c.query_row(
        "SELECT t.identifier, a.name FROM tasks t
         JOIN actors a ON a.id = t.assignee_actor_id AND a.kind = 'agent' AND a.deleted_at IS NULL
         WHERE t.project_id = ?1 AND t.deleted_at IS NULL AND t.state_category = 'deploy'
           AND EXISTS (SELECT 1 FROM team_members m WHERE m.actor_id = a.id AND m.deleted_at IS NULL AND m.role_key = 'devops')
         ORDER BY t.sort_key, t.created_at LIMIT 1",
        [project_id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?)
}
