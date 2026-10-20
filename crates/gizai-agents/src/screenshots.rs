//! A run's screenshots (GA-182): what its agent saved in the worktree's `.gizai-tmp/screenshots/`, checked before Gizai
//! keeps them on the card (`gizai_core::files::insert_evidence`). Collected at the run's end, after its CLI's process
//! group and its preview have ended and before its `.gizai-tmp` is emptied. No database here.
//!
//! The checks (docs/research/screenshots.md §3.4):
//! 1. Only that folder's own files: no subfolders, nothing else from `.gizai-tmp/`. The folder is found from the
//!    worktree Gizai made, never from a path the agent wrote.
//! 2. Real files only: a link, to anything, is left out and never followed; on Unix also a file with more than one hard
//!    link. Each file is opened (without following a link) and read once, and every check looks at what was read.
//! 3. Images by their content, not their name: the first bytes must be PNG's, JPEG's or WebP's signature, and the header
//!    must give a width and a height. SVG, GIF, HTML and anything else are left out.
//! 4. The limits: Screenshots per run (the first by name), Largest screenshot, 16 to 16,384 pixels a side, at most 50
//!    megapixels.
//! 5. Captions from `captions.json` (at most 64 KB, a JSON object of file name → text): plain text, control characters
//!    removed, at most 200 characters. A file without one gets one made from its name. A broken captions.json costs only
//!    the captions.
//!
//! Everything left out is listed with why (`Collected::left_out`), for the run's set and its output (`Collected::note`).
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// The folder in the worktree's `.gizai-tmp` where a run's agent saves its screenshots.
pub const DIR: &str = "screenshots";
/// The agent's captions, in that folder: `{"01-board-desktop.png": "The board with three cards"}`.
pub const CAPTIONS: &str = "captions.json";
/// A picture's sides: at least this many pixels…
pub const MIN_SIDE: u32 = 16;
/// …and at most this many (the usual largest texture of a GPU).
pub const MAX_SIDE: u32 = 16_384;
/// At most this many pixels in all (no "image bombs").
pub const MAX_PIXELS: u64 = 50_000_000;
/// The largest captions.json read.
pub const MAX_CAPTIONS_BYTES: u64 = 64 * 1024;
/// The longest caption kept.
pub const MAX_CAPTION_CHARS: usize = 200;

/// Where a run's agent saves its screenshots: `<worktree>/.gizai-tmp/screenshots`.
pub fn dir(worktree: &Path) -> PathBuf {
    worktree.join(crate::worktree::TEMP_DIR).join(DIR)
}

/// An image's type, from its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Webp,
}

impl Format {
    pub fn mime(self) -> &'static str {
        match self {
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Webp => "image/webp",
        }
    }
}

/// Why `image_size` refused some bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotAnImage {
    /// No PNG, JPEG or WebP signature.
    Unknown,
    /// The signature is there, but the header gives no width and height.
    NoSize,
}

/// The image type and its width and height in pixels, read from the first bytes and the header of `bytes`: PNG (its
/// IHDR), JPEG (its first frame header) or WebP (VP8, VP8L or VP8X). Nothing else is an image here: SVG, GIF, HTML…
pub fn image_size(bytes: &[u8]) -> Result<(Format, u32, u32), NotAnImage> {
    let be16 = |i: usize| u32::from(u16::from_be_bytes([bytes[i], bytes[i + 1]]));
    let le16 = |i: usize| u32::from(u16::from_le_bytes([bytes[i], bytes[i + 1]]));
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
            return Err(NotAnImage::NoSize);
        }
        let be32 = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        return Ok((Format::Png, be32(16), be32(20)));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        // the markers up to the first frame header (SOF0–SOF15, not DHT, JPG or DAC), which holds the size
        let mut i = 2;
        loop {
            if i >= bytes.len() || bytes[i] != 0xFF {
                return Err(NotAnImage::NoSize);
            }
            while i < bytes.len() && bytes[i] == 0xFF {
                i += 1;
            }
            let Some(&marker) = bytes.get(i) else { return Err(NotAnImage::NoSize) };
            i += 1;
            match marker {
                0x01 | 0xD0..=0xD8 => continue,
                // the end, or the scan, before any frame header
                0xD9 | 0xDA => return Err(NotAnImage::NoSize),
                _ => {}
            }
            if i + 2 > bytes.len() {
                return Err(NotAnImage::NoSize);
            }
            let length = be16(i) as usize;
            if length < 2 {
                return Err(NotAnImage::NoSize);
            }
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                // precision, then height and width
                if i + 7 > bytes.len() {
                    return Err(NotAnImage::NoSize);
                }
                return Ok((Format::Jpeg, be16(i + 5), be16(i + 3)));
            }
            i += length;
        }
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let chunk = bytes.get(12..16).ok_or(NotAnImage::NoSize)?;
        return match chunk {
            // lossy: a key frame's start code, then 14 bits each
            b"VP8 " if bytes.len() >= 30 && bytes[23..26] == [0x9D, 0x01, 0x2A] => Ok((Format::Webp, le16(26) & 0x3FFF, le16(28) & 0x3FFF)),
            // lossless: its signature, then width − 1 and height − 1 in 14 bits each
            b"VP8L" if bytes.len() >= 25 && bytes[20] == 0x2F => {
                let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
                Ok((Format::Webp, (bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            }
            // extended: the canvas's width − 1 and height − 1 in 24 bits each
            b"VP8X" if bytes.len() >= 30 => {
                let le24 = |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], 0]);
                Ok((Format::Webp, le24(24) + 1, le24(27) + 1))
            }
            _ => Err(NotAnImage::NoSize),
        };
    }
    Err(NotAnImage::Unknown)
}

