//! Memory for agent runs (GA-202, docs/PLATFORMS.md): Settings → Runs → Memory for agent runs, one limit for all agent
//! runs together. On Linux with a systemd user session the runs, chat answers, board checks and worktree preparations
//! start in this Gizai's own slice (`gizai_agents::run_memory`), which the setting limits; Gizai itself stays outside it.
//! On every system no run starts while memory is short (`wait_reason`), and a process Linux ended at the limit shows in
//! the card's activity (`ended_on_card`).
//!
//! What runs leave behind in the slice (GA-211): a run's end ends what it left in its scope and its log names it
//! (`leftovers_note`); quitting ends whatever is still in the slice (`end_leftovers`), and so does Gizai's next start,
//! after a crash, before it recovers the runs (`end_leftovers_at_start`).
use std::path::Path;

use gizai_agents::run_memory::{self as mem, GB, Leftover};
use gizai_core::{ids, settings};

use crate::AppState;

/// The settings key: the limit in whole GB. Unset until someone sets it (`default_gb`).
pub const KEY: &str = "runs_memory_gb";
/// The smallest limit Settings → Runs allows, in GB.
pub const MIN_GB: u32 = 2;
/// No run starts while the runs use more than this share of the limit, in percent…
pub const USED_PERCENT: u64 = 80;
/// …or while the computer has less than this free.
pub const MIN_FREE: u64 = 2 * GB;

/// This computer's memory in whole GB, as Settings → Runs shows it; None when the system doesn't say.
pub fn computer_gb() -> Option<u32> {
    mem::total().map(|t| ((t + GB / 2) / GB).max(1) as u32)
}

/// The limit until someone sets it: half of this computer's memory (64 GB → 32 GB, 16 GB → 8 GB), at least 2 GB; 8 GB
/// when the system doesn't say how much it has.
pub fn default_gb() -> u32 {
    mem::total().map(|t| ((t / 2 + GB / 2) / GB) as u32).unwrap_or(8).max(MIN_GB)
}

/// Memory for agent runs in GB: as saved, else `default_gb`.
pub fn setting_gb(st: &AppState) -> u32 {
    settings::get::<u32>(&st.db, KEY).ok().flatten().unwrap_or_else(default_gb)
}

/// The limit the runs have now, in bytes: the slice's (`RunsMemory::limit`, the setting once Gizai started), else the
/// setting.
pub fn limit(st: &AppState) -> u64 {
    st.run_memory.limit().unwrap_or_else(|| u64::from(setting_gb(st)) * GB)
}

/// Whether `gb` may be saved: 2 GB up to this computer's memory.
pub fn check(gb: u32) -> Result<(), String> {
    match computer_gb() {
        Some(c) if !(MIN_GB..=c.max(MIN_GB)).contains(&gb) =>
            Err(format!("memory for agent runs may be between {MIN_GB} and {} GB: this computer has {c} GB", c.max(MIN_GB))),
        None if gb < MIN_GB => Err(format!("memory for agent runs must be at least {MIN_GB} GB")),
        _ => Ok(()),
    }
}

/// Saves Memory for agent runs, checked with `check` first. While nothing was saved, the default stays unset, so it keeps
/// following this computer's memory. The slice gets the new limit at once (MemoryMax and MemoryHigh together); when that
/// fails the setting is saved anyway, Gizai logs why, and the next start sets it.
pub fn save(st: &AppState, gb: u32) -> Result<(), String> {
    check(gb)?;
    if settings::get::<u32>(&st.db, KEY).ok().flatten().is_some() || gb != default_gb() {
        settings::set(&st.db, KEY, &gb).map_err(|e| e.to_string())?;
    }
    if let Err(e) = st.run_memory.set_limit(u64::from(gb) * GB) {
        eprintln!("gizai: the agent runs' slice keeps its memory limit until Gizai starts again: {e}");
    }
    Ok(())
}

/// Gizai's start: its agent processes start in its own slice from now on (`mem::slice_name` of its data folder), with
/// the limit from Settings → Runs. The slice is set up on a thread of its own now, so the first run rarely waits for it;
/// when it can't be made, processes start as before and Gizai logs why, once. Tests that want the slice call this too.
pub fn start(st: &AppState) {
    st.run_memory.enable(&mem::slice_name(&st.data_dir), u64::from(setting_gb(st)) * GB);
    let st = st.clone();
    std::thread::spawn(move || {
        let _ = st.run_memory.slice();
    });
}

