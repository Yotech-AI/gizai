//! GA-70, agents ask the Team Lead before a person, in core: who takes a task agent's `needs_decision` (the Team Lead
//! when the step is on, agents aren't paused, it is active, on Claude Code and under budget; never for a `run_for_me`
//! request, a failed push, the Team Lead's own question or an Other CLI), the Inbox rule (`tasks::needs_you` leaves a card
//! with the Team Lead out), the board check (leaves it out, and an escalated or limited question too until a person
//! answers), the limits (one try per question, two per card, never right after an answer), escalating (the Team Lead's
//! comment, the reason on the hold, the Inbox), a card that moved on first, the Team Lead's run on a question (trigger
//! question, no card, its cost on the asking run and on the Team Lead's budget), keeping answers in memory linked to the
//! card (the Team Lead's and yours), and questions left waiting when Gizai stopped.
use gizai_core::board::{self, Context};
use gizai_core::clis::Cli;
use gizai_core::memory::{self, Who};
use gizai_core::workflow::GateResult;
use gizai_core::{clis, comments, db::Db, ids, model::*, projects, questions, runs, seed::ensure_seed, settings, tasks, team, workflow};

const QUESTION: &str = "CSV or JSON for the export?";

struct Q {
    db: Db,
    you: String,
    task: String,
    be: String,
    lead: String,
}

/// Card KADE-1 "Export invoices" in To do, a Backend Agent on it and a Team Lead with Chat on (on the built-in Claude
/// Code), as a new install has them.
fn setup() -> Q {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let task = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "Export invoices".into(), ..Default::default() }).unwrap();
    let todo: String = db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name='To do'", [], |r| r.get(0))?)).unwrap();
    tasks::move_to(&db, &s.you_id, &task, &todo, "a1").unwrap();
    let be = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true),
        ..Default::default() }).unwrap();
    Q { db, you: s.you_id, task, be, lead }
}

fn tick() {
    // holds, comments and runs are ordered by the millisecond
    std::thread::sleep(std::time::Duration::from_millis(3));
}

fn asks(summary: &str) -> Outcome {
    Outcome { outcome: "needs_decision".into(), summary: summary.into(), issues: vec!["Which format should the export use?".into()] }
}

