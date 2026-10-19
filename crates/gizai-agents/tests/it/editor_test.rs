// GA-107: what is inside a folder open in the Editor. The path gate (with Linux's rules and with Windows' rules, on any
// system), reading and saving (atomic, CRLF, byte order mark, "changed on disk"), what is read-only and why, new, rename
// and delete, Go to file, search with git grep and its limits, git status, the files' versions, and Open in VS Code, Zed
// or PhpStorm (found under each of their names, started at a line, `.idea/` kept out of git status).
// Links, Unix permissions and fake editors (shell scripts) are tested on Linux and macOS only.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use gizai_agents::editor::{self, AppId, EditorApp, Find, Folder, GitMark};

const WORKING: &str = "Backend Agent is working on GA-12";

/// A temp folder by its real path (on macOS /var/folders is /private/var/folders), without Windows' \\?\.
fn real_tempdir() -> tempfile::TempDir {
    let base = std::env::temp_dir().canonicalize().unwrap();
    let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\").to_string());
    tempfile::tempdir_in(base).unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "core.autocrlf=false"]).args(args)
        .current_dir(dir).output().unwrap();
    assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
}

/// A git repository at `tmp/name` on branch main, with `files` committed.
fn repo(tmp: &Path, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = tmp.join(name);
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    for (path, text) in files {
        put(&dir, path, text.as_bytes());
    }
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    dir
}

fn put(dir: &Path, path: &str, bytes: &[u8]) {
    let p = dir.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}

fn open(dir: &Path) -> Folder<'_> {
    Folder { dir, read_only: None }
}

fn busy(dir: &Path) -> Folder<'_> {
    Folder { dir, read_only: Some(WORKING) }
}

fn err<T: std::fmt::Debug>(r: Result<T, String>) -> String {
    r.expect_err("refused")
}

/// A refusal reads as a plain sentence: a capital letter first, no Rust or OS error codes.
fn sentence(why: &str) {
    assert!(why.chars().next().is_some_and(|c| c.is_uppercase() || c == '.'), "{why}");
    assert!(!why.contains("Err(") && !why.contains("os error") && !why.contains("Some("), "{why}");
}

// ---- the path gate ----

#[test]
fn the_path_gate_refuses_a_full_path_dot_dot_any_git_part_and_a_control_character_with_a_plain_sentence() {
    for windows in [false, true] {
        for p in ["/etc/passwd", "/", "//server/share"] {
            let why = err(editor::split_path(p, windows));
            assert_eq!(why, "Give a path inside the folder, like src/main.rs, not a full path", "{p} (windows: {windows})");
        }
        for p in ["..", "../x", "a/../../x", "a/..", "src/../../etc/passwd"] {
            let why = err(editor::split_path(p, windows));
            assert_eq!(why, "A path can't go up out of the folder (..)", "{p} (windows: {windows})");
        }
        for p in [".git", ".git/config", "src/.GIT/x", ".Git", ".git.", "a/.git./b", ".git ", "x/.gIt", "./.git/HEAD"] {
            let why = err(editor::split_path(p, windows));
            assert_eq!(why, "The Editor never opens or changes git's own files (.git)", "{p} (windows: {windows})");
        }
        for p in ["a\nb", "a\tb", "a\u{7f}b", "a\rb", "a\0b"] {
            let why = err(editor::split_path(p, windows));
            assert!(why.contains("control character"), "{p:?}: {why}");
            sentence(&why);
        }
        // the folder itself, and what is fine
        assert_eq!(editor::split_path("", windows).unwrap(), Vec::<&str>::new());
        assert_eq!(editor::split_path(".", windows).unwrap(), Vec::<&str>::new());
        assert_eq!(editor::split_path("./src//main.rs", windows).unwrap(), ["src", "main.rs"]);
        for p in [".gitignore", ".github/workflows/ci.yml", "a..b", "src/.gitkeep", "git/x", ".gitattributes", "my.git/x"] {
            assert!(editor::split_path(p, windows).is_ok(), "{p} (windows: {windows})");
        }
    }
}

#[test]
fn windows_paths_are_checked_with_windows_rules_on_any_system() {
    let full = "Give a path inside the folder, like src/main.rs, not a full path";
    // a:b is b on drive A:
    for p in [r"C:\Windows\win.ini", "c:/x", "C:", "z:foo", "a:b", r"\x", r"\\server\share\x", r"\\?\C:\x"] {
        assert_eq!(err(editor::split_path(p, true)), full, "{p}");
    }
    for p in [r"a\..\b", r"..\x", ".. ", ". .", "...", r"a\. \b", "a/.. /b"] {
        assert_eq!(err(editor::split_path(p, true)), "A path can't go up out of the folder (..)", "{p}");
    }
    for p in [r".git\config", r"src\.GIT\x", r"a\.git.\b", r"a\.git \b"] {
        assert_eq!(err(editor::split_path(p, true)), "The Editor never opens or changes git's own files (.git)", "{p}");
    }
    for p in ["file.txt:stream", "ab:c", r"src\x:$DATA"] {
        let why = err(editor::split_path(p, true));
        assert!(why.starts_with("A path can't have a : in it on Windows"), "{p}: {why}");
    }
    for p in ["CON", "nul.txt", "Com1", "LPT9.log", "aux", r"src\prn.md", "con .txt", "NUL.tar.gz"] {
        let why = err(editor::split_path(p, true));
        assert!(why.ends_with("is the name of a device on Windows, not of a file"), "{p}: {why}");
    }
    // what Windows allows
    assert_eq!(editor::split_path(r"src\main.rs", true).unwrap(), ["src", "main.rs"]);
    for p in ["COM0", "LPT10", "CONSOLE", "console.log", "a/nulx", "auxiliary.md", "com1x"] {
        assert!(editor::split_path(p, true).is_ok(), "{p}");
    }
    // with Linux's rules a backslash, a : and a device name are only characters of a name
    assert_eq!(editor::split_path(r"a\b", false).unwrap(), [r"a\b"]);
    for p in ["C:x", "a:b", "CON", "nul.txt", "...", ".. "] {
        assert!(editor::split_path(p, false).is_ok(), "{p}");
    }
}

#[test]
fn every_command_refuses_a_path_out_of_the_folder_and_into_git_and_changes_nothing() {
    let tmp = real_tempdir();
    let outside = tmp.path().join("outside.txt");
    fs::write(&outside, "secret\n").unwrap();
    let dir = repo(tmp.path(), "shop", &[("a.txt", "one\n")]);
    let f = open(&dir);
    let v = editor::read(f, "a.txt").unwrap().version;
    for p in ["../outside.txt", "/etc/passwd", ".git/config", "src/../../outside.txt"] {
        for why in [
            err(editor::read(f, p)),
            err(editor::write(f, p, "x", &v)),
            err(editor::write(f, p, "x", "")),
            err(editor::create(f, p, false)),
            err(editor::create(f, p, true)),
            err(editor::rename(f, "a.txt", p)),
            err(editor::rename(f, p, "b.txt")),
            err(editor::delete(f, p)),
            err(editor::list(&dir, p)),
        ] {
            sentence(&why);
        }
    }
    assert_eq!(fs::read_to_string(&outside).unwrap(), "secret\n");
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
    assert!(dir.join(".git/config").is_file() && dir.join(".git/HEAD").is_file());
    assert_eq!(editor::versions(&dir, &["../outside.txt".into(), ".git/HEAD".into()]), ["", ""]);
    // .git never shows in a folder level
    assert!(editor::list(&dir, "").unwrap().iter().all(|e| e.name != ".git"));
}

