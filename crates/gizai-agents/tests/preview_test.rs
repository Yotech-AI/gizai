// Linux and macOS only: the previews here are bash and node commands, and the checks use process groups.
#![cfg(unix)]
// GA-182: a project's preview, as `gizai_agents::preview` serves a card's branch: a free block of ten ports, the Preview
// command in the worktree with its environment and its log, the wait until its port answers (or not), and its whole tree
// ended with the run. The previews are small node servers (`node -e`), never a project's real one.
use std::fs;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gizai_agents::preview::{self, Preview};

/// Gizai itself may run inside Claude Code: the preview must not see CLAUDECODE (Astro detaches its server when it does).
/// Set once, the same for every test here, before any of them starts a preview.
fn claudecode_set() {
    static SET: OnceLock<()> = OnceLock::new();
    SET.get_or_init(|| {
        // SAFETY: set once (OnceLock), before any test in this file starts a process, to the same value for all of them.
        unsafe { std::env::set_var("CLAUDECODE", "1") };
    });
}

/// A worktree with its temp folder, as a run has it when its preview starts.
fn worktree() -> (tempfile::TempDir, PathBuf) {
    claudecode_set();
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("wt");
    fs::create_dir_all(wt.join(gizai_agents::worktree::TEMP_DIR)).unwrap();
    (tmp, wt)
}

/// A port block of this test's own, away from the default Preview ports and from the other tests here.
fn port(base: u16) -> u16 {
    preview::free_port(base, base + 99, &[]).expect("a free block")
}

/// A node server on GIZAI_PREVIEW_PORT after `delay_ms`, which first writes its environment, its folder, its pid and
/// its process group to `<out>`, says "listening" on stdout, and (with `stubborn`) ignores SIGTERM.
fn server(out: &Path, delay_ms: u64, stubborn: bool) -> String {
    let js = format!(
        "const fs=require('fs'),e=process.env;{ignore}\
         fs.writeFileSync({out:?},JSON.stringify({{port:e.GIZAI_PREVIEW_PORT,PORT:e.PORT,url:e.GIZAI_PREVIEW_URL,dir:e.GIZAI_PREVIEW_DIR,\
         claudecode:e.CLAUDECODE||null,cwd:process.cwd(),pid:process.pid}}));\
         setTimeout(()=>require('http').createServer((q,s)=>s.end('ok')).listen(+e.GIZAI_PREVIEW_PORT,'127.0.0.1',\
         ()=>console.log('listening on '+e.GIZAI_PREVIEW_PORT)),{delay_ms});",
        ignore = if stubborn { "process.on('SIGTERM',()=>console.log('ignoring SIGTERM'));" } else { "" },
        out = out.display().to_string(),
    );
    // a child of bash, so its tree has two processes; `wait` keeps bash there
    format!("echo starting the preview; node -e \"{}\" & wait", js.replace('"', "\\\""))
}

fn read_json(path: &Path) -> serde_json::Value {
    let t0 = Instant::now();
    loop {
        if let Ok(s) = fs::read_to_string(path) && let Ok(v) = serde_json::from_str(&s) {
            return v;
        }
        assert!(t0.elapsed() < Duration::from_secs(20), "{} wasn't written", path.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn alive(pid: i64) -> bool {
    // SAFETY: signal 0 only checks that the process exists; nothing is sent.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Waits a moment for `pid` to be gone (its parent, init, reaps it).
fn gone(pid: i64) -> bool {
    let t0 = Instant::now();
    while alive(pid) && t0.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    !alive(pid)
}

fn start(command: &str, wt: &Path, port: u16) -> Preview {
    preview::start(command, wt, port, None, None).unwrap()
}

#[test]
fn the_address_and_the_environment_of_the_command_and_the_agent() {
    let wt = Path::new("/work/GA-182");
    assert_eq!(preview::url(47130), "http://127.0.0.1:47130");
    assert_eq!(preview::dir(wt), wt.join(".gizai-tmp/preview"));
    assert_eq!(preview::log_path(wt), wt.join(".gizai-tmp/preview/preview.log"));
    let agent = preview::agent_env(47130, wt);
    let names: Vec<&str> = agent.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, ["GIZAI_PREVIEW_PORT", "GIZAI_PREVIEW_URL", "GIZAI_PREVIEW_DIR"], "the agent gets no PORT");
    assert_eq!(agent[0].1, "47130");
    assert_eq!(agent[1].1, "http://127.0.0.1:47130");
    assert_eq!(agent[2].1, wt.join(".gizai-tmp/preview").display().to_string());
    let command = preview::command_env(47130, wt);
    assert!(command.contains(&("PORT".to_string(), "47130".to_string())), "{command:?}");
    assert_eq!(command.len(), 4);
}

#[test]
fn free_port_takes_a_block_of_ten_none_of_another_runs_and_checks_its_first_port() {
    let from = port(48_300);
    let first = preview::free_port(from, from + 99, &[]).unwrap();
    assert_eq!(first, from);
    // another live run's block, and one that overlaps it, are skipped a whole block at a time
    assert_eq!(preview::free_port(from, from + 99, &[from]), Some(from + 10));
    assert_eq!(preview::free_port(from, from + 99, &[from + 5]), Some(from + 20), "a block overlapping from+5..from+14 is taken");
    // a port something else listens on: its block is skipped
    let held = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, from))).unwrap();
    assert_eq!(preview::free_port(from, from + 99, &[]), Some(from + 10));
    drop(held);
    // three runs at once: three blocks, ten apart
    let mut taken = vec![];
    for _ in 0..3 {
        taken.push(preview::free_port(from, from + 99, &taken).unwrap());
    }
    assert_eq!(taken, [from, from + 10, from + 20]);
    // no room for a block of ten
    assert_eq!(preview::free_port(from, from + 8, &[]), None);
    assert_eq!(preview::free_port(from, from + 19, &[from, from + 10]), None);
    // the top of the range: 65526–65535 is a whole block
    assert!(matches!(preview::free_port(65_526, 65_535, &[]), Some(65_526) | None));
}

