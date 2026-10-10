// GA-62: the Usage page's Subscription tab in the record (limits.rs), from fixtures: Claude Code's rate_limit_event lines
// (shaped like Claude Code 2.1.x's own schema) and a Codex session log (Codex 0.154's token_count events), kept per coding
// CLI entry, so two Claude Code accounts and a Codex account keep their own numbers.
use std::path::Path;

use gizai_core::clis::{self, Cli};
use gizai_core::limits::{self, CliLimits, Reading};
use gizai_core::model::*;
use gizai_core::{board, chat, db::Db, ids, projects, runs, seed, settings, tasks, team};

const H: i64 = 3_600_000;
/// 2026-10-09 00:00 UTC.
const OCT_9: i64 = 1_791_504_000_000;
/// 2026-10-09 15:00 UTC: "now" in these tests.
const NOW: i64 = OCT_9 + 15 * H;
const HOME: &str = "/home/jef";

const CLAUDE_1: &str = include_str!("fixtures/limits-claude-1.jsonl");
const CLAUDE_2: &str = include_str!("fixtures/limits-claude-2.jsonl");
const CODEX_LOG: &str = include_str!("fixtures/limits-codex-rollout.jsonl");
/// The Codex thread the fixture's session log belongs to (its session_meta id).
const THREAD: &str = "0199c3a1-7a2b-7c3d-8e4f-123456789abc";

/// The readings in a Claude Code stream, each line read at `at`.
fn stream(text: &str, at: i64) -> Vec<Reading> {
    text.lines().flat_map(|l| limits::claude_line(l, at)).collect()
}

struct World {
    db: Db,
    s: seed::SeedIds,
    project: String,
    cc2: String,
    codex: String,
    gemini: String,
    other: String,
    backend: String,
    frontend: String,
    lead: String,
    codex_agent: String,
    codex_home: tempfile::TempDir,
}

/// Settings → Coding CLIs with Claude Code (built in), Claude Code 2 (its own CLAUDE_CONFIG_DIR), Codex (its own CODEX_HOME),
/// Gemini and an Other CLI; the Backend Agent on Claude Code, the Frontend Agent and the Team Lead on Claude Code 2, the
/// Codex Agent on Codex.
fn world() -> World {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let codex_home = tempfile::tempdir().unwrap();
    let all = clis::save(&db, vec![
        Cli { name: "Claude Code 2".into(), kind: "claude_code".into(), command: "claude".into(), env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..Default::default() },
        Cli { name: "Codex".into(), kind: "codex".into(), command: "codex".into(), env: vec![format!("CODEX_HOME={}", codex_home.path().display())], ..Default::default() },
        Cli { name: "Gemini".into(), kind: "gemini".into(), command: "gemini".into(), ..Default::default() },
        Cli { name: "Aider".into(), kind: "other".into(), command: "aider".into(), args: "--message {prompt}".into(), ..Default::default() },
    ]).unwrap();
    let id = |n: &str| all.iter().find(|c| c.name == n).unwrap().id.clone();
    let (cc2, codex, gemini, other) = (id("Claude Code 2"), id("Codex"), id("Gemini"), id("Aider"));
    let agent = |name: &str, role: &str, adapter: &str, chat: Option<bool>| {
        team::add_agent(&db, &s.you_id, &s.team_id,
                        AgentInput { name: name.into(), role_key: role.into(), adapter: adapter.into(), chat_enabled: chat, ..Default::default() }).unwrap()
    };
    let backend = agent("Backend Agent", "backend", "", None);
    let frontend = agent("Frontend Agent", "frontend", &cc2, None);
    let lead = agent("Team Lead", "lead", &cc2, Some(true));
    let codex_agent = agent("Codex Agent", "backend", &codex, None);
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    World { db, s, project, cc2, codex, gemini, other, backend, frontend, lead, codex_agent, codex_home }
}

impl World {
    /// A finished task run of `agent` on a card of its own, with this session (a Codex run's thread id).
    fn run(&self, agent: &str, session: &str) -> String {
        let task = tasks::create(&self.db, &self.s.you_id, TaskInput { project_id: self.project.clone(), title: format!("Card {}", ids::new_id()), ..Default::default() }).unwrap();
        let r = runs::create(&self.db, agent, &task, "backend", session, "/tmp", "/tmp", "b", "/tmp/l").unwrap();
        runs::finish(&self.db, &r, "succeeded", None, 1_000, 10, 2, None).unwrap();
        r
    }

