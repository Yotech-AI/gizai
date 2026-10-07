//! GA-14: a run saves the commit it ended at (`runs.head_sha`), and Gizai lists the commits the run made for the Runs
//! tab. Runs use the fake Claude Code, never the real one: FAKE_COMMIT_TWICE in the card makes it commit twice.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn rev(dir: &Path, r: &str) -> String {
    String::from_utf8(Command::new("git").args(["rev-parse", r]).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    repo
}

fn describe(st: &gizai_lib::AppState, task: &str, text: &str) {
    gizai_core::tasks::update(&st.db, &st.you_id, task, gizai_core::model::TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

fn only_run(st: &gizai_lib::AppState, task: &str) -> gizai_core::model::Run {
    let mut runs = gizai_core::runs::list_for_task(&st.db, task).unwrap();
    assert_eq!(runs.len(), 1);
    runs.remove(0)
}

fn subjects(list: &[gizai_agents::worktree::Commit]) -> Vec<&str> { list.iter().map(|c| c.subject.as_str()).collect() }

#[tokio::test]
async fn a_run_that_commits_twice_shows_two_commits_with_their_subjects() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    describe(&st, &task, "Make two commits. FAKE_COMMIT_TWICE");

    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.status, "succeeded");
    let run = only_run(&st, &task);
    let wt = PathBuf::from(run.worktree_path.clone().unwrap());
    let (base, head) = (run.base_sha.clone().unwrap(), run.head_sha.clone().expect("head_sha saved when the run finished"));
    assert_eq!(head, rev(&wt, "HEAD"), "the commit its worktree ended at");
    assert_ne!(head, base);
    assert_eq!(base, rev(&repo, "main"), "it started at main");

    let list = gizai_lib::runs::commits(&st, &run.id).unwrap();
    assert_eq!(list.len(), 2, "2 commits");
    assert_eq!(subjects(&list), ["First change", "Second change"], "oldest first, with their subjects");
    assert_eq!((list[0].sha.as_str(), list[1].sha.as_str()), (rev(&wt, "HEAD~1").as_str(), head.as_str()));

    // what the Runs tab gets: camelCase JSON, full ids
    let json = serde_json::to_value(&list[1]).unwrap();
    assert_eq!(json, serde_json::json!({ "sha": head, "subject": "Second change" }));
    let run_json = serde_json::to_value(&run).unwrap();
    assert_eq!(run_json["headSha"], serde_json::json!(head));
}

#[tokio::test]
async fn a_run_that_made_no_commits_ends_where_it_started() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = only_run(&st, &task);
    assert!(run.head_sha.is_some());
    assert_eq!(run.head_sha, run.base_sha);
    assert!(gizai_lib::runs::commits(&st, &run.id).unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_run_and_a_stopped_run_save_where_they_ended_too() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");

    describe(&st, &task, "FAKE_CRASH");
    assert_eq!(gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap().status, "failed");
    let failed = only_run(&st, &task);
    let wt = PathBuf::from(failed.worktree_path.clone().unwrap());
    assert_eq!(failed.head_sha.as_deref(), Some(rev(&wt, "HEAD").as_str()));

    describe(&st, &task, "FAKE_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, Some(FAKE.into()), "manual").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(gizai_core::runs::get(&st.db, &run_id).unwrap().head_sha, None, "nothing yet while it runs");
    git(&wt, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "Work before Stop"]);
    gizai_lib::runs::stop(&st, &run_id);
    assert_eq!(done.await.unwrap().status, "cancelled");
    let stopped = gizai_core::runs::get(&st.db, &run_id).unwrap();
    assert_eq!(stopped.head_sha.as_deref(), Some(rev(&wt, "HEAD").as_str()));
    assert_eq!(subjects(&gizai_lib::runs::commits(&st, &run_id).unwrap()), ["Work before Stop"]);
}

#[tokio::test]
async fn a_claude_that_cannot_start_still_saves_where_the_run_ended() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    // executable, so Gizai accepts the path, but its interpreter doesn't exist, so it can't be spawned
    let bad = tmp.path().join("claude");
    std::fs::write(&bad, "#!/nonexistent/interpreter\n").unwrap();
    let mut perms = std::fs::metadata(&bad).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&bad, perms).unwrap();

    assert!(gizai_lib::runs::run_once(&st, &task, None, Some(bad.to_string_lossy().into())).await.is_err());
    let run = only_run(&st, &task);
    assert_eq!(run.status, "failed");
    assert!(run.head_sha.is_some());
    assert_eq!(run.head_sha, run.base_sha);
}

