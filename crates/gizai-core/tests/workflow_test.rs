use gizai_core::columns::{self, ColumnInput};
use gizai_core::{db::Db, model::*, projects, runs, seed::ensure_seed, tasks, workflow};

fn setup() -> (Db, gizai_core::seed::SeedIds, String) {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "CSV".into(), ..Default::default() }).unwrap();
    let be: String = db.read(|c| Ok(c.query_row("SELECT id FROM labels WHERE name='backend'", [], |r| r.get(0))?)).unwrap();
    tasks::set_labels(&db, &s.you_id, &t, vec![be]).unwrap();
    let todo = state(&db, "To do");
    tasks::move_to(&db, &s.you_id, &t, &todo, "a1").unwrap();
    // add_agent puts each on its role's usual columns: the Backend Agent on To do and In progress, the QA Agent on Testing
    for (name, role) in [("Backend Agent", "backend"), ("QA Agent", "qa")] {
        gizai_core::team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap();
    }
    (db, s, t)
}

fn state(db: &Db, name: &str) -> String {
    db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
}

fn agent(db: &Db, name: &str) -> String {
    db.read(|c| Ok(c.query_row("SELECT id FROM actors WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
}

fn run(db: &Db, agent: &str, task: &str, role: &str) -> String {
    runs::create(db, agent, task, role, "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap()
}

/// A run that really started, as the app does it: a card in To do moves on to In progress (`move_on_start`).
fn start(db: &Db, agent: &str, task: &str, role: &str) -> String {
    let r = run(db, agent, task, role);
    workflow::move_on_start(db, agent, task).unwrap();
    r
}

fn out(o: &str) -> Outcome {
    Outcome { outcome: o.into(), summary: format!("summary for {o}"), issues: vec![] }
}

#[test]
fn backend_then_qa_then_review() {
    let (db, _s, t) = setup();
    let (agent, role) = workflow::run_agent(&db, &t).unwrap().unwrap();
    assert_eq!(role, "backend");
    let r1 = start(&db, &agent, &t, &role);
    workflow::apply_outcome(&db, &r1, Some(&Outcome { outcome: "ready_for_testing".into(), summary: "done".into(), issues: vec![] })).unwrap();
    assert_eq!(tasks::get(&db, &t).unwrap().state_name, "Testing");
    let (qa, qrole) = workflow::run_agent(&db, &t).unwrap().unwrap();
    assert_eq!(qrole, "qa");
    let r2 = run(&db, &qa, &t, &qrole);
    workflow::apply_outcome(&db, &r2, Some(&Outcome { outcome: "qa_pass".into(), summary: "ok".into(), issues: vec![] })).unwrap();
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.state_name.as_str(), task.assignee_kind.as_deref()), ("Review", Some("person")));
    let notes = gizai_core::comments::list(&db, &t).unwrap();
    assert!(notes.iter().any(|c| c.author_name == "Backend Agent" && c.body_md.contains("done") && c.run_id.as_deref() == Some(r1.as_str())));
}

#[test]
fn three_bounces_put_the_card_on_hold() {
    let (db, _s, t) = setup();
    for _ in 0..3 {
        let (a, role) = workflow::run_agent(&db, &t).unwrap().unwrap();
        let r = start(&db, &a, &t, &role);
        workflow::apply_outcome(&db, &r, Some(&Outcome { outcome: "ready_for_testing".into(), summary: "".into(), issues: vec![] })).unwrap();
        let (qa, qrole) = workflow::run_agent(&db, &t).unwrap().unwrap();
        let q = start(&db, &qa, &t, &qrole);
        workflow::apply_outcome(&db, &q, Some(&Outcome { outcome: "qa_fail".into(), summary: "".into(), issues: vec!["1. broken".into()] })).unwrap();
    }
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.bounce_count, task.hold.as_deref()), (3, Some("needs_decision")));
    assert_eq!(task.hold_reason.as_deref(), Some("3 QA bounces"));
    for a in ["Backend Agent", "QA Agent"] {
        assert!(workflow::waiting_for(&db, &agent(&db, a)).unwrap().is_empty(), "held cards are not dispatched ({a})");
    }
}

#[test]
fn qa_fail_goes_back_to_the_implementer_with_the_issues() {
    let (db, _s, t) = setup();
    let be = agent(&db, "Backend Agent");
    let r = start(&db, &be, &t, "backend");
    workflow::apply_outcome(&db, &r, Some(&out("ready_for_testing"))).unwrap();
    let q = start(&db, &agent(&db, "QA Agent"), &t, "qa");
    let g = workflow::apply_outcome(&db, &q, Some(&Outcome { outcome: "qa_fail".into(), summary: "Two problems.".into(), issues: vec!["Pin overlaps the header".into()] })).unwrap();
    assert_eq!(g.moved_to.as_deref(), Some("In progress"));
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!(task.assignee_id.as_deref(), Some(be.as_str()));
    assert_eq!(workflow::run_agent(&db, &t).unwrap().unwrap().0, be);
    let notes = gizai_core::comments::list(&db, &t).unwrap();
    assert!(notes.iter().any(|c| c.author_name == "QA Agent" && c.body_md.contains("Pin overlaps the header")));
}

