//! Screenshots (GA-182, from GA-173's research: `docs/research/screenshots.md`). An agent that builds something with a
//! screen saves screenshots of it in its run's `.gizai-tmp/screenshots/`; at the run's end Gizai checks them
//! (`gizai_agents::screenshots::collect`) and keeps them on the card, each with the run's id (`files::insert_evidence`).
//! Here: the settings (the project's, the card's and Settings → Agents and runs → Screenshots) with their defaults and
//! ranges, a card's sets for its Screenshots tab (contract CS), and the clearing of old ones in housekeeping (`prune`,
//! then `sweep_blobs`).
use std::collections::HashSet;
use std::ops::RangeInclusive;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::model::{FileRow, Project, Task};
use crate::{Error, Result, files, settings};

/// One value of Settings → Agents and runs → Screenshots: its settings key, default and range, and how messages name it.
#[derive(Debug, Clone)]
pub struct Setting {
    pub key: &'static str,
    pub default: u32,
    pub range: RangeInclusive<u32>,
    /// Its label: "Screenshots per run".
    pub name: &'static str,
    /// After a number in a message: " MB", " days" or "".
    pub unit: &'static str,
}

pub const PER_RUN: Setting = Setting { key: "screenshots_per_run", default: 12, range: 1..=30, name: "Screenshots per run", unit: "" };
pub const MAX_MB: Setting = Setting { key: "screenshot_max_mb", default: 10, range: 1..=50, name: "Largest screenshot", unit: " MB" };
pub const KEEP_SETS: Setting = Setting { key: "screenshots_keep_sets", default: 3, range: 1..=20, name: "Keep per card", unit: " sets" };
pub const KEEP_DAYS: Setting = Setting {
    key: "screenshots_keep_days", default: 30, range: 0..=365, name: "Delete a finished card's screenshots after", unit: " days",
};
pub const PORT_FROM: Setting = Setting { key: "preview_port_from", default: 47100, range: 1024..=65535, name: "Preview ports", unit: "" };
pub const PORT_TO: Setting = Setting { key: "preview_port_to", default: 47999, range: 1024..=65535, name: "Preview ports", unit: "" };
/// A run's preview gets a block of this many ports: its own and the nine after it (for a second server, an API or Vite).
pub const PORTS_PER_RUN: u32 = 10;

/// Settings → Agents and runs → Screenshots, as saved; each value its default until it is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Screenshots per run: Gizai keeps the first ones by file name.
    pub per_run: u32,
    /// Largest screenshot, in MB.
    pub max_mb: u32,
    /// Keep per card: a card's newest sets with pictures.
    pub keep_sets: u32,
    /// Delete a finished card's screenshots after this many days in Done or Cancelled; 0 keeps them.
    pub keep_days: u32,
    /// Preview ports: the range runs' previews get their blocks of ten from.
    pub port_from: u32,
    pub port_to: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { per_run: PER_RUN.default, max_mb: MAX_MB.default, keep_sets: KEEP_SETS.default, keep_days: KEEP_DAYS.default,
                 port_from: PORT_FROM.default, port_to: PORT_TO.default }
    }
}

impl Limits {
    /// Largest screenshot in bytes.
    pub fn max_bytes(&self) -> u64 {
        u64::from(self.max_mb) * 1024 * 1024
    }
}

/// The values saved in Settings → Agents and runs → Screenshots; a value missing or outside its range is its default.
pub fn limits(db: &Db) -> Limits {
    let get = |s: &Setting| settings::get::<u32>(db, s.key).ok().flatten().filter(|v| s.range.contains(v)).unwrap_or(s.default);
    let (port_from, port_to) = (get(&PORT_FROM), get(&PORT_TO));
    let (port_from, port_to) = if port_from <= port_to { (port_from, port_to) } else { (PORT_FROM.default, PORT_TO.default) };
    Limits { per_run: get(&PER_RUN), max_mb: get(&MAX_MB), keep_sets: get(&KEEP_SETS), keep_days: get(&KEEP_DAYS), port_from, port_to }
}

