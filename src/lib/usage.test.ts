// GA-33: the Usage page's numbers: token counts, the API cost with its unknown part, the bars, the period switch.
import { describe, expect, it } from "vitest";
import {
  COST_NOTE, DEFAULT_PERIOD, INPUT_LABEL, PERIODS, barHeights, dayLabel, formatTokenCount, formatUsageCost, fullCount, knownCost, periodDays,
  periodPhrase, runsLabel, shareBasis, shareOf, unknownCostLine, unknownCostNote,
} from "./usage";
import type { UsageTotals } from "../types";
import usageRs from "../../crates/gizai-core/src/usage.rs?raw";
import tauriLib from "../../src-tauri/src/lib.rs?raw";
import apiTs from "../api.ts?raw";

const DAY = 86_400_000;
const OCT_1 = Date.UTC(2026, 9, 1);
const OCT_9 = Date.UTC(2026, 9, 9);
const t = (p: Partial<UsageTotals>): UsageTotals => ({ runs: 0, chatTurns: 0, inputTokens: 0, outputTokens: 0, costUsdMicros: 0, unknownCostRuns: 0, ...p });

describe("token counts", () => {
  it("are short: 950, 12.3K, 4.56M, 1.2B", () => {
    expect(formatTokenCount(0)).toBe("0");
    expect(formatTokenCount(950)).toBe("950");
    expect(formatTokenCount(1_000)).toBe("1K");
    expect(formatTokenCount(12_345)).toBe("12.3K");
    expect(formatTokenCount(4_560_000)).toBe("4.56M");
    expect(formatTokenCount(1_200_000_000)).toBe("1.2B");
  });

  it("are in full on hover", () => {
    expect(fullCount(1_234_567)).toBe("1,234,567");
    expect(fullCount(0)).toBe("0");
  });
});

describe("the API cost", () => {
  it("is in dollars when every run reported its cost, $0.00 included", () => {
    expect(formatUsageCost(t({ costUsdMicros: 1_234_567 }))).toBe("$1.23");
    expect(formatUsageCost(t({}))).toBe("$0.00");
    expect(knownCost(t({ costUsdMicros: 1_234_567 }))).toBe("$1.23");
    expect(knownCost(t({}))).toBe("$0.00");
  });

  it("is Unknown, not $0, when the runs that used tokens reported no cost", () => {
    expect(formatUsageCost(t({ unknownCostRuns: 2 }))).toBe("Unknown");
    expect(knownCost(t({ unknownCostRuns: 2 }))).toBe("Unknown");
    expect(unknownCostLine(t({ unknownCostRuns: 2 }))).toBe("");
  });

  it("says so when part of it is unknown", () => {
    expect(formatUsageCost(t({ costUsdMicros: 1_230_000, unknownCostRuns: 1 }))).toBe("$1.23 + unknown");
    expect(knownCost(t({ costUsdMicros: 1_230_000, unknownCostRuns: 1 }))).toBe("$1.23");
    expect(unknownCostLine(t({ costUsdMicros: 1_230_000, unknownCostRuns: 1 }))).toBe("+ an unknown cost for 1 run");
    expect(unknownCostLine(t({ costUsdMicros: 1_230_000, unknownCostRuns: 3 }))).toBe("+ an unknown cost for 3 runs");
    expect(unknownCostLine(t({ costUsdMicros: 1_230_000 }))).toBe("");
  });

  it("explains an unknown cost on hover, and nothing when it is all known", () => {
    expect(unknownCostNote(0)).toBeUndefined();
    expect(unknownCostNote(1)).toBe("1 run reported no cost (Codex, Gemini and other CLIs don't): its tokens count, its cost is unknown.");
    expect(unknownCostNote(4)).toBe("4 runs reported no cost (Codex, Gemini and other CLIs don't): their tokens count, their cost is unknown.");
  });

  it("is labelled as an estimate at API prices, not a bill, and the input tokens say they include cache", () => {
    expect(COST_NOTE).toContain("API cost: what these tokens would cost at API prices");
    expect(COST_NOTE).toContain("not a bill");
    expect(INPUT_LABEL).toBe("Input tokens (incl. cache)");
  });
});

