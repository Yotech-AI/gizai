//! Gizai's own update, end to end without GitHub and without a real install: a scratch folder holds the data, an
//! "installed" Gizai (a stub that says it is this version) in <prefix>/lib/gizai, a local repository with the release
//! tag v9.9.9 whose install.sh is a stub, and a file shaped like GitHub's latest-release answer. Files in control/
//! make the stub fail where a test wants it to.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gizai_core::settings;
use gizai_lib::AppState;
use gizai_lib::runs::Note;
use gizai_lib::update::{self, LastCheck, Source, UpdateStatus, VERSION};

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

/// Writes an executable script through a child sh, so this process never holds it open for writing ("Text file busy").
fn write_script(path: &Path, script: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

/// The release's install.sh: --build-only makes target/release/gizai (a stub that says it is 9.9.9, or what
/// control/built-version says), --skip-build checks the data was backed up first, then installs it into
/// $GIZAI_PREFIX/lib/gizai the way install.sh does (a temporary name, then a rename).
const RELEASE_INSTALL_SH: &str = r#"set -e
T='@T@'; C="$T/control"; M="$T/marks"
case "$1" in
  --build-only)
    echo "nice=$(nice)" > "$M/build"
    [ -f "$C/build-secs" ] && sleep "$(cat "$C/build-secs")"
    if [ -f "$C/fail-build" ]; then echo "error[E0425]: cannot find value (a fake build failure)" >&2; exit 101; fi
    v=9.9.9; [ -f "$C/built-version" ] && v="$(cat "$C/built-version")"
    mkdir -p target/release
    printf '#!/bin/sh\n[ "$1" = --version ] && echo "gizai %s"\nexit 0\n' "$v" > target/release/gizai
    printf '#!/bin/sh\nexit 0\n' > target/release/gizai-mcp
    chmod 755 target/release/gizai target/release/gizai-mcp
    echo "Built gizai $v"
    ;;
  --skip-build)
    ls "$T/data/backups" 2>/dev/null | grep -q '^gizai-before-update-' || { echo "no backup of the data before installing" >&2; exit 9; }
    echo "prefix=$GIZAI_PREFIX" > "$M/install"
    echo "xdg=$XDG_DATA_HOME" >> "$M/install"
    if [ -f "$C/fail-install" ]; then echo "Could not copy Gizai into $GIZAI_PREFIX/lib/gizai, so nothing was installed." >&2; exit 1; fi
    L="$GIZAI_PREFIX/lib/gizai"; mkdir -p "$L"
    cp target/release/gizai "$L/.gizai.new.$$" && mv -f "$L/.gizai.new.$$" "$L/gizai"
    printf '#!/bin/sh\nexec "$(dirname "$0")/gizai" "$@"\n' > "$L/.launch.new.$$" && chmod 755 "$L/.launch.new.$$" && mv -f "$L/.launch.new.$$" "$L/gizai-launch"
    if [ -f "$C/fail-after" ]; then echo "install: cannot create the icons" >&2; exit 1; fi
    echo "Done: gizai"
    ;;
esac
"#;

struct Scratch {
    _tmp: tempfile::TempDir,
    t: PathBuf,
    st: AppState,
    notes: Arc<AtomicUsize>,
    prefix: PathBuf,
}

impl Scratch {
    fn control(&self, name: &str, text: &str) {
        std::fs::write(self.t.join("control").join(name), text).unwrap();
    }
    fn uncontrol(&self, name: &str) {
        let _ = std::fs::remove_file(self.t.join("control").join(name));
    }
    fn installed(&self) -> PathBuf {
        self.prefix.join("lib/gizai/gizai")
    }
    fn installed_says(&self) -> String {
        String::from_utf8(Command::new(self.installed()).arg("--version").output().unwrap().stdout).unwrap().trim().to_string()
    }
    fn backups(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.t.join("data/backups")).map(|d| {
            d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.starts_with("gizai-before-update-")).collect()
        }).unwrap_or_default();
        v.sort();
        v
    }
}

