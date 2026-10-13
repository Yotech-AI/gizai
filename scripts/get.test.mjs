// Tests for scripts/get.sh (GA-150), the one-line install for Linux and macOS: curl -fsSL <url>/get.sh | sh. Each
// test pipes get.sh into /bin/sh as curl does (dash on Ubuntu, macOS's own sh), with HOME in a scratch folder. Most
// run against a small git repository with releases v0.1.0 and v0.2.0, whose install.sh only says how it was run and
// writes that down; the last ones run this repository's own install.sh --check. Not on Windows, where get.ps1 runs
// instead (CI checks that one in both PowerShells).
import { spawnSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

const GET_SH = fileURLToPath(new URL("./get.sh", import.meta.url));
const REPO_ROOT = fileURLToPath(new URL("..", import.meta.url));
const unix = process.platform !== "win32";
const scratch = realpathSync(mkdtempSync(join(tmpdir(), "get-sh-")));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));
// git never looks above the scratch folder for a repository, so a get.sh that lost its --git-dir can't check out or
// fetch tags into a repository around it (the temp folder may be inside a checkout of Gizai).
const CEILING = { GIT_CEILING_DIRECTORIES: scratch };

function git(dir, ...args) {
  const run = spawnSync(
    "git",
    ["-c", "user.name=Gizai test", "-c", "user.email=test@gizai.invalid", "-c", "commit.gpgsign=false",
      "-c", "tag.gpgsign=false", "-c", "core.autocrlf=false", ...args],
    { cwd: dir, encoding: "utf8", env: { ...process.env, ...CEILING } },
  );
  if (run.status !== 0) throw new Error(`git ${args.join(" ")} failed: ${run.stderr}`);
  return run.stdout.trim();
}

// Writes the files (null removes one) and commits them. Returns the commit.
function commit(dir, files, message) {
  for (const [path, text] of Object.entries(files)) {
    if (text === null) {
      rmSync(join(dir, path));
      continue;
    }
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
  }
  git(dir, "add", "-A");
  git(dir, "commit", "-q", "--allow-empty", "-m", message);
  return git(dir, "rev-parse", "HEAD");
}

// A stand-in for install.sh: it says how it was run, writes a line to GET_TEST_LOG (its options, GIZAI_PREFIX,
// XDG_DATA_HOME, its own folder and what it read from stdin), and exits with FAKE_CHECK (with --check) or FAKE_INSTALL.
const FAKE_INSTALL = [
  "#!/usr/bin/env bash",
  'here="$(cd "${0%/*}" && pwd -P)"',
  'echo "install.sh $* (from $here)"',
  'input="$(cat)"',
  `printf '%s|%s|%s|%s|%s\\n' "$*" "\${GIZAI_PREFIX-}" "\${XDG_DATA_HOME-}" "$here" "$input" >> "$GET_TEST_LOG"`,
  'case " $* " in *" --check "*) exit "${FAKE_CHECK:-0}" ;; esac',
  'exit "${FAKE_INSTALL:-0}"',
  "",
].join("\n");

// The releases: v0.1.0, v0.2.0 (main), a branch next after it, and v0.9.0, which has no install.sh.
const fake = join(scratch, "gizai.git");
mkdirSync(fake);
git(fake, "init", "-q", "-b", "main");
const commits = {};
if (unix) {
  commits["v0.1.0"] = commit(fake, { "install.sh": FAKE_INSTALL, VERSION: "0.1.0\n", ".gitignore": "node_modules/\ntarget/\n" }, "0.1.0");
  git(fake, "tag", "-a", "-m", "v0.1.0", "v0.1.0");
  commits["v0.2.0"] = commit(fake, { VERSION: "0.2.0\n" }, "0.2.0");
  git(fake, "tag", "-a", "-m", "v0.2.0", "v0.2.0");
  git(fake, "checkout", "-q", "-b", "next");
  commits.next = commit(fake, { VERSION: "0.3.0-next\n" }, "Next");
  git(fake, "checkout", "-q", "-b", "no-installer", "v0.2.0");
  commits["v0.9.0"] = commit(fake, { "install.sh": null }, "Without install.sh");
  git(fake, "tag", "-a", "-m", "v0.9.0", "v0.9.0");
  git(fake, "checkout", "-q", "main");
}

