// GA-46: Chat → Recent asks for the 30 newest chats, and Chat → Archive searches all of them.
use gizai_core::chat::{self, NewMessage, ThreadHit};
use gizai_core::model::*;
use gizai_core::{db::Db, seed, team};
use serde_json::json;

struct W {
    db: Db,
    you: String,
    lead: String,
}

fn setup() -> W {
    let db = Db::open_in_memory().unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let lead = team::add_agent(&db, &s.you_id, &s.team_id,
        AgentInput { name: "Team Lead".into(), role_key: "lead".into(), chat_enabled: Some(true), ..Default::default() }).unwrap();
    W { db, you: s.you_id, lead }
}

impl W {
    fn chat(&self, title: &str) -> String {
        chat::create_thread(&self.db, &self.you, &self.lead, title).unwrap()
    }
    fn say(&self, thread: &str, role: &str, body: &str) -> String {
        let author = if role == "user" { self.you.clone() } else { self.lead.clone() };
        let tool = (role == "tool").then(|| json!({"id": "toolu_1", "input": {"title": body}}));
        chat::add_message(&self.db, NewMessage { thread_id: thread.into(), role: role.into(), author_id: Some(author), body_md: Some(body.into()),
                                                 tool_name: tool.as_ref().map(|_| "mcp__gizai__create_task".to_string()), tool, ..Default::default() })
            .unwrap().id
    }
    /// Sets a chat's last activity, so the order doesn't hang on the clock.
    fn at(&self, thread: &str, updated_at: i64) {
        self.db.write(None, |w| { w.conn().execute("UPDATE chat_threads SET updated_at = ?2 WHERE id = ?1", rusqlite::params![thread, updated_at])?; Ok(()) }).unwrap();
    }
    fn sql(&self, sql: &str, id: &str) {
        self.db.write(None, |w| { w.conn().execute(sql, [id])?; Ok(()) }).unwrap();
    }
    fn found(&self, query: &str) -> Vec<ThreadHit> {
        chat::search_threads(&self.db, query).unwrap()
    }
    fn found_ids(&self, query: &str) -> Vec<String> {
        self.found(query).into_iter().map(|h| h.thread.id).collect()
    }
}

/// `n` chats called "Chat 0" … ; chat i's last activity is 1000 + i, so the last one made is the newest.
fn many(w: &W, n: usize) -> Vec<String> {
    (0..n).map(|i| {
        let id = w.chat(&format!("Chat {i}"));
        w.at(&id, 1000 + i as i64);
        id
    }).collect()
}

#[test]
fn recent_returns_the_newest_chats_up_to_the_limit_newest_first() {
    let w = setup();
    let ids = many(&w, 35);
    let newest_first: Vec<String> = ids.iter().rev().cloned().collect();
    let recent: Vec<String> = chat::recent_threads(&w.db, 30).unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(recent, newest_first[..30]);
    // fewer chats than the limit: all of them
    assert_eq!(chat::recent_threads(&w.db, 50).unwrap().len(), 35);
    assert!(chat::recent_threads(&w.db, 0).unwrap().is_empty());
    // the full list is still every chat (the Inbox uses it)
    assert_eq!(chat::list_threads(&w.db).unwrap().into_iter().map(|t| t.id).collect::<Vec<_>>(), newest_first);
}

#[test]
fn recent_skips_deleted_chats_and_a_new_message_moves_an_old_chat_back_in() {
    let w = setup();
    let ids = many(&w, 32);
    // a deleted chat doesn't take one of the 30 places
    w.sql("UPDATE chat_threads SET deleted_at = 1 WHERE id = ?1", &ids[31]);
    let recent: Vec<String> = chat::recent_threads(&w.db, 30).unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(recent.len(), 30);
    assert!(!recent.contains(&ids[31]));
    assert_eq!(recent.first(), Some(&ids[30]));
    assert_eq!(recent.last(), Some(&ids[1]));
    assert!(!recent.contains(&ids[0]));
    // the oldest chat, opened on its own (from the Archive), works as usual
    assert_eq!(chat::get_thread(&w.db, &ids[0]).unwrap().title, "Chat 0");
    // and a new message in it moves it to the top of Recent
    w.say(&ids[0], "user", "Still there?");
    let recent: Vec<String> = chat::recent_threads(&w.db, 30).unwrap().into_iter().map(|t| t.id).collect();
    assert_eq!(recent.first(), Some(&ids[0]));
    assert!(!recent.contains(&ids[1]), "the 31st newest chat leaves Recent");
    // nothing was hidden or deleted along the way
    assert_eq!(chat::list_threads(&w.db).unwrap().len(), 31);
}

#[test]
fn search_finds_chats_by_title_and_by_your_and_the_team_leads_messages() {
    let w = setup();
    let title = w.chat("Warehouse invoices");
    w.say(&title, "user", "Who pays the rent?");
    let yours = w.chat("Stock");
    w.say(&yours, "user", "Please check the warehouse stock");
    let leads = w.chat("Dashboard");
    w.say(&leads, "user", "Is it ready?");
    w.say(&leads, "agent", "The warehouse dashboard is ready.");
    let none = w.chat("Unrelated");
    w.say(&none, "user", "Nothing to see");
    w.at(&title, 3000);
    w.at(&yours, 2000);
    w.at(&leads, 4000);
    w.at(&none, 5000);

    let hits = w.found("warehouse");
    assert_eq!(hits.iter().map(|h| h.thread.id.clone()).collect::<Vec<_>>(), [leads.clone(), title.clone(), yours.clone()], "newest first");
    let by = |id: &str| hits.iter().find(|h| h.thread.id == id).unwrap();
    // the matching message comes along, with who wrote it
    let m = by(&leads).message.as_ref().unwrap();
    assert_eq!((m.role.as_str(), m.body_md.as_deref(), m.author_name.as_deref()), ("agent", Some("The warehouse dashboard is ready."), Some("Team Lead")));
    let m = by(&yours).message.as_ref().unwrap();
    assert_eq!((m.role.as_str(), m.author_name.as_deref()), ("user", Some("Jeffrey")));
    // a chat that matches only on its title has no message
    assert!(by(&title).message.is_none());
    // the hit carries the chat as Recent shows it
    assert_eq!(by(&leads).thread.title, "Dashboard");
    assert!(w.found("no such words").is_empty());
}

