// GA-112 QA: Card | Changes at the top of a card's page. Card shows the card with Properties as you left it; Changes
// (#/task/<id>/changes) shows the Changes view full width, Properties closed with Show properties to bring it back, and
// the card's description out of the way. Rendered to HTML on the server with the data hooks answering at once (the
// card's changes load in an effect, so the view shows its loading state).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Task, Team } from "../types";

const answers: Record<string, (...a: unknown[]) => unknown> = {};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : (...a: unknown[]) => (answers[k] ? answers[k](...a) : new Promise(() => {}));
  return out;
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    let data: unknown = null;
    try { const v = fetch(); if (!(v instanceof Promise)) data = v; } catch { /* no data */ }
    return { data, error: null, reload: () => {}, setData: () => {} };
  },
}));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));

const { TaskPage } = await import("./TaskPage");

const team: Team = { id: "team", name: "Software", members: [], labels: [], states: [{ id: "s-prog", name: "In progress", category: "started", sortKey: "a2" }] } as unknown as Team;
const task = {
  id: "t1", identifier: "GA-12", projectId: "p1", projectName: "Shop", projectColor: "#36f", title: "Export invoices", descriptionMd: "Write the CSV export.",
  acceptanceMd: "- [ ] it works", stateId: "s-prog", stateName: "In progress", stateCategory: "started", priority: 0, labels: [], bounceCount: 0,
  failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0, branch: "gizai/ga-12-export-invoices",
} as unknown as Task;

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v), removeItem: (k: string) => void store.delete(k) });
  Object.assign(answers, { getTask: () => task, getTeam: () => team, listUsers: () => [], listComments: () => [], taskActivity: () => [], listRuns: () => [] });
});
afterEach(() => vi.unstubAllGlobals());

const tab = (html: string, name: "Card" | "Changes") => html.match(new RegExp(`<a class="tab" role="tab"[^>]*>(?:(?!</a>).)*${name}(?:(?!</a>).)*</a>`))?.[0] ?? "";

describe("Card | Changes on a card's page", () => {
  it("on Card: Card is selected, the card shows with Properties, and Changes links to #/task/<id>/changes", () => {
    const html = renderToStaticMarkup(<TaskPage id="t1" />);
    expect(html).toContain('<div class="tabs task-tabs" role="tablist" aria-label="The card, its changes and its screenshots">');
    expect(tab(html, "Card")).toContain('aria-selected="true" href="#/task/t1"');
    expect(tab(html, "Changes")).toContain('aria-selected="false" href="#/task/t1/changes"');
    expect(html).toContain('<div class="doc">');
    expect(html).toContain('aria-label="Properties"');
    expect(html).not.toContain('class="chg');
  });

  it("on Changes: Changes is selected, the Changes view shows full width, Properties closed with Show properties", () => {
    const html = renderToStaticMarkup(<TaskPage id="t1" view="changes" />);
    expect(tab(html, "Changes")).toContain('aria-selected="true" href="#/task/t1/changes"');
    expect(tab(html, "Card")).toContain('aria-selected="false" href="#/task/t1"');
    expect(html).not.toContain('<div class="doc">');
    expect(html).not.toContain("Write the CSV export.");
    expect(html).toContain('<div class="chg">');
    expect(html).toContain("Loading the changed files…");
    expect(html).not.toContain('aria-label="Properties"');
    expect(html).toContain('aria-label="Show properties" title="Show properties (])"');
  });

  it("on Changes, Properties stays closed even when it was open on Card, and Card keeps its own choice", () => {
    store.set("gizai-props", "open");
    expect(renderToStaticMarkup(<TaskPage id="t1" view="changes" />)).not.toContain('aria-label="Properties"');
    expect(store.get("gizai-props")).toBe("open");
    store.set("gizai-props", "closed");
    expect(renderToStaticMarkup(<TaskPage id="t1" />)).not.toContain('aria-label="Properties"');
  });

  it("one run's changes: the Changes tab with that run's heading", () => {
    const html = renderToStaticMarkup(<TaskPage id="t1" view="changes" run="01HZRUN1" />);
    expect(tab(html, "Changes")).toContain('aria-selected="true"');
    expect(html).toContain("One run&#x27;s changes");
  });

  it("the Changes tab has no counts while they load", () => {
    const html = renderToStaticMarkup(<TaskPage id="t1" />);
    expect(tab(html, "Changes")).not.toContain("diffstat");
    expect(tab(html, "Changes")).not.toContain("No changes");
  });
});
