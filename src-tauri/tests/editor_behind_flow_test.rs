// Linux and macOS only: composer and npm are shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
//! GA-110: behind main in the Editor end to end (contract C3): how a folder stands against main (`editor_main`), Fetch
//! (`editor_fetch`: where the project's cards start for a project's folder, origin's default branch for another; not
//! again within a minute) and Update (`editor_update`: in the background, never switching branch, an install only for a
//! lock file that changed, never the setup command, never while a card's worktree is prepared from the folder), and its
//! end as `editor-updated` with what it printed. "GitHub" is a local bare repository; composer and npm are fakes on
//! PATH, which belongs to the whole process: every test here sets the same one first.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use gizai_agents::behind::EditorUpdated;
use gizai_core::editor::Kind;
use gizai_core::model::ProjectInput;
use gizai_core::projects;
use gizai_lib::{AppState, code, editor, runs};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, files: &[(&str, &str)], msg: &str) -> String {
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
        git(dir, &["add", name]);
    }
    git(dir, &["commit", "-q", "--allow-empty", "-m", msg]);
    git_out(dir, &["rev-parse", "HEAD"])
}

/// A temp folder by its real path, the way git and Gizai report it.
fn real_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

/// Fake composer and npm first on PATH, for the whole process: each call goes to calls.log in the folder it installs
/// (vendor/ or node_modules/, which git ignores), and each prints a line as the real ones do.
fn fakes() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let bin = Box::leak(Box::new(real_tempdir())).path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let write = |name: &str, script: &str| {
            let path = bin.join(name);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        };
        write("composer", "#!/bin/sh\nmkdir -p vendor/composer\necho \"composer $*\" >> vendor/calls.log\necho 'Installing dependencies from lock file'\n");
        write("npm", "#!/bin/sh\nmkdir -p node_modules/.bin\necho \"npm $*\" >> node_modules/calls.log\necho 'added 12 packages in 1s'\n");
        let old = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{old}", bin.display())) };
    });
}

/// "GitHub": a bare repository fed from `src`.
struct Github {
    src: PathBuf,
    bare: PathBuf,
}

impl Github {
    fn new(tmp: &Path, name: &str) -> Github {
        let src = tmp.join(format!("{name}-src"));
        std::fs::create_dir(&src).unwrap();
        git(&src, &["init", "-q", "-b", "main"]);
        commit(&src, &[(".gitignore", "vendor/\nnode_modules/\n.env\n"), ("README.md", "# Kade\n"), ("composer.json", "{}\n"),
                       ("composer.lock", "{\"v\": 1}\n"), ("package.json", "{}\n"), ("package-lock.json", "{\"lockfileVersion\": 3}\n")], "init");
        let bare = tmp.join(format!("{name}.git"));
        git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
        Github { src, bare }
    }

    fn url(&self) -> &str {
        self.bare.to_str().unwrap()
    }

    /// A commit on GitHub's main; nobody fetches it.
    fn push(&self, files: &[(&str, &str)], msg: &str) -> String {
        let sha = commit(&self.src, files, msg);
        git(&self.src, &["push", "-q", self.url(), "main"]);
        sha
    }

    /// A clone of it at `dir`, with vendor/ and node_modules/ installed.
    fn clone_to(&self, dir: &Path) -> PathBuf {
        git(dir.parent().unwrap(), &["clone", "-q", self.url(), dir.to_str().unwrap()]);
        std::fs::create_dir_all(dir.join("vendor/composer")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        dir.to_path_buf()
    }
}

/// Project KADE, linked to `folder` and to GitHub, with a setup command that leaves setup-ran behind if it ever runs.
fn kade(st: &AppState, folder: &Path, url: &str) -> String {
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), status: Some("active".into()),
        repo_path: Some(folder.display().to_string()), repo_url: Some(url.into()), default_branch: Some("main".into()),
        worktree_setup: Some("touch setup-ran".into()), ..Default::default() }).unwrap();
    let f = editor::folders(st).unwrap().into_iter().find(|f| f.kind == Kind::Project).expect("the linked folder");
    assert_eq!((f.name.as_str(), f.id.as_str()), ("kade-portal", folder.to_str().unwrap()));
    f.id
}

/// What `editor-updated` got.
fn listen(st: &mut AppState) -> Arc<Mutex<Vec<EditorUpdated>>> {
    let got: Arc<Mutex<Vec<EditorUpdated>>> = Arc::default();
    let seen = got.clone();
    st.notify = Arc::new(move |n| {
        if let runs::Note::EditorUpdated(e) = n {
            seen.lock().unwrap().push(e);
        }
    });
    got
}

