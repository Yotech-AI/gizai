//! GA-133: terminals (`gizai_agents::terminal`), with a shell the tests type into: /bin/sh on Linux and macOS, cmd.exe on
//! Windows, never your own shell (docs/research/terminal.md §5.7). What a view gets (bytes in order, at most one message
//! every 8 ms (GA-140: the sender's rule itself, and a count over a whole run, never the gap between two calls of the
//! view), the last 256 KB first after a new attach, 50 MB through a slow view with reading paused above 500,000
//! unacknowledged bytes and going on below 200,000), what it sends (input, a 1 MB paste, binary input, a resize), the
//! environment, `busy`, names and renames, how a shell ends and what `close` ends. The folder checks, the data folder and
//! quitting are the app's: src-tauri/tests/it/terminal_flow_test.rs.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::terminal::{self, Changed, Start, TerminalEnd, TerminalSession, Terminals};

/// The shell the tests type into.
fn test_shell() -> (PathBuf, Vec<OsString>) {
    if cfg!(windows) {
        (PathBuf::from(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into())), vec![])
    } else {
        (PathBuf::from("/bin/sh"), vec![])
    }
}

/// A folder for the shell, by its real path on Linux and macOS (as the Editor gives it).
fn folder() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = if cfg!(windows) { tmp.path().to_path_buf() } else { tmp.path().canonicalize().unwrap() };
    (tmp, dir)
}

/// What the app gives `Terminals::open`: `program` in `dir`, with Gizai's environment plus two GIZAI_* variables of
/// Gizai's own that the shell must not get.
fn start(dir: &Path, folder_name: &str, name: Option<&str>, program: (PathBuf, Vec<OsString>)) -> Start {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let own = std::env::vars_os().chain([
        (OsString::from("GIZAI_DATA_DIR"), OsString::from("/gizai/data")),
        (OsString::from("GIZAI_TEST_SECRET"), OsString::from("s3cret")),
    ]);
    Start { folder: dir.display().to_string(), folder_path: dir.display().to_string(), folder_name: folder_name.into(), task_id: None,
            program: program.0, args: program.1, env: terminal::environment(own, &path, "9.9.9"), cols: 80, rows: 24,
            name: name.map(str::to_string) }
}

