//! The README's demo data, on top of `example demo`: a team of agents with a Team Lead, a short chat with it, and
//! project KADE linked to `<git repo>` with Claude Code pointed at `<fake claude>`, so `scripts/readme-gif.sh` can
//! show a card being worked on (the Backend Agent's instructions end with FAKE_HANG, which keeps the fake running).
//! usage: cargo run -p gizai-core --example prep_readme -- <data dir> <git repo> <fake claude>
use gizai_core::chat::{self, NewMessage};
use gizai_core::model::{AgentInput, ProjectInput, TaskInput, TaskPatch};
use gizai_core::{db::Db, projects, seed, settings, tasks, team};
use serde_json::json;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (dir, repo, fake) = (&a[1], &a[2], &a[3]);
    let db = Db::open(&std::path::Path::new(dir).join("gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let agent = |name: &str, role: &str, chat: bool, extra: &str| {
        let instructions = format!("{}{extra}", seed::role_template(role));
        team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), model: Some("opus".into()),
            effort: Some("high".into()), instructions_md: Some(instructions), chat_enabled: Some(chat), ..Default::default() }).unwrap()
    };
    let lead = agent("Team Lead", "lead", true, "");
    agent("Frontend Agent", "frontend", false, "");
    let backend = agent("Backend Agent", "backend", false, "\n\nFAKE_HANG");
    agent("Design Agent", "design", false, "");
    agent("QA Agent", "qa", false, "");
    settings::set(&db, "claude_bin", &fake.to_string()).unwrap();

    let kade = projects::list(&db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(&db, &s.you_id, &kade.id, ProjectInput { name: kade.name.clone(), key: kade.key.clone(), client_id: kade.client_id.clone(),
        status: Some(kade.status.clone()), goal_md: kade.goal_md.clone(), repo_path: Some(repo.to_string()), color: kade.color.clone(),
        default_branch: Some("main".into()), ..Default::default() }).unwrap();
    let card = |id: &str| tasks::list(&db, &Default::default()).unwrap().into_iter().find(|t| t.identifier == id).unwrap();
    tasks::update(&db, &s.you_id, &card("KADE-1").id, TaskPatch { assignee_id: Some(backend.clone()), ..Default::default() }).unwrap();

    // A short chat with the Team Lead; the card it adds is real, so its link works.
    let gfw = projects::list(&db).unwrap().into_iter().find(|p| p.key == "GFW").unwrap();
    let todo = tasks::list(&db, &Default::default()).unwrap().into_iter().find(|t| t.identifier == "GFW-3").unwrap().state_id;
    let new_card = tasks::create(&db, &lead, TaskInput { project_id: gfw.id.clone(), title: "Newsletter signup on the homepage".into(),
        state_id: Some(todo), ..Default::default() }).unwrap();
    let new_id = tasks::get(&db, &new_card).unwrap().identifier;
    let thread = chat::create_thread(&db, &s.you_id, &lead, "What needs my attention today?").unwrap();
    let say = |role: &str, author: &str, body: Option<&str>, tool: Option<(&str, serde_json::Value)>| {
        chat::add_message(&db, NewMessage { thread_id: thread.clone(), role: role.into(), author_id: Some(author.into()), body_md: body.map(Into::into),
            run_id: None, tool_name: tool.as_ref().map(|(n, _)| format!("mcp__gizai__{n}")), tool: tool.map(|(_, v)| v), meta: None }).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    say("user", &s.you_id, Some("What needs my attention today?"), None);
    say("tool", &lead, None, Some(("read_inbox", json!({"id": "t1", "input": {}, "isError": false, "result": json!({"ok": true, "count": 2}).to_string()}))));
    say("agent", &lead, Some("Two things:\n\n- **GFW-2** Product filters: price range slider is waiting for your review.\n- **KADE-2** Rate limit the public tracking endpoint is in Testing with the QA Agent.\n\nThe Backend Agent is working on KADE-1, the CSV export."), None);
    say("user", &s.you_id, Some("Add a card for Groene Fiets: newsletter signup on the homepage, for the Frontend Agent."), None);
    say("tool", &lead, None, Some(("create_task", json!({"id": "t2", "input": {"project": "GFW", "title": "Newsletter signup on the homepage"}, "isError": false,
        "result": json!({"ok": true, "done": "created", "link": {"id": new_card, "label": format!("{new_id} Newsletter signup on the homepage"), "page": "task"}}).to_string()}))));
    say("agent", &lead, Some(&format!("Added **{new_id}** in To do. Give it the frontend label and the Frontend Agent picks it up.")), None);
    println!("{thread}");
}