    fn blocks(&self) -> Vec<CliLimits> {
        limits::subscription(&self.db, HOME, &|_| None).unwrap()
    }

    fn block(&self, cli_id: &str) -> CliLimits {
        self.blocks().into_iter().find(|b| b.cli_id == cli_id).unwrap_or_else(|| panic!("no block for {cli_id}"))
    }

    /// Writes the fixture's Codex session log where Codex 0.154 keeps it: sessions/<year>/<month>/<day>/rollout-<time>-<thread>.jsonl.
    fn codex_log(&self, day: &str, thread: &str, text: &str) {
        let dir = self.codex_home.path().join("sessions").join(day);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("rollout-2026-10-09T13-58-00-{thread}.jsonl")), text).unwrap();
    }
}

/// (key, used %, reset in Unix seconds) of each limit of a block that has a reading.
fn numbers(b: &CliLimits) -> Vec<(String, Option<f64>, Option<i64>)> {
    b.limits.iter().filter_map(|l| l.reading.as_ref().map(|r| (l.key.clone(), r.used_percent.map(|u| (u * 10.0).round() / 10.0), r.resets_at.map(|t| t / 1000))))
        .collect()
}

fn names(b: &CliLimits) -> Vec<&str> {
    b.limits.iter().map(|l| l.name.as_str()).collect()
}

// Reading Claude Code's stream

#[test]
fn a_rate_limit_event_gives_the_session_weekly_and_fable_windows_with_use_reset_and_time() {
    let rs = stream(CLAUDE_1, NOW);
    assert_eq!(rs.len(), 3, "one event, three windows; the assistant's text quoting rate_limit_event is not a reading: {rs:?}");
    let by = |k: &str| rs.iter().find(|r| r.key == k).unwrap_or_else(|| panic!("no {k}"));
    let session = by(limits::FIVE_HOUR);
    assert_eq!((session.used_percent, session.resets_at, session.window_minutes, session.observed_at), (Some(42.0), Some(1_791_565_200_000), Some(300), NOW));
    assert_eq!(session.status.as_deref(), Some("allowed"), "the event's status belongs to the window it names (rateLimitType)");
    let weekly = by(limits::SEVEN_DAY);
    assert_eq!((weekly.used_percent, weekly.resets_at, weekly.window_minutes), (Some(18.0), Some(1_791_882_000_000), Some(10_080)));
    assert_eq!(weekly.status, None, "only the named window gets the status");
    let fable = by(limits::FABLE);
    assert_eq!((fable.used_percent, fable.resets_at, fable.window_minutes), (Some(5.0), Some(1_791_968_400_000), Some(10_080)));
    assert!(rs.iter().all(|r| r.run_id.is_none() && r.resets_text.is_none()));
}

#[test]
fn an_account_without_a_fable_window_reports_none_and_a_warning_marks_its_window() {
    let rs = stream(CLAUDE_2, NOW);
    let keys: Vec<&str> = rs.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&limits::FIVE_HOUR) && keys.contains(&limits::SEVEN_DAY), "{keys:?}");
    let session = rs.iter().find(|r| r.key == limits::FIVE_HOUR).unwrap();
    assert_eq!((session.used_percent, session.status.as_deref()), (Some(91.0), Some("allowed_warning")));
}

#[test]
fn lines_without_a_rate_limit_info_or_with_broken_json_give_nothing() {
    for line in [
        r#"{"type":"rate_limit_event","info":{}}"#,
        r#"{"type":"rate_limit_event"}"#,
        r#"{"type":"rate_limit_event","rate_limit_info":"five_hour"}"#,
        r#"{"type":"rate_limit_event","rate_limit_info":{}}"#,
        r#"{"type":"rate_limit_event","rate_limit_info":{"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"rate_limit_event"}]}}"#,
        r#"{"type":"user","rate_limit_info":{"rateLimitType":"five_hour","utilization":0.5,"resetsAt":1791565200}}"#,
        "",
        "not json rate_limit_event",
    ] {
        assert_eq!(limits::claude_line(line, NOW), vec![], "{line}");
    }
}

