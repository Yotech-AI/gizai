// GA-60: the project form's Repository field takes a GitHub or Bitbucket link and offers the folder's GitHub or Bitbucket
// remote until you type a link yourself. Rendered to HTML on the server, with the form's state handed in, in the order of
// its useState calls (initial, v, clients, repo, keyTouched, urlTouched, copyText, err, busy); the remote is offered by the
// form's last effect, run here by hand with the setters recorded.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { RepoCheck } from "../api";
import type { ProjectInput } from "../types";

const queue: unknown[] = [];
let sets: unknown[][] | null = null;
let effects: (() => void)[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    const v = queue.length ? queue.shift() : typeof init === "function" ? (init as () => unknown)() : init;
    if (!sets) return R.useState(v);
    const mine: unknown[] = [];
    sets.push(mine);
    return [v, (x: unknown) => mine.push(x)];
  }) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (sets ? void effects.push(f) : R.useEffect(f, deps))) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : () => new Promise(() => {});
  return out;
});

const { ProjectDrawer, toProjectInput } = await import("./ProjectForm");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const GH = "https://github.com/acme/shop";
const BB = "https://bitbucket.org/acme/shop";
const form = (more: Partial<ProjectInput> = {}): ProjectInput => ({ ...toProjectInput(null), name: "Shop", repoPath: "/home/you/shop", ...more });
type State = { v?: ProjectInput; repo?: RepoCheck | null; urlTouched?: boolean };
const states = (s: State) => { const v = s.v ?? form(); return [v, v, [], s.repo ?? null, false, s.urlTouched ?? false, null, null, false]; };

function render(s: State = {}) {
  queue.push(...states(s));
  try { return renderToStaticMarkup(<ProjectDrawer onClose={() => {}} />); } finally { queue.length = 0; }
}
/** What the form sets the link to when the folder's remotes come in: the `v` setter's calls after the last effect runs. */
function offered(s: State) {
  sets = []; effects = [];
  queue.push(...states(s));
  try {
    ProjectDrawer({ onClose: () => {} });
    const got = sets;
    effects[effects.length - 1]();
    return (got[1] as ProjectInput[]).map((v) => v.repoUrl);
  } finally { sets = null; effects = []; queue.length = 0; }
}
const repo = (more: Partial<RepoCheck>): RepoCheck => ({ isGit: true, branch: "main", dirty: false, ...more });

describe("the Repository field", () => {
  it("is called Repository and asks for a GitHub or Bitbucket link", () => {
    const html = render();
    expect(html).toContain('<label for="p-url">Repository</label>');
    expect(html).not.toContain("GitHub repository");
    expect(html).toMatch(/<input id="p-url" class="input mono" placeholder="https:\/\/github\.com\/owner\/name or https:\/\/bitbucket\.org\/workspace\/name"/);
  });
  it("says a GitHub or Bitbucket link is optional, or that the folder has neither remote", () => {
    expect(text(render())).toContain("Optional. With a GitHub or Bitbucket link, new cards start from the main branch fetched from there");
    expect(text(render({ repo: repo({}) }))).toContain("This folder has no GitHub or Bitbucket remote: paste the link, or leave it empty to start from the local branch");
    expect(text(render({ repo: repo({ bitbucket: BB }) }))).toContain("Optional. With a GitHub or Bitbucket link");
    expect(text(render({ v: form({ repoUrl: BB }), repo: repo({ bitbucket: BB }) }))).toContain("New cards start from the main branch fetched from here");
  });
  it("takes a Bitbucket link as typed", () => {
    expect(render({ v: form({ repoUrl: BB }) })).toContain(`value="${BB}"`);
  });
});

describe("the folder's remote", () => {
  it("is offered when it is on Bitbucket", () => {
    expect(offered({ repo: repo({ github: null, bitbucket: BB }) })).toEqual([BB]);
  });
  it("is offered when it is on GitHub, as before, also when the folder has both", () => {
    expect(offered({ repo: repo({ github: GH }) })).toEqual([GH]);
    expect(offered({ repo: repo({ github: GH, bitbucket: BB }) })).toEqual([GH]);
  });
  it("isn't offered over a link you typed or one already there, nor when there is none", () => {
    expect(offered({ repo: repo({ bitbucket: BB }), urlTouched: true })).toEqual([]);
    expect(offered({ v: form({ repoUrl: "https://bitbucket.org/acme/other" }), repo: repo({ bitbucket: BB }) })).toEqual([]);
    expect(offered({ repo: repo({}) })).toEqual([]);
    expect(offered({ repo: null })).toEqual([]);
  });
});
