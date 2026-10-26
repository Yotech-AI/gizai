//! GA-225 QA, in core: a project's team.
//! - A new project joins the team given, else the first team; an edit without a team keeps it; an unknown team is refused.
//! - Moving a project moves all its cards, archived ones too: each to the new team's first column of the same category in
//!   board order, else its first Backlog column, else its first column, after the cards already there, in board order.
//! - Agents that aren't on the new team come off a card's assignee, pin and implementer; people stay.
//! - A move while one of its cards has a run live or starting is refused word for word, and nothing changes.
//! - One write, recorded in the changes log: each card's column and team, and the project's team and cards moved.
use gizai_core::db::Db;
use gizai_core::model::*;
use gizai_core::{projects, runs, seed, tasks, team, users};

struct B {
    db: Db,
    you: String,
    my: String,
    research: String,
}

/// "My team" (the first, with its seven columns) and "Research team" (made after it, seven columns, no agents).
fn setup() -> B {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    // teams are ordered by when they were made: never in the same millisecond as the first
    std::thread::sleep(std::time::Duration::from_millis(5));
    let research = team::add_team(&db, &s.you_id, "Research team").unwrap();
    B { db, you: s.you_id, my: s.team_id, research }
}

impl B {
    fn col(&self, team_id: &str, name: &str) -> String {
        team::get(&self.db, team_id).unwrap().states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
    }
    fn project(&self, name: &str, key: &str, team_id: Option<&str>) -> String {
        projects::create(&self.db, &self.you, ProjectInput { name: name.into(), key: key.into(), team_id: team_id.map(String::from), ..Default::default() })
            .unwrap()
    }
    fn card(&self, project: &str, team_id: &str, column: &str, title: &str) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: project.into(), title: title.into(), state_id: Some(self.col(team_id, column)),
            ..Default::default() }).unwrap()
    }
    /// Saves the project as the form does, with `team_id` (None: the team is left as it is).
    fn save(&self, project: &str, team_id: Option<&str>) -> gizai_core::Result<()> {
        projects::update(&self.db, &self.you, project, self.input(project, team_id))
    }
    fn input(&self, project: &str, team_id: Option<&str>) -> ProjectInput {
        let p = projects::get(&self.db, project).unwrap();
        ProjectInput { name: p.name, key: p.key, status: Some(p.status), color: p.color, team_id: team_id.map(String::from), ..Default::default() }
    }
    fn agent(&self, team_id: &str, name: &str) -> String {
        team::add_agent(&self.db, &self.you, team_id, AgentInput { name: name.into(), role_key: "backend".into(), ..Default::default() }).unwrap()
    }
    /// (state_id, state_category, deleted_at is set, sort_key) of a card, as the tasks table has it.
    fn place(&self, t: &str) -> (String, String, bool, String) {
        self.db.read(|c| Ok(c.query_row("SELECT state_id, state_category, deleted_at IS NOT NULL, sort_key FROM tasks WHERE id=?1", [t],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?)).unwrap()
    }
    /// (assignee, pin, implementer) of a card.
    fn actors(&self, t: &str) -> (Option<String>, Option<String>, Option<String>) {
        self.db.read(|c| Ok(c.query_row("SELECT assignee_actor_id, pinned_actor_id, implementer_actor_id FROM tasks WHERE id=?1", [t],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?)).unwrap()
    }
    fn set_actors(&self, t: &str, assignee: Option<&str>, pin: Option<&str>, implementer: Option<&str>) {
        self.db.read(|c| Ok(c.execute("UPDATE tasks SET assignee_actor_id=?2, pinned_actor_id=?3, implementer_actor_id=?4 WHERE id=?1",
            rusqlite::params![t, assignee, pin, implementer])?)).unwrap();
    }
    fn changes(&self) -> i64 {
        self.db.read(|c| Ok(c.query_row("SELECT count(*) FROM changes", [], |r| r.get(0))?)).unwrap()
    }
    /// The cards of a column in board order, by title.
    fn column_titles(&self, state: &str) -> Vec<String> {
        self.db.read(|c| {
            let mut st = c.prepare("SELECT title FROM tasks WHERE state_id=?1 ORDER BY sort_key")?;
            Ok(st.query_map([state], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?)
        }).unwrap()
    }
}

#[test]
fn a_new_project_joins_the_team_given_else_the_first_and_an_edit_without_a_team_keeps_it() {
    let b = setup();
    let ga = b.project("Giz AI", "GA", None);
    let r = b.project("Rover", "ROV", Some(&b.research));
    let p = projects::get(&b.db, &ga).unwrap();
    assert_eq!((p.team_id.as_deref(), p.team_name.as_deref()), (Some(b.my.as_str()), Some("My team")));
    let p = projects::get(&b.db, &r).unwrap();
    assert_eq!((p.team_id.as_deref(), p.team_name.as_deref()), (Some(b.research.as_str()), Some("Research team")));
    let listed: Vec<(String, Option<String>)> = projects::list(&b.db).unwrap().into_iter().map(|p| (p.key, p.team_name)).collect();
    assert_eq!(listed, vec![("ROV".into(), Some("Research team".into())), ("GA".into(), Some("My team".into()))]);

    // a card of the second team's project starts in that team's first column, and never in the first team's columns
    let t = tasks::create(&b.db, &b.you, TaskInput { project_id: r.clone(), title: "First".into(), ..Default::default() }).unwrap();
    assert_eq!(tasks::get(&b.db, &t).unwrap().state_id, b.col(&b.research, "Backlog"));
    let e = tasks::create(&b.db, &b.you, TaskInput { project_id: r.clone(), title: "Wrong board".into(), state_id: Some(b.col(&b.my, "To do")),
        ..Default::default() }).unwrap_err();
    assert!(e.to_string().contains("doesn't belong to this project's team"), "{e}");

    // an edit without a team keeps it
    let mut input = b.input(&r, None);
    input.name = "Rover 2".into();
    projects::update(&b.db, &b.you, &r, input).unwrap();
    let p = projects::get(&b.db, &r).unwrap();
    assert_eq!((p.name.as_str(), p.team_id.as_deref()), ("Rover 2", Some(b.research.as_str())));

    // a team that doesn't exist is refused, and no project is made
    let n = projects::list(&b.db).unwrap().len();
    let e = projects::create(&b.db, &b.you, ProjectInput { name: "Nowhere".into(), key: "NOW".into(), team_id: Some("no-such-team".into()),
        ..Default::default() }).unwrap_err();
    assert!(matches!(e, gizai_core::Error::NotFound(_)), "{e}");
    assert_eq!(projects::list(&b.db).unwrap().len(), n);
}

#[test]
fn a_project_without_a_team_of_its_own_is_the_first_teams_and_saving_it_there_moves_nothing() {
    let b = setup();
    let ga = b.project("Giz AI", "GA", None);
    let t = b.card(&ga, &b.my, "To do", "Stays");
    // an older project: no team of its own
    b.db.read(|c| Ok(c.execute("UPDATE projects SET team_id=NULL WHERE id=?1", [&ga])?)).unwrap();
    let p = projects::get(&b.db, &ga).unwrap();
    assert_eq!((p.team_id.as_deref(), p.team_name.as_deref()), (Some(b.my.as_str()), Some("My team")));
    let before = b.place(&t);
    let version = |t: &str| -> i64 { b.db.read(|c| Ok(c.query_row("SELECT version FROM tasks WHERE id=?1", [t], |r| r.get(0))?)).unwrap() };
    let v = version(&t);
    b.save(&ga, Some(&b.my)).unwrap();
    assert_eq!(b.place(&t), before);
    assert_eq!(version(&t), v, "the card isn't touched");
    assert!(tasks::activity(&b.db, &t).unwrap().iter().all(|e| e.diff.get("team").is_none()));
    assert_eq!(projects::team_move(&b.db, &ga, &b.research).unwrap(),
               projects::TeamMove { project: "Giz AI".into(), from: "My team".into(), to: "Research team".into(), cards: 1 });
}

#[test]
fn a_move_takes_every_card_archived_ones_too_to_the_first_column_of_the_same_kind_else_backlog() {
    let b = setup();
    // My team gets a Cancelled column, which Research team doesn't have
    team::add_state(&b.db, &b.you, &b.my, "Cancelled", &b.col(&b.my, "Done"), "cancelled").unwrap();
    // Research team: a work column Design before In progress (the first work column in board order), and no Deploy column
    team::add_state(&b.db, &b.you, &b.research, "Design", &b.col(&b.research, "To do"), "in_progress").unwrap();
    gizai_core::columns::remove_state(&b.db, &b.you, &b.col(&b.research, "Deploy"), &b.col(&b.research, "Review")).unwrap();
    let ga = b.project("Giz AI", "GA", None);
    let kade = b.project("Kade", "KADE", None);
    let rover = b.project("Rover", "ROV", Some(&b.research));
    let already = b.card(&rover, &b.research, "To do", "Rover's card");
    let backlog = b.card(&ga, &b.my, "Backlog", "Idea");
    let todo1 = b.card(&ga, &b.my, "To do", "First to do");
    let todo2 = b.card(&ga, &b.my, "To do", "Second to do");
    let working = b.card(&ga, &b.my, "In progress", "Working");
    let testing = b.card(&ga, &b.my, "Testing", "Testing");
    let review = b.card(&ga, &b.my, "Review", "Review");
    let deploy = b.card(&ga, &b.my, "Deploy", "Deploy");
    let done = b.card(&ga, &b.my, "Done", "Done and archived");
    tasks::archive(&b.db, &b.you, &done).unwrap();
    let cancelled = b.card(&ga, &b.my, "Cancelled", "Cancelled");
    let other = b.card(&kade, &b.my, "To do", "Another project's card");
    let other_before = b.place(&other);
    // the project form's question counts every card, the archived one too
    assert_eq!(projects::team_move(&b.db, &ga, &b.research).unwrap(),
               projects::TeamMove { project: "Giz AI".into(), from: "My team".into(), to: "Research team".into(), cards: 9 });
    // the board's cards, without the archived one, for the app to hold during the move
    let mut held = projects::card_ids(&b.db, &ga).unwrap();
    held.sort();
    let mut want = vec![backlog.clone(), todo1.clone(), todo2.clone(), working.clone(), testing.clone(), review.clone(), deploy.clone(), cancelled.clone()];
    want.sort();
    assert_eq!(held, want);

    b.save(&ga, Some(&b.research)).unwrap();

    let p = projects::get(&b.db, &ga).unwrap();
    assert_eq!((p.team_id.as_deref(), p.team_name.as_deref()), (Some(b.research.as_str()), Some("Research team")));
    let r = |name: &str| b.col(&b.research, name);
    for (card, column, category) in [(&backlog, "Backlog", "backlog"), (&todo1, "To do", "ready"), (&todo2, "To do", "ready"),
                                     (&working, "Design", "in_progress"), (&testing, "Testing", "testing"), (&review, "Review", "review"),
                                     (&deploy, "Backlog", "backlog"), (&done, "Done", "done"), (&cancelled, "Backlog", "backlog")] {
        let (state, cat, _, _) = b.place(card);
        assert_eq!((state, cat.as_str()), (r(column), category), "{} goes to {column}", tasks::get(&b.db, card).unwrap().title);
    }
    assert!(b.place(&done).2, "the archived card stays archived");
    // after the cards already there, in the board order they had
    assert_eq!(b.column_titles(&r("To do")), vec!["Rover's card", "First to do", "Second to do"]);
    assert_eq!(b.column_titles(&r("Backlog")), vec!["Idea", "Deploy", "Cancelled"]);
    assert_eq!(b.place(&already).0, r("To do"));
    // another project's card stays where it is
    assert_eq!(b.place(&other), other_before);
    // nothing of the project is left on My team's board
    let left: i64 = b.db.read(|c| Ok(c.query_row(
        "SELECT count(*) FROM tasks t JOIN workflow_states s ON s.id = t.state_id WHERE t.project_id=?1 AND s.team_id=?2",
        [&ga, &b.my], |r| r.get(0))?)).unwrap();
    assert_eq!(left, 0);
}

#[test]
fn a_team_without_a_backlog_column_takes_the_cards_of_a_missing_kind_in_its_first_column() {
    let b = setup();
    let ga = b.project("Giz AI", "GA", None);
    let deploy = b.card(&ga, &b.my, "Deploy", "Deploy");
    let review = b.card(&ga, &b.my, "Review", "Review");
    // Research team without Backlog and Deploy (the app keeps a last Backlog column, so this is made by hand)
    b.db.read(|c| Ok(c.execute("UPDATE workflow_states SET deleted_at=1 WHERE team_id=?1 AND name IN ('Backlog', 'Deploy')", [&b.research])?)).unwrap();
    b.save(&ga, Some(&b.research)).unwrap();
    assert_eq!(b.place(&deploy).0, b.col(&b.research, "To do"), "its first column");
    assert_eq!(b.place(&review).0, b.col(&b.research, "Review"));
}

#[test]
fn a_move_takes_agents_of_the_old_team_off_its_cards_and_people_and_the_new_teams_agents_stay() {
    let b = setup();
    let be = b.agent(&b.my, "Backend Agent");
    let ra = b.agent(&b.research, "Research Agent");
    let sam = users::create(&b.db, &b.you, "Sam", None).unwrap();
    let ga = b.project("Giz AI", "GA", None);
    let all_agents = b.card(&ga, &b.my, "In progress", "All the old team's agent");
    let mixed = b.card(&ga, &b.my, "Review", "People and the new team's agent");
    let nobody = b.card(&ga, &b.my, "To do", "Nobody on it");
    let archived = b.card(&ga, &b.my, "Done", "Archived, with the old team's agent");
    b.set_actors(&all_agents, Some(&be), Some(&be), Some(&be));
    b.set_actors(&mixed, Some(&sam), Some(&ra), Some(&be));
    b.set_actors(&archived, Some(&be), None, Some(&b.you));
    tasks::archive(&b.db, &b.you, &archived).unwrap();

    b.save(&ga, Some(&b.research)).unwrap();

    assert_eq!(b.actors(&all_agents), (None, None, None));
    assert_eq!(b.actors(&mixed), (Some(sam.clone()), Some(ra.clone()), None), "a person and the new team's agent stay");
    assert_eq!(b.actors(&nobody), (None, None, None));
    assert_eq!(b.actors(&archived), (None, None, Some(b.you.clone())), "on an archived card too; you stay");
    // the agent itself stays on its team
    assert!(team::get(&b.db, &b.my).unwrap().members.iter().any(|m| m.actor_id == be));

    // the changes log: each card's column and team, agentsOff when an agent came off
    let last = |t: &str| tasks::activity(&b.db, t).unwrap().into_iter().last().unwrap();
    let e = last(&all_agents);
    assert_eq!((e.table.as_str(), e.op.as_str()), ("tasks", "update"));
    assert_eq!(e.diff["team"], serde_json::json!(["My team", "Research team"]));
    assert_eq!(e.diff["column"], serde_json::json!(["In progress", "In progress"]));
    assert_eq!(e.diff["agentsOff"], true);
    assert_eq!(last(&mixed).diff["agentsOff"], true);
    assert_eq!(last(&nobody).diff["team"], serde_json::json!(["My team", "Research team"]));
    assert!(last(&nobody).diff.get("agentsOff").is_none(), "{}", last(&nobody).diff);
    assert_eq!(last(&archived).diff["column"], serde_json::json!(["Done", "Done"]));
    // and the project's: its team and how many cards moved
    let e = tasks::activity(&b.db, &ga).unwrap().into_iter().last().unwrap();
    assert_eq!((e.table.as_str(), e.op.as_str()), ("projects", "update"));
    assert_eq!(e.diff["team"], serde_json::json!(["My team", "Research team"]));
    assert_eq!(e.diff["cardsMoved"], 4);
}

#[test]
fn a_move_while_a_card_has_a_run_live_or_starting_is_refused_word_for_word_and_nothing_changes() {
    let b = setup();
    let be = b.agent(&b.my, "Backend Agent");
    let ga = b.project("Giz AI", "GA", None);
    let kade = b.project("Kade", "KADE", None);
    let cards: Vec<String> = (1..=10).map(|i| b.card(&ga, &b.my, "To do", &format!("Card {i}"))).collect();
    assert_eq!(tasks::get(&b.db, &cards[9]).unwrap().identifier, "GA-10");
    // another project's run doesn't stop the move
    let k = b.card(&kade, &b.my, "To do", "Kade's card");
    runs::create(&b.db, &be, &k, "backend", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/k.jsonl").unwrap();
    // a run in the runs table on GA-10, and GA-2 and GA-10 starting in the app: GA-2 is named, the first by number
    let run = runs::create(&b.db, &be, &cards[9], "backend", "S", "/tmp", "/tmp", "gizai/ga-10", "/tmp/r.jsonl").unwrap();
    let places: Vec<_> = cards.iter().map(|t| b.place(t)).collect();
    let n = b.changes();
    // renamed in the same save: the refusal names the project as saved, and the new name isn't saved either
    let mut input = b.input(&ga, Some(&b.research));
    input.name = "Giz AI 2".into();
    let e = projects::update_with(&b.db, &b.you, &ga, input.clone(), &[cards[9].clone(), cards[1].clone()]).unwrap_err();
    assert_eq!(e.to_string(), "Wait for GA-2's run to end, or stop it, before you move Giz AI to another team.");
    let e = projects::update_with(&b.db, &b.you, &ga, input.clone(), &[]).unwrap_err();
    assert_eq!(e.to_string(), "Wait for GA-10's run to end, or stop it, before you move Giz AI to another team.");
    let p = projects::get(&b.db, &ga).unwrap();
    assert_eq!((p.name.as_str(), p.team_id.as_deref()), ("Giz AI", Some(b.my.as_str())));
    assert_eq!(cards.iter().map(|t| b.place(t)).collect::<Vec<_>>(), places);
    assert_eq!(b.changes(), n, "nothing in the changes log");

    // a save that doesn't move the project isn't refused
    let mut keep = b.input(&ga, Some(&b.my));
    keep.name = "Giz AI".into();
    projects::update_with(&b.db, &b.you, &ga, keep, &[cards[1].clone()]).unwrap();

    // once the run has ended, it moves
    runs::finish(&b.db, &run, "cancelled", None, 0, 0, 0, None).unwrap();
    projects::update_with(&b.db, &b.you, &ga, input, &[]).unwrap();
    let p = projects::get(&b.db, &ga).unwrap();
    assert_eq!((p.name.as_str(), p.team_id.as_deref()), ("Giz AI 2", Some(b.research.as_str())));
    assert!(cards.iter().all(|t| b.place(t).0 == b.col(&b.research, "To do")));
}

#[test]
fn a_move_to_a_team_that_doesnt_exist_is_refused_and_nothing_changes() {
    let b = setup();
    let ga = b.project("Giz AI", "GA", None);
    let t = b.card(&ga, &b.my, "To do", "Stays");
    let before = b.place(&t);
    let e = b.save(&ga, Some("no-such-team")).unwrap_err();
    assert!(matches!(e, gizai_core::Error::NotFound(_)), "{e}");
    assert_eq!(b.place(&t), before);
    assert_eq!(projects::get(&b.db, &ga).unwrap().team_id.as_deref(), Some(b.my.as_str()));
}
