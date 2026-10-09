//! Chat with the Team Lead: threads (one Claude Code session each, on the coding CLI the chat runs on), their messages,
//! and the messages that wait while the Team Lead answers (the queue). A turn's process, stream and tools live in the app;
//! this is only the record, and the hand-over a new session gets.
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::Value;

use crate::db::{Db, Writer};
use crate::{Error, Result, ids, util};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatThread {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub session_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// The Claude session's cumulative totals after its last turn.
    pub cost_usd_micros: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    /// Who started it: you, or the Team Lead during a board check.
    pub created_by: Option<String>,
    /// A chat the Team Lead started: question | approval. None: your own chat.
    pub kind: Option<String>,
    /// The cards a Team Lead chat is about (ids), and their identifiers.
    pub task_ids: Vec<String>,
    pub tasks: Vec<String>,
    /// You sent a message in it.
    pub answered_at: Option<i64>,
    /// You dismissed it in the Inbox.
    pub dismissed_at: Option<i64>,
    /// A Team Lead chat that still waits for you: it is in the Inbox.
    pub waiting: bool,
    /// The chat's own Runs on (a coding CLI's id); None: it follows the Team Lead's (agent form → Runs on).
    pub cli: Option<String>,
    /// The coding CLI whose account holds `session_id`: another one can't resume it.
    pub session_cli: Option<String>,
}

impl ChatThread {
    pub fn totals(&self) -> Totals {
        Totals { cost_usd_micros: self.cost_usd_micros, input_tokens: self.input_tokens, output_tokens: self.output_tokens }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub thread_id: String,
    /// user | agent | tool | system
    pub role: String,
    pub author_id: Option<String>,
    pub author_name: Option<String>,
    pub body_md: Option<String>,
    pub run_id: Option<String>,
    pub tool_name: Option<String>,
    /// For tool messages: `{id, input, result?, isError?}`.
    pub tool: Option<Value>,
    pub created_at: i64,
    /// For some system notes, what the Chat page offers with them: `{kind: "switch", cli, cliName}` where the chat moved to
    /// another coding CLI, `{kind: "limit", cli, cliName, limit, resets?, messageIds}` where an answer hit a usage limit.
    pub meta: Option<Value>,
}

#[derive(Debug, Clone, Default)]
pub struct NewMessage {
    pub thread_id: String,
    pub role: String,
    pub author_id: Option<String>,
    pub body_md: Option<String>,
    pub run_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool: Option<Value>,
    pub meta: Option<Value>,
}

/// Cost (µ$) and tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub cost_usd_micros: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// What one turn cost, from the session's cumulative totals before and after it, each on its own: a total that is
/// less than before was started anew (or isn't reported cumulatively), and then the turn cost what it reports. A failed
/// resumed turn often reports its cumulative cost without its tokens: its cost is still only the difference, not the
/// whole session's again.
pub fn turn_cost(prev: Totals, now: Totals) -> Totals {
    let one = |p: i64, n: i64| if n < p { n } else { n - p };
    Totals {
        cost_usd_micros: one(prev.cost_usd_micros, now.cost_usd_micros),
        input_tokens: one(prev.input_tokens, now.input_tokens),
        output_tokens: one(prev.output_tokens, now.output_tokens),
    }
}

impl Totals {
    /// Each total at least as high as in `other`: a resumed session's totals only grow, so a total it didn't report
    /// (a failed turn without its tokens) keeps the earlier one.
    pub fn at_least(self, other: Totals) -> Totals {
        Totals { cost_usd_micros: self.cost_usd_micros.max(other.cost_usd_micros), input_tokens: self.input_tokens.max(other.input_tokens),
                 output_tokens: self.output_tokens.max(other.output_tokens) }
    }
}

const TITLE_MAX: usize = 60;

/// A thread's title from its first message: one line, at most 60 characters, cut on a word.
pub fn title_from(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return "New chat".into();
    }
    if flat.chars().count() <= TITLE_MAX {
        return flat;
    }
    let cut: String = flat.chars().take(TITLE_MAX).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > TITLE_MAX / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([',', '.', ':', ';', ' ']))
}

