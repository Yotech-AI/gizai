// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-45: an agent's folders end to end, with the fake CLIs (never the real ones): what a task run's command line gets,
// a missing folder skipped with a note in the run log, the Team Lead's tools that can't change the list, and the
// check update_checkout (GA-44) asks before it updates a folder.
use std::path::{Path, PathBuf};

use gizai_core::clis::Cli;
use gizai_core::folders::Folder;
use gizai_core::model::{AgentInput, TaskPatch};
use gizai_core::team;
use serde_json::{Value, json};

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

fn f(path: &Path, access: &str) -> Folder {
    Folder { path: path.display().to_string(), access: access.into() }
}

fn real(p: &Path) -> String {
    p.canonicalize().unwrap().display().to_string()
}

/// A read folder and a read and change folder that are there, and a read folder that isn't.
struct Dirs {
    shared: PathBuf,
    out: PathBuf,
    gone: PathBuf,
}

fn dirs(tmp: &Path) -> Dirs {
    let d = Dirs { shared: tmp.join("shared"), out: tmp.join("out"), gone: tmp.join("gone") };
    std::fs::create_dir_all(&d.shared).unwrap();
    std::fs::create_dir_all(&d.out).unwrap();
    d
}

/// Gives `test_task`'s backend agent these folders (as the agent form does), on the CLI `cli` ("" for Claude Code).
fn give_folders(st: &gizai_lib::AppState, cli: &str, list: Vec<Folder>) -> String {
    let (_, m) = team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    team::update_agent(&st.db, &st.you_id, &m.actor_id, AgentInput { name: m.name.clone(), role_key: "backend".into(), adapter: cli.into(),
        folders: Some(list), ..Default::default() }).unwrap();
    m.actor_id
}

fn stderr_of(run: &gizai_core::model::Run) -> String {
    std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap()
}

/// The fake's `argv:` line, split at the spaces between arguments.
fn argv_line(stderr: &str) -> String {
    stderr.lines().find_map(|l| l.strip_prefix("argv: ")).unwrap_or_else(|| panic!("no argv in {stderr}")).to_string()
}

fn events(st: &gizai_lib::AppState, run_id: &str) -> Vec<Value> {
    gizai_lib::runs::events_for(st, run_id).into_iter().map(|e| serde_json::to_value(&e.event).unwrap()).collect()
}

#[tokio::test]
async fn a_claude_code_run_gets_its_folders_and_skips_a_missing_one_with_a_note_in_the_run_log() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let d = dirs(tmp.path());
    give_folders(&st, "", vec![f(&d.shared, "read"), f(&d.gone, "read"), f(&d.out, "change")]);

    let s = gizai_lib::runs::run_once(&st, &task, None, Some(FAKE_CLAUDE.into())).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "the run still starts and finishes");
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let note = format!("Skipped the folder {}: it's missing, so this run goes without it.", d.gone.display());
    // the log: Gizai's note first (Claude Code's log has no header), then Claude Code's output
    let log = std::fs::read_to_string(&run.log_path).unwrap();
    let first: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(first, json!({"type": "gizai_note", "text": note}));
    let evs = events(&st, &s.run_id);
    assert_eq!(evs[0], json!({"kind": "note", "text": note}), "the Run panel shows the note, read back from the log");
    assert_eq!(evs.len(), 8, "the note and the fake's 7 events");
    // the command line: --add-dir for the two that are there, deny rules for the read one
    let argv = argv_line(&stderr_of(&run));
    let (shared, out) = (real(&d.shared), real(&d.out));
    assert!(argv.contains(&format!("--add-dir {shared} {out}")), "{argv}");
    assert!(argv.ends_with(&format!("--disallowedTools Edit(/{shared}/**) Write(/{shared}/**)")), "{argv}");
    assert!(!argv.contains(&d.gone.display().to_string()), "the missing folder isn't passed: {argv}");
}

