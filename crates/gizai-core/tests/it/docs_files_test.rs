use gizai_core::{db::Db, docs, files, model::ProjectInput, projects, seed::ensure_seed};

#[test]
fn docs_keep_versions_and_reject_stale_saves() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let d = docs::create(&db, &s.you_id, &p, "Requirements").unwrap();
    let v2 = docs::save(&db, &s.you_id, &d, "# Eisen\nCSV-export met puntkomma", 1).unwrap();
    assert_eq!(v2, 2);
    assert!(docs::save(&db, &s.you_id, &d, "stale", 1).is_err());
    assert_eq!(docs::versions(&db, &d).unwrap().len(), 2);
    let doc = docs::get(&db, &d).unwrap();
    assert_eq!((doc.current_version, doc.body_md.as_str()), (2, "# Eisen\nCSV-export met puntkomma"));
    assert_eq!(docs::list(&db, &p).unwrap().len(), 1);
}

#[test]
fn files_are_content_addressed_and_deduplicated() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("a.csv"); std::fs::write(&a, "x;y\n1;2\n").unwrap();
    let b = tmp.path().join("b.csv"); std::fs::write(&b, "x;y\n1;2\n").unwrap();
    let fa = files::add_from_path(&db, &s.you_id, tmp.path(), "project", "P1", &a).unwrap();
    let fb = files::add_from_path(&db, &s.you_id, tmp.path(), "project", "P1", &b).unwrap();
    assert_eq!(fa.sha256, fb.sha256);
    assert!(files::blob_path(tmp.path(), &fa.sha256).exists());
    assert!(std::fs::metadata(files::blob_path(tmp.path(), &fa.sha256)).unwrap().permissions().readonly());
    assert_eq!(files::list(&db, "project", "P1").unwrap().len(), 2, "two file rows, one blob");
    assert_eq!(fa.mime.as_deref(), Some("text/csv"));
}

#[test]
fn adding_a_folder_or_missing_file_fails_cleanly() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    assert!(files::add_from_path(&db, &s.you_id, tmp.path(), "project", "P1", tmp.path()).is_err());
    assert!(files::add_from_path(&db, &s.you_id, tmp.path(), "project", "P1", &tmp.path().join("nope.txt")).is_err());
    assert!(files::add_from_path(&db, &s.you_id, tmp.path(), "planet", "P1", &tmp.path().join("nope.txt")).is_err());
    assert!(files::list(&db, "project", "P1").unwrap().is_empty());
}

#[test]
fn docs_can_be_renamed_and_old_versions_read() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let p = projects::create(&db, &s.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), ..Default::default() }).unwrap();
    let d = docs::create(&db, &s.you_id, &p, "Notes").unwrap();
    docs::save(&db, &s.you_id, &d, "first", 1).unwrap();
    docs::save(&db, &s.you_id, &d, "second", 2).unwrap();
    assert_eq!(docs::version_body(&db, &d, 2).unwrap(), "first");
    docs::rename(&db, &s.you_id, &d, "  Meeting notes ").unwrap();
    assert_eq!(docs::get(&db, &d).unwrap().title, "Meeting notes");
    assert!(docs::rename(&db, &s.you_id, &d, "  ").is_err());
    assert!(docs::create(&db, &s.you_id, &p, "").is_err());
}

#[test]
fn removed_files_leave_the_list() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("shot.png"); std::fs::write(&a, [0x89, b'P', b'N', b'G']).unwrap();
    let f = files::add_from_path(&db, &s.you_id, tmp.path(), "task", "T1", &a).unwrap();
    assert_eq!(f.mime.as_deref(), Some("image/png"));
    files::remove(&db, &s.you_id, &f.id).unwrap();
    assert!(files::list(&db, "task", "T1").unwrap().is_empty());
    assert_eq!(files::get(&db, &f.id).unwrap_err().to_string().contains("not found"), true);
}

#[test]
fn materialize_gives_a_named_copy_for_opening() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("Offerte Kade.pdf"); std::fs::write(&a, b"%PDF-1.7").unwrap();
    let f = files::add_from_path(&db, &s.you_id, tmp.path(), "project", "P1", &a).unwrap();
    let p = files::materialize(tmp.path(), &f).unwrap();
    assert!(p.ends_with("Offerte Kade.pdf"));
    assert!(p.starts_with(tmp.path().join("open")));
    assert_eq!(std::fs::read(&p).unwrap(), b"%PDF-1.7");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let blob = std::fs::metadata(files::blob_path(tmp.path(), &f.sha256)).unwrap();
        assert_ne!(std::fs::metadata(&p).unwrap().ino(), blob.ino(), "a copy, not a hard link");
    }
    assert_eq!(files::materialize(tmp.path(), &f).unwrap(), p, "second call reuses the copy");
    let odd = gizai_core::model::FileRow { name: "..".into(), ..f.clone() };
    assert!(files::materialize(tmp.path(), &odd).unwrap().starts_with(tmp.path().join("open")));
}