const THREAD_COLS: &str = "id, agent_actor_id, COALESCE(title, 'New chat'), session_id, created_at, updated_at, cost_usd_micros, input_tokens, output_tokens,
    created_by, kind, task_ids_json, answered_at, dismissed_at, cli, session_cli";

fn thread_row(r: &rusqlite::Row) -> rusqlite::Result<ChatThread> {
    let task_ids: Vec<String> = serde_json::from_str(&r.get::<_, String>(11)?).unwrap_or_default();
    let (kind, answered_at, dismissed_at): (Option<String>, Option<i64>, Option<i64>) = (r.get(10)?, r.get(12)?, r.get(13)?);
    Ok(ChatThread { id: r.get(0)?, agent_id: r.get(1)?, title: r.get(2)?, session_id: r.get(3)?, created_at: r.get(4)?, updated_at: r.get(5)?,
                    cost_usd_micros: r.get(6)?, input_tokens: r.get(7)?, output_tokens: r.get(8)?, created_by: r.get(9)?,
                    waiting: kind.is_some() && answered_at.is_none() && dismissed_at.is_none(), kind, task_ids, tasks: vec![], answered_at, dismissed_at,
                    cli: r.get(14)?, session_cli: r.get(15)? })
}

/// Fills in the identifiers of the cards Team Lead chats are about.
fn with_tasks(c: &rusqlite::Connection, mut threads: Vec<ChatThread>) -> Result<Vec<ChatThread>> {
    for t in threads.iter_mut().filter(|t| !t.task_ids.is_empty()) {
        for id in &t.task_ids {
            if let Some(ident) = c.query_row("SELECT identifier FROM tasks WHERE id=?1 AND deleted_at IS NULL", [id], |r| r.get::<_, String>(0)).optional()? {
                t.tasks.push(ident);
            }
        }
    }
    Ok(threads)
}

pub fn create_thread(db: &Db, you: &str, agent_id: &str, first_text: &str) -> Result<String> {
    create_thread_on(db, you, agent_id, first_text, None)
}

/// A new chat of yours that runs on `cli` (picked under the text box before its first message); None: on the Team Lead's.
pub fn create_thread_on(db: &Db, you: &str, agent_id: &str, first_text: &str, cli: Option<&str>) -> Result<String> {
    let title = title_from(first_text);
    let cli = chat_cli(db, cli)?;
    db.write(Some(you), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute(
            "INSERT INTO chat_threads(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, title, cli) VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, now, you, util::org_id(c)?, agent_id, title, cli],
        )?;
        w.insert("chat_threads", &id, serde_json::json!({"agent": agent_id, "title": title, "cli": cli}))?;
        Ok(id)
    })
}

/// A chat's own pick of coding CLI, checked: it is in Settings and can run the Team Lead's chat. None or empty: none.
fn chat_cli(db: &Db, cli: Option<&str>) -> Result<Option<String>> {
    let Some(id) = cli.map(str::trim).filter(|c| !c.is_empty()) else { return Ok(None) };
    let c = crate::clis::get(db, id)?;
    if let Some(p) = crate::clis::chat_problem(&c) {
        return Err(Error::Invalid(format!("{} can't run the chat: {p}", c.name)));
    }
    Ok(Some(c.id))
}

/// Runs on under the text box: the chat's next answers run on `cli`; None: on the Team Lead's Runs on again.
pub fn set_cli(db: &Db, you: &str, thread_id: &str, cli: Option<&str>) -> Result<ChatThread> {
    let cli = chat_cli(db, cli)?;
    db.write(Some(you), |w| {
        let n = w.conn().execute("UPDATE chat_threads SET cli=?2 WHERE id=?1 AND deleted_at IS NULL", rusqlite::params![thread_id, cli])?;
        if n == 0 {
            return Err(Error::NotFound(format!("chat {thread_id}")));
        }
        w.update("chat_threads", thread_id, serde_json::json!({"cli": cli}))
    })?;
    get_thread(db, thread_id)
}

