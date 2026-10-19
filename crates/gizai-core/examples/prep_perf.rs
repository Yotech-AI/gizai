//! Makes the data for GA-188's benchmark harness (docs/research/performance.md §7 part 1): first the demo data, as the
//! demo example makes it, when <data dir> has none, then on it:
//! - The Team Lead (Chat on), the Backend, Frontend and QA Agents. Claude Code is <fake claude>; the Team Lead's chat
//!   turns run on a second coding CLI, "Claude Code (chat)", whose program is fake-claude-chat.py next to it. Agents are
//!   paused in Settings, so the queue and the board check start nothing, and the Frontend and QA Agents are paused
//!   themselves. The Team Lead stays active (the chat refuses a paused one) and so does the Backend Agent (Run refuses a
//!   paused agent; Settings' pause stops only the queue): KADE-1 is assigned to it, so Run on KADE-1 starts it. Tool
//!   calls per run are 2,000 (FAKE_STREAM 50x20 makes 333; the default 200 would stop the run).
//! - 500 cards in KADE over every column, each with a 1 to 3 KB description, up to three labels and three comments
//!   (yours, Sanne's, the agents'); one open card in twelve on hold, some assigned (Review and Deploy cards to you: the
//!   Inbox). KADE-1 stays in In progress, and its description has FAKE_STREAM 50x20 (the fake streams 50 events a
//!   second for 20 s).
//! - 90 days of history: 2,000 finished runs with tokens and cost (KADE's cards mostly, one in ten on GFW's; mostly
//!   succeeded), 60 chats with the Team Lead (59 of a few messages, a chat turn per answer; the newest has 1,000
//!   messages, every tenth a tool call). Their times, and the activity's (`changes`), are set back to when they happened.
//! - 300 Memory notes that link each other (prep_graph's pattern), in the shared folders and the Team Lead's; the most
//!   linked is Projects/Kade portal.
//! - KADE-1's last run, by the Backend Agent, whose log has 5,000 events: <data dir>/runs/<session>.jsonl, stored with
//!   its absolute path, as the app stores a run's log. A copy of the data dir reads the original's log.
//! --years <n>: the history over n × 365 days at the same pace (about 8,100 runs a year), and with it more comments and
//!   chat messages; the 1,000-message chat stays 1,000.
//! --repo <dir>: links KADE to the git repository <dir>, made (main, one empty commit) when it isn't there, so Run starts.
//! --editor <dir>: makes <dir>/big (once): a git repository of 20,000 small files in 200 folders and big.ts of 5,000
//!   lines. Then <dir> is the Editor folder, so big is one of the Editor's folders. Checked as the app checks it, with
//!   HOME and XDG_DATA_HOME, so give prep_perf the ones the app runs with: a folder in Gizai's data folders (the data
//!   dir, $XDG_DATA_HOME/gizai, ~/.local/share/gizai) is refused, and prep_perf stops with why.
//! On a data dir it made before, or a copy of one, it makes nothing again: it does --repo and --editor and prints the ids.
//! Prints one line, progress goes to stderr: card=<KADE-1> run=<its 5,000-event run> chat=<the 1,000-message chat>
//! note=<Projects/Kade portal> lead=<the Team Lead> folder=<the Editor's id of <dir>/big, or -> file=big.ts
//! usage: cargo run -p gizai-core --example prep_perf -- <data dir> <fake claude> [--years <n>] [--repo <dir>] [--editor <dir>]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use gizai_core::chat::{self, NewMessage};
use gizai_core::clis::{self, Cli};
use gizai_core::folders::{self, Places};
use gizai_core::memory::{self, Who};
use gizai_core::seed::{SeedIds, ensure_seed};
use gizai_core::{clients, comments, db::Db, docs, editor, files, ids, model::*, projects, runs, settings, tasks, team, users};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;
/// KADE's cards in all, the demo's four included.
const CARDS: usize = 500;
/// Finished runs in 90 days.
const RUNS_90: f64 = 2_000.0;
/// The chats with a few messages, and the messages of the big one (the newest).
const SMALL_CHATS: usize = 59;
const BIG_CHAT: usize = 1_000;
const NOTES: usize = 300;
/// The events in KADE-1's last run's log.
const LOG_EVENTS: usize = 5_000;
/// The files of <editor>/big, besides big.ts.
const BIG_FILES: usize = 20_000;
/// The most linked Memory note.
const HUB: &str = "Projects/Kade portal";
/// The coding CLI the Team Lead's chat turns run on.
const CHAT_CLI: &str = "Claude Code (chat)";

const FOLDERS: [&str; 9] = ["Decisions", "Workflows", "Standards", "Lessons", "Clients", "Projects", "Deployments", "Dependencies", "Team Lead"];

