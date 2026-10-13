//! GA-44 part 2: how a project's linked folder (your own checkout) stands against main, and its update to main once
//! you said yes in chat. "GitHub" is a local bare repository; composer and npm are fakes, never the real ones.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use gizai_agents::checkout::{self, DepBehind};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAIN: &str = "refs/remotes/origin/main";

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn git_out(dir: &Path, args: &[&str]) -> String {
    String::from_utf8(Command::new("git").args(args).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

fn commit(dir: &Path, files: &[(&str, &str)], msg: &str) -> String {
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
        git(dir, &["add", name]);
    }
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", msg]);
    git_out(dir, &["rev-parse", "HEAD"])
}

struct Repos {
    /// Where "GitHub's" main is written.
    src: PathBuf,
    bare: PathBuf,
    /// The linked folder: a clone of GitHub, on main, with vendor/ and node_modules/ installed.
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
        let folder = tmp.join("herd");
        git(tmp, &["clone", "-q", bare.to_str().unwrap(), folder.to_str().unwrap()]);
        std::fs::create_dir_all(folder.join("vendor/composer")).unwrap();
        std::fs::create_dir_all(folder.join("node_modules/.bin")).unwrap();
        Repos { src, bare, folder }
    }

    /// A commit on GitHub's main, then the fetch Gizai does before a turn.
    fn github_commit(&self, files: &[(&str, &str)], msg: &str) -> String {
        let sha = commit(&self.src, files, msg);
        git(&self.src, &["push", "-q", self.bare.to_str().unwrap(), "main"]);
        git(&self.folder, &["fetch", "-q", "origin"]);
        sha
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

/// Fake composer and npm (as in GA-30's tests): each call goes to dir/calls; composer makes vendor/, npm node_modules/.
fn fakes(dir: &Path) -> OsString {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let d = dir.display();
    write_script(&bin.join("composer"), &format!("#!/bin/sh\necho \"composer $*\" >> '{d}/calls'\nmkdir -p vendor/composer\n"));
    write_script(&bin.join("npm"), &format!("#!/bin/sh\necho \"npm $*\" >> '{d}/calls'\nmkdir -p node_modules/.bin\n"));
    let mut path = vec![bin];
    path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    std::env::join_paths(path).unwrap()
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls")).unwrap_or_default().lines().map(str::to_string).collect()
}

#[test]
fn a_folder_that_only_misses_commits_is_not_outdated() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    let s = checkout::status(&r.folder, "main", MAIN).unwrap();
    assert_eq!((s.branch.as_deref(), s.behind, s.moves, s.deps.len(), s.outdated()), (Some("main"), 0, Some(0), 0, false));
    r.github_commit(&[("app.php", "<?php\n")], "a");
    r.github_commit(&[("app.js", "1\n")], "b");
    let s = checkout::status(&r.folder, "main", MAIN).unwrap();
    assert_eq!((s.behind, s.moves, s.outdated()), (2, Some(2), false), "{s:?}");
}

#[test]
fn a_lock_file_that_differs_from_mains_or_a_missing_folder_makes_it_outdated() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let s = checkout::status(&r.folder, "main", MAIN).unwrap();
    assert!(s.outdated());
    assert_eq!(s.deps, vec![DepBehind { folder: "vendor".into(), lock: "composer.lock".into(), missing: false }]);
    assert_eq!(s.deps[0].said(), "composer.lock differs from main's");
    std::fs::remove_dir_all(r.folder.join("node_modules")).unwrap();
    let s = checkout::status(&r.folder, "main", MAIN).unwrap();
    assert_eq!(s.deps.iter().map(DepBehind::said).collect::<Vec<_>>(), ["composer.lock differs from main's", "node_modules/ is missing"]);
    // looking changed nothing
    assert_eq!(std::fs::read_to_string(r.folder.join("composer.lock")).unwrap(), "{\"v\": 1}\n");
    assert_eq!(git_out(&r.folder, &["status", "--porcelain"]), "");
}

