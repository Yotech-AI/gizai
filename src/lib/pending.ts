// GA-102: what the Chat page shows for the Team Lead's calls that wait for your approval. Pure, so it is tested.
import type { DiffLine, PendingAction } from "../types";
import type { Group } from "./chat";

/** Where each waiting call shows: under the last group of messages of the answer that tried it (its run), by that group's
 *  key. A call whose answer has no message in the chat (yet) shows at the end, under "". Each list keeps the calls' order. */
export function placePending(groups: Group[], actions: PendingAction[]): Map<string, PendingAction[]> {
  const lastGroupOfRun = new Map<string, string>();
  for (const g of groups) {
    if (g.side !== "agent") continue;
    for (const m of g.items) if (m.runId) lastGroupOfRun.set(m.runId, g.key);
  }
  const out = new Map<string, PendingAction[]>();
  for (const a of actions) {
    const key = (a.runId && lastGroupOfRun.get(a.runId)) || "";
    out.set(key, [...(out.get(key) ?? []), a]);
  }
  return out;
}

/** A diff as it reads: lines, and in place of a long run of unchanged lines, how many were left out. */
export type DiffPart = DiffLine | { kind: "fold"; count: number };

/** Folds each run of unchanged lines longer than `keep` lines on each side of a change (and at the start or end) to the
 *  lines next to the changes and a count of the rest. A diff without changes keeps its first `keep` lines. */
export function foldDiff(lines: DiffLine[], keep = 2): DiffPart[] {
  const out: DiffPart[] = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].kind !== "same") { out.push(lines[i]); i++; continue; }
    let j = i;
    while (j < lines.length && lines[j].kind === "same") j++;
    const run = lines.slice(i, j);
    const before = i === 0 ? 0 : keep; // lines kept after the change above
    const after = j === lines.length ? 0 : keep; // lines kept before the change below
    if (run.length > before + after + 1) {
      out.push(...run.slice(0, before));
      out.push({ kind: "fold", count: run.length - before - after });
      out.push(...run.slice(run.length - after));
    } else {
      out.push(...run);
    }
    i = j;
  }
  // No change at all: the start of the text.
  if (!lines.some((l) => l.kind !== "same") && lines.length > keep + 1) return [...lines.slice(0, keep), { kind: "fold", count: lines.length - keep }];
  return out;
}

/** A tool from outside Gizai in words: "search on the otus server", "web search", "fetching a web page". */
export function outsideLabel(tool: string): string {
  if (tool === "WebSearch") return "web search";
  if (tool === "WebFetch") return "fetching a web page";
  const m = /^mcp__(.+?)__(.+)$/.exec(tool);
  if (m) return `${m[2].replace(/_/g, " ")} on the ${m[1]} server`;
  return tool;
}
