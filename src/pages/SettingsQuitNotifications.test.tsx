// GA-21: the top of Settings. Quit Gizai completely with its warning (how many runs and chat answers it stops), the
// Notifications section under it with a switch per kind, and the warning under Runs at once (was GA-27). Rendered to HTML
// on the server, so no data loads: the page's state is handed in, in the order of its useState calls (settings, budget,
// msg, detecting, savedAt, agents), and the live runs and chat answers come from the replaced hooks. Switches and the
// Quit button are used on the element tree with the hooks replaced, and the api is mocked to see what they send. The
// sections below Notifications that GA-21 didn't change are stubs.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { ChatStatus, LiveRun, Settings } from "../types";
import type { CardAgent } from "../lib/settings";

const queue: unknown[] = [];
// Direct: components are called as functions; each useState's setter records what it is given, in `sets[i]`.
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
let liveRuns: LiveRun[] = [];
let liveChats: ChatStatus[] = [];
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => liveRuns }));
vi.mock("../components/chat/useChat", () => ({ useChatLive: () => liveChats }));
vi.mock("../components/UpdateSettings", () => ({ UpdateSettings: () => <section className="form-section"><h3>Updates</h3></section> }));
vi.mock("../components/CliSettings", () => ({ CliSettings: () => null }));
vi.mock("../components/GithubSettings", () => ({ GithubSettings: () => null }));
vi.mock("../components/BitbucketSettings", () => ({ BitbucketSettings: () => null }));
vi.mock("../components/McpSettings", () => ({ McpSettings: () => null }));
vi.mock("../components/OldWorktrees", () => ({ OldWorktrees: () => null }));

const { SettingsPage } = await import("./SettingsPage");
const { QuitSettings } = await import("../components/QuitSettings");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const settle = () => new Promise((r) => setTimeout(r, 0));

const SETTINGS: Settings = {
  claudeBin: "/usr/bin/claude", dataDir: "/home/you/.local/share/gizai", maxConcurrentRuns: 4, agentsPaused: false, maxRunUsd: null,
  maxRunMinutes: 45, maxRunToolCalls: 400, pushOver: "ssh",
  notifications: { hold: true, waiting: true, leadAsks: true, leadAnswered: true },
};
const AGENTS: CardAgent[] = [{ name: "Backend Agent", cardsAtOnce: 3 }, { name: "QA Agent", cardsAtOnce: 1 }, { name: "Frontend Agent", cardsAtOnce: 2 }];
const run = (n: number): LiveRun => ({ runId: `r${n}`, taskId: `t${n}`, agentId: "be" });
const answer = (n: number): ChatStatus => ({ threadId: `c${n}`, runId: `cr${n}`, draft: "", seq: 0 });
const LABELS = [
  "A card goes on hold",
  "A card waits for your review or deploy",
  "The Team Lead asks you something (a Question or Approval chat)",
  "The Team Lead answers in a chat while the Gizai window is hidden or in the background",
];

type Page = { s?: Partial<Settings>; agents?: CardAgent[] };
const states = (p: Page) => [{ ...SETTINGS, ...p.s }, "", null, false, 0, p.agents ?? AGENTS];

