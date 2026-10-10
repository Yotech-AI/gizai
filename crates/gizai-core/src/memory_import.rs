//! Claude Code's own memory notes into Gizai's memory (GA-85). Before Gizai had memory (GA-19), an agent on Claude Code
//! kept notes in that CLI's own memory folder, `projects/<project folder>/memory/` in its account's folder
//! (CLAUDE_CONFIG_DIR, else ~/.claude): a Markdown file per note and `MEMORY.md` as their index. Only agents on that
//! account saw them, and Gizai didn't show them. When Gizai starts, each such file it hasn't imported yet becomes a note
//! in the Team Lead's folder, `Team Lead/Imported/<project folder>/<file name>`, with where it came from in its
//! properties, and `Team Lead/Notes` gets an open thread to sort them with the user. They start there so no agent gets
//! them before someone has read them. The files are only read, never changed. Gizai's own runs start Claude Code with
//! that memory off (gizai-agents' `claude::AUTO_MEMORY_OFF`). In plain language: `docs/memory.md`.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{Db, Writer};
use crate::memory::{self, IMPORTED, LEAD, Who};
use crate::{Error, Result, clis, limits, settings};

/// What became of every file found so far, by its full path: a setting.
const KEY: &str = "claude_memory_imports";
/// Claude Code's index of the other files in a memory folder: no note of its own.
const INDEX: &str = "MEMORY.md";
/// A file larger than this is no note: it is skipped.
pub const MAX_BYTES: u64 = 200_000;
/// The open thread lists at most this many characters in `Team Lead/Notes`; a longer list goes in a note of its own in
/// the import folder, linked from the thread.
pub const LIST_MAX: usize = 2_000;

/// A file imported: where it is (`~/.claude-2/projects/-home-me-shop/memory/deploy-SHOP.md`) and the note it became.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    pub file: String,
    pub note: String,
}

/// A file skipped: where it is and why, never its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub file: String,
    pub why: String,
}

/// What one import did; empty when there was nothing new.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub imported: Vec<Imported>,
    pub skipped: Vec<Skipped>,
    /// The note the list went in, when it was too long for `Team Lead/Notes`.
    pub list: Option<String>,
}

/// What became of a file: the note it became, or why it was skipped with its size and time then (a skipped file is tried
/// again once it changed).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Done {
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    skipped: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    stamp: String,
}

/// A Markdown file in a Claude Code memory folder that wasn't imported yet.
struct Found {
    /// Its account's folder as Settings writes it: `~/.claude-2`.
    account: String,
    /// Its folder under `projects/`: `-home-me-shop`.
    project: String,
    /// `deploy-SHOP.md`.
    file: String,
    /// Where it is, as lists show it: `~/.claude-2/projects/-home-me-shop/memory/deploy-SHOP.md`.
    shown: String,
    /// Its full path, which the setting keeps it by.
    key: String,
    /// Its size and modified time.
    stamp: String,
    /// Its text, or why it is no note.
    text: std::result::Result<String, String>,
}

/// The account folders of the Claude Code CLIs in Settings → Coding CLIs, each once (`limits::account_dir`: an entry's
/// CLAUDE_CONFIG_DIR line, else Gizai's own, `inherited`, else ~/.claude).
pub fn claude_dirs(db: &Db, home: &str, inherited: &dyn Fn(&str) -> Option<String>) -> Result<Vec<PathBuf>> {
    let mut out: Vec<(PathBuf, PathBuf)> = vec![];
    for cli in clis::list(db)?.iter().filter(|c| c.kind == "claude_code") {
        let Some(dir) = limits::account_dir(cli, home, inherited) else { continue };
        let real = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !out.iter().any(|(_, r)| *r == real) {
            out.push((dir, real));
        }
    }
    Ok(out.into_iter().map(|(dir, _)| dir).collect())
}

