// GA-136 QA: the words of the sidebar's Terminal rows and of the Terminal page (src/lib/terminals.ts), against the table in
// design/terminal §8 word for word, and the small rules TF1 builds on: which closes ask first, the note from 20 open
// terminals, a card's identifier on its row, Ctrl+PgDn and Ctrl+PgUp's order and the tab that shows when one closes. The
// question a card that isn't pinned to you asks in the folder picker (FolderPicker's notYoursAsk) is here too.
import { describe, expect, it } from "vitest";
import type { TerminalSession } from "../types";
import {
  asksToClose, closeQuestion, doingWords, endedSay, folderNameOf, manyNote, MANY_OPEN, nextAfterClose, openTip, openWord, rowTitle,
  sentence, showsCardId, stepTerminal, tabTitle,
} from "./terminals";
import { notYoursAsk } from "../components/editor/FolderPicker";

const term = (more: Partial<TerminalSession> = {}): TerminalSession => ({
  id: "t2", name: "gizai 2", folder: "/home/you/Herd/gizai", folderPath: "~/Herd/gizai", taskId: null, shell: "zsh", busy: null, ended: null,
  cols: 132, rows: 38, openedAt: 1, ...more,
});
const KADE41 = { folder: "/home/you/.local/share/gizai/worktrees/KADE/KADE-41", folderPath: "~/.local/share/gizai/worktrees/KADE/KADE-41", taskId: "k41" };

describe("a sidebar row's tooltip (design §8)", () => {
  it("a program runs in it: '~/Herd/gizai · zsh · running npm'", () => {
    expect(rowTitle(term({ busy: "npm" }), null)).toBe("~/Herd/gizai · zsh · running npm");
  });
  it("the shell waits for a command: '~/Herd/gizai · zsh · waiting for a command'", () => {
    expect(rowTitle(term(), null)).toBe("~/Herd/gizai · zsh · waiting for a command");
  });
  it("a card's worktree whose shell ended with code 2: 'KADE-41's worktree · zsh ended with exit code 2'", () => {
    const ended = term({ ...KADE41, name: "KADE-41", ended: { code: 2, at: 1, why: "zsh ended with exit code 2" } });
    expect(rowTitle(ended, "KADE-41")).toBe("KADE-41's worktree · zsh ended with exit code 2");
    // without the backend's sentence it says the same in its own words, or that a signal ended it
    expect(rowTitle({ ...ended, ended: { code: 2, at: 1, why: null } }, "KADE-41")).toBe("KADE-41's worktree · zsh ended with exit code 2");
    expect(rowTitle({ ...ended, ended: { code: null, at: 1, why: null } }, "KADE-41")).toBe("KADE-41's worktree · zsh was ended by a signal");
  });
  it("takes the backend's sentence as it is ('sh ended with exit code 3')", () => {
    expect(doingWords(term({ shell: "sh", ended: { code: 3, at: 1, why: "sh ended with exit code 3" } }))).toBe("sh ended with exit code 3");
    expect(doingWords(term({ busy: "cargo" }))).toBe("running cargo");
    expect(doingWords(term())).toBe("waiting for a command");
  });
});

describe("a tab's tooltip (design §8)", () => {
  it("'gizai 2 · running npm', 'gizai · waiting for a command', 'gizai 3 · ended'", () => {
    expect(tabTitle(term({ busy: "npm" }))).toBe("gizai 2 · running npm");
    expect(tabTitle(term({ name: "gizai" }))).toBe("gizai · waiting for a command");
    expect(tabTitle(term({ name: "gizai 3", busy: "npm", ended: { code: 2, at: 1, why: "zsh ended with exit code 2" } }))).toBe("gizai 3 · ended");
  });
});

describe("closing (design §1 and §3)", () => {
  it("asks first only while a program runs: a terminal that waits for a command, or has ended, closes at once", () => {
    expect(asksToClose(term({ busy: "npm" }))).toBe(true);
    expect(asksToClose(term())).toBe(false);
    expect(asksToClose(term({ busy: "npm", ended: { code: 2, at: 1, why: null } }))).toBe(false);
  });
  it("asks in one sentence: 'npm is still running in gizai 2. Close it and end npm?'", () => {
    expect(closeQuestion(term({ busy: "npm" }))).toBe("npm is still running in gizai 2. Close it and end npm?");
    expect(closeQuestion(term({ name: "KADE-41", busy: "php" }))).toBe("php is still running in KADE-41. Close it and end php?");
  });
});

describe("the bar under the tabs (design §3 and §8)", () => {
  it("'The shell ended with exit code 2.' for any code but 0", () => {
    expect(endedSay({ code: 2, at: 1, why: "zsh ended with exit code 2" })).toBe("The shell ended with exit code 2.");
    expect(endedSay({ code: 127, at: 1, why: null })).toBe("The shell ended with exit code 127.");
  });
  it("'The shell was ended by a signal.' when a signal ended it", () => {
    expect(endedSay({ code: null, at: 1, why: "zsh was ended by SIGKILL" })).toBe("The shell was ended by a signal.");
    expect(endedSay({ code: null, at: 1, why: null })).toBe("The shell was ended by a signal.");
  });
  it("nothing while the shell runs", () => {
    expect(endedSay(null)).toBe("");
  });
  it("a shell that couldn't start says why, as a sentence: 'Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.'", () => {
    expect(sentence("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found")).toBe("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.");
    expect(sentence("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.")).toBe("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found.");
    expect(sentence("  Gizai couldn't open a terminal: it needs Windows 10 version 1809 or newer  ")).toBe("Gizai couldn't open a terminal: it needs Windows 10 version 1809 or newer.");
    expect(sentence("Why?")).toBe("Why?");
    expect(sentence("")).toBe("");
  });
});

