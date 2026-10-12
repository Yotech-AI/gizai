// Added and removed lines as the Changes view writes them (D1: `+964 −112`, in mono, diff-add and diff-del).
import { count } from "../../lib/changes";

/** The added count is always written (a deleted file reads `+0 −45`, as in D1); a zero `del` isn't (an added file: `+35`). */
export function Diffstat({ add, del }: { add: number; del: number }) {
  return (
    <span className="diffstat">
      <span className="add">+{count(add)}</span>
      {del > 0 && <span className="del">−{count(del)}</span>}
    </span>
  );
}
