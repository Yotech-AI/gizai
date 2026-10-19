//! GA-48: each run gets a temp folder as TMPDIR, TMP and TEMP, emptied when the run ends; every task prompt ends with
//! "How this run works"; the tool calls Claude Code refused are saved on the run and returned to the Team Lead. GA-210:
//! that temp folder is short and outside the worktree (`worktree::prepare_run_temp`, in the test's own `temp_base`), and
//! the worktree's `.gizai-tmp` stays, out of git status, for the screenshots and the preview, and for the run when the
//! short one can't be used. Runs use the fake CLIs (FAKE_TEMP=1 makes them print their environment and prompt), never
//! the real ones.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, TaskPatch};
use serde_json::json;

const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");
const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const GITIGNORE: &str = ".env\ntarget/\n";

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap()
}

/// A main checkout with a .gitignore of its own, committed.
fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), GITIGNORE).unwrap();
    git(&repo, &["add", ".gitignore"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    repo
}

/// Adds a CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent on the CLI `cli` and returns its id.
fn put_agent_on(st: &gizai_lib::AppState, cli: &str, extra: AgentInput) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..extra }).unwrap();
    agent.actor_id
}

fn describe(st: &gizai_lib::AppState, task: &str, text: &str) {
    gizai_core::tasks::update(&st.db, &st.you_id, task, TaskPatch { description_md: Some(text.into()), ..Default::default() }).unwrap();
}

