use gizai_agents::prepare::{self, Prepare};
use gizai_agents::worktree;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) { assert!(Command::new("git").args(args).current_dir(dir).status().unwrap().success(), "git {args:?}"); }

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", msg]);
}

fn rev(dir: &Path, r: &str) -> String {
    String::from_utf8(Command::new("git").args(["rev-parse", r]).current_dir(dir).output().unwrap().stdout).unwrap().trim().to_string()
}

/// A main checkout with `files` committed, and the ignores a PHP and JS project has.
fn repo(tmp: &Path, files: &[(&str, &str)]) -> PathBuf {
    let repo = tmp.join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join(".gitignore"), ".env\nvendor/\nnode_modules/\ntarget/\n").unwrap();
    for (name, text) in files {
        std::fs::write(repo.join(name), text).unwrap();
    }
    commit_all(&repo, "init");
    repo
}

fn new_worktree(tmp: &Path, repo: &Path) -> PathBuf {
    worktree::ensure(repo, &tmp.join("wt"), "KADE-1", "Export invoices", "main").unwrap().path
}

/// Writes an executable script through a child sh, so a fork elsewhere in the test process never holds it open
/// for writing ("Text file busy").
fn write_script(path: &Path, script: &str) {
    use std::io::Write;
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path)
        .stdin(std::process::Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// Fake composer and npm in `dir/bin`, never the real ones (no network). Each call goes to dir/calls as one line,
/// and whether its stdin was closed to dir/stdin; composer makes vendor/, npm makes node_modules/. The PATH returned
/// has the fakes first, then the system's folders (for bash, cp, mkdir).
fn fakes(dir: &Path) -> OsString {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let d = dir.display();
    write_script(&bin.join("composer"), &format!(r#"#!/bin/sh
if read -r _line; then echo "composer: stdin open" >> '{d}/stdin'; else echo "composer: stdin closed" >> '{d}/stdin'; fi
echo "composer $*" >> '{d}/calls'
mkdir -p vendor/composer
echo '<?php' > vendor/autoload.php
"#));
    write_script(&bin.join("npm"), &format!(r#"#!/bin/sh
if read -r _line; then echo "npm: stdin open" >> '{d}/stdin'; else echo "npm: stdin closed" >> '{d}/stdin'; fi
echo "npm $*" >> '{d}/calls'
mkdir -p node_modules/.bin
"#));
    let mut path = vec![bin];
    path.extend(["/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    std::env::join_paths(path).unwrap()
}

fn calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("calls")).unwrap_or_default().lines().map(str::to_string).collect()
}

const PHP_AND_JS: [(&str, &str); 3] = [("composer.json", "{}\n"), ("package.json", "{}\n"), ("package-lock.json", "{\"lockfileVersion\": 3}\n")];

#[test]
fn a_worktree_without_vendor_and_node_modules_ends_up_with_both_before_the_setup_command() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &PHP_AND_JS);
    std::fs::write(repo.join(".env"), "APP_KEY=secret\n").unwrap();
    let wt = new_worktree(tmp.path(), &repo);
    assert!(!wt.join("vendor").exists() && !wt.join("node_modules").exists());
    let path = fakes(tmp.path());
    let log = tmp.path().join("calls");
    let plan = Prepare {
        // the main checkout has neither folder: both are skipped, then installed
        copy: vec![".env".into(), "vendor/".into(), "node_modules/".into()],
        install: true,
        setup: Some(format!("echo \"setup vendor=$(test -f vendor/autoload.php && echo y) node_modules=$(test -d node_modules && echo y)\" >> '{}'", log.display())),
    };
    let did = prepare::prepare(&repo, &wt, &plan, None, Some(&path)).unwrap();

    assert!(wt.join("vendor/autoload.php").is_file(), "composer install made vendor/");
    assert!(wt.join("node_modules").is_dir(), "npm made node_modules/");
    assert_eq!(std::fs::read_to_string(wt.join(".env")).unwrap(), "APP_KEY=secret\n", "the .env is copied");
    assert_eq!(calls(tmp.path()), [
        "composer install --no-interaction --no-progress --no-ansi",
        "npm ci --no-audit --no-fund",
        "setup vendor=y node_modules=y",
    ], "install first (npm ci: there is a package-lock.json), then the setup command, which sees both folders");
    let stdin = std::fs::read_to_string(tmp.path().join("stdin")).unwrap();
    assert_eq!(stdin, "composer: stdin closed\nnpm: stdin closed\n", "nothing can prompt");
    assert_eq!(did.copied, [".env"]);
    assert_eq!(did.skipped, ["vendor/", "node_modules/"], "a path the main checkout doesn't have is skipped, not an error");
    assert_eq!(did.installed, ["composer install", "npm ci"]);
    assert!(did.setup);
    assert_eq!(did.summary(), "copied .env; ran composer install and npm ci and the setup command");
    assert_eq!(worktree::uncommitted(&wt).unwrap(), 0, "the copies and installs are ignored files, the worktree stays clean");
}

#[test]
fn without_a_package_lock_npm_install_runs_and_writes_no_lock_file() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &[("package.json", "{}\n")]);
    let wt = new_worktree(tmp.path(), &repo);
    let path = fakes(tmp.path());
    let did = prepare::prepare(&repo, &wt, &Prepare { install: true, ..Default::default() }, None, Some(&path)).unwrap();
    assert_eq!(calls(tmp.path()), ["npm install --no-package-lock --no-audit --no-fund"]);
    assert_eq!(did.installed, ["npm install"]);
    assert!(wt.join("node_modules").is_dir());
    assert!(!wt.join("vendor").exists(), "no composer.json, no composer install");
}

#[test]
fn folders_copied_from_the_main_checkout_are_not_installed_again() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &PHP_AND_JS);
    std::fs::create_dir_all(repo.join("vendor/acme")).unwrap();
    std::fs::write(repo.join("vendor/acme/lib.php"), "<?php // warm\n").unwrap();
    std::fs::create_dir_all(repo.join("node_modules/.bin")).unwrap();
    std::fs::write(repo.join("node_modules/.bin/tool"), "#!/bin/sh\n").unwrap();
    std::process::Command::new("chmod").args(["755"]).arg(repo.join("node_modules/.bin/tool")).status().unwrap();
    let wt = new_worktree(tmp.path(), &repo);
    let path = fakes(tmp.path());
    let plan = Prepare { copy: vec!["vendor/".into(), "./node_modules".into()], install: true, setup: None };
    let did = prepare::prepare(&repo, &wt, &plan, None, Some(&path)).unwrap();

    assert_eq!(did.copied, ["vendor/", "./node_modules"]);
    assert!(did.installed.is_empty(), "{:?}", did.installed);
    assert!(calls(tmp.path()).is_empty(), "neither composer nor npm ran: {:?}", calls(tmp.path()));
    assert_eq!(std::fs::read_to_string(wt.join("vendor/acme/lib.php")).unwrap(), "<?php // warm\n");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(wt.join("node_modules/.bin/tool")).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755, "cp -a keeps modes");
    assert!(repo.join("vendor/acme/lib.php").is_file(), "the main checkout keeps its own");
}

