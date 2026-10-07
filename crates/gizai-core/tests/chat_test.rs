use gizai_core::chat::{self, NewMessage, Totals};
use gizai_core::model::*;
use gizai_core::{db::Db, projects, runs, seed, tasks, team, tokens};
use serde_json::json;

fn setup() -> (Db, seed::SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    (db, s)
}

fn agent(db: &Db, s: &seed::SeedIds, name: &str, role: &str, chat: Option<bool>) -> String {
    team::add_agent(db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), chat_enabled: chat, ..Default::default() }).unwrap()
}

#[test]
fn chat_agent_is_none_until_one_has_chat() {
    let (db, s) = setup();
    agent(&db, &s, "Backend Agent", "backend", None);
    assert!(team::chat_agent(&db).unwrap().is_none());
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let m = team::chat_agent(&db).unwrap().unwrap();
    assert_eq!(m.actor_id, lead);
    assert!(m.chat_enabled);
}

#[test]
fn turning_chat_on_for_one_agent_turns_it_off_for_the_others() {
    let (db, s) = setup();
    let a = agent(&db, &s, "Team Lead", "lead", Some(true));
    let b = agent(&db, &s, "Second Lead", "lead", Some(true));
    assert!(!team::agent(&db, &a).unwrap().chat_enabled);
    assert!(team::agent(&db, &b).unwrap().chat_enabled);
    // on update: None leaves it alone, Some(true) moves it back
    let m = team::agent(&db, &a).unwrap();
    team::update_agent(&db, &s.you_id, &a, AgentInput { name: m.name.clone(), role_key: m.role_key.clone(), ..Default::default() }).unwrap();
    assert!(team::agent(&db, &b).unwrap().chat_enabled);
    team::update_agent(&db, &s.you_id, &a, AgentInput { name: m.name, role_key: m.role_key, chat_enabled: Some(true), ..Default::default() }).unwrap();
    assert!(team::agent(&db, &a).unwrap().chat_enabled);
    assert!(!team::agent(&db, &b).unwrap().chat_enabled);
}

#[test]
fn the_roles_include_design_and_devops() {
    assert_eq!(team::ROLES, ["lead", "frontend", "backend", "design", "qa", "devops"]);
}

#[test]
fn needs_you_lists_held_cards_and_review_cards_assigned_to_you() {
    let (db, s) = setup();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let t = team::get(&db, &s.team_id).unwrap();
    let col = |cat: &str| t.states.iter().find(|x| x.category == cat).unwrap().id.clone();
    let mk = |title: &str, cat: &str, assignee: Option<String>| tasks::create(&db, &s.you_id, TaskInput {
        project_id: p.clone(), title: title.into(), state_id: Some(col(cat)), assignee_id: assignee, ..Default::default() }).unwrap();
    let held = mk("Held", "ready", None);
    tasks::update(&db, &s.you_id, &held, TaskPatch { hold: Some("needs_decision".into()), hold_reason: Some("Which API?".into()), ..Default::default() }).unwrap();
    mk("Mine to review", "review", Some(s.you_id.clone()));
    let other = agent(&db, &s, "Backend Agent", "backend", None);
    mk("Someone else's review", "review", Some(other));
    let done_held = mk("Done but held", "done", None);
    tasks::update(&db, &s.you_id, &done_held, TaskPatch { hold: Some("blocked".into()), ..Default::default() }).unwrap();
    mk("Plain", "ready", None);
    let mut titles: Vec<String> = tasks::needs_you(&db, &s.you_id).unwrap().into_iter().map(|t| t.title).collect();
    titles.sort();
    assert_eq!(titles, ["Held", "Mine to review"]);
}

