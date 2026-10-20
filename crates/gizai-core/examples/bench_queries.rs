//! Times Gizai's hot core functions on prep_perf's data, without a window (GA-188, docs/research/performance.md §7 part
//! 4 and F3): each one 30 times after a warm-up, and prints one JSON line per query, like
//! {"name":"tasks.list","runs":30,"median_us":4200,"p95_us":5100}. The queries are what the window and the Team Lead's
//! tools read most: the task list with each filter they use (the window's list_tasks sends none, filters in the page and
//! gets the lean list without acceptance criteria, tasks::list_lean since GA-191; the Team Lead's tools get the full one,
//! with none to find a card, or a project and open cards, then a column or an assignee in Rust), the bin, the Inbox, a
//! card, its comments and activity, the project list, the team (the window's lean one without the agents' instructions,
//! and the tools' full one), the chat list, a big chat, Usage, Memory, and the whole board check, the longest read.
//! --plan: each line also has "plan": every SQL statement the function ran (caught with SQLite's trace hook; not the
//!   BEGIN and ROLLBACK each read runs in since GA-191), how many times it ran, and its EXPLAIN QUERY PLAN with its
//!   parameters NULL: [{"sql":…,"times":…,"plan":["SCAN …"]}].
//! --writer: a writer works beside the reads for the whole bench, as a working agent does: a comment and an update of one
//!   of KADE's In progress cards every 20 ms, through the core functions; each line says "writer":true. It CHANGES the
//!   data dir: run it on a copy.
//! usage: cargo run --release -p gizai-core --example bench_queries -- <data dir> [--plan] [--writer]
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gizai_core::board::{self, Context};
use gizai_core::memory::{self, Who};
use gizai_core::model::{TaskFilter, TaskPatch};
use gizai_core::db::{Db, READERS};
use gizai_core::{chat, comments, ids, projects, seed, settings, tasks, team, usage};
use rusqlite::ffi;
use serde_json::{Value, json};

