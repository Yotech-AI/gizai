//! One real Claude Code chat turn against a scratch data folder, to check the flags, the MCP shim and a tool
//! call end to end. Run by hand (it spends a little: one short turn); never part of the test suite.
//! usage: GIZAI_CHAT_NO_PERSIST=1 cargo run -p gizai --example chat_probe -- <scratch dir> [model] [message]
//! A second message after the first runs a second, resumed turn: that checks how Claude Code reports cost for a
//! resumed session. Resuming needs a saved session, so leave GIZAI_CHAT_NO_PERSIST unset for it; Claude Code then
//! writes one session file under ~/.claude/projects.
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use gizai_core::model::{AgentInput, ProjectInput};
use gizai_core::{chat, projects, runs, team};
use gizai_lib::runs::Note;

#[tokio::main]
async fn main() {
    let a: Vec<String> = std::env::args().collect();
    let dir = PathBuf::from(a.get(1).expect("usage: chat_probe <scratch dir> [model] [message]"));
    let model = a.get(2).cloned().unwrap_or_else(|| "haiku".into());
    let text = a.get(3).cloned().unwrap_or_else(|| "Use get_overview, then tell me in one sentence how many projects there are and their keys.".into());
    let mut st = gizai_lib::open_state(dir.join("data"), Arc::new(|n: Note| {
        if let Note::Chat { event, .. } = n {
            let v = serde_json::to_value(&event).unwrap();
            match v["kind"].as_str() {
                Some("delta") => { print!("{}", v["text"].as_str().unwrap_or("")); let _ = std::io::stdout().flush(); }
                Some("tool") => println!("\n[tool] {}", v["name"]),
                _ => {}
            }
        }
    })).unwrap();
    st.mcp_socket = gizai_lib::mcp::socket_path(&st.data_dir);
    st.mcp_shim = Some(PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/debug/gizai-mcp")));
    if projects::list(&st.db).unwrap().is_empty() {
        projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    }
    if team::chat_agent(&st.db).unwrap().is_none() {
        let tid = team::list(&st.db).unwrap()[0].id.clone();
        team::add_agent(&st.db, &st.you_id, &tid, AgentInput { name: "Team Lead".into(), role_key: "lead".into(), model: Some(model),
            chat_enabled: Some(true), ..Default::default() }).unwrap();
    }
    println!("claude: {:?}", gizai_lib::runs::detect_claude(&st));
    let _server = gizai_lib::mcp::start(&st).unwrap();
    let (thread, done) = gizai_lib::chat::send(&st, None, text, None).await.unwrap();
    let mut summary = tokio::time::timeout(std::time::Duration::from_secs(240), done).await.expect("turn finished in 4 min").unwrap();
    println!("\n\n== summary: {summary:?}");
    if let Some(second) = a.get(4).cloned() {
        let first = runs::get(&st.db, &summary.run_id).unwrap();
        let t1 = chat::get_thread(&st.db, &thread).unwrap();
        println!("== turn 1: session {:?}, run cost ${:.4}, thread cumulative ${:.4}", t1.session_id, first.cost_usd_micros as f64 / 1e6, t1.cost_usd_micros as f64 / 1e6);
        let (_, done) = gizai_lib::chat::send(&st, Some(thread.clone()), second, None).await.unwrap();
        summary = tokio::time::timeout(std::time::Duration::from_secs(240), done).await.expect("turn finished in 4 min").unwrap();
        let t2 = chat::get_thread(&st.db, &thread).unwrap();
        let second_run = runs::get(&st.db, &summary.run_id).unwrap();
        println!("\n== turn 2: {summary:?}");
        println!("== turn 2: session {:?} (same as turn 1: {}), run cost ${:.4}, thread cumulative ${:.4}", t2.session_id, t2.session_id == t1.session_id,
                 second_run.cost_usd_micros as f64 / 1e6, t2.cost_usd_micros as f64 / 1e6);
        println!("== if Claude Code reports cumulative totals, the thread's cumulative is about turn 1 + turn 2; if not, it is about turn 2 alone.");
    }
    for m in chat::messages(&st.db, &thread).unwrap() {
        println!("-- {} {}: {}", m.role, m.tool_name.clone().unwrap_or_default(), m.body_md.clone().unwrap_or_else(|| m.tool.map(|t| t.to_string()).unwrap_or_default()).chars().take(400).collect::<String>());
    }
    let r = runs::get(&st.db, &summary.run_id).unwrap();
    println!("== run: status {} cost ${:.4} tokens in {} out {} error {:?}", r.status, r.cost_usd_micros as f64 / 1e6, r.input_tokens, r.output_tokens, r.error);
    println!("== log: {}", r.log_path);
    gizai_lib::mcp::remove_socket(&st);
}
