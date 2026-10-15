// GA-178 QA: what is on screen while a page's code loads the first time. React itself renders App here (react-dom/client,
// so React.lazy, Suspense and useRoute's transition really run) into a small stand-in DOM, as StreamParses.test.tsx does:
// there is no DOM library in the project. Every page but TasksPage is a fake whose code arrives only when the test lets
// it (its vi.mock factory waits), so a test sees the screen in between. A page opened from another page leaves that page
// and the sidebar as they are until the new page draws: no empty page area flashes. At start, a deep link to a page leaves
// the page area empty (no spinner) until its code arrives. Ctrl+K and N work while a page loads. The api, the sidebar, the
// palette, the drawers and the self-test are fakes; the fake self-test notes when it is loaded.
import { act } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

// ---- a stand-in DOM, as much of it as React DOM and App use ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  dataset: Record<string, string> = {};
  nodeValue: string | null = null;
  constructor(public nodeType: number, public nodeName: string, public ownerDocument: FakeNode | null, public namespaceURI = HTML) {}
  get tagName() { return this.nodeName; }
  get firstChild(): FakeNode | null { return this.childNodes[0] ?? null; }
  get lastChild(): FakeNode | null { return this.childNodes[this.childNodes.length - 1] ?? null; }
  get nextSibling(): FakeNode | null { const p = this.parentNode; return p ? p.childNodes[p.childNodes.indexOf(this) + 1] ?? null : null; }
  get textContent(): string { return this.nodeType === 3 ? this.nodeValue ?? "" : this.childNodes.map((c) => c.textContent).join(""); }
  set textContent(t: string) {
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
    if (t) this.appendChild(doc.createTextNode(t));
  }
  appendChild(c: FakeNode) { return this.insertBefore(c, null); }
  insertBefore(c: FakeNode, ref: FakeNode | null) {
    c.parentNode?.removeChild(c);
    const i = ref ? this.childNodes.indexOf(ref) : -1;
    if (i < 0) this.childNodes.push(c); else this.childNodes.splice(i, 0, c);
    c.parentNode = this;
    return c;
  }
  removeChild(c: FakeNode) { this.childNodes.splice(this.childNodes.indexOf(c), 1); c.parentNode = null; return c; }
  setAttribute(k: string, v: unknown) { this.attrs.set(k, String(v)); }
  removeAttribute(k: string) { this.attrs.delete(k); }
  getAttribute(k: string) { return this.attrs.get(k) ?? null; }
  hasAttribute(k: string) { return this.attrs.has(k); }
  addEventListener(type: string, f: (e: unknown) => void, opt?: boolean | { capture?: boolean }) {
    const capture = typeof opt === "boolean" ? opt : !!opt?.capture;
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), { f, capture }]);
  }
  removeEventListener(type: string, f: (e: unknown) => void) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter((l) => l.f !== f)); }
  contains(n: FakeNode | null) { for (let x = n; x; x = x.parentNode) if (x === this) return true; return false; }
  // App's keys: N opens the New task drawer when no field has the focus and no dialog is open
  closest() { return null; }
  querySelector() { return null; }
  focus() {}
  blur() {}
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);

// the window: its hash, and its hashchange and keydown listeners (useRoute's and App's). As in a browser, setting the hash
// fires hashchange a moment later, and only when the hash changes.
const winListeners = new Map<string, Set<(e: unknown) => void>>();
let hash = "";
const win = {
  document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1,
  location: {
    get hash() { return hash; },
    set hash(v: string) {
      const next = v.startsWith("#") ? v : `#${v}`;
      if (next === hash) return;
      hash = next;
      setTimeout(() => emit("hashchange", { type: "hashchange" }), 0);
    },
  },
  addEventListener(type: string, f: (e: unknown) => void) { if (!winListeners.has(type)) winListeners.set(type, new Set()); winListeners.get(type)!.add(f); },
  removeEventListener(type: string, f: (e: unknown) => void) { winListeners.get(type)?.delete(f); },
};
const emit = (type: string, e: unknown) => { for (const f of [...(winListeners.get(type) ?? [])]) f(e); };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// ---- the fakes ----
const h = vi.hoisted(() => {
  const gates = new Map<string, { promise: Promise<void>; open: () => void }>();
  const gate = (name: string) => {
    let g = gates.get(name);
    if (!g) { let open = () => {}; const promise = new Promise<void>((r) => { open = r; }); g = { promise, open }; gates.set(name, g); }
    return g;
  };
  /** Each module App asked for after its start: a page's code, or the self-test. */
  const loaded: string[] = [];
  /** A lazy page's module: noted when App asks for it, there when the test opens its gate. The page shows its name and props. */
  const page = async (name: string) => {
    loaded.push(name);
    await gate(name).promise;
    const { createElement } = await import("react");
    const Page = (p: Record<string, unknown>) =>
      createElement("section", { className: "page" }, [name, p.id, p.view, p.tab, p.archive ? "archive" : ""].filter(Boolean).join(" "));
    return { [name]: Page };
  };
  return { gate, loaded, page, info: {} as Record<string, unknown> };
});

