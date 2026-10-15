// GA-164 QA: the first start's onboarding, step by step, against GA-164's acceptance criteria and GA-168's texts
// (design/onboarding/README.md). React itself renders OnboardingPage here (react-dom/client, so its state, effects and
// focus really run) into a small stand-in DOM, as StreamParses.test.tsx does: there is no DOM library in the project.
// Clicks, typing, ticks, the select and Enter go through React's own event listeners. The api is a fake over a small
// world (the CLIs installed and listed, Claude Code's MCP servers, Settings' servers) that records each call; the
// mountains are a marker. Step 1: the seeded name, the empty team name, Next needs both. Step 2: the CLIs found, the
// ticked ones added with save_clis, the agents' CLI picked, and without Claude Code the warning, its link and Look again.
// Step 3: Settings' import rows and rules, importing and going on without. Step 4: the summary, and Open the Team Lead
// chat sends what finish_onboarding needs. Skip setup on every step; an error stays under its step.
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import type { Cli, CliStatus, McpCandidate, McpScan } from "../types";

// ---- a stand-in DOM, as much of it as React DOM and the onboarding use ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  nodeValue: string | null = null;
  // what React DOM sets as properties on fields: their value, tick and type (a field is a text field until React says)
  value = "";
  checked = false;
  type = "";
  selectionStart = 0;
  selectionEnd = 0;
  innerHTML = "";
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
  removeChild(c: FakeNode) {
    this.childNodes.splice(this.childNodes.indexOf(c), 1);
    c.parentNode = null;
    // as in a browser: the focus leaves with the element that had it
    if (c.contains(focused())) Object.assign(doc, { activeElement: body });
    return c;
  }
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
  /** Selectors of tag names, `.class` and `[attr=value]`, comma-separated. */
  matches(sel: string) {
    return this.nodeType === 1 && sel.split(",").map((s) => s.trim()).some((s) => {
      const attr = /^\[([\w-]+)=["']?([^"'\]]*)["']?\]$/.exec(s);
      if (attr) return this.getAttribute(attr[1]) === attr[2];
      if (s.startsWith(".")) return (this.getAttribute("class") ?? "").split(" ").includes(s.slice(1));
      return this.nodeName.toLowerCase() === s.toLowerCase();
    });
  }
  closest(sel: string): FakeNode | null { for (let x: FakeNode | null = this; x; x = x.parentNode) if (x.matches(sel)) return x; return null; }
  querySelectorAll(sel: string) { return this.all().slice(1).filter((n) => n.matches(sel)); }
  querySelector(sel: string) { return this.querySelectorAll(sel)[0] ?? null; }
  scrollIntoView() {}
  focus() { Object.assign(doc, { activeElement: this }); }
  blur() { if (focused() === this) Object.assign(doc, { activeElement: body }); }
  getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { const n = new FakeNode(1, tag.toUpperCase(), this); if (n.nodeName === "INPUT") n.type = "text"; return n; }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
const focused = () => (doc as unknown as { activeElement: FakeNode }).activeElement;
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, addEventListener() {}, removeEventListener() {} };
// oninput: React DOM listens for input events only where the document has them
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win, oninput: null });
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
  return e;
}

// ---- the fakes ----
const h = vi.hoisted(() => ({
  calls: [] as [string, unknown[]][],
  /** What the fake api answers, by function name; one without an answer never settles. */
  api: {} as Record<string, (...a: unknown[]) => unknown>,
  opened: [] as string[],
}));
vi.mock("../api", () => Object.fromEntries(["appInfo", "listUsers", "getTeam", "findClis", "listClis", "saveClis", "scanClaudeCodeMcp", "listMcpServers",
  "importMcpServers", "finishOnboarding", "skipOnboarding"].map((name) => [name, (...a: unknown[]) => {
  h.calls.push([name, a]);
  const f = h.api[name];
  return f ? Promise.resolve().then(() => f(...a)) : new Promise(() => {});
}])));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (url: string) => { h.opened.push(url); return Promise.resolve(); } }));
vi.mock("../components/Band", async () => {
  const { createElement } = await import("react");
  return { Band: () => createElement("div", { className: "band" }) };
});

const { createRoot } = await import("react-dom/client");
const { createElement } = await import("react");
const { OnboardingPage } = await import("./OnboardingPage");

