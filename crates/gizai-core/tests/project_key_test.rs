use gizai_core::db::Db;
use gizai_core::model::ProjectInput;
use gizai_core::projects::{self, free_key, suggest_key, unused_key};
use gizai_core::seed;

#[test]
fn keys_come_from_initials_or_the_first_letters() {
    assert_eq!(suggest_key("Kade Logistics portal"), "KLP");
    assert_eq!(suggest_key("Kade portal"), "KP");
    assert_eq!(suggest_key("Webshop"), "WEBS");
    assert_eq!(suggest_key("2026 rebrand"), "P2R");
    assert_eq!(suggest_key("X"), "XP");
    assert_eq!(suggest_key("  "), "PRJ");
    assert_eq!(suggest_key("a b c d e f g h"), "ABCDEF");
}

/// The keys `free_key` suggests for `n` projects called `name`, one after the other.
fn keys_in_a_row(name: &str, n: usize) -> Vec<String> {
    let mut taken: Vec<String> = vec![];
    for _ in 0..n {
        let k = free_key(name, &taken);
        assert!(!taken.contains(&k), "{k} suggested twice");
        taken.push(k);
    }
    taken
}

#[test]
fn a_taken_key_gets_a_number_and_stays_six_characters_after_nine_collisions() {
    // GA-25: the tenth used to be ABCDE10, seven characters
    assert_eq!(keys_in_a_row("a b c d e f g h", 12),
               ["ABCDEF", "ABCDE2", "ABCDE3", "ABCDE4", "ABCDE5", "ABCDE6", "ABCDE7", "ABCDE8", "ABCDE9", "ABCD10", "ABCD11", "ABCD12"]);
    let many = keys_in_a_row("a b c d e f g h", 102);
    assert_eq!(many[98..=100], ["ABCD99", "ABC100", "ABC101"]);
    let mut taken: Vec<String> = vec!["ABCDEF".into()];
    taken.extend((2..10).map(|n| format!("ABCDE{n}")));
    taken.extend((10..100).map(|n| format!("ABCD{n}")));
    taken.extend((100..1000).map(|n| format!("ABC{n}")));
    assert_eq!(free_key("a b c d e f g h", &taken), "AB1000");
    // shorter keys keep their letters until the number needs the room
    assert_eq!(keys_in_a_row("Kade portal", 11)[8..], ["KP9", "KP10", "KP11"]);
    let webs = keys_in_a_row("Webshop", 101);
    assert_eq!((webs[1].as_str(), webs[9].as_str(), webs[98].as_str(), webs[99].as_str()), ("WEBS2", "WEBS10", "WEBS99", "WEB100"));
    for k in many.iter().chain(&webs) {
        assert!((2..=6).contains(&k.len()) && k.starts_with(|c: char| c.is_ascii_uppercase())
                && k.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()), "{k}");
    }
}

#[test]
fn a_key_is_taken_whatever_its_case() {
    assert_eq!(free_key("Kade portal", &["kp".into()]), "KP2");
    assert_eq!(free_key("Kade portal", &["KLP".into()]), "KP", "a free key keeps its letters");
}

#[test]
fn unused_key_checks_every_project_and_its_keys_are_accepted() {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let make = |key: &str, status: Option<&str>| projects::create(&db, &s.you_id, ProjectInput { name: "a b c d e f g h".into(), key: key.into(),
        status: status.map(Into::into), ..Default::default() });
    // an archived project's key is taken too: create refuses it
    make("ABCDEF", Some("archived")).unwrap();
    assert!(make("ABCDEF", None).is_err());
    for _ in 0..110 {
        let k = unused_key(&db, "a b c d e f g h").unwrap();
        assert!(k.len() <= 6, "{k}");
        make(&k, None).unwrap_or_else(|e| panic!("{k}: {e}"));
    }
    let keys: Vec<String> = projects::list(&db).unwrap().into_iter().map(|p| p.key).collect();
    for k in ["ABCDE2", "ABCDE9", "ABCD10", "ABCD99", "ABC100", "ABC111"] {
        assert!(keys.contains(&k.to_string()), "{k} missing from {keys:?}");
    }
    assert_eq!(unused_key(&db, "a b c d e f g h").unwrap(), "ABC112");
}
