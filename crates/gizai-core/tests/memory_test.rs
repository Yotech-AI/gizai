//! GA-19 QA: Memory in core. Migration 0014 on a schema 13 database, the access rule (`can_read`, `can_write`), paths,
//! writes with the version check, appends under a heading, links in `doc_links` and backlinks, renames and moves that
//! rewrite the links, search, the secret refusal, the agents' own folders, `learned`, promoting a note, and the Memory
//! block of a prompt (caps, order, client isolation).
use gizai_core::db::{self, Db};
use gizai_core::memory::{self, Context, Who};
use gizai_core::model::*;
use gizai_core::{clients, docs, projects, runs, seed, settings, tasks, team};
use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

struct F {
    db: Db,
    you: String,
    team: String,
    lead: String,
    be: String,
    qa: String,
}

/// A Team Lead (Chat on), a Backend Agent and a QA Agent.
fn setup() -> F {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let add = |name: &str, role: &str, chat: bool| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), chat_enabled: chat.then_some(true), ..Default::default() }).unwrap();
    let lead = add("Team Lead", "lead", true);
    let be = add("Backend Agent", "backend", false);
    let qa = add("QA Agent", "qa", false);
    F { you: s.you_id, team: s.team_id, lead, be, qa, db }
}

impl F {
    fn lead(&self) -> Who { Who::Lead(self.lead.clone()) }
    fn be(&self) -> Who { Who::Agent(self.be.clone()) }
    fn qa(&self) -> Who { Who::Agent(self.qa.clone()) }
    fn you(&self) -> Who { Who::Person(self.you.clone()) }
    fn write(&self, path: &str, body: &str) -> String {
        memory::write(&self.db, &self.lead(), path, body, None, None).unwrap().id
    }
    fn body(&self, id: &str) -> String {
        memory::get(&self.db, &self.lead(), id).unwrap().body_md
    }
    fn paths(&self, who: &Who) -> Vec<String> {
        memory::list(&self.db, who).unwrap().into_iter().map(|n| n.path).collect()
    }
    fn project(&self, name: &str, key: &str, client: Option<&str>) -> String {
        projects::create(&self.db, &self.you, ProjectInput { name: name.into(), key: key.into(), client_id: client.map(str::to_string), ..Default::default() }).unwrap()
    }
}

fn err<T: std::fmt::Debug>(r: gizai_core::Result<T>) -> String {
    r.unwrap_err().to_string()
}

// ---- Migration 0014 ----

/// A genuine schema 13 database file (migrations 0001..0013), with a person, a Team Lead, a Backend agent, a project and
/// one doc with two versions, as the previous release left them.
fn v13_db(dir: &std::path::Path) -> std::path::PathBuf {
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
    ];
    Migrations::new(all.iter().map(|sql| M::up(sql)).collect()).to_latest(&mut c).unwrap();
    c.pragma_update(None, "foreign_keys", "ON").unwrap();
    c.execute_batch("
        INSERT INTO orgs (id, created_at, updated_at, name, key) VALUES ('org', 1, 1, 'Yotech', 'YT');
        INSERT INTO actors (id, created_at, updated_at, org_id, kind, name, handle, status) VALUES
          ('you', 1, 1, 'org', 'person', 'Jeffrey', 'jeffrey', 'active'),
          ('lead', 1, 1, 'org', 'agent', 'Master Chief', 'chief', 'active'),
          ('be', 2, 2, 'org', 'agent', 'Backend', 'backend', 'active');
        INSERT INTO agent_configs (actor_id, created_at, updated_at, adapter, wakeup) VALUES ('lead', 1, 1, 'claude_code', 'manual'), ('be', 2, 2, 'claude_code', 'on_assign');
        INSERT INTO teams (id, created_at, updated_at, org_id, name, lead_actor_id) VALUES ('team', 1, 1, 'org', 'Software', 'lead');
        INSERT INTO team_members (team_id, actor_id, role_key, is_lead, created_at) VALUES
          ('team', 'you', 'reviewer', 0, 1), ('team', 'lead', 'lead', 1, 2), ('team', 'be', 'backend', 0, 3);
        INSERT INTO projects (id, created_at, updated_at, org_id, number, key, name) VALUES ('p', 1, 1, 'org', '2026-001', 'KADE', 'Kade');
        INSERT INTO docs (id, created_at, updated_at, created_by, updated_by, org_id, project_id, title, body_md, current_version) VALUES
          ('d1', 1, 5, 'you', 'you', 'org', 'p', 'Requirements', '# Requirements\nSee [[Rust style]].', 2);
        INSERT INTO doc_versions (id, created_at, doc_id, version, body_md, author_actor_id) VALUES
          ('v1', 1, 'd1', 1, '', 'you'), ('v2', 5, 'd1', 2, '# Requirements\nSee [[Rust style]].', 'you');
        INSERT INTO devices (id, created_at, name, is_self) VALUES ('dev', 1, 'test', 1);").unwrap();
    let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(v, 13, "the fixture is schema 13");
    path
}