// ---- the world the fake api answers from ----
const claudeCode = (found: boolean): CliStatus => ({ id: "claude_code", name: "Claude Code", kind: "claude_code", command: "claude", env: [], args: "",
  path: found ? "/home/jefsev/.local/bin/claude" : null, problem: found ? null : "claude isn't on your login shell's PATH" });
const installed = (name: string, kind: Cli["kind"], program: string): Cli => ({ id: "", name, kind, command: program, env: [], args: "" });
const CODEX = installed("Codex", "codex", "/home/jefsev/.npm-global/bin/codex");
const GEMINI = installed("Gemini", "gemini", "/home/jefsev/.npm-global/bin/gemini");
const server = (key: string, name: string, extra: Partial<McpCandidate> = {}): McpCandidate => ({ key, name, account: "Claude Code", scope: "user",
  folder: null, transport: "http", command: "", args: [], url: `https://mcp.${name}.com/mcp`, envNames: [], headerNames: [], already: false, clash: null, ...extra });
const SCAN: McpScan = { servers: [
  server("user:figma", "figma"),
  server("user:github", "github", { transport: "stdio", command: "npx", args: ["-y", "@modelcontextprotocol/server-github"], url: "", envNames: ["GITHUB_PERSONAL_ACCESS_TOKEN"] }),
  server("local:linear", "linear", { scope: "local", folder: "/home/jefsev/code/kade", transport: "sse", url: "https://mcp.linear.app/sse", clash: "linear" }),
], problems: [] };
const AGENTS = ["Team Lead", "Backend Agent", "Frontend Agent", "QA Agent", "DevOps Agent"];

let world: { claude: boolean; listed: CliStatus[]; installed: Cli[]; scan: McpScan; servers: string[]; chat: string | null };
beforeEach(() => {
  world = { claude: true, listed: [], installed: [CODEX, GEMINI], scan: SCAN, servers: ["linear"], chat: "thread-1" };
  h.calls = [];
  h.opened = [];
  h.api = {
    appInfo: () => ({ version: "0.0.0", data_dir: "/data", you_id: "you", selftest: false }),
    listUsers: () => [{ id: "p0", name: "Someone else", handle: "else", openTasks: 0 }, { id: "you", name: "Jefsev", handle: "jefsev", openTasks: 0 }],
    getTeam: () => ({ id: "t1", name: "Team", states: [], labels: [], members: [
      ...AGENTS.map((name, i) => ({ actorId: `a${i}`, name, kind: "agent" })), { actorId: "you", name: "Jefsev", kind: "user" }] }),
    // find_clis: the known CLIs installed that aren't listed yet, each with an empty id
    findClis: () => world.installed.filter((c) => !world.listed.some((l) => l.name.toLowerCase() === c.name.toLowerCase())).map((c) => ({ ...c, id: "" })),
    listClis: () => [claudeCode(world.claude), ...world.listed],
    // save_clis takes the whole list (without the built-in Claude Code) and gives each new one an id
    saveClis: (clis) => {
      world.listed = (clis as Cli[]).map((c) => ({ ...c, id: c.id || `cli-${c.name.toLowerCase()}`, path: c.command, problem: null }));
      return [claudeCode(world.claude), ...world.listed];
    },
    scanClaudeCodeMcp: () => world.scan,
    listMcpServers: () => world.servers.map((name) => ({ id: name, name })),
    importMcpServers: (picks) => {
      const added = (picks as { key: string; name: string }[]).map((p) => p.name);
      world.servers.push(...added);
      return added.map((name) => ({ id: name, name }));
    },
    finishOnboarding: () => world.chat,
    skipOnboarding: () => undefined,
  };
});

// ---- helpers ----
// One turn of the event loop for React and the fake api. A timer tick is about 16 ms on Windows, so the helpers wait for
// what they wait for, at most 1.5 s, and the check after them then fails.
const tick = () => act(async () => { await new Promise((r) => setTimeout(r, 0)); });
async function until(done: () => boolean) {
  const end = Date.now() + 1500;
  await tick();
  while (!done() && Date.now() < end) await tick();
}
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
let done: Mock<(to: unknown, note?: string) => void>;
afterEach(async () => {
  if (root) await act(async () => { root!.unmount(); });
  root = null;
  container?.parentNode?.removeChild(container);
  Object.assign(doc, { activeElement: body });
});