fn stderr_of(run: &gizai_core::model::Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

/// The prompt the fake CLI was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &gizai_core::model::Run) -> String {
    let err = stderr_of(run);
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

/// The run's temp folder (GA-210): short, outside its worktree, in Gizai's `temp_base`.
fn temp_of(st: &gizai_lib::AppState, run: &gizai_core::model::Run) -> PathBuf {
    gizai_agents::worktree::run_temp_in(&st.temp_base, Path::new(run.worktree_path.as_deref().unwrap()))
}

/// The worktree's own `.gizai-tmp`: the screenshots and the preview, and the run's temp files when the short folder
/// can't be used.
fn own_temp_of(run: &gizai_core::model::Run) -> PathBuf {
    Path::new(run.worktree_path.as_deref().unwrap()).join(".gizai-tmp")
}

/// Gizai's notes in the run's output.
fn notes_of(st: &gizai_lib::AppState, run: &gizai_core::model::Run) -> Vec<String> {
    gizai_lib::runs::events_for(st, &run.id).into_iter()
        .filter_map(|e| match e.event { gizai_agents::stream::RunEvent::Note { text } => Some(text), _ => None }).collect()
}

fn entries(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
}

/// `git status` the way a person runs it, with every untracked file.
fn status(dir: &Path) -> String {
    git_out(dir, &["status", "--porcelain", "--untracked-files=all"])
}

fn exclude_lines(repo: &Path) -> usize {
    std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap_or_default().lines().filter(|l| l.trim() == "/.gizai-tmp/").count()
}

/// Makes In progress Manual (GA-49: the column decides who starts a card). A run stopped at a limit leaves its card there,
/// assigned to its agent: in an Auto column the queue would start the card again on its own as the run ends, while the
/// test looks at the temp folder or Continues the run by hand.
fn in_progress_manual(st: &gizai_lib::AppState) {
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let state = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "in_progress").unwrap().id;
    gizai_core::columns::set_column(&st.db, &st.you_id, &state, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
}

#[tokio::test]
async fn claude_code_codex_gemini_and_other_clis_get_the_runs_short_temp_folder_as_tmpdir_tmp_and_temp() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let clis = [
        ("Claude Code (temp)", "claude_code", FAKE_CLAUDE, vec!["FAKE_TEMP=1"]),
        ("Codex", "codex", FAKE_CLI, vec!["FAKE_KIND=codex", "FAKE_TEMP=1"]),
        ("Gemini", "gemini", FAKE_CLI, vec!["FAKE_KIND=gemini", "FAKE_TEMP=1"]),
        ("Plain", "other", FAKE_CLI, vec!["FAKE_KIND=other", "FAKE_TEMP=1"]),
    ];
    for (name, kind, command, env) in clis {
        // a CLI's own TMPDIR line is overruled: the run's temp folder comes after the CLI's environment lines
        let mut env = env.clone();
        env.push("TMPDIR=/tmp/not-this-one");
        let cli = add_cli(&st, name, kind, command, &env);
        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        put_agent_on(&st, &cli, AgentInput::default());
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{name}: {:?}", s.error);
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        let wt = Path::new(run.worktree_path.as_deref().unwrap());
        let dir = temp_of(&st, &run);
        let d = dir.display();
        let err = stderr_of(&run);
        assert!(err.contains(&format!("temp: TMPDIR={d} TMP={d} TEMP={d}\n")), "{name}: {err}");
        assert!(err.contains("temp exists: yes"), "{name}: the folder is there when the run starts: {err}");
        assert!(err.contains("temp entries: \n"), "{name}: empty when the run starts: {err}");
        // GA-210: outside the worktree and Gizai's data folder, and the CLI may read and change it
        assert!(!dir.starts_with(wt) && !dir.starts_with(&st.data_dir), "{name}: {d}");
        let argv = err.lines().find(|l| l.starts_with("argv: ")).unwrap_or_else(|| panic!("{name}: no argv in {err}"));
        let given = match kind {
            "claude_code" => argv.contains(&format!("--add-dir {d}")),
            "codex" => argv.contains(&format!(",\"{d}\"]")),
            "gemini" => argv.contains(&format!("--include-directories={d}")),
            _ => !argv.contains(&d.to_string()),
        };
        assert!(given, "{name}: {argv}");
        // the fake left a file and a folder in it: the run's end emptied it, and the folder stays
        assert!(dir.is_dir(), "{name}");
        assert!(entries(&dir).is_empty(), "{name}: emptied when the run ended: {:?}", entries(&dir));
        // the worktree's .gizai-tmp is still made, for the screenshots and the preview, empty and out of git status
        let own = own_temp_of(&run);
        assert!(own.is_dir() && entries(&own).is_empty(), "{name}");
        std::fs::write(own.join("left-by-hand.txt"), "x").unwrap();
        assert!(!status(wt).contains("gizai-tmp"), "{name}: the worktree's git status doesn't show it: {}", status(wt));
        assert_eq!(std::fs::read_to_string(wt.join(".gitignore")).unwrap(), GITIGNORE, "{name}: .gitignore untouched");
    }
    // four runs in four worktrees of one repository: one line in the shared info/exclude, .gitignore as it was
    assert_eq!(exclude_lines(&repo), 1, "{}", std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap());
    assert_eq!(std::fs::read_to_string(repo.join(".gitignore")).unwrap(), GITIGNORE);
    // the main checkout ignores a .gizai-tmp of its own too
    std::fs::create_dir_all(repo.join(".gizai-tmp/sub")).unwrap();
    std::fs::write(repo.join(".gizai-tmp/sub/x.txt"), "x").unwrap();
    assert_eq!(status(&repo), "", "the main checkout's git status is clean");
}

#[tokio::test]
async fn every_task_prompt_new_and_continued_ends_with_how_this_run_works() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let claude = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    // no list of its own: the default one, with the read-only helpers
    put_agent_on(&st, &claude, AgentInput::default());
    describe(&st, &task, "The card's own words.");
    in_progress_manual(&st);
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let first = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    assert_eq!(first.status, "timed_out", "{:?}", first.error);
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &200u32).unwrap();
    let (id, done) = gizai_lib::runs::continue_run(&st, &first.id, None).await.unwrap();
    done.await.unwrap();
    let next = gizai_core::runs::get(&st.db, &id).unwrap();
    assert_eq!((next.trigger.as_str(), next.status.as_str()), ("nudge", "succeeded"), "{:?}", next.error);

    let temp = temp_of(&st, &first).display().to_string();
    for (which, run) in [("new", &first), ("continued", &next)] {
        let p = prompt_of(run);
        let at = p.find("## How this run works").unwrap_or_else(|| panic!("{which}: no section in {p}"));
        let section = &p[at..];
        assert!(!section[3..].contains("\n## "), "{which}: the section is the prompt's last: {section}");
        for want in ["Nobody can approve anything during this run", "`git status`", "`git commit`", "`cargo`", "`./vendor/bin/*`",
                     "`head`", "`tail`", "`wc`", "`sort`", "`uniq`", "`cut`", "`diff`", "`grep`", "`jq`", "`pwd`", "`which`", "`tree`",
                     &format!("`{temp}`"), "never in `/tmp`", "Make files with the Write tool", "don't try other spellings"] {
            assert!(section.contains(want), "{which}: {want} missing in {section}");
        }
        let lines = section.lines().filter(|l| l.starts_with("- ")).count();
        assert!((4..=12).contains(&lines), "{which}: at most about 12 lines: {lines} in {section}");
    }
    assert!(prompt_of(&first).contains("The card's own words."));
    assert!(!prompt_of(&next).contains("The card's own words."), "the continued run got the short Continue prompt, with the section");
}

