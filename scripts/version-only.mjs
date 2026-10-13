#!/usr/bin/env node
// Says whether a pull request changes nothing but Gizai's own version, as a release's first step does
// (docs/RELEASING.md, Making a release). CI and Install skip their builds and tests on such a pull request into main,
// as the release's pull request into production runs them all.
//
//   node scripts/version-only.mjs <base> <head>
//
// It looks at what <head> changes since it branched off <base>, as a pull request's Files changed does, in the git
// repository it runs in. That is version only when it changes these five files and nothing else, and in them only
// Gizai's version, from one old version to one new version, everywhere the version is:
//   - Cargo.toml: version under [workspace.package], which says the old and the new version;
//   - Cargo.lock: the version of each of the workspace's own crates (the packages without a source);
//   - package.json: version;
//   - package-lock.json: version, and the version of packages[""] (the package itself);
//   - src-tauri/tauri.conf.json: version.
// A version is MAJOR.MINOR.PATCH. Exit code 0: version only. 1: anything else, and it says what. 2: it can't tell
// (wrong arguments, no such commit, git failed).
import { execFileSync } from "node:child_process";

const FILES = ["Cargo.toml", "Cargo.lock", "package.json", "package-lock.json", "src-tauri/tauri.conf.json"];
// Where the JSON files keep the package's own version: the keys to it, by the name messages give it.
const JSON_VERSIONS = {
  "package.json": { version: ["version"] },
  "package-lock.json": { version: ["version"], 'packages[""].version': ["packages", "", "version"] },
  "src-tauri/tauri.conf.json": { version: ["version"] },
};

class Answer extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

function no(why) {
  throw new Answer(1, `Not version only: ${why}`);
}

function cant(why) {
  throw new Answer(2, `version-only: ${why}`);
}

function git(args, problem) {
  try {
    return execFileSync("git", args, { encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "pipe"] });
  } catch (e) {
    return cant(problem ?? `git ${args.join(" ")} failed: ${String(e.stderr || e.message).trim()}`);
  }
}

function versionOnly(baseRev, headRev) {
  const commit = (rev) => git(["rev-parse", "--verify", "--quiet", `${rev}^{commit}`], `${rev} is not a commit`).trim();
  const base = commit(baseRev);
  const head = commit(headRev);
  const from = git(["merge-base", base, head], `${baseRev} and ${headRev} have no commit in common`).trim();

  // Every change is an edit of one of the five files: no other file, none added or removed, no new mode or type.
  const raw = git(["diff", "--raw", "-z", "--no-renames", "--no-abbrev", from, head]).split("\0");
  const edited = new Set();
  for (let i = 0; i + 1 < raw.length; i += 2) {
    const entry = raw[i].match(/^:(\d+) (\d+) \w+ \w+ (\w+)$/) ?? cant(`git diff said: ${raw[i]}`);
    const [, oldMode, newMode, status] = entry;
    const path = raw[i + 1];
    if (!FILES.includes(path)) no(`${path} changes too.`);
    if (status !== "M") no(`${path} ${{ A: "is added", D: "is removed" }[status] ?? "changes its type"}.`);
    if (oldMode !== newMode) no(`${path} changes its mode.`);
    edited.add(path);
  }
  for (const path of FILES) if (!edited.has(path)) no(`${path} doesn't change.`);
  const [cargoToml, cargoLock, ...json] = FILES.map((path) => ({
    path,
    before: git(["cat-file", "blob", `${from}:${path}`]).split("\n"),
    after: git(["cat-file", "blob", `${head}:${path}`]).split("\n"),
  }));

  // Cargo.toml: version under [workspace.package] is Gizai's version, and the only line that changes.
  const tomlChanges = changedLines(cargoToml);
  const at = workspaceVersionLines(cargoToml.before);
  if (at.length !== 1) no(`Cargo.toml has ${at.length ? "more than one" : "no"} version under [workspace.package].`);
  const old = versionIn(cargoToml.before[at[0]]);
  const now = versionIn(cargoToml.after[at[0]]);
  if (!old || !now) no(`Cargo.toml's version under [workspace.package] isn't MAJOR.MINOR.PATCH (line ${at[0] + 1}).`);
  if (old === now) no(`Cargo.toml keeps version ${old}.`);
  onlyAt(cargoToml, tomlChanges, at, old, now);

  // Cargo.lock: the version of each of the workspace's own crates, all of them, and nothing else.
  const crates = packagesIn(cargoLock.before);
  const own = crates.filter((crate) => !crate.source && crate.version === old);
  if (!own.length) no(`Cargo.lock has none of the workspace's crates at ${old}.`);
  onlyAt(cargoLock, changedLines(cargoLock), own.map((crate) => crate.line), old, now, (i) => {
    const crate = crates.find((c) => c.line === i);
    return crate ? `the version of ${crate.name}` : `line ${i + 1}`;
  });

  // The JSON files: the package's own version, everywhere it is, and nothing else.
  for (const file of json) onlyVersions(file, JSON_VERSIONS[file.path], old, now);

  return `Version only: ${old} to ${now}.`;
}

