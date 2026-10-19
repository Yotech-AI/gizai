// GA-212 QA: the project form's Test runs at once, in New worktrees next to the setup command, with its help text word
// for word; an empty field for a new project or one without it, a project's own number kept when it is edited; what Save
// refuses before saving (with the backend's why) and what it sends: the number, or 0 for an empty field, which removes
// the limit. Rendered to HTML on the server, so no data loads.
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ProjectDrawer, toProjectInput } from "./ProjectForm";
import { TEST_RUNS, testRunsProblem, testRunsToSave } from "../lib/testRunsAtOnce";
import { shownNumber, typedNumber } from "../lib/screenshotSettings";
import type { Project } from "../types";

const HELP = "How many of this project's cards are tested at the same time. A test run builds and runs the whole test suite; on a big "
  + "project (Rust, a large monorepo) more than one at a time can fill the disk and slow your computer down. Empty: no limit.";
const WHY = "Test runs at once must be from 1 to 20, or empty for no limit.";

const text = (html: string) => html.replace(/&#x27;/g, "'").replace(/&amp;/g, "&");
/** Each field as its label and hint, in order. */
const fields = (html: string) => [...text(html).matchAll(/<div class="field[^"]*"[^>]*><label[^>]*>([^<]*)<\/label>[\s\S]*?<span class="(?:hint|error|warn)">([^<]*)<\/span><\/div>/g)]
  .map((m) => [m[1], m[2]]);

const PROJECT = { id: "p1", number: "P-1", key: "GA", name: "Giz AI", status: "active", defaultBranch: "main", openTasks: 0, doneTasks: 0, updatedAt: 0,
  worktreeCopy: [], worktreeInstall: true } as unknown as Project;

describe("Test runs at once (GA-212)", () => {
  it("is in New worktrees right after the setup command, with its help text word for word", () => {
    const html = renderToStaticMarkup(<ProjectDrawer onClose={() => {}} />);
    const f = fields(html);
    const at = f.findIndex(([label]) => label === "Test runs at once");
    expect(at).toBeGreaterThan(0);
    expect(f[at]).toEqual(["Test runs at once", HELP]);
    expect(f[at - 1][0]).toBe("Setup command");
    const worktrees = html.indexOf("New worktrees");
    const field = html.indexOf('id="p-test-runs"');
    const preview = html.indexOf('id="project-preview"');
    expect(worktrees).toBeGreaterThan(0);
    expect(field).toBeGreaterThan(worktrees);
    expect(field).toBeLessThan(preview);
  });

  it("is a number input from 1 to 20, empty for a new project", () => {
    const html = renderToStaticMarkup(<ProjectDrawer onClose={() => {}} />);
    const input = /<input ([^>]*id="p-test-runs"[^>]*)>/.exec(html)?.[1] ?? "";
    const a = (k: string) => new RegExp(`(?:^| )${k}="([^"]*)"`).exec(input)?.[1];
    expect([a("type"), a("min"), a("max"), a("step"), a("value")]).toEqual(["number", "1", "20", "1", ""]);
    expect(html).toMatch(/<label for="p-test-runs">Test runs at once<\/label>/);
    expect(TEST_RUNS).toEqual({ low: 1, high: 20 });
  });

  it("starts empty for a new project or one without it, and keeps a project's own number", () => {
    expect(toProjectInput(null).testRunsAtOnce).toBeNull();
    expect(toProjectInput(PROJECT).testRunsAtOnce).toBeNull();
    expect(toProjectInput({ ...PROJECT, testRunsAtOnce: null }).testRunsAtOnce).toBeNull();
    expect(toProjectInput({ ...PROJECT, testRunsAtOnce: 3 }).testRunsAtOnce).toBe(3);
    expect(shownNumber(toProjectInput({ ...PROJECT, testRunsAtOnce: 3 }).testRunsAtOnce)).toBe(3);
  });

  it("saves 1 to 20 as typed and an empty field as 0, which removes the limit", () => {
    for (const n of [1, 2, 20]) {
      expect(testRunsProblem(n)).toBeNull();
      expect(testRunsToSave(n)).toBe(n);
    }
    // emptied (NaN while typed, null as loaded): no problem, saved as 0
    for (const empty of [typedNumber(""), typedNumber("  "), null, undefined]) {
      expect(testRunsProblem(empty)).toBeNull();
      expect(testRunsToSave(empty)).toBe(0);
    }
    expect(testRunsToSave(typedNumber("4"))).toBe(4);
  });

  it("refuses any other value before saving, with the backend's why", () => {
    for (const bad of [0, 21, -1, 2.5, 100, typedNumber("1e3")]) {
      expect(testRunsProblem(bad)).toBe(WHY);
    }
  });
});
