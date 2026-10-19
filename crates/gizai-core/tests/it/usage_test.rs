// GA-33: the Usage page's sums (usage.rs) and the AI usage of the Projects list (projects.rs).
use gizai_core::model::*;
use gizai_core::usage::{self, Usage, UsageTotals};
use gizai_core::{board, chat, db::Db, ids, projects, runs, seed, tasks, team};

const DAY: i64 = 86_400_000;
/// 2026-10-01 00:00 UTC.
const OCT_1: i64 = 1_790_812_800_000;
/// 2026-10-09 00:00 UTC.
const OCT_9: i64 = OCT_1 + 8 * DAY;
/// 2026-10-09 15:00 UTC: "now" in these tests.
const NOW: i64 = OCT_9 + 15 * 3_600_000;

fn setup() -> (Db, seed::SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    (db, s)
}

fn agent(db: &Db, s: &seed::SeedIds, name: &str, role: &str, chat: Option<bool>) -> String {
    team::add_agent(db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), chat_enabled: chat, ..Default::default() }).unwrap()
}

fn project(db: &Db, s: &seed::SeedIds, name: &str, key: &str) -> String {
    projects::create(db, &s.you_id, ProjectInput { name: name.into(), key: key.into(), ..Default::default() }).unwrap()
}

fn card(db: &Db, s: &seed::SeedIds, project_id: &str, title: &str) -> String {
    tasks::create(db, &s.you_id, TaskInput { project_id: project_id.into(), title: title.into(), ..Default::default() }).unwrap()
}

/// Moves a run to `at` (its created_at).
fn at(db: &Db, run: &str, at: i64) {
    db.write(None, |w| { w.conn().execute("UPDATE runs SET created_at = ?2 WHERE id = ?1", rusqlite::params![run, at])?; Ok(()) }).unwrap();
}

/// A finished run of `agent` on `task` with this cost and these tokens, created at `when` (None: now).
fn card_run(db: &Db, agent: &str, task: &str, cost: i64, input: i64, output: i64, when: Option<i64>) -> String {
    let r = runs::create(db, agent, task, "backend", &ids::new_id(), "/tmp", "/tmp", "b", "/tmp/l").unwrap();
    runs::finish(db, &r, "succeeded", None, cost, input, output, None).unwrap();
    if let Some(w) = when {
        at(db, &r, w);
    }
    r
}

/// A finished chat turn of the Team Lead.
fn chat_turn(db: &Db, s: &seed::SeedIds, lead: &str, status: &str, cost: i64, input: i64, output: i64, when: Option<i64>) -> String {
    let th = chat::create_thread(db, &s.you_id, lead, "Hi").unwrap();
    let r = runs::create_chat(db, lead, &th, &ids::new_id(), "/tmp/lead", "/tmp/r.jsonl").unwrap();
    runs::finish_chat(db, &r, status, cost, input, output, None).unwrap();
    if let Some(w) = when {
        at(db, &r, w);
    }
    r
}

fn totals(runs: i64, chat_turns: i64, input_tokens: i64, output_tokens: i64, cost_usd_micros: i64, unknown_cost_runs: i64) -> UsageTotals {
    UsageTotals { runs, chat_turns, input_tokens, output_tokens, cost_usd_micros, unknown_cost_runs }
}

/// The agents add up to the total, the projects with the chat and no-project lines too, and so do the days.
fn assert_tabs_agree(u: &Usage) {
    let agents: UsageTotals = u.agents.iter().map(|a| a.totals).sum();
    assert_eq!(agents, u.total, "the agents add up to the total");
    let projects: UsageTotals = u.projects.iter().map(|p| p.totals).sum::<UsageTotals>();
    let mut with_chat = projects;
    with_chat += u.chat;
    with_chat += u.no_project;
    assert_eq!(with_chat, u.total, "the projects, the chat line and the no-project line add up to the total");
    let days: UsageTotals = u.days.iter().map(|d| d.totals).sum();
    assert_eq!(days, u.total, "the days add up to the total");
}

struct World {
    db: Db,
    kade: String,
    mobi: String,
    backend: String,
    frontend: String,
    codex: String,
    lead: String,
}

