//! GA-96 QA: agents that share one memory folder, in core. Migration 0016 on a v0.7.0 (schema 15) database, the access
//! rule for an agent in a group, joining (learned lines once, notes moved, links kept), leaving, no chains, never the Team
//! Lead, the owner renamed or removed, learned lines with who learned them, the Memory block, client isolation and a
//! setting that survives a restart.
use gizai_core::db::{self, Db};
use gizai_core::memory::{self, Context, Who};
use gizai_core::model::*;
use gizai_core::{clients, projects, seed, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

const DAY: &str = "2026-10-11";

struct F {
    db: Db,
    you: String,
    team: String,
    lead: String,
    be: String,
    be2: String,
    qa: String,
}

/// A Team Lead (Chat on), a Backend Agent, a Backend Agent 2 and a QA Agent, each on its own folder.
fn setup() -> F {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let add = |name: &str, role: &str, chat: bool| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), chat_enabled: chat.then_some(true), ..Default::default() }).unwrap();
    let lead = add("Team Lead", "lead", true);
    let be = add("Backend Agent", "backend", false);
    let be2 = add("Backend Agent 2", "backend", false);
    let qa = add("QA Agent", "qa", false);
    F { you: s.you_id, team: s.team_id, lead, be, be2, qa, db }
}

impl F {
    fn lead(&self) -> Who { Who::Lead(self.lead.clone()) }
    fn you(&self) -> Who { Who::Person(self.you.clone()) }
    fn who(&self, id: &str) -> Who { Who::of(&self.db, id).unwrap() }
    /// A new agent, sharing `with`'s folder when given.
    fn add(&self, name: &str, with: Option<&str>) -> gizai_core::Result<String> {
        team::add_agent(&self.db, &self.you, &self.team, AgentInput { name: name.into(), role_key: "backend".into(),
            shares_memory_with: with.map(str::to_string), ..Default::default() })
    }
    /// Saves agent `id`'s form as you: a new name when given, and Shares memory with `with` (None: unchanged, "": its own
    /// folder).
    fn save(&self, id: &str, name: Option<&str>, with: Option<&str>) -> gizai_core::Result<()> {
        let m = team::agent(&self.db, id).unwrap();
        team::update_agent(&self.db, &self.you, id, AgentInput { name: name.map(str::to_string).unwrap_or(m.name), role_key: m.role_key,
            wakeup: m.wakeup.unwrap_or_default(), shares_memory_with: with.map(str::to_string), ..Default::default() })
    }
    fn shares(&self, id: &str) -> Option<String> { team::agent(&self.db, id).unwrap().shares_memory_with }
    fn note(&self, path: &str) -> Option<memory::Note> { memory::find(&self.db, path).unwrap() }
    fn body(&self, path: &str) -> String { self.note(path).unwrap_or_else(|| panic!("{path} missing")).body_md }
    fn path_of(&self, id: &str) -> String { memory::get(&self.db, &self.lead(), id).unwrap().path }
    fn write(&self, who: &Who, path: &str, body: &str) -> String { memory::write(&self.db, who, path, body, None, None).unwrap().id }
    fn learned(&self, agent: &str, card: &str, lines: &[&str]) {
        let lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        memory::learned(&self.db, agent, card, &lines, None, DAY).unwrap();
    }
    /// Every note in an agent's folder, by path (the Team Lead's view).
    fn agent_paths(&self) -> Vec<String> {
        let mut p: Vec<String> = memory::list(&self.db, &self.lead()).unwrap().into_iter().map(|n| n.path).filter(|p| p.starts_with("Agents/")).collect();
        p.sort();
        p
    }
    fn paths(&self, who: &Who) -> Vec<String> {
        let mut p: Vec<String> = memory::list(&self.db, who).unwrap().into_iter().map(|n| n.path).collect();
        p.sort();
        p
    }
    /// The notes a note's links find, by id, sorted.
    fn links(&self, id: &str) -> Vec<String> {
        let mut l: Vec<String> = memory::links_from(&self.db, id).unwrap().into_iter().filter(|l| l.target_type == "doc").map(|l| l.target_id).collect();
        l.sort();
        l
    }
    fn block(&self, who: &Who, cx: &Context) -> memory::Block { memory::prompt_block(&self.db, who, cx).unwrap() }
    fn exec(&self, sql: &str, id: &str) {
        self.db.write(None, |w| { w.conn().execute(sql, [id])?; Ok(()) }).unwrap();
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn kade() -> Context {
    Context { role: "backend".into(), project: Some(("KADE".into(), "Kade".into())), client: None }
}

// ---- Migration 0016 ----

/// A genuine schema 15 database file (migrations 0001..0015, as v0.7.0 left it) with a person, a Team Lead, a Backend
/// Agent and a Backend Agent 2, each agent with its own Notes.
fn v15_db(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("gizai.db");
    let mut c = Connection::open(&path).unwrap();
    c.pragma_update(None, "foreign_keys", "OFF").unwrap();
    let all = [
        include_str!("../migrations/0001_init.sql"), include_str!("../migrations/0002_agents.sql"), include_str!("../migrations/0003_chat.sql"),
        include_str!("../migrations/0004_effort.sql"), include_str!("../migrations/0005_pull_requests.sql"),
        include_str!("../migrations/0006_worktree_prepare.sql"), include_str!("../migrations/0007_card_flow.sql"),
        include_str!("../migrations/0008_board_check.sql"), include_str!("../migrations/0009_agent_folders.sql"),
        include_str!("../migrations/0010_run_refusals.sql"), include_str!("../migrations/0011_column_agents.sql"),
        include_str!("../migrations/0012_chat_runs_on.sql"), include_str!("../migrations/0013_bitbucket.sql"),
        include_str!("../migrations/0014_memory.sql"), include_str!("../migrations/0015_lead_merges.sql"),
    ];
    assert_eq!(all.len() as i64, db::SCHEMA_VERSION - 1, "one schema step back");
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch("
        INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
        INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
          ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active'),
          ('lead', 1, 1, 'org', 'agent', 'Team Lead', 'lead', 'active'),
          ('be', 2, 2, 'org', 'agent', 'Backend Agent', 'backend', 'active'),
          ('be2', 3, 3, 'org', 'agent', 'Backend Agent 2', 'backend-2', 'active');
        INSERT INTO agent_configs (actor_id, created_at, updated_at, adapter, wakeup, chat_enabled) VALUES
          ('lead', 1, 1, 'claude_code', 'manual', 1), ('be', 2, 2, 'claude_code', 'on_assign', 0), ('be2', 3, 3, 'claude_code', 'on_assign', 0);
        INSERT INTO teams (id, created_at, updated_at, org_id, name, lead_actor_id) VALUES ('team', 1, 1, 'org', 'Software', 'lead');
        INSERT INTO team_members (team_id, actor_id, role_key, is_lead, created_at) VALUES
          ('team', 'you', 'reviewer', 0, 1), ('team', 'lead', 'lead', 1, 2), ('team', 'be', 'backend', 0, 3), ('team', 'be2', 'backend', 0, 4);
        INSERT INTO docs (id, created_at, updated_at, created_by, updated_by, org_id, title, body_md, current_version, kind, path, scope, owner_actor_id) VALUES
          ('n-be', 2, 2, 'be', 'be', 'org', 'Notes', '# Notes\n\n## Learned\n\n- 2026-10-09 (GA-1): One.\n', 1, 'memory', 'Agents/Backend Agent/Notes', 'agent', 'be'),
          ('n-be2', 3, 3, 'be2', 'be2', 'org', 'Notes', '# Notes\n\n## Learned\n\n- 2026-10-10 (GA-2): Two.\n', 1, 'memory', 'Agents/Backend Agent 2/Notes', 'agent', 'be2');
        INSERT INTO doc_versions (id, created_at, doc_id, version, body_md, author_actor_id) VALUES
          ('v-be', 2, 'n-be', 1, '# Notes\n\n## Learned\n\n- 2026-10-09 (GA-1): One.\n', 'be'),
          ('v-be2', 3, 'n-be2', 1, '# Notes\n\n## Learned\n\n- 2026-10-10 (GA-2): Two.\n', 'be2');
        INSERT INTO devices (id, created_at, name, is_self) VALUES ('dev', 1, 'test', 1);").unwrap();
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 15, "the fixture is schema 15");
    path
}

#[test]
fn migration_0016_on_a_v070_database_keeps_every_agent_on_its_own_folder_after_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = v15_db(dir.path());
    let db = Db::open(&path).unwrap();
    assert_eq!(db::SCHEMA_VERSION, 16);
    let (v, broken): (i64, i64) = db.read(|c| Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?))).unwrap();
    assert_eq!((v, broken), (16, 0), "schema 16, foreign keys intact");
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(snaps.len() == 1 && snaps[0].starts_with("gizai-before-v16-"), "{snaps:?}");
    // the column: optional, empty for every agent
    let (notnull, default, set): (i64, Option<String>, i64) = db.read(|c| Ok((
        c.query_row("SELECT \"notnull\" FROM pragma_table_info('agent_configs') WHERE name = 'shares_memory_with'", [], |r| r.get(0))?,
        c.query_row("SELECT dflt_value FROM pragma_table_info('agent_configs') WHERE name = 'shares_memory_with'", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM agent_configs WHERE shares_memory_with IS NOT NULL", [], |r| r.get(0))?))).unwrap();
    assert_eq!((notnull, default, set), (0, None, 0));
    for id in ["be", "be2"] {
        assert_eq!(team::agent(&db, id).unwrap().shares_memory_with, None, "{id}: its own folder");
        assert_eq!(Who::of(&db, id).unwrap(), Who::Agent(id.into()));
        assert_eq!(memory::folder_owner(&db, id).unwrap(), id);
    }
    // start-up has nothing to hand over and no folder to make; the notes are as they were
    assert_eq!(memory::ensure_groups(&db, "you").unwrap(), 0);
    assert_eq!(memory::ensure_agent_folders(&db).unwrap(), 0);
    let n = memory::find(&db, "Agents/Backend Agent 2/Notes").unwrap().unwrap();
    assert_eq!((n.owner_id.as_deref(), n.current_version, n.body_md.as_str()), (Some("be2"), 1, "# Notes\n\n## Learned\n\n- 2026-10-10 (GA-2): Two.\n"));
    // an agent alone keeps writing as before: no name on its lines
    memory::learned(&db, "be2", "GA-3", &["Three.".into()], None, DAY).unwrap();
    assert_eq!(memory::find(&db, "Agents/Backend Agent 2/Notes").unwrap().unwrap().body_md,
               "# Notes\n\n## Learned\n\n- 2026-10-10 (GA-2): Two.\n- 2026-10-11 (GA-3): Three.\n");
    // the backup is the schema 15 database, without the column
    let old = Connection::open(dir.path().join("backups").join(&snaps[0])).unwrap();
    let (old_v, has): (i64, i64) = (old.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap(),
        old.query_row("SELECT count(*) FROM pragma_table_info('agent_configs') WHERE name = 'shares_memory_with'", [], |r| r.get(0)).unwrap());
    assert_eq!((old_v, has), (15, 0));
    // a second open changes nothing and makes no new backup
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(team::agent(&db, "be2").unwrap().shares_memory_with, None);
    assert_eq!(std::fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
}

// ---- The access rule ----

#[test]
fn an_agent_in_a_group_reads_and_writes_the_groups_folder_and_the_shared_ones_never_another_agents_or_the_team_leads() {
    let f = setup();
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    let be2 = f.who(&f.be2);
    assert_eq!(be2, Who::Shares(f.be2.clone(), f.be.clone()));
    assert_eq!(be2.folder_owner(), f.be);
    assert_eq!(memory::folder_owner(&f.db, &f.be2).unwrap(), f.be);
    assert_eq!(f.who(&f.be), Who::Agent(f.be.clone()), "the owner keeps its own folder");
    memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap();
    f.write(&f.lead(), "Standards/Rust style", "Use thiserror.");
    let get = |p: &str| memory::get(&f.db, &f.lead(), p).unwrap();
    let (shared, group, lead_notes, qa_notes) = (get("Standards/Rust style"), get("Agents/Backend Agent/Notes"), get("Team Lead/Notes"), get("Agents/QA Agent/Notes"));

    for who in [be2.clone(), f.who(&f.be)] {
        assert!(memory::can_read(&who, &shared) && !memory::can_write(&who, &shared), "{who:?}: shared, read only");
        assert!(memory::can_read(&who, &group) && memory::can_write(&who, &group), "{who:?}: the group's folder, read and write");
        for n in [&lead_notes, &qa_notes] {
            assert!(!memory::can_read(&who, n) && !memory::can_write(&who, n), "{who:?} on {}", n.path);
        }
    }
    // the rule holds for any owner: a Shares pointing at the QA Agent reads the QA Agent's folder, not the Backend Agent's
    let other = Who::Shares(f.be2.clone(), f.qa.clone());
    assert!(memory::can_read(&other, &qa_notes) && memory::can_write(&other, &qa_notes));
    assert!(!memory::can_read(&other, &group) && !memory::can_write(&other, &group));
    assert_eq!(f.paths(&be2), ["Agents/Backend Agent/Notes", "Standards/Rust style"], "what it sees");
    assert_eq!(f.paths(&be2), f.paths(&f.who(&f.be)), "the same as its owner");

    // a note it writes in the group's folder is the group's: the owner and the group see it, the QA Agent doesn't
    let s = memory::write(&f.db, &be2, "agents/backend agent/Gotchas", "Use -j 2.", None, None).unwrap();
    assert_eq!(s.path, "Agents/Backend Agent/Gotchas");
    let n = memory::get(&f.db, &f.who(&f.be), &s.id).unwrap();
    assert_eq!((n.scope.as_str(), n.owner_id.as_deref()), ("agent", Some(f.be.as_str())));
    assert!(memory::get(&f.db, &f.who(&f.qa), &s.id).is_err());
    assert!(memory::append(&f.db, &f.who(&f.be), "Agents/Backend Agent/Gotchas", None, "And -j 1 when it is killed.", None).is_ok());
    assert!(memory::append(&f.db, &be2, "Agents/Backend Agent/Notes", Some("Learned"), "- by hand", None).is_ok());
    // nowhere else: not in a folder of its own, not another agent's, not the Team Lead's, not a shared one
    let e = memory::write(&f.db, &be2, "Agents/Backend Agent 2/Ideas", "x", None, None).unwrap_err().to_string();
    assert!(e.contains("Backend Agent 2 shares Backend Agent's memory folder") && e.contains("Agents/Backend Agent/"), "{e}");
    for path in ["Agents/QA Agent/Gotchas", "Team Lead/Ideas", "Standards/Go style"] {
        assert!(memory::write(&f.db, &be2, path, "x", None, None).is_err(), "{path}");
    }
    let e = memory::write(&f.db, &be2, "Standards/Go style", "x", None, None).unwrap_err().to_string();
    assert!(e.contains("own folder"), "{e}");
    assert!(memory::append(&f.db, &be2, "Agents/QA Agent/Notes", Some("Learned"), "x", None).is_err());
    assert!(memory::get(&f.db, &be2, "Team Lead/Notes").is_err() && memory::get(&f.db, &be2, "Agents/QA Agent/Notes").is_err());
}

// ---- Joining ----

#[test]
fn joining_adds_its_learned_lines_to_the_owners_notes_once_moves_its_other_notes_and_every_link_still_finds_its_note() {
    let f = setup();
    f.learned(&f.be, "GA-1", &["Both know this.", "Only the Backend Agent knows this."]);
    f.learned(&f.be2, "GA-1", &["Both know this.", "Backend Agent 2 learned this."]);
    f.learned(&f.be2, "GA-3", &["And this, on another card."]);
    let (be, be2) = (f.who(&f.be), f.who(&f.be2));
    let be_cargo = f.write(&be, "Agents/Backend Agent/Cargo", "Backend Agent's cargo tips.");
    let be2_cargo = f.write(&be2, "Agents/Backend Agent 2/Cargo", "Backend Agent 2's cargo tips. Deploys: [[Deploys]].");
    let deploys = f.write(&be2, "Agents/Backend Agent 2/Deploys", "## Kamal\nDeploy with Kamal. Build first: [[Cargo]].");
    let docker = f.write(&be2, "Agents/Backend Agent 2/Gotchas/Docker", "Docker needs [[Deploys#Kamal|the deploy notes]].");
    let handover = f.write(&f.lead(), "Workflows/Handover", "Read [[Agents/Backend Agent 2/Cargo]], [[Agents/Backend Agent 2/Deploys|its deploys]], \
        ![[Agents/Backend Agent 2/Gotchas/Docker]], [[Agents/Backend Agent 2/Notes#Learned]] and [[Agents/Backend Agent/Cargo]].");
    let be_notes = f.note("Agents/Backend Agent/Notes").unwrap().id;
    let be2_notes = f.note("Agents/Backend Agent 2/Notes").unwrap().id;
    // before: a short link finds the note in its own folder
    assert_eq!(f.links(&deploys), [be2_cargo.clone()]);
    assert_eq!(f.links(&handover), sorted(vec![be2_cargo.clone(), deploys.clone(), docker.clone(), be2_notes.clone(), be_cargo.clone()]));
    let count = memory::list(&f.db, &f.lead()).unwrap().len();

    f.save(&f.be2, None, Some(&f.be)).unwrap();
    assert_eq!(f.shares(&f.be2), Some(f.be.clone()));

    // its learned lines are in the owner's Notes under Learned, each once, after the owner's
    let body = f.body("Agents/Backend Agent/Notes");
    assert!(body.ends_with(&format!("## Learned\n\n- {DAY} (GA-1): Both know this.\n- {DAY} (GA-1): Only the Backend Agent knows this.\n\
        - {DAY} (GA-1): Backend Agent 2 learned this.\n- {DAY} (GA-3): And this, on another card.\n")), "{body}");
    assert_eq!(body.matches("Both know this.").count(), 1, "not doubled: {body}");
    assert_eq!(body.matches("## Learned").count(), 1);
    // its Notes was only its template and those lines: it is gone, and nothing is left in a folder of its own
    assert!(f.note("Agents/Backend Agent 2/Notes").is_none());
    assert!(memory::get(&f.db, &f.lead(), &be2_notes).is_err());
    assert_eq!(f.agent_paths(), ["Agents/Backend Agent/Cargo", "Agents/Backend Agent/Cargo (Backend Agent 2)", "Agents/Backend Agent/Deploys",
                                 "Agents/Backend Agent/Gotchas/Docker", "Agents/Backend Agent/Notes", "Agents/QA Agent/Notes"]);
    assert_eq!(memory::list(&f.db, &f.lead()).unwrap().len(), count - 1, "no note lost but its Notes");
    // its other notes keep their place in the folder, a taken title gets its name; they are the group's now
    assert_eq!(f.path_of(&be2_cargo), "Agents/Backend Agent/Cargo (Backend Agent 2)");
    assert_eq!(f.path_of(&deploys), "Agents/Backend Agent/Deploys");
    assert_eq!(f.path_of(&docker), "Agents/Backend Agent/Gotchas/Docker");
    assert_eq!((f.path_of(&be_cargo), f.body("Agents/Backend Agent/Cargo")), ("Agents/Backend Agent/Cargo".into(), "Backend Agent's cargo tips.".into()));
    for id in [&be2_cargo, &deploys, &docker] {
        let n = memory::get(&f.db, &f.lead(), id).unwrap();
        assert_eq!((n.scope.as_str(), n.owner_id.as_deref()), ("agent", Some(f.be.as_str())), "{}", n.path);
        assert!(memory::get(&f.db, &f.who(&f.be2), id).is_ok() && memory::get(&f.db, &f.who(&f.be), id).is_ok());
        assert!(memory::get(&f.db, &f.who(&f.qa), id).is_err());
    }
    // every link finds the note it found: a short [[Cargo]] still the Backend Agent 2's, not the Backend Agent's
    assert_eq!(f.links(&deploys), [be2_cargo.clone()], "{}", memory::get(&f.db, &f.lead(), &deploys).unwrap().body_md);
    assert_eq!(f.links(&be2_cargo), [deploys.clone()]);
    assert_eq!(f.links(&docker), [deploys.clone()]);
    assert_eq!(f.links(&handover), sorted(vec![be2_cargo.clone(), deploys.clone(), docker.clone(), be_notes.clone(), be_cargo.clone()]),
               "the link to its Notes finds the owner's, where its lines went");
    let h = memory::get(&f.db, &f.lead(), &handover).unwrap().body_md;
    for link in ["[[Agents/Backend Agent/Cargo (Backend Agent 2)]]", "[[Agents/Backend Agent/Deploys|its deploys]]",
                 "![[Agents/Backend Agent/Gotchas/Docker]]", "[[Agents/Backend Agent/Notes#Learned]]", "[[Agents/Backend Agent/Cargo]]"] {
        assert!(h.contains(link), "{link} in {h}");
    }
    assert!(memory::get(&f.db, &f.lead(), &docker).unwrap().body_md.contains("#Kamal|the deploy notes]]"), "the heading and alias stay");
    // no folder of its own is made for it at start-up
    assert_eq!(memory::ensure_agent_folders(&f.db).unwrap(), 0);
    assert!(f.note("Agents/Backend Agent 2/Notes").is_none());
}

#[test]
fn joining_with_more_than_its_template_in_its_notes_keeps_the_rest_as_notes_with_its_name() {
    let f = setup();
    memory::append(&f.db, &f.you(), "Agents/Backend Agent 2/Notes", Some("Preferences"), "Jeffrey likes small commits.", None).unwrap();
    f.learned(&f.be2, "GA-2", &["A lesson."]);
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    assert!(f.body("Agents/Backend Agent/Notes").contains(&format!("- {DAY} (GA-2): A lesson.")));
    let rest = f.body("Agents/Backend Agent/Notes (Backend Agent 2)");
    assert!(rest.contains("Jeffrey likes small commits."), "{rest}");
    assert!(!rest.contains("A lesson.") && !rest.contains("## Learned"), "its learned lines went to the owner's Notes: {rest}");
    assert!(f.note("Agents/Backend Agent 2/Notes").is_none());
}

// ---- Learned lines and the Memory block ----

#[test]
fn a_group_writes_its_learned_lines_in_the_owners_notes_with_who_learned_them_and_every_run_gets_them() {
    let f = setup();
    f.learned(&f.be, "GA-11", &["The owner's lesson."]);
    assert!(f.body("Agents/Backend Agent/Notes").contains(&format!("- {DAY} (GA-11): The owner's lesson.\n")), "alone: as before");
    f.save(&f.be2, None, Some(&f.be)).unwrap();

    // a Backend Agent 2 run's lines land in Agents/Backend Agent/Notes with its name, written by it
    f.learned(&f.be2, "GA-12", &["Use -j 2 when the linker is killed."]);
    let n = f.note("Agents/Backend Agent/Notes").unwrap();
    assert!(n.body_md.contains(&format!("- {DAY} (GA-12, Backend Agent 2): Use -j 2 when the linker is killed.\n")), "{}", n.body_md);
    assert_eq!(n.updated_by.as_deref(), Some("Backend Agent 2"));
    let author: String = f.db.read(|c| Ok(c.query_row("SELECT author_actor_id FROM doc_versions WHERE doc_id = ?1 AND version = ?2",
        rusqlite::params![n.id, n.current_version], |r| r.get(0))?)).unwrap();
    assert_eq!(author, f.be2);
    assert!(f.note("Agents/Backend Agent 2/Notes").is_none(), "not in a folder of its own");
    // while it shares, the owner's lines say who learned them too
    f.learned(&f.be, "GA-13", &["The owner again."]);
    assert!(f.body("Agents/Backend Agent/Notes").contains(&format!("- {DAY} (GA-13, Backend Agent): The owner again.\n")));
    // an agent alone writes as before
    f.learned(&f.qa, "GA-14", &["QA alone."]);
    assert!(f.body("Agents/QA Agent/Notes").contains(&format!("- {DAY} (GA-14): QA alone.\n")));

    // a Backend Agent 2 run gets the group's notes as its own, first
    let b = f.block(&f.who(&f.be2), &kade());
    assert_eq!(b.given.first().map(|g| g.path.as_str()), Some("Agents/Backend Agent/Notes"), "{:?}", b.given);
    assert!(b.text.contains("The owner's lesson.") && b.text.contains("The owner again."), "{}", b.text);
    assert!(!b.text.contains("QA alone"));
    // and the Backend Agent's next run gets the Backend Agent 2's lines
    let b = f.block(&f.who(&f.be), &kade());
    assert_eq!(b.given.first().map(|g| g.path.as_str()), Some("Agents/Backend Agent/Notes"));
    assert!(b.text.contains("(GA-12, Backend Agent 2): Use -j 2 when the linker is killed."), "{}", b.text);
    // a note one of them writes in the folder reaches the other's run
    f.write(&f.who(&f.be2), "Agents/Backend Agent/Gotchas", "Run the fake CLI, never the real one.");
    let b = f.block(&f.who(&f.be), &kade());
    assert!(b.given.iter().any(|g| g.path == "Agents/Backend Agent/Gotchas") && b.text.contains("never the real one"), "{:?}", b.given);
    // the QA Agent's run gets none of it
    let qa = Context { role: "qa".into(), ..kade() };
    let b = f.block(&f.who(&f.qa), &qa);
    assert!(!b.text.contains("Backend Agent 2") && !b.text.contains("never the real one"), "{}", b.text);
}

// ---- Leaving ----

#[test]
fn leaving_gives_a_fresh_notes_and_the_groups_notes_stay_where_they_are() {
    let f = setup();
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    f.learned(&f.be2, "GA-12", &["Learned in the group."]);
    let gotcha = f.write(&f.who(&f.be2), "Agents/Backend Agent/Gotchas", "A group gotcha.");
    let group_notes = f.body("Agents/Backend Agent/Notes");
    let before = f.agent_paths();

    f.save(&f.be2, None, Some("")).unwrap();
    assert_eq!(f.shares(&f.be2), None);
    assert_eq!(f.who(&f.be2), Who::Agent(f.be2.clone()));
    let n = f.note("Agents/Backend Agent 2/Notes").expect("a fresh Notes");
    assert_eq!((n.body_md.as_str(), n.owner_id.as_deref(), n.current_version), (memory::agent_template("Backend Agent 2").as_str(), Some(f.be2.as_str()), 1));
    // the group's notes stay, with its lines
    let mut expected = before.clone();
    expected.push("Agents/Backend Agent 2/Notes".into());
    assert_eq!(f.agent_paths(), sorted(expected));
    assert_eq!(f.body("Agents/Backend Agent/Notes"), group_notes);
    assert_eq!(f.path_of(&gotcha), "Agents/Backend Agent/Gotchas");
    // it reads and writes only its own folder again
    let be2 = f.who(&f.be2);
    assert!(memory::get(&f.db, &be2, &gotcha).is_err());
    assert!(memory::write(&f.db, &be2, "Agents/Backend Agent/Ideas", "x", None, None).is_err());
    assert!(memory::write(&f.db, &be2, "Agents/Backend Agent 2/Ideas", "x", None, None).is_ok());
    // both alone again: lines without a name
    f.learned(&f.be2, "GA-15", &["Alone again."]);
    f.learned(&f.be, "GA-16", &["The owner alone."]);
    assert!(f.body("Agents/Backend Agent 2/Notes").ends_with(&format!("- {DAY} (GA-15): Alone again.\n")));
    assert!(f.body("Agents/Backend Agent/Notes").ends_with(&format!("- {DAY} (GA-16): The owner alone.\n")));
    // saving its own folder again changes nothing
    f.save(&f.be2, None, Some("")).unwrap();
    assert_eq!(f.note("Agents/Backend Agent 2/Notes").unwrap().current_version, 2);

    // from one group to another: its notes stay with the old group, nothing moves into the new one
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    let (be_paths, be_notes) = (f.agent_paths(), f.body("Agents/Backend Agent/Notes"));
    let qa_notes = f.body("Agents/QA Agent/Notes");
    f.save(&f.be2, None, Some(&f.qa)).unwrap();
    assert_eq!(f.shares(&f.be2), Some(f.qa.clone()));
    assert_eq!((f.agent_paths(), f.body("Agents/Backend Agent/Notes"), f.body("Agents/QA Agent/Notes")), (be_paths, be_notes, qa_notes));
    assert_eq!(f.who(&f.be2), Who::Shares(f.be2.clone(), f.qa.clone()));
}

// ---- Picking ----

#[test]
fn picking_an_agent_that_shares_picks_its_owner_and_the_team_lead_itself_or_an_unknown_agent_is_refused_and_changes_nothing() {
    let f = setup();
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    // no chains: an agent that shares stands for its group's owner, on create and on update
    let be3 = f.add("Backend Agent 3", Some(&f.be2)).unwrap();
    assert_eq!(f.shares(&be3), Some(f.be.clone()));
    assert_eq!(f.who(&be3), Who::Shares(be3.clone(), f.be.clone()));
    assert!(f.note("Agents/Backend Agent 3/Notes").is_none(), "made in the group: no folder of its own");
    let be4 = f.add("Backend Agent 4", None).unwrap();
    f.save(&be4, None, Some(&f.be2)).unwrap();
    assert_eq!(f.shares(&be4), Some(f.be.clone()));

    let chat = f.add("Chat Agent", None).unwrap();
    f.db.write(None, |w| { w.conn().execute("UPDATE agent_configs SET chat_enabled = 1 WHERE actor_id = ?1", [&chat])?; Ok(()) }).unwrap();
    let before = f.agent_paths();
    // the Team Lead can't be picked; that save changes nothing, its other fields neither
    let e = f.save(&f.be2, Some("Renamed"), Some(&f.lead)).unwrap_err().to_string();
    assert!(e.contains("Team Lead"), "{e}");
    let m = team::agent(&f.db, &f.be2).unwrap();
    assert_eq!((m.name.as_str(), m.shares_memory_with.as_deref()), ("Backend Agent 2", Some(f.be.as_str())));
    let e = f.add("Backend Agent 5", Some(&f.lead)).unwrap_err().to_string();
    assert!(e.contains("Team Lead"), "{e}");
    assert!(!team::all_agents(&f.db).unwrap().iter().any(|(_, a)| a.name == "Backend Agent 5"), "no agent was added");
    // nor an agent with Chat on (the Team Lead as memory sees it)
    assert!(f.save(&be4, None, Some(&chat)).is_err());
    assert_eq!(f.shares(&be4), Some(f.be.clone()));
    // the Team Lead never shares
    assert!(f.save(&f.lead, None, Some(&f.be)).unwrap_err().to_string().contains("Team Lead"));
    assert_eq!(f.shares(&f.lead), None);
    // nor itself; an unknown agent is refused
    assert!(f.save(&f.be, None, Some(&f.be)).unwrap_err().to_string().contains("itself"));
    assert!(f.save(&f.be2, Some("Renamed"), Some("no-such-agent")).is_err());
    assert_eq!(team::agent(&f.db, &f.be2).unwrap().name, "Backend Agent 2");
    assert_eq!((f.shares(&f.be), f.shares(&f.be2)), (None, Some(f.be.clone())));
    assert_eq!(f.agent_paths(), before, "no note moved");

    // the owner picking one of its own group keeps its own folder
    f.save(&f.be, None, Some(&f.be2)).unwrap();
    assert_eq!((f.shares(&f.be), f.shares(&f.be2)), (None, Some(f.be.clone())));
    // an owner that joins another group brings its group along: still one owner
    f.learned(&f.be2, "GA-20", &["The group's lesson."]);
    f.save(&f.be, None, Some(&f.qa)).unwrap();
    for id in [&f.be, &f.be2, &be3, &be4] {
        assert_eq!(f.shares(id), Some(f.qa.clone()), "{id}");
        assert_eq!(f.who(id), Who::Shares(id.clone(), f.qa.clone()));
    }
    assert_eq!(f.agent_paths(), ["Agents/Chat Agent/Notes", "Agents/QA Agent/Notes"], "the Backend Agent's folder is in the QA Agent's");
    assert!(f.body("Agents/QA Agent/Notes").contains(&format!("- {DAY} (GA-20, Backend Agent 2): The group's lesson.")));
}

// ---- The owner renamed or removed ----

#[test]
fn renaming_the_owner_moves_the_folder_and_the_group_follows_renaming_a_member_moves_nothing() {
    let f = setup();
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    f.learned(&f.be2, "GA-12", &["Before the rename."]);
    let handover = f.write(&f.lead(), "Workflows/Handover", "Ask [[Agents/Backend Agent/Notes]].");

    f.save(&f.be, Some("Backend Prime"), None).unwrap();
    assert!(f.note("Agents/Backend Agent/Notes").is_none());
    let n = f.note("Agents/Backend Prime/Notes").expect("the folder follows the owner's name");
    assert_eq!(n.owner_id.as_deref(), Some(f.be.as_str()));
    assert!(n.body_md.contains(&format!("- {DAY} (GA-12, Backend Agent 2): Before the rename.")));
    assert_eq!(memory::get(&f.db, &f.lead(), &handover).unwrap().body_md, "Ask [[Agents/Backend Prime/Notes]].");
    // the group keeps working
    assert_eq!(f.shares(&f.be2), Some(f.be.clone()));
    let be2 = f.who(&f.be2);
    assert_eq!(be2, Who::Shares(f.be2.clone(), f.be.clone()));
    f.learned(&f.be2, "GA-13", &["After the rename."]);
    assert!(f.body("Agents/Backend Prime/Notes").contains(&format!("- {DAY} (GA-13, Backend Agent 2): After the rename.")));
    assert_eq!(f.block(&be2, &kade()).given.first().map(|g| g.path.clone()).as_deref(), Some("Agents/Backend Prime/Notes"));
    assert!(memory::write(&f.db, &be2, "Agents/Backend Prime/Ideas", "x", None, None).is_ok());
    // a member renamed: nothing moves, no folder is made for it, its next lines carry its new name
    let before = f.agent_paths();
    f.save(&f.be2, Some("Backend Second"), None).unwrap();
    assert_eq!(f.agent_paths(), before);
    f.learned(&f.be2, "GA-14", &["Under a new name."]);
    assert!(f.body("Agents/Backend Prime/Notes").contains(&format!("- {DAY} (GA-14, Backend Second): Under a new name.")));
}

#[test]
fn a_removed_owner_hands_the_folder_to_the_groups_next_agent_at_start_up_and_its_notes_stay() {
    let f = setup();
    let be3 = f.add("Backend Agent 3", None).unwrap();
    // the Backend Agent 2 is the older of the two others
    f.exec("UPDATE actors SET created_at = 100 WHERE id = ?1", &f.be2);
    f.exec("UPDATE actors SET created_at = 200 WHERE id = ?1", &be3);
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    f.save(&be3, None, Some(&f.be)).unwrap();
    f.learned(&f.be2, "GA-12", &["From the group."]);
    let gotcha = f.write(&f.who(&be3), "Agents/Backend Agent/Gotchas", "A group gotcha.");
    let handover = f.write(&f.lead(), "Workflows/Handover", "See [[Agents/Backend Agent/Gotchas]] and [[Agents/Backend Agent/Notes]].");
    let notes = f.note("Agents/Backend Agent/Notes").unwrap().id;
    assert_eq!(memory::ensure_groups(&f.db, &f.you).unwrap(), 0, "a group with its owner: nothing to do");

    f.exec("UPDATE actors SET deleted_at = 5 WHERE id = ?1", &f.be);
    assert_eq!(memory::ensure_groups(&f.db, &f.you).unwrap(), 1);
    assert_eq!((f.shares(&f.be2), f.shares(&be3)), (None, Some(f.be2.clone())), "the next agent owns the folder, the other shares it");
    assert_eq!(f.who(&f.be2), Who::Agent(f.be2.clone()));
    assert_eq!(f.who(&be3), Who::Shares(be3.clone(), f.be2.clone()));
    // the notes stay, in a folder renamed to the new owner's name, and the links follow
    assert!(f.agent_paths().iter().all(|p| !p.starts_with("Agents/Backend Agent/")), "{:?}", f.agent_paths());
    let n = f.note("Agents/Backend Agent 2/Notes").expect("the group's Notes");
    assert_eq!((n.id.as_str(), n.owner_id.as_deref()), (notes.as_str(), Some(f.be2.as_str())));
    assert!(n.body_md.contains(&format!("- {DAY} (GA-12, Backend Agent 2): From the group.")));
    assert_eq!(f.path_of(&gotcha), "Agents/Backend Agent 2/Gotchas");
    assert_eq!(f.links(&handover), sorted(vec![gotcha.clone(), notes.clone()]));
    assert_eq!(memory::get(&f.db, &f.lead(), &handover).unwrap().body_md, "See [[Agents/Backend Agent 2/Gotchas]] and [[Agents/Backend Agent 2/Notes]].");
    // the group goes on
    f.learned(&be3, "GA-13", &["After the hand-over."]);
    assert!(f.body("Agents/Backend Agent 2/Notes").contains(&format!("- {DAY} (GA-13, Backend Agent 3): After the hand-over.")));
    assert_eq!(f.block(&f.who(&be3), &kade()).given.first().map(|g| g.path.clone()).as_deref(), Some("Agents/Backend Agent 2/Notes"));
    assert!(f.block(&f.who(&f.be2), &kade()).text.contains("A group gotcha."));
    assert_eq!(memory::ensure_groups(&f.db, &f.you).unwrap(), 0, "only once");
}

#[test]
fn an_owner_taken_off_the_team_or_made_the_team_lead_hands_its_group_on_too() {
    // off the team: at start-up
    let f = setup();
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    f.learned(&f.be2, "GA-12", &["Kept."]);
    f.exec("UPDATE team_members SET deleted_at = 5 WHERE actor_id = ?1", &f.be);
    assert_eq!(memory::ensure_groups(&f.db, &f.you).unwrap(), 1);
    assert_eq!(f.shares(&f.be2), None);
    assert!(f.body("Agents/Backend Agent 2/Notes").contains("(GA-12, Backend Agent 2): Kept."));

    // made the Team Lead: at once, and the Team Lead shares nothing
    let f = setup();
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    f.learned(&f.be2, "GA-12", &["Kept too."]);
    let m = team::agent(&f.db, &f.be).unwrap();
    team::update_agent(&f.db, &f.you, &f.be, AgentInput { name: m.name, role_key: "lead".into(), wakeup: m.wakeup.unwrap_or_default(), ..Default::default() }).unwrap();
    assert_eq!((f.shares(&f.be), f.shares(&f.be2)), (None, None));
    assert_eq!(f.who(&f.be2), Who::Agent(f.be2.clone()));
    assert!(f.body("Agents/Backend Agent 2/Notes").contains("(GA-12, Backend Agent 2): Kept too."));
}

// ---- Client isolation ----

#[test]
fn a_note_about_another_client_never_reaches_a_run_from_the_groups_folder_and_a_joining_notes_about_one_moves_whole() {
    let f = setup();
    let acme = clients::create(&f.db, &f.you, ClientInput { name: "Acme".into(), ..Default::default() }).unwrap();
    let globex = clients::create(&f.db, &f.you, ClientInput { name: "Globex".into(), ..Default::default() }).unwrap();
    for (name, key, client) in [("Shop", "SHOP", &acme), ("Globex portal", "GX", &globex)] {
        projects::create(&f.db, &f.you, ProjectInput { name: name.into(), key: key.into(), client_id: Some(client.clone()), ..Default::default() }).unwrap();
    }
    let shop = Context { role: "backend".into(), project: Some(("SHOP".into(), "Shop".into())), client: Some("Acme".into()) };
    let gx = Context { role: "backend".into(), project: Some(("GX".into(), "Globex portal".into())), client: Some("Globex".into()) };
    f.save(&f.be2, None, Some(&f.be)).unwrap();
    f.write(&f.lead(), "Agents/Backend Agent/Globex gotchas", "---\nclient: Globex\nproject: GX\n---\nGlobex's staging needs a VPN.");
    f.write(&f.lead(), "Agents/Backend Agent/Shop gotchas", "---\nclient: Acme\n---\nShop's tests need Redis.");
    for who in [f.who(&f.be2), f.who(&f.be)] {
        let b = f.block(&who, &shop);
        assert!(b.given.iter().any(|g| g.path == "Agents/Backend Agent/Shop gotchas"), "{who:?}: {:?}", b.given);
        assert!(!b.text.contains("Globex") && !b.text.contains("VPN"), "{who:?}: a run for Acme got Globex's note from the group's folder: {}", b.text);
        let b = f.block(&who, &gx);
        assert!(b.text.contains("needs a VPN") && !b.text.contains("Redis"), "{who:?}: {}", b.text);
    }

    // an agent joins with a Notes about Globex: it moves whole, so its lines keep reaching only Globex's runs
    let be3 = f.add("Backend Agent 3", None).unwrap();
    let v = f.note("Agents/Backend Agent 3/Notes").unwrap().current_version;
    memory::write(&f.db, &f.you(), "Agents/Backend Agent 3/Notes",
        "---\ntype: note\ntags: [agent]\nclient: Globex\n---\n# Notes\n\n## Learned\n\n- 2026-10-10 (GX-1): Globex deploys at night.\n", Some(v), None).unwrap();
    f.save(&be3, None, Some(&f.be)).unwrap();
    assert!(!f.body("Agents/Backend Agent/Notes").contains("deploys at night"), "not added to the owner's Notes");
    let moved = f.body("Agents/Backend Agent/Notes (Backend Agent 3)");
    assert!(moved.contains("client: Globex") && moved.contains("- 2026-10-10 (GX-1): Globex deploys at night."), "{moved}");
    for who in [f.who(&f.be), f.who(&f.be2), f.who(&be3)] {
        assert!(!f.block(&who, &shop).text.contains("deploys at night"), "{who:?}");
        assert!(f.block(&who, &gx).text.contains("deploys at night"), "{who:?}");
    }
}

// ---- A restart ----

#[test]
fn the_setting_is_still_there_after_a_restart_and_start_up_changes_nothing_for_a_working_group() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gizai.db");
    let (you, be, be2) = {
        let db = Db::open(&path).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let add = |name: &str, with: Option<String>| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
            name: name.into(), role_key: "backend".into(), shares_memory_with: with, ..Default::default() }).unwrap();
        let be = add("Backend Agent", None);
        let be2 = add("Backend Agent 2", None);
        let m = team::agent(&db, &be2).unwrap();
        team::update_agent(&db, &s.you_id, &be2, AgentInput { name: m.name, role_key: m.role_key, wakeup: m.wakeup.unwrap_or_default(),
            shares_memory_with: Some(be.clone()), ..Default::default() }).unwrap();
        (s.you_id, be, be2)
    };
    let db = Db::open(&path).unwrap();
    assert_eq!(team::agent(&db, &be2).unwrap().shares_memory_with, Some(be.clone()));
    assert_eq!(team::agent(&db, &be).unwrap().shares_memory_with, None);
    assert_eq!(Who::of(&db, &be2).unwrap(), Who::Shares(be2.clone(), be.clone()));
    assert_eq!(memory::ensure_groups(&db, &you).unwrap(), 0);
    assert_eq!(memory::ensure_agent_folders(&db).unwrap(), 0);
    assert!(memory::find(&db, "Agents/Backend Agent 2/Notes").unwrap().is_none());
    assert_eq!(team::agent(&db, &be2).unwrap().shares_memory_with, Some(be));
    // a save that leaves Shares memory with out keeps it
    let m = team::agent(&db, &be2).unwrap();
    team::update_agent(&db, &you, &be2, AgentInput { name: m.name, role_key: m.role_key, wakeup: m.wakeup.unwrap_or_default(),
        title: Some("Second account".into()), ..Default::default() }).unwrap();
    assert!(team::agent(&db, &be2).unwrap().shares_memory_with.is_some());
}
