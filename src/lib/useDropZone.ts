// Files dropped on the window arrive through Tauri's native drag-and-drop event (HTML5 drop never fires in the webview;
// see the spike). One listener hears them all and lib/drop hands each one to a single zone.
import { useEffect, useRef, useState, type RefObject } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { addDropZone, routeDragDrop } from "./drop";

// Started by the first zone and kept: with no zones, an event goes nowhere.
let listening = false;
function listen() {
  if (listening) return;
  listening = true;
  try {
    getCurrentWebview().onDragDropEvent((ev) => routeDragDrop(ev.payload, window.devicePixelRatio)).catch(() => { listening = false; });
  } catch { listening = false; }
}

/** Makes the element a drop zone while `on`, and says whether dragged files are over it. `modal`: see DropZone. */
export function useDropZone(ref: RefObject<HTMLElement | null>, onDrop: (paths: string[]) => void, { on = true, modal = false } = {}): boolean {
  const [hover, setHover] = useState(false);
  const dropRef = useRef(onDrop);
  dropRef.current = onDrop;
  useEffect(() => {
    if (!on) return;
    listen();
    const remove = addDropZone({ rect: () => ref.current?.getBoundingClientRect() ?? null, modal, hover: setHover, drop: (paths) => dropRef.current(paths) });
    return () => { remove(); setHover(false); };
  }, [ref, on, modal]);
  return hover;
}
