use gizai_core::seed::{role_template, role_tools};

const RESULT_LINE: &str = r#"GIZAI_RESULT: {"outcome":"<outcome>","summary":"<one paragraph for the task comment>","issues":[]}"#;

/// Everything after a text's first line.
fn rest(t: &str) -> &str {
    t.split_once('\n').unwrap().1
}

#[test]
fn builders_get_our_own_instructions_they_push_and_leave_tests_and_pull_requests_to_qa() {
    // GA-63: the Backend and Frontend Agents' texts, word for word (crates/gizai-core/roles).
    let be = role_template("backend");
    assert!(be.starts_with("You are the Backend Agent in Gizai's Software team. You build the server side of a card (APIs, database, jobs, services). \
                            You do not test and you do not open pull requests: the QA Agent does both.\n"), "{be}");
    let fe = role_template("frontend");
    assert!(fe.starts_with("You are the Frontend Agent in Gizai's Software team. You build the user-facing part of a card (pages, components, styling, \
                            client-side behaviour). You do not test and you do not open pull requests: the QA Agent does both.\n"), "{fe}");
    for t in [&be, &fe] {
        assert!(t.contains("push the branch with `git push -u origin HEAD` (for a project without a remote, committing is enough)"), "{t}");
        assert!(t.contains("never merge, except the main branch into your branch when 'Your branch' below asks for it"), "{t}");
        assert!(t.contains("(README, CLAUDE.md, AGENTS.md, an agent guide)"), "{t}");
        assert!(t.contains("Do not run tests") && t.contains("Do not open a pull request."), "{t}");
        assert!(t.contains("A run has limits: Gizai names them at the end of this prompt."), "{t}");
        assert!(t.contains("- ready_for_testing: ") && t.contains("- needs_decision: "), "{t}");
        // The old stubs told builders to write tests and never push.
        assert!(!t.contains("add or update tests") && !t.contains("Never push"), "{t}");
    }
    assert!(be.contains("cargo test, go test, node --test") && fe.contains("vitest, jest, playwright, cypress"));
}

#[test]
fn qa_runs_the_tests_and_opens_the_pull_request_on_github_only() {
    let qa = role_template("qa");
    assert!(qa.starts_with("You are the QA Agent in Gizai's Software team. You check the work of the developer agents (Backend, Frontend and the like). \
                            You are the only agent that runs tests and the only one that opens pull requests.\n"), "{qa}");
    assert!(qa.contains("Do not change application code. You may add or fix tests only"), "{qa}");
    assert!(qa.contains("When the project's remote is on GitHub (`git remote -v`), open a pull request with `gh pr create`"), "{qa}");
    assert!(qa.contains("On another host, or without a remote, don't open one: say so in your summary, and the user opens it from Review."), "{qa}");
    assert!(qa.contains("run `git push origin HEAD` (for a project without a remote, committing is enough)"), "{qa}");
    assert!(qa.contains("never merge, except the main branch into this branch when 'Your branch' below asks for it"), "{qa}");
    assert!(qa.contains("\nOutcomes: qa_pass, qa_fail, needs_decision."), "{qa}");
}

#[test]
fn the_lead_keeps_its_template() {
    let lead = role_template("lead");
    assert!(lead.starts_with("You are the Team Lead"));
    assert!(lead.contains("Chat page"));
    assert!(lead.contains("Don't write code yourself"));
    assert!(!lead.contains("routing"), "labels don't route any more (GA-49): {lead}");
    assert_eq!(lead.trim_end().lines().last().unwrap(), RESULT_LINE);
}

#[test]
fn devops_releases_and_deploys_by_the_projects_own_flow_without_our_projects() {
    let o = role_template("devops");
    assert!(o.starts_with("You are the DevOps Agent in Gizai's Software team. You make releases, you deploy them, you fix pull requests that can't be merged"), "{o}");
    assert!(o.contains("\nOutcomes: deployed, ready_for_testing, needs_decision."), "{o}");
    assert!(o.contains("The card goes to Review for the user, never to QA."), "{o}");
    // It waits the way "How this run works" says: a check first, then a short sleep.
    assert!(o.contains("`gh run view <id> --json status,conclusion; sleep 45`"), "{o}");
    assert!(o.contains("Deploy mode: manual (the team deploys it by hand)"), "{o}");
    for gone in ["Known projects", "doctl", "insteadOf", "latest", "45 minutes", "80 tool turns"] {
        assert!(!o.contains(gone), "{gone}: {o}");
    }
}

