// GA-176 QA: while the Team Lead writes, a streamed piece redraws only the answer being written, and a live run's clock and
// new events don't parse earlier events' Markdown again. React itself renders here (react-dom/client, so React.memo and
// useMemo really skip work), into a small stand-in DOM: there is no DOM library in the project. react-markdown is wrapped to
// count each parse by its text. useChat is a store the test changes; the api is a fake that records its calls; the text
// box (CodeMirror) and the drop zone are left out. Clicks and scrolls go through React's own event listeners.
import { act, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChatCli, ChatMessage, LiveRun, Member, PendingAction, QueuedMessage, Run, SeqEvent, Task, Team } from "../types";

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
  scrollTop = 0;
  scrollHeight = 0;
  clientHeight = 0;
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
const cls = (n: FakeNode) => (n.getAttribute("class") ?? "").split(" ");
const byClass = (root: FakeNode, c: string) => root.all().filter((n) => n.nodeType === 1 && cls(n).includes(c));
const byLabel = (root: FakeNode, label: string) => root.all().filter((n) => n.getAttribute("aria-label") === label);
const buttonsNamed = (root: FakeNode, text: string) => root.all().filter((n) => n.nodeName === "BUTTON" && n.textContent.trim() === text);
const click = async (n: FakeNode) => { await act(async () => { fire(n, "click"); }); };

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
}
/** The chat as useChat now gives it. */
async function chat(over: Record<string, unknown>) {
  await act(async () => { h.chat = { ...h.chat, ...over }; h.listeners.forEach((f) => f()); });
}
const parsed = (t: string) => h.parses.get(t) ?? 0;

beforeEach(() => {
  h.parses.clear();
  h.calls.length = 0;
  for (const k of Object.keys(h.api)) delete h.api[k];
  h.chat = { messages: [], draft: "", tool: null, working: false, queue: [], pending: [], error: null, setWorking: () => {}, reload: () => {} };
});
afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
  vi.useRealTimers();
});

// ---- the chat ----
const T0 = 1_760_000_000_000;
const MIN = 60_000;
const lead = { actorId: "lead", name: "Team Lead", kind: "agent", roleKey: "lead", handle: "lead", status: "active", isLead: false,
  allowedTools: [], chatEnabled: true } as Member;
const CLIS: ChatCli[] = [{ id: "claude_code", name: "Claude Code", kind: "claude_code" }, { id: "cc2", name: "Claude Code 2", kind: "claude_code" }];
const message = (i: number, role: string, body: string, more: Partial<ChatMessage> = {}): ChatMessage => ({
  id: `m${i}`, threadId: "c1", role, authorId: role === "user" ? "you" : "lead", authorName: role === "user" ? "You" : "Team Lead", bodyMd: body,
  runId: role === "user" ? null : `r${Math.floor(i / 2)}`, toolName: null, tool: null, createdAt: T0 + i * 10 * MIN, ...more,
});
const answerText = (i: number) => `Answer ${i}: **done**, see [the board](https://example.com/${i}) and \`task ${i}\`.`;
/** 200 messages: yours and the Team Lead's answers in turn, your question last (the Team Lead is answering it). */
const conversation = () => Array.from({ length: 200 }, (_, i) => i % 2 === 0 ? message(i, "agent", answerText(i)) : message(i, "user", `Question ${i}`));
const pieces = (n: number) => Array.from({ length: n }, (_, i) => `word${i} `);
const action = (id: string, runId: string): PendingAction => ({ id, threadId: "c1", runId, actorId: "lead", tool: "memory_write", args: {},
  outsideTool: "mcp__otus__search", status: "pending", createdAt: T0 });
const queued = (id: string, held: boolean): QueuedMessage => ({ id, threadId: "c1", bodyMd: `Queued ${id}`, createdAt: T0, updatedAt: T0, held });
const draftOf = () => byClass(container, "draft")[0];
const workingLine = () => byClass(container, "chat-working")[0]?.textContent;
const groupOf = (n: FakeNode) => { let x: FakeNode | null = n; while (x && !cls(x).includes("chat-group")) x = x.parentNode; return x; };

