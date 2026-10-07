//! Adds a few agents to a copy of the demo data, for README screenshots of the org chart and agent pages.
//! usage: cargo run -p gizai-core --example prep_shots -- <data dir>
use gizai_core::{db::Db, model::AgentInput, seed::ensure_seed, team};

fn main() {
    let dir = std::env::args().nth(1).expect("data dir");
    let db = Db::open(&std::path::Path::new(&dir).join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let have: Vec<String> = team::all_agents(&db).unwrap().into_iter().map(|(_, m)| m.name).collect();
    for (name, role, wakeup) in [("Frontend Agent", "frontend", "on_assign"), ("Backend Agent", "backend", "heartbeat"), ("Design Agent", "design", "manual"), ("QA Agent", "qa", "on_assign")] {
        if !have.iter().any(|n| n == name) {
            team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), wakeup: wakeup.into(),
                heartbeat_minutes: Some(30), ..Default::default() }).unwrap();
        }
    }
}