#[test]
fn an_event_without_windows_reads_the_limit_it_names_and_leaves_usage_credits_out() {
    // An older Claude Code, or a limit unifiedWindows doesn't track (the Opus weekly limit), rejected.
    let opus = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"seven_day_opus","resetsAt":1791882000}}"#, NOW);
    assert_eq!(opus.len(), 1);
    assert_eq!((opus[0].key.as_str(), opus[0].used_percent, opus[0].status.as_deref(), opus[0].resets_at), (limits::OPUS, None, Some("rejected"), Some(1_791_882_000_000)));
    let session = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"five_hour","utilization":0.25}}"#, NOW);
    assert_eq!((session[0].key.as_str(), session[0].used_percent, session[0].resets_at), (limits::FIVE_HOUR, Some(25.0), None));
    // Usage credits aren't a subscription limit; a named window without a number or a rejection isn't a reading.
    assert_eq!(limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","rateLimitType":"overage","utilization":0.9,"resetsAt":1791882000}}"#, NOW), vec![]);
    assert_eq!(limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","rateLimitType":"seven_day"}}"#, NOW), vec![]);
}

#[test]
fn a_window_past_its_cap_keeps_its_number_and_a_window_without_a_reset_or_use_is_left_out() {
    let info = serde_json::json!({ "status": "allowed", "unifiedWindows": {
        "five_hour": { "utilization": 1.2, "resetsAt": 1791565200 },
        "seven_day": { "utilization": 0.3 },
        "seven_day_overage_included": { "resetsAt": 1791968400 },
    }});
    let rs = limits::claude_info(&info, NOW);
    assert_eq!(rs.len(), 1, "{rs:?}");
    assert_eq!((rs[0].key.as_str(), rs[0].used_percent.map(|u| u.round())), (limits::FIVE_HOUR, Some(120.0)));
    // A time stamp that is in ms already stays so; a negative use isn't a number.
    let ms = limits::claude_info(&serde_json::json!({ "unifiedWindows": { "five_hour": { "utilization": 0.1, "resetsAt": 1_791_565_200_000i64 } } }), NOW);
    assert_eq!(ms[0].resets_at, Some(1_791_565_200_000));
    assert_eq!(limits::claude_info(&serde_json::json!({ "unifiedWindows": { "five_hour": { "utilization": -0.1, "resetsAt": 1791565200 } } }), NOW), vec![]);
    assert_eq!(limits::claude_info(&serde_json::json!("five_hour"), NOW), vec![]);
}

#[test]
fn a_limit_hit_in_words_is_a_reached_reading_of_the_limit_it_names() {
    let cases = [
        ("session limit", limits::FIVE_HOUR), ("5-hour limit", limits::FIVE_HOUR), ("weekly limit", limits::SEVEN_DAY),
        ("Fable limit", limits::FABLE), ("Opus limit", limits::OPUS), ("Sonnet limit", limits::SONNET), ("usage limit", limits::USAGE),
    ];
    for (name, key) in cases {
        let r = limits::hit(name, Some(" 3pm (Europe/Amsterdam) "), None, NOW);
        assert_eq!((r.key.as_str(), r.used_percent, r.status.as_deref()), (key, None, Some("rejected")), "{name}");
        assert_eq!((r.resets_text.as_deref(), r.observed_at), (Some("3pm (Europe/Amsterdam)"), NOW));
    }
    let old = limits::hit("usage limit", Some("   "), Some(1_751_230_800_000), NOW);
    assert_eq!((old.resets_text, old.resets_at), (None, Some(1_751_230_800_000)));
}

// Reading Codex's session log

#[test]
fn a_codex_session_log_gives_its_plans_two_windows_from_the_newest_snapshot_of_codex_itself() {
    let rs = limits::codex_log(CODEX_LOG, 1);
    assert_eq!(rs.len(), 2, "{rs:?}");
    let at = OCT_9 + 14 * H + 90_250; // 14:01:30.250Z, the line's own time
    let primary = &rs[0];
    assert_eq!((primary.key.as_str(), primary.used_percent, primary.window_minutes, primary.resets_at, primary.observed_at),
               (limits::PRIMARY, Some(23.5), Some(300), Some(1_791_565_200_000), at),
               "the newest token_count of Codex's own limit, not the older 20% nor the other model's 77%");
    let secondary = &rs[1];
    assert_eq!((secondary.key.as_str(), secondary.used_percent, secondary.window_minutes, secondary.resets_at),
               (limits::SECONDARY, Some(41.0), Some(10_080), Some(1_791_882_000_000)));
}

#[test]
fn an_older_codex_log_with_seconds_to_reset_and_no_time_uses_the_fallback() {
    let log = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":12.0,"window_minutes":300,"resets_in_seconds":3600}}}}"#;
    let rs = limits::codex_log(log, NOW);
    assert_eq!(rs.len(), 1);
    assert_eq!((rs[0].used_percent, rs[0].observed_at, rs[0].resets_at), (Some(12.0), NOW, Some(NOW + H)));
    // An offset time stamp is read as the moment it names.
    let offset = r#"{"timestamp":"2026-10-09T16:01:30+02:00","type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":1.0}}}}"#;
    assert_eq!(limits::codex_log(offset, 1)[0].observed_at, OCT_9 + 14 * H + 90_000);
    // Only another limit's snapshot: it is better than nothing.
    let other = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"limit_id":"codex_other","primary":{"used_percent":7.0}}}}"#;
    assert_eq!(limits::codex_log(other, NOW)[0].used_percent, Some(7.0));
    // Nothing to read: no snapshot, an empty one, a cut-off line.
    for text in ["", r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":null}}"#,
                 r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":null,"secondary":null}}}"#,
                 r#"limits":{"primary":{"used_percent":50.0}}}}"#] {
        assert_eq!(limits::codex_log(text, NOW), vec![], "{text}");
    }
}

