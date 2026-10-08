//! QA for GA-49: columns replace label routing. One small test per acceptance rule; each asserts the spec.
use gizai_core::columns::{self, ColumnInput};
use gizai_core::{Error, db::Db, labels, model::*, projects, pulls, runs, seed, tasks, team, workflow};

const PR: &str = "https://github.com/acme/shop/pull/7";

struct B {
    db: Db,
    you: String,
    team: String,
    project: String,
}

fn board() -> B {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    B { db, you: s.you_id, team: s.team_id, project }
}

impl B {
    /// A column of the seed team by name.
    fn st(&self, name: &str) -> String {
        self.st_in(&self.team, name)
    }
    fn st_in(&self, team: &str, name: &str) -> String {
        self.db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE team_id=?1 AND name=?2 AND deleted_at IS NULL",
                                        [team, name], |r| r.get(0))?)).unwrap()
    }
    fn agent(&self, name: &str, role: &str) -> String {
        self.agent_in(&self.team, name, role)
    }
    fn agent_in(&self, team: &str, name: &str, role: &str) -> String {
        team::add_agent(&self.db, &self.you, team, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap()
    }
    fn card(&self, column: &str) -> String {
        self.card_with(column, TaskInput::default())
    }
    fn card_with(&self, column: &str, input: TaskInput) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: self.project.clone(), title: "Export invoices".into(),
                                                        state_id: Some(self.st(column)), ..input }).unwrap()
    }
    fn task(&self, id: &str) -> Task {
        tasks::get(&self.db, id).unwrap()
    }
    fn set(&self, column: &str, input: ColumnInput) -> gizai_core::Result<()> {
        columns::set_column(&self.db, &self.you, &self.st(column), input)
    }
    fn state(&self, id: &str) -> team::WorkflowState {
        team::get(&self.db, &self.team).unwrap().states.into_iter().find(|s| s.id == id).unwrap()
    }
    fn names(&self) -> Vec<String> {
        team::get(&self.db, &self.team).unwrap().states.into_iter().map(|s| s.name).collect()
    }
    fn waiting(&self, agent: &str) -> Vec<String> {
        workflow::waiting_for(&self.db, agent).unwrap()
    }
    /// A run of `agent` on the card that answers `outcome`.
    fn answer(&self, agent: &str, task: &str, role: &str, outcome: &str) -> workflow::GateResult {
        let r = runs::create(&self.db, agent, task, role, "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
        workflow::apply_outcome(&self.db, &r, Some(&Outcome { outcome: outcome.into(), summary: format!("summary for {outcome}"), issues: vec![] })).unwrap()
    }
}

