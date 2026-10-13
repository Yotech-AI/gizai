// Tests for the product tour's capture (GA-149, round 3). This repository keeps only the capture, which needs the app:
// scripts/tour-capture.sh writes its stills and scenes.json to --out <folder>, or to .devdata/tour/capture, which git
// ignores; the tour video and the README's GIFs are made from those stills in the website's repository. The screen is
// 1600 × 1040 CSS pixels at scale 2, so every still is 3200 × 2080; the eight scenes are in the website's order; and
// the capture uses made-up data only, in a fake home, so no real project, client, folder or user name and no real home
// folder can show. The capture itself runs in a headless cage (cage, grim, wlr-randr), so it is run by hand, not here:
// these tests read the script, and run its argument handling on Linux.
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SCRIPT = join(ROOT, "scripts", "tour-capture.sh");
/** The website's feature rows 01 to 08 (design/website/README.md, GA-144), as the card lists them. */
const WEBSITE_ORDER = ["01-board", "02-chat", "03-clis", "04-review", "05-editor", "06-memory", "07-tools", "08-inbox"];

const read = (...parts) => readFileSync(join(ROOT, ...parts), "utf8");
const sh = read("scripts", "tour-capture.sh");
const git = (...args) => spawnSync("git", args, { cwd: ROOT, encoding: "utf8" });

/** The script's `scene <n> <name> "<title>" <route> <steps…>` lines, with their steps, joined over line ends. */
function scenes() {
  const body = sh.slice(sh.indexOf("# The website's eight features")).replace(/\\\n\s*/g, " ");
  return [...body.matchAll(/^scene (\d+) (\S+) "([^"]+)" (\S+)((?: +(?:"[^"]*"|'[^']*'))*)/gm)].map((m) => ({
    n: Number(m[1]), name: m[2], folder: `${m[1].padStart(2, "0")}-${m[2]}`, title: m[3], route: m[4],
    steps: [...m[5].matchAll(/"([^"]*)"|'([^']*)'/g)].map((s) => s[1] ?? s[2]),
  }));
}

/** Runs the script's argument handling (everything up to where OUT gets its default) from `cwd`, and prints OUT, the
 *  folder it went to and the arguments left. The rest needs the cage, so it doesn't run. */
function args(cwd, ...argv) {
  const head = sh.slice(0, sh.indexOf("\n", sh.indexOf("OUT=${OUT:-")) + 1);
  const r = spawnSync("bash", ["-c", `${head}printf '%s\\n' "$OUT" "$PWD" "$*"`, SCRIPT, ...argv], { cwd, encoding: "utf8" });
  const [out, pwd, rest] = r.stdout.split("\n");
  return { status: r.status, stdout: r.stdout, out, pwd, rest };
}

describe("the screen: 1600 × 1040 CSS pixels at scale 2, so 3200 × 2080 stills", () => {
  it("PAGE_W, PAGE_H and SCALE say so, and nothing in the script still names the old 1280 × 832 or 2560 × 1664", () => {
    expect(sh).toMatch(/^SCALE=2; PAGE_W=1600; PAGE_H=1040; TITLE=37$/m);
    expect(sh).not.toMatch(/\b(1280|832|2560|1664)\b/);
    expect(sh).toMatch(/1600x1040 CSS pixels at scale 2, so 3200x2080 pixels/);
  });

  it("the screen's mode, the crop, the size checks and scenes.json's screen all follow from them", () => {
    // the cage's screen: the page and the window's title bar, at scale 2
    expect(sh).toContain('MODE_SIZE="$((PAGE_W * SCALE))x$(((PAGE_H + TITLE) * SCALE))"');
    expect(sh).toContain("--custom-mode $MODE_SIZE --scale $SCALE");
    // each still: the screen's lowest PAGE_W × PAGE_H at scale 2, without the title bar
    expect(sh).toContain('crop=$((PAGE_W * SCALE)):$((PAGE_H * SCALE)):0:ih-$((PAGE_H * SCALE))');
    // the page's size as the app reports it, and every still's, are checked against the same numbers
    expect(sh).toContain('python3 - "$dir/steps.json" "$((PAGE_W * SCALE))" "$((PAGE_H * SCALE))"');
    expect(sh).toContain('grep -vc "^$((PAGE_W * SCALE)),$((PAGE_H * SCALE))\\$"');
    // scenes.json's screen: the stills' size in pixels and the scale
    expect(sh).toContain('scenes_json() {  # <theme>\n  python3 - "$OUT/$1" "$1" "$T/titles" "$((PAGE_W * SCALE))" "$((PAGE_H * SCALE))" "$SCALE"');
    expect(sh).toMatch(/"screen": \{"width": width, "height": height, "scale": scale\}/);
  });
});

