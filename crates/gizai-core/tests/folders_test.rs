// GA-45: an agent's folders (agent form → Permissions → Folders): what is refused and why, what is saved, what a run
// gets, and when the Team Lead's update_checkout may update one.
use std::path::{Path, PathBuf};

use gizai_core::db::{self, Db};
use gizai_core::folders::{self, Folder, Places};
use gizai_core::model::{AgentInput, ProjectInput};
use gizai_core::{projects, seed, team};

fn f(path: &str, access: &str) -> Folder {
    Folder { path: path.into(), access: access.into() }
}

/// A home folder of /home/u, with Gizai's data in ~/.local/share/gizai and one more data folder.
fn places() -> Places {
    Places { home: "/home/u".into(), data: vec!["/home/u/.local/share/gizai".into(), "/var/gizai-dev".into()], checkouts: vec![] }
}

fn refusal(p: &str) -> Option<String> {
    folders::refusal(Path::new(p), &places())
}

/// A scratch home folder in a temporary folder, with Gizai's data in it.
fn scratch(dir: &Path) -> Places {
    let home = dir.canonicalize().unwrap().join("home");
    std::fs::create_dir_all(home.join(".local/share/gizai")).unwrap();
    Places { home: home.clone(), data: vec![home.join(".local/share/gizai")], checkouts: vec![] }
}

#[test]
fn a_path_is_expanded_and_worked_out_by_name() {
    let home = Path::new("/home/u");
    assert_eq!(folders::normalize("~/Herd/shared", home), Some(PathBuf::from("/home/u/Herd/shared")));
    assert_eq!(folders::normalize("  ~/x/ ", home), Some(PathBuf::from("/home/u/x")), "trimmed, no slash at the end");
    assert_eq!(folders::normalize("~", home), Some(PathBuf::from("/home/u")));
    assert_eq!(folders::normalize("/srv/a/../b/./c/", home), Some(PathBuf::from("/srv/b/c")));
    assert_eq!(folders::normalize("/..", home), Some(PathBuf::from("/")));
    for relative in ["Herd/shared", "./x", "~other/x", ""] {
        assert_eq!(folders::normalize(relative, home), None, "{relative:?} isn't a full path");
    }
}

#[test]
fn the_whole_disk_the_home_folder_and_the_folders_above_it_are_refused() {
    assert!(refusal("/").unwrap().contains("whole disk"));
    assert!(refusal("/home/u").unwrap().contains("home folder"));
    assert!(refusal("/home").unwrap().contains("holds your home folder"));
}

#[test]
fn gizais_data_folder_is_refused_with_what_is_inside_and_above_it() {
    for p in ["/home/u/.local/share/gizai", "/home/u/.local/share/gizai/worktrees/GA/GA-45", "/var/gizai-dev", "/var/gizai-dev/backups"] {
        let why = refusal(p).unwrap_or_else(|| panic!("{p} was allowed"));
        assert!(why.contains("Gizai's data folder"), "{p}: {why}");
    }
    assert!(refusal("/home/u/.local/share/gizai").unwrap().contains("~/.local/share/gizai"), "shown from ~");
    assert!(refusal("/home/u/.local").unwrap().contains("holds Gizai's data folder"));
    assert!(refusal("/var").unwrap().contains("holds Gizai's data folder"));
}

#[test]
fn folders_with_keys_or_logins_are_refused_with_what_is_inside_and_above_them() {
    for (p, key) in [("/home/u/.ssh", ".ssh"), ("/home/u/.ssh/keys", ".ssh"), ("/home/u/.gnupg", ".gnupg"), ("/home/u/.config", ".config"),
                     ("/home/u/.config/gh", ".config"), ("/home/u/.aws", ".aws"), ("/home/u/.claude", ".claude"), ("/home/u/.claude-2", ".claude-2"),
                     ("/home/u/.codex", ".codex"), ("/home/u/.gemini/x", ".gemini"), ("/home/u/.password-store", ".password-store"),
                     ("/home/u/.local/share/keyrings", ".local/share/keyrings")] {
        let why = refusal(p).unwrap_or_else(|| panic!("{p} was allowed"));
        assert_eq!(why, format!("it holds keys or logins (~/{key})"), "{p}");
    }
}

