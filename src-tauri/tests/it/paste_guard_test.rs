//! GA-222 QA (Linux): the paste guard's pure parts. `targets_for_caller` is what WebKitGTK's callback gets for GTK3's list
//! of formats and its count; `paste_command` is which WebKit editing command the self-test's `selftest_paste` may run, and
//! only in a self-test run. The wrapper itself, and that WebKit's pastes reach it, are checked in the cage
//! (scripts/ui-test.sh, step 21) and with `nm -D --defined-only` on the release binary.
#![cfg(target_os = "linux")]
use gizai_lib::paste_guard::{paste_command, targets_for_caller, GdkAtom};

const NONE: *mut GdkAtom = std::ptr::null_mut();

#[test]
fn an_empty_clipboards_count_of_minus_one_with_no_list_becomes_0() {
    assert_eq!(targets_for_caller(NONE, -1), (NONE, 0));
}

#[test]
fn any_count_below_0_becomes_0_without_a_list() {
    let mut atoms: [GdkAtom; 1] = [0x10 as GdkAtom];
    for n in [-2, -1000, i32::MIN] {
        assert_eq!(targets_for_caller(NONE, n), (NONE, 0), "{n}");
        // a list beside a count below 0 is never handed on: WebKit 2.54.1 would read it
        assert_eq!(targets_for_caller(atoms.as_mut_ptr(), n), (NONE, 0), "{n} with a list");
    }
}

#[test]
fn a_count_of_0_stays_0() {
    assert_eq!(targets_for_caller(NONE, 0), (NONE, 0));
}

#[test]
fn a_list_of_3_formats_passes_through_unchanged() {
    let mut atoms: [GdkAtom; 3] = [0x10 as GdkAtom, 0x20 as GdkAtom, 0x30 as GdkAtom];
    let (list, n) = targets_for_caller(atoms.as_mut_ptr(), 3);
    assert_eq!((list, n), (atoms.as_mut_ptr(), 3));
    // SAFETY: the same 3 atoms, still in scope
    let seen = unsafe { std::slice::from_raw_parts(list, n as usize) };
    assert_eq!(seen, [0x10 as GdkAtom, 0x20 as GdkAtom, 0x30 as GdkAtom]);
}

#[test]
fn selftest_paste_refuses_everything_outside_a_self_test_run() {
    for command in [None, Some("Paste"), Some("PasteGlobalSelection"), Some("Copy")] {
        assert_eq!(paste_command(false, command), Err("only in a self-test run".to_string()), "{command:?}");
    }
}

#[test]
fn in_a_self_test_run_it_runs_only_the_two_paste_commands() {
    assert_eq!(paste_command(true, None), Ok(None));
    assert_eq!(paste_command(true, Some("Paste")), Ok(Some("Paste")));
    assert_eq!(paste_command(true, Some("PasteGlobalSelection")), Ok(Some("PasteGlobalSelection")));
    for other in ["Copy", "Cut", "SelectAll", "InsertText", "PasteAsPlainText", "paste", ""] {
        assert_eq!(paste_command(true, Some(other)), Err(format!("not a paste command: {other}")));
    }
}

/// A test can't set GIZAI_SELFTEST for the command (the tests here share one process), so this reads its source: the
/// first thing `selftest_paste` does is ask `paste_command` whether this is a self-test run, before it touches the
/// clipboard or the web view.
#[test]
fn selftest_paste_asks_paste_command_first_whether_this_is_a_self_test_run() {
    let src = include_str!("../../src/paste_guard.rs");
    let at = src.find("pub async fn selftest_paste(").expect("selftest_paste");
    let first = src[at..].lines().nth(1).expect("its first line").trim();
    assert_eq!(first, "let command = paste_command(selftest_run(), command.as_deref())?;");
    let run = src.find("fn selftest_run()").expect("selftest_run");
    assert!(src[run..].lines().take(4).any(|l| l.contains(r#"std::env::var("GIZAI_SELFTEST").is_ok()"#)), "selftest_run reads GIZAI_SELFTEST");
}