/// A scratch Gizai "installed" in <t>/home/.local, its data in <t>/data, and the release v9.9.9 at <t>/fake-repo
/// (its Cargo.toml says `cargo_version`).
fn scratch(cargo_version: &str) -> Scratch {
    let tmp = tempfile::tempdir().unwrap();
    let t = tmp.path().canonicalize().unwrap();
    for d in ["control", "marks", "fake-repo"] {
        std::fs::create_dir_all(t.join(d)).unwrap();
    }
    let repo = t.join("fake-repo");
    git(&repo, &["init", "-q", "-b", "production"]);
    std::fs::write(repo.join("Cargo.toml"), format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{cargo_version}\"\n")).unwrap();
    std::fs::write(repo.join("install.sh"), RELEASE_INSTALL_SH.replace("@T@", &t.display().to_string())).unwrap();
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "release 9.9.9"]);
    git(&repo, &["tag", "v9.9.9"]);
    std::fs::write(t.join("latest.json"), r#"{"tag_name": "v9.9.9", "name": "v9.9.9", "html_url": "https://github.com/Yotech-AI/gizai/releases/tag/v9.9.9",
        "published_at": "2026-10-07T12:00:00Z", "draft": false, "prerelease": false, "body": "- What changed"}"#).unwrap();

    let prefix = t.join("home/.local");
    write_script(&prefix.join("lib/gizai/gizai"), &format!("#!/bin/sh\n[ \"$1\" = --version ] && echo 'gizai {VERSION}'\nexit 0\n"));
    write_script(&prefix.join("lib/gizai/gizai-launch"), "#!/bin/sh\nexec \"$(dirname \"$0\")/gizai\" \"$@\"\n");

    let notes = Arc::new(AtomicUsize::new(0));
    let counter = notes.clone();
    let st = gizai_lib::open_state(t.join("data"), Arc::new(move |n| {
        if matches!(n, Note::UpdateChanged) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    })).unwrap();
    st.updates.set_source(Source::new(&repo.display().to_string(), Some(format!("file://{}", t.join("latest.json").display()))));
    st.updates.set_exe(Some(prefix.join("lib/gizai/gizai")));
    Scratch { _tmp: tmp, t, st, notes, prefix }
}

