//! GA-111: a person's card. A card assigned to you that you start by hand (Open in editor) is pinned to you, so no
//! agent of an Auto column takes it; Hand over moves it on as a builder's `ready_for_testing` does and takes the pin
//! off, so QA takes it; a QA fail brings it back to you, pinned again. The core side (`workflow::start_by_hand`,
//! `hand_over`, the queue, `apply_outcome`, the board check); the app side is in src-tauri/tests/it/hand_flow_test.rs.
use gizai_core::{board, comments, db::Db, model::*, projects, runs, seed::ensure_seed, tasks, workflow};

fn setup() -> (Db, gizai_core::seed::SeedIds, String) {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "CSV".into(), ..Default::default() }).unwrap();
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "To do"), "a1").unwrap();
    // add_agent puts each on its role's usual columns: the Backend Agent on To do and In progress, the QA Agent on Testing
    for (name, role) in [("Backend Agent", "backend"), ("QA Agent", "qa")] {
        gizai_core::team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap();
    }
    (db, s, t)
}

/// `setup`, with the card assigned to you.
fn yours() -> (Db, gizai_core::seed::SeedIds, String) {
    let (db, s, t) = setup();
    assign(&db, &s, &t, &s.you_id);
    (db, s, t)
}

fn assign(db: &Db, s: &gizai_core::seed::SeedIds, t: &str, to: &str) {
    tasks::update(db, &s.you_id, t, TaskPatch { assignee_id: Some(to.into()), ..Default::default() }).unwrap();
}

fn state(db: &Db, name: &str) -> String {
    db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
}

fn agent(db: &Db, name: &str) -> String {
    db.read(|c| Ok(c.query_row("SELECT id FROM actors WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
}

/// (pinned_actor_id, implementer_actor_id) of the card, which `Task` doesn't carry.
fn pin_of(db: &Db, t: &str) -> (Option<String>, Option<String>) {
    db.read(|c| Ok(c.query_row("SELECT pinned_actor_id, implementer_actor_id FROM tasks WHERE id=?1", [t], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap()
}

fn run(db: &Db, agent: &str, task: &str, role: &str) -> String {
    runs::create(db, agent, task, role, "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap()
}

fn out(o: &str) -> Outcome {
    Outcome { outcome: o.into(), summary: format!("summary for {o}"), issues: vec![] }
}

/// Neither the Backend Agent (on To do and In progress, Auto) nor the QA Agent (on Testing, Auto) would take the card.
fn no_agent_takes(db: &Db, t: &str, when: &str) {
    for a in ["Backend Agent", "QA Agent"] {
        let id = agent(db, a);
        assert!(!workflow::waiting_for(db, &id).unwrap().contains(&t.to_string()), "{a} would take it {when}");
        assert_ne!(workflow::next_task_for(db, &id).unwrap().as_deref(), Some(t), "{a} would start it {when}");
    }
}

fn err(r: gizai_core::Result<impl std::fmt::Debug>) -> String {
    r.unwrap_err().to_string()
}

#[test]
fn a_card_started_by_hand_is_pinned_to_you_and_no_agent_of_an_auto_column_takes_it() {
    let (db, s, t) = yours();
    // a card assigned to a person, in To do with the Backend Agent on it: the queue takes it, as before GA-111
    let be = agent(&db, "Backend Agent");
    assert!(workflow::waiting_for(&db, &be).unwrap().contains(&t), "a person as assignee alone doesn't keep agents off");
    assert!(!tasks::get(&db, &t).unwrap().by_hand);

    workflow::start_by_hand_check(&db, &s.you_id, &t).unwrap();
    assert_eq!(workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap(), "In progress", "moved on from To do");
    let task = tasks::get(&db, &t).unwrap();
    assert!(task.by_hand);
    assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref(), task.branch.as_deref()),
               ("In progress", Some(s.you_id.as_str()), Some("gizai/kade-1-csv")));
    assert_eq!(pin_of(&db, &t), (Some(s.you_id.clone()), Some(s.you_id.clone())), "pinned to you, you its implementer");
    no_agent_takes(&db, &t, "in In progress, Auto");
    // moved as you, as a run's start moves a card
    let moved = tasks::activity(&db, &t).unwrap();
    assert!(moved.iter().any(|e| e.actor_name.as_deref() == Some("Jeffrey") && e.diff["column"] == serde_json::json!(["To do", "In progress"])),
            "{moved:?}");

    // back in To do, Auto, it is still yours: no agent takes it there either
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "To do"), "a2").unwrap();
    assert!(tasks::get(&db, &t).unwrap().by_hand);
    no_agent_takes(&db, &t, "in To do, Auto");
    assert_eq!(serde_json::to_value(tasks::get(&db, &t).unwrap()).unwrap()["byHand"], serde_json::json!(true), "the UI reads byHand");
}

#[test]
fn opening_it_again_keeps_its_branch_and_changes_nothing() {
    let (db, s, t) = yours();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    let version = |db: &Db| db.read(|c| Ok(c.query_row("SELECT version FROM tasks WHERE id=?1", [&t], |r| r.get::<_, i64>(0))?)).unwrap();
    let before = version(&db);
    assert_eq!(workflow::start_by_hand(&db, &s.you_id, &t, "gizai/other").unwrap(), "In progress");
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.branch.as_deref(), task.by_hand), (Some("gizai/kade-1-csv"), true), "the branch it had stays");
    assert_eq!(version(&db), before, "nothing written again");
}

#[test]
fn starting_by_hand_takes_a_hold_off_with_fresh_tries() {
    let (db, s, t) = yours();
    tasks::update(&db, &s.you_id, &t, TaskPatch { hold: Some("stalled".into()), hold_reason: Some("No result three times".into()), ..Default::default() }).unwrap();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.hold, task.hold_reason, task.fail_count), (None, None, 0));
    no_agent_takes(&db, &t, "with its hold off");
}

