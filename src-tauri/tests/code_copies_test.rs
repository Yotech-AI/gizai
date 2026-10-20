//! GA-44: the Team Lead's own read-only copies of the projects' code (`<data dir>/code/<KEY>`), refreshed before each
//! chat turn, and the note and update of an outdated linked folder. "GitHub" is a local bare repository; runs use the
//! fake Claude Code, never the real one.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use gizai_core::model::ProjectInput;
use gizai_core::{chat, projects, team};
use gizai_lib::{AppState, code, tools};
use serde_json::json;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, files: &[(&str, &str)], msg: &str) -> String {
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
        git(dir, &["add", name]);
    }
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", msg]);
    git_out(dir, &["rev-parse", "HEAD"])
}

/// "GitHub" (a bare repository, fed from `src`) and the project's linked folder in ~/Herd: a clone of it with vendor/
/// and node_modules/ installed and an ignored .env.
struct Github {
    src: PathBuf,
    bare: PathBuf,
}

impl Github {
    fn new(tmp: &Path, name: &str) -> Github {
        let src = tmp.join(format!("{name}-src"));
        std::fs::create_dir(&src).unwrap();
        git(&src, &["init", "-q", "-b", "main"]);
        commit(&src, &[(".gitignore", "vendor/\nnode_modules/\n.env\n"), ("README.md", &format!("{name}\n")), ("composer.json", "{}\n"),
                       ("composer.lock", "{\"v\": 1}\n"), ("package.json", "{}\n"), ("package-lock.json", "{\"lockfileVersion\": 3}\n")], "init");
        let bare = tmp.join(format!("{name}.git"));
        git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
        Github { src, bare }
    }

    fn url(&self) -> &str {
        self.bare.to_str().unwrap()
    }

    /// A commit on GitHub's main (nobody fetches it).
    fn push(&self, files: &[(&str, &str)], msg: &str) -> String {
        let sha = commit(&self.src, files, msg);
        git(&self.src, &["push", "-q", self.url(), "main"]);
        sha
    }

    fn main(&self) -> String {
        git_out(&self.bare, &["rev-parse", "main"])
    }

    fn herd(&self, tmp: &Path, name: &str) -> PathBuf {
        let folder = tmp.join(name);
        git(tmp, &["clone", "-q", self.url(), folder.to_str().unwrap()]);
        std::fs::create_dir_all(folder.join("vendor/composer")).unwrap();
        std::fs::create_dir_all(folder.join("node_modules/.bin")).unwrap();
        std::fs::write(folder.join(".env"), "APP_KEY=secret\n").unwrap();
        folder
    }
}

/// Creates or updates project `key` (its linked folder, GitHub link and status).
fn set_project(st: &AppState, key: &str, folder: Option<&Path>, url: Option<&str>, status: &str) {
    let input = ProjectInput { name: format!("Project {key}"), key: key.into(), status: Some(status.into()),
        repo_path: folder.map(|f| f.display().to_string()), repo_url: url.map(str::to_string), default_branch: Some("main".into()),
        ..Default::default() };
    match projects::list(&st.db).unwrap().into_iter().find(|p| p.key == key) {
        Some(p) => projects::update(&st.db, &st.you_id, &p.id, input).unwrap(),
        None => { projects::create(&st.db, &st.you_id, input).unwrap(); }
    }
}

fn project(st: &AppState, key: &str) -> gizai_core::model::Project {
    projects::list(&st.db).unwrap().into_iter().find(|p| p.key == key).unwrap()
}

fn copy(st: &AppState, key: &str) -> PathBuf {
    st.data_dir.join("code").join(key)
}

/// Whether `git worktree list` in `repo` shows `dir`. git gives the real path, which on macOS differs for a temp folder
/// (/var/folders/… is /private/var/folders/…).
fn listed(repo: &Path, dir: &Path) -> bool {
    let real = dir.canonicalize().ok()
        .or_else(|| Some(dir.parent()?.canonicalize().ok()?.join(dir.file_name()?)))
        .unwrap_or_else(|| dir.to_path_buf());
    git_out(repo, &["worktree", "list", "--porcelain"]).lines()
        .any(|l| l == format!("worktree {}", dir.display()) || l == format!("worktree {}", real.display()))
}

fn mtimes(dir: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name() != ".git")
        .map(|e| (e.path(), e.metadata().unwrap().modified().unwrap())).collect();
    out.sort();
    out
}

