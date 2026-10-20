// GA-162: the first start's onboarding. A new install's seed marks it as waiting; finishing it saves your name, its
// handle and the team's name, can move the Backend, Frontend, QA and DevOps Agents to another coding CLI (the Team Lead
// stays on Claude Code) and clears the flag, in one write; skipping only clears the flag. Starting the welcome chat is the
// app's (src-tauri/tests/it/onboarding_flow_test.rs).
use gizai_core::clis::{self, Cli};
use gizai_core::db::Db;
use gizai_core::onboarding::{self, OnboardingInput, WELCOME};
use gizai_core::seed::{SeedIds, ensure_seed, ensure_seed_with_agents};
use gizai_core::{settings, team, users};

/// A new install, as `open_state` seeds it: five agents on Claude Code, the onboarding waiting.
fn new_install() -> (Db, SeedIds) {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed_with_agents(&db, "jefsev", || None).unwrap();
    (db, s)
}

fn input(you: &str, team: &str, cli: Option<&str>) -> OnboardingInput {
    OnboardingInput { you_name: you.into(), team_name: team.into(), agents_cli: cli.map(str::to_string) }
}

fn cli(name: &str, kind: &str, command: &str) -> Cli {
    Cli { name: name.into(), kind: kind.into(), command: command.into(), ..Default::default() }
}

/// Codex, Gemini, an Other CLI and a second Claude Code account under Settings → Coding CLIs; returns the full list.
fn add_clis(db: &Db) -> Vec<Cli> {
    clis::save(db, vec![
        cli("Codex", "codex", "/opt/codex/bin/codex"),
        cli("Gemini", "gemini", "/opt/gemini/bin/gemini"),
        cli("OpenCode", "other", "/opt/opencode/bin/opencode"),
        Cli { env: vec!["CLAUDE_CONFIG_DIR=~/.claude-2".into()], ..cli("Claude Code (2nd account)", "claude_code", "/opt/claude/bin/claude") },
    ]).unwrap();
    clis::list(db).unwrap()
}

fn id_of(list: &[Cli], name: &str) -> String {
    list.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no {name}")).id.clone()
}

/// You, as (name, handle).
fn you(db: &Db, s: &SeedIds) -> (String, String) {
    let p = users::list(db).unwrap().into_iter().find(|p| p.id == s.you_id).expect("you");
    (p.name, p.handle)
}

fn team_name(db: &Db, s: &SeedIds) -> String {
    team::get(db, &s.team_id).unwrap().name
}

/// The team's agents as (name, adapter, permission mode, model, effort), by name.
fn agents(db: &Db, s: &SeedIds) -> Vec<(String, String, String, Option<String>, Option<String>)> {
    let mut out: Vec<_> = team::get(db, &s.team_id).unwrap().members.into_iter().filter(|m| m.kind == "agent")
        .map(|m| (m.name, m.adapter.unwrap_or_default(), m.permission_mode.unwrap_or_default(), m.model, m.effort)).collect();
    out.sort();
    out
}

/// The four builders on `cli` with `mode` and their CLI's own model and effort, the Team Lead on Claude Code.
fn four_on(cli: &str, mode: &str) -> Vec<(String, String, String, Option<String>, Option<String>)> {
    let mut v: Vec<_> = ["Backend Agent", "DevOps Agent", "Frontend Agent", "QA Agent"].iter()
        .map(|n| (n.to_string(), cli.to_string(), mode.to_string(), None, None)).collect();
    v.push(("Team Lead".into(), clis::CLAUDE_CODE.into(), "acceptEdits".into(), None, None));
    v.sort();
    v
}

/// Every row that finishing or skipping could touch, and the change log's length: equal before and after means
/// nothing changed.
fn snapshot(db: &Db) -> Vec<String> {
    db.read(|c| {
        let mut out = vec![];
        for sql in [
            "SELECT id || '|' || name || '|' || handle || '|' || version FROM actors ORDER BY id",
            "SELECT id || '|' || name || '|' || version FROM teams ORDER BY id",
            "SELECT actor_id || '|' || COALESCE(adapter, '') || '|' || COALESCE(permission_mode, '') || '|' || COALESCE(model, '')
               || '|' || COALESCE(effort, '') || '|' || version FROM agent_configs ORDER BY actor_id",
            "SELECT key || '|' || org_id || '|' || value_json FROM settings ORDER BY key, org_id",
            "SELECT 'changes ' || count(*) FROM changes",
        ] {
            let mut st = c.prepare(sql)?;
            out.extend(st.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?);
        }
        Ok(out)
    }).unwrap()
}

