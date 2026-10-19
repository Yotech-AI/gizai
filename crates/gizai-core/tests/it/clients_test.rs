use gizai_core::{clients, db::Db, model::{ClientInput, Contact}, seed::ensure_seed, users};

fn input(name: &str) -> ClientInput {
    ClientInput {
        name: name.into(),
        country: Some("NL".into()),
        vat_number: Some("NL004512378B01".into()),
        coc_number: Some("34098812".into()),
        ..Default::default()
    }
}

#[test]
fn create_list_update_archive() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let id = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    clients::create(&db, &s.you_id, input("Atelier Mees")).unwrap();
    let all = clients::list(&db).unwrap();
    assert_eq!(all.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["Atelier Mees", "Kade Logistics B.V."]);
    clients::update(&db, &s.you_id, &id, ClientInput { city: Some("Amsterdam".into()), ..input("Kade Logistics B.V.") }).unwrap();
    assert_eq!(clients::get(&db, &id).unwrap().city.as_deref(), Some("Amsterdam"));
    clients::archive(&db, &s.you_id, &id).unwrap();
    assert_eq!(clients::list(&db).unwrap().len(), 1, "archived clients are hidden");
}

#[test]
fn empty_name_is_rejected() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let err = clients::create(&db, &s.you_id, input("   ")).unwrap_err();
    assert!(err.to_string().contains("name"), "{err}");
}

#[test]
fn contacts_keep_one_primary() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    let contact = |name: &str, primary: bool| Contact { id: String::new(), client_id: c.clone(), name: name.into(), role: None, email: None, phone: None, is_primary: primary };
    clients::upsert_contact(&db, &s.you_id, contact("Marloes Visser", true)).unwrap();
    clients::upsert_contact(&db, &s.you_id, contact("Tom Brouwer", true)).unwrap();
    let list = clients::contacts(&db, &c).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().filter(|x| x.is_primary).map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Tom Brouwer"]);
}

#[test]
fn users_get_unique_handles() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    users::create(&db, &s.you_id, "Sanne Bakker", Some("sanne@studio.example")).unwrap();
    users::create(&db, &s.you_id, "Sanne  Bakker", None).unwrap();
    let mut handles: Vec<String> = users::list(&db).unwrap().into_iter().map(|p| p.handle).collect();
    handles.sort();
    assert_eq!(handles, ["jeffrey", "sanne-bakker", "sanne-bakker-2"]);
}

fn add_contact(db: &Db, actor: &str, client_id: &str, name: &str, primary: bool) -> String {
    clients::upsert_contact(db, actor, Contact { client_id: client_id.into(), name: name.into(), is_primary: primary, ..Default::default() }).unwrap()
}

fn names(db: &Db, client_id: &str) -> Vec<(String, bool)> {
    clients::contacts(db, client_id).unwrap().into_iter().map(|k| (k.name, k.is_primary)).collect()
}

#[test]
fn removing_a_contact_keeps_the_primary() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    add_contact(&db, &s.you_id, &c, "Marloes Visser", true);
    let tom = add_contact(&db, &s.you_id, &c, "Tom Brouwer", false);
    clients::remove_contact(&db, &s.you_id, &tom).unwrap();
    assert_eq!(names(&db, &c), [("Marloes Visser".to_string(), true)]);
    assert_eq!(clients::get(&db, &c).unwrap().main_contact.as_deref(), Some("Marloes Visser"));
}

