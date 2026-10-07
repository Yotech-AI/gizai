// Soft checks for Dutch/EU business identifiers. The UI warns; it never blocks saving.
const compact = (s: string) => s.replace(/[\s.\-]/g, "").toUpperCase();

/** EU VAT number shape; NL numbers must look like NL + 9 digits + B + 2 digits. */
export function isVatNumber(s: string): boolean {
  const v = compact(s);
  if (v.startsWith("NL")) return /^NL\d{9}B\d{2}$/.test(v);
  // other EU countries: country code, then 2–12 letters/digits with at least two digits
  return /^[A-Z]{2}[0-9A-Z]{2,12}$/.test(v) && (v.slice(2).match(/\d/g)?.length ?? 0) >= 2;
}

/** Dutch KvK (Chamber of Commerce) number: 8 digits. */
export function isKvk(s: string): boolean {
  return /^\d{8}$/.test(compact(s));
}

/** IBAN with the mod-97 checksum. */
export function isIban(s: string): boolean {
  const v = compact(s);
  if (!/^[A-Z]{2}\d{2}[A-Z0-9]{10,30}$/.test(v)) return false;
  const rearranged = v.slice(4) + v.slice(0, 4);
  let rem = 0;
  for (const ch of rearranged) {
    const n = /[A-Z]/.test(ch) ? String(ch.charCodeAt(0) - 55) : ch;
    for (const d of n) rem = (rem * 10 + Number(d)) % 97;
  }
  return rem === 1;
}
