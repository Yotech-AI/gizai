// Tests for the product tour's renders in docs/tour/ (GA-149), which `cd video && npm run render` makes: for dark and
// light an H.264 MP4 under 25 MB, a VP9 WebM under 20 MB, a 1280 × 832 GIF at 12 to 15 fps under 10 MB and the poster.
// The files' headers are read here with no ffprobe, so the tests run wherever `npm test` does. Also: the README shows
// the GIF for GitHub's theme, and the app (its package.json, tsc, vitest and git) leaves video/ alone.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const TOUR = join(ROOT, "docs", "tour");
const MB = 1000 * 1000;
const THEMES = ["dark", "light"];
/** The video's size and the stills', 2560 × 1664 (video/src/scenes.ts), and the card's 45 to 60 seconds (round 2: a shorter tour). */
const VIDEO = { width: 2560, height: 1664, fps: 30, minSeconds: 45, maxSeconds: 60 };

// MP4: the boxes down to the video track's sample description, its media header and its frame times.
function mp4Info(buf) {
  const boxes = (start, end) => {
    const out = [];
    for (let at = start; at + 8 <= end; ) {
      let size = buf.readUInt32BE(at);
      const type = buf.toString("latin1", at + 4, at + 8);
      let header = 8;
      if (size === 1) { size = Number(buf.readBigUInt64BE(at + 8)); header = 16; }
      if (size === 0) size = end - at;
      out.push({ type, body: at + header, end: at + size });
      at += size;
    }
    return out;
  };
  const child = (box, type) => boxes(box.body, box.end).find((b) => b.type === type);
  const moov = boxes(0, buf.length).find((b) => b.type === "moov");
  const header = (box) => {
    const v = buf[box.body];
    return v === 1
      ? { timescale: buf.readUInt32BE(box.body + 20), duration: Number(buf.readBigUInt64BE(box.body + 24)) }
      : { timescale: buf.readUInt32BE(box.body + 12), duration: buf.readUInt32BE(box.body + 16) };
  };
  const tracks = boxes(moov.body, moov.end).filter((b) => b.type === "trak");
  const video = tracks.find((t) => {
    const hdlr = child(child(t, "mdia"), "hdlr");
    return buf.toString("latin1", hdlr.body + 8, hdlr.body + 12) === "vide";
  });
  const mdia = child(video, "mdia");
  const stbl = child(child(mdia, "minf"), "stbl");
  const stsd = child(stbl, "stsd");
  const entry = stsd.body + 8;
  const stts = child(stbl, "stts");
  let frames = 0;
  for (let i = 0, n = buf.readUInt32BE(stts.body + 4); i < n; i++) frames += buf.readUInt32BE(stts.body + 8 + i * 8);
  const media = header(child(mdia, "mdhd"));
  return {
    tracks: tracks.length,
    codec: buf.toString("latin1", entry + 4, entry + 8),
    width: buf.readUInt16BE(entry + 32),
    height: buf.readUInt16BE(entry + 34),
    seconds: media.duration / media.timescale,
    frames,
    faststart: boxes(0, buf.length).findIndex((b) => b.type === "moov") < boxes(0, buf.length).findIndex((b) => b.type === "mdat"),
  };
}