#[tokio::test]
async fn the_temp_folder_is_emptied_after_stop_and_after_the_tool_call_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let claude = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);

    // Stop: the fake fills the folder, then hangs until it is stopped
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &claude, AgentInput::default());
    describe(&st, &task, "FAKE_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    let dir = temp_of(&st, &gizai_core::runs::get(&st.db, &run_id).unwrap());
    let t0 = std::time::Instant::now();
    while !dir.join("left.txt").exists() {
        assert!(t0.elapsed() < Duration::from_secs(15), "the fake never wrote in {}", dir.display());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(dir.join("scratch/test.db").is_file());
    gizai_lib::runs::stop(&st, &run_id);
    done.await.unwrap();
    assert_eq!(gizai_core::runs::get(&st.db, &run_id).unwrap().status, "cancelled");
    assert!(dir.is_dir() && entries(&dir).is_empty(), "emptied after Stop: {:?}", entries(&dir));

    // the tool-call limit
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    in_progress_manual(&st);
    gizai_core::settings::set(&st.db, "max_run_tool_calls", &1u32).unwrap();
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert_eq!(run.status, "timed_out", "{:?}", run.error);
    assert!(stderr_of(&run).contains("temp exists: yes"));
    let dir = temp_of(&st, &run);
    assert!(dir.is_dir() && entries(&dir).is_empty(), "emptied after the limit: {:?}", entries(&dir));
}

#[tokio::test]
async fn a_run_starts_with_an_empty_temp_folder_after_a_run_gizai_couldnt_follow_to_its_end() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let claude = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &claude, AgentInput::default());
    describe(&st, &task, "FAKE_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    let dir = temp_of(&st, &gizai_core::runs::get(&st.db, &run_id).unwrap());
    gizai_lib::runs::stop(&st, &run_id);
    done.await.unwrap();
    // what a run Gizai couldn't follow to its end (Gizai quit or crashed) left in it, a link to your files too
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("keep.txt"), "mine").unwrap();
    std::fs::create_dir_all(dir.join("cache/deep")).unwrap();
    std::fs::write(dir.join("cache/deep/db.sqlite"), "x").unwrap();
    std::fs::write(dir.join("left.txt"), "x").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link")).unwrap();
    describe(&st, &task, "Go on.");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let err = stderr_of(&run);
    assert!(err.contains(&format!("temp: TMPDIR={} ", dir.display())), "{err}");
    assert!(err.contains("temp entries: \n"), "empty when the run started: {err}");
    assert!(dir.is_dir() && entries(&dir).is_empty(), "and when it ended: {:?}", entries(&dir));
    assert_eq!(std::fs::read_to_string(outside.join("keep.txt")).unwrap(), "mine", "the link was removed, never followed");
}

