/** Only http(s) and mailto links survive; anything else (javascript:, data:, file:, relative) becomes "". */
export function safeUrl(url: string): string {
  const u = url.trim();
  return /^(https?:\/\/|mailto:)/i.test(u) ? u : "";
}
