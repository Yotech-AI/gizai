import type { AgentServer, CliKind, CliTools } from "../types";
import { switchServer } from "./mcp";

/** The built-in browser's id (`BROWSER` in crates/gizai-core/src/mcp_servers.rs): an agent's switch for it is an MCP switch. */
export const BROWSER = "chrome-devtools";

export const NO_CLI_TOOLS: CliTools = { webSearch: false, webFetch: false, fetchDomains: [], insecureCerts: false, builtin: [], slashCommands: false };

export const cliToolsFrom = (t?: CliTools | null): CliTools => ({
  webSearch: !!t?.webSearch, webFetch: !!t?.webFetch, fetchDomains: [...(t?.fetchDomains ?? [])], insecureCerts: !!t?.insecureCerts, builtin: [...(t?.builtin ?? [])],
  slashCommands: !!t?.slashCommands,
});

/** Domains as typed (one per line, or separated by commas or spaces), each once. */
export function parseDomains(text: string): string[] {
  const out: string[] = [];
  for (const d of text.split(/[\s,]+/).map((x) => x.trim()).filter(Boolean)) if (!out.includes(d)) out.push(d);
  return out;
}

/** What a CLI of `kind` keeps of the switches when an agent moves to it (and what is saved for it). */
export function fitCliTools(t: CliTools, kind: CliKind): CliTools {
  switch (kind) {
    case "claude_code": return t;
    case "codex": return { ...t, webFetch: false, fetchDomains: [], builtin: [], slashCommands: false };
    case "gemini": return { ...t, webSearch: false, fetchDomains: [], builtin: [], slashCommands: false };
    default: return { ...NO_CLI_TOOLS };
  }
}

export function sameCliTools(a: CliTools, b: CliTools): boolean {
  const norm = (t: CliTools) => JSON.stringify({ ...t, fetchDomains: t.webFetch ? t.fetchDomains : [], builtin: [...t.builtin].sort() });
  return norm(a) === norm(b);
}

/** Whether the browser is on in the agent's MCP switches. */
export const browserOn = (mcp: AgentServer[]) => mcp.some((s) => s.serverId === BROWSER && s.on);

export const switchBrowser = (mcp: AgentServer[], on: boolean) => switchServer(mcp, BROWSER, on);

/** A built-in tool switched on or off. */
export function switchBuiltin(t: CliTools, id: string, on: boolean): CliTools {
  const rest = t.builtin.filter((b) => b !== id);
  return { ...t, builtin: on ? [...rest, id].sort() : rest };
}

/** The group headings of the Built-in tools list, in order. */
export const GROUPS: [string, string][] = [
  ["files", "Files"], ["commands", "Commands"], ["agents", "Agents"], ["planning", "Planning"], ["other", "Other"],
];

/** How a tool a run gets reads, for the ones without a switch. */
export const HOW_LABEL: Record<string, string> = { always: "Always on", elsewhere: "Set elsewhere", off: "Off" };