describe("runs and chat turns", () => {
  it("count chat turns apart from runs on cards", () => {
    expect(runsLabel(t({ runs: 12 }))).toBe("12 runs");
    expect(runsLabel(t({ runs: 1 }))).toBe("1 run");
    expect(runsLabel(t({}))).toBe("0 runs");
    expect(runsLabel(t({ runs: 11, chatTurns: 8 }))).toBe("3 runs · 8 chat turns");
    expect(runsLabel(t({ runs: 2, chatTurns: 1 }))).toBe("1 run · 1 chat turn");
    expect(runsLabel(t({ runs: 1, chatTurns: 1 }))).toBe("1 chat turn");
    expect(runsLabel(t({ runs: 5, chatTurns: 5 }))).toBe("5 chat turns");
  });
});

describe("the bars", () => {
  it("are a share of the highest day; a day with anything shows, an empty day doesn't", () => {
    expect(barHeights([])).toEqual([]);
    expect(barHeights([0, 0])).toEqual([0, 0]);
    expect(barHeights([0, 50, 100])).toEqual([0, 50, 100]);
    expect(barHeights([1, 1_000])).toEqual([2, 100]);
    expect(barHeights([-5, 10])).toEqual([0, 100]);
  });

  it("compare the cost, or the tokens when no run in the period reported a cost", () => {
    expect(shareBasis(t({ costUsdMicros: 1 }))).toBe("cost");
    expect(shareBasis(t({ inputTokens: 100, unknownCostRuns: 1 }))).toBe("tokens");
    const total = t({ costUsdMicros: 600_000, inputTokens: 900, outputTokens: 100 });
    expect(shareOf(t({ costUsdMicros: 420_000 }), total, "cost")).toBeCloseTo(70);
    expect(shareOf(t({ inputTokens: 200, outputTokens: 50 }), total, "tokens")).toBeCloseTo(25);
    expect(shareOf(t({ inputTokens: 5_000 }), total, "cost")).toBe(0);
    expect(shareOf(t({ costUsdMicros: 1 }), t({}), "cost")).toBe(0);
    expect(shareOf(t({ costUsdMicros: 900_000 }), total, "cost")).toBe(100);
  });
});

describe("the period switch", () => {
  it("offers today, 7 days, 30 days and this month, this month first selected", () => {
    expect(PERIODS.map((p) => p.label)).toEqual(["Today", "7 days", "30 days", "This month"]);
    expect(PERIODS.map((p) => p.key)).toEqual(["today", "7d", "30d", "month"]);
    expect(DEFAULT_PERIOD).toBe("month");
  });

  it("uses the periods the backend knows, through the one Tauri command", () => {
    expect(usageRs).toContain(`pub const PERIODS: [&str; 4] = ["today", "7d", "30d", "month"];`);
    expect(tauriLib).toContain("commands::usage_summary");
    expect(apiTs).toContain(`invoke<T.Usage>("usage_summary", { period })`);
  });

  it("says what an empty period is", () => {
    expect(periodPhrase("today")).toBe("today");
    expect(periodPhrase("7d")).toBe("in the last 7 days");
    expect(periodPhrase("30d")).toBe("in the last 30 days");
    expect(periodPhrase("month")).toBe("this month");
  });

  it("shows its UTC days", () => {
    expect(dayLabel(OCT_9)).toBe("9 Oct");
    expect(dayLabel(OCT_9 + DAY - 1)).toBe("9 Oct");
    expect(periodDays(OCT_9, OCT_9 + DAY)).toBe("9 Oct");
    expect(periodDays(OCT_1, OCT_9 + DAY)).toBe("1 Oct – 9 Oct");
    expect(periodDays(OCT_9 - 29 * DAY, OCT_9 + DAY)).toBe("10 Sept – 9 Oct");
  });
});
