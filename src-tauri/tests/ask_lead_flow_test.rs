//! GA-70 end to end with fake Claude Codes (never the real one): a task agent's question goes to the Team Lead first.
//! The Backend Agent runs on fake-claude.sh (FAKE_ASKS in the card asks a question, FAKE_RUN_FOR_ME asks you to run
//! commands); the Team Lead runs on a fake of its own that answers by a word in the card's description, which its prompt
//! quotes: LEAD_ANSWER answers (and names a shared note), LEAD_AGAIN answers with FAKE_ASKS so the continued agent asks
//! again, LEAD_ESCALATE asks you, LEAD_CRASH fails, LEAD_SILENT ends without a result line, LEAD_LOOP makes tool calls past
//! the limit, LEAD_GATE waits for a file first. Answered: the comment is on the card, the answer is in memory and the
//! agent carries on in its session with the answer as the Continue note. Escalated, failed, timed out, silent, a second
//! question, the step off, a paused Team Lead, no MCP helper and a run_for_me result: the Inbox.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use gizai_core::clis::Cli;
use gizai_core::model::{AgentInput, Run, TaskPatch};
use gizai_core::{comments, memory, questions, runs as core_runs, tasks, team};
use gizai_lib::ask_lead::{self, Settled, Verdict};
use gizai_mcp::Tools;
use serde_json::json;

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
/// The question fake-claude.sh asks with FAKE_ASKS (fixtures/run-asks.jsonl).
const QUESTION: &str = "CSV or JSON for the export?";

/// The Team Lead's Claude Code on a question: keeps its arguments and prompt in fake-lead.log next to itself, then
/// answers by a word in the prompt.
const LEAD_FAKE: &str = r##"#!/usr/bin/env bash
here="$(cd "$(dirname "$0")" && pwd)"
prompt="$(cat)"
{ printf 'argv>>'; printf '%s\n' "$@"; printf '<<argv\nprompt>>%s<<prompt\n' "$prompt"; } >> "$here/fake-lead.log"
printf 'auto memory: %s\n' "${CLAUDE_CODE_DISABLE_AUTO_MEMORY-unset}" >> "$here/fake-lead.log"
init='{"type":"system","subtype":"init","session_id":"L1","model":"claude-opus-5-5","tools":["Read","Glob","Grep"]}'
say() {
  echo "$init"
  printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"%s"}]},"session_id":"L1"}\n' "$1"
  printf '{"type":"result","subtype":"success","is_error":false,"result":"%s","total_cost_usd":0.03,"num_turns":2,"session_id":"L1","usage":{"input_tokens":3000,"output_tokens":200}}\n' "$1"
}
case "$prompt" in *LEAD_GATE*)
  n=0
  while [ ! -e "$here/lead-go" ] && [ $n -lt 300 ]; do sleep 0.1; n=$((n+1)); done ;;
esac
case "$prompt" in
  *LEAD_CRASH*) echo "error: the lead fake was told to crash" >&2; exit 1 ;;
  *LEAD_SILENT*) say 'I looked in memory and found nothing.'; exit 0 ;;
  *LEAD_LOOP*)
    trap 'exit 130' INT TERM
    lines=("$init")
    n=0
    while [ $n -lt 70 ]; do
      printf -v line '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t%s","name":"Grep","input":{"pattern":"csv"}}]},"session_id":"L1"}' "$n"
      lines+=("$line")
      n=$((n+1))
    done
    # its lines come from the foreground child that then waits, perl, as in fake-claude.sh's print_and_wait (GA-89)
    perl -e '$| = 1; print map { "$_\n" } @ARGV; sleep 30' -- "${lines[@]}"
    exit 0 ;;
  *LEAD_ESCALATE*) say 'This is about money.\nGIZAI_RESULT: {\"outcome\":\"escalated\",\"reason\":\"the client pays per export, so it is about money\",\"options\":[\"CSV for every invoice\",\"one JSON file a month\"],\"advice\":\"CSV: the accountant uses Excel\"}'; exit 0 ;;
  *LEAD_AGAIN*) say 'Memory has it.\nGIZAI_RESULT: {\"outcome\":\"answered\",\"answer\":\"Use CSV. FAKE_ASKS\"}'; exit 0 ;;
  *LEAD_ANSWER*) say 'Standards/Exports says CSV.\nGIZAI_RESULT: {\"outcome\":\"answered\",\"answer\":\"Use CSV with semicolons, as Standards/Exports says.\",\"memory\":{\"path\":\"Standards/Exports\",\"text\":\"Invoice exports are CSV with semicolons\"}}'; exit 0 ;;
