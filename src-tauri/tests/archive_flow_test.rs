//! GA-43 in the app: the Team Lead's get_task finds an archived card by its identifier and says so, list_tasks leaves
//! archived cards out unless archived: true asks for them, its other tools can't change one, and no run starts on an
//! archived card. A card an agent works on can't be archived.
use std::path::{Path, PathBuf};

use gizai_core::model::*;
use gizai_core::{projects, tasks, team};
use gizai_lib::{AppState, runs, tools};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

struct T { st: AppState, lead: String, _dir: tempfile::TempDir }

fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    T { st, lead, _dir: dir }
}

impl T {
    async fn call(&self, name: &str, args: Value) -> Result<Value, String> { tools::call(&self.st, &self.lead, name, args).await }
    async fn ok(&self, name: &str, args: Value) -> Value { self.call(name, args).await.unwrap_or_else(|e| panic!("{name} failed: {e}")) }
    fn id(&self, identifier: &str) -> String {
        tasks::list(&self.st.db, &TaskFilter::default()).unwrap().into_iter().chain(tasks::archived(&self.st.db, None).unwrap())
            .find(|t| t.identifier == identifier).unwrap_or_else(|| panic!("no {identifier}")).id
    }
    fn archive(&self, identifier: &str) { tasks::archive(&self.st.db, &self.st.you_id, &self.id(identifier)).unwrap(); }
    fn you(&self) -> String {
        self.st.db.read(|c| Ok(c.query_row("SELECT name FROM actors WHERE id=?1", [&self.st.you_id], |r| r.get(0))?)).unwrap()
    }
    fn identifiers(v: &Value) -> Vec<String> {
        v["tasks"].as_array().unwrap().iter().map(|t| t["task"].as_str().unwrap().to_string()).collect()
    }
}

/// Kade portal: KADE-1 Export invoices (Done), KADE-2 Old import (Done), KADE-3 Import drivers (To do); Blog: BLOG-1
/// New post (Done).
async fn cards(t: &T) {
    for (name, key) in [("Kade portal", "KADE"), ("Blog", "BLOG")] {
        projects::create(&t.st.db, &t.st.you_id, ProjectInput { name: name.into(), key: key.into(), ..Default::default() }).unwrap();
    }
    for (project, title, column) in [("KADE", "Export invoices", "Done"), ("KADE", "Old import", "Done"), ("KADE", "Import drivers", "To do"),
                                     ("BLOG", "New post", "Done")] {
        t.ok("create_task", json!({"project": project, "title": title, "column": column})).await;
    }
}

#[tokio::test]
async fn get_task_finds_an_archived_card_by_its_identifier_and_says_it_is_archived() {
    let t = setup();
    cards(&t).await;
    let before = t.ok("get_task", json!({"task": "KADE-1"})).await;
    assert_eq!(before["task"]["archived"], false);
    t.archive("KADE-1");
    let got = t.ok("get_task", json!({"task": "kade-1"})).await;
    assert_eq!(got["task"]["title"], "Export invoices");
    assert_eq!(got["task"]["column"], "Done");
    assert_eq!(got["task"]["archived"], true);
    assert_eq!(got["task"]["archived_by"], json!(t.you()));
    assert!(got["task"]["archived_on"].as_str().is_some_and(|d| d.len() == 10), "a date: {}", got["task"]["archived_on"]);
    // by its id too, but never by its title
    assert_eq!(t.ok("get_task", json!({"task": t.id("KADE-1")})).await["task"]["archived"], true);
    let err = t.call("get_task", json!({"task": "Export invoices"})).await.unwrap_err();
    assert!(!err.is_empty());
}

#[tokio::test]
async fn list_tasks_leaves_archived_cards_out_and_archived_true_lists_only_them_newest_first() {
    let t = setup();
    cards(&t).await;
    t.archive("KADE-1");
    std::thread::sleep(std::time::Duration::from_millis(3));
    t.archive("BLOG-1");
    let open = t.ok("list_tasks", json!({})).await;
    assert_eq!(T::identifiers(&open), ["KADE-3"]);
    let with_done = t.ok("list_tasks", json!({"include_done": true})).await;
    let mut ids = T::identifiers(&with_done);
    ids.sort();
    assert_eq!(ids, ["KADE-2", "KADE-3"], "archived cards are left out even with include_done");

    let bin = t.ok("list_tasks", json!({"archived": true})).await;
    assert_eq!(T::identifiers(&bin), ["BLOG-1", "KADE-1"], "the most recently archived first");
    assert_eq!(bin["tasks"][0]["archived"], true);
    assert_eq!(bin["tasks"][0]["archived_by"], json!(t.you()));
    let kade = t.ok("list_tasks", json!({"archived": true, "project": "KADE"})).await;
    assert_eq!(T::identifiers(&kade), ["KADE-1"]);
    let text = t.ok("list_tasks", json!({"archived": true, "text": "post"})).await;
    assert_eq!(T::identifiers(&text), ["BLOG-1"]);
    assert!(open["tasks"][0].get("archived").is_none(), "a card on the board has no archived fields");

    tasks::restore(&t.st.db, &t.st.you_id, &t.id("KADE-1")).unwrap();
    assert_eq!(T::identifiers(&t.ok("list_tasks", json!({"archived": true})).await), ["BLOG-1"]);
    assert!(T::identifiers(&t.ok("list_tasks", json!({"include_done": true})).await).contains(&"KADE-1".to_string()));
}

