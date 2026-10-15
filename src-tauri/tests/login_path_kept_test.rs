//! GA-179: on Linux `runs::command_path` keeps your login shell's PATH for a minute (`LOGIN_PATH_TTL`), so a burst of
//! calls starts one login shell, and Settings → Coding CLIs' find and save read it again, so a CLI installed a moment
//! ago is found. The login shell is faked: HOME is a scratch folder whose .bash_profile writes a line to shells.log
//! each time a login shell starts, and adds the folders listed in more-path to PATH. HOME, PATH and the count of login
//! shells belong to the whole process, so this file has a single test.
// Linux only: macOS and Windows start no login shell here (their own PATH, `shell_path`).
#![cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};

use gizai_core::clis::Cli;
use gizai_lib::runs::{self, LOGIN_PATH_TTL};

const FAKE_CLAUDE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");

const PROFILE: &str = r#"printf 'x\n' >> "$HOME/shells.log"
if [ -f "$HOME/more-path" ]; then PATH="$PATH:$(cat "$HOME/more-path")"; fi
export PATH
"#;

fn dirs(path: &std::ffi::OsStr) -> Vec<PathBuf> {
    std::env::split_paths(path).collect()
}

/// Login shells started so far, as the fake .bash_profile counted them.
fn shells(home: &Path) -> usize {
    std::fs::read_to_string(home.join("shells.log")).map(|s| s.lines().count()).unwrap_or(0)
}

/// A fake program `name` in the folder `dir`: a script that does nothing.
fn install(dir: &Path, name: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

/// The next login shell adds `dirs` to PATH.
fn login_adds(home: &Path, dirs: &[&Path]) {
    let line: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
    std::fs::write(home.join("more-path"), line.join(":")).unwrap();
}

/// Adds CLIs in Settings → Coding CLIs; returns what save returns.
fn add_clis(st: &gizai_lib::AppState, add: &[(&str, &str, &str)]) -> Vec<gizai_lib::clis::CliStatus> {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    for (name, kind, command) in add {
        list.push(Cli { name: name.to_string(), kind: kind.to_string(), command: command.to_string(), ..Default::default() });
    }
    gizai_lib::clis::save(st, list).unwrap()
}

fn shown<'a>(list: &'a [gizai_lib::clis::CliStatus], name: &str) -> (Option<&'a str>, Option<&'a str>) {
    let c = list.iter().find(|c| c.cli.name == name).unwrap_or_else(|| panic!("no CLI {name}"));
    (c.path.as_deref(), c.problem.as_deref())
}

