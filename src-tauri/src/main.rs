// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// The help, with where your data lives on this system.
macro_rules! help {
    ($data:literal) => {
        concat!("Gizai: clients, projects and tasks, worked on by local AI agents.

Usage: gizai [--version | --help | --backup [name]]

Without options it opens the app (one at a time: starting it again brings the open window forward).
--backup  saves a snapshot of your data in <data folder>/backups (gizai-<name>-<time>.db, name \"manual\" by default) and prints where. Your data lives in ", $data, " (GIZAI_DATA_DIR overrides it).")
    };
}

#[cfg(not(any(target_os = "macos", windows)))]
const HELP: &str = help!("~/.local/share/gizai");
#[cfg(target_os = "macos")]
const HELP: &str = help!("~/Library/Application Support/Gizai");
#[cfg(windows)]
const HELP: &str = help!(r"%APPDATA%\Gizai");

fn main() {
    // macOS: the CLIs your shell finds, also when Gizai starts from the Dock (first, before any thread starts).
    gizai_lib::shell_path::adopt_login_path();
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    if args.iter().any(|a| ["--version", "-V", "--help", "-h", "--backup"].contains(&a.as_str())) {
        attach_console();
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("gizai {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--backup") {
        let label = args.get(i + 1).map(String::as_str).unwrap_or("manual");
        match gizai_lib::backup_data_dir(&gizai_lib::data_dir(), label) {
            Ok(p) => println!("{}", p.display()),
            Err(e) => { eprintln!("gizai: {e}"); std::process::exit(1); }
        }
        return;
    }
    gizai_lib::run()
}

/// Windows: an installed Gizai is a window app with no console, so what it prints for an option would go nowhere in the
/// terminal it was started from: it prints into that terminal's console instead. Output that goes to a pipe already (an
/// update reading `--version`, install.ps1) stays there. Nothing happens without a console to join.
#[cfg(windows)]
fn attach_console() {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
    // SAFETY: plain calls without pointers; a failed attach changes nothing.
    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if out.is_null() || out == INVALID_HANDLE_VALUE {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
