// The browser mode's fake backend (GA-185): answers Tauri's commands from src/browser/fixtures.json. A command without a
// fixture answers empty, by the type src/api.ts gives it ([] for a list, false, 0, "" or null), and says so in the
// console once. A few writes change the fixtures in memory, so a form's save shows until the page is reloaded.
import { emit } from "@tauri-apps/api/event";
import apiSource from "../api.ts?raw";
import { screenshotPicture } from "./pictures";

/** An answer for any arguments, or answers by one argument's value. */
type Keyed = { $by: string; $answers: Record<string, unknown> };
export type Fixtures = { about?: string; writtenAt: number; commands: Record<string, unknown> };

let commands: Record<string, unknown> = {};

const isKeyed = (f: unknown): f is Keyed => !!f && typeof f === "object" && "$by" in f && "$answers" in f;

/** Moves every time (a field whose name ends in "At", and `at`) by `delta`, so "2 h ago" in the fixtures stays 2 h ago. */
function shiftTimes(v: unknown, delta: number): void {
  if (Array.isArray(v)) { v.forEach((x) => shiftTimes(x, delta)); return; }
  if (!v || typeof v !== "object") return;
  const o = v as Record<string, unknown>;
  for (const [k, x] of Object.entries(o)) {
    if (typeof x === "number" && (k === "at" || k.endsWith("At")) && x > 0) o[k] = x + delta;
    else shiftTimes(x, delta);
  }
}

export function loadFixtures(f: Fixtures) {
  commands = f.commands;
  shiftTimes(commands, Date.now() - f.writtenAt);
  console.info(`Gizai in a browser: ${Object.keys(commands).length} commands answer from the fixtures (src/browser/fixtures.json).`);
}

/** What each command answers without a fixture, from its type in src/api.ts. */
const EMPTY: Record<string, unknown> = (() => {
  const out: Record<string, unknown> = {};
  for (const m of apiSource.matchAll(/invoke<(.+?)>\(\s*"([^"]+)"/g)) {
    const [, type, cmd] = m;
    out[cmd] = type.trim().endsWith("[]") ? [] : type === "boolean" ? false : type === "number" ? 0 : type === "string" ? "" : null;
  }
  return out;
})();

const told = new Set<string>();
function empty(cmd: string, why: string) {
  const v = cmd in EMPTY ? EMPTY[cmd] : null;
  if (!told.has(why)) {
    told.add(why);
    console.warn(`Gizai in a browser: no fixture for ${why}, so it answers ${JSON.stringify(v)}.`);
  }
  return structuredClone(v);
}

/** The fixture `cmd` answers with these arguments, or undefined without one. */
function fixture(cmd: string, args: Record<string, unknown>): { found: boolean; value?: unknown; why: string } {
  const f = commands[cmd];
  if (f === undefined) return { found: false, why: cmd };
  if (!isKeyed(f)) return { found: true, value: f, why: cmd };
  const key = String(args[f.$by]);
  return key in f.$answers ? { found: true, value: f.$answers[key], why: cmd } : { found: false, why: `${cmd} (${f.$by} ${key})` };
}

/** The answer object behind a keyed fixture, to change it in place. */
function stored(cmd: string, key: string): Record<string, unknown> | undefined {
  const f = commands[cmd];
  return isKeyed(f) ? (f.$answers[key] as Record<string, unknown> | undefined) : undefined;
}

const changed = (table: string) => { void emit("rows-changed", { table }); };

type Row = Record<string, unknown>;

/** An agent as get_agent and every team in get_team have it, to change it in place. */
function agents(id: unknown): Row[] {
  const teams = commands.get_team;
  const members = isKeyed(teams) ? Object.values(teams.$answers).flatMap((t) => ((t as { members?: Row[] }).members ?? [])) : [];
  return [stored("get_agent", String(id)), ...members.filter((m) => m.actorId === id)].filter((m): m is Row => !!m);
}

