// GA-60: a Bitbucket project names Bitbucket on the task page's pull request, the board badge and the project page; a GitHub
// project reads exactly as before, and a plain git link has no pull request. Rendered to HTML on the server: the project is
// handed in through a mocked useData, and PullPanel's state in the order of its useState calls (busy, err, note, pushed,
// checkErr). The activity's words are in lib/activity.test.ts.
import { describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Project, Task, WorkflowState } from "../types";

const queue: unknown[] = [];
let sets: unknown[][] | null = null;
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => {
    // The board's open rails start as an empty Set: here, a set that holds every column (all rails open).
    const v = queue.length ? queue.shift() : init instanceof Set && init.size === 0 ? { has: () => true } : init;
    if (!sets) return R.useState(v);
    const mine: unknown[] = [];
    sets.push(mine);
    return [v, (x: unknown) => mine.push(x)];
  }) as typeof R.useState;
  const useEffect = ((f: () => void, deps?: unknown[]) => (sets ? undefined : R.useEffect(f, deps))) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
const data: unknown[] = [];
vi.mock("../lib/useData", () => ({ useData: () => ({ data: data.length ? data.shift() : null, error: null, reload: () => {}, setData: () => {} }) }));
const answers: Record<string, (...a: unknown[]) => unknown> = {};
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) out[k] = typeof v !== "function" ? v : (...a: unknown[]) => Promise.resolve().then(() => answers[k]?.(...a));
  return out;
});

const { PullPanel } = await import("./PullPanel");
const { Board } = await import("./Board");
const { ProjectPage } = await import("../pages/ProjectPage");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&lt;/g, "<")
  .replace(/&gt;/g, ">").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();

const GH = "https://github.com/acme/shop";
const BB = "https://bitbucket.org/acme/shop";
const project = (repoUrl: string | null): Project => ({ id: "p1", number: "P-1", key: "SHOP", name: "Shop", status: "active", repoPath: "/home/you/shop",
  repoUrl, defaultBranch: "main", openTasks: 1, doneTasks: 0, updatedAt: 0, aiCostUsdMicros: 0, aiUnknownCostRuns: 0, worktreeCopy: [], worktreeInstall: true });
const task = (more: Partial<Task> = {}): Task => ({
  id: "t1", identifier: "SHOP-1", projectId: "p1", title: "Export", descriptionMd: "", stateId: "s-review", stateName: "Review", stateCategory: "review",
  priority: 0, labels: [], bounceCount: 0, failCount: 0, sortKey: "a0", testing: true, createdAt: 0, updatedAt: 0, branch: "gizai/shop-1-export", ...more,
} as Task);

function panel(repoUrl: string | null, t: Task, state: { pushed?: string; checkErr?: string } = {}) {
  data.push(project(repoUrl));
  queue.push(false, null, null, state.pushed ?? null, state.checkErr ?? null);
  try { return renderToStaticMarkup(<PullPanel task={t} live={false} />); } finally { queue.length = 0; data.length = 0; }
}

function expand(node: ReactNode): ReactNode {
  if (Array.isArray(node)) return node.map(expand);
  if (node && typeof node === "object" && "props" in node) {
    const el = node as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") return expand((el.type as (p: unknown) => ReactNode)(el.props));
    return { ...el, props: { ...el.props, children: expand(el.props.children as ReactNode) } } as ReactNode;
  }
  return node;
}
function findAll(node: ReactNode, pred: (p: Record<string, unknown>) => boolean, out: Record<string, unknown>[] = []) {
  if (Array.isArray(node)) node.forEach((n) => findAll(n, pred, out));
  else if (node && typeof node === "object" && "props" in node) {
    const p = (node as ReactElement<Record<string, unknown>>).props;
    if (pred(p)) out.push(p);
    findAll(p.children as ReactNode, pred, out);
  }
  return out;
}
const label = (p: Record<string, unknown>) => [p.children].flat(Infinity).filter((c) => typeof c === "string").join("");
/** What Open pull request says after it pushed: PullPanel called directly, its setters recorded. */
async function pushedText(repoUrl: string) {
  sets = [];
  data.push(project(repoUrl));
  try {
    const t = expand(PullPanel({ task: task(), live: false }));
    const got = sets;
    const button = findAll(t, (p) => typeof p.onClick === "function" && label(p) === "Open pull request")[0] as { onClick: () => Promise<void> };
    await button.onClick();
    return got[3].filter(Boolean);
  } finally { sets = null; data.length = 0; queue.length = 0; }
}

