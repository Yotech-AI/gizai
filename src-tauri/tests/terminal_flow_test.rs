// GA-133: terminals end to end (contract CT). A terminal starts only in one of the Editor's folders: a folder the Editor
// refuses is refused with the Editor's own sentence, and a card's worktree an agent works on now with who it is, while
// one opened there before stays open. The shell starts in the folder with the PATH runs get; the window hears every
// change (terminals-changed, the whole list); nothing a terminal shows or is sent goes under Gizai's data folder; and
// quitting Gizai ends every terminal with everything it started. The commands only call these functions. Tests that
// type use /bin/sh, never your own shell.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gizai_agents::terminal::TerminalSession;
use gizai_core::editor::Kind;
use gizai_core::model::ProjectInput;
use gizai_core::projects;
use gizai_lib::{AppState, editor, runs, terminal};

/// A temp folder by its real path, the way git and Gizai report it.
fn real_tempdir() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).current_dir(dir).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A repository at `dir` on main, with README.md committed.
fn git_repo(dir: &Path) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "# Kade\n").unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
    dir.to_path_buf()
}

/// Project KADE linked to a new repository `repo` in `tmp`: one of the Editor's folders. Its id.
fn linked_folder(st: &AppState, tmp: &Path) -> String {
    let repo = git_repo(&tmp.join("repo"));
    projects::create(&st.db, &st.you_id, ProjectInput { name: "Kade".into(), key: "KADE".into(), repo_path: Some(repo.display().to_string()),
        ..Default::default() }).unwrap();
    let f = editor::folders(st).unwrap().into_iter().find(|f| f.kind == Kind::Project).expect("the linked folder");
    assert_eq!(f.id, repo.display().to_string());
    f.id
}

/// The shell the tests type into: /bin/sh, or cmd.exe on Windows.
fn test_shell() -> (PathBuf, Vec<OsString>) {
    if cfg!(windows) {
        (PathBuf::from(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into())), vec![])
    } else {
        (PathBuf::from("/bin/sh"), vec![])
    }
}

/// What terminals-changed got, in order. Set before a terminal opens: it keeps the notifier it opened with.
fn listen(st: &mut AppState) -> Arc<Mutex<Vec<Vec<TerminalSession>>>> {
    let got: Arc<Mutex<Vec<Vec<TerminalSession>>>> = Arc::default();
    let seen = got.clone();
    st.notify = Arc::new(move |n| {
        if let runs::Note::TerminalsChanged(list) = n {
            seen.lock().unwrap().push(list);
        }
    });
    got
}

/// Waits up to 20 s for `ok`.
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

/// Attaches a view that keeps what it gets and acknowledges it, as the window does.
#[cfg(unix)]
fn view(st: &AppState, id: &str) -> Arc<Mutex<Vec<u8>>> {
    let got: Arc<Mutex<Vec<u8>>> = Arc::default();
    let (seen, terms, id2) = (got.clone(), st.terminals.clone(), id.to_string());
    terminal::attach(st, id, Box::new(move |b| {
        let _ = terms.ack(&id2, b.len() as u32);
        seen.lock().unwrap().extend_from_slice(&b);
        true
    })).unwrap();
    got
}

/// Waits until the view shows `want`; returns all it got.
#[cfg(unix)]
fn shows(got: &Arc<Mutex<Vec<u8>>>, want: &str) -> String {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let text = String::from_utf8_lossy(&got.lock().unwrap()).into_owned();
        if text.contains(want) {
            return text;
        }
        assert!(Instant::now() < end, "never showed {want:?}; got {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The number printed right after `tag` ("bg=1234"), skipping where the typed command shows it ("bg=$!").
#[cfg(unix)]
fn number_after(text: &str, tag: &str) -> Option<u32> {
    text.match_indices(tag).find_map(|(at, _)| text[at + tag.len()..].chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok())
}

/// Linux and macOS: the process still runs (a zombie has ended).
#[cfg(unix)]
fn runs_now(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else { return false };
        return stat.rfind(')').and_then(|i| stat[i + 1..].split_whitespace().next()).is_some_and(|s| s != "Z" && s != "X");
    }
    // SAFETY: signal 0 only asks whether the process is there.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// Linux and macOS: programs a test started in a terminal, by their exact ids, ended if the test fails first.
#[cfg(unix)]
struct Started(Vec<u32>);

#[cfg(unix)]
impl Drop for Started {
    fn drop(&mut self) {
        for &pid in &self.0 {
            if runs_now(pid) {
                // SAFETY: a plain kill of a process this test started.
                unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
            }
        }
    }
}

/// Every file under `dir`, with what is in it.
fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut all = vec![];
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => todo.push(p),
                Ok(t) if t.is_file() => all.push((p.clone(), std::fs::read(&p).unwrap_or_default())),
                _ => {}
            }
        }
    }
    all.sort();
    all
}