#[tokio::test]
async fn the_command_runs_in_the_worktree_with_its_ports_and_log_and_the_wait_ends_when_it_answers() {
    let (tmp, wt) = worktree();
    let out = tmp.path().join("server.json");
    let port = port(48_400);
    let mut p = start(&server(&out, 1_500, false), &wt, port);
    let t0 = Instant::now();
    p.wait_ready(Duration::from_secs(30), || false).await.unwrap();
    assert!(t0.elapsed() >= Duration::from_millis(1_400), "it waited for the port: {:?}", t0.elapsed());
    assert!(preview::answers(port));
    let env = read_json(&out);
    assert_eq!(env["port"], port.to_string());
    assert_eq!(env["PORT"], port.to_string());
    assert_eq!(env["url"], format!("http://127.0.0.1:{port}"));
    assert_eq!(env["dir"], wt.join(".gizai-tmp/preview").display().to_string());
    assert_eq!(env["claudecode"], serde_json::Value::Null, "no CLAUDECODE");
    assert_eq!(fs::canonicalize(env["cwd"].as_str().unwrap()).unwrap(), fs::canonicalize(&wt).unwrap());
    assert_eq!(p.port, port);
    assert_eq!(p.log, wt.join(".gizai-tmp/preview/preview.log"));
    let log = fs::read_to_string(&p.log).unwrap();
    assert!(log.contains("starting the preview") && log.contains(&format!("listening on {port}")), "{log}");
    let pid = env["pid"].as_i64().unwrap();
    assert!(p.running());
    p.stop().await;
    assert!(gone(pid), "the server is gone after stop");
    assert!(!preview::answers(port), "nothing answers on its port");
}

#[tokio::test]
async fn its_stdin_is_closed() {
    let (tmp, wt) = worktree();
    let out = tmp.path().join("stdin.txt");
    let mut p = start(&format!("read -r line; echo \"read=$? line=$line\" > {out:?}; sleep 600"), &wt, port(48_500));
    let t0 = Instant::now();
    while !out.is_file() && t0.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(fs::read_to_string(&out).unwrap().trim(), "read=1 line=", "read got the end of its input at once");
    assert!(p.running());
    p.stop().await;
}

#[tokio::test]
async fn one_that_never_answers_lets_the_run_go_on_after_the_start_time_and_is_ended() {
    let (tmp, wt) = worktree();
    let pid_file = tmp.path().join("sleep.pid");
    let port = port(48_600);
    let mut p = start(&format!("echo still building; echo $$ > {pid_file:?}; exec sleep 600"), &wt, port);
    let t0 = Instant::now();
    let err = p.wait_ready(Duration::from_secs(2), || false).await.unwrap_err();
    let waited = t0.elapsed();
    assert_eq!(err, format!("It didn't answer on port {port} in 2 s"));
    assert!(waited >= Duration::from_secs(2) && waited < Duration::from_secs(6), "{waited:?}");
    assert_eq!(preview::log_tail(&wt), "still building");
    let pid: i64 = fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
    assert!(alive(pid));
    p.stop().await;
    assert!(gone(pid), "the never-answering preview is gone");
}

