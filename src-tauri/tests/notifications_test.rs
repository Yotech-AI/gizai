// GA-21: desktop notifications. The comparison (`new_items`) on its own; what the Inbox gives (`inbox`); a look at the
// Inbox (`look`) and the watch that looks again after a poke (`watch`), with the switches in Settings; and "The Team Lead
// answered" (`answered`), also at the end of a real (fake Claude Code) chat answer. Every test hands Gizai a desktop
// that only records what it would show, so no test shows a real notification.
#[path = "support/data_lock.rs"]
mod data_lock;
use std::collections::HashSet;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gizai_core::model::*;
use gizai_core::chat::{self, NewMessage};
use gizai_core::{projects, tasks, team};
use gizai_lib::chat::TurnSummary;
use gizai_lib::notifications::{self, Desktop, Item, Kind, Notice, Switches};
use gizai_lib::{AppState, runs};
#[cfg(unix)]
use gizai_lib::mcp;

struct T {
    st: AppState,
    shown: Arc<Mutex<Vec<Notice>>>,
    away: Arc<AtomicBool>,
    project: String,
    // keeps the data folder while the test runs
    #[cfg_attr(not(unix), allow(dead_code))]
    dir: tempfile::TempDir,
}

/// Gizai on fresh data, with project KADE and a desktop that records what it shows. The window counts as away.
fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let mut st = gizai_lib::test_state(dir.path());
    let (shown, away): (Arc<Mutex<Vec<Notice>>>, _) = (Arc::default(), Arc::new(AtomicBool::new(true)));
    let (s2, a2) = (shown.clone(), away.clone());
    st.desktop = Desktop { show: Arc::new(move |n| s2.lock().unwrap().push(n)), away: Arc::new(move || a2.load(Ordering::SeqCst)) };
    let project = projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    T { st, shown, away, project, dir }
}

