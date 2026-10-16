//! Prepares a copy of the demo data for the UI Screenshots test (GA-185): GFW-2 (Product filters: price range slider, in
//! Review) gets the six runs the browser mode's fixtures give it (examples/fixtures.rs), each with the Screenshots
//! section, and is assigned to you after QA's pass, so the Inbox has it. Oldest first:
//!  1. the Frontend Agent asks a question: no screen changed, so it took none;
//!  2. it stops at the time limit: the preview didn't start, so it took none;
//!  3. it builds: 01 and 02 on a desktop, 03 and 04 on a phone, and 2 files left out;
//!  4. QA fails it: 1 picture;
//!  5. it fixes: 01 and 02 changed, 03 the same picture, 04-…-light new, and 04-filters-open not taken again;
//!  6. QA passes it: 2 pictures.
//!
//! The newest round, 5 and QA's 6 after it, has 6 screenshots. Each picture is a real PNG of plain bands, which the app
//! draws; the same look is the same picture (the same hash: Unchanged). The agents are paused, and Keep per card is 5
//! sets, so the housekeeping at start keeps the four sets with pictures and the runs before them.
//! Prints GFW-2's id, then the six runs' ids, oldest first, on one line.
//! usage: cargo run -p gizai-core --example prep_screenshots -- <data dir>
use std::path::Path;

use gizai_core::files::{self, Evidence};
use gizai_core::{db::Db, ids, model::*, runs, screenshots, seed::ensure_seed, settings, tasks, team, tokens};

const MIN: i64 = 60_000;
const HOUR: i64 = 60 * MIN;
const DAY: i64 = 24 * HOUR;

/// The bits of a deflate stream, each byte filled from its lowest bit.
struct Bits {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, len: u32) {
        self.acc |= u64::from(v) << self.n;
        self.n += len;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// A Huffman code, which deflate writes from its highest bit.
    fn code(&mut self, code: u32, len: u32) {
        self.put((0..len).fold(0, |r, i| r | ((code >> i) & 1) << (len - 1 - i)), len);
    }

    /// A literal byte, a length (257–285) or the block's end (256), in deflate's fixed codes.
    fn sym(&mut self, s: u32) {
        match s {
            0..=143 => self.code(0x30 + s, 8),
            144..=255 => self.code(0x190 + s - 144, 9),
            256..=279 => self.code(s - 256, 7),
            _ => self.code(0xc0 + s - 280, 8),
        }
    }
}

/// `data` in one deflate block with the fixed codes: each run of one byte as copies of the byte before it (distance 1).
fn deflate(data: &[u8]) -> Vec<u8> {
    const BASE: [u32; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
    const EXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
    let mut b = Bits { out: vec![], acc: 0, n: 0 };
    b.put(1, 1); // the last block
    b.put(1, 2); // with the fixed codes
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        b.sym(u32::from(byte));
        let mut run = data[i + 1..].iter().take_while(|&&x| x == byte).count();
        i += 1 + run;
        while run >= 3 {
            let len = run.min(258) as u32;
            let k = BASE.iter().rposition(|&x| x <= len).unwrap();
            b.sym(257 + k as u32);
            b.put(len - BASE[k], EXTRA[k]);
            b.code(0, 5); // distance 1
            run -= len as usize;
        }
        for _ in 0..run {
            b.sym(u32::from(byte));
        }
    }
    b.sym(256);
    if b.n > 0 {
        b.out.push(b.acc as u8);
    }
    b.out
}

fn crc32(bytes: &[u8]) -> u32 {
    !bytes.iter().fold(!0u32, |c, &x| (0..8).fold(c ^ u32::from(x), |c, _| if c & 1 == 1 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 }))
}

fn adler32(bytes: &[u8]) -> u32 {
    let (a, b) = bytes.iter().fold((1u32, 0u32), |(a, b), &x| {
        let a = (a + u32::from(x)) % 65521;
        (a, (b + a) % 65521)
    });
    (b << 16) | a
}

