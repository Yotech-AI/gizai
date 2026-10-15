// Linux and macOS only: these tests run shell scripts as fake programs, which Windows can't start.
#![cfg(unix)]
// GA-182: a task run's screenshots end to end. A run whose agent has the browser on (with take_screenshot), its Take
// screenshots on, on a card (else project) with screenshots on, in a project with a Preview command, gets the project's
// preview served on a free block of ports, the Screenshots section in its prompt and GIZAI_PREVIEW_* in its environment;
// at its end the preview's whole tree ends and the pictures it saved go onto the card. With fake Claude Codes
// (fake-claude-shots.sh), a fake npx and a stand-in browser program that never runs, and a small node server as the
// project's preview: never the real Claude Code, npx, Chrome DevTools MCP or a browser.
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gizai_agents::stream::RunEvent;
use gizai_core::clis::Cli;
use gizai_core::mcp_servers::{self as core_mcp, AgentServer, AgentTools, BrowserEntry};
use gizai_core::model::{AgentInput, ProjectInput, TaskPatch};
use gizai_core::screenshots::{self as core_shots, Limits, ProjectScreenshots, ScreenshotSet};
use gizai_lib::AppState;
use gizai_lib::mcp_servers as app_mcp;
use gizai_lib::runs::RunSummary;
use serde_json::Value;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-claude-shots.sh");
const FAKE_NPX: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../crates/gizai-agents/tests/fake-npx-browser.sh");

/// The two PNGs the fake saves: their signature and IHDR header only (32 x 20 and 24 x 48).
const PNG_1: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x20\x00\x00\x00\x14\x08\x06\x00\x00\x00\x00\x00\x00\x00";

/// The project's preview for these tests: a node server on GIZAI_PREVIEW_PORT that answers with its GIZAI_PREVIEW_DIR,
/// with a child of its own (sleep), so the test can see that the preview's whole tree ends. Before it listens it writes
/// what it got to <info dir>/preview-<port>.json (outside the worktree). With "stubborn" it ignores SIGTERM. With "shots"
/// it first saves two PNGs and a
/// captions.json in the run's screenshots folder itself (for a run that is stopped before its agent can). Saved as
/// server.cjs: a package.json around the test's temp folder may say "type": "module".
const SERVER_JS: &str = r#"
const fs = require('fs'), http = require('http'), path = require('path'), cp = require('child_process');
const [infoDir, mode] = process.argv.slice(2);
const port = Number(process.env.GIZAI_PREVIEW_PORT);
const child = cp.spawn('sleep', ['600'], { stdio: 'ignore' });
if (mode === 'stubborn') process.on('SIGTERM', () => console.log('not ending'));
if (mode === 'shots') {
  const dir = path.join(process.env.GIZAI_PREVIEW_DIR, '..', 'screenshots');
  fs.mkdirSync(dir, { recursive: true });
  const u32 = (n) => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
  const png = (w, h) => Buffer.concat([Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13]), Buffer.from('IHDR'),
                                       u32(w), u32(h), Buffer.from([8, 6, 0, 0, 0, 0, 0, 0, 0])]);
  fs.writeFileSync(path.join(dir, '01-board-desktop.png'), png(32, 20));
  fs.writeFileSync(path.join(dir, '02-card-mobile.png'), png(24, 48));
  fs.writeFileSync(path.join(dir, 'captions.json'), JSON.stringify({ '01-board-desktop.png': 'The board' }));
}
const info = {
  pid: process.pid, child: child.pid, port: process.env.PORT ?? null, previewPort: process.env.GIZAI_PREVIEW_PORT ?? null,
  url: process.env.GIZAI_PREVIEW_URL ?? null, dir: process.env.GIZAI_PREVIEW_DIR ?? null, claudecode: process.env.CLAUDECODE ?? null,
  cwd: process.cwd(),
};
const file = path.join(infoDir, 'preview-' + port + '.json');
fs.writeFileSync(file + '.part', JSON.stringify(info));
fs.renameSync(file + '.part', file);
http.createServer((q, s) => s.end(process.env.GIZAI_PREVIEW_DIR || '')).listen(port, '127.0.0.1', () => console.log('preview listening on ' + port));
"#;

/// The browser's server starts through GIZAI_NPX, the fake npx: set once, the same for every test here, before any of
/// them starts a run. The fakes are made runnable too.
fn fakes() {
    static SET: OnceLock<()> = OnceLock::new();
    SET.get_or_init(|| {
        use std::os::unix::fs::PermissionsExt;
        for f in [FAKE, FAKE_NPX] {
            std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        // SAFETY: set once (OnceLock), before any test in this file goes on, to the same value for all of them.
        unsafe { std::env::set_var("GIZAI_NPX", FAKE_NPX) };
    });
}

/// node, found on this process's PATH (the preview command names it by its full path: the login PATH it gets may not
/// have it).
fn node() -> PathBuf {
    std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join("node")).find(|n| n.is_file()))
        .expect("these tests need node on PATH")
}

fn git_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir(&repo).unwrap();
    for a in [&["init", "-q", "-b", "main"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "i"][..]] {
        assert!(std::process::Command::new("git").args(a).current_dir(&repo).status().unwrap().success());
    }
    repo
}

/// A stand-in browser program (a script nothing here starts): the program Settings passes with --executablePath.
fn stand_in(path: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_path_buf()
}