fn invalid<T: std::fmt::Debug>(r: gizai_core::Result<T>, what: &str) -> String {
    match r {
        Err(Error::Invalid(m)) => {
            assert!(m.trim().len() > 10, "{what}: the reason isn't readable: {m:?}");
            m
        }
        other => panic!("{what}: expected Error::Invalid with a reason, got {other:?}"),
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

// ---- 1. default columns ----

#[test]
fn r1_seed_and_new_team_get_the_seven_linked_columns() {
    let b = board();
    let other = team::add_team(&b.db, &b.you, "Second").unwrap();
    for tid in [b.team.clone(), other] {
        let t = team::get(&b.db, &tid).unwrap();
        let names: Vec<&str> = t.states.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done"]);
        let cats: Vec<&str> = t.states.iter().map(|s| s.category.as_str()).collect();
        assert_eq!(cats, ["backlog", "ready", "in_progress", "testing", "review", "deploy", "done"]);
        let next: Vec<Option<&str>> = t.states.iter()
            .map(|s| s.next_state_id.as_ref().map(|n| t.states.iter().find(|x| &x.id == n).expect("next column is in the same team").name.as_str()))
            .collect();
        assert_eq!(next, [None, Some("In progress"), Some("Testing"), Some("Review"), Some("Deploy"), Some("Done"), None]);
        let auto: Vec<bool> = t.states.iter().map(|s| s.auto).collect();
        assert_eq!(auto, [false, true, true, true, false, false, false], "To do, In progress, Testing Auto; Deploy Manual; others not Auto");
        assert!(t.states.iter().all(|s| s.agent_ids.is_empty()), "no agents on a new team's columns");
    }
}

// ---- 2. new agents land on their role's columns ----

#[test]
fn r2_new_agent_lands_on_its_roles_usual_columns() {
    let b = board();
    for role in ["backend", "frontend", "design", "docs"] {
        let a = b.agent(&format!("{role} agent"), role);
        assert_eq!(columns::of_agent(&b.db, &a).unwrap(), s(&["To do", "In progress"]), "builder {role}");
    }
    let qa = b.agent("QA Agent", "qa");
    assert_eq!(columns::of_agent(&b.db, &qa).unwrap(), s(&["Testing"]));
    let ops = b.agent("DevOps Agent", "devops");
    assert_eq!(columns::of_agent(&b.db, &ops).unwrap(), s(&["Deploy"]));
    let lead = b.agent("Team Lead", "lead");
    assert!(columns::of_agent(&b.db, &lead).unwrap().is_empty(), "the lead is on no column");
    // In another team, an agent lands on that team's columns.
    let other = team::add_team(&b.db, &b.you, "Second").unwrap();
    let be2 = b.agent_in(&other, "Backend Two", "backend");
    let t2 = team::get(&b.db, &other).unwrap();
    assert!(t2.states.iter().find(|x| x.name == "To do").unwrap().agent_ids.contains(&be2));
    assert!(!b.state(&b.st("To do")).agent_ids.contains(&be2));
}

#[test]
fn r2_an_agent_can_be_on_several_columns() {
    let b = board();
    let qa = b.agent("QA Agent", "qa");
    let be = b.agent("Backend Agent", "backend");
    b.set("Testing", ColumnInput { agent_ids: Some(vec![qa.clone(), be.clone()]), ..Default::default() }).unwrap();
    assert_eq!(columns::of_agent(&b.db, &be).unwrap(), s(&["To do", "In progress", "Testing"]));
    assert_eq!(b.state(&b.st("Testing")).agent_ids, vec![qa, be]);
}

// ---- 3. set_column refusals ----

#[test]
fn r3_no_agents_on_backlog_review_done_or_cancelled() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    team::add_state(&b.db, &b.you, &b.team, "Cancelled", &b.st("Done"), "cancelled").unwrap();
    for col in ["Backlog", "Review", "Done", "Cancelled"] {
        invalid(b.set(col, ColumnInput { agent_ids: Some(vec![be.clone()]), ..Default::default() }), col);
        invalid(columns::add_agent(&b.db, &b.you, &b.st(col), &be), &format!("add_agent {col}"));
        assert!(b.state(&b.st(col)).agent_ids.is_empty(), "{col} still has no agents");
    }
}

#[test]
fn r3_no_auto_on_backlog_review_done_or_cancelled() {
    let b = board();
    team::add_state(&b.db, &b.you, &b.team, "Cancelled", &b.st("Done"), "cancelled").unwrap();
    for col in ["Backlog", "Review", "Done", "Cancelled"] {
        // Give it a next column, so the only problem is Auto.
        let m = invalid(b.set(col, ColumnInput { auto: Some(true), next_state_id: Some(b.st("To do")), ..Default::default() }), col);
        assert!(m.contains("Auto"), "{col}: {m}");
        assert!(!b.state(&b.st(col)).auto);
    }
}

#[test]
fn r3_auto_needs_a_next_column() {
    let b = board();
    let id = team::add_state(&b.db, &b.you, &b.team, "Extra", &b.st("In progress"), "in_progress").unwrap();
    invalid(columns::set_column(&b.db, &b.you, &id, ColumnInput { auto: Some(true), ..Default::default() }), "Auto without next");
    // Clearing an Auto column's next column is refused too.
    invalid(b.set("To do", ColumnInput { next_state_id: Some(String::new()), ..Default::default() }), "clear next of Auto To do");
    assert!(b.state(&b.st("To do")).next_state_id.is_some());
}

#[test]
fn r3_no_link_to_itself() {
    let b = board();
    let todo = b.st("To do");
    invalid(b.set("To do", ColumnInput { next_state_id: Some(todo.clone()), ..Default::default() }), "To do → To do");
    invalid(b.set("Review", ColumnInput { next_state_id: Some(b.st("Review")), ..Default::default() }), "Review → Review");
}

#[test]
fn r3_no_link_to_another_teams_column() {
    let b = board();
    let other = team::add_team(&b.db, &b.you, "Second").unwrap();
    let theirs = b.st_in(&other, "Review");
    invalid(b.set("In progress", ColumnInput { next_state_id: Some(theirs), ..Default::default() }), "link to other team");
    assert_eq!(b.state(&b.st("In progress")).next_state_id, Some(b.st("Testing")));
}

#[test]
fn r3_no_duplicate_column_name_in_any_case() {
    let b = board();
    invalid(b.set("In progress", ColumnInput { name: Some("testing".into()), ..Default::default() }), "rename to testing");
    invalid(b.set("In progress", ColumnInput { name: Some("TESTING".into()), ..Default::default() }), "rename to TESTING");
    invalid(team::add_state(&b.db, &b.you, &b.team, "testing", &b.st("Review"), "testing"), "add testing");
    assert!(b.names().contains(&"In progress".to_string()));
}

#[test]
fn r3_only_the_teams_agents_go_on_its_columns() {
    let b = board();
    let other = team::add_team(&b.db, &b.you, "Second").unwrap();
    let stranger = b.agent_in(&other, "Stranger", "backend");
    invalid(b.set("To do", ColumnInput { agent_ids: Some(vec![stranger.clone()]), ..Default::default() }), "agent of another team");
    invalid(b.set("To do", ColumnInput { agent_ids: Some(vec![b.you.clone()]), ..Default::default() }), "a person");
    assert!(!b.state(&b.st("To do")).agent_ids.contains(&stranger));
}

#[test]
fn r3_a_valid_full_setup_is_accepted() {
    let b = board();
    let designer = b.agent("Design Agent", "design");
    let be = b.agent("Backend Agent", "backend");
    let id = team::add_state(&b.db, &b.you, &b.team, "Design", &b.st("Backlog"), "in_progress").unwrap();
    columns::set_column(&b.db, &b.you, &id, ColumnInput {
        name: Some("Design work".into()), agent_ids: Some(vec![designer.clone(), be.clone()]), auto: Some(true),
        next_state_id: Some(b.st("Review")), after_id: Some(b.st("To do")),
    }).unwrap();
    let c = b.state(&id);
    assert_eq!((c.name.as_str(), c.auto, c.next_state_id.clone(), c.agent_ids.clone()),
               ("Design work", true, Some(b.st("Review")), vec![designer, be]));
    assert_eq!(b.names(), s(&["Backlog", "To do", "Design work", "In progress", "Testing", "Review", "Deploy", "Done"]));
}

// ---- 4. add a column, reorder ----

#[test]
fn r4_add_state_takes_a_name_a_kind_and_a_place() {
    let b = board();
    for (kind, cat) in [("Waiting", "ready"), ("Work", "in_progress"), ("Testing", "testing"), ("Review", "review"), ("Deploy", "deploy"),
                        ("Done", "done"), ("Backlog", "backlog")] {
        assert_eq!(columns::category_of(kind), Some(cat), "kind {kind}");
    }
    // As the UI command does: the kind in plain words → its category → add_state after a column.
    let id = team::add_state(&b.db, &b.you, &b.team, "Design", &b.st("To do"), columns::category_of("Work").unwrap()).unwrap();
    assert_eq!(b.names(), s(&["Backlog", "To do", "Design", "In progress", "Testing", "Review", "Deploy", "Done"]));
    let c = b.state(&id);
    assert_eq!((c.category.as_str(), c.auto, c.next_state_id.clone(), c.agent_ids.len()), ("in_progress", false, None, 0), "Manual, no next, no agents");
    let qa2 = team::add_state(&b.db, &b.you, &b.team, "Acceptance", &b.st("Review"), columns::category_of("Testing").unwrap()).unwrap();
    assert_eq!(b.state(&qa2).category, "testing");
    assert_eq!(b.names()[6], "Acceptance");
}

#[test]
fn r4_reorder_changes_the_board_order_not_the_links() {
    let b = board();
    let links = |b: &B| -> Vec<(String, Option<String>)> {
        let mut v: Vec<_> = team::get(&b.db, &b.team).unwrap().states.into_iter().map(|s| (s.id, s.next_state_id)).collect();
        v.sort();
        v
    };
    let before = links(&b);
    b.set("Testing", ColumnInput { after_id: Some(b.st("Backlog")), ..Default::default() }).unwrap();
    assert_eq!(b.names(), s(&["Backlog", "Testing", "To do", "In progress", "Review", "Deploy", "Done"]));
    b.set("Done", ColumnInput { after_id: Some(String::new()), ..Default::default() }).unwrap();
    assert_eq!(b.names(), s(&["Done", "Backlog", "Testing", "To do", "In progress", "Review", "Deploy"]));
    b.set("Done", ColumnInput { after_id: Some(b.st("Deploy")), ..Default::default() }).unwrap();
    assert_eq!(b.names(), s(&["Backlog", "Testing", "To do", "In progress", "Review", "Deploy", "Done"]));
    assert_eq!(links(&b), before, "next columns don't change on reorder");
}

// ---- 5. removing a column ----

#[test]
fn r5_removal_reports_cards_target_and_relinked_columns() {
    let b = board();
    b.card("Testing");
    b.card("Testing");
    let r = columns::removal(&b.db, &b.st("Testing")).unwrap();
    assert_eq!((r.cards, r.archived), (2, 0));
    assert_eq!(r.default_target, Some(b.st("In progress")), "the column before it");
    assert_eq!(r.relinked, s(&["In progress"]));
    assert!(r.unlinked.is_empty());
    assert_eq!(r.blocked, None);
}

#[test]
fn r5_removal_counts_archived_cards_and_remove_moves_them_too() {
    let b = board();
    let shipped = team::add_state(&b.db, &b.you, &b.team, "Shipped", &b.st("Done"), "done").unwrap();
    let live = tasks::create(&b.db, &b.you, TaskInput { project_id: b.project.clone(), title: "a".into(), state_id: Some(shipped.clone()), ..Default::default() }).unwrap();
    let old = tasks::create(&b.db, &b.you, TaskInput { project_id: b.project.clone(), title: "b".into(), state_id: Some(shipped.clone()), ..Default::default() }).unwrap();
    tasks::archive(&b.db, &b.you, &old).unwrap();
    let r = columns::removal(&b.db, &shipped).unwrap();
    assert_eq!((r.cards, r.archived), (2, 1), "cards include archived ones");
    assert_eq!(r.default_target, Some(b.st("Done")));
    assert_eq!(r.blocked, None, "not the last Done column");
    let moved = columns::remove_state(&b.db, &b.you, &shipped, &b.st("Done")).unwrap();
    assert_eq!(moved, vec![live.clone()]);
    for id in [&live, &old] {
        assert_eq!(b.task(id).state_name, "Done");
        let acts = tasks::activity(&b.db, id).unwrap();
        assert!(acts.iter().any(|a| a.diff.get("columnRemoved").and_then(|v| v.as_str()) == Some("Shipped")), "activity on {id}: {acts:?}");
    }
    assert!(b.task(&old).archived_at.is_some(), "still archived");
}

#[test]
fn r5_removal_lists_columns_left_without_a_link() {
    let b = board();
    // Testing → Extra (no next): removing Extra leaves Testing with no next column.
    let extra = team::add_state(&b.db, &b.you, &b.team, "Extra", &b.st("Testing"), "in_progress").unwrap();
    b.set("Testing", ColumnInput { next_state_id: Some(extra.clone()), ..Default::default() }).unwrap();
    let r = columns::removal(&b.db, &extra).unwrap();
    assert_eq!((r.relinked.clone(), r.unlinked.clone()), (vec![], s(&["Testing"])));
    columns::remove_state(&b.db, &b.you, &extra, &b.st("Testing")).unwrap();
    let t = b.state(&b.st("Testing"));
    assert_eq!((t.next_state_id, t.auto), (None, false), "loses the link and turns Manual");

    // In progress → Testing → In progress: removing Testing would link In progress to itself.
    let b = board();
    b.set("Testing", ColumnInput { next_state_id: Some(b.st("In progress")), ..Default::default() }).unwrap();
    let r = columns::removal(&b.db, &b.st("Testing")).unwrap();
    assert_eq!((r.relinked.clone(), r.unlinked.clone()), (vec![], s(&["In progress"])));
    columns::remove_state(&b.db, &b.you, &b.st("Testing"), &b.st("In progress")).unwrap();
    let ip = b.state(&b.st("In progress"));
    assert_eq!((ip.next_state_id, ip.auto), (None, false));
}

#[test]
fn r5_removal_is_blocked_for_last_backlog_last_done_and_a_live_run() {
    let b = board();
    for col in ["Backlog", "Done"] {
        let r = columns::removal(&b.db, &b.st(col)).unwrap();
        assert!(r.blocked.as_deref().is_some_and(|m| m.len() > 10), "{col}: {:?}", r.blocked);
        invalid(columns::remove_state(&b.db, &b.you, &b.st(col), &b.st("To do")), col);
    }
    let be = b.agent("Backend Agent", "backend");
    let t = b.card("In progress");
    let run = runs::create(&b.db, &be, &t, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
    runs::set_running(&b.db, &run, 4242).unwrap();
    let r = columns::removal(&b.db, &b.st("In progress")).unwrap();
    assert!(r.blocked.is_some(), "a live run blocks: {r:?}");
    invalid(columns::remove_state(&b.db, &b.you, &b.st("In progress"), &b.st("To do")), "live run");
    // A second Backlog makes the first removable.
    team::add_state(&b.db, &b.you, &b.team, "Ideas", &b.st("Backlog"), "backlog").unwrap();
    assert_eq!(columns::removal(&b.db, &b.st("Backlog")).unwrap().blocked, None);
}

#[test]
fn r5_remove_state_moves_cards_keeps_them_bridges_links_and_frees_the_name() {
    let b = board();
    let qa = b.agent("QA Agent", "qa");
    let be = b.agent("Backend Agent", "backend");
    let held = b.card_with("Testing", TaskInput { assignee_id: Some(be.clone()), testing: Some(false), ..Default::default() });
    tasks::update(&b.db, &b.you, &held, TaskPatch { hold: Some("blocked".into()), hold_reason: Some("why".into()), ..Default::default() }).unwrap();
    let walked = b.card("In progress");
    tasks::move_to(&b.db, &b.you, &walked, &b.st("Testing"), "").unwrap();
    let testing = b.st("Testing");
    let moved = columns::remove_state(&b.db, &b.you, &testing, &b.st("In progress")).unwrap();
    assert_eq!(moved.len(), 2);
    let h = b.task(&held);
    assert_eq!((h.state_name.as_str(), h.hold.as_deref(), h.assignee_id.as_deref(), h.testing), ("In progress", Some("blocked"), Some(be.as_str()), false));
    for id in [&held, &walked] {
        assert_eq!(b.task(id).state_name, "In progress");
        assert!(tasks::activity(&b.db, id).unwrap().iter().any(|a| a.diff.get("columnRemoved").and_then(|v| v.as_str()) == Some("Testing")), "activity on {id}");
    }
    // Old activity keeps the column's name.
    assert!(tasks::activity(&b.db, &walked).unwrap().iter().any(|a| a.diff.get("column") == Some(&serde_json::json!(["In progress", "Testing"]))));
    // In progress → Testing → Review becomes In progress → Review, still Auto.
    let ip = b.state(&b.st("In progress"));
    assert_eq!((ip.next_state_id, ip.auto), (Some(b.st("Review")), true));
    // Its agents come off; the row is kept with deleted_at.
    let on: i64 = b.db.read(|c| Ok(c.query_row("SELECT count(*) FROM column_agents WHERE state_id=?1", [&testing], |r| r.get(0))?)).unwrap();
    assert_eq!(on, 0);
    assert!(columns::of_agent(&b.db, &qa).unwrap().is_empty());
    let (name, deleted): (String, Option<i64>) = b.db.read(|c| Ok(c.query_row("SELECT name, deleted_at FROM workflow_states WHERE id=?1", [&testing],
                                                                              |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!(name, "Testing");
    assert!(deleted.is_some());
    assert!(!b.names().contains(&"Testing".to_string()));
    // The name is free again.
    let again = team::add_state(&b.db, &b.you, &b.team, "Testing", &b.st("In progress"), "testing").unwrap();
    assert_ne!(again, testing);
}

#[test]
fn r5_cards_of_a_removed_column_count_as_a_drag_auto_targets_pick_them_up_manual_ones_dont() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    let ops = b.agent("DevOps Agent", "devops");
    // A Manual Design column without agents, holding two cards nobody picks up.
    let design = team::add_state(&b.db, &b.you, &b.team, "Design", &b.st("In progress"), "in_progress").unwrap();
    let cards = [b.card("Design"), b.card("Design")];
    assert!(cards.iter().all(|c| !b.waiting(&be).contains(c)));
    // Removed into In progress (Auto, the Backend Agent on it): its agent picks them up.
    let moved = columns::remove_state(&b.db, &b.you, &design, &b.st("In progress")).unwrap();
    assert_eq!(moved.len(), 2);
    assert!(cards.iter().all(|c| b.waiting(&be).contains(c)), "{:?}", b.waiting(&be));
    // Removed into Deploy (Manual, the DevOps Agent on it): nothing starts by itself; Run takes the column's agent.
    let parking = team::add_state(&b.db, &b.you, &b.team, "Parking", &b.st("Review"), "in_progress").unwrap();
    let parked = b.card("Parking");
    columns::remove_state(&b.db, &b.you, &parking, &b.st("Deploy")).unwrap();
    assert_eq!(b.task(&parked).state_name, "Deploy");
    assert!(!b.waiting(&ops).contains(&parked));
    assert!(!b.waiting(&be).contains(&parked));
    assert_eq!(workflow::run_agent(&b.db, &parked).unwrap().map(|(a, _)| a), Some(ops));
}

// ---- 6. labels ----

#[test]
fn r6_labels_list_save_rename_recolour_and_remove() {
    let b = board();
    let list = labels::list(&b.db).unwrap();
    assert_eq!(list.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["backend", "bug", "frontend", "qa"]);
    let backend = list.iter().find(|l| l.name == "backend").unwrap().id.clone();
    let t = b.card_with("To do", TaskInput { label_ids: vec![backend.clone()], ..Default::default() });
    assert_eq!(labels::list(&b.db).unwrap().iter().find(|l| l.id == backend).unwrap().cards, 1);
    let id = labels::save(&b.db, &b.you, None, "Must have", Some("#7B9BFF")).unwrap();
    let l = labels::list(&b.db).unwrap().into_iter().find(|l| l.id == id).unwrap();
    assert_eq!((l.name.as_str(), l.color.as_deref(), l.cards), ("Must have", Some("#7b9bff"), 0));
    labels::save(&b.db, &b.you, Some(&id), "Should have", None).unwrap();
    let l = labels::list(&b.db).unwrap().into_iter().find(|l| l.id == id).unwrap();
    assert_eq!((l.name.as_str(), l.color.as_deref()), ("Should have", Some("#7b9bff")), "rename keeps the colour");
    labels::save(&b.db, &b.you, Some(&id), "Should have", Some("#123456")).unwrap();
    assert_eq!(labels::list(&b.db).unwrap().into_iter().find(|l| l.id == id).unwrap().color.as_deref(), Some("#123456"));
    invalid(labels::save(&b.db, &b.you, None, "BACKEND", None), "new label BACKEND");
    invalid(labels::save(&b.db, &b.you, Some(&id), "Bug", None), "rename to Bug");
    assert_eq!(labels::remove(&b.db, &b.you, &backend).unwrap(), 1);
    assert!(b.task(&t).labels.is_empty(), "the card lost the label");
    assert!(labels::list(&b.db).unwrap().iter().all(|l| l.name != "backend"));
}

#[test]
fn r6_labels_never_decide_who_works_a_card() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let be = b.agent("Backend Agent", "backend");
    columns::remove_agent(&b.db, &b.you, &b.st("To do"), &be).unwrap();
    let backend = labels::find(&b.db, "backend").unwrap().unwrap().id;
    let t = b.card_with("To do", TaskInput { label_ids: vec![backend], ..Default::default() });
    assert_eq!(b.waiting(&fe), vec![t.clone()]);
    assert!(!b.waiting(&be).contains(&t), "the backend agent is off To do");
    assert_eq!(workflow::next_task_for(&b.db, &be).unwrap(), None);
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap().map(|x| x.0), Some(fe));
}

// ---- 7. branches ----

#[test]
fn r7_branches_per_team() {
    let b = board();
    let names = |team: &str| team::get(&b.db, team).unwrap().branches.into_iter().map(|x| x.name).collect::<Vec<_>>();
    assert_eq!(names(&b.team), s(&["Design", "Development", "Quality", "Operations"]));
    let other = team::add_team(&b.db, &b.you, "Second").unwrap();
    let after = team::add_branch(&b.db, &b.you, &b.team, "Docs").unwrap();
    let docs = after.last().unwrap();
    assert_eq!((docs.key.as_str(), docs.name.as_str(), docs.roles.clone()), ("docs", "Docs", s(&["docs"])));
    assert_eq!(names(&b.team), s(&["Design", "Development", "Quality", "Operations", "Docs"]));
    b.agent("Docs Agent", "docs");
    invalid(team::remove_branch(&b.db, &b.you, &b.team, "docs"), "remove Docs with an agent");
    assert!(names(&b.team).contains(&"Docs".to_string()));
    team::remove_branch(&b.db, &b.you, &b.team, "design").unwrap();
    assert_eq!(names(&b.team), s(&["Development", "Quality", "Operations", "Docs"]));
    assert_eq!(names(&other), s(&["Design", "Development", "Quality", "Operations"]), "another team is unaffected");
}

// ---- 8. start rules ----

#[test]
fn r8_auto_column_queue_order() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let p = |prio: i64| TaskInput { priority: prio, ..Default::default() };
    let none = b.card_with("To do", p(0));
    let low = b.card_with("To do", p(4));
    let urgent = b.card_with("In progress", p(1));
    let high = b.card_with("To do", p(2));
    let med_a = b.card_with("To do", p(3));
    let med_ip = b.card_with("In progress", p(3));
    let med_mine = b.card_with("In progress", TaskInput { priority: 3, assignee_id: Some(fe.clone()), ..Default::default() });
    let med_b = b.card_with("To do", p(3));
    assert_eq!(b.waiting(&fe), vec![urgent, high, med_mine, med_a, med_b, med_ip, low, none.clone()]);
    assert_eq!(workflow::next_task_for(&b.db, &fe).unwrap(), Some(b.waiting(&fe)[0].clone()));
}

#[test]
fn r8_a_card_assigned_to_an_agent_is_offered_only_to_that_agent() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let qa = b.agent("QA Agent", "qa");
    let t = b.card_with("To do", TaskInput { assignee_id: Some(qa.clone()), ..Default::default() });
    assert!(!b.waiting(&fe).contains(&t), "not offered to the agents on the column");
    assert!(b.waiting(&qa).contains(&t), "offered to its agent, which isn't on To do");
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), Some((qa, "qa".to_string())));
}

