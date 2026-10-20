// GA-185 QA: the Screenshots tab's route (#/task/<id>/screenshots and …/screenshots/<run>, routed as Changes is), and the
// pictures' blob: URLs (lib/shotUrls.ts): one URL per picture while something shows it, freed when the last user lets go,
// also when it is let go while its bytes still load.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { href, parseHash } from "../router";

vi.mock("../api", () => ({ screenshotBytes: () => Promise.reject(new Error("not in this test")) }));
const { acquireShot, releaseShot, shotUrlCount } = await import("./shotUrls");

describe("the Screenshots tab's route (GA-185)", () => {
  it("opens at #/task/<id>/screenshots and one run's set at …/screenshots/<run id>", () => {
    expect(parseHash("#/task/0192-abc/screenshots")).toEqual({ page: "task", id: "0192-abc", view: "screenshots" });
    expect(parseHash("#/task/0192-abc/screenshots/01HZRUN1")).toEqual({ page: "task", id: "0192-abc", view: "screenshots", run: "01HZRUN1" });
    expect(href({ page: "task", id: "0192-abc", view: "screenshots" })).toBe("#/task/0192-abc/screenshots");
    expect(href({ page: "task", id: "0192-abc", view: "screenshots", run: "01HZRUN1" })).toBe("#/task/0192-abc/screenshots/01HZRUN1");
  });
  it("round-trips ids that need encoding, and leaves Card and Changes as they were", () => {
    for (const r of [{ page: "task", id: "a/b #c", view: "screenshots" }, { page: "task", id: "x", view: "screenshots", run: "r/1 2" }] as const) {
      expect(parseHash(href(r))).toEqual(r);
    }
    expect(parseHash("#/task/0192-abc")).toEqual({ page: "task", id: "0192-abc" });
    expect(parseHash("#/task/0192-abc/changes")).toEqual({ page: "task", id: "0192-abc", view: "changes" });
  });
});

describe("a picture's blob: URL", () => {
  let made: string[], freed: string[];
  beforeEach(() => {
    made = []; freed = [];
    vi.spyOn(URL, "createObjectURL").mockImplementation(() => { const u = `blob:test/${made.length + 1}`; made.push(u); return u; });
    vi.spyOn(URL, "revokeObjectURL").mockImplementation((u: string) => { freed.push(u); });
  });
  afterEach(() => vi.restoreAllMocks());
  const bytes = () => Promise.resolve(new ArrayBuffer(4));

  it("is made once for a picture two places show (a thumbnail and the larger view), and freed when both let go", async () => {
    const [a, b] = await Promise.all([acquireShot("f1", "01.png", bytes), acquireShot("f1", "01.png", bytes)]);
    expect(a).toBe(b);
    expect(made).toEqual([a]);
    expect(shotUrlCount()).toBe(1);
    releaseShot("f1");
    expect(freed).toEqual([]);
    releaseShot("f1");
    expect(freed).toEqual([a]);
    expect(shotUrlCount()).toBe(0);
  });
  it("let go while its bytes still load: the URL is freed as soon as it is made", async () => {
    let done!: (b: ArrayBuffer) => void;
    const p = acquireShot("f2", "02.png", () => new Promise<ArrayBuffer>((r) => { done = r; }));
    releaseShot("f2");
    done(new ArrayBuffer(4));
    await expect(p).rejects.toThrow("released");
    expect(made).toHaveLength(1);
    expect(freed).toEqual(made);
    expect(shotUrlCount()).toBe(0);
  });
  it("a picture that can't load makes no URL, and a later look tries again", async () => {
    await expect(acquireShot("f3", "03.png", () => Promise.reject(new Error("gone")))).rejects.toThrow("gone");
    expect(made).toEqual([]);
    const url = await acquireShot("f3", "03.png", bytes);
    expect(url).toBe("blob:test/1");
    releaseShot("f3");
    expect(freed).toEqual(["blob:test/1"]);
  });
  it("is typed by the file's name (PNG, JPEG or WebP)", async () => {
    const types: string[] = [];
    vi.mocked(URL.createObjectURL).mockImplementation((b) => { types.push((b as Blob).type); return `blob:t/${types.length}`; });
    for (const [id, name] of [["p", "a.png"], ["j", "b.JPG"], ["w", "c.webp"]]) { await acquireShot(id, name, bytes); releaseShot(id); }
    expect(types).toEqual(["image/png", "image/jpeg", "image/webp"]);
  });
});
