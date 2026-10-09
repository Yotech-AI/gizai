// Housekeeping (GA-25): the chat tools' dead tokens and the run and chat logs older than 30 days go; everything else
// stays.
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gizai_core::db::Db;
use gizai_core::model::AgentInput;
use gizai_core::{housekeeping, ids, seed, team, tokens};
use serde_json::json;

const HOUR: i64 = 60 * 60 * 1000;
const DAY: i64 = 24 * HOUR;

fn lead(db: &Db) -> String {
    let s = seed::ensure_seed(db, "Jeffrey").unwrap();
    team::add_agent(db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap()
}

/// Sets a token's expiry and revocation, in ms (None: never / not revoked).
fn set_token(db: &Db, token: &str, expires_at: Option<i64>, revoked_at: Option<i64>) {
    db.write(None, |w| {
        w.conn().execute("UPDATE api_tokens SET expires_at=?2, revoked_at=?3 WHERE token_sha256=?1",
                         rusqlite::params![tokens::sha256_hex(token), expires_at, revoked_at])?;
        Ok(())
    }).unwrap();
}

fn token_count(db: &Db) -> i64 {
    db.read(|c| Ok(c.query_row("SELECT count(*) FROM api_tokens", [], |r| r.get(0))?)).unwrap()
}

fn time(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms as u64)
}

/// A file at `path`, last written at `ms`.
fn file_at(path: &Path, ms: i64) {
    fs::write(path, "{}\n").unwrap();
    File::options().write(true).open(path).unwrap().set_modified(time(ms)).unwrap();
}

/// Sets the link's own time, not its target's (std only sets a target's).
fn link_at(path: &Path, ms: i64) {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
    let ts = libc::timespec { tv_sec: (ms / 1000) as libc::time_t, tv_nsec: 0 };
    // SAFETY: a valid C string and two timespecs that live through the call.
    let r = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), [ts, ts].as_ptr(), libc::AT_SYMLINK_NOFOLLOW) };
    assert_eq!(r, 0, "utimensat {path:?}");
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn tokens_dead_for_more_than_a_day_are_pruned_and_the_others_stay() {
    let db = Db::open_in_memory().unwrap();
    let lead = lead(&db);
    let now = ids::now_ms();
    let mint = || tokens::mint(&db, &lead, json!({"chat": "T1"}), HOUR).unwrap();
    let live = mint();
    let no_expiry = mint();
    set_token(&db, &no_expiry, None, None);
    let expired_lately = mint();
    set_token(&db, &expired_lately, Some(now - 23 * HOUR), None);
    let expired_long_ago = mint();
    set_token(&db, &expired_long_ago, Some(now - 25 * HOUR), None);
    let revoked_lately = mint();
    tokens::revoke(&db, &revoked_lately).unwrap();
    // revoked two days ago, though it would only expire in an hour
    let revoked_long_ago = mint();
    set_token(&db, &revoked_long_ago, Some(now + HOUR), Some(now - 2 * DAY));
    assert_eq!(token_count(&db), 6);

    assert_eq!(tokens::prune(&db, now).unwrap(), 2, "the expired and the revoked one, both dead for more than a day");
    assert_eq!(token_count(&db), 4);
    assert_eq!(tokens::verify(&db, &live).unwrap().unwrap().actor_id, lead, "a live token still works");
    assert!(tokens::verify(&db, &no_expiry).unwrap().is_some());
    assert_eq!(tokens::prune(&db, now).unwrap(), 0, "nothing more to prune");

    // two days on, every dead token has gone; one that never expires stays
    assert_eq!(tokens::prune(&db, now + 2 * DAY).unwrap(), 3, "the live one expired, and the two that died lately");
    assert_eq!(token_count(&db), 1);
    assert!(tokens::verify(&db, &no_expiry).unwrap().is_some());
    assert_eq!(tokens::KEEP_DEAD_MS, DAY);
}

