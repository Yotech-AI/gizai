//! Run and chat events go to the window in batches (GA-191), as a terminal's output does: at most one message every
//! `EVERY` per run and per chat, with every event since the one before, in order.
//! - A run's events (`run_event`) go as `Note::RunEvents`, the window's `run-events`: `{ runId, events: [{ seq, event }] }`,
//!   in `seq` order. They are queued under the live runs' lock as they are numbered (`runs`), so a batch never has them
//!   out of order.
//! - A chat's events (`chat_event`, `chat_draft`) go as `Note::ChatEvents`, the window's `chat-events`:
//!   `{ threadId, events }`. Pieces of text written one after the other (`delta`) are joined into one. The text being
//!   written that `chat_live` shows changes when its batch goes (`chat::publish`), so a snapshot holds whole batches: the
//!   page adds the batches after it, and gets no piece twice and loses none.
//!
//! The first event after a message starts the clock; `EVERY` later everything queued goes. So an event waits at most
//! `EVERY`, and the last batch of a run or an answer goes within `EVERY` of its end.
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use crate::AppState;
use crate::chat::ChatUiEvent;
use crate::runs::{Note, SeqEvent};

/// At most one message per run, and one per chat, this often.
pub const EVERY: Duration = Duration::from_millis(50);

/// The events that wait for their run's or chat's next message.
#[derive(Default)]
pub struct Streams {
    runs: Mutex<HashMap<String, Vec<SeqEvent>>>,
    chats: Mutex<HashMap<String, Vec<ChatUiEvent>>>,
}

/// A change to the text being written in a chat (`chat_draft`).
pub enum Draft {
    /// A piece of text written.
    Delta(String),
    /// A new text block starts, or the one being written was saved: the text being written starts again.
    Block,
}

/// Queues the run's event, numbered `seq`, for the window. The caller holds the live runs' lock while it numbers and
/// queues, so the events queue in `seq` order.
pub(crate) fn run_event(st: &AppState, run_id: &str, event: SeqEvent) {
    let first = {
        let mut runs = st.streams.runs.lock().unwrap();
        let queued = runs.entry(run_id.to_string()).or_default();
        queued.push(event);
        queued.len() == 1
    };
    if first {
        let (st, run_id) = (st.clone(), run_id.to_string());
        later(move || send_run(&st, &run_id));
    }
}

/// Queues an event of a chat for the window: a tool running, or a message saved or changed. Changes to the text being
/// written go through `chat_draft`.
pub(crate) fn chat_event(st: &AppState, thread_id: &str, event: ChatUiEvent) {
    let first = {
        let mut chats = st.streams.chats.lock().unwrap();
        let queued = chats.entry(thread_id.to_string()).or_default();
        queued.push(event);
        queued.len() == 1
    };
    if first {
        schedule_chat(st, thread_id);
    }
}

/// Queues a change to the text being written in a chat, numbered (`ChatStatus::seq`) as it queues, so the numbers go up
/// in the order the window gets the changes. A piece of text right after another one joins it, with the later number.
pub(crate) fn chat_draft(st: &AppState, thread_id: &str, change: Draft) {
    let first = {
        let mut chats = st.streams.chats.lock().unwrap();
        let seq = crate::chat::next_seq(st);
        let queued = chats.entry(thread_id.to_string()).or_default();
        match (change, queued.last_mut()) {
            (Draft::Delta(more), Some(ChatUiEvent::Delta { text, seq: last })) => {
                text.push_str(&more);
                *last = seq;
            }
            (Draft::Delta(text), _) => queued.push(ChatUiEvent::Delta { text, seq }),
            (Draft::Block, _) => queued.push(ChatUiEvent::Block { seq }),
        }
        queued.len() == 1
    };
    if first {
        schedule_chat(st, thread_id);
    }
}

fn schedule_chat(st: &AppState, thread_id: &str) {
    let (st, thread_id) = (st.clone(), thread_id.to_string());
    later(move || send_chat(&st, &thread_id));
}

/// The run's queued events go to the window, in one message.
fn send_run(st: &AppState, run_id: &str) {
    let mut events = st.streams.runs.lock().unwrap().remove(run_id).unwrap_or_default();
    if events.is_empty() {
        return;
    }
    // queued in order already (`run_event`); a sort costs nothing then, and keeps the promise
    events.sort_by_key(|e| e.seq);
    (st.notify)(Note::RunEvents { run_id: run_id.to_string(), events });
}

/// The chat's queued events go to the window, in one message; their changes to the text being written are what `chat_live`
/// shows from now on (`chat::publish`).
fn send_chat(st: &AppState, thread_id: &str) {
    let events = crate::chat::publish(st, thread_id, || st.streams.chats.lock().unwrap().remove(thread_id).unwrap_or_default());
    if events.is_empty() {
        return;
    }
    (st.notify)(Note::ChatEvents { thread_id: thread_id.to_string(), events });
}

/// Runs `f` once `EVERY` has passed: on the async runtime this runs on, else on Tauri's.
fn later(f: impl FnOnce() + Send + 'static) {
    let task = async move {
        tokio::time::sleep(EVERY).await;
        f();
    };
    match tokio::runtime::Handle::try_current() {
        Ok(rt) => {
            rt.spawn(task);
        }
        Err(_) => {
            tauri::async_runtime::spawn(task);
        }
    }
}

/// For tests: how many events wait for their message now, of runs and of chats.
#[doc(hidden)]
pub fn queued(st: &AppState) -> (usize, usize) {
    let runs = st.streams.runs.lock().unwrap().values().map(Vec::len).sum();
    let chats = st.streams.chats.lock().unwrap().values().map(Vec::len).sum();
    (runs, chats)
}