// GitHub's answer for the latest release, as a file: GIZAI_RELEASES_URL takes file:// addresses.
function release(tag) {
  const file = join(scratch, `release-${tag}.json`);
  writeFileSync(file, `{\n  "url": "https://api.github.com/repos/Yotech-AI/gizai/releases/1",\n  "tag_name": "${tag}",\n  "name": "${tag}",\n  "draft": false,\n  "prerelease": false\n}\n`);
  return pathToFileURL(file).href;
}

let cases = 0;

// A scratch HOME, with the folder get.sh uses in it when XDG_DATA_HOME isn't set.
function newCase() {
  const root = join(scratch, `case-${++cases}`);
  const home = join(root, "home");
  mkdirSync(home, { recursive: true });
  const share = join(home, ".local", "share");
  return { root, home, share, src: join(share, "gizai-src"), log: join(root, "install.log") };
}

// get.sh piped into /bin/sh, as curl -fsSL <url> | sh -s -- <args> runs it, or with stdin run as a file
// (sh get.sh <args>) with that on stdin. env adds or (with undefined) removes variables.
function getSh(c, args = [], env = {}, stdin = undefined) {
  const all = {
    HOME: c.home,
    PATH: process.env.PATH,
    GIZAI_REPO: fake,
    GIZAI_RELEASES_URL: release("v0.1.0"),
    GET_TEST_LOG: c.log,
    ...env,
    ...CEILING,
  };
  const asFile = stdin !== undefined;
  const run = spawnSync("/bin/sh", asFile ? [GET_SH, ...args] : ["-s", "--", ...args], {
    input: asFile ? stdin : readFileSync(GET_SH),
    encoding: "utf8",
    env: Object.fromEntries(Object.entries(all).filter(([, v]) => v !== undefined)),
    timeout: 60_000,
  });
  return { code: run.status, out: `${run.stdout}${run.stderr}` };
}

// How the stand-in install.sh was run, one entry per run.
function runs(c) {
  if (!existsSync(c.log)) return [];
  return readFileSync(c.log, "utf8").trim().split("\n").map((line) => {
    const [args, prefix, xdg, from, input] = line.split("|");
    return { args, prefix, xdg, from, input };
  });
}

// A folder of programs that run the real ones by their full path, for a PATH with only these.
function tools(c, names, extra = {}) {
  const bin = join(c.root, "bin");
  mkdirSync(bin, { recursive: true });
  for (const name of names) {
    const found = spawnSync("/bin/sh", ["-c", `command -v ${name}`], { encoding: "utf8" }).stdout.trim();
    if (!found.startsWith("/")) throw new Error(`${name} isn't installed as a program`);
    writeFileSync(join(bin, name), `#!/bin/sh\nexec '${found}' "$@"\n`);
    chmodSync(join(bin, name), 0o755);
  }
  for (const [name, script] of Object.entries(extra)) {
    writeFileSync(join(bin, name), script);
    chmodSync(join(bin, name), 0o755);
  }
  return bin;
}

