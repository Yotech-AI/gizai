use gizai_core::seed::role_template;

#[test]
fn implementer_templates_name_the_role_and_allowed_outcomes() {
    let fe = role_template("frontend");
    assert!(fe.starts_with("You are the Frontend Agent in Gizai's Software team."));
    assert!(fe.contains("Implement the task and add or update tests."));
    assert!(fe.contains("Allowed outcomes: ready_for_testing, needs_decision."));
    assert!(fe.contains("Never push, merge or change branches."));
    assert!(role_template("backend").starts_with("You are the Backend Agent"));
}

#[test]
fn qa_and_lead_get_their_own_rules() {
    let qa = role_template("qa");
    assert!(qa.starts_with("You are the QA Agent"));
    assert!(qa.contains("Do not change application code; you may add or fix tests only."));
    assert!(qa.contains("qa_pass, qa_fail (list numbered issues), needs_decision"));
    let lead = role_template("lead");
    assert!(lead.starts_with("You are the Team Lead"));
    assert!(lead.contains("Chat page"));
    assert!(lead.contains("don't write code yourself"));
}

#[test]
fn design_and_devops_have_templates() {
    let d = role_template("design");
    assert!(d.starts_with("You are the Design Agent"));
    assert!(d.contains("Allowed outcomes: ready_for_testing, needs_decision."));
    let o = role_template("devops");
    assert!(o.starts_with("You are the DevOps Agent"));
    // Started by hand: it deploys a card in Deploy (deployed), and its other jobs go to Review, never to QA.
    assert!(o.contains("Allowed outcomes: deployed, ready_for_testing, needs_decision."), "{o}");
    assert!(o.contains("never to QA"), "{o}");
    assert!(!o.contains("Never deploy"), "{o}");
}

#[test]
fn every_template_ends_with_the_result_line() {
    for role in ["frontend", "backend", "qa", "lead", "design", "devops", "docs"] {
        let t = role_template(role);
        let last = t.trim_end().lines().last().unwrap();
        assert_eq!(last, r#"GIZAI_RESULT: {"outcome":"<outcome>","summary":"<one paragraph for the task comment>","issues":[]}"#, "role {role}");
    }
    assert!(role_template("docs").starts_with("You are the Docs Agent"));
}