// The indexes of a file's lines that change. A version bump keeps every line where it was.
function changedLines({ path, before, after }) {
  if (before.length !== after.length) no(`${path} adds or removes lines.`);
  return before.flatMap((line, i) => (line === after[i] ? [] : [i]));
}

// Each changed line changes nothing but the old version, in quotes, to the new one.
function versionsOnly({ path, before, after }, changes, old, now) {
  for (const i of changes) {
    if (after[i] === before[i].replaceAll(`"${old}"`, `"${now}"`)) continue;
    // The same line with another version: say which.
    const start = before[i].split(`"${old}"`)[0];
    const other = before[i].includes(`"${old}"`) && after[i].startsWith(start)
      ? after[i].slice(start.length).match(/^"(\d+\.\d+\.\d+)"/)?.[1]
      : undefined;
    if (other && after[i] === before[i].replaceAll(`"${old}"`, `"${other}"`)) {
      no(`${path} goes to ${other}, Cargo.toml to ${now}.`);
    }
    no(`${path} changes line ${i + 1} too.`);
  }
}

// The lines that change are the version's lines, all of them, each from the old version to the new one.
function onlyAt(file, changes, versionLines, old, now, what = (i) => `line ${i + 1}`) {
  for (const i of changes) if (!versionLines.includes(i)) no(`${file.path} changes ${what(i)} too.`);
  for (const i of versionLines) if (!changes.includes(i)) no(`${file.path} keeps ${what(i)} at ${old}.`);
  versionsOnly(file, changes, old, now);
}

// A JSON file changes the version at these places, each from the old version to the new one, and nothing else.
function onlyVersions(file, places, old, now) {
  versionsOnly(file, changedLines(file), old, now);
  const [before, after] = [file.before, file.after].map((lines) => {
    try {
      return JSON.parse(lines.join("\n"));
    } catch {
      return no(`${file.path} isn't valid JSON.`);
    }
  });
  for (const [name, place] of Object.entries(places)) {
    const key = place.at(-1);
    const was = holder(before, place)?.[key];
    const is = holder(after, place)?.[key];
    if (was !== old) no(`${file.path} had ${was === undefined ? `no ${name}` : `${name} ${was}`}, Cargo.toml ${old}.`);
    if (is !== now) no(`${file.path} ${is === undefined ? `has no ${name}` : `keeps ${name} at ${is}`}.`);
    holder(after, place)[key] = old;
  }
  if (JSON.stringify(after) !== JSON.stringify(before)) no(`${file.path} changes another ${old} to ${now} too.`);
}

// The object in a JSON document that holds the last of these keys, if there is one.
function holder(doc, place) {
  const isObject = (o) => o !== null && typeof o === "object";
  const found = place.slice(0, -1).reduce((at, key) => (isObject(at) ? at[key] : undefined), doc);
  return isObject(found) ? found : undefined;
}

// The indexes of Cargo.toml's lines with version under [workspace.package].
function workspaceVersionLines(lines) {
  let table = "";
  return lines.flatMap((line, i) => {
    if (/^\s*\[/.test(line)) table = line.replace(/#.*/, "").replace(/\s/g, "");
    return table === "[workspace.package]" && /^\s*version\s*=/.test(line) ? [i] : [];
  });
}

// The version in a line `version = "X.Y.Z"`.
function versionIn(line) {
  return line?.match(/^\s*version\s*=\s*"(\d+\.\d+\.\d+)"\s*$/)?.[1];
}

// Cargo.lock's packages: name, version, the index of the version's line, and whether it has a source. crates.io and
// git dependencies have one; the workspace's own crates don't.
function packagesIn(lines) {
  const packages = [];
  let crate = null;
  lines.forEach((line, i) => {
    if (/^\s*\[/.test(line)) {
      crate = /^\s*\[\[\s*package\s*\]\]\s*$/.test(line) ? { source: false } : null;
      if (crate) packages.push(crate);
      return;
    }
    const [, key, value] = line.match(/^\s*([\w-]+)\s*=\s*(.*?)\s*$/) ?? [];
    if (!crate || !key) return;
    if (key === "name") crate.name = value.match(/^"(.*)"$/)?.[1] ?? value;
    if (key === "version") [crate.version, crate.line] = [value.match(/^"(.*)"$/)?.[1], i];
    if (key === "source") crate.source = true;
  });
  return packages;
}

try {
  const args = process.argv.slice(2);
  if (args.length !== 2 || args.some((arg) => !arg || arg.startsWith("-"))) {
    throw new Answer(2, "Usage: node scripts/version-only.mjs <base> <head>");
  }
  console.log(versionOnly(...args));
} catch (e) {
  const answer = e instanceof Answer ? e : new Answer(2, `version-only: ${e?.stack ?? e}`);
  (answer.code === 1 ? console.log : console.error)(answer.message);
  process.exitCode = answer.code;
}