impl Q {
    /// A run of `agent` (role `role`) on the card ends with `outcome`, as the app does it: finished, then the gates.
    fn ends_with(&self, agent: &str, role: &str, outcome: &Outcome, run_for_me: &[String]) -> (String, GateResult) {
        tick();
        let r = runs::create(&self.db, agent, &self.task, role, "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/r.jsonl").unwrap();
        workflow::move_on_start(&self.db, agent, &self.task).unwrap();
        runs::finish(&self.db, &r, "succeeded", Some(outcome), 80_000, 6000, 400, None).unwrap();
        let g = workflow::apply_outcome_with(&self.db, &r, Some(outcome), run_for_me).unwrap();
        (r, g)
    }
    /// The Backend Agent asks a question.
    fn ask(&self) -> (String, GateResult) {
        self.ends_with(&self.be, "backend", &asks(QUESTION), &[])
    }
    fn card(&self) -> Task {
        tasks::get(&self.db, &self.task).unwrap()
    }
    fn in_inbox(&self) -> bool {
        tasks::needs_you(&self.db, &self.you).unwrap().iter().any(|t| t.id == self.task)
    }
    fn lead_of(&self, run: &str) -> Option<questions::LeadAnswer> {
        runs::get(&self.db, run).unwrap().lead
    }
    fn state_of(&self, run: &str) -> Option<String> {
        self.lead_of(run).map(|l| l.state)
    }
    /// The board check's findings on the card, as (kind, code).
    fn findings(&self) -> Vec<(String, String)> {
        let cx = Context { now: ids::now_ms(), max_concurrent: 3, lead_id: Some(self.lead.clone()), ..Default::default() };
        board::check(&self.db, &cx).unwrap().into_iter().filter(|f| f.task_id == self.task).map(|f| (f.kind, f.code)).collect()
    }
    /// Adds a CLI in Settings → Coding CLIs and returns its id.
    fn cli(&self, name: &str, kind: &str) -> String {
        let mut list = clis::list(&self.db).unwrap();
        list.push(Cli { name: name.into(), kind: kind.into(), command: "/usr/bin/true".into(), args: if kind == "other" { "{prompt}".into() } else { String::new() },
            ..Default::default() });
        clis::save(&self.db, list).unwrap().into_iter().find(|c| c.name == name).unwrap().id
    }
    fn put_on(&self, agent: &str, cli: &str) {
        let m = team::agent(&self.db, agent).unwrap();
        team::update_agent(&self.db, &self.you, agent, AgentInput { name: m.name, role_key: m.role_key, adapter: cli.into(), ..Default::default() }).unwrap();
    }
    /// The Team Lead's run on the question `asked` (as `ask_lead` records it), finished with `cost`.
    fn lead_run(&self, asked: &str, cost: i64) -> String {
        let id = questions::create_run(&self.db, &self.lead, asked, "LS", "/tmp/lead", "/tmp/lead.jsonl").unwrap();
        questions::finish_run(&self.db, &id, "succeeded", cost, 1000, 100, None, Some("Memory says CSV.")).unwrap();
        id
    }
    fn person_says(&self, text: &str) -> String {
        tick();
        comments::add(&self.db, &self.you, &self.task, text, None).unwrap()
    }
    fn note(&self, path: &str) -> Option<memory::Note> {
        memory::find(&self.db, path).unwrap()
    }
    /// The Team Lead's comments on the card (the agent's own summary is a comment too).
    fn leads_comments(&self) -> Vec<Comment> {
        comments::list(&self.db, &self.task).unwrap().into_iter().filter(|c| c.author_id == self.lead).collect()
    }
}

// ---- who takes the question ----

#[test]
fn a_task_agents_question_goes_to_the_team_lead_first_and_the_card_is_not_in_the_inbox_meanwhile() {
    let q = setup();
    assert!(questions::enabled(&q.db), "on by default");
    let (r, g) = q.ask();
    assert_eq!((g.hold.as_deref(), g.lead.as_deref()), (Some("needs_decision"), Some(q.lead.as_str())));
    let t = q.card();
    assert_eq!((t.hold.as_deref(), t.hold_reason.as_deref(), t.with_lead), (Some("needs_decision"), Some(QUESTION), true));
    assert_eq!(serde_json::to_value(&t).unwrap()["withLead"], true, "the UI gets it as withLead");
    // the same in the list the board, the Tasks page and the Inbox load
    assert!(tasks::list(&q.db, &TaskFilter::default()).unwrap().into_iter().find(|x| x.id == q.task).unwrap().with_lead);
    assert!(!q.in_inbox(), "not in the Inbox while the Team Lead has it");
    assert!(q.findings().is_empty(), "the board check leaves it to the Team Lead's run: {:?}", q.findings());
    let lead = q.lead_of(&r).unwrap();
    assert_eq!((lead.state.as_str(), lead.lead_id.as_deref(), lead.run_id.as_deref()), ("asking", Some(q.lead.as_str()), None));
    let v = serde_json::to_value(runs::get(&q.db, &r).unwrap()).unwrap();
    assert_eq!(v["lead"]["state"], "asking", "the Runs tab gets it as lead");
    // the verdict is kept next to it
    let json: String = q.db.read(|c| Ok(c.query_row("SELECT outcome_json FROM runs WHERE id=?1", [&r], |x| x.get(0))?)).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!((json["outcome"].as_str(), json["summary"].as_str(), json["lead"]["state"].as_str()), (Some("needs_decision"), Some(QUESTION), Some("asking")));
}