#[test]
fn a_minted_token_verifies_until_revoked_or_expired() {
    let (db, s) = setup();
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let tok = tokens::mint(&db, &lead, json!({"chat": "T1"}), 60_000).unwrap();
    assert_eq!(tok.len(), 64);
    let g = tokens::verify(&db, &tok).unwrap().unwrap();
    assert_eq!(g.actor_id, lead);
    assert_eq!(g.scope["chat"], "T1");
    assert!(tokens::verify(&db, "not-a-token").unwrap().is_none());
    tokens::revoke(&db, &tok).unwrap();
    assert!(tokens::verify(&db, &tok).unwrap().is_none());
    let expired = tokens::mint(&db, &lead, json!({}), 0).unwrap();
    assert!(tokens::verify(&db, &expired).unwrap().is_none());
    assert_ne!(tokens::mint(&db, &lead, json!({}), 1000).unwrap(), tokens::mint(&db, &lead, json!({}), 1000).unwrap());
}

#[test]
fn tokens_are_stored_hashed() {
    let (db, s) = setup();
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let tok = tokens::mint(&db, &lead, json!({}), 60_000).unwrap();
    let stored: Vec<String> = db.read(|c| {
        let mut st = c.prepare("SELECT token_sha256 || scopes_json FROM api_tokens")?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }).unwrap();
    assert_eq!(stored.len(), 1);
    assert!(!stored[0].contains(&tok));
}

#[test]
fn threads_list_newest_first_with_titles_cut_at_60() {
    let (db, s) = setup();
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let long = "Please plan the Kade portal: clients, invoices, the CSV export and the new dashboard for the warehouse";
    let a = chat::create_thread(&db, &s.you_id, &lead, "What needs me?").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    let b = chat::create_thread(&db, &s.you_id, &lead, long).unwrap();
    let list = chat::list_threads(&db).unwrap();
    assert_eq!(list.iter().map(|t| t.id.clone()).collect::<Vec<_>>(), [b.clone(), a.clone()]);
    assert_eq!(list[1].title, "What needs me?");
    assert!(list[0].title.chars().count() <= 61 && list[0].title.ends_with('…'), "{}", list[0].title);
    assert!(!list[0].title.contains("dashboard"));
    // a new message moves a thread to the top
    std::thread::sleep(std::time::Duration::from_millis(5));
    chat::add_message(&db, NewMessage { thread_id: a.clone(), role: "user".into(), author_id: Some(s.you_id.clone()), body_md: Some("Hi".into()), ..Default::default() }).unwrap();
    assert_eq!(chat::list_threads(&db).unwrap()[0].id, a);
}

#[test]
fn title_from_keeps_short_text_and_cuts_on_a_word() {
    assert_eq!(chat::title_from("  Hello\nthere  "), "Hello there");
    let t = chat::title_from(&"word ".repeat(30));
    assert!(t.ends_with('…') && !t.contains("  "));
    assert!(t.chars().count() <= 61);
    assert_eq!(chat::title_from(""), "New chat");
}

#[test]
fn messages_keep_insert_order_and_tool_results_merge_into_tool_json() {
    let (db, s) = setup();
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let th = chat::create_thread(&db, &s.you_id, &lead, "Add a task").unwrap();
    let m = |role: &str, body: Option<&str>| NewMessage { thread_id: th.clone(), role: role.into(), author_id: Some(lead.clone()), body_md: body.map(str::to_string), ..Default::default() };
    chat::add_message(&db, NewMessage { author_id: Some(s.you_id.clone()), ..m("user", Some("Add a task")) }).unwrap();
    chat::add_message(&db, m("agent", Some("Sure."))).unwrap();
    let tool = chat::add_message(&db, NewMessage { tool_name: Some("mcp__gizai__create_task".into()), tool: Some(json!({"id": "toolu_1", "input": {"title": "X"}})), ..m("tool", None) }).unwrap();
    chat::add_message(&db, m("agent", Some("Done."))).unwrap();
    let updated = chat::set_tool_result(&db, &tool.id, "{\"ok\":true}", false).unwrap();
    assert_eq!(updated.tool.as_ref().unwrap()["result"], "{\"ok\":true}");
    assert_eq!(updated.tool.as_ref().unwrap()["isError"], false);
    assert_eq!(updated.tool.as_ref().unwrap()["input"]["title"], "X");
    let all = chat::messages(&db, &th).unwrap();
    assert_eq!(all.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(), ["user", "agent", "tool", "agent"]);
    assert_eq!(all[0].author_name.as_deref(), Some("Jeffrey"));
    assert_eq!(all[1].author_name.as_deref(), Some("Team Lead"));
    assert_eq!(all[2].tool.as_ref().unwrap()["result"], "{\"ok\":true}");
    assert!(chat::add_message(&db, m("robot", Some("x"))).is_err());
}

