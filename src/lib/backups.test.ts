// GA-217 QA: Settings → General → Backups in words, from contract BK (GA-216): a snapshot's reason and step, its size and
// computer ("from another computer" when Gizai couldn't name it: "this machine" or empty), why it can't be restored, the
// last and next backup, the last error, when Back up now can start one, and Keep as typed (the backend checks the range).
import { describe, expect, it } from "vitest";
import { backupWhen, canBackUp, entryProblem, entrySize, EVERY, fromText, keepOf, KEEP_MAX, KEEP_MIN, lastErrorText, lastText, nextText,
  NO_FOLDER, reasonText, stepText } from "./backups";
import type { BackupStatus } from "../types";

// local times, so the day words don't depend on the time zone the tests run in
const NOW = new Date(2026, 9, 27, 14, 0).getTime();
const at = (day: number, h: number, m = 0) => new Date(2026, 9, day, h, m).getTime();

const STATUS: BackupStatus = {
  settings: { folder: "/home/you/Dropbox", every: "daily", onQuit: false, keep: 10 },
  folder: "/home/you/Dropbox/Gizai backups", folderProblem: null, running: null,
  last: { name: "gizai-daily-20261027-110000-000-ce82a749", madeAt: at(27, 11), bytes: 1_153_434 },
  lastError: null, next: at(28, 11),
};

describe("the section's words, as the card has them", () => {
  it("When: Daily, Weekly or Only when I ask, in that order", () => {
    expect(EVERY).toEqual([["daily", "Daily"], ["weekly", "Weekly"], ["manual", "Only when I ask"]]);
  });
  it("Folder without one: Not set: Gizai makes no backups.", () => {
    expect(NO_FOLDER).toBe("Not set: Gizai makes no backups.");
  });
  it("Keep is 1 to 100, as the backend checks it", () => {
    expect([KEEP_MIN, KEEP_MAX]).toEqual([1, 100]);
  });
});

describe("a snapshot's reason in words", () => {
  it("Daily, Weekly, By hand, On quit, Before a restore", () => {
    expect((["daily", "weekly", "manual", "quit", "before-restore"] as const).map(reasonText))
      .toEqual(["Daily", "Weekly", "By hand", "On quit", "Before a restore"]);
  });
  it("a reason a newer Gizai added shows as it is, not blank", () => {
    expect(reasonText("hourly")).toBe("hourly");
  });
});

describe("a running snapshot's step (gizai-core's Step)", () => {
  it("each of the backend's five steps in words", () => {
    expect(["database", "files", "compress", "manifest", "keep"].map(stepText)).toEqual([
      "Copying the database…", "Copying your files…", "Compressing the database…", "Writing its manifest…", "Removing older backups…",
    ]);
  });
  it("a step it doesn't know shows as it is", () => {
    expect(stepText("verify")).toBe("verify…");
  });
});

describe("a snapshot's time, size and computer", () => {
  it("today, yesterday and an older day on your clock", () => {
    expect(backupWhen(at(27, 9, 5), NOW)).toBe("today 09:05");
    expect(backupWhen(at(26, 21, 30), NOW)).toBe("yesterday 21:30");
    expect(backupWhen(at(28, 9, 0), NOW)).toBe("tomorrow 09:00");
    expect(backupWhen(at(24, 8, 0), NOW)).toMatch(/^\w{3} 24 Oct, 08:00$/);
  });
  it("its size is the database and the files together", () => {
    expect(entrySize({ dbBytes: 4_456_448, filesBytes: 3_276_800 })).toBe("7.4 MB");
    expect(entrySize({ dbBytes: 512, filesBytes: 0 })).toBe("512 B");
  });
  it("this computer's: no computer named", () => {
    expect(fromText({ thisComputer: true, deviceName: "work-laptop" })).toBeNull();
    expect(fromText({ thisComputer: true, deviceName: "this machine" })).toBeNull();
  });
  it("another computer's: from its name, or from another computer when it has none (Db::init's \"this machine\" or empty)", () => {
    expect(fromText({ thisComputer: false, deviceName: "work-laptop" })).toBe("from work-laptop");
    expect(fromText({ thisComputer: false, deviceName: "  work-laptop " })).toBe("from work-laptop");
    expect(fromText({ thisComputer: false, deviceName: "this machine" })).toBe("from another computer");
    expect(fromText({ thisComputer: false, deviceName: "" })).toBe("from another computer");
    expect(fromText({ thisComputer: false, deviceName: "   " })).toBe("from another computer");
  });
  it("one that can't be restored says why; one that can says nothing", () => {
    expect(entryProblem({ restorable: true, problem: null })).toBeNull();
    expect(entryProblem({ restorable: false, problem: "A newer Gizai made it (0.15.0): update Gizai to restore it" }))
      .toBe("A newer Gizai made it (0.15.0): update Gizai to restore it");
    // without the backend's reason it still says it can't be restored
    expect(entryProblem({ restorable: false, problem: null })).toBe("It can't be restored.");
    expect(entryProblem({ restorable: false, problem: "" })).toBe("It can't be restored.");
  });
});