#[test]
fn a_burst_of_calls_starts_one_login_shell_and_finding_or_saving_clis_reads_the_path_again() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join(".bash_profile"), PROFILE).unwrap();
    let early = tmp.path().join("early");
    std::fs::create_dir_all(&early).unwrap();
    login_adds(&home, &[&early]);
    // SAFETY: the only test in this binary, and no thread of its own runs yet. A short PATH of Gizai's own, so the
    // CLIs installed on this computer aren't found before the fakes.
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::set_var("PATH", "/usr/bin:/bin");
    }
    let st = gizai_lib::test_state(&tmp.path().join("gizai"));
    // Claude Code by its saved path, so looking for it starts no login shell of its own.
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE_CLAUDE.to_string()).unwrap();
    let counts = || (runs::login_shells(), shells(&home));

    // 1. Ten calls within a minute start one login shell, and each gets the PATH it read.
    runs::age_login_path(LOGIN_PATH_TTL);
    let (n, logged) = counts();
    let first = runs::command_path();
    assert!(dirs(&first).contains(&early), "the login shell's folders are added: {first:?}");
    assert_eq!(dirs(&first)[..2], [PathBuf::from("/usr/bin"), PathBuf::from("/bin")], "Gizai's own PATH comes first");
    for i in 2..=10 {
        assert_eq!(runs::command_path(), first, "call {i}");
    }
    assert_eq!(counts(), (n + 1, logged + 1), "ten calls, one login shell");

    // 2. Ten calls at the same moment, with nothing kept: one login shell, and all get what it read.
    runs::age_login_path(LOGIN_PATH_TTL);
    let (n, logged) = counts();
    let start = Arc::new(Barrier::new(10));
    let got: Vec<_> = (0..10).map(|_| {
        let start = start.clone();
        std::thread::spawn(move || { start.wait(); runs::command_path() })
    }).collect::<Vec<_>>().into_iter().map(|t| t.join().unwrap()).collect();
    assert!(got.iter().all(|p| *p == first), "{got:?}");
    assert_eq!(counts(), (n + 1, logged + 1), "ten calls at once, one login shell");

    // 3. Kept for a minute: half a minute on it is still kept, a minute on it is read again.
    let (n, logged) = counts();
    runs::age_login_path(LOGIN_PATH_TTL / 2);
    runs::command_path();
    assert_eq!(counts(), (n, logged), "kept for half a minute");
    runs::age_login_path(LOGIN_PATH_TTL / 2);
    runs::command_path();
    assert_eq!(counts(), (n + 1, logged + 1), "read again after a minute");

    // 4. fresh_command_path reads it on every call, also within the minute, and a command_path after it gets that PATH
    // without a login shell of its own.
    let (n, logged) = counts();
    assert_eq!(runs::fresh_command_path(), first);
    assert_eq!(runs::fresh_command_path(), first);
    assert_eq!(counts(), (n + 2, logged + 2), "each fresh read starts a login shell");
    assert_eq!(runs::command_path(), first);
    assert_eq!(counts(), (n + 2, logged + 2), "the call after it uses the PATH it read");

    // 5. Codex installed a moment ago: the kept PATH doesn't have it yet, Find in Settings (find_clis) does.
    let later = tmp.path().join("later");
    let codex = install(&later, "codex");
    login_adds(&home, &[&early, &later]);
    let (n, logged) = counts();
    assert!(!dirs(&runs::command_path()).contains(&later), "the kept PATH is from before the install");
    assert_eq!(counts(), (n, logged));
    let found = gizai_lib::clis::find(&st).unwrap();
    let cx = found.iter().find(|c| c.kind == "codex").unwrap_or_else(|| panic!("Codex isn't found: {found:?}"));
    assert_eq!((cx.name.as_str(), cx.command.as_str()), ("Codex", codex.display().to_string().as_str()));
    assert_eq!(counts(), (n + 1, logged + 1), "find reads the login shell's PATH now");
    // the calls after it get that PATH, without a login shell of their own
    assert!(dirs(&runs::command_path()).contains(&later));
    assert_eq!(counts(), (n + 1, logged + 1));

    // 6. Saving Settings → Coding CLIs reads the PATH again: Gemini, saved before it was installed, is found once it is.
    let (n, logged) = counts();
    let saved = add_clis(&st, &[("Gemini", "gemini", "gemini"), ("Claude Code 2", "claude_code", "no-such-claude")]);
    assert_eq!(shown(&saved, "Gemini"), (None, Some("gemini not found")));
    assert_eq!(counts(), (n + 1, logged + 1), "save reads the login shell's PATH now");
    let newer = tmp.path().join("newer");
    let gemini = install(&newer, "gemini");
    login_adds(&home, &[&early, &later, &newer]);
    // the list (the chat, the agent form) uses the kept PATH: not found yet, and no login shell
    assert_eq!(shown(&gizai_lib::clis::list(&st).unwrap(), "Gemini"), (None, Some("gemini not found")));
    assert_eq!(counts(), (n + 1, logged + 1));
    let saved = gizai_lib::clis::save(&st, gizai_core::clis::list(&st.db).unwrap()).unwrap();
    let gemini = gemini.display().to_string();
    assert_eq!(shown(&saved, "Gemini"), (Some(gemini.as_str()), None), "found once saved again");
    assert_eq!(counts(), (n + 2, logged + 2));
    assert_eq!(shown(&gizai_lib::clis::list(&st).unwrap(), "Gemini"), (Some(gemini.as_str()), None));
    assert_eq!(counts(), (n + 2, logged + 2), "the list after it uses the PATH save read");

    // 7. The chat's CLI list, as before: Claude Code first, a CLI that isn't Claude Code with why it can't run the
    // chat, and a Claude Code whose program is missing as not found. Within the minute it starts no login shell.
    let chat = gizai_lib::chat::clis(&st).unwrap();
    let names: Vec<(&str, Option<&str>)> = chat.iter().map(|c| (c.name.as_str(), c.problem.as_deref())).collect();
    assert_eq!(names[0], ("Claude Code", None), "{names:?}");
    assert!(names.contains(&("Gemini", Some("the chat runs on Claude Code only"))), "{names:?}");
    assert!(names.contains(&("Claude Code 2", Some("no-such-claude not found"))), "{names:?}");
    assert_eq!(counts(), (n + 2, logged + 2), "opening a chat within the minute starts no login shell");
}