/// Why a picture of `width` × `height` is refused (too small, too large, too many pixels), or None when it fits.
pub fn size_problem(width: u32, height: u32) -> Option<String> {
    let thousands = |n: u32| {
        let s = n.to_string();
        if s.len() > 3 { format!("{},{}", &s[..s.len() - 3], &s[s.len() - 3..]) } else { s }
    };
    for (side, word) in [(width, "wide"), (height, "high")] {
        if side < MIN_SIDE {
            return Some(format!("under {MIN_SIDE} pixels {word}"));
        }
        if side > MAX_SIDE {
            return Some(format!("over {} pixels {word}", thousands(MAX_SIDE)));
        }
    }
    (u64::from(width) * u64::from(height) > MAX_PIXELS).then(|| format!("over {} megapixels", MAX_PIXELS / 1_000_000))
}

/// A caption as Gizai keeps it: plain text on one line (control characters and line breaks become spaces, runs of
/// spaces one), at most `MAX_CAPTION_CHARS` characters.
pub fn clean_caption(text: &str) -> String {
    let plain: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let one_line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.chars().take(MAX_CAPTION_CHARS).collect::<String>().trim_end().to_string()
}

/// The words at the end of a file name that say its size or theme, besides the project's size names: each becomes a part
/// of its own in a caption made from the name.
const TAGS: [&str; 6] = ["desktop", "mobile", "tablet", "phone", "dark", "light"];

/// A caption made from a file name, for a screenshot without one in captions.json: the number in front and the
/// extension go, and the words that say its size or theme (`TAGS` and the project's `sizes`, as one word each) go after
/// commas. "01-board-with-cards-desktop-dark.png" → "Board with cards, desktop, dark".
pub fn caption_from_name(name: &str, sizes: &[String]) -> String {
    let stem = match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    };
    let rest = stem.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start_matches(['-', '_', ' ', '.']);
    let rest = if rest.is_empty() { stem } else { rest };
    let words: Vec<&str> = rest.split(['-', '_', ' ', '.']).filter(|w| !w.is_empty()).collect();
    let is_tag = |w: &str| {
        let w = w.to_lowercase();
        TAGS.contains(&w.as_str()) || sizes.iter().any(|s| !s.contains(char::is_whitespace) && s.to_lowercase() == w)
    };
    let first_tag = words.iter().rposition(|w| !is_tag(w)).map_or(0, |i| i + 1);
    let mut parts: Vec<String> = vec![];
    if first_tag > 0 {
        parts.push(words[..first_tag].join(" "));
    }
    parts.extend(words[first_tag..].iter().map(|w| w.to_lowercase()));
    let caption = parts.join(", ");
    let mut chars = caption.chars();
    let caption = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => name.to_string(),
    };
    clean_caption(&caption)
}

/// What a run's screenshots are held to (Settings → Agents and runs → Screenshots) and the project's size names (for
/// captions made from file names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    /// Screenshots per run: the first ones by file name are kept.
    pub per_run: usize,
    /// Largest screenshot, in bytes.
    pub max_bytes: u64,
    /// The project's size names ("Desktop", "Window").
    pub sizes: Vec<String>,
}

