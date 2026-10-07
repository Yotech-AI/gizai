use gizai_core::{comments, db::Db, model::*, projects, seed::ensure_seed, tasks};

fn project(db: &Db, you: &str, key: &str) -> String {
    projects::create(db, you, ProjectInput { name: "Kade portal".into(), key: key.into(), ..Default::default() }).unwrap()
}

#[test]
fn identifiers_count_up_per_project_and_keys_are_uppercased() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = project(&db, &s.you_id, "kade");
    let t1 = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: "One".into(), ..Default::default() }).unwrap();
    let t2 = tasks::create(&db, &s.you_id, TaskInput { project_id: p.clone(), title: "Two".into(), ..Default::default() }).unwrap();
    assert_eq!(tasks::get(&db, &t1).unwrap().identifier, "KADE-1");
    assert_eq!(tasks::get(&db, &t2).unwrap().identifier, "KADE-2");
    assert_eq!(tasks::get(&db, &t1).unwrap().state_name, "Backlog", "new tasks start in Backlog");
    let (a, b) = (tasks::get(&db, &t1).unwrap().sort_key, tasks::get(&db, &t2).unwrap().sort_key);
    assert!(a < b, "new tasks go to the bottom: {a} < {b}");
}

#[test]
fn project_keys_are_validated_and_unique_and_numbers_count_per_year() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p1 = project(&db, &s.you_id, "KADE");
    assert!(projects::create(&db, &s.you_id, ProjectInput { name: "x".into(), key: "kade".into(), ..Default::default() }).is_err(), "duplicate key");
    assert!(projects::create(&db, &s.you_id, ProjectInput { name: "x".into(), key: "K".into(), ..Default::default() }).is_err(), "too short");
    assert!(projects::create(&db, &s.you_id, ProjectInput { name: "x".into(), key: "KA DE".into(), ..Default::default() }).is_err(), "space");
    let p2 = project(&db, &s.you_id, "GFW");
    let (n1, n2) = (projects::get(&db, &p1).unwrap().number, projects::get(&db, &p2).unwrap().number);
    assert!(n1.ends_with("-001") && n2.ends_with("-002"), "{n1} {n2}");
}

#[test]
fn project_keeps_repo_and_client() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = gizai_core::clients::create(&db, &s.you_id, ClientInput { name: "Kade Logistics B.V.".into(), ..Default::default() }).unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput {
        name: "Kade portal".into(), key: "KADE".into(), client_id: Some(c), repo_path: Some("/home/j/Code/kade".into()),
        ..Default::default()
    }).unwrap();
    let got = projects::get(&db, &p).unwrap();
    assert_eq!(got.client_name.as_deref(), Some("Kade Logistics B.V."));
    assert_eq!((got.repo_path.as_deref(), got.default_branch.as_str()), (Some("/home/j/Code/kade"), "main"));
}

#[test]
fn unicode_and_large_markdown_round_trip() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = project(&db, &s.you_id, "GFW");
    let big = "## Café über Zoë ëëë 🚲\n".repeat(45_000);
    let id = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "Ünïcode ✓".into(), description_md: big.clone(), ..Default::default() }).unwrap();
    let t = tasks::get(&db, &id).unwrap();
    assert_eq!(t.title, "Ünïcode ✓");
    assert_eq!(t.description_md, big);
}

#[test]
fn move_and_labels_and_comments() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = project(&db, &s.you_id, "HAV");
    let id = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "Hero".into(), ..Default::default() }).unwrap();
    let todo: String = db.read(|c| Ok(c.query_row("SELECT id FROM workflow_states WHERE name='To do'", [], |r| r.get(0))?)).unwrap();
    tasks::move_to(&db, &s.you_id, &id, &todo, "a1").unwrap();
    let fe: String = db.read(|c| Ok(c.query_row("SELECT id FROM labels WHERE name='frontend'", [], |r| r.get(0))?)).unwrap();
    tasks::set_labels(&db, &s.you_id, &id, vec![fe]).unwrap();
    comments::add(&db, &s.you_id, &id, "Looks good **so far**", None).unwrap();
    let t = tasks::get(&db, &id).unwrap();
    assert_eq!((t.state_name.as_str(), t.labels[0].name.as_str()), ("To do", "frontend"));
    assert_eq!(comments::list(&db, &id).unwrap()[0].body_md, "Looks good **so far**");
    assert!(tasks::activity(&db, &id).unwrap().len() >= 3, "create, move, labels recorded");
}

#[test]
fn patch_updates_and_clears_fields() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = project(&db, &s.you_id, "HAV");
    let id = tasks::create(&db, &s.you_id, TaskInput { project_id: p, title: "Hero".into(), ..Default::default() }).unwrap();
    tasks::update(&db, &s.you_id, &id, TaskPatch { title: Some("Hero block".into()), assignee_id: Some(s.you_id.clone()), priority: Some(2), ..Default::default() }).unwrap();
    let t = tasks::get(&db, &id).unwrap();
    assert_eq!((t.title.as_str(), t.assignee_kind.as_deref(), t.priority), ("Hero block", Some("person"), 2));
    tasks::update(&db, &s.you_id, &id, TaskPatch { assignee_id: Some(String::new()), ..Default::default() }).unwrap();
    assert!(tasks::get(&db, &id).unwrap().assignee_id.is_none(), "empty string clears the assignee");
    assert!(tasks::update(&db, &s.you_id, &id, TaskPatch { title: Some("  ".into()), ..Default::default() }).is_err());
}

#[test]
fn list_filters_by_project() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let a = project(&db, &s.you_id, "AAA");
    let b = project(&db, &s.you_id, "BBB");
    for (p, t) in [(&a, "a1"), (&a, "a2"), (&b, "b1")] {
        tasks::create(&db, &s.you_id, TaskInput { project_id: p.to_string(), title: t.into(), ..Default::default() }).unwrap();
    }
    assert_eq!(tasks::list(&db, &TaskFilter { project_id: Some(a), open_only: true }).unwrap().len(), 2);
    assert_eq!(tasks::list(&db, &TaskFilter::default()).unwrap().len(), 3);
}