#[test]
fn r8_a_person_assignee_doesnt_block() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let t = b.card_with("To do", TaskInput { assignee_id: Some(b.you.clone()), ..Default::default() });
    assert_eq!(b.waiting(&fe), vec![t]);
}

#[test]
fn r8_a_manual_column_offers_nothing() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let ops = b.agent("DevOps Agent", "devops");
    let t = b.card("To do");
    let mine = b.card_with("To do", TaskInput { assignee_id: Some(fe.clone()), ..Default::default() });
    assert_eq!(b.waiting(&fe).len(), 2);
    b.set("To do", ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    assert!(b.waiting(&fe).is_empty(), "Manual To do: {:?} {t} {mine}", b.waiting(&fe));
    b.card("Deploy");
    b.card_with("Deploy", TaskInput { assignee_id: Some(ops.clone()), ..Default::default() });
    assert!(b.waiting(&ops).is_empty(), "Deploy is Manual");
    assert_eq!(workflow::next_task_for(&b.db, &ops).unwrap(), None);
}

#[test]
fn r8_an_agent_off_the_column_or_paused_gets_nothing_new() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let be = b.agent("Backend Agent", "backend");
    let t = b.card("To do");
    assert!(b.waiting(&fe).contains(&t) && b.waiting(&be).contains(&t));
    columns::remove_agent(&b.db, &b.you, &b.st("To do"), &fe).unwrap();
    assert!(b.waiting(&fe).is_empty());
    team::set_agent_status(&b.db, &b.you, &be, "paused").unwrap();
    assert!(b.waiting(&be).is_empty());
    assert_eq!(workflow::next_task_for(&b.db, &be).unwrap(), None);
}