describe.skipIf(!unix)("scripts/get.sh", { timeout: 60_000 }, () => {
  it("gets the latest release into ~/.local/share/gizai-src, runs its install.sh --check, then install.sh", () => {
    const c = newCase();
    const r = getSh(c);
    expect(r.code, r.out).toBe(0);
    expect(r.out).toContain("Latest release: v0.1.0");
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);
    expect(runs(c).map((run) => run.args)).toEqual(["--check", ""]);
    expect(runs(c).every((run) => run.from === c.src)).toBe(true);
    // the check's result shows, before the install's
    expect(r.out).toMatch(/install\.sh --check \(from .*gizai-src\)[\s\S]*install\.sh  \(from .*gizai-src\)/);
    // the source folder has a repository of its own, and get.sh wrote nothing else
    expect(existsSync(join(c.src, ".git", "HEAD"))).toBe(true);
    expect(readdirSync(c.home)).toEqual([".local"]);
    expect(readdirSync(c.share)).toEqual(["gizai-src"]);
  });

  it("gives install.sh nothing on stdin, so neither run reads what was sent to get.sh", () => {
    const c = newCase();
    expect(getSh(c).code).toBe(0);
    const r = getSh(c, [], {}, "y\ny\n");
    expect(r.code, r.out).toBe(0);
    expect(runs(c).map((run) => run.input)).toEqual(["", "", "", ""]);
  });

  it("run again, updates the folder to the new release and installs again: changes go, target/ stays", () => {
    const c = newCase();
    expect(getSh(c).code).toBe(0);
    writeFileSync(join(c.src, "VERSION"), "changed by hand\n");
    writeFileSync(join(c.src, "stray.txt"), "left behind\n");
    mkdirSync(join(c.src, "node_modules", "left"), { recursive: true });
    mkdirSync(join(c.src, "target", "release"), { recursive: true });
    writeFileSync(join(c.src, "target", "release", "gizai"), "an earlier build\n");

    const r = getSh(c, [], { GIZAI_RELEASES_URL: release("v0.2.0") });
    expect(r.code, r.out).toBe(0);
    expect(r.out).toContain("Latest release: v0.2.0");
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.2.0"]);
    expect(readFileSync(join(c.src, "VERSION"), "utf8")).toBe("0.2.0\n");
    expect(existsSync(join(c.src, "stray.txt"))).toBe(false);
    expect(existsSync(join(c.src, "node_modules"))).toBe(false);
    expect(readFileSync(join(c.src, "target", "release", "gizai"), "utf8")).toBe("an earlier build\n");
    expect(git(c.src, "status", "--porcelain")).toBe("");
    expect(runs(c).map((run) => run.args)).toEqual(["--check", "", "--check", ""]);
  });

  it("updates a gizai-src that install.sh's own curl | bash cloned before", () => {
    const c = newCase();
    mkdirSync(c.share, { recursive: true });
    git(c.share, "clone", "-q", "--depth", "1", "--branch", "main", pathToFileURL(fake).href, "gizai-src");
    const r = getSh(c);
    expect(r.code, r.out).toBe(0);
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);
    expect(runs(c).map((run) => run.args)).toEqual(["--check", ""]);
  });

  it("makes a .git that was cut short again", () => {
    const c = newCase();
    mkdirSync(join(c.src, ".git"), { recursive: true });
    writeFileSync(join(c.src, ".git", "HEAD"), "");
    const r = getSh(c);
    expect(r.code, r.out).toBe(0);
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);
  });

  it("works in the source folder's own repository only, never in one around it", () => {
    const c = newCase();
    git(c.home, "init", "-q", "-b", "main");
    const outer = commit(c.home, { "notes.txt": "mine\n" }, "My home folder");
    const r = getSh(c);
    expect(r.code, r.out).toBe(0);
    expect(git(c.src, "rev-parse", "--absolute-git-dir")).toBe(join(c.src, ".git"));
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);
    expect(git(c.home, "rev-parse", "HEAD")).toBe(outer);
    expect(existsSync(join(c.home, ".git", "FETCH_HEAD"))).toBe(false);
    expect(readFileSync(join(c.home, "notes.txt"), "utf8")).toBe("mine\n");
    expect(git(c.home, "status", "--porcelain", "--untracked-files=no")).toBe("");
  });

  it("writes nothing in Gizai's data folder", () => {
    const c = newCase();
    const data = join(c.share, "gizai");
    mkdirSync(join(data, "backups"), { recursive: true });
    writeFileSync(join(data, "gizai.db"), "my data\n");
    expect(getSh(c).code).toBe(0);
    expect(getSh(c, [], { GIZAI_RELEASES_URL: release("v0.2.0") }).code).toBe(0);
    expect(readdirSync(data).sort()).toEqual(["backups", "gizai.db"]);
    expect(readdirSync(join(data, "backups"))).toEqual([]);
    expect(readFileSync(join(data, "gizai.db"), "utf8")).toBe("my data\n");
    expect(readdirSync(c.share).sort()).toEqual(["gizai", "gizai-src"]);
    expect(existsSync(join(c.home, "Library"))).toBe(false);
  });

  it("uses XDG_DATA_HOME's gizai-src, and passes GIZAI_PREFIX and XDG_DATA_HOME on to install.sh", () => {
    const c = newCase();
    const xdg = join(c.root, "xdg");
    const prefix = join(c.root, "opt", "gizai");
    const r = getSh(c, [], { XDG_DATA_HOME: xdg, GIZAI_PREFIX: prefix });
    expect(r.code, r.out).toBe(0);
    expect(git(join(xdg, "gizai-src"), "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);
    expect(existsSync(c.share)).toBe(false);
    expect(runs(c)).toEqual([
      { args: "--check", prefix, xdg, from: join(xdg, "gizai-src"), input: "" },
      { args: "", prefix, xdg, from: join(xdg, "gizai-src"), input: "" },
    ]);
  });

  it("passes its options on to install.sh, to the check and the install alike", () => {
    const c = newCase();
    expect(getSh(c, ["--build-only"]).code).toBe(0);
    expect(getSh(c, ["--skip-build"]).code).toBe(0);
    expect(runs(c).map((run) => run.args)).toEqual([
      "--check --build-only",
      "--build-only",
      "--check --skip-build",
      "--skip-build",
    ]);
  });

  it("with --check, runs only the check and ends with its result", () => {
    const c = newCase();
    expect(getSh(c, ["--check"]).code).toBe(0);
    const missing = getSh(c, ["--check"], { FAKE_CHECK: "1" });
    expect(missing.code).toBe(1);
    expect(missing.out).toContain("install.sh --check --check");
    expect(missing.out).not.toContain("Install what is missing above");
    expect(runs(c).map((run) => run.args)).toEqual(["--check --check", "--check --check"]);
  });

  it("stops when the check finds something missing, and says so, except with --skip-build", () => {
    const c = newCase();
    const r = getSh(c, [], { FAKE_CHECK: "1" });
    expect(r.code).toBe(1);
    expect(r.out).toContain("Install what is missing above, then run this again.");
    expect(runs(c).map((run) => run.args)).toEqual(["--check"]);

    const skip = getSh(c, ["--skip-build"], { FAKE_CHECK: "1" });
    expect(skip.code, skip.out).toBe(0);
    expect(runs(c).map((run) => run.args)).toEqual(["--check", "--check --skip-build", "--skip-build"]);
  });

  it("stops with the check's code when it fails otherwise, and ends with the install's code", () => {
    const c = newCase();
    expect(getSh(c, [], { FAKE_CHECK: "2" }).code).toBe(2);
    expect(runs(c).map((run) => run.args)).toEqual(["--check"]);
    expect(getSh(c, [], { FAKE_INSTALL: "3" }).code).toBe(3);
    expect(runs(c).map((run) => run.args)).toEqual(["--check", "--check", ""]);
  });

  it("runs --uninstall without the check, and refuses --check with --uninstall before getting anything", () => {
    const c = newCase();
    const r = getSh(c, ["--uninstall", "--purge"]);
    expect(r.code, r.out).toBe(0);
    expect(runs(c).map((run) => run.args)).toEqual(["--uninstall --purge"]);

    const d = newCase();
    const both = getSh(d, ["--check", "--uninstall"]);
    expect(both.code).toBe(2);
    expect(both.out).toContain("--check and --uninstall don't go together");
    expect(existsSync(d.share)).toBe(false);
    expect(runs(d)).toEqual([]);
  });

  it("with --help, shows the one line and its options, and does nothing", () => {
    const c = newCase();
    const r = getSh(c, ["--help"]);
    expect(r.code).toBe(0);
    expect(r.out).toContain("curl -fsSL https://gizai.app/install.sh | sh");
    expect(r.out).not.toContain("raw.githubusercontent.com");
    expect(r.out).toContain("--uninstall");
    expect(readdirSync(c.home)).toEqual([]);
  });

  it("without git, bash or curl, says in one sentence what to install, and does nothing else", () => {
    const c = newCase();
    const none = getSh(c, [], { PATH: tools(c, ["uname"]) });
    expect(none.code).toBe(1);
    expect(none.out.trim()).toBe(
      "To install GizAi, first install git, bash and curl with your package manager, then run this again.",
    );
    expect(readdirSync(c.home)).toEqual([]);

    const d = newCase();
    const noGit = getSh(d, [], { PATH: tools(d, ["uname", "bash", "curl"]) });
    expect(noGit.code).toBe(1);
    expect(noGit.out.trim()).toBe(
      process.platform === "darwin"
        ? "To install GizAi, first install Apple's Command Line Tools, which bring git: run xcode-select --install, then run this again."
        : "To install GizAi, first install git with your package manager, then run this again.",
    );
    expect(readdirSync(d.home)).toEqual([]);

    const e = newCase();
    const noBash = getSh(e, [], { PATH: tools(e, ["uname", "git", "curl"]) });
    expect(noBash.code).toBe(1);
    expect(noBash.out.trim()).toBe("To install GizAi, first install bash with your package manager, then run this again.");
    expect(readdirSync(e.home)).toEqual([]);
    expect(runs(e)).toEqual([]);
  });

  it("needs no curl when GIZAI_REF says what to get", () => {
    const c = newCase();
    const r = getSh(c, [], { PATH: tools(c, ["uname", "git", "bash", "mkdir", "rm", "cat"]), GIZAI_REF: "v0.2.0" });
    expect(r.code, r.out).toBe(0);
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.2.0"]);
  });

  it("on another system, points to get.ps1 and does nothing", () => {
    const c = newCase();
    const r = getSh(c, [], { PATH: tools(c, ["git", "bash", "curl"], { uname: "#!/bin/sh\necho MINGW64_NT-10.0-26100\n" }) });
    expect(r.code).toBe(1);
    expect(r.out.trim()).toBe(
      "This installs GizAi on Linux and macOS. On Windows, run this in PowerShell: irm https://gizai.app/install.ps1 | iex",
    );
    expect(readdirSync(c.home)).toEqual([]);
  });

  it("gets what it downloads from GitHub, never from gizai.app: the latest release from GitHub's API, the source from github.com", () => {
    // curl and git stand-ins that write down what they were asked and stop there, so nothing goes over the network
    const realGit = spawnSync("/bin/sh", ["-c", "command -v git"], { encoding: "utf8" }).stdout.trim();
    const net = (c) => join(c.root, "net.log");
    const offline = (c) => `${tools(c, [], {
      curl: `#!/bin/sh\nprintf 'curl %s\\n' "$*" >> '${net(c)}'\nexit 22\n`,
      git: `#!/bin/sh\ncase " $* " in *" fetch "*) printf 'git %s\\n' "$*" >> '${net(c)}'; exit 128 ;; esac\nexec '${realGit}' "$@"\n`,
    })}:${process.env.PATH}`;

    const c = newCase();
    const latest = getSh(c, [], { PATH: offline(c), GIZAI_REPO: undefined, GIZAI_RELEASES_URL: undefined });
    expect(latest.code).toBe(1);
    expect(latest.out).toContain(
      "Could not read the latest release from https://api.github.com/repos/Yotech-AI/gizai/releases/latest.",
    );
    const asked = readFileSync(net(c), "utf8").trim().split("\n");
    expect(asked).toHaveLength(1);
    expect(asked[0]).toMatch(/^curl .* https:\/\/api\.github\.com\/repos\/Yotech-AI\/gizai\/releases\/latest$/);
    expect(existsSync(c.share)).toBe(false);

    const d = newCase();
    const source = getSh(d, [], { PATH: offline(d), GIZAI_REPO: undefined, GIZAI_RELEASES_URL: undefined, GIZAI_REF: "v0.2.0" });
    expect(source.code).toBe(1);
    expect(source.out).toContain("Could not get v0.2.0 from https://github.com/Yotech-AI/gizai.git.");
    const fetched = readFileSync(net(d), "utf8").trim().split("\n");
    expect(fetched).toHaveLength(1);
    expect(fetched[0]).toMatch(/ fetch .* -- https:\/\/github\.com\/Yotech-AI\/gizai\.git v0\.2\.0$/);
    expect(runs(d)).toEqual([]);

    for (const log of [...asked, ...fetched]) expect(log).not.toContain("gizai.app");
  });

  it("gets a branch or a commit instead of the latest release with GIZAI_REF, and stops at one that doesn't exist", () => {
    const c = newCase();
    const branch = getSh(c, [], { GIZAI_REF: "next", GIZAI_RELEASES_URL: undefined });
    expect(branch.code, branch.out).toBe(0);
    expect(branch.out).not.toContain("Latest release");
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits.next);

    const sha = getSh(c, [], { GIZAI_REF: commits["v0.1.0"] });
    expect(sha.code, sha.out).toBe(0);
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);

    const missing = getSh(c, [], { GIZAI_REF: "no-such-ref" });
    expect(missing.code).toBe(1);
    expect(missing.out).toContain(`Could not get no-such-ref from ${realpathSync(fake)}.`);
    expect(git(c.src, "rev-parse", "HEAD")).toBe(commits["v0.1.0"]);
    expect(runs(c)).toHaveLength(4);
  });

  it("stops before getting anything when the latest release can't be read or its tag isn't a version", () => {
    const c = newCase();
    const unread = getSh(c, [], { GIZAI_RELEASES_URL: pathToFileURL(join(scratch, "no-such-release.json")).href });
    expect(unread.code).toBe(1);
    expect(unread.out).toContain("Could not read the latest release from");
    const nightly = getSh(c, [], { GIZAI_RELEASES_URL: release("nightly") });
    expect(nightly.code).toBe(1);
    expect(nightly.out).toContain("The latest release's tag, nightly, isn't a version like v1.2.3");
    const notGitHub = getSh(c, [], { GIZAI_RELEASES_URL: undefined });
    expect(notGitHub.code).toBe(1);
    expect(notGitHub.out).toContain("isn't on GitHub, so there is no latest release to ask it for");
    expect(existsSync(c.share)).toBe(false);
    expect(runs(c)).toEqual([]);
  });

  it("stops at a release without install.sh", () => {
    const c = newCase();
    const r = getSh(c, [], { GIZAI_RELEASES_URL: release("v0.9.0") });
    expect(r.code).toBe(1);
    expect(r.out).toContain("v0.9.0 has no install.sh, so it can't be installed this way.");
    expect(runs(c)).toEqual([]);
  });

  it("runs this repository's own install.sh --check, and its result shows", () => {
    const c = newCase();
    const r = getSh(c, ["--check"], { GIZAI_REPO: REPO_ROOT, GIZAI_REF: "HEAD", GIZAI_RELEASES_URL: undefined });
    expect(r.out).toContain("== Checking what Gizai needs");
    expect(r.out).toMatch(/Claude Code: (found|not found)/);
    expect(r.code === 0, r.out).toBe(r.out.includes("Everything Gizai needs to build is here."));
    expect(git(c.src, "rev-parse", "HEAD")).toBe(git(REPO_ROOT, "rev-parse", "HEAD"));
    expect(readdirSync(c.share)).toEqual(["gizai-src"]);

    // an option install.sh doesn't know stops at its check, with its code
    const typo = getSh(c, ["--chek"], { GIZAI_REPO: REPO_ROOT, GIZAI_REF: "HEAD", GIZAI_RELEASES_URL: undefined });
    expect(typo.code).toBe(2);
    expect(typo.out).toContain("unknown option: --chek");
    expect(readdirSync(c.share)).toEqual(["gizai-src"]);
  });
});

// The one lines people see are gizai.app's (GA-159), which the website sends on to get.sh and get.ps1 on production;
// what the scripts download stays on GitHub. get.ps1 is read as text here: CI runs it, on Windows.
describe("the one-line installs people see", () => {
  const CURL = "curl -fsSL https://gizai.app/install.sh | sh";
  const IRM = "irm https://gizai.app/install.ps1 | iex";
  const read = (path) => readFileSync(join(REPO_ROOT, path), "utf8");

  it("README.md, docs/install.md and the website's design show gizai.app's lines, not GitHub's raw addresses", () => {
    for (const path of ["README.md", "docs/install.md", "design/website/README.md"]) {
      const text = read(path);
      expect(text, path).toContain(CURL);
      expect(text, path).toContain(IRM);
      expect(text, path).not.toContain("raw.githubusercontent.com");
      expect(text, path).not.toContain("gizai.ai");
    }
  });

  it("get.sh and get.ps1 show gizai.app's lines in their headers, and only ever show them", () => {
    const sh = read("scripts/get.sh").split("\n");
    expect(sh[2]).toBe(`#   ${CURL}`);
    expect(sh).toContain("GET_PS1=https://gizai.app/install.ps1");
    expect(sh.filter((line) => line.includes("$GET_PS1")).map((line) => line.trim())).toEqual([
      '*) say "This installs GizAi on Linux and macOS. On Windows, run this in PowerShell: irm $GET_PS1 | iex"; return 1 ;;',
    ]);

    const ps1 = read("scripts/get.ps1").split(/\r?\n/);
    expect(ps1[1]).toBe(`#   ${IRM}`);
    expect(ps1).toContain("  $GetPs1 = 'https://gizai.app/install.ps1'");
    expect(ps1).toContain("  $GetSh = 'https://gizai.app/install.sh'");
    const shown = ps1.filter((line) => /\$GetPs1|\$GetSh/.test(line) && !/^\s*\$Get(Ps1|Sh) = /.test(line));
    expect(shown.map((line) => line.trim())).toEqual([
      'Say "  irm $GetPs1 | iex"',
      'Say "This installs GizAi on Windows. On Linux and macOS, run: curl -fsSL $GetSh | sh"',
    ]);
  });

  it("get.sh and get.ps1 download from GitHub: gizai.app is only in what they show and in comments", () => {
    for (const path of ["scripts/get.sh", "scripts/get.ps1"]) {
      const lines = read(path).split(/\r?\n/);
      const app = lines.filter((line) => line.includes("gizai.app") && !line.trim().startsWith("#"));
      expect(app.map((line) => line.trim()), path).toEqual(
        path.endsWith(".sh")
          ? ["GET_PS1=https://gizai.app/install.ps1", CURL]
          : ["$GetPs1 = 'https://gizai.app/install.ps1'", "$GetSh = 'https://gizai.app/install.sh'"],
      );
    }
    const ps1 = read("scripts/get.ps1");
    expect(ps1).toContain("$DefaultRepo = 'https://github.com/Yotech-AI/gizai.git'");
    expect(ps1).toContain('$url = "https://api.github.com/repos/$($Matches[2])/releases/latest"');
    expect(ps1).toContain("$repo = if ($env:GIZAI_REPO) { $env:GIZAI_REPO } else { $DefaultRepo }");
  });
});
