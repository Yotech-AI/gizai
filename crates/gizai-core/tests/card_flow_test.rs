//! GA-32, the card flow: Backlog → To do → In progress → Testing → Review → Deploy → Done. To do is a queue by priority,
//! a start moves the card to In progress, the Testing switch, DevOps runs that never go to QA, the Deploy column (in the
//! seed since GA-49, Manual: only Run starts its cards) and its `deployed` outcome, a merge that moves the card to Deploy,
//! and migration 0007.
use gizai_core::columns::{self, ColumnInput};
use gizai_core::{db::Db, model::*, projects, pulls, runs, seed::ensure_seed, tasks, team, workflow};
use serde_json::json;

struct Board { db: Db, you: String, team: String, project: String, be: String, qa: String, ops: String }

/// The default board (its Deploy column after Review, Manual), with a Backend, a QA and a DevOps Agent on their role's
/// usual columns: the Backend Agent on To do and In progress, the QA Agent on Testing, the DevOps Agent on Deploy.
fn board() -> Board {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let mut ids = vec![];
    for (name, role) in [("Backend Agent", "backend"), ("QA Agent", "qa"), ("DevOps Agent", "devops")] {
        ids.push(team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap());
    }
    Board { db, you: s.you_id, team: s.team_id, project, be: ids[0].clone(), qa: ids[1].clone(), ops: ids[2].clone() }
}

/// Like `board`, with its Deploy column removed (Review then links to Done).
fn bare_board() -> Board {
    let b = board();
    columns::remove_state(&b.db, &b.you, &b.state("Deploy"), &b.state("Review")).unwrap();
    b
}

impl Board {
    fn state(&self, name: &str) -> String {
        self.db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1 AND deleted_at IS NULL", [name], |r| r.get(0))?)).unwrap()
    }
    fn label(&self, name: &str) -> String {
        self.db.read(|c| Ok(c.query_row("SELECT id FROM labels WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
    }
    /// A backend card in `column` with `priority`.
    fn card(&self, column: &str, priority: i64) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: self.project.clone(), title: format!("Card p{priority}"),
            state_id: Some(self.state(column)), priority, label_ids: vec![self.label("backend")], ..Default::default() }).unwrap()
    }
    fn to(&self, task: &str, column: &str) {
        tasks::move_to(&self.db, &self.you, task, &self.state(column), "").unwrap();
    }
    fn patch(&self, task: &str, patch: TaskPatch) {
        tasks::update(&self.db, &self.you, task, patch).unwrap();
    }
    fn run(&self, agent: &str, task: &str, role: &str) -> String {
        runs::create(&self.db, agent, task, role, "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap()
    }
    /// A run of `agent` on `task` that ends with `outcome` (None: without a result).
    fn finish(&self, agent: &str, task: &str, role: &str, outcome: Option<&str>) -> workflow::GateResult {
        let r = self.run(agent, task, role);
        let o = outcome.map(|o| Outcome { outcome: o.into(), summary: format!("summary for {o}"), issues: vec![] });
        workflow::apply_outcome(&self.db, &r, o.as_ref()).unwrap()
    }
    fn task(&self, id: &str) -> Task { tasks::get(&self.db, id).unwrap() }
    fn column(&self, id: &str) -> String { self.task(id).state_name }
    fn implementer(&self, id: &str) -> Option<String> {
        self.db.read(|c| Ok(c.query_row("SELECT implementer_actor_id FROM tasks WHERE id=?1", [id], |r| r.get(0))?)).unwrap()
    }
    fn columns(&self) -> Vec<String> {
        team::get(&self.db, &self.team).unwrap().states.iter().map(|s| s.name.clone()).collect()
    }
}

// ---- Part 1: the queue and the start ----

#[test]
fn the_queue_takes_cards_by_priority_with_none_last_then_assigned_then_board_order() {
    let b = board();
    // Dragged in this order; the queue goes by priority, not by the order they came in.
    let none = b.card("To do", 0);
    let low = b.card("To do", 4);
    let urgent = b.card("To do", 1);
    let medium = b.card("To do", 3);
    let high_first = b.card("To do", 2);
    let high_second = b.card("To do", 2);
    let medium_assigned = b.card("To do", 3);
    b.patch(&medium_assigned, TaskPatch { assignee_id: Some(b.be.clone()), ..Default::default() });
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [urgent.clone(), high_first.clone(), high_second.clone(), medium_assigned.clone(), medium, low, none]);
    assert_eq!(workflow::next_task_for(&b.db, &b.be).unwrap().as_deref(), Some(urgent.as_str()), "the queue and the Agent page agree");
    // Board order within a priority: the card at the top of the column first.
    tasks::move_to(&b.db, &b.you, &high_second, &b.state("To do"), "Zz").unwrap();
    let order = workflow::waiting_for(&b.db, &b.be).unwrap();
    assert_eq!(order[1..3], [high_second, high_first], "{order:?}");
}

#[test]
fn qa_takes_its_cards_from_testing_by_priority() {
    let b = board();
    let low = b.card("Testing", 4);
    let high = b.card("Testing", 2);
    let todo = b.card("To do", 1);
    assert_eq!(workflow::waiting_for(&b.db, &b.qa).unwrap(), [high, low]);
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [todo], "the builder never takes Testing cards");
}

