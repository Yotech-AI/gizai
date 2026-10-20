// Tests for scripts/perf-report.mjs (GA-188), the benchmark harness's report, and scripts/perf.sh --compare, which hands
// two results to it. Each test makes the runs perf.sh leaves (<scenario>-<n> folders with the self-test's report.json,
// launch_ms and the /proc samples) in a scratch folder: no app runs.
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

const REPORT = fileURLToPath(new URL("./perf-report.mjs", import.meta.url));
const PERF_SH = fileURLToPath(new URL("./perf.sh", import.meta.url));
const scratch = mkdtempSync(join(tmpdir(), "perf-report-"));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));

const LAUNCH = 1_760_000_000_000;
// perf.sh is a bash script (it runs on Linux, in a cage); its --compare and its arguments are checked where bash is
const unix = process.platform !== "win32";

/** One run's folder as perf.sh leaves it: the page began 300 ms after the launch; the app at 100 MB using a whole core
 *  (100 ticks a second at CLK_TCK 100), the web process at 200 MB using half of one, sampled at 0, 0.5 and 1 s. */
function run(dir, name, { scenario, numbers, report = true }) {
  const d = join(dir, name);
  mkdirSync(d, { recursive: true });
  writeFileSync(join(d, "launch_ms"), `${LAUNCH}\n`);
  const samples = [0, 500, 1000].flatMap((ms, i) => [
    `${LAUNCH + 400 + ms} 10 gizai 102400 ${i * 50}`,
    `${LAUNCH + 400 + ms} 11 WebKitWebProces 204800 ${i * 25}`,
  ]);
  writeFileSync(join(d, "samples.txt"), samples.join("\n") + "\n");
  if (report) {
    writeFileSync(join(d, "report.json"), JSON.stringify({
      ready: true, errors: [], version: "0.12.0",
      perf: { ok: true, scenario, timeOrigin: LAUNCH + 300, numbers, windows: {}, detail: { note: name } },
    }));
  } else {
    writeFileSync(join(d, "exit_code"), "1\n");
  }
}

function node(...args) {
  return spawnSync(process.execPath, [REPORT, ...args], { encoding: "utf8" });
}

describe("--collect", () => {
  const runs = join(scratch, "runs");
  const out = join(scratch, "a.json");
  // board: run 1 is cold, the medians are runs 2 and 3's; chat: run 2 left no report
  run(runs, "board-1", { scenario: "board", numbers: { columns_drawn_ms: 900, scroll_fps: 20 } });
  run(runs, "board-2", { scenario: "board", numbers: { columns_drawn_ms: 100, scroll_fps: 30 } });
  run(runs, "board-3", { scenario: "board", numbers: { columns_drawn_ms: 300, scroll_fps: 30 } });
  run(runs, "chat-1", { scenario: "chat", numbers: { first_screen_ms: 105 } });
  run(runs, "chat-2", { scenario: "chat", report: false });
  writeFileSync(join(runs, "not-a-run.txt"), "");
  const res = node("--collect", runs, out, "--commit", "abc1234", "--runs", "3", "--clk-tck", "100");
  const result = JSON.parse(readFileSync(out, "utf8"));

  it("takes the median of the warm runs, and keeps the cold run's numbers apart", () => {
    const board = result.scenarios.board;
    expect(board.runs.map((r) => r.n)).toEqual([1, 2, 3]);
    expect(board.median.columns_drawn_ms).toBe(200);
    expect(board.median.scroll_fps).toBe(30);
    expect(board.cold.columns_drawn_ms).toBe(900);
    expect([board.min.columns_drawn_ms, board.max.columns_drawn_ms]).toEqual([100, 300]);
  });

  it("counts from the launch: to the page, and to the board drawn", () => {
    const m = result.scenarios.board.median;
    expect(m.launch_to_page_ms).toBe(300);
    expect(m.launch_to_board_ms).toBe(500); // 300 + the warm runs' 100 and 300
  });

  it("reads memory and CPU per process group from the samples", () => {
    const m = result.scenarios.board.median;
    expect([m.app_rss_mb, m.app_rss_max_mb, m.web_rss_mb]).toEqual([100, 100, 200]);
    expect([m.app_cpu_pct, m.web_cpu_pct]).toEqual([100, 50]);
  });

  it("writes the commit and the runs, names a run without a report, and exits 1 for it", () => {
    expect([result.commit, result.runs]).toEqual(["abc1234", 3]);
    expect(result.scenarios.chat.errors).toEqual([expect.stringMatching(/^run 2: no report \(exit 1; see .*chat-2/)]);
    expect(result.scenarios.board.errors).toEqual([]);
    expect(res.status).toBe(1);
    expect(res.stdout).toMatch(/^board\s+500\s/m);
    expect(res.stdout).toContain(`written: ${out}`);
  });
});

describe("--compare", () => {
  const a = join(scratch, "cmp-a.json");
  const b = join(scratch, "cmp-b.json");
  const result = (commit, board, bundleStart) => ({
    commit, date: "2026-10-16T18:00:00.000Z", runs: 5, scenarios: { board: { median: board } },
    bundle: { totals: { start: { bytes: bundleStart, gzip: 100 }, later: { bytes: 10, gzip: 5 }, all: { bytes: bundleStart + 10, gzip: 105 } } },
  });
  writeFileSync(a, JSON.stringify(result("main1", { launch_to_board_ms: 500, scroll_fps: 30, only_a: 7 }, 1000)));
  writeFileSync(b, JSON.stringify(result("card2", { launch_to_board_ms: 400, scroll_fps: 60, only_b: 3 }, 1100)));

  const expectBoth = (stdout) => {
    expect(stdout).toContain("a: main1");
    expect(stdout).toContain("b: card2");
    expect(stdout).toMatch(/launch_to_board_ms\s+500\s+400\s+-20%/);
    expect(stdout).toMatch(/scroll_fps\s+30\s+60\s+\+100%/);
    expect(stdout).toMatch(/only_a\s+7\s+–\s*$/m);
    expect(stdout).toMatch(/only_b\s+–\s+3\s*$/m);
    expect(stdout).toMatch(/start bytes\s+1000\s+1100\s+\+10%/);
  };

  it("prints both results side by side with the change in %", () => {
    const res = node("--compare", a, b);
    expect(res.status).toBe(0);
    expectBoth(res.stdout);
  });

  it.runIf(unix)("is what scripts/perf.sh --compare runs", () => {
    const res = spawnSync("bash", [PERF_SH, "--compare", a, b], { encoding: "utf8" });
    expect(res.status).toBe(0);
    expectBoth(res.stdout);
    const usage = spawnSync("bash", [PERF_SH, "--compare", a], { encoding: "utf8" });
    expect(usage.status).toBe(2);
    expect(usage.stdout).toContain("usage: scripts/perf.sh --compare a.json b.json");
  });
});

describe.runIf(unix)("scripts/perf.sh", () => {
  it("refuses a scenario it doesn't know before it builds anything, and names the eight", () => {
    const res = spawnSync("bash", [PERF_SH, "board", "nope"], { encoding: "utf8" });
    expect(res.status).toBe(2);
    expect(res.stdout).toContain("unknown scenario: nope (the scenarios: start board chat memory runlog idle editor lead)");
  });
});
