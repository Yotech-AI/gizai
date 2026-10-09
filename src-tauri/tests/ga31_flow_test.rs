//! GA-31 end to end with the fake Claude Code (never the real one): Continue with a note (yours, or the Team Lead's on
//! continue_agent_run) reaches the continued run's prompt and is saved on the card as a comment; a `run_for_me` result
//! holds the card with the commands shown, and Done, continue resumes the run with a note that they were run; the
//! commands still show when the push after the run failed; Gizai's nudge continues a run that was already pushed, and
//! the nudged run's end pushes again. "GitHub" is a local bare repository, as in push_after_run_test.
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, ProjectInput, Run, TaskPatch};
use gizai_core::{comments, runs as core_runs, tasks};
use serde_json::json;

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const LINK: &str = "https://github.com/acme/shop";
/// What the fake Claude Code asks you to run with FAKE_RUN_FOR_ME (fixtures/run-for-me.jsonl), exactly.
const CMDS: [&str; 2] = ["sudo pacman -S libayatana-appindicator",
                         "echo \"fs.inotify.max_user_watches=524288\" | sudo tee /etc/sysctl.d/40-watches.conf && sudo sysctl --system"];

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"]);
    repo
}

/// Writes an executable script through a child `sh`, so this process never holds it open for writing ("Text file busy").
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// Adds a CLI in Settings → Coding CLIs and puts the Backend Agent on it.
fn backend_on(st: &gizai_lib::AppState, name: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: "claude_code".into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    let cli = gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id;
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli, ..Default::default() }).unwrap();
    agent.actor_id
}

