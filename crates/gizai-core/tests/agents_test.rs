use gizai_core::{db::Db, model::*, seed::ensure_seed, team};
#[test]
fn add_agent_with_heartbeat_and_role_template() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let id = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Frontend Agent".into(), role_key: "frontend".into(),
        wakeup: "heartbeat".into(), heartbeat_minutes: Some(15), ..Default::default() }).unwrap();
    let t = team::get(&db, &s.team_id).unwrap();
    let m = t.members.iter().find(|m| m.actor_id == id).unwrap();
    assert_eq!((m.kind.as_str(), m.role_key.as_str(), m.adapter.as_deref()), ("agent", "frontend", Some("claude_code")));
    assert!(m.instructions_md.as_deref().unwrap().contains("GIZAI_RESULT"), "prefilled from the role template");
    assert!(team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: " ".into(), role_key: "qa".into(), ..Default::default() }).is_err());
    assert!(team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Beat".into(), role_key: "qa".into(), wakeup: "heartbeat".into(), heartbeat_minutes: Some(0), ..Default::default() }).is_err(), "heartbeat needs >= 1 minute");
}

#[test]
fn update_agent_keeps_instructions_unless_given_and_pausing_works() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let id = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    team::update_agent(&db, &s.you_id, &id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), wakeup: "on_assign".into(),
        allowed_tools: vec!["Bash(npm test:*)".into(), " ".into()], ..Default::default() }).unwrap();
    let m = team::get(&db, &s.team_id).unwrap().members.into_iter().find(|m| m.actor_id == id).unwrap();
    assert!(m.instructions_md.unwrap().contains("QA Agent"));
    assert_eq!((m.wakeup.as_deref(), m.allowed_tools), (Some("on_assign"), vec!["Bash(npm test:*)".to_string()]));
    team::set_agent_status(&db, &s.you_id, &id, "paused").unwrap();
    let m = team::get(&db, &s.team_id).unwrap().members.into_iter().find(|m| m.actor_id == id).unwrap();
    assert_eq!(m.status, "paused");
    assert!(team::set_agent_status(&db, &s.you_id, &id, "sleeping").is_err());
    assert!(team::set_agent_status(&db, &s.you_id, &s.you_id, "paused").is_err(), "people are not paused here");
}

#[test]
fn a_new_team_gets_the_seven_columns_and_no_agents() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let id = team::add_team(&db, &s.you_id, "Mobile team").unwrap();
    let t = team::get(&db, &id).unwrap();
    assert_eq!(t.states.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done"]);
    assert!(t.members.iter().all(|m| m.kind == "person"), "Jeffrey sets agents up himself");
    assert!(team::list(&db).unwrap().len() == 2);
    assert!(team::add_team(&db, &s.you_id, "  ").is_err());
}
