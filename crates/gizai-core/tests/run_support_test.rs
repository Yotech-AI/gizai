use gizai_core::{db::Db, model::*, projects, runs, seed::ensure_seed, settings, tasks, team, workflow};

#[test]
fn settings_round_trip_typed_values() {
    let db = Db::open_in_memory().unwrap();
    ensure_seed(&db, "Jeffrey").unwrap();
    assert_eq!(settings::get::<String>(&db, "claude_bin").unwrap(), None);
    settings::set(&db, "claude_bin", &"/usr/bin/claude".to_string()).unwrap();
    settings::set(&db, "max_concurrent_runs", &3u32).unwrap();
    settings::set(&db, "max_concurrent_runs", &2u32).unwrap();
    assert_eq!(settings::get::<String>(&db, "claude_bin").unwrap().as_deref(), Some("/usr/bin/claude"));
    assert_eq!(settings::get::<u32>(&db, "max_concurrent_runs").unwrap(), Some(2));
}

#[test]
fn the_last_qa_issues_feed_the_next_prompt() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "CSV".into(), ..Default::default() }).unwrap();
    let qa = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    assert!(runs::last_qa_issues(&db, &t).unwrap().is_empty());
    let r = runs::create(&db, &qa, &t, "qa", "S", "/tmp", "/tmp", "b", "/tmp/l").unwrap();
    workflow::apply_outcome(&db, &r, Some(&Outcome { outcome: "qa_fail".into(), summary: "".into(), issues: vec!["Pin overlaps".into()] })).unwrap();
    assert_eq!(runs::last_qa_issues(&db, &t).unwrap(), vec!["Pin overlaps".to_string()]);
    let r2 = runs::create(&db, &qa, &t, "qa", "S2", "/tmp", "/tmp", "b", "/tmp/l2").unwrap();
    workflow::apply_outcome(&db, &r2, Some(&Outcome { outcome: "qa_pass".into(), summary: "".into(), issues: vec![] })).unwrap();
    assert!(runs::last_qa_issues(&db, &t).unwrap().is_empty(), "a pass clears them");
}

#[test]
fn all_agents_lists_agents_of_every_team_with_their_team() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let t2 = team::add_team(&db, &s.you_id, "Mobile").unwrap();
    team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "A".into(), role_key: "backend".into(), wakeup: "heartbeat".into(), heartbeat_minutes: Some(5), ..Default::default() }).unwrap();
    team::add_agent(&db, &s.you_id, &t2, AgentInput { name: "B".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    let all = team::all_agents(&db).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|(team_id, m)| team_id == &t2 && m.name == "B"));
    assert!(all.iter().all(|(_, m)| m.kind == "agent"));
}

#[test]
fn month_start_is_the_first_of_the_month_utc() {
    assert_eq!(runs::month_start_ms(1_791_288_000_000), 1_790_812_800_000); // 2026-10-06 12:00 → 2026-10-01
    assert_eq!(runs::month_start_ms(1_711_929_599_000), 1_709_251_200_000); // 2024-03-31 23:59:59 → 2024-03-01
    assert_eq!(runs::month_start_ms(1_733_011_200_000), 1_733_011_200_000); // 2024-12-01 00:00 exactly
}

#[test]
fn an_agents_spend_counts_its_runs_this_month() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let a = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "B".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    for i in 0..2 {
        let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: format!("T{i}"), ..Default::default() }).unwrap();
        let r = runs::create(&db, &a, &t, "backend", &format!("S{i}"), "/tmp", "/tmp", "b", "/tmp/l").unwrap();
        runs::finish(&db, &r, "succeeded", None, 420_000, 0, 0, None).unwrap();
    }
    let now = gizai_core::ids::now_ms();
    assert_eq!(runs::agent_spend_since(&db, &a, runs::month_start_ms(now)).unwrap(), 840_000);
    assert_eq!(runs::agent_spend_since(&db, &a, now + 1).unwrap(), 0);
}

#[test]
fn daily_stats_count_an_agents_runs_per_utc_day_for_the_last_n_days() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let a = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "B".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let mut ids = vec![];
    for (i, status) in ["succeeded", "failed", "succeeded"].iter().enumerate() {
        let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: format!("T{i}"), ..Default::default() }).unwrap();
        let r = runs::create(&db, &a, &t, "backend", &format!("S{i}"), "/tmp", "/tmp", "b", "/tmp/l").unwrap();
        runs::finish(&db, &r, status, None, 0, 0, 0, None).unwrap();
        ids.push(r);
    }
    // move the second run two days back
    let now = gizai_core::ids::now_ms();
    db.write(None, |w| { w.conn().execute("UPDATE runs SET created_at = ?2 WHERE id = ?1", rusqlite::params![ids[1], now - 2 * 86_400_000])?; Ok(()) }).unwrap();
    let days = runs::daily_stats(&db, &a, 14, now).unwrap();
    assert_eq!(days.len(), 14);
    assert_eq!(days.last().unwrap().day_start, now - now.rem_euclid(86_400_000));
    let today = days.last().unwrap();
    assert_eq!((today.succeeded, today.failed), (2, 0));
    assert_eq!((days[11].succeeded, days[11].failed), (0, 1));
    assert_eq!(days.iter().map(|d| d.succeeded + d.failed + d.other).sum::<i64>(), 3);
}

#[test]
fn an_agents_runs_are_listed_newest_first() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let a = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "B".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let mut ids = vec![];
    for i in 0..3 {
        let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: format!("T{i}"), ..Default::default() }).unwrap();
        ids.push(runs::create(&db, &a, &t, "backend", &format!("S{i}"), "/tmp", "/tmp", "b", "/tmp/l").unwrap());
    }
    let listed: Vec<String> = runs::list_for_agent(&db, &a, 2).unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(listed, vec![ids[2].clone(), ids[1].clone()]);
}
