//! GA-30: a card's worktree is prepared before its agent starts, takes over a finished card's worktree, and Settings →
//! Data lists and removes the worktrees of finished cards. Runs use the fake Claude Code, never the real one.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

/// A main checkout that ignores build output, with a .env of its own (not committed).
fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), ".env\ntarget/\n*.log\n").unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    std::fs::write(repo.join(".env"), "APP_KEY=from-main\n").unwrap();
    repo
}

/// Sets project KADE's New worktrees settings.
fn set_prepare(st: &gizai_lib::AppState, copy: &[&str], install: bool, setup: &str) {
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), default_branch: Some(p.default_branch.clone()),
        worktree_copy: Some(copy.iter().map(|c| c.to_string()).collect()), worktree_install: Some(install), worktree_setup: Some(setup.into()),
        ..Default::default() }).unwrap();
}

/// Moves the card to Done, or makes it Cancelled (the default board has no Cancelled column, so that one is set in
/// the database).
fn finish(st: &gizai_lib::AppState, task: &str, category: &str) {
    if category == "cancelled" {
        st.db.write(None, |w| { w.conn().execute("UPDATE tasks SET state_category = 'cancelled' WHERE id = ?1", [task])?; Ok(()) }).unwrap();
        return;
    }
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let state = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == category).unwrap().id;
    gizai_core::tasks::move_to(&st.db, &st.you_id, task, &state, "").unwrap();
}

fn backend_agent(st: &gizai_lib::AppState) -> String {
    gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap().1.actor_id
}

fn worktree_dir(st: &gizai_lib::AppState, identifier: &str) -> PathBuf {
    st.data_dir.join("worktrees").join("KADE").join(identifier)
}

fn identifier(st: &gizai_lib::AppState, task: &str) -> String {
    gizai_core::tasks::get(&st.db, task).unwrap().identifier
}

fn listed(repo: &Path, wt: &Path) -> bool {
    git_out(repo, &["worktree", "list", "--porcelain"]).lines().any(|l| l == format!("worktree {}", wt.display()))
}

#[tokio::test]
async fn a_new_worktree_is_prepared_before_the_agent_starts() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let log = tmp.path().join("setup.log");
    set_prepare(&st, &[".env", "not-in-main/"], true, &format!("echo \"$PWD env=$(cat .env)\" >> '{}'", log.display()));

    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded", "a copy path the main checkout lacks doesn't stop the start");
    let wt = worktree_dir(&st, &identifier(&st, &task));
    assert_eq!(std::fs::read_to_string(wt.join(".env")).unwrap(), "APP_KEY=from-main\n");
    assert!(!wt.join("not-in-main").exists());
    assert_eq!(std::fs::read_to_string(&log).unwrap(), format!("{} env=APP_KEY=from-main\n", wt.display()),
               "the setup command ran once, in the worktree, after the copy");
    assert_eq!(gizai_agents::worktree::unprepared(&wt), None, "prepared: the note is gone");

    // the card's own worktree is not prepared again at its next start
    gizai_lib::runs::run_once(&st, &task, Some(backend_agent(&st)), Some(FAKE.into())).await.unwrap();
    assert_eq!(std::fs::read_to_string(&log).unwrap().lines().count(), 1);
}

#[tokio::test]
async fn a_failing_setup_command_holds_the_card_blocked_and_is_not_a_failed_run() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    set_prepare(&st, &[], true, "echo migrating; echo 'could not find driver' >&2; exit 4");

    let err = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap_err();
    assert!(err.contains("Preparing its worktree failed") && err.contains("exit code 4"), "{err}");
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!(t.hold.as_deref(), Some("blocked"));
    let reason = t.hold_reason.unwrap_or_default();
    assert!(reason.contains("exit code 4") && reason.contains("could not find driver") && reason.contains("migrating"), "{reason}");
    assert_eq!((t.state_name.as_str(), t.fail_count), ("To do", 0), "not a failed run");
    assert!(gizai_core::runs::list_for_task(&st.db, &task).unwrap().is_empty(), "no run started");
    assert!(gizai_lib::runs::live(&st).is_empty());
    let wt = worktree_dir(&st, &t.identifier);
    assert!(gizai_agents::worktree::unprepared(&wt).is_some(), "still to prepare");

    // fixed, and the hold cleared: the next start prepares again, then the agent runs
    set_prepare(&st, &[], true, "true");
    gizai_core::tasks::update(&st.db, &st.you_id, &task, gizai_core::model::TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded");
    assert_eq!(gizai_agents::worktree::unprepared(&wt), None);
}

