//! Prepares a copy of the demo data for the UI Usage test (GA-33): three paused agents (agents paused in Settings too, and
//! Claude Code pointed at the fake) and finished runs with their tokens and cost. Today: the Backend Agent on KADE ($0.42)
//! and GFW ($0.15), a Codex run on KADE with tokens but no cost (unknown), and a chat turn of the Team Lead ($0.03);
//! 20 days ago: the Backend Agent on KADE ($1.00).
//! GA-62, the Subscription tab: Settings → Coding CLIs also has Claude Code 2 (with the Team Lead on it), Codex (with the
//! Codex Agent) and Gemini, and the limits their runs reported: Claude Code's session 42% and weekly 85% (no Fable number),
//! Claude Code 2's session limit reached (in words) and weekly 35%, Codex's 5-hour 23.5% and weekly 41%.
//! usage: cargo run -p gizai-core --example prep_usage -- <data dir> <fake claude>
use gizai_core::clis::{self, Cli};
use gizai_core::limits::{self, Reading};
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

    // GA-62: the coding CLIs and the limits they reported. The agents move after their runs, so the Usage tabs stay as above.
    let all = clis::save(&db, vec![
        Cli { name: "Claude Code 2".into(), kind: "claude_code".into(), command: fake.clone(), env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..Default::default() },
        Cli { name: "Codex".into(), kind: "codex".into(), command: "codex".into(), env: vec!["CODEX_HOME=~/.codex-work".into()], ..Default::default() },
        Cli { name: "Gemini".into(), kind: "gemini".into(), command: "gemini".into(), ..Default::default() },
    ]).unwrap();
    let id = |n: &str| all.iter().find(|c| c.name == n).unwrap().id.clone();
    let (cc2, codex_cli) = (id("Claude Code 2"), id("Codex"));
    db.write(None, |w| {
        w.conn().execute("UPDATE agent_configs SET adapter = ?2 WHERE actor_id = ?1", rusqlite::params![lead, cc2])?;
        w.conn().execute("UPDATE agent_configs SET adapter = ?2 WHERE actor_id = ?1", rusqlite::params![codex, codex_cli])?;
        Ok(())
    }).unwrap();
    let now = ids::now_ms();
    let (min, hour, day) = (60_000, 3_600_000, 86_400_000);
    let read = |key: &str, used: f64, resets_in: i64, minutes: i64| Reading {
        key: key.into(), used_percent: Some(used), resets_at: Some(now + resets_in), window_minutes: Some(minutes), observed_at: now - 10 * min, ..Default::default()
    };
    limits::record(&db, clis::CLAUDE_CODE, &[read(limits::FIVE_HOUR, 42.0, 2 * hour, 300), read(limits::SEVEN_DAY, 85.0, 4 * day, 10_080)]).unwrap();
    limits::record(&db, &cc2, &[limits::hit("session limit", Some("3pm (Europe/Amsterdam)"), None, now - 5 * min), read(limits::SEVEN_DAY, 35.0, 5 * day, 10_080)]).unwrap();
    limits::record(&db, &codex_cli, &[read(limits::PRIMARY, 23.5, 3 * hour, 300), read(limits::SECONDARY, 41.0, 6 * day, 10_080)]).unwrap();
}
