//! GA-228 QA: Builds on in core. Migration 0021 on a schema 20 database (only the new table and its index, after a
//! backup); the rules (a card on itself, a loop directly or through other cards, word for word, more than 20 cards, a card
//! named twice, another project's card, and a refusal changes nothing); merged (Deploy or Done, archived from Done
//! counts, a cancelled card never does); `Task::waits_for` by number and `waits_message` word for word; the queue
//! (`workflow::waiting_for`) passes over a card that waits, in To do, Testing or a Research column, without a move or a
//! hold, and takes it once the last card is merged; the board check's `builds_on_blocked`; `tasks::create_with` and
//! `update_with` set it in the same write as the card; and a card without Builds on works as today.
use std::path::Path;

use gizai_core::board::{self, Context, Finding};
use gizai_core::columns::{self, ColumnInput};
use gizai_core::db::{self, Db};
use gizai_core::{builds_on, ids, model::*, projects, seed, tasks, team, workflow};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

const HOUR: i64 = 60 * 60_000;

// ---- set-up ----

struct B { db: Db, you: String, team: String, be: String, qa: String, lead: String, ga: String, kade: String }

/// Project "Giz AI" (GA) and "Kade" (KADE); a Backend Agent with four cards at once (To do, In progress), a QA Agent
/// (Testing) and a Team Lead; the seven default columns and a Cancelled column after Done.
fn board_() -> B {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let ga = projects::create(&db, &s.you_id, ProjectInput { name: "Giz AI".into(), key: "GA".into(), ..Default::default() }).unwrap();
    let kade = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let agent = |name: &str, role: &str, max_runs: i64| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), wakeup: "on_assign".into(), max_runs: Some(max_runs), chat_enabled: Some(role == "lead"),
        ..Default::default() }).unwrap();
    let be = agent("Backend Agent", "backend", 4);
    let qa = agent("QA Agent", "qa", 3);
    let lead = agent("Team Lead", "lead", 1);
    gizai_core::questions::set_enabled(&db, false).unwrap();
    let b = B { db, you: s.you_id, team: s.team_id, be, qa, lead, ga, kade };
    team::add_state(&b.db, &b.you, &b.team, "Cancelled", &b.state("Done"), "cancelled").unwrap();
    b
}

impl B {
    fn state(&self, name: &str) -> String {
        team::get(&self.db, &self.team).unwrap().states.into_iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id
    }
    fn card_in(&self, project: &str, column: &str, priority: i64) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: project.to_string(), title: format!("A card in {column}"),
            state_id: Some(self.state(column)), priority, ..Default::default() }).unwrap()
    }
    fn card(&self, column: &str) -> String { self.card_in(&self.ga, column, 0) }
    fn ident(&self, t: &str) -> String { tasks::get(&self.db, t).unwrap().identifier }
    fn mv(&self, t: &str, column: &str) { tasks::move_to(&self.db, &self.you, t, &self.state(column), "").unwrap(); }
    fn set(&self, t: &str, on: &[&String]) -> gizai_core::Result<()> {
        builds_on::set(&self.db, &self.you, t, &on.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }
    fn on(&self, t: &str) -> Vec<String> { builds_on::of(&self.db, t).unwrap().builds_on.into_iter().map(|l| l.identifier).collect() }
    fn needed_by(&self, t: &str) -> Vec<String> { builds_on::of(&self.db, t).unwrap().needed_by.into_iter().map(|l| l.identifier).collect() }
    fn waits(&self, t: &str) -> Vec<String> { tasks::get(&self.db, t).unwrap().waits_for }
    fn hold(&self, t: &str, hold: &str) {
        tasks::update(&self.db, &self.you, t, TaskPatch { hold: Some(hold.into()), hold_reason: Some("Which export format?".into()), ..Default::default() }).unwrap();
    }
    fn check_at(&self, now: i64) -> Vec<Finding> {
        board::check(&self.db, &Context { now, max_concurrent: 10, lead_id: Some(self.lead.clone()), ..Default::default() }).unwrap()
    }
    fn of(&self, all: &[Finding], task: &str) -> Vec<(String, String, String)> {
        all.iter().filter(|f| f.task_id == task).map(|f| (f.kind.clone(), f.code.clone(), f.reason.clone())).collect()
    }
    fn count(&self) -> usize { tasks::list(&self.db, &TaskFilter::default()).unwrap().len() }
}

