// GA-190 QA: "A keystroke in the chat, a card's title, a comment or the Editor re-renders only its own component (a test
// counts renders)". React itself renders here (react-dom/client), into a small stand-in DOM: there is no DOM library in
// the project (as in components/StreamParses.test.tsx). Renders are counted per component by name, the way React
// DevTools does: a stand-in DevTools hook is told of each commit and walks the fibers that did work in it (a component
// that bailed out, or a subtree React left alone, is not counted). The parts the counts are about (Composer, TitleInput,
// CommentComposer, CursorItem, OpenInAtCursor) are not exported, so they are counted by their function's name; their
// parents (ChatThread, TaskPage, FolderEditor) are counted that way too, and once more through a hook only they call.
// The Markdown editor (CodeMirror) is a stand-in that keeps its props, so a test can "type" through its onChange; the
// Editor's code view is a stand-in that keeps the real EditorState and tells the page's update listener what each
// transaction did, as in EditorPage.test.tsx.
import { act, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EditorState, Transaction, type TransactionSpec } from "@codemirror/state";
import type { ChatMessage, DirEntry, EditorFolder, Member, OpenedFile, Task, Team } from "../types";

// ---- render counts, by component name, as React DevTools sees them ----
type Fiber = { tag: number; type: unknown; flags: number; child: Fiber | null; sibling: Fiber | null; alternate: Fiber | null };
const h = vi.hoisted(() => {
  const renders = new Map<string, number>();
  const nameOf = (t: unknown) => (t && (typeof t === "function" || typeof t === "object") ? ((t as { displayName?: string }).displayName ?? (t as { name?: string }).name ?? "?") : "?");
  // FunctionComponent 0, ForwardRef 11, SimpleMemoComponent 15; flag 1 is React's PerformedWork (the component rendered)
  const walk = (next: Fiber, prev: Fiber | null) => {
    if ((next.tag === 0 || next.tag === 11 || next.tag === 15) && (!prev || (next.flags & 1))) {
      const n = nameOf(next.type);
      renders.set(n, (renders.get(n) ?? 0) + 1);
    }
    if (prev && next.child === prev.child) return; // React didn't go into this subtree
    for (let c = next.child; c; c = c.sibling) walk(c, c.alternate);
  };
  (globalThis as Record<string, unknown>).__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    isDisabled: false, supportsFiber: true, renderers: new Map(), inject: () => 1, checkDCE() {},
    onScheduleFiberRoot() {}, onCommitFiberUnmount() {}, onPostCommitFiberRoot() {},
    onCommitFiberRoot: (_id: number, root: { current: Fiber }) => walk(root.current, root.current.alternate),
  };
  return {
    renders,
    /** Calls of hooks only one page component calls: ChatThread (useChat), TaskPage (useLiveRuns), FolderEditor (useEditorApps). */
    calls: { ChatThread: 0, TaskPage: 0, FolderEditor: 0 },
    chat: null as unknown as Record<string, unknown>,
    chatListeners: new Set<() => void>(),
    /** What the fake api answers, by function name; a function without an answer never settles. */
    api: {} as Record<string, (...a: unknown[]) => unknown>,
    apiCalls: [] as [string, unknown[]][],
    /** True while useData's stand-in asks the api: then the api answers at once, not with a promise. */
    sync: false,
    /** The props each Markdown editor got last, by its aria-label. */
    editors: new Map<string, Record<string, unknown>>(),
    cleared: 0,
    /** The line Open in got, each time it rendered. */
    openIn: [] as (number | null)[],
    views: [] as unknown[],
    went: [] as unknown[],
  };
});
const rendersOf = (name: string) => h.renders.get(name) ?? 0;
const resetCounts = () => { h.renders.clear(); h.calls.ChatThread = 0; h.calls.TaskPage = 0; h.calls.FolderEditor = 0; h.openIn.length = 0; };

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
  value?: string;
  type?: string;
  scrollTop = 0;
  scrollHeight = 0;
  clientHeight = 0;
  constructor(public nodeType: number, public nodeName: string, public ownerDocument: FakeNode | null, public namespaceURI = HTML) {}
  get tagName() { return this.nodeName; }
  get firstChild(): FakeNode | null { return this.childNodes[0] ?? null; }
  get lastChild(): FakeNode | null { return this.childNodes[this.childNodes.length - 1] ?? null; }
  get nextSibling(): FakeNode | null { const p = this.parentNode; return p ? p.childNodes[p.childNodes.indexOf(this) + 1] ?? null : null; }
  get parentElement(): FakeNode | null { return this.parentNode; }
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
  dispatchEvent() { return true; }
  contains(n: FakeNode | null) { for (let x = n; x; x = x.parentNode) if (x === this) return true; return false; }
  closest() { return null; }
  querySelector() { return null; }
  scrollIntoView() {}
  focus() {}
  /** As in a browser: the element loses the focus, and React's onBlur (focusout) hears it. */
  blur() { fire(this, "focusout"); }
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
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, innerWidth: 1400, innerHeight: 900,
  addEventListener() {}, removeEventListener() {}, setTimeout, clearTimeout };
