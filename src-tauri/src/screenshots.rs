//! Screenshots (GA-182): the app's side of a run's screenshots. At a run's start: whether it gets the Screenshots section
//! (`gets_section`), then its preview's ports, start and wait (`start`). At its end: the preview's end and the collect
//! (`finish`). And the card's sets and a screenshot's bytes for its Screenshots tab (contract CS: `list_screenshots`,
//! `screenshot_bytes`). The checks and the data live in `gizai_agents::{preview, screenshots}` and
//! `gizai_core::screenshots`.
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gizai_agents::os::{End, Tree};
use gizai_agents::preview::{self, Preview};
use gizai_agents::prompt::{PreviewDown, Screenshots};
use gizai_agents::run_memory::Leftover;
use gizai_agents::screenshots::{self as shots, Collected, Rules};
use gizai_core::files::{self, Evidence};
use gizai_core::model::{Project, Task};
use gizai_core::screenshots::{self as core_shots, ScreenshotSet};
use tauri::State;

use crate::AppState;

/// The previews of the runs that have one now: each run's first port, and its preview's tree while it runs (so Gizai
/// quitting at once can end it: `end_previews`).
pub(crate) type Previews = Arc<Mutex<HashMap<String, (u16, Option<Tree>)>>>;

/// A run's block of ports, held until the run has ended and its preview with it; dropped, the block is free again.
pub(crate) struct PortLease {
    previews: Previews,
    run_id: String,
    port: u16,
}

impl Drop for PortLease {
    fn drop(&mut self) {
        self.previews.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.run_id);
    }
}

/// What a run that gets the Screenshots section carries from its start to its end.
pub(crate) struct RunShots {
    /// The section for its prompt.
    pub section: Screenshots,
    /// The environment its agent's own commands get (`GIZAI_PREVIEW_*`), once it has a port.
    pub env: Vec<(String, String)>,
    /// What its output says at its start: why its preview didn't start.
    pub note: Option<String>,
    /// Its preview while it runs: ended at the run's end, before the collect.
    preview: Option<Preview>,
    /// What its preview left running in its scope, ended when the preview did (GA-211): named in the run's note at its end.
    left: Vec<Leftover>,
    /// Its ports.
    lease: Option<PortLease>,
    /// What its screenshots are held to.
    rules: Rules,
}

/// Whether a run gets the Screenshots section: the agent's browser is on in this run (`browser`: on Claude Code, in the
/// run's MCP config, with take_screenshot on), its Take screenshots of what it built is on (its role's default until set),
/// the card's Screenshots before Review is on (else its project's Screenshots), and the project has a Preview command.
/// Otherwise the prompt has no Screenshots section and the run starts as before.
pub(crate) fn gets_section(st: &AppState, browser: bool, agent_id: &str, role: &str, task: &Task, project: &Project) -> bool {
    browser && gizai_core::mcp_servers::takes_screenshots(gizai_core::mcp_servers::agent_screenshots(&st.db, agent_id).ok().flatten(), role)
        && core_shots::card_on(task, project) && project.preview_command.as_deref().is_some_and(|c| !c.trim().is_empty())
}

/// "It didn't answer…" → "it didn't answer…", for after a colon; "PHP …" stays.
fn lower_first(s: &str) -> String {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if first.is_uppercase() && second.is_lowercase() => first.to_lowercase().chain(s.chars().skip(1)).collect(),
        _ => s.to_string(),
    }
}