fn err(r: gizai_core::Result<impl std::fmt::Debug>) -> String {
    r.unwrap_err().to_string()
}

// ---- 1. migration 0021 ----

/// Migrations 0001..0020 on a new file at `path`: a genuine schema 20 database, as the version before GA-228 leaves it.
fn v20_db(path: &Path) -> Connection {
    let all = [
        include_str!("../../migrations/0001_init.sql"), include_str!("../../migrations/0002_agents.sql"), include_str!("../../migrations/0003_chat.sql"),
        include_str!("../../migrations/0004_effort.sql"), include_str!("../../migrations/0005_pull_requests.sql"),
        include_str!("../../migrations/0006_worktree_prepare.sql"), include_str!("../../migrations/0007_card_flow.sql"),
        include_str!("../../migrations/0008_board_check.sql"), include_str!("../../migrations/0009_agent_folders.sql"),
        include_str!("../../migrations/0010_run_refusals.sql"), include_str!("../../migrations/0011_column_agents.sql"),
        include_str!("../../migrations/0012_chat_runs_on.sql"), include_str!("../../migrations/0013_bitbucket.sql"),
        include_str!("../../migrations/0014_memory.sql"), include_str!("../../migrations/0015_lead_merges.sql"),
        include_str!("../../migrations/0016_memory_groups.sql"), include_str!("../../migrations/0017_pending_actions.sql"),
        include_str!("../../migrations/0018_indexes.sql"), include_str!("../../migrations/0019_screenshots.sql"),
        include_str!("../../migrations/0020_test_runs_at_once.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 1, "one schema step back");
    let mut c = Connection::open(path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c
}

fn schema(c: &Connection) -> Vec<(String, String, Option<String>)> {
    let mut v: Vec<_> = c.prepare("SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'").unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect();
    v.sort();
    v
}

fn version(c: &Connection) -> i64 {
    c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

#[test]
fn a_schema_20_database_gets_the_builds_on_table_and_nothing_else_after_a_backup() {
    assert_eq!(db::SCHEMA_VERSION, 21, "0021 is the latest migration");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let ga = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let ga = projects::create(&db, &s.you_id, ProjectInput { name: "Giz AI".into(), key: "GA".into(), ..Default::default() }).unwrap();
        for title in ["One", "Two"] {
            tasks::create(&db, &s.you_id, TaskInput { project_id: ga.clone(), title: title.into(), ..Default::default() }).unwrap();
        }
        ga
    };
    // one step back to the database the version before GA-228 leaves (schema 20): 0021 only adds the table and its index
    let c = Connection::open(&path).unwrap();
    c.execute_batch("DROP INDEX task_builds_on_needed_by; DROP TABLE task_builds_on; PRAGMA user_version = 20;").unwrap();
    let other = tempfile::tempdir().unwrap();
    let genuine = v20_db(&other.path().join("gizai.db"));
    assert_eq!(schema(&c), schema(&genuine), "the step back is exactly schema 20 (migrations 0001..0020)");
    drop(c);

    let db = Db::open(&path).unwrap();
    db.read(|c| {
        assert_eq!(version(c), 21, "0021 ran");
        let cols: Vec<(String, String, i64, i64)> = c.prepare("SELECT name, type, \"notnull\", pk FROM pragma_table_info('task_builds_on') ORDER BY cid")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        assert_eq!(cols, vec![("task_id".to_string(), "TEXT".to_string(), 1, 1), ("builds_on_id".to_string(), "TEXT".to_string(), 1, 2),
                              ("created_at".to_string(), "INTEGER".to_string(), 1, 0)], "(task_id, builds_on_id, created_at), the pair as primary key");
        let n: i64 = c.query_row("SELECT count(*) FROM task_builds_on", [], |r| r.get(0))?;
        assert_eq!(n, 0, "no card builds on another yet");
        let mut rest = schema(c);
        rest.retain(|(_, name, _)| name != "task_builds_on" && name != "task_builds_on_needed_by");
        assert_eq!(rest, schema(&genuine), "nothing else changed");
        Ok(())
    }).unwrap();
    // the cards are as they were, and wait for nothing
    let list = tasks::list(&db, &TaskFilter { project_id: Some(ga), open_only: false }).unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|t| t.waits_for.is_empty()));
    let snaps: Vec<_> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().path()).collect();
    assert!(snaps.len() == 1 && snaps[0].file_name().unwrap().to_string_lossy().starts_with("gizai-before-v21-"), "{snaps:?}");
    let old = Connection::open(&snaps[0]).unwrap();
    assert_eq!(version(&old), 20, "the backup is the schema 20 database");
    assert_eq!(schema(&old), schema(&genuine));
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.read(|c| Ok(version(c))).unwrap(), 21);
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1, "no new backup");
}

