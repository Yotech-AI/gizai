//! The `claude` command line for one headless agent run or chat turn.
use std::path::PathBuf;

/// Claude Code's own switch for its memory (its auto memory, `projects/<project folder>/memory/` in the account's folder):
/// set to 1, Claude Code neither reads nor writes that folder, whatever its settings say (read in Claude Code 2.1.273 to
/// 2.1.289).
pub const AUTO_MEMORY_OFF: &str = "CLAUDE_CODE_DISABLE_AUTO_MEMORY";

#[derive(Debug, Clone, Default)]
pub struct ClaudeArgs {
    pub bin: PathBuf,
    pub prompt: String,
    pub session_id: String,
    /// One of acceptEdits, auto, bypassPermissions, manual, dontAsk, plan (Claude Code 2.1).
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub append_system_prompt: Option<String>,
    pub model: Option<String>,
    /// Claude Code stops the run once it has spent this much (print mode only).
    pub max_budget_usd: Option<f64>,
    /// Continue `session_id` (`--resume`) instead of starting it (`--session-id`).
    pub resume: bool,
    /// An MCP config file for this run (the only MCP servers it gets: `--strict-mcp-config` is always on).
    pub mcp_config: Option<PathBuf>,
    /// Stream text as it is written (`--include-partial-messages`), for chat.
    pub partial_messages: bool,
    /// `--restricted`: ignore user, project and local settings files (so no plugins or hooks), no command tools.
    pub restricted: bool,
    /// The built-in tools available at all (`--tools`); None = Claude Code's default set.
    pub tools: Option<Vec<String>>,
    /// Refuse anything that would ask for permission (`--permission-prompts none`).
    pub permission_prompts_none: bool,
    /// More directories the file tools may read (`--add-dir`).
    pub add_dirs: Vec<String>,
    /// Tools or rules to refuse (`--disallowedTools`), like `Edit(//home/me/shared/**)`.
    pub disallowed_tools: Vec<String>,
    /// Don't save the session (`--no-session-persistence`); such a session can't be resumed.
    pub no_session_persistence: bool,
    /// Run no hooks at all (`--settings {"disableAllHooks":true}`): not the user's, not a plugin's, not a repo's.
    pub disable_hooks: bool,
    /// No skills or slash commands (`--disable-slash-commands`), so plugin skills can't steer a headless run. Only a task
    /// run of an agent with Slash commands and skills on goes without it (`TaskRun::slash_commands`); chat never does.
    pub disable_skills: bool,
    /// Claude Code's own memory off (`CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`): an agent keeps its notes in Gizai's memory
    /// only, not in a second one that only this account sees (GA-85).
    pub disable_auto_memory: bool,
    /// How hard the model thinks (`--effort`: low, medium, high, xhigh, max); None = Claude Code's default.
    pub effort: Option<String>,
    /// Set on top of Gizai's environment: CLAUDE_CONFIG_DIR for a second Claude Code account.
    pub env: Vec<(String, String)>,
}

impl ClaudeArgs {
    /// The program to start: the arguments below, the environment, and the prompt on stdin. The memory switch comes after
    /// the account's own lines, so none of them turns Claude Code's memory back on.
    pub fn exec(&self) -> crate::process::Exec {
        let mut env = self.env.clone();
        if self.disable_auto_memory {
            env.push((AUTO_MEMORY_OFF.into(), "1".into()));
        }
        crate::process::Exec { bin: self.bin.clone(), args: self.argv(), env, stdin: self.prompt.clone() }
    }

    /// Arguments after the binary. The prompt is not among them: it goes in on stdin (`claude -p` reads it
    /// there), because Linux caps one argument at 128 KiB and a task with a long description would not start.
    /// Task runs load only user settings (no project hooks or MCP servers from the repo the agent works in);
    /// restricted runs (chat) load no settings files at all.
    pub fn argv(&self) -> Vec<String> {
        let mut a: Vec<String> = ["-p", "--output-format", "stream-json", "--verbose"].iter().map(|s| s.to_string()).collect();
        if self.partial_messages {
            a.push("--include-partial-messages".into());
        }
        a.extend([if self.resume { "--resume" } else { "--session-id" }.into(), self.session_id.clone()]);
        a.extend(["--permission-mode".into(), self.permission_mode.clone()]);
        if self.restricted {
            a.push("--restricted".into());
        } else {
            a.extend(["--setting-sources".into(), "user".into()]);
        }
        a.push("--strict-mcp-config".into());
        if let Some(p) = &self.mcp_config {
            a.extend(["--mcp-config".into(), p.display().to_string()]);
        }
        if let Some(t) = &self.tools {
            a.extend(["--tools".into(), t.join(",")]);
        }
        if self.permission_prompts_none {
            a.extend(["--permission-prompts".into(), "none".into()]);
        }
        if self.no_session_persistence {
            a.push("--no-session-persistence".into());
        }
        if self.disable_hooks {
            a.extend(["--settings".into(), r#"{"disableAllHooks":true}"#.into()]);
        }
        if self.disable_skills {
            a.push("--disable-slash-commands".into());
        }
        if let Some(m) = &self.model {
            a.extend(["--model".into(), m.clone()]);
        }
        if let Some(e) = &self.effort {
            a.extend(["--effort".into(), e.clone()]);
        }
        if let Some(b) = self.max_budget_usd {
            a.extend(["--max-budget-usd".into(), format!("{b:.2}")]);
        }
        if let Some(s) = &self.append_system_prompt {
            a.extend(["--append-system-prompt".into(), s.clone()]);
        }
        // Variadic options last, each followed only by its own values.
        if !self.add_dirs.is_empty() {
            a.push("--add-dir".into());
            a.extend(self.add_dirs.iter().cloned());
        }
        if !self.allowed_tools.is_empty() {
            a.push("--allowedTools".into());
            a.extend(self.allowed_tools.iter().cloned());
        }
        if !self.disallowed_tools.is_empty() {
            a.push("--disallowedTools".into());
            a.extend(self.disallowed_tools.iter().cloned());
        }
        a
    }
}