fn lead(st: &AppState) -> String {
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    team::add_agent(&st.db, &st.you_id, &team_id, gizai_core::model::AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap()
}

/// Every turn fetches (no minute between fetches); the turn waits as long as in Gizai.
fn always_fetch(st: &AppState) {
    st.code.set_timing(Duration::ZERO, code::TURN_WAIT);
}

#[tokio::test]
async fn a_commit_pushed_to_github_shows_up_in_the_copy_at_the_next_turn_and_the_herd_folder_stays_as_it_is() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    git(&herd, &["switch", "-q", "-c", "feature/presentation-cta-banner"]);
    std::fs::write(herd.join("README.md"), "my uncommitted change\n").unwrap();
    let branches = git_out(&herd, &["branch", "--list"]);
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);

    let first = gh.push(&[("app.php", "<?php\n")], "on github before the first turn");
    let turn = code::before_turn(&st, "thread-1").await;
    let dir = copy(&st, "KADE");
    assert_eq!(turn.dirs, [dir.display().to_string()]);
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), first);
    assert!(turn.line.starts_with("[Gizai: Your copies of the code: KADE ") && turn.line.contains(&first[..7]), "{}", turn.line);
    assert!(!turn.line.contains("couldn't"), "{}", turn.line);
    // the same commit a new card of the project starts from, chosen by the same helper
    let start = gizai_lib::git::start_point(&project(&st, "KADE"), &herd, None).unwrap();
    assert_eq!(git_out(&herd, &["rev-parse", &start]), first);
    assert_eq!(git_out(&dir, &["branch", "--show-current"]), "", "detached");
    assert!(!dir.join(".env").exists() && !dir.join("vendor").exists(), "tracked files only");

    let second = gh.push(&[("app.php", "<?php // 2\n")], "pushed between two turns");
    let turn = code::before_turn(&st, "thread-1").await;
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), second, "the next turn shows it");
    assert!(turn.line.contains(&second[..7]), "{}", turn.line);

    // the linked folder: on its feature branch, its uncommitted change untouched, no branch made
    assert_eq!(git_out(&herd, &["branch", "--show-current"]), "feature/presentation-cta-banner");
    assert_eq!(std::fs::read_to_string(herd.join("README.md")).unwrap(), "my uncommitted change\n");
    assert_eq!(git_out(&herd, &["status", "--porcelain", "--untracked-files=no"]), "M README.md");
    assert_eq!(git_out(&herd, &["branch", "--list"]), branches);
    assert!(herd.join(".env").is_file() && herd.join("vendor").is_dir());
}

#[tokio::test]
async fn a_repository_with_two_github_remotes_follows_the_one_the_project_links() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let origin = Github::new(tmp.path(), "origin");
    let oranje = Github::new(tmp.path(), "oranje-uil");
    let herd = origin.herd(tmp.path(), "herd");
    git(&herd, &["remote", "add", "oranje-uil", oranje.url()]);
    let theirs = oranje.push(&[("cards.md", "pushed here\n")], "a card's merge");
    origin.push(&[("other.md", "not this one\n")], "origin moves too");
    set_project(&st, "OO", Some(&herd), Some(oranje.url()), "active");

    code::before_turn(&st, "t").await;
    let dir = copy(&st, "OO");
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), theirs);
    assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "oranje-uil\n");
    let start = gizai_lib::git::start_point(&project(&st, "OO"), &herd, None).unwrap();
    assert_eq!(start, "refs/remotes/oranje-uil/main", "the same start as a new card");
}

#[tokio::test]
async fn a_second_turn_within_a_minute_doesnt_fetch_and_an_unmoved_copy_is_not_touched() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    let first = gh.main();
    code::before_turn(&st, "t").await;
    let dir = copy(&st, "KADE");
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), first);
    std::fs::write(dir.join("scratch.txt"), "left here\n").unwrap();
    let before = mtimes(&dir);
    std::thread::sleep(Duration::from_millis(20));

    let later = gh.push(&[("app.php", "<?php\n")], "on github");
    code::before_turn(&st, "t").await;
    assert_eq!(git_out(&herd, &["rev-parse", "refs/remotes/origin/main"]), first, "not fetched within a minute");
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), first);
    assert_eq!(mtimes(&dir), before, "the commit didn't move: no file in the copy touched");

    // a minute later (made shorter here) it fetches, and the copy moves
    always_fetch(&st);
    code::before_turn(&st, "t").await;
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), later);
    assert!(!dir.join("scratch.txt").exists(), "a moved copy throws away what changed in it");
}

#[tokio::test]
async fn a_card_start_that_just_fetched_saves_the_copy_a_fetch() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    code::fetched(&st, "KADE");
    gh.push(&[("app.php", "<?php\n")], "on github");
    code::before_turn(&st, "t").await;
    assert!(git_out(&herd, &["rev-parse", "--verify", "--quiet", "refs/remotes/origin/main"]) != gh.main(), "no second fetch");
}