/// Imports the Markdown files in `projects/*/memory/` of each account folder in `dirs` that weren't imported yet, except
/// `MEMORY.md` (Claude Code's index of them) and the folders below. Each becomes a note
/// `Team Lead/Imported/<project folder>/<file name>` (` (2)` and on when two share that path), written by the Team Lead
/// (by `you_id` when there is none): its text as it was, with where it came from at the top of its properties. A file
/// with a secret in it, or that is no note (not text, too big, unreadable), is skipped: listed by its path, never its
/// text, and tried again once it changed. What was imported and skipped goes in `Team Lead/Notes` as an open thread,
/// dated `day`, to sort with `you`. `home` shows as `~`.
pub fn import(db: &Db, dirs: &[PathBuf], home: &str, you_id: &str, you: &str, day: &str) -> Result<Report> {
    let mut done: BTreeMap<String, Done> = settings::get(db, KEY)?.unwrap_or_default();
    let found: Vec<Found> = dirs.iter().flat_map(|d| scan(d, home, &done)).collect();
    if found.is_empty() {
        return Ok(Report::default());
    }
    let who = match db.read(memory::lead_in)? {
        Some(lead) => Who::Lead(lead),
        None => Who::Person(you_id.to_string()),
    };
    db.write(Some(who.id()), |w| {
        let mut report = Report::default();
        for f in found {
            match note_for(w, &who, &f)? {
                Ok((id, path)) => {
                    done.insert(f.key, Done { note: Some(id), ..Default::default() });
                    report.imported.push(Imported { file: f.shown, note: path });
                }
                Err(why) => {
                    done.insert(f.key, Done { skipped: Some(why.clone()), stamp: f.stamp, ..Default::default() });
                    report.skipped.push(Skipped { file: f.shown, why });
                }
            }
        }
        report.list = thread(w, &who, you, day, &report)?;
        // The links in the new notes, and the links elsewhere that name them, point at them now.
        memory::relink_all(w)?;
        settings::set_in(w, KEY, &done)?;
        Ok(report)
    })
}

/// The Markdown files in `projects/*/memory/` of account folder `dir`, by project folder and file name: not `MEMORY.md`,
/// not the folders below it, and not those `done` has (imported, or skipped and unchanged since).
fn scan(dir: &Path, home: &str, done: &BTreeMap<String, Done>) -> Vec<Found> {
    let account = limits::tilde(dir, home);
    let real = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut out = vec![];
    for (project, project_dir) in entries(&dir.join("projects")) {
        for (file, path) in entries(&project_dir.join("memory")) {
            if !file.to_lowercase().ends_with(".md") || file.eq_ignore_ascii_case(INDEX) {
                continue;
            }
            // A link is followed; a folder or a broken link is no note.
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() {
                continue;
            }
            let key = real.join("projects").join(&project).join("memory").join(&file).display().to_string();
            let stamp = format!("{} {}", meta.len(), modified_ms(&meta));
            if done.get(&key).is_some_and(|d| d.note.is_some() || d.stamp == stamp) {
                continue;
            }
            out.push(Found { account: account.clone(), project: project.clone(), shown: limits::tilde(&path, home), key, stamp,
                             text: read(&path, meta.len()), file });
        }
    }
    out
}

/// What is in folder `dir`, by name: (name, path). Nothing when it can't be read.
fn entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut out: Vec<(String, PathBuf)> = rd.filter_map(|e| e.ok()).map(|e| (e.file_name().to_string_lossy().to_string(), e.path())).collect();
    out.sort();
    out
}

fn modified_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A file's text, or why it is no note.
fn read(path: &Path, len: u64) -> std::result::Result<String, String> {
    if len > MAX_BYTES {
        return Err(format!("it is {} KB, too big for a note", len.div_ceil(1000)));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("it can't be read ({e})"))?;
    String::from_utf8(bytes).map_err(|_| "it isn't text (UTF-8)".to_string())
}

/// Makes the note for file `f`: its id and path, or why it was skipped. Errors only when the database does.
fn note_for(w: &Writer, who: &Who, f: &Found) -> Result<std::result::Result<(String, String), String>> {
    let text = match &f.text {
        Ok(t) => t,
        Err(why) => return Ok(Err(why.clone())),
    };
    let body = with_source(text, &f.account, &f.project, &f.file);
    if let Some(what) = memory::secret_in(&body) {
        // A private key's example (-----BEGIN … PRIVATE KEY-----) is what the check looks for itself: the reason goes in
        // Team Lead/Notes without it, else that line would look like a secret too.
        let what = match what.split_once(" (") {
            Some((kind, _)) if memory::secret_in(&what).is_some() => kind.to_string(),
            _ => what,
        };
        return Ok(Err(format!("it holds {what}")));
    }
    let c = w.conn();
    let base = format!("{LEAD}/{IMPORTED}/{}/{}", part(&f.project, 80, "Project"), part(stem(&f.file), 90, "Note"));
    let path = free_path(c, &base)?;
    match memory::insert_in(w, who, &path, &body, None) {
        Ok(id) => {
            let path: String = c.query_row("SELECT path FROM docs WHERE id = ?1", [&id], |r| r.get(0))?;
            Ok(Ok((id, path)))
        }
        // What memory refuses to keep says why, never the text.
        Err(Error::Invalid(why)) => Ok(Err(why)),
        Err(e) => Err(e),
    }
}

