// The Usage page's numbers (GA-33): the period switch, token counts, the API cost with its unknown part, and the bars.
// Days are UTC, like the agents' monthly budgets.
import type { UsagePeriod, UsageTotals } from "../types";
import { formatCost } from "./runs";

export const PERIODS: { key: UsagePeriod; label: string }[] = [
  { key: "today", label: "Today" }, { key: "7d", label: "7 days" }, { key: "30d", label: "30 days" }, { key: "month", label: "This month" },
];
export const DEFAULT_PERIOD: UsagePeriod = "month";

/** "today", "in the last 7 days", "in the last 30 days", "this month": after "No runs". */
export function periodPhrase(period: UsagePeriod): string {
  return period === "today" ? "today" : period === "month" ? "this month" : `in the last ${period === "7d" ? 7 : 30} days`;
}

export const COST_NOTE = "API cost: what these tokens would cost at API prices. It is an estimate, not a bill: on a subscription plan it is not what you pay.";
export const INPUT_LABEL = "Input tokens (incl. cache)";

const compact = new Intl.NumberFormat("en-US", { notation: "compact", maximumSignificantDigits: 3 });

/** 950, 12.3K, 4.56M, 1.2B. */
export function formatTokenCount(n: number): string {
  return compact.format(n);
}

/** 1,234,567: for a title. */
export function fullCount(n: number): string {
  return n.toLocaleString("en-GB");
}

/** The API cost of runs: "$1.23"; "Unknown" when the runs that used tokens reported no cost; "$1.23 + unknown" when some didn't. */
export function formatUsageCost(t: Pick<UsageTotals, "costUsdMicros" | "unknownCostRuns">): string {
  if (t.unknownCostRuns <= 0) return formatCost(t.costUsdMicros);
  if (t.costUsdMicros <= 0) return "Unknown";
  return `${formatCost(t.costUsdMicros)} + unknown`;
}

/** Why part of a cost is unknown (a title), or undefined when it is all known. */
export function unknownCostNote(n: number): string | undefined {
  if (n <= 0) return undefined;
  return n === 1
    ? "1 run reported no cost (Codex, Gemini and other CLIs don't): its tokens count, its cost is unknown."
    : `${n} runs reported no cost (Codex, Gemini and other CLIs don't): their tokens count, their cost is unknown.`;
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

/** "12 runs", "3 runs · 8 chat turns", "1 chat turn": chat turns are runs without a card. */
export function runsLabel(t: Pick<UsageTotals, "runs" | "chatTurns">): string {
  const runs = t.runs - t.chatTurns;
  if (t.chatTurns <= 0) return plural(runs, "run");
  if (runs <= 0) return plural(t.chatTurns, "chat turn");
  return `${plural(runs, "run")} · ${plural(t.chatTurns, "chat turn")}`;
}

/** Bar heights in % of the highest value: 0 stays 0, anything above 0 gets at least 2% so it shows. */
export function barHeights(values: number[]): number[] {
  const max = Math.max(0, ...values);
  return values.map((v) => (v <= 0 || max <= 0 ? 0 : Math.max(2, (v / max) * 100)));
}

/** What the share bars of a tab compare: the cost, or the tokens when no run in the period reported a cost. */
export function shareBasis(total: UsageTotals): "cost" | "tokens" {
  return total.costUsdMicros > 0 ? "cost" : "tokens";
}

/** A row's share of the total in % (0 to 100), by cost or by tokens. */
export function shareOf(t: UsageTotals, total: UsageTotals, basis: "cost" | "tokens"): number {
  const part = basis === "cost" ? t.costUsdMicros : t.inputTokens + t.outputTokens;
  const whole = basis === "cost" ? total.costUsdMicros : total.inputTokens + total.outputTokens;
  return whole > 0 ? Math.min(100, Math.max(0, (part / whole) * 100)) : 0;
}

/** A UTC day: "9 Oct". */
export function dayLabel(ms: number): string {
  return new Date(ms).toLocaleDateString("en-GB", { day: "numeric", month: "short", timeZone: "UTC" });
}

/** The days a period covers, from `since` up to (not including) `until`: "9 Oct", or "1 Oct – 9 Oct". */
export function periodDays(since: number, until: number): string {
  const first = dayLabel(since), last = dayLabel(until - 1);
  return first === last ? first : `${first} – ${last}`;
}