fn add_cli(st: &AppState, name: &str, command: &str, env: &[String]) -> String {
    let mut list: Vec<Cli> = gizai_core::clis::list(&st.db).unwrap();
    list.push(Cli { name: name.into(), kind: "claude_code".into(), command: command.into(), env: env.to_vec(), ..Default::default() });
    gizai_lib::clis::save(st, list).unwrap().into_iter().find(|c| c.cli.name == name).unwrap().cli.id
}

/// Puts the backend agent on the CLI `cli` (its other settings as a plain form save) and returns its id.
fn put_agent_on(st: &AppState, cli: &str) -> String {
    let (_, agent) = gizai_core::team::all_agents(&st.db).unwrap().into_iter().find(|(_, m)| m.role_key == "backend").unwrap();
    gizai_core::team::update_agent(&st.db, &st.you_id, &agent.actor_id, AgentInput { name: agent.name.clone(), role_key: "backend".into(),
        adapter: cli.into(), ..Default::default() }).unwrap();
    agent.actor_id
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn notes(st: &AppState, run_id: &str) -> Vec<String> {
    gizai_lib::runs::events_for(st, run_id).into_iter().filter_map(|e| match e.event { RunEvent::Note { text } => Some(text), _ => None }).collect()
}

/// Whether the process `pid` still runs: one that has ended but waits to be reaped (a zombie) has ended.
fn alive(pid: i64) -> bool {
    // SAFETY: kill with signal 0 only asks whether the process exists.
    if unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
        return false;
    }
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(s) => !s.rsplit_once(')').is_some_and(|(_, rest)| rest.trim_start().starts_with('Z')),
            Err(_) => false,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Whether something answers on 127.0.0.1:`port` now.
fn answers(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(1)).is_ok()
}

/// The names in a folder (none when it is missing).
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

