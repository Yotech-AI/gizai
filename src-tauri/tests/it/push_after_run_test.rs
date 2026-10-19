//! GA-56: Gizai pushes a card's branch itself when a run ends, so an agent whose own `git push` was refused no longer
//! holds a card up. End to end with fake CLIs (never the real ones) and "GitHub" as a local bare repository: the
//! project's clone sends https://github.com/acme/ there and allows no transport but local files, so nothing here can
//! reach the real GitHub.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, TaskPatch};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const LINK: &str = "https://github.com/acme/shop";

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn has_branch(repo: &Path, branch: &str) -> bool {
    Command::new("git").args(["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).current_dir(repo).status().unwrap().success()
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing (see
/// pull_flow_test: a leaked write handle makes running it fail with "Text file busy").
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// "GitHub" (a bare repository under tmp/github) and the project's local clone of https://github.com/acme/shop.
fn github(tmp: &Path) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = tmp.join("github/acme/shop");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    // its own hooks, whatever the global git config says
    git(&bare, &["config", "core.hooksPath", bare.join("hooks").to_str().unwrap()]);
    let local = tmp.join("local");
    let rewrite = format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display());
    git(tmp, &["clone", "-q", "-c", &rewrite, "-c", "protocol.allow=never", "-c", "protocol.file.allow=always",
               "-c", "user.email=t@t", "-c", "user.name=t", LINK, local.to_str().unwrap()]);
    (bare, local)
}

struct Card { st: gizai_lib::AppState, task: String, bare: PathBuf, tmp: PathBuf }

/// Card KADE-1 of a project linked to https://github.com/acme/shop, in To do and assigned to the Backend Agent.
fn linked_card(tmp: &Path) -> Card {
    card_on(gizai_lib::test_state(tmp), tmp)
}

fn card_on(st: gizai_lib::AppState, tmp: &Path) -> Card {
    let (bare, local) = github(tmp);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, gizai_core::model::ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(LINK.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    Card { st, task, bare, tmp: tmp.to_path_buf() }
}

fn describe(c: &Card, text: &str) {
    gizai_core::tasks::update(&c.st.db, &c.st.you_id, &c.task, TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

/// A fake CLI that commits in its working folder (the run's worktree) without pushing, one commit per subject, and
/// then runs `then` with the same arguments and stdin.
fn committing(c: &Card, name: &str, subjects: &[&str], then: &str) -> String {
    let mut s = String::from("#!/bin/bash\n");
    for subject in subjects {
        s.push_str(&format!("git -c user.email=t@t -c user.name=t commit -q --allow-empty -m '{subject}' || exit 1\n"));
    }
    s.push_str(&format!("exec bash '{then}' \"$@\"\n"));
    let path = c.tmp.join(name);
    write_script(&path, &s);
    path.to_string_lossy().to_string()
}

/// A run's notes from Gizai, as Show output reads them back from its log.
fn notes(st: &gizai_lib::AppState, run_id: &str) -> Vec<String> {
    gizai_lib::runs::events_for(st, run_id).into_iter()
        .filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

fn task(c: &Card) -> gizai_core::model::Task { gizai_core::tasks::get(&c.st.db, &c.task).unwrap() }

fn run_of(c: &Card, run_id: &str) -> (gizai_core::model::Run, PathBuf, String) {
    let run = gizai_core::runs::get(&c.st.db, run_id).unwrap();
    let (wt, branch) = (PathBuf::from(run.worktree_path.clone().unwrap()), run.branch.clone().unwrap());
    (run, wt, branch)
}

/// Adds a CLI in Settings → Coding CLIs and puts the Backend Agent on it.
fn backend_on_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str, env: &[&str]) {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    let cli = gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id;
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli, ..Default::default() }).unwrap();
}

/// Makes In progress Manual, so the queue doesn't start a card that stayed there again by itself.
fn in_progress_manual(st: &gizai_lib::AppState) {
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let state = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &state, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..300 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn a_claude_code_run_that_commits_but_does_not_push_leaves_its_branch_on_github_and_the_card_moves_on() {
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    // what the Run panel gets while the run ends
    let live: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = live.clone();
    st.notify = Arc::new(move |n| {
        if let gizai_lib::runs::Note::Event { event: RunEvent::Note { text }, .. } = n {
            seen.lock().unwrap().push(text);
        }
    });
    let c = card_on(st, tmp.path());
    describe(&c, "Make two commits. FAKE_COMMIT_TWICE");

    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref(), s.error.as_deref()), ("succeeded", Some("ready_for_testing"), None));
    let (run, wt, branch) = run_of(&c, &s.run_id);
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt, &["rev-parse", "HEAD"]),
               "the branch is on GitHub with the run's last commit");
    assert_eq!(git(&c.bare, &["log", "--format=%s", &format!("main..{branch}")]), "Second change\nFirst change");
    assert_eq!(run.head_sha.as_deref(), Some(git(&wt, &["rev-parse", "HEAD"]).as_str()));
    // the card moved on as its outcome says
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("Testing", None));

    let said = format!("Gizai pushed {branch} (2 commits).");
    assert_eq!(notes(&c.st, &s.run_id), [said.clone()], "the run output says what Gizai pushed");
    let log = std::fs::read_to_string(&run.log_path).unwrap();
    assert!(log.lines().any(|l| l.contains("\"gizai_note\"") && l.contains(&said)), "in the run's log: {log}");
    assert!(live.lock().unwrap().contains(&said), "and live in the Run panel: {:?}", live.lock().unwrap());
}

