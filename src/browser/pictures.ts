// The browser mode's screenshots (GA-185): the fixtures hold no images, so each one is drawn from its size, caption and
// file name: a made-up shop page in the theme its name says (-light or dark), the caption on it and its size in a corner.
// The same caption and size draw the same picture, so a pair Compare calls Unchanged looks the same too.

type Shot = { name: string; caption: string; width: number; height: number };

/** A small number from the text, so changed captions move the page's parts a little. */
function seed(text: string): number {
  let h = 2166136261;
  for (const c of text) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return (h >>> 0) / 4294967296;
}

export async function screenshotPicture(s: Shot): Promise<ArrayBuffer> {
  const w = Math.max(1, s.width), h = Math.max(1, s.height);
  const canvas = new OffscreenCanvas(w, h);
  const g = canvas.getContext("2d")!;
  const light = /-light\b|-light\./.test(s.name);
  const c = light
    ? { bg: "#f6f7f9", panel: "#ffffff", line: "#dfe2e8", text: "#1d2230", faint: "#6b7280", card: "#eceff3", accent: "#2f6fe4" }
    : { bg: "#15171c", panel: "#1d2027", line: "#2c3038", text: "#e8eaef", faint: "#8b919c", card: "#262a32", accent: "#6f97ff" };
  const r = seed(s.caption);
  const phone = w < 600;
  g.fillStyle = c.bg; g.fillRect(0, 0, w, h);
  // the page's bar
  g.fillStyle = c.panel; g.fillRect(0, 0, w, 56);
  g.fillStyle = c.line; g.fillRect(0, 56, w, 1);
  g.fillStyle = c.accent; g.fillRect(20, 18, 20, 20);
  g.fillStyle = c.text; g.font = "600 16px sans-serif"; g.fillText("Groene Fiets", 50, 34);
  // the filters with the price slider
  const side = phone ? 0 : 260;
  if (!phone) {
    g.fillStyle = c.panel; g.fillRect(0, 57, side, h - 57);
    g.fillStyle = c.line; g.fillRect(side, 57, 1, h - 57);
  }
  const sy = phone ? 80 : 100, sx = 20, sw = (phone ? w : side) - 40;
  g.fillStyle = c.faint; g.font = "13px sans-serif"; g.fillText("Price", sx, sy - 12);
  g.fillStyle = c.line; g.fillRect(sx, sy, sw, 4);
  const a = sx + sw * (0.1 + r * 0.3), b = sx + sw * (0.6 + r * 0.3);
  g.fillStyle = c.accent; g.fillRect(a, sy, b - a, 4);
  for (const x of [a, b]) { g.beginPath(); g.arc(x, sy + 2, 8, 0, Math.PI * 2); g.fill(); }
  // the bikes
  const top = phone ? sy + 40 : 80, left = side + 24, cols = phone ? 1 : Math.max(1, Math.floor((w - left - 24) / 230));
  const cw = (w - left - 24 - (cols - 1) * 16) / cols, ch = phone ? 150 : 190;
  const n = 3 + Math.floor(r * 6);
  for (let i = 0; i < n * cols; i++) {
    const x = left + (i % cols) * (cw + 16), y = top + Math.floor(i / cols) * (ch + 16);
    if (y > h) break;
    g.fillStyle = c.card; g.fillRect(x, y, cw, ch);
    g.fillStyle = c.line; g.fillRect(x + 12, y + 12, cw - 24, ch - 60);
    g.fillStyle = c.faint; g.fillRect(x + 12, y + ch - 36, cw * 0.5, 10);
  }
  // the caption and the size, so each picture says what it is
  const pad = 16, font = phone ? 15 : 20;
  g.font = `600 ${font}px sans-serif`;
  const lines = wrap(g, s.caption, w - 2 * pad - 24);
  const bh = lines.length * (font + 6) + 2 * pad;
  g.fillStyle = light ? "rgba(255,255,255,.92)" : "rgba(20,22,27,.92)";
  g.fillRect(12, h - bh - 12, w - 24, bh);
  g.fillStyle = c.text;
  lines.forEach((l, i) => g.fillText(l, 12 + pad, h - bh - 12 + pad + font + i * (font + 6) - 4));
  g.fillStyle = c.faint; g.font = "12px monospace";
  const size = `${s.width} × ${s.height}`;
  g.fillText(size, w - g.measureText(size).width - 16, 76);
  const blob = await canvas.convertToBlob({ type: "image/png" });
  return blob.arrayBuffer();
}

function wrap(g: OffscreenCanvasRenderingContext2D, text: string, width: number): string[] {
  const out: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/)) {
    const next = line ? `${line} ${word}` : word;
    if (line && g.measureText(next).width > width) { out.push(line); line = word; } else line = next;
  }
  if (line) out.push(line);
  return out;
}
