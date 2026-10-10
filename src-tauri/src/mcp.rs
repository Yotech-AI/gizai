//! Gizai's MCP server for chat turns, on a user-only local socket: a unix socket on Linux and macOS, a named pipe on
//! Windows. Claude Code starts the `gizai-mcp` shim, which connects here and sends the turn's token as its first line;
//! with a valid token the connection gets the Gizai tools, acting as the token's agent. No TCP port, no web server.
//!
//! The unix socket is 0600, in a folder of the user's (0700 when Gizai makes it), and serves only processes of the same
//! user. The named pipe is kept to the user just as much: Gizai makes its first instance, so if another program already
//! holds the name Gizai refuses to start the tools rather than share it; its DACL allows the user running Gizai (their
//! SID) and no one else, with nothing inherited; remote clients are refused; and each client's process must run as that
//! same user.
#[cfg(unix)]
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
#[cfg(windows)]
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

use crate::AppState;
use crate::tools::GizaiTools;

/// `$XDG_RUNTIME_DIR/gizai/<12 hex of sha256(data dir)>.sock`, else `<data dir>/mcp.sock` (so on macOS, which has no
/// XDG_RUNTIME_DIR). Per data dir, so a test Gizai with its own data never touches the socket of the Gizai you use.
#[cfg(unix)]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).filter(|p| p.is_dir());
    socket_path_in(runtime.as_deref(), data_dir)
}

/// Windows: the named pipe `\\.\pipe\gizai-<8 hex of sha256(the user's SID)>-<12 hex of sha256(data dir)>`. Pipe names
/// are shared by everyone on the PC, so the user is in it too; per data dir like the socket.
#[cfg(windows)]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    let user = gizai_mcp::pipe::User::current().and_then(|u| u.sid_string()).unwrap_or_else(|_| std::env::var("USERNAME").unwrap_or_default());
    let user = gizai_core::tokens::sha256_hex(&user);
    PathBuf::from(format!(r"\\.\pipe\gizai-{}-{}", &user[..8], crate::data_dir_key(data_dir)))
}

/// Unix socket paths must fit in 108 bytes (104 on macOS); stay well under.
#[cfg(unix)]
const MAX_SOCKET_PATH: usize = 100;

#[cfg(unix)]
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

/// The `gizai-mcp` shim (`gizai-mcp.exe` on Windows): `$GIZAI_MCP_BIN`, else next to Gizai's own binary.
pub fn shim_bin() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GIZAI_MCP_BIN").map(PathBuf::from) {
        return Some(p).filter(|p| p.is_file());
    }
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(format!("gizai-mcp{}", std::env::consts::EXE_SUFFIX))).filter(|p| p.is_file())
}

/// Binds the socket and serves connections until the app ends. A socket file left by an earlier Gizai is
/// replaced; any other file in the way is an error (never deleted).
#[cfg(unix)]
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

/// Windows: makes the named pipe and serves connections until the app ends. If another program already made a pipe by
/// this name, it is an error (Gizai doesn't share the name), like a file in the socket's way.
#[cfg(windows)]
pub fn start(st: &AppState) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let me = Arc::new(gizai_mcp::pipe::User::current()?);
    let only_me = gizai_mcp::pipe::OnlyUser::new(&me)?;
    let name = st.mcp_socket.clone();
    let mut server = pipe_instance(&name, &only_me, true).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => std::io::Error::new(e.kind(), format!("{} is taken by another program ({e})", name.display())),
        _ => e,
    })?;
    let st = st.clone();
    Ok(tokio::spawn(async move {
        loop {
            let connected = server.connect().await;
            // The next instance is made before this one is handed on: the name is never free for another program.
            let next = loop {
                match pipe_instance(&name, &only_me, false) {
                    Ok(next) => break next,
                    // e.g. out of resources: don't spin
                    Err(_) => tokio::time::sleep(Duration::from_millis(200)).await,
                }
            };
            let pipe = std::mem::replace(&mut server, next);
            if connected.is_ok() {
                tokio::spawn(pipe_connection(st.clone(), pipe, me.clone()));
            } else {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }))
}

/// One instance of the pipe, with the DACL that lets only the user in; `first` claims the name, and fails if a pipe by
/// that name exists already. Remote clients are refused.
#[cfg(windows)]
fn pipe_instance(name: &Path, only_me: &gizai_mcp::pipe::OnlyUser, first: bool) -> std::io::Result<NamedPipeServer> {
    let mut attrs = only_me.attributes();
    // SAFETY: `attrs` is a valid SECURITY_ATTRIBUTES whose descriptor `only_me` keeps alive; Windows copies it.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(name, (&raw mut attrs).cast())
    }
}

/// Removes the socket file (when Gizai quits).
#[cfg(unix)]
pub fn remove_socket(st: &AppState) {
    if std::fs::symlink_metadata(&st.mcp_socket).is_ok_and(|m| m.file_type().is_socket()) {
        let _ = std::fs::remove_file(&st.mcp_socket);
    }
}

/// Windows: nothing to remove; the pipe goes when Gizai's last instance of it closes.
#[cfg(windows)]
pub fn remove_socket(_st: &AppState) {}

async fn refuse<W: AsyncWrite + Unpin>(mut w: W, why: &str) {
    let _ = w.write_all(format!("{}\n", serde_json::json!({"error": why})).as_bytes()).await;
    let _ = w.shutdown().await;
}

#[cfg(unix)]
async fn connection(st: AppState, stream: UnixStream) {
    // Only processes of the same user (the directory is 0700 too).
    // SAFETY: getuid has no preconditions.
    let me = unsafe { libc::getuid() };
    let same_user = stream.peer_cred().map(|c| c.uid() == me).unwrap_or(false);
    let (r, w) = stream.into_split();
    serve_turn(st, r, w, same_user).await
}

#[cfg(windows)]
async fn pipe_connection(st: AppState, pipe: NamedPipeServer, me: Arc<gizai_mcp::pipe::User>) {
    // Only processes of the same user (the pipe's DACL lets no one else open it in the first place).
    // SAFETY: the handle is open while `pipe` lives.
    let client = unsafe { gizai_mcp::pipe::client_user(pipe.as_raw_handle()) };
    let same_user = client.is_ok_and(|u| u == *me);
    let (r, w) = tokio::io::split(pipe);
    serve_turn(st, r, w, same_user).await
}

/// One connection, the same on every system: the hello line with the turn's token, then the tools as its agent.
async fn serve_turn<R, W>(st: AppState, r: R, w: W, same_user: bool)
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
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