/// The coding CLI a chat's next answer runs on: its own pick, else the Team Lead's Runs on (`lead_adapter`), also when
/// its pick has been removed from Settings.
pub fn runs_on(db: &Db, thread: &ChatThread, lead_adapter: Option<&str>) -> Result<crate::clis::Cli> {
    if let Some(c) = thread.cli.as_deref().and_then(|id| crate::clis::get(db, id).ok()) {
        return Ok(c);
    }
    crate::clis::get(db, lead_adapter.unwrap_or_default())
}

/// Newest activity first.
pub fn list_threads(db: &Db) -> Result<Vec<ChatThread>> {
    threads(db, None)
}

/// Chat → Recent: the `limit` chats with the newest activity, newest first.
pub fn recent_threads(db: &Db, limit: usize) -> Result<Vec<ChatThread>> {
    threads(db, Some(limit))
}

fn threads(db: &Db, limit: Option<usize>) -> Result<Vec<ChatThread>> {
    // SQLite reads a negative LIMIT as no limit.
    let limit = limit.map_or(-1, |n| i64::try_from(n).unwrap_or(i64::MAX));
    db.read(|c| {
        let mut st = c.prepare(&format!("SELECT {THREAD_COLS} FROM chat_threads WHERE deleted_at IS NULL ORDER BY updated_at DESC, rowid DESC LIMIT ?1"))?;
        with_tasks(c, st.query_map([limit], thread_row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// A chat the Archive finds, with the newest of its messages (yours or the Team Lead's) whose text matches. No message:
/// only its title matches, or there was nothing to search for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadHit {
    pub thread: ChatThread,
    pub message: Option<ChatMessage>,
}

/// `text` anywhere, for `LIKE … ESCAPE '\'`: its `%` and `_` match themselves.
fn like_pattern(text: &str) -> String {
    let mut p = String::from("%");
    for ch in text.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            p.push('\\');
        }
        p.push(ch);
    }
    p.push('%');
    p
}

/// Chat → Archive: the chats whose title, or the text of one of your messages or the Team Lead's, holds `query` (case
/// ignored for A to Z), newest activity first, each once. Tool calls, Gizai's notes and deleted chats and messages are
/// left out. An empty query lists every chat.
pub fn search_threads(db: &Db, query: &str) -> Result<Vec<ThreadHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(list_threads(db)?.into_iter().map(|thread| ThreadHit { thread, message: None }).collect());
    }
    let pattern = like_pattern(query);
    db.read(|c| {
        // One pass over the messages: per chat, the newest one that matches.
        let mut st = c.prepare(&format!(
            "SELECT {THREAD_COLS}, hit.message_rowid FROM chat_threads
             LEFT JOIN (SELECT thread_id, MAX(rowid) AS message_rowid FROM chat_messages
                        WHERE deleted_at IS NULL AND role IN ('user', 'agent') AND body_md LIKE ?1 ESCAPE '\\' GROUP BY thread_id) hit
               ON hit.thread_id = chat_threads.id
             WHERE chat_threads.deleted_at IS NULL AND (COALESCE(title, 'New chat') LIKE ?1 ESCAPE '\\' OR hit.message_rowid IS NOT NULL)
             ORDER BY chat_threads.updated_at DESC, chat_threads.rowid DESC"))?;
        let found = st.query_map([&pattern], |r| Ok((thread_row(r)?, r.get::<_, Option<i64>>(16)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let (threads, rowids): (Vec<_>, Vec<_>) = found.into_iter().unzip();
        let mut msg = c.prepare(&format!("{MSG_SELECT} WHERE m.rowid=?1"))?;
        with_tasks(c, threads)?.into_iter().zip(rowids).map(|(thread, rowid)| -> Result<ThreadHit> {
            let message = rowid.map(|r| msg.query_row([r], msg_row)).transpose()?;
            Ok(ThreadHit { thread, message })
        }).collect()
    })
}

pub fn get_thread(db: &Db, id: &str) -> Result<ChatThread> {
    db.read(|c| {
        let t = c.query_row(&format!("SELECT {THREAD_COLS} FROM chat_threads WHERE id=?1 AND deleted_at IS NULL"), [id], thread_row)
            .optional()?.ok_or_else(|| Error::NotFound(format!("chat {id}")))?;
        Ok(with_tasks(c, vec![t])?.remove(0))
    })
}

/// The chats the Team Lead started that still wait for you (the Inbox), newest activity first.
pub fn waiting_lead_chats(db: &Db) -> Result<Vec<ChatThread>> {
    Ok(list_threads(db)?.into_iter().filter(|t| t.waiting).collect())
}

pub const LEAD_KINDS: [&str; 2] = ["question", "approval"];

/// The Team Lead asks you something in a chat (`start_chat`): `body_md` is its first message, saved as the agent's.
/// One waiting Team Lead chat per card: when one of `task_ids` already has one, the message goes there instead (it
/// moves to the top again, and its cards are joined). Returns the chat's id and whether it was a new chat.
pub fn start_lead_chat(db: &Db, agent_id: &str, title: &str, kind: &str, task_ids: &[String], body_md: &str, run_id: Option<&str>)
    -> Result<(String, bool)> {
    if !LEAD_KINDS.contains(&kind) {
        return Err(Error::Invalid(format!("a Team Lead chat is a question or an approval, not {kind}")));
    }
    if body_md.trim().is_empty() {
        return Err(Error::Invalid("write the first message (body_md)".into()));
    }
    let title = title_from(title);
    let existing = waiting_lead_chats(db)?.into_iter().find(|t| t.agent_id == agent_id && t.task_ids.iter().any(|id| task_ids.contains(id)));
    let (id, new) = match existing {
        Some(t) => {
            let mut ids = t.task_ids.clone();
            ids.extend(task_ids.iter().filter(|id| !t.task_ids.contains(id)).cloned());
            db.write(Some(agent_id), |w| {
                w.conn().execute("UPDATE chat_threads SET task_ids_json=?2 WHERE id=?1", rusqlite::params![t.id, serde_json::to_string(&ids)?])?;
                Ok(())
            })?;
            (t.id, false)
        }
        None => {
            let id = db.write(Some(agent_id), |w| {
                let c = w.conn();
                let now = ids::now_ms();
                let id = ids::new_id();
                c.execute(
                    "INSERT INTO chat_threads(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, title, kind, task_ids_json)
                     VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?3, ?5, ?6, ?7)",
                    rusqlite::params![id, now, agent_id, util::org_id(c)?, title, kind, serde_json::to_string(task_ids)?],
                )?;
                w.insert("chat_threads", &id, serde_json::json!({"agent": agent_id, "title": title, "kind": kind, "tasks": task_ids}))?;
                Ok(id)
            })?;
            (id, true)
        }
    };
    add_message(db, NewMessage { thread_id: id.clone(), role: "agent".into(), author_id: Some(agent_id.to_string()), body_md: Some(body_md.trim().to_string()),
                                 run_id: run_id.map(str::to_string), ..Default::default() })?;
    Ok((id, new))
}

/// You dismissed a Team Lead chat in the Inbox: it stays in Chat → Recent, no longer waiting.
pub fn dismiss(db: &Db, you: &str, thread_id: &str) -> Result<()> {
    db.write(Some(you), |w| {
        let n = w.conn().execute("UPDATE chat_threads SET dismissed_at=COALESCE(dismissed_at, ?2) WHERE id=?1 AND deleted_at IS NULL",
                                 rusqlite::params![thread_id, ids::now_ms()])?;
        if n == 0 {
            return Err(Error::NotFound(format!("chat {thread_id}")));
        }
        w.update("chat_threads", thread_id, serde_json::json!({"dismissed": true}))
    })
}

const MSG_SELECT: &str = "SELECT m.id, m.thread_id, m.role, m.author_actor_id, a.name, m.body_md, m.run_id, m.tool_name, m.tool_json, m.created_at, m.meta_json
    FROM chat_messages m LEFT JOIN actors a ON a.id = m.author_actor_id";

fn msg_row(r: &rusqlite::Row) -> rusqlite::Result<ChatMessage> {
    let (tool, meta): (Option<String>, Option<String>) = (r.get(8)?, r.get(10)?);
    Ok(ChatMessage { id: r.get(0)?, thread_id: r.get(1)?, role: r.get(2)?, author_id: r.get(3)?, author_name: r.get(4)?, body_md: r.get(5)?,
                     run_id: r.get(6)?, tool_name: r.get(7)?, tool: tool.and_then(|t| serde_json::from_str(&t).ok()), created_at: r.get(9)?,
                     meta: meta.and_then(|t| serde_json::from_str(&t).ok()) })
}

/// In the order they were written.
pub fn messages(db: &Db, thread_id: &str) -> Result<Vec<ChatMessage>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("{MSG_SELECT} WHERE m.thread_id=?1 AND m.deleted_at IS NULL ORDER BY m.rowid"))?;
        Ok(st.query_map([thread_id], msg_row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

fn get_message(c: &rusqlite::Connection, id: &str) -> Result<ChatMessage> {
    c.query_row(&format!("{MSG_SELECT} WHERE m.id=?1"), [id], msg_row).optional()?.ok_or_else(|| Error::NotFound(format!("message {id}")))
}

/// One message, deleted ones included.
pub fn message(db: &Db, id: &str) -> Result<ChatMessage> {
    db.read(|c| get_message(c, id))
}

/// Saves a message and moves its thread to the top of the list.
pub fn add_message(db: &Db, m: NewMessage) -> Result<ChatMessage> {
    db.write(m.author_id.as_deref(), |w| {
        let id = insert_message(w, &m)?;
        get_message(w.conn(), &id)
    })
}

/// `add_message` inside a write. Returns the new message's id.
pub(crate) fn insert_message(w: &Writer, m: &NewMessage) -> Result<String> {
    if !["user", "agent", "tool", "system"].contains(&m.role.as_str()) {
        return Err(Error::Invalid(format!("a chat message is from the user, the agent, a tool or the system, not {}", m.role)));
    }
    let c = w.conn();
    let now = ids::now_ms();
    let id = ids::new_id();
    let n = c.execute("UPDATE chat_threads SET updated_at=?2 WHERE id=?1 AND deleted_at IS NULL", rusqlite::params![m.thread_id, now])?;
    if n == 0 {
        return Err(Error::NotFound(format!("chat {}", m.thread_id)));
    }
    if m.role == "user" {
        // You answered a Team Lead chat: it leaves the Inbox.
        c.execute("UPDATE chat_threads SET answered_at=?2 WHERE id=?1 AND kind IS NOT NULL AND answered_at IS NULL", rusqlite::params![m.thread_id, now])?;
    }
    c.execute(
        "INSERT INTO chat_messages(id, created_at, thread_id, role, author_actor_id, body_md, run_id, tool_name, tool_json, meta_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![id, now, m.thread_id, m.role, m.author_id, m.body_md, m.run_id, m.tool_name, m.tool.as_ref().map(Value::to_string),
                          m.meta.as_ref().map(Value::to_string)],
    )?;
    w.insert("chat_messages", &id, serde_json::json!({"thread": m.thread_id, "role": m.role}))?;
    Ok(id)
}

/// Adds a tool call's result to its message.
pub fn set_tool_result(db: &Db, message_id: &str, text: &str, is_error: bool) -> Result<ChatMessage> {
    db.write(None, |w| {
        let c = w.conn();
        let mut msg = get_message(c, message_id)?;
        let mut tool = msg.tool.take().unwrap_or_else(|| serde_json::json!({}));
        tool["result"] = Value::String(text.to_string());
        tool["isError"] = Value::Bool(is_error);
        c.execute("UPDATE chat_messages SET tool_json=?2 WHERE id=?1", rusqlite::params![message_id, tool.to_string()])?;
        w.update("chat_messages", message_id, serde_json::json!({"tool_result": true, "is_error": is_error}))?;
        get_message(c, message_id)
    })
}

/// After a turn: the session to resume next time and its cumulative totals.
pub fn record_session(db: &Db, thread_id: &str, session_id: &str, totals: Totals) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute(
            "UPDATE chat_threads SET session_id=?2, cost_usd_micros=?3, input_tokens=?4, output_tokens=?5 WHERE id=?1",
            rusqlite::params![thread_id, session_id, totals.cost_usd_micros, totals.input_tokens, totals.output_tokens],
        )?;
        Ok(())
    })
}

/// `record_session` for a session on `cli` (a coding CLI's id): only that CLI's account can resume it.
pub fn record_session_on(db: &Db, thread_id: &str, session_id: &str, cli: &str, totals: Totals) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute(
            "UPDATE chat_threads SET session_id=?2, session_cli=?3, cost_usd_micros=?4, input_tokens=?5, output_tokens=?6 WHERE id=?1",
            rusqlite::params![thread_id, session_id, cli, totals.cost_usd_micros, totals.input_tokens, totals.output_tokens],
        )?;
        Ok(())
    })
}

