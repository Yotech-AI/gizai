//! The fixtures of the browser mode (GA-185): GizAi's screens in a plain browser, answered by a fake backend (Tauri's
//! mockIPC, `src/browser/`), from the demo data and what this adds to it:
//! - the Team Lead, Frontend Agent, Backend Agent and QA Agent; the Frontend and QA Agents have the hidden browser and
//!   Take screenshots on, the Backend Agent neither;
//! - Groene Fiets webshop linked to GitHub, with a Preview command and the default screenshot sizes; Kade portal has no
//!   Preview command;
//! - GFW-2 in Review: the Frontend Agent's card with pull request #41 and six runs' screenshot sets (no screen changed, a
//!   preview that didn't start, files left out, QA's failure and pass, a picture unchanged between two runs, one not
//!   taken again), assigned to you after QA's pass, so the Inbox has it;
//! - KADE-2 in Testing with one set; KADE-3 in Review with a pull request and no screenshots; the empty tab's cases: KADE-1
//!   (no preview), GFW-1 (the Backend Agent's: no browser), GFW-3 (its own switch off), GFW-5 (no run yet);
//! - Settings → General → Backups: a folder, daily, Keep 10, and three snapshots, one from another computer.
//!
//! Times are set back as they would read in use: the demo data 5 days ago, the runs from 2 days to 2 hours ago. Each
//! command's answer has the shape the app's own command returns: gizai-core's types, and for the commands that live in
//! src-tauri its structs mirrored by hand. The mock shifts every time in it by how long ago the file was written.
//! usage: cargo run -p gizai-core --example demo -- <dir>
//!        cargo run -p gizai-core --example fixtures -- <dir> src/browser/fixtures.json
use std::path::{Path, PathBuf};
use std::process::exit;

use gizai_core::db::Db;
use gizai_core::files::{self, Evidence};
use gizai_core::mcp_servers::{self, AgentServer, AgentTools};
use gizai_core::model::*;
use gizai_core::{backups, clients, clis, comments, db, docs, editor, ids, labels, memory, onboarding, projects, pulls, questions, runs,
    screenshots, seed, settings, tasks, team, tokens, users, workflow};
use serde::Serialize;
use serde_json::{Map, Value, json};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;
/// Gizai's data folder as the fixtures name it (no real folder).
const DATA: &str = "/home/you/.local/share/gizai-browser";
/// Groene Fiets webshop's checkout and its repository on GitHub.
const GFW_REPO: &str = "/home/you/Herd/groene-fiets-webshop";
const GFW_GITHUB: &str = "https://github.com/groene-fiets/webshop";
/// What a card without a branch shows on its Changes tab (`NO_BRANCH` in src-tauri/src/changes.rs).
const NO_BRANCH: &str = "No changes yet: the first run makes this card's branch.";

