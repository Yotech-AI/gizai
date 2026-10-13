// The Editor's terminal panel (GA-137, TF2; design/terminal/terminal.html §4 and §7, research §5.1 and §5.2): what it keeps
// on this computer, its height's limits and the keys that show it, apart from the screen so it can be tested.
// - Its height is a share of the code column (.ed-main), 30% the first time, the same in every folder (gizai.editor.panel).
// - Whether it shows is kept per folder (gizai.editor.terminal: the folders it shows in).
// localStorage is read inside try/catch: the tests run in Node, which has none.
import type { EditorFolder, Task, TerminalSession } from "../types";

export const PANEL_KEY = "gizai.editor.panel";
export const PANEL_SHOWN_KEY = "gizai.editor.terminal";
/** The first height, and the one a double-click on the handle gives back: 30% of the column. */
export const FIRST_SHARE = 30;
/** ↑ and ↓ on the handle move it this much. */
export const SHARE_STEP = 5;
/** The least height of the panel (its tabs and 4 lines) and of the code while the panel shows. */
export const PANEL_MIN = 120;
export const CODE_MIN = 120;

function readKey(key: string): string | null { try { return localStorage.getItem(key); } catch { return null; } }
function writeKey(key: string, value: string) { try { localStorage.setItem(key, value); } catch { /* private mode */ } }

/** The share kept (0 to 100); nothing kept, or something unreadable, is the first 30%. */
export function shareOf(kept: string | null): number {
  try {
    const n: unknown = JSON.parse(kept ?? "null");
    return typeof n === "number" && Number.isFinite(n) ? Math.min(100, Math.max(0, n)) : FIRST_SHARE;
  } catch { return FIRST_SHARE; }
}
export const loadShare = () => shareOf(readKey(PANEL_KEY));
export const keepShare = (share: number) => writeKey(PANEL_KEY, JSON.stringify(Math.round(share * 10) / 10));

/** The folders the panel shows in, from what localStorage keeps. */
export function shownFolders(kept: string | null): string[] {
  try {
    const list: unknown = JSON.parse(kept ?? "[]");
    return Array.isArray(list) ? list.filter((x): x is string => typeof x === "string" && x !== "") : [];
  } catch { return []; }
}
/** What to keep after the panel was shown (`on`) or hidden in `folder`. */
export function withShown(kept: string | null, folder: string, on: boolean): string {
  const rest = shownFolders(kept).filter((f) => f !== folder);
  return JSON.stringify(on ? [...rest, folder] : rest);
}
export const loadShown = (folder: string) => shownFolders(readKey(PANEL_SHOWN_KEY)).includes(folder);
export const keepShown = (folder: string, on: boolean) => writeKey(PANEL_SHOWN_KEY, withShown(readKey(PANEL_SHOWN_KEY), folder, on));

/** The panel's height for a wanted height in pixels, within its limits: at least PANEL_MIN of panel and CODE_MIN of code.
 *  `panel` and `code` are their heights now; the code gives what the panel takes. When there is no room for both, the
 *  panel keeps its least height. */
export function clampPanel(want: number, panel: number, code: number): number {
  const most = panel + code - CODE_MIN;
  return Math.max(PANEL_MIN, Math.min(want, most));
}
/** The share of the column (0 to 100) a panel height in pixels is. */
export function shareFor(height: number, column: number): number {
  if (!(column > 0)) return FIRST_SHARE;
  return Math.min(100, Math.max(0, (height / column) * 100));
}

/** What a key does in the Editor (research §5.2): Ctrl+` shows or hides the panel, Ctrl+Shift+` starts a new terminal in
 *  it; read by where the key is (Backquote, left of 1), so it works whatever character it has. Ctrl on all three systems,
 *  as in VS Code. */
export function panelKey(e: { code: string; key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }): "toggle" | "new" | null {
  if (!e.ctrlKey || e.altKey || e.metaKey) return null;
  if (e.code !== "Backquote" && !(e.code === "" && (e.key === "`" || e.key === "~"))) return null;
  return e.shiftKey ? "new" : "toggle";
}

/** Why a terminal can't start in the folder now (the Terminal button's and +'s tooltip, design §8), or null when it can:
 *  an agent works on the card, or the card isn't pinned to you and Edit anyway wasn't pressed (a shell can change files
 *  just as an edit can). */
export function terminalOff(folder: EditorFolder, task: Task | null, youId: string, editAnyway: boolean): string | null {
  if (folder.readOnly) return `${folder.readOnlyWhy ?? "An agent is working on it"}: a terminal there waits until its run ends.`;
  if (folder.kind !== "card") return null;
  const mine = !!task && task.byHand && task.assigneeId === youId;
  if (mine || editAnyway) return null;
  const owner = task?.assigneeName ?? null;
  return `${owner ? `${folder.name} is ${owner}'s card` : `${folder.name} isn't assigned to anyone`}: press Edit anyway first, since a shell can change its files.`;
}

/** A terminal tab's tooltip (design §8): "gizai 2 · running npm", "… · ended", "… · waiting for a command"; while its
 *  view is on its own page, "gizai 2 is on its own page." */
export function tabTitle(s: TerminalSession, onItsPage: boolean): string {
  if (onItsPage) return `${s.name} is on its own page.`;
  return `${s.name} · ${s.busy ? `running ${s.busy}` : s.ended ? "ended" : "waiting for a command"}`;
}
