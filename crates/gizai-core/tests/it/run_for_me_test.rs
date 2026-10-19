//! GA-31, "Run this for me": a `needs_decision` result may name commands the agent asks the user to run. The verdict
//! keeps them (`Run::run_for_me`), the held card shows them (`Task::run_for_me`), also when the push after the run
//! failed, and the board check's held finding carries them. Any other outcome drops them. Gizai's nudge reads as its
//! own trigger, `result_nudge`, apart from a Continue.
use gizai_core::board::{self, Context};
use gizai_core::{db::Db, model::*, projects, runs, seed::ensure_seed, tasks, workflow};

const CMDS: [&str; 2] = ["sudo pacman -S libayatana-appindicator",
                         "echo \"fs.inotify.max_user_watches=524288\" | sudo tee /etc/sysctl.d/40-watches.conf && sudo sysctl --system"];

fn cmds() -> Vec<String> {
    CMDS.iter().map(|c| c.to_string()).collect()
}

struct S {
    db: Db,
    you: String,
    task: String,
    agent: String,
}

/// Card KADE-1 in To do, with the Backend Agent on the column.
fn setup() -> S {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let task = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "Tray icon".into(), ..Default::default() }).unwrap();
    let todo: String = db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name='To do'", [], |r| r.get(0))?)).unwrap();
    tasks::move_to(&db, &s.you_id, &task, &todo, "a1").unwrap();
    let agent = gizai_core::team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(),
        ..Default::default() }).unwrap();
    S { db, you: s.you_id, task, agent }
}

impl S {
    /// A run that started on the card (it moves on to In progress), as the app does it.
    fn start(&self) -> String {
        let r = runs::create(&self.db, &self.agent, &self.task, "backend", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/r.jsonl").unwrap();
        workflow::move_on_start(&self.db, &self.agent, &self.task).unwrap();
        r
    }
    fn task(&self) -> Task {
        tasks::get(&self.db, &self.task).unwrap()
    }
    fn outcome_json(&self, run: &str) -> Option<String> {
        self.db.read(|c| Ok(c.query_row("SELECT outcome_json FROM runs WHERE id=?1", [run], |r| r.get(0))?)).unwrap()
    }
}

fn verdict(outcome: &str, summary: &str) -> Outcome {
    Outcome { outcome: outcome.into(), summary: summary.into(), issues: vec![] }
}

#[test]
fn a_needs_decision_with_commands_holds_the_card_and_both_the_run_and_the_card_show_them_exactly() {
    let s = setup();
    let r = s.start();
    let g = workflow::apply_outcome_with(&s.db, &r, Some(&verdict("needs_decision", "The tray needs a system library.")), &cmds()).unwrap();
    assert_eq!((g.hold.as_deref(), g.moved_to.as_deref()), (Some("needs_decision"), None));
    let run = runs::get(&s.db, &r).unwrap();
    assert_eq!(run.run_for_me, CMDS, "exactly as the agent wrote them, in order");
    assert_eq!((run.outcome.as_deref(), run.status.as_str()), (Some("needs_decision"), "succeeded"));
    let json: serde_json::Value = serde_json::from_str(&s.outcome_json(&r).unwrap()).unwrap();
    assert_eq!(json["run_for_me"], serde_json::json!(CMDS), "kept with the verdict");
    assert_eq!(json["summary"], "The tray needs a system library.");

    let t = s.task();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.hold_reason.as_deref()), ("In progress", Some("needs_decision"), Some("The tray needs a system library.")));
    assert_eq!(t.run_for_me, CMDS);
    // the same in the list the Tasks page and the Inbox load, and the card is in your Inbox
    let listed = tasks::list(&s.db, &TaskFilter::default()).unwrap().into_iter().find(|x| x.id == s.task).unwrap();
    assert_eq!(listed.run_for_me, CMDS);
    assert!(tasks::needs_you(&s.db, &s.you).unwrap().iter().any(|x| x.id == s.task && x.run_for_me == CMDS));
    // and the UI gets it as runForMe
    let v = serde_json::to_value(&t).unwrap();
    assert_eq!(v["runForMe"], serde_json::json!(CMDS));
    assert_eq!(serde_json::to_value(&run).unwrap()["runForMe"], serde_json::json!(CMDS));
}