/// For a run that gets the Screenshots section (`gets_section`), after its worktree and `.gizai-tmp` are ready and
/// before its CLI starts: marks the run (its set shows on the card also when it saves none), takes a free block of ten
/// ports from Settings → Agents and runs → Screenshots → Preview ports (none of another live run's, the first one
/// checked by binding 127.0.0.1 on it), starts the project's Preview command in the worktree
/// (`gizai_agents::preview::start`) and waits until its port answers, at most the project's Start time. When it
/// doesn't, the run starts anyway: its prompt says why and to take no screenshots, its set records why, and the preview
/// is ended at once. The card is never held.
pub(crate) async fn start(st: &AppState, run_id: &str, role: &str, project: &Project, wt: &Path) -> RunShots {
    let limits = core_shots::limits(&st.db);
    if let Err(e) = core_shots::begin(&st.db, run_id) {
        eprintln!("gizai: marking run {run_id} as one that takes screenshots failed: {e}");
    }
    let s = &project.screenshots;
    let rules = Rules { per_run: limits.per_run as usize, max_bytes: limits.max_bytes(), sizes: s.sizes.iter().map(|z| z.name.clone()).collect() };
    let section = Screenshots {
        qa: role == "qa",
        preview: Ok(String::new()),
        log: preview::log_path(wt).display().to_string(),
        folder: shots::dir(wt).display().to_string(),
        sizes: s.sizes.iter().map(|z| (z.name.clone(), z.width, z.height)).collect(),
        both_themes: s.both_themes,
        full_page: s.full_page,
        per_run: limits.per_run,
    };
    let mut run = RunShots { section, env: vec![], note: None, preview: None, left: vec![], lease: None, rules };
    let Some(lease) = take_ports(st, run_id, limits.port_from, limits.port_to) else {
        let why = format!("There was no free block of ten ports for it in {} to {} (Settings → Agents and runs → Screenshots → Preview ports)",
                          limits.port_from, limits.port_to);
        return failed(st, run_id, run, why, String::new());
    };
    let port = lease.port;
    run.env = preview::agent_env(port, wt);
    run.lease = Some(lease);
    let (command, dir, memory) = (project.preview_command.clone().unwrap_or_default(), wt.to_path_buf(), st.run_memory.clone());
    let started = tokio::task::spawn_blocking(move || {
        let path = crate::runs::command_path();
        // GA-202: in the slice of the agent runs, under their memory limit (Linux), as the setup command
        preview::start(&command, &dir, port, Some(path.as_os_str()), memory.slice().as_ref())
    }).await;
    let mut p = match started {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return failed(st, run_id, run, format!("Gizai couldn't start it: {e}"), String::new()),
        Err(e) => return failed(st, run_id, run, format!("Gizai couldn't start it: {e}"), String::new()),
    };
    if let Some(entry) = st.runs.previews.lock().unwrap_or_else(|e| e.into_inner()).get_mut(run_id) {
        entry.1 = Some(p.tree());
    }
    if let Err(e) = core_shots::set_preview_pid(&st.db, run_id, Some(p.pid())) {
        eprintln!("gizai: saving run {run_id}'s preview failed: {e}");
    }
    let limit = Duration::from_secs(u64::from(project.preview_start_secs));
    match p.wait_ready(limit, || crate::runs::is_closing(st)).await {
        Ok(()) => {
            run.section.preview = Ok(preview::url(port));
            run.preview = Some(p);
            run
        }
        Err(why) => {
            let tail = preview::log_tail(wt);
            run.left = p.stop().await;
            let _ = core_shots::set_preview_pid(&st.db, run_id, None);
            failed(st, run_id, run, why, tail)
        }
    }
}

/// The run's preview didn't start (`why`, its log's last lines `tail`): the set records why, and the prompt and the
/// run's output say it and that the agent takes no screenshots.
fn failed(st: &AppState, run_id: &str, mut run: RunShots, why: String, tail: String) -> RunShots {
    if let Err(e) = core_shots::set_preview_why(&st.db, run_id, &why) {
        eprintln!("gizai: saving why run {run_id}'s preview didn't start failed: {e}");
    }
    let mut note = format!("The project's preview didn't start, so the agent takes no screenshots in this run: {}.", lower_first(why.trim_end_matches('.')));
    if !tail.trim().is_empty() {
        note.push_str(&format!(" Its last lines: {}", tail.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" · ")));
    }
    run.note = Some(note);
    run.section.preview = Err(PreviewDown { why, tail });
    run
}

/// A free block of ten ports for `run_id` from `from` to `to`, none of another live run's (`gizai_agents::preview::
/// free_port`), held until the lease is dropped. Picked and noted under one lock: two runs starting together never get
/// the same block.
fn take_ports(st: &AppState, run_id: &str, from: u32, to: u32) -> Option<PortLease> {
    let (from, to) = (u16::try_from(from).ok()?, u16::try_from(to).ok()?);
    let mut previews = st.runs.previews.lock().unwrap_or_else(|e| e.into_inner());
    let taken: Vec<u16> = previews.iter().filter(|(id, _)| id.as_str() != run_id).map(|(_, (port, _))| *port).collect();
    let port = preview::free_port(from, to, &taken)?;
    previews.insert(run_id.to_string(), (port, None));
    Some(PortLease { previews: st.runs.previews.clone(), run_id: run_id.to_string(), port })
}

