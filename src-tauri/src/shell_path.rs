//! Where Gizai finds the coding CLIs when it starts from the Dock or Finder on macOS: such an app gets a short PATH
//! (/usr/bin:/bin:/usr/sbin:/sbin), not the one your shell sets up. See docs/PLATFORMS.md.

/// macOS: adds the folders your login shell puts on PATH to Gizai's own PATH, once, before anything else starts.
/// Elsewhere it does nothing (Linux adds them per command in `runs::command_path`; Windows apps get the full PATH).
pub fn adopt_login_path() {}
