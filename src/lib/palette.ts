export type PaletteItem = { kind: "task" | "project" | "client" | "note" | "action"; id: string; label: string; hint?: string };

export const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

/** Every word of the query must appear in label or hint. Label-prefix matches rank first, then label, then hint. */
export function searchItems(query: string, items: PaletteItem[], limit = 30): PaletteItem[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return items.slice(0, limit);
  const scored: { item: PaletteItem; score: number; i: number }[] = [];
  items.forEach((item, i) => {
    const label = fold(item.label);
    const hay = `${label} ${fold(item.hint ?? "")}`;
    if (!words.every((w) => hay.includes(w))) return;
    const score = label.startsWith(words[0]) ? 0 : words.every((w) => label.includes(w)) ? 1 : 2;
    scored.push({ item, score, i });
  });
  return scored.sort((a, b) => a.score - b.score || a.i - b.i).slice(0, limit).map((s) => s.item);
}