const RUNS: usize = 30;
const DAY: i64 = 86_400_000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).filter(|a| !a.starts_with("--")).expect("usage: bench_queries <data dir> [--plan] [--writer]");
    let (plans, writer) = (args.iter().any(|a| a == "--plan"), args.iter().any(|a| a == "--writer"));
    let db = Db::open(&Path::new(dir).join("gizai.db")).unwrap();

    // What the queries read, found as the app finds it. The data is seeded, so this only reads.
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let kade = projects::list(&db).unwrap().into_iter().find(|p| p.key == "KADE").expect("no project KADE: run prep_perf first").id;
    let kade1 = tasks::get(&db, &tasks::id_of(&db, "KADE-1").unwrap()).unwrap();
    let backend = kade1.assignee_id.clone().expect("KADE-1 has no assignee: run prep_perf first");
    let lead = team::chat_agent(&db).unwrap().expect("no Team Lead with Chat on").actor_id;
    let in_progress = team::get(&db, &s.team_id).unwrap().states.into_iter().find(|st| st.category == "in_progress").unwrap().name;
    let (big, first_run): (String, i64) = db.read(|c| Ok((
        c.query_row("SELECT thread_id FROM chat_messages GROUP BY thread_id ORDER BY count(*) DESC LIMIT 1", [], |r| r.get(0))?,
        c.query_row("SELECT COALESCE(min(created_at), 0) FROM runs", [], |r| r.get(0))?,
    ))).unwrap();
    let now = ids::now_ms();
    let today_end = now - now.rem_euclid(DAY) + DAY;
    // The board check's context, as src-tauri/src/board.rs builds it: no pull is paused without the app, and "Runs at
    // once" is 3 until it is set (src-tauri's DEFAULT_MAX_CONCURRENT).
    let cx = Context {
        now, pull_paused: HashMap::new(), agents_paused: settings::get(&db, "agents_paused").unwrap().unwrap_or(false),
        max_concurrent: settings::get(&db, "max_concurrent_runs").unwrap().unwrap_or(3), lead_id: Some(lead.clone()), memory_wait: None,
    };
    let as_lead = Who::Lead(lead.clone());
    let kade_open = TaskFilter { project_id: Some(kade.clone()), open_only: true };

    let queries: Vec<(&str, Box<dyn Fn() + '_>)> = vec![
        ("tasks.list_lean", Box::new(|| keep(tasks::list_lean(&db, &TaskFilter::default())))),
        ("tasks.list", Box::new(|| keep(tasks::list(&db, &TaskFilter::default())))),
        ("tasks.list kade", Box::new(|| keep(tasks::list(&db, &TaskFilter { project_id: Some(kade.clone()), open_only: false })))),
        ("tasks.list open", Box::new(|| keep(tasks::list(&db, &TaskFilter { project_id: None, open_only: true })))),
        ("tasks.list kade open", Box::new(|| keep(tasks::list(&db, &kade_open)))),
        ("tasks.list kade open, column In progress", Box::new(|| {
            keep(tasks::list(&db, &kade_open).map(|l| l.into_iter().filter(|t| t.state_name == in_progress).collect::<Vec<_>>()))
        })),
        ("tasks.list kade open, assignee Backend Agent", Box::new(|| {
            keep(tasks::list(&db, &kade_open).map(|l| l.into_iter().filter(|t| t.assignee_id.as_deref() == Some(&backend)).collect::<Vec<_>>()))
        })),
        ("tasks.archived", Box::new(|| keep(tasks::archived(&db, None)))),
        ("tasks.needs_you", Box::new(|| keep(tasks::needs_you(&db, &s.you_id)))),
        ("tasks.get kade-1", Box::new(|| keep(tasks::get(&db, &kade1.id)))),
        ("tasks.activity kade-1", Box::new(|| keep(tasks::activity(&db, &kade1.id)))),
        ("comments.list kade-1", Box::new(|| keep(comments::list(&db, &kade1.id)))),
        ("projects.list", Box::new(|| keep(projects::list(&db)))),
        ("team.get_lean", Box::new(|| keep(team::get_lean(&db, &s.team_id)))),
        ("team.get", Box::new(|| keep(team::get(&db, &s.team_id)))),
        ("chat.list_threads", Box::new(|| keep(chat::list_threads(&db)))),
        ("chat.messages big", Box::new(|| keep(chat::messages(&db, &big)))),
        ("usage.summary 30d", Box::new(|| keep(usage::for_period(&db, "30d", now)))),
        ("usage.summary all", Box::new(|| keep(usage::summary(&db, first_run - first_run.rem_euclid(DAY), today_end)))),
        ("memory.list lead", Box::new(|| keep(memory::list(&db, &as_lead)))),
        ("memory.list_with_text lead", Box::new(|| keep(memory::list_with_text(&db, &as_lead)))),
        ("memory.search lead invoice", Box::new(|| keep(memory::search(&db, &as_lead, "invoice", 50)))),
        ("board.check", Box::new(|| keep(board::check(&db, &cx)))),
    ];

    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        if writer {
            let cards: Vec<String> = tasks::list(&db, &kade_open).unwrap().into_iter()
                .filter(|t| t.state_category == "in_progress" && t.id != kade1.id).map(|t| t.id).collect();
            let (db, backend, stop) = (&db, &backend, &stop);
            scope.spawn(move || write_like_an_agent(db, backend, &cards, stop));
        }
        for (name, f) in &queries {
            let (median, p95) = time(f.as_ref());
            let mut line = format!("{{\"name\":{},\"runs\":{RUNS},\"median_us\":{median},\"p95_us\":{p95}", json!(name));
            if writer {
                line += ",\"writer\":true";
            }
            if plans {
                line += &format!(",\"plan\":{}", plan(&db, f.as_ref()));
            }
            println!("{line}}}");
        }
        stop.store(true, Ordering::Relaxed);
    });
}

/// Keeps a query's answer from being optimised away.
fn keep<T>(answer: gizai_core::Result<T>) {
    std::hint::black_box(answer.unwrap());
}

/// `f` once to warm up, then 30 times: the median and the 95th percentile, in µs.
fn time(f: &dyn Fn()) -> (u128, u128) {
    f();
    let mut us: Vec<u128> = (0..RUNS).map(|_| {
        let t = Instant::now();
        f();
        t.elapsed().as_micros()
    }).collect();
    us.sort();
    (us[RUNS / 2], us[(RUNS * 95).div_ceil(100) - 1])
}