#[tokio::test]
async fn the_same_card_in_two_data_folders_gets_two_temp_folders() {
    let tmp = tempfile::tempdir().unwrap();
    // Gizai's own tests run the same card names in many data folders at once, all with one /var/tmp/gizai-<uid>
    let shared = tmp.path().join("run-temp");
    let mut dirs = vec![];
    for data in ["one", "two"] {
        let here = tmp.path().join(data);
        std::fs::create_dir(&here).unwrap();
        let mut st = gizai_lib::test_state(&here);
        st.temp_base = shared.clone();
        let repo = git_repo(&here);
        let claude = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        put_agent_on(&st, &claude, AgentInput::default());
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        assert!(run.worktree_path.as_deref().unwrap().ends_with("/KADE-1"), "{:?}", run.worktree_path);
        let dir = temp_of(&st, &run);
        assert!(stderr_of(&run).contains(&format!("temp: TMPDIR={} ", dir.display())), "{data}: {}", stderr_of(&run));
        dirs.push(dir);
    }
    assert_ne!(dirs[0], dirs[1]);
    for d in &dirs {
        assert_eq!(d.parent(), Some(shared.as_path()));
        assert!(d.file_name().unwrap().to_string_lossy().starts_with("KADE-1-"), "{}", d.display());
    }
}

#[tokio::test]
async fn a_temp_base_that_is_a_link_or_open_to_others_is_never_used_the_run_gets_gizai_tmp_and_its_log_says_why() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let claude = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    // a link to a private folder of yours, and a folder others may open
    let mine = tmp.path().join("mine");
    std::fs::create_dir(&mine).unwrap();
    std::fs::set_permissions(&mine, std::fs::Permissions::from_mode(0o700)).unwrap();
    let link = tmp.path().join("base-link");
    std::os::unix::fs::symlink(&mine, &link).unwrap();
    let open = tmp.path().join("base-open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (base, why) in [(&link, "is a link"), (&open, "is open to other users (mode 755, not 700)")] {
        st.temp_base = base.clone();
        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        put_agent_on(&st, &claude, AgentInput::default());
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{why}: {:?}", s.error);
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        let own = own_temp_of(&run);
        let d = own.display();
        let err = stderr_of(&run);
        assert!(err.contains(&format!("temp: TMPDIR={d} TMP={d} TEMP={d}\n")), "{why}: {err}");
        assert!(err.contains("temp entries: \n"), "{why}: {err}");
        let said = notes_of(&st, &run);
        let want = format!("Couldn't use a short temp folder for this run: {} {why}. TMPDIR, TMP and TEMP point at {d} instead", base.display());
        assert!(said.iter().any(|n| n.starts_with(&want)), "{why}: {want} not in {said:?}");
        assert!(prompt_of(&run).contains(&format!("Throwaway files go only in `{d}`")), "{why}: {}", prompt_of(&run));
        // nothing made in it, and .gizai-tmp emptied at the run's end
        assert!(entries(&mine).is_empty() && entries(&open).is_empty(), "{why}");
        assert!(own.is_dir() && entries(&own).is_empty(), "{why}: {:?}", entries(&own));
    }
    assert_eq!(std::fs::metadata(&open).unwrap().permissions().mode() & 0o777, 0o755, "left as it was");
}

#[tokio::test]
async fn a_run_starts_with_an_empty_temp_folder_and_the_folder_goes_with_the_worktree() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let run = gizai_core::runs::list_for_task(&st.db, &task).unwrap().remove(0);
    let wt = PathBuf::from(run.worktree_path.clone().unwrap());
    let dir = temp_of(&st, &run);
    let own = own_temp_of(&run);
    // what a run Gizai couldn't follow to its end (a crash) left behind, in both
    for d in [&dir, &own] {
        std::fs::create_dir_all(d.join("cache/deep")).unwrap();
        std::fs::write(d.join("cache/deep/db.sqlite"), "x").unwrap();
        std::fs::write(d.join("left.txt"), "x").unwrap();
    }

    // the card is done: Settings → Data removes its worktree, never by force, and the temp folder goes with it
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let done = gizai_core::team::get(&st.db, &team_id).unwrap().states.into_iter().find(|s| s.category == "done").unwrap().id;
    gizai_core::tasks::move_to(&st.db, &st.you_id, &task, &done, "").unwrap();
    let listed = gizai_lib::worktrees::list(&st).unwrap();
    let w = listed.iter().find(|w| w.task_id == task).expect("listed");
    assert_eq!(w.uncommitted, 0, "the temp folder isn't uncommitted work");
    let out = gizai_lib::worktrees::remove(&st, std::slice::from_ref(&task)).unwrap();
    assert!(out[0].removed, "{:?}", out[0]);
    assert!(!wt.exists() && !own.exists(), "the worktree and its .gizai-tmp are gone");
    assert!(!dir.exists(), "its temp folder outside the worktree is gone too: {}", dir.display());
    assert_eq!(status(&repo), "");
}

