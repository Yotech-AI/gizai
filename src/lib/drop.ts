type Rect = { left: number; top: number; right: number; bottom: number };

/** Tauri reports drag-and-drop positions in physical pixels; DOM rects are in CSS pixels. */
export function dropHits(pos: { x: number; y: number } | null | undefined, dpr: number, r: Rect): boolean {
  if (!pos) return false;
  const x = pos.x / (dpr || 1), y = pos.y / (dpr || 1);
  return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
}
