// GA-225 QA: the team the board, the Team page and the sidebar show (one pick, shared), the cards on a team's board, the
// project form's question before a move (word for word), and a moved card's activity line.
import { afterEach, describe, expect, it, vi } from "vitest";
import { cardsOfTeam, moveQuestion, pickTeam, shownTeam, teamOfProject } from "./team";
import { describeChange } from "./activity";

const teams = [{ id: "my" }, { id: "rt" }];
const projects = [{ id: "ga", teamId: "my" }, { id: "rov", teamId: "rt" }, { id: "old", teamId: null }];
const cards = [
  { id: "ga-1", projectId: "ga" }, { id: "rov-1", projectId: "rov" }, { id: "old-1", projectId: "old" },
  { id: "loose", projectId: null }, { id: "gone", projectId: "deleted-project" },
];

describe("the team shown", () => {
  it("is the one picked while it is one of the teams, else the first; none without teams", () => {
    expect(shownTeam(teams, "rt")).toBe("rt");
    expect(shownTeam(teams, null)).toBe("my");
    expect(shownTeam(teams, "another-data-folders-team")).toBe("my");
    expect(shownTeam([], "rt")).toBeNull();
  });

  it("is one pick for the board, the Team page and the sidebar: kept on this device, and told to every page", () => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v),
      removeItem: (k: string) => void store.delete(k) });
    const win = new EventTarget();
    vi.stubGlobal("window", win);
    const heard = vi.fn();
    win.addEventListener("gizai-team-changed", heard);
    pickTeam("rt");
    expect(store.get("gizai-team")).toBe("rt");
    expect(heard).toHaveBeenCalledTimes(1);
    pickTeam(null);
    expect(store.has("gizai-team")).toBe(false);
    expect(heard).toHaveBeenCalledTimes(2);
  });
  afterEach(() => vi.unstubAllGlobals());
});

describe("a team's board", () => {
  it("has its projects' cards; the first team's also those without a project or a team of their own", () => {
    expect(cardsOfTeam(cards, projects, "rt", "my").map((c) => c.id)).toEqual(["rov-1"]);
    expect(cardsOfTeam(cards, projects, "my", "my").map((c) => c.id)).toEqual(["ga-1", "old-1", "loose", "gone"]);
  });

  it("is the project's team, else the first team", () => {
    expect(teamOfProject("rov", projects, "my")).toBe("rt");
    expect(teamOfProject("old", projects, "my")).toBe("my");
    expect(teamOfProject(null, projects, "my")).toBe("my");
    expect(teamOfProject("unknown", projects, "my")).toBe("my");
  });
});

describe("the question before a move", () => {
  it("is word for word", () => {
    expect(moveQuestion({ project: "Giz AI", from: "My team", to: "Research team", cards: 31 }))
      .toBe("Move Giz AI to Research team? Its 31 cards go to the columns of the same kind, and My team's agents are taken off them.");
  });

  it("says one card, and no cards, as such", () => {
    expect(moveQuestion({ project: "Giz AI", from: "My team", to: "Research team", cards: 1 }))
      .toBe("Move Giz AI to Research team? Its 1 card goes to the column of the same kind, and My team's agents are taken off it.");
    expect(moveQuestion({ project: "Giz AI", from: "My team", to: "Research team", cards: 0 })).toBe("Move Giz AI to Research team? It has no cards yet.");
  });
});

describe("a moved card's activity", () => {
  it("says it moved with its project, into which column, and that the old team's agents came off", () => {
    expect(describeChange({ table: "tasks", op: "update", diff: { column: ["In progress", "Design"], team: ["My team", "Research team"], agentsOff: true } }))
      .toBe("moved it with its project to Research team, into Design, and took My team's agents off it");
    expect(describeChange({ table: "tasks", op: "update", diff: { column: ["To do", "To do"], team: ["My team", "Research team"] } }))
      .toBe("moved it with its project to Research team, into To do");
  });

  it("keeps a move between columns as before", () => {
    expect(describeChange({ table: "tasks", op: "update", diff: { column: ["To do", "In progress"] } })).toBe("moved it from To do to In progress");
  });
});