#[test]
fn a_folder_the_editor_refuses_is_refused_with_its_sentence_and_nothing_starts() {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let told = listen(&mut st);
    linked_folder(&st, tmp.path());
    let home = if cfg!(windows) { std::env::home_dir() } else { std::env::var_os("HOME").map(PathBuf::from) }.unwrap();
    let elsewhere = tmp.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    for id in [home.join(".ssh").display().to_string(), elsewhere.display().to_string(), st.data_dir.display().to_string(), String::new()] {
        let refused = editor::folder(&st, &id).unwrap_err();
        let e = terminal::open_with(&st, &id, 80, 24, None, Some(test_shell())).unwrap_err();
        assert_eq!(e, refused, "{id}: the Editor's own sentence");
        assert!(e.starts_with("The Editor can't open") || e == "Pick one of the Editor's folders to open", "{e}");
    }
    let ssh = terminal::open_with(&st, &home.join(".ssh").display().to_string(), 80, 24, None, Some(test_shell())).unwrap_err();
    assert!(ssh.starts_with("The Editor can't open ~") && ssh.contains(".ssh: "), "{ssh}");
    assert!(st.terminals.list().is_empty(), "nothing started");
    assert!(told.lock().unwrap().is_empty(), "nothing to tell");
}

#[cfg(unix)]
#[test]
fn open_attach_input_close_and_the_window_hears_of_each() {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let told = listen(&mut st);
    let folder = linked_folder(&st, tmp.path());
    let t0 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    let s = terminal::open_with(&st, &folder, 100, 30, None, Some(test_shell())).unwrap();
    assert_eq!((s.name.as_str(), s.folder.as_str(), s.task_id.as_deref(), s.shell.as_str(), s.cols, s.rows, &s.busy, &s.ended),
               ("repo", folder.as_str(), None, "sh", 100, 30, &None, &None));
    assert_eq!(s.folder_path, editor::folder(&st, &folder).unwrap().path, "the folder as the Editor shows it");
    assert!(s.opened_at >= t0, "{} {t0}", s.opened_at);
    assert_eq!(st.terminals.list(), [s.clone()]);
    assert_eq!(told.lock().unwrap().last().cloned(), Some(vec![s.clone()]), "open tells the whole list");

    // a second one in the same folder: the folder's name, numbered
    let second = terminal::open_with(&st, &folder, 80, 24, Some(""), Some(test_shell())).unwrap();
    assert_eq!(second.name, "repo 2");
    assert_eq!(told.lock().unwrap().last().unwrap().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["repo", "repo 2"]);

    let got = view(&st, &s.id);
    st.terminals.input(&s.id, "echo \"in=$(pwd)\"; echo \"path=$PATH\"\n", false).unwrap();
    let text = shows(&got, &format!("in={folder}"));
    let own = std::env::var("PATH").unwrap();
    let text = if text.contains(&format!("path={own}")) { text } else { shows(&got, &format!("path={own}")) };
    assert!(text.contains(&format!("path={own}")), "the PATH runs get: Gizai's own first");

    let r = st.terminals.rename(&second.id, "build").unwrap();
    assert_eq!(r.name, "build");
    assert_eq!(told.lock().unwrap().last().unwrap().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["repo", "build"]);

    st.terminals.close(&s.id).unwrap();
    assert_eq!(st.terminals.list().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["build"]);
    assert_eq!(told.lock().unwrap().last().unwrap().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["build"], "close tells the list");
    st.terminals.close(&second.id).unwrap();
    assert_eq!(told.lock().unwrap().last().cloned(), Some(vec![]));
    assert_eq!(st.terminals.input(&s.id, "x", false).unwrap_err(), "That terminal is closed");
}

#[cfg(unix)]
#[test]
fn quitting_gizai_ends_every_terminal_and_everything_it_started() {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let told = listen(&mut st);
    let folder = linked_folder(&st, tmp.path());
    let mut all = vec![];
    for _ in 0..2 {
        let s = terminal::open_with(&st, &folder, 80, 24, None, Some(test_shell())).unwrap();
        let got = view(&st, &s.id);
        st.terminals.input(&s.id, "echo sh=$$; sleep 1000 & echo bg=$!; nohup sleep 1000 >/dev/null 2>&1 & echo nh=$!\n", false).unwrap();
        let ids = until("the ids", || {
            let text = String::from_utf8_lossy(&got.lock().unwrap()).into_owned();
            Some([number_after(&text, "sh=")?, number_after(&text, "bg=")?, number_after(&text, "nh=")?])
        });
        all.extend(ids);
    }
    let _started = Started(all.clone());
    assert!(all.iter().all(|&p| runs_now(p)));

    let t = Instant::now();
    terminal::end_all(&st);
    let took = t.elapsed();
    assert!(took < Duration::from_millis(4000), "all at the same time, in about 3 s, not {took:?}");
    for pid in &all {
        let gone_by = Instant::now() + Duration::from_millis(500);
        while runs_now(*pid) && Instant::now() < gone_by {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!runs_now(*pid), "{pid} outlived Gizai");
    }
    assert!(st.terminals.list().is_empty());
    assert_eq!(told.lock().unwrap().last().cloned(), Some(vec![]));
}