/// Forgets the thread's session (it can't be resumed): the next turn starts a new one.
pub fn reset_session(db: &Db, thread_id: &str) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE chat_threads SET session_id=NULL, session_cli=NULL, cost_usd_micros=0, input_tokens=0, output_tokens=0 WHERE id=?1", [thread_id])?;
        Ok(())
    })
}

/// What a chat says when Gizai stopped while an answer was being written (it crashed, or the computer went off).
pub const INTERRUPTED: &str = "Interrupted: Gizai closed before the Team Lead finished this answer.";

/// About how long (characters) the conversation a new session is handed may be.
pub const HANDOVER_CAP: usize = 40_000;
/// How much of a tool call's result the hand-over keeps.
const HANDOVER_RESULT: usize = 300;
/// How much of a tool call's input the hand-over keeps.
const HANDOVER_INPUT: usize = 200;

fn shorten(s: &str, n: usize) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &flat[..i]),
        None => flat,
    }
}

/// What a tool call was called on, from its input: `task: GA-12, comment: …` (its plain values, shortened).
fn tool_target(input: &Value) -> String {
    match input {
        Value::Object(map) => {
            let parts: Vec<String> = map.iter().filter_map(|(k, v)| match v {
                Value::String(s) if !s.trim().is_empty() => Some(format!("{k}: {s}")),
                Value::Number(n) => Some(format!("{k}: {n}")),
                Value::Bool(b) => Some(format!("{k}: {b}")),
                Value::Array(a) if !a.is_empty() => Some(format!("{k}: {}", Value::Array(a.clone()))),
                _ => None,
            }).collect();
            shorten(&parts.join(", "), HANDOVER_INPUT)
        }
        Value::Null => String::new(),
        other => shorten(&other.to_string(), HANDOVER_INPUT),
    }
}

