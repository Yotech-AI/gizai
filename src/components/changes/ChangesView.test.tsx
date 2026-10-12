// GA-112 QA: the Changes view of a card as D1 draws it, rendered to HTML on the server (effects don't run, so no file's
// diff loads): the heading for each state (worktree, no remote, branch only, merged, a pull request's diff), the counts,
// the files not committed yet, the file list's folders, letters and counts, the folded files with their notes, and the
// empty, loading and failed states.
import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { CardChanges, FileChange, Task } from "../../types";

vi.mock("../../api", () => ({ runChanges: () => new Promise(() => {}), runCommits: () => new Promise(() => {}), cardFileDiff: () => new Promise(() => {}) }));

const { ChangesView } = await import("./ChangesView");

const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/\s+/g, " ").trim();
const task = (more: Partial<Task> = {}) => ({ id: "t1", identifier: "GA-12", title: "Export invoices", branch: "gizai/ga-12-export-invoices", ...more } as Task);
const file = (path: string, more: Partial<FileChange> = {}): FileChange => ({
  path, oldPath: null, status: "modified", additions: 1, deletions: 1, binary: false, uncommitted: false, ...more,
});
const FILES: FileChange[] = [
  file("crates/gizai-agents/src/editor.rs", { status: "added", additions: 35, deletions: 0 }),
  file("crates/gizai-agents/src/old_folders.rs", { status: "deleted", additions: 0, deletions: 45 }),
  file("crates/gizai-core/src/folder_rules.rs", { status: "renamed", oldPath: "crates/gizai-core/src/folders.rs", additions: 0, deletions: 0 }),
  file("package-lock.json", { additions: 412, deletions: 20 }),
  file("src/assets/editor-empty.png", { binary: true, additions: null, deletions: null }),
  file("src/components/Sidebar.tsx", { additions: 7, deletions: 2 }),
  file("src/components/editor/EditorSection.tsx", { status: "added", additions: 25, deletions: 0, uncommitted: true }),
  file("src/lib/sidebar.ts", { uncommitted: true }),
];
const changes = (more: Partial<CardChanges> = {}): CardChanges => ({
  source: "worktree", from: "1111111", to: null, baseLabel: "origin/main", additions: 481, deletions: 69, files: FILES, note: null, ...more,
});
const view = (data: CardChanges | null, more: { error?: string; runId?: string; t?: Task } = {}) =>
  renderToStaticMarkup(<ChangesView task={more.t ?? task()} card={{ data, error: more.error ?? null, reload: () => {} }} runId={more.runId} runs={[]} />);