describe("folded Terminal's heading and the note in New terminal (design §1, §2 and §8)", () => {
  it("'3 open', with '3 terminals are open' as its tooltip ('1 terminal is open' for one)", () => {
    expect(openWord(3)).toBe("3 open");
    expect(openTip(3)).toBe("3 terminals are open");
    expect(openWord(1)).toBe("1 open");
    expect(openTip(1)).toBe("1 terminal is open");
  });
  it("from 20 open terminals: '20 terminals are open: each keeps its shell running until you close it.', with how many are open", () => {
    expect(MANY_OPEN).toBe(20);
    expect(manyNote(0)).toBeNull();
    expect(manyNote(19)).toBeNull();
    expect(manyNote(20)).toBe("20 terminals are open: each keeps its shell running until you close it.");
    expect(manyNote(21)).toBe("21 terminals are open: each keeps its shell running until you close it.");
  });
});

describe("names", () => {
  it("the folder's own name, as the Editor names it", () => {
    expect(folderNameOf({ folderPath: "~/Herd/gizai" })).toBe("gizai");
    expect(folderNameOf({ folderPath: "~/Herd/gizai/" })).toBe("gizai");
    expect(folderNameOf({ folderPath: KADE41.folderPath })).toBe("KADE-41");
    expect(folderNameOf({ folderPath: "C:\\Users\\you\\Herd\\gizai" })).toBe("gizai");
  });
  it("a card's worktree shows its identifier in mono for the names it starts with ('KADE-41', 'KADE-41 2'), not for one given", () => {
    expect(showsCardId("KADE-41", "KADE-41")).toBe(true);
    expect(showsCardId("KADE-41 2", "KADE-41")).toBe(true);
    expect(showsCardId("KADE-410", "KADE-41")).toBe(false);
    expect(showsCardId("the server", "KADE-41")).toBe(false);
  });
});

describe("Ctrl+PgDn and Ctrl+PgUp (stepTerminal) and the tab after a close (nextAfterClose)", () => {
  const tabs = ["a", "b", "c"];
  it("steps to the next and the one before, round from the last to the first", () => {
    expect(stepTerminal(tabs, "a", 1)).toBe("b");
    expect(stepTerminal(tabs, "b", 1)).toBe("c");
    expect(stepTerminal(tabs, "c", 1)).toBe("a");
    expect(stepTerminal(tabs, "a", -1)).toBe("c");
    expect(stepTerminal(tabs, "c", -1)).toBe("b");
  });
  it("stays on a single tab, and has nowhere to go without tabs", () => {
    expect(stepTerminal(["a"], "a", 1)).toBe("a");
    expect(stepTerminal(["a"], "a", -1)).toBe("a");
    expect(stepTerminal([], "a", 1)).toBeNull();
  });
  it("starts from the first (or the last, going back) when the one shown isn't among them", () => {
    expect(stepTerminal(tabs, "x", 1)).toBe("a");
    expect(stepTerminal(tabs, "x", -1)).toBe("c");
  });
  it("shows the next tab in the folder when one closes, else the one before it, else none", () => {
    expect(nextAfterClose(tabs, "b", ["a", "c"])).toBe("c");
    expect(nextAfterClose(tabs, "c", ["a", "b"])).toBe("b");
    expect(nextAfterClose(tabs, "a", ["b", "c"])).toBe("b");
    expect(nextAfterClose(tabs, "b", ["a"])).toBe("a");
    expect(nextAfterClose(tabs, "b", [])).toBeNull();
    // the terminals of other folders (not among the tabs it had) don't count
    expect(nextAfterClose(["b"], "b", ["z"])).toBeNull();
  });
});

describe("New terminal in a card's worktree that isn't pinned to you (FolderPicker's notYoursAsk, design §2 and §8)", () => {
  const ga112 = { name: "GA-112" };
  it("asks with the design's sentence: 'GA-112 is Frontend Agent's card: a shell there can change its files, as an edit can.'", () => {
    expect(notYoursAsk(ga112, { byHand: false, assigneeId: "fe", assigneeName: "Frontend Agent" }, "u1"))
      .toBe("GA-112 is Frontend Agent's card: a shell there can change its files, as an edit can.");
  });
  it("asks for another person's card pinned to them, and for nobody's card", () => {
    expect(notYoursAsk(ga112, { byHand: true, assigneeId: "u2", assigneeName: "Mila" }, "u1"))
      .toBe("GA-112 is Mila's card: a shell there can change its files, as an edit can.");
    expect(notYoursAsk(ga112, { byHand: false, assigneeId: null, assigneeName: null }, "u1"))
      .toBe("GA-112 isn't assigned to anyone: a shell there can change its files, as an edit can.");
  });
  it("asks for an agent's card between runs, even when it is assigned to you without being pinned", () => {
    expect(notYoursAsk(ga112, { byHand: false, assigneeId: "u1", assigneeName: "Jeffrey" }, "u1")).not.toBeNull();
  });
  it("a card pinned to you (by hand) starts at once", () => {
    expect(notYoursAsk(ga112, { byHand: true, assigneeId: "u1", assigneeName: "Jeffrey" }, "u1")).toBeNull();
  });
});