async fn until(what: &str, secs: u64, mut ok: impl FnMut() -> bool) {
    let t0 = Instant::now();
    while !ok() {
        assert!(t0.elapsed() < Duration::from_secs(secs), "waited {secs} s for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

struct T {
    tmp: tempfile::TempDir,
    st: AppState,
    agent: String,
    repo: PathBuf,
    out: PathBuf,
    project: String,
    /// Where the preview writes what it got (`SERVER_JS`).
    info: PathBuf,
}

/// What one run's fake Claude Code got.
struct Got {
    run_id: String,
    /// The card's worktree.
    wt: PathBuf,
    prompt: String,
    env: HashMap<String, String>,
    /// Whether the preview's port answered when it started.
    answered: Option<String>,
    /// What GET / on that port answered.
    answer: Option<String>,
    /// The preview's log as it was when it started.
    preview_log: Option<String>,
    /// It saved its screenshots.
    shot: bool,
}

impl Got {
    fn port(&self) -> u16 {
        self.env.get("GIZAI_PREVIEW_PORT").unwrap_or_else(|| panic!("no GIZAI_PREVIEW_PORT in {:?}", self.env)).parse().unwrap()
    }
}

/// The backend agent on a fake Claude Code (fake-claude-shots.sh) with the browser on (its program a stand-in) and Take
/// screenshots on, in project KADE (no Preview command yet). Its runs' previews get ports `ports` (None: the default
/// 47100–47999), so tests running at once don't race for the same block.
fn setup(ports: Option<(u32, u32)>) -> T {
    fakes();
    let tmp = tempfile::tempdir().unwrap();
    let st = gizai_lib::test_state(tmp.path());
    let repo = git_repo(tmp.path());
    gizai_core::settings::set(&st.db, "claude_bin", &FAKE.to_string()).unwrap();
    // only the runs a test starts
    gizai_core::settings::set(&st.db, "agents_paused", &true).unwrap();
    gizai_lib::test_task(&st, repo.to_str().unwrap(), "backend");
    let out = tmp.path().join("fake-out");
    let cli = add_cli(&st, "Claude Code (shots)", FAKE, &[format!("FAKE_MCP_OUT={}", out.display())]);
    let agent = put_agent_on(&st, &cli);
    let program = stand_in(&tmp.path().join("bin/chromium"));
    app_mcp::save_browser(&st, BrowserEntry { version: "1.10.1".into(), program: program.display().to_string() }).unwrap();
    let project = gizai_core::projects::list(&st.db).unwrap().into_iter().find(|p| p.key == "KADE").unwrap().id;
    let info = tmp.path().join("preview-info");
    std::fs::create_dir_all(&info).unwrap();
    std::fs::write(tmp.path().join("server.cjs"), SERVER_JS).unwrap();
    let t = T { tmp, st, agent, repo, out, project, info };
    t.browser(true, &[]);
    // a backend agent's Take screenshots is off until it is set
    t.agent_screenshots(true);
    if let Some((from, to)) = ports {
        core_shots::save_limits(&t.st.db, &Limits { port_from: from, port_to: to, ..Default::default() }, 3).unwrap();
    }
    t
}

impl T {
    /// A new card for the backend agent, with `description` (the fake's FAKE_SHOTS_* words go there).
    fn task(&self, description: &str) -> String {
        let task = gizai_lib::test_task(&self.st, self.repo.to_str().unwrap(), "backend");
        if !description.is_empty() {
            gizai_core::tasks::update(&self.st.db, &self.st.you_id, &task, TaskPatch { description_md: Some(description.into()), ..Default::default() })
                .unwrap();
        }
        task
    }

    fn browser(&self, on: bool, tools_off: &[&str]) {
        let mcp = if on {
            vec![AgentServer { server_id: core_mcp::BROWSER.into(), on: true, tools_off: tools_off.iter().map(|t| t.to_string()).collect() }]
        } else {
            vec![]
        };
        app_mcp::save_agent(&self.st, &self.agent, AgentTools { mcp }).unwrap();
    }

    fn agent_screenshots(&self, on: bool) {
        core_mcp::set_agent_screenshots(&self.st.db, &self.st.you_id, &self.agent, on).unwrap();
    }

    /// Saves the project's Preview command ("" removes it), Start time and Screenshots switch, as a person's form save
    /// that keeps the rest.
    fn project(&self, command: &str, start_secs: Option<u32>, shots_on: Option<bool>) {
        let p = gizai_core::projects::get(&self.st.db, &self.project).unwrap();
        gizai_core::projects::update(&self.st.db, &self.st.you_id, &p.id, ProjectInput {
            client_id: p.client_id.clone(), name: p.name.clone(), key: p.key.clone(), status: Some(p.status.clone()), goal_md: p.goal_md.clone(),
            repo_path: p.repo_path.clone(), repo_url: p.repo_url.clone(), default_branch: Some(p.default_branch.clone()), color: p.color.clone(),
            budget_amount_minor: p.budget_amount_minor, budget_hours: p.budget_hours,
            screenshots: shots_on.map(|on| ProjectScreenshots { on, ..p.screenshots.clone() }),
            preview_command: Some(command.into()), preview_start_secs: start_secs, ..Default::default()
        }).unwrap();
    }

    /// The node server as the project's Preview command; `mode` "shots" saves two PNGs itself, "stubborn" ignores SIGTERM.
    fn node_preview(&self, mode: &str) -> String {
        format!("echo 'preview starting'; '{}' '{}' '{}' '{mode}'", node().display(), self.tmp.path().join("server.cjs").display(), self.info.display())
    }

    /// Commits a folder `name` (with a file in it) to the project's repository, so a card's new worktree has it.
    fn commit_folder(&self, name: &str) {
        std::fs::create_dir_all(self.repo.join(name)).unwrap();
        std::fs::write(self.repo.join(name).join("index.html"), "<p>hi</p>\n").unwrap();
        for a in [&["add", "-A"][..], &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "web"][..]] {
            assert!(std::process::Command::new("git").args(a).current_dir(&self.repo).status().unwrap().success());
        }
    }

    /// What the preview on `port` wrote before it listened.
    fn info(&self, port: u16) -> Value {
        serde_json::from_str(&read(&self.info.join(format!("preview-{port}.json")))).unwrap()
    }

    /// Every preview that wrote what it got in this test.
    fn infos(&self) -> Vec<String> {
        names_in(&self.info)
    }

    fn worktree(&self, run_id: &str) -> PathBuf {
        PathBuf::from(gizai_core::runs::get(&self.st.db, run_id).unwrap().worktree_path.expect("the run's worktree"))
    }

    /// Where the fake of run `run_id` keeps what it got.
    fn out_of(&self, run_id: &str) -> PathBuf {
        self.out.join(self.worktree(run_id).file_name().unwrap())
    }

    fn got(&self, run_id: &str) -> Got {
        let out = self.out_of(run_id);
        let opt = |name: &str| std::fs::read_to_string(out.join(name)).ok();
        let env = read(&out.join("env")).lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Got {
            run_id: run_id.into(), wt: self.worktree(run_id), prompt: read(&out.join("prompt")), env,
            answered: opt("answered").map(|s| s.trim().to_string()), answer: opt("answer"), preview_log: opt("preview.log"),
            shot: out.join("shot").is_file(),
        }
    }

    /// Runs `task` to its end; returns how it ended and what its Claude Code got.
    async fn run(&self, task: &str) -> (RunSummary, Got) {
        let s = gizai_lib::runs::run_once(&self.st, task, None, None).await.unwrap();
        let got = self.got(&s.run_id);
        (s, got)
    }

    fn sets(&self, task: &str) -> Vec<ScreenshotSet> {
        core_shots::sets(&self.st.db, task).unwrap()
    }

    /// Runs a new card and checks it got no Screenshots section: no preview, no GIZAI_PREVIEW_*, no set on the card.
    async fn run_without_section(&self, why: &str) {
        let task = self.task("");
        let (s, g) = self.run(&task).await;
        assert_eq!(s.status, "succeeded", "{why}: {:?}", s.error);
        assert!(!g.prompt.contains("## Screenshots"), "{why}: {}", g.prompt);
        assert!(!g.prompt.contains("127.0.0.1:"), "{why}: {}", g.prompt);
        for k in ["GIZAI_PREVIEW_PORT", "GIZAI_PREVIEW_URL", "GIZAI_PREVIEW_DIR"] {
            assert!(!g.env.contains_key(k), "{why}: {k} in {:?}", g.env);
        }
        assert!(!g.shot, "{why}: the fake saved no screenshots");
        assert!(self.sets(&task).is_empty(), "{why}: {:?}", self.sets(&task));
        assert_eq!(core_shots::run_note(&self.st.db, &s.run_id).unwrap(), None, "{why}");
        assert!(self.infos().is_empty(), "{why}: no preview started: {:?}", self.infos());
        assert!(!notes(&self.st, &s.run_id).iter().any(|n| n.contains("screenshot")), "{why}: {:?}", notes(&self.st, &s.run_id));
    }
}

/// The preview's processes (node and its child) have ended and its port answers no more.
fn preview_gone(info: &Value, port: u16) {
    for k in ["pid", "child"] {
        let pid = info[k].as_i64().unwrap_or_else(|| panic!("no {k} in {info}"));
        assert!(!alive(pid), "the preview's {k} ({pid}) still runs after the run");
    }
    assert!(!answers(port), "port {port} still answers after the run");
}

/// The two pictures the fake (or the preview) saved, as the card's set has them.
fn two_shots(set: &ScreenshotSet) {
    let names: Vec<&str> = set.shots.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["01-board-desktop.png", "02-card-mobile.png"], "{set:?}");
    let captions: Vec<&str> = set.shots.iter().map(|s| s.caption.as_str()).collect();
    assert_eq!(captions, ["The board", "Card, mobile"], "from captions.json, else from the name");
    let sizes: Vec<(i64, i64)> = set.shots.iter().map(|s| (s.width, s.height)).collect();
    assert_eq!(sizes, [(32, 20), (24, 48)]);
    assert!(set.shots.iter().all(|s| s.size_bytes == PNG_1.len() as i64), "{set:?}");
    assert!(set.left_out.is_empty(), "{:?}", set.left_out);
}

#[tokio::test]
async fn a_run_with_the_browser_and_a_preview_command_gets_its_preview_and_its_screenshots_go_onto_the_card() {
    let t = setup(None);
    t.project(&t.node_preview(""), None, None);
    let task = t.task("");
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);

    // the preview was served on a block from Settings' default Preview ports, and answered before the agent started
    let port = g.port();
    assert!((47100..=47999).contains(&port), "{port}");
    assert!(port + 9 <= 47999, "a whole block of ten: {port}");
    assert_eq!(g.answered.as_deref(), Some("yes"), "the preview answered when the agent started");
    let preview_dir = g.wt.join(".gizai-tmp/preview");
    assert_eq!(g.answer.as_deref().map(|a| a.ends_with(preview_dir.to_str().unwrap())), Some(true), "{:?}", g.answer);

    // the prompt's Screenshots section, with the preview's address and the run's screenshots folder
    let url = format!("http://127.0.0.1:{port}");
    assert!(g.prompt.contains("## Screenshots"), "{}", g.prompt);
    assert!(g.prompt.contains(&format!("served at {url}/ ")), "{}", g.prompt);
    assert!(g.prompt.contains(&format!("{}/<nn>-<what>.png", g.wt.join(".gizai-tmp/screenshots").display())), "{}", g.prompt);
    assert!(g.prompt.contains(&g.wt.join(".gizai-tmp/preview/preview.log").display().to_string()), "{}", g.prompt);
    assert!(!g.prompt.contains("take no screenshots"));

    // the agent's own commands find the preview, but don't get PORT
    assert_eq!(g.env.get("GIZAI_PREVIEW_PORT"), Some(&port.to_string()));
    assert_eq!(g.env.get("GIZAI_PREVIEW_URL"), Some(&url));
    assert_eq!(g.env.get("GIZAI_PREVIEW_DIR").map(PathBuf::from), Some(preview_dir.clone()));
    match std::env::var("PORT") {
        Err(_) => assert!(!g.env.contains_key("PORT"), "the agent gets no PORT: {:?}", g.env),
        Ok(own) => assert_eq!(g.env.get("PORT"), Some(&own), "only the PORT this test process has"),
    }
    assert_ne!(g.env.get("PORT"), Some(&port.to_string()));
    assert_eq!(g.env.get("TMPDIR").map(PathBuf::from), Some(g.wt.join(".gizai-tmp")), "{:?}", g.env);

    // the Preview command got PORT, the three GIZAI_PREVIEW_*, the worktree as its folder and no CLAUDECODE
    let info = t.info(port);
    assert_eq!(info["port"], port.to_string(), "{info}");
    assert_eq!(info["previewPort"], port.to_string());
    assert_eq!(info["url"], url);
    assert_eq!(info["dir"].as_str().map(PathBuf::from), Some(preview_dir.clone()));
    assert_eq!(std::fs::canonicalize(info["cwd"].as_str().unwrap()).unwrap(), std::fs::canonicalize(&g.wt).unwrap());
    assert!(info["claudecode"].is_null(), "{info}");
    // its output went to .gizai-tmp/preview/preview.log, there while the run lived
    let log = g.preview_log.as_deref().expect("preview.log was there when the agent started");
    assert!(log.contains("preview starting") && log.contains(&format!("preview listening on {port}")), "{log}");

    // the card's set: this run's two pictures, in name order, with their captions and sizes, at the run's commit
    let sets = t.sets(&task);
    assert_eq!(sets.len(), 1, "{sets:?}");
    let set = &sets[0];
    assert_eq!(set.run_id, s.run_id);
    assert_eq!(set.agent_id, t.agent);
    assert_eq!(set.status, "succeeded");
    assert_eq!(set.preview, None);
    two_shots(set);
    let run = gizai_core::runs::get(&t.st.db, &s.run_id).unwrap();
    assert!(run.head_sha.is_some());
    assert_eq!(set.commit, run.head_sha);
    // the picture's own bytes, from the blob store
    assert_eq!(core_shots::bytes(&t.st.db, &t.st.data_dir, &set.shots[0].file_id).unwrap(), PNG_1);
    // a screenshot isn't an attachment
    assert!(gizai_core::files::list(&t.st.db, "task", &task).unwrap().is_empty());
    // its note: nothing left out, and no preview left to end after a crash
    let note = core_shots::run_note(&t.st.db, &s.run_id).unwrap().expect("the run's note");
    assert!(note.left_out.is_empty() && note.preview.is_none() && note.preview_pid.is_none(), "{note:?}");

    // the preview's whole tree ended with the run, and its port is free again
    preview_gone(&info, port);
    // the run's output says what Gizai kept
    let said = notes(&t.st, &s.run_id);
    assert!(said.iter().any(|n| n == "Gizai kept 2 screenshots on the card."), "{said:?}");
    // and its temp folder is empty
    assert_eq!(names_in(&g.wt.join(".gizai-tmp")), Vec::<String>::new());
    assert!(g.wt.join(".gizai-tmp").is_dir());
}

