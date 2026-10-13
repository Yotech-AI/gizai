// Linux and macOS only: the fake programs are shell scripts, which Windows can't start.
#![cfg(unix)]
// Settings → Coding CLIs shows a program in your home folder as ~/…, and Claude Code's saved path may start with ~, as
// the other CLIs' do (GA-149: the product tour runs Gizai in a fake home, with Claude Code at ~/.local/bin/claude and
// Codex at ~/.local/bin/codex, so no real user name shows). HOME is the process's: this binary has one test.
use gizai_core::clis::Cli;

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
const FAKE_CLI: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-cli.sh");

/// Adds a CLI in Settings → Coding CLIs.
fn add_cli(st: &gizai_lib::AppState, name: &str, kind: &str, command: &str) {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: kind.into(), command: command.into(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap();
}

fn shown<'a>(list: &'a [gizai_lib::clis::CliStatus], name: &str) -> (Option<&'a str>, Option<&'a str>) {
    let c = list.iter().find(|c| c.cli.name == name).unwrap_or_else(|| panic!("no CLI {name}"));
    (c.path.as_deref(), c.problem.as_deref())
}

#[test]
fn programs_in_your_home_folder_show_as_tilde_and_claude_codes_path_may_start_with_one() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let bin = home.join(".local").join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    // fs::copy keeps the fakes' executable bit
    std::fs::copy(FAKE_CLAUDE, bin.join("claude")).unwrap();
    std::fs::copy(FAKE_CLI, bin.join("codex")).unwrap();
    // a folder next to the home whose name only starts like it: not in the home folder
    let beside = dir.path().join("home-other");
    std::fs::create_dir_all(&beside).unwrap();
    std::fs::copy(FAKE_CLI, beside.join("gemini")).unwrap();
    // SAFETY: the only test in this binary, so no other thread reads the environment meanwhile.
    unsafe { std::env::set_var("HOME", &home) };
    assert_eq!(gizai_core::clis::home(), home.display().to_string());
    let st = gizai_lib::test_state(&dir.path().join("gizai"));

    // Claude Code saved as ~/.local/bin/claude is found there, and shows as it was typed
    gizai_core::settings::set(&st.db, "claude_bin", &"~/.local/bin/claude".to_string()).unwrap();
    add_cli(&st, "Codex", "codex", "~/.local/bin/codex");
    add_cli(&st, "Codex by its full path", "codex", &bin.join("codex").display().to_string());
    add_cli(&st, "Gemini beside home", "gemini", &beside.join("gemini").display().to_string());
    add_cli(&st, "Outside home", "other", FAKE_CLI);
    let list = gizai_lib::clis::list(&st).unwrap();
    assert_eq!(shown(&list, "Claude Code"), (Some("~/.local/bin/claude"), None));
    assert_eq!(shown(&list, "Codex"), (Some("~/.local/bin/codex"), None));
    // a full path in your home folder shows as ~/… too; anything else as it is
    assert_eq!(shown(&list, "Codex by its full path"), (Some("~/.local/bin/codex"), None));
    let beside_path = beside.join("gemini").display().to_string();
    assert_eq!(shown(&list, "Gemini beside home"), (Some(beside_path.as_str()), None));
    assert_eq!(shown(&list, "Outside home"), (Some(FAKE_CLI), None));
    let in_home = format!("{}/", home.display());
    for c in &list {
        assert!(!c.path.as_deref().unwrap_or_default().starts_with(&in_home), "{} shows the home folder: {:?}", c.cli.name, c.path);
    }

    // a ~ path that isn't there is reported as it was typed, as for the other CLIs
    gizai_core::settings::set(&st.db, "claude_bin", &"~/.local/bin/no-claude".to_string()).unwrap();
    let list = gizai_lib::clis::list(&st).unwrap();
    assert_eq!(shown(&list, "Claude Code"), (None, Some("~/.local/bin/no-claude not found")));
}