const squash = (s: string) => s.replace(/\s+/g, " ").trim();
const text = (n: FakeNode = container) => squash(n.textContent);
const heading = () => text(container.all().find((n) => n.nodeName === "H1")!);
const eyebrow = () => text(container.all().find((n) => n.attrs.get("id") === "onb-eyebrow")!);
const byId = (id: string) => container.all().find((n) => n.attrs.get("id") === id) ?? null;
const byLabel = (label: string) => container.all().filter((n) => n.attrs.get("aria-label") === label);
const byClass = (c: string, at: FakeNode = container) => at.all().filter((n) => n.nodeType === 1 && (n.attrs.get("class") ?? "").split(" ").includes(c));
const button = (name: string) => container.all().find((n) => n.nodeName === "BUTTON" && text(n) === name) ?? null;
const off = (n: FakeNode | null) => !!n?.hasAttribute("disabled");
const calls = (name: string) => h.calls.filter(([n]) => n === name).map(([, a]) => a);
const alert = () => { const a = container.all().find((n) => n.attrs.get("role") === "alert"); return a ? text(a) : null; };

const click = async (n: FakeNode | null, then: () => boolean = () => true) => {
  expect(n, "the thing to click").not.toBeNull();
  await act(async () => { fire(n!, "click"); });
  await until(then);
};
/** Types `value` into a text field, as a person does: the field holds it, then the input event. */
const type = async (field: FakeNode | null, value: string) => {
  expect(field, "the field").not.toBeNull();
  await act(async () => { field!.value = value; fire(field!, "input"); });
  await tick();
};
/** Ticks or unticks a checkbox, as a click on it does. */
const tickBox = async (box: FakeNode, on: boolean) => {
  await act(async () => { box.checked = on; fire(box, "click"); });
  await tick();
};
const pick = async (select: FakeNode, value: string) => {
  await act(async () => { select.value = value; fire(select, "change"); });
  await tick();
};
const enter = async (at: FakeNode, then: () => boolean = () => true) => {
  await act(async () => {
    fire(at, "keydown", { key: "Enter", keyCode: 13, which: 13, repeat: false, shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, isComposing: false });
  });
  await until(then);
};
const on = (title: string) => () => heading().startsWith(title);

/** The onboarding opened, its step 1 filled in from the seed. */
async function open() {
  done = vi.fn();
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(createElement(OnboardingPage, { onDone: done })); });
  await until(() => byId("onb-you")?.value === "Jefsev" && text().includes("Your team starts with five agents") && h.calls.some(([n]) => n === "listClis"));
}
/** From step 1 to step 2, with the team's name typed. */
async function toClis(team = "YoTech") {
  await type(byId("onb-team"), team);
  await click(button("Next"), () => on("Coding CLIs")() && !text().includes("Looking for coding CLIs"));
}
async function toMcp() { await toClis(); await click(button("Next"), () => on("MCP servers")() && !text().includes("Looking for the MCP servers")); }
async function toReady() { await toMcp(); await click(button("Next"), on("You're all set")); }
/** The summary on step 4: each key with its value. */
const summary = () => Object.fromEntries(container.all().filter((n) => n.nodeName === "DT").map((dt) => [text(dt), text(dt.nextSibling!)]));
/** Step 2's rows: each CLI's text, its tick and whether that can change. */
const cliRows = () => byClass("onb-clis")[0]?.childNodes.map((li) => {
  const box = li.all().find((n) => n.nodeName === "INPUT")!;
  return { row: text(li), ticked: box.checked, locked: off(box) };
}) ?? [];
const cliBox = (name: string) => byClass("onb-clis")[0].childNodes.find((li) => text(li).startsWith(name))!.all().find((n) => n.nodeName === "INPUT")!;
const select = () => byId("onb-agents-cli")!;
const options = () => select().options.map((o) => text(o));