#[test]
fn r8_the_wakeup_value_changes_nothing() {
    let b = board();
    let mut agents = vec![];
    for (i, w) in ["manual", "on_assign", "heartbeat"].into_iter().enumerate() {
        agents.push(team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: format!("FE {i}"), role_key: "frontend".into(), wakeup: w.into(),
            heartbeat_minutes: (w == "heartbeat").then_some(15), ..Default::default() }).unwrap());
    }
    let t = b.card("To do");
    for a in &agents {
        assert_eq!(b.waiting(a), vec![t.clone()]);
    }
}

#[test]
fn r8_held_cards_are_skipped() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let held = b.card("To do");
    let free = b.card("To do");
    tasks::update(&b.db, &b.you, &held, TaskPatch { hold: Some("needs_decision".into()), ..Default::default() }).unwrap();
    assert_eq!(b.waiting(&fe), vec![free]);
}

#[test]
fn r8_testing_switch_off_skips_qa() {
    let b = board();
    let qa = b.agent("QA Agent", "qa");
    let off = b.card_with("Testing", TaskInput { testing: Some(false), ..Default::default() });
    let on = b.card("Testing");
    assert_eq!(b.waiting(&qa), vec![on]);
    assert!(!b.waiting(&qa).contains(&off));
}

#[test]
fn r8_run_agent_assignee_then_first_on_column_then_none() {
    let b = board();
    let fe = b.agent("Frontend Agent", "frontend");
    let be = b.agent("Backend Agent", "backend");
    let qa = b.agent("QA Agent", "qa");
    let t = b.card("To do");
    b.set("To do", ColumnInput { agent_ids: Some(vec![be.clone(), fe.clone()]), ..Default::default() }).unwrap();
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), Some((be.clone(), "backend".into())));
    b.set("To do", ColumnInput { agent_ids: Some(vec![fe.clone(), be.clone()]), ..Default::default() }).unwrap();
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), Some((fe.clone(), "frontend".into())));
    tasks::update(&b.db, &b.you, &t, TaskPatch { assignee_id: Some(qa.clone()), ..Default::default() }).unwrap();
    assert_eq!(workflow::run_agent(&b.db, &t).unwrap(), Some((qa, "qa".into())), "the card's agent assignee first");
    let r = b.card("Review");
    assert_eq!(workflow::run_agent(&b.db, &r).unwrap(), None);
    let empty = team::add_state(&b.db, &b.you, &b.team, "Spare", &b.st("In progress"), "in_progress").unwrap();
    let sp = tasks::create(&b.db, &b.you, TaskInput { project_id: b.project.clone(), title: "x".into(), state_id: Some(empty), ..Default::default() }).unwrap();
    assert_eq!(workflow::run_agent(&b.db, &sp).unwrap(), None);
}