#[tokio::test]
async fn a_project_without_a_preview_command_gives_no_screenshots_section() {
    let t = setup(Some((48600, 48699)));
    t.run_without_section("no Preview command").await;
    // a command of only spaces is none either
    t.project("   ", None, None);
    t.run_without_section("a blank Preview command").await;
}

#[tokio::test]
async fn an_agent_with_take_screenshots_off_gets_no_screenshots_section() {
    let t = setup(Some((48700, 48799)));
    t.project(&t.node_preview(""), None, None);
    t.agent_screenshots(false);
    t.run_without_section("the agent's Take screenshots off").await;
}

#[tokio::test]
async fn an_agent_without_the_browser_or_its_take_screenshot_gets_no_screenshots_section() {
    let t = setup(Some((48800, 48899)));
    t.project(&t.node_preview(""), None, None);
    t.browser(false, &[]);
    t.run_without_section("the browser off").await;
    t.browser(true, &["take_screenshot"]);
    t.run_without_section("the browser's take_screenshot off").await;
}

#[tokio::test]
async fn a_card_or_project_with_screenshots_off_gets_no_screenshots_section_and_the_cards_switch_wins() {
    let t = setup(Some((48900, 48999)));
    t.project(&t.node_preview(""), None, None);
    // the card's switch off, its project's on
    let task = t.task("");
    gizai_core::tasks::update(&t.st.db, &t.st.you_id, &task, TaskPatch { screenshots: Some(Some(false)), ..Default::default() }).unwrap();
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    assert!(!g.prompt.contains("## Screenshots") && !g.env.contains_key("GIZAI_PREVIEW_PORT"), "{:?}", g.env);
    assert!(t.sets(&task).is_empty());
    assert!(t.infos().is_empty(), "{:?}", t.infos());

    // the project's off and the card's unset: none
    t.project(&t.node_preview(""), None, Some(false));
    t.run_without_section("the project's Screenshots off").await;

    // the project's off and the card's on: the card's switch wins
    let task = t.task("");
    gizai_core::tasks::update(&t.st.db, &t.st.you_id, &task, TaskPatch { screenshots: Some(Some(true)), ..Default::default() }).unwrap();
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    assert!(g.prompt.contains("## Screenshots"), "{}", g.prompt);
    assert_eq!(g.answered.as_deref(), Some("yes"));
    two_shots(&t.sets(&task)[0]);
}

