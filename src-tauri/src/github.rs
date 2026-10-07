//! Settings → GitHub: how Open pull request and Push branch reach GitHub (Push over: SSH with your keys, the default,
//! or HTTPS with the GitHub CLI's login). Gizai never stores a token or password: it uses gh's login and your SSH keys.
use std::path::Path;

use gizai_agents::connection::PushOver;
use gizai_core::settings;

use crate::AppState;

/// The setting that says how pushes go.
const PUSH_OVER: &str = "github_push_over";

/// The ways a push can go, as Settings keeps them.
pub const PUSH_OVER_NAMES: [&str; 2] = ["ssh", "https"];

/// Settings → GitHub → Push over: "ssh" (the default) or "https".
pub fn push_over_name(st: &AppState) -> String {
    match settings::get::<String>(&st.db, PUSH_OVER).ok().flatten() {
        Some(name) if PUSH_OVER_NAMES.contains(&name.as_str()) => name,
        _ => "ssh".into(),
    }
}

pub(crate) fn save_push_over(st: &AppState, name: &str) -> Result<(), String> {
    if !PUSH_OVER_NAMES.contains(&name) {
        return Err(format!("pushes go over ssh or https, not {name:?}"));
    }
    settings::set(&st.db, PUSH_OVER, &name).map_err(|e| e.to_string())
}

/// How a push goes now; over HTTPS with the login of `gh` (the GitHub CLI Gizai uses).
pub(crate) fn push_over(st: &AppState, gh: &Path) -> PushOver {
    match push_over_name(st).as_str() {
        "https" => PushOver::Https { gh: gh.to_path_buf() },
        _ => PushOver::Ssh,
    }
}
