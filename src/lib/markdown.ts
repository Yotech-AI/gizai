import { parseItemUrl } from "./itemLinks";

/** Only http(s) and mailto links survive; anything else (javascript:, data:, file:, relative) becomes "". */
export function safeUrl(url: string): string {
  const u = url.trim();
  return /^(https?:\/\/|mailto:)/i.test(u) ? u : "";
}

/** What shown Markdown keeps of a link: `safeUrl`'s, and links to Gizai items (gizai:task/GA-12), which open inside Gizai. */
export function viewUrl(url: string): string {
  return parseItemUrl(url) ? url.trim() : safeUrl(url);
}