#[test]
fn the_table_refuses_a_card_on_itself_and_a_pair_twice_and_a_deleted_card_takes_its_links_with_it() {
    let b = board_();
    let (a, c) = (b.card("To do"), b.card("To do"));
    b.db.write(Some(&b.you), |w| {
        let x = w.conn();
        assert!(x.execute("INSERT INTO task_builds_on(task_id, builds_on_id, created_at) VALUES (?1, ?1, 1)", [&a]).is_err(), "on itself");
        x.execute("INSERT INTO task_builds_on(task_id, builds_on_id, created_at) VALUES (?1, ?2, 1)", [&c, &a])?;
        assert!(x.execute("INSERT INTO task_builds_on(task_id, builds_on_id, created_at) VALUES (?1, ?2, 2)", [&c, &a]).is_err(), "the pair twice");
        x.execute("DELETE FROM tasks WHERE id=?1", [&a])?;
        let n: i64 = x.query_row("SELECT count(*) FROM task_builds_on", [], |r| r.get(0))?;
        assert_eq!(n, 0, "ON DELETE CASCADE");
        Ok(())
    }).unwrap();
}

// ---- 2. the rules ----

#[test]
fn a_card_on_itself_a_direct_loop_and_a_loop_through_other_cards_are_refused_word_for_word_and_nothing_changes() {
    let b = board_();
    let (g1, g2, g3, g4) = (b.card("To do"), b.card("To do"), b.card("To do"), b.card("To do"));
    assert_eq!([b.ident(&g1), b.ident(&g2), b.ident(&g3), b.ident(&g4)], ["GA-1", "GA-2", "GA-3", "GA-4"]);

    assert_eq!(err(b.set(&g1, &[&g1])), "GA-1 can't build on itself.");
    assert_eq!(err(b.set(&g1, &[&g2, &g1])), "GA-1 can't build on itself.", "among others too");
    assert!(b.on(&g1).is_empty(), "nothing changed");

    // direct: GA-2 builds on GA-1, so GA-1 can't build on GA-2
    b.set(&g2, &[&g1]).unwrap();
    assert_eq!(err(b.set(&g1, &[&g2])), "GA-1 can't build on GA-2: GA-2 already builds on GA-1.");
    assert_eq!(err(builds_on::add(&b.db, &b.you, &g1, &g2)), "GA-1 can't build on GA-2: GA-2 already builds on GA-1.", "the card page's Add");
    // through a third card: GA-3 builds on GA-2, which builds on GA-1
    b.set(&g3, &[&g2]).unwrap();
    assert_eq!(err(b.set(&g1, &[&g3])), "GA-1 can't build on GA-3: GA-3 already builds on GA-1, through GA-2.");
    // through two: GA-4 → GA-3 → GA-2 → GA-1
    b.set(&g4, &[&g3]).unwrap();
    assert_eq!(err(b.set(&g1, &[&g4])), "GA-1 can't build on GA-4: GA-4 already builds on GA-1, through GA-3 and GA-2.");
    // a refused list changes nothing, also when other cards in it were fine
    let g5 = b.card("To do");
    b.set(&g1, &[&g5]).unwrap();
    assert!(b.set(&g1, &[&g5, &g3]).is_err());
    assert_eq!(b.on(&g1), ["GA-5"], "GA-1 still builds on GA-5 only");
    assert_eq!((b.on(&g2), b.on(&g3), b.on(&g4)), (vec!["GA-1".to_string()], vec!["GA-2".to_string()], vec!["GA-3".to_string()]));

    // not a loop: two cards building on the same card, and a card that builds on two that build on one (a diamond)
    let g6 = b.card("To do");
    b.set(&g6, &[&g2, &g3, &g1]).unwrap();
    assert_eq!(b.on(&g6), ["GA-1", "GA-2", "GA-3"], "by number");
    assert_eq!(b.needed_by(&g1), ["GA-2", "GA-6"]);
}