describe("when the onboarding opens (GA-164: on a new install, step 1)", () => {
  it("shows the top bar (the logo, the four steps, Skip setup), the mountains first and step 1, with Back off", async () => {
    await open();
    const onb = container.firstChild!;
    expect(onb.attrs.get("class")).toBe("onb");
    expect(onb.firstChild!.attrs.get("class")).toBe("band"); // the mountains, as in the app
    expect(byClass("onb-logo")[0].innerHTML).toContain('aria-label="GizAi"');
    const steps = byClass("onb-steps")[0];
    expect(steps.childNodes.map((li) => text(li))).toEqual(["1You and your team", "2Coding CLIs", "3MCP servers", "4Ready"]);
    expect(steps.all().filter((n) => n.attrs.get("aria-current") === "step").map((n) => text(n))).toEqual(["1You and your team"]);
    expect(steps.all().some((n) => n.nodeName === "BUTTON")).toBe(false); // nothing done yet
    expect(eyebrow()).toBe("Step 1 of 4");
    expect(heading()).toBe("Welcome to GizAi");
    expect(text()).toContain("Four short steps, then your Team Lead says hello. First, you and your team.");
    expect(button("Skip setup")!.attrs.get("title")).toBe("Open the board. You can set things up later in Settings.");
    expect(off(button("Back"))).toBe(true);
    expect(off(button("Next"))).toBe(false);
  });

  it("fills in your name with the seeded one (your person) and leaves Team name empty with the placeholder My team, the cursor in it", async () => {
    await open();
    expect(byId("onb-you")!.value).toBe("Jefsev");
    expect(text(byId("onb-you-hint")!)).toBe("From your login name. The Team Lead calls you by it.");
    const team = byId("onb-team")!;
    expect(team.value).toBe("");
    expect(team.attrs.get("placeholder")).toBe("My team");
    expect(text(byId("onb-team-hint")!)).toBe("Your agents work in this team.");
    expect(focused()).toBe(team);
    expect(byClass("agent-chip").map((n) => text(n))).toEqual(AGENTS.map((a) => expect.stringContaining(a)));
  });
});

describe("step 1 · You and your team: Next needs both names", () => {
  it("with Team name empty, marks it, says why under it, puts the cursor there and stays", async () => {
    await open();
    await click(button("Next"));
    const team = byId("onb-team")!;
    expect(heading()).toBe("Welcome to GizAi");
    expect(team.attrs.get("aria-invalid")).toBe("true");
    expect(team.attrs.get("aria-describedby")).toBe("onb-team-error");
    expect(text(byId("onb-team-error")!)).toBe("Type a name for your team to go on.");
    expect(focused()).toBe(team);
    expect(byId("onb-you")!.hasAttribute("aria-invalid")).toBe(false);
    // typing clears it
    await type(team, "Y");
    expect(byId("onb-team-error")).toBeNull();
    expect(team.hasAttribute("aria-invalid")).toBe(false);
    expect(text(byId("onb-team-hint")!)).toBe("Your agents work in this team.");
  });

  it("with your name emptied (spaces only count as empty), says why under Your name and puts the cursor there first", async () => {
    await open();
    await type(byId("onb-you"), "   ");
    await enter(byId("onb-you")!);
    expect(heading()).toBe("Welcome to GizAi");
    expect(text(byId("onb-you-error")!)).toBe("Type your name to go on.");
    expect(text(byId("onb-team-error")!)).toBe("Type a name for your team to go on.");
    expect(focused()).toBe(byId("onb-you"));
  });

  it("Enter in a field does what Next does: with both names it goes to step 2, and the focus goes to its heading", async () => {
    await open();
    await type(byId("onb-team"), "YoTech");
    await enter(byId("onb-team")!, on("Coding CLIs"));
    expect(heading()).toBe("Coding CLIs");
    expect(eyebrow()).toBe("Step 2 of 4");
    expect(focused()).toBe(container.all().find((n) => n.nodeName === "H1"));
    // step 1 is done: a button that opens it again
    expect(byLabel("Back to step 1: You and your team")).toHaveLength(1);
    expect(off(button("Back"))).toBe(false);
  });

  it("Back keeps what you filled in", async () => {
    await open();
    await type(byId("onb-you"), "Jef");
    await toClis("YoTech");
    await click(button("Back"), on("Welcome to GizAi"));
    expect(byId("onb-you")!.value).toBe("Jef");
    expect(byId("onb-team")!.value).toBe("YoTech");
  });
});

