// GA-39: the agent form's Tools section: each MCP server from Settings with a switch (off until switched on), one line on
// its tools and their risk, its sign-in and last-run state, the npm/npx warning, and Codex/Gemini agents shown disabled
// with the reason. Rendered to HTML on the server, so no data loads: the servers and the agent's view are handed to the
// component's state directly (in the order its useState calls come). The switches' handlers are called on the element
// tree, with the hooks replaced, to see what the form gets.
import { describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { AgentMcpView, AgentServer, McpServerView, McpToolView } from "../types";
import coreRs from "../../crates/gizai-core/src/mcp_servers.rs?raw";

// Values for the next useState calls, in order; then each useState starts as written.
const queue: unknown[] = [];
// Direct: the component is called as a function (no React render): useState hands its value and a setter that does nothing.
let direct = false;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    const v = queue.length ? queue.shift() : typeof init === "function" ? (init as () => unknown)() : init;
    return direct ? [v, () => {}] : R.useState(v);
  }) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (direct ? undefined : R.useEffect(f, deps))) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});

const { AgentToolsField, MCP_NOT_ON } = await import("./AgentTools");
const { AgentDrawer } = await import("./AgentForm");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

const tool = (name: string, risk: string, summary: string): McpToolView => ({
  name, description: `${name} things`, params: [], hints: { readOnly: risk === "low", destructive: risk === "high", idempotent: false, openWorld: false },
  hintsSent: [], risk, summary, notes: [],
});
const server = (s: Partial<McpServerView> & { id: string; name: string }): McpServerView => ({
  transport: "http", command: "", args: [], envNames: [], url: `https://${s.name}.example.com/mcp`, headerNames: [], clientId: "", source: "",
  signIn: "", problem: null, missing: [], listed: null, usedBy: [], ...s,
});
const otus = server({ id: "s-otus", name: "otus", signIn: "signed_in", listed: { serverName: "otus", serverVersion: "1.2.0", listedAt: 0, tools: [
  tool("search", "low", "Only reads: it searches your notes."),
  tool("update_note", "medium", "Changes things: it edits a note."),
  tool("delete_note", "high", "May delete or overwrite: it removes a note."),
] } });
const local = server({ id: "s-local", name: "local", transport: "stdio", command: "npx", args: ["-y", "@acme/mcp"], url: "" });
const acme = server({ id: "s-acme", name: "acme", signIn: "needs_sign_in", headerNames: ["X-Api-Key"], missing: ["X-Api-Key"] });
const servers = [otus, local, acme];
const view: AgentMcpView = { disabled: null, warning: null, servers: [
  { serverId: "s-otus", name: "otus", transport: "http", on: true, toolsOff: [], signIn: "signed_in", lastRun: { status: "connected", at: 1_700_000_000_000 },
    tools: [], summary: "", risk: "high" },
  { serverId: "s-local", name: "local", transport: "stdio", on: false, toolsOff: [], signIn: "", lastRun: { status: "failed", at: 1_700_000_000_000 },
    tools: [], summary: "", risk: "unknown" },
] };

type Props = Parameters<typeof AgentToolsField>[0];
/** The field as HTML, with the servers and the agent's view loaded. */
function render(p: Partial<Props>, loaded: { servers?: McpServerView[] | null; view?: AgentMcpView | null; err?: string | null } = {}) {
  queue.push(loaded.servers === undefined ? servers : loaded.servers, loaded.view === undefined ? view : loaded.view, loaded.err ?? null);
  try {
    return renderToStaticMarkup(<AgentToolsField agentId="a1" kind="claude_code" allowedTools={[]} value={[]} onChange={() => {}} {...p} />);
  } finally { queue.length = 0; }
}
/** The HTML of one server's row. */
const row = (html: string, name: string) => {
  const at = html.indexOf(`aria-label="Use ${name}"`);
  const start = html.lastIndexOf("<li", at);
  const next = html.indexOf('<li class="ma-server', at);
  return html.slice(start, next < 0 ? undefined : next);
};