/// A `w` × `h` RGB PNG of bands of colour, each from its first row on (one starts at row 0). A band's first row is
/// filtered Sub and every other row Up, so each row is zeros after its first pixel and deflates to a few bytes.
fn png(w: u32, h: u32, bands: &[(u32, [u8; 3])]) -> Vec<u8> {
    let row = w as usize * 3;
    let mut raw = Vec::with_capacity((row + 1) * h as usize);
    for y in 0..h {
        match bands.iter().find(|(from, _)| *from == y) {
            Some((_, rgb)) => {
                raw.push(1);
                raw.extend_from_slice(rgb);
                raw.resize(raw.len() + row - 3, 0);
            }
            None => {
                raw.push(2);
                raw.resize(raw.len() + row, 0);
            }
        }
    }
    let mut z = vec![0x78, 0x01];
    z.extend(deflate(&raw));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut head = [w.to_be_bytes(), h.to_be_bytes()].concat();
    head.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits a channel, RGB
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(b"IHDR", head), (b"IDAT", z), (b"IEND", vec![])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let at = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(&data);
        let crc = crc32(&out[at..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}

/// A screenshot a run kept; `look` makes its picture.
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

/// The picture: a page's bar, its content and its foot, in colours from the look (light ones for a -light file).
fn picture(s: &Shot) -> Vec<u8> {
    let hash = tokens::sha256_hex(s.look);
    let byte = |i: usize| u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap();
    let light = s.name.contains("-light");
    let tone = |i: usize| if light { 160 + byte(i) % 96 } else { 16 + byte(i) % 80 };
    let bar = 48 + u32::from(byte(9)) % 32;
    png(s.w, s.h, &[(0, [tone(0), tone(1), tone(2)]), (bar, [tone(3), tone(4), tone(5)]), (s.h - s.h / 5, [tone(6), tone(7), tone(8)])])
}

/// One ended run on GFW-2.
struct Spec<'a> {
    agent: &'a str,
    role: &'a str,
    trigger: &'a str,
    /// When it was queued, and how long it ran.
    at: i64,
    minutes: i64,
    status: &'a str,
    outcome: Option<Outcome>,
    error: Option<&'a str>,
    /// The commits it started and ended at.
    base: &'a str,
    head: &'a str,
    shots: Vec<Shot>,
    left_out: Vec<&'a str>,
    preview: Option<&'a str>,
}

fn outcome(outcome: &str, summary: &str) -> Option<Outcome> {
    Some(Outcome { outcome: outcome.into(), summary: summary.into(), issues: vec![] })
}

/// Records the run as Gizai does (its commits, its end, the Screenshots section and the pictures it kept), at its times.
fn record(db: &Db, dir: &Path, task: &str, branch: &str, r: Spec) -> String {
    let wt = dir.join("worktrees/GFW/GFW-2").display().to_string();
    let id = runs::create_with_trigger(db, r.agent, task, r.role, r.trigger, &ids::new_id(), &wt, &wt, branch, "/dev/null").unwrap();
    runs::set_base_sha(db, &id, r.base).unwrap();
    runs::finish(db, &id, r.status, r.outcome.as_ref(), 0, 0, 0, r.error).unwrap();
    runs::set_head_sha(db, &id, r.head).unwrap();
    screenshots::begin(db, &id).unwrap();
    for s in &r.shots {
        let blob = files::store_bytes(dir, s.name, &picture(s)).unwrap();
        db.write(Some(r.agent), |w| {
            files::insert_evidence(w, r.agent, task, &blob, &Evidence { run_id: &id, caption: s.caption, mime: "image/png", width: s.w, height: s.h })
        }).unwrap();
    }
    if !r.left_out.is_empty() {
        screenshots::set_left_out(db, &id, &r.left_out.iter().map(|l| l.to_string()).collect::<Vec<_>>()).unwrap();
    }
    if let Some(why) = r.preview {
        screenshots::set_preview_why(db, &id, why).unwrap();
    }
    db.write(None, |w| {
        w.conn().execute("UPDATE runs SET created_at = ?2, started_at = ?3, ended_at = ?4, updated_at = ?4 WHERE id = ?1",
                         rusqlite::params![id, r.at, r.at + 40_000, r.at + r.minutes * MIN])?;
        Ok(())
    }).unwrap();
    id
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: prep_screenshots <data dir>");
    let dir = Path::new(&dir);
    let db = Db::open(&dir.join("gizai.db")).unwrap();
    let s = ensure_seed(&db, "Jeffrey").unwrap();
    let you = s.you_id.clone();
    settings::set(&db, "agents_paused", &true).unwrap();
    settings::set(&db, screenshots::KEEP_SETS.key, &5u32).unwrap();
    let agent = |name: &str, role: &str| {
        let id = team::add_agent(&db, &you, &s.team_id, AgentInput { name: name.into(), role_key: role.into(), ..Default::default() }).unwrap();
        team::set_agent_status(&db, &you, &id, "paused").unwrap();
        id
    };
    let (fe, qa) = (agent("Frontend Agent", "frontend"), agent("QA Agent", "qa"));
    let card = tasks::id_of(&db, "GFW-2").unwrap();
    let branch = "gizai/gfw-2-product-filters-price-range-slider";

    let now = ids::now_ms();
    let sha = |label: &str| tokens::sha256_hex(label)[..40].to_string();
    let (m0, c1, c2, c3, c5) = (sha("GFW main"), sha("GFW-2 1"), sha("GFW-2 2"), sha("GFW-2 3b"), sha("GFW-2 5b"));
    let base = |who: &str, role: &'static str, trigger: &'static str, at: i64, minutes: i64| Spec {
        agent: if who == "fe" { &fe } else { &qa }, role, trigger, at, minutes, status: "succeeded", outcome: None, error: None,
        base: "", head: "", shots: vec![], left_out: vec![], preview: None,
    };
    let r1 = record(&db, dir, &card, branch, Spec {
        outcome: outcome("needs_decision", "Should the price slider show prices with BTW or without? No screen changed yet."), base: &m0, head: &c1,
        ..base("fe", "frontend", "assigned", now - 2 * DAY - 3 * HOUR, 25)
    });
    let r2 = record(&db, dir, &card, branch, Spec {
        status: "timed_out", error: Some("stopped at the time limit of 90 minutes per run (Settings → Runs)"), base: &c1, head: &c2,
        preview: Some("It didn't answer on port 47130 in 120 s"),
        ..base("fe", "frontend", "nudge", now - 2 * DAY - HOUR - 55 * MIN, 91)
    });
    let r3 = record(&db, dir, &card, branch, Spec {
        outcome: outcome("ready_for_testing", "Added the price slider above the product list."), base: &c2, head: &c3,
        shots: vec![
            desktop("01-product-list-desktop-dark.png", "Product list with the price slider", "gfw2-r3-01"),
            desktop("02-slider-dragged-desktop-dark.png", "Slider dragged to € 400 to € 900: 12 bikes", "gfw2-r3-02"),
            phone("03-product-list-mobile-dark.png", "Product list on a phone, the filters folded", "gfw2-03-phone"),
            phone("04-filters-open-mobile-dark.png", "The filters open on a phone", "gfw2-r3-04"),
        ],
        left_out: vec!["notes.txt: not a PNG, JPEG or WebP image", "05-product-list-full-desktop-dark.png: over 16,384 pixels high"],
        ..base("fe", "frontend", "manual", now - DAY - 5 * HOUR, 40)
    });
    let r4 = record(&db, dir, &card, branch, Spec {
        outcome: outcome("qa_fail", "QA failed: AC 4 fails on a phone."), base: &c3, head: &c3,
        shots: vec![phone("01-fail-slider-covers-menu-mobile-dark.png", "Failure 1: the slider covers the menu", "gfw2-r4-01")],
        ..base("qa", "qa", "routed", now - DAY - 2 * HOUR, 35)
    });
    let r5 = record(&db, dir, &card, branch, Spec {
        outcome: outcome("ready_for_testing", "Fixed QA's failure 1: on a phone the filters open below the header."), base: &c3, head: &c5,
        shots: vec![
            desktop("01-product-list-desktop-dark.png", "Product list with the price slider below the menu", "gfw2-r5-01"),
            desktop("02-slider-dragged-desktop-dark.png", "Slider dragged to € 400 to € 900: 12 bikes left", "gfw2-r5-02"),
            phone("03-product-list-mobile-dark.png", "Product list on a phone, the filters folded", "gfw2-03-phone"),
            desktop("04-product-list-desktop-light.png", "Product list with the price slider, light", "gfw2-r5-04"),
        ],
        ..base("fe", "frontend", "assigned", now - 3 * HOUR - 35 * MIN, 35)
    });
    let r6 = record(&db, dir, &card, branch, Spec {
        outcome: outcome("qa_pass", "QA passed. All four criteria hold on desktop and phone."), base: &c5, head: &c5,
        shots: vec![
            desktop("01-ac1-slider-filters-desktop-dark.png", "AC 1: the slider filters the list as you drag", "gfw2-r6-01"),
            desktop("02-ac3-no-bikes-desktop-dark.png", "AC 3: a range without bikes says what to try", "gfw2-r6-02"),
        ],
        ..base("qa", "qa", "routed", now - 2 * HOUR - 30 * MIN, 30)
    });
    // QA's pass leaves it in Review assigned to you, the person who reviews (`workflow::apply_outcome`)
    tasks::update(&db, &you, &card, TaskPatch { assignee_id: Some(you.clone()), ..Default::default() }).unwrap();
    println!("{card} {r1} {r2} {r3} {r4} {r5} {r6}");
}
