// GA-228 QA: the card page's Builds on and Needed by (components/BuildsOn.tsx, in the card's properties). Builds on lists
// each card as a link to its card page with its ID and column, a check mark only on a merged one, and × to take it off;
// Add a card opens a finder with the @ picker's card list (not the card itself, not a card it builds on already), and a
// pick (Enter or a click) adds it. A refusal (a loop) shows its message word for word. Needed by is read-only, None when
// empty. An archived card (readOnly) shows both rows without Add or ×.
// The rows render to HTML on the server; the clicks and keys go through React itself (react-dom/client) in a small
// stand-in DOM, as in pages/TasksSearchKeep.test.tsx (there is no DOM library in the project). The data hook answers at
// once with the mocked data.
import { act, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BuildsOn, BuildsOnCard, Task } from "../types";
import type { PickItem } from "../lib/itemLinks";

// ---- a stand-in DOM, as much of it as React DOM uses ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  nodeValue: string | null = null;
  declare value?: string;
  declare type?: string;
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
  closest() { return null; }
  querySelector() { return null; }
  scrollIntoView() {}
  focus() {}
  blur() {}
  getBoundingClientRect() { return { top: 100, left: 40, right: 300, bottom: 124, width: 260, height: 24 }; }
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
  getElementById(id: string) { return this.all().find((n) => n.getAttribute("id") === id) ?? null; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
(doc as unknown as { oninput: null }).oninput = null;
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, innerHeight: 900, innerWidth: 1400,
  addEventListener() {}, removeEventListener() {}, matchMedia: () => ({ matches: false, addEventListener() {}, removeEventListener() {} }) };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win, hidden: false });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** An event at `target`, through the listeners on the way as a browser calls them (capture down, then bubble up). */
function fire(target: FakeNode, type: string, extra: Record<string, unknown> = {}) {
  const path: FakeNode[] = [];
  for (let x: FakeNode | null = target; x; x = x.parentNode) path.push(x);
  let stopped = false;
  const e = { type, target, bubbles: true, button: 0, defaultPrevented: false, timeStamp: 0, ...extra,
    preventDefault() { e.defaultPrevented = true; }, stopPropagation() { stopped = true; } };
  for (const n of [...path].reverse()) for (const l of n.listeners.get(type) ?? []) if (l.capture && !stopped) l.f(e);
  for (const n of path) for (const l of n.listeners.get(type) ?? []) if (!l.capture && !stopped) l.f(e);
}
const cls = (n: FakeNode) => (n.getAttribute("class") ?? "").split(" ");
const byLabel = (root: FakeNode, label: string) => root.all().filter((n) => n.getAttribute("aria-label") === label);
const buttonsNamed = (root: FakeNode, text: string) => root.all().filter((n) => n.nodeName === "BUTTON" && n.textContent.trim() === text);

// ---- what the rows read and write ----
const h = vi.hoisted(() => ({
  data: null as unknown,
  items: [] as unknown[],
  calls: [] as [string, unknown[]][],
  /** What addBuildsOn answers: undefined resolves, a string rejects with it (as Tauri's invoke does). */
  refuse: undefined as string | undefined,
}));
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  return {
    ...real,
    getBuildsOn: (...a: unknown[]) => { h.calls.push(["getBuildsOn", a]); return { tag: "buildsOn" }; },
    addBuildsOn: (...a: unknown[]) => { h.calls.push(["addBuildsOn", a]); return h.refuse ? Promise.reject(h.refuse) : Promise.resolve(); },
    removeBuildsOn: (...a: unknown[]) => { h.calls.push(["removeBuildsOn", a]); return Promise.resolve(); },
  };
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    const r = fetch() as { tag?: string } | null;
    return { data: r && r.tag === "buildsOn" ? h.data : null, error: null, reload: () => {}, setData: () => {} };
  },
}));
vi.mock("../lib/pickItems", () => ({ loadPickItems: () => { h.calls.push(["loadPickItems", []]); return Promise.resolve(h.items); } }));

const { createRoot } = await import("react-dom/client");
const { BuildsOnRows } = await import("./BuildsOn");

// ---- the data ----
const linked = (n: number, over: Partial<BuildsOnCard>): BuildsOnCard => ({ id: `t${n}`, identifier: `GA-${n}`, title: `Card ${n}`, projectId: "p1",
  column: "In progress", category: "in_progress", merged: false, hold: null, archived: false, ...over });
const ME = { id: "t5", identifier: "GA-5", title: "Team templates (screens)", stateCategory: "ready", stateName: "To do" } as Task;
const DATA: BuildsOn = {
  buildsOn: [
    linked(1, { title: "Team templates (backend)", column: "Deploy", category: "deploy", merged: true }),
    linked(2, { title: "Team templates (base)", column: "In progress", category: "in_progress" }),
    linked(3, { title: "Old base", column: "Done", category: "done", merged: true, archived: true }),
  ],
  neededBy: [linked(7, { title: "Team templates (docs)", column: "To do", category: "ready" })],
};
const pickTask = (key: string, title: string, column: string): PickItem =>
  ({ kind: "task", key, label: `${key} - ${title}`, text: `${key} - ${title}`, hint: "Giz AI", column: { name: column } });
