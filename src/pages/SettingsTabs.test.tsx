// GA-42: Settings in tabs. The tabs look and work like the Usage page's (the shared Tabs component, role="tablist", an icon
// and a label each); #/settings/<tab> picks the open one and General opens first; each tab has the sections the card lists.
// Every tab stays on the page and only the open one shows, so an edit not saved yet survives a tab switch and Save settings
// (in the top bar) saves it from any tab. Rendered to HTML on the server like SettingsQuitNotifications.test.tsx: the page's
// state is handed in in the order of its useState calls (settings, budget, msg, detecting, savedAt, agents), and the
// sections other cards own are stubs that only say where they are.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Settings } from "../types";
import type { Route } from "../router";

const queue: unknown[] = [];
let sets: unknown[][] | null = null;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    const v = queue.length ? queue.shift() : typeof init === "function" ? (init as () => unknown)() : init;
    if (!sets) return R.useState(v);
    const mine: unknown[] = [];
    sets.push(mine);
    return [v, (x: unknown) => mine.push(x)];
  }) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (sets ? undefined : R.useEffect(f, deps))) as typeof R.useEffect;
  const useCallback = ((f: unknown, deps?: unknown[]) => (sets ? f : R.useCallback(f as () => void, deps ?? []))) as typeof R.useCallback;
  return { ...R, default: { ...R, useState, useEffect, useCallback }, useState, useEffect, useCallback };
});