// React reads "oninput" in document when it loads, to know the input event is there (text boxes' onChange).
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win, hidden: false, oninput: null, getElementById: () => null });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
vi.stubGlobal("navigator", { userAgent: "Linux", platform: "Linux x86_64", clipboard: { writeText: () => Promise.resolve() } });
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));
vi.stubGlobal("cancelAnimationFrame", (t: ReturnType<typeof setTimeout>) => clearTimeout(t));
vi.stubGlobal("CSS", { escape: (s: string) => s });
const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** An event at `target`, through the listeners on the way as a browser calls them (capture down, then bubble up). */
function fire(target: FakeNode, type: string, more: Record<string, unknown> = {}) {
  const path: FakeNode[] = [];
  for (let x: FakeNode | null = target; x; x = x.parentNode) path.push(x);
  let stopped = false;
  const e = { type, target, bubbles: true, button: 0, defaultPrevented: false, timeStamp: 0, ...more,
    preventDefault() { e.defaultPrevented = true; }, stopPropagation() { stopped = true; } };
  for (const n of [...path].reverse()) for (const l of n.listeners.get(type) ?? []) if (l.capture && !stopped) l.f(e);
  for (const n of path) for (const l of n.listeners.get(type) ?? []) if (!l.capture && !stopped) l.f(e);
}
const cls = (n: FakeNode) => (n.getAttribute("class") ?? "").split(" ");
const byClass = (root: FakeNode, c: string) => root.all().filter((n) => n.nodeType === 1 && cls(n).includes(c));
const byLabel = (root: FakeNode, label: string) => root.all().filter((n) => n.getAttribute("aria-label") === label);
const buttonsNamed = (root: FakeNode, text: string) => root.all().filter((n) => n.nodeName === "BUTTON" && n.textContent.trim() === text);
const click = async (n: FakeNode) => { await act(async () => { fire(n, "click"); }); };
/** Types into a text box as a person does: the box has the new text, then the input event. */
async function typeInto(input: FakeNode, text: string) {
  input.type = "text";
  input.value = text;
  await act(async () => { fire(input, "input"); });
}
const key = async (n: FakeNode, k: string) => { await act(async () => { fire(n, "keydown", { key: k }); }); };

