// GA-55: the agent form's Tools → Web, Browser and Built-in tools. Everything shows off until switched on, each item says
// in one line what it allows and how risky it is, the browser says whether Node and Chrome are found (and what to install
// if not), the form warns when the web or the browser is on next to Bash(npm:*) or Bash(npx:*), and what the agent's CLI
// can't take is disabled with the reason. Rendered to HTML on the server with the component's state handed in (in the
// order of its useState calls: view, err, asking, domains); the switches are used on the element tree.
import { describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { AgentServer, CatalogTool, CliTools, ToolsView } from "../types";

const queue: unknown[] = [];
// Direct: the component is called as a function; useState hands its value and a setter that does nothing.
let direct = false;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    const v = queue.length ? queue.shift() : typeof init === "function" ? (init as () => unknown)() : init;
    return direct ? [v, () => {}] : R.useState(v);
  }) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (direct ? undefined : R.useEffect(f, deps))) as typeof R.useEffect;
  const useRef = ((init: unknown) => (direct ? { current: init } : R.useRef(init))) as typeof R.useRef;
  const hooks = { useState, useEffect, useRef };
  return { ...R, ...hooks, default: { ...R, ...hooks } };
});

const { AgentCliToolsFields } = await import("./AgentCliTools");
const { NO_CLI_TOOLS, fitCliTools, parseDomains, sameCliTools, switchBuiltin, switchBrowser, browserOn } = await import("../lib/cliTools");
const { npmWebWarning } = await import("../lib/mcp");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

const tool = (id: string, label: string, group: string, risk: string, how: string, description: string, note = "", reported = false): CatalogTool =>
  ({ id, label, group, risk, how, description, note, reported });
const claudeTools: CatalogTool[] = [
  tool("WebSearch", "Web search", "web", "medium", "web", "Searches the web. Results are untrusted text, and the search terms leave your computer.", "", true),
  tool("WebFetch", "Fetch web pages", "web", "high", "web", "Reads web pages, any or only the domains you list. Pages are untrusted, and an address can carry data out."),
  tool("Read", "Read files", "files", "low", "always", "Reads files in its worktree and its folders.", "Claude Code uses it without asking.", true),
  tool("Bash", "Run commands", "commands", "high", "elsewhere", "Runs shell commands.", "Only the commands under Permissions → Allowed commands."),
  tool("Skill", "Skills", "other", "medium", "off", "Loads a skill from a plugin or your settings.", "Gizai keeps skills and slash commands off in agent runs (--disable-slash-commands)."),
  tool("FancyNewTool", "FancyNewTool", "other", "unknown", "switch", "Claude Code reports it, and Gizai's catalog doesn't know it yet: check what it does before you switch it on.", "", true),
];
const found = { node: "/usr/bin/node", nodeVersion: "v24.1.0", npx: "/usr/bin/npx", browser: "/usr/bin/chromium", browserName: "Chromium", missing: [] };
const claude: ToolsView = {
  kind: "claude_code", web: { search: null, fetch: null, domains: null },
  browser: { disabled: null, needs: found, version: "1.10.1", lastRun: null, tools: [], summary: "", risk: "high" },
  builtin: { tools: claudeTools, source: "Gizai's catalog, with the tools Claude Code reported in this agent's last run (2 min ago).", canAsk: true },
  saved: { ...NO_CLI_TOOLS },
};
const codex: ToolsView = {
  kind: "codex", web: { search: null, fetch: "Codex has no tool that fetches a page: only web search.", domains: "Codex has no tool that fetches a page." },
  browser: { ...claude.browser, disabled: "Codex takes MCP servers per run, but Gizai hasn't checked yet that a headless codex exec may call their tools: MCP servers and the browser stay off for Codex agents for now." },
  builtin: { tools: [tool("web_search", "Web search", "web", "medium", "web", "Searches the web (Codex's live search)."),
    tool("web_fetch", "Fetch web pages", "web", "high", "off", "Reads web pages.", "Codex has no tool of its own that fetches a page.")],
    source: "From Gizai's catalog: Codex has no command that lists its tools without a model call.", canAsk: false },
  saved: { ...NO_CLI_TOOLS },
};

type Props = { view?: ToolsView; value?: CliTools; mcp?: AgentServer[]; allowedTools?: string[]; lead?: boolean };
const props = (p: Props) => ({ agentId: "a1", cliId: "claude_code", allowedTools: p.allowedTools ?? ["Bash(git:*)"], value: p.value ?? { ...NO_CLI_TOOLS },
  onChange: () => {}, mcp: p.mcp ?? [], onMcp: () => {}, lead: p.lead ?? false });
const render = (p: Props = {}) => {
  queue.push(p.view ?? claude, null, false, (p.value?.fetchDomains ?? []).join("\n"));
  try { return renderToStaticMarkup(<AgentCliToolsFields {...props(p)} />); } finally { queue.length = 0; }
};