esac
say 'No idea.'
"##;

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

/// Adds a Claude Code CLI in Settings → Coding CLIs and returns its id.
fn add_cli(st: &gizai_lib::AppState, name: &str, command: &str, env: &[&str]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: "claude_code".into(), command: command.into(), env: env.iter().map(|e| e.to_string()).collect(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Every column Manual: only the test starts runs.
fn all_manual(st: &gizai_lib::AppState) {
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    for s in team::get(&st.db, &team_id).unwrap().states {
        gizai_core::columns::set_column(&st.db, &st.you_id, &s.id, gizai_core::columns::ColumnInput { auto: Some(false), ..Default::default() }).unwrap();
    }
}

struct T {
    st: gizai_lib::AppState,
    task: String,
    be: String,
    lead: String,
    /// The folder of the Team Lead's fake (its log and the gate file).
    lead_dir: PathBuf,
}

/// Card KADE-1 with `description`, the Backend Agent on fake-claude.sh (it prints its prompt), a Team Lead with Chat on
/// on its own fake, the MCP helper "found", every column Manual.
fn setup(tmp: &Path, description: &str) -> T {
    let mut st = gizai_lib::test_state(tmp);
    let lead_dir = tmp.join("lead-cli");
    std::fs::create_dir_all(&lead_dir).unwrap();
    let fake = lead_dir.join("claude");
    write_script(&fake, LEAD_FAKE);
    // The fake ignores --mcp-config: any file will do as the helper.
    st.mcp_shim = Some(fake.clone());
    let repo = git_repo(tmp);
    let task = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    tasks::update(&st.db, &st.you_id, &task, TaskPatch { description_md: Some(description.into()), ..Default::default() }).unwrap();
    let be_cli = add_cli(&st, "Claude Code (prints its prompt)", FAKE_CLAUDE, &["FAKE_TEMP=1"]);
    let (_, be) = team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    team::update_agent(&st.db, &st.you_id, &be.actor_id, AgentInput { name: be.name.clone(), role_key: "backend".into(), adapter: be_cli, ..Default::default() }).unwrap();
    let lead_cli = add_cli(&st, "Lead CC", fake.to_str().unwrap(), &[]);
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), adapter: lead_cli,
        chat_enabled: Some(true), ..Default::default() }).unwrap();
    all_manual(&st);
    T { st, task, be: be.actor_id, lead, lead_dir }
}

