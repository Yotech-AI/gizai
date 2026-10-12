// GA-112 QA: Show changes beside a run in the Runs tab (only what that run committed, at #/task/<id>/changes/run/<run
// id>), and the card's counts with Show changes in the Pull request panel and in Properties. Rendered to HTML on the
// server: in the Runs tab a row's commits are handed over as if they had loaded (React's useState is replaced so the
// values queued in `loaded` stand in for the row's `null` starting values, in order: events, commits, commits' error).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { CardChanges, Commit, Run, Task, Team } from "../types";

const loaded: unknown[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init?: unknown) => [init === null && loaded.length ? loaded.shift() : init, () => {}]) as unknown as typeof R.useState;
  const useEffect = (() => {}) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
vi.mock("../api", () => ({
  runCommits: () => new Promise(() => {}), runEvents: () => new Promise(() => {}), checkPullRequest: () => new Promise(() => {}),
  getProject: () => new Promise(() => {}), openPullRequest: () => new Promise(() => {}), listTasks: () => new Promise(() => {}),
  moveTask: () => new Promise(() => {}), setTaskLabels: () => new Promise(() => {}), updateTask: () => new Promise(() => {}),
}));
// the project of the Pull request panel: linked to GitHub
vi.mock("../lib/useData", () => ({ useData: () => ({ data: { id: "p1", repoUrl: "https://github.com/acme/shop", defaultBranch: "main" }, error: null, reload: () => {}, setData: () => {} }) }));

const { RunHistory } = await import("./RunHistory");
const { PullPanel } = await import("./PullPanel");
const { Properties } = await import("./Properties");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
const commit = (n: number): Commit => ({ sha: `${n}`.repeat(40).slice(0, 40), subject: `Change ${n}` });
const run = (id: string, more: Partial<Run> = {}): Run => ({
  id, agentId: "a1", agentName: "Backend Agent", taskId: "t1", roleKey: "backend", trigger: "manual", status: "succeeded",
  outcome: "ready_for_testing", createdAt: 1, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "/runs/r.jsonl",
  baseSha: "a".repeat(40), headSha: "b".repeat(40), ...more,
} as Run);
const showChanges = (html: string) => [...html.matchAll(/<a class="btn ghost sm" href="([^"]+)" title="Only what this run committed">(?:(?!<\/a>).)*Show changes<\/a>/g)].map((m) => m[1]);

beforeEach(() => { loaded.length = 0; });

describe("Show changes in the Runs tab", () => {
  it("is beside a run that committed, and opens only that run's changes", () => {
    loaded.push(null, [commit(1), commit(2)], null);
    const html = renderToStaticMarkup(<RunHistory runs={[run("01HZRUN0000000000000000001")]} />);
    expect(showChanges(html)).toEqual(["#/task/t1/changes/run/01HZRUN0000000000000000001"]);
    expect(text(html)).toContain("Backend Agent · backend · Manual · 2 commits Show changes");
    // a link beside the row's button, not inside it
    expect(html).toMatch(/<div class="panel-row run-line"><button class="run-open" aria-expanded="false">(?:(?!<\/button>).)*<\/button><a class="btn ghost sm"/);
  });

  it("isn't there for a run without commits, or before its commits load", () => {
    loaded.push(null, [], null);
    expect(showChanges(renderToStaticMarkup(<RunHistory runs={[run("r-empty")]} />))).toEqual([]);
    expect(showChanges(renderToStaticMarkup(<RunHistory runs={[run("r-loading", { headSha: null })]} />))).toEqual([]);
  });

  it("each run links to its own changes", () => {
    loaded.push(null, [commit(1)], null, null, [commit(2)], null);
    const html = renderToStaticMarkup(<RunHistory runs={[run("r1"), run("r2")]} />);
    expect(showChanges(html)).toEqual(["#/task/t1/changes/run/r1", "#/task/t1/changes/run/r2"]);
  });
});

const task = (more: Partial<Task> = {}) => ({
  id: "t1", identifier: "GA-12", projectId: "p1", title: "Export invoices", branch: "gizai/ga-12-export-invoices", stateId: "s-review",
  stateName: "Review", stateCategory: "review", priority: 0, labels: [], prUrl: "https://github.com/acme/shop/pull/81", prState: "open",
  createdAt: 0, updatedAt: 0, ...more,
} as Task);
const changes = (more: Partial<CardChanges> = {}): CardChanges => ({
  source: "worktree", from: "abc", to: null, baseLabel: "origin/main", additions: 1923, deletions: 112, note: null,
  files: Array.from({ length: 34 }, (_, i) => ({ path: `src/f${i}.ts`, oldPath: null, status: "modified" as const, additions: 1, deletions: 1, binary: false, uncommitted: false })),
  ...more,
});

describe("the Pull request panel", () => {
  it("shows the card's counts and Show changes, which opens the Changes tab", () => {
    const html = renderToStaticMarkup(<PullPanel task={task()} live={false} changes={changes()} />);
    expect(html).toContain('<span class="diffstat"><span class="add">+1,923</span><span class="del">−112</span></span>');
    expect(html).toMatch(/<a class="link"[^>]*href="#\/task\/t1\/changes">Show changes<\/a>/);
  });
  it("shows neither without changes, or while they load", () => {
    for (const c of [null, undefined, changes({ files: [], additions: 0, deletions: 0 })]) {
      const html = renderToStaticMarkup(<PullPanel task={task()} live={false} changes={c} />);
      expect(html).toContain('aria-label="Pull request"');
      expect(html).not.toContain("Show changes");
      expect(html).not.toContain("diffstat");
    }
  });
});

describe("Properties", () => {
  const team = { id: "team", name: "Software", members: [], labels: [], states: [{ id: "s-review", name: "Review", category: "review", sortKey: "a4" }] } as unknown as Team;
  it("counts the changes under Branch, linking to the Changes tab", () => {
    const html = renderToStaticMarkup(<Properties task={task()} team={team} people={[]} onError={() => {}} onClose={() => {}} changes={changes()} />);
    expect(html).toMatch(/<a class="v" href="#\/task\/t1\/changes" title="Show what this card changed">.*?\+1,923.*?−112.*?34 files<\/span><\/a>/);
    expect(text(html).indexOf("Branch")).toBeLessThan(text(html).indexOf("Changes"));
  });
  it("has no Changes row without changes", () => {
    const html = renderToStaticMarkup(<Properties task={task()} team={team} people={[]} onError={() => {}} onClose={() => {}} changes={changes({ files: [] })} />);
    expect(html).not.toContain("Show what this card changed");
    const none = renderToStaticMarkup(<Properties task={task()} team={team} people={[]} onError={() => {}} onClose={() => {}} />);
    expect(none).not.toContain("Show what this card changed");
  });
});