describe("step 2 · Coding CLIs", () => {
  it("lists the CLIs found, Claude Code first and built in, the others ticked, the ones not found and the Team Lead's line", async () => {
    await open();
    await toClis();
    expect(cliRows()).toEqual([
      { row: "Claude Code· Built in/home/jefsev/.local/bin/claude", ticked: true, locked: true },
      { row: "Codex/home/jefsev/.npm-global/bin/codex", ticked: true, locked: false },
      { row: "Gemini/home/jefsev/.npm-global/bin/gemini", ticked: true, locked: false },
    ]);
    expect(text()).toContain("Your agents work through the coding CLIs on this computer, with your own logins.");
    expect(text()).toContain("Found on this computer");
    expect(text()).toContain("Not found: OpenCode, Cursor Agent and Crush. You can add a CLI later in Settings → Coding CLIs.");
    expect(text()).toContain("The Team Lead's chat needs Claude Code, whichever CLI your agents use.");
    expect(text()).not.toContain("isn't installed");
    expect(button("Look again")).not.toBeNull();
    // the agents' CLI: the ticked ones, Claude Code first
    expect(text(container.all().find((n) => n.attrs.get("for") === "onb-agents-cli")!)).toBe("Your agents build and test with");
    expect(options()).toEqual(["Claude Code", "Codex", "Gemini"]);
    expect(text(byId("onb-agents-cli-hint")!)).toBe("The Backend, Frontend, QA and DevOps agents run on it. Each can change it in its settings.");
  });

  it("Next adds the ticked CLIs to Settings → Coding CLIs (save_clis) and leaves the unticked one out", async () => {
    await open();
    await toClis();
    await tickBox(cliBox("Gemini"), false);
    expect(options()).toEqual(["Claude Code", "Codex"]); // an unticked CLI can't be picked
    await click(button("Next"), on("MCP servers"));
    expect(calls("saveClis")).toEqual([[[CODEX]]]);
    expect(world.listed.map((c) => [c.id, c.name])).toEqual([["cli-codex", "Codex"]]);
  });

  it("Next with nothing new to add saves nothing", async () => {
    world.installed = [];
    await open();
    await toClis();
    expect(cliRows().map((r) => r.row)).toEqual(["Claude Code· Built in/home/jefsev/.local/bin/claude"]);
    expect(text()).toContain("Not found: Codex, Gemini, OpenCode, Cursor Agent and Crush.");
    await click(button("Next"), on("MCP servers"));
    expect(calls("saveClis")).toEqual([]);
  });

  it("the agents' CLI defaults to Claude Code; picking another makes finishing put the agents on its new id", async () => {
    await open();
    await toClis();
    await pick(select(), "codex");
    await click(button("Next"), on("MCP servers"));
    await click(button("Next"), on("You're all set"));
    expect(summary()["Your agents build and test with"]).toBe("Codex");
    await click(button("Open the Team Lead chat"), () => done.mock.calls.length > 0);
    expect(calls("finishOnboarding")).toEqual([[{ youName: "Jefsev", teamName: "YoTech", agentsCli: "cli-codex" }]]);
  });

  it("left on Claude Code, finishing puts the agents on claude_code", async () => {
    await open();
    await toReady();
    expect(summary()["Your agents build and test with"]).toBe("Claude Code");
    await click(button("Open the Team Lead chat"), () => done.mock.calls.length > 0);
    expect(calls("finishOnboarding")[0][0]).toMatchObject({ agentsCli: "claude_code" });
  });

  it("an error from save_clis shows under the step, and the step stays", async () => {
    await open();
    await toClis();
    h.api.saveClis = () => { throw "Couldn't save the coding CLIs: the database is read only."; };
    await click(button("Next"), () => alert() !== null);
    expect(alert()).toBe("Couldn't save the coding CLIs: the database is read only.");
    expect(heading()).toBe("Coding CLIs");
    expect(off(button("Next"))).toBe(false);
  });
});

describe("step 2 · Coding CLIs, coming back", () => {
  it("a CLI listed before the onboarding (the seed's Codex) shows as Already added, its tick locked, and stays", async () => {
    world.listed = [{ ...CODEX, id: "seed-codex", path: CODEX.command, problem: null }];
    await open();
    await toClis();
    expect(cliRows()[1]).toEqual({ row: "Codex· Already added/home/jefsev/.npm-global/bin/codex", ticked: true, locked: true });
    await tickBox(cliBox("Gemini"), false);
    await click(button("Next"), on("MCP servers"));
    expect(calls("saveClis")).toEqual([]); // nothing new
  });

  it("back on step 2 after Next added Codex, unticking it and Next takes it out of Settings again; it stays to tick again", async () => {
    world.installed = [CODEX];
    await open();
    await toClis();
    await click(button("Next"), on("MCP servers"));
    expect(world.listed.map((c) => c.id)).toEqual(["cli-codex"]);
    await click(button("Back"), on("Coding CLIs"));
    expect(cliRows()[1]).toEqual({ row: "Codex/home/jefsev/.npm-global/bin/codex", ticked: true, locked: false });
    await tickBox(cliBox("Codex"), false);
    await click(button("Next"), on("MCP servers"));
    expect(calls("saveClis").at(-1)).toEqual([[]]);
    expect(world.listed).toEqual([]);
    await click(button("Back"), on("Coding CLIs"));
    expect(cliRows().map((r) => [r.row, r.ticked])).toEqual([["Claude Code· Built in/home/jefsev/.local/bin/claude", true], ["Codex/home/jefsev/.npm-global/bin/codex", false]]);
  });
});

