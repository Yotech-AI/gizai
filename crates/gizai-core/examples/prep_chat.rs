//! Prepares a copy of the demo data for the UI chat test: points Claude Code at the fake chat Claude, and adds a second
//! Claude Code account (the same fake, a scratch CLAUDE_CONFIG_DIR it only reads) and a Codex for Runs on. No agents:
//! the probe sets up the Team Lead through the Chat page's setup panel.
//! usage: cargo run -p gizai-core --example prep_chat -- <data dir> <fake claude>
use gizai_core::{clis, db::Db, seed::ensure_seed, settings};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, fake) = (&a[1], &a[2]);
    let db = Db::open(&std::path::Path::new(dir).join("gizai.db")).unwrap();
    ensure_seed(&db, "Jeffrey").unwrap();
    settings::set(&db, "claude_bin", fake).unwrap();
    let acct = std::path::Path::new(dir).join("acct-2").display().to_string();
    clis::save(&db, vec![
        clis::Cli { name: "Claude Code 2".into(), kind: "claude_code".into(), command: fake.clone(), env: vec![format!("CLAUDE_CONFIG_DIR={acct}")], ..Default::default() },
        clis::Cli { name: "Codex".into(), kind: "codex".into(), command: "codex".into(), ..Default::default() },
    ]).unwrap();
}
