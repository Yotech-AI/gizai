// Linux and macOS only: the run uses a shell script as a fake Claude Code, which Windows can't start.
#![cfg(unix)]
// GA-107: the Editor's commands end to end (contract C2): Settings → General → Editor folders (~/Herd until you set your
// own, checked on save), the folders the Editor may open, a card's worktree that is read-only while an agent works on it
// (and not after), and the commands on a folder: list, read, save, new, rename, delete, files, search, git, versions and
// Open in. Every call checks the folder again.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gizai_core::editor::Kind;
use gizai_core::model::TaskPatch;
use gizai_core::{settings, tasks, team};
use gizai_lib::{editor, runs};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

/// A temp folder by its real path, the way git and Gizai report it.
fn real_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A repository at `dir` on main, with README.md committed.
fn git_repo(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "# Kade\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
    dir.to_path_buf()
}

#[tokio::test]
async fn a_cards_worktree_is_read_only_while_an_agent_works_on_it_and_says_who() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let repo = git_repo(&tmp.path().join("repo"));
    let card = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    tasks::update(&st.db, &st.you_id, &card, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let identifier = tasks::get(&st.db, &card).unwrap().identifier;
    let team = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    let be = team.members.iter().find(|m| m.name == "Backend Agent").unwrap().actor_id.clone();

    // the project's linked folder
    let list = editor::folders(&st).unwrap();
    let project = list.iter().find(|f| f.kind == Kind::Project).expect("the linked folder");
    assert_eq!((project.id.clone(), project.name.as_str(), project.read_only), (repo.display().to_string(), "repo", false));
    assert!(list.iter().all(|f| f.kind != Kind::Card), "no worktree yet");

    let (run, finished) = runs::start(&st, &card, Some(be), Some(FAKE.into()), "manual").await.unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    let wt = loop {
        if let Some(f) = editor::folders(&st).unwrap().into_iter().find(|f| f.kind == Kind::Card) {
            break f;
        }
        assert!(Instant::now() < until, "the card's worktree shows");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let why = format!("Backend Agent is working on {identifier}");
    assert_eq!((wt.name.as_str(), wt.task_id.as_deref(), wt.read_only, wt.read_only_why.as_deref()), (identifier.as_str(), Some(card.as_str()), true, Some(why.as_str())));
    assert_eq!(wt.id, st.data_dir.join("worktrees/KADE").join(&identifier).canonicalize().unwrap().display().to_string());
    let opened = editor::read(&st, &wt.id, "README.md").unwrap();
    assert_eq!((opened.text.as_deref(), opened.read_only, opened.read_only_why.as_deref()), (Some("# Kade\n"), true, Some(why.as_str())));
    let refused = format!("The Editor can't change README.md now: {why}");
    assert_eq!(editor::write(&st, &wt.id, "README.md", "# Mine\n", &opened.version).unwrap_err(), refused);
    assert!(editor::create(&st, &wt.id, "notes.md", false).unwrap_err().ends_with(&why));
    assert!(editor::rename(&st, &wt.id, "README.md", "READ.md").unwrap_err().ends_with(&why));
    assert!(editor::delete(&st, &wt.id, "README.md").unwrap_err().ends_with(&why));
    let on_disk = std::fs::read_to_string(Path::new(&wt.id).join("README.md")).unwrap();
    assert_eq!(on_disk, "# Kade\n");
    // looking is fine
    assert!(editor::files(&st, &wt.id).unwrap().contains(&"README.md".to_string()));
    assert!(editor::git(&st, &wt.id).unwrap().branch.is_some_and(|b| b.starts_with("gizai/kade-")));

    runs::stop(&st, &run);
    finished.await.unwrap();
    let wt = editor::folder(&st, &wt.id).unwrap();
    assert_eq!((wt.read_only, wt.read_only_why), (false, None), "once the run ended, a person may change it");
    let v = editor::read(&st, &wt.id, "README.md").unwrap().version;
    editor::write(&st, &wt.id, "README.md", "# Mine\n", &v).unwrap();
    assert_eq!(std::fs::read_to_string(Path::new(&wt.id).join("README.md")).unwrap(), "# Mine\n");
    runs::stop_all(&st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn editor_folders_are_a_setting_herd_by_default_and_a_refused_one_saves_nothing() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let s = runs::get_settings(&st);
    assert_eq!(s.editor_roots, ["~/Herd"]);
    assert_eq!(serde_json::to_value(&s).unwrap()["editorRoots"], serde_json::json!(["~/Herd"]));
    // the screen's Settings without the field: ~/Herd
    let mut json = serde_json::to_value(&s).unwrap();
    json.as_object_mut().unwrap().remove("editorRoots");
    assert_eq!(serde_json::from_value::<runs::Settings>(json).unwrap().editor_roots, ["~/Herd"]);

    let mut bad = s.clone();
    bad.editor_roots = vec!["~/Herd".into(), "/".into()];
    bad.max_run_minutes = 30;
    let e = runs::save_settings(&st, &bad).unwrap_err();
    assert_eq!(e, "Editor folders: / can't be used: that's the whole disk (/): pick the folder that holds your projects");
    let now = runs::get_settings(&st);
    assert_eq!((now.editor_roots, now.max_run_minutes), (vec!["~/Herd".to_string()], s.max_run_minutes), "nothing of a refused save is kept");
    bad.editor_roots = vec!["Herd".into()];
    assert!(runs::save_settings(&st, &bad).unwrap_err().contains("give the full path"));
    bad.editor_roots = vec![st.data_dir.join("worktrees").display().to_string()];
    assert!(runs::save_settings(&st, &bad).unwrap_err().contains("Gizai's data folder"));

    let herd = tmp.path().join("herd");
    git_repo(&herd.join("kade-portal"));
    std::fs::create_dir_all(herd.join("shop")).unwrap();
    std::fs::create_dir_all(herd.join(".cache")).unwrap();
    let mut good = s.clone();
    good.editor_roots = vec![format!("{}/", herd.display()), "  ".into()];
    runs::save_settings(&st, &good).unwrap();
    assert_eq!(runs::get_settings(&st).editor_roots, [herd.display().to_string()]);
    let list = editor::folders(&st).unwrap();
    let names: Vec<(Kind, &str)> = list.iter().map(|f| (f.kind, f.name.as_str())).collect();
    assert_eq!(names, [(Kind::Root, "kade-portal"), (Kind::Root, "shop")]);
}

#[tokio::test]
async fn the_commands_work_in_a_folder_the_editor_may_open_and_refuse_any_other() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let herd = tmp.path().join("herd");
    let kade = git_repo(&herd.join("kade-portal"));
    let mut s = runs::get_settings(&st);
    s.editor_roots = vec![herd.display().to_string()];
    runs::save_settings(&st, &s).unwrap();
    let f = editor::folders(&st).unwrap().remove(0).id;
    assert_eq!(f, kade.display().to_string());

    editor::create(&st, &f, "src", true).unwrap();
    let v = editor::write(&st, &f, "src/app.ts", "export const needle = 1;\r\n", "").unwrap();
    assert_eq!(editor::versions(&st, &f, &["src/app.ts".into(), "gone.ts".into()]).unwrap(), [v.clone(), String::new()]);
    let opened = editor::read(&st, &f, "src/app.ts").unwrap();
    assert_eq!((opened.text.as_deref(), opened.crlf, opened.version.as_str()), (Some("export const needle = 1;\n"), false, v.as_str()), "a new file has LF");
    let list = editor::list(&st, &f, "").unwrap();
    assert_eq!(list.iter().map(|e| (e.name.as_str(), e.dir)).collect::<Vec<_>>(), [("src", true), ("README.md", false)]);
    assert_eq!(editor::list(&st, &f, "src").unwrap()[0].path, "src/app.ts");
    assert_eq!(editor::files(&st, &f).unwrap(), ["README.md", "src/app.ts"]);
    let found = editor::search(&st, &f, "NEEDLE", false, true, false).unwrap();
    assert_eq!(found.hits.iter().map(|h| (h.path.as_str(), h.line, h.column)).collect::<Vec<_>>(), [("src/app.ts", 1, 14)]);
    assert!(editor::search(&st, &f, "NEEDLE", true, false, false).unwrap().hits.is_empty());
    let g = editor::git(&st, &f).unwrap();
    assert_eq!((g.branch.as_deref(), g.uncommitted), (Some("main"), 1));
    editor::rename(&st, &f, "src/app.ts", "src/main.ts").unwrap();
    editor::delete(&st, &f, "src").unwrap();
    assert!(!kade.join("src").exists());

    // a path out of the folder, and a folder the Editor may not open
    assert_eq!(editor::read(&st, &f, "../../data/gizai.db").unwrap_err(), "A path can't go up out of the folder (..)");
    let why = editor::list(&st, "/etc", "").unwrap_err();
    assert!(why.starts_with("The Editor can't open /etc: it isn't a project's folder"), "{why}");
    let why = editor::read(&st, &st.data_dir.display().to_string(), "gizai.db").unwrap_err();
    assert!(why.contains("Gizai's data folder"), "{why}");
    assert!(editor::write(&st, "/", "x", "", "").unwrap_err().contains("that's the whole disk"));
    assert!(editor::search(&st, &herd.display().to_string(), "x", false, false, false).unwrap_err().contains("it isn't a project's folder"),
            "the Editor folder itself is no folder to open");
    // Open in: only the three editors, and the path through the gate first
    assert_eq!(editor::open_in(&st, "sublime", &f, None, None).unwrap_err(), "Gizai can't open a folder in sublime: pick VS Code, Zed or PhpStorm");
    assert_eq!(editor::open_in(&st, "zed", &f, Some("../x"), Some(3)).unwrap_err(), "A path can't go up out of the folder (..)");
    assert!(editor::open_in(&st, "vscode", "/etc", None, None).unwrap_err().starts_with("The Editor can't open /etc"));
    let apps = editor::apps(&st);
    assert!(apps.iter().all(|a| ["vscode", "zed", "phpstorm"].contains(&a.id.id())));

    // the Editor folder taken out of Settings: its folders stop at once
    s.editor_roots = vec![];
    runs::save_settings(&st, &s).unwrap();
    assert!(editor::read(&st, &f, "README.md").unwrap_err().contains("it isn't a project's folder"));
}