describe("the last and next backup", () => {
  it("Last backup: <time> (<size>), or No backup yet", () => {
    expect(lastText(STATUS, NOW)).toBe("Last backup: today 11:00 (1.1 MB)");
    expect(lastText({ last: null }, NOW)).toBe("No backup yet");
  });
  it("Next: <time> for Daily and Weekly, Next: when you ask for Only when I ask", () => {
    expect(nextText(STATUS, NOW)).toBe("Next: tomorrow 11:00");
    expect(nextText({ ...STATUS, settings: { ...STATUS.settings, every: "weekly" }, next: at(31, 11) }, NOW)).toMatch(/^Next: \w{3} 31 Oct, 11:00$/);
    expect(nextText({ ...STATUS, settings: { ...STATUS.settings, every: "manual" }, next: null }, NOW)).toBe("Next: when you ask");
  });
  it("one that is due already comes within the hour (the schedule looks every hour)", () => {
    expect(nextText({ ...STATUS, last: null, next: NOW }, NOW)).toBe("Next: within the hour");
    expect(nextText({ ...STATUS, next: NOW - 60_000 }, NOW)).toBe("Next: within the hour");
  });
  it("while the folder can't be used the next one waits for it; without a folder there is no next", () => {
    expect(nextText({ ...STATUS, folderProblem: "The folder isn't there: is the disk plugged in?", next: null }, NOW))
      .toBe("Next: once the folder can be used again");
    expect(nextText({ ...STATUS, settings: { ...STATUS.settings, folder: null }, folder: null, next: null }, NOW)).toBeNull();
  });
  it("the last error with its time and the backend's message", () => {
    expect(lastErrorText({ at: at(26, 9, 0), message: "No space left on device" }, NOW))
      .toBe("The last backup failed (yesterday 09:00): No space left on device");
  });
});

describe("Back up now", () => {
  it("can start one with a folder, while none runs", () => {
    expect(canBackUp(STATUS)).toBe(true);
  });
  it("can't without a folder", () => {
    expect(canBackUp({ folder: null, running: null })).toBe(false);
  });
  it("can't while one runs, whatever its reason", () => {
    for (const reason of ["manual", "daily", "quit"] as const) {
      expect(canBackUp({ ...STATUS, running: { reason, startedAt: NOW, step: "files" } })).toBe(false);
    }
  });
});

describe("Keep as typed", () => {
  it("a whole number is sent as it is, also outside 1 to 100: the backend refuses it and says why", () => {
    expect(keepOf("20")).toEqual({ keep: 20 });
    expect(keepOf(" 7 ")).toEqual({ keep: 7 });
    expect(keepOf("0")).toEqual({ keep: 0 });
    expect(keepOf("101")).toEqual({ keep: 101 });
  });
  it("an empty field saves nothing", () => {
    expect(keepOf("")).toBeNull();
    expect(keepOf("   ")).toBeNull();
  });
  it("a number that isn't whole says so", () => {
    expect(keepOf("2.5")).toEqual({ error: "Keep must be a whole number, 1 to 100" });
    expect(keepOf("abc")).toEqual({ error: "Keep must be a whole number, 1 to 100" });
  });
});
