// GA-41: what the @ picker lists. Open tasks only, projects that aren't archived (and their docs), clients that aren't
// archived, each agent once (not people on a team, not archived agents) and people. Kept for a short while.
import { beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  listTasks: vi.fn(), listProjects: vi.fn(), listClients: vi.fn(), listUsers: vi.fn(), listTeams: vi.fn(), getTeam: vi.fn(), listDocs: vi.fn(),
}));
vi.mock("../api", () => api);

const member = (actorId: string, name: string, kind = "agent", status = "active") => ({ actorId, name, kind, status, title: null });

function answers() {
  api.listTasks.mockResolvedValue([{ identifier: "GA-12", title: "Fix the login", projectName: "Giz AI" }]);
  api.listProjects.mockResolvedValue([
    { id: "p1", key: "GA", name: "Giz AI", status: "active", clientName: null },
    { id: "p2", key: "OLD", name: "Old site", status: "archived", clientName: null },
  ]);
  api.listClients.mockResolvedValue([{ id: "c1", name: "Spoorwegmuseum", status: "active" }, { id: "c2", name: "Gone BV", status: "archived" }]);
  api.listUsers.mockResolvedValue([{ id: "u1", name: "Sanne", handle: "sanne" }]);
  api.listTeams.mockResolvedValue([{ id: "t1", name: "Software" }, { id: "t2", name: "Design" }]);
  api.getTeam.mockImplementation(async (id: string) => ({
    members: id === "t1"
      ? [member("a1", "Backend Agent"), member("u1", "Sanne", "human"), member("a9", "Retired Agent", "agent", "archived")]
      : [member("a1", "Backend Agent"), member("a2", "Designer")],
  }));
  api.listDocs.mockImplementation(async (projectId: string) => (projectId === "p1" ? [{ id: "d1", title: "Spec" }] : [{ id: "d2", title: "Old doc" }]));
}

describe("loadPickItems", () => {
  beforeEach(() => {
    vi.resetModules();
    for (const f of Object.values(api)) f.mockReset();
    answers();
  });

  it("asks for open tasks only and lists every kind, archived ones left out", async () => {
    const { loadPickItems } = await import("./pickItems");
    const items = await loadPickItems();
    expect(api.listTasks).toHaveBeenCalledWith({ openOnly: true });
    expect(items.map((i) => `${i.kind}:${i.key}:${i.label}`)).toEqual([
      "task:GA-12:GA-12 - Fix the login",
      "project:GA:GA - Giz AI",
      "client:c1:Spoorwegmuseum",
      "agent:a1:Backend Agent",
      "agent:a2:Designer",
      "person:u1:Sanne",
      "doc:d1:Spec",
    ]);
    expect(api.listDocs).toHaveBeenCalledTimes(1);
    expect(api.listDocs).toHaveBeenCalledWith("p1");
  });

  it("keeps the items for the next @ and asks again after a failure", async () => {
    const { loadPickItems } = await import("./pickItems");
    await loadPickItems();
    await loadPickItems();
    expect(api.listTasks).toHaveBeenCalledTimes(1);

    vi.resetModules();
    const fresh = await import("./pickItems");
    api.listTasks.mockReset();
    api.listTasks.mockRejectedValueOnce("offline").mockResolvedValue([]);
    await expect(fresh.loadPickItems()).rejects.toBe("offline");
    await expect(fresh.loadPickItems()).resolves.toBeInstanceOf(Array);
    expect(api.listTasks).toHaveBeenCalledTimes(2);
  });

  it("still lists the rest when a team or a project's docs can't be read", async () => {
    api.getTeam.mockRejectedValue("no team");
    api.listDocs.mockRejectedValue("no docs");
    const { loadPickItems } = await import("./pickItems");
    const kinds = (await loadPickItems()).map((i) => i.kind);
    expect(kinds).toEqual(["task", "project", "client", "person"]);
  });
});