#[test]
fn without_a_summary_the_hold_says_the_agent_asks_you_to_run_commands() {
    let s = setup();
    let r = s.start();
    workflow::apply_outcome_with(&s.db, &r, Some(&verdict("needs_decision", "  ")), &cmds()[..1]).unwrap();
    let t = s.task();
    assert_eq!((t.hold.as_deref(), t.hold_reason.as_deref()), (Some("needs_decision"), Some("The agent asks you to run commands for it")));
    assert_eq!(t.run_for_me, &CMDS[..1]);
    // a needs_decision without commands keeps its usual reason
    let s = setup();
    let r = s.start();
    workflow::apply_outcome_with(&s.db, &r, Some(&verdict("needs_decision", "")), &[]).unwrap();
    assert_eq!(s.task().hold_reason.as_deref(), Some("The agent needs a decision"));
}

#[test]
fn another_outcome_or_an_older_result_line_has_no_commands() {
    for o in ["ready_for_testing", "qa_pass", "qa_fail", "deployed"] {
        let s = setup();
        let r = s.start();
        workflow::apply_outcome_with(&s.db, &r, Some(&verdict(o, "done")), &cmds()).unwrap();
        assert!(runs::get(&s.db, &r).unwrap().run_for_me.is_empty(), "{o}");
        assert!(!s.outcome_json(&r).unwrap().contains("run_for_me"), "{o}: dropped");
        assert!(s.task().run_for_me.is_empty(), "{o}");
    }
    // an older needs_decision (apply_outcome, no commands): held as before, nothing to run
    let s = setup();
    let r = s.start();
    workflow::apply_outcome(&s.db, &r, Some(&verdict("needs_decision", "CSV or JSON?"))).unwrap();
    let t = s.task();
    assert_eq!((t.hold.as_deref(), t.hold_reason.as_deref()), (Some("needs_decision"), Some("CSV or JSON?")));
    assert!(t.run_for_me.is_empty() && runs::get(&s.db, &r).unwrap().run_for_me.is_empty());
    assert!(!s.outcome_json(&r).unwrap().contains("run_for_me"));
    // a run without a result
    let s = setup();
    let r = s.start();
    runs::finish(&s.db, &r, "succeeded", None, 0, 0, 0, None).unwrap();
    workflow::apply_outcome_with(&s.db, &r, None, &cmds()).unwrap();
    assert!(runs::get(&s.db, &r).unwrap().run_for_me.is_empty() && s.task().run_for_me.is_empty());
}

