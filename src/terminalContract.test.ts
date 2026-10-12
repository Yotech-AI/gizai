// GA-133: the terminals' contract CT with the terminal's screens (TF0, TF1, TF2), word for word: src/api.ts and
// src/types.ts hold the card's lines as written (with doc comments between them), each call invokes its command with
// the arguments Tauri expects, terminalAttach hands the Channel's bytes on as a Uint8Array, onTerminalsChanged hands on
// terminals-changed's payload, and src-tauri/src/terminal.rs has the card's eight commands, each handed to Tauri in lib.rs.
import { describe, expect, it, vi } from "vitest";
import apiSource from "./api.ts?raw";
import typesSource from "./types.ts?raw";
import type { TerminalEnd, TerminalSession } from "./types";

const invoked: [string, unknown][] = [];
const listening: [string, (m: { payload: unknown }) => void][] = [];
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); },
  Channel: class<T> { constructor(public onmessage: (m: T) => void) {} },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, cb: (m: { payload: unknown }) => void) => { listening.push([event, cb]); return Promise.resolve(() => {}); },
}));

const api = await import("./api");

// The Rust side is read from disk. node:fs comes in through a computed specifier: the build type-checks tests without
// Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const rust = (path: string): string => fs.readFileSync(new URL(`../src-tauri/src/${path}`, import.meta.url), "utf8");

/** Contract CT's `src/api.ts`, as the card has it. */
const CT_API = `
export const terminalList = () => invoke<T.TerminalSession[]>("terminal_list");
export const terminalOpen = (folder: string, cols: number, rows: number, name: string | null) =>
  invoke<T.TerminalSession>("terminal_open", { folder, cols, rows, name });
export function terminalAttach(id: string, onBytes: (bytes: Uint8Array) => void): Promise<void> {
  const output = new Channel<ArrayBuffer>((b) => onBytes(new Uint8Array(b)));
  return invoke<void>("terminal_attach", { id, output });
}
export const terminalInput = (id: string, data: string, binary = false) => invoke<void>("terminal_input", { id, data, binary });
export const terminalResize = (id: string, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows });
export const terminalAck = (id: string, bytes: number) => invoke<void>("terminal_ack", { id, bytes });
export const terminalRename = (id: string, name: string) => invoke<T.TerminalSession>("terminal_rename", { id, name });
export const terminalClose = (id: string) => invoke<void>("terminal_close", { id });
export const onTerminalsChanged = (cb: (list: T.TerminalSession[]) => void): Promise<UnlistenFn> =>
  listen<T.TerminalSession[]>("terminals-changed", (m) => cb(m.payload));
`;

/** Contract CT's `src/types.ts`, as the card has it. */
const CT_TYPES = `
export type TerminalSession = {
  id: string;
  name: string;
  folder: string;          // the Editor folder's id (EditorFolder.id)
  folderPath: string;      // as shown, ~ for the home folder
  taskId: string | null;   // the card, for a card's worktree
  shell: string;           // "zsh", "bash", "pwsh"…
  busy: string | null;     // the program in the foreground, when it isn't the shell
  ended: TerminalEnd | null;
  cols: number;
  rows: number;
  openedAt: number;
};
export type TerminalEnd = { code: number | null; at: number; why: string | null };
`;

/** Contract CT's Rust commands, as the card has them. */
const CT_RUST = `
#[tauri::command] pub fn terminal_list(st: State<'_, AppState>) -> Vec<TerminalSession>
#[tauri::command] pub async fn terminal_open(st: State<'_, AppState>, folder: String, cols: u16, rows: u16, name: Option<String>) -> Result<TerminalSession, String>
#[tauri::command] pub fn terminal_attach(st: State<'_, AppState>, id: String, output: tauri::ipc::Channel<tauri::ipc::Response>) -> Result<(), String>
#[tauri::command] pub fn terminal_input(st: State<'_, AppState>, id: String, data: String, binary: bool) -> Result<(), String>
#[tauri::command] pub fn terminal_resize(st: State<'_, AppState>, id: String, cols: u16, rows: u16) -> Result<(), String>
#[tauri::command] pub fn terminal_ack(st: State<'_, AppState>, id: String, bytes: u32) -> Result<(), String>
#[tauri::command] pub fn terminal_rename(st: State<'_, AppState>, id: String, name: String) -> Result<TerminalSession, String>
#[tauri::command] pub async fn terminal_close(st: State<'_, AppState>, id: String) -> Result<(), String>
`;

/** The lines of `text` that aren't empty or a doc comment's. */
const codeLines = (text: string): string[] => text.split("\n").filter((l) => l.trim() !== "" && !/^\s*(\/\*\*|\*)/.test(l));

