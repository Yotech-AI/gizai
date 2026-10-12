// Builds design/branding/bottom-band.html: the design of the mountains along the bottom of Gizai's window (GA-139), as one
// page that opens from disk in a browser. It follows design/code-editor/gen-editor.mjs and design/terminal/gen-terminal.mjs:
// the screens are drawn with the app's own stylesheets (src/styles), the band's component in band.css and its tokens in
// band-tokens.css, every screen in dark and then light. Icons are Lucide's own data (lucide-react, ISC). The code in the
// Editor's frame is src/App.tsx, coloured by @lezer/javascript with the app's syn-* tokens.
//
// The artwork is our own: a landscape this file computes from a fixed seed (SEED), so it comes out the same every time.
// A height field (a flat valley between ridged mountains), seen through a perspective camera, drawn as a mesh of
// four-sided cells; the lines a nearer slope hides are taken out (a floating horizon, front to back), so the SVG holds
// only lines you see, in currentColor, and the page and the app give them their colour (the band-* tokens).
// It writes mountains.svg (the app's band) and mountains-hero.svg (the website's hero, the same land seen closer up).
//
// The band's colours live in BAND below, once: this file checks them (contrast of the text over the band, the band apart
// from --live, --needs and --accent) before it writes anything, then writes new-tokens.json (design/tokens.json's format),
// band-tokens.css (the same tokens for this page) and the tables in README.md.
// usage: node design/branding/gen-bottom-band.mjs
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const GZ = path.resolve(HERE, "../..");