/// One message in the hand-over; None for Gizai's own notes.
fn handover_entry(m: &ChatMessage) -> Option<String> {
    let body = || m.body_md.clone().unwrap_or_default().trim().to_string();
    match m.role.as_str() {
        "user" => Some(format!("User: {}", body())),
        "agent" => Some(format!("You: {}", body())),
        "tool" => {
            let name = m.tool_name.clone().unwrap_or_default();
            let name = name.trim_start_matches("mcp__gizai__");
            let tool = m.tool.clone().unwrap_or(Value::Null);
            let target = tool_target(tool.get("input").unwrap_or(&Value::Null));
            let on = if target.is_empty() { String::new() } else { format!(" ({target})") };
            let result = match tool.get("result") {
                Some(Value::String(r)) if tool.get("isError").and_then(Value::as_bool) == Some(true) => format!("it failed: {}", shorten(r, HANDOVER_RESULT)),
                Some(Value::String(r)) => format!("its result began: {}", shorten(r, HANDOVER_RESULT)),
                Some(other) => format!("its result began: {}", shorten(&other.to_string(), HANDOVER_RESULT)),
                None => "it had no result".into(),
            };
            Some(format!("(You called {name}{on}; {result})"))
        }
        _ => None,
    }
}

/// The conversation so far, for a new session that can't resume the chat's old one (the chat moved to another coding
/// CLI, whose account doesn't have it, or the session is gone): your messages and the Team Lead's, oldest first, and per
/// tool call its name, what it was called on (the card, doc or file) and the start of its result, so the new session
/// knows what was looked up and can look it up again. Gizai's own notes are left out. When it is longer than `cap`
/// characters, the oldest messages go first and the first line says how many went.
pub fn handover(messages: &[ChatMessage], cap: usize) -> String {
    let entries: Vec<String> = messages.iter().filter_map(handover_entry).map(|e| shorten_keep_lines(&e, cap)).collect();
    let mut size = 0;
    let mut keep = 0;
    for e in entries.iter().rev() {
        let n = e.chars().count() + 2;
        if size + n > cap {
            break;
        }
        size += n;
        keep += 1;
    }
    let dropped = entries.len() - keep;
    let mut out = String::new();
    if dropped > 0 {
        out.push_str(&format!("(The {dropped} oldest {} left out, to keep this short.)\n\n", if dropped == 1 { "message is" } else { "messages are" }));
    }
    out.push_str(&entries[dropped..].join("\n\n"));
    out
}

