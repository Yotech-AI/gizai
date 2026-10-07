import { keyForDrop } from "./sortKey";

type Placed = { id: string; stateId: string; sortKey: string; identifier: string };

/** Card ids per column id, in board order (sort key, then identifier). Every listed column is present. */
export function groupByColumn(tasks: Placed[], stateIds: string[]): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  for (const s of stateIds) out[s] = [];
  const sorted = [...tasks].sort((a, b) =>
    a.sortKey < b.sortKey ? -1 : a.sortKey > b.sortKey ? 1 : a.identifier.localeCompare(b.identifier, undefined, { numeric: true }));
  for (const t of sorted) (out[t.stateId] ??= []).push(t.id);
  return out;
}

/** Sort key for `movingId`, given the column's ids after the drop (including the moved card). */
export function dropKey(ids: string[], movingId: string, keyOf: (id: string) => string): string {
  const index = ids.indexOf(movingId);
  return keyForDrop(ids.filter((x) => x !== movingId).map(keyOf), Math.max(0, index));
}
