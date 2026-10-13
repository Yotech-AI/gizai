// GizAi's brand artwork (GA-52): the mark, the logo, the favicon and the app icon as SVG files in design/brand/.
// The Figma file "Giz-AI" (page Brandbook) holds the same drawings: they were imported from these files.
// Run it with: node design/brand/gen-brand.mjs
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = dirname(fileURLToPath(import.meta.url));
const PINK = { dark: "#f472b6", light: "#c0287a" }; // --needs: the pink of the board's Review and Deploy glyphs
const INKS = { "on-dark": PINK.dark, "on-light": PINK.light, white: "#ffffff", black: "#000000" };
const TILE = "#15171b"; // --raised (dark): the icon's tile at small sizes
const n = (v) => +v.toFixed(2);

// The mark: the Review glyph (a ring with a dot) drawn as a G. The ring opens at the top right, and a bar carries it
// into the dot in the middle. One closed outline: the outer circle from the top terminal round to the bar, the bar's
// top edge, the dot, the bar's bottom edge, the inner circle back to the terminal.
// R: outer radius; t: the ring's thickness; b: the bar's thickness; d: the dot's radius; term: the top terminal (degrees).
function mark(cx, cy, R, t, b, d, term) {
  const r = R - t, h = b / 2, rad = (term * Math.PI) / 180;
  const p = (rr) => `${n(cx + rr * Math.cos(rad))} ${n(cy - rr * Math.sin(rad))}`;
  const xo = n(cx + Math.sqrt(R * R - h * h)), xd = n(cx + Math.sqrt(d * d - h * h)), xi = n(cx + Math.sqrt(r * r - h * h));
  return `M${p(R)}A${n(R)} ${n(R)} 0 1 0 ${xo} ${n(cy - h)}L${xd} ${n(cy - h)}A${n(d)} ${n(d)} 0 1 0 ${xd} ${n(cy + h)}`
    + `L${xi} ${n(cy + h)}A${n(r)} ${n(r)} 0 1 1 ${p(r)}Z`;
}
// The master's proportions, on its 54-unit box: R 27, ring and bar 9, dot 9.2, terminal at 48°.
const markAt = (cx, cy, R) => mark(cx, cy, R, R / 3, R / 3, (R * 9.2) / 27, 48);

// The wordmark "GizAi": Geist Bold at 100px, letter spacing -2%, outlined in Figma; the i's square dots are round here,
// like the mark's dot. Baseline at y 72.6, cap height 71; the ink runs from x 0 to 241.58.
const WORD = [
  "M32.8 74.2C25.93 74.2 20.07 72.63 15.2 69.5C10.33 66.37 6.57 62.03 3.9 56.5C1.3 50.9 0 44.47 0 37.2C0 30.07 1.33 23.7 4 18.1C6.67 12.5 10.5 8.1 15.5 4.9C20.57 1.63 26.63 0 33.7 0C39.83 0 45 1.03 49.2 3.1C53.47 5.1 56.87 7.97 59.4 11.7C61.93 15.43 63.7 19.83 64.7 24.9L49 25.6C48.27 21.67 46.7 18.57 44.3 16.3C41.97 13.97 38.47 12.8 33.8 12.8C29.73 12.8 26.33 13.87 23.6 16C20.93 18.13 18.93 21.03 17.6 24.7C16.33 28.37 15.7 32.53 15.7 37.2C15.7 41.93 16.33 46.13 17.6 49.8C18.93 53.4 20.93 56.23 23.6 58.3C26.33 60.37 29.83 61.4 34.1 61.4C37.23 61.4 39.93 60.77 42.2 59.5C44.53 58.23 46.33 56.5 47.6 54.3C48.93 52.1 49.73 49.6 50 46.8L33.8 46.8L33.8 35.7L64.9 35.7L64.9 72.6L55.5 72.6L54.7 57.4L56.5 58.2C55.83 61.33 54.37 64.1 52.1 66.5C49.83 68.9 47.03 70.8 43.7 72.2C40.43 73.53 36.8 74.2 32.8 74.2Z",
  "M74.23 72.6L74.23 19L89.23 19L89.23 72.6Z",
  "M99.35 72.6L99.35 60.8L128.85 30.2L100.55 30.2L100.55 19L146.85 19L146.85 30.8L116.85 61.4L147.65 61.4L147.65 72.6Z",
  "M153.13 72.6L178.73 1.6L196.73 1.6L222.33 72.6L206.53 72.6L201.33 57.5L174.03 57.5L168.83 72.6ZM178.33 45L197.13 45L187.73 17.6Z",
  "M226.33 72.6L226.33 19L241.33 19L241.33 72.6Z",
];
const DOT = { r: 7.75, y: 5.6, x: [81.73, 233.83] };
const circle = (cx, cy, r) => `M${n(cx - r)} ${n(cy)}a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0Z`;
// Moves (and scales) absolute path data: every number pair is a point.
function move(d, dx, dy, k = 1) {
  let i = 0;
  return d.replace(/-?\d*\.?\d+/g, (s) => String(n(+s * k + (i++ % 2 ? dy : dx))));
}
const word = (dx, dy) => WORD.map((d) => move(d, dx, dy)).join("") + DOT.x.map((x) => circle(x + dx, DOT.y + dy, DOT.r)).join("");

