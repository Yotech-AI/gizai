//! Desktop notifications (Settings → Notifications): Gizai tells you when something needs you, also while its window
//! is hidden. Four kinds, each with its own switch, all on by default:
//! - a card goes on hold, and a card lands in Review or Deploy assigned to you: the cards in the Inbox
//!   (`tasks::needs_you`);
//! - the Team Lead starts a Question or Approval chat: the chats at the top of the Inbox (`chat::waiting_lead_chats`);
//! - an answer in a chat ends while Gizai's window is hidden or not focused (`answered`).
//!
//! The Team Lead merging a card's pull request (`lead_merged`, GA-86) notifies too, under the switch for cards waiting
//! for you.
//!
//! The Inbox is looked at again when tasks or chats change (`poke`; a burst of changes is one look, at most one every
//! few seconds), and each item that is new since the last look notifies (`new_items`). At start, what is in the Inbox
//! counts as seen, and an item notifies again only after it left the Inbox and came back.
//!
//! Every notification goes through `AppState::desktop`: the real one (`real`) shows it with notify-rust and, on a
//! click, shows the window on the card or the chat (Linux and Windows; on macOS a notification only informs). Tests set
//! their own, so no test shows a notification.
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gizai_core::db::Db;
use gizai_core::{chat, settings, tasks};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::AppState;
use crate::chat::TurnSummary;

/// A burst of changes (a run that ends moves its card, comments on it and may hold it) is one look: the look waits this
/// long after a change, so there is at most one look every few seconds.
pub const GROUP: Duration = Duration::from_secs(3);
/// A hold reason in a notification's first line is cut to about this many characters.
const REASON_CHARS: usize = 80;
/// The text under the first line (the start of an answer, why it failed) is cut to about this many characters.
const BODY_CHARS: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// An open card went on hold.
    Hold,
    /// A card landed in Review or Deploy, assigned to you.
    Waiting,
    /// The Team Lead started a Question or Approval chat.
    LeadAsks,
    /// An answer in a chat ended while Gizai's window was out of sight.
    LeadAnswered,
    /// The Team Lead merged a card's pull request (GA-86). Under the Waiting switch: the card waits for deploy now.
    LeadMerged,
}

/// One desktop notification: its first line, the text under it (may be empty), and what a click opens: `#/task/<id>`
/// or `#/chat/<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub kind: Kind,
    pub title: String,
    pub body: String,
    pub route: String,
}

/// Something in the Inbox, and the notification it gives. `key` names it: while the same key is in the Inbox, it is the
/// same item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub notice: Notice,
}

/// The switches in Settings → Notifications, one per kind; all on by default. Kept in the settings table
/// (`notify_hold`, `notify_waiting`, `notify_lead_asks`, `notify_lead_answered`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Switches {
    pub hold: bool,
    pub waiting: bool,
    pub lead_asks: bool,
    pub lead_answered: bool,
}

impl Default for Switches {
    fn default() -> Self {
        Switches { hold: true, waiting: true, lead_asks: true, lead_answered: true }
    }
}

impl Switches {
    pub fn on(&self, kind: Kind) -> bool {
        match kind {
            Kind::Hold => self.hold,
            Kind::Waiting | Kind::LeadMerged => self.waiting,
            Kind::LeadAsks => self.lead_asks,
            Kind::LeadAnswered => self.lead_answered,
        }
    }

