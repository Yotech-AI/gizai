// GA-185 QA against contract CS (GA-182): the UI's calls for a card's screenshots and their types match the Rust side,
// read as text (Vite's ?raw), so a change on one side without the other fails here. Also the Settings fields of contract
// CP that Settings → Agents and runs → Screenshots edits, and the project's Preview fields.
import { describe, expect, it, vi } from "vitest";
import type { ProjectScreenshots, Screenshot, ScreenshotSet, Settings } from "./types";
import libRs from "../src-tauri/src/lib.rs?raw";
import shotsRs from "../src-tauri/src/screenshots.rs?raw";
import runsRs from "../src-tauri/src/runs.rs?raw";
import coreShotsRs from "../crates/gizai-core/src/screenshots.rs?raw";
import modelRs from "../crates/gizai-core/src/model.rs?raw";
import { LIMITS, type LimitKey } from "./lib/screenshotSettings";

const invoked: [string, unknown][] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: unknown) => { invoked.push([cmd, args]); return Promise.resolve(null); } }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

const api = await import("./api");

const camel = (s: string) => s.replace(/_(\w)/g, (_, c: string) => c.toUpperCase());

/** The fields of a Rust struct, camelCased as serde's rename_all = "camelCase" sends them. */
function serdeFields(rs: string, name: string): string[] {
  const body = new RegExp(`#\\[serde\\(rename_all = "camelCase"[^\\]]*\\)\\]\\s*pub struct ${name} \\{([\\s\\S]*?)\\n\\}`).exec(rs);
  if (!body) throw new Error(`no camelCase struct ${name}`);
  return [...body[1].matchAll(/^\s*pub (\w+):/gm)].map((m) => camel(m[1])).sort();
}

describe("contract CS's commands", () => {
  it("are invoked with camelCase arguments and registered with Tauri", async () => {
    invoked.length = 0;
    await api.listScreenshots("t1");
    await api.screenshotBytes("f1");
    expect(invoked).toEqual([["list_screenshots", { taskId: "t1" }], ["screenshot_bytes", { fileId: "f1" }]]);
    const handler = /generate_handler!\[([\s\S]*?)\]\)/.exec(libRs)?.[1] ?? "";
    for (const cmd of ["list_screenshots", "screenshot_bytes"]) expect(handler, cmd).toMatch(new RegExp(`screenshots::${cmd}\\b`));
  });
  it("take the arguments the UI sends (Tauri camelCases them), and answer the sets and the bytes", () => {
    const args = (cmd: string) => new RegExp(`pub async fn ${cmd}\\(([^)]*)\\)`).exec(shotsRs)?.[1]
      .replace(/^st: State<'_, AppState>,?/, "").split(",").map((a) => a.trim()).filter(Boolean);
    expect(args("list_screenshots")).toEqual(["task_id: String"]);
    expect(args("screenshot_bytes")).toEqual(["file_id: String"]);
    expect(shotsRs).toMatch(/pub async fn list_screenshots\([^)]*\) -> Result<Vec<ScreenshotSet>, String>/);
    // raw bytes (an ArrayBuffer in the UI), not a JSON array of numbers
    expect(shotsRs).toMatch(/pub async fn screenshot_bytes\([^)]*\) -> Result<tauri::ipc::Response, String>/);
  });
});

describe("contract CS's types are the JSON the backend sends", () => {
  it("ScreenshotSet and Screenshot have the Rust structs' fields, and nothing else", () => {
    // Required<> and the literals' excess-property checks (tsc -b in npm run build) make these keys the types' own.
    const shot: Required<Screenshot> = { fileId: "f1", name: "01-board-desktop-dark.png", caption: "The board", width: 1280, height: 800,
      sizeBytes: 1, sha256: "ab" };
    const set: Required<ScreenshotSet> = { runId: "r1", agentId: "a1", agentName: "Frontend Agent", role: "frontend", outcome: "ready_for_testing",
      status: "succeeded", endedAt: 1, commit: "9f8e7d6", shots: [shot], leftOut: [], preview: null };
    expect(Object.keys(shot).sort()).toEqual(serdeFields(coreShotsRs, "Screenshot"));
    expect(Object.keys(set).sort()).toEqual(serdeFields(coreShotsRs, "ScreenshotSet"));
  });
  it("a set's status is one of the four the backend gives an ended run", () => {
    const statuses: ScreenshotSet["status"][] = ["succeeded", "failed", "cancelled", "timed_out"];
    expect(coreShotsRs).toMatch(/r\.status IN \('succeeded', 'failed', 'cancelled', 'timed_out'\)/);
    expect(statuses).toHaveLength(4);
  });
});

describe("contract CP's fields that GA-185's forms edit", () => {
  it("Settings' screenshot limits are the Rust Settings' fields, and the forms' ranges and defaults are the backend's", () => {
    const keys = Object.keys(LIMITS) as LimitKey[];
    const rust = serdeFields(runsRs, "Settings");
    for (const k of keys) expect(rust, k).toContain(k);
    const s: Pick<Settings, LimitKey> = { screenshotsPerRun: 12, screenshotMaxMb: 10, screenshotsKeepSets: 3, screenshotsKeepDays: 30,
      previewPortFrom: 47100, previewPortTo: 47999 };
    expect(Object.keys(s).sort()).toEqual([...keys].sort());
    // each Setting in crates/gizai-core/src/screenshots.rs: its key (snake case), default and range
    const settings = [...coreShotsRs.matchAll(/Setting \{\s*key: "(\w+)", default: (\d+), range: (\d+)\.\.=(\d+), name: "([^"]+)", unit: "([^"]*)"/g)]
      .map((m) => ({ key: camel(m[1]), default: +m[2], low: +m[3], high: +m[4], name: m[5].replace(/'/g, "’"), unit: m[6] }));
    expect(settings.map((x) => x.key).sort()).toEqual([...keys].sort());
    for (const x of settings) {
      const l = LIMITS[x.key as LimitKey];
      expect({ key: x.key, default: l.default, low: l.low, high: l.high, name: l.name, unit: l.unit }).toEqual(x);
    }
  });
  it("a project's Preview fields: screenshots (on, sizes, both themes, whole page), the Preview command and Start time", () => {
    const shots: Required<ProjectScreenshots> = { on: true, sizes: [{ name: "Desktop", width: 1280, height: 800 }], bothThemes: false, fullPage: false };
    expect(Object.keys(shots).sort()).toEqual(serdeFields(coreShotsRs, "ProjectScreenshots"));
    expect(Object.keys(shots.sizes[0]).sort()).toEqual(serdeFields(coreShotsRs, "ShotSize"));
    const project = serdeFields(modelRs, "Project");
    for (const k of ["screenshots", "previewCommand", "previewStartSecs"]) expect(project, k).toContain(k);
    const input = serdeFields(modelRs, "ProjectInput");
    for (const k of ["screenshots", "previewCommand", "previewStartSecs"]) expect(input, k).toContain(k);
  });
});