#[tokio::test]
async fn a_failing_setup_on_a_start_by_the_queue_puts_the_card_on_hold_and_the_agent_moves_on() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    set_prepare(&st, &[], true, "echo 'composer: command not found' >&2; exit 127");
    let agent = gizai_core::team::all_agents(&st.db).unwrap()[0].1.clone();
    assert!(gizai_lib::runs::pull(&st).await.is_empty());
    let t = gizai_core::tasks::get(&st.db, &task).unwrap();
    assert_eq!(t.hold.as_deref(), Some("blocked"));
    assert!(t.hold_reason.as_deref().unwrap_or("").contains("command not found"), "{:?}", t.hold_reason);
    assert_eq!(t.fail_count, 0);
    assert!(gizai_core::runs::list_for_task(&st.db, &task).unwrap().is_empty());
    assert_eq!(gizai_core::workflow::next_task_for(&st.db, &agent.actor_id).unwrap(), None, "the agent doesn't retry it in a loop");
}

#[tokio::test]
async fn a_new_card_takes_over_the_worktree_of_a_done_then_a_cancelled_card() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let log = tmp.path().join("setup.log");
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    set_prepare(&st, &[".env"], true, &format!("echo \"$PWD\" >> '{}'", log.display()));
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    let a_wt = worktree_dir(&st, &identifier(&st, &a));
    let a_branch = gizai_core::tasks::get(&st.db, &a).unwrap().branch.unwrap();
    assert_eq!(gizai_agents::worktree::uncommitted(&a_wt).unwrap(), 0, "a run leaves nothing untracked behind");
    // a warm build, and a file of the finished card that isn't kept
    std::fs::create_dir_all(a_wt.join("target/debug")).unwrap();
    std::fs::write(a_wt.join("target/debug/warm.rlib"), "compiled").unwrap();
    std::fs::write(a_wt.join("agent.log"), "the old card's\n").unwrap();
    finish(&st, &a, "done");

    let b = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s = gizai_lib::runs::run_once(&st, &b, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded");
    let b_wt = worktree_dir(&st, &identifier(&st, &b));
    let run = gizai_core::runs::list_for_task(&st.db, &b).unwrap().remove(0);
    assert_eq!(run.worktree_path.as_deref(), Some(b_wt.to_str().unwrap()));
    assert!(!a_wt.exists(), "the Done card's folder moved to the new card");
    assert!(listed(&repo, &b_wt) && !listed(&repo, &a_wt));
    let b_branch = gizai_core::tasks::get(&st.db, &b).unwrap().branch.unwrap();
    assert_ne!(b_branch, a_branch);
    assert_eq!(git_out(&b_wt, &["branch", "--show-current"]), b_branch, "on its own branch");
    assert_eq!(git_out(&b_wt, &["rev-parse", "HEAD"]), git_out(&repo, &["rev-parse", "main"]), "from main");
    assert_eq!(std::fs::read_to_string(b_wt.join("target/debug/warm.rlib")).unwrap(), "compiled", "the build stays warm");
    assert_eq!(std::fs::read_to_string(b_wt.join(".env")).unwrap(), "APP_KEY=from-main\n");
    assert!(!b_wt.join("agent.log").exists(), "nothing else of the finished card is left");
    assert_eq!(gizai_agents::worktree::uncommitted(&b_wt).unwrap(), 0);
    assert_eq!(gizai_agents::worktree::unprepared(&b_wt), None);
    assert_eq!(std::fs::read_to_string(&log).unwrap(), format!("{}\n{}\n", a_wt.display(), b_wt.display()),
               "the setup command runs again in a taken-over worktree");
    assert!(git_out(&repo, &["branch", "--list", &a_branch]).contains(&a_branch), "the Done card's branch stays");

    // a Cancelled card's worktree is taken over too
    finish(&st, &b, "cancelled");
    let c = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &c, None, Some(FAKE.into())).await.unwrap();
    let c_wt = worktree_dir(&st, &identifier(&st, &c));
    assert!(!b_wt.exists() && c_wt.join("target/debug/warm.rlib").is_file());
}