const ITEMS: PickItem[] = [
  pickTask("GA-1", "Team templates (backend)", "Deploy"), pickTask("GA-2", "Team templates (base)", "In progress"),
  pickTask("GA-3", "Old base", "Done"), pickTask("GA-4", "Export invoices", "To do"), pickTask("GA-5", "Team templates (screens)", "To do"),
  pickTask("KADE-6", "Import contacts", "Review"),
  { kind: "project", key: "GA", label: "GA - Giz AI", text: "Giz AI" },
];

const text = (s: string) => s.replace(/&#x27;/g, "'").replace(/&amp;/g, "&");
/** The prop row whose label is `k`. */
const rowHtml = (html: string, k: string) => {
  const at = html.indexOf(`<span class="k">${k}</span>`);
  const next = html.indexOf('<div class="prop-row', at + 1);
  return html.slice(at, next < 0 ? undefined : next);
};
/** A linked card's link in a row: its href, tooltip and inner HTML. */
const linkOf = (row: string, id: string) => {
  const m = new RegExp(`<span class="builds-on-card" data-card="${id}"><a href="([^"]*)" title="([^"]*)">([\\s\\S]*?)</a>([\\s\\S]*?)</span>(?=<span class="builds-on-card"|<button|<span class="none"|</div>)`).exec(row);
  return m ? { href: m[1], title: text(m[2]), inner: m[3], after: m[4] } : null;
};

describe("Builds on and Needed by on the card page (GA-228)", () => {
  beforeEach(() => { h.data = DATA; h.calls.length = 0; h.refuse = undefined; h.items = ITEMS; });

  it("shows each card it builds on as a link to its card page, with its ID and column, and a check mark only once merged", () => {
    const html = renderToStaticMarkup(<BuildsOnRows task={ME} />);
    expect(h.calls).toContainEqual(["getBuildsOn", ["t5"]]);
    const row = rowHtml(html, "Builds on");
    const one = linkOf(row, "GA-1")!;
    expect(one.href).toBe("#/task/t1");
    expect(one.inner).toContain('<span class="id">GA-1</span><span class="col">Deploy</span>');
    expect(one.inner).toMatch(/aria-label="Merged"/);
    expect(one.title).toBe("GA-1 Team templates (backend): merged");
    const two = linkOf(row, "GA-2")!;
    expect(two.href).toBe("#/task/t2");
    expect(two.inner).toContain('<span class="id">GA-2</span><span class="col">In progress</span>');
    expect(two.inner).not.toMatch(/Merged/);
    expect(two.title).toBe("GA-2 Team templates (base): in In progress, not merged yet");
    const three = linkOf(row, "GA-3")!;
    expect(three.inner).toContain('<span class="col">Done</span>');
    expect(three.inner).toMatch(/aria-label="Merged"/);
    // in the order the backend gives (by number), each with its ×, and Add a card after them
    expect([...row.matchAll(/data-card="([^"]+)"/g)].map((m) => m[1])).toEqual(["GA-1", "GA-2", "GA-3"]);
    for (const id of ["GA-1", "GA-2", "GA-3"]) expect(row).toContain(`aria-label="Take ${id} off Builds on"`);
    expect(row).toContain("Add a card");
    expect(row.indexOf("Add a card")).toBeGreaterThan(row.indexOf('data-card="GA-3"'));
  });

  it("lists Needed by under it, read-only", () => {
    const html = renderToStaticMarkup(<BuildsOnRows task={ME} />);
    expect(html.indexOf('<span class="k">Needed by</span>')).toBeGreaterThan(html.indexOf('<span class="k">Builds on</span>'));
    const row = rowHtml(html, "Needed by");
    const seven = linkOf(row, "GA-7")!;
    expect(seven.href).toBe("#/task/t7");
    expect(seven.inner).toContain('<span class="id">GA-7</span><span class="col">To do</span>');
    expect(row).not.toContain("<button");
    expect(row).not.toContain("Add a card");
    expect(row).not.toContain("None");
  });

  it("shows None for an empty Needed by, and only Add a card for an empty Builds on", () => {
    h.data = { buildsOn: [], neededBy: [] };
    const html = renderToStaticMarkup(<BuildsOnRows task={ME} />);
    expect(rowHtml(html, "Needed by")).toContain('<span class="none">None</span>');
    const on = rowHtml(html, "Builds on");
    expect(on).toContain("Add a card");
    expect(on).not.toContain("data-card=");
  });

  it("on an archived card (read-only) shows the cards without Add or ×, and None when there are none", () => {
    const html = renderToStaticMarkup(<BuildsOnRows task={ME} readOnly />);
    const on = rowHtml(html, "Builds on");
    expect(linkOf(on, "GA-1")!.href).toBe("#/task/t1");
    expect(on).not.toContain("<button");
    expect(on).not.toContain("Add a card");
    h.data = { buildsOn: [], neededBy: [] };
    const empty = renderToStaticMarkup(<BuildsOnRows task={ME} readOnly />);
    expect(rowHtml(empty, "Builds on")).toContain('<span class="none">None</span>');
  });
});

// ---- adding and removing, through React ----
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
function mount(el: ReactNode) {
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  act(() => { root!.render(el); });
}
async function unmount() {
  await act(async () => { root!.unmount(); });
  root = null;
  container.parentNode?.removeChild(container);
}
/** Lets the mocked promises settle and React render what they set. */
const settle = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
const finder = () => byLabel(body, "Find a card it builds on")[0];
const picker = () => byLabel(body, "Link an item").find((n) => n.getAttribute("role") === "listbox");
/** The options the picker shows, by the card's ID. */
const offered = () => (picker()?.all() ?? []).filter((n) => n.getAttribute("role") === "option").map((n) => n.textContent.split(" - ")[0]);
async function openFinder() {
  act(() => { fire(buttonsNamed(container, "Add a card")[0]!, "click"); });
  await settle();
}
function type(value: string) {
  const box = finder()!;
  act(() => { box.type = "text"; box.value = value; fire(box, "input"); });
}
const press = (key: string) => act(() => { fire(finder()!, "keydown", { key }); });
const alertText = () => container.all().filter((n) => n.getAttribute("role") === "alert").map((n) => n.textContent);
const addCalls = () => h.calls.filter(([k]) => k === "addBuildsOn").map(([, a]) => a);
const removeCalls = () => h.calls.filter(([k]) => k === "removeBuildsOn").map(([, a]) => a);

describe("adding and removing a card it builds on (GA-228)", () => {
  beforeEach(() => { h.data = DATA; h.calls.length = 0; h.refuse = undefined; h.items = ITEMS; });
  afterEach(async () => { if (root) await unmount(); });

  it("Add a card opens a finder with the @ picker's cards, not the card itself nor the ones it builds on already", async () => {
    mount(<BuildsOnRows task={ME} />);
    expect(finder()).toBeUndefined();
    await openFinder();
    expect(h.calls.some(([k]) => k === "loadPickItems")).toBe(true);
    expect(finder()).toBeDefined();
    expect(finder()!.getAttribute("placeholder")).toBe("ID or title");
    expect(picker()).toBeDefined();
    expect(offered().sort()).toEqual(["GA-4", "KADE-6"]);
    // Escape closes it, adding nothing
    press("Escape");
    expect(finder()).toBeUndefined();
    expect(picker()).toBeUndefined();
    expect(addCalls()).toEqual([]);
  });

  it("typing filters the cards, and Enter adds the first one by its ID", async () => {
    mount(<BuildsOnRows task={ME} />);
    await openFinder();
    type("import");
    expect(offered()).toEqual(["KADE-6"]);
    press("Enter");
    await settle();
    expect(addCalls()).toEqual([["t5", "KADE-6"]]);
    expect(finder()).toBeUndefined();
    expect(alertText()).toEqual([]);
  });

  it("a click on a card adds it", async () => {
    mount(<BuildsOnRows task={ME} />);
    await openFinder();
    type("GA-4");
    const option = picker()!.all().find((n) => n.getAttribute("role") === "option" && n.textContent.startsWith("GA-4"))!;
    act(() => { fire(option, "click"); });
    await settle();
    expect(addCalls()).toEqual([["t5", "GA-4"]]);
  });

  it("× takes a card off by its id", async () => {
    mount(<BuildsOnRows task={ME} />);
    act(() => { fire(byLabel(container, "Take GA-2 off Builds on")[0]!, "click"); });
    await settle();
    expect(removeCalls()).toEqual([["t5", "t2"]]);
    expect(addCalls()).toEqual([]);
  });

  it("a refusal (a loop) shows its message word for word under the field, and the next try clears it", async () => {
    const LOOP = "GA-5 can't build on GA-4: GA-4 already builds on GA-5.";
    h.refuse = LOOP;
    mount(<BuildsOnRows task={ME} />);
    await openFinder();
    type("GA-4");
    press("Enter");
    await settle();
    expect(addCalls()).toEqual([["t5", "GA-4"]]);
    expect(alertText()).toEqual([LOOP]);
    const alert = container.all().find((n) => n.getAttribute("role") === "alert")!;
    expect(cls(alert)).toContain("builds-on-error");
    // a loop through a third card, word for word too
    const THROUGH = "GA-5 can't build on KADE-6: KADE-6 already builds on GA-5, through GA-4.";
    h.refuse = THROUGH;
    await openFinder();
    type("KADE-6");
    press("Enter");
    await settle();
    expect(alertText()).toEqual([THROUGH]);
    // a pick that goes through clears the message
    h.refuse = undefined;
    await openFinder();
    type("GA-4");
    press("Enter");
    await settle();
    expect(alertText()).toEqual([]);
  });

  it("an archived card has no Add a card and no ×", () => {
    mount(<BuildsOnRows task={ME} readOnly />);
    expect(buttonsNamed(container, "Add a card")).toEqual([]);
    expect(byLabel(container, "Take GA-1 off Builds on")).toEqual([]);
    expect(container.all().filter((n) => n.nodeName === "BUTTON")).toEqual([]);
  });
});
