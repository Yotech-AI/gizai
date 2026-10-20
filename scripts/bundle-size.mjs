#!/usr/bin/env node
// The bundle report (GA-188; docs/research/performance.md, section 7, part 5): after `npm run build`, every file in
// dist/assets/ with its size in bytes and gzipped (zlib, level 9), the files dist/index.html loads at start (its
// <script type="module" src>, <link rel="modulepreload" href> and <link rel="stylesheet" href>) apart from the ones the
// app loads later.
//
//   node scripts/bundle-size.mjs          a table: the start files, then the later ones (largest first), each group
//                                         with a subtotal, then the total. KB is 1,000 bytes, as Vite counts.
//   node scripts/bundle-size.mjs --json   the same as one JSON object, for scripts/perf.sh to keep:
//                                         { start: [{ file, bytes, gzip }], later: [...],
//                                           totals: { start: { bytes, gzip }, later: {...}, all: {...} } }
//
// A file is its path in dist/ ("assets/index-….js"). Fonts count as later: index.html doesn't name them, and WebKit
// fetches each one when text first uses it. Exit code 1 without a build (it says to run npm run build first).
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, sep } from "node:path";
import { gzipSync } from "node:zlib";

const dist = join(import.meta.dirname, "..", "dist");
const assets = join(dist, "assets");

// The files index.html loads at start, as paths in dist/.
function loadedAtStart(html) {
  const files = new Set();
  for (const [, tag, attrs] of html.matchAll(/<(script|link)\b([^>]*)>/gi)) {
    const attr = (name) => {
      const m = attrs.match(new RegExp(`(?:^|\\s)${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s"'>]+))`, "i"));
      return m ? (m[1] ?? m[2] ?? m[3]) : undefined;
    };
    const rel = (attr("rel") ?? "").toLowerCase().split(/\s+/);
    const url = tag.toLowerCase() === "script"
      ? attr("type")?.toLowerCase() === "module" && attr("src")
      : (rel.includes("modulepreload") || rel.includes("stylesheet")) && attr("href");
    // Only Gizai's own files: no other host. The path without its query, from dist/.
    if (url && !/^([a-z][a-z\d+.-]*:|\/\/)/i.test(url)) files.add(url.replace(/[?#].*$/, "").replace(/^\.?\//, ""));
  }
  return files;
}

// Every file in dist/assets/, largest first: its path in dist/, its bytes, and its bytes gzipped.
function measure() {
  return readdirSync(assets, { recursive: true })
    .filter((name) => statSync(join(assets, name)).isFile())
    .map((name) => {
      const data = readFileSync(join(assets, name));
      return { file: `assets/${name.split(sep).join("/")}`, bytes: data.length, gzip: gzipSync(data, { level: 9 }).length };
    })
    .sort((a, b) => b.bytes - a.bytes || a.file.localeCompare(b.file));
}

const sum = (files) => ({
  bytes: files.reduce((n, f) => n + f.bytes, 0),
  gzip: files.reduce((n, f) => n + f.gzip, 0),
});

// The table: one line per file, a subtotal per group, then the total.
function table({ start, later, totals }) {
  const all = [...start, ...later];
  const count = (files) => `${files.length} file${files.length === 1 ? "" : "s"}`;
  const bytes = (n) => n.toLocaleString("en-US");
  const kb = (n) => (n / 1000).toLocaleString("en-US", { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const head = (title) => [title, "bytes", "KB", "gzip", "gzip KB"];
  const row = (name, f) => [name, bytes(f.bytes), kb(f.bytes), bytes(f.gzip), kb(f.gzip)];
  const rows = [
    head("Loaded at start"),
    ...start.map((f) => row(`  ${f.file}`, f)),
    row(`Start, ${count(start)}`, totals.start),
    [],
    head("Loaded later"),
    ...later.map((f) => row(`  ${f.file}`, f)),
    row(`Later, ${count(later)}`, totals.later),
    [],
    row(`Total, ${count(all)}`, totals.all),
  ];
  // Each column as wide as its widest cell: the names to the left, the numbers to the right.
  const widths = rows[0].map((_, i) => Math.max(...rows.map((r) => r[i]?.length ?? 0)));
  return rows.map((r) => r.map((cell, i) => (i ? cell.padStart(widths[i]) : cell.padEnd(widths[i]))).join("  ")).join("\n");
}

const args = process.argv.slice(2);
if (args.some((arg) => arg !== "--json")) {
  console.error("Usage: node scripts/bundle-size.mjs [--json]");
  process.exit(1);
}
if (!existsSync(join(dist, "index.html")) || !existsSync(assets)) {
  console.error("bundle-size: there is no build in dist/: run npm run build first.");
  process.exit(1);
}
const atStart = loadedAtStart(readFileSync(join(dist, "index.html"), "utf8"));
const files = measure();
const start = files.filter((f) => atStart.has(f.file));
const later = files.filter((f) => !atStart.has(f.file));
const report = { start, later, totals: { start: sum(start), later: sum(later), all: sum(files) } };
console.log(args.includes("--json") ? JSON.stringify(report, null, 2) : table(report));
