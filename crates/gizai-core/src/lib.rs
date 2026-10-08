//! Gizai core: SQLite store, domain and workflow rules. Never imports Tauri.
pub mod db;
pub mod ids;
pub mod seed;
pub mod model;
pub mod clients;
pub mod users;
pub mod projects;
pub mod tasks;
pub mod comments;
pub mod sortkey;
pub mod team;
pub mod docs;
pub mod files;
pub mod runs;
pub mod settings;
pub mod workflow;
pub mod board;
pub mod chat;
pub mod clis;
pub mod tokens;
pub mod repo_url;
pub mod pulls;
pub mod worktrees;
pub mod folders;
mod util;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
