// GA-107: the Editor's contract C2 with the Editor page (F2), word for word: src/api.ts and src/types.ts hold the card's
// lines as written, each call invokes its command with the arguments Tauri expects, and src-tauri has one command per
// call, by the same name in snake case, with its arguments in snake case, handed to Tauri in lib.rs.
import { describe, expect, it, vi } from "vitest";
import apiSource from "./api.ts?raw";
import typesSource from "./types.ts?raw";
import type { DirEntry, EditorApp, EditorFolder, FolderGit, OpenedFile, SearchResult, Settings } from "./types";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

// The Rust side is read from disk. node:fs comes in through a computed specifier: the build type-checks tests without
// Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const rust = (path: string): string => fs.readFileSync(new URL(`../src-tauri/src/${path}`, import.meta.url), "utf8");

/** Contract C2's `src/api.ts`, as the card has it. */
const C2_API = `
/** The folders the Editor may open: projects' linked folders, the folders under Settings → Editor folders, card worktrees. */
export const editorFolders = () => invoke<T.EditorFolder[]>("editor_folders");
/** One folder level, folders first; \`dir\` "" for the top. */
export const editorList = (folder: string, dir: string) => invoke<T.DirEntry[]>("editor_list", { folder, dir });
export const editorRead = (folder: string, path: string) => invoke<T.OpenedFile>("editor_read", { folder, path });
/** Saves; refuses with "changed on disk" when the file's version isn't \`version\` any more. Resolves with the new version. */
export const editorWrite = (folder: string, path: string, text: string, version: string) =>
  invoke<string>("editor_write", { folder, path, text, version });
export const editorCreate = (folder: string, path: string, dir: boolean) => invoke<void>("editor_create", { folder, path, dir });
export const editorRename = (folder: string, from: string, to: string) => invoke<void>("editor_rename", { folder, from, to });
export const editorDelete = (folder: string, path: string) => invoke<void>("editor_delete", { folder, path });
/** Go to file: tracked and untracked files git doesn't ignore. */
export const editorFiles = (folder: string) => invoke<string[]>("editor_files", { folder });
export const editorSearch = (folder: string, query: string, caseSensitive: boolean, wholeWord: boolean, regex: boolean) =>
  invoke<T.SearchResult>("editor_search", { folder, query, caseSensitive, wholeWord, regex });
/** The branch and each changed file's git status (local, no fetch). */
export const editorGit = (folder: string) => invoke<T.FolderGit>("editor_git", { folder });
/** The current version of each path ("" for a file that is gone), to notice outside changes. */
export const editorVersions = (folder: string, paths: string[]) => invoke<string[]>("editor_versions", { folder, paths });
/** VS Code, Zed and PhpStorm, those found on this computer (under any of their names, such as zeditor). */
export const editorApps = () => invoke<T.EditorApp[]>("editor_apps");
/** Opens the folder, or a file in it (at a line when given), in that editor. */
export const openInApp = (app: T.EditorApp["id"], folder: string, path: string | null, line: number | null) =>
  invoke<void>("open_in_app", { app, folder, path, line });
`;

/** Contract C2's `src/types.ts`, as the card has it (the Settings field is in Settings' own type). */
const C2_TYPES = `
export type EditorFolder = {
  id: string;                 // the folder's canonical path
  path: string;               // as shown, ~ for the home folder
  name: string;               // its own name, or the card's identifier
  kind: "project" | "root" | "card";
  projectId: string | null;
  taskId: string | null;
  readOnly: boolean;          // a card an agent is working on now
  readOnlyWhy: string | null;
};
export type GitMark = "modified" | "added" | "deleted" | "renamed" | "untracked" | "conflicted";
export type DirEntry = {
  name: string;
  path: string;
  dir: boolean;
  size: number;
  ignored: boolean;           // git ignores it: greyed; read-only inside an ignored folder
  git: GitMark | null;        // for a folder: whether something inside it changed ("modified")
};
export type OpenedFile = {
  path: string;
  text: string | null;        // null when binary or too large
  binary: boolean;
  tooLarge: boolean;          // over 5 MB
  readOnly: boolean;
  readOnlyWhy: string | null; // "Over 1 MB", "Ignored by git", "Backend Agent is working on GA-12"
  crlf: boolean;
  bom: boolean;
  version: string;            // what editorWrite checks
};
export type SearchHit = { path: string; line: number; column: number; text: string };
export type SearchResult = { hits: SearchHit[]; truncated: boolean };
export type FolderGit = { branch: string | null; files: Record<string, GitMark>; uncommitted: number };
export type EditorApp = { id: "vscode" | "zed" | "phpstorm"; name: string };
`;