/// At most `n` characters, keeping the text's lines (a message's own layout).
fn shorten_keep_lines(s: &str, n: usize) -> String {
    match s.char_indices().nth(n.saturating_sub(1)) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// The text a turn sends for messages that go together (the queue): one as it is; several oldest first, with a line
/// that says they were written while the Team Lead answered.
pub fn joined(texts: &[String]) -> String {
    match texts {
        [one] => one.clone(),
        many => format!("({} messages, written while you answered, oldest first:)\n\n{}", many.len(), many.join("\n\n")),
    }
}

/// A message you sent while the Team Lead was answering: it waits in the chat's queue until the answer is done.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedMessage {
    pub id: String,
    pub thread_id: String,
    pub body_md: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// It waits for Send now (the answer before it was stopped or failed, or Gizai restarted); otherwise it goes by
    /// itself when the answer that is being written is done.
    pub held: bool,
}

const QUEUE_SELECT: &str = "SELECT id, thread_id, body_md, created_at, updated_at, held_at FROM chat_queue";

fn queued_row(r: &rusqlite::Row) -> rusqlite::Result<QueuedMessage> {
    Ok(QueuedMessage { id: r.get(0)?, thread_id: r.get(1)?, body_md: r.get(2)?, created_at: r.get(3)?, updated_at: r.get(4)?,
                       held: r.get::<_, Option<i64>>(5)?.is_some() })
}