#[test]
fn removing_the_primary_makes_the_first_by_name_primary() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    let other = clients::create(&db, &s.you_id, input("Atelier Mees")).unwrap();
    let marloes = add_contact(&db, &s.you_id, &c, "Marloes Visser", true);
    add_contact(&db, &s.you_id, &c, "Tom Brouwer", false);
    // Lower case on purpose: by name ignores case, so "anna" comes before "Tom".
    add_contact(&db, &s.you_id, &c, "anna de Vries", false);
    add_contact(&db, &s.you_id, &other, "Sanne Mees", true);
    add_contact(&db, &s.you_id, &other, "Bram Mees", false);
    clients::remove_contact(&db, &s.you_id, &marloes).unwrap();
    assert_eq!(names(&db, &c), [("anna de Vries".to_string(), true), ("Tom Brouwer".to_string(), false)],
               "exactly one primary, the first remaining by name");
    assert_eq!(clients::get(&db, &c).unwrap().main_contact.as_deref(), Some("anna de Vries"));
    let listed = clients::list(&db).unwrap();
    assert_eq!(listed.iter().find(|x| x.id == c).unwrap().main_contact.as_deref(), Some("anna de Vries"));
    assert_eq!(names(&db, &other), [("Sanne Mees".to_string(), true), ("Bram Mees".to_string(), false)],
               "another client's contacts stay as they were");
}

#[test]
fn removing_the_last_contact_leaves_no_main_contact() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    let only = add_contact(&db, &s.you_id, &c, "Marloes Visser", true);
    clients::remove_contact(&db, &s.you_id, &only).unwrap();
    assert!(clients::contacts(&db, &c).unwrap().is_empty());
    assert_eq!(clients::get(&db, &c).unwrap().main_contact, None);
    // The client stays usable: a new contact can be added and becomes the main one.
    add_contact(&db, &s.you_id, &c, "Tom Brouwer", true);
    assert_eq!(clients::get(&db, &c).unwrap().main_contact.as_deref(), Some("Tom Brouwer"));
}

#[test]
fn removing_an_unknown_or_removed_contact_is_not_found() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    add_contact(&db, &s.you_id, &c, "Marloes Visser", true);
    let tom = add_contact(&db, &s.you_id, &c, "Tom Brouwer", false);
    clients::remove_contact(&db, &s.you_id, &tom).unwrap();
    let again = clients::remove_contact(&db, &s.you_id, &tom).unwrap_err();
    assert!(matches!(again, gizai_core::Error::NotFound(_)), "{again}");
    let unknown = clients::remove_contact(&db, &s.you_id, "no-such-contact").unwrap_err();
    assert!(matches!(unknown, gizai_core::Error::NotFound(_)), "{unknown}");
    assert_eq!(names(&db, &c), [("Marloes Visser".to_string(), true)], "a failed remove changes nothing");
    assert_eq!(clients::get(&db, &c).unwrap().main_contact.as_deref(), Some("Marloes Visser"));
}

#[test]
fn removing_a_contact_is_a_soft_delete_in_the_change_log() {
    let db = Db::open_in_memory().unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let c = clients::create(&db, &s.you_id, input("Kade Logistics B.V.")).unwrap();
    let marloes = add_contact(&db, &s.you_id, &c, "Marloes Visser", true);
    let tom = add_contact(&db, &s.you_id, &c, "Tom Brouwer", false);
    clients::remove_contact(&db, &s.you_id, &marloes).unwrap();
    let (deleted_at, primary): (Option<i64>, i64) = db.read(|k| Ok(k.query_row(
        "SELECT deleted_at, is_primary FROM contacts WHERE id=?1", [&marloes], |r| Ok((r.get(0)?, r.get(1)?)))?)).unwrap();
    assert!(deleted_at.is_some(), "the row is kept, marked deleted");
    assert_eq!(primary, 0, "a removed contact is no longer primary");
    let ops: Vec<(String, String, Option<String>)> = db.read(|k| {
        let mut st = k.prepare("SELECT row_id, op, actor_id FROM changes WHERE table_name='contacts' AND op IN ('delete','update') ORDER BY hlc")?;
        Ok(st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
    }).unwrap();
    assert_eq!(ops, [(marloes, "delete".to_string(), Some(s.you_id.clone())), (tom, "update".to_string(), Some(s.you_id.clone()))]);
}
