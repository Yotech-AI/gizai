//! GA-68 QA: what the Memory page asks of core. `list_with_text` (every note the asker may read, with its text, by path)
//! and `recent` (Recently changed: each note's last saved version, newest first, with who wrote it: a person, an agent,
//! and the run and its card when a run's result wrote it).
use gizai_core::db::Db;
use gizai_core::memory::{self, Who};
use gizai_core::model::*;
use gizai_core::{ids, projects, runs, seed, tasks, team};

struct F {
    db: Db,
    you: String,
    lead: String,
    be: String,
}

/// A Team Lead (Chat on), a Backend Agent and a QA Agent; each agent has its own Notes from the start.
fn setup() -> F {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let add = |name: &str, role: &str, chat: bool| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput {
        name: name.into(), role_key: role.into(), chat_enabled: chat.then_some(true), ..Default::default() }).unwrap();
    let lead = add("Team Lead", "lead", true);
    let be = add("Backend Agent", "backend", false);
    add("QA Agent", "qa", false);
    F { you: s.you_id, lead, be, db }
}

impl F {
    fn you(&self) -> Who { Who::Person(self.you.clone()) }
    fn be(&self) -> Who { Who::Agent(self.be.clone()) }
    fn lead(&self) -> Who { Who::Lead(self.lead.clone()) }
    /// Every saved version of the notes moves to `at` + its place in `order` (milliseconds), so "newest" is certain.
    fn stamp(&self, order: &[&str]) {
        for (i, path) in order.iter().enumerate() {
            let id = memory::find(&self.db, path).unwrap().unwrap().id;
            self.db.write(None, |w| {
                w.conn().execute("UPDATE doc_versions SET created_at = ?1 + version WHERE doc_id = ?2", rusqlite::params![1_000_000 + 1000 * i as i64, id])?;
                Ok(())
            }).unwrap();
        }
    }
}

#[test]
fn list_with_text_gives_every_note_the_asker_may_read_with_its_text_and_list_gives_the_same_notes_without() {
    let f = setup();
    memory::write(&f.db, &f.you(), "Standards/Rust style", "Use clippy. [[Deploy steps]]", None, None).unwrap();
    memory::write(&f.db, &f.you(), "Workflows/Deploy steps", "# Deploy steps\n\n## Rollback\n", None, None).unwrap();
    memory::write(&f.db, &f.lead(), "Team Lead/Plans", "Private to the lead.", None, None).unwrap();

    // a person reads every note, by path (case ignored), each with its text
    let all = memory::list_with_text(&f.db, &f.you()).unwrap();
    let paths: Vec<&str> = all.iter().map(|n| n.path.as_str()).collect();
    assert_eq!(paths, ["Agents/Backend Agent/Notes", "Agents/QA Agent/Notes", "Standards/Rust style", "Team Lead/Plans", "Workflows/Deploy steps"]);
    let style = all.iter().find(|n| n.path == "Standards/Rust style").unwrap();
    assert_eq!(style.body_md, "Use clippy. [[Deploy steps]]");
    assert_eq!(style.chars, style.body_md.len() as i64);
    assert_eq!(style.scope, "shared");
    assert!(all.iter().all(|n| !n.body_md.is_empty()), "every note has its text: {all:?}");
    let be_notes = all.iter().find(|n| n.path == "Agents/Backend Agent/Notes").unwrap();
    assert_eq!((be_notes.scope.as_str(), be_notes.owner_id.as_deref()), ("agent", Some(f.be.as_str())));

    // `list` is the same notes in the same order, without their text
    let plain = memory::list(&f.db, &f.you()).unwrap();
    assert_eq!(plain.iter().map(|n| n.path.as_str()).collect::<Vec<_>>(), paths);
    assert!(plain.iter().all(|n| n.body_md.is_empty()));
    assert_eq!(plain.iter().map(|n| n.chars).collect::<Vec<_>>(), all.iter().map(|n| n.chars).collect::<Vec<_>>());

    // an agent: the shared folders and its own, never the Team Lead's or another agent's
    let be: Vec<String> = memory::list_with_text(&f.db, &f.be()).unwrap().into_iter().map(|n| n.path).collect();
    assert_eq!(be, ["Agents/Backend Agent/Notes", "Standards/Rust style", "Workflows/Deploy steps"]);
}

