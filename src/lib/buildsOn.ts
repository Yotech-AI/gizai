// Builds on (GA-228): a card waits until the cards it builds on are merged (in a Deploy or Done column; a cancelled card
// never counts), then starts by itself. The board's "Waits for" badge and its tooltip, a linked card's line on the card
// page, and the card picker's choices. Pure, so it is tested.
import type { PickItem } from "./itemLinks";
import type { BuildsOnCard, Task } from "../types";

/** Columns where a card waits for nothing any more: nothing starts there. */
const SETTLED = new Set(["done", "cancelled"]);

function joinWords(xs: readonly string[]): string {
  return xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`;
}

/** The board's badge: "Waits for GA-226", or "Waits for 2 cards" for several; null when the card waits for none, or is in
 *  Done or Cancelled. */
export function waitsBadge(task: Pick<Task, "waitsFor" | "stateCategory">): string | null {
  const waits = task.waitsFor ?? [];
  if (waits.length === 0 || SETTLED.has(task.stateCategory)) return null;
  return waits.length === 1 ? `Waits for ${waits[0]}` : `Waits for ${waits.length} cards`;
}

/** The badge's tooltip, naming the cards, each with its title when `titleOf` knows it (the board's own cards):
 *  "Waits for GA-226 Team templates (backend) to be merged", or a line per card for several. */
export function waitsTip(waitsFor: readonly string[], titleOf: (identifier: string) => string | undefined): string {
  const named = waitsFor.map((id) => { const title = titleOf(id); return title ? `${id} ${title}` : id; });
  if (named.length <= 1) return `Waits for ${joinWords(named)} to be merged`;
  return `Waits for these cards to be merged:\n${named.join("\n")}`;
}

/** A linked card's tooltip on the card page: "GA-226 Team templates (backend): merged", or where it is and that it isn't
 *  merged yet (a cancelled card never is). */
export function linkedTip(card: BuildsOnCard): string {
  const name = `${card.identifier} ${card.title}`;
  if (card.merged) return `${name}: merged${card.archived ? " (archived)" : ""}`;
  if (card.category === "cancelled") return `${name}: cancelled, so it never counts as merged`;
  return `${name}: in ${card.column}${card.hold ? ", on hold" : ""}, not merged yet`;
}

/** The cards the picker offers to build on: every card it lists (open ones first, as @ does), but not the card itself or
 *  one it builds on already. */
export function buildsOnChoices(items: readonly PickItem[], self: string, taken: readonly string[]): PickItem[] {
  const skip = new Set([self, ...taken].map((x) => x.toUpperCase()));
  return items.filter((i) => i.kind === "task" && !skip.has(i.key.toUpperCase()));
}