#[tokio::test]
async fn a_preview_that_never_answers_lets_the_run_start_without_screenshots_and_is_ended() {
    let t = setup(Some((48000, 48099)));
    let pidfile = t.tmp.path().join("sleep.pid");
    t.project(&format!("echo $$ > '{}'; echo 'still building'; exec sleep 600", pidfile.display()), Some(10), None);
    let task = t.task("");
    let t0 = Instant::now();
    let (s, g) = t.run(&task).await;
    let took = t0.elapsed();
    assert_eq!(s.status, "succeeded", "the run starts anyway: {:?}", s.error);
    assert!(took >= Duration::from_secs(10), "it waited the Start time: {took:?}");
    assert!(took < Duration::from_secs(40), "not much longer: {took:?}");

    let port = g.port();
    assert!((48000..=48099).contains(&port), "{port}");
    let why = format!("it didn't answer on port {port} in 10 s");
    assert!(g.prompt.contains("## Screenshots"), "{}", g.prompt);
    assert!(g.prompt.contains(&format!("The project's preview didn't start, so take no screenshots in this run: {why}.")), "{}", g.prompt);
    assert!(g.prompt.contains("still building"), "its log's last lines: {}", g.prompt);
    assert!(!g.prompt.contains("served at"), "{}", g.prompt);
    assert_eq!(g.answered.as_deref(), Some("no"), "nothing answered on the port when the agent started");
    assert!(!g.shot);

    // the card's set says why there was no preview, with no pictures
    let sets = t.sets(&task);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!(sets[0].run_id, s.run_id);
    let preview = sets[0].preview.as_deref().expect("why there was no preview");
    assert!(preview.contains("didn't answer") && preview.contains(&port.to_string()), "{preview}");
    assert!(sets[0].shots.is_empty());
    // the run's output says it too
    let said = notes(&t.st, &s.run_id);
    assert!(said.iter().any(|n| n.starts_with(&format!("The project's preview didn't start, so the agent takes no screenshots in this run: {why}."))
                               && n.contains("still building")), "{said:?}");

    // the preview (bash, then sleep in its place) is gone
    let pid: i64 = read(&pidfile).trim().parse().unwrap();
    assert!(!alive(pid), "the preview {pid} still runs after the run");
    assert!(core_shots::run_note(&t.st.db, &s.run_id).unwrap().unwrap().preview_pid.is_none());
    assert_eq!(names_in(&g.wt.join(".gizai-tmp")), Vec::<String>::new());
}

