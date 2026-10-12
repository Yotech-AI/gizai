// The Editor's menus (GA-113): a right-click menu (the tree's rows, the sidebar's folders) and Open in VS Code, Zed or
// PhpStorm, which lists only the editors Gizai found on this computer.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, ExternalLink } from "lucide-react";
import { editorApps, openInApp } from "../../api";
import type { EditorApp } from "../../types";
import { Popover } from "../Popover";

export type MenuItem =
  | { label: string; kbd?: string; disabled?: boolean; title?: string; run: () => void }
  | { sep: true }
  | { note: string };

/** A menu at the mouse (or under a row, from the keyboard), on top of everything; it closes on a click outside, Escape,
 *  scrolling, or a choice. ↑ and ↓ move between its items. */
export function ContextMenu({ at, items, label, onClose }: { at: { x: number; y: number }; items: MenuItem[]; label: string; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState(at);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({ x: Math.max(4, Math.min(at.x, window.innerWidth - r.width - 4)), y: Math.max(4, Math.min(at.y, window.innerHeight - r.height - 4)) });
    (el.querySelector("button:not(:disabled)") as HTMLButtonElement | null)?.focus();
  }, [at.x, at.y]);
  useEffect(() => {
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); return; }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const all = [...(ref.current?.querySelectorAll("button:not(:disabled)") ?? [])] as HTMLButtonElement[];
      const i = all.indexOf(document.activeElement as HTMLButtonElement);
      all[(i + (e.key === "ArrowDown" ? 1 : -1) + all.length) % all.length]?.focus();
    };
    window.addEventListener("mousedown", down);
    window.addEventListener("keydown", key, true);
    window.addEventListener("blur", onClose);
    return () => { window.removeEventListener("mousedown", down); window.removeEventListener("keydown", key, true); window.removeEventListener("blur", onClose); };
  }, [onClose]);
  return createPortal(
    <div ref={ref} className="pop" role="menu" aria-label={label} style={{ position: "fixed", left: pos.x, top: pos.y, maxHeight: "none" }}
      onContextMenu={(e) => e.preventDefault()}>
      {items.map((it, i) => "sep" in it ? <div key={i} className="sep" />
        : "note" in it ? <div key={i} className="pop-note">{it.note}</div>
        : <button key={i} className="opt" role="menuitem" disabled={it.disabled} title={it.title} onClick={() => { onClose(); it.run(); }}>
            {it.label}{it.kbd && <span className="kbd">{it.kbd}</span>}
          </button>)}
    </div>,
    document.body,
  );
}

let apps: Promise<EditorApp[]> | null = null;
/** VS Code, Zed and PhpStorm, those found on this computer: asked once, then the same answer. */
export function useEditorApps(): EditorApp[] | null {
  const [list, setList] = useState<EditorApp[] | null>(null);
  useEffect(() => {
    let alive = true;
    apps ??= editorApps().catch(() => { apps = null; return []; });
    apps.then((a) => { if (alive) setList(a); });
    return () => { alive = false; };
  }, []);
  return list;
}

/** What Open in says when it found none of them. */
export const NO_APP = "No other editor found. Gizai looks for VS Code (code), Zed (zed, zeditor, zedit) and PhpStorm (phpstorm, phpstorm.sh).";

/** The Open in items for a menu: one per editor found (the folder, or the file at its line), or why there are none. */
export function openInItems(list: EditorApp[] | null, folder: string, path: string | null, line: number | null, fail: (e: string) => void): MenuItem[] {
  if (!list) return [{ note: "Looking for editors…" }];
  if (!list.length) return [{ note: NO_APP }];
  return list.map((a) => ({ label: `Open in ${a.name}`, run: () => { openInApp(a.id, folder, path, line).catch((e) => fail(String(e))); } }));
}

/** Open in, on the Editor's top bar: the editors found, each opening the folder at the open file and line. */
export function OpenInMenu({ folder, name, path, line, fail }: { folder: string; name: string; path: string | null; line: number | null; fail: (e: string) => void }) {
  const list = useEditorApps();
  const where = path ? `At ${path.slice(path.lastIndexOf("/") + 1)}${line ? `, line ${line}` : ""}` : null;
  return (
    <Popover label="Open in" align="right" button={() => <button className="btn sm"><ExternalLink className="icon" />Open in<ChevronDown className="icon" /></button>}>
      {(close) => (
        <>
          <div className="pop-label">Open {name} in</div>
          {!list ? <div className="pop-note">Looking for editors…</div>
            : !list.length ? <div className="pop-note" style={{ maxWidth: 300 }}>{NO_APP}</div>
            : list.map((a) => (
              <button key={a.id} className="opt" role="menuitem" onClick={() => { close(); openInApp(a.id, folder, path, line).catch((e) => fail(String(e))); }}>{a.name}</button>
            ))}
          {where && !!list?.length && <div className="pop-note">{where}</div>}
        </>
      )}
    </Popover>
  );
}