/// Waits for the `n`th `editor-updated`.
async fn ended(got: &Arc<Mutex<Vec<EditorUpdated>>>, n: usize) -> EditorUpdated {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(e) = got.lock().unwrap().get(n - 1) {
            return e.clone();
        }
        assert!(Instant::now() < until, "editor-updated #{n} came");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn calls(dir: &Path) -> Vec<String> {
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap_or_default();
    (read("vendor/calls.log") + &read("node_modules/calls.log")).lines().map(str::to_string).collect()
}

#[tokio::test]
async fn a_projects_folder_stands_against_where_its_cards_start_and_fetch_brings_main_in_once_a_minute() {
    fakes();
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let folder = gh.clone_to(&tmp.path().join("kade-portal"));
    let id = kade(&st, &folder, gh.url());

    let m = editor::main(&st, &id).unwrap();
    assert_eq!((m.main_label.as_str(), m.branch.as_deref(), m.on_default, m.behind, m.ahead, m.uncommitted, m.can_update, m.updating),
               ("origin/main", Some("main"), true, 0, 0, 0, false, false), "{m:?}");
    assert_eq!((m.fetched_at, m.fetch_error.as_deref(), m.update_says.as_str()), (None, None, "kade-portal is up to date with origin/main."));

    // two commits on GitHub: Main doesn't see them, Fetch does
    gh.push(&[("app.php", "<?php\n")], "a");
    let to = gh.push(&[("app.js", "1\n")], "b");
    assert_eq!(editor::main(&st, &id).unwrap().behind, 0, "local only");
    let asked = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    let m = editor::fetch(&st, &id).unwrap();
    assert_eq!((m.behind, m.ahead, m.can_update, m.fetch_error.as_deref()), (2, 0, true, None), "{m:?}");
    assert_eq!(m.update_says, format!("move main 2 commits to {}", &to[..7]));
    assert!(m.fetched_at.is_some_and(|at| at + 2000 >= asked), "{m:?}");
    assert_eq!(git_out(&folder, &["rev-parse", "HEAD"]), git_out(&gh.src, &["rev-parse", "HEAD~2"]), "Fetch moves nothing");
    // a change in the folder shows at once
    std::fs::write(folder.join("notes.txt"), "mine\n").unwrap();
    assert_eq!(editor::main(&st, &id).unwrap().uncommitted, 1);
    std::fs::remove_file(folder.join("notes.txt")).unwrap();

    // within a minute of that fetch: not fetched again
    let newer = gh.push(&[("app.css", "a{}\n")], "c");
    assert_eq!(editor::fetch(&st, &id).unwrap().behind, 2, "not fetched twice within a minute");
    // a minute later (the test makes that now): fetched
    st.code.set_timing(Duration::ZERO, code::TURN_WAIT);
    let m = editor::fetch(&st, &id).unwrap();
    assert_eq!((m.behind, m.update_says.clone()), (3, format!("move main 3 commits to {}", &newer[..7])));
}

#[tokio::test]
async fn a_folder_that_isnt_a_projects_follows_its_origins_default_branch_and_says_when_it_has_none() {
    fakes();
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let herd = tmp.path().join("herd");
    std::fs::create_dir_all(&herd).unwrap();
    let gh = Github::new(tmp.path(), "github");
    let shop = gh.clone_to(&herd.join("shop"));
    let broken = gh.clone_to(&herd.join("broken"));
    git(&broken, &["remote", "set-url", "origin", tmp.path().join("gone.git").to_str().unwrap()]);
    let alone = herd.join("alone");
    std::fs::create_dir_all(&alone).unwrap();
    git(&alone, &["init", "-q", "-b", "main"]);
    std::fs::create_dir_all(herd.join("plain")).unwrap();
    let mut s = runs::get_settings(&st);
    s.editor_roots = vec![herd.display().to_string()];
    runs::save_settings(&st, &s).unwrap();
    let names: Vec<(Kind, String)> = editor::folders(&st).unwrap().into_iter().map(|f| (f.kind, f.name)).collect();
    assert_eq!(names.len(), 4, "{names:?}");
    let id = |d: &Path| d.display().to_string();

    let to = gh.push(&[("app.php", "<?php\n")], "a");
    let m = editor::main(&st, &id(&shop)).unwrap();
    assert_eq!((m.main_label.as_str(), m.behind, m.can_update), ("origin/main", 0, false), "{m:?}");
    let m = editor::fetch(&st, &id(&shop)).unwrap();
    assert_eq!((m.behind, m.can_update, m.update_says.clone(), m.fetch_error), (1, true, format!("move main 1 commit to {}", &to[..7]), None));

    // origin doesn't answer: why, until a fetch works
    let m = editor::fetch(&st, &id(&broken)).unwrap();
    let why = m.fetch_error.clone().unwrap_or_default();
    assert!(why.starts_with("Couldn't fetch main from origin: "), "{m:?}");
    assert_eq!((m.fetched_at, m.behind), (None, 0));
    assert_eq!(editor::main(&st, &id(&broken)).unwrap().fetch_error, Some(why), "Main still says so");

    // no origin remote; not a git repository: no main to compare with, and nothing to fetch
    let m = editor::fetch(&st, &id(&alone)).unwrap();
    assert_eq!((m.main_label.as_str(), m.can_update, m.fetch_error.as_deref(), m.update_says.as_str()),
               ("", false, None, "alone has no origin remote: Gizai can't tell whether it's behind."));
    let m = editor::fetch(&st, &id(&herd.join("plain"))).unwrap();
    assert_eq!((m.main_label.as_str(), m.branch, m.can_update, m.update_says.as_str()),
               ("", None, false, "plain isn't a git repository, so there's no main to compare it with."));
    let e = editor::update(&st, &id(&herd.join("plain"))).await.unwrap_err();
    assert_eq!(e, "plain isn't a git repository, so there's no main to compare it with.");
}

#[tokio::test]
async fn update_fast_forwards_in_the_background_installs_only_for_a_changed_lock_and_says_how_it_ended() {
    fakes();
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let got = listen(&mut st);
    let gh = Github::new(tmp.path(), "github");
    let folder = gh.clone_to(&tmp.path().join("kade-portal"));
    let id = kade(&st, &folder, gh.url());
    st.code.set_timing(Duration::ZERO, code::TURN_WAIT);
    let head = || git_out(&folder, &["rev-parse", "HEAD"]);

    // composer.lock changed on GitHub; node_modules/ is only missing: composer install alone
    std::fs::remove_dir_all(folder.join("node_modules")).unwrap();
    gh.push(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let to = gh.push(&[("app.js", "1\n")], "more");
    editor::fetch(&st, &id).unwrap();
    let will = editor::update(&st, &id).await.unwrap();
    assert_eq!(will, format!("move main 2 commits to {} and run composer install", &to[..7]));
    let e = ended(&got, 1).await;
    assert_eq!((e.folder.as_str(), e.ok), (id.as_str(), true), "{e:?}");
    assert_eq!(e.said, format!("Moved main 2 commits to {} and ran composer install.", &to[..7]));
    assert_eq!(e.output, "Installing dependencies from lock file");
    assert_eq!(head(), to);
    assert_eq!(git_out(&folder, &["branch", "--show-current"]), "main");
    assert_eq!(git_out(&folder, &["rev-list", "--count", "--merges", "HEAD"]), "0", "no merge commit");
    assert_eq!(calls(&folder), ["composer install --no-interaction --no-progress --no-ansi"], "npm didn't run");
    assert!(!folder.join("setup-ran").exists(), "never the setup command");
    let m = editor::main(&st, &id).unwrap();
    assert_eq!((m.behind, m.updating, m.can_update, m.update_says.as_str()), (0, false, false, "kade-portal is up to date with origin/main."));

    // no lock file changed: the fast-forward alone, which prints nothing
    std::fs::create_dir_all(folder.join("node_modules")).unwrap();
    let to = gh.push(&[("README.md", "# Kade 2\n")], "docs");
    editor::fetch(&st, &id).unwrap();
    assert_eq!(editor::update(&st, &id).await.unwrap(), format!("move main 1 commit to {}", &to[..7]));
    let e = ended(&got, 2).await;
    assert_eq!((e.ok, e.said, e.output), (true, format!("Moved main 1 commit to {}.", &to[..7]), String::new()));
    assert_eq!(calls(&folder).len(), 1, "no install");

    // refused at once, with the plan's sentence, and nothing changes
    let to = gh.push(&[("package-lock.json", "{\"lockfileVersion\": 3, \"v\": 2}\n")], "new js packages");
    editor::fetch(&st, &id).unwrap();
    let at = head();
    std::fs::write(folder.join("README.md"), "# Mine\n").unwrap();
    assert_eq!(editor::update(&st, &id).await.unwrap_err(), "kade-portal has uncommitted changes in README.md. Commit or stash them first.");
    assert_eq!(std::fs::read_to_string(folder.join("README.md")).unwrap(), "# Mine\n");
    git(&folder, &["checkout", "-q", "--", "README.md"]);
    git(&folder, &["switch", "-q", "-c", "feature/x"]);
    assert_eq!(editor::update(&st, &id).await.unwrap_err(),
               "kade-portal is on branch feature/x, not main. Switch to main first, in a terminal or your own editor.");
    assert_eq!(git_out(&folder, &["branch", "--show-current"]), "feature/x", "it didn't switch");
    git(&folder, &["switch", "-q", "main"]);
    commit(&folder, &[("mine.txt", "x\n")], "mine");
    let mine = head();
    let e = editor::update(&st, &id).await.unwrap_err();
    assert!(e.starts_with("main in kade-portal has 1 commit that origin/main doesn't"), "{e}");
    assert_eq!(head(), mine);
    git(&folder, &["reset", "-q", "--hard", &at]);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(got.lock().unwrap().len(), 2, "a refused update sends no editor-updated");
    assert_eq!(calls(&folder).len(), 1, "nothing installed");

    // then it runs: npm ci for package-lock.json
    editor::update(&st, &id).await.unwrap();
    let e = ended(&got, 3).await;
    assert_eq!((e.ok, e.said, e.output), (true, format!("Moved main 1 commit to {} and ran npm ci.", &to[..7]), "added 12 packages in 1s".into()));
    assert_eq!(calls(&folder)[1..], ["npm ci --no-audit --no-fund".to_string()]);
    assert!(!folder.join("setup-ran").exists(), "never the setup command");
}

#[tokio::test]
async fn update_waits_for_a_cards_preparation_from_the_same_folder_runs_one_at_a_time_and_checks_again_when_its_turn_comes() {
    fakes();
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let got = listen(&mut st);
    let gh = Github::new(tmp.path(), "github");
    let folder = gh.clone_to(&tmp.path().join("kade-portal"));
    let id = kade(&st, &folder, gh.url());
    st.code.set_timing(Duration::ZERO, code::TURN_WAIT);
    let head = || git_out(&folder, &["rev-parse", "HEAD"]);
    let before = head();
    let to = gh.push(&[("app.php", "<?php\n")], "a");
    editor::fetch(&st, &id).unwrap();

    // a card's worktree is being prepared from the folder (it holds the folder's lock)
    let lock = code::folder_lock(&st, &folder);
    let held = lock.clone().lock_owned().await;
    let will = editor::update(&st, &id).await.unwrap();
    assert_eq!(will, format!("move main 1 commit to {}", &to[..7]), "it answers at once");
    let m = editor::main(&st, &id).unwrap();
    assert_eq!((m.updating, m.can_update, m.update_says.as_str()), (true, false, will.as_str()), "{m:?}");
    assert!(code::updating(&st, &folder).is_some(), "the Team Lead's update_checkout sees it too");
    assert_eq!(editor::update(&st, &id).await.unwrap_err(), "An update of kade-portal is already running.");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(head(), before, "the update waits for the card's preparation");
    assert!(got.lock().unwrap().is_empty());
    drop(held);
    let e = ended(&got, 1).await;
    assert_eq!((e.ok, e.said.clone()), (true, format!("Moved main 1 commit to {}.", &to[..7])), "{e:?}");
    assert_eq!(head(), to);
    assert!(!editor::main(&st, &id).unwrap().updating);

    // the folder changed while it waited: checked again, and it doesn't start
    let to = gh.push(&[("app.js", "1\n")], "b");
    editor::fetch(&st, &id).unwrap();
    let held = lock.clone().lock_owned().await;
    editor::update(&st, &id).await.unwrap();
    std::fs::write(folder.join("README.md"), "# Mine\n").unwrap();
    let at = head();
    drop(held);
    let e = ended(&got, 2).await;
    assert_eq!((e.ok, e.said.as_str(), e.output.as_str()),
               (false, "The update of kade-portal didn't start: kade-portal has uncommitted changes in README.md. Commit or stash them first.", ""));
    assert_eq!(head(), at, "nothing changed");
    assert_ne!(head(), to);
    assert_eq!(std::fs::read_to_string(folder.join("README.md")).unwrap(), "# Mine\n");
    assert!(!editor::main(&st, &id).unwrap().updating);
}