#[test]
fn devops_keeps_a_projects_deploy_note_in_gizais_memory_not_in_claude_codes() {
    // GA-85: the same content and format, in Deployments/<KEY>, reported as learned lines for the Team Lead to fold in.
    let o = role_template("devops");
    let section = &o[o.find("## Memory: how each project is deployed").expect("its section")..o.find("## Outcomes").unwrap()];
    for want in ["Each project has one deploy note in Gizai's Memory, Deployments/<KEY> (Deployments/KADE for project KADE)",
                 "`type: deployment`, `project: <KEY>` and `applies_to: devops`", "comes into your run in the Memory section",
                 "At the start of a run, read this project's deploy note there.", "the repo's own docs and workflows win",
                 "Report what changed as `learned` lines on your result line", "The Team Lead folds them into the deploy note.",
                 "No deploy note yet: also put the whole note, in the format above, in your summary",
                 // the format stays
                 "Deploy mode: manual (the team deploys it by hand) or agent", "Gotchas: what is easy to get wrong",
                 "Last checked: date, commit, and the files you read", "Never deploy a manual project"] {
        assert!(section.contains(want), "{want} missing in {section}");
    }
    for gone in ["MEMORY.md", "memory directory", "deploy-<KEY>", "your memory", "If you cannot save memory"] {
        assert!(!o.contains(gone), "{gone}: {o}");
    }
    assert!(o.contains("Find out how a project is deployed, for its deploy note in Gizai's Memory"), "{o}");
    assert!(o.contains("'Memory:' with what you reported for the deploy note"), "{o}");
    assert!(o.contains("Never put secret values in a learned line or a summary"), "{o}");
}

#[test]
fn design_is_the_frontend_text_with_its_own_name_and_job() {
    let d = role_template("design");
    let (first, _) = d.split_once('\n').unwrap();
    assert_eq!(first, "You are the Design Agent in Gizai's Software team. You design the screens and flows a card asks for and build them as UI \
                       components and styles, following the project's design system; explain your design decisions in the hand-over. You do not \
                       test and you do not open pull requests: the QA Agent does both.");
    assert_eq!(rest(&d), rest(&role_template("frontend")));
}

#[test]
fn any_other_role_is_the_backend_text_with_its_own_name() {
    for (role, name) in [("docs", "Docs"), ("data", "Data"), ("mobile-app", "Mobile-app")] {
        let t = role_template(role);
        let (first, _) = t.split_once('\n').unwrap();
        assert_eq!(first, format!("You are the {name} Agent in Gizai's Software team. You build what the card asks for. You do not test and you do not \
                                   open pull requests: the QA Agent does both."));
        assert_eq!(rest(&t), rest(&role_template("backend")), "{role}");
    }
}

#[test]
fn every_template_asks_for_the_result_line() {
    for role in ["frontend", "backend", "qa", "lead", "design", "devops", "docs"] {
        let t = role_template(role);
        let lines: Vec<&str> = t.lines().collect();
        let at = lines.iter().position(|l| *l == RESULT_LINE).unwrap_or_else(|| panic!("{role}: no result line in {t}"));
        assert_eq!(lines[at - 1], "When you finish, end your final message with exactly one line:", "{role}");
        assert!(lines.len() - at <= 2, "{role}: the result line is at the end: {t}");
    }
}

#[test]
fn no_default_text_or_command_list_names_our_company_or_its_repositories() {
    for role in ["lead", "backend", "frontend", "design", "qa", "devops", "docs"] {
        let all = format!("{}\n{}", role_template(role), role_tools(role).join("\n")).to_lowercase();
        for ours in ["yotech", "otus", "oranje", "jeffrey", "gizai.git", "github.com/"] {
            assert!(!all.contains(ours), "{role} mentions {ours}");
        }
    }
}

