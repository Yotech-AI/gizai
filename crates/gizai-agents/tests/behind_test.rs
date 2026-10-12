//! GA-110: behind main for a folder open in the Editor (contract C3): how it stands against main (behind, ahead,
//! uncommitted), Update's plan or why it can't run, the update itself (a fast-forward, and an install only for a lock
//! file that changed) and how it ended, with what git and the installs printed; and main for a folder that isn't a
//! project's, from its origin remote. "GitHub" is a local bare repository; composer and npm are fakes, never the real ones.
// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use gizai_agents::behind::{self, End, Fetched, Main, Named};
use gizai_agents::checkout;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, files: &[(&str, &str)], msg: &str) -> String {
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
        git(dir, &["add", name]);
    }
    git(dir, &["commit", "-q", "--allow-empty", "-m", msg]);
    git_out(dir, &["rev-parse", "HEAD"])
}

fn main_ref() -> Main {
    Main { rev: "refs/remotes/origin/main".into(), label: "origin/main".into(), branch: "main".into() }
}

fn named(dir: &Path) -> Named<'_> {
    Named { dir, name: "kade-portal", card: false }
}

/// "GitHub" and a folder in ~/Herd: a clone of it on main, with vendor/ and node_modules/ installed.
struct Repos {
    src: PathBuf,
    bare: PathBuf,
    folder: PathBuf,
}

impl Repos {
    fn new(tmp: &Path) -> Repos {
        let src = tmp.join("src");
        std::fs::create_dir(&src).unwrap();
        git(&src, &["init", "-q", "-b", "main"]);
        commit(&src, &[(".gitignore", "vendor/\nnode_modules/\n.env\n"), ("composer.json", "{}\n"), ("composer.lock", "{\"v\": 1}\n"),
                       ("package.json", "{}\n"), ("package-lock.json", "{\"lockfileVersion\": 3}\n")], "init");
        let bare = tmp.join("github.git");
        git(tmp, &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);
        let folder = tmp.join("kade-portal");
        git(tmp, &["clone", "-q", bare.to_str().unwrap(), folder.to_str().unwrap()]);
        std::fs::create_dir_all(folder.join("vendor/composer")).unwrap();
        std::fs::create_dir_all(folder.join("node_modules/.bin")).unwrap();
        Repos { src, bare, folder }
    }

    /// A commit on GitHub's main; nobody fetches it.
    fn push(&self, files: &[(&str, &str)], msg: &str) -> String {
        let sha = commit(&self.src, files, msg);
        git(&self.src, &["push", "-q", self.bare.to_str().unwrap(), "main"]);
        sha
    }

    fn fetch(&self) {
        git(&self.folder, &["fetch", "-q", "origin"]);
    }

    fn head(&self) -> String {
        git_out(&self.folder, &["rev-parse", "HEAD"])
    }

    fn porcelain(&self) -> String {
        git_out(&self.folder, &["status", "--porcelain"])
    }
}

/// Writes an executable script through a child sh, so a fork elsewhere in the test process never holds it open.
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success());
}

/// Fake composer and npm: each call goes to dir/calls, and each prints a line as the real ones do.
fn fakes(dir: &Path, npm_fails: bool) -> OsString {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let d = dir.display();
    write_script(&bin.join("composer"), &format!("#!/bin/sh\necho \"composer $*\" >> '{d}/calls'\nmkdir -p vendor/composer\n\
                                                  echo 'Installing dependencies from lock file'\n"));
    let npm_end = if npm_fails { "echo 'npm ERR! code EUSAGE' >&2\nexit 1\n" } else { "mkdir -p node_modules/.bin\necho 'added 12 packages in 1s'\n" };
    write_script(&bin.join("npm"), &format!("#!/bin/sh\necho \"npm $*\" >> '{d}/calls'\n{npm_end}"));
    let mut path = vec![bin];
    path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    std::env::join_paths(path).unwrap()
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls")).unwrap_or_default().lines().map(str::to_string).collect()
}