/// A file name without `.md`.
fn stem(file: &str) -> &str {
    let n = file.len();
    if n > 3 && file.is_char_boundary(n - 3) && file[n - 3..].eq_ignore_ascii_case(".md") { &file[..n - 3] } else { file }
}

/// `s` as a folder or title in a note's path: what a path can't hold as a dash, spaces tidied, no dot at either end, at
/// most `max` characters; `fallback` when nothing is left.
fn part(s: &str, max: usize, fallback: &str) -> String {
    let s: String = s.chars().map(|c| if memory::BAD_CHARS.contains(&c) || c.is_control() { '-' } else { c }).collect();
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(max).collect();
    let s = s.trim().trim_matches('.').trim();
    if s.is_empty() { fallback.to_string() } else { s.to_string() }
}

/// `base`, else `base (2)`, `base (3)` …: the first path no note has (case ignored).
fn free_path(c: &Connection, base: &str) -> Result<String> {
    let mut path = base.to_string();
    let mut n = 2;
    while memory::by_path_in(c, &path)?.is_some() {
        path = format!("{base} ({n})");
        n += 1;
    }
    Ok(path)
}

/// The note an imported file becomes: its text as it was, with where it came from at the top of its properties
/// (`source: claude-code`, the account's folder, the project folder and the file name). The file's own properties stay,
/// except any by those names.
pub fn with_source(text: &str, account: &str, project: &str, file: &str) -> String {
    let ours = [("source", memory::FROM_CLAUDE), ("claude_config_dir", account), ("claude_project", project), ("claude_file", file)];
    let head: String = ours.iter().map(|(k, v)| format!("{k}: {}\n", yaml(v))).collect();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    match frontmatter(text) {
        Some((props, rest)) => format!("---\n{head}{}---\n{rest}", without(props, &ours.map(|(k, _)| k))),
        None => format!("---\n{head}---\n\n{text}"),
    }
}

/// A text's properties (the lines between a first line `---` and the next `---` or `...`) and the rest after them; None
/// when it has none.
fn frontmatter(text: &str) -> Option<(&str, &str)> {
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut at = first.len();
    for l in lines {
        if matches!(l.trim_end(), "---" | "...") {
            return Some((&text[first.len()..at], &text[at + l.len()..]));
        }
        at += l.len();
    }
    None
}