/// Moves every time in the database from `from` to `to` back by `by` ms: each `…_at` column of every table, and the time
/// in each change's hlc (the activity's).
fn shift(db: &Db, from: i64, to: i64, by: i64) {
    db.write(None, |w| {
        let c = w.conn();
        let tables: Vec<String> = c
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite%' AND sql NOT LIKE 'CREATE VIRTUAL%'")?
            .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        for t in tables {
            let cols: Vec<String> = c.prepare("SELECT name FROM pragma_table_info(?1) WHERE name LIKE '%\\_at' ESCAPE '\\'")?
                .query_map([&t], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for col in cols {
                c.execute(&format!("UPDATE \"{t}\" SET \"{col}\" = \"{col}\" - ?1 WHERE typeof(\"{col}\") = 'integer' AND \"{col}\" BETWEEN ?2 AND ?3"),
                          rusqlite::params![by, from, to])?;
            }
        }
        c.execute("UPDATE changes SET hlc = (CAST(substr(hlc, 1, instr(hlc, '-') - 1) AS INTEGER) - ?1) || substr(hlc, instr(hlc, '-'))
                   WHERE CAST(substr(hlc, 1, instr(hlc, '-') - 1) AS INTEGER) BETWEEN ?2 AND ?3", rusqlite::params![by, from, to])?;
        Ok(())
    }).unwrap();
}

/// Does `f` as if at `when`: every time it writes is moved back to then.
fn at<T>(db: &Db, when: i64, f: impl FnOnce() -> T) -> T {
    let start = ids::now_ms();
    let out = f();
    shift(db, start, ids::now_ms(), start - when);
    out
}

/// A fake commit id, the same for the same label.
fn sha(label: &str) -> String {
    tokens::sha256_hex(label)[..40].to_string()
}

/// A screenshot a run kept. `look` makes its bytes: the same look is the same picture (the same hash).
struct Shot {
    name: &'static str,
    caption: &'static str,
    w: u32,
    h: u32,
    look: &'static str,
}

fn desktop(name: &'static str, caption: &'static str, look: &'static str) -> Shot {
    Shot { name, caption, w: 1280, h: 800, look }
}

fn phone(name: &'static str, caption: &'static str, look: &'static str) -> Shot {
    Shot { name, caption, w: 390, h: 844, look }
}

/// Stand-in bytes of about a real screenshot's size (not an image: the browser mode draws its own placeholders).
fn picture(s: &Shot) -> Vec<u8> {
    let text = format!("GizAi browser-mode fixture: {} {}x{}\n", s.look, s.w, s.h);
    let vary = usize::from_str_radix(&tokens::sha256_hex(s.look)[..4], 16).unwrap();
    let size = (s.w * s.h) as usize / 6 + vary;
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    while b.len() < size {
        b.extend_from_slice(text.as_bytes());
    }
    b.truncate(size);
    b
}

/// One finished run on a card.
struct Spec<'a> {
    agent: &'a str,
    role: &'a str,
    task: &'a str,
    branch: &'a str,
    /// Its worktree under Gizai's data folder: `<KEY>/<identifier>`.
    worktree: &'a str,
    trigger: &'a str,
    /// When it was queued, and how long it ran.
    at: i64,
    minutes: i64,
    status: &'a str,
    outcome: Option<Outcome>,
    error: Option<&'a str>,
    /// Its cost in µUSD, its input and output tokens.
    usage: (i64, i64, i64),
    /// The commits it started and ended at.
    base: &'a str,
    head: &'a str,
    shots: Vec<Shot>,
    left_out: Vec<&'a str>,
    preview: Option<&'a str>,
}

fn outcome(outcome: &str, summary: &str, issues: &[&str]) -> Option<Outcome> {
    Some(Outcome { outcome: outcome.into(), summary: summary.into(), issues: issues.iter().map(|i| i.to_string()).collect() })
}

/// Records the run as Gizai does: queued, then ended with its verdict (its summary, with QA's issues, on the card as the
/// agent's comment), its commits, the Screenshots section and the screenshots it kept. Returns its id and when it ended.
fn record(db: &Db, dir: &Path, r: Spec) -> (String, i64) {
    let wt = format!("{DATA}/worktrees/{}", r.worktree);
    let session = ids::new_id();
    let id = at(db, r.at, || {
        runs::create_with_trigger(db, r.agent, r.task, r.role, r.trigger, &session, &wt, &wt, r.branch, &format!("{DATA}/runs/{session}.jsonl"))
            .unwrap()
    });
    let ended = r.at + r.minutes * MIN;
    at(db, ended, || {
        runs::set_base_sha(db, &id, r.base).unwrap();
        let (cost, input, output) = r.usage;
        runs::finish(db, &id, r.status, r.outcome.as_ref(), cost, input, output, r.error).unwrap();
        runs::set_head_sha(db, &id, r.head).unwrap();
        screenshots::begin(db, &id).unwrap();
        for s in &r.shots {
            let blob = files::store_bytes(dir, s.name, &picture(s)).unwrap();
            db.write(Some(r.agent), |w| {
                files::insert_evidence(w, r.agent, r.task, &blob, &Evidence { run_id: &id, caption: s.caption, mime: "image/png", width: s.w, height: s.h })
            }).unwrap();
        }
        if !r.left_out.is_empty() {
            screenshots::set_left_out(db, &id, &r.left_out.iter().map(|l| l.to_string()).collect::<Vec<_>>()).unwrap();
        }
        if let Some(why) = r.preview {
            screenshots::set_preview_why(db, &id, why).unwrap();
        }
        if let Some(o) = &r.outcome {
            let mut body = o.summary.clone();
            if !o.issues.is_empty() {
                body.push_str("\n\n");
                body.push_str(&o.issues.iter().enumerate().map(|(i, x)| format!("{}. {x}", i + 1)).collect::<Vec<_>>().join("\n"));
            }
            comments::add(db, r.agent, r.task, &body, Some(&id)).unwrap();
        }
        db.write(None, |w| {
            w.conn().execute("UPDATE runs SET started_at = ?2 WHERE id = ?1", rusqlite::params![id, r.at + 40_000])?;
            Ok(())
        }).unwrap();
    });
    (id, ended)
}

/// git's hunks between two texts as `git diff -U0` prints them (DiffHunk): a side without lines starts at the line before.
fn hunks(old: &str, new: &str) -> Vec<Value> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        let (i0, j0) = (i, j);
        while (i < n || j < m) && !(i < n && j < m && a[i] == b[j]) {
            if j == m || (i < n && lcs[i + 1][j] >= lcs[i][j + 1]) { i += 1 } else { j += 1 }
        }
        let (del, add) = (i - i0, j - j0);
        out.push(json!({"oldStart": if del == 0 { i0 } else { i0 + 1 }, "oldLines": del, "newStart": if add == 0 { j0 } else { j0 + 1 }, "newLines": add}));
    }
    out
}

/// A changed file (FileChange) and its diff (FileDiff); `old` None for an added file.
fn changed(path: &str, old: Option<&str>, new: &str) -> (Value, Value) {
    let h = hunks(old.unwrap_or(""), new);
    let sum = |k: &str| h.iter().map(|x| x[k].as_u64().unwrap()).sum::<u64>();
    let file = json!({"path": path, "oldPath": null, "status": if old.is_some() { "modified" } else { "added" }, "additions": sum("newLines"),
                      "deletions": sum("oldLines"), "binary": false, "uncommitted": false});
    let diff = json!({"path": path, "oldText": old, "newText": new, "hunks": h, "patch": null, "binary": false, "tooLarge": false,
                      "oldSize": old.map_or(0, str::len), "newSize": new.len(), "crlf": false});
    (file, diff)
}

const PRICE_SLIDER: &str = r#"import { useId } from "react";

type Props = { min: number; max: number; value: [number, number]; onChange: (range: [number, number]) => void };

const euro = (n: number) => `€ ${n.toLocaleString("nl-NL")}`;

/** Two thumbs on one track, in steps of € 10. Prices with 21% BTW, as the shop shows them. */
export function PriceSlider({ min, max, value, onChange }: Props) {
  const id = useId();
  const [from, to] = value;
  return (
    <fieldset className="price-slider" aria-describedby={`${id}-range`}>
      <legend>Price</legend>
      <input type="range" aria-label="Lowest price" min={min} max={max} step={10} value={from}
        onChange={(e) => onChange([Math.min(Number(e.target.value), to), to])} />
      <input type="range" aria-label="Highest price" min={min} max={max} step={10} value={to}
        onChange={(e) => onChange([from, Math.max(Number(e.target.value), from)])} />
      <p id={`${id}-range`}>{euro(from)} to {euro(to)}</p>
    </fieldset>
  );
}
"#;

const PRODUCTS_OLD: &str = r#"import { Head } from "@inertiajs/react";
import { ProductCard } from "../components/ProductCard";
import type { Product } from "../types";

type Props = { products: Product[] };

export default function Products({ products }: Props) {
  return (
    <>
      <Head title="Bikes" />
      <h1>Bikes</h1>
      <ul className="product-grid">
        {products.map((p) => (
          <li key={p.id}><ProductCard product={p} /></li>
        ))}
      </ul>
    </>
  );
}
"#;