// ---------- the land: a seeded height field ----------
export const SEED = 139;
/** mulberry32: a small seeded random number generator, the same numbers on every machine. */
function random(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
/** Gradient noise (Perlin's), its lattice shuffled by the seed: about -0.7 to 0.7, smooth, no repeat in reach. */
function noise(seed) {
  const rnd = random(seed);
  const perm = Array.from({ length: 256 }, (_, i) => i);
  for (let i = 255; i > 0; i--) { const j = Math.floor(rnd() * (i + 1)); [perm[i], perm[j]] = [perm[j], perm[i]]; }
  const p = new Uint8Array(512);
  for (let i = 0; i < 512; i++) p[i] = perm[i & 255];
  const gx = new Float64Array(256), gy = new Float64Array(256);
  for (let i = 0; i < 256; i++) { const a = rnd() * Math.PI * 2; gx[i] = Math.cos(a); gy[i] = Math.sin(a); }
  const fade = (t) => t * t * t * (t * (t * 6 - 15) + 10);
  return (x, y) => {
    const xi = Math.floor(x), yi = Math.floor(y), xf = x - xi, yf = y - yi;
    const g = (i, j, dx, dy) => { const h = p[(p[(xi + i) & 255] + yi + j) & 255]; return gx[h] * dx + gy[h] * dy; };
    const u = fade(xf), v = fade(yf);
    const a = g(0, 0, xf, yf) + u * (g(1, 0, xf - 1, yf) - g(0, 0, xf, yf));
    const b = g(0, 1, xf, yf - 1) + u * (g(1, 1, xf - 1, yf - 1) - g(0, 1, xf, yf - 1));
    return a + v * (b - a);
  };
}
const smooth = (a, b, x) => { const t = Math.min(1, Math.max(0, (x - a) / (b - a))); return t * t * (3 - 2 * t); };
const N1 = noise(SEED), N2 = noise(SEED * 7 + 3), N3 = noise(SEED * 13 + 5);
/** Ridged noise: sharp crests where the noise crosses zero, octaves weighted by the one before (Musgrave's multifractal). */
function ridged(x, y) {
  let sum = 0, amp = 1, freq = 1, weight = 1, norm = 0;
  for (let o = 0; o < 6; o++) {
    let r = 1 - Math.abs(N1(x * freq + o * 17.3, y * freq - o * 9.1) * 1.45);
    r = Math.max(0, r) ** 2 * weight;
    weight = Math.min(1, r * 1.8);
    sum += r * amp; norm += amp;
    amp *= 0.52; freq *= 2.07;
  }
  return sum / norm;
}
// World units: 1 is 1px of the band where it meets the bottom edge (depth F). X runs across, Z into the distance, h up.
export const LAND = {
  valley: 150,   // half the valley's width: a flat floor between the mountains, as wide as the narrowest window shows
  rise: 190,     // the slopes rise over this much on either side of it
  height: 330,   // the tallest mountains, before the ceiling softens them
  wave: 560,     // the mountains' size
};
/** How high the land may reach at depth Z and still show whole in the picture: the eye's height plus what lies between
 *  the horizon and the top edge. The cameras below share it (the hero is the band seen closer up). */
const ceiling = (Z, F) => 88 + 34 * (Z / F);
/** The land's height at (X, Z): 0 on the valley floor, mountains beside it, foothills near the camera. */
export function height(X, Z, F) {
  const ax = Math.abs(X);
  const side = smooth(LAND.valley, LAND.valley + LAND.rise, ax);
  if (side === 0) return 0;
  // ranges come and go along the band, taller near the valley, so every window width shows peaks on both sides
  const range = 0.3 + 0.7 * smooth(-0.32, 0.38, N2(X / 1500, Z / 2200 + 3.7));
  const near = 1 + 0.6 * Math.exp(-(((ax - 470) / 330) ** 2));
  const front = smooth(0.86 * F, 1.32 * F, Z);
  const r = ridged(X / LAND.wave, Z / LAND.wave) ** 1.55;
  const h = LAND.height * side * range * near * front * r * (0.85 + 0.15 * N3(X / 230, Z / 230));
  const top = ceiling(Z, F);
  return top * Math.tanh(h / top);
}

// ---------- the camera and the mesh ----------
/** A camera: the picture W × H px, the horizon at yh, the eye E world units above the floor, s px per world unit at the
 *  bottom edge (depth F), rows from Z0 to Zfar (each q times farther), columns dx apart, lines fading with distance. */
export const BAND_CAMERA = { W: 4800, H: 128, yh: 40, E: 88, s: 1, F: 400, Z0: 0.86, Zfar: 8, q: 1.14, dx: 27, margin: 40 };
export const HERO_CAMERA = { W: 3840, H: 448, yh: 140, E: 88, s: 3.5, F: 400, Z0: 0.86, Zfar: 8, q: 1.062, dx: 10.8, margin: 60 };

function project(cam, X, Z, h) {
  const k = (cam.s * cam.F) / Z;
  return [cam.W / 2 + X * k, cam.yh + (cam.E - h) * k];
}
/** How strong a line at depth Z is: full near the camera, fading to the far end of the land. */
export function depthFade(cam, Z) {
  const t = Math.min(1, Math.max(0, Math.log(Z / (1.25 * cam.F)) / Math.log(cam.Zfar / 1.25)));
  return 1 - 0.95 * t ** 0.75;
}

/** The mesh's rows, nearest first: each the land's points across the picture, with its depth. */
function rows(cam) {
  const out = [];
  for (let Z = cam.Z0 * cam.F; Z <= cam.Zfar * cam.F * 1.0001; Z *= cam.q) {
    const reach = ((cam.W / 2 + cam.margin) * Z) / (cam.s * cam.F);
    const j0 = Math.floor(-reach / cam.dx), j1 = Math.ceil(reach / cam.dx);
    const pts = [];
    for (let j = j0; j <= j1; j++) { const X = j * cam.dx; pts.push(project(cam, X, Z, height(X, Z, cam.F))); }
    out.push({ Z, j0, pts });
  }
  return out;
}

// ---------- hidden lines: a floating horizon, the highest line drawn so far at each x (in quarter pixels) ----------
const EPS = 0.5;
function horizon(cam) {
  const R = 4, n = Math.ceil((cam.W + 2 * cam.margin) * R) + 2, off = cam.margin;
  const hb = new Float64Array(n).fill(Infinity);
  const at = (x) => {
    const f = (x + off) * R;
    if (f <= 0 || f >= n - 1) return Infinity;
    const i = Math.floor(f), t = f - i, a = hb[i], b = hb[i + 1];
    if (a === Infinity || b === Infinity) return Infinity;
    return a + (b - a) * t;
  };
  const add = ([x1, y1], [x2, y2]) => {
    const lo = Math.max(0, Math.ceil((Math.min(x1, x2) + off) * R)), hi = Math.min(n - 1, Math.floor((Math.max(x1, x2) + off) * R));
    for (let i = lo; i <= hi; i++) {
      const x = i / R - off, y = x2 === x1 ? Math.min(y1, y2) : y1 + ((y2 - y1) * (x - x1)) / (x2 - x1);
      if (y < hb[i]) hb[i] = y;
    }
  };
  return { at, add };
}
/** The parts of a segment above the horizon: [t0, t1] pieces of it, found by sampling and refined by halving. */
function visibleParts(a, b, hz) {
  const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
  const n = Math.max(2, Math.ceil(len * 2));
  const pt = (t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
  const vis = (t) => { const [x, y] = pt(t); return y <= hz.at(x) + EPS; };
  const parts = [];
  let prevT = 0, prevV = vis(0), start = prevV ? 0 : null;
  for (let i = 1; i <= n; i++) {
    const t = i / n, v = vis(t);
    if (v !== prevV) {
      let lo = prevT, hi = t;
      for (let k = 0; k < 10; k++) { const m = (lo + hi) / 2; if (vis(m) === prevV) lo = m; else hi = m; }
      const edge = (lo + hi) / 2;
      if (v) start = edge; else { parts.push([start, edge]); start = null; }
    }
    prevT = t; prevV = v;
  }
  if (start !== null) parts.push([start, 1]);
  return parts.filter(([s, e]) => (e - s) * len > 0.05);
}
/** Liang–Barsky: the segment cut to the picture, or null. */
function clip(cam, a, b) {
  let t0 = 0, t1 = 1;
  const dx = b[0] - a[0], dy = b[1] - a[1];
  for (const [p, q] of [[-dx, a[0]], [dx, cam.W - a[0]], [-dy, a[1]], [dy, cam.H - a[1]]]) {
    if (p === 0) { if (q < 0) return null; continue; }
    const r = q / p;
    if (p < 0) { if (r > t1) return null; if (r > t0) t0 = r; } else { if (r < t0) return null; if (r < t1) t1 = r; }
  }
  return [[a[0] + dx * t0, a[1] + dy * t0], [a[0] + dx * t1, a[1] + dy * t1]];
}

/** The visible lines, as polylines grouped by how strong they are (their depth): [{ fade, lines: [[x, y], ...][] }] */
export function draw(cam, { levels = 7 } = {}) {
  const rs = rows(cam), hz = horizon(cam);
  const groups = new Map();
  const level = (Z) => Math.round(depthFade(cam, Z) * (levels - 1)) / (levels - 1);
  const put = (Z, a, b, kind) => {
    const c = clip(cam, a, b);
    if (!c) return;
    const f = level(Z);
    if (!groups.has(f)) groups.set(f, { row: [], col: [] });
    groups.get(f)[kind].push(c);
  };
  const piecesOf = (Z, a, b, kind) => {
    for (const [s, e] of visibleParts(a, b, hz)) {
      const p = (t) => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
      put(Z, p(s), p(e), kind);
    }
  };
  rs.forEach((r, k) => {
    if (k > 0) {
      const prev = rs[k - 1];
      // the columns between the row in front and this one, then this row, both against what is in front of them
      for (let i = 0; i < prev.pts.length; i++) {
        const j = prev.j0 + i, here = j - r.j0;
        if (here >= 0 && here < r.pts.length) piecesOf(r.Z, prev.pts[i], r.pts[here], "col");
      }
    }
    for (let i = 1; i < r.pts.length; i++) {
      if (k === 0) put(r.Z, r.pts[i - 1], r.pts[i], "row"); else piecesOf(r.Z, r.pts[i - 1], r.pts[i], "row");
    }
    for (let i = 1; i < r.pts.length; i++) hz.add(r.pts[i - 1], r.pts[i]);
    if (k > 0) {
      const prev = rs[k - 1];
      for (let i = 0; i < prev.pts.length; i++) { const here = prev.j0 + i - r.j0; if (here >= 0 && here < r.pts.length) hz.add(prev.pts[i], r.pts[here]); }
    }
  });
  // join pieces that meet into polylines, then drop the points a straight line doesn't need
  const out = [];
  for (const [fade, g] of [...groups.entries()].sort((a, b) => b[0] - a[0])) {
    out.push({ fade, lines: [...chain(g.row), ...chain(g.col)].map((l) => simplify(l, 0.12)).filter((l) => length(l) > 0.6) });
  }
  return out;
}
const same = (p, q) => Math.abs(p[0] - q[0]) < 1e-6 && Math.abs(p[1] - q[1]) < 1e-6;
function chain(segs) {
  const lines = [];
  const byStart = new Map();
  const key = (p) => `${p[0].toFixed(5)},${p[1].toFixed(5)}`;
  segs.forEach((s, i) => { const k = key(s[0]); if (!byStart.has(k)) byStart.set(k, []); byStart.get(k).push(i); });
  const used = new Uint8Array(segs.length), ends = new Set(segs.map((s) => key(s[1])));
  const walk = (i) => {
    const line = [segs[i][0], segs[i][1]];
    used[i] = 1;
    for (;;) {
      const next = (byStart.get(key(line[line.length - 1])) ?? []).find((n) => !used[n] && same(segs[n][0], line[line.length - 1]));
      if (next === undefined) break;
      used[next] = 1; line.push(segs[next][1]);
    }
    return line;
  };
  segs.forEach((s, i) => { if (!used[i] && !ends.has(key(s[0]))) lines.push(walk(i)); });
  segs.forEach((s, i) => { if (!used[i]) lines.push(walk(i)); });
  return lines;
}
function simplify(pts, tol) {
  if (pts.length < 3) return pts;
  const keep = new Uint8Array(pts.length); keep[0] = keep[pts.length - 1] = 1;
  const stack = [[0, pts.length - 1]];
  while (stack.length) {
    const [i, j] = stack.pop();
    const [ax, ay] = pts[i], [bx, by] = pts[j], L = Math.hypot(bx - ax, by - ay) || 1e-9;
    let best = -1, far = tol;
    for (let k = i + 1; k < j; k++) { const d = Math.abs((bx - ax) * (ay - pts[k][1]) - (ax - pts[k][0]) * (by - ay)) / L; if (d > far) { far = d; best = k; } }
    if (best > 0) { keep[best] = 1; stack.push([i, best], [best, j]); }
  }
  return pts.filter((_, i) => keep[i]);
}
const length = (l) => l.slice(1).reduce((s, p, i) => s + Math.hypot(p[0] - l[i][0], p[1] - l[i][1]), 0);

// ---------- the SVG: integers in quarter pixels, relative steps, one path per strength of line ----------
const Q = 4;
function pathData(lines) {
  let d = "", cx = 0, cy = 0, first = true;
  const num = (n, lead) => (n < 0 ? `${n}` : lead ? ` ${n}` : `${n}`);
  for (const l of lines) {
    const pts = l.map(([x, y]) => [Math.round(x * Q), Math.round(y * Q)]).filter((p, i, a) => i === 0 || p[0] !== a[i - 1][0] || p[1] !== a[i - 1][1]);
    if (pts.length < 2) continue;
    const [x0, y0] = pts[0];
    d += first ? `M${x0}${num(y0, true)}` : `m${x0 - cx}${num(y0 - cy, true)}`;
    first = false;
    d += "l";
    pts.slice(1).forEach(([x, y], i) => { const dx = x - pts[i][0], dy = y - pts[i][1]; d += `${i ? num(dx, true) : `${dx}`}${num(dy, true)}`; });
    [cx, cy] = pts[pts.length - 1];
  }
  return d;
}
/** The artwork as SVG. `id` prefixes its one gradient and mask, so two of them can share a page. With `symbol`, the
 *  inner part only, for a <symbol>. The lines are currentColor; `color` is only for opening the file on its own. */
export function svg(cam, { id, symbol = false, color = "#8fb7d3", top = 0.3, groups = draw(cam) } = {}) {
  const W = cam.W * Q, H = cam.H * Q, f = (n) => String(Math.round(n * 1000) / 1000).replace(/^0\./, ".");
  const paths = groups.map((g) => `<path vector-effect="non-scaling-stroke"${g.fade < 1 ? ` stroke-opacity="${f(g.fade)}"` : ""} d="${pathData(g.lines)}"/>`).join("\n");
  // the top of the picture fades, so the peaks melt into what is above the band
  const defs = `<defs><linearGradient id="${id}-fade" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="${f(top)}"/><stop offset=".5" stop-color="#fff"/></linearGradient>`
    + `<mask id="${id}-mask" maskUnits="userSpaceOnUse" x="0" y="0" width="${W}" height="${H}"><rect width="${W}" height="${H}" fill="url(#${id}-fade)"/></mask></defs>`;
  const inner = `${defs}\n<g mask="url(#${id}-mask)" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round">\n${paths}\n</g>`;
  if (symbol) return inner;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" preserveAspectRatio="xMidYMax slice" color="${color}" aria-hidden="true">\n`
    + `<!-- Gizai's mountains (GA-139): our own land from seed ${SEED}, made by design/branding/gen-bottom-band.mjs. The lines are currentColor. -->\n${inner}\n</svg>\n`;
}
export const stats = (groups) => ({ lines: groups.reduce((s, g) => s + g.lines.length, 0), points: groups.reduce((s, g) => s + g.lines.reduce((t, l) => t + l.length, 0), 0) });


// ---------- colours: contrast (WCAG 2), OKLCH and CIEDE2000, for the checks and the tables ----------
const hex2rgb = (h) => { const n = parseInt(h.slice(1), 16); return [n >> 16, (n >> 8) & 255, n & 255]; };
const toHex = (c) => `#${c.map((v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, "0")).join("")}`;
const lin = (v) => { v /= 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
const lum = (c) => 0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2]);
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, s) => s - p); return (x + 0.05) / (y + 0.05); };
/** fg at alpha a over bg, in sRGB, as a browser composites. */
const over = (fg, a, bg) => fg.map((c, i) => c * a + bg[i] * (1 - a));
/** OKLCH (Björn Ottosson's OKLab): lightness, chroma, hue in degrees. */
function oklch(hex) {
  const [r, g, b] = hex2rgb(hex).map(lin);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  const A = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, B = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s, Math.hypot(A, B), ((Math.atan2(B, A) * 180) / Math.PI + 360) % 360];
}
function fromOklch(L, C, H) {
  const a = C * Math.cos((H * Math.PI) / 180), b = C * Math.sin((H * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3, m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3, s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const enc = (v) => { v = Math.min(1, Math.max(0, v)); return (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055) * 255; };
  return toHex([4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s, -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s, -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s].map(enc));
}
const hueGap = (a, b) => { const d = Math.abs(oklch(a)[2] - oklch(b)[2]) % 360; return d > 180 ? 360 - d : d; };
function lab(hex) {
  const [r, g, b] = hex2rgb(hex).map(lin);
  const f = (x) => (x > 216 / 24389 ? Math.cbrt(x) : (24389 / 27 * x + 16) / 116);
  const [X, Y, Z] = [f((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047), f(0.2126 * r + 0.7152 * g + 0.0722 * b), f((0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883)];
  return [116 * Y - 16, 500 * (X - Y), 200 * (Y - Z)];
}
/** CIEDE2000 colour difference. */
function deltaE(h1, h2) {
  const [L1, a1, b1] = lab(h1), [L2, a2, b2] = lab(h2), rad = Math.PI / 180, deg = 180 / Math.PI;
  const Cb = (Math.hypot(a1, b1) + Math.hypot(a2, b2)) / 2, G = 0.5 * (1 - Math.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)));
  const a1p = (1 + G) * a1, a2p = (1 + G) * a2, C1 = Math.hypot(a1p, b1), C2 = Math.hypot(a2p, b2);
  const hp1 = (Math.atan2(b1, a1p) * deg + 360) % 360, hp2 = (Math.atan2(b2, a2p) * deg + 360) % 360;
  let dh = hp2 - hp1; if (C1 * C2 === 0) dh = 0; else if (dh > 180) dh -= 360; else if (dh < -180) dh += 360;
  const dH = 2 * Math.sqrt(C1 * C2) * Math.sin((dh / 2) * rad), Lb = (L1 + L2) / 2, Cbp = (C1 + C2) / 2;
  let hb = hp1 + hp2; if (C1 * C2 !== 0) { if (Math.abs(hp1 - hp2) > 180) hb += hp1 + hp2 < 360 ? 360 : -360; hb /= 2; }
  const T = 1 - 0.17 * Math.cos((hb - 30) * rad) + 0.24 * Math.cos(2 * hb * rad) + 0.32 * Math.cos((3 * hb + 6) * rad) - 0.2 * Math.cos((4 * hb - 63) * rad);
  const Rc = 2 * Math.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7)), dTh = 30 * Math.exp(-(((hb - 275) / 25) ** 2));
  const Sl = 1 + (0.015 * (Lb - 50) ** 2) / Math.sqrt(20 + (Lb - 50) ** 2), Sc = 1 + 0.045 * Cbp, Sh = 1 + 0.015 * Cbp * T;
  const dC = C2 - C1, dL = L2 - L1;
  return Math.sqrt((dL / Sl) ** 2 + (dC / Sc) ** 2 + (dH / Sh) ** 2 - Math.sin(2 * dTh * rad) * Rc * (dC / Sc) * (dH / Sh));
}
function themeVars(file) {
  const css = fs.readFileSync(file, "utf8");
  const block = (sel) => { const i = css.indexOf(sel); return css.slice(css.indexOf("{", i) + 1, css.indexOf("}", i)); };
  const vars = (text) => Object.fromEntries([...text.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
  return { dark: vars(block(':root, [data-theme="dark"] {')), light: vars(block('[data-theme="light"] {')) };
}
const APP = themeVars(`${GZ}/src/styles/tokens.css`);
const THEMES = ["dark", "light"];

// ---------- the band's colours: one list, checked, then written to new-tokens.json, band-tokens.css and README.md ----------
// One cool blue, hue 228° in OKLCH: about 40° from teal (live, 187°) and from the accent's blue (266°), and quieter (less
// chroma) than either, so the band reads as the window's own light, not as an agent at work, "needs you" or a control.
// BASE is each theme's line and glow colour, opaque; a strength gives each its alpha. The glow is band-glow at the bottom
// edge, fading to nothing at the band's top.
export const HUE = 228;
const BASE = {
  dark: { line: fromOklch(0.84, 0.062, HUE), glow: fromOklch(0.47, 0.085, HUE + 4) },
  light: { line: fromOklch(0.5, 0.085, HUE + 4), glow: fromOklch(0.9, 0.032, HUE) },
};
export const STRENGTHS = [
  { key: "subtle", name: "Subtle", what: "About as much as Omarchy's see-through shows now: there when you look for it.",
    alpha: { dark: { line: 0.1, glow: 0.08 }, light: { line: 0.14, glow: 0.35 } } },
  { key: "medium", name: "Medium", what: "Clearly there, never loud: the lines read at a glance, the glow tints the bottom edge.",
    alpha: { dark: { line: 0.17, glow: 0.15 }, light: { line: 0.24, glow: 0.65 } } },
  { key: "strong", name: "Strong", what: "The bright strip of Jefsev's screenshot: the band is the first thing you see at the bottom.",
    alpha: { dark: { line: 0.32, glow: 0.3 }, light: { line: 0.4, glow: 1 } } },
];
/** The design's proposal, until Jefsev picks: band-line and band-glow follow it. */
export const PICK = "medium";
const rgba = (hex, a) => `rgba(${hex2rgb(hex).join(", ")}, ${a.toFixed(2)})`;
/** Each strength's two tokens per theme. */
const tokenOf = (s, theme) => ({ line: rgba(BASE[theme].line, s.alpha[theme].line), glow: rgba(BASE[theme].glow, s.alpha[theme].glow) });

// What the text on the canvases keeps over the band: at its bottom edge, where it is strongest, between the lines (the
// glow over the surface) and on a line (a line over that). The canvases are bg and side; text-3 is the faintest text.
// Subtle and medium keep all three at 4.5:1 between the lines, text and text-2 at 4.5:1 on a line too, and text-3 at 3:1 there.
// Strong keeps text and text-2 at 4.5:1 between the lines and text on a line; text-3 drops to about 4:1 at the bottom edge.
const RULES = {
  subtle: { between: { text: 4.5, "text-2": 4.5, "text-3": 4.5 }, on: { text: 4.5, "text-2": 4.5, "text-3": 3 } },
  medium: { between: { text: 4.5, "text-2": 4.5, "text-3": 4.5 }, on: { text: 4.5, "text-2": 4.5, "text-3": 3 } },
  strong: { between: { text: 4.5, "text-2": 4.5, "text-3": 3 }, on: { text: 4.5 } },
};
const TEXTS = ["text", "text-2", "text-3"];
export const CONTRAST = [];
for (const s of STRENGTHS) for (const theme of THEMES) for (const surface of ["bg", "side"]) {
  const base = hex2rgb(APP[theme][surface]);
  const between = over(hex2rgb(BASE[theme].glow), s.alpha[theme].glow, base);
  const on = over(hex2rgb(BASE[theme].line), s.alpha[theme].line, between);
  const c = Object.fromEntries(TEXTS.map((k) => [k, { between: contrast(hex2rgb(APP[theme][k]), between), on: contrast(hex2rgb(APP[theme][k]), on) }]));
  CONTRAST.push({ strength: s.key, theme, surface, between: toHex(between), on: toHex(on), c });
}

// The checks: nothing is written when one fails.
const problems = [];
for (const theme of THEMES) for (const [what, hex] of Object.entries(BASE[theme])) {
  for (const state of ["live", "needs"]) {
    const g = hueGap(hex, APP[theme][state]), d = deltaE(hex, APP[theme][state]);
    if (g < 35 || d < 15) problems.push(`${theme}: the band's ${what} (${hex}) is too near ${state} (${g.toFixed(0)}°, ΔE ${d.toFixed(1)})`);
  }
  if (hueGap(hex, APP[theme].accent) < 30) problems.push(`${theme}: the band's ${what} (${hex}) is too near the accent (${hueGap(hex, APP[theme].accent).toFixed(0)}°)`);
}
for (const r of CONTRAST) for (const where of ["between", "on"]) for (const [k, min] of Object.entries(RULES[r.strength][where])) {
  if (r.c[k][where] < min) problems.push(`${r.strength}, ${r.theme}, on ${r.surface}: ${k} is ${r.c[k][where].toFixed(2)}:1 ${where === "on" ? "on a line" : "between the lines"}, under ${min}:1`);
}
export const PROBLEMS = problems;
/** For the tables: how far each of the band's colours is from live, needs and the accent. */
export const HUES = THEMES.flatMap((theme) => Object.entries(BASE[theme]).map(([what, hex]) => ({ theme, what, hex, hue: oklch(hex)[2],
  live: [hueGap(hex, APP[theme].live), deltaE(hex, APP[theme].live)], needs: [hueGap(hex, APP[theme].needs), deltaE(hex, APP[theme].needs)], accent: [hueGap(hex, APP[theme].accent), deltaE(hex, APP[theme].accent)] })));

// ---------- the page: markup helpers ----------
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const q = (s) => esc(s).replace(/"/g, "&quot;");
const LUCIDE = `${GZ}/node_modules/lucide-react/dist/esm/icons`;
// icons: Lucide's own data, as one sprite of the icons the page uses
const used = new Set();
const ic = (name, cls = "icon") => { used.add(name); return `<svg class="${cls}" aria-hidden="true"><use href="#i-${name}"/></svg>`; };
const sm = (name) => ic(name, "icon sm");
async function iconNode(name) {
  const file = `${LUCIDE}/${name}.js`;
  if (!fs.existsSync(file)) throw new Error(`no Lucide icon ${name}`);
  const m = await import(pathToFileURL(file).href);
  if (m.__iconNode) return m.__iconNode;
  const alias = fs.readFileSync(file, "utf8").match(/from '\.\/(.+?)\.js'/);
  if (alias) return iconNode(alias[1]);
  throw new Error(`no icon data in ${name}`);
}
const attrs = (o) => Object.entries(o).filter(([k]) => k !== "key").map(([k, v]) => `${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())}="${v}"`).join(" ");
async function sprite() {
  const out = [];
  for (const name of [...used].sort()) out.push(`<symbol id="i-${name}" viewBox="0 0 24 24">${(await iconNode(name)).map(([tag, a]) => `<${tag} ${attrs(a)}/>`).join("")}</symbol>`);
  return out.join("\n");
}
// Status glyphs: Gizai's own, the paths of src/components/StatusIcon.tsx
const GLYPH = {
  backlog: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6" stroke-dasharray="2.4 2.1"/>',
  ready: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/>',
  in_progress: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 4.25a3.75 3.75 0 0 1 0 7.5z" fill="currentColor"/>',
  testing: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 8V4.25a3.75 3.75 0 1 1-3.75 3.75z" fill="currentColor"/>',
  review: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="8" r="2.4" fill="currentColor"/>',
  done: '<circle cx="8" cy="8" r="7" fill="currentColor"/><path d="m5.1 8.2 2 1.9 3.8-3.9" style="stroke:var(--bg)" stroke-width="1.7" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
};
const ST_NAME = { backlog: "Backlog", ready: "To do", in_progress: "In progress", testing: "Testing", review: "Review", done: "Done" };
const st = (cat) => `<svg class="st st-${cat}" viewBox="0 0 16 16" fill="none" role="img" aria-label="${ST_NAME[cat]}">${GLYPH[cat]}</svg>`;
const kbd = (k) => `<span class="kbd">${k}</span>`;
const pri = (p) => p === "urgent" ? '<span class="pri urgent" title="Urgent">!</span>' : `<span class="pri ${p}" title="${p[0].toUpperCase()}${p.slice(1)}"><i></i><i></i><i></i></span>`;
const LABELS = { Feature: "c-blue", Bug: "c-red", Backend: "c-orange", Frontend: "c-violet" };
const label = (name) => `<span class="label-pill"><span class="dot" style="background:var(--${LABELS[name]})"></span>${name}</span>`;
const AGENTS = { BA: "Backend Agent", FA: "Frontend Agent", QA: "QA Agent", TL: "Team Lead" };
const avatar = (who, size = "sm") => AGENTS[who] ? `<span class="avatar ${size} agent" title="${AGENTS[who]}">${who}</span>` : `<span class="avatar ${size}" title="Jeffrey Sevinga">JS</span>`;
const topbar = (crumbs, actions = "", mid = "") => `<div class="topbar"><div class="crumbs">${crumbs}</div>${mid}${actions ? `<div class="actions">${actions}</div>` : ""}</div>`;

// ---------- the sidebar, as on main today; `on` is the page it shows ----------
const nav = (icon, text, { meta = "", on = false, tag = "a", title = "" } = {}) =>
  `<${tag} class="nav-item${on ? " on" : ""}"${title ? ` title="${q(title)}"` : ""}>${ic(icon)}<span>${text}</span>${meta ? `<span class="meta">${meta}</span>` : ""}</${tag}>`;
const liveTag = (n) => `<span class="live-tag"><span class="pulse"></span>${n} live</span>`;
const fold = (name, open, body = "", extra = "") => `<div class="nav-section" aria-label="${name}"><div class="nav-label"><span class="fold">${name}</span>${extra}`
  + `<button aria-expanded="${open}" aria-label="${open ? "Fold" : "Show"} ${name}" title="${open ? "Fold" : "Show"} ${name}">${sm(open ? "chevron-down" : "chevron-up")}</button></div>${open ? `<div class="nav-list">${body}</div>` : ""}</div>`;
function sidebar(on) {
  const top = nav("square-pen", "New task", { tag: "button", meta: kbd("N") }) + nav("search", "Search", { tag: "button", meta: kbd("Ctrl K") })
    + nav("inbox", "Inbox", { meta: '<span class="count alert">2</span>' });
  const work = `<div class="nav-section"><div class="nav-label">Work</div>${nav("messages-square", "Chat", { on: on === "chat" })}${nav("list-todo", "Tasks", { on: on === "tasks" })}${nav("building-2", "Clients")}${nav("folder-kanban", "Projects")}</div>`;
  const dot = (c) => `<span class="dot" style="background:var(--${c})"></span>`;
  const projects = `<div class="nav-section"><div class="nav-label">Projects<button aria-label="New project" title="New project">${sm("plus")}</button></div>`
    + `<a class="nav-item">${dot("c-blue")}<span>Gizai</span></a><a class="nav-item">${dot("c-orange")}<span>Kade portal</span></a><a class="nav-item">${dot("c-violet")}<span>Gizai site</span></a></div>`;
  const editor = fold("Editor", true,
    `<a class="nav-item${on === "editor" ? " on" : ""}" title="~/Herd/gizai · main, up to date with origin/main · 2 files changed, not committed">${ic("folder-code")}<span>gizai</span><span class="meta"><span class="stand">2 changed</span></span></a>`
    + `<a class="nav-item" title="~/Herd/kade-portal">${ic("folder-code")}<span>kade-portal</span></a>`,
    `<button aria-label="Open a folder" title="Open a folder">${sm("plus")}</button>`);
  const agents = fold("Agents", true, nav("crown", "Team Lead") + nav("monitor", "Frontend Agent") + nav("server", "Backend Agent", { meta: liveTag(1) })
    + nav("flask-conical", "QA Agent", { meta: liveTag(1) }) + nav("palette", "Design Agent"));
  const memory = fold("Memory", false);
  const foot = `<div class="side-foot"><div class="nav-section"><div class="nav-label">Company</div>${nav("chart-column", "Usage")}${nav("network", "Team")}${nav("users", "Users")}${nav("settings", "Settings", { on: on === "settings" })}</div></div>`;
  return `<aside class="side" aria-label="Navigation"><div class="side-main">${top}${work}${projects}${editor}${agents}${memory}</div>${foot}</aside>`;
}

// ---------- the board: Kade portal's tasks ----------
const card = ({ id, title, p, labels = [], who, working, hand }) => `<div class="card"><div class="top"><span class="id">${id}</span>${pri(p)}</div><div class="title">${title}</div>`
  + `${working ? `<div class="working"><span class="pulse" aria-hidden="true"></span>${AGENTS[working]} is working</div>` : ""}`
  + `<div class="meta">${labels.map(label).join("")}${hand ? `<span class="badge by-hand" title="Pinned to Jeffrey Sevinga: agents leave it alone until it's handed over">${sm("keyboard")}By hand</span>` : ""}${who ? avatar(who) : ""}</div></div>`;
const column = (cat, name, cards, { note = "", more = "", n = cards.length } = {}) => `<div class="col"><div class="col-head">${st(cat)}<span class="name">${name}</span><span class="n">${n}</span>`
  + `<span class="add"><button class="btn ghost sm icon-only" aria-label="New task in ${name}" title="New task in ${name}">${ic("plus")}</button></span></div>`
  + `${note ? `<div class="col-note">${note}</div>` : ""}<div class="col-body">${cards.map(card).join("")}${more ? `<button class="show-more">${more}</button>` : ""}</div></div>`;
const BOARD = `<div class="main">${topbar(`<a>Kade portal</a><span class="sep">/</span><b>Tasks</b><span class="faint">25</span>`)}`
  + `<div class="toolbar"><button class="btn">${ic("plus")}New task</button><input class="input search-input" type="search" aria-label="Search tasks" placeholder="Search ID or title"><span class="spacer"></span>`
  + `<div class="seg" role="group" aria-label="View"><button aria-pressed="true" aria-label="Board" title="Board (Ctrl+B)">${ic("columns-3")}</button><button aria-pressed="false" aria-label="List" title="List (Ctrl+B)">${ic("list")}</button></div>`
  + `<button class="btn ghost">${ic("list-filter")}Filters</button><button class="btn ghost">${ic("arrow-up-down")}Sort</button></div>`
  + `<div class="board-wrap"><div class="board">`
  + `<div class="col rail" title="Show Backlog"><div class="col-head">${st("backlog")} <span class="name">Backlog</span> <span class="n">6</span></div></div>`
  + column("ready", "To do", [
    { id: "KADE-44", title: "Send a reminder when an invoice is due", p: "medium", labels: ["Feature"], who: "BA" },
    { id: "KADE-47", title: "Rate limit the public tracking endpoint", p: "high", labels: ["Backend"], who: "BA" },
    { id: "KADE-45", title: "Show the VAT rate on the invoice PDF", p: "low", labels: ["Feature"] },
  ], { note: "Frontend Agent and Backend Agent take cards by priority" })
  + column("in_progress", "In progress", [
    { id: "KADE-39", title: "Export invoices as CSV", p: "medium", labels: ["Feature"], who: "BA", working: "BA" },
    { id: "KADE-41", title: "Fix the invoice total", p: "high", labels: ["Bug"], hand: true, who: "JS" },
    { id: "KADE-43", title: "Postcode and house number lookup on checkout", p: "urgent", labels: ["Frontend"], who: "FA" },
    { id: "KADE-42", title: "Filament table: remember column order per user", p: "medium", labels: ["Frontend"], who: "FA" },
    { id: "KADE-46", title: "Log in with a link by mail", p: "medium", labels: ["Feature"], who: "FA" },
    { id: "KADE-48", title: "Archive paid invoices after a year", p: "low", labels: ["Backend"], who: "BA" },
  ])
  + column("testing", "Testing", [
    { id: "KADE-37", title: "Credit notes", p: "medium", labels: ["Feature"], who: "QA", working: "QA" },
    { id: "KADE-38", title: "Send the invoice PDF by mail", p: "medium", labels: ["Feature"], who: "QA" },
  ])
  + column("review", "Review", [
    { id: "KADE-31", title: "Dashboard: open invoices per client", p: "medium", labels: ["Frontend"], who: "JS" },
    { id: "KADE-32", title: "Import clients from the old Excel lists", p: "high", labels: ["Backend"], who: "JS" },
    { id: "KADE-35", title: "Round amounts per line, not per invoice", p: "high", labels: ["Bug"], who: "JS" },
    { id: "KADE-33", title: "Dutch and English invoice texts", p: "low", labels: ["Feature"], who: "JS" },
    { id: "KADE-34", title: "Show KvK and BTW numbers on the client page", p: "low", labels: ["Frontend"], who: "JS" },
    { id: "KADE-36", title: "Mark an invoice as paid from the list", p: "medium", labels: ["Feature"], who: "JS" },
  ], { more: "Show 2 more of 8", n: 8 })
  + `</div></div></div>`;

// ---------- the chat: a thread with the Team Lead, the composer at the bottom ----------
const THREADS = [["Plan the Kade portal as tasks", "2m ago"], ["Why did KADE-39's run stop?", "1h ago"], ["Split the export card", "3h ago"], ["Set up the QA Agent", "yesterday"],
  ["Release v0.9.1", "yesterday"], ["What's left on GA?", "2d ago"], ["Invoice reminders: who and when", "3d ago"], ["Name the second Kade project", "5d ago"],
  ["Labels for the Gizai site", "6d ago"], ["Clean up old worktrees", "8d ago"], ["Which CLI for the Design Agent", "9d ago"], ["Move KADE to the new board", "12d ago"],
  ["Budget per run", "14d ago"], ["First tasks for Gizai site", "15d ago"]];
const tool = (verb, lbl) => `<div class="tool-card ok"><div class="tc-row"><span class="tc-state" aria-hidden="true">${sm("check")}</span><span class="tc-verb">${verb}</span><a class="tc-label">${lbl}</a>`
  + `<button class="tc-more" aria-expanded="false" aria-label="Show details" title="Details">${sm("chevron-right")}</button></div></div>`;
const lead = (when, body) => `<div class="chat-group agent"><div class="chat-head">${avatar("TL", "")}<b>Team Lead</b><span class="faint">${when}</span></div><div class="chat-body">${body}</div></div>`;
const you = (text, when) => `<div class="chat-group user"><div class="chat-msg user"><div class="bubble">${text}</div></div><div class="chat-meta">${when}</div></div>`;
const CHAT = `<div class="main">${topbar(`<a>Chat</a><span class="sep">/</span><span class="ellipsis">Plan the Kade portal as tasks</span>`,
  `<a class="lead-chip" title="Team Lead: open its page">${avatar("TL")}Team Lead</a><button class="btn ghost sm icon-only" aria-label="Team Lead settings" title="Team Lead settings">${ic("settings")}</button><button class="btn sm">${ic("plus")}New chat</button>`)}`
  + `<div class="chat"><aside class="chat-threads" aria-label="Chats"><div class="chat-threads-list"><a class="th"><span class="t">${sm("plus")}New chat</span></a><div class="nav-label">Recent</div>`
  + THREADS.map(([t, w], i) => `<a class="th${i === 0 ? " on" : ""}"><span class="t"><span class="ellipsis">${t}</span></span><span class="when">${w}</span></a>`).join("")
  + `</div><div class="chat-threads-foot"><a class="th" title="Every chat, with a search through their titles and messages"><span class="t">${sm("history")}Archive</span></a></div></aside>`
  + `<section class="chat-main"><div class="chat-scroll at-end"><div class="chat-col">`
  + you("Can you split the CSV export into two cards? One exports the invoices, one mails the export to Kade's office each month.", "14:02")
  + lead("14:02", tool("Created task", "KADE-12") + tool("Created task", "KADE-13") + tool("Cancelled task", "KADE-11")
    + `<div class="chat-msg agent"><div class="prose"><p>Done: two cards in <b>Kade portal</b>, both in To do.</p><ul><li><b>KADE-12</b> Export invoices as CSV. Backend Agent takes it next: it's the highest in To do.</li>`
    + `<li><b>KADE-13</b> Mail the export each month. It waits for KADE-12, and its description says so.</li></ul><p>The old card, KADE-11, is cancelled with a link to both.</p></div></div>`)
  + you("Good. Who tests them?", "14:05")
  + lead("14:05", `<div class="chat-msg agent"><div class="prose"><p>QA Agent, when Backend Agent hands them over: both go to Testing first. KADE-13 needs a mail account for its tests, so I added that to its acceptance criteria.</p></div></div>`)
  + `</div></div><div class="chat-composer"><div class="chat-col"><div class="composer-box"><div class="md-editor composer-editor"><div class="md-surface"><div class="cm-editor"><div class="cm-scroller">`
  + `<div class="cm-content" role="textbox" aria-label="Message"><span class="cm-placeholder">Message the Team Lead…</span></div></div></div></div></div>`
  + `<div class="composer-foot"><button class="btn ghost sm icon-only composer-plus" aria-label="Add files or link an item" title="Add files or link an item">${ic("plus")}</button>`
  + `<div class="composer-hint"><span>${kbd("Enter")} sends</span><span>${kbd("Shift")} ${kbd("Enter")} new line</span><span>${kbd("@")} links a task, project, client or agent</span></div>`
  + `<label class="runs-on" title="The coding CLI this chat's answers run on"><span>Runs on</span><span class="runs-on-pick"><span class="runs-on-size" aria-hidden="true">Claude Code</span><select class="select" aria-label="Runs on"><option>Claude Code</option></select></span></label>`
  + `<button class="btn primary sm icon-only" aria-label="Send" title="Send (Enter)" disabled>${ic("arrow-up")}</button></div></div></div></div></section></div></div>`;

// ---------- the Editor: gizai's src/App.tsx, coloured as CodeMirror does, with the status bar at the bottom ----------
const { parser: jsParser } = await import("@lezer/javascript");
const { highlightTree, tagHighlighter, tags: t } = await import("@lezer/highlight");
const HL = tagHighlighter([
  { tag: [t.keyword, t.controlKeyword, t.definitionKeyword, t.moduleKeyword, t.operatorKeyword, t.modifier, t.self], class: "syn-keyword" },
  { tag: [t.string, t.special(t.string), t.regexp, t.escape, t.docString, t.attributeValue], class: "syn-string" },
  { tag: [t.number, t.integer, t.float, t.bool, t.null, t.atom, t.unit], class: "syn-number" },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], class: "syn-comment" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.function(t.definition(t.variableName))], class: "syn-function" },
  { tag: [t.typeName, t.className, t.namespace], class: "syn-type" },
  { tag: [t.variableName, t.definition(t.variableName), t.labelName], class: "syn-variable" },
  { tag: [t.propertyName, t.definition(t.propertyName)], class: "syn-property" },
  { tag: [t.tagName], class: "syn-tag" },
  { tag: [t.attributeName], class: "syn-attribute" },
  { tag: [t.operator, t.arithmeticOperator, t.logicOperator, t.compareOperator, t.updateOperator, t.definitionOperator, t.typeOperator, t.controlOperator], class: "syn-operator" },
  { tag: [t.punctuation, t.separator, t.bracket, t.angleBracket, t.squareBracket, t.paren, t.brace, t.derefOperator], class: "syn-punctuation" },
  { tag: [t.meta, t.documentMeta, t.annotation, t.processingInstruction, t.macroName], class: "syn-meta" },
]);
const APP_SRC = fs.readFileSync(`${GZ}/src/App.tsx`, "utf8");
const APP_CLS = (() => {
  const cls = new Array(APP_SRC.length).fill(null);
  highlightTree(jsParser.configure({ dialect: "jsx ts" }).parse(APP_SRC), HL, (from, to, c) => { for (let i = from; i < to; i++) cls[i] = c; });
  return cls;
})();
const APP_LINE = APP_SRC.split("\n").findIndex((l) => l.includes('<div className="app">')) + 1;
if (!APP_LINE) throw new Error('src/App.tsx has no <div className="app"> line');
const runs = (text, cls) => {
  let html = "";
  for (let i = 0; i < text.length;) { let j = i; while (j < text.length && cls[j] === cls[i]) j++; html += cls[i] ? `<span class="${cls[i]}">${esc(text.slice(i, j))}</span>` : esc(text.slice(i, j)); i = j; }
  return html;
};
function code(first, count, active) {
  const out = [];
  let start = 0;
  APP_SRC.split("\n").forEach((text, n) => {
    if (n + 1 >= first && n + 1 < first + count) out.push(`<div class="ln${n + 1 === active ? " active" : ""}"><span class="no">${n + 1}</span><span class="fd"></span><span class="tx">${runs(text, APP_CLS.slice(start, start + text.length))}${n + 1 === active ? '<span class="cursor"></span>' : ""}</span></div>`);
    start += text.length + 1;
  });
  return `<div class="ed-code" role="textbox" aria-multiline="true" aria-label="Code">${out.join("")}</div>`;
}
const MARK = { modified: ["M", "Modified"], untracked: ["U", "Untracked: new, not in git yet"] };
const mark = (m) => `<span class="ed-mark ${m}" title="${q(MARK[m][1])}">${MARK[m][0]}</span>`;
const fileIcon = (name) => /\.(tsx?|rs|css|mjs)$/.test(name) ? "file-code" : /\.json$/.test(name) ? "file-json" : /\.md$/.test(name) ? "file-text" : "file";
const treeRow = ([depth, kind, name, o = {}]) => `<div class="ed-row${o.ignored ? " ignored" : ""}" style="--depth:${depth}" role="treeitem"${kind === "dir" ? ` aria-expanded="${!!o.open}"` : ""}${o.selected ? ' aria-selected="true"' : ""}>`
  + `${kind === "dir" ? ic(o.open ? "chevron-down" : "chevron-right") : '<span class="room"></span>'}${ic(kind === "dir" ? (o.open ? "folder-open" : "folder") : fileIcon(name))}`
  + `<span class="name">${name}</span>${o.mark ? mark(o.mark) : ""}${o.dot ? '<span class="ed-dot" title="Changes inside"></span>' : ""}</div>`;
