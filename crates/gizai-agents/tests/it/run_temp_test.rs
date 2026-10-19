//! GA-210: a run's temp folder is short and outside its worktree, Gizai's data folder and any git repository
//! (`worktree::prepare_run_temp`), so a Unix socket fits in it (Chromium's SingletonSocket). It is made private, starts
//! empty, is emptied without following links, and goes with the card's worktree. `.gizai-tmp` stays for the screenshots
//! and the preview (worktree_test.rs).
use gizai_agents::worktree;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "init"]);
    repo
}

/// A temp folder by its real path, the way git and Gizai report it (see worktree_test.rs).
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    tempfile::tempdir_in(base).unwrap()
}

/// A card's worktree folder where Gizai puts it, `<data>/worktrees/<KEY>/<card>`, made.
fn card_folder(data: &Path, card: &str) -> PathBuf {
    let wt = data.join("worktrees").join(card.split('-').next().unwrap()).join(card);
    std::fs::create_dir_all(&wt).unwrap();
    wt
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

/// The folder's name split into the card part and its hash.
fn name_and_hash(dir: &Path) -> (String, String) {
    let file = dir.file_name().unwrap().to_string_lossy().into_owned();
    let (name, hash) = file.rsplit_once('-').unwrap_or_else(|| panic!("no hash in {file}"));
    (name.to_string(), hash.to_string())
}

#[test]
fn the_runs_temp_folders_go_in_var_tmp_on_linux_and_macos_and_in_your_own_temp_folder_on_windows() {
    #[cfg(unix)]
    // SAFETY: getuid has no preconditions.
    assert_eq!(worktree::temp_base(), PathBuf::from(format!("/var/tmp/gizai-{}", unsafe { libc::getuid() })));
    #[cfg(windows)]
    assert_eq!(worktree::temp_base(), std::env::temp_dir().join("gizai"));
}

#[test]
fn a_cards_folder_is_its_name_and_8_hex_of_a_hash_at_most_50_characters_outside_its_worktree_and_the_data_folder() {
    let tmp = real_tempdir();
    // as deep as Gizai's data folder on Linux, ~/.local/share/gizai
    let data = tmp.path().join("home/jefsev/.local/share/gizai");
    let base = worktree::temp_base();
    // a key of 4 and of 6 letters with a number of 7 digits, and the cards the browser crashed on
    for card in ["EUQR-1234567", "EUQRST-1234567", "DSO-30", "DSO-16", "GA-1"] {
        let wt = card_folder(&data, card);
        let dir = worktree::run_temp_in(&base, &wt);
        assert_eq!(dir.parent(), Some(base.as_path()), "{card}: {}", dir.display());
        let (name, hash) = name_and_hash(&dir);
        assert_eq!(name, card);
        assert!(hash.len() == 8 && hash.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "{card}: {}", dir.display());
        assert!(!dir.starts_with(&wt) && !dir.starts_with(&data) && !dir.starts_with(tmp.path()), "{card}: {}", dir.display());
        #[cfg(unix)]
        assert!(dir.as_os_str().len() <= 50, "{card}: {} is {} characters", dir.display(), dir.as_os_str().len());
    }
    #[cfg(unix)]
    {
        // the longest uid there is (10 digits), and a name longer than any card's: cut to 14 characters
        let dir = worktree::run_temp_in(Path::new("/var/tmp/gizai-4294967295"), &card_folder(&data, "EUQRST-1234567"));
        assert!(dir.as_os_str().len() <= 50, "{} is {} characters", dir.display(), dir.as_os_str().len());
        let dir = worktree::run_temp_in(Path::new("/var/tmp/gizai-4294967295"), &card_folder(&data, "ABCDEFGHIJ-123456789"));
        assert_eq!(name_and_hash(&dir).0, "ABCDEFGHIJ-123");
        assert!(dir.as_os_str().len() <= 50, "{} is {} characters", dir.display(), dir.as_os_str().len());
        // and the old place is too long for a socket: Chromium's 'Socket path too long' on DSO-30 (16 Oct)
        let old = "/home/jefsev/.local/share/gizai/worktrees/DSO/DSO-30/.gizai-tmp/org.chromium.Chromium.NAVwPd/SingletonSocket";
        assert!(old.len() >= 104, "{}", old.len());
    }
}

// Linux and macOS only: Unix sockets. This one is made in the real `/var/tmp/gizai-<uid>`, where Gizai makes it, and
// removed again.
#[cfg(unix)]
#[test]
fn chromiums_singleton_socket_binds_in_the_run_temp_folder_of_a_worktree_named_euqr_1234567() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = real_tempdir();
    let wt = card_folder(&tmp.path().join("home/jefsev/.local/share/gizai"), "EUQR-1234567");
    let base = worktree::temp_base();
    let dir = worktree::prepare_run_temp(&base, &wt).unwrap_or_else(|why| panic!("the run's temp folder can't be used: {why}"));
    assert_eq!(dir, worktree::run_temp_in(&base, &wt));
    let base_mode = std::fs::symlink_metadata(&base).unwrap().permissions().mode() & 0o777;
    let socket = dir.join("org.chromium.Chromium.AbCdEf").join("SingletonSocket");
    std::fs::create_dir(socket.parent().unwrap()).unwrap();
    let bound = std::os::unix::net::UnixListener::bind(&socket).map(|_| socket.exists());
    // no git repository around it: git run there finds none, so it can't reach the card's
    let git_here = Command::new("git").args(["rev-parse", "--show-toplevel"]).current_dir(&dir)
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").output().unwrap();
    // nothing left in /var/tmp, whatever the asserts say
    worktree::remove_run_temp(&base, &wt).unwrap();
    assert!(!dir.exists(), "removed");
    let bound = bound.unwrap_or_else(|e| panic!("binding {} ({} bytes): {e}", socket.display(), socket.as_os_str().len()));
    assert!(bound, "the socket is there");
    assert!(socket.as_os_str().len() < 104, "shorter than macOS's limit: {}", socket.display());
    assert_eq!(base_mode, 0o700, "{} is private", base.display());
    assert!(!git_here.status.success(), "git found a repository around {}: {}", dir.display(), String::from_utf8_lossy(&git_here.stdout));
}