#[test]
fn the_step_switched_off_puts_the_card_in_the_inbox_at_once_as_before() {
    let q = setup();
    questions::set_enabled(&q.db, false).unwrap();
    assert!(!questions::enabled(&q.db));
    let (r, g) = q.ask();
    assert_eq!((g.hold.as_deref(), g.lead.as_deref()), (Some("needs_decision"), None));
    assert!(!q.card().with_lead && q.in_inbox());
    assert_eq!(q.lead_of(&r), None, "nothing recorded: the question went to you directly");
    assert_eq!(q.findings(), [("held".to_string(), "needs_decision".to_string())], "the board check sees it as before");
    // switched on again: the next card's question goes to the Team Lead
    questions::set_enabled(&q.db, true).unwrap();
    assert!(questions::enabled(&q.db));
}

#[test]
fn a_paused_team_lead_paused_agents_or_no_team_lead_send_the_question_to_the_inbox() {
    // the Team Lead paused
    let q = setup();
    team::set_agent_status(&q.db, &q.you, &q.lead, "paused").unwrap();
    let (r, g) = q.ask();
    assert_eq!(g.lead, None);
    assert!(q.in_inbox() && q.lead_of(&r).is_none());
    // Settings → Pause all agents
    let q = setup();
    settings::set(&q.db, "agents_paused", &true).unwrap();
    let (_, g) = q.ask();
    assert!(g.lead.is_none() && q.in_inbox());
    // no agent has Chat on: there is no Team Lead
    let q = setup();
    let m = team::agent(&q.db, &q.lead).unwrap();
    team::update_agent(&q.db, &q.you, &q.lead, AgentInput { name: m.name, role_key: m.role_key, chat_enabled: Some(false), ..Default::default() }).unwrap();
    let (_, g) = q.ask();
    assert!(g.lead.is_none() && q.in_inbox());
}

#[test]
fn a_team_lead_over_its_budget_or_not_on_claude_code_leaves_the_question_to_you() {
    // over its monthly budget (its own runs count)
    let q = setup();
    let m = team::agent(&q.db, &q.lead).unwrap();
    team::update_agent(&q.db, &q.you, &q.lead, AgentInput { name: m.name, role_key: m.role_key, budget_usd_micros: Some(100_000), ..Default::default() }).unwrap();
    let r = runs::create_with_trigger(&q.db, &q.lead, &q.task, "lead", "manual", "S0", "/tmp", "/tmp", "gizai/kade-1", "/tmp/l.jsonl").unwrap();
    runs::finish(&q.db, &r, "succeeded", None, 100_000, 0, 0, None).unwrap();
    let (_, g) = q.ask();
    assert!(g.lead.is_none() && q.in_inbox(), "at its budget");
    // its CLI changed to Codex in Settings after it got Chat (an agent with Chat on can't be put on Codex itself): the
    // question needs a Claude Code CLI
    let q = setup();
    let cc = q.cli("Second account", "claude_code");
    q.put_on(&q.lead, &cc);
    let list: Vec<Cli> = clis::list(&q.db).unwrap().into_iter().map(|c| if c.id == cc { Cli { kind: "codex".into(), ..c } } else { c }).collect();
    clis::save(&q.db, list).unwrap();
    assert_eq!(clis::get(&q.db, &cc).unwrap().kind, "codex");
    let (_, g) = q.ask();
    assert!(g.lead.is_none() && q.in_inbox());
    // on a second Claude Code account: that works
    let q = setup();
    let cc = q.cli("Claude Code 2", "claude_code");
    q.put_on(&q.lead, &cc);
    let (_, g) = q.ask();
    assert_eq!(g.lead.as_deref(), Some(q.lead.as_str()));
}