#[tokio::test]
async fn a_failing_fetch_keeps_the_last_good_copy_and_the_line_says_why() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    let good = gh.main();
    code::before_turn(&st, "t").await;
    gh.push(&[("app.php", "<?php\n")], "can't be fetched");
    git(&herd, &["config", "remote.origin.uploadpack", "false"]);
    let turn = code::before_turn(&st, "t").await;
    assert_eq!(git_out(&copy(&st, "KADE"), &["rev-parse", "HEAD"]), good);
    assert_eq!(turn.dirs, [copy(&st, "KADE").display().to_string()], "the last good copy is still read");
    assert!(turn.line.contains(&format!("KADE {}", &good[..7])) && turn.line.contains("couldn't be refreshed"), "{}", turn.line);
}

#[tokio::test]
async fn with_github_unreachable_the_turn_starts_within_ten_seconds_on_the_last_good_copy() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    let good = gh.main();
    code::before_turn(&st, "t").await;
    gh.push(&[("app.php", "<?php\n")], "behind a hanging network");
    // the fetch hangs (like a network that doesn't answer) for 12 s, then goes through
    git(&herd, &["config", "remote.origin.uploadpack", "sleep 12; git-upload-pack"]);
    let started = Instant::now();
    let turn = code::before_turn(&st, "t").await;
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(9) && waited < Duration::from_secs(11), "{waited:?}");
    assert_eq!(git_out(&copy(&st, "KADE"), &["rev-parse", "HEAD"]), good, "the last good copy");
    assert!(turn.line.contains(&good[..7]) && turn.line.contains("couldn't be refreshed: still fetching after 10 s"), "{}", turn.line);
}

#[tokio::test]
async fn a_paused_archived_or_unlinked_project_loses_its_copy_at_the_next_turn_or_start_up() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let mut folders = vec![];
    for key in ["KADE", "PAUS", "ARCH", "UNL", "MOVE"] {
        let herd = gh.herd(tmp.path(), &key.to_lowercase());
        set_project(&st, key, Some(&herd), Some(gh.url()), "active");
        folders.push(herd);
    }
    // a project without a GitHub link follows its local default branch
    let local = Github::new(tmp.path(), "local").herd(tmp.path(), "nolink");
    set_project(&st, "LOC", Some(&local), None, "active");
    code::startup(&st).await;
    for key in ["KADE", "PAUS", "ARCH", "UNL", "MOVE", "LOC"] {
        assert!(copy(&st, key).join(".git").exists(), "{key}");
    }
    assert_eq!(git_out(&copy(&st, "LOC"), &["rev-parse", "HEAD"]), git_out(&local, &["rev-parse", "main"]));
    // something in the folder of copies that links elsewhere loses only the link
    let keep = tmp.path().join("keep");
    std::fs::create_dir(&keep).unwrap();
    std::fs::write(keep.join("mine.txt"), "mine\n").unwrap();
    std::os::unix::fs::symlink(&keep, st.data_dir.join("code/STRAY")).unwrap();

    set_project(&st, "PAUS", Some(&folders[1]), Some(gh.url()), "paused");
    set_project(&st, "ARCH", Some(&folders[2]), Some(gh.url()), "archived");
    set_project(&st, "UNL", None, None, "active");
    let other_gh = Github::new(tmp.path(), "other");
    let other = other_gh.herd(tmp.path(), "moved");
    set_project(&st, "MOVE", Some(&other), Some(other_gh.url()), "active");
    code::before_turn(&st, "t").await;
    for (key, folder) in [("PAUS", &folders[1]), ("ARCH", &folders[2]), ("UNL", &folders[3])] {
        assert!(!copy(&st, key).exists(), "{key}'s copy is gone");
        assert!(!listed(folder, &copy(&st, key)), "{key}: git worktree list no longer shows it");
    }
    assert!(!listed(&folders[4], &copy(&st, "MOVE")), "the copy of the old repository went");
    assert!(gizai_agents::copies::is_checkout_of(&copy(&st, "MOVE"), &other), "and one of the new repository came");
    assert!(!st.data_dir.join("code/STRAY").exists() && keep.join("mine.txt").is_file());
    assert!(copy(&st, "KADE").join(".git").exists() && listed(&folders[0], &copy(&st, "KADE")));

    // at start-up too
    set_project(&st, "KADE", Some(&folders[0]), Some(gh.url()), "paused");
    code::startup(&st).await;
    assert!(!copy(&st, "KADE").exists() && !listed(&folders[0], &copy(&st, "KADE")));
    for folder in &folders {
        assert_eq!(git_out(folder, &["status", "--porcelain", "--untracked-files=no"]), "", "the linked folders are untouched");
    }
}

