const TASK_REF = /\b[A-Z][A-Z0-9]{1,5}-\d+\b/g;
const MENTION = /(^|\s)@([a-z0-9_-]{2,32})/gi;

const unique = (xs: string[]) => [...new Set(xs)];

/** "KADE-41", "GFW-7": task identifiers mentioned in Markdown, first occurrence order. */
export function findTaskRefs(md: string): string[] {
  return unique(md.match(TASK_REF) ?? []);
}

/** "@jeffrey" → "jeffrey"; e-mail addresses don't count (the @ must follow a space or start the text). */
export function findMentions(md: string): string[] {
  return unique([...md.matchAll(MENTION)].map((m) => m[2]));
}
