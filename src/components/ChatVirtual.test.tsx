// GA-190 QA: long lists draw only what is in sight. A chat of 1,000 messages and a run's log of 1,000 lines put only the
// groups (lines) in sight, and a margin around them, in the page; each is measured as it shows, so scrolling shows it at
// its height, and what you read doesn't jump; the chat follows the answer at its bottom and keeps its place when you
// scrolled up. React itself renders here, with @tanstack/react-virtual as the app uses it, into the stand-in DOM of
// StreamParses.test.tsx (copied: there is no DOM library in the project): blocks stack, text takes lines, the boxes that
// scroll have their CSS heights; scroll boxes clamp and fire their scroll event; there is a ResizeObserver and the window's
// timers; settle() lets the page settle as a browser does. useChat is a store the test changes; the api is a fake.
import { act, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatMessage, LiveRun, Member, Run, SeqEvent, Task, Team } from "../types";

// ---- a stand-in DOM, as much of it as React DOM and the virtualizer use ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };

// The layout (GA-190). Blocks stack; the other text in a block takes a line of LINE px per CHARS characters; the boxes
// in CSS below have their height, max-height (they scroll) and padding, from src/styles. A test can give nodes a height
// of their own with `layout.fixed`. Margins, widths and wrapping by words are left out.
const LINE = 20, CHARS = 80, WIDTH = 760;
const BLOCK = new Set(["DIV", "P", "UL", "OL", "LI", "PRE", "SECTION", "H1", "H2", "H3", "H4", "H5", "H6", "BLOCKQUOTE", "TABLE", "TR", "DL",
  "DT", "DD", "HR", "DETAILS", "SUMMARY", "FORM"]);
const CSS: Record<string, { height?: number; maxHeight?: number; pad?: number }> = {
  // flex: 1 in the window: 800px tall here
  "chat-scroll": { height: 800 },
  "chat-col": { pad: 24 },
  "run-stream": { maxHeight: 320, pad: 10 },
};
const layout = { fixed: (_n: FakeNode): number | undefined => undefined };
const cssOf = (n: FakeNode) => cls(n).map((c) => CSS[c]).find(Boolean) ?? {};
/** What is in `n`: its blocks stacked, a line per CHARS characters of its other text, and its padding. */
function contentHeight(n: FakeNode): number {
  let h = 0, text = 0;
  for (const c of n.childNodes) {
    if (c.nodeType === 1 && BLOCK.has(c.nodeName)) h += boxHeight(c);
    else text += c.textContent.trim().length;
  }
  return h + LINE * Math.ceil(text / CHARS) + 2 * (cssOf(n).pad ?? 0);
}
/** How tall `n` draws: a height a test gave it, its style's, its CSS's, else what is in it (up to its max-height). */
function boxHeight(n: FakeNode): number {
  if (n.nodeType !== 1) return 0;
  const fixed = n.ownHeight ?? layout.fixed(n);
  if (fixed !== undefined) return fixed;
  const style = parseFloat(String(n.style.height ?? ""));
  if (!Number.isNaN(style)) return style;
  const c = cssOf(n);
  return c.height ?? Math.min(contentHeight(n), c.maxHeight ?? Infinity);
}
/** Where `n`'s top is in the window: under its parent's padding and the blocks before it, less what its parent scrolled. */
function docTop(n: FakeNode): number {
  const p = n.parentNode;
  if (!p || p.nodeType !== 1) return 0;
  let y = docTop(p) + (cssOf(p).pad ?? 0) - p.scrollTop;
  for (const c of p.childNodes) {
    if (c === n) break;
    if (c.nodeType === 1 && BLOCK.has(c.nodeName)) y += boxHeight(c);
  }
  return y;
}
/** The boxes whose scrollTop changed: a browser fires their scroll event before it draws again (settle() does). */
const scrolled = new Set<FakeNode>();
/** The boxes scrolled down: settle() looks whether what is in them shrank under their scrollTop. */
const boxes = new Set<FakeNode>();
/** The window's timers and animation frames: settle() runs them, as time goes by. */
let timers: { id: number; f: () => void }[] = [];
let timerIds = 0;
const later = (f: () => void) => { timers.push({ id: ++timerIds, f }); return timerIds; };
const cancel = (id: number) => { timers = timers.filter((t) => t.id !== id); };
/** A ResizeObserver: settle() tells it of each node it watches whose size changed since it last told, the first time
 *  too, as a browser does after a layout. */