#[test]
fn a_card_builds_on_at_most_20_cards_a_card_named_twice_counts_once_and_another_projects_card_may_be_named() {
    let b = board_();
    let me = b.card("To do");
    let others: Vec<String> = (0..21).map(|_| b.card("Backlog")).collect();
    let refs: Vec<&String> = others.iter().collect();
    assert_eq!(err(b.set(&me, &refs)), "GA-1 can't build on more than 20 cards.", "a 21st card");
    assert!(b.on(&me).is_empty(), "nothing changed");
    b.set(&me, &refs[..20]).unwrap();
    assert_eq!(b.on(&me).len(), 20, "20 is the most");
    assert_eq!(err(builds_on::add(&b.db, &b.you, &me, &others[20])), "GA-1 can't build on more than 20 cards.", "the card page's Add");
    // the same card twice counts once: still 20
    let mut twice = refs[..20].to_vec();
    twice.push(refs[0]);
    b.set(&me, &twice).unwrap();
    assert_eq!(b.on(&me).len(), 20);
    // another project's card
    let k = b.card_in(&b.kade, "In progress", 0);
    b.set(&me, &[&k]).unwrap();
    assert_eq!(b.on(&me), ["KADE-1"]);
    assert_eq!(b.waits(&me), ["KADE-1"]);
    assert_eq!(b.needed_by(&k), ["GA-1"]);
    // a card that doesn't exist
    assert!(matches!(b.set(&me, &[&"nope".to_string()]).unwrap_err(), gizai_core::Error::NotFound(_)));
    assert_eq!(b.on(&me), ["KADE-1"]);
    // an empty list clears it; remove takes one off; adding one it has changes nothing
    builds_on::add(&b.db, &b.you, &me, &others[0]).unwrap();
    builds_on::add(&b.db, &b.you, &me, &others[0]).unwrap();
    assert_eq!(b.on(&me), ["GA-2", "KADE-1"]);
    builds_on::remove(&b.db, &b.you, &me, &k).unwrap();
    builds_on::remove(&b.db, &b.you, &me, &k).unwrap();
    assert_eq!(b.on(&me), ["GA-2"]);
    b.set(&me, &[]).unwrap();
    assert!(b.on(&me).is_empty() && b.waits(&me).is_empty());
}

// ---- 3. merged ----

#[test]
fn merged_means_a_deploy_or_done_column_an_archived_card_counts_and_a_cancelled_one_never_does() {
    let b = board_();
    let a = b.card("To do");
    let c = b.card("To do");
    b.set(&c, &[&a]).unwrap();
    let merged = || builds_on::of(&b.db, &c).unwrap().builds_on[0].merged;
    for column in ["To do", "In progress", "Testing", "Review", "Backlog"] {
        b.mv(&a, column);
        assert_eq!((b.waits(&c), merged()), (vec!["GA-1".to_string()], false), "{column}");
    }
    for column in ["Deploy", "Done"] {
        b.mv(&a, column);
        assert_eq!((b.waits(&c), merged()), (vec![], true), "{column}");
    }
    // archived (from Done): still merged
    tasks::archive(&b.db, &b.you, &a).unwrap();
    let l = builds_on::of(&b.db, &c).unwrap().builds_on.remove(0);
    assert_eq!((l.merged, l.archived, l.column.as_str()), (true, true, "Done"));
    assert!(b.waits(&c).is_empty());
    tasks::restore(&b.db, &b.you, &a).unwrap();
    // cancelled: never
    b.mv(&a, "Cancelled");
    let l = builds_on::of(&b.db, &c).unwrap().builds_on.remove(0);
    assert_eq!((l.merged, l.category.as_str()), (false, "cancelled"));
    assert_eq!(b.waits(&c), ["GA-1"]);
    assert!(!builds_on::merged("cancelled") && builds_on::merged("deploy") && builds_on::merged("done"));
    // a column of kind deploy of the team's own ("Staging") counts too
    let staging = team::add_state(&b.db, &b.you, &b.team, "Staging", &b.state("Review"), "deploy").unwrap();
    tasks::move_to(&b.db, &b.you, &a, &staging, "").unwrap();
    assert!(b.waits(&c).is_empty());
}