#[test]
fn the_log_of_a_codex_thread_is_found_in_the_newest_date_folders_and_a_bad_thread_id_finds_none() {
    let w = world();
    w.codex_log("2026/09/30", "aaaa-older", "{}");
    w.codex_log("2026/10/09", THREAD, CODEX_LOG);
    std::fs::create_dir_all(w.codex_home.path().join("sessions/2026/10/not-a-day")).unwrap();
    let found = limits::codex_log_path(w.codex_home.path(), THREAD).expect("the thread's log");
    assert!(found.ends_with(format!("sessions/2026/10/09/rollout-2026-10-09T13-58-00-{THREAD}.jsonl")), "{found:?}");
    assert!(limits::codex_log_path(w.codex_home.path(), "aaaa-older").is_some(), "an older date folder too");
    for bad in ["", "  ", "../../auth", "x/y", "nope"] {
        assert_eq!(limits::codex_log_path(w.codex_home.path(), bad), None, "{bad}");
    }
    assert_eq!(limits::codex_log_path(Path::new("/nonexistent/codex-home"), THREAD), None);
    assert_eq!(limits::codex_limits(w.codex_home.path(), THREAD).len(), 2);
    assert_eq!(limits::codex_limits(w.codex_home.path(), "nope"), vec![]);
}

// Kept per coding CLI entry

#[test]
fn every_run_and_chat_turn_records_the_coding_cli_entry_it_used_not_its_kind() {
    let w = world();
    let adapter = |r: &str| runs::get(&w.db, r).unwrap().adapter;
    assert_eq!(adapter(&w.run(&w.backend, "s1")).as_deref(), Some(clis::CLAUDE_CODE));
    assert_eq!(adapter(&w.run(&w.frontend, "s2")).as_deref(), Some(w.cc2.as_str()), "Claude Code 2's own id, not claude_code");
    assert_eq!(adapter(&w.run(&w.codex_agent, THREAD)).as_deref(), Some(w.codex.as_str()));
    let th = chat::create_thread(&w.db, &w.s.you_id, &w.lead, "Hi").unwrap();
    let turn = runs::create_chat(&w.db, &w.lead, &th, &ids::new_id(), "/tmp/lead", "/tmp/r.jsonl").unwrap();
    assert_eq!(adapter(&turn).as_deref(), Some(w.cc2.as_str()), "a chat turn without its own Runs on: the Team Lead's");
    let on_cc = runs::create_chat_on(&w.db, &w.lead, &th, Some(clis::CLAUDE_CODE), &ids::new_id(), "/tmp/lead", "/tmp/r2.jsonl").unwrap();
    assert_eq!(adapter(&on_cc).as_deref(), Some(clis::CLAUDE_CODE), "a chat's own Runs on");
    let check = board::create_run(&w.db, &w.lead, &ids::new_id(), "/tmp/lead", "/tmp/c.jsonl", &[]).unwrap();
    assert_eq!(adapter(&check).as_deref(), Some(w.cc2.as_str()), "a board check runs on the Team Lead's CLI");
}

