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

// GA-54: the waiting rule in "How this run works", and Gizai's nudge.
use gizai_agents::prompt::nudge_prompt;

const ENDS: &str = "Ending your message ends the run: nothing wakes you up later";
const WAIT: &str = "To wait for something outside this run (a CI run, a release or deploy workflow, a pull request's checks)";
const LIMIT: &str = "If it won't be done before the limit, don't wait for it: end with your GIZAI_RESULT line, and say in your summary what to \
check and what is left.";

fn with_tools(kind: Kind, mode: &str, tools: &[&str]) -> RunRules {
    RunRules { allowed_tools: tools.iter().map(|t| t.to_string()).collect(), ..rules(kind, mode) }
}

/// Gizai's default list (`DEFAULT_TOOLS` in src-tauri/src/runs.rs), which has `sleep` since GA-54.
fn default_list(kind: Kind, mode: &str) -> RunRules {
    with_tools(kind, mode, &["Bash(git status:*)", "Bash(git commit:*)", "Bash(npm:*)", "Bash(cargo:*)", "Bash(ls:*)", "Bash(cat:*)",
                             "Bash(head:*)", "Bash(grep:*)", "Bash(pwd:*)", "Bash(tree:*)", "Bash(sleep:*)"])
}

fn names_sleep(r: &RunRules) -> bool {
    let s = rules_section(r);
    let named = s.contains("`<check>; sleep 45`");
    assert_eq!(named, s.contains("about once a minute") && s.contains("then the sleep in one command"), "{s}");
    // without it, sleep isn't named anywhere (an allowed `sleeper` is only the agent's own list)
    assert_eq!(named, s.replace("`sleeper`", "").contains("sleep"), "sleep is named in the waiting rule or not at all: {s}");
    named
}

#[test]
fn every_task_prompt_new_continued_answered_and_nudged_has_the_waiting_rule_on_every_cli() {
    let limits = Some(RunLimits { minutes: 90, tool_calls: 200 });
    for kind in [Kind::ClaudeCode, Kind::Codex, Kind::Gemini, Kind::Other] {
        for r in [rules(kind, ""), default_list(kind, "")] {
            for (which, p) in [("new", build(&TaskContext { limits, ..ctx() }, "You are the DevOps Agent.")),
                               ("continued", continue_prompt("stopped at the limit of 200 tool calls per run", limits)),
                               ("answered", answered_prompt("Release 1.11.0, please.", limits)), ("nudged", nudge_prompt(limits))] {
                let full = with_rules(&p, &r);
                let sec = &full[full.find("## How this run works").unwrap_or_else(|| panic!("{kind:?} {which}: no section in {full}"))..];
                for want in [ENDS, WAIT, "in the foreground", LIMIT] {
                    assert!(sec.contains(want), "{kind:?} {which}: {want} missing in {sec}");
                }
                assert_eq!(full.matches(ENDS).count(), 1, "{kind:?} {which}: once: {full}");
                // it goes before the last line, about refusals, and the section stays about 12 lines
                assert!(sec.find(LIMIT).unwrap() < sec.find("don't try other spellings").unwrap(), "{kind:?} {which}: {sec}");
                assert!(bullets(sec) <= 12, "{kind:?} {which}: at most about 12 lines: {sec}");
            }
        }
    }
}