#[tokio::test]
async fn a_preview_that_ends_before_it_answers_lets_the_run_start_without_screenshots() {
    let t = setup(Some((49000, 49099)));
    t.project("echo 'cannot find module vite'; exit 3", Some(60), None);
    let task = t.task("");
    let t0 = Instant::now();
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    assert!(t0.elapsed() < Duration::from_secs(30), "not the whole Start time: {:?}", t0.elapsed());
    let port = g.port();
    let why = format!("it ended before it answered on port {port} (exit code 3)");
    assert!(g.prompt.contains(&format!("The project's preview didn't start, so take no screenshots in this run: {why}.")), "{}", g.prompt);
    assert!(g.prompt.contains("cannot find module vite"), "{}", g.prompt);
    let sets = t.sets(&task);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert!(sets[0].preview.as_deref().is_some_and(|p| p.contains("exit code 3")), "{:?}", sets[0].preview);
    assert!(sets[0].shots.is_empty());
}

#[tokio::test]
async fn three_runs_at_once_each_get_a_block_of_ports_of_their_own() {
    let t = setup(Some((48100, 48299)));
    t.project(&t.node_preview(""), None, None);
    let tasks: Vec<String> = (0..3).map(|_| t.task("FAKE_SHOTS_GATE")).collect();
    let (a, b, c) = tokio::join!(
        gizai_lib::runs::start(&t.st, &tasks[0], None, None, "manual"),
        gizai_lib::runs::start(&t.st, &tasks[1], None, None, "manual"),
        gizai_lib::runs::start(&t.st, &tasks[2], None, None, "manual"),
    );
    let started = [a.unwrap(), b.unwrap(), c.unwrap()];
    let ids: Vec<String> = started.iter().map(|(id, _)| id.clone()).collect();
    // all three live at once, each held at the gate
    until("the three fakes", 60, || ids.iter().all(|id| t.out_of(id).join("env").is_file())).await;
    let got: Vec<Got> = ids.iter().map(|id| t.got(id)).collect();
    let mut ports: Vec<u16> = got.iter().map(Got::port).collect();
    for g in &got {
        assert_eq!(g.answered.as_deref(), Some("yes"), "{}", g.run_id);
        // each agent reached its own card's preview
        let own = g.wt.join(".gizai-tmp/preview");
        assert_eq!(g.answer.as_deref().map(|a| a.ends_with(own.to_str().unwrap())), Some(true), "{:?}", g.answer);
        assert!(g.prompt.contains(&format!("served at http://127.0.0.1:{}/ ", g.port())));
    }
    ports.sort();
    assert!(ports[1] - ports[0] >= 10 && ports[2] - ports[1] >= 10, "blocks of ten that don't overlap: {ports:?}");
    assert!(ports.iter().all(|p| (48100..=48290).contains(p)), "{ports:?}");

    std::fs::write(t.out.join("gate"), "").unwrap();
    for ((id, done), task) in started.into_iter().zip(&tasks) {
        let s = tokio::time::timeout(Duration::from_secs(60), done).await.expect("the run ended").unwrap();
        assert_eq!(s.status, "succeeded", "{id}: {:?}", s.error);
        let sets = t.sets(task);
        assert_eq!(sets.len(), 1, "{sets:?}");
        assert_eq!(sets[0].run_id, id);
        two_shots(&sets[0]);
    }
    for p in ports {
        preview_gone(&t.info(p), p);
    }
}

#[tokio::test]
async fn a_run_stopped_at_its_time_limit_keeps_its_screenshots_and_its_preview_ends() {
    let t = setup(Some((48300, 48399)));
    // the preview saves the pictures itself: a run stopped at once can't
    t.project(&t.node_preview("shots"), None, None);
    // Settings refuses less than 5 minutes; 0 straight in the database reaches the time limit at once
    gizai_core::settings::set(&t.st.db, "max_run_minutes", &0u64).unwrap();
    let task = t.task("FAKE_SHOTS_HANG");
    let t0 = Instant::now();
    let s = gizai_lib::runs::run_once(&t.st, &task, None, None).await.unwrap();
    assert!(t0.elapsed() < Duration::from_secs(40), "{:?}", t0.elapsed());
    assert_eq!(s.status, "timed_out", "{s:?}");
    let sets = t.sets(&task);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!((sets[0].run_id.as_str(), sets[0].status.as_str()), (s.run_id.as_str(), "timed_out"));
    two_shots(&sets[0]);
    let said = notes(&t.st, &s.run_id);
    assert!(said.iter().any(|n| n == "Gizai kept 2 screenshots on the card."), "{said:?}");
    let infos = t.infos();
    assert_eq!(infos.len(), 1, "{infos:?}");
    let port: u16 = infos[0].trim_start_matches("preview-").trim_end_matches(".json").parse().unwrap();
    preview_gone(&t.info(port), port);
    assert_eq!(names_in(&t.worktree(&s.run_id).join(".gizai-tmp")), Vec::<String>::new());
}