/// How many runs' previews fit in the ports `from` to `to`: blocks of ten.
pub fn port_blocks(from: u32, to: u32) -> u32 {
    if to < from { 0 } else { (to - from + 1) / PORTS_PER_RUN }
}

/// Refuses a value outside its range, with why, as Settings → Runs does: "Screenshots per run must be from 1 to 30.".
/// The preview ports must hold a block of ten for each run that may run at once (`runs_at_once`, Settings → Runs).
pub fn check_limits(l: &Limits, runs_at_once: u32) -> std::result::Result<(), String> {
    for (v, s) in [(l.per_run, &PER_RUN), (l.max_mb, &MAX_MB), (l.keep_sets, &KEEP_SETS), (l.keep_days, &KEEP_DAYS),
                   (l.port_from, &PORT_FROM), (l.port_to, &PORT_TO)] {
        if !s.range.contains(&v) {
            return Err(format!("{} must be from {} to {}{}.", s.name, s.range.start(), s.range.end(), s.unit));
        }
    }
    if l.port_from > l.port_to {
        return Err(format!("Preview ports must go from the lower port to the higher one, not from {} to {}.", l.port_from, l.port_to));
    }
    let room = port_blocks(l.port_from, l.port_to);
    if room < runs_at_once {
        return Err(format!("Preview ports must hold at least ten ports for each run at once: {} to {} has room for {room} {}, and Runs at \
once (Settings → Runs) is {runs_at_once}.", l.port_from, l.port_to, if room == 1 { "run" } else { "runs" }));
    }
    Ok(())
}

/// Saves Settings → Agents and runs → Screenshots, checked as `check_limits` does.
pub fn save_limits(db: &Db, l: &Limits, runs_at_once: u32) -> Result<()> {
    check_limits(l, runs_at_once).map_err(Error::Invalid)?;
    db.write(None, |w| {
        for (s, v) in [(&PER_RUN, l.per_run), (&MAX_MB, l.max_mb), (&KEEP_SETS, l.keep_sets), (&KEEP_DAYS, l.keep_days),
                       (&PORT_FROM, l.port_from), (&PORT_TO, l.port_to)] {
            settings::set_in(w, s.key, &v)?;
        }
        Ok(())
    })
}

/// One size a project's screenshots are taken at (project form → Preview → Sizes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotSize {
    /// "Desktop", "Mobile".
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// A project's screenshots (project form → Preview), saved in `projects.screenshots_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectScreenshots {
    /// Screenshots for the project: off, no agent takes any on its cards. On by default.
    pub on: bool,
    /// 1 to 4 sizes; Desktop 1280 × 800 and Mobile 390 × 844 by default, for every project.
    pub sizes: Vec<ShotSize>,
    /// Both themes: each changed screen in the first theme, and the card's main screen in the other one too. Off.
    pub both_themes: bool,
    /// Whole page: the full length of each page, not one window's height. Off.
    pub full_page: bool,
}

impl Default for ProjectScreenshots {
    fn default() -> Self {
        ProjectScreenshots { on: true, sizes: default_sizes(), both_themes: false, full_page: false }
    }
}

/// Desktop 1280 × 800 and Mobile 390 × 844: every project's sizes until its form sets its own.
pub fn default_sizes() -> Vec<ShotSize> {
    vec![ShotSize { name: "Desktop".into(), width: 1280, height: 800 }, ShotSize { name: "Mobile".into(), width: 390, height: 844 }]
}

/// How many sizes a project has, and the width and height of each.
pub const SIZES: RangeInclusive<usize> = 1..=4;
pub const WIDTH: RangeInclusive<u32> = 320..=3840;
pub const HEIGHT: RangeInclusive<u32> = 320..=2400;
/// The longest name of a size.
const SIZE_NAME_CHARS: usize = 40;
/// Start time: how long Gizai waits for a project's preview to answer, in seconds. 120 by default.
pub const DEFAULT_START_SECS: u32 = 120;
pub const START_SECS: RangeInclusive<u32> = 10..=600;

