//! GA-35, the Team Lead's board check in code: answered, held, waiting and stopped cards and what is left out, what a
//! check saw (only new findings start the model), three failed checks pause it, the board check setting, Team Lead
//! chats (one waiting chat per card, answered by your message, dismissed), a person dragging a held card back, and
//! migration 0008 on a database with data.
use gizai_core::board::{self, Context, Finding};
use gizai_core::columns::{self, ColumnInput};
use gizai_core::{chat, comments, db::Db, ids, model::*, projects, runs, seed::ensure_seed, tasks, team, workflow};

struct B { db: Db, you: String, project: String, be: String, qa: String, lead: String, team: String }

/// A Backend Agent and a QA Agent that take cards (one at a time) and a Team Lead with Chat on. add_agent puts them on
/// their role's columns: the Backend Agent on To do and In progress, the QA Agent on Testing, the Team Lead on none.
fn board_() -> B {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let agent = |name: &str, role: &str, wakeup: &str| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), wakeup: wakeup.into(), chat_enabled: Some(role == "lead"), ..Default::default() }).unwrap();
    let be = agent("Backend Agent", "backend", "on_assign");
    let qa = agent("QA Agent", "qa", "on_assign");
    let lead = agent("Team Lead", "lead", "manual");
    B { db, you: s.you_id, project, be, qa, lead, team: s.team_id }
}

fn tick() {
    // comments and holds are compared by the millisecond
    std::thread::sleep(std::time::Duration::from_millis(3));
}

impl B {
    fn state(&self, name: &str) -> String {
        self.db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
    }
    /// A card in `column` (labels don't route: the column's agents pick it up).
    fn card(&self, column: &str) -> String {
        self.card_in(&self.project, column)
    }
    fn card_in(&self, project: &str, column: &str) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: project.to_string(), title: format!("A card in {column}"),
            state_id: Some(self.state(column)), ..Default::default() }).unwrap()
    }
    /// Takes the Backend Agent off `column`: no agent is on it, so nothing picks its cards up.
    fn no_agents_on(&self, column: &str) {
        columns::remove_agent(&self.db, &self.you, &self.state(column), &self.be).unwrap();
    }
    fn run(&self, agent: &str, task: &str) -> String {
        runs::create(&self.db, agent, task, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap()
    }
    /// The Backend Agent's run on `task` ends asking a decision: the card is on hold needs_decision.
    fn ask(&self, task: &str) -> String {
        let r = self.run(&self.be, task);
        let o = Outcome { outcome: "needs_decision".into(), summary: "Which export format, CSV or JSON?".into(), issues: vec![] };
        workflow::apply_outcome(&self.db, &r, Some(&o)).unwrap();
        assert_eq!(tasks::get(&self.db, task).unwrap().hold.as_deref(), Some("needs_decision"));
        r
    }
    fn cx_at(&self, now: i64) -> Context {
        Context { now, max_concurrent: 3, lead_id: Some(self.lead.clone()), ..Default::default() }
    }
    fn check_with(&self, cx: &Context) -> Vec<Finding> {
        board::check(&self.db, cx).unwrap()
    }
    fn check(&self) -> Vec<Finding> {
        self.check_with(&self.cx_at(ids::now_ms()))
    }
    /// The card's findings as (kind, code).
    fn of(&self, all: &[Finding], task: &str) -> Vec<(String, String)> {
        all.iter().filter(|f| f.task_id == task).map(|f| (f.kind.clone(), f.code.clone())).collect()
    }
    fn agent_input(&self, id: &str) -> AgentInput {
        let m = team::agent(&self.db, id).unwrap();
        AgentInput { name: m.name, role_key: m.role_key, wakeup: m.wakeup.unwrap_or_default(), heartbeat_minutes: m.heartbeat_minutes,
                     budget_usd_micros: m.budget_usd_micros, ..Default::default() }
    }
    fn member(&self, id: &str) -> team::Member { team::agent(&self.db, id).unwrap() }
    fn failures(&self) -> i64 {
        self.db.read(|c| Ok(c.query_row("SELECT board_check_failures FROM agent_configs WHERE actor_id=?1", [&self.lead], |r| r.get(0))?)).unwrap()
    }
    /// One finished check of the Team Lead that saw `all`.
    fn checked(&self, all: &[Finding], status: &str) -> Option<String> {
        let r = board::create_run(&self.db, &self.lead, "S", "/tmp/lead", "/tmp/c.jsonl", &board::seen_of(all)).unwrap();
        board::finish_run(&self.db, &r, status, 5_000, 10, 2, (status != "succeeded").then_some("Claude Code exited"), Some("Checked the board.")).unwrap()
    }
}

// ---- Part 1: the findings ----

