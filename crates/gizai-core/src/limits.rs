//! Subscription limits (the Usage page's Subscription tab, GA-62): the newest reading of each limit of each coding CLI in
//! Settings → Coding CLIs, so two accounts of one CLI (Claude Code and Claude Code 2) keep their own numbers.
//!
//! Gizai reads only what the unmodified CLIs write, never their login files or the vendors' usage endpoints:
//! - Claude Code writes a `rate_limit_event` line in its stream (task runs, chat turns, board checks) when what it heard of
//!   the account's limits changes. Its `rate_limit_info` has `unifiedWindows`: the session (`five_hour`), weekly
//!   (`seven_day`) and Fable (`seven_day_overage_included`) windows, each with `utilization` (0 to 1, more past the cap)
//!   and `resetsAt` (Unix seconds), on a Claude subscription only (an API key gets none); and the limit that counts now
//!   (`rateLimitType`) with its `status` (allowed, allowed_warning, rejected). From Claude Code 2.1.289's own schema.
//! - Codex reports none in `codex exec --json`, but writes a `token_count` event with `rate_limits` (its plan's `primary`
//!   and `secondary` windows: `used_percent`, `window_minutes`, `resets_at`) in its session log,
//!   `$CODEX_HOME/sessions/<year>/<month>/<day>/rollout-<time>-<thread>.jsonl`. Gizai reads the log of its own run after the
//!   run, and never writes there. From Codex 0.154's protocol.
//! - A run that hit a limit says so in its result ("You've hit your session limit · resets 3pm"): a reading too.
//!
//! Readings are kept in the `subscription_limits` setting (no schema change), newest per CLI and limit; an older one never
//! replaces a newer one. Nothing is made up: a limit without a reading has none, and every reading keeps when it was made.
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::clis::{self, Cli};
use crate::db::Db;
use crate::{Error, Result, settings};

const KEY: &str = "subscription_limits";

/// Claude Code's session limit: a 5-hour window.
pub const FIVE_HOUR: &str = "five_hour";
/// Claude Code's weekly limit.
pub const SEVEN_DAY: &str = "seven_day";
/// Claude Code's Fable limit: a weekly window of its own for the Fable model ("overage included").
pub const FABLE: &str = "seven_day_overage_included";
/// Claude Code's weekly Opus and Sonnet limits (older plans).
pub const OPUS: &str = "seven_day_opus";
pub const SONNET: &str = "seven_day_sonnet";
/// A limit Claude Code said was reached without naming it ("Claude AI usage limit reached").
pub const USAGE: &str = "usage";
/// Codex's plan windows: the first (usually 5 hours) and the second (usually a week).
pub const PRIMARY: &str = "primary";
pub const SECONDARY: &str = "secondary";

/// The limits a Claude Code account always shows, in this order; others follow when one is read.
const CLAUDE_LIMITS: [&str; 3] = [FIVE_HOUR, SEVEN_DAY, FABLE];
const CODEX_LIMITS: [&str; 2] = [PRIMARY, SECONDARY];
/// The window Codex's plans usually have, for the name of a window not read yet ("from memory": 5 hours and a week).
const CODEX_USUAL_MINUTES: [(&str, i64); 2] = [(PRIMARY, 300), (SECONDARY, 10_080)];

/// How many date folders of Codex's session logs `codex_log_path` looks through, newest first.
const CODEX_DAYS: usize = 62;
/// How much of the end of a Codex session log `codex_limits` reads: its newest `token_count` events are there.
const CODEX_TAIL: u64 = 8 * 1024 * 1024;

/// One reading of one limit of one coding CLI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Reading {
    /// Which limit: five_hour, seven_day, seven_day_overage_included (Fable), seven_day_opus, seven_day_sonnet or usage
    /// (Claude Code); primary or secondary (Codex).
    pub key: String,
    /// How much of it was used, in % (0 to 100, more when usage ran past the cap). None: the CLI didn't say (a limit it
    /// said was reached, in words).
    pub used_percent: Option<f64>,
    /// As the CLI said it: allowed, allowed_warning (near the limit) or rejected (reached).
    pub status: Option<String>,
    /// When the window resets (Unix ms).
    pub resets_at: Option<i64>,
    /// When it resets as the CLI wrote it ("3pm (Europe/Amsterdam)"), when it gave no time stamp.
    pub resets_text: Option<String>,
    /// The window's length in minutes: Codex says it; Claude Code's names do (5 hours, 7 days).
    pub window_minutes: Option<i64>,
    /// When the CLI reported it (Unix ms).
    pub observed_at: i64,
    /// The run or chat turn it came from.
    pub run_id: Option<String>,
}