#[test]
fn migration_0014_keeps_existing_docs_as_they_were_and_a_projects_doc_list_never_shows_memory_notes() {
    let dir = tempfile::tempdir().unwrap();
    let path = v13_db(dir.path());
    let db = Db::open(&path).unwrap();
    assert_eq!(db::SCHEMA_VERSION, 14);
    let (v, broken): (i64, i64) = db.read(|c| Ok((c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
        c.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r.get(0))?))).unwrap();
    assert_eq!((v, broken), (14, 0), "schema 14, foreign keys intact");
    let snaps: Vec<String> = std::fs::read_dir(dir.path().join("backups")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(snaps.len() == 1 && snaps[0].starts_with("gizai-before-v14-"), "{snaps:?}");

    // the old doc is unchanged: a doc of kind doc, no path, scope or owner, its text and versions as they were
    let row: (String, Option<String>, Option<String>, Option<String>, String, i64, i64) = db.read(|c| Ok(c.query_row(
        "SELECT kind, path, scope, owner_actor_id, body_md, current_version, updated_at FROM docs WHERE id = 'd1'", [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?)).unwrap();
    assert_eq!(row, ("doc".into(), None, None, None, "# Requirements\nSee [[Rust style]].".into(), 2, 5));
    let d = docs::get(&db, "d1").unwrap();
    assert_eq!((d.kind.as_str(), d.path.as_deref(), d.current_version), ("doc", None, 2));
    assert_eq!(docs::versions(&db, "d1").unwrap().len(), 2);
    // the agents use memory unless switched off, and every run has room for its notes
    let on: Vec<i64> = db.read(|c| { let mut st = c.prepare("SELECT use_memory FROM agent_configs ORDER BY actor_id")?;
        Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?) }).unwrap();
    assert_eq!(on, [1, 1]);
    assert_eq!(team::agent(&db, "be").unwrap().use_memory, true);

    // agents made before memory get their own folder at start-up, once; the Team Lead keeps Team Lead/ instead
    assert_eq!(memory::ensure_agent_folders(&db).unwrap(), 1);
    assert_eq!(memory::ensure_agent_folders(&db).unwrap(), 0, "only once");
    let n = memory::find(&db, "Agents/Backend/Notes").unwrap().expect("Backend's own notes");
    assert_eq!((n.scope.as_str(), n.owner_id.as_deref()), ("agent", Some("be")));
    assert!(memory::find(&db, "Agents/Master Chief/Notes").unwrap().is_none());
    memory::ensure_lead_notes(&db, "lead", "Jeffrey").unwrap();
    let lead = Who::of(&db, "lead").unwrap();
    assert_eq!(lead, Who::Lead("lead".into()));
    let rust = memory::write(&db, &lead, "Standards/Rust style", "Use thiserror.", None, None).unwrap();

    // a project's doc list shows only its docs, never memory notes
    assert_eq!(docs::list(&db, "p").unwrap().into_iter().map(|d| d.title).collect::<Vec<_>>(), ["Requirements"]);
    // a link in an old doc finds the note once it exists, and a save keeps the doc as it is
    assert_eq!(memory::links_from(&db, "d1").unwrap(), [memory::LinkRow { target_type: "doc".into(), target_id: rust.id.clone(), kind: "link".into() }]);
    assert_eq!(docs::save(&db, "you", "d1", "# Requirements\nNo links.", 2).unwrap(), 3);
    assert!(memory::links_from(&db, "d1").unwrap().is_empty());
    assert_eq!(docs::get(&db, "d1").unwrap().kind, "doc");
}

// ---- The access rule ----

#[test]
fn the_team_lead_and_people_read_and_write_everything_an_agent_reads_shared_notes_and_its_own_and_writes_only_its_own() {
    let f = setup();
    assert_eq!(Who::of(&f.db, &f.lead).unwrap(), f.lead());
    assert_eq!(Who::of(&f.db, &f.you).unwrap(), f.you());
    assert_eq!(Who::of(&f.db, &f.be).unwrap(), f.be());
    memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap();
    f.write("Standards/Rust style", "Use thiserror.");
    let get = |p: &str| memory::get(&f.db, &f.lead(), p).unwrap();
    let (shared, lead_notes, be_notes, qa_notes) =
        (get("Standards/Rust style"), get("Team Lead/Notes"), get("Agents/Backend Agent/Notes"), get("Agents/QA Agent/Notes"));
    assert_eq!((shared.scope.as_str(), lead_notes.scope.as_str(), be_notes.scope.as_str()), ("shared", "agent", "agent"));
    assert_eq!((lead_notes.owner_id.as_deref(), be_notes.owner_id.as_deref()), (Some(f.lead.as_str()), Some(f.be.as_str())));

    for who in [f.lead(), f.you()] {
        for n in [&shared, &lead_notes, &be_notes, &qa_notes] {
            assert!(memory::can_read(&who, n) && memory::can_write(&who, n), "{who:?} on {}", n.path);
        }
    }
    let be = f.be();
    assert!(memory::can_read(&be, &shared) && !memory::can_write(&be, &shared), "shared: read, not write");
    assert!(memory::can_read(&be, &be_notes) && memory::can_write(&be, &be_notes), "its own folder: read and write");
    for n in [&lead_notes, &qa_notes] {
        assert!(!memory::can_read(&be, n) && !memory::can_write(&be, n), "{} is not the Backend Agent's", n.path);
    }

    // what each one sees
    assert_eq!(f.paths(&f.be()), ["Agents/Backend Agent/Notes", "Standards/Rust style"]);
    assert_eq!(f.paths(&f.lead()), ["Agents/Backend Agent/Notes", "Agents/QA Agent/Notes", "Standards/Rust style", "Team Lead/Notes"]);
    assert_eq!(f.paths(&f.you()), f.paths(&f.lead()));
    assert!(memory::get(&f.db, &f.be(), "Agents/QA Agent/Notes").unwrap_err().to_string().contains("not found"), "another agent's notes are not there for it");
    assert!(memory::get(&f.db, &f.be(), "Team Lead/Notes").is_err());

    // the same rule on writes: an agent writes only in its own folder, and the error says what to do
    let e = err(memory::write(&f.db, &f.be(), "Standards/Go style", "x", None, None));
    assert!(e.contains("an agent writes only in its own folder"), "{e}");
    let e = err(memory::write(&f.db, &f.be(), "Standards/Rust style", "x", Some(1), None));
    assert!(e.contains("own folder"), "{e}");
    assert!(memory::write(&f.db, &f.be(), "Agents/QA Agent/Notes", "x", Some(1), None).is_err());
    assert!(memory::write(&f.db, &f.be(), "Agents/QA Agent/Gotchas", "x", None, None).is_err());
    assert!(memory::write(&f.db, &f.be(), "Team Lead/Ideas", "x", None, None).is_err());
    assert!(memory::append(&f.db, &f.be(), "Standards/Rust style", None, "x", None).is_err());
    let mine = memory::write(&f.db, &f.be(), "agents/backend agent/Gotchas", "Run cargo with -j 8.", None, None).unwrap();
    assert_eq!((mine.path.as_str(), mine.created, mine.version), ("Agents/Backend Agent/Gotchas", true, 1));
    let n = memory::get(&f.db, &f.be(), &mine.id).unwrap();
    assert_eq!((n.scope.as_str(), n.owner_id.as_deref()), ("agent", Some(f.be.as_str())));
    assert!(memory::get(&f.db, &f.qa(), &mine.id).is_err(), "the QA Agent can't see it");
    // the Team Lead and people write in an agent's folder too
    assert!(memory::append(&f.db, &f.lead(), "Agents/Backend Agent/Gotchas", None, "And -j 4 on a laptop.", None).is_ok());
    assert!(memory::append(&f.db, &f.you(), "Agents/QA Agent/Notes", Some("Learned"), "Check main first.", None).is_ok());
    assert!(memory::write(&f.db, &f.you(), "Decisions/Memory in the database", "Because backups.", None, None).is_ok());
}

#[test]
fn paths_are_made_tidy_and_a_path_outside_the_memory_folders_is_refused_with_what_to_write() {
    let f = setup();
    let s = memory::write(&f.db, &f.lead(), " /standards/ Rust   style.md ", "x", None, None).unwrap();
    assert_eq!(s.path, "Standards/Rust style");
    let s = memory::write(&f.db, &f.lead(), "team lead/Ideas", "x", None, None).unwrap();
    assert_eq!(s.path, "Team Lead/Ideas");
    let e = err(memory::write(&f.db, &f.lead(), "Rust style", "x", None, None));
    assert!(e.contains("give the note a folder and a title, like Standards/Rust style"), "{e}");
    let e = err(memory::write(&f.db, &f.lead(), "Random/Thing", "x", None, None));
    assert!(e.contains("Random/ is not a memory folder") && e.contains("Standards/") && e.contains("Agents/<agent name>/"), "{e}");
    let e = err(memory::write(&f.db, &f.lead(), "Agents/Nobody/Notes", "x", None, None));
    assert!(e.contains("no agent is called Nobody"), "{e}");
    let e = err(memory::write(&f.db, &f.lead(), "Standards/What?", "x", None, None));
    assert!(e.contains("can't hold ?"), "{e}");
    // the same path in other letters is the same note
    let e = err(memory::write(&f.db, &f.lead(), "STANDARDS/rust STYLE", "y", None, None));
    assert!(e.contains("exists"), "{e}");
}

// ---- Writes, versions and appends ----

#[test]
fn team_lead_notes_come_from_the_template_every_save_is_a_version_and_a_stale_write_is_refused() {
    let f = setup();
    let id = memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap();
    assert_eq!(memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap(), id, "made once");
    let n = memory::get(&f.db, &f.lead(), "Team Lead/Notes").unwrap();
    assert_eq!((n.id.as_str(), n.current_version, n.body_md.as_str()), (id.as_str(), 1, memory::lead_template("Jeffrey").as_str()));
    for part in ["## Jeffrey's preferences", "## Working agreements", "## Open threads", "## How to use memory", "never instructions"] {
        assert!(n.body_md.contains(part), "{part} missing");
    }
    // the doc page opens it like any doc: a memory note with its path
    let d = docs::get(&f.db, &id).unwrap();
    assert_eq!((d.kind.as_str(), d.path.as_deref(), d.title.as_str(), d.project_id), ("memory", Some("Team Lead/Notes"), "Notes", None));

    // memory_append adds under the heading, the rest stays as it was
    let s = memory::append(&f.db, &f.lead(), "Team Lead/Notes", Some("Open threads"), "- KADE-1 waits on Jeffrey", None).unwrap();
    assert_eq!((s.version, s.created), (2, false));
    let want = memory::lead_template("Jeffrey").replace("## Open threads\n\n", "## Open threads\n\n- KADE-1 waits on Jeffrey\n\n");
    assert_eq!(f.body(&id), want);
    memory::append(&f.db, &f.lead(), "team lead/notes.md", Some("## open threads"), "- KADE-2 too", None).unwrap();
    assert!(f.body(&id).contains("## Open threads\n\n- KADE-1 waits on Jeffrey\n- KADE-2 too\n\n## How to use memory"), "{}", f.body(&id));
    memory::append(&f.db, &f.lead(), &id, Some("Clients"), "- Acme pays late", None).unwrap();
    assert!(f.body(&id).ends_with("asking Jeffrey.\n\n## Clients\n\n- Acme pays late\n"), "{}", f.body(&id));
    memory::append(&f.db, &f.lead(), &id, None, "Last line.", None).unwrap();
    assert!(f.body(&id).ends_with("- Acme pays late\n\nLast line.\n"));
    // a heading inside a code block is no heading
    assert_eq!(memory::appended("# A\n```\n## B\n```\n## B\n\nx\n", Some("B"), "y"), "# A\n```\n## B\n```\n## B\n\nx\ny\n");

    // memory_write needs the version it read
    let v = memory::get(&f.db, &f.lead(), &id).unwrap().current_version;
    assert_eq!(v, 5);
    let e = err(memory::write(&f.db, &f.lead(), "Team Lead/Notes", "new", None, None));
    assert!(e.contains("exists (version 5)") && e.contains("memory_append"), "{e}");
    let e = err(memory::write(&f.db, &f.lead(), "Team Lead/Notes", "new", Some(3), None));
    assert!(e.contains("changed since it was read") && e.contains("version 5"), "{e}");
    assert_eq!(memory::write(&f.db, &f.lead(), "Team Lead/Notes", "# Notes\nnew", Some(5), None).unwrap().version, 6);
    // and so does an edit in the doc page; every save is a version, with its author
    assert!(docs::save(&f.db, &f.you, &id, "stale", 5).is_err());
    assert_eq!(docs::save(&f.db, &f.you, &id, "# Notes\nEdited by hand.", 6).unwrap(), 7);
    let versions = docs::versions(&f.db, &id).unwrap();
    assert_eq!(versions.len(), 7);
    let mut authors: Vec<_> = versions.iter().map(|v| (v.version, v.author_name.clone().unwrap_or_default())).collect();
    authors.sort();
    assert_eq!(authors.first().unwrap(), &(1, "Team Lead".to_string()));
    assert_eq!(authors.last().unwrap(), &(7, "Jeffrey".to_string()));
    assert_eq!(docs::version_body(&f.db, &id, 2).unwrap(), want);
    assert_eq!(memory::get(&f.db, &f.lead(), &id).unwrap().updated_by.as_deref(), Some("Jeffrey"));
}

// ---- Links and backlinks ----

#[test]
fn wikilinks_embeds_task_references_and_mentions_land_in_doc_links_and_backlinks_find_the_notes_that_link() {
    let f = setup();
    let p = f.project("Kade", "KADE", None);
    let task = tasks::create(&f.db, &f.you, TaskInput { project_id: p, title: "Export".into(), ..Default::default() }).unwrap();
    assert_eq!(tasks::get(&f.db, &task).unwrap().identifier, "KADE-1");
    let handle = team::agent(&f.db, &f.be).unwrap().handle;
    let rust = f.write("Standards/Rust style", "# Rust style\n\n## Errors\n\nUse thiserror.");
    let decision = f.write("Decisions/Memory in the database", "Because backups.");
    let wf = f.write("Workflows/Deploy", "Tag it.");
    let dep = f.write("Deployments/Deploy", "Production.");
    let body = format!("See [[rust style]], ![[Decisions/Memory in the database]], [[Memory in the database#Why|why]], \
        [[Deployments/Deploy]], KADE-1 (not KADE-99, not xKADE-1), @{handle}, @jeffrey and t@example.com, [[Nowhere]] and [[#Local]].");
    let gotchas = f.write("Lessons/Gotchas", &body);

    let links = memory::links_from(&f.db, &gotchas).unwrap();
    let mut want = vec![("doc", rust.clone(), "link"), ("doc", decision.clone(), "embed"), ("doc", decision.clone(), "link"), ("doc", dep.clone(), "link"),
                        ("task", task.clone(), "link"), ("actor", f.be.clone(), "mention"), ("actor", f.you.clone(), "mention")];
    want.sort_by(|a, b| (a.0, &a.1, a.2).cmp(&(b.0, &b.1, b.2)));
    let got: Vec<(String, String, String)> = links.into_iter().map(|l| (l.target_type, l.target_id, l.kind)).collect();
    let want: Vec<(String, String, String)> = want.into_iter().map(|(a, b, c)| (a.to_string(), b, c.to_string())).collect();
    assert_eq!(got, want, "[[Note]], ![[Note]], [[Note#Heading|text]], a path that picks one of two, KADE-1 and @mentions; nothing for a missing note");
    assert!(!got.iter().any(|l| l.1 == wf), "[[Deployments/Deploy]] is not Workflows/Deploy");

    // backlinks: the notes that link to a note
    let back = |r: &str| memory::backlinks(&f.db, &f.lead(), r).unwrap().into_iter().map(|n| n.path).collect::<Vec<_>>();
    assert_eq!(back("Standards/Rust style"), ["Lessons/Gotchas"]);
    assert_eq!(back(&decision), ["Lessons/Gotchas"]);
    assert!(back("Workflows/Deploy").is_empty());
    f.write("Lessons/More", "Also [[Rust style#Errors]].");
    assert_eq!(back(&rust), ["Lessons/Gotchas", "Lessons/More"]);
    // an agent sees only the backlinks it may read
    memory::append(&f.db, &f.lead(), "Agents/QA Agent/Notes", None, "Mind [[Rust style]].", None).unwrap();
    assert_eq!(back(&rust).len(), 3);
    assert_eq!(memory::backlinks(&f.db, &f.be(), &rust).unwrap().len(), 2);

    // a note made later is found by the links that named it
    let nowhere = f.write("Workflows/Nowhere", "Now it exists.");
    assert!(memory::links_from(&f.db, &gotchas).unwrap().iter().any(|l| l.target_id == nowhere));
    // every save rebuilds the links
    let v = memory::get(&f.db, &f.lead(), &gotchas).unwrap().current_version;
    memory::write(&f.db, &f.lead(), "Lessons/Gotchas", "No links left.", Some(v), None).unwrap();
    assert!(memory::links_from(&f.db, &gotchas).unwrap().is_empty());
    assert_eq!(back(&rust), ["Agents/QA Agent/Notes", "Lessons/More"]);
}

#[test]
fn renaming_or_moving_a_note_rewrites_the_links_that_point_to_it() {
    let f = setup();
    let p = f.project("Kade", "KADE", None);
    let rust = f.write("Standards/Rust style", "# Rust style\n\n## Errors\n\nUse thiserror.");
    let gotchas = f.write("Lessons/Gotchas", "See [[Rust style]], [[Standards/Rust style#Errors|errors]] and ![[rust style]]; not [[Other]].\n");
    let readme = docs::create(&f.db, &f.you, &p, "Readme").unwrap();
    docs::save(&f.db, &f.you, &readme, "Our rules: [[Rust style]].", 1).unwrap();

    // the doc page's rename keeps the folder and moves the note
    docs::rename(&f.db, &f.you, &rust, "Rust rules").unwrap();
    let n = memory::get(&f.db, &f.lead(), &rust).unwrap();
    assert_eq!(n.path, "Standards/Rust rules");
    assert_eq!(docs::get(&f.db, &rust).unwrap().title, "Rust rules");
    assert_eq!(f.body(&gotchas), "See [[Rust rules]], [[Standards/Rust rules#Errors|errors]] and ![[Rust rules]]; not [[Other]].\n");
    assert_eq!(memory::get(&f.db, &f.lead(), &gotchas).unwrap().current_version, 2, "the rewrite is a version");
    assert_eq!(docs::get(&f.db, &readme).unwrap().body_md, "Our rules: [[Rust rules]].", "a project doc's link follows too");
    assert_eq!(memory::backlinks(&f.db, &f.lead(), &rust).unwrap().into_iter().map(|n| n.path).collect::<Vec<_>>(), ["Lessons/Gotchas"]);
    assert!(memory::find(&f.db, "Standards/Rust style").unwrap().is_none());

    // a move to another folder
    let moved = memory::move_note(&f.db, &f.lead(), "Standards/Rust rules", "Decisions/", false).unwrap();
    assert_eq!((moved.id.as_str(), moved.path.as_str(), moved.scope.as_str()), (rust.as_str(), "Decisions/Rust rules", "shared"));
    assert_eq!(f.body(&gotchas), "See [[Rust rules]], [[Decisions/Rust rules#Errors|errors]] and ![[Rust rules]]; not [[Other]].\n");
    assert_eq!(memory::links_from(&f.db, &gotchas).unwrap().iter().filter(|l| l.target_id == rust).count(), 2, "a link and an embed");
    // and to a new title: a link by title takes the new one
    memory::move_note(&f.db, &f.lead(), &rust, "Standards/Coding rules", false).unwrap();
    assert_eq!(f.body(&gotchas), "See [[Coding rules]], [[Standards/Coding rules#Errors|errors]] and ![[Coding rules]]; not [[Other]].\n");
    assert_eq!(docs::get(&f.db, &readme).unwrap().body_md, "Our rules: [[Coding rules]].");
    // a move onto a note that exists is refused, and nothing moves
    f.write("Lessons/Taken", "x");
    let e = err(memory::move_note(&f.db, &f.lead(), &rust, "Lessons/Taken", false));
    assert!(e.contains("exists already"), "{e}");
    assert_eq!(memory::get(&f.db, &f.lead(), &rust).unwrap().path, "Standards/Coding rules");
}

// ---- Search ----

#[test]
fn search_finds_words_quoted_phrases_path_and_tag_filters_and_only_notes_the_asker_may_read() {
    let f = setup();
    memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap();
    memory::append(&f.db, &f.lead(), "Team Lead/Notes", Some("Working agreements"), "- Jeffrey reviews on Fridays.", None).unwrap();
    f.write("Standards/Rust style", "---\ntype: standard\ntags: [rust, style]\n---\nUse thiserror for errors. Never unwrap in library code.");
    f.write("Decisions/Memory in the database", "We keep memory in SQLite, not in files. #decision");
    f.write("Lessons/Builds", "rust rust rust: cargo needs -j 8 here.");
    let paths = |who: &Who, q: &str| memory::search(&f.db, who, q, 20).unwrap().into_iter().map(|h| h.note.path).collect::<Vec<_>>();

    let hits = memory::search(&f.db, &f.lead(), "THISERROR", 20).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].note.path.as_str(), hits[0].snippet.as_str()), ("Standards/Rust style", "Use thiserror for errors. Never unwrap in library code."));
    assert!(hits[0].note.body_md.is_empty(), "a hit has no text, only the line");
    assert_eq!(paths(&f.lead(), "\"not in files\""), ["Decisions/Memory in the database"]);
    assert!(paths(&f.lead(), "\"in files not\"").is_empty(), "a phrase is matched as a whole");
    assert_eq!(paths(&f.lead(), "rust"), ["Standards/Rust style", "Lessons/Builds"], "a title match first");
    assert_eq!(paths(&f.lead(), "path:standards unwrap"), ["Standards/Rust style"]);
    assert_eq!(paths(&f.lead(), "path:Lessons rust"), ["Lessons/Builds"]);
    assert_eq!(paths(&f.lead(), "path:\"Team Lead\" fridays"), ["Team Lead/Notes"]);
    assert_eq!(paths(&f.lead(), "tag:rust"), ["Standards/Rust style"]);
    assert_eq!(paths(&f.lead(), "tag:#decision"), ["Decisions/Memory in the database"]);
    assert!(paths(&f.lead(), "tag:rust sqlite").is_empty());
    assert!(memory::search(&f.db, &f.lead(), "  ", 20).is_err());
    // an agent finds only what it may read
    assert!(paths(&f.be(), "fridays").is_empty(), "the Team Lead's notes are not the Backend Agent's");
    assert_eq!(paths(&f.be(), "thiserror"), ["Standards/Rust style"]);
    // the search stays right after a move
    memory::move_note(&f.db, &f.lead(), "Lessons/Builds", "Workflows/", false).unwrap();
    assert_eq!(paths(&f.lead(), "path:Workflows cargo"), ["Workflows/Builds"]);
    assert!(paths(&f.lead(), "path:Lessons cargo").is_empty());
}

