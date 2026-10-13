// GA-137 QA: the Editor's terminal panel, its pure parts (src/lib/terminalPanel.ts), against TF2's acceptance criteria and
// design/terminal/terminal.html §4, §7 and §8: the height kept as a share of the column, the same in every folder; whether
// the panel shows, kept per folder; the drag's limits (120px of code, 120px of panel) and 30% back; Ctrl+` and
// Ctrl+Shift+` read by where the key is; why the Terminal button and + are off (an agent works on the card, or it isn't
// pinned to you before Edit anyway), word for word as §8 gives them; and the tabs' tooltips.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditorFolder, Task, TerminalSession } from "../types";
import {
  CODE_MIN, clampPanel, FIRST_SHARE, keepShare, keepShown, loadShare, loadShown, PANEL_KEY, PANEL_MIN, PANEL_SHOWN_KEY, panelKey,
  SHARE_STEP, shareFor, shareOf, shownFolders, tabTitle, terminalOff, withShown,
} from "./terminalPanel";

const store = new Map<string, string>();
beforeEach(() => {
  store.clear();
  vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
});
afterEach(() => { vi.unstubAllGlobals(); });

describe("the panel's height: a share of the column, kept once for every folder (gizai.editor.panel)", () => {
  it("is 30% the first time, and the design's limits are 120px of code and 120px of panel", () => {
    expect([FIRST_SHARE, PANEL_MIN, CODE_MIN, SHARE_STEP, PANEL_KEY]).toEqual([30, 120, 120, 5, "gizai.editor.panel"]);
    expect(loadShare(), "nothing kept yet: 30%").toBe(30);
  });

  it("reads back what was kept, and anything it can't read as 30%", () => {
    expect(shareOf(null)).toBe(30);
    expect(shareOf("42.5")).toBe(42.5);
    expect(shareOf("0")).toBe(0);
    expect(shareOf("150"), "never more than the whole column").toBe(100);
    expect(shareOf("-4"), "never less than nothing").toBe(0);
    for (const bad of ["", "abc", "\"40\"", "[40]", "{}", "null", "NaN", "Infinity"]) expect(shareOf(bad), `kept ${JSON.stringify(bad)}`).toBe(30);
  });

  it("keeps a share to a tenth of a percent, and the next panel (any folder, after a reload) opens at it", () => {
    keepShare(47.3333);
    expect(store.get(PANEL_KEY)).toBe("47.3");
    expect(loadShare()).toBe(47.3);
    keepShare(30);
    expect(loadShare()).toBe(30);
    // one key for all folders: nothing in it names a folder
    expect([...store.keys()]).toEqual([PANEL_KEY]);
  });

  it("works without localStorage (private mode, Node): nothing is kept and the panel opens at 30%", () => {
    vi.stubGlobal("localStorage", { getItem: () => { throw new Error("denied"); }, setItem: () => { throw new Error("denied"); } });
    expect(() => keepShare(50)).not.toThrow();
    expect(loadShare()).toBe(30);
    expect(() => keepShown("/f", true)).not.toThrow();
    expect(loadShown("/f")).toBe(false);
  });
});

describe("whether the panel shows: kept per folder (gizai.editor.terminal)", () => {
  it("is hidden in a folder until it was shown there", () => {
    expect(PANEL_SHOWN_KEY).toBe("gizai.editor.terminal");
    expect(loadShown("/srv/Herd/gizai")).toBe(false);
  });

  it("keeps each folder apart: showing it in one folder doesn't show it in another, hiding it there only hides it there", () => {
    keepShown("/srv/Herd/gizai", true);
    keepShown("/srv/Herd/kade", true);
    expect([loadShown("/srv/Herd/gizai"), loadShown("/srv/Herd/kade"), loadShown("/srv/Herd/other")]).toEqual([true, true, false]);
    keepShown("/srv/Herd/gizai", false);
    expect([loadShown("/srv/Herd/gizai"), loadShown("/srv/Herd/kade")]).toEqual([false, true]);
    expect(JSON.parse(store.get(PANEL_SHOWN_KEY)!)).toEqual(["/srv/Herd/kade"]);
  });

  it("lists a folder once, however often the panel is shown there", () => {
    keepShown("/a", true);
    keepShown("/a", true);
    keepShown("/b", true);
    keepShown("/a", true);
    expect(shownFolders(store.get(PANEL_SHOWN_KEY)!)).toEqual(["/b", "/a"]);
  });

  it("reads anything else as no folder", () => {
    for (const bad of [null, "", "x", "{}", "42", "null"]) expect(shownFolders(bad), `kept ${JSON.stringify(bad)}`).toEqual([]);
    expect(shownFolders(JSON.stringify(["/a", 3, "", null, "/b"])), "only folder names").toEqual(["/a", "/b"]);
    expect(JSON.parse(withShown("not json", "/a", true))).toEqual(["/a"]);
    expect(JSON.parse(withShown(null, "/a", false))).toEqual([]);
  });
});