#[test]
fn no_result_three_times_stalls_and_needs_decision_holds() {
    let (db, _s, t) = setup();
    let be = agent(&db, "Backend Agent");
    for _ in 0..3 {
        let r = run(&db, &be, &t, "backend");
        workflow::apply_outcome(&db, &r, None).unwrap();
    }
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.fail_count, task.hold.as_deref()), (3, Some("stalled")));
    let (db, _s, t) = setup();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    let g = workflow::apply_outcome(&db, &r, Some(&out("needs_decision"))).unwrap();
    assert_eq!(g.hold.as_deref(), Some("needs_decision"));
}

#[test]
fn an_outcome_outside_the_role_holds_instead_of_moving() {
    let (db, _s, t) = setup();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    let g = workflow::apply_outcome(&db, &r, Some(&out("qa_pass"))).unwrap();
    assert_eq!((g.moved_to, g.hold.as_deref()), (None, Some("needs_decision")));
    assert_eq!(tasks::get(&db, &t).unwrap().state_name, "To do");
}

#[test]
fn a_card_moved_by_hand_during_the_run_stays_put() {
    let (db, s, t) = setup();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Backlog"), "").unwrap();
    let g = workflow::apply_outcome(&db, &r, Some(&out("ready_for_testing"))).unwrap();
    assert_eq!(g.moved_to, None);
    assert_eq!(tasks::get(&db, &t).unwrap().state_name, "Backlog");
}

#[test]
fn a_task_has_one_active_run_at_a_time() {
    let (db, _s, t) = setup();
    let be = agent(&db, "Backend Agent");
    let r = run(&db, &be, &t, "backend");
    let again = runs::create(&db, &be, &t, "backend", "S2", "/tmp", "/tmp", "gizai/x", "/tmp/r2.jsonl");
    assert!(again.unwrap_err().to_string().contains("already has an active run"));
    runs::finish(&db, &r, "succeeded", Some(&out("ready_for_testing")), 420_000, 38_000, 4_100, None).unwrap();
    let listed = &runs::list_for_task(&db, &t).unwrap()[0];
    assert_eq!((listed.status.as_str(), listed.outcome.as_deref(), listed.cost_usd_micros), ("succeeded", Some("ready_for_testing"), 420_000));
    assert!(runs::create(&db, &be, &t, "backend", "S3", "/tmp", "/tmp", "gizai/x", "/tmp/r3.jsonl").is_ok(), "finish released the claim");
}

#[test]
fn the_queue_finds_work_for_each_role() {
    let (db, s, t) = setup();
    let (be, qa) = (agent(&db, "Backend Agent"), agent(&db, "QA Agent"));
    assert_eq!(workflow::next_task_for(&db, &be).unwrap().as_deref(), Some(t.as_str()));
    assert_eq!(workflow::next_task_for(&db, &qa).unwrap(), None);
    let r = start(&db, &be, &t, "backend");
    assert_eq!(workflow::next_task_for(&db, &be).unwrap(), None, "claimed and busy");
    workflow::apply_outcome(&db, &r, Some(&out("ready_for_testing"))).unwrap();
    assert_eq!(workflow::next_task_for(&db, &qa).unwrap().as_deref(), Some(t.as_str()));
    tasks::update(&db, &s.you_id, &t, TaskPatch { hold: Some("blocked".into()), ..Default::default() }).unwrap();
    assert_eq!(workflow::next_task_for(&db, &qa).unwrap(), None, "held cards wait for a human");
}

#[test]
fn interrupted_runs_are_recovered_on_start() {
    let (db, _s, t) = setup();
    let (a, role) = workflow::run_agent(&db, &t).unwrap().unwrap();
    let r = run(&db, &a, &t, &role);
    runs::set_running(&db, &r, 4242).unwrap();
    assert_eq!(runs::recover_interrupted(&db).unwrap(), 1);
    let run = &runs::list_for_task(&db, &t).unwrap()[0];
    assert_eq!((run.status.as_str(), run.error.as_deref()), ("failed", Some("interrupted")));
    assert!(runs::create(&db, &a, &t, &role, "S2", "/tmp", "/tmp", "gizai/x", "/tmp/r2.jsonl").is_ok(), "the claim was released");
}