#[test]
fn behind_ahead_and_uncommitted_are_counted_against_main_as_last_fetched() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    let main = main_ref();
    let stand = || behind::stand(named(&r.folder), Ok(&main), &Fetched::default(), None, None);

    // up to date, and never fetched since the clone
    let s = stand();
    assert_eq!((s.main_label.as_str(), s.branch.as_deref(), s.on_default, s.behind, s.ahead, s.uncommitted, s.can_update, s.updating),
               ("origin/main", Some("main"), true, 0, 0, 0, false, false), "{s:?}");
    assert_eq!((s.fetched_at, s.fetch_error.as_deref()), (None, None));
    assert_eq!(s.update_says, "kade-portal is up to date with origin/main.");
    let json = serde_json::to_value(&s).unwrap();
    let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["ahead", "behind", "branch", "canUpdate", "fetchError", "fetchedAt", "mainLabel", "onDefault", "uncommitted",
                      "updateSays", "updating"], "contract C3's FolderMain");
    assert_eq!((json["fetchedAt"].clone(), json["fetchError"].clone()), (serde_json::Value::Null, serde_json::Value::Null));

    // behind: two commits on GitHub count once they are fetched, never before (nothing is fetched here)
    r.push(&[("app.php", "<?php\n")], "a");
    let to = r.push(&[("app.js", "1\n")], "b");
    assert_eq!(stand().behind, 0, "local only");
    let before = SystemTime::now() - Duration::from_secs(2);
    r.fetch();
    let s = stand();
    assert_eq!((s.behind, s.ahead, s.uncommitted, s.can_update), (2, 0, 0, true), "{s:?}");
    assert_eq!(s.update_says, format!("move main 2 commits to {}", &to[..7]));
    let at = s.fetched_at.expect("the fetch counts, from FETCH_HEAD");
    assert!(at >= before.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64, "{at}");

    // behind and ahead: main has a commit of its own
    commit(&r.folder, &[("local.txt", "mine\n")], "local");
    let s = stand();
    assert_eq!((s.behind, s.ahead, s.can_update), (2, 1, false), "{s:?}");
    assert!(s.update_says.starts_with("main in kade-portal has 1 commit that origin/main doesn't"), "{}", s.update_says);

    // only ahead: the own commit on top of main
    git(&r.folder, &["rebase", "-q", "refs/remotes/origin/main"]);
    let s = stand();
    assert_eq!((s.behind, s.ahead, s.can_update), (0, 1, false), "{s:?}");

    // uncommitted: a changed tracked file and a new one both count; Update names the tracked one
    git(&r.folder, &["reset", "-q", "--hard", "refs/remotes/origin/main"]);
    git(&r.folder, &["reset", "-q", "--hard", "HEAD~2"]);
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    std::fs::write(r.folder.join("notes.txt"), "new\n").unwrap();
    let s = stand();
    assert_eq!((s.behind, s.ahead, s.uncommitted, s.can_update), (2, 0, 2, false), "{s:?}");
    assert_eq!(s.update_says, "kade-portal has uncommitted changes in package.json. Commit or stash them first.");

    // on another branch: not on the default one, and counted from that branch
    git(&r.folder, &["checkout", "-q", "--", "package.json"]);
    git(&r.folder, &["switch", "-q", "-c", "feature/x"]);
    commit(&r.folder, &[("feature.txt", "x\n")], "feature");
    let s = stand();
    assert_eq!((s.branch.as_deref(), s.on_default, s.behind, s.ahead, s.uncommitted), (Some("feature/x"), false, 2, 1, 1), "{s:?}");
}

