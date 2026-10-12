//! GA-111: a person's card end to end (contract C4): Open in editor (`hand::start`), the card bar (`hand::card`), Commit
//! (`hand::commit`), Push (`hand::push`) and Hand over (`hand::over`), with the real queue and fake CLIs (never the real
//! ones), and "GitHub" as a local bare repository: the project's clone sends https://github.com/acme/ there and allows
//! no transport but local files, so nothing here can reach the real GitHub. The core rules are in
//! crates/gizai-core/tests/by_hand_test.rs.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_core::model::TaskPatch;
use gizai_core::{settings, tasks, team};
use gizai_lib::{editor, hand, runs};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const LINK: &str = "https://github.com/acme/shop";
const BRANCH: &str = "gizai/kade-1-export-invoices-as-csv";

/// A temp folder by its real path, the way git and Gizai report it (the Editor's folder ids are canonical).
fn real_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing ("Text file busy").
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// "GitHub" (a bare repository under tmp/github) and the project's local clone of https://github.com/acme/shop, as in
/// push_after_run_test.
fn github(tmp: &Path) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    std::fs::write(src.join("README.md"), "# Shop\n").unwrap();
    git(&src, &["add", "-A"]);
    git(&src, &["commit", "-q", "-m", "init"]);
    let bare = tmp.join("github/acme/shop");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    git(&bare, &["config", "core.hooksPath", bare.join("hooks").to_str().unwrap()]);
    let local = tmp.join("local");
    let rewrite = format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display());
    git(tmp, &["clone", "-q", "-c", &rewrite, "-c", "protocol.allow=never", "-c", "protocol.file.allow=always", LINK, local.to_str().unwrap()]);
    (bare, local)
}

struct Card { st: gizai_lib::AppState, task: String, bare: PathBuf, tmp: PathBuf }

impl Card {
    fn task(&self) -> gizai_core::model::Task { tasks::get(&self.st.db, &self.task).unwrap() }
    /// Where a run (and Open in editor) makes the card's worktree.
    fn wt(&self) -> PathBuf { self.st.data_dir.join("worktrees/KADE/KADE-1") }
    fn agent(&self, name: &str) -> String {
        let team = team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap();
        team.members.iter().find(|m| m.name == name).unwrap().actor_id.clone()
    }
    fn assign(&self, to: &str) {
        tasks::update(&self.st.db, &self.st.you_id, &self.task, TaskPatch { assignee_id: Some(to.into()), ..Default::default() }).unwrap();
    }
    fn you(&self) -> String {
        gizai_core::users::list(&self.st.db).unwrap().into_iter().find(|p| p.id == self.st.you_id).unwrap().name
    }
    fn runs(&self) -> Vec<gizai_core::model::Run> { gizai_core::runs::list_for_task(&self.st.db, &self.task).unwrap() }
}

/// Card KADE-1 "Export invoices as CSV" of a project linked to https://github.com/acme/shop, in To do (Auto, the Backend
/// Agent on To do and In progress) and assigned to you. Every agent's CLI is the fake, so a run the queue wrongly
/// started would show in the card's runs without reaching the real Claude Code.
fn yours(tmp: &Path) -> Card {
    let c = linked(tmp);
    c.assign(&c.st.you_id.clone());
    c
}