#[test]
fn a_folder_already_in_the_worktree_is_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &PHP_AND_JS);
    std::fs::create_dir_all(repo.join("node_modules/main-only")).unwrap();
    let wt = new_worktree(tmp.path(), &repo);
    std::fs::create_dir_all(wt.join("node_modules/mine")).unwrap();
    std::fs::create_dir_all(wt.join("vendor")).unwrap();
    let path = fakes(tmp.path());
    let plan = Prepare { copy: vec!["node_modules/".into()], install: true, setup: None };
    let did = prepare::prepare(&repo, &wt, &plan, None, Some(&path)).unwrap();
    assert_eq!(did.skipped, ["node_modules/"], "the worktree already has it: not copied over");
    assert!(wt.join("node_modules/mine").is_dir() && !wt.join("node_modules/main-only").exists());
    assert!(calls(tmp.path()).is_empty(), "{:?}", calls(tmp.path()));
}

#[test]
fn the_install_can_be_switched_off() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &PHP_AND_JS);
    let wt = new_worktree(tmp.path(), &repo);
    let path = fakes(tmp.path());
    let plan = Prepare { copy: vec![], install: false, setup: Some("touch .setup-ran".into()) };
    let did = prepare::prepare(&repo, &wt, &plan, None, Some(&path)).unwrap();
    assert!(calls(tmp.path()).is_empty(), "{:?}", calls(tmp.path()));
    assert!(!wt.join("vendor").exists() && !wt.join("node_modules").exists());
    assert!(did.installed.is_empty());
    assert!(did.setup && wt.join(".setup-ran").is_file(), "the setup command still runs");
}

