import type { AgentServer, McpServerView, McpToolView, SecretLine } from "../types";

/** How each risk reads, and its badge. */
export const RISK_LABEL: Record<string, string> = { low: "Low risk", medium: "Medium risk", high: "High risk", unknown: "Not listed yet" };
export const RISK_BADGE: Record<string, string> = { low: "ok", medium: "warn", high: "fail", unknown: "outline" };

export const TRANSPORT_LABEL: Record<string, string> = { stdio: "Command", http: "Address (HTTP)", sse: "Address (SSE)" };

/** The highest risk among `tools` and one line about them, like "3 tools: 2 only read, 1 may delete or overwrite. High risk."
 * The same words as `summary` in src-tauri/src/mcp_servers.rs. */
export function toolsSummary(tools: McpToolView[]): { risk: string; text: string } {
  if (tools.length === 0) return { risk: "unknown", text: "Its tools aren't listed yet: List tools in Settings → MCP servers shows them." };
  const n = (r: string) => tools.filter((t) => t.risk === r).length;
  const [low, med, high] = [n("low"), n("medium"), n("high")];
  const risk = high > 0 ? "high" : med > 0 ? "medium" : "low";
  const parts = [low > 0 && `${low} only read`, med > 0 && `${med} change things`, high > 0 && `${high} may delete or overwrite`].filter(Boolean);
  return { risk, text: `${tools.length} tool${tools.length === 1 ? "" : "s"}: ${parts.join(", ")}. ${RISK_LABEL[risk]}.` };
}

/** The warning when an MCP server is on and the agent may run npm or npx (`npm_warning` in src-tauri/src/mcp_servers.rs). */
export function npmWarning(allowedTools: string[], anyOn: boolean): string | null {
  const risky = allowedTools.map((t) => t.replace(/ /g, "")).some((t) => t.startsWith("Bash(npm:") || t.startsWith("Bash(npx:") || t === "Bash(npm)" || t === "Bash(npx)");
  return anyOn && risky
    ? "This agent may run npm or npx, and an MCP server is on: a server's answer could try to make it run code. Take npm and npx out of its commands, or switch the server off."
    : null;
}

/** The same warning when web search, fetching pages or the browser is on (`npm_web_warning` in src-tauri/src/mcp_servers.rs). */
export function npmWebWarning(allowedTools: string[], webOn: boolean): string | null {
  return npmWarning(allowedTools, webOn)
    ? "This agent may run npm or npx, and web search, fetching pages or the browser is on: a web page could try to make it run code. Take npm and npx out of its commands, or switch those off."
    : null;
}

/** The switches with the server on or off; a server switched on for the first time has all its tools on. */
export function switchServer(list: AgentServer[], serverId: string, on: boolean): AgentServer[] {
  return list.some((s) => s.serverId === serverId)
    ? list.map((s) => (s.serverId === serverId ? { ...s, on } : s))
    : [...list, { serverId, on, toolsOff: [] }];
}

/** The switches with one tool of a server on or off. */
export function switchTool(list: AgentServer[], serverId: string, tool: string, on: boolean): AgentServer[] {
  const mine = list.find((s) => s.serverId === serverId) ?? { serverId, on: false, toolsOff: [] };
  const toolsOff = on ? mine.toolsOff.filter((t) => t !== tool) : [...mine.toolsOff.filter((t) => t !== tool), tool].sort();
  const next = { ...mine, toolsOff };
  return list.some((s) => s.serverId === serverId) ? list.map((s) => (s.serverId === serverId ? next : s)) : [...list, next];
}

/** The switches as saved: servers that are off and have no tools off say nothing, so they are left out. */
export function cleanSwitches(list: AgentServer[]): AgentServer[] {
  return list.filter((s) => s.on || s.toolsOff.length > 0).map((s) => ({ ...s, toolsOff: [...s.toolsOff].sort() }));
}

