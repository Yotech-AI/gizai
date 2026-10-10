//! GA-51, Windows: an agent's folders (GA-45) with Windows paths: drive letters, both slashes, `~\`, names in any case,
//! and the refused folders in your profile (folders_test.rs is the same with Linux and macOS paths).
#![cfg(windows)]
use std::path::{Path, PathBuf};

use gizai_core::folders::{self, Folder, Places};

fn f(path: &str, access: &str) -> Folder {
    Folder { path: path.into(), access: access.into() }
}

/// A profile folder of C:\Users\u, with Gizai's data in %APPDATA%\Gizai.
fn places() -> Places {
    Places { home: r"C:\Users\u".into(), data: vec![r"C:\Users\u\AppData\Roaming\Gizai".into()], checkouts: vec![] }
}

fn refused(p: &str) -> Option<String> {
    folders::refusal(&folders::normalize(p, &places().home).unwrap(), &places())
}

#[test]
fn paths_are_written_one_way_whatever_slash_drive_or_tilde_they_were_typed_with() {
    let home = places().home;
    let n = |raw: &str| folders::normalize(raw, &home);
    assert_eq!(n(r"~\Herd\shared"), Some(PathBuf::from(r"C:\Users\u\Herd\shared")));
    assert_eq!(n("~/Herd/shared"), Some(PathBuf::from(r"C:\Users\u\Herd\shared")));
    assert_eq!(n("~"), Some(home.clone()));
    assert_eq!(n("d:/data/x/"), Some(PathBuf::from(r"D:\data\x")));
    assert_eq!(n(r"\\?\C:\data\x"), Some(PathBuf::from(r"C:\data\x")), "as canonicalize gives it");
    assert_eq!(n(r"C:\data\old\..\x"), Some(PathBuf::from(r"C:\data\x")));
    assert_eq!(n(r"\\server\share\docs"), Some(PathBuf::from(r"\\server\share\docs")));
    assert_eq!(n(r"Herd\shared"), None, "not a full path");
}

#[test]
fn the_whole_disk_your_profile_gizais_data_and_the_key_folders_are_refused_in_any_case() {
    assert!(refused(r"C:\").unwrap().contains("whole disk"));
    assert!(refused(r"C:\Users\u").unwrap().contains("home folder"));
    assert!(refused(r"c:\users\U").unwrap().contains("home folder"), "names are the same in any case");
    assert!(refused(r"C:\Users").unwrap().contains("holds your home folder"));
    assert!(refused(r"~\AppData\Roaming\Gizai\backups").unwrap().contains("Gizai's data folder"));
    assert_eq!(refused(r"~\.ssh").as_deref(), Some(r"it holds keys or logins (~\.ssh)"));
    assert_eq!(refused(r"C:\USERS\U\.SSH\keys").as_deref(), Some(r"it holds keys or logins (~\.ssh)"));
    assert!(refused(r"~\AppData\Roaming\GitHub CLI").unwrap().contains(r"AppData\Roaming"), "gh's and git's logins");
    assert!(refused(r"~\AppData\Local\Microsoft\Credentials").unwrap().contains(r"AppData\Local\Microsoft"));
    assert!(refused(r"~\AppData\Local").unwrap().contains(r"keys or logins (~\AppData\Local\Microsoft)"), "a folder above a key folder");
    assert!(refused(r"~\AppData").unwrap().contains("holds Gizai's data folder"), "a folder above Gizai's data");
    assert!(refused(r"~\.claude-2").unwrap().contains(".claude-2"), "a second Claude Code account");
    assert_eq!(refused(r"~\Herd\shop"), None);
    assert_eq!(refused(r"D:\work\shared"), None);
}

#[test]
fn a_backslash_is_fine_on_windows_but_the_odd_characters_are_not() {
    let checks = folders::check(&[f(r"D:\work\shared", "read"), f(r"~\Herd\out", "change"), f(r"D:\old (2)", "read")], &places());
    assert_eq!(checks[0].path, r"D:\work\shared");
    assert_eq!((checks[0].error.as_deref(), checks[1].error.as_deref()), (None, None), "{checks:?}");
    assert_eq!(checks[1].path, r"C:\Users\u\Herd\out");
    assert!(checks[2].error.as_deref().unwrap_or_default().contains("( ) [ ] * ? or ,"), "{checks:?}");
    let typed = folders::check(&[f(r"work\shared", "read")], &places());
    assert!(typed[0].error.as_deref().unwrap_or_default().contains(r"~\Herd\shared or D:\data"), "{typed:?}");
}

#[test]
fn a_folder_that_isnt_there_is_left_out_of_a_run_with_a_note() {
    let dir = tempfile::tempdir().unwrap();
    let here = dir.path().display().to_string();
    let gone = Path::new(&here).join("gone").display().to_string();
    let (kept, notes) = folders::for_run(&[f(&here, "read"), f(&gone, "read")], &places());
    assert_eq!(kept.len(), 1, "{kept:?} {notes:?}");
    assert_eq!(notes.len(), 1, "{notes:?}");
}