describe("a streamed answer in a chat of 200 messages (GA-176)", () => {
  it("parses each earlier message's Markdown once over 50 draft updates, and each piece of the draft once", async () => {
    const messages = conversation();
    h.chat = { ...h.chat, messages, working: true };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const answers = messages.filter((m) => m.role === "agent").map((m) => m.bodyMd!);
    expect(answers).toHaveLength(100);
    for (const t of answers) expect(parsed(t)).toBe(1);

    let draft = "";
    for (const p of pieces(50)) { draft += p; await chat({ draft }); }

    // Each earlier message: parsed once, when the chat opened; not once per update (50 times before GA-176).
    expect(answers.map(parsed)).toEqual(answers.map(() => 1));
    // The draft is drawn again for each piece: 50 updates, 50 parses, one per text it has been.
    const drafts = [...h.parses].filter(([t]) => t.startsWith("word0"));
    expect(drafts).toHaveLength(50);
    expect(drafts.every(([, n]) => n === 1)).toBe(true);
    expect([...h.parses.values()].reduce((a, b) => a + b, 0)).toBe(150);
  });

  it("does not parse earlier messages again when a new message arrives, a tool runs or a call starts waiting", async () => {
    const messages = conversation();
    h.chat = { ...h.chat, messages, working: true };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const total = () => [...h.parses.values()].reduce((a, b) => a + b, 0);
    expect(total()).toBe(100);
    await chat({ tool: "mcp__gizai__create_task" });
    await chat({ pending: [action("a1", "r0")] });
    const block = message(200, "agent", "The first block of this answer.", { runId: "r100" });
    await chat({ messages: [...messages, block], draft: "" });
    expect(parsed(block.bodyMd!)).toBe(1);
    expect(total()).toBe(101);
  });

  it("shows each new piece with its caret, says Thinking, Writing or Using a tool, and the final message replaces the draft", async () => {
    const messages = conversation();
    h.chat = { ...h.chat, messages, working: true };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    expect(draftOf()).toBeUndefined();
    expect(workingLine()).toBe("Thinking…");
    // Before the answer has a message of its own, it is written in a group of its own under your question.
    expect(groupOf(byClass(container, "chat-working")[0])).not.toBe(groupOf(byClass(container, "chat-msg").filter((n) => cls(n).includes("agent"))[99]));

    let draft = "";
    for (const p of pieces(5)) {
      draft += p;
      await chat({ draft });
      const d = draftOf();
      expect(d.textContent).toBe(draft.trim());
      expect(byClass(d, "caret")).toHaveLength(1);
      expect(d.lastChild).toBe(byClass(d, "caret")[0]);
      expect(workingLine()).toBe("Writing…");
    }
    await chat({ tool: "mcp__gizai__create_task" });
    expect(workingLine()).toBe("Using create task…");
    expect(draftOf().textContent).toBe(draft.trim());

    const final = message(200, "agent", draft.trim(), { runId: "r100" });
    await chat({ messages: [...messages, final], draft: "", tool: null, working: false });
    expect(draftOf()).toBeUndefined();
    expect(byClass(container, "caret")).toHaveLength(0);
    expect(workingLine()).toBeUndefined();
    const shown = byClass(container, "chat-msg").filter((n) => n.textContent === draft.trim());
    expect(shown).toHaveLength(1);
    expect(cls(shown[0])).not.toContain("draft");
    expect(groupOf(shown[0])).toBe(byClass(container, "chat-group").at(-1));
  });

  it("keeps the answer's earlier block and its tool cards in the group being written; a card stays open while text streams", async () => {
    const messages = conversation();
    const block = message(200, "agent", "Let me look that up.", { runId: "r100" });
    const tool = message(201, "tool", "", { runId: "r100", toolName: "mcp__gizai__list_tasks", tool: { id: "t1", input: { project: "GA" } } });
    h.chat = { ...h.chat, messages: [...messages, block, tool], working: true, tool: "mcp__gizai__list_tasks" };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const card = () => byClass(container, "tool-card")[0];
    expect(cls(card())).toContain("running");
    expect(groupOf(card())).toBe(byClass(container, "chat-group").at(-1));

    await click(byLabel(card(), "Show details")[0]);
    expect(byLabel(card(), "Hide details")[0].getAttribute("aria-expanded")).toBe("true");
    expect(byClass(card(), "tc-detail")[0].textContent).toContain('"project": "GA"');

    let draft = "";
    for (const p of pieces(5)) { draft += p; await chat({ draft }); }
    expect(groupOf(draftOf())).toBe(groupOf(card()));
    expect(byClass(card(), "tc-detail")).toHaveLength(1);
    expect(parsed(block.bodyMd!)).toBe(1);

    // The tool's result arrives as a new copy of its message: the card shows it.
    const done = { ...tool, tool: { ...tool.tool, result: { tasks: 3 } } };
    await chat({ messages: [...messages, block, done], tool: null });
    expect(cls(card())).toContain("ok");
    expect(byClass(card(), "tc-detail")[0].textContent).toContain('"tasks": 3');

    await click(byLabel(card(), "Hide details")[0]);
    expect(byClass(card(), "tc-detail")).toHaveLength(0);
  });

  it("shows the calls waiting for your OK under the answer that tried them, through the stream; Approve and Discard call the api", async () => {
    const messages = conversation();
    h.chat = { ...h.chat, messages, working: true, pending: [action("a1", "r10"), action("a2", "r10"), { ...action("x", "r10"), threadId: "other" }] };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const box = () => byLabel(container, "Waiting for your OK");
    expect(box()).toHaveLength(1);
    const answer = byClass(container, "chat-msg").find((n) => n.textContent.startsWith("Answer 20:"))!;
    expect(groupOf(box()[0])).toBe(groupOf(answer));
    expect(byClass(box()[0], "pa-card")).toHaveLength(2);

    let draft = "";
    for (const p of pieces(5)) { draft += p; await chat({ draft }); }
    expect(groupOf(box()[0])).toBe(groupOf(answer));

    // A call of the answer being written: at the end until that answer has a message, then under it.
    await chat({ pending: [action("a1", "r10"), action("a2", "r10"), action("a3", "r100")] });
    expect(box()).toHaveLength(2);
    const block = message(200, "agent", "Saved it.", { runId: "r100" });
    await chat({ messages: [...messages, block] });
    expect(groupOf(box()[1])).toBe(groupOf(byClass(container, "chat-msg").find((n) => n.textContent === "Saved it.")!));

    await click(buttonsNamed(box()[0], "Approve")[0]);
    expect(h.calls.filter(([k]) => k === "approvePendingAction")).toEqual([["approvePendingAction", ["a1"]]]);
    await click(buttonsNamed(box()[1], "Discard")[0]);
    expect(h.calls.filter(([k]) => k === "discardPendingAction")).toEqual([["discardPendingAction", ["a3"]]]);
  });

  it("shows queued messages under the answer through the stream, with Edit, Remove and, for a held one, Send now", async () => {
    const messages = conversation();
    h.chat = { ...h.chat, messages, working: true, queue: [queued("q1", false), queued("q2", true)] };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    let draft = "";
    for (const p of pieces(5)) { draft += p; await chat({ draft }); }
    const list = byLabel(container, "Queued messages")[0];
    expect(byClass(list, "queued").map((n) => n.textContent)).toEqual([
      "Queued q1Queued: goes when this answer is done", "Queued q2Waiting: the answer before it didn't finish"]);
    expect(byLabel(list, "Edit")).toHaveLength(2);
    // The queue comes after the answer being written.
    const all = container.all();
    expect(all.indexOf(list)).toBeGreaterThan(all.indexOf(draftOf()));

    await click(byLabel(byClass(list, "queued")[0], "Remove")[0]);
    expect(h.calls.filter(([k]) => k === "removeQueuedChat")).toEqual([["removeQueuedChat", ["q1"]]]);
    await click(buttonsNamed(list, "Send now")[0]);
    expect(h.calls.filter(([k]) => k === "sendChatQueue")).toEqual([["sendChatQueue", ["c1"]]]);
  });

  it("offers Answer on another Claude Code under a usage-limit note that is the last word, once the CLIs are known", async () => {
    let answerClis: (c: ChatCli[]) => void = () => {};
    h.api.chatClis = () => new Promise((r) => { answerClis = r; });
    const note = message(200, "system", "Claude Code hit its usage limit.", { runId: null, meta: { kind: "limit", cli: "claude_code", cliName: "Claude Code" } });
    const messages = [...conversation(), note];
    h.chat = { ...h.chat, messages };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    expect(byClass(container, "chat-note-actions")).toHaveLength(0);

    await act(async () => { answerClis(CLIS); });
    const actions = () => byClass(container, "chat-note-actions");
    expect(actions()).toHaveLength(1);
    expect(actions()[0].textContent).toBe("Answer on Claude Code 2");

    await click(buttonsNamed(actions()[0], "Answer on Claude Code 2")[0]);
    expect(h.calls.filter(([k]) => k === "answerChatOn")).toEqual([["answerChatOn", ["c1", "cc2", "m200"]]]);
    expect(actions()[0].textContent).toBe("Starting…");
    expect(actions()[0].all().find((n) => n.nodeName === "BUTTON")!.getAttribute("aria-busy")).toBe("true");

    // The answer starts: the button goes.
    await chat({ working: true });
    expect(actions()).toHaveLength(0);
    await chat({ working: false });
    expect(actions()).toHaveLength(1);
    // A newer message: the note isn't the last word any more.
    await chat({ messages: [...messages, message(201, "user", "Try again")] });
    expect(actions()).toHaveLength(0);
  });

  it("follows the answer while you are at the bottom, and stays put when you scrolled up", async () => {
    const messages = conversation();
    h.chat = { ...h.chat, messages, working: true };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    const s = byClass(container, "chat-scroll")[0];
    s.clientHeight = 600;
    s.scrollHeight = 20_000;
    await chat({ draft: "word0 " });
    expect(s.scrollTop).toBe(20_000);

    // You scroll up to read.
    s.scrollTop = 5_000;
    await act(async () => { fire(s, "scroll"); });
    s.scrollHeight = 20_400;
    await chat({ draft: "word0 word1 " });
    await chat({ draft: "word0 word1 word2 " });
    expect(s.scrollTop).toBe(5_000);

    // Back at the bottom: it follows again.
    s.scrollTop = 20_400 - 600 - 10;
    await act(async () => { fire(s, "scroll"); });
    s.scrollHeight = 20_800;
    await chat({ draft: "word0 word1 word2 word3 " });
    expect(s.scrollTop).toBe(20_800);
  });

  it("still opens an earlier answer's links while the answer streams: a web link in the browser, a card's link in Gizai", async () => {
    // MarkdownView's link and image parts are made once since GA-176; they act as before in an answer that isn't parsed again.
    const messages = conversation();
    messages[10] = message(10, "agent", "See [GA-12 - Fix the login](gizai:task/GA-12) and ![a diagram](https://example.com/d.png).");
    h.chat = { ...h.chat, messages, working: true };
    await mount(<ChatThread threadId="c1" agent={lead} />);
    let draft = "";
    for (const p of pieces(5)) { draft += p; await chat({ draft }); }
    const answer = (start: string) => byClass(container, "chat-msg").find((n) => n.textContent.startsWith(start))!;

    const link = answer("Answer 20:").all().find((n) => n.nodeName === "A")!;
    expect(link.getAttribute("href")).toBe("https://example.com/20");
    expect(link.getAttribute("title")).toBe("https://example.com/20");
    await click(link);
    expect(h.calls.filter(([k]) => k === "openUrl")).toEqual([["openUrl", ["https://example.com/20"]]]);

    const chip = byClass(answer("See "), "item-chip")[0];
    expect(chip.textContent).toBe("GA-12 - Fix the login");
    expect(chip.getAttribute("href")).toBe("gizai:task/GA-12");
    await click(chip);
    // openItem looks the card up by its identifier before it opens its page.
    expect(h.calls.filter(([k]) => k === "itemId")).toEqual([["itemId", ["task", "GA-12"]]]);
    expect(h.calls.filter(([k]) => k === "openUrl")).toHaveLength(1);
    // A remote image shows its alt text, as the content security policy blocks it.
    expect(answer("See ").textContent).toBe("See GA-12 - Fix the login and [image: a diagram].");
    expect(parsed(messages[10].bodyMd!)).toBe(1);
  });
});