fn describe(st: &gizai_lib::AppState, task: &str, text: &str) {
    tasks::update(&st.db, &st.you_id, task, TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

/// Makes In progress Manual, so the queue doesn't start a card that stayed there again by itself.
fn in_progress_manual(st: &gizai_lib::AppState) {
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let state = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &state, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
}

fn stderr_of(run: &Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

/// The prompt the fake Claude Code was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &Run) -> String {
    let err = stderr_of(run);
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

/// The card's runs, oldest first.
fn runs_of(st: &gizai_lib::AppState, task: &str) -> Vec<Run> {
    let mut runs = core_runs::list_for_task(&st.db, task).unwrap();
    runs.sort_by_key(|r| r.created_at);
    runs
}

/// Waits until the card has `n` runs and none of them is live.
async fn wait_for_runs(st: &gizai_lib::AppState, task: &str, n: usize) -> Vec<Run> {
    let t0 = Instant::now();
    loop {
        let runs = runs_of(st, task);
        if runs.len() >= n && gizai_lib::runs::live(st).iter().all(|l| l.task_id != task) && runs.iter().all(|r| r.ended_at.is_some()) {
            return runs;
        }
        assert!(t0.elapsed() < Duration::from_secs(30), "{n} runs expected: {:?}", runs.iter().map(|r| (&r.trigger, &r.status)).collect::<Vec<_>>());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn your_name(st: &gizai_lib::AppState) -> String {
    gizai_core::users::list(&st.db).unwrap().into_iter().find(|p| p.id == st.you_id).unwrap().name
}

/// The comments `author` wrote on the card, oldest first.
fn comments_by(st: &gizai_lib::AppState, task: &str, author: &str) -> Vec<gizai_core::model::Comment> {
    let mut list: Vec<_> = comments::list(&st.db, task).unwrap().into_iter().filter(|c| c.author_id == author).collect();
    list.sort_by_key(|c| c.created_at);
    list
}

fn commands() -> Vec<String> {
    CMDS.iter().map(|c| c.to_string()).collect()
}

/// A state with the Backend Agent on a fake Claude Code that prints its prompt, In progress Manual, and card KADE-1.
fn card_with_fake(tmp: &Path) -> (gizai_lib::AppState, String, String) {
    let st = gizai_lib::test_state(tmp);
    let repo = git_repo(tmp);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let agent = backend_on(&st, "Claude Code (prints its prompt)", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    in_progress_manual(&st);
    (st, task, agent)
}

/// A first run that stops at the tool-call limit (the fake makes two tool calls): Continue takes it up.
async fn stopped_run(st: &gizai_lib::AppState, task: &str) -> Run {
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    let s = gizai_lib::runs::run_once(st, task, None, None).await.unwrap();
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &200u32).unwrap();
    assert_eq!(s.status, "timed_out", "{:?}", s.error);
    core_runs::get(&st.db, &s.run_id).unwrap()
}

// ---- Continue with a message ----

#[tokio::test]
async fn continue_with_your_note_tells_the_agent_next_to_why_it_stopped_and_saves_the_note_as_your_comment() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, task, _) = card_with_fake(tmp.path());
    let first = stopped_run(&st, &task).await;
    assert!(comments_by(&st, &task, &st.you_id).is_empty());

    let (id, done) = gizai_lib::runs::continue_with_note(&st, &first.id, Some("  Use the existing CSV writer.\nKeep the column order.\n ".into()), None)
        .await.unwrap();
    let end = done.await.unwrap();
    assert_eq!((end.status.as_str(), end.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}", end.error);
    let run = core_runs::get(&st.db, &id).unwrap();
    // a Continue: the same session, resumed, read as a Continue (not Gizai's nudge)
    assert_eq!((run.trigger.as_str(), run.nudged), ("nudge", false));
    assert_eq!(run.session_id, first.session_id);
    assert!(stderr_of(&run).contains(&format!("--resume {}", first.session_id.clone().unwrap())), "{}", stderr_of(&run));

    // the prompt: why it stopped, your note quoted, then what to do; How this run works at the end
    let p = prompt_of(&run);
    let you = your_name(&st);
    let (why, by, note, go, rules) = (
        p.find(&format!("Your last run on this task was stopped: {}", first.error.clone().unwrap().trim_end_matches('.'))).unwrap_or_else(|| panic!("{p}")),
        p.find(&format!("{you} wrote a note for this run:\n\n")).unwrap_or_else(|| panic!("{p}")),
        p.find("> Use the existing CSV writer.\n> Keep the column order.\n\n").unwrap_or_else(|| panic!("{p}")),
        p.find("Continue where you left off.").unwrap(), p.find("## How this run works").unwrap());
    assert!(why < by && by < note && note < go && go < rules, "{p}");
    assert_eq!(p.matches("Use the existing CSV writer.").count(), 1, "{p}");

    // the note on the card: your comment, as you wrote it (trimmed), not one of the run's
    let mine = comments_by(&st, &task, &st.you_id);
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert_eq!((mine[0].body_md.as_str(), mine[0].run_id.as_deref()), ("Use the existing CSV writer.\nKeep the column order.", None));
    assert!(mine[0].created_at >= run.created_at, "saved once the continued run was recorded");
    // the agent's own summary is the run's comment, as before
    assert!(comments::list(&st.db, &task).unwrap().iter().any(|c| c.run_id.as_deref() == Some(id.as_str()) && c.body_md.contains("Exporter added")));
}

#[tokio::test]
async fn an_empty_note_is_a_plain_continue_and_a_continue_that_cant_start_leaves_no_comment() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, task, _) = card_with_fake(tmp.path());
    let first = stopped_run(&st, &task).await;
    let (id, done) = gizai_lib::runs::continue_with_note(&st, &first.id, Some("  \n ".into()), None).await.unwrap();
    done.await.unwrap();
    let p = prompt_of(&core_runs::get(&st.db, &id).unwrap());
    assert!(p.starts_with("Your last run on this task was stopped:") && !p.contains("wrote a note"), "{p}");
    assert!(comments_by(&st, &task, &st.you_id).is_empty(), "nothing to save");

    // refused Continues save nothing: an older run, a finished one
    let e = gizai_lib::runs::continue_with_note(&st, &first.id, Some("Use the CSV writer".into()), None).await.map(|_| ()).unwrap_err();
    assert!(e.contains("only the card's latest run can continue"), "{e}");
    let e = gizai_lib::runs::continue_with_note(&st, &id, Some("Use the CSV writer".into()), None).await.map(|_| ()).unwrap_err();
    assert!(e.contains("this run finished"), "{e}");
    assert!(comments_by(&st, &task, &st.you_id).is_empty());
    assert_eq!(runs_of(&st, &task).len(), 2, "nothing started");
}

#[tokio::test]
async fn the_team_leads_continue_agent_run_takes_a_note_and_on_a_question_the_note_is_the_answer() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, task, _) = card_with_fake(tmp.path());
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(),
        chat_enabled: Some(true), ..Default::default() }).unwrap();
    let call = |args: serde_json::Value| { let (st, lead) = (st.clone(), lead.clone()); async move { gizai_lib::tools::call(&st, &lead, "continue_agent_run", args).await } };

    // the run asks a decision (an older result line, no commands)
    describe(&st, &task, "Export the invoices. FAKE_ASKS");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("needs_decision"));
    assert_eq!(tasks::get(&st.db, &task).unwrap().hold.as_deref(), Some("needs_decision"));
    // nobody answered and no note (or an empty one): refused, as before
    for args in [json!({"task": "KADE-1"}), json!({"task": "KADE-1", "note": "   "})] {
        let e = call(args.clone()).await.unwrap_err();
        assert!(e.contains("nobody has answered"), "{args}: {e}");
    }
    assert!(comments_by(&st, &task, &lead).is_empty());

    // with a note: it continues, the note is the answer
    let r = call(json!({"task": "KADE-1", "note": "Use JSON: the client's importer reads it."})).await.unwrap();
    assert_eq!(r["done"], "continued", "{r}");
    let second = r["run"]["id"].as_str().unwrap().to_string();
    let runs = wait_for_runs(&st, &task, 2).await;
    let run = runs.iter().find(|x| x.id == second).unwrap();
    assert_eq!((run.trigger.as_str(), run.nudged, run.outcome.as_deref()), ("nudge", false, Some("ready_for_testing")));
    let p = prompt_of(run);
    let (asked, by, text, go) = (p.find("Your last run on this task ended asking for a decision.").unwrap_or_else(|| panic!("{p}")),
        p.find("Team Lead wrote a note for this run:\n\n").unwrap_or_else(|| panic!("{p}")),
        p.find("> Use JSON: the client's importer reads it.").unwrap(), p.find("Continue where you left off, with this answer.").unwrap());
    assert!(asked < by && by < text && text < go, "{p}");
    assert!(!p.contains("It was answered on the card since"), "nothing else was written: {p}");
    // saved as the Team Lead's comment; the hold is cleared and the card moved on
    let notes = comments_by(&st, &task, &lead);
    assert_eq!(notes.iter().map(|c| (c.body_md.as_str(), c.run_id.as_deref())).collect::<Vec<_>>(), [("Use JSON: the client's importer reads it.", None)]);
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.hold.as_deref(), t.state_name.as_str()), (None, "Testing"));

    // on a run that stopped part-way, the Team Lead's note goes next to why it stopped
    let card = gizai_lib::test_task(&st, "", "backend");
    let first = stopped_run(&st, &card).await;
    let ident = tasks::get(&st.db, &card).unwrap().identifier;
    let r = call(json!({"task": ident, "note": "Keep the column order."})).await.unwrap();
    let id = r["run"]["id"].as_str().unwrap().to_string();
    wait_for_runs(&st, &card, 2).await;
    let p = prompt_of(&core_runs::get(&st.db, &id).unwrap());
    assert!(p.starts_with("Your last run on this task was stopped:") && p.contains("Team Lead wrote a note for this run:\n\n> Keep the column order.\n\n"), "{p}");
    assert!(p.find("Team Lead wrote a note").unwrap() < p.find("Continue where you left off.").unwrap(), "{p}");
    assert_eq!(core_runs::get(&st.db, &id).unwrap().session_id, first.session_id);
    assert_eq!(comments_by(&st, &card, &lead).len(), 1);
}

