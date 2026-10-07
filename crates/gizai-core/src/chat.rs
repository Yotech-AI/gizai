//! Chat with the Team Lead: threads (one Claude Code session each) and their messages. A turn's process,
//! stream and tools live in the app; this is only the record.
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::Value;

use crate::db::Db;
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
}

/// Cost (µ$) and tokens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub cost_usd_micros: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

/// What one turn cost, from the session's cumulative totals before and after it. A session that reports
/// less than before was started anew (or doesn't report cumulatively): then the turn cost what it reports.
pub fn turn_cost(prev: Totals, now: Totals) -> Totals {
    if now.cost_usd_micros < prev.cost_usd_micros || now.input_tokens < prev.input_tokens || now.output_tokens < prev.output_tokens {
        return now;
    }
    Totals {
        cost_usd_micros: now.cost_usd_micros - prev.cost_usd_micros,
        input_tokens: now.input_tokens - prev.input_tokens,
        output_tokens: now.output_tokens - prev.output_tokens,
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

const THREAD_COLS: &str = "id, agent_actor_id, COALESCE(title, 'New chat'), session_id, created_at, updated_at, cost_usd_micros, input_tokens, output_tokens";

fn thread_row(r: &rusqlite::Row) -> rusqlite::Result<ChatThread> {
    Ok(ChatThread { id: r.get(0)?, agent_id: r.get(1)?, title: r.get(2)?, session_id: r.get(3)?, created_at: r.get(4)?, updated_at: r.get(5)?,
                    cost_usd_micros: r.get(6)?, input_tokens: r.get(7)?, output_tokens: r.get(8)? })
}

pub fn create_thread(db: &Db, you: &str, agent_id: &str, first_text: &str) -> Result<String> {
    let title = title_from(first_text);
    db.write(Some(you), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let id = ids::new_id();
        c.execute(
            "INSERT INTO chat_threads(id, created_at, updated_at, created_by, updated_by, org_id, agent_actor_id, title) VALUES (?1, ?2, ?2, ?3, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, now, you, util::org_id(c)?, agent_id, title],
        )?;
        w.insert("chat_threads", &id, serde_json::json!({"agent": agent_id, "title": title}))?;
        Ok(id)
    })
}

/// Newest activity first.
pub fn list_threads(db: &Db) -> Result<Vec<ChatThread>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("SELECT {THREAD_COLS} FROM chat_threads WHERE deleted_at IS NULL ORDER BY updated_at DESC, rowid DESC"))?;
        Ok(st.query_map([], thread_row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn get_thread(db: &Db, id: &str) -> Result<ChatThread> {
    db.read(|c| {
        c.query_row(&format!("SELECT {THREAD_COLS} FROM chat_threads WHERE id=?1 AND deleted_at IS NULL"), [id], thread_row)
            .optional()?.ok_or_else(|| Error::NotFound(format!("chat {id}")))
    })
}

const MSG_SELECT: &str = "SELECT m.id, m.thread_id, m.role, m.author_actor_id, a.name, m.body_md, m.run_id, m.tool_name, m.tool_json, m.created_at
    FROM chat_messages m LEFT JOIN actors a ON a.id = m.author_actor_id";

fn msg_row(r: &rusqlite::Row) -> rusqlite::Result<ChatMessage> {
    let tool: Option<String> = r.get(8)?;
    Ok(ChatMessage { id: r.get(0)?, thread_id: r.get(1)?, role: r.get(2)?, author_id: r.get(3)?, author_name: r.get(4)?, body_md: r.get(5)?,
                     run_id: r.get(6)?, tool_name: r.get(7)?, tool: tool.and_then(|t| serde_json::from_str(&t).ok()), created_at: r.get(9)? })
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

/// Saves a message and moves its thread to the top of the list.
pub fn add_message(db: &Db, m: NewMessage) -> Result<ChatMessage> {
    if !["user", "agent", "tool", "system"].contains(&m.role.as_str()) {
        return Err(Error::Invalid(format!("a chat message is from the user, the agent, a tool or the system, not {}", m.role)));
    }
    db.write(m.author_id.as_deref(), |w| {
        let c = w.conn();
        let now = ids::now_ms();
        let id = ids::new_id();
        let n = c.execute("UPDATE chat_threads SET updated_at=?2 WHERE id=?1 AND deleted_at IS NULL", rusqlite::params![m.thread_id, now])?;
        if n == 0 {
            return Err(Error::NotFound(format!("chat {}", m.thread_id)));
        }
        c.execute(
            "INSERT INTO chat_messages(id, created_at, thread_id, role, author_actor_id, body_md, run_id, tool_name, tool_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, now, m.thread_id, m.role, m.author_id, m.body_md, m.run_id, m.tool_name, m.tool.as_ref().map(Value::to_string)],
        )?;
        w.insert("chat_messages", &id, serde_json::json!({"thread": m.thread_id, "role": m.role}))?;
        get_message(c, &id)
    })
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

/// Forgets the thread's session (it can't be resumed): the next turn starts a new one.
pub fn reset_session(db: &Db, thread_id: &str) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE chat_threads SET session_id=NULL, cost_usd_micros=0, input_tokens=0, output_tokens=0 WHERE id=?1", [thread_id])?;
        Ok(())
    })
}
