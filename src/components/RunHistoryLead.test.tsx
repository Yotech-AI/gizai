// GA-70 QA: a card's Runs tab says who answered a run's question: the badge while the Team Lead has it or answered it,
// the row's line when it went to you, and in the open row "Who answered", the answer, your answer kept in memory and
// what the Team Lead's look cost (on its budget). Rendered to HTML on the server with the row open: React's useState is
// replaced so `useState(false)` (the row's open switch) starts true, and effects don't run, so nothing loads.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { LeadAnswer, Run } from "../types";

vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init?: unknown) => [init === false ? true : init, () => {}]) as unknown as typeof R.useState;
  const useEffect = (() => {}) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
vi.mock("../api", () => ({ runCommits: () => Promise.resolve([]), runEvents: () => Promise.resolve([]) }));

const { RunHistory } = await import("./RunHistory");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const lead = (more: Partial<LeadAnswer>): LeadAnswer => ({ state: "asking", leadId: "lead", runId: "01HZLEADRUN000000000000001", costUsdMicros: 30_000, ...more });
const asked = (l: LeadAnswer | null): Run => ({
  id: "01HZRUN0000000000000000001", agentId: "a1", agentName: "Backend Agent", taskId: "t1", roleKey: "backend", trigger: "manual",
  status: "succeeded", outcome: "needs_decision", summaryMd: "CSV or JSON for the export?", createdAt: 1, costUsdMicros: 80_000,
  inputTokens: 0, outputTokens: 0, logPath: "/runs/r.jsonl", lead: l,
} as Run);
const render = (l: LeadAnswer | null) => renderToStaticMarkup(<RunHistory runs={[asked(l)]} />);
const row = (html: string) => text(html.slice(html.indexOf('<button class="panel-row"'), html.indexOf("</button>")));

describe("the Runs tab: who answered a question (GA-70)", () => {
  it("while the Team Lead has it: a teal badge, and its look in the open row", () => {
    const html = render(lead({ state: "asking", costUsdMicros: 0 }));
    expect(html).toContain('<span class="badge live">With the Team Lead</span>');
    expect(row(html)).not.toContain("· With the Team Lead");
    expect(text(html)).toContain("Who answered With the Team Lead");
  });
  it("answered: the badge, the answer, where it was kept and what the look cost on the Team Lead's budget", () => {
    const html = render(lead({ state: "answered", answer: "Use CSV with semicolons.", note: "Standards/Exports" }));
    expect(html).toContain('<span class="badge ok">Team Lead answered</span>');
    expect(row(html)).not.toContain("· Team Lead answered");
    const t = text(html);
    expect(t).toContain("Who answered Team Lead answered Use CSV with semicolons.");
    expect(t).toContain("Team Lead's look $0.03, on the Team Lead's budget · answer kept in Standards/Exports");
  });
  it("escalated: Needs your decision, the row says the Team Lead asked you, the open row says why, and your answer once kept", () => {
    const html = render(lead({ state: "escalated", reason: "the client pays per export" }));
    expect(html).toContain('<span class="badge needs">Needs your decision</span>');
    expect(row(html)).toContain("· Team Lead escalated to you");
    expect(row(html)).not.toContain("the client pays per export");
    expect(text(html)).toContain("Who answered Team Lead escalated to you: the client pays per export");
    const learned = text(render(lead({ state: "escalated", reason: "money", learned: true, note: "Decisions/Kade" })));
    expect(learned).toContain("Your answer is kept in memory (Decisions/Kade).");
    expect(learned).not.toContain("answer kept in Decisions/Kade");
  });
  it("went to you by a limit, or because the Team Lead can't look here: the row says which", () => {
    expect(row(render(lead({ state: "limit", leadId: null, runId: null, reason: "the Team Lead answered this card's last question" }))))
      .toContain("· Went to you: the Team Lead answered this card's last question");
    expect(row(render(lead({ state: "skipped", runId: null, reason: "no gizai-mcp helper" }))))
      .toContain("· Went to you: the Team Lead can't look at questions here (no gizai-mcp helper)");
    // no run of the Team Lead: no cost line
    expect(text(render(lead({ state: "limit", leadId: null, runId: null, reason: "x" })))).not.toContain("Team Lead's look");
  });
  it("a question that went to you directly shows none of it", () => {
    const html = render(null);
    expect(html).toContain('<span class="badge needs">Needs your decision</span>');
    expect(text(html)).not.toContain("Who answered");
    expect(text(html)).not.toContain("Team Lead");
  });
});
