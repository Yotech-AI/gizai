//! GA-30: a card's worktree is prepared before its agent starts, takes over a finished card's worktree, and Settings →
//! Data lists and removes the worktrees of finished cards. GA-138: a take-over empties a build folder over the limit,
//! and Settings → Data lists a worktree's uncommitted files and can Remove anyway after stashing them. Runs use the fake
//! Claude Code, never the real one.
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

// ---- GA-138: a take-over empties a build folder over the limit; Settings → Data lists a worktree's uncommitted files
// and can Remove anyway, after putting them in git's stash ----

fn kade(st: &gizai_lib::AppState) -> gizai_core::model::Project {
    gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap()
}

/// Card `task` takes over a finished card's worktree, as a run's start does, with `limit` for the build folder.
fn take_over(st: &gizai_lib::AppState, repo: &Path, task: &str, limit: u64) -> Option<gizai_agents::worktree::Worktree> {
    let t = gizai_core::tasks::get(&st.db, task).unwrap();
    gizai_lib::worktrees::take_over(st, &kade(st), &t, repo, &st.data_dir.join("worktrees").join("KADE"), "main", limit)
}

/// The cleanup notes in the card's activity, oldest first.
fn cleanups(st: &gizai_lib::AppState, task: &str) -> Vec<String> {
    gizai_core::tasks::activity(&st.db, task).unwrap().into_iter()
        .filter_map(|e| e.diff.get("cleanup").and_then(|c| c.as_str()).map(String::from)).collect()
}

/// `git stash list` of the repository, one entry per line as "On <branch>: <message>".
fn stashes(repo: &Path) -> String {
    git_out(repo, &["stash", "list", "--format=%gs"])
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", msg]);
}

/// Card `a` runs once (so it has its worktree and branch), gets 200,000 bytes in target/debug, then is Done.
async fn done_with_a_build(st: &gizai_lib::AppState, a: &str) -> PathBuf {
    gizai_lib::runs::run_once(st, a, None, Some(FAKE.into())).await.unwrap();
    let wt = worktree_dir(st, &identifier(st, a));
    std::fs::create_dir_all(wt.join("target/debug")).unwrap();
    std::fs::write(wt.join("target/debug/big.bin"), vec![7u8; 200_000]).unwrap();
    finish(st, a, "done");
    wt
}

#[tokio::test]
async fn a_take_over_empties_a_build_folder_over_the_limit_and_the_card_says_so() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    // a copied folder inside target/ would keep most of the build, so it goes too
    set_prepare(&st, &[".env", "./target/debug"], false, "");
    let a_wt = done_with_a_build(&st, &a).await;
    // installed dependencies are ignored, as in a real project
    use std::io::Write;
    let mut exclude = std::fs::OpenOptions::new().append(true).create(true).open(repo.join(".git/info/exclude")).unwrap();
    writeln!(exclude, "\nnode_modules/").unwrap();
    std::fs::create_dir_all(a_wt.join("node_modules/left-pad")).unwrap();
    std::fs::write(a_wt.join("node_modules/left-pad/index.js"), "module.exports = 1;\n").unwrap();
    let bytes = gizai_lib::worktrees::disk_use(&a_wt.join("target"));
    assert!(bytes >= 200_000, "{bytes}");

    let b = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let wt = take_over(&st, &repo, &b, 100_000).expect("B takes over A's worktree");
    let b_wt = worktree_dir(&st, &identifier(&st, &b));
    assert_eq!(wt.path, b_wt);
    assert!(wt.reused.is_some(), "taken over, not made anew");
    assert!(!a_wt.exists() && listed(&repo, &b_wt) && !listed(&repo, &a_wt), "A's folder moved to B");
    assert!(!b_wt.join("target").exists(), "B starts with an empty build folder");
    assert_eq!(std::fs::read_to_string(b_wt.join("node_modules/left-pad/index.js")).unwrap(), "module.exports = 1;\n",
               "only the build folder goes: installed dependencies stay");
    assert_eq!(std::fs::read_to_string(b_wt.join(".env")).unwrap(), "APP_KEY=from-main\n", "a copied file stays");
    let said = format!("took over {}'s worktree and emptied its build folder (target/, {}): it was over 97.7 KB",
                       identifier(&st, &a), gizai_lib::worktrees::size(bytes));
    let last = gizai_core::tasks::activity(&st.db, &b).unwrap().pop().unwrap();
    assert_eq!((last.actor_name, last.diff), (None, serde_json::json!({"cleanup": said})), "B's activity says so, by Gizai");
    assert!(cleanups(&st, &a).is_empty(), "the note is on the card that took over");

    // B's first run works in that worktree
    let s = gizai_lib::runs::run_once(&st, &b, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded");
    let run = gizai_core::runs::list_for_task(&st.db, &b).unwrap().remove(0);
    assert_eq!(run.worktree_path.as_deref(), Some(b_wt.to_str().unwrap()));
}

