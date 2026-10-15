// GA-182: a run's screenshots, as `gizai_agents::screenshots::collect` checks them at the run's end: only the folder's own
// real files, images by their content (PNG, JPEG, WebP), the limits, and the captions. The image headers come from tiny
// files made here, byte by byte.
use std::fs;
use std::path::PathBuf;

use gizai_agents::screenshots::{self as shots, Collected, Format, NotAnImage, Rules, Shot};

// ---- tiny images: just enough of each format's header for its width and height ----

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
    b.extend(13u32.to_be_bytes());
    b.extend(b"IHDR");
    b.extend(width.to_be_bytes());
    b.extend(height.to_be_bytes());
    b.extend([8, 6, 0, 0, 0]);
    b.extend([0, 0, 0, 0]); // the CRC: not checked
    b.extend(0u32.to_be_bytes());
    b.extend(b"IEND");
    b.extend([0xAE, 0x42, 0x60, 0x82]);
    b
}

/// A JPEG with an APP0 (JFIF) segment, then `extra` segments (like a DHT), then the frame header `sof`.
fn jpeg_with(width: u16, height: u16, sof: u8, extra: &[u8]) -> Vec<u8> {
    let mut b = vec![0xFF, 0xD8];
    b.extend([0xFF, 0xE0, 0x00, 0x10]);
    b.extend(b"JFIF\0");
    b.extend([1, 1, 0, 0, 1, 0, 1, 0, 0]);
    b.extend(extra);
    b.extend([0xFF, sof, 0x00, 0x11, 0x08]);
    b.extend(height.to_be_bytes());
    b.extend(width.to_be_bytes());
    b.extend([3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    b.extend([0xFF, 0xD9]);
    b
}

fn jpeg(width: u16, height: u16) -> Vec<u8> {
    jpeg_with(width, height, 0xC0, &[])
}

fn riff(chunk: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = b"RIFF".to_vec();
    b.extend((4 + 8 + body.len() as u32).to_le_bytes());
    b.extend(b"WEBP");
    b.extend(chunk);
    b.extend((body.len() as u32).to_le_bytes());
    b.extend(body);
    b
}

/// Lossy WebP: a key frame's tag, its start code, then 14 bits each.
fn webp_vp8(width: u16, height: u16) -> Vec<u8> {
    let mut body = vec![0x10, 0x02, 0x00, 0x9D, 0x01, 0x2A];
    body.extend(width.to_le_bytes());
    body.extend(height.to_le_bytes());
    body.extend([0; 8]);
    riff(b"VP8 ", &body)
}

/// Lossless WebP: its signature, then width − 1 and height − 1 in 14 bits each.
fn webp_vp8l(width: u32, height: u32) -> Vec<u8> {
    let bits = (width - 1) | ((height - 1) << 14);
    let mut body = vec![0x2F];
    body.extend(bits.to_le_bytes());
    body.extend([0; 4]);
    riff(b"VP8L", &body)
}

/// Extended WebP: flags, then the canvas's width − 1 and height − 1 in 24 bits each.
fn webp_vp8x(width: u32, height: u32) -> Vec<u8> {
    let mut body = vec![0, 0, 0, 0];
    body.extend(&(width - 1).to_le_bytes()[..3]);
    body.extend(&(height - 1).to_le_bytes()[..3]);
    riff(b"VP8X", &body)
}

const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="100" height="100"/></svg>"#;
const GIF: &[u8] = b"GIF89a\x64\x00\x64\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x00\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";
const HTML: &[u8] = b"<!doctype html><html><body>Not a picture</body></html>";

// ---- a worktree with its temp folder ----

const MB: u64 = 1024 * 1024;

fn rules(per_run: usize) -> Rules {
    Rules { per_run, max_bytes: 10 * MB, sizes: vec!["Desktop".into(), "Mobile".into()] }
}

struct Wt {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    folder: PathBuf,
}

/// A worktree with an empty `.gizai-tmp/screenshots/`.
fn worktree() -> Wt {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("wt");
    let folder = shots::dir(&root);
    fs::create_dir_all(&folder).unwrap();
    Wt { _tmp: tmp, root, folder }
}

impl Wt {
    fn put(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.folder.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }
    /// A file next to the worktree, outside it.
    fn outside(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.root.parent().unwrap().join(name);
        fs::write(&p, bytes).unwrap();
        p
    }
    fn collect(&self, rules: &Rules) -> (Vec<Shot>, Collected) {
        shots::collect_all(&self.root, rules)
    }
}

fn kept(shots: &[Shot]) -> Vec<&str> {
    shots.iter().map(|s| s.name.as_str()).collect()
}

/// The reason `name` was left out, from "<name>: <why>".
fn why<'a>(c: &'a Collected, name: &str) -> &'a str {
    c.left_out.iter().find_map(|l| l.strip_prefix(&format!("{name}: "))).unwrap_or_else(|| panic!("{name} isn't left out: {:?}", c.left_out))
}