/// One limit on the Subscription tab, with its newest reading.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Limit {
    pub key: String,
    /// In plain words: "Session limit", "Weekly limit", "Fable limit", "5-hour limit".
    pub name: String,
    /// The window's length in minutes, when known.
    pub window_minutes: Option<i64>,
    /// None: no run or chat turn on this CLI has reported it yet.
    pub reading: Option<Reading>,
}

/// An agent that runs on a coding CLI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitAgent {
    pub agent_id: String,
    pub name: String,
    pub role_key: String,
    /// active or paused
    pub status: String,
    /// The Team Lead (the team's lead, or the agent with Chat on).
    pub is_lead: bool,
}

/// One coding CLI's block on the Subscription tab.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliLimits {
    pub cli_id: String,
    pub name: String,
    /// claude_code, codex, gemini or other
    pub kind: String,
    /// Whether Gizai reads this kind of CLI's limits: Claude Code and Codex; Gemini and Other not yet.
    pub readable: bool,
    /// The account's folder, `~` for the home folder: Claude Code's CLAUDE_CONFIG_DIR (else ~/.claude), Codex's CODEX_HOME
    /// (else ~/.codex), which holds the session logs Gizai reads. None for Gemini and Other.
    pub account_dir: Option<String>,
    /// Its limits: the ones it always has first (Claude Code: session, weekly, Fable; Codex: its two windows), each with
    /// its newest reading or none, then any other it reported. Empty when Gizai can't read them.
    pub limits: Vec<Limit>,
    /// The agents that run on it (their Runs on), the Team Lead first.
    pub agents: Vec<LimitAgent>,
    /// The Team Lead's chat and board checks run on it (the Team Lead's Runs on), for chats without their own Runs on.
    pub lead_chat: bool,
    /// Chats whose own Runs on is this CLI (Chat → Runs on, under the text box).
    pub chats: i64,
}

/// Claude Code's `resetsAt` and Codex's `resets_at` are Unix seconds; a number this large is in ms already.
fn epoch_ms(n: f64) -> Option<i64> {
    if !n.is_finite() || n <= 0.0 {
        return None;
    }
    Some(if n < 100_000_000_000.0 { (n * 1000.0).round() as i64 } else { n.round() as i64 })
}

/// A number that is 0 or more (a fraction used, a percentage, seconds).
fn non_negative(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|u| u.is_finite() && *u >= 0.0)
}

/// A Claude Code window's length in minutes, from its name.
fn claude_window(key: &str) -> Option<i64> {
    match key {
        FIVE_HOUR => Some(300),
        k if k.starts_with("seven_day") => Some(10_080),
        _ => None,
    }
}