#[test]
fn logs_older_than_thirty_days_are_pruned_and_everything_else_stays() {
    let tmp = tempfile::tempdir().unwrap();
    let runs = tmp.path().join("runs");
    fs::create_dir(&runs).unwrap();
    let now = ids::now_ms();
    let old = now - 30 * DAY - 60_000;
    let recent = now - 30 * DAY + 60_000;
    file_at(&runs.join("s1.jsonl"), old);
    file_at(&runs.join("s1.stderr.log"), old);
    file_at(&runs.join("s2.jsonl"), recent);
    file_at(&runs.join("s2.stderr.log"), recent);
    // a live run's MCP config, and anything else that isn't a log
    file_at(&runs.join("r1.mcp.json"), old);
    file_at(&runs.join("notes.txt"), old);
    // a folder named like a log
    fs::create_dir(runs.join("old.jsonl")).unwrap();
    File::open(runs.join("old.jsonl")).unwrap().set_modified(time(old)).unwrap();
    // a link named like a log, to an old log somewhere else: neither goes
    let elsewhere = tmp.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    file_at(&elsewhere.join("kept.jsonl"), old);
    std::os::unix::fs::symlink(elsewhere.join("kept.jsonl"), runs.join("link.jsonl")).unwrap();
    link_at(&runs.join("link.jsonl"), old);

    assert_eq!(housekeeping::prune_logs(&runs, now), 2);
    assert_eq!(names(&runs), ["link.jsonl", "notes.txt", "old.jsonl", "r1.mcp.json", "s2.jsonl", "s2.stderr.log"]);
    assert!(elsewhere.join("kept.jsonl").exists(), "a link's target stays");
    assert_eq!(housekeeping::prune_logs(&runs, now), 0, "nothing more to prune");
    assert_eq!(housekeeping::prune_logs(&tmp.path().join("missing"), now), 0, "a missing folder has nothing to prune");
    assert_eq!(housekeeping::KEEP_LOG_DAYS, 30);
}

#[test]
fn housekeeping_prunes_tokens_and_the_logs_of_runs_and_chats() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path();
    let db = Db::open(&data.join("gizai.db")).unwrap();
    let lead = lead(&db);
    let now = ids::now_ms();
    let dead = tokens::mint(&db, &lead, json!({}), HOUR).unwrap();
    set_token(&db, &dead, Some(now - 2 * DAY), None);
    let live = tokens::mint(&db, &lead, json!({}), HOUR).unwrap();
    // before the folders exist: only the token goes
    assert_eq!(housekeeping::run(&db, data, now).unwrap(), housekeeping::Pruned { tokens: 1, logs: 0 });

    for d in ["runs", "chat", "backups"] {
        fs::create_dir(data.join(d)).unwrap();
    }
    file_at(&data.join("runs/s1.jsonl"), now - 40 * DAY);
    file_at(&data.join("runs/s2.jsonl"), now - DAY);
    file_at(&data.join("chat/a1.jsonl"), now - 31 * DAY);
    file_at(&data.join("chat/a1.stderr.log"), now - 31 * DAY);
    file_at(&data.join("chat/check-s3.jsonl"), now - 31 * DAY);
    file_at(&data.join("chat/a2.jsonl"), now - HOUR);
    // other folders aren't touched, whatever their files are called
    file_at(&data.join("backups/old.log"), now - 400 * DAY);
    file_at(&data.join("old.jsonl"), now - 400 * DAY);
    assert_eq!(housekeeping::run(&db, data, now).unwrap(), housekeeping::Pruned { tokens: 0, logs: 4 });
    assert_eq!(names(&data.join("runs")), ["s2.jsonl"]);
    assert_eq!(names(&data.join("chat")), ["a2.jsonl"]);
    assert_eq!(names(&data.join("backups")), ["old.log"]);
    assert!(data.join("old.jsonl").exists());
    assert!(tokens::verify(&db, &live).unwrap().is_some());
}
