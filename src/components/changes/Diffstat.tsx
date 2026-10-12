// Added and removed lines as the Changes view writes them (D1: `+964 −112`, in mono, diff-add and diff-del).
import { count } from "../../lib/changes";

/** `add` null leaves the added count out (a deleted file); a zero `del` isn't written. */
export function Diffstat({ add, del }: { add: number | null; del: number }) {
  return (
    <span className="diffstat">
      {add !== null && <span className="add">+{count(add)}</span>}
      {del > 0 && <span className="del">−{count(del)}</span>}
    </span>
  );
}
