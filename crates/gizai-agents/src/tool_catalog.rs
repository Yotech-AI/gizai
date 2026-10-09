//! The coding CLIs' own tools (agent form → Tools): Gizai's catalog of what each CLI offers, in plain words with a risk and
//! how a run gets it, merged with what the CLI itself reports (Claude Code's init line, in a run or when asked). Gizai
//! builds no tools: every tool here is the CLI's own. A tool the CLI reports that the catalog doesn't know shows under
//! "Other tools the CLI reports", risk unknown, off.
//!
//! Checked against the installed CLIs' own files, without starting them: Claude Code 2.1.289 (the tool names in its
//! binary), Codex 0.154 (its config keys: `web_search = "live" | "cached" | "disabled"`) and Gemini CLI 0.62 (its bundled
//! tool reference and policies: `google_web_search` is allowed by its read-only policy in every mode).
use std::ffi::OsStr;

use serde::Serialize;

use crate::cli::Kind;

/// How a run gets a tool.
pub mod how {
    /// The Web switches (search, fetch): on, the run gets it.
    pub const WEB: &str = "web";
    /// A switch of its own: on, it is allowed in the agent's task runs.
    pub const SWITCH: &str = "switch";
    /// Always there: the CLI uses it without asking.
    pub const ALWAYS: &str = "always";
    /// Set elsewhere in the agent form (the permission mode, Allowed commands); the note says where.
    pub const ELSEWHERE: &str = "elsewhere";
    /// Not for Gizai's agents, or no per-run switch for it in this CLI; the note says why.
    pub const OFF: &str = "off";
}

/// One tool in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTool {
    /// Its name in the CLI, like WebFetch or web_search.
    pub id: String,
    pub label: String,
    /// web | browser | files | commands | agents | planning | other
    pub group: String,
    /// One line: what it allows.
    pub description: String,
    /// low | medium | high | unknown
    pub risk: String,
    /// `how::…`
    pub how: String,
    /// For always, elsewhere and off: one line why or where.
    pub note: String,
    /// The CLI named it in its list (last run, or when asked).
    pub reported: bool,
}