#[cfg(unix)]
#[test]
fn nothing_a_terminal_shows_or_is_sent_goes_under_gizais_data_folder() {
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let _told = listen(&mut st);
    let folder = linked_folder(&st, tmp.path());
    let before: Vec<PathBuf> = files_under(&st.data_dir).into_iter().map(|(p, _)| p).collect();
    let s = terminal::open_with(&st, &folder, 80, 24, Some("ga133-secret-name"), Some(test_shell())).unwrap();
    let got = view(&st, &s.id);
    st.terminals.input(&s.id, "echo ga133-typed-$((6*7))-shown\n", false).unwrap();
    shows(&got, "ga133-typed-42-shown");
    st.terminals.rename(&s.id, "ga133-renamed").unwrap();
    st.terminals.input(&s.id, "exit 3\n", false).unwrap();
    until("the shell's end", || st.terminals.list().first().and_then(|s| s.ended.clone()));
    st.terminals.close(&s.id).unwrap();

    let after = files_under(&st.data_dir);
    let paths: Vec<PathBuf> = after.iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(paths, before, "no file was added under the data folder");
    for (p, bytes) in &after {
        let text = String::from_utf8_lossy(bytes);
        for mark in ["ga133-typed", "ga133-secret-name", "ga133-renamed", "exit code 3"] {
            assert!(!text.contains(mark), "{} has {mark:?}", p.display());
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_cards_worktree_an_agent_works_on_is_refused_and_a_terminal_opened_there_before_stays_open() {
    use gizai_core::model::TaskPatch;
    use gizai_core::{settings, tasks, team};
    const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude.sh");
    let tmp = real_tempdir();
    let mut st = gizai_lib::test_state(tmp.path());
    let told = listen(&mut st);
    settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    let repo = git_repo(&tmp.path().join("repo"));
    let card = gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    tasks::update(&st.db, &st.you_id, &card, TaskPatch { description_md: Some("FAKE_HANG".into()), ..Default::default() }).unwrap();
    let identifier = tasks::get(&st.db, &card).unwrap().identifier;
    let team = team::get(&st.db, &team::list(&st.db).unwrap()[0].id).unwrap();
    let be = team.members.iter().find(|m| m.name == "Backend Agent").unwrap().actor_id.clone();
    let worktree = |st: &AppState| editor::folders(st).unwrap().into_iter().find(|f| f.kind == Kind::Card);

    // while the agent works there: refused, and nothing starts
    let (run, finished) = runs::start(&st, &card, Some(be.clone()), Some(FAKE.into()), "manual").await.unwrap();
    let wt = until("the card's worktree, read-only", || worktree(&st).filter(|f| f.read_only));
    let e = terminal::open_with(&st, &wt.id, 80, 24, None, Some(test_shell())).unwrap_err();
    assert_eq!(e, format!("Backend Agent is working on {identifier}: a terminal can start there when the run ends"));
    assert!(st.terminals.list().is_empty() && told.lock().unwrap().is_empty(), "nothing started");
    runs::stop(&st, &run);
    finished.await.unwrap();

    // after the run: it starts, named after the card
    let s = terminal::open_with(&st, &wt.id, 80, 24, None, Some(test_shell())).unwrap();
    assert_eq!((s.name.as_str(), s.task_id.as_deref(), s.folder.as_str()), (identifier.as_str(), Some(card.as_str()), wt.id.as_str()));

    // an agent starts there again: the terminal opened before stays open, and works
    let (run, finished) = runs::start(&st, &card, Some(be), Some(FAKE.into()), "manual").await.unwrap();
    until("the card's worktree, read-only again", || worktree(&st).filter(|f| f.read_only));
    let got = view(&st, &s.id);
    st.terminals.input(&s.id, "echo still-$((40+2))\n", false).unwrap();
    shows(&got, "still-42");
    assert_eq!(st.terminals.list().iter().map(|x| (x.id.clone(), x.ended.clone())).collect::<Vec<_>>(), [(s.id.clone(), None)]);
    runs::stop(&st, &run);
    finished.await.unwrap();
    st.terminals.close(&s.id).unwrap();
    runs::stop_all(&st, Duration::from_secs(10)).await;
}