#[test]
fn two_claude_code_accounts_and_a_codex_account_keep_their_own_limits() {
    let w = world();
    let r1 = w.run(&w.backend, "s1");
    let r2 = w.run(&w.frontend, "s2");
    let r3 = w.run(&w.codex_agent, THREAD);
    w.codex_log("2026/10/09", THREAD, CODEX_LOG);
    assert!(limits::record_for_run(&w.db, &r1, &stream(CLAUDE_1, NOW)).unwrap());
    assert!(limits::record_for_run(&w.db, &r2, &stream(CLAUDE_2, NOW)).unwrap());
    assert!(limits::record_for_run(&w.db, &r3, &limits::codex_limits(w.codex_home.path(), THREAD)).unwrap());

    let blocks = w.blocks();
    let order: Vec<(&str, &str, bool)> = blocks.iter().map(|b| (b.name.as_str(), b.kind.as_str(), b.readable)).collect();
    assert_eq!(order, vec![("Claude Code", "claude_code", true), ("Claude Code 2", "claude_code", true), ("Codex", "codex", true),
                           ("Gemini", "gemini", false), ("Aider", "other", false)], "a block per entry, in Settings' order");

    let cc = w.block(clis::CLAUDE_CODE);
    assert_eq!(names(&cc), ["Session limit", "Weekly limit", "Fable limit"]);
    assert_eq!(numbers(&cc), vec![(limits::FIVE_HOUR.into(), Some(42.0), Some(1_791_565_200)), (limits::SEVEN_DAY.into(), Some(18.0), Some(1_791_882_000)),
                                  (limits::FABLE.into(), Some(5.0), Some(1_791_968_400))]);
    assert!(cc.limits.iter().all(|l| l.reading.as_ref().is_some_and(|r| r.run_id.as_deref() == Some(r1.as_str()) && r.observed_at == NOW)),
            "each number keeps its run and when it was read");
    // shown as your system writes it: ~\.claude on Windows
    assert_eq!(cc.account_dir.as_deref(), Some(format!("~{}.claude", std::path::MAIN_SEPARATOR).as_str()));

    let cc2 = w.block(&w.cc2);
    assert_eq!(names(&cc2), ["Session limit", "Weekly limit", "Fable limit"], "the three limits always show");
    assert_eq!(numbers(&cc2), vec![(limits::FIVE_HOUR.into(), Some(91.0), Some(1_791_561_600)), (limits::SEVEN_DAY.into(), Some(35.0), Some(1_792_054_800))]);
    assert_eq!(cc2.limits[2].reading, None, "this account reported no Fable window: none is made up");
    assert_eq!(cc2.account_dir.as_deref(), Some(format!("~{}.claude-2", std::path::MAIN_SEPARATOR).as_str()));

    let codex = w.block(&w.codex);
    assert_eq!(names(&codex), ["5-hour limit", "Weekly limit"]);
    assert_eq!(numbers(&codex), vec![(limits::PRIMARY.into(), Some(23.5), Some(1_791_565_200)), (limits::SECONDARY.into(), Some(41.0), Some(1_791_882_000))]);
    assert_eq!(codex.account_dir.as_deref(), Some(w.codex_home.path().to_str().unwrap()));

    for id in [&w.gemini, &w.other] {
        let b = w.block(id);
        assert!(!b.readable && b.limits.is_empty() && b.account_dir.is_none(), "{}: Gizai can't read its limits yet", b.name);
    }
    // In the record: per entry, nothing of one account on another.
    assert_eq!(limits::kept(&w.db, clis::CLAUDE_CODE).unwrap().len(), 3);
    assert_eq!(limits::kept(&w.db, &w.cc2).unwrap().len(), 2);
    assert_eq!(limits::kept(&w.db, &w.codex).unwrap().len(), 2);
    assert_eq!(limits::kept(&w.db, &w.gemini).unwrap(), vec![]);
}