// ---- Run this for me ----

#[tokio::test]
async fn a_run_for_me_result_holds_the_card_with_the_commands_and_done_continue_resumes_the_run() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, task, agent) = card_with_fake(tmp.path());
    describe(&st, &task, "Add a tray icon. FAKE_RUN_FOR_ME");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("needs_decision")), "{:?}", s.error);

    // the run and the card show the commands exactly, and the card is held, in your Inbox
    let first = core_runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!(first.run_for_me, CMDS);
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("In progress", Some("needs_decision")));
    assert_eq!(t.hold_reason.as_deref(), Some("The tray needs libayatana-appindicator, which needs sudo. Everything else is committed."));
    assert_eq!(t.run_for_me, CMDS);
    assert_eq!(serde_json::to_value(&t).unwrap()["runForMe"], json!(CMDS), "what the UI gets");
    assert!(tasks::needs_you(&st.db, &st.you_id).unwrap().iter().any(|x| x.id == task && x.run_for_me == CMDS), "in the Inbox");
    // its hold notifies like any other hold (GA-21)
    let items = gizai_lib::notifications::inbox(&st.db, &st.you_id).unwrap();
    assert!(items.iter().any(|i| i.notice.title.starts_with(&format!("{} is on hold: The tray needs", t.identifier))),
            "{:?}", items.iter().map(|i| &i.notice.title).collect::<Vec<_>>());
    // the Team Lead's board check sees the commands and leaves the card to you
    let board = gizai_lib::board::check_json(&st).unwrap();
    let f = board["findings"].as_array().unwrap().iter().find(|f| f["task"] == t.identifier.as_str()).unwrap_or_else(|| panic!("{board}"));
    assert_eq!((f["kind"].as_str(), f["run_for_me"].clone()), (Some("held"), json!(CMDS)), "{f}");
    // a result line was there: no nudge
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(runs_of(&st, &task).len(), 1);

    // Done, continue: the same run's session continues with your note that the commands were run
    let (id, done) = gizai_lib::runs::continue_after_run_for_me(&st, &task).await.unwrap();
    assert_eq!(tasks::get(&st.db, &task).unwrap().hold, None, "the hold is cleared as it starts");
    let end = done.await.unwrap();
    assert_eq!((end.status.as_str(), end.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}", end.error);
    let run = core_runs::get(&st.db, &id).unwrap();
    assert_eq!((run.trigger.as_str(), run.nudged, run.agent_id.as_str()), ("nudge", false, agent.as_str()), "a Continue of the same agent");
    assert_eq!((&run.session_id, &run.worktree_path, &run.branch), (&first.session_id, &first.worktree_path, &first.branch));
    assert!(stderr_of(&run).contains("--resume S1"), "{}", stderr_of(&run));
    let p = prompt_of(&run);
    let you = your_name(&st);
    assert!(p.starts_with("Your last run on this task ended asking for a decision."), "{p}");
    let quoted = format!("{you} wrote a note for this run:\n\n> Done: I ran the commands you asked me to run.\n> \n> ```sh\n> {}\n> {}\n> ```\n> \n\
> Check that they worked, then carry on.\n\nContinue where you left off", CMDS[0], CMDS[1]);
    assert!(p.contains(&quoted), "{quoted}\n---\n{p}");
    assert!(!p.contains("It was answered on the card since"), "the agent's own summary isn't an answer: {p}");
    // the note is your comment, word for word
    let mine = comments_by(&st, &task, &st.you_id);
    assert_eq!(mine.iter().map(|c| (c.body_md.clone(), c.run_id.clone())).collect::<Vec<_>>(), [(gizai_lib::runs::ran_for_me_note(&commands()), None)]);
    assert_eq!(mine[0].body_md, format!("Done: I ran the commands you asked me to run.\n\n```sh\n{}\n{}\n```\n\nCheck that they worked, then carry on.", CMDS[0], CMDS[1]));
    // the card moved on and asks nothing any more
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.run_for_me.len()), ("Testing", None, 0));
    // and Done, continue doesn't go twice
    let e = gizai_lib::runs::continue_after_run_for_me(&st, &task).await.map(|_| ()).unwrap_err();
    assert!(e.contains("didn't ask you to run anything"), "{e}");
    assert_eq!(runs_of(&st, &task).len(), 2);
}

