//! Quitting cleanly. WebKitGTK's page process (WebKitWebProcess) can crash in its own teardown once Gizai's
//! window goes away: its main thread, in exit(), frees Mesa's GBM device while a compositor thread still
//! releases its EGL state, and the heap breaks (a WebKitGTK/Mesa bug, seen as WebKitWebProcess core dumps).
//! So Gizai ends that process itself just before its window closes or Gizai exits, with WebKit's own call:
//! an instant kill, which runs no teardown and leaves no core dump. Signals quit the usual way, so they take
//! that path too.
//!
//! Agents at work (runs and chat answers) are stopped before Gizai quits, however it is asked to: the window, a
//! signal, logging out (which closes the window, then sends SIGTERM) or shutting down. They are recorded as stopped
//! because Gizai quit, and none outlives Gizai.
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::{AppState, chat, runs};

/// How long quitting waits for the agents it stops (Stop escalates to SIGKILL after 10 s, so 12 s is enough).
const STOP_WAIT: Duration = Duration::from_secs(12);
/// How long Gizai, as it exits, waits for the agents it ended at once to be recorded.
const END_WAIT: Duration = Duration::from_secs(4);

/// Set by the first request to quit while agents are at work: a second request quits at once.
static STOPPING: AtomicBool = AtomicBool::new(false);

/// A request to quit (the window closed, a signal, the app quitting itself). With agents at work, the first request
/// stops them and quits once they have ended (at most 12 s), so Gizai stays for now: returns true. A second request,
/// or one with no agents at work, quits at once.
pub fn stop_agents_first(app: &AppHandle) -> bool {
    let st = app.state::<AppState>().inner().clone();
    let busy = !runs::live(&st).is_empty() || !chat::live(&st).is_empty() || chat::checking(&st);
    if !busy || STOPPING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::join!(runs::stop_all(&st, STOP_WAIT), chat::stop_all(&st, STOP_WAIT));
        app.exit(0);
    });
    true
}

/// Gizai exits: agents still at work (asked to quit again while they were being stopped, or slow to stop) end at
/// once, SIGKILL to their process groups, and Gizai waits a moment (at most 4 s) until they are recorded as stopped
/// because Gizai quit. On the main thread, as Gizai's last step: the runs' own tasks record them meanwhile.
pub fn end_agents(st: &AppState) {
    if runs::kill_all(st) + chat::kill_all(st) == 0 {
        return;
    }
    let t0 = Instant::now();
    while (!runs::live(st).is_empty() || !chat::live(st).is_empty() || chat::checking(st)) && t0.elapsed() < END_WAIT {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Ends the main window's page process, if it still runs. Only for a window that is about to close or a
/// Gizai that is about to exit: the window shows nothing after it, and WebKit doesn't start a new one by
/// itself.
pub fn end_web_content(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    if let Some(w) = app.get_webview_window("main") {
        // On the main thread (setup and the event loop) this runs at once, not later.
        let _ = w.with_webview(|wv| {
            use webkit2gtk::WebViewExt;
            wv.inner().terminate_web_process();
        });
    }
}

/// SIGTERM (`kill`, logging out, the headless test scripts), SIGINT (Ctrl+C) and SIGHUP (a closed terminal)
/// quit Gizai the way closing its window does: agents at work are stopped first, and a second signal quits at
/// once. A signal that was ignored when Gizai started (nohup, a background job in a script) stays ignored.
pub fn on_signals(app: &AppHandle) {
    use tokio::signal::unix::{SignalKind, signal};
    // Registered now rather than in the tasks, so a signal right after setup quits this way too.
    let rt = tauri::async_runtime::handle();
    let _in_runtime = rt.inner().enter();
    for (sig, name) in [(libc::SIGTERM, "SIGTERM"), (libc::SIGINT, "SIGINT"), (libc::SIGHUP, "SIGHUP")] {
        if ignored(sig) {
            continue;
        }
        let Ok(mut signals) = signal(SignalKind::from_raw(sig)) else { continue };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            while signals.recv().await.is_some() {
                // At once, not after the event loop has heard: logging out and shutting down send SIGTERM to the
                // agents too, and a run or chat answer that ends now was stopped because Gizai quit (not failed).
                runs::mark_closing(app.state::<AppState>().inner());
                // So a log of Gizai's output shows why it quit.
                eprintln!("gizai: quitting on {name}");
                app.exit(0);
            }
        });
    }
}

/// Whether `sig` is set to be ignored (SIG_IGN).
fn ignored(sig: libc::c_int) -> bool {
    // SAFETY: with no new action, sigaction only reads the current one into memory we own.
    unsafe {
        let mut old: libc::sigaction = std::mem::zeroed();
        libc::sigaction(sig, std::ptr::null(), &mut old) == 0 && old.sa_sigaction == libc::SIG_IGN
    }
}