#[test]
fn an_ordinary_folder_is_allowed() {
    for p in ["/home/u/Herd/shared", "/home/u/.local/share/other-app", "/srv/data", "/home/u/configs", "/home/u/.sshx"] {
        assert_eq!(refusal(p), None, "{p}");
    }
    // no home folder known: only / is refused
    let none = Places::default();
    assert!(folders::refusal(Path::new("/"), &none).is_some());
    assert_eq!(folders::refusal(Path::new("/home/u/.ssh"), &none), None);
}

#[test]
fn clean_expands_drops_empty_rows_and_names_the_first_refused_folder() {
    let dir = tempfile::tempdir().unwrap();
    let p = scratch(dir.path());
    std::fs::create_dir_all(p.home.join("shared")).unwrap();
    let saved = folders::clean(&[f("~/shared", "read"), f("  ", "change"), f(" ~/notes/ ", "change")], &p).unwrap();
    assert_eq!(saved, [f(&p.home.join("shared").display().to_string(), "read"), f(&p.home.join("notes").display().to_string(), "change")],
               "~ expanded, empty row dropped; a folder that isn't there yet is kept");
    for (list, why) in [
        (vec![f("/", "read")], "whole disk"),
        (vec![f("~", "read")], "your home folder"),
        (vec![f("~/.ssh", "read")], "keys or logins (~/.ssh)"),
        (vec![f("~/.local/share/gizai/worktrees", "read")], "Gizai's data folder"),
        (vec![f("shared", "read")], "give the full path"),
        (vec![f("~/a,b", "read")], "rename the folder"),
        (vec![f("~/a*", "read")], "rename the folder"),
        (vec![f("~/shared", "write")], "pick read or read and change"),
        (vec![f("~/shared", "read"), f("~/shared/", "change")], "in the list twice"),
        (vec![f("~/shared", "read"), f("~/shared/out", "change")], "which is set to read"),
    ] {
        let e = folders::clean(&list, &p).unwrap_err().to_string();
        assert!(e.contains("Folders:") && e.contains(why), "{list:?}: {e}");
    }
    std::fs::write(p.home.join("notes.txt"), "x").unwrap();
    assert!(folders::clean(&[f("~/notes.txt", "read")], &p).unwrap_err().to_string().contains("that's a file"));
    let many: Vec<Folder> = (0..=folders::MAX).map(|i| f(&format!("/srv/f{i}"), "read")).collect();
    assert!(folders::clean(&many, &p).unwrap_err().to_string().contains("at most 20"));
    assert!(folders::clean(&[], &p).unwrap().is_empty());
}

#[test]
fn a_read_folder_inside_a_read_and_change_one_is_fine() {
    let p = places();
    let saved = folders::clean(&[f("/srv/a", "change"), f("/srv/a/docs", "read")], &p).unwrap();
    assert_eq!(saved.len(), 2);
}