#[test]
fn the_queue_skips_held_stopped_review_and_deploy_cards_and_cards_of_another_agent() {
    let b = board();
    let waiting = b.card("To do", 3);
    let held = b.card("To do", 1);
    b.patch(&held, TaskPatch { hold: Some("blocked".into()), ..Default::default() });
    let other_agents = b.card("To do", 1);
    b.patch(&other_agents, TaskPatch { assignee_id: Some(b.qa.clone()), ..Default::default() });
    let review = b.card("Review", 1);
    let deploy = b.card("Deploy", 1);
    b.patch(&deploy, TaskPatch { assignee_id: Some(b.be.clone()), ..Default::default() });
    // a person's assignment is the review's: a card dragged back from Review is picked up again
    let yours = b.card("To do", 2);
    b.patch(&yours, TaskPatch { assignee_id: Some(b.you.clone()), ..Default::default() });
    // the last run was stopped: only Run or Continue starts it again
    let stopped = b.card("To do", 1);
    let r = b.run(&b.be, &stopped, "backend");
    runs::finish(&b.db, &r, "cancelled", None, 0, 0, 0, Some("Stopped")).unwrap();
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap(), [yours.clone(), waiting.clone()]);
    assert_eq!(workflow::waiting_for(&b.db, &b.qa).unwrap(), [other_agents], "the card assigned to QA is QA's; {review} is nobody's");
    // a stopped card that ran again and failed is waiting again
    std::thread::sleep(std::time::Duration::from_millis(5)); // runs are told apart by when they were made
    let r = b.run(&b.be, &stopped, "backend");
    runs::finish(&b.db, &r, "failed", None, 0, 0, 0, Some("exit 1")).unwrap();
    assert_eq!(workflow::waiting_for(&b.db, &b.be).unwrap()[0], stopped);
}

#[test]
fn a_start_moves_a_card_from_to_do_or_backlog_to_in_progress_as_the_agent() {
    let b = board();
    let todo = b.card("To do", 0);
    let first = b.card("In progress", 0);
    assert_eq!(workflow::move_on_start(&b.db, &b.be, &todo).unwrap(), Some(b.state("To do")));
    let t = b.task(&todo);
    assert_eq!((t.state_name.as_str(), t.state_category.as_str()), ("In progress", "in_progress"));
    assert!(t.sort_key > b.task(&first).sort_key, "appended to the column");
    let a = tasks::activity(&b.db, &todo).unwrap();
    assert!(a.iter().any(|e| e.actor_name.as_deref() == Some("Backend Agent") && e.diff == json!({"column": ["To do", "In progress"]})), "{a:?}");
    // a person pressed Run on a Backlog card
    let backlog = b.card("Backlog", 0);
    assert_eq!(workflow::move_on_start(&b.db, &b.ops, &backlog).unwrap(), Some(b.state("Backlog")));
    assert_eq!(b.column(&backlog), "In progress");
    // put back after a start that couldn't work
    workflow::put_back(&b.db, &b.be, &todo, &b.state("To do")).unwrap();
    assert_eq!(b.column(&todo), "To do");
}

#[test]
fn a_start_leaves_cards_in_other_columns_where_they_are() {
    let b = board();
    team::add_state(&b.db, &b.you, &b.team, "Cancelled", &b.state("Done"), "cancelled").unwrap();
    for column in ["In progress", "Testing", "Review", "Deploy", "Done", "Cancelled"] {
        let t = b.card(column, 0);
        assert_eq!(workflow::move_on_start(&b.db, &b.ops, &t).unwrap(), None, "{column}");
        assert_eq!(b.column(&t), column);
    }
}

#[test]
fn a_start_that_cannot_work_holds_the_card_blocked_without_a_failure() {
    let b = board();
    let t = b.card("To do", 2);
    let r = b.run(&b.be, &t, "backend");
    workflow::hold_unstarted(&b.db, &r, "Claude Code has no model called opuss").unwrap();
    let task = b.task(&t);
    assert_eq!((task.state_name.as_str(), task.hold.as_deref(), task.hold_reason.as_deref(), task.fail_count),
               ("To do", Some("blocked"), Some("Claude Code has no model called opuss"), 0));
    assert_eq!(runs::get(&b.db, &r).unwrap().outcome.as_deref(), Some("no_result"));
    assert!(runs::create(&b.db, &b.be, &t, "backend", "S2", "/tmp", "/tmp", "gizai/x", "/tmp/r2.jsonl").is_ok(), "its claim is released");
    assert!(workflow::waiting_for(&b.db, &b.be).unwrap().is_empty(), "a held card waits for a person");
}

#[test]
fn a_run_without_a_result_leaves_the_card_in_progress_with_the_usual_fail_count() {
    let b = board();
    let t = b.card("To do", 0);
    workflow::move_on_start(&b.db, &b.be, &t).unwrap();
    b.finish(&b.be, &t, "backend", None);
    let task = b.task(&t);
    assert_eq!((task.state_name.as_str(), task.fail_count, task.hold), ("In progress", 1, None));
}

// ---- Part 2: the Testing switch ----

#[test]
fn every_card_is_tested_unless_its_switch_is_off() {
    let b = board();
    let on = b.card("To do", 0);
    assert!(b.task(&on).testing, "on by default");
    let off = tasks::create(&b.db, &b.you, TaskInput { project_id: b.project.clone(), title: "Fix a typo".into(), testing: Some(false), ..Default::default() }).unwrap();
    assert!(!b.task(&off).testing);
    b.patch(&off, TaskPatch { testing: Some(true), ..Default::default() });
    assert!(b.task(&off).testing);
    b.patch(&off, TaskPatch { title: Some("Fix two typos".into()), ..Default::default() });
    assert!(b.task(&off).testing, "a patch without the switch leaves it");
    b.patch(&off, TaskPatch { testing: Some(false), ..Default::default() });
    assert!(!b.task(&off).testing);
}

