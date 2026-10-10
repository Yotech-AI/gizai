//! The server side of MCP (Model Context Protocol) as Claude Code speaks it over stdio: newline-delimited
//! JSON-RPC 2.0 with `initialize`, `ping`, `tools/list` and `tools/call`. Gizai serves it on a local socket (a
//! named pipe on Windows, see `pipe`); the `gizai-mcp` shim (src/bin) connects Claude Code's stdio to that socket.
//! No Tauri, no database.
use std::future::Future;

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Answered when the client doesn't name a protocol version. Otherwise the server answers with the
/// client's version: the four methods served here are the same in every version so far.
pub const DEFAULT_PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Gizai's tools: clients, projects, tasks, agents, docs, files and the inbox. \
Look items up before changing them; refer to tasks by identifier (KADE-12), projects by key or name.";

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    /// A JSON Schema object for the arguments.
    pub input_schema: Value,
    /// Reads only (shown to the client as `readOnlyHint`).
    pub read_only: bool,
}

/// What the server offers. `call` returns the tool's JSON result, or a sentence saying what went wrong.
pub trait Tools: Send + Sync {
    fn list(&self) -> Vec<ToolDef>;
    fn call(&self, name: &str, args: Value) -> impl Future<Output = Result<Value, String>> + Send;
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Plain text for the model: strings as they are, everything else as compact JSON.
fn text_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Answers one JSON-RPC message. Notifications (no `id`) get no answer.
pub async fn handle<T: Tools>(tools: &T, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return id.map(|id| error(id, -32600, "invalid request"));
    };
    let id = id?;
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(DEFAULT_PROTOCOL);
            ok(id, json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "gizai", "version": env!("CARGO_PKG_VERSION")},
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => ok(id, json!({})),
        "tools/list" => {
            let list: Vec<Value> = tools.list().into_iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
                "annotations": {"readOnlyHint": t.read_only},
            })).collect();
            ok(id, json!({"tools": list}))
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "tools/call needs a tool name"));
            };
            let args = match params.get("arguments") {
                Some(a) if !a.is_null() => a.clone(),
                _ => json!({}),
            };
            let (text, is_error) = match tools.call(name, args).await {
                Ok(v) => (text_of(&v), false),
                Err(e) => (e, true),
            };
            ok(id, json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
        }
        _ => error(id, -32601, &format!("method not found: {method}")),
    })
}

/// Reads JSON-RPC messages, one per line, and writes one line per answer until the reader ends.
pub async fn serve<R, W, T>(reader: R, mut writer: W, tools: &T) -> std::io::Result<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
    T: Tools,
{
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(tools, &msg).await,
            Err(_) => Some(error(Value::Null, -32700, "parse error")),
        };
        if let Some(a) = answer {
            writer.write_all(format!("{a}\n").as_bytes()).await?;
            writer.flush().await?;
        }
    }
    Ok(())
}

/// The shim's first line on the socket: the turn's token.
pub fn hello_line(token: &str) -> String {
    format!("{}\n", json!({"token": token}))
}

/// The token from a hello line, if it is one.
pub fn parse_hello(line: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    v.get("token")?.as_str().map(str::to_string)
}

/// Windows: the named pipe kept to one user. A security descriptor that lets only that user open the pipe, and who runs
/// the process at the other end: Gizai checks each client, the shim checks that the pipe's server is its own user's.
#[cfg(windows)]
pub mod pipe {
    use std::io;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{EqualSid, GetTokenInformation, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};

    /// A user, by the SID in a process's token. Holds the TOKEN_USER that GetTokenInformation wrote, with the SID it
    /// points to (u64s keep it aligned).
    pub struct User(Vec<u64>);

    impl User {
        /// The user this process runs as.
        pub fn current() -> io::Result<User> {
            // SAFETY: GetCurrentProcess returns a pseudo handle that is always valid and needs no closing.
            unsafe { user_of(GetCurrentProcess()) }
        }