#[test]
fn update_says_why_it_cant_run_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    let main = main_ref();
    // not fetched yet: no main to move to
    let fresh = tmp.path().join("fresh");
    git(tmp.path(), &["clone", "-q", r.bare.to_str().unwrap(), fresh.to_str().unwrap()]);
    git(&fresh, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let e = behind::plan(named(&fresh), Ok(&main), None).unwrap_err();
    assert_eq!(e, "origin/main hasn't been fetched into kade-portal yet. Fetch it first.");

    r.push(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    r.fetch();
    let head = r.head();
    let refused = |what: &str| {
        let e = behind::plan(named(&r.folder), Ok(&main), None).unwrap_err();
        let s = behind::stand(named(&r.folder), Ok(&main), &Fetched::default(), None, None);
        assert!(!s.can_update, "{what}: {s:?}");
        assert_eq!(s.update_says, e, "{what}: the behind bar says the same");
        e
    };

    // uncommitted changes in a tracked file: named, an untracked file isn't
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    std::fs::write(r.folder.join("untracked.txt"), "doesn't count\n").unwrap();
    let porcelain = r.porcelain();
    assert_eq!(refused("uncommitted"), "kade-portal has uncommitted changes in package.json. Commit or stash them first.");
    assert_eq!((r.head(), r.porcelain()), (head.clone(), porcelain), "nothing changed");
    assert_eq!(std::fs::read_to_string(r.folder.join("package.json")).unwrap(), "{\"name\": \"wip\"}\n");
    git(&r.folder, &["checkout", "-q", "--", "package.json"]);
    std::fs::remove_file(r.folder.join("untracked.txt")).unwrap();

    // a merge in progress
    git(&r.folder, &["switch", "-q", "-c", "side"]);
    commit(&r.folder, &[("package.json", "{\"side\": 1}\n")], "side");
    git(&r.folder, &["switch", "-q", "main"]);
    let local = commit(&r.folder, &[("package.json", "{\"main\": 1}\n")], "local");
    assert!(!Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "merge", "-q", "side"]).current_dir(&r.folder)
        .output().unwrap().status.success(), "a conflict");
    let porcelain = r.porcelain();
    assert_eq!(refused("merge"), "a merge is in progress in kade-portal. Finish or abort it first.");
    assert_eq!((r.head(), r.porcelain()), (local.clone(), porcelain), "nothing changed");
    git(&r.folder, &["merge", "--abort"]);

    // main with a commit of its own: only a merge could bring origin/main in
    assert_eq!(refused("ahead"), "main in kade-portal has 1 commit that origin/main doesn't, so a fast-forward can't bring it up to \
                                  date. Push or merge it first, in a terminal or your own editor.");
    assert_eq!((r.head(), r.porcelain()), (local.clone(), String::new()), "nothing changed");
    assert_eq!(git_out(&r.folder, &["rev-list", "--count", "--merges", "HEAD"]), "0", "no merge commit");
    git(&r.folder, &["reset", "-q", "--hard", &head]);

    // another branch, or none: the Editor never switches
    git(&r.folder, &["switch", "-q", "-c", "feature/x"]);
    assert_eq!(refused("other branch"), "kade-portal is on branch feature/x, not main. Switch to main first, in a terminal or your own editor.");
    assert_eq!(git_out(&r.folder, &["branch", "--show-current"]), "feature/x", "it didn't switch");
    let card = behind::plan(Named { dir: &r.folder, name: "KADE-12", card: true }, Ok(&main), None).unwrap_err();
    assert_eq!(card, "KADE-12 is a card's worktree, on the card's branch feature/x: Update only brings a folder on main up to date.");
    git(&r.folder, &["switch", "-q", "--detach", "HEAD"]);
    assert_eq!(refused("no branch"), "kade-portal has no branch checked out. Switch to main first, in a terminal or your own editor.");
    git(&r.folder, &["switch", "-q", "main"]);

    // an agent works on it; outside git; no main to compare with
    assert_eq!(behind::plan(named(&r.folder), Ok(&main), Some("Backend Agent is working on KADE-12")).unwrap_err(),
               "Backend Agent is working on KADE-12.");
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(behind::plan(named(&plain), Ok(&main), None).unwrap_err(),
               "kade-portal isn't a git repository, so there's no main to compare it with.");
    let s = behind::stand(named(&plain), Ok(&main), &Fetched::default(), None, None);
    assert_eq!((s.main_label.as_str(), s.branch.as_deref(), s.can_update), ("", None, false), "{s:?}");
    let s = behind::stand(named(&r.folder), Err("kade-portal has no origin remote: Gizai can't tell whether it's behind."), &Fetched::default(),
                          None, None);
    assert_eq!((s.main_label.as_str(), s.branch.as_deref(), s.can_update, s.update_says.as_str()),
               ("", Some("main"), false, "kade-portal has no origin remote: Gizai can't tell whether it's behind."));

    // after all that: still where it was
    assert_eq!((r.head(), r.porcelain()), (head, String::new()));
}