#[test]
fn every_working_role_hears_that_gizai_pushes_its_branch_after_the_run_and_keeps_git_push() {
    // GA-56: Gizai pushes the card's branch's commits when a run ends, so a refused push is no reason for needs_decision.
    for role in ["backend", "frontend", "design", "docs"] {
        let t = role_template(role);
        assert!(t.contains("Gizai also pushes the branch's commits when your run ends, so a refused push is no reason for needs_decision: \
                            mention it in your hand-over and end with the outcome the work deserves."), "{role}: {t}");
        assert!(t.contains("whether your push went through (Gizai pushes the branch after the run either way)"), "{role}: {t}");
        assert!(t.contains("- ready_for_testing: the work is committed and the hand-over is written."), "{role}: {t}");
        assert!(!t.contains("committed (and pushed)"), "{role}: {t}");
    }
    let qa = role_template("qa");
    assert!(qa.contains("If the push is refused, go on: Gizai pushes this branch's commits when your run ends, before the card moves on."), "{qa}");
    assert!(qa.contains("A refused `git push` is no reason for needs_decision: Gizai pushes the branch when your run ends, so mention it in \
                         your summary and end with the outcome the work deserves."), "{qa}");
    assert!(qa.contains("If your push was refused, also give it `--head <branch>`"), "{qa}");
    assert!(!qa.contains("when pushing or opening the pull request fails"), "a failed push is no longer a reason to stop: {qa}");
    let devops = role_template("devops");
    assert!(devops.contains("The one exception is a refused push of the card's own branch: Gizai pushes that branch's commits when your run ends"), "{devops}");
    assert!(devops.contains("Gizai pushes nothing else: not a pull request's branch, not a tag."), "{devops}");
    assert!(devops.contains("if only that push is refused, go on, since Gizai pushes it after the run"), "{devops}");
    // they all keep git push in their allowed commands
    for role in ["backend", "frontend", "design", "docs", "qa", "devops"] {
        assert!(role_tools(role).iter().any(|t| t == "Bash(git push:*)"), "{role}: {:?}", role_tools(role));
    }
}

#[test]
fn every_working_role_hears_how_to_ask_the_user_to_run_a_command_next_to_the_push_line() {
    // GA-31, Run this for me: the commands go in run_for_me on a needs_decision result line, and Done, continue resumes the run.
    for role in ["backend", "frontend", "design", "docs", "qa", "devops"] {
        let t = role_template(role);
        for want in ["Run this for me: ", "sudo", "needs_decision", "\"run_for_me\":[\"sudo pacman -S libayatana-appindicator\"]",
                     "The user runs them and presses Done, continue, which continues this run: check that they worked"] {
            assert!(t.contains(want), "{role}: {want} missing in {t}");
        }
        assert_eq!(t.matches("run_for_me").count(), 1, "{role}: one example");
        // its example, added to the result line the template asks for, is a valid result line
        let start = t.find("\"run_for_me\":[").unwrap();
        let example = &t[start..start + t[start..].find(']').unwrap() + 1];
        let line: serde_json::Value = serde_json::from_str(&RESULT_LINE["GIZAI_RESULT: ".len()..]
            .replace("\"issues\":[]", &format!("\"issues\":[],{example}"))).unwrap_or_else(|e| panic!("{role}: {e}: {example}"));
        assert_eq!(line["run_for_me"], serde_json::json!(["sudo pacman -S libayatana-appindicator"]), "{role}");
        // the push line is kept, before it
        let push = ["Gizai pushes", "Gizai also pushes"].iter().filter_map(|w| t.find(w)).min().unwrap_or_else(|| panic!("{role}: no push line: {t}"));
        assert!(push < t.find("Run this for me").unwrap(), "{role}: {t}");
    }
    // the Team Lead runs no card, so it isn't told
    assert!(!role_template("lead").contains("run_for_me"));
}