impl T {
    /// The Backend Agent's run on the card, to its end.
    async fn run(&self) -> Run {
        let s = gizai_lib::runs::run_once(&self.st, &self.task, None, None).await.unwrap();
        core_runs::get(&self.st.db, &s.run_id).unwrap()
    }
    /// How the Team Lead's work on the question `asked` ended.
    async fn settled(&self, asked: &str) -> Settled {
        let h = ask_lead::take(&self.st, asked).unwrap_or_else(|| panic!("the Team Lead didn't take the question of {asked}"));
        tokio::time::timeout(Duration::from_secs(60), h).await.expect("the Team Lead's work ended").unwrap()
    }
    fn card(&self) -> gizai_core::model::Task {
        tasks::get(&self.st.db, &self.task).unwrap()
    }
    fn in_inbox(&self) -> bool {
        tasks::needs_you(&self.st.db, &self.st.you_id).unwrap().iter().any(|t| t.id == self.task)
    }
    /// The card's notification (`notifications::inbox`), if it has one.
    fn notice(&self) -> Option<String> {
        gizai_lib::notifications::inbox(&self.st.db, &self.st.you_id).unwrap().into_iter()
            .find(|i| i.notice.route == format!("#/task/{}", self.task)).map(|i| i.notice.title)
    }
    fn leads_comments(&self) -> Vec<gizai_core::model::Comment> {
        let mut c: Vec<_> = comments::list(&self.st.db, &self.task).unwrap().into_iter().filter(|c| c.author_id == self.lead).collect();
        c.sort_by_key(|c| c.created_at);
        c
    }
    fn lead_runs(&self) -> Vec<Run> {
        core_runs::list_for_agent(&self.st.db, &self.lead, 50).unwrap()
    }
    fn you(&self) -> String {
        gizai_core::users::list(&self.st.db).unwrap().into_iter().find(|p| p.id == self.st.you_id).unwrap().name
    }
    /// What the Team Lead's fake was given, all calls.
    fn lead_log(&self) -> String {
        std::fs::read_to_string(self.lead_dir.join("fake-lead.log")).unwrap_or_default()
    }
}

/// The prompt fake-claude.sh was given (it writes it to stderr with FAKE_TEMP=1).
fn prompt_of(run: &Run) -> String {
    let err = std::fs::read_to_string(Path::new(&run.log_path).with_extension("stderr.log")).unwrap();
    let start = err.find("prompt>>").unwrap_or_else(|| panic!("no prompt in {err}")) + "prompt>>".len();
    let end = err[start..].find("<<prompt").unwrap() + start;
    err[start..end].to_string()
}

