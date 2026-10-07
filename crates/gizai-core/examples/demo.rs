//! Fills a data dir with demo data for headless screenshots: `cargo run -p gizai-core --example demo -- <dir>`.
use gizai_core::{clients, comments, db::Db, docs, files, model::*, projects, seed::ensure_seed, tasks, users};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: demo <data dir>");
    std::fs::create_dir_all(&dir).unwrap();
    let db = Db::open(&std::path::Path::new(&dir).join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    if !projects::list(&db).unwrap().is_empty() {
        println!("demo data already present");
        return;
    }
    let you = &s.you_id;
    users::create(&db, you, "Sanne Bakker", Some("sanne@studio.example")).unwrap();
    let kade = clients::create(&db, you, ClientInput { name: "Kade Logistics B.V.".into(), city: Some("Amsterdam".into()),
        coc_number: Some("34098812".into()), vat_number: Some("NL004512378B01".into()), ..Default::default() }).unwrap();
    let fiets = clients::create(&db, you, ClientInput { name: "De Groene Fiets".into(), city: Some("Utrecht".into()),
        coc_number: Some("68873310".into()), vat_number: Some("NL857712093B01".into()), ..Default::default() }).unwrap();
    clients::create(&db, you, ClientInput { name: "Atelier Mees".into(), city: Some("Haarlem".into()), status: Some("lead".into()), ..Default::default() }).unwrap();
    clients::upsert_contact(&db, you, Contact { client_id: kade.clone(), name: "Marloes Visser".into(), role: Some("Head of finance".into()),
        email: Some("marloes@kade-logistics.example".into()), is_primary: true, ..Default::default() }).unwrap();
    let p1 = projects::create(&db, you, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), client_id: Some(kade),
        color: Some("#e8c547".into()), goal_md: Some("Give Kade's office one place for shipments, invoices and drivers.".into()), ..Default::default() }).unwrap();
    let p2 = projects::create(&db, you, ProjectInput { name: "Groene Fiets webshop".into(), key: "GFW".into(), client_id: Some(fiets),
        color: Some("#4fc1a6".into()), ..Default::default() }).unwrap();
    let team = gizai_core::team::get(&db, &s.team_id).unwrap();
    let col = |n: &str| team.states.iter().find(|x| x.name == n).unwrap().id.clone();
    let label = |n: &str| team.labels.iter().find(|x| x.name == n).unwrap().id.clone();
    let items = [
        (&p1, "Export invoices as CSV from the portal", "In progress", vec!["backend", "frontend"], 2),
        (&p1, "Rate limit the public tracking endpoint", "Testing", vec!["backend"], 3),
        (&p1, "Filament table: remember column order per user", "To do", vec!["frontend"], 3),
        (&p1, "Driver app: offline queue, one-page spec", "Backlog", vec![], 4),
        (&p2, "Mollie webhook: retry failed payment updates", "In progress", vec!["backend", "bug"], 1),
        (&p2, "Product filters: price range slider", "Review", vec!["frontend"], 4),
        (&p2, "Postcode and house number lookup on checkout", "To do", vec!["frontend"], 3),
        (&p2, "BTW 9% for the books category", "Done", vec!["backend"], 0),
    ];
    for (p, title, column, labels, prio) in items {
        let id = tasks::create(&db, you, TaskInput { project_id: p.to_string(), title: title.into(), priority: prio,
            description_md: format!("## Context\n\n{title}.\n\n- [ ] first step\n- [x] agreed with the client"),
            label_ids: labels.iter().map(|l| label(l)).collect(), ..Default::default() }).unwrap();
        tasks::move_to(&db, you, &id, &col(column), "").unwrap();
        if prio == 2 {
            comments::add(&db, you, &id, "Use Filament's exporter. Sample file from Marloes is attached.", None).unwrap();
        }
    }
    let req = docs::create(&db, you, &p1, "Requirements").unwrap();
    docs::save(&db, you, &req, "# Requirements\n\nThe portal replaces the Excel lists.\n\n## Invoices\n\n- [x] List and filter invoices\n- [ ] CSV export with semicolons (KADE-1)\n\nAsk @sanne-bakker about the driver app.", 1).unwrap();
    docs::save(&db, you, &req, "# Requirements\n\nThe portal replaces the Excel lists Kade's office keeps today.\n\n## Invoices\n\n- [x] List and filter invoices\n- [ ] CSV export with semicolons (KADE-1)\n\n## Drivers\n\nAsk @sanne-bakker about the driver app.", 2).unwrap();
    docs::create(&db, you, &p1, "Meeting notes 2026-10-02").unwrap();
    let sample = std::path::Path::new(&dir).join("invoices-sample.csv");
    std::fs::write(&sample, "number;date;amount\n2026-0412;2026-09-30;1250,00\n").unwrap();
    files::add_from_path(&db, you, std::path::Path::new(&dir), "project", &p1, &sample).unwrap();
    std::fs::remove_file(&sample).unwrap();
    println!("demo data written to {dir}");
}