#[tokio::test]
async fn the_worktree_of_a_card_that_is_not_finished_is_never_taken_over() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(gizai_core::tasks::get(&st.db, &a).unwrap().state_name, "Testing");
    let a_wt = worktree_dir(&st, &identifier(&st, &a));
    let b = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &b, None, Some(FAKE.into())).await.unwrap();
    let b_wt = worktree_dir(&st, &identifier(&st, &b));
    assert!(a_wt.is_dir() && b_wt.is_dir() && listed(&repo, &a_wt) && listed(&repo, &b_wt), "two worktrees side by side");
    let a_branch = gizai_core::tasks::get(&st.db, &a).unwrap().branch.unwrap();
    assert_eq!(git_out(&a_wt, &["branch", "--show-current"]), a_branch, "the unfinished card keeps its own");
}

#[tokio::test]
async fn a_done_cards_worktree_with_uncommitted_work_is_not_taken_over() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    let a_wt = worktree_dir(&st, &identifier(&st, &a));
    std::fs::write(a_wt.join("notes.md"), "unsaved\n").unwrap();
    finish(&st, &a, "done");
    let b = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &b, None, Some(FAKE.into())).await.unwrap();
    assert!(a_wt.join("notes.md").is_file(), "left as it was");
    assert!(worktree_dir(&st, &identifier(&st, &b)).is_dir(), "the new card got a worktree of its own");
}

#[tokio::test]
async fn settings_data_lists_and_removes_the_worktrees_of_finished_cards() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let ids: Vec<String> = (0..3).map(|_| gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend")).collect();
    // Each card is started here: To do is Manual, so the queue doesn't start the next one on its own as a run ends.
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let todo = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "ready").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &todo, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    for t in &ids {
        gizai_lib::runs::run_once(&st, t, None, Some(FAKE.into())).await.unwrap();
    }
    let (done, cancelled, testing) = (&ids[0], &ids[1], &ids[2]);
    let done_wt = worktree_dir(&st, &identifier(&st, done));
    std::fs::create_dir_all(done_wt.join("target")).unwrap();
    std::fs::write(done_wt.join("target/big.bin"), vec![7u8; 200_000]).unwrap();
    finish(&st, done, "done");
    finish(&st, cancelled, "cancelled");
    let cancelled_wt = worktree_dir(&st, &identifier(&st, cancelled));
    std::fs::write(cancelled_wt.join("draft.md"), "unsaved\n").unwrap();

    let list = gizai_lib::worktrees::list(&st).unwrap();
    let mut shown: Vec<&str> = list.iter().map(|w| w.task_id.as_str()).collect();
    shown.sort();
    let mut want = vec![done.as_str(), cancelled.as_str()];
    want.sort();
    assert_eq!(shown, want, "only Done and Cancelled cards, not the one in Testing");
    let d = list.iter().find(|w| &w.task_id == done).unwrap();
    assert_eq!((d.category.as_str(), d.path.as_str(), d.live, d.uncommitted), ("done", done_wt.to_str().unwrap(), false, 0));
    assert!(d.bytes >= 200_000, "its disk use counts its files: {}", d.bytes);
    let c = list.iter().find(|w| &w.task_id == cancelled).unwrap();
    assert_eq!((c.category.as_str(), c.uncommitted), ("cancelled", 1));

    let out = gizai_lib::worktrees::remove(&st, &[done.clone(), cancelled.clone(), testing.clone()]).unwrap();
    let by = |id: &str| out.iter().find(|r| r.task_id == id).unwrap();
    assert!(by(done).removed, "{:?}", by(done));
    assert!(!done_wt.exists(), "its folder is gone");
    assert!(!listed(&repo, &done_wt), "git worktree list no longer shows it");
    assert!(!by(cancelled).removed && cancelled_wt.join("draft.md").is_file(), "uncommitted work stays: {:?}", by(cancelled));
    assert!(!by(testing).removed && worktree_dir(&st, &identifier(&st, testing)).is_dir(), "a card in Testing keeps its worktree");
    assert!(by(testing).note.contains("no worktree of a Done or Cancelled card"), "{:?}", by(testing));
    let left: Vec<String> = gizai_lib::worktrees::list(&st).unwrap().into_iter().map(|w| w.task_id).collect();
    assert_eq!(left, [cancelled.clone()]);
}

/// A temp folder by its real path, the way git and Gizai report it: on macOS /var/folders is /private/var/folders, and on
/// Windows TEMP can be a short name (RUNNER~1) that git gives in full. Without the \\?\ that canonicalize puts before a
/// Windows drive.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let base = std::path::PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    tempfile::tempdir_in(base).unwrap()
}