/** The commands of C2 and their arguments in snake case, in the order of src/api.ts. */
const COMMANDS: [string, string[]][] = [
  ["editor_folders", []],
  ["editor_list", ["folder", "dir"]],
  ["editor_read", ["folder", "path"]],
  ["editor_write", ["folder", "path", "text", "version"]],
  ["editor_create", ["folder", "path", "dir"]],
  ["editor_rename", ["folder", "from", "to"]],
  ["editor_delete", ["folder", "path"]],
  ["editor_files", ["folder"]],
  ["editor_search", ["folder", "query", "case_sensitive", "whole_word", "regex"]],
  ["editor_git", ["folder"]],
  ["editor_versions", ["folder", "paths"]],
  ["editor_apps", []],
  ["open_in_app", ["app", "folder", "path", "line"]],
];

/** GA-111: contract C4's commands (a person's card) follow C2's in editor.rs; handContract.test.ts checks them. */
const C4_COMMANDS: [string, string[]][] = [
  ["start_by_hand", ["task_id"]],
  ["hand_card", ["task_id"]],
  ["hand_commit", ["task_id", "message"]],
  ["hand_push", ["task_id"]],
  ["hand_over", ["task_id"]],
];

describe("contract C2 in src/api.ts and src/types.ts (GA-107)", () => {
  it("api.ts holds C2's calls word for word, in C2's order", () => {
    const lines = C2_API.trim().split("\n");
    let from = 0;
    for (const line of lines) {
      const at = apiSource.indexOf(line, from);
      expect(at, `api.ts lacks, or has out of order: ${line}`).toBeGreaterThanOrEqual(0);
      from = at + line.length;
    }
    expect(apiSource).toContain(C2_API.trim());
  });

  it("types.ts holds C2's types word for word, and Settings has editorRoots", () => {
    expect(typesSource).toContain(C2_TYPES.trim());
    const settings = typesSource.slice(typesSource.indexOf("export type Settings = {"));
    expect(settings.slice(0, settings.indexOf("};"))).toContain(`  editorRoots: string[];   // Settings → General → Editor folders; default ["~/Herd"]`);
  });

  it("each call invokes its command with the arguments as C2 names them", async () => {
    invoked.length = 0;
    await api.editorFolders();
    await api.editorList("/home/you/Herd/shop", "");
    await api.editorRead("/home/you/Herd/shop", "src/main.rs");
    await api.editorWrite("/home/you/Herd/shop", "src/main.rs", "fn main() {}\n", "v1");
    await api.editorCreate("/home/you/Herd/shop", "src/new", true);
    await api.editorRename("/home/you/Herd/shop", "a.txt", "b.txt");
    await api.editorDelete("/home/you/Herd/shop", "b.txt");
    await api.editorFiles("/home/you/Herd/shop");
    await api.editorSearch("/home/you/Herd/shop", "needle", true, false, true);
    await api.editorGit("/home/you/Herd/shop");
    await api.editorVersions("/home/you/Herd/shop", ["a.txt", "gone.txt"]);
    await api.editorApps();
    await api.openInApp("phpstorm", "/home/you/Herd/shop", "src/main.rs", 12);
    await api.openInApp("zed", "/home/you/Herd/shop", null, null);
    const shop = "/home/you/Herd/shop";
    expect(invoked).toEqual([
      ["editor_folders", undefined],
      ["editor_list", { folder: shop, dir: "" }],
      ["editor_read", { folder: shop, path: "src/main.rs" }],
      ["editor_write", { folder: shop, path: "src/main.rs", text: "fn main() {}\n", version: "v1" }],
      ["editor_create", { folder: shop, path: "src/new", dir: true }],
      ["editor_rename", { folder: shop, from: "a.txt", to: "b.txt" }],
      ["editor_delete", { folder: shop, path: "b.txt" }],
      ["editor_files", { folder: shop }],
      ["editor_search", { folder: shop, query: "needle", caseSensitive: true, wholeWord: false, regex: true }],
      ["editor_git", { folder: shop }],
      ["editor_versions", { folder: shop, paths: ["a.txt", "gone.txt"] }],
      ["editor_apps", undefined],
      ["open_in_app", { app: "phpstorm", folder: shop, path: "src/main.rs", line: 12 }],
      ["open_in_app", { app: "zed", folder: shop, path: null, line: null }],
    ]);
  });

  it("types the answers as C2 says", () => {
    // Type-checked by npm run build (tsc -b also checks the tests).
    const folder: EditorFolder = { id: "/home/you/.local/share/gizai/worktrees/GA/GA-12", path: "~/.local/share/gizai/worktrees/GA/GA-12",
      name: "GA-12", kind: "card", projectId: "p1", taskId: "t1", readOnly: true, readOnlyWhy: "Backend Agent is working on GA-12" };
    const entry: DirEntry = { name: "src", path: "src", dir: true, size: 0, ignored: false, git: "modified" };
    const file: OpenedFile = { path: "big.bin", text: null, binary: true, tooLarge: false, readOnly: true, readOnlyWhy: "A binary file",
      crlf: false, bom: false, version: "v" };
    const found: SearchResult = { hits: [{ path: "a.txt", line: 1, column: 3, text: "a needle" }], truncated: false };
    const git: FolderGit = { branch: "main", files: { "a.txt": "untracked" }, uncommitted: 1 };
    const apps: EditorApp[] = [{ id: "vscode", name: "VS Code" }, { id: "zed", name: "Zed" }, { id: "phpstorm", name: "PhpStorm" }];
    const roots: Settings["editorRoots"] = ["~/Herd"];
    expect([folder, entry, file, found, git, apps, roots].length).toBe(7);
  });
});