#[test]
fn update_switches_when_asked_fast_forwards_and_installs_only_what_is_behind() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    git(&r.folder, &["switch", "-q", "-c", "feature/x"]);
    let feature = commit(&r.folder, &[("feature.txt", "mine\n")], "my feature");
    let old_main = git_out(&r.folder, &["rev-parse", "main"]);
    r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let to = r.github_commit(&[("app.js", "1\n")], "more");

    // not asked to switch: left alone, and the answer says so
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, false).unwrap_err();
    assert!(e.contains("is on branch feature/x, not main") && e.contains("nothing changed"), "{e}");
    assert_eq!(git_out(&r.folder, &["branch", "--show-current"]), "feature/x");

    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, true, false).unwrap();
    assert_eq!((plan.on.as_deref(), plan.switch, plan.commits, plan.from.as_str(), plan.to.as_str()),
               (Some("feature/x"), true, 2, old_main.as_str(), to.as_str()));
    assert_eq!(plan.install, ["vendor"], "package-lock.json is as main's and node_modules/ is there");
    let said = plan.said();
    assert!(said.contains("switch from feature/x to main (feature/x stays as it is)") && said.contains("move main 2 commits to")
            && said.contains("run composer install") && !said.contains("npm"), "{said}");

    let fake = tmp.path().join("fake");
    let path = fakes(&fake);
    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!(done.switched_from.as_deref(), Some("feature/x"));
    assert_eq!(done.installed, ["composer install"]);
    assert_eq!(git_out(&r.folder, &["branch", "--show-current"]), "main");
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), to, "at main's commit");
    assert_eq!(git_out(&r.folder, &["rev-list", "--count", "--merges", "HEAD"]), "0", "no merge commit");
    assert_eq!(git_out(&r.folder, &["rev-parse", "feature/x"]), feature, "the feature branch is unchanged");
    assert_eq!(calls(&fake), ["composer install --no-interaction --no-progress --no-ansi"], "npm didn't run");
    assert!(!checkout::status(&r.folder, "main", MAIN).unwrap().outdated());

    // a missing node_modules/ on main: only npm ci runs
    std::fs::remove_dir_all(r.folder.join("node_modules")).unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, false).unwrap();
    assert_eq!((plan.switch, plan.commits, plan.install.clone()), (false, 0, vec!["node_modules".to_string()]));
    checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!(calls(&fake)[1..], ["npm ci --no-audit --no-fund".to_string()]);
    assert!(r.folder.join("node_modules").is_dir());
}

#[test]
fn uncommitted_changes_a_merge_in_progress_or_a_local_main_with_its_own_commits_change_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let head = git_out(&r.folder, &["rev-parse", "HEAD"]);
    let refused = |what: &str, want: &str| {
        let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, true, false).unwrap_err();
        assert!(e.contains(want) && e.contains("nothing changed"), "{what}: {e}");
        assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), head, "{what}: HEAD didn't move");
        e
    };

    // uncommitted changes to tracked files: named
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    std::fs::write(r.folder.join("untracked.txt"), "doesn't count\n").unwrap();
    let e = refused("uncommitted", "has uncommitted changes in package.json");
    assert!(!e.contains("untracked.txt"), "{e}");
    assert_eq!(std::fs::read_to_string(r.folder.join("package.json")).unwrap(), "{\"name\": \"wip\"}\n");
    git(&r.folder, &["checkout", "-q", "--", "package.json"]);

    // a merge in progress
    git(&r.folder, &["switch", "-q", "-c", "side"]);
    commit(&r.folder, &[("package.json", "{\"side\": 1}\n")], "side");
    git(&r.folder, &["switch", "-q", "main"]);
    commit(&r.folder, &[("package.json", "{\"main\": 1}\n")], "local");
    let head2 = git_out(&r.folder, &["rev-parse", "HEAD"]);
    assert!(!Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "merge", "-q", "side"]).current_dir(&r.folder)
        .output().unwrap().status.success(), "a conflict");
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, true, false).unwrap_err();
    assert!(e.contains("a merge is in progress") && e.contains("nothing changed"), "{e}");
    git(&r.folder, &["merge", "--abort"]);

    // the local main has a commit main doesn't: only a merge could bring main in
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, true, false).unwrap_err();
    assert!(e.contains("main in") && e.contains("has 1 commit that main doesn't") && e.contains("fast-forward"), "{e}");
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), head2);
    assert_eq!(checkout::status(&r.folder, "main", MAIN).unwrap().moves, None);
    assert_eq!(git_out(&r.folder, &["rev-list", "--count", "--merges", "HEAD"]), "0");

    // a folder that isn't a checkout of the project's repository
    let other = tmp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    git(&other, &["init", "-q", "-b", "main"]);
    let e = checkout::plan(&other, &r.folder, "main", MAIN, true, false).unwrap_err();
    assert!(e.contains("isn't a checkout of the project's repository"), "{e}");
    let e = checkout::plan(&r.folder.join("vendor"), &r.folder, "main", MAIN, true, false).unwrap_err();
    assert!(e.contains("isn't a checkout of the project's repository"), "a folder inside it: {e}");
}