/// As `yours`, but still assigned to the Backend Agent.
fn linked(tmp: &Path) -> Card {
    let st = gizai_lib::test_state(tmp);
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let (bare, local) = github(tmp);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(LINK.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    Card { st, task, bare, tmp: tmp.to_path_buf() }
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn open_in_editor_makes_the_worktree_pins_the_card_to_you_and_the_queue_never_takes_it() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());

    let h = hand::start(&c.st, &c.task).await.unwrap();
    let folder = editor::folder(&c.st, &c.wt().display().to_string()).unwrap();
    assert_eq!(h, hand::HandCard { task_id: c.task.clone(), folder: folder.id.clone(), branch: BRANCH.into(), changed: 0, unpushed: 0 });
    assert_eq!(folder.id, c.wt().canonicalize().unwrap().display().to_string(), "the folder is the worktree's EditorFolder id");
    assert_eq!((folder.task_id.as_deref(), folder.read_only), (Some(c.task.as_str()), false), "yours to change");
    assert_eq!(git(&c.wt(), &["branch", "--show-current"]), BRANCH, "made from main, on the card's branch");
    assert_eq!(std::fs::read_to_string(c.wt().join("README.md")).unwrap(), "# Shop\n");
    let t = c.task();
    assert_eq!((t.state_name.as_str(), t.by_hand, t.branch.as_deref(), t.assignee_id.as_deref()),
               ("In progress", true, Some(BRANCH), Some(c.st.you_id.as_str())));
    assert_eq!(serde_json::to_value(&h).unwrap(), serde_json::json!({"taskId": c.task, "folder": folder.id, "branch": BRANCH, "changed": 0, "unpushed": 0}),
               "contract C4's HandCard");
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap(), h, "the card bar reads the same");

    // the queue, with agents running and the Backend Agent on In progress (Auto): nothing starts on it
    let started = runs::pull(&c.st).await;
    assert!(started.iter().all(|(t, _)| *t != c.task), "the queue started a run on a card done by hand");
    // back in To do (Auto) it stays yours too
    let todo = team::get(&c.st.db, &team::list(&c.st.db).unwrap()[0].id).unwrap().states.into_iter().find(|s| s.category == "ready").unwrap().id;
    tasks::move_to(&c.st.db, &c.st.you_id, &c.task, &todo, "a1").unwrap();
    let started = runs::pull(&c.st).await;
    assert!(started.iter().all(|(t, _)| *t != c.task));
    assert!(c.runs().is_empty(), "no agent's run on it: {:?}", c.runs());

    // Open in editor again: the same worktree, nothing new
    let again = hand::start(&c.st, &c.task).await.unwrap();
    assert_eq!((again.folder, again.branch), (h.folder, h.branch));
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn commit_and_push_send_the_branch_where_open_pull_request_pushes() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    hand::start(&c.st, &c.task).await.unwrap();
    std::fs::write(c.wt().join("README.md"), "# Shop\n\nCSV export.\n").unwrap();
    std::fs::write(c.wt().join("export.csv"), "id,total\n").unwrap();
    std::fs::write(c.wt().join(".gitignore"), "*.log\n").unwrap();
    std::fs::write(c.wt().join("debug.log"), "noise\n").unwrap();
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap().changed, 3, "README.md, export.csv and .gitignore; debug.log is ignored");

    // Commit
    assert_eq!(hand::commit(&c.st, &c.task, "  ").await.unwrap_err(), "Write a commit message first");
    let made = hand::commit(&c.st, &c.task, "Export invoices as CSV").await.unwrap();
    assert_eq!((made.sha.clone(), made.subject.as_str()), (git(&c.wt(), &["rev-parse", "HEAD"]), "Export invoices as CSV"));
    let files = git(&c.wt(), &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files.lines().collect::<Vec<_>>(), [".gitignore", "README.md", "export.csv"], "every change, untracked files too");
    assert_eq!(git(&c.wt(), &["log", "-1", "--format=%an", "HEAD"]), "Gizai CI", "the author is who your git config says");
    assert_eq!(hand::commit(&c.st, &c.task, "Again").await.unwrap_err(), "There is nothing to commit: no file has changed");
    let h = hand::card(&c.st, &c.task).await.unwrap();
    assert_eq!((h.changed, h.unpushed), (0, 1));
    assert!(!has_branch(&c.bare, BRANCH), "a commit stays on this computer");

    // Push: to GitHub, through the project's clone, as a run's push and Open pull request do
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(), format!("Pushed {BRANCH} (1 commit)."));
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]), made.sha);
    assert_eq!(git(&c.bare, &["log", "--format=%s", &format!("main..{BRANCH}")]), "Export invoices as CSV");
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(), format!("Nothing to push: GitHub has every commit of {BRANCH} already."));
    // what isn't committed isn't pushed, and Push says so
    std::fs::write(c.wt().join("draft.txt"), "draft\n").unwrap();
    std::fs::write(c.wt().join("export.csv"), "id,total,vat\n").unwrap();
    git(&c.wt(), &["add", "export.csv"]);
    git(&c.wt(), &["commit", "-q", "-m", "Add VAT"]);
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(),
               format!("Pushed {BRANCH} (1 commit). 1 uncommitted change isn't pushed: commit it first."));
    assert!(git(&c.bare, &["ls-tree", "-r", "--name-only", BRANCH]).lines().all(|f| f != "draft.txt"), "only commits go");
}