/// Waits until `f` holds (at most 20 s).
async fn until(what: &str, f: impl Fn() -> bool) {
    let t0 = Instant::now();
    while !f() {
        assert!(t0.elapsed() < Duration::from_secs(20), "timed out waiting: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---- answered ----

#[tokio::test]
async fn answered_the_comment_is_on_the_card_the_answer_in_memory_and_the_agent_carries_on_in_its_session() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "Export the invoices. FAKE_ASKS LEAD_GATE LEAD_ANSWER");
    let asked = t.run().await;
    assert_eq!(asked.outcome.as_deref(), Some("needs_decision"));
    // meanwhile: on hold, with the Team Lead, out of the Inbox and without a notification
    let card = t.card();
    assert_eq!((card.hold.as_deref(), card.with_lead), (Some("needs_decision"), true));
    assert!(!t.in_inbox() && t.notice().is_none());
    until("the Team Lead's run on the question started", || t.lead_runs().iter().any(|r| r.status == "running")).await;
    let lr = t.lead_runs().remove(0);
    assert_eq!((lr.trigger.as_str(), lr.task_id.as_deref(), lr.question_task_id.as_deref(), lr.role_key.as_deref()),
               ("question", None, Some(t.task.as_str()), Some("lead")));
    assert_eq!(core_runs::list_for_task(&t.st.db, &t.task).unwrap()[0].id, asked.id, "the card's latest run is still the agent's");
    assert!(core_runs::get(&t.st.db, &asked.id).unwrap().lead.is_some_and(|l| l.state == "asking"));
    std::fs::write(t.lead_dir.join("lead-go"), "").unwrap();

    let s = t.settled(&asked.id).await;
    assert_eq!(s.state, "answered", "{s:?}");
    let (cont_id, done) = s.continued.expect("the agent continued");
    let summary = done.await.unwrap();
    assert_eq!((summary.status.as_str(), summary.outcome.as_deref()), ("succeeded", Some("ready_for_testing")), "{:?}", summary.error);
    // the answer is the Team Lead's comment on the card, and the Continue note in the agent's own session
    let answer = "Use CSV with semicolons, as Standards/Exports says.";
    let c = t.leads_comments();
    assert_eq!(c.len(), 1, "{c:?}");
    assert!(c[0].body_md.contains(answer), "{}", c[0].body_md);
    let cont = core_runs::get(&t.st.db, &cont_id).unwrap();
    assert_eq!((cont.agent_id.as_str(), cont.session_id.as_deref()), (t.be.as_str(), asked.session_id.as_deref()), "the same session");
    let p = prompt_of(&cont);
    assert!(p.contains(answer), "{p}");
    assert!(p.contains("Team Lead"), "the note says whose it is: {p}");
    // the card carried on and is out of the Inbox
    let card = t.card();
    assert_eq!(card.hold, None);
    assert!(matches!(card.state_category.as_str(), "testing" | "review"), "{}", card.state_name);
    // kept in the shared note it named, linked to the card
    let note = memory::find(&t.st.db, "Standards/Exports").unwrap().expect("the note");
    assert!(note.body_md.contains("Invoice exports are CSV with semicolons (KADE-1)"), "{}", note.body_md);
    // the asking run says who answered and what the Team Lead's look cost (on its budget)
    let l = core_runs::get(&t.st.db, &asked.id).unwrap().lead.unwrap();
    assert_eq!((l.state.as_str(), l.note.as_deref(), l.run_id.as_deref(), l.cost_usd_micros), ("answered", Some("Standards/Exports"), Some(lr.id.as_str()), 30_000));
    assert_eq!(l.answer.as_deref(), Some(answer));
    let lr = core_runs::get(&t.st.db, &lr.id).unwrap();
    assert_eq!((lr.status.as_str(), lr.cost_usd_micros, lr.agent_id.as_str()), ("succeeded", 30_000, t.lead.as_str()));
    assert_eq!(lr.summary_md.as_deref(), Some("Standards/Exports says CSV."), "its message without the result line");
}

#[tokio::test]
async fn the_team_leads_run_reads_the_question_and_has_the_always_escalate_rules_and_only_read_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "Export the invoices. FAKE_ASKS LEAD_ESCALATE");
    let asked = t.run().await;
    t.settled(&asked.id).await;
    let log = t.lead_log();
    // the question, the card and who asks
    assert!(log.contains(&format!("Backend Agent (backend) ended its run on KADE-1 \"Export invoices as CSV\"")), "{log}");
    assert!(log.contains(&format!("> {QUESTION}")) && log.contains("> 1. Which format should the export use?"), "{log}");
    assert!(log.contains("> Export the invoices. FAKE_ASKS LEAD_ESCALATE"), "the description: {log}");
    assert!(log.contains("Look in memory first."), "{log}");
    // its rules: always leave money, scope, deadlines, client messages, security and deleting to you
    assert!(log.contains("Always leave it to") && log.contains("money, scope, deadlines, messages to clients, security, deleting anything, and anything you can't find"),
            "{log}");
    assert!(log.contains("GIZAI_RESULT: {\"outcome\":\"answered\"") && log.contains("GIZAI_RESULT: {\"outcome\":\"escalated\""), "{log}");
    // read-only on code: Read, Glob and Grep, the gizai tools, no saved session
    let argv: Vec<&str> = log.split("argv>>").nth(1).unwrap().split("<<argv").next().unwrap().lines().collect();
    let after = |flag: &str| argv.iter().position(|a| *a == flag).map(|i| argv[i + 1]);
    assert_eq!(after("--tools"), Some("Read,Glob,Grep"), "{argv:?}");
    assert_eq!(after("--allowedTools"), Some("mcp__gizai"), "{argv:?}");
    assert!(argv.contains(&"--no-session-persistence") && argv.contains(&"--strict-mcp-config"), "{argv:?}");
    assert!(after("--mcp-config").is_some(), "{argv:?}");
}