/// Gizai exits with runs still live: ends their previews' trees at once (SIGKILL; on Windows their Job Objects).
pub(crate) fn end_previews(previews: &Previews) {
    for (_, tree) in previews.lock().unwrap_or_else(|e| e.into_inner()).values() {
        if let Some(t) = tree {
            t.end(End::Kill);
        }
    }
}

/// At a run's end, however it ended, after its CLI's process group has ended: ends its preview's whole tree (SIGTERM,
/// SIGKILL after 3 s; on Windows its Job Object), then what the preview left in its scope (GA-211), and frees its ports,
/// then, unless Gizai's quitting stopped the run (`collect` false), collects its screenshots onto the card (`keep`) and
/// records on the run what was left out and why. Before its `.gizai-tmp` is emptied. Returns what the run's output says
/// about them, if anything, and the processes its preview left running that were ended (also those of a preview that
/// didn't start).
pub(crate) async fn finish(st: &AppState, run_id: &str, task_id: &str, agent_id: &str, wt: &Path, run: RunShots, collect: bool)
    -> (Option<String>, Vec<Leftover>) {
    let RunShots { preview, mut left, lease, rules, .. } = run;
    if let Some(p) = preview {
        left.extend(p.stop().await);
        let _ = core_shots::set_preview_pid(&st.db, run_id, None);
    }
    drop(lease);
    if !collect {
        return (None, left);
    }
    let (st2, rid, tid, aid, dir) = (st.clone(), run_id.to_string(), task_id.to_string(), agent_id.to_string(), wt.to_path_buf());
    let collected = match tokio::task::spawn_blocking(move || keep(&st2, &rid, &tid, &aid, &dir, &rules)).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("gizai: collecting the screenshots of run {run_id} failed: {e}");
            return (None, left);
        }
    };
    if let Err(e) = core_shots::set_left_out(&st.db, run_id, &collected.left_out) {
        eprintln!("gizai: saving what run {run_id} left out of its screenshots failed: {e}");
    }
    if collected.kept > 0 {
        (st.notify)(crate::runs::Note::RowsChanged("files"));
    }
    (collected.note(), left)
}

/// Collects the screenshots in the run's worktree `wt` (`gizai_agents::screenshots::collect`, one at a time) and keeps
/// each on the card: into the blob store (`files::store_bytes`, the bytes collect read once), then its row with the run's
/// id, the agent, its caption, width and height (`files::insert_evidence`).
fn keep(st: &AppState, run_id: &str, task_id: &str, agent_id: &str, wt: &Path, rules: &Rules) -> Collected {
    shots::collect(wt, rules, |shot| {
        let blob = files::store_bytes(&st.data_dir, &shot.name, &shot.bytes).map_err(|e| e.to_string())?;
        let e = Evidence { run_id, caption: &shot.caption, mime: shot.format.mime(), width: shot.width, height: shot.height };
        st.db.write(Some(agent_id), |w| files::insert_evidence(w, agent_id, task_id, &blob, &e)).map(|_| ()).map_err(|e| e.to_string())
    })
}

/// The card's screenshot sets, newest run first (contract CS): every ended run of the card that got the Screenshots
/// section, with its pictures, what it left out, why it had no preview and its commit; also a run that saved none.
#[tauri::command]
pub async fn list_screenshots(st: State<'_, AppState>, task_id: String) -> Result<Vec<ScreenshotSet>, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core_shots::sets(&st.db, &task_id).map_err(|e| e.to_string()))
        .await.map_err(|e| e.to_string())?
}

/// One screenshot's bytes (PNG, JPEG or WebP), as an ArrayBuffer for a blob: URL: only a run's screenshot, by its id,
/// read from the blob store; any other file id is refused.
#[tauri::command]
pub async fn screenshot_bytes(st: State<'_, AppState>, file_id: String) -> Result<tauri::ipc::Response, String> {
    let st = st.inner().clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || core_shots::bytes(&st.db, &st.data_dir, &file_id).map_err(|e| e.to_string()))
        .await.map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}
