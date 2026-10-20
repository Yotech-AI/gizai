// GA-107: the folders the Editor may open (projects' linked folders, every folder in an Editor folder, cards' worktrees),
// Settings → General → Editor folders (~/Herd until you set your own), the refusals agent folders get, in the Editor's
// words, and why a folder can't be opened, in a plain sentence.
// Linux and macOS only: Unix paths and links.
#![cfg(unix)]
use std::collections::HashMap;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use gizai_core::db::Db;
use gizai_core::editor::{self, EditorFolder, Kind};
use gizai_core::folders::Places;
use gizai_core::model::{ProjectInput, TaskInput};
use gizai_core::{projects, seed, tasks};

/// A temp folder by its real path (on macOS /var/folders is /private/var/folders).
fn real_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

/// A home folder and Gizai's data folder in `tmp`, as the checks see them.
fn places(tmp: &Path) -> Places {
    let (home, data) = (tmp.join("home"), tmp.join("data"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    Places { home, data: vec![data], checkouts: vec![] }
}

fn dirs(base: &Path, names: &[&str]) {
    for n in names {
        std::fs::create_dir_all(base.join(n)).unwrap();
    }
}

fn s(list: &[&str]) -> Vec<String> {
    list.iter().map(|x| x.to_string()).collect()
}

fn real(p: &Path) -> String {
    p.canonicalize().unwrap().display().to_string()
}

fn project(db: &Db, you: &str, name: &str, key: &str, repo: &str) -> String {
    projects::create(db, you, ProjectInput { name: name.into(), key: key.into(), repo_path: Some(repo.into()), ..Default::default() }).unwrap()
}

/// A card in `project` with `branch` (None: never started), and its identifier.
fn card(db: &Db, you: &str, project: &str, branch: Option<&str>) -> (String, String) {
    let id = tasks::create(db, you, TaskInput { project_id: project.into(), title: "Export invoices".into(), ..Default::default() }).unwrap();
    db.write(None, |w| {
        w.conn().execute("UPDATE tasks SET branch = ?2 WHERE id = ?1", rusqlite::params![id, branch])?;
        Ok(())
    }).unwrap();
    let identifier = tasks::get(db, &id).unwrap().identifier;
    (id, identifier)
}

fn ids(list: &[EditorFolder]) -> Vec<(Kind, String)> {
    list.iter().map(|f| (f.kind, f.name.clone())).collect()
}

#[test]
fn the_editor_folders_are_herd_until_you_set_your_own_and_an_empty_list_stays_empty() {
    let tmp = real_tempdir();
    let p = places(tmp.path());
    let db = Db::open_in_memory().unwrap();
    assert_eq!((editor::ROOTS, editor::DEFAULT_ROOTS), ("editor_roots", ["~/Herd"]));
    assert_eq!(editor::roots(&db), ["~/Herd"]);
    let elsewhere = tmp.path().join("srv/projects");
    let saved = editor::set_roots(&db, &s(&["  ", "~/Code/", &format!("{}/", elsewhere.display()), &p.home.join("Sites").display().to_string()]), &p).unwrap();
    assert_eq!(saved, ["~/Code".to_string(), elsewhere.display().to_string(), "~/Sites".to_string()], "written as Settings shows them; not there yet is fine");
    assert_eq!(editor::roots(&db), saved);
    assert_eq!(editor::set_roots(&db, &[], &p).unwrap(), Vec::<String>::new());
    assert_eq!(editor::roots(&db), Vec::<String>::new(), "an empty list stays empty");
}

#[test]
fn an_editor_folder_is_refused_with_the_agent_folders_refusals_and_says_why() {
    let tmp = real_tempdir();
    let p = places(tmp.path());
    let db = Db::open_in_memory().unwrap();
    dirs(&p.home, &[".ssh", "Herd"]);
    std::fs::write(p.home.join("notes.txt"), "").unwrap();
    symlink(p.home.join(".ssh"), p.home.join("keys")).unwrap();
    let refused = |raw: &str| editor::clean_roots(&s(&[raw]), &p).unwrap_err().to_string();
    let data = p.data[0].display().to_string();
    assert_eq!(refused("Herd"), "Editor folders: Herd can't be used: give the full path, like ~/Herd or /srv/projects");
    assert_eq!(refused("/"), "Editor folders: / can't be used: that's the whole disk (/): pick the folder that holds your projects");
    assert_eq!(refused("~"), "Editor folders: ~ can't be used: that's your home folder: pick the folder that holds your projects inside it");
    assert!(refused(&tmp.path().display().to_string()).contains("it holds your home folder"));
    assert_eq!(refused("~/.ssh"), "Editor folders: ~/.ssh can't be used: it holds keys or logins (~/.ssh)");
    assert!(refused(&format!("{data}/worktrees")).ends_with(&format!("that's Gizai's data folder ({data}), or inside it")));
    assert!(refused("~/keys").contains("it leads to ~/.ssh: it holds keys or logins"), "{}", refused("~/keys"));
    assert_eq!(refused("~/notes.txt"), "Editor folders: ~/notes.txt can't be used: that's a file: pick a folder");
    let twice = editor::clean_roots(&s(&["~/Herd", &p.home.join("Herd/").display().to_string()]), &p).unwrap_err().to_string();
    assert!(twice.ends_with("can't be used: it's in the list twice"), "{twice}");
    // nothing is saved when one is refused
    assert!(editor::set_roots(&db, &s(&["~/Herd", "/"]), &p).is_err());
    assert_eq!(editor::roots(&db), ["~/Herd"]);
}

#[test]
fn the_editor_may_open_linked_folders_the_folders_in_its_editor_folders_and_card_worktrees_each_once() {
    let tmp = real_tempdir();
    let p = places(tmp.path());
    let db = Db::open_in_memory().unwrap();
    let you = seed::ensure_seed(&db, "Jeffrey").unwrap().you_id;
    let herd = p.home.join("Herd");
    dirs(&herd, &["shop", "kade-portal", ".cache", "Zeta"]);
    std::fs::write(herd.join("notes.txt"), "").unwrap();
    dirs(&p.home, &[".ssh"]);
    symlink(p.home.join(".ssh"), herd.join("secrets")).unwrap();
    dirs(tmp.path(), &["code/blog", "elsewhere"]);
    symlink(tmp.path().join("elsewhere"), herd.join("linked")).unwrap();
    let shop = project(&db, &you, "Shop", "SHOP", "~/Herd/shop");
    let blog = project(&db, &you, "Blog", "BLOG", &tmp.path().join("code/blog").display().to_string());
    project(&db, &you, "Gone", "GONE", &tmp.path().join("gone").display().to_string());
    project(&db, &you, "Everything", "ALL", "~");
    // the cards: SHOP-1 to SHOP-10, with a worktree for SHOP-9 and SHOP-10 (and one for SHOP-1, which has no branch)
    let wt = p.data[0].join("worktrees");
    let mut shop_cards = vec![];
    for n in 1..=10 {
        let branch = (n >= 8).then(|| format!("gizai/shop-{n}"));
        shop_cards.push(card(&db, &you, &shop, branch.as_deref()));
    }
    let (blog_1, _) = card(&db, &you, &blog, Some("gizai/blog-1"));
    dirs(&wt, &["SHOP/SHOP-1", "SHOP/SHOP-9", "SHOP/SHOP-10", "BLOG/BLOG-1"]);
    // SHOP-8's worktree leads out of the worktrees folder
    std::fs::create_dir_all(wt.join("SHOP")).unwrap();
    symlink(tmp.path().join("elsewhere"), wt.join("SHOP/SHOP-8")).unwrap();
    // an archived card's worktree doesn't show
    let (archived, _) = card(&db, &you, &blog, Some("gizai/blog-2"));
    dirs(&wt, &["BLOG/BLOG-2"]);
    let states = gizai_core::team::get(&db, &gizai_core::team::list(&db).unwrap()[0].id).unwrap().states;
    tasks::move_to(&db, &you, &archived, &states.iter().find(|s| s.category == "done").unwrap().id, "").unwrap();
    tasks::archive(&db, &you, &archived).unwrap();
    // Gizai's data folder as an Editor folder: refused, so nothing in it shows
    editor::set_roots(&db, &s(&["~/Herd"]), &p).unwrap();
    gizai_core::settings::set(&db, editor::ROOTS, &s(&["~/Herd", &p.data[0].display().to_string(), "~/Nothing"])).unwrap();

    let ten = shop_cards[9].0.clone();
    let working = HashMap::from([(ten.clone(), "Backend Agent".to_string())]);
    let list = editor::folders(&db, &p, &wt, &working).unwrap();
    assert_eq!(ids(&list), [
        (Kind::Project, "blog".into()), (Kind::Project, "shop".into()),
        (Kind::Root, "kade-portal".into()), (Kind::Root, "linked".into()), (Kind::Root, "Zeta".into()),
        (Kind::Card, "BLOG-1".into()), (Kind::Card, "SHOP-9".into()), (Kind::Card, "SHOP-10".into()),
    ], "projects, then the Editor folders' folders, then the cards, each by name; shop once, as a project's");
    let by = |name: &str| list.iter().find(|f| f.name == name).unwrap().clone();
    assert_eq!(by("shop"), EditorFolder { id: real(&herd.join("shop")), path: "~/Herd/shop".into(), name: "shop".into(), kind: Kind::Project,
        project_id: Some(shop.clone()), task_id: None, read_only: false, read_only_why: None });
    assert_eq!(by("kade-portal"), EditorFolder { id: real(&herd.join("kade-portal")), path: "~/Herd/kade-portal".into(), name: "kade-portal".into(),
        kind: Kind::Root, project_id: None, task_id: None, read_only: false, read_only_why: None });
    assert_eq!((by("linked").id, by("linked").path), (real(&tmp.path().join("elsewhere")), "~/Herd/linked".to_string()), "id: where it is on disk");
    assert_eq!(by("BLOG-1"), EditorFolder { id: real(&wt.join("BLOG/BLOG-1")), path: wt.join("BLOG/BLOG-1").display().to_string(), name: "BLOG-1".into(),
        kind: Kind::Card, project_id: Some(blog.clone()), task_id: Some(blog_1), read_only: false, read_only_why: None });
    let busy = by("SHOP-10");
    assert_eq!((busy.task_id.as_deref(), busy.read_only, busy.read_only_why.as_deref()), (Some(ten.as_str()), true, Some("Backend Agent is working on SHOP-10")));
    assert!(!by("SHOP-9").read_only);

    // what the screen gets
    let json = serde_json::to_value(by("SHOP-10")).unwrap();
    assert_eq!(json, serde_json::json!({"id": real(&wt.join("SHOP/SHOP-10")), "path": wt.join("SHOP/SHOP-10").display().to_string(), "name": "SHOP-10",
        "kind": "card", "projectId": shop, "taskId": ten, "readOnly": true, "readOnlyWhy": "Backend Agent is working on SHOP-10"}));
    assert_eq!(serde_json::to_value(by("kade-portal")).unwrap()["kind"], "root");
    assert_eq!(serde_json::to_value(by("shop")).unwrap()["kind"], "project");

    // a folder the Editor may no longer open stops at once
    let kade = by("kade-portal").id;
    assert_eq!(editor::find(&db, &p, &wt, &working, &kade).unwrap().name, "kade-portal");
    editor::set_roots(&db, &[], &p).unwrap();
    assert!(editor::folders(&db, &p, &wt, &working).unwrap().iter().all(|f| f.kind != Kind::Root));
    let why = editor::find(&db, &p, &wt, &working, &kade).unwrap_err();
    assert!(why.starts_with("The Editor can't open ~/Herd/kade-portal: it isn't a project's folder"), "{why}");
}

#[test]
fn a_folder_the_editor_may_not_open_is_refused_in_a_plain_sentence() {
    let tmp = real_tempdir();
    let p = places(tmp.path());
    let db = Db::open_in_memory().unwrap();
    let you = seed::ensure_seed(&db, "Jeffrey").unwrap().you_id;
    let herd = p.home.join("Herd");
    dirs(&herd, &["shop"]);
    dirs(&p.home, &[".ssh"]);
    dirs(tmp.path(), &["other"]);
    let shop = project(&db, &you, "Shop", "SHOP", "~/Herd/shop");
    let (_, _) = card(&db, &you, &shop, None);
    let wt = p.data[0].join("worktrees");
    dirs(&wt, &["SHOP/SHOP-1"]);
    let none = HashMap::new();
    let find = |id: &str| editor::find(&db, &p, &wt, &none, id);
    assert_eq!(find(&real(&herd.join("shop"))).unwrap().kind, Kind::Project);
    assert_eq!(find("").unwrap_err(), "Pick one of the Editor's folders to open");
    assert_eq!(find("  ").unwrap_err(), "Pick one of the Editor's folders to open");
    assert_eq!(find("/").unwrap_err(), "The Editor can't open /: that's the whole disk (/): pick a project's own folder");
    assert!(find(&p.home.display().to_string()).unwrap_err().contains(": that's your home folder: pick a project's own folder inside it"));
    assert_eq!(find(&p.home.join(".ssh").display().to_string()).unwrap_err(), "The Editor can't open ~/.ssh: it holds keys or logins (~/.ssh)");
    let data = p.data[0].display().to_string();
    let why = find(&wt.join("SHOP/SHOP-1").display().to_string()).unwrap_err();
    assert!(why.ends_with(&format!("that's Gizai's data folder ({data}), or inside it")), "a worktree of a card that never started: {why}");
    let why = find(&tmp.path().join("other").display().to_string()).unwrap_err();
    assert_eq!(why, format!("The Editor can't open {}: it isn't a project's folder, a folder in one of the Editor folders (Settings → General), or a card's worktree",
                            tmp.path().join("other").display()));
    let why = find(&tmp.path().join("gone").display().to_string()).unwrap_err();
    assert_eq!(why, format!("The Editor can't open {}: it isn't there any more", tmp.path().join("gone").display()));
    assert!(find("relative/path").unwrap_err().starts_with("Pick one of the Editor's folders"), "not a full path");
    // ~/Herd itself is no folder to open, only the folders in it
    editor::set_roots(&db, &s(&["~/Herd"]), &p).unwrap();
    assert!(find(&real(&herd)).unwrap_err().contains("it isn't a project's folder"));
}

/// The places as the app checks them on every Editor call: without the projects' main checkouts (their warnings are the
/// agent form's), the rest as `Places::of`.
#[test]
fn the_editors_checks_are_places_without_the_main_checkouts() {
    let tmp = real_tempdir();
    std::fs::create_dir_all(tmp.path().join("data")).unwrap();
    let db = Db::open(&tmp.path().join("data/gizai.db")).unwrap();
    let you = seed::ensure_seed(&db, "Jeffrey").unwrap().you_id;
    project(&db, &you, "Shop", "SHOP", "/srv/shop");
    let (all, light) = (Places::of(&db), Places::without_checkouts(&db));
    assert_eq!(all.checkouts, [("Shop".to_string(), PathBuf::from("/srv/shop"))]);
    assert!(light.checkouts.is_empty());
    assert_eq!((light.home, light.data), (all.home, all.data));
}