/** The commands of C3 (GA-110), which editor.rs has after C2's. */
const C3_COMMANDS: [string, string[]][] = [
  ["editor_main", ["folder"]],
  ["editor_fetch", ["folder"]],
  ["editor_update", ["folder"]],
];

describe("contracts C2 and C3 in src-tauri (GA-107, GA-110)", () => {
  const editor = rust("editor.rs");
  const lib = rust("lib.rs");

  it("has one #[tauri::command] per call, by its name in snake case and with its arguments in snake case", () => {
    const commands = [...editor.matchAll(/#\[tauri::command\]\s*pub async fn (\w+)\(([^)]*)\)/g)]
      // `st: State<'_, AppState>` has a comma of its own: an argument is what has a type after its name
      .map(([, name, params]) => [name, params.split(",").filter((p) => p.includes(":")).map((p) => p.split(":")[0].trim()).filter((p) => p !== "st")]);
    expect(commands).toEqual([...COMMANDS, ...C3_COMMANDS, ...C4_COMMANDS]);
  });

  it("hands every command to Tauri", () => {
    const handler = lib.slice(lib.indexOf("generate_handler!["));
    for (const [name] of [...COMMANDS, ...C3_COMMANDS]) expect(handler, name).toContain(`editor::${name}`);
  });
});
