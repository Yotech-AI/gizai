// GA-63: a new install's first start makes five agents ready to work (on Codex when only Codex is installed), with
// their role's instructions and allowed commands; an install seeded before, and a new team, get none.
use gizai_core::clis::{self, Cli};
use gizai_core::db::Db;
use gizai_core::model::AgentInput;
use gizai_core::seed::{ensure_seed, ensure_seed_with_agents, role_template, role_tools, DEFAULT_COLUMNS, DEFAULT_TOOLS};
use gizai_core::team::{self, Member, Team};

/// What builders get on top of Gizai's default list (the card's list).
const BUILDER_EXTRA: [&str; 11] = [
    "Bash(git show:*)", "Bash(git ls-files:*)", "Bash(git rev-parse:*)", "Bash(git branch --show-current:*)", "Bash(git remote -v:*)",
    "Bash(git push:*)", "Bash(git pull:*)", "Bash(git fetch:*)", "Bash(node:*)", "Bash(echo:*)", "Bash(printf:*)",
];
/// What QA gets on top of the builders' list.
const QA_EXTRA: [&str; 4] = ["Bash(gh pr create:*)", "Bash(gh pr list:*)", "Bash(gh pr view:*)", "Bash(gh pr edit:*)"];
/// The DevOps Agent's own list, as the card has it.
const DEVOPS: [&str; 59] = [
    "Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git show:*)", "Bash(git branch -r:*)", "Bash(git branch -a:*)",
    "Bash(git branch --show-current:*)", "Bash(git branch --contains:*)", "Bash(git tag:*)", "Bash(git describe:*)", "Bash(git rev-parse:*)",
    "Bash(git rev-list:*)", "Bash(git merge-base:*)", "Bash(git merge-tree:*)", "Bash(git ls-remote:*)", "Bash(git ls-files:*)", "Bash(git fetch:*)",
    "Bash(git pull:*)", "Bash(git clone:*)", "Bash(git remote -v:*)", "Bash(git remote get-url:*)", "Bash(git add:*)", "Bash(git commit:*)",
    "Bash(git merge:*)", "Bash(git checkout --ours:*)", "Bash(git checkout --theirs:*)", "Bash(git push:*)", "Bash(gh auth status:*)",
    "Bash(gh repo view:*)", "Bash(gh repo clone:*)", "Bash(gh pr create:*)", "Bash(gh pr list:*)", "Bash(gh pr view:*)", "Bash(gh pr checks:*)",
    "Bash(gh pr diff:*)", "Bash(gh pr merge:*)", "Bash(gh pr edit:*)", "Bash(gh pr update-branch:*)", "Bash(gh release create:*)",
    "Bash(gh release list:*)", "Bash(gh release view:*)", "Bash(gh run list:*)", "Bash(gh run view:*)", "Bash(gh workflow list:*)",
    "Bash(gh workflow view:*)", "Bash(gh variable list:*)", "Bash(gh variable get:*)", "Bash(gh secret list:*)",
    "Bash(npm install --package-lock-only:*)", "Bash(cargo update --workspace:*)", "Bash(cargo check:*)", "Bash(npm ci:*)", "Bash(npx tsc -b:*)",
    "Bash(ls:*)", "Bash(cat:*)", "Bash(rg:*)", "Bash(jq:*)", "Bash(sleep:*)", "Bash(date:*)",
];

/// The five agents, by name, with where each should be: (name, role, columns).
const FIVE: [(&str, &str, &[&str]); 5] = [
    ("Team Lead", "lead", &[]),
    ("Backend Agent", "backend", &["To do", "In progress"]),
    ("Frontend Agent", "frontend", &["To do", "In progress"]),
    ("QA Agent", "qa", &["Testing"]),
    ("DevOps Agent", "devops", &["Deploy"]),
];

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn agents(t: &Team) -> Vec<&Member> {
    t.members.iter().filter(|m| m.kind == "agent").collect()
}

fn agent<'a>(t: &'a Team, name: &str) -> &'a Member {
    agents(t).into_iter().find(|m| m.name == name).unwrap_or_else(|| panic!("no {name}"))
}

fn columns_of(t: &Team, id: &str) -> Vec<String> {
    t.states.iter().filter(|s| s.agent_ids.iter().any(|a| a == id)).map(|s| s.name.clone()).collect()
}

fn codex() -> Cli {
    Cli { name: "Codex".into(), kind: "codex".into(), command: "/opt/codex/bin/codex".into(), ..Default::default() }
}