/// The readings in one line of Claude Code's stream-json: a `rate_limit_event`'s (`claude_info`); none for any other line.
pub fn claude_line(line: &str, at: i64) -> Vec<Reading> {
    let line = line.trim();
    if !line.contains("rate_limit_event") {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
    if v.get("type").and_then(Value::as_str) != Some("rate_limit_event") {
        return vec![];
    }
    v.get("rate_limit_info").map(|i| claude_info(i, at)).unwrap_or_default()
}

/// A `rate_limit_event`'s `rate_limit_info` → a reading, at `at` (Unix ms), of each window it reports: each one in
/// `unifiedWindows` with both its fraction used and its reset (Claude Code leaves out the others too), the event's `status`
/// on the window it names (`rateLimitType`); and that window from the event itself when `unifiedWindows` doesn't have it
/// (an older Claude Code, or a window it doesn't track there, like the Opus and Sonnet limits). Usage credits (`overage`)
/// aren't a subscription limit: left out.
pub fn claude_info(info: &Value, at: i64) -> Vec<Reading> {
    let Some(info) = info.as_object() else { return vec![] };
    let status = info.get("status").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let named = info.get("rateLimitType").and_then(Value::as_str).filter(|k| !k.is_empty() && *k != "overage");
    let mut out: Vec<Reading> = vec![];
    for (key, w) in info.get("unifiedWindows").and_then(Value::as_object).into_iter().flatten() {
        let (Some(used), Some(resets)) = (non_negative(w.get("utilization")), w.get("resetsAt").and_then(Value::as_f64).and_then(epoch_ms)) else { continue };
        out.push(Reading {
            key: key.clone(), used_percent: Some(used * 100.0), status: status.clone().filter(|_| named == Some(key.as_str())),
            resets_at: Some(resets), window_minutes: claude_window(key), observed_at: at, ..Default::default()
        });
    }
    if let Some(key) = named && !out.iter().any(|r| r.key == key) {
        let used = non_negative(info.get("utilization")).map(|u| u * 100.0);
        let resets = info.get("resetsAt").and_then(Value::as_f64).and_then(epoch_ms);
        if used.is_some() || resets.is_some() || status.as_deref() == Some("rejected") {
            out.push(Reading { key: key.into(), used_percent: used, status, resets_at: resets, window_minutes: claude_window(key), observed_at: at,
                               ..Default::default() });
        }
    }
    out
}

/// A limit Claude Code said the account reached, in a failed answer's text (`chat_stream::usage_limit`: its name in plain
/// words, and when it resets, as written or as a time stamp) → a reading at `at`: reached, how much is used not said.
pub fn hit(limit: &str, resets: Option<&str>, resets_at: Option<i64>, at: i64) -> Reading {
    let l = limit.to_lowercase();
    let key = if l.contains("session") || l.contains("5-hour") {
        FIVE_HOUR
    } else if l.contains("fable") {
        FABLE
    } else if l.contains("opus") {
        OPUS
    } else if l.contains("sonnet") {
        SONNET
    } else if l.contains("week") {
        SEVEN_DAY
    } else {
        USAGE
    };
    Reading {
        key: key.into(), used_percent: None, status: Some("rejected".into()), resets_at,
        resets_text: resets.map(str::trim).filter(|r| !r.is_empty()).map(str::to_string), window_minutes: claude_window(key), observed_at: at,
        run_id: None,
    }
}

/// "2026-10-09T12:34:56.789Z" (or with an offset, like +02:00) → Unix ms.
fn rfc3339_ms(s: &str) -> Option<i64> {
    let (date, time) = s.trim().split_once(['T', 't', ' '])?;
    let mut d = date.splitn(3, '-');
    let (y, mo, day): (i64, i64, i64) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?, d.next()?.parse().ok()?);
    let (time, offset_min) = if let Some(t) = time.strip_suffix(['Z', 'z']) {
        (t, 0)
    } else if let Some(i) = time.rfind(['+', '-']) {
        let (h, m) = time[i + 1..].split_once(':').unwrap_or((&time[i + 1..], "0"));
        let off = h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?;
        (&time[..i], if time.as_bytes()[i] == b'-' { -off } else { off })
    } else {
        (time, 0)
    };
    let mut t = time.splitn(3, ':');
    let (h, mi): (i64, i64) = (t.next()?.parse().ok()?, t.next()?.parse().ok()?);
    let secs: f64 = t.next().unwrap_or("0").parse().ok()?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let doy = (153 * ((mo + 9) % 12) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 - offset_min * 60) as f64 * 1000.0 + secs * 1000.0).round() as i64)
}