#[test]
fn worktrees_with_the_same_name_in_different_data_folders_get_different_folders() {
    let tmp = real_tempdir();
    let base = tmp.path().join("run-temp");
    let a = card_folder(&tmp.path().join("one"), "KADE-1");
    let b = card_folder(&tmp.path().join("two"), "KADE-1");
    let (ta, tb) = (worktree::run_temp_in(&base, &a), worktree::run_temp_in(&base, &b));
    assert_ne!(ta, tb);
    assert_eq!((name_and_hash(&ta).0, name_and_hash(&tb).0), ("KADE-1".to_string(), "KADE-1".to_string()));
    assert_eq!(worktree::run_temp_in(&base, &a), ta, "the same worktree gets the same folder every run");
    // both ready at once, and apart: emptying one leaves the other alone
    assert_eq!(worktree::prepare_run_temp(&base, &a).unwrap(), ta);
    assert_eq!(worktree::prepare_run_temp(&base, &b).unwrap(), tb);
    std::fs::write(ta.join("a.txt"), "a").unwrap();
    std::fs::write(tb.join("b.txt"), "b").unwrap();
    worktree::empty_run_temp(&base, &a).unwrap();
    assert!(entries(&ta).is_empty());
    assert_eq!(entries(&tb), ["b.txt"]);
    // the worktree by another path to it is the same worktree: the hash is of its path on disk
    #[cfg(unix)]
    {
        let link = tmp.path().join("link-to-one");
        std::os::unix::fs::symlink(tmp.path().join("one"), &link).unwrap();
        assert_eq!(worktree::run_temp_in(&base, &link.join("worktrees/KADE/KADE-1")), ta);
    }
    // and once its folder is gone (git removed it) it still has the folder it had
    std::fs::remove_dir(&a).unwrap();
    assert_eq!(worktree::run_temp_in(&base, &a), ta);
}

