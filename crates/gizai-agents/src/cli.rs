//! The coding CLIs an agent can run on: Claude Code, Codex, Gemini, or any other program. For each kind, the command
//! line of one headless task run and how its output turns into the same `RunEvent`s the Run panel shows.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde_json::Value;

use crate::claude::ClaudeArgs;
use crate::process::Exec;
use crate::stream::{self, RunEvent, cut, summarize};

/// How Gizai starts a CLI and reads what it writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `claude -p --output-format stream-json`
    ClaudeCode,
    /// `codex exec --json`
    Codex,
    /// `gemini --output-format stream-json`
    Gemini,
    /// Any other program: the prompt as an argument or on stdin, its output read as plain text.
    Other,
}

impl Kind {
    pub fn parse(key: &str) -> Option<Kind> {
        match key {
            "claude_code" => Some(Kind::ClaudeCode),
            "codex" => Some(Kind::Codex),
            "gemini" => Some(Kind::Gemini),
            "other" => Some(Kind::Other),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Kind::ClaudeCode => "claude_code",
            Kind::Codex => "codex",
            Kind::Gemini => "gemini",
            Kind::Other => "other",
        }
    }

    /// Whether a stopped run can continue its session (a plain-text CLI has none Gizai knows).
    pub fn can_resume(self) -> bool {
        self != Kind::Other
    }
}

/// One CLI as Gizai starts it.
#[derive(Debug, Clone)]
pub struct CliSpec {
    pub kind: Kind,
    pub bin: PathBuf,
    /// Set on top of Gizai's own environment, e.g. CLAUDE_CONFIG_DIR for a second Claude Code account.
    pub env: Vec<(String, String)>,
    /// Other only: its arguments. `{prompt}` is replaced by the prompt (else the prompt goes in on stdin) and
    /// `{model}` by the agent's model (without a model, that argument is left out, with the option before it in
    /// `-m {model}`, alone in `--model={model}`).
    pub args: String,
}

/// One task run, whatever the CLI.
#[derive(Debug, Clone, Default)]
pub struct TaskRun {
    pub prompt: String,
    /// The session to start (Claude Code, Gemini) or to resume.
    pub session_id: String,
    pub resume: bool,
    pub model: Option<String>,
    pub effort: Option<String>,
    /// In the CLI's own terms: a Claude Code permission mode, a Codex sandbox or a Gemini approval mode.
    pub permission_mode: String,
    /// Claude Code style, e.g. `Bash(git status:*)`.
    pub allowed_tools: Vec<String>,
    pub max_budget_usd: Option<f64>,
    /// Folders outside the worktree the agent must be able to write: the repository's git folder, where a worktree's
    /// commits go (Codex's sandbox only lets it write the worktree).
    pub writable_dirs: Vec<String>,
    /// The agent's own folders (agent form → Folders) that are there: see `task_exec` for what each CLI gets.
    pub folders: Vec<RunFolder>,
    /// The run's temp folder (`<worktree>/.gizai-tmp`, `worktree::prepare_temp`): every CLI gets it as TMPDIR, TMP and
    /// TEMP. None when Gizai couldn't make it.
    pub temp_dir: Option<String>,
    /// Claude Code: the run's MCP config file (`mcp_run`), with the agent's MCP servers on.
    pub mcp_config: Option<PathBuf>,
    /// Claude Code: more tools to refuse, like the MCP tools switched off (`mcp_run::permissions`).
    pub disallowed_tools: Vec<String>,
}

/// One of the agent's folders for a run (absolute, as it is on disk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFolder {
    pub path: String,
    /// Read and change; else read only.
    pub change: bool,
}

/// Claude Code's deny rules that keep the read folders read only: `Edit(//<path>/**)` and `Write(//<path>/**)`. In a
/// permission rule `//` starts an absolute path (a single `/` is relative to the settings).
pub fn claude_read_only(folders: &[RunFolder]) -> Vec<String> {
    folders.iter().filter(|f| !f.change)
        .flat_map(|f| { let p = f.path.trim_end_matches('/').to_string(); ["Edit", "Write"].map(|t| format!("{t}(/{p}/**)")) })
        .collect()
}