describe("the Changes view of a card in its worktree", () => {
  const html = view(changes());
  const t = text(html);

  it("says Since origin/main, the counts, the files and how many aren't committed yet", () => {
    expect(html).toContain('<div class="src"><span>Since <span class="mono">origin/main</span></span>');
    expect(t).toContain("+481 −69 · 8 files");
    expect(html).toContain('<div class="unc"><span class="chg-dot"></span>2 files not committed yet</div>');
    expect(html).toContain('aria-label="Fold the file list"');
    expect(html).toContain('placeholder="Filter files…"');
  });

  it("lists the files under their folders, folders in path order and the top level last", () => {
    const dirs = [...html.matchAll(/<button class="chg-dir"[^>]*title="([^"]+)"/g)].map((m) => m[1]);
    expect(dirs).toEqual(["crates/gizai-agents/src", "crates/gizai-core/src", "src/assets", "src/components", "src/components/editor", "src/lib"]);
    const rows = [...html.matchAll(/<button class="chg-file[^"]*" data-path="([^"]+)"/g)].map((m) => m[1]);
    expect(rows).toHaveLength(8);
    expect(rows[rows.length - 1]).toBe("package-lock.json");
    // the boxes on the right come in the list's order
    const boxes = [...html.matchAll(/<section class="chg-box[^"]*" data-path="([^"]+)"/g)].map((m) => m[1]);
    expect(boxes).toEqual(rows);
    // the first file is the current one
    expect(html).toContain('class="chg-file on" data-path="crates/gizai-agents/src/editor.rs"');
  });

  it("gives each file its letter, its counts, Binary or Renamed, and a dot when not committed", () => {
    const row = (path: string) => html.match(new RegExp(`<button class="chg-file[^"]*" data-path="${path.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&")}"(?:(?!</button>).)*</button>`))![0];
    expect(row("crates/gizai-agents/src/editor.rs")).toContain('<span class="chg-st a" title="Added" aria-label="Added">A</span>');
    expect(text(row("crates/gizai-agents/src/editor.rs"))).toContain("editor.rs +35");
    expect(row("crates/gizai-agents/src/old_folders.rs")).toContain('aria-label="Deleted">D</span>');
    expect(text(row("crates/gizai-agents/src/old_folders.rs"))).toContain("−45");
    expect(row("crates/gizai-core/src/folder_rules.rs")).toContain('aria-label="Renamed">R</span>');
    expect(text(row("crates/gizai-core/src/folder_rules.rs"))).toBe("R folders.rs folder_rules.rs Renamed");
    expect(row("crates/gizai-core/src/folder_rules.rs")).toContain('title="crates/gizai-core/src/folders.rs → crates/gizai-core/src/folder_rules.rs"');
    expect(row("src/assets/editor-empty.png")).toContain('<span class="bin">Binary</span>');
    expect(text(row("src/components/Sidebar.tsx"))).toBe("M Sidebar.tsx +7 −2");
    expect(row("src/lib/sidebar.ts")).toContain('class="chg-dot" title="Not committed yet"');
    expect(row("src/components/Sidebar.tsx")).not.toContain("chg-dot");
  });

  it("folds the lock file and the long deleted file with a note and Show, and says a rename without changes is one", () => {
    const box = (path: string) => html.match(new RegExp(`<section class="chg-box[^"]*" data-path="${path.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&")}"(?:(?!</section>).)*</section>`))![0];
    const lock = box("package-lock.json");
    expect(lock).toContain('class="chg-box folded"');
    expect(lock).toContain('<span class="why">Lock file, folded</span>');
    expect(text(lock)).toContain("A lock file: what npm wrote for the packages. Show diff");
    const gone = box("crates/gizai-agents/src/old_folders.rs");
    expect(gone).toContain('<span class="why">Deleted, folded</span>');
    expect(text(gone)).toContain("Deleted: 45 lines. Show");
    const renamed = box("crates/gizai-core/src/folder_rules.rs");
    expect(renamed).toContain('<span class="chg-caret-off"></span>');
    expect(renamed).toContain('<span class="why">Renamed, no changes</span>');
    expect(renamed).not.toContain("chg-wait");
    const open = box("src/components/Sidebar.tsx");
    expect(open).not.toContain("folded");
    expect(open).toContain('aria-expanded="true" title="Fold this file"');
    expect(open).toContain('<span class="chg-path" title="src/components/Sidebar.tsx"><span class="dir">src/components/</span><b>Sidebar.tsx</b></span>');
    expect(box("src/lib/sidebar.ts")).toContain('<span class="badge">Not committed</span>');
  });

  it("keeps a file's room before its diff loads, and loads nothing far from the screen", () => {
    expect(html).toMatch(/<section class="chg-box" data-path="src\/components\/Sidebar.tsx" aria-label="src\/components\/Sidebar.tsx">(?:(?!<\/section>).)*<div class="chg-wait" style="--rows-h:348px"><\/div>/);
    expect(html).not.toContain("Loading Sidebar.tsx");
    expect(html).not.toContain("cm-editor");
  });

  it("lists its keys", () => {
    expect(t).toContain("n p next, previous file");
    expect(t).toContain("Alt ↓ Alt ↑ next, previous change");
    expect(t).toContain("[ fold this list");
    expect(t).toContain("Esc back to Card");
  });
});

describe("the Changes view's heading in each state", () => {
  const src = (html: string) => text(html.match(/<div class="src">(.*?)<\/div>/)![1]);
  it("no remote: Since main", () => {
    expect(src(view(changes({ baseLabel: "main" })))).toBe("Since main");
  });
  it("branch only: From branch <branch> (its worktree was removed), the branch in mono", () => {
    // committed only: a branch has no files waiting to be committed
    const html = view(changes({ source: "branch", to: "2222222", note: "From the card's branch.", files: FILES.map((f) => ({ ...f, uncommitted: false })) }));
    expect(src(html)).toBe("From branch gizai/ga-12-export-invoices (its worktree was removed)");
    expect(html).toContain('<span class="mono">gizai/ga-12-export-invoices</span>');
    expect(html).not.toContain("not committed yet");
  });
  it("merged: what the backend says", () => {
    expect(src(view(changes({ source: "merge", note: "As merged into main on 10 Oct (pull request #81)." })))).toBe("As merged into main on 10 Oct (pull request #81)");
    expect(src(view(changes({ source: "merge", note: "From the merge commit 3f5c1a2 on main." })))).toBe("From the merge commit 3f5c1a2 on main");
  });
  it("a pull request's diff: folds can't open", () => {
    expect(src(view(changes({ source: "pull", note: "From GitHub's diff of pull request #12." })))).toBe("From GitHub's diff of pull request #12 (folds can't open)");
  });
});

describe("nothing to show, loading and failures", () => {
  it("while the files load", () => {
    const html = view(null);
    expect(text(html)).toBe("Changes Loading the changed files…");
    expect(html).toContain('<div class="tot faint">Loading the changed files…</div>');
  });
  it("no branch yet: No changes yet, and Back to Card", () => {
    const t = text(view(changes({ source: "none", files: [], additions: 0, deletions: 0, from: null, note: "No branch yet." })));
    expect(t).toBe("No changes yet The first run makes this card's branch. Back to Card");
  });
  it("a branch without changes: No changes against the base", () => {
    expect(text(view(changes({ files: [], additions: 0, deletions: 0 })))).toBe("No changes This card's branch has no changes against origin/main.");
    expect(text(view(changes({ files: [], additions: 0, deletions: 0, note: "The merge had no changes." })))).toBe("No changes The merge had no changes.");
  });
  it("can't find what the card changed: says what to do, and opens GitHub settings when it names them", () => {
    const html = view(null, { error: "Gizai can't find what GA-12 changed: fetch main, or log gh in (Settings → GitHub)" });
    expect(text(html)).toBe("Gizai can't find what this card changed Fetch main, or log gh in (Settings GitHub). Open GitHub settings");
    expect(html).toContain('aria-label="then"'); // the arrow, drawn as an icon
    expect(html).toContain('href="#/settings/github"');
    const bb = view(null, { error: "Gizai can't find what GA-12 changed: log in to Bitbucket (Settings → Bitbucket)." });
    expect(text(bb)).toContain("Open Bitbucket settings");
    const plain = text(view(null, { error: "Gizai can't find what GA-12 changed: it was merged before Gizai saved its range." }));
    expect(plain).toBe("Gizai can't find what this card changed It was merged before Gizai saved its range. Back to Card");
  });
  it("another failure: the error and Try again", () => {
    const html = view(null, { error: "git failed: bad object" });
    expect(html).toContain('<div class="error-banner" role="alert">');
    expect(text(html)).toBe("git failed: bad object Try again");
  });
  it("one run's changes, while they load", () => {
    const html = view(changes(), { runId: "01HZRUN1" });
    expect(text(html)).toBe("One run's changes Loading the changed files…");
  });
});

describe("the current file when the view opens", () => {
  it("is the first file on the right, at the top of the diffs, also when a top-level file comes first in path order", () => {
    // git's path order puts package.json before src/…, but the list and the diffs show folders first, the top level last
    const html = view(changes({ files: [file("package.json"), file("src/App.tsx"), file("src/lib/x.ts")] }));
    const boxes = [...html.matchAll(/<section class="chg-box[^"]*" data-path="([^"]+)"/g)].map((m) => m[1]);
    expect(boxes).toEqual(["src/App.tsx", "src/lib/x.ts", "package.json"]);
    expect(html.match(/class="chg-file on" data-path="([^"]+)"/)?.[1]).toBe(boxes[0]);
  });
});

describe("a deleted file's counts", () => {
  it("are +0 −45 in the list and in its header, as D1 writes them (design/code-editor/changes.html)", () => {
    const html = view(changes({ files: [file("crates/gizai-agents/src/old_folders.rs", { status: "deleted", additions: 0, deletions: 45 })] }));
    const stat = '<span class="diffstat"><span class="add">+0</span><span class="del">−45</span></span>';
    expect(html.match(/<button class="chg-file[^"]*"(?:(?!<\/button>).)*<\/button>/)?.[0]).toContain(stat);
    expect(html.match(/<header class="chg-fh">(?:(?!<\/header>).)*<\/header>/)?.[0]).toContain(stat);
  });
});