#[tokio::test]
async fn the_branch_is_on_github_before_the_card_moves_on_so_the_next_agent_finds_it() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    describe(&c, "Make two commits. FAKE_COMMIT_TWICE");
    // QA takes the card in Testing by itself; its fake CLI first writes down what GitHub has
    let team_id = gizai_core::team::list(&c.st.db).unwrap()[0].id.clone();
    gizai_core::team::add_agent(&c.st.db, &c.st.you_id, &team_id, AgentInput { name: "QA Agent".into(), role_key: "qa".into(), ..Default::default() }).unwrap();
    let seen = c.tmp.join("qa-saw");
    let qa = c.tmp.join("qa-claude");
    write_script(&qa, &format!("#!/bin/bash\ngit ls-remote origin \"refs/heads/$(git branch --show-current)\" > '{}'\nexec bash '{FAKE}' \"$@\"\n", seen.display()));
    gizai_core::settings::set(&c.st.db, "claude_bin", &qa.to_string_lossy().to_string()).unwrap();

    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(FAKE.into())).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("ready_for_testing"));
    // where the Backend Agent's run ended (QA's fake commits on, as the card says)
    let (run, _, branch) = run_of(&c, &s.run_id);
    let ended_at = run.head_sha.unwrap();
    until("QA's run",|| seen.exists() && gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().iter().all(|r| r.ended_at.is_some())
        && gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len() == 2).await;
    let saw = std::fs::read_to_string(&seen).unwrap();
    assert_eq!(saw.trim(), format!("{ended_at}\trefs/heads/{branch}"), "QA's run found the branch on GitHub");
    let runs = gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap();
    assert!(runs.iter().any(|r| r.id != s.run_id && r.agent_name == "QA Agent"), "QA took it from Testing");
}

#[tokio::test]
async fn a_run_without_commits_of_its_own_pushes_nothing_and_says_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(FAKE.into())).await.unwrap();
    let (_, _, branch) = run_of(&c, &s.run_id);
    assert!(!has_branch(&c.bare, &branch), "no commits beyond main: nothing to push");
    assert!(notes(&c.st, &s.run_id).is_empty(), "{:?}", notes(&c.st, &s.run_id));
    assert_eq!((task(&c).state_name.as_str(), task(&c).hold), ("Testing", None));
}

