use gizai_agents::prompt::{build, continue_prompt, BaseInfo, RunLimits, TaskContext};

fn ctx() -> TaskContext {
    TaskContext { identifier: "KADE-41".into(), title: "Export invoices as CSV".into(), description_md: "Use semicolons.".into(),
                  acceptance_md: "- [ ] opens in Excel NL".into(), recent_comments: vec![("Jeffrey".into(), "Sample file is attached.".into())],
                  role: "backend".into(), qa_issues: vec![], limits: None, base: None, project_goal_md: String::new() }
}

#[test]
fn puts_instructions_first_then_the_task() {
    let p = build(&ctx(), "You are the Backend Agent.");
    assert!(p.starts_with("You are the Backend Agent."));
    let (i_task, i_desc, i_acc, i_com) = (p.find("KADE-41: Export invoices as CSV").unwrap(), p.find("Use semicolons.").unwrap(),
        p.find("opens in Excel NL").unwrap(), p.find("Jeffrey: Sample file is attached.").unwrap());
    assert!(i_task < i_desc && i_desc < i_acc && i_acc < i_com);
    assert!(!p.contains("QA found"));
}

#[test]
fn lists_qa_issues_and_fills_gaps() {
    let p = build(&TaskContext { qa_issues: vec!["Pin overlaps the header".into()], acceptance_md: "".into(), recent_comments: vec![], ..ctx() }, "R");
    assert!(p.contains("QA found these issues"));
    assert!(p.contains("1. Pin overlaps the header"));
    assert!(p.contains("No acceptance criteria were written"));
}

#[test]
fn qa_issues_keep_numbers_that_are_not_list_markers() {
    let p = build(&TaskContext { qa_issues: vec!["404 on /login".into(), "2) 3 tests fail in CI".into(), "3.Bad spacing".into()], ..ctx() }, "R");
    assert!(p.contains("1. 404 on /login"), "{p}");
    assert!(p.contains("2. 3 tests fail in CI"), "{p}");
    assert!(p.contains("3. Bad spacing"), "{p}");
}

#[test]
fn tells_the_agent_its_limits_and_to_save_work_early() {
    let p = build(&TaskContext { limits: Some(RunLimits { minutes: 60, tool_calls: 200 }), ..ctx() }, "R");
    assert!(p.contains("200 tool calls") && p.contains("60 minutes"), "{p}");
    assert!(p.contains("commit"), "{p}");
    assert!(!build(&ctx(), "R").contains("tool calls"));
}

#[test]
fn a_continued_run_hears_why_it_stopped_and_what_to_check_first() {
    let p = continue_prompt("stopped at the limit of 200 tool calls per run", Some(RunLimits { minutes: 90, tool_calls: 200 }));
    assert!(p.contains("stopped at the limit of 200 tool calls"), "{p}");
    assert!(p.contains("git status") && p.contains("GIZAI_RESULT") && p.contains("90 minutes"), "{p}");
}

#[test]
fn says_where_the_branch_started_and_when_github_main_moved_on() {
    let fresh = build(&TaskContext { base: Some(BaseInfo { from: "acme-labs/main".into(), behind: 0 }), ..ctx() }, "R");
    assert!(fresh.contains("acme-labs/main") && !fresh.contains("Merge"), "{fresh}");
    let behind = build(&TaskContext { base: Some(BaseInfo { from: "acme-labs/main".into(), behind: 3 }), ..ctx() }, "R");
    assert!(behind.contains("3 commits") && behind.contains("Merge acme-labs/main"), "{behind}");
    assert!(!build(&ctx(), "R").contains("## Your branch"));
}

#[test]
fn includes_the_project_goal_the_project_page_promises() {
    let p = build(&TaskContext { project_goal_md: "Read CLAUDE.md first.".into(), ..ctx() }, "R");
    let goal = p.find("## Project goal").expect("a goal section");
    assert!(p[goal..].contains("Read CLAUDE.md first.") && goal < p.find("## Description").unwrap(), "{p}");
    assert!(!build(&ctx(), "R").contains("## Project goal"));
}