// ---- the parts the tests leave out or stand in for ----
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => {
      h.apiCalls.push([k, a]);
      const f = h.api[k];
      if (!f) return new Promise(() => {});
      return h.sync ? f(...a) : Promise.resolve().then(() => f(...a));
    };
  }
  return out;
});
// The Markdown editor (CodeMirror): keeps its props by its label, and gives its parent a handle that counts clears.
vi.mock("../components/MarkdownEditor", () => ({
  MarkdownEditor: (p: Record<string, unknown>) => {
    h.editors.set(String(p.ariaLabel), p);
    const handle = p.handle as { current: unknown } | undefined;
    if (handle) handle.current = { focus: () => {}, clear: () => { h.cleared++; }, startLink: () => {}, goto: () => {} };
    return null;
  },
}));
vi.mock("../components/chat/useChat", async () => {
  const R = await import("react");
  const subscribe = (f: () => void) => { h.chatListeners.add(f); return () => { h.chatListeners.delete(f); }; };
  return { useChat: () => { h.calls.ChatThread++; return R.useSyncExternalStore(subscribe, () => h.chat); }, useChatLive: () => [] };
});
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: () => Promise.resolve() }));
vi.mock("../lib/useDropZone", () => ({ useDropZone: () => false }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => { h.calls.TaskPage++; return []; } }));
// The data hooks answer at once with what the api answers (as in TaskPageHand.test).
vi.mock("../lib/useData", async (orig) => ({
  ...(await orig<object>()),
  useData: (fetch: () => unknown) => {
    let data: unknown = null;
    h.sync = true;
    try { const v = fetch(); if (!(v instanceof Promise)) data = v; } catch { /* no data */ } finally { h.sync = false; }
    return { data, error: null, reload: () => {}, setData: () => {} };
  },
}));
vi.mock("../router", async (orig) => ({ ...(await orig<object>()), go: (r: unknown) => { h.went.push(r); } }));
// The card page's parts these tests don't look at.
vi.mock("../components/RunHistory", () => ({ RunHistory: () => null }));
vi.mock("../components/Properties", () => ({ Properties: () => null }));
vi.mock("../components/FileDrop", async (orig) => ({ ...(await orig<object>()), FileDrop: () => null }));
vi.mock("../components/RunPanel", () => ({ RunPanel: () => null }));
vi.mock("../components/PullPanel", () => ({ PullPanel: () => null }));
vi.mock("../components/changes/ChangesView", () => ({ ChangesView: () => null }));
// The Editor's code view: the real EditorView's statics with a stand-in instance that keeps the state and tells the
// update listeners (the page's) about each transaction.
type Update = { state: EditorState; startState: EditorState; docChanged: boolean; selectionSet: boolean; transactions: Transaction[]; view: unknown };
vi.mock("@codemirror/view", async (orig) => {
  const real = await orig<typeof import("@codemirror/view")>();
  const { EditorState: State, Transaction: Tr } = await import("@codemirror/state");
  class View {
    state = State.create({ doc: "" });
    setState(s: EditorState) { this.state = s; }
    dispatch(...specs: (Transaction | TransactionSpec)[]) {
      const tr = specs[0] instanceof Tr ? specs[0] : this.state.update(...(specs as TransactionSpec[]));
      this.state = tr.state;
      const u: Update = { state: tr.state, startState: tr.startState, docChanged: tr.docChanged, selectionSet: !!tr.selection, transactions: [tr], view: this };
      for (const l of tr.state.facet(real.EditorView.updateListener)) l(u as never);
    }
    focus() {}
    destroy() {}
  }
  function EditorView() { const v = new View(); h.views.push(v); return v; }
  Object.setPrototypeOf(EditorView, real.EditorView);
  return { ...real, EditorView };
});
vi.mock("@codemirror/search", async (orig) => ({ ...(await orig<object>()), openSearchPanel: () => true, gotoLine: () => true }));
vi.mock("../components/editor/FolderSearch", () => ({ FolderSearch: () => null }));
vi.mock("../components/editor/GoToFile", () => ({ GoToFile: () => null }));
vi.mock("../components/editor/FolderPicker", () => ({ FolderPicker: () => null }));
vi.mock("../components/editor/Menus", async (orig) => {
  const real = await orig<typeof import("../components/editor/Menus")>();
  return {
    ...real,
    // only FolderEditor asks for the apps: each call is one of its renders
    useEditorApps: () => { h.calls.FolderEditor++; return real.useEditorApps(); },
    OpenInMenu: (p: { line: number | null }) => { h.openIn.push(p.line); return null; },
  };
});
vi.mock("../lib/terminals", async (orig) => ({
  ...(await orig<object>()),
  useTerminals: () => [], useTerminalsLoaded: () => true, useShownHere: () => "", useShownIn: () => null, noteOpened: () => {}, showIn: () => {},
}));
vi.mock("../components/terminal/TerminalView", () => ({ TerminalView: () => null }));

const { createRoot } = await import("react-dom/client");
const { ChatThread } = await import("../components/chat/ChatThread");
const { SUGGESTIONS } = await import("../lib/chat");
const { TaskPage } = await import("./TaskPage");
const { EditorPage } = await import("./EditorPage");

let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
async function flush() { await act(async () => { for (let i = 0; i < 5; i++) await new Promise((r) => setTimeout(r, 0)); }); }
/** Flushes until the Editor page has stopped drawing (its first answers can come late when the machine is busy), so a
 *  count that follows sees only what the step itself drew. */
async function quiet() {
  for (let i = 0; i < 20; i++) { const before = h.renders.get("FolderEditor") ?? 0; await flush(); if ((h.renders.get("FolderEditor") ?? 0) === before) return; }
}
async function mount(el: ReactNode) {
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(el); });
  await flush();
}
async function show(el: ReactNode) { await act(async () => { root!.render(el); }); await flush(); }

