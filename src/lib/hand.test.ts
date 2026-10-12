// GA-115 QA: what Open in editor, the card bar in the Editor (Commit…, Push, Hand over) and "By hand" say, against the
// card's acceptance criteria and design/code-editor/editor.html §6: where the card stands, which button is on and primary
// (the next step's) and why one is off, what each did, where Hand over sent the card, when Open in editor shows, and
// QA sending the card back until you open it.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Member, Run, Task, Team, WorkflowState } from "../types";
import {
  backFromQa, backSay, byHandTip, canDoByHand, cardSeenAt, commitsWord, committedSay, couldntOpen, filesWord, handButtons, handedSay,
  handOverLabel, handState, markCardSeen, openInEditorTitle, plainError, SEEN_KEY, seenOf, testersOf, withSeen, type HandStand,
} from "./hand";

const member = (actorId: string, name: string): Member => ({ actorId, name, kind: "agent", roleKey: "qa", handle: name, status: "idle", isLead: false,
  allowedTools: [], chatEnabled: false } as Member);
const col = (id: string, name: string, category: string, sortKey: string, more: Partial<WorkflowState> = {}): WorkflowState =>
  ({ id, name, category, sortKey, ...more });
const team = (states: WorkflowState[], members: Member[] = [member("qa", "QA Agent"), member("qa2", "Second QA")]): Team =>
  ({ id: "team", name: "Software", members, states, labels: [] });
const SOFTWARE = team([
  col("s-todo", "To do", "ready", "a1"), col("s-prog", "In progress", "started", "a2"),
  col("s-test", "Testing", "testing", "a3", { auto: true, agentIds: ["qa"] }), col("s-rev", "Review", "review", "a4"), col("s-done", "Done", "done", "a5"),
]);
const stand = (more: Partial<HandStand> = {}): HandStand => ({ changed: 0, unpushed: 0, own: 0, link: true, ...more });
const run = (n: number, more: Partial<Run>): Run => ({ id: `r${n}`, agentId: "qa", agentName: "QA Agent", taskId: "t1", trigger: "task", status: "succeeded",
  createdAt: n * 1000, costUsdMicros: 0, inputTokens: 0, outputTokens: 0, logPath: "", ...more } as Run);

describe("words", () => {
  it("counts files and commits, one and many", () => {
    expect([filesWord(0), filesWord(1), filesWord(2), filesWord(1234)]).toEqual(["0 files", "1 file", "2 files", "1,234 files"]);
    expect([commitsWord(1), commitsWord(3)]).toEqual(["1 commit", "3 commits"]);
  });
});

describe("how far the card is, in the card bar", () => {
  it("changed files first, then commits not pushed, then nothing yet or all done", () => {
    expect(handState(stand({ changed: 2, unpushed: 3, own: 3 }))).toBe("2 files changed");
    expect(handState(stand({ changed: 1 }))).toBe("1 file changed");
    expect(handState(stand({ unpushed: 2, own: 2 }))).toBe("2 commits not pushed");
    expect(handState(stand({ own: 0 }))).toBe("No changes yet");
    expect(handState(stand({ own: 2 }))).toBe("Everything committed and pushed");
    expect(handState(stand({ own: 2, link: false }))).toBe("Everything committed");
  });
  it("while Gizai doesn't know the branch's own commits yet, it doesn't say no changes yet", () => {
    expect(handState(stand({ own: null }))).toBe("Everything committed and pushed");
  });
});

