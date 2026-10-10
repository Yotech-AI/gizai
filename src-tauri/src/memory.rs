//! Memory in Gizai's prompts (GA-19): the Memory block of the Team Lead's chat answers, board checks and task runs and
//! of every agent's task run (`gizai_core::memory::prompt_block`), and the `learned` lines a run's result adds to its
//! agent's own notes. Plain text in the prompt and a list on the result line, so it works the same on every coding CLI.
//! At start-up, the notes agents kept in Claude Code's own memory come in (GA-85, `gizai_core::memory_import`).
use gizai_core::memory::{self, Block, Context, Who};
use gizai_core::memory_import::{self, Report};
use gizai_core::model::Project;
use gizai_core::team::Member;

use crate::AppState;

fn you(st: &AppState) -> String {
    gizai_core::users::list(&st.db).ok().and_then(|l| l.into_iter().find(|p| p.id == st.you_id)).map(|p| p.name)
        .unwrap_or_else(|| "the user".into())
}

/// The Team Lead's Memory block for a chat answer or a board check: `Team Lead/Notes` (made the first time) and its other
/// notes in full, then the paths of the rest. Empty when memory is off (Settings → Runs, or its agent form).
pub fn lead_block(st: &AppState, agent: &Member) -> String {
    if !memory::agent_uses(&st.db, &agent.actor_id) {
        return String::new();
    }
    if let Err(e) = memory::ensure_lead_notes(&st.db, &agent.actor_id, &you(st)) {
        eprintln!("gizai: making Team Lead/Notes failed: {e}");
    }
    match memory::prompt_block(&st.db, &Who::Lead(agent.actor_id.clone()), &Context::default()) {
        Ok(b) => b.text,
        Err(e) => {
            eprintln!("gizai: reading memory for the Team Lead's prompt failed: {e}");
            String::new()
        }
    }
}

/// The Memory block of a new task run of `agent` (role `role`) on a card of `project`: the Team Lead's as in chat, another
/// agent's from its own notes and the shared notes for this project, its client and its role. Empty when memory is off.
pub fn run_block(st: &AppState, agent: &Member, role: &str, project: &Project) -> Block {
    if !memory::agent_uses(&st.db, &agent.actor_id) {
        return Block::default();
    }
    let lead = agent.is_lead || gizai_core::team::chat_agent(&st.db).ok().flatten().is_some_and(|a| a.actor_id == agent.actor_id);
    let who = if lead { Who::Lead(agent.actor_id.clone()) } else { Who::Agent(agent.actor_id.clone()) };
    if lead && let Err(e) = memory::ensure_lead_notes(&st.db, &agent.actor_id, &you(st)) {
        eprintln!("gizai: making Team Lead/Notes failed: {e}");
    }
    let cx = Context { role: role.to_string(), project: Some((project.key.clone(), project.name.clone())), client: project.client_name.clone() };
    memory::prompt_block(&st.db, &who, &cx).unwrap_or_else(|e| {
        eprintln!("gizai: reading memory for {}'s run failed: {e}", agent.name);
        Block::default()
    })
}

/// Claude Code's own memory notes into Gizai's memory, once per file (GA-85): the Markdown files in `projects/*/memory/`
/// of every Claude Code account in Settings → Coding CLIs (its CLAUDE_CONFIG_DIR, else Gizai's, else ~/.claude) become
/// notes in `Team Lead/Imported/`, from the folders of Gizai's own places only (`memory_import::places`: the projects'
/// linked folders, the cards' worktrees, the Team Lead's code copies and working folder), and `Team Lead/Notes` gets an
/// open thread to sort them. When Gizai starts, after the migrations; your home folder is $HOME (your profile folder on
/// Windows).
pub fn import_claude(st: &AppState) -> Report {
    let home = gizai_core::clis::home();
    let inherited = |name: &str| std::env::var(name).ok();
    let day = crate::tools::ymd(gizai_core::ids::now_ms());
    let done = memory_import::claude_dirs(&st.db, &home, &inherited)
        .and_then(|dirs| Ok((dirs, memory_import::places(&st.db, &st.data_dir)?)))
        .and_then(|(dirs, places)| memory_import::import(&st.db, &dirs, &places, &home, &st.you_id, &you(st), &day));
    match done {
        Ok(r) => {
            if !r.imported.is_empty() || !r.skipped.is_empty() {
                eprintln!("gizai: {} notes came from Claude Code's own memory into Team Lead/Imported/ ({} files could not; {} folders of other \
paths, with {} notes, were left out)", r.imported.len(), r.skipped.len(), r.left_out.folders, r.left_out.notes);
                (st.notify)(crate::runs::Note::RowsChanged("docs"));
            }
            r
        }
        Err(e) => {
            eprintln!("gizai: importing Claude Code's own memory notes failed: {e}");
            Report::default()
        }
    }
}

/// Saves the `learned` lines on a run's result line (`outcome::learned`) in its agent's own notes, dated and with the
/// card, the run on the note's version. Nothing when there are none or memory is off for the agent. Returns what to note
/// in the run's log (lines left out, a failed save).
pub fn save_learned(st: &AppState, run_id: &str, agent_id: &str, card: &str, lines: &[String]) -> Vec<String> {
    if lines.is_empty() || !memory::agent_uses(&st.db, agent_id) {
        return vec![];
    }
    match memory::learned(&st.db, agent_id, card, lines, Some(run_id), &crate::tools::ymd(gizai_core::ids::now_ms())) {
        Ok((saved, mut notes)) => {
            if saved.is_some() {
                (st.notify)(crate::runs::Note::RowsChanged("docs"));
            }
            notes.iter_mut().for_each(|n| *n = format!("Memory: {n}."));
            notes
        }
        Err(e) => vec![format!("Memory: saving what this run learned failed: {e}")],
    }
}
