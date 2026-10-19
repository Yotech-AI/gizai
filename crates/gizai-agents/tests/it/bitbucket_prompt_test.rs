//! GA-59: a run of a project on Bitbucket hears that its main branch was just fetched from Bitbucket, not GitHub.
use gizai_agents::prompt::{build, build_from, BaseInfo, TaskContext};

fn ctx(behind: u32) -> TaskContext {
    TaskContext { identifier: "SH-7".into(), title: "Export invoices as CSV".into(), description_md: "Use semicolons.".into(),
                  acceptance_md: "- [ ] opens in Excel NL".into(), recent_comments: vec![("Jeffrey".into(), "Sample file is attached.".into())],
                  role: "backend".into(), qa_issues: vec![], limits: None, base: Some(BaseInfo { from: "acme/master".into(), behind }),
                  project_goal_md: String::new() }
}

#[test]
fn a_bitbucket_project_hears_its_main_branch_was_just_fetched_from_bitbucket() {
    for behind in [0, 3] {
        let p = build_from(&ctx(behind), "You are the Backend Agent.", "Bitbucket");
        assert!(p.contains("The main branch is acme/master, just fetched from Bitbucket."), "{p}");
        assert!(p.contains("just fetched from Bitbucket."), "{p}");
        assert!(!p.contains("GitHub"), "no GitHub in a Bitbucket project's prompt: {p}");
        assert_eq!(p.contains("3 commits your branch doesn't have yet. Merge acme/master into your branch"), behind == 3, "{p}");
    }
}

#[test]
fn the_old_build_still_says_github() {
    let p = build(&ctx(0), "You are the Backend Agent.");
    assert!(p.contains("The main branch is acme/master, just fetched from GitHub."), "{p}");
    assert!(!p.contains("Bitbucket"), "{p}");
    assert_eq!(p, build_from(&ctx(0), "You are the Backend Agent.", "GitHub"), "build is build_from with GitHub");
}

#[test]
fn another_git_url_and_no_base_name_neither_host() {
    let p = build_from(&ctx(0), "R", "the project's repository");
    assert!(p.contains("just fetched from the project's repository."), "{p}");
    assert!(!p.contains("GitHub") && !p.contains("Bitbucket"), "{p}");
    // without a fetched main branch there is no branch section, so no host either
    let p = build_from(&TaskContext { base: None, ..ctx(0) }, "R", "Bitbucket");
    assert!(!p.contains("## Your branch") && !p.contains("Bitbucket") && !p.contains("GitHub"), "{p}");
}