/// Runs and chat turns spread over September and October 2026 (see the comments for each).
fn world() -> World {
    let (db, s) = setup();
    let kade = project(&db, &s, "Kade", "KADE");
    let mobi = project(&db, &s, "Mobi", "MOBI");
    let backend = agent(&db, &s, "Backend Agent", "backend", None);
    let frontend = agent(&db, &s, "Frontend Agent", "frontend", None);
    let codex = agent(&db, &s, "Codex Agent", "backend", None);
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    // A card whose project is gone (tasks.project_id is NULL).
    let loose = card(&db, &s, &kade, "Loose");
    db.write(None, |w| { w.conn().execute("UPDATE tasks SET project_id = NULL WHERE id = ?1", [&loose])?; Ok(()) }).unwrap();

    // Today (9 Oct).
    card_run(&db, &backend, &card(&db, &s, &kade, "K1"), 400_000, 1_000, 100, Some(OCT_9 + 10 * 3_600_000));
    card_run(&db, &codex, &card(&db, &s, &kade, "K2"), 0, 300, 30, Some(OCT_9 + 11 * 3_600_000)); // Codex: tokens, no cost
    chat_turn(&db, &s, &lead, "succeeded", 12_000, 80, 20, Some(OCT_9 + 12 * 3_600_000));
    let check = board::create_run(&db, &lead, &ids::new_id(), "/tmp/lead", "/tmp/c.jsonl", &[]).unwrap();
    board::finish_run(&db, &check, "succeeded", 5_000, 40, 10, None, None).unwrap();
    at(&db, &check, OCT_9 + 13 * 3_600_000);
    // A run whose cost is NULL (not 0): unknown too.
    let null = card_run(&db, &codex, &card(&db, &s, &mobi, "M3"), 0, 10, 0, Some(OCT_9 + 14 * 3_600_000));
    db.write(None, |w| { w.conn().execute("UPDATE runs SET cost_usd_micros = NULL WHERE id = ?1", [&null])?; Ok(()) }).unwrap();
    // Earlier this week.
    card_run(&db, &frontend, &card(&db, &s, &mobi, "M1"), 250_000, 600, 60, Some(OCT_9 - 2 * DAY + 9 * 3_600_000)); // 7 Oct
    chat_turn(&db, &s, &lead, "cancelled", 0, 0, 0, Some(OCT_9 - 3 * DAY)); // 6 Oct: stopped before it used anything
    card_run(&db, &backend, &loose, 30_000, 50, 5, Some(OCT_9 - 4 * DAY + 1)); // 5 Oct, a card without a project
    // The first moment of this month (1 Oct 00:00), and the last of September.
    card_run(&db, &backend, &card(&db, &s, &kade, "K3"), 1_000_000, 2_000, 200, Some(OCT_1));
    card_run(&db, &frontend, &card(&db, &s, &mobi, "M2"), 2_000_000, 9_000, 900, Some(OCT_1 - 1));
    // 9 Sep: just before the last 30 days.
    card_run(&db, &backend, &card(&db, &s, &kade, "K4"), 7_000_000, 70_000, 7_000, Some(OCT_9 - 30 * DAY + 12 * 3_600_000));
    World { db, kade, mobi, backend, frontend, codex, lead }
}

#[test]
fn a_period_is_whole_utc_days_up_to_the_end_of_today() {
    let end = OCT_9 + DAY;
    assert_eq!(usage::period_range("today", NOW).unwrap(), (OCT_9, end));
    assert_eq!(usage::period_range("7d", NOW).unwrap(), (OCT_9 - 6 * DAY, end));
    assert_eq!(usage::period_range("30d", NOW).unwrap(), (OCT_9 - 29 * DAY, end));
    assert_eq!(usage::period_range("month", NOW).unwrap(), (OCT_1, end));
    // On the 1st, this month is today; at midnight exactly, today starts then.
    assert_eq!(usage::period_range("month", OCT_1).unwrap(), (OCT_1, OCT_1 + DAY));
    assert_eq!(usage::period_range("today", OCT_1).unwrap(), (OCT_1, OCT_1 + DAY));
    for p in usage::PERIODS {
        assert!(usage::period_range(p, NOW).is_ok(), "{p}");
    }
    assert!(matches!(usage::period_range("year", NOW), Err(gizai_core::Error::Invalid(_))));
    assert!(matches!(usage::for_period(&Db::open_in_memory().unwrap(), "", NOW), Err(gizai_core::Error::Invalid(_))));
}