/// A Codex session log's text → the readings of its newest `token_count` event with `rate_limits`: its plan's `primary`
/// and `secondary` windows (`used_percent`, `window_minutes`, and `resets_at`, or `resets_in_seconds` in older versions),
/// at the time of that line (`timestamp`), else at `fallback`. A snapshot of Codex's own limit (`limit_id` codex, or none)
/// goes before one of another limit. Empty when the log has none.
pub fn codex_log(text: &str, fallback: i64) -> Vec<Reading> {
    let mut other: Option<Vec<Reading>> = None;
    for line in text.lines().rev() {
        if !line.contains("\"rate_limits\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { continue };
        let payload = v.get("payload").unwrap_or(&v);
        let Some(limits) = payload.get("rate_limits").filter(|l| l.is_object()) else { continue };
        let at = v.get("timestamp").and_then(Value::as_str).and_then(rfc3339_ms).unwrap_or(fallback);
        let found: Vec<Reading> = [PRIMARY, SECONDARY].into_iter().filter_map(|key| {
            let w = limits.get(key).filter(|w| w.is_object())?;
            let used = non_negative(w.get("used_percent"))?;
            let resets = w.get("resets_at").and_then(Value::as_f64).and_then(epoch_ms)
                .or_else(|| non_negative(w.get("resets_in_seconds")).map(|s| at + (s * 1000.0).round() as i64));
            Some(Reading { key: key.into(), used_percent: Some(used), resets_at: resets,
                           window_minutes: w.get("window_minutes").and_then(Value::as_i64).filter(|m| *m > 0), observed_at: at, ..Default::default() })
        }).collect();
        if found.is_empty() {
            continue;
        }
        match limits.get("limit_id").and_then(Value::as_str) {
            None | Some("codex") | Some("") => return found,
            Some(_) => { other.get_or_insert(found); }
        }
    }
    other.unwrap_or_default()
}

/// The session log of Codex thread `thread_id` in the account folder `codex_home`:
/// `sessions/<year>/<month>/<day>/rollout-<time>-<thread>.jsonl`, looked for in the newest 62 date folders. None when there
/// is none, or `thread_id` isn't a thread id.
pub fn codex_log_path(codex_home: &Path, thread_id: &str) -> Option<PathBuf> {
    let id = thread_id.trim();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let end = format!("-{id}.jsonl");
    // The folders in `dir` named by a number, the highest first.
    let numbered = |dir: &Path| -> Vec<PathBuf> {
        let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(dir).into_iter().flatten().flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| Some((e.file_name().to_str()?.parse::<u32>().ok()?, e.path())))
            .collect();
        found.sort_by_key(|a| std::cmp::Reverse(a.0));
        found.into_iter().map(|(_, p)| p).collect()
    };
    let mut days = 0;
    for year in numbered(&codex_home.join("sessions")) {
        for month in numbered(&year) {
            for day in numbered(&month) {
                days += 1;
                if days > CODEX_DAYS {
                    return None;
                }
                let log = std::fs::read_dir(&day).into_iter().flatten().flatten().map(|e| e.path())
                    .find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&end)));
                if log.is_some() {
                    return log;
                }
            }
        }
    }
    None
}

/// The readings in the session log of Codex thread `thread_id` in `codex_home` (`codex_log` of its last 8 MB), at the
/// time each was written, else the file's own time. Only reads; empty without a log or a reading in it.
pub fn codex_limits(codex_home: &Path, thread_id: &str) -> Vec<Reading> {
    let Some(path) = codex_log_path(codex_home, thread_id) else { return vec![] };
    let Ok(mut f) = std::fs::File::open(&path) else { return vec![] };
    let Ok(meta) = f.metadata() else { return vec![] };
    let written = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64)
        .unwrap_or_else(crate::ids::now_ms);
    let from = meta.len().saturating_sub(CODEX_TAIL);
    let mut bytes = vec![];
    if f.seek(SeekFrom::Start(from)).is_err() || f.read_to_end(&mut bytes).is_err() {
        return vec![];
    }
    // A line cut off at the start isn't whole JSON, so `codex_log` passes it by.
    codex_log(&String::from_utf8_lossy(&bytes), written)
}

/// The folder a coding CLI keeps its account in: Claude Code's CLAUDE_CONFIG_DIR (default ~/.claude), Codex's CODEX_HOME
/// (default ~/.codex), from the CLI's own environment lines, else Gizai's (`inherited`). None for Gemini and Other.
pub fn account_dir(cli: &Cli, home: &str, inherited: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let (var, default) = match cli.kind.as_str() {
        "claude_code" => ("CLAUDE_CONFIG_DIR", ".claude"),
        "codex" => ("CODEX_HOME", ".codex"),
        _ => return None,
    };
    let set = clis::env_pairs(cli, home).into_iter().find(|(k, _)| k == var).map(|(_, v)| v)
        .or_else(|| inherited(var)).map(|v| clis::expand_home(v.trim(), home)).filter(|v| !v.is_empty());
    Some(set.map(PathBuf::from).unwrap_or_else(|| Path::new(home).join(default)))
}

/// `path` with the home folder as `~`.
fn tilde(path: &Path, home: &str) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if !home.is_empty() && rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) if !home.is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

type Kept = BTreeMap<String, Vec<Reading>>;