#[tokio::test]
async fn push_is_never_forced_and_goes_over_the_push_over_setting_like_open_pull_request() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    hand::start(&c.st, &c.task).await.unwrap();
    std::fs::write(c.wt().join("export.csv"), "id\n").unwrap();
    hand::commit(&c.st, &c.task, "Mine").await.unwrap();

    // someone else's commit on the same branch on GitHub: the push is refused, never forced over it
    let other = c.tmp.join("other");
    git(&c.tmp, &["clone", "-q", c.bare.to_str().unwrap(), other.to_str().unwrap()]);
    git(&other, &["checkout", "-q", "-b", BRANCH]);
    git(&other, &["commit", "-q", "--allow-empty", "-m", "Theirs"]);
    git(&other, &["push", "-q", "origin", BRANCH]);
    let theirs = git(&c.bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]);
    let e = hand::push(&c.st, &c.task).await.unwrap_err();
    assert!(!e.is_empty());
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]), theirs, "not forced: {e}");
    // and Hand over, which pushes first, changes nothing on the card when that push fails
    assert!(!hand::over(&c.st, &c.task).await.unwrap_err().is_empty());
    let t = c.task();
    assert_eq!((t.state_name.as_str(), t.by_hand), ("In progress", true));
    assert!(gizai_core::comments::list(&c.st.db, &c.task).unwrap().is_empty());

    // Push over HTTPS with gh's login, as Open pull request: without gh it says so
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    hand::start(&c.st, &c.task).await.unwrap();
    std::fs::write(c.wt().join("export.csv"), "id\n").unwrap();
    hand::commit(&c.st, &c.task, "Mine").await.unwrap();
    settings::set(&c.st.db, "github_push_over", &"https").unwrap();
    let missing = c.tmp.join("no-gh-here");
    settings::set(&c.st.db, "gh_bin", &missing.to_string_lossy().to_string()).unwrap();
    let e = hand::push(&c.st, &c.task).await.unwrap_err();
    assert!(e.contains(&format!("The GitHub CLI isn't at {} any more", missing.display())), "{e}");
    assert!(!has_branch(&c.bare, BRANCH));
    // with gh there, it goes
    let gh = c.tmp.join("gh");
    write_script(&gh, "#!/bin/sh\nexit 1\n");
    settings::set(&c.st.db, "gh_bin", &gh.to_string_lossy().to_string()).unwrap();
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(), format!("Pushed {BRANCH} (1 commit)."));
    assert!(has_branch(&c.bare, BRANCH));
}

