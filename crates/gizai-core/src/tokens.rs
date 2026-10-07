//! Short-lived tokens for the MCP socket: one per chat turn, so the tools know which agent acts and nothing
//! else on the machine can borrow them. Only the sha256 is stored; the token itself lives in the turn's
//! 0600 MCP config file and Claude Code's environment, and dies with the turn (revoked or expired).
use std::io::Read;

use rusqlite::OptionalExtension;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::db::Db;
use crate::{Result, ids};

#[derive(Debug, Clone, PartialEq)]
pub struct TokenGrant {
    pub actor_id: String,
    pub scope: Value,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// sha256 of a string, as 64 hex characters.
pub fn sha256_hex(s: &str) -> String {
    hex(&Sha256::digest(s.as_bytes()))
}

fn hash(token: &str) -> String {
    sha256_hex(token)
}

/// A new random token (64 hex characters) for `actor_id`, valid for `ttl_ms`.
pub fn mint(db: &Db, actor_id: &str, scope: Value, ttl_ms: i64) -> Result<String> {
    let mut raw = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut raw)?;
    let token = hex(&raw);
    let now = ids::now_ms();
    db.write(None, |w| {
        w.conn().execute(
            "INSERT INTO api_tokens(id, created_at, actor_id, token_sha256, scopes_json, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![ids::new_id(), now, actor_id, hash(&token), scope.to_string(), now + ttl_ms],
        )?;
        Ok(())
    })?;
    Ok(token)
}

/// Who the token speaks for, if it exists, isn't revoked and hasn't expired.
pub fn verify(db: &Db, token: &str) -> Result<Option<TokenGrant>> {
    let now = ids::now_ms();
    db.read(|c| {
        let row: Option<(String, String)> = c.query_row(
            "SELECT actor_id, scopes_json FROM api_tokens WHERE token_sha256=?1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?2)",
            rusqlite::params![hash(token), now], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        Ok(row.map(|(actor_id, scope)| TokenGrant { actor_id, scope: serde_json::from_str(&scope).unwrap_or(Value::Null) }))
    })
}

pub fn revoke(db: &Db, token: &str) -> Result<()> {
    db.write(None, |w| {
        w.conn().execute("UPDATE api_tokens SET revoked_at=?2 WHERE token_sha256=?1 AND revoked_at IS NULL", rusqlite::params![hash(token), ids::now_ms()])?;
        Ok(())
    })
}