#[test]
fn recent_lists_each_note_once_by_its_last_saved_version_newest_first_with_who_wrote_it() {
    let f = setup();
    let style = memory::write(&f.db, &f.you(), "Standards/Rust style", "Use clippy.", None, None).unwrap();
    memory::write(&f.db, &f.lead(), "Decisions/Use SQLite", "Local data.", None, None).unwrap();
    // a second version of Rust style, by the Team Lead
    memory::write(&f.db, &f.lead(), "Standards/Rust style", "Use clippy and rustfmt.", Some(style.version), None).unwrap();
    // the Backend Agent learned something in a run on KADE-1
    let project = projects::create(&f.db, &f.you, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let card = tasks::create(&f.db, &f.you, TaskInput { project_id: project, title: "Export".into(), ..Default::default() }).unwrap();
    let kade1 = tasks::get(&f.db, &card).unwrap();
    assert_eq!(kade1.identifier, "KADE-1");
    let run = runs::create(&f.db, &f.be, &kade1.id, "backend", &ids::new_id(), "/tmp", "/tmp", "b", "/tmp/l").unwrap();
    memory::learned(&f.db, &f.be, "KADE-1", &["Run the fake CLI in tests.".to_string()], Some(&run), "2026-10-10").unwrap();
    f.stamp(&["Agents/QA Agent/Notes", "Decisions/Use SQLite", "Standards/Rust style", "Agents/Backend Agent/Notes"]);

    let recent = memory::recent(&f.db, &f.you(), 30).unwrap();
    let rows: Vec<(&str, i64, Option<&str>, Option<&str>)> = recent.iter()
        .map(|c| (c.note.path.as_str(), c.version, c.author_name.as_deref(), c.author_kind.as_deref())).collect();
    assert_eq!(rows, [
        ("Agents/Backend Agent/Notes", 2, Some("Backend Agent"), Some("agent")),
        ("Standards/Rust style", 2, Some("Team Lead"), Some("agent")),
        ("Decisions/Use SQLite", 1, Some("Team Lead"), Some("agent")),
        ("Agents/QA Agent/Notes", 1, Some("QA Agent"), Some("agent")),
    ]);
    // the run and its card are on the version a run wrote, and on no other
    let learned = &recent[0];
    assert_eq!(learned.author_id.as_deref(), Some(f.be.as_str()));
    assert_eq!((learned.run_id.as_deref(), learned.task_id.as_deref(), learned.task_identifier.as_deref()), (Some(run.as_str()), Some(kade1.id.as_str()), Some("KADE-1")));
    assert!(recent[1..].iter().all(|c| c.run_id.is_none() && c.task_id.is_none() && c.task_identifier.is_none()));
    // newest first, and `at` is when the last version was saved
    assert!(recent.windows(2).all(|w| w[0].at > w[1].at), "{:?}", recent.iter().map(|c| c.at).collect::<Vec<_>>());
    assert_eq!(recent[1].at, 1_000_000 + 2000 + 2);
    // the note comes without its text, as `list` gives it
    assert!(recent.iter().all(|c| c.note.body_md.is_empty()));
    assert_eq!(recent[1].note.current_version, 2);
}

#[test]
fn recent_says_a_person_wrote_a_note_keeps_to_what_the_asker_may_read_and_to_the_limit() {
    let f = setup();
    memory::write(&f.db, &f.you(), "Lessons/Flaky tests", "Wait for the lock.", None, None).unwrap();
    memory::write(&f.db, &f.lead(), "Team Lead/Plans", "Private to the lead.", None, None).unwrap();
    f.stamp(&["Agents/QA Agent/Notes", "Agents/Backend Agent/Notes", "Lessons/Flaky tests", "Team Lead/Plans"]);

    let all = memory::recent(&f.db, &f.you(), 30).unwrap();
    assert_eq!(all.iter().map(|c| c.note.path.as_str()).collect::<Vec<_>>(),
               ["Team Lead/Plans", "Lessons/Flaky tests", "Agents/Backend Agent/Notes", "Agents/QA Agent/Notes"]);
    let flaky = &all[1];
    assert_eq!((flaky.author_id.as_deref(), flaky.author_name.as_deref(), flaky.author_kind.as_deref()), (Some(f.you.as_str()), Some("Jeffrey"), Some("person")));

    // an agent sees only the shared notes and its own
    let be: Vec<String> = memory::recent(&f.db, &f.be(), 30).unwrap().into_iter().map(|c| c.note.path).collect();
    assert_eq!(be, ["Lessons/Flaky tests", "Agents/Backend Agent/Notes"]);

    // at most `limit`, the newest; a limit of 0 still gives one
    let two: Vec<String> = memory::recent(&f.db, &f.you(), 2).unwrap().into_iter().map(|c| c.note.path).collect();
    assert_eq!(two, ["Team Lead/Plans", "Lessons/Flaky tests"]);
    assert_eq!(memory::recent(&f.db, &f.you(), 0).unwrap().len(), 1);
}

#[test]
fn recent_with_versions_at_the_same_moment_orders_them_by_path_and_a_moved_note_keeps_its_history() {
    let f = setup();
    memory::write(&f.db, &f.you(), "Standards/b note", "b", None, None).unwrap();
    memory::write(&f.db, &f.you(), "Standards/A note", "a", None, None).unwrap();
    let a = memory::find(&f.db, "Standards/A note").unwrap().unwrap();
    f.db.write(None, |w| { w.conn().execute("UPDATE doc_versions SET created_at = 5", [])?; Ok(()) }).unwrap();
    let same: Vec<String> = memory::recent(&f.db, &f.you(), 30).unwrap().into_iter().map(|c| c.note.path).collect();
    assert_eq!(same, ["Agents/Backend Agent/Notes", "Agents/QA Agent/Notes", "Standards/A note", "Standards/b note"]);

    // the Memory page's drag and rename: a move keeps the note, its last version and its author
    memory::move_note(&f.db, &f.you(), &a.id, "Decisions/", false).unwrap();
    let moved = memory::recent(&f.db, &f.you(), 30).unwrap().into_iter().find(|c| c.note.id == a.id).unwrap();
    assert_eq!((moved.note.path.as_str(), moved.version, moved.author_name.as_deref()), ("Decisions/A note", 1, Some("Jeffrey")));
}