#[test]
fn a_period_that_ends_before_it_starts_is_refused() {
    let (db, _) = setup();
    assert!(matches!(usage::summary(&db, OCT_9, OCT_9), Err(gizai_core::Error::Invalid(_))));
    assert!(matches!(usage::summary(&db, OCT_9, OCT_1), Err(gizai_core::Error::Invalid(_))));
}

#[test]
fn no_runs_is_all_zero_with_a_day_for_every_day() {
    let (db, _) = setup();
    let u = usage::for_period(&db, "month", NOW).unwrap();
    assert_eq!((u.since, u.until), (OCT_1, OCT_9 + DAY));
    assert_eq!(u.total, UsageTotals::default());
    assert_eq!(u.days.len(), 9, "1 to 9 October");
    assert!(u.days.iter().all(|d| d.totals == UsageTotals::default()));
    assert!(u.agents.is_empty() && u.projects.is_empty());
    assert_eq!((u.chat, u.no_project), (UsageTotals::default(), UsageTotals::default()));
}

#[test]
fn today_sums_the_runs_chat_turns_and_board_checks_with_unknown_costs_left_out() {
    let w = world();
    let u = usage::for_period(&w.db, "today", NOW).unwrap();
    assert_eq!((u.since, u.until), (OCT_9, OCT_9 + DAY));
    // 2 Claude Code runs' worth of cost, the chat turn and the board check; the Codex run and the NULL-cost run are unknown.
    assert_eq!(u.total, totals(5, 1, 1_430, 160, 417_000, 2));
    assert_eq!(u.days.len(), 1);
    assert_eq!(u.days[0].day_start, OCT_9);
    assert_tabs_agree(&u);

    // Per agent, the highest cost first: the Team Lead's line holds its chat turn and its board check.
    let names: Vec<&str> = u.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Backend Agent", "Team Lead", "Codex Agent"]);
    let of = |id: &str| u.agents.iter().find(|a| a.agent_id == id).unwrap();
    assert_eq!(of(&w.backend).totals, totals(1, 0, 1_000, 100, 400_000, 0));
    assert_eq!(of(&w.lead).totals, totals(2, 1, 120, 30, 17_000, 0));
    assert_eq!(of(&w.lead).role_key.as_deref(), Some("lead"));
    assert_eq!(of(&w.codex).totals, totals(2, 0, 310, 30, 0, 2), "its tokens count, its cost is unknown");
    assert!(u.agents.iter().all(|a| a.agent_id != w.frontend), "an agent without runs in the period has no line");

    // Per project: the chat turn and the board check are the chat line, not a project.
    let kade = u.projects.iter().find(|p| p.project_id == w.kade).unwrap();
    assert_eq!((kade.key.as_str(), kade.name.as_str()), ("KADE", "Kade"));
    assert_eq!(kade.totals, totals(2, 0, 1_300, 130, 400_000, 1));
    let mobi = u.projects.iter().find(|p| p.project_id == w.mobi).unwrap();
    assert_eq!(mobi.totals, totals(1, 0, 10, 0, 0, 1), "a NULL cost is unknown, not $0");
    assert_eq!(u.projects[0].project_id, w.kade, "the highest cost first");
    assert_eq!(u.chat, totals(2, 1, 120, 30, 17_000, 0));
    assert_eq!(u.no_project, UsageTotals::default());
}