const observers = new Set<FakeResizeObserver>();
class FakeResizeObserver {
  told = new Map<FakeNode, number>();
  constructor(private f: (entries: unknown[], o: FakeResizeObserver) => void) { observers.add(this); }
  observe(n: FakeNode) { if (!this.told.has(n)) this.told.set(n, -1); }
  unobserve(n: FakeNode) { this.told.delete(n); }
  disconnect() { this.told.clear(); observers.delete(this); }
  tell(): boolean {
    const entries = [...this.told].filter(([n, h]) => n.offsetHeight !== h).map(([n]) => {
      const blockSize = n.offsetHeight, inlineSize = n.offsetWidth;
      this.told.set(n, blockSize);
      return { target: n, borderBoxSize: [{ blockSize, inlineSize }], contentRect: { top: 0, left: 0, width: inlineSize, height: blockSize } };
    });
    if (entries.length) this.f(entries, this);
    return entries.length > 0;
  }
}

class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  nodeValue: string | null = null;
  /** A height set by hand (clientHeight = …); else the layout's. */
  ownHeight: number | undefined = undefined;
  private top = 0;
  constructor(public nodeType: number, public nodeName: string, public ownerDocument: FakeNode | null, public namespaceURI = HTML) {}
  get isConnected() { for (let x: FakeNode | null = this; x; x = x.parentNode) if (x === doc) return true; return false; }
  get clientHeight() { return boxHeight(this); }
  set clientHeight(h: number) { this.ownHeight = h; }
  get offsetHeight() { return this.isConnected ? boxHeight(this) : 0; }
  get offsetWidth() { return this.isConnected ? WIDTH : 0; }
  get scrollHeight() { return Math.max(boxHeight(this), contentHeight(this)); }
  /** As in a browser: from 0 to scrollHeight - clientHeight, also when what is in the box shrinks; a change fires the
   *  box's scroll event (at settle()). */
  get scrollTop() {
    if (this.top > 0) this.scrollTop = this.top;
    return this.top;
  }
  set scrollTop(y: number) {
    const to = Math.max(0, Math.min(y, this.scrollHeight - this.clientHeight));
    if (to !== this.top) { this.top = to; scrolled.add(this); }
    if (to > 0) boxes.add(this);
  }
  scrollTo(o: { top?: number }) { if (o.top !== undefined) this.scrollTop = o.top; }
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
  get options() { return this.all().filter((n) => n.nodeName === "OPTION"); }
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
  scrollIntoView() {}
  focus() {}
  blur() {}
  getBoundingClientRect() {
    const top = docTop(this), height = this.offsetHeight;
    return { top, left: 0, right: WIDTH, bottom: top + height, width: WIDTH, height, x: 0, y: top };
  }
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
// GA-190: the virtualizer reaches the window through the scroll box (ownerDocument.defaultView): its timers, animation
// frames and ResizeObserver.
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, addEventListener() {}, removeEventListener() {},
  setTimeout: later, clearTimeout: cancel, requestAnimationFrame: later, cancelAnimationFrame: cancel, ResizeObserver: FakeResizeObserver };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** An event at `target`, through the listeners on the way as a browser calls them (capture down, then bubble up; a scroll
 *  event doesn't bubble). */
function fire(target: FakeNode, type: string) {
  const path: FakeNode[] = [];
  for (let x: FakeNode | null = target; x; x = x.parentNode) path.push(x);
  let stopped = false;
  const bubbles = type !== "scroll";
  const e = { type, target, bubbles, button: 0, defaultPrevented: false, timeStamp: 0,
    preventDefault() { e.defaultPrevented = true; }, stopPropagation() { stopped = true; } };
  for (const n of [...path].reverse()) for (const l of n.listeners.get(type) ?? []) if (l.capture && !stopped) l.f(e);
  for (const n of bubbles ? path : [target]) for (const l of n.listeners.get(type) ?? []) if (!l.capture && !stopped) l.f(e);
}
/** Lets the page settle, as a browser does after a change: the scroll events of the boxes that scrolled, what the
 *  ResizeObservers see, and when nothing else happens, the timers (the virtualizer's "scrolling" ends 150ms after the
 *  last scroll event). */
async function settle() {
  for (let round = 0; round < 200; round++) {
    let busy = false;
    await act(async () => {
      for (const n of boxes) if (!n.isConnected || n.scrollTop === 0) boxes.delete(n);
      for (const n of [...scrolled]) { scrolled.delete(n); busy = true; fire(n, "scroll"); }
      for (const o of [...observers]) if (o.tell()) busy = true;
      if (!busy) { const due = timers; timers = []; for (const t of due) { busy = true; t.f(); } }
    });
    if (!busy) return;
  }
  throw new Error("the page doesn't settle");
}
/** You scroll the box to `y` (the browser clamps it), and the page settles. */
async function scrollTo(box: FakeNode, y: number) {
  await act(async () => { box.scrollTop = y; if (scrolled.delete(box)) fire(box, "scroll"); });
  await settle();
}
const cls = (n: FakeNode) => (n.getAttribute("class") ?? "").split(" ");
const byClass = (root: FakeNode, c: string) => root.all().filter((n) => n.nodeType === 1 && cls(n).includes(c));
const buttonsNamed = (root: FakeNode, text: string) => root.all().filter((n) => n.nodeName === "BUTTON" && n.textContent.trim() === text);
const click = async (n: FakeNode) => { await act(async () => { fire(n, "click"); }); await settle(); };

// ---- what each part reads ----
const h = vi.hoisted(() => ({
  /** Each parse of Markdown, by its text. */
  parses: new Map<string, number>(),
  chat: null as unknown as Record<string, unknown>,
  listeners: new Set<() => void>(),
  /** What the fake api answers, by function name; a function without an answer never settles. */
  api: {} as Record<string, (...a: unknown[]) => unknown>,
  calls: [] as [string, unknown[]][],
  live: [] as LiveRun[],
  runs: [] as Run[],
}));

vi.mock("react-markdown", async (orig) => {
  const real = await orig<typeof import("react-markdown")>();
  const Markdown = (props: Parameters<typeof real.default>[0]) => {
    const t = String(props.children);
    h.parses.set(t, (h.parses.get(t) ?? 0) + 1);
    return real.default(props);
  };
  return { ...real, default: Markdown };
});
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { h.calls.push([k, a]); return h.api[k] ? h.api[k](...a) : new Promise(() => {}); };
  }
  return out;
});
vi.mock("./chat/useChat", async () => {
  const R = await import("react");
  const subscribe = (f: () => void) => { h.listeners.add(f); return () => { h.listeners.delete(f); }; };
  return { useChat: () => R.useSyncExternalStore(subscribe, () => h.chat), useChatLive: () => [] };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (...a: unknown[]) => { h.calls.push(["openUrl", a]); return Promise.resolve(); } }));