#[test]
fn done_continues_note_names_one_command_or_several() {
    let one = gizai_lib::runs::ran_for_me_note(&["sudo pacman -S libayatana-appindicator".into()]);
    assert_eq!(one, "Done: I ran the command you asked me to run.\n\n```sh\nsudo pacman -S libayatana-appindicator\n```\n\nCheck that it worked, then carry on.");
    let two = gizai_lib::runs::ran_for_me_note(&commands());
    assert!(two.starts_with("Done: I ran the commands you asked me to run.") && two.ends_with("Check that they worked, then carry on."), "{two}");
}

#[tokio::test]
async fn done_continue_needs_a_latest_run_that_asked_you_to_run_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, task, _) = card_with_fake(tmp.path());
    let e = gizai_lib::runs::continue_after_run_for_me(&st, &task).await.map(|_| ()).unwrap_err();
    assert!(e.contains("no run to continue"), "{e}");
    // a question without commands
    describe(&st, &task, "FAKE_ASKS");
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.hold.as_deref(), t.run_for_me.len()), (Some("needs_decision"), 0));
    let e = gizai_lib::runs::continue_after_run_for_me(&st, &task).await.map(|_| ()).unwrap_err();
    assert!(e.contains("didn't ask you to run anything"), "{e}");
    assert!(comments_by(&st, &task, &st.you_id).is_empty());
    assert_eq!(runs_of(&st, &task).len(), 1, "nothing started");
    assert_eq!(tasks::get(&st.db, &task).unwrap().hold.as_deref(), Some("needs_decision"), "still held");
}