describe("Commit…, Push and Hand over as the card stands", () => {
  const o = { testing: true, testers: "QA Agent", project: "Kade portal" };

  it("with uncommitted changes: Commit… is on and primary, Hand over off saying to commit first", () => {
    const b = handButtons("KADE-41", "gizai/kade-41-fix", stand({ changed: 2, own: 0 }), o);
    expect(b.commit).toEqual({ off: false, primary: true, title: "Commit every change in KADE-41's worktree: 2 files" });
    expect(b.over).toEqual({ off: true, primary: false, title: "Commit your changes first: 2 files aren't committed." });
    expect(b.push).toEqual({ off: true, primary: false, title: "Nothing to push" });
    expect(handButtons("KADE-41", "b", stand({ changed: 1 }), o).over.title).toBe("Commit your changes first: 1 file isn't committed.");
  });

  it("with uncommitted changes and commits not pushed: Push is on too, but not primary", () => {
    const b = handButtons("KADE-41", "gizai/kade-41-fix", stand({ changed: 1, unpushed: 2, own: 2 }), o);
    expect(b.push).toEqual({ off: false, primary: false, title: "Push gizai/kade-41-fix: 2 commits the remote doesn't have" });
    expect(b.commit.primary).toBe(true);
    expect(b.over.off).toBe(true);
  });

  it("everything committed: Commit… off, Hand over on and primary, saying it pushes and who tests it", () => {
    const b = handButtons("KADE-41", "gizai/kade-41-fix", stand({ unpushed: 1, own: 1 }), o);
    expect(b.commit).toEqual({ off: true, primary: false, title: "Nothing to commit" });
    expect(b.push).toEqual({ off: false, primary: false, title: "Push gizai/kade-41-fix: 1 commit the remote doesn't have" });
    expect(b.over).toEqual({ off: false, primary: true, title: "Pushes, moves KADE-41 to Testing for QA Agent and takes the pin off" });
  });

  it("Hand over goes to Review for you when the card's Testing switch is off, and to Testing without a tester named", () => {
    expect(handButtons("KADE-41", "b", stand({ own: 1 }), { ...o, testing: false }).over.title).toBe("Pushes, moves KADE-41 to Review for you and takes the pin off");
    expect(handButtons("KADE-41", "b", stand({ own: 1 }), { ...o, testers: "" }).over.title).toBe("Pushes, moves KADE-41 to Testing and takes the pin off");
  });

  it("nothing done yet: every button off, Hand over saying to commit a change first", () => {
    const b = handButtons("KADE-41", "b", stand({ own: 0 }), o);
    expect([b.commit.off, b.push.off, b.over.off]).toEqual([true, true, true]);
    expect(b.over.title).toBe("Commit a change first");
  });

  it("a project without a link: Push off saying so, Hand over only moves it", () => {
    const b = handButtons("KADE-41", "b", stand({ own: 2, link: false }), o);
    expect(b.push).toEqual({ off: true, primary: false, title: "Kade portal has no GitHub, Bitbucket or git link to push to" });
    expect(b.over.title).toBe("Moves KADE-41 to Testing for QA Agent and takes the pin off");
  });

  it("while Gizai doesn't know the branch's own commits yet, Hand over is on (the backend still refuses uncommitted changes)", () => {
    expect(handButtons("KADE-41", "b", stand({ own: null }), o).over.off).toBe(false);
  });
});

describe("Hand over's label", () => {
  it("to Testing, or to Review with the Testing switch off", () => {
    expect(handOverLabel(true)).toBe("Hand over to Testing");
    expect(handOverLabel(false)).toBe("Hand over to Review");
  });
});

describe("who tests the card", () => {
  it("the agents of the team's first Testing column when it is Auto, else none", () => {
    expect(testersOf(SOFTWARE)).toBe("QA Agent");
    expect(testersOf(team([col("t2", "QA", "testing", "b2", { auto: true, agentIds: ["qa2"] }), col("t1", "Testing", "testing", "b1", { auto: true, agentIds: ["qa", "qa2"] })])))
      .toBe("QA Agent and Second QA");
    expect(testersOf(team([col("t1", "Testing", "testing", "b1", { auto: false, agentIds: ["qa"] })]))).toBe("");
    expect(testersOf(team([col("p", "In progress", "started", "a1")]))).toBe("");
    expect(testersOf(null)).toBe("");
  });
});