#[test]
fn start_by_hand_is_refused_in_plain_sentences_and_changes_nothing() {
    let (db, s, t) = setup();
    let refused = |db: &Db, why: &str| {
        let check = err(workflow::start_by_hand_check(db, &s.you_id, &t));
        let start = err(workflow::start_by_hand(db, &s.you_id, &t, "gizai/kade-1-csv"));
        assert_eq!(check, start, "the check and the start say the same");
        assert!(start.contains(why), "{start}");
        let task = tasks::get(db, &t).unwrap();
        assert!(!task.by_hand, "{why}: not pinned");
        assert_eq!(pin_of(db, &t), (None, None), "{why}");
    };
    // assigned to nobody
    refused(&db, "KADE-1 isn't assigned to anyone: assign it to yourself to do it by hand");
    // assigned to an agent
    assign(&db, &s, &t, &agent(&db, "Backend Agent"));
    refused(&db, "KADE-1 is assigned to Backend Agent, an agent: assign it to yourself to do it by hand");
    // assigned to another person
    let sam = gizai_core::users::create(&db, &s.you_id, "Sam", None).unwrap();
    assign(&db, &s, &t, &sam);
    refused(&db, "KADE-1 is assigned to Sam: only a card assigned to you can be done by hand");
    // yours, while an agent works on it
    assign(&db, &s, &t, &s.you_id);
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    refused(&db, "Backend Agent is working on KADE-1: wait for its run to end");
    runs::finish(&db, &r, "cancelled", None, 0, 0, 0, None).unwrap();
    // in Done, then archived
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Done"), "a1").unwrap();
    refused(&db, "KADE-1 is in Done: move it back to work on it by hand");
    tasks::archive(&db, &s.you_id, &t).unwrap();
    refused(&db, "KADE-1 is archived: restore it first");
}