#[tokio::test]
async fn the_team_leads_run_on_a_question_starts_claude_code_with_its_own_memory_off() {
    // GA-85: like task runs, chat answers and board checks, also when the Team Lead's account lines say otherwise.
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "Export the invoices. FAKE_ASKS LEAD_ESCALATE");
    let mut list = gizai_core::clis::list(&t.st.db).unwrap();
    list.iter_mut().find(|c| c.name == "Lead CC").unwrap().env = vec!["CLAUDE_CODE_DISABLE_AUTO_MEMORY=0".into()];
    gizai_lib::clis::save(&t.st, list).unwrap();
    let asked = t.run().await;
    t.settled(&asked.id).await;
    let log = t.lead_log();
    assert!(log.contains("auto memory: 1\n") && !log.contains("auto memory: 0") && !log.contains("auto memory: unset"), "{log}");
}

#[tokio::test]
async fn in_a_question_the_team_lead_gets_only_the_tools_that_read_memory_and_cards() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS");
    let tools = gizai_lib::tools::GizaiTools { st: t.st.clone(), actor: t.lead.clone(), thread: None, check: None, run: None, question: Some("asked".into()) };
    let listed: Vec<String> = tools.list().into_iter().map(|d| d.name).collect();
    let all: Vec<String> = gizai_lib::tools::catalog().into_iter().map(|d| d.name).collect();
    for name in ["memory_search", "memory_read", "memory_list", "get_task", "read_doc", "list_docs", "check_board"] {
        assert!(listed.iter().any(|n| n == name), "{name} in {listed:?}");
    }
    for name in ["comment_on_task", "update_task", "move_task", "memory_write", "memory_append", "memory_move", "continue_agent_run",
                 "start_agent_run", "start_chat", "create_task", "update_checkout"] {
        assert!(all.iter().any(|n| n == name), "{name} is a tool");
        assert!(!listed.iter().any(|n| n == name), "{name} not in {listed:?}");
        let e = tools.call(name, json!({})).await.unwrap_err();
        assert!(e.starts_with(&format!("{name} can't be used while you look at an agent's question")), "{e}");
    }
    assert!(tools.list().iter().all(|d| d.read_only));
    let got = tools.call("get_task", json!({"task": "KADE-1"})).await.unwrap();
    assert_eq!(got["task"]["with_team_lead"], false);
    // a chat or board check token isn't limited this way
    let chat = gizai_lib::tools::GizaiTools { st: t.st.clone(), actor: t.lead.clone(), thread: None, check: None, run: None, question: None };
    assert_eq!(chat.list().len(), all.len());
}

// ---- escalated, and what goes to you ----