#[cfg(unix)]
#[test]
fn the_path_gate_follows_links_inside_the_folder_and_refuses_those_that_lead_out_into_git_or_to_nothing() {
    use std::os::unix::fs::symlink;
    let tmp = real_tempdir();
    let out = tmp.path().join("outside");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("secret.txt"), "secret\n").unwrap();
    let dir = repo(tmp.path(), "shop", &[("src/main.rs", "fn main() {}\n")]);
    symlink(out.join("secret.txt"), dir.join("out")).unwrap();
    symlink(&out, dir.join("outdir")).unwrap();
    symlink("../outside/secret.txt", dir.join("relout")).unwrap();
    symlink(dir.join(".git/config"), dir.join("into_git")).unwrap();
    symlink(".git", dir.join("gitdir")).unwrap();
    symlink(tmp.path().join("nothing"), dir.join("dangling")).unwrap();
    symlink("src/main.rs", dir.join("inner")).unwrap();
    symlink("src", dir.join("srclink")).unwrap();

    let leads_out = |p: &str| {
        let why = err(editor::gate(&dir, p));
        assert_eq!(why, format!("{p} leads out of the folder through a link"));
    };
    leads_out("out");
    leads_out("relout");
    leads_out("outdir/secret.txt");
    leads_out("outdir/new.txt");
    leads_out("outdir/deeper/new.txt");
    for p in ["into_git", "gitdir/config", "gitdir"] {
        assert_eq!(err(editor::gate(&dir, p)), "The Editor never opens or changes git's own files (.git)", "{p}");
    }
    let why = err(editor::gate(&dir, "dangling"));
    assert_eq!(why, "dangling goes through a link to something that isn't there");
    // links that stay inside work
    let main = dir.join("src/main.rs").canonicalize().unwrap();
    assert_eq!(editor::gate(&dir, "inner").unwrap(), main);
    assert_eq!(editor::gate(&dir, "srclink/main.rs").unwrap(), main);
    assert_eq!(editor::read(open(&dir), "srclink/main.rs").unwrap().text.as_deref(), Some("fn main() {}\n"));
    assert_eq!(editor::list(&dir, "srclink").unwrap().iter().map(|e| e.path.as_str()).collect::<Vec<_>>(), ["srclink/main.rs"]);

    // nothing out of the folder is read, saved, made, moved or listed through a link
    let f = open(&dir);
    assert!(err(editor::read(f, "out")).contains("leads out of the folder"));
    assert!(err(editor::read(f, "outdir/secret.txt")).contains("leads out of the folder"));
    assert!(err(editor::write(f, "out", "changed\n", "")).contains("leads out of the folder"));
    assert!(err(editor::write(f, "outdir/new.txt", "x", "")).contains("leads out of the folder"));
    assert!(err(editor::write(f, "dangling", "x", "")).contains("isn't there"));
    assert!(err(editor::create(f, "outdir/new", true)).contains("leads out of the folder"));
    assert!(err(editor::rename(f, "src/main.rs", "outdir/main.rs")).contains("leads out of the folder"));
    assert!(err(editor::delete(f, "outdir/secret.txt")).contains("leads out of the folder"));
    assert!(err(editor::list(&dir, "outdir")).contains("leads out of the folder"));
    assert!(err(editor::read(f, "into_git")).contains(".git"));
    assert!(err(editor::write(f, "gitdir/config", "x", "")).contains(".git"));
    assert_eq!(fs::read_dir(&out).unwrap().count(), 1, "nothing new outside");
    assert_eq!(fs::read_to_string(out.join("secret.txt")).unwrap(), "secret\n");
    assert!(!tmp.path().join("nothing").exists(), "a dangling link's target isn't made");
    assert_eq!(editor::versions(&dir, &["out".into(), "inner".into(), "dangling".into()])[..2], ["".to_string(),
        editor::read(f, "src/main.rs").unwrap().version]);

    // a link goes as the link: deleting or renaming it leaves what it leads to alone
    editor::delete(f, "outdir").unwrap();
    assert!(fs::symlink_metadata(dir.join("outdir")).is_err());
    assert_eq!(fs::read_to_string(out.join("secret.txt")).unwrap(), "secret\n");
    editor::rename(f, "out", "moved-link").unwrap();
    assert!(fs::symlink_metadata(dir.join("moved-link")).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_to_string(out.join("secret.txt")).unwrap(), "secret\n");
}

// ---- reading ----

#[test]
fn read_gives_the_text_its_line_endings_and_byte_order_mark_and_a_version() {
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "plain.txt", b"a\nb\n");
    put(dir, "crlf.txt", b"\xEF\xBB\xBFone\r\ntwo\r\n");
    let f = open(dir);
    let plain = editor::read(f, "plain.txt").unwrap();
    assert_eq!((plain.path.as_str(), plain.text.as_deref(), plain.crlf, plain.bom), ("plain.txt", Some("a\nb\n"), false, false));
    assert_eq!((plain.binary, plain.too_large, plain.read_only, plain.read_only_why.as_deref()), (false, false, false, None));
    assert!(!plain.version.is_empty());
    assert_eq!(editor::versions(dir, &["plain.txt".into()]), [plain.version.clone()]);
    let crlf = editor::read(f, "crlf.txt").unwrap();
    assert_eq!((crlf.text.as_deref(), crlf.crlf, crlf.bom, crlf.read_only), (Some("one\ntwo\n"), true, true, false));
    assert_ne!(crlf.version, plain.version);
    // what isn't a file to open
    fs::create_dir(dir.join("src")).unwrap();
    assert_eq!(err(editor::read(f, "src")), "src is a folder");
    assert_eq!(err(editor::read(f, "")), "Pick a file to open");
    assert_eq!(err(editor::read(f, "gone.txt")), "gone.txt isn't there any more");
}