// What the cards, comments, chats and notes say, combined by number.
const SENTENCES: [&str; 32] = [
    "Kade's office exports the invoices every Friday and imports them in Exact Online.",
    "The CSV uses semicolons and a decimal comma, as Dutch Excel expects.",
    "Drivers confirm a delivery in the app; the office sees it within a minute.",
    "Mollie retries a failed webhook for a day, so the handler must be idempotent.",
    "The tracking endpoint is public, so it needs a rate limit per IP address.",
    "Filament keeps a table's column order in the session, not per user.",
    "The queue worker runs with --tries=3, so one stuck job doesn't block the others.",
    "The postcode lookup answers in about 80 ms; keep its answers for a day.",
    "Marloes wants the totals per month at the bottom of the invoice export.",
    "Use the existing InvoiceExporter and add a test per column.",
    "The shipments table has 1.2 million rows, so the list pages on the server.",
    "BTW is 21%, except for books, which are 9%.",
    "Sanne tests the driver app on an old Android phone before every release.",
    "Deploys go through Forge from main; a release is a tag.",
    "Answer within 200 ms at the 95th percentile, also on the busiest Monday.",
    "The invoice numbers must stay gapless: the accountant checks them every quarter.",
    "Customers see the tracking page without logging in, so it shows no prices.",
    "A shipment can have up to 40 stops; the map draws them in order.",
    "The driver app works offline and sends its queue when the network is back.",
    "Old invoices stay readable for seven years, as the tax office asks.",
    "The export runs in the background and mails a link when it is ready.",
    "Every change to an invoice is in the audit log, with who made it.",
    "The API tokens page shows when each token was last used.",
    "Validation messages are in Dutch; the code and the logs stay in English.",
    "The dashboard totals come from one query per widget, cached for five minutes.",
    "Password reset links work once and expire after an hour.",
    "The SMS gateway charges per message, so the portal sends reminders in batches.",
    "Address validation accepts a house number with a letter, like 12a.",
    "A PDF invoice has the company's KvK and BTW numbers in its footer.",
    "The monthly report compares each driver's deliveries with the month before.",
    "Roles are office, driver and admin; only admins see the invoice settings.",
    "Search finds a shipment by its number, its customer or its postcode.",
];
const STEPS: [&str; 24] = [
    "Write the migration and try it on a copy of production", "Add the column to the Filament table",
    "Cover the edge cases with a feature test", "Ask Marloes for a sample file", "Check the query plan with EXPLAIN",
    "Add the route and its policy", "Update the API documentation", "Log failures with the shipment number",
    "Show a toast when the export is ready", "Keep the old endpoint working for a month", "Put a rate limiter on the route",
    "Try it on the slowest phone the drivers use", "Translate the new labels", "Move the job to the high queue",
    "Remove the feature flag after the release", "Add an index on shipments (customer_id, created_at)",
    "Write the changelog entry", "Seed the demo data with ten examples", "Handle a webhook that arrives twice",
    "Round amounts the way the accountant does", "Page the list on the server", "Mail a link instead of the file",
    "Check it in Safari on an iPad", "Keep the answer under 200 ms",
];
const CODE: [(&str, &str); 6] = [
    ("php", "public function headings(): array\n{\n    return ['number', 'date', 'customer', 'amount', 'btw'];\n}"),
    ("php", "RateLimiter::for('tracking', fn (Request $r) => Limit::perMinute(60)->by($r->ip()));"),
    ("sql", "SELECT customer_id, count(*) FROM shipments\nWHERE created_at >= now() - interval '30 days'\nGROUP BY customer_id;"),
    ("ts", "export function formatAmount(cents: number): string {\n  return (cents / 100).toLocaleString(\"nl-NL\", { style: \"currency\", currency: \"EUR\" });\n}"),
    ("bash", "php artisan queue:work --queue=high,default --tries=3 --timeout=120"),
    ("php", "if (Payment::where('mollie_id', $id)->where('status', 'paid')->exists()) {\n    return response()->noContent();\n}"),
];
const ASKS: [&str; 16] = [
    "What is left before the Kade portal release?", "Can you split KADE-12 into smaller cards?",
    "Why did the last run on the CSV export fail?", "Make a card for the rate limit on the tracking endpoint.",
    "Which cards are waiting for me?", "Summarise what the agents did yesterday.", "Is the Mollie webhook fix in Review yet?",
    "Move the driver app cards to the backlog for now.", "What did we decide about the invoice numbers?",
    "Give the postcode lookup card a clear acceptance list.", "How much did the runs cost this week?",
    "Ask Sanne which Android version the drivers use.", "Which tests cover the export?",
    "Plan the work for next week, biggest risk first.", "Put the BTW change for books on the board.",
    "Check the board and tell me what is stuck.",
];
const REMARKS: [&str; 12] = [
    "Looks good to me.", "Marloes confirmed this on the phone.", "Blocked until the sample file arrives.",
    "Picked this up; the tests are next.", "QA found two issues, see the run.", "Moved to Review: the pull request is open.",
    "Merged and deployed to staging.", "Please keep the old column names in the export.", "This also fixes the slow dashboard.",
    "Can we do this after the release?", "The agent asked a question; answered below.", "Sanne tried it on the old phone: it works.",
];
const VERBS: [&str; 12] = ["Add", "Fix", "Speed up", "Rework", "Test", "Document", "Clean up", "Split", "Cache", "Log", "Validate", "Translate"];
const OBJECTS: [&str; 42] = [
    "the invoice list", "the CSV export", "the tracking endpoint", "driver check-in", "the shipments table", "Mollie webhooks",
    "the postcode lookup", "the monthly report", "user roles", "the login page", "password reset", "the API tokens page",
    "Filament filters", "the queue worker", "PDF invoices", "the dashboard totals", "address validation", "the delivery map",
    "SMS reminders", "the audit log", "the customer page", "the driver app sync", "the stops list", "the price rules", "BTW rates",
    "the export mail", "search", "the settings page", "the onboarding mail", "the proof of delivery", "the route planner",
    "the depot list", "the import of old invoices", "credit notes", "the payment page", "the customer portal", "the vehicle list",
    "the holiday calendar", "damage reports", "the signature pad", "the label printer", "the backup job",
];
/// Why a card waits for you (a "needs a decision" hold).
const HOLDS: [&str; 4] = [
    "Which delimiter does Kade's accounting import expect: a semicolon or a comma?",
    "Should a cancelled shipment keep its invoice number?",
    "May the driver app keep photos on the phone after sending them?",
    "Is 60 requests a minute per IP address enough for the tracking page?",
];
const BLOCKED: &str = "Preparing the worktree failed: composer install exited with code 1 (ext-intl is missing).";
const SUBJECTS: [&str; 20] = [
    "CSV export", "Rate limits", "Mollie webhooks", "Driver app", "Postcode lookup", "Release checklist", "Forge deploys", "Queue workers",
    "Filament tables", "Invoice numbers", "BTW rates", "Backups", "Tracking page", "PDF invoices", "Audit log", "SMS gateway", "Search",
    "Roles", "Dashboard", "Route planner",
];
const TAGS: [&str; 5] = ["invoices", "deploys", "drivers", "security", "performance"];
const FILES: [&str; 6] = [
    "app/Exports/InvoiceExporter.php", "app/Http/Controllers/InvoiceController.php", "tests/Feature/InvoiceExportTest.php",
    "resources/views/invoices/index.blade.php", "routes/web.php", "app/Models/Invoice.php",
];
/// KADE-1's description: its demo text, more, and the fake's marker for a run that streams.
const KADE1: &str = "## Context\n\nExport invoices as CSV from the portal: Kade's office imports them in Exact Online every Friday. \
The CSV uses semicolons and a decimal comma, as Dutch Excel expects.\n\nFAKE_STREAM 50x20\n\n## Steps\n\n- [ ] first step: \
add the export button to the invoice list\n- [ ] write the rows with Filament's exporter\n- [x] agreed with the client\n\n\
## Acceptance\n\n1. The file opens in Excel with every column in its own cell.\n2. Amounts have a decimal comma and two \
decimals.\n3. An export of 10,000 invoices is ready within a minute.\n";
const KADE1_BRANCH: &str = "gizai/kade-1-export-invoices-as-csv-from-the-portal";
/// The big chat's first message, and so its title.
const BIG_FIRST: &str = "Let's plan the Kade portal release: what is left, and in what order?";

struct Opts {
    dir: PathBuf,
    fake: PathBuf,
    days: i64,
    repo: Option<PathBuf>,
    editor: Option<PathBuf>,
}

