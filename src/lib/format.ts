/** "just now", "5m ago", "3h ago", "yesterday", "4d ago". */
export function relTime(ms: number, now = Date.now()): string {
  const s = Math.round((now - ms) / 1000);
  if (s < 60) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.round(h / 24);
  return d === 1 ? "yesterday" : `${d}d ago`;
}

/** The end of a text longer than `max` characters: its last words within `max`, after "…" (a hold reason ends with its
 *  questions). Null when the text fits. A run of spaces and line breaks counts as one space. */
export function textEnd(text: string, max: number): string | null {
  const t = text.replace(/\s+/g, " ").trim();
  if (t.length <= max) return null;
  const end = t.slice(-max);
  const space = end.indexOf(" ");
  // Start at a word: drop the part of one the cut split, unless that is a long one (a path): then the cut stays inside it.
  return `…${t[t.length - max - 1] !== " " && space >= 0 && space < max / 4 ? end.slice(space + 1) : end}`;
}

/** "Marloes van der Visser" → "MV"; one word → its first two letters. */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
}

const LEGAL_FORMS = new Set(["BV", "NV", "VOF", "CV", "GMBH", "AG", "LTD", "LLC", "INC", "SA", "SL", "SRL", "BVBA"]);

/** "Kade Logistics B.V." → "KL"; legal forms are skipped; one word → its first two letters. */
export function companyInitials(name: string): string {
  const words = name.trim().split(/\s+/).filter((w) => w && !LEGAL_FORMS.has(w.replace(/\./g, "").toUpperCase()));
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

/** 999 B, 1.5 KB, 20 KB, 5.3 MB, 3.0 GB (1024-based; one decimal below 10). */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024, i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}
