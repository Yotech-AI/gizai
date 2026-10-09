//! Prepares a copy of the demo data for the UI run test: links project KADE to a git repo, adds a Backend
//! Agent (it lands on To do and In progress), points Claude Code at the fake, and makes KADE-1 hang until stopped.
//! usage: cargo run -p gizai-core --example prep_run -- <data dir> <repo> <fake claude>
use gizai_core::{db::Db, model::*, projects, seed::ensure_seed, settings, tasks, team};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, repo, fake) = (&a[1], &a[2], &a[3]);
    let db = Db::open(&std::path::Path::new(dir).join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::list(&db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(&db, &s.you_id, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), client_id: p.client_id.clone(),
        repo_path: Some(repo.clone()), default_branch: Some("main".into()), color: p.color.clone(), goal_md: p.goal_md.clone(), ..Default::default() }).unwrap();
    team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: "Backend Agent".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    settings::set(&db, "claude_bin", fake).unwrap();
    let t = tasks::list(&db, &TaskFilter::default()).unwrap().into_iter().find(|t| t.identifier == "KADE-1").unwrap();
    tasks::update(&db, &s.you_id, &t.id, TaskPatch { description_md: Some("Export as CSV. FAKE_HANG".into()), ..Default::default() }).unwrap();
    println!("{}", t.id);
}