beforeEach(() => {
  resetCounts();
  h.apiCalls.length = 0;
  for (const k of Object.keys(h.api)) delete h.api[k];
  h.editors.clear();
  h.cleared = 0;
  h.went.length = 0;
  h.views.length = 0;
  store.clear();
});
afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
});

// ---- the counter itself ----
describe("the render counter", () => {
  it("counts a component whose state changes and the children it draws again, not a memoised child nor its parent", async () => {
    const R = await import("react");
    const Leaf = () => <i>leaf</i>;
    const Kept = R.memo(function Kept() { return <b>kept</b>; });
    let bump: () => void = () => {};
    function Inner() { const [n, setN] = R.useState(0); bump = () => setN((x) => x + 1); return <div>{n}<Leaf /><Kept /></div>; }
    function Outer() { return <section><Inner /></section>; }
    await mount(<Outer />);
    expect([rendersOf("Outer"), rendersOf("Inner"), rendersOf("Leaf"), rendersOf("Kept")]).toEqual([1, 1, 1, 1]);
    resetCounts();
    await act(async () => { bump(); });
    expect([rendersOf("Outer"), rendersOf("Inner"), rendersOf("Leaf"), rendersOf("Kept")]).toEqual([0, 1, 1, 0]);
    resetCounts();
    await show(<Outer />);
    expect([rendersOf("Outer"), rendersOf("Inner"), rendersOf("Leaf"), rendersOf("Kept")]).toEqual([1, 1, 1, 0]);
  });
});

// ---- the chat ----
const T0 = 1_760_000_000_000;
const lead = { actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status: "active", isLead: false,
  allowedTools: [], chatEnabled: true } as Member;
const message = (i: number, role: string, body: string): ChatMessage => ({
  id: `m${i}`, threadId: "c1", role, authorId: role === "user" ? "you" : "lead", authorName: role === "user" ? "You" : "Team Lead", bodyMd: body,
  runId: role === "user" ? null : `r${i}`, toolName: null, tool: null, createdAt: T0 + i * 600_000,
});
const conversation = () => Array.from({ length: 12 }, (_, i) => i % 2 === 0 ? message(i, "user", `Question ${i}`) : message(i, "agent", `Answer ${i}: **done**.`));
const composer = () => h.editors.get("Message the Team Lead")!;
const typeInChat = async (text: string) => { await act(async () => { (composer().onChange as (v: string) => void)(text); }); };
const sendButton = () => byLabel(container, "Send")[0];
const chatState = (over: Record<string, unknown> = {}) => ({ messages: [], draft: "", tool: null, working: false, queue: [], pending: [], error: null,
  setWorking: () => {}, reload: () => {}, ...over });

