use gizai_core::{db::Db, seed::ensure_seed};

#[test]
fn migrates_and_seeds_once() {
    let db = Db::open_in_memory().unwrap();
    let a = ensure_seed(&db, "Jeffrey").unwrap();
    let b = ensure_seed(&db, "Jeffrey").unwrap();
    assert_eq!(a.org_id, b.org_id, "seed is idempotent");
    let cols: Vec<(String, String)> = db
        .read(|c| {
            let mut st = c.prepare("SELECT name, category FROM workflow_states WHERE team_id=?1 ORDER BY sort_key")?;
            let rows = st.query_map([&a.team_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            Ok(rows)
        })
        .unwrap();
    assert_eq!(
        cols.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(),
        ["Backlog", "To do", "In progress", "Testing", "Review", "Deploy", "Done"]
    );
    let agents: i64 = db.read(|c| Ok(c.query_row("SELECT count(*) FROM actors WHERE kind='agent'", [], |r| r.get(0))?)).unwrap();
    assert_eq!(agents, 0, "no agents are seeded; Jeffrey creates them");
}

#[test]
fn every_write_appends_a_change() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let before: i64 = db.read(|c| Ok(c.query_row("SELECT count(*) FROM changes", [], |r| r.get(0))?)).unwrap();
    db.write(Some(&s.you_id), |w| {
        let id = gizai_core::ids::new_id();
        w.conn().execute(
            "INSERT INTO labels(id,created_at,updated_at,org_id,name,color) VALUES(?1,?2,?2,?3,'docs','#888')",
            rusqlite::params![id, gizai_core::ids::now_ms(), s.org_id],
        )?;
        w.insert("labels", &id, serde_json::json!({"name": "docs"}))?;
        Ok(())
    })
    .unwrap();
    let after: i64 = db.read(|c| Ok(c.query_row("SELECT count(*) FROM changes", [], |r| r.get(0))?)).unwrap();
    assert_eq!(after, before + 1);
}

#[test]
fn file_database_reopens_with_data() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("gizai.db");
    let org = ensure_seed(&Db::open(&path).unwrap(), "Jeffrey").unwrap().org_id;
    let again = ensure_seed(&Db::open(&path).unwrap(), "Jeffrey").unwrap().org_id;
    assert_eq!(org, again, "reopening keeps the seeded data and doesn't re-seed");
}