#[test]
fn with_testing_on_a_finished_card_goes_to_testing_for_qa() {
    let b = board();
    let t = b.card("In progress", 0);
    let g = b.finish(&b.be, &t, "backend", Some("ready_for_testing"));
    assert_eq!(g.moved_to.as_deref(), Some("Testing"));
    let task = b.task(&t);
    assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref()), ("Testing", None));
    assert_eq!(b.implementer(&t).as_deref(), Some(b.be.as_str()));
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap().map(|(a, _)| a).as_deref(), Some(b.qa.as_str()));
    assert_eq!(workflow::waiting_for(&b.db, &b.qa).unwrap(), [t], "the QA Agent on Testing picks it up");
}

#[test]
fn with_testing_off_a_finished_card_goes_straight_to_review_for_its_person() {
    let b = board();
    let t = b.card("In progress", 0);
    b.patch(&t, TaskPatch { testing: Some(false), ..Default::default() });
    let g = b.finish(&b.be, &t, "backend", Some("ready_for_testing"));
    assert_eq!((g.moved_to.as_deref(), g.hold), (Some("Review"), None));
    let task = b.task(&t);
    assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref(), task.assignee_kind.as_deref()),
               ("Review", Some(b.you.as_str()), Some("person")));
    assert_eq!(b.implementer(&t).as_deref(), Some(b.be.as_str()), "the agent still built it");
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), None, "no QA run");
    assert!(workflow::waiting_for(&b.db, &b.qa).unwrap().is_empty());
    let run = &runs::list_for_task(&b.db, &t).unwrap()[0];
    assert_eq!(run.outcome.as_deref(), Some("ready_for_testing"), "the outcome stays");
    let notes = gizai_core::comments::list(&b.db, &t).unwrap();
    assert!(notes.iter().any(|c| c.author_name == "Backend Agent" && c.body_md == "summary for ready_for_testing"));
    assert!(tasks::needs_you(&b.db, &b.you).unwrap().iter().any(|x| x.id == t), "in the inbox");
}

#[test]
fn the_switch_does_not_change_qa_fail_or_a_card_dragged_into_testing() {
    let b = board();
    let t = b.card("In progress", 0);
    b.patch(&t, TaskPatch { testing: Some(false), ..Default::default() });
    // a person drags it into Testing: Run starts QA on it as usual (the queue skips it: its switch is off, GA-49)
    b.to(&t, "Testing");
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap().map(|(a, _)| a).as_deref(), Some(b.qa.as_str()));
    assert!(!workflow::waiting_for(&b.db, &b.qa).unwrap().contains(&t));
    b.db.read(|c| Ok(c.execute("UPDATE tasks SET implementer_actor_id=?2 WHERE id=?1", [&t, &b.be])?)).unwrap();
    let g = b.finish(&b.qa, &t, "qa", Some("qa_fail"));
    assert_eq!(g.moved_to.as_deref(), Some("In progress"));
    assert_eq!((b.task(&t).assignee_id, b.task(&t).bounce_count), (Some(b.be.clone()), 1), "back to its builder, as always");
    // its next finish follows the switch as it is then
    let g = b.finish(&b.be, &t, "backend", Some("ready_for_testing"));
    assert_eq!(g.moved_to.as_deref(), Some("Review"));
    // and qa_pass stops at Review, never Deploy
    let t2 = b.card("Testing", 0);
    let g = b.finish(&b.qa, &t2, "qa", Some("qa_pass"));
    assert_eq!((g.moved_to.as_deref(), b.column(&t2).as_str()), (Some("Review"), "Review"));
}

#[test]
fn a_person_moving_a_testing_off_card_during_the_run_keeps_it_there() {
    let b = board();
    for column in ["Backlog", "Done"] {
        let t = b.card("In progress", 0);
        b.patch(&t, TaskPatch { testing: Some(false), ..Default::default() });
        let r = b.run(&b.be, &t, "backend");
        b.to(&t, column);
        let o = Outcome { outcome: "ready_for_testing".into(), summary: "done".into(), issues: vec![] };
        let g = workflow::apply_outcome(&b.db, &r, Some(&o)).unwrap();
        assert_eq!((g.moved_to, b.column(&t)), (None, column.to_string()));
    }
}

// ---- 3.4: a DevOps run never sends a card to QA ----

#[test]
fn a_devops_ready_for_testing_ends_in_review_for_its_person_with_testing_on_or_off() {
    let b = board();
    for testing in [true, false] {
        for column in ["To do", "In progress", "Testing", "Review"] {
            let t = b.card("In progress", 0);
            b.finish(&b.be, &t, "backend", Some("ready_for_testing")); // the Backend Agent built it
            b.to(&t, column);
            b.patch(&t, TaskPatch { testing: Some(testing), assignee_id: Some(b.ops.clone()), ..Default::default() });
            let g = b.finish(&b.ops, &t, "devops", Some("ready_for_testing"));
            let task = b.task(&t);
            let why = format!("{column}, testing {testing}");
            assert_eq!((g.moved_to.as_deref(), g.hold.as_deref()), (Some("Review"), None), "{why}");
            assert_eq!((task.state_name.as_str(), task.assignee_id.as_deref()), ("Review", Some(b.you.as_str())), "{why}");
            assert_eq!(b.implementer(&t).as_deref(), Some(b.be.as_str()), "the implementer stays: {why}");
            assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), None, "no QA run: {why}");
            assert!(!workflow::waiting_for(&b.db, &b.qa).unwrap().contains(&t), "{why}");
            assert_eq!(runs::list_for_task(&b.db, &t).unwrap()[0].outcome.as_deref(), Some("ready_for_testing"));
            // a later qa_fail goes back to the agent that built it
            b.to(&t, "Testing");
            b.finish(&b.qa, &t, "qa", Some("qa_fail"));
            assert_eq!(b.task(&t).assignee_id.as_deref(), Some(b.be.as_str()), "{why}");
        }
    }
}