#[test]
fn the_team_leads_run_on_a_question_counts_toward_its_budget() {
    let q = setup();
    let m = team::agent(&q.db, &q.lead).unwrap();
    team::update_agent(&q.db, &q.you, &q.lead, AgentInput { name: m.name, role_key: m.role_key, budget_usd_micros: Some(150_000), ..Default::default() }).unwrap();
    let (r1, g) = q.ask();
    assert!(g.lead.is_some());
    q.lead_run(&r1, 150_000);
    assert!(questions::escalate(&q.db, &r1, None, "the budget is yours", "Needs Jeffrey: the budget.").unwrap());
    // the second question on the card would be allowed by the limits, but the Team Lead's look used its budget
    q.person_says("Go with CSV.");
    let (r2, g) = q.ask();
    assert_eq!(g.lead, None, "{:?}", q.lead_of(&r2));
    assert!(q.in_inbox());
}

#[test]
fn a_run_for_me_request_a_failed_push_the_team_leads_own_question_and_an_other_cli_skip_the_team_lead() {
    // Run this for me (GA-31): commands for you, not a question
    let q = setup();
    let cmds = vec!["sudo pacman -S libayatana-appindicator".to_string()];
    let (r, g) = q.ends_with(&q.be, "backend", &asks("The tray needs a system library."), &cmds);
    assert_eq!((g.hold.as_deref(), g.lead.as_deref()), (Some("needs_decision"), None));
    let t = q.card();
    assert!(!t.with_lead && t.run_for_me == cmds && q.in_inbox());
    assert!(q.lead_of(&r).is_none());
    // a failed push's blocked hold (GA-56), also with a needs_decision verdict
    let q = setup();
    tick();
    let r = runs::create(&q.db, &q.be, &q.task, "backend", "S", "/tmp", "/tmp", "gizai/kade-1", "/tmp/r.jsonl").unwrap();
    runs::finish(&q.db, &r, "succeeded", Some(&asks(QUESTION)), 0, 0, 0, None).unwrap();
    let g = workflow::hold_unpushed(&q.db, &r, Some(&asks(QUESTION)), "The push was refused").unwrap();
    assert_eq!((g.hold.as_deref(), g.lead.as_deref()), (Some("blocked"), None));
    let t = q.card();
    assert_eq!((t.hold.as_deref(), t.with_lead), (Some("blocked"), false));
    assert!(q.in_inbox() && q.lead_of(&r).is_none());
    // the Team Lead's own question goes to you
    let q = setup();
    let (_, g) = q.ends_with(&q.lead, "lead", &asks("Which client?"), &[]);
    assert!(g.lead.is_none() && q.in_inbox());
    // an agent on an Other CLI can't be continued with an answer
    let q = setup();
    let other = q.cli("My CLI", "other");
    q.put_on(&q.be, &other);
    let (_, g) = q.ask();
    assert!(g.lead.is_none() && q.in_inbox());
}

#[test]
fn a_gates_hold_is_no_question_for_the_team_lead() {
    // three QA bounces hold the card for a decision: that is the gate's, not an agent's question
    let q = setup();
    let qa = team::add_agent(&q.db, &q.you, &team::list(&q.db).unwrap()[0].id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(),
        ..Default::default() }).unwrap();
    let fail = Outcome { outcome: "qa_fail".into(), summary: "The export crashes".into(), issues: vec![] };
    let mut last = None;
    for _ in 0..3 {
        last = Some(q.ends_with(&qa, "qa", &fail, &[]).1);
    }
    let g = last.unwrap();
    assert_eq!((g.hold.as_deref(), g.lead.as_deref()), (Some("needs_decision"), None));
    assert!(q.in_inbox() && !q.card().with_lead);
}

// ---- the limits ----

