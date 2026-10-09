//! Prepares a copy of the demo data for the UI Chat archive test (GA-46): a paused Team Lead with Chat on (agents paused in
//! Settings too, Claude Code pointed at the fake) and 36 chats, more than Recent's 30. "Chat 1" … "Chat 35", Chat n's last
//! activity n hours ago, each with a message of yours; the oldest, Chat 35, has the Team Lead's "The old flamingo plan is in
//! the docs."; Chat 34 has "We are 50% done"; Chat 33 has "flamingo" only in a tool call and in Gizai's note, which the search
//! skips. The newest is a Team Lead question, "Which database?", half an hour ago (labelled Question).
//! usage: cargo run -p gizai-core --example prep_chats -- <data dir> <fake claude>
use gizai_core::{chat, db::Db, ids, model::*, seed::ensure_seed, settings, team};
use serde_json::json;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, fake) = (&a[1], &a[2]);
    let db = Db::open(&std::path::Path::new(dir).join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    settings::set(&db, "claude_bin", fake).unwrap();
    settings::set(&db, "agents_paused", &true).unwrap();
    let lead = team::add_agent(&db, &s.you_id, &s.team_id,
        AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    team::set_agent_status(&db, &s.you_id, &lead, "paused").unwrap();

    let say = |thread: &str, role: &str, body: &str| {
        let author = if role == "user" { s.you_id.clone() } else { lead.clone() };
        let tool = (role == "tool").then(|| json!({"id": "toolu_1", "input": {"query": body}, "result": "{}", "isError": false}));
        chat::add_message(&db, chat::NewMessage { thread_id: thread.into(), role: role.into(), author_id: Some(author), body_md: Some(body.into()),
                                                  tool_name: tool.as_ref().map(|_| "mcp__gizai__list_tasks".to_string()), tool, ..Default::default() })
            .unwrap();
    };
    let at = |thread: &str, ago_ms: i64| {
        db.write(None, |w| {
            w.conn().execute("UPDATE chat_threads SET updated_at = ?2 WHERE id = ?1", rusqlite::params![thread, ids::now_ms() - ago_ms])?;
            Ok(())
        }).unwrap();
    };
    for n in 1..=35 {
        let th = chat::create_thread(&db, &s.you_id, &lead, &format!("Chat {n}")).unwrap();
        say(&th, "user", &format!("Hello from chat {n}"));
        match n {
            35 => say(&th, "agent", "The old flamingo plan is in the docs."),
            34 => say(&th, "user", "We are 50% done"),
            33 => { say(&th, "tool", "flamingo"); say(&th, "system", "flamingo note"); }
            _ => {}
        }
        at(&th, n * 3_600_000);
    }
    let (q, _) = chat::start_lead_chat(&db, &lead, "Which database?", "question", &[], "Postgres or SQLite for the portal?", None).unwrap();
    at(&q, 1_800_000);
}
