use gizai_core::{db::Db, seed::ensure_seed, team};

#[test]
fn team_has_members_states_labels_rules() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let t = team::get(&db, &s.team_id).unwrap();
    assert_eq!(t.members.len(), 1, "only the user");
    assert_eq!(t.members[0].kind, "person");
    assert_eq!(t.states.len(), 6);
    assert_eq!(t.labels.len(), 4);
    assert!(t.rules.is_empty(), "no routing rules until Jeffrey adds them");
    assert_eq!(team::list(&db).unwrap().len(), 1);
}

#[test]
fn an_agent_takes_one_card_at_a_time_unless_told_more() {
    let db = gizai_core::db::Db::open_in_memory().unwrap();
    let s = gizai_core::seed::ensure_seed(&db, "Jeffrey").unwrap();
    let input = gizai_core::model::AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() };
    let id = gizai_core::team::add_agent(&db, &s.you_id, &s.team_id, input.clone()).unwrap();
    assert_eq!(gizai_core::team::agent(&db, &id).unwrap().max_runs, 1);
    gizai_core::team::update_agent(&db, &s.you_id, &id, gizai_core::model::AgentInput { max_runs: Some(6), ..input.clone() }).unwrap();
    assert_eq!(gizai_core::team::agent(&db, &id).unwrap().max_runs, 6);
    gizai_core::team::update_agent(&db, &s.you_id, &id, input.clone()).unwrap();
    assert_eq!(gizai_core::team::agent(&db, &id).unwrap().max_runs, 6, "unchanged when not given");
    for bad in [0, 11] {
        let e = gizai_core::team::update_agent(&db, &s.you_id, &id, gizai_core::model::AgentInput { max_runs: Some(bad), ..input.clone() }).unwrap_err();
        assert!(e.to_string().contains("1 and 10"), "{e}");
    }
}