#[test]
fn the_waiting_rule_names_sleep_only_when_the_agent_may_run_it() {
    // Claude Code: its list allows sleep with any arguments, or every command; or its mode runs everything
    for tools in [&["Bash(git status:*)", "Bash(sleep:*)"][..], &["Bash(sleep *)"], &["Bash(sleep*)"], &["Bash"], &["Bash(*)"], &[" Bash( sleep:* ) "]] {
        assert!(names_sleep(&with_tools(Kind::ClaudeCode, "", tools)), "{tools:?}");
    }
    assert!(names_sleep(&default_list(Kind::ClaudeCode, "acceptEdits")));
    assert!(names_sleep(&rules(Kind::ClaudeCode, "bypassPermissions")), "every command runs");
    for tools in [&["Bash(git status:*)", "Bash(npm test)"][..], &["Bash(sleeper:*)"], &["Bash(git:*)"], &["Write", "Edit"], &[]] {
        assert!(!names_sleep(&with_tools(Kind::ClaudeCode, "", tools)), "{tools:?}");
    }
    // Codex has no list (its sandbox decides): always
    assert!(names_sleep(&rules(Kind::Codex, "")) && names_sleep(&with_tools(Kind::Codex, "", &[])));
    // Gemini: its list (as run_shell_command rules) or yolo; a bare Bash is no Gemini rule
    assert!(names_sleep(&default_list(Kind::Gemini, "")) && names_sleep(&rules(Kind::Gemini, "yolo")));
    assert!(!names_sleep(&rules(Kind::Gemini, "")) && !names_sleep(&with_tools(Kind::Gemini, "", &["Bash"])));
    // another CLI: Gizai doesn't know what it may run
    assert!(!names_sleep(&default_list(Kind::Other, "")) && !names_sleep(&rules(Kind::Other, "")));
    // without sleep the agent still hears to check again in the foreground
    let s = rules_section(&rules(Kind::ClaudeCode, ""));
    assert!(s.contains("check it again in the foreground until it is done, within this run's limits"), "{s}");
}

#[test]
fn claude_code_also_hears_what_was_checked_against_it_and_other_clis_do_not() {
    let s = rules_section(&default_list(Kind::ClaudeCode, ""));
    for want in ["a command still running in the background (run_in_background) is stopped", "Don't write a loop (`for`, `while`, `until`)",
                 "A command that starts with a sleep longer than 20 seconds is blocked.", "longer than 2 minutes", "at most 10 minutes",
                 "is moved to the background", "one check and then the sleep in one command"] {
        assert!(s.contains(want), "{want} missing in {s}");
    }
    assert!(!s.contains("Monitor"), "the rule doesn't offer a Monitor: {s}");
    // without sleep: the loop and timeout lines, not the 20-second block
    let s = rules_section(&rules(Kind::ClaudeCode, ""));
    assert!(s.contains("Don't write a loop") && s.contains("2 minutes") && !s.contains("20 seconds"), "{s}");
    for kind in [Kind::Codex, Kind::Gemini, Kind::Other] {
        let s = rules_section(&default_list(kind, "yolo"));
        assert!(s.contains("anything still running in the background is stopped"), "{kind:?}: {s}");
        for not in ["run_in_background", "Don't write a loop", "20 seconds", "2 minutes", "10 minutes"] {
            assert!(!s.contains(not), "{kind:?}: {not} is Claude Code's only: {s}");
        }
    }
}

#[test]
fn the_nudge_says_the_run_ended_without_its_result_and_to_check_in_the_foreground_now() {
    let p = nudge_prompt(Some(RunLimits { minutes: 90, tool_calls: 200 }));
    for want in ["Your run ended without your GIZAI_RESULT line", "nothing wakes you up later", "If you were waiting for something",
                 "check it in the foreground now", "End with your GIZAI_RESULT line.", "## Limits of this run", "200 tool calls", "90 minutes"] {
        assert!(p.contains(want), "{want} missing in {p}");
    }
    assert!(p.starts_with("Your run ended") && p.ends_with('\n') && !p.ends_with("\n\n"), "{p:?}");
    let short = nudge_prompt(None);
    assert!(!short.contains("## Limits") && short.contains("check it in the foreground now"), "{short}");
    assert!(!p.contains("was stopped") && !p.contains("asking for a decision"), "not Continue's or an answer's words: {p}");
}

// GA-56: Gizai pushes the card's branch after every run, so a refused push is no reason to stop.
use gizai_agents::prompt::PUSHED_BY_GIZAI;

