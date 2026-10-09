use gizai_core::{db::Db, model::*, projects, runs, seed::ensure_seed, settings, tasks, team, workflow};
use rusqlite_migration::{M, Migrations};

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

#[test]
fn a_runs_refused_tool_calls_are_saved_in_order_and_read_back_with_the_run() {
    // GA-48: Refused in this run (table run_refusals, migration 0010)
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let db = Db::open(&path).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: "CSV".into(), ..Default::default() }).unwrap();
    let a = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "B".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let r = runs::create(&db, &a, &t, "backend", "S", "/tmp", "/tmp", "b", "/tmp/l").unwrap();
    let t2 = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "PDF".into(), ..Default::default() }).unwrap();
    let other = runs::create(&db, &a, &t2, "backend", "S2", "/tmp", "/tmp", "b", "/tmp/l2").unwrap();
    assert!(runs::get(&db, &r).unwrap().refused.is_empty(), "none until the run reports any");

    let list = vec![
        Refusal { tool: "Bash".into(), input: "cat <<EOF\nhi\nEOF".into(), reason: "Heredoc with unquoted delimiter undergoes shell expansion".into() },
        Refusal { tool: "Write".into(), input: "/tmp/x.txt".into(), reason: String::new() },
    ];
    runs::set_refused(&db, &r, &list).unwrap();
    assert_eq!(runs::get(&db, &r).unwrap().refused, list);
    assert_eq!(runs::list_for_task(&db, &t).unwrap().into_iter().find(|x| x.id == r).unwrap().refused, list);
    assert_eq!(runs::list_for_agent(&db, &a, 10).unwrap().into_iter().find(|x| x.id == r).unwrap().refused, list);
    assert!(runs::get(&db, &other).unwrap().refused.is_empty(), "only that run's");
    // saved again: replaced, not added to
    runs::set_refused(&db, &r, &list[1..]).unwrap();
    assert_eq!(runs::get(&db, &r).unwrap().refused, list[1..]);
    assert!(matches!(runs::set_refused(&db, "no-such-run", &list), Err(gizai_core::Error::NotFound(_))));
    // the UI and the Team Lead get them as JSON: tool, input, reason
    let v = serde_json::to_value(runs::get(&db, &r).unwrap()).unwrap();
    assert_eq!(v["refused"], serde_json::json!([{"tool": "Write", "input": "/tmp/x.txt", "reason": ""}]));
    // and they stay after the database is opened again
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(runs::get(&db, &r).unwrap().refused, list[1..]);
}

#[test]
fn a_schema_9_database_gets_the_refusals_table_and_its_runs_keep_their_shape() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    // A genuine schema 9 database (v0.2.0), built from the migrations: GA-49's 0011 can't be rolled back by
    // dropping a table, so a current database can't be stepped back to 9.
    let (r, cols) = {
        let mut c = rusqlite::Connection::open(&path).unwrap();
        c.pragma_update(None, "foreign_keys", "OFF").unwrap();
        Migrations::new(vec![
            M::up(include_str!("../migrations/0001_init.sql")), M::up(include_str!("../migrations/0002_agents.sql")),
            M::up(include_str!("../migrations/0003_chat.sql")), M::up(include_str!("../migrations/0004_effort.sql")),
            M::up(include_str!("../migrations/0005_pull_requests.sql")), M::up(include_str!("../migrations/0006_worktree_prepare.sql")),
            M::up(include_str!("../migrations/0007_card_flow.sql")), M::up(include_str!("../migrations/0008_board_check.sql")),
            M::up(include_str!("../migrations/0009_agent_folders.sql")),
        ]).to_latest(&mut c).unwrap();
        c.execute_batch("
            INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
            INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
              ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active'), ('b', 1, 1, 'org', 'agent', 'B', 'b', 'active');
            INSERT INTO teams (id, created_at, updated_at, org_id, name, lead_actor_id) VALUES ('team', 1, 1, 'org', 'Software', 'you');
            INSERT INTO workflow_states (id, created_at, updated_at, team_id, name, category, sort_key) VALUES ('todo', 1, 1, 'team', 'To do', 'ready', 'a0');
            INSERT INTO tasks (id, created_at, updated_at, org_id, identifier, title, state_id, state_category, sort_key) VALUES
              ('t', 1, 1, 'org', 'YT-1', 'CSV', 'todo', 'ready', 'a0');
            INSERT INTO runs (id, created_at, updated_at, org_id, agent_actor_id, task_id, trigger, role_key, adapter, status, log_path, started_at) VALUES
              ('r', 2, 2, 'org', 'b', 't', 'manual', 'backend', 'claude_code', 'succeeded', '/tmp/l', 2);").unwrap();
        let mut st = c.prepare("SELECT name FROM pragma_table_info('runs')").unwrap();
        let cols = st.query_map([], |row| row.get(0)).unwrap().collect::<Result<Vec<String>, _>>().unwrap();
        ("r".to_string(), cols)
    };
    let db = Db::open(&path).unwrap();
    let (version, after): (i64, Vec<String>) = db.read(|c| {
        let mut st = c.prepare("SELECT name FROM pragma_table_info('runs')")?;
        let v = st.query_map([], |row| row.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok((c.query_row("PRAGMA user_version", [], |row| row.get(0))?, v))
    }).unwrap();
    assert_eq!(version, gizai_core::db::SCHEMA_VERSION);
    assert_eq!(after, cols, "the runs table keeps its columns");
    assert!(!cols.iter().any(|c| c.contains("refus")));
    assert!(runs::get(&db, &r).unwrap().refused.is_empty(), "an older run has none");
    runs::set_refused(&db, &r, &[Refusal { tool: "Bash".into(), input: "ls /tmp".into(), reason: String::new() }]).unwrap();
    assert_eq!(runs::get(&db, &r).unwrap().refused.len(), 1);
}