describe("what each button did", () => {
  it("Commit: the files and the short commit", () => {
    expect(committedSay(2, "3f5c1a29b8e7d6c5")).toBe("Committed 2 files as 3f5c1a2.");
    expect(committedSay(1, "abcdef1234")).toBe("Committed 1 file as abcdef1.");
    expect(committedSay(0, "abcdef1234")).toBe("Committed as abcdef1.");
  });

  it("Hand over: where the card went, who takes it, and that its worktree is read-only while QA works", () => {
    expect(handedSay("Testing", SOFTWARE)).toBe("Handed over to Testing: QA Agent takes it from here. Its worktree is read-only while an agent works on it.");
    expect(handedSay("Review", SOFTWARE)).toBe("Handed over to Review: it waits for your review.");
    const two = team([col("t", "Testing", "testing", "a1", { auto: true, agentIds: ["qa", "qa2"] })]);
    expect(handedSay("Testing", two)).toBe("Handed over to Testing: QA Agent and Second QA take it from here. Its worktree is read-only while an agent works on it.");
    const manual = team([col("t", "Testing", "testing", "a1", { auto: false, agentIds: ["qa"] })]);
    expect(handedSay("Testing", manual)).toBe("Handed over to Testing. Its worktree is read-only while an agent works on it.");
    expect(handedSay("Somewhere", null)).toBe("Handed over to Somewhere. Its worktree is read-only while an agent works on it.");
  });
});

describe("errors as sentences", () => {
  it("without Tauri's Error: and with a full stop", () => {
    expect(plainError("Error: KADE-41 has 2 uncommitted changes: commit them before you hand it over"))
      .toBe("KADE-41 has 2 uncommitted changes: commit them before you hand it over.");
    expect(plainError(new Error("Nothing to commit."))).toBe("Nothing to commit.");
    expect(plainError("Is it there?")).toBe("Is it there?");
  });

  it("Open in editor refused or failed: the card, then the reason starting in lowercase (not an identifier or a path)", () => {
    expect(couldntOpen("KADE-41", "Error: Npm ci failed in its new worktree (exit code 1)"))
      .toBe("Couldn't open KADE-41 in the editor: npm ci failed in its new worktree (exit code 1).");
    expect(couldntOpen("KADE-41", "KADE-41 isn't assigned to you")).toBe("Couldn't open KADE-41 in the editor: KADE-41 isn't assigned to you.");
    expect(couldntOpen("KADE-41", "/srv/x is gone")).toBe("Couldn't open KADE-41 in the editor: /srv/x is gone.");
  });
});

describe("Open in editor on the card's top bar", () => {
  const card = (more: Partial<Task> = {}) => ({ identifier: "KADE-41", byHand: false, stateCategory: "ready", assigneeId: "you", archivedAt: null, ...more });

  it("shows on a card assigned to you, in a column where work is done", () => {
    for (const c of ["backlog", "ready", "started", "testing", "review"]) expect(canDoByHand(card({ stateCategory: c }), "you"), c).toBe(true);
  });
  it("doesn't show on a card assigned to an agent, another person or nobody, nor for nobody", () => {
    expect(canDoByHand(card({ assigneeId: "agent-backend" }), "you")).toBe(false);
    expect(canDoByHand(card({ assigneeId: "someone-else" }), "you")).toBe(false);
    expect(canDoByHand(card({ assigneeId: null }), "you")).toBe(false);
    expect(canDoByHand(card(), undefined)).toBe(false);
    expect(canDoByHand(card(), "")).toBe(false);
  });
  it("doesn't show on an archived card, nor in Deploy, Done or Cancelled", () => {
    expect(canDoByHand(card({ archivedAt: 1 }), "you")).toBe(false);
    for (const c of ["deploy", "done", "cancelled"]) expect(canDoByHand(card({ stateCategory: c }), "you"), c).toBe(false);
  });

  it("its tooltip: off while an agent works on it, saying who; else what it does", () => {
    expect(openInEditorTitle(card(), "Backend Agent")).toBe("Backend Agent is working on KADE-41: wait for its run to end, or stop it.");
    expect(openInEditorTitle(card({ stateCategory: "ready" }), null))
      .toBe("Work on KADE-41 by hand: Gizai makes its worktree, pins the card to you so agents leave it alone, and moves it to In progress.");
    expect(openInEditorTitle(card({ stateCategory: "backlog" }), null)).toContain(", and moves it to In progress.");
    expect(openInEditorTitle(card({ stateCategory: "started" }), null))
      .toBe("Work on KADE-41 by hand: Gizai makes its worktree, pins the card to you so agents leave it alone.");
    expect(openInEditorTitle(card({ byHand: true, stateCategory: "started" }), null)).toBe("Open KADE-41's worktree in the Editor");
  });

  it("By hand's tooltip names the person", () => {
    expect(byHandTip({ assigneeName: "Jeffrey" })).toBe("Pinned to Jeffrey: agents leave it alone until it's handed over");
    expect(byHandTip({ assigneeName: null })).toBe("Pinned to a person: agents leave it alone until it's handed over");
  });
});