#[test]
fn hand_over_moves_it_to_testing_qa_takes_it_and_a_qa_fail_brings_it_back_to_you_still_not_taken_by_agents() {
    let (db, s, t) = yours();
    let (be, qa) = (agent(&db, "Backend Agent"), agent(&db, "QA Agent"));
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();

    assert_eq!(workflow::hand_over(&db, &s.you_id, &t, "  Handed over by Jeffrey: 1 commit.  ").unwrap(), "Testing");
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.state_name.as_str(), task.by_hand), ("Testing", false));
    assert_eq!(pin_of(&db, &t), (None, Some(s.you_id.clone())), "the pin is off; you stay its implementer");
    let notes = comments::list(&db, &t).unwrap();
    assert!(notes.iter().any(|c| c.author_name == "Jeffrey" && c.body_md == "Handed over by Jeffrey: 1 commit."), "{notes:?}");
    // QA takes it from Testing
    assert!(workflow::waiting_for(&db, &qa).unwrap().contains(&t));
    assert_eq!(workflow::next_task_for(&db, &qa).unwrap().as_deref(), Some(t.as_str()));
    assert_eq!(workflow::run_agent(&db, &t).unwrap().map(|a| a.1).as_deref(), Some("qa"));
    assert!(!workflow::waiting_for(&db, &be).unwrap().contains(&t));

    // a QA fail brings it back to you, pinned again
    let q = run(&db, &qa, &t, "qa");
    let g = workflow::apply_outcome(&db, &q, Some(&Outcome { outcome: "qa_fail".into(), summary: "One problem.".into(),
        issues: vec!["The CSV has no header".into()] })).unwrap();
    assert_eq!(g.moved_to.as_deref(), Some("In progress"));
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref(), task.by_hand, task.bounce_count),
               ("In progress", Some(s.you_id.as_str()), true, 1));
    assert_eq!(pin_of(&db, &t), (Some(s.you_id.clone()), Some(s.you_id.clone())));
    no_agent_takes(&db, &t, "after a QA fail");
    workflow::by_hand_check(&db, &s.you_id, &t, true).unwrap();
    assert!(comments::list(&db, &t).unwrap().iter().any(|c| c.author_name == "QA Agent" && c.body_md.contains("The CSV has no header")));

    // handed over again, QA passes it: Review, assigned to the person who reviews
    assert_eq!(workflow::hand_over(&db, &s.you_id, &t, "").unwrap(), "Testing");
    assert_eq!(comments::list(&db, &t).unwrap().iter().filter(|c| c.author_name == "Jeffrey").count(), 1, "no empty comment");
    let q = run(&db, &qa, &t, "qa");
    workflow::apply_outcome(&db, &q, Some(&out("qa_pass"))).unwrap();
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.state_name.as_str(), task.assignee_kind.as_deref(), task.by_hand), ("Review", Some("person"), false));
}

#[test]
fn with_its_testing_switch_off_hand_over_moves_it_to_review_assigned_to_the_reviewer() {
    let (db, s, t) = yours();
    tasks::update(&db, &s.you_id, &t, TaskPatch { testing: Some(false), ..Default::default() }).unwrap();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    assert_eq!(workflow::hand_over(&db, &s.you_id, &t, "Done by hand").unwrap(), "Review");
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref(), task.by_hand), ("Review", Some(s.you_id.as_str()), false));
    no_agent_takes(&db, &t, "in Review");
}

#[test]
fn hand_over_in_testing_or_backlog_leaves_it_there_with_the_pin_off() {
    let (db, s, t) = yours();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Testing"), "a1").unwrap();
    assert_eq!(workflow::hand_over(&db, &s.you_id, &t, "").unwrap(), "Testing");
    assert!(workflow::waiting_for(&db, &agent(&db, "QA Agent")).unwrap().contains(&t), "QA takes it in Testing");

    let (db, s, t) = yours();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Backlog"), "a1").unwrap();
    assert_eq!(workflow::hand_over(&db, &s.you_id, &t, "").unwrap(), "Backlog");
    assert_eq!(pin_of(&db, &t).0, None);
}

