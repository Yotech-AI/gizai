// GA-164 QA: App and the first start's onboarding. While it waits (main.tsx asks onboarding_pending before drawing), App
// shows the onboarding instead of the app: no sidebar, no page, and the app's keys off. Finishing opens the chat it started
// (#/chat/<id>) or the board with a note saying why there is none; Skip setup opens the page the app would open anyway.
// The board never shows first. Without it, the app opens as today and never loads the onboarding's code. React itself
// renders App here (react-dom/client, so React.lazy, Suspense and useRoute's transition really run) into the small
// stand-in DOM of AppPageSwitch.test.tsx; the onboarding is a fake that hands the test its onDone.
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
  setAttributeNS(_ns: string, k: string, v: unknown) { this.attrs.set(k, String(v)); }
  removeAttribute(k: string) { this.attrs.delete(k); }
  removeAttributeNS(_ns: string, k: string) { this.attrs.delete(k); }
  getAttribute(k: string) { return this.attrs.get(k) ?? null; }
  hasAttribute(k: string) { return this.attrs.has(k); }
  addEventListener(type: string, f: (e: unknown) => void, opt?: boolean | { capture?: boolean }) {
    const capture = typeof opt === "boolean" ? opt : !!opt?.capture;
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), { f, capture }]);
  }
  removeEventListener(type: string, f: (e: unknown) => void) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter((l) => l.f !== f)); }
  contains(n: FakeNode | null) { for (let x = n; x; x = x.parentNode) if (x === this) return true; return false; }
  /** `.class` and `[attr=value]`: what App's keys ask (the onboarding, a dialog). */
  matches(sel: string) {
    const attr = /^\[([\w-]+)=["']?([^"'\]]*)["']?\]$/.exec(sel);
    if (attr) return this.getAttribute(attr[1]) === attr[2];
    return sel.startsWith(".") && (this.getAttribute("class") ?? "").split(" ").includes(sel.slice(1));
  }
  closest() { return null; }
  querySelector(sel: string) { return this.all().slice(1).find((n) => n.nodeType === 1 && n.matches(sel)) ?? null; }
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

/** An event at `target`, through the listeners on the way as a browser calls them (capture down, then bubble up). */
function fire(target: FakeNode, type: string) {
  const path: FakeNode[] = [];
  for (let x: FakeNode | null = target; x; x = x.parentNode) path.push(x);
  let stopped = false;
  const e = { type, target, bubbles: true, button: 0, defaultPrevented: false, timeStamp: 0,
    preventDefault() { e.defaultPrevented = true; }, stopPropagation() { stopped = true; } };
  for (const n of [...path].reverse()) for (const l of n.listeners.get(type) ?? []) if (l.capture && !stopped) l.f(e);
  for (const n of path) for (const l of n.listeners.get(type) ?? []) if (!l.capture && !stopped) l.f(e);
}

// ---- the fakes ----
const h = vi.hoisted(() => ({
  /** Each module App asked for after its start. */
  loaded: [] as string[],
  /** Each page drawn, in order. */
  drawn: [] as string[],
  info: {} as Record<string, unknown>,
  onDone: null as null | ((to: unknown, note?: string) => void),
}));
const page = async (name: string) => {
  h.loaded.push(name);
  const { createElement } = await import("react");
  const Page = (p: Record<string, unknown>) => { h.drawn.push(`${name} ${p.id ?? ""}`.trim()); return createElement("section", { className: "page" }, `${name} ${p.id ?? ""}`.trim()); };
  return { [name]: Page };
};
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
vi.mock("./pages/TasksPage", async () => {
  const { createElement } = await import("react");
  return { TasksPage: (p: { inboxFor?: string; initialView?: string }) => {
    const name = p.inboxFor !== undefined ? "Inbox" : `TasksPage ${p.initialView ?? ""}`.trim();
    h.drawn.push(name);
    return createElement("section", { className: "page" }, name);
  } };
});
// the onboarding: a fake that hands the test its onDone
vi.mock("./pages/OnboardingPage", async () => {
  h.loaded.push("OnboardingPage");
  const { createElement } = await import("react");
  return { OnboardingPage: ({ onDone }: { onDone: (to: unknown, note?: string) => void }) => {
    h.onDone = onDone;
    return createElement("div", { className: "onb" }, "onboarding");
  } };
});
vi.mock("./pages/Placeholder", () => page("Placeholder"));
vi.mock("./pages/ClientsPage", () => page("ClientsPage"));
vi.mock("./pages/ClientPage", () => page("ClientPage"));
vi.mock("./pages/UsersPage", () => page("UsersPage"));
vi.mock("./pages/ProjectsPage", () => page("ProjectsPage"));
vi.mock("./pages/ProjectPage", () => page("ProjectPage"));
vi.mock("./pages/TaskPage", () => page("TaskPage"));
vi.mock("./pages/DocPage", () => page("DocPage"));
vi.mock("./pages/TeamPage", () => page("TeamPage"));
vi.mock("./pages/SettingsPage", () => page("SettingsPage"));
vi.mock("./pages/AgentPage", () => page("AgentPage"));
vi.mock("./pages/ChatPage", () => page("ChatPage"));
vi.mock("./pages/UsagePage", () => page("UsagePage"));
vi.mock("./pages/MemoryPage", () => page("MemoryPage"));
vi.mock("./pages/EditorPage", () => page("EditorPage"));
vi.mock("./pages/TerminalPage", () => page("TerminalPage"));

const { createRoot } = await import("react-dom/client");
const { createElement } = await import("react");
const { default: App } = await import("./App");