vi.mock("./MarkdownEditor", () => ({ MarkdownEditor: () => null }));
vi.mock("../lib/useDropZone", () => ({ useDropZone: () => false }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => h.live }));
vi.mock("../lib/useData", () => ({ useData: () => ({ data: h.runs, error: null, reload: () => {}, setData: () => {} }) }));

const { createRoot } = await import("react-dom/client");
const { ChatThread } = await import("./chat/ChatThread");
const { RunPanel } = await import("./RunPanel");
const { RunHistory } = await import("./RunHistory");

let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
async function mount(el: ReactNode) {
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(el); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
  await settle();
}
/** The chat as useChat now gives it. */
async function chat(over: Record<string, unknown>) {
  await act(async () => { h.chat = { ...h.chat, ...over }; h.listeners.forEach((f) => f()); });
  await settle();
}
const parsed = (t: string) => h.parses.get(t) ?? 0;

beforeEach(() => {
  h.parses.clear();
  h.calls.length = 0;
  for (const k of Object.keys(h.api)) delete h.api[k];
  h.chat = { messages: [], draft: "", tool: null, working: false, queue: [], pending: [], error: null, setWorking: () => {}, reload: () => {} };
  layout.fixed = () => undefined;
});
afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
  scrolled.clear();
  boxes.clear();
  timers = [];
  observers.clear();
  vi.useRealTimers();
});