        /// The user process `pid` runs as.
        pub fn of_process(pid: u32) -> io::Result<User> {
            // SAFETY: the process handle is checked, and closed before returning.
            unsafe {
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if process.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let user = user_of(process);
                CloseHandle(process);
                user
            }
        }

        fn sid(&self) -> PSID {
            // SAFETY: the buffer starts with the TOKEN_USER that GetTokenInformation wrote (see `user_of`).
            unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
        }

        /// The SID as text, e.g. `S-1-5-21-…-1001`.
        pub fn sid_string(&self) -> io::Result<String> {
            let mut text: *mut u16 = null_mut();
            // SAFETY: the SID is valid; Windows allocates the NUL-ended text with LocalAlloc, freed here.
            unsafe {
                if ConvertSidToStringSidW(self.sid(), &mut text) == 0 {
                    return Err(io::Error::last_os_error());
                }
                let len = (0..).take_while(|&i| *text.add(i) != 0).count();
                let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
                LocalFree(text.cast());
                Ok(sid)
            }
        }
    }

    impl PartialEq for User {
        fn eq(&self, other: &User) -> bool {
            // SAFETY: both SIDs are valid.
            unsafe { EqualSid(self.sid(), other.sid()) != 0 }
        }
    }

    /// The user in a process's token. Safety: `process` is a valid process handle.
    unsafe fn user_of(process: HANDLE) -> io::Result<User> {
        // SAFETY: the token handle is checked, and closed before returning; the buffer is as long as Windows asked.
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            // Asked once for the size, then for the TOKEN_USER itself.
            let mut len = 0u32;
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len);
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            let ok = len > 0 && GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) != 0;
            let err = io::Error::last_os_error();
            CloseHandle(token);
            if ok { Ok(User(buf)) } else { Err(err) }
        }
    }

    /// A security descriptor whose DACL lets one user open the pipe and no one else.
    pub struct OnlyUser(PSECURITY_DESCRIPTOR);

    // SAFETY: the descriptor never changes after it is made; CreateNamedPipeW only reads it.
    unsafe impl Send for OnlyUser {}
    unsafe impl Sync for OnlyUser {}

    impl OnlyUser {
        pub fn new(user: &User) -> io::Result<OnlyUser> {
            // D:P is a protected DACL (nothing inherited); (A;;GA;;;<SID>) allows that user everything. No other entry,
            // so everyone else is denied.
            let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", user.sid_string()?).encode_utf16().chain([0]).collect();
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            // SAFETY: `sddl` ends in a NUL; Windows allocates the descriptor with LocalAlloc, freed in Drop.
            if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(OnlyUser(sd))
        }

        /// The SECURITY_ATTRIBUTES for CreateNamedPipeW (tokio's `create_with_security_attributes_raw`); they point at
        /// this descriptor, so they are good while it lives.
        pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
            SECURITY_ATTRIBUTES { nLength: size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: self.0, bInheritHandle: 0 }
        }
    }

    impl Drop for OnlyUser {
        fn drop(&mut self) {
            // SAFETY: made by ConvertStringSecurityDescriptorToSecurityDescriptorW, freed once.
            unsafe { LocalFree(self.0) };
        }
    }

    /// The user running the client at the other end of a pipe instance Gizai serves (`pipe`: the server's handle).
    ///
    /// # Safety
    /// `pipe` is an open handle (one that is not a pipe only makes the call fail).
    pub unsafe fn client_user(pipe: HANDLE) -> io::Result<User> {
        let mut pid = 0u32;
        // SAFETY: `pipe` is open (the caller's promise).
        if unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        User::of_process(pid)
    }

    /// The user running the server of a pipe the shim opened (`pipe`: the client's handle).
    ///
    /// # Safety
    /// `pipe` is an open handle (one that is not a pipe only makes the call fail).
    pub unsafe fn server_user(pipe: HANDLE) -> io::Result<User> {
        let mut pid = 0u32;
        // SAFETY: `pipe` is open (the caller's promise).
        if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        User::of_process(pid)
    }
}