#[test]
fn the_form_warns_about_a_missing_folder_and_a_main_checkout_set_to_read_and_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = scratch(dir.path());
    let work = dir.path().join("work");
    let checkout = work.join("kade");
    std::fs::create_dir_all(checkout.join("src")).unwrap();
    p.checkouts = vec![("Kade portal".into(), checkout.clone())];
    let c = |path: &Path, access: &str| folders::check(&[f(&path.display().to_string(), access)], &p).remove(0);
    let missing = c(&dir.path().join("gone"), "read");
    assert_eq!((missing.error, missing.warning.as_deref()), (None, Some("It isn't there now: runs go without it until it is")));
    let w = c(&checkout, "change");
    assert_eq!(w.error, None);
    assert!(w.warning.as_deref().unwrap().starts_with("This is Kade portal's main checkout: agents normally never touch it"), "{w:?}");
    assert!(c(&checkout.join("src"), "change").warning.unwrap().starts_with("This is inside Kade portal's main checkout"));
    assert!(c(&work, "change").warning.unwrap().starts_with("This holds Kade portal's main checkout"));
    assert_eq!(c(&checkout, "read").warning, None, "reading the main checkout is fine");
    let refused = c(&p.home.join(".ssh"), "read");
    assert_eq!((refused.error.as_deref(), refused.warning), (Some("it holds keys or logins (~/.ssh)"), None));
    assert_eq!(c(&checkout, "change").path, checkout.display().to_string(), "the path as it would be saved");
}

#[test]
fn a_missing_folder_is_skipped_at_run_time_with_a_note_and_the_others_kept() {
    let dir = tempfile::tempdir().unwrap();
    let p = scratch(dir.path());
    let (shared, out) = (dir.path().join("shared"), dir.path().join("out"));
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    let gone = dir.path().join("gone");
    let list = [f(&shared.display().to_string(), "read"), f(&gone.display().to_string(), "change"), f(&out.display().to_string(), "change")];
    let (kept, notes) = folders::for_run(&list, &p);
    let real = |d: &Path| d.canonicalize().unwrap().display().to_string();
    assert_eq!(kept, [f(&real(&shared), "read"), f(&real(&out), "change")]);
    assert_eq!(notes, [format!("Skipped the folder {}: it's missing, so this run goes without it.", gone.display())]);
}

#[test]
fn a_link_that_leads_to_a_refused_folder_is_skipped_at_run_time() {
    let dir = tempfile::tempdir().unwrap();
    let p = scratch(dir.path());
    std::fs::create_dir_all(p.home.join(".ssh")).unwrap();
    let link = dir.path().join("keys");
    std::os::unix::fs::symlink(p.home.join(".ssh"), &link).unwrap();
    let list = [f(&link.display().to_string(), "read")];
    assert!(folders::check(&list, &p)[0].error.as_deref().unwrap().contains("it leads to ~/.ssh: it holds keys or logins"));
    let (kept, notes) = folders::for_run(&list, &p);
    assert!(kept.is_empty());
    assert!(notes[0].starts_with(&format!("Skipped the folder {}: it leads to ~/.ssh", link.display())), "{notes:?}");
}

#[test]
fn update_checkout_may_update_a_listed_folder_only_when_it_is_read_and_change() {
    let p = places();
    let list = [f("/srv/repos", "read"), f("/srv/repos/kade", "change"), f("/srv/shop", "read")];
    assert_eq!(folders::may_update(&list, Path::new("/srv/repos/kade"), &p), Ok(()));
    assert_eq!(folders::may_update(&list, Path::new("/srv/repos/kade/sub"), &p), Ok(()), "the closest folder that holds it counts");
    let e = folders::may_update(&list, Path::new("/srv/shop"), &p).unwrap_err();
    assert!(e.contains("/srv/shop is set to read") && e.contains("nothing changed"), "{e}");
    assert!(folders::may_update(&list, Path::new("/srv/repos/other"), &p).is_err(), "inside a read folder");
    assert_eq!(folders::may_update(&list, Path::new("/srv/elsewhere"), &p), Ok(()), "a folder outside the list: GA-44's own rules");
    assert_eq!(folders::may_update(&[], Path::new("/srv/shop"), &p), Ok(()));
}

fn agent_input(name: &str, folders: Option<Vec<Folder>>) -> AgentInput {
    let role = name.split(' ').next().unwrap().to_lowercase();
    AgentInput { name: name.into(), role_key: role, folders, ..Default::default() }
}

