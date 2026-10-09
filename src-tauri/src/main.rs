// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

const HELP: &str = "Gizai: clients, projects and tasks, worked on by local AI agents.

Usage: gizai [--version | --help | --backup [name]]

Without options it opens the app (one at a time: starting it again brings the open window forward).
--backup  saves a snapshot of your data in <data folder>/backups (gizai-<name>-<time>.db, name \"manual\" by default) and prints where. Your data lives in ~/.local/share/gizai (GIZAI_DATA_DIR overrides it).";

fn main() {
    // macOS: the CLIs your shell finds, also when Gizai starts from the Dock (first, before any thread starts).
    gizai_lib::shell_path::adopt_login_path();
    let args: Vec<String> = std::env::args().skip(1).collect();
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