    fn keyed(&self) -> [(&'static str, bool); 4] {
        [("notify_hold", self.hold), ("notify_waiting", self.waiting), ("notify_lead_asks", self.lead_asks), ("notify_lead_answered", self.lead_answered)]
    }
}

/// The switches as saved; a switch that was never saved is on.
pub fn switches(db: &Db) -> Switches {
    let on = |key: &str| settings::get::<bool>(db, key).ok().flatten().unwrap_or(true);
    Switches { hold: on("notify_hold"), waiting: on("notify_waiting"), lead_asks: on("notify_lead_asks"), lead_answered: on("notify_lead_answered") }
}

pub fn save_switches(db: &Db, s: &Switches) -> Result<(), String> {
    for (key, on) in s.keyed() {
        settings::set(db, key, &on).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// How Gizai reaches the desktop. `open_state` starts with `silent`; `run` sets the real one (`real`), and tests set
/// their own.
#[derive(Clone)]
pub struct Desktop {
    /// Shows a notification: every notification goes through here.
    pub show: Arc<dyn Fn(Notice) + Send + Sync>,
    /// Whether Gizai's window is hidden, minimised or not focused ("The Team Lead answered" only notifies then).
    pub away: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Desktop {
    /// Shows nothing. There is no window, so it counts as away.
    pub fn silent() -> Desktop {
        Desktop { show: Arc::new(|_| {}), away: Arc::new(|| true) }
    }
}

/// The Inbox as the last look saw it, and the poke that asks for a new look.
#[derive(Default)]
pub struct Watch {
    /// The keys of the items in the Inbox at the last look; None before the first look.
    seen: Mutex<Option<HashSet<String>>>,
    /// Items a notification of their own already told (the Team Lead's merge, `lead_merged`): a look doesn't notify them
    /// again.
    told: Mutex<HashSet<String>>,
    poke: tokio::sync::Notify,
}

/// Tasks or chats changed: `watch` looks at the Inbox again soon.
pub fn poke(st: &AppState) {
    st.inbox.poke.notify_one();
}

/// The comparison: of the items now in the Inbox (`now`), those that weren't there at the last look (`seen`, their
/// keys), in `now`'s order; and what counts as seen from now on (the keys of `now`). An item that left the Inbox and
/// came back is new again: the look in between no longer saw it. An item twice in `now` counts once.
pub fn new_items(seen: &HashSet<String>, now: Vec<Item>) -> (Vec<Item>, HashSet<String>) {
    let mut keys = HashSet::new();
    let fresh = now.into_iter().filter(|i| keys.insert(i.key.clone()) && !seen.contains(&i.key)).collect();
    (fresh, keys)
}

/// What is in the Inbox now, as items: a card on hold (keyed by when its hold was set, so a new hold is a new item),
/// a card waiting for you in Review or Deploy (keyed by the column's kind, so the merge that moves it to Deploy is new),
/// and a chat the Team Lead started that waits for you. A card in Review that is also on hold is both.
pub fn inbox(db: &Db, you_id: &str) -> Result<Vec<Item>, String> {
    let mut items = vec![];
    for t in tasks::needs_you(db, you_id).map_err(|e| e.to_string())? {
        let route = format!("#/task/{}", t.id);
        // A card whose question is with the Team Lead (GA-70) doesn't need you yet: it notifies if the Team Lead asks you.
        if let Some(hold) = t.hold.as_ref().filter(|_| !t.with_lead) {
            let why = t.hold_reason.as_deref().map(str::trim).filter(|r| !r.is_empty())
                .map(|r| short(r, REASON_CHARS)).unwrap_or_else(|| hold_name(hold).into());
            items.push(Item {
                key: format!("hold:{}:{}", t.id, t.hold_at.unwrap_or_default()),
                notice: Notice { kind: Kind::Hold, title: format!("{} is on hold: {why}", t.identifier), body: t.title.clone(), route: route.clone() },
            });
        }
        if t.assignee_id.as_deref() != Some(you_id) {
            continue;
        }
        let title = match t.state_category.as_str() {
            "review" => format!("{} waits for your review", t.identifier),
            "deploy" => format!("{} is merged and waits for deploy", t.identifier),
            _ => continue,
        };
        items.push(Item {
            key: format!("waiting:{}:{}", t.id, t.state_category),
            notice: Notice { kind: Kind::Waiting, title, body: t.title, route },
        });
    }
    for c in chat::waiting_lead_chats(db).map_err(|e| e.to_string())? {
        let what = if c.kind.as_deref() == Some("approval") { "Approval for" } else { "Question about" };
        let body = match c.tasks.is_empty() {
            true => what.split(' ').next().unwrap_or_default().to_string(),
            false => format!("{what} {}", c.tasks.join(", ")),
        };
        items.push(Item {
            key: format!("chat:{}", c.id),
            notice: Notice { kind: Kind::LeadAsks, title: format!("The Team Lead asks: {}", c.title), body, route: format!("#/chat/{}", c.id) },
        });
    }
    Ok(items)
}

/// A hold without a reason, in words (as on the card's Hold row).
fn hold_name(hold: &str) -> &'static str {
    match hold {
        "needs_decision" => "needs your decision",
        "stalled" => "stalled",
        "merge_conflict" => "merge conflict",
        "waiting_approval" => "waiting for approval",
        "rate_limited" => "rate limited",
        _ => "blocked",
    }
}

/// `text` on one line, cut on a word to about `max` characters.
fn short(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([',', '.', ':', ';', ' ']))
}

/// Looks at the Inbox, and notifies each item that is new since the last look and whose switch is on. The first look
/// only takes in what is there; a switched-off item still counts as seen. Nothing while Gizai quits. Returns the
/// notifications it sent.
pub fn look(st: &AppState) -> Vec<Notice> {
    if crate::runs::is_closing(st) {
        return vec![];
    }
    let now = match inbox(&st.db, &st.you_id) {
        Ok(items) => items,
        Err(e) => {
            eprintln!("gizai: notifications couldn't read the Inbox: {e}");
            return vec![];
        }
    };
    let fresh = {
        let mut seen = st.inbox.seen.lock().unwrap();
        let (fresh, keys) = new_items(seen.as_ref().unwrap_or(&HashSet::new()), now);
        let first = seen.replace(keys).is_none();
        if first {
            return vec![];
        }
        fresh
    };
    let fresh: Vec<Item> = {
        let mut told = st.inbox.told.lock().unwrap();
        fresh.into_iter().filter(|i| !told.remove(&i.key)).collect()
    };
    if fresh.is_empty() {
        return vec![];
    }
    let on = switches(&st.db);
    let sent: Vec<Notice> = fresh.into_iter().map(|i| i.notice).filter(|n| on.on(n.kind)).collect();
    for n in &sent {
        (st.desktop.show)(n.clone());
    }
    sent
}

/// Watches the Inbox while Gizai runs: a first look at start (what is there counts as seen), then a look after each
/// poke, `GROUP` later, so a burst of changes is one look.
pub async fn watch(st: AppState) {
    let first = st.clone();
    let _ = tokio::task::spawn_blocking(move || look(&first)).await;
    loop {
        st.inbox.poke.notified().await;
        tokio::time::sleep(GROUP).await;
        let st2 = st.clone();
        let _ = tokio::task::spawn_blocking(move || look(&st2)).await;
    }
}

/// An answer in a chat ended (`chat::chain` returned: the answer and the messages queued after it are done). "The Team
/// Lead answered: <chat>" with the start of the answer, or "The Team Lead couldn't answer: <chat>" with why, when its
/// switch is on and Gizai's window is hidden, minimised or not focused. Nothing after Stop or while Gizai quits. Returns
/// what it sent.
pub fn answered(st: &AppState, thread_id: &str, summary: &TurnSummary) -> Option<Notice> {
    let failed = match summary.status.as_str() {
        "succeeded" => false,
        "failed" | "timed_out" => true,
        _ => return None,
    };
    if crate::runs::is_closing(st) || !switches(&st.db).lead_answered || !(st.desktop.away)() {
        return None;
    }
    let thread = chat::get_thread(&st.db, thread_id).ok()?;
    let n = if failed {
        Notice { kind: Kind::LeadAnswered, title: format!("The Team Lead couldn't answer: {}", thread.title),
                 body: summary.error.as_deref().map(|e| short(e, BODY_CHARS)).unwrap_or_default(), route: format!("#/chat/{thread_id}") }
    } else {
        let answer = chat::messages(&st.db, thread_id).unwrap_or_default().into_iter().rev()
            .find(|m| m.role == "agent" && m.body_md.as_deref().is_some_and(|b| !b.trim().is_empty()))
            .and_then(|m| m.body_md).map(|b| short(&b, BODY_CHARS)).unwrap_or_default();
        Notice { kind: Kind::LeadAnswered, title: format!("The Team Lead answered: {}", thread.title), body: answer, route: format!("#/chat/{thread_id}") }
    };
    (st.desktop.show)(n.clone());
    Some(n)
}

/// The Team Lead merged `task`'s pull request `number` (merge_pull_request, GA-86): "The Team Lead merged GA-12's pull
/// request #34", with the card's title, when the Waiting switch is on, also while Gizai's window shows, so you see every
/// merge. Called before the PR check moves the card to Deploy: the Inbox's own "GA-12 is merged and waits for deploy"
/// then doesn't notify as well. Nothing while Gizai quits. Returns what it sent.
pub fn lead_merged(st: &AppState, task: &gizai_core::model::Task, number: u64) -> Option<Notice> {
    st.inbox.told.lock().unwrap().insert(format!("waiting:{}:deploy", task.id));
    if crate::runs::is_closing(st) || !switches(&st.db).waiting {
        return None;
    }
    let n = Notice { kind: Kind::LeadMerged, title: format!("The Team Lead merged {}'s pull request #{number}", task.identifier),
                     body: task.title.clone(), route: format!("#/task/{}", task.id) };
    (st.desktop.show)(n.clone());
    Some(n)
}

/// The real desktop: notify-rust, and Gizai's main window. In a headless test or screenshot run (`test_run`) a
/// notification is only written to stderr.
pub fn real(app: &AppHandle, test_run: bool) -> Desktop {
    let (a, b) = (app.clone(), app.clone());
    let show: Arc<dyn Fn(Notice) + Send + Sync> = if test_run {
        Arc::new(|n: Notice| eprintln!("gizai: notification (not shown in a test run): {} ({})", n.title, n.route))
    } else {
        Arc::new(move |n: Notice| {
            let app = a.clone();
            // Talking to the notification server blocks, and so does waiting for a click: on a thread of its own.
            let _ = std::thread::Builder::new().name("gizai-notification".into()).spawn(move || show_now(&app, n));
        })
    };
    Desktop { show, away: Arc::new(move || window_away(&b)) }
}

fn window_away(app: &AppHandle) -> bool {
    let Some(w) = app.get_webview_window("main") else { return true };
    !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false) || !w.is_focused().unwrap_or(false)
}

/// A click on a notification: the window shows, on the card or the chat.
#[cfg(not(target_os = "macos"))]
fn open(app: &AppHandle, route: &str) {
    crate::show_main(app);
    if let (Some(w), Ok(hash)) = (app.get_webview_window("main"), serde_json::to_string(route)) {
        let _ = w.eval(format!("window.location.hash = {hash}"));
    }
}

/// At most this many notifications wait for a click at once (each on a thread, until the notification closes): beyond
/// it a notification only informs.
#[cfg(not(target_os = "macos"))]
const MAX_WAITING: usize = 32;

/// Linux (freedesktop notifications, such as mako on Omarchy): the notification has a default action, which a click
/// invokes (mako: a left click). This thread waits for it until the notification closes.
#[cfg(all(unix, not(target_os = "macos")))]
fn show_now(app: &AppHandle, n: Notice) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static WAITING: AtomicUsize = AtomicUsize::new(0);
    let mut note = notify_rust::Notification::new();
    note.appname("Gizai").summary(&n.title).body(&body_text(&n.body)).icon("gizai")
        .hint(notify_rust::Hint::DesktopEntry("gizai".into()))
        .action("default", "Open");
    let handle = match note.show() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("gizai: a desktop notification couldn't be shown: {e}");
            return;
        }
    };
    if WAITING.fetch_add(1, Ordering::SeqCst) < MAX_WAITING {
        handle.wait_for_action(|action| {
            if action == "default" {
                open(app, &n.route);
            }
        });
    }
    WAITING.fetch_sub(1, Ordering::SeqCst);
}

