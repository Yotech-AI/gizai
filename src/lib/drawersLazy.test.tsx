// GA-190 QA: every drawer opens as before, now that its code loads when it is first opened. DrawerHost renders each drawer
// as a React.lazy import in a Suspense (fallback null). Each of the six drawer modules is replaced by a small component that
// records the props it gets; its module records when it is evaluated (first imported) and waits for the test to let it
// finish loading, so the moment between asking for a drawer and its code arriving can be seen. React itself renders
// (react-dom/client) into a small stand-in DOM: there is no DOM library in the project.
import { act, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { DrawerReq } from "./drawers";

// ---- a stand-in DOM, as much of it as React DOM uses (as in components/StreamParses.test.tsx) ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
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
  focus() {}
  blur() {}
  getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, addEventListener() {}, removeEventListener() {} };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// ---- the six drawer modules ----
type Kind = "task" | "project" | "client" | "person" | "agent" | "doc";
const h = vi.hoisted(() => {
  const kinds = ["task", "project", "client", "person", "agent", "doc"] as const;
  type K = (typeof kinds)[number];
  const gates = {} as Record<K, { promise: Promise<void>; open: () => void }>;
  for (const k of kinds) {
    let open = () => {};
    const promise = new Promise<void>((r) => { open = r; });
    gates[k] = { promise, open };
  }
  const h = {
    /** The drawer modules evaluated so far, in order: each module's code ran when it was first imported. */
    loaded: [] as K[],
    /** The props each drawer was rendered with, render by render. */
    props: Object.fromEntries(kinds.map((k) => [k, [] as Record<string, unknown>[]])) as Record<K, Record<string, unknown>[]>,
    /** A module finishes loading only once its gate is open. */
    gates,
    /** A drawer's module: records that it is evaluated, waits for its gate, and exports a component that records its props. */
    module: async (kind: K, name: string) => {
      h.loaded.push(kind);
      await gates[kind].promise;
      const { createElement } = await import("react");
      const Drawer = (p: Record<string, unknown>) => { h.props[kind].push(p); return createElement("div", { "data-drawer": kind }, `${kind} drawer`); };
      return { [name]: Drawer };
    },
  };
  return h;
});

vi.mock("../components/NewTaskDrawer", () => h.module("task", "NewTaskDrawer"));
vi.mock("../components/ProjectForm", () => h.module("project", "ProjectDrawer"));
vi.mock("../components/ClientForm", () => h.module("client", "ClientDrawer"));
vi.mock("../components/PersonDrawer", () => h.module("person", "PersonDrawer"));
vi.mock("../components/AgentForm", () => h.module("agent", "AgentDrawer"));
vi.mock("../components/DocDrawer", () => h.module("doc", "DocDrawer"));

const { createRoot } = await import("react-dom/client");
const { DrawerHost, useDrawer } = await import("./drawers");

// ---- drawing the host ----
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
/** The setter the page below the host gets from useDrawer(). */
let ask: (r: DrawerReq) => void = () => {};
function Page() {
  ask = useDrawer();
  return <p data-page="tasks">The page</p>;
}
const host = (props: { request?: DrawerReq | null; setRequest?: (r: DrawerReq | null) => void } = {}) => <DrawerHost {...props}><Page /></DrawerHost>;
/** Lets the module loads waiting on promises and timers go on, inside act, a few turns. */
const settle = async () => { for (let i = 0; i < 10; i++) await new Promise((r) => setTimeout(r, 0)); };
async function mount(el: ReactNode) {
  if (root) await unmount();
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(el); await settle(); });
}
async function unmount() {
  await act(async () => { root!.unmount(); });
  root = null;
  container.parentNode?.removeChild(container);
}
/** The drawers on screen, by kind. */
const shown = () => container.all().filter((n) => n.nodeType === 1 && n.hasAttribute("data-drawer")).map((n) => n.getAttribute("data-drawer"));
const pageShown = () => container.all().filter((n) => n.getAttribute("data-page") === "tasks").length === 1 && container.textContent.startsWith("The page");
const lastProps = (k: Kind) => h.props[k].at(-1)!;

/** What origin/main's DrawerHost (before GA-190) passed each drawer for a request, written out from its JSX. */
function mainProps(r: DrawerReq): Record<string, unknown> {
  const onClose = "a function";
  switch (r.kind) {
    case "task": return { onClose, stateId: r.stateId, projectId: r.projectId, assigneeId: r.assigneeId };
    case "project": return { id: r.id, onClose };
    case "client": return { id: r.id, onClose };
    case "person": return { onClose };
    case "agent": return { teamId: r.teamId, agentId: r.id, preset: r.preset, onClose };
    case "doc": return { projectId: r.projectId, onClose };
  }
}
/** The drawer got exactly main's props: the same keys (an absent field still a key, as JSX passes it) and the same values. */
function expectMainProps(got: Record<string, unknown>, r: DrawerReq) {
  const want = mainProps(r);
  expect(Object.keys(got).sort()).toEqual(Object.keys(want).sort());
  for (const [k, v] of Object.entries(want)) {
    if (k === "onClose") expect(typeof got.onClose).toBe("function");
    else expect(got[k], k).toBe(v);
  }
}