const PRODUCTS_NEW: &str = r#"import { useState } from "react";
import { Head } from "@inertiajs/react";
import { PriceSlider } from "../components/PriceSlider";
import { ProductCard } from "../components/ProductCard";
import type { Product } from "../types";

type Props = { products: Product[]; prices: { min: number; max: number } };

export default function Products({ products, prices }: Props) {
  const [range, setRange] = useState<[number, number]>([prices.min, prices.max]);
  const shown = products.filter((p) => p.priceWithVat >= range[0] && p.priceWithVat <= range[1]);
  return (
    <>
      <Head title="Bikes" />
      <h1>Bikes</h1>
      <PriceSlider min={prices.min} max={prices.max} value={range} onChange={setRange} />
      {shown.length === 0 && <p className="empty">No bikes in this range: widen it, or try another price.</p>}
      <ul className="product-grid">
        {shown.map((p) => (
          <li key={p.id}><ProductCard product={p} /></li>
        ))}
      </ul>
    </>
  );
}
"#;

/// Claude Code's tools in Gizai's catalog (crates/gizai-agents/src/tool_catalog.rs), a part of them: (id, label, group,
/// description, risk, how, note).
const CATALOG: [(&str, &str, &str, &str, &str, &str, &str); 13] = [
    ("WebSearch", "Web search", "web", "Searches the web. Results are untrusted text, and the search terms leave your computer.", "medium", "web", ""),
    ("WebFetch", "Fetch web pages", "web", "Reads web pages, any or only the domains you list. Pages are untrusted, and an address can carry data out.", "high", "web", ""),
    ("Read", "Read files", "files", "Reads files in its worktree and its folders.", "low", "always", "Claude Code uses it without asking."),
    ("Glob", "Find files", "files", "Finds files by name pattern.", "low", "always", "Claude Code uses it without asking."),
    ("Grep", "Search in files", "files", "Searches file contents.", "low", "always", "Claude Code uses it without asking."),
    ("Edit", "Edit files", "files", "Changes files in its worktree and read-and-change folders.", "medium", "elsewhere", "On through the permission mode (Permissions → Permission mode)."),
    ("Write", "Write files", "files", "Creates or overwrites files in its worktree and read-and-change folders.", "medium", "elsewhere", "On through the permission mode (Permissions → Permission mode)."),
    ("Bash", "Run commands", "commands", "Runs shell commands.", "high", "elsewhere", "Only the commands under Permissions → Allowed commands."),
    ("Task", "Subagents", "agents", "Hands part of the work to a subagent with the same permissions. Each subagent costs more.", "medium", "always", "Claude Code uses it without asking."),
    ("TodoWrite", "To-do list", "planning", "Keeps its own list of steps for the run.", "low", "always", "Claude Code uses it without asking."),
    ("AskUserQuestion", "Ask you a question", "planning", "Asks you to pick an answer.", "low", "off", "A headless run can't ask: the agent asks in its result line (needs_decision) instead."),
    ("SlashCommand", "Slash commands and skills", "other", "Lets the agent run your slash commands and skills, like /design, including those from plugins; they can steer the run.", "medium", "slash", ""),
    ("Skill", "Slash commands and skills", "other", "Lets the agent run your slash commands and skills, like /design, including those from plugins; they can steer the run.", "medium", "slash", ""),
];

fn val(x: impl Serialize) -> Value {
    serde_json::to_value(x).unwrap()
}