#[test]
fn binaries_and_files_over_5_mb_come_without_text_and_files_over_1_mb_are_read_only() {
    let tmp = real_tempdir();
    let dir = tmp.path();
    const MB: usize = 1024 * 1024;
    assert_eq!((editor::READ_ONLY_OVER, editor::TEXT_UP_TO), (MB as u64, 5 * MB as u64));
    put(dir, "logo.png", b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    put(dir, "one_mb.txt", &vec![b'a'; MB]);
    put(dir, "over_one_mb.txt", &vec![b'a'; MB + 1]);
    put(dir, "five_mb.txt", &vec![b'a'; 5 * MB]);
    put(dir, "over_five_mb.txt", &vec![b'a'; 5 * MB + 1]);
    let mut late_nul = vec![b'a'; 6 * MB];
    late_nul[100] = 0;
    put(dir, "big.bin", &late_nul);
    put(dir, "latin1.txt", b"caf\xe9\n");
    let f = open(dir);

    let png = editor::read(f, "logo.png").unwrap();
    assert_eq!((png.text, png.binary, png.too_large, png.read_only, png.read_only_why.as_deref()), (None, true, false, true, Some("A binary file")));
    let one = editor::read(f, "one_mb.txt").unwrap();
    assert_eq!((one.text.as_ref().map(String::len), one.read_only, one.read_only_why), (Some(MB), false, None), "1 MB itself may change");
    let over_one = editor::read(f, "over_one_mb.txt").unwrap();
    assert_eq!((over_one.text.as_ref().map(String::len), over_one.read_only, over_one.read_only_why.as_deref()), (Some(MB + 1), true, Some("Over 1 MB")));
    let five = editor::read(f, "five_mb.txt").unwrap();
    assert_eq!((five.text.is_some(), five.too_large, five.read_only_why.as_deref()), (true, false, Some("Over 1 MB")), "5 MB itself has its text");
    let over_five = editor::read(f, "over_five_mb.txt").unwrap();
    assert_eq!((over_five.text, over_five.binary, over_five.too_large, over_five.read_only, over_five.read_only_why.as_deref()),
               (None, false, true, true, Some("Over 5 MB")));
    let big = editor::read(f, "big.bin").unwrap();
    assert_eq!((big.text, big.binary, big.too_large, big.read_only), (None, true, true, true));
    let latin1 = editor::read(f, "latin1.txt").unwrap();
    assert_eq!((latin1.read_only, latin1.read_only_why.as_deref()), (true, Some("Not UTF-8 text")));
    assert!(latin1.text.unwrap().starts_with("caf"));

    // and none of them is saved
    for (p, says) in [("logo.png", "is a binary file"), ("over_one_mb.txt", "is over 1 MB"), ("over_five_mb.txt", "is over 1 MB"),
                      ("latin1.txt", "isn't UTF-8 text")] {
        let v = editor::read(f, p).unwrap().version;
        let why = err(editor::write(f, p, "x", &v));
        assert!(why.starts_with(&format!("{p} {says}")), "{p}: {why}");
    }
    assert_eq!(fs::read(dir.join("logo.png")).unwrap(), b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
    assert_eq!(fs::metadata(dir.join("over_one_mb.txt")).unwrap().len(), MB as u64 + 1);
}

#[test]
fn a_card_an_agent_is_working_on_is_read_only_and_says_who_and_nothing_in_it_changes() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "GA-12", &[("a.txt", "one\n"), ("src/b.rs", "fn b() {}\n")]);
    put(&dir, "logo.png", b"\0\0");
    let f = busy(&dir);
    let a = editor::read(f, "a.txt").unwrap();
    assert_eq!((a.text.as_deref(), a.read_only, a.read_only_why.as_deref()), (Some("one\n"), true, Some(WORKING)));
    // the folder's reason comes first, also for a file that is read-only anyway
    assert_eq!(editor::read(f, "logo.png").unwrap().read_only_why.as_deref(), Some(WORKING));
    let says = |why: String, rel: &str| assert_eq!(why, format!("The Editor can't change {rel} now: {WORKING}"));
    says(err(editor::write(f, "a.txt", "two\n", &a.version)), "a.txt");
    says(err(editor::write(f, "new.txt", "x", "")), "new.txt");
    says(err(editor::create(f, "c.txt", false)), "c.txt");
    says(err(editor::create(f, "docs", true)), "docs");
    says(err(editor::rename(f, "a.txt", "z.txt")), "a.txt");
    says(err(editor::delete(f, "src")), "src");
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
    assert!(dir.join("src/b.rs").is_file() && !dir.join("new.txt").exists() && !dir.join("c.txt").exists() && !dir.join("docs").exists());
    // looking still works
    assert_eq!(editor::list(&dir, "").unwrap().len(), 3);
    assert_eq!(editor::files(&dir).unwrap(), ["a.txt", "logo.png", "src/b.rs"]);
}

#[test]
fn a_folder_git_ignores_is_read_only_inside_and_nothing_in_it_is_made_moved_or_deleted() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[(".gitignore", "node_modules/\nvendor/\n*.log\n"), ("a.txt", "one\n")]);
    put(&dir, "node_modules/pkg/index.js", b"module.exports = 1;\n");
    put(&dir, "debug.log", b"log\n");
    let f = open(&dir);
    let js = editor::read(f, "node_modules/pkg/index.js").unwrap();
    assert_eq!((js.text.as_deref(), js.read_only, js.read_only_why.as_deref()), (Some("module.exports = 1;\n"), true, Some("Ignored by git")));
    let says = |why: String, rel: &str| assert_eq!(why, format!("{rel} is in a folder git ignores, so the Editor keeps it read-only"));
    says(err(editor::write(f, "node_modules/pkg/index.js", "x", &js.version)), "node_modules/pkg/index.js");
    says(err(editor::write(f, "node_modules/pkg/new.js", "x", "")), "node_modules/pkg/new.js");
    says(err(editor::create(f, "node_modules/new.js", false)), "node_modules/new.js");
    says(err(editor::rename(f, "a.txt", "node_modules/a.txt")), "node_modules/a.txt");
    says(err(editor::rename(f, "node_modules/pkg/index.js", "index.js")), "node_modules/pkg/index.js");
    says(err(editor::delete(f, "node_modules/pkg/index.js")), "node_modules/pkg/index.js");
    assert_eq!(fs::read_to_string(dir.join("node_modules/pkg/index.js")).unwrap(), "module.exports = 1;\n");
    assert!(dir.join("a.txt").is_file() && !dir.join("node_modules/a.txt").exists() && !dir.join("index.js").exists());
}

/// git check-ignore refuses GIT_LITERAL_PATHSPECS ("pathspec magic not supported by this command: 'literal'"): the
/// Editor's look at what git ignores must not use it.
#[test]
fn what_git_ignores_is_greyed_in_the_tree_but_never_a_tracked_file() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[(".gitignore", "node_modules/\nvendor/\n*.log\n"), ("a.txt", "one\n")]);
    put(&dir, "node_modules/pkg/index.js", b"module.exports = 1;\n");
    put(&dir, "debug.log", b"log\n");
    let top = editor::list(&dir, "").unwrap();
    let ignored = |name: &str| top.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name} in {top:?}")).ignored;
    assert!(ignored("node_modules") && ignored("debug.log") && !ignored("a.txt") && !ignored(".gitignore"));
    let inside = editor::list(&dir, "node_modules").unwrap();
    assert_eq!((inside[0].path.as_str(), inside[0].dir, inside[0].ignored), ("node_modules/pkg", true, true));
    // a tracked file in an ignored folder isn't ignored
    put(&dir, "vendor/kept.php", b"<?php\n");
    git(&dir, &["add", "-f", "vendor/kept.php"]);
    assert!(!editor::list(&dir, "vendor").unwrap()[0].ignored);
}

/// A linked folder that is a folder of a repository (repo/app): its own .gitignore and the repository's both count.
#[test]
fn in_a_folder_of_a_repository_what_git_ignores_there_is_greyed_and_read_only_inside() {
    let tmp = real_tempdir();
    let site = repo(tmp.path(), "site", &[(".gitignore", "/app/dist/\n"), ("README.md", "# Site\n"), ("app/.gitignore", "node_modules/\n"),
                                           ("app/main.js", "main\n")]);
    let dir = site.join("app");
    put(&dir, "node_modules/x.js", b"x\n");
    put(&dir, "dist/bundle.js", b"bundle\n");
    let top = editor::list(&dir, "").unwrap();
    assert_eq!(top.iter().map(|e| (e.name.as_str(), e.ignored)).collect::<Vec<_>>(),
               [("dist", true), ("node_modules", true), (".gitignore", false), ("main.js", false)]);
    assert!(editor::list(&dir, "node_modules").unwrap()[0].ignored);
    let f = open(&dir);
    let says = |why: String, rel: &str| assert_eq!(why, format!("{rel} is in a folder git ignores, so the Editor keeps it read-only"));
    for (rel, text) in [("node_modules/x.js", "x\n"), ("dist/bundle.js", "bundle\n")] {
        let opened = editor::read(f, rel).unwrap();
        assert_eq!((opened.text.as_deref(), opened.read_only, opened.read_only_why.as_deref()), (Some(text), true, Some("Ignored by git")));
        says(err(editor::write(f, rel, "changed\n", &opened.version)), rel);
        says(err(editor::rename(f, rel, "moved.js")), rel);
        says(err(editor::delete(f, rel)), rel);
        assert_eq!(fs::read_to_string(dir.join(rel)).unwrap(), text);
    }
    says(err(editor::create(f, "node_modules/new.js", false)), "node_modules/new.js");
    says(err(editor::create(f, "dist/sub", true)), "dist/sub");
    says(err(editor::rename(f, "main.js", "node_modules/main.js")), "node_modules/main.js");
    assert!(!dir.join("moved.js").exists() && !dir.join("node_modules/new.js").exists() && !dir.join("dist/sub").exists());
    // the rest of the folder saves as usual
    let main = editor::read(f, "main.js").unwrap();
    assert_eq!((main.read_only, main.read_only_why), (false, None));
    editor::write(f, "main.js", "main2\n", &main.version).unwrap();
    assert_eq!(fs::read_to_string(dir.join("main.js")).unwrap(), "main2\n");
}