#[tokio::test]
async fn a_take_over_keeps_a_build_folder_at_the_limit_and_says_nothing() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let a_wt = done_with_a_build(&st, &a).await;
    let bytes = gizai_lib::worktrees::disk_use(&a_wt.join("target"));

    let b = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let wt = take_over(&st, &repo, &b, bytes).expect("B takes over A's worktree");
    assert!(!a_wt.exists());
    assert_eq!(std::fs::read(wt.path.join("target/debug/big.bin")).unwrap().len(), 200_000, "the build stays warm");
    assert!(cleanups(&st, &b).is_empty(), "nothing to say: {:?}", cleanups(&st, &b));
}

#[tokio::test]
async fn the_take_over_plan_drops_every_spelling_of_target_only_above_the_limit() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    set_prepare(&st, &[".env", "target", "target/debug/", "./target/release", "target-old/"], false, "");
    let wt = tmp.path().join("finished");
    std::fs::create_dir_all(wt.join("target/debug")).unwrap();
    std::fs::write(wt.join("target/debug/big.bin"), vec![7u8; 200_000]).unwrap();
    let bytes = gizai_lib::worktrees::disk_use(&wt.join("target"));
    let all = gizai_lib::worktrees::keep_on_reuse(&kade(&st));

    let at = gizai_lib::worktrees::take_over_plan(&kade(&st), &wt, bytes);
    assert_eq!((at.keep, at.emptied), (all.clone(), None), "at the limit everything is kept");
    let over = gizai_lib::worktrees::take_over_plan(&kade(&st), &wt, bytes - 1);
    assert_eq!(over.emptied, Some(bytes));
    assert_eq!(over.keep, [".env", "target-old/", "vendor/", "node_modules/"], "one byte over, target/ and folders in it go");
    assert_eq!(gizai_lib::worktrees::take_over_plan(&kade(&st), &tmp.path().join("no-build"), 0).emptied, None,
               "a worktree without target/ has nothing to empty");

    // a target/ that is a link to a build elsewhere stays: emptying it frees nothing here
    let linked = tmp.path().join("linked");
    std::fs::create_dir(&linked).unwrap();
    std::os::unix::fs::symlink(wt.join("target"), linked.join("target")).unwrap();
    let plan = gizai_lib::worktrees::take_over_plan(&kade(&st), &linked, 0);
    assert_eq!((plan.keep, plan.emptied), (all, None));
}

#[test]
fn the_build_limit_is_20_gb_and_sizes_read_like_the_card_says() {
    use gizai_lib::worktrees::{size, BUILD_LIMIT};
    assert_eq!(BUILD_LIMIT, 20 * 1024 * 1024 * 1024);
    assert_eq!(size(BUILD_LIMIT), "20 GB");
    assert_eq!(size(25_125_558_682), "23.4 GB");
    assert_eq!(size(512 * 1024), "512 KB");
    assert_eq!(size(100_000), "97.7 KB");
    assert_eq!(size(200_704), "196 KB");
    assert_eq!(size(3 * 1024 * 1024 + 1024 * 1024 / 2), "3.5 MB");
    assert_eq!(size(1023), "1023 B");
    assert_eq!(size(1024), "1 KB");
}

