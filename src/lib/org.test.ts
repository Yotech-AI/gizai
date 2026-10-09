import { describe, expect, it } from "vitest";
import { buildOrg } from "./org";
import type { Member } from "../types";

const m = (name: string, roleKey: string, extra: Partial<Member> = {}): Member => ({
  actorId: name, name, kind: "agent", roleKey, handle: name, status: "active", isLead: roleKey === "lead", allowedTools: [], chatEnabled: false, ...extra,
});
const shape = (o: ReturnType<typeof buildOrg>) => ({
  leads: o.leads.map((n) => (n.kind === "agent" ? n.member.name : `+${n.role}`)),
  departments: o.departments.map((d) => [d.name, d.nodes.map((n) => (n.kind === "agent" ? n.member.name : `+${n.role}`))]),
});

describe("buildOrg", () => {
  // GA-53: the branches keep the app's names, Design first, and every branch shows one empty spot below its agents.
  it("shows the branches with one empty spot each when there are no agents, Design first", () => {
    expect(shape(buildOrg([]))).toEqual({
      leads: ["+lead"],
      departments: [["Design", ["+design"]], ["Development", ["+frontend"]], ["Quality", ["+qa"]], ["Operations", ["+devops"]]],
    });
  });
  it("puts agents in their branch and keeps one empty spot below them", () => {
    const o = buildOrg([m("Team Lead", "lead", { chatEnabled: true }), m("Backend Agent", "backend"), m("Second Backend", "backend"), m("QA Agent", "qa")]);
    expect(shape(o)).toEqual({
      leads: ["Team Lead"],
      departments: [["Design", ["+design"]], ["Development", ["Backend Agent", "Second Backend", "+frontend"]], ["Quality", ["QA Agent", "+qa"]], ["Operations", ["+devops"]]],
    });
  });
  it("sends unknown roles to Specialists", () => {
    const o = buildOrg([m("Docs Agent", "docs")]);
    expect(o.departments[o.departments.length - 1]).toMatchObject({ name: "Specialists" });
    expect(shape(o).departments.pop()).toEqual(["Specialists", ["Docs Agent"]]);
  });
  it("counts the chat agent as the lead whatever its role, and still counts it for its branch", () => {
    const o = buildOrg([m("Chief", "backend", { chatEnabled: true })]);
    expect(shape(o).leads).toEqual(["Chief"]);
    expect(shape(o).departments[1]).toEqual(["Development", ["+frontend"]]);
    // the branch can't be removed while the lead has one of its roles
    expect(o.departments[1].agents).toBe(1);
  });
  it("ignores people", () => {
    expect(shape(buildOrg([m("Jeffrey", "reviewer", { kind: "person" })])).leads).toEqual(["+lead"]);
  });
});
