// GA-228 QA: Builds on's pure parts. The board's "Waits for" badge (one card by its ID, several as a count; none once the
// cards are merged, or in Done or Cancelled), its tooltip naming the cards, a linked card's tooltip on the card page
// (merged, archived, cancelled never merged, in work, on hold), and the cards the picker offers (tasks only, never the
// card itself or one it builds on already, whatever their case).
import { describe, expect, it } from "vitest";
import { buildsOnChoices, linkedTip, waitsBadge, waitsTip } from "./buildsOn";
import type { PickItem } from "./itemLinks";
import type { BuildsOnCard } from "../types";

describe("the board's Waits for badge (GA-228)", () => {
  it("names one card, and counts several", () => {
    expect(waitsBadge({ waitsFor: ["GA-226"], stateCategory: "ready" })).toBe("Waits for GA-226");
    expect(waitsBadge({ waitsFor: ["GA-225", "GA-226"], stateCategory: "ready" })).toBe("Waits for 2 cards");
    expect(waitsBadge({ waitsFor: ["GA-1", "GA-2", "KADE-3"], stateCategory: "testing" })).toBe("Waits for 3 cards");
  });

  it("is in every column where a card can still start, Auto or not", () => {
    for (const c of ["backlog", "ready", "in_progress", "testing", "review", "deploy"]) {
      expect(waitsBadge({ waitsFor: ["GA-226"], stateCategory: c as never }), c).toBe("Waits for GA-226");
    }
  });

  it("goes once the cards are merged (no waitsFor left), and never shows in Done or Cancelled", () => {
    expect(waitsBadge({ waitsFor: [], stateCategory: "ready" })).toBeNull();
    expect(waitsBadge({ stateCategory: "ready" })).toBeNull();
    expect(waitsBadge({ waitsFor: undefined, stateCategory: "in_progress" })).toBeNull();
    expect(waitsBadge({ waitsFor: ["GA-226"], stateCategory: "done" })).toBeNull();
    expect(waitsBadge({ waitsFor: ["GA-225", "GA-226"], stateCategory: "cancelled" })).toBeNull();
  });
});

describe("the badge's tooltip (GA-228)", () => {
  const titles: Record<string, string> = { "GA-225": "Team templates", "GA-226": "Team templates (backend)" };
  const titleOf = (id: string) => titles[id];

  it("names one card with its title, or by its ID when the board doesn't have it", () => {
    expect(waitsTip(["GA-226"], titleOf)).toBe("Waits for GA-226 Team templates (backend) to be merged");
    expect(waitsTip(["KADE-9"], titleOf)).toBe("Waits for KADE-9 to be merged");
  });

  it("names several, one line each, with their titles where known", () => {
    expect(waitsTip(["GA-225", "GA-226", "KADE-9"], titleOf))
      .toBe("Waits for these cards to be merged:\nGA-225 Team templates\nGA-226 Team templates (backend)\nKADE-9");
    expect(waitsTip(["GA-225", "GA-226"], () => undefined)).toBe("Waits for these cards to be merged:\nGA-225\nGA-226");
  });
});

describe("a linked card's tooltip on the card page (GA-228)", () => {
  const card = (over: Partial<BuildsOnCard>): BuildsOnCard => ({ id: "t226", identifier: "GA-226", title: "Team templates (backend)", projectId: "p1",
    column: "In progress", category: "in_progress", merged: false, hold: null, archived: false, ...over });

  it("says a merged card is merged, an archived one too", () => {
    expect(linkedTip(card({ column: "Deploy", category: "deploy", merged: true }))).toBe("GA-226 Team templates (backend): merged");
    expect(linkedTip(card({ column: "Done", category: "done", merged: true }))).toBe("GA-226 Team templates (backend): merged");
    expect(linkedTip(card({ column: "Done", category: "done", merged: true, archived: true }))).toBe("GA-226 Team templates (backend): merged (archived)");
  });

  it("says a cancelled card never counts as merged", () => {
    expect(linkedTip(card({ column: "Cancelled", category: "cancelled" }))).toBe("GA-226 Team templates (backend): cancelled, so it never counts as merged");
  });

  it("says where a card in work is, and that it is on hold", () => {
    expect(linkedTip(card({}))).toBe("GA-226 Team templates (backend): in In progress, not merged yet");
    expect(linkedTip(card({ column: "Review", category: "review" }))).toBe("GA-226 Team templates (backend): in Review, not merged yet");
    expect(linkedTip(card({ hold: "needs_decision" }))).toBe("GA-226 Team templates (backend): in In progress, on hold, not merged yet");
  });
});

describe("the cards the picker offers to build on (GA-228)", () => {
  const task = (key: string): PickItem => ({ kind: "task", key, label: `${key} - Card`, text: `${key} - Card` });
  const items: PickItem[] = [
    task("GA-1"), task("GA-2"), task("GA-3"), task("KADE-4"),
    { kind: "project", key: "GA", label: "GA - Giz AI", text: "Giz AI" },
    { kind: "agent", key: "a1", label: "Backend Agent", text: "Backend Agent" },
    { kind: "doc", key: "d1", label: "GA-1 notes", text: "GA-1 notes" },
  ];

  it("offers every other card of any project, and nothing that isn't a card", () => {
    expect(buildsOnChoices(items, "GA-3", []).map((i) => i.key)).toEqual(["GA-1", "GA-2", "KADE-4"]);
  });

  it("never offers the card itself or a card it builds on already, whatever their case", () => {
    expect(buildsOnChoices(items, "GA-3", ["GA-1"]).map((i) => i.key)).toEqual(["GA-2", "KADE-4"]);
    expect(buildsOnChoices(items, "ga-3", ["ga-1", "Kade-4"]).map((i) => i.key)).toEqual(["GA-2"]);
    expect(buildsOnChoices(items, "GA-3", ["GA-1", "GA-2", "KADE-4"])).toEqual([]);
  });
});