#[tokio::test]
async fn a_run_stopped_by_stop_keeps_the_screenshots_it_saved_and_its_preview_ends() {
    let t = setup(Some((48400, 48499)));
    t.project(&t.node_preview(""), None, None);
    let task = t.task("FAKE_SHOTS_HANG");
    let (run_id, done) = gizai_lib::runs::start(&t.st, &task, None, None, "manual").await.unwrap();
    until("the fake's screenshots", 60, || t.out_of(&run_id).join("shot").is_file()).await;
    // while it runs, its preview answers and its set isn't on the card yet
    let port = t.got(&run_id).port();
    assert!(answers(port));
    assert!(t.sets(&task).is_empty(), "a run still at work has no set yet");
    tokio::time::sleep(Duration::from_millis(200)).await;
    gizai_lib::runs::stop(&t.st, &run_id);
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("stopped").unwrap();
    assert_eq!(s.status, "cancelled", "{s:?}");
    let sets = t.sets(&task);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!(sets[0].status, "cancelled");
    two_shots(&sets[0]);
    preview_gone(&t.info(port), port);
    assert_eq!(names_in(&t.worktree(&run_id).join(".gizai-tmp")), Vec::<String>::new());
}

#[tokio::test]
async fn a_run_stopped_because_gizai_quits_keeps_no_screenshots_but_its_preview_still_ends() {
    let t = setup(Some((48500, 48599)));
    t.project(&t.node_preview(""), None, None);
    let task = t.task("FAKE_SHOTS_HANG");
    let (run_id, done) = gizai_lib::runs::start(&t.st, &task, None, None, "manual").await.unwrap();
    until("the fake's screenshots", 60, || t.out_of(&run_id).join("shot").is_file()).await;
    let port = t.got(&run_id).port();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(gizai_lib::runs::stop_all(&t.st, Duration::from_secs(15)).await, 1);
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("stopped").unwrap();
    assert_eq!(s.status, "cancelled", "{s:?}");
    assert!(t.sets(&task).iter().all(|set| set.shots.is_empty()), "a run Gizai's quitting stopped collects nothing: {:?}", t.sets(&task));
    preview_gone(&t.info(port), port);
    assert_eq!(names_in(&t.worktree(&run_id).join(".gizai-tmp")), Vec::<String>::new());
}

#[tokio::test]
async fn a_preview_that_ignores_sigterm_is_killed_three_seconds_later() {
    let t = setup(Some((49600, 49699)));
    t.project(&t.node_preview("stubborn"), None, None);
    let task = t.task("");
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let port = g.port();
    assert_eq!(g.answered.as_deref(), Some("yes"));
    // SIGTERM, then SIGKILL to what still runs after 3 s: the run's end waited for it
    let run = gizai_core::runs::get(&t.st.db, &s.run_id).unwrap();
    let ended = run.ended_at.expect("ended_at");
    let started = run.started_at.expect("started_at");
    assert!(ended - started >= 3000, "the run's end waited the 3 s before SIGKILL: {} ms", ended - started);
    preview_gone(&t.info(port), port);
    two_shots(&t.sets(&task)[0]);
}

#[tokio::test]
async fn a_port_something_else_listens_on_is_skipped_for_the_next_block_of_ten() {
    let t = setup(Some((49100, 49199)));
    // something else already listens on the range's first port (bound by this test, or by whatever holds it)
    let _held = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 49100)).ok();
    assert!(answers(49100));
    t.project(&t.node_preview(""), None, None);
    let task = t.task("");
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    assert_eq!(g.port(), 49110, "the next block of ten");
    assert_eq!(g.answered.as_deref(), Some("yes"));
    two_shots(&t.sets(&task)[0]);
    preview_gone(&t.info(49110), 49110);
}

#[tokio::test]
async fn without_a_free_block_of_ports_the_run_starts_without_a_preview_and_says_why() {
    let t = setup(Some((49200, 49229)));
    // each block's first port is taken
    let _held: Vec<_> = [49200u16, 49210, 49220].iter().map(|p| std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, *p)).ok()).collect();
    t.project(&t.node_preview(""), None, None);
    let task = t.task("");
    let (s, g) = t.run(&task).await;
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let why = "There was no free block of ten ports for it in 49200 to 49229 (Settings → Agents and runs → Screenshots → Preview ports)";
    let lower = "there was no free block of ten ports for it in 49200 to 49229 (Settings → Agents and runs → Screenshots → Preview ports)";
    assert!(g.prompt.contains(&format!("The project's preview didn't start, so take no screenshots in this run: {lower}.")), "{}", g.prompt);
    assert!(!g.env.contains_key("GIZAI_PREVIEW_PORT"), "{:?}", g.env);
    assert!(t.infos().is_empty(), "no preview started: {:?}", t.infos());
    let sets = t.sets(&task);
    assert_eq!(sets.len(), 1, "{sets:?}");
    assert_eq!(sets[0].preview.as_deref(), Some(why));
    assert!(sets[0].shots.is_empty());
    let said = notes(&t.st, &s.run_id);
    assert!(said.iter().any(|n| n == &format!("The project's preview didn't start, so the agent takes no screenshots in this run: {lower}.")),
            "{said:?}");
}