// Lockups, in the wordmark's units (cap height 71).
// Horizontal: the mark is 90 high (about 1.27 cap heights), centred on the capitals' middle, 27 (0.3 of the mark) before the G.
// Stacked: the mark is 142 high (2 cap heights), centred over the word, 32 above the G.
const LOGO = {
  horizontal: { w: 358.58, h: 90, d: markAt(45, 45, 45) + word(117, 7.9) },
  stacked: { w: 241.58, h: 248.2, d: markAt(120.79, 71, 71) + word(0, 174) },
  mark: { w: 54, h: 54, d: markAt(27, 27, 27) },
};

const svg = (w, h, body, title) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="0 0 ${w} ${h}" role="img" aria-label="${title}">\n<title>${title}</title>\n${body}\n</svg>\n`;

const files = {};
for (const [layout, l] of Object.entries(LOGO)) {
  for (const [ink, hex] of Object.entries(INKS)) {
    const name = layout === "mark" ? `mark-${ink}.svg` : `logo-${layout}-${ink}.svg`;
    files[name] = svg(l.w, l.h, `<path fill="${hex}" d="${l.d}"/>`, "GizAi");
  }
}

// Small icons, drawn on the pixel grid: a dark tile and the pink mark (the favicon, and the app icon at 16 and 32 px).
const SMALL = {
  16: { rx: 3.5, d: mark(8, 8, 7, 2, 2, 2.6, 50) },
  32: { rx: 7, d: mark(16, 16, 12, 4, 4, 4.5, 48) },
  48: { rx: 10.5, d: mark(24, 24, 18, 6, 6, 6.6, 48) },
};
const small = (s) => svg(s, s, `<rect width="${s}" height="${s}" rx="${SMALL[s].rx}" fill="${TILE}"/>\n<path fill="${PINK.dark}" d="${SMALL[s].d}"/>`, "GizAi");
for (const s of [16, 32, 48]) files[`favicon-${s}.svg`] = small(s);
files["favicon.svg"] = small(32);
files["app-icon-16.svg"] = small(16);
files["app-icon-32.svg"] = small(32);

// The app icon's 1024 px master (Linux and Windows): a 896 px tile with smoothed corners (Figma's 60% corner smoothing),
// a dark gradient, a faint inner edge and the pink mark at 60% of the tile.
const SQ = {
  896: "M0 322.56C0 209.65 0 153.2 21.97 110.08C41.3 72.14 72.14 41.3 110.08 21.97C153.2 0 209.65 0 322.56 0L573.44 0C686.35 0 742.8 0 785.92 21.97C823.86 41.3 854.7 72.14 874.03 110.08C896 153.2 896 209.65 896 322.56L896 573.44C896 686.35 896 742.8 874.03 785.92C854.7 823.86 823.86 854.7 785.92 874.03C742.8 896 686.35 896 573.44 896L322.56 896C209.65 896 153.2 896 110.08 874.03C72.14 854.7 41.3 823.86 21.97 785.92C0 742.8 0 686.35 0 573.44Z",
  824: "M0 296.64C0 192.81 0 140.89 20.21 101.23C37.98 66.34 66.34 37.98 101.23 20.21C140.89 0 192.81 0 296.64 0L527.36 0C631.19 0 683.11 0 722.77 20.21C757.66 37.98 786.02 66.34 803.79 101.23C824 140.89 824 192.81 824 296.64L824 527.36C824 631.19 824 683.11 803.79 722.77C786.02 757.66 757.66 786.02 722.77 803.79C683.11 824 631.19 824 527.36 824L296.64 824C192.81 824 140.89 824 101.23 803.79C66.34 786.02 37.98 757.66 20.21 722.77C0 683.11 0 631.19 0 527.36Z",
};
function bigIcon(size, at, id) {
  const c = at + size / 2, k = (size - 8) / size; // the inner edge: the tile shrunk by 4px on each side
  const grad = `<defs><linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#1d1f25"/><stop offset="1" stop-color="#0e0f12"/></linearGradient></defs>`;
  return `${grad}\n<path fill="url(#${id})" d="${move(SQ[size], at, at)}"/>\n`
    + `<path fill="none" stroke="#ffffff" stroke-opacity="0.08" stroke-width="4" d="${move(SQ[size], c - (size * k) / 2, c - (size * k) / 2, k)}"/>\n`
    + `<path fill="${PINK.dark}" d="${markAt(c, c, (size * 0.6) / 2)}"/>`;
}
files["app-icon-1024.svg"] = svg(1024, 1024, bigIcon(896, 64, "gz-tile"), "GizAi");
// macOS: Apple's grid, an 824 px rounded square in the 1024 px canvas, with the soft shadow of the macOS template.
files["app-icon-macos-1024.svg"] = svg(1024, 1024,
  `<path fill="#000000" fill-opacity="0.3" transform="translate(0 10)" filter="url(#gz-shadow)" d="${move(SQ[824], 100, 100)}"/>\n`
  + `<defs><filter id="gz-shadow" x="-10%" y="-10%" width="120%" height="120%"><feGaussianBlur stdDeviation="10"/></filter></defs>\n`
  + bigIcon(824, 100, "gz-tile-mac"), "GizAi");

for (const [name, body] of Object.entries(files)) writeFileSync(join(DIR, name), body);
console.log(Object.keys(files).length + " files: " + Object.keys(files).join(", "));