#[test]
fn a_new_install_waits_for_the_onboarding_in_the_settings_key_onboarding() {
    let (db, _) = new_install();
    assert!(onboarding::pending(&db).unwrap());
    assert_eq!(onboarding::KEY, "onboarding");
    let raw: String = db.read(|c| Ok(c.query_row("SELECT value_json FROM settings WHERE key='onboarding' AND org_id=''", [], |r| r.get(0))?)).unwrap();
    assert_eq!(raw, "\"pending\"");
    // Seeded again (the next start): it still waits, and nothing is added.
    ensure_seed_with_agents(&db, "jefsev", || panic!("a seeded install never looks for Codex")).unwrap();
    assert!(onboarding::pending(&db).unwrap());
}

#[test]
fn ensure_seed_and_a_database_seeded_before_never_wait() {
    // Tests and the demo data.
    let db = Db::open_in_memory().unwrap();
    ensure_seed(&db, "jefsev").unwrap();
    assert!(!onboarding::pending(&db).unwrap());
    // Started again as a new install's Gizai starts: seeded before, so it gets nothing new.
    ensure_seed_with_agents(&db, "jefsev", || panic!("a seeded install never looks for Codex")).unwrap();
    assert!(!onboarding::pending(&db).unwrap());
    assert_eq!(settings::get::<String>(&db, "onboarding").unwrap(), None);
    // And there is nothing to finish.
    let s = ensure_seed(&db, "jefsev").unwrap();
    let e = onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", None)).unwrap_err();
    assert_eq!(e.to_string(), "Nothing to set up");

    // A new install seeded before this card (GA-63: five agents, no flag), started again: it doesn't wait either.
    let db = Db::open_in_memory().unwrap();
    ensure_seed_with_agents(&db, "jefsev", || None).unwrap();
    db.write(None, |w| Ok(w.conn().execute("DELETE FROM settings WHERE key='onboarding'", [])?)).unwrap();
    ensure_seed_with_agents(&db, "jefsev", || panic!("a seeded install never looks for Codex")).unwrap();
    assert!(!onboarding::pending(&db).unwrap());
}

#[test]
fn the_welcome_message_is_the_cards_text_word_for_word() {
    assert_eq!(WELCOME, "Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else.");
}