// ---- Safety ----

#[test]
fn a_write_with_a_private_key_or_an_api_token_is_refused_with_a_reason_the_model_can_act_on() {
    let f = setup();
    let id = f.write("Deployments/Acme", "The deploy key is in the keychain as acme-deploy.");
    let secrets = [
        ("-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAA\n-----END OPENSSH PRIVATE KEY-----", "a private key"),
        ("-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----", "a private key"),
        ("key: sk-proj-abcdefghijklmnopqrstuvwxyz123456", "an API key (sk-"),
        ("token ghp_abcdefghijklmnopqrstuvwxyz0123456789", "a GitHub token (ghp_"),
        ("aws AKIAIOSFODNN7EXAMPLE here", "an AWS access key (AKIA"),
        ("github_pat_11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyz", "a GitHub token (github_pat_"),
    ];
    for (secret, what) in secrets {
        assert!(memory::secret_in(secret).is_some_and(|w| w.starts_with(what)), "{secret}: {:?}", memory::secret_in(secret));
        let text = format!("# Acme\n\n{secret}\n");
        for e in [err(memory::write(&f.db, &f.lead(), "Deployments/Keys", &text, None, None)),
                  err(memory::write(&f.db, &f.lead(), "Deployments/Acme", &text, Some(1), None)),
                  err(memory::append(&f.db, &f.lead(), "Deployments/Acme", None, secret, None)),
                  err(memory::append(&f.db, &f.be(), "Agents/Backend Agent/Notes", Some("Learned"), secret, None)),
                  err(docs::save(&f.db, &f.you, &id, &text, 1))] {
            assert!(e.contains("Memory doesn't keep secrets") && e.contains(what) && e.contains("Nothing was saved")
                    && e.contains("Take the secret out") && e.contains("where it is kept"), "{e}");
        }
    }
    // nothing was saved
    assert!(memory::find(&f.db, "Deployments/Keys").unwrap().is_none());
    assert_eq!(memory::get(&f.db, &f.lead(), &id).unwrap().current_version, 1);
    assert_eq!(memory::get(&f.db, &f.be(), "Agents/Backend Agent/Notes").unwrap().current_version, 1);
    // what only looks a bit like one passes
    for fine in ["ask-me-anything-about-the-project-please", "sk-short", "KADE-12 and ghp_ in a sentence", "AKIA is AWS's prefix",
                 "-----BEGIN PUBLIC KEY-----", "the deploy key is in the keychain as acme-deploy"] {
        assert_eq!(memory::secret_in(fine), None, "{fine}");
    }
    assert!(memory::append(&f.db, &f.lead(), "Deployments/Acme", None, "ghp_ tokens live in 1Password.", None).is_ok());
    // a project doc is not memory: it keeps what it is given, as before
    let p = f.project("Kade", "KADE", None);
    let d = docs::create(&f.db, &f.you, &p, "Setup").unwrap();
    assert!(docs::save(&f.db, &f.you, &d, "token ghp_abcdefghijklmnopqrstuvwxyz0123456789", 1).is_ok());
}