#[test]
fn a_failing_install_stops_the_update_and_says_what_it_had_done() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    std::fs::remove_dir_all(r.folder.join("vendor")).unwrap();
    let to = r.github_commit(&[("app.php", "<?php\n")], "a");
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    write_script(&bin.join("composer"), "#!/bin/sh\necho 'Your requirements could not be resolved' >&2\nexit 2\n");
    let path = std::env::join_paths([bin, "/usr/bin".into(), "/bin".into()]).unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, false).unwrap();
    let stopped = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap_err();
    assert!(stopped.why.contains("composer install"), "{stopped:?}");
    assert!(stopped.output.contains("could not be resolved"), "{stopped:?}");
    assert_eq!(stopped.done.len(), 1, "{stopped:?}");
    assert!(stopped.done[0].starts_with("moved main from"), "{stopped:?}");
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), to);
}

// ---- GA-156: with stash, uncommitted changes in tracked files go to git's stash first ----

/// Who makes commits in the folder, so git can make a stash entry there whatever the machine's own git config says. An
/// empty name makes git refuse.
fn identity(dir: &Path, name: &str) {
    git(dir, &["config", "user.name", name]);
    git(dir, &["config", "user.email", "t@t"]);
}

/// `git stash list`, newest first: "On main: Gizai: before the update to main".
fn stashes(dir: &Path) -> Vec<String> {
    git_out(dir, &["stash", "list", "--format=%gs"]).lines().map(str::to_string).collect()
}

fn read(dir: &Path, file: &str) -> String {
    std::fs::read_to_string(dir.join(file)).unwrap()
}

const LOCAL_LOCK: &str = "{\"lockfileVersion\": 3, \"packages\": {\"left-pad\": {}}}\n";

#[test]
fn with_stash_uncommitted_changes_go_to_gits_stash_first_then_it_fast_forwards_and_installs() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    let old_main = git_out(&r.folder, &["rev-parse", "HEAD"]);
    r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let to = r.github_commit(&[("app.js", "1\n")], "more");
    // a local npm install changed the lock file; a new file of yours isn't tracked
    std::fs::write(r.folder.join("package-lock.json"), LOCAL_LOCK).unwrap();
    std::fs::write(r.folder.join("notes.txt"), "mine\n").unwrap();

    // without stash: refused as before, with the new ending, and nothing changes
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, false).unwrap_err();
    assert_eq!(e, format!("{} has uncommitted changes in package-lock.json: nothing changed. Commit or stash them first, or call again \
                           with stash only when the user agreed to put them in git's stash", r.folder.display()));

    // with stash: it puts them in git's stash first; the installs are as the lock file is now, so npm ci too
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    assert_eq!((plan.stash.clone(), plan.switch, plan.commits, plan.install.clone()),
               (vec!["package-lock.json".to_string()], false, 2, vec!["vendor".to_string(), "node_modules".to_string()]));
    assert_eq!(plan.said(), format!("put the uncommitted changes in package-lock.json in git's stash, move main 2 commits to {} and run \
                                     composer install and npm ci", &to[..7]));
    // looking changed nothing
    assert_eq!((read(&r.folder, "package-lock.json"), stashes(&r.folder), git_out(&r.folder, &["rev-parse", "HEAD"])),
               (LOCAL_LOCK.to_string(), vec![], old_main));

    let fake = tmp.path().join("fake");
    let path = fakes(&fake);
    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!(done.stashed, ["package-lock.json"]);
    assert_eq!(done.installed, ["composer install", "npm ci"]);
    assert_eq!(calls(&fake), ["composer install --no-interaction --no-progress --no-ansi", "npm ci --no-audit --no-fund"]);
    // one stash entry, holding the lock file's change; the folder is clean and at main's commit
    assert_eq!(stashes(&r.folder), ["On main: Gizai: before the update to main"]);
    assert_eq!(git_out(&r.folder, &["stash", "show", "--name-only", "stash@{0}"]), "package-lock.json");
    assert_eq!(git_out(&r.folder, &["status", "--porcelain"]), "?? notes.txt", "clean, but for the untracked file");
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), to);
    assert_eq!(read(&r.folder, "package-lock.json"), "{\"lockfileVersion\": 3}\n", "main's lock");
    assert!(!checkout::status(&r.folder, "main", MAIN).unwrap().outdated());
    // the untracked file stays where it was, and isn't in the stash
    assert_eq!(read(&r.folder, "notes.txt"), "mine\n");
    assert!(!Command::new("git").args(["rev-parse", "--verify", "--quiet", "stash@{0}^3"]).current_dir(&r.folder).output().unwrap().status.success(),
            "the stash has no untracked files");

    // git stash pop brings the change back
    git(&r.folder, &["stash", "pop", "-q"]);
    assert_eq!(read(&r.folder, "package-lock.json"), LOCAL_LOCK);
    assert!(stashes(&r.folder).is_empty());
}