describe("dragging the handle: at least 120px of code and 120px of panel; the share counts the whole column", () => {
  // the design's frame: a 600px column (.ed-main) with 36px of file tabs and 24px of status bar; the panel at 30% (180px)
  // leaves 360px of code
  const column = 600, panel = 180, code = 360;

  it("follows the pointer between the limits", () => {
    expect(clampPanel(250, panel, code)).toBe(250);
    expect(clampPanel(180, panel, code)).toBe(180);
    expect(shareFor(clampPanel(250, panel, code), column)).toBeCloseTo(41.67, 2);
  });

  it("stops with 120px of code left: the code gives what the panel takes", () => {
    const most = clampPanel(10_000, panel, code);
    expect(most).toBe(panel + code - 120);
    expect(panel + code - most, "the code keeps 120px").toBe(CODE_MIN);
    expect(shareFor(most, column)).toBe(70);
  });

  it("stops at 120px of panel", () => {
    expect(clampPanel(30, panel, code)).toBe(120);
    expect(clampPanel(-500, panel, code)).toBe(120);
    expect(shareFor(clampPanel(30, panel, code), column)).toBe(20);
  });

  it("keeps the panel at 120px when the column has no room for both", () => {
    // the smallest window: no 240px for code and panel together
    expect(clampPanel(200, 120, 60)).toBe(120);
    expect(clampPanel(50, 120, 60)).toBe(120);
  });

  it("measures the share against the column, so 30% stays 30% when the window grows", () => {
    expect(shareFor(180, 600)).toBe(30);
    expect(shareFor(240, 800)).toBe(30);
    expect(shareFor(0, 600)).toBe(0);
    expect(shareFor(900, 600), "never more than the column").toBe(100);
    expect(shareFor(100, 0), "a column not measured yet: 30%").toBe(30);
    expect(shareFor(100, Number.NaN)).toBe(30);
  });

  it("moves 5% of the column per arrow key, within the same limits", () => {
    const up = clampPanel(panel + (SHARE_STEP * column) / 100, panel, code);
    expect(shareFor(up, column)).toBe(35);
    const down = clampPanel(120 - (SHARE_STEP * column) / 100, 120, 420);
    expect(shareFor(down, column), "not under 120px").toBe(20);
  });
});

describe("the keys (research §5.2): Ctrl+` shows or hides the panel, Ctrl+Shift+` starts a terminal in it", () => {
  const k = (more: Partial<{ code: string; key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }>) =>
    panelKey({ code: "Backquote", key: "`", ctrlKey: true, shiftKey: false, altKey: false, metaKey: false, ...more });

  it("reads Ctrl+` as show or hide, and Ctrl+Shift+` as a new terminal", () => {
    expect(k({})).toBe("toggle");
    expect(k({ shiftKey: true, key: "~" })).toBe("new");
  });

  it("goes by where the key is (left of 1), whatever character the layout gives it", () => {
    expect(k({ key: "^" }), "German").toBe("toggle");
    expect(k({ key: "²" }), "French").toBe("toggle");
    expect(k({ key: "°", shiftKey: true })).toBe("new");
    expect(k({ code: "", key: "`" }), "no code: the character").toBe("toggle");
    expect(k({ code: "", key: "~", shiftKey: true })).toBe("new");
    expect(k({ code: "", key: "^" })).toBe(null);
  });

  it("leaves every other key alone: without Ctrl, with Alt or Cmd, or another key", () => {
    expect(k({ ctrlKey: false })).toBe(null);
    expect(k({ altKey: true })).toBe(null);
    expect(k({ metaKey: true }), "Ctrl on all three systems, as in VS Code").toBe(null);
    for (const [code, key] of [["KeyW", "w"], ["KeyP", "p"], ["KeyB", "b"], ["Digit1", "1"], ["Quote", "'"]]) expect(k({ code, key }), key).toBe(null);
  });
});