// ---- 9. flow ----

#[test]
fn r9_start_in_to_do_moves_to_its_next_column() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    let t = b.card("To do");
    assert_eq!(workflow::move_on_start(&b.db, &be, &t).unwrap(), Some(b.st("To do")));
    assert_eq!(b.task(&t).state_name, "In progress");
    b.set("To do", ColumnInput { next_state_id: Some(b.st("Testing")), ..Default::default() }).unwrap();
    let t2 = b.card("To do");
    workflow::move_on_start(&b.db, &be, &t2).unwrap();
    assert_eq!(b.task(&t2).state_name, "Testing", "follows To do's next column");
}

#[test]
fn r9_ready_for_testing_then_qa_pass_then_deployed() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    let qa = b.agent("QA Agent", "qa");
    let ops = b.agent("DevOps Agent", "devops");
    let t = b.card("In progress");
    assert_eq!(b.answer(&be, &t, "backend", "ready_for_testing").moved_to.as_deref(), Some("Testing"));
    assert_eq!(b.task(&t).state_name, "Testing");
    assert_eq!(b.answer(&qa, &t, "qa", "qa_pass").moved_to.as_deref(), Some("Review"));
    let x = b.task(&t);
    assert_eq!((x.state_name.as_str(), x.assignee_id.as_deref(), x.assignee_kind.as_deref()), ("Review", Some(b.you.as_str()), Some("person")));
    let d = b.card("Deploy");
    b.answer(&ops, &d, "devops", "deployed");
    assert_eq!(b.task(&d).state_name, "Done");
}

