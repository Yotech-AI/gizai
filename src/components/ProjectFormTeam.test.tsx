// GA-225 QA, the project form's Team: a select only when there is more than one team; a new project starts in the team the
// board shows (the first when none is picked); an edit keeps the project's own team. Rendered to HTML on the server: the
// form's lists (clients, then teams) start empty and fill in effects, which a server render doesn't run, so they are
// handed in here in that order.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Project, TeamSummary } from "../types";

const page = vi.hoisted(() => ({ lists: [] as unknown[][] }));
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    if (Array.isArray(init) && init.length === 0 && page.lists.length) return R.useState(page.lists.shift());
    return R.useState(init);
  }) as typeof R.useState;
  return { ...R, useState, default: { ...R, useState } };
});

const { ProjectDrawer, toProjectInput } = await import("./ProjectForm");

const two: TeamSummary[] = [{ id: "my", name: "My team" }, { id: "rt", name: "Research team" }];

let store: Map<string, string>;
beforeEach(() => {
  store = new Map();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k) });
});
afterEach(() => { vi.unstubAllGlobals(); page.lists = []; });

const form = (teams: TeamSummary[]) => {
  page.lists = [[], teams];
  return renderToStaticMarkup(<ProjectDrawer onClose={() => {}} />);
};
const teamField = (html: string) => /<label for="p-team">Team<\/label>[\s\S]*?<\/select>[\s\S]*?<span class="hint">([^<]*)<\/span>/.exec(html);
const options = (html: string) => /<select id="p-team"[^>]*>([\s\S]*?)<\/select>/.exec(html)?.[1] ?? null;

describe("the project form's Team", () => {
  it("isn't there with one team", () => {
    const html = form([two[0]]);
    expect(html).not.toContain('id="p-team"');
    expect(html).not.toContain(">Team</label>");
  });

  it("is a select with two teams, right after Client, on the first team when none is picked", () => {
    const html = form(two);
    expect(options(html)).toBe('<option value="my" selected="">My team</option><option value="rt">Research team</option>');
    expect(teamField(html)?.[1]).toBe("Its cards go on this team&#x27;s board");
    expect(html.indexOf('id="p-team"')).toBeGreaterThan(html.indexOf('id="p-client"'));
    expect(html.indexOf('id="p-team"')).toBeLessThan(html.indexOf('id="p-status"'));
  });

  it("starts a new project in the team the board shows", () => {
    store.set("gizai-team", "rt");
    expect(options(form(two))).toBe('<option value="my">My team</option><option value="rt" selected="">Research team</option>');
  });

  it("keeps a project's own team on an edit, and none for a new one", () => {
    const p = { id: "p1", number: "P-1", key: "GA", name: "Giz AI", status: "active", defaultBranch: "main", openTasks: 0, doneTasks: 0, updatedAt: 0,
      worktreeCopy: [], worktreeInstall: true, teamId: "rt", teamName: "Research team" } as unknown as Project;
    expect(toProjectInput(p).teamId).toBe("rt");
    expect(toProjectInput(null).teamId).toBeNull();
  });
});