vi.mock("./api", () => {
  const ok = (v: unknown = null) => () => Promise.resolve(v);
  return {
    appInfo: () => Promise.resolve(h.info), exitApp: ok(), selftestReport: ok(), tourStill: ok(), tourSteps: ok(),
    getDoc: ok(), getTask: ok({ descriptionMd: "" }), getTeam: ok({ states: [] }), listRuns: ok([]), saveDoc: ok(), updateTask: ok(),
    listClients: ok([]), listProjects: ok([]), listTasks: ok([]),
  };
});
vi.mock("./selftest", () => { h.loaded.push("selftest"); return {}; });
vi.mock("./lib/appearance", () => ({ appearanceOf: () => ({}), setAppearance: () => {}, toggleDensity: () => {}, toggleTheme: () => {} }));
vi.mock("./lib/terminals", () => ({ startTerminals: () => {} }));
vi.mock("./lib/drawers", async () => {
  const { createElement, Fragment } = await import("react");
  return { DrawerHost: ({ request, children }: { request: { kind: string } | null; children: unknown }) =>
    createElement(Fragment, null, children as never, request ? createElement("aside", { className: "drawer" }, `drawer ${request.kind}`) : null) };
});
// the sidebar shows the page it marks as open
vi.mock("./components/Sidebar", async () => {
  const { createElement } = await import("react");
  return { Sidebar: ({ route }: { route: { page: string } }) => createElement("nav", { className: "sidebar" }, `at ${route.page}`) };
});
vi.mock("./components/Band", () => ({ Band: () => null }));
vi.mock("./components/CommandPalette", async () => {
  const { createElement } = await import("react");
  return { CommandPalette: ({ open }: { open: boolean }) => (open ? createElement("div", { className: "palette" }, "palette") : null) };
});
vi.mock("./components/CloseGuard", () => ({ CloseGuard: () => null }));
// the board and the Inbox: in the start, there at once
vi.mock("./pages/TasksPage", async () => {
  const { createElement } = await import("react");
  return { TasksPage: (p: { inboxFor?: string; initialView?: string }) =>
    createElement("section", { className: "page" }, p.inboxFor !== undefined ? "Inbox" : `TasksPage ${p.initialView ?? ""}`.trim()) };
});
vi.mock("./pages/Placeholder", () => h.page("Placeholder"));
vi.mock("./pages/ClientsPage", () => h.page("ClientsPage"));
vi.mock("./pages/ClientPage", () => h.page("ClientPage"));
vi.mock("./pages/UsersPage", () => h.page("UsersPage"));
vi.mock("./pages/ProjectsPage", () => h.page("ProjectsPage"));
vi.mock("./pages/ProjectPage", () => h.page("ProjectPage"));
vi.mock("./pages/TaskPage", () => h.page("TaskPage"));
vi.mock("./pages/DocPage", () => h.page("DocPage"));
vi.mock("./pages/TeamPage", () => h.page("TeamPage"));
vi.mock("./pages/SettingsPage", () => h.page("SettingsPage"));
vi.mock("./pages/AgentPage", () => h.page("AgentPage"));
vi.mock("./pages/ChatPage", () => h.page("ChatPage"));
vi.mock("./pages/UsagePage", () => h.page("UsagePage"));
vi.mock("./pages/MemoryPage", () => h.page("MemoryPage"));
vi.mock("./pages/EditorPage", () => h.page("EditorPage"));
vi.mock("./pages/TerminalPage", () => h.page("TerminalPage"));

const { createRoot } = await import("react-dom/client");
const { createElement } = await import("react");
const { default: App } = await import("./App");

// ---- helpers ----
const settle = async () => { for (let i = 0; i < 20; i++) await new Promise((r) => setTimeout(r, 0)); };
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
/** Gizai started at `at` (a hash), with appInfo's answer `info`; its start has run. */
async function start(at: string, info: Record<string, unknown> = {}) {
  hash = at;
  h.info = { version: "0.0.0", data_dir: "/data", you_id: "you", selftest: false, start_route: null, selftest_mode: null, ...info };
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(createElement(App)); });
  await act(settle);
}
/** The text a node shows: React hides what a Suspense fallback replaces with display: none, and keeps it in the DOM. */
const visible = (n: FakeNode): string => n.nodeType === 3 ? n.nodeValue ?? "" : n.style.display === "none" ? "" : n.childNodes.map(visible).join("");
/** The page area (App's <main>): the text it shows, "" when it is empty. */
const pageArea = () => container.all().find((n) => n.nodeName === "MAIN")!;
const shown = () => visible(pageArea());
const sidebar = () => visible(container.all().find((n) => n.nodeName === "NAV")!);
const byClass = (c: string) => container.all().filter((n) => n.attrs.get("class") === c);
/** Goes to `to` as a sidebar link, a card or a notification does: it sets the hash. */
async function go(to: string) { await act(async () => { win.location.hash = to; await settle(); }); }
/** The code of page `name` arrives. */
async function arrive(name: string) { await act(async () => { h.gate(name).open(); await settle(); }); }
async function key(k: string, ctrlKey = false) {
  await act(async () => {
    const e = { type: "keydown", key: k, ctrlKey, metaKey: false, altKey: false, target: body, defaultPrevented: false, preventDefault() { e.defaultPrevented = true; } };
    emit("keydown", e);
    await settle();
  });
}

afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
  h.loaded.length = 0;
});

describe("opening a page the first time (GA-178)", () => {
  it("a normal start on the board shows the board at once, and loads no other page and no self-test", async () => {
    await start("#/board");
    expect(shown()).toBe("TasksPage board");
    expect(sidebar()).toBe("at board");
    expect(h.loaded).toEqual([]);
  });

  it("the Inbox, a start's other page, shows at once too", async () => {
    await start("#/inbox");
    expect(shown()).toBe("Inbox");
    expect(h.loaded).toEqual([]);
  });

  it("keeps the board and the sidebar as they are while a page's code loads, then shows the page", async () => {
    await start("#/board");
    await go("#/chat/c1");
    // the chat's code was asked for, and until it is there the board stays: no empty page area
    expect(h.loaded).toEqual(["ChatPage"]);
    expect(shown()).toBe("TasksPage board");
    expect(sidebar()).toBe("at board");

    await arrive("ChatPage");
    expect(shown()).toBe("ChatPage c1");
    expect(sidebar()).toBe("at chat");
  });

  it("opens a page whose code has loaded at once, the next time and with other props", async () => {
    await start("#/board");
    await go("#/chat/c2");
    expect(shown()).toBe("ChatPage c2");
    await go("#/chats");
    expect(shown()).toBe("ChatPage archive");
    await go("#/board");
    expect(shown()).toBe("TasksPage board");
    expect(h.loaded).toEqual([]);
  });

  it("at start, a deep link to a page leaves the page area empty, with no spinner, until its code arrives", async () => {
    await start("#/usage");
    expect(h.loaded).toEqual(["UsagePage"]);
    expect(pageArea().childNodes).toEqual([]);
    expect(sidebar()).toBe("at usage");

    await arrive("UsagePage");
    expect(shown()).toBe("UsagePage");
  });

  it("a start route (GIZAI_ROUTE, scripts' deep links) keeps the Inbox until the page's code is there, and loads no self-test", async () => {
    await start("#/inbox", { start_route: "memory/shared" });
    expect(h.loaded).toEqual(["MemoryPage"]);
    expect(shown()).toBe("Inbox");

    await arrive("MemoryPage");
    expect(shown()).toBe("MemoryPage");
    expect(sidebar()).toBe("at memory");
    expect(h.loaded).not.toContain("selftest");
  });

  it("Ctrl+K and N work while a page's code loads, and the page shows under them when it arrives", async () => {
    await start("#/board");
    await go("#/settings/appearance");
    expect(shown()).toBe("TasksPage board");

    await key("k", true);
    expect(byClass("palette")).toHaveLength(1);
    await key("k", true);
    expect(byClass("palette")).toHaveLength(0);
    await key("n");
    expect(byClass("drawer").map((n) => n.textContent)).toEqual(["drawer task"]);
    expect(shown()).toBe("TasksPage board");

    await arrive("SettingsPage");
    expect(shown()).toBe("SettingsPage appearance");
    expect(byClass("drawer")).toHaveLength(1);
    await key("k", true);
    expect(byClass("palette")).toHaveLength(1);
  });

  // from the board to each other page in turn, as a person clicks through the sidebar and a card: each page's first open
  // leaves the page before it on screen, a page already loaded (the task's Changes) opens at once
  const WALK: [string, string, string][] = [
    ["#/team", "TeamPage", "TeamPage"],
    ["#/clients", "ClientsPage", "ClientsPage"],
    ["#/client/k1", "ClientPage", "ClientPage k1"],
    ["#/projects", "ProjectsPage", "ProjectsPage"],
    ["#/project/p1", "ProjectPage", "ProjectPage p1"],
    ["#/doc/d1", "DocPage", "DocPage d1"],
    ["#/task/t1", "TaskPage", "TaskPage t1 card"],
    ["#/task/t1/changes", "", "TaskPage t1 changes"],
    ["#/users", "UsersPage", "UsersPage"],
    ["#/agent/a1", "AgentPage", "AgentPage a1"],
    ["#/editor", "EditorPage", "EditorPage"],
    ["#/editor/home/me/site", "", "EditorPage home/me/site"],
    ["#/terminal", "TerminalPage", "TerminalPage"],
    ["#/board", "", "TasksPage board"],
  ];
  it("each other page's first open leaves the page before it on screen until its code arrives", async () => {
    await start("#/board");
    let before = "TasksPage board";
    for (const [to, page, text] of WALK) {
      await go(to);
      if (page) {
        expect(h.loaded.at(-1), to).toBe(page);
        expect(shown(), `${to} while its code loads`).toBe(before);
        await arrive(page);
      }
      expect(shown(), to).toBe(text);
      before = text;
    }
    expect(h.loaded).toEqual(WALK.map(([, page]) => page).filter(Boolean));
  });
});