function render(p: Page = {}) {
  queue.push(...states(p));
  try { return renderToStaticMarkup(<SettingsPage />); } finally { queue.length = 0; }
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
beforeEach(() => { calls.length = 0; liveRuns = []; liveChats = []; });

describe("the top of Settings", () => {
  it("starts with Quit Gizai completely, then Notifications, then the other sections", () => {
    const t = text(render());
    const quit = t.indexOf("Quit Gizai completely");
    const notifications = t.indexOf("Notifications");
    const updates = t.indexOf("Updates");
    const runs = t.indexOf("Runs at once");
    expect(quit).toBeGreaterThan(-1);
    expect(t.indexOf("Quit")).toBeLessThan(notifications);
    expect(quit).toBeLessThan(notifications);
    expect(notifications).toBeLessThan(updates);
    expect(updates).toBeLessThan(runs);
    const html = render();
    expect(html.indexOf('<section class="form-section"><header><h3>Quit</h3>')).toBe(html.indexOf('<section class="form-section">'));
    expect(html).toMatch(/<button class="btn danger">.*<span>Quit Gizai completely<\/span><\/button>/);
  });
  it("warns what quitting stops and that closing the window only hides Gizai", () => {
    const t = text(render());
    expect(t).toContain("Quitting stops all agents (running cards are stopped), heartbeats and notifications until you start Gizai again. Closing the window only hides Gizai.");
    expect(t).not.toContain("will be stopped");
  });
  it("says how many runs and chat answers will be stopped while some are live", () => {
    liveRuns = [run(1), run(2)];
    liveChats = [answer(1)];
    const html = render();
    expect(html).toMatch(/<span class="warn">Quitting stops all agents[^<]*Now 2 runs and 1 chat answer will be stopped\.<\/span>/);
    liveRuns = [];
    expect(text(render())).toContain("Now 1 chat answer will be stopped.");
  });
});

describe("Settings → Notifications", () => {
  it("has a switch per kind, all on by default", () => {
    const html = render();
    for (const l of LABELS) expect(html).toContain(`<label class="check"><input type="checkbox" checked=""/>${l}</label>`);
  });
  it("shows a switched-off kind as off", () => {
    const html = render({ s: { notifications: { hold: true, waiting: false, leadAsks: true, leadAnswered: false } } });
    expect(html).toContain(`<input type="checkbox"/>${LABELS[1]}`);
    expect(html).toContain(`<input type="checkbox"/>${LABELS[3]}`);
    expect(html).toContain(`<input type="checkbox" checked=""/>${LABELS[0]}`);
  });
  it("saves a switch as soon as it is switched", async () => {
    sets = [];
    queue.push(...states({}));
    let tree: ReactNode;
    try { tree = expand(SettingsPage()); } finally { queue.length = 0; }
    const got = sets;
    sets = null;
    const boxes = findAll(tree, (p) => p.type === "checkbox" && typeof p.onChange === "function") as { onChange: (e: unknown) => void; checked: boolean }[];
    // the four switches come first on the page, before Pause all agents
    expect(boxes.length).toBe(5);
    boxes[2].onChange({ target: { checked: false } });
    await settle();
    const want = { hold: true, waiting: true, leadAsks: false, leadAnswered: true };
    expect(got[0]).toEqual([{ ...SETTINGS, notifications: want }]);
    const saved = calls.filter(([k]) => k === "saveSettings");
    expect(saved.length).toBe(1);
    expect((saved[0][1][0] as Settings).notifications).toEqual(want);
  });
});

describe("Quit Gizai completely", () => {
  function quitTree(asking = false) {
    sets = [];
    // asking, busy, err (the live runs and answers come from the replaced hooks)
    queue.push(asking, false, null);
    try {
      const t = expand(QuitSettings());
      const button = findAll(t, (p) => typeof p.onClick === "function")[0] as { onClick: () => void };
      return { t, button, got: sets };
    } finally { sets = null; queue.length = 0; }
  }
  it("quits at once when no agent is at work, through exit_app (which stops agents first in the backend)", () => {
    const { button } = quitTree();
    button.onClick();
    expect(calls).toEqual([["exitApp", [0]]]);
  });
  it("asks once more first while runs or chat answers are live, then quits", () => {
    liveRuns = [run(1)];
    const { button, got } = quitTree();
    button.onClick();
    expect(calls).toEqual([]);
    expect(got[0]).toEqual([true]);
    const again = quitTree(true);
    expect(text(renderToStaticMarkup(<>{again.t}</>))).toContain("Stop them and quit?");
    again.button.onClick();
    expect(calls).toEqual([["exitApp", [0]]]);
  });
});

describe("the warning under Runs at once", () => {
  const field = (html: string) => html.slice(html.indexOf('<label for="s-max">'), html.indexOf('<label for="s-usd">'));
  it("shows when Runs at once is lower than the active agents' cards at once added up, instead of the hint", () => {
    const f = field(render({ s: { maxConcurrentRuns: 4 } }));
    expect(text(f)).toContain("Your active agents can work on 6 cards at once together (3 for Backend Agent, 1 for QA Agent, 2 for Frontend Agent), but Runs at once is 4");
    expect(f).toContain('<span class="warn">');
    expect(f).not.toContain("All agents together, 1 to 20");
  });
  it("goes away when the numbers fit, and the hint is back", () => {
    for (const n of [6, 10]) {
      const f = field(render({ s: { maxConcurrentRuns: n } }));
      expect(f).not.toContain('<span class="warn">');
      expect(text(f)).toContain("All agents together, 1 to 20; each agent also has its own cards at once");
    }
  });
  it("follows the field as it is edited", () => {
    sets = [];
    queue.push(...states({ s: { maxConcurrentRuns: 6 } }));
    let tree: ReactNode;
    try { tree = expand(SettingsPage()); } finally { queue.length = 0; }
    const got = sets;
    sets = null;
    const input = findAll(tree, (p) => p.id === "s-max")[0] as { onChange: (e: unknown) => void };
    input.onChange({ target: { value: "3" } });
    const edited = got[0][0] as Settings;
    expect(edited.maxConcurrentRuns).toBe(3);
    expect(text(field(render({ s: edited })))).toContain("but Runs at once is 3,");
  });
});