fn opts() -> Opts {
    let usage = "usage: prep_perf <data dir> <fake claude> [--years <n>] [--repo <dir>] [--editor <dir>]";
    let abs = |p: &str| std::path::absolute(p).unwrap();
    let mut a = std::env::args().skip(1);
    let (Some(dir), Some(fake)) = (a.next(), a.next()) else { panic!("{usage}") };
    let mut o = Opts { dir: abs(&dir), fake: abs(&fake), days: 90, repo: None, editor: None };
    while let Some(flag) = a.next() {
        let value = a.next().unwrap_or_else(|| panic!("{flag} needs a value; {usage}"));
        match flag.as_str() {
            "--years" => o.days = (value.parse::<f64>().expect("--years takes a number") * 365.0).round().max(1.0) as i64,
            "--repo" => o.repo = Some(abs(&value)),
            "--editor" => o.editor = Some(abs(&value)),
            _ => panic!("{usage}"),
        }
    }
    o
}

/// What the probe needs: the line prep_perf prints.
struct Ids {
    card: String,
    run: String,
    chat: String,
    note: String,
    lead: String,
}

fn main() {
    let o = opts();
    std::fs::create_dir_all(&o.dir).unwrap();
    let db = Db::open(&o.dir.join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    if projects::list(&db).unwrap().is_empty() {
        eprintln!("prep_perf: the demo data");
        demo(&db, &s.you_id, &o.dir);
    }
    let ids = match made(&db) {
        Some(ids) => {
            eprintln!("prep_perf: the data is there already, so only --repo and --editor");
            ids
        }
        None => make(&db, &s, &o),
    };
    if let Some(repo) = &o.repo {
        link_repo(&db, &s.you_id, repo);
    }
    let folder = match &o.editor {
        None => "-".to_string(),
        Some(dir) => match editor_folder(&db, &o.dir, dir) {
            Ok(id) => id,
            Err(why) => {
                drop(db);
                eprintln!("prep_perf: {why}");
                std::process::exit(1);
            }
        },
    };
    println!("card={} run={} chat={} note={} lead={} folder={folder} file=big.ts", ids.card, ids.run, ids.chat, ids.note, ids.lead);
}

/// The ids, when the data is made already: KADE-1's last run has its log, and the big chat, the hub note and the Team
/// Lead are there.
fn made(db: &Db) -> Option<Ids> {
    let card = tasks::id_of(db, "KADE-1").ok()?;
    let run = runs::list_for_task(db, &card).ok()?.into_iter().next().filter(|r| Path::new(&r.log_path).is_file())?.id;
    let chat: Option<String> = db.read(|c| Ok(c.query_row(
        "SELECT thread_id FROM chat_messages GROUP BY thread_id HAVING count(*) >= ?1 ORDER BY count(*) DESC LIMIT 1",
        [BIG_CHAT as i64], |r| r.get(0)).optional()?)).ok()?;
    let note = memory::find(db, HUB).ok()??.id;
    let lead = team::chat_agent(db).ok()??.actor_id;
    Some(Ids { card, run, chat: chat?, note, lead })
}

fn make(db: &Db, s: &SeedIds, o: &Opts) -> Ids {
    let t0 = std::time::Instant::now();
    let sanne = users::list(db).unwrap().into_iter().find(|p| p.name == "Sanne Bakker").map_or_else(|| s.you_id.clone(), |p| p.id);
    let a = agents(db, s, &o.fake);
    eprintln!("prep_perf: the cards");
    let card = cards(db, s, &a, &sanne);
    eprintln!("prep_perf: {} days of history", o.days);
    let (chat, run) = history(db, s, &a, &sanne, &o.dir, o.days, &card);
    eprintln!("prep_perf: the Memory notes");
    let note = notes(db, &s.you_id, &a);
    eprintln!("prep_perf: made in {:.1} s", t0.elapsed().as_secs_f64());
    Ids { card, run, chat, note, lead: a.lead }
}

/// The agents' ids.
struct Agents {
    lead: String,
    backend: String,
    frontend: String,
    qa: String,
}

/// The agents (an agent already there by its name is kept), the coding CLIs and Settings.
fn agents(db: &Db, s: &SeedIds, fake: &Path) -> Agents {
    settings::set(db, "claude_bin", &fake.display().to_string()).unwrap();
    settings::set(db, "agents_paused", &true).unwrap();
    settings::set(db, "max_run_tool_calls", &2_000u32).unwrap();
    // The Team Lead's chat turns run on the fake chat Claude, next to the fake.
    let chat_bin = fake.parent().unwrap().join("fake-claude-chat.py").display().to_string();
    let mut added: Vec<Cli> = clis::list(db).unwrap().into_iter().filter(|c| c.id != clis::CLAUDE_CODE).collect();
    match added.iter_mut().find(|c| c.name == CHAT_CLI) {
        Some(c) => c.command = chat_bin,
        None => added.push(Cli { name: CHAT_CLI.into(), kind: "claude_code".into(), command: chat_bin, ..Default::default() }),
    }
    let chat_cli = clis::save(db, added).unwrap().into_iter().find(|c| c.name == CHAT_CLI).unwrap().id;
    let have = team::all_agents(db).unwrap();
    let agent = |name: &str, role: &str, status: &str| {
        let id = have.iter().find(|(_, m)| m.name == name).map(|(_, m)| m.actor_id.clone()).unwrap_or_else(|| {
            team::add_agent(db, &s.you_id, &s.team_id, AgentInput { name: name.into(), role_key: role.into(),
                chat_enabled: (role == "lead").then_some(true), ..Default::default() }).unwrap()
        });
        team::set_agent_status(db, &s.you_id, &id, status).unwrap();
        id
    };
    let lead = agent("Team Lead", "lead", "active");
    let backend = agent("Backend Agent", "backend", "active");
    let frontend = agent("Frontend Agent", "frontend", "paused");
    let qa = agent("QA Agent", "qa", "paused");
    db.write(None, |w| {
        w.conn().execute("UPDATE agent_configs SET adapter = ?2 WHERE actor_id = ?1", params![lead, chat_cli])?;
        Ok(())
    }).unwrap();
    memory::ensure_lead_notes(db, &lead, "Jeffrey").unwrap();
    Agents { lead, backend, frontend, qa }
}

/// KADE's cards up to 500 (the demo's four count), over every column of the team. Returns KADE-1's id.
fn cards(db: &Db, s: &SeedIds, a: &Agents, sanne: &str) -> String {
    let team = team::get(db, &s.team_id).unwrap();
    let kade = projects::list(db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    let have = tasks::list(db, &TaskFilter { project_id: Some(kade.id.clone()), open_only: false }).unwrap().len();
    // Each column's share by its kind: Backlog and Done hold the most.
    let share = |category: &str| match category { "backlog" => 3, "ready" => 2, "done" => 4, _ => 1 };
    let deck: Vec<&team::WorkflowState> = team.states.iter().flat_map(|st| std::iter::repeat_n(st, share(&st.category))).collect();
    for k in have..CARDS {
        let st = deck[pick(1, k, deck.len())];
        let title = format!("{} {}", VERBS[k % VERBS.len()], OBJECTS[k / VERBS.len() % OBJECTS.len()]);
        let first = pick(2, k, team.labels.len().max(1));
        let labels: Vec<&Label> = (0..pick(3, k, 4).min(team.labels.len())).map(|j| &team.labels[(first + j) % team.labels.len()]).collect();
        let front = labels.iter().any(|l| l.name == "frontend");
        let who = pick(4, k, 6);
        // Half the Review and Deploy cards wait for you: the Inbox.
        let assignee = match st.category.as_str() {
            "review" | "deploy" => Some(if who < 3 { s.you_id.as_str() } else { sanne }),
            "ready" | "in_progress" if who < 2 => Some(if front { a.frontend.as_str() } else { a.backend.as_str() }),
            "testing" if who < 2 => Some(a.qa.as_str()),
            "backlog" | "done" | "cancelled" if who == 0 => Some(sanne),
            _ => None,
        };
        let id = tasks::create(db, &s.you_id, TaskInput { project_id: kade.id.clone(), title: title.clone(), description_md: description(k, &title),
            priority: pick(5, k, 5) as i64, assignee_id: assignee.map(str::to_string), label_ids: labels.iter().map(|l| l.id.clone()).collect(),
            ..Default::default() }).unwrap();
        // One open card in twelve is on hold: a question for you, or a start that couldn't work.
        let open = matches!(st.category.as_str(), "ready" | "in_progress" | "testing");
        let hold = (open && pick(6, k, 12) == 0).then(|| if k % 3 == 0 { ("blocked", BLOCKED) } else { ("needs_decision", HOLDS[k % HOLDS.len()]) });
        if st.id != team.states[0].id {
            tasks::move_to_held(db, &s.you_id, &id, &st.id, "", hold).unwrap();
        }
    }
    let kade1 = tasks::get(db, &tasks::id_of(db, "KADE-1").unwrap()).unwrap();
    if let Some(col) = team.states.iter().find(|st| st.category == "in_progress").filter(|col| col.id != kade1.state_id) {
        tasks::move_to(db, &s.you_id, &kade1.id, &col.id, "").unwrap();
    }
    tasks::update(db, &s.you_id, &kade1.id, TaskPatch { description_md: Some(KADE1.into()), assignee_id: Some(a.backend.clone()),
        ..Default::default() }).unwrap();
    kade1.id
}

/// Something that happened at a time: run i, a comment on card i, or message m of chat i (the big chat is the last).
enum Ev {
    Run(usize),
    Comment(usize),
    Message(usize, usize),
}

/// A row written now that happened earlier: queued (or written) at `at`; a run started at `started` and ended at `end`.
struct Dated {
    table: &'static str,
    id: String,
    at: i64,
    started: i64,
    end: i64,
}

impl Dated {
    fn at(table: &'static str, id: String, at: i64) -> Dated {
        Dated { table, id, at, started: at, end: at }
    }
}

/// The runs, comments and chats of the last `days` days, made in the order they happened and then set back to then;
/// last, KADE-1's run with the 5,000-event log. Returns the big chat's id and that run's.
fn history(db: &Db, s: &SeedIds, a: &Agents, sanne: &str, data: &Path, days: i64, kade1: &str) -> (String, String) {
    let now = ids::now_ms();
    let (start, end) = (now - days * DAY, now - 2 * HOUR);
    let scale = days as f64 / 90.0;
    let all = tasks::list(db, &TaskFilter::default()).unwrap();
    let kade: Vec<&Task> = all.iter().filter(|t| t.identifier.starts_with("KADE-")).collect();
    let gfw: Vec<&Task> = all.iter().filter(|t| t.identifier.starts_with("GFW-")).collect();

    let mut evs: Vec<(i64, Ev)> = vec![];
    let n_runs = (RUNS_90 * scale).round() as usize;
    let gap = (end - start) / n_runs as i64;
    for i in 0..n_runs {
        evs.push((start + i as i64 * gap + pick(10, i, gap as usize) as i64, Ev::Run(i)));
    }
    // Up to three comments a card in 90 days (KADE-1 two more than the demo's one).
    for (c, t) in all.iter().enumerate() {
        let base = if t.id == kade1 { 2 } else { pick(11, c, 4) };
        for n in 0..(base as f64 * scale).round() as usize {
            evs.push((start + pick(12, c * 10_000 + n, (end - start) as usize) as i64, Ev::Comment(c)));
        }
    }
    // The small chats end spread over the time (the newest a day before the end), a message every 4 minutes.
    let span = end - DAY - start;
    for chat in 0..SMALL_CHATS {
        let last = start + span * (2 * chat as i64 + 1) / (2 * SMALL_CHATS as i64);
        let n = ((2 + pick(13, chat, 7)) as f64 * scale).round() as usize;
        for m in 0..n {
            evs.push((last - (n - 1 - m) as i64 * 4 * MIN, Ev::Message(chat, m)));
        }
    }
    // The big chat is the newest: a message about every 3 minutes up to 5 minutes ago.
    for m in 0..BIG_CHAT {
        evs.push((now - 5 * MIN - (BIG_CHAT - 1 - m) as i64 * 170_000, Ev::Message(SMALL_CHATS, m)));
    }
    evs.sort_by_key(|(at, _)| *at);

    let authors = [s.you_id.as_str(), sanne, &a.backend, &a.frontend, &a.qa, &a.lead];
    let chat_dir = data.join("chat").display().to_string();
    let mut threads: Vec<Option<String>> = vec![None; SMALL_CHATS + 1];
    let mut dated: Vec<Dated> = vec![];
    let total = evs.len();
    for (e, (at, ev)) in evs.into_iter().enumerate() {
        match ev {
            Ev::Run(i) => dated.push(card_run(db, a, data, if pick(20, i, 10) == 0 && !gfw.is_empty() { &gfw } else { &kade }, i, at)),
            Ev::Comment(c) => {
                let id = comments::add(db, authors[pick(30, e, authors.len())], &all[c].id, &comment(e), None).unwrap();
                dated.push(Dated::at("comments", id, at));
            }
            Ev::Message(chat, m) => {
                let big = chat == SMALL_CHATS;
                let seed = chat * 100_000 + m;
                let role = role_of(m);
                let text = match (role, big && m == 0) {
                    ("user", true) => BIG_FIRST.to_string(),
                    ("user", false) => ask(seed),
                    ("agent", _) => answer(seed),
                    _ => String::new(),
                };
                let thread = threads[chat].get_or_insert_with(|| chat::create_thread(db, &s.you_id, &a.lead, &text).unwrap()).clone();
                let mut msg = NewMessage { thread_id: thread.clone(), role: role.into(), ..Default::default() };
                match role {
                    "user" => (msg.author_id, msg.body_md) = (Some(s.you_id.clone()), Some(text)),
                    "agent" => {
                        (msg.author_id, msg.body_md) = (Some(a.lead.clone()), Some(text));
                        // In the small chats each answer is a chat turn of its own (Usage counts them).
                        if !big {
                            let took = (20 + pick(31, seed, 100)) as i64 * 1_000;
                            let turn = runs::create_chat(db, &a.lead, &thread, &ids::new_id(), &chat_dir, &format!("{chat_dir}/{}.jsonl", ids::new_id())).unwrap();
                            let (input, output) = (2_000 + pick(32, seed, 40_000) as i64, 100 + pick(33, seed, 3_000) as i64);
                            runs::finish_chat(db, &turn, "succeeded", input * 3 + output * 15, input, output, None).unwrap();
                            dated.push(Dated { table: "runs", id: turn.clone(), at: at - took, started: at - took + 2_000, end: at });
                            msg.run_id = Some(turn);
                        }
                    }
                    _ => {
                        let (name, tool) = tool_call(seed);
                        (msg.author_id, msg.tool_name, msg.tool) = (Some(a.lead.clone()), Some(name), Some(tool));
                    }
                }
                dated.push(Dated::at("chat_messages", chat::add_message(db, msg).unwrap().id, at));
            }
        }
        if (e + 1) % 2_000 == 0 {
            eprintln!("prep_perf: history {}/{total}", e + 1);
        }
    }
    let run = kade1_run(db, a, data, kade1, now);
    let run_id = run.id.clone();
    dated.push(run);
    date(db, &dated);
    tidy(db, start - HOUR);
    (threads[SMALL_CHATS].clone().unwrap(), run_id)
}

/// Run `i` of the history, queued at `at`: by QA one time in five, else by the builder the card's labels name; mostly
/// succeeded, with tokens and a cost that vary (one in 50 without a cost, as a CLI that reports none).
fn card_run(db: &Db, a: &Agents, data: &Path, cards: &[&Task], i: usize, at: i64) -> Dated {
    let t = cards[pick(21, i, cards.len())];
    let qa = pick(22, i, 5) == 0;
    let (agent, role) = if qa { (&a.qa, "qa") } else if t.labels.iter().any(|l| l.name == "frontend") { (&a.frontend, "frontend") } else { (&a.backend, "backend") };
    let session = ids::new_id();
    let key = t.identifier.split('-').next().unwrap_or_default();
    let wt = data.join("worktrees").join(key).join(&t.identifier).display().to_string();
    let log = data.join("runs").join(format!("{session}.jsonl")).display().to_string();
    let branch = format!("gizai/{}-{}", t.identifier.to_lowercase(), slug(&t.title));
    let run = runs::create(db, agent, &t.id, role, &session, &wt, &wt, &branch, &log).unwrap();
    let said = |outcome: &str, issues: Vec<String>| Some(Outcome { outcome: outcome.into(), summary: SENTENCES[pick(24, i, SENTENCES.len())].into(), issues });
    let roll = pick(23, i, 100);
    let (status, outcome, error) = match roll {
        0..84 if qa && roll % 5 == 0 => ("succeeded", said("qa_fail", vec!["1. The export misses the BTW column.".into()]), None),
        0..84 if qa => ("succeeded", said("qa_pass", vec![]), None),
        0..84 => ("succeeded", said("ready_for_testing", vec![]), None),
        84..94 => ("failed", None, Some("Claude Code exited with code 1: the tests failed twice.")),
        94..98 => ("cancelled", None, Some("Stopped by Jeffrey.")),
        _ => ("timed_out", None, Some("It stopped at the time limit (90 min).")),
    };
    let (input, output) = (8_000 + pick(25, i, 180_000) as i64, 400 + pick(26, i, 16_000) as i64);
    let cost = if pick(27, i, 50) == 0 { 0 } else { input * 3 + output * 15 };
    runs::finish(db, &run, status, outcome.as_ref(), cost, input, output, error).unwrap();
    let took = (2 + pick(28, i, 44)) as i64 * MIN;
    Dated { table: "runs", id: run, at, started: at + 3_000, end: at + 3_000 + took }
}

/// KADE-1's last run, by the Backend Agent: queued 50 minutes ago and done 10 minutes ago, after KADE-1's other runs,
/// with a log of 5,000 events in <data dir>/runs/ (the data dir is absolute, so is the path).
fn kade1_run(db: &Db, a: &Agents, data: &Path, card: &str, now: i64) -> Dated {
    let session = ids::new_id();
    let log = data.join("runs").join(format!("{session}.jsonl"));
    std::fs::create_dir_all(data.join("runs")).unwrap();
    std::fs::write(&log, run_log(&session)).unwrap();
    let wt = data.join("worktrees/KADE/KADE-1").display().to_string();
    let run = runs::create_with_trigger(db, &a.backend, card, "backend", "manual", &session, &wt, &wt, KADE1_BRANCH, &log.display().to_string())
        .unwrap();
    let outcome = Outcome { outcome: "ready_for_testing".into(), summary: "The invoices export as CSV with semicolons; 41 tests pass.".into(), issues: vec![] };
    runs::finish(db, &run, "succeeded", Some(&outcome), 1_870_000, 412_000, 38_500, None).unwrap();
    Dated { table: "runs", id: run, at: now - 50 * MIN, started: now - 50 * MIN + 3_000, end: now - 10 * MIN }
}

/// Sets the rows' times back to when they happened, and their activity's: the time in each change's hlc
/// (`<unix ms>-<counter>-<device>`), a run's end for its update.
fn date(db: &Db, dated: &[Dated]) {
    db.write(None, |w| {
        let c = w.conn();
        let mut run = c.prepare("UPDATE runs SET created_at = ?2, started_at = ?3, ended_at = ?4, updated_at = ?4 WHERE id = ?1")?;
        let mut comment = c.prepare("UPDATE comments SET created_at = ?2, updated_at = ?2 WHERE id = ?1")?;
        let mut message = c.prepare("UPDATE chat_messages SET created_at = ?2 WHERE id = ?1")?;
        let mut change = c.prepare("UPDATE changes SET hlc = (CASE op WHEN 'insert' THEN ?2 ELSE ?3 END) || substr(hlc, instr(hlc, '-')) WHERE row_id = ?1")?;
        for d in dated {
            match d.table {
                "runs" => run.execute(params![d.id, d.at, d.started, d.end])?,
                "comments" => comment.execute(params![d.id, d.at])?,
                _ => message.execute(params![d.id, d.at])?,
            };
            change.execute(params![d.id, d.at, d.end])?;
        }
        Ok(())
    }).unwrap();
}

/// What follows from the dated rows: each chat from its first message to its last; the cards made before the history
/// (`before`), a quarter of an hour apart in the order they were made, changed at their last comment and held since.
fn tidy(db: &Db, before: i64) {
    db.write(None, |w| {
        let c = w.conn();
        c.execute("UPDATE chat_threads SET created_at = (SELECT min(m.created_at) FROM chat_messages m WHERE m.thread_id = chat_threads.id),
                          updated_at = (SELECT max(m.created_at) FROM chat_messages m WHERE m.thread_id = chat_threads.id)
                   WHERE EXISTS (SELECT 1 FROM chat_messages m WHERE m.thread_id = chat_threads.id)", [])?;
        c.execute("UPDATE tasks SET created_at = ?1 - (SELECT count(*) FROM tasks n WHERE n.rowid > tasks.rowid) * ?2", params![before, 15 * MIN])?;
        c.execute("UPDATE tasks SET updated_at = max(created_at, COALESCE((SELECT max(m.created_at) FROM comments m WHERE m.task_id = tasks.id), 0))", [])?;
        c.execute("UPDATE tasks SET hold_at = updated_at WHERE hold IS NOT NULL", [])?;
        for table in ["tasks", "chat_threads"] {
            c.execute(&format!("UPDATE changes SET hlc = (SELECT x.created_at FROM {table} x WHERE x.id = changes.row_id) || substr(hlc, instr(hlc, '-'))
                                WHERE table_name = '{table}' AND row_id IN (SELECT id FROM {table})"), [])?;
        }
        Ok(())
    }).unwrap();
}

/// 300 Memory notes that link each other (prep_graph's pattern), in the shared folders and the Team Lead's: each links
/// two or three others, every tenth the hub (Projects/Kade portal, which links twelve), every 40th a note that isn't
/// there yet; every seventh names a KADE card, every 25th mentions the Backend Agent, every eighth has a #tag and every
/// fifth properties. Returns the hub's id.
fn notes(db: &Db, you: &str, a: &Agents) -> String {
    let handle = team::agent(db, &a.backend).unwrap().handle;
    let (as_you, as_lead) = (Who::Person(you.into()), Who::Lead(a.lead.clone()));
    let title = |i: usize| if i == 0 { "Kade portal".to_string() } else { format!("{} {i}", SUBJECTS[i % SUBJECTS.len()]) };
    let mut hub = String::new();
    for i in 0..NOTES {
        let folder = if i == 0 { "Projects" } else { FOLDERS[i % FOLDERS.len()] };
        let mut body = String::new();
        if i % 5 == 0 {
            body += &format!("---\ntype: {}\ntags: [{}]\n---\n", if folder == "Decisions" { "decision" } else { "note" }, TAGS[i % TAGS.len()]);
        }
        body += &format!("# {}\n\n{}\n\nSee [[{}]] and [[{}]].", title(i), para(40, i, 2 + i % 3), title((i * 7 + 1) % NOTES), title((i * 13 + 5) % NOTES));
        if i == 0 {
            body += &format!("\n\n## Areas\n\n{}", (1..=12).map(|j| format!("- [[{}]]\n", title(j * 23 % NOTES))).collect::<String>());
        }
        if i % 3 == 0 {
            body += &format!(" Next: [[{}]].", title((i + 1) % NOTES));
        }
        if i % 10 == 0 && i > 0 {
            body += " Part of [[Kade portal]].";
        }
        if i % 40 == 0 {
            body += &format!(" Not written yet: [[Missing {i}]].");
        }
        if i % 7 == 0 {
            body += &format!(" First seen on KADE-{}.", 1 + i % 120);
        }
        if i % 25 == 0 {
            body += &format!(" Asked @{handle}.");
        }
        if i % 4 == 0 {
            body += &format!("\n\n## Steps\n\n{}", list(41, i, 3, "- "));
        }
        if i % 8 == 0 {
            body += &format!("\n\n#{}", TAGS[i % TAGS.len()]);
        }
        let who = if folder == "Team Lead" || i % 2 == 1 { &as_lead } else { &as_you };
        let id = memory::write(db, who, &format!("{folder}/{}", title(i)), &body, None, None).unwrap().id;
        if i == 0 {
            hub = id;
        }
        if (i + 1) % 100 == 0 {
            eprintln!("prep_perf: notes {}/{NOTES}", i + 1);
        }
    }
    hub
}

/// --repo: KADE's linked folder, a git repository made (main, one empty commit) when it isn't there yet.
fn link_repo(db: &Db, you: &str, repo: &Path) {
    if !repo.join(".git").exists() {
        std::fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "-q", "-b", "main"]);
        git(repo, &["commit", "-q", "--allow-empty", "-m", "Start"]);
    }
    let p = projects::list(db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap();
    projects::update(db, you, &p.id, ProjectInput { name: p.name.clone(), key: p.key.clone(), client_id: p.client_id.clone(),
        repo_path: Some(repo.display().to_string()), default_branch: Some("main".into()), color: p.color.clone(), goal_md: p.goal_md.clone(),
        ..Default::default() }).unwrap();
}

/// --editor: <dir>/big (made once), <dir> as the Editor folder, and big's id as the Editor lists it (its canonical path),
/// with HOME and XDG_DATA_HOME as the app reads them (`Places`, as src-tauri/src/editor.rs builds it). Else why not.
fn editor_folder(db: &Db, data: &Path, dir: &Path) -> Result<String, String> {
    let big = dir.join("big");
    if tracked(&big) != BIG_FILES + 1 {
        eprintln!("prep_perf: making {} (20,000 files)", big.display());
        make_big(&big);
    }
    let places = Places::without_checkouts(db);
    editor::set_roots(db, &[dir.display().to_string()], &places).map_err(|e| e.to_string())?;
    let want = folders::on_disk(&big).ok_or_else(|| format!("{} isn't there", big.display()))?.display().to_string();
    let worktrees = data.join("worktrees");
    let listed = editor::folders(db, &places, &worktrees, &HashMap::new()).map_err(|e| e.to_string())?;
    match listed.into_iter().find(|f| f.id == want) {
        Some(f) => Ok(f.id),
        None => Err(editor::find(db, &places, &worktrees, &HashMap::new(), &want).err().unwrap_or_else(|| format!("the Editor doesn't list {want}"))),
    }
}

/// <dir>/big: 20,000 small source files in 200 folders (src/area00/module0 to src/area19/module9, 100 each) and big.ts
/// of 5,000 lines at its top, in one commit. Written again over what is there when an earlier run didn't finish.
fn make_big(big: &Path) {
    for f in 0..BIG_FILES {
        let dir = big.join(format!("src/area{:02}/module{}", f / 1_000, f / 100 % 10));
        if f % 100 == 0 {
            std::fs::create_dir_all(&dir).unwrap();
        }
        let body = format!("// area {}, module {}, file {}\nimport {{ compute }} from \"../../../big\";\n\nexport function part{f}(a: number): number {{\n  return compute(a, {f}) + {};\n}}\n",
                           f / 1_000, f / 100 % 10, f % 100, f % 7);
        std::fs::write(dir.join(format!("file{:02}.ts", f % 100)), body).unwrap();
    }
    let mut ts = String::from("export function compute(a: number, b: number | string): number {\n  return a + String(b).length;\n}\n");
    for i in 4..=5_000 {
        ts += &if i % 25 == 0 { "\n".to_string() } else { format!("export const value{i} = compute({i}, \"item {i}\");\n") };
    }
    std::fs::write(big.join("big.ts"), ts).unwrap();
    if !big.join(".git").exists() {
        git(big, &["init", "-q", "-b", "main"]);
    }
    git(big, &["add", "-A"]);
    git(big, &["commit", "-q", "--allow-empty", "-m", "20,000 files and big.ts"]);
}

/// How many files git tracks in `dir` (0 when it isn't a repository of its own).
fn tracked(dir: &Path) -> usize {
    if !dir.join(".git").exists() {
        return 0;
    }
    Command::new("git").arg("-C").arg(dir).args(["ls-files", "-z"]).output().ok()
        .filter(|o| o.status.success()).map_or(0, |o| o.stdout.iter().filter(|b| **b == 0).count())
}

/// Runs git in `dir` as t <t@t>, without hooks or signing; its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir)
        .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"])
        .args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?} in {}: {}", dir.display(), String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The demo data, as the demo example makes it.
fn demo(db: &Db, you: &str, dir: &Path) {
    users::create(db, you, "Sanne Bakker", Some("sanne@studio.example")).unwrap();
    let kade = clients::create(db, you, ClientInput { name: "Kade Logistics B.V.".into(), city: Some("Amsterdam".into()),
        coc_number: Some("34098812".into()), vat_number: Some("NL004512378B01".into()), ..Default::default() }).unwrap();
    let fiets = clients::create(db, you, ClientInput { name: "De Groene Fiets".into(), city: Some("Utrecht".into()),
        coc_number: Some("68873310".into()), vat_number: Some("NL857712093B01".into()), ..Default::default() }).unwrap();
    clients::create(db, you, ClientInput { name: "Atelier Mees".into(), city: Some("Haarlem".into()), status: Some("lead".into()), ..Default::default() }).unwrap();
    clients::upsert_contact(db, you, Contact { client_id: kade.clone(), name: "Marloes Visser".into(), role: Some("Head of finance".into()),
        email: Some("marloes@kade-logistics.example".into()), is_primary: true, ..Default::default() }).unwrap();
    let p1 = projects::create(db, you, ProjectInput { name: "Kade portal".into(), key: "KADE".into(), client_id: Some(kade),
        color: Some("#e8c547".into()), goal_md: Some("Give Kade's office one place for shipments, invoices and drivers.".into()), ..Default::default() }).unwrap();
    let p2 = projects::create(db, you, ProjectInput { name: "Groene Fiets webshop".into(), key: "GFW".into(), client_id: Some(fiets),
        color: Some("#4fc1a6".into()), ..Default::default() }).unwrap();
    let team = team::get(db, &team::list(db).unwrap()[0].id).unwrap();
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
        let id = tasks::create(db, you, TaskInput { project_id: p.to_string(), title: title.into(), priority: prio,
            description_md: format!("## Context\n\n{title}.\n\n- [ ] first step\n- [x] agreed with the client"),
            label_ids: labels.iter().map(|l| label(l)).collect(), ..Default::default() }).unwrap();
        tasks::move_to(db, you, &id, &col(column), "").unwrap();
        if prio == 2 {
            comments::add(db, you, &id, "Use Filament's exporter. Sample file from Marloes is attached.", None).unwrap();
        }
    }
    let req = docs::create(db, you, &p1, "Requirements").unwrap();
    docs::save(db, you, &req, "# Requirements\n\nThe portal replaces the Excel lists.\n\n## Invoices\n\n- [x] List and filter invoices\n- [ ] CSV export with semicolons (KADE-1)\n\nAsk @sanne-bakker about the driver app.", 1).unwrap();
    docs::save(db, you, &req, "# Requirements\n\nThe portal replaces the Excel lists Kade's office keeps today.\n\n## Invoices\n\n- [x] List and filter invoices\n- [ ] CSV export with semicolons (KADE-1)\n\n## Drivers\n\nAsk @sanne-bakker about the driver app.", 2).unwrap();
    docs::create(db, you, &p1, "Meeting notes 2026-10-02").unwrap();
    let sample = dir.join("invoices-sample.csv");
    std::fs::write(&sample, "number;date;amount\n2026-0412;2026-09-30;1250,00\n").unwrap();
    files::add_from_path(db, you, dir, "project", &p1, &sample).unwrap();
    std::fs::remove_file(&sample).unwrap();
}

// ---- Deterministic text -------------------------------------------------------------------------------------------

/// A number below `n` that looks random but is the same every time for the same `tag` and `i` (splitmix64).
fn pick(tag: u64, i: usize, n: usize) -> usize {
    let mut x = tag.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (i as u64);
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((x ^ (x >> 31)) % n.max(1) as u64) as usize
}

fn one<'a>(list: &[&'a str], tag: u64, i: usize) -> &'a str {
    list[pick(tag, i, list.len())]
}

