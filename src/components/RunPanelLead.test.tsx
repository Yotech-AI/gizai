// GA-70 QA on the card's Run panel: while an agent's question is with the Team Lead, the panel says so (the Team Lead
// answers and the agent carries on, or it asks you) instead of "This card is on hold … Clear the hold"; once the Team Lead
// escalated, it is the usual hold with its reason. Rendered to HTML on the server with the data handed in (as in
// RunForMe.test).
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Member, Run, Task, Team } from "../types";

const { data } = vi.hoisted(() => ({ data: { runs: [] as unknown[] } }));
vi.mock("@tauri-apps/api/core", async (orig) => ({ ...(await orig<typeof import("@tauri-apps/api/core")>()), invoke: vi.fn(async () => null) }));
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../lib/useData", () => ({ useData: () => ({ data: data.runs, error: null, reload: () => {}, setData: () => {} }) }));

const { RunPanel } = await import("./RunPanel");

const agent: Member = { actorId: "be2", name: "Backend Agent 2", kind: "agent", roleKey: "backend", handle: "be2", status: "active", isLead: false,
  allowedTools: [], chatEnabled: false };
const team = { id: "team", name: "Software", members: [agent], states: [], labels: [] } as unknown as Team;
const asking = (lead: Run["lead"]): Run => ({ id: "run-000000001", agentId: "be2", agentName: "Backend Agent 2", taskId: "t1", trigger: "manual",
  status: "succeeded", outcome: "needs_decision", summaryMd: "CSV or JSON for the export?", createdAt: 0, endedAt: 1, costUsdMicros: 80_000,
  inputTokens: 0, outputTokens: 0, logPath: "/x.jsonl", sessionId: "S1", worktreePath: "/wt", lead });
const task = (over: Partial<Task>) => ({ id: "t1", identifier: "GA-70", title: "Export invoices", stateId: "ip", stateCategory: "in_progress",
  assigneeId: "be2", hold: "needs_decision", ...over }) as unknown as Task;

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const panel = (t: Task, runs: Run[]) => { data.runs = runs; return text(renderToStaticMarkup(<RunPanel task={t} team={team} />)); };

describe("the Run panel of a card whose question is with the Team Lead (GA-70)", () => {
  it("says the Team Lead is looking at the question, not that the card is on hold for you", () => {
    const t = panel(task({ holdReason: "CSV or JSON for the export?", withLead: true }), [asking({ state: "asking", costUsdMicros: 0 })]);
    expect(t).toContain("Backend Agent 2 asked a question, and the Team Lead is looking at it: it answers and the agent carries on, or it asks you in the Inbox.");
    expect(t).toContain("With the Team Lead");
    expect(t).not.toContain("This card is on hold");
    expect(t).not.toContain("Clear the hold to run an agent.");
  });
  it("is the usual hold, with the Team Lead's reason, once it asked you", () => {
    const t = panel(task({ holdReason: "Team Lead escalated to you: the client pays per export", withLead: false }),
      [asking({ state: "escalated", reason: "the client pays per export", costUsdMicros: 30_000 })]);
    expect(t).toContain("This card is on hold (Team Lead escalated to you: the client pays per export). Clear the hold to run an agent.");
    expect(t).not.toContain("the Team Lead is looking at it");
  });
});