// ---- a live run on the card page ----
const team = { id: "team", name: "Software", members: [], states: [], labels: [] } as unknown as Team;
const task = { id: "t1", identifier: "GA-1", title: "A card", stateId: "doing", assigneeId: null, hold: null } as unknown as Task;
const eventText = (i: number) => `Step ${i}: read **src/a${i}.ts** and [its docs](https://example.com/${i}).`;
const textEvent = (i: number): SeqEvent => ({ seq: i, event: { kind: "text", text: eventText(i) } });

describe("a live run's log on the card page (GA-176)", () => {
  it("parses no earlier event again on each second or each new event, while the clock and the log stay current", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    vi.setSystemTime(T0);
    let send: (m: { runId: string; seq: number; event: SeqEvent["event"] }) => void = () => {};
    h.api.onRunEvent = (f) => { send = f as typeof send; return Promise.resolve(() => {}); };
    h.api.runEvents = () => Promise.resolve(Array.from({ length: 200 }, (_, i) => textEvent(i + 1)));
    h.live = [{ runId: "run1", taskId: "t1", agentId: "be" }];
    h.runs = [{ id: "run1", agentId: "be", agentName: "Backend Agent", taskId: "t1", trigger: "manual", status: "running", createdAt: T0, startedAt: T0,
      costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "" } as Run];
    await mount(<RunPanel task={task} team={team} />);
    const texts = Array.from({ length: 200 }, (_, i) => eventText(i + 1));
    expect(texts.map(parsed)).toEqual(texts.map(() => 1));
    const clock = () => byClass(container, "run-head")[0].textContent;
    expect(clock()).toContain("0s");

    for (let i = 0; i < 10; i++) await act(async () => { vi.advanceTimersByTime(1000); });
    expect(clock()).toContain("10s");
    expect(texts.map(parsed)).toEqual(texts.map(() => 1));

    for (let i = 201; i <= 210; i++) await act(async () => { send({ runId: "run1", ...textEvent(i) }); });
    const stream = byClass(container, "run-stream")[0];
    expect(stream.textContent).toContain("Step 210: read src/a210.ts and its docs.");
    const all = Array.from({ length: 210 }, (_, i) => eventText(i + 1));
    expect(all.map(parsed)).toEqual(all.map(() => 1));
    // Another run's event changes nothing.
    await act(async () => { send({ runId: "run2", ...textEvent(211) }); });
    expect(parsed(eventText(211))).toBe(0);
    expect([...h.parses.values()].reduce((a, b) => a + b, 0)).toBe(210);
  });

  it("stops the clock when the run ends: the live panel goes and no tick is left", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval", "Date"] });
    vi.setSystemTime(T0);
    h.api.onRunEvent = () => Promise.resolve(() => {});
    h.api.runEvents = () => Promise.resolve([textEvent(1)]);
    const running = { id: "run1", agentId: "be", agentName: "Backend Agent", taskId: "t1", trigger: "manual", status: "running", createdAt: T0, startedAt: T0,
      costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "" } as Run;
    h.live = [{ runId: "run1", taskId: "t1", agentId: "be" }];
    h.runs = [running];
    await mount(<RunPanel task={task} team={team} />);
    expect(vi.getTimerCount()).toBe(1);
    await act(async () => { vi.advanceTimersByTime(3000); });
    expect(byClass(container, "run-head")[0].textContent).toContain("3s");

    h.live = [];
    h.runs = [{ ...running, status: "succeeded", outcome: "qa_pass", endedAt: T0 + 3000 }];
    await act(async () => { root!.render(<RunPanel task={{ ...task }} team={team} />); });
    expect(byClass(container, "pulse")).toHaveLength(0);
    expect(vi.getTimerCount()).toBe(0);
  });
});