// WebM (Matroska's EBML): the DocType, the Segment's Info (its Duration) and its Tracks, up to the first Cluster.
function webmInfo(buf) {
  const id = (at) => {
    const len = Math.clz32(buf[at]) - 23;
    return { id: buf.readUIntBE(at, len), len };
  };
  const size = (at) => {
    const len = Math.clz32(buf[at]) - 23;
    let value = buf[at] & (0xff >> len);
    for (let i = 1; i < len; i++) value = value * 256 + buf[at + i];
    return { value: value === 2 ** (7 * len) - 1 ? Infinity : value, len };
  };
  const elements = (start, end) => {
    const out = [];
    for (let at = start; at < end; ) {
      const i = id(at), s = size(at + i.len);
      const body = at + i.len + s.len;
      out.push({ id: i.id, body, end: Math.min(end, body + s.value) });
      if (i.id === 0x1f43b675) break; // the first Cluster: the headers are all before it
      at = body + s.value;
    }
    return out;
  };
  const uint = (e) => buf.readUIntBE(e.body, e.end - e.body);
  const find = (list, wanted) => list.find((e) => e.id === wanted);
  const [ebml, segment] = elements(0, buf.length);
  const docType = find(elements(ebml.body, ebml.end), 0x4282);
  const top = elements(segment.body, segment.end);
  const info = elements(find(top, 0x1549a966).body, find(top, 0x1549a966).end);
  const scale = find(info, 0x2ad7b1) ? uint(find(info, 0x2ad7b1)) : 1e6;
  const dur = find(info, 0x4489);
  const durValue = dur.end - dur.body === 8 ? buf.readDoubleBE(dur.body) : buf.readFloatBE(dur.body);
  const tracks = elements(find(top, 0x1654ae6b).body, find(top, 0x1654ae6b).end).filter((e) => e.id === 0xae);
  const entries = tracks.map((t) => elements(t.body, t.end));
  const videoEntry = entries.find((e) => find(e, 0xe0));
  const video = elements(find(videoEntry, 0xe0).body, find(videoEntry, 0xe0).end);
  const codec = find(videoEntry, 0x86);
  return {
    docType: buf.toString("latin1", docType.body, docType.end),
    tracks: tracks.length,
    codec: buf.toString("latin1", codec.body, codec.end),
    width: uint(find(video, 0xb0)),
    height: uint(find(video, 0xba)),
    seconds: (durValue * scale) / 1e9,
  };
}

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