/// One screenshot that passed the checks, as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shot {
    /// Its file name: "01-board-with-cards-desktop-dark.png".
    pub name: String,
    pub bytes: Vec<u8>,
    pub format: Format,
    pub width: u32,
    pub height: u32,
    /// From captions.json, else made from its name.
    pub caption: String,
}

/// What `collect` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collected {
    /// How many screenshots were kept.
    pub kept: usize,
    /// How many screenshots the folder had: files that were meant as one (a PNG, JPEG or WebP by its content, or too
    /// large to look at). Links, folders and files that aren't an image are left out without counting.
    pub found: usize,
    /// What was left out, each "<name>: <why>": "13-board.png: over the limit of 12 a run".
    pub left_out: Vec<String>,
    /// Whether screenshots were left out for the per-run limit, which was this.
    pub over_limit: Option<usize>,
}

impl Collected {
    /// What the run's output says about it, or None when the folder was empty and nothing was left out: "Gizai kept 12
    /// of 15 screenshots (Settings → Agents and runs → Screenshots allows 12 a run). Left out: 13-board.png: over the
    /// limit of 12 a run; notes.txt: not a PNG, JPEG or WebP image."
    pub fn note(&self) -> Option<String> {
        if self.found == 0 && self.left_out.is_empty() {
            return None;
        }
        let word = |n: usize| if n == 1 { "screenshot" } else { "screenshots" };
        let mut s = if self.left_out.is_empty() {
            format!("Gizai kept {} {} on the card.", self.kept, word(self.kept))
        } else if self.found == 0 {
            "Gizai kept no screenshots.".to_string()
        } else {
            let limit = self.over_limit.map(|n| format!(" (Settings → Agents and runs → Screenshots allows {n} a run)")).unwrap_or_default();
            format!("Gizai kept {} of {} {}{limit}.", self.kept, self.found, word(self.found))
        };
        if !self.left_out.is_empty() {
            s.push_str(&format!(" Left out: {}.", self.left_out.join("; ")));
        }
        Some(s)
    }
}

/// Collects the screenshots in `worktree`'s `.gizai-tmp/screenshots/` (see the module's checks), handing each one that
/// passes to `keep`, one at a time and in name order, so only one is held in memory. `keep` stores it; an error from it
/// leaves the screenshot out, with that error as the reason. A missing folder has none.
pub fn collect(worktree: &Path, rules: &Rules, mut keep: impl FnMut(Shot) -> Result<(), String>) -> Collected {
    let mut out = Collected::default();
    let temp = worktree.join(crate::worktree::TEMP_DIR);
    let folder = dir(worktree);
    for (path, name) in [(&temp, crate::worktree::TEMP_DIR), (&folder, DIR)] {
        match std::fs::symlink_metadata(path) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => {
                out.left_out.push(format!("{name}: not a folder (a link or a file), so Gizai looked no further"));
                return out;
            }
            Err(_) => return out,
        }
    }
    let mut entries: Vec<(String, PathBuf)> = match std::fs::read_dir(&folder) {
        Ok(rd) => rd.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).collect(),
        Err(e) => {
            out.left_out.push(format!("{DIR}: couldn't be read ({e})"));
            return out;
        }
    };
    entries.sort();
    let captions = match entries.iter().find(|(n, _)| n == CAPTIONS) {
        Some((_, path)) => match read_captions(path) {
            Ok(c) => c,
            Err(why) => {
                out.left_out.push(format!("{CAPTIONS}: {why}, so the captions come from the file names"));
                HashMap::new()
            }
        },
        None => HashMap::new(),
    };
    for (name, path) in entries.iter().filter(|(n, _)| n != CAPTIONS) {
        let (format, width, height, bytes) = match read_image(path, rules.max_bytes) {
            Ok(image) => image,
            Err(Refused { why, a_screenshot }) => {
                out.found += usize::from(a_screenshot);
                out.left_out.push(format!("{name}: {why}"));
                continue;
            }
        };
        out.found += 1;
        if out.kept >= rules.per_run {
            out.left_out.push(format!("{name}: over the limit of {} a run", rules.per_run));
            out.over_limit = Some(rules.per_run);
            continue;
        }
        let caption = captions.get(name).filter(|c| !c.is_empty()).cloned().unwrap_or_else(|| caption_from_name(name, &rules.sizes));
        match keep(Shot { name: name.clone(), bytes, format, width, height, caption }) {
            Ok(()) => out.kept += 1,
            Err(why) => out.left_out.push(format!("{name}: couldn't be kept ({why})")),
        }
    }
    out
}

