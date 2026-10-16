// The screenshots sign (GA-185, GA-181's Shot/Sign): on a board card in Review whose newest round has screenshots, after
// its pull request sign, "6"; in its Inbox row "6 screenshots". A click opens the card's Screenshots tab.
import { useEffect, useState } from "react";
import { Images } from "lucide-react";
import { listScreenshots, onRunsChanged } from "../../api";
import { go, href } from "../../router";
import { roundCount, shotCount } from "../../lib/screenshots";
import type { Task } from "../../types";

const title = (n: number) => `${shotCount(n)}: open the Screenshots tab`;

/** On a board card: a button of its own, so pressing it neither starts a drag nor opens the card's Card tab. */
export function BoardShotSign({ taskId, count }: { taskId: string; count: number }) {
  return (
    <button type="button" className="shot-sign" title={title(count)} aria-label={title(count)}
      onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}
      onClick={(e) => { e.stopPropagation(); go({ page: "task", id: taskId, view: "screenshots" }); }}>
      <Images className="icon" aria-hidden />{count}
    </button>
  );
}

/** In an Inbox row, whose link opens the Screenshots tab itself (`shotsHref`). */
export function RowShotSign({ count }: { count: number }) {
  return <span className="shot-sign" title={title(count)}><Images className="icon" aria-hidden />{shotCount(count)}</span>;
}

export const shotsHref = (taskId: string) => href({ page: "task", id: taskId, view: "screenshots" });

/** The newest round's count of each card in a Review column that has screenshots (task id → count), again when a run
 *  ends. Cards elsewhere get none, to keep the board quiet. */
export function useReviewShots(tasks: Pick<Task, "id" | "stateCategory">[] | null | undefined): Map<string, number> {
  const ids = (tasks ?? []).filter((t) => t.stateCategory === "review").map((t) => t.id);
  const key = ids.join(" ");
  const [counts, setCounts] = useState<Map<string, number>>(() => new Map());
  useEffect(() => {
    const review = key ? key.split(" ") : [];
    if (!review.length) { setCounts(new Map()); return; }
    let alive = true, un: (() => void) | undefined;
    const load = () => Promise.all(review.map((id) => listScreenshots(id).then((s) => [id, roundCount(s)] as const).catch(() => [id, 0] as const)))
      .then((rows) => { if (alive) setCounts(new Map(rows.filter(([, n]) => n > 0))); });
    load();
    onRunsChanged(() => alive && load()).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, [key]);
  return counts;
}
