//! Quitting cleanly. WebKitGTK's page process (WebKitWebProcess) can crash in its own teardown once Gizai's
//! window goes away: its main thread, in exit(), frees Mesa's GBM device while a compositor thread still
//! releases its EGL state, and the heap breaks (a WebKitGTK/Mesa bug, seen as WebKitWebProcess core dumps).
//! So Gizai ends that process itself just before its window closes or Gizai exits, with WebKit's own call:
//! an instant kill, which runs no teardown and leaves no core dump.
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