fn mentions(c: &Collected, name: &str) -> bool {
    c.left_out.iter().any(|l| l.contains(name))
}

// ---- image headers ----

#[test]
fn the_image_headers_give_the_type_width_and_height() {
    assert_eq!(shots::image_size(&png(1280, 800)), Ok((Format::Png, 1280, 800)));
    assert_eq!(shots::image_size(&jpeg(390, 844)), Ok((Format::Jpeg, 390, 844)));
    // a progressive JPEG (SOF2), with a Huffman table (DHT) before its frame header
    let dht = [0xFF, 0xC4, 0x00, 0x05, 0x00, 0x01, 0x02];
    assert_eq!(shots::image_size(&jpeg_with(640, 480, 0xC2, &dht)), Ok((Format::Jpeg, 640, 480)));
    assert_eq!(shots::image_size(&webp_vp8(1440, 900)), Ok((Format::Webp, 1440, 900)));
    assert_eq!(shots::image_size(&webp_vp8l(900, 600)), Ok((Format::Webp, 900, 600)));
    assert_eq!(shots::image_size(&webp_vp8x(3840, 2400)), Ok((Format::Webp, 3840, 2400)));
    assert_eq!((Format::Png.mime(), Format::Jpeg.mime(), Format::Webp.mime()), ("image/png", "image/jpeg", "image/webp"));
}

#[test]
fn svg_gif_html_and_text_are_not_images_and_a_signature_without_a_size_is_refused() {
    for bytes in [SVG, GIF, HTML, b"just some text, named .png".as_slice(), b"".as_slice()] {
        assert_eq!(shots::image_size(bytes), Err(NotAnImage::Unknown), "{}", String::from_utf8_lossy(bytes));
    }
    // PNG's signature and nothing after it; a JPEG that ends before its frame header; a WebP with an unknown chunk
    assert_eq!(shots::image_size(b"\x89PNG\r\n\x1a\n"), Err(NotAnImage::NoSize));
    assert_eq!(shots::image_size(&[0xFF, 0xD8, 0xFF, 0xD9]), Err(NotAnImage::NoSize));
    assert_eq!(shots::image_size(&jpeg(10, 10)[..20]), Err(NotAnImage::NoSize));
    assert_eq!(shots::image_size(&riff(b"ALPH", &[0; 16])), Err(NotAnImage::NoSize));
}

#[test]
fn sizes_from_16_to_16384_pixels_a_side_and_at_most_50_megapixels_fit() {
    for (w, h) in [(16, 16), (16_384, 16), (16, 16_384), (7_000, 7_000), (1280, 800)] {
        assert_eq!(shots::size_problem(w, h), None, "{w} x {h}");
    }
    assert_eq!(shots::size_problem(15, 100).as_deref(), Some("under 16 pixels wide"));
    assert_eq!(shots::size_problem(100, 15).as_deref(), Some("under 16 pixels high"));
    assert_eq!(shots::size_problem(16_385, 100).as_deref(), Some("over 16,384 pixels wide"));
    assert_eq!(shots::size_problem(100, 16_385).as_deref(), Some("over 16,384 pixels high"));
    assert_eq!(shots::size_problem(10_000, 6_000).as_deref(), Some("over 50 megapixels"));
}

// ---- what collect keeps and leaves out ----

#[test]
fn png_jpeg_and_webp_are_kept_by_their_content_with_their_size_and_type() {
    let wt = worktree();
    wt.put("01-board-desktop.png", &png(1280, 800));
    // the name says nothing: a JPEG named .png is a JPEG
    wt.put("02-card-mobile.png", &jpeg(390, 844));
    wt.put("03-settings.webp", &webp_vp8l(900, 600));
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(kept(&shots), ["01-board-desktop.png", "02-card-mobile.png", "03-settings.webp"]);
    assert_eq!(shots.iter().map(|s| (s.format, s.width, s.height)).collect::<Vec<_>>(),
               [(Format::Png, 1280, 800), (Format::Jpeg, 390, 844), (Format::Webp, 900, 600)]);
    assert_eq!(shots[0].bytes, png(1280, 800), "the bytes read are the file's");
    assert_eq!((c.kept, c.found, c.left_out.len(), c.over_limit), (3, 3, 0, None));
    assert_eq!(c.note().as_deref(), Some("Gizai kept 3 screenshots on the card."));
}

