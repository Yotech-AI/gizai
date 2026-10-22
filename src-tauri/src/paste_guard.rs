//! Linux (GA-222): Gizai's own `gtk_clipboard_request_targets`, which keeps WebKitGTK 2.54.1 from crashing Gizai on a
//! paste while nothing is copied, and the command for its self-test step (scripts/ui-test.sh).
use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, OnceLock};

use tauri::{AppHandle, Manager};

/// GTK3's `GdkAtom`: one format on the clipboard (text, HTML, an image, files …).
pub type GdkAtom = *mut c_void;
/// GTK3's `GtkClipboardTargetsReceivedFunc`: the clipboard, its formats, how many, and the caller's data.
pub type TargetsReceived = unsafe extern "C" fn(*mut c_void, *mut GdkAtom, c_int, *mut c_void);
/// GTK3's `gtk_clipboard_request_targets`.
type RequestTargets = unsafe extern "C" fn(*mut c_void, Option<TargetsReceived>, *mut c_void);

/// Asks GTK3 which formats the clipboard has, like GTK's own function of this name, and hands the answer to the
/// caller's callback through `targets_for_caller`. Gizai's binary exports it (build.rs), so WebKitGTK's calls come here.
/// Why: with nothing copied (on Wayland the clipboard empties when the app you copied from closes), GTK3 answers with no
/// list and a count of -1. WebKitGTK 2.54.1 checks only for 0, reads the missing list as a huge one, and Gizai crashes,
/// whichever way WebKit pastes (Ctrl+V, Shift+Insert, the right-click menu's Paste, middle-click). With a count of 0
/// WebKit's own check works and nothing is pasted, as before 2.54.1. WebKit's fix is commit 75cdc5a66f84 (bug 324466);
/// this wrapper, its link argument and its self-test step can go once WebKitGTK 2.54.1 is no longer in use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gtk_clipboard_request_targets(clipboard: *mut c_void, callback: Option<TargetsReceived>, data: *mut c_void) {
    let Some(own) = gtk_own() else {
        // no GTK3 to ask (never in Gizai, which loads it before anything can paste): nothing to paste
        if let Some(callback) = callback {
            // SAFETY: the caller's callback with its own data, as GTK calls it when it finds no formats.
            unsafe { callback(clipboard, std::ptr::null_mut(), 0, data) };
        }
        return;
    };
    match callback {
        Some(callback) if !clipboard.is_null() => {
            let pending = Box::into_raw(Box::new(Pending { callback, data }));
            // SAFETY: GTK3's own function, which calls `answered` once, with `pending`.
            unsafe { own(clipboard, Some(answered), pending.cast()) }
        }
        // GTK refuses a call without a clipboard or a callback and calls nothing back: it still does, as before
        // SAFETY: GTK3's own function, with the caller's own arguments.
        _ => unsafe { own(clipboard, callback, data) },
    }
}

/// What the caller's callback gets for the list of formats and the count GTK3 gives: a count below 0 (GTK3's answer when
/// nothing is copied) becomes no list and a count of 0; anything else passes through unchanged.
pub fn targets_for_caller(atoms: *mut GdkAtom, n_atoms: c_int) -> (*mut GdkAtom, c_int) {
    if n_atoms < 0 { (std::ptr::null_mut(), 0) } else { (atoms, n_atoms) }
}

/// How often the wrapper has turned a count below 0 into 0 so far. Counted in a self-test run only (GIZAI_SELFTEST):
/// elsewhere it stays 0.
pub fn guarded() -> u64 {
    GUARDED.load(Ordering::Relaxed)
}

static GUARDED: AtomicU64 = AtomicU64::new(0);

/// The caller's callback and its data, while GTK looks up the formats.
struct Pending {
    callback: TargetsReceived,
    data: *mut c_void,
}

