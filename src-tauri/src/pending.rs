//! Pending actions (GA-102): what a chat answer couldn't do after a tool from outside Gizai (`tools::NOT_AFTER_OUTSIDE`)
//! waits under that answer until you approve it with one click or discard it. Approve runs exactly the stored call, as the
//! Team Lead in its chat, with every other rule still in force (`tools::call_approved`), and only once; Discard runs
//! nothing. Either way a note of Gizai's in the chat says what happened, and the Team Lead hears it at the start of its
//! next answer there (`for_turn`). A decision doesn't start an answer, so once nothing waits and the Team Lead isn't
//! answering, a last note says its answer is done (GA-154, `ANSWER_DONE`). Only you decide: the UI's commands come here,
//! and no tool of the Team Lead's lists, approves or discards a pending action. A board check never makes one: it uses no
//! tool from outside Gizai.
use std::sync::Mutex;

use gizai_core::pending::{self as core, NewPending, PendingAction};
use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::runs::Note;
use crate::tools::preview::{self, Preview};

/// A pending action as the Chat page shows it: the call, and what it would do as things are now.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingView {
    #[serde(flatten)]
    pub action: PendingAction,
    pub preview: Preview,
}

/// Open screens look again: the chat's calls that wait, and its count of them (Chat → Recent and the Inbox).
fn changed(st: &AppState) {
    (st.notify)(Note::RowsChanged("pending_actions"));
    (st.notify)(Note::RowsChanged("chat_threads"));
}

/// Keeps the call `tool` (with `args`) that the Team Lead `actor` made in the chat `thread_id` and that was refused because
/// that answer used `outside`, a tool from outside Gizai. `run_id`: the answer's run (else the answer under way in the chat,
/// if any). A newer memory_write or memory_append for the same note replaces the older one.
pub(crate) fn keep(st: &AppState, actor: &str, thread_id: &str, run_id: Option<&str>, tool: &str, args: &Map<String, Value>, outside: &str)
    -> Result<PendingAction, String> {
    let run_id = run_id.filter(|r| !r.is_empty()).map(str::to_string).or_else(|| crate::chat::live_run(st, thread_id));
    let note_key = preview::note_key(st, actor, thread_id, tool, args);
    let a = core::keep(&st.db, NewPending { thread_id: thread_id.into(), run_id, actor_id: actor.into(), tool: tool.into(),
                                            args: Value::Object(args.clone()), outside_tool: outside.into(), note_key })
        .map_err(|e| e.to_string())?;
    changed(st);
    // The Inbox: the chat waits for you now.
    crate::notifications::poke(st);
    Ok(a)
}

/// The Team Lead made `tool` with `args` itself in the chat `thread_id` (it ran): the same call that still waits there no
/// longer does, so Approve can't make it twice, and the chat says so. Nothing happens when none waits.
pub(crate) fn made(st: &AppState, thread_id: &str, tool: &str, args: &Map<String, Value>) {
    const WHY: &str = "The Team Lead made the same call itself in a later answer.";
    let gone = match core::made(&st.db, thread_id, tool, &Value::Object(args.clone()), WHY) {
        Ok(g) => g,
        Err(e) => return eprintln!("gizai: clearing a call that waited for approval failed: {e}"),
    };
    for a in &gone {
        let what = lower_first(&preview::preview(st, &a.actor_id, &a.thread_id, &a.tool, &a.args).line);
        crate::chat::note(st, &a.thread_id, &format!("No longer waiting for your OK: {what}. {WHY}"),
                          json!({"kind": "pending_gone", "action": a.id, "tool": a.tool, "run": a.run_id}));
    }
    if !gone.is_empty() {
        changed(st);
    }
}

fn view(st: &AppState, a: PendingAction) -> PendingView {
    let preview = preview::preview(st, &a.actor_id, &a.thread_id, &a.tool, &a.args);
    PendingView { action: a, preview }
}

/// The chat's calls that wait for you, in the order they were tried, each with what it would do now.
pub fn list(st: &AppState, thread_id: &str) -> Result<Vec<PendingView>, String> {
    Ok(core::waiting(&st.db, thread_id).map_err(|e| e.to_string())?.into_iter().map(|a| view(st, a)).collect())
}

/// "Save the note X" → "save the note X", after "Approved: " and "Discarded: ".
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The note in the chat for a decision, with what it is about for the Chat page and the hand-over.
fn note(st: &AppState, a: &PendingAction, text: &str) {
    crate::chat::note(st, &a.thread_id, text, json!({"kind": gizai_core::chat::DECISION_NOTE, "action": a.id, "tool": a.tool,
                                                    "status": a.status, "run": a.run_id}));
}

/// The last note after your decisions in a chat, so it doesn't look stuck: approving doesn't start a new answer.
pub const ANSWER_DONE: &str = "Nothing else waits for your OK. The Team Lead's answer above is done: send a message to go on.";

/// Held from a decision's end until its notes are in the chat: of two decisions in a chat that end together, only the
/// later one finds that nothing waits any more, so the chat gets one `ANSWER_DONE`.
static ENDING: Mutex<()> = Mutex::new(());

/// You decided on `a`: its note in the chat, then `ANSWER_DONE` when no call of the chat waits any more (of an earlier
/// answer neither) and the Team Lead isn't answering there. That one is for you only: it isn't a decision note, so the
/// Team Lead's next answer (`for_turn`) and a hand-over leave it out.
fn decided(st: &AppState, a: &PendingAction, text: &str) {
    note(st, a, text);
    if crate::chat::answering(st, &a.thread_id) || !core::waiting(&st.db, &a.thread_id).is_ok_and(|w| w.is_empty()) {
        return;
    }
    crate::chat::note(st, &a.thread_id, ANSWER_DONE, json!({"kind": "pending_done", "run": a.run_id}));
}