const TREE = [
  [0, "dir", "crates"], [0, "dir", "design", { dot: true }], [0, "dir", "docs"], [0, "dir", "node_modules", { ignored: true }], [0, "dir", "scripts"],
  [0, "dir", "src", { open: true, dot: true }], [1, "dir", "assets"], [1, "dir", "components"], [1, "dir", "lib"], [1, "dir", "pages"],
  [1, "dir", "styles", { open: true, dot: true }], [2, "file", "app.css"], [2, "file", "appearance.css"], [2, "file", "changes.css"], [2, "file", "components.css"],
  [2, "file", "fonts.css"], [2, "file", "tokens.css", { mark: "modified" }], [1, "file", "api.ts"], [1, "file", "App.tsx", { selected: true, mark: "modified" }], [1, "file", "main.tsx"],
  [1, "file", "router.ts"], [0, "dir", "src-tauri"], [0, "dir", "target", { ignored: true }], [0, "file", "Cargo.toml"], [0, "file", "index.html"], [0, "file", "package.json"],
  [0, "file", "README.md"], [0, "file", "vite.config.ts"],
];
const tab = (name, path, { on = false, m = "" } = {}) => `<div class="ed-tab" role="tab" tabindex="${on ? 0 : -1}" aria-selected="${on}" title="${path}"><span class="name">${name}</span>${m ? mark(m) : ""}`
  + `<button class="x" aria-label="Close ${name}" title="Close (Ctrl+W)">${ic("x")}</button></div>`;