#[test]
fn waits_for_is_by_number_on_every_card_and_the_start_message_is_word_for_word_for_one_card_and_for_several() {
    let b = board_();
    let cards: Vec<String> = (0..11).map(|_| b.card("In progress")).collect();
    let me = b.card("To do");
    assert_eq!(b.ident(&me), "GA-12");
    // GA-10 after GA-9, not after GA-1
    b.set(&me, &[&cards[9], &cards[1], &cards[8]]).unwrap();
    assert_eq!(b.waits(&me), ["GA-2", "GA-9", "GA-10"]);
    let listed = tasks::list(&b.db, &TaskFilter::default()).unwrap().into_iter().find(|t| t.id == me).unwrap();
    assert_eq!(listed.waits_for, ["GA-2", "GA-9", "GA-10"], "the list's cards too");
    b.mv(&cards[1], "Deploy");
    assert_eq!(b.waits(&me), ["GA-9", "GA-10"], "only those not merged yet");

    assert_eq!(builds_on::waits_message("GA-227", &["GA-226".into()]),
        "GA-227 builds on GA-226, which isn't merged yet. It starts by itself once GA-226 is merged; to start it now, take GA-226 off its Builds on.");
    assert_eq!(builds_on::waits_message("GA-227", &["GA-225".into(), "GA-226".into()]),
        "GA-227 builds on GA-225 and GA-226, which aren't merged yet. It starts by itself once they are merged; to start it now, take them off its Builds on.");
    assert_eq!(builds_on::waits_message("GA-12", &["GA-2".into(), "GA-9".into(), "GA-10".into()]),
        "GA-12 builds on GA-2, GA-9 and GA-10, which aren't merged yet. It starts by itself once they are merged; to start it now, take them off its Builds on.");
}

// ---- 4. the queue ----

#[test]
fn the_queue_passes_over_a_card_that_waits_without_moving_or_holding_it_and_takes_it_once_the_last_card_is_merged() {
    let b = board_();
    let a = b.card_in(&b.ga, "To do", 2);
    let w = b.card_in(&b.ga, "To do", 1); // urgent: first in line but for Builds on
    let c = b.card_in(&b.ga, "To do", 3);
    b.set(&w, &[&a]).unwrap();
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [a.clone(), c.clone()], "GA-2 passed over; GA-1 and GA-3 taken");
    let t = tasks::get(&b.db, &w).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("To do", None), "no move, no hold");
    assert_eq!(workflow::next_task_for(&b.db, &b.be).unwrap(), Some(a.clone()));

    // GA-1 moves on but isn't merged yet: GA-2 still waits
    for column in ["In progress", "Testing", "Review"] {
        b.mv(&a, column);
        assert!(!workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w), "{column}");
    }
    b.mv(&a, "Deploy");
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [w.clone(), c.clone()], "GA-2 in its place again, first");

    // two cards: it waits until the last one is merged
    let a2 = b.card("In progress");
    b.set(&w, &[&a, &a2]).unwrap();
    assert!(!workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w));
    b.mv(&a2, "Done");
    assert!(workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w));
    // cancelled never counts
    b.mv(&a2, "Cancelled");
    assert!(!workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w));
    // taking the card off its Builds on lets it be taken
    builds_on::remove(&b.db, &b.you, &w, &a2).unwrap();
    assert!(workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w));
}