#[tokio::test]
async fn a_live_run_shows_the_notes_first() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let d = dirs(tmp.path());
    give_folders(&st, "", vec![f(&d.gone, "change")]);
    gizai_core::tasks::update(&st.db, &st.you_id, &task, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let (run_id, done) = gizai_lib::runs::start(&st, &task, None, Some(FAKE_CLAUDE.into()), "manual").await.unwrap();
    let mut evs = vec![];
    for _ in 0..50 {
        evs = events(&st, &run_id);
        if evs.len() >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(evs[0]["kind"], "note", "{evs:?}");
    assert!(evs[0]["text"].as_str().unwrap().contains("it's missing"), "{evs:?}");
    assert_eq!(evs[1]["kind"], "init", "then the CLI's own events: {evs:?}");
    let seqs: Vec<u64> = gizai_lib::runs::events_for(&st, &run_id).iter().map(|e| e.seq).collect();
    assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>(), "numbered on from the notes");
    gizai_lib::runs::stop(&st, &run_id);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(20), done).await;
}

#[tokio::test]
async fn a_gemini_run_gets_the_read_and_change_folders_and_the_log_says_which_it_left_out() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: "Gemini".into(), kind: "gemini".into(), command: FAKE_CLI.into(), env: vec!["FAKE_KIND=gemini".into()], ..Default::default() });
    let gemini = gizai_lib::clis::save(&st, list).unwrap().into_iter().find(|c| c.cli.name == "Gemini").unwrap().cli.id;
    let d = dirs(tmp.path());
    give_folders(&st, &gemini, vec![f(&d.shared, "read"), f(&d.out, "change")]);

    let s = gizai_lib::runs::run_once(&st, &task, None, None).await.unwrap();
    assert_eq!((s.status.as_str(), s.outcome.as_deref()), ("succeeded", Some("ready_for_testing")));
    let run = gizai_core::runs::get(&st.db, &s.run_id).unwrap();
    let argv = argv_line(&stderr_of(&run));
    assert!(argv.contains(&format!("--include-directories={}", real(&d.out))), "{argv}");
    assert!(!argv.contains(&real(&d.shared)), "Gemini can't keep a folder read only: {argv}");
    let evs = events(&st, &s.run_id);
    assert_eq!(evs[0], json!({"kind": "note", "text": format!("Left out the folder {}: Gemini can't keep a folder read only.", real(&d.shared))}));
    assert_eq!(evs[1]["kind"], "init", "the header is read as before: {evs:?}");
}

struct Lead {
    st: gizai_lib::AppState,
    lead: String,
    _dir: tempfile::TempDir,
}

fn lead() -> Lead {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    // the fake answers Claude Code's model list
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    Lead { st, lead, _dir: dir }
}

fn agent_named(st: &gizai_lib::AppState, name: &str) -> Option<team::Member> {
    team::all_agents(&st.db).unwrap().into_iter().map(|(_, m)| m).find(|m| m.name == name)
}