#[test]
fn r9_qa_fail_goes_back_to_the_builder_and_the_third_bounce_holds() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    let qa = b.agent("QA Agent", "qa");
    let t = b.card("To do");
    workflow::move_on_start(&b.db, &be, &t).unwrap();
    for i in 1..=3 {
        b.answer(&be, &t, "backend", "ready_for_testing");
        assert_eq!(b.task(&t).state_name, "Testing", "round {i}");
        let g = b.answer(&qa, &t, "qa", "qa_fail");
        let x = b.task(&t);
        assert_eq!((x.state_name.as_str(), x.assignee_id.as_deref(), x.bounce_count), ("In progress", Some(be.as_str()), i), "round {i}");
        assert_eq!(g.moved_to.as_deref(), Some("In progress"));
        if i < 3 {
            assert_eq!(x.hold, None, "round {i}");
        } else {
            assert_eq!(x.hold.as_deref(), Some("needs_decision"));
        }
    }
}

#[test]
fn r9_testing_off_skips_testing_to_review() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    let t = b.card_with("In progress", TaskInput { testing: Some(false), ..Default::default() });
    b.answer(&be, &t, "backend", "ready_for_testing");
    let x = b.task(&t);
    assert_eq!((x.state_name.as_str(), x.assignee_id.as_deref()), ("Review", Some(b.you.as_str())));
}