const EDITOR = `<div class="main"><div class="ed-page">${topbar(`<a>Editor</a><span class="sep">/</span><b title="~/Herd/gizai">gizai</b>`,
  `<button class="btn ghost sm" title="Fetch origin/main now (last fetched 2m ago)">${ic("refresh-cw")}Fetch</button><button class="btn ghost sm" aria-pressed="false" title="Show the terminal (Ctrl+\`)">${ic("terminal")}Terminal</button>`
  + `<button class="btn sm" aria-haspopup="menu">${ic("external-link")}Open in${ic("chevron-down")}</button>`,
  `<div class="ed-standing">${sm("git-branch")}<span class="branch">main</span><span class="sep">·</span><span>up to date with origin/main</span><span class="sep">·</span><span>2 changed</span></div>`)
  .replace('<div class="crumbs">', '<div class="crumbs ed-crumbs">')}`
  + `<div class="ed"><aside class="ed-side" aria-label="Files and search"><div class="ed-side-top"><div class="seg" role="group" aria-label="Files or search"><button aria-pressed="true">Files</button><button aria-pressed="false" title="Search in the folder (Ctrl+Shift+F)">${sm("search")}Search</button></div>`
  + `<button class="btn ghost sm icon-only" aria-label="Fold the file tree (Ctrl+B)" title="Fold the file tree (Ctrl+B)">${ic("panel-left-close")}</button></div>`
  + `<button class="ed-goto">${sm("search")}Go to file${kbd("Ctrl P")}</button>`
  + `<div class="ed-tree-head"><span>gizai</span><button class="btn ghost sm icon-only" aria-label="New file" title="New file">${ic("file-plus")}</button><button class="btn ghost sm icon-only" aria-label="New folder" title="New folder">${ic("folder-plus")}</button><button class="btn ghost sm icon-only" aria-label="Fold every folder" title="Fold every folder">${ic("chevrons-down-up")}</button></div>`
  + `<div class="ed-tree" role="tree" aria-label="Files of gizai">${TREE.map(treeRow).join("")}</div></aside>`
  + `<div class="ed-main"><div class="ed-tabs" role="tablist" aria-label="Open files">${tab("App.tsx", "src/App.tsx", { on: true, m: "modified" })}${tab("tokens.css", "src/styles/tokens.css", { m: "modified" })}${tab("appearance.ts", "src/lib/appearance.ts")}</div>`
  + code(APP_LINE - 18, 44, APP_LINE)
  + `<div class="ed-status" role="status" aria-label="Status bar"><span class="path" title="src/App.tsx">src/App.tsx</span><span class="item git modified">Modified</span><span class="grow"></span>`
  + `<button class="item" title="Go to line (Ctrl+Alt+G)">Ln ${APP_LINE}, Col 7</button><span class="item" title="Line endings, kept as the file has them">LF</span><span class="item">TypeScript JSX</span><span class="item">Saved</span></div></div></div></div></div>`;