// ---- helpers ----
// One turn of the event loop for React, the hash and the module loader; the helpers wait for what they wait for (at most
// 1.5 s), since a timer tick is about 16 ms on Windows.
const tick = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
async function until(done: () => boolean) {
  const end = Date.now() + 1500;
  await tick();
  while (!done() && Date.now() < end) await tick();
}
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
/** Gizai started at `at` (a hash); `props` are what main.tsx passes. */
async function start(at: string, props: Record<string, unknown>, ready: () => boolean) {
  hash = at;
  h.info = { version: "0.0.0", data_dir: "/data", you_id: "you", selftest: false, start_route: null, selftest_mode: null };
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(createElement(App, props)); });
  await until(ready);
}
const squash = (s: string) => s.replace(/\s+/g, " ").trim();
const tag = (name: string) => container.all().find((n) => n.nodeName === name) ?? null;
const byClass = (c: string) => container.all().filter((n) => n.nodeType === 1 && (n.attrs.get("class") ?? "").split(" ").includes(c));
const onboarding = () => byClass("onb").length === 1;
const shown = () => { const m = tag("MAIN"); return m ? squash(m.textContent) : null; };
async function key(k: string, ctrlKey: boolean) {
  await act(async () => {
    const e = { type: "keydown", key: k, ctrlKey, metaKey: false, altKey: false, target: body, defaultPrevented: false, preventDefault() { e.defaultPrevented = true; } };
    emit("keydown", e);
  });
  await tick();
}
/** The onboarding ends as OnboardingPage ends it. */
const leave = (to: unknown, note?: string) => act(async () => { h.onDone!(to, note); });

afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
  h.loaded.length = 0;
  h.drawn.length = 0;
  h.onDone = null;
});

describe("a new install's first start (the onboarding waits)", () => {
  it("shows the onboarding instead of the app: no sidebar, no page, and the board not drawn", async () => {
    await start("", { onboarding: true }, onboarding);
    expect(squash(container.textContent)).toBe("onboarding");
    expect(tag("NAV")).toBeNull();
    expect(tag("MAIN")).toBeNull();
    expect(h.drawn).toEqual([]);
    expect(h.loaded).toEqual(["OnboardingPage"]);
  });

  it("leaves the app's keys off while it shows (Ctrl+K, N), and on again after it", async () => {
    await start("#/board", { onboarding: true }, onboarding);
    await key("k", true);
    await key("n", false);
    expect(byClass("palette")).toHaveLength(0);
    expect(byClass("drawer")).toHaveLength(0);
    await leave(null);
    await until(() => shown() === "TasksPage board");
    await key("k", true);
    expect(byClass("palette")).toHaveLength(1);
  });

  it("finishing with a chat opens #/chat/<id>, with the sidebar, and the board is never drawn first", async () => {
    await start("", { onboarding: true }, onboarding);
    await leave({ page: "chat", id: "thread-9" });
    await until(() => shown() === "ChatPage thread-9");
    expect(hash).toBe("#/chat/thread-9");
    expect(shown()).toBe("ChatPage thread-9");
    expect(squash(tag("NAV")!.textContent)).toBe("at chat");
    expect(onboarding()).toBe(false);
    expect(byClass("warn-banner")).toHaveLength(0);
    expect(h.drawn.filter((p) => p.startsWith("TasksPage") || p === "Inbox")).toEqual([]);
  });

  it("finishing without a chat opens the board with a note saying why, which Close takes away", async () => {
    const why = "There's no Team Lead chat yet: it needs Claude Code, which isn't installed. Once it is, start one on the Chat page.";
    await start("#/inbox", { onboarding: true }, onboarding);
    await leave({ page: "board" }, why);
    await until(() => byClass("warn-banner").length === 1 && h.drawn.length > 0);
    expect(hash).toBe("#/board");
    expect(h.drawn).toEqual(["TasksPage board"]); // not the Inbox it started on
    const note = byClass("warn-banner")[0];
    expect(note.attrs.get("role")).toBe("status");
    expect(squash(note.textContent)).toBe(why);
    const close = note.all().find((n) => n.nodeName === "BUTTON" && n.attrs.get("aria-label") === "Close")!;
    await act(async () => { fire(close, "click"); });
    await tick();
    expect(byClass("warn-banner")).toHaveLength(0);
  });

  it("on the board already, finishing without a chat opens it at once with the note", async () => {
    await start("#/board", { onboarding: true }, onboarding);
    await leave({ page: "board" }, "The Team Lead's chat couldn't start. You can start one on the Chat page.");
    await until(() => h.drawn.length > 0);
    expect(byClass("warn-banner").map((n) => squash(n.textContent))).toEqual(["The Team Lead's chat couldn't start. You can start one on the Chat page."]);
    expect(h.drawn).toEqual(["TasksPage board"]);
    expect(hash).toBe("#/board");
  });

  it("Skip setup opens the page the app would open anyway, with no note", async () => {
    await start("#/board", { onboarding: true }, onboarding);
    await leave(null);
    await until(() => shown() === "TasksPage board");
    expect(hash).toBe("#/board");
    expect(byClass("warn-banner")).toHaveLength(0);
    expect(squash(tag("NAV")!.textContent)).toBe("at board");
  });
});

describe("any other start (the onboarding doesn't wait)", () => {
  for (const props of [{ onboarding: false }, {}]) {
    it(`opens the app as today and never loads the onboarding's code (${JSON.stringify(props)})`, async () => {
      await start("#/board", props, () => shown() === "TasksPage board");
      expect(shown()).toBe("TasksPage board");
      expect(onboarding()).toBe(false);
      expect(h.loaded).toEqual([]);
    });
  }
});