#[test]
fn a_column_must_name_real_agents_and_a_real_next_column() {
    let (db, s, _t) = setup();
    let testing = state(&db, "Testing");
    let bad = columns::set_column(&db, &s.you_id, &testing, ColumnInput { agent_ids: Some(vec!["nope".into()]), ..Default::default() });
    assert!(bad.is_err());
    let bad = columns::set_column(&db, &s.you_id, &testing, ColumnInput { next_state_id: Some("nope".into()), ..Default::default() });
    assert!(bad.is_err());
}

#[test]
fn a_card_dragged_into_in_progress_is_still_picked_up() {
    let (db, s, t) = setup();
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "In progress"), "").unwrap();
    assert_eq!(workflow::next_task_for(&db, &agent(&db, "Backend Agent")).unwrap().as_deref(), Some(t.as_str()));
}

#[test]
fn a_failed_run_keeps_its_error_outcome_after_the_gate() {
    let (db, _s, t) = setup();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    runs::finish(&db, &r, "failed", None, 0, 0, 0, Some("exit code 1")).unwrap();
    workflow::apply_outcome(&db, &r, None).unwrap();
    let listed = &runs::list_for_task(&db, &t).unwrap()[0];
    assert_eq!((listed.status.as_str(), listed.outcome.as_deref(), listed.error.as_deref()), ("failed", Some("error"), Some("exit code 1")));
}

#[test]
fn a_testing_card_goes_to_the_qa_agent_never_the_builder_and_review_is_the_human_gate() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "CSV".into(), ..Default::default() }).unwrap();
    let be: String = db.read(|c| Ok(c.query_row("SELECT id FROM labels WHERE name='backend'", [], |r| r.get(0))?)).unwrap();
    tasks::set_labels(&db, &s.you_id, &t, vec![be]).unwrap();
    // the Backend Agent goes on To do and In progress; the card's backend label routes nothing
    gizai_core::team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Testing"), "").unwrap();
    assert_eq!(workflow::run_agent(&db, &t).unwrap(), None, "no QA agent: nobody, and never the builder");
    assert_eq!(workflow::next_task_for(&db, &agent(&db, "Backend Agent")).unwrap(), None, "never the builder");
    gizai_core::team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    let qa = agent(&db, "QA Agent");
    assert_eq!(workflow::run_agent(&db, &t).unwrap(), Some((qa.clone(), "qa".into())), "a new QA agent goes on Testing");
    assert_eq!(workflow::next_task_for(&db, &qa).unwrap().as_deref(), Some(t.as_str()));
    tasks::move_to(&db, &s.you_id, &t, &state(&db, "Review"), "").unwrap();
    assert_eq!(workflow::run_agent(&db, &t).unwrap(), None, "Review is the human gate");
}

#[test]
fn qa_issue_comments_keep_leading_numbers() {
    let (db, _s, t) = setup();
    let r = run(&db, &agent(&db, "Backend Agent"), &t, "backend");
    workflow::apply_outcome(&db, &r, Some(&out("ready_for_testing"))).unwrap();
    let q = run(&db, &agent(&db, "QA Agent"), &t, "qa");
    workflow::apply_outcome(&db, &q, Some(&Outcome { outcome: "qa_fail".into(), summary: "".into(), issues: vec!["404 on /login".into(), "1. 3 tests fail".into()] })).unwrap();
    let body = gizai_core::comments::list(&db, &t).unwrap().into_iter().find(|c| c.author_name == "QA Agent").unwrap().body_md;
    assert_eq!(body, "1. 404 on /login\n2. 3 tests fail");
}

#[test]
fn a_stalled_card_says_what_went_wrong_last() {
    let (db, _s, t) = setup();
    let be = agent(&db, "Backend Agent");
    for err in ["model not found", "model not found", "stopped at the limit of 80 tool calls per run"] {
        let r = run(&db, &be, &t, "backend");
        runs::finish(&db, &r, "timed_out", None, 0, 0, 0, Some(err)).unwrap();
        workflow::apply_outcome(&db, &r, None).unwrap();
    }
    let task = tasks::get(&db, &t).unwrap();
    let reason = task.hold_reason.unwrap_or_default();
    assert!(reason.contains("3 runs ended without a result") && reason.contains("80 tool calls"), "{reason}");
}

#[test]
fn clearing_the_hold_gives_the_card_fresh_tries() {
    let (db, s, t) = setup();
    let be = agent(&db, "Backend Agent");
    for _ in 0..3 {
        let r = run(&db, &be, &t, "backend");
        workflow::apply_outcome(&db, &r, None).unwrap();
    }
    tasks::update(&db, &s.you_id, &t, TaskPatch { hold: Some("".into()), ..Default::default() }).unwrap();
    assert_eq!(tasks::get(&db, &t).unwrap().fail_count, 0);
    let r = run(&db, &be, &t, "backend");
    workflow::apply_outcome(&db, &r, None).unwrap();
    let task = tasks::get(&db, &t).unwrap();
    assert_eq!((task.fail_count, task.hold.as_deref()), (1, None), "one more failure doesn't stall it again");
}