function expand(node: ReactNode): ReactNode {
  if (Array.isArray(node)) return node.map(expand);
  if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") return expand((el.type as (p: unknown) => ReactNode)(el.props));
    return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode) } } as ReactNode;
  }
  return node;
}
function find(node: ReactNode, label: string): Record<string, unknown> | undefined {
  if (Array.isArray(node)) { for (const n of node) { const f = find(n, label); if (f) return f; } return undefined; }
  if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<Record<string, unknown>>).props;
    if (p["aria-label"] === label) return p;
    return find(p.children as ReactNode, label);
  }
  return undefined;
}
/** The element tree with state from `p`; what onChange and onMcp were given. */
function tree(p: Props) {
  const changed: CliTools[] = [];
  const mcp: AgentServer[][] = [];
  direct = true;
  queue.push(p.view ?? claude, null, false, "");
  try {
    const t = expand(AgentCliToolsFields({ ...props(p), onChange: (v: CliTools) => changed.push(v), onMcp: (v: AgentServer[]) => mcp.push(v) }));
    return { input: (label: string) => find(t, label) as { onChange: (e: unknown) => void; checked?: boolean; disabled?: boolean } | undefined, changed, mcp };
  } finally { direct = false; queue.length = 0; }
}
const box = (html: string, label: string) => html.match(new RegExp(`<input[^>]*aria-label="${label}"[^>]*>`))?.[0] ?? "";

const WARN = "a web page could try to make it run code";
const browserSwitch: AgentServer[] = [{ serverId: "chrome-devtools", on: true, toolsOff: [] }];

describe("the Tools section's Web, Browser and Built-in tools", () => {
  it("shows every switch off for an agent with nothing on, each with what it allows and its risk", () => {
    const html = render();
    for (const label of ["Search the web", "Fetch web pages", "Use the hidden browser", "Use FancyNewTool"]) {
      expect(box(html, label), label).not.toBe("");
      expect(box(html, label), label).not.toContain("checked");
    }
    const t = text(html);
    expect(t).toContain("Search the web WebSearch Medium risk Searches the web. Results are untrusted text");
    expect(t).toContain("Fetch web pages WebFetch High risk Reads web pages");
    expect(t).toContain("Test web pages in a hidden browser chrome-devtools 1.10.1 High risk Opens, reads and clicks web pages");
    expect(t).toContain("Read files Read Low risk Always on Reported Reads files in its worktree and its folders. Claude Code uses it without asking.");
    expect(t).toContain("Skills Skill Medium risk Off");
    // a tool the catalog doesn't know: under Other tools the CLI reports, risk unknown, off
    expect(t).toContain("Other tools the CLI reports FancyNewTool FancyNewTool Risk unknown Reported Claude Code reports it");
    expect(t).not.toContain("Accept self-signed certificates");
    expect(t).not.toContain(WARN);
    expect(t).toContain("Suggested for the Team Lead (web search), never switched on by Gizai");
    expect(t).toContain("Suggested for QA, Frontend and Design agents, never switched on by Gizai");
    expect(t).toContain("hidden Chrome with a throwaway profile, never on your screen");
    expect(t).toContain("Every click is a tool call, and the cap per run is in Settings → Runs");
    expect(t).toContain("reported in this agent's last run");
    expect(html).toContain("Ask Claude Code again");
  });

  it("says Node and the browser were found, or what is missing and what to install", () => {
    expect(text(render())).toContain("Found: Node v24.1.0, npx, Chromium (/usr/bin/chromium).");
    const missing = ["Node isn't found: install Node 20.19 or newer (22.12 or newer on Node 22) (it comes with npx).",
      "No Google Chrome or Chromium found: install one of them (Brave doesn't count)."];
    const t = text(render({ view: { ...claude, browser: { ...claude.browser, needs: { missing } } } }));
    for (const m of missing) expect(t).toContain(m);
    expect(t).not.toContain("Found:");
  });

  it("warns when web search, fetching pages or the browser is on next to Bash(npm:*) or Bash(npx:*)", () => {
    for (const [allowedTools, p] of [
      [["Bash(npm:*)"], { value: { ...NO_CLI_TOOLS, webSearch: true } }],
      [["Bash(npx:*)"], { value: { ...NO_CLI_TOOLS, webFetch: true } }],
      [["Bash(git:*)", "Bash(npm:*)"], { mcp: browserSwitch }],
    ] as [string[], Props][]) {
      expect(text(render({ ...p, allowedTools })), allowedTools.join()).toContain(WARN);
    }
    expect(text(render({ allowedTools: ["Bash(npm:*)"] }))).not.toContain(WARN);
    expect(text(render({ allowedTools: ["Bash(git:*)"], value: { ...NO_CLI_TOOLS, webSearch: true, webFetch: true }, mcp: browserSwitch }))).not.toContain(WARN);
    expect(npmWebWarning(["Bash(npm:*)"], false)).toBeNull();
    expect(npmWebWarning(["Bash(npx:*)"], true)).toContain(WARN);
  });

  it("switches each item on for this agent only through the form", () => {
    const t = tree({});
    t.input("Search the web")!.onChange({ target: { checked: true } });
    t.input("Fetch web pages")!.onChange({ target: { checked: true } });
    t.input("Use FancyNewTool")!.onChange({ target: { checked: true } });
    t.input("Use the hidden browser")!.onChange({ target: { checked: true } });
    expect(t.changed.map((c) => [c.webSearch, c.webFetch, c.builtin])).toEqual([[true, false, []], [false, true, []], [false, false, ["FancyNewTool"]]]);
    expect(t.mcp).toEqual([[{ serverId: "chrome-devtools", on: true, toolsOff: [] }]]);
    // tools that are always on, set elsewhere or off have no switch
    for (const id of ["Read", "Bash", "Skill"]) expect(t.input(`Use ${id}`), id).toBeUndefined();
    // with the browser on: the certificate switch, off
    const b = tree({ mcp: browserSwitch });
    expect(b.input("Accept self-signed certificates")!.checked).toBe(false);
    b.input("Accept self-signed certificates")!.onChange({ target: { checked: true } });
    expect(b.changed[0]!.insecureCerts).toBe(true);
  });

  it("disables what a Codex agent can't take, with the reason, and labels the list as Gizai's catalog", () => {
    const html = render({ view: codex, value: { ...NO_CLI_TOOLS, webFetch: true }, mcp: browserSwitch });
    expect(box(html, "Fetch web pages")).toContain('disabled=""');
    expect(box(html, "Fetch web pages")).not.toContain("checked");
    expect(box(html, "Use the hidden browser")).toContain('disabled=""');
    expect(box(html, "Use the hidden browser")).not.toContain("checked");
    expect(box(html, "Search the web")).not.toContain("disabled");
    const t = text(html);
    expect(t).toContain("Codex has no tool that fetches a page: only web search.");
    expect(t).toContain("MCP servers and the browser stay off for Codex agents for now.");
    expect(t).toContain("From Gizai's catalog: Codex has no command that lists its tools without a model call.");
    expect(html).not.toContain("Ask Claude Code again");
    expect(t).not.toContain(WARN);
  });

  it("shows the domain list only while fetching is on, one per line", () => {
    expect(render()).not.toContain('aria-label="Fetch only these domains"');
    const html = render({ value: { ...NO_CLI_TOOLS, webFetch: true, fetchDomains: ["docs.rs", "*.laravel.com"] } });
    expect(html).toMatch(/aria-label="Fetch only these domains"[^>]*>docs.rs\n\*.laravel.com</);
  });
});