/// git check-ignore reads pathspec magic in a name that starts with a colon: such a name is still checked as itself,
/// and doesn't stop the check of the names after it. Names with a colon don't exist on Windows.
#[cfg(unix)]
#[test]
fn names_that_start_with_a_colon_are_checked_as_themselves() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[(".gitignore", "*.log\n"), ("a.txt", "a\n")]);
    for name in [":a.log", ":!b", ":(top)c", ":^e.log", "c.log", "z.log", "zz.txt"] {
        put(&dir, name, b"x\n");
    }
    let top = editor::list(&dir, "").unwrap();
    let ignored: Vec<&str> = top.iter().filter(|e| e.ignored).map(|e| e.name.as_str()).collect();
    assert_eq!(ignored, [":^e.log", ":a.log", "c.log", "z.log"], "{top:?}");
    // a folder named so: what is in it is read-only
    put(&dir, ":x.log/in.txt", b"in\n");
    let inner = editor::read(open(&dir), ":x.log/in.txt").unwrap();
    assert_eq!(inner.read_only_why.as_deref(), Some("Ignored by git"));
    let plain = editor::read(open(&dir), ":!b").unwrap();
    assert_eq!((plain.read_only, plain.read_only_why), (false, None));
}

/// Outside a repository nothing is ignored, also with a .gitignore, and every change goes through.
#[test]
fn outside_git_nothing_is_ignored_and_every_change_goes_through() {
    let tmp = real_tempdir();
    if !outside_git(tmp.path()) {
        return;
    }
    let dir = tmp.path().join("plain");
    put(&dir, ".gitignore", b"node_modules/\n*.log\n");
    put(&dir, "node_modules/x.js", b"x\n");
    put(&dir, "debug.log", b"log\n");
    let top = editor::list(&dir, "").unwrap();
    assert_eq!(top.iter().map(|e| (e.name.as_str(), e.ignored, e.git)).collect::<Vec<_>>(),
               [("node_modules", false, None), (".gitignore", false, None), ("debug.log", false, None)]);
    let f = open(&dir);
    let x = editor::read(f, "node_modules/x.js").unwrap();
    assert_eq!((x.read_only, x.read_only_why), (false, None));
    editor::write(f, "node_modules/x.js", "y\n", &x.version).unwrap();
    editor::create(f, "node_modules/new.js", false).unwrap();
    editor::rename(f, "node_modules/new.js", "node_modules/renamed.js").unwrap();
    editor::delete(f, "node_modules/x.js").unwrap();
    assert_eq!(editor::list(&dir, "node_modules").unwrap().iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["renamed.js"]);
}

/// Every git call but check-ignore keeps literal pathspecs: in a folder named like a glob (app/[id]), git status, Go to
/// file and search see that folder only, never app/i.
#[test]
fn in_a_folder_named_like_a_glob_git_status_go_to_file_and_search_see_only_that_folder() {
    let tmp = real_tempdir();
    let site = repo(tmp.path(), "site", &[("app/[id]/page.tsx", "needle\n"), ("app/i/page.tsx", "needle\n"), ("app/d/page.tsx", "needle\n")]);
    put(&site, "app/[id]/page.tsx", b"needle two\n");
    put(&site, "app/i/page.tsx", b"needle three\n");
    put(&site, "app/[id]/new.tsx", b"needle\n");
    put(&site, "app/d/new.tsx", b"needle\n");
    let dir = site.join("app").join("[id]");
    let st = editor::status(&dir);
    assert_eq!(st.files.iter().map(|(p, m)| (p.as_str(), *m)).collect::<Vec<_>>(), [("new.tsx", GitMark::Untracked), ("page.tsx", GitMark::Modified)]);
    assert_eq!(st.uncommitted, 2);
    assert_eq!(editor::files(&dir).unwrap(), ["new.tsx", "page.tsx"]);
    let found = editor::search(&dir, "needle", Find::default()).unwrap();
    assert_eq!(found.hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(), ["new.tsx", "page.tsx"]);
    assert_eq!(editor::list(&dir, "").unwrap().iter().map(|e| (e.name.as_str(), e.git, e.ignored)).collect::<Vec<_>>(),
               [("new.tsx", Some(GitMark::Untracked), false), ("page.tsx", Some(GitMark::Modified), false)]);
}

/// check-ignore runs without literal pathspecs, so a name that reads as a glob mustn't take the place of another one.
/// Known limit (GA-107 QA): check-ignore leaves out every path whose pathspec matches a tracked file, and `[ab].log`
/// as a glob matches the tracked `a.log`, so `[ab].log` isn't greyed.
#[test]
#[ignore = "known limit: git check-ignore matches a name like [ab].log as a glob against tracked files"]
fn a_name_with_glob_characters_is_ignored_as_itself() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[(".gitignore", "*.log\n"), ("a.txt", "a\n")]);
    put(&dir, "a.log", b"kept\n");
    git(&dir, &["add", "-f", "a.log"]);
    put(&dir, "[ab].log", b"x\n");
    let top = editor::list(&dir, "").unwrap();
    let ignored = |name: &str| top.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name} in {top:?}")).ignored;
    assert!(!ignored("a.log"), "a tracked file is never ignored");
    assert!(ignored("[ab].log"), "{top:?}");
}

// ---- saving ----

#[test]
fn save_keeps_crlf_and_the_byte_order_mark_and_returns_the_new_version() {
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "crlf.txt", b"\xEF\xBB\xBFone\r\ntwo\r\n");
    put(dir, "lf.txt", b"a\n");
    let f = open(dir);
    let opened = editor::read(f, "crlf.txt").unwrap();
    let v = editor::write(f, "crlf.txt", "one\ntwo\nthree\n", &opened.version).unwrap();
    assert_eq!(fs::read(dir.join("crlf.txt")).unwrap(), b"\xEF\xBB\xBFone\r\ntwo\r\nthree\r\n");
    assert_ne!(v, opened.version);
    let again = editor::read(f, "crlf.txt").unwrap();
    assert_eq!((again.version.as_str(), again.text.as_deref(), again.crlf, again.bom), (v.as_str(), Some("one\ntwo\nthree\n"), true, true));
    assert_eq!(editor::versions(dir, &["crlf.txt".into()]), [v.clone()]);
    // a byte order mark in the text doesn't count twice, and CRLF in the text doesn't become CRCRLF
    editor::write(f, "crlf.txt", "\u{feff}x\r\ny\n", &v).unwrap();
    assert_eq!(fs::read(dir.join("crlf.txt")).unwrap(), b"\xEF\xBB\xBFx\r\ny\r\n");
    // an LF file stays LF, without a byte order mark
    let lf = editor::read(f, "lf.txt").unwrap();
    editor::write(f, "lf.txt", "b\r\nc\n", &lf.version).unwrap();
    assert_eq!(fs::read(dir.join("lf.txt")).unwrap(), b"b\nc\n");
    // a new file (version "") with the folders above it
    let made = editor::write(f, "docs/new/notes.md", "# Notes\n", "").unwrap();
    assert_eq!(fs::read_to_string(dir.join("docs/new/notes.md")).unwrap(), "# Notes\n");
    assert_eq!(editor::read(f, "docs/new/notes.md").unwrap().version, made);
    // nothing of the saves is left next to the files
    let left: Vec<_> = fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.contains("gizai-save")).collect();
    assert!(left.is_empty(), "{left:?}");
}