#[tokio::test]
async fn the_team_leads_create_agent_and_update_agent_cant_change_the_folders() {
    let t = lead();
    let shared = t._dir.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let call = |name: &'static str, args: Value| gizai_lib::tools::call(&t.st, &t.lead, name, args);
    let e = call("create_agent", json!({"name": "Backend Agent", "role": "backend", "folders": [{"path": shared.display().to_string(), "access": "change"}]}))
        .await.unwrap_err();
    assert!(e.contains("folders can't be changed from chat") && e.contains("agent form"), "{e}");
    assert!(agent_named(&t.st, "Backend Agent").is_none(), "nothing was created");

    call("create_agent", json!({"name": "Backend Agent", "role": "backend"})).await.unwrap();
    let m = agent_named(&t.st, "Backend Agent").unwrap();
    assert!(m.folders.is_empty());
    // you set its folders in the agent form
    team::update_agent(&t.st.db, &t.st.you_id, &m.actor_id, AgentInput { name: m.name.clone(), role_key: "backend".into(),
        folders: Some(vec![f(&shared, "read")]), ..Default::default() }).unwrap();
    let want = vec![f(&shared, "read")];
    for folders in [json!([{"path": shared.display().to_string(), "access": "change"}]), json!([]), json!("/")] {
        let e = call("update_agent", json!({"agent": "Backend Agent", "folders": folders})).await.unwrap_err();
        assert!(e.contains("folders can't be changed from chat") && e.contains("Nothing changed"), "{e}");
        assert_eq!(team::agent(&t.st.db, &m.actor_id).unwrap().folders, want);
    }
    // a change to its other settings keeps the list
    call("update_agent", json!({"agent": "Backend Agent", "model": "sonnet"})).await.unwrap();
    let after = team::agent(&t.st.db, &m.actor_id).unwrap();
    assert_eq!((after.model.as_deref(), after.folders), (Some("sonnet"), want));
    // and the Team Lead's own list is as safe
    let e = call("update_agent", json!({"agent": "Team Lead", "folders": [{"path": "/srv", "access": "change"}]})).await.unwrap_err();
    assert!(e.contains("can't be changed from chat"), "{e}");
    // the tool's description says so
    let catalog = gizai_lib::tools::catalog().into_iter().find(|d| d.name == "update_agent").unwrap().description;
    assert!(catalog.contains("set only by the user, in the agent form"), "{catalog}");
}

#[test]
fn update_checkout_may_update_a_folder_only_when_the_team_leads_list_sets_it_to_read_and_change() {
    let t = lead();
    let repos = t._dir.path().join("repos");
    let (kade, shop) = (repos.join("kade"), repos.join("shop"));
    std::fs::create_dir_all(&kade).unwrap();
    std::fs::create_dir_all(&shop).unwrap();
    assert_eq!(gizai_lib::folders::lead_may_update(&t.st, &kade), Ok(()), "no list: GA-44's own rules");
    let m = team::agent(&t.st.db, &t.lead).unwrap();
    team::update_agent(&t.st.db, &t.st.you_id, &t.lead, AgentInput { name: m.name, role_key: "lead".into(), chat_enabled: Some(true),
        folders: Some(vec![f(&kade, "change"), f(&shop, "read")]), ..Default::default() }).unwrap();
    assert_eq!(gizai_lib::folders::lead_may_update(&t.st, &kade), Ok(()));
    assert_eq!(gizai_lib::folders::lead_may_update(&t.st, &kade.join("src")), Ok(()));
    assert!(gizai_lib::folders::lead_may_update(&t.st, &shop.join("src")).is_err(), "inside a read folder");
    let e = gizai_lib::folders::lead_may_update(&t.st, &shop).unwrap_err();
    assert!(e.contains("is set to read in the Team Lead's folders") && e.contains("nothing changed"), "{e}");
    assert_eq!(gizai_lib::folders::lead_may_update(&t.st, t._dir.path().join("elsewhere").as_path()), Ok(()));
}

#[test]
fn the_forms_check_refuses_gizais_data_folder_and_warns_about_a_main_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let check = |folder: Folder| gizai_lib::folders::check(&st, &[folder]).remove(0);
    let checks = [check(f(&st.data_dir.join("worktrees"), "read")), check(f(&repo, "change")), check(f(&repo, "read")),
                  check(Folder { path: "~/.ssh".into(), access: "read".into() })];
    assert!(checks[0].error.as_deref().unwrap().contains("Gizai's data folder"), "{checks:?}");
    assert_eq!(checks[1].error, None);
    assert!(checks[1].warning.as_deref().unwrap().starts_with("This is Kade's main checkout"), "{checks:?}");
    assert_eq!((&checks[2].error, &checks[2].warning), (&None, &None), "reading it is fine: {checks:?}");
    assert!(checks[3].error.as_deref().unwrap().contains("keys or logins"), "{checks:?}");
}