/** Calls function components down the tree, so every element's handlers can be reached. */
function expand(node: ReactNode): ReactNode {
  if (Array.isArray(node)) return node.map(expand);
  if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") return expand((el.type as (p: unknown) => ReactNode)(el.props));
    return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode) } } as ReactNode;
  }
  return node;
}
function findAll(node: ReactNode, pred: (p: Record<string, unknown>) => boolean, out: Record<string, unknown>[] = []) {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, pred, out));
  else if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<Record<string, unknown>>).props;
    if (pred(p)) out.push(p);
    findAll(p.children as ReactNode, pred, out);
  }
  return out;
}
/** The field's element tree with the servers loaded, and what its onChange got. */
function tree(p: Partial<Props>) {
  const got: AgentServer[][] = [];
  direct = true;
  queue.push(servers, view, null);
  try {
    const t = expand(AgentToolsField({ agentId: "a1", kind: "claude_code", allowedTools: [], value: [], onChange: (v) => got.push(v), ...p }));
    const input = (label: string) => findAll(t, (x) => x["aria-label"] === label)[0] as { onChange: (e: unknown) => void; disabled?: boolean; checked?: boolean };
    return { t, got, input };
  } finally { direct = false; queue.length = 0; }
}

describe("the Tools section in the agent form", () => {
  it("comes between Permissions and Instructions, with the MCP servers field", () => {
    const html = renderToStaticMarkup(<AgentDrawer teamId="t1" onClose={() => {}} />);
    const [perm, tools, instr] = ["<h3>Permissions</h3>", "<h3>Tools</h3>", "<h3>Instructions</h3>"].map((h) => html.indexOf(h));
    expect(perm).toBeGreaterThan(0);
    expect(tools).toBeGreaterThan(perm);
    expect(instr).toBeGreaterThan(tools);
    const section = html.slice(tools, instr);
    expect(section).toContain(">MCP servers<");
    expect(text(section)).toContain("Everything is off until you switch it on.");
  });
});

