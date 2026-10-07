use gizai_core::{db::Db, model::*, projects, pulls, seed::ensure_seed, tasks, team};
use serde_json::json;

const PR: &str = "https://github.com/acme/shop/pull/7";

struct Shop { db: Db, you: String, project: String, states: Vec<(String, String)> }

/// Project SHOP linked to https://github.com/acme/shop, with its local repository.
fn shop() -> Shop {
    let db = Db::open_in_memory().unwrap();
    let you = ensure_seed(&db, "Jeffrey").unwrap().you_id;
    let project = projects::create(&db, &you, ProjectInput { name: "Shop".into(), key: "SHOP".into(), repo_path: Some("/home/j/Code/shop".into()),
        repo_url: Some("git@github.com:acme/shop.git".into()), default_branch: Some("trunk".into()), ..Default::default() }).unwrap();
    let t = team::get(&db, &team::list(&db).unwrap()[0].id).unwrap();
    let states = t.states.iter().map(|s| (s.category.clone(), s.id.clone())).collect();
    Shop { db, you, project, states }
}

impl Shop {
    fn state(&self, category: &str) -> String { self.states.iter().find(|(c, _)| c == category).unwrap().1.clone() }
    /// A card in the column of `category`, with `branch` (an agent's run gives a card its branch).
    fn card(&self, project: &str, category: &str, branch: Option<&str>) -> String {
        let id = tasks::create(&self.db, &self.you, TaskInput { project_id: project.into(), title: "Export invoices".into(),
            state_id: Some(self.state(category)), ..Default::default() }).unwrap();
        if let Some(b) = branch {
            self.db.read(|c| Ok(c.execute("UPDATE tasks SET branch=?2 WHERE id=?1", [&id, b])?)).unwrap();
        }
        id
    }
}

#[test]
fn a_card_has_a_pull_request_only_with_a_github_link_and_a_branch() {
    let s = shop();
    let id = s.card(&s.project, "review", Some("gizai/shop-1-export"));
    let c = pulls::card(&s.db, &id).unwrap();
    assert_eq!((c.repo.as_str(), c.repo_url.as_str(), c.default_branch.as_str()), ("acme/shop", "https://github.com/acme/shop", "trunk"));
    assert_eq!((c.branch.as_str(), c.repo_path.as_str(), c.category.as_str()), ("gizai/shop-1-export", "/home/j/Code/shop", "review"));
    assert_eq!((c.pr_url, c.pr_state), (None, None));
    // no branch yet
    let e = pulls::card(&s.db, &s.card(&s.project, "review", None)).unwrap_err().to_string();
    assert!(e.contains("has no branch yet"), "{e}");
    // a project without a GitHub link, or with another host
    let plain = projects::create(&s.db, &s.you, ProjectInput { name: "Plain".into(), key: "PLN".into(), repo_path: Some("/home/j/Code/plain".into()), ..Default::default() }).unwrap();
    let e = pulls::card(&s.db, &s.card(&plain, "review", Some("gizai/pln-1-x"))).unwrap_err().to_string();
    assert!(e.contains("Link Plain to its GitHub repository first"), "{e}");
    let lab = projects::create(&s.db, &s.you, ProjectInput { name: "Lab".into(), key: "LAB".into(), repo_path: Some("/home/j/Code/lab".into()),
        repo_url: Some("https://gitlab.com/acme/lab".into()), ..Default::default() }).unwrap();
    let e = pulls::card(&s.db, &s.card(&lab, "review", Some("gizai/lab-1-x"))).unwrap_err().to_string();
    assert!(e.contains("Link Lab to its GitHub repository first"), "{e}");
}