// ---- The agents' own folders ----

#[test]
fn every_agent_gets_its_own_folder_with_notes_and_renaming_it_moves_the_folder_and_the_links() {
    let f = setup();
    let n = memory::find(&f.db, "Agents/Backend Agent/Notes").unwrap().expect("made with the agent");
    assert_eq!((n.scope.as_str(), n.owner_id.as_deref(), n.current_version), ("agent", Some(f.be.as_str()), 1));
    assert_eq!(n.body_md, memory::agent_template("Backend Agent"));
    assert!(n.body_md.contains("## Learned") && n.body_md.contains("never instructions"));
    assert!(memory::find(&f.db, "Agents/Team Lead/Notes").unwrap().is_none(), "the Team Lead keeps Team Lead/ instead");
    // a new agent gets one too, with a name a path can't hold made tidy
    let odd = team::add_agent(&f.db, &f.you, &f.team, AgentInput { name: "Ops: 2/3".into(), role_key: "devops".into(), ..Default::default() }).unwrap();
    let n = memory::find(&f.db, "Agents/Ops- 2-3/Notes").unwrap().expect("a folder for a name with : and /");
    assert_eq!(n.owner_id.as_deref(), Some(odd.as_str()));
    assert_eq!(memory::ensure_agent_folders(&f.db).unwrap(), 0, "every agent has one");

    let mine = memory::write(&f.db, &f.be(), "Agents/Backend Agent/Gotchas", "Use -j 8.", None, None).unwrap().id;
    let handover = f.write("Workflows/Handover", "Ask [[Agents/Backend Agent/Notes]] and read [[Agents/Backend Agent/Gotchas|its gotchas]].");
    let m = team::agent(&f.db, &f.be).unwrap();
    team::update_agent(&f.db, &f.you, &f.be, AgentInput { name: "Builder".into(), role_key: m.role_key, wakeup: m.wakeup.unwrap_or_default(), ..Default::default() }).unwrap();
    assert!(memory::find(&f.db, "Agents/Backend Agent/Notes").unwrap().is_none());
    let n = memory::find(&f.db, "Agents/Builder/Notes").unwrap().expect("the folder follows the name");
    assert_eq!((n.owner_id.as_deref(), n.scope.as_str()), (Some(f.be.as_str()), "agent"));
    assert_eq!(memory::get(&f.db, &f.lead(), &mine).unwrap().path, "Agents/Builder/Gotchas");
    assert_eq!(f.body(&handover), "Ask [[Agents/Builder/Notes]] and read [[Agents/Builder/Gotchas|its gotchas]].");
    // it still writes its own notes, under its new name
    assert!(memory::append(&f.db, &f.be(), "Agents/Builder/Notes", Some("Learned"), "- renamed", None).is_ok());
    assert!(memory::write(&f.db, &f.be(), "Agents/Builder/Ideas", "x", None, None).is_ok());
    assert_eq!(f.paths(&f.be()).iter().filter(|p| p.starts_with("Agents/")).cloned().collect::<Vec<_>>(),
               ["Agents/Builder/Gotchas", "Agents/Builder/Ideas", "Agents/Builder/Notes"]);
}