fn load_in(c: &rusqlite::Connection) -> Result<Kept> {
    let raw: Option<String> = c.query_row("SELECT value_json FROM settings WHERE key=?1 AND org_id=''", [KEY], |r| r.get(0)).optional()?;
    Ok(raw.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default())
}

fn load(db: &Db) -> Result<Kept> {
    Ok(settings::get::<Kept>(db, KEY)?.unwrap_or_default())
}

/// The readings kept for coding CLI `cli_id`, in the order their limits were first read.
pub fn kept(db: &Db, cli_id: &str) -> Result<Vec<Reading>> {
    Ok(load(db)?.remove(cli_id).unwrap_or_default())
}

/// Keeps `readings` for coding CLI `cli_id` (empty: the built-in Claude Code): each replaces the reading kept for its limit,
/// unless that one is newer. A limit reached that a run said in words (no percentage) keeps the reset time the window
/// already had, and is left out when the same run's stream already said that limit was reached. Returns whether anything
/// changed.
pub fn record(db: &Db, cli_id: &str, readings: &[Reading]) -> Result<bool> {
    let cli_id = if cli_id.trim().is_empty() { clis::CLAUDE_CODE } else { cli_id.trim() };
    if readings.is_empty() {
        return Ok(false);
    }
    db.write(None, |w| {
        let mut all = load_in(w.conn())?;
        let list = all.entry(cli_id.to_string()).or_default();
        let mut changed = false;
        for r in readings.iter().filter(|r| !r.key.trim().is_empty()) {
            let mut r = r.clone();
            match list.iter_mut().find(|k| k.key == r.key) {
                Some(k) if k.observed_at > r.observed_at => {}
                Some(k) => {
                    if r.used_percent.is_none() && r.status.as_deref() == Some("rejected") {
                        if k.status.as_deref() == Some("rejected") && k.run_id.is_some() && k.run_id == r.run_id {
                            continue;
                        }
                        // A window's reset doesn't move until it comes.
                        r.resets_at = r.resets_at.or(k.resets_at.filter(|t| *t > r.observed_at));
                    }
                    if *k != r {
                        *k = r;
                        changed = true;
                    }
                }
                None => {
                    list.push(r);
                    changed = true;
                }
            }
        }
        if changed {
            settings::set_in(w, KEY, &all)?;
        }
        Ok(changed)
    })
}

/// `record` for the readings of run `run_id` (a task run, chat turn or board check): they count for the coding CLI the run
/// recorded (`runs.adapter`), and only for it.
pub fn record_for_run(db: &Db, run_id: &str, readings: &[Reading]) -> Result<bool> {
    if readings.is_empty() {
        return Ok(false);
    }
    let cli: String = db.read(|c| {
        c.query_row("SELECT adapter FROM runs WHERE id=?1", [run_id], |r| r.get(0)).optional()?
            .ok_or_else(|| Error::NotFound(format!("run {run_id}")))
    })?;
    let readings: Vec<Reading> = readings.iter().cloned().map(|r| Reading { run_id: Some(run_id.to_string()), ..r }).collect();
    record(db, &cli, &readings)
}

/// A limit's name in plain words: Claude Code's own names (session, weekly, Fable, Opus, Sonnet); a Codex window by its
/// length ("5-hour limit", "Weekly limit").
pub fn limit_name(key: &str, window_minutes: Option<i64>) -> String {
    match key {
        FIVE_HOUR => "Session limit".into(),
        SEVEN_DAY => "Weekly limit".into(),
        FABLE => "Fable limit".into(),
        OPUS => "Opus limit".into(),
        SONNET => "Sonnet limit".into(),
        USAGE => "Usage limit".into(),
        PRIMARY | SECONDARY => match window_minutes.filter(|m| *m > 0) {
            Some(10_080) => "Weekly limit".into(),
            Some(m) if m % 1440 == 0 => format!("{}-day limit", m / 1440),
            Some(m) if m % 60 == 0 => format!("{}-hour limit", m / 60),
            Some(m) => format!("{m}-minute limit"),
            None if key == PRIMARY => "Primary limit".into(),
            None => "Secondary limit".into(),
        },
        other => {
            let words = other.replace('_', " ");
            let mut cs = words.trim().chars();
            match cs.next() {
                Some(c) => format!("{}{} limit", c.to_uppercase(), cs.as_str()),
                None => "Limit".into(),
            }
        }
    }
}

