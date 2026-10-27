//! Which settings belong to this computer and which are shared (GA-218). A restore keeps this computer's: its programs
//! and paths, its capacity and state, its desktop notifications and its backups stay as they were, whatever the snapshot
//! says, and only a setting today doesn't have comes from the snapshot. Every other app-wide setting (`org_id` ''), and
//! every org's setting (`org_id` not ''), is shared: it comes from the snapshot.
//!
//! Sync (Gizai Cloud) will send the shared settings only, never this computer's. So a new setting must go on one side:
//! a path, a program, a port or anything else that is only true on one computer goes in THIS_COMPUTER (or starts with
//! one of THIS_COMPUTER_PREFIXES); a setting that is about the work (the team's rules, the limits of a run) is shared,
//! and needs nothing here.

/// This computer's app-wide settings, by key.
pub const THIS_COMPUTER: &[&str] = &[
    // its programs and paths
    "claude_bin",
    "gh_bin",
    "clis",
    "mcp_servers",
    "mcp_tools",
    "mcp_last_run",
    "mcp_browser",
    "cli_tools_seen",
    "cli_tools_asked",
    "editor_roots",
    "github_push_over",
    "claude_memory_imports",
    // its capacity and state
    "runs_memory_gb",
    "max_concurrent_runs",
    "agents_paused",
    "subscription_limits",
    "onboarding",
    "update_auto_check",
    "update_last_check",
    // the ports its previews take: checked against max_concurrent_runs (ten for each run at once)
    "preview_port_from",
    "preview_port_to",
    // its desktop notifications (`notify_*`, below)
    "notify_hold",
    "notify_waiting",
    "notify_lead_asks",
    "notify_lead_answered",
    // its backups (GA-216), and the last one that failed there
    "backup_folder",
    "backup_every",
    "backup_on_quit",
    "backup_keep",
    "backup_last_error",
    // its restores (GA-218)
    LAST_RESTORE,
    RESTORE_RESULT,
];

/// Every app-wide setting whose key starts with one of these is this computer's too: the desktop notifications' switches.
pub const THIS_COMPUTER_PREFIXES: &[&str] = &["notify_"];

/// The snapshot this computer's data came back from, its computer, its last change and the before-restore snapshot
/// (GA-218, docs/backup-format.md): where a sync knows this computer's data starts.
pub const LAST_RESTORE: &str = "last_restore";
/// How the last restore went at the start, for the window, until it is dismissed (GA-218).
pub const RESTORE_RESULT: &str = "restore_result";

/// Whether app-wide setting `key` is this computer's: a restore keeps today's value, and sync never sends it.
pub fn this_computer(key: &str) -> bool {
    THIS_COMPUTER.contains(&key) || THIS_COMPUTER_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// Whether the setting `key` of `org_id` ('' app-wide) is shared: every org's setting is.
pub fn shared(key: &str, org_id: &str) -> bool {
    !org_id.is_empty() || !this_computer(key)
}
