// Renders the product tour again into docs/tour/ (GA-149), for each theme:
//   tour-<theme>.mp4         H.264 for the website, under 25 MB
//   tour-<theme>.webm        VP9, under 20 MB
//   tour-<theme>.gif         the README's GIF: 1280 × 832 at 15 fps with ffmpeg's palette, under 10 MB
//   tour-<theme>-poster.png  the first frame
// Remotion renders a master per theme into out/ (2560 × 1664, 30 fps, near lossless), and ffmpeg encodes the MP4 and
// the WebM from it; the GIF is its own cut (TourGif-<theme>: cuts for crossfades), as PNGs at half size.
// Needs ffmpeg on the PATH (for VP9 and the GIF's palette). The first render downloads Chrome Headless Shell.
// usage: npm run render            both themes
//        npm run render -- dark    one theme
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const DOCS = join(HERE, "..", "docs", "tour");
const OUT = join(HERE, "out");
const MB = 1000 * 1000;
/** TourGif-<theme>'s frame rate (GIF_FPS in src/Root.tsx). */
const GIF_FPS = 15;
/** The upper limits from the card, per file. */
const LIMITS = { mp4: 25 * MB, webm: 20 * MB, gif: 10 * MB };
/** What the two-pass encodes aim at: under the limits with room to spare, as they land within a few per cent. */
const AIM = { mp4: 21 * MB, webm: 17 * MB };

const themes = process.argv.slice(2).length ? process.argv.slice(2) : ["dark", "light"];
for (const theme of themes) {
  if (theme !== "dark" && theme !== "light") {
    console.error(`usage: npm run render [-- dark|light]; not ${theme}`);
    process.exit(1);
  }
}
try {
  execFileSync("ffmpeg", ["-version"], { stdio: "ignore" });
} catch {
  console.error("render needs ffmpeg on the PATH, for the WebM and the GIF's palette");
  process.exit(1);
}

const npx = process.platform === "win32" ? "npx.cmd" : "npx";
const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit", cwd: HERE, shell: process.platform === "win32" });
// Headless Chrome now and then crashes while it loads the page ("Page crashed!"); a second try goes through.
const remotion = (...args) => {
  try {
    run(npx, ["remotion", ...args, "--log=warn"]);
  } catch {
    console.log("Remotion failed; trying once more");
    run(npx, ["remotion", ...args, "--log=warn"]);
  }
};
const ffmpeg = (...args) => run("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", ...args]);

mkdirSync(DOCS, { recursive: true });
mkdirSync(OUT, { recursive: true });
const problems = [];

for (const theme of themes) {
  const doc = (ext) => join(DOCS, `tour-${theme}${ext}`);
  const master = join(OUT, `tour-${theme}-master.mp4`);

  console.log(`\n${theme}: the master, ${master}`);
  remotion("render", `Tour-${theme}`, master, "--codec=h264", "--crf=10", "--pixel-format=yuv444p", "--x264-preset=veryfast");

  console.log(`${theme}: the poster`);
  remotion("still", `Tour-${theme}`, doc("-poster.png"), "--frame=0", "--image-format=png");

  // Two passes at the bitrate that fills the aimed size, so the files stay under their limits whatever the scenes hold.
  const seconds = Number(execFileSync("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", master]).toString());
  const rate = (bytes) => `${Math.floor((bytes * 8) / seconds / 1000)}k`;

  console.log(`${theme}: the MP4, ${rate(AIM.mp4)}bit/s`);
  const x264 = ["-an", "-c:v", "libx264", "-preset", "slow", "-b:v", rate(AIM.mp4), "-pix_fmt", "yuv420p",
    "-passlogfile", join(OUT, `x264-${theme}`)];
  ffmpeg("-i", master, ...x264, "-pass", "1", "-f", "null", "-");
  ffmpeg("-i", master, ...x264, "-pass", "2", "-movflags", "+faststart", doc(".mp4"));

  console.log(`${theme}: the WebM, ${rate(AIM.webm)}bit/s`);
  const vp9 = ["-an", "-c:v", "libvpx-vp9", "-b:v", rate(AIM.webm), "-deadline", "good", "-row-mt", "1", "-tile-columns", "2",
    "-auto-alt-ref", "1", "-lag-in-frames", "25", "-pix_fmt", "yuv420p", "-passlogfile", join(OUT, `vp9-${theme}`)];
  ffmpeg("-i", master, ...vp9, "-cpu-used", "4", "-pass", "1", "-f", "null", "-");
  ffmpeg("-i", master, ...vp9, "-cpu-used", "2", "-pass", "2", doc(".webm"));

  console.log(`${theme}: the GIF`);
  const frames = join(OUT, `gif-${theme}`);
  rmSync(frames, { recursive: true, force: true });
  remotion("render", `TourGif-${theme}`, frames, "--sequence", "--image-format=png", "--scale=0.5",
    "--image-sequence-pattern=frame-[frame].[ext]");
  const first = readdirSync(frames).filter((f) => f.endsWith(".png")).sort()[0];
  const digits = first.match(/frame-(\d+)\.png/)[1].length;
  ffmpeg("-framerate", String(GIF_FPS), "-i", join(frames, `frame-%0${digits}d.png`), "-vf",
    "split[a][b];[a]palettegen=max_colors=256:stats_mode=full:reserve_transparent=1[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle",
    "-loop", "0", doc(".gif"));
  rmSync(frames, { recursive: true, force: true });

  for (const [ext, limit] of Object.entries(LIMITS)) {
    const size = statSync(doc(`.${ext}`)).size;
    const line = `tour-${theme}.${ext}: ${(size / MB).toFixed(1)} MB (under ${limit / MB} MB)`;
    console.log(line);
    if (size >= limit) problems.push(line);
  }
}

if (problems.length) {
  console.error(`\nToo big:\n${problems.join("\n")}`);
  process.exit(1);
}
console.log(`\nDone: ${DOCS}`);
