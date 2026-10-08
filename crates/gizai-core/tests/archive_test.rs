//! GA-43: archiving a card in Done (a soft delete) and restoring it. Only a card in Done is archived, never one an agent
//! works on; it leaves every list, keeps its column, comments, runs, branch and identifier, and is read-only until
//! Restore puts it back at the bottom of Done. The bin lists archived cards, the most recently archived first.
use gizai_core::{Error, clients, comments, db::Db, files, model::*, projects, runs, seed::ensure_seed, tasks, team, workflow, worktrees};

struct B { db: Db, you: String, project: String }

fn board() -> B {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let client = clients::create(&db, &s.you_id, ClientInput { name: "Kade Logistics".into(), ..Default::default() }).unwrap();
    let project = projects::create(&db, &s.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), client_id: Some(client),
        repo_path: Some("/home/j/Code/kade".into()), ..Default::default() }).unwrap();
    B { db, you: s.you_id, project }
}

impl B {
    fn state(&self, name: &str) -> String {
        self.db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name=?1", [name], |r| r.get(0))?)).unwrap()
    }
    fn card_in(&self, project: &str, column: &str, title: &str) -> String {
        tasks::create(&self.db, &self.you, TaskInput { project_id: project.into(), title: title.into(), state_id: Some(self.state(column)),
            ..Default::default() }).unwrap()
    }
    fn card(&self, column: &str) -> String { self.card_in(&self.project, column, "Export invoices") }
    fn task(&self, id: &str) -> Task { tasks::get(&self.db, id).unwrap() }
    fn listed(&self, open_only: bool) -> Vec<String> {
        tasks::list(&self.db, &TaskFilter { open_only, ..Default::default() }).unwrap().into_iter().map(|t| t.id).collect()
    }
    fn deleted_at(&self, id: &str) -> Option<i64> {
        self.db.read(|c| Ok(c.query_row("SELECT deleted_at FROM tasks WHERE id=?1", [id], |r| r.get(0))?)).unwrap()
    }
    fn invalid(&self, r: gizai_core::Result<()>, contains: &str) {
        assert!(matches!(&r, Err(Error::Invalid(m)) if m.contains(contains)), "expected a refusal with {contains:?}, got {r:?}");
    }
}

#[test]
fn only_a_card_in_done_can_be_archived_and_the_others_are_refused_plainly() {
    let b = board();
    let team_id = team::list(&b.db).unwrap()[0].id.clone();
    // the extra columns a team can add: Deploy (GA-32) and Cancelled; neither is Done
    team::add_state(&b.db, &b.you, &team_id, "Deploy", &b.state("Review"), "deploy", None).unwrap();
    team::add_state(&b.db, &b.you, &team_id, "Cancelled", &b.state("Done"), "cancelled", None).unwrap();
    for column in ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Cancelled"] {
        let t = b.card(column);
        let id = b.task(&t).identifier;
        b.invalid(tasks::archive(&b.db, &b.you, &t), &format!("Only a card in Done can be archived, and {id} is in {column}"));
        assert_eq!(b.deleted_at(&t), None, "{column}: nothing changed");
        assert!(b.listed(false).contains(&t), "{column}: still on the board");
    }
    let done = b.card("Done");
    tasks::archive(&b.db, &b.you, &done).unwrap();
    assert!(b.deleted_at(&done).is_some());
    assert!(matches!(tasks::archive(&b.db, &b.you, "no-such-card"), Err(Error::NotFound(_))));
}

