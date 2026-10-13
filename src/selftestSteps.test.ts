// GA-148 QA: the steps mode (GIZAI_SELFTEST_MODE=steps:…) as the product tour's capture uses it. A named click
// ("<name>[@ms]=<selector>", with an optional :text(words)) takes a still before and one after the click, notes the
// clicked element's box in screen pixels (after scrolling it into view), and the steps are reported at the end with the
// page's size and theme; "theme:", "wait:" and "term:" set things up without stills; and steps without a name ("#/route",
// a bare selector) work as before: no stills and no report. The Tauri commands behind it are tour_still { name } and
// tour_steps { json }. There is no DOM here, so a small fake document stands in for the page.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}), emit: () => Promise.resolve() }));
const term = vi.hoisted(() => ({ list: [] as { id: string }[], input: [] as [string, string][] }));
vi.mock("./api", async (orig) => ({
  ...(await orig<typeof import("./api")>()),
  terminalList: () => Promise.resolve(term.list),
  terminalInput: (id: string, data: string) => { term.input.push([id, data]); return Promise.resolve(); },
}));
const themes = vi.hoisted(() => [] as [unknown, unknown][]);
vi.mock("./lib/appearance", () => ({
  getAppearance: () => ({}), resetAppearance: () => {},
  setAppearance: (patch: { theme?: string }, keep?: boolean) => {
    themes.push([patch, keep]);
    if (patch.theme) (globalThis as unknown as { document: FakeDoc }).document.documentElement.dataset.theme = patch.theme;
  },
}));
vi.mock("./lib/terminals", () => ({ hardwareWebgl2: () => false, probeView: () => null, setWebglAllowed: () => {}, showIn: () => {} }));
vi.mock("./components/memory/GraphCanvas", () => ({ graphProbe: () => null }));
vi.mock("./components/terminal/TerminalView", () => ({ TerminalView: () => null }));
vi.mock("@codemirror/view", () => ({ EditorView: { findFromDOM: () => null } }));
vi.mock("@codemirror/merge", () => ({ getChunks: () => null, getOriginalDoc: () => null }));

type Rect = { left: number; top: number; width: number; height: number };
type FakeDoc = { documentElement: { dataset: Record<string, string> }; querySelectorAll: (css: string) => FakeEl[]; querySelector: (css: string) => FakeEl | null };

/** What happened, in order, with the (fake) time: stills, clicks and scrolls. */
let events: string[] = [];
let page: FakeEl[] = [];

class FakeEl {
  constructor(public css: string, public textContent: string, public rect: Rect, public scrollTo?: Rect) {}
  getBoundingClientRect() {
    const { left, top, width, height } = this.rect;
    return { left, top, width, height, right: left + width, bottom: top + height, x: left, y: top };
  }
  click() { events.push(`click ${this.css} ${this.textContent} @${Date.now()}`); }
  scrollIntoView() { events.push(`scroll ${this.css}`); if (this.scrollTo) this.rect = this.scrollTo; }
}

const fakeWindow = { devicePixelRatio: 2, innerWidth: 1280, innerHeight: 832, location: { hash: "" } };
const fakeDocument: FakeDoc = {
  documentElement: { dataset: {} },
  querySelectorAll: (css) => { if (css.startsWith("!")) throw new Error("not a selector"); return page.filter((e) => e.css === css || css === "*"); },
  querySelector: (css) => page.find((e) => e.css === css) ?? null,
};
vi.stubGlobal("window", fakeWindow);
vi.stubGlobal("document", fakeDocument);
vi.stubGlobal("requestAnimationFrame", (cb: (t: number) => void) => setTimeout(() => cb(0), 16));

const { runSteps } = await import("./selftest");
const api = await import("./api");

/** Runs the steps with stills and a report that only note what they were asked, the timers run out by hand. */
async function run(steps: string[], still: (name: string) => Promise<void> = () => Promise.resolve()) {
  const reports: any[] = [];
  const done = runSteps(steps, {
    still: (name) => { events.push(`still ${name} @${Date.now()}`); return still(name); },
    report: (json) => { reports.push(json); return Promise.resolve(); },
  });
  await vi.runAllTimersAsync();
  await done;
  return reports;
}
const at = (e: string) => Number(e.split("@").at(-1));

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date", "performance"] });
  vi.setSystemTime(0);
  events = []; page = []; themes.length = 0; term.list = []; term.input = []; invoked.length = 0;
  fakeWindow.location.hash = "";
  fakeDocument.documentElement.dataset = {};
});
afterEach(() => { vi.useRealTimers(); });