#[test]
fn a_lock_file_changed_by_a_local_install_on_mains_commit_is_stashed_and_gets_its_install() {
    // 13 Oct: the folder on main's commit, with only package-lock.json changed by a local npm install
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    let head = git_out(&r.folder, &["rev-parse", "HEAD"]);
    std::fs::write(r.folder.join("package-lock.json"), LOCAL_LOCK).unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    assert_eq!(plan.said(), format!("put the uncommitted changes in package-lock.json in git's stash, leave main where it is ({}, already \
                                     main's commit) and run npm ci", &head[..7]));
    let fake = tmp.path().join("fake");
    let path = fakes(&fake);
    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!((done.stashed, done.installed, done.from == done.to), (vec!["package-lock.json".to_string()], vec!["npm ci".to_string()], true));
    assert_eq!(calls(&fake), ["npm ci --no-audit --no-fund"]);
    assert_eq!((stashes(&r.folder), git_out(&r.folder, &["status", "--porcelain"]), git_out(&r.folder, &["rev-parse", "HEAD"])),
               (vec!["On main: Gizai: before the update to main".to_string()], String::new(), head));
    git(&r.folder, &["stash", "pop", "-q"]);
    assert_eq!(read(&r.folder, "package-lock.json"), LOCAL_LOCK);
}

#[test]
fn with_stash_and_nothing_uncommitted_no_stash_entry_is_made_and_the_update_runs_as_before() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    let to = r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    std::fs::write(r.folder.join("notes.txt"), "an untracked file doesn't count\n").unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    assert_eq!(plan, checkout::plan(&r.folder, &r.folder, "main", MAIN, false, false).unwrap(), "the plan without stash");
    assert!(plan.stash.is_empty());
    assert_eq!(plan.said(), format!("move main 1 commit to {} and run composer install", &to[..7]));
    let fake = tmp.path().join("fake");
    let path = fakes(&fake);
    let done = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap();
    assert_eq!((done.stashed.len(), done.installed), (0, vec!["composer install".to_string()]));
    assert!(stashes(&r.folder).is_empty(), "no stash entry");
    assert_eq!((git_out(&r.folder, &["rev-parse", "HEAD"]), git_out(&r.folder, &["status", "--porcelain"])), (to, "?? notes.txt".to_string()));
}

#[test]
fn changes_that_went_after_the_check_are_not_said_to_be_stashed_and_your_own_stash_entries_stay() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    // an older stash entry of yours
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"older\"}\n").unwrap();
    git(&r.folder, &["stash", "push", "-q", "-m", "mine"]);
    let mine = git_out(&r.folder, &["rev-parse", "stash@{0}"]);
    let to = r.github_commit(&[("app.js", "1\n")], "more");
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    assert_eq!(plan.stash, ["package.json"]);

    // the change is gone by the time it runs: git stashes nothing, and nothing says it did
    git(&r.folder, &["checkout", "-q", "--", "package.json"]);
    let done = checkout::update(&r.folder, &plan, None).unwrap();
    assert!(done.stashed.is_empty(), "{done:?}");
    assert_eq!(stashes(&r.folder), ["On main: mine"]);
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), to);

    // with a change it goes on top of yours, which stays as it was
    let to = r.github_commit(&[("app.js", "2\n")], "even more");
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    let done = checkout::update(&r.folder, &plan, None).unwrap();
    assert_eq!(done.stashed, ["package.json"]);
    assert_eq!(stashes(&r.folder), ["On main: Gizai: before the update to main", "On main: mine"]);
    assert_eq!(git_out(&r.folder, &["rev-parse", "stash@{1}"]), mine);
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), to);
}