#[tokio::test]
async fn a_card_run_a_previous_gizai_left_running_saves_where_it_ended_at_start_up() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let first = only_run(&st, &task);
    let wt = first.worktree_path.clone().unwrap();
    // a run Gizai was in the middle of when it quit: in the database, not finished
    let left = gizai_core::runs::create(&st.db, &first.agent_id, &task, "backend", "S2", &wt, &wt, first.branch.as_deref().unwrap(),
                                        tmp.path().join("left.jsonl").to_str().unwrap()).unwrap();
    gizai_core::runs::set_base_sha(&st.db, &left, &rev(Path::new(&wt), "HEAD")).unwrap();
    git(Path::new(&wt), &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "Left behind"]);
    drop(st);

    let st = gizai_lib::test_state(tmp.path());
    let run = gizai_core::runs::get(&st.db, &left).unwrap();
    assert_eq!((run.status.as_str(), run.error.as_deref()), ("failed", Some("interrupted")));
    assert_eq!(run.head_sha.as_deref(), Some(rev(Path::new(&wt), "HEAD").as_str()));
    assert_eq!(subjects(&gizai_lib::runs::commits(&st, &left).unwrap()), ["Left behind"]);
}

#[tokio::test]
async fn the_commits_are_read_from_the_projects_repository_once_the_worktree_is_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    describe(&st, &task, "FAKE_COMMIT_TWICE");
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = only_run(&st, &task);
    git(&repo, &["worktree", "remove", "--force", run.worktree_path.as_deref().unwrap()]);
    assert!(!Path::new(run.worktree_path.as_deref().unwrap()).exists());
    assert_eq!(subjects(&gizai_lib::runs::commits(&st, &run.id).unwrap()), ["First change", "Second change"]);
}

#[tokio::test]
async fn a_run_without_a_saved_end_says_so_instead_of_listing_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let run = only_run(&st, &task);
    // like a run from before Gizai saved where runs end
    st.db.write(None, |w| { w.conn().execute("UPDATE runs SET head_sha = NULL WHERE id = ?1", [&run.id])?; Ok(()) }).unwrap();
    let err = gizai_lib::runs::commits(&st, &run.id).unwrap_err();
    assert!(err.contains("didn't save where this run started and ended"), "{err}");
}

// GA-40: GA-14 merged with GA-15 (stopping on quit) and GA-3 (coding CLIs). Where they meet: a run stopped because
// Gizai quit and a run on another coding CLI save where they ended too.

#[tokio::test]
async fn a_run_stopped_because_gizai_quit_saves_where_it_ended() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    describe(&st, &task, "FAKE_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, Some(FAKE.into()), "manual").await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let wt = PathBuf::from(gizai_core::runs::get(&st.db, &run_id).unwrap().worktree_path.unwrap());
    git(&wt, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "Work before quitting"]);

    assert_eq!(gizai_lib::runs::stop_all(&st, Duration::from_secs(12)).await, 1);
    let s = done.await.unwrap();
    assert_eq!((s.status.as_str(), s.error.as_deref()), ("cancelled", Some(gizai_lib::runs::STOPPED_BY_QUIT)));
    let run = gizai_core::runs::get(&st.db, &run_id).unwrap();
    assert_eq!(run.error.as_deref(), Some(gizai_lib::runs::STOPPED_BY_QUIT));
    assert_eq!(run.head_sha.as_deref(), Some(rev(&wt, "HEAD").as_str()));
    assert_eq!(subjects(&gizai_lib::runs::commits(&st, &run_id).unwrap()), ["Work before quitting"]);
}

#[tokio::test]
async fn a_run_on_another_coding_cli_saves_its_cli_and_where_it_ended() {
    const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let mut clis = gizai_core::clis::list(&st.db).unwrap();
    clis.push(gizai_core::clis::Cli { name: "Codex".into(), kind: "codex".into(), command: FAKE_CLI.into(), env: vec!["FAKE_KIND=codex".into()],
                                      ..Default::default() });
    let codex = gizai_lib::clis::save(&st, clis).unwrap().into_iter().find(|c| c.cli.name == "Codex").unwrap().cli.id;
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, gizai_core::model::AgentInput { name: agent.name.clone(),
        role_key: "backend".into(), adapter: codex.clone(), ..Default::default() }).unwrap();

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.status, "succeeded");
    let run = only_run(&st, &task);
    let wt = PathBuf::from(run.worktree_path.clone().unwrap());
    assert_eq!(run.adapter.as_deref(), Some(codex.as_str()), "the CLI it ran on");
    assert_eq!(run.head_sha.as_deref(), Some(rev(&wt, "HEAD").as_str()), "where it ended");
    assert_eq!(run.head_sha, run.base_sha, "the fake Codex made no commits");
    assert!(gizai_lib::runs::commits(&st, &run.id).unwrap().is_empty());
    let json = serde_json::to_value(&run).unwrap();
    assert_eq!((json["adapter"].clone(), json["headSha"].clone()), (serde_json::json!(codex), serde_json::json!(run.head_sha)));
}
