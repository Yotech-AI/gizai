// Tests for what is left of the product tour's renders in this repository (GA-149, round 3): the tour video and its
// Remotion project moved to the website's repository, so docs/tour/ holds only the README's two GIFs, dark and light,
// each 1280 × 832 at 12 to 15 fps, 45 to 60 s, under 10 MB; the README shows the one for GitHub's theme and says where
// the video and the GIFs are made; and git has no video/ folder and no MP4, WebM or poster. The GIFs' headers are read
// here with no ffprobe, so the tests run wherever `npm test` does.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const TOUR = join(ROOT, "docs", "tour");
const MB = 1000 * 1000;
const THEMES = ["dark", "light"];
/** The README's GIF: the website's 1280 × 832 frame, the whole tour (the card's 45 to 60 seconds). */
const GIF = { width: 1280, height: 832, minFps: 12, maxFps: 15.05, minSeconds: 45, maxSeconds: 60 };

// GIF: the logical screen, and each frame's delay from its Graphic Control Extension (in hundredths of a second).
function gifInfo(buf) {
  const skipSubBlocks = (at) => {
    while (buf[at] !== 0) at += buf[at] + 1;
    return at + 1;
  };
  const colourTable = (packed) => (packed & 0x80 ? 3 * 2 ** ((packed & 7) + 1) : 0);
  const out = { signature: buf.toString("latin1", 0, 6), width: buf.readUInt16LE(6), height: buf.readUInt16LE(8), frames: 0, delays: [], loops: false };
  let at = 13 + colourTable(buf[10]);
  let delay = 0;
  while (at < buf.length && buf[at] !== 0x3b) {
    if (buf[at] === 0x21) {
      const label = buf[at + 1];
      if (label === 0xf9) delay = buf.readUInt16LE(at + 4);
      if (label === 0xff && buf.toString("latin1", at + 3, at + 14) === "NETSCAPE2.0") out.loops = true;
      at = skipSubBlocks(at + 2);
    } else if (buf[at] === 0x2c) {
      out.frames++;
      out.delays.push(delay);
      at = skipSubBlocks(at + 10 + colourTable(buf[at + 9]) + 1);
    } else {
      throw new Error(`a GIF block that starts with 0x${buf[at].toString(16)} at ${at}`);
    }
  }
  out.ended = buf[at] === 0x3b;
  return out;
}

const gif = (theme) => join(TOUR, `tour-${theme}.gif`);
const git = (...args) => spawnSync("git", args, { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * MB });
const tracked = (...paths) => {
  const r = git("ls-files", "--", ...paths);
  if (r.status !== 0) throw new Error(`git ls-files: ${r.stderr}`);
  return r.stdout.split("\n").filter(Boolean);
};

describe("docs/tour/ holds only the README's two GIFs", () => {
  it("tour-dark.gif and tour-light.gif, on disk and in git, and nothing else", () => {
    expect(readdirSync(TOUR).sort()).toEqual(["tour-dark.gif", "tour-light.gif"]);
    expect(tracked("docs/tour").sort()).toEqual(["docs/tour/tour-dark.gif", "docs/tour/tour-light.gif"]);
  });

  it.each(THEMES)("%s: a 1280 × 832 GIF of the whole tour, 45 to 60 s at 12 to 15 fps, that loops and stays under 10 MB", (theme) => {
    expect(statSync(gif(theme)).size).toBeLessThan(10 * MB);
    const info = gifInfo(readFileSync(gif(theme)));
    expect(info.signature).toBe("GIF89a");
    expect(info.ended).toBe(true);
    expect({ width: info.width, height: info.height }).toEqual({ width: GIF.width, height: GIF.height });
    expect(info.loops).toBe(true);
    // ffmpeg stores 15 fps as delays of 6 and 7 hundredths; the frame rate over the whole GIF is what counts.
    const seconds = info.delays.reduce((a, b) => a + b, 0) / 100;
    expect(info.frames / seconds).toBeGreaterThanOrEqual(GIF.minFps);
    expect(info.frames / seconds).toBeLessThanOrEqual(GIF.maxFps);
    expect(Math.max(...info.delays)).toBeLessThanOrEqual(9);
    expect(seconds).toBeGreaterThanOrEqual(GIF.minSeconds);
    expect(seconds).toBeLessThanOrEqual(GIF.maxSeconds);
  });

  it("dark and light are the same tour: as many frames, as long", () => {
    const [dark, light] = THEMES.map((t) => gifInfo(readFileSync(gif(t))));
    expect(light.frames).toBe(dark.frames);
    expect(light.delays).toEqual(dark.delays);
  });
});