// ---- a chat of 1,000 messages ----
const T0 = 1_760_000_000_000;
const MIN = 60_000;
const lead = { actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status: "active", isLead: false,
  allowedTools: [], chatEnabled: true } as Member;
const message = (i: number, role: string, body: string, more: Partial<ChatMessage> = {}): ChatMessage => ({
  id: `m${i}`, threadId: "c1", role, authorId: role === "user" ? "you" : "lead", authorName: role === "user" ? "You" : "Team Lead", bodyMd: body,
  runId: role === "user" ? null : `r${Math.floor(i / 2)}`, toolName: null, tool: null, createdAt: T0 + i * 10 * MIN, ...more,
});
const answerText = (i: number) => `Answer ${i}: **done**, see [the board](https://example.com/${i}) and \`task ${i}\`.`;
/** `n` messages: yours and the Team Lead's answers in turn, each a group of its own; your question last. */
const conversation = (n: number) => Array.from({ length: n }, (_, i) => i % 2 === 0 ? message(i, "agent", answerText(i)) : message(i, "user", `Question ${i}`));
/** A draft of `n` paragraphs, each a line tall. */
const lines = (n: number) => Array.from({ length: n }, (_, i) => `Paragraph ${i} of the answer being written.`).join("\n\n");
/** How tall group `i` draws: 60 to 200px. Its estimate before it is measured is about 100px (a header, and a line per 90
 *  characters), so most are off. */
const groupHeight = (i: number) => 60 + ((i * 53) % 141);
/** The heights of groups `from` to `to` (not included). */
const sum = (from: number, to: number) => Array.from({ length: Math.max(0, to - from) }, (_, k) => groupHeight(from + k)).reduce((a, b) => a + b, 0);
/** Gives the chat's first `upTo` groups their height (later ones draw what is in them). */
const measured = (upTo = Infinity) => {
  layout.fixed = (n) => {
    if (!cls(n).includes("chat-row")) return undefined;
    const i = Number(n.getAttribute("data-index"));
    return i < upTo ? groupHeight(i) : undefined;
  };
};
const PAD = 24;
const rows = () => byClass(container, "chat-row");
const indexOf = (r: FakeNode) => Number(r.getAttribute("data-index"));
const scroller = () => byClass(container, "chat-scroll")[0];
const end = (s: FakeNode) => s.scrollHeight - s.clientHeight;
const rect = (n: FakeNode) => n.getBoundingClientRect();
const view = (s: FakeNode) => ({ top: rect(s).top, bottom: rect(s).top + s.clientHeight });
/** The groups in the page that show in the view, if only in part. */
const inView = (s: FakeNode) => rows().filter((r) => rect(r).bottom > view(s).top && rect(r).top < view(s).bottom);
/** The group at the top of the view. */
const rowAtTop = (s: FakeNode) => rows().find((r) => rect(r).bottom > view(s).top)!;
const totalParses = () => [...h.parses.values()].reduce((a, b) => a + b, 0);