fn queue_in(c: &rusqlite::Connection, thread_id: &str) -> Result<Vec<QueuedMessage>> {
    let mut st = c.prepare(&format!("{QUEUE_SELECT} WHERE thread_id=?1 ORDER BY rowid"))?;
    Ok(st.query_map([thread_id], queued_row)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The chat's queued messages, in the order they were written.
pub fn queue(db: &Db, thread_id: &str) -> Result<Vec<QueuedMessage>> {
    db.read(|c| queue_in(c, thread_id))
}

fn queue_text(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Error::Invalid("type a message first".into()));
    }
    Ok(text.to_string())
}

/// Queues a message while the Team Lead answers: it goes when the answer is done.
pub fn enqueue(db: &Db, you: &str, thread_id: &str, text: &str) -> Result<QueuedMessage> {
    let text = queue_text(text)?;
    db.write(Some(you), |w| {
        let c = w.conn();
        get_thread_in(c, thread_id)?;
        let (id, now) = (ids::new_id(), ids::now_ms());
        c.execute("INSERT INTO chat_queue(id, created_at, updated_at, thread_id, author_actor_id, body_md) VALUES (?1, ?2, ?2, ?3, ?4, ?5)",
                  rusqlite::params![id, now, thread_id, you, text])?;
        w.insert("chat_queue", &id, serde_json::json!({"thread": thread_id}))?;
        Ok(c.query_row(&format!("{QUEUE_SELECT} WHERE id=?1"), [&id], queued_row)?)
    })
}

fn get_thread_in(c: &rusqlite::Connection, id: &str) -> Result<()> {
    c.query_row("SELECT 1 FROM chat_threads WHERE id=?1 AND deleted_at IS NULL", [id], |_| Ok(())).optional()?
        .ok_or_else(|| Error::NotFound(format!("chat {id}")))
}