const calls: [string, unknown[]][] = [];
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { calls.push([k, a]); return Promise.resolve(); };
  }
  return out;
});
const went: Route[] = [];
vi.mock("../router", async (orig) => ({ ...(await orig<typeof import("../router")>()), go: (r: Route) => { went.push(r); } }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../components/chat/useChat", () => ({ useChatLive: () => [] }));
const stub = (title: string) => () => <section className="form-section"><header><h3>{title}</h3></header></section>;
vi.mock("../components/UpdateSettings", () => ({ UpdateSettings: stub("Updates") }));
vi.mock("../components/GithubSettings", () => ({ GithubSettings: stub("GitHub") }));
vi.mock("../components/BitbucketSettings", () => ({ BitbucketSettings: stub("Bitbucket") }));
vi.mock("../components/CliSettings", () => ({ CliSettings: () => <i>the CLI list</i> }));
vi.mock("../components/McpSettings", () => ({ McpSettings: () => <i>the MCP server list</i> }));
vi.mock("../components/OldWorktrees", () => ({ OldWorktrees: () => <i>the old worktrees</i> }));

const { SettingsPage } = await import("./SettingsPage");
const { parseHash, href } = await import("../router");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const settle = () => new Promise((r) => setTimeout(r, 0));

const SETTINGS: Settings = {
  claudeBin: "/usr/bin/claude", dataDir: "/home/you/.local/share/gizai", maxConcurrentRuns: 4, agentsPaused: false, maxRunUsd: null,
  maxRunMinutes: 45, maxRunToolCalls: 400, pushOver: "ssh",
  notifications: { hold: true, waiting: true, leadAsks: true, leadAnswered: true },
};
const states = (s: Settings = SETTINGS, budget = "") => [s, budget, null, false, 0, []];

function render(tab?: string, s?: Settings, budget?: string) {
  queue.push(...states(s, budget));
  try { return renderToStaticMarkup(<SettingsPage tab={tab} />); } finally { queue.length = 0; }
}
/** The page's element tree with its components called, and what each useState setter was given (in order). */
function tree(tab?: string, s?: Settings, budget?: string) {
  sets = [];
  queue.push(...states(s, budget));
  try { return { t: expand(SettingsPage({ tab })), got: sets }; } finally { sets = null; queue.length = 0; }
}
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

const LABELS = ["General", "Appearance", "Notifications", "Agents and runs", "MCP servers", "GitHub and Bitbucket"];
const KEYS = ["general", "appearance", "notifications", "agents", "mcp", "github"];
/** Each panel's label, whether it is hidden, and its HTML. */
function panels(html: string) {
  const starts = [...html.matchAll(/<div role="tabpanel" class="form settings-panel"( hidden="")? aria-label="([^"]+)">/g)];
  return starts.map((m, i) => ({ label: m[2] ?? "", hidden: !!m[1], html: html.slice(m.index ?? 0, starts[i + 1]?.index ?? html.length) }));
}
const sections = (html: string) => [...html.matchAll(/<section class="form-section"><header><h3>([^<]+)<\/h3>/g)].map((m) => m[1]);

beforeEach(() => { calls.length = 0; went.length = 0; });

describe("Settings' tabs", () => {
  it("look like the Usage page's: a tablist with an icon and a label per tab, in the card's order", () => {
    const html = render();
    expect(html).toContain('<div class="tabs" role="tablist" aria-label="Settings">');
    const tabs = [...html.matchAll(/<button class="tab" role="tab" aria-selected="(true|false)"><svg[^>]*class="lucide[^"]*icon"[^>]*>.*?<\/svg>([^<]+)<\/button>/g)];
    expect(tabs.map((m) => m[2])).toEqual(LABELS);
    // the tabs come before the panels
    expect(html.indexOf('role="tablist"')).toBeLessThan(html.indexOf('role="tabpanel"'));
  });
  it("open General first, with only its panel showing", () => {
    for (const tab of [undefined, "", "nope"]) {
      const html = render(tab);
      expect(html.match(/aria-selected="true">.*?<\/svg>([^<]+)<\/button>/)?.[1]).toBe("General");
      expect(panels(html).map((p) => [p.label, p.hidden])).toEqual(LABELS.map((l) => [l, l !== "General"]));
    }
  });
  it("open the tab in the address, #/settings/<tab>, and #/settings opens General", () => {
    KEYS.forEach((key, i) => {
      const route = parseHash(`#/settings/${key}`);
      const html = render(route.id);
      expect(html.match(/aria-selected="true">.*?<\/svg>([^<]+)<\/button>/)?.[1]).toBe(LABELS[i]);
      expect(panels(html).filter((p) => !p.hidden).map((p) => p.label)).toEqual([LABELS[i]]);
    });
    expect(panels(render(parseHash("#/settings").id)).filter((p) => !p.hidden).map((p) => p.label)).toEqual(["General"]);
  });
  it("the update notice's link (#/settings) opens General, where Updates are", () => {
    const html = render(parseHash(href({ page: "settings" })).id);
    const general = panels(html).find((p) => p.label === "General");
    expect(general?.hidden).toBe(false);
    expect(sections(general?.html ?? "")).toContain("Updates");
  });
  it("a click on a tab puts it in the address", () => {
    const { t } = tree();
    const tabs = findAll(t, (p) => p.role === "tab") as unknown as { onClick: () => void; children: unknown }[];
    expect(tabs.length).toBe(6);
    tabs.forEach((b) => b.onClick());
    expect(went).toEqual(KEYS.map((id) => ({ page: "settings", id })));
  });
});

describe("what each tab has", () => {
  const html = render();
  const of = (label: string) => panels(html).find((p) => p.label === label)?.html ?? "";
  it("General: Quit Gizai completely at the top, Updates, then Data with the folders and the old worktrees", () => {
    expect(sections(of("General"))).toEqual(["Quit", "Updates", "Data"]);
    const t = text(of("General"));
    for (const s of ["Data folder", "/home/you/.local/share/gizai/worktrees", "Run logs", "Worktrees of finished cards", "the old worktrees"]) expect(t).toContain(s);
  });
  it("Appearance: Font, the three text sizes, Theme and Density, and Reset to defaults", () => {
    expect(sections(of("Appearance"))).toEqual(["Font", "Text size", "Theme and density"]);
    const a = of("Appearance");
    for (const l of ["Font", "Chat size", "Interface size", "Tasks and docs size", "Theme", "Density"]) expect(a).toContain(`aria-label="${l}"`);
    expect(text(a)).toContain("Reset to defaults");
  });
  it("Notifications: the four switches", () => {
    expect(sections(of("Notifications"))).toEqual(["Notifications"]);
    expect(of("Notifications").match(/type="checkbox"/g)?.length).toBe(4);
  });
  it("Agents and runs: Coding CLIs (the Claude Code program and the CLI list), then Runs with its limits and Pause all agents", () => {
    const a = of("Agents and runs");
    expect(sections(a)).toEqual(["Coding CLIs", "Runs"]);
    for (const id of ["s-bin", "s-max", "s-usd"]) expect(a).toContain(`id="${id}"`);
    const t = text(a);
    for (const s of ["the CLI list", "Runs at once", "Pause all agents"]) expect(t).toContain(s);
    expect(t.indexOf("Coding CLIs")).toBeLessThan(t.indexOf("Runs at once"));
  });
  it("MCP servers: the server list", () => {
    expect(sections(of("MCP servers"))).toEqual(["MCP servers"]);
    expect(text(of("MCP servers"))).toContain("the MCP server list");
  });
  it("GitHub and Bitbucket: GitHub, then Bitbucket", () => {
    expect(sections(of("GitHub and Bitbucket"))).toEqual(["GitHub", "Bitbucket"]);
  });
  it("every section is on exactly one tab", () => {
    const all = sections(html);
    expect(all).toEqual(["Quit", "Updates", "Data", "Font", "Text size", "Theme and density", "Notifications", "Coding CLIs", "Runs", "MCP servers", "GitHub", "Bitbucket"]);
  });
});

describe("saving across tabs", () => {
  it("keeps every tab on the page, so an edit on a tab that isn't open is still there", () => {
    const edited = { ...SETTINGS, claudeBin: "/opt/claude-next", maxConcurrentRuns: 7 };
    const html = render("appearance", edited, "12.5");
    const agents = panels(html).find((p) => p.label === "Agents and runs");
    expect(agents?.hidden).toBe(true);
    expect(agents?.html).toMatch(/id="s-bin"[^>]*value="\/opt\/claude-next"/);
    expect(agents?.html).toMatch(/id="s-max"[^>]*value="7"/);
    expect(agents?.html).toMatch(/id="s-usd"[^>]*value="12.5"/);
  });
  it("Save settings in the top bar saves an edit made on another tab", async () => {
    // edit the Claude Code program on Agents and runs
    const onAgents = tree("agents");
    const bin = findAll(onAgents.t, (p) => p.id === "s-bin")[0] as { onChange: (e: unknown) => void };
    bin.onChange({ target: { value: "/opt/claude-next" } });
    const edited = onAgents.got[0]?.[0] as Settings;
    expect(edited.claudeBin).toBe("/opt/claude-next");
    // switch to General (the same page, its state kept) and press Save settings
    const onGeneral = tree("general", edited, "3");
    const save = findAll(onGeneral.t, (p) => p.className === "btn primary" && typeof p.onClick === "function")[0] as { onClick: () => void; children: unknown };
    expect(save.children).toBe("Save settings");
    save.onClick();
    await settle();
    const saved = calls.filter(([k]) => k === "saveSettings").map(([, a]) => a[0] as Settings);
    expect(saved).toEqual([{ ...SETTINGS, claudeBin: "/opt/claude-next", maxRunUsd: 3 }]);
  });
  it("the Save settings button is in the top bar, outside the tabs, whichever tab is open", () => {
    for (const key of KEYS) {
      const html = render(key);
      const bar = html.slice(html.indexOf('<div class="topbar">'), html.indexOf('<div class="content">'));
      expect(bar).toContain('<button class="btn primary">Save settings</button>');
    }
  });
  it("Pause all agents still saves at once, on its own tab", async () => {
    const { t } = tree("agents");
    const pause = findAll(t, (p) => p.type === "checkbox" && typeof p.onChange === "function").at(-1) as { onChange: (e: unknown) => void };
    pause.onChange({ target: { checked: true } });
    await settle();
    const saved = calls.filter(([k]) => k === "saveSettings").map(([, a]) => a[0] as Settings);
    expect(saved).toEqual([{ ...SETTINGS, agentsPaused: true, maxRunUsd: null }]);
  });
});
