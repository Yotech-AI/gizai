//! Gizai's own updates, the core: versions, GitHub's latest-release answer, the release check (curl against a file and
//! a local HTTP server, never GitHub), getting a release's source from a local repository, and running its install.sh
//! (a stub) to build and install it, with Stop. Nothing here touches a real install.
// Linux and macOS only: these tests run shell or Python scripts as fake programs, which Windows can't start.
#![cfg(unix)]
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gizai_agents::update::{self as up, Log, Stop, Version};

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", msg]);
}

/// Writes an executable script through a child sh, so this process never holds it open for writing ("Text file busy").
fn write_script(path: &Path, script: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut sh = Command::new("sh").args(["-c", r#"cat > "$1" && chmod 755 "$1""#, "sh"]).arg(path).stdin(Stdio::piped()).spawn().unwrap();
    sh.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
    assert!(sh.wait().unwrap().success(), "can't write {}", path.display());
}

fn github_answer(tag: &str, more: &str) -> String {
    format!(r#"{{"tag_name": "{tag}", "name": "{tag}", "html_url": "https://github.com/Yotech-AI/gizai/releases/tag/{tag}",
                "published_at": "2026-10-07T15:54:54Z", "draft": false, "prerelease": false, "body": "- What changed"{more}}}"#)
}

// ---------- versions ----------

#[test]
fn a_version_is_major_minor_patch_with_or_without_a_v() {
    assert_eq!(Version::parse("0.1.6"), Some(Version(0, 1, 6)));
    assert_eq!(Version::parse("v0.1.6"), Some(Version(0, 1, 6)));
    assert_eq!(Version::parse(" V1.20.300\n"), Some(Version(1, 20, 300)));
    assert_eq!(Version(0, 1, 6).to_string(), "0.1.6");
    for bad in ["", "v", "0.2", "1.2.3.4", "0.2.0-beta.1", "latest", "1..3", "1.2.x", "v 1.2.3", "+1.2.3", "-1.2.3", "1234567890.0.0"] {
        assert_eq!(Version::parse(bad), None, "{bad:?} parsed");
    }
}

#[test]
fn newer_compares_numbers_not_text() {
    assert!(up::is_newer("0.1.10", "0.1.9"), "10 > 9, though \"10\" < \"9\" as text");
    assert!(up::is_newer("0.2.0", "0.1.99"));
    assert!(up::is_newer("1.0.0", "0.99.99"));
    assert!(up::is_newer("v0.1.6", "0.1.5"));
    assert!(!up::is_newer("0.1.5", "0.1.5"), "the same version is not newer");
    assert!(!up::is_newer("0.1.4", "0.1.5"));
    assert!(!up::is_newer("0.2.0-beta.1", "0.1.5"), "a pre-release tag is never newer");
    assert!(!up::is_newer("9.9.9", "nonsense"));
}

// ---------- GitHub's answer ----------

#[test]
fn githubs_latest_release_becomes_a_release() {
    let r = up::parse_release(&github_answer("v0.1.6", "")).unwrap();
    assert_eq!(r.version, "0.1.6");
    assert_eq!(r.tag, "v0.1.6");
    assert_eq!(r.name.as_deref(), Some("v0.1.6"));
    assert_eq!(r.url.as_deref(), Some("https://github.com/Yotech-AI/gizai/releases/tag/v0.1.6"));
    assert_eq!(r.published_at.as_deref(), Some("2026-10-07T15:54:54Z"));
    assert_eq!(r.notes.as_deref(), Some("- What changed"));
    // fields GitHub sends that Gizai doesn't need are fine, and missing optional ones too
    let bare = up::parse_release(r#"{"tag_name": "v1.0.0", "assets": [], "author": {"login": "x"}}"#).unwrap();
    assert_eq!((bare.version.as_str(), bare.name, bare.url, bare.notes), ("1.0.0", None, None, None));
}

#[test]
fn a_draft_a_pre_release_or_an_odd_tag_is_no_release_to_update_to() {
    let draft = up::parse_release(&github_answer("v0.1.6", "").replace(r#""draft": false"#, r#""draft": true"#)).unwrap_err();
    assert!(draft.what.contains("draft"), "{draft:?}");
    let pre = up::parse_release(&github_answer("v0.2.0", "").replace(r#""prerelease": false"#, r#""prerelease": true"#)).unwrap_err();
    assert!(pre.what.contains("pre-release"), "{pre:?}");
    for tag in ["nightly", "v0.2.0-beta.1", "0.2"] {
        let e = up::parse_release(&github_answer(tag, "")).unwrap_err();
        assert!(e.what.contains(tag) && e.what.contains("isn't a version"), "{tag}: {e:?}");
    }
    let junk = up::parse_release("<html>Bad gateway</html>").unwrap_err();
    assert!(junk.what.contains("can't read"), "{junk:?}");
}

#[test]
fn a_release_keeps_its_tag_as_github_gives_it_and_trims_what_is_blank() {
    let r = up::parse_release(r#"{"tag_name": " v0.1.6 ", "name": "  ", "body": "  \n "}"#).unwrap();
    assert_eq!(r.version, "0.1.6");
    assert_eq!(r.tag, "v0.1.6", "trimmed, so the fetch asks for the real tag");
    assert_eq!(r.name, None, "a blank title is no title");
    assert_eq!(r.notes, None, "blank notes are no notes");
    let upper = up::parse_release(r#"{"tag_name": "V0.1.6"}"#).unwrap();
    assert_eq!((upper.version.as_str(), upper.tag.as_str()), ("0.1.6", "V0.1.6"), "the tag stays as it is on GitHub");
}

#[test]
fn long_release_notes_are_cut() {
    let body = "é".repeat(25_000);
    let r = up::parse_release(&format!(r#"{{"tag_name": "v0.1.6", "body": "{body}"}}"#)).unwrap();
    let notes = r.notes.unwrap();
    assert_eq!(notes.chars().count(), 20_001, "20,000 characters and an ellipsis (counted in characters, not bytes)");
    assert!(notes.ends_with('…'));
    let short = up::parse_release(&format!(r#"{{"tag_name": "v0.1.6", "body": "{}"}}"#, "a".repeat(20_000))).unwrap();
    assert_eq!(short.notes.unwrap().len(), 20_000, "exactly 20,000 is kept whole");
}

#[test]
fn the_check_asks_githubs_latest_release_api() {
    assert_eq!(up::latest_release_url("Yotech-AI", "gizai"), "https://api.github.com/repos/Yotech-AI/gizai/releases/latest");
}

// ---------- the release check (curl) ----------

#[test]
fn the_check_reads_a_fake_release_from_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("latest.json");
    std::fs::write(&file, github_answer("v9.9.9", "")).unwrap();
    let r = up::latest_release(&format!("file://{}", file.display()), "gizai/test", Duration::from_secs(10)).unwrap().unwrap();
    assert_eq!((r.version.as_str(), r.tag.as_str()), ("9.9.9", "v9.9.9"));

    let gone = up::latest_release(&format!("file://{}/missing.json", tmp.path().display()), "gizai/test", Duration::from_secs(10)).unwrap_err();
    assert!(gone.what.contains("Can't read") && gone.what.contains("missing.json"), "{gone:?}");
}

/// A one-request HTTP server on localhost that answers `status` with `body`, and hands back the request it got.
fn serve_once(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/repos/Yotech-AI/gizai/releases/latest", listener.local_addr().unwrap().port());
    let (status, body) = (status.to_string(), body.to_string());
    let server = std::thread::spawn(move || {
        let (mut conn, _) = listener.accept().unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut req = Vec::new();
        let mut buf = [0u8; 4096];
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = conn.read(&mut buf).unwrap();
            if n == 0 { break; }
            req.extend_from_slice(&buf[..n]);
        }
        let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        conn.write_all(reply.as_bytes()).unwrap();
        String::from_utf8_lossy(&req).to_string()
    });
    (url, server)
}

#[test]
fn the_check_reads_githubs_answer_over_http_and_sends_nothing_about_you() {
    let (url, server) = serve_once("200 OK", &github_answer("v0.1.6", ""));
    let r = up::latest_release(&url, "gizai/0.1.5", Duration::from_secs(10)).unwrap().unwrap();
    assert_eq!(r.version, "0.1.6");
    let req = server.join().unwrap();
    assert!(req.starts_with("GET /repos/Yotech-AI/gizai/releases/latest HTTP/1.1"), "{req}");
    assert!(req.contains("User-Agent: gizai/0.1.5"), "{req}");
    assert!(req.contains("Accept: application/vnd.github+json"), "{req}");
    assert!(!req.to_ascii_lowercase().contains("authorization"), "no login is sent: {req}");
    assert!(!req.to_ascii_lowercase().contains("cookie"), "{req}");
}

#[test]
fn no_release_yet_is_not_a_problem() {
    let (url, server) = serve_once("404 Not Found", r#"{"message": "Not Found"}"#);
    assert_eq!(up::latest_release(&url, "gizai/test", Duration::from_secs(10)).unwrap(), None);
    server.join().unwrap();
}

#[test]
fn githubs_rate_limit_and_errors_are_said_plainly() {
    let (url, server) = serve_once("403 Forbidden", r#"{"message": "API rate limit exceeded for 1.2.3.4."}"#);
    let e = up::latest_release(&url, "gizai/test", Duration::from_secs(10)).unwrap_err();
    assert!(e.what.contains("too many requests") && e.fix.as_deref() == Some("Try again later."), "{e:?}");
    server.join().unwrap();

    let (url, server) = serve_once("500 Internal Server Error", "oops");
    let e = up::latest_release(&url, "gizai/test", Duration::from_secs(10)).unwrap_err();
    assert!(e.what.contains("HTTP 500"), "{e:?}");
    server.join().unwrap();
}

#[test]
fn a_server_that_isnt_there_means_github_cant_be_reached() {
    // a port nobody listens on: bind, note it, close
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let e = up::latest_release(&format!("http://127.0.0.1:{port}/x"), "gizai/test", Duration::from_secs(10)).unwrap_err();
    assert!(e.what.contains("Can't reach GitHub"), "{e:?}");
    assert!(e.fix.is_some(), "it says what to do: {e:?}");
}

// ---------- the install it updates ----------

#[test]
fn only_a_gizai_in_prefix_lib_gizai_has_an_install_to_update() {
    assert_eq!(up::install_prefix(Path::new("/home/x/.local/lib/gizai/gizai")), Some(PathBuf::from("/home/x/.local")));
    assert_eq!(up::install_prefix(Path::new("/tmp/t/home/.local/lib/gizai/gizai")), Some(PathBuf::from("/tmp/t/home/.local")));
    for not in ["/repo/target/release/gizai", "/repo/target/debug/gizai", "/home/x/.local/lib/gizai/gizai-mcp",
                "/home/x/.local/lib64/gizai/gizai", "/home/x/.local/lib/other/gizai", "/usr/bin/gizai", "gizai"] {
        assert_eq!(up::install_prefix(Path::new(not)), None, "{not}");
    }
}

#[test]
fn the_installed_version_is_what_gizai_version_says() {
    let tmp = tempfile::tempdir().unwrap();
    let prefix = tmp.path().join("prefix");
    assert_eq!(up::installed_version(&prefix), None, "nothing installed");
    write_script(&prefix.join("lib/gizai/gizai"), "#!/bin/sh\n[ \"$1\" = --version ] && echo 'gizai 9.9.9'\n");
    assert_eq!(up::installed_version(&prefix).as_deref(), Some("9.9.9"));
    write_script(&prefix.join("lib/gizai/gizai"), "#!/bin/sh\necho 'gizai dev-build'\n");
    assert_eq!(up::installed_version(&prefix), None, "no version in its answer");
    write_script(&prefix.join("lib/gizai/gizai"), "#!/bin/sh\necho 'gizai 9.9.9'\nexit 1\n");
    assert_eq!(up::installed_version(&prefix), None, "a program that fails doesn't count");
}

#[test]
fn a_sources_version_is_its_workspace_package_version() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(up::source_version(tmp.path()), None, "no Cargo.toml");
    std::fs::write(tmp.path().join("Cargo.toml"), r#"[workspace]
members = ["crates/*"]

[workspace.dependencies]
serde = { version = "1" }

[workspace.package]
edition = "2024"
version = "0.1.6"
license = "MIT"

[package]
version = "7.7.7"
"#).unwrap();
    assert_eq!(up::source_version(tmp.path()).as_deref(), Some("0.1.6"));
    std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nversion = \"7.7.7\"\n").unwrap();
    assert_eq!(up::source_version(tmp.path()), None, "only [workspace.package] counts");
}

#[test]
fn this_repositorys_cargo_toml_gives_its_version() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(up::source_version(&root).as_deref(), Some(env!("CARGO_PKG_VERSION")));
}

// ---------- the release's source ----------

/// A repository standing in for GitHub, with tags v1.0.0 and v1.0.1 (different files in each).
fn release_repo(tmp: &Path) -> PathBuf {
    let repo = tmp.join("releases");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "production"]);
    std::fs::write(repo.join("Cargo.toml"), "[workspace.package]\nversion = \"1.0.0\"\n").unwrap();
    std::fs::write(repo.join("old.txt"), "only in 1.0.0").unwrap();
    commit_all(&repo, "1.0.0");
    git(&repo, &["tag", "v1.0.0"]);
    std::fs::write(repo.join("Cargo.toml"), "[workspace.package]\nversion = \"1.0.1\"\n").unwrap();
    std::fs::remove_file(repo.join("old.txt")).unwrap();
    std::fs::write(repo.join("new.txt"), "only in 1.0.1").unwrap();
    commit_all(&repo, "1.0.1");
    git(&repo, &["tag", "v1.0.1"]);
    // main has moved on past the release: the update takes the tag, not a branch
    git(&repo, &["checkout", "-q", "-b", "main"]);
    std::fs::write(repo.join("Cargo.toml"), "[workspace.package]\nversion = \"1.1.0-dev\"\n").unwrap();
    commit_all(&repo, "work in progress");
    repo
}

#[test]
fn the_source_is_that_tag_alone_and_the_next_one_keeps_the_build() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = release_repo(tmp.path());
    let dir = tmp.path().join("data/update/source");
    let log = Log::create(&tmp.path().join("data/update/update.log")).unwrap();
    let repo_url = repo.display().to_string();

    up::get_source(&repo_url, "v1.0.0", &dir, &log, &Stop::default()).unwrap();
    assert_eq!(up::source_version(&dir).as_deref(), Some("1.0.0"));
    assert!(dir.join("old.txt").exists() && !dir.join("new.txt").exists());
    assert_eq!(git(&dir, &["rev-list", "--count", "HEAD"]), "1", "a shallow fetch: one commit");

    // a build left target/, and something stray is lying around
    std::fs::create_dir_all(dir.join("target/release")).unwrap();
    std::fs::write(dir.join("target/release/cached"), "x").unwrap();
    std::fs::write(dir.join("stray.txt"), "x").unwrap();
    std::fs::write(dir.join("Cargo.toml"), "edited").unwrap();

    up::get_source(&repo_url, "v1.0.1", &dir, &log, &Stop::default()).unwrap();
    assert_eq!(up::source_version(&dir).as_deref(), Some("1.0.1"), "local edits are gone");
    assert!(dir.join("new.txt").exists() && !dir.join("old.txt").exists());
    assert!(!dir.join("stray.txt").exists(), "everything else goes");
    assert!(dir.join("target/release/cached").exists(), "the build's target/ stays, so the next build is quicker");
    let said = std::fs::read_to_string(tmp.path().join("data/update/update.log")).unwrap();
    assert!(said.contains("$ git fetch --depth 1") && said.contains("refs/tags/v1.0.1"), "{said}");
}

#[test]
fn a_tag_that_isnt_there_says_the_release_may_be_gone() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = release_repo(tmp.path());
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    let e = up::get_source(&repo.display().to_string(), "v9.9.9", &tmp.path().join("source"), &log, &Stop::default()).unwrap_err();
    assert!(e.what.contains("has no tag v9.9.9"), "{e:?}");
    assert!(!e.output.is_empty(), "with what git said");

    let missing = tmp.path().join("no-such-repo");
    let e = up::get_source(&missing.display().to_string(), "v1.0.0", &tmp.path().join("source"), &log, &Stop::default()).unwrap_err();
    assert!(e.what.contains("can't find its repository"), "{e:?}");
}

#[test]
fn git_never_works_in_a_checkout_around_the_data_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = release_repo(tmp.path());
    // the data folder sits inside someone's checkout, and the source folder's own .git was cut short
    let outer = tmp.path().join("outer");
    std::fs::create_dir_all(&outer).unwrap();
    git(&outer, &["init", "-q", "-b", "main"]);
    std::fs::write(outer.join("mine.txt"), "mine").unwrap();
    commit_all(&outer, "outer");
    let head = git(&outer, &["rev-parse", "HEAD"]);
    let dir = outer.join("data/update/source");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".git/HEAD"), "garbage").unwrap();

    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    up::get_source(&repo.display().to_string(), "v1.0.1", &dir, &log, &Stop::default()).unwrap();
    assert_eq!(up::source_version(&dir).as_deref(), Some("1.0.1"));
    assert_eq!(git(&dir, &["rev-parse", "--show-toplevel"]), dir.canonicalize().unwrap().display().to_string(), "a repository of its own");
    assert_eq!(git(&outer, &["rev-parse", "HEAD"]), head, "the checkout around it is untouched");
    assert_eq!(git(&outer, &["tag", "--list"]), "", "no tag was fetched into it");
    assert!(outer.join("mine.txt").exists());
}

// ---------- building and installing with the release's install.sh ----------

/// A release source with a stub install.sh that notes how it ran in `marks`.
fn stub_source(tmp: &Path, script: &str) -> (PathBuf, PathBuf) {
    let dir = tmp.join("source");
    let marks = tmp.join("marks");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&marks).unwrap();
    std::fs::write(dir.join("install.sh"), script.replace("$MARKS", &marks.display().to_string())).unwrap();
    (dir, marks)
}

const STUB: &str = r#"set -e
echo "args=$*" > "$MARKS/args"
echo "nice=$(nice)" >> "$MARKS/args"
echo "pwd=$(pwd)" >> "$MARKS/args"
echo "target=$CARGO_TARGET_DIR" >> "$MARKS/args"
echo "prefix=${GIZAI_PREFIX:-}" >> "$MARKS/args"
echo "xdg=${XDG_DATA_HOME:-}" >> "$MARKS/args"
echo "data=${GIZAI_DATA_DIR:-}" >> "$MARKS/args"
echo "prompt=${GIT_TERMINAL_PROMPT:-}" >> "$MARKS/args"
echo "building or installing"
"#;

#[test]
fn the_build_runs_the_releases_install_sh_build_only_at_low_priority() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, marks) = stub_source(tmp.path(), STUB);
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    up::build(&dir, None, &log, &Stop::default()).unwrap();
    let args = std::fs::read_to_string(marks.join("args")).unwrap();
    assert!(args.contains("args=--build-only\n"), "{args}");
    let own_nice: i32 = String::from_utf8(Command::new("nice").output().unwrap().stdout).unwrap().trim().parse().unwrap();
    let nice: i32 = args.lines().find_map(|l| l.strip_prefix("nice=")).unwrap().parse().unwrap();
    assert_eq!(nice, (own_nice + 10).min(19), "nice 10 above Gizai: {args}");
    assert!(args.contains(&format!("pwd={}\n", dir.display())), "runs in the source: {args}");
    assert!(args.contains(&format!("target={}\n", dir.join("target").display())), "builds into the source's own target/: {args}");
    assert!(args.contains("prompt=0\n"), "never prompts: {args}");
    let said = std::fs::read_to_string(tmp.path().join("update.log")).unwrap();
    assert!(said.contains("$ ./install.sh --build-only") && said.contains("building or installing"), "its output is in the log: {said}");
}

