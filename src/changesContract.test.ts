// GA-106 against contract C1: the UI's calls for a card's changes and their types match the Rust side, read as text
// (Vite's ?raw), so a change on one side without the other fails here before F1 builds on it.
import { describe, expect, it, vi } from "vitest";
import type { CardChanges, ChangeSource, DiffHunk, FileChange, FileDiff, FileStatus } from "./types";
import libRs from "../src-tauri/src/lib.rs?raw";
import changesRs from "../src-tauri/src/changes.rs?raw";
import agentsChangesRs from "../crates/gizai-agents/src/changes.rs?raw";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

/** The fields of a Rust struct, camelCased as serde's rename_all = "camelCase" sends them. */
function serdeFields(rs: string, name: string): string[] {
  const body = new RegExp(`#\\[serde\\(rename_all = "camelCase"\\)\\]\\s*pub struct ${name} \\{([\\s\\S]*?)\\n\\}`).exec(rs);
  if (!body) throw new Error(`no camelCase struct ${name}`);
  return [...body[1].matchAll(/^\s*pub (\w+):/gm)].map((m) => m[1].replace(/_(\w)/g, (_, c: string) => c.toUpperCase())).sort();
}

/** The variants of a Rust enum, lower-cased as serde's rename_all = "lowercase" sends them. */
function serdeVariants(rs: string, name: string): string[] {
  const body = new RegExp(`#\\[serde\\(rename_all = "lowercase"\\)\\]\\s*pub enum ${name} \\{([\\s\\S]*?)\\n\\}`).exec(rs);
  if (!body) throw new Error(`no lowercase enum ${name}`);
  return [...body[1].matchAll(/^\s*(\w+),/gm)].map((m) => m[1].toLowerCase());
}

describe("contract C1's commands", () => {
  it("are invoked with camelCase arguments and registered with Tauri", async () => {
    invoked.length = 0;
    await api.cardChanges("t1");
    await api.cardFileDiff("t1", "src/b.ts", "src/a.ts", "abc123", null);
    await api.cardFileDiff("t1", "src/b.ts", null, "", "def456");
    await api.runChanges("r1");
    expect(invoked).toEqual([
      ["card_changes", { taskId: "t1" }],
      ["card_file_diff", { taskId: "t1", path: "src/b.ts", oldPath: "src/a.ts", from: "abc123", to: null }],
      ["card_file_diff", { taskId: "t1", path: "src/b.ts", oldPath: null, from: "", to: "def456" }],
      ["run_changes", { runId: "r1" }],
    ]);
    const handler = /generate_handler!\[([\s\S]*?)\]\)/.exec(libRs)?.[1] ?? "";
    for (const cmd of ["card_changes", "card_file_diff", "run_changes"]) expect(handler, cmd).toMatch(new RegExp(`changes::${cmd}\\b`));
  });
  it("take the arguments the UI sends (Tauri camelCases them)", () => {
    const args = (cmd: string) => new RegExp(`pub async fn ${cmd}\\(([^)]*)\\)`).exec(changesRs)?.[1]
      .replace(/^st: State<'_, AppState>,?/, "").split(",").map((a) => a.trim()).filter(Boolean);
    expect(args("card_changes")).toEqual(["task_id: String"]);
    expect(args("card_file_diff")).toEqual(["task_id: String", "path: String", "old_path: Option<String>", "from: String", "to: Option<String>"]);
    expect(args("run_changes")).toEqual(["run_id: String"]);
    expect(changesRs).toMatch(/pub async fn card_changes\([^)]*\) -> Result<CardChanges, String>/);
    expect(changesRs).toMatch(/pub async fn card_file_diff\([^)]*\) -> Result<FileDiff, String>/);
    expect(changesRs).toMatch(/pub async fn run_changes\([^)]*\) -> Result<CardChanges, String>/);
  });
});

describe("contract C1's types are the JSON the backend sends", () => {
  it("each type has the Rust struct's fields, and nothing else", () => {
    // Required<> and the literals' excess-property checks (tsc -b in npm run build) make these keys the types' own.
    const file: Required<FileChange> = { path: "b.ts", oldPath: "a.ts", status: "renamed", additions: 1, deletions: null, binary: false, uncommitted: true };
    const changes: Required<CardChanges> = { source: "worktree", from: "abc", to: null, baseLabel: "origin/main", additions: 1, deletions: 0,
      files: [file], note: null };
    const hunk: Required<DiffHunk> = { oldStart: 0, oldLines: 0, newStart: 1, newLines: 3 };
    const diff: Required<FileDiff> = { path: "b.ts", oldText: null, newText: "x\n", hunks: [hunk], patch: null, binary: false, tooLarge: false,
      oldSize: 0, newSize: 2, crlf: false };
    expect(Object.keys(file).sort()).toEqual(serdeFields(agentsChangesRs, "FileChange"));
    expect(Object.keys(changes).sort()).toEqual(serdeFields(agentsChangesRs, "CardChanges"));
    expect(Object.keys(hunk).sort()).toEqual(serdeFields(agentsChangesRs, "DiffHunk"));
    expect(Object.keys(diff).sort()).toEqual(serdeFields(agentsChangesRs, "FileDiff"));
  });
  it("the sources and statuses are the Rust enums' variants, in lower case", () => {
    const sources: ChangeSource[] = ["worktree", "branch", "merge", "pull", "run", "none"];
    const statuses: FileStatus[] = ["added", "modified", "deleted", "renamed", "copied", "typechange"];
    expect(serdeVariants(agentsChangesRs, "ChangeSource")).toEqual(sources);
    expect(serdeVariants(agentsChangesRs, "FileStatus")).toEqual(statuses);
  });
});