/// The project's screenshots as saved (`projects.screenshots_json`): the defaults when nothing is saved or it can't be
/// read.
pub fn parse_project(raw: Option<&str>) -> ProjectScreenshots {
    raw.and_then(|j| serde_json::from_str::<ProjectScreenshots>(j).ok()).filter(|s| check_project(s).is_ok()).unwrap_or_default()
}

/// The project's screenshots, tidied (names trimmed), or why they can't be saved: 1 to 4 sizes, each with a name and 320
/// to 3,840 pixels wide and 320 to 2,400 high.
pub fn check_project(s: &ProjectScreenshots) -> Result<ProjectScreenshots> {
    if !SIZES.contains(&s.sizes.len()) {
        return Err(Error::Invalid(format!("Screenshots need 1 to 4 sizes, not {}.", s.sizes.len())));
    }
    let mut sizes = vec![];
    for size in &s.sizes {
        let name: String = size.name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() || name.chars().count() > SIZE_NAME_CHARS || name.chars().any(char::is_control) {
            return Err(Error::Invalid(format!("Each size needs a name of at most {SIZE_NAME_CHARS} characters, like Desktop.")));
        }
        if !WIDTH.contains(&size.width) {
            return Err(Error::Invalid(format!("{name} must be from 320 to 3,840 pixels wide, not {}.", size.width)));
        }
        if !HEIGHT.contains(&size.height) {
            return Err(Error::Invalid(format!("{name} must be from 320 to 2,400 pixels high, not {}.", size.height)));
        }
        sizes.push(ShotSize { name, width: size.width, height: size.height });
    }
    Ok(ProjectScreenshots { sizes, ..s.clone() })
}

/// Refuses a Start time outside 10 to 600 seconds, with why.
pub fn check_start_secs(secs: u32) -> Result<()> {
    if START_SECS.contains(&secs) {
        Ok(())
    } else {
        Err(Error::Invalid(format!("Start time must be from {} to {} seconds.", START_SECS.start(), START_SECS.end())))
    }
}

/// Whether agents take screenshots on this card: its own Screenshots before Review switch (Properties), else its
/// project's Screenshots switch.
pub fn card_on(task: &Task, project: &Project) -> bool {
    task.screenshots.unwrap_or(project.screenshots.on)
}

/// What a run's set says besides its pictures (`runs.screenshots_note`): what its collect left out and why, and why
/// there was no preview, when there wasn't.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunNote {
    /// "13-board.png: over the limit of 12 a run", "notes.txt: not a PNG, JPEG or WebP image".
    pub left_out: Vec<String>,
    /// "It didn't answer on port 47130 in 120 s".
    pub preview: Option<String>,
    /// While the run's preview runs: the id of the process Gizai started for it, which leads its process group, so a Gizai
    /// that starts again after a crash ends it (`gizai_agents::process::end_orphan_group`). Not shown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_pid: Option<u32>,
}

/// Marks `run_id` as a run that got the Screenshots section, so its set shows on the card's tab also when it saves none.
pub fn begin(db: &Db, run_id: &str) -> Result<()> {
    write_note(db, run_id, &RunNote::default())
}

/// Changes the note of a run `begin` marked; a run without one has no set, and keeps none.
fn change_note(db: &Db, run_id: &str, change: impl FnOnce(&mut RunNote)) -> Result<()> {
    let Some(mut note) = run_note(db, run_id)? else { return Ok(()) };
    change(&mut note);
    write_note(db, run_id, &note)
}

/// Why the run had no preview: "It didn't answer on port 47130 in 120 s".
pub fn set_preview_why(db: &Db, run_id: &str, why: &str) -> Result<()> {
    change_note(db, run_id, |n| n.preview = Some(why.to_string()))
}