describe("where the stills go: --out <folder>, or .devdata/tour/capture", () => {
  it("the usage names --out, and the stills, steps.json and scenes.json all go under OUT", () => {
    expect(sh).toContain("# usage: scripts/tour-capture.sh [--out <folder>] [dark|light|both] [scene number…]");
    expect(sh).toContain('local dir=$OUT/$theme/$name steps');
    expect(sh).toContain('GIZAI_TOUR_DIR="$dir"');
    expect(sh).toContain('python3 - "$OUT/$1"');
    expect(sh).not.toMatch(/video\//);
  });

  it("a new capture keeps the stills of earlier ones: only the scene it captures again is emptied", () => {
    const prepare = sh.slice(sh.indexOf("prepare() {"), sh.indexOf("\n}\n", sh.indexOf("prepare() {")));
    const wipe = prepare.match(/rm -rf [^&\n]+/g);
    expect(wipe).toEqual(['rm -rf "$BASE" "$LIVE" "$T/run" "$T/logs" "$T/ids.sh" "$T/titles" "$T/runner.sh" ']);
    const removals = [...sh.matchAll(/rm -rf ([^&\n]+)/g)].map((m) => m[1].trim());
    expect(removals.filter((r) => /OUT|dir/.test(r))).toEqual(['"$dir"']);
  });

  it(".devdata/ is ignored by git, so the default folder's stills never get committed", () => {
    for (const f of [".devdata/tour/capture/dark/scenes.json", ".devdata/tour/capture/light/01-board/open-card-before.png"]) {
      expect(git("check-ignore", "-q", f).status).toBe(0);
    }
    expect(git("ls-files", ".devdata").stdout).toBe("");
  });

  describe.runIf(process.platform === "linux")("its arguments (run by bash, as on Linux, where the capture runs)", () => {
    it("without --out the stills go to .devdata/tour/capture in the repository, wherever it is run from", () => {
      const r = args(join(ROOT, "src"), "dark", "3", "4");
      expect(r.status).toBe(0);
      expect(r.out).toBe(join(realpathSync(ROOT), ".devdata", "tour", "capture"));
      expect(r.pwd).toBe(realpathSync(ROOT));
      expect(r.rest).toBe("dark 3 4");
      expect(args(ROOT).out).toBe(join(realpathSync(ROOT), ".devdata", "tour", "capture"));
    });

    it("--out <folder> is read from where it is run, before the script goes to the repository's root", () => {
      const from = realpathSync(mkdtempSync(join(tmpdir(), "tour-out-")));
      try {
        const relative = args(from, "--out", "site/capture", "light", "5");
        expect(relative.status).toBe(0);
        expect(relative.out).toBe(join(from, "site", "capture"));
        expect(relative.pwd).toBe(realpathSync(ROOT));
        expect(relative.rest).toBe("light 5");
        // an absolute folder stays itself, and the folder needn't exist yet: the capture makes it
        const absolute = args(from, "--out", join(from, "elsewhere", "stills"));
        expect(absolute.out).toBe(join(from, "elsewhere", "stills"));
        expect(absolute.rest).toBe("");
        expect(existsSync(join(from, "site"))).toBe(false);
      } finally {
        rmSync(from, { recursive: true, force: true });
      }
    });

    it("--out without a folder prints the usage and stops", () => {
      const r = args(ROOT, "--out");
      expect(r.status).toBe(1);
      expect(r.stdout).toBe("usage: scripts/tour-capture.sh [--out <folder>] [dark|light|both] [scene number…]\n");
    });
  });
});

describe("the scenes: eight, in the website's order, as in round 2", () => {
  const list = scenes();

  it("numbered 1 to 8, in the website's order, each with at least one named click", () => {
    expect(list.map((s) => s.folder)).toEqual(WEBSITE_ORDER);
    expect(list.map((s) => s.n)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    for (const s of list) {
      expect(s.steps.some((step) => /^[a-z0-9][a-z0-9-]*(@\d+)?=/.test(step))).toBe(true);
    }
  });

  it("the named clicks are round 2's fifteen steps", () => {
    const named = list.map((s) => [s.folder, s.steps.filter((st) => /^[a-z-]+(@\d+)?=/.test(st)).map((st) => st.split(/[@=]/)[0])]);
    expect(named).toEqual([
      ["01-board", ["open-card"]],
      ["02-chat", ["approve"]],
      ["03-clis", ["agents-tab", "usage"]],
      ["04-review", ["open-card", "runs"]],
      ["05-editor", ["open-folder", "open-project", "terminal"]],
      ["06-memory", ["open-link", "graph"]],
      ["07-tools", ["settings", "use-playwright"]],
      ["08-inbox", ["inbox", "done-continue"]],
    ]);
  });

  it("the Review scene scrolls the card's Runs tab into view (and 150px under it) before it clicks it", () => {
    const review = list.find((s) => s.folder === "04-review");
    expect(review.steps).toEqual(["wait:2000", 'open-card=[data-card="NWS-2"]', "scroll:.tabs button.tab:text(Runs)", "runs=.tabs button.tab:text(Runs)"]);
  });

  it("the Editor and Terminal is one short scene: open a folder, pick a project in ~/Projects, then its Terminal", () => {
    const editor = list.find((s) => s.folder === "05-editor");
    expect(editor.steps.slice(1).map((s) => s.split(/[@=]/)[0])).toEqual(["open-folder", "open-project", "terminal"]);
    expect(editor.steps[2]).toMatch(/title="~\/Projects\/[a-z-]+"/);
    expect(editor.steps[3]).toMatch(/term-btn/);
    expect(list.some((s) => /terminal/.test(s.name))).toBe(false);
  });

  it("the CLI scene shows Claude Code and Codex: Settings → Agents and runs, then Usage", () => {
    const clis = list.find((s) => s.folder === "03-clis");
    expect(clis.title).toMatch(/Claude Code and Codex/);
    expect(clis.steps.slice(1).map((s) => s.split("=")[0])).toEqual(["agents-tab", "usage"]);
  });

  it("each scene is captured in dark and in light, the theme set by its first step", () => {
    expect(sh).toContain('case "$THEMES" in both) THEMES="dark light" ;; dark|light) ;; *) echo "$USAGE"; exit 1 ;; esac');
    expect(sh).toContain('steps="theme:$theme;$(IFS=\';\'; echo "$*")"');
  });
});

describe("the capture uses made-up data only", () => {
  // the names that must never show: the real user, the real home and projects folder, the round 1 tour's second account
  const REAL = /jefsev|jeffrey|sevinga|yotech|\/home\/|\/Users\/|~\/Herd|\bHerd\b|Claude Code 2/i;
  const sources = [
    "scripts/tour-capture.sh",
    "scripts/tour/fake-claude.sh",
    "scripts/tour/fake-codex.sh",
    "crates/gizai-core/examples/prep_tour.rs",
  ];

  it.each(sources)("%s names no real user, home folder or projects folder", (file) => {
    expect(read(file)).not.toMatch(REAL);
  });

  it("prep_tour makes three made-up projects for three made-up clients, and an owner who isn't a real person here", () => {
    const prep = read("crates/gizai-core/examples/prep_tour.rs");
    const projects = [...prep.matchAll(/project\("([^"]+)", "([A-Z]+)"/g)].map((m) => [m[1], m[2]]);
    expect(projects).toEqual([["Northwind Shop", "NWS"], ["Lumen Health", "LUM"], ["Orbit CRM", "ORB"]]);
    expect(prep).toMatch(/client\("Northwind Traders"/);
    expect(prep).toMatch(/client\("Lumen Health"/);
    expect(prep).toMatch(/client\("Orbit Labs"/);
    expect(prep).toMatch(/ensure_seed\(&db, "Alex Morgan"\)/);
    // no demo data from elsewhere: the old README GIF's data maker is gone
    expect(existsSync(join(ROOT, "crates/gizai-core/examples/prep_readme.rs"))).toBe(false);
  });

  it("Gizai runs in the cage with a fake home, its data, cache and config in it, and only the fakes' bin on its PATH", () => {
    expect(sh).toMatch(/FAKE_HOME=\$LIVE\/home; DATA=\$FAKE_HOME\/\.local\/share\/gizai/);
    expect(sh).toMatch(/PATH_IN_CAGE=\$FAKE_HOME\/\.local\/bin:\/usr\/local\/bin:\/usr\/bin:\/bin\n/);
    const run = sh.slice(sh.indexOf("env -i "), sh.indexOf("timeout -k"));
    for (const v of ['HOME="$FAKE_HOME"', 'PATH="$PATH_IN_CAGE"', 'GIZAI_DATA_DIR="$DATA"', 'XDG_DATA_HOME="$FAKE_HOME/.local/share"',
      'XDG_CACHE_HOME="$FAKE_HOME/.cache"', 'XDG_CONFIG_HOME="$FAKE_HOME/.config"', "WLR_BACKENDS=headless"]) {
      expect(run).toContain(v);
    }
    // the fake data lives in .devdata/tour, apart from the stills' folder
    expect(sh).toMatch(/^T=\$GZ\/\.devdata\/tour; BASE=\$T\/base; LIVE=\$T\/live$/m);
    // the fakes are Claude Code and Codex, at ~/.local/bin/claude and ~/.local/bin/codex in the fake home
    expect(sh).toContain('cp scripts/tour/fake-claude.sh "$FAKE_HOME/.local/bin/claude"');
    expect(sh).toContain('cp scripts/tour/fake-codex.sh "$FAKE_HOME/.local/bin/codex"');
    const prep = read("crates/gizai-core/examples/prep_tour.rs");
    expect(prep).toContain("~/.local/bin/claude");
    expect(prep).toContain("~/.local/bin/codex");
  });
});
