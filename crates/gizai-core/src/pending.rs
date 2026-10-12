//! Pending actions (GA-102): calls of the Team Lead's tools that act, refused in a chat answer because that answer used a
//! tool from outside Gizai. Each waits in its chat until you approve it (the app runs exactly that call, every other rule
//! still in force) or discard it; the Team Lead hears the decision in its next answer. Only you decide: the app's Team
//! Lead tools never read this table. This is only the record; running a call is the app's.
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::Value;

use crate::db::Db;
use crate::{Error, Result, ids};

/// The tools whose newer pending call for the same note replaces the older one in a chat.
pub const NOTE_WRITES: [&str; 2] = ["memory_write", "memory_append"];

/// One kept call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingAction {
    pub id: String,
    pub thread_id: String,
    /// The chat answer that tried it; None for a call no answer made.
    pub run_id: Option<String>,
    /// The Team Lead: the call runs as it.
    pub actor_id: String,
    pub tool: String,
    /// Its arguments, exactly as called.
    pub args: Value,
    /// The tool from outside Gizai that answer used first.
    pub outside_tool: String,
    /// pending, running (approved, under way), approved (it ran), failed (approved, and it returned an error), discarded,
    /// or replaced (a newer call for the same note took its place).
    pub status: String,
    pub created_at: i64,
    pub decided_at: Option<i64>,
    /// What happened: what the approved call did, its error, or what was discarded.
    pub result_md: Option<String>,
}

/// A call to keep (`keep`).
#[derive(Debug, Clone, Default)]
pub struct NewPending {
    pub thread_id: String,
    pub run_id: Option<String>,
    pub actor_id: String,
    pub tool: String,
    pub args: Value,
    pub outside_tool: String,
    /// memory_write and memory_append: the note it is for (its id, else its path in lower case).
    pub note_key: Option<String>,
}

const COLS: &str = "id, thread_id, run_id, actor_id, tool, args_json, outside_tool, status, created_at, decided_at, result_md";

fn row(r: &rusqlite::Row) -> rusqlite::Result<PendingAction> {
    let args: String = r.get(5)?;
    Ok(PendingAction { id: r.get(0)?, thread_id: r.get(1)?, run_id: r.get(2)?, actor_id: r.get(3)?, tool: r.get(4)?,
                       args: serde_json::from_str(&args).unwrap_or(Value::Null), outside_tool: r.get(6)?, status: r.get(7)?,
                       created_at: r.get(8)?, decided_at: r.get(9)?, result_md: r.get(10)? })
}

fn get_in(c: &rusqlite::Connection, id: &str) -> Result<PendingAction> {
    c.query_row(&format!("SELECT {COLS} FROM pending_actions WHERE id=?1"), [id], row).optional()?
        .ok_or_else(|| Error::NotFound(format!("pending action {id}")))
}

pub fn get(db: &Db, id: &str) -> Result<PendingAction> {
    db.read(|c| get_in(c, id))
}

