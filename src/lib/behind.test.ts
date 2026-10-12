// GA-114 QA: behind main in the Editor (F3), its words against D2's design (design/code-editor/editor.html §5): the top
// bar's standing, the bar under it (behind with Update, each refusal, Update running, done, stopped with its output, didn't
// start, a fetch that failed, no remote), Fetch's tooltip, the sidebar row's word and tooltip and the folded heading. The
// FolderMain answers are as contract C3's commands give them (crates/gizai-agents/src/behind.rs writes the sentences).
import { describe, expect, it } from "vitest";
import type { EditorUpdated, FolderGit, FolderMain } from "../types";
import { FETCH_EVERY, fetchDue, fetchTitle, foldedBehind, fullStop, hasMain, mainBar, notGit, rowTip, rowWord, standing, standingSentences } from "./behind";

const NOW = Date.UTC(2026, 9, 12, 12, 0, 0);
const MIN = 60_000;
const main = (more: Partial<FolderMain> = {}): FolderMain => ({
  mainLabel: "origin/main", fetchedAt: NOW - 2 * MIN, fetchError: null, branch: "main", onDefault: true, behind: 3, ahead: 0, uncommitted: 0,
  canUpdate: true, updateSays: "move main 3 commits to 319bf0b and run composer install and npm ci", updating: false, ...more,
});
const refused = (updateSays: string, more: Partial<FolderMain> = {}) => main({ canUpdate: false, updateSays, ...more });
const git = (uncommitted: number, branch: string | null = "main"): FolderGit => ({ branch, files: {}, uncommitted });
const top = (m: FolderMain | null, g: FolderGit | null = null) => { const s = standing(m, g, NOW); return [s.branch, ...s.words].filter(Boolean).join(" · "); };
const opts = (more: Partial<Parameters<typeof mainBar>[1]> = {}) => ({ name: "kade-portal", ended: null, refusal: null, starting: false, now: NOW, ...more });
/** The bar's text as the screen reads it: the bold start, then the rest. */
const say = (m: FolderMain | null, more: Partial<Parameters<typeof mainBar>[1]> = {}) => {
  const b = mainBar(m, opts(more));
  if (!b) return null;
  return "lead" in b ? `${b.lead ?? ""}${b.rest}` : b.text;
};

describe("the top bar's standing, as in each of §5's screens", () => {
  it("behind: 'main · 3 behind origin/main', with the files changed and the commits ahead after it", () => {
    expect(top(main())).toBe("main · 3 behind origin/main");
    expect(top(main({ behind: 1 }))).toBe("main · 1 behind origin/main");
    expect(top(main(), git(2))).toBe("main · 3 behind origin/main · 2 changed");
    expect(top(main({ ahead: 1 }))).toBe("main · 3 behind origin/main · 1 ahead");
    expect(top(main({ branch: "feature/pdf-export", onDefault: false }))).toBe("feature/pdf-export · 3 behind origin/main");
  });

  it("up to date, not fetched yet, no remote, not a git repository", () => {
    expect(top(main({ behind: 0 }))).toBe("main · up to date with origin/main");
    expect(top(main({ behind: 0, fetchedAt: null }))).toBe("main · not fetched yet");
    expect(top(main({ mainLabel: "", fetchedAt: null, behind: 0, canUpdate: false, updateSays: "kade-portal has no origin remote: Gizai can't tell whether it's behind." }))).toBe("main · no remote");
    const outside = { ...main(), mainLabel: "", branch: null, fetchedAt: null, behind: 0, canUpdate: false, updateSays: "notes isn't a git repository, so there's no main to compare it with." };
    expect(notGit(outside)).toBe(true);
    expect(top(outside)).toBe("Not a git repository");
  });

  it("the Editor's git status counts the changed files when it has it (it follows a save at once), else main's count", () => {
    expect(top(main({ uncommitted: 4 }))).toBe("main · 3 behind origin/main · 4 changed");
    expect(top(main({ uncommitted: 4 }), git(1))).toBe("main · 3 behind origin/main · 1 changed");
  });

  it("before editor_main answers, the branch and the changed files from git's status", () => {
    expect(top(null, git(2, "main"))).toBe("main · 2 changed");
    expect(standing(null, null, NOW)).toEqual({ branch: null, words: [], tip: "" });
  });

  it("no branch: says so where the branch goes (the design adds the commit, 'no branch (3f5c1a2)', which C3 doesn't carry)", () => {
    expect(top(main({ branch: null, onDefault: false }))).toBe("no branch · 3 behind origin/main");
  });

  it("its tooltip says it in sentences, with when main was fetched", () => {
    expect(standing(main(), null, NOW).tip).toBe("main is 3 commits behind origin/main, fetched 2m ago");
    expect(standing(main({ behind: 1, ahead: 1, fetchedAt: NOW - 5_000 }), git(4), NOW).tip)
      .toBe("main is 1 commit behind origin/main, fetched just now · main has 1 commit that origin/main doesn't · 4 files changed, not committed");
    expect(standingSentences(main({ behind: 0, fetchedAt: null }), null, NOW)).toEqual(["main, origin/main not fetched yet"]);
    expect(standingSentences(main({ behind: 0 }), null, NOW)).toEqual(["main, up to date with origin/main"]);
    expect(standingSentences(main({ updating: true }), null, NOW)).toEqual(["Updating: move main 3 commits to 319bf0b and run composer install and npm ci"]);
  });
});

