// What the Chat page shows for tool calls and how it groups messages. Pure, so it is tested.
import { href, type Page } from "../router";
import type { ChatMessage } from "../types";

const PREFIX = "mcp__gizai__";

/** "mcp__gizai__create_task" → "create_task"; other tools keep their name. */
export function toolName(raw: string): string {
  return raw.startsWith(PREFIX) ? raw.slice(PREFIX.length) : raw;
}

const VERBS: Record<string, string> = {
  get_overview: "Looked at the overview", read_inbox: "Read the inbox", list_clients: "Listed clients", get_client: "Opened client",
  list_projects: "Listed projects", get_project: "Opened project", list_tasks: "Listed tasks", get_task: "Opened task",
  list_agents: "Listed agents", get_agent: "Opened agent", list_docs: "Listed docs", read_doc: "Read doc", list_people: "Listed people",
  get_workflow: "Read the workflow", create_client: "Added client", update_client: "Changed client", save_contact: "Saved contact for",
  create_project: "Created project", update_project: "Changed project", create_task: "Created task", update_task: "Changed task",
  move_task: "Moved task", comment_on_task: "Commented on", create_agent: "Added agent", update_agent: "Changed agent",
  set_agent_status: "Changed agent status", add_routing_rule: "Added a routing rule", add_column: "Added a column", start_agent_run: "Started an agent on",
  stop_agent_run: "Stopped the agent on", create_doc: "Created doc", write_doc: "Wrote doc", attach_file: "Attached a file to",
  add_person: "Added person",
};
/** Claude Code's own (read-only) tools in chat. */
const BUILTIN: Record<string, string> = { Read: "Read a file", Glob: "Looked for files", Grep: "Searched the code" };

export type ToolCard = { verb: string; label?: string; href?: string; state: "running" | "ok" | "error"; detail?: string };

function parse(text: unknown): Record<string, unknown> | null {
  if (typeof text !== "string") return null;
  try { const v = JSON.parse(text); return v && typeof v === "object" ? v : null; } catch { return null; }
}

/** What a call was about, from its input: a task id, a project, a file name. */
function target(input: unknown): string | undefined {
  if (!input || typeof input !== "object") return undefined;
  const i = input as Record<string, unknown>;
  for (const k of ["task", "project", "client", "agent", "doc", "name", "title"]) if (typeof i[k] === "string" && i[k]) return i[k] as string;
  for (const k of ["file_path", "path"]) if (typeof i[k] === "string") return (i[k] as string).split("/").pop();
  if (typeof i.pattern === "string") return i.pattern;
  return undefined;
}

export function toolCard(m: ChatMessage): ToolCard {
  const raw = m.toolName ?? "";
  const name = toolName(raw);
  const verb = (raw.startsWith(PREFIX) ? VERBS[name] : BUILTIN[raw]) ?? raw;
  const t = (m.tool ?? {}) as Record<string, unknown>;
  const fromInput = target(t.input);
  if (t.result === undefined) return { verb, label: fromInput, state: "running" };
  if (t.isError) return { verb, label: fromInput, state: "error", detail: String(t.result) };
  const r = parse(t.result);
  const link = r?.link as { page?: string; id?: string; label?: string } | undefined;
  if (link?.page && link.id) return { verb, label: link.label ?? fromInput, href: href({ page: link.page as Page, id: link.id }), state: "ok" };
  if (typeof r?.count === "number") return { verb, label: `${r.count} ${r.count === 1 ? "item" : "items"}`, state: "ok" };
  return { verb, label: fromInput, state: "ok" };
}

export type Group = { side: "user" | "agent" | "system"; key: string; at: number; author?: string | null; items: ChatMessage[] };

const QUIET_MS = 5 * 60_000;

/** Consecutive messages of one speaker share a header; the Team Lead's text and tool calls are one speaker. */
export function groupMessages(msgs: ChatMessage[]): Group[] {
  const out: Group[] = [];
  for (const m of msgs) {
    const side: Group["side"] = m.role === "user" ? "user" : m.role === "system" ? "system" : "agent";
    const last = out[out.length - 1];
    const lastAt = last?.items[last.items.length - 1].createdAt ?? 0;
    if (last && last.side === side && side !== "system" && m.createdAt - lastAt <= QUIET_MS) last.items.push(m);
    else out.push({ side, key: m.id, at: m.createdAt, author: m.authorName, items: [m] });
  }
  return out;
}

/** Ways to start, on an empty chat. */
export const SUGGESTIONS: { label: string; text: string }[] = [
  { label: "What needs my attention?", text: "What needs my attention today?" },
  { label: "Add a client", text: "Add a client: " },
  { label: "Plan a project", text: "Plan a new project for " },
  { label: "Set up my team", text: "Set up my software team: a Frontend, a Backend and a QA agent, with the usual routing rules." },
];

/** The text being written, and the last change to it that it holds (ChatStatus.seq). */
export type LiveDraft = { text: string; seq: number };
/** A change to the text being written: words added, or a new block (the draft starts again). */
export type DraftChange = { kind: "delta"; text: string; seq: number } | { kind: "block"; seq: number };

/** Applies a change once: one the draft already holds (it came from a snapshot taken after it) is skipped. */
export function applyDraft(d: LiveDraft, c: DraftChange): LiveDraft {
  if (c.seq <= d.seq) return d;
  return c.kind === "delta" ? { text: d.text + c.text, seq: c.seq } : { text: "", seq: c.seq };
}

/** A snapshot of the text being written (chat_live) joined with the changes heard after it, also those that arrived while
 *  it was asked for: no words go missing, and none come twice. */
export function withSnapshot(snap: LiveDraft, heard: DraftChange[]): LiveDraft {
  return heard.filter((c) => c.seq > snap.seq).sort((a, b) => a.seq - b.seq).reduce(applyDraft, snap);
}

/** The coding CLI a chat runs on: its own pick while that is still in Settings, else the Team Lead's Runs on. */
export function chatRunsOn(chatCli: string | null | undefined, leadCli: string | null | undefined, clis: { id: string }[] | null): string {
  if (chatCli && (!clis || clis.some((c) => c.id === chatCli))) return chatCli;
  return leadCli || "claude_code";
}