#[tokio::test]
async fn settings_data_lists_a_finished_worktrees_uncommitted_files_the_first_ten() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    std::fs::write(repo.join("readme.md"), "Kade\n").unwrap();
    std::fs::write(repo.join("old.txt"), "old\n").unwrap();
    commit_all(&repo, "two files");
    let ids: Vec<String> = (0..2).map(|_| gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend")).collect();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let todo = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "ready").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &todo, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    for t in &ids {
        gizai_lib::runs::run_once(&st, t, None, Some(FAKE.into())).await.unwrap();
    }
    // finished only now: a card that starts after one is Done takes over its worktree
    ids.iter().for_each(|t| finish(&st, t, "done"));
    let (left, clean) = (&ids[0], &ids[1]);
    let wt = worktree_dir(&st, &identifier(&st, left));
    std::fs::write(wt.join("readme.md"), "Kade, changed\n").unwrap();
    std::fs::remove_file(wt.join("old.txt")).unwrap();
    for i in 1..=11 {
        std::fs::write(wt.join(format!("new-{i:02}.md")), "draft\n").unwrap();
    }
    std::fs::create_dir_all(wt.join("drafts/deep")).unwrap();
    std::fs::write(wt.join("drafts/deep/ünï cödé.md"), "draft\n").unwrap();

    let list = gizai_lib::worktrees::list(&st).unwrap();
    let w = list.iter().find(|w| &w.task_id == left).unwrap();
    assert_eq!(w.uncommitted, 14, "every file counts: 2 tracked, 11 new files and 1 new folder");
    assert_eq!(w.changes.len(), gizai_lib::worktrees::SHOWN_CHANGES);
    assert_eq!(gizai_lib::worktrees::SHOWN_CHANGES, 10);
    let shown: Vec<(String, String)> = w.changes.iter()
        .map(|f| (f.path.clone(), serde_json::to_value(f.change).unwrap().as_str().unwrap().to_string())).collect();
    let mut want = vec![("old.txt".to_string(), "deleted".to_string()), ("readme.md".into(), "changed".into()), ("drafts/".into(), "new".into())];
    want.extend((1..=7).map(|i| (format!("new-{i:02}.md"), "new".to_string())));
    assert_eq!(shown, want, "in git status's order: changed tracked files first, then new ones; a new folder is one entry");
    let json = serde_json::to_value(w).unwrap();
    assert_eq!(json["changes"][0], serde_json::json!({"path": "old.txt", "change": "deleted"}), "as the app reads it");
    assert_eq!(json["uncommitted"], 14);

    let c = list.iter().find(|w| &w.task_id == clean).unwrap();
    assert_eq!((c.uncommitted, c.changes.len()), (0, 0));
    assert_eq!(serde_json::to_value(c).unwrap()["changes"], serde_json::json!([]));
}

#[tokio::test]
async fn remove_anyway_stashes_the_uncommitted_changes_then_removes_the_worktree_and_its_merged_branch() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    std::fs::write(repo.join("readme.md"), "Kade\n").unwrap();
    commit_all(&repo, "readme");
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    finish(&st, &a, "done");
    let (id, branch) = (identifier(&st, &a), gizai_core::tasks::get(&st.db, &a).unwrap().branch.unwrap());
    let wt = worktree_dir(&st, &id);
    std::fs::write(wt.join("readme.md"), "Kade, half done\n").unwrap();
    std::fs::write(wt.join("draft.md"), "unsaved\n").unwrap();
    std::fs::create_dir_all(wt.join("target")).unwrap();
    std::fs::write(wt.join("target/big.bin"), vec![7u8; 4096]).unwrap();
    assert_eq!(git_out(&repo, &["rev-list", "--count", &format!("main..{branch}")]), "0", "main has all its commits");

    let r = gizai_lib::worktrees::remove_anyway(&st, &a).unwrap();
    let message = format!("{id} leftover, removed in Settings");
    assert_eq!(gizai_lib::worktrees::stash_message(&id), message);
    assert!(r.removed, "{r:?}");
    assert_eq!((r.task_id.as_str(), r.identifier.as_str()), (a.as_str(), id.as_str()));
    assert_eq!(r.note, format!("put its 2 uncommitted changes in git's stash as \"{message}\", removed its worktree and deleted branch {branch}"));
    assert!(!wt.exists() && !listed(&repo, &wt), "its folder is gone");
    assert_eq!(stashes(&repo), format!("On {branch}: {message}"), "git stash list shows it");
    assert_eq!(git_out(&repo, &["branch", "--list", &branch]), "", "main had all its commits, so the branch went");
    assert_eq!(cleanups(&st, &a).last().unwrap(), &format!("{} in Settings → Data", r.note));
    assert!(gizai_lib::worktrees::list(&st).unwrap().is_empty());

    // nothing is lost: the stash gives both changes back
    git(&repo, &["stash", "apply", "--quiet", "stash@{0}"]);
    assert_eq!(std::fs::read_to_string(repo.join("readme.md")).unwrap(), "Kade, half done\n");
    assert_eq!(std::fs::read_to_string(repo.join("draft.md")).unwrap(), "unsaved\n");
}