#[test]
fn learned_lines_become_dated_bullets_in_the_agents_own_notes_with_the_run_on_the_version() {
    let f = setup();
    let p = f.project("Kade", "KADE", None);
    let task = tasks::create(&f.db, &f.you, TaskInput { project_id: p, title: "Export".into(), ..Default::default() }).unwrap();
    let run = runs::create(&f.db, &f.be, &task, "backend", "S", "/tmp", "/tmp", "gizai/x", "/tmp/r.jsonl").unwrap();
    let lines = vec!["Use the fake CLI in tests.".to_string(), "   ".into(), "- The token is ghp_abcdefghijklmnopqrstuvwxyz0123".into(),
                     "  * Run  cargo with\n -j 8. ".into()];
    let (v, left_out) = memory::learned(&f.db, &f.be, "KADE-1", &lines, Some(&run), "2026-10-09").unwrap();
    assert_eq!(v, Some(2));
    assert_eq!(left_out, ["a learned line with a GitHub token (ghp_…) was not saved"]);
    let n = memory::get(&f.db, &f.be(), "Agents/Backend Agent/Notes").unwrap();
    assert!(n.body_md.ends_with("## Learned\n\n- 2026-10-09 (KADE-1): Use the fake CLI in tests.\n- 2026-10-09 (KADE-1): Run cargo with -j 8.\n"), "{}", n.body_md);
    assert!(!n.body_md.contains("ghp_"));
    assert_eq!(n.updated_by.as_deref(), Some("Backend Agent"), "written by the agent");
    let (author, run_on): (String, Option<String>) = f.db.read(|c| Ok(c.query_row(
        "SELECT author_actor_id, run_id FROM doc_versions WHERE doc_id = ?1 AND version = 2", [&n.id], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert_eq!((author.as_str(), run_on.as_deref()), (f.be.as_str(), Some(run.as_str())));
    // a later run adds below; a long line is cut; nothing to add saves nothing
    memory::learned(&f.db, &f.be, "KADE-2", &["x".repeat(400)], None, "2026-10-10").unwrap();
    let body = memory::get(&f.db, &f.be(), &n.id).unwrap().body_md;
    assert!(body.contains("-j 8.\n- 2026-10-10 (KADE-2): ") && body.trim_end().ends_with('…'), "{body}");
    assert!(body.lines().last().unwrap().chars().count() < 340);
    assert_eq!(memory::learned(&f.db, &f.be, "KADE-3", &["  ".into()], None, "2026-10-10").unwrap(), (None, vec![]));
    assert_eq!(memory::get(&f.db, &f.be(), &n.id).unwrap().current_version, 3);
    // the Team Lead's go into Team Lead/Notes
    memory::learned(&f.db, &f.lead, "KADE-1", &["Jeffrey wants short summaries.".into()], None, "2026-10-09").unwrap();
    assert!(memory::get(&f.db, &f.lead(), "Team Lead/Notes").unwrap().body_md.contains("## Learned\n\n- 2026-10-09 (KADE-1): Jeffrey wants short summaries.\n"));
}

#[test]
fn the_team_lead_promotes_an_agents_note_into_a_shared_folder_and_agents_cant_write_there() {
    let f = setup();
    let mine = memory::write(&f.db, &f.be(), "Agents/Backend Agent/Gotchas", "Use -j 8 with cargo.", None, None).unwrap().id;
    let links = memory::write(&f.db, &f.be(), "Agents/Backend Agent/Index", "See [[Agents/Backend Agent/Gotchas]].", None, None).unwrap().id;
    // an agent can't move or copy its notes into a shared folder
    for copy in [false, true] {
        let e = err(memory::move_note(&f.db, &f.be(), &mine, "Standards/", copy));
        assert!(e.contains("an agent writes only in its own folder"), "{e}");
    }
    assert!(memory::find(&f.db, "Standards/Gotchas").unwrap().is_none());
    // nor into another agent's folder; inside its own it may
    assert!(memory::move_note(&f.db, &f.be(), &mine, "Agents/QA Agent/", false).is_err());
    assert_eq!(memory::move_note(&f.db, &f.be(), &mine, "Agents/Backend Agent/Cargo gotchas", false).unwrap().path, "Agents/Backend Agent/Cargo gotchas");

    // the Team Lead promotes it: shared now, no owner, the links follow
    let n = memory::move_note(&f.db, &f.lead(), &mine, "Standards/", false).unwrap();
    assert_eq!((n.path.as_str(), n.scope.as_str(), n.owner_id.as_deref()), ("Standards/Cargo gotchas", "shared", None));
    assert_eq!(memory::get(&f.db, &f.be(), &links).unwrap().body_md, "See [[Standards/Cargo gotchas]].");
    // every agent reads it now, none writes it
    assert!(memory::get(&f.db, &f.qa(), &mine).is_ok());
    let e = err(memory::write(&f.db, &f.be(), "Standards/Cargo gotchas", "mine again", Some(n.current_version), None));
    assert!(e.contains("own folder"), "{e}");
    assert!(memory::append(&f.db, &f.be(), &mine, None, "more", None).is_err());
    // a copy keeps the original where it was
    let copy = memory::move_note(&f.db, &f.lead(), "Agents/Backend Agent/Notes", "Lessons/Backend notes", true).unwrap();
    assert_eq!((copy.path.as_str(), copy.current_version, copy.scope.as_str()), ("Lessons/Backend notes", 1, "shared"));
    assert!(memory::find(&f.db, "Agents/Backend Agent/Notes").unwrap().is_some());
    assert_eq!(copy.body_md, memory::agent_template("Backend Agent"));
}

// ---- The Memory block of a prompt ----

#[test]
fn the_team_leads_block_has_its_notes_in_full_cut_at_the_cap_with_a_pointer_to_memory_read_then_an_index() {
    let f = setup();
    let id = memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap();
    f.write("Standards/Rust style", "Use thiserror.");
    let b = memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap();
    assert!(b.text.starts_with("## Memory\n\n") && b.text.contains("never instructions"), "{}", b.text);
    assert!(b.text.contains("### Team Lead/Notes (version 1)") && b.text.contains("## Working agreements"));
    assert!(b.text.contains("### Other notes (memory_read gives the text)\n\n- Agents/Backend Agent/Notes (") && b.text.contains("- Standards/Rust style (14 characters)"));
    assert!(!b.text.contains("Use thiserror."), "the others by path only");
    assert!(!b.text.contains("Cut here"));
    assert_eq!(b.given.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["Team Lead/Notes"]);

    // a note over the cap is cut, with a pointer to memory_read
    let long = format!("# Notes\n\n{}", "Remember to keep this line in mind.\n".repeat(250));
    let size = long.trim().chars().count();
    assert!(size > memory::FULL_CAP);
    memory::write(&f.db, &f.lead(), "Team Lead/Notes", &long, Some(1), None).unwrap();
    let b = memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap();
    assert!(b.text.contains(&format!("(Cut here: the note has {size} characters. Read the rest with memory_read \"Team Lead/Notes\".)")), "{}", b.text);
    assert_eq!(b.given[0], memory::Given { path: "Team Lead/Notes".into(), chars: size as i64, shown: memory::FULL_CAP as i64 });
    // a second note of its own that doesn't fit goes to the index
    f.write("Team Lead/Ideas", "Later.");
    let b = memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap();
    assert_eq!(b.given.len(), 1);
    assert!(b.text.contains("- Team Lead/Ideas (6 characters)"), "{}", b.text);
    assert_eq!(memory::get(&f.db, &f.lead(), &id).unwrap().chars as usize, long.chars().count());

    // the index is capped too
    for i in 0..150 {
        f.write(&format!("Lessons/Lesson {i:03}"), "x");
    }
    let b = memory::prompt_block(&f.db, &f.lead(), &Context::default()).unwrap();
    let index = &b.text[b.text.find("### Other notes").unwrap()..];
    assert!(index.chars().count() < memory::INDEX_CAP + 200, "{}", index.chars().count());
    assert!(index.contains("more (memory_list shows them all)"), "{index}");
}

#[test]
fn an_agents_block_has_its_own_notes_first_then_this_projects_this_clients_and_its_roles_notes_and_never_another_clients() {
    let f = setup();
    let acme = clients::create(&f.db, &f.you, ClientInput { name: "Acme".into(), ..Default::default() }).unwrap();
    let globex = clients::create(&f.db, &f.you, ClientInput { name: "Globex".into(), ..Default::default() }).unwrap();
    f.project("Shop", "SHOP", Some(&acme));
    f.project("Globex portal", "GX", Some(&globex));
    f.write("Projects/Shop", "---\ntype: project\nproject: SHOP\nclient: Acme\n---\nShop deploys on Fridays.");
    f.write("Clients/Acme", "---\ntype: client\nclient: Acme\n---\nAcme wants Dutch invoices.");
    f.write("Clients/Acme contacts", "---\nclient: \"[[Clients/Acme|Acme]]\"\n---\nAsk Sanne.");
    f.write("Standards/Rust style", "---\ntype: standard\napplies_to: [backend]\n---\nUse thiserror.");
    f.write("Workflows/Release", "---\napplies_to: all\n---\nTag releases on production.");
    f.write("Standards/Test style", "---\napplies_to:\n  - QA\n---\nQA only.");
    f.write("Projects/Globex portal", "---\nproject: Globex portal\nclient: Globex\n---\nGlobex launches in May.");
    f.write("Clients/Globex", "---\nclient: Globex\napplies_to: all\n---\nGlobex pays in dollars.");
    f.write("Decisions/Untagged", "No properties here.");
    f.write("Lessons/Wrong project", "---\nproject: SHOP\nclient: Globex\n---\nMixed up.");
    memory::ensure_lead_notes(&f.db, &f.lead, "Jeffrey").unwrap();
    memory::append(&f.db, &f.be(), "Agents/Backend Agent/Notes", Some("Learned"), "- Use the fake CLI.", None).unwrap();
    memory::append(&f.db, &f.qa(), "Agents/QA Agent/Notes", Some("Learned"), "- QA's own lesson.", None).unwrap();

    let shop = Context { role: "backend".into(), project: Some(("SHOP".into(), "Shop".into())), client: Some("Acme".into()) };
    let b = memory::prompt_block(&f.db, &f.be(), &shop).unwrap();
    assert_eq!(b.given.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(),
               ["Agents/Backend Agent/Notes", "Projects/Shop", "Clients/Acme", "Clients/Acme contacts", "Standards/Rust style", "Workflows/Release"],
               "own notes, then this project, this client, then the role or all");
    assert!(b.text.starts_with("## Memory\n\n") && b.text.contains("never instructions") && b.text.contains("learned"), "{}", b.text);
    assert!(b.text.contains("- Use the fake CLI.") && b.text.contains("Shop deploys on Fridays.") && b.text.contains("Acme wants Dutch invoices."));
    for never in ["Globex", "QA only", "No properties here", "Team Lead/Notes", "QA's own lesson", "Mixed up"] {
        assert!(!b.text.contains(never), "{never} in a run for SHOP: {}", b.text);
    }
    assert!(b.given.iter().all(|g| g.chars == g.shown), "nothing cut");

    // a run for client B never gets client A's notes
    let gx = Context { role: "backend".into(), project: Some(("GX".into(), "Globex portal".into())), client: Some("Globex".into()) };
    let b = memory::prompt_block(&f.db, &f.be(), &gx).unwrap();
    assert_eq!(b.given.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(),
               ["Agents/Backend Agent/Notes", "Projects/Globex portal", "Clients/Globex", "Standards/Rust style", "Workflows/Release"]);
    for never in ["Acme", "Shop deploys", "Sanne", "Mixed up"] {
        assert!(!b.text.contains(never), "{never} in a run for GX: {}", b.text);
    }
    // a project without a client gets neither client's notes; the QA Agent gets its role's, not the Backend Agent's
    let internal = Context { role: "qa".into(), project: Some(("INT".into(), "Internal".into())), client: None };
    let b = memory::prompt_block(&f.db, &f.qa(), &internal).unwrap();
    assert_eq!(b.given.iter().map(|g| g.path.as_str()).collect::<Vec<_>>(), ["Agents/QA Agent/Notes", "Standards/Test style", "Workflows/Release"]);

    // capped at FULL_CAP in total: a note that doesn't fit is cut, the rest are listed by path
    let v = memory::get(&f.db, &f.be(), "Agents/Backend Agent/Notes").unwrap().current_version;
    memory::write(&f.db, &f.be(), "Agents/Backend Agent/Notes", &"Own lesson, kept for later.\n".repeat(180), Some(v), None).unwrap();
    memory::write(&f.db, &f.lead(), "Projects/Shop", &format!("---\nproject: SHOP\n---\n{}", "Shop deploys on Fridays.\n".repeat(80)), Some(1), None).unwrap();
    let b = memory::prompt_block(&f.db, &f.be(), &shop).unwrap();
    let shown: i64 = b.given.iter().map(|g| g.shown).sum();
    assert!(shown <= memory::FULL_CAP as i64, "{shown}");
    assert_eq!(b.given[0].path, "Agents/Backend Agent/Notes");
    assert_eq!(b.given[0].shown, b.given[0].chars, "its own notes in full (5,000 characters)");
    assert_eq!(b.given[1].path, "Projects/Shop");
    assert!(b.given[1].shown < b.given[1].chars, "cut: {:?}", b.given[1]);
    assert!(b.text.contains("the rest stays in Gizai's Memory at Projects/Shop"), "{}", b.text);
    let more = &b.text[b.text.find("### More notes for this card, not shown in full").expect("an index of the rest")..];
    assert!(more.contains("- Clients/Acme (") && more.contains("- Workflows/Release ("), "{more}");
    assert!(!more.contains("Globex"));
}

#[test]
fn a_note_of_another_client_in_the_agents_own_folder_stays_out_of_a_run_for_this_client() {
    // Client isolation (the card: "never include notes of another client or project") holds for the agent's own folder
    // too: the Team Lead or a person may keep a client's note there.
    let f = setup();
    let acme = clients::create(&f.db, &f.you, ClientInput { name: "Acme".into(), ..Default::default() }).unwrap();
    let globex = clients::create(&f.db, &f.you, ClientInput { name: "Globex".into(), ..Default::default() }).unwrap();
    f.project("Shop", "SHOP", Some(&acme));
    f.project("Globex portal", "GX", Some(&globex));
    f.write("Agents/Backend Agent/Globex gotchas", "---\nclient: Globex\nproject: GX\n---\nGlobex's staging needs a VPN.");
    f.write("Agents/Backend Agent/Shop gotchas", "---\nclient: Acme\n---\nShop's tests need Redis.");
    let shop = Context { role: "backend".into(), project: Some(("SHOP".into(), "Shop".into())), client: Some("Acme".into()) };
    let b = memory::prompt_block(&f.db, &f.be(), &shop).unwrap();
    assert!(b.given.iter().any(|g| g.path == "Agents/Backend Agent/Shop gotchas"), "{:?}", b.given);
    assert!(!b.text.contains("Globex"), "a run for Acme got Globex's note from the agent's own folder: {:?}", b.given);
}

#[test]
fn memory_append_finds_a_note_by_its_title_as_its_tool_says() {
    // The memory_append tool's description: "note: The note's path, title or id". memory_read and memory_move take a
    // title; an append by title should reach the same note.
    let f = setup();
    let id = f.write("Standards/Rust style", "# Rust style\n");
    let s = memory::append(&f.db, &f.lead(), "Rust style", Some("Errors"), "- No unwrap.", None);
    assert!(s.is_ok(), "{:?}", s.err());
    assert_eq!(s.unwrap().id, id);
    assert!(f.body(&id).contains("## Errors\n\n- No unwrap."));
}

#[test]
fn the_use_memory_switches_are_on_by_default_and_each_turns_it_off() {
    let f = setup();
    assert!(memory::enabled(&f.db) && memory::agent_uses(&f.db, &f.be));
    assert!(team::agent(&f.db, &f.be).unwrap().use_memory);
    memory::set_agent_uses(&f.db, &f.you, &f.be, false).unwrap();
    assert!(!memory::agent_uses(&f.db, &f.be) && memory::agent_uses(&f.db, &f.qa));
    assert!(!team::agent(&f.db, &f.be).unwrap().use_memory);
    // the agent form's switch, on a new agent and on update; None leaves it as it is
    let off = team::add_agent(&f.db, &f.you, &f.team, AgentInput { name: "Quiet".into(), role_key: "frontend".into(), use_memory: Some(false), ..Default::default() }).unwrap();
    assert!(!memory::agent_uses(&f.db, &off));
    let m = team::agent(&f.db, &off).unwrap();
    team::update_agent(&f.db, &f.you, &off, AgentInput { name: m.name.clone(), role_key: m.role_key.clone(), ..Default::default() }).unwrap();
    assert!(!memory::agent_uses(&f.db, &off), "None keeps it off");
    team::update_agent(&f.db, &f.you, &off, AgentInput { name: m.name, role_key: m.role_key, use_memory: Some(true), ..Default::default() }).unwrap();
    assert!(memory::agent_uses(&f.db, &off));
    // the app-wide switch (Settings → Runs) turns it off for every agent
    memory::set_enabled(&f.db, false).unwrap();
    assert!(!memory::enabled(&f.db) && !memory::agent_uses(&f.db, &f.qa) && !memory::agent_uses(&f.db, &off));
    assert_eq!(settings::get::<bool>(&f.db, "memory_on").unwrap(), Some(false));
    memory::set_enabled(&f.db, true).unwrap();
    assert!(memory::agent_uses(&f.db, &f.qa));
    assert!(!memory::agent_uses(&f.db, &f.be), "its own switch still off");
}