/// What a CLI can't be given of the agent's folders, as a note for the run log: Gemini can't keep a folder read only,
/// so it gets only the read and change ones, and an Other CLI gets none.
pub fn folders_left_out(kind: Kind, folders: &[RunFolder]) -> Option<String> {
    let (left, why) = match kind {
        Kind::Gemini => (folders.iter().filter(|f| !f.change).collect::<Vec<_>>(), "Gemini can't keep a folder read only"),
        Kind::Other => (folders.iter().collect(), "this CLI can't be given folders"),
        Kind::ClaudeCode | Kind::Codex => (vec![], ""),
    };
    let paths: Vec<&str> = left.iter().map(|f| f.path.as_str()).collect();
    (!paths.is_empty()).then(|| format!("Left out the folder{} {}: {why}.", if paths.len() == 1 { "" } else { "s" }, paths.join(", ")))
}

/// The command for one task run on `cli`. The agent's folders:
/// - Claude Code: `--add-dir` for each, and deny rules for `Edit` and `Write` in each read folder (`claude_read_only`);
/// - Codex: its sandbox reads every folder anyway; a read and change folder becomes a writable root (workspace-write);
/// - Gemini: `--include-directories` for each read and change folder; read folders are left out;
/// - Other: none.
///
/// Every CLI gets the run's temp folder as TMPDIR, TMP and TEMP, after its own environment lines.
pub fn task_exec(cli: &CliSpec, run: &TaskRun) -> Exec {
    let mut exec = cli_exec(cli, run);
    if let Some(dir) = &run.temp_dir {
        exec.env.extend(crate::worktree::temp_env(std::path::Path::new(dir)));
    }
    exec
}

fn cli_exec(cli: &CliSpec, run: &TaskRun) -> Exec {
    match cli.kind {
        Kind::ClaudeCode => ClaudeArgs {
            bin: cli.bin.clone(), prompt: run.prompt.clone(), session_id: run.session_id.clone(), resume: run.resume,
            permission_mode: if run.permission_mode.is_empty() { "acceptEdits".into() } else { run.permission_mode.clone() },
            allowed_tools: run.allowed_tools.clone(), model: run.model.clone(), max_budget_usd: run.max_budget_usd,
            // Your own hooks (e.g. a SessionStart hook) and plugin skills (e.g. superpowers) are for your sessions, not
            // for headless agents.
            disable_hooks: true, disable_skills: true, effort: run.effort.clone(), env: cli.env.clone(),
            add_dirs: run.folders.iter().map(|f| f.path.clone()).collect(),
            disallowed_tools: claude_read_only(&run.folders).into_iter().chain(run.disallowed_tools.iter().cloned()).collect(),
            mcp_config: run.mcp_config.clone(),
            ..Default::default()
        }.exec(),
        Kind::Codex => Exec { bin: cli.bin.clone(), args: codex_args(run), env: cli.env.clone(), stdin: run.prompt.clone() },
        Kind::Gemini => Exec { bin: cli.bin.clone(), args: gemini_args(run), env: cli.env.clone(), stdin: run.prompt.clone() },
        Kind::Other => {
            let (args, on_stdin) = other_args(&cli.args, &run.prompt, run.model.as_deref());
            Exec { bin: cli.bin.clone(), args, env: cli.env.clone(), stdin: if on_stdin { run.prompt.clone() } else { String::new() } }
        }
    }
}

