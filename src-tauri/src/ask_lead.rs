//! Agents ask the Team Lead before a person (GA-70), the app's half. When a task agent's question went to the Team Lead
//! (`gizai_core::questions::hand_over_in`, `GateResult::lead`), the Team Lead's run on it (`chat::question_once`) reads
//! it, looks in memory and ends with a result line. Gizai then does what that line says: an answer goes on the card as the
//! Team Lead's comment and continues the agent's session (Continue with a message, GA-31), and is kept in memory, linked
//! to the card; an escalation puts a comment "Needs <you>: why, the options, my advice" on the card, which lands in the
//! Inbox. An error, a timeout, a run without its result line, and an answer the agent can't be continued with all go to
//! you too. When you answer a question the Team Lead escalated, the Team Lead keeps your answer in memory as the agent
//! starts again (`learn`).
use std::time::{Duration, Instant};

use gizai_core::questions::{self, Question};
use gizai_core::{comments, ids, team};

use crate::AppState;
use crate::runs::{Note, RunSummary};

/// A Continue that only has to wait ("Runs at once" is full, the agent is paused, …) is tried again this often…
const RETRY_EVERY: Duration = Duration::from_secs(30);
/// …this many times (ten minutes); then the question goes to you, with the answer.
const RETRIES: u32 = 20;
/// The card's description, acceptance criteria and comments are cut to this many characters in the question's prompt.
const DESCRIPTION_CHARS: usize = 3000;
const COMMENT_CHARS: usize = 800;
/// The last this many comments go into the prompt.
const COMMENTS: usize = 8;

/// How a question the Team Lead took ended (`take`).
#[derive(Debug)]
pub struct Settled {
    /// answered, escalated, dropped (the card moved on before the Team Lead was done) or skipped (the Team Lead can't
    /// run here: the question went to you as before, without a comment).
    pub state: String,
    /// The Team Lead's run on the question, when it started.
    pub run_id: Option<String>,
    /// Escalated: why.
    pub reason: Option<String>,
    /// Answered: the agent's continued run, and its handle.
    pub continued: Option<(String, tokio::task::JoinHandle<RunSummary>)>,
}

/// What the Team Lead's result line says about a question.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// The answer for the agent, and where to keep it in memory when the Team Lead names a note (a shared one) and a line.
    Answered { answer: String, path: Option<String>, text: Option<String> },
    /// The user decides: why, the options and the Team Lead's advice.
    Escalated { reason: String, options: Vec<String>, advice: String },
}

/// The last `GIZAI_RESULT:` line of the Team Lead's message: `answered` with a non-empty `answer` (and an optional
/// `memory` object with `path` and `text`), or `escalated` (`reason`, `options`, `advice`; a `needs_decision` line counts
/// as escalated, its summary as the reason). None: no such line, a line that isn't JSON, another outcome, or an answer
/// without text.
pub fn verdict(text: &str) -> Option<Verdict> {
    let line = text.lines().rev().map(str::trim_start).find(|l| l.starts_with("GIZAI_RESULT:"))?;
    let v: serde_json::Value = serde_json::from_str(line["GIZAI_RESULT:".len()..].trim()).ok()?;
    let text_of = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
    match v.get("outcome")?.as_str()?.trim() {
        "answered" => {
            let answer = text_of(&v, "answer")?;
            let memory = v.get("memory").filter(|m| m.is_object());
            Some(Verdict::Answered { answer, path: memory.and_then(|m| text_of(m, "path")), text: memory.and_then(|m| text_of(m, "text")) })
        }
        "escalated" | "needs_decision" => {
            let options = match v.get("options") {
                Some(serde_json::Value::Array(items)) => items.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
                Some(serde_json::Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
                _ => vec![],
            };
            let reason = text_of(&v, "reason").or_else(|| text_of(&v, "summary")).unwrap_or_else(|| "it needs your decision".into());
            Some(Verdict::Escalated { reason, options, advice: text_of(&v, "advice").unwrap_or_default() })
        }
        _ => None,
    }
}

/// The user's name, as the prompt and the comments say it.
fn you(st: &AppState) -> String {
    gizai_core::users::list(&st.db).ok().and_then(|l| l.into_iter().find(|p| p.id == st.you_id)).map(|p| p.name)
        .filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "the user".into())
}