#[test]
fn a_devops_run_on_a_card_moved_to_backlog_done_or_cancelled_leaves_it_there() {
    let b = board();
    team::add_state(&b.db, &b.you, &b.team, "Cancelled", &b.state("Done"), "cancelled").unwrap();
    for column in ["Backlog", "Done", "Cancelled"] {
        let t = b.card("In progress", 0);
        let r = b.run(&b.ops, &t, "devops");
        b.to(&t, column);
        let o = Outcome { outcome: "ready_for_testing".into(), summary: "merge conflicts fixed".into(), issues: vec![] };
        let g = workflow::apply_outcome(&b.db, &r, Some(&o)).unwrap();
        assert_eq!((g.moved_to, g.hold, b.column(&t)), (None, None, column.to_string()));
        assert_eq!(b.implementer(&t), None);
    }
}

#[test]
fn a_devops_needs_decision_holds_the_card_where_it_is() {
    let b = board();
    let t = b.card("Review", 0);
    let g = b.finish(&b.ops, &t, "devops", Some("needs_decision"));
    assert_eq!((g.hold.as_deref(), b.column(&t).as_str()), (Some("needs_decision"), "Review"));
}

// ---- Part 3: the Deploy column ----

#[test]
fn add_column_puts_it_after_the_chosen_column_with_a_unique_name() {
    let b = board();
    assert_eq!(b.columns(), ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done"]);
    let deploy = team::get(&b.db, &b.team).unwrap().states.into_iter().find(|s| s.name == "Deploy").unwrap();
    assert_eq!((deploy.category.as_str(), deploy.auto), ("deploy", false), "Manual: only your Run starts its cards");
    // between two columns again, and after the last one
    let staging = team::add_state(&b.db, &b.you, &b.team, "Staging", &b.state("Review"), "review").unwrap();
    let archive = team::add_state(&b.db, &b.you, &b.team, "Archive", &b.state("Done"), "done").unwrap();
    let design = team::add_state(&b.db, &b.you, &b.team, "Design", &b.state("To do"), "in_progress").unwrap();
    assert_eq!(b.columns(), ["Backlog", "To do", "Design", "In progress", "Testing", "Review", "Staging", "Deploy", "Done", "Archive"]);
    // a new column, a Deploy one too, is Manual, without agents and without a next column
    let release = team::add_state(&b.db, &b.you, &b.team, "Release", &b.state("Deploy"), "deploy").unwrap();
    let states = team::get(&b.db, &b.team).unwrap().states;
    for id in [&staging, &archive, &design, &release] {
        let s = states.iter().find(|s| &s.id == id).unwrap();
        assert_eq!((s.auto, s.agent_ids.len(), s.next_state_id.as_deref()), (false, 0, None), "{}", s.name);
    }
    // refused: a name in use (any case), no name, an unknown category, a column that isn't there
    let e = team::add_state(&b.db, &b.you, &b.team, "deploy", &b.state("Review"), "deploy").unwrap_err().to_string();
    assert!(e.contains("already has a column called deploy"), "{e}");
    assert!(team::add_state(&b.db, &b.you, &b.team, "  ", &b.state("Review"), "review").is_err());
    assert!(team::add_state(&b.db, &b.you, &b.team, "Shipped", &b.state("Review"), "shipped").is_err());
    assert!(team::add_state(&b.db, &b.you, &b.team, "Shipped", "no-such-column", "done").is_err());
    assert_eq!(b.columns().len(), 11);
}

#[test]
fn nothing_routes_or_pulls_a_deploy_card() {
    let b = board();
    // the DevOps Agent is on Deploy; a card with labels, assigned and even pinned to it
    assert_eq!(team::get(&b.db, &b.team).unwrap().states.iter().find(|s| s.name == "Deploy").unwrap().agent_ids, [b.ops.clone()]);
    let t = b.card("Deploy", 1);
    tasks::set_labels(&b.db, &b.you, &t, vec![b.label("bug"), b.label("backend")]).unwrap();
    b.patch(&t, TaskPatch { assignee_id: Some(b.ops.clone()), ..Default::default() });
    assert!(workflow::waiting_for(&b.db, &b.ops).unwrap().is_empty());
    b.patch(&t, TaskPatch { pinned_actor_id: Some(b.ops.clone()), ..Default::default() });
    for agent in [&b.ops, &b.be, &b.qa] {
        assert!(workflow::waiting_for(&b.db, agent).unwrap().is_empty());
        assert_eq!(workflow::next_task_for(&b.db, agent).unwrap(), None, "nothing starts on it by itself");
    }
    // the same card in To do (Auto) would go to the DevOps Agent: it's the column that stops it
    b.to(&t, "To do");
    assert_eq!(workflow::next_task_for(&b.db, &b.ops).unwrap().as_deref(), Some(t.as_str()));
}

#[test]
fn run_on_a_deploy_card_picks_the_teams_devops_agent() {
    let b = board();
    let t = b.card("Deploy", 0);
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), Some((b.ops.clone(), "devops".to_string())));
    // an active one first: with the DevOps Agent paused, the next agent on Deploy
    let bot = team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: "Release bot".into(), role_key: "devops".into(), ..Default::default() }).unwrap();
    team::set_agent_status(&b.db, &b.you, &b.ops, "paused").unwrap();
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), Some((bot, "devops".to_string())), "only an active one");
    // nobody on Deploy: nobody
    columns::set_column(&b.db, &b.you, &b.state("Deploy"), ColumnInput { agent_ids: Some(vec![]), ..Default::default() }).unwrap();
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), None);
}