describe("why a terminal can't start in the folder (the Terminal button's and +'s tooltip, design §8)", () => {
  const folder = (more: Partial<EditorFolder> = {}): EditorFolder => ({
    id: "/w/GA-112", path: "~/.local/share/gizai/worktrees/GA/GA-112", name: "GA-112", kind: "card", projectId: "p1", taskId: "t1", readOnly: false, readOnlyWhy: null, ...more,
  });
  const card = (more: Partial<Task>) => ({ id: "t1", identifier: "GA-112", byHand: false, assigneeId: null, assigneeName: null, ...more }) as Task;

  it("is off while an agent works on the card, with who works and that it waits for the run", () => {
    const busy = folder({ readOnly: true, readOnlyWhy: "QA Agent is working on GA-118", name: "GA-118" });
    expect(terminalOff(busy, card({}), "u1", false)).toBe("QA Agent is working on GA-118: a terminal there waits until its run ends.");
    expect(terminalOff(busy, card({ byHand: true, assigneeId: "u1" }), "u1", true), "Edit anyway doesn't open it while the agent works")
      .toBe("QA Agent is working on GA-118: a terminal there waits until its run ends.");
    expect(terminalOff(folder({ readOnly: true }), null, "u1", false)).toBe("An agent is working on it: a terminal there waits until its run ends.");
  });

  it("is off on another's card until Edit anyway, then on", () => {
    const theirs = card({ assigneeId: "a2", assigneeName: "Frontend Agent" });
    expect(terminalOff(folder(), theirs, "u1", false)).toBe("GA-112 is Frontend Agent's card: press Edit anyway first, since a shell can change its files.");
    expect(terminalOff(folder(), theirs, "u1", true)).toBe(null);
  });

  it("is off on nobody's card, and on a card assigned to you that isn't pinned to you by hand, until Edit anyway", () => {
    expect(terminalOff(folder(), card({}), "u1", false)).toBe("GA-112 isn't assigned to anyone: press Edit anyway first, since a shell can change its files.");
    expect(terminalOff(folder(), null, "u1", false), "the card not loaded yet").toBe("GA-112 isn't assigned to anyone: press Edit anyway first, since a shell can change its files.");
    expect(terminalOff(folder(), card({ assigneeId: "u1", assigneeName: "Jefsev" }), "u1", false)).toBe("GA-112 is Jefsev's card: press Edit anyway first, since a shell can change its files.");
    expect(terminalOff(folder(), card({}), "u1", true)).toBe(null);
  });

  it("is on for a card pinned to you by hand, and for projects' folders and folders in ~/Herd", () => {
    expect(terminalOff(folder(), card({ byHand: true, assigneeId: "u1", assigneeName: "Jefsev" }), "u1", false)).toBe(null);
    expect(terminalOff(folder({ kind: "project", taskId: null }), null, "u1", false)).toBe(null);
    expect(terminalOff(folder({ kind: "root", taskId: null }), null, "u1", false)).toBe(null);
  });
});

describe("a terminal tab's tooltip (design §8)", () => {
  const s = (more: Partial<TerminalSession>): TerminalSession => ({
    id: "t", name: "gizai 2", folder: "/h/gizai", folderPath: "~/Herd/gizai", taskId: null, shell: "zsh", busy: null, ended: null, cols: 80, rows: 24, openedAt: 0, ...more,
  });

  it("says what runs in it, that it waits, or that it ended", () => {
    expect(tabTitle(s({ busy: "npm" }), false)).toBe("gizai 2 · running npm");
    expect(tabTitle(s({}), false)).toBe("gizai 2 · waiting for a command");
    expect(tabTitle(s({ ended: { code: 2, at: 1, why: null } }), false)).toBe("gizai 2 · ended");
  });

  it("says it is on its own page while its view is there", () => {
    expect(tabTitle(s({ busy: "npm" }), true)).toBe("gizai 2 is on its own page.");
  });
});