#[test]
fn a_card_with_a_live_run_is_refused_until_the_run_ends() {
    let b = board();
    let be = team::add_agent(&b.db, &b.you, &team::list(&b.db).unwrap()[0].id,
        AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let t = b.card("Done");
    let run = runs::create(&b.db, &be, &t, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
    b.invalid(tasks::archive(&b.db, &b.you, &t), "An agent is working on this card");
    runs::set_running(&b.db, &run, 4242).unwrap();
    b.invalid(tasks::archive(&b.db, &b.you, &t), "An agent is working on this card");
    assert_eq!(b.deleted_at(&t), None);
    runs::finish(&b.db, &run, "succeeded", None, 0, 0, 0, None).unwrap();
    tasks::archive(&b.db, &b.you, &t).unwrap();
    assert!(b.deleted_at(&t).is_some());
}

#[test]
fn archiving_sets_deleted_at_says_who_and_keeps_column_comments_runs_branch_and_identifier() {
    let b = board();
    let be = team::add_agent(&b.db, &b.you, &team::list(&b.db).unwrap()[0].id,
        AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let t = b.card("Done");
    comments::add(&b.db, &b.you, &t, "Shipped", None).unwrap();
    let run = runs::create(&b.db, &be, &t, "backend", "S", "/tmp", "/tmp", "gizai/kade-1-export", "/tmp/r.jsonl").unwrap();
    runs::finish(&b.db, &run, "succeeded", None, 0, 0, 0, None).unwrap();
    let before = b.task(&t);
    assert_eq!((before.archived_at, before.archived_by.as_deref()), (None, None), "a card on the board isn't archived");

    tasks::archive(&b.db, &b.you, &t).unwrap();
    let after = b.task(&t); // get still finds it
    assert_eq!(after.archived_at, b.deleted_at(&t), "archived_at is deleted_at");
    assert!(after.archived_at.is_some());
    assert_eq!(after.archived_by.as_deref(), Some("Jeffrey"));
    assert_eq!((after.identifier.as_str(), after.state_name.as_str(), after.branch.as_deref()), ("KADE-1", "Done", Some("gizai/kade-1-export")));
    assert_eq!(after.sort_key, before.sort_key, "its place stays until it's restored");
    assert_eq!(comments::list(&b.db, &t).unwrap().len(), 1);
    assert_eq!(runs::list_for_task(&b.db, &t).unwrap().len(), 1);
    let last = tasks::activity(&b.db, &t).unwrap().pop().unwrap();
    assert_eq!((last.table.as_str(), last.op.as_str(), last.actor_name.as_deref()), ("tasks", "delete", Some("Jeffrey")),
               "the activity records who archived it");
    b.invalid(tasks::archive(&b.db, &b.you, &t), "KADE-1 is already archived");
}

#[test]
fn an_archived_card_is_left_out_of_the_list_the_inbox_the_counts_and_the_queue() {
    let b = board();
    let team_id = team::list(&b.db).unwrap()[0].id.clone();
    let be = team::add_agent(&b.db, &b.you, &team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let kept = b.card("Done");
    let t = b.card("Done");
    // on hold and assigned to the agent: it would show in the Inbox and in the agent's queue if it were open
    tasks::update(&b.db, &b.you, &t, TaskPatch { hold: Some("blocked".into()), assignee_id: Some(be.clone()), ..Default::default() }).unwrap();
    let project_before = projects::get(&b.db, &b.project).unwrap();
    assert_eq!(project_before.done_tasks, 2);

    tasks::archive(&b.db, &b.you, &t).unwrap();
    assert_eq!(b.listed(false), [kept.clone()], "the board and the list (with Done)");
    assert!(!b.listed(true).contains(&t));
    assert!(tasks::list(&b.db, &TaskFilter { project_id: Some(b.project.clone()), open_only: false }).unwrap().iter().all(|x| x.id != t));
    assert!(tasks::needs_you(&b.db, &b.you).unwrap().iter().all(|x| x.id != t), "the Inbox");
    assert_eq!(projects::get(&b.db, &b.project).unwrap().done_tasks, 1, "the project's count");
    assert_eq!(projects::list(&b.db).unwrap()[0].done_tasks, 1);
    assert!(workflow::waiting_for(&b.db, &be).unwrap().is_empty(), "the queue");
    assert_eq!(workflow::next_task_for(&b.db, &be).unwrap(), None, "the heartbeat");
    assert!(!matches!(workflow::pick_agent(&b.db, &t), Ok(Some(_))), "routing never picks an agent for it");
}

#[test]
fn the_bin_lists_archived_cards_most_recently_archived_first_for_one_project_or_all() {
    let b = board();
    let blog = projects::create(&b.db, &b.you, ProjectInput { name: "Blog".into(), key: "BLOG".into(), ..Default::default() }).unwrap();
    let first = b.card_in(&b.project, "Done", "Old export");
    let second = b.card_in(&blog, "Done", "New post");
    let third = b.card_in(&b.project, "Done", "Invoices");
    let _on_board = b.card("Done");
    assert!(tasks::archived(&b.db, None).unwrap().is_empty(), "nothing archived yet");
    for t in [&first, &second, &third] {
        tasks::archive(&b.db, &b.you, t).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    let all: Vec<String> = tasks::archived(&b.db, None).unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(all, [third.clone(), second.clone(), first.clone()], "the most recently archived first");
    let kade = tasks::archived(&b.db, Some(&b.project)).unwrap();
    assert_eq!(kade.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), [third.as_str(), first.as_str()], "the project filter");
    let row = &kade[0];
    assert_eq!((row.identifier.as_str(), row.title.as_str(), row.project_name.as_deref(), row.archived_by.as_deref()),
               ("KADE-2", "Invoices", Some("Kade portal"), Some("Jeffrey")));
    assert!(row.archived_at.is_some());
    tasks::restore(&b.db, &b.you, &third).unwrap();
    assert_eq!(tasks::archived(&b.db, Some(&b.project)).unwrap().len(), 1, "a restored card leaves the bin");
}

#[test]
fn restore_puts_the_card_back_at_the_bottom_of_done_and_says_who() {
    let b = board();
    let t = b.card("Done");
    let _above = b.card("Done");
    tasks::archive(&b.db, &b.you, &t).unwrap();
    let later = b.card("Done"); // added to Done while it was archived
    tasks::restore(&b.db, &b.you, &t).unwrap();

    let back = b.task(&t);
    assert_eq!((back.archived_at, back.archived_by.as_deref(), back.state_name.as_str()), (None, None, "Done"));
    assert_eq!(b.deleted_at(&t), None);
    let done: Vec<String> = tasks::list(&b.db, &TaskFilter::default()).unwrap().into_iter()
        .filter(|x| x.state_name == "Done").map(|x| x.id).collect();
    assert_eq!(done.last(), Some(&t), "at the bottom of Done: {done:?}");
    assert!(back.sort_key > b.task(&later).sort_key);
    let last = tasks::activity(&b.db, &t).unwrap().pop().unwrap();
    assert_eq!((last.op.as_str(), last.actor_name.as_deref(), &last.diff), ("update", Some("Jeffrey"), &serde_json::json!({"archived": false})),
               "the activity records who restored it");
    b.invalid(tasks::restore(&b.db, &b.you, &t), "KADE-1 isn't archived");
    // and it can be archived again, and edited again
    tasks::update(&b.db, &b.you, &t, TaskPatch { title: Some("Export invoices as CSV".into()), ..Default::default() }).unwrap();
    tasks::archive(&b.db, &b.you, &t).unwrap();
    assert_eq!(b.task(&t).archived_by.as_deref(), Some("Jeffrey"));
}

#[test]
fn an_archived_card_is_read_only_until_it_is_restored() {
    let b = board();
    let be = team::add_agent(&b.db, &b.you, &team::list(&b.db).unwrap()[0].id,
        AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    let t = b.card("Done");
    tasks::archive(&b.db, &b.you, &t).unwrap();
    let ro = "KADE-1 is archived: restore it first";
    b.invalid(tasks::update(&b.db, &b.you, &t, TaskPatch { title: Some("New title".into()), ..Default::default() }), ro);
    b.invalid(tasks::move_to(&b.db, &b.you, &t, &b.state("To do"), ""), ro);
    b.invalid(tasks::set_labels(&b.db, &b.you, &t, vec![]), ro);
    let comment = comments::add(&b.db, &b.you, &t, "One more thing", None);
    assert!(matches!(&comment, Err(Error::Invalid(m)) if m.contains(ro)), "{comment:?}");
    let run = runs::create(&b.db, &be, &t, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl");
    assert!(matches!(&run, Err(Error::Invalid(m)) if m.contains(ro)), "{run:?}");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    std::fs::write(&file, "hello").unwrap();
    let added = files::add_from_path(&b.db, &b.you, dir.path(), "task", &t, &file);
    assert!(matches!(&added, Err(Error::Invalid(m)) if m.contains(ro)), "{added:?}");

    let t2 = b.task(&t);
    assert_eq!((t2.title.as_str(), t2.state_name.as_str()), ("Export invoices", "Done"), "nothing changed");
    assert!(comments::list(&b.db, &t).unwrap().is_empty());
    assert!(runs::list_for_task(&b.db, &t).unwrap().is_empty());
    tasks::restore(&b.db, &b.you, &t).unwrap();
    tasks::move_to(&b.db, &b.you, &t, &b.state("To do"), "").unwrap();
    comments::add(&b.db, &b.you, &t, "One more thing", None).unwrap();
    assert_eq!(b.task(&t).state_name, "To do", "restored, it moves again");
}

#[test]
fn an_archived_cards_identifier_is_never_reused() {
    let b = board();
    let one = b.card("Done");
    let two = b.card("Done");
    tasks::archive(&b.db, &b.you, &two).unwrap();
    tasks::archive(&b.db, &b.you, &one).unwrap();
    let next = b.card("To do");
    assert_eq!(b.task(&next).identifier, "KADE-3");
    assert_eq!((b.task(&one).identifier.as_str(), b.task(&two).identifier.as_str()), ("KADE-1", "KADE-2"));
}

#[test]
fn the_worktree_clean_up_still_finds_an_archived_cards_worktree() {
    let b = board();
    let t = b.card("Done");
    b.db.write(None, |w| { w.conn().execute("UPDATE tasks SET branch='gizai/kade-1-export' WHERE id=?1", [&t])?; Ok(()) }).unwrap();
    tasks::archive(&b.db, &b.you, &t).unwrap();
    let found: Vec<String> = worktrees::finished(&b.db, None).unwrap().into_iter().map(|c| c.task_id).collect();
    assert_eq!(found, [t.clone()]);
    assert_eq!(worktrees::finished(&b.db, Some(&b.project)).unwrap()[0].branch, "gizai/kade-1-export");
}

#[test]
fn the_clients_open_count_never_counts_an_archived_card() {
    let b = board();
    let open = b.card("To do");
    let t = b.card("Done");
    tasks::archive(&b.db, &b.you, &t).unwrap();
    assert_eq!(clients::list(&b.db).unwrap()[0].open_tasks, 1);
    assert_eq!(projects::get(&b.db, &b.project).unwrap().open_tasks, 1);
    assert!(b.listed(true) == [open]);
}
