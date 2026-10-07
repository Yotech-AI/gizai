//! Agent runtime pieces: Claude Code command line, stream parsing, outcomes, git worktrees and how they are prepared,
//! pull requests (gh), prompts and process control. Never imports Tauri.
pub mod chat_stream;
pub mod claude;
pub mod github;
pub mod models;
pub mod outcome;
pub mod prepare;
pub mod process;
pub mod prompt;
pub mod stream;
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
