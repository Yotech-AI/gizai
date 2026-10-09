// The @ picker's list (design system: Popover), at the cursor of a Markdown editor or the chat composer, where it opens
// upward. It never takes focus: the editor keeps it (the task page's edit box saves when it loses focus), the editor
// hands it ↑, ↓, Enter and Escape, and a click on a row picks it.
import { Fragment, useEffect, useRef, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { Bot, Building2, FileText, FolderKanban, ListTodo, User, type LucideIcon } from "lucide-react";
import { KIND_GROUP, KIND_NAME, parseQuery, type ItemKind, type PickRow } from "../lib/itemLinks";

export const KIND_ICON: Record<ItemKind, LucideIcon> = { task: ListTodo, project: FolderKanban, client: Building2, agent: Bot, person: User, doc: FileText };

/** The cursor's box on screen, in CSS pixels. */
export type PickerAt = { left: number; top: number; bottom: number };

/** About how tall the list gets: below this much room under the cursor it opens upward. */
const ROOM = 300;

export function ItemPicker({ at, up, query, rows, loading, error, selected, onSelect, onPick }: {
  at: PickerAt; up?: boolean; query: string; rows: PickRow[]; loading: boolean; error?: string | null;
  selected: number; onSelect: (i: number) => void; onPick: (i: number) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => { list.current?.querySelector(".opt.active")?.scrollIntoView({ block: "nearest" }); }, [selected, rows]);
  const height = window.innerHeight;
  const upward = up ?? (height - at.bottom < ROOM && at.top > height - at.bottom);
  const style: CSSProperties = {
    left: Math.max(8, Math.min(at.left - 8, window.innerWidth - 368)),
    ...(upward ? { top: "auto", bottom: height - at.top + 6 } : { top: at.bottom + 6, bottom: "auto" }),
  };
  // Searching every kind: a heading above each kind's rows.
  const grouped = query !== "" && !parseQuery(query).kind;
  return createPortal(
    <div ref={list} className="pop item-picker" role="listbox" aria-label="Link an item" style={style}
      onMouseDown={(e) => e.preventDefault() /* keep the editor focused: leaving it saves or closes it */}>
      {query === "" && <div className="pop-label">Link a</div>}
      {rows.map((r, i) => {
        const prev = rows[i - 1];
        const head = grouped && r.type === "item" && (prev?.type !== "item" || prev.item.kind !== r.item.kind);
        const kind = r.type === "kind" ? r.kind : r.item.kind;
        const Icon = KIND_ICON[kind];
        return (
          <Fragment key={r.type === "kind" ? `kind:${r.kind}` : `${r.item.kind}:${r.item.key}`}>
            {head && <div className="pop-label">{KIND_GROUP[kind]}</div>}
            <button type="button" tabIndex={-1} role="option" aria-selected={i === selected} className={`opt${i === selected ? " active" : ""}`}
              onMouseMove={() => { if (i !== selected) onSelect(i); }} onClick={() => onPick(i)}>
              <Icon className="icon sm" />
              {r.type === "kind"
                ? <><span>{KIND_NAME[r.kind]}</span><span className="faint">@{r.kind}.</span></>
                : <><span className="ellipsis">{r.item.label}</span>{r.item.hint && <span className="faint ellipsis">{r.item.hint}</span>}</>}
            </button>
          </Fragment>
        );
      })}
      {rows.length === 0 && <div className="pop-empty">{error ? `Couldn't load the items: ${error}` : loading ? "Loading…" : "Nothing found"}</div>}
    </div>,
    document.body,
  );
}