describe("a keystroke in the chat redraws the composer only (GA-190)", () => {
  it("each of 10 keystrokes: the Composer renders once; ChatThread and the message groups don't", async () => {
    h.chat = chatState({ messages: conversation() });
    await mount(<ChatThread threadId="c1" agent={lead} />);
    // the counter sees the chat as it opens: the thread, its composer and its groups
    expect(rendersOf("ChatThread")).toBeGreaterThan(0);
    expect(rendersOf("Composer")).toBeGreaterThan(0);
    expect(rendersOf("MessageGroup")).toBeGreaterThan(0);
    expect(byClass(container, "chat-msg").length).toBeGreaterThan(0);

    let typed = "";
    const per: Record<string, number>[] = [];
    for (const c of "Hello Lead") {
      resetCounts();
      typed += c;
      await typeInChat(typed);
      per.push({ Composer: rendersOf("Composer"), ChatThread: rendersOf("ChatThread"), MessageGroup: rendersOf("MessageGroup"), RunsOn: rendersOf("RunsOn"),
        useChat: h.calls.ChatThread });
      expect(composer().value).toBe(typed);
    }
    expect(per).toEqual(Array.from({ length: 10 }, () => ({ Composer: 1, ChatThread: 0, MessageGroup: 0, RunsOn: 0, useChat: 0 })));
    // the box having text turns Send on
    expect(sendButton().hasAttribute("disabled")).toBe(false);
  });

  it("Send sends the typed text and empties the box; Enter sends what the box says", async () => {
    h.chat = chatState({ messages: conversation() });
    h.api.sendChat = () => "c1";
    await mount(<ChatThread threadId="c1" agent={lead} />);
    expect(sendButton().hasAttribute("disabled")).toBe(true);
    await typeInChat("  Make a card for the login bug  ");
    await click(sendButton());
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "sendChat")).toEqual([["sendChat", ["c1", "Make a card for the login bug", null, []]]]);
    expect(composer().value).toBe("");
    expect(h.cleared).toBe(1);
    expect(sendButton().hasAttribute("disabled")).toBe(true);
    expect(h.went).toEqual([]);

    // Enter: the editor hands over its text
    await typeInChat("Second");
    await act(async () => { (composer().onEnter as (v: string) => void)("Second"); });
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "sendChat").map(([, a]) => a[1])).toEqual(["Make a card for the login bug", "Second"]);
    expect(composer().value).toBe("");
  });

  it("a send that fails keeps the text", async () => {
    h.chat = chatState({ messages: conversation() });
    h.api.sendChat = () => { throw new Error("offline"); };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    await typeInChat("Keep me");
    await click(sendButton());
    await flush();
    expect(composer().value).toBe("Keep me");
    expect(h.cleared).toBe(0);
    expect(byClass(container, "chat-banner").map((n) => n.textContent)).toEqual(["Error: offline"]);
  });

  it("a suggestion puts its text in the box (the composer redraws, not the thread), and Send sends it", async () => {
    h.chat = chatState();
    h.api.sendChat = () => "c9";
    await mount(<ChatThread threadId={null} agent={lead} />);
    const s = SUGGESTIONS[1]!;
    resetCounts();
    await click(buttonsNamed(container, s.label)[0]);
    expect(composer().value).toBe(s.text);
    expect({ Composer: rendersOf("Composer"), ChatThread: rendersOf("ChatThread"), useChat: h.calls.ChatThread }).toEqual({ Composer: 1, ChatThread: 0, useChat: 0 });
    await click(sendButton());
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "sendChat")).toEqual([["sendChat", [null, s.text.trim(), null, []]]]);
    // a new chat opens its own page
    expect(h.went).toEqual([{ page: "chat", id: "c9" }]);
    expect(composer().value).toBe("");
  });
});

// ---- the card page: its title and a comment ----
const team: Team = {
  id: "team", name: "Software", labels: [], members: [],
  states: [{ id: "s-todo", name: "To do", category: "ready", sortKey: "a1" }, { id: "s-done", name: "Done", category: "done", sortKey: "a5" }],
} as unknown as Team;
const card = (id: string, more: Partial<Task> = {}): Task => ({
  id, identifier: `KADE-${id}`, projectId: "p1", projectName: "Kade portal", projectColor: "#36f", title: "Fix the invoice total", descriptionMd: "",
  acceptanceMd: "", stateId: "s-todo", stateName: "To do", stateCategory: "ready", priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0",
  testing: true, createdAt: 0, updatedAt: 0, byHand: false, assigneeId: null, assigneeName: null, assigneeKind: null, ...more,
} as unknown as Task);
let tasks: Record<string, Task>;
const titleBox = () => byLabel(container, "Title")[0];
const commentBox = () => h.editors.get("New comment");
const commentButton = () => buttonsNamed(container, "Comment")[0];
const tabNamed = (name: string) => container.all().find((n) => n.getAttribute("role") === "tab" && n.nodeName === "BUTTON" && n.textContent.startsWith(name))!;
const typeComment = async (text: string) => { await act(async () => { (commentBox()!.onChange as (v: string) => void)(text); }); };