#[test]
fn builds_on_holds_a_card_in_testing_or_in_a_research_column_the_same_way() {
    let b = board_();
    let a = b.card("Review"); // not merged yet, and in no agent's column
    // Testing: the QA Agent
    let t = b.card("Testing");
    b.set(&t, &[&a]).unwrap();
    let other = b.card("Testing");
    assert_eq!(workflow::waiting_for(&b.db, &b.qa).unwrap(), [other.clone()]);
    // Research: a work column of the team's own, on Auto, with a Research agent on it
    let research = team::add_state(&b.db, &b.you, &b.team, "Research", &b.state("To do"), "in_progress").unwrap();
    let ra = team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: "Research Agent".into(), role_key: "backend".into(),
        wakeup: "on_assign".into(), max_runs: Some(2), ..Default::default() }).unwrap();
    columns::set_column(&b.db, &b.you, &research, ColumnInput { agent_ids: Some(vec![ra.clone()]), auto: Some(true),
        next_state_id: Some(b.state("In progress")), ..Default::default() }).unwrap();
    let r = b.card("Research");
    b.set(&r, &[&a]).unwrap();
    let r2 = b.card("Research");
    assert_eq!(workflow::waiting_for(&b.db, &ra).unwrap(), [r2.clone()]);
    for (card, column) in [(&t, "Testing"), (&r, "Research")] {
        let x = tasks::get(&b.db, card).unwrap();
        assert_eq!((x.state_name.as_str(), x.hold.as_deref()), (column, None));
    }
    b.mv(&a, "Deploy");
    assert_eq!(workflow::waiting_for(&b.db, &b.qa).unwrap(), [t.clone(), other.clone()]);
    assert_eq!(workflow::waiting_for(&b.db, &ra).unwrap(), [r.clone(), r2.clone()]);
}

#[test]
fn a_card_without_builds_on_works_as_today() {
    let b = board_();
    let x = b.card("To do");
    let y = b.card("To do");
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [x.clone(), y.clone()]);
    assert_eq!(builds_on::of(&b.db, &x).unwrap(), builds_on::BuildsOn::default());
    assert!(b.waits(&x).is_empty());
    let json = serde_json::to_value(tasks::get(&b.db, &x).unwrap()).unwrap();
    assert_eq!(json["waitsFor"], serde_json::json!([]), "the app gets an empty list");
    // holds still hold, and the board check is as before: a free slot is a finding after the grace
    b.hold(&y, "blocked");
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [x.clone()]);
    let now = ids::now_ms();
    let all = b.check_at(now + HOUR);
    assert_eq!(b.of(&all, &x).iter().map(|f| f.1.as_str()).collect::<Vec<_>>(), ["free_slot"]);
}

// ---- 5. the board check ----