// ---- With Gizai's push after the run (GA-56) ----

/// "GitHub" (a bare repository under tmp/github) and the project's local clone of https://github.com/acme/shop.
fn github(tmp: &Path) -> (PathBuf, PathBuf) {
    let src = tmp.join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    let bare = tmp.join("github/acme/shop");
    git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
    git(&bare, &["config", "core.hooksPath", bare.join("hooks").to_str().unwrap()]);
    let local = tmp.join("local");
    let rewrite = format!("url.{}/.insteadOf=https://github.com/acme/", tmp.join("github/acme").display());
    git(tmp, &["clone", "-q", "-c", &rewrite, "-c", "protocol.allow=never", "-c", "protocol.file.allow=always",
               "-c", "user.email=t@t", "-c", "user.name=t", LINK, local.to_str().unwrap()]);
    (bare, local)
}

/// Card KADE-1 of a project linked to https://github.com/acme/shop, for the Backend Agent on a CLI running `command`.
fn linked_card(tmp: &Path, command: &str) -> (gizai_lib::AppState, String, PathBuf) {
    let st = gizai_lib::test_state(tmp);
    let (bare, local) = github(tmp);
    let task = gizai_lib::test_task(&st, local.to_str().unwrap(), "backend");
    let p = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    gizai_core::projects::update(&st.db, &st.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(),
        repo_path: p.repo_path.clone(), repo_url: Some(LINK.into()), default_branch: Some("main".into()), ..Default::default() }).unwrap();
    backend_on(&st, "Claude Code (prints its prompt)", command, &["FAKE_TEMP=1"]);
    in_progress_manual(&st);
    (st, task, bare)
}