#[test]
fn deployed_moves_a_deploy_card_to_done_with_the_summary() {
    let b = board();
    let t = b.card("Deploy", 0);
    let g = b.finish(&b.ops, &t, "devops", Some("deployed"));
    assert_eq!((g.moved_to.as_deref(), g.hold), (Some("Done"), None));
    assert_eq!(b.column(&t), "Done");
    assert_eq!(runs::list_for_task(&b.db, &t).unwrap()[0].outcome.as_deref(), Some("deployed"));
    let notes = gizai_core::comments::list(&b.db, &t).unwrap();
    assert!(notes.iter().any(|c| c.author_name == "DevOps Agent" && c.body_md == "summary for deployed"));
    let a = tasks::activity(&b.db, &t).unwrap();
    assert!(a.iter().any(|e| e.actor_name.as_deref() == Some("DevOps Agent") && e.diff == json!({"column": ["Deploy", "Done"]})), "{a:?}");
}

#[test]
fn the_other_devops_outcomes_leave_a_deploy_card_in_deploy() {
    let b = board();
    // needs_decision: on hold, in the inbox
    let t = b.card("Deploy", 0);
    let g = b.finish(&b.ops, &t, "devops", Some("needs_decision"));
    assert_eq!((g.moved_to, g.hold.as_deref(), b.column(&t).as_str()), (None, Some("needs_decision"), "Deploy"));
    assert!(tasks::needs_you(&b.db, &b.you).unwrap().iter().any(|x| x.id == t));
    // ready_for_testing (it only checked something): stays, no hold, never Testing, whatever the switch says
    for testing in [true, false] {
        let t = b.card("Deploy", 0);
        b.patch(&t, TaskPatch { testing: Some(testing), ..Default::default() });
        let g = b.finish(&b.ops, &t, "devops", Some("ready_for_testing"));
        assert_eq!((g.moved_to, g.hold, b.column(&t)), (None, None, "Deploy".to_string()), "testing {testing}");
        assert!(gizai_core::comments::list(&b.db, &t).unwrap().iter().any(|c| c.body_md == "summary for ready_for_testing"));
    }
    // without a result: the usual fail count, and the hold after three
    let t = b.card("Deploy", 0);
    b.finish(&b.ops, &t, "devops", None);
    assert_eq!((b.column(&t).as_str(), b.task(&t).fail_count, b.task(&t).hold), ("Deploy", 1, None));
    b.finish(&b.ops, &t, "devops", None);
    b.finish(&b.ops, &t, "devops", None);
    assert_eq!((b.column(&t).as_str(), b.task(&t).hold.as_deref()), ("Deploy", Some("stalled")));
    // another agent a person started on a Deploy card: its ready_for_testing leaves the card in Deploy too
    let t = b.card("Deploy", 0);
    let g = b.finish(&b.be, &t, "backend", Some("ready_for_testing"));
    assert_eq!((g.moved_to, b.column(&t)), (None, "Deploy".to_string()));
}

#[test]
fn deployed_on_a_card_outside_deploy_holds_it_or_leaves_a_card_a_person_moved() {
    let b = board();
    team::add_state(&b.db, &b.you, &b.team, "Cancelled", &b.state("Done"), "cancelled").unwrap();
    for column in ["Backlog", "Done", "Cancelled"] {
        let t = b.card("In progress", 0);
        let r = b.run(&b.ops, &t, "devops");
        b.to(&t, column);
        let o = Outcome { outcome: "deployed".into(), summary: "released".into(), issues: vec![] };
        let g = workflow::apply_outcome(&b.db, &r, Some(&o)).unwrap();
        assert_eq!((g.moved_to, g.hold, b.column(&t)), (None, None, column.to_string()));
    }
    for column in ["To do", "In progress", "Testing", "Review"] {
        let t = b.card(column, 0);
        let g = b.finish(&b.ops, &t, "devops", Some("deployed"));
        let task = b.task(&t);
        assert_eq!((g.moved_to, g.hold.as_deref(), task.state_name.as_str()), (None, Some("needs_decision"), column), "{column}");
        assert!(task.hold_reason.as_deref().unwrap_or("").contains("isn't in Deploy"), "{:?}", task.hold_reason);
    }
}

#[test]
fn only_the_devops_role_can_answer_deployed() {
    let b = board();
    for (agent, role) in [(&b.be, "backend"), (&b.qa, "qa")] {
        let t = b.card("Deploy", 0);
        let g = b.finish(agent, &t, role, Some("deployed"));
        assert_eq!((g.moved_to, g.hold.as_deref(), b.column(&t)), (None, Some("needs_decision"), "Deploy".to_string()), "{role}");
        assert!(b.task(&t).hold_reason.unwrap().contains("which its role can't use"));
    }
    // and the DevOps Agent can't answer QA's outcomes
    let t = b.card("Review", 0);
    let g = b.finish(&b.ops, &t, "devops", Some("qa_pass"));
    assert_eq!(g.hold.as_deref(), Some("needs_decision"));
}