#[test]
fn the_board_check_reports_builds_on_blocked_for_a_card_waiting_for_a_held_or_cancelled_card_and_nothing_for_one_still_in_work() {
    let b = board_();
    let a = b.card("In progress");
    let w = b.card("To do");
    b.set(&w, &[&a]).unwrap();
    let later = ids::now_ms() + 3 * HOUR;
    // simply still in work: no finding at all (no free_slot either, though the Backend Agent has free slots)
    assert!(b.of(&b.check_at(later), &w).is_empty(), "{:?}", b.of(&b.check_at(later), &w));

    b.hold(&a, "needs_decision");
    let all = b.check_at(later);
    assert_eq!(b.of(&all, &w), [("waiting".to_string(), "builds_on_blocked".to_string(),
        "GA-2 waits for GA-1, which is on hold: needs_decision".to_string())]);
    assert!(b.of(&all, &a).iter().any(|f| f.0 == "held"), "the held card is a finding of its own");

    tasks::update(&b.db, &b.you, &a, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    assert!(b.of(&b.check_at(later), &w).is_empty(), "off hold: nothing again");

    b.mv(&a, "Cancelled");
    assert_eq!(b.of(&b.check_at(later), &w), [("waiting".to_string(), "builds_on_blocked".to_string(),
        "GA-2 waits for GA-1, which is cancelled".to_string())]);

    // one held and one cancelled: one finding naming both
    let h = b.card("In progress");
    b.hold(&h, "stalled");
    b.set(&w, &[&a, &h]).unwrap();
    assert_eq!(b.of(&b.check_at(later), &w), [("waiting".to_string(), "builds_on_blocked".to_string(),
        "GA-2 waits for GA-1, which is cancelled, and for GA-3, which is on hold: stalled".to_string())]);

    // merged: it starts, so the usual findings again
    b.set(&w, &[&a]).unwrap();
    b.mv(&a, "Done");
    assert_eq!(b.of(&b.check_at(later), &w).iter().map(|f| f.1.as_str()).collect::<Vec<_>>(), ["free_slot"]);
}

// ---- 6. in the same write as the card ----

#[test]
fn create_with_and_update_with_set_builds_on_in_the_same_write_and_a_refusal_changes_nothing() {
    let b = board_();
    let a = b.card("In progress");
    let others: Vec<String> = (0..21).map(|_| b.card("Backlog")).collect();
    let input = |title: &str| TaskInput { project_id: b.ga.clone(), title: title.into(), state_id: Some(b.state("To do")), ..Default::default() };

    let n = b.count();
    assert!(tasks::create_with(&b.db, &b.you, input("Too many"), None, &others).is_err(), "21 cards");
    assert_eq!(b.count(), n, "no card made");
    let w = tasks::create_with(&b.db, &b.you, input("Screens"), None, &[a.clone()]).unwrap();
    assert_eq!(b.waits(&w), [b.ident(&a)]);
    assert!(!workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w), "no agent takes it before it waits");

    // a loop together with a new title: nothing changes
    let patch = || TaskPatch { title: Some("Renamed".into()), ..Default::default() };
    let e = tasks::update_with(&b.db, &b.you, &a, patch(), None, Some(vec![w.clone()])).unwrap_err();
    assert_eq!(e.to_string(), format!("{} can't build on {}: {} already builds on {}.", b.ident(&a), b.ident(&w), b.ident(&w), b.ident(&a)));
    assert_eq!(tasks::get(&b.db, &a).unwrap().title, "A card in In progress");
    // left out keeps it; an empty list clears it
    tasks::update_with(&b.db, &b.you, &w, patch(), None, None).unwrap();
    assert_eq!(b.waits(&w), [b.ident(&a)]);
    tasks::update_with(&b.db, &b.you, &w, TaskPatch::default(), None, Some(vec![])).unwrap();
    assert!(b.on(&w).is_empty());
    assert!(workflow::waiting_for(&b.db, &b.be).unwrap().contains(&w));
    // an archived card is read-only
    b.mv(&w, "Done");
    tasks::archive(&b.db, &b.you, &w).unwrap();
    assert!(err(b.set(&w, &[&a])).contains("is archived: restore it first"));
}

#[test]
fn the_cards_activity_notes_the_new_list_by_number() {
    let b = board_();
    let (x, y, me) = (b.card("Backlog"), b.card("Backlog"), b.card("To do"));
    b.set(&me, &[&y, &x]).unwrap();
    b.set(&me, &[]).unwrap();
    let diffs: Vec<String> = b.db.read(|c| {
        let mut st = c.prepare("SELECT diff_json FROM changes WHERE table_name='tasks' AND row_id=?1 AND diff_json LIKE '%buildsOn%' ORDER BY seq")?;
        Ok(st.query_map([&me], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?)
    }).unwrap();
    let diffs: Vec<serde_json::Value> = diffs.iter().map(|d| serde_json::from_str(d).unwrap()).collect();
    assert_eq!(diffs, [serde_json::json!({"buildsOn": ["GA-1", "GA-2"]}), serde_json::json!({"buildsOn": []})]);
}

#[test]
fn the_team_leads_instructions_have_the_new_line_word_for_word() {
    let lead = seed::role_template("lead");
    assert!(lead.contains("\n- When a card needs another card's code, set builds_on: it then starts by itself once that card is merged.\n"), "{lead}");
}