/// Gizai's notes in a run's output, as Show output reads them back.
fn notes(st: &gizai_lib::AppState, run_id: &str) -> Vec<String> {
    gizai_lib::runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

#[tokio::test]
async fn the_commands_still_show_when_the_push_after_the_run_fails_and_done_continue_pushes_again() {
    let tmp = tempfile::tempdir().unwrap();
    let (st, task, bare) = linked_card(tmp.path(), FAKE_CLAUDE);
    // GitHub says no to pushes until `allow` exists
    let allow = tmp.path().join("allow");
    write_script(&bare.join("hooks/pre-receive"), &format!("#!/bin/sh\n[ -e '{}' ] && exit 0\n\
echo 'ERROR: Permission to acme/shop.git denied to octocat.' >&2\nexit 1\n", allow.display()));
    describe(&st, &task, "Add a tray icon. FAKE_COMMIT_TWICE FAKE_RUN_FOR_ME");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("needs_decision"), "{:?}", s.error);
    let first = core_runs::get(&st.db, &s.run_id).unwrap();
    let branch = first.branch.clone().unwrap();
    assert!(notes(&st, &s.run_id).iter().any(|n| n.contains(&format!("Couldn't push {branch}"))), "{:?}", notes(&st, &s.run_id));

    // the failed push's hold replaces the question's, and the commands still show on the card and in the Inbox
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("In progress", Some("blocked")));
    assert!(t.hold_reason.as_deref().unwrap().contains("octocat can't push"), "{:?}", t.hold_reason);
    assert_eq!(t.run_for_me, CMDS);
    assert_eq!((first.outcome.as_deref(), first.run_for_me.clone()), (Some("needs_decision"), commands()));
    assert!(tasks::needs_you(&st.db, &st.you_id).unwrap().iter().any(|x| x.id == task && x.run_for_me == CMDS));
    let board = gizai_lib::board::check_json(&st).unwrap();
    let f = board["findings"].as_array().unwrap().iter().find(|f| f["task"] == t.identifier.as_str()).unwrap_or_else(|| panic!("{board}"));
    assert_eq!((f["hold"].as_str(), f["run_for_me"].clone()), (Some("blocked"), json!(CMDS)), "{f}");

    // access is fixed; Done, continue resumes the run, and its end pushes the branch
    std::fs::write(&allow, "").unwrap();
    let (id, done) = gizai_lib::runs::continue_after_run_for_me(&st, &task).await.unwrap();
    let end = done.await.unwrap();
    assert_eq!(end.outcome.as_deref(), Some("ready_for_testing"), "{:?}", end.error);
    let run = core_runs::get(&st.db, &id).unwrap();
    assert_eq!(run.session_id, first.session_id);
    assert!(prompt_of(&run).contains("> Done: I ran the commands you asked me to run."), "{}", prompt_of(&run));
    let wt = PathBuf::from(run.worktree_path.clone().unwrap());
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt, &["rev-parse", "HEAD"]), "on GitHub now");
    assert_eq!(notes(&st, &id), [format!("Gizai pushed {branch} (2 commits).")]);
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref(), t.run_for_me.len()), ("Testing", None, 0));
}

#[tokio::test]
async fn gizais_nudge_continues_a_run_that_was_pushed_and_the_nudged_runs_end_pushes_again() {
    let tmp = tempfile::tempdir().unwrap();
    // every run commits once, then the fake Claude Code; only the first prompt (the task's) ends without a result
    let wrapper = tmp.path().join("commits-then-fake");
    write_script(&wrapper, &format!("#!/bin/bash\ngit -c user.email=t@t -c user.name=t commit -q --allow-empty -m Work || exit 1\n\
exec bash '{FAKE_CLAUDE}' \"$@\"\n"));
    let (st, task, bare) = linked_card(tmp.path(), &wrapper.to_string_lossy());
    let pushes = tmp.path().join("pushes");
    write_script(&bare.join("hooks/pre-receive"), &format!("#!/bin/sh\ncat >> '{}'\n", pushes.display()));
    describe(&st, &task, "Release it. FAKE_NO_RESULT");

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", None), "{:?}", s.error);
    let runs = wait_for_runs(&st, &task, 2).await;
    assert_eq!(runs.iter().map(|r| (r.trigger.as_str(), r.outcome.as_deref().unwrap_or(""), r.nudged)).collect::<Vec<_>>(),
               [("manual", "no_result", false), ("result_nudge", "ready_for_testing", true)]);
    let (first, nudged) = (&runs[0], &runs[1]);
    let branch = first.branch.clone().unwrap();
    // the first run was pushed before the nudge started, and the nudge's end pushed its own commit
    assert_eq!(notes(&st, &first.id), [format!("Gizai pushed {branch} (1 commit).")]);
    assert_eq!(notes(&st, &nudged.id), [format!("Gizai pushed {branch} (1 commit).")]);
    assert_eq!(std::fs::read_to_string(&pushes).unwrap().lines().count(), 2, "two pushes");
    assert_eq!(nudged.session_id, first.session_id);
    let wt = PathBuf::from(nudged.worktree_path.clone().unwrap());
    assert_eq!(git(&bare, &["rev-parse", &format!("refs/heads/{branch}")]), git(&wt, &["rev-parse", "HEAD"]));
    assert_eq!(git(&bare, &["rev-list", "--count", &format!("main..{branch}")]), "2");
    assert!(prompt_of(nudged).starts_with("Your run ended without your GIZAI_RESULT line"), "{}", prompt_of(nudged));
    let t = tasks::get(&st.db, &task).unwrap();
    assert_eq!((t.state_name.as_str(), t.hold.as_deref()), ("Testing", None));
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert_eq!(runs_of(&st, &task).len(), 2, "one nudge, no more");
}
