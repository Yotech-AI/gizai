// The [[ picker of a memory note's editor (GA-68), built like the @ picker (ItemPicker): a list at the cursor that never
// takes the focus; the editor hands it ↑, ↓, Enter, Tab and Escape, and a click picks a row. Notes show their folder;
// after # the note's headings, after | its aliases.
import { useEffect, useRef, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { FileText, Hash, Tag } from "lucide-react";
import { folderOf, titleOf, type WikiRow, type WikiTrigger } from "../../lib/memory";
import type { PickerAt } from "../ItemPicker";

const ROOM = 300;
const LABEL: Record<WikiTrigger["part"], string> = { note: "Link a note", heading: "Link a heading", alias: "Link with an alias" };

export function WikiPicker({ at, up, part, rows, selected, onSelect, onPick }: {
  at: PickerAt; up?: boolean; part: WikiTrigger["part"]; rows: WikiRow[]; selected: number; onSelect: (i: number) => void; onPick: (i: number) => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => { list.current?.querySelector(".opt.active")?.scrollIntoView({ block: "nearest" }); }, [selected, rows]);
  const height = window.innerHeight;
  const upward = up ?? (height - at.bottom < ROOM && at.top > height - at.bottom);
  const style: CSSProperties = {
    left: Math.max(8, Math.min(at.left - 8, window.innerWidth - 368)),
    ...(upward ? { top: "auto", bottom: height - at.top + 6 } : { top: at.bottom + 6, bottom: "auto" }),
  };
  return createPortal(
    <div ref={list} className="pop item-picker wiki-picker" role="listbox" aria-label={LABEL[part]} style={style}
      onMouseDown={(e) => e.preventDefault() /* keep the editor focused */}>
      <div className="pop-label">{LABEL[part]}</div>
      {rows.map((r, i) => (
        <button key={r.type === "note" ? r.note.id : `${r.type}:${r.text}`} type="button" tabIndex={-1} role="option" aria-selected={i === selected}
          className={`opt${i === selected ? " active" : ""}`} onMouseMove={() => { if (i !== selected) onSelect(i); }} onClick={() => onPick(i)}>
          {r.type === "note" ? <>
            <FileText className="icon sm" /><span className="ellipsis">{titleOf(r.note.path)}</span><span className="faint ellipsis">{folderOf(r.note.path)}</span>
          </> : r.type === "heading" ? <>
            <Hash className="icon sm" /><span className="ellipsis" style={{ paddingLeft: (r.level - 1) * 10 }}>{r.text}</span><span className="faint">H{r.level}</span>
          </> : <>
            <Tag className="icon sm" /><span className="ellipsis">{r.text}</span>
          </>}
        </button>
      ))}
    </div>,
    document.body,
  );
}