#[cfg(unix)]
#[test]
fn save_writes_a_new_file_and_renames_it_over_the_old_one_with_its_permissions() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "run.sh", b"#!/bin/sh\r\necho one\r\n");
    fs::set_permissions(dir.join("run.sh"), fs::Permissions::from_mode(0o750)).unwrap();
    // a second name for the old file: an in-place write would change it too, a rename over it doesn't
    fs::hard_link(dir.join("run.sh"), dir.join("old-name.sh")).unwrap();
    let before = fs::metadata(dir.join("run.sh")).unwrap().ino();
    let f = open(dir);
    let v = editor::read(f, "run.sh").unwrap().version;
    editor::write(f, "run.sh", "#!/bin/sh\necho two\n", &v).unwrap();
    assert_eq!(fs::read(dir.join("run.sh")).unwrap(), b"#!/bin/sh\r\necho two\r\n");
    assert_eq!(fs::read(dir.join("old-name.sh")).unwrap(), b"#!/bin/sh\r\necho one\r\n", "the old file was never written to");
    let after = fs::metadata(dir.join("run.sh")).unwrap();
    assert_ne!(after.ino(), before, "a new file took the old one's place");
    assert_eq!(after.permissions().mode() & 0o777, 0o750, "the script keeps its permissions");
    assert_eq!(fs::read_dir(dir).unwrap().count(), 2, "no file of the save is left");
}

#[test]
fn save_refuses_with_changed_on_disk_when_the_file_changed_or_went_since_it_was_read() {
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "a.txt", b"one\n");
    put(dir, "b.txt", b"bee\n");
    let f = open(dir);
    let v = editor::read(f, "a.txt").unwrap().version;
    fs::write(dir.join("a.txt"), "one, and an agent's two\n").unwrap();
    let why = err(editor::write(f, "a.txt", "mine\n", &v));
    assert_eq!(why, "a.txt changed on disk since you opened it");
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one, and an agent's two\n", "the other change is kept");
    let now = editor::versions(dir, &["a.txt".into()]).remove(0);
    assert_ne!(now, v);
    editor::write(f, "a.txt", "mine\n", &now).unwrap();
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "mine\n");
    // gone since it was read: not made again
    let v = editor::read(f, "b.txt").unwrap().version;
    fs::remove_file(dir.join("b.txt")).unwrap();
    let why = err(editor::write(f, "b.txt", "bee\n", &v));
    assert!(why.contains("changed on disk"), "{why}");
    assert!(!dir.join("b.txt").exists());
    assert_eq!(editor::versions(dir, &["b.txt".into()]), [""]);
    // "" is a file that isn't there yet: one that is there now changed on disk
    editor::write(f, "c.txt", "first\n", "").unwrap();
    assert!(err(editor::write(f, "c.txt", "second\n", "")).contains("changed on disk"));
    assert_eq!(fs::read_to_string(dir.join("c.txt")).unwrap(), "first\n");
    // a folder isn't saved over
    fs::create_dir(dir.join("src")).unwrap();
    assert_eq!(err(editor::write(f, "src", "x", "")), "src is a folder");
    assert_eq!(err(editor::write(f, "", "x", "")), "Pick a file to save");
}

#[cfg(unix)]
#[test]
fn any_change_gives_another_version_even_one_of_the_same_size_with_the_old_time_put_back() {
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "a.txt", b"abc\n");
    let f = open(dir);
    let v = editor::read(f, "a.txt").unwrap().version;
    let modified = fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let file = fs::OpenOptions::new().write(true).open(dir.join("a.txt")).unwrap();
    use std::io::Write;
    (&file).write_all(b"xyz\n").unwrap();
    file.set_modified(modified).unwrap();
    drop(file);
    assert_eq!(fs::metadata(dir.join("a.txt")).unwrap().modified().unwrap(), modified);
    assert_ne!(editor::versions(dir, &["a.txt".into()])[0], v);
    assert!(err(editor::write(f, "a.txt", "mine\n", &v)).contains("changed on disk"));
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "xyz\n");
}

#[cfg(unix)]
#[test]
fn a_file_that_is_read_only_on_disk_is_read_only_in_the_editor() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "locked.txt", b"one\n");
    fs::set_permissions(dir.join("locked.txt"), fs::Permissions::from_mode(0o444)).unwrap();
    let f = open(dir);
    let opened = editor::read(f, "locked.txt").unwrap();
    assert_eq!((opened.read_only, opened.read_only_why.as_deref()), (true, Some("Read-only on disk")));
    assert_eq!(err(editor::write(f, "locked.txt", "two\n", &opened.version)), "locked.txt is read-only on disk");
    assert_eq!(fs::read_to_string(dir.join("locked.txt")).unwrap(), "one\n");
}

// ---- new, rename and delete ----

#[test]
fn new_rename_and_delete_change_only_what_they_name() {
    let tmp = real_tempdir();
    let dir = tmp.path();
    put(dir, "a.txt", b"one\n");
    put(dir, "src/main.rs", b"fn main() {}\n");
    let f = open(dir);
    editor::create(f, "src/deep/new.rs", false).unwrap();
    assert_eq!(fs::read(dir.join("src/deep/new.rs")).unwrap(), b"");
    editor::create(f, "docs/guides", true).unwrap();
    assert!(dir.join("docs/guides").is_dir());
    assert_eq!(err(editor::create(f, "a.txt", false)), "a.txt is there already");
    assert_eq!(err(editor::create(f, "src", true)), "src is there already");
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "one\n");
    assert_eq!(err(editor::create(f, "", false)), "Pick a file or folder in the folder to make");

    editor::rename(f, "src/deep/new.rs", "lib/moved.rs").unwrap();
    assert!(dir.join("lib/moved.rs").is_file() && !dir.join("src/deep/new.rs").exists());
    assert_eq!(err(editor::rename(f, "lib/moved.rs", "a.txt")), "a.txt is there already");
    assert_eq!(err(editor::rename(f, "src", "src/inner/src")), "src can't move into itself");
    editor::rename(f, "a.txt", "a.txt").unwrap();
    editor::rename(f, "a.txt", "A.txt").unwrap();
    assert_eq!(fs::read_to_string(dir.join("A.txt")).unwrap(), "one\n");
    editor::rename(f, "src", "source").unwrap();
    assert_eq!(fs::read_to_string(dir.join("source/main.rs")).unwrap(), "fn main() {}\n");
    assert_eq!(err(editor::rename(f, "gone.txt", "b.txt")), "gone.txt isn't there any more");
    assert_eq!(err(editor::rename(f, "", "b.txt")), "Pick a file or folder in the folder to rename");

    put(dir, "docs/guides/one.md", b"# One\n");
    editor::delete(f, "docs").unwrap();
    assert!(!dir.join("docs").exists());
    editor::delete(f, "lib/moved.rs").unwrap();
    assert!(!dir.join("lib/moved.rs").exists() && dir.join("lib").is_dir());
    assert_eq!(err(editor::delete(f, "gone.txt")), "gone.txt isn't there any more");
    assert_eq!(err(editor::delete(f, "")), "Pick a file or folder in the folder to delete");
    assert_eq!(err(editor::delete(f, ".")), "Pick a file or folder in the folder to delete");
    assert!(dir.join("A.txt").is_file() && dir.join("source/main.rs").is_file());
}

// ---- a folder level, Go to file, git status ----