describe("the task page's pull request", () => {
  it("offers Open pull request on a Bitbucket project, saying it pushes with your SSH keys and opens it on Bitbucket", () => {
    const t = text(panel(BB, task()));
    expect(t).toContain("Open pull request");
    expect(t).toContain("Pushes gizai/shop-1-export with your SSH keys and opens a pull request into main on Bitbucket. When it is merged, the card moves to Done.");
    expect(t).not.toContain("GitHub");
    expect(t).not.toContain(" gh");
  });
  it("says GitHub and gh for a GitHub project, as before", () => {
    const t = text(panel(GH, task()));
    expect(t).toContain("Open pull request");
    expect(t).toContain("Pushes gizai/shop-1-export to GitHub with your git login and opens a pull request into main with gh. When it is merged, the card moves to Done.");
    expect(t).not.toContain("Bitbucket");
  });
  it("has no pull request for a plain git link or no link", () => {
    expect(panel("git@gitlab.com:acme/shop.git", task())).toBe("");
    expect(panel(null, task())).toBe("");
  });
  it("links a Bitbucket pull request by its number, or On Bitbucket without one, with Bitbucket's words under it", () => {
    const html = panel(BB, task({ prUrl: `${BB}/pull-requests/12`, prState: "open" }));
    expect(html).toContain(`href="${BB}/pull-requests/12"`);
    expect(text(html)).toContain("Pull request #12 Open");
    expect(text(html)).toContain("When it is merged on Bitbucket, the card moves to Done and its worktree is removed.");
    expect(text(panel(BB, task({ prUrl: `${BB}/pull-requests`, prState: "open" })))).toContain("Pull request On Bitbucket Open");
    expect(text(panel(GH, task({ prUrl: `${GH}/pulls`, prState: "open" })))).toContain("Pull request On GitHub Open");
    expect(text(panel(BB, task({ prUrl: `${BB}/pull-requests/12`, prState: "merged", stateCategory: "done" })))).toContain("Merged on Bitbucket.");
  });
  it("says Couldn't ask Bitbucket, or Couldn't ask GitHub as before", () => {
    expect(text(panel(BB, task({ prUrl: `${BB}/pull-requests/12`, prState: "open" }), { checkErr: "no answer" }))).toContain("Couldn't ask Bitbucket: no answer");
    const gh = text(panel(GH, task({ prUrl: `${GH}/pull/12`, prState: "open" }), { checkErr: "no answer" }));
    expect(gh).toContain("Couldn't ask GitHub: no answer");
    expect(gh).not.toContain("Bitbucket");
  });
  it("says Pushed … to Bitbucket after Open pull request on a Bitbucket project, and to GitHub on a GitHub one", async () => {
    answers.openPullRequest = () => ({ url: `${BB}/pull-requests/12`, number: 12, state: "open", note: null });
    expect(await pushedText(BB)).toEqual(["Pushed gizai/shop-1-export to Bitbucket; pull request #12 is open."]);
    answers.openPullRequest = () => ({ url: `${GH}/pull/12`, number: 12, state: "open", note: null });
    expect(await pushedText(GH)).toEqual(["Pushed gizai/shop-1-export to GitHub; pull request #12 is open."]);
  });
});

describe("the board badge", () => {
  const states: WorkflowState[] = [{ id: "s-review", name: "Review", category: "review", sortKey: "a4" }];
  it("names Bitbucket in the title of a Bitbucket pull request's badge, and GitHub for GitHub's", () => {
    const html = renderToStaticMarkup(<Board states={states} onMove={() => {}} onOpen={() => {}}
      tasks={[task({ prUrl: `${BB}/pull-requests/12`, prState: "open" }), task({ id: "t2", identifier: "SHOP-2", prUrl: `${GH}/pull/7`, prState: "merged" })]} />);
    expect(html).toContain('<span class="badge info" title="Pull request: open on Bitbucket">PR #12</span>');
    expect(html).toContain('<span class="badge ok" title="Pull request: merged on GitHub">PR #7</span>');
  });
});

describe("the project page", () => {
  const page = (repoUrl: string | null) => {
    data.push(project(repoUrl), []);
    try { return renderToStaticMarkup(<ProjectPage id="p1" />); } finally { data.length = 0; }
  };
  it("labels a Bitbucket link Bitbucket", () => {
    const html = page(BB);
    expect(html).toMatch(/<span class="k">Bitbucket<\/span><span class="mono"[^>]*><a href="https:\/\/bitbucket\.org\/acme\/shop">bitbucket\.org\/acme\/shop<\/a>/);
    expect(html).not.toContain(">GitHub<");
  });
  it("labels a GitHub link GitHub, as before", () => {
    const html = page(GH);
    expect(html).toMatch(/<span class="k">GitHub<\/span><span class="mono"[^>]*><a href="https:\/\/github\.com\/acme\/shop">github\.com\/acme\/shop<\/a>/);
    expect(html).not.toContain("Bitbucket");
  });
  it("labels another git link, or none, Remote", () => {
    expect(page("ssh://git@example.com/shop.git")).toMatch(/<span class="k">Remote<\/span><span class="mono"[^>]*><a href="ssh:\/\/git@example\.com\/shop\.git">/);
    expect(text(page(null))).toContain("Remote Not linked: cards start from the local branch");
  });
});