#[test]
fn svg_gif_html_and_a_text_file_named_png_are_left_out_with_why() {
    let wt = worktree();
    wt.put("01-board.png", &png(100, 100));
    wt.put("02-logo.svg", SVG);
    wt.put("03-spinner.gif", GIF);
    wt.put("04-page.png", HTML);
    wt.put("05-fake.png", b"this is text, not a picture");
    wt.put("notes.txt", b"what I did");
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(kept(&shots), ["01-board.png"]);
    for name in ["02-logo.svg", "03-spinner.gif", "04-page.png", "05-fake.png", "notes.txt"] {
        assert_eq!(why(&c, name), "not a PNG, JPEG or WebP image");
    }
    // what isn't an image isn't counted as a screenshot
    assert_eq!((c.kept, c.found), (1, 1));
    let note = c.note().unwrap();
    assert!(note.starts_with("Gizai kept 1 of 1 screenshot. Left out: "), "{note}");
    assert!(note.contains("notes.txt: not a PNG, JPEG or WebP image"), "{note}");
}

#[test]
fn a_png_signature_without_a_header_is_left_out() {
    let wt = worktree();
    wt.put("01-cut.png", b"\x89PNG\r\n\x1a\n\0\0");
    let (shots, c) = wt.collect(&rules(12));
    assert!(shots.is_empty());
    assert_eq!(why(&c, "01-cut.png"), "its image header gives no width and height");
}

#[test]
fn only_the_folders_own_files_count_not_subfolders_or_the_rest_of_gizai_tmp() {
    let wt = worktree();
    wt.put("01-board.png", &png(100, 100));
    let sub = wt.folder.join("more");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("02-inside.png"), png(100, 100)).unwrap();
    // elsewhere in .gizai-tmp: the preview's folder, the temp folder itself
    let temp = wt.root.join(gizai_agents::worktree::TEMP_DIR);
    fs::write(temp.join("03-loose.png"), png(100, 100)).unwrap();
    fs::create_dir_all(temp.join("preview")).unwrap();
    fs::write(temp.join("preview/04-preview.png"), png(100, 100)).unwrap();
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(kept(&shots), ["01-board.png"]);
    assert_eq!(why(&c, "more"), "a folder: only the screenshots folder's own files count");
    for name in ["02-inside.png", "03-loose.png", "04-preview.png"] {
        assert!(!mentions(&c, name), "{name} was looked at: {:?}", c.left_out);
    }
}

#[cfg(unix)]
#[test]
fn links_are_never_followed_and_hard_linked_files_are_left_out() {
    let wt = worktree();
    wt.put("01-board.png", &png(100, 100));
    // a link to a real picture outside the worktree, one to a folder, one to nothing
    let outside = wt.outside("secret.png", &png(200, 200));
    std::os::unix::fs::symlink(&outside, wt.folder.join("02-link.png")).unwrap();
    std::os::unix::fs::symlink(wt.root.parent().unwrap(), wt.folder.join("03-dir-link.png")).unwrap();
    std::os::unix::fs::symlink(wt.root.join("gone.png"), wt.folder.join("04-dangling.png")).unwrap();
    // a hard link to a file outside the worktree
    let other = wt.outside("other.png", &png(300, 300));
    fs::hard_link(&other, wt.folder.join("05-hard.png")).unwrap();
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(kept(&shots), ["01-board.png"]);
    for name in ["02-link.png", "03-dir-link.png", "04-dangling.png"] {
        assert_eq!(why(&c, name), "a link, which Gizai doesn't follow");
    }
    assert_eq!(why(&c, "05-hard.png"), "a file with more than one hard link");
    assert!(shots.iter().all(|s| s.width == 100), "nothing outside was read");
}

#[cfg(unix)]
#[test]
fn a_pipe_in_the_folder_is_left_out_without_holding_gizai_up() {
    let wt = worktree();
    let fifo = wt.folder.join("01-pipe.png");
    let c_path = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: a valid C string that lives through the call.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    wt.put("02-board.png", &png(100, 100));
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(kept(&shots), ["02-board.png"]);
    assert_eq!(why(&c, "01-pipe.png"), "not a file");
}