#[test]
fn update_fast_forwards_and_installs_only_for_a_lock_file_that_changed_with_what_they_printed() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    let main = main_ref();
    let fake = tmp.path().join("fake");
    let path = fakes(&fake, false);

    // composer.lock changes; node_modules/ is only missing (package-lock.json as before): composer install alone
    std::fs::remove_dir_all(r.folder.join("node_modules")).unwrap();
    r.push(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let to = r.push(&[("app.js", "1\n")], "more");
    r.fetch();
    let plan = behind::plan(named(&r.folder), Ok(&main), None).unwrap();
    assert_eq!((plan.switch, plan.commits, plan.install.clone()), (false, 2, vec!["vendor".to_string()]), "{plan:?}");
    assert_eq!(plan.said(), format!("move main 2 commits to {} and run composer install", &to[..7]));
    let s = behind::stand(named(&r.folder), Ok(&main), &Fetched::default(), None, None);
    assert_eq!((s.can_update, s.update_says.clone()), (true, plan.said()));
    // while an update runs, the bar says what it does
    let s = behind::stand(named(&r.folder), Ok(&main), &Fetched::default(), Some(&plan.said()), None);
    assert_eq!((s.updating, s.can_update, s.update_says.clone()), (true, false, plan.said()));

    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!(r.head(), to, "at main's commit");
    assert_eq!(git_out(&r.folder, &["rev-list", "--count", "--merges", "HEAD"]), "0", "no merge commit");
    assert_eq!(calls(&fake), ["composer install --no-interaction --no-progress --no-ansi"], "npm didn't run");
    assert!(!r.folder.join("node_modules").exists(), "a folder that was only missing isn't installed");
    assert_eq!(done.printed, "Installing dependencies from lock file");
    let ended = behind::ended(r.folder.to_str().unwrap(), "kade-portal", End::Done(plan, done));
    assert_eq!((ended.folder.as_str(), ended.ok), (r.folder.to_str().unwrap(), true));
    assert_eq!(ended.said, format!("Moved main 2 commits to {} and ran composer install.", &to[..7]));
    assert_eq!(ended.output, "Installing dependencies from lock file");
    let json = serde_json::to_value(&ended).unwrap();
    let mut keys: Vec<&String> = json.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["folder", "ok", "output", "said"], "contract C3's EditorUpdated");

    // package-lock.json changes: npm ci alone
    std::fs::create_dir_all(r.folder.join("node_modules/.bin")).unwrap();
    let to = r.push(&[("package-lock.json", "{\"lockfileVersion\": 3, \"v\": 2}\n")], "new js packages");
    r.fetch();
    let plan = behind::plan(named(&r.folder), Ok(&main), None).unwrap();
    assert_eq!(plan.install, ["node_modules"]);
    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!(calls(&fake)[1..], ["npm ci --no-audit --no-fund".to_string()]);
    let ended = behind::ended("k", "kade-portal", End::Done(plan, done));
    assert_eq!((ended.said, ended.output), (format!("Moved main 1 commit to {} and ran npm ci.", &to[..7]), "added 12 packages in 1s".to_string()));

    // no lock file changes: only the fast-forward, which prints nothing
    let to = r.push(&[("README.md", "# Kade\n")], "docs");
    r.fetch();
    let plan = behind::plan(named(&r.folder), Ok(&main), None).unwrap();
    assert_eq!((plan.install.len(), plan.said()), (0, format!("move main 1 commit to {}", &to[..7])));
    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    let ended = behind::ended("k", "kade-portal", End::Done(plan, done));
    assert_eq!((ended.ok, ended.said, ended.output), (true, format!("Moved main 1 commit to {}.", &to[..7]), String::new()));
    assert_eq!(calls(&fake).len(), 2, "no install");
    assert_eq!(r.head(), to);
    assert_eq!(behind::plan(named(&r.folder), Ok(&main), None).unwrap_err(), "kade-portal is up to date with origin/main.");
}

#[test]
fn a_failing_install_ends_the_update_saying_what_it_did_before_with_the_output() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    let main = main_ref();
    let fake = tmp.path().join("fake");
    let path = fakes(&fake, true);
    let to = r.push(&[("package-lock.json", "{\"lockfileVersion\": 3, \"v\": 2}\n")], "new js packages");
    r.fetch();
    let plan = behind::plan(named(&r.folder), Ok(&main), None).unwrap();
    let stopped = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap_err();
    assert!(stopped.moved && stopped.installed.is_empty(), "{stopped:?}");
    assert_eq!(r.head(), to, "the fast-forward had run");
    let ended = behind::ended("k", "kade-portal", End::Stopped(plan, stopped));
    assert!(!ended.ok);
    assert_eq!(ended.said, format!("The update of kade-portal stopped: npm ci failed (exit code 1). Before that it moved main 1 commit to {}.",
                                   &to[..7]));
    assert_eq!(ended.output, "npm ERR! code EUSAGE");

    // refused when its turn came: it didn't start, and printed nothing
    let ended = behind::ended("k", "kade-portal", End::Refused("kade-portal has uncommitted changes in a.txt. Commit or stash them first.".into()));
    assert_eq!((ended.ok, ended.said.as_str(), ended.output.as_str()),
               (false, "The update of kade-portal didn't start: kade-portal has uncommitted changes in a.txt. Commit or stash them first.", ""));
}