/// The start check, on every system: why a run may not start now for want of memory, or None. A run waits while the
/// agent runs use more than 80% of the limit (Linux with the slice: its memory.stat `anon`, without the file cache) or
/// the computer has less than 2 GB free. The queue tries again as it does: when a run ends, and once a minute.
pub fn wait_reason(st: &AppState) -> Option<String> {
    let limit = limit(st);
    let why = match st.run_memory.reading() {
        mem::Reading { used: Some(used), .. } if u128::from(used) * 100 > u128::from(limit) * u128::from(USED_PERCENT) =>
            Some(format!("Waiting for memory: agent runs use {} of {} GB (Settings → Runs)", tenths(used), whole(limit))),
        mem::Reading { available: Some(free), .. } if free < MIN_FREE =>
            Some(format!("Waiting for memory: this computer has {} GB free, less than {} GB (Settings → Runs)", tenths(free), whole(MIN_FREE))),
        _ => None,
    };
    let mut waiting = st.runs.memory_wait.lock().unwrap();
    let since = waiting.as_ref().map(|(since, _)| *since).unwrap_or_else(ids::now_ms);
    *waiting = why.clone().map(|w| (since, w));
    why
}

/// Since when the start check has been waiting for memory, and why (the board check).
pub fn waiting_since(st: &AppState) -> Option<(i64, String)> {
    st.runs.memory_wait.lock().unwrap().clone()
}

/// Linux ended a process in `agent`'s run on the card `task_id` at the limit: the card's activity says so, as Gizai, with
/// the limit and where to change it.
pub fn ended_on_card(st: &AppState, task_id: &str, agent: &str) {
    let said = format!("saw Linux end a process in {agent}'s run: the agent runs reached their memory limit of {} GB together \
                        (Settings → Runs → Memory for agent runs)", whole(limit(st)));
    if gizai_core::pulls::note_cleanup(&st.db, task_id, &said).is_ok() {
        (st.notify)(crate::runs::Note::RowsChanged("tasks"));
    }
}

/// The run's own note at its end, when Linux ended `count` processes in it at the limit.
pub fn ended_note(st: &AppState, count: u64) -> String {
    let what = if count == 1 { "a process".to_string() } else { format!("{count} processes") };
    format!("Linux ended {what} in this run: Gizai's agent runs reached their memory limit of {} GB together. \
             Change it in Settings → Runs → Memory for agent runs.", whole(limit(st)))
}

/// The run's own note at its end, when Gizai ended processes it left running in its scope, outside its process group
/// (and its preview in the preview's scope): "Ended 1 process this run left running: chromium (pid 12345).", each
/// process once.
pub fn leftovers_note(left: &[Leftover]) -> String {
    format!("Ended {}.", mem::leftovers_text(left, "this run"))
}

/// Gizai quits, once its agents have ended (`quit`): whatever still runs in its slice ends too, SIGTERM and SIGKILL 3 s
/// later, so nothing outlives Gizai: a browser a run started detached, a run that didn't end in time. Only the slice this
/// Gizai set up (none in a test's Gizai, unless the test enabled it); Gizai itself is outside it. Returns what it
/// ended, which Gizai's log names.
pub fn end_leftovers(st: &AppState) -> Vec<Leftover> {
    let Some(slice) = st.run_memory.made() else { return vec![] };
    let left = slice.end_leftovers();
    if !left.is_empty() {
        eprintln!("gizai: quitting ended {}", mem::leftovers_text(&left, "the agent runs"));
    }
    left
}

/// Gizai's start, before it recovers the runs a previous Gizai left (`open_state`): whatever that Gizai left running in
/// the slice of the data folder `data_dir` ends (it crashed, or was killed), its runs' CLIs and what they started, SIGTERM
/// and SIGKILL 3 s later. A Gizai with another data folder has another slice, which this never touches. Linux with a
/// systemd user session; nothing with `GIZAI_RUNS_SLICE=off`. Returns what it ended, which Gizai's log names.
pub fn end_leftovers_at_start(data_dir: &Path) -> Vec<Leftover> {
    let left = mem::end_leftovers_in(&mem::slice_name(data_dir));
    if !left.is_empty() {
        eprintln!("gizai: ended {}", mem::leftovers_text(&left, "a previous Gizai"));
    }
    left
}

/// Bytes as GB with one decimal ("27.3").
fn tenths(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / GB as f64)
}

/// Bytes as whole GB, rounded ("32").
fn whole(bytes: u64) -> u64 {
    (bytes + GB / 2) / GB
}