#[test]
fn turn_cost_subtracts_the_previous_cumulative_totals_and_restarts_after_a_reset() {
    let prev = Totals { cost_usd_micros: 10_000, input_tokens: 1000, output_tokens: 100 };
    let now = Totals { cost_usd_micros: 25_000, input_tokens: 2600, output_tokens: 180 };
    assert_eq!(chat::turn_cost(prev, now), Totals { cost_usd_micros: 15_000, input_tokens: 1600, output_tokens: 80 });
    // a new session reports less than the old cumulative total: the turn cost is what it reports
    let fresh = Totals { cost_usd_micros: 3_000, input_tokens: 500, output_tokens: 20 };
    assert_eq!(chat::turn_cost(prev, fresh), fresh);
    assert_eq!(chat::turn_cost(Totals::default(), now), now);
}

#[test]
fn sessions_are_recorded_and_reset() {
    let (db, s) = setup();
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let th = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    assert!(chat::get_thread(&db, &th).unwrap().session_id.is_none());
    let t = Totals { cost_usd_micros: 7, input_tokens: 8, output_tokens: 9 };
    chat::record_session(&db, &th, "SESS", t).unwrap();
    let got = chat::get_thread(&db, &th).unwrap();
    assert_eq!(got.session_id.as_deref(), Some("SESS"));
    assert_eq!(got.totals(), t);
    chat::reset_session(&db, &th).unwrap();
    let got = chat::get_thread(&db, &th).unwrap();
    assert!(got.session_id.is_none());
    assert_eq!(got.totals(), Totals::default());
}

#[test]
fn a_chat_run_has_no_task_and_finishes_like_any_run() {
    let (db, s) = setup();
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let th = chat::create_thread(&db, &s.you_id, &lead, "Hi").unwrap();
    let r = runs::create_chat(&db, &lead, &th, "SESS", "/tmp/lead", "/tmp/r.jsonl").unwrap();
    let run = runs::get(&db, &r).unwrap();
    assert_eq!((run.trigger.as_str(), run.status.as_str(), run.task_id.as_deref(), run.role_key.as_deref()), ("chat", "queued", None, Some("lead")));
    runs::set_running(&db, &r, 4242).unwrap();
    runs::finish(&db, &r, "succeeded", None, 12_000, 10, 5, None).unwrap();
    let run = runs::get(&db, &r).unwrap();
    assert_eq!((run.status.as_str(), run.cost_usd_micros), ("succeeded", 12_000));
    assert_eq!(runs::agent_spend_since(&db, &lead, 0).unwrap(), 12_000);
    assert_eq!(runs::list_for_agent(&db, &lead, 5).unwrap().len(), 1);
}

#[test]
fn an_agent_keeps_its_effort_level_and_unknown_levels_are_refused() {
    let (db, s) = setup();
    let id = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Frontend Agent".into(), role_key: "frontend".into(),
        effort: Some("xhigh".into()), ..Default::default() }).unwrap();
    assert_eq!(team::agent(&db, &id).unwrap().effort.as_deref(), Some("xhigh"));
    team::update_agent(&db, &s.you_id, &id, AgentInput { name: "Frontend Agent".into(), role_key: "frontend".into(), effort: Some("max".into()), ..Default::default() }).unwrap();
    assert_eq!(team::agent(&db, &id).unwrap().effort.as_deref(), Some("max"));
    team::update_agent(&db, &s.you_id, &id, AgentInput { name: "Frontend Agent".into(), role_key: "frontend".into(), effort: Some(" ".into()), ..Default::default() }).unwrap();
    assert_eq!(team::agent(&db, &id).unwrap().effort, None, "empty means Claude Code's default");
    let e = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "X".into(), role_key: "qa".into(), effort: Some("extreme".into()), ..Default::default() }).unwrap_err();
    assert!(e.to_string().contains("low, medium, high, xhigh, max"), "{e}");
}