#[test]
fn a_reading_from_a_chat_turn_counts_for_the_cli_that_turn_ran_on_only() {
    let w = world();
    let th = chat::create_thread(&w.db, &w.s.you_id, &w.lead, "Hi").unwrap();
    let on_cc = runs::create_chat_on(&w.db, &w.lead, &th, Some(clis::CLAUDE_CODE), &ids::new_id(), "/tmp/lead", "/tmp/r.jsonl").unwrap();
    limits::record_for_run(&w.db, &on_cc, &stream(CLAUDE_1, NOW)).unwrap();
    assert_eq!(numbers(&w.block(clis::CLAUDE_CODE)).len(), 3, "the turn ran on Claude Code, though the Team Lead is on Claude Code 2");
    assert_eq!(numbers(&w.block(&w.cc2)), vec![]);
    let turn = runs::create_chat(&w.db, &w.lead, &th, &ids::new_id(), "/tmp/lead", "/tmp/r2.jsonl").unwrap();
    limits::record_for_run(&w.db, &turn, &stream(CLAUDE_2, NOW + 1)).unwrap();
    assert_eq!(numbers(&w.block(&w.cc2)).len(), 2);
    assert_eq!(numbers(&w.block(clis::CLAUDE_CODE))[0].1, Some(42.0), "Claude Code's numbers are untouched");
    // A run Gizai doesn't know: nothing is kept anywhere.
    assert!(limits::record_for_run(&w.db, "no-such-run", &stream(CLAUDE_1, NOW + 2)).is_err());
    assert!(!limits::record_for_run(&w.db, "no-such-run", &[]).unwrap(), "no readings: nothing to look up");
}

#[test]
fn an_older_reading_never_replaces_a_newer_one_and_the_same_one_again_changes_nothing() {
    let w = world();
    let r1 = w.run(&w.backend, "s1");
    let r2 = w.run(&w.backend, "s2");
    assert!(limits::record_for_run(&w.db, &r2, &stream(CLAUDE_1, NOW)).unwrap());
    let older = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.1,"resetsAt":1791565200}}}}"#, NOW - H);
    assert!(!limits::record_for_run(&w.db, &r1, &older).unwrap(), "a run that ended later reported an older number");
    assert_eq!(numbers(&w.block(clis::CLAUDE_CODE))[0].1, Some(42.0));
    assert!(!limits::record_for_run(&w.db, &r2, &stream(CLAUDE_1, NOW)).unwrap(), "the same reading again");
    let newer = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","unifiedWindows":{"five_hour":{"utilization":0.5,"resetsAt":1791565200}}}}"#, NOW + H);
    assert!(limits::record_for_run(&w.db, &r1, &newer).unwrap());
    let cc = w.block(clis::CLAUDE_CODE);
    assert_eq!(numbers(&cc)[0].1, Some(50.0));
    assert_eq!(cc.limits[0].reading.as_ref().unwrap().observed_at, NOW + H);
    assert_eq!(numbers(&cc)[1].1, Some(18.0), "the weekly number it didn't report stays as it was");
}