#[tokio::test]
async fn an_agent_that_pushed_itself_gets_no_second_push_and_only_commits_github_lacks_are_counted() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    // the agent pushes its first commit itself (it was allowed): GitHub has the branch at the tip, so Gizai pushes nothing
    let pushes = c.tmp.join("pushes");
    write_script(&c.bare.join("hooks/pre-receive"), &format!("#!/bin/sh\ncat >> '{}'\n", pushes.display()));
    let pushed_itself = committing(&c, "pushes-itself", &["Mine"], &c.tmp.join("push-then-fake").to_string_lossy());
    write_script(&c.tmp.join("push-then-fake"), &format!("#!/bin/bash\ngit push -q origin HEAD || exit 1\nexec bash '{FAKE}' \"$@\"\n"));
    in_progress_manual(&c.st);
    gizai_core::settings::set(&c.st.db, "agents_paused", &true).unwrap();
    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(pushed_itself)).await.unwrap();
    let (_, wt, branch) = run_of(&c, &s.run_id);
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt, &["rev-parse", "HEAD"]));
    assert_eq!(std::fs::read_to_string(&pushes).unwrap().lines().count(), 1, "only the agent's own push");
    assert!(notes(&c.st, &s.run_id).is_empty(), "nothing needed pushing: {:?}", notes(&c.st, &s.run_id));
    assert_eq!(task(&c).state_name, "Testing");

    // back to In progress: the next run pushes one commit itself and leaves two more; Gizai pushes those two
    let team_id = gizai_core::team::list(&c.st.db).unwrap()[0].id.clone();
    let in_progress = gizai_core::team::get(&c.st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::tasks::move_to(&c.st.db, &c.st.you_id, &c.task, &in_progress, "").unwrap();
    let mixed = c.tmp.join("mixed");
    write_script(&mixed, &format!("#!/bin/bash\nc() {{ git -c user.email=t@t -c user.name=t commit -q --allow-empty -m \"$1\" || exit 1; }}\n\
c Pushed\ngit push -q origin HEAD || exit 1\nc Left1\nc Left2\nexec bash '{FAKE}' \"$@\"\n"));
    let agent = gizai_core::team::all_agents(&c.st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap().1.actor_id;
    let s2 = gizai_lib::runs::run_once(&c.st, &c.task, Some(agent), Some(mixed.to_string_lossy().to_string())).await.unwrap();
    let (_, wt2, branch2) = run_of(&c, &s2.run_id);
    assert_eq!(branch2, branch, "the card's branch");
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt2, &["rev-parse", "HEAD"]));
    assert_eq!(notes(&c.st, &s2.run_id), [format!("Gizai pushed {branch} (2 commits).")], "only the two GitHub didn't have");
    assert_eq!(std::fs::read_to_string(&pushes).unwrap().lines().count(), 3, "two pushes by the agent, one by Gizai");
}

#[tokio::test]
async fn a_push_github_rejects_keeps_the_card_where_it_is_on_hold_with_the_reason_and_is_never_forced() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    // while the agent works, someone else pushes a commit of their own to the card's branch on GitHub
    let other = c.tmp.join("other");
    git(&c.tmp, &["clone", "-q", c.bare.to_str().unwrap(), other.to_str().unwrap()]);
    let script = c.tmp.join("someone-else-pushes");
    write_script(&script, &format!("#!/bin/bash\n\
git -C '{o}' -c user.email=t@t -c user.name=t commit -q --allow-empty -m Theirs || exit 1\n\
git -C '{o}' push -q origin \"HEAD:refs/heads/$(git branch --show-current)\" || exit 1\n\
exec bash '{FAKE}' \"$@\"\n", o = other.display()));
    let wrapper = committing(&c, "commits-then", &["Mine"], &script.to_string_lossy());
    let agent_before = task(&c).assignee_id;

    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(wrapper)).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")));
    let (run, _, branch) = run_of(&c, &s.run_id);
    assert_eq!(git(&c.bare, &["log", "-1", "--format=%s", &branch]), "Theirs", "GitHub's copy is left alone: never forced");
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("In progress", Some("blocked")), "it stays where it is, on hold");
    let reason = t.hold_reason.clone().unwrap();
    assert!(reason.contains("The branch on GitHub has commits this one doesn't") && reason.contains("never forces"), "{reason}");
    assert!(!reason.contains("[rejected]") && !reason.contains("error:"), "plain words, not git's: {reason}");
    assert_eq!((t.assignee_id, t.fail_count), (agent_before, 0), "nothing else changes");
    // the run's verdict and the agent's summary are still kept
    assert_eq!((run.outcome.as_deref(), run.status.as_str()), (Some("ready_for_testing"), "succeeded"));
    let comments = gizai_core::comments::list(&c.st.db, &c.task).unwrap();
    assert!(comments.iter().any(|m| m.body_md.contains("Exporter added") && m.run_id.as_deref() == Some(s.run_id.as_str())), "{comments:?}");
    // the run output says why
    let said = notes(&c.st, &s.run_id);
    assert!(said.iter().any(|n| n.contains(&format!("Couldn't push {branch}")) && n.contains("The branch on GitHub has commits this one doesn't")), "{said:?}");
    assert!(!said.iter().any(|n| n.starts_with("Gizai pushed")), "{said:?}");
    // and nothing starts again or tries again by itself
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len(), 1);
    assert_eq!(git(&c.bare, &["log", "-1", "--format=%s", &branch]), "Theirs");
}

