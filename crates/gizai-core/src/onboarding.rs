//! First start (GA-162): a new install asks your name and your team's name, which coding CLI the agents run on and
//! whether to import MCP servers, then opens the Team Lead's chat with a welcome message. The seed has made everything
//! already (`seed::ensure_seed_with_agents`), so a closed or skipped onboarding still leaves a working app: finishing only
//! renames, moves the agents to a CLI and starts the chat (the app starts it, with `WELCOME`).
use serde::{Deserialize, Serialize};

use crate::db::{Db, Writer};
use crate::{Error, Result, clis, ids, seed, settings};

/// The setting that says the onboarding waits: `"pending"`, written by a new install's seed
/// (`seed::ensure_seed_with_agents`) and removed when the onboarding is finished or skipped. `seed::ensure_seed` (tests
/// and demo data) doesn't write it, and a database seeded before never gets it.
pub const KEY: &str = "onboarding";
const PENDING: &str = "pending";

/// The welcome chat's first message, yours, word for word.
pub const WELCOME: &str = "Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else.";

/// What the onboarding asks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OnboardingInput {
    pub you_name: String,
    pub team_name: String,
    /// The id of a CLI in Settings → Coding CLIs for the Backend, Frontend, QA and DevOps Agents; None or empty: they stay
    /// where the seed put them.
    pub agents_cli: Option<String>,
}

/// Whether the onboarding waits: on a new install, until it is finished or skipped.
pub fn pending(db: &Db) -> Result<bool> {
    db.read(pending_in)
}

fn pending_in(c: &rusqlite::Connection) -> Result<bool> {
    Ok(settings::get_in::<String>(c, KEY)?.as_deref() == Some(PENDING))
}

/// The seed's part: the onboarding waits from a new install's first start, in the seed's own write.
pub(crate) fn set_pending_in(w: &Writer) -> Result<()> {
    settings::set_in(w, KEY, &PENDING)
}

/// Skip: the onboarding no longer waits, and nothing else changes. Returns whether it waited.
pub fn skip(db: &Db) -> Result<bool> {
    db.write(None, |w| clear_in(w))
}

fn clear_in(w: &Writer) -> Result<bool> {
    Ok(w.conn().execute("DELETE FROM settings WHERE key=?1 AND org_id=''", [KEY])? > 0)
}

/// Finishes the onboarding in one write: your name and its handle on you (`you_id`, the seed's person), the team's name
/// on the first team, the Backend, Frontend, QA and DevOps Agents on `agents_cli`, and the onboarding no longer waits.
/// The names are trimmed. Moved agents get the CLI's permission default and its own model and effort, as the seed gives
/// them: acceptEdits on Claude Code, the CLI's own default on the others (Codex's workspace-write sandbox, Gemini's
/// auto_edit, none on Other). The Team Lead stays where it is, on Claude Code, where the chat runs. Nothing to set up,
/// an empty name and an unknown CLI fail and change nothing. Starting the welcome chat is the app's.
pub fn finish(db: &Db, you_id: &str, input: &OnboardingInput) -> Result<()> {
    let you_name = input.you_name.trim();
    let team_name = input.team_name.trim();
    let cli = input.agents_cli.as_deref().map(str::trim).filter(|c| !c.is_empty());
    db.write(Some(you_id), |w| {
        let c = w.conn();
        if !pending_in(c)? {
            return Err(Error::Invalid("Nothing to set up".into()));
        }
        if you_name.is_empty() {
            return Err(Error::Invalid("Your name can't be empty".into()));
        }
        if team_name.is_empty() {
            return Err(Error::Invalid("The team's name can't be empty".into()));
        }
        let cli = match cli {
            Some(id) => Some((id, clis::kind_in(c, id)?.ok_or_else(|| Error::Invalid(format!("No coding CLI with the id {id}")))?)),
            None => None,
        };
        let now = ids::now_ms();

        // You: the handle comes from the name, with -2, -3… when another actor (an agent called the same) has it.
        let base = seed::handle_for(you_name);
        let mut handle = base.clone();
        let mut n = 2;
        while c.query_row("SELECT count(*) FROM actors WHERE handle=?1 AND id<>?2", rusqlite::params![handle, you_id], |r| r.get::<_, i64>(0))? > 0 {
            handle = format!("{base}-{n}");
            n += 1;
        }
        let found = c.execute(
            "UPDATE actors SET name=?2, handle=?3, updated_at=?4, updated_by=?1, version=version+1 WHERE id=?1 AND kind='person' AND deleted_at IS NULL",
            rusqlite::params![you_id, you_name, handle, now],
        )?;
        if found == 0 {
            return Err(Error::NotFound(format!("person {you_id}")));
        }
        w.update("actors", you_id, serde_json::json!({"name": you_name, "handle": handle}))?;

        let team_id: String = c.query_row("SELECT id FROM teams WHERE deleted_at IS NULL ORDER BY created_at LIMIT 1", [], |r| r.get(0))?;
        c.execute("UPDATE teams SET name=?2, updated_at=?3, updated_by=?4, version=version+1 WHERE id=?1",
                  rusqlite::params![team_id, team_name, now, you_id])?;
        w.update("teams", &team_id, serde_json::json!({"name": team_name}))?;

        if let Some((cli_id, kind)) = cli {
            let mode = clis::permission_modes(&kind).first().copied().unwrap_or_default();
            // Every default agent's role but the Team Lead's; the agent with Chat stays too.
            let roles: Vec<&str> = seed::DEFAULT_AGENTS.iter().map(|(_, role)| *role).filter(|r| *r != "lead").collect();
            let agents: Vec<String> = {
                let mut st = c.prepare(
                    "SELECT m.actor_id FROM team_members m JOIN actors a ON a.id = m.actor_id JOIN agent_configs g ON g.actor_id = a.id
                     WHERE m.team_id=?1 AND m.deleted_at IS NULL AND a.kind='agent' AND a.deleted_at IS NULL AND COALESCE(g.chat_enabled, 0) = 0
                       AND m.role_key IN (SELECT value FROM json_each(?2))
                     ORDER BY m.created_at")?;
                st.query_map(rusqlite::params![team_id, serde_json::to_string(&roles)?], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for agent in agents {
                c.execute(
                    "UPDATE agent_configs SET adapter=?2, permission_mode=?3, model=NULL, effort=NULL, updated_at=?4, version=version+1 WHERE actor_id=?1",
                    rusqlite::params![agent, cli_id, mode, now],
                )?;
                w.update("agent_configs", &agent, serde_json::json!({"adapter": cli_id, "permission_mode": mode, "model": null, "effort": null}))?;
            }
        }

        clear_in(w)?;
        Ok(())
    })
}