#[tokio::test]
async fn one_that_ends_before_it_answers_says_its_exit_code_and_its_log_says_why() {
    let (_tmp, wt) = worktree();
    let port = port(48_700);
    let mut p = start("echo one; echo; echo 'Error: no such script \"preview\"' >&2; exit 3", &wt, port);
    let err = p.wait_ready(Duration::from_secs(20), || false).await.unwrap_err();
    assert_eq!(err, format!("It ended before it answered on port {port} (exit code 3)"));
    // its stdout and stderr both, without the empty line
    assert_eq!(preview::log_tail(&wt), "one\nError: no such script \"preview\"");
    p.stop().await;
}

#[tokio::test]
async fn gizai_quitting_ends_the_wait() {
    let (_tmp, wt) = worktree();
    let mut p = start("exec sleep 600", &wt, port(48_800));
    let t0 = Instant::now();
    let err = p.wait_ready(Duration::from_secs(60), || true).await.unwrap_err();
    assert_eq!(err, "Gizai is quitting");
    assert!(t0.elapsed() < Duration::from_secs(5));
    p.stop().await;
}

#[tokio::test]
async fn stop_ends_one_that_ignores_sigterm_with_sigkill_after_three_seconds() {
    let (tmp, wt) = worktree();
    let out = tmp.path().join("server.json");
    let port = port(48_900);
    let mut p = start(&format!("trap '' TERM; {}", server(&out, 0, true)), &wt, port);
    p.wait_ready(Duration::from_secs(30), || false).await.unwrap();
    let pid = read_json(&out)["pid"].as_i64().unwrap();
    let t0 = Instant::now();
    p.stop().await;
    let took = t0.elapsed();
    assert!(took >= Duration::from_millis(2_900) && took < Duration::from_secs(8), "SIGKILL came after the grace: {took:?}");
    assert!(gone(pid));
    assert!(!preview::answers(port));
}

#[tokio::test]
async fn a_preview_dropped_without_stop_is_killed() {
    let (tmp, wt) = worktree();
    let out = tmp.path().join("server.json");
    let port = port(49_000);
    let mut p = start(&server(&out, 0, false), &wt, port);
    p.wait_ready(Duration::from_secs(30), || false).await.unwrap();
    let pid = read_json(&out)["pid"].as_i64().unwrap();
    drop(p);
    assert!(gone(pid), "a preview never outlives its run");
}

#[tokio::test]
async fn three_previews_at_once_get_their_own_ports_and_all_answer() {
    let (tmp, wt) = worktree();
    let from = port(49_100);
    let mut taken = vec![];
    let mut previews = vec![];
    for i in 0..3 {
        let port = preview::free_port(from, from + 99, &taken).unwrap();
        taken.push(port);
        let w = wt.join(format!("run{i}"));
        fs::create_dir_all(w.join(".gizai-tmp")).unwrap();
        let out = tmp.path().join(format!("server{i}.json"));
        previews.push((start(&server(&out, 300, false), &w, port), out));
    }
    let mut pids = vec![];
    for (p, out) in previews.iter_mut() {
        p.wait_ready(Duration::from_secs(30), || false).await.unwrap();
        let env = read_json(out);
        assert_eq!(env["port"], p.port.to_string());
        pids.push(env["pid"].as_i64().unwrap());
    }
    assert!(taken[1] - taken[0] >= 10 && taken[2] - taken[1] >= 10, "{taken:?}");
    for (p, _) in previews {
        p.stop().await;
    }
    for (pid, port) in pids.iter().zip(&taken) {
        assert!(gone(*pid));
        assert!(!preview::answers(*port));
    }
}

#[test]
fn log_tail_gives_the_last_ten_lines_without_colours_or_empty_lines() {
    let (_tmp, wt) = worktree();
    assert_eq!(preview::log_tail(&wt), "", "no log yet");
    fs::create_dir_all(preview::dir(&wt)).unwrap();
    let lines: Vec<String> = (1..=15).map(|n| format!("\u{1b}[31mline {n}\u{1b}[0m\n")).collect();
    fs::write(preview::log_path(&wt), lines.concat() + "\n\n").unwrap();
    let want: Vec<String> = (6..=15).map(|n| format!("line {n}")).collect();
    assert_eq!(preview::log_tail(&wt), want.join("\n"));
}