#[test]
fn a_folder_level_shows_folders_first_by_name_with_sizes_and_git_marks() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[("README.md", "# Shop\n"), ("src/main.rs", "fn main() {}\n"), ("src/lib.rs", "\n"),
                                          ("Zeta/z.txt", "z\n"), ("alpha/a.txt", "a\n"), ("app/[id]/page.tsx", "x\n")]);
    put(&dir, "src/main.rs", b"fn main() { run() }\n");
    put(&dir, "alpha/new.txt", b"new\n");
    put(&dir, "new.txt", b"new\n");
    put(&dir, "added.txt", b"added\n");
    git(&dir, &["add", "added.txt"]);
    let top = editor::list(&dir, "").unwrap();
    let names: Vec<&str> = top.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["alpha", "app", "src", "Zeta", "added.txt", "new.txt", "README.md"]);
    let by = |name: &str| top.iter().find(|e| e.name == name).unwrap();
    assert_eq!((by("src").dir, by("src").size, by("src").git), (true, 0, Some(GitMark::Modified)));
    assert_eq!(by("alpha").git, Some(GitMark::Modified), "an untracked file inside counts");
    assert_eq!((by("Zeta").git, by("app").git), (None, None));
    assert_eq!((by("new.txt").git, by("added.txt").git, by("README.md").git), (Some(GitMark::Untracked), Some(GitMark::Added), None));
    assert_eq!((by("README.md").dir, by("README.md").size, by("README.md").path.as_str()), (false, 7, "README.md"));

    let src = editor::list(&dir, "src").unwrap();
    assert_eq!(src.iter().map(|e| (e.path.as_str(), e.git)).collect::<Vec<_>>(), [("src/lib.rs", None), ("src/main.rs", Some(GitMark::Modified))]);
    // a folder named like a pathspec is that folder
    put(&dir, "app/[id]/page.tsx", b"y\n");
    put(&dir, "app/i/page.tsx", b"i\n");
    let app = editor::list(&dir, "app/[id]").unwrap();
    assert_eq!((app[0].path.as_str(), app[0].git), ("app/[id]/page.tsx", Some(GitMark::Modified)));
    assert_eq!(editor::list(&dir, "app").unwrap().iter().map(|e| (e.name.as_str(), e.git)).collect::<Vec<_>>(),
               [("[id]", Some(GitMark::Modified)), ("i", Some(GitMark::Modified))]);

    assert_eq!(err(editor::list(&dir, "README.md")), "README.md is a file, not a folder");
    assert_eq!(err(editor::list(&dir, "nope")), "nope isn't there any more");
}

#[test]
fn git_status_gives_the_branch_and_each_changed_files_mark() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[("a.txt", "a\n"), ("b.txt", "b\n"), ("c.txt", "c\n"), ("e.txt", "e\n"), ("sub/x.txt", "x\n")]);
    // a merge conflict in e.txt
    git(&dir, &["checkout", "-q", "-b", "other"]);
    put(&dir, "e.txt", b"theirs\n");
    git(&dir, &["commit", "-q", "-am", "theirs"]);
    git(&dir, &["checkout", "-q", "main"]);
    put(&dir, "e.txt", b"ours\n");
    git(&dir, &["commit", "-q", "-am", "ours"]);
    let merge = Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t", "merge", "-q", "other"]).current_dir(&dir).output().unwrap();
    assert!(!merge.status.success(), "the merge conflicts");
    put(&dir, "a.txt", b"changed\n");
    put(&dir, "f.txt", b"f\n");
    git(&dir, &["add", "f.txt"]);
    git(&dir, &["rm", "-q", "b.txt"]);
    git(&dir, &["mv", "c.txt", "c2.txt"]);
    put(&dir, "u/one.txt", b"1\n");
    put(&dir, "u/two.txt", b"2\n");

    let st = editor::status(&dir);
    assert_eq!(st.branch.as_deref(), Some("main"));
    let want = [("a.txt", GitMark::Modified), ("b.txt", GitMark::Deleted), ("c2.txt", GitMark::Renamed), ("e.txt", GitMark::Conflicted),
                ("f.txt", GitMark::Added), ("u/one.txt", GitMark::Untracked), ("u/two.txt", GitMark::Untracked)];
    assert_eq!(st.files.iter().map(|(p, m)| (p.as_str(), *m)).collect::<Vec<_>>(), want);
    assert_eq!(st.uncommitted, 7);

    // a folder inside a repository: its paths are the folder's own
    put(&dir, "sub/x.txt", b"changed\n");
    let sub = editor::status(&dir.join("sub"));
    assert_eq!(sub.files.iter().map(|(p, m)| (p.as_str(), *m)).collect::<Vec<_>>(), [("x.txt", GitMark::Modified)]);

    // a detached HEAD has no branch
    let clean = repo(tmp.path(), "clean", &[("a.txt", "a\n")]);
    git(&clean, &["checkout", "-q", "--detach"]);
    let st = editor::status(&clean);
    assert_eq!((st.branch, st.files.len(), st.uncommitted), (None, 0, 0));
}

/// Outside git: works only where the temp folder isn't inside a git checkout itself.
fn outside_git(tmp: &Path) -> bool {
    !Command::new("git").args(["rev-parse", "--git-dir"]).current_dir(tmp).output().unwrap().status.success()
}

#[test]
fn go_to_file_lists_tracked_and_untracked_files_that_git_doesnt_ignore() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[(".gitignore", "node_modules/\n*.log\n"), ("a.txt", "a\n"), ("src/b.rs", "b\n"), ("gone.txt", "g\n")]);
    fs::remove_file(dir.join("gone.txt")).unwrap();
    put(&dir, "c.txt", b"c\n");
    put(&dir, "new/d.txt", b"d\n");
    put(&dir, "node_modules/x.js", b"x\n");
    put(&dir, "debug.log", b"log\n");
    assert_eq!(editor::files(&dir).unwrap(), [".gitignore", "a.txt", "c.txt", "new/d.txt", "src/b.rs"]);
    // a folder in the repository: its own files, by their paths in it
    assert_eq!(editor::files(&dir.join("src")).unwrap(), ["b.rs"]);
    // outside git, every file but .git's
    let plain = tmp.path().join("plain");
    put(&plain, "a.txt", b"a\n");
    put(&plain, "sub/b.txt", b"b\n");
    put(&plain, ".git/config", b"x\n");
    if outside_git(tmp.path()) {
        assert_eq!(editor::files(&plain).unwrap(), ["a.txt", "sub/b.txt"]);
        assert_eq!(editor::status(&plain), Default::default());
    }
}

// ---- search ----

#[test]
fn search_finds_lines_with_git_grep_in_files_git_doesnt_ignore() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[(".gitignore", "node_modules/\n"), ("a.txt", "Needle here\nno match\nhéllo needle\n"),
                                          ("b.rs", "fn needle_case() {}\nNEEDLES\n")]);
    put(&dir, "c.txt", b"an untracked needle\n");
    put(&dir, "node_modules/x.js", b"needle\n");
    put(&dir, "logo.png", b"needle\0\0");
    let hits = |q: &str, how: Find| {
        let found = editor::search(&dir, q, how).unwrap();
        assert!(!found.truncated);
        found.hits.into_iter().map(|h| (h.path, h.line, h.column, h.text)).collect::<Vec<_>>()
    };
    let s = |p: &str, l: u32, c: u32, t: &str| (p.to_string(), l, c, t.to_string());
    assert_eq!(hits("needle", Find::default()), [s("a.txt", 1, 1, "Needle here"), s("a.txt", 3, 7, "héllo needle"), s("b.rs", 1, 4, "fn needle_case() {}"),
                                                 s("b.rs", 2, 1, "NEEDLES"), s("c.txt", 1, 14, "an untracked needle")]);
    assert_eq!(hits("Needle", Find { case_sensitive: true, ..Default::default() }), [s("a.txt", 1, 1, "Needle here")]);
    assert_eq!(hits("needle", Find { whole_word: true, ..Default::default() }).iter().map(|h| (h.0.as_str(), h.1)).collect::<Vec<_>>(),
               [("a.txt", 1), ("a.txt", 3), ("c.txt", 1)]);
    assert_eq!(hits("^fn [a-z_]+\\(", Find { regex: true, ..Default::default() }), [s("b.rs", 1, 1, "fn needle_case() {}")]);
    // without regex, the query is plain text
    assert_eq!(hits("needle_case()", Find::default()).len(), 1);
    assert!(hits("^fn", Find::default()).is_empty());
    let why = err(editor::search(&dir, "(", Find { regex: true, ..Default::default() }));
    assert!(why.starts_with("Search couldn't use that regular expression"), "{why}");
    assert_eq!(editor::search(&dir, "", Find::default()).unwrap(), Default::default());
    assert_eq!(err(editor::search(&dir, "a\nb", Find::default())), "Search looks within one line at a time: leave out the line break");
    // outside git, the .gitignore files still count
    if outside_git(tmp.path()) {
        let plain = tmp.path().join("plain");
        put(&plain, ".gitignore", b"skip/\n");
        put(&plain, "a.txt", b"a needle\n");
        put(&plain, "skip/b.txt", b"needle\n");
        let found = editor::search(&plain, "needle", Find::default()).unwrap();
        assert_eq!(found.hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(), ["a.txt"]);
    }
}

