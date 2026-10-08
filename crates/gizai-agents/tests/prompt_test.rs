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

// GA-48: "How this run works" at the end of every task prompt.
use gizai_agents::cli::Kind;
use gizai_agents::prompt::{answered_prompt, rules_section, with_rules, RunRules};

fn rules(kind: Kind, mode: &str) -> RunRules {
    RunRules { kind, mode: mode.into(), allowed_tools: vec!["Bash(git status:*)".into(), "Bash(npm test)".into(), "Bash(./vendor/bin/*)".into(),
        "WebFetch(domain:docs.rs)".into()], folders: vec!["/home/u/notes".into()], temp_dir: Some("/w/KADE-1/.gizai-tmp".into()) }
}

fn bullets(s: &str) -> usize {
    s.lines().filter(|l| l.starts_with("- ")).count()
}

#[test]
fn every_task_prompt_new_continued_and_answered_ends_with_how_this_run_works() {
    let r = rules(Kind::ClaudeCode, "");
    for p in [build(&ctx(), "You are the Backend Agent."), continue_prompt("stopped at the limit of 200 tool calls per run", None),
              answered_prompt("Use semicolons.", None)] {
        let full = with_rules(&p, &r);
        assert!(full.starts_with(p.trim_end()), "the prompt comes first: {full}");
        assert!(full.trim_end().ends_with(rules_section(&r).trim_end()), "the section comes last: {full}");
        assert_eq!(full.matches("## How this run works").count(), 1);
    }
}

#[test]
fn claude_code_in_accept_edits_hears_its_commands_the_checked_shell_rules_and_the_temp_path() {
    for mode in ["", "acceptEdits"] {
        let s = rules_section(&rules(Kind::ClaudeCode, mode));
        for want in ["## How this run works", "Nobody can approve anything during this run: a command or tool that needs approval is refused.",
                     "`git status`", "`npm test` (exact)", "`./vendor/bin/*`", "Also allowed: `WebFetch(domain:docs.rs)`",
                     "this worktree and your folders (`/home/u/notes`)", "`/tmp`", "`$(…)`", "backticks", "`$TMPDIR`", "`<<EOF`",
                     "Pipes, `2>&1`", "Make files with the Write tool.", "`/w/KADE-1/.gizai-tmp`", "write this path, not `$TMPDIR`",
                     "never in `/tmp`", "never committed", "don't try other spellings of it", "under what you could not check"] {
            assert!(s.contains(want), "{mode:?}: {want} missing in {s}");
        }
        // checked against Claude Code 2.1.289 and false there, so left out
        assert!(!s.contains("One plain command per Bash call"), "{s}");
        assert!(!s.contains("redirects (`>`, `>>`, `2> file`)") && !s.to_lowercase().contains("redirects are refused"), "{s}");
        assert!(bullets(&s) <= 12, "at most about 12 lines: {s}");
    }
}

#[test]
fn only_rules_that_hold_for_the_mode_and_the_cli_go_in() {
    // bypassPermissions: no list and no shell rules (nothing is refused for them)
    let s = rules_section(&rules(Kind::ClaudeCode, "bypassPermissions"));
    assert!(!s.contains("The commands you may run") && !s.contains("`$(…)`"), "{s}");
    assert!(s.contains("`/w/KADE-1/.gizai-tmp`") && s.contains("Make files with the Write tool."), "{s}");
    // a stricter mode: the list, but none of the shell rules checked in acceptEdits, and no Write tool to make files
    let s = rules_section(&rules(Kind::ClaudeCode, "default"));
    assert!(s.contains("`git status`") && !s.contains("`$(…)`") && !s.contains("Write tool"), "{s}");
    let s = rules_section(&RunRules { allowed_tools: vec!["Write".into()], ..rules(Kind::ClaudeCode, "default") });
    assert!(s.contains("No commands are allowed for you.") && s.contains("Make files with the Write tool."), "{s}");
    // Codex: no allowed list (its sandbox decides), no Claude Code shell rules
    let s = rules_section(&rules(Kind::Codex, ""));
    assert!(s.contains("Nobody can approve anything") && s.contains("`/w/KADE-1/.gizai-tmp`") && s.contains("don't try other spellings"), "{s}");
    assert!(!s.contains("The commands you may run") && !s.contains("`$(…)`") && !s.contains("Write tool"), "{s}");
    // Gemini: its commands (the Bash rules it is given), its own file tool
    let s = rules_section(&rules(Kind::Gemini, ""));
    assert!(s.contains("`git status`") && s.contains("write_file tool") && !s.contains("WebFetch") && !s.contains("`$(…)`"), "{s}");
    assert!(!rules_section(&rules(Kind::Gemini, "yolo")).contains("The commands you may run"));
    // another CLI: no approvals, the temp folder
    let s = rules_section(&rules(Kind::Other, ""));
    assert!(s.contains("Nobody can approve anything during this run.") && s.contains("`/w/KADE-1/.gizai-tmp`") && !s.contains("commands"), "{s}");
    // without a temp folder (Gizai couldn't make it): no path, still not /tmp
    let s = rules_section(&RunRules { temp_dir: None, ..rules(Kind::ClaudeCode, "") });
    assert!(s.contains("Throwaway files never go in `/tmp`") && !s.contains(".gizai-tmp") && !s.contains("TMPDIR, TMP and TEMP point"), "{s}");
}