#[test]
fn the_question_right_after_a_team_lead_answer_goes_to_you_and_the_board_check_leaves_it_to_you() {
    let q = setup();
    let (r1, _) = q.ask();
    let lr = q.lead_run(&r1, 50_000);
    assert!(questions::answering(&q.db, &r1, "Use CSV: Standards/Exports says so.", None).unwrap());
    assert!(q.card().with_lead && !q.in_inbox(), "still with the Team Lead while Gizai continues the agent");
    questions::answered(&q.db, &r1).unwrap();
    let l = q.lead_of(&r1).unwrap();
    assert_eq!((l.state.as_str(), l.answer.as_deref(), l.run_id.as_deref(), l.cost_usd_micros), ("answered", Some("Use CSV: Standards/Exports says so."),
               Some(lr.as_str()), 50_000));
    // the continued run asks again
    let (r2, g) = q.ask();
    assert_eq!(g.lead, None);
    let l = q.lead_of(&r2).unwrap();
    assert_eq!((l.state.as_str(), l.lead_id.as_deref()), ("limit", None));
    assert_eq!(l.reason.as_deref(), Some("the Team Lead answered this card's last question"));
    assert!(q.in_inbox() && !q.card().with_lead);
    assert!(q.findings().is_empty(), "no board finding: it waits for you, {:?}", q.findings());
    // you answer on the card: now the board check sees an answered card
    q.person_says("JSON after all.");
    assert_eq!(q.findings(), [("answered".to_string(), "answered".to_string())]);
}

#[test]
fn the_team_lead_takes_at_most_two_questions_per_card() {
    let q = setup();
    for i in 0..2 {
        let (r, g) = q.ask();
        assert!(g.lead.is_some(), "question {}", i + 1);
        assert!(questions::escalate(&q.db, &r, None, "it is about money", "Needs Jeffrey: money.").unwrap());
        assert!(q.in_inbox());
        q.person_says("Decided.");
    }
    let (r3, g) = q.ask();
    assert_eq!(g.lead, None);
    let l = q.lead_of(&r3).unwrap();
    assert_eq!((l.state.as_str(), l.reason.as_deref()), ("limit", Some("the Team Lead already took two questions on this card")));
    assert!(q.in_inbox());
    assert!(q.findings().is_empty(), "{:?}", q.findings());
}

#[test]
fn a_question_the_team_lead_couldnt_look_at_doesnt_count_toward_the_limit() {
    let q = setup();
    for _ in 0..2 {
        let (r, _) = q.ask();
        questions::skip(&q.db, &r, "The gizai-mcp helper is missing next to Gizai").unwrap();
        let l = q.lead_of(&r).unwrap();
        assert_eq!((l.state.as_str(), l.reason.as_deref()), ("skipped", Some("The gizai-mcp helper is missing next to Gizai")));
        assert!(q.in_inbox() && !q.card().with_lead, "skipped: the Inbox as before");
        assert!(q.leads_comments().is_empty(), "without a comment");
        q.person_says("CSV.");
    }
    let (_, g) = q.ask();
    assert!(g.lead.is_some(), "two skipped questions are no tries");
}

// ---- escalated ----

#[test]
fn escalated_the_team_leads_comment_and_the_reason_go_on_the_card_and_it_lands_in_the_inbox() {
    let q = setup();
    let (r, _) = q.ask();
    let before = q.card().hold_at.unwrap_or(0);
    tick();
    let lr = q.lead_run(&r, 30_000);
    let comment = "Needs Jeffrey: the client decides on the format.\n\nOptions:\n1. CSV\n2. JSON\n\nMy advice: CSV, the accountant uses Excel.";
    assert!(questions::escalate(&q.db, &r, Some(&lr), "the client decides on the format", comment).unwrap());
    let t = q.card();
    assert_eq!((t.hold.as_deref(), t.hold_reason.as_deref(), t.with_lead),
               (Some("needs_decision"), Some("Team Lead escalated to you: the client decides on the format"), false));
    assert!(t.hold_at.unwrap_or(0) > before, "the hold is new, so it notifies");
    assert!(q.in_inbox());
    let c = q.leads_comments();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].author_id.as_str(), c[0].body_md.as_str(), c[0].run_id.as_deref()), (q.lead.as_str(), comment, Some(lr.as_str())));
    let l = q.lead_of(&r).unwrap();
    assert_eq!((l.state.as_str(), l.reason.as_deref(), l.comment_id.as_deref(), l.cost_usd_micros),
               ("escalated", Some("the client decides on the format"), Some(c[0].id.as_str()), 30_000));
    // the board check leaves it to you until you answer; the Team Lead's own comment is no answer
    assert!(q.findings().is_empty(), "{:?}", q.findings());
    q.person_says("CSV.");
    assert_eq!(q.findings(), [("answered".to_string(), "answered".to_string())]);
}

