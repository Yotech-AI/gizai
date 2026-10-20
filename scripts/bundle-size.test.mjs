// Tests for scripts/bundle-size.mjs (GA-188), the bundle report: every file in dist/assets/ with its bytes and gzip bytes,
// the files dist/index.html loads at start apart from the ones loaded later. The script reads the dist/ next to its own
// folder, so each test copies it into a scratch checkout (<scratch>/scripts/bundle-size.mjs) with a small dist/ of its own.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import { afterAll, describe, expect, it } from "vitest";

const SCRIPT = fileURLToPath(new URL("./bundle-size.mjs", import.meta.url));
const scratch = mkdtempSync(join(tmpdir(), "bundle-size-"));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));

// What Vite's index.html names: the entry module, a preloaded chunk, the stylesheet; and what it doesn't: a lazy page, a
// font, a file in a subfolder, an icon link and a script from another host.
const INDEX = `<!doctype html>
<html>
  <head>
    <link rel="icon" href="/favicon.svg" />
    <script type="module" crossorigin src="/assets/index-AAA.js"></script>
    <link rel="modulepreload" crossorigin href="/assets/vendor-BBB.js?v=1">
    <link rel='stylesheet' href='./assets/index-CCC.css'>
    <script src="https://example.com/outside.js"></script>
  </head>
  <body><div id="root"></div></body>
</html>
`;
const FILES = {
  "index-AAA.js": "console.log('start');\n".repeat(400),
  "vendor-BBB.js": "export const v = 1;\n".repeat(300),
  "index-CCC.css": ".a { color: red }\n".repeat(200),
  "TaskPage-DDD.js": "export default function TaskPage() {}\n".repeat(900),
  "font-EEE.woff2": "w".repeat(50),
  "nested/chunk-FFF.js": "export const n = 2;\n".repeat(10),
};

/** A scratch checkout with the script and, unless `build` is false, a dist/ with `index` and `files` in dist/assets/. */
function checkout(name, { build = true, index = INDEX, files = FILES } = {}) {
  const root = join(scratch, name);
  mkdirSync(join(root, "scripts"), { recursive: true });
  copyFileSync(SCRIPT, join(root, "scripts", "bundle-size.mjs"));
  if (build) {
    for (const [file, text] of Object.entries(files)) {
      const path = join(root, "dist", "assets", file);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, text);
    }
    writeFileSync(join(root, "dist", "index.html"), index);
  }
  return root;
}

const run = (root, ...args) => spawnSync(process.execPath, [join(root, "scripts", "bundle-size.mjs"), ...args], { encoding: "utf8" });

/** The file's report entry as the script makes it: bytes, and bytes gzipped at level 9. */
const entry = (file) => {
  const data = Buffer.from(FILES[file]);
  return { file: `assets/${file}`, bytes: data.length, gzip: gzipSync(data, { level: 9 }).length };
};
const largestFirst = (a, b) => b.bytes - a.bytes || a.file.localeCompare(b.file);
const total = (files) => ({ bytes: files.reduce((n, f) => n + f.bytes, 0), gzip: files.reduce((n, f) => n + f.gzip, 0) });

describe("bundle-size.mjs --json", () => {
  it("lists every file with bytes and gzip bytes, the files index.html loads at start apart, largest first", () => {
    const r = run(checkout("json"), "--json");
    expect(r.status, r.stderr).toBe(0);
    const report = JSON.parse(r.stdout);
    const start = ["index-AAA.js", "vendor-BBB.js", "index-CCC.css"].map(entry).sort(largestFirst);
    const later = ["TaskPage-DDD.js", "font-EEE.woff2", "nested/chunk-FFF.js"].map(entry).sort(largestFirst);
    expect(report.start).toEqual(start);
    expect(report.later).toEqual(later);
    expect(report.totals).toEqual({ start: total(start), later: total(later), all: total([...start, ...later]) });
    // gzip is measured, not guessed: the repeated text shrinks, the 50 bytes of one letter don't
    expect(report.start.every((f) => f.gzip < f.bytes)).toBe(true);
  });

  it("counts a module script, a modulepreload and a stylesheet as start files, whatever their quotes or query", () => {
    const index = `<script src="/assets/index-AAA.js" type="module"></script>
<link href=/assets/vendor-BBB.js rel="modulepreload">
<link rel="preload stylesheet" href="assets/index-CCC.css#x">
<script src="/assets/TaskPage-DDD.js"></script>
<link rel="prefetch" href="/assets/nested/chunk-FFF.js">`;
    const report = JSON.parse(run(checkout("attrs", { index }), "--json").stdout);
    expect(report.start.map((f) => f.file).sort()).toEqual(["assets/index-AAA.js", "assets/index-CCC.css", "assets/vendor-BBB.js"]);
    // a classic script and a prefetch aren't loaded at start as the app's modules are
    expect(report.later.map((f) => f.file)).toContain("assets/TaskPage-DDD.js");
    expect(report.later.map((f) => f.file)).toContain("assets/nested/chunk-FFF.js");
  });
});

describe("bundle-size.mjs, the table", () => {
  it("prints the start files, then the later ones, each with a subtotal, then the total", () => {
    const r = run(checkout("table"));
    expect(r.status, r.stderr).toBe(0);
    const lines = r.stdout.trimEnd().split("\n");
    expect(lines[0]).toMatch(/^Loaded at start\s+bytes\s+KB\s+gzip\s+gzip KB$/);
    const at = (text) => lines.findIndex((l) => l.startsWith(text));
    const startTotal = at("Start, 3 files");
    const laterHead = at("Loaded later");
    const laterTotal = at("Later, 3 files");
    const allTotal = at("Total, 6 files");
    expect([startTotal, laterHead, laterTotal, allTotal].every((i) => i > 0)).toBe(true);
    expect(startTotal < laterHead && laterHead < laterTotal && laterTotal < allTotal).toBe(true);
    // each start file above the start's subtotal, each later file between the later heading and its subtotal
    for (const file of ["index-AAA.js", "vendor-BBB.js", "index-CCC.css"]) {
      const i = at(`  assets/${file}`);
      expect(i > 0 && i < startTotal, file).toBe(true);
    }
    for (const file of ["TaskPage-DDD.js", "font-EEE.woff2", "nested/chunk-FFF.js"]) {
      const i = at(`  assets/${file}`);
      expect(i > laterHead && i < laterTotal, file).toBe(true);
    }
    // a file's row: its bytes with thousands separators, KB (1,000 bytes) with one decimal, its gzip bytes and gzip KB
    const task = entry("TaskPage-DDD.js");
    const cells = lines[at("  assets/TaskPage-DDD.js")].trim().split(/\s+/);
    expect(cells).toEqual([
      "assets/TaskPage-DDD.js",
      task.bytes.toLocaleString("en-US"),
      (task.bytes / 1000).toFixed(1),
      task.gzip.toLocaleString("en-US"),
      (task.gzip / 1000).toFixed(1),
    ]);
  });
});

describe("bundle-size.mjs without a build or with a wrong argument", () => {
  it("says to run npm run build first and exits 1 when dist/ has no build", () => {
    const r = run(checkout("nobuild", { build: false }));
    expect(r.status).toBe(1);
    expect(r.stderr).toContain("npm run build");
    expect(r.stdout).toBe("");
  });

  it("prints its usage and exits 1 on an argument it doesn't know", () => {
    const r = run(checkout("usage"), "--html");
    expect(r.status).toBe(1);
    expect(r.stderr).toContain("Usage: node scripts/bundle-size.mjs [--json]");
  });
});
