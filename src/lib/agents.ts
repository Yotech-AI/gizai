import type { AgentFolder, AgentInput, AgentServer, Member, Wakeup } from "../types";

export function wakeupLabel(wakeup: Wakeup | string | null | undefined, minutes: number | null | undefined): string {
  if (wakeup === "on_assign") return "When assigned";
  if (wakeup === "heartbeat") return minutes === 60 ? "Every hour" : `Every ${minutes ?? "?"} min`;
  return "Manual";
}

/** "2.5" → 2_500_000; empty or not a number → null. */
export function dollarsToMicros(s: string): number | null {
  if (!s.trim()) return null;
  const n = Number(s.replace(",", "."));
  return Number.isFinite(n) && n >= 0 ? Math.round(n * 1_000_000) : null;
}

export function microsToDollars(m: number | null | undefined): string {
  return m == null ? "" : (m / 1_000_000).toFixed(2);
}

/** One allowed tool per line, e.g. Bash(npm test:*). */
export function parseTools(text: string): string[] {
  return text.split("\n").map((t) => t.trim()).filter(Boolean);
}

type RuleLike = { kind: string; matchLabelId?: string | null; matchStateId?: string | null; targetRole?: string | null };
type TeamLike = { labels: { id: string; name: string }[]; states: { id: string; name: string }[] };

export function ruleSentence(r: RuleLike, team: TeamLike): string {
  const to = `goes to a ${r.targetRole ?? "?"} agent`;
  if (r.kind === "label") return `A card labelled ${team.labels.find((l) => l.id === r.matchLabelId)?.name ?? "(deleted label)"} ${to}`;
  return `A card that enters ${team.states.find((s) => s.id === r.matchStateId)?.name ?? "(deleted column)"} ${to}`;
}

/** The roles the agent form offers (any other key is allowed via "Other"). */
export const ROLES = ["lead", "frontend", "backend", "design", "qa", "devops"];

const ROLE_LABELS: Record<string, string> = { lead: "Team Lead", qa: "QA", devops: "DevOps" };
export function roleLabel(role: string): string {
  return ROLE_LABELS[role] ?? (role ? role[0].toUpperCase() + role.slice(1) : role);
}

/** Gizai's default list, the one the run manager uses for an agent without its own (`DEFAULT_TOOLS` in
 * crates/gizai-core/src/seed.rs): commit their work, run the usual package managers and test runners, read files, the
 * read-only helpers agents use in pipes, and `sleep` to wait in the foreground between checks. The agent form starts a
 * new agent with it until its role's list (`roleTools`) has loaded. Edit per agent. */
export const DEFAULT_TOOLS = ["Bash(git status:*)", "Bash(git diff:*)", "Bash(git log:*)", "Bash(git add:*)", "Bash(git commit:*)", "Bash(git merge:*)", "Bash(npm:*)",
  "Bash(npx:*)", "Bash(composer:*)", "Bash(php:*)", "Bash(./vendor/bin/*)", "Bash(cargo:*)", "Bash(pytest:*)", "Bash(ls:*)", "Bash(cat:*)", "Bash(rg:*)",
  "Bash(head:*)", "Bash(tail:*)", "Bash(wc:*)", "Bash(sort:*)", "Bash(uniq:*)", "Bash(cut:*)", "Bash(diff:*)", "Bash(grep:*)", "Bash(jq:*)",
  "Bash(pwd:*)", "Bash(which:*)", "Bash(tree:*)", "Bash(sleep:*)"];

/** How the agent form opens for a new agent from elsewhere (the Chat page's Team Lead, an empty place on the org chart). */
export type AgentPreset = { name?: string; role?: string; chat?: boolean };

/** No wake-up or heartbeat: the columns an agent is on decide when it works (GA-49). The Team Lead's board check stays. */
export type AgentDraft = { name: string; role: string; model: string; instructions: string;
  permissionMode: string; tools: string; budget: string; chat: boolean; effort: string; maxRuns: string;
  /** The Team Lead's board check (with Chat on), every `boardMinutes` minutes. */
  boardCheck: boolean; boardMinutes: string;
  /** The id of the coding CLI it runs on. */
  cli: string;
  /** Folders besides its worktree, as typed (rows with an empty path are dropped on save). */
  folders: AgentFolder[];
  /** Its MCP servers switched on or off, with the tools switched off of each (Tools; saved apart from the rest). */
  mcp: AgentServer[] };

export function draftFrom(m?: Member | null, preset?: AgentPreset): AgentDraft {
  return {
    name: m?.name ?? preset?.name ?? "", role: m?.roleKey ?? preset?.role ?? "frontend", model: m?.model ?? "",
    instructions: m?.instructionsMd ?? "", permissionMode: m?.permissionMode ?? "acceptEdits",
    tools: (m ? m.allowedTools : DEFAULT_TOOLS).join("\n"), budget: microsToDollars(m?.budgetUsdMicros), chat: m ? !!m.chatEnabled : !!preset?.chat,
    effort: m?.effort ?? "", maxRuns: String(m?.maxRuns ?? 1), cli: m?.adapter || "claude_code",
    boardCheck: !!m?.boardCheckMinutes, boardMinutes: String(m?.boardCheckMinutes ?? 15),
    folders: (m?.folders ?? []).map((f) => ({ path: f.path, access: f.access })),
    mcp: (m?.tools?.mcp ?? []).map((s) => ({ serverId: s.serverId, on: s.on, toolsOff: [...s.toolsOff] })),
  };
}

/** The folders to save: trimmed, without empty rows. */
export function foldersFrom(list: AgentFolder[]): AgentFolder[] {
  return list.map((f) => ({ path: f.path.trim(), access: f.access })).filter((f) => f.path);
}

export function inputFrom(d: AgentDraft): AgentInput {
  return {
    name: d.name, roleKey: d.role, model: d.model.trim() || null, instructionsMd: d.instructions.trim() ? d.instructions : null,
    // No wake-up and no heartbeat: an old setting goes on save (it changed nothing since GA-49).
    permissionMode: d.permissionMode, allowedTools: parseTools(d.tools),
    budgetUsdMicros: dollarsToMicros(d.budget), adapter: d.cli || "claude_code", chatEnabled: d.chat, effort: d.effort || null,
    maxRuns: /^\d+$/.test(d.maxRuns.trim()) ? Number(d.maxRuns) : null,
    // The board check belongs to the agent with Chat on: off with Chat.
    boardCheckMinutes: d.chat && d.boardCheck ? Number(d.boardMinutes) || 0 : 0,
    folders: foldersFrom(d.folders),
  };
}
