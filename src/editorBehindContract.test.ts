// GA-110: the Editor's contract C3 with the behind bar (F3), word for word: src/api.ts and src/types.ts hold the card's
// lines as written, each call invokes its command with the arguments Tauri expects, and onEditorUpdated hands on the
// editor-updated event's payload. src-tauri's commands are checked with C2's in editorContract.test.ts.
import { describe, expect, it, vi } from "vitest";
import apiSource from "./api.ts?raw";
import typesSource from "./types.ts?raw";
import type { EditorUpdated, FolderMain } from "./types";

const invoked: [string, unknown][] = [];
const listening: [string, (m: { payload: unknown }) => void][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, cb: (m: { payload: unknown }) => void) => { listening.push([event, cb]); return Promise.resolve(() => {}); },
}));

const api = await import("./api");

/** Contract C3's `src/api.ts`, as the card has it. */
const C3_API = `
/** How the folder stands against main as last fetched (local only). */
export const editorMain = (folder: string) => invoke<T.FolderMain>("editor_main", { folder });
/** Fetches main first (at most 60 s; not again within a minute), then as editorMain. */
export const editorFetch = (folder: string) => invoke<T.FolderMain>("editor_fetch", { folder });
/** Starts Update in the background: resolves with what it will do, or rejects with why not (nothing changed). */
export const editorUpdate = (folder: string) => invoke<string>("editor_update", { folder });
export const onEditorUpdated = (cb: (e: T.EditorUpdated) => void): Promise<UnlistenFn> =>
  listen<T.EditorUpdated>("editor-updated", (m) => cb(m.payload));
`;

/** Contract C3's `src/types.ts`, as the card has it (with `output`, which D2's design asks for its Show output). */
const C3_TYPES = `
export type FolderMain = {
  mainLabel: string;          // "origin/main"
  fetchedAt: number | null;   // ms since 1970; null: never fetched
  fetchError: string | null;
  branch: string | null;
  onDefault: boolean;         // on the default branch
  behind: number;
  ahead: number;
  uncommitted: number;
  canUpdate: boolean;
  updateSays: string;         // what Update will do (the plan's sentence), or why it can't
  updating: boolean;
};
export type EditorUpdated = {
  folder: string;
  ok: boolean;
  said: string;               // "Moved main 3 commits to 319bf0b and ran npm ci."
  output: string;             // what git and the install printed, for Show output; "" when nothing
};
`;

describe("contract C3 in src/api.ts and src/types.ts (GA-110)", () => {
  it("api.ts and types.ts hold C3 word for word", () => {
    expect(apiSource).toContain(C3_API.trim());
    expect(typesSource).toContain(C3_TYPES.trim());
  });

  it("each call invokes its command with the folder, and onEditorUpdated hands on editor-updated's payload", async () => {
    invoked.length = 0;
    const shop = "/home/you/Herd/shop";
    await api.editorMain(shop);
    await api.editorFetch(shop);
    await api.editorUpdate(shop);
    expect(invoked).toEqual([["editor_main", { folder: shop }], ["editor_fetch", { folder: shop }], ["editor_update", { folder: shop }]]);

    const got: EditorUpdated[] = [];
    await api.onEditorUpdated((e) => got.push(e));
    expect(listening.map(([event]) => event)).toEqual(["editor-updated"]);
    const ended: EditorUpdated = { folder: shop, ok: true, said: "Moved main 3 commits to 319bf0b and ran npm ci.", output: "added 12 packages in 1s" };
    listening[0][1]({ payload: ended });
    expect(got).toEqual([ended]);
  });

  it("types the answers as C3 says", () => {
    // Type-checked by npm run build (tsc -b also checks the tests).
    const stands: FolderMain = { mainLabel: "origin/main", fetchedAt: null, fetchError: null, branch: "main", onDefault: true, behind: 3,
      ahead: 0, uncommitted: 0, canUpdate: true, updateSays: "move main 3 commits to 319bf0b and run npm ci", updating: false };
    const quiet: EditorUpdated = { folder: "/home/you/Herd/shop", ok: true, said: "Moved main 1 commit to 319bf0b.", output: "" };
    expect([stands, quiet].length).toBe(2);
  });
});