/// `collect`, keeping every screenshot that passes in a list (tests, and anything that wants them all at once).
pub fn collect_all(worktree: &Path, rules: &Rules) -> (Vec<Shot>, Collected) {
    let mut shots = vec![];
    let collected = collect(worktree, rules, |s| {
        shots.push(s);
        Ok(())
    });
    (shots, collected)
}

/// Opens `path` without following a link, and reads at most `max` bytes of it, once: only a real file with one link
/// (on Unix), as the opened file itself says. Err says why not, as a run's output says it.
fn read_once(path: &Path, max: u64, too_big: impl Fn() -> String) -> Result<Vec<u8>, String> {
    // a first look without opening: a link or a folder isn't opened at all
    let meta = std::fs::symlink_metadata(path).map_err(|e| format!("couldn't be read ({e})"))?;
    real_file(&meta)?;
    if meta.len() > max {
        return Err(too_big());
    }
    let file = open_no_follow(path).map_err(|e| format!("couldn't be read ({e})"))?;
    // what was opened, not what the path names now
    let meta = file.metadata().map_err(|e| format!("couldn't be read ({e})"))?;
    real_file(&meta)?;
    let mut bytes = vec![];
    file.take(max + 1).read_to_end(&mut bytes).map_err(|e| format!("couldn't be read ({e})"))?;
    if bytes.len() as u64 > max {
        return Err(too_big());
    }
    Ok(bytes)
}

/// Err says why `meta` isn't a real file with one link: a link, a folder, something else, or (Unix) a file with more
/// than one hard link, which could be a file outside the worktree.
fn real_file(meta: &std::fs::Metadata) -> Result<(), String> {
    let t = meta.file_type();
    if t.is_symlink() {
        return Err("a link, which Gizai doesn't follow".into());
    }
    if t.is_dir() {
        return Err("a folder: only the screenshots folder's own files count".into());
    }
    if !t.is_file() {
        return Err("not a file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() > 1 {
            return Err("a file with more than one hard link".into());
        }
    }
    Ok(())
}

/// Opens a file for reading without following a link: O_NOFOLLOW on Unix (and O_NONBLOCK, so a pipe in its place can't
/// hold Gizai up), the link itself on Windows (FILE_FLAG_OPEN_REPARSE_POINT), which then isn't a file.
fn open_no_follow(path: &Path) -> std::io::Result<File> {
    let mut open = std::fs::OpenOptions::new();
    open.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        open.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        open.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    open.open(path)
}

/// Why `read_image` left a file out, and whether it counts as a screenshot the agent saved (an image, or too large to
/// look at) or not (a link, a folder, a file that isn't an image).
struct Refused {
    why: String,
    a_screenshot: bool,
}

/// Reads one screenshot once and checks it: a real file of at most `max_bytes`, a PNG, JPEG or WebP by its content, of
/// a size `size_problem` accepts.
fn read_image(path: &Path, max_bytes: u64) -> Result<(Format, u32, u32, Vec<u8>), Refused> {
    let mb = max_bytes / (1024 * 1024);
    let too_big = format!("over the largest screenshot of {mb} MB");
    let bytes = read_once(path, max_bytes, || too_big.clone()).map_err(|why| Refused { a_screenshot: why == too_big, why })?;
    let (format, width, height) = image_size(&bytes).map_err(|e| match e {
        NotAnImage::Unknown => Refused { why: "not a PNG, JPEG or WebP image".into(), a_screenshot: false },
        NotAnImage::NoSize => Refused { why: "its image header gives no width and height".into(), a_screenshot: true },
    })?;
    if let Some(why) = size_problem(width, height) {
        return Err(Refused { why, a_screenshot: true });
    }
    Ok((format, width, height, bytes))
}

/// The captions in captions.json, read once as `read_once` reads a screenshot: file name → caption, cleaned
/// (`clean_caption`). A value that isn't text is no caption. Err says why the file can't be used.
fn read_captions(path: &Path) -> Result<HashMap<String, String>, String> {
    let bytes = read_once(path, MAX_CAPTIONS_BYTES, || "over 64 KB".to_string())?;
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&bytes).map_err(|_| "not a JSON object of file names and captions".to_string())?;
    Ok(map.into_iter().filter_map(|(name, v)| v.as_str().map(|c| (name, clean_caption(c)))).collect())
}