#[tokio::test]
async fn hand_over_refuses_uncommitted_changes_then_pushes_qa_takes_it_and_a_qa_fail_brings_it_back_to_you() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    // the QA Agent on Testing (Auto); its fake CLI fails the card
    let team_id = team::list(&c.st.db).unwrap()[0].id.clone();
    team::add_agent(&c.st.db, &c.st.you_id, &team_id, gizai_core::model::AgentInput { name: "QA Agent".into(), role_key: "qa".into(),
        ..Default::default() }).unwrap();
    let result = r#"GIZAI_RESULT: {\"outcome\":\"qa_fail\",\"summary\":\"The CSV has no header.\",\"issues\":[\"(1) The CSV has no header\"]}"#;
    let stream = c.tmp.join("qa-fail.jsonl");
    std::fs::write(&stream, format!(
        "{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"Q1\",\"model\":\"claude-opus-5-5\",\"tools\":[\"Read\"]}}\n\
         {{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"{result}\"}}]}},\"session_id\":\"Q1\"}}\n\
         {{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"{result}\",\"total_cost_usd\":0.01,\"num_turns\":1,\"session_id\":\"Q1\",\
         \"usage\":{{\"input_tokens\":10,\"output_tokens\":10}}}}\n")).unwrap();
    let qa = c.tmp.join("qa-claude");
    write_script(&qa, &format!("#!/bin/bash\ncase \" $* \" in *\" --input-format stream-json \"*) exec bash '{FAKE}' \"$@\" ;; esac\n\
                                cat > /dev/null\ncat '{}'\n", stream.display()));
    settings::set(&c.st.db, "claude_bin", &qa.to_string_lossy().to_string()).unwrap();

    hand::start(&c.st, &c.task).await.unwrap();
    std::fs::write(c.wt().join("export.csv"), "id,total\n").unwrap();
    // uncommitted changes: refused, in a plain sentence, and nothing happens
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap_err(), "KADE-1 has 1 uncommitted change: commit it before you hand it over");
    std::fs::write(c.wt().join("notes.txt"), "x\n").unwrap();
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap_err(), "KADE-1 has 2 uncommitted changes: commit them before you hand it over");
    let t = c.task();
    assert_eq!((t.state_name.as_str(), t.by_hand), ("In progress", true));
    assert!(!has_branch(&c.bare, BRANCH), "nothing pushed");
    std::fs::remove_file(c.wt().join("notes.txt")).unwrap();
    let made = hand::commit(&c.st, &c.task, "Export invoices as CSV").await.unwrap();

    // Hand over: pushed, moved to Testing, the pin off, a comment as you
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap(), "Testing");
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]), made.sha, "pushed first");
    let said = format!("Handed over by {}: 1 commit.\n\n- {} Export invoices as CSV", c.you(), &made.sha[..7]);
    let notes = gizai_core::comments::list(&c.st.db, &c.task).unwrap();
    assert!(notes.iter().any(|n| n.body_md == said && n.author_name == c.you()), "{said:?} in {notes:?}");

    // QA takes it from Testing (the queue runs after Hand over) and fails it: back to you, pinned again
    let qa_id = c.agent("QA Agent");
    until("QA's run to end", || c.runs().iter().any(|r| r.agent_id == qa_id && r.ended_at.is_some())).await;
    until("the card back with you", || c.task().state_name == "In progress").await;
    let t = c.task();
    assert_eq!((t.state_name.as_str(), t.assignee_id.as_deref(), t.by_hand, t.hold.as_deref()),
               ("In progress", Some(c.st.you_id.as_str()), true, None));
    assert_eq!(c.runs().len(), 1, "only QA's run: {:?}", c.runs());
    // still not taken by agents
    let started = runs::pull(&c.st).await;
    assert!(started.iter().all(|(t, _)| *t != c.task));
    assert_eq!(c.runs().len(), 1, "the Backend Agent didn't take it back: {:?}", c.runs());
    // the card bar works on it again, and Hand over sends it to QA once more
    let h = hand::card(&c.st, &c.task).await.unwrap();
    assert_eq!((h.branch.as_str(), h.changed), (BRANCH, 0));
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap(), "Testing");
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn unpushed_counts_what_a_push_through_the_projects_remote_would_still_send() {
    // a project linked by a plain git URL, its clone's origin that same URL (as `git remote -v` shows it, unrewritten), so
    // Gizai pushes through origin and git keeps origin's copy of the branch up to date
    let tmp = real_tempdir();
    let st = gizai_lib::test_state(tmp.path());
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = tmp.path().join("remote/shop.git");
    git(tmp.path(), &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    let local = tmp.path().join("local");
    git(tmp.path(), &["clone", "-q", bare.to_str().unwrap(), local.to_str().unwrap()]);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(bare.display().to_string()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    let c = Card { st, task, bare, tmp: tmp.path().to_path_buf() };
    c.assign(&c.st.you_id.clone());

    assert_eq!(hand::start(&c.st, &c.task).await.unwrap().unpushed, 0, "nothing of its own yet");
    std::fs::write(c.wt().join("export.csv"), "id\n").unwrap();
    hand::commit(&c.st, &c.task, "Export").await.unwrap();
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap().unpushed, 1);
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(), format!("Pushed {BRANCH} (1 commit)."));
    assert_eq!(git(&local, &["rev-parse", &format!("refs/remotes/origin/{BRANCH}")]), git(&c.wt(), &["rev-parse", "HEAD"]), "pushed through origin");
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap().unpushed, 0, "origin's copy is up to date");
    // a commit made in a terminal counts too
    git(&c.wt(), &["commit", "-q", "--allow-empty", "-m", "More"]);
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap().unpushed, 1);
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(), format!("Pushed {BRANCH} (1 commit)."));
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap(), format!("Nothing to push: The project's repository has every commit of {BRANCH} already."));
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap().unpushed, 0);
    assert_eq!(git(&c.bare, &["log", "--format=%s", &format!("main..{BRANCH}")]), "More\nExport");
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn with_its_testing_switch_off_hand_over_moves_it_to_review_and_no_agent_takes_it() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    tasks::update(&c.st.db, &c.st.you_id, &c.task, TaskPatch { testing: Some(false), ..Default::default() }).unwrap();
    hand::start(&c.st, &c.task).await.unwrap();
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap(), "Review", "no commits of its own is fine");
    let t = c.task();
    assert_eq!((t.state_name.as_str(), t.by_hand, t.assignee_id.as_deref()), ("Review", false, Some(c.st.you_id.as_str())));
    let notes = gizai_core::comments::list(&c.st.db, &c.task).unwrap();
    assert!(notes.iter().any(|n| n.body_md == format!("Handed over by {}: no commits of its own.", c.you())), "{notes:?}");
    // hand_card now says it isn't yours by hand any more
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap_err(), "KADE-1 isn't yours to do by hand: open it in the editor first");
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn refused_for_a_card_assigned_to_an_agent_or_to_nobody_or_never_opened() {
    let tmp = real_tempdir();
    let c = linked(tmp.path());
    // assigned to the Backend Agent
    let e = hand::start(&c.st, &c.task).await.unwrap_err();
    assert_eq!(e, "KADE-1 is assigned to Backend Agent, an agent: assign it to yourself to do it by hand");
    assert!(!c.wt().exists(), "no worktree made");
    // assigned to nobody
    c.assign("");
    assert_eq!(hand::start(&c.st, &c.task).await.unwrap_err(), "KADE-1 isn't assigned to anyone: assign it to yourself to do it by hand");
    assert!(!c.wt().exists());
    let t = c.task();
    assert_eq!((t.state_name.as_str(), t.by_hand), ("To do", false));
    // assigned to you but never opened: the card bar's commands refuse
    c.assign(&c.st.you_id.clone());
    let never = "KADE-1 isn't yours to do by hand: open it in the editor first";
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap_err(), never);
    assert_eq!(hand::commit(&c.st, &c.task, "x").await.unwrap_err(), never);
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap_err(), never);
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap_err(), never);
}