#[tokio::test]
async fn a_continued_run_gets_its_preview_and_its_own_set_too() {
    let t = setup(Some((49300, 49399)));
    t.project(&t.node_preview(""), None, None);
    let task = t.task("FAKE_SHOTS_HANG");
    let (first, done) = gizai_lib::runs::start(&t.st, &task, None, None, "manual").await.unwrap();
    until("the fake's screenshots", 60, || t.out_of(&first).join("shot").is_file()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    gizai_lib::runs::stop(&t.st, &first);
    let s = tokio::time::timeout(Duration::from_secs(30), done).await.expect("stopped").unwrap();
    assert_eq!(s.status, "cancelled", "{s:?}");

    gizai_core::tasks::update(&t.st.db, &t.st.you_id, &task, TaskPatch { description_md: Some("Export the invoices".into()), ..Default::default() })
        .unwrap();
    let (second, done) = gizai_lib::runs::continue_run(&t.st, &first, None).await.unwrap();
    let s = tokio::time::timeout(Duration::from_secs(60), done).await.expect("the continued run ended").unwrap();
    assert_eq!(s.status, "succeeded", "{:?}", s.error);
    let g = t.got(&second);
    assert!(g.prompt.contains("## Screenshots") && g.prompt.contains(&format!("served at http://127.0.0.1:{}/ ", g.port())), "{}", g.prompt);
    assert_eq!(g.answered.as_deref(), Some("yes"));
    // newest run first, each with its own pictures
    let sets = t.sets(&task);
    let runs: Vec<&str> = sets.iter().map(|s| s.run_id.as_str()).collect();
    assert_eq!(runs, [second.as_str(), first.as_str()]);
    two_shots(&sets[0]);
    two_shots(&sets[1]);
    preview_gone(&t.info(g.port()), g.port());
}

/// Gizai crashed while run `run_id` of `t` lived, and starts again: a consistent copy of its data (the run still active,
/// its preview's pid in its note) opened as Gizai opens its data, which ends what such a run left running. The run's own
/// claude process group is left out (its pid taken out of the copy), so only what happens to the preview is seen.
fn start_again_after_a_crash(t: &T, run_id: &str) -> AppState {
    let snap = gizai_lib::backup_data_dir(&t.st.data_dir, "crash").unwrap();
    let again = t.tmp.path().join("again");
    std::fs::create_dir_all(again.join("data")).unwrap();
    std::fs::copy(&snap, again.join("data/gizai.db")).unwrap();
    {
        let db = gizai_core::db::Db::open(&again.join("data/gizai.db")).unwrap();
        db.write(None, |w| {
            w.conn().execute("UPDATE runs SET pid = NULL WHERE id = ?1", [run_id])?;
            Ok(())
        }).unwrap();
    }
    gizai_lib::test_state(&again)
}

/// Starts a card whose fake hangs, waits until its preview answers and its agent has saved its pictures; returns the
/// run's id, its end, its preview's port and what the preview got.
async fn hanging_run(t: &T) -> (String, tokio::task::JoinHandle<RunSummary>, u16, Value) {
    let task = t.task("FAKE_SHOTS_HANG");
    let (run_id, done) = gizai_lib::runs::start(&t.st, &task, None, None, "manual").await.unwrap();
    until("the fake's screenshots", 60, || t.out_of(&run_id).join("shot").is_file()).await;
    let port = t.got(&run_id).port();
    let note = core_shots::run_note(&t.st.db, &run_id).unwrap().expect("the run's note");
    assert!(note.preview_pid.is_some(), "the preview's pid is saved while it runs: {note:?}");
    (run_id, done, port, t.info(port))
}

#[tokio::test]
async fn after_a_crash_gizai_starting_again_ends_the_preview_a_run_left_running() {
    let t = setup(Some((49400, 49499)));
    t.project(&t.node_preview(""), None, None);
    let (run_id, done, port, info) = hanging_run(&t).await;
    let pid = info["pid"].as_i64().unwrap();
    let _again = start_again_after_a_crash(&t, &run_id);
    // the run itself lives on here (only the copy saw a crash); its preview was ended by the start
    assert_eq!(gizai_core::runs::get(&t.st.db, &run_id).unwrap().status, "running");
    until("the preview to end after the start", 5, || !alive(pid)).await;
    until("its port to be free", 5, || !answers(port)).await;
    gizai_lib::runs::stop(&t.st, &run_id);
    let _ = tokio::time::timeout(Duration::from_secs(30), done).await.expect("stopped");
}

// A preview never outlives its run, also after a crash: the next start ends it, also when its Preview command changed
// folder first, such as `cd web && npm run dev` in a repository with the site in a subfolder, which moves the process
// that leads its group (bash) into <worktree>/web (`end_orphan_group` compares that process's folder with the worktree:
// src-tauri/src/lib.rs:280 → crates/gizai-agents/src/process.rs:120).
#[tokio::test]
async fn after_a_crash_a_preview_whose_command_changed_folder_is_ended() {
    let t = setup(Some((49500, 49599)));
    t.commit_folder("web");
    t.project(&format!("cd web && {}", t.node_preview("")), None, None);
    let (run_id, done, port, info) = hanging_run(&t).await;
    let pid = info["pid"].as_i64().unwrap();
    // the process that leads the preview's group, as its note saved it, and the folder it works in
    let leader = core_shots::run_note(&t.st.db, &run_id).unwrap().unwrap().preview_pid.unwrap();
    let leader_cwd = std::fs::read_link(format!("/proc/{leader}/cwd")).ok();
    let wt = t.worktree(&run_id);
    let _again = start_again_after_a_crash(&t, &run_id);
    let t0 = Instant::now();
    while alive(pid) && t0.elapsed() < Duration::from_secs(5) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let left_running = alive(pid);
    // end the run (and with it its preview) before saying so
    gizai_lib::runs::stop(&t.st, &run_id);
    let _ = tokio::time::timeout(Duration::from_secs(30), done).await.expect("stopped");
    assert!(!left_running, "Gizai started again after a crash and left the preview of `cd web && …` running (node {pid}, port {port}): \
its group's leader {leader} works in {leader_cwd:?}, not in the worktree {}", wt.display());
}
