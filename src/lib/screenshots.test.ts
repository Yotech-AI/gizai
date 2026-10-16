// GA-185 QA: what the Screenshots tab, its larger view, the board's sign and the Inbox row show of a card's sets
// (lib/screenshots.ts), against GA-181's notes (design/screenshots/README.md): the newest round's count (4 + 2 = 6),
// the answers, Compare's pairing by file name (Unchanged by hash, New, Not taken this time), the notes' why, the folded
// older runs and the empty tab's four reasons, word for word.
import { describe, expect, it } from "vitest";
import type { Screenshot, ScreenshotSet } from "../types";
import {
  answerOf, compare, emptyReason, emptyTexts, formatOf, leftOutText, newestRound, noneText, notTakenText, olderCount, olderText, previewText,
  roundCount, runBefore, shortCommit, shotCount, summaryOf, themeOf, when,
} from "./screenshots";

const NOW = 1_760_000_000_000;
const H = 3_600_000;
const shot = (name: string, sha = name, w = 1280, h = 800): Screenshot => ({ fileId: `f-${name}-${sha}`, name, caption: name, width: w, height: h, sizeBytes: 1, sha256: sha });
const set = (runId: string, o: Partial<ScreenshotSet> = {}): ScreenshotSet => ({
  runId, agentId: "fa", agentName: "Frontend Agent", role: "frontend", outcome: "ready_for_testing", status: "succeeded", endedAt: NOW - H,
  commit: "9f8e7d6aa", shots: [], leftOut: [], preview: null, ...o,
});

// GA-181's tab: QA's set (2), the Frontend Agent's (4: two changed, one unchanged phone picture, one new light one), and
// four older runs (QA failed, the Frontend Agent's 3 with files left out, a preview that didn't start, no screen changed)
const qa = set("r6", { agentId: "qa", agentName: "QA Agent", role: "qa", outcome: "qa_pass", endedAt: NOW - 2 * H,
  shots: [shot("01-ac2-no-results-desktop-dark.png"), shot("02-ac4-slash-opens-search-desktop-dark.png")] });
const fa = set("r5", { endedAt: NOW - 3 * H, shots: [shot("01-docs-search-desktop-dark.png", "new1"), shot("02-search-results-desktop-dark.png", "new2"),
  shot("03-docs-search-mobile-dark.png", "same", 390, 844), shot("04-docs-search-desktop-light.png")] });
const qaFail = set("r4", { agentId: "qa", agentName: "QA Agent", role: "qa", outcome: "qa_fail", endedAt: NOW - 24 * H,
  shots: [shot("01-fail-results-cover-menu-mobile-dark.png", "x", 390, 844)] });
const faOld = set("r3", { endedAt: NOW - 25 * H, shots: [shot("01-docs-search-desktop-dark.png", "old1"), shot("02-search-results-desktop-dark.png", "old2"),
  shot("03-docs-search-mobile-dark.png", "same", 390, 844), shot("05-filters-open-mobile-dark.png", "gone", 390, 844)],
  leftOut: ["notes.txt: not a PNG, JPEG or WebP image", "05-docs-full-page-desktop-dark.png: over 16,384 pixels high"] });
const noPreview = set("r2", { status: "timed_out", outcome: null, endedAt: NOW - 48 * H, preview: "It didn't answer on port 47130 in 120 s" });
const noChange = set("r1", { outcome: "needs_decision", endedAt: NOW - 49 * H });
const SETS = [qa, fa, qaFail, faOld, noPreview, noChange];

describe("the newest round's count (the tab, the board's sign and the Inbox row)", () => {
  it("is the newest builder's set plus QA's sets after it: 4 + 2 = 6", () => {
    expect(newestRound(SETS).map((s) => s.runId)).toEqual(["r6", "r5"]);
    expect(roundCount(SETS)).toBe(6);
  });
  it("is the builder's alone when QA hasn't run since, and 0 without sets", () => {
    expect(roundCount([fa, qaFail, faOld])).toBe(4);
    expect(roundCount([])).toBe(0);
    expect(roundCount(null)).toBe(0);
  });
  it("is 0 when the newest builder's run took none, even when an older one did", () => {
    expect(roundCount([noChange, faOld])).toBe(0);
  });
  it("counts QA's sets only when no builder ran", () => {
    expect(roundCount([qa, qaFail])).toBe(3);
  });
  it("says 6 screenshots, 1 screenshot, no screenshots", () => {
    expect([shotCount(6), shotCount(1), shotCount(0)]).toEqual(["6 screenshots", "1 screenshot", "no screenshots"]);
  });
});

describe("a set's heading", () => {
  it("gives the answers in the design's words, with their badges", () => {
    expect(answerOf(qa)).toEqual({ cls: "ok", text: "QA passed" });
    expect(answerOf(fa)).toEqual({ cls: "info", text: "Ready for testing" });
    expect(answerOf(qaFail)).toEqual({ cls: "fail", text: "QA failed" });
    expect(answerOf(noChange)).toEqual({ cls: "needs", text: "Needs a decision" });
    expect(answerOf(noPreview)).toEqual({ cls: "warn", text: "Stopped at the time limit" });
  });
  it("says when as the design does, and the commit in 7 characters", () => {
    expect([when(NOW, NOW), when(NOW - 5 * 60_000, NOW), when(NOW - 2 * H, NOW), when(NOW - 24 * H, NOW), when(NOW - 48 * H, NOW)])
      .toEqual(["just now", "5 min ago", "2 h ago", "yesterday", "2 days ago"]);
    expect(shortCommit("9f8e7d6aa")).toBe("9f8e7d6");
    expect(shortCommit(null)).toBeNull();
  });
  it("reads format and theme from the file name, else from the size", () => {
    expect([formatOf(fa.shots[0]), formatOf(fa.shots[2]), formatOf(shot("board.png", "a", 390, 844))]).toEqual(["Desktop", "Mobile", "Mobile"]);
    expect([themeOf(fa.shots[0]), themeOf(fa.shots[3]), themeOf(shot("board.png"))]).toEqual(["Dark", "Light", "Dark"]);
  });
});