/** Whether `source` holds `lines` in this order, each one whole, with only doc comments between them. */
function holdsInOrder(source: string, lines: string[]): boolean {
  const code = codeLines(source);
  const at = code.indexOf(lines[0]);
  return at >= 0 && lines.every((l, i) => code[at + i] === l);
}

describe("contract CT in src/api.ts and src/types.ts (GA-133)", () => {
  it("api.ts and types.ts hold CT word for word", () => {
    expect(holdsInOrder(apiSource, codeLines(CT_API))).toBe(true);
    expect(holdsInOrder(typesSource, codeLines(CT_TYPES))).toBe(true);
  });

  it("each call invokes its command with the arguments Tauri expects", async () => {
    invoked.length = 0;
    await api.terminalList();
    await api.terminalOpen("/home/you/Herd/gizai", 120, 32, null);
    await api.terminalOpen("/home/you/Herd/gizai", 80, 24, "build");
    await api.terminalInput("t1", "ls\r");
    await api.terminalInput("t1", "ÿ", true);
    await api.terminalResize("t1", 100, 30);
    await api.terminalAck("t1", 100000);
    await api.terminalRename("t1", "server");
    await api.terminalClose("t1");
    expect(invoked).toEqual([
      ["terminal_list", undefined],
      ["terminal_open", { folder: "/home/you/Herd/gizai", cols: 120, rows: 32, name: null }],
      ["terminal_open", { folder: "/home/you/Herd/gizai", cols: 80, rows: 24, name: "build" }],
      ["terminal_input", { id: "t1", data: "ls\r", binary: false }],
      ["terminal_input", { id: "t1", data: "ÿ", binary: true }],
      ["terminal_resize", { id: "t1", cols: 100, rows: 30 }],
      ["terminal_ack", { id: "t1", bytes: 100000 }],
      ["terminal_rename", { id: "t1", name: "server" }],
      ["terminal_close", { id: "t1" }],
    ]);
  });

  it("terminalAttach sends a Channel and hands each message on as bytes", async () => {
    invoked.length = 0;
    const got: Uint8Array[] = [];
    await api.terminalAttach("t1", (b) => got.push(b));
    expect(invoked.map(([cmd]) => cmd)).toEqual(["terminal_attach"]);
    const args = invoked[0][1] as { id: string; output: { onmessage: (m: ArrayBuffer) => void } };
    expect(args.id).toBe("t1");
    args.output.onmessage(new Uint8Array([104, 105, 0xff, 0x1b]).buffer);
    expect(got.map((b) => [...b])).toEqual([[104, 105, 0xff, 0x1b]]);
    expect(got[0]).toBeInstanceOf(Uint8Array);
  });

  it("onTerminalsChanged hands on terminals-changed's whole list", async () => {
    listening.length = 0;
    const got: TerminalSession[][] = [];
    await api.onTerminalsChanged((list) => got.push(list));
    expect(listening.map(([event]) => event)).toEqual(["terminals-changed"]);
    const ended: TerminalEnd = { code: 3, at: 1760000001000, why: "zsh ended with exit code 3" };
    const list: TerminalSession[] = [
      { id: "t1", name: "gizai", folder: "/home/you/Herd/gizai", folderPath: "~/Herd/gizai", taskId: null, shell: "zsh", busy: "npm", ended: null,
        cols: 120, rows: 32, openedAt: 1760000000000 },
      { id: "t2", name: "GA-123", folder: "/w/GA-123", folderPath: "~/.local/share/gizai/worktrees/GA/GA-123", taskId: "c1", shell: "zsh",
        busy: null, ended, cols: 80, rows: 24, openedAt: 1760000000500 },
    ];
    listening[0][1]({ payload: list });
    expect(got).toEqual([list]);
  });
});

describe("contract CT in src-tauri (GA-133)", () => {
  const terminal = rust("terminal.rs");
  const lib = rust("lib.rs");

  it("has the card's eight commands word for word", () => {
    // the attribute on its own line counts as on the same line
    const oneLine = terminal.replace(/#\[tauri::command\]\s*\n\s*/g, "#[tauri::command] ");
    for (const line of codeLines(CT_RUST)) expect(oneLine, line).toContain(line);
    expect([...terminal.matchAll(/#\[tauri::command\]/g)].length).toBe(8);
  });

  it("hands every command to Tauri", () => {
    const handler = lib.slice(lib.indexOf("generate_handler!["));
    for (const [, name] of CT_RUST.matchAll(/pub (?:async )?fn (\w+)\(/g)) expect(handler, name).toContain(`terminal::${name}`);
  });

  it("emits terminals-changed with the list", () => {
    expect(lib).toContain(`runs::Note::TerminalsChanged(list) => app.emit("terminals-changed", list)`);
  });
});