/// --writer: what a working agent writes every 20 ms until `stop`: a comment on one of `cards` and an update of it.
fn write_like_an_agent(db: &Db, agent: &str, cards: &[String], stop: &AtomicBool) {
    let mut n = 0;
    while !stop.load(Ordering::Relaxed) && !cards.is_empty() {
        let card = &cards[n % cards.len()];
        comments::add(db, agent, card, &format!("Progress {n}: the exporter writes the header row; the tests are next."), None).unwrap();
        tasks::update(db, agent, card, TaskPatch { priority: Some(1 + (n % 4) as i64), ..Default::default() }).unwrap();
        n += 1;
        std::thread::sleep(Duration::from_millis(20));
    }
    eprintln!("bench_queries: the writer wrote {n} comments and {n} card updates");
}

thread_local! {
    /// The statements SQLite started on this thread while a plan is taken; None: no plan is being taken (the writer's
    /// thread never takes one, so its statements stay out).
    static SEEN: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// SQLite's trace hook for SQLITE_TRACE_STMT: `sql` is the statement's text as it was prepared (or a comment, for a
/// trigger). A read's own transaction (BEGIN, ROLLBACK) has no plan and stays out. rusqlite's own `trace_v2` needs its `trace` feature, which Gizai doesn't build, so this is the hook itself
/// through rusqlite's ffi.
unsafe extern "C" fn on_statement(_event: c_uint, _ctx: *mut c_void, _stmt: *mut c_void, sql: *mut c_void) -> c_int {
    // SAFETY: for SQLITE_TRACE_STMT, SQLite passes a NUL-terminated UTF-8 string that lives during the call.
    let sql = unsafe { CStr::from_ptr(sql as *const c_char) }.to_string_lossy().into_owned();
    if !sql.starts_with("--") && !matches!(sql.split_whitespace().next(), Some("BEGIN" | "COMMIT" | "ROLLBACK")) {
        SEEN.with(|seen| seen.borrow_mut().as_mut().map(|list| list.push(sql)));
    }
    0
}

/// Turns the trace hook on or off on every connection a read can take. Since GA-191 a read takes one of the database's
/// `READERS` read-only connections, so these reads nest until each one is held, also one the writer's thread is using
/// now. A read outside a transaction is on the one connection (the read-only ones couldn't open): nothing to nest.
fn trace(db: &Db, on: bool) {
    trace_from(db, on, READERS);
}

fn trace_from(db: &Db, on: bool, left: usize) {
    db.read(|c| {
        // SAFETY: the handle is only used to set the hook, while this read holds the connection.
        unsafe {
            ffi::sqlite3_trace_v2(c.handle(), if on { ffi::SQLITE_TRACE_STMT } else { 0 }, if on { Some(on_statement) } else { None },
                                  std::ptr::null_mut());
        }
        if left > 1 && !c.is_autocommit() {
            trace_from(db, on, left - 1);
        }
        Ok(())
    }).unwrap();
}

/// The SQL statements `f` runs, each once in the order they first ran, how many times, and SQLite's plan for each.
fn plan(db: &Db, f: &dyn Fn()) -> Value {
    SEEN.with(|seen| *seen.borrow_mut() = Some(vec![]));
    trace(db, true);
    f();
    trace(db, false);
    let mut distinct: Vec<(String, usize)> = vec![];
    for sql in SEEN.with(|seen| seen.borrow_mut().take()).unwrap_or_default() {
        match distinct.iter_mut().find(|(s, _)| *s == sql) {
            Some((_, times)) => *times += 1,
            None => distinct.push((sql, 1)),
        }
    }
    distinct.into_iter().map(|(sql, times)| {
        let steps = db.read(|c| {
            let mut st = c.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
            let nulls = vec![rusqlite::types::Null; st.parameter_count()];
            Ok(st.query_map(rusqlite::params_from_iter(nulls), |r| r.get::<_, String>(3))?.collect::<rusqlite::Result<Vec<_>>>()?)
        }).unwrap_or_else(|e| vec![format!("no plan: {e}")]);
        json!({"sql": sql.split_whitespace().collect::<Vec<_>>().join(" "), "times": times, "plan": steps})
    }).collect()
}
