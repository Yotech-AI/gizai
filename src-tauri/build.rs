fn main() {
    // Linux (GA-222): the gizai binary exports its own gtk_clipboard_request_targets (src/paste_guard.rs), so WebKitGTK's
    // calls reach it rather than GTK's. Exported, it is also never dropped as unused when the binary is linked. The
    // target's system, not this build script's: cfg!(target_os) here would be the one building.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bin=gizai=-Wl,--export-dynamic-symbol=gtk_clipboard_request_targets");
    }
    tauri_build::build()
}