export function sameSwitches(a: AgentServer[], b: AgentServer[]): boolean {
  const key = (l: AgentServer[]) => JSON.stringify(cleanSwitches(l).sort((x, y) => x.serverId.localeCompare(y.serverId)));
  return key(a) === key(b);
}

/** Its state in the agent's last run, in a few words, and its badge. */
export function lastRunLabel(status: string): { text: string; badge: string } {
  if (status === "connected") return { text: "Connected in its last run", badge: "ok" };
  if (status === "needs-auth") return { text: "Needed sign-in in its last run", badge: "needs" };
  if (status === "pending") return { text: "Still connecting when its last run started", badge: "warn" };
  return { text: `Failed to connect in its last run${status && status !== "failed" ? ` (${status})` : ""}`, badge: "fail" };
}

/** Its sign-in state as a badge, for an address server that has one. */
export function signInLabel(signIn: string): { text: string; badge: string } | null {
  if (signIn === "signed_in") return { text: "Signed in", badge: "ok" };
  if (signIn === "needs_sign_in") return { text: "Needs sign-in", badge: "needs" };
  return null;
}

/** A server's command line or address, to show in one line (never a value of its lines). */
export function serverWhere(s: { transport: string; command: string; args: string[]; url: string }): string {
  return s.transport === "stdio" ? [s.command, ...s.args].join(" ") : s.url;
}

/** One argument per line; empty lines dropped. */
export function parseArgs(text: string): string[] {
  return text.split("\n").map((a) => a.trim()).filter(Boolean);
}

/** A secret line in the server form: its name, a new value typed now, and whether a value is saved already under that
 * name. `savedAs`: the name its saved value is kept under, once the line is renamed. */
export type LineDraft = { name: string; value: string; saved: boolean; savedAs?: string };

/** The lines of a saved server: names only, their values stay in the keychain. */
export function linesOf(names: string[], missing: string[] = []): LineDraft[] {
  return names.map((name) => ({ name, value: "", saved: !missing.includes(name) }));
}

/** The line under a new name. The keychain keeps a saved value under the line's name, and Save removes the old name's
 * value, so a renamed line has no saved value until it gets its old name back. */
export function renameLine(l: LineDraft, name: string): LineDraft {
  const savedAs = l.savedAs ?? (l.saved ? l.name.trim() : undefined);
  return savedAs === undefined ? { ...l, name } : { ...l, name, saved: name.trim() === savedAs, savedAs };
}

/** The lines to save: a value only when typed in now (null keeps the saved one). */
export function secretLines(lines: LineDraft[]): SecretLine[] {
  return lines.filter((l) => l.name.trim()).map((l) => ({ name: l.name.trim(), value: l.value ? l.value : null }));
}

/** What the agent form says next to a signed-in server's switch. */
export function actsAsYou(s: Pick<McpServerView, "signIn" | "name">): string | null {
  return s.signIn === "signed_in" ? `Signed in: its tools act as you in ${s.name}.` : null;
}

/** Names Gizai keeps for itself (`TAKEN` in crates/gizai-core/src/mcp_servers.rs). */
export const TAKEN_NAMES = ["gizai", "chrome-devtools"];

/** Why `name` can't be a new server's name (taken by Gizai, by a server in the list, or by another pick), in plain words;
 * null when it can. The same rules as `name_problem` in crates/gizai-core/src/mcp_servers.rs. */
export function nameProblem(name: string, taken: string[]): string | null {
  const n = name.trim();
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(n)) return `A name takes letters, digits, - and _ (at most 64)${n ? `, not "${n}"` : ""}`;
  if (n.includes("__") || n.endsWith("_")) {
    return `A name can't have two _ in a row or end with _, not "${n}": its tools are called mcp__<name>__<tool>, and Claude Code reads __ as where the name ends`;
  }
  if (TAKEN_NAMES.includes(n.toLowerCase())) return `${n} is Gizai's own: give the server another name`;
  if (taken.some((t) => t.toLowerCase() === n.toLowerCase())) return `There is already an MCP server called ${n}: give this one another name`;
  return null;
}