describe("AgentToolsField", () => {
  it("says it is loading, then that there are no servers yet", () => {
    expect(text(render({}, { servers: null }))).toContain("Loading the MCP servers…");
    expect(text(render({}, { servers: [] }))).toContain("No MCP servers yet: add or import them in Settings → MCP servers.");
  });

  it("lists every server off by default", () => {
    const html = render({ value: [] });
    expect(html).toContain('aria-label="MCP servers"');
    for (const s of servers) {
      expect(row(html, s.name)).toContain(`<b>${s.name}</b>`);
      expect(row(html, s.name)).not.toMatch(/checked/);
      expect(row(html, s.name)).not.toContain("disabled");
    }
    // a server that is off shows no tool switches
    expect(html).not.toContain('aria-label="Use search"');
    expect(text(html)).toContain("Off until you switch them on.");
  });

  it("says in one line per server what its tools allow and how risky they are", () => {
    const html = render({});
    const o = row(html, "otus");
    expect(text(o)).toContain("3 tools: 1 only read, 1 change things, 1 may delete or overwrite. High risk.");
    expect(o).toContain('<span class="badge fail">High risk</span>');
    const l = row(html, "local");
    expect(text(l)).toContain("Its tools aren't listed yet: List tools in Settings → MCP servers shows them.");
    expect(l).toContain('<span class="badge outline">Not listed yet</span>');
  });

  it("shows a server switched on with a switch, a risk and one line per tool, and how many are on", () => {
    const html = render({ value: [{ serverId: "s-otus", on: true, toolsOff: ["delete_note"] }] });
    const o = row(html, "otus");
    expect(o).toMatch(/aria-label="Use otus" checked=""/);
    expect(o).toMatch(/aria-label="Use search" checked=""/);
    expect(o).toMatch(/aria-label="Use update_note" checked=""/);
    expect(o).not.toMatch(/aria-label="Use delete_note" checked/);
    const t = text(o);
    expect(t).toContain("search Low risk Only reads: it searches your notes.");
    expect(t).toContain("update_note Medium risk Changes things: it edits a note.");
    expect(t).toContain("delete_note High risk May delete or overwrite: it removes a note.");
    expect(t).toContain("High risk. 2 of 3 on.");
  });

  it("says a server switched on before its tools are listed gets all of them", () => {
    const html = render({ value: [{ serverId: "s-local", on: true, toolsOff: [] }] });
    expect(text(row(html, "local"))).toContain("List its tools in Settings → MCP servers to switch them one by one; until then, all its tools are on.");
  });

  it("says next to a signed-in server's switch that its tools act as you in that service", () => {
    const html = render({});
    expect(text(row(html, "otus"))).toContain("Signed in Connected in its last run");
    expect(text(row(html, "otus"))).toContain("Signed in: its tools act as you in otus. Switch it on only for an agent you trust with that.");
    expect(row(html, "acme")).not.toContain("act as you");
    expect(row(html, "local")).not.toContain("act as you");
  });

  it("shows the sign-in state: Needs sign-in, and that a run leaves it out until then", () => {
    const html = render({});
    const a = row(html, "acme");
    expect(a).toContain('<span class="badge needs">Needs sign-in</span>');
    expect(text(a)).toContain("Needs sign-in: a run leaves it out until you sign in in Settings → MCP servers.");
    expect(text(a)).toContain("No value saved for X-Api-Key: a run leaves it out until you enter it in Settings → MCP servers.");
    expect(row(html, "local")).not.toContain("sign-in");
  });

  it("shows each server's state in the agent's last run: connected or failed", () => {
    const html = render({});
    expect(row(html, "otus")).toContain('<span class="badge ok" title="At ');
    expect(text(row(html, "otus"))).toContain("Connected in its last run");
    expect(row(html, "local")).toContain('class="badge fail"');
    expect(text(row(html, "local"))).toContain("Failed to connect in its last run");
    // a server that wasn't in the agent's last run (or a new agent) says nothing about it
    expect(text(row(html, "acme"))).not.toContain("last run");
    expect(text(render({ agentId: undefined }, { view: null }))).not.toContain("last run");
  });

  it("warns when a server is on and the agent may run npm or npx", () => {
    const warn = "This agent may run npm or npx, and an MCP server is on: a server's answer could try to make it run code. Take npm and npx out of its commands, or switch the server off.";
    const on: AgentServer[] = [{ serverId: "s-otus", on: true, toolsOff: [] }];
    for (const cmd of ["Bash(npm:*)", "Bash(npx:*)"]) {
      expect(text(render({ allowedTools: ["Bash(git:*)", cmd], value: on })), cmd).toContain(warn);
    }
    expect(text(render({ allowedTools: ["Bash(npm:*)"], value: [] }))).not.toContain(warn);
    expect(text(render({ allowedTools: ["Bash(npm:*)"], value: [{ serverId: "s-otus", on: false, toolsOff: ["search"] }] }))).not.toContain(warn);
    expect(text(render({ allowedTools: ["Bash(git:*)", "Bash(npm run build:*)"], value: on }))).not.toContain(warn);
  });

  for (const kind of ["codex", "gemini"] as const) {
    it(`shows the servers disabled with the reason for an agent on ${kind}`, () => {
      const html = render({ kind, allowedTools: ["Bash(npm:*)"], value: [{ serverId: "s-otus", on: true, toolsOff: [] }] });
      for (const s of servers) {
        expect(row(html, s.name)).toMatch(new RegExp(`aria-label="Use ${s.name}" disabled=""`));
        // the attribute, not the word: Codex's reason says what Gizai "hasn't checked yet"
        expect(row(html, s.name)).not.toContain('checked=""');
      }
      // GA-55: each CLI says why, in the same words as mcp_not_on in crates/gizai-core/src/mcp_servers.rs
      expect(text(html)).toContain(MCP_NOT_ON[kind]);
      expect(MCP_NOT_ON[kind]).toContain(kind === "codex" ? "Codex" : "Gemini");
      expect(MCP_NOT_ON[kind]).toContain("the browser");
      expect(coreRs).toContain(MCP_NOT_ON[kind]!.split(": ")[0]!);
      // no tool switches and no npm warning while it can't use them
      expect(html).not.toContain('aria-label="Use search"');
      expect(text(html)).not.toContain("This agent may run npm or npx");
    });
  }
});

describe("the switches", () => {
  it("switch a server on with all its tools, and leave the others as they are", () => {
    const { got, input } = tree({ value: [{ serverId: "s-local", on: false, toolsOff: ["x"] }] });
    input("Use otus").onChange({ target: { checked: true } });
    expect(got).toEqual([[{ serverId: "s-local", on: false, toolsOff: ["x"] }, { serverId: "s-otus", on: true, toolsOff: [] }]]);
  });

  it("switch one tool off, and a server off keeping its tools off", () => {
    const value: AgentServer[] = [{ serverId: "s-otus", on: true, toolsOff: [] }];
    const { got, input } = tree({ value });
    input("Use delete_note").onChange({ target: { checked: false } });
    input("Use otus").onChange({ target: { checked: false } });
    expect(got).toEqual([[{ serverId: "s-otus", on: true, toolsOff: ["delete_note"] }], [{ serverId: "s-otus", on: false, toolsOff: [] }]]);
  });

  it("are disabled for an agent on Codex", () => {
    const { input } = tree({ kind: "codex", value: [{ serverId: "s-otus", on: true, toolsOff: [] }] });
    expect([input("Use otus").disabled, input("Use otus").checked]).toEqual([true, false]);
    expect(input("Use search")).toBeUndefined();
  });
});