#[tokio::test]
async fn remove_anyway_keeps_a_branch_with_commits_main_doesnt_have() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    finish(&st, &a, "done");
    let (id, branch) = (identifier(&st, &a), gizai_core::tasks::get(&st.db, &a).unwrap().branch.unwrap());
    let wt = worktree_dir(&st, &id);
    std::fs::write(wt.join("feature.rs"), "pub fn f() {}\n").unwrap();
    commit_all(&wt, "the card's own work");
    std::fs::write(wt.join("notes.md"), "unsaved\n").unwrap();

    let r = gizai_lib::worktrees::remove_anyway(&st, &a).unwrap();
    assert!(r.removed, "{r:?}");
    assert_eq!(r.note, format!("put its 1 uncommitted change in git's stash as \"{id} leftover, removed in Settings\", removed its worktree \
                                and kept branch {branch}, which has commits main doesn't"));
    assert!(!wt.exists());
    assert_eq!(stashes(&repo), format!("On {branch}: {id} leftover, removed in Settings"));
    assert_eq!(git_out(&repo, &["branch", "--list", &branch]).trim_start_matches(['*', '+', ' ']), branch, "the branch stays");
    assert_eq!(git_out(&repo, &["show", &format!("{branch}:feature.rs")]), "pub fn f() {}", "with its commit");
}

#[tokio::test]
async fn when_the_stash_fails_remove_anyway_removes_nothing_and_says_why() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    finish(&st, &a, "done");
    let (id, branch) = (identifier(&st, &a), gizai_core::tasks::get(&st.db, &a).unwrap().branch.unwrap());
    let wt = worktree_dir(&st, &id);
    std::fs::write(wt.join("draft.md"), "unsaved\n").unwrap();
    // another git command holds the repository's stash
    let common = PathBuf::from(git_out(&wt, &["rev-parse", "--path-format=absolute", "--git-common-dir"]));
    let lock = common.join("refs/stash.lock");
    std::fs::write(&lock, "").unwrap();
    let before = cleanups(&st, &a);

    let r = gizai_lib::worktrees::remove_anyway(&st, &a).unwrap();
    assert!(!r.removed, "{r:?}");
    assert!(r.note.starts_with("kept its worktree: git couldn't put its uncommitted change in the stash, so nothing was removed ("),
            "{}", r.note);
    // git 2.55 says only "Cannot save the current status" here; older ones name the lock
    assert!((r.note.ends_with("(Cannot save the current status)") || r.note.contains("stash.lock")) && !r.note.contains('\n')
            && !r.note.contains("error:"), "git's own reason, on one line: {}", r.note);
    assert!(wt.is_dir() && listed(&repo, &wt), "the worktree stays");
    assert_eq!(std::fs::read_to_string(wt.join("draft.md")).unwrap(), "unsaved\n", "with its change");
    assert!(git_out(&repo, &["branch", "--list", &branch]).contains(&branch), "and its branch");
    assert_eq!(stashes(&repo), "", "nothing went into the stash");
    assert_eq!(cleanups(&st, &a), before, "nothing happened, so the card's activity has nothing new");
    let w = gizai_lib::worktrees::list(&st).unwrap().into_iter().find(|w| w.task_id == a).unwrap();
    assert_eq!(w.uncommitted, 1, "Settings still lists it with its change");

    // the lock gone, Remove anyway works
    std::fs::remove_file(&lock).unwrap();
    let r = gizai_lib::worktrees::remove_anyway(&st, &a).unwrap();
    assert!(r.removed && !wt.exists(), "{r:?}");
    assert_eq!(stashes(&repo), format!("On {branch}: {id} leftover, removed in Settings"));
}

/// A left-over index.lock in the worktree: git 2.55 stops before it stashes anything; a git that stores the stash before
/// its `reset --hard` fails on the lock has the change in the stash already. Either way the worktree stays, the row says
/// why and no change is lost.
#[tokio::test]
async fn a_held_index_keeps_the_worktree_and_loses_no_change() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    finish(&st, &a, "done");
    let wt = worktree_dir(&st, &identifier(&st, &a));
    std::fs::write(wt.join("draft.md"), "unsaved\n").unwrap();
    std::fs::write(PathBuf::from(git_out(&wt, &["rev-parse", "--absolute-git-dir"])).join("index.lock"), "").unwrap();

    let r = gizai_lib::worktrees::remove_anyway(&st, &a).unwrap();
    assert!(!r.removed && r.note.contains("nothing was removed") && r.note.contains("index.lock"), "{r:?}");
    assert!(wt.is_dir() && listed(&repo, &wt), "the worktree stays");
    let kept = std::fs::read_to_string(wt.join("draft.md")).ok();
    let stashed = git_out(&repo, &["show", "stash@{0}^3:draft.md"]);
    assert!(kept.as_deref() == Some("unsaved\n") || stashed == "unsaved", "the change is lost: {kept:?}, stash: {:?}", stashes(&repo));
}