#[tokio::test]
async fn a_link_in_place_of_the_temp_folder_is_removed_never_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let claude = add_cli(&st, "Claude Code (temp)", "claude_code", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &claude, AgentInput::default());
    describe(&st, &task, "FAKE_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    let first = gizai_core::runs::get(&st.db, &run_id).unwrap();
    let (dir, own) = (temp_of(&st, &first), own_temp_of(&first));
    gizai_lib::runs::stop(&st, &run_id);
    done.await.unwrap();
    // a link to a folder outside the worktree, in place of the temp folder and of the worktree's .gizai-tmp
    let outside = tmp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("keep.txt"), "mine").unwrap();
    for d in [&dir, &own] {
        std::fs::remove_dir_all(d).unwrap();
        std::os::unix::fs::symlink(&outside, d).unwrap();
    }
    describe(&st, &task, "Go on.");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    assert!(stderr_of(&run).contains(&format!("temp: TMPDIR={} ", dir.display())), "{}", stderr_of(&run));
    assert!(stderr_of(&run).contains("temp exists: yes"), "{}", stderr_of(&run));
    for d in [&dir, &own] {
        assert!(!std::fs::symlink_metadata(d).unwrap().file_type().is_symlink(), "a real folder now: {}", d.display());
        assert!(entries(d).is_empty(), "{}", d.display());
    }
    assert_eq!(entries(&outside), ["keep.txt"]);
    assert_eq!(std::fs::read_to_string(outside.join("keep.txt")).unwrap(), "mine", "the link's target is left alone");
}

#[tokio::test]
async fn refused_tool_calls_are_saved_on_the_run_shown_as_they_happen_and_returned_by_get_task_and_get_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    describe(&st, &task, "FAKE_REFUSED");
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();

    // the fixture: Claude Code 2.1.289 refused four calls, each with a permission_denied line, then listed them again
    // in its result line's permission_denials; each is saved once, in order, with its reason
    let got: Vec<(&str, &str, &str)> = run.refused.iter().map(|r| (r.tool.as_str(), r.input.as_str(), r.reason.as_str())).collect();
    assert_eq!(got, [
        ("Bash", "cat <<EOF\nhello\nEOF", "Heredoc with unquoted delimiter undergoes shell expansion"),
        ("Bash", "ls /tmp", "ls in '/tmp' was blocked. For security, Claude Code may only list files in the allowed working directories for this session: '/w/repo'."),
        ("Bash", "git status --short > /tmp/ga48-check-b.txt", "Output redirection to '/tmp/ga48-check-b.txt' needs approval. The path is outside the working directories for this session ('/w/repo'). Allowing runs the command as written."),
        ("Write", "/tmp/ga48-check-write.txt", "Path is outside allowed working directories"),
    ]);
    // the run's output marks each one where it happened, once
    use gizai_agents::stream::RunEvent;
    let evs: Vec<RunEvent> = gizai_lib::runs::events_for(&st, &run.id).into_iter().map(|e| e.event).collect();
    assert_eq!(evs.iter().filter(|e| matches!(e, RunEvent::Refused { .. })).count(), 4, "{evs:?}");

    // the Team Lead sees them without the raw logs
    let t = gizai_lib::tools::call(&st, &lead, "get_task", json!({"task": "KADE-1"})).await.unwrap();
    let runs = t["runs"].as_array().unwrap_or_else(|| panic!("{t}"));
    assert_eq!(runs[0]["refused"].as_array().unwrap().len(), 4, "{t}");
    assert_eq!(runs[0]["refused"][1], json!({"tool": "Bash", "input": "ls /tmp", "reason": got[1].2}));
    let a = gizai_lib::tools::call(&st, &lead, "get_agent", json!({"agent": "Backend Agent"})).await.unwrap();
    let recent = &a["recent_runs"][0];
    assert_eq!((recent["task"].as_str(), recent["refused"].as_array().map(Vec::len)), (Some("KADE-1"), Some(4)), "{a}");
    assert_eq!(recent["refused"][3]["tool"], "Write");

    // a run without refusals returns an empty list
    let task2 = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let s2 = gizai_lib::runs::run_once(&st, &task2, None, None).await.unwrap();
    assert!(gizai_core::runs::get(&st.db, &s2.run_id).unwrap().refused.is_empty());
    let t2 = gizai_lib::tools::call(&st, &lead, "get_task", json!({"task": "KADE-2"})).await.unwrap();
    assert_eq!(t2["runs"][0]["refused"], json!([]), "{t2}");
}

