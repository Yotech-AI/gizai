//! Prepares a copy of the demo data for the UI Memory test (GA-68): a paused Backend Agent, Team Lead and Backend Agent 2 (agents
//! paused in Settings too, Claude Code pointed at the fake), and notes that link each other: Decisions/Use SQLite links
//! Workflows/Deploy steps (also its #Rollback heading), a note that doesn't exist yet (Backup plan), embeds
//! Standards/Release checklist and names the card KADE-1; Lessons/Flaky tests names Deploy steps without a link (an
//! unlinked mention). The Backend Agent learned a line in a run on KADE-1 (Recently changed: an agent, in a run).
//! Prints the id of Decisions/Use SQLite, then the Backend Agent's id, on one line.
//! usage: cargo run -p gizai-core --example prep_memory -- <data dir> <fake claude>
use gizai_core::memory::{self, Who};
use gizai_core::{db::Db, ids, model::*, runs, seed::ensure_seed, settings, tasks, team};

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
    // the Backend Agent first: Memory still puts the Team Lead first
    let backend = agent("Backend Agent", "backend", None);
    let lead = agent("Team Lead", "lead", Some(true));
    // a name that starts like the Backend Agent's: its folder is Agents/Backend Agent 2, which the Backend Agent's page never shows
    agent("Backend Agent 2", "backend", None);
    memory::ensure_lead_notes(&db, &lead, "Jeffrey").unwrap();

    let you = Who::Person(s.you_id.clone());
    let note = |path: &str, body: &str| memory::write(&db, &you, path, body, None, None).unwrap().id;
    let sqlite = note("Decisions/Use SQLite", "---\ntype: decision\ntags: [storage]\n---\n# Use SQLite\n\n\
Data stays on this computer. Deploy with [[Deploy steps]]; [[Deploy steps#Rollback|roll back]] when it breaks. Not written \
yet: [[Backup plan]].\n\n![[Release checklist]]\n\nFirst seen on KADE-1.\n");
    note("Workflows/Deploy steps", "---\ntype: workflow\ntags: [ops]\n---\n# Deploy steps\n\n## Build\n\nRun the release build \
PREVIEW-MARK.\n\n## Rollback\n\nInstall the release before.\n");
    note("Standards/Release checklist", "Read the changelog before a release EMBED-MARK.\n");
    note("Lessons/Flaky tests", "The Deploy steps note says how to deploy; tests wait for the lock.\n");

    let all = tasks::list(&db, &TaskFilter::default()).unwrap();
    let kade1 = all.iter().find(|t| t.identifier == "KADE-1").unwrap().id.clone();
    let run = runs::create(&db, &backend, &kade1, "backend", &ids::new_id(), "/tmp", "/tmp", "b", "/tmp/l").unwrap();
    runs::finish(&db, &run, "succeeded", None, 0, 0, 0, None).unwrap();
    memory::learned(&db, &backend, "KADE-1", &["Run the fake CLI in tests.".to_string()], Some(&run), "2026-10-10").unwrap();
    println!("{sqlite} {backend}");
}
