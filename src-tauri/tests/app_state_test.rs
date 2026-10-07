// One Gizai per data folder, and backups on demand.
use std::sync::Arc;

#[test]
fn a_second_gizai_on_the_same_data_folder_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let first = gizai_lib::open_state(data.clone(), Arc::new(|_| {})).unwrap();
    let e = gizai_lib::open_state(data.clone(), Arc::new(|_| {})).err().expect("the second one is refused");
    assert!(e.contains("already running"), "{e}");
    // another data folder is fine
    assert!(gizai_lib::open_state(dir.path().join("other"), Arc::new(|_| {})).is_ok());
    drop(first);
    assert!(gizai_lib::open_state(data, Arc::new(|_| {})).is_ok(), "the lock goes with the first Gizai");
}

#[test]
fn backing_up_writes_a_snapshot_next_to_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    drop(gizai_lib::open_state(data.clone(), Arc::new(|_| {})).unwrap());
    let snap = gizai_lib::backup_data_dir(&data, "manual").unwrap();
    assert!(snap.starts_with(data.join("backups")) && snap.exists(), "{snap:?}");
    let e = gizai_lib::backup_data_dir(&dir.path().join("empty"), "manual").unwrap_err();
    assert!(e.contains("no Gizai data"), "{e}");
}

#[test]
fn a_backup_is_named_after_why_it_was_taken() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    drop(gizai_lib::open_state(data.clone(), Arc::new(|_| {})).unwrap());
    let snap = gizai_lib::backup_data_dir(&data, "before-install").unwrap();
    let name = snap.file_name().unwrap().to_string_lossy().to_string();
    assert!(name.starts_with("gizai-before-install-") && name.ends_with(".db"), "{name}");
    for bad in ["", "../x", "a b", "a/b"] {
        assert!(gizai_lib::backup_data_dir(&data, bad).is_err(), "label {bad:?} accepted");
    }
}

#[test]
fn each_data_folder_is_its_own_single_instance() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(dir.path().join("x")).unwrap();
    let id = gizai_lib::instance_id(&a);
    assert_eq!(id, gizai_lib::instance_id(&dir.path().join("x/../a")), "the same folder, spelled differently");
    assert_ne!(id, gizai_lib::instance_id(&b));
    // a valid D-Bus name: dot-separated elements of [A-Za-z0-9_], none starting with a digit
    assert!(id.starts_with("ai.gizai.app.") && id.len() < 200, "{id}");
    for part in id.split('.') {
        assert!(!part.is_empty() && !part.starts_with(|c: char| c.is_ascii_digit()), "{id}");
        assert!(part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'), "{id}");
    }
}

#[test]
fn only_a_data_folder_other_than_the_usual_one_gets_a_label() {
    assert_eq!(gizai_lib::data_label(&gizai_lib::default_data_dir()), None);
    assert_eq!(gizai_lib::data_label(std::path::Path::new("/home/x/gizai/.devdata/dev")).as_deref(), Some("dev"));
}