fn cut(s: &str, n: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

fn quote(text: &str) -> String {
    text.trim().lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")
}

/// The message of the Team Lead's run on the question: who asks what, on which card, the card's description and
/// acceptance criteria, and its last comments.
pub fn prompt(st: &AppState, q: &Question) -> String {
    let you = you(st);
    let project = match (&q.project_name, &q.project_key) {
        (Some(name), Some(key)) => format!("project {name} ({key}){}", q.client_name.as_ref().map(|c| format!(", client {c}")).unwrap_or_default()),
        _ => "no project".into(),
    };
    let asked = if q.text.trim().is_empty() { "(It gave no question: it only asked for a decision.)".to_string() } else { quote(&q.text) };
    let mut p = format!("{} ({}) ended its run on {} \"{}\" ({project}, column {}) asking for a decision. Its question, as it wrote it on the card:\n\n{asked}\n\n",
                        q.agent_name, q.role, q.identifier, q.title, q.column);
    if !q.description_md.trim().is_empty() {
        p.push_str(&format!("The card's description:\n\n{}\n\n", quote(&cut(&q.description_md, DESCRIPTION_CHARS))));
    }
    if let Some(a) = q.acceptance_md.as_deref().filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("Its acceptance criteria:\n\n{}\n\n", quote(&cut(a, DESCRIPTION_CHARS))));
    }
    let said: Vec<String> = comments::list(&st.db, &q.task_id).unwrap_or_default().into_iter()
        .filter(|c| c.run_id.as_deref() != Some(q.run_id.as_str()))
        .map(|c| format!("- {}: {}", c.author_name, cut(&c.body_md.split_whitespace().collect::<Vec<_>>().join(" "), COMMENT_CHARS)))
        .collect();
    if !said.is_empty() {
        p.push_str(&format!("The card's last comments, oldest first:\n\n{}\n\n", said[said.len().saturating_sub(COMMENTS)..].join("\n")));
    }
    p.push_str(&format!("Look in memory first. Answer it when memory, the card, the project's docs or the code settle it; otherwise leave it to {you}. \
                         End with your GIZAI_RESULT line."));
    p
}

/// The comment of an escalation: "Needs <you>: why", the options and the Team Lead's advice.
fn escalation_comment(you: &str, reason: &str, options: &[String], advice: &str) -> String {
    let mut c = format!("Needs {you}: {}", reason.trim());
    if !options.is_empty() {
        let list: Vec<String> = options.iter().enumerate().map(|(i, o)| format!("{}. {}", i + 1, o.trim())).collect();
        c.push_str(&format!("\n\nOptions:\n{}", list.join("\n")));
    }
    if !advice.trim().is_empty() {
        c.push_str(&format!("\n\nMy advice: {}", advice.trim()));
    }
    c
}

/// Starts the Team Lead's work on the question the run `asked_run_id` ended with (`settle`), in the background; `take`
/// gives its end.
pub fn start(st: &AppState, asked_run_id: &str, lead_id: &str) {
    let (st2, asked, lead) = (st.clone(), asked_run_id.to_string(), lead_id.to_string());
    let handle = tokio::spawn(async move {
        let s = settle(&st2, &asked, &lead).await;
        (st2.notify)(Note::RowsChanged("tasks"));
        (st2.notify)(Note::RowsChanged("comments"));
        (st2.notify)(Note::RowsChanged("runs"));
        (st2.notify)(Note::RunsChanged);
        s
    });
    let mut settled = st.chat.settled.lock().unwrap();
    // Ends nobody took within ten minutes are let go.
    settled.retain(|_, (at, h)| !h.is_finished() || at.elapsed() < Duration::from_secs(600));
    settled.insert(asked_run_id.to_string(), (Instant::now(), handle));
}

/// The end of the Team Lead's work on the question the run `asked_run_id` ended with, once (tests, and anything that
/// waits for it). None when the Team Lead didn't take that question, or its end was taken already.
pub fn take(st: &AppState, asked_run_id: &str) -> Option<tokio::task::JoinHandle<Settled>> {
    st.chat.settled.lock().unwrap().remove(asked_run_id).map(|(_, h)| h)
}

