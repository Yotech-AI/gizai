//! GA-30: a project's New worktrees settings, and the finished cards whose worktree can be taken over or removed.
use gizai_core::{db::Db, model::*, projects, seed::ensure_seed, tasks, team, worktrees};

fn db() -> (Db, String) {
    let db = Db::open_in_memory().unwrap();
    let you = ensure_seed(&db, "Jeffrey").unwrap().you_id;
    (db, you)
}

fn input(name: &str, key: &str, repo: &str) -> ProjectInput {
    ProjectInput { name: name.into(), key: key.into(), repo_path: Some(repo.into()), ..Default::default() }
}

#[test]
fn a_new_project_installs_missing_dependencies_and_copies_nothing_by_default() {
    let (db, you) = db();
    let id = projects::create(&db, &you, input("Shop", "SHOP", "/home/j/Code/shop")).unwrap();
    let p = projects::get(&db, &id).unwrap();
    assert_eq!((p.worktree_copy.len(), p.worktree_install, p.worktree_setup), (0, true, None));
}

#[test]
fn the_worktree_settings_are_saved_tidied_and_kept_when_left_out() {
    let (db, you) = db();
    let id = projects::create(&db, &you, ProjectInput {
        worktree_copy: Some(vec![" ./.env ".into(), "node_modules/".into(), "".into(), "./vendor/".into(), "node_modules".into(), "config//local.ini".into()]),
        worktree_install: Some(false), worktree_setup: Some("  php artisan migrate --seed  ".into()),
        ..input("Shop", "SHOP", "/home/j/Code/shop") }).unwrap();
    let p = projects::get(&db, &id).unwrap();
    assert_eq!(p.worktree_copy, [".env", "node_modules/", "vendor/", "config/local.ini"], "tidied, each once");
    assert!(!p.worktree_install);
    assert_eq!(p.worktree_setup.as_deref(), Some("php artisan migrate --seed"));

    // an update that leaves them out (the Team Lead's update_project) keeps them
    projects::update(&db, &you, &id, ProjectInput { status: Some("paused".into()), ..input("Shop", "SHOP", "/home/j/Code/shop") }).unwrap();
    let p = projects::get(&db, &id).unwrap();
    assert_eq!(p.worktree_copy, [".env", "node_modules/", "vendor/", "config/local.ini"]);
    assert!(!p.worktree_install);
    assert_eq!(p.worktree_setup.as_deref(), Some("php artisan migrate --seed"));

    // and the form changes them; an empty setup command removes it
    projects::update(&db, &you, &id, ProjectInput { worktree_copy: Some(vec!["target/".into()]), worktree_install: Some(true),
        worktree_setup: Some("  ".into()), ..input("Shop", "SHOP", "/home/j/Code/shop") }).unwrap();
    let p = projects::get(&db, &id).unwrap();
    assert_eq!((p.worktree_copy, p.worktree_install, p.worktree_setup), (vec!["target/".to_string()], true, None));
}

#[test]
fn copy_paths_outside_the_repository_are_refused() {
    let (db, you) = db();
    for bad in ["../secrets", "/etc/passwd", "~/.ssh", "config/../../x", ".git/config", "./.git"] {
        let r = projects::create(&db, &you, ProjectInput { worktree_copy: Some(vec![bad.into()]), ..input("Shop", "SHOP", "/srv/shop") });
        assert!(matches!(&r, Err(gizai_core::Error::Invalid(m)) if m.contains("relative to the repository")), "{bad}: {r:?}");
    }
    assert!(projects::list(&db).unwrap().is_empty(), "nothing saved");
    assert_eq!(projects::clean_copy_paths(&[".gitignore".into(), ".github/".into()]).unwrap(), [".gitignore", ".github/"],
               "only .git itself is refused");
}

#[test]
fn finished_lists_done_and_cancelled_cards_with_a_branch_most_recent_first() {
    let (db, you) = db();
    let shop = projects::create(&db, &you, input("Shop", "SHOP", "/home/j/Code/shop")).unwrap();
    let blog = projects::create(&db, &you, input("Blog", "BLOG", "/home/j/Code/blog")).unwrap();
    let nowhere = projects::create(&db, &you, ProjectInput { name: "Notes".into(), key: "NOTE".into(), ..Default::default() }).unwrap();
    let states = team::get(&db, &team::list(&db).unwrap()[0].id).unwrap().states;
    let state = |cat: &str| states.iter().find(|s| s.category == cat).unwrap().id.clone();
    let card = |project: &str, cat: &str, branch: Option<&str>, at: i64| {
        let id = tasks::create(&db, &you, TaskInput { project_id: project.into(), title: "Export invoices".into(),
            state_id: Some(state(if cat == "cancelled" { "ready" } else { cat })), ..Default::default() }).unwrap();
        db.write(None, |w| {
            w.conn().execute("UPDATE tasks SET branch = ?2, state_category = ?3, completed_at = NULL, updated_at = ?4 WHERE id = ?1",
                rusqlite::params![id, branch, cat, at])?;
            Ok(())
        }).unwrap();
        id
    };
    let done_old = card(&shop, "done", Some("gizai/shop-1-a"), 1_000);
    let cancelled = card(&shop, "cancelled", Some("gizai/shop-2-b"), 3_000);
    let _review = card(&shop, "review", Some("gizai/shop-3-c"), 4_000);
    let _testing = card(&shop, "testing", Some("gizai/shop-4-d"), 5_000);
    let _no_branch = card(&shop, "done", None, 6_000);
    let blog_done = card(&blog, "done", Some("gizai/blog-1-e"), 2_000);
    let _no_repo = card(&nowhere, "done", Some("gizai/note-1-f"), 7_000);

    let all: Vec<String> = worktrees::finished(&db, None).unwrap().into_iter().map(|c| c.task_id).collect();
    assert_eq!(all, [cancelled.clone(), blog_done.clone(), done_old.clone()],
               "only Done and Cancelled cards with a branch, in a project with a repository; the most recently finished first");
    let shop_only = worktrees::finished(&db, Some(&shop)).unwrap();
    assert_eq!(shop_only.iter().map(|c| c.task_id.as_str()).collect::<Vec<_>>(), [cancelled.as_str(), done_old.as_str()]);
    let c = &shop_only[0];
    assert_eq!((c.category.as_str(), c.branch.as_str(), c.project_key.as_str(), c.repo_path.as_str(), c.default_branch.as_str()),
               ("cancelled", "gizai/shop-2-b", "SHOP", "/home/j/Code/shop", "main"));
}
