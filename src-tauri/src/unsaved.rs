//! Unsaved text in the doc page's or the Memory page's editor when Gizai closes (GA-122). The page says when it has some
//! (`set_unsaved_edits`); only then do closing the window (its close button) and quitting (Quit Gizai completely in the
//! tray, Cmd+Q on macOS, a signal, restarting after an update) wait for the page. It saves the text as one version
//! (waiting at most 2 s), then Gizai goes on closing; when that fails or takes longer, the page asks first ("Close Gizai
//! without saving your changes to …?"), and Stay keeps Gizai as it was. The page answers with `close_answer`.
//!
//! A page that doesn't answer within PAGE_WAIT (it hangs, or its process has gone) doesn't keep Gizai from closing, and a
//! second request to quit while the page saves or asks quits at once, like a second signal while agents are stopped. The
//! page's own Quit (`exit_app`: Settings, a self-test's end) has nothing to wait for. macOS's own Quit (the Dock's,
//! logging out) and Windows logging off come straight to `RunEvent::Exit`, past this.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, runs, show_main};

/// How long Gizai waits for the page's answer (it saves for at most 2 s, then answers, or asks and says so).
const PAGE_WAIT: Duration = Duration::from_secs(5);

/// The page has unsaved text in an editor.
static UNSAVED: AtomicBool = AtomicBool::new(false);
/// This quit no longer waits for the page: it saved, Close was chosen, or a second request came.
static SETTLED: AtomicBool = AtomicBool::new(false);
/// The close or quit that waits for the page's answer.
static WAITING: Mutex<Option<Waiting>> = Mutex::new(None);
static NEXT: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy)]
struct Waiting {
    id: u64,
    then: Then,
    /// The page asks Close or Stay: Gizai waits for that answer, however long.
    asking: bool,
}

/// What Gizai does once the page has answered: hide the window (its close button) or quit with this exit code.
#[derive(Clone, Copy)]
enum Then {
    Hide,
    Quit(i32),
}

fn waiting() -> MutexGuard<'static, Option<Waiting>> {
    WAITING.lock().unwrap_or_else(|e| e.into_inner())
}

/// The page has unsaved text in an editor (or no longer has). On the main thread (GA-191), with `close_answer` and the
/// window's close and quit requests, which run there too: the page's words are taken in the order it said them, so a
/// "saved" never comes before the "unsaved" it answers. A flag in memory: no database or file work.
#[tauri::command]
pub fn set_unsaved_edits(unsaved: bool) {
    UNSAVED.store(unsaved, Ordering::SeqCst);
}

/// The window's close button: true when the page has unsaved text, so the window stays until the page answers (it hears
/// of the close as Tauri's close request). Otherwise the window hides at once, as before.
pub fn close_waits(app: &AppHandle) -> bool {
    if !UNSAVED.load(Ordering::SeqCst) {
        return false;
    }
    let mut w = waiting();
    // Already waiting for a close or a quit: that one goes on.
    if w.is_none() {
        let id = NEXT.fetch_add(1, Ordering::SeqCst);
        *w = Some(Waiting { id, then: Then::Hide, asking: false });
        drop(w);
        give_up_after(app, id);
    }
    true
}

/// A request to quit: true when the page has unsaved text to save first, so Gizai stays for now. A second request while
/// the page saves or asks quits at once.
pub fn quit_waits(app: &AppHandle, code: i32) -> bool {
    if SETTLED.load(Ordering::SeqCst) || !UNSAVED.load(Ordering::SeqCst) {
        return false;
    }
    let mut w = waiting();
    let started = match *w {
        Some(Waiting { then: Then::Quit(_), .. }) => {
            *w = None;
            SETTLED.store(true, Ordering::SeqCst);
            return false;
        }
        // A close the page saves or asks for becomes this quit: its answer quits.
        Some(x) => {
            *w = Some(Waiting { then: Then::Quit(code), ..x });
            None
        }
        None => {
            let id = NEXT.fetch_add(1, Ordering::SeqCst);
            *w = Some(Waiting { id, then: Then::Quit(code), asking: false });
            Some(id)
        }
    };
    drop(w);
    let _ = app.emit("quit-requested", ());
    if let Some(id) = started {
        give_up_after(app, id);
    }
    true
}

/// The page quits Gizai itself (`exit_app`): it has no unsaved text to wait for.
pub fn settle() {
    SETTLED.store(true, Ordering::SeqCst);
}

/// The page's answer: "asking" (the question shows: the window comes forward, and Gizai waits for Close or Stay),
/// "close" (saved, or Close: go on closing) or "stay" (Gizai stays as it was). On the main thread (GA-191): it shows,
/// hides or quits the window, in order with `set_unsaved_edits` and the window's close and quit requests. No database or
/// file work.
#[tauri::command]
pub fn close_answer(app: AppHandle, answer: String) {
    let mut w = waiting();
    match answer.as_str() {
        "asking" => {
            match w.as_mut() {
                Some(x) => x.asking = true,
                // The window hid at once (the page told of its unsaved text a moment too late): Close hides it again.
                None => *w = Some(Waiting { id: NEXT.fetch_add(1, Ordering::SeqCst), then: Then::Hide, asking: true }),
            }
            drop(w);
            show_main(&app);
        }
        "close" => {
            let x = w.take();
            drop(w);
            if let Some(x) = x {
                go_on(&app, x.then);
            }
        }
        _ => {
            let x = w.take();
            drop(w);
            // A signal marked Gizai as quitting: runs and chat answers start again. And only another Quit Gizai
            // completely backs up as it quits (see `backups`).
            if let Some(Waiting { then: Then::Quit(_), .. }) = x {
                runs::unmark_closing(app.state::<AppState>().inner());
                app.state::<AppState>().backups.quit_called_off();
            }
            show_main(&app);
        }
    }
}

fn go_on(app: &AppHandle, then: Then) {
    match then {
        Then::Hide => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        }
        Then::Quit(code) => {
            SETTLED.store(true, Ordering::SeqCst);
            app.exit(code);
        }
    }
}

/// A page that hasn't answered (nor asks) after PAGE_WAIT doesn't keep Gizai from closing.
fn give_up_after(app: &AppHandle, id: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(PAGE_WAIT).await;
        let mut w = waiting();
        let Some(x) = *w else { return };
        if x.id != id || x.asking {
            return;
        }
        *w = None;
        drop(w);
        go_on(&app, x.then);
    });
}