/// `n` sentences.
fn para(tag: u64, i: usize, n: usize) -> String {
    (0..n).map(|j| one(&SENTENCES, tag, i * 16 + j)).collect::<Vec<_>>().join(" ")
}

/// `n` steps, each on a line after `bullet`.
fn list(tag: u64, i: usize, n: usize, bullet: &str) -> String {
    (0..n).map(|j| format!("{bullet}{}\n", one(&STEPS, tag, i * 16 + j))).collect()
}

fn code(i: usize) -> String {
    let (lang, code) = CODE[i % CODE.len()];
    format!("```{lang}\n{code}\n```\n")
}

/// A card's description: 1 to 3 KB of Markdown (context, steps, notes, acceptance and now and then a code block).
fn description(k: usize, title: &str) -> String {
    let want = 1_000 + pick(80, k, 1_600);
    let mut s = format!("## Context\n\n{title}. {}\n", para(81, k, 3));
    let mut part = 0;
    while s.len() < want {
        let j = k * 32 + part;
        s += &match part % 4 {
            0 => format!("\n## Steps\n\n{}", list(82, j, 3 + j % 3, "- [ ] ")),
            1 if k % 3 == 0 => format!("\n{}", code(j)),
            1 | 2 => format!("\n### Notes\n\n{}\n", para(83, j, 2)),
            _ => format!("\n## Acceptance\n\n{}", (1..=3).map(|n| format!("{n}. {}\n", one(&SENTENCES, 84, j * 4 + n))).collect::<String>()),
        };
        part += 1;
    }
    s
}

