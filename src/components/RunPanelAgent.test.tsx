// GA-53: the Run panel's agent picker says why Gizai picks its agent: the card's agent "(assigned)", else the first agent
// on the card's column "(on the column)"; routing is gone. Rendered to HTML on the server, so no data loads: the agent
// suggestAgent would answer is handed to the component's state directly.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Member, Task, Team } from "../types";

// The values for the useState calls that start as null, in order (the suggested agent, then the error).
let nulls: unknown[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init === null && nulls.length ? nulls.shift() : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});
vi.mock("../lib/useLiveRuns", () => ({ useLiveRuns: () => [] }));
vi.mock("../lib/useData", () => ({ useData: () => ({ data: [], error: null, reload: () => {}, setData: () => {} }) }));

const { RunPanel } = await import("./RunPanel");

const agent = (actorId: string, name: string, roleKey: string): Member => ({
  actorId, name, kind: "agent", roleKey, handle: actorId, status: "active", isLead: false, allowedTools: [], chatEnabled: false,
});
const team = { id: "team", name: "Software", members: [agent("be2", "Backend Agent 2", "backend"), agent("qa2", "QA Agent 2", "qa")],
  states: [], labels: [] } as unknown as Team;
const task = (assigneeId: string | null) => ({ id: "t1", identifier: "GA-1", title: "A card", stateId: "testing", assigneeId, hold: null }) as unknown as Task;
const render = (suggested: string | null, assigneeId: string | null) => {
  nulls = [suggested, null];
  try { return renderToStaticMarkup(<RunPanel task={task(assigneeId)} team={team} />); } finally { nulls = []; }
};
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/\s+/g, " ").trim();

describe("the Run panel's agent picker", () => {
  it("says (assigned) when Gizai picks the card's agent", () => {
    const html = render("be2", "be2");
    expect(html).toContain('<option value="" selected="">Backend Agent 2 (assigned)</option>');
    expect(text(html)).toContain("Run starts Backend Agent 2 unless you pick another agent");
  });

  it("says (on the column) when Gizai picks the first agent on the card's column", () => {
    const html = render("qa2", null);
    expect(html).toContain('<option value="" selected="">QA Agent 2 (on the column)</option>');
    expect(html).not.toContain("(routed)");
    expect(text(html)).toContain("Run starts the agent chosen here (or the card's agent, else the first agent on its column)");
    expect(text(html)).not.toMatch(/routing/i);
  });

  it("asks you to choose when nobody picks the card up by itself", () => {
    const html = render(null, null);
    expect(html).toContain('<option value="" selected="">Choose an agent</option>');
    expect(text(html)).toContain("No agent picks this card up by itself; pick one");
  });
});