/// Keeps a refused call as a pending action of its chat. In the same write, an older pending memory_write or
/// memory_append in that chat for the same note (`note_key`) is replaced: only the newest waits. The same call made again
/// (the same tool with the same arguments) while the first still waits is that one: it isn't kept twice.
pub fn keep(db: &Db, n: NewPending) -> Result<PendingAction> {
    db.write(Some(&n.actor_id), |w| {
        let c = w.conn();
        let same: Option<PendingAction> = {
            let mut st = c.prepare(&format!("SELECT {COLS} FROM pending_actions WHERE thread_id=?1 AND status='pending' AND tool=?2 ORDER BY rowid"))?;
            let waiting = st.query_map(rusqlite::params![n.thread_id, n.tool], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
            waiting.into_iter().find(|a| a.args == n.args)
        };
        if let Some(a) = same {
            return Ok(a);
        }
        let (id, now) = (ids::new_id(), ids::now_ms());
        let note_key = n.note_key.as_ref().filter(|_| NOTE_WRITES.contains(&n.tool.as_str()));
        if let Some(key) = note_key {
            let older: Vec<String> = {
                let mut st = c.prepare("SELECT id FROM pending_actions WHERE thread_id=?1 AND status='pending' AND note_key=?2
                                        AND tool IN ('memory_write', 'memory_append')")?;
                st.query_map(rusqlite::params![n.thread_id, key], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            };
            for old in older {
                c.execute("UPDATE pending_actions SET status='replaced', decided_at=?2, updated_at=?2 WHERE id=?1",
                          rusqlite::params![old, now])?;
                w.update("pending_actions", &old, serde_json::json!({"status": "replaced", "by": id}))?;
            }
        }
        c.execute(
            "INSERT INTO pending_actions(id, created_at, updated_at, thread_id, run_id, actor_id, tool, args_json, outside_tool, note_key)
             VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![id, now, n.thread_id, n.run_id, n.actor_id, n.tool, n.args.to_string(), n.outside_tool, note_key],
        )?;
        w.insert("pending_actions", &id, serde_json::json!({"thread": n.thread_id, "tool": n.tool, "outside": n.outside_tool}))?;
        get_in(w.conn(), &id)
    })
}

/// The chat's calls that wait for you (and those approved just now, still running), in the order they were tried.
pub fn waiting(db: &Db, thread_id: &str) -> Result<Vec<PendingAction>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("SELECT {COLS} FROM pending_actions WHERE thread_id=?1 AND status IN ('pending', 'running') ORDER BY rowid"))?;
        Ok(st.query_map([thread_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// Why a call that no longer waits can't be approved or discarded now.
fn not_waiting(a: &PendingAction) -> Error {
    Error::Invalid(match a.status.as_str() {
        "running" => "It is being approved already.".into(),
        "approved" | "failed" => "It was approved already, so it doesn't run again.".into(),
        "discarded" => "It was discarded already.".into(),
        "replaced" => "A newer call for the same note took its place: look at that one.".into(),
        other => format!("It is {other}."),
    })
}

/// Approve, the first step: the call goes from pending to running, as `you`, in one write. Only one approval gets it; a
/// second (a double click, Approve all together with Approve) is refused, so the call runs once.
pub fn claim(db: &Db, you: &str, id: &str) -> Result<PendingAction> {
    db.write(Some(you), |w| {
        let c = w.conn();
        let a = get_in(c, id)?;
        if a.status != "pending" {
            return Err(not_waiting(&a));
        }
        let now = ids::now_ms();
        c.execute("UPDATE pending_actions SET status='running', decided_at=?2, decided_by=?3, updated_at=?2 WHERE id=?1 AND status='pending'",
                  rusqlite::params![id, now, you])?;
        w.update("pending_actions", id, serde_json::json!({"status": "running"}))?;
        get_in(c, id)
    })
}

/// Approve, the last step: the call ran, and `ok` says whether it did what it was asked (else `result_md` is its error).
pub fn finish(db: &Db, id: &str, ok: bool, result_md: &str) -> Result<PendingAction> {
    db.write(None, |w| {
        let c = w.conn();
        let status = if ok { "approved" } else { "failed" };
        let n = c.execute("UPDATE pending_actions SET status=?2, result_md=?3, updated_at=?4 WHERE id=?1 AND status='running'",
                          rusqlite::params![id, status, result_md, ids::now_ms()])?;
        if n == 0 {
            return Err(not_waiting(&get_in(c, id)?));
        }
        w.update("pending_actions", id, serde_json::json!({"status": status}))?;
        get_in(c, id)
    })
}

/// Discard: nothing runs. `what_md` says what was discarded, for the chat and the Team Lead.
pub fn discard(db: &Db, you: &str, id: &str, what_md: &str) -> Result<PendingAction> {
    db.write(Some(you), |w| {
        let c = w.conn();
        let a = get_in(c, id)?;
        if a.status != "pending" {
            return Err(not_waiting(&a));
        }
        let now = ids::now_ms();
        c.execute("UPDATE pending_actions SET status='discarded', decided_at=?2, decided_by=?3, result_md=?4, updated_at=?2 WHERE id=?1",
                  rusqlite::params![id, now, you, what_md])?;
        w.update("pending_actions", id, serde_json::json!({"status": "discarded"}))?;
        get_in(c, id)
    })
}

/// The chat's decisions the Team Lead hasn't heard yet (approved, failed or discarded), in the order they were made.
pub fn untold(db: &Db, thread_id: &str) -> Result<Vec<PendingAction>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("SELECT {COLS} FROM pending_actions WHERE thread_id=?1 AND status IN ('approved', 'failed', 'discarded')
                                         AND told_at IS NULL ORDER BY decided_at, rowid"))?;
        Ok(st.query_map([thread_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// The Team Lead heard these decisions (an answer started with them).
pub fn mark_told(db: &Db, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    db.write(None, |w| {
        let now = ids::now_ms();
        for id in ids {
            w.conn().execute("UPDATE pending_actions SET told_at=?2 WHERE id=?1 AND told_at IS NULL", rusqlite::params![id, now])?;
        }
        Ok(())
    })
}

/// The calls approved and not finished, in every chat: at start-up, those Gizai closed under (the app finishes them as
/// failed, so none runs twice and the Team Lead hears of them).
pub fn running(db: &Db) -> Result<Vec<PendingAction>> {
    db.read(|c| {
        let mut st = c.prepare(&format!("SELECT {COLS} FROM pending_actions WHERE status='running' ORDER BY rowid"))?;
        Ok(st.query_map([], row)?.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// One line of `line_diff`: `same`, `add` (only in the new text) or `del` (only in the old one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: &'static str,
    pub text: String,
}

/// More lines than this (old times new, after their common start and end) and `line_diff` shows the middle as removed
/// and added, without looking for lines they share.
const DIFF_CELLS: usize = 4_000_000;

/// What changes from `old` to `new`, line by line: the lines they share (`same`), in order, and those removed (`del`) and
/// added (`add`) around them, each removed run before the added one that replaces it.
pub fn line_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (am, bm) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let line = |kind: &'static str, t: &str| DiffLine { kind, text: t.to_string() };
    let mut out: Vec<DiffLine> = a[..head].iter().map(|t| line("same", t)).collect();
    if am.len().saturating_mul(bm.len()) > DIFF_CELLS {
        out.extend(am.iter().map(|t| line("del", t)));
        out.extend(bm.iter().map(|t| line("add", t)));
    } else {
        // The longest common subsequence of the middles, from the end: lcs[i][j] for am[i..] and bm[j..].
        let (n, m) = (am.len(), bm.len());
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if am[i] == bm[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        let (mut dels, mut adds) = (vec![], vec![]);
        while i < n || j < m {
            if i < n && j < m && am[i] == bm[j] {
                out.append(&mut dels);
                out.append(&mut adds);
                out.push(line("same", am[i]));
                i += 1;
                j += 1;
            } else if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) {
                dels.push(line("del", am[i]));
                i += 1;
            } else {
                adds.push(line("add", bm[j]));
                j += 1;
            }
        }
        out.append(&mut dels);
        out.append(&mut adds);
    }
    out.extend(a[a.len() - tail..].iter().map(|t| line("same", t)));
    out
}