#[test]
fn seven_days_count_a_card_without_a_project_and_a_chat_turn_without_tokens() {
    let w = world();
    let u = usage::for_period(&w.db, "7d", NOW).unwrap();
    assert_eq!((u.since, u.until), (OCT_9 - 6 * DAY, OCT_9 + DAY));
    assert_eq!(u.total, totals(8, 2, 2_080, 225, 697_000, 2));
    assert_tabs_agree(&u);
    // The stopped chat turn used nothing: it counts as a turn, not as an unknown cost.
    assert_eq!(u.chat, totals(3, 2, 120, 30, 17_000, 0));
    assert_eq!(u.no_project, totals(1, 0, 50, 5, 30_000, 0));
    assert_eq!(u.agents.iter().find(|a| a.agent_id == w.frontend).unwrap().totals, totals(1, 0, 600, 60, 250_000, 0));

    // A day for each of the 7 days, oldest first; days without runs are zero.
    let starts: Vec<i64> = u.days.iter().map(|d| d.day_start).collect();
    assert_eq!(starts, (0..7).map(|i| OCT_9 - (6 - i) * DAY).collect::<Vec<_>>());
    let day = |ms: i64| u.days.iter().find(|d| d.day_start == ms).unwrap().totals;
    assert_eq!(day(OCT_9), totals(5, 1, 1_430, 160, 417_000, 2));
    assert_eq!(day(OCT_9 - DAY), UsageTotals::default());
    assert_eq!(day(OCT_9 - 2 * DAY), totals(1, 0, 600, 60, 250_000, 0));
    assert_eq!(day(OCT_9 - 3 * DAY), totals(1, 1, 0, 0, 0, 0));
    assert_eq!(day(OCT_9 - 4 * DAY), totals(1, 0, 50, 5, 30_000, 0));
}

#[test]
fn this_month_starts_on_the_first_at_midnight_utc() {
    let w = world();
    let u = usage::for_period(&w.db, "month", NOW).unwrap();
    assert_eq!((u.since, u.until), (OCT_1, OCT_9 + DAY));
    // The run at 1 Oct 00:00 counts, the one a millisecond earlier doesn't.
    assert_eq!(u.total, totals(9, 2, 4_080, 425, 1_697_000, 2));
    assert_eq!(u.days.len(), 9);
    assert_eq!(u.days[0].totals, totals(1, 0, 2_000, 200, 1_000_000, 0));
    assert_tabs_agree(&u);
    assert_eq!(u.projects.iter().find(|p| p.project_id == w.kade).unwrap().totals, totals(3, 0, 3_300, 330, 1_400_000, 1));
}

#[test]
fn thirty_days_reach_back_into_last_month_but_not_further() {
    let w = world();
    let u = usage::for_period(&w.db, "30d", NOW).unwrap();
    assert_eq!((u.since, u.until), (OCT_9 - 29 * DAY, OCT_9 + DAY));
    assert_eq!(u.days.len(), 30);
    // Last month's run counts; the one on 9 Sep (7,000,000) doesn't.
    assert_eq!(u.total, totals(10, 2, 13_080, 1_325, 3_697_000, 2));
    assert_tabs_agree(&u);
    assert_eq!(u.projects[0].project_id, w.mobi, "Mobi costs the most in the last 30 days");
}

#[test]
fn a_removed_project_keeps_its_runs_in_the_total() {
    let w = world();
    // A deleted project still has its line: its runs happened.
    w.db.write(None, |x| { x.conn().execute("UPDATE projects SET deleted_at = 1 WHERE id = ?1", [&w.mobi])?; Ok(()) }).unwrap();
    let u = usage::for_period(&w.db, "30d", NOW).unwrap();
    assert_tabs_agree(&u);
    assert!(u.projects.iter().any(|p| p.project_id == w.mobi));
}

#[test]
fn the_usage_reaches_the_ui_in_camel_case() {
    let w = world();
    let v = serde_json::to_value(usage::for_period(&w.db, "today", NOW).unwrap()).unwrap();
    for key in ["since", "until", "total", "days", "agents", "projects", "chat", "noProject"] {
        assert!(v.get(key).is_some(), "{key}");
    }
    for key in ["runs", "chatTurns", "inputTokens", "outputTokens", "costUsdMicros", "unknownCostRuns"] {
        assert!(v["total"].get(key).is_some(), "total.{key}");
    }
    assert_eq!(v["days"][0]["dayStart"], OCT_9);
    assert!(v["agents"][0].get("agentId").is_some() && v["agents"][0].get("roleKey").is_some());
    assert!(v["projects"][0].get("projectId").is_some() && v["projects"][0].get("color").is_some());
}

