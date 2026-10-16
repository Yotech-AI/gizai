// GA-182: the Screenshots section of a run's prompt, word for word as docs/research/screenshots.md §5.4 gives it, with
// the project's values filled in; QA's version; and the section when the preview didn't start. Whether a run gets it
// at all (the four switches) is tested end to end in src-tauri/tests/screenshots_flow_test.rs.
use gizai_agents::prompt::{self, PreviewDown, Screenshots};

fn section(qa: bool) -> Screenshots {
    Screenshots {
        qa,
        preview: Ok("http://127.0.0.1:47130".into()),
        log: "/w/GA-180/.gizai-tmp/preview/preview.log".into(),
        folder: "/w/GA-180/.gizai-tmp/screenshots".into(),
        sizes: vec![("Desktop".into(), 1280, 800), ("Mobile".into(), 390, 844)],
        both_themes: false,
        full_page: false,
        per_run: 12,
    }
}

#[test]
fn the_builders_section_is_the_research_documents_with_the_projects_values() {
    let want = "## Screenshots

When your change shows on a screen, take screenshots of it after your last commit and before your result line: the card \
shows them in Review, before the pull request.
- The card's branch is served at http://127.0.0.1:47130/ (Gizai started the project's preview; its output is in \
/w/GA-180/.gizai-tmp/preview/preview.log). Use only this address: never a .test site, a live site or a real account.
- Shoot with the browser's take_screenshot, always with filePath, as /w/GA-180/.gizai-tmp/screenshots/<nn>-<what>.png, \
numbered in the order to look at them: the screens you changed, in the states the card names, and their empty and error \
states when you touched them; each at Desktop 1280 x 800 and Mobile 390 x 844 (resize_page or emulate's viewport).
- Take them in one theme: the one the page opens in.
- Take one window's height, not whole pages.
- At most 12. Write a caption for each in /w/GA-180/.gizai-tmp/screenshots/captions.json, like \
{\"01-board-desktop.png\": \"The board with three cards\"}.
- Don't open your pictures unless you must: each one in your conversation costs tokens on every later turn. Check what a \
page says with take_snapshot.
- Never commit them or attach them to the pull request: Gizai keeps them on the card.
";
    assert_eq!(prompt::screenshots_section(&section(false)), want);
}

#[test]
fn the_sizes_themes_whole_page_and_limit_are_the_projects() {
    let s = Screenshots {
        sizes: vec![("Window".into(), 1440, 900), ("Small".into(), 900, 600), ("Phone".into(), 390, 844)],
        both_themes: true,
        full_page: true,
        per_run: 5,
        ..section(false)
    };
    let p = prompt::screenshots_section(&s);
    assert!(p.contains("each at Window 1440 x 900, Small 900 x 600 and Phone 390 x 844 (resize_page"), "{p}");
    assert!(p.contains("- Take each screen in the app's first theme, and the card's main screen in the other theme (light or dark) too.\n"), "{p}");
    assert!(p.contains("- Take whole pages: take_screenshot with fullPage.\n"), "{p}");
    assert!(p.contains("- At most 5. Write a caption"), "{p}");
    assert!(!p.contains("one theme") && !p.contains("one window's height"), "{p}");
    // one size
    let one = Screenshots { sizes: vec![("Desktop".into(), 1280, 800)], ..section(false) };
    assert!(prompt::screenshots_section(&one).contains("; each at Desktop 1280 x 800 (resize_page"));
}

#[test]
fn qa_adds_only_its_failures_and_at_most_four_criteria() {
    let p = prompt::screenshots_section(&section(true));
    let mut lines = p.lines();
    assert_eq!(lines.next(), Some("## Screenshots"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("Add only what the builder's screenshots don't show: each failure you found, and at most four acceptance \
criteria you checked, each captioned with the criterion."));
    assert!(!p.contains("When your change shows on a screen"), "{p}");
    assert!(!p.contains("the screens you changed"), "QA doesn't shoot the builder's screens: {p}");
    // the rest is the builder's
    assert!(p.contains("- The card's branch is served at http://127.0.0.1:47130/ "), "{p}");
    assert!(p.contains("as /w/GA-180/.gizai-tmp/screenshots/<nn>-<what>.png, numbered in the order to look at them; each at Desktop 1280 x 800"), "{p}");
    assert!(p.contains("- At most 12."), "{p}");
    assert!(p.contains("- Never commit them or attach them to the pull request: Gizai keeps them on the card."), "{p}");
}

#[test]
fn a_preview_that_didnt_start_says_why_with_its_last_lines_and_to_take_none() {
    let down = |why: &str, tail: &str| Screenshots { preview: Err(PreviewDown { why: why.into(), tail: tail.into() }), ..section(false) };
    let p = prompt::screenshots_section(&down("It didn't answer on port 47130 in 120 s", "npm ERR! missing script: preview\nexit 1"));
    assert_eq!(p, "## Screenshots\n\nThe project's preview didn't start, so take no screenshots in this run: it didn't answer on port 47130 \
in 120 s. The last lines of its output (/w/GA-180/.gizai-tmp/preview/preview.log):\n\n````\nnpm ERR! missing script: preview\nexit 1\n````\n");
    assert!(!p.contains("take_screenshot") && !p.contains("At most"), "no instructions to shoot: {p}");
    // no output: no block; a name in capitals stays as it is
    let p = prompt::screenshots_section(&down("PHP couldn't start.", ""));
    assert_eq!(p, "## Screenshots\n\nThe project's preview didn't start, so take no screenshots in this run: PHP couldn't start.\n");
}

#[test]
fn the_section_goes_after_the_prompt_and_a_run_without_it_keeps_its_prompt() {
    let base = "You are the Frontend Agent.\n\n# Task KADE-41: Export invoices\n";
    assert_eq!(prompt::with_screenshots(base, None), base);
    let s = section(false);
    let p = prompt::with_screenshots(base, Some(&s));
    assert!(p.starts_with("You are the Frontend Agent.\n\n# Task KADE-41: Export invoices\n\n## Screenshots\n\n"), "{p}");
    assert!(p.ends_with("Gizai keeps them on the card.\n"), "{p}");
    assert_eq!(p.matches("## Screenshots").count(), 1);
}