// Linux and macOS only: Unix modes, owners and symlinks.
#[cfg(unix)]
#[test]
fn the_base_is_made_0700_and_a_link_someone_elses_folder_or_one_open_to_others_is_never_used() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let mode = |p: &Path| std::fs::symlink_metadata(p).unwrap().permissions().mode() & 0o777;
    let tmp = real_tempdir();
    let wt = card_folder(&tmp.path().join("data"), "KADE-7");

    // made with mode 0700, and the card's folder in it too
    let base = tmp.path().join("gizai-1000");
    let dir = worktree::prepare_run_temp(&base, &wt).unwrap();
    assert_eq!(dir.parent(), Some(base.as_path()));
    assert_eq!((mode(&base), mode(&dir)), (0o700, 0o700));
    // one of yours that is 0700 already is used as it is
    assert_eq!(worktree::prepare_run_temp(&base, &wt).unwrap(), dir);

    // one others may open: never used, nothing made in it
    for open in [0o755, 0o750, 0o705, 0o777] {
        let base = tmp.path().join(format!("open-{open:o}"));
        std::fs::create_dir(&base).unwrap();
        std::fs::set_permissions(&base, std::fs::Permissions::from_mode(open)).unwrap();
        let why = worktree::prepare_run_temp(&base, &wt).unwrap_err();
        assert!(why.contains(&base.display().to_string()) && why.contains("is open to other users"), "{open:o}: {why}");
        assert!(entries(&base).is_empty(), "{open:o}");
        assert_eq!(mode(&base), open, "left as it was");
    }

    // a link, even to a private folder of yours: never used, never followed, also not to empty or remove
    let mine = tmp.path().join("mine");
    std::fs::create_dir(&mine).unwrap();
    std::fs::set_permissions(&mine, std::fs::Permissions::from_mode(0o700)).unwrap();
    let there = worktree::run_temp_in(&mine, &wt);
    std::fs::create_dir(&there).unwrap();
    std::fs::write(there.join("keep.txt"), "mine").unwrap();
    let link = tmp.path().join("gizai-link");
    symlink(&mine, &link).unwrap();
    let why = worktree::prepare_run_temp(&link, &wt).unwrap_err();
    assert!(why.contains("is a link"), "{why}");
    assert!(worktree::empty_run_temp(&link, &wt).is_err());
    assert!(worktree::remove_run_temp(&link, &wt).is_err());
    assert_eq!(std::fs::read_to_string(there.join("keep.txt")).unwrap(), "mine", "the link's target is left alone");
    assert_eq!(entries(&mine), [there.file_name().unwrap().to_string_lossy().into_owned()]);

    // a file in its place
    let file = tmp.path().join("gizai-file");
    std::fs::write(&file, "x").unwrap();
    let why = worktree::prepare_run_temp(&file, &wt).unwrap_err();
    assert!(why.contains("is not a folder"), "{why}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "x");

    // someone else's folder: root's /, unless the tests run as root
    // SAFETY: getuid has no preconditions.
    if unsafe { libc::getuid() } != 0 {
        let why = worktree::prepare_run_temp(Path::new("/"), &wt).unwrap_err();
        assert!(why.contains("belongs to another user"), "{why}");
        assert!(worktree::empty_run_temp(Path::new("/"), &wt).is_err());
    }

    // one that can't be made: its parent isn't there
    let why = worktree::prepare_run_temp(&tmp.path().join("no/such/gizai-1000"), &wt).unwrap_err();
    assert!(why.starts_with("couldn't make"), "{why}");
}

