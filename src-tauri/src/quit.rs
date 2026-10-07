//! Quitting cleanly. WebKitGTK's page process (WebKitWebProcess) can crash in its own teardown once Gizai's
//! window goes away: its main thread, in exit(), frees Mesa's GBM device while a compositor thread still
//! releases its EGL state, and the heap breaks (a WebKitGTK/Mesa bug, seen as WebKitWebProcess core dumps).
//! So Gizai ends that process itself just before its window closes or Gizai exits, with WebKit's own call:
//! an instant kill, which runs no teardown and leaves no core dump. Signals quit the usual way, so they take
//! that path too.
use tauri::{AppHandle, Manager};

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