function pngSize(buf) {
  expect(buf.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
  expect(buf.toString("latin1", 12, 16)).toBe("IHDR");
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

const file = (theme, ext) => join(TOUR, `tour-${theme}${ext}`);
const git = (...args) => {
  const r = spawnSync("git", args, { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * MB });
  if (r.status !== 0) throw new Error(`git ${args.join(" ")}: ${r.stderr}`);
  return r.stdout;
};

describe("docs/tour/ holds the tour's renders", () => {
  it("has the MP4, the WebM, the GIF and the poster for dark and for light, and nothing else", () => {
    const expected = THEMES.flatMap((t) => [`tour-${t}.mp4`, `tour-${t}.webm`, `tour-${t}.gif`, `tour-${t}-poster.png`]);
    expect(readdirSync(TOUR).sort()).toEqual(expected.sort());
  });

  describe.each(THEMES)("%s", (theme) => {
    it("the MP4 is H.264, 2560 × 1664 at 30 fps, 45 to 60 s long, under 25 MB, with its index first for the web", () => {
      expect(statSync(file(theme, ".mp4")).size).toBeLessThan(25 * MB);
      const mp4 = mp4Info(readFileSync(file(theme, ".mp4")));
      expect(mp4.codec).toBe("avc1");
      expect(mp4.tracks).toBe(1); // no sound
      expect({ width: mp4.width, height: mp4.height }).toEqual({ width: VIDEO.width, height: VIDEO.height });
      expect(mp4.seconds).toBeGreaterThanOrEqual(VIDEO.minSeconds);
      expect(mp4.seconds).toBeLessThanOrEqual(VIDEO.maxSeconds);
      expect(mp4.frames / mp4.seconds).toBeCloseTo(VIDEO.fps, 1);
      expect(mp4.faststart).toBe(true);
    });

    it("the WebM is VP9, 2560 × 1664, as long as the MP4, under 20 MB", () => {
      expect(statSync(file(theme, ".webm")).size).toBeLessThan(20 * MB);
      const webm = webmInfo(readFileSync(file(theme, ".webm")));
      expect(webm.docType).toBe("webm");
      expect(webm.codec).toBe("V_VP9");
      expect(webm.tracks).toBe(1);
      expect({ width: webm.width, height: webm.height }).toEqual({ width: VIDEO.width, height: VIDEO.height });
      expect(webm.seconds).toBeCloseTo(mp4Info(readFileSync(file(theme, ".mp4"))).seconds, 0);
    });

    it("the GIF is 1280 × 832 at 12 to 15 fps, loops and stays under 10 MB", () => {
      expect(statSync(file(theme, ".gif")).size).toBeLessThan(10 * MB);
      const gif = gifInfo(readFileSync(file(theme, ".gif")));
      expect(gif.signature).toBe("GIF89a");
      expect(gif.ended).toBe(true);
      expect({ width: gif.width, height: gif.height }).toEqual({ width: 1280, height: 832 });
      expect(gif.loops).toBe(true);
      // ffmpeg stores 15 fps as delays of 6 and 7 hundredths; the frame rate over the whole GIF is what counts.
      const seconds = gif.delays.reduce((a, b) => a + b, 0) / 100;
      expect(gif.frames / seconds).toBeGreaterThanOrEqual(12);
      expect(gif.frames / seconds).toBeLessThanOrEqual(15.05);
      expect(Math.max(...gif.delays)).toBeLessThanOrEqual(9);
      expect(seconds).toBeGreaterThan(30);
    });

    it("the poster is a PNG of the video's size", () => {
      expect(pngSize(readFileSync(file(theme, "-poster.png")))).toEqual({ width: VIDEO.width, height: VIDEO.height });
    });
  });
});

describe("the README's demo is the tour GIF, with GitHub's theme", () => {
  const readme = readFileSync(join(ROOT, "README.md"), "utf8");
  // the README's first <picture> is the logo; the demo's is the one with the tour
  const picture = (readme.match(/<picture>[\s\S]*?<\/picture>/g) ?? []).find((p) => p.includes("docs/tour/")) ?? "";

  it("a <picture> picks tour-dark.gif for a dark theme and tour-light.gif for a light one, with an <img> for the rest", () => {
    expect(picture).toMatch(/<source media="\(prefers-color-scheme: dark\)" srcset="docs\/tour\/tour-dark\.gif">/);
    expect(picture).toMatch(/<source media="\(prefers-color-scheme: light\)" srcset="docs\/tour\/tour-light\.gif">/);
    expect(picture).toMatch(/<img alt="[^"]+" src="docs\/tour\/tour-(dark|light)\.gif">/);
    for (const src of picture.match(/docs\/tour\/[^"]+/g)) expect(existsSync(join(ROOT, src))).toBe(true);
  });

  it("the old GIF and its script are gone, and nothing points to them", () => {
    for (const old of ["docs/gizai.gif", "docs/gizai.mp4", "scripts/readme-gif.sh"]) expect(existsSync(join(ROOT, old))).toBe(false);
    expect(readme).not.toMatch(/docs\/gizai\.gif|readme-gif/);
  });
});

describe("the app leaves video/ alone", () => {
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

  it("git has none of video/'s packages, caches or masters", () => {
    const tracked = git("ls-files", "video").split("\n").filter(Boolean);
    expect(tracked).toContain("video/package.json");
    expect(tracked.filter((p) => /(^|\/)(node_modules|\.cache|\.remotion|out)\//.test(p))).toEqual([]);
  });

  it("the captured stills are in git once, in video/capture/", () => {
    const index = git("ls-files", "-s").split("\n").filter(Boolean).map((line) => {
      const [meta, path] = line.split("\t");
      return { sha: meta.split(" ")[1], path };
    });
    const stills = index.filter((f) => f.path.startsWith("video/capture/") && f.path.endsWith(".png"));
    expect(stills.length).toBe(2 * 2 * 15); // a before and an after for 15 steps, in two themes
    const shas = new Set(stills.map((f) => f.sha));
    expect(index.filter((f) => !f.path.startsWith("video/capture/") && shas.has(f.sha)).map((f) => f.path)).toEqual([]);
  });
});