/// A comment: a remark, sometimes with more, a checklist or a code block.
fn comment(i: usize) -> String {
    let mut s = one(&REMARKS, 70, i).to_string();
    match pick(71, i, 5) {
        0 | 1 => s += &format!(" {}", one(&SENTENCES, 72, i)),
        2 => s += &format!("\n\n{}", list(73, i, 2 + i % 3, "- [ ] ")),
        3 if i % 3 == 0 => s += &format!("\n\n{}", code(i)),
        _ => {}
    }
    s
}

/// Who writes message `m` of a chat: every tenth is a tool call of the Team Lead's, the others are yours and its in turn.
fn role_of(m: usize) -> &'static str {
    if m % 10 == 9 {
        "tool"
    } else if (m - (m + 1) / 10) % 2 == 0 {
        "user"
    } else {
        "agent"
    }
}

/// A message of yours: a question or a request, sometimes with more.
fn ask(i: usize) -> String {
    let s = one(&ASKS, 50, i).to_string();
    match pick(51, i, 4) {
        0 => format!("{s} {}", para(52, i, 2)),
        1 => format!("{s}\n\n{}", list(53, i, 3, "- ")),
        _ => s,
    }
}

/// An answer of the Team Lead's: from a line to a few screens of Markdown (paragraphs, lists, code, a table).
fn answer(i: usize) -> String {
    let mut s = para(61, i, 1 + pick(62, i, 3));
    for p in 1..1 + pick(60, i, 6) {
        let j = i * 8 + p;
        s += &match pick(63, j, 4) {
            0 => format!("\n\n{}", list(64, j, 2 + pick(65, j, 4), "- ")),
            1 => format!("\n\n{}", code(j)),
            2 => format!("\n\n| Card | Column | Agent |\n|---|---|---|\n{}", (0..3).map(|r| format!("| KADE-{} | {} | {} |\n", 1 + pick(66, j * 4 + r, 480),
                ["To do", "In progress", "Testing", "Review"][pick(67, j * 4 + r, 4)], ["Backend Agent", "Frontend Agent", "QA Agent"][pick(68, j * 4 + r, 3)]))
                .collect::<String>()),
            _ => format!("\n\n## {}\n\n{}", one(&SUBJECTS, 69, j), para(70, j, 2)),
        };
    }
    s
}

