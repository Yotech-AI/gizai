//! GA-225 QA, end to end with a fake `claude`: several teams.
//! - With one team, the Team Lead's tools work without `team`, as before.
//! - With two, add_column, set_column and create_agent without `team` are refused word for word; with it, they change
//!   that team only. get_workflow without `team` lists each team's columns under its name; list_agents names each agent's
//!   team. create_project takes `team`; get_project and list_projects show it; update_project with `team` moves the
//!   project as the form does, and is refused word for word while one of its cards has a run live (nothing changes).
//! - The second team's agents pick up the cards of its projects in its Auto columns; the first team's agents don't. A
//!   moved project's waiting cards are picked up by the new team's agents.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::model::*;
use gizai_core::{projects, runs as core_runs, settings, tasks, team};
use gizai_lib::{AppState, runs, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const NAME_THE_TEAM: &str = "Name the team: My team or Research team.";

fn git_repo(dir: &Path, name: &str) -> PathBuf {
    let repo = dir.join(name);
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

struct App { st: AppState, lead: String, my: String, tmp: tempfile::TempDir }

/// One team ("My team") with the Team Lead; "Runs at once" 10.
fn app() -> App {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    settings::set(&st.db, "max_concurrent_runs", &10u32).unwrap();
    let my = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &my, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    App { st, lead, my, tmp }
}

impl App {
    /// A second team, "Research team".
    fn research(&self) -> String {
        // teams are ordered by when they were made: never in the same millisecond as the first
        std::thread::sleep(Duration::from_millis(5));
        team::add_team(&self.st.db, &self.st.you_id, "Research team").unwrap()
    }
    async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        tools::call(&self.st, &self.lead, name, args).await
    }
    async fn ok(&self, name: &str, args: Value) -> Value {
        self.call(name, args).await.unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }
    fn team(&self, id: &str) -> team::Team { team::get(&self.st.db, id).unwrap() }
    fn col(&self, team_id: &str, name: &str) -> String {
        self.team(team_id).states.iter().find(|s| s.name == name).unwrap_or_else(|| panic!("no column {name}")).id.clone()
    }
    fn column_names(&self, team_id: &str) -> Vec<String> { self.team(team_id).states.iter().map(|s| s.name.clone()).collect() }
    fn agent_names(&self, team_id: &str) -> Vec<String> {
        self.team(team_id).members.iter().filter(|m| m.kind == "agent").map(|m| m.name.clone()).collect()
    }
    fn agent(&self, team_id: &str, name: &str, max_runs: i64) -> String {
        team::add_agent(&self.st.db, &self.st.you_id, team_id, AgentInput { name: name.into(), role_key: "backend".into(), max_runs: Some(max_runs),
            ..Default::default() }).unwrap()
    }
    fn project(&self, name: &str, key: &str, team_id: Option<&str>) -> String {
        let repo = git_repo(self.tmp.path(), &key.to_lowercase()).to_string_lossy().into_owned();
        projects::create(&self.st.db, &self.st.you_id, ProjectInput { name: name.into(), key: key.into(), repo_path: Some(repo),
            team_id: team_id.map(String::from), ..Default::default() }).unwrap()
    }
    fn card(&self, project: &str, team_id: &str, column: &str, title: &str) -> String {
        tasks::create(&self.st.db, &self.st.you_id, TaskInput { project_id: project.into(), title: title.into(), description_md: "FAKE_HANG".into(),
            state_id: Some(self.col(team_id, column)), ..Default::default() }).unwrap()
    }
    fn live_agent(&self, t: &str) -> Option<String> { runs::live(&self.st).into_iter().find(|l| l.task_id == t).map(|l| l.agent_id) }
}

async fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---- one team: as before ----

#[tokio::test]
async fn with_one_team_the_tools_work_without_team_as_before() {
    let a = app();
    let w = a.ok("get_workflow", json!({})).await;
    assert_eq!(w["team"], "My team");
    assert_eq!(w["columns"].as_array().unwrap().len(), 7);
    assert!(w.get("teams").is_none(), "{w}");
    assert_eq!(w["labels"].as_array().unwrap().len(), 4);
    let r = a.ok("add_column", json!({"name": "Design", "after": "To do", "kind": "work"})).await;
    assert_eq!(r["ok"], true);
    assert_eq!(a.column_names(&a.my), ["Backlog", "To do", "Design", "In progress", "Testing", "Review", "Deploy", "Done"]);
    a.ok("set_column", json!({"column": "Design", "auto": false})).await;
    a.ok("create_agent", json!({"name": "Backend Agent", "role": "backend"})).await;
    assert_eq!(a.agent_names(&a.my), ["Backend Agent", "Team Lead"]);
    let list = a.ok("list_agents", json!({})).await;
    assert_eq!(list["count"], 2);
    assert!(list["agents"].as_array().unwrap().iter().all(|g| g.get("team").is_none()), "no team with one team: {list}");
    let p = a.ok("create_project", json!({"name": "Giz AI", "key": "GA"})).await;
    assert_eq!(p["project"]["team"], "My team");
}

// ---- two teams: the team is named ----

#[tokio::test]
async fn with_two_teams_add_column_set_column_and_create_agent_without_team_are_refused_word_for_word_and_nothing_changes() {
    let a = app();
    let rt = a.research();
    let before = (a.column_names(&a.my), a.column_names(&rt), a.agent_names(&a.my), a.agent_names(&rt));
    for (tool, args) in [("add_column", json!({"name": "Design", "after": "To do", "kind": "work"})),
                         ("set_column", json!({"column": "To do", "auto": false})),
                         ("create_agent", json!({"name": "Backend Agent", "role": "backend"}))] {
        assert_eq!(a.call(tool, args).await.unwrap_err(), NAME_THE_TEAM, "{tool}");
    }
    assert_eq!((a.column_names(&a.my), a.column_names(&rt), a.agent_names(&a.my), a.agent_names(&rt)), before);
    assert!(a.team(&a.my).states.iter().chain(a.team(&rt).states.iter()).filter(|s| s.name == "To do").all(|s| s.auto));
    // a team that doesn't exist says which do
    let e = a.call("add_column", json!({"team": "Mobile", "name": "Design", "after": "To do", "kind": "work"})).await.unwrap_err();
    assert!(e.contains("My team") && e.contains("Research team"), "{e}");
}

#[tokio::test]
async fn with_team_add_column_set_column_and_create_agent_change_that_team_only() {
    let a = app();
    let rt = a.research();
    let r = a.ok("add_column", json!({"team": "Research team", "name": "Design", "after": "To do", "kind": "work"})).await;
    assert_eq!(r["team"], "Research team");
    assert_eq!(a.column_names(&rt), ["Backlog", "To do", "Design", "In progress", "Testing", "Review", "Deploy", "Done"]);
    assert_eq!(a.column_names(&a.my), ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done"]);
    // by id, and by a part of its name, ignoring case
    a.ok("create_agent", json!({"team": rt, "name": "Research Agent", "role": "backend"})).await;
    assert_eq!(a.agent_names(&rt), ["Research Agent"]);
    assert_eq!(a.agent_names(&a.my), ["Team Lead"]);
    a.ok("set_column", json!({"team": "research", "column": "To do", "auto": false})).await;
    a.ok("set_column", json!({"team": "Research team", "column": "Design", "agents": ["Research Agent"]})).await;
    let to_do = |t: &str| a.team(t).states.into_iter().find(|s| s.name == "To do").unwrap();
    assert!(!to_do(&rt).auto);
    assert!(to_do(&a.my).auto, "My team's To do stays Auto");
    let ra = a.team(&rt).members.iter().find(|m| m.name == "Research Agent").unwrap().actor_id.clone();
    assert_eq!(a.team(&rt).states.iter().find(|s| s.name == "Design").unwrap().agent_ids, [ra]);
    // My team's columns take only My team's agents
    assert!(a.call("set_column", json!({"team": "My team", "column": "In progress", "agents": ["Research Agent"]})).await.is_err());
    assert!(a.team(&a.my).states.iter().all(|s| s.agent_ids.is_empty()));
    // and the agent works only its own team's columns
    let list = a.ok("list_agents", json!({})).await;
    let teams: Vec<(String, String)> = list["agents"].as_array().unwrap().iter()
        .map(|g| (g["name"].as_str().unwrap().to_string(), g["team"].as_str().unwrap().to_string())).collect();
    assert_eq!(teams, [("Team Lead".to_string(), "My team".to_string()), ("Research Agent".to_string(), "Research team".to_string())]);
    let only = a.ok("list_agents", json!({"team": "Research team"})).await;
    assert_eq!(only["count"], 1);
    assert_eq!(only["agents"][0]["name"], "Research Agent");
}

#[tokio::test]
async fn get_workflow_without_team_lists_each_teams_columns_under_its_name_and_with_team_that_teams() {
    let a = app();
    let rt = a.research();
    a.ok("add_column", json!({"team": "Research team", "name": "Design", "after": "To do", "kind": "work"})).await;
    let w = a.ok("get_workflow", json!({})).await;
    let teams = w["teams"].as_array().unwrap();
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0]["team"], "My team");
    assert_eq!(teams[0]["columns"].as_array().unwrap().len(), 7);
    assert_eq!(teams[1]["team"], "Research team");
    let names: Vec<&str> = teams[1]["columns"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Backlog", "To do", "Design", "In progress", "Testing", "Review", "Deploy", "Done"]);
    // the labels, which all teams share
    assert_eq!(w["labels"].as_array().unwrap().len(), 4);
    let one = a.ok("get_workflow", json!({"team": rt})).await;
    assert_eq!(one["team"], "Research team");
    assert_eq!(one["columns"].as_array().unwrap().len(), 8);
    assert_eq!(one["labels"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn create_project_with_team_get_project_and_list_projects_show_it_and_its_cards_get_that_teams_columns() {
    let a = app();
    let rt = a.research();
    let r = a.ok("create_project", json!({"name": "Rover", "key": "ROV", "team": "Research team"})).await;
    assert_eq!(r["project"]["team"], "Research team");
    a.ok("create_project", json!({"name": "Giz AI", "key": "GA"})).await;
    let p = a.ok("get_project", json!({"project": "ROV"})).await;
    assert_eq!(p["project"]["team"], "Research team");
    let list = a.ok("list_projects", json!({})).await;
    let teams: Vec<(String, String)> = list["projects"].as_array().unwrap().iter()
        .map(|p| (p["key"].as_str().unwrap().to_string(), p["team"].as_str().unwrap().to_string())).collect();
    // newest first
    assert_eq!(teams, [("GA".to_string(), "My team".to_string()), ("ROV".to_string(), "Research team".to_string())]);
    let rover = projects::get(&a.st.db, r["project"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(rover.team_id.as_deref(), Some(rt.as_str()));
    // its cards get the Research team's columns
    let t = a.ok("create_task", json!({"project": "ROV", "title": "Map the moon", "column": "To do"})).await;
    let id = tasks::id_of(&a.st.db, t["task"]["identifier"].as_str().unwrap()).unwrap();
    assert_eq!(tasks::get(&a.st.db, &id).unwrap().state_id, a.col(&rt, "To do"));
}

#[tokio::test]
async fn update_project_with_team_moves_it_as_the_form_does_and_a_live_run_refuses_it_word_for_word() {
    let a = app();
    let rt = a.research();
    let be = a.agent(&a.my, "Backend Agent", 3);
    let ga = a.project("Giz AI", "GA", None);
    let working = a.card(&ga, &a.my, "In progress", "Working");
    let waiting = a.card(&ga, &a.my, "Review", "Waiting for review");
    tasks::update(&a.st.db, &a.st.you_id, &waiting, TaskPatch { assignee_id: Some(be.clone()), ..Default::default() }).unwrap();
    let (run, _) = runs::start(&a.st, &working, None, Some(FAKE.to_string()), "manual").await.unwrap();
    until("the run is live", 30, || a.live_agent(&working).is_some()).await;
    let before = (tasks::get(&a.st.db, &working).unwrap().state_id, tasks::get(&a.st.db, &waiting).unwrap().state_id);

    let e = a.call("update_project", json!({"project": "GA", "team": "Research team", "name": "Giz AI 2"})).await.unwrap_err();
    assert_eq!(e, "Wait for GA-1's run to end, or stop it, before you move Giz AI to another team.");
    let p = projects::get(&a.st.db, &ga).unwrap();
    assert_eq!((p.name.as_str(), p.team_name.as_deref()), ("Giz AI", Some("My team")));
    assert_eq!((tasks::get(&a.st.db, &working).unwrap().state_id, tasks::get(&a.st.db, &waiting).unwrap().state_id), before);
    // the app's save (the project form) is refused the same way
    let e = runs::update_project(&a.st, &a.st.you_id, &ga, ProjectInput { name: "Giz AI".into(), key: "GA".into(), team_id: Some(rt.clone()),
        ..Default::default() }).unwrap_err();
    assert_eq!(e.to_string(), "Wait for GA-1's run to end, or stop it, before you move Giz AI to another team.");

    // paused, so the stopped card isn't picked up again before the move
    team::set_agent_status(&a.st.db, &a.st.you_id, &be, "paused").unwrap();
    runs::stop(&a.st, &run);
    until("the run stops", 30, || a.live_agent(&working).is_none()).await;
    let r = a.ok("update_project", json!({"project": "GA", "team": "Research team"})).await;
    assert_eq!(r["project"]["team"], "Research team");
    assert_eq!(r["done"], "moved to Research team with all its cards");
    let w = tasks::get(&a.st.db, &working).unwrap();
    assert_eq!(a.team(&rt).states.iter().find(|s| s.id == w.state_id).unwrap().category, w.state_category);
    let t = tasks::get(&a.st.db, &waiting).unwrap();
    assert_eq!(t.state_id, a.col(&rt, "Review"));
    assert_eq!(t.assignee_id, None, "My team's agent is taken off it");
    let p = a.ok("get_project", json!({"project": "GA"})).await;
    assert_eq!(p["project"]["team"], "Research team");
    // the same team again moves nothing and says nothing of a move
    let r = a.ok("update_project", json!({"project": "GA", "team": "Research team"})).await;
    assert!(r.get("done").is_none(), "{r}");
    runs::stop_all(&a.st, Duration::from_secs(12)).await;
}

#[tokio::test]
async fn a_board_check_never_moves_a_project_to_another_team() {
    let a = app();
    a.research();
    let ga = a.project("Giz AI", "GA", None);
    let e = tools::call_check(&a.st, &a.lead, "check-run", "update_project", json!({"project": "GA", "team": "Research team"})).await.unwrap_err();
    assert!(e.starts_with("A board check never moves a project to another team"), "{e}");
    assert_eq!(projects::get(&a.st.db, &ga).unwrap().team_name.as_deref(), Some("My team"));
}

// ---- the queue: each team's agents work its own cards ----

#[tokio::test]
async fn the_second_teams_agents_pick_up_the_cards_in_its_auto_columns_and_the_first_teams_dont() {
    let a = app();
    let rt = a.research();
    // the same role in each team, each on its own team's To do and In progress (Auto)
    let be = a.agent(&a.my, "Backend Agent", 3);
    let ra = a.agent(&rt, "Research Agent", 3);
    let ga = a.project("Giz AI", "GA", None);
    let rover = a.project("Rover", "ROV", Some(&rt));
    let r1 = a.card(&rover, &rt, "To do", "Rover one");
    let r2 = a.card(&rover, &rt, "To do", "Rover two");
    let g1 = a.card(&ga, &a.my, "To do", "Giz one");
    runs::pull(&a.st).await;
    until("the three cards start", 30, || [&r1, &r2, &g1].iter().all(|t| a.live_agent(t).is_some())).await;
    assert_eq!(a.live_agent(&r1).as_deref(), Some(ra.as_str()));
    assert_eq!(a.live_agent(&r2).as_deref(), Some(ra.as_str()));
    assert_eq!(a.live_agent(&g1).as_deref(), Some(be.as_str()));
    for t in [&r1, &r2] {
        assert!(core_runs::list_for_task(&a.st.db, t).unwrap().iter().all(|r| r.agent_id == ra), "only the Research team's agent");
    }
    runs::stop_all(&a.st, Duration::from_secs(12)).await;
}

#[tokio::test]
async fn a_moved_projects_waiting_cards_are_picked_up_by_the_new_teams_agents() {
    let a = app();
    let rt = a.research();
    let be = a.agent(&a.my, "Backend Agent", 3);
    team::set_agent_status(&a.st.db, &a.st.you_id, &be, "paused").unwrap();
    let ra = a.agent(&rt, "Research Agent", 3);
    let ga = a.project("Giz AI", "GA", None);
    let t = a.card(&ga, &a.my, "To do", "Waits in My team");
    runs::pull(&a.st).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(a.live_agent(&t).is_none(), "My team's only agent is paused, and the Research team's doesn't take My team's cards");
    a.ok("update_project", json!({"project": "GA", "team": "Research team"})).await;
    assert_eq!(tasks::get(&a.st.db, &t).unwrap().state_id, a.col(&rt, "To do"));
    until("the Research team's agent takes the moved card", 30, || a.live_agent(&t).is_some()).await;
    assert_eq!(a.live_agent(&t).as_deref(), Some(ra.as_str()));
    runs::stop_all(&a.st, Duration::from_secs(12)).await;
}
