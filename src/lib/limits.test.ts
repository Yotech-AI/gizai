// GA-62: the Subscription tab's words and states (limits.ts). Times are local, so they are made with local dates here: "now" is
// Friday 9 October 2026, 15:00.
import { describe, expect, it } from "vitest";
import type { CliLimits, LimitReading, SubscriptionLimit } from "../types";
import {
  NEAR_PERCENT, anyRead, asOfLabel, cantRead, chatsLine, limitState, resetLabel, sourceNote, unreadLabel, usedLabel, usedWidth, whenLabel, windowLabel,
} from "./limits";

const at = (month: number, day: number, h: number, m = 0, year = 2026) => new Date(year, month, day, h, m).getTime();
const NOW = at(9, 9, 15);
const reading = (o: Partial<LimitReading>): LimitReading => ({ key: "five_hour", observedAt: at(9, 9, 14, 2), ...o });
const limit = (r: Partial<LimitReading> | null, key = "five_hour"): SubscriptionLimit =>
  ({ key, name: "Session limit", windowMinutes: 300, reading: r ? reading({ key, ...r }) : null });
const cli = (o: Partial<CliLimits>): CliLimits =>
  ({ cliId: "claude_code", name: "Claude Code", kind: "claude_code", readable: true, accountDir: "~/.claude", limits: [], agents: [], leadChat: false, chats: 0, ...o });

describe("where a limit stands", () => {
  it("is unread without a reading, and reset once its window has reset since the reading", () => {
    expect(limitState(limit(null), NOW)).toBe("unread");
    expect(limitState(limit({ usedPercent: 95, resetsAt: NOW }), NOW)).toBe("reset");
    expect(limitState(limit({ usedPercent: 95, resetsAt: NOW - 1 }), NOW)).toBe("reset");
    expect(limitState(limit({ status: "rejected", resetsAt: NOW - 60_000 }), NOW)).toBe("reset");
    expect(limitState(limit({ usedPercent: 95, resetsAt: NOW + 1 }), NOW)).toBe("near");
  });

  it("is reached when the CLI said rejected or at 100% and more, near from 80% or on Claude Code's warning, else ok", () => {
    expect(NEAR_PERCENT).toBe(80);
    expect(limitState(limit({ status: "rejected" }), NOW)).toBe("reached");
    expect(limitState(limit({ usedPercent: 100 }), NOW)).toBe("reached");
    expect(limitState(limit({ usedPercent: 130 }), NOW)).toBe("reached");
    expect(limitState(limit({ usedPercent: 80 }), NOW)).toBe("near");
    expect(limitState(limit({ usedPercent: 10, status: "allowed_warning" }), NOW)).toBe("near");
    expect(limitState(limit({ usedPercent: 79.9 }), NOW)).toBe("ok");
    expect(limitState(limit({ usedPercent: 0, status: "allowed" }), NOW)).toBe("ok");
  });
});

describe("how much is used", () => {
  it("rounds to a whole percent, says <1% for a sliver and 0% for none", () => {
    expect(usedLabel(limit({ usedPercent: 42.4 }), NOW)).toBe("42%");
    expect(usedLabel(limit({ usedPercent: 99.6 }), NOW)).toBe("100%");
    expect(usedLabel(limit({ usedPercent: 0.4 }), NOW)).toBe("<1%");
    expect(usedLabel(limit({ usedPercent: 0 }), NOW)).toBe("0%");
    expect(usedLabel(limit({ usedPercent: 120 }), NOW)).toBe("120%");
  });

  it("never makes up a number: Limit reached when the CLI only said so, nothing without a reading or after a reset", () => {
    expect(usedLabel(limit({ usedPercent: null, status: "rejected" }), NOW)).toBe("Limit reached");
    expect(usedLabel(limit({ usedPercent: null, status: "allowed" }), NOW)).toBe("");
    expect(usedLabel(limit(null), NOW)).toBe("");
    expect(usedLabel(limit({ usedPercent: 60, resetsAt: NOW - 1 }), NOW)).toBe("");
  });

  it("fills the bar up to 100% and leaves it empty without a number", () => {
    expect(usedWidth(reading({ usedPercent: 42 }))).toBe(42);
    expect(usedWidth(reading({ usedPercent: 150 }))).toBe(100);
    expect(usedWidth(reading({ usedPercent: null, status: "rejected" }))).toBe(0);
    expect(usedWidth(null)).toBe(0);
  });
});