/// A TOML string for `codex -c key=value`.
fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `codex exec --json … -` (a new session) or `codex exec resume --json … <session> -`; the prompt comes on stdin.
/// The sandbox is set with `-c`, which both forms take: "workspace-write" (the default) may write the worktree and the
/// repository's git folder and use the network (for fetches and pushes), "read-only" changes nothing, and
/// "danger-full-access" runs without a sandbox.
fn codex_args(run: &TaskRun) -> Vec<String> {
    let mut a: Vec<String> = vec!["exec".into()];
    if run.resume {
        a.push("resume".into());
    }
    a.push("--json".into());
    if let Some(m) = &run.model {
        a.extend(["-m".into(), m.clone()]);
    }
    if let Some(e) = &run.effort {
        a.extend(["-c".into(), format!("model_reasoning_effort={}", toml_str(e))]);
    }
    a.extend(["-c".into(), r#"approval_policy="never""#.into()]);
    match run.permission_mode.as_str() {
        "danger-full-access" => a.push("--dangerously-bypass-approvals-and-sandbox".into()),
        "read-only" => a.extend(["-c".into(), r#"sandbox_mode="read-only""#.into()]),
        _ => {
            a.extend(["-c".into(), r#"sandbox_mode="workspace-write""#.into()]);
            a.extend(["-c".into(), "sandbox_workspace_write.network_access=true".into()]);
            // The repository's git folder, and the agent's read and change folders.
            let roots: Vec<String> = run.writable_dirs.iter().chain(run.folders.iter().filter(|f| f.change).map(|f| &f.path))
                .map(|d| toml_str(d)).collect();
            if !roots.is_empty() {
                a.extend(["-c".into(), format!("sandbox_workspace_write.writable_roots=[{}]", roots.join(","))]);
            }
        }
    }
    if run.resume {
        a.push(run.session_id.clone());
    }
    a.push("-".into());
    a
}

/// A Claude Code allowed command as a Gemini allowed tool: `Bash(git status:*)` → `run_shell_command(git status)`.
/// Gemini matches a shell command by its start. Other tools have other names in Gemini, so they are left out.
pub fn gemini_tool(claude_tool: &str) -> Option<String> {
    let inner = claude_tool.trim().strip_prefix("Bash(")?.strip_suffix(')')?;
    let cmd = inner.strip_suffix(":*").or_else(|| inner.strip_suffix('*')).unwrap_or(inner).trim();
    (!cmd.is_empty()).then(|| format!("run_shell_command({cmd})"))
}

/// `gemini --output-format stream-json …`, headless; the prompt comes on stdin and `-p` only adds a last line after it.
/// The approval mode: "auto_edit" (the default: edits, plus the allowed commands), "yolo" (anything), "plan" (read-only)
/// or "default" (asks, which a headless run can't, so it is refused).
fn gemini_args(run: &TaskRun) -> Vec<String> {
    let mut a: Vec<String> = ["--output-format", "stream-json", "--skip-trust"].iter().map(|s| s.to_string()).collect();
    if let Some(m) = &run.model {
        a.extend(["--model".into(), m.clone()]);
    }
    let mode = if run.permission_mode.is_empty() { "auto_edit" } else { run.permission_mode.as_str() };
    a.extend(["--approval-mode".into(), mode.into()]);
    a.extend([if run.resume { "--resume" } else { "--session-id" }.into(), run.session_id.clone()]);
    for t in run.allowed_tools.iter().filter_map(|t| gemini_tool(t)) {
        a.push(format!("--allowed-tools={t}"));
    }
    // Gemini's file tools may use these folders as well as the worktree, read and write: it has no read-only folder.
    for f in run.folders.iter().filter(|f| f.change) {
        a.push(format!("--include-directories={}", f.path));
    }
    a.extend(["-p".into(), "Do the work described above.".into()]);
    a
}

/// Splits arguments the way a shell would for plain words and '…' or "…" quotes (no variables, no globs).
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => { if let Some(n) = chars.next() { cur.push(n); } }
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => { quote = Some(c); started = true; }
            (None, '\\') => { if let Some(n) = chars.next() { cur.push(n); started = true; } }
            (None, c) if c.is_whitespace() => {
                if started || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                started = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if started || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// An Other CLI's arguments, and whether the prompt goes in on stdin (when no argument takes `{prompt}`).
pub fn other_args(template: &str, prompt: &str, model: Option<&str>) -> (Vec<String>, bool) {
    let mut out: Vec<String> = vec![];
    let mut on_stdin = true;
    for t in split_args(template) {
        if t.contains("{model}") {
            match model {
                Some(m) => out.push(t.replace("{model}", m)),
                // No model: leave out this argument, and the option before it when that takes it as its value
                // (`-m {model}`). An option that holds it itself (`--model={model}`) goes alone.
                None => if !t.starts_with('-') && out.last().is_some_and(|p| p.starts_with('-') && !p.contains('=')) { out.pop(); },
            }
            continue;
        }
        if t.contains("{prompt}") {
            on_stdin = false;
        }
        out.push(t.replace("{prompt}", prompt));
    }
    (out, on_stdin)
}

// ---- reading the output ----

/// The first line of a run log that isn't Claude Code's: which parser reads the rest.
pub fn log_header(kind: Kind) -> String {
    serde_json::json!({"type": "gizai_cli", "kind": kind.key()}).to_string()
}

fn header_kind(line: &str) -> Option<Kind> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    (v.get("type")?.as_str()? == "gizai_cli").then(|| Kind::parse(v.get("kind")?.as_str()?)).flatten()
}

/// A line of Gizai's own in a run log (after the header, before the CLI's output): a note the Run panel shows.
pub fn note_line(text: &str) -> String {
    serde_json::json!({"type": "gizai_note", "text": text}).to_string()
}

fn note_of(line: &str) -> Option<RunEvent> {
    if !line.contains("\"gizai_note\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    (v.get("type")?.as_str()? == "gizai_note").then(|| RunEvent::Note { text: text_of(&v, "text") })
}

/// A finished run's log → its events. A log without a header is Claude Code's.
pub fn parse_log(text: &str) -> Vec<RunEvent> {
    let mut lines = text.lines().peekable();
    let kind = match lines.peek().and_then(|l| header_kind(l)) {
        Some(k) => { lines.next(); k }
        None => Kind::ClaudeCode,
    };
    let mut p = Parser::new(kind);
    let mut out: Vec<RunEvent> = lines.flat_map(|l| note_of(l).map(|n| vec![n]).unwrap_or_else(|| p.line(l))).collect();
    out.extend(p.finish());
    out
}

/// Turns one CLI's output into `RunEvent`s, line by line; `finish` gives what is still due when the output ends.
pub enum Parser {
    Claude(Claude),
    Codex(Codex),
    Gemini(Gemini),
    Text(Text),
}

impl Parser {
    pub fn new(kind: Kind) -> Parser {
        match kind {
            Kind::ClaudeCode => Parser::Claude(Claude::default()),
            Kind::Codex => Parser::Codex(Codex::default()),
            Kind::Gemini => Parser::Gemini(Gemini::default()),
            Kind::Other => Parser::Text(Text::default()),
        }
    }

    pub fn line(&mut self, line: &str) -> Vec<RunEvent> {
        match self {
            Parser::Claude(p) => p.line(line),
            Parser::Codex(p) => p.line(line),
            Parser::Gemini(p) => p.line(line),
            Parser::Text(p) => p.line(line),
        }
    }

    pub fn finish(&mut self) -> Vec<RunEvent> {
        match self {
            Parser::Claude(_) => vec![],
            Parser::Codex(p) => p.finish(),
            Parser::Gemini(p) => p.finish(),
            Parser::Text(p) => p.finish(),
        }
    }
}

/// Claude Code's stream-json (`stream::parse_line`), and each refused tool call as it happens: a `system` line with
/// subtype `permission_denied` names the call by its id and says why, and the call's own `tool_use` block said what it
/// asked for. The result line's `permission_denials` then adds only the calls not shown yet, so a run stopped before
/// its result (Stop, a limit) keeps the refusals it had.
#[derive(Default)]
pub struct Claude {
    /// What each tool call so far asked for, by its id: (tool, command or file).
    asked: HashMap<String, (String, String)>,
    /// The refused calls already shown, by id.
    refused: HashSet<String>,
}

impl Claude {
    pub fn line(&mut self, line: &str) -> Vec<RunEvent> {
        let mut evs = stream::parse_line(line);
        if line.contains("\"mcp_servers\"") {
            evs.extend(mcp_states(line));
        }
        let watch = ["\"tool_use\"", "\"permission_denied\"", "\"permission_denials\""];
        if !watch.iter().any(|w| line.contains(w)) {
            return evs;
        }
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return evs };
        match (v.get("type").and_then(Value::as_str), v.get("subtype").and_then(Value::as_str)) {
            (Some("assistant"), _) => {
                for b in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                    if b.get("type").and_then(Value::as_str) == Some("tool_use") {
                        self.asked.insert(text_of(b, "id"), (text_of(b, "name"), stream::refused_input(&b["input"])));
                    }
                }
            }
            (Some("system"), Some("permission_denied")) => {
                let id = text_of(&v, "tool_use_id");
                if self.refused.insert(id.clone()) || id.is_empty() {
                    let (tool, input) = self.asked.get(&id).cloned().unwrap_or_else(|| (text_of(&v, "tool_name"), String::new()));
                    let reason = [text_of(&v, "decision_reason"), text_of(&v, "message")].into_iter().find(|r| !r.trim().is_empty()).unwrap_or_default();
                    evs = vec![RunEvent::Refused { tool, input, reason: stream::cut(reason.trim(), 300) }];
                }
            }
            (Some("result"), _) => {
                // `stream::parse_line` gave one Refused per denial, in their order: drop those already shown.
                let ids: Vec<String> = v.get("permission_denials").and_then(Value::as_array).into_iter().flatten()
                    .map(|d| text_of(d, "tool_use_id")).collect();
                let mut i = 0;
                evs.retain(|e| match e {
                    RunEvent::Refused { .. } => {
                        let id = ids.get(i).cloned().unwrap_or_default();
                        i += 1;
                        id.is_empty() || !self.refused.contains(&id)
                    }
                    _ => true,
                });
            }
            _ => {}
        }
        evs
    }
}

/// An init line's MCP servers besides Gizai's own: their states, and a note for each that didn't connect.
fn mcp_states(line: &str) -> Vec<RunEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return vec![] };
    if v.get("type").and_then(Value::as_str) != Some("system") || v.get("subtype").and_then(Value::as_str) != Some("init") {
        return vec![];
    }
    let states: Vec<(String, String)> = crate::mcp_run::init_states(&v).unwrap_or_default().into_iter().filter(|(n, _)| n != "gizai").collect();
    if states.is_empty() {
        return vec![];
    }
    let mut out: Vec<RunEvent> = states.iter().filter_map(|(n, s)| crate::mcp_run::not_connected(n, s)).map(|text| RunEvent::Note { text }).collect();
    out.push(RunEvent::McpServers { servers: states.into_iter().map(|(name, status)| stream::McpState { name, status }).collect() });
    out
}

fn text_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn result(is_error: bool, subtype: &str, text: String, input_tokens: i64, output_tokens: i64) -> RunEvent {
    RunEvent::Result { is_error, subtype: subtype.into(), text, cost_usd: None, input_tokens, output_tokens, num_turns: 1 }
}

/// `codex exec --json`: thread.started, item.started/completed (agent_message, command_execution, file_change,
/// mcp_tool_call, web_search, reasoning, …), turn.completed with the tokens, turn.failed and error.
#[derive(Default)]
pub struct Codex {
    last_message: String,
    error: Option<String>,
    /// Tool items already shown (started), so each counts once.
    shown: HashSet<String>,
    done: bool,
}

impl Codex {
    fn tool(item: &Value) -> Option<(String, String)> {
        match item.get("type").and_then(Value::as_str)? {
            "command_execution" => Some(("Shell".into(), cut(&text_of(item, "command"), 120))),
            "file_change" => {
                let paths: Vec<String> = item.get("changes").and_then(Value::as_array).map(|c| c.iter().map(|x| text_of(x, "path")).collect()).unwrap_or_default();
                Some(("Edit".into(), cut(&paths.join(", "), 120)))
            }
            "mcp_tool_call" => Some((format!("{}.{}", text_of(item, "server"), text_of(item, "tool")), summarize(item.get("arguments").unwrap_or(&Value::Null)))),
            "web_search" => Some(("WebSearch".into(), cut(&text_of(item, "query"), 120))),
            _ => None,
        }
    }

    pub fn line(&mut self, line: &str) -> Vec<RunEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![RunEvent::Other { raw_type: "invalid".into() }] };
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string();
        let item = v.get("item").cloned().unwrap_or(Value::Null);
        let id = text_of(&item, "id");
        match ty.as_str() {
            "thread.started" => vec![RunEvent::Init { session_id: text_of(&v, "thread_id"), model: String::new() }],
            "item.started" => match Self::tool(&item) {
                Some((name, summary)) if self.shown.insert(id) => vec![RunEvent::ToolUse { name, summary }],
                _ => vec![RunEvent::Other { raw_type: ty }],
            },
            "item.completed" => {
                let kind = text_of(&item, "type");
                match kind.as_str() {
                    "agent_message" => {
                        self.last_message = text_of(&item, "text");
                        vec![RunEvent::Text { text: self.last_message.clone() }]
                    }
                    "error" => {
                        self.error = Some(text_of(&item, "message"));
                        vec![RunEvent::Other { raw_type: "error".into() }]
                    }
                    _ => {
                        let mut out = vec![];
                        if let Some((name, summary)) = Self::tool(&item) {
                            if self.shown.insert(id) {
                                out.push(RunEvent::ToolUse { name, summary });
                            }
                            if kind == "command_execution" || kind == "mcp_tool_call" {
                                let failed = text_of(&item, "status") == "failed" || item.get("exit_code").and_then(Value::as_i64).is_some_and(|c| c != 0);
                                let preview = match item.get("aggregated_output").and_then(Value::as_str) {
                                    Some(o) => o.to_string(),
                                    None => item.get("result").or_else(|| item.get("error")).map(|r| r.to_string()).unwrap_or_default(),
                                };
                                out.push(RunEvent::ToolResult { is_error: failed, preview: cut(&preview, 200) });
                            }
                        }
                        if out.is_empty() { vec![RunEvent::Other { raw_type: if kind.is_empty() { ty } else { kind } }] } else { out }
                    }
                }
            }
            "turn.completed" => {
                self.done = true;
                let u = v.get("usage").cloned().unwrap_or(Value::Null);
                let n = |k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
                vec![result(false, "success", self.last_message.clone(), n("input_tokens"), n("output_tokens"))]
            }
            "turn.failed" => {
                self.done = true;
                let msg = v.pointer("/error/message").and_then(Value::as_str).map(str::to_string)
                    .or_else(|| self.error.clone()).unwrap_or_else(|| "the turn failed".into());
                vec![result(true, "error", msg, 0, 0)]
            }
            "error" => {
                self.error = Some(text_of(&v, "message"));
                vec![RunEvent::Other { raw_type: "error".into() }]
            }
            _ => vec![RunEvent::Other { raw_type: ty }],
        }
    }

    /// Ended without a turn result: an error it reported is the result.
    pub fn finish(&mut self) -> Vec<RunEvent> {
        match (&self.error, self.done) {
            (Some(e), false) => { self.done = true; vec![result(true, "error", e.clone(), 0, 0)] }
            _ => vec![],
        }
    }
}

/// `gemini --output-format stream-json`: init, message (the answer in deltas), tool_use, tool_result, error and result.
#[derive(Default)]
pub struct Gemini {
    /// Answer text not yet shown.
    pending: String,
    /// The text after the last tool call: the final message.
    last_message: String,
    error: Option<String>,
    done: bool,
}

impl Gemini {
    fn flush(&mut self) -> Vec<RunEvent> {
        let t = std::mem::take(&mut self.pending);
        if t.trim().is_empty() { vec![] } else { vec![RunEvent::Text { text: t }] }
    }

    pub fn line(&mut self, line: &str) -> Vec<RunEvent> {
        let line = line.trim();
        if line.is_empty() {
            return vec![];
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![RunEvent::Other { raw_type: "invalid".into() }] };
        let ty = v.get("type").and_then(Value::as_str).unwrap_or("unknown").to_string();
        match ty.as_str() {
            "init" => vec![RunEvent::Init { session_id: text_of(&v, "session_id"), model: text_of(&v, "model") }],
            "message" if text_of(&v, "role") == "assistant" => {
                let t = text_of(&v, "content");
                self.pending.push_str(&t);
                self.last_message.push_str(&t);
                vec![]
            }
            "tool_use" => {
                let mut out = self.flush();
                self.last_message.clear();
                out.push(RunEvent::ToolUse { name: text_of(&v, "tool_name"), summary: summarize(v.get("parameters").unwrap_or(&Value::Null)) });
                out
            }
            "tool_result" => {
                let mut out = self.flush();
                let preview = v.get("output").and_then(Value::as_str).map(str::to_string)
                    .or_else(|| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
                out.push(RunEvent::ToolResult { is_error: text_of(&v, "status") != "success", preview: cut(&preview, 200) });
                out
            }
            "error" => {
                self.error = Some(text_of(&v, "message"));
                vec![RunEvent::Other { raw_type: "error".into() }]
            }
            "result" => {
                let mut out = self.flush();
                self.done = true;
                let ok = text_of(&v, "status") == "success";
                let s = v.get("stats").cloned().unwrap_or(Value::Null);
                let n = |a: &str, b: &str| s.get(a).or_else(|| s.get(b)).and_then(Value::as_i64).unwrap_or(0);
                let text = if ok {
                    self.last_message.trim().to_string()
                } else {
                    v.pointer("/error/message").and_then(Value::as_str).map(str::to_string)
                        .or_else(|| self.error.clone()).unwrap_or_else(|| "Gemini ended with an error".into())
                };
                out.push(result(!ok, if ok { "success" } else { "error" }, text, n("input_tokens", "input"), n("output_tokens", "output")));
                out
            }
            _ => vec![RunEvent::Other { raw_type: ty }],
        }
    }

    pub fn finish(&mut self) -> Vec<RunEvent> {
        let mut out = self.flush();
        if let (Some(e), false) = (&self.error, self.done) {
            self.done = true;
            out.push(result(true, "error", e.clone(), 0, 0));
        }
        out
    }
}

/// How much of a plain-text CLI's output its result keeps: the end, where the GIZAI_RESULT line is.
const TEXT_KEEP: usize = 32 * 1024;

/// Removes terminal colours and cursor moves, and keeps what a progress line (`\r`) ended with.
pub fn plain(line: &str) -> String {
    let line = line.rsplit('\r').find(|s| !s.is_empty()).unwrap_or("");
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() || n == '~' {
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out
}

/// Plain text: every line is text, and the end of the output is the result.
#[derive(Default)]
pub struct Text {
    all: String,
}

impl Text {
    pub fn line(&mut self, line: &str) -> Vec<RunEvent> {
        let t = plain(line);
        self.all.push_str(&t);
        self.all.push('\n');
        if self.all.len() > 2 * TEXT_KEEP {
            let mut from = self.all.len() - TEXT_KEEP;
            while !self.all.is_char_boundary(from) {
                from += 1;
            }
            self.all.drain(..from);
        }
        if t.trim().is_empty() { vec![] } else { vec![RunEvent::Text { text: t }] }
    }

    pub fn finish(&mut self) -> Vec<RunEvent> {
        let mut from = self.all.len().saturating_sub(TEXT_KEEP);
        while !self.all.is_char_boundary(from) {
            from += 1;
        }
        vec![result(false, "success", self.all[from..].trim().to_string(), 0, 0)]
    }
}