#[test]
fn a_missing_composer_fails_readably_and_nothing_after_it_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &[("composer.json", "{}\n")]);
    let wt = new_worktree(tmp.path(), &repo);
    let empty = tmp.path().join("empty-bin");
    std::fs::create_dir(&empty).unwrap();
    let plan = Prepare { copy: vec![], install: true, setup: Some("touch .setup-ran".into()) };
    let failed = prepare::prepare(&repo, &wt, &plan, None, Some(empty.as_os_str())).unwrap_err();
    assert_eq!(failed.command, "composer install");
    assert!(failed.why.contains("composer isn't installed"), "{failed}");
    assert!(!wt.join(".setup-ran").exists(), "the setup command doesn't run after a failed install");
}

#[test]
fn a_failing_setup_command_gives_its_exit_code_and_output() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &[("README.md", "hi\n")]);
    let wt = new_worktree(tmp.path(), &repo);
    let plan = Prepare { copy: vec![], install: true, setup: Some("echo migrating; echo 'SQLSTATE: no such table' >&2; exit 3".into()) };
    let failed = prepare::prepare(&repo, &wt, &plan, None, None).unwrap_err();
    assert_eq!(failed.why, "failed (exit code 3)");
    assert!(failed.output.contains("migrating") && failed.output.contains("SQLSTATE: no such table"), "{failed}");
    let shown = failed.to_string();
    assert!(shown.starts_with("echo migrating;") && shown.contains("exit code 3") && shown.contains("no such table"), "{shown}");
}

#[test]
fn missing_and_unsafe_copy_paths_are_skipped_without_failing() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &[("README.md", "hi\n")]);
    std::fs::create_dir_all(repo.join("config/local")).unwrap();
    std::fs::write(repo.join("config/local/app.ini"), "debug=1\n").unwrap();
    std::fs::write(tmp.path().join("outside.txt"), "not yours\n").unwrap();
    let wt = new_worktree(tmp.path(), &repo);
    let plan = Prepare {
        copy: vec!["missing/".into(), ".env".into(), "../outside.txt".into(), "/etc/hostname".into(), ".git/config".into(),
                   "config/local/app.ini".into()],
        install: true,
        setup: None,
    };
    let did = prepare::prepare(&repo, &wt, &plan, None, None).unwrap();
    assert_eq!(did.copied, ["config/local/app.ini"], "a nested file gets its folders");
    assert_eq!(did.skipped, ["missing/", ".env", "../outside.txt", "/etc/hostname", ".git/config"]);
    assert!(did.failed.is_empty(), "{:?}", did.failed);
    assert_eq!(std::fs::read_to_string(wt.join("config/local/app.ini")).unwrap(), "debug=1\n");
    assert!(!tmp.path().join("wt/outside.txt").exists() && !wt.join("etc").exists());
}

#[test]
fn a_reused_worktree_installs_again_only_when_its_lock_file_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &PHP_AND_JS);
    let wt = new_worktree(tmp.path(), &repo);
    std::fs::create_dir_all(wt.join("vendor")).unwrap();
    std::fs::create_dir_all(wt.join("node_modules")).unwrap();
    let path = fakes(tmp.path());
    let plan = Prepare { install: true, ..Default::default() };
    let since = rev(&wt, "HEAD");

    // same lock files as the finished card had: nothing to do
    let did = prepare::prepare(&repo, &wt, &plan, Some(&since), Some(&path)).unwrap();
    assert!(did.installed.is_empty() && calls(tmp.path()).is_empty(), "{:?}", calls(tmp.path()));

    // main moved on with a new package-lock.json: npm ci again, composer stays
    std::fs::write(wt.join("package-lock.json"), "{\"lockfileVersion\": 3, \"packages\": {}}\n").unwrap();
    commit_all(&wt, "new lock");
    let did = prepare::prepare(&repo, &wt, &plan, Some(&since), Some(&path)).unwrap();
    assert_eq!(did.installed, ["npm ci"]);
    assert_eq!(calls(tmp.path()), ["npm ci --no-audit --no-fund"]);
}

#[test]
fn copy_defaults_are_suggested_from_the_repository() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path(), &[("composer.json", "{}\n"), ("package.json", "{}\n"), ("Cargo.toml", "[workspace]\n")]);
    std::fs::write(repo.join(".env"), "A=1\n").unwrap();
    assert_eq!(prepare::suggest_copy(&repo), [".env", "vendor/", "node_modules/", "target/"]);
    let plain = tmp.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert!(prepare::suggest_copy(&plain).is_empty());
    std::fs::write(plain.join("Cargo.toml"), "[package]\n").unwrap();
    assert_eq!(prepare::suggest_copy(&plain), ["target/"]);
}