#[tokio::test]
async fn escalated_the_inbox_with_the_reason_the_options_and_the_advice_and_your_answer_is_kept_in_memory() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS LEAD_ESCALATE");
    let asked = t.run().await;
    let s = t.settled(&asked.id).await;
    assert_eq!((s.state.as_str(), s.reason.as_deref()), ("escalated", Some("the client pays per export, so it is about money")));
    assert!(s.continued.is_none());
    let card = t.card();
    assert_eq!((card.hold.as_deref(), card.hold_reason.as_deref(), card.with_lead),
               (Some("needs_decision"), Some("Team Lead escalated to you: the client pays per export, so it is about money"), false));
    assert!(t.in_inbox());
    assert_eq!(t.notice().as_deref(), Some("KADE-1 is on hold: Team Lead escalated to you: the client pays per export, so it is about money"));
    let c = t.leads_comments();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].body_md, format!("Needs {}: the client pays per export, so it is about money\n\nOptions:\n1. CSV for every invoice\n2. one JSON file a month\n\n\
                                      My advice: CSV: the accountant uses Excel", t.you()));
    let l = core_runs::get(&t.st.db, &asked.id).unwrap().lead.unwrap();
    assert_eq!((l.state.as_str(), l.comment_id.as_deref(), l.cost_usd_micros), ("escalated", Some(c[0].id.as_str()), 30_000));
    // the Team Lead's comment is no answer: Continue still waits for yours
    let e = gizai_lib::runs::continue_answered(&t.st, &asked.id).await.unwrap_err();
    assert!(e.contains("nobody has answered"), "{e}");
    // you answer and continue: the agent carries on, and the Team Lead keeps your answer in memory, linked to the card
    tokio::time::sleep(Duration::from_millis(5)).await;
    comments::add(&t.st.db, &t.st.you_id, &t.task, "CSV for every invoice.", None).unwrap();
    let (_, done) = gizai_lib::runs::continue_answered(&t.st, &asked.id).await.unwrap();
    assert_eq!(done.await.unwrap().outcome.as_deref(), Some("ready_for_testing"));
    let body = memory::find(&t.st.db, "Decisions/Kade").unwrap().expect("Decisions/Kade").body_md;
    assert!(body.contains(&format!("(KADE-1, answered by {}): {QUESTION} → CSV for every invoice.", t.you())), "{body}");
    let l = core_runs::get(&t.st.db, &asked.id).unwrap().lead.unwrap();
    assert!(l.learned && l.note.as_deref() == Some("Decisions/Kade"), "{l:?}");
}

#[tokio::test]
async fn a_failed_a_silent_and_a_timed_out_team_lead_run_send_the_question_to_you() {
    for (word, status, reason) in [
        ("LEAD_CRASH", "failed", "its look at the question failed ("),
        ("LEAD_SILENT", "succeeded", "it ended without an answer"),
        ("LEAD_LOOP", "timed_out", "its look at the question stopped at the limit (15 min or 60 tool calls)"),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let t = setup(tmp.path(), &format!("FAKE_ASKS {word}"));
        let asked = t.run().await;
        let s = t.settled(&asked.id).await;
        assert_eq!(s.state, "escalated", "{word}: {s:?}");
        assert!(s.reason.as_deref().unwrap().starts_with(reason), "{word}: {:?}", s.reason);
        let card = t.card();
        assert!(card.hold_reason.as_deref().unwrap().starts_with(&format!("Team Lead escalated to you: {reason}")), "{word}: {:?}", card.hold_reason);
        assert!(t.in_inbox() && !card.with_lead && t.notice().is_some(), "{word}");
        let c = t.leads_comments();
        assert_eq!(c.len(), 1, "{word}");
        assert!(c[0].body_md.starts_with(&format!("Needs {}: ", t.you())) && c[0].body_md.ends_with("so it is yours to decide."), "{word}: {}", c[0].body_md);
        let lr = t.lead_runs().remove(0);
        assert_eq!((lr.trigger.as_str(), lr.status.as_str()), ("question", status), "{word}: {:?}", lr.error);
        assert!(s.continued.is_none(), "{word}");
    }
}

#[tokio::test]
async fn an_answer_the_agent_cant_be_continued_with_goes_to_you_with_the_answer() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS LEAD_GATE LEAD_ANSWER");
    let asked = t.run().await;
    // its worktree is gone before the Team Lead answers: Continue can't work
    std::fs::remove_dir_all(asked.worktree_path.as_deref().unwrap()).unwrap();
    std::fs::write(t.lead_dir.join("lead-go"), "").unwrap();
    let s = t.settled(&asked.id).await;
    assert_eq!(s.state, "escalated", "{s:?}");
    assert!(s.continued.is_none());
    let reason = s.reason.unwrap();
    assert!(reason.starts_with("it answered, but Gizai couldn't continue Backend Agent with the answer ("), "{reason}");
    let card = t.card();
    assert_eq!(card.hold_reason.as_deref(), Some(format!("Team Lead escalated to you: {reason}").as_str()));
    assert!(t.in_inbox() && !card.with_lead);
    let c = t.leads_comments();
    assert_eq!(c.len(), 1, "{c:?}");
    assert!(c[0].body_md.contains("My answer:\n\n> Use CSV with semicolons, as Standards/Exports says.")
            && c[0].body_md.ends_with("Clear the hold and press Run: Backend Agent starts again and reads this."), "{}", c[0].body_md);
    assert_eq!(core_runs::list_for_task(&t.st.db, &t.task).unwrap().len(), 1, "no continued run");
}

