// The coding CLIs agents run on (Settings → Coding CLIs): what each kind offers in the agent form, and the rows of the
// Settings list. The backend checks the same rules (gizai-core clis.rs).
import type { Cli, CliKind } from "../types";

export const CLAUDE_CODE = "claude_code";

export const KIND_LABEL: Record<CliKind, string> = { claude_code: "Claude Code", codex: "Codex", gemini: "Gemini", other: "Other" };
export const KINDS: CliKind[] = ["claude_code", "codex", "gemini", "other"];

/** What each permission mode lets an agent do, per kind of CLI; the first is the default. */
export const PERMISSIONS: Record<CliKind, Record<string, string>> = {
  claude_code: {
    acceptEdits: "Edits files freely; runs only the commands allowed below. Recommended.",
    dontAsk: "Never asks; anything not allowed below is refused.",
    auto: "Claude decides what is safe to run on its own.",
    plan: "Plans only; changes nothing.",
    manual: "Asks for every action; a background agent can't answer, so most actions are refused.",
    bypassPermissions: "Runs anything without asking. Only for a sandboxed machine.",
  },
  codex: {
    "workspace-write": "Edits files and runs commands in Codex's sandbox: it writes only the worktree and the repository's git folder, and may use the network. Recommended.",
    "read-only": "Reads and plans only; changes nothing.",
    "danger-full-access": "No sandbox: runs anything without asking. Only for a sandboxed machine.",
  },
  gemini: {
    auto_edit: "Edits files freely; runs only the commands allowed below. Recommended.",
    yolo: "Runs anything without asking. Only for a sandboxed machine.",
    plan: "Reads and plans only; changes nothing.",
    default: "Asks for every action; a background agent can't answer, so most actions are refused.",
  },
  other: {},
};

/** What each kind of CLI does with the agent's folders (agent form → Folders); the backend's `cli::task_exec`. */
export const FOLDERS_NOTE: Record<CliKind, string> = {
  claude_code: "Claude Code may use each folder with its file tools, and can't edit or write files in a read folder.",
  codex: "Codex reads every folder anyway; it may write a read and change folder in its workspace-write sandbox, which covers its commands too.",
  gemini: "Gemini can't keep a folder read only, so it gets only the read and change folders.",
  other: "This CLI can't limit folders: Gizai passes it none of them.",
};

/** The risky mode of each kind, shown as a warning. */
export const RISKY = new Set(["bypassPermissions", "danger-full-access", "yolo"]);

/** Effort levels per kind, lowest first; none for Gemini and Other. */
export const EFFORTS_BY_KIND: Record<CliKind, string[]> = {
  claude_code: ["low", "medium", "high", "xhigh", "max"],
  codex: ["minimal", "low", "medium", "high", "xhigh"],
  gemini: [],
  other: [],
};

export function kindOf(cliId: string | null | undefined, clis: Cli[] | null): CliKind {
  const id = cliId || CLAUDE_CODE;
  return (clis?.find((c) => c.id === id)?.kind as CliKind | undefined) ?? (id === CLAUDE_CODE ? "claude_code" : "other");
}

export function defaultMode(kind: CliKind): string {
  return Object.keys(PERMISSIONS[kind])[0] ?? "";
}

/** The permission mode to keep when an agent moves to a CLI of `kind`: its own when that kind has it, else the default. */
export function modeFor(kind: CliKind, mode: string): string {
  return mode in PERMISSIONS[kind] ? mode : defaultMode(kind);
}

/** Whether the allowed-commands list means anything for this kind (Gemini takes the Bash(...) commands as shell commands). */
export function usesAllowedTools(kind: CliKind): boolean {
  return kind === "claude_code" || kind === "gemini";
}

/** "CLAUDE_CONFIG_DIR=~/.claude-2" lines → the list the backend takes. */
export function parseEnv(text: string): string[] {
  return text.split("\n").map((l) => l.trim()).filter(Boolean);
}

/** One line under a CLI in the list: what it runs. */
export function cliSummary(c: Cli & { path?: string | null }): string {
  const prog = c.path || c.command || "found when needed";
  const env = c.env.length ? `${c.env.join(" ")} ` : "";
  const args = c.kind === "other" && c.args ? ` ${c.args}` : "";
  return `${env}${prog}${args}`;
}

/** The name of the CLI an agent runs on; "Coding CLI" while the list loads. */
export function cliName(adapter: string | null | undefined, clis: Cli[] | null | undefined): string {
  const id = adapter || CLAUDE_CODE;
  return clis?.find((c) => c.id === id)?.name ?? (id === CLAUDE_CODE ? "Claude Code" : clis ? "Unknown CLI" : "Coding CLI");
}