describe("when you last looked at a card", () => {
  it("reads what is kept, and nothing (or something unreadable) as none", () => {
    expect(seenOf(null)).toEqual({});
    expect(seenOf("not json")).toEqual({});
    expect(seenOf("[1,2]")).toEqual({});
    expect(seenOf("null")).toEqual({});
    expect(seenOf(JSON.stringify({ t1: 5, t2: "x" }))).toEqual({ t1: 5 });
  });

  it("keeps the latest 100 cards", () => {
    let kept: string | null = null;
    for (let i = 0; i < 105; i++) kept = withSeen(kept, `t${i}`, i);
    const all = seenOf(kept);
    expect(Object.keys(all)).toHaveLength(100);
    expect(all.t0).toBeUndefined();
    expect(all.t104).toBe(104);
    expect(seenOf(withSeen(kept, "t5", 200)).t5).toBe(200);
  });

  describe("in localStorage", () => {
    let store: Map<string, string>;
    beforeEach(() => {
      store = new Map();
      vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => void store.set(k, v) });
    });
    afterEach(() => vi.unstubAllGlobals());

    it("under gizai.cards.seen: 0 for a card never opened", () => {
      expect(cardSeenAt("t1")).toBe(0);
      markCardSeen("t1", 1234);
      expect(cardSeenAt("t1")).toBe(1234);
      expect(JSON.parse(store.get(SEEN_KEY)!)).toEqual({ t1: 1234 });
    });

    it("without localStorage (private mode), never throws", () => {
      vi.stubGlobal("localStorage", { getItem: () => { throw new Error("denied"); }, setItem: () => { throw new Error("denied"); } });
      expect(() => markCardSeen("t1", 1)).not.toThrow();
      expect(cardSeenAt("t1")).toBe(0);
    });
  });
});

describe("QA sent your card by hand back", () => {
  const mine = { byHand: true };

  it("the latest run failed it and you haven't opened the card since: that run", () => {
    const runs = [run(1, { agentName: "Backend Agent", outcome: "ready_for_testing" }), run(3, { outcome: "qa_fail", endedAt: 5000 })];
    expect(backFromQa(mine, runs, 4000)?.id).toBe("r3");
    expect(backFromQa(mine, runs, 0)?.id).toBe("r3");
  });
  it("not once you opened the card after it, nor when the latest run passed, nor on a card no longer by hand", () => {
    const failed = [run(3, { outcome: "qa_fail", endedAt: 5000 })];
    expect(backFromQa(mine, failed, 5000)).toBeNull();
    expect(backFromQa(mine, failed, 6000)).toBeNull();
    expect(backFromQa(mine, [run(1, { outcome: "qa_fail", endedAt: 1500 }), run(2, { outcome: "qa_pass", endedAt: 2500 })], 0)).toBeNull();
    expect(backFromQa({ byHand: false }, failed, 0)).toBeNull();
    expect(backFromQa(mine, [], 0)).toBeNull();
    expect(backFromQa(mine, null, 0)).toBeNull();
  });
  it("reads the latest run by when it was made, whatever their order, and a run without an end by its start", () => {
    const runs = [run(5, { outcome: "qa_fail" }), run(2, { outcome: "qa_pass", endedAt: 2500 })];
    expect(backFromQa(mine, runs, 4999)?.id).toBe("r5");
    expect(backFromQa(mine, runs, 5000)).toBeNull();
  });
  it("says who sent it back and where to read why", () => {
    expect(backSay({ agentName: "QA Agent" }, "KADE-41")).toBe("QA Agent sent KADE-41 back to you: what it found is on the card.");
  });
});