const preset = { name: "Reviewer", role: "qa", chat: true };
// Each kind: the request it is first opened with, and another one for its second opening (absent fields, other values).
const CASES: { kind: Kind; first: DrawerReq; again: DrawerReq; firstControlled: boolean }[] = [
  { kind: "task", first: { kind: "task", stateId: "st-review", projectId: "p1", assigneeId: "u1" }, again: { kind: "task", stateId: null }, firstControlled: false },
  { kind: "project", first: { kind: "project", id: "p1" }, again: { kind: "project" }, firstControlled: true },
  { kind: "client", first: { kind: "client", id: "c1" }, again: { kind: "client" }, firstControlled: false },
  { kind: "person", first: { kind: "person" }, again: { kind: "person" }, firstControlled: true },
  { kind: "agent", first: { kind: "agent", teamId: "team1", id: "a1", preset }, again: { kind: "agent", teamId: null, preset: { chat: true } }, firstControlled: false },
  { kind: "doc", first: { kind: "doc", projectId: "p1" }, again: { kind: "doc", projectId: "p2" }, firstControlled: true },
];

describe("the drawers load when first opened, and open as before (GA-190)", () => {
  it("evaluates none of the six drawer modules on importing lib/drawers and drawing DrawerHost with no drawer asked", async () => {
    expect(h.loaded).toEqual([]);
    await mount(host());
    expect(pageShown()).toBe(true);
    expect(shown()).toEqual([]);
    expect(h.loaded).toEqual([]);
    // controlled, with no request: still none
    await mount(host({ request: null, setRequest: () => {} }));
    expect(shown()).toEqual([]);
    expect(h.loaded).toEqual([]);
    await unmount();
  });

  it.each(CASES.map((c, i) => [c.kind, c.firstControlled ? "controlled" : "uncontrolled", c, i] as const))(
    "'%s' (first opened %s): nothing until its code arrives, the page staying; then the drawer with main's props; only its module loads; onClose closes it; it opens at once the next time",
    async (kind, _mode, c, i) => {
      const before = CASES.slice(0, i).map((x) => x.kind);
      expect(h.loaded).toEqual(before);
      const setRequest = vi.fn();

      // ask for it, the first time
      if (c.firstControlled) {
        await mount(host({ request: null, setRequest }));
        // in controlled mode useDrawer gives the page the host's setRequest itself
        expect(ask).toBe(setRequest);
        await act(async () => { root!.render(host({ request: c.first, setRequest })); await settle(); });
      } else {
        await mount(host());
        await act(async () => { ask(c.first); await settle(); });
      }
      // its module is being imported (and only it); until it arrives: the fallback, null, and the page as it was
      expect(h.loaded).toEqual([...before, kind]);
      expect(shown()).toEqual([]);
      expect(h.props[kind]).toHaveLength(0);
      expect(pageShown()).toBe(true);
      expect(container.textContent).toBe("The page");

      // its code arrives: the drawer, with the props main's DrawerHost passed
      await act(async () => { h.gates[kind].open(); await settle(); });
      expect(shown()).toEqual([kind]);
      expect(pageShown()).toBe(true);
      expectMainProps(lastProps(kind), c.first);
      if (c.kind === "agent") expect(lastProps(kind).preset).toBe(preset);
      expect(h.loaded).toEqual([...before, kind]);

      // onClose closes it
      if (c.firstControlled) {
        await act(async () => { (lastProps(kind).onClose as () => void)(); });
        expect(setRequest.mock.calls).toEqual([[null]]);
        await act(async () => { root!.render(host({ request: null, setRequest })); });
      } else {
        await act(async () => { (lastProps(kind).onClose as () => void)(); });
      }
      expect(shown()).toEqual([]);
      expect(pageShown()).toBe(true);

      // the same kind again, in the same host: at once (a synchronous act, no waiting), its module not evaluated again
      const n = h.props[kind].length;
      if (c.firstControlled) act(() => { root!.render(host({ request: c.again, setRequest })); });
      else act(() => { ask(c.again); });
      expect(shown()).toEqual([kind]);
      expect(h.props[kind].length).toBeGreaterThan(n);
      expectMainProps(lastProps(kind), c.again);

      // and in the other mode, in a new host: at once too, and its onClose closes it there
      const set2 = vi.fn();
      if (c.firstControlled) {
        await mount(host());
        act(() => { ask(c.first); });
        expect(shown()).toEqual([kind]);
        expectMainProps(lastProps(kind), c.first);
        await act(async () => { (lastProps(kind).onClose as () => void)(); });
      } else {
        await mount(host({ request: null, setRequest: set2 }));
        act(() => { root!.render(host({ request: c.first, setRequest: set2 })); });
        expect(shown()).toEqual([kind]);
        expectMainProps(lastProps(kind), c.first);
        await act(async () => { (lastProps(kind).onClose as () => void)(); });
        expect(set2.mock.calls).toEqual([[null]]);
        await act(async () => { root!.render(host({ request: null, setRequest: set2 })); });
      }
      expect(shown()).toEqual([]);
      expect(pageShown()).toBe(true);
      // each module evaluated once, the drawers not opened yet still not
      expect(h.loaded).toEqual([...before, kind]);
      await unmount();
    },
  );

  it("with every module loaded, each kind opens at once in turn, one drawer at a time, and switching kinds swaps the drawer", async () => {
    expect(h.loaded).toEqual(["task", "project", "client", "person", "agent", "doc"]);
    await mount(host());
    for (const c of CASES) {
      act(() => { ask(c.first); });
      expect(shown()).toEqual([c.kind]);
      expectMainProps(lastProps(c.kind), c.first);
      expect(pageShown()).toBe(true);
    }
    await act(async () => { (lastProps("doc").onClose as () => void)(); });
    expect(shown()).toEqual([]);
    // still each module evaluated once
    expect(h.loaded).toEqual(["task", "project", "client", "person", "agent", "doc"]);
    await unmount();
  });
});