describe("the video left this repository", () => {
  it("git has no video/ folder, and no MP4, WebM or poster of the tour anywhere", () => {
    expect(tracked("video")).toEqual([]);
    expect(tracked().filter((p) => /\.(mp4|webm|mov)$/i.test(p) || /(^|\/)tour-[a-z]+-poster\.png$/.test(p))).toEqual([]);
  });

  it("nothing in git points to the video project, its stills or the renders that left", () => {
    const r = git("grep", "-nE", "(^|[^a-z/])video/(capture|src|out|render\\.mjs|package\\.json|README)|tour-(dark|light)(\\.mp4|\\.webm|-poster\\.png)",
      "--", ".", ":(exclude)scripts/tour-*.test.mjs");
    expect(r.stdout).toBe("");
  });

  it("package.json has no workspaces and no Remotion, and the lock file has nothing from video/", () => {
    const pkg = JSON.parse(readFileSync(join(ROOT, "package.json"), "utf8"));
    expect(pkg.workspaces).toBeUndefined();
    const deps = Object.keys({ ...pkg.dependencies, ...pkg.devDependencies });
    expect(deps.filter((d) => /remotion/.test(d))).toEqual([]);
    expect(JSON.stringify(pkg.scripts)).not.toMatch(/video/);
    const lock = JSON.parse(readFileSync(join(ROOT, "package-lock.json"), "utf8"));
    expect(Object.keys(lock.packages).filter((p) => p.startsWith("video") || /remotion/.test(p))).toEqual([]);
  });

  it("tsc and vitest only look in src/ (and vitest in scripts/)", () => {
    const tsconfig = JSON.parse(readFileSync(join(ROOT, "tsconfig.json"), "utf8"));
    expect(tsconfig.include).toEqual(["src"]);
    const vitest = readFileSync(join(ROOT, "vitest.config.ts"), "utf8");
    const include = [...vitest.match(/include:\s*\[([^\]]*)\]/)[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    expect(include.length).toBeGreaterThan(0);
    for (const pattern of include) expect(pattern).toMatch(/^(src|scripts)\//);
  });
});

describe("the README", () => {
  const readme = readFileSync(join(ROOT, "README.md"), "utf8");
  // the README's first <picture> is the logo; the demo's is the one with the tour
  const picture = (readme.match(/<picture>[\s\S]*?<\/picture>/g) ?? []).find((p) => p.includes("docs/tour/")) ?? "";
  const development = readme.split(/\n## /).find((s) => s.startsWith("Development\n")) ?? "";

  it("shows the tour GIF with GitHub's theme: a <picture> with tour-dark.gif for dark, tour-light.gif for light, an <img> for the rest", () => {
    expect(picture).toMatch(/<source media="\(prefers-color-scheme: dark\)" srcset="docs\/tour\/tour-dark\.gif">/);
    expect(picture).toMatch(/<source media="\(prefers-color-scheme: light\)" srcset="docs\/tour\/tour-light\.gif">/);
    expect(picture).toMatch(/<img alt="[^"]+" src="docs\/tour\/tour-(dark|light)\.gif">/);
    for (const src of picture.match(/docs\/tour\/[^"]+/g)) expect(existsSync(join(ROOT, src))).toBe(true);
  });

  it("its Development section runs the capture with --out, and says the capture makes the stills (by default in .devdata/tour/capture)", () => {
    expect(development).toMatch(/^scripts\/tour-capture\.sh --out <folder> .*default \.devdata\/tour\/capture/m);
    expect(development).toMatch(/The capture makes the product tour's stills/);
  });

  it("its Development section says the video and the README's GIFs are made in the website's repository", () => {
    expect(development).toMatch(/The tour video and the two GIFs above \(`docs\/tour\/tour-dark\.gif` and `tour-light\.gif`\) are made from those stills in the website's repository/);
    expect(development).toMatch(/Remotion/);
  });

  it("names no video/ folder, MP4 or WebM, and the old GIF and its script are gone", () => {
    expect(readme).not.toMatch(/video\/|\.mp4|\.webm|poster/);
    for (const old of ["docs/gizai.gif", "docs/gizai.mp4", "scripts/readme-gif.sh"]) expect(existsSync(join(ROOT, old))).toBe(false);
    expect(readme).not.toMatch(/docs\/gizai\.gif|readme-gif/);
  });
});