impl T {
    fn col(&self, category: &str) -> String {
        let team = team::get(&self.st.db, &team::list(&self.st.db).unwrap()[0].id).unwrap();
        team.states.iter().find(|s| s.category == category).unwrap().id.clone()
    }
    fn card(&self, title: &str, category: &str, assignee: Option<String>) -> String {
        tasks::create(&self.st.db, &self.st.you_id, TaskInput {
            project_id: self.project.clone(), title: title.into(), state_id: Some(self.col(category)), assignee_id: assignee, ..Default::default() }).unwrap()
    }
    fn ident(&self, id: &str) -> String {
        tasks::get(&self.st.db, id).unwrap().identifier
    }
    fn hold(&self, id: &str, hold: &str, reason: Option<&str>) {
        // hold_at is in milliseconds: a new hold right after a cleared one gets a time of its own
        std::thread::sleep(Duration::from_millis(5));
        tasks::update(&self.st.db, &self.st.you_id, id, TaskPatch { hold: Some(hold.into()), hold_reason: reason.map(Into::into), ..Default::default() }).unwrap();
    }
    fn unhold(&self, id: &str) {
        tasks::update(&self.st.db, &self.st.you_id, id, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    }
    fn move_to(&self, id: &str, category: &str) {
        tasks::move_to(&self.st.db, &self.st.you_id, id, &self.col(category), "").unwrap();
    }
    fn agent(&self, name: &str, role: &str) -> String {
        let team_id = team::list(&self.st.db).unwrap()[0].id.clone();
        team::add_agent(&self.st.db, &self.st.you_id, &team_id, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap()
    }
    fn titles(&self, sent: &[Notice]) -> Vec<String> {
        sent.iter().map(|n| n.title.clone()).collect()
    }
    fn shown(&self) -> Vec<String> {
        self.shown.lock().unwrap().iter().map(|n| n.title.clone()).collect()
    }
    fn set_switches(&self, s: Switches) {
        let mut all = runs::get_settings(&self.st);
        all.notifications = s;
        runs::save_settings(&self.st, &all).unwrap();
    }
}

fn item(key: &str) -> Item {
    Item { key: key.into(), notice: Notice { kind: Kind::Hold, title: format!("title {key}"), body: String::new(), route: "#/task/x".into() } }
}

fn keys(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.key.as_str()).collect()
}

fn set(keys: &[&str]) -> HashSet<String> {
    keys.iter().map(|k| k.to_string()).collect()
}

// ---- the comparison: the items seen before and the items now in, what to notify out ----

#[test]
fn new_items_are_those_not_seen_at_the_last_look_in_the_inbox_order() {
    let (fresh, seen) = notifications::new_items(&HashSet::new(), vec![item("b"), item("a"), item("c")]);
    assert_eq!(keys(&fresh), ["b", "a", "c"], "nothing seen yet: everything is new, in the Inbox's order");
    assert_eq!(seen, set(&["a", "b", "c"]));
    let (fresh, seen) = notifications::new_items(&seen, vec![item("a"), item("d"), item("b"), item("c")]);
    assert_eq!(keys(&fresh), ["d"], "only the one that wasn't there");
    assert_eq!(seen, set(&["a", "b", "c", "d"]));
    let (fresh, seen2) = notifications::new_items(&seen, vec![item("a"), item("b"), item("c"), item("d")]);
    assert!(fresh.is_empty(), "the same Inbox again notifies nothing: {:?}", keys(&fresh));
    assert_eq!(seen2, seen);
    let (fresh, _) = notifications::new_items(&seen, vec![]);
    assert!(fresh.is_empty());
}

#[test]
fn an_item_notifies_again_only_after_it_left_the_inbox_and_came_back() {
    let (_, seen) = notifications::new_items(&HashSet::new(), vec![item("hold:t1:100"), item("waiting:t2:review")]);
    // it stays: no repeat, however often the Inbox is looked at
    let (fresh, seen) = notifications::new_items(&seen, vec![item("hold:t1:100"), item("waiting:t2:review")]);
    assert!(fresh.is_empty());
    // it leaves (the hold is cleared, the review is done) ...
    let (fresh, seen) = notifications::new_items(&seen, vec![item("waiting:t2:review")]);
    assert!(fresh.is_empty(), "leaving notifies nothing");
    assert_eq!(seen, set(&["waiting:t2:review"]), "what left no longer counts as seen");
    // ... and comes back: new again
    let (fresh, seen) = notifications::new_items(&seen, vec![item("hold:t1:100"), item("waiting:t2:review")]);
    assert_eq!(keys(&fresh), ["hold:t1:100"]);
    // a new hold on the same card is a new key, so a new item even when the Inbox never saw the card leave
    let (fresh, _) = notifications::new_items(&seen, vec![item("hold:t1:200"), item("waiting:t2:review")]);
    assert_eq!(keys(&fresh), ["hold:t1:200"]);
}

#[test]
fn an_item_twice_in_the_inbox_notifies_once() {
    let (fresh, seen) = notifications::new_items(&set(&["a"]), vec![item("b"), item("a"), item("b")]);
    assert_eq!(keys(&fresh), ["b"]);
    assert_eq!(seen, set(&["a", "b"]));
}

// ---- what the Inbox gives ----

#[test]
fn the_inbox_gives_held_cards_cards_waiting_for_you_and_the_team_leads_chats_with_their_texts_and_links() {
    let t = setup();
    let backend = t.agent("Backend Agent", "backend");
    let lead = t.agent("Team Lead", "lead");
    let held = t.card("Login page", "in_progress", Some(backend.clone()));
    t.hold(&held, "needs_decision", Some("Which API should the login use?\nThe old or the new one."));
    let bare = t.card("Bare hold", "ready", None);
    t.hold(&bare, "needs_decision", None);
    let long = t.card("Long hold", "ready", None);
    t.hold(&long, "blocked", Some(&"word ".repeat(60)));
    let review = t.card("Review me", "review", Some(t.st.you_id.clone()));
    let deploy = t.card("Deploy me", "deploy", Some(t.st.you_id.clone()));
    t.card("Someone else's review", "review", Some(backend.clone()));
    let done = t.card("Done but held", "done", None);
    t.hold(&done, "blocked", Some("old"));
    t.card("Plain", "ready", None);
    let (question, _) = chat::start_lead_chat(&t.st.db, &lead, "Which API for the login?", "question", &[held.clone()], "Old or new?", None).unwrap();
    let (approval, _) = chat::start_lead_chat(&t.st.db, &lead, "Merge the release branch", "approval", &[review.clone()], "May I merge?", None).unwrap();

    let items = notifications::inbox(&t.st.db, &t.st.you_id).unwrap();
    let get = |key_start: &str| items.iter().find(|i| i.key.starts_with(key_start)).unwrap_or_else(|| panic!("no {key_start}: {items:?}")).clone();

    let h = get(&format!("hold:{held}:"));
    let hold_at = tasks::get(&t.st.db, &held).unwrap().hold_at.expect("a held card has hold_at");
    assert_eq!(h.key, format!("hold:{held}:{hold_at}"));
    assert_eq!(h.notice, Notice { kind: Kind::Hold, title: format!("{} is on hold: Which API should the login use? The old or the new one.", t.ident(&held)),
                                  body: "Login page".into(), route: format!("#/task/{held}") });
    assert_eq!(get(&format!("hold:{bare}:")).notice.title, format!("{} is on hold: needs your decision", t.ident(&bare)), "no reason: the hold in words");
    let cut = get(&format!("hold:{long}:")).notice.title;
    let reason = cut.strip_prefix(&format!("{} is on hold: ", t.ident(&long))).unwrap();
    assert!(reason.ends_with('…') && reason.chars().count() <= 81, "the reason is cut short: {reason:?}");

    let r = get(&format!("waiting:{review}:"));
    assert_eq!(r.key, format!("waiting:{review}:review"));
    assert_eq!(r.notice, Notice { kind: Kind::Waiting, title: format!("{} waits for your review", t.ident(&review)), body: "Review me".into(), route: format!("#/task/{review}") });
    let d = get(&format!("waiting:{deploy}:"));
    assert_eq!(d.key, format!("waiting:{deploy}:deploy"));
    assert_eq!(d.notice.title, format!("{} is merged and waits for deploy", t.ident(&deploy)));

    let q = get(&format!("chat:{question}"));
    assert_eq!(q.notice, Notice { kind: Kind::LeadAsks, title: "The Team Lead asks: Which API for the login?".into(),
                                  body: format!("Question about {}", t.ident(&held)), route: format!("#/chat/{question}") });
    let a = get(&format!("chat:{approval}"));
    assert_eq!((a.notice.kind, a.notice.title.as_str(), a.notice.body.clone()),
               (Kind::LeadAsks, "The Team Lead asks: Merge the release branch", format!("Approval for {}", t.ident(&review))));

    // nothing else: not someone else's review, not a closed card's hold, not a plain card
    assert_eq!(items.len(), 7, "{:?}", keys(&items));
    assert!(!items.iter().any(|i| i.notice.body == "Someone else's review"), "{items:?}");
    assert!(!items.iter().any(|i| i.key.contains(&done)), "a Done card's hold isn't in the Inbox");
}

// ---- a look at the Inbox ----

#[test]
fn at_start_the_inbox_counts_as_seen_and_each_new_item_notifies_once() {
    let t = setup();
    let lead = t.agent("Team Lead", "lead");
    let old = t.card("Already held", "ready", None);
    t.hold(&old, "blocked", Some("from yesterday"));
    t.card("Already in review", "review", Some(t.st.you_id.clone()));
    assert!(notifications::look(&t.st).is_empty(), "the first look only takes in what is there");
    assert!(t.shown().is_empty());

    let card = t.card("Login page", "in_progress", None);
    t.hold(&card, "needs_decision", Some("Which API?"));
    let sent = notifications::look(&t.st);
    assert_eq!(t.titles(&sent), [format!("{} is on hold: Which API?", t.ident(&card))]);
    assert_eq!(sent[0].route, format!("#/task/{card}"));
    assert_eq!(t.shown(), t.titles(&sent), "what look returns is what it showed");
    assert!(notifications::look(&t.st).is_empty(), "no repeat while it stays on hold");
    assert!(notifications::look(&t.st).is_empty());

    // the hold is cleared (no notification), then a new hold: it notifies again
    t.unhold(&card);
    assert!(notifications::look(&t.st).is_empty());
    t.hold(&card, "blocked", Some("The build server is down"));
    assert_eq!(t.titles(&notifications::look(&t.st)), [format!("{} is on hold: The build server is down", t.ident(&card))]);

    // a card that lands in Review assigned to you, then Deploy, then back in Review
    let mine = t.card("Profile page", "in_progress", Some(t.st.you_id.clone()));
    assert!(notifications::look(&t.st).is_empty(), "In progress doesn't need you");
    t.move_to(&mine, "review");
    assert_eq!(t.titles(&notifications::look(&t.st)), [format!("{} waits for your review", t.ident(&mine))]);
    assert!(notifications::look(&t.st).is_empty());
    t.move_to(&mine, "deploy");
    assert_eq!(t.titles(&notifications::look(&t.st)), [format!("{} is merged and waits for deploy", t.ident(&mine))]);
    t.move_to(&mine, "review");
    assert_eq!(t.titles(&notifications::look(&t.st)), [format!("{} waits for your review", t.ident(&mine))], "back in Review: again");
    // someone else's card in Review is not yours to review
    let theirs = t.card("Their page", "in_progress", Some(t.agent("Backend Agent", "backend")));
    t.move_to(&theirs, "review");
    assert!(notifications::look(&t.st).is_empty());

    // the Team Lead asks: once; a second message in the same waiting chat is the same item
    let (thread, _) = chat::start_lead_chat(&t.st.db, &lead, "Which API for the login?", "question", &[card.clone()], "Old or new?", None).unwrap();
    let sent = notifications::look(&t.st);
    assert_eq!(t.titles(&sent), ["The Team Lead asks: Which API for the login?"]);
    assert_eq!(sent[0].route, format!("#/chat/{thread}"));
    let (again, new) = chat::start_lead_chat(&t.st.db, &lead, "Still waiting", "question", &[card.clone()], "Any news?", None).unwrap();
    assert_eq!((again.as_str(), new), (thread.as_str(), false));
    assert!(notifications::look(&t.st).is_empty());
    chat::dismiss(&t.st.db, &t.st.you_id, &thread).unwrap();
    assert!(notifications::look(&t.st).is_empty(), "leaving the Inbox notifies nothing");

    // several new things at once: one notification each
    let a = t.card("A", "ready", None);
    let b = t.card("B", "ready", None);
    t.hold(&a, "stalled", None);
    t.hold(&b, "merge_conflict", None);
    let mut both = t.titles(&notifications::look(&t.st));
    both.sort();
    let mut want = vec![format!("{} is on hold: stalled", t.ident(&a)), format!("{} is on hold: merge conflict", t.ident(&b))];
    want.sort();
    assert_eq!(both, want);
    assert!(!t.shown().iter().any(|s| s.contains("Already")), "what was there at start never notified: {:?}", t.shown());
}

#[test]
fn a_kind_that_is_switched_off_doesnt_notify_and_its_items_still_count_as_seen() {
    let t = setup();
    let lead = t.agent("Team Lead", "lead");
    notifications::look(&t.st);
    t.set_switches(Switches { hold: false, waiting: true, lead_asks: false, lead_answered: true });
    let held = t.card("Held", "ready", None);
    t.hold(&held, "blocked", Some("no"));
    chat::start_lead_chat(&t.st.db, &lead, "A question", "question", &[held.clone()], "Well?", None).unwrap();
    let mine = t.card("Mine", "review", Some(t.st.you_id.clone()));
    assert_eq!(t.titles(&notifications::look(&t.st)), [format!("{} waits for your review", t.ident(&mine))], "only the kind that is on");
    // switched on again: what came in meanwhile was seen, so it doesn't notify late
    t.set_switches(Switches::default());
    assert!(notifications::look(&t.st).is_empty(), "{:?}", t.shown());
    // a new one of those kinds does
    let other = t.card("Other", "ready", None);
    t.hold(&other, "blocked", Some("yes"));
    assert_eq!(t.titles(&notifications::look(&t.st)), [format!("{} is on hold: yes", t.ident(&other))]);
    // Waiting off
    t.set_switches(Switches { waiting: false, ..Switches::default() });
    t.card("Mine too", "deploy", Some(t.st.you_id.clone()));
    assert!(notifications::look(&t.st).is_empty());
    assert_eq!(t.shown().len(), 2, "{:?}", t.shown());
}

#[test]
fn nothing_notifies_while_gizai_quits() {
    let t = setup();
    notifications::look(&t.st);
    runs::mark_closing(&t.st);
    let held = t.card("Held", "ready", None);
    t.hold(&held, "blocked", Some("no"));
    assert!(notifications::look(&t.st).is_empty());
    let thread = chat::start_lead_chat(&t.st.db, &t.agent("Team Lead", "lead"), "Q", "question", &[held], "Well?", None).unwrap().0;
    assert_eq!(notifications::answered(&t.st, &thread, &summary("succeeded", None)), None);
    assert!(t.shown().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_watch_takes_in_the_inbox_at_start_and_a_burst_of_changes_is_one_look_a_few_seconds_later() {
    let t = setup();
    let old = t.card("Already held", "ready", None);
    t.hold(&old, "blocked", Some("from yesterday"));
    let watch = tokio::spawn(notifications::watch(t.st.clone()));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(t.shown().is_empty(), "at start nothing notifies: {:?}", t.shown());

    // a burst: two holds, each with its poke (as ui_notifier pokes on RowsChanged("tasks"))
    let (a, b) = (t.card("A", "ready", None), t.card("B", "ready", None));
    t.hold(&a, "blocked", Some("first"));
    notifications::poke(&t.st);
    tokio::time::sleep(Duration::from_millis(300)).await;
    t.hold(&b, "blocked", Some("second"));
    notifications::poke(&t.st);
    tokio::time::sleep(notifications::GROUP / 2).await;
    assert!(t.shown().is_empty(), "the look waits a few seconds after a change: {:?}", t.shown());
    tokio::time::sleep(notifications::GROUP + Duration::from_millis(1500)).await;
    let mut got = t.shown();
    got.sort();
    assert_eq!(got, [format!("{} is on hold: first", t.ident(&a)), format!("{} is on hold: second", t.ident(&b))]);

    // a poke with nothing new: no repeat
    notifications::poke(&t.st);
    tokio::time::sleep(notifications::GROUP + Duration::from_millis(1000)).await;
    assert_eq!(t.shown().len(), 2, "{:?}", t.shown());
    watch.abort();
}

// ---- The Team Lead answered ----

fn summary(status: &str, error: Option<&str>) -> TurnSummary {
    TurnSummary { run_id: "r1".into(), status: status.into(), error: error.map(Into::into) }
}

#[test]
fn the_team_lead_answered_notifies_only_while_the_window_is_out_of_sight() {
    let t = setup();
    let lead = t.agent("Team Lead", "lead");
    let thread = chat::create_thread(&t.st.db, &t.st.you_id, &lead, "Plan the login page").unwrap();
    let title = chat::get_thread(&t.st.db, &thread).unwrap().title;
    chat::add_message(&t.st.db, NewMessage { thread_id: thread.clone(), role: "agent".into(), author_id: Some(lead.clone()),
        body_md: Some(format!("Here is the plan: {}", "step ".repeat(60))), ..Default::default() }).unwrap();
    chat::add_message(&t.st.db, NewMessage { thread_id: thread.clone(), role: "tool".into(), author_id: Some(lead.clone()),
        tool_name: Some("mcp__gizai__get_overview".into()), ..Default::default() }).unwrap();

    // focused: nothing
    t.away.store(false, Ordering::SeqCst);
    assert_eq!(notifications::answered(&t.st, &thread, &summary("succeeded", None)), None);
    assert_eq!(notifications::answered(&t.st, &thread, &summary("failed", Some("boom"))), None);
    assert!(t.shown().is_empty());

    // hidden, minimised or not focused
    t.away.store(true, Ordering::SeqCst);
    let n = notifications::answered(&t.st, &thread, &summary("succeeded", None)).expect("answered");
    assert_eq!((n.kind, n.title.clone(), n.route.clone()), (Kind::LeadAnswered, format!("The Team Lead answered: {title}"), format!("#/chat/{thread}")));
    assert!(n.body.starts_with("Here is the plan: step step") && n.body.ends_with('…') && n.body.chars().count() <= 161, "the start of the answer: {:?}", n.body);
    let n = notifications::answered(&t.st, &thread, &summary("failed", Some("API Error: 500 the fake failed"))).expect("couldn't answer");
    assert_eq!((n.title.as_str(), n.body.as_str()), (format!("The Team Lead couldn't answer: {title}").as_str(), "API Error: 500 the fake failed"));
    let n = notifications::answered(&t.st, &thread, &summary("timed_out", None)).expect("timed out");
    assert_eq!(n.title, format!("The Team Lead couldn't answer: {title}"));
    assert_eq!(t.shown().len(), 3);

    // you pressed Stop, or a message only went in the queue: nothing
    assert_eq!(notifications::answered(&t.st, &thread, &summary("cancelled", None)), None);
    assert_eq!(notifications::answered(&t.st, &thread, &summary("queued", None)), None);
    // switched off
    t.set_switches(Switches { lead_answered: false, ..Switches::default() });
    assert_eq!(notifications::answered(&t.st, &thread, &summary("succeeded", None)), None);
    assert_eq!(t.shown().len(), 3, "{:?}", t.shown());
}

/// The shim binary: target/debug/gizai-mcp (built by `cargo test --workspace`; built here when missing).
#[cfg(unix)]
fn shim() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/gizai-mcp"));
    BUILT.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let ok = std::process::Command::new(cargo).args(["build", "-q", "-p", "gizai-mcp"])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).status().map(|s| s.success()).unwrap_or(false);
        assert!(ok && path.is_file(), "could not build gizai-mcp");
    });
    path
}