// The Editor marks a hit's match by cutting its line at `column` (src/lib/editor.ts hitParts): so a tab counts as one,
// and letters count as JavaScript counts them (UTF-16), whatever git's byte column says.
#[test]
fn search_counts_a_hits_column_in_the_lines_own_characters_a_tab_as_one_and_utf16_beyond_ascii() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[("Send.php", "<?php\n\t\treturn $invoice;\n"), ("notes.md", "😀\tneedle\nhé\tneedle\n"),
                                          ("crlf.txt", "\tneedle\r\n")]);
    let hits = |q: &str, how: Find| {
        editor::search(&dir, q, how).unwrap().hits.into_iter().map(|h| (h.path, h.line, h.column, h.text)).collect::<Vec<_>>()
    };
    let s = |p: &str, l: u32, c: u32, t: &str| (p.to_string(), l, c, t.to_string());
    assert_eq!(hits("return", Find::default()), [s("Send.php", 2, 3, "\t\treturn $invoice;")]);
    assert_eq!(hits("ret\\w+", Find { regex: true, ..Default::default() }), [s("Send.php", 2, 3, "\t\treturn $invoice;")]);
    assert_eq!(hits("return", Find { whole_word: true, case_sensitive: true, ..Default::default() }), [s("Send.php", 2, 3, "\t\treturn $invoice;")]);
    // 😀 is 4 bytes and 2 UTF-16 units, é 2 bytes and 1 unit; a Windows line ending is left out of the text
    assert_eq!(hits("needle", Find::default()), [s("crlf.txt", 1, 2, "\tneedle"), s("notes.md", 1, 4, "😀\tneedle"), s("notes.md", 2, 4, "hé\tneedle")]);
}

#[test]
fn search_stops_at_2000_hits_or_after_its_time_and_says_so() {
    assert_eq!((editor::MAX_HITS, editor::SEARCH_LIMIT), (2000, Duration::from_secs(10)));
    let tmp = real_tempdir();
    let many: String = (1..=2500).map(|n| format!("needle {n}\n")).collect();
    let exact: String = (1..=2000).map(|n| format!("pin {n}\n")).collect();
    let dir = repo(tmp.path(), "shop", &[("many.txt", &many), ("exact.txt", &exact)]);
    let found = editor::search(&dir, "needle", Find::default()).unwrap();
    assert_eq!((found.hits.len(), found.truncated), (2000, true));
    assert_eq!((found.hits[0].line, found.hits[1999].line), (1, 2000));
    let found = editor::search(&dir, "pin", Find::default()).unwrap();
    assert_eq!((found.hits.len(), found.truncated), (2000, false), "2,000 hits is all of them");
    let found = editor::search_within(&dir, "needle", Find::default(), 3, Duration::from_secs(10)).unwrap();
    assert_eq!((found.hits.len(), found.truncated), (3, true));
    // out of time
    let started = std::time::Instant::now();
    let found = editor::search_within(&dir, "needle", Find::default(), 100_000, Duration::ZERO).unwrap();
    assert!(found.truncated && found.hits.len() < 2500, "{} hits", found.hits.len());
    assert!(started.elapsed() < Duration::from_secs(5));
}

// ---- Open in VS Code, Zed or PhpStorm ----

#[test]
fn the_editors_open_the_folder_or_a_file_in_it_at_a_line() {
    let dir = Path::new("shop");
    let file = Path::new("shop").join("src").join("main.rs");
    let args = |id, line| editor::open_args(id, dir, Some(&file), line).into_iter().map(|a| a.to_string_lossy().to_string()).collect::<Vec<_>>();
    let (d, f) = (dir.display().to_string(), file.display().to_string());
    assert_eq!(args(AppId::VsCode, Some(12)), [d.clone(), "-g".into(), format!("{f}:12")]);
    assert_eq!(args(AppId::Zed, Some(12)), [d.clone(), format!("{f}:12")]);
    assert_eq!(args(AppId::PhpStorm, Some(12)), [d.clone(), "--line".into(), "12".into(), f.clone()]);
    for id in AppId::ALL {
        assert_eq!(args(id, None), [d.clone(), f.clone()]);
        assert_eq!(args(id, Some(0)), [d.clone(), f.clone()], "no line 0");
        assert_eq!(editor::open_args(id, dir, None, Some(3)), [dir.as_os_str().to_os_string()]);
    }
    assert_eq!(AppId::ALL.map(|a| (a.id(), a.name())), [("vscode", "VS Code"), ("zed", "Zed"), ("phpstorm", "PhpStorm")]);
    assert_eq!(AppId::parse("zed"), Some(AppId::Zed));
    assert_eq!(AppId::parse("sublime"), None);
    assert_eq!(serde_json::to_value(EditorApp { id: AppId::VsCode, name: "VS Code".into() }).unwrap(), serde_json::json!({"id": "vscode", "name": "VS Code"}));
    assert_eq!(serde_json::to_value(AppId::PhpStorm).unwrap(), "phpstorm");
}

