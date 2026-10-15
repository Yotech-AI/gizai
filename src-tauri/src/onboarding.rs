//! First start (GA-162): the onboarding a new install shows. It asks your name and your team's name, which coding CLIs
//! to use (Settings → Coding CLIs: `find_clis`, `save_clis`) and whether to import MCP servers (`scan_claude_code_mcp`,
//! `import_mcp_servers`), then opens the Team Lead's welcome chat. The rest is `gizai_core::onboarding`.
use gizai_core::onboarding::{self as core_onboarding, OnboardingInput};

use crate::AppState;
use crate::chat::TurnSummary;
use crate::runs::Note;

/// Set to 1 for Gizai to start as if the onboarding was skipped: the scripts that start it on a new, empty data folder
/// (`scripts/smoke-cage.sh`, `run.sh` and the others), so they never stop at it.
pub const SKIP_ENV: &str = "GIZAI_SKIP_ONBOARDING";

/// Whether the onboarding waits: on a new install, until it is finished or skipped. One that can't be read doesn't wait:
/// the seed made a working app already.
pub fn pending(st: &AppState) -> bool {
    core_onboarding::pending(&st.db).unwrap_or_else(|e| {
        eprintln!("gizai: reading whether the first start's onboarding waits failed: {e}");
        false
    })
}

/// Skip: the onboarding no longer waits; nothing else changes.
pub fn skip(st: &AppState) -> Result<(), String> {
    if core_onboarding::skip(&st.db).map_err(|e| e.to_string())? {
        (st.notify)(Note::RowsChanged("settings"));
    }
    Ok(())
}

/// When Gizai starts (`open_state`) with `GIZAI_SKIP_ONBOARDING=1`: the onboarding is skipped, as `skip` does.
pub(crate) fn skip_at_start(db: &gizai_core::db::Db) {
    if std::env::var(SKIP_ENV).is_ok_and(|v| v.trim() == "1")
        && let Err(e) = core_onboarding::skip(db)
    {
        eprintln!("gizai: skipping the first start's onboarding ({SKIP_ENV}=1) failed: {e}");
    }
}

/// Finish: saves your name, the team's and the agents' coding CLI (`gizai_core::onboarding::finish`), then, when the
/// Team Lead can answer, starts a new chat with the welcome message (`gizai_core::onboarding::WELCOME`) as yours and the
/// Team Lead's answer to it. Returns that chat's id, and a handle that resolves when the answer has ended. None when the
/// Team Lead can't answer (`lead_can_chat`), or its chat couldn't start (why goes to stderr): the names and the CLI are
/// saved all the same.
pub async fn finish(st: &AppState, input: OnboardingInput) -> Result<Option<(String, tokio::task::JoinHandle<TurnSummary>)>, String> {
    let st2 = st.clone();
    // Off the async threads: finding Claude Code may ask your login shell.
    let can_chat = tokio::task::spawn_blocking(move || -> Result<bool, String> {
        core_onboarding::finish(&st2.db, &st2.you_id, &input).map_err(|e| e.to_string())?;
        Ok(lead_can_chat(&st2))
    }).await.map_err(|e| e.to_string())??;
    (st.notify)(Note::RowsChanged("actors"));
    (st.notify)(Note::RowsChanged("teams"));
    if !can_chat {
        return Ok(None);
    }
    match crate::chat::send(st, None, core_onboarding::WELCOME.to_string(), None).await {
        Ok(started) => Ok(Some(started)),
        Err(e) => {
            eprintln!("gizai: the welcome chat with the Team Lead could not start: {e}");
            Ok(None)
        }
    }
}

/// Whether the Team Lead can answer in a new chat: an agent has Chat on, and the coding CLI it runs on is one the chat
/// offers without a problem (`chat::clis`: a Claude Code CLI whose program is found).
fn lead_can_chat(st: &AppState) -> bool {
    let Ok(Some(lead)) = gizai_core::team::chat_agent(&st.db) else {
        return false;
    };
    let on = lead.adapter.as_deref().map(str::trim).filter(|a| !a.is_empty()).unwrap_or(gizai_core::clis::CLAUDE_CODE).to_string();
    crate::chat::clis(st).is_ok_and(|list| list.iter().any(|c| c.id == on && c.problem.is_none()))
}