#[test]
fn the_pr_check_follows_review_cards_and_pull_requests_that_are_not_merged() {
    let s = shop();
    let review = s.card(&s.project, "review", Some("gizai/shop-1"));
    let ready = s.card(&s.project, "ready", Some("gizai/shop-2"));
    let ready_open = s.card(&s.project, "ready", Some("gizai/shop-3"));
    pulls::record(&s.db, None, &ready_open, PR, "open", false).unwrap();
    let ready_merged = s.card(&s.project, "in_progress", Some("gizai/shop-4"));
    pulls::record(&s.db, None, &ready_merged, PR, "merged", false).unwrap();
    let done_open = s.card(&s.project, "done", Some("gizai/shop-5"));
    pulls::record(&s.db, None, &done_open, PR, "open", false).unwrap();
    let _review_without_branch = s.card(&s.project, "review", None);
    let plain = projects::create(&s.db, &s.you, ProjectInput { name: "Plain".into(), key: "PLN".into(), repo_path: Some("/r".into()), ..Default::default() }).unwrap();
    let _review_without_link = s.card(&plain, "review", Some("gizai/pln-1"));
    let followed: Vec<String> = pulls::to_check(&s.db).unwrap().into_iter().map(|c| c.task_id).collect();
    assert_eq!(followed, [review.clone(), ready_open], "Review, and an open card with an unmerged pull request");
    assert!(!pulls::card(&s.db, &ready).unwrap().followed());
    // a closed pull request is still followed (it can be reopened), a merged one isn't
    pulls::record(&s.db, None, &review, PR, "closed", false).unwrap();
    assert!(pulls::card(&s.db, &review).unwrap().followed());
}

#[test]
fn recording_a_pull_request_notes_only_real_changes() {
    let s = shop();
    let id = s.card(&s.project, "review", Some("gizai/shop-1"));
    let before = tasks::activity(&s.db, &id).unwrap().len();
    assert!(pulls::record(&s.db, Some(&s.you), &id, PR, "open", true).unwrap());
    let t = tasks::get(&s.db, &id).unwrap();
    assert_eq!((t.pr_url.as_deref(), t.pr_state.as_deref()), (Some(PR), Some("open")));
    let a = tasks::activity(&s.db, &id).unwrap();
    assert_eq!(a.len(), before + 1);
    let last = a.last().unwrap();
    assert_eq!((last.actor_name.as_deref(), &last.diff), (Some("Jeffrey"), &json!({"pullRequest": PR, "prState": "open", "opened": true})));
    // the same again: nothing changes, nothing is noted
    assert!(!pulls::record(&s.db, None, &id, PR, "open", false).unwrap());
    assert_eq!(tasks::activity(&s.db, &id).unwrap().len(), before + 1);
    // a new state from the PR check is Gizai's, not yours
    assert!(pulls::record(&s.db, None, &id, PR, "draft", false).unwrap());
    let last = tasks::activity(&s.db, &id).unwrap().pop().unwrap();
    assert_eq!((last.actor_name, last.diff), (None, json!({"pullRequest": PR, "prState": "draft"})));
    assert!(pulls::record(&s.db, None, &id, PR, "weird", false).is_err(), "unknown state");
    assert!(pulls::record(&s.db, None, "nope", PR, "open", false).is_err(), "unknown card");
}

#[test]
fn a_merge_moves_the_card_to_done_once() {
    let s = shop();
    let id = s.card(&s.project, "review", Some("gizai/shop-1"));
    pulls::record(&s.db, Some(&s.you), &id, PR, "open", true).unwrap();
    assert_eq!(pulls::merged(&s.db, &s.you, &id, PR).unwrap().as_deref(), Some("Done"));
    let t = tasks::get(&s.db, &id).unwrap();
    assert_eq!((t.state_name.as_str(), t.state_category.as_str(), t.pr_state.as_deref()), ("Done", "done", Some("merged")));
    let a = tasks::activity(&s.db, &id).unwrap();
    assert!(a.iter().any(|e| e.diff == json!({"column": ["Review", "Done"]})), "{a:?}");
    assert!(a.iter().any(|e| e.actor_name.is_none() && e.diff == json!({"pullRequest": PR, "prState": "merged"})), "Gizai saw the merge: {a:?}");
    assert!(!pulls::card(&s.db, &id).unwrap().followed() && pulls::to_check(&s.db).unwrap().is_empty(), "not followed any more");
    // a card that is already Done stays where it is
    assert_eq!(pulls::merged(&s.db, &s.you, &id, PR).unwrap(), None);
    assert_eq!(tasks::get(&s.db, &id).unwrap().state_name, "Done");
    // the clean-up is noted as Gizai's
    pulls::note_cleanup(&s.db, &id, "removed its worktree after the merge").unwrap();
    let last = tasks::activity(&s.db, &id).unwrap().pop().unwrap();
    assert_eq!((last.actor_name, last.diff), (None, json!({"cleanup": "removed its worktree after the merge"})));
}