#[test]
fn a_limit_hit_keeps_the_windows_reset_and_isnt_counted_twice_for_one_run() {
    let w = world();
    let r1 = w.run(&w.frontend, "s1");
    limits::record_for_run(&w.db, &r1, &stream(CLAUDE_2, NOW)).unwrap();
    let r2 = w.run(&w.frontend, "s2");
    assert!(limits::record_for_run(&w.db, &r2, &[limits::hit("session limit", Some("4pm"), None, NOW + 60_000)]).unwrap());
    let session = w.block(&w.cc2).limits[0].reading.clone().unwrap();
    assert_eq!((session.used_percent, session.status.as_deref(), session.resets_text.as_deref()), (None, Some("rejected"), Some("4pm")));
    assert_eq!(session.resets_at, Some(1_791_561_600_000), "the window's reset as the stream last said it");
    assert_eq!(w.block(clis::CLAUDE_CODE).limits[0].reading, None, "the other account didn't hit anything");

    // A run whose stream said the limit was reached, then said so in words too: the stream's reading stays.
    let r3 = w.run(&w.frontend, "s3");
    let rejected = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"seven_day","utilization":1.0,"resetsAt":1792054800,"unifiedWindows":{"seven_day":{"utilization":1.0,"resetsAt":1792054800}}}}"#, NOW + 2 * 60_000);
    assert!(limits::record_for_run(&w.db, &r3, &rejected).unwrap());
    assert!(!limits::record_for_run(&w.db, &r3, &[limits::hit("weekly limit", Some("Oct 15, 9am"), None, NOW + 3 * 60_000)]).unwrap());
    let weekly = w.block(&w.cc2).limits[1].reading.clone().unwrap();
    assert_eq!((weekly.used_percent, weekly.status.as_deref()), (Some(100.0), Some("rejected")));

    // A reset that has come since isn't carried over to a new hit.
    let r4 = w.run(&w.backend, "s4");
    let past = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.7,"resetsAt":1791550800}}}}"#, NOW - 3 * H);
    limits::record_for_run(&w.db, &r4, &past).unwrap();
    limits::record_for_run(&w.db, &r4, &[limits::hit("session limit", None, None, NOW)]).unwrap();
    assert_eq!(w.block(clis::CLAUDE_CODE).limits[0].reading.as_ref().unwrap().resets_at, None);
}

#[test]
fn a_reading_for_the_empty_cli_id_is_the_built_in_claude_codes_and_a_blank_key_is_ignored() {
    let w = world();
    let mut rs = stream(CLAUDE_1, NOW);
    rs.push(Reading { key: "  ".into(), used_percent: Some(99.0), observed_at: NOW, ..Default::default() });
    assert!(limits::record(&w.db, " ", &rs).unwrap());
    assert_eq!(limits::kept(&w.db, clis::CLAUDE_CODE).unwrap().len(), 3);
    assert!(!limits::record(&w.db, &w.cc2, &[]).unwrap());
}

#[test]
fn other_limits_a_cli_reported_follow_its_own_and_a_kind_change_hides_the_other_kinds_numbers() {
    let w = world();
    let r = w.run(&w.backend, "s1");
    let opus = limits::claude_line(r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"seven_day_opus","resetsAt":1791882000}}"#, NOW);
    limits::record_for_run(&w.db, &r, &opus).unwrap();
    limits::record_for_run(&w.db, &r, &[limits::hit("usage limit", None, None, NOW)]).unwrap();
    limits::record(&w.db, clis::CLAUDE_CODE, &[Reading { key: limits::PRIMARY.into(), used_percent: Some(5.0), observed_at: NOW, ..Default::default() }]).unwrap();
    assert_eq!(names(&w.block(clis::CLAUDE_CODE)), ["Session limit", "Weekly limit", "Fable limit", "Opus limit", "Usage limit"],
               "a Codex window kept for a Claude Code entry doesn't show there");
}

#[test]
fn a_codex_block_without_readings_names_its_usual_windows_and_a_read_window_its_own_length() {
    let w = world();
    let codex = w.block(&w.codex);
    assert_eq!(names(&codex), ["5-hour limit", "Weekly limit"]);
    assert!(codex.limits.iter().all(|l| l.reading.is_none()));
    let r = w.run(&w.codex_agent, THREAD);
    limits::record_for_run(&w.db, &r, &[Reading { key: limits::PRIMARY.into(), used_percent: Some(3.0), window_minutes: Some(1440), observed_at: NOW, ..Default::default() }]).unwrap();
    assert_eq!(names(&w.block(&w.codex)), ["1-day limit", "Weekly limit"]);
    for (key, minutes, name) in [(limits::PRIMARY, Some(300), "5-hour limit"), (limits::SECONDARY, Some(10_080), "Weekly limit"), (limits::PRIMARY, Some(90), "90-minute limit"),
                                 (limits::SECONDARY, Some(4320), "3-day limit"), (limits::PRIMARY, None, "Primary limit"), (limits::SECONDARY, None, "Secondary limit"),
                                 (limits::FIVE_HOUR, None, "Session limit"), (limits::FABLE, None, "Fable limit"), ("seven_day_haiku", None, "Seven day haiku limit")] {
        assert_eq!(limits::limit_name(key, minutes), name);
    }
}