// ---- a finished run's output: the same memoised log, filled once its events have loaded ----
const agentTeam = { ...team, members: [{ actorId: "be", name: "Backend Agent", kind: "agent", status: "active", roleKey: "backend" }] } as unknown as Team;
const finished = { id: "run9", agentId: "be", agentName: "Backend Agent", taskId: "t1", roleKey: "backend", trigger: "manual", status: "succeeded",
  outcome: "ready_for_testing", summaryMd: "All done.", createdAt: T0, startedAt: T0, endedAt: T0 + MIN, costUsdMicros: 0, inputTokens: 0,
  outputTokens: 0, logPath: "" } as Run;
const output = (): SeqEvent[] => [
  { seq: 1, event: { kind: "init", session_id: "s1", model: "opus" } },
  ...Array.from({ length: 5 }, (_, i) => textEvent(i + 2)),
  { seq: 7, event: { kind: "tool_use", name: "Read", summary: "src/a.ts" } },
  { seq: 8, event: { kind: "result", is_error: false, subtype: "success", text: "", input_tokens: 0, output_tokens: 0, num_turns: 3 } },
];
const outputLines = ["Started · opus", ...Array.from({ length: 5 }, (_, i) => `Step ${i + 2}: read src/a${i + 2}.ts and its docs.`), "Read src/a.ts",
  "Done · 3 turns", ""];

