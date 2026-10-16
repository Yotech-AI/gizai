// The benchmark harness's report (GA-188; scripts/perf.sh writes the runs, this reads them).
//   --collect <runs dir> <out.json> [--commit c] [--runs n] [--clk-tck t] [--webkit-env e] [--bundle bundle.json]
//       joins each run's self-test report (its perf probe's numbers), its launch time and its /proc samples, takes the
//       median of the warm runs (every run but a scenario's first, which is cold), prints a table and writes out.json.
//   --compare <a.json> <b.json>
//       prints two results side by side with the change in %.
// A run's folder is <scenario>-<n> with report.json (the self-test JSON, its perf probe's result as `perf`), launch_ms
// (ms since 1970) and samples.txt: one line per process every 500 ms, "<ms> <pid> <name> <VmRSS KiB> <utime+stime ticks>".
import { readFileSync, readdirSync, writeFileSync, existsSync } from "node:fs";
import { cpus, release } from "node:os";
import { join } from "node:path";

const SCENARIOS = ["start", "board", "chat", "memory", "runlog", "idle", "editor", "lead"];
/** The processes by name (/proc/<pid>/comm, at most 15 characters): the app, WebKit's web and network processes, the rest
 *  (agent runs, their shells and tools). */
const GROUPS = { app: (n) => n === "gizai", web: (n) => n.startsWith("WebKitWebProces"), net: (n) => n.startsWith("WebKitNetwork") };
const groupOf = (name) => Object.keys(GROUPS).find((g) => GROUPS[g](name)) ?? "other";
/** Each scenario's headline numbers, for the summary table: its main time, frames and calls. */
const HEAD = {
  start: { time: "launch_to_card_ms", calls: "first_5s_calls", kb: "first_5s_kb" },
  board: { time: "launch_to_board_ms", frame: "scroll_longest_frame_ms", fps: "scroll_fps", calls: "drag_calls", kb: "drag_kb", window: "scroll" },
  chat: { time: "first_screen_ms", frame: "scroll_longest_frame_ms", fps: "scroll_fps", calls: "run_calls", kb: "run_kb" },
  memory: { time: "tree_ms", frame: "graph_longest_frame_ms", fps: "graph_fps", calls: "run_calls", kb: "run_kb" },
  runlog: { time: "log_drawn_ms", frame: "live_longest_frame_ms", fps: "live_fps", calls: "live_calls", kb: "live_kb", window: "live" },
  idle: { calls: "idle_60s_calls", kb: "idle_60s_kb", window: "idle_60s" },
  editor: { time: "tree_ms", calls: "open_60s_calls", kb: "open_60s_kb", window: "open_60s" },
  lead: { frame: "stream_longest_frame_ms", fps: "stream_fps", calls: "stream_calls", kb: "stream_kb", window: "stream" },
};