#[test]
fn an_agents_folders_save_and_reload_and_none_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let db = Db::open(&dir.path().join("data/gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let shared = dir.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let id = team::add_agent(&db, &s.you_id, &s.team_id, agent_input("Backend Agent", None)).unwrap();
    assert!(team::agent(&db, &id).unwrap().folders.is_empty(), "a new agent has none");
    let list = vec![f(&format!("{}/", shared.display()), "read"), f(&dir.path().join("out").display().to_string(), "change")];
    team::update_agent(&db, &s.you_id, &id, agent_input("Backend Agent", Some(list))).unwrap();
    let want = vec![f(&shared.display().to_string(), "read"), f(&dir.path().join("out").display().to_string(), "change")];
    assert_eq!(team::agent(&db, &id).unwrap().folders, want);
    team::update_agent(&db, &s.you_id, &id, agent_input("Backend Agent", None)).unwrap();
    assert_eq!(team::agent(&db, &id).unwrap().folders, want, "None leaves the list as it is");
    // a refused folder changes nothing
    let e = team::update_agent(&db, &s.you_id, &id, agent_input("Backend Agent", Some(vec![f("/", "read")]))).unwrap_err();
    assert!(e.to_string().contains("whole disk"), "{e}");
    assert_eq!(team::agent(&db, &id).unwrap().folders, want);
    // Gizai's own data folder (the database's folder) is refused too
    let e = team::update_agent(&db, &s.you_id, &id, agent_input("Backend Agent", Some(vec![f(&dir.path().join("data/worktrees").display().to_string(), "read")])))
        .unwrap_err();
    assert!(e.to_string().contains("Gizai's data folder"), "{e}");
    team::update_agent(&db, &s.you_id, &id, agent_input("Backend Agent", Some(vec![]))).unwrap();
    assert!(team::agent(&db, &id).unwrap().folders.is_empty(), "an empty list clears it");
    // a new agent can be given folders from the form, and a refused one stops it
    let qa = team::add_agent(&db, &s.you_id, &s.team_id, agent_input("QA Agent", Some(vec![f(&shared.display().to_string(), "read")]))).unwrap();
    assert_eq!(team::agent(&db, &qa).unwrap().folders, [f(&shared.display().to_string(), "read")]);
    assert!(team::add_agent(&db, &s.you_id, &s.team_id, agent_input("Frontend Agent", Some(vec![f("/", "read")]))).is_err());
    assert!(team::all_agents(&db).unwrap().iter().all(|(_, m)| m.name != "Frontend Agent"));
}

#[test]
fn places_know_the_database_folder_and_the_projects_main_checkouts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let db = Db::open(&dir.path().join("data/gizai.db")).unwrap();
    assert_eq!(db.dir(), Some(dir.path().join("data").as_path()));
    assert_eq!(Db::open_in_memory().unwrap().dir(), None);
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), repo_path: Some("/srv/kade".into()), ..Default::default() }).unwrap();
    let p = Places::of(&db);
    assert!(p.data.contains(&dir.path().join("data")), "{:?}", p.data);
    assert_eq!(p.checkouts, [("Kade".to_string(), PathBuf::from("/srv/kade"))]);
}

#[test]
fn an_older_database_gets_an_empty_folder_list_for_each_agent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let id = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        team::add_agent(&db, &s.you_id, &s.team_id, agent_input("Backend Agent", None)).unwrap()
    };
    let c = rusqlite::Connection::open(&path).unwrap();
    // Schema 8: GA-35's 0008_board_check ran, 0009_agent_folders didn't.
    c.execute_batch("ALTER TABLE agent_configs DROP COLUMN folders_json; PRAGMA user_version = 8;").unwrap();
    drop(c);
    let db = Db::open(&path).unwrap();
    assert_eq!(db.read(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))?)).unwrap(), db::SCHEMA_VERSION);
    assert_eq!(db::SCHEMA_VERSION, 9);
    assert!(team::agent(&db, &id).unwrap().folders.is_empty());
}