/** The few writes the screens need to show what they saved. */
const WRITES: Record<string, (args: Row) => unknown> = {
  update_task: ({ id, patch }) => {
    const p = patch as Row;
    for (const t of [stored("get_task", String(id)), ...((commands.list_tasks as Row[] | undefined) ?? []).filter((x) => x.id === id)]) if (t) Object.assign(t, p);
    changed("tasks");
    return null;
  },
  move_task: ({ id, stateId, sortKey }) => {
    const team = fixture("get_team", { id: null }).value as { states?: Row[] } | undefined;
    const s = team?.states?.find((x) => x.id === stateId);
    for (const t of [stored("get_task", String(id)), ...((commands.list_tasks as Row[] | undefined) ?? []).filter((x) => x.id === id)]) {
      if (t) Object.assign(t, { stateId, sortKey, ...(s ? { stateName: s.name, stateCategory: s.category } : {}) });
    }
    changed("tasks");
    return null;
  },
  save_settings: ({ settings }) => { commands.get_settings = settings; changed("settings"); return null; },
  save_project: ({ id, input }) => {
    const i = input as Row;
    for (const p of [stored("get_project", String(id)), ...((commands.list_projects as Row[] | undefined) ?? []).filter((x) => x.id === id)]) if (p) Object.assign(p, i);
    changed("projects");
    return id ?? "new-project";
  },
  save_agent_cli_tools: ({ agentId, tools }) => {
    const view = stored("agent_cli_tools", String(agentId));
    const saved = { ...((view?.saved as Row | undefined) ?? {}), ...(tools as Row) };
    if (view) view.saved = saved;
    for (const a of agents(agentId)) a.cliTools = { ...((a.cliTools as Row | undefined) ?? {}), ...(tools as Row) };
    changed("actors");
    return saved;
  },
  // the browser's switch and the MCP servers' are the agent's own (its form's Tools)
  save_agent_mcp: ({ agentId, tools }) => {
    for (const a of agents(agentId)) a.tools = structuredClone(tools);
    changed("team");
    const view = fixture("agent_mcp", { agentId });
    return view.found ? structuredClone(view.value) : empty("agent_mcp", view.why);
  },
  open_file: ({ id }) => { console.info(`Gizai in a browser: Open would show file ${id} in the system's image viewer.`); return null; },
};

/** The answer to one command: a fixture, a write, a screenshot's picture, or empty. */
export async function answer(cmd: string, args: Row): Promise<unknown> {
  if (cmd in WRITES) return WRITES[cmd](args);
  if (cmd === "screenshot_bytes") return picture(String(args.fileId));
  if (cmd === "list_tasks") return tasks(args.filter as Row | undefined);
  const f = fixture(cmd, args);
  return f.found ? structuredClone(f.value) : empty(cmd, f.why);
}

/** list_tasks: every card in the fixtures, filtered as the backend does by project and open ones only. */
function tasks(filter: Row | undefined) {
  const all = commands.list_tasks as Row[] | undefined;
  if (!all) return empty("list_tasks", "list_tasks");
  return structuredClone(all.filter((t) => (!filter?.projectId || t.projectId === filter.projectId)
    && (!filter?.openOnly || !["done", "cancelled"].includes(String(t.stateCategory)))));
}

/** A screenshot's bytes: a picture drawn from its size and caption (the fixtures hold no images). */
function picture(fileId: string): Promise<ArrayBuffer> {
  const sets = commands.list_screenshots;
  const all = isKeyed(sets) ? Object.values(sets.$answers).flat() as { shots: { fileId: string; name: string; caption: string; width: number; height: number }[] }[] : [];
  const shot = all.flatMap((s) => s.shots).find((s) => s.fileId === fileId);
  if (!shot) return Promise.reject("That file isn't a run's screenshot: only screenshots are shown here.");
  return screenshotPicture(shot);
}