describe("a named click (the product tour)", () => {
  it("takes a still before the click and one after, and reports the step with its stills and its box in screen pixels", async () => {
    page = [new FakeEl('[data-card="KADE-1"]', "KADE-1 Driver app", { left: 100.4, top: 50.2, width: 200, height: 40.3 })];
    const reports = await run(['open-card=[data-card="KADE-1"]']);
    expect(events.map((e) => e.split(" @")[0])).toEqual(["still open-card-before", 'click [data-card="KADE-1"] KADE-1 Driver app', "still open-card-after"]);
    expect(reports).toEqual([{
      viewport: { width: 2560, height: 1664, scale: 2 }, theme: "dark", errors: [],
      steps: [{ name: "open-card", selector: '[data-card="KADE-1"]', before: "open-card-before.png", after: "open-card-after.png", box: { x: 201, y: 100, width: 400, height: 81 } }],
    }]);
  });

  it("waits 1.5 s after the click for its after still, or the @<ms> the step gives", async () => {
    page = [new FakeEl(".a", "A", { left: 10, top: 10, width: 10, height: 10 }), new FakeEl(".b", "B", { left: 30, top: 10, width: 10, height: 10 })];
    await run(["first=.a", "second@3000=.b"]);
    const [, clickA, afterA, , clickB, afterB] = events;
    expect(events.map((e) => e.split(" @")[0])).toEqual(["still first-before", "click .a A", "still first-after", "still second-before", "click .b B", "still second-after"]);
    expect(at(afterA!) - at(clickA!)).toBeGreaterThanOrEqual(1500);
    expect(at(afterA!) - at(clickA!)).toBeLessThan(1600);
    expect(at(afterB!) - at(clickB!)).toBeGreaterThanOrEqual(3000);
    expect(at(afterB!) - at(clickB!)).toBeLessThan(3100);
  });

  it(":text(words) clicks the first element of the selector that shows and has those words", async () => {
    page = [
      new FakeEl(".btn", "Approve", { left: 0, top: 0, width: 0, height: 0 }), // not shown
      new FakeEl(".btn", "Reject", { left: 10, top: 10, width: 50, height: 20 }),
      new FakeEl(".btn", "Approve all", { left: 70, top: 10, width: 50, height: 20 }),
    ];
    const reports = await run(["approve=.btn:text(Approve)"]);
    expect(events.filter((e) => e.startsWith("click")).map((e) => e.split(" @")[0])).toEqual(["click .btn Approve all"]);
    expect(reports[0].steps[0]).toMatchObject({ selector: ".btn:text(Approve)", box: { x: 140, y: 20, width: 100, height: 40 } });
  });

  it("scrolls an element out of sight into view first, and its box is where it is after the scroll", async () => {
    page = [
      new FakeEl(".low", "Low", { left: 100, top: 900, width: 80, height: 30 }, { left: 100, top: 400, width: 80, height: 30 }),
      new FakeEl(".seen", "Seen", { left: 100, top: 100, width: 80, height: 30 }, { left: 100, top: 0, width: 80, height: 30 }),
    ];
    const reports = await run(["low=.low", "seen=.seen"]);
    expect(events.map((e) => e.split(" @")[0])).toEqual(["scroll .low", "still low-before", "click .low Low", "still low-after", "still seen-before", "click .seen Seen", "still seen-after"]);
    expect(reports[0].steps.map((s: { box: unknown }) => s.box)).toEqual([{ x: 200, y: 800, width: 160, height: 60 }, { x: 200, y: 200, width: 160, height: 60 }]);
  });

  it("says so when there is nothing to click (after waiting 10 s), and goes on with the next step", async () => {
    page = [new FakeEl(".b", "B", { left: 10, top: 10, width: 10, height: 10 })];
    const reports = await run(["missing=.nothing", "broken=!bad", "there=.b"]);
    expect(Date.now()).toBeGreaterThanOrEqual(20000);
    expect(events.map((e) => e.split(" @")[0])).toEqual(["still there-before", "click .b B", "still there-after"]);
    expect(reports[0].errors).toEqual(["missing: nothing to click at .nothing", "broken: nothing to click at !bad"]);
    expect(reports[0].steps.map((s: { name: string }) => s.name)).toEqual(["there"]);
  });

  it("notes a still that couldn't be taken and still clicks", async () => {
    page = [new FakeEl(".a", "A", { left: 10, top: 10, width: 10, height: 10 })];
    const reports = await run(["a=.a"], (name) => (name.endsWith("before") ? Promise.reject("grim: no screen") : Promise.resolve()));
    expect(events.map((e) => e.split(" @")[0])).toEqual(["still a-before", "click .a A", "still a-after"]);
    expect(reports[0].errors).toEqual(["a-before: grim: no screen"]);
    expect(reports[0].steps).toHaveLength(1);
  });
});