describe("a card's title and a new comment keep their text in their own box (GA-190)", () => {
  beforeEach(() => {
    tasks = { t1: card("t1"), t2: card("t2", { title: "Another card" }), t3: card("t3"), t4: card("t4"), t5: card("t5") };
    Object.assign(h.api, {
      getTask: (id: string) => tasks[id], getTeam: () => team, listUsers: () => [], listComments: () => [], taskActivity: () => [], listRuns: () => [],
      updateTask: (id: string, p: Partial<Task>) => { tasks[id] = { ...tasks[id]!, ...p }; return tasks[id]; },
      addComment: () => ({ id: "c-new" }),
    });
  });

  it("each keystroke in the title redraws TitleInput only, not TaskPage", async () => {
    await mount(<TaskPage id="t1" />);
    expect(titleBox().value).toBe("Fix the invoice total");
    expect(rendersOf("TaskPage")).toBeGreaterThan(0);
    expect(rendersOf("TitleInput")).toBeGreaterThan(0);
    let typed = "Fix the invoice total";
    const per: Record<string, number>[] = [];
    for (const c of " in EUR") {
      resetCounts();
      typed += c;
      await typeInto(titleBox(), typed);
      per.push({ TitleInput: rendersOf("TitleInput"), TaskPage: rendersOf("TaskPage"), useLiveRuns: h.calls.TaskPage, EditableMarkdown: rendersOf("EditableMarkdown"),
        CommentComposer: rendersOf("CommentComposer") });
      expect(titleBox().value).toBe(typed);
    }
    expect(per).toEqual(Array.from({ length: 7 }, () => ({ TitleInput: 1, TaskPage: 0, useLiveRuns: 0, EditableMarkdown: 0, CommentComposer: 0 })));
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toEqual([]);
  });

  it("Enter saves the title (trimmed) once; leaving the box with no change saves nothing", async () => {
    await mount(<TaskPage id="t1" />);
    await typeInto(titleBox(), "  Fix the invoice total in EUR ");
    await key(titleBox(), "Enter");
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toEqual([["updateTask", ["t1", { title: "Fix the invoice total in EUR" }]]]);
    // the card comes back with its new title (the page reloads it after the write): the box shows it, and leaving the
    // box again without a change saves nothing
    await show(<TaskPage id="t1" />);
    expect(titleBox().value).toBe("Fix the invoice total in EUR");
    await act(async () => { titleBox().blur(); });
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toHaveLength(1);
  });

  it("Escape puts the saved title back and saves nothing", async () => {
    await mount(<TaskPage id="t1" />);
    await typeInto(titleBox(), "Something else");
    await key(titleBox(), "Escape");
    await flush();
    expect(titleBox().value).toBe("Fix the invoice total");
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toEqual([]);
    // the next edit saves again as usual
    await typeInto(titleBox(), "Fix the total");
    await act(async () => { titleBox().blur(); });
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toEqual([["updateTask", ["t1", { title: "Fix the total" }]]]);
  });

  it("an empty title isn't saved: the box shows the saved title again", async () => {
    await mount(<TaskPage id="t1" />);
    await typeInto(titleBox(), "   ");
    await key(titleBox(), "Enter");
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toEqual([]);
    expect(titleBox().value).toBe("Fix the invoice total");
  });

  it("an archived card's title isn't saved", async () => {
    tasks.t1 = card("t1", { archivedAt: 5, stateId: "s-done", stateName: "Done", stateCategory: "done" } as Partial<Task>);
    await mount(<TaskPage id="t1" />);
    expect(titleBox().hasAttribute("readOnly")).toBe(true);
    await typeInto(titleBox(), "Changed");
    await act(async () => { titleBox().blur(); });
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "updateTask")).toEqual([]);
  });

  it("each keystroke in a new comment redraws CommentComposer only, not TaskPage; Comment posts it and empties the box", async () => {
    await mount(<TaskPage id="t2" />);
    expect(commentBox()!.value).toBe("");
    expect(commentButton().hasAttribute("disabled")).toBe(true);
    let typed = "";
    const per: Record<string, number>[] = [];
    for (const c of "Looks good") {
      resetCounts();
      typed += c;
      await typeComment(typed);
      per.push({ CommentComposer: rendersOf("CommentComposer"), TaskPage: rendersOf("TaskPage"), useLiveRuns: h.calls.TaskPage, TitleInput: rendersOf("TitleInput") });
    }
    expect(per).toEqual(Array.from({ length: 10 }, () => ({ CommentComposer: 1, TaskPage: 0, useLiveRuns: 0, TitleInput: 0 })));
    expect(commentButton().hasAttribute("disabled")).toBe(false);

    await click(commentButton());
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "addComment")).toEqual([["addComment", ["t2", "Looks good"]]]);
    expect(commentBox()!.value).toBe("");
    expect(commentButton().hasAttribute("disabled")).toBe(true);
    // posted: the draft is gone, also after leaving and coming back
    await show(<div />);
    await show(<TaskPage id="t2" />);
    expect(commentBox()!.value).toBe("");
  });

  it("Ctrl+Enter (the editor's onSave) posts the comment too", async () => {
    await mount(<TaskPage id="t5" />);
    await typeComment("Via the keys");
    await act(async () => { await (commentBox()!.onSave as (v: string) => Promise<void>)("Via the keys"); });
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "addComment")).toEqual([["addComment", ["t5", "Via the keys"]]]);
    expect(commentBox()!.value).toBe("");
  });

  it("a comment that couldn't be posted stays in the box", async () => {
    h.api.addComment = () => { throw new Error("offline"); };
    await mount(<TaskPage id="t4" />);
    await typeComment("Not lost");
    await click(commentButton());
    await flush();
    expect(byClass(container, "error-banner")[0]?.textContent).toContain("offline");
    expect(commentButton().hasAttribute("disabled")).toBe(false);
    // the editor kept its text (it wasn't remade empty) and the draft is kept for the card
    await show(<div />);
    await show(<TaskPage id="t4" />);
    expect(commentBox()!.value).toBe("Not lost");
  });

  it("a comment being written survives the Activity tab and back, and leaving the card's page and back; another card has its own", async () => {
    await mount(<TaskPage id="t3" />);
    await typeComment("Half a thought");
    await click(tabNamed("Activity"));
    expect(commentBox()).toBeDefined(); // the last props it had
    expect(buttonsNamed(container, "Comment")).toHaveLength(0);
    h.editors.clear();
    await click(tabNamed("Comments"));
    expect(commentBox()!.value).toBe("Half a thought");
    expect(commentButton().hasAttribute("disabled")).toBe(false);

    // another card: its own (empty) box
    h.editors.clear();
    await show(<TaskPage id="t1" />);
    expect(commentBox()!.value).toBe("");
    await show(<div />);
    h.editors.clear();
    await show(<TaskPage id="t3" />);
    expect(commentBox()!.value).toBe("Half a thought");
    expect(commentButton().hasAttribute("disabled")).toBe(false);
    await click(commentButton());
    await flush();
    expect(h.apiCalls.filter(([k]) => k === "addComment")).toEqual([["addComment", ["t3", "Half a thought"]]]);
    // emptied by hand: nothing left for the card
    await typeComment("x");
    await typeComment("");
    await show(<div />);
    await show(<TaskPage id="t3" />);
    expect(commentBox()!.value).toBe("");
  });
});

