//! GA-63: Gizai's first start on a new install (`open_state` on an empty data folder) makes five agents, on Codex when
//! only Codex is installed. Which coding CLIs are installed is faked: HOME is an empty folder and PATH holds only fake
//! `claude` and `codex` programs, so a login shell finds those and nothing else. The environment belongs to the whole
//! process, so this file has a single test.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gizai_core::{clis, seed, team};

/// A folder with fake programs, each a script that does nothing.
fn bin_with(dir: &Path, programs: &[&str]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    for p in programs {
        let path = dir.join(p);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    dir.to_path_buf()
}

fn set_path(bin: &Path) {
    unsafe { std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display())) };
}

/// The team's agents as (name, adapter, permission mode), by name.
fn agents(st: &gizai_lib::AppState) -> Vec<(String, String, String)> {
    let t = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    let mut out: Vec<(String, String, String)> = t.members.into_iter().filter(|m| m.kind == "agent")
        .map(|m| (m.name, m.adapter.unwrap_or_default(), m.permission_mode.unwrap_or_default())).collect();
    out.sort();
    out
}

fn on(cli: &str, mode: &str, lead_on: &str) -> Vec<(String, String, String)> {
    let mut v: Vec<(String, String, String)> = ["Backend Agent", "DevOps Agent", "Frontend Agent", "QA Agent"].iter()
        .map(|n| (n.to_string(), cli.to_string(), mode.to_string())).collect();
    v.push(("Team Lead".into(), lead_on.into(), "acceptEdits".into()));
    v.sort();
    v
}

#[test]
fn a_new_install_gets_five_agents_on_claude_code_or_on_codex_when_only_codex_is_installed() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    unsafe {
        std::env::set_var("HOME", &home);
        std::env::remove_var("XDG_DATA_HOME");
        std::env::set_var("GIZAI_FAKE_KEYCHAIN", tmp.path().join("keychain.json"));
    }
    let open = |name: &str| gizai_lib::open_state(tmp.path().join(name), Arc::new(|_| {})).unwrap();

    // 1. Only Codex: it is added under Settings → Coding CLIs and runs everyone but the Team Lead, who chats on Claude Code.
    let codex_only = bin_with(&tmp.path().join("codex-only"), &["codex"]);
    set_path(&codex_only);
    let st = open("codex-data");
    let list = clis::list(&st.db).unwrap();
    assert_eq!(list.len(), 2, "{list:?}");
    let cx = list[1].clone();
    assert_eq!((cx.name.as_str(), cx.kind.as_str()), ("Codex", "codex"));
    assert_eq!(cx.command, codex_only.join("codex").display().to_string(), "the program the login shell found");
    assert!(!cx.id.is_empty());
    assert_eq!(agents(&st), on(&cx.id, "workspace-write", clis::CLAUDE_CODE));
    assert_eq!(team::chat_agent(&st.db).unwrap().unwrap().name, "Team Lead");
    assert_eq!(gizai_core::settings::get::<String>(&st.db, "claude_bin").unwrap(), None, "looking for Claude Code saves nothing");
    let t = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    for m in t.members.iter().filter(|m| m.kind == "agent") {
        assert_eq!(m.instructions_md.as_deref(), Some(seed::role_template(&m.role_key).as_str()), "{}", m.name);
        assert_eq!(m.allowed_tools, seed::role_tools(&m.role_key), "{}", m.name);
        assert_eq!((m.model.as_deref(), m.effort.as_deref(), m.max_runs), (None, None, 1), "{}", m.name);
    }
    // Started again, now with Claude Code installed too: nothing changes.
    drop(st);
    set_path(&bin_with(&tmp.path().join("both"), &["claude", "codex"]));
    let st = open("codex-data");
    assert_eq!(clis::list(&st.db).unwrap().len(), 2);
    assert_eq!(agents(&st), on(&cx.id, "workspace-write", clis::CLAUDE_CODE));
    drop(st);

    // 2. Claude Code and Codex: all five on Claude Code, and no CLI added.
    let st = open("both-data");
    assert_eq!(clis::list(&st.db).unwrap().len(), 1);
    assert_eq!(agents(&st), on(clis::CLAUDE_CODE, "acceptEdits", clis::CLAUDE_CODE));
    drop(st);

    // 3. Only Claude Code: the same.
    set_path(&bin_with(&tmp.path().join("claude-only"), &["claude"]));
    let st = open("claude-data");
    assert_eq!(clis::list(&st.db).unwrap().len(), 1);
    assert_eq!(agents(&st), on(clis::CLAUDE_CODE, "acceptEdits", clis::CLAUDE_CODE));
    drop(st);

    // 4. Neither: all five on Claude Code (a run then holds its card as blocked), and no CLI added.
    set_path(&bin_with(&tmp.path().join("neither"), &[]));
    let st = open("neither-data");
    assert_eq!(clis::list(&st.db).unwrap().len(), 1);
    assert_eq!(agents(&st), on(clis::CLAUDE_CODE, "acceptEdits", clis::CLAUDE_CODE));
    drop(st);

    // 5. A test's Gizai (test_state) starts without agents, as before.
    set_path(&codex_only);
    let st = gizai_lib::test_state(&tmp.path().join("test"));
    assert!(agents(&st).is_empty());
    assert_eq!(clis::list(&st.db).unwrap().len(), 1);
}