#[test]
fn the_install_goes_into_the_prefix_with_its_desktop_entry_under_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, marks) = stub_source(tmp.path(), STUB);
    let prefix = tmp.path().join("home/.local");
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    up::install(&dir, &prefix, None, &log, &Stop::default()).unwrap();
    let args = std::fs::read_to_string(marks.join("args")).unwrap();
    assert!(args.contains("args=--skip-build\n"), "{args}");
    assert!(args.contains(&format!("prefix={}\n", prefix.display())), "{args}");
    assert!(args.contains(&format!("xdg={}\n", prefix.join("share").display())), "the desktop entry and icons go under the prefix: {args}");
    let own_nice: i32 = String::from_utf8(Command::new("nice").output().unwrap().stdout).unwrap().trim().parse().unwrap();
    assert!(args.contains(&format!("nice={own_nice}\n")), "the install runs at Gizai's own priority: {args}");
}

#[test]
fn a_failed_build_says_so_with_the_end_of_its_output() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, _) = stub_source(tmp.path(), "echo 'Compiling gizai v9.9.9'\necho 'error: linker `cc` not found' >&2\nexit 3\n");
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    let e = up::build(&dir, None, &log, &Stop::default()).unwrap_err();
    assert_eq!(e.what, "./install.sh --build-only failed (exit code 3)");
    assert!(e.output.contains("linker `cc` not found") && e.output.contains("Compiling gizai"), "stdout and stderr: {e:?}");
    assert!(e.to_string().starts_with("./install.sh --build-only failed (exit code 3):\n"), "{e}");

    std::fs::remove_file(dir.join("install.sh")).unwrap();
    let e = up::build(&dir, None, &log, &Stop::default()).unwrap_err();
    assert!(e.what.contains("no install.sh"), "{e:?}");
}