// Linux and macOS only: the fake Claude Code is a Python script.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_real_chat_answer_ends_with_the_team_lead_answered_when_the_window_is_away_and_with_nothing_when_it_is_focused() {
    let mut t = setup();
    t.st.mcp_shim = Some(shim());
    gizai_core::settings::set(&t.st.db, "claude_bin", &concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-chat.py").to_string()).unwrap();
    let team_id = team::list(&t.st.db).unwrap()[0].id.clone();
    team::add_agent(&t.st.db, &t.st.you_id, &team_id, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    let server = mcp::start(&t.st).unwrap();
    let turn = |text: &str, thread: Option<String>| {
        let st = t.st.clone();
        let text = text.to_string();
        async move {
            let (id, done) = gizai_lib::chat::send(&st, thread, text, None).await.unwrap();
            let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("turn finished").unwrap();
            (id, s)
        }
    };

    // focused: the answer ends without a notification
    t.away.store(false, Ordering::SeqCst);
    let (thread, s) = turn("How is the board doing?", None).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    assert!(t.shown().is_empty(), "{:?}", t.shown());

    // away: "The Team Lead answered", with the start of the answer, and a click opens the chat
    t.away.store(true, Ordering::SeqCst);
    let (_, s) = turn("And now?", Some(thread.clone())).await;
    assert_eq!(s.status, "succeeded", "{s:?}");
    let title = chat::get_thread(&t.st.db, &thread).unwrap().title;
    let shown = t.shown.lock().unwrap().clone();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!((shown[0].kind, shown[0].title.clone(), shown[0].body.as_str(), shown[0].route.clone()),
               (Kind::LeadAnswered, format!("The Team Lead answered: {title}"), "Here is the overview.", format!("#/chat/{thread}")));

    // an answer that fails: "couldn't answer"
    let (_, s) = turn("FAKE_CHAT_FAIL please", Some(thread.clone())).await;
    assert_eq!(s.status, "failed", "{s:?}");
    let last = t.shown.lock().unwrap().last().cloned().unwrap();
    assert_eq!(last.title, format!("The Team Lead couldn't answer: {title}"));
    assert_eq!(t.shown().len(), 2);
    server.abort();
    drop(t.dir);
}

// ---- the switches are settings ----

#[test]
fn the_switches_are_all_on_by_default_and_are_kept_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    let s = runs::get_settings(&st);
    assert_eq!(s.notifications, Switches { hold: true, waiting: true, lead_asks: true, lead_answered: true });
    // the UI's names (src/types.ts NotificationSwitches)
    assert_eq!(serde_json::to_value(&s).unwrap()["notifications"], serde_json::json!({"hold": true, "waiting": true, "leadAsks": true, "leadAnswered": true}));
    let mut off = s.clone();
    off.notifications = Switches { hold: false, waiting: true, lead_asks: false, lead_answered: false };
    runs::save_settings(&st, &off).unwrap();
    drop(st);
    data_lock::released_blocking(&dir.path().join("data"));
    let st = gizai_lib::test_state(dir.path());
    assert_eq!(runs::get_settings(&st).notifications, Switches { hold: false, waiting: true, lead_asks: false, lead_answered: false });
    assert_eq!(notifications::switches(&st.db), runs::get_settings(&st).notifications);
    // a settings object from the UI without the switches leaves them all on
    let mut v = serde_json::to_value(runs::get_settings(&st)).unwrap();
    v.as_object_mut().unwrap().remove("notifications");
    let parsed: runs::Settings = serde_json::from_value(v).unwrap();
    assert_eq!(parsed.notifications, Switches::default());
    let partial: Switches = serde_json::from_value(serde_json::json!({"hold": false})).unwrap();
    assert_eq!(partial, Switches { hold: false, ..Switches::default() });
}

// ---- the tray's library ----

#[test]
fn the_deb_depends_on_the_tray_library_and_the_readme_names_it() {
    let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let depends = conf["bundle"]["linux"]["deb"]["depends"].as_array().expect("the .deb's depends");
    assert!(depends.iter().any(|d| d == "libayatana-appindicator3-1"), "{depends:?}");
    let readme = include_str!("../../README.md");
    assert!(readme.contains("libayatana-appindicator"), "the README names the tray library");
    assert!(readme.contains("`libayatana-appindicator` on Arch and Omarchy"), "and the Arch/Omarchy package");
    let cargo = include_str!("../Cargo.toml");
    assert!(cargo.lines().any(|l| l.starts_with("tauri = ") && l.contains("\"tray-icon\"")), "tauri's tray-icon feature is on");
}
