// A card's pull request on GitHub or Bitbucket, as the task page, the board and the activity show it.
import type { PullState, Task } from "../types";
import { HOST_NAMES, type PullHost } from "./provider";

/** The number at the end of a pull request link (https://github.com/owner/name/pull/12 or
 *  https://bitbucket.org/workspace/name/pull-requests/12 → 12). */
export function pullNumber(url: string): number | null {
  const m = /\/pull(?:-requests)?\/(\d+)\/?$/.exec(url.trim());
  return m ? Number(m[1]) : null;
}

/** "PR #12", or "PR" for a link without a number. */
export function pullLabel(url: string): string {
  const n = pullNumber(url);
  return n == null ? "PR" : `PR #${n}`;
}

const BADGES: Record<PullState, { text: string; cls: string }> = {
  open: { text: "Open", cls: "info" },
  draft: { text: "Draft", cls: "outline" },
  merged: { text: "Merged", cls: "ok" },
  closed: { text: "Closed", cls: "fail" },
};

/** The badge for a pull request's state; an unknown or missing state shows as open. */
export function pullBadge(state?: string | null): { text: string; cls: string } {
  return BADGES[(state ?? "open") as PullState] ?? BADGES.open;
}

type PullTask = Pick<Task, "stateCategory" | "prUrl" | "prState" | "branch">;

/** Whether Gizai follows the card's pull request on GitHub or Bitbucket, like the PR check: an open card in Review, or one whose
 *  pull request isn't merged yet. Done and Cancelled cards aren't followed. */
export function followsPull(t: PullTask): boolean {
  if (t.stateCategory === "done" || t.stateCategory === "cancelled") return false;
  return t.stateCategory === "review" || (!!t.prUrl && t.prState !== "merged");
}

/** The button on a card in Review: Open pull request, or Push branch when it has an open one. Null: no button (the
 *  card isn't in Review). `why` says why it can't be used now. */
export function pullAction(t: PullTask, live: boolean): { label: string; why: string | null } | null {
  if (t.stateCategory !== "review") return null;
  const open = !!t.prUrl && (t.prState === "open" || t.prState === "draft" || !t.prState);
  const label = open ? "Push branch" : "Open pull request";
  if (!t.branch) return { label, why: "This card has no branch yet: an agent makes one when it first works on the card." };
  if (live) return { label, why: "An agent is working on this card: wait until its run has ended." };
  return { label, why: null };
}

/** One plain line under the pull request: what the button does, or what happens next. `mergeTo`: the column a merge moves
 *  the card to (the team's Deploy column, else Done). `host`: where the pull request is, GitHub unless said. */
export function pullHint(t: PullTask, defaultBranch: string, mergeTo = "Done", host: PullHost = "github"): string {
  const branch = t.branch ?? "the card's branch";
  const on = HOST_NAMES[host];
  if (t.prUrl && t.prState === "merged") return `Merged on ${on}.`;
  if (t.prUrl && t.prState === "closed") {
    return t.stateCategory === "review" ? `Closed on ${on} without a merge. Open pull request opens a new one.` : `Closed on ${on} without a merge.`;
  }
  if (t.prUrl) {
    const push = t.stateCategory === "review" ? ` Push branch adds new commits from ${branch} to it.` : "";
    return `When it is merged on ${on}, the card moves to ${mergeTo} and its worktree is removed.${push}`;
  }
  // Bitbucket: the push always goes over SSH, and Gizai opens the pull request through Bitbucket's API (no gh)
  if (host === "bitbucket") {
    return `Pushes ${branch} with your SSH keys and opens a pull request into ${defaultBranch} on Bitbucket. When it is merged, the card moves to ${mergeTo}.`;
  }
  return `Pushes ${branch} to GitHub with your git login and opens a pull request into ${defaultBranch} with gh. When it is merged, the card moves to ${mergeTo}.`;
}