#[test]
fn the_projects_list_has_each_projects_api_cost_this_month() {
    let (db, s) = setup();
    let kade = project(&db, &s, "Kade", "KADE");
    let mobi = project(&db, &s, "Mobi", "MOBI");
    let idle = project(&db, &s, "Idle", "IDLE");
    let backend = agent(&db, &s, "Backend Agent", "backend", None);
    let codex = agent(&db, &s, "Codex Agent", "backend", None);
    let lead = agent(&db, &s, "Team Lead", "lead", Some(true));
    let month = runs::month_start_ms(ids::now_ms());
    card_run(&db, &backend, &card(&db, &s, &kade, "K1"), 400_000, 1_000, 100, None);
    card_run(&db, &backend, &card(&db, &s, &kade, "K2"), 20_000, 100, 10, Some(month));
    card_run(&db, &codex, &card(&db, &s, &kade, "K3"), 0, 300, 30, None); // unknown cost
    card_run(&db, &backend, &card(&db, &s, &kade, "K4"), 9_000_000, 1, 1, Some(month - 1)); // last month
    card_run(&db, &backend, &card(&db, &s, &mobi, "M1"), 0, 0, 0, None); // used nothing: $0, not unknown
    card_run(&db, &codex, &card(&db, &s, &mobi, "M2"), 0, 10, 0, Some(month - 1)); // last month's unknown doesn't count
    chat_turn(&db, &s, &lead, "succeeded", 5_000_000, 10, 10, None); // a chat turn is no project's

    let list = projects::list(&db).unwrap();
    let of = |id: &str| list.iter().find(|p| p.id == id).unwrap();
    assert_eq!((of(&kade).ai_cost_usd_micros, of(&kade).ai_unknown_cost_runs), (420_000, 1));
    assert_eq!((of(&mobi).ai_cost_usd_micros, of(&mobi).ai_unknown_cost_runs), (0, 0));
    assert_eq!((of(&idle).ai_cost_usd_micros, of(&idle).ai_unknown_cost_runs), (0, 0));
    // The same numbers on one project, and the same as the Usage page's Projects tab for this month.
    let k = projects::get(&db, &kade).unwrap();
    assert_eq!((k.ai_cost_usd_micros, k.ai_unknown_cost_runs), (420_000, 1));
    let u = usage::for_period(&db, "month", ids::now_ms()).unwrap();
    let tab = u.projects.iter().find(|p| p.project_id == kade).unwrap();
    assert_eq!((tab.totals.cost_usd_micros, tab.totals.unknown_cost_runs), (420_000, 1));
    // An archived card's runs still count (archiving is a soft delete).
    let first = list_tasks_of(&db, &kade).remove(0);
    db.write(None, |w| { w.conn().execute("UPDATE tasks SET deleted_at = 1 WHERE id = ?1", [&first])?; Ok(()) }).unwrap();
    assert_eq!(projects::get(&db, &kade).unwrap().ai_cost_usd_micros, 420_000);
    assert_eq!(usage::for_period(&db, "month", ids::now_ms()).unwrap().projects.iter().find(|p| p.project_id == kade).unwrap().totals.cost_usd_micros,
               420_000);
    // The UI gets the new fields in camelCase.
    let v = serde_json::to_value(&k).unwrap();
    assert_eq!((v["aiCostUsdMicros"].as_i64(), v["aiUnknownCostRuns"].as_i64()), (Some(420_000), Some(1)));
    // Nothing here moved the budget rule: the cost stays the agent's.
    assert_eq!(runs::agent_spend_since(&db, &backend, month).unwrap(), 420_000);
}

fn list_tasks_of(db: &Db, project_id: &str) -> Vec<String> {
    db.read(|c| {
        let mut st = c.prepare("SELECT id FROM tasks WHERE project_id = ?1 ORDER BY created_at")?;
        Ok(st.query_map([project_id], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?)
    }).unwrap()
}