#[test]
fn the_card_shows_the_commands_only_while_it_is_held_and_only_from_its_latest_run() {
    let s = setup();
    let r = s.start();
    workflow::apply_outcome_with(&s.db, &r, Some(&verdict("needs_decision", "Needs sudo.")), &cmds()).unwrap();
    assert_eq!(s.task().run_for_me, CMDS);
    // the hold is cleared (by hand, or as Done, continue starts the run): nothing to run any more, the run keeps them
    tasks::update(&s.db, &s.you, &s.task, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    let t = s.task();
    assert_eq!(t.hold, None);
    assert!(t.run_for_me.is_empty(), "{:?}", t.run_for_me);
    assert_eq!(runs::get(&s.db, &r).unwrap().run_for_me, CMDS, "the run still says what it asked");
    // the next run on the card asks nothing and is held for another reason: its commands, none
    let r2 = s.start();
    workflow::apply_outcome_with(&s.db, &r2, Some(&verdict("needs_decision", "CSV or JSON?")), &[]).unwrap();
    let t = s.task();
    assert_eq!(t.hold.as_deref(), Some("needs_decision"));
    assert!(t.run_for_me.is_empty(), "only the latest run's: {:?}", t.run_for_me);
}

#[test]
fn a_failed_push_after_the_run_holds_the_card_blocked_and_the_commands_still_show() {
    let s = setup();
    let r = s.start();
    let g = workflow::hold_unpushed_with(&s.db, &r, Some(&verdict("needs_decision", "Needs sudo.")), &cmds(),
                                         "Couldn't push gizai/kade-1: no access").unwrap();
    assert_eq!(g.hold.as_deref(), Some("blocked"));
    let t = s.task();
    assert_eq!((t.hold.as_deref(), t.hold_reason.as_deref()), (Some("blocked"), Some("Couldn't push gizai/kade-1: no access")));
    assert_eq!(t.run_for_me, CMDS, "the push's hold doesn't lose them");
    let run = runs::get(&s.db, &r).unwrap();
    assert_eq!((run.outcome.as_deref(), run.run_for_me.clone()), (Some("needs_decision"), cmds()));
    // another outcome with a failed push: nothing to run
    let s = setup();
    let r = s.start();
    workflow::hold_unpushed_with(&s.db, &r, Some(&verdict("ready_for_testing", "Done.")), &cmds(), "no access").unwrap();
    assert!(s.task().run_for_me.is_empty() && runs::get(&s.db, &r).unwrap().run_for_me.is_empty());
}

#[test]
fn the_board_checks_held_finding_carries_the_commands_and_a_nudge_reads_as_result_nudge() {
    let s = setup();
    let r = s.start();
    workflow::apply_outcome_with(&s.db, &r, Some(&verdict("needs_decision", "Needs sudo.")), &cmds()).unwrap();
    let cx = Context { now: gizai_core::ids::now_ms(), max_concurrent: 3, ..Default::default() };
    let found = board::check(&s.db, &cx).unwrap();
    let f = found.iter().find(|f| f.task_id == s.task).unwrap_or_else(|| panic!("{found:?}"));
    assert_eq!((f.kind.as_str(), f.hold.as_deref()), ("held", Some("needs_decision")));
    assert_eq!(f.run_for_me, CMDS);
    assert_eq!(serde_json::to_value(f).unwrap()["runForMe"], serde_json::json!(CMDS));

    // a card held for something else: none
    let s = setup();
    let r = s.start();
    workflow::apply_outcome_with(&s.db, &r, Some(&verdict("needs_decision", "CSV or JSON?")), &[]).unwrap();
    let found = board::check(&s.db, &Context { now: gizai_core::ids::now_ms(), max_concurrent: 3, ..Default::default() }).unwrap();
    assert!(found.iter().find(|f| f.task_id == s.task).unwrap().run_for_me.is_empty());

    // Gizai's nudge after a run without a result: the finding's last run says result_nudge, a Continue says nudge
    let s = setup();
    let r = s.start();
    runs::finish(&s.db, &r, "succeeded", None, 0, 0, 0, None).unwrap();
    workflow::apply_outcome(&s.db, &r, None).unwrap();
    let n = runs::create_nudge(&s.db, &s.agent, &s.task, "backend", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/n.jsonl").unwrap();
    runs::finish(&s.db, &n, "succeeded", None, 0, 0, 0, None).unwrap();
    workflow::apply_outcome(&s.db, &n, None).unwrap();
    let found = board::check(&s.db, &Context { now: gizai_core::ids::now_ms(), max_concurrent: 3, ..Default::default() }).unwrap();
    let f = found.iter().find(|f| f.task_id == s.task).unwrap();
    assert_eq!((f.hold.as_deref(), f.last_run.as_ref().map(|l| l.trigger.as_str())), (Some("stalled"), Some("result_nudge")));
    // stored as a nudged nudge, so no migration: the runs table's CHECK still takes it
    let stored: (String, i64) = s.db.read(|c| Ok(c.query_row("SELECT trigger, nudged FROM runs WHERE id=?1", [&n], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!(stored, ("nudge".to_string(), 1));
    let listed = runs::list_for_task(&s.db, &s.task).unwrap();
    assert_eq!(listed.iter().map(|r| (r.trigger.as_str(), r.nudged)).collect::<Vec<_>>(), [("result_nudge", true), ("routed", false)]);

    // a person's Continue on the stalled card reads as a Continue
    tasks::update(&s.db, &s.you, &s.task, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    let c = runs::create_with_trigger(&s.db, &s.agent, &s.task, "backend", "nudge", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/c.jsonl").unwrap();
    let run = runs::get(&s.db, &c).unwrap();
    assert_eq!((run.trigger.as_str(), run.nudged), ("nudge", false));
}