#[tokio::test]
async fn a_push_without_access_is_tried_once_and_holds_the_card_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    describe(&c, "Make two commits. FAKE_COMMIT_TWICE");
    // GitHub says no to every push, and writes down each attempt
    let tries = c.tmp.join("tries");
    write_script(&c.bare.join("hooks/pre-receive"), &format!("#!/bin/sh\necho try >> '{}'\n\
echo 'ERROR: Permission to acme/shop.git denied to octocat.' >&2\nexit 1\n", tries.display()));

    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(FAKE.into())).await.unwrap();
    let (_, _, branch) = run_of(&c, &s.run_id);
    assert!(!has_branch(&c.bare, &branch));
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("In progress", Some("blocked")));
    let reason = t.hold_reason.unwrap();
    assert!(reason.contains("Your GitHub account octocat can't push to this repository"), "{reason}");
    assert!(notes(&c.st, &s.run_id).iter().any(|n| n.contains("octocat can't push")), "{:?}", notes(&c.st, &s.run_id));
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(std::fs::read_to_string(&tries).unwrap().lines().count(), 1, "one try, no retry");
    assert_eq!(gizai_core::runs::list_for_task(&c.st.db, &c.task).unwrap().len(), 1, "nothing starts again");
}

#[tokio::test]
async fn a_stopped_run_and_a_run_stopped_at_a_limit_are_pushed_too() {
    // Stop
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    describe(&c, "FAKE_HANG");
    in_progress_manual(&c.st);
    let wrapper = committing(&c, "commits-then-hangs", &["Before stop"], FAKE);
    let (run_id, done) = gizai_lib::runs::start(&c.st, &c.task, None, Some(wrapper), "manual").await.unwrap();
    let (_, wt, branch) = run_of(&c, &run_id);
    until("the commit", || Command::new("git").args(["log", "-1", "--format=%s"]).current_dir(&wt).output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "Before stop")).await;
    gizai_lib::runs::stop(&c.st, &run_id);
    let s = done.await.unwrap();
    assert_eq!(s.status, "cancelled");
    assert_eq!(git(&c.bare, &["log", "-1", "--format=%s", &branch]), "Before stop");
    assert_eq!(notes(&c.st, &run_id), [format!("Gizai pushed {branch} (1 commit).")]);
    assert_eq!(task(&c).hold, None, "a stopped run's card is held only when the push fails");

    // the tool-call limit, on Codex
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    in_progress_manual(&c.st);
    let wrapper = committing(&c, "codex", &["Before the limit"], FAKE_CLI);
    backend_on_cli(&c.st, "Codex", "codex", &wrapper, &["FAKE_KIND=codex"]);
    gizai_core::settings::set(&c.st.db, "max_run_tool_calls", &1u32).unwrap();
    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, None).await.unwrap();
    assert_eq!(s.status, "timed_out", "{:?}", s.error);
    let (_, _, branch) = run_of(&c, &s.run_id);
    assert_eq!(git(&c.bare, &["log", "-1", "--format=%s", &branch]), "Before the limit");
    assert_eq!(notes(&c.st, &s.run_id), [format!("Gizai pushed {branch} (1 commit).")]);
}

#[tokio::test]
async fn a_stopped_run_whose_push_fails_holds_its_card_with_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    describe(&c, "FAKE_HANG");
    in_progress_manual(&c.st);
    write_script(&c.bare.join("hooks/pre-receive"), "#!/bin/sh\necho 'ERROR: Permission to acme/shop.git denied to octocat.' >&2\nexit 1\n");
    let wrapper = committing(&c, "commits-then-hangs", &["Before stop"], FAKE);
    let (run_id, done) = gizai_lib::runs::start(&c.st, &c.task, None, Some(wrapper), "manual").await.unwrap();
    let (_, wt, branch) = run_of(&c, &run_id);
    until("the commit", || Command::new("git").args(["log", "-1", "--format=%s"]).current_dir(&wt).output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "Before stop")).await;
    gizai_lib::runs::stop(&c.st, &run_id);
    assert_eq!(done.await.unwrap().status, "cancelled");
    assert!(!has_branch(&c.bare, &branch));
    let t = task(&c);
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("In progress", Some("blocked")));
    assert!(t.hold_reason.unwrap().contains("octocat can't push"));
}