#[test]
fn every_task_prompt_on_every_cli_says_gizai_pushes_the_branch_after_the_run() {
    for want in ["When the run ends, Gizai itself pushes this branch's commits (not uncommitted changes)", "a refused `git push` of this branch",
                 "is no reason for `needs_decision`", "mention it in your summary", "end with the outcome your work deserves"] {
        assert!(PUSHED_BY_GIZAI.contains(want), "{want} missing in {PUSHED_BY_GIZAI}");
    }
    let limits = Some(RunLimits { minutes: 90, tool_calls: 200 });
    for kind in [Kind::ClaudeCode, Kind::Codex, Kind::Gemini, Kind::Other] {
        for mode in ["", "acceptEdits", "auto", "default", "bypassPermissions", "yolo"] {
            for r in [rules(kind, mode), default_list(kind, mode)] {
                for (which, p) in [("new", build(&TaskContext { limits, ..ctx() }, "You are the Backend Agent.")),
                                   ("continued", continue_prompt("stopped at the limit of 200 tool calls per run", limits)),
                                   ("answered", answered_prompt("Use semicolons.", limits)), ("nudged", nudge_prompt(limits))] {
                    let full = with_rules(&p, &r);
                    let sec = &full[full.find("## How this run works").unwrap()..];
                    assert_eq!(sec.matches(PUSHED_BY_GIZAI).count(), 1, "{kind:?} {mode:?} {which}: {sec}");
                    assert_eq!(full.matches("Gizai itself pushes").count(), 1, "{kind:?} {mode:?} {which}: once");
                    // a bullet of its own, before the last line about refusals, and the section stays about 12 lines
                    assert!(sec.contains(&format!("\n- {PUSHED_BY_GIZAI}\n")), "{kind:?} {mode:?} {which}: {sec}");
                    assert!(sec.find(PUSHED_BY_GIZAI).unwrap() < sec.find("don't try other spellings").unwrap(), "{kind:?} {mode:?} {which}: {sec}");
                    assert!(bullets(sec) <= 12, "{kind:?} {mode:?} {which}: at most about 12 lines: {sec}");
                }
            }
        }
    }
}

// GA-31: Continue with a message (a note next to why the run stopped, or next to the answer), and "Run this for me" in
// How this run works.
use gizai_agents::prompt::{answered_prompt_with, continue_prompt_with, Note, RUN_FOR_ME};

fn note(from: &str, text: &str) -> Note {
    Note { from: from.into(), text: text.into() }
}

#[test]
fn a_continue_with_a_note_quotes_it_after_why_the_run_stopped_and_before_the_instructions() {
    let limits = Some(RunLimits { minutes: 90, tool_calls: 200 });
    let n = note("Jeffrey", "  Use the existing CSV writer.\nKeep the column order.\n ");
    let p = continue_prompt_with("stopped at the limit of 200 tool calls per run.", Some(&n), limits);
    let (why, by, first, second, go) = (p.find("Your last run on this task was stopped: stopped at the limit of 200 tool calls per run.").unwrap(),
        p.find("Jeffrey wrote a note for this run:\n\n").unwrap(), p.find("> Use the existing CSV writer.\n> Keep the column order.").unwrap(),
        p.find("> Keep the column order.").unwrap(), p.find("Continue where you left off.").unwrap());
    assert!(why < by && by < first && first < second && second < go, "{p}");
    assert!(p.contains("## Limits of this run") && p.find("## Limits of this run").unwrap() > go, "{p}");
    assert_eq!(p.matches("wrote a note").count(), 1);
    // no note, an empty one or only spaces: the plain Continue, word for word
    let plain = continue_prompt("stopped at the limit of 200 tool calls per run", limits);
    for empty in [None, Some(note("Jeffrey", "")), Some(note("Jeffrey", " \n  "))] {
        assert_eq!(continue_prompt_with("stopped at the limit of 200 tool calls per run", empty.as_ref(), limits), plain);
    }
    assert!(!plain.contains("wrote a note"));
    // a note without a name still reads
    let p = continue_prompt_with("it ended without a result", Some(&note(" ", "Try again")), None);
    assert!(p.contains("Someone wrote a note for this run:\n\n> Try again\n\n"), "{p}");
}