#[test]
fn new_agents_start_with_the_longer_list_and_the_two_lists_are_the_same() {
    // src/lib/agents.ts's DEFAULT_TOOLS, the list the agent form starts a new agent with
    let ts = include_str!("../../src/lib/agents.ts");
    let start = ts.find("export const DEFAULT_TOOLS = [").expect("DEFAULT_TOOLS in agents.ts");
    let body = &ts[start..start + ts[start..].find("];").unwrap()];
    let ui: Vec<&str> = body.split('"').skip(1).step_by(2).collect();
    let rust: Vec<&str> = gizai_lib::runs::DEFAULT_TOOLS.to_vec();
    assert_eq!(ui, rust, "the agent form and the run manager start from the same list");
    for helper in ["head", "tail", "wc", "sort", "uniq", "cut", "diff", "grep", "jq", "pwd", "which", "tree"] {
        let rule = format!("Bash({helper}:*)");
        assert!(rust.contains(&rule.as_str()), "{rule} missing");
    }
    // GA-54: and sleep, so an agent can wait in the foreground between checks
    assert!(rust.contains(&"Bash(sleep:*)"), "sleep missing: {rust:?}");
    let mut unique = rust.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), rust.len(), "no rule twice");
}

// ---- QA (GA-48): an agent's own list, the other CLIs' prompts, an answered Continue, refusals before a Stop ----

/// A copy of the fake `claude` whose runs end asking for a decision (needs_decision), so an answered Continue can follow.
fn asking_claude(dir: &Path) -> String {
    let d = dir.join("fake-asks");
    std::fs::create_dir_all(d.join("fixtures")).unwrap();
    let src = Path::new(FAKE_CLAUDE).parent().unwrap().join("fixtures");
    let run = std::fs::read_to_string(src.join("run-ok.jsonl")).unwrap().replace("ready_for_testing", "needs_decision");
    std::fs::write(d.join("fixtures/run-ok.jsonl"), run).unwrap();
    std::fs::copy(src.join("models-init.jsonl"), d.join("fixtures/models-init.jsonl")).unwrap();
    // copied by a child, so this process never holds a script open for writing (ETXTBSY)
    assert!(Command::new("cp").arg(FAKE_CLAUDE).arg(d.join("fake-claude.sh")).status().unwrap().success());
    d.join("fake-claude.sh").to_string_lossy().into_owned()
}

/// The agent's own list (not Gizai's default one) with a command the default list doesn't have.
fn own_list() -> AgentInput {
    AgentInput { allowed_tools: vec!["Bash(make test:*)".into(), "Bash(git status:*)".into()], ..Default::default() }
}

fn section(p: &str) -> &str {
    &p[p.find("## How this run works").unwrap_or_else(|| panic!("no section in {p}"))..]
}

