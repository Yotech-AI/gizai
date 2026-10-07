//! Agent runtime pieces: Claude Code command line, stream parsing, outcomes, git worktrees and how they are prepared,
//! pull requests (gh), the connection to GitHub (pushes over SSH or HTTPS, gh's login), prompts and process control,
//! and Gizai's own updates (the release check, building and installing a release). Never imports Tauri.
pub mod chat_stream;
pub mod claude;
pub mod connection;
pub mod github;
pub mod models;
pub mod outcome;
pub mod prepare;
pub mod process;
pub mod prompt;
pub mod stream;
pub mod update;
pub mod worktree;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("{0} is not a git repository")]
    NotGitRepo(PathBuf),
    #[error("git: {0}")]
    Git(String),
    #[error("could not start the agent: {0}")]
    Spawn(String),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
}