#[tokio::test]
async fn settings_data_doesnt_list_the_copies_and_a_new_card_never_takes_one_over() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    let task = gizai_lib::test_task(&st, herd.to_str().unwrap(), "backend");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    // the card's worktree installs nothing: the run is what counts here, and composer isn't on every system (macOS CI)
    let p = project(&st, "KADE");
    projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), status: Some("active".into()),
        repo_path: p.repo_path.clone(), repo_url: Some(gh.url().into()), default_branch: Some("main".into()),
        worktree_install: Some(false), ..Default::default() }).unwrap();
    code::startup(&st).await;
    let dir = copy(&st, "KADE");
    let at = git_out(&dir, &["rev-parse", "HEAD"]);

    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let wt = PathBuf::from(run.worktree_path.unwrap());
    assert!(wt.starts_with(st.data_dir.join("worktrees")) && wt != dir, "{}", wt.display());
    assert_eq!(git_out(&dir, &["rev-parse", "HEAD"]), at);
    assert_eq!(git_out(&dir, &["branch", "--show-current"]), "", "still detached");
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let done = team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "done").unwrap().id;
    gizai_core::tasks::move_to(&st.db, &st.you_id, &task, &done, "").unwrap();
    let shown: Vec<String> = gizai_lib::worktrees::list(&st).unwrap().into_iter().map(|w| w.path).collect();
    assert_eq!(shown, [wt.display().to_string()], "only the card's worktree, not the copy");
}

#[tokio::test]
async fn an_outdated_linked_folder_is_noted_on_a_chats_first_turn_and_again_only_when_it_changes() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    assert!(!code::before_turn(&st, "a").await.line.contains("linked folder"));
    // missing commits alone don't count
    gh.push(&[("app.php", "<?php\n")], "a");
    let line = code::before_turn(&st, "a").await.line;
    assert!(!line.contains("linked folder"), "{line}");

    gh.push(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let line = code::before_turn(&st, "a").await.line;
    assert!(line.contains(&format!("KADE's linked folder {} is outdated: composer.lock differs from main's", herd.display()))
            && line.contains("on branch main, 2 commits behind main") && line.contains("composer install") && !line.contains("npm ci"), "{line}");
    let again = code::before_turn(&st, "a").await.line;
    assert!(!again.contains("linked folder"), "not on every turn: {again}");
    let other_chat = code::before_turn(&st, "b").await.line;
    assert!(other_chat.contains("KADE's linked folder"), "a new chat's first turn: {other_chat}");

    std::fs::remove_dir_all(herd.join("node_modules")).unwrap();
    let changed = code::before_turn(&st, "a").await.line;
    assert!(changed.contains("composer.lock differs from main's, node_modules/ is missing") && changed.contains("npm ci"), "{changed}");
    assert!(!code::before_turn(&st, "a").await.line.contains("linked folder"));
}