/// Waits up to 20 s for `ok` to give something.
fn until<T>(what: &str, mut ok: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(t) = ok() {
            return t;
        }
        assert!(Instant::now() < end, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The last 2,000 characters of `text`, for a failure's message.
fn tail(text: &str) -> &str {
    let at = text.char_indices().rev().nth(2000).map_or(0, |(i, _)| i);
    &text[at..]
}

/// What `terminals-changed` was told, in order.
#[derive(Clone, Default)]
struct Told(Arc<Mutex<Vec<Vec<TerminalSession>>>>);

impl Told {
    fn changed(&self) -> Changed {
        let told = self.0.clone();
        Arc::new(move |list| told.lock().unwrap().push(list))
    }

    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn last(&self) -> Vec<TerminalSession> {
        self.0.lock().unwrap().last().cloned().unwrap_or_default()
    }

    /// Waits for a list told after the first `after` that passes `ok`.
    fn until(&self, after: usize, what: &str, ok: impl Fn(&[TerminalSession]) -> bool) -> Vec<TerminalSession> {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(list) = self.0.lock().unwrap().iter().skip(after).find(|l| ok(l)) {
                return list.clone();
            }
            assert!(Instant::now() < end, "never told: {what}; told {:#?}", self.0.lock().unwrap());
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A fake view: keeps the bytes it gets and when each message came.
#[derive(Clone, Default)]
struct Screen {
    bytes: Arc<Mutex<Vec<u8>>>,
    messages: Arc<Mutex<Vec<(Instant, usize)>>>,
}

impl Screen {
    /// Attaches as the terminal's view; with `ack` it acknowledges each message as it comes.
    fn attach(&self, terms: &Arc<Terminals>, id: &str, ack: bool) {
        let (me, terms2, id2) = (self.clone(), terms.clone(), id.to_string());
        terms.attach(id, Box::new(move |b: Vec<u8>| {
            me.messages.lock().unwrap().push((Instant::now(), b.len()));
            // Windows: a view answers ConPTY's cursor-position request, as xterm.js does
            if cfg!(windows) && b.windows(4).any(|w| w == b"\x1b[6n") {
                let _ = terms2.input(&id2, "\x1b[1;1R", false);
            }
            me.bytes.lock().unwrap().extend_from_slice(&b);
            if ack {
                let _ = terms2.ack(&id2, b.len() as u32);
            }
            true
        })).unwrap();
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().unwrap()).into_owned()
    }

    #[cfg(unix)]
    fn len(&self) -> usize {
        self.bytes.lock().unwrap().len()
    }

    /// Waits until the screen shows `want`; returns all it got.
    fn shows(&self, want: &str) -> String {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            let text = self.text();
            if text.contains(want) {
                return text;
            }
            assert!(Instant::now() < end, "the screen never showed {want:?}; it got:\n{}", tail(&text));
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The number printed right after `tag` ("bg=1234"), skipping where the typed command shows it ("bg=$!").
#[cfg(unix)]
fn number_after(text: &str, tag: &str) -> Option<u32> {
    text.match_indices(tag).find_map(|(at, _)| {
        let digits: String = text[at + tag.len()..].chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

#[cfg(unix)]
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(unix)]
fn find_last(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).rposition(|w| w == needle)
}

/// Linux and macOS: the process still runs (a zombie has ended).
#[cfg(unix)]
fn runs(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
        return stat.rfind(')').and_then(|i| stat[i + 1..].split_whitespace().next()).is_some_and(|s| s != "Z" && s != "X");
    }
    // SAFETY: signal 0 only asks whether the process is there.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Linux and macOS: the programs a test started in a terminal, by their exact ids: ended with SIGKILL if the test fails
/// before the terminal ends them.
#[cfg(unix)]
struct Started(Vec<u32>);

#[cfg(unix)]
impl Drop for Started {
    fn drop(&mut self) {
        for &pid in &self.0 {
            if runs(pid) {
                // SAFETY: a plain kill of a process this test started.
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
            }
        }
    }
}

// ---- everywhere ----

#[test]
fn a_session_goes_to_the_window_in_contract_cts_shape() {
    let s = TerminalSession { id: "a1".into(), name: "GA-123 2".into(), folder: "/w/GA-123".into(), folder_path: "~/w/GA-123".into(),
        task_id: Some("t1".into()), shell: "zsh".into(), busy: Some("npm".into()),
        ended: Some(TerminalEnd { code: Some(3), at: 2, why: Some("zsh ended with exit code 3".into()) }), cols: 80, rows: 24, opened_at: 1 };
    assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({
        "id": "a1", "name": "GA-123 2", "folder": "/w/GA-123", "folderPath": "~/w/GA-123", "taskId": "t1", "shell": "zsh", "busy": "npm",
        "ended": {"code": 3, "at": 2, "why": "zsh ended with exit code 3"}, "cols": 80, "rows": 24, "openedAt": 1,
    }));
    let open = TerminalSession { task_id: None, busy: None, ended: None, ..s };
    let json = serde_json::to_value(&open).unwrap();
    assert_eq!((&json["taskId"], &json["busy"], &json["ended"]), (&serde_json::Value::Null, &serde_json::Value::Null, &serde_json::Value::Null));
}

/// The sender's own rule, on its own clock (GA-140): a view's clock can't check it on a busy machine.
#[test]
fn the_sender_sends_the_first_message_at_once_and_each_next_one_8_ms_after_the_last_at_the_earliest() {
    let last = Instant::now();
    let ms = Duration::from_millis;
    assert_eq!(terminal::next_send(None, last), last, "nothing sent before: at once");
    assert_eq!(terminal::next_send(None, last + ms(3)), last + ms(3));
    for now in [last, last + ms(1), last + ms(7), last + ms(8) - Duration::from_nanos(1)] {
        assert_eq!(terminal::next_send(Some(last), now), last + ms(8), "asked {:?} after the last send: 8 ms after it", now - last);
    }
    for now in [last + ms(8), last + ms(9), last + ms(500)] {
        assert_eq!(terminal::next_send(Some(last), now), now, "asked {:?} after the last send: at once", now - last);
    }
}

#[test]
fn the_environment_has_term_colorterm_term_program_and_the_path_runs_get_and_no_gizai_variable() {
    let own = [("PATH", "/usr/bin"), ("GIZAI_DATA_DIR", "/d"), ("gizai_dev", "1"), ("TERM", "dumb"), ("HOME", "/home/me"), ("NOT_GIZAI_X", "kept")]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
    let env = terminal::environment(own, OsStr::new("/login/bin:/usr/bin"), "1.2.3");
    let get = |k: &str| env.iter().filter(|(n, _)| n == k).map(|(_, v)| v.to_string_lossy().into_owned()).collect::<Vec<_>>();
    assert_eq!(get("TERM"), ["xterm-256color"]);
    assert_eq!(get("COLORTERM"), ["truecolor"]);
    assert_eq!(get("TERM_PROGRAM"), ["gizai"]);
    assert_eq!(get("TERM_PROGRAM_VERSION"), ["1.2.3"]);
    assert_eq!(get("PATH"), ["/login/bin:/usr/bin"], "the PATH runs get");
    assert_eq!((get("HOME"), get("NOT_GIZAI_X")), (vec!["/home/me".to_string()], vec!["kept".to_string()]), "the rest of Gizai's environment");
    assert!(!env.iter().any(|(k, _)| k.to_string_lossy().to_ascii_uppercase().starts_with("GIZAI_")), "{env:?}");
    // macOS: LANG when no locale variable is set, and only then
    if cfg!(target_os = "macos") {
        assert_eq!(get("LANG"), ["en_US.UTF-8"]);
    } else {
        assert!(get("LANG").is_empty(), "{env:?}");
    }
    let with_locale = terminal::environment([(OsString::from("LC_ALL"), OsString::from("nl_NL.UTF-8"))], OsStr::new("/usr/bin"), "1.2.3");
    assert!(!with_locale.iter().any(|(k, _)| k == "LANG"), "{with_locale:?}");
    assert!(with_locale.iter().any(|(k, v)| k == "LC_ALL" && v == "nl_NL.UTF-8"));
}

#[test]
fn the_shell_is_your_shell_a_login_shell_on_macos_and_on_windows_powershell_7_else_windows_powershell_else_cmd() {
    use gizai_agents::os;
    let path = std::env::var_os("PATH").unwrap_or_default();
    let (shell, args) = os::user_shell(&path);
    if cfg!(windows) {
        let want = os::find_in("pwsh.exe", &path).or_else(|| os::find_in("powershell.exe", &path));
        match want {
            Some(ps) => assert_eq!((shell, args), (ps, vec![OsString::from("-NoLogo")])),
            None => assert_eq!(shell.file_stem().unwrap().to_ascii_lowercase(), "cmd"),
        }
        return;
    }
    match std::env::var_os("SHELL").filter(|s| !s.is_empty()).and_then(|s| os::find_in(&s.to_string_lossy(), &path)) {
        Some(yours) => assert_eq!(shell, yours, "$SHELL"),
        None => assert!(shell.is_absolute() && shell.exists(), "the password database's shell, else /bin/sh: {}", shell.display()),
    }
    let login: Vec<OsString> = if cfg!(target_os = "macos") { vec!["-l".into()] } else { vec![] };
    assert_eq!(args, login, "an interactive shell; on macOS a login shell");
}

#[test]
fn default_names_take_the_lowest_free_number_and_a_rename_keeps_40_characters_or_gives_the_default_back() {
    let (_tmp, dir) = folder();
    let terms = Terminals::default();
    let told = Told::default();
    let open = |folder_name: &str, name: Option<&str>| terms.open(start(&dir, folder_name, name, test_shell()), told.changed()).unwrap();
    let a = open("gizai", None);
    let b = open("gizai", None);
    let c = open("gizai", Some(""));
    assert_eq!([a.name.as_str(), b.name.as_str(), c.name.as_str()], ["gizai", "gizai 2", "gizai 3"]);
    assert_eq!(told.last().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["gizai", "gizai 2", "gizai 3"], "each open tells the whole list");
    terms.close(&b.id).unwrap();
    let d = open("gizai", Some("   "));
    assert_eq!(d.name, "gizai 2", "the lowest number no open terminal has");
    let e = open("gizai", Some("  build \n"));
    assert_eq!(e.name, "build", "a name given is kept, trimmed");
    let card = open("GA-123", None);
    assert_eq!(card.name, "GA-123", "a card's worktree: the card's identifier");

    // a rename: trimmed, at most 40 characters, and told
    let n = told.count();
    let long = format!("  {} {}  ", "x".repeat(30), "y".repeat(20));
    let r = terms.rename(&a.id, &long).unwrap();
    assert_eq!(r.name, format!("{} {}", "x".repeat(30), "y".repeat(9)));
    assert_eq!(r.name.chars().count(), 40);
    told.until(n, "the rename", |l| l.iter().any(|s| s.id == a.id && s.name == r.name));
    assert_eq!(terms.rename(&a.id, &"é".repeat(41)).unwrap().name, "é".repeat(40), "characters, not bytes");
    // an empty name gives the default back: the folder's name, numbered when another open terminal has it
    assert_eq!(terms.rename(&a.id, "   ").unwrap().name, "gizai", "free again");
    assert_eq!(terms.rename(&e.id, "").unwrap().name, "gizai 4");
    assert_eq!(terms.list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["gizai", "gizai 3", "gizai 2", "gizai 4", "GA-123"],
               "in the order they were opened");
    assert_eq!(terms.rename(&b.id, "x").unwrap_err(), "That terminal is closed");

    terms.close_all();
    assert!(terms.list().is_empty());
    assert!(told.last().is_empty(), "closing tells the list without them");
}

// ---- Linux and macOS ----

#[cfg(unix)]
#[test]
fn a_shell_echoes_what_it_is_sent_starts_in_its_folder_and_follows_a_resize() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let told = Told::default();
    let s = terms.open(start(&dir, "kade", None, test_shell()), told.changed()).unwrap();
    assert_eq!((s.name.as_str(), s.shell.as_str(), s.cols, s.rows, &s.busy, &s.ended), ("kade", "sh", 80, 24, &None, &None));
    assert_eq!((s.folder.as_str(), s.folder_path.as_str()), (dir.to_str().unwrap(), dir.to_str().unwrap()));
    assert_eq!(told.last(), [s.clone()], "the open is told");
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    terms.input(&s.id, "echo got-$((40+2)) in \"$(pwd)\"\n", false).unwrap();
    screen.shows(&format!("got-42 in {}", dir.display()));

    terms.resize(&s.id, 100, 33).unwrap();
    assert_eq!(terms.list().iter().map(|s| (s.cols, s.rows)).collect::<Vec<_>>(), [(100, 33)]);
    terms.input(&s.id, "stty size\n", false).unwrap();
    screen.shows("33 100");
    // a place not measured yet (0) keeps the size
    terms.resize(&s.id, 0, 0).unwrap();
    assert_eq!((terms.list()[0].cols, terms.list()[0].rows), (100, 33));
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn the_shell_has_term_colorterm_and_term_program_set_and_no_gizai_variable() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let s = terms.open(start(&dir, "kade", None, test_shell()), Told::default().changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    terms.input(&s.id, "printf 'vars=%s,%s,%s,%s\\n' \"$TERM\" \"$COLORTERM\" \"$TERM_PROGRAM\" \"$TERM_PROGRAM_VERSION\"\n", false).unwrap();
    screen.shows("vars=xterm-256color,truecolor,gizai,9.9.9");
    terms.input(&s.id, "echo gizai-vars=$(env | grep -ci '^gizai_')\n", false).unwrap();
    screen.shows("gizai-vars=0");
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn busy_names_sleep_while_it_runs_and_the_window_hears_of_each_change() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let told = Told::default();
    let s = terms.open(start(&dir, "kade", None, test_shell()), told.changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    let n = told.count();
    terms.input(&s.id, "sh -c 'echo fg=$$; exec sleep 1000'\n", false).unwrap();
    let fg = until("sleep's id", || number_after(&screen.text(), "fg="));
    let _started = Started(vec![fg]);
    told.until(n, "busy says sleep", |l| l.iter().any(|x| x.id == s.id && x.busy.as_deref() == Some("sleep")));
    assert_eq!(terms.list()[0].busy.as_deref(), Some("sleep"));
    let n = told.count();
    terms.input(&s.id, "\x03", false).unwrap();
    told.until(n, "busy is null again", |l| l.iter().any(|x| x.id == s.id && x.busy.is_none()));
    assert_eq!(terms.list()[0].busy, None);
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn close_ends_the_shell_its_foreground_program_a_background_job_and_a_nohup_program_within_3_s() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let told = Told::default();
    let s = terms.open(start(&dir, "kade", None, test_shell()), told.changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    terms.input(&s.id, "echo sh=$$\n", false).unwrap();
    terms.input(&s.id, "sleep 1000 & echo bg=$!\n", false).unwrap();
    terms.input(&s.id, "nohup sleep 1000 >/dev/null 2>&1 & echo nh=$!\n", false).unwrap();
    let ids = until("the shell's, the job's and the nohup program's ids", || {
        let text = screen.text();
        Some([number_after(&text, "sh=")?, number_after(&text, "bg=")?, number_after(&text, "nh=")?])
    });
    terms.input(&s.id, "sh -c 'echo fg=$$; exec sleep 1000'\n", false).unwrap();
    let fg = until("the foreground program's id", || number_after(&screen.text(), "fg="));
    let all = [("the shell", ids[0]), ("the background job", ids[1]), ("the nohup program", ids[2]), ("the foreground program", fg)];
    let _started = Started(all.iter().map(|(_, p)| *p).collect());
    for (what, pid) in all {
        assert!(runs(pid), "{what} ({pid}) runs");
    }

    let n = told.count();
    let t = Instant::now();
    terms.close(&s.id).unwrap();
    let took = t.elapsed();
    assert!(took < Duration::from_millis(4000), "close returned after about 3 s at most, not {took:?}");
    for (what, pid) in all {
        let gone_by = Instant::now() + Duration::from_millis(500);
        while runs(pid) && Instant::now() < gone_by {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!runs(pid), "{what} ({pid}) still runs after close");
    }
    assert!(terms.list().is_empty());
    told.until(n, "the close", |l| l.is_empty());
    assert_eq!(terms.input(&s.id, "echo\n", false).unwrap_err(), "That terminal is closed");
    assert_eq!(terms.attach(&s.id, Box::new(|_| true)).unwrap_err(), "That terminal is closed");
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn exit_3_keeps_the_terminal_with_its_code_and_why_and_exit_0_leaves_the_list() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let told = Told::default();
    let open = || terms.open(start(&dir, "kade", None, test_shell()), told.changed()).unwrap();
    let (a, b, c) = (open(), open(), open());

    terms.input(&a.id, "exit 3\n", false).unwrap();
    let list = told.until(0, "a's end", |l| l.iter().any(|x| x.id == a.id && x.ended.is_some()));
    let ended = list.iter().find(|x| x.id == a.id).unwrap().ended.clone().unwrap();
    assert_eq!((ended.code, ended.why.as_deref()), (Some(3), Some("sh ended with exit code 3")));
    assert!(ended.at >= a.opened_at);
    assert!(terms.list().iter().any(|x| x.id == a.id && x.ended == Some(ended.clone())), "it stays until it is closed");
    terms.input(&a.id, "echo\n", false).unwrap();

    terms.input(&c.id, "kill -9 $$\n", false).unwrap();
    let list = told.until(0, "c's end", |l| l.iter().any(|x| x.id == c.id && x.ended.is_some()));
    let ended = list.iter().find(|x| x.id == c.id).unwrap().ended.clone().unwrap();
    assert_eq!((ended.code, ended.why.as_deref()), (None, Some("sh was ended by SIGKILL")));

    let n = told.count();
    terms.input(&b.id, "exit 0\n", false).unwrap();
    told.until(n, "b leaves the list", |l| !l.iter().any(|x| x.id == b.id));
    assert_eq!(terms.list().iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), [a.id.as_str(), c.id.as_str()]);
    assert_eq!(terms.attach(&b.id, Box::new(|_| true)).unwrap_err(), "That terminal is closed");

    terms.close(&a.id).unwrap();
    terms.close(&a.id).unwrap();
    terms.close(&c.id).unwrap();
    assert!(terms.list().is_empty());
}

#[cfg(unix)]
#[test]
fn input_takes_a_1_mb_paste_and_binary_input_one_byte_per_character() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let s = terms.open(start(&dir, "kade", None, test_shell()), Told::default().changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    terms.input(&s.id, "stty raw -echo; echo re''ady; head -c $((1024 * 1024)) | wc -c; stty sane\n", false).unwrap();
    screen.shows("ready");
    let unit = "Plakken ✓ é ünïcødé 0123456789 abcdefghijklmnopqrstuvwxyz\n";
    let mut paste = unit.repeat((1 << 20) / unit.len());
    paste.push_str(&"x".repeat((1 << 20) - paste.len()));
    assert_eq!(paste.len(), 1 << 20);
    terms.input(&s.id, &paste, false).unwrap();
    screen.shows("1048576");

    terms.input(&s.id, "stty raw -echo; echo bi''nary; head -c 4 | od -An -tx1; stty sane\n", false).unwrap();
    screen.shows("binary");
    terms.input(&s.id, "\u{1}\u{80}\u{ff}A", true).unwrap();
    // (macOS's od puts two spaces between the bytes)
    until("the four bytes, as od shows them", || screen.text().split_whitespace().collect::<Vec<_>>().join(" ").contains("01 80 ff 41").then_some(()));
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn fifty_megabytes_arrive_complete_and_in_order_through_a_slow_view_with_reading_paused_and_going_on() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc;
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let s = terms.open(start(&dir, "kade", None, test_shell()), Told::default().changed()).unwrap();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let (acked, most) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    let messages = Arc::new(AtomicU64::new(0));
    // before the attach: the shell's first prompt can come before the command is typed
    let since = Instant::now();
    {
        let (acked, most, messages) = (acked.clone(), most.clone(), messages.clone());
        let mut sent = 0u64;
        terms.attach(&s.id, Box::new(move |b| {
            messages.fetch_add(1, Ordering::SeqCst);
            sent += b.len() as u64;
            most.fetch_max(sent.saturating_sub(acked.load(Ordering::SeqCst)), Ordering::SeqCst);
            tx.send(b).is_ok()
        })).unwrap();
    }
    // The view: at first it writes nothing, until nothing more comes (reading paused); then it writes 100,000 bytes
    // every 2 ms at most and acknowledges each 100,000, as xterm.js's write callback does.
    let view = {
        let (terms, id, acked) = (terms.clone(), s.id.clone(), acked.clone());
        std::thread::spawn(move || {
            let mut all: Vec<u8> = Vec::with_capacity(56 << 20);
            let mut unacked = 0u64;
            let give_up = Instant::now() + Duration::from_secs(30);
            let paused_at = loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(b) => {
                        unacked += b.len() as u64;
                        all.extend_from_slice(&b);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) if unacked > 500_000 => break unacked,
                    Err(mpsc::RecvTimeoutError::Timeout) => assert!(Instant::now() < give_up, "output stopped at {unacked} bytes"),
                    Err(e) => panic!("the listener went: {e}"),
                }
            };
            loop {
                while unacked >= 100_000 {
                    std::thread::sleep(Duration::from_millis(2));
                    terms.ack(&id, 100_000).unwrap();
                    acked.fetch_add(100_000, Ordering::SeqCst);
                    unacked -= 100_000;
                }
                let b = rx.recv_timeout(Duration::from_secs(30)).expect("output goes on");
                unacked += b.len() as u64;
                all.extend_from_slice(&b);
                if find(&all[all.len().saturating_sub(b.len() + 16)..], b"\nEND\r\n").is_some() {
                    break (all, paused_at);
                }
            }
        })
    };
    let t = Instant::now();
    terms.input(&s.id, "echo ST''ART; awk 'BEGIN { for (i = 1; i <= 6000000; i++) print i }'; echo EN''D\n", false).unwrap();
    let (all, paused_at) = view.join().unwrap();
    let took = t.elapsed();
    // in this order: the prompt after END can still come, and then it falls in the run too
    let count = messages.load(Ordering::SeqCst);
    let run = since.elapsed();

    assert!(paused_at > 500_000 && paused_at <= 500_000 + 64 * 1024, "reading paused above 500,000 unacknowledged bytes, at {paused_at}");
    let most = most.load(Ordering::SeqCst);
    assert!(most <= 500_000 + 64 * 1024, "never more than one read past 500,000 unacknowledged bytes, but {most}");
    // (the typed command shows ST''ART and EN''D)
    let from = find(&all, b"START\r\n").unwrap_or_else(|| panic!("START in {:?}", String::from_utf8_lossy(&all[..all.len().min(600)])))
        + b"START\r\n".len();
    let to = find_last(&all, b"\nEND\r\n").expect("END") + 1;
    let body = &all[from..to];
    assert!(body.len() >= 50_000_000, "{} bytes", body.len());
    let (mut want, mut n, mut digits) = (1u64, 0u64, 0);
    for &c in body {
        match c {
            b'0'..=b'9' => {
                n = n * 10 + u64::from(c - b'0');
                digits += 1;
            }
            b'\r' => {}
            b'\n' => {
                assert!(digits > 0 && n == want, "line {want} came as {n} ({digits} digits)");
                (want, n, digits) = (want + 1, 0, 0);
            }
            _ => panic!("byte {c} after line {}", want - 1),
        }
    }
    assert_eq!(want - 1, 6_000_000, "every line, in order");
    // At most one message every 8 ms, counted over the whole run: the gap between two calls of the view says nothing on a
    // busy machine, where the sender can be held up between choosing its time and the call (GA-140).
    let limit = run.as_nanos() / Duration::from_millis(8).as_nanos() + 1;
    assert!(u128::from(count) <= limit, "at most one message every 8 ms: {count} messages in {run:?}, at most {limit}");
    eprintln!("{} bytes in {count} messages in {took:?}", body.len());
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn reading_pauses_above_500000_unacknowledged_bytes_and_goes_on_only_below_200000() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let s = terms.open(start(&dir, "kade", None, test_shell()), Told::default().changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, false);
    terms.input(&s.id, "awk 'BEGIN { for (i = 1; i <= 3000000; i++) print i }'\n", false).unwrap();
    // the view acknowledges nothing: output stops just above 500,000 bytes
    let mut last = (0, Instant::now());
    let paused = until("output stops", || {
        let len = screen.len();
        if len != last.0 {
            last = (len, Instant::now());
            return None;
        }
        (len > 500_000 && last.1.elapsed() > Duration::from_millis(500)).then_some(len)
    });
    assert!(paused <= 500_000 + 64 * 1024, "one read past 500,000 at most: {paused}");
    // 200,000 left unacknowledged: still paused
    terms.ack(&s.id, (paused - 200_000) as u32).unwrap();
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(screen.len(), paused, "still paused with 200,000 bytes unacknowledged");
    // 199,999: reading goes on
    terms.ack(&s.id, 1).unwrap();
    until("reading goes on below 200,000", || (screen.len() > paused).then_some(()));
    terms.close(&s.id).unwrap();
}

#[cfg(unix)]
#[test]
fn a_new_attach_first_gets_the_last_256_kb_then_the_live_output_and_replaces_the_old_view() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let s = terms.open(start(&dir, "kade", None, test_shell()), Told::default().changed()).unwrap();
    let old = Screen::default();
    old.attach(&terms, &s.id, true);
    terms.input(&s.id, "awk 'BEGIN { for (i = 1; i <= 100000; i++) print i }'; echo EN''D\n", false).unwrap();
    old.shows("\n100000\r\nEND\r\n");
    std::thread::sleep(Duration::from_millis(300));
    let old_bytes = old.bytes.lock().unwrap().clone();

    let new = Screen::default();
    new.attach(&terms, &s.id, true);
    let first = until("the kept output", || new.messages.lock().unwrap().first().map(|m| m.1));
    assert!((256 * 1024 - 4096..=256 * 1024).contains(&first), "the last 256 KB, from the start of a line: {first} bytes");
    let kept = new.bytes.lock().unwrap()[..first].to_vec();
    assert!(old_bytes.ends_with(&kept), "the last of what the terminal showed");
    let text = String::from_utf8_lossy(&kept).into_owned();
    assert!(text.contains("\n100000\r\nEND\r\n"));
    let lines: Vec<u64> = text.split("\r\n").take(3).map(|l| l.parse().unwrap()).collect();
    assert_eq!(lines, [lines[0], lines[0] + 1, lines[0] + 2], "whole lines");

    terms.input(&s.id, "echo li''ve\n", false).unwrap();
    let after = new.shows("live");
    assert!(after.find("live").unwrap() > first, "the live output after what was kept");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(old.len(), old_bytes.len(), "one view at a time: the old one gets nothing more");
    terms.close(&s.id).unwrap();
}

// ---- Windows ----

#[cfg(windows)]
#[test]
fn cmd_echo_hi_works_through_conpty_with_and_without_a_view() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let told = Told::default();
    let (cmd, _) = test_shell();
    // no view: Gizai answers ConPTY's first cursor-position request itself, so cmd doesn't hang
    let alone = terms.open(start(&dir, "kade", None, (cmd.clone(), vec!["/c".into(), "echo hi& exit 3".into()])), told.changed()).unwrap();
    assert_eq!(alone.shell, "cmd");
    told.until(0, "cmd /c ended with exit code 3", |l| l.iter().any(|x| x.id == alone.id && x.ended.as_ref().is_some_and(|e| e.code == Some(3))));
    // a view gets what it printed
    let s = terms.open(start(&dir, "kade", None, (cmd, vec!["/c".into(), "echo hi& exit 3".into()])), told.changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    screen.shows("hi");
    told.until(0, "the second cmd /c ended", |l| l.iter().any(|x| x.id == s.id && x.ended.is_some()));
    terms.close(&alone.id).unwrap();
    terms.close(&s.id).unwrap();
    assert!(terms.list().is_empty());
}

/// Windows: the ids of the PING.EXE processes that run now.
#[cfg(windows)]
fn pings() -> Vec<u32> {
    let out = std::process::Command::new("tasklist").args(["/FI", "IMAGENAME eq PING.EXE", "/FO", "CSV", "/NH"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| l.split(',').nth(1)?.trim_matches('"').parse().ok()).collect()
}

#[cfg(windows)]
#[test]
fn resize_and_close_return_at_once_and_close_ends_a_ping_started_from_the_shell() {
    let (_tmp, dir) = folder();
    let terms = Arc::new(Terminals::default());
    let told = Told::default();
    let before = pings();
    let s = terms.open(start(&dir, "kade", None, test_shell()), told.changed()).unwrap();
    let screen = Screen::default();
    screen.attach(&terms, &s.id, true);
    terms.input(&s.id, "ping -t 127.0.0.1\r\n", false).unwrap();
    told.until(0, "busy says ping", |l| l.iter().any(|x| x.id == s.id && x.busy.as_deref().is_some_and(|b| b.eq_ignore_ascii_case("ping"))));
    let mine: Vec<u32> = until("ping runs", || Some(pings().into_iter().filter(|p| !before.contains(p)).collect::<Vec<_>>()).filter(|p| !p.is_empty()));

    let t = Instant::now();
    terms.resize(&s.id, 120, 40).unwrap();
    assert!(t.elapsed() < Duration::from_millis(500), "resize took {:?}", t.elapsed());
    let t = Instant::now();
    terms.close(&s.id).unwrap();
    assert!(t.elapsed() < Duration::from_millis(1000), "close took {:?}", t.elapsed());
    until("ping ended with the terminal's Job Object", || (!pings().iter().any(|p| mine.contains(p))).then_some(()));
    assert!(terms.list().is_empty());
}