#[test]
fn a_deploy_card_assigned_to_you_is_in_the_inbox_and_counts_as_open() {
    let b = board();
    let t = b.card("Deploy", 0);
    assert!(!tasks::needs_you(&b.db, &b.you).unwrap().iter().any(|x| x.id == t), "nobody's");
    b.patch(&t, TaskPatch { assignee_id: Some(b.you.clone()), ..Default::default() });
    assert!(tasks::needs_you(&b.db, &b.you).unwrap().iter().any(|x| x.id == t));
    let open = tasks::list(&b.db, &TaskFilter { open_only: true, ..Default::default() }).unwrap();
    assert!(open.iter().any(|x| x.id == t && x.state_category == "deploy"));
}

// ---- 3.6: a merged pull request moves the card to Deploy ----

const PR: &str = "https://github.com/acme/kade/pull/7";

#[test]
fn a_merged_pull_request_moves_a_review_card_to_deploy_and_nothing_picks_it_up() {
    let b = board();
    let t = b.card("Review", 0);
    b.patch(&t, TaskPatch { assignee_id: Some(b.ops.clone()), ..Default::default() });
    assert_eq!(pulls::merged(&b.db, &b.you, &t, PR).unwrap().as_deref(), Some("Deploy"));
    let task = b.task(&t);
    assert_eq!((task.state_name.as_str(), task.pr_state.as_deref(), task.assignee_id.as_deref()), ("Deploy", Some("merged"), Some(b.ops.as_str())),
               "the assignee stays");
    let a = tasks::activity(&b.db, &t).unwrap();
    assert!(a.iter().any(|e| e.diff == json!({"column": ["Review", "Deploy"]})), "{a:?}");
    assert!(workflow::waiting_for(&b.db, &b.ops).unwrap().is_empty());
    assert_eq!(workflow::next_task_for(&b.db, &b.ops).unwrap(), None);
    // already in Deploy: stays
    assert_eq!(pulls::merged(&b.db, &b.you, &t, PR).unwrap(), None);
    assert_eq!(b.column(&t), "Deploy");
    // a card in Done stays in Done
    let done = b.card("Done", 0);
    assert_eq!(pulls::merged(&b.db, &b.you, &done, PR).unwrap(), None);
    assert_eq!((b.column(&done).as_str(), b.task(&done).pr_state.as_deref()), ("Done", Some("merged")));
}

#[test]
fn a_merge_on_a_team_without_a_deploy_column_still_moves_the_card_to_done() {
    let b = bare_board();
    let t = b.card("Review", 0);
    assert_eq!(pulls::merged(&b.db, &b.you, &t, PR).unwrap().as_deref(), Some("Done"));
    assert_eq!(b.column(&t), "Done");
}

// ---- Migration 0007 ----

/// One table as 0001 made it, renamed to `<table>_v6`.
fn table_v6(table: &str) -> String {
    let init = include_str!("../migrations/0001_init.sql");
    let start = init.find(&format!("CREATE TABLE {table} (")).unwrap();
    let end = start + init[start..].find(") STRICT;").unwrap() + ") STRICT;".len();
    init[start..end].replacen(&format!("CREATE TABLE {table} ("), &format!("CREATE TABLE {table}_v6 ("), 1)
}

type Rows = Vec<Vec<Option<String>>>;

fn rows(c: &rusqlite::Connection, sql: &str) -> Rows {
    let mut st = c.prepare(sql).unwrap();
    let n = st.column_count();
    st.query_map([], |r| (0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).map(|v| match v {
        rusqlite::types::Value::Null => None,
        rusqlite::types::Value::Integer(i) => Some(i.to_string()),
        rusqlite::types::Value::Text(t) => Some(t),
        other => Some(format!("{other:?}")),
    })).collect::<rusqlite::Result<Vec<_>>>()).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
}

const SNAPSHOTS: [&str; 5] = [
    "SELECT id, team_id, name, category, owner_role, sort_key FROM workflow_states ORDER BY id",
    "SELECT id, identifier, state_id, state_category, priority, assignee_actor_id, implementer_actor_id, hold, fail_count FROM tasks ORDER BY id",
    "SELECT id, kind, match_label_id, match_state_id, target_role, priority FROM routing_rules ORDER BY id",
    "SELECT id, task_id, agent_actor_id, status, outcome, summary_md, error FROM runs ORDER BY id",
    "SELECT id, task_id, run_id, body_md FROM comments ORDER BY id",
];
/// SNAPSHOTS[2]: the routing rules, which 0011 (GA-49) turns into agents on columns and drops.
const RULES: usize = 2;

/// workflow_states' columns as 0001 made them (0011 added auto and next_state_id).
const STATE_COLUMNS_V6: &str = "id, created_at, updated_at, deleted_at, version, created_by, updated_by, team_id, name, category, owner_role, wip_limit, color, sort_key";

