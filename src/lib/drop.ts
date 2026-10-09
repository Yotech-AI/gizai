import type { DragDropEvent } from "@tauri-apps/api/webview";

type Rect = { left: number; top: number; right: number; bottom: number };

/** Tauri reports drag-and-drop positions in physical pixels; DOM rects are in CSS pixels. */
export function dropHits(pos: { x: number; y: number } | null | undefined, dpr: number, r: Rect): boolean {
  if (!pos) return false;
  const x = pos.x / (dpr || 1), y = pos.y / (dpr || 1);
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
}

/** A place that takes dropped files: the Files section of a page, or the one in an open drawer. */
export type DropZone = {
  /** Its box on screen, in CSS pixels; null when it has none. */
  rect: () => Rect | null;
  /** In a drawer: it takes every drop while it is open, wherever the files land, so the page behind it gets none. */
  modal?: boolean;
  /** Dragged files are over it (true), or went elsewhere or away (false). */
  hover: (on: boolean) => void;
  drop: (paths: string[]) => void;
};

// Every mounted zone, oldest first.
const zones: DropZone[] = [];

/** Adds a zone; the function it returns takes it off again. */
export function addDropZone(zone: DropZone): () => void {
  zones.push(zone);
  return () => {
    const i = zones.indexOf(zone);
    if (i >= 0) zones.splice(i, 1);
  };
}

/** The one zone files dropped at `pos` go to: a drawer's zone while a drawer has one open, else the zone under them,
 *  else the only zone on screen. Null: none takes them. */
export function dropTarget(pos: { x: number; y: number } | null | undefined, dpr: number, list: readonly DropZone[] = zones): DropZone | null {
  for (let i = list.length - 1; i >= 0; i--) if (list[i].modal) return list[i];
  const under = list.find((z) => { const r = z.rect(); return !!r && dropHits(pos, dpr, r); });
  return under ?? (list.length === 1 ? list[0] : null);
}

/** Hands one drag-and-drop event to one zone at most: only that zone shows the drag, and only it gets the dropped files. */
export function routeDragDrop(ev: DragDropEvent, dpr: number, list: readonly DropZone[] = zones): void {
  const target = ev.type === "leave" ? null : dropTarget(ev.position, dpr, list);
  for (const z of [...list]) z.hover(ev.type !== "drop" && z === target);
  if (ev.type === "drop" && target) target.drop(ev.paths);
}