/// What every new agent has, whichever CLI it runs on.
fn assert_ready(t: &Team, name: &str, role: &str, cols: &[&str]) {
    let m = agent(t, name);
    assert_eq!(m.role_key, role, "{name}");
    assert_eq!(m.instructions_md.as_deref(), Some(role_template(role).as_str()), "{name}'s instructions are its role's");
    assert_eq!(m.allowed_tools, role_tools(role), "{name}'s allowed commands are its role's");
    assert_eq!((m.model.as_deref(), m.effort.as_deref()), (None, None), "{name}: its CLI's own model and effort");
    assert_eq!(m.max_runs, 1, "{name}");
    assert_eq!(m.budget_usd_micros, None, "{name}: no budget");
    assert_eq!(m.board_check_minutes, None, "{name}: no board check");
    assert!(m.tools.mcp.is_empty(), "{name}: no MCP servers");
    assert!(m.folders.is_empty(), "{name}: no folders");
    assert_eq!(m.status, "active", "{name}");
    assert_eq!((m.chat_enabled, m.is_lead), (role == "lead", role == "lead"), "{name}: only the Team Lead has Chat");
    assert_eq!(columns_of(t, &m.actor_id), strs(cols), "{name}'s columns");
}

#[test]
fn a_new_install_starts_with_the_workflow_and_five_agents_on_claude_code() {
    let db = Db::open_in_memory().unwrap();
    let mut asked = 0;
    let s = ensure_seed_with_agents(&db, "Sanne", || { asked += 1; None }).unwrap();
    assert_eq!(asked, 1, "Codex is looked for once, on the first start");
    let t = team::get(&db, &s.team_id).unwrap();
    assert_eq!(t.states.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), DEFAULT_COLUMNS.map(|c| c.0).to_vec());
    assert_eq!(t.labels.len(), 4);
    let mut names: Vec<&str> = agents(&t).iter().map(|m| m.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Backend Agent", "DevOps Agent", "Frontend Agent", "QA Agent", "Team Lead"]);
    for (name, role, cols) in FIVE {
        assert_ready(&t, name, role, cols);
        let m = agent(&t, name);
        assert_eq!(m.adapter.as_deref(), Some(clis::CLAUDE_CODE), "{name} runs on Claude Code");
        assert_eq!(m.permission_mode.as_deref(), Some("acceptEdits"), "{name}");
    }
    assert_eq!(team::chat_agent(&db).unwrap().unwrap().name, "Team Lead");
    assert_eq!(clis::list(&db).unwrap().len(), 1, "no coding CLI added: only the built-in Claude Code");
    // You are still the only person, and the reviewer.
    assert_eq!(t.members.iter().filter(|m| m.kind == "person").count(), 1);
}

#[test]
fn with_only_codex_installed_codex_is_added_and_runs_every_agent_but_the_team_lead() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed_with_agents(&db, "Sanne", || Some(codex())).unwrap();
    let list = clis::list(&db).unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    let cx = &list[1];
    assert_eq!((cx.name.as_str(), cx.kind.as_str(), cx.command.as_str()), ("Codex", "codex", "/opt/codex/bin/codex"));
    assert!(!cx.id.is_empty() && cx.id != clis::CLAUDE_CODE, "saved with an id of its own: {cx:?}");
    let t = team::get(&db, &s.team_id).unwrap();
    for (name, role, cols) in FIVE {
        assert_ready(&t, name, role, cols);
        let m = agent(&t, name);
        if role == "lead" {
            // Chat runs only on Claude Code.
            assert_eq!((m.adapter.as_deref(), m.permission_mode.as_deref()), (Some(clis::CLAUDE_CODE), Some("acceptEdits")));
        } else {
            // Codex has no acceptEdits: its own sandbox, workspace-write.
            assert_eq!((m.adapter.as_deref(), m.permission_mode.as_deref()), (Some(cx.id.as_str()), Some("workspace-write")), "{name}");
        }
    }
    assert_eq!(team::chat_agent(&db).unwrap().unwrap().name, "Team Lead");
}