#[cfg(unix)]
#[test]
fn a_screenshots_folder_or_temp_folder_that_is_a_link_is_not_looked_into() {
    let wt = worktree();
    let elsewhere = wt.root.parent().unwrap().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("01-board.png"), png(100, 100)).unwrap();
    fs::remove_dir(&wt.folder).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &wt.folder).unwrap();
    let (shots, c) = wt.collect(&rules(12));
    assert!(shots.is_empty());
    assert_eq!(c.kept, 0);
    assert!(why(&c, "screenshots").starts_with("not a folder"), "{:?}", c.left_out);

    // the temp folder itself a link
    let wt2 = worktree();
    let temp = wt2.root.join(gizai_agents::worktree::TEMP_DIR);
    fs::remove_dir_all(&temp).unwrap();
    let real = wt2.root.parent().unwrap().join("real-temp");
    fs::create_dir_all(real.join("screenshots")).unwrap();
    fs::write(real.join("screenshots/01-board.png"), png(100, 100)).unwrap();
    std::os::unix::fs::symlink(&real, &temp).unwrap();
    let (shots, c) = wt2.collect(&rules(12));
    assert!(shots.is_empty(), "{:?}", kept(&shots));
    assert!(why(&c, ".gizai-tmp").starts_with("not a folder"), "{:?}", c.left_out);
}

#[test]
fn a_missing_folder_has_no_screenshots_and_no_note() {
    let tmp = tempfile::tempdir().unwrap();
    let (shots, c) = shots::collect_all(&tmp.path().join("wt"), &rules(12));
    assert!(shots.is_empty());
    assert_eq!(c, Collected::default());
    assert_eq!(c.note(), None);
    // an empty folder: no note either
    let wt = worktree();
    assert_eq!(wt.collect(&rules(12)).1.note(), None);
}

#[test]
fn past_the_per_run_limit_the_first_by_name_are_kept() {
    let wt = worktree();
    // written in the reverse order: the name decides
    for n in (1..=5).rev() {
        wt.put(&format!("0{n}-screen.png"), &png(100, 100));
    }
    wt.put("notes.txt", b"not an image");
    let (shots, c) = wt.collect(&rules(3));
    assert_eq!(kept(&shots), ["01-screen.png", "02-screen.png", "03-screen.png"]);
    assert_eq!(why(&c, "04-screen.png"), "over the limit of 3 a run");
    assert_eq!(why(&c, "05-screen.png"), "over the limit of 3 a run");
    assert_eq!((c.kept, c.found, c.over_limit), (3, 5, Some(3)));
    assert_eq!(c.note().unwrap(), "Gizai kept 3 of 5 screenshots (Settings → Agents and runs → Screenshots allows 3 a run). Left out: \
04-screen.png: over the limit of 3 a run; 05-screen.png: over the limit of 3 a run; notes.txt: not a PNG, JPEG or WebP image.");
}

#[test]
fn a_file_over_the_largest_screenshot_is_left_out_and_one_at_it_is_kept() {
    let wt = worktree();
    let at_limit = { let mut b = png(100, 100); b.resize(MB as usize, 0); b };
    let over = { let mut b = png(100, 100); b.resize(MB as usize + 1, 0); b };
    wt.put("01-at-limit.png", &at_limit);
    wt.put("02-over.png", &over);
    let r = Rules { max_bytes: MB, ..rules(12) };
    let (shots, c) = wt.collect(&r);
    assert_eq!(kept(&shots), ["01-at-limit.png"]);
    assert_eq!(why(&c, "02-over.png"), "over the largest screenshot of 1 MB");
    // it was meant as a screenshot: counted
    assert_eq!((c.kept, c.found), (1, 2));
}

#[test]
fn pictures_under_16_or_over_16384_pixels_a_side_or_over_50_megapixels_are_left_out() {
    let wt = worktree();
    wt.put("01-ok.png", &png(16, 16));
    wt.put("02-narrow.png", &png(15, 800));
    wt.put("03-flat.jpg", &jpeg(800, 15));
    wt.put("04-wide.png", &png(16_385, 100));
    wt.put("05-tall.webp", &webp_vp8x(100, 16_385));
    wt.put("06-bomb.png", &png(10_000, 6_000));
    wt.put("07-edge.png", &png(16_384, 3_000));
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(kept(&shots), ["01-ok.png", "07-edge.png"]);
    assert_eq!(why(&c, "02-narrow.png"), "under 16 pixels wide");
    assert_eq!(why(&c, "03-flat.jpg"), "under 16 pixels high");
    assert_eq!(why(&c, "04-wide.png"), "over 16,384 pixels wide");
    assert_eq!(why(&c, "05-tall.webp"), "over 16,384 pixels high");
    assert_eq!(why(&c, "06-bomb.png"), "over 50 megapixels");
    assert_eq!((c.kept, c.found), (2, 7));
}