#[tokio::test]
async fn refused_while_an_agent_works_on_it_and_for_an_archived_card() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    tasks::update(&c.st.db, &c.st.you_id, &c.task, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    // a person's Run on the card: the Backend Agent works on it
    let (run, finished) = runs::start(&c.st, &c.task, Some(c.agent("Backend Agent")), Some(FAKE.into()), "manual").await.unwrap();
    until("the run to be live", || runs::working_on(&c.st, &c.task)).await;
    let e = hand::start(&c.st, &c.task).await.unwrap_err();
    assert!(e.ends_with("is working on KADE-1: wait for its run to end"), "{e}");
    assert!(!c.task().by_hand);
    runs::stop(&c.st, &run);
    finished.await.unwrap();

    // yours by hand, and then a person's Run on it: Commit, Push and Hand over wait for it; the card bar still reads it
    hand::start(&c.st, &c.task).await.unwrap();
    std::fs::write(c.wt().join("export.csv"), "id\n").unwrap();
    let (run, finished) = runs::start(&c.st, &c.task, Some(c.agent("Backend Agent")), Some(FAKE.into()), "manual").await.unwrap();
    until("the run to be live", || runs::working_on(&c.st, &c.task)).await;
    for (what, e) in [("commit", hand::commit(&c.st, &c.task, "x").await.unwrap_err()), ("push", hand::push(&c.st, &c.task).await.unwrap_err()),
                      ("over", hand::over(&c.st, &c.task).await.unwrap_err())] {
        assert!(e.contains("is working on KADE-1: wait for its run to end"), "{what}: {e}");
    }
    hand::card(&c.st, &c.task).await.unwrap();
    assert!(!has_branch(&c.bare, BRANCH));
    runs::stop(&c.st, &run);
    finished.await.unwrap();

    // archived: every command refuses
    let done = team::get(&c.st.db, &team::list(&c.st.db).unwrap()[0].id).unwrap().states.into_iter().find(|s| s.category == "done").unwrap().id;
    tasks::move_to(&c.st.db, &c.st.you_id, &c.task, &done, "a1").unwrap();
    tasks::archive(&c.st.db, &c.st.you_id, &c.task).unwrap();
    let archived = "KADE-1 is archived: restore it first";
    assert_eq!(hand::start(&c.st, &c.task).await.unwrap_err(), archived);
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap_err(), archived);
    assert_eq!(hand::commit(&c.st, &c.task, "x").await.unwrap_err(), archived);
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap_err(), archived);
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap_err(), archived);
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_worktree_on_another_branch_is_refused_and_a_project_without_a_link_hands_over_without_a_push() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    hand::start(&c.st, &c.task).await.unwrap();
    git(&c.wt(), &["checkout", "-q", "-b", "elsewhere"]);
    let e = format!("KADE-1's worktree is on branch elsewhere, not on the card's branch {BRANCH}: switch it back first");
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap_err(), e);
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap_err(), e);
    git(&c.wt(), &["checkout", "-q", BRANCH]);
    hand::card(&c.st, &c.task).await.unwrap();

    // no link: Push says so, Hand over moves it on without one
    let p = gizai_core::projects::list(&c.st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&c.st.db, &c.st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: None, default_branch: Some("main".into()), ..Default::default() }).unwrap();
    std::fs::write(c.wt().join("export.csv"), "id\n").unwrap();
    hand::commit(&c.st, &c.task, "Mine").await.unwrap();
    assert_eq!(hand::card(&c.st, &c.task).await.unwrap().unpushed, 0, "nothing to push to");
    assert_eq!(hand::push(&c.st, &c.task).await.unwrap_err(),
               "KADE-1 has no GitHub, Bitbucket or git link to push to: add one to Kade first (project page → Edit)");
    assert_eq!(hand::over(&c.st, &c.task).await.unwrap(), "Testing");
    assert!(!has_branch(&c.bare, BRANCH));
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}

#[tokio::test]
async fn a_second_click_while_the_first_still_works_says_gizai_is_busy() {
    let tmp = real_tempdir();
    let c = yours(tmp.path());
    let (a, b) = tokio::join!(hand::start(&c.st, &c.task), hand::start(&c.st, &c.task));
    let busy = "Gizai is still busy with KADE-1 for you: wait a moment".to_string();
    assert!(matches!((&a, &b), (Ok(_), Err(e)) | (Err(e), Ok(_)) if *e == busy), "{a:?} {b:?}");
    // once it is done, the next one goes
    hand::start(&c.st, &c.task).await.unwrap();
    runs::stop_all(&c.st, Duration::from_secs(10)).await;
}