/// Where a Claude Code limit goes after the ones it always shows.
fn claude_rank(key: &str) -> usize {
    [FIVE_HOUR, SEVEN_DAY, FABLE, OPUS, SONNET, USAGE].iter().position(|k| *k == key).unwrap_or(99)
}

/// A CLI's limits: the ones it always has, each with its reading or none, then the others it reported (only its own
/// kind's: a CLI's kind can change in Settings).
fn limits_of(kind: &str, kept: &[Reading]) -> Vec<Limit> {
    let (always, own): (&[&str], fn(&str) -> bool) = match kind {
        "claude_code" => (&CLAUDE_LIMITS, |k| k != PRIMARY && k != SECONDARY),
        "codex" => (&CODEX_LIMITS, |k| k == PRIMARY || k == SECONDARY),
        _ => return vec![],
    };
    let limit = |key: &str, reading: Option<&Reading>| {
        let usual = CODEX_USUAL_MINUTES.iter().find(|(k, _)| *k == key && kind == "codex").map(|(_, m)| *m);
        let window = reading.and_then(|r| r.window_minutes).or(if kind == "codex" { usual } else { claude_window(key) });
        Limit { key: key.into(), name: limit_name(key, window), window_minutes: window, reading: reading.cloned() }
    };
    let mut out: Vec<Limit> = always.iter().map(|k| limit(k, kept.iter().find(|r| r.key == *k))).collect();
    let mut more: Vec<&Reading> = kept.iter().filter(|r| own(&r.key) && !always.contains(&r.key.as_str())).collect();
    more.sort_by(|a, b| claude_rank(&a.key).cmp(&claude_rank(&b.key)).then_with(|| a.key.cmp(&b.key)));
    out.extend(more.into_iter().map(|r| limit(&r.key, Some(r))));
    out
}

/// The Subscription tab: a block per coding CLI in Settings, Claude Code first, with its limits and what runs on it.
/// `home` and `inherited` (Gizai's own environment) say where each account's folder is.
pub fn subscription(db: &Db, home: &str, inherited: &dyn Fn(&str) -> Option<String>) -> Result<Vec<CliLimits>> {
    let all = load(db)?;
    let on = |adapter: Option<&str>| adapter.map(str::trim).filter(|a| !a.is_empty()).unwrap_or(clis::CLAUDE_CODE).to_string();
    let lead = crate::team::chat_agent(db)?;
    let lead_cli = lead.as_ref().map(|l| on(l.adapter.as_deref()));
    let mut agents: Vec<(String, LimitAgent)> = vec![];
    for (_, m) in crate::team::all_agents(db)? {
        if m.status != "archived" && !agents.iter().any(|(_, a)| a.agent_id == m.actor_id) {
            let is_lead = m.is_lead || lead.as_ref().is_some_and(|l| l.actor_id == m.actor_id);
            agents.push((on(m.adapter.as_deref()),
                         LimitAgent { agent_id: m.actor_id, name: m.name, role_key: m.role_key, status: m.status, is_lead }));
        }
    }
    let chats: BTreeMap<String, i64> = db.read(|c| {
        let mut st = c.prepare("SELECT cli, count(*) FROM chat_threads WHERE deleted_at IS NULL AND cli IS NOT NULL AND cli <> '' GROUP BY cli")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    })?;
    Ok(clis::list(db)?.into_iter().map(|cli| {
        let readable = matches!(cli.kind.as_str(), "claude_code" | "codex");
        let kept = all.get(&cli.id).map(Vec::as_slice).unwrap_or_default();
        let mut on_it: Vec<LimitAgent> = agents.iter().filter(|(c, _)| *c == cli.id).map(|(_, a)| a.clone()).collect();
        on_it.sort_by_key(|a| !a.is_lead);
        CliLimits {
            account_dir: account_dir(&cli, home, inherited).map(|d| tilde(&d, home)),
            limits: if readable { limits_of(&cli.kind, kept) } else { vec![] },
            agents: on_it,
            lead_chat: cli.kind == "claude_code" && lead_cli.as_deref() == Some(cli.id.as_str()),
            chats: if cli.kind == "claude_code" { chats.get(&cli.id).copied().unwrap_or(0) } else { 0 },
            readable, cli_id: cli.id, name: cli.name, kind: cli.kind,
        }
    }).collect())
}
