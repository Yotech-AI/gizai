// The Team Lead's Gizai tools, called the way the MCP server calls them.
use gizai_core::model::*;
use gizai_core::{projects, tasks, team};
use gizai_lib::{AppState, tools};
use serde_json::{Value, json};

struct T {
    st: AppState,
    lead: String,
    _dir: tempfile::TempDir,
}

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

fn setup() -> T {
    let dir = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(dir.path());
    // the fake answers Claude Code's model list (opus, sonnet, haiku, fable, default)
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let team_id = team::list(&st.db).unwrap()[0].id.clone();
    let lead = team::add_agent(&st.db, &st.you_id, &team_id, AgentInput {
        name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    T { st, lead, _dir: dir }
}

impl T {
    async fn call(&self, name: &str, args: Value) -> Result<Value, String> {
        tools::call(&self.st, &self.lead, name, args).await
    }
    async fn ok(&self, name: &str, args: Value) -> Value {
        self.call(name, args).await.unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }
    fn project(&self, name: &str, key: &str) -> String {
        projects::create(&self.st.db, &self.st.you_id, ProjectInput { name: name.into(), key: key.into(), ..Default::default() }).unwrap()
    }
}

#[test]
fn the_catalog_has_unique_names_and_object_schemas() {
    let cat = tools::catalog();
    assert!(cat.len() >= 33, "{}", cat.len());
    let mut names: Vec<&str> = cat.iter().map(|t| t.name.as_str()).collect();
    names.sort();
    let n = names.len();
    names.dedup();
    assert_eq!(names.len(), n, "duplicate tool names");
    for t in &cat {
        assert_eq!(t.input_schema["type"], "object", "{}", t.name);
        assert!(t.input_schema["properties"].is_object(), "{}", t.name);
        assert!(!t.description.is_empty() && t.description.len() < 600, "{}", t.name);
        let reads = t.name.starts_with("get_") || t.name.starts_with("list_") || t.name.starts_with("read_");
        assert_eq!(t.read_only, reads, "{}", t.name);
    }
    for must in ["create_task", "read_inbox", "create_agent", "write_doc", "attach_file", "start_agent_run"] {
        assert!(cat.iter().any(|t| t.name == must), "missing {must}");
    }
}

#[tokio::test]
async fn unknown_tools_and_missing_arguments_say_so() {
    let t = setup();
    assert!(t.call("drop_database", json!({})).await.unwrap_err().contains("unknown tool"));
    assert!(t.call("create_client", json!({})).await.unwrap_err().contains("name"));
}

#[tokio::test]
async fn create_client_then_find_it_by_name() {
    let t = setup();
    let r = t.ok("create_client", json!({"name": "Kade Logistics", "city": "Rotterdam", "email": "info@kade.nl"})).await;
    assert_eq!(r["ok"], true);
    assert_eq!(r["link"]["page"], "client");
    let got = t.ok("get_client", json!({"client": "kade logistics"})).await;
    assert_eq!(got["client"]["city"], "Rotterdam");
    let list = t.ok("list_clients", json!({})).await;
    assert_eq!(list["clients"][0]["name"], "Kade Logistics");
}

#[tokio::test]
async fn update_client_changes_only_the_given_fields() {
    let t = setup();
    t.ok("create_client", json!({"name": "Kade Logistics", "city": "Rotterdam", "email": "info@kade.nl", "notes_md": "Pays late."})).await;
    t.ok("update_client", json!({"client": "Kade", "city": "Delft"})).await;
    let got = t.ok("get_client", json!({"client": "Kade Logistics"})).await;
    assert_eq!(got["client"]["city"], "Delft");
    assert_eq!(got["client"]["email"], "info@kade.nl");
    assert_eq!(got["client"]["notes_md"], "Pays late.");
}

#[tokio::test]
async fn contacts_are_added_and_updated_by_name() {
    let t = setup();
    t.ok("create_client", json!({"name": "Kade Logistics"})).await;
    t.ok("save_contact", json!({"client": "Kade Logistics", "name": "Anna de Vries", "email": "anna@kade.nl", "is_primary": true})).await;
    t.ok("save_contact", json!({"client": "Kade Logistics", "contact": "Anna de Vries", "name": "Anna de Vries", "role": "CTO"})).await;
    let got = t.ok("get_client", json!({"client": "Kade Logistics"})).await;
    let contacts = got["contacts"].as_array().unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["role"], "CTO");
    assert_eq!(contacts[0]["email"], "anna@kade.nl");
}

#[tokio::test]
async fn create_project_suggests_a_key_and_links_the_client_by_name() {
    let t = setup();
    t.ok("create_client", json!({"name": "Kade Logistics"})).await;
    let r = t.ok("create_project", json!({"name": "Kade portal", "client": "Kade Logistics", "goal_md": "Self-service for drivers."})).await;
    assert_eq!(r["project"]["key"], "KP");
    let p = t.ok("get_project", json!({"project": "KP"})).await;
    assert_eq!(p["project"]["client"], "Kade Logistics");
    assert_eq!(p["project"]["goal_md"], "Self-service for drivers.");
    // a second project with the same initials gets another key
    let r2 = t.ok("create_project", json!({"name": "Kade planner"})).await;
    assert_ne!(r2["project"]["key"], "KP");
}

#[tokio::test]
async fn update_project_keeps_repo_and_colour() {
    let t = setup();
    let id = t.project("Kade portal", "KADE");
    let p = projects::get(&t.st.db, &id).unwrap();
    projects::update(&t.st.db, &t.st.you_id, &id, ProjectInput { name: p.name, key: p.key, repo_path: Some("/srv/kade".into()),
        color: Some("#7b9bff".into()), goal_md: Some("Goal".into()), ..Default::default() }).unwrap();
    t.ok("update_project", json!({"project": "KADE", "status": "paused"})).await;
    let p = projects::get(&t.st.db, &id).unwrap();
    assert_eq!(p.status, "paused");
    assert_eq!(p.repo_path.as_deref(), Some("/srv/kade"));
    assert_eq!(p.color.as_deref(), Some("#7b9bff"));
    assert_eq!(p.goal_md.as_deref(), Some("Goal"));
}

#[tokio::test]
async fn create_task_by_project_key_with_column_labels_and_assignee_names() {
    let t = setup();
    t.project("Kade portal", "KADE");
    let r = t.ok("create_task", json!({"project": "kade", "title": "Export invoices as CSV", "description_md": "As CSV.",
        "acceptance_md": "- [ ] a CSV downloads", "column": "to do", "labels": ["backend", "bug"], "assignee": "me", "priority": 2})).await;
    assert_eq!(r["task"]["identifier"], "KADE-1");
    assert_eq!(r["link"]["page"], "task");
    assert!(r["link"]["label"].as_str().unwrap().starts_with("KADE-1"));
    let task = tasks::get(&t.st.db, r["link"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(task.state_name, "To do");
    assert_eq!(task.assignee_id.as_deref(), Some(t.st.you_id.as_str()));
    assert_eq!(task.priority, 2);
    let mut labels: Vec<String> = task.labels.iter().map(|l| l.name.clone()).collect();
    labels.sort();
    assert_eq!(labels, ["backend", "bug"]);
    // assign to an agent by name
    let r = t.ok("create_task", json!({"project": "Kade portal", "title": "Second", "assignee": "team lead"})).await;
    let task = tasks::get(&t.st.db, r["link"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(task.assignee_id.as_deref(), Some(t.lead.as_str()));
}

#[tokio::test]
async fn get_task_by_identifier_includes_comments() {
    let t = setup();
    t.project("Kade portal", "KADE");
    t.ok("create_task", json!({"project": "KADE", "title": "Export", "description_md": "Long text"})).await;
    t.ok("comment_on_task", json!({"task": "kade-1", "body_md": "Started on this."})).await;
    let got = t.ok("get_task", json!({"task": "KADE-1"})).await;
    assert_eq!(got["task"]["description_md"], "Long text");
    assert_eq!(got["comments"][0]["body_md"], "Started on this.");
    assert_eq!(got["comments"][0]["author"], "Team Lead");
}

#[tokio::test]
async fn move_task_by_column_name() {
    let t = setup();
    t.project("Kade portal", "KADE");
    t.ok("create_task", json!({"project": "KADE", "title": "Export"})).await;
    let r = t.ok("move_task", json!({"task": "KADE-1", "column": "In progress"})).await;
    assert_eq!(r["task"]["column"], "In progress");
    let err = t.call("move_task", json!({"task": "KADE-1", "column": "Somewhere"})).await.unwrap_err();
    assert!(err.contains("Backlog") && err.contains("Done"), "{err}");
}

#[tokio::test]
async fn update_task_sets_labels_and_clears_a_hold() {
    let t = setup();
    t.project("Kade portal", "KADE");
    t.ok("create_task", json!({"project": "KADE", "title": "Export", "labels": ["bug"]})).await;
    t.ok("update_task", json!({"task": "KADE-1", "hold": "needs_decision", "hold_reason": "Which format?"})).await;
    let inbox = t.ok("read_inbox", json!({})).await;
    assert_eq!(inbox["count"], 1);
    assert_eq!(inbox["items"][0]["reason"], "Which format?");
    t.ok("update_task", json!({"task": "KADE-1", "clear_hold": true, "labels": ["frontend"], "title": "Export as CSV", "assignee": "none"})).await;
    let task = tasks::list(&t.st.db, &TaskFilter::default()).unwrap().remove(0);
    assert!(task.hold.is_none());
    assert_eq!(task.title, "Export as CSV");
    assert_eq!(task.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["frontend"]);
    assert_eq!(t.ok("read_inbox", json!({})).await["count"], 0);
}

#[tokio::test]
async fn ambiguous_names_list_the_candidates() {
    let t = setup();
    t.project("Kade portal", "KADE");
    t.project("Fiets portal", "GFP");
    t.ok("create_client", json!({"name": "Portal Partners"})).await;
    let err = t.call("get_project", json!({"project": "portal"})).await.unwrap_err();
    assert!(err.contains("Kade portal") && err.contains("Fiets portal"), "{err}");
    assert!(err.contains("KADE") && err.contains("GFP"), "{err}");
    // an exact name wins over a partial one
    t.project("Portal", "PRT");
    assert_eq!(t.ok("get_project", json!({"project": "portal"})).await["project"]["key"], "PRT");
}

#[tokio::test]
async fn unknown_names_say_what_exists() {
    let t = setup();
    t.project("Kade portal", "KADE");
    let err = t.call("create_task", json!({"project": "Webshop", "title": "x"})).await.unwrap_err();
    assert!(err.contains("Webshop") && err.contains("Kade portal"), "{err}");
    let err = t.call("get_task", json!({"task": "KADE-99"})).await.unwrap_err();
    assert!(err.contains("KADE-99"), "{err}");
    let err = t.call("create_task", json!({"project": "KADE", "title": "x", "labels": ["urgent"]})).await.unwrap_err();
    assert!(err.contains("urgent") && err.contains("backend"), "{err}");
}

#[tokio::test]
async fn create_and_update_an_agent_and_pause_it() {
    let t = setup();
    let r = t.ok("create_agent", json!({"name": "Frontend Agent", "role": "frontend", "wakeup": "heartbeat", "heartbeat_minutes": 30, "monthly_budget_usd": 25})).await;
    assert_eq!(r["link"]["page"], "agent");
    t.ok("update_agent", json!({"agent": "frontend agent", "model": "sonnet"})).await;
    let a = t.ok("get_agent", json!({"agent": "Frontend Agent"})).await;
    assert_eq!(a["agent"]["model"], "sonnet");
    assert_eq!(a["agent"]["wakeup"], "heartbeat");
    assert_eq!(a["agent"]["heartbeat_minutes"], 30);
    assert_eq!(a["agent"]["monthly_budget_usd"], 25.0);
    assert!(a["agent"]["instructions_md"].as_str().unwrap().contains("Frontend Agent"));
    t.ok("set_agent_status", json!({"agent": "Frontend Agent", "status": "paused"})).await;
    let list = t.ok("list_agents", json!({})).await;
    let fe = list["agents"].as_array().unwrap().iter().find(|a| a["name"] == "Frontend Agent").unwrap().clone();
    assert_eq!(fe["status"], "paused");
    // the Team Lead can't hand the Chat page to another agent
    let m = team::agent(&t.st.db, r["link"]["id"].as_str().unwrap()).unwrap();
    assert!(!m.chat_enabled);
    assert!(team::chat_agent(&t.st.db).unwrap().unwrap().actor_id == t.lead);
}

#[tokio::test]
async fn routing_rules_and_the_workflow_read_back() {
    let t = setup();
    t.ok("add_routing_rule", json!({"kind": "label", "match": "backend", "role": "backend", "priority": 10})).await;
    let w = t.ok("get_workflow", json!({})).await;
    assert_eq!(w["columns"].as_array().unwrap().len(), 6);
    assert!(w["rules"][0].as_str().unwrap().contains("backend"));
}

#[tokio::test]
async fn docs_can_be_created_written_and_read() {
    let t = setup();
    t.project("Kade portal", "KADE");
    let r = t.ok("create_doc", json!({"project": "KADE", "title": "Requirements", "body_md": "# Requirements\n\nDrivers sign in."})).await;
    assert_eq!(r["link"]["page"], "doc");
    let d = t.ok("read_doc", json!({"doc": "Requirements"})).await;
    assert_eq!(d["doc"]["version"], 2);
    assert!(d["doc"]["body_md"].as_str().unwrap().contains("Drivers sign in."));
    t.ok("write_doc", json!({"doc": "Requirements", "project": "KADE", "body_md": "# Requirements\n\nVersion three."})).await;
    let d = t.ok("read_doc", json!({"doc": "requirements"})).await;
    assert_eq!(d["doc"]["version"], 3);
    assert_eq!(t.ok("list_docs", json!({"project": "KADE"})).await["docs"][0]["title"], "Requirements");
}

#[tokio::test]
async fn attach_file_takes_only_files_named_in_the_chat_or_inside_a_linked_repo() {
    let t = setup();
    t.project("Kade portal", "KADE");
    t.ok("create_task", json!({"project": "KADE", "title": "Export"})).await;
    let f = t._dir.path().join("brief.txt");
    std::fs::write(&f, "the brief").unwrap();
    let secret = t._dir.path().join("id_ed25519");
    std::fs::write(&secret, "key").unwrap();
    let thread = gizai_core::chat::create_thread(&t.st.db, &t.st.you_id, &t.lead, "Attach my brief").unwrap();
    gizai_core::chat::add_message(&t.st.db, gizai_core::chat::NewMessage { thread_id: thread.clone(), role: "user".into(),
        author_id: Some(t.st.you_id.clone()), body_md: Some(format!("Attach {} to KADE-1", f.display())), ..Default::default() }).unwrap();
    let r = tools::call_in(&t.st, &t.lead, Some(&thread), "attach_file", json!({"path": f.display().to_string(), "task": "KADE-1"})).await.unwrap();
    assert_eq!(r["file"]["name"], "brief.txt");
    let got = t.ok("get_task", json!({"task": "KADE-1"})).await;
    assert_eq!(got["files"][0]["name"], "brief.txt");
    // a file nobody named in this chat
    let e = tools::call_in(&t.st, &t.lead, Some(&thread), "attach_file", json!({"path": secret.display().to_string(), "task": "KADE-1"})).await.unwrap_err();
    assert!(e.contains("named in this chat"), "{e}");
    // without a chat, only files inside a linked repository
    assert!(t.call("attach_file", json!({"path": f.display().to_string(), "task": "KADE-1"})).await.is_err());
    let repo = t._dir.path().join("kade");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(repo.join("docs/spec.md"), "# Spec").unwrap();
    t.ok("update_project", json!({"project": "KADE", "repo_path": repo.display().to_string()})).await;
    t.ok("attach_file", json!({"path": repo.join("docs/spec.md").display().to_string(), "task": "KADE-1"})).await;
    // the checks before any of that
    assert!(tools::call_in(&t.st, &t.lead, Some(&thread), "attach_file", json!({"path": "/no/such/file.pdf", "task": "KADE-1"})).await.is_err());
    assert!(tools::call_in(&t.st, &t.lead, Some(&thread), "attach_file", json!({"path": f.display().to_string()})).await.unwrap_err().contains("task, project or client"));
}

#[tokio::test]
async fn writes_are_attributed_to_the_team_lead() {
    let t = setup();
    t.project("Kade portal", "KADE");
    let r = t.ok("create_task", json!({"project": "KADE", "title": "Export"})).await;
    let act = tasks::activity(&t.st.db, r["link"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(act[0].actor_name.as_deref(), Some("Team Lead"));
}

#[tokio::test]
async fn list_tasks_filters_by_column_and_text() {
    let t = setup();
    t.project("Kade portal", "KADE");
    t.ok("create_task", json!({"project": "KADE", "title": "Export invoices", "column": "To do"})).await;
    t.ok("create_task", json!({"project": "KADE", "title": "Import drivers", "column": "Backlog"})).await;
    t.ok("create_task", json!({"project": "KADE", "title": "Old export", "column": "Done"})).await;
    let r = t.ok("list_tasks", json!({"column": "To do"})).await;
    assert_eq!(r["count"], 1);
    let r = t.ok("list_tasks", json!({"text": "export"})).await;
    assert_eq!(r["count"], 1, "done tasks are left out unless asked");
    let r = t.ok("list_tasks", json!({"text": "export", "include_done": true})).await;
    assert_eq!(r["count"], 2);
    assert_eq!(t.ok("get_overview", json!({})).await["tasks_by_column"]["Backlog"], 1);
}

#[tokio::test]
async fn people_can_be_listed_and_added() {
    let t = setup();
    t.ok("add_person", json!({"name": "Sanne Bakker", "email": "sanne@example.com"})).await;
    let p = t.ok("list_people", json!({})).await;
    assert!(p["people"].as_array().unwrap().iter().any(|x| x["name"] == "Sanne Bakker"));
    assert!(p["people"].as_array().unwrap().iter().any(|x| x["you"] == true));
}

#[tokio::test]
async fn the_chat_cannot_make_an_agent_that_skips_permissions() {
    let t = setup();
    let e = t.call("create_agent", json!({"name": "Root Agent", "role": "backend", "permission_mode": "bypassPermissions"})).await.unwrap_err();
    assert!(e.contains("bypassPermissions") && e.contains("agent form"), "{e}");
    for tools in [json!(["Bash"]), json!(["Bash(*)"]), json!(["Bash(:*)"]), json!(["Read", " bash "])] {
        let e = t.call("create_agent", json!({"name": "Shell Agent", "role": "backend", "allowed_tools": tools})).await.unwrap_err();
        assert!(e.contains("any command"), "{e}");
    }
    t.ok("create_agent", json!({"name": "Backend Agent", "role": "backend", "allowed_tools": ["Bash(npm test:*)", "Bash(git commit:*)"]})).await;
    let e = t.call("update_agent", json!({"agent": "Backend Agent", "permission_mode": "bypassPermissions"})).await.unwrap_err();
    assert!(e.contains("bypassPermissions"), "{e}");
    assert!(t.call("update_agent", json!({"agent": "Backend Agent", "allowed_tools": ["Bash"]})).await.is_err());
    assert_eq!(team::agent(&t.st.db, &resolve_agent(&t, "Backend Agent")).unwrap().permission_mode.as_deref(), Some("acceptEdits"));
}

fn resolve_agent(t: &T, name: &str) -> String {
    team::all_agents(&t.st.db).unwrap().into_iter().find(|(_, m)| m.name == name).unwrap().1.actor_id
}

#[tokio::test]
async fn repo_paths_from_the_chat_must_be_git_repositories() {
    let t = setup();
    let home = std::env::var("HOME").unwrap();
    for bad in ["/".to_string(), home.clone(), t._dir.path().display().to_string()] {
        let e = t.call("create_project", json!({"name": "Bad", "repo_path": bad})).await.unwrap_err();
        assert!(e.contains("git repository"), "{bad}: {e}");
    }
    let repo = t._dir.path().join("kade");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    t.ok("create_project", json!({"name": "Kade portal", "key": "KADE", "repo_path": repo.display().to_string()})).await;
    let e = t.call("update_project", json!({"project": "KADE", "repo_path": "/"})).await.unwrap_err();
    assert!(e.contains("git repository"), "{e}");
    t.ok("update_project", json!({"project": "KADE", "repo_path": ""})).await; // clearing is fine
}

#[tokio::test]
async fn agents_get_only_models_and_efforts_claude_code_offers() {
    let t = setup();
    let e = t.call("create_agent", json!({"name": "Frontend Agent", "role": "frontend", "model": "opus 5.5"})).await.unwrap_err();
    assert!(e.contains("opus 5.5") && e.contains("opus") && e.contains("sonnet"), "{e}");
    t.ok("create_agent", json!({"name": "Frontend Agent", "role": "frontend", "model": "opus", "effort": "xhigh"})).await;
    t.ok("update_agent", json!({"agent": "Frontend Agent", "model": "claude-sonnet-5-5"})).await;
    let e = t.call("update_agent", json!({"agent": "Frontend Agent", "model": "haiku", "effort": "max"})).await.unwrap_err();
    assert!(e.contains("Haiku") && e.contains("effort"), "{e}");
    let a = t.ok("get_agent", json!({"agent": "Frontend Agent"})).await;
    assert_eq!(a["agent"]["model"], "claude-sonnet-5-5");
    assert_eq!(a["agent"]["effort"], "xhigh", "an update keeps the effort it didn't name");
}

/// Adds coding CLIs in Settings and returns their ids by name (GA-3).
fn add_clis(t: &T, list: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    let clis: Vec<gizai_core::clis::Cli> = list.iter().map(|(name, kind)| gizai_core::clis::Cli { name: name.to_string(), kind: kind.to_string(),
        command: format!("/usr/bin/{kind}"), ..Default::default() }).collect();
    gizai_core::clis::save(&t.st.db, clis).unwrap().into_iter().map(|c| (c.name, c.id)).collect()
}

#[tokio::test]
async fn the_team_lead_can_put_agents_on_codex_or_gemini_by_name() {
    let t = setup();
    let ids = add_clis(&t, &[("Codex", "codex"), ("Gemini", "gemini"), ("Crush", "other")]);
    // Codex takes its own model names and efforts; Claude Code's list isn't asked
    t.ok("create_agent", json!({"name": "Codex Agent", "role": "backend", "runs_on": "codex", "model": "gpt-5-codex", "effort": "minimal"})).await;
    let m = team::agent(&t.st.db, &resolve_agent(&t, "Codex Agent")).unwrap();
    assert_eq!((m.adapter.as_deref(), m.model.as_deref(), m.effort.as_deref(), m.permission_mode.as_deref()),
               (Some(ids["Codex"].as_str()), Some("gpt-5-codex"), Some("minimal"), Some("workspace-write")));
    // by id works too
    t.ok("create_agent", json!({"name": "Gemini Agent", "role": "qa", "runs_on": ids["Gemini"], "permission_mode": "plan"})).await;
    assert_eq!(team::agent(&t.st.db, &resolve_agent(&t, "Gemini Agent")).unwrap().permission_mode.as_deref(), Some("plan"));
    let e = t.call("create_agent", json!({"name": "X", "role": "qa", "runs_on": "Cursor"})).await.unwrap_err();
    assert!(e.contains("no coding CLI called \"Cursor\"") && e.contains("Claude Code, Codex, Gemini, Crush"), "{e}");
    let e = t.call("create_agent", json!({"name": "X", "role": "qa", "runs_on": "Crush"})).await.unwrap_err();
    assert!(e.contains("Crush runs with its own permissions"), "{e}");
    let e = t.call("create_agent", json!({"name": "X", "role": "qa", "runs_on": "Gemini", "effort": "high"})).await.unwrap_err();
    assert!(e.contains("Gemini takes no effort level"), "{e}");
    let e = t.call("create_agent", json!({"name": "X", "role": "qa", "runs_on": "Codex", "permission_mode": "acceptEdits"})).await.unwrap_err();
    assert!(e.contains("unknown permission mode acceptEdits for Codex"), "{e}");
}

#[tokio::test]
async fn the_chat_cannot_unlock_codex_or_gemini_either() {
    let t = setup();
    add_clis(&t, &[("Codex", "codex"), ("Gemini", "gemini")]);
    for (cli, mode) in [("Codex", "danger-full-access"), ("Gemini", "yolo")] {
        let e = t.call("create_agent", json!({"name": "Root Agent", "role": "backend", "runs_on": cli, "permission_mode": mode})).await.unwrap_err();
        assert!(e.contains(mode) && e.contains("agent form"), "{e}");
    }
    t.ok("create_agent", json!({"name": "Codex Agent", "role": "backend", "runs_on": "Codex"})).await;
    let e = t.call("update_agent", json!({"agent": "Codex Agent", "permission_mode": "danger-full-access"})).await.unwrap_err();
    assert!(e.contains("danger-full-access"), "{e}");
    assert_eq!(team::agent(&t.st.db, &resolve_agent(&t, "Codex Agent")).unwrap().permission_mode.as_deref(), Some("workspace-write"));
}

#[tokio::test]
async fn moving_an_agent_to_another_kind_of_cli_starts_from_that_clis_defaults() {
    let t = setup();
    let ids = add_clis(&t, &[("Codex", "codex"), ("Claude 2", "claude_code")]);
    t.ok("create_agent", json!({"name": "Backend Agent", "role": "backend", "model": "opus", "effort": "max", "permission_mode": "dontAsk"})).await;
    let id = resolve_agent(&t, "Backend Agent");
    // another Claude Code account keeps model, effort and mode
    t.ok("update_agent", json!({"agent": "Backend Agent", "runs_on": "Claude 2"})).await;
    let m = team::agent(&t.st.db, &id).unwrap();
    assert_eq!((m.adapter.as_deref(), m.model.as_deref(), m.effort.as_deref(), m.permission_mode.as_deref()),
               (Some(ids["Claude 2"].as_str()), Some("opus"), Some("max"), Some("dontAsk")));
    // Codex: Claude Code's model, effort and mode don't carry over
    t.ok("update_agent", json!({"agent": "Backend Agent", "runs_on": "Codex"})).await;
    let m = team::agent(&t.st.db, &id).unwrap();
    assert_eq!((m.adapter.as_deref(), m.model.as_deref(), m.effort.as_deref(), m.permission_mode.as_deref()),
               (Some(ids["Codex"].as_str()), None, None, Some("workspace-write")));
    // other changes keep it on Codex
    t.ok("update_agent", json!({"agent": "Backend Agent", "effort": "high", "title": "Backend"})).await;
    let m = team::agent(&t.st.db, &id).unwrap();
    assert_eq!((m.adapter.as_deref(), m.effort.as_deref()), (Some(ids["Codex"].as_str()), Some("high")));
    // and back to Claude Code: its model list is checked again
    let e = t.call("update_agent", json!({"agent": "Backend Agent", "runs_on": "Claude Code", "model": "opus 5.5"})).await.unwrap_err();
    assert!(e.contains("opus 5.5"), "{e}");
    t.ok("update_agent", json!({"agent": "Backend Agent", "runs_on": "Claude Code"})).await;
    let m = team::agent(&t.st.db, &id).unwrap();
    assert_eq!((m.adapter.as_deref(), m.permission_mode.as_deref(), m.effort.as_deref()), (Some("claude_code"), Some("acceptEdits"), None));
}

#[test]
fn the_agent_tools_offer_runs_on() {
    let cat = tools::catalog();
    for name in ["create_agent", "update_agent"] {
        let t = cat.iter().find(|t| t.name == name).unwrap();
        assert!(t.input_schema["properties"]["runs_on"].is_object(), "{name}");
    }
}