#[test]
fn a_persons_comment_after_a_needs_decision_hold_is_an_answer_but_an_agents_comment_or_an_older_one_is_not() {
    let b = board_();
    // a person's comment from before the hold doesn't answer it
    let old = b.card("In progress");
    comments::add(&b.db, &b.you, &old, "Use CSV please", None).unwrap();
    tick();
    b.ask(&old);
    // the agent's own comments (its summary, and one more) don't answer it either
    let own = b.card("In progress");
    b.ask(&own);
    tick();
    comments::add(&b.db, &b.be, &own, "Still waiting for an answer", None).unwrap();
    // a person's comment after the hold does
    let answered = b.card("In progress");
    b.ask(&answered);
    tick();
    comments::add(&b.db, &b.you, &answered, "JSON, with a CSV export later", None).unwrap();

    let all = b.check();
    assert_eq!(b.of(&all, &old), [("held".to_string(), "needs_decision".to_string())]);
    assert_eq!(b.of(&all, &own), [("held".to_string(), "needs_decision".to_string())]);
    assert_eq!(b.of(&all, &answered), [("answered".to_string(), "answered".to_string())]);
    let f = all.iter().find(|f| f.task_id == answered).unwrap();
    assert_eq!(f.answer.as_deref(), Some("JSON, with a CSV export later"));
    assert_eq!(f.agent_id.as_deref(), Some(b.be.as_str()), "the agent that asked");
    assert_eq!(f.hold.as_deref(), Some("needs_decision"));
    assert_eq!(f.last_run.as_ref().unwrap().outcome.as_deref(), Some("needs_decision"));
    // answered cards come first
    assert_eq!(all[0].kind, "answered", "{all:?}");
}

#[test]
fn a_held_card_names_its_hold_its_reason_and_its_latest_run() {
    let b = board_();
    let t = b.card("To do");
    let r = b.run(&b.be, &t);
    runs::finish(&b.db, &r, "failed", None, 0, 0, 0, Some("exit 1: rate limited")).unwrap();
    tasks::update(&b.db, &b.you, &t, TaskPatch { hold: Some("blocked".into()), hold_reason: Some("The API key is missing".into()), ..Default::default() }).unwrap();
    let all = b.check();
    assert_eq!(b.of(&all, &t), [("held".to_string(), "blocked".to_string())]);
    let f = &all[0];
    assert_eq!(f.hold_reason.as_deref(), Some("The API key is missing"));
    assert!(f.reason.contains("blocked") && f.reason.contains("The API key is missing"), "{}", f.reason);
    let last = f.last_run.as_ref().unwrap();
    assert_eq!((last.status.as_str(), last.error.as_deref()), ("failed", Some("exit 1: rate limited")));
    assert!(f.since > 0 && f.since <= ids::now_ms());
}

#[test]
fn a_card_in_progress_whose_run_stopped_part_way_is_a_stopped_finding() {
    let b = board_();
    let cases = [("timed_out", Some("it stopped at the 45 min limit"), "limit"), ("failed", Some("exit 1"), "failed"), ("cancelled", Some("Stopped by you"), "stopped")];
    let mut cards = vec![];
    for (status, error, _) in cases {
        let t = b.card("In progress");
        let r = b.run(&b.be, &t);
        runs::finish(&b.db, &r, status, None, 0, 0, 0, error).unwrap();
        cards.push(t);
    }
    // a run that ended without a result
    let nores = b.card("In progress");
    let r = b.run(&b.be, &nores);
    runs::finish(&b.db, &r, "succeeded", None, 0, 0, 0, None).unwrap();
    workflow::apply_outcome(&b.db, &r, None).unwrap();
    let all = b.check();
    for ((_, _, code), t) in cases.iter().zip(&cards) {
        assert_eq!(b.of(&all, t), [("stopped".to_string(), code.to_string())], "{code}");
    }
    let f = all.iter().find(|f| f.task_id == cards[0]).unwrap();
    assert_eq!(f.agent_id.as_deref(), Some(b.be.as_str()));
    assert!(f.reason.contains("limit"), "{}", f.reason);
    // without a result: a hold or a stopped finding, never nothing
    let n = b.of(&all, &nores);
    assert_eq!(n.len(), 1, "{all:?}");
    // a card with a run at work now is no finding
    let busy = b.card("In progress");
    b.run(&b.be, &busy);
    assert!(b.of(&b.check(), &busy).is_empty());
}

#[test]
fn a_card_in_a_column_without_agents_or_assigned_to_a_person_there_is_waiting_with_why() {
    let b = board_();
    b.no_agents_on("To do");
    let loose = b.card("To do");
    let person = b.card("To do");
    tasks::update(&b.db, &b.you, &person, TaskPatch { assignee_id: Some(b.you.clone()), ..Default::default() }).unwrap();
    let all = b.check();
    assert_eq!(b.of(&all, &loose), [("waiting".to_string(), "no_agents".to_string())]);
    assert_eq!(b.of(&all, &person), [("waiting".to_string(), "no_agents".to_string())]);
    let f = all.iter().find(|f| f.task_id == person).unwrap();
    assert!(f.reason.contains("Jeffrey"), "{}", f.reason);
}