#[tokio::test]
async fn update_checkout_is_chat_only_for_the_linked_folder_and_posts_its_result_in_the_chat() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    git(&herd, &["switch", "-q", "-c", "feature/x"]);
    let feature = git_out(&herd, &["rev-parse", "HEAD"]);
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    let p = project(&st, "KADE");
    let ran = tmp.path().join("setup-ran");
    projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), status: Some("active".into()),
        repo_path: p.repo_path.clone(), repo_url: Some(gh.url().into()), default_branch: Some("main".into()),
        worktree_setup: Some(format!("touch '{}'", ran.display())), ..Default::default() }).unwrap();
    always_fetch(&st);
    let to = gh.push(&[("app.php", "<?php\n")], "on github");
    code::before_turn(&st, "warm-up").await;
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update my folder").unwrap();

    // outside a chat (a task run): refused
    let e = tools::call(&st, &lead, "update_checkout", json!({"project": "KADE", "switch": true})).await.unwrap_err();
    assert!(e.contains("only in chat"), "{e}");
    // another folder: refused
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "folder": tmp.path().display().to_string()})).await.unwrap_err();
    assert!(e.contains("only updates KADE's linked folder"), "{e}");
    // on another branch, not asked to switch: left alone, and the answer says so
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE"})).await.unwrap_err();
    assert!(e.contains("is on branch feature/x, not main") && e.contains("nothing changed"), "{e}");
    assert_eq!(git_out(&herd, &["branch", "--show-current"]), "feature/x");

    let r = tools::call_in(&st, &lead, Some(&thread), "update_checkout",
                           json!({"project": "KADE", "switch": true, "folder": herd.display().to_string()})).await.unwrap();
    assert_eq!(r["done"], "started");
    assert!(r["will"].as_str().unwrap().contains("switch from feature/x to main"), "{r}");
    let mut said = None;
    for _ in 0..200 {
        said = chat::messages(&st.db, &thread).unwrap().into_iter().find(|m| m.role == "system").and_then(|m| m.body_md);
        if said.is_some() { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let said = said.expect("the result as a system message");
    assert!(said.contains("Updated KADE's linked folder") && said.contains("switched from feature/x") && said.contains(&to[..7])
            && said.contains("ran no installs"), "{said}");
    assert_eq!(git_out(&herd, &["branch", "--show-current"]), "main");
    assert_eq!(git_out(&herd, &["rev-parse", "HEAD"]), to);
    assert_eq!(git_out(&herd, &["rev-parse", "feature/x"]), feature, "the feature branch stays as it was");
    assert!(!ran.exists(), "the project's setup command never runs");
    // the next turn's line mentions it, once
    let line = code::before_turn(&st, &thread).await.line;
    assert!(line.contains("The update you started ended: Updated KADE's linked folder"), "{line}");
    assert!(!code::before_turn(&st, &thread).await.line.contains("The update you started ended"));
}

#[tokio::test]
async fn an_update_and_a_card_preparation_of_the_same_project_wait_for_each_other() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    let task = gizai_lib::test_task(&st, herd.to_str().unwrap(), "backend");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    let p = project(&st, "KADE");
    projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), status: Some("active".into()),
        repo_path: p.repo_path.clone(), repo_url: Some(gh.url().into()), default_branch: Some("main".into()),
        worktree_copy: Some(vec!["vendor/".into()]), worktree_install: Some(false), worktree_setup: Some("true".into()), ..Default::default() }).unwrap();

    // a card's preparation waits while the folder is held (as an update holds it)
    let lock = code::folder_lock(&st, &herd);
    let held = lock.clone().write_owned().await;
    let st2 = st.clone();
    let run = tokio::spawn(async move { gizai_lib::runs::run_once(&st2, &task, None, Some(FAKE.into())).await });
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!run.is_finished(), "the card's preparation waits for the update");
    drop(held);
    let s = tokio::time::timeout(Duration::from_secs(30), run).await.unwrap().unwrap().unwrap();
    assert_eq!(s.status, "succeeded", "{s:?}");

    // an update waits while the folder is held (as a card's preparation holds it)
    let to = gh.push(&[("app.php", "<?php\n")], "on github");
    always_fetch(&st);
    code::before_turn(&st, "warm-up").await;
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update").unwrap();
    let held = lock.clone().read_owned().await;
    let r = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE"})).await.unwrap();
    assert_eq!(r["done"], "started", "it answers at once");
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE"})).await.unwrap_err();
    assert!(e.contains("already running"), "{e}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_ne!(git_out(&herd, &["rev-parse", "HEAD"]), to, "the update waits for the card's preparation");
    drop(held);
    for _ in 0..200 {
        if git_out(&herd, &["rev-parse", "HEAD"]) == to && chat::messages(&st.db, &thread).unwrap().iter().any(|m| m.role == "system") { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(git_out(&herd, &["rev-parse", "HEAD"]), to);
}

#[tokio::test]
async fn update_checkout_leaves_the_linked_folder_alone_when_the_team_leads_folders_set_it_to_read() {
    // GA-47: PR #21 asked whichever of GA-44 and GA-45 merged second to call lead_may_update in update_checkout.
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    let before = git_out(&herd, &["rev-parse", "HEAD"]);
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    let to = gh.push(&[("app.php", "<?php\n")], "on github");
    code::before_turn(&st, "warm-up").await;
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update my folder").unwrap();
    let set_folders = |path: &Path, access: &str| {
        let m = team::agent(&st.db, &lead).unwrap();
        team::update_agent(&st.db, &st.you_id, &lead, gizai_core::model::AgentInput { name: m.name, role_key: m.role_key,
            folders: Some(vec![gizai_core::folders::Folder { path: path.display().to_string(), access: access.into() }]), ..Default::default() }).unwrap();
    };

    // the linked folder set to read: refused, and nothing starts
    set_folders(&herd, "read");
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE"})).await.unwrap_err();
    assert!(e.contains("is set to read in the Team Lead's folders") && e.contains("nothing changed"), "{e}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(git_out(&herd, &["rev-parse", "HEAD"]), before, "the folder stays as it was");
    assert!(chat::messages(&st.db, &thread).unwrap().iter().all(|m| m.role != "system"), "no update ran");

    // set to read and change: the update runs as before
    set_folders(&herd, "change");
    let r = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE"})).await.unwrap();
    assert_eq!(r["done"], "started", "{r}");
    for _ in 0..200 {
        if git_out(&herd, &["rev-parse", "HEAD"]) == to && chat::messages(&st.db, &thread).unwrap().iter().any(|m| m.role == "system") { break; }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(git_out(&herd, &["rev-parse", "HEAD"]), to);
}

/// A temp folder by its real path, the way git and Gizai report it: on macOS /var/folders is /private/var/folders, and on
/// Windows TEMP can be a short name (RUNNER~1) that git gives in full. Without the \\?\ that canonicalize puts before a
/// Windows drive.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let base = std::path::PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    tempfile::tempdir_in(base).unwrap()
}

// ---- GA-156: update_checkout with stash puts uncommitted changes in git's stash first ----

/// Fake composer and npm first on PATH, for the whole process (as in editor_behind_flow_test): each call goes to
/// calls.log in the folder it installs. Only the stash tests install; the other tests here never do.
fn fakes() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let bin = Box::leak(Box::new(real_tempdir())).path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let write = |name: &str, script: &str| {
            let path = bin.join(name);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        };
        write("composer", "#!/bin/sh\nmkdir -p vendor/composer\necho \"composer $*\" >> vendor/calls.log\n");
        // npm fails in a folder with .npm-fails in it
        write("npm", "#!/bin/sh\nif [ -f .npm-fails ]; then echo 'npm ERR! code EUSAGE' >&2; exit 1; fi\nmkdir -p node_modules/.bin\n\
                      echo \"npm $*\" >> node_modules/calls.log\n");
        let old = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{old}", bin.display())) };
    });
}