/// An executable program `name` in `dir`.
#[cfg(unix)]
fn program(dir: &Path, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    fs::write(&p, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[cfg(unix)]
#[test]
fn only_the_editors_found_are_listed_under_any_of_their_names() {
    let tmp = real_tempdir();
    let home = tmp.path().join("home");
    fs::create_dir(&home).unwrap();
    let path = |dirs: &[&Path]| std::env::join_paths(dirs).unwrap();
    let ids = |p: &std::ffi::OsStr| editor::apps(p, &home).into_iter().map(|a| a.id).collect::<Vec<_>>();
    // macOS also looks in /Applications, which this Mac may have
    let mac = cfg!(target_os = "macos");
    let empty = tmp.path().join("empty");
    fs::create_dir(&empty).unwrap();
    if !mac {
        assert!(ids(&path(&[&empty])).is_empty());
    }
    for (name, id) in [("code", AppId::VsCode), ("zed", AppId::Zed), ("zeditor", AppId::Zed), ("zedit", AppId::Zed), ("phpstorm", AppId::PhpStorm),
                       ("phpstorm.sh", AppId::PhpStorm)] {
        let bin = tmp.path().join(format!("bin-{name}"));
        let p = program(&bin, name);
        assert_eq!(editor::find_app(id, &path(&[&empty, &bin]), &home), Some(p), "{name}");
        if !mac {
            assert_eq!(ids(&path(&[&bin])), [id], "{name}");
        }
    }
    // not executable: not an editor
    let plain = tmp.path().join("plain");
    fs::create_dir(&plain).unwrap();
    fs::write(plain.join("code"), "").unwrap();
    if !mac {
        assert_eq!(editor::find_app(AppId::VsCode, &path(&[&plain]), &home), None);
    }
    // ZFS's zed (zpool next to it) isn't Zed; a zeditor further on PATH is
    let zfs = tmp.path().join("sbin");
    program(&zfs, "zed");
    program(&zfs, "zpool");
    if !mac {
        assert_eq!(editor::find_app(AppId::Zed, &path(&[&zfs]), &home), None);
    }
    let zed = tmp.path().join("zed-bin");
    let zeditor = program(&zed, "zeditor");
    assert_eq!(editor::find_app(AppId::Zed, &path(&[&zfs, &zed]), &home), Some(zeditor));
    // JetBrains Toolbox's script, not on PATH
    let toolbox = program(&editor::toolbox_scripts(&home), "phpstorm");
    assert_eq!(editor::find_app(AppId::PhpStorm, &path(&[&empty]), &home), Some(toolbox));
    // all three, in the Open in menu's order, with their names
    let all = tmp.path().join("all");
    for name in ["phpstorm.sh", "zedit", "code"] {
        program(&all, name);
    }
    assert_eq!(editor::apps(&path(&[&all]), &home), [
        EditorApp { id: AppId::VsCode, name: "VS Code".into() },
        EditorApp { id: AppId::Zed, name: "Zed".into() },
        EditorApp { id: AppId::PhpStorm, name: "PhpStorm".into() },
    ]);
}

#[cfg(windows)]
#[test]
fn on_windows_the_editors_are_found_as_cmd_and_exe() {
    let tmp = real_tempdir();
    let home = tmp.path().join("home");
    let bin = tmp.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    for name in ["code.cmd", "zeditor.exe", "phpstorm64.exe"] {
        fs::write(bin.join(name), "").unwrap();
    }
    let path = bin.as_os_str();
    assert_eq!(editor::find_app(AppId::VsCode, path, &home), Some(bin.join("code.cmd")));
    assert_eq!(editor::find_app(AppId::Zed, path, &home), Some(bin.join("zeditor.exe")));
    assert_eq!(editor::find_app(AppId::PhpStorm, path, &home), Some(bin.join("phpstorm64.exe")));
    let empty = tmp.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    let scripts = editor::toolbox_scripts(&home);
    fs::create_dir_all(&scripts).unwrap();
    fs::write(scripts.join("phpstorm.cmd"), "").unwrap();
    assert_eq!(editor::find_app(AppId::PhpStorm, empty.as_os_str(), &home), Some(scripts.join("phpstorm.cmd")));
}

#[cfg(unix)]
#[test]
fn open_in_starts_the_editor_in_the_folder_with_the_file_at_its_line_and_doesnt_wait_for_it() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[("src/main.rs", "fn main() {}\n")]);
    let said = tmp.path().join("said.txt");
    let fake = tmp.path().join("bin/code");
    fs::create_dir_all(fake.parent().unwrap()).unwrap();
    // writes its folder and arguments, then keeps running as an editor does
    fs::write(&fake, format!("#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$PATH\" \"$@\" > '{}.part'\nmv '{0}.part' '{0}'\nexec sleep 3\n", said.display())).unwrap();
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755)).unwrap();
    let file = editor::joined(&dir, "src/main.rs").unwrap();
    assert_eq!(file, dir.join("src").join("main.rs"));
    assert!(editor::joined(&dir, "../x").is_err());
    let path = std::ffi::OsString::from(format!("{}:/usr/bin:/bin", fake.parent().unwrap().display()));
    let started = std::time::Instant::now();
    editor::open_in(AppId::VsCode, &fake, &dir, Some(&file), Some(7), &path).unwrap();
    assert!(started.elapsed() < Duration::from_secs(2), "Gizai doesn't wait for the editor");
    let until = std::time::Instant::now() + Duration::from_secs(20);
    while !said.exists() && std::time::Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    let lines: Vec<String> = fs::read_to_string(&said).unwrap().lines().map(String::from).collect();
    let d = dir.display().to_string();
    assert_eq!(lines, [d.clone(), path.to_string_lossy().to_string(), d.clone(), "-g".into(), format!("{d}/src/main.rs:7")]);
    let missing = tmp.path().join("bin/nothing");
    let why = err(editor::open_in(AppId::Zed, &missing, &dir, None, None, &path));
    assert!(why.starts_with("Gizai couldn't start Zed"), "{why}");
}

#[test]
fn before_phpstorm_opens_a_worktree_idea_is_kept_out_of_git_status_for_every_checkout() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[("a.txt", "a\n")]);
    let wt = tmp.path().join("GA-12");
    git(&dir, &["worktree", "add", "-q", "-b", "gizai/ga-12", wt.to_str().unwrap()]);
    put(&wt, ".idea/workspace.xml", b"<project/>\n");
    assert_eq!(editor::status(&wt).files.keys().collect::<Vec<_>>(), [".idea/workspace.xml"]);
    editor::keep_idea_out(&wt).unwrap();
    editor::keep_idea_out(&wt).unwrap();
    let exclude = fs::read_to_string(dir.join(".git/info/exclude")).unwrap();
    assert_eq!(exclude.lines().filter(|l| l.trim() == editor::IDEA_EXCLUDE).count(), 1, "{exclude}");
    assert_eq!(editor::IDEA_EXCLUDE, "/.idea/");
    let st = editor::status(&wt);
    assert_eq!((st.uncommitted, st.files.len()), (0, 0), "{:?}", st.files);
    let porcelain = Command::new("git").args(["status", "--porcelain", "--untracked-files=all"]).current_dir(&wt).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&porcelain.stdout), "");
    // the main checkout too, and .gitignore is never touched
    put(&dir, ".idea/misc.xml", b"<project/>\n");
    assert_eq!(editor::status(&dir).uncommitted, 0);
    assert!(!dir.join(".gitignore").exists() && !wt.join(".gitignore").exists());
    // what an editor writes in .idea stays readable
    assert!(editor::files(&wt).unwrap().iter().all(|p| !p.starts_with(".idea")));
}

#[test]
fn the_answers_have_contract_c2s_field_names() {
    let tmp = real_tempdir();
    let dir = repo(tmp.path(), "shop", &[("a.txt", "a needle\n")]);
    put(&dir, "b.txt", b"b\n");
    let entry = serde_json::to_value(&editor::list(&dir, "").unwrap()[0]).unwrap();
    let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    let mut want = ["dir", "git", "ignored", "name", "path", "size"].map(String::from).to_vec();
    assert_eq!(keys(&entry), want);
    assert_eq!(entry["git"], serde_json::Value::Null);
    assert_eq!(serde_json::to_value(&editor::list(&dir, "").unwrap()[1]).unwrap()["git"], "untracked");
    let opened = serde_json::to_value(editor::read(open(&dir), "a.txt").unwrap()).unwrap();
    want = ["binary", "bom", "crlf", "path", "readOnly", "readOnlyWhy", "text", "tooLarge", "version"].map(String::from).to_vec();
    assert_eq!(keys(&opened), want);
    let found = serde_json::to_value(editor::search(&dir, "needle", Find::default()).unwrap()).unwrap();
    assert_eq!(found, serde_json::json!({"hits": [{"path": "a.txt", "line": 1, "column": 3, "text": "a needle"}], "truncated": false}));
    let st = serde_json::to_value(editor::status(&dir)).unwrap();
    assert_eq!(st, serde_json::json!({"branch": "main", "files": {"b.txt": "untracked"}, "uncommitted": 1}));
    for (mark, said) in [(GitMark::Modified, "modified"), (GitMark::Added, "added"), (GitMark::Deleted, "deleted"), (GitMark::Renamed, "renamed"),
                         (GitMark::Untracked, "untracked"), (GitMark::Conflicted, "conflicted")] {
        assert_eq!(serde_json::to_value(mark).unwrap(), said);
    }
}