describe("Compare with the run before, paired by file name", () => {
  it("pairs with the same agent's newest older set with pictures (not QA's in between)", () => {
    expect(runBefore(SETS, fa)?.runId).toBe("r3");
    expect(runBefore(SETS, qa)?.runId).toBe("r4");
  });
  it("same hash: Unchanged; another hash: Changed; a new name: New; the run before's other names: Not taken this time", () => {
    const c = compare(SETS, fa)!;
    expect(c.before.runId).toBe("r3");
    expect(fa.shots.map((s) => c.pairing.get(s.fileId))).toEqual(["changed", "changed", "unchanged", "new"]);
    expect(c.pair.get(fa.shots[0].fileId)?.sha256).toBe("old1");
    expect(c.pair.has(fa.shots[3].fileId)).toBe(false);
    expect(summaryOf(c)).toBe("2 changed, 1 unchanged, 1 new");
    expect(c.notTaken).toEqual(["05-filters-open-mobile-dark.png"]);
    expect(notTakenText(c.notTaken)).toBe("Not taken this time: 05-filters-open-mobile-dark.png.");
  });
  it("is off for a set that shares no file name with the run before (QA's additions)", () => {
    expect(compare(SETS, qa)).toBeNull();
    expect(compare([fa], fa)).toBeNull();
  });
});

describe("the notes, with why", () => {
  it("what was left out, as GA-181's text", () => {
    expect(leftOutText(faOld.leftOut)).toBe("Left out 2: notes.txt isn’t a PNG, JPEG or WebP image; 05-docs-full-page-desktop-dark.png is over 16,384 pixels high.");
  });
  it("a preview that didn't start, and a run where no screen changed, as GA-181's texts", () => {
    expect(noneText(noPreview)).toEqual({ kind: "preview",
      text: "The preview didn’t start: it didn’t answer on port 47130 in 120 s, so the agent took none. Its last lines are in the run." });
    expect(noneText(noChange)).toEqual({ kind: "none", text: "No screen changed, the agent says, so it took none." });
    expect(noneText(fa)).toBeNull();
  });
  it("a run that ended early took none because it ended", () => {
    expect(noneText(set("x", { status: "failed", outcome: null }))?.text).toBe("It failed before it saved any.");
    expect(noneText(set("x", { status: "cancelled", outcome: null }))?.text).toBe("It was stopped before it saved any.");
    expect(noneText(set("x", { status: "timed_out", outcome: null }))?.text).toBe("It stopped at the time limit before it saved any.");
  });
  it("a set with pictures whose preview didn't start says so", () => {
    expect(previewText("It didn't answer on port 47130 in 120 s.")).toBe("The preview didn’t start: It didn’t answer on port 47130 in 120 s.");
  });
});

describe("older runs, folded", () => {
  it("'4 older runs' and when: 'Yesterday and 2 days ago: 2 sets of screenshots and 2 runs that took none'", () => {
    const older = SETS.slice(2);
    expect(olderCount(older.length)).toBe("4 older runs");
    expect(olderCount(1)).toBe("1 older run");
    expect(olderText(older, NOW)).toBe("Yesterday and 2 days ago: 2 sets of screenshots and 2 runs that took none");
  });
});

describe("the empty tab's four reasons, in GA-181's words", () => {
  const project = { name: "gizai-website", on: true, previewCommand: "npm run preview" };
  const agent = { id: "fa", name: "Frontend Agent", browser: true, screenshots: true };
  it("off for this card comes first", () => {
    const r = emptyReason({ cardSwitch: false, project: { ...project, previewCommand: null }, agent: { ...agent, browser: false } });
    expect(r.kind).toBe("card-off");
    expect(emptyTexts(r)).toEqual({ title: "No screenshots: they’re off for this card.",
      text: "Screenshots before Review is off in this card’s Properties, so its agents take none here. gizai-website has them on." });
  });
  it("then no preview", () => {
    const r = emptyReason({ cardSwitch: null, project: { name: "spoorwegmuseum", on: true, previewCommand: " " }, agent });
    expect(emptyTexts(r)).toEqual({ title: "No screenshots: spoorwegmuseum has no preview yet.",
      text: "Agents take screenshots of the card’s branch, served by the project’s Preview command with demo data. Without one they take none." });
  });
  it("then the agent's switch, or its browser", () => {
    expect(emptyTexts(emptyReason({ cardSwitch: true, project, agent: { ...agent, screenshots: false } }))).toEqual({
      title: "No screenshots: the Frontend Agent doesn’t take them.",
      text: "Its Take screenshots of what it built is off, so it builds this card’s screens without keeping pictures of them." });
    expect(emptyTexts(emptyReason({ cardSwitch: null, project, agent: { ...agent, browser: false, screenshots: false } })).text)
      .toBe("It has no browser, so it builds this card’s screens without keeping pictures of them.");
  });
  it("else no run yet", () => {
    expect(emptyTexts(emptyReason({ cardSwitch: null, project, agent }))).toEqual({ title: "No screenshots yet.",
      text: "Agents with the browser save screenshots of the screens they changed at the end of their run. The Frontend Agent takes them when it has built this card." });
    expect(emptyTexts(emptyReason({ cardSwitch: null, project, agent: null })).text)
      .toBe("Agents with the browser save screenshots of the screens they changed at the end of their run.");
  });
});