describe("times", () => {
  it("says a moment as short as now allows, in local time", () => {
    expect(whenLabel(at(9, 9, 17), NOW)).toBe("17:00");
    expect(whenLabel(at(9, 9, 0, 5), NOW)).toBe("00:05");
    expect(whenLabel(at(9, 10, 9), NOW)).toBe("tomorrow 09:00");
    expect(whenLabel(at(9, 8, 14, 2), NOW)).toBe("yesterday 14:02");
    expect(whenLabel(at(9, 13, 9), NOW)).toBe("Tue 13 Oct, 09:00");
    expect(whenLabel(at(9, 5, 9), NOW)).toBe("Mon 5 Oct, 09:00");
    expect(whenLabel(at(9, 20, 9), NOW)).toBe("20 Oct, 09:00");
    expect(whenLabel(at(0, 2, 9, 0, 2027), NOW)).toBe("2 Jan 2027, 09:00");
  });

  it("gives the reset as a time, else as the CLI wrote it, else nothing; and the reading's time as 'as of'", () => {
    expect(resetLabel(reading({ resetsAt: at(9, 9, 17), resetsText: "3pm" }), NOW)).toBe("17:00");
    expect(resetLabel(reading({ resetsText: " 3pm (Europe/Amsterdam) " }), NOW)).toBe("3pm (Europe/Amsterdam)");
    expect(resetLabel(reading({}), NOW)).toBe("");
    expect(resetLabel(null, NOW)).toBe("");
    expect(asOfLabel(reading({ observedAt: at(9, 9, 14, 2) }), NOW)).toBe("as of 14:02");
    expect(asOfLabel(reading({ observedAt: at(9, 8, 23, 59) }), NOW)).toBe("as of yesterday 23:59");
  });

  it("names a window's length", () => {
    expect(windowLabel(300)).toBe("5 hours");
    expect(windowLabel(60)).toBe("1 hour");
    expect(windowLabel(10_080)).toBe("7 days");
    expect(windowLabel(1440)).toBe("1 day");
    expect(windowLabel(90)).toBe("90 minutes");
    expect(windowLabel(0)).toBe("");
    expect(windowLabel(null)).toBe("");
  });
});

describe("what a block says", () => {
  it("says why a limit has no number: Codex reads its log after a run, Claude Code reports it in runs and chat turns, Fable only for some accounts", () => {
    const codex = cli({ cliId: "c-3", name: "Codex work", kind: "codex", accountDir: "~/.codex-work" });
    expect(unreadLabel(codex, limit(null, "primary"))).toEqual({ text: "Not read yet", title: "Gizai reads it from Codex's session log when a run on Codex work ends." });
    expect(unreadLabel(cli({ name: "Claude Code 2" }), limit(null))).toEqual({
      text: "Not reported yet", title: "Claude Code reports it in the runs and chat turns on Claude Code 2, on a Claude subscription (not with an API key).",
    });
    expect(unreadLabel(cli({}), limit(null, "seven_day_overage_included")).title).toContain("only for an account that has one");
  });

  it("says where the numbers come from, with the Codex account's session folder", () => {
    expect(sourceNote(cli({ name: "Claude Code 2" }))).toBe(
      "Claude Code reports these limits in the runs and chat turns on Claude Code 2 when they change, on a Claude subscription only. Gizai keeps the newest numbers.");
    expect(sourceNote(cli({ name: "Codex", kind: "codex", accountDir: "~/.codex" }))).toBe(
      "Codex writes these limits in its session log (~/.codex/sessions). Gizai reads the log of each run on Codex when the run ends.");
    expect(sourceNote(cli({ name: "Codex", kind: "codex", accountDir: null }))).toBe(
      "Codex writes these limits in its session log. Gizai reads the log of each run on Codex when the run ends.");
  });

  it("says Gizai can't read Gemini's or an Other CLI's limits yet", () => {
    expect(cantRead(cli({ name: "Gemini", kind: "gemini", readable: false }))).toBe("Gizai can't read Gemini's limits yet.");
    expect(cantRead(cli({ name: "Aider", kind: "other", readable: false }))).toBe("Gizai can't read the limits of Aider yet: it reads Claude Code's and Codex's.");
    expect(sourceNote(cli({ name: "Gemini 2", kind: "gemini", readable: false }))).toBe("Gizai can't read Gemini's limits yet.");
  });

  it("knows whether any limit has a reading", () => {
    expect(anyRead(cli({ limits: [limit(null), limit(null, "seven_day")] }))).toBe(false);
    expect(anyRead(cli({ limits: [limit(null), limit({ usedPercent: 3 }, "seven_day")] }))).toBe(true);
    expect(anyRead(cli({ limits: [] }))).toBe(false);
  });

  it("says which chats run on a CLI: the Team Lead's, chats that picked it, both, or none", () => {
    expect(chatsLine(cli({}))).toBe("");
    expect(chatsLine(cli({ leadChat: true }))).toBe("The Team Lead's chat runs here.");
    expect(chatsLine(cli({ chats: 1 }))).toBe("1 chat picked it under Runs on.");
    expect(chatsLine(cli({ chats: 3 }))).toBe("3 chats picked it under Runs on.");
    expect(chatsLine(cli({ leadChat: true, chats: 2 }))).toBe("The Team Lead's chat runs here, and 2 chats picked it under Runs on.");
  });
});