/// Changes a queued message's text, until it goes.
pub fn edit_queued(db: &Db, you: &str, id: &str, text: &str) -> Result<QueuedMessage> {
    let text = queue_text(text)?;
    db.write(Some(you), |w| {
        let c = w.conn();
        let n = c.execute("UPDATE chat_queue SET body_md=?2, updated_at=?3 WHERE id=?1", rusqlite::params![id, text, ids::now_ms()])?;
        if n == 0 {
            return Err(Error::Invalid("that message has gone already, so it can't be changed".into()));
        }
        w.update("chat_queue", id, serde_json::json!({"edited": true}))?;
        Ok(c.query_row(&format!("{QUEUE_SELECT} WHERE id=?1"), [id], queued_row)?)
    })
}

/// Removes a queued message, until it goes.
pub fn remove_queued(db: &Db, you: &str, id: &str) -> Result<()> {
    db.write(Some(you), |w| {
        let n = w.conn().execute("DELETE FROM chat_queue WHERE id=?1", [id])?;
        if n == 0 {
            return Err(Error::Invalid("that message has gone already".into()));
        }
        w.delete("chat_queue", id)
    })
}

/// The answer was stopped or failed: the chat's queued messages wait for Send now instead of going by themselves.
pub fn hold_queue(db: &Db, thread_id: &str) -> Result<usize> {
    db.write(None, |w| Ok(w.conn().execute("UPDATE chat_queue SET held_at=?2 WHERE thread_id=?1 AND held_at IS NULL",
                                            rusqlite::params![thread_id, ids::now_ms()])?))
}

/// At start-up: queued messages wait for Send now, as after a failed answer (the answer they waited for is gone).
pub fn hold_all_queues(db: &Db) -> Result<usize> {
    db.write(None, |w| Ok(w.conn().execute("UPDATE chat_queue SET held_at=?1 WHERE held_at IS NULL", [ids::now_ms()])?))
}

/// Send now while an answer is being written: the chat's waiting messages go when that answer is done.
pub fn release_queue(db: &Db, thread_id: &str) -> Result<usize> {
    db.write(None, |w| Ok(w.conn().execute("UPDATE chat_queue SET held_at=NULL WHERE thread_id=?1 AND held_at IS NOT NULL", [thread_id])?))
}

/// Whether the chat has queued messages that go by themselves when the answer is done.
pub fn queue_ready(db: &Db, thread_id: &str) -> Result<bool> {
    db.read(|c| Ok(c.query_row("SELECT count(*) FROM chat_queue WHERE thread_id=?1 AND held_at IS NULL", [thread_id], |r| r.get::<_, i64>(0))? > 0))
}

/// The queued messages go: they leave the queue and become your messages in the chat, each its own, in the order they
/// were written, in one write. `all`: the waiting ones too (Send now), else only those that go by themselves. Returns
/// them as saved; none when nothing was queued.
pub fn send_queued(db: &Db, thread_id: &str, all: bool) -> Result<Vec<ChatMessage>> {
    db.write(None, |w| {
        let items: Vec<(QueuedMessage, Option<String>)> = {
            let c = w.conn();
            let mut st = c.prepare(&format!(
                "SELECT id, thread_id, body_md, created_at, updated_at, held_at, author_actor_id FROM chat_queue WHERE thread_id=?1 {} ORDER BY rowid",
                if all { "" } else { "AND held_at IS NULL" }))?;
            st.query_map([thread_id], |r| Ok((queued_row(r)?, r.get(6)?)))?.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut out = vec![];
        for (q, author) in items {
            w.conn().execute("DELETE FROM chat_queue WHERE id=?1", [&q.id])?;
            w.delete("chat_queue", &q.id)?;
            let id = insert_message(w, &NewMessage { thread_id: thread_id.into(), role: "user".into(), author_id: author, body_md: Some(q.body_md),
                                                     ..Default::default() })?;
            out.push(get_message(w.conn(), &id)?);
        }
        Ok(out)
    })
}
