// GA-225 QA: the UI's calls for a project's team match the Rust side, read as text (Vite's ?raw): project_team_move is
// invoked with camelCase arguments and registered with Tauri, TeamMove's fields are serde's, ProjectInput carries the team;
// and the New team drawer's subtitle is word for word.
import { describe, expect, it, vi } from "vitest";
import libRs from "../src-tauri/src/lib.rs?raw";
import projectsRs from "../crates/gizai-core/src/projects.rs?raw";
import modelRs from "../crates/gizai-core/src/model.rs?raw";
import teamPage from "./pages/TeamPage.tsx?raw";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

/** The fields of a Rust struct, camelCased as serde's rename_all = "camelCase" sends them. */
function serdeFields(rs: string, name: string): string[] {
  const body = new RegExp(`#\\[serde\\(rename_all = "camelCase"\\)\\]\\s*pub struct ${name} \\{([\\s\\S]*?)\\n\\}`).exec(rs);
  if (!body) throw new Error(`no camelCase struct ${name}`);
  return [...body[1].matchAll(/^\s*pub (\w+):/gm)].map((m) => m[1].replace(/_(\w)/g, (_, c: string) => c.toUpperCase())).sort();
}

describe("a project's team", () => {
  it("asks what a move would do with project_team_move, registered with Tauri", async () => {
    await api.projectTeamMove("p1", "rt");
    expect(invoked.at(-1)).toEqual(["project_team_move", { id: "p1", teamId: "rt" }]);
    expect(libRs).toContain("commands::project_team_move");
  });

  it("gets TeamMove's fields as the backend sends them", () => {
    expect(serdeFields(projectsRs, "TeamMove")).toEqual(["cards", "from", "project", "to"]);
  });

  it("is saved with the project as teamId", async () => {
    expect(modelRs).toMatch(/pub struct ProjectInput \{[\s\S]*?pub team_id: Option<String>,[\s\S]*?\n\}/);
    await api.saveProject("p1", { name: "Giz AI", key: "GA", teamId: "rt" } as never);
    expect(invoked.at(-1)).toEqual(["save_project", { id: "p1", input: { name: "Giz AI", key: "GA", teamId: "rt" } }]);
  });
});

describe("the New team drawer", () => {
  it("says what a new team gets, word for word", () => {
    expect(teamPage).toContain('subtitle="A new team gets the seven usual columns and no agents. Move a project to it in the project\'s form."');
  });
});
