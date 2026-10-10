//! The memory tools (GA-19): the Team Lead lists, searches, reads, writes, appends to and moves the notes in Gizai's
//! Memory (`gizai_core::memory`), as itself, so the activity feed shows it did. The Team Lead sees every scope.
use gizai_core::memory::{self, Note, Who};
use serde_json::{Value, json};

use super::{Args, Cx, err, ymd};

/// The caller as memory sees it. The agent with Chat on is the Team Lead, whatever its role says.
fn who(cx: &Cx) -> Result<Who, String> {
    let who = Who::of(cx.db(), cx.actor).map_err(err)?;
    let chat = gizai_core::team::chat_agent(cx.db()).ok().flatten().is_some_and(|a| a.actor_id == cx.actor);
    Ok(match who {
        Who::Agent(id) if chat => Who::Lead(id),
        other => other,
    })
}

fn link(n: &Note) -> Value {
    json!({"page": "doc", "id": n.id, "label": n.path})
}

fn note_json(n: &Note) -> Value {
    json!({"id": n.id, "path": n.path, "scope": n.scope, "version": n.current_version, "updated": ymd(n.updated_at),
           "updated_by": n.updated_by, "chars": n.chars})
}

pub(crate) fn list(cx: &Cx, a: &Args) -> Result<Value, String> {
    let who = who(cx)?;
    let folder = a.opt("folder").map(|f| format!("{}/", f.trim_matches('/').to_lowercase()));
    let notes: Vec<Value> = memory::list(cx.db(), &who).map_err(err)?.iter()
        .filter(|n| folder.as_ref().is_none_or(|f| n.path.to_lowercase().starts_with(f.as_str())))
        .map(note_json).collect();
    Ok(json!({"notes": notes, "shared_folders": memory::SHARED_FOLDERS, "own_folders": [format!("{}/", memory::LEAD), format!("{}/<agent name>/", memory::AGENTS)]}))
}

pub(crate) fn search(cx: &Cx, a: &Args) -> Result<Value, String> {
    let who = who(cx)?;
    let limit = a.int("limit")?.unwrap_or(20).clamp(1, 100) as usize;
    let hits: Vec<Value> = memory::search(cx.db(), &who, &a.req("query")?, limit).map_err(err)?.into_iter()
        .map(|h| json!({"path": h.note.path, "version": h.note.current_version, "updated": ymd(h.note.updated_at), "line": h.snippet}))
        .collect();
    Ok(json!({"notes": hits}))
}

pub(crate) fn read(cx: &Cx, a: &Args) -> Result<Value, String> {
    let who = who(cx)?;
    let n = memory::get(cx.db(), &who, &a.req("note")?).map_err(err)?;
    let backlinks: Vec<String> = memory::backlinks(cx.db(), &who, &n.id).map_err(err)?.into_iter().map(|b| b.path).collect();
    Ok(json!({"note": {"id": n.id, "path": n.path, "scope": n.scope, "version": n.current_version, "updated": ymd(n.updated_at),
                       "updated_by": n.updated_by, "body_md": n.body_md, "linked_from": backlinks}, "link": link(&n)}))
}

pub(crate) fn write(cx: &Cx, a: &Args) -> Result<Value, String> {
    let who = who(cx)?;
    let body = a.text("body_md").ok_or("missing \"body_md\"")?;
    let s = memory::write(cx.db(), &who, &a.req("path")?, &body, a.int("version")?, None).map_err(err)?;
    saved(cx, &who, s)
}

pub(crate) fn append(cx: &Cx, a: &Args) -> Result<Value, String> {
    let who = who(cx)?;
    let s = memory::append(cx.db(), &who, &a.req("note")?, a.opt("heading").as_deref(), &a.req("text")?, None).map_err(err)?;
    saved(cx, &who, s)
}

pub(crate) fn move_note(cx: &Cx, a: &Args) -> Result<Value, String> {
    let who = who(cx)?;
    let copy = a.flag("copy").unwrap_or(false);
    let n = memory::move_note(cx.db(), &who, &a.req("note")?, &a.req("to")?, copy).map_err(err)?;
    cx.changed("docs");
    Ok(json!({"ok": true, "copied": copy, "note": note_json(&n), "link": link(&n)}))
}

fn saved(cx: &Cx, who: &Who, s: memory::Saved) -> Result<Value, String> {
    cx.changed("docs");
    let n = memory::get(cx.db(), who, &s.id).map_err(err)?;
    Ok(json!({"ok": true, "created": s.created, "note": note_json(&n), "link": link(&n)}))
}
