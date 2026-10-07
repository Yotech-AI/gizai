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