#[test]
fn a_card_you_took_over_first_is_dropped_nothing_is_posted_or_held() {
    // you cleared the hold while the Team Lead looked: its escalation posts nothing
    let q = setup();
    let (r, _) = q.ask();
    tasks::update(&q.db, &q.you, &q.task, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    assert!(!questions::waiting(&q.db, &r).unwrap());
    assert!(!questions::escalate(&q.db, &r, None, "money", "Needs Jeffrey: money.").unwrap());
    assert_eq!(q.state_of(&r).as_deref(), Some("dropped"));
    let t = q.card();
    assert_eq!(t.hold, None);
    assert!(q.leads_comments().is_empty());
    // or before its answer was recorded: no answer either
    let q = setup();
    let (r, _) = q.ask();
    assert!(questions::waiting(&q.db, &r).unwrap());
    tasks::update(&q.db, &q.you, &q.task, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    assert!(!questions::answering(&q.db, &r, "CSV", None).unwrap());
    assert_eq!(q.state_of(&r).as_deref(), Some("dropped"));
}

#[test]
fn questions_that_waited_for_the_team_lead_when_gizai_stopped_go_to_you_at_the_next_start() {
    let q = setup();
    let (r, _) = q.ask();
    // its run on the question was running when Gizai stopped
    let lr = questions::create_run(&q.db, &q.lead, &r, "LS", "/tmp/lead", "/tmp/lead.jsonl").unwrap();
    runs::set_running(&q.db, &lr, 4242).unwrap();
    assert_eq!(runs::recover_interrupted(&q.db).unwrap(), 1);
    let lrun = runs::get(&q.db, &lr).unwrap();
    assert_eq!((lrun.status.as_str(), lrun.error.as_deref(), lrun.question_task_id.as_deref()), ("failed", Some("interrupted"), Some(q.task.as_str())));
    assert_eq!(questions::recover(&q.db).unwrap(), 1);
    let t = q.card();
    assert_eq!(t.hold_reason.as_deref(), Some("Team Lead escalated to you: Gizai stopped before it had answered"));
    assert!(q.in_inbox());
    let c = q.leads_comments();
    assert_eq!(c.len(), 1);
    assert_eq!((c[0].author_id.as_str(), c[0].body_md.as_str()), (q.lead.as_str(), "Gizai stopped before I had answered this question, so it is yours to decide."));
    assert_eq!(questions::recover(&q.db).unwrap(), 0, "once");
}

// ---- the Team Lead's run on a question ----

#[test]
fn the_team_leads_run_on_a_question_has_no_card_reads_as_question_and_keeps_the_agents_run_the_cards_latest() {
    let q = setup();
    let (r, _) = q.ask();
    let lr = questions::create_run(&q.db, &q.lead, &r, "LS", "/tmp/lead", "/tmp/lead.jsonl").unwrap();
    let run = runs::get(&q.db, &lr).unwrap();
    assert_eq!((run.trigger.as_str(), run.task_id.as_deref(), run.question_task_id.as_deref(), run.role_key.as_deref(), run.status.as_str()),
               ("question", None, Some(q.task.as_str()), Some("lead"), "queued"));
    assert_eq!(runs::list_for_task(&q.db, &q.task).unwrap()[0].id, r, "the card's latest run is still the one that asked");
    assert_eq!(q.lead_of(&r).unwrap().run_id.as_deref(), Some(lr.as_str()));
    assert!(runs::list_for_agent(&q.db, &q.lead, 10).unwrap().iter().any(|x| x.id == lr && x.trigger == "question"), "on its agent page");
    questions::finish_run(&q.db, &lr, "succeeded", 42_000, 1200, 300, None, Some("Decisions/Kade says CSV.")).unwrap();
    let run = runs::get(&q.db, &lr).unwrap();
    assert_eq!((run.status.as_str(), run.cost_usd_micros, run.summary_md.as_deref()), ("succeeded", 42_000, Some("Decisions/Kade says CSV.")));
    assert_eq!(q.lead_of(&r).unwrap().cost_usd_micros, 42_000, "the asking run shows what the Team Lead's look cost");
    assert!(questions::finish_run(&q.db, &r, "succeeded", 0, 0, 0, None, None).is_err(), "only the Team Lead's run on a question");
    assert!(questions::finish_run(&q.db, &lr, "done", 0, 0, 0, None, None).is_err());
    // the question it reads
    let qq = questions::question(&q.db, &r).unwrap();
    assert_eq!((qq.identifier.as_str(), qq.title.as_str(), qq.agent_name.as_str(), qq.role.as_str(), qq.project_key.as_deref()),
               ("KADE-1", "Export invoices", "Backend Agent", "backend", Some("KADE")));
    assert_eq!(qq.text, format!("{QUESTION}\n\n1. Which format should the export use?"));
}

// ---- memory ----

#[test]
fn an_answer_is_kept_in_the_projects_decisions_note_linked_to_the_card() {
    let q = setup();
    let (r, _) = q.ask();
    let lr = q.lead_run(&r, 10_000);
    let saved = questions::remember(&q.db, &q.lead, &r, Some(&lr), "Team Lead", "Use CSV, like the other exports.", None, None, "2026-10-10").unwrap();
    assert_eq!((saved.path.as_str(), saved.created), ("Decisions/Kade", true));
    assert_eq!(questions::decisions_path(Some("Kade")), "Decisions/Kade");
    assert_eq!(questions::decisions_path(None), "Decisions/General");
    let note = q.note("Decisions/Kade").unwrap();
    assert_eq!(note.scope, "shared");
    assert!(note.body_md.starts_with("---\ntype: decision\nproject: KADE\n---\n# Kade\n"), "{}", note.body_md);
    assert!(note.body_md.contains(&format!("- 2026-10-10 (KADE-1, answered by Team Lead): {QUESTION} → Use CSV, like the other exports.")), "{}", note.body_md);
    assert!(memory::task_refs(&note.body_md).contains(&"KADE-1".to_string()), "linked to the card");
    // the agents on that project's cards get it
    assert_eq!(memory::properties(&note.body_md).get("project").cloned(), Some(vec!["KADE".to_string()]));
    // a second answer is added to the same note
    let saved = questions::remember(&q.db, &q.lead, &r, None, "Team Lead", "Semicolons.", None, None, "2026-10-11").unwrap();
    assert_eq!((saved.path.as_str(), saved.created), ("Decisions/Kade", false));
    let body = q.note("Decisions/Kade").unwrap().body_md;
    assert!(body.contains("- 2026-10-10 (KADE-1") && body.contains("- 2026-10-11 (KADE-1, answered by Team Lead): CSV or JSON for the export? → Semicolons."), "{body}");
    questions::noted(&q.db, &r, &saved.path).unwrap();
    assert_eq!(q.lead_of(&r).unwrap().note.as_deref(), Some("Decisions/Kade"));
}

#[test]
fn an_answer_goes_to_the_shared_note_the_team_lead_names_else_to_decisions() {
    let q = setup();
    let (r, _) = q.ask();
    let saved = questions::remember(&q.db, &q.lead, &r, None, "Team Lead", "CSV", Some("Standards/Exports"), Some("Exports are CSV with semicolons"),
                                    "2026-10-10").unwrap();
    assert_eq!(saved.path, "Standards/Exports");
    let body = q.note("Standards/Exports").unwrap().body_md;
    assert!(body.contains("- 2026-10-10: Exports are CSV with semicolons (KADE-1)"), "the card's identifier is added: {body}");
    // already naming the card: not twice
    questions::remember(&q.db, &q.lead, &r, None, "Team Lead", "CSV", Some("Standards/Exports"), Some("KADE-1: dates as ISO"), "2026-10-10").unwrap();
    assert!(q.note("Standards/Exports").unwrap().body_md.contains("- 2026-10-10: KADE-1: dates as ISO\n") || q.note("Standards/Exports").unwrap().body_md.ends_with("- 2026-10-10: KADE-1: dates as ISO"));
    // not a shared folder (the Team Lead's own, or an agent's): Decisions/<project>
    let saved = questions::remember(&q.db, &q.lead, &r, None, "Team Lead", "CSV", Some("Agents/Backend Agent/Notes"), Some("CSV"), "2026-10-10").unwrap();
    assert_eq!(saved.path, "Decisions/Kade");
    // a path without text: Decisions/<project> too
    let saved = questions::remember(&q.db, &q.lead, &r, None, "Team Lead", "CSV", Some("Standards/Exports"), None, "2026-10-10").unwrap();
    assert_eq!(saved.path, "Decisions/Kade");
}

#[test]
fn your_answer_to_an_escalated_question_is_kept_in_memory_once_when_the_agent_starts_again() {
    let q = setup();
    q.person_says("Before the question: the client is on holiday.");
    let (r, _) = q.ask();
    tick();
    assert!(questions::escalate(&q.db, &r, None, "the client decides", "Needs Jeffrey: the client decides.").unwrap());
    // nothing to learn yet: nobody answered since (the Team Lead's comment and older ones don't count)
    assert!(questions::learn_from_person(&q.db, &q.task, "no-run-yet", "2026-10-10").unwrap().is_none());
    assert!(q.note("Decisions/Kade").is_none());
    // you answer, and the agent starts again (the app calls this as the run starts)
    q.person_says("CSV, the client said so on the phone.");
    tick();
    let next = runs::create(&q.db, &q.be, &q.task, "backend", "S2", "/tmp", "/tmp", "gizai/kade-1", "/tmp/r2.jsonl").unwrap();
    let saved = questions::learn_from_person(&q.db, &q.task, &next, "2026-10-10").unwrap().expect("learned");
    assert_eq!(saved.path, "Decisions/Kade");
    let body = q.note("Decisions/Kade").unwrap().body_md;
    assert!(body.contains(&format!("- 2026-10-10 (KADE-1, answered by Jeffrey): {QUESTION} → CSV, the client said so on the phone.")), "{body}");
    assert!(!body.contains("holiday"), "only what was said after the Team Lead asked: {body}");
    assert!(memory::get(&q.db, &Who::Lead(q.lead.clone()), "Decisions/Kade").unwrap().updated_by.as_deref() == Some("Team Lead"), "kept as the Team Lead's");
    let l = q.lead_of(&r).unwrap();
    assert!(l.learned && l.note.as_deref() == Some("Decisions/Kade"), "{l:?}");
    // once per question
    assert!(questions::learn_from_person(&q.db, &q.task, &next, "2026-10-10").unwrap().is_none());
    assert_eq!(q.note("Decisions/Kade").unwrap().body_md.matches("answered by Jeffrey").count(), 1);
}

#[test]
fn an_answered_question_teaches_nothing_new_when_the_agent_starts_again() {
    let q = setup();
    let (r, _) = q.ask();
    assert!(questions::answering(&q.db, &r, "CSV", None).unwrap());
    questions::answered(&q.db, &r).unwrap();
    q.person_says("Thanks.");
    assert!(questions::learn_from_person(&q.db, &q.task, "no-run-yet", "2026-10-10").unwrap().is_none(), "only an escalated question");
}