// ---- the Editor: the cursor ----
const FOLDER = "/srv/test-root/Herd/kade-portal";
const files: Record<string, string> = { "README.md": "# Kade portal\n\nLine three\n", "app.ts": "export const one = 1;\n" };
const folderInfo: EditorFolder = { id: FOLDER, path: FOLDER, name: "kade-portal", kind: "root", projectId: null, taskId: null, readOnly: false, readOnlyWhy: null } as EditorFolder;
type View = { state: EditorState; dispatch: (spec: TransactionSpec) => void };
const view = () => h.views.at(-1) as View;
const statusText = () => byLabel(container, "Status bar")[0]?.textContent ?? "";
const cursorItem = () => container.all().find((n) => n.nodeName === "BUTTON" && (n.getAttribute("title") ?? "").startsWith("Go to line"));

describe("a cursor move or a keystroke in the Editor redraws what shows the cursor, not the page (GA-190)", () => {
  beforeEach(() => {
    Object.assign(h.api, {
      editorFolders: () => [folderInfo],
      editorList: (_f: string, dir: string): DirEntry[] => (dir ? [] : Object.keys(files).map((p) => ({ name: p, path: p, dir: false, size: files[p]!.length, ignored: false, git: null }))),
      editorRead: (_f: string, path: string): OpenedFile => ({ path, text: files[path]!, binary: false, tooLarge: false, readOnly: false, readOnlyWhy: null, crlf: false, bom: false, version: "v1" }),
      editorGit: () => ({ branch: "main", files: {}, uncommitted: 0 }),
      editorVersions: (_f: string, paths: string[]) => paths.map(() => "v1"),
      editorApps: () => [],
    });
  });

  it("a cursor move: the status bar's line and column and Open in redraw; FolderEditor doesn't", async () => {
    await mount(<EditorPage id={FOLDER} youId="u1" />);
    const row = container.all().find((n) => n.getAttribute("role") === "treeitem" && n.getAttribute("data-path") === "README.md")!;
    await click(row);
    await quiet();
    expect(view().state.doc.toString()).toBe(files["README.md"]);
    expect(cursorItem()?.textContent).toBe("Ln 1, Col 1");
    expect(rendersOf("FolderEditor")).toBeGreaterThan(0);

    const moves: Record<string, unknown>[] = [];
    for (const at of [3, 7, 16, 20]) {
      resetCounts();
      await act(async () => { view().dispatch({ selection: { anchor: at } }); });
      moves.push({ FolderEditor: rendersOf("FolderEditor"), useEditorApps: h.calls.FolderEditor, CursorItem: rendersOf("CursorItem"),
        OpenInAtCursor: rendersOf("OpenInAtCursor"), FileBar: rendersOf("FileBar"), FileTree: rendersOf("FileTree"), openIn: [...h.openIn] });
    }
    // a line of README.md: "# Kade portal" (1), "" (2), "Line three" (3)
    // Open in, in the top bar, at the cursor's line (the open file's bar has none: the file isn't read-only)
    const moved = (line: number) => ({ FolderEditor: 0, useEditorApps: 0, CursorItem: 1, OpenInAtCursor: 1, FileBar: 0, FileTree: 0, openIn: [line] });
    expect(moves).toEqual([moved(1), moved(1), moved(3), moved(3)]);
    expect(cursorItem()?.textContent).toBe("Ln 3, Col 6");

    // a selection: the status bar says how much
    resetCounts();
    await act(async () => { view().dispatch({ selection: { anchor: 16, head: 26 } }); });
    expect(rendersOf("FolderEditor")).toBe(0);
    expect(cursorItem()?.textContent).toBe("Ln 4, Col 1 (10 selected)");
  });

  it("keystrokes after the first (which marks the tab not saved) redraw the cursor only, not FolderEditor", async () => {
    await mount(<EditorPage id={FOLDER} youId="u1" />);
    await click(container.all().find((n) => n.getAttribute("role") === "treeitem" && n.getAttribute("data-path") === "app.ts")!);
    await quiet();
    const typeAtEnd = async (text: string) => {
      const v = view();
      await act(async () => { v.dispatch({ changes: { from: v.state.doc.length, insert: text }, selection: { anchor: v.state.doc.length + text.length }, userEvent: "input.type" }); });
    };
    resetCounts();
    await typeAtEnd("x");
    // the first keystroke: the tab gets its dot, so the page draws once
    expect(rendersOf("FolderEditor")).toBe(1);
    expect(statusText()).toContain("Not saved");
    const per: Record<string, number>[] = [];
    for (const c of "yzabcdefg") {
      resetCounts();
      await typeAtEnd(c);
      per.push({ FolderEditor: rendersOf("FolderEditor"), useEditorApps: h.calls.FolderEditor, CursorItem: rendersOf("CursorItem"), OpenInAtCursor: rendersOf("OpenInAtCursor") });
    }
    expect(per).toEqual(Array.from({ length: 9 }, () => ({ FolderEditor: 0, useEditorApps: 0, CursorItem: 1, OpenInAtCursor: 1 })));
    expect(view().state.doc.toString()).toBe("export const one = 1;\nxyzabcdefg");
    expect(cursorItem()?.textContent).toBe("Ln 2, Col 11");
  });

  it("a read-only file: both Open in (the top bar's and the file bar's) follow the cursor's line; FolderEditor and FileBar don't redraw", async () => {
    h.api.editorRead = (_f: unknown, path: unknown): OpenedFile => ({ path: path as string, text: files[path as string]!, binary: false, tooLarge: false, readOnly: true,
      readOnlyWhy: "Over 1 MB", crlf: false, bom: false, version: "v1" });
    await mount(<EditorPage id={FOLDER} youId="u1" />);
    await click(container.all().find((n) => n.getAttribute("role") === "treeitem" && n.getAttribute("data-path") === "README.md")!);
    await quiet();
    expect(byClass(container, "ed-bar").some((n) => cls(n).includes("file"))).toBe(true);
    resetCounts();
    await act(async () => { view().dispatch({ selection: { anchor: 18 } }); });
    expect({ FolderEditor: rendersOf("FolderEditor"), useEditorApps: h.calls.FolderEditor, FileBar: rendersOf("FileBar"), CursorItem: rendersOf("CursorItem"),
      OpenInAtCursor: rendersOf("OpenInAtCursor"), openIn: [...h.openIn] })
      .toEqual({ FolderEditor: 0, useEditorApps: 0, FileBar: 0, CursorItem: 1, OpenInAtCursor: 2, openIn: [3, 3] });
    expect(cursorItem()?.textContent).toBe("Ln 3, Col 4");
  });
});