describe("step 2 without Claude Code", () => {
  it("says the Team Lead's chat needs it, links how to install it and looks again; once it is found the warning goes", async () => {
    world.claude = false;
    world.installed = [CODEX];
    await open();
    await toClis();
    const warning = byClass("onb-notice")[0];
    expect(warning.attrs.get("class")).toBe("onb-notice warn");
    expect(text(warning)).toContain("Claude Code isn't installed yet. The Team Lead's chat needs it.");
    const link = warning.all().find((n) => n.nodeName === "A")!;
    expect(text(link)).toBe("How to install Claude Code");
    expect(link.attrs.get("href")).toBe("https://docs.claude.com/en/docs/claude-code");
    await click(link);
    expect(h.opened).toEqual(["https://docs.claude.com/en/docs/claude-code"]);
    // the list's own Look again and the info line go: the warning says both
    expect(container.all().filter((n) => n.nodeName === "BUTTON" && text(n) === "Look again")).toHaveLength(1);
    expect(text()).not.toContain("whichever CLI your agents use");
    // no Claude Code row; the agents default to the first ticked CLI
    expect(cliRows().map((r) => r.row)).toEqual(["Codex/home/jefsev/.npm-global/bin/codex"]);
    expect(options()).toEqual(["Codex"]);

    const finds = calls("findClis").length;
    world.claude = true; // installed meanwhile
    const look = text(warning).includes("Look again") ? warning.all().find((n) => n.nodeName === "BUTTON")! : null;
    await click(look, () => !text().includes("isn't installed"));
    expect(calls("findClis").length).toBe(finds + 1);
    expect(byClass("onb-notice")).toHaveLength(0);
    expect(text()).toContain("The Team Lead's chat needs Claude Code, whichever CLI your agents use.");
    expect(cliRows()[0].row).toBe("Claude Code· Built in/home/jefsev/.local/bin/claude");
    // the Look again pressed went with the warning: the focus is on the step's heading, not lost
    expect(focused()).toBe(container.all().find((n) => n.nodeName === "H1"));
  });

  it("Next stays on, and step 4 says why there is no chat and opens the board, as GA-168's proposal says", async () => {
    world.claude = false;
    world.installed = [CODEX];
    world.chat = null;
    await open();
    await toClis();
    expect(off(button("Next"))).toBe(false);
    await click(button("Next"), on("MCP servers"));
    await click(button("Next"), on("You're all set"));
    expect(text()).toContain("Here's what you set up.");
    expect(text()).toContain("Claude Code isn't installed yet. The Team Lead's chat needs it.");
    expect(text()).not.toContain("The chat opens with this message from you:");
    expect(summary()["Your agents build and test with"]).toBe("Codex");
    await click(button("Open the board"), () => done.mock.calls.length > 0);
    expect(calls("finishOnboarding")).toEqual([[{ youName: "Jefsev", teamName: "YoTech", agentsCli: "cli-codex" }]]);
    expect(done.mock.calls).toEqual([[{ page: "board" }, "There's no Team Lead chat yet: it needs Claude Code, which isn't installed. Once it is, start one on the Chat page."]]);
  });
});