/// The process the run's preview runs in while it runs (`RunNote::preview_pid`); None once it has ended.
pub fn set_preview_pid(db: &Db, run_id: &str, pid: Option<u32>) -> Result<()> {
    change_note(db, run_id, |n| n.preview_pid = pid)
}

/// Saves what the run's collect left out, keeping the rest of its note.
pub fn set_left_out(db: &Db, run_id: &str, left_out: &[String]) -> Result<()> {
    change_note(db, run_id, |n| n.left_out = left_out.to_vec())
}

/// The run's note, None when it had no Screenshots section (or its set was cleared).
pub fn run_note(db: &Db, run_id: &str) -> Result<Option<RunNote>> {
    let raw: Option<Option<String>> = db.read(|c| {
        Ok(c.query_row("SELECT screenshots_note FROM runs WHERE id = ?1", [run_id], |r| r.get(0)).optional()?)
    })?;
    let raw = raw.ok_or_else(|| Error::NotFound(format!("run {run_id}")))?;
    Ok(raw.map(|j| serde_json::from_str(&j).unwrap_or_default()))
}

fn write_note(db: &Db, run_id: &str, note: &RunNote) -> Result<()> {
    db.write(None, |w| {
        let n = w.conn().execute("UPDATE runs SET screenshots_note = ?2 WHERE id = ?1", rusqlite::params![run_id, serde_json::to_string(note)?])?;
        if n == 0 {
            return Err(Error::NotFound(format!("run {run_id}")));
        }
        Ok(())
    })
}

/// One run's set on the card's Screenshots tab (contract CS, `src/types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotSet {
    pub run_id: String,
    pub agent_id: String,
    pub agent_name: String,
    /// The role the agent ran in: "frontend", "design", "qa", …
    pub role: Option<String>,
    /// "ready_for_testing", "qa_pass", "qa_fail", "needs_decision"; None without a result line.
    pub outcome: Option<String>,
    /// "succeeded", "failed", "cancelled" or "timed_out".
    pub status: String,
    /// When the run ended, in ms since 1970.
    pub ended_at: i64,
    /// The commit the run ended at (`runs.head_sha`): the commit in each run's heading.
    pub commit: Option<String>,
    /// Its pictures, by file name.
    pub shots: Vec<Screenshot>,
    /// What its collect left out, and why.
    pub left_out: Vec<String>,
    /// Why there was no preview, when there wasn't.
    pub preview: Option<String>,
}

/// One screenshot of a set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screenshot {
    pub file_id: String,
    /// "01-board-with-cards-desktop-dark.png".
    pub name: String,
    /// From captions.json, else made from the name.
    pub caption: String,
    pub width: i64,
    pub height: i64,
    pub size_bytes: i64,
    /// The same picture in two runs has the same hash: Compare says Unchanged.
    pub sha256: String,
}