#[test]
fn an_answered_continue_quotes_the_answer_then_the_note_and_a_note_alone_is_the_answer() {
    let n = note("Team Lead", "Use JSON: the client's importer reads it.");
    let p = answered_prompt_with("Jeffrey: CSV or JSON? JSON.", Some(&n), None);
    let (asked, answer, by, text, go) = (p.find("Your last run on this task ended asking for a decision.").unwrap(),
        p.find("It was answered on the card since:\n\n> Jeffrey: CSV or JSON? JSON.").unwrap(), p.find("Team Lead wrote a note for this run:").unwrap(),
        p.find("> Use JSON: the client's importer reads it.").unwrap(), p.find("Continue where you left off, with this answer.").unwrap());
    assert!(asked < answer && answer < by && by < text && text < go, "{p}");
    // Done, continue: the note is the whole answer
    let ran = note("Jeffrey", "Done: I ran the command you asked me to run.\n\n```sh\nsudo pacman -S libayatana-appindicator\n```\n\nCheck that it worked, then carry on.");
    let p = answered_prompt_with("", Some(&ran), Some(RunLimits { minutes: 90, tool_calls: 200 }));
    assert!(!p.contains("It was answered on the card since"), "{p}");
    // (a blank line in the note is quoted as "> ")
    assert!(p.contains("Jeffrey wrote a note for this run:\n\n> Done: I ran the command you asked me to run.\n> \n> ```sh\n\
> sudo pacman -S libayatana-appindicator\n> ```\n> \n> Check that it worked, then carry on.\n\nContinue where you left off"), "{p}");
    assert!(p.contains("## Limits of this run"), "{p}");
    // without a note: as before
    assert_eq!(answered_prompt_with("Use semicolons.", None, None), answered_prompt("Use semicolons.", None));
    assert!(answered_prompt("Use semicolons.", None).contains("It was answered on the card since:\n\n> Use semicolons.\n\n"));
}

#[test]
fn every_task_prompt_on_every_cli_says_how_to_ask_the_user_to_run_a_command() {
    for want in ["a command you may not run", "sudo", "an install", "not a push of this branch", "finish what you can", "`needs_decision`",
                 "exactly as it is to be typed", "`run_for_me`", "The user runs them and continues this run", "check that they worked"] {
        assert!(RUN_FOR_ME.contains(want), "{want} missing in {RUN_FOR_ME}");
    }
    // its example is a working result line
    let example = RUN_FOR_ME.split('`').find(|s| s.starts_with("\"run_for_me\"")).unwrap();
    let line = format!("GIZAI_RESULT: {{\"outcome\":\"needs_decision\",\"summary\":\"s\",\"issues\":[],{example}}}");
    assert_eq!(gizai_agents::outcome::parse(&line).unwrap().run_for_me, ["sudo pacman -S libayatana-appindicator"]);
    let limits = Some(RunLimits { minutes: 90, tool_calls: 200 });
    for kind in [Kind::ClaudeCode, Kind::Codex, Kind::Gemini, Kind::Other] {
        for mode in ["", "acceptEdits", "auto", "default", "bypassPermissions", "yolo"] {
            for r in [rules(kind, mode), default_list(kind, mode)] {
                for (which, p) in [("new", build(&TaskContext { limits, ..ctx() }, "You are the Backend Agent.")),
                                   ("continued", continue_prompt_with("it ended without a result", Some(&note("Jeffrey", "Use the CSV writer")), limits)),
                                   ("answered", answered_prompt("Use semicolons.", limits)), ("nudged", nudge_prompt(limits))] {
                    let full = with_rules(&p, &r);
                    let sec = &full[full.find("## How this run works").unwrap()..];
                    // a bullet of its own, once, with the push line kept, and the section stays about 12 lines
                    assert!(sec.contains(&format!("\n- {RUN_FOR_ME}\n")), "{kind:?} {mode:?} {which}: {sec}");
                    assert_eq!(full.matches("run_for_me").count(), 2, "{kind:?} {mode:?} {which}: once, with its example: {full}");
                    assert_eq!(sec.matches(PUSHED_BY_GIZAI).count(), 1, "{kind:?} {mode:?} {which}");
                    assert!(bullets(sec) <= 12, "{kind:?} {mode:?} {which}: at most about 12 lines ({}): {sec}", bullets(sec));
                }
            }
        }
    }
    // it names neither "commands" (another CLI's section names none) nor sleep (named only when the agent may run it)
    assert!(!RUN_FOR_ME.contains("commands") && !RUN_FOR_ME.contains("sleep"), "{RUN_FOR_ME}");
}