#[tokio::test]
async fn codex_gemini_and_other_clis_get_the_same_push_after_their_runs() {
    for kind in ["codex", "gemini", "other"] {
        let tmp = tempfile::tempdir().unwrap();
        let c = linked_card(tmp.path());
        let wrapper = committing(&c, kind, &["First change", "Second change"], FAKE_CLI);
        backend_on_cli(&c.st, kind, kind, &wrapper, &[&format!("FAKE_KIND={kind}")]);
        let s = gizai_lib::runs::run_once(&c.st, &c.task, None, None).await.unwrap();
        assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{kind}: {:?}", s.error);
        let (run, wt, branch) = run_of(&c, &s.run_id);
        assert_eq!(run.adapter.as_deref().map(|a| a.is_empty()), Some(false), "{kind}: ran on its CLI");
        assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt, &["rev-parse", "HEAD"]), "{kind}");
        assert_eq!(notes(&c.st, &s.run_id), [format!("Gizai pushed {branch} (2 commits).")], "{kind}");
        assert_eq!(task(&c).state_name, "Testing", "{kind}");
    }
}

#[tokio::test]
async fn the_push_goes_over_the_push_over_setting_like_open_pull_request() {
    // HTTPS with gh's login: Gizai needs gh for it, as Open pull request does
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    describe(&c, "Make two commits. FAKE_COMMIT_TWICE");
    gizai_core::settings::set(&c.st.db, "github_push_over", &"https").unwrap();
    let missing = c.tmp.join("no-gh-here");
    gizai_core::settings::set(&c.st.db, "gh_bin", &missing.to_string_lossy().to_string()).unwrap();
    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(FAKE.into())).await.unwrap();
    let (_, _, branch) = run_of(&c, &s.run_id);
    assert!(!has_branch(&c.bare, &branch));
    let t = task(&c);
    assert_eq!(t.hold.as_deref(), Some("blocked"));
    assert!(t.hold_reason.as_deref().unwrap().contains(&format!("The GitHub CLI isn't at {} any more", missing.display())), "{:?}", t.hold_reason);

    // with gh there, the same run's branch goes
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    describe(&c, "Make two commits. FAKE_COMMIT_TWICE");
    gizai_core::settings::set(&c.st.db, "github_push_over", &"https").unwrap();
    let gh = c.tmp.join("gh");
    write_script(&gh, "#!/bin/sh\nexit 1\n");
    gizai_core::settings::set(&c.st.db, "gh_bin", &gh.to_string_lossy().to_string()).unwrap();
    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(FAKE.into())).await.unwrap();
    let (_, wt, branch) = run_of(&c, &s.run_id);
    assert_eq!(git(&c.bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt, &["rev-parse", "HEAD"]));
    assert_eq!(task(&c).state_name, "Testing");
}

#[tokio::test]
async fn a_project_without_a_link_is_not_pushed_and_its_card_moves_on_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let (bare, local) = github(tmp.path());
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    gizai_core::tasks::update(&st.db, &st.you_id, &task, TaskPatch { description_md: Some("FAKE_COMMIT_TWICE".into()), ..Default::default() }).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE.into())).await.unwrap();
    let branch = gizai_core::runs::get(&st.db, &s.run_id).unwrap().branch.unwrap();
    assert!(!has_branch(&bare, &branch), "no link: Gizai doesn't push");
    assert!(notes(&st, &s.run_id).is_empty());
    assert_eq!(gizai_core::tasks::get(&st.db, &task).unwrap().state_name, "Testing");
}

#[tokio::test]
async fn uncommitted_work_stays_in_the_worktree_and_the_run_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let c = linked_card(tmp.path());
    let wrapper = c.tmp.join("leaves-work");
    write_script(&wrapper, &format!("#!/bin/bash\ngit -c user.email=t@t -c user.name=t commit -q --allow-empty -m Committed || exit 1\n\
echo draft > draft.txt\nexec bash '{FAKE}' \"$@\"\n"));
    let s = gizai_lib::runs::run_once(&c.st, &c.task, None, Some(wrapper.to_string_lossy().to_string())).await.unwrap();
    let (_, wt, branch) = run_of(&c, &s.run_id);
    assert_eq!(git(&c.bare, &["log", "-1", "--format=%s", &branch]), "Committed");
    assert!(git(&c.bare, &["ls-tree", "-r", "--name-only", &branch]).lines().all(|f| f != "draft.txt"), "only commits go");
    assert_eq!(std::fs::read_to_string(wt.join("draft.txt")).unwrap(), "draft\n", "the uncommitted file stays");
    assert_eq!(notes(&c.st, &s.run_id), [format!("Gizai pushed {branch} (1 commit)."),
                                         "Its worktree has 1 uncommitted change, which Gizai doesn't push.".to_string()]);
}