describe("step 3 · MCP servers (optional)", () => {
  it("shows the heading with the Optional badge and Settings' import rows for your Claude Code's servers, none ticked", async () => {
    await open();
    await toMcp();
    expect(heading()).toBe("MCP serversOptional");
    expect(byClass("badge", container.all().find((n) => n.nodeName === "H1")!).map((n) => text(n))).toEqual(["Optional"]);
    expect(eyebrow()).toBe("Step 3 of 4");
    expect(text()).toContain("Bring the MCP servers your Claude Code has. Each stays off until you switch it on for an agent.");
    expect(text()).toContain("In your Claude Code");
    expect(text()).toContain("None ticked: Next goes on");
    const rows = byClass("onb-servers")[0].all().filter((n) => n.nodeName === "LI");
    expect(rows.map((li) => text(li))).toEqual([
      "figma · Claude Code · user scope · Address (HTTP)https://mcp.figma.com/mcp",
      "github · Claude Code · user scope · Commandnpx -y @modelcontextprotocol/server-githubEnvironment: GITHUB_PERSONAL_ACCESS_TOKEN (values go to your keychain)",
      "linear · Claude Code · local scope: /home/jefsev/code/kade · Address (SSE)https://mcp.linear.app/sse",
    ].map(squash));
    // the ticks carry Settings' labels; none is ticked
    const boxes = ["Import figma (Claude Code, user scope)", "Import github (Claude Code, user scope)", "Import linear (Claude Code, local scope)"].map((l) => byLabel(l)[0]);
    expect(boxes.map((b) => b?.checked)).toEqual([false, false, false]);
    expect(text()).toContain("Read only: GizAi doesn't change Claude Code's files or read its sign-ins. An import is a copy: later changes in Claude Code don't follow.");
  });

  it("Next imports the ticked servers under their names and goes on; step 4 lists them", async () => {
    await open();
    await toMcp();
    await tickBox(byLabel("Import figma (Claude Code, user scope)")[0], true);
    expect(text()).toContain("1 ticked: Next imports it");
    await tickBox(byLabel("Import github (Claude Code, user scope)")[0], true);
    expect(text()).toContain("2 ticked: Next imports them");
    await click(button("Next"), on("You're all set"));
    expect(calls("importMcpServers")).toEqual([[[{ key: "user:figma", name: "figma" }, { key: "user:github", name: "github" }]]]);
    expect(summary()["MCP servers imported"]).toBe("figma, github");
  });

  it("with none ticked, Next goes on and imports nothing; step 4 says None", async () => {
    await open();
    await toReady();
    expect(calls("importMcpServers")).toEqual([]);
    expect(summary()["MCP servers imported"]).toBe("None");
  });

  it("a ticked server whose name is taken asks for another name, as in Settings: Next puts the cursor there and imports nothing", async () => {
    await open();
    await toMcp();
    await tickBox(byLabel("Import linear (Claude Code, local scope)")[0], true);
    const rename = byLabel("New name for linear")[0];
    expect(rename.value).toBe("linear");
    expect(text()).toContain("There is already an MCP server called linear: give this one another name");
    await click(button("Next"));
    expect(heading()).toBe("MCP serversOptional");
    expect(focused()).toBe(rename);
    expect(calls("importMcpServers")).toEqual([]);
    await type(rename, "linear-kade");
    expect(text()).not.toContain("There is already an MCP server called");
    await click(button("Next"), on("You're all set"));
    expect(calls("importMcpServers")).toEqual([[[{ key: "local:linear", name: "linear-kade" }]]]);
  });

  it("with no servers in Claude Code, says so in GA-164's words and Next goes on", async () => {
    world.scan = { servers: [], problems: [] };
    await open();
    await toMcp();
    expect(text(byClass("empty")[0])).toBe("Your Claude Code has no MCP servers yet. You can add them later in Settings → MCP servers.");
    expect(text(byClass("empty")[0].all().find((n) => n.nodeName === "B")!)).toBe("Your Claude Code has no MCP servers yet.");
    await click(button("Next"), on("You're all set"));
  });

  it("an error from the import shows under the step, and the step stays", async () => {
    await open();
    await toMcp();
    h.api.importMcpServers = () => { throw "Couldn't import figma: the keychain is locked."; };
    await tickBox(byLabel("Import figma (Claude Code, user scope)")[0], true);
    await click(button("Next"), () => alert() !== null);
    expect(alert()).toBe("Couldn't import figma: the keychain is locked.");
    expect(heading()).toBe("MCP serversOptional");
  });
});

