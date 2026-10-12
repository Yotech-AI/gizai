// GA-111: contract C4 with the card bar and Open in editor (F4), word for word: src/api.ts and src/types.ts hold the
// card's lines as written, each call invokes its command with the arguments Tauri expects, and src-tauri has the five
// commands in editor.rs, with their arguments in snake case, handed to Tauri in lib.rs. Task gains byHand.
import { describe, expect, it, vi } from "vitest";
import apiSource from "./api.ts?raw";
import typesSource from "./types.ts?raw";
import type { Commit, HandCard, Task } from "./types";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

// The Rust side is read from disk. node:fs comes in through a computed specifier: the build type-checks tests without
// Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const rust = (path: string): string => fs.readFileSync(new URL(`../src-tauri/src/${path}`, import.meta.url), "utf8");

/** Contract C4's `src/api.ts`, as the card has it. */
const C4_API = `
/** Open in editor, on a card assigned to you: makes and prepares its worktree as for an agent, pins the card to you and moves
 *  it on from To do. Resolves with where it is (its folder is an EditorFolder id). */
export const startByHand = (taskId: string) => invoke<T.HandCard>("start_by_hand", { taskId });
export const handCard = (taskId: string) => invoke<T.HandCard>("hand_card", { taskId });
/** Commits every change in the card's worktree with this message. */
export const handCommit = (taskId: string, message: string) => invoke<T.Commit>("hand_commit", { taskId, message });
/** Pushes the card's branch like Gizai's push after a run; resolves with what it did ("Pushed gizai/ga-123-… (2 commits)."). */
export const handPush = (taskId: string) => invoke<string>("hand_push", { taskId });
/** Refuses uncommitted changes; pushes, moves the card to Testing (Review with its Testing switch off) and takes the pin off.
 *  Resolves with the column's name. */
export const handOver = (taskId: string) => invoke<string>("hand_over", { taskId });
`;

/** Contract C4's `src/types.ts`: the new type, and the field Task gains. */
const C4_HAND_CARD = "export type HandCard = { taskId: string; folder: string; branch: string; changed: number; unpushed: number };";
const C4_BY_HAND = "  byHand: boolean;         // pinned to a person: agents leave it alone";

/** The commands of C4 and their arguments in snake case, in the order of src/api.ts. */
const COMMANDS: [string, string[]][] = [
  ["start_by_hand", ["task_id"]],
  ["hand_card", ["task_id"]],
  ["hand_commit", ["task_id", "message"]],
  ["hand_push", ["task_id"]],
  ["hand_over", ["task_id"]],
];

describe("contract C4 in src/api.ts and src/types.ts (GA-111)", () => {
  it("api.ts holds C4's calls word for word, in C4's order", () => {
    expect(apiSource).toContain(C4_API.trim());
  });

  it("types.ts holds HandCard word for word, and Task gains byHand", () => {
    expect(typesSource).toContain(C4_HAND_CARD);
    const task = typesSource.slice(typesSource.indexOf("export type Task = {"));
    expect(task.slice(0, task.indexOf("\n};")).split("\n")).toContain(C4_BY_HAND);
  });

  it("each call invokes its command with the arguments as C4 names them", async () => {
    invoked.length = 0;
    await api.startByHand("t1");
    await api.handCard("t1");
    await api.handCommit("t1", "Export invoices as CSV");
    await api.handPush("t1");
    await api.handOver("t1");
    expect(invoked).toEqual([
      ["start_by_hand", { taskId: "t1" }],
      ["hand_card", { taskId: "t1" }],
      ["hand_commit", { taskId: "t1", message: "Export invoices as CSV" }],
      ["hand_push", { taskId: "t1" }],
      ["hand_over", { taskId: "t1" }],
    ]);
  });

  it("types the answers as C4 says", () => {
    // Type-checked by npm run build (tsc -b also checks the tests).
    const card: HandCard = { taskId: "t1", folder: "/home/you/.local/share/gizai/worktrees/GA/GA-123", branch: "gizai/ga-123-export",
      changed: 2, unpushed: 1 };
    const made: Commit = { sha: "abc1234", subject: "Export invoices as CSV" };
    const byHand: Task["byHand"] = true;
    const answers: [Promise<HandCard>, Promise<Commit>, Promise<string>, Promise<string>] =
      [api.handCard("t1"), api.handCommit("t1", "m"), api.handPush("t1"), api.handOver("t1")];
    expect([card, made, byHand, answers.length]).toHaveLength(4);
  });
});

describe("contract C4 in src-tauri (GA-111)", () => {
  const editor = rust("editor.rs");
  const lib = rust("lib.rs");
  const model = fs.readFileSync(new URL("../crates/gizai-core/src/model.rs", import.meta.url), "utf8") as string;

  it("has the five commands in editor.rs, by their names in snake case and with their arguments in snake case", () => {
    const commands = [...editor.matchAll(/#\[tauri::command\]\s*pub async fn (\w+)\(([^)]*)\)/g)]
      .map(([, name, params]) => [name, params.split(",").filter((p) => p.includes(":")).map((p) => p.split(":")[0].trim()).filter((p) => p !== "st")])
      .filter(([name]) => COMMANDS.some(([c]) => c === name));
    expect(commands).toEqual(COMMANDS);
  });

  it("hands every command to Tauri", () => {
    const handler = lib.slice(lib.indexOf("generate_handler!["));
    for (const [name] of COMMANDS) expect(handler, name).toContain(`editor::${name}`);
  });

  it("Task gains by_hand, which Tauri sends as byHand", () => {
    const task = model.slice(model.indexOf("pub struct Task {"));
    expect(task.slice(0, task.indexOf("\n}"))).toContain("pub by_hand: bool,");
    expect(model.slice(model.lastIndexOf("#[serde(rename_all", model.indexOf("pub struct Task {")), model.indexOf("pub struct Task {")))
      .toContain('#[serde(rename_all = "camelCase")]');
  });
});