// Linux and macOS only: Unix symlinks (a symlink on Windows needs Developer Mode or an administrator).
#[cfg(unix)]
#[test]
fn the_run_temp_folder_starts_empty_is_emptied_and_removed_without_following_links() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let tmp = real_tempdir();
    let base = tmp.path().join("run-temp");
    let wt = card_folder(&tmp.path().join("data"), "KADE-8");
    // nothing there yet: nothing to empty or remove
    worktree::empty_run_temp(&base, &wt).unwrap();
    worktree::remove_run_temp(&base, &wt).unwrap();
    let dir = worktree::prepare_run_temp(&base, &wt).unwrap();
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(outside.join("sub")).unwrap();
    std::fs::write(outside.join("sub/keep.txt"), "mine").unwrap();
    let kept = || std::fs::read_to_string(outside.join("sub/keep.txt")).unwrap() == "mine" && entries(&outside) == ["sub"];
    let fill = || {
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::create_dir_all(dir.join("b/c")).unwrap();
        std::fs::write(dir.join("b/c/d.txt"), "d").unwrap();
        symlink(&outside, dir.join("link-to-dir")).unwrap();
        symlink(outside.join("sub/keep.txt"), dir.join("link-to-file")).unwrap();
        symlink(&outside, dir.join("b/link-deeper")).unwrap();
    };

    // what a run Gizai couldn't follow to its end left: the next run starts with it empty
    fill();
    assert_eq!(worktree::prepare_run_temp(&base, &wt).unwrap(), dir);
    assert!(entries(&dir).is_empty(), "{:?}", entries(&dir));
    assert!(kept(), "links aren't followed");

    // at a run's end: emptied the same way, and the folder stays
    fill();
    worktree::empty_run_temp(&base, &wt).unwrap();
    assert!(dir.is_dir() && entries(&dir).is_empty(), "{:?}", entries(&dir));
    assert!(kept(), "links aren't followed");

    // a link in its place: removed, never followed, and a private folder made
    std::fs::remove_dir(&dir).unwrap();
    symlink(&outside, &dir).unwrap();
    assert_eq!(worktree::prepare_run_temp(&base, &wt).unwrap(), dir);
    let m = std::fs::symlink_metadata(&dir).unwrap();
    assert!(m.is_dir() && !m.file_type().is_symlink() && m.permissions().mode() & 0o777 == 0o700);
    assert!(kept(), "the link's target is left alone");
    // a file in its place too
    std::fs::remove_dir(&dir).unwrap();
    std::fs::write(&dir, "x").unwrap();
    assert_eq!(worktree::prepare_run_temp(&base, &wt).unwrap(), dir);
    assert!(dir.is_dir() && entries(&dir).is_empty());

    // removed with everything in it, links not followed
    fill();
    worktree::remove_run_temp(&base, &wt).unwrap();
    assert!(std::fs::symlink_metadata(&dir).is_err(), "gone");
    assert!(kept(), "links aren't followed");
    // a link in its place is removed, not what it points to
    symlink(&outside, &dir).unwrap();
    worktree::remove_run_temp(&base, &wt).unwrap();
    assert!(std::fs::symlink_metadata(&dir).is_err() && kept());
}

#[test]
fn the_run_temp_folder_goes_with_the_cards_worktree_and_stays_with_one_git_keeps() {
    let tmp = real_tempdir();
    let repo = repo(tmp.path());
    let wts = tmp.path().join("wt");
    let base = tmp.path().join("run-temp");
    let a = worktree::ensure(&repo, &wts, "KADE-4", "Four", "main").unwrap();
    let b = worktree::ensure(&repo, &wts, "KADE-5", "Five", "main").unwrap();
    let ta = worktree::prepare_run_temp(&base, &a.path).unwrap();
    let tb = worktree::prepare_run_temp(&base, &b.path).unwrap();
    std::fs::create_dir_all(ta.join("left/over")).unwrap();
    std::fs::write(ta.join("left/over/db.sqlite"), "x").unwrap();
    std::fs::write(tb.join("x.txt"), "x").unwrap();

    let out = worktree::remove_card_worktree_with(&repo, &wts, &base, &a.branch, false);
    assert_eq!(out.removed.as_deref(), Some(a.path.as_path()), "{:?}", out.kept);
    assert!(!a.path.exists() && !ta.exists(), "the worktree and its runs' temp folder are gone");
    assert_eq!(entries(&tb), ["x.txt"], "another card's folder stays");

    // git keeps a worktree with uncommitted work, and its temp folder stays with it
    std::fs::write(b.path.join("work.txt"), "w").unwrap();
    let out = worktree::remove_card_worktree_with(&repo, &wts, &base, &b.branch, false);
    assert!(out.removed.is_none() && out.kept.is_some(), "{:?}", out.kept);
    assert!(b.path.exists() && tb.join("x.txt").is_file());
}