/// GTK3's answer, handed on to the caller's callback through `targets_for_caller`.
unsafe extern "C" fn answered(clipboard: *mut c_void, atoms: *mut GdkAtom, n_atoms: c_int, pending: *mut c_void) {
    // SAFETY: the box `gtk_clipboard_request_targets` made for this request; GTK answers each request once.
    let Pending { callback, data } = *unsafe { Box::from_raw(pending.cast::<Pending>()) };
    let (atoms_out, n_out) = targets_for_caller(atoms, n_atoms);
    if n_out != n_atoms && selftest_run() {
        GUARDED.fetch_add(1, Ordering::Relaxed);
    }
    // SAFETY: the caller's callback with its own data; the list stays GTK's, and valid until this returns.
    unsafe { callback(clipboard, atoms_out, n_out, data) }
}

/// GTK3's own `gtk_clipboard_request_targets`: the next one after Gizai's binary, in libgtk-3.
fn gtk_own() -> Option<RequestTargets> {
    static OWN: OnceLock<Option<RequestTargets>> = OnceLock::new();
    *OWN.get_or_init(|| {
        // SAFETY: a NUL-terminated name; RTLD_NEXT looks only in what was loaded after the binary this code is in.
        let f = unsafe { libc::dlsym(libc::RTLD_NEXT, c"gtk_clipboard_request_targets".as_ptr()) };
        // never this wrapper itself, which would call itself without end
        if f.is_null() || std::ptr::addr_eq(f, gtk_clipboard_request_targets as *const ()) {
            return None;
        }
        // SAFETY: GTK3's function, whose C signature RequestTargets is.
        Some(unsafe { std::mem::transmute::<*mut c_void, RequestTargets>(f) })
    })
}

/// Whether this is a self-test run: GIZAI_SELFTEST is set (scripts/smoke-cage.sh), as `app_info` reads it.
fn selftest_run() -> bool {
    static SELFTEST: LazyLock<bool> = LazyLock::new(|| std::env::var("GIZAI_SELFTEST").is_ok());
    *SELFTEST
}

/// The WebKit editing command `selftest_paste` runs for `command`: none, "Paste" or "PasteGlobalSelection". Refused
/// outside a self-test run, and for any other command.
pub fn paste_command(selftest: bool, command: Option<&str>) -> Result<Option<&'static str>, String> {
    if !selftest {
        return Err("only in a self-test run".into());
    }
    match command {
        None => Ok(None),
        Some("Paste") => Ok(Some("Paste")),
        Some("PasteGlobalSelection") => Ok(Some("PasteGlobalSelection")),
        Some(other) => Err(format!("not a paste command: {other}")),
    }
}

/// The wrapper's self-test step (scripts/ui-test.sh, in a cage of its own, whose clipboard starts empty). Puts
/// `clipboard`, when given, on the clipboard (on the primary selection for PasteGlobalSelection), then makes the main
/// window's web view paste into the field that has the focus, with WebKit's editing `command`: "Paste" the way Ctrl+V,
/// Shift+Insert and the right-click menu's Paste do, or "PasteGlobalSelection" the way middle-click does. Returns
/// `guarded()` as it is before this paste: WebKit pastes after this returns. With neither, it only returns that count.
/// Refused outside a self-test run.
#[tauri::command]
pub fn selftest_paste(app: AppHandle, command: Option<String>, clipboard: Option<String>) -> Result<u64, String> {
    let command = paste_command(selftest_run(), command.as_deref())?;
    if command.is_some() || clipboard.is_some() {
        let window = app.get_webview_window("main").ok_or("no main window")?;
        // A command without async runs on the main thread, so this runs at once, not later (as in quit::end_web_content).
        window
            .with_webview(move |wv| {
                use webkit2gtk::WebViewExt;
                if let Some(text) = &clipboard {
                    let selection = match command {
                        Some("PasteGlobalSelection") => gtk::gdk::SELECTION_PRIMARY,
                        _ => gtk::gdk::SELECTION_CLIPBOARD,
                    };
                    gtk::Clipboard::get(&selection).set_text(text);
                }
                if let Some(command) = command {
                    wv.inner().execute_editing_command(command);
                }
            })
            .map_err(|e| e.to_string())?;
    }
    Ok(guarded())
}
