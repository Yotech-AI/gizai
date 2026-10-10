//! Prepares a copy of the demo data for the UI graph test (GA-69): a paused Team Lead and Backend Agent (agents paused in
//! Settings too, Claude Code pointed at the fake) and 500 notes. 480 are "Note 0" to "Note 479" spread over the shared
//! folders and the Team Lead's; each links two or three others, every twelfth links Note 0 (a hub), every fortieth a note
//! that doesn't exist yet ("Missing <n>"), every tenth names KADE-1, every 25th mentions the Backend Agent and every
//! eighth has a #topic tag. The other 20 are "Agent note 0" to "Agent note 19" in the Backend Agent's own folder: each
//! links the next, names KADE-2 (never KADE-1) and links "Not here <n>", which doesn't exist.
//! Prints the Backend Agent's id, then the id of Note 0, on one line.
//! usage: cargo run -p gizai-core --example prep_graph -- <data dir> <fake claude>
use gizai_core::memory::{self, Who};
use gizai_core::{db::Db, model::*, seed::ensure_seed, settings, team};

const FOLDERS: [&str; 9] = ["Decisions", "Workflows", "Standards", "Lessons", "Clients", "Projects", "Deployments", "Dependencies", "Team Lead"];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, fake) = (&a[1], &a[2]);
    let db = Db::open(&std::path::Path::new(dir).join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    settings::set(&db, "claude_bin", fake).unwrap();
    settings::set(&db, "agents_paused", &true).unwrap();
    let agent = |name: &str, role: &str, chat: Option<bool>| {
        let id = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), chat_enabled: chat, ..Default::default() }).unwrap();
        team::set_agent_status(&db, &s.you_id, &id, "paused").unwrap();
        id
    };
    let lead = agent("Team Lead", "lead", Some(true));
    let backend = agent("Backend Agent", "backend", None);
    memory::ensure_lead_notes(&db, &lead, "Jeffrey").unwrap();
    let handle = team::get(&db, &s.team_id).unwrap().members.into_iter().find(|m| m.actor_id == backend).unwrap().handle;

    let you = Who::Person(s.you_id.clone());
    let note = |path: &str, body: &str| memory::write(&db, &you, path, body, None, None).unwrap().id;
    let mut hub = String::new();
    for i in 0..480usize {
        let mut body = format!("# Note {i}\n\nSee [[Note {}]] and [[Note {}]].", (i * 7 + 1) % 480, (i * 13 + 5) % 480);
        if i % 3 == 0 { body.push_str(&format!(" Next: [[Note {}]].", (i + 1) % 480)); }
        if i % 12 == 0 && i > 0 { body.push_str(" Back to [[Note 0]]."); }
        if i % 40 == 0 { body.push_str(&format!(" Not written yet: [[Missing {i}]].")); }
        if i % 10 == 0 { body.push_str(" First seen on KADE-1."); }
        if i % 25 == 0 { body.push_str(&format!(" Asked @{handle}.")); }
        if i % 8 == 0 { body.push_str(&format!("\n\n#topic{}", i % 3)); }
        let id = note(&format!("{}/Note {i}", FOLDERS[i % FOLDERS.len()]), &body);
        if i == 0 { hub = id; }
    }
    for j in 0..20usize {
        note(&format!("Agents/Backend Agent/Agent note {j}"), &format!("[[Agent note {}]] on KADE-2. Later: [[Not here {j}]].", (j + 1) % 20));
    }
    println!("{backend} {hub}");
}
