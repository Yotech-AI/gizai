// GA-217 QA: Settings → General → Backups in the browser mode. src/browser/fixtures.json has backup_status and list_backups
// in contract BK's shape (written by crates/gizai-core/examples/fixtures.rs: a folder in Dropbox, daily, Keep 10, three
// snapshots newest first, one from another computer that can't be restored), and the fake backend in mock.ts answers the
// section's writes as GA-216's commands do: Keep outside 1 to 100 refused with the backend's reason, the status with the
// change and `backups-changed`, Back up now's steps ending in a new row, and nothing listed after Turn off.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BackupEntry, BackupStatus } from "../types";

const emitted = vi.hoisted(() => [] as [string, unknown][]);
vi.mock("@tauri-apps/api/event", () => ({ emit: async (name: string, payload: unknown) => { emitted.push([name, structuredClone(payload)]); } }));
vi.mock("./pictures", () => ({ screenshotPicture: async () => new ArrayBuffer(0) }));

const fixtures = (await import("./fixtures.json")).default as unknown as import("./mock").Fixtures;
const { answer, loadFixtures } = await import("./mock");

const HOUR = 3_600_000, DAY = 24 * HOUR;
const status = async () => (await answer("backup_status", {})) as BackupStatus;
const list = async () => (await answer("list_backups", { folder: null })) as BackupEntry[];
const save = (s: Partial<BackupStatus["settings"]>, base: BackupStatus["settings"]) => answer("save_backup_settings", { settings: { ...base, ...s } });
const changes = () => emitted.filter(([n]) => n === "backups-changed").map(([, p]) => p as BackupStatus);

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date(2026, 9, 27, 14, 0));
  vi.spyOn(console, "info").mockImplementation(() => {});
  vi.spyOn(console, "warn").mockImplementation(() => {});
  emitted.length = 0;
  loadFixtures(structuredClone(fixtures));
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); });

describe("the fixtures (fixtures.rs → fixtures.json)", () => {
  it("backup_status: a folder in Dropbox (its Gizai backups folder in use), daily, Keep 10, not on quit, nothing running or failed", async () => {
    const s = await status();
    expect(s.settings).toEqual({ folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 10 });
    expect(s.folder).toBe("/home/you/Dropbox/Gizai backups");
    expect([s.folderProblem, s.running, s.lastError]).toEqual([null, null, null]);
    expect(Object.keys(s).sort()).toEqual(["folder", "folderProblem", "last", "lastError", "next", "running", "settings"]);
  });

  it("its last backup is the newest of this computer's, 3 h ago, and the next is a day after it (times moved to now)", async () => {
    const [s, l] = [await status(), await list()];
    const mine = l.find((e) => e.thisComputer)!;
    expect(s.last?.name).toBe(mine.name);
    expect(s.last?.madeAt).toBe(mine.madeAt);
    expect(Date.now() - s.last!.madeAt).toBeGreaterThan(3 * HOUR - 60_000);
    expect(Date.now() - s.last!.madeAt).toBeLessThan(3 * HOUR + 60_000);
    expect(s.next).toBe(s.last!.madeAt + DAY);
    expect(s.last!.bytes).toBeGreaterThan(0);
  });

  it("list_backups: three snapshots, newest first, in contract BK's shape", async () => {
    const l = await list();
    expect(l).toHaveLength(3);
    expect(l.map((e) => e.madeAt)).toEqual([...l.map((e) => e.madeAt)].sort((a, b) => b - a));
    const keys = ["appVersion", "dbBytes", "deviceId", "deviceName", "filesBytes", "filesCount", "filesMissing", "madeAt", "name", "problem",
      "reason", "restorable", "schemaVersion", "thisComputer"];
    for (const e of l) expect(Object.keys(e).sort()).toEqual(keys);
    expect(l.map((e) => e.reason)).toEqual(["daily", "manual", "daily"]);
    for (const e of l) expect(e.name).toMatch(new RegExp(`^gizai-${e.reason}-\\d{8}-\\d{6}-\\d{3}-[0-9a-f]+$`));
  });

  it("one of them is another computer's (work-laptop), made by a newer Gizai, so it can't be restored and says why", async () => {
    const other = (await list()).filter((e) => !e.thisComputer);
    expect(other).toHaveLength(1);
    expect(other[0]).toMatchObject({ deviceName: "work-laptop", restorable: false });
    expect(other[0]!.problem).toMatch(/newer Gizai/);
    expect((await list()).filter((e) => e.thisComputer).every((e) => e.restorable && e.problem === null)).toBe(true);
  });
});

describe("the browser's answers to the section's writes", () => {
  it("Keep 0 and 101 are refused with the backend's reason, and nothing changes", async () => {
    const before = await status();
    await expect(save({ keep: 0 }, before.settings)).rejects.toBe("Keep must be 1 to 100 snapshots, not 0");
    await expect(save({ keep: 101 }, before.settings)).rejects.toBe("Keep must be 1 to 100 snapshots, not 101");
    expect(await status()).toEqual(before);
    expect(changes()).toEqual([]);
  });

  it("a change saves: the status with it, and backups-changed says so", async () => {
    const before = await status();
    const out = (await save({ every: "weekly", keep: 20, onQuit: true }, before.settings)) as BackupStatus;
    expect(out.settings).toEqual({ folder: "/home/you/Dropbox", every: "weekly", onQuit: true, keep: 20 });
    expect(out.next).toBe(before.last!.madeAt + 7 * DAY);
    expect(await status()).toEqual(out);
    expect(changes()).toEqual([out]);
  });

  it("Only when I ask: no next", async () => {
    const out = (await save({ every: "manual" }, (await status()).settings)) as BackupStatus;
    expect(out.next).toBeNull();
  });

  it("Turn off: no folder, no last backup, nothing listed, and Back up now refused", async () => {
    const out = (await save({ folder: null }, (await status()).settings)) as BackupStatus;
    expect([out.settings.folder, out.folder, out.last, out.next]).toEqual([null, null, null, null]);
    expect(await list()).toEqual([]);
    await expect(answer("back_up_now", {})).rejects.toBe("Choose a folder for backups first");
  });

  it("Back up now: running with each step a moment, refused while it runs, then a new By hand row on top and the last backup", async () => {
    const before = await list();
    const started = (await answer("back_up_now", {})) as BackupStatus;
    expect(started.running).toMatchObject({ reason: "manual", step: "database" });
    await expect(answer("back_up_now", {})).rejects.toBe("A backup is running already");
    const steps: string[] = [];
    for (let i = 0; i < 4; i++) {
      await vi.advanceTimersByTimeAsync(900);
      steps.push(changes().at(-1)!.running!.step);
    }
    expect(steps).toEqual(["files", "compress", "manifest", "keep"]);
    await vi.advanceTimersByTimeAsync(900);
    const done = changes().at(-1)!;
    expect(done.running).toBeNull();
    const after = await list();
    expect(after).toHaveLength(before.length + 1);
    expect(after[0]).toMatchObject({ reason: "manual", thisComputer: true, madeAt: Date.now() });
    expect(after[0]!.name).toMatch(/^gizai-manual-20261027-1400\d{2}-\d{3}-[0-9a-f]+$/);
    expect(done.last).toMatchObject({ name: after[0]!.name, madeAt: after[0]!.madeAt });
    expect(done.next).toBe(after[0]!.madeAt + DAY);
    expect(await status()).toEqual(done);
  });
});