describe("a chat of 1,000 messages (GA-190)", () => {
  it("A chat of 1,000 messages puts only the messages in sight, plus a margin, in the page", async () => {
    measured();
    const messages = conversation(1000);
    h.chat = { ...h.chat, messages };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const s = scroller();
    // It opens at its end: the last group is in the page, at the bottom of the view.
    expect(s.scrollTop).toBe(end(s));
    const page = rows();
    expect(indexOf(page.at(-1)!)).toBe(999);
    expect(page.at(-1)!.textContent).toContain("Question 999");
    expect(rect(page.at(-1)!).bottom).toBe(view(s).bottom - PAD);
    // In the page: the groups in sight and the 6 before them, one after the other; 800px of groups of 60px or more, and
    // the margin, at most 27 of the 1,000.
    expect(page.map(indexOf)).toEqual(Array.from({ length: page.length }, (_, k) => 1000 - page.length + k));
    const seen = inView(s);
    expect(seen.length).toBeGreaterThan(3);
    expect(page.length - seen.length).toBe(6);
    expect(page.length).toBeLessThanOrEqual(Math.ceil(800 / 60) + 1 + 2 * 6);
    // The view is filled, from its top.
    expect(rect(seen[0]).top).toBeLessThanOrEqual(view(s).top);
    // The groups out of the page are space of their (estimated) height: the column is far taller than the page's groups.
    expect(s.scrollHeight).toBeGreaterThan(50_000);
    // Only the answers in the page have been parsed, each once.
    const shown = new Set(page.map(indexOf));
    expect(Array.from({ length: 500 }, (_, k) => parsed(answerText(2 * k)))).toEqual(Array.from({ length: 500 }, (_, k) => (shown.has(2 * k) ? 1 : 0)));
    expect(totalParses()).toBe(page.filter((r) => indexOf(r) % 2 === 0).length);
  });

  it("scrolling shows each message at its right height, without jumps", async () => {
    measured();
    const N = 60;
    h.chat = { ...h.chat, messages: conversation(N) };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const s = scroller();
    // Each group in the page is as tall as it draws, right under the one before it, and those in the page cover the view.
    const placed = () => {
      const page = rows();
      page.forEach((r, k) => {
        expect(rect(r).height).toBe(groupHeight(indexOf(r)));
        if (k > 0) expect(rect(r).top).toBe(rect(page[k - 1]).bottom);
      });
      expect(indexOf(page[0]) === 0 || rect(page[0]).top <= view(s).top).toBe(true);
      expect(indexOf(page.at(-1)!) === N - 1 || rect(page.at(-1)!).bottom >= view(s).bottom).toBe(true);
    };
    placed();

    // You scroll up 200px at a time, to the top. The groups that come in the page are measured (their estimate was off),
    // and the scroll makes up for the difference above the view: what you saw moves down by the 200px you scrolled, no
    // more and no less.
    let steps = 0, madeUp = 0;
    for (; s.scrollTop > 0 && steps < 100; steps++) {
      const seen = rowAtTop(s);
      const y = rect(seen).top;
      const to = Math.max(0, s.scrollTop - 200);
      const by = s.scrollTop - to;
      await scrollTo(s, to);
      if (s.scrollTop !== to) madeUp++;
      expect(seen.isConnected).toBe(true);
      expect(rect(seen).top).toBe(y + by);
      placed();
    }
    expect(steps).toBeGreaterThan(10);
    // (the scroll did make up for groups measured above the view, on most steps)
    expect(madeUp).toBeGreaterThan(steps / 2);
    expect(s.scrollTop).toBe(0);
    // Every group has been in the page now, so the chat is as tall as its groups (and the column's padding).
    expect(s.scrollHeight).toBe(2 * PAD + sum(0, N));

    // In the middle: the space above the groups in the page is the height of the groups above them, the space under them
    // that of the groups under them, and each group is where the groups before it end.
    await scrollTo(s, 2_000);
    expect(s.scrollTop).toBe(2_000);
    placed();
    const page = rows();
    const first = indexOf(page[0]), last = indexOf(page.at(-1)!);
    expect(first).toBeGreaterThan(0);
    expect(last).toBeLessThan(N - 1);
    const space = page[0].parentNode!.childNodes.filter((n) => !cls(n).includes("chat-row")).map((n) => parseFloat(String(n.style.height)));
    expect(space).toEqual([sum(0, first), sum(last + 1, N)]);
    for (const r of page) expect(rect(r).top).toBe(view(s).top - 2_000 + PAD + sum(0, indexOf(r)));
    // The group at the top of the view is the one whose place takes in scrollTop, with 6 groups before it in the page.
    const top = indexOf(rowAtTop(s));
    expect(PAD + sum(0, top)).toBeLessThanOrEqual(2_000);
    expect(PAD + sum(0, top + 1)).toBeGreaterThan(2_000);
    expect(top - first).toBe(6);
  });

  it("The chat follows the answer at the bottom and keeps its place when you scrolled up", async () => {
    // the 1,000 groups have their heights; the answer's new groups draw what is in them, so they grow as it is written
    measured(1000);
    const messages = conversation(1000);
    h.chat = { ...h.chat, messages, working: true };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const s = scroller();
    expect(s.scrollTop).toBe(end(s));

    // At the bottom it follows the answer as it grows...
    await chat({ draft: lines(2) });
    const tall = s.scrollHeight;
    expect(s.scrollTop).toBe(end(s));
    await chat({ draft: lines(6) });
    expect(s.scrollHeight).toBe(tall + 4 * LINE);
    expect(s.scrollTop).toBe(end(s));
    // ...and a new message: the answer's first block, in a group at the end of the list, the draft under it.
    const block = message(1000, "agent", "The first block of this answer.", { runId: "r500" });
    await chat({ messages: [...messages, block], draft: lines(1) });
    expect(s.scrollTop).toBe(end(s));
    const newest = rows().at(-1)!;
    expect(indexOf(newest)).toBe(1000);
    expect(newest.textContent).toContain("The first block of this answer.");
    expect(byClass(newest, "draft")).toHaveLength(1);
    expect(rect(newest).bottom).toBeLessThanOrEqual(view(s).bottom);
    // The draft grows in that group, which is measured again: still at the bottom.
    await chat({ draft: lines(7) });
    expect(s.scrollTop).toBe(end(s));
    expect(rect(byClass(newest, "draft")[0]).bottom).toBeLessThanOrEqual(view(s).bottom);

    // You scroll far up to read: the chat keeps its place, and the group you read stays where it is, while the answer
    // grows, a tool runs, a message comes in and the answer ends.
    await scrollTo(s, 30_000);
    const at = s.scrollTop;
    const reading = rowAtTop(s);
    const y = rect(reading).top;
    await chat({ draft: lines(12) });
    await chat({ tool: "mcp__gizai__create_task" });
    const second = message(1001, "agent", "The second block.", { runId: "r500" });
    await chat({ messages: [...messages, block, second], draft: "", tool: null });
    await chat({ working: false });
    expect(s.scrollTop).toBe(at);
    expect(reading.isConnected).toBe(true);
    expect(rect(reading).top).toBe(y);

    // Back at the bottom, it follows again.
    await scrollTo(s, end(s));
    await chat({ working: true, draft: lines(3) });
    expect(s.scrollTop).toBe(end(s));
    expect(rect(byClass(container, "draft")[0]).bottom).toBeLessThanOrEqual(view(s).bottom);
  });
});