#[test]
fn the_build_gets_the_path_it_is_given() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, marks) = stub_source(tmp.path(), r#"echo "path=$PATH" > "$MARKS/path""#);
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    let path = format!("{}:/usr/bin:/bin", tmp.path().join("bin").display());
    up::build(&dir, Some(std::ffi::OsStr::new(&path)), &log, &Stop::default()).unwrap();
    assert_eq!(std::fs::read_to_string(marks.join("path")).unwrap().trim(), format!("path={path}"));
}

fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(s) => s.rsplit_once(") ").is_some_and(|(_, rest)| rest.starts_with('Z') || rest.starts_with('X')),
    }
}

#[test]
fn stop_ends_the_build_with_everything_it_started() {
    let tmp = tempfile::tempdir().unwrap();
    // the build starts a child of its own (a compiler, say) and waits for it
    let (dir, marks) = stub_source(tmp.path(), "sleep 120 &\necho $! > \"$MARKS/child\"\nwait\n");
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    let stop = Stop::default();
    let stopper = {
        let (stop, marks) = (stop.clone(), marks.clone());
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while !marks.join("child").exists() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(100));
            stop.stop();
        })
    };
    let started = Instant::now();
    let e = up::build(&dir, None, &log, &stop).unwrap_err();
    stopper.join().unwrap();
    // Stop signals the group itself, so the command may end before run() notices the stop: then it says the build
    // "was ended by a signal". The app goes by stop.asked() and shows "stopped" either way.
    assert!(e.what == "Stopped" || e.what == "./install.sh --build-only was ended by a signal", "{e:?}");
    assert!(started.elapsed() < Duration::from_secs(20), "it stopped soon: {:?}", started.elapsed());
    assert!(stop.asked());
    let child: u32 = std::fs::read_to_string(marks.join("child")).unwrap().trim().parse().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !gone(child) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(gone(child), "the build's own child {child} ended too");
}

#[test]
fn after_stop_no_other_command_starts() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, marks) = stub_source(tmp.path(), STUB);
    let log = Log::create(&tmp.path().join("update.log")).unwrap();
    let stop = Stop::default();
    stop.stop();
    assert_eq!(up::build(&dir, None, &log, &stop).unwrap_err().what, "Stopped");
    assert!(!marks.join("args").exists(), "the installer never ran");
    let repo = release_repo(tmp.path());
    assert_eq!(up::get_source(&repo.display().to_string(), "v1.0.0", &tmp.path().join("src2"), &log, &stop).unwrap_err().what, "Stopped");
}
