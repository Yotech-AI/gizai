//! Gizai's MCP server for chat turns, on a user-only unix socket. Claude Code starts the `gizai-mcp` shim,
//! which connects here and sends the turn's token as its first line; with a valid token the connection gets
//! the Gizai tools, acting as the token's agent. No TCP port, no web server.
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::AppState;
use crate::tools::GizaiTools;

/// `$XDG_RUNTIME_DIR/gizai/<12 hex of sha256(data dir)>.sock`, else `<data dir>/mcp.sock`. Per data dir, so a
/// test Gizai with its own data never touches the socket of the Gizai you use.
pub fn socket_path(data_dir: &Path) -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir());
    socket_path_in(runtime.as_deref(), data_dir)
}

/// Unix socket paths must fit in 108 bytes; stay well under.
const MAX_SOCKET_PATH: usize = 100;

pub fn socket_path_in(runtime_dir: Option<&Path>, data_dir: &Path) -> PathBuf {
    let name = format!("{}.sock", crate::data_dir_key(data_dir));
    let wanted = match runtime_dir {
        Some(run) => run.join("gizai").join(&name),
        None => data_dir.join("mcp.sock"),
    };
    if wanted.as_os_str().len() < MAX_SOCKET_PATH {
        return wanted;
    }
    // Too long for a socket: a private folder in the temp dir, still one socket per data dir.
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("gizai-{uid}")).join(name)
}

/// The `gizai-mcp` shim: `$GIZAI_MCP_BIN`, else next to Gizai's own binary.
pub fn shim_bin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GIZAI_MCP_BIN").map(PathBuf::from) {
        return Some(p).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("gizai-mcp")).filter(|p| p.is_file())
}

/// Binds the socket and serves connections until the app ends. A socket file left by an earlier Gizai is
/// replaced; any other file in the way is an error (never deleted).
pub fn start(st: &AppState) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let path = st.mcp_socket.clone();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
        // A folder someone else made (e.g. in /tmp) could swap the socket: refuse it.
        use std::os::unix::fs::MetadataExt;
        // SAFETY: getuid has no preconditions.
        if std::fs::metadata(parent)?.uid() != unsafe { libc::getuid() } {
            return Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, format!("{} belongs to another user", parent.display())));
        }
    }
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_socket() {
            std::fs::remove_file(&path)?;
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} exists and is not a socket", path.display())));
        }
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let st = st.clone();
    Ok(tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                // e.g. too many open files: don't spin
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            };
            let st = st.clone();
            tokio::spawn(async move { connection(st, stream).await });
        }
    }))
}

/// Removes the socket file (when Gizai quits).
pub fn remove_socket(st: &AppState) {
    if std::fs::symlink_metadata(&st.mcp_socket).is_ok_and(|m| m.file_type().is_socket()) {
        let _ = std::fs::remove_file(&st.mcp_socket);
    }
}

async fn refuse(mut w: tokio::net::unix::OwnedWriteHalf, why: &str) {
    let _ = w.write_all(format!("{}\n", serde_json::json!({"error": why})).as_bytes()).await;
    let _ = w.shutdown().await;
}

async fn connection(st: AppState, stream: UnixStream) {
    // Only processes of the same user (the directory is 0700 too).
    // SAFETY: getuid has no preconditions.
    let me = unsafe { libc::getuid() };
    let same_user = stream.peer_cred().map(|c| c.uid() == me).unwrap_or(false);
    let (r, w) = stream.into_split();
    if !same_user {
        return refuse(w, "this socket is for the user running Gizai").await;
    }
    let mut reader = BufReader::new(r);
    let mut hello = String::new();
    match tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut hello)).await {
        Ok(Ok(n)) if n > 0 => {}
        _ => return refuse(w, "expected a hello line with the turn's token").await,
    }
    let Some(token) = gizai_mcp::parse_hello(&hello) else {
        return refuse(w, "expected a hello line with the turn's token").await;
    };
    let grant = match gizai_core::tokens::verify(&st.db, &token) {
        Ok(Some(g)) => g,
        _ => return refuse(w, "this token is not valid: the chat turn has ended or it was never issued").await,
    };
    let thread = grant.scope.get("chat").and_then(|v| v.as_str()).map(str::to_string);
    let check = grant.scope.get("check").and_then(|v| v.as_str()).map(str::to_string);
    let tools = GizaiTools { st, actor: grant.actor_id, thread, check };
    let _ = gizai_mcp::serve(reader, w, &tools).await;
}