/// Windows: a toast from Gizai (its AppUserModelID, `APP_ID`, which install.ps1's Start menu shortcut carries; without
/// that shortcut, as in a dev build, Windows shows no toast). A click on it while it shows opens the window on the card
/// or the chat; this thread waits for that until the toast goes (it times out into the notification centre, or is
/// dismissed). Gizai doesn't hear a click in the notification centre later.
#[cfg(windows)]
fn show_now(app: &AppHandle, n: Notice) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static WAITING: AtomicUsize = AtomicUsize::new(0);
    let mut note = notify_rust::Notification::new();
    note.app_id(crate::APP_ID).summary(&n.title).body(&n.body);
    let handle = match note.show() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("gizai: a desktop notification couldn't be shown: {e}");
            return;
        }
    };
    if WAITING.fetch_add(1, Ordering::SeqCst) < MAX_WAITING {
        // a click on the toast itself (not a button: it has none) is the default response
        let _ = handle.wait_for_response(|r: &notify_rust::NotificationResponse| {
            if matches!(r, notify_rust::NotificationResponse::Default) {
                open(app, &n.route);
            }
        });
    }
    WAITING.fetch_sub(1, Ordering::SeqCst);
}

/// macOS: the notification only informs (a click shows nothing more): the tray's Open Gizai and the Dock icon bring the
/// window back. It comes from Gizai (`APP_ID`, Gizai.app's bundle id) when macOS knows Gizai.app, else from notify-rust's
/// stand-in, Finder. Whether it shows is up to System Settings → Notifications.
#[cfg(target_os = "macos")]
fn show_now(_app: &AppHandle, n: Notice) {
    static APP: std::sync::Once = std::sync::Once::new();
    APP.call_once(|| {
        let _ = notify_rust::set_application(crate::APP_ID);
    });
    let mut note = notify_rust::Notification::new();
    note.appname("Gizai").summary(&n.title).body(&n.body);
    if let Err(e) = note.show() {
        eprintln!("gizai: a desktop notification couldn't be shown: {e}");
    }
}

/// The text under the first line, safe for a notification server that reads markup there (most do, mako too): `&`, `<`
/// and `>` show as written.
#[cfg(all(unix, not(target_os = "macos")))]
fn body_text(text: &str) -> String {
    static MARKUP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let markup = *MARKUP.get_or_init(|| notify_rust::get_capabilities().is_ok_and(|c| c.iter().any(|c| c == "body-markup")));
    if !markup {
        return text.to_string();
    }
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}
