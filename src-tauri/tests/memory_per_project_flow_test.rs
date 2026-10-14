//! GA-172 QA end to end: when Gizai starts on data from before the notes per project, the learned lines in the agents'
//! Notes move to their notes per project, once: after a group whose owner was removed is handed on, so its lines move
//! too. A second start changes no note.
#[path = "support/data_lock.rs"]
mod data_lock;

use gizai_core::memory::{self, Who};
use gizai_core::model::*;
use gizai_core::{projects, seed, settings, team};
use gizai_lib::AppState;

/// Every note as you see it, by path, with its version.
fn versions(st: &AppState) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = memory::list(&st.db, &Who::Person(st.you_id.clone())).unwrap().into_iter()
        .map(|n| (n.path, n.current_version)).collect();
    v.sort();
    v
}

#[tokio::test]
async fn the_first_start_moves_the_agents_learned_lines_to_their_notes_per_project_and_a_second_start_changes_no_note() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let head = memory::agent_template("Backend Agent");
    let builder_head = memory::agent_template("Builder");
    {
        // the data as Gizai 0.11.0 left it: one Notes per agent with the lines of every project, and no setting yet
        let db = gizai_core::db::Db::open(&data.join("gizai.db")).unwrap();
        let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
        let add = |name: &str, with: Option<String>| team::add_agent(&db, &s.you_id, &s.team_id, AgentInput { name: name.into(),
            role_key: "backend".into(), shares_memory_with: with, ..Default::default() }).unwrap();
        let be = add("Backend Agent", None);
        for (name, key) in [("Gizai", "GA"), ("Mono", "MN")] {
            projects::create(&db, &s.you_id, ProjectInput { name: name.into(), key: key.into(), ..Default::default() }).unwrap();
        }
        let notes = format!("{head}\n- 2026-10-11 (GA-99): 2026-10-11 (GA-99): A GA line.\n- 2026-10-11 (MN-3): An MN line.\n\
- 2026-10-12 (ZZ-1): Of no project.\n- 2026-10-12 (GA-111): GA-111: Another GA line.\n");
        memory::write(&db, &Who::Agent(be), "Agents/Backend Agent/Notes", &notes, Some(1), None).unwrap();
        // a group whose owner was removed: its folder goes to Builder 2 at start, and its lines move from there
        let builder = add("Builder", None);
        let builder2 = add("Builder 2", Some(builder.clone()));
        memory::learned(&db, &builder2, "GA-12", &["The group's GA line.".into()], None, "2026-10-12").unwrap();
        assert!(memory::find(&db, "Agents/Builder/Notes").unwrap().unwrap().body_md.contains("(GA-12, Builder 2): The group's GA line."),
                "GA-12 is no card here, so in Notes as before");
        db.write(None, |w| { w.conn().execute("UPDATE actors SET deleted_at = 5 WHERE id = ?1", [&builder])?; Ok(()) }).unwrap();
        assert!(settings::get::<serde_json::Value>(&db, "learned_by_project").unwrap().is_none());
    }

    let st = gizai_lib::test_state(tmp.path());
    let note = |st: &AppState, path: &str| memory::find(&st.db, path).unwrap().unwrap_or_else(|| panic!("{path} missing"));
    assert_eq!(note(&st, "Agents/Backend Agent/Notes").body_md, format!("{head}\n- 2026-10-12 (ZZ-1): Of no project.\n"));
    assert_eq!(note(&st, "Agents/Backend Agent/Notes").current_version, 3, "one new version");
    assert_eq!(note(&st, "Agents/Backend Agent/GA").body_md, format!("{}\n## Learned\n\n- 2026-10-11 (GA-99): A GA line.\n\
- 2026-10-12 (GA-111): Another GA line.\n", memory::project_template("Backend Agent", "GA")));
    assert_eq!(note(&st, "Agents/Backend Agent/MN").body_md, format!("{}\n## Learned\n\n- 2026-10-11 (MN-3): An MN line.\n",
                                                                     memory::project_template("Backend Agent", "MN")));
    // the group handed on first, then its lines moved
    assert!(memory::find(&st.db, "Agents/Builder/Notes").unwrap().is_none(), "handed on to Builder 2");
    let left = note(&st, "Agents/Builder 2/Notes").body_md;
    assert!(left.trim_end().ends_with("## Learned") && !left.contains("The group's GA line."), "only the text around the list stays: {left}");
    assert_eq!(left.split("## Learned").next(), builder_head.split("## Learned").next(), "{left}");
    let group = note(&st, "Agents/Builder 2/GA");
    assert!(group.body_md.ends_with("## Learned\n\n- 2026-10-12 (GA-12, Builder 2): The group's GA line.\n"), "{}", group.body_md);
    assert!(group.body_md.starts_with("---\ntype: note\ntags: [agent]\nproject: GA\n---\n# GA\n"), "{}", group.body_md);
    let set = settings::get::<serde_json::Value>(&st.db, "learned_by_project").unwrap().expect("recorded");
    assert_eq!(set.get("moved"), Some(&serde_json::json!(4)), "{set}");
    let first = versions(&st);

    // a second start changes no note
    drop(st);
    data_lock::released(&data).await;
    let st = gizai_lib::test_state(tmp.path());
    assert_eq!(versions(&st), first);
    assert_eq!(settings::get::<serde_json::Value>(&st.db, "learned_by_project").unwrap(), Some(set));
}