#[test]
fn a_card_whose_agent_is_paused_over_budget_or_has_its_pull_paused_is_waiting_with_why_and_a_manual_column_is_no_finding() {
    let b = board_();
    let t = b.card("To do");
    let now = ids::now_ms();
    let code = |cx: &Context| b.of(&b.check_with(cx), &t);
    let w = |c: &str| vec![("waiting".to_string(), c.to_string())];

    // its pull is paused (GA-32), with why
    let mut cx = b.cx_at(now);
    cx.pull_paused.insert(b.be.clone(), "its last start failed: not logged in".into());
    assert_eq!(code(&cx), w("pull_paused"));
    assert!(b.check_with(&cx)[0].reason.contains("not logged in"));
    // agents paused in Settings
    let mut cx = b.cx_at(now);
    cx.agents_paused = true;
    assert_eq!(code(&cx), w("agents_paused"));
    // wake-up manual no longer matters (GA-49): its free slot left unused is the finding
    let mut i = b.agent_input(&b.be);
    i.wakeup = "manual".into();
    team::update_agent(&b.db, &b.you, &b.be, i).unwrap();
    assert_eq!(code(&b.cx_at(now + 3 * 60_000)), w("free_slot"));
    // a Manual column: only a person's Run starts its cards, so they wait by design
    columns::set_column(&b.db, &b.you, &b.state("To do"), ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    assert!(code(&b.cx_at(now + 3 * 60_000)).is_empty());
    columns::set_column(&b.db, &b.you, &b.state("To do"), ColumnInput { auto: Some(true), ..Default::default() }).unwrap();
    // over its budget: it spent $0.05 of $0.01 this month
    let mut i = b.agent_input(&b.be);
    i.wakeup = "on_assign".into();
    i.budget_usd_micros = Some(10_000);
    team::update_agent(&b.db, &b.you, &b.be, i).unwrap();
    let other = b.card("Backlog");
    let r = b.run(&b.be, &other);
    runs::finish(&b.db, &r, "succeeded", None, 50_000, 0, 0, None).unwrap();
    assert_eq!(code(&b.cx_at(ids::now_ms())), w("budget"));
    // paused
    team::set_agent_status(&b.db, &b.you, &b.be, "paused").unwrap();
    let all = b.check();
    assert_eq!(b.of(&all, &t), w("paused"));
    assert_eq!(all.iter().find(|f| f.task_id == t).unwrap().agent.as_deref(), Some("Backend Agent"));
}

#[test]
fn a_card_waiting_only_for_its_busy_agent_is_no_finding_but_a_free_slot_left_unused_is() {
    let b = board_();
    let waiting = b.card("To do");
    // just dragged in, the agent has a free slot: the queue takes it soon, no finding yet
    assert!(b.of(&b.check(), &waiting).is_empty(), "a card the queue is about to take is no finding");
    // three minutes on, the free slot is still unused
    let later = ids::now_ms() + 3 * 60_000;
    let all = b.check_with(&b.cx_at(later));
    assert_eq!(b.of(&all, &waiting), [("waiting".to_string(), "free_slot".to_string())]);
    assert_eq!(all.iter().find(|f| f.task_id == waiting).unwrap().agent_id.as_deref(), Some(b.be.as_str()));
    // the agent works on another card (its one slot): the card only waits for it
    let other = b.card("In progress");
    b.run(&b.be, &other);
    let all = b.check_with(&b.cx_at(later));
    assert!(b.of(&all, &waiting).is_empty(), "{all:?}");
    assert!(b.of(&all, &other).is_empty(), "a card with a run at work");
}

#[test]
fn runs_at_once_full_for_over_an_hour_is_a_finding_and_a_testing_card_routes_to_qa() {
    let b = board_();
    let testing = b.card("Testing");
    // QA is free for three minutes: a free slot left unused
    let later = ids::now_ms() + 3 * 60_000;
    let all = b.check_with(&b.cx_at(later));
    assert_eq!(b.of(&all, &testing), [("waiting".to_string(), "free_slot".to_string())]);
    assert_eq!(all.iter().find(|f| f.task_id == testing).unwrap().agent_id.as_deref(), Some(b.qa.as_str()));
    // "Runs at once" is 1 and the Backend Agent's run takes it: within the hour nothing, after it a finding
    let other = b.card("In progress");
    b.run(&b.be, &other);
    let mut cx = b.cx_at(later);
    cx.max_concurrent = 1;
    assert!(b.of(&b.check_with(&cx), &testing).is_empty());
    cx.now = ids::now_ms() + 61 * 60_000;
    assert_eq!(b.of(&b.check_with(&cx), &testing), [("waiting".to_string(), "runs_full".to_string())]);
}

#[test]
fn backlog_review_done_cancelled_cards_and_paused_or_done_projects_are_left_out() {
    let b = board_();
    let mut quiet = vec![];
    for column in ["Backlog", "Review", "Done"] {
        let t = b.card(column);
        tasks::update(&b.db, &b.you, &t, TaskPatch { hold: Some("blocked".into()), hold_reason: Some("x".into()), ..Default::default() }).unwrap();
        quiet.push(t);
    }
    let cancelled = b.db.read(|c| Ok(c.query_row("SELECT name FROM workflow_states WHERE category='cancelled'", [], |r| r.get::<_, String>(0)).ok())).unwrap();
    if let Some(name) = cancelled {
        quiet.push(b.card(&name));
    }
    for status in ["paused", "done", "archived"] {
        let p = projects::create(&b.db, &b.you, ProjectInput { name: format!("P {status}"), key: format!("P{}", &status[..2].to_uppercase()), ..Default::default() }).unwrap();
        let t = b.card_in(&p, "To do");
        let h = b.card_in(&p, "In progress");
        b.ask(&h);
        tick();
        comments::add(&b.db, &b.you, &h, "Go with JSON", None).unwrap();
        let mut input = ProjectInput { name: format!("P {status}"), key: format!("P{}", &status[..2].to_uppercase()), ..Default::default() };
        input.status = Some(status.into());
        projects::update(&b.db, &b.you, &p, input).unwrap();
        quiet.extend([t, h]);
    }
    let all = b.check();
    for t in &quiet {
        assert!(b.of(&all, t).is_empty(), "{t}: {all:?}");
    }
    // the same card in To do of an active project is a finding (with no agent on To do)
    b.no_agents_on("To do");
    let loose = b.card("To do");
    assert_eq!(b.of(&b.check(), &loose).len(), 1);
}

// ---- Part 2: only new findings start the model; three failures pause the check ----

#[test]
fn a_finding_already_seen_comes_back_only_after_its_card_changes() {
    let b = board_();
    b.no_agents_on("To do");
    let loose = b.card("To do");
    let asked = b.card("In progress");
    b.ask(&asked);
    let all = b.check();
    assert_eq!(all.len(), 2, "{all:?}");
    assert_eq!(board::new_findings(&b.db, &b.lead, &all).unwrap().len(), 2, "never seen");
    b.checked(&all, "succeeded");
    // nothing changed: nothing new
    assert!(board::new_findings(&b.db, &b.lead, &b.check()).unwrap().is_empty());
    // the Team Lead's own comment doesn't make it new
    comments::add(&b.db, &b.lead, &loose, "Looking into this", None).unwrap();
    assert!(board::new_findings(&b.db, &b.lead, &b.check()).unwrap().is_empty());
    // a person's comment on a held card does (and makes it answered)
    tick();
    comments::add(&b.db, &b.you, &asked, "CSV", None).unwrap();
    let new = board::new_findings(&b.db, &b.lead, &b.check()).unwrap();
    assert_eq!(new.iter().map(|f| (f.task_id.as_str(), f.kind.as_str())).collect::<Vec<_>>(), [(asked.as_str(), "answered")]);
    // a change to the other card (a person edits it) makes that one new too
    tasks::update(&b.db, &b.you, &loose, TaskPatch { description_md: Some("More detail".into()), ..Default::default() }).unwrap();
    let new = board::new_findings(&b.db, &b.lead, &b.check()).unwrap();
    assert_eq!(new.len(), 2, "{new:?}");
}

#[test]
fn a_failed_check_leaves_its_findings_new_and_three_in_a_row_pause_the_check_until_the_agent_changes_or_resumes() {
    let b = board_();
    b.no_agents_on("To do");
    b.card("To do");
    let all = b.check();
    assert_eq!(b.checked(&all, "failed"), None);
    assert_eq!(board::new_findings(&b.db, &b.lead, &all).unwrap().len(), 1, "a failed check saw nothing");
    assert_eq!(b.checked(&all, "timed_out"), None);
    assert_eq!(b.failures(), 2);
    // a check stopped because Gizai quit is no failure
    assert_eq!(b.checked(&all, "cancelled"), None);
    assert_eq!(b.failures(), 2);
    let why = b.checked(&all, "failed").expect("the third failure in a row pauses the check");
    assert!(why.contains("3 board checks in a row failed") && why.contains("Claude Code exited"), "{why}");
    assert_eq!(b.member(&b.lead).board_check_paused.as_deref(), Some(why.as_str()));
    // resumed: the check runs again
    team::set_agent_status(&b.db, &b.you, &b.lead, "paused").unwrap();
    assert!(b.member(&b.lead).board_check_paused.is_some(), "pausing the agent doesn't clear it");
    team::set_agent_status(&b.db, &b.you, &b.lead, "active").unwrap();
    assert_eq!((b.member(&b.lead).board_check_paused, b.failures()), (None, 0));
    // changed: the same
    for _ in 0..3 { b.checked(&all, "failed"); }
    assert!(b.member(&b.lead).board_check_paused.is_some());
    team::update_agent(&b.db, &b.you, &b.lead, b.agent_input(&b.lead)).unwrap();
    assert_eq!((b.member(&b.lead).board_check_paused, b.failures()), (None, 0));
    // a success resets the count
    b.checked(&all, "failed");
    b.checked(&all, "failed");
    b.checked(&all, "succeeded");
    assert_eq!(b.failures(), 0);
}

#[test]
fn a_check_is_a_run_of_the_team_lead_with_no_card_and_no_chat_and_its_cost_counts_toward_its_budget() {
    let b = board_();
    let r = board::create_run(&b.db, &b.lead, "S", "/tmp/lead", "/tmp/c.jsonl", &[]).unwrap();
    board::finish_run(&b.db, &r, "succeeded", 123_000, 10, 2, None, Some("Released KADE-1.")).unwrap();
    let run = runs::get(&b.db, &r).unwrap();
    assert_eq!((run.trigger.as_str(), run.task_id, run.status.as_str(), run.cost_usd_micros), ("board_check", None, "succeeded", 123_000));
    assert_eq!(run.summary_md.as_deref(), Some("Released KADE-1."));
    let chat: Option<String> = b.db.read(|c| Ok(c.query_row("SELECT chat_thread_id FROM runs WHERE id=?1", [&r], |r| r.get(0))?)).unwrap();
    assert_eq!(chat, None);
    assert_eq!(runs::agent_spend_since(&b.db, &b.lead, runs::month_start_ms(ids::now_ms())).unwrap(), 123_000);
    assert!(runs::list_for_agent(&b.db, &b.lead, 10).unwrap().iter().any(|x| x.id == r));
    // finish_run only finishes checks
    let t = b.card("To do");
    let card_run = b.run(&b.be, &t);
    assert!(board::finish_run(&b.db, &card_run, "succeeded", 0, 0, 0, None, None).is_err());
}

// ---- The setting ----

#[test]
fn the_board_check_setting_is_off_for_a_new_agent_set_by_create_and_update_and_unchanged_when_left_out() {
    let b = board_();
    assert_eq!(b.member(&b.lead).board_check_minutes, None, "off for an existing agent");
    let mut i = b.agent_input(&b.lead);
    i.board_check_minutes = Some(15);
    team::update_agent(&b.db, &b.you, &b.lead, i).unwrap();
    assert_eq!(b.member(&b.lead).board_check_minutes, Some(15));
    // left out on update: unchanged
    team::update_agent(&b.db, &b.you, &b.lead, b.agent_input(&b.lead)).unwrap();
    assert_eq!(b.member(&b.lead).board_check_minutes, Some(15));
    // 0 turns it off
    let mut i = b.agent_input(&b.lead);
    i.board_check_minutes = Some(0);
    team::update_agent(&b.db, &b.you, &b.lead, i).unwrap();
    assert_eq!(b.member(&b.lead).board_check_minutes, None);
    // out of range
    for bad in [3, 2000] {
        let mut i = b.agent_input(&b.lead);
        i.board_check_minutes = Some(bad);
        assert!(team::update_agent(&b.db, &b.you, &b.lead, i).is_err(), "{bad}");
    }
    // create sets it, and the team lists it
    let id = team::add_agent(&b.db, &b.you, &b.team, AgentInput { name: "Lead 2".into(), role_key: "lead".into(), board_check_minutes: Some(30),
        ..Default::default() }).unwrap();
    assert_eq!(b.member(&id).board_check_minutes, Some(30));
    let listed = team::all_agents(&b.db).unwrap().into_iter().find(|(_, m)| m.actor_id == id).unwrap().1;
    assert_eq!(listed.board_check_minutes, Some(30));
    board::touch(&b.db, &id, 42).unwrap();
    assert_eq!(b.member(&id).board_checked_at, Some(42));
}

// ---- Part 4: chats the Team Lead starts ----

#[test]
fn start_lead_chat_keeps_one_waiting_chat_per_card_and_your_message_answers_it() {
    let b = board_();
    let (t1, t2, t3) = (b.card("To do"), b.card("To do"), b.card("To do"));
    let (a, new) = chat::start_lead_chat(&b.db, &b.lead, "KADE-1: which agent?", "question", &[t1.clone()], "Nothing routes KADE-1.", None).unwrap();
    assert!(new);
    let th = chat::get_thread(&b.db, &a).unwrap();
    assert_eq!((th.kind.as_deref(), th.created_by.as_deref(), th.waiting), (Some("question"), Some(b.lead.as_str()), true));
    assert_eq!(th.tasks, ["KADE-1"]);
    let msgs = chat::messages(&b.db, &a).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!((msgs[0].role.as_str(), msgs[0].author_id.as_deref(), msgs[0].body_md.as_deref()), ("agent", Some(b.lead.as_str()), Some("Nothing routes KADE-1.")));
    // another message about KADE-1 (and KADE-2) goes into the same chat, which moves to the top again
    let (other, _) = chat::start_lead_chat(&b.db, &b.lead, "KADE-3: approve?", "approval", &[t3.clone()], "May I?", None).unwrap();
    tick();
    let (same, new) = chat::start_lead_chat(&b.db, &b.lead, "KADE-1 and KADE-2", "question", &[t2.clone(), t1.clone()], "KADE-2 too.", None).unwrap();
    assert_eq!((same.as_str(), new), (a.as_str(), false));
    assert_eq!(chat::messages(&b.db, &a).unwrap().len(), 2);
    assert_eq!(chat::get_thread(&b.db, &a).unwrap().tasks, ["KADE-1", "KADE-2"]);
    let waiting: Vec<String> = chat::waiting_lead_chats(&b.db).unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(waiting, [a.clone(), other.clone()], "newest activity first");
    // your own chats are never waiting Team Lead chats
    let mine = chat::create_thread(&b.db, &b.you, &b.lead, "Hello").unwrap();
    let mt = chat::get_thread(&b.db, &mine).unwrap();
    assert_eq!((mt.kind, mt.waiting), (None, false));
    // your message answers it: it leaves the Inbox, and the next question about KADE-1 starts a new chat
    chat::add_message(&b.db, chat::NewMessage { thread_id: a.clone(), role: "user".into(), author_id: Some(b.you.clone()),
        body_md: Some("Give it to the Backend Agent".into()), ..Default::default() }).unwrap();
    let th = chat::get_thread(&b.db, &a).unwrap();
    assert!(!th.waiting && th.answered_at.is_some());
    assert_eq!(chat::waiting_lead_chats(&b.db).unwrap().len(), 1);
    let (fresh, new) = chat::start_lead_chat(&b.db, &b.lead, "KADE-1 again", "question", &[t1.clone()], "One more thing.", None).unwrap();
    assert!(new && fresh != a);
    // × dismisses it
    chat::dismiss(&b.db, &b.you, &other).unwrap();
    let ot = chat::get_thread(&b.db, &other).unwrap();
    assert!(!ot.waiting && ot.dismissed_at.is_some());
    assert_eq!(chat::waiting_lead_chats(&b.db).unwrap().into_iter().map(|t| t.id).collect::<Vec<_>>(), [fresh]);
    assert!(chat::dismiss(&b.db, &b.you, "nope").is_err());
    // a kind other than question or approval, or no message, is refused
    assert!(chat::start_lead_chat(&b.db, &b.lead, "x", "fyi", &[t1.clone()], "x", None).is_err());
    assert!(chat::start_lead_chat(&b.db, &b.lead, "x", "question", &[t1], "  ", None).is_err());
}

// ---- Part 6: a person dragging a held card back releases it ----

#[test]
fn a_person_dragging_a_held_card_into_to_do_or_in_progress_takes_the_hold_off_but_a_gate_move_keeps_it() {
    let b = board_();
    for column in ["To do", "In progress"] {
        let t = b.card("Testing");
        tasks::update(&b.db, &b.you, &t, TaskPatch { hold: Some("blocked".into()), hold_reason: Some("x".into()), ..Default::default() }).unwrap();
        tasks::move_to(&b.db, &b.you, &t, &b.state(column), "").unwrap();
        let task = tasks::get(&b.db, &t).unwrap();
        assert_eq!((task.state_name.as_str(), task.hold, task.hold_reason), (column, None, None), "{column}");
    }
    // into Backlog or Review: the hold stays
    for column in ["Backlog", "Review"] {
        let t = b.card("To do");
        tasks::update(&b.db, &b.you, &t, TaskPatch { hold: Some("blocked".into()), ..Default::default() }).unwrap();
        tasks::move_to(&b.db, &b.you, &t, &b.state(column), "").unwrap();
        assert_eq!(tasks::get(&b.db, &t).unwrap().hold.as_deref(), Some("blocked"), "{column}");
    }
    // an agent's move (the Team Lead's move_task) keeps it too
    let t = b.card("Testing");
    tasks::update(&b.db, &b.you, &t, TaskPatch { hold: Some("blocked".into()), ..Default::default() }).unwrap();
    tasks::move_to(&b.db, &b.lead, &t, &b.state("To do"), "").unwrap();
    assert_eq!(tasks::get(&b.db, &t).unwrap().hold.as_deref(), Some("blocked"));
    // a gate's move when a run ends keeps its hold: QA fails a card back to To do and asks a decision
    let t = b.card("In progress");
    let r = b.run(&b.be, &t);
    workflow::apply_outcome(&b.db, &r, Some(&Outcome { outcome: "needs_decision".into(), summary: "Which format?".into(), issues: vec![] })).unwrap();
    let task = tasks::get(&b.db, &t).unwrap();
    assert_eq!(task.hold.as_deref(), Some("needs_decision"), "the gate's hold stays where the gate put the card ({})", task.state_name);
    // the person then drags it into To do: the hold comes off, and a check no longer sees it held
    tasks::move_to(&b.db, &b.you, &t, &b.state("To do"), "").unwrap();
    assert_eq!(tasks::get(&b.db, &t).unwrap().hold, None);
    assert!(b.check().iter().all(|f| f.task_id != t || f.kind == "waiting"));
}

// ---- Migration 0008 ----

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

/// Steps a current database back to schema 7: 0007's runs table, none of 0008's columns, no agent folders (0009),
/// none of 0011's column setup (no column agents, Auto or next columns, branches; an empty routing_rules table back)
/// and no chat Runs on or queue (0012).
pub fn back_to_7(c: &rusqlite::Connection) {
    let m7 = include_str!("../migrations/0007_card_flow.sql");
    let start = m7.find("CREATE TABLE runs_new (").unwrap();
    let end = start + m7[start..].find(") STRICT;").unwrap() + ") STRICT;".len();
    let runs7 = m7[start..end].replacen("CREATE TABLE runs_new (", "CREATE TABLE runs_v7 (", 1);
    assert!(!runs7.contains("board_check") && !runs7.contains("findings_json"));
    let m1 = include_str!("../migrations/0001_init.sql");
    let start = m1.find("CREATE TABLE routing_rules (").unwrap();
    let rules = &m1[start..start + m1[start..].find(") STRICT;").unwrap() + ") STRICT;".len()];
    c.execute_batch(&format!("PRAGMA foreign_keys=OFF; BEGIN;
        ALTER TABLE runs DROP COLUMN findings_json;
        {runs7}; INSERT INTO runs_v7 SELECT * FROM runs; DROP TABLE runs; ALTER TABLE runs_v7 RENAME TO runs;
        CREATE INDEX runs_task ON runs(task_id, created_at); CREATE INDEX runs_agent_period ON runs(agent_actor_id, started_at);
        ALTER TABLE tasks DROP COLUMN hold_at;
        ALTER TABLE agent_configs DROP COLUMN board_check_minutes; ALTER TABLE agent_configs DROP COLUMN board_checked_at;
        ALTER TABLE agent_configs DROP COLUMN board_check_failures; ALTER TABLE agent_configs DROP COLUMN board_check_paused;
        ALTER TABLE chat_threads DROP COLUMN kind; ALTER TABLE chat_threads DROP COLUMN task_ids_json;
        ALTER TABLE chat_threads DROP COLUMN answered_at; ALTER TABLE chat_threads DROP COLUMN dismissed_at;
        ALTER TABLE agent_configs DROP COLUMN folders_json;
        DROP TABLE column_agents; ALTER TABLE workflow_states DROP COLUMN next_state_id; ALTER TABLE workflow_states DROP COLUMN auto;
        ALTER TABLE teams DROP COLUMN branches_json; {rules};
        DROP TABLE chat_queue; ALTER TABLE chat_messages DROP COLUMN meta_json; ALTER TABLE chat_threads DROP COLUMN session_cli; ALTER TABLE chat_threads DROP COLUMN cli;
        COMMIT; PRAGMA user_version = 7;")).unwrap();
}

const SNAPSHOTS: [&str; 6] = [
    "SELECT id, identifier, state_id, hold, hold_reason, fail_count FROM tasks ORDER BY id",
    "SELECT id, task_id, chat_thread_id, agent_actor_id, trigger, status, outcome, cost_usd_micros, summary_md, error FROM runs ORDER BY id",
    "SELECT id, agent_actor_id, title, session_id, created_by, cost_usd_micros FROM chat_threads ORDER BY id",
    "SELECT id, thread_id, role, author_actor_id, body_md, run_id FROM chat_messages ORDER BY id",
    "SELECT actor_id, adapter, wakeup, heartbeat_minutes, budget_usd_micros, chat_enabled, max_concurrent_runs FROM agent_configs ORDER BY actor_id",
    "SELECT id, name, kind, status FROM actors ORDER BY id",
];

#[test]
fn migration_0008_keeps_every_chat_message_run_and_agent_and_existing_chats_stay_your_own() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let (asked, hold_time) = {
        let db = Db::open(&path).unwrap();
        let s = ensure_seed(&db, "Jeffrey").unwrap();
        let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
        let be = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
        let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
            budget_usd_micros: Some(5_000_000), ..Default::default() }).unwrap();
        let state: String = db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name='In progress'", [], |r| r.get(0))?)).unwrap();
        let mut ids = vec![];
        for i in 0..3 {
            ids.push(tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: format!("Card {i}"), state_id: Some(state.clone()), ..Default::default() }).unwrap());
        }
        let r = runs::create(&db, &be, &ids[0], "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
        workflow::apply_outcome(&db, &r, Some(&Outcome { outcome: "needs_decision".into(), summary: "Which format?".into(), issues: vec![] })).unwrap();
        let hold_time = ids::now_ms();
        let r = runs::create(&db, &be, &ids[1], "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
        runs::finish(&db, &r, "failed", None, 7, 0, 0, Some("exit 1")).unwrap();
        // two chats with a turn each
        for text in ["Plan the Kade portal", "What is on the board?"] {
            let th = chat::create_thread(&db, &s.you_id, &lead, text).unwrap();
            let run = runs::create_chat(&db, &lead, &th, "S", "/tmp", "/tmp/c.jsonl").unwrap();
            chat::add_message(&db, chat::NewMessage { thread_id: th.clone(), role: "agent".into(), author_id: Some(lead.clone()), body_md: Some("Sure.".into()),
                run_id: Some(run.clone()), ..Default::default() }).unwrap();
            runs::finish_chat(&db, &run, "succeeded", 10_000, 100, 10, None).unwrap();
        }
        (ids[0].clone(), hold_time)
    };
    let c = rusqlite::Connection::open(&path).unwrap();
    back_to_7(&c);
    assert!(c.execute("UPDATE runs SET trigger='board_check'", []).is_err(), "schema 7 has no board_check trigger");
    let before: Vec<Rows> = SNAPSHOTS.iter().map(|q| rows(&c, q)).collect();
    assert_eq!((before[1].len(), before[2].len(), before[3].len()), (4, 2, 2), "2 card runs, 2 chats with an answer each");
    drop(c);

    // Gizai opens it: 0008 to 0012 run.
    let db = Db::open(&path).unwrap();
    let after: Vec<Rows> = db.read(|c| Ok(SNAPSHOTS.iter().map(|q| rows(c, q)).collect())).unwrap();
    for (i, q) in SNAPSHOTS.iter().enumerate() {
        assert_eq!(after[i], before[i], "{q}");
    }
    let (version, broken): (i64, usize) = db.read(|c| Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?, rows(c, "PRAGMA foreign_key_check").len()))).unwrap();
    assert_eq!((version, broken), (gizai_core::db::SCHEMA_VERSION, 0));
    // existing chats stay your own, nothing waits in the Inbox
    let threads = chat::list_threads(&db).unwrap();
    assert_eq!(threads.len(), 2);
    assert!(threads.iter().all(|t| t.kind.is_none() && !t.waiting && t.tasks.is_empty()), "{threads:?}");
    assert!(chat::waiting_lead_chats(&db).unwrap().is_empty());
    // every agent's check is off
    assert!(team::all_agents(&db).unwrap().iter().all(|(_, m)| m.board_check_minutes.is_none() && m.board_check_paused.is_none()));
    // an existing hold got its time from the changes log, so an older comment doesn't answer it
    let hold_at: Option<i64> = db.read(|c| Ok(c.query_row("SELECT hold_at FROM tasks WHERE id=?1", [&asked], |r| r.get(0))?)).unwrap();
    let hold_at = hold_at.expect("a held card has hold_at");
    assert!((hold_at - hold_time).abs() < 5_000, "{hold_at} vs {hold_time}");
    let unheld: i64 = db.read(|c| Ok(c.query_row("SELECT count(*) FROM tasks WHERE hold IS NULL AND hold_at IS NOT NULL", [], |r| r.get(0))?)).unwrap();
    assert_eq!(unheld, 0);
    // and the new things fit: a board check run, a Team Lead chat
    let lead: String = db.read(|c| Ok(c.query_row("SELECT id FROM actors WHERE name='Team Lead'", [], |r| r.get(0))?)).unwrap();
    let r = board::create_run(&db, &lead, "S", "/tmp/lead", "/tmp/c.jsonl", &[]).unwrap();
    board::finish_run(&db, &r, "succeeded", 1, 0, 0, None, Some("ok")).unwrap();
    chat::start_lead_chat(&db, &lead, "KADE-1: format?", "question", &[asked], "CSV or JSON?", Some(&r)).unwrap();
    assert_eq!(chat::waiting_lead_chats(&db).unwrap().len(), 1);
}