/// Who makes commits in the folder, so git can make a stash entry there whatever the machine's own git config says. An
/// empty name makes git refuse.
fn identity(dir: &Path, name: &str) {
    git(dir, &["config", "user.name", name]);
    git(dir, &["config", "user.email", "t@t"]);
}

/// `git stash list`, newest first: "On main: Gizai: before the update to main".
fn stashes(dir: &Path) -> Vec<String> {
    git_out(dir, &["stash", "list", "--format=%gs"]).lines().map(str::to_string).collect()
}

/// The update's result, as Gizai posts it in the chat.
async fn update_ended(st: &AppState, thread: &str) -> String {
    for _ in 0..400 {
        if let Some(said) = chat::messages(&st.db, thread).unwrap().into_iter().find(|m| m.role == "system").and_then(|m| m.body_md) {
            return said;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no result in the chat");
}

const LOCAL_LOCK: &str = "{\"lockfileVersion\": 3, \"packages\": {\"left-pad\": {}}}\n";

#[tokio::test]
async fn update_checkout_with_stash_puts_uncommitted_changes_in_gits_stash_first_and_says_how_to_get_them_back() {
    fakes();
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    identity(&herd, "t");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    code::before_turn(&st, "warm-up").await;
    let head = git_out(&herd, &["rev-parse", "HEAD"]);
    // 13 Oct: on main's commit, with package-lock.json changed by a local npm install; a new file of yours isn't tracked
    std::fs::write(herd.join("package-lock.json"), LOCAL_LOCK).unwrap();
    std::fs::write(herd.join("notes.txt"), "mine\n").unwrap();
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update my folder").unwrap();

    // without stash: refused as before, with the new ending; nothing changes and nothing starts
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE"})).await.unwrap_err();
    assert!(e.contains(&format!("{} has uncommitted changes in package-lock.json: nothing changed. Commit or stash them first", herd.display()))
            && e.trim_end_matches('.').ends_with(", or call again with stash only when the user agreed to put them in git's stash"), "{e}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(stashes(&herd).is_empty());
    assert_eq!(std::fs::read_to_string(herd.join("package-lock.json")).unwrap(), LOCAL_LOCK);
    assert!(chat::messages(&st.db, &thread).unwrap().iter().all(|m| m.role != "system"), "no update ran");

    // with stash
    let r = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "stash": true})).await.unwrap();
    assert_eq!(r["done"], "started", "{r}");
    assert_eq!(r["will"], format!("put the uncommitted changes in package-lock.json in git's stash, leave main where it is ({}, already \
                                   main's commit) and run npm ci", &head[..7]));
    let said = update_ended(&st, &thread).await;
    assert_eq!(said, format!("Updated KADE's linked folder {h}: put the uncommitted changes in package-lock.json in git's stash (git -C {h} \
                              stash pop brings them back), stayed at {} (already main's commit) and ran npm ci.", &head[..7], h = herd.display()));
    // one stash entry with that change; the folder is clean and at main's commit; npm ci ran; the untracked file stays
    assert_eq!(stashes(&herd), ["On main: Gizai: before the update to main"]);
    assert_eq!(git_out(&herd, &["stash", "show", "--name-only", "stash@{0}"]), "package-lock.json");
    assert_eq!(git_out(&herd, &["status", "--porcelain"]), "?? notes.txt");
    assert_eq!(git_out(&herd, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read_to_string(herd.join("package-lock.json")).unwrap(), "{\"lockfileVersion\": 3}\n", "main's lock");
    assert_eq!(std::fs::read_to_string(herd.join("node_modules/calls.log")).unwrap(), "npm ci --no-audit --no-fund\n");
    assert!(!herd.join("vendor/calls.log").exists(), "composer.lock is main's: no composer install");
    assert_eq!(std::fs::read_to_string(herd.join("notes.txt")).unwrap(), "mine\n");
    // the next turn hears it, once
    let line = code::before_turn(&st, &thread).await.line;
    assert!(line.contains("The update you started ended: Updated KADE's linked folder") && line.contains("stash pop brings them back"), "{line}");

    // git stash pop, as the message says, brings the change back
    git(&herd, &["stash", "pop", "-q"]);
    assert_eq!(std::fs::read_to_string(herd.join("package-lock.json")).unwrap(), LOCAL_LOCK);
}

#[tokio::test]
async fn update_checkout_with_stash_stashes_nothing_when_something_else_is_in_the_way() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    identity(&herd, "t");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    gh.push(&[("app.php", "<?php\n")], "on github");
    code::before_turn(&st, "warm-up").await;
    std::fs::write(herd.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update my folder").unwrap();
    let now = || (git_out(&herd, &["rev-parse", "HEAD"]), git_out(&herd, &["branch", "--show-current"]), git_out(&herd, &["status", "--porcelain"]));
    let set_folders = |access: &str| {
        let m = team::agent(&st.db, &lead).unwrap();
        team::update_agent(&st.db, &st.you_id, &lead, gizai_core::model::AgentInput { name: m.name, role_key: m.role_key,
            folders: Some(vec![gizai_core::folders::Folder { path: herd.display().to_string(), access: access.into() }]), ..Default::default() }).unwrap();
    };

    // the linked folder set to read in the Team Lead's folders
    set_folders("read");
    let before = now();
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "stash": true})).await.unwrap_err();
    assert!(e.contains("is set to read in the Team Lead's folders") && e.contains("nothing changed"), "{e}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!((now(), stashes(&herd).len()), (before, 0), "nothing stashed, nothing changed");

    // on another branch, not asked to switch
    set_folders("change");
    git(&herd, &["switch", "-q", "-c", "feature/x"]);
    let before = now();
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "stash": true})).await.unwrap_err();
    assert!(e.contains("is on branch feature/x, not main") && e.contains("nothing changed"), "{e}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!((now(), stashes(&herd).len()), (before, 0), "nothing stashed, nothing changed");
    assert!(chat::messages(&st.db, &thread).unwrap().iter().all(|m| m.role != "system"), "no update ran");
}

