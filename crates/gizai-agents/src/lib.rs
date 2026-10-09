//! Agent runtime pieces: the coding CLIs (Claude Code, Codex, Gemini, others) and their command lines, stream parsing, outcomes, git worktrees and how they are prepared,
//! the Team Lead's read-only copies of the projects' code,
//! pull requests (gh, and Bitbucket's REST API), the connection to GitHub and Bitbucket (pushes over SSH or HTTPS, gh's
//! login), prompts and process control,
//! and Gizai's own updates (the release check, building and installing a release). Never imports Tauri.
pub mod bitbucket;
pub mod chat_stream;
pub mod checkout;
pub mod claude;
pub mod cli;
pub mod connection;
pub mod copies;
pub mod github;
pub mod mcp_client;
pub mod mcp_import;
pub mod mcp_run;
pub mod mcp_tools;
pub mod models;
pub mod oauth;
pub mod os;
pub mod outcome;
pub mod prepare;
pub mod process;
pub mod prompt;
pub mod secrets;
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