#[tokio::test]
async fn a_card_you_took_over_while_the_team_lead_looked_is_left_to_you() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS LEAD_GATE LEAD_ANSWER");
    let asked = t.run().await;
    // you clear the hold before the Team Lead has answered
    tasks::update(&t.st.db, &t.st.you_id, &t.task, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    std::fs::write(t.lead_dir.join("lead-go"), "").unwrap();
    let s = t.settled(&asked.id).await;
    assert_eq!(s.state, "dropped", "{s:?}");
    assert!(s.continued.is_none());
    assert_eq!(t.card().hold, None, "no hold set again");
    assert!(t.leads_comments().is_empty(), "nothing posted");
    assert!(memory::find(&t.st.db, "Standards/Exports").unwrap().is_none(), "nothing kept");
    assert_eq!(core_runs::get(&t.st.db, &asked.id).unwrap().lead.unwrap().state, "dropped");
    assert_eq!(core_runs::list_for_task(&t.st.db, &t.task).unwrap().len(), 1, "no continued run");
    // the Team Lead's run still counts (its cost is on its budget)
    assert_eq!(t.lead_runs().remove(0).cost_usd_micros, 30_000);
}

#[tokio::test]
async fn a_second_question_after_a_team_lead_answer_goes_to_the_inbox() {
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS LEAD_AGAIN");
    let asked = t.run().await;
    let s = t.settled(&asked.id).await;
    assert_eq!(s.state, "answered");
    let (cont_id, done) = s.continued.unwrap();
    // the answer has FAKE_ASKS in it: the continued run asks again
    assert_eq!(done.await.unwrap().outcome.as_deref(), Some("needs_decision"));
    assert!(ask_lead::take(&t.st, &cont_id).is_none(), "the Team Lead doesn't take it");
    let l = core_runs::get(&t.st.db, &cont_id).unwrap().lead.unwrap();
    assert_eq!((l.state.as_str(), l.reason.as_deref()), ("limit", Some("the Team Lead answered this card's last question")));
    let card = t.card();
    assert_eq!((card.hold.as_deref(), card.with_lead), (Some("needs_decision"), false));
    assert!(t.in_inbox() && t.notice().is_some());
    assert_eq!(t.lead_runs().len(), 1, "one Team Lead run on the card");
}

#[tokio::test]
async fn the_step_off_a_paused_team_lead_and_a_run_for_me_result_go_to_the_inbox_at_once() {
    // Settings → Runs → Ask the Team Lead first, off
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS LEAD_ANSWER");
    questions::set_enabled(&t.st.db, false).unwrap();
    let asked = t.run().await;
    assert!(ask_lead::take(&t.st, &asked.id).is_none());
    let card = t.card();
    assert_eq!((card.hold.as_deref(), card.hold_reason.as_deref(), card.with_lead), (Some("needs_decision"), Some(QUESTION), false));
    assert!(t.in_inbox() && t.notice().is_some());
    assert!(core_runs::get(&t.st.db, &asked.id).unwrap().lead.is_none());
    assert!(t.lead_runs().is_empty() && t.leads_comments().is_empty() && t.lead_log().is_empty());

    // the Team Lead paused
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_ASKS LEAD_ANSWER");
    team::set_agent_status(&t.st.db, &t.st.you_id, &t.lead, "paused").unwrap();
    let asked = t.run().await;
    assert!(ask_lead::take(&t.st, &asked.id).is_none());
    assert!(t.in_inbox() && !t.card().with_lead);
    assert!(t.lead_runs().is_empty() && t.lead_log().is_empty());

    // Run this for me (GA-31): commands for you, not a question
    let tmp = tempfile::tempdir().unwrap();
    let t = setup(tmp.path(), "FAKE_RUN_FOR_ME LEAD_ANSWER");
    let asked = t.run().await;
    assert_eq!(asked.run_for_me.len(), 2);
    assert!(ask_lead::take(&t.st, &asked.id).is_none());
    let card = t.card();
    assert!(t.in_inbox() && !card.with_lead && card.run_for_me.len() == 2);
    assert!(t.lead_runs().is_empty() && t.lead_log().is_empty());
}