/// The Team Lead's work on a question, from its run to what Gizai does with the result.
async fn settle(st: &AppState, asked: &str, lead_id: &str) -> Settled {
    let you = you(st);
    let escalated = |run_id: Option<String>, reason: String, comment: String| {
        let done = questions::escalate(&st.db, asked, run_id.as_deref(), &reason, &comment);
        if let Err(e) = &done {
            eprintln!("gizai: sending the question of run {asked} to the Inbox failed: {e}");
        }
        let state = if done.unwrap_or(false) { "escalated" } else { "dropped" };
        Settled { state: state.into(), run_id, reason: Some(reason), continued: None }
    };
    // The reasons follow "Team Lead escalated to you: " on the card; the comments are the Team Lead's own.
    let (q, lead) = match (questions::question(&st.db, asked), team::agent(&st.db, lead_id)) {
        (Ok(q), Ok(lead)) => (q, lead),
        (Err(e), _) | (_, Err(e)) => {
            let why = format!("it couldn't look at the question ({e})");
            return escalated(None, why, format!("Needs {you}: I couldn't look at this question ({e}), so it is yours to decide."));
        }
    };
    // It can't run here at all (no MCP helper, not on a Claude Code it finds): the question goes to you as before.
    if let Err(why) = crate::chat::question_ready(st, &lead) {
        eprintln!("gizai: the Team Lead can't look at the question of run {asked}, so it goes to the Inbox: {why}");
        if let Err(e) = questions::skip(&st.db, asked, &why) {
            eprintln!("gizai: sending the question of run {asked} to the Inbox failed: {e}");
        }
        return Settled { state: "skipped".into(), run_id: None, reason: Some(why), continued: None };
    }
    let ran = crate::chat::question_once(st, &lead, asked, &prompt(st, &q)).await;
    let run_id = Some(ran.run_id.clone()).filter(|r| !r.is_empty());
    if ran.status != "succeeded" {
        let error = ran.error.clone().unwrap_or_default();
        let (why, mine) = match ran.status.as_str() {
            // "it stopped at the time limit (15 min or 60 tool calls per chat answer, Settings → Runs)"
            "timed_out" => {
                let what = error.strip_prefix("it ").unwrap_or(&error);
                (format!("its look at the question {what}"), format!("my look at {}'s question {what}", q.agent_name))
            }
            "cancelled" => ("Gizai quit before it had answered".to_string(), format!("Gizai quit before I had answered {}'s question", q.agent_name)),
            _ => (format!("its look at the question failed ({error})"), format!("my look at {}'s question failed ({error})", q.agent_name)),
        };
        return escalated(run_id, why, format!("Needs {you}: {mine}, so it is yours to decide."));
    }
    match verdict(&ran.text) {
        None => escalated(run_id, "it ended without an answer".into(),
                          format!("Needs {you}: I ended without an answer to {}'s question, so it is yours to decide.", q.agent_name)),
        Some(Verdict::Escalated { reason, options, advice }) => escalated(run_id, reason.clone(), escalation_comment(&you, &reason, &options, &advice)),
        Some(Verdict::Answered { answer, path, text }) => {
            match questions::answering(&st.db, asked, &answer, None) {
                Ok(true) => {}
                Ok(false) => return Settled { state: "dropped".into(), run_id, reason: None, continued: None },
                Err(e) => {
                    return escalated(run_id, format!("Gizai couldn't record its answer ({e})"),
                                     format!("Needs {you}: Gizai couldn't record my answer ({e}). My answer:\n\n{}", quote(&answer)));
                }
            }
            // Kept in memory, linked to the card, before the agent goes on.
            match questions::remember(&st.db, lead_id, asked, run_id.as_deref(), &lead.name, &answer, path.as_deref(), text.as_deref(),
                                      &crate::tools::ymd(ids::now_ms())) {
                Ok(saved) => {
                    let _ = questions::noted(&st.db, asked, &saved.path);
                    (st.notify)(Note::RowsChanged("docs"));
                }
                Err(e) => eprintln!("gizai: keeping the Team Lead's answer to run {asked} in memory failed: {e}"),
            }
            let mut tries = 0;
            loop {
                match crate::runs::continue_for_question(st, asked, lead_id, &answer).await {
                    Ok((id, done)) => {
                        if let Err(e) = questions::answered(&st.db, asked) {
                            eprintln!("gizai: recording the Team Lead's answer to run {asked} failed: {e}");
                        }
                        return Settled { state: "answered".into(), run_id, reason: None, continued: Some((id, done)) };
                    }
                    Err((true, _)) if tries < RETRIES && !crate::runs::is_closing(st) => {
                        tries += 1;
                        tokio::time::sleep(RETRY_EVERY).await;
                        // A person may have taken the card over meanwhile.
                        if !questions::waiting(&st.db, asked).unwrap_or(false) {
                            return Settled { state: "dropped".into(), run_id, reason: None, continued: None };
                        }
                    }
                    Err((_, e)) => {
                        let e = e.trim_end_matches('.');
                        let why = format!("it answered, but Gizai couldn't continue {} with the answer ({e})", q.agent_name);
                        let comment = format!("Needs {you}: I answered, but Gizai couldn't continue {} with it ({e}). My answer:\n\n{}\n\n\
                                               Clear the hold and press Run: {} starts again and reads this.", q.agent_name, quote(&answer), q.agent_name);
                        return escalated(run_id, why, comment);
                    }
                }
            }
        }
    }
}

/// A run `new_run_id` starts on the card after its last question went to you from the Team Lead: what people wrote on
/// the card since is kept in memory as the Team Lead's (`questions::learn_from_person`), linked to the card.
pub fn learn(st: &AppState, task_id: &str, new_run_id: &str) {
    match questions::learn_from_person(&st.db, task_id, new_run_id, &crate::tools::ymd(ids::now_ms())) {
        Ok(Some(_)) => (st.notify)(Note::RowsChanged("docs")),
        Ok(None) => {}
        Err(e) => eprintln!("gizai: keeping your answer on card {task_id} in memory failed: {e}"),
    }
}