#[tokio::test]
async fn an_answered_continue_also_ends_with_how_this_run_works_with_the_agents_own_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let cli = add_cli(&st, "Claude Code (asks)", "claude_code", &asking_claude(tmp.path()), &["FAKE_TEMP=1"]);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    put_agent_on(&st, &cli, own_list());
    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!(s.outcome.as_deref(), Some("needs_decision"), "{:?}", s.error);
    gizai_core::comments::add(&st.db, &st.you_id, &task, "Use JSON, please", None).unwrap();
    let (id, done) = gizai_lib::runs::continue_answered(&st, &s.run_id).await.unwrap();
    done.await.unwrap();
    let first = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let answered = gizai_core::runs::get(&st.db, &id).unwrap();
    let temp = temp_of(&st, &first).display().to_string();
    let p = prompt_of(&answered);
    assert!(p.contains("ended asking for a decision") && p.contains("Use JSON, please"), "the answered prompt: {p}");
    for (which, p) in [("new", prompt_of(&first)), ("answered", p.clone())] {
        let sec = section(&p);
        for want in ["Nobody can approve anything", "`make test`", "`git status`", &format!("`{temp}`"), "don't try other spellings"] {
            assert!(sec.contains(want), "{which}: {want} missing in {sec}");
        }
        // the agent's own list, not Gizai's default one
        for not in ["`cargo`", "`jq`", "`npm`"] {
            assert!(!sec.contains(not), "{which}: {not} is not in the agent's list: {sec}");
        }
    }
}

#[tokio::test]
async fn codex_gemini_and_other_prompts_end_with_how_this_run_works_with_only_what_holds_for_them() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    for (name, kind) in [("Codex", "codex"), ("Gemini", "gemini"), ("Plain", "other")] {
        let cli = add_cli(&st, name, kind, FAKE_CLI, &[&format!("FAKE_KIND={kind}"), "FAKE_TEMP=1"]);
        let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
        put_agent_on(&st, &cli, own_list());
        let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
        assert_eq!(s.status, "succeeded", "{name}: {:?}", s.error);
        let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
        let p = prompt_of(&run);
        let sec = section(&p);
        let temp = temp_of(&st, &run).display().to_string();
        assert_eq!(p.matches("## How this run works").count(), 1, "{name}: {p}");
        for want in ["Nobody can approve anything during this run", &format!("`{temp}`"), "never in `/tmp`", "don't try other spellings"] {
            assert!(sec.contains(want), "{name}: {want} missing in {sec}");
        }
        // Claude Code's shell rules and its Write tool are Claude Code's only
        for not in ["`$(…)`", "backticks", "`<<EOF`", "Write tool", "write this path, not `$TMPDIR`"] {
            assert!(!sec.contains(not), "{name}: {not} in {sec}");
        }
        // only Gemini is given the agent's commands; Codex's sandbox decides and another CLI gets none
        assert_eq!(sec.contains("`make test`"), kind == "gemini", "{name}: {sec}");
    }
}

#[tokio::test]
async fn refusals_before_a_stop_stay_on_the_run_and_get_task_returns_them() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let team_id = gizai_core::team::list(&st.db).unwrap()[0].id.clone();
    let lead = gizai_core::team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    // Claude Code refuses four calls (a permission_denied line each), then the run is stopped before its result line
    describe(&st, &task, "FAKE_REFUSED_THEN_HANG");
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, None, "manual").await.unwrap();
    use gizai_agents::stream::RunEvent;
    let refused_so_far = || gizai_lib::runs::events_for(&st, &run_id).into_iter().filter(|e| matches!(e.event, RunEvent::Refused { .. })).count();
    let t0 = std::time::Instant::now();
    while refused_so_far() < 4 {
        assert!(t0.elapsed() < Duration::from_secs(15), "the run never showed its four refusals: {}", refused_so_far());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    gizai_lib::runs::stop(&st, &run_id);
    done.await.unwrap();
    let run = gizai_core::runs::get(&st.db, &run_id).unwrap();
    assert_eq!(run.status, "cancelled", "{:?}", run.error);
    let got: Vec<(&str, &str)> = run.refused.iter().map(|r| (r.tool.as_str(), r.input.as_str())).collect();
    assert_eq!(got, [("Bash", "cat <<EOF\nhello\nEOF"), ("Bash", "ls /tmp"), ("Bash", "git status --short > /tmp/ga48-check-b.txt"),
        ("Write", "/tmp/ga48-check-write.txt")]);
    assert!(run.refused.iter().all(|r| !r.reason.is_empty()), "each with Claude Code's reason: {:?}", run.refused);
    let t = gizai_lib::tools::call(&st, &lead, "get_task", json!({"task": "KADE-1"})).await.unwrap();
    assert_eq!(t["runs"][0]["refused"].as_array().map(Vec::len), Some(4), "{t}");
}