describe("the bar under the top bar: behind, with Update and what it will do", () => {
  it("3 commits behind, fetched 2m ago: the plan's words, a primary Update with the plan as its tooltip", () => {
    const b = mainBar(main(), opts());
    expect(b).toEqual({
      kind: "behind", lead: "3 commits behind origin/main", update: "on",
      rest: ", fetched 2m ago. Update will move main 3 commits to 319bf0b and run composer install and npm ci.",
      tip: "Move main 3 commits to 319bf0b and run composer install and npm ci",
    });
  });

  it("1 commit behind, fetched just now, nothing to install", () => {
    expect(say(main({ behind: 1, fetchedAt: NOW - 20_000, updateSays: "move main 1 commit to 7c2e0aa" })))
      .toBe("1 commit behind origin/main, fetched just now. Update will move main 1 commit to 7c2e0aa.");
  });

  it("up to date, or no main to compare with: no bar", () => {
    expect(mainBar(main({ behind: 0, canUpdate: false, updateSays: "kade-portal is up to date with origin/main." }), opts())).toBeNull();
    expect(mainBar(main({ mainLabel: "", behind: 0, fetchedAt: null, canUpdate: false, updateSays: "kade-portal has no origin remote: Gizai can't tell whether it's behind." }), opts())).toBeNull();
    expect(mainBar(null, opts())).toBeNull();
  });
});

describe("a refused Update: the bar says why after 'Update can't run:', Update stays, off, with the same sentence as its tooltip", () => {
  const cases: [string, FolderMain][] = [
    ["kade-portal has uncommitted changes in app/Models/Invoice.php and routes/web.php. Commit or stash them first.", refused("kade-portal has uncommitted changes in app/Models/Invoice.php and routes/web.php. Commit or stash them first.", { uncommitted: 2 })],
    ["a merge is in progress in kade-portal. Finish or abort it first.", refused("a merge is in progress in kade-portal. Finish or abort it first.", { uncommitted: 4 })],
    ["kade-portal is on branch feature/pdf-export, not main. Switch to main first, in a terminal or your own editor.", refused("kade-portal is on branch feature/pdf-export, not main. Switch to main first, in a terminal or your own editor.", { branch: "feature/pdf-export", onDefault: false })],
    ["kade-portal has no branch checked out. Switch to main first, in a terminal or your own editor.", refused("kade-portal has no branch checked out. Switch to main first, in a terminal or your own editor.", { branch: null, onDefault: false })],
    ["main in kade-portal has 1 commit that origin/main doesn't, so a fast-forward can't bring it up to date. Push or merge it first, in a terminal or your own editor.", refused("main in kade-portal has 1 commit that origin/main doesn't, so a fast-forward can't bring it up to date. Push or merge it first, in a terminal or your own editor.", { ahead: 1 })],
  ];
  it.each(cases)("%s", (why, m) => {
    const b = mainBar(m, opts());
    expect(b).toEqual({ kind: "refused", lead: "3 commits behind origin/main", rest: `, fetched 2m ago. Update can't run: ${why}`, tip: `Update can't run: ${why}`, update: "off" });
  });

  it("refused when clicked while the folder's own answer still lets it run: the bar says why, Update stays on to try again", () => {
    const b = mainBar(main(), opts({ refusal: "kade-portal has uncommitted changes in routes/web.php. Commit or stash them first." }));
    expect(b).toEqual({
      kind: "refused", lead: "3 commits behind origin/main", update: "on",
      rest: ", fetched 2m ago. Update can't run: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.",
      tip: "Update can't run: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.",
    });
  });

  it("another update of the folder already runs: says so, without Update", () => {
    expect(mainBar(main(), opts({ refusal: "An update of kade-portal is already running." }))).toEqual({ kind: "busy", text: "An update of kade-portal is already running." });
  });

  it("a refusal for a folder without a main still shows, after 'Update can't run:'", () => {
    const b = mainBar(main({ mainLabel: "", behind: 0, canUpdate: false, updateSays: "x" }), opts({ refusal: "kade-portal has no origin remote: Gizai can't tell whether it's behind" }));
    expect(b).toMatchObject({ kind: "refused", update: "off", rest: "Update can't run: kade-portal has no origin remote: Gizai can't tell whether it's behind." });
  });
});

