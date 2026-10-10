// Housekeeping in the app (GA-25): Gizai prunes old run and chat logs and dead tokens when it starts, and a run whose
// log was pruned says so.
#[path = "support/data_lock.rs"]
mod data_lock;
use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use gizai_agents::stream::RunEvent;
use gizai_core::model::AgentInput;
use gizai_core::{ids, runs, tasks, team, tokens};
use serde_json::json;

const DAY: i64 = 24 * 60 * 60 * 1000;

/// A log at `path`, last written at `ms`.
fn file_at(path: &Path, ms: i64) {
    fs::write(path, "{\"type\":\"gizai_note\",\"text\":\"from the log\"}\n").unwrap();
    File::options().write(true).open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_millis(ms as u64)).unwrap();
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn gizai_prunes_old_logs_and_dead_tokens_when_it_starts() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    let st = gizai_lib::test_state(tmp.path());
    let now = ids::now_ms();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), ..Default::default() }).unwrap();
    let dead = tokens::mint(&st.db, &lead, json!({}), 60_000).unwrap();
    st.db.write(None, |w| {
        w.conn().execute(&format!("UPDATE api_tokens SET revoked_at={} WHERE token_sha256=?1", now - 2 * DAY), [tokens::sha256_hex(&dead)])?;
        Ok(())
    }).unwrap();
    let live = tokens::mint(&st.db, &lead, json!({}), 60 * 60_000).unwrap();
    drop(st);

    fs::create_dir_all(data.join("runs")).unwrap();
    fs::create_dir_all(data.join("chat")).unwrap();
    file_at(&data.join("runs/s1.jsonl"), now - 31 * DAY);
    file_at(&data.join("runs/s1.stderr.log"), now - 31 * DAY);
    file_at(&data.join("runs/s2.jsonl"), now - 29 * DAY);
    file_at(&data.join("chat/a1.jsonl"), now - 45 * DAY);
    file_at(&data.join("chat/check-s3.jsonl"), now - DAY);
    data_lock::released_blocking(&data);
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(names(&data.join("runs")), ["s2.jsonl"]);
    assert_eq!(names(&data.join("chat")), ["check-s3.jsonl"]);
    let left: i64 = st.db.read(|c| Ok(c.query_row("SELECT count(*) FROM api_tokens", [], |r| r.get(0))?)).unwrap();
    assert_eq!(left, 1, "the token revoked two days ago went");
    assert!(tokens::verify(&st.db, &live).unwrap().is_some(), "the live one still works");
}

#[test]
fn a_run_whose_log_was_pruned_says_its_output_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let now = ids::now_ms();
    // an ended run (each on its own card) with its log in runs/
    let ended_run = |session: &str, days_ago: i64| -> String {
        let task = gizai_lib::test_task(&st, "", "backend");
        let agent = tasks::get(&st.db, &task).unwrap().assignee_id.unwrap();
        let log = st.data_dir.join("runs").join(format!("{session}.jsonl"));
        let id = runs::create(&st.db, &agent, &task, "backend", session, "/w", "/w", "b", &log.to_string_lossy()).unwrap();
        st.db.write(None, |w| {
            w.conn().execute(&format!("UPDATE runs SET status='succeeded', ended_at={} WHERE id=?1", now - days_ago * DAY), [&id])?;
            Ok(())
        }).unwrap();
        id
    };
    let events = |id: &str| -> Vec<RunEvent> { gizai_lib::runs::events_for(&st, id).into_iter().map(|e| e.event).collect() };

    let pruned = ended_run("s1", 31);
    let evs = events(&pruned);
    assert_eq!(evs.len(), 1, "{evs:?}");
    let RunEvent::Note { text } = &evs[0] else { panic!("{evs:?}") };
    assert!(text.contains("30 days") && text.contains("gone") && text.contains("summary, cost and commits stay"), "{text}");

    // a recent run without its log: nothing to say, as before
    let recent = ended_run("s2", 29);
    assert!(events(&recent).is_empty());

    // an old run whose log is still there is replayed from it
    let kept = ended_run("s3", 31);
    fs::create_dir_all(st.data_dir.join("runs")).unwrap();
    file_at(&st.data_dir.join("runs/s3.jsonl"), now - 31 * DAY);
    assert_eq!(events(&kept), [RunEvent::Note { text: "from the log".into() }]);
}