/// Approve: runs exactly the stored call, once (a second approval is refused, `core::claim`), as the Team Lead in its chat
/// with every rule but the outside one in force. "Approved: <what>. <what happened>" or "… It failed: <its error>" goes
/// into the chat as a note of Gizai's, and to the Team Lead in its next answer.
pub async fn approve(st: &AppState, id: &str) -> Result<PendingAction, String> {
    let a = core::claim(&st.db, &st.you_id, id).map_err(|e| e.to_string())?;
    changed(st);
    let what = lower_first(&preview::preview(st, &a.actor_id, &a.thread_id, &a.tool, &a.args).line);
    let ran = crate::tools::call_approved(st, &a.actor_id, &a.thread_id, &a.tool, a.args.clone()).await;
    let (ok, text) = match &ran {
        Ok(result) => (true, format!("Approved: {what}. {}", preview::done(&a.tool, result))),
        Err(e) => (false, format!("Approved: {what}. It failed: {}", e.trim_end_matches('.'))),
    };
    let a = {
        let _ending = ENDING.lock().unwrap_or_else(|e| e.into_inner());
        let a = core::finish(&st.db, &a.id, ok, &text).map_err(|e| e.to_string())?;
        decided(st, &a, &text);
        a
    };
    changed(st);
    Ok(a)
}

/// Approve all: the calls you saw (`ids`), one after the other in the order given, each as Approve runs it; one that
/// fails doesn't stop the rest. Calls that no longer wait are skipped, and the error names them. The ones it hasn't run
/// yet still wait, so `ANSWER_DONE` comes once, after the last one it ran.
pub async fn approve_all(st: &AppState, ids: &[String]) -> Result<Vec<PendingAction>, String> {
    let mut done = vec![];
    let mut skipped = vec![];
    for id in ids {
        match approve(st, id).await {
            Ok(a) => done.push(a),
            Err(e) => skipped.push(e),
        }
    }
    if !skipped.is_empty() && done.is_empty() {
        return Err(skipped.join(" "));
    }
    Ok(done)
}

/// Discard: nothing runs. "Discarded: <what>." goes into the chat, and to the Team Lead in its next answer.
pub fn discard(st: &AppState, id: &str) -> Result<PendingAction, String> {
    let a = core::get(&st.db, id).map_err(|e| e.to_string())?;
    let text = format!("Discarded: {}.", lower_first(&preview::preview(st, &a.actor_id, &a.thread_id, &a.tool, &a.args).line));
    let a = {
        let _ending = ENDING.lock().unwrap_or_else(|e| e.into_inner());
        let a = core::discard(&st.db, &st.you_id, id, &text).map_err(|e| e.to_string())?;
        decided(st, &a, &text);
        a
    };
    changed(st);
    Ok(a)
}

/// What the Team Lead hears at the start of its next answer in a chat: what you decided since on its calls that waited.
#[derive(Debug, Clone, Default)]
pub struct ForTurn {
    /// The line the answer's prompt starts with (empty: none), in square brackets like Gizai's other lines there.
    pub line: String,
    /// The decisions in it: once the answer has started with them (`told`), the next one doesn't hear them again.
    pub ids: Vec<String>,
}

/// The decisions in the chat the Team Lead hasn't heard yet, for its next answer.
pub fn for_turn(st: &AppState, thread_id: &str) -> ForTurn {
    let decided = core::untold(&st.db, thread_id).unwrap_or_default();
    if decided.is_empty() {
        return ForTurn::default();
    }
    let you = gizai_core::users::list(&st.db).ok().and_then(|l| l.into_iter().find(|p| p.id == st.you_id)).map(|p| p.name)
        .unwrap_or_else(|| "The user".into());
    let lines: Vec<String> = decided.iter().map(|a| {
        let said = a.result_md.clone().unwrap_or_else(|| format!("{} {}", a.status, a.tool));
        format!("- {} (your {} call)", said.trim_end_matches('.'), a.tool)
    }).collect();
    ForTurn {
        line: format!("[Gizai: {you} decided on calls of yours that waited for their approval after a tool from outside Gizai:\n{}]", lines.join("\n")),
        ids: decided.into_iter().map(|a| a.id).collect(),
    }
}

/// The answer started with these decisions: the Team Lead has heard them.
pub fn told(st: &AppState, ids: &[String]) {
    if let Err(e) = core::mark_told(&st.db, ids) {
        eprintln!("gizai: noting that the Team Lead heard the decisions failed: {e}");
    }
}

/// What a chat says about an approved call Gizai closed under while it ran.
pub const INTERRUPTED: &str = "Gizai closed while it ran, so it may or may not have happened: check before you ask for it again.";

/// At start-up: a call approved and still running when Gizai closed counts as failed, so it never runs twice, and its
/// chat (and the Team Lead's next answer there) says it may or may not have happened. Calls that wait stay waiting.
pub fn at_start(st: &AppState) {
    let running = match core::running(&st.db) {
        Ok(r) => r,
        Err(e) => return eprintln!("gizai: looking for approved calls Gizai closed under failed: {e}"),
    };
    for a in running {
        let what = lower_first(&preview::preview(st, &a.actor_id, &a.thread_id, &a.tool, &a.args).line);
        let text = format!("Approved: {what}. {INTERRUPTED}");
        match core::finish(&st.db, &a.id, false, &text) {
            Ok(a) => note(st, &a, &text),
            Err(e) => eprintln!("gizai: closing the approved call {} failed: {e}", a.id),
        }
    }
}
