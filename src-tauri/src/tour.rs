//! GA-148, the product tour (scripts/tour-capture.sh): the steps self-test takes stills of the screen before and after
//! each click, and writes down what it clicked. Only when Gizai starts with a GIZAI_SELFTEST_MODE and GIZAI_TOUR_DIR
//! (the headless cage of the capture): without them both commands do nothing but say so.
use std::path::PathBuf;

/// The folder the stills and steps.json go to, while a tour is captured.
fn dir() -> Result<PathBuf, String> {
    let mode = std::env::var("GIZAI_SELFTEST_MODE").unwrap_or_default();
    match std::env::var_os("GIZAI_TOUR_DIR") {
        Some(d) if !d.is_empty() && !mode.is_empty() => Ok(PathBuf::from(d)),
        _ => Err("no tour is being captured".into()),
    }
}

/// A still's file name: letters, digits, - and _ only, so a name never leaves the folder.
fn still_name(name: &str) -> Result<String, String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(format!("not a still's name: {name:?}"));
    }
    Ok(format!("{name}.png"))
}

/// Takes a still of the whole screen with grim, as `<name>.png` in the tour's folder. The script crops the window's
/// title bar off afterwards.
#[tauri::command]
pub async fn tour_still(name: String) -> Result<(), String> {
    let path = dir()?.join(still_name(&name)?);
    let out = tauri::async_runtime::spawn_blocking(move || gizai_agents::os::command("grim").arg(&path).output())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("grim: {e}"))?;
    if !out.status.success() {
        return Err(format!("grim: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}

/// Writes the steps, their stills and the boxes of what they clicked, as `steps.json` in the tour's folder: the capture
/// waits for this file, then quits Gizai.
#[tauri::command]
pub fn tour_steps(json: String) -> Result<(), String> {
    let dir = dir()?;
    let tmp = dir.join("steps.json.part");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join("steps.json")).map_err(|e| e.to_string())
}