describe("step 4 · Ready", () => {
  it("sums up what you set up, quotes the welcome message word for word, and opens the new chat", async () => {
    await open();
    await type(byId("onb-you"), "  Jefsev S  ");
    await toReady();
    expect(eyebrow()).toBe("Step 4 of 4");
    expect(text()).toContain("Here's what you set up. Your Team Lead is ready to say hello.");
    expect(summary()).toEqual({ "Your name": "Jefsev S", Team: "YoTech", "Your agents build and test with": "Claude Code", "MCP servers imported": "None" });
    expect(container.all().filter((n) => n.nodeName === "BUTTON" && text(n) === "Change").map((n) => n.attrs.get("aria-label"))).toEqual(
      ["Change your name", "Change the team's name", "Change the agents' coding CLI", "Change the MCP servers"]);
    expect(text()).toContain("The chat opens with this message from you:");
    expect(text(container.all().find((n) => n.nodeName === "BLOCKQUOTE")!)).toBe(
      "Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else.");
    await click(button("Open the Team Lead chat"), () => done.mock.calls.length > 0);
    // the names trimmed, the chat's thread opened
    expect(calls("finishOnboarding")).toEqual([[{ youName: "Jefsev S", teamName: "YoTech", agentsCli: "claude_code" }]]);
    expect(done.mock.calls).toEqual([[{ page: "chat", id: "thread-1" }, undefined]]);
  });

  it("with no chat (the Team Lead couldn't start one), opens the board and says why", async () => {
    world.chat = null;
    await open();
    await toReady();
    await click(button("Open the Team Lead chat"), () => done.mock.calls.length > 0);
    expect(done.mock.calls).toEqual([[{ page: "board" }, "The Team Lead's chat couldn't start. You can start one on the Chat page."]]);
  });

  it("an error from finish_onboarding shows under the step; the step stays and the button works again", async () => {
    await open();
    await toReady();
    h.api.finishOnboarding = () => { throw "The team's name can't be empty"; };
    await click(button("Open the Team Lead chat"), () => alert() !== null);
    expect(alert()).toBe("The team's name can't be empty");
    expect(heading()).toBe("You're all set");
    expect(done).not.toHaveBeenCalled();
    expect(off(button("Open the Team Lead chat"))).toBe(false);
  });

  it("Change opens that step again, and its Next comes back to Ready", async () => {
    await open();
    await toReady();
    await click(byLabel("Change the team's name")[0], on("Welcome to GizAi"));
    await type(byId("onb-team"), "Giz crew");
    await click(button("Next"), on("You're all set"));
    expect(summary().Team).toBe("Giz crew");
    await click(byLabel("Change the agents' coding CLI")[0], on("Coding CLIs"));
    await click(button("Next"), on("You're all set"));
    // a done step in the indicator does the same
    await click(byLabel("Back to step 3: MCP servers")[0], on("MCP servers"));
    await click(button("Next"), on("You're all set"));
  });

  it("Enter on the step does what its button does", async () => {
    await open();
    await toReady();
    await enter(container.all().find((n) => n.nodeName === "H1")!, () => done.mock.calls.length > 0);
    expect(calls("finishOnboarding")).toHaveLength(1);
  });
});

describe("Skip setup", () => {
  for (const [step, go] of [["1", async () => {}], ["2", () => toClis()], ["3", () => toMcp()], ["4", () => toReady()]] as const) {
    it(`on step ${step} calls skip_onboarding, saves nothing else and opens the app as today`, async () => {
      await open();
      await go();
      const before = h.calls.length;
      await click(button("Skip setup"), () => done.mock.calls.length > 0);
      expect(h.calls.slice(before).map(([n]) => n)).toEqual(["skipOnboarding"]);
      expect(done.mock.calls).toEqual([[null]]);
      expect(calls("finishOnboarding")).toEqual([]);
    });
  }

  it("an error from skip_onboarding shows under the step, and the step stays", async () => {
    await open();
    h.api.skipOnboarding = () => { throw "Couldn't skip the setup: the database is read only."; };
    await click(button("Skip setup"), () => alert() !== null);
    expect(alert()).toBe("Couldn't skip the setup: the database is read only.");
    expect(done).not.toHaveBeenCalled();
    expect(heading()).toBe("Welcome to GizAi");
  });
});

describe("with the keyboard alone", () => {
  it("Enter on a button does that button's own thing, not Next as well", async () => {
    await open();
    await type(byId("onb-team"), "YoTech");
    await toClis();
    await click(button("Back"), on("Welcome to GizAi"));
    // Enter on Skip setup: the button's own key; the onboarding's Enter leaves it alone
    const e = await act(async () => fire(button("Skip setup")!, "keydown", { key: "Enter", repeat: false, shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, isComposing: false }));
    await tick();
    expect(heading()).toBe("Welcome to GizAi");
    expect(e.defaultPrevented).toBe(false);
  });

  it("goes from step 1 to the chat with Enter only", async () => {
    await open();
    await type(byId("onb-team"), "YoTech");
    await enter(byId("onb-team")!, on("Coding CLIs"));
    await until(() => !text().includes("Looking for coding CLIs"));
    await enter(focused(), on("MCP servers"));
    await until(() => !text().includes("Looking for the MCP servers"));
    await enter(focused(), on("You're all set"));
    await enter(focused(), () => done.mock.calls.length > 0);
    expect(done.mock.calls).toEqual([[{ page: "chat", id: "thread-1" }, undefined]]);
  });
});