#[test]
fn the_plan_and_the_refusal_name_at_most_ten_files() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    let names: Vec<String> = (1..=12).map(|i| format!("f{i:02}.txt")).collect();
    let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "a\n")).collect();
    let to = r.github_commit(&files, "twelve files");
    git(&r.folder, &["merge", "-q", "--ff-only", MAIN]);
    for n in &names {
        std::fs::write(r.folder.join(n), "b\n").unwrap();
    }
    let ten = names[..10].join(", ");
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, false).unwrap_err();
    assert!(e.contains(&format!("has uncommitted changes in {ten} and 2 more: nothing changed")), "{e}");
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    assert_eq!(plan.stash, names);
    assert_eq!(plan.said(), format!("put the uncommitted changes in {ten} and 2 more in git's stash and leave main where it is ({}, already \
                                     main's commit)", &to[..7]));
    let done = checkout::update(&r.folder, &plan, None).unwrap();
    assert_eq!(done.stashed, names);
    assert_eq!(git_out(&r.folder, &["stash", "show", "--name-only", "stash@{0}"]).lines().count(), 12);
    assert_eq!(git_out(&r.folder, &["status", "--porcelain"]), "");
}

#[test]
fn with_stash_a_merge_in_progress_another_branch_or_a_local_main_with_its_own_commits_still_change_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    let now = || (git_out(&r.folder, &["rev-parse", "HEAD"]), git_out(&r.folder, &["branch", "--show-current"]),
                  git_out(&r.folder, &["status", "--porcelain"]));
    let refused = |what: &str, switch: bool, want: &str| {
        let before = now();
        let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, switch, true).unwrap_err();
        assert!(e.contains(want) && e.contains("nothing changed"), "{what}: {e}");
        assert!(stashes(&r.folder).is_empty(), "{what}: nothing stashed");
        assert_eq!(now(), before, "{what}: nothing changed");
    };

    // another branch, not asked to switch (and no branch at all)
    git(&r.folder, &["switch", "-q", "-c", "feature/x"]);
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    refused("other branch", false, "is on branch feature/x, not main");
    git(&r.folder, &["switch", "-q", "--detach"]);
    refused("no branch", false, "has no branch checked out");
    git(&r.folder, &["switch", "-q", "main"]);

    // the local main has a commit main doesn't, even when switching is allowed
    commit(&r.folder, &[("local.txt", "mine\n")], "local");
    refused("ahead", true, "has 1 commit that main doesn't");
    git(&r.folder, &["checkout", "-q", "--", "package.json"]);

    // a merge in progress
    git(&r.folder, &["switch", "-q", "-c", "side"]);
    commit(&r.folder, &[("package.json", "{\"side\": 1}\n")], "side");
    git(&r.folder, &["switch", "-q", "main"]);
    commit(&r.folder, &[("package.json", "{\"main\": 1}\n")], "local 2");
    assert!(!Command::new("git").args(["merge", "-q", "side"]).current_dir(&r.folder).output().unwrap().status.success(), "a conflict");
    refused("merge", true, "a merge is in progress");
    assert!(Command::new("git").args(["rev-parse", "-q", "--verify", "MERGE_HEAD"]).current_dir(&r.folder).output().unwrap().status.success(),
            "the merge is still in progress");
    git(&r.folder, &["merge", "--abort"]);
}