// ---- a run's log of 1,000 lines ----
const team = { id: "team", name: "Software", members: [], states: [], labels: [] } as unknown as Team;
const task = { id: "t1", identifier: "GA-1", title: "A card", stateId: "doing", assigneeId: null, hold: null } as unknown as Task;
const eventText = (i: number) => `Step ${i}: read **src/a${i}.ts** and [its docs](https://example.com/${i}).`;
const stepLine = (i: number) => `Step ${i}: read src/a${i}.ts and its docs.`;
const textEvent = (i: number): SeqEvent => ({ seq: i, event: { kind: "text", text: eventText(i) } });
const thousand = () => Array.from({ length: 1000 }, (_, i) => textEvent(i + 1));
const run = { id: "run1", agentId: "be", agentName: "Backend Agent", taskId: "t1", roleKey: "backend", trigger: "manual", status: "running",
  createdAt: T0, startedAt: T0, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "" } as Run;
const logLines = (log: FakeNode) => log.childNodes.filter((n) => n.hasAttribute("data-index"));
/** The most lines in the page: 320px of lines of 20px, and 20 lines before and after them. */
const MOST = 320 / LINE + 1 + 2 * 20;

describe("a run's live and past logs draw only what is in sight (GA-190)", () => {
  it("the live log of a run with 1,000 lines on the card page holds the lines in sight, and shows a new line at its end", async () => {
    let send: (m: { runId: string; seq: number; event: SeqEvent["event"] }) => void = () => {};
    h.api.onRunEvent = (f) => { send = f as typeof send; return Promise.resolve(() => {}); };
    h.api.runEvents = () => Promise.resolve(thousand());
    h.live = [{ runId: "run1", taskId: "t1", agentId: "be" }];
    h.runs = [run];
    await mount(<RunPanel task={task} team={team} />);
    // The live log keeps the 500 newest events (mergeEvents); of those, only the lines in sight and a margin are in the page.
    const log = byClass(container, "run-stream")[0];
    expect(log.clientHeight).toBe(320);
    expect(log.scrollTop).toBe(log.scrollHeight - log.clientHeight);
    const page = logLines(log);
    expect(page.length).toBeGreaterThan(320 / LINE);
    expect(page.length).toBeLessThanOrEqual(MOST);
    expect(page.at(-1)!.textContent).toBe(stepLine(1000));
    expect(totalParses()).toBe(page.length);

    // A new line: it is in the page, at the end of the view; still only the lines in sight.
    await act(async () => { send({ runId: "run1", ...textEvent(1001) }); });
    await settle();
    expect(logLines(log).at(-1)!.textContent).toBe(stepLine(1001));
    expect(log.scrollTop).toBe(log.scrollHeight - log.clientHeight);
    expect(logLines(log).length).toBeLessThanOrEqual(MOST);
  });

  it("Run history's Show the whole output of a run with 1,000 lines holds the lines in sight, at its end and at its start", async () => {
    h.api.runEvents = () => Promise.resolve(thousand());
    await mount(<RunHistory runs={[{ ...run, id: "run9", status: "succeeded", outcome: "ready_for_testing", endedAt: T0 + MIN }]} />);
    await click(byClass(container, "run-open")[0]);
    await click(buttonsNamed(container, "Show the whole output (1000 steps)")[0]);
    const log = byClass(container, "run-stream")[0];
    expect(log.scrollTop).toBe(log.scrollHeight - log.clientHeight);
    expect(logLines(log).length).toBeLessThanOrEqual(MOST);
    expect(logLines(log).at(-1)!.textContent).toBe(stepLine(1000));

    // Scrolled to its start: the first lines, still only those in sight.
    await scrollTo(log, 0);
    expect(log.scrollTop).toBe(0);
    expect(logLines(log)[0].textContent).toBe(stepLine(1));
    expect(logLines(log).length).toBeLessThanOrEqual(MOST);
  });
});