#[tokio::test]
async fn the_team_leads_tools_cannot_move_edit_comment_on_or_run_an_archived_card() {
    let t = setup();
    cards(&t).await;
    t.archive("KADE-1");
    let refused = "KADE-1 is archived: restore it first";
    for (tool, args) in [
        ("move_task", json!({"task": "KADE-1", "column": "To do"})),
        ("update_task", json!({"task": "KADE-1", "title": "Export invoices as CSV"})),
        ("update_task", json!({"task": "KADE-1", "labels": ["bug"]})),
        ("comment_on_task", json!({"task": "KADE-1", "body_md": "One more thing"})),
        ("start_agent_run", json!({"task": "KADE-1"})),
    ] {
        let err = t.call(tool, args.clone()).await.unwrap_err();
        assert!(err.contains(refused), "{tool} {args}: {err}");
    }
    let task = tasks::get(&t.st.db, &t.id("KADE-1")).unwrap();
    assert_eq!((task.title.as_str(), task.state_name.as_str(), task.labels.len()), ("Export invoices", "Done", 0));
    assert!(gizai_core::comments::list(&t.st.db, &task.id).unwrap().is_empty());
    assert!(gizai_core::runs::list_for_task(&t.st.db, &task.id).unwrap().is_empty());
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

#[tokio::test]
async fn no_run_starts_on_an_archived_card_and_a_card_with_a_live_run_is_not_archived() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let repo = git_repo(tmp.path());
    let card = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    tasks::update(&st.db, &st.you_id, &card, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let team = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    let done = team.states.iter().find(|s| s.category == "done").unwrap().id.clone();
    tasks::move_to(&st.db, &st.you_id, &card, &done, "").unwrap();
    let be = team.members.iter().find(|m| m.name == "Backend Agent").unwrap().actor_id.clone();

    // a person's Run on a Done card: while it works, the card isn't archived
    assert!(!runs::working_on(&st, &card));
    let (run, finished) = runs::start(&st, &card, Some(be.clone()), Some(FAKE.into()), "manual").await.unwrap();
    assert!(runs::working_on(&st, &card), "the app sees the live run (archive_task refuses it)");
    let r = tasks::archive(&st.db, &st.you_id, &card);
    assert!(matches!(&r, Err(gizai_core::Error::Invalid(m)) if m == "An agent is working on this card"), "{r:?}");
    runs::stop(&st, &run);
    finished.await.unwrap();
    assert!(!runs::working_on(&st, &card));
    assert_eq!(tasks::get(&st.db, &card).unwrap().state_name, "Done");
    tasks::archive(&st.db, &st.you_id, &card).unwrap();

    // archived: Run is refused with a plain message and no run is recorded
    let n = gizai_core::runs::list_for_task(&st.db, &card).unwrap().len();
    let err = runs::start(&st, &card, Some(be), Some(FAKE.into()), "manual").await.unwrap_err();
    let identifier = tasks::get(&st.db, &card).unwrap().identifier;
    assert_eq!(err, format!("{identifier} is archived: restore it first"));
    let err = runs::start(&st, &card, None, Some(FAKE.into()), "manual").await.unwrap_err();
    assert!(err.contains("is archived: restore it first"), "{err}");
    assert_eq!(gizai_core::runs::list_for_task(&st.db, &card).unwrap().len(), n);
    assert!(!runs::working_on(&st, &card), "a refused start leaves nothing starting");
    runs::stop_all(&st, std::time::Duration::from_secs(10)).await;
}

#[tokio::test]
async fn settings_data_still_lists_and_removes_an_archived_cards_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let card = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    runs::run_once(&st, &card, None, Some(FAKE.into())).await.unwrap();
    let identifier = tasks::get(&st.db, &card).unwrap().identifier;
    let wt = st.data_dir.join("worktrees").join("KADE").join(&identifier);
    assert!(wt.is_dir(), "the run made its worktree");
    let team = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    let done = team.states.iter().find(|s| s.category == "done").unwrap().id.clone();
    tasks::move_to(&st.db, &st.you_id, &card, &done, "").unwrap();
    tasks::archive(&st.db, &st.you_id, &card).unwrap();

    let listed: Vec<String> = gizai_lib::worktrees::list(&st).unwrap().into_iter().map(|w| w.identifier).collect();
    assert_eq!(listed, [identifier.clone()], "Settings → Data lists the archived card's worktree");
    let out = gizai_lib::worktrees::remove(&st, &[card.clone()]).unwrap();
    assert!(out[0].removed, "{:?}", out[0]);
    assert!(!wt.exists(), "its folder is gone");
    assert!(gizai_lib::worktrees::list(&st).unwrap().is_empty());
    let last = tasks::activity(&st.db, &card).unwrap().pop().unwrap();
    assert!(last.diff["cleanup"].as_str().is_some_and(|s| s.contains("Settings → Data")), "{last:?}");
    assert!(tasks::get(&st.db, &card).unwrap().archived_at.is_some(), "it stays archived");
}