/// A tool call of the Team Lead's, as the app saves one: no text, the tool's name, and its input and result.
fn tool_call(i: usize) -> (String, Value) {
    let n = 1 + i % 480;
    let (name, input, result) = match i % 5 {
        0 => ("list_tasks", json!({"project": "KADE", "column": "In progress"}), format!("{{\"count\": 38, \"shown\": 38, \"tasks\": [\"KADE-{n}\", \"KADE-{}\"]}}", n + 1)),
        1 => ("get_task", json!({"task": format!("KADE-{n}")}), format!("{{\"identifier\": \"KADE-{n}\", \"column\": \"Testing\", \"description_md\": \"{}\"}}", one(&SENTENCES, 90, i))),
        2 => ("update_task", json!({"task": format!("KADE-{n}"), "priority": 2}), format!("Updated KADE-{n}.")),
        3 => ("memory_search", json!({"query": "csv export"}), "[{\"path\": \"Standards/CSV export 20\", \"snippet\": \"The CSV uses semicolons\"}]".to_string()),
        _ => ("check_board", json!({}), "{\"findings\": 12, \"new\": 3}".to_string()),
    };
    (format!("mcp__gizai__{name}"), json!({"id": format!("toolu_{i}"), "input": input, "result": result, "isError": false}))
}