describe("Update running, and how it ended (editor-updated)", () => {
  const done: EditorUpdated = { folder: "/f", ok: true, said: "Moved main 3 commits to 319bf0b and ran composer install and npm ci.", output: "added 812 packages in 9s\n" };
  const stopped: EditorUpdated = {
    folder: "/f", ok: false, said: "The update of kade-portal stopped: npm ci failed (exit code 1). Before that it moved main 3 commits to 319bf0b and ran composer install.",
    output: "npm error code EBADENGINE\nnpm error engine Unsupported engine\n",
  };
  const didnt: EditorUpdated = { folder: "/f", ok: false, said: "The update of kade-portal didn't start: kade-portal has uncommitted changes in routes/web.php. Commit or stash them first.", output: "" };

  it("updating: the plan's words with an ellipsis, from the click until it ends (here or from the Team Lead's chat)", () => {
    expect(mainBar(main(), opts({ starting: true }))).toEqual({ kind: "updating", text: "Updating kade-portal: move main 3 commits to 319bf0b and run composer install and npm ci…" });
    expect(mainBar(main({ updating: true, canUpdate: false }), opts())).toEqual({ kind: "updating", text: "Updating kade-portal: move main 3 commits to 319bf0b and run composer install and npm ci…" });
    expect(mainBar(main({ updating: true, canUpdate: false, updateSays: "" }), opts())).toEqual({ kind: "updating", text: "Updating kade-portal…" });
  });

  it("done: 'Updated kade-portal: moved main …', with its output to show", () => {
    expect(mainBar(main({ behind: 0, canUpdate: false }), opts({ ended: done })))
      .toEqual({ kind: "done", text: "Updated kade-portal: moved main 3 commits to 319bf0b and ran composer install and npm ci.", output: "added 812 packages in 9s\n" });
  });

  it("stopped: what failed and what it did before, with the output", () => {
    expect(mainBar(main({ behind: 0, canUpdate: false }), opts({ ended: stopped }))).toEqual({ kind: "stopped", text: stopped.said, output: stopped.output });
  });

  it("didn't start: why, as the backend says it, with nothing to show", () => {
    expect(mainBar(main({ uncommitted: 1, canUpdate: false }), opts({ ended: didnt }))).toEqual({ kind: "didnt-start", text: didnt.said, output: "" });
  });

  it("an update that runs again comes before how the last one ended", () => {
    expect(mainBar(main({ updating: true }), opts({ ended: done }))?.kind).toBe("updating");
  });
});

describe("a fetch that failed, with Try again", () => {
  const why = "Couldn't fetch main from origin: could not resolve host github.com";
  it("the counts are from the last fetch, 3h ago", () => {
    expect(mainBar(main({ fetchedAt: NOW - 3 * 60 * MIN, fetchError: why }), opts()))
      .toEqual({ kind: "fetch-failed", text: "Couldn't fetch main from origin: could not resolve host github.com. The counts are from the last fetch, 3h ago." });
  });
  it("never fetched: Gizai can't tell yet whether the folder is behind", () => {
    expect(mainBar(main({ fetchedAt: null, behind: 0, fetchError: `${why}.` }), opts()))
      .toEqual({ kind: "fetch-failed", text: "Couldn't fetch main from origin: could not resolve host github.com. Gizai can't tell yet whether kade-portal is behind." });
  });
});