const args = process.argv.slice(2);
const opt = (name, fallback = "") => { const i = args.indexOf(name); return i >= 0 && i + 1 < args.length ? args[i + 1] : fallback; };
const read = (file) => { try { return readFileSync(file, "utf8"); } catch { return null; } };
const round = (n, d = 0) => (n === null || n === undefined || Number.isNaN(n) ? null : Math.round(n * 10 ** d) / 10 ** d);
const median = (xs) => {
  const s = xs.filter((x) => typeof x === "number" && x >= 0).sort((a, b) => a - b);
  if (!s.length) return null;
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

/** Memory and CPU from a run's samples: per group the median and largest RSS (MB) over the run, and CPU % over the run and
 *  over each of the probe's windows. */
function processNumbers(samples, clkTck, windows) {
  const rows = samples.trim().split("\n").filter(Boolean).map((l) => {
    const [ms, pid, name, rss, ticks] = l.split(" ");
    return { ms: +ms, pid: +pid, group: groupOf(name ?? ""), rss: +rss, ticks: +ticks };
  }).filter((r) => Number.isFinite(r.ms) && Number.isFinite(r.ticks));
  const out = {};
  if (!rows.length) return out;
  const times = [...new Set(rows.map((r) => r.ms))].sort((a, b) => a - b);
  for (const g of [...Object.keys(GROUPS), "other"]) {
    const sums = times.map((t) => rows.filter((r) => r.ms === t && r.group === g).reduce((n, r) => n + r.rss, 0));
    if (!sums.some((x) => x > 0)) continue;
    out[`${g}_rss_mb`] = round(median(sums) / 1024, 1);
    out[`${g}_rss_max_mb`] = round(Math.max(...sums) / 1024, 1);
  }
  // CPU % between two moments: each process's ticks from its first sample to its last one in that span
  const cpu = (g, from, to) => {
    const by = new Map();
    for (const r of rows) {
      if (r.group !== g || r.ms < from || r.ms > to) continue;
      const p = by.get(r.pid);
      if (!p) by.set(r.pid, { first: r.ticks, last: r.ticks });
      else p.last = r.ticks;
    }
    const ticks = [...by.values()].reduce((n, p) => n + (p.last - p.first), 0);
    const secs = (Math.min(to, times.at(-1)) - Math.max(from, times[0])) / 1000;
    return secs > 0 ? round((ticks / clkTck / secs) * 100, 1) : null;
  };
  for (const g of [...Object.keys(GROUPS), "other"]) {
    if (!rows.some((r) => r.group === g)) continue;
    out[`${g}_cpu_pct`] = cpu(g, times[0], times.at(-1));
    for (const [w, [from, to]] of Object.entries(windows ?? {})) {
      if (g === "net" || to - from < 1500) continue;
      out[`${w}_${g}_cpu_pct`] = cpu(g, from, to);
    }
  }
  return out;
}

/** One run's numbers: the probe's, the launch to the page and to what it drew, and the processes'. */
function runOf(dir, clkTck) {
  const report = JSON.parse(read(join(dir, "report.json")) ?? "null");
  const launch = Number(read(join(dir, "launch_ms")) ?? NaN);
  const perf = report?.perf;
  const errors = [...(report?.errors ?? [])];
  if (!report) errors.push(`no report (exit ${(read(join(dir, "exit_code")) ?? "none, timed out").trim()}; see ${join(dir, "cage.log")})`);
  else if (!perf) errors.push("the report has no perf result");
  const numbers = { ...(perf?.numbers ?? {}) };
  if (perf && Number.isFinite(launch)) {
    const at = (k) => (typeof numbers[k] === "number" && numbers[k] >= 0 ? round(perf.timeOrigin + numbers[k] - launch) : undefined);
    numbers.launch_to_page_ms = round(perf.timeOrigin - launch);
    if (perf.scenario === "start") { numbers.launch_to_app_info_ms = at("app_info_ms"); numbers.launch_to_card_ms = at("first_card_ms"); }
    if (perf.scenario === "board") numbers.launch_to_board_ms = at("columns_drawn_ms");
  }
  Object.assign(numbers, processNumbers(read(join(dir, "samples.txt")) ?? "", clkTck, perf?.windows));
  for (const k of Object.keys(numbers)) if (numbers[k] === undefined || numbers[k] === null) delete numbers[k];
  return { ok: !!perf?.ok && errors.length === 0, errors, numbers, detail: perf?.detail };
}

function collect() {
  const [dir, outFile] = [args[1], args[2]];
  if (!dir || !outFile) { console.error("usage: perf-report.mjs --collect <runs dir> <out.json> [options]"); process.exit(2); }
  const clkTck = Number(opt("--clk-tck", "100")) || 100;
  const scenarios = {};
  for (const name of readdirSync(dir).sort((a, b) => a.localeCompare(b, "en", { numeric: true }))) {
    const m = /^([a-z]+)-(\d+)$/.exec(name);
    if (!m) continue;
    const s = (scenarios[m[1]] ??= { runs: [] });
    s.runs.push({ n: +m[2], ...runOf(join(dir, name), clkTck) });
  }
  for (const s of Object.values(scenarios)) {
    s.runs.sort((a, b) => a.n - b.n);
    const warm = s.runs.length > 1 ? s.runs.slice(1) : s.runs;
    const keys = [...new Set(s.runs.flatMap((r) => Object.keys(r.numbers)))];
    s.cold = s.runs[0]?.numbers ?? {};
    s.median = {}; s.min = {}; s.max = {};
    for (const k of keys) {
      const xs = warm.map((r) => r.numbers[k]).filter((x) => typeof x === "number" && x >= 0);
      if (!xs.length) continue;
      s.median[k] = round(median(xs), 1);
      s.min[k] = Math.min(...xs);
      s.max[k] = Math.max(...xs);
    }
    s.errors = s.runs.flatMap((r) => r.errors.map((e) => `run ${r.n}: ${e}`));
  }
  const bundleFile = opt("--bundle");
  const result = {
    commit: opt("--commit", "unknown"), date: new Date().toISOString(), runs: Number(opt("--runs", "0")) || null,
    webkitEnv: opt("--webkit-env") || null,
    machine: { cpu: cpus()[0]?.model ?? null, cores: cpus().length, kernel: release() },
    scenarios,
    bundle: bundleFile && existsSync(bundleFile) ? JSON.parse(readFileSync(bundleFile, "utf8")) : null,
  };
  writeFileSync(outFile, JSON.stringify(result, null, 1) + "\n");
  print(result);
  console.log(`\nwritten: ${outFile}`);
  if (Object.values(scenarios).some((s) => s.errors.length)) process.exitCode = 1;
}

const cell = (v, w) => String(v ?? "–").padStart(w);
const headOf = (name, nums) => {
  const h = HEAD[name] ?? {};
  const w = h.window;
  const cpu = (g) => nums[w ? `${w}_${g}_cpu_pct` : `${g}_cpu_pct`] ?? nums[`${g}_cpu_pct`];
  return [nums[h.time], nums[h.frame], nums[h.fps], nums[h.calls], nums[h.kb], nums.app_rss_mb, nums.web_rss_mb, cpu("app"), cpu("web")];
};
const HEAD_COLS = ["time ms", "longest frame ms", "fps", "calls", "KB back", "app RSS MB", "web RSS MB", "app CPU %", "web CPU %"];

function print(r) {
  console.log(`Gizai perf ${r.commit}, ${r.date.slice(0, 16).replace("T", " ")} UTC, ${r.runs ?? "?"} runs a scenario (medians of the warm runs)` +
    `${r.webkitEnv ? `, with ${r.webkitEnv}` : ""}; ${r.machine.cpu}`);
  console.log("time: start launch→first card, board launch→all columns, chat first screen, memory tree, runlog whole log, editor tree;");
  console.log("CPU % in the scenario's main window (board scrolling, runlog live, idle 60 s, editor open 60 s, lead streaming), else the whole run\n");
  console.log(["scenario".padEnd(8), ...HEAD_COLS.map((c) => c.padStart(c.length))].join("  "));
  for (const name of SCENARIOS.filter((n) => r.scenarios[n])) {
    console.log([name.padEnd(8), ...headOf(name, r.scenarios[name].median).map((v, i) => cell(v, HEAD_COLS[i].length))].join("  "));
  }
  for (const name of SCENARIOS.filter((n) => r.scenarios[n])) {
    const s = r.scenarios[name];
    console.log(`\n${name}${s.errors.length ? `  (${s.errors.length} errors)` : ""}`);
    console.log(`  ${"number".padEnd(34)} ${"median".padStart(9)} ${"cold".padStart(9)} ${"min".padStart(9)} ${"max".padStart(9)}`);
    for (const k of Object.keys(s.median).sort()) {
      console.log(`  ${k.padEnd(34)} ${cell(s.median[k], 9)} ${cell(s.cold[k], 9)} ${cell(s.min[k], 9)} ${cell(s.max[k], 9)}`);
    }
    for (const e of s.errors) console.log(`  ! ${e}`);
  }
  if (r.bundle?.totals) {
    const t = r.bundle.totals;
    console.log(`\nbundle: start files ${t.start.bytes} bytes (${t.start.gzip} gzip), later chunks ${t.later.bytes} (${t.later.gzip} gzip)`);
  }
}

function compare() {
  const [aFile, bFile] = [args[1], args[2]];
  const [a, b] = [aFile, bFile].map((f) => JSON.parse(readFileSync(f, "utf8")));
  const change = (x, y) => (typeof x === "number" && typeof y === "number" && x !== 0 ? `${y >= x ? "+" : ""}${round(((y - x) / Math.abs(x)) * 100, 1)}%` : "");
  console.log(`a: ${a.commit} (${a.date.slice(0, 16).replace("T", " ")})   b: ${b.commit} (${b.date.slice(0, 16).replace("T", " ")})   medians of the warm runs\n`);
  for (const name of SCENARIOS.filter((n) => a.scenarios[n] || b.scenarios[n])) {
    const [ma, mb] = [a.scenarios[name]?.median ?? {}, b.scenarios[name]?.median ?? {}];
    console.log(`${name}`);
    console.log(`  ${"number".padEnd(34)} ${"a".padStart(9)} ${"b".padStart(9)} ${"change".padStart(9)}`);
    for (const k of [...new Set([...Object.keys(ma), ...Object.keys(mb)])].sort()) {
      console.log(`  ${k.padEnd(34)} ${cell(ma[k], 9)} ${cell(mb[k], 9)} ${change(ma[k], mb[k]).padStart(9)}`);
    }
    console.log("");
  }
  if (a.bundle?.totals && b.bundle?.totals) {
    console.log("bundle");
    for (const part of ["start", "later", "all"]) for (const k of ["bytes", "gzip"]) {
      const [x, y] = [a.bundle.totals[part]?.[k], b.bundle.totals[part]?.[k]];
      console.log(`  ${`${part} ${k}`.padEnd(34)} ${cell(x, 9)} ${cell(y, 9)} ${change(x, y).padStart(9)}`);
    }
  }
}

if (args[0] === "--collect") collect();
else if (args[0] === "--compare") compare();
else { console.error("usage: perf-report.mjs --collect <runs dir> <out.json> [options] | --compare <a.json> <b.json>"); process.exit(2); }