/// Waits until the update has ended, and returns how it ended.
fn wait_for_update(st: &AppState) -> UpdateStatus {
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        let s = update::status(st);
        let job = s.job.as_ref().expect("an update");
        if !job.running() {
            return s;
        }
        assert!(Instant::now() < until, "the update is still at {} after 90 s", job.step);
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---------- where releases come from ----------

#[test]
fn releases_come_from_githubs_api_for_a_github_repository() {
    let gh = Source::new("https://github.com/Yotech-AI/gizai.git", None);
    assert_eq!(gh.releases_url.as_deref(), Some("https://api.github.com/repos/Yotech-AI/gizai/releases/latest"));
    let ssh = Source::new("git@github.com:someone/fork.git", None);
    assert_eq!(ssh.releases_url.as_deref(), Some("https://api.github.com/repos/someone/fork/releases/latest"));
    assert_eq!(Source::new("/srv/git/gizai", None).releases_url, None, "not on GitHub, and no URL given");
    let given = Source::new("/srv/git/gizai", Some("file:///tmp/latest.json".into()));
    assert_eq!(given.releases_url.as_deref(), Some("file:///tmp/latest.json"), "GIZAI_RELEASES_URL wins");
}

// ---------- the release check ----------

#[tokio::test]
async fn a_check_finds_a_newer_release_and_offers_it() {
    let s = scratch("9.9.9");
    let before = update::status(&s.st);
    assert_eq!(before.current, VERSION);
    assert!(before.auto_check, "on unless switched off");
    assert_eq!((before.checked_at, before.available.clone(), before.job.clone()), (None, None, None), "nothing checked yet");
    assert_eq!(before.install_to.as_deref(), Some(s.prefix.display().to_string().as_str()));
    assert_eq!(before.cannot_install, None);

    let after = update::check(&s.st).await;
    assert!(!after.checking, "the flag clears when the check ends");
    assert!(after.checked_at.is_some());
    assert_eq!(after.problem, None);
    let r = after.available.clone().expect("9.9.9 is offered");
    assert_eq!((r.version.as_str(), r.tag.as_str(), r.notes.as_deref()), ("9.9.9", "v9.9.9", Some("- What changed")));
    assert_eq!(after.latest, after.available);
    assert_eq!(after.installed, None, "the installed Gizai is still this version");
    assert!(s.notes.load(Ordering::SeqCst) >= 2, "the UI hears when the check starts and ends");
    assert_eq!(update::status(&s.st), after, "status says the same afterwards");
}

#[tokio::test]
async fn what_the_check_found_is_kept_so_the_notice_shows_from_the_start() {
    let s = scratch("9.9.9");
    update::check(&s.st).await;
    // a fresh start: a new Updates, on the same data
    let restarted = gizai_lib::update::Updates::new(Source::new("/nowhere", None), Some(s.installed()));
    let mut st2 = s.st.clone();
    st2.updates = Arc::new(restarted);
    let fresh = update::status(&st2);
    assert_eq!(fresh.available.map(|r| r.version).as_deref(), Some("9.9.9"));
    assert!(fresh.checked_at.is_some());
}

#[tokio::test]
async fn the_same_or_an_older_release_is_not_offered() {
    let s = scratch("9.9.9");
    for (tag, why) in [(format!("v{VERSION}"), "the same version"), ("v0.0.1".to_string(), "an older version")] {
        std::fs::write(s.t.join("latest.json"), format!(r#"{{"tag_name": "{tag}", "draft": false, "prerelease": false}}"#)).unwrap();
        let st = update::check(&s.st).await;
        assert_eq!(st.available, None, "{why}");
        assert_eq!(st.latest.map(|r| r.tag), Some(tag), "{why}: the latest release is still shown");
        assert_eq!(st.problem, None);
    }
}

#[tokio::test]
async fn a_check_that_doesnt_work_says_why_and_keeps_the_release_found_before() {
    let s = scratch("9.9.9");
    update::check(&s.st).await;
    std::fs::remove_file(s.t.join("latest.json")).unwrap();
    let st = update::check(&s.st).await;
    assert!(!st.checking);
    let p = st.problem.clone().expect("why the check didn't work");
    assert!(p.what.contains("Can't read"), "{p:?}");
    assert_eq!(st.available.map(|r| r.version).as_deref(), Some("9.9.9"), "the release found before stays offered");

    // a draft as the latest release is a problem too, and still keeps what was found before
    std::fs::write(s.t.join("latest.json"), r#"{"tag_name": "v10.0.0", "draft": true}"#).unwrap();
    let st = update::check(&s.st).await;
    assert!(st.problem.unwrap().what.contains("draft"));
    assert_eq!(st.available.map(|r| r.version).as_deref(), Some("9.9.9"));

    // once it works again the problem goes
    std::fs::write(s.t.join("latest.json"), r#"{"tag_name": "v9.9.9"}"#).unwrap();
    assert_eq!(update::check(&s.st).await.problem, None);
}

#[tokio::test]
async fn without_a_releases_url_the_check_says_what_to_set() {
    let s = scratch("9.9.9");
    s.st.updates.set_source(Source::new("/srv/git/gizai", None));
    let st = update::check(&s.st).await;
    let p = st.problem.expect("a problem");
    assert!(p.what.contains("doesn't know where"), "{p:?}");
    assert!(p.fix.unwrap().contains("GIZAI_RELEASES_URL"));
}

#[tokio::test]
async fn a_newer_version_installed_meanwhile_only_needs_a_restart() {
    let s = scratch("9.9.9");
    write_script(&s.installed(), "#!/bin/sh\n[ \"$1\" = --version ] && echo 'gizai 9.9.9'\nexit 0\n");
    let st = update::check(&s.st).await;
    assert_eq!(st.installed.as_deref(), Some("9.9.9"), "install.sh put 9.9.9 in place while this Gizai ran");
    assert!(update::restart_command(&s.st).is_ok(), "Restart is offered");
}

// ---------- Check for new releases (on or off) and when it asks ----------

#[tokio::test]
async fn the_automatic_check_can_be_switched_off_and_it_is_kept() {
    let s = scratch("9.9.9");
    assert!(update::auto_check(&s.st));
    let st = update::set_auto_check(&s.st, false).unwrap();
    assert!(!st.auto_check);
    assert!(!update::auto_check(&s.st));
    assert_eq!(settings::get::<bool>(&s.st.db, "update_auto_check").unwrap(), Some(false), "kept in settings");
    assert!(!update::tick(&s.st, gizai_core::ids::now_ms()).await, "switched off: the automatic check doesn't ask");
    assert_eq!(update::last_check(&s.st), None);
    // Check now still works with it off
    assert!(update::check(&s.st).await.available.is_some());
    assert!(update::set_auto_check(&s.st, true).unwrap().auto_check);
}

#[tokio::test]
async fn the_automatic_check_asks_every_six_hours_and_an_hour_after_a_failed_one() {
    let s = scratch("9.9.9");
    let hour = 60 * 60 * 1000_i64;
    let t0 = 1_800_000_000_000_i64;
    assert!(update::tick(&s.st, t0).await, "never checked: due at once");
    let set_last = |at: i64, failed: bool| {
        let last = LastCheck { at, latest: None, problem: failed.then(|| gizai_agents::connection::Problem::plain("no internet")) };
        settings::set(&s.st.db, "update_last_check", &last).unwrap();
    };
    set_last(t0, false);
    assert!(!update::tick(&s.st, t0 + 5 * hour).await, "five hours after a check that worked: not yet");
    assert!(update::tick(&s.st, t0 + 6 * hour).await, "six hours: due");
    set_last(t0, true);
    assert!(!update::tick(&s.st, t0 + hour - 1).await, "just under an hour after a failed check: not yet");
    assert!(update::tick(&s.st, t0 + hour).await, "an hour after a failed check: tried again");
    set_last(t0 + 10 * hour, false);
    assert!(update::tick(&s.st, t0).await, "a last check in the future (the clock went back) doesn't block checks");
    assert_eq!(update::FIRST_CHECK_AFTER, Duration::from_secs(20));
    assert_eq!(update::CHECK_EVERY, Duration::from_secs(6 * 60 * 60));
}

// ---------- Update to 9.9.9 ----------

#[tokio::test]
async fn an_update_builds_in_the_background_backs_up_installs_and_offers_a_restart() {
    let s = scratch("9.9.9");
    s.control("build-secs", "2");
    update::check(&s.st).await;
    let asked = Instant::now();
    let started = update::start(&s.st, "9.9.9").unwrap();
    assert!(asked.elapsed() < Duration::from_secs(1), "start returns at once ({:?}): Gizai stays usable", asked.elapsed());
    let job = started.job.clone().expect("an update runs");
    assert!(job.running() && job.version == "9.9.9" && job.unchanged, "{job:?}");
    // meanwhile the rest of Gizai answers
    assert!(update::status(&s.st).job.unwrap().running());
    assert!(gizai_core::settings::get::<bool>(&s.st.db, "update_auto_check").is_ok());
    // a second press while it runs leaves it to run
    assert!(update::start(&s.st, "9.9.9").unwrap().job.unwrap().running());

    let seen_build = {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let step = update::status(&s.st).job.unwrap().step;
            if step == "build" { break true; }
            if !matches!(step.as_str(), "source") || Instant::now() > until { break false; }
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    assert!(seen_build, "the update shows its build step");

    let done = wait_for_update(&s.st);
    let job = done.job.clone().unwrap();
    assert_eq!(job.step, "installed", "{job:?}");
    assert_eq!(job.problem, None);
    assert!(job.ended_at.is_some());
    assert_eq!(s.installed_says(), "gizai 9.9.9", "9.9.9 is in place");
    assert_eq!(done.installed.as_deref(), Some("9.9.9"));

    // the data was backed up first (the stub's install refuses to run without it), into the data folder's backups/
    let backups = s.backups();
    assert_eq!(backups.len(), 1, "{backups:?}");
    assert_eq!(job.backup.as_deref(), Some(s.t.join("data/backups").join(&backups[0]).display().to_string().as_str()));
    // it built at low priority, and installed into this Gizai's prefix with the desktop entry under it
    let own_nice: i32 = String::from_utf8(Command::new("nice").output().unwrap().stdout).unwrap().trim().parse().unwrap();
    assert_eq!(std::fs::read_to_string(s.t.join("marks/build")).unwrap().trim(), format!("nice={}", (own_nice + 10).min(19)));
    let install = std::fs::read_to_string(s.t.join("marks/install")).unwrap();
    assert!(install.contains(&format!("prefix={}\n", s.prefix.display())) && install.contains(&format!("xdg={}\n", s.prefix.join("share").display())), "{install}");
    // the source and its build stay in the data folder for the next update
    assert!(s.t.join("data/update/source/target/release/gizai").exists());
    let log = std::fs::read_to_string(&job.log).unwrap();
    assert_eq!(Path::new(&job.log), s.t.join("data/update/update.log"));
    for line in ["== Updating Gizai", "$ git fetch --depth 1", "== Building 9.9.9", "$ ./install.sh --build-only", "Built gizai 9.9.9",
                 "== Backing up your data", "== Installing 9.9.9", "$ ./install.sh --skip-build", "== Installed 9.9.9. Restart Gizai to use it."] {
        assert!(log.contains(line), "the log has {line:?}:\n{log}");
    }
    // the restart: a shell that waits for this Gizai to quit, then starts the installed one with its launcher
    let restart = update::restart_command(&s.st).unwrap();
    let args: Vec<String> = restart.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    assert_eq!(restart.get_program(), "sh");
    assert_eq!(args.last().map(String::as_str), Some(s.prefix.join("lib/gizai/gizai-launch").display().to_string().as_str()), "{args:?}");
    assert!(args.contains(&std::process::id().to_string()), "it waits for this process: {args:?}");
}

#[tokio::test]
async fn an_update_is_offered_only_for_the_newer_release_the_check_found() {
    let s = scratch("9.9.9");
    let e = update::start(&s.st, "9.9.9").unwrap_err();
    assert!(e.contains("no newer release 9.9.9"), "not checked yet: {e}");
    update::check(&s.st).await;
    let e = update::start(&s.st, "9.9.8").unwrap_err();
    assert!(e.contains("9.9.8"), "{e}");
    assert!(update::status(&s.st).job.is_none(), "nothing started");
    assert!(update::restart_command(&s.st).is_err(), "nothing to restart into yet");
}

#[tokio::test]
async fn a_gizai_that_isnt_installed_never_installs_anything() {
    let s = scratch("9.9.9");
    let dev = s.t.join("checkout/target/release/gizai");
    s.st.updates.set_exe(Some(dev.clone()));
    let st = update::check(&s.st).await;
    assert!(st.available.is_some(), "it still says a release is out");
    assert_eq!(st.install_to, None);
    let why = st.cannot_install.expect("why it doesn't update itself");
    assert!(why.contains("not from an install") && why.contains(&dev.display().to_string()), "{why}");
    assert!(update::start(&s.st, "9.9.9").unwrap_err().contains("not from an install"));
    assert!(!s.t.join("data/update").exists(), "nothing was fetched or built");
    s.st.updates.set_exe(None);
    assert!(update::status(&s.st).cannot_install.unwrap().contains("can't tell where it runs from"));
}

#[tokio::test]
async fn a_failed_build_keeps_the_installed_gizai_and_says_why_and_try_again_works() {
    let s = scratch("9.9.9");
    s.control("fail-build", "");
    update::check(&s.st).await;
    let before = std::fs::metadata(s.installed()).unwrap().modified().unwrap();
    update::start(&s.st, "9.9.9").unwrap();
    let done = wait_for_update(&s.st);
    let job = done.job.clone().unwrap();
    assert_eq!(job.step, "failed");
    assert!(job.unchanged, "the installed Gizai is as it was");
    assert_eq!(job.problem.as_deref(), Some("./install.sh --build-only failed (exit code 101)"));
    assert!(job.output.as_deref().unwrap_or("").contains("a fake build failure"), "the end of what the build said: {job:?}");
    assert_eq!(s.installed_says(), format!("gizai {VERSION}"));
    assert_eq!(std::fs::metadata(s.installed()).unwrap().modified().unwrap(), before, "not touched");
    assert!(s.backups().is_empty(), "the backup comes after the build");
    assert!(!s.t.join("marks/install").exists(), "nothing was installed");
    assert!(std::fs::read_to_string(&job.log).unwrap().contains("== ./install.sh --build-only failed (exit code 101)"), "the log ends with why");
    assert_eq!(done.available.map(|r| r.version).as_deref(), Some("9.9.9"), "the release is still offered");
    assert_eq!(done.installed, None);
    assert!(update::restart_command(&s.st).is_err(), "no restart after a failed update");

    // Try again, now that the build works
    s.uncontrol("fail-build");
    update::start(&s.st, "9.9.9").unwrap();
    let job = wait_for_update(&s.st).job.unwrap();
    assert_eq!(job.step, "installed", "{job:?}");
    assert_eq!((job.problem, job.output), (None, None), "the old failure is gone");
    assert_eq!(s.installed_says(), "gizai 9.9.9");
}

#[tokio::test]
async fn an_install_that_fails_before_replacing_anything_keeps_the_installed_gizai() {
    let s = scratch("9.9.9");
    s.control("fail-install", "");
    update::check(&s.st).await;
    update::start(&s.st, "9.9.9").unwrap();
    let job = wait_for_update(&s.st).job.unwrap();
    assert_eq!(job.step, "failed");
    assert_eq!(job.problem.as_deref(), Some("./install.sh --skip-build failed (exit code 1)"));
    assert!(job.output.unwrap().contains("nothing was installed"));
    assert!(job.unchanged, "the installed version still says {VERSION}");
    assert_eq!(s.installed_says(), format!("gizai {VERSION}"));
    assert_eq!(s.backups().len(), 1, "the data was backed up before the install was tried");
    assert!(job.backup.is_some());
}

#[tokio::test]
async fn an_installer_that_fails_after_the_new_version_is_in_place_counts_as_installed() {
    let s = scratch("9.9.9");
    s.control("fail-after", "");
    update::check(&s.st).await;
    update::start(&s.st, "9.9.9").unwrap();
    let job = wait_for_update(&s.st).job.unwrap();
    assert_eq!(job.step, "installed", "{job:?}");
    let note = job.problem.expect("what the installer said");
    assert!(note.starts_with("9.9.9 is installed, but the installer said: ./install.sh --skip-build failed"), "{note}");
    assert!(update::restart_command(&s.st).is_ok());
}

#[tokio::test]
async fn an_installed_program_that_says_another_version_is_a_failure() {
    let s = scratch("9.9.9");
    s.control("built-version", "9.9.8");
    update::check(&s.st).await;
    update::start(&s.st, "9.9.9").unwrap();
    let job = wait_for_update(&s.st).job.unwrap();
    assert_eq!(job.step, "failed");
    assert_eq!(job.problem.as_deref(), Some("The installer finished, but the installed Gizai says it is version 9.9.8"));
    assert!(!job.unchanged, "what is installed is no longer the version that runs");
}

#[tokio::test]
async fn a_release_whose_source_says_another_version_is_not_built() {
    let s = scratch("9.9.8");
    update::check(&s.st).await;
    update::start(&s.st, "9.9.9").unwrap();
    let job = wait_for_update(&s.st).job.unwrap();
    assert_eq!(job.step, "failed");
    assert_eq!(job.problem.as_deref(), Some("The source of v9.9.9 says it is version 9.9.8, not 9.9.9, so Gizai doesn't install it"));
    assert!(job.unchanged);
    assert!(!s.t.join("marks/build").exists(), "nothing was built");
    assert_eq!(s.installed_says(), format!("gizai {VERSION}"));
}

#[tokio::test]
async fn a_release_whose_tag_is_gone_says_so() {
    let s = scratch("9.9.9");
    update::check(&s.st).await;
    git(&s.t.join("fake-repo"), &["tag", "-d", "v9.9.9"]);
    update::start(&s.st, "9.9.9").unwrap();
    let job = wait_for_update(&s.st).job.unwrap();
    assert_eq!(job.step, "failed");
    assert!(job.problem.as_deref().unwrap_or("").contains("has no tag v9.9.9"), "{job:?}");
    assert!(job.unchanged);
}

#[tokio::test]
async fn stop_ends_the_build_and_the_installed_gizai_stays() {
    let s = scratch("9.9.9");
    s.control("build-secs", "60");
    update::check(&s.st).await;
    update::start(&s.st, "9.9.9").unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while !s.t.join("marks/build").exists() {
        assert!(Instant::now() < until, "the build never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    let asked = Instant::now();
    update::stop(&s.st);
    let done = wait_for_update(&s.st);
    assert!(asked.elapsed() < Duration::from_secs(15), "it stopped soon: {:?}", asked.elapsed());
    let job = done.job.unwrap();
    assert_eq!(job.step, "stopped", "{job:?}");
    assert_eq!(job.problem, None, "a stop isn't a failure");
    assert!(job.unchanged);
    assert!(s.backups().is_empty() && !s.t.join("marks/install").exists(), "no backup, no install");
    assert_eq!(s.installed_says(), format!("gizai {VERSION}"));
    assert!(std::fs::read_to_string(&job.log).unwrap().contains("== Stopped"));
    assert_eq!(done.available.map(|r| r.version).as_deref(), Some("9.9.9"), "it can be started again");
}

#[tokio::test]
async fn quitting_gizai_stops_an_update_that_builds() {
    let s = scratch("9.9.9");
    s.control("build-secs", "60");
    update::check(&s.st).await;
    update::start(&s.st, "9.9.9").unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while !s.t.join("marks/build").exists() {
        assert!(Instant::now() < until, "the build never started");
        std::thread::sleep(Duration::from_millis(20));
    }
    update::on_exit(&s.st);
    assert_eq!(wait_for_update(&s.st).job.unwrap().step, "stopped");
}