#[test]
fn when_git_cant_stash_the_update_stops_before_the_fast_forward_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    // no name to put on the stash entry: git won't make one
    identity(&r.folder, "");
    let head = git_out(&r.folder, &["rev-parse", "HEAD"]);
    r.github_commit(&[("composer.lock", "{\"v\": 2}\n")], "new php packages");
    std::fs::write(r.folder.join("package-lock.json"), LOCAL_LOCK).unwrap();
    std::fs::write(r.folder.join("vendor/composer/installed.json"), "as it was\n").unwrap();
    std::fs::write(r.folder.join("node_modules/.bin/tool"), "as it was\n").unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    assert_eq!((plan.stash.clone(), plan.install.clone()), (vec!["package-lock.json".to_string()], vec!["vendor".to_string(), "node_modules".to_string()]));

    let fake = tmp.path().join("fake");
    let path = fakes(&fake);
    let stopped = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap_err();
    assert_eq!(stopped.why, "putting the uncommitted changes in git's stash failed");
    assert!(stopped.output.contains("ident"), "git's error: {stopped:?}");
    assert!(stopped.done.is_empty() && stopped.stashed.is_empty() && !stopped.moved && stopped.installed.is_empty(), "{stopped:?}");
    // main, the change and the dependency folders are as they were; no install ran
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), head);
    assert_eq!(read(&r.folder, "package-lock.json"), LOCAL_LOCK);
    assert!(stashes(&r.folder).is_empty());
    assert!(calls(&fake).is_empty(), "{:?}", calls(&fake));
    assert_eq!((read(&r.folder, "vendor/composer/installed.json"), read(&r.folder, "node_modules/.bin/tool")),
               ("as it was\n".to_string(), "as it was\n".to_string()));
}

#[test]
fn gits_hooks_dont_run_when_it_stashes_switches_and_fast_forwards() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    let to = r.github_commit(&[("app.js", "1\n")], "more");
    git(&r.folder, &["switch", "-q", "-c", "feature/x"]);
    let feature = git_out(&r.folder, &["rev-parse", "HEAD"]);
    std::fs::write(r.folder.join("package.json"), "{\"name\": \"wip\"}\n").unwrap();
    let ran = tmp.path().join("hooks-ran");
    let hooks = PathBuf::from(git_out(&r.folder, &["rev-parse", "--path-format=absolute", "--git-path", "hooks"]));
    std::fs::create_dir_all(&hooks).unwrap();
    for hook in ["pre-commit", "post-commit", "post-checkout", "post-merge", "reference-transaction", "post-index-change"] {
        write_script(&hooks.join(hook), &format!("#!/bin/sh\necho {hook} >> '{}'\n", ran.display()));
    }
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, true, true).unwrap();
    assert_eq!(plan.said(), format!("put the uncommitted changes in package.json in git's stash, switch from feature/x to main (feature/x \
                                     stays as it is) and move main 1 commit to {}", &to[..7]));
    let done = checkout::update(&r.folder, &plan, None).unwrap();
    assert_eq!((done.stashed, done.switched_from.as_deref()), (vec!["package.json".to_string()], Some("feature/x")));
    assert!(!ran.exists(), "hooks ran: {:?}", std::fs::read_to_string(&ran));
    assert_eq!(stashes(&r.folder), ["On feature/x: Gizai: before the update to main"]);
    assert_eq!((git_out(&r.folder, &["branch", "--show-current"]), git_out(&r.folder, &["rev-parse", "HEAD"])), ("main".to_string(), to));
    assert_eq!(git_out(&r.folder, &["rev-parse", "feature/x"]), feature, "the feature branch stays as it is");
    // the hooks work: a commit of your own runs them
    commit(&r.folder, &[("local.txt", "mine\n")], "local");
    assert!(ran.exists(), "the hooks are live");
}

#[test]
fn when_a_later_step_fails_the_stop_says_the_changes_went_to_gits_stash() {
    let tmp = tempfile::tempdir().unwrap();
    let r = Repos::new(tmp.path());
    identity(&r.folder, "t");
    std::fs::write(r.folder.join("package-lock.json"), LOCAL_LOCK).unwrap();
    let bin = tmp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    write_script(&bin.join("npm"), "#!/bin/sh\necho 'npm ERR! code EUSAGE' >&2\nexit 1\n");
    let path = std::env::join_paths([bin, "/usr/bin".into(), "/bin".into()]).unwrap();
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false, true).unwrap();
    let stopped = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap_err();
    assert_eq!(stopped.why, "npm ci failed (exit code 1)", "{stopped:?}");
    assert_eq!((stopped.stashed.clone(), stopped.done.clone()),
               (vec!["package-lock.json".to_string()], vec!["put the uncommitted changes in package-lock.json in git's stash".to_string()]));
    assert_eq!(stashes(&r.folder), ["On main: Gizai: before the update to main"], "the change stays in the stash");
}
