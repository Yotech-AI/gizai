//! The models Claude Code offers, exactly as its own /model picker lists them: asked from the installed
//! `claude` with the stream-json `initialize` handshake. No message is sent, so nothing is spent, and the list
//! matches what this user's Claude Code accepts (aliases like `opus`, their full ids, prices, effort levels).
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::AgentError;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    /// What `--model` takes: an alias (`opus`) or `default`.
    pub value: String,
    /// The full model id the alias stands for today (`claude-opus-5-5`).
    pub resolved_model: Option<String>,
    pub display_name: String,
    /// One line, with the price, e.g. "Opus 5.5 · Best for everyday, complex tasks · $4/$20 per Mtok".
    pub description: String,
    pub supports_effort: bool,
    /// What `--effort` takes for this model (empty when it takes none).
    pub effort_levels: Vec<String>,
}

const REQUEST_ID: &str = "gizai-models";

/// The model list from one line of Claude Code's output, if it is the answer to `request_id`.
pub fn parse_models(line: &str, request_id: &str) -> Option<Vec<ModelOption>> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if v.get("type")?.as_str()? != "control_response" {
        return None;
    }
    let r = v.get("response")?;
    if r.get("request_id")?.as_str()? != request_id || r.get("subtype")?.as_str()? != "success" {
        return None;
    }
    let s = |m: &Value, k: &str| m.get(k).and_then(Value::as_str).map(str::to_string);
    Some(r.pointer("/response/models")?.as_array()?.iter().filter_map(|m| {
        Some(ModelOption {
            value: s(m, "value")?,
            resolved_model: s(m, "resolvedModel"),
            display_name: s(m, "displayName").unwrap_or_default(),
            description: s(m, "description").unwrap_or_default(),
            supports_effort: m.get("supportsEffort").and_then(Value::as_bool).unwrap_or(false),
            effort_levels: m.get("supportedEffortLevels").and_then(Value::as_array)
                .map(|l| l.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
        })
    }).collect())
}

/// Starts `claude` the way agents run it (no hooks, no skills, no MCP servers, no saved session), asks for its
/// model list and lets it exit. Takes a second or two.
pub async fn fetch_models(bin: &Path, cwd: &Path) -> Result<Vec<ModelOption>, AgentError> {
    fetch_models_with_env(bin, cwd, &[]).await
}

/// The same for a Claude Code that runs with its own environment (a second account's CLAUDE_CONFIG_DIR).
pub async fn fetch_models_with_env(bin: &Path, cwd: &Path, env: &[(String, String)]) -> Result<Vec<ModelOption>, AgentError> {
    let mut child = crate::os::tokio_command(bin)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence",
               "--setting-sources", "user", "--settings", r#"{"disableAllHooks":true}"#, "--disable-slash-commands", "--strict-mcp-config"])
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| AgentError::Spawn(format!("{}: {e}", bin.display())))?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let ask = format!("{}\n", serde_json::json!({"type": "control_request", "request_id": REQUEST_ID, "request": {"subtype": "initialize"}}));
    let _ = stdin.write_all(ask.as_bytes()).await;
    let _ = stdin.flush().await;
    let read = async {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(models) = parse_models(&line, REQUEST_ID) {
                return Some(models);
            }
        }
        None
    };
    let found = tokio::time::timeout(Duration::from_secs(20), read).await.ok().flatten();
    // Closing stdin ends a print-mode session that got no message; stop it ourselves if it lingers.
    drop(stdin);
    if tokio::time::timeout(Duration::from_secs(3), child.wait()).await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
    match found {
        Some(m) if !m.is_empty() => Ok(m),
        _ => Err(AgentError::Spawn("Claude Code gave no model list (is it installed and up to date?)".into())),
    }
}