describe("Fetch: its tooltip, and when the Editor fetches again", () => {
  it("'Fetch origin/main now (last fetched 2m ago)', or not fetched yet", () => {
    expect(fetchTitle(main(), NOW)).toBe("Fetch origin/main now (last fetched 2m ago)");
    expect(fetchTitle(main({ fetchedAt: null }), NOW)).toBe("Fetch origin/main now (not fetched yet)");
  });
  it("10 minutes after the last try, not while an update runs", () => {
    expect(FETCH_EVERY).toBe(10 * MIN);
    expect(fetchDue(NOW - 10 * MIN, NOW, main())).toBe(true);
    expect(fetchDue(NOW - 10 * MIN + 1, NOW, main())).toBe(false);
    expect(fetchDue(NOW - 30 * MIN, NOW, main({ updating: true }))).toBe(false);
    expect(fetchDue(NOW - 10 * MIN, NOW, null)).toBe(true);
  });
  it("only a folder with a main has Fetch", () => {
    expect(hasMain(main())).toBe(true);
    expect(hasMain(main({ mainLabel: "" }))).toBe(false);
    expect(hasMain(null)).toBe(false);
  });
});

describe("the sidebar: the folder's row and the folded Editor heading", () => {
  it("the row says updating, then behind first, then changed, then ahead", () => {
    expect(rowWord(main({ updating: true }), git(2))).toBe("updating");
    expect(rowWord(main(), git(2))).toBe("3 behind");
    expect(rowWord(main({ behind: 0 }), git(2))).toBe("2 changed");
    expect(rowWord(main({ behind: 0, ahead: 1 }), null)).toBe("1 ahead");
    expect(rowWord(main({ behind: 0 }), git(0))).toBe("");
    expect(rowWord(main({ behind: 1_234 }), null)).toBe("1,234 behind");
  });
  it("after Update, the row loses its '3 behind'", () => {
    expect(rowWord(main(), git(0))).toBe("3 behind");
    expect(rowWord(main({ behind: 0, canUpdate: false }), git(0))).toBe("");
  });
  it("a card's worktree says only how many files changed (Update doesn't move a card's branch)", () => {
    expect(rowWord(main({ ahead: 2 }), git(0), true)).toBe("");
    expect(rowWord(main(), git(3), true)).toBe("3 changed");
  });
  it("a folder without a main says only how many files changed", () => {
    expect(rowWord(main({ mainLabel: "", behind: 0 }), git(1))).toBe("1 changed");
  });
  it("the row's tooltip: the path, then how it stands", () => {
    expect(rowTip("/home/you/Herd/kade-portal", main(), git(2), NOW))
      .toBe("/home/you/Herd/kade-portal · main is 3 commits behind origin/main, fetched 2m ago · 2 files changed, not committed");
  });
  it("folded, the heading says how many folders are behind; cards' worktrees don't count", () => {
    expect(foldedBehind([{ main: main(), card: false }, { main: main({ behind: 0 }), card: false }])).toEqual({ word: "1 behind", tip: "1 folder is behind origin/main" });
    expect(foldedBehind([{ main: main(), card: false }, { main: main({ behind: 5 }), card: false }, { main: main(), card: true }]))
      .toEqual({ word: "2 behind", tip: "2 folders are behind origin/main" });
    expect(foldedBehind([{ main: main(), card: false }, { main: main({ mainLabel: "origin/master" }), card: false }])?.tip).toBe("2 folders are behind main");
    expect(foldedBehind([{ main: main({ behind: 0 }), card: false }, { main: null, card: false }, { main: main(), card: true }])).toBeNull();
  });
});

describe("fullStop", () => {
  it("adds a full stop only when the sentence has none", () => {
    expect(fullStop("Commit or stash them first")).toBe("Commit or stash them first.");
    expect(fullStop("Commit or stash them first. ")).toBe("Commit or stash them first.");
    expect(fullStop("Already running…")).toBe("Already running…");
  });
});
