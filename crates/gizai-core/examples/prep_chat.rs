//! Prepares a copy of the demo data for the UI chat test: points Claude Code at the fake chat Claude. No
//! agents: the probe sets up the Team Lead through the Chat page's setup panel.
//! usage: cargo run -p gizai-core --example prep_chat -- <data dir> <fake claude>
use gizai_core::{db::Db, seed::ensure_seed, settings};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, fake) = (&a[1], &a[2]);
    let db = Db::open(&std::path::Path::new(dir).join("gizai.db")).unwrap();
    ensure_seed(&db, "Jeffrey").unwrap();
    settings::set(&db, "claude_bin", fake).unwrap();
}