#[test]
fn main_of_a_folder_that_isnt_a_projects_is_its_origins_default_branch() {
    let tmp = tempfile::tempdir().unwrap();
    // GitHub's default branch is trunk
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    git(&src, &["init", "-q", "-b", "trunk"]);
    commit(&src, &[("README.md", "x\n")], "init");
    let bare = tmp.path().join("shop.git");
    git(tmp.path(), &["clone", "-q", "--bare", src.to_str().unwrap(), bare.to_str().unwrap()]);

    // a clone knows origin's default branch
    let clone = tmp.path().join("clone");
    git(tmp.path(), &["clone", "-q", bare.to_str().unwrap(), clone.to_str().unwrap()]);
    let m = behind::origin_main(&clone).unwrap();
    assert_eq!((m.rev.as_str(), m.label.as_str(), m.branch.as_str()), ("refs/remotes/origin/trunk", "origin/trunk", "trunk"));

    // a repository that hasn't heard which branch that is: Fetch asks origin first and keeps the answer
    let added = tmp.path().join("added");
    std::fs::create_dir(&added).unwrap();
    git(&added, &["init", "-q", "-b", "trunk"]);
    git(&added, &["remote", "add", "origin", bare.to_str().unwrap()]);
    assert_eq!(behind::origin_main(&added).unwrap().label, "origin/main", "a guess until it is asked");
    assert_eq!(behind::fetched_at(&added, "trunk"), None);
    let m = behind::fetch_origin_main(&added, Duration::from_secs(60)).unwrap();
    assert_eq!((m.rev.as_str(), m.label.as_str(), m.branch.as_str()), ("refs/remotes/origin/trunk", "origin/trunk", "trunk"));
    assert_eq!(git_out(&added, &["rev-parse", "refs/remotes/origin/trunk"]), git_out(&bare, &["rev-parse", "trunk"]));
    assert_eq!(behind::origin_main(&added).unwrap().label, "origin/trunk", "now known locally");
    let at = behind::fetched_at(&added, "trunk").expect("FETCH_HEAD lists trunk");
    assert!(at.elapsed().unwrap() < Duration::from_secs(60));
    assert_eq!(behind::fetched_at(&added, "main"), None, "only main's own branch counts");

    // no origin remote: no main; an origin that doesn't answer: why, as a sentence
    let alone = tmp.path().join("alone");
    std::fs::create_dir(&alone).unwrap();
    git(&alone, &["init", "-q", "-b", "main"]);
    assert_eq!(behind::origin_main(&alone), None);
    git(&alone, &["remote", "add", "origin", tmp.path().join("gone.git").to_str().unwrap()]);
    let e = behind::fetch_origin_main(&alone, Duration::from_secs(20)).unwrap_err();
    assert!(e.starts_with("Couldn't fetch main from origin: "), "{e}");

    // what Gizai saw of its own fetches: a failure shows until a fetch works
    let failed = SystemTime::now();
    let seen = Fetched { ok_at: None, failed: Some((failed, e.clone())) };
    let s = behind::stand(Named { dir: &alone, name: "alone", card: false }, Ok(&behind::origin_main(&alone).unwrap()), &seen, None, None);
    assert_eq!((s.fetched_at, s.fetch_error.as_deref()), (None, Some(e.as_str())));
    let seen = Fetched { ok_at: Some(failed + Duration::from_secs(1)), failed: Some((failed, e.clone())) };
    let s = behind::stand(Named { dir: &alone, name: "alone", card: false }, Ok(&behind::origin_main(&alone).unwrap()), &seen, None, None);
    assert_eq!(s.fetch_error, None, "a later fetch worked");
    assert!(s.fetched_at.is_some());
}