type Row = (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str, &'static str);

const BY_MODE: &str = "On through the permission mode (Permissions → Permission mode).";
const BY_COMMANDS: &str = "Only the commands under Permissions → Allowed commands.";
const NO_ASK: &str = "Claude Code uses it without asking.";
const HEADLESS: &str = "For interactive sessions: a headless run can't use it.";

/// Claude Code 2.1.289: (id, label, group, description, risk, how, note).
const CLAUDE: [Row; 33] = [
    ("WebSearch", "Web search", "web", "Searches the web. Results are untrusted text, and the search terms leave your computer.", "medium", how::WEB, ""),
    ("WebFetch", "Fetch web pages", "web", "Reads web pages, any or only the domains you list. Pages are untrusted, and an address can carry data out.", "high", how::WEB, ""),
    ("Read", "Read files", "files", "Reads files in its worktree and its folders.", "low", how::ALWAYS, NO_ASK),
    ("Glob", "Find files", "files", "Finds files by name pattern.", "low", how::ALWAYS, NO_ASK),
    ("Grep", "Search in files", "files", "Searches file contents.", "low", how::ALWAYS, NO_ASK),
    ("LSP", "Code intelligence", "files", "Asks a language server for definitions, references and errors.", "low", how::ALWAYS, NO_ASK),
    ("Edit", "Edit files", "files", "Changes files in its worktree and read-and-change folders.", "medium", how::ELSEWHERE, BY_MODE),
    ("Write", "Write files", "files", "Creates or overwrites files in its worktree and read-and-change folders.", "medium", how::ELSEWHERE, BY_MODE),
    ("NotebookEdit", "Edit notebooks", "files", "Changes Jupyter notebook cells.", "medium", how::ELSEWHERE, BY_MODE),
    ("Bash", "Run commands", "commands", "Runs shell commands.", "high", how::ELSEWHERE, BY_COMMANDS),
    ("BashOutput", "Read a background command", "commands", "Reads the output of a command it started in the background.", "low", how::ALWAYS, NO_ASK),
    ("KillShell", "Stop a background command", "commands", "Stops a command it started in the background.", "low", how::ALWAYS, NO_ASK),
    ("TaskOutput", "Read a background task", "commands", "Reads the output of a background command or task it started.", "low", how::ALWAYS, NO_ASK),
    ("TaskStop", "Stop a background task", "commands", "Stops a background command or task it started.", "low", how::ALWAYS, NO_ASK),
    ("Monitor", "Watch a background command", "commands", "Waits for new output from a command it started in the background.", "low", how::ALWAYS, NO_ASK),
    ("Task", "Subagents", "agents", "Hands part of the work to a subagent with the same permissions. Each subagent costs more.", "medium", how::ALWAYS, NO_ASK),
    ("Agent", "Subagents", "agents", "Hands part of the work to a subagent with the same permissions (the newer name of Task).", "medium", how::ALWAYS, NO_ASK),
    ("SendMessage", "Message other agents", "agents", "Sends a message to an agent of an interactive agent team.", "medium", how::OFF, HEADLESS),
    ("TodoWrite", "To-do list", "planning", "Keeps its own list of steps for the run.", "low", how::ALWAYS, NO_ASK),
    ("TaskCreate", "Task list: add", "planning", "Adds a step to its own task list.", "low", how::ALWAYS, NO_ASK),
    ("TaskGet", "Task list: read", "planning", "Reads a step of its own task list.", "low", how::ALWAYS, NO_ASK),
    ("TaskList", "Task list: list", "planning", "Lists its own task list.", "low", how::ALWAYS, NO_ASK),
    ("TaskUpdate", "Task list: update", "planning", "Updates a step of its own task list.", "low", how::ALWAYS, NO_ASK),
    ("EnterPlanMode", "Enter plan mode", "planning", "Switches itself to read-only planning.", "low", how::ALWAYS, NO_ASK),
    ("ExitPlanMode", "Leave plan mode", "planning", "Ends planning to start changing things.", "low", how::ELSEWHERE, "Only in the plan permission mode, where a headless run can't be approved."),
    ("AskUserQuestion", "Ask you a question", "planning", "Asks you to pick an answer.", "low", how::OFF, "A headless run can't ask: the agent asks in its result line (needs_decision) instead."),
    ("Skill", "Skills", "other", "Loads a skill from a plugin or your settings.", "medium", how::OFF, "Gizai keeps skills and slash commands off in agent runs (--disable-slash-commands)."),
    ("SlashCommand", "Slash commands", "other", "Runs one of your slash commands.", "medium", how::OFF, "Gizai keeps skills and slash commands off in agent runs (--disable-slash-commands)."),
    ("ToolSearch", "Find more tools", "other", "Loads the full description of a tool it has but hasn't loaded yet.", "low", how::ALWAYS, NO_ASK),
    ("ListMcpResourcesTool", "MCP resources: list", "other", "Lists what its MCP servers offer to read.", "low", how::ALWAYS, NO_ASK),
    ("ReadMcpResourceTool", "MCP resources: read", "other", "Reads something an MCP server offers. Only its servers switched on.", "low", how::ALWAYS, NO_ASK),
    ("CronCreate", "Scheduled prompts", "other", "Schedules a prompt for later in the same session.", "medium", how::OFF, HEADLESS),
    ("EnterWorktree", "Worktrees", "other", "Moves the session into a git worktree of its own.", "medium", how::OFF, "Gizai gives each card its own worktree already."),
];

const CODEX_NOTE: &str = "From Gizai's catalog: Codex has no command that lists its tools without a model call.";

/// Codex 0.154 (its config keys and stream items): (id, label, group, description, risk, how, note).
const CODEX: [Row; 7] = [
    ("web_search", "Web search", "web", "Searches the web (Codex's live search). Results are untrusted text, and the search terms leave your computer.", "medium", how::WEB, ""),
    ("shell", "Run commands", "commands", "Runs shell commands inside Codex's sandbox.", "high", how::ELSEWHERE, "The sandbox under Permissions → Permission mode decides what they may change."),
    ("apply_patch", "Edit files", "files", "Changes files in its worktree.", "medium", how::ELSEWHERE, "The sandbox under Permissions → Permission mode."),
    ("update_plan", "Plan", "planning", "Keeps its own list of steps for the run.", "low", how::ALWAYS, "Codex uses it without asking."),
    ("view_image", "Look at images", "files", "Looks at an image file in its worktree.", "low", how::ALWAYS, "Codex uses it without asking."),
    ("web_fetch", "Fetch web pages", "web", "Reads web pages.", "high", how::OFF, "Codex has no tool of its own that fetches a page: only web search, and commands its sandbox lets out."),
    ("request_user_input", "Ask you a question", "planning", "Asks you for input.", "low", how::OFF, "A headless run can't ask: the agent asks in its result line (needs_decision) instead."),
];

const GEMINI_ALWAYS: &str = "Gemini uses it without asking.";

/// Gemini CLI 0.62 (its bundled tool reference): (id, label, group, description, risk, how, note).
const GEMINI: [Row; 16] = [
    ("google_web_search", "Web search", "web", "Searches Google. Results are untrusted text, and the search terms leave your computer.", "medium", how::ALWAYS,
     "On in every Gemini run: Gemini's own read-only policy allows it, and Gizai has no checked per-run switch to turn it off."),
    ("web_fetch", "Fetch web pages", "web", "Reads web pages (not private addresses). Pages are untrusted, and an address can carry data out.", "high", how::WEB, ""),
    ("read_file", "Read files", "files", "Reads a file.", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("read_many_files", "Read many files", "files", "Reads several files at once.", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("list_directory", "List folders", "files", "Lists a folder.", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("glob", "Find files", "files", "Finds files by name pattern.", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("grep_search", "Search in files", "files", "Searches file contents (once called search_file_content).", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("replace", "Edit files", "files", "Changes text in a file.", "medium", how::ELSEWHERE, "On through the approval mode (Permissions → Permission mode)."),
    ("write_file", "Write files", "files", "Creates or overwrites a file.", "medium", how::ELSEWHERE, "On through the approval mode (Permissions → Permission mode)."),
    ("run_shell_command", "Run commands", "commands", "Runs shell commands.", "high", how::ELSEWHERE, "Only the Bash(…) lines under Permissions → Allowed commands."),
    ("write_todos", "To-do list", "planning", "Keeps its own list of steps for the run.", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("ask_user", "Ask you a question", "planning", "Asks you for input.", "low", how::OFF, "Gemini refuses it in a headless run: the agent asks in its result line instead."),
    ("save_memory", "Memory", "other", "Saves a fact in Gemini's memory file under ~/.gemini.", "medium", how::OFF, "Gizai never lets an agent write in ~/.gemini."),
    ("activate_skill", "Skills", "other", "Loads a skill from .gemini/skills.", "medium", how::OFF, "Not switched on by Gizai."),
    ("list_mcp_resources", "MCP resources: list", "other", "Lists what its MCP servers offer to read.", "low", how::ALWAYS, GEMINI_ALWAYS),
    ("read_mcp_resource", "MCP resources: read", "other", "Reads something an MCP server offers.", "low", how::ALWAYS, GEMINI_ALWAYS),
];

const GEMINI_NOTE: &str = "From Gizai's catalog: Gemini has no command that lists its tools without a model call.";

fn rows(kind: Kind) -> &'static [Row] {
    match kind {
        Kind::ClaudeCode => &CLAUDE,
        Kind::Codex => &CODEX,
        Kind::Gemini => &GEMINI,
        Kind::Other => &[],
    }
}

/// Where the list comes from when the CLI can't be asked: shown above it.
pub fn source_note(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::ClaudeCode => None,
        Kind::Codex => Some(CODEX_NOTE),
        Kind::Gemini => Some(GEMINI_NOTE),
        Kind::Other => Some("This CLI runs with its own settings: Gizai has no catalog of its tools and passes it none."),
    }
}

fn tool(r: &Row, reported: bool) -> CatalogTool {
    CatalogTool { id: r.0.into(), label: r.1.into(), group: r.2.into(), description: r.3.into(), risk: r.4.into(), how: r.5.into(),
                  note: r.6.into(), reported }
}

/// The catalog of `kind`, without what the CLI reported.
pub fn catalog(kind: Kind) -> Vec<CatalogTool> {
    rows(kind).iter().map(|r| tool(r, false)).collect()
}

/// A catalog entry by its id, if the catalog knows it.
pub fn find(kind: Kind, id: &str) -> Option<CatalogTool> {
    rows(kind).iter().find(|r| r.0 == id).map(|r| tool(r, false))
}

/// The catalog merged with what the CLI reported (`reported`: its built-in tools, MCP tools left out; None = never asked).
/// Never only one of them: the catalog's tools come first, marked when reported; then the reported tools the catalog
/// doesn't know, under "other", risk unknown, with a switch (off until you switch it on).
pub fn merged(kind: Kind, reported: Option<&[String]>) -> Vec<CatalogTool> {
    let said = |id: &str| reported.is_some_and(|r| r.iter().any(|t| t == id));
    let mut out: Vec<CatalogTool> = rows(kind).iter().map(|r| tool(r, said(r.0))).collect();
    if kind == Kind::ClaudeCode {
        for t in reported.unwrap_or_default().iter().filter(|t| valid_name(t)) {
            if out.iter().any(|o| &o.id == t) {
                continue;
            }
            out.push(CatalogTool {
                id: t.clone(), label: t.clone(), group: "other".into(), risk: "unknown".into(), how: how::SWITCH.into(), reported: true,
                description: "Claude Code reports it, and Gizai's catalog doesn't know it yet: check what it does before you switch it on.".into(),
                note: String::new(),
            });
        }
    }
    out
}

/// Whether a run of `kind` can be given `id` through its own switch (an unknown reported tool, or a catalog switch).
pub fn switchable(kind: Kind, id: &str) -> bool {
    match find(kind, id) {
        Some(t) => t.how == how::SWITCH,
        None => kind == Kind::ClaudeCode && crate::tool_catalog::valid_name(id),
    }
}

/// A built-in tool name: letters, digits, `_` and `-` (MCP tools are switched per server).
pub fn valid_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && !n.starts_with("mcp__") && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Whether an entry of an agent's allowed commands (Claude Code style: `Bash(git status:*)`, `WebFetch(domain:docs.rs)`)
/// names a tool only the agent form's Tools switches give: web search, fetching pages, a built-in tool with a switch of its
/// own (one the catalog doesn't know) or one Gizai keeps off. A run leaves such an entry out, so with its switch off the
/// tool is absent whatever the list says. MCP tools stay: a server that's off isn't in the run's config, and a tool
/// switched off is refused.
pub fn only_by_switch(entry: &str) -> bool {
    let name = entry.split('(').next().unwrap_or_default().trim();
    match find(Kind::ClaudeCode, name) {
        Some(t) => t.how != how::ALWAYS && t.how != how::ELSEWHERE,
        None => valid_name(name),
    }
}

/// Whether every run on a CLI of `kind` may reach the web whatever the agent's switches (Gemini's web search, which its own
/// policy allows): its prompt says web content is data in every run.
pub fn web_in_every_run(kind: Kind) -> bool {
    rows(kind).iter().any(|r| r.2 == "web" && r.5 == how::ALWAYS)
}

/// What each CLI can be given of the Web switches: (search, fetch, fetch with a domain list), each None when it can, else why.
pub fn web_support(kind: Kind) -> (Option<&'static str>, Option<&'static str>, Option<&'static str>) {
    match kind {
        Kind::ClaudeCode => (None, None, None),
        Kind::Codex => (None, Some("Codex has no tool that fetches a page: only web search."), Some("Codex has no tool that fetches a page.")),
        Kind::Gemini => (Some("Gemini's own read-only policy allows its web search in every run, and Gizai has no checked per-run switch to turn it off."),
                         None, Some("Gemini can't limit fetching to some domains: it gets web_fetch for any page.")),
        Kind::Other => (Some(OTHER), Some(OTHER), Some(OTHER)),
    }
}

const OTHER: &str = "This CLI runs with its own settings: Gizai passes it no tools.";

/// Claude Code's rules for the Web switches: `WebSearch`, and `WebFetch` or one `WebFetch(domain:…)` per domain. For
/// `--allowedTools` (task runs and chat).
pub fn claude_web_rules(search: bool, fetch: bool, domains: &[String]) -> Vec<String> {
    let mut out = vec![];
    if search {
        out.push("WebSearch".to_string());
    }
    if fetch {
        if domains.is_empty() {
            out.push("WebFetch".to_string());
        } else {
            out.extend(domains.iter().map(|d| format!("WebFetch(domain:{d})")));
        }
    }
    out
}

/// Claude Code's tool names for the Web switches, for chat's `--tools` (which takes names, not rules).
pub fn claude_web_names(search: bool, fetch: bool) -> Vec<String> {
    [(search, "WebSearch"), (fetch, "WebFetch")].into_iter().filter(|(on, _)| *on).map(|(_, n)| n.to_string()).collect()
}

// ---- what Claude Code reports ----

/// The built-in tools in a Claude Code init line (`system`/`init`, `tools`), MCP servers' tools left out.
pub fn init_tools(v: &serde_json::Value) -> Option<Vec<String>> {
    if v.get("type")?.as_str()? != "system" || v.get("subtype")?.as_str()? != "init" {
        return None;
    }
    let list = v.get("tools")?.as_array()?;
    Some(list.iter().filter_map(|t| t.as_str()).filter(|t| !t.starts_with("mcp__")).map(str::to_string).collect())
}

/// The tools of the first init line in a run's or chat turn's log (Gizai's notes and other lines before it are skipped).
/// Reads only up to that line.
pub fn log_tools(path: &std::path::Path) -> Option<Vec<String>> {
    use std::io::BufRead;
    let f = std::fs::File::open(path).ok()?;
    for line in std::io::BufReader::new(f).lines().map_while(Result::ok).take(200) {
        if !line.contains("\"init\"") {
            continue;
        }
        if let Some(tools) = serde_json::from_str::<serde_json::Value>(line.trim()).ok().and_then(|v| init_tools(&v)) {
            return Some(tools);
        }
    }
    None
}

/// Asks the installed Claude Code for its tools without a login: starts `claude` with a scratch `CLAUDE_CONFIG_DIR` and
/// HOME in `scratch` and only PATH from Gizai's environment (no API key or token can reach it, so nothing can be spent, and
/// nothing is written in ~/.claude), reads its init line (printed before its "Not logged in" error), and ends its process
/// group. Takes a second or two. `bin`: the Claude Code program; `path`: the PATH it gets.
pub async fn ask_claude(bin: &std::path::Path, scratch: &std::path::Path, path: &OsStr) -> Result<Vec<String>, String> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (home, config, tmp) = (scratch.join("home"), scratch.join("config"), scratch.join("tmp"));
    for d in [&home, &config, &tmp] {
        std::fs::create_dir_all(d).map_err(|e| format!("couldn't make a scratch folder {}: {e}", d.display()))?;
    }
    let mut child = tokio::process::Command::new(bin)
        .env_clear()
        .env("PATH", path).env("HOME", &home).env("CLAUDE_CONFIG_DIR", &config).env("TMPDIR", &tmp).env("LANG", "C.UTF-8")
        .args(["-p", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--setting-sources", "user",
               "--settings", r#"{"disableAllHooks":true}"#, "--disable-slash-commands", "--strict-mcp-config",
               "--permission-mode", "acceptEdits", "--max-budget-usd", "0.01"])
        .current_dir(&home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", bin.display()))?;
    let group = child.id().unwrap_or(0);
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let _ = stdin.write_all(b"Reply with OK.").await;
    let _ = stdin.shutdown().await;
    drop(stdin);
    let read = async {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(tools) = serde_json::from_str::<serde_json::Value>(line.trim()).ok().and_then(|v| init_tools(&v)) {
                return Some(tools);
            }
        }
        None
    };
    let found = tokio::time::timeout(std::time::Duration::from_secs(30), read).await.ok().flatten();
    // Its own process group only: SIGTERM, then SIGKILL if it lingers.
    if group > 1 {
        // SAFETY: a negative pid addresses the process group this function created for it.
        unsafe { libc::kill(-(group as i32), libc::SIGTERM); }
    }
    if tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await.is_err() {
        if group > 1 {
            // SAFETY: as above.
            unsafe { libc::kill(-(group as i32), libc::SIGKILL); }
        }
        let _ = child.wait().await;
    }
    match found {
        Some(t) if !t.is_empty() => Ok(t),
        _ => Err("Claude Code gave no list of its tools (is it installed and up to date?)".into()),
    }
}