/// A command whose answer depends on one argument: `{ "$by": arg, "$answers": { value: answer } }`.
fn by(arg: &str, answers: impl IntoIterator<Item = (String, Value)>) -> Value {
    json!({"$by": arg, "$answers": answers.into_iter().collect::<Map<String, Value>>()})
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(dir), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: fixtures <the demo example's data dir> <fixture file, like src/browser/fixtures.json>");
        exit(2);
    };
    let dir = PathBuf::from(dir);
    let db = Db::open(&dir.join("gizai.db")).unwrap();
    let s = seed::ensure_seed(&db, "Jeffrey").unwrap();
    let you = s.you_id.clone();
    if !team::all_agents(&db).unwrap().is_empty() {
        eprintln!("{} has agents already: run this once, on the demo example's fresh data", dir.display());
        exit(1);
    }
    let project = |key: &str| projects::list(&db).unwrap().into_iter().find(|p| p.key == key)
        .unwrap_or_else(|| { eprintln!("no project {key}: run the demo example into {} first", dir.display()); exit(1) });
    project("KADE");
    let gfw = project("GFW");
    let card = |identifier: &str| tasks::id_of(&db, identifier).unwrap();
    let (kade1, kade2, kade3) = (card("KADE-1"), card("KADE-2"), card("KADE-3"));
    let (gfw1, gfw2, gfw3) = (card("GFW-1"), card("GFW-2"), card("GFW-3"));
    let board = team::get(&db, &s.team_id).unwrap();
    let col = |name: &str| board.states.iter().find(|x| x.name == name).unwrap().id.clone();
    let frontend_label = board.labels.iter().find(|l| l.name == "frontend").unwrap().id.clone();

    // The demo data reads as made 5 days ago.
    let now = ids::now_ms();
    let latest: i64 = db.read(|c| Ok(c.query_row("SELECT max(CAST(substr(hlc, 1, instr(hlc, '-') - 1) AS INTEGER)) FROM changes", [], |r| r.get(0))?))
        .unwrap();
    shift(&db, 0, now, latest - (now - 5 * DAY));

    // The agents, as a new install's (`seed::ensure_seed_with_agents`): each on its role's usual columns.
    let (lead, fe, be, qa) = at(&db, now - 4 * DAY - 2 * HOUR, || {
        let add = |name: &str, role: &str| team::add_agent(&db, &you, &s.team_id, AgentInput {
            name: name.into(), role_key: role.into(), instructions_md: Some(seed::role_template(role)), allowed_tools: seed::role_tools(role),
            permission_mode: "acceptEdits".into(), chat_enabled: (role == "lead").then_some(true), max_runs: Some(1), ..Default::default()
        }).unwrap();
        (add("Team Lead", "lead"), add("Frontend Agent", "frontend"), add("Backend Agent", "backend"), add("QA Agent", "qa"))
    });
    at(&db, now - 4 * DAY - HOUR, || {
        for a in [&fe, &qa] {
            mcp_servers::set_agent_tools(&db, &you, a, AgentTools { mcp: vec![AgentServer { server_id: mcp_servers::BROWSER.into(), on: true, tools_off: vec![] }] })
                .unwrap();
            mcp_servers::set_agent_screenshots(&db, &you, a, true).unwrap();
        }
        mcp_servers::set_agent_screenshots(&db, &you, &be, false).unwrap();
        settings::set(&db, "claude_bin", &"/home/you/.local/bin/claude").unwrap();
    });
    // Groene Fiets webshop: its repository and Preview; Kade portal keeps none.
    at(&db, now - 4 * DAY, || {
        let p = &gfw;
        projects::update(&db, &you, &p.id, ProjectInput {
            client_id: p.client_id.clone(), name: p.name.clone(), key: p.key.clone(), status: Some(p.status.clone()), goal_md: p.goal_md.clone(),
            repo_path: Some(GFW_REPO.into()), repo_url: Some(GFW_GITHUB.into()), default_branch: Some("main".into()), color: p.color.clone(),
            budget_amount_minor: p.budget_amount_minor, budget_hours: p.budget_hours, screenshots: Some(screenshots::ProjectScreenshots::default()),
            preview_command: Some("composer preview".into()), preview_start_secs: Some(120), ..Default::default()
        }).unwrap();
    });
    at(&db, now - 3 * DAY - 6 * HOUR, || {
        tasks::update(&db, &you, &gfw1, TaskPatch { assignee_id: Some(be.clone()), ..Default::default() }).unwrap();
    });
    at(&db, now - 3 * DAY, || {
        tasks::update(&db, &you, &gfw2, TaskPatch { assignee_id: Some(fe.clone()), acceptance_md: Some(
            "- [ ] AC 1: the slider filters the product list as you drag\n- [ ] AC 2: prices show with 21% BTW, in steps of € 10\n\
             - [ ] AC 3: a range without bikes says what to try\n- [ ] AC 4: on a phone the filters fold, and the menu stays reachable".into()),
            ..Default::default() }).unwrap();
    });

    // GFW-2's six runs, each with the Screenshots section.
    let gfw_branch = "gizai/gfw-2-product-filters-price-range-slider";
    let (m0, c1, c2, c3a, c3b, c5a, c5b) = (sha("GFW main"), sha("GFW-2 1"), sha("GFW-2 2"), sha("GFW-2 3a"), sha("GFW-2 3b"), sha("GFW-2 5a"), sha("GFW-2 5b"));
    let base = |agent: &str, role: &'static str, trigger: &'static str, at: i64, minutes: i64| Spec {
        agent: if agent == "fe" { &fe } else { &qa }, role, task: &gfw2, branch: gfw_branch, worktree: "GFW/GFW-2", trigger, at, minutes,
        status: "succeeded", outcome: None, error: None, usage: (0, 0, 0), base: "", head: "", shots: vec![], left_out: vec![], preview: None,
    };
    let ask = "Should the price slider show prices with BTW or without? The product list shows them with 21% BTW, but the filter API stores \
               them without. I made the filter read both; no screen changed yet, the slider waits for your answer.";
    let (r1, r1_end) = record(&db, &dir, Spec {
        outcome: outcome("needs_decision", ask, &[]), usage: (620_000, 1_480_000, 21_400), base: &m0, head: &c1,
        ..base("fe", "frontend", "assigned", now - 2 * DAY - 3 * HOUR, 25)
    });
    at(&db, r1_end + 2_000, || {
        tasks::update(&db, &fe, &gfw2, TaskPatch { hold: Some("needs_decision".into()), hold_reason: Some(ask.into()), ..Default::default() }).unwrap();
    });
    at(&db, now - 2 * DAY - 2 * HOUR, || {
        comments::add(&db, &you, &gfw2, "With BTW, as the shop shows them everywhere. Steps of € 10 are fine.", None).unwrap();
        tasks::update(&db, &you, &gfw2, TaskPatch { hold: Some(String::new()), ..Default::default() }).unwrap();
    });
    let (r2, _) = record(&db, &dir, Spec {
        status: "timed_out", error: Some("stopped at the time limit of 90 minutes per run (Settings → Runs)"), usage: (2_840_000, 6_900_000, 88_000),
        base: &c1, head: &c2, preview: Some("It didn't answer on port 47130 in 120 s"),
        ..base("fe", "frontend", "nudge", now - 2 * DAY - HOUR - 55 * MIN, 91)
    });
    let (r3, _) = record(&db, &dir, Spec {
        outcome: outcome("ready_for_testing", "Added the price slider above the product list: two thumbs in steps of € 10, prices with BTW. The list \
            filters as you drag, and a range without bikes says to widen it. On a phone the filters fold under one Filters button.", &[]),
        usage: (1_960_000, 4_100_000, 61_000), base: &c2, head: &c3b,
        shots: vec![
            desktop("01-product-list-desktop-dark.png", "Product list with the price slider", "gfw2-r3-01"),
            desktop("02-slider-dragged-desktop-dark.png", "Slider dragged to € 400 to € 900: 12 bikes", "gfw2-r3-02"),
            phone("03-product-list-mobile-dark.png", "Product list on a phone, the filters folded", "gfw2-03-phone"),
            // the newest set hasn't this one: "Not taken this time" under it
            phone("04-filters-open-mobile-dark.png", "The filters open on a phone", "gfw2-r3-04"),
        ],
        left_out: vec!["notes.txt: not a PNG, JPEG or WebP image", "05-product-list-full-desktop-dark.png: over 16,384 pixels high"],
        ..base("fe", "frontend", "manual", now - DAY - 5 * HOUR, 40)
    });
    let (r4, _) = record(&db, &dir, Spec {
        outcome: outcome("qa_fail", "QA failed: AC 4 fails on a phone.", &["On a phone (390 × 844) the open filters cover the menu: the slider's \
            panel sits on top of the header, so the menu button can't be tapped."]),
        usage: (880_000, 2_050_000, 19_800), base: &c3b, head: &c3b,
        shots: vec![phone("01-fail-slider-covers-menu-mobile-dark.png", "Failure 1: the slider covers the menu", "gfw2-r4-01")],
        ..base("qa", "qa", "routed", now - DAY - 2 * HOUR, 35)
    });
    let (r5, _) = record(&db, &dir, Spec {
        outcome: outcome("ready_for_testing", "Fixed QA's failure 1: on a phone the filters open below the header, so the menu stays reachable. \
            The slider follows the light theme now too.", &[]),
        usage: (1_120_000, 2_600_000, 33_500), base: &c3b, head: &c5b,
        shots: vec![
            desktop("01-product-list-desktop-dark.png", "Product list with the price slider below the menu", "gfw2-r5-01"),
            desktop("02-slider-dragged-desktop-dark.png", "Slider dragged to € 400 to € 900: 12 bikes left", "gfw2-r5-02"),
            phone("03-product-list-mobile-dark.png", "Product list on a phone, the filters folded", "gfw2-03-phone"),
            desktop("04-product-list-desktop-light.png", "Product list with the price slider, light", "gfw2-r5-04"),
        ],
        ..base("fe", "frontend", "assigned", now - 3 * HOUR - 35 * MIN, 35)
    });
    let (r6, r6_end) = record(&db, &dir, Spec {
        outcome: outcome("qa_pass", "QA passed. All four criteria hold on desktop and phone: the slider filters the list as you drag (AC 1), prices \
            show with BTW in steps of € 10 (AC 2), an empty range says what to try (AC 3), and the menu stays reachable with the filters open \
            (AC 4). Opened pull request #41.", &[]),
        usage: (740_000, 1_700_000, 15_200), base: &c5b, head: &c5b,
        shots: vec![
            desktop("01-ac1-slider-filters-desktop-dark.png", "AC 1: the slider filters the list as you drag", "gfw2-r6-01"),
            desktop("02-ac3-no-bikes-desktop-dark.png", "AC 3: a range without bikes says what to try", "gfw2-r6-02"),
        ],
        ..base("qa", "qa", "routed", now - 2 * HOUR - 30 * MIN, 30)
    });
    // QA's pass leaves it in Review assigned to you, the person who reviews (`workflow::apply_outcome`): the Inbox has it.
    at(&db, r6_end + 1_000, || {
        tasks::update(&db, &qa, &gfw2, TaskPatch { assignee_id: Some(you.clone()), ..Default::default() }).unwrap();
    });
    at(&db, r6_end + MIN, || { pulls::record(&db, None, &gfw2, &format!("{GFW_GITHUB}/pull/41"), "open", false).unwrap(); });

    // KADE-2 in Testing: one set, which the board doesn't sign outside Review.
    let (k0, k1) = (sha("KADE main"), sha("KADE-2 1"));
    let (rk, _) = record(&db, &dir, Spec {
        agent: &fe, role: "frontend", task: &kade2, branch: "gizai/kade-2-rate-limit-the-public-tracking-endpoint", worktree: "KADE/KADE-2",
        trigger: "assigned", at: now - DAY - 6 * HOUR, minutes: 30, status: "succeeded",
        outcome: outcome("ready_for_testing", "The tracking page now says when to try again after too many lookups (HTTP 429): \"Too many lookups. \
            Try again in 60 seconds.\" Checked on desktop and on a phone.", &[]),
        error: None, usage: (950_000, 2_200_000, 24_000), base: &k0, head: &k1,
        shots: vec![
            desktop("01-tracking-too-many-desktop-dark.png", "The tracking page after too many lookups: try again in 60 seconds", "kade2-01"),
            phone("02-tracking-too-many-mobile-dark.png", "The same message on a phone", "kade2-02"),
        ],
        left_out: vec![], preview: None,
    });
    // GFW-3 with its own Screenshots switch off; KADE-3 in Review with a pull request and no screenshots; GFW-5 waiting.
    at(&db, now - DAY - HOUR, || {
        tasks::update(&db, &you, &gfw3, TaskPatch { screenshots: Some(Some(false)), ..Default::default() }).unwrap();
    });
    at(&db, now - DAY - 30 * MIN, || {
        tasks::move_to(&db, &you, &kade3, &col("Review"), "").unwrap();
        tasks::update(&db, &you, &kade3, TaskPatch { assignee_id: Some(you.clone()), ..Default::default() }).unwrap();
        pulls::record(&db, None, &kade3, "https://github.com/kade-logistics/portal/pull/17", "open", false).unwrap();
    });
    let gfw5 = at(&db, now - 50 * MIN, || tasks::create(&db, &you, TaskInput {
        project_id: gfw.id.clone(), title: "Wishlist: save a bike for later".into(), priority: 3, state_id: Some(col("To do")),
        description_md: "## Context\n\nCustomers want to keep a bike for later and find it again on their next visit, also without an account.\n\n\
            - [ ] a heart on each product card\n- [ ] a Wishlist page in the menu".into(),
        assignee_id: Some(fe.clone()), label_ids: vec![frontend_label.clone()], ..Default::default()
    }).unwrap());

    // ---- the answers ----
    let mut c = Map::new();
    let all_tasks = tasks::list(&db, &TaskFilter::default()).unwrap();
    let task_ids: Vec<String> = all_tasks.iter().map(|t| t.id.clone()).collect();
    let agents: Vec<team::Member> = team::all_agents(&db).unwrap().into_iter().map(|(_, m)| m).collect();
    let per_task = |f: &dyn Fn(&str) -> Value| task_ids.iter().map(|id| (id.clone(), f(id.as_str()))).collect::<Vec<_>>();
    let per_agent = |f: &dyn Fn(&team::Member) -> Value| agents.iter().map(|a| (a.actor_id.clone(), f(a))).collect::<Vec<_>>();

    c.insert("app_info".into(), json!({"version": env!("CARGO_PKG_VERSION"), "data_dir": DATA, "selftest": false, "you_id": you,
        "start_route": null, "selftest_mode": null, "data_label": "Browser mode"}));
    c.insert("onboarding_pending".into(), json!(onboarding::pending(&db).unwrap()));
    let the_team = val(team::get(&db, &s.team_id).unwrap());
    c.insert("get_team".into(), by("id", [("null".to_string(), the_team.clone()), (s.team_id.clone(), the_team)]));
    c.insert("list_teams".into(), val(team::list(&db).unwrap()));
    let all_projects = projects::list(&db).unwrap();
    c.insert("get_project".into(), by("id", all_projects.iter().map(|p| (p.id.clone(), val(p)))));
    c.insert("list_docs".into(), by("projectId", all_projects.iter().map(|p| (p.id.clone(), val(docs::list(&db, &p.id).unwrap())))));
    c.insert("check_repo".into(), by("path", [(GFW_REPO.to_string(), json!({"isGit": true, "branch": "main", "dirty": false, "github": GFW_GITHUB,
        "bitbucket": null, "suggestCopy": [".env", "vendor/", "node_modules/"]}))]));
    c.insert("list_projects".into(), val(&all_projects));
    c.insert("list_tasks".into(), val(&all_tasks));
    c.insert("get_task".into(), by("id", per_task(&|id| val(tasks::get(&db, id).unwrap()))));
    c.insert("list_users".into(), val(users::list(&db).unwrap()));
    let all_clients = clients::list(&db).unwrap();
    c.insert("get_client".into(), by("id", all_clients.iter().map(|x| (x.id.clone(), val(x)))));
    c.insert("list_clients".into(), val(&all_clients));
    c.insert("list_comments".into(), by("taskId", per_task(&|id| val(comments::list(&db, id).unwrap()))));
    c.insert("task_activity".into(), by("taskId", per_task(&|id| val(tasks::activity(&db, id).unwrap()))));
    c.insert("list_runs".into(), by("taskId", per_task(&|id| val(runs::list_for_task(&db, id).unwrap()))));
    c.insert("list_screenshots".into(), by("taskId", per_task(&|id| val(screenshots::sets(&db, id).unwrap()))));
    c.insert("suggest_agent".into(), by("taskId", per_task(&|id| json!(workflow::run_agent(&db, id).unwrap().map(|(a, _)| a)))));
    let mut owners: Vec<(String, &str)> = task_ids.iter().map(|id| (id.clone(), "task")).collect();
    owners.extend(all_projects.iter().map(|p| (p.id.clone(), "project")));
    owners.extend(all_clients.iter().map(|x| (x.id.clone(), "client")));
    c.insert("list_files".into(), by("ownerId", owners.iter().map(|(id, kind)| (id.clone(), val(files::list(&db, kind, id).unwrap())))));
    c.insert("list_labels".into(), val(labels::list(&db).unwrap()));

    // Each run's commits, oldest first (worktree::Commit).
    let commit = |id: &str, subject: &str| json!({"sha": id, "subject": subject});
    c.insert("run_commits".into(), by("runId", [
        (r1, json!([commit(&c1, "Products: the price range for the filter, with BTW")])),
        (r2, json!([commit(&c2, "PriceSlider: two thumbs in steps of € 10")])),
        (r3, json!([commit(&c3a, "Products: filter the list by the slider's range"), commit(&c3b, "PriceSlider: the dark theme and the phone layout")])),
        (r4, json!([])),
        (r5, json!([commit(&c5a, "PriceSlider: below the header on a phone (QA failure 1)"), commit(&c5b, "PriceSlider: the light theme's colours")])),
        (r6, json!([])),
        (rk, json!([commit(&k1, "Tracking: say when to try again after too many lookups")])),
    ]));
    // A card's changes (CardChanges): GFW-2's branch; every card without a branch has none (KADE-2's is left out).
    let (slider, slider_diff) = changed("resources/js/components/PriceSlider.tsx", None, PRICE_SLIDER);
    let (products, products_diff) = changed("resources/js/pages/Products.tsx", Some(PRODUCTS_OLD), PRODUCTS_NEW);
    let total = |k: &str| slider[k].as_u64().unwrap() + products[k].as_u64().unwrap();
    let gfw2_changes = json!({"source": "branch", "from": m0, "to": c5b, "baseLabel": "origin/main", "additions": total("additions"),
        "deletions": total("deletions"), "files": [slider, products], "note": format!("From its branch {gfw_branch}: its worktree was removed.")});
    c.insert("card_changes".into(), by("taskId", all_tasks.iter().filter(|t| t.id == gfw2 || t.branch.is_none()).map(|t| {
        let label = if t.project_id.as_deref() == Some(gfw.id.as_str()) { "origin/main" } else { "main" };
        let answer = if t.id == gfw2 { gfw2_changes.clone() } else {
            json!({"source": "none", "from": null, "to": null, "baseLabel": label, "additions": 0, "deletions": 0, "files": [], "note": NO_BRANCH})
        };
        (t.id.clone(), answer)
    })));
    c.insert("card_file_diff".into(), by("path", [
        ("resources/js/components/PriceSlider.tsx".to_string(), slider_diff), ("resources/js/pages/Products.tsx".to_string(), products_diff),
    ]));

    // Settings (src-tauri/src/runs.rs: Settings), from what the database has saved and src-tauri's defaults.
    let shots = screenshots::limits(&db);
    let num = |key: &str, default: u64| settings::get::<u64>(&db, key).ok().flatten().unwrap_or(default);
    c.insert("get_settings".into(), json!({
        "claudeBin": settings::get::<String>(&db, "claude_bin").ok().flatten(), "dataDir": DATA, "maxConcurrentRuns": num("max_concurrent_runs", 3),
        "runsMemoryGb": 16, "computerMemoryGb": 32, "runsMemoryEnforced": true,
        "agentsPaused": settings::get::<bool>(&db, "agents_paused").ok().flatten().unwrap_or(false),
        "maxRunUsd": settings::get::<f64>(&db, "max_run_usd").ok().flatten(), "maxRunMinutes": num("max_run_minutes", 90),
        "maxRunToolCalls": num("max_run_tool_calls", 200), "maxChatMinutes": num("max_chat_minutes", 15), "maxChatToolCalls": num("max_chat_tool_calls", 60),
        "ghBin": settings::get::<String>(&db, "gh_bin").ok().flatten(), "pushOver": "ssh",
        "notifications": {"hold": true, "waiting": true, "leadAsks": true, "leadAnswered": true}, "editorRoots": editor::roots(&db),
        "screenshotsPerRun": shots.per_run, "screenshotMaxMb": shots.max_mb, "screenshotsKeepSets": shots.keep_sets,
        "screenshotsKeepDays": shots.keep_days, "previewPortFrom": shots.port_from, "previewPortTo": shots.port_to,
    }));
    c.insert("memory_enabled".into(), json!(memory::enabled(&db)));
    c.insert("ask_lead_enabled".into(), json!(questions::enabled(&db)));
    c.insert("list_old_worktrees".into(), json!([]));
    c.insert("update_status".into(), json!({"current": env!("CARGO_PKG_VERSION"), "autoCheck": true, "checking": false, "checkedAt": now - 2 * HOUR,
        "latest": {"version": env!("CARGO_PKG_VERSION"), "tag": format!("v{}", env!("CARGO_PKG_VERSION")), "name": format!("Gizai {}", env!("CARGO_PKG_VERSION")),
                   "url": format!("https://github.com/Yotech-AI/gizai/releases/tag/v{}", env!("CARGO_PKG_VERSION")), "publishedAt": null, "notes": null},
        "problem": null, "available": null, "installed": null, "installTo": "/home/you/.local", "cannotInstall": null, "job": null,
        "repo": "https://github.com/Yotech-AI/gizai.git"}));
    // Settings → General → Backups (contract BK, gizai-core's backups types): a folder in Dropbox, daily, Keep 10, and three
    // snapshots there, newest first: this computer's daily one and Back up now's, and another computer's, made by a newer
    // Gizai, so it can't be restored here.
    let chosen = "/home/you/Dropbox";
    let me = db.device_id().to_string();
    let laptop = "0199f3a2-7c41-7d2e-9b1a-4e5f6a7b8c9d";
    let newer = {
        let v: Vec<i64> = env!("CARGO_PKG_VERSION").split('.').map(|p| p.parse().unwrap_or(0)).collect();
        format!("{}.{}.0", v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0) + 1)
    };
    let snap = |reason: backups::Reason, made_at: i64, device: &str, name: &str, db_bytes: i64, problem: Option<String>| backups::BackupEntry {
        name: backups::snapshot_name(reason, made_at, &backups::device_tag(device)), reason: reason.as_str().into(), made_at,
        device_id: device.into(), device_name: name.into(), this_computer: device == me,
        app_version: if problem.is_some() { newer.clone() } else { env!("CARGO_PKG_VERSION").into() },
        schema_version: db::SCHEMA_VERSION + i64::from(problem.is_some()),
        db_bytes, files_count: 14, files_bytes: 3_276_800, files_missing: 0, restorable: problem.is_none(), problem,
    };
    let daily_at = now - 3 * HOUR;
    c.insert("list_backups".into(), val(vec![
        snap(backups::Reason::Daily, daily_at, &me, "this machine", 4_456_448, None),
        snap(backups::Reason::Manual, now - DAY - 5 * HOUR, &me, "this machine", 4_390_912, None),
        snap(backups::Reason::Daily, now - 2 * DAY - 2 * HOUR, laptop, "work-laptop", 4_128_768,
             Some(format!("A newer Gizai made it ({newer}): update Gizai to restore it"))),
    ]));
    let every = backups::Every::Daily;
    c.insert("backup_status".into(), val(backups::BackupStatus {
        settings: backups::BackupSettings { folder: Some(chosen.into()), every, on_quit: false, keep: backups::KEEP_DEFAULT },
        folder: Some(format!("{chosen}/{}", backups::FOLDER_NAME)), folder_problem: None, running: None,
        last: Some(backups::Last { name: backups::snapshot_name(backups::Reason::Daily, daily_at, &backups::device_tag(&me)), made_at: daily_at, bytes: 1_153_434 }),
        last_error: None, next: backups::next_due(every, Some(daily_at), now),
    }));
    // Settings → GitHub and Bitbucket (src-tauri's github::Status and bitbucket::BitbucketStatus): gh logged in, no Bitbucket login.
    c.insert("github_status".into(), json!({"ghPath": "/usr/bin/gh", "ghVersion": "2.62.0", "ghProblem": null, "account": "jefsev",
        "accountProblem": null, "pushOver": "ssh", "login": null, "loginCommand": "gh auth login --hostname github.com --web"}));
    c.insert("bitbucket_status".into(), json!({"email": null, "hasToken": false, "account": null, "accountProblem": {"what": "No Bitbucket login yet",
        "fix": "Make an API token with scopes for Bitbucket (Atlassian account → Security → API tokens → Create API token with scopes) with the \
                scopes read:user:bitbucket, read:pullrequest:bitbucket, write:pullrequest:bitbucket, and save it here with your Atlassian account's email."}}));
    // Nothing runs in the browser mode.
    for name in ["live_runs", "chat_live", "terminal_list"] {
        c.insert(name.into(), json!([]));
    }

    // The agent page and form.
    c.insert("get_agent".into(), by("id", per_agent(&|a| val(team::agent(&db, &a.actor_id).unwrap()))));
    c.insert("agent_runs".into(), by("id", per_agent(&|a| val(runs::list_for_agent(&db, &a.actor_id, 20).unwrap()))));
    c.insert("agent_stats".into(), by("id", per_agent(&|a| val(runs::daily_stats(&db, &a.actor_id, 14, now).unwrap()))));
    c.insert("chat_agent".into(), val(team::chat_agent(&db).unwrap()));
    // No MCP servers in Settings (`mcp_servers::list` is empty), so an agent's MCP view lists none.
    c.insert("list_mcp_servers".into(), json!([]));
    c.insert("agent_mcp".into(), by("agentId", per_agent(&|_| json!({"disabled": null, "warning": null, "servers": []}))));
    let browser = mcp_servers::browser(&db).unwrap();
    let needs = json!({"node": "/usr/bin/node", "nodeVersion": "v24.9.0", "npx": "/usr/bin/npx", "browser": "/usr/bin/chromium",
                       "browserName": "Chromium", "missing": []});
    c.insert("browser_entry".into(), json!({"id": mcp_servers::BROWSER, "version": browser.version, "program": browser.program,
        "command": format!("npx -y chrome-devtools-mcp@{} --headless --isolated --no-usage-statistics --no-performance-crux --executablePath=/usr/bin/chromium",
                           browser.version),
        "needs": needs, "problem": null, "listed": null,
        "usedBy": agents.iter().filter(|a| a.tools.browser_on()).map(|a| a.name.clone()).collect::<Vec<_>>()}));
    let catalog: Vec<Value> = CATALOG.iter().map(|(id, label, group, description, risk, how, note)| json!({"id": id, "label": label, "group": group,
        "description": description, "risk": risk, "how": how, "note": note, "reported": false})).collect();
    // ToolsView for Claude Code: the browser's state in the agent's last run, and its saved switches (null for a new agent).
    let tools_view = |saved: Value, last_run: Value| json!({
        "kind": "claude_code", "web": {"search": null, "fetch": null, "domains": null},
        "browser": {"disabled": null, "needs": needs, "version": browser.version, "lastRun": last_run, "tools": [],
                    "summary": "Its tools aren't listed yet: List tools in Settings → MCP servers shows them.", "risk": "unknown"},
        "builtin": {"tools": catalog, "source": "Gizai's catalog: after its first run, or Ask Claude Code again, it shows the tools Claude Code itself reports.",
                    "canAsk": true, "slash": null},
        "saved": saved,
    });
    let mut cli_tools: Vec<(String, Value)> = per_agent(&|a| {
        let last = runs::list_for_agent(&db, &a.actor_id, 1).unwrap().into_iter().next().and_then(|r| r.ended_at)
            .filter(|_| a.tools.browser_on()).map(|ended| json!({"status": "connected", "at": ended}));
        tools_view(val(&a.cli_tools), json!(last))
    });
    cli_tools.push(("null".into(), tools_view(Value::Null, Value::Null)));
    c.insert("agent_cli_tools".into(), by("agentId", cli_tools));
    c.insert("list_clis".into(), json!(clis::list(&db).unwrap().into_iter().map(|x| {
        let mut v = val(&x);
        v["path"] = json!((x.id == clis::CLAUDE_CODE).then_some("~/.local/bin/claude"));
        v["problem"] = Value::Null;
        v
    }).collect::<Vec<_>>()));
    let efforts = ["low", "medium", "high", "xhigh", "max"];
    c.insert("claude_models".into(), json!([
        {"value": "default", "resolvedModel": "claude-opus-5-5", "displayName": "Default (recommended)",
         "description": "Use the default model (currently Opus 5.5) · $4/$20 per Mtok", "supportsEffort": true, "effortLevels": efforts},
        {"value": "opus", "resolvedModel": "claude-opus-5-5", "displayName": "Opus", "description": "Opus 5.5 · Best for everyday, complex tasks · $4/$20 per Mtok",
         "supportsEffort": true, "effortLevels": efforts},
        {"value": "sonnet", "resolvedModel": "claude-sonnet-5-5", "displayName": "Sonnet", "description": "Sonnet 5.5 · Efficient for routine tasks · $2/$10 per Mtok",
         "supportsEffort": true, "effortLevels": efforts},
        {"value": "haiku", "resolvedModel": "claude-haiku-4-5-20251001", "displayName": "Haiku", "description": "Haiku 4.5 · Fastest for quick answers · $1/$5 per Mtok",
         "supportsEffort": false, "effortLevels": []},
    ]));
    let roles: Vec<&str> = team::ROLES.iter().copied().chain(["agent"]).collect();
    c.insert("role_template".into(), by("role", roles.iter().map(|r| (r.to_string(), json!(seed::role_template(&team::role_key(r)))))));
    c.insert("role_tools".into(), by("role", roles.iter().map(|r| (r.to_string(), json!(seed::role_tools(&team::role_key(r)))))));

    let file = json!({
        "about": "Written by crates/gizai-core/examples/fixtures.rs from the demo data: cargo run -p gizai-core --example demo -- <dir>, then \
                  cargo run -p gizai-core --example fixtures -- <dir> src/browser/fixtures.json. The browser mode (npm run browser) answers Tauri \
                  commands from it.",
        "writtenAt": now,
        "commands": c,
    });
    let out = Path::new(&out);
    if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(out, serde_json::to_string_pretty(&file).unwrap() + "\n").unwrap();
    let ident = |id: &str| tasks::get(&db, id).unwrap().identifier;
    println!("fixtures written to {}", out.display());
    println!("Frontend Agent {fe}\nBackend Agent {be}\nQA Agent {qa}\nTeam Lead {lead}");
    for (id, what) in [(&gfw2, "Review, six runs with the Screenshots section, PR #41"), (&kade2, "Testing, one set of 2"),
                       (&kade3, "Review, PR #17, no screenshots"), (&kade1, "no preview: Kade portal has no Preview command"),
                       (&gfw1, "the Backend Agent's: no browser"), (&gfw3, "its own Screenshots switch off"), (&gfw5, "To do, the Frontend Agent's, no run yet")] {
        println!("{} {id}: {what}", ident(id));
    }
}
