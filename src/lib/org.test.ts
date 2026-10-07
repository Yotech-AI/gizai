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
  it("shows the usual software team as empty places when there are no agents", () => {
    expect(shape(buildOrg([]))).toEqual({
      leads: ["+lead"],
      departments: [["Development", ["+frontend", "+backend"]], ["Design", ["+design"]], ["Quality", ["+qa"]], ["Operations", ["+devops"]]],
    });
  });
  it("puts agents in their department and keeps empty places for the rest", () => {
    const o = buildOrg([m("Team Lead", "lead", { chatEnabled: true }), m("Backend Agent", "backend"), m("Second Backend", "backend"), m("QA Agent", "qa")]);
    expect(shape(o)).toEqual({
      leads: ["Team Lead"],
      departments: [["Development", ["Backend Agent", "Second Backend", "+frontend"]], ["Design", ["+design"]], ["Quality", ["QA Agent"]], ["Operations", ["+devops"]]],
    });
  });
  it("sends unknown roles to Specialists", () => {
    const o = buildOrg([m("Docs Agent", "docs")]);
    expect(o.departments[o.departments.length - 1]).toMatchObject({ name: "Specialists" });
    expect(shape(o).departments.pop()).toEqual(["Specialists", ["Docs Agent"]]);
  });
  it("counts the chat agent as the lead whatever its role, and still offers its role's place", () => {
    const o = buildOrg([m("Chief", "backend", { chatEnabled: true })]);
    expect(shape(o).leads).toEqual(["Chief"]);
    expect(shape(o).departments[0]).toEqual(["Development", ["+frontend", "+backend"]]);
  });
  it("ignores people", () => {
    expect(shape(buildOrg([m("Jeffrey", "reviewer", { kind: "person" })])).leads).toEqual(["+lead"]);
  });
});