describe("a finished run's output (GA-176)", () => {
  it("Show output on the card page fills the log when its events arrive, parses each once, and Hide output closes it", async () => {
    let load: (e: SeqEvent[]) => void = () => {};
    h.api.onRunEvent = () => Promise.resolve(() => {});
    h.api.runEvents = () => new Promise((r) => { load = r; });
    h.live = [];
    h.runs = [finished];
    await mount(<RunPanel task={task} team={agentTeam} />);
    expect(byClass(container, "run-stream")).toHaveLength(0);

    await click(buttonsNamed(container, "Show output")[0]);
    const stream = () => byClass(container, "run-stream")[0];
    expect(h.calls.filter(([k]) => k === "runEvents")).toEqual([["runEvents", ["run9"]]]);
    expect(stream().textContent).toBe("");
    // The log was drawn empty first; it still shows the events once they have loaded.
    await act(async () => { load(output()); });
    expect(stream().childNodes.map((n) => n.textContent)).toEqual(outputLines);
    const texts = Array.from({ length: 5 }, (_, i) => eventText(i + 2));
    expect(texts.map(parsed)).toEqual([1, 1, 1, 1, 1]);
    expect(parsed("All done.")).toBe(1);

    // The panel redraws (a new copy of the card): nothing is parsed again.
    await act(async () => { root!.render(<RunPanel task={{ ...task }} team={agentTeam} />); });
    expect(texts.map(parsed)).toEqual([1, 1, 1, 1, 1]);
    expect(parsed("All done.")).toBe(1);

    await click(buttonsNamed(container, "Hide output")[0]);
    expect(byClass(container, "run-stream")).toHaveLength(0);
  });

  it("Run history's Show the whole output shows every step of the run, and Hide output closes it", async () => {
    h.api.runEvents = () => Promise.resolve(output());
    await mount(<RunHistory runs={[{ ...finished, summaryMd: null }]} />);
    await click(byClass(container, "run-open")[0]);
    expect(h.calls.filter(([k]) => k === "runEvents")).toEqual([["runEvents", ["run9"]]]);
    // The last step's text is the agent's last message.
    expect(byClass(container, "run-last")[0].textContent).toBe("Step 6: read src/a6.ts and its docs.");

    await click(buttonsNamed(container, "Show the whole output (8 steps)")[0]);
    expect(byClass(container, "run-stream")[0].childNodes.map((n) => n.textContent)).toEqual(outputLines);
    // Each step parsed once in the log; the last one once more as the last message.
    expect(Array.from({ length: 5 }, (_, i) => parsed(eventText(i + 2)))).toEqual([1, 1, 1, 1, 2]);

    await click(buttonsNamed(container, "Hide output")[0]);
    expect(byClass(container, "run-stream")).toHaveLength(0);
  });
});