#[test]
fn the_input_is_camel_case_over_ipc() {
    let i: OnboardingInput = serde_json::from_str(r#"{"youName":"Jeffrey","teamName":"YoTech","agentsCli":"abc"}"#).unwrap();
    assert_eq!(i, input("Jeffrey", "YoTech", Some("abc")));
    let i: OnboardingInput = serde_json::from_str(r#"{"youName":"Jeffrey","teamName":"YoTech","agentsCli":null}"#).unwrap();
    assert_eq!(i.agents_cli, None);
    let i: OnboardingInput = serde_json::from_str(r#"{"youName":"Jeffrey","teamName":"YoTech"}"#).unwrap();
    assert_eq!(i.agents_cli, None, "agentsCli may be left out");
}

#[test]
fn finishing_saves_your_name_its_handle_and_the_teams_name_trimmed_and_ends_the_onboarding() {
    let (db, s) = new_install();
    let before = agents(&db, &s);
    onboarding::finish(&db, &s.you_id, &input("  Jeffrey Sevinga ", "\tYoTech Software  ", None)).unwrap();
    assert_eq!(you(&db, &s), ("Jeffrey Sevinga".into(), "jeffrey-sevinga".into()));
    assert_eq!(team_name(&db, &s), "YoTech Software");
    assert!(!onboarding::pending(&db).unwrap());
    assert_eq!(settings::get::<String>(&db, "onboarding").unwrap(), None, "the flag is removed");
    assert_eq!(agents(&db, &s), before, "without a CLI, the agents stay where the seed put them");
    assert_eq!(team::list(&db).unwrap().len(), 1, "no team added");
    assert_eq!(users::list(&db).unwrap().len(), 1, "no person added");
    // It was finished: once is enough.
    let e = onboarding::finish(&db, &s.you_id, &input("Someone", "Else", None)).unwrap_err();
    assert_eq!(e.to_string(), "Nothing to set up");
    assert_eq!(you(&db, &s).0, "Jeffrey Sevinga");
}

#[test]
fn your_handle_is_kept_when_it_is_yours_and_numbered_when_an_agent_has_it() {
    let (db, s) = new_install();
    // The seed named you jefsev: the same name keeps the same handle, not jefsev-2.
    let snap = you(&db, &s);
    assert_eq!(snap, ("jefsev".into(), "jefsev".into()));
    onboarding::finish(&db, &s.you_id, &input("jefsev", "My team", None)).unwrap();
    assert_eq!(you(&db, &s), ("jefsev".into(), "jefsev".into()));

    // Named like an agent: team-lead is the Team Lead's, so you get team-lead-2 instead of a UNIQUE error.
    let (db, s) = new_install();
    let lead = team::chat_agent(&db).unwrap().unwrap();
    assert_eq!(lead.handle, "team-lead");
    onboarding::finish(&db, &s.you_id, &input("Team Lead", "My team", None)).unwrap();
    assert_eq!(you(&db, &s), ("Team Lead".into(), "team-lead-2".into()));
    assert_eq!(team::chat_agent(&db).unwrap().unwrap().handle, "team-lead", "the Team Lead keeps its handle");

    // A name without letters or digits gets the seed's fallback handle.
    let (db, s) = new_install();
    onboarding::finish(&db, &s.you_id, &input("✨", "My team", None)).unwrap();
    assert_eq!(you(&db, &s), ("✨".into(), "user".into()));
}

#[test]
fn an_empty_name_or_team_name_fails_with_the_cards_error_and_changes_nothing() {
    let (db, s) = new_install();
    let list = add_clis(&db);
    let codex = id_of(&list, "Codex");
    let before = snapshot(&db);
    for (i, want) in [
        (input("", "YoTech", Some(&codex)), "Your name can't be empty"),
        (input("  \t ", "YoTech", Some(&codex)), "Your name can't be empty"),
        (input("Jeffrey", "", Some(&codex)), "The team's name can't be empty"),
        (input("Jeffrey", "   ", Some(&codex)), "The team's name can't be empty"),
        (input("", "", None), "Your name can't be empty"),
        (input("Jeffrey", "YoTech", Some("no-such-cli")), "No coding CLI with the id no-such-cli"),
        (input("Jeffrey", "YoTech", Some("  no-such-cli ")), "No coding CLI with the id no-such-cli"),
    ] {
        let e = onboarding::finish(&db, &s.you_id, &i).unwrap_err();
        assert_eq!(e.to_string(), want, "{i:?}");
        assert_eq!(snapshot(&db), before, "{i:?} changed something");
        assert!(onboarding::pending(&db).unwrap(), "{i:?}: the onboarding still waits");
    }
    // Fixed, it goes through.
    onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", Some(&codex))).unwrap();
    assert_eq!(you(&db, &s).0, "Jeffrey");
}

#[test]
fn an_agents_cli_puts_backend_frontend_qa_and_devops_on_it_and_the_team_lead_stays_on_claude_code() {
    for (name, mode) in [("Codex", "workspace-write"), ("Gemini", "auto_edit"), ("OpenCode", ""), ("Claude Code (2nd account)", "acceptEdits")] {
        let (db, s) = new_install();
        let list = add_clis(&db);
        let id = id_of(&list, name);
        // A model and an effort of the old CLI's don't go along to the new one.
        let backend = team::get(&db, &s.team_id).unwrap().members.into_iter().find(|m| m.name == "Backend Agent").unwrap().actor_id;
        db.write(None, |w| Ok(w.conn().execute("UPDATE agent_configs SET model='opus', effort='high' WHERE actor_id=?1", [&backend])?)).unwrap();

        onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", Some(&id))).unwrap();
        assert_eq!(agents(&db, &s), four_on(&id, mode), "on {name}");
        let lead = team::chat_agent(&db).unwrap().unwrap();
        assert_eq!((lead.name.as_str(), lead.adapter.as_deref()), ("Team Lead", Some(clis::CLAUDE_CODE)), "on {name}: the chat stays");
        assert!(!onboarding::pending(&db).unwrap());
        assert_eq!(clis::list(&db).unwrap(), list, "the CLIs themselves are left as they are");
    }
}

#[test]
fn agents_on_codex_move_back_to_claude_code_with_accept_edits() {
    // Only Codex was installed at the first start: the seed put the four on it.
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed_with_agents(&db, "jefsev", || Some(cli("Codex", "codex", "/opt/codex/bin/codex"))).unwrap();
    assert!(onboarding::pending(&db).unwrap());
    let codex = id_of(&clis::list(&db).unwrap(), "Codex");
    assert_eq!(agents(&db, &s), four_on(&codex, "workspace-write"));
    onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", Some(clis::CLAUDE_CODE))).unwrap();
    assert_eq!(agents(&db, &s), four_on(clis::CLAUDE_CODE, "acceptEdits"));
}

#[test]
fn an_empty_agents_cli_leaves_the_agents_where_they_are() {
    let (db, s) = new_install();
    add_clis(&db);
    let before = agents(&db, &s);
    onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", Some("  "))).unwrap();
    assert_eq!(agents(&db, &s), before);
    assert_eq!(you(&db, &s).0, "Jeffrey");
}

#[test]
fn only_the_first_teams_agents_move_and_another_team_is_left_alone() {
    let (db, s) = new_install();
    let list = add_clis(&db);
    let codex = id_of(&list, "Codex");
    // A second team, newer than the first, with a Backend Agent of its own.
    std::thread::sleep(std::time::Duration::from_millis(5));
    let other = team::add_team(&db, &s.you_id, "Other team").unwrap();
    team::add_agent(&db, &s.you_id, &other, gizai_core::model::AgentInput {
        name: "Other Backend".into(), role_key: "backend".into(), ..Default::default() }).unwrap();
    onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", Some(&codex))).unwrap();
    assert_eq!(team_name(&db, &s), "YoTech", "the first team is renamed");
    assert_eq!(agents(&db, &s), four_on(&codex, "workspace-write"));
    let t = team::get(&db, &other).unwrap();
    assert_eq!(t.name, "Other team", "another team keeps its name");
    let m = t.members.iter().find(|m| m.name == "Other Backend").unwrap();
    assert_eq!(m.adapter.as_deref(), Some(clis::CLAUDE_CODE), "another team's agents stay");
}

#[test]
fn skipping_only_ends_the_onboarding() {
    let (db, s) = new_install();
    add_clis(&db);
    let before = snapshot(&db);
    assert!(onboarding::skip(&db).unwrap(), "it waited");
    assert!(!onboarding::pending(&db).unwrap());
    let after = snapshot(&db);
    let gone: Vec<&String> = before.iter().filter(|r| !after.contains(r)).collect();
    let new: Vec<&String> = after.iter().filter(|r| !before.contains(r)).collect();
    assert_eq!(gone, [&"onboarding||\"pending\"".to_string()], "only the flag went");
    assert!(new.is_empty(), "nothing else changed: {new:?}");
    assert_eq!(you(&db, &s), ("jefsev".into(), "jefsev".into()));
    assert_eq!(team_name(&db, &s), "My team");
    // Nothing waits: skipping again does nothing, and finishing has nothing to set up.
    let snap = snapshot(&db);
    assert!(!onboarding::skip(&db).unwrap());
    assert_eq!(snapshot(&db), snap);
    assert_eq!(onboarding::finish(&db, &s.you_id, &input("Jeffrey", "YoTech", None)).unwrap_err().to_string(), "Nothing to set up");
    assert_eq!(snapshot(&db), snap);

    // On a database that never waited, skipping does nothing either.
    let db = Db::open_in_memory().unwrap();
    ensure_seed(&db, "jefsev").unwrap();
    let snap = snapshot(&db);
    assert!(!onboarding::skip(&db).unwrap());
    assert_eq!(snapshot(&db), snap);
}