#[tokio::test]
async fn without_the_mcp_helper_the_question_goes_to_you_as_before_without_a_comment() {
    let tmp = tempfile::tempdir().unwrap();
    let mut t = setup(tmp.path(), "FAKE_ASKS LEAD_ANSWER");
    t.st.mcp_shim = None;
    let asked = t.run().await;
    let s = t.settled(&asked.id).await;
    assert_eq!(s.state, "skipped");
    assert!(s.reason.as_deref().unwrap().contains("gizai-mcp helper is missing"), "{:?}", s.reason);
    let card = t.card();
    assert_eq!((card.hold.as_deref(), card.hold_reason.as_deref(), card.with_lead), (Some("needs_decision"), Some(QUESTION), false));
    assert!(t.in_inbox());
    assert!(t.leads_comments().is_empty() && t.lead_runs().is_empty() && t.lead_log().is_empty());
    let l = core_runs::get(&t.st.db, &asked.id).unwrap().lead.unwrap();
    assert_eq!(l.state, "skipped");
}

// ---- the Team Lead's result line ----

#[test]
fn the_result_line_answers_or_escalates_and_anything_else_is_no_answer() {
    let answered = ask_lead::verdict("Found it.\nGIZAI_RESULT: {\"outcome\":\"answered\",\"answer\":\" CSV \",\"memory\":{\"path\":\"Decisions/Exports\",\"text\":\"CSV\"}}");
    assert_eq!(answered, Some(Verdict::Answered { answer: "CSV".into(), path: Some("Decisions/Exports".into()), text: Some("CSV".into()) }));
    assert_eq!(ask_lead::verdict("GIZAI_RESULT: {\"outcome\":\"answered\",\"answer\":\"CSV\"}"),
               Some(Verdict::Answered { answer: "CSV".into(), path: None, text: None }));
    assert_eq!(ask_lead::verdict("GIZAI_RESULT: {\"outcome\":\"escalated\",\"reason\":\"money\",\"options\":[\"a\",\" \",\"b\"],\"advice\":\"a\"}"),
               Some(Verdict::Escalated { reason: "money".into(), options: vec!["a".into(), "b".into()], advice: "a".into() }));
    // a needs_decision line counts as escalated, with its summary as the reason
    assert_eq!(ask_lead::verdict("GIZAI_RESULT: {\"outcome\":\"needs_decision\",\"summary\":\"scope\",\"issues\":[]}"),
               Some(Verdict::Escalated { reason: "scope".into(), options: vec![], advice: String::new() }));
    // the last line counts
    assert!(matches!(ask_lead::verdict("GIZAI_RESULT: {\"outcome\":\"answered\",\"answer\":\"x\"}\nGIZAI_RESULT: {\"outcome\":\"escalated\"}"),
                     Some(Verdict::Escalated { .. })));
    // no answer: no line, not JSON, an empty answer, another outcome
    for text in ["No idea.", "GIZAI_RESULT: {nope", "GIZAI_RESULT: {\"outcome\":\"answered\",\"answer\":\"  \"}", "GIZAI_RESULT: {\"outcome\":\"qa_pass\"}"] {
        assert_eq!(ask_lead::verdict(text), None, "{text}");
    }
}
