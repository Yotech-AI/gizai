import type { ChangeEntry } from "../types";
import { hostName } from "./provider";
import { pullNumber } from "./pulls";

const FIELD_NAMES: Record<string, string> = {
  title: "title", descriptionMd: "description", acceptanceMd: "acceptance criteria", priority: "priority",
  assigneeId: "assignee", pinnedActorId: "pinned agent", dueOn: "due date",
};

function joinWords(xs: string[]): string {
  return xs.length <= 1 ? xs.join("") : `${xs.slice(0, -1).join(", ")} and ${xs[xs.length - 1]}`;
}

/** One plain sentence (without the actor) for a change-log entry of a task or its comments. */
export function describeChange(e: Pick<ChangeEntry, "table" | "op" | "diff">): string {
  const d = (e.diff ?? {}) as Record<string, unknown>;
  if (e.table === "comments") return e.op === "insert" ? "commented" : "edited a comment";
  if (e.table !== "tasks") return `${e.op} ${e.table}`;
  if (e.op === "insert") return "created the task";
  // Archiving (from Done) is the card's soft delete; restoring undoes it.
  if (e.op === "delete") return "archived the card";
  if (d.archived === false) return "restored the card";
  if (Array.isArray(d.column)) return `moved it from ${d.column[0]} to ${d.column[1]}`;
  if (Array.isArray(d.labels)) return d.labels.length ? `set labels to ${d.labels.join(", ")}` : "removed all labels";
  // Review on GitHub or Bitbucket (its link says which): a pull request Gizai opened or saw change, and the clean-up after its merge
  if (typeof d.cleanup === "string" && d.cleanup) return d.cleanup;
  if (typeof d.pullRequest === "string") {
    const n = pullNumber(d.pullRequest);
    const pr = n == null ? "a pull request" : `pull request #${n}`;
    const on = hostName(d.pullRequest);
    if (d.opened) return `opened ${pr} on ${on}`;
    const state = typeof d.prState === "string" ? d.prState : "open";
    return state === "draft" ? `saw ${pr} as a draft on ${on}` : `saw ${pr} ${state} on ${on}`;
  }
  const set = (k: string) => d[k] !== null && d[k] !== undefined;
  const fields = Object.keys(FIELD_NAMES).filter(set).map((k) => FIELD_NAMES[k]);
  // The Testing switch: on, the QA Agent tests the card before Review
  if (typeof d.testing === "boolean") {
    if (!fields.length && !set("hold")) return d.testing ? "turned testing on" : "turned testing off";
    fields.push("testing");
  }
  if (set("hold")) {
    if (d.hold === "") return fields.length ? `cleared the hold and changed the ${joinWords(fields)}` : "cleared the hold";
    fields.push("hold");
  }
  return fields.length ? `changed the ${joinWords(fields)}` : "updated the task";
}
