/** "Kade Logistics portal" → "KLP"; one word → its first 4 letters. Keys start with a letter. */
export function suggestKey(name: string): string {
  const words = name.normalize("NFD").replace(/\p{M}/gu, "").toUpperCase().split(/[^A-Z0-9]+/).filter(Boolean);
  if (words.length === 0) return "";
  const k = words.length === 1 ? words[0].slice(0, 4) : words.map((w) => w[0]).join("").slice(0, 6);
  return /^[A-Z]/.test(k) ? k : "P" + k.slice(0, 5);
}