#[test]
fn r9_needs_decision_holds_in_place() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    let t = b.card("In progress");
    let g = b.answer(&be, &t, "backend", "needs_decision");
    let x = b.task(&t);
    assert_eq!((x.state_name.as_str(), x.hold.as_deref(), g.hold.as_deref()), ("In progress", Some("needs_decision"), Some("needs_decision")));
}

#[test]
fn r9_devops_rules_from_ga32() {
    let b = board();
    let ops = b.agent("DevOps Agent", "devops");
    let a = b.card("In progress");
    b.answer(&ops, &a, "devops", "ready_for_testing");
    assert_eq!(b.task(&a).state_name, "Review", "devops ready_for_testing outside Deploy ends in Review, never Testing");
    let d = b.card("Deploy");
    b.answer(&ops, &d, "devops", "ready_for_testing");
    let x = b.task(&d);
    assert_eq!((x.state_name.as_str(), x.hold.as_deref()), ("Deploy", None));
    let c = b.card("In progress");
    b.answer(&ops, &c, "devops", "deployed");
    let x = b.task(&c);
    assert_eq!((x.state_name.as_str(), x.hold.as_deref()), ("In progress", Some("needs_decision")));
}

#[test]
fn r9_a_design_column_with_its_agent_and_review_next() {
    let b = board();
    let designer = b.agent("Design Agent", "design");
    let id = team::add_state(&b.db, &b.you, &b.team, "Design", &b.st("To do"), columns::category_of("Work").unwrap()).unwrap();
    columns::set_column(&b.db, &b.you, &id, ColumnInput { agent_ids: Some(vec![designer.clone()]), next_state_id: Some(b.st("Review")),
                                                          auto: Some(true), ..Default::default() }).unwrap();
    let t = b.card("Backlog");
    tasks::move_to(&b.db, &b.you, &t, &id, "").unwrap();
    assert_eq!(b.waiting(&designer), vec![t.clone()]);
    b.answer(&designer, &t, "design", "ready_for_testing");
    let x = b.task(&t);
    assert_eq!((x.state_name.as_str(), x.assignee_id.as_deref()), ("Review", Some(b.you.as_str())));
}

