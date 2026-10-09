//! Prepares a copy of the demo data for the UI Usage test (GA-33): three paused agents (agents paused in Settings too, and
//! Claude Code pointed at the fake) and finished runs with their tokens and cost. Today: the Backend Agent on KADE ($0.42)
//! and GFW ($0.15), a Codex run on KADE with tokens but no cost (unknown), and a chat turn of the Team Lead ($0.03);
//! 20 days ago: the Backend Agent on KADE ($1.00).
//! usage: cargo run -p gizai-core --example prep_usage -- <data dir> <fake claude>
use gizai_core::{chat, db::Db, ids, model::*, runs, seed::ensure_seed, settings, tasks, team};

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
    let backend = agent("Backend Agent", "backend", None);
    let codex = agent("Codex Agent", "backend", None);
    let lead = agent("Team Lead", "lead", Some(true));

    let all = tasks::list(&db, &TaskFilter::default()).unwrap();
    let card = |prefix: &str, n: usize| all.iter().filter(|t| t.identifier.starts_with(prefix)).nth(n).unwrap().id.clone();
    let run = |agent: &str, task: String, cost: i64, input: i64, output: i64| {
        let r = runs::create(&db, agent, &task, "backend", &ids::new_id(), "/tmp", "/tmp", "b", "/tmp/l").unwrap();
        runs::finish(&db, &r, "succeeded", None, cost, input, output, None).unwrap();
        r
    };
    run(&backend, card("KADE-", 0), 420_000, 12_000, 3_000);
    run(&backend, card("GFW-", 0), 150_000, 4_000, 400);
    run(&codex, card("KADE-", 1), 0, 5_000, 500);
    let th = chat::create_thread(&db, &s.you_id, &lead, "What is next?").unwrap();
    let turn = runs::create_chat(&db, &lead, &th, &ids::new_id(), "/tmp/lead", "/tmp/r.jsonl").unwrap();
    runs::finish_chat(&db, &turn, "succeeded", 30_000, 2_000, 100, None).unwrap();
    let old = run(&backend, card("KADE-", 2), 1_000_000, 1_000, 100);
    db.write(None, |w| {
        w.conn().execute("UPDATE runs SET created_at = ?2 WHERE id = ?1", rusqlite::params![old, ids::now_ms() - 20 * 86_400_000])?;
        Ok(())
    }).unwrap();
}