#[tokio::test]
async fn when_git_cant_stash_the_update_from_the_chat_stops_before_the_fast_forward_and_says_why() {
    fakes();
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    // no name to put on the stash entry: git won't make one
    identity(&herd, "");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    let to = gh.push(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    code::before_turn(&st, "warm-up").await;
    let head = git_out(&herd, &["rev-parse", "HEAD"]);
    std::fs::write(herd.join("package-lock.json"), LOCAL_LOCK).unwrap();
    let deps = (mtimes(&herd.join("vendor")), mtimes(&herd.join("node_modules")));
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update my folder").unwrap();

    let r = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "stash": true})).await.unwrap();
    assert_eq!(r["will"], format!("put the uncommitted changes in package-lock.json in git's stash, move main 1 commit to {} and run composer \
                                   install and npm ci", &to[..7]));
    let said = update_ended(&st, &thread).await;
    assert!(said.starts_with(&format!("The update of KADE's linked folder {} stopped: putting the uncommitted changes in git's stash failed.\n\n```\n",
                                      herd.display())) && said.contains("ident"), "the reason and git's error: {said}");
    assert!(!said.contains("Before that"), "{said}");
    // main, the change and the dependency folders are as they were
    assert_eq!(git_out(&herd, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read_to_string(herd.join("package-lock.json")).unwrap(), LOCAL_LOCK);
    assert!(stashes(&herd).is_empty());
    assert_eq!((mtimes(&herd.join("vendor")), mtimes(&herd.join("node_modules"))), deps, "no install ran");
    assert!(!herd.join("vendor/calls.log").exists() && !herd.join("node_modules/calls.log").exists());
    let line = code::before_turn(&st, &thread).await.line;
    assert!(line.contains("stopped: putting the uncommitted changes in git's stash failed"), "{line}");
}