#[test]
fn search_ignores_case_in_titles_and_messages() {
    let w = setup();
    let a = w.chat("Kade Portal");
    let b = w.chat("Other");
    w.say(&b, "agent", "The KADE export runs nightly");
    w.at(&a, 1000);
    w.at(&b, 2000);
    assert_eq!(w.found_ids("kade"), [b.clone(), a.clone()]);
    assert_eq!(w.found_ids("KaDe"), [b.clone(), a.clone()]);
    assert_eq!(w.found_ids("PORTAL"), [a.clone()]);
    assert_eq!(w.found_ids("export RUNS"), [b]);
}

#[test]
fn percent_underscore_and_backslash_match_themselves() {
    let w = setup();
    let pct = w.chat("Progress");
    w.say(&pct, "user", "We are 50% done");
    let digits = w.chat("Numbers");
    w.say(&digits, "user", "We have 500 done");
    let under = w.chat("Names");
    w.say(&under, "agent", "Rename it to user_id");
    let x = w.chat("Other names");
    w.say(&x, "agent", "Rename it to userXid");
    let slash = w.chat("Paths");
    w.say(&slash, "user", r"Look in C:\temp");
    let colon = w.chat("Times");
    w.say(&colon, "user", "Meet at 10:temp");
    let title = w.chat("Discount 100%");

    assert_eq!(w.found_ids("50%"), [pct.clone()]);
    assert_eq!(w.found_ids("%"), [title.clone(), pct.clone()]);
    assert_eq!(w.found_ids("user_id"), [under.clone()]);
    assert_eq!(w.found_ids("_"), [under]);
    assert_eq!(w.found_ids(r":\t"), [slash]);
    assert_eq!(w.found_ids("100%"), [title]);
    assert!(w.found_ids("5_0").is_empty());
    assert!(w.found_ids("%%").is_empty());
}

#[test]
fn search_leaves_out_tool_calls_gizais_notes_and_deleted_chats_and_messages() {
    let w = setup();
    let tool = w.chat("One");
    w.say(&tool, "tool", "zebra");
    let note = w.chat("Two");
    w.say(&note, "system", "zebra moved to another account");
    let gone = w.chat("Three");
    w.say(&gone, "user", "zebra crossing");
    w.sql("UPDATE chat_threads SET deleted_at = 1 WHERE id = ?1", &gone);
    let gone_title = w.chat("Zebra plans");
    w.sql("UPDATE chat_threads SET deleted_at = 1 WHERE id = ?1", &gone_title);
    let gone_msg = w.chat("Four");
    let id = w.say(&gone_msg, "agent", "zebra stripes");
    w.sql("UPDATE chat_messages SET deleted_at = 1 WHERE id = ?1", &id);
    assert!(w.found("zebra").is_empty(), "{:?}", w.found_ids("zebra"));
    // the chats themselves are still there, and listed with an empty search
    let all = w.found_ids("");
    for id in [&tool, &note, &gone_msg] {
        assert!(all.contains(id));
    }
    assert!(!all.contains(&gone) && !all.contains(&gone_title));
}

#[test]
fn each_chat_shows_once_with_its_newest_matching_message() {
    let w = setup();
    let t = w.chat("Apples");
    w.say(&t, "user", "apples first");
    w.say(&t, "agent", "apples second");
    let newest = w.say(&t, "user", "apples third");
    w.say(&t, "agent", "pears, no match");
    w.say(&t, "tool", "apples in a tool call");
    let hits = w.found("APPLES");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].message.as_ref().map(|m| m.id.clone()), Some(newest));
}

#[test]
fn an_empty_search_lists_every_chat_newest_first_without_messages() {
    let w = setup();
    let ids = many(&w, 35);
    w.say(&ids[3], "user", "something");
    w.at(&ids[3], 500);
    w.sql("UPDATE chat_threads SET deleted_at = 1 WHERE id = ?1", &ids[10]);
    let mut want: Vec<String> = ids.iter().rev().filter(|id| *id != &ids[10] && *id != &ids[3]).cloned().collect();
    want.push(ids[3].clone());
    for q in ["", "   "] {
        let hits = w.found(q);
        assert_eq!(hits.len(), 34, "more than the 30 in Recent");
        assert_eq!(hits.iter().map(|h| h.thread.id.clone()).collect::<Vec<_>>(), want);
        assert!(hits.iter().all(|h| h.message.is_none()));
    }
    // spaces around a search are trimmed
    assert_eq!(w.found_ids("  Chat 34  "), [ids[34].clone()]);
}

#[test]
fn a_hit_serializes_as_the_ui_reads_it() {
    let w = setup();
    let t = w.chat("Hello");
    w.say(&t, "agent", "hello back");
    let v = serde_json::to_value(&w.found("hello")[0]).unwrap();
    assert_eq!(v["thread"]["id"], t.as_str());
    assert_eq!(v["thread"]["title"], "Hello");
    assert_eq!(v["message"]["bodyMd"], "hello back");
    assert_eq!(v["message"]["role"], "agent");
    assert_eq!(v["message"]["authorName"], "Team Lead");
    let v = serde_json::to_value(&w.found("")[0]).unwrap();
    assert!(v["message"].is_null());
}