describe("the switches' helpers", () => {
  it("keep only what each CLI takes", () => {
    const all: CliTools = { webSearch: true, webFetch: true, fetchDomains: ["docs.rs"], insecureCerts: true, builtin: ["FancyNewTool"] };
    expect(fitCliTools(all, "claude_code")).toEqual(all);
    expect(fitCliTools(all, "codex")).toEqual({ ...all, webFetch: false, fetchDomains: [], builtin: [] });
    expect(fitCliTools(all, "gemini")).toEqual({ ...all, webSearch: false, fetchDomains: [], builtin: [] });
    expect(fitCliTools(all, "other")).toEqual(NO_CLI_TOOLS);
    expect(NO_CLI_TOOLS).toEqual({ webSearch: false, webFetch: false, fetchDomains: [], insecureCerts: false, builtin: [] });
  });

  it("read domains, compare and switch", () => {
    expect(parseDomains(" docs.rs\n*.laravel.com, docs.rs  kade.test ")).toEqual(["docs.rs", "*.laravel.com", "kade.test"]);
    expect(sameCliTools({ ...NO_CLI_TOOLS, fetchDomains: ["x.com"] }, NO_CLI_TOOLS)).toBe(true);
    expect(sameCliTools({ ...NO_CLI_TOOLS, builtin: ["B", "A"] }, { ...NO_CLI_TOOLS, builtin: ["A", "B"] })).toBe(true);
    expect(sameCliTools({ ...NO_CLI_TOOLS, webSearch: true }, NO_CLI_TOOLS)).toBe(false);
    expect(switchBuiltin(switchBuiltin(NO_CLI_TOOLS, "Z", true), "A", true).builtin).toEqual(["A", "Z"]);
    expect(switchBuiltin({ ...NO_CLI_TOOLS, builtin: ["A"] }, "A", false).builtin).toEqual([]);
    expect(browserOn(switchBrowser([], true))).toBe(true);
    expect(browserOn(switchBrowser(browserSwitch, false))).toBe(false);
  });
});