/// The card's screenshot sets, newest run first: every ended run of the card that got the Screenshots section, also
/// one that saved none (its `shots` is empty). A run still at work has no set yet.
pub fn sets(db: &Db, task_id: &str) -> Result<Vec<ScreenshotSet>> {
    db.read(|c| {
        let mut runs = c.prepare(
            "SELECT r.id, r.agent_actor_id, COALESCE(a.name, ''), r.role_key, r.outcome, r.status, COALESCE(r.ended_at, r.updated_at),
                    r.head_sha, r.screenshots_note
             FROM runs r LEFT JOIN actors a ON a.id = r.agent_actor_id
             WHERE r.task_id = ?1 AND r.deleted_at IS NULL AND r.screenshots_note IS NOT NULL
               AND r.status IN ('succeeded', 'failed', 'cancelled', 'timed_out')
             ORDER BY r.created_at DESC, r.id DESC")?;
        let mut shots = c.prepare(
            "SELECT id, name, COALESCE(caption, name), COALESCE(width, 0), COALESCE(height, 0), size_bytes, sha256 FROM files
             WHERE run_id = ?1 AND owner_type = 'task' AND owner_id = ?2 AND deleted_at IS NULL ORDER BY name, id")?;
        let found = runs.query_map([task_id], |r| {
            let note: String = r.get(8)?;
            let outcome: Option<String> = r.get(4)?;
            Ok((ScreenshotSet {
                run_id: r.get(0)?, agent_id: r.get(1)?, agent_name: r.get(2)?, role: r.get(3)?,
                // no_result and error: the run ended without a result line Gizai could read
                outcome: outcome.filter(|o| !matches!(o.as_str(), "no_result" | "error")),
                status: r.get(5)?, ended_at: r.get(6)?, commit: r.get(7)?, shots: vec![], left_out: vec![], preview: None,
            }, note))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut out = vec![];
        for (mut set, note) in found {
            let note: RunNote = serde_json::from_str(&note).unwrap_or_default();
            set.left_out = note.left_out;
            set.preview = note.preview;
            set.shots = shots.query_map([set.run_id.as_str(), task_id], |r| Ok(Screenshot {
                file_id: r.get(0)?, name: r.get(1)?, caption: r.get(2)?, width: r.get(3)?, height: r.get(4)?, size_bytes: r.get(5)?,
                sha256: r.get(6)?,
            }))?.collect::<rusqlite::Result<Vec<_>>>()?;
            out.push(set);
        }
        Ok(out)
    })
}

/// The file `file_id` when it is a run's screenshot (a PNG, JPEG or WebP a run kept on its card); any other file,
/// attachments included, is refused.
pub fn shot(db: &Db, file_id: &str) -> Result<FileRow> {
    db.read(|c| {
        c.query_row(&format!("SELECT {} FROM files WHERE id = ?1 AND run_id IS NOT NULL AND owner_type = 'task' AND deleted_at IS NULL
                                AND mime IN ('image/png', 'image/jpeg', 'image/webp')", files::ROW_COLS), [file_id], files::row)
            .optional()?
            .ok_or_else(|| Error::Invalid("That file isn't a run's screenshot: only screenshots are shown here.".into()))
    })
}

/// A run's screenshot's bytes (`shot`), read from the blob store by its content hash, never from a path given.
pub fn bytes(db: &Db, data_dir: &Path, file_id: &str) -> Result<Vec<u8>> {
    let f = shot(db, file_id)?;
    std::fs::read(files::blob_path(data_dir, &f.sha256))
        .map_err(|e| Error::Invalid(format!("Couldn't read the screenshot {}: {e}", f.name)))
}

/// What `prune` cleared.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Pruned {
    /// Runs whose set went.
    pub sets: usize,
    /// Screenshots (file rows) that went.
    pub files: usize,
}

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Clears old screenshots (housekeeping, at start and daily). Of each card: the sets past its newest `keep_sets` sets with
/// pictures, with the runs before them; and every set of a card that has been in Done or Cancelled for longer than
/// `keep_days` days (counted from when it was done, else from its last change; 0 keeps them). A set that goes loses its
/// screenshots' rows and its note, so it no longer shows on the tab. Only a run's screenshots go: what people attached
/// (files without a run) is never touched. Their blobs go in `sweep_blobs`.
pub fn prune(db: &Db, now_ms: i64, keep_sets: u32, keep_days: u32) -> Result<Pruned> {
    db.write(None, |w| {
        let c = w.conn();
        // Every run with a set, by card, newest first, with whether it has pictures.
        let mut st = c.prepare(
            "SELECT r.task_id, r.id, EXISTS (SELECT 1 FROM files f WHERE f.run_id = r.id AND f.deleted_at IS NULL),
                    t.state_category IN ('done', 'cancelled'), COALESCE(t.completed_at, t.updated_at)
             FROM runs r JOIN tasks t ON t.id = r.task_id
             WHERE r.screenshots_note IS NOT NULL OR EXISTS (SELECT 1 FROM files f WHERE f.run_id = r.id)
             ORDER BY r.task_id, r.created_at DESC, r.id DESC")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? != 0,
                                            r.get::<_, i64>(3)? != 0, r.get::<_, i64>(4)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(st);
        let mut clear: Vec<String> = vec![];
        let (mut card, mut with_pictures) = (String::new(), 0u32);
        for (task, run, pictures, finished, since) in rows {
            if task != card {
                card = task.clone();
                with_pictures = 0;
            }
            let expired = finished && keep_days > 0 && now_ms - since > i64::from(keep_days) * DAY_MS;
            // past the newest keep_sets sets with pictures: this run and every older one go
            let past = with_pictures >= keep_sets;
            if pictures && !past {
                with_pictures += 1;
            }
            if expired || past {
                clear.push(run);
            }
        }
        let mut pruned = Pruned::default();
        for run in &clear {
            let ids: Vec<String> = {
                let mut q = c.prepare("SELECT id FROM files WHERE run_id = ?1")?;
                q.query_map([run], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for id in &ids {
                c.execute("DELETE FROM files WHERE id = ?1 AND run_id IS NOT NULL", [id])?;
                w.delete("files", id)?;
            }
            pruned.files += ids.len();
            if c.execute("UPDATE runs SET screenshots_note = NULL WHERE id = ?1 AND screenshots_note IS NOT NULL", [run])? > 0 || !ids.is_empty() {
                pruned.sets += 1;
            }
        }
        Ok(pruned)
    })
}

/// How long a blob no row uses is left alone, so one stored a moment ago for a row about to be written isn't taken.
pub const BLOB_GRACE_MS: i64 = DAY_MS;

/// Deletes the blobs in `data_dir/files` that no row uses any more (housekeeping, after `prune`): any kind of file, as
/// long as no `files` row names it (live or removed: a removed attachment keeps its blob, as `files::remove` promises)
/// and no artifact version names it. Never one a row still uses, nor one changed in the last day (`BLOB_GRACE_MS`): it
/// may have been stored for a row not written yet. Only names that are a blob's (`<2 hex>/<64 hex>`) count; links and
/// everything else stay. Done inside one write, so no row can be written meanwhile. Returns how many went.
pub fn sweep_blobs(db: &Db, data_dir: &Path, now_ms: i64) -> Result<usize> {
    let root = data_dir.join("files");
    if !std::fs::symlink_metadata(&root).is_ok_and(|m| m.is_dir()) {
        return Ok(0);
    }
    let before = UNIX_EPOCH + Duration::from_millis((now_ms - BLOB_GRACE_MS).max(0) as u64);
    db.write(None, |w| {
        let c = w.conn();
        let mut used: HashSet<String> = HashSet::new();
        for sql in ["SELECT sha256 FROM files", "SELECT sha256 FROM artifact_versions",
                    "SELECT thumbnail_sha256 FROM artifact_versions WHERE thumbnail_sha256 IS NOT NULL"] {
            let mut st = c.prepare(sql)?;
            for sha in st.query_map([], |r| r.get::<_, String>(0))? {
                used.insert(sha?.to_ascii_lowercase());
            }
        }
        let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let mut gone = 0;
        for dir in std::fs::read_dir(&root)?.flatten() {
            let prefix = dir.file_name().to_string_lossy().into_owned();
            // read_dir's file_type doesn't follow a link
            if !hex(&prefix, 2) || !dir.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(dir.path()) else { continue };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !hex(&name, 64) || !name.starts_with(&prefix) || used.contains(&name) || !e.file_type().is_ok_and(|t| t.is_file()) {
                    continue;
                }
                if !e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < before) {
                    continue;
                }
                if remove_blob(&e.path()).is_ok() {
                    gone += 1;
                }
            }
        }
        Ok(gone)
    })
}

/// Deletes a blob. Blobs are read-only, which on Windows stops a delete: it is made writable first there.
fn remove_blob(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let mut perm = std::fs::metadata(path)?.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        std::fs::set_permissions(path, perm)?;
    }
    std::fs::remove_file(path)
}