// ---------- Settings → Appearance, scrolled to its end: the switch ----------
const field = (lbl, control, hint = "", wide = false) => `<div class="field${wide ? " wide" : ""}"><label>${lbl}</label>${control}${hint ? `<span class="hint">${hint}</span>` : ""}</div>`;
const formSection = (title, text, fields) => `<section class="form-section"><header><h3>${title}</h3><p>${text}</p></header><div class="fields">${fields}</div></section>`;
const seg = (opts, on) => `<div class="seg" role="group">${opts.map((o) => `<button aria-pressed="${o === on}">${o}</button>`).join("")}</div>`;
export const SWITCH = {
  section: "Background",
  text: "A landscape of wireframe mountains along the bottom of the window. It stays behind text and controls.",
  label: "Mountains",
  check: "Show mountains along the bottom",
  hint: "Under the sidebar and the page, in the theme's colours. Off gives the plain background back.",
};
const SETTINGS = (checked = true) => `<div class="main">${topbar("<b>Settings</b>", '<button class="btn primary">Save settings</button>')}<div class="content at-end"><div class="page settings-page" style="max-width:1100px">`
  + `<div class="tabs" role="tablist" aria-label="Settings">${[["sliders-horizontal", "General"], ["palette", "Appearance"], ["bell", "Notifications"], ["bot", "Agents and runs"], ["plug", "MCP servers"], ["git-pull-request", "GitHub and Bitbucket"]]
    .map(([i, l]) => `<button class="tab" role="tab" aria-selected="${l === "Appearance"}">${ic(i)}${l}</button>`).join("")}</div>`
  + `<div class="appearance-note"><span class="faint">Changes show at once, without Save settings, and are kept on this computer.</span><button class="link" disabled title="Everything is at its default">Reset to defaults</button></div>`
  + `<div class="form">`
  + formSection("Text size", "In px of the main text. Headings grow about half as much; IDs, labels, times, badges and icons stay about the same.",
    field("Chat size", seg(["14", "15", "16", "17", "18", "20"], "15"), "Your messages, the agent's messages and the composer. The chat column gets wider with it, so lines stay about as long.", true)
    + field("Interface size", seg(["12.5", "13.5", "14.5", "15.5", "16.5"], "13.5"), "The sidebar, top bar, lists, board, forms, menus and the command palette. Rows, the sidebar and board columns grow with it.", true))
  + formSection("Theme and density", "Also with the t and d keys, when you aren't typing.",
    field("Theme", `<div class="seg" role="group" aria-label="Theme"><button aria-pressed="true"><span class="seg-label">${sm("moon")}Dark</span></button><button aria-pressed="false"><span class="seg-label">${sm("sun")}Light</span></button></div>`)
    + field("Density", seg(["Comfortable", "Compact"], "Comfortable"), "Compact makes rows, menus, buttons and fields shorter."))
  + formSection(SWITCH.section, SWITCH.text, field(SWITCH.label, `<label class="check"><input type="checkbox"${checked ? " checked" : ""}>${SWITCH.check}</label>`, SWITCH.hint, true))
  + `</div></div></div></div>`;

// ---------- frames: each screen in dark (its light twin is made in the browser) ----------
/** The band's height in a window `h` px high: clamp(80px, 14vh, 128px), as band.css has it. */
const bandHeight = (h) => Math.round(Math.min(128, Math.max(80, 0.14 * h)));
const BAND_EL = `<div class="band" aria-hidden="true"><svg><use href="#gz-mountains" width="100%" height="100%"/></svg></div>`;
const frame = (theme, inner, { h = 900, w = null, band = PICK, el = true, cls = "", bandH = bandHeight(h) } = {}) =>
  `<div class="win${cls ? ` ${cls}` : ""}" data-theme="${theme}"${band ? ` data-band="${band}"` : ""} style="height:${h}px;${w ? `width:${w}px;` : ""}--frame-h:${h}px;--band-h:${bandH}px">${band && el ? BAND_EL : ""}${inner}</div>`;
const shot = (lbl, inner, o = {}) => `<figure class="shot dark"${o.band ? ` data-strength="${o.band}"` : ""}><figcaption class="shot-label"><span class="theme-name">Dark</span> · ${lbl}</figcaption>${frame("dark", inner, o)}</figure>`;
/** A row of frames in dark, then the same row in light. */
const frameRows = (items) => `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="vitems">${items.map((it) =>
  `<figure class="vitem"${it.w ? ` style="width:${it.w}px"` : ""}>${it.label ? `<figcaption>${it.label}</figcaption>` : ""}${frame("dark", it.inner, it)}${it.tip ? `<p class="tip">${it.tip}</p>` : ""}</figure>`).join("")}</div></div></div>`;