/// Property lines without the properties named in `keys` (and the lines that go on from them: indented ones and list
/// items).
fn without(props: &str, keys: &[&str]) -> String {
    let mut out = String::new();
    let mut dropping = false;
    for line in props.split_inclusive('\n') {
        let t = line.trim_end();
        if !(t.is_empty() || line.starts_with([' ', '\t']) || t.starts_with('-') || t.starts_with('#')) {
            let key = t.split(':').next().unwrap_or_default().trim().trim_matches(['"', '\'']).to_lowercase();
            dropping = keys.contains(&key.as_str());
        }
        if !dropping {
            out.push_str(line);
        }
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// A value as a YAML property: as it is when it can stand plain, else in single quotes.
fn yaml(v: &str) -> String {
    let v: String = v.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let word = ["~", "null", "true", "false", "yes", "no", "on", "off"].iter().any(|w| v.eq_ignore_ascii_case(w)) || v.parse::<f64>().is_ok();
    let plain = !v.is_empty() && !word && v.trim() == v && !v.contains(": ") && !v.contains(" #") && !v.ends_with(':') && v != "-"
        && !v.starts_with("- ") && !v.starts_with(['!', '&', '*', '?', '|', '>', '\'', '"', '%', '@', '`', '[', ']', '{', '}', ',', '#']);
    if plain { v } else { format!("'{}'", v.replace('\'', "''")) }
}

/// Adds what was imported and skipped to `Team Lead/Notes` (made from its template when it isn't there) as an open
/// thread, with the agents whose own instructions still keep notes in Claude Code's memory folder. A list longer than
/// `LIST_MAX` goes in a note of its own in the import folder, linked from the thread: its path.
fn thread(w: &Writer, who: &Who, you: &str, day: &str, r: &Report) -> Result<Option<String>> {
    if r.imported.is_empty() && r.skipped.is_empty() {
        return Ok(None);
    }
    let c = w.conn();
    let notes = memory::lead_notes_path();
    if memory::by_path_in(c, &notes)?.is_none() {
        memory::insert_in(w, who, &notes, &memory::lead_template(you), None)?;
    }
    let folder = format!("{LEAD}/{IMPORTED}");
    // The notes by the folder they came from, each linked by its path after the import folder (a move rewrites it),
    // then the files skipped, by their path and why. Team Lead/Notes keeps no secret either: a path that looks like one
    // isn't written, and a reason that would make its line look like one is left out.
    let mut groups: Vec<(&str, Vec<String>)> = vec![];
    for i in &r.imported {
        let from = i.file.rsplit_once(['/', '\\']).map(|(f, _)| f).unwrap_or_default();
        let link = format!("[[{}]]", i.note.strip_prefix(&format!("{folder}/")).unwrap_or(&i.note));
        match groups.iter_mut().find(|(f, _)| *f == from) {
            Some((_, links)) => links.push(link),
            None => groups.push((from, vec![link])),
        }
    }
    let count = |k: usize, what: &str| if k == 1 { format!("1 {what}") } else { format!("{k} {what}s") };
    let secret = |s: &str| memory::secret_in(s).is_some();
    let list: Vec<String> = groups.iter().map(|(from, links)| match format!("- From {from}: {}", links.join(", ")) {
            l if secret(&l) => format!("- From a folder whose path, or a note's, looks like a secret (not shown): {}", count(links.len(), "note")),
            l => l,
        })
        .chain(r.skipped.iter().map(|s| match format!("- Not imported: {} ({}; nothing of its text was kept)", s.file, s.why) {
            _ if secret(&s.file) => "- Not imported: a file whose path looks like a secret (not shown)".to_string(),
            l if secret(&l) => format!("- Not imported: {} (nothing of its text was kept)", s.file),
            l => l,
        }))
        .collect();
    let (n, k) = (r.imported.len(), r.skipped.len());
    let mut text = if n > 0 {
        let deploy = r.imported.iter().map(|i| memory::title_of(&i.note))
            .find(|t| t.len() > 7 && t.is_char_boundary(7) && t[..7].eq_ignore_ascii_case("deploy-"));
        let example = deploy.map(|t| format!(" (like {t} to Deployments/{})", &t[7..])).filter(|e| !secret(e)).unwrap_or_default();
        let skipped = if k > 0 { format!("; {} could not be imported", count(k, "file")) } else { String::new() };
        format!("- {day}: Imported {n} note{} from Claude Code's own memory into {folder}/, a folder per Claude Code project, each \
with where it came from in its properties{skipped}. Sort them with {you}: a project's deploy note goes to Deployments/<KEY>{example}, \
with project: <KEY> and applies_to: devops; the others to a shared folder or an agent's own (memory_move), or they stay. memory_list \
with folder \"{folder}\" lists them.", if n == 1 { "" } else { "s" })
    } else {
        format!("- {day}: {} in Claude Code's own memory could not be imported into {folder}/:", count(k, "file"))
    };
    let mut listed = None;
    if list.iter().map(|l| l.chars().count() + 3).sum::<usize>() <= LIST_MAX {
        for l in &list {
            text.push_str(&format!("\n  {l}"));
        }
    } else {
        let path = free_path(c, &format!("{folder}/From Claude Code {day}"))?;
        let body = format!("---\ntype: note\ntags: [imported]\n---\n# From Claude Code {day}\n\nWhat came from Claude Code's own memory on \
{day}, and the files that could not come; the open thread in [[{notes}]] says what to do with them.\n\n{}\n", list.join("\n"));
        memory::insert_in(w, who, &path, &body, None)?;
        text.push_str(&format!("\n  - The list: [[{path}]]"));
        listed = Some(path);
    }
    // Agents made before this keep their own copy of their instructions, which may still say to keep notes there.
    let stale: Vec<String> = {
        let mut st = c.prepare("SELECT a.name FROM agent_configs g JOIN actors a ON a.id = g.actor_id
                                WHERE a.deleted_at IS NULL AND g.instructions_md LIKE '%MEMORY.md%' ORDER BY a.name COLLATE NOCASE")?;
        st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if let Some((last, first)) = stale.split_last() {
        let names = if first.is_empty() { last.clone() } else { format!("{} and {last}", first.join(", ")) };
        text.push_str(&format!("\n  - The instructions of {names} still say to keep notes in Claude Code's memory folder (MEMORY.md), \
which Gizai's runs switch off now: update them with {you}, as the DevOps role now keeps a project's deploy note in Deployments/<KEY>."));
    }
    memory::append_in(w, who, &notes, Some("Open threads"), &text, None)?;
    Ok(listed)
}
