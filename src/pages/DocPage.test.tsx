// GA-19: the doc page opens a memory note with the crumbs "Memory / Team Lead / Notes", each folder and slash a direct
// child of .crumbs (its flex gap spaces them, like "Projects / Kade /"), and a project doc still with Projects and its
// project. Rendered to HTML on the server: the data hooks are replaced so the page gets its data at once, and the editor's
// text and version (set by an effect in the app) are handed in as its first null states.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Doc, DocVersion, Project } from "../types";

// What the mocked api answers, by function name; anything else never answers (its useData stays empty).
const answers: Record<string, (...a: unknown[]) => unknown> = {};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => (answers[k] ? answers[k](...a) : new Promise(() => {}));
  }
  return out;
});
vi.mock("../lib/useData", () => ({
  useData: (fetch: () => unknown) => {
    let data: unknown = null;
    try { const v = fetch(); if (!(v instanceof Promise)) data = v; } catch { /* no data */ }
    return { data, error: null, reload: () => {}, setData: () => {} };
  },
}));
vi.mock("../components/MarkdownEditor", () => ({ MarkdownEditor: () => null }));
// The editor's text and the version it is based on: the page's first null-initialised states take these values.
const states: unknown[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(init === null && states.length > 0 ? states.shift() : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { DocPage } = await import("./DocPage");

const doc = (more: Partial<Doc>): Doc => ({ id: "d1", title: "Notes", bodyMd: "# Notes\n", currentVersion: 2, updatedAt: Date.now(), ...more });
const page = (d: Doc) => {
  answers.getDoc = () => d;
  answers.docVersions = () => [] as DocVersion[];
  answers.getProject = () => ({ id: "p1", name: "Kade" }) as Project;
  states.push(d.bodyMd, d.currentVersion);
  return renderToStaticMarkup(<DocPage id={d.id} />);
};
const crumbs = (html: string) => {
  const from = html.indexOf('<div class="crumbs">') + '<div class="crumbs">'.length;
  return html.slice(from, html.indexOf("</div>", from));
};

beforeEach(() => { states.length = 0; for (const k of Object.keys(answers)) delete answers[k]; });

describe("DocPage crumbs", () => {
  it("show a memory note as Memory / its folders / its title, every part spaced by .crumbs itself", () => {
    const html = page(doc({ kind: "memory", path: "Team Lead/Notes" }));
    expect(crumbs(html)).toBe('<span>Memory</span><span class="sep">/</span><span>Team Lead</span><span class="sep">/</span><b>Notes</b>');
    expect(html).not.toContain(">Projects<");
    expect(html).toContain("Saved · version 2");
  });

  it("show every folder of a deeper note, and no wrapping span around a folder and its slash", () => {
    const c = crumbs(page(doc({ kind: "memory", path: "Agents/Backend Agent/Notes" })));
    expect(c).toBe('<span>Memory</span><span class="sep">/</span><span>Agents</span><span class="sep">/</span>'
      + '<span>Backend Agent</span><span class="sep">/</span><b>Notes</b>');
    expect(c).not.toContain("<span><span>");
  });

  it("keep Projects and the project for a project's doc", () => {
    const c = crumbs(page(doc({ kind: "doc", projectId: "p1", title: "Spec" })));
    expect(c).toBe('<a href="#/projects">Projects</a><span class="sep">/</span><a href="#/project/p1">Kade</a><span class="sep">/</span><b>Spec</b>');
    expect(c).not.toContain("Memory");
  });
});