describe("the steps that set a scene up", () => {
  it("theme:light switches the theme for this start only (not kept), and the report says light", async () => {
    page = [new FakeEl(".a", "A", { left: 10, top: 10, width: 10, height: 10 })];
    const reports = await run(["theme:light", "a=.a"]);
    expect(themes).toEqual([[{ theme: "light" }, false]]);
    expect(reports[0].theme).toBe("light");
  });

  it("theme:dark is dark, and an unknown theme changes nothing", async () => {
    page = [new FakeEl(".a", "A", { left: 10, top: 10, width: 10, height: 10 })];
    await run(["theme:sepia"]);
    expect(themes).toEqual([]);
    const reports = await run(["theme:dark", "a=.a"]);
    expect(themes).toEqual([[{ theme: "dark" }, false]]);
    expect(reports[0].theme).toBe("dark");
  });

  it("wait:<ms> waits that long and takes no still", async () => {
    await run(["wait:2500"]);
    expect(Date.now()).toBe(2500);
    expect(events).toEqual([]);
  });

  it("term:<command> types the command and Enter in the newest terminal, or says no terminal is open", async () => {
    term.list = [{ id: "t1" }, { id: "t2" }];
    page = [new FakeEl(".a", "A", { left: 10, top: 10, width: 10, height: 10 })];
    await run(["term:git status --short", "a=.a"]);
    expect(term.input).toEqual([["t2", "git status --short\r"]]);
    term.list = []; term.input = [];
    const reports = await run(["term:ls", "a=.a"]);
    expect(term.input).toEqual([]);
    expect(reports[0].errors).toEqual(["term:ls: no terminal is open"]);
  });
});

describe("steps without a name (scripts/shot-cage.sh, scripts/readme-gif.sh) work as before", () => {
  it("go to a route and click a bare selector 1.2 s after the step before, with no stills and no report", async () => {
    page = [new FakeEl('a[href="#/board"]', "Board", { left: 10, top: 10, width: 10, height: 10 })];
    const reports = await run(["#/tasks", 'a[href="#/board"]']);
    expect(fakeWindow.location.hash).toBe("#/tasks");
    expect(events).toEqual(['scroll a[href="#/board"]', 'click a[href="#/board"] Board @2400']);
    expect(reports).toEqual([]);
  });

  it("a bare selector with = in it is not a named click", async () => {
    page = [new FakeEl("button[data-x=one]", "X", { left: 10, top: 10, width: 10, height: 10 })];
    const reports = await run(["button[data-x=one]"]);
    expect(events.map((e) => e.split(" @")[0])).toEqual(["scroll button[data-x=one]", "click button[data-x=one] X"]);
    expect(reports).toEqual([]);
  });

  it("a missing bare selector is skipped quietly, as before", async () => {
    const reports = await run([".nothing", "#/board"]);
    expect(events).toEqual([]);
    expect(fakeWindow.location.hash).toBe("#/board");
    expect(reports).toEqual([]);
  });
});

describe("the tour's Tauri commands", () => {
  it("tourStill calls tour_still with { name } and tourSteps calls tour_steps with the steps as JSON text", async () => {
    await api.tourStill("open-card-before");
    await api.tourSteps({ theme: "dark", steps: [] });
    expect(invoked).toEqual([["tour_still", { name: "open-card-before" }], ["tour_steps", { json: '{"theme":"dark","steps":[]}' }]]);
  });
});
