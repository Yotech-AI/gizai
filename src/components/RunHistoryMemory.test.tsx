// GA-19 QA: a card's Runs tab lists the memory notes a run's prompt was given, with their size, and says when a note was
// cut. Rendered to HTML on the server with the row open: React's useState is replaced so `useState(false)` (the row's
// open switch) starts true, and effects don't run, so nothing loads.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Run } from "../types";

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init?: unknown) => [init === false ? true : init, () => {}]) as unknown as typeof R.useState;
  const useEffect = (() => {}) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
vi.mock("../api", () => ({ runCommits: () => Promise.resolve([]), runEvents: () => Promise.resolve([]) }));

const { RunHistory } = await import("./RunHistory");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const run = (more: Partial<Run> = {}): Run => ({
  id: "01HZRUN0000000000000000001", agentId: "a1", agentName: "Backend Agent", taskId: "t1", roleKey: "backend", trigger: "manual",
  status: "succeeded", outcome: "ready_for_testing", createdAt: 1, costUsdMicros: 420_000, inputTokens: 0, outputTokens: 0,
  logPath: "/runs/r.jsonl", ...more,
} as Run);

describe("the Runs tab and memory", () => {
  it("lists the notes a run was given, in the order its prompt had them, with their size", () => {
    const html = renderToStaticMarkup(<RunHistory runs={[run({ memory: [
      { path: "Agents/Backend Agent/Notes", chars: 320, shown: 320 },
      { path: "Projects/Kade", chars: 9000, shown: 1200 },
    ] })]} />);
    expect(html).toContain("<dt>Memory</dt>");
    const t = text(html);
    expect(t).toContain("Memory Agents/Backend Agent/Notes · 320 characters");
    expect(t).toContain("Projects/Kade · 1200 of 9000 characters (cut)");
    expect(t.indexOf("Agents/Backend Agent/Notes")).toBeLessThan(t.indexOf("Projects/Kade"));
    expect(html).toContain('<span class="mono">Projects/Kade</span>');
  });
  it("shows no Memory line for a run without notes, or from before memory", () => {
    for (const r of [run({ memory: [] }), run()]) {
      const html = renderToStaticMarkup(<RunHistory runs={[r]} />);
      expect(html).toContain("run-detail");
      expect(html).not.toContain("<dt>Memory</dt>");
    }
  });
  it("shows a path as text, never as markup", () => {
    const html = renderToStaticMarkup(<RunHistory runs={[run({ memory: [{ path: "Lessons/<b>x</b>", chars: 1, shown: 1 }] })]} />);
    expect(html).toContain("Lessons/&lt;b&gt;x&lt;/b&gt;");
  });
});