#[test]
fn an_install_seeded_before_gets_nothing_new_and_codex_isnt_looked_for() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Sanne").unwrap();
    // An agent made before GA-63, with instructions and a list of its own.
    let old = team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: "Backend Agent".into(), role_key: "backend".into(), instructions_md: Some("Our own text.\nGIZAI_RESULT: …".into()),
        allowed_tools: strs(&["Bash(npm test:*)"]), ..Default::default() }).unwrap();
    let again = ensure_seed_with_agents(&db, "Someone else", || panic!("a seeded install never looks for Codex")).unwrap();
    assert_eq!((&again.org_id, &again.you_id, &again.team_id), (&s.org_id, &s.you_id, &s.team_id));
    let t = team::get(&db, &s.team_id).unwrap();
    assert_eq!(agents(&t).len(), 1, "no agents added");
    let m = agent(&t, "Backend Agent");
    assert_eq!(m.actor_id, old);
    assert_eq!(m.instructions_md.as_deref(), Some("Our own text.\nGIZAI_RESULT: …"), "its instructions are kept");
    assert_eq!(m.allowed_tools, strs(&["Bash(npm test:*)"]), "its list is kept");
    assert_eq!(clis::list(&db).unwrap().len(), 1);
    assert_eq!(team::list(&db).unwrap().len(), 1);
}

#[test]
fn a_second_start_adds_nothing_and_keeps_the_agents_as_they_are() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed_with_agents(&db, "Sanne", || None).unwrap();
    let t = team::get(&db, &s.team_id).unwrap();
    let qa = agent(&t, "QA Agent").clone();
    team::update_agent(&db, &s.you_id, &qa.actor_id, AgentInput {
        name: qa.name.clone(), role_key: qa.role_key.clone(), instructions_md: Some("Changed.".into()), allowed_tools: strs(&["Bash(make:*)"]),
        ..Default::default() }).unwrap();
    let again = ensure_seed_with_agents(&db, "Sanne", || panic!("not asked again")).unwrap();
    assert_eq!(again.team_id, s.team_id);
    let t = team::get(&db, &s.team_id).unwrap();
    assert_eq!(agents(&t).len(), 5);
    let qa = agent(&t, "QA Agent");
    assert_eq!((qa.instructions_md.as_deref(), qa.allowed_tools.clone()), (Some("Changed."), strs(&["Bash(make:*)"])));
    // Plain ensure_seed on a new install's data changes nothing either.
    ensure_seed(&db, "Sanne").unwrap();
    assert_eq!(agents(&team::get(&db, &s.team_id).unwrap()).len(), 5);
}

#[test]
fn ensure_seed_and_a_new_team_make_the_columns_without_agents() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Sanne").unwrap();
    assert!(agents(&team::get(&db, &s.team_id).unwrap()).is_empty(), "tests and the demo data start without agents");

    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed_with_agents(&db, "Sanne", || None).unwrap();
    let other = team::add_team(&db, &s.you_id, "Mobile").unwrap();
    let t = team::get(&db, &other).unwrap();
    assert_eq!(t.states.len(), 7);
    assert!(agents(&t).is_empty(), "New team makes no agents");
    assert!(t.states.iter().all(|s| s.agent_ids.is_empty()));
    assert_eq!(agents(&team::get(&db, &s.team_id).unwrap()).len(), 5, "the first team keeps its five");
}

#[test]
fn each_role_starts_with_its_own_allowed_commands() {
    let builders: Vec<String> = DEFAULT_TOOLS.iter().chain(&BUILDER_EXTRA).map(|s| s.to_string()).collect();
    let qa: Vec<String> = builders.iter().cloned().chain(strs(&QA_EXTRA)).collect();
    assert_eq!(role_tools("lead"), strs(&DEFAULT_TOOLS), "the Team Lead: Gizai's default list");
    for role in ["backend", "frontend", "design", "docs", "mobile"] {
        assert_eq!(role_tools(role), builders, "{role}: Gizai's default list plus the builders'");
    }
    assert_eq!(role_tools("qa"), qa);
    assert_eq!(role_tools("devops"), strs(&DEVOPS));
    for role in ["lead", "backend", "qa", "devops"] {
        let list = role_tools(role);
        let mut unique = list.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), list.len(), "{role} has a command twice");
        assert!(list.iter().all(|t| t.starts_with("Bash(") && t.ends_with(')')), "{role}: {list:?}");
    }
    // Why: Gizai's default list can't push or open a pull request.
    assert!(!DEFAULT_TOOLS.iter().any(|t| t.contains("git push") || t.contains("gh ")));
    assert!(role_tools("backend").contains(&"Bash(git push:*)".to_string()) && !role_tools("backend").iter().any(|t| t.contains("gh ")));
    assert!(role_tools("qa").contains(&"Bash(gh pr create:*)".to_string()) && !role_tools("qa").contains(&"Bash(gh pr merge:*)".to_string()));
}