#[test]
fn each_block_lists_the_agents_on_it_the_team_lead_first_and_which_chats_run_there() {
    let w = world();
    team::set_agent_status(&w.db, &w.s.you_id, &w.frontend, "paused").unwrap();
    let mine = chat::create_thread(&w.db, &w.s.you_id, &w.lead, "Hi").unwrap();
    chat::set_cli(&w.db, &w.s.you_id, &mine, Some(clis::CLAUDE_CODE)).unwrap();
    chat::create_thread(&w.db, &w.s.you_id, &w.lead, "Follows the Team Lead").unwrap();
    let agents = |b: &CliLimits| b.agents.iter().map(|a| (a.name.clone(), a.status.clone(), a.is_lead)).collect::<Vec<_>>();
    let cc = w.block(clis::CLAUDE_CODE);
    assert_eq!(agents(&cc), vec![("Backend Agent".into(), "active".into(), false)]);
    assert_eq!((cc.lead_chat, cc.chats), (false, 1), "one chat picked Claude Code under Runs on");
    let cc2 = w.block(&w.cc2);
    assert_eq!(agents(&cc2), vec![("Team Lead".into(), "active".into(), true), ("Frontend Agent".into(), "paused".into(), false)]);
    assert_eq!((cc2.lead_chat, cc2.chats), (true, 0));
    let codex = w.block(&w.codex);
    assert_eq!(agents(&codex), vec![("Codex Agent".into(), "active".into(), false)]);
    assert_eq!((codex.lead_chat, codex.chats), (false, 0));
    assert!(w.block(&w.gemini).agents.is_empty());
}

#[test]
fn each_account_has_its_own_folder_from_its_environment_lines_else_gizais_else_the_default() {
    let w = world();
    let cli = |id: &str| clis::get(&w.db, id).unwrap();
    let none = |_: &str| None;
    assert_eq!(limits::account_dir(&cli(clis::CLAUDE_CODE), HOME, &none), Some("/home/jef/.claude".into()));
    assert_eq!(limits::account_dir(&cli(&w.cc2), HOME, &none), Some("/home/jef/.claude-2".into()));
    assert_eq!(limits::account_dir(&cli(&w.codex), HOME, &none), Some(w.codex_home.path().to_path_buf()));
    assert_eq!(limits::account_dir(&cli(&w.gemini), HOME, &none), None);
    assert_eq!(limits::account_dir(&cli(&w.other), HOME, &none), None);
    let gizais = |name: &str| (name == "CLAUDE_CONFIG_DIR").then(|| "$HOME/.claude-main".to_string());
    assert_eq!(limits::account_dir(&cli(clis::CLAUDE_CODE), HOME, &gizais), Some("/home/jef/.claude-main".into()), "Gizai's own CLAUDE_CONFIG_DIR");
    assert_eq!(limits::account_dir(&cli(&w.cc2), HOME, &gizais), Some("/home/jef/.claude-2".into()), "the entry's own line wins");
    let plain_codex = Cli { id: "x".into(), name: "Codex 2".into(), kind: "codex".into(), command: "codex".into(), ..Default::default() };
    assert_eq!(limits::account_dir(&plain_codex, HOME, &none), Some("/home/jef/.codex".into()));
}

#[test]
fn the_readings_live_in_their_own_setting_and_survive_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    {
        let db = Db::open(&path).unwrap();
        seed::ensure_seed(&db, "Jeffrey").unwrap();
        limits::record(&db, clis::CLAUDE_CODE, &stream(CLAUDE_1, NOW)).unwrap();
        let raw: Option<serde_json::Value> = settings::get(&db, "subscription_limits").unwrap();
        assert_eq!(raw.unwrap()[clis::CLAUDE_CODE].as_array().map(Vec::len), Some(3));
    }
    let db = Db::open(&path).unwrap();
    assert_eq!(limits::kept(&db, clis::CLAUDE_CODE).unwrap(), stream(CLAUDE_1, NOW));
}