// ---------- the page's sections ----------
const sections = [];
const section = (id, title, html) => sections.push({ id, title, html: `<section class="dz-sec" id="${id}"><h2>${title}</h2>${html}</section>` });
const notes = (...items) => `<ul class="dz-notes">${items.map((i) => `<li>${i}</li>`).join("")}</ul>`;
const p = (html) => `<p class="dz-p">${html}</p>`;
const h3 = (s) => `<h3>${s}</h3>`;
const c = (s) => `<code>${s}</code>`;
const table = (head, body, rowCls = []) => `<table class="dz-table"><thead><tr>${head.map((x) => `<th>${x}</th>`).join("")}</tr></thead><tbody>${body.map((r, i) => `<tr${rowCls[i] ? ` class="${rowCls[i]}"` : ""}>${r.map((x) => `<td>${x}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
const S = Object.fromEntries(STRENGTHS.map((s) => [s.key, s]));
const kb = (n) => `${(n / 1024).toFixed(1)} kB`;
const fmt = (n) => n.toFixed(2);

async function main() {
  if (problems.length) { console.error(`The band's colours break the card's rules:\n- ${problems.join("\n- ")}`); process.exit(1); }
  const band = draw(BAND_CAMERA), hero = draw(HERO_CAMERA);
  const bandSvg = svg(BAND_CAMERA, { id: "gz-band", groups: band }), heroSvg = svg(HERO_CAMERA, { id: "gz-hero", groups: hero, top: 0.45 });
  if (bandSvg.length >= 60 * 1024 || heroSvg.length >= 60 * 1024) { console.error(`An SVG is 60 kB or more: mountains.svg ${kb(bandSvg.length)}, mountains-hero.svg ${kb(heroSvg.length)}`); process.exit(1); }
  const bandStats = stats(band), heroStats = stats(hero);

  // 1. The artwork ------------------------------------------------------------------------------------------------------------
  const W = (w, h = 900, note = "") => {
    const bh = bandHeight(h), k = Math.min(1, 1270 / w);
    return `<div class="width-row"><div class="what"><b>${w} × ${h}</b>${note}</div><div class="scaled" style="width:${Math.round(w * k)}px;height:${Math.round(bh * k)}px">`
      + `<div class="win strip" data-theme="dark" data-band="${PICK}" style="width:${w}px;height:${bh}px;--frame-h:${h}px;--band-h:${bh}px;transform:scale(${k})">${BAND_EL}</div></div></div>`;
  };
  section("art", "1. The artwork",
    p(`Our own land, drawn by ${c("gen-bottom-band.mjs")} from seed ${SEED}: the same lines every time it runs, and no wallpaper or photo in it. A flat valley runs into the distance between ridged mountains, seen through a perspective camera, as a mesh of four-sided cells. The lines a nearer slope hides are taken out (a floating horizon, front to back), so the file holds only the lines you see; far lines fade, and the top of the picture fades so the peaks melt into the page above.`)
    + `<div class="dz-facts"><div class="dz-fact"><b>${kb(bandSvg.length)}</b><span>mountains.svg, for the app's band (under 60 kB)</span></div>`
    + `<div class="dz-fact"><b>${kb(heroSvg.length)}</b><span>mountains-hero.svg, for the website's hero</span></div>`
    + `<div class="dz-fact"><b>${BAND_CAMERA.W} × ${BAND_CAMERA.H}</b><span>the band at 1:1 (viewBox in quarter pixels: 0 0 ${BAND_CAMERA.W * 4} ${BAND_CAMERA.H * 4})</span></div>`
    + `<div class="dz-fact"><b>${bandStats.lines} lines</b><span>${bandStats.points} points, in ${band.length} paths by distance</span></div>`
    + `<div class="dz-fact"><b>Seed ${SEED}</b><span>mulberry32 and gradient noise: no randomness from outside the file</span></div></div>`
    + notes(
      "<b>Sharp at every width.</b> It is vector and has no tiles: one picture, centred under the window's middle and cut at the sides (<code>preserveAspectRatio=\"xMidYMax slice\"</code>), so there is no seam to see. Its lines keep 1px at any size (<code>vector-effect: non-scaling-stroke</code>).",
      `<b>Peaks on both sides at every width.</b> The valley floor is ${LAND.valley * 2}px wide at the bottom edge, so even an 800px window shows the slopes rising on both sides of it; a wider window shows more ranges, smaller and fainter toward the sides.`,
      "<b>A short window</b> makes the band lower (§5.4): the picture scales down with it and shows more of its width.",
      "<b>One file for both themes.</b> The lines are <code>currentColor</code>: the band's colour comes from <code>band-line</code>, so light needs no file of its own.")
    + h3("The whole band at 1:1: scroll sideways")
    + `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="art-scroll">${frame("dark", "", { h: 128, w: BAND_CAMERA.W, cls: "strip", bandH: 128 })}</div></div></div>`
    + h3("What windows of each width show")
    + p(`Each strip is the band as a window that wide shows it at ${PICK}, scaled down here to fit the page (the browser draws the vector at the smaller size, so it stays sharp). 128px high from 915px of window height up; ${bandHeight(600)}px in Gizai's smallest window, 900 × 600.`)
    + `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="widths">${W(800, 900, "The card's narrowest")}${W(900, 600, "Gizai's smallest window")}${W(1440, 900, "Gizai's first size")}${W(1920, 1080)}${W(2560, 1440)}${W(3840, 2160, "A 4K screen at 1:1")}</div></div></div>`);

  // 2. Three strengths on three screens ------------------------------------------------------------------------------------------
  const three = (name, inner) => STRENGTHS.map((s) => shot(`${name} · <b>${s.name}</b>${s.key === PICK ? " (the proposal)" : ""}`, inner(s), { band: s.key, h: 900 })).join("");
  section("screens", "2. Three strengths on three screens",
    p(`Gizai's first window size, 1440 × 900, where the band is ${bandHeight(900)}px high. ${STRENGTHS.map((s) => `<b>${s.name}</b>: ${s.what}`).join(" ")} The switch at the top shows one strength.`)
    + h3("The board")
    + notes(
      "The canvas is see-through: the band shows under the sidebar, between the columns and below the cards. Cards stay solid, so their text never has the band behind it, and a live card's working strip keeps its teal on the card's own surface.",
      "A long column's last cards stand in the band, and so does Review's “Show 2 more of 8” (<code>text-2</code> on no surface of its own).",
      "Under the sidebar the band runs behind Company's Usage, Team, Users and Settings (<code>text-2</code>, icons <code>text-3</code>). The sidebar's colour stays: it moves under the band.")
    + three("Board", () => sidebar("tasks") + BOARD)
    + h3("The chat, with its composer at the bottom")
    + notes(
      "The composer's strip is see-through and its box stays solid: the text, the hints (<code>text-3</code>), Runs on and Send never have the band behind them. The band shows beside the box and under it.",
      "The threads column is see-through like the sidebar: its last rows (their times are <code>text-3</code>) and Archive stand in the band.",
      "The messages stop at the composer's top border, so a message scrolls off before it reaches the band's strong part.")
    + three("Chat", () => sidebar("chat") + CHAT)
    + h3("The Editor, with its status bar")
    + notes(
      "The file tree is see-through like the sidebar: its last rows stand in the band (an ignored folder's name is <code>text-3</code>).",
      "The code, its tabs, the status bar and the terminal panel stay solid: <b>the band never runs behind code</b>, where every character counts. So in the Editor the band shows under the sidebar and the tree, and the code column stands in front of it, its status bar on the bottom edge as now.")
    + three("Editor", () => sidebar("editor") + EDITOR));

  // 3. Readability ----------------------------------------------------------------------------------------------------------------
  const cell = (r, k) => {
    const v = r.c[k], low = (n) => (n < 4.5 ? ' class="low"' : "");
    return `<span class="num"><span${low(v.between)}>${fmt(v.between)}</span> · <span${low(v.on)}>${fmt(v.on)}</span></span>`;
  };
  const sw = (hex) => `<span class="sw" style="background:${hex}"></span>`;
  section("readability", "3. Text over the band",
    p("WCAG 2 contrast of the text colours over the band, where the band is strongest: its bottom edge, with the glow at full strength over the canvas (<code>bg</code>, or <code>side</code> under the sidebar, the threads and the tree), and a line at full strength over that. Each cell gives <b>between the lines · on a line</b>; under 4.5:1 is marked. Text sits higher than the edge almost everywhere (the sidebar's last row starts 12px up, where the glow is a tenth weaker), so these are the worst case.")
    + table(["Strength", "Theme", "Canvas", "Behind the text", "text", "text-2", "text-3"],
      CONTRAST.map((r) => [`${S[r.strength].name}${r.strength === PICK ? " (the proposal)" : ""}`, r.theme === "dark" ? "Dark" : "Light", c(r.surface),
        `<span class="num">${sw(r.between)}${r.between} · ${sw(r.on)}${r.on}</span>`, cell(r, "text"), cell(r, "text-2"), cell(r, "text-3")]),
      CONTRAST.map((r) => (r.strength === PICK ? "pick" : "")))
    + notes(
      "<b>Subtle and medium</b> keep all three text colours at 4.5:1 or more between the lines, and <code>text</code> and <code>text-2</code> at 4.5:1 even on a line; <code>text-3</code> keeps 3:1 there. A line is 1px wide: where one crosses a letter, that pixel has the second number and the rest of the letter the first.",
      "<b>Strong</b> keeps <code>text</code> and <code>text-2</code> at 4.5:1 between the lines and <code>text</code> on a line; <code>text-3</code> drops to about 4:1 at the bottom edge, and a line crossing <code>text-2</code> drops under 3:1. That is the price of the bright strip: picking it means the faint text near the bottom (thread times, an ignored file's name) reads less well.",
      "Controls keep their own surfaces (buttons, inputs, the composer), so the band changes no control's contrast. The focus ring (<code>focus</code>) keeps 3:1 on the band in every strength.",
      `${c("gen-bottom-band.mjs")} checks these numbers before it writes anything, and stops when a strength breaks its rule.`));

  // 4. Colours ----------------------------------------------------------------------------------------------------------------------
  const tok = (name, d, l, usage) => [c(`--${name}`), d.startsWith("{") ? `= ${d.slice(1, -1)}` : `${sw(d)}<span class="num">${d}</span>`, l.startsWith("{") ? `= ${l.slice(1, -1)}` : `${sw(l)}<span class="num">${l}</span>`, usage];
  const TOKENS = [
    ...STRENGTHS.flatMap((s) => [
      [`band-${s.key}-line`, tokenOf(s, "dark").line, tokenOf(s, "light").line, `${s.name}: the lines (currentColor of the band's SVG).`],
      [`band-${s.key}-glow`, tokenOf(s, "dark").glow, tokenOf(s, "light").glow, `${s.name}: the glow at the bottom edge, fading to nothing at the band's top.`],
    ]),
    ["band-line", `{band-${PICK}-line}`, `{band-${PICK}-line}`, "The band's lines: the strength Jefsev picks (the proposal until then)."],
    ["band-glow", `{band-${PICK}-glow}`, `{band-${PICK}-glow}`, "The band's glow: the strength Jefsev picks."],
  ];
  const SIZES = [
    ["size-band", "128px", "The band's height in a window 915px high or more; lower windows get 14% of their height (clamp(size-band-min, 14vh, size-band))."],
    ["size-band-min", "80px", "The band's least height; under 560px of window height the band goes."],
  ];
  const deg = (n) => `${n.toFixed(0)}°`;
  const CMP = [
    ["teal", "Teal, as in the wallpaper", "Reads as <b>an agent at work</b>: the same hue as <code>live</code>, the Live badge and the working strip.", APP.dark.live, fromOklch(0.47, 0.085, oklch(APP.dark.live)[2]), APP.light.live, fromOklch(0.9, 0.032, oklch(APP.light.live)[2])],
    ["accent", "The accent's blue", "Reads as <b>a control or a selection</b>: the hue of primary buttons, links and focus.", APP.dark.accent, fromOklch(0.47, 0.085, oklch(APP.dark.accent)[2]), APP.light.accent, fromOklch(0.9, 0.032, oklch(APP.light.accent)[2])],
    ["ice", "Ice blue, the proposal", `<b>The window's own light</b>: hue ${HUE}°, about 40° from both, and quieter than either.`, BASE.dark.line, BASE.dark.glow, BASE.light.line, BASE.light.glow],
  ];
  const cmpStyle = CMP.map(([k, , , dl, dg, ll, lg]) => `.cmp-${k}[data-theme="dark"] { --band-line: ${rgba(dl, S[PICK].alpha.dark.line)}; --band-glow: ${rgba(dg, S[PICK].alpha.dark.glow)}; }\n`
    + `.cmp-${k}[data-theme="light"] { --band-line: ${rgba(ll, S[PICK].alpha.light.line)}; --band-glow: ${rgba(lg, S[PICK].alpha.light.glow)}; }`).join("\n");
  const cmpInner = `<div class="row"><span class="badge live"><span class="pulse"></span>Live</span><span class="badge needs">Review</span><button class="btn primary sm">Run</button><span class="live-tag"><span class="pulse"></span>1 live</span></div>`
    + `<div class="card"><div class="top"><span class="id">KADE-39</span>${pri("medium")}</div><div class="working"><span class="pulse" aria-hidden="true"></span>Backend Agent is working</div></div>`;
  section("colours", "4. Colours: the band isn't a status",
    p(`Teal means an agent at work and magenta “needs you” (${c("design/README.md")}, Colour carries meaning). Jefsev's wallpaper is teal, so the band takes a cooler, quieter blue: one hue, ${HUE}° in OKLCH, between teal (<code>live</code>, ${deg(oklch(APP.dark.live)[2])}) and the accent's blue (${deg(oklch(APP.dark.accent)[2])}), with less chroma than either and at a low alpha. It never pulses or moves: only the live pulse moves on its own.`)
    + `<div class="variants"><div class="vrow dark"><div class="vhead theme-name">Dark</div><div class="cmp">${CMP.map(([k, title, say]) =>
      `<figure class="vitem" style="margin:0"><figcaption>${title}</figcaption>${frame("dark", cmpInner, { h: 190, band: "cmp", cls: `strip cmp-${k}`, bandH: 120 })}<p class="cmp-say">${say}</p></figure>`).join("")}</div></div></div>`
    + h3("The tokens")
    + p(`For ${c("design/tokens.json")}, in its format, in ${c("new-tokens.json")}: the three strengths, and <code>band-line</code> and <code>band-glow</code>, which follow the strength Jefsev picks (${S[PICK].name.toLowerCase()} until then). The frontend card can leave out the two strengths not picked.`)
    + table(["Token", "Dark", "Light", "For"], [...TOKENS.map(([n, d, l, u]) => tok(n, d, l, u)), ...SIZES.map(([n, v, u]) => [c(`--${n}`), `<span class="num">${v}</span>`, `<span class="num">${v}</span>`, u])])
    + h3("How far the band is from the states")
    + p("OKLCH hue apart, and CIEDE2000 colour difference (ΔE), of the band's two colours, opaque. The terminal's rule (GA-132) holds: at least 35° and a ΔE of 15 from <code>live</code> and <code>needs</code>; and 30° from the accent.")
    + table(["Colour", "Hue", "From live", "From needs", "From the accent"], HUES.map((h) => [`${sw(h.hex)}${h.theme === "dark" ? "Dark" : "Light"} ${h.what} <span class="num">${h.hex}</span>`, `<span class="num">${deg(h.hue)}</span>`,
      ...["live", "needs", "accent"].map((k) => `<span class="num">${deg(h[k][0])} · ΔE ${h[k][1].toFixed(1)}</span>`)])));

  // 5. How to build it ---------------------------------------------------------------------------------------------------------------
  const BAND_CSS = fs.readFileSync(`${HERE}/band.css`, "utf8");
  section("build", "5. How to build it",
    p("For the frontend card that builds the strength Jefsev picks. Nothing in the app changes in this card. The CSS is in <code>band.css</code>, which this page uses as it is; the tokens are in <code>new-tokens.json</code>.")
    + h3("5.1 Where the layer sits")
    + p("One element, the window's lowest layer: the first child of <code>.app</code>, along its bottom edge at <code>z-index: -1</code>, inside the window's own stacking context (<code>isolation: isolate</code> on <code>.app</code>). Everything in the window is above it; the canvases are see-through, so it shows between what is on them.")
    + `<pre class="dz-code">${esc(`<div class="app">                          position: relative; isolation: isolate
  <div class="band" aria-hidden="true">   z-index: -1, the window's width, along its bottom
    <svg …>mountains.svg's lines</svg>     currentColor: band-line, on a glow of band-glow
  </div>
  <aside class="side">…</aside>           see-through: its colour on .side::before, z-index: -2
  <main class="main">…</main>             see-through: body's bg is its colour
</div>`)}</pre>`
    + notes(
      "<b>Inline, not an image.</b> <code>Band.tsx</code> puts the SVG's markup in the page (Vite's <code>?raw</code> import of <code>mountains.svg</code>), not an <code>&lt;img&gt;</code> or a CSS background: inline, the lines take <code>currentColor</code>, so one file serves both themes and every strength, and no mask image has to load.",
      "<b>The layers</b>, bottom up: body's <code>bg</code>; the side-coloured columns' <code>::before</code> (z-index -2); the band (-1), its glow and then its lines; then everything else in the window, in its usual order.",
      "<b>The glow</b> is a gradient on the band itself: <code>transparent</code> at its top to <code>band-glow</code> at the bottom edge. The SVG fades its own top (a mask in the file) and its far lines.",
      "<b>Nothing else needs a z-index.</b> A stacking context inside <code>.app</code> (a popover, the palette, a dragged card) stays above the band as now. Only a new <code>z-index</code> below 0 could go under it.")
    + h3("5.2 See-through and solid")
    + table(["Surface", "Over the band", "Why"], [
      ["The sidebar (<code>.side</code>)", "See-through; its colour moves to <code>.side::before</code>", "The band runs under it, behind Company's rows; a row's hover and selected fill stay solid"],
      ["The main column (<code>.main</code>) and its canvases: <code>.content</code>, <code>.board</code>, <code>.doc</code>, <code>.page</code>", "See-through", "body's <code>bg</code> is their colour already"],
      ["The chat's threads (<code>.chat-threads</code>)", "See-through, like the sidebar", "The band runs on under the second column"],
      ["The chat's composer strip (<code>.chat-composer</code>)", "See-through", "Only the strip: the box in it stays solid"],
      ["The Editor's file tree (<code>.ed-side</code>)", "See-through, like the sidebar", "Its rows are names, not code"],
      ["The properties panel (<code>.props</code>)", "See-through", "Its rows are at the top; its bottom is empty"],
      ["The composer's box and its banners, inputs, buttons, chips", "Solid", "Controls keep their own surface and contrast"],
      ["Cards, panels, run cards, tool cards, user bubbles", "Solid", "They hold text"],
      ["The Editor's code, tabs and status bar; the terminal panel and the Terminal page; the Changes view", "Solid", "The band never runs behind code or a terminal"],
      ["Sticky table heads, banners", "Solid", "They hold text over what scrolls"],
      ["Menus, popovers, the palette, drawers, toasts, dialogs", "Solid, and above everything", "They float"],
      ["The scrim", "Over the band too", "It dims the whole window"],
    ])
    + h3("5.3 Never in the way")
    + notes(
      "<code>pointer-events: none</code> and <code>user-select: none</code>: clicks, drags, the wheel and selections go to what is above it. A card dragged to the bottom, or text selected down to the last line, works as now.",
      "<code>aria-hidden=\"true\"</code> on the element: screen readers skip it. It has no title, no role and takes no focus.",
      "It never moves: no animation, so nothing for <code>prefers-reduced-motion</code> to stop. Only the live pulse moves on its own.",
      "It takes no space: turning it on or off moves nothing.",
      "It goes with the system's high contrast (<code>prefers-contrast: more</code>), in forced colours (Windows' contrast themes) and on paper.")
    + h3("5.4 Its height, and short windows")
    + notes(
      "<code>height: clamp(var(--size-band-min), 14vh, var(--size-band))</code>: 128px in a window 915px high or more, 14% of the height below that, at least 80px. 1080 high: 128px. 900 (Gizai's first size): 126px. 768: 108px. 600 (Gizai's smallest): 84px.",
      "The picture scales with the band's height and is cut at the sides, so a lower band shows the same mountains smaller, and more of them (§1).",
      "Under 560px of window height the band goes: Gizai's own minimum is 600, but a tiling window manager can make the window shorter, and there the content needs the room more than the band.")
    + `<div class="variants">${shot(`The board at Gizai's smallest window, 900 × 600: the band is ${bandHeight(600)}px`, sidebar("tasks") + BOARD, { h: 600, w: 900, band: PICK })}</div>`
    + h3("5.5 The switch: Settings → Appearance")
    + notes(
      `A new section at the end of Appearance, <b>${SWITCH.section}</b>, with one checkbox: <b>${SWITCH.check}</b>, on by default. Its hint: “${SWITCH.hint}” As everything on this tab, a change shows at once, without Save settings, and is kept on this computer.`,
      "<code>src/lib/appearance.ts</code> gets <code>band: boolean</code> (default <code>true</code>), kept as <code>gizai-band</code>, and puts <code>data-band=\"on\"</code> on <code>&lt;html&gt;</code> while it is on, before the first paint, as it does <code>data-density</code>. Reset to defaults turns it on again. App.tsx renders the band only while it is on.",
      "No key for it: t and d stay the only Appearance keys.")
    + frameRows([
      { label: "On, the default", inner: sidebar("settings") + SETTINGS(true), h: 600, w: 1440 },
      { label: "Off: the plain background", inner: sidebar("settings") + SETTINGS(false), h: 600, w: 1440, band: null },
    ])
    + h3("5.6 For the frontend card")
    + notes(
      "<b>Tokens:</b> from <code>new-tokens.json</code>, <code>band-line</code>, <code>band-glow</code> and the pair of the strength Jefsev picked to <code>color.tokens</code> after <code>scrim</code>, and <code>size-band</code> and <code>size-band-min</code> to <code>size.tokens</code> after <code>size-statusbar</code>; then <code>node scripts/tokens-css.ts</code>.",
      "<b>The artwork:</b> <code>mountains.svg</code> to <code>src/assets/</code>, as it is (run the generator again to change it, then copy it again).",
      "<b>The CSS:</b> <code>band.css</code> to <code>src/styles/band.css</code>, imported after <code>appearance.css</code> in <code>main.tsx</code>; the page-only <code>.win[data-band]</code> can go.",
      "<b>The component:</b> <code>Band.tsx</code>, the element of §5.1, rendered as <code>.app</code>'s first child while the setting is on; the setting as in §5.5.",
      "<b>Check by eye</b> in both themes: the board with a long column, a chat, the Editor with the tree open, Settings, a task page with its properties, and a drawer open over the band.")
    + h3("band.css")
    + `<pre class="dz-code">${esc(BAND_CSS).replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="c">$1</span>')}</pre>`);

  // 6. The website's hero ------------------------------------------------------------------------------------------------------------
  const HERO = `<div class="site"><div class="site-nav"><b>Gizai</b><span>Features</span><span>Docs</span><span>Releases</span><a class="btn sm">${ic("github")}GitHub</a></div>`
    + `<div class="hero"><div class="hero-band" aria-hidden="true"><svg><use href="#gz-hero" width="100%" height="100%"/></svg></div>`
    + `<div class="hero-text"><h1>Gizai</h1><p>Your clients, projects and tasks, worked on by a team of AI coding agents on your own computer.</p>`
    + `<div class="acts"><a class="btn primary">${ic("download")}Download</a><a class="btn">Read the docs</a></div></div></div></div>`;
  section("website", "6. The website's hero (GA-52)",
    p(`The same land, seen closer up: ${c("mountains-hero.svg")} comes from the same seed, the same terrain and the same camera, 3.5 times nearer and with a finer mesh: ${HERO_CAMERA.W} × ${HERO_CAMERA.H} at 1:1, ${kb(heroSvg.length)}. It sits along the bottom of the hero, in the band's colours at strong (the website has only a headline above it), with the headline and buttons in its sky. The text and the menu stand in for GA-52's.`)
    + notes(
      "As in the app it is inline SVG, cut at the sides (<code>xMidYMax slice</code>), its lines 1px at every size: on a wide screen more ranges show, on a narrow one the valley and its nearest peaks.",
      "The glow is a gradient on the hero's band, from <code>transparent</code> at a fifth of its height to <code>band-glow</code> at the bottom.",
      "The website can take the tokens' values as they are; it has no live agents, but keeping the band's blue keeps the website and the app one family.")
    + frameRows([
      { label: "1440 wide", inner: HERO, h: 580, w: 1440, band: "strong", el: false },
      { label: "800 wide", inner: HERO, h: 580, w: 800, band: "strong", el: false },
    ]));

  // 7. Files ----------------------------------------------------------------------------------------------------------------------------
  section("files", "7. Files",
    table(["File", "What it is"], [
      [c("bottom-band.html"), "This page. It opens from disk and uses the app's stylesheets in <code>src/styles</code>."],
      [c("gen-bottom-band.mjs"), "Builds every file below from one list of colours and one seed. It checks the band's colours (contrast, apart from the states) and the SVGs' size first, and writes nothing when one fails. <code>node design/branding/gen-bottom-band.mjs</code>"],
      [c("mountains.svg"), `The app's band, ${kb(bandSvg.length)}: ${BAND_CAMERA.W} × ${BAND_CAMERA.H} at 1:1, lines in <code>currentColor</code>.`],
      [c("mountains-hero.svg"), `The website's hero, ${kb(heroSvg.length)}: ${HERO_CAMERA.W} × ${HERO_CAMERA.H} at 1:1.`],
      [c("band.css"), "The band's CSS: the layer, and which surfaces are see-through. The frontend card moves it into <code>src/styles</code>."],
      [c("new-tokens.json"), "The band's tokens in <code>design/tokens.json</code>'s format."],
      [c("band-tokens.css"), "The same tokens as CSS, for this page, with each strength for its frames."],
      [c("bottom-band-page.css"), "This page's layout, and stand-ins for CodeMirror's code and the composer's empty line."],
      [c("README.md"), "What is here, the decisions, and the tables of the tokens and the contrast."],
    ]));

  // ---------- the files ----------
  fs.writeFileSync(`${HERE}/mountains.svg`, bandSvg);
  fs.writeFileSync(`${HERE}/mountains-hero.svg`, heroSvg);
  const tokenValue = (d, l) => (d === l && d.startsWith("{") ? d : { dark: d, light: l });
  fs.writeFileSync(`${HERE}/new-tokens.json`, JSON.stringify({
    note: `GA-139 (the mountains along the bottom of the window): the band's tokens for design/tokens.json, in its format. The frontend card adds band-line and band-glow, with the pair of the strength Jefsev picked (and points them at it), to color.tokens after scrim, and the sizes to size.tokens after size-statusbar, then runs node scripts/tokens-css.ts. Made by gen-bottom-band.mjs.`,
    color: { tokens: TOKENS.map(([name, d, l, usage]) => ({ name, value: tokenValue(d, l), usage })) },
    size: { tokens: SIZES.map(([name, value, usage]) => ({ name, value, usage })) },
  }, null, 2) + "\n");
  const cssValue = (v) => v.replace(/^\{([\w-]+)\}$/, "var(--$1)");
  fs.writeFileSync(`${HERE}/band-tokens.css`, `/* The band's tokens (GA-139), until the frontend card puts them in design/tokens.json and src/styles/tokens.css. Made by
   gen-bottom-band.mjs from the same list as new-tokens.json: change them there. An alias is declared in each theme, so a
   frame of this page in the other theme resolves it to its own colour. */

:root, [data-theme="dark"] {
${TOKENS.map(([n, d]) => `  --${n}: ${cssValue(d)};`).join("\n")}
}

[data-theme="light"] {
${TOKENS.map(([n, , l]) => `  --${n}: ${cssValue(l)};`).join("\n")}
}

:root {
${SIZES.map(([n, v]) => `  --${n}: ${v};`).join("\n")}
}

/* This page only: a frame's strength (the app has one, in band-line and band-glow) */
${STRENGTHS.map((s) => `[data-band="${s.key}"] { --band-line: var(--band-${s.key}-line); --band-glow: var(--band-${s.key}-glow); }`).join("\n")}
`);

  // the page
  const toc = sections.map((s) => `<a href="#${s.id}">${s.title}</a>`).join("");
  const body = `<main class="dz">
<header class="dz-head">
  <div class="dz-kicker"><span class="badge outline">GA-139</span><span>Branding</span><span class="faint">for Jefsev's review</span></div>
  <h1>Mountains along the bottom</h1>
  <p class="dz-lead">Gizai's own wireframe landscape as a band along the bottom of the window, from the left edge to the right edge, under the sidebar and the main column: more visible than what Omarchy lets through the window now, and never in the way of text or controls. Three strengths to pick from, on the board, the chat and the Editor, in dark and light.</p>
  <p class="dz-p">Every screen is drawn with the app's own stylesheets (<code>src/styles</code>), so it looks as the app will; the band is <code>band.css</code> with its tokens in <code>band-tokens.css</code> (from <code>new-tokens.json</code>). Each screen shows dark, then light; the switches show one theme or one strength. The proposal is <span class="dz-pick">${S[PICK].name}</span>: the band reads at a glance, and all three text colours keep 4.5:1 over it. Built by <code>node design/branding/gen-bottom-band.mjs</code>.</p>
  <div class="dz-switches">
    <div class="dz-show" role="group" aria-label="Theme"><span class="faint">Theme</span><div class="seg" data-key="show"><button data-v="both">Dark and light</button><button data-v="dark">Dark</button><button data-v="light">Light</button></div></div>
    <div class="dz-show" role="group" aria-label="Strength"><span class="faint">Strength</span><div class="seg" data-key="strength-show"><button data-v="all">All three</button>${STRENGTHS.map((s) => `<button data-v="${s.key}">${s.name}</button>`).join("")}</div></div>
  </div>
  <nav class="dz-toc" aria-label="Contents">${toc}</nav>
</header>
${sections.map((s) => s.html).join("\n")}
</main>`;
  const SCRIPT = `// Each screen is written once, in dark; its light twin is made here, so the file holds every screen once.
document.querySelectorAll(".shot.dark, .vrow.dark").forEach((dark) => {
  const light = dark.cloneNode(true);
  light.classList.replace("dark", "light");
  light.querySelectorAll('[data-theme="dark"]').forEach((w) => { w.dataset.theme = "light"; });
  light.querySelectorAll(".theme-name").forEach((n) => { n.textContent = "Light"; });
  light.querySelectorAll('.seg[aria-label="Theme"] button').forEach((b, i) => b.setAttribute("aria-pressed", String(i === 1)));
  dark.after(light);
});
// The switches: a theme (data-show) and a strength (data-strength-show), kept for the next visit
const root = document.documentElement;
function set(key, v) {
  root.dataset[key === "show" ? "show" : "strengthShow"] = v;
  if (key === "show") root.dataset.theme = v === "light" ? "light" : "dark";
  document.querySelectorAll('.seg[data-key="' + key + '"] button').forEach((b) => b.setAttribute("aria-pressed", String(b.dataset.v === v)));
  try { localStorage.setItem("gizai.design." + key, v); } catch {}
}
document.querySelectorAll(".seg[data-key] button").forEach((b) => b.addEventListener("click", () => set(b.parentElement.dataset.key, b.dataset.v)));
const kept = (key) => { try { return localStorage.getItem("gizai.design." + key); } catch { return null; } };
set("show", kept("show") || "both");
set("strength-show", kept("strength-show") || "all");`;
  const html = `<!doctype html>
<html lang="en" data-theme="dark" data-show="both" data-strength-show="all">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Gizai · GA-139: mountains along the bottom of the window</title>
<!-- Generated by design/branding/gen-bottom-band.mjs: change that file, then run it. -->
<!-- The app's stylesheets in main.tsx's order. band-tokens.css comes first, so the app's own tokens win once they have its names. -->
<link rel="stylesheet" href="band-tokens.css">
<link rel="stylesheet" href="../../src/styles/fonts.css">
<link rel="stylesheet" href="../../src/styles/tokens.css">
<link rel="stylesheet" href="../../src/styles/components.css">
<link rel="stylesheet" href="../../src/styles/app.css">
<link rel="stylesheet" href="../../src/styles/changes.css">
<link rel="stylesheet" href="../../src/styles/appearance.css">
<link rel="stylesheet" href="band.css">
<!-- This page only: its layout around the frames, and stand-ins for what CodeMirror draws -->
<link rel="stylesheet" href="bottom-band-page.css">
<style>
/* §4: the band in the wallpaper's teal and in the accent's blue, beside the proposal, at ${S[PICK].name.toLowerCase()}'s alphas */
${cmpStyle}
</style>
</head>
<body>
<svg width="0" height="0" style="position:absolute" aria-hidden="true">
${await sprite()}
<symbol id="gz-mountains" viewBox="0 0 ${BAND_CAMERA.W * 4} ${BAND_CAMERA.H * 4}" preserveAspectRatio="xMidYMax slice">${svg(BAND_CAMERA, { id: "gz-band", symbol: true, groups: band })}</symbol>
<symbol id="gz-hero" viewBox="0 0 ${HERO_CAMERA.W * 4} ${HERO_CAMERA.H * 4}" preserveAspectRatio="xMidYMax slice">${svg(HERO_CAMERA, { id: "gz-hero", symbol: true, top: 0.45, groups: hero })}</symbol>
</svg>
${body}
<script>
${SCRIPT}
</script>
</body>
</html>
`;
  fs.writeFileSync(`${HERE}/bottom-band.html`, html);

  // README.md's tables, between its markers
  const README = `${HERE}/README.md`;
  if (fs.existsSync(README)) {
    const md = fs.readFileSync(README, "utf8");
    const v = (x) => (x.startsWith("{") ? `= ${x.slice(1, -1)}` : `\`${x}\``);
    const tables = "| Token | Dark | Light |\n|---|---|---|\n" + TOKENS.map(([n, d, l]) => `| \`--${n}\` | ${v(d)} | ${v(l)} |`).join("\n")
      + "\n" + SIZES.map(([n, val]) => `| \`--${n}\` | ${val} | ${val} |`).join("\n")
      + "\n\nContrast at the band's bottom edge, between the lines · on a line (WCAG 2):\n\n| Strength | Theme | Canvas | text | text-2 | text-3 |\n|---|---|---|---|---|---|\n"
      + CONTRAST.map((r) => `| ${S[r.strength].name} | ${r.theme} | \`${r.surface}\` | ${TEXTS.map((k) => `${fmt(r.c[k].between)} · ${fmt(r.c[k].on)}`).join(" | ")} |`).join("\n");
    const out = md.replace(/(<!-- tables: generated by gen-bottom-band\.mjs -->)[\s\S]*?(<!-- \/tables -->)/, (_, open, close) => `${open}\n${tables}\n${close}`);
    if (out !== md) fs.writeFileSync(README, out);
  }
  console.log(`wrote design/branding/bottom-band.html: ${sections.length} sections, ${used.size} icons, ${kb(html.length)}; mountains.svg ${kb(bandSvg.length)} (${bandStats.lines} lines, ${bandStats.points} points); mountains-hero.svg ${kb(heroSvg.length)} (${heroStats.lines} lines); new-tokens.json (${TOKENS.length} colours, ${SIZES.length} sizes); band-tokens.css`);
}
if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) await main();