/// KADE-1's last run's log, as the app keeps one (Claude Code's stream-json): 5,000 lines that are 5,000 events in the
/// Run panel. The init line; then text, a tool call and its result in turn; then the result line.
fn run_log(session: &str) -> String {
    let line = |v: Value| format!("{v}\n");
    let message = |role: &str, content: Value| json!({"type": role, "message": {"role": role, "content": [content]}, "session_id": session});
    let mut out = line(json!({"type": "system", "subtype": "init", "session_id": session, "model": "claude-opus-5-5",
                              "tools": ["Read", "Edit", "Write", "Bash", "Grep", "Glob"]}));
    for i in 0..LOG_EVENTS - 2 {
        let step = i / 3;
        let id = format!("toolu_{step:05}");
        let file = FILES[step % FILES.len()];
        let (tool, input) = match step % 6 {
            0 => ("Read", json!({"file_path": file})),
            1 => ("Grep", json!({"pattern": "InvoiceExporter", "path": "app"})),
            2 => ("Bash", json!({"command": "php artisan test --filter=InvoiceExport", "description": "Run the exporter tests"})),
            3 => ("Edit", json!({"file_path": file, "old_string": "return [];", "new_string": "return $this->rows();"})),
            4 => ("Glob", json!({"pattern": "tests/Feature/**/*Export*Test.php"})),
            _ => ("Write", json!({"file_path": file, "content": "<?php\n\nnamespace App\\Exports;\n"})),
        };
        out += &line(match i % 3 {
            0 if step % 10 == 0 => message("assistant", json!({"type": "text", "text": format!("Step {step}. {}\n\n{}", para(91, step, 2), list(92, step, 3, "- "))})),
            0 => message("assistant", json!({"type": "text", "text": one(&SENTENCES, 93, step)})),
            1 => message("assistant", json!({"type": "tool_use", "id": id, "name": tool, "input": input})),
            _ => {
                let content = match tool {
                    "Read" => format!("<?php\n\nnamespace App\\Exports;\n\nclass InvoiceExporter\n{{\n    // {}\n}}\n", one(&SENTENCES, 94, step)),
                    "Bash" => format!("PASS  Tests\\Feature\\InvoiceExportTest\n✓ exports {} rows\n\nTests: {} passed", 10 + step % 90, 30 + step % 12),
                    "Grep" => format!("{file}:12: class InvoiceExporter\n{file}:40: new InvoiceExporter($rows)"),
                    _ => format!("The file {file} has been updated."),
                };
                message("user", json!({"type": "tool_result", "tool_use_id": id, "content": content, "is_error": step % 37 == 36}))
            }
        });
    }
    let result = "The export is done.\nGIZAI_RESULT: {\"outcome\":\"ready_for_testing\",\"summary\":\"The invoices export as CSV with semicolons; 41 tests pass.\",\"issues\":[]}";
    out + &line(json!({"type": "result", "subtype": "success", "is_error": false, "result": result, "total_cost_usd": 1.87, "num_turns": 1_666,
                       "session_id": session, "usage": {"input_tokens": 412_000, "output_tokens": 38_500}}))
}

/// A card title as a branch name's end: "rate-limit-the-public-tracking".
fn slug(title: &str) -> String {
    let s: String = title.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).take(6).collect::<Vec<_>>().join("-")
}