#[tokio::test]
async fn a_worktree_an_agent_is_working_in_is_never_removed_not_even_anyway() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let a = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &a, None, Some(FAKE.into())).await.unwrap();
    finish(&st, &a, "done");
    let wt = worktree_dir(&st, &identifier(&st, &a));
    std::fs::write(wt.join("draft.md"), "unsaved\n").unwrap();
    gizai_core::tasks::update(&st.db, &st.you_id, &a, gizai_core::model::TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let (run, finished) = gizai_lib::runs::start(&st, &a, Some(backend_agent(&st)), Some(FAKE.into()), "manual").await.unwrap();
    assert!(gizai_lib::runs::live(&st).iter().any(|l| l.task_id == a), "an agent works in it");
    let w = gizai_lib::worktrees::list(&st).unwrap().into_iter().find(|w| w.task_id == a).unwrap();
    assert!(w.live && w.uncommitted >= 1, "{w:?}");

    let r = gizai_lib::worktrees::remove_anyway(&st, &a).unwrap();
    assert_eq!((r.removed, r.note.as_str()), (false, "kept its worktree: an agent is working in it"));
    let r = gizai_lib::worktrees::remove(&st, &[a.clone()]).unwrap().remove(0);
    assert_eq!((r.removed, r.note.as_str()), (false, "kept its worktree: an agent is working in it"));
    assert!(wt.join("draft.md").is_file() && listed(&repo, &wt));
    assert_eq!(stashes(&repo), "", "nothing was stashed");

    gizai_lib::runs::stop(&st, &run);
    finished.await.unwrap();
    gizai_lib::runs::stop_all(&st, std::time::Duration::from_secs(10)).await;
}

#[tokio::test]
async fn remove_all_still_skips_a_worktree_with_uncommitted_changes_and_a_clean_one_goes_as_before() {
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let ids: Vec<String> = (0..3).map(|_| gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend")).collect();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let todo = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "ready").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &todo, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    for t in &ids {
        gizai_lib::runs::run_once(&st, t, None, Some(FAKE.into())).await.unwrap();
    }
    ids.iter().for_each(|t| finish(&st, t, "done"));
    let (left, clean, also_clean) = (&ids[0], &ids[1], &ids[2]);
    let left_wt = worktree_dir(&st, &identifier(&st, left));
    std::fs::write(left_wt.join("draft.md"), "unsaved\n").unwrap();

    // Remove all: never by force, never a stash
    let out = gizai_lib::worktrees::remove(&st, &[left.clone(), clean.clone()]).unwrap();
    let by = |id: &str| out.iter().find(|r| r.task_id == id).unwrap().clone();
    assert!(!by(left).removed && left_wt.join("draft.md").is_file(), "{:?}", by(left));
    assert!(by(left).note.contains("it has uncommitted changes"), "it says why: {:?}", by(left));
    assert_eq!(stashes(&repo), "", "Remove all doesn't stash");
    let clean_branch = gizai_core::tasks::get(&st.db, clean).unwrap().branch.unwrap();
    assert_eq!(by(clean).note, format!("removed its worktree and deleted branch {clean_branch}"));
    assert!(by(clean).removed && !worktree_dir(&st, &identifier(&st, clean)).exists());

    // Remove anyway on a worktree without uncommitted changes removes it just like Remove: no stash, the same words
    let also_branch = gizai_core::tasks::get(&st.db, also_clean).unwrap().branch.unwrap();
    let r = gizai_lib::worktrees::remove_anyway(&st, also_clean).unwrap();
    assert!(r.removed && !worktree_dir(&st, &identifier(&st, also_clean)).exists(), "{r:?}");
    assert_eq!(r.note, format!("removed its worktree and deleted branch {also_branch}"));
    assert_eq!(stashes(&repo), "");

    // a card without a finished card's worktree
    let r = gizai_lib::worktrees::remove_anyway(&st, clean).unwrap();
    assert_eq!((r.removed, r.note.as_str()), (false, "has no worktree of a Done or Cancelled card"));
}

/// A temp folder by its real path, the way git and Gizai report it: on macOS /var/folders is /private/var/folders, and on
/// Windows TEMP can be a short name (RUNNER~1) that git gives in full. Without the \\?\ that canonicalize puts before a
/// Windows drive.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let base = std::path::PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    tempfile::tempdir_in(base).unwrap()
}
