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
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, false).unwrap_err();
    assert!(e.contains("is on branch feature/x, not main") && e.contains("nothing changed"), "{e}");
    assert_eq!(git_out(&r.folder, &["branch", "--show-current"]), "feature/x");

    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, true).unwrap();
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
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false).unwrap();
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
        let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, true).unwrap_err();
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
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, true).unwrap_err();
    assert!(e.contains("a merge is in progress") && e.contains("nothing changed"), "{e}");
    git(&r.folder, &["merge", "--abort"]);

    // the local main has a commit main doesn't: only a merge could bring main in
    let e = checkout::plan(&r.folder, &r.folder, "main", MAIN, true).unwrap_err();
    assert!(e.contains("main in") && e.contains("has 1 commit that main doesn't") && e.contains("fast-forward"), "{e}");
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), head2);
    assert_eq!(checkout::status(&r.folder, "main", MAIN).unwrap().moves, None);
    assert_eq!(git_out(&r.folder, &["rev-list", "--count", "--merges", "HEAD"]), "0");

    // a folder that isn't a checkout of the project's repository
    let other = tmp.path().join("other");
    std::fs::create_dir(&other).unwrap();
    git(&other, &["init", "-q", "-b", "main"]);
    let e = checkout::plan(&other, &r.folder, "main", MAIN, true).unwrap_err();
    assert!(e.contains("isn't a checkout of the project's repository"), "{e}");
    let e = checkout::plan(&r.folder.join("vendor"), &r.folder, "main", MAIN, true).unwrap_err();
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
    let plan = checkout::plan(&r.folder, &r.folder, "main", MAIN, false).unwrap();
    let stopped = checkout::update(&r.folder, &plan, Some(path.as_os_str())).unwrap_err();
    assert!(stopped.why.contains("composer install"), "{stopped:?}");
    assert!(stopped.output.contains("could not be resolved"), "{stopped:?}");
    assert_eq!(stopped.done.len(), 1, "{stopped:?}");
    assert!(stopped.done[0].starts_with("moved main from"), "{stopped:?}");
    assert_eq!(git_out(&r.folder, &["rev-parse", "HEAD"]), to);
}