#[test]
fn a_schema_8_database_keeps_its_board_check_and_every_agent_gets_folders_json_empty() {
    // GA-47: main before the merge was schema 8 (GA-35's board check); 0009 adds the folders and keeps the rest.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let (lead, backend) = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let lead = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
            chat_enabled: Some(true), board_check_minutes: Some(30), ..Default::default() }).unwrap();
        (lead, team::add_agent(&db, &s.you_id, &s.team_id, agent_input("Backend Agent", None)).unwrap())
    };
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("ALTER TABLE agent_configs DROP COLUMN folders_json; PRAGMA user_version = 8;").unwrap();
    let agents: i64 = c.query_row("SELECT COUNT(*) FROM agent_configs", [], |r| r.get(0)).unwrap();
    drop(c);
    let db = Db::open(&path).unwrap();
    let (version, empty, all): (i64, i64, i64) = db.read(|c| Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
        c.query_row("SELECT COUNT(*) FROM agent_configs WHERE folders_json = '[]'", [], |r| r.get(0))?,
        c.query_row("SELECT COUNT(*) FROM agent_configs", [], |r| r.get(0))?))).unwrap();
    assert_eq!((version, empty, all), (9, agents, agents), "every agent kept, each with '[]'");
    assert_eq!(team::agent(&db, &lead).unwrap().board_check_minutes, Some(30), "the board check survives 0009");
    assert!(team::agent(&db, &backend).unwrap().folders.is_empty());
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(snaps.len() == 1 && snaps[0].starts_with("gizai-before-v9-") && snaps[0].ends_with(".db"), "{snaps:?}");
}

#[test]
fn the_board_check_and_the_folders_are_saved_side_by_side_and_each_leaves_the_other_alone() {
    // GA-47 merged GA-35 (board_check_minutes, ?13) and GA-45 (folders_json, ?14) into the same INSERT and UPDATE.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let db = Db::open(&dir.path().join("data/gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let (shared, out) = (dir.path().join("shared").display().to_string(), dir.path().join("out").display().to_string());
    let lead_input = |board: Option<i64>, folders: Option<Vec<Folder>>| AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), board_check_minutes: board, folders, ..Default::default() };
    let id = team::add_agent(&db, &s.you_id, &s.team_id, lead_input(Some(20), Some(vec![f(&shared, "read")]))).unwrap();
    let m = team::agent(&db, &id).unwrap();
    assert_eq!((m.board_check_minutes, m.folders), (Some(20), vec![f(&shared, "read")]), "both saved by add_agent");

    // the folders change, the board check stays
    team::update_agent(&db, &s.you_id, &id, lead_input(None, Some(vec![f(&shared, "read"), f(&out, "change")]))).unwrap();
    let m = team::agent(&db, &id).unwrap();
    assert_eq!((m.board_check_minutes, m.folders.clone()), (Some(20), vec![f(&shared, "read"), f(&out, "change")]));
    // the board check changes, the folders stay
    team::update_agent(&db, &s.you_id, &id, lead_input(Some(45), None)).unwrap();
    let m = team::agent(&db, &id).unwrap();
    assert_eq!((m.board_check_minutes, m.folders.clone()), (Some(45), vec![f(&shared, "read"), f(&out, "change")]));
    // both given at once, then neither: nothing else moves
    team::update_agent(&db, &s.you_id, &id, lead_input(Some(0), Some(vec![f(&out, "read")]))).unwrap();
    team::update_agent(&db, &s.you_id, &id, lead_input(None, None)).unwrap();
    let m = team::agent(&db, &id).unwrap();
    assert_eq!((m.board_check_minutes, m.folders), (None, vec![f(&out, "read")]), "0 turns the check off; the list as last saved");
    // a refused folder stops the whole update, the board check included
    assert!(team::update_agent(&db, &s.you_id, &id, lead_input(Some(60), Some(vec![f("/", "read")]))).is_err());
    assert_eq!(team::agent(&db, &id).unwrap().board_check_minutes, None);
}
