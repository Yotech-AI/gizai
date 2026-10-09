import { describe, expect, it } from "vitest";
import type { DragDropEvent } from "@tauri-apps/api/webview";
import { addDropZone, dropHits, dropTarget, routeDragDrop, type DropZone } from "./drop";
const rect = { left: 100, top: 50, right: 300, bottom: 150 };
describe("dropHits", () => {
  it("converts physical pixels to CSS pixels before testing the rectangle", () => {
    expect(dropHits({ x: 400, y: 200 }, 2, rect)).toBe(true);   // (200, 100) in CSS px
    expect(dropHits({ x: 400, y: 200 }, 1, rect)).toBe(false);  // (400, 200) is outside
  });
  it("counts the edges as inside and ignores a missing position", () => {
    expect(dropHits({ x: 100, y: 50 }, 1, rect)).toBe(true);
    expect(dropHits(null, 1, rect)).toBe(false);
  });
});

// GA-57: each drag-and-drop event goes to one zone at most. A drawer's zone (modal) takes every drop while it is open, so the
// Files section of the page behind it never gets the same files.
// A zone with a record of what it was told.
function rec(r: typeof rect | null, modal = false) {
  const log = { hover: [] as boolean[], dropped: [] as string[][] };
  const z: DropZone = { rect: () => r, modal, hover: (on) => log.hover.push(on), drop: (paths) => log.dropped.push(paths) };
  return { z, log };
}
const page = { left: 0, top: 0, right: 800, bottom: 600 };
const side = { left: 900, top: 0, right: 1200, bottom: 600 };
const at = (x: number, y: number) => ({ x, y });
const ev = (type: DragDropEvent["type"], x = 100, y = 100, paths = ["/home/jef/a.png"]): DragDropEvent =>
  (type === "leave" ? { type } : type === "over" ? { type, position: { x, y } } : { type, paths, position: { x, y } }) as DragDropEvent;

describe("dropTarget", () => {
  it("is the zone under the drop, else the only zone on screen, else none", () => {
    const a = rec(page), b = rec(side);
    expect(dropTarget(at(100, 100), 1, [a.z, b.z])).toBe(a.z);
    expect(dropTarget(at(1000, 100), 1, [a.z, b.z])).toBe(b.z);
    expect(dropTarget(at(850, 100), 1, [a.z, b.z])).toBeNull();
    expect(dropTarget(at(850, 100), 1, [a.z])).toBe(a.z);
    expect(dropTarget(null, 1, [a.z])).toBe(a.z);
    expect(dropTarget(at(100, 100), 1, [])).toBeNull();
  });
  it("is a drawer's zone while one is open, wherever the files land, also right over the page's zone", () => {
    const pageZone = rec(page), drawer = rec(side, true);
    expect(dropTarget(at(100, 100), 1, [pageZone.z, drawer.z])).toBe(drawer.z);
    expect(dropTarget(at(1000, 100), 1, [pageZone.z, drawer.z])).toBe(drawer.z);
    expect(dropTarget(null, 1, [pageZone.z, drawer.z])).toBe(drawer.z);
    // A drawer with no box (New task without a project) takes them too.
    const boxless = rec(null, true);
    expect(dropTarget(at(100, 100), 1, [pageZone.z, boxless.z])).toBe(boxless.z);
  });
  it("is the newest drawer's zone when two are open", () => {
    const older = rec(side, true), newer = rec(side, true), pageZone = rec(page);
    expect(dropTarget(at(100, 100), 1, [older.z, pageZone.z, newer.z])).toBe(newer.z);
  });
  it("uses CSS pixels for the zone under the drop", () => {
    const a = rec(page), b = rec(side);
    expect(dropTarget(at(2000, 200), 2, [a.z, b.z])).toBe(b.z); // (1000, 100) in CSS px
  });
});

describe("routeDragDrop", () => {
  it("shows the drag on the target only, and hands the dropped files to it alone", () => {
    const pageZone = rec(page), drawer = rec(side, true);
    const list = [pageZone.z, drawer.z];
    routeDragDrop(ev("enter", 100, 100), 1, list);
    routeDragDrop(ev("over", 120, 100), 1, list);
    expect(drawer.log.hover).toEqual([true, true]);
    expect(pageZone.log.hover).toEqual([false, false]);
    routeDragDrop(ev("drop", 120, 100, ["/home/jef/a.png", "/home/jef/b.pdf"]), 1, list);
    expect(drawer.log.dropped).toEqual([["/home/jef/a.png", "/home/jef/b.pdf"]]);
    expect(pageZone.log.dropped).toEqual([]);
    expect(drawer.log.hover.at(-1)).toBe(false);
    expect(pageZone.log.hover.at(-1)).toBe(false);
  });
  it("on leave, clears the drag everywhere and drops nothing", () => {
    const a = rec(page), b = rec(side);
    routeDragDrop(ev("enter", 100, 100), 1, [a.z, b.z]);
    routeDragDrop(ev("leave"), 1, [a.z, b.z]);
    expect(a.log.hover).toEqual([true, false]);
    expect(b.log.hover).toEqual([false, false]);
    expect(a.log.dropped).toEqual([]);
    expect(b.log.dropped).toEqual([]);
  });
  it("moves the drag from one page zone to the other as the files move", () => {
    const a = rec(page), b = rec(side);
    routeDragDrop(ev("over", 100, 100), 1, [a.z, b.z]);
    routeDragDrop(ev("over", 1000, 100), 1, [a.z, b.z]);
    expect(a.log.hover).toEqual([true, false]);
    expect(b.log.hover).toEqual([false, true]);
    routeDragDrop(ev("drop", 1000, 100), 1, [a.z, b.z]);
    expect(b.log.dropped).toHaveLength(1);
    expect(a.log.dropped).toHaveLength(0);
  });
  it("drops nothing when no zone takes the files", () => {
    const a = rec(page), b = rec(side);
    routeDragDrop(ev("drop", 850, 100), 1, [a.z, b.z]);
    expect(a.log.dropped).toEqual([]);
    expect(b.log.dropped).toEqual([]);
  });
});

describe("addDropZone", () => {
  it("adds a zone to the shared list and takes it off again", () => {
    const pageZone = rec(page), drawer = rec(side, true);
    const offPage = addDropZone(pageZone.z);
    const offDrawer = addDropZone(drawer.z);
    expect(dropTarget(at(100, 100), 1)).toBe(drawer.z);
    offDrawer();
    expect(dropTarget(at(100, 100), 1)).toBe(pageZone.z);
    offDrawer(); // a second call changes nothing
    expect(dropTarget(at(1000, 100), 1)).toBe(pageZone.z); // the only zone left
    offPage();
    expect(dropTarget(at(100, 100), 1)).toBeNull();
  });
});