#[tokio::test]
async fn when_a_step_after_the_stash_fails_the_chat_says_the_changes_are_in_gits_stash_and_how_to_get_them_back() {
    fakes();
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    identity(&herd, "t");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    code::before_turn(&st, "warm-up").await;
    std::fs::write(herd.join("package-lock.json"), LOCAL_LOCK).unwrap();
    std::fs::write(herd.join(".npm-fails"), "").unwrap();
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Update my folder").unwrap();
    tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "stash": true})).await.unwrap();
    let said = update_ended(&st, &thread).await;
    let h = herd.display();
    assert!(said.starts_with(&format!("The update of KADE's linked folder {h} stopped: npm ci failed (exit code 1). Before that it put the \
                                       uncommitted changes in package-lock.json in git's stash (git -C {h} stash pop brings them back).")), "{said}");
    assert!(said.contains("npm ERR! code EUSAGE"), "the end of the output: {said}");
    assert_eq!(stashes(&herd), ["On main: Gizai: before the update to main"]);
}

// ---- GA-160: GA-156's stash where it meets GA-154's closing note, as 0.11.0 has them together ----

/// Gizai's notes in a chat, oldest first.
fn notes(st: &AppState, thread: &str) -> Vec<String> {
    chat::messages(&st.db, thread).unwrap().into_iter().filter(|m| m.role == "system").filter_map(|m| m.body_md).collect()
}

#[tokio::test]
async fn approving_a_kept_update_with_stash_stashes_ends_the_answer_with_the_closing_note_and_then_posts_the_updates_result() {
    fakes();
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let lead = lead(&st);
    let gh = Github::new(tmp.path(), "github");
    let herd = gh.herd(tmp.path(), "herd");
    identity(&herd, "t");
    set_project(&st, "KADE", Some(&herd), Some(gh.url()), "active");
    always_fetch(&st);
    code::before_turn(&st, "warm-up").await;
    let head = git_out(&herd, &["rev-parse", "HEAD"]);
    std::fs::write(herd.join("package-lock.json"), LOCAL_LOCK).unwrap();
    // the answer used a tool from outside Gizai, so update_checkout with stash waits for your OK and changes nothing yet
    let thread = chat::create_thread(&st.db, &st.you_id, &lead, "Look it up in Otus, then update my folder and stash my changes").unwrap();
    gizai_lib::chat::mark_outside(&st, &thread, "mcp__otus__search");
    let e = tools::call_in(&st, &lead, Some(&thread), "update_checkout", json!({"project": "KADE", "stash": true})).await.unwrap_err();
    assert!(e.starts_with("update_checkout wasn't run: this answer used mcp__otus__search"), "{e}");
    let kept = gizai_lib::pending::list(&st, &thread).unwrap();
    assert_eq!(kept.len(), 1, "{kept:?}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(stashes(&herd).is_empty() && notes(&st, &thread).is_empty(), "nothing ran before Approve");
    assert_eq!(std::fs::read_to_string(herd.join("package-lock.json")).unwrap(), LOCAL_LOCK);

    // Approve: the decision's note, then GA-154's closing note (nothing else waits), then the update's own result
    gizai_lib::pending::approve(&st, &kept[0].action.id).await.unwrap();
    let h = herd.display().to_string();
    let result = format!("Updated KADE's linked folder {h}: put the uncommitted changes in package-lock.json in git's stash (git -C {h} \
                          stash pop brings them back), stayed at {} (already main's commit) and ran npm ci.", &head[..7]);
    for _ in 0..400 {
        if notes(&st, &thread).len() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let said = notes(&st, &thread);
    assert_eq!(said.len(), 3, "{said:?}");
    assert!(said[0].starts_with("Approved: ") && said[0].ends_with("The update started: its result comes as a message in this chat."), "{said:?}");
    assert_eq!(said[1], gizai_lib::pending::ANSWER_DONE, "{said:?}");
    assert_eq!(said[2], result, "{said:?}");
    assert!(gizai_lib::pending::list(&st, &thread).unwrap().is_empty());
    assert_eq!(stashes(&herd), ["On main: Gizai: before the update to main"]);
    assert_eq!(git_out(&herd, &["stash", "show", "--name-only", "stash@{0}"]), "package-lock.json");
    assert_eq!(std::fs::read_to_string(herd.join("node_modules/calls.log")).unwrap(), "npm ci --no-audit --no-fund\n");
}