#[test]
fn r9_ready_for_testing_follows_the_columns_next_link() {
    let b = board();
    let be = b.agent("Backend Agent", "backend");
    b.set("In progress", ColumnInput { next_state_id: Some(b.st("Review")), ..Default::default() }).unwrap();
    let t = b.card("In progress");
    b.answer(&be, &t, "backend", "ready_for_testing");
    let x = b.task(&t);
    assert_eq!((x.state_name.as_str(), x.assignee_id.as_deref()), ("Review", Some(b.you.as_str())));
}

#[test]
fn r9_qa_fail_returns_to_a_custom_column_it_came_from() {
    let b = board();
    let designer = b.agent("Design Agent", "design");
    let qa = b.agent("QA Agent", "qa");
    let id = team::add_state(&b.db, &b.you, &b.team, "Design", &b.st("To do"), "in_progress").unwrap();
    columns::set_column(&b.db, &b.you, &id, ColumnInput { agent_ids: Some(vec![designer.clone()]), next_state_id: Some(b.st("Testing")),
                                                          auto: Some(true), ..Default::default() }).unwrap();
    let t = tasks::create(&b.db, &b.you, TaskInput { project_id: b.project.clone(), title: "Screens".into(), state_id: Some(id), ..Default::default() }).unwrap();
    b.answer(&designer, &t, "design", "ready_for_testing");
    assert_eq!(b.task(&t).state_name, "Testing");
    b.answer(&qa, &t, "qa", "qa_fail");
    let x = b.task(&t);
    assert_eq!((x.state_name.as_str(), x.assignee_id.as_deref()), ("Design", Some(designer.as_str())));
    assert_eq!(b.waiting(&designer), vec![t]);
}

// ---- 10. merge ----

#[test]
fn r10_a_merge_moves_a_review_card_to_reviews_next_else_deploy_else_done() {
    let b = board();
    let a = b.card("Review");
    assert_eq!(pulls::merged(&b.db, &b.you, &a, PR).unwrap().as_deref(), Some("Deploy"));
    assert_eq!(b.task(&a).state_name, "Deploy");
    // Review's next decides.
    b.set("Review", ColumnInput { next_state_id: Some(b.st("Done")), ..Default::default() }).unwrap();
    let c = b.card("Review");
    pulls::merged(&b.db, &b.you, &c, PR).unwrap();
    assert_eq!(b.task(&c).state_name, "Done");
    // Without one: the first Deploy column…
    b.set("Review", ColumnInput { next_state_id: Some(String::new()), ..Default::default() }).unwrap();
    let d = b.card("Review");
    pulls::merged(&b.db, &b.you, &d, PR).unwrap();
    assert_eq!(b.task(&d).state_name, "Deploy");
    // …else Done.
    columns::remove_state(&b.db, &b.you, &b.st("Deploy"), &b.st("Done")).unwrap();
    let e = b.card("Review");
    pulls::merged(&b.db, &b.you, &e, PR).unwrap();
    assert_eq!(b.task(&e).state_name, "Done");
}