#[test]
fn a_keep_that_fails_leaves_that_screenshot_out_with_why() {
    let wt = worktree();
    wt.put("01-a.png", &png(100, 100));
    wt.put("02-b.png", &png(100, 100));
    let mut seen = vec![];
    let c = shots::collect(&wt.root, &rules(12), |s| {
        seen.push(s.name.clone());
        if s.name == "01-a.png" { Err("disk full".into()) } else { Ok(()) }
    });
    assert_eq!(seen, ["01-a.png", "02-b.png"], "one at a time, in name order");
    assert_eq!(c.kept, 1);
    assert_eq!(why(&c, "01-a.png"), "couldn't be kept (disk full)");
}

// ---- captions ----

#[test]
fn captions_come_from_captions_json_cleaned_and_the_rest_from_the_names() {
    let wt = worktree();
    for name in ["01-board-with-cards-desktop-dark.png", "02-card-mobile.png", "03-settings.png", "04-long.png", "05-number.png"] {
        wt.put(name, &png(100, 100));
    }
    let long = "x".repeat(300);
    let captions = serde_json::json!({
        "01-board-with-cards-desktop-dark.png": "The board\nwith three\tcards\u{7}",
        "03-settings.png": "  ",
        "04-long.png": long,
        "05-number.png": 42,
        "99-not-there.png": "No such file",
    });
    wt.put("captions.json", captions.to_string().as_bytes());
    let (shots, c) = wt.collect(&rules(12));
    assert!(c.left_out.is_empty(), "captions.json isn't a screenshot and isn't left out: {:?}", c.left_out);
    let cap: Vec<&str> = shots.iter().map(|s| s.caption.as_str()).collect();
    assert_eq!(cap[0], "The board with three cards", "control characters and line breaks go");
    assert_eq!(cap[1], "Card, mobile", "no caption: made from the name");
    assert_eq!(cap[2], "Settings", "an empty caption: made from the name");
    assert_eq!(cap[3].chars().count(), 200, "at most 200 characters");
    assert_eq!(cap[4], "Number", "a caption that isn't text: made from the name");
}

#[test]
fn a_broken_captions_json_costs_only_the_captions() {
    for (content, why_text) in [
        (b"{\"01-board.png\": \"The board\"".to_vec(), "not a JSON object of file names and captions"),
        (b"[\"The board\"]".to_vec(), "not a JSON object of file names and captions"),
        (format!("{{\"01-board.png\": \"{}\"}}", "x".repeat(70 * 1024)).into_bytes(), "over 64 KB"),
    ] {
        let wt = worktree();
        wt.put("01-board.png", &png(100, 100));
        wt.put("02-card-desktop.png", &png(100, 100));
        wt.put("captions.json", &content);
        let (shots, c) = wt.collect(&rules(12));
        assert_eq!(kept(&shots), ["01-board.png", "02-card-desktop.png"]);
        assert_eq!(shots[0].caption, "Board");
        assert_eq!(shots[1].caption, "Card, desktop");
        assert_eq!(why(&c, "captions.json"), format!("{why_text}, so the captions come from the file names"));
        assert_eq!(c.kept, 2);
    }
}

#[cfg(unix)]
#[test]
fn a_captions_json_that_is_a_link_is_not_followed() {
    let wt = worktree();
    wt.put("01-board.png", &png(100, 100));
    let outside = wt.outside("captions.json", br#"{"01-board.png": "From outside"}"#);
    std::os::unix::fs::symlink(&outside, wt.folder.join("captions.json")).unwrap();
    let (shots, c) = wt.collect(&rules(12));
    assert_eq!(shots[0].caption, "Board");
    assert!(why(&c, "captions.json").starts_with("a link, which Gizai doesn't follow"), "{:?}", c.left_out);
}

#[test]
fn a_caption_made_from_a_name_loses_its_number_and_puts_the_size_and_theme_after_commas() {
    let sizes = ["Desktop".to_string(), "Mobile".to_string(), "Window".to_string()];
    assert_eq!(shots::caption_from_name("01-board-with-cards-desktop-dark.png", &sizes), "Board with cards, desktop, dark");
    assert_eq!(shots::caption_from_name("02_card_mobile_light.webp", &sizes), "Card, mobile, light");
    assert_eq!(shots::caption_from_name("03-editor-window.png", &sizes), "Editor, window");
    assert_eq!(shots::caption_from_name("settings.png", &sizes), "Settings");
    assert_eq!(shots::caption_from_name("12.png", &sizes), "12");
}

#[test]
fn clean_caption_keeps_plain_text_on_one_line() {
    assert_eq!(shots::clean_caption("  A\r\nB\u{0}\tC  "), "A B C");
    assert_eq!(shots::clean_caption(&"é".repeat(250)).chars().count(), 200);
}