#[test]
fn hand_over_is_refused_for_a_card_that_is_not_yours_by_hand_and_while_an_agent_works_on_it() {
    let (db, s, t) = yours();
    let refused = |db: &Db, why: &str| {
        let e = err(workflow::hand_over(db, &s.you_id, &t, "x"));
        assert!(e.contains(why), "{e}");
        assert_eq!(err(workflow::by_hand_check(db, &s.you_id, &t, true)), e, "the check says the same");
    };
    // assigned to you but not started by hand
    refused(&db, "KADE-1 isn't yours to do by hand: open it in the editor first");
    // pinned to an agent
    tasks::update(&db, &s.you_id, &t, TaskPatch { pinned_actor_id: Some(agent(&db, "Backend Agent")), ..Default::default() }).unwrap();
    refused(&db, "KADE-1 isn't yours to do by hand: open it in the editor first");
    // pinned to another person
    let sam = gizai_core::users::create(&db, &s.you_id, "Sam", None).unwrap();
    tasks::update(&db, &s.you_id, &t, TaskPatch { pinned_actor_id: Some(sam), ..Default::default() }).unwrap();
    refused(&db, "KADE-1 is Sam's to do by hand");
    tasks::update(&db, &s.you_id, &t, TaskPatch { pinned_actor_id: Some(String::new()), ..Default::default() }).unwrap();

    // yours by hand, while an agent works on it (a person's Run): Hand over waits; the card bar may still read it
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    refused(&db, "Backend Agent is working on KADE-1: wait for its run to end");
    workflow::by_hand_check(&db, &s.you_id, &t, false).unwrap();
    assert_eq!(tasks::get(&db, &t).unwrap().state_name, "In progress", "not moved");
    runs::finish(&db, &r, "cancelled", None, 0, 0, 0, None).unwrap();
    workflow::by_hand_check(&db, &s.you_id, &t, true).unwrap();

    // archived
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Done"), "a1").unwrap();
    tasks::archive(&db, &s.you_id, &t).unwrap();
    refused(&db, "KADE-1 is archived: restore it first");
    assert!(err(workflow::by_hand_check(&db, &s.you_id, &t, false)).contains("archived"));
}

#[test]
fn a_builders_ready_for_testing_on_a_card_done_by_hand_takes_the_pin_off() {
    // a person's Run on a card they do by hand: the agent built it, so the next column's agents take it
    let (db, s, t) = yours();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    let (be, role) = workflow::run_agent(&db, &t).unwrap().unwrap();
    assert_eq!((be.as_str(), role.as_str()), (agent(&db, "Backend Agent").as_str(), "backend"));
    let r = run(&db, &be, &t, "backend");
    workflow::apply_outcome(&db, &r, Some(&out("ready_for_testing"))).unwrap();
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.state_name.as_str(), task.by_hand), ("Testing", false));
    assert_eq!(pin_of(&db, &t), (None, Some(be.clone())), "the agent is its implementer now");
    assert!(workflow::waiting_for(&db, &agent(&db, "QA Agent")).unwrap().contains(&t));
    // a QA fail then goes to the agent, not back to the person
    let q = run(&db, &agent(&db, "QA Agent"), &t, "qa");
    workflow::apply_outcome(&db, &q, Some(&out("qa_fail"))).unwrap();
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.assignee_id.as_deref(), task.by_hand), (Some(be.as_str()), false));
}

#[test]
fn the_board_check_leaves_a_card_done_by_hand_alone() {
    // a card done by hand whose last run (a person's Run) failed part-way: theirs to pick up, not a finding
    let (db, s, t) = yours();
    workflow::start_by_hand(&db, &s.you_id, &t, "gizai/kade-1-csv").unwrap();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    runs::finish(&db, &r, "failed", None, 0, 0, 0, Some("boom")).unwrap();
    let cx = board::Context { now: i64::MAX / 2, max_concurrent: 2, ..Default::default() };
    let found = board::check(&db, &cx).unwrap();
    assert!(found.iter().all(|f| f.task_id != t), "{found:?}");

    // the same card without the pin is a finding, as before
    tasks::update(&db, &s.you_id, &t, TaskPatch { pinned_actor_id: Some(String::new()), ..Default::default() }).unwrap();
    let found = board::check(&db, &cx).unwrap();
    assert!(found.iter().any(|f| f.task_id == t && f.code == "failed"), "{found:?}");
}