#[test]
fn migration_0007_keeps_every_column_card_rule_run_and_comment_of_an_older_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let be = {
        // A database with data: cards in several columns, finished and failed runs (the rules are added below, as schema 6
        // had them).
        let db = Db::open(&path).unwrap();
        let s = ensure_seed(&db, "Jeffrey").unwrap();
        let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
        let be = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
        for (i, column) in ["To do", "In progress", "Review"].iter().enumerate() {
            let state: String = db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1", [column], |r| r.get(0))?)).unwrap();
            let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: format!("Card {i}"), state_id: Some(state), priority: i as i64,
                ..Default::default() }).unwrap();
            let r = runs::create(&db, &be, &t, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
            if i == 1 {
                runs::finish(&db, &r, "failed", None, 0, 0, 0, Some("exit 1")).unwrap();
                workflow::apply_outcome(&db, &r, None).unwrap();
            } else {
                let o = Outcome { outcome: "ready_for_testing".into(), summary: format!("built card {i}"), issues: vec![] };
                workflow::apply_outcome(&db, &r, Some(&o)).unwrap();
            }
        }
        be
    };
    // Step back to schema 6 as it was: none of 0011's columns, agents on columns or branches, routing rules on a label and
    // on a column, no Deploy column (the seed's has no place in schema 6), workflow_states and runs with 0001's CHECKs,
    // no Testing switch, none of 0008's board check columns, no agent folders (0009), no chat Runs on or queue (0012) and
    // no memory (0014), no Team Lead may merge (0015) and no shared memory folders (0016).
    let c = rusqlite::Connection::open(&path).unwrap();
    let mut sql = format!("PRAGMA foreign_keys=OFF; BEGIN; {} ALTER TABLE projects DROP COLUMN lead_may_merge; {} DROP TABLE column_agents; ALTER TABLE teams DROP COLUMN branches_json;", undo_0016(&c), undo_0014());
    sql.push_str("
        DELETE FROM workflow_states WHERE category='deploy';
        ALTER TABLE runs DROP COLUMN findings_json; ALTER TABLE tasks DROP COLUMN hold_at;
        ALTER TABLE agent_configs DROP COLUMN board_check_minutes; ALTER TABLE agent_configs DROP COLUMN board_checked_at;
        ALTER TABLE agent_configs DROP COLUMN board_check_failures; ALTER TABLE agent_configs DROP COLUMN board_check_paused;
        ALTER TABLE chat_threads DROP COLUMN kind; ALTER TABLE chat_threads DROP COLUMN task_ids_json;
        ALTER TABLE chat_threads DROP COLUMN answered_at; ALTER TABLE chat_threads DROP COLUMN dismissed_at;");
    for (table, columns) in [("workflow_states", STATE_COLUMNS_V6), ("runs", "*")] {
        sql.push_str(&format!("{}; INSERT INTO {table}_v6 SELECT {columns} FROM {table}; DROP TABLE {table}; ALTER TABLE {table}_v6 RENAME TO {table};",
                              table_v6(table)));
    }
    sql.push_str(&format!("{}; ", table_v6("routing_rules").replacen("routing_rules_v6 (", "routing_rules (", 1)));
    sql.push_str("INSERT INTO routing_rules (id, created_at, updated_at, team_id, kind, match_label_id, target_role, priority)
                    SELECT 'rule-label', 1, 1, (SELECT id FROM teams), 'label', id, 'backend', 10 FROM labels WHERE name='backend';
                  INSERT INTO routing_rules (id, created_at, updated_at, team_id, kind, match_state_id, target_role, priority)
                    SELECT 'rule-column', 1, 1, team_id, 'column', id, 'qa', 20 FROM workflow_states WHERE name='Testing';");
    sql.push_str("CREATE INDEX runs_task ON runs(task_id, created_at); CREATE INDEX runs_agent_period ON runs(agent_actor_id, started_at);
                  ALTER TABLE tasks DROP COLUMN testing; ALTER TABLE agent_configs DROP COLUMN folders_json;
                  DROP TABLE chat_queue; ALTER TABLE chat_messages DROP COLUMN meta_json; ALTER TABLE chat_threads DROP COLUMN session_cli; ALTER TABLE chat_threads DROP COLUMN cli; COMMIT; PRAGMA user_version = 6;");
    c.execute_batch(&sql).unwrap();
    assert!(c.execute("UPDATE workflow_states SET category='deploy' WHERE name='Review'", []).is_err(), "schema 6 has no deploy category");
    let before: Vec<Rows> = SNAPSHOTS.iter().map(|q| rows(&c, q)).collect();
    assert_eq!((before[1].len(), before[2].len(), before[3].len()), (3, 2, 3));
    assert!(before[4].len() >= 2, "the summaries are comments");
    drop(c);

    // Gizai opens it: 0007 to 0012 run.
    let db = Db::open(&path).unwrap();
    let after: Vec<Option<Rows>> = db.read(|c| Ok(SNAPSHOTS.iter().enumerate().map(|(i, q)| (i != RULES).then(|| rows(c, q))).collect())).unwrap();
    for (i, q) in SNAPSHOTS.iter().enumerate() {
        if let Some(after) = &after[i] {
            assert_eq!(after, &before[i], "{q}");
        }
    }
    // the rules are agents on columns now (0011): the Backend Agent works To do and In progress
    let rules_left: i64 = db.read(|c| Ok(c.query_row("SELECT count(*) FROM sqlite_master WHERE name='routing_rules'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(rules_left, 0);
    assert_eq!(columns::of_agent(&db, &be).unwrap(), ["To do", "In progress"]);
    let (version, fks, broken, off): (i64, i64, usize, i64) = db.read(|c| Ok((
        c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
        c.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?,
        rows(c, "PRAGMA foreign_key_check").len(),
        c.query_row("SELECT count(*) FROM tasks WHERE testing != 1", [], |r| r.get(0))?,
    ))).unwrap();
    assert_eq!((version, fks, broken, off), (gizai_core::db::SCHEMA_VERSION, 1, 0, 0), "the current version, foreign keys on and intact, every card tested");
    assert!(tasks::list(&db, &TaskFilter::default()).unwrap().iter().all(|t| t.testing));
    let indexes = db.read(|c| Ok(rows(c, "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='runs' AND name NOT LIKE 'sqlite_%' ORDER BY name"))).unwrap();
    assert_eq!(indexes, [[Some("runs_agent_period".to_string())], [Some("runs_task".to_string())]]);
    // foreign keys still refuse a card in a column that isn't there
    let bad = db.read(|c| Ok(c.execute("UPDATE tasks SET state_id='nope' WHERE identifier='KADE-1'", [])?));
    assert!(bad.is_err(), "foreign keys are enforced again");

    // and the new things fit: a Deploy column (linked to Done: a new column has no next column), a card in it, a deployed run
    let team_id = team::list(&db).unwrap()[0].id.clone();
    let you: String = db.read(|c| Ok(c.query_row("SELECT id FROM actors WHERE kind='person'", [], |r| r.get(0))?)).unwrap();
    let state = |name: &str| -> String { db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1", [name], |r| r.get(0))?)).unwrap() };
    let deploy = team::add_state(&db, &you, &team_id, "Deploy", &state("Review"), "deploy").unwrap();
    columns::set_column(&db, &you, &deploy, ColumnInput { next_state_id: Some(state("Done")), ..Default::default() }).unwrap();
    let ops = team::add_agent(&db, &you, &team_id, AgentInput { name: "DevOps Agent".into(), role_key: "devops".into(), ..Default::default() }).unwrap();
    let card: String = db.read(|c| Ok(c.query_row("SELECT id FROM tasks WHERE identifier='KADE-3'", [], |r| r.get(0))?)).unwrap();
    tasks::move_to(&db, &you, &card, &deploy, "").unwrap();
    let r = runs::create(&db, &ops, &card, "devops", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
    let o = Outcome { outcome: "deployed".into(), summary: "Released 0.1.7".into(), issues: vec![] };
    workflow::apply_outcome(&db, &r, Some(&o)).unwrap();
    assert_eq!(tasks::get(&db, &card).unwrap().state_name, "Done");
    assert_eq!(runs::get(&db, &r).unwrap().outcome.as_deref(), Some("deployed"));
}

/// Undoes GA-96's 0016: agent_configs rebuilt without shares_memory_with (it has a foreign key, so it can't be dropped),
/// its other columns, rows and indexes as they were. Runs inside an open transaction, before anything else that changes
/// agent_configs.
fn undo_0016(c: &rusqlite::Connection) -> String {
    let create: String = c.query_row("SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'agent_configs'", [], |r| r.get(0)).unwrap();
    let without = create.replacen(", shares_memory_with TEXT REFERENCES actors(id)", "", 1);
    assert_ne!(without, create, "0016's column: {create}");
    let table = format!("CREATE TABLE agent_configs_v15 {}", &without[without.find('(').unwrap()..]);
    let list = |sql: &str| -> Vec<String> {
        let mut st = c.prepare(sql).unwrap();
        st.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<Vec<String>>>().unwrap()
    };
    let columns = list("SELECT name FROM pragma_table_info('agent_configs') WHERE name <> 'shares_memory_with' ORDER BY cid").join(", ");
    let indexes: String = list("SELECT sql FROM sqlite_master WHERE tbl_name = 'agent_configs' AND type IN ('index', 'trigger') AND sql IS NOT NULL")
        .into_iter().map(|s| format!("{s}; ")).collect();
    format!("{table}; INSERT INTO agent_configs_v15 ({columns}) SELECT {columns} FROM agent_configs;
        DROP TABLE agent_configs; ALTER TABLE agent_configs_v15 RENAME TO agent_configs; {indexes}")
}

/// Undoes GA-19's 0014: docs rebuilt as 0001 made them (owner_actor_id has a foreign key, so it can't be dropped), its
/// memory notes and their versions and links gone, no use_memory or memory_json. Runs inside an open transaction.
fn undo_0014() -> String {
    let m1 = include_str!("../migrations/0001_init.sql");
    let start = m1.find("CREATE TABLE docs (").unwrap();
    let docs = m1[start..start + m1[start..].find(") STRICT;").unwrap() + ") STRICT;".len()].replacen("CREATE TABLE docs (", "CREATE TABLE docs_v13 (", 1);
    format!("DELETE FROM doc_links WHERE source_type = 'doc' AND source_id IN (SELECT id FROM docs WHERE kind = 'memory');
        DELETE FROM doc_versions WHERE doc_id IN (SELECT id FROM docs WHERE kind = 'memory');
        {docs}; INSERT INTO docs_v13 SELECT id, created_at, updated_at, deleted_at, version, created_by, updated_by, org_id, project_id, client_id,
        parent_id, title, body_md, mirror_path, current_version, sort_key FROM docs WHERE kind = 'doc';
        DROP TABLE docs; ALTER TABLE docs_v13 RENAME TO docs;
        ALTER TABLE agent_configs DROP COLUMN use_memory; ALTER TABLE runs DROP COLUMN memory_json;")
}
