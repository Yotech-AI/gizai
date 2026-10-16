// GA-190 QA: askMain and askGit (src/lib/behind.ts) give the sidebar and the Editor page one answer: while an ask for a
// folder is on its way, asking again gets that same answer, and editor_main (some 15 to 30 git processes) or editor_git
// runs once. Once it has settled (answered or failed) the next ask asks again. The asks are fakes held until released.
import { describe, expect, it, vi } from "vitest";
import type { FolderGit, FolderMain } from "../types";
import { askGit, askMain } from "./behind";

type Held<T> = { folder: string; ok: (v: T) => void; fail: (e: unknown) => void };
/** A fake ask that records each call and holds its answer until the test settles it. */
function asker<T>() {
  const calls: Held<T>[] = [];
  const ask = vi.fn((folder: string) => new Promise<T>((ok, fail) => { calls.push({ folder, ok, fail }); }));
  return { ask, calls };
}
const main = (behind: number): FolderMain => ({
  mainLabel: "origin/main", fetchedAt: 1, fetchError: null, branch: "main", onDefault: true, behind, ahead: 0, uncommitted: 0,
  canUpdate: behind > 0, updateSays: "x", updating: false,
});
const git = (uncommitted: number): FolderGit => ({ branch: "main", files: {}, uncommitted });
const tick = () => new Promise((r) => setTimeout(r, 0));

describe("askMain: one editor_main for the sidebar and the Editor page (GA-190)", () => {
  it("two asks while one is on its way make one call, and both get its answer", async () => {
    const { ask, calls } = asker<FolderMain>();
    const sidebar = askMain("/a/one", ask);
    const page = askMain("/a/one", ask);
    expect(ask).toHaveBeenCalledTimes(1);
    expect(page).toBe(sidebar);
    calls[0]!.ok(main(3));
    expect(await sidebar).toEqual(main(3));
    expect(await page).toEqual(main(3));
  });

  it("after the answer came, a new ask asks again", async () => {
    const { ask, calls } = asker<FolderMain>();
    const first = askMain("/a/two", ask);
    calls[0]!.ok(main(1));
    await first;
    await tick();
    const second = askMain("/a/two", ask);
    expect(ask).toHaveBeenCalledTimes(2);
    expect(second).not.toBe(first);
    calls[1]!.ok(main(0));
    expect(await second).toEqual(main(0));
  });

  it("different folders are asked apart, and each shares only its own answer", async () => {
    const { ask, calls } = asker<FolderMain>();
    const a = askMain("/a/three", ask);
    const b = askMain("/a/four", ask);
    const a2 = askMain("/a/three", ask);
    expect(ask.mock.calls.map(([f]) => f)).toEqual(["/a/three", "/a/four"]);
    expect(a2).toBe(a);
    expect(b).not.toBe(a);
    calls[1]!.ok(main(4));
    calls[0]!.ok(main(5));
    expect([await a, await b, await a2].map((m) => m.behind)).toEqual([5, 4, 5]);
  });

  it("a failed answer fails for everyone who asked, and is cleared: the next ask asks again", async () => {
    const { ask, calls } = asker<FolderMain>();
    const x = askMain("/a/five", ask);
    const y = askMain("/a/five", ask);
    calls[0]!.fail("no such folder");
    await expect(x).rejects.toBe("no such folder");
    await expect(y).rejects.toBe("no such folder");
    await tick();
    const z = askMain("/a/five", ask);
    expect(ask).toHaveBeenCalledTimes(2);
    calls[1]!.ok(main(2));
    expect((await z).behind).toBe(2);
  });

  it("an ask that throws at once isn't kept either", async () => {
    let n = 0;
    const ask = (folder: string) => { n++; return Promise.reject(new Error(`can't open ${folder}`)); };
    await expect(askMain("/a/six", ask)).rejects.toThrow("can't open /a/six");
    await tick();
    await expect(askMain("/a/six", ask)).rejects.toThrow("can't open /a/six");
    expect(n).toBe(2);
  });
});

describe("askGit: one editor_git for the sidebar and the Editor page (GA-190)", () => {
  it("two asks while one is on its way make one call; after it settles a new ask asks again", async () => {
    const { ask, calls } = asker<FolderGit>();
    const sidebar = askGit("/g/one", ask);
    const page = askGit("/g/one", ask);
    expect(ask).toHaveBeenCalledTimes(1);
    calls[0]!.ok(git(4));
    expect(await sidebar).toEqual(git(4));
    expect(await page).toEqual(git(4));
    await tick();
    const later = askGit("/g/one", ask);
    expect(ask).toHaveBeenCalledTimes(2);
    calls[1]!.ok(git(0));
    expect((await later).uncommitted).toBe(0);
  });

  it("git and main for the same folder are asked apart: an editor_git on its way doesn't answer an editor_main", async () => {
    const g = asker<FolderGit>(), m = asker<FolderMain>();
    const pg = askGit("/g/two", g.ask);
    const pm = askMain("/g/two", m.ask);
    expect(g.ask).toHaveBeenCalledTimes(1);
    expect(m.ask).toHaveBeenCalledTimes(1);
    g.calls[0]!.ok(git(1));
    m.calls[0]!.ok(main(1));
    expect(await pg).toEqual(git(1));
    expect(await pm).toEqual(main(1));
  });

  it("a rejected editor_git is cleared", async () => {
    const { ask, calls } = asker<FolderGit>();
    const p = askGit("/g/three", ask);
    calls[0]!.fail(new Error("not a git repository"));
    await expect(p).rejects.toThrow("not a git repository");
    await tick();
    void askGit("/g/three", ask).catch(() => {});
    expect(ask).toHaveBeenCalledTimes(2);
  });
});
