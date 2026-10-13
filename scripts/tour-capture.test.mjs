// Tests for the product tour's stills and what makes them (GA-149, round 2): video/capture/<theme>/ holds the eight
// scenes in the website's order and scenes.json matches the stills there; the capture uses made-up data only, in a fake
// home, so no real project, client, folder or user name and no real home folder can show; the Editor and Terminal is
// one short scene; and the video's frame never zooms. The stills' pixels are checked by eye and OCR at QA, not here.
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const CAPTURE = join(ROOT, "video", "capture");
const THEMES = ["dark", "light"];
const SCREEN = { width: 2560, height: 1664 };
/** The website's feature rows 01 to 08 (design/website/README.md, GA-144), as the card lists them. */
const WEBSITE_ORDER = ["01-board", "02-chat", "03-clis", "04-review", "05-editor", "06-memory", "07-tools", "08-inbox"];

const read = (...parts) => readFileSync(join(ROOT, ...parts), "utf8");
const scenesOf = (theme) => JSON.parse(read("video", "capture", theme, "scenes.json"));

function pngSize(buf) {
  expect(buf.subarray(1, 4).toString("latin1")).toBe("PNG");
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

describe.each(THEMES)("video/capture/%s/", (theme) => {
  const json = scenesOf(theme);

  it("scenes.json is for its theme and lists the eight scenes in the website's order, numbered 1 to 8", () => {
    expect(json.theme).toBe(theme);
    expect(json.screen).toEqual({ ...SCREEN, scale: 2 });
    expect(json.scenes.map((s) => s.name)).toEqual(WEBSITE_ORDER);
    expect(json.scenes.map((s) => s.scene)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    for (const scene of json.scenes) expect(scene.steps.length).toBeGreaterThan(0);
  });

  it("holds only those scenes' folders: none left from the first round's ten", () => {
    const folders = readdirSync(join(CAPTURE, theme), { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name);
    expect(folders.sort()).toEqual(WEBSITE_ORDER);
    expect(readdirSync(join(CAPTURE, theme)).filter((f) => !folders.includes(f))).toEqual(["scenes.json"]);
  });

  it.each(WEBSITE_ORDER)("%s: its stills are exactly the steps' before and after, and steps.json says the same as scenes.json", (name) => {
    const scene = json.scenes.find((s) => s.name === name);
    const folder = join(CAPTURE, theme, name);
    const listed = scene.steps.flatMap((s) => [s.before, s.after]);
    for (const path of listed) expect(path.startsWith(`${name}/`)).toBe(true);
    expect(readdirSync(folder).filter((f) => f.endsWith(".png")).map((f) => `${name}/${f}`).sort()).toEqual([...listed].sort());
    const steps = JSON.parse(readFileSync(join(folder, "steps.json"), "utf8"));
    expect(steps.errors).toEqual([]);
    expect(steps.viewport).toEqual({ ...SCREEN, scale: 2 });
    expect(steps.steps.map((s) => ({ name: s.name, before: `${name}/${s.before}`, after: `${name}/${s.after}`, box: s.box, selector: s.selector })))
      .toEqual(scene.steps);
  });

  it("every still is a 2560 × 1664 PNG, and every clicked box lies inside it", () => {
    for (const step of json.scenes.flatMap((s) => s.steps)) {
      for (const still of [step.before, step.after]) expect(pngSize(readFileSync(join(CAPTURE, theme, still)))).toEqual(SCREEN);
      const { x, y, width, height } = step.box;
      expect(width).toBeGreaterThan(0);
      expect(height).toBeGreaterThan(0);
      expect(x).toBeGreaterThanOrEqual(0);
      expect(y).toBeGreaterThanOrEqual(0);
      expect(x + width).toBeLessThanOrEqual(SCREEN.width);
      expect(y + height).toBeLessThanOrEqual(SCREEN.height);
    }
  });

  it("the Editor and Terminal is one short scene: open a folder, pick a project in ~/Projects, then its Terminal", () => {
    const editor = json.scenes.find((s) => s.name === "05-editor");
    expect(editor.steps.map((s) => s.name)).toEqual(["open-folder", "open-project", "terminal"]);
    expect(editor.steps[1].selector).toMatch(/title="~\/Projects\/[a-z-]+"/);
    expect(editor.steps[2].selector).toMatch(/term-btn/);
    // no separate terminals scene any more
    expect(json.scenes.some((s) => /terminal/.test(s.name))).toBe(false);
  });

  it("the CLI scene opens Settings → Agents and runs (Coding CLIs), then Usage", () => {
    const clis = json.scenes.find((s) => s.name === "03-clis");
    expect(clis.title).toMatch(/Claude Code and Codex/);
    expect(clis.steps.map((s) => s.name)).toEqual(["agents-tab", "usage"]);
  });
});

describe("dark and light are the same tour", () => {
  it("the same scenes and steps, in the same order", () => {
    const shape = (theme) => scenesOf(theme).scenes.map((s) => ({ name: s.name, title: s.title, steps: s.steps.map((st) => [st.name, st.selector]) }));
    expect(shape("light")).toEqual(shape("dark"));
  });
});

describe("the video follows the capture", () => {
  const source = read("video", "src", "scenes.ts");

  it("video/src/scenes.ts's ORDER is the website's order, with a numbered caption for each scene", () => {
    const order = [...source.match(/export const ORDER = \[([^\]]*)\]/)[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    expect(order).toEqual(WEBSITE_ORDER);
    WEBSITE_ORDER.forEach((name, i) => {
      expect(source).toMatch(new RegExp(`"${name}": \\{ number: "0${i + 1}", name: "[^"]+", line: "[^"]+" \\}`));
    });
    expect(source).toMatch(/"03-clis": \{[^}]*Claude Code and Codex/);
  });

  it("the frame never zooms: the stills fill it at their own size; the only scaling is the cursor's and the opening logo's", () => {
    const code = (f) => read("video", "src", f).replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, "");
    const files = readdirSync(join(ROOT, "video", "src")).filter((f) => /\.(ts|tsx)$/.test(f));
    const scaling = files.filter((f) => /scale\(|\bzoom\b|camera/i.test(code(f)));
    expect(scaling.sort()).toEqual(["Cursor.tsx", "Ends.tsx"]);
    // Ends.tsx: the logo on its own, before the first scene, grows by 2.5% at most; the closing doesn't move
    expect(code("Ends.tsx").match(/scale\([^)]*\)/g)).toEqual(["scale(${1 + 0.025 * progress})"]);
    expect(code("Ends.tsx").slice(code("Ends.tsx").indexOf("export const Closing"))).not.toMatch(/scale\(/);
    const tour = read("video", "src", "Tour.tsx");
    expect(tour).toMatch(/position: "absolute", left: 0, top: 0, width: WIDTH, height: HEIGHT/);
    expect(tour).not.toMatch(/transform/);
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

  it.each(THEMES)("%s: scenes.json and the steps.json files name no real user or folder", (theme) => {
    const files = [join(CAPTURE, theme, "scenes.json"), ...WEBSITE_ORDER.map((n) => join(CAPTURE, theme, n, "steps.json"))];
    for (const f of files) expect(readFileSync(f, "utf8")).not.toMatch(REAL);
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
    const sh = read("scripts/tour-capture.sh");
    expect(sh).toMatch(/FAKE_HOME=\$LIVE\/home; DATA=\$FAKE_HOME\/\.local\/share\/gizai/);
    expect(sh).toMatch(/PATH_IN_CAGE=\$FAKE_HOME\/\.local\/bin:\/usr\/local\/bin:\/usr\/bin:\/bin\n/);
    const run = sh.slice(sh.indexOf("env -i "), sh.indexOf("timeout -k"));
    for (const v of ['HOME="$FAKE_HOME"', 'PATH="$PATH_IN_CAGE"', 'GIZAI_DATA_DIR="$DATA"', 'XDG_DATA_HOME="$FAKE_HOME/.local/share"',
      'XDG_CACHE_HOME="$FAKE_HOME/.cache"', 'XDG_CONFIG_HOME="$FAKE_HOME/.config"', "WLR_BACKENDS=headless"]) {
      expect(run).toContain(v);
    }
    // the fakes are Claude Code and Codex, at ~/.local/bin/claude and ~/.local/bin/codex in the fake home
    expect(sh).toContain('cp scripts/tour/fake-claude.sh "$FAKE_HOME/.local/bin/claude"');
    expect(sh).toContain('cp scripts/tour/fake-codex.sh "$FAKE_HOME/.local/bin/codex"');
    const prep = read("crates/gizai-core/examples/prep_tour.rs");
    expect(prep).toContain("~/.local/bin/claude");
    expect(prep).toContain("~/.local/bin/codex");
  });
});
