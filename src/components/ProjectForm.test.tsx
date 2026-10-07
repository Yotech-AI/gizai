// GA-30: the project form's New worktrees section: paths to copy, the install switch and the setup command.
// Rendered to HTML on the server, so no data loads: this checks what a new project's form shows and sends.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { copyPaths, ProjectDrawer, toProjectInput } from "./ProjectForm";
import type { Project } from "../types";

describe("New worktrees", () => {
  it("shows one copy list, the install switch (on) and a setup command for a new project", () => {
    const html = renderToStaticMarkup(<ProjectDrawer onClose={() => {}} />);
    expect(html).toContain("New worktrees");
    expect(html).toContain("Copy from the main checkout");
    expect(html.match(/<textarea[^>]*id="p-copy"/g)).toHaveLength(1);
    expect(html).toContain("cp --reflink=auto");
    expect(html).toMatch(/<input type="checkbox" checked=""\/>Install missing dependencies/);
    expect(html).toMatch(/<input id="p-setup"/);
    expect(html).toContain("Setup command");
  });

  it("starts a new project with nothing to copy, the install on and no setup command", () => {
    const v = toProjectInput(null);
    expect(v.worktreeCopy).toEqual([]);
    expect(v.worktreeInstall).toBe(true);
    expect(v.worktreeSetup).toBe("");
  });

  it("keeps a project's own settings when it is edited", () => {
    const p = { id: "p1", number: "P-1", key: "SHOP", name: "Shop", status: "active", defaultBranch: "main", openTasks: 0, doneTasks: 0, updatedAt: 0,
      worktreeCopy: [".env", "vendor/"], worktreeInstall: false, worktreeSetup: "php artisan migrate" } as Project;
    const v = toProjectInput(p);
    expect([v.worktreeCopy, v.worktreeInstall, v.worktreeSetup]).toEqual([[".env", "vendor/"], false, "php artisan migrate"]);
  });

  it("reads the copy list one path per line, skipping blank lines", () => {
    expect(copyPaths(".env\n  node_modules/  \n\n vendor/\ntarget/\n")).toEqual([".env", "node_modules/", "vendor/", "target/"]);
    expect(copyPaths("")).toEqual([]);
  });
});
