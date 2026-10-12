// GA-135 QA: the terminals' store (src/lib/terminals.ts) and the view's place (TerminalView's text), against TF0's
// acceptance criteria. The pure parts are tested as they are: the keys of research §5.2 on each system, http(s) links
// only, acknowledging every 100,000 bytes, the theme from the term-* tokens. The store runs against fakes: contract CT's
// calls are recorded, xterm.js and its addons are small stand-ins that keep what they are given, and there is no DOM, so
// the few elements the store makes are stand-ins too. React's useSyncExternalStore reads the snapshot, so the hooks can
// be called outside a component.
import { describe, expect, it, vi } from "vitest";
import type { TerminalSession } from "../types";
import type { TerminalPlace } from "./terminals";
import source from "./terminals.ts?raw";

type El = { children: El[]; parentElement: El | null; className: string; style: Record<string, string>; appendChild: (c: El) => El; remove: () => void; getContext: (kind: string, opts?: unknown) => unknown };
const h = vi.hoisted(() => {
  const s = {
    list: [] as unknown[],
    changed: null as null | ((list: unknown[]) => void),
    attached: [] as [string, (bytes: Uint8Array) => void][],
    acks: [] as [string, number][],
    inputs: [] as [string, string, boolean | undefined][],
    resizes: [] as [string, number, number][],
    clip: "pasted text",
    clipReads: 0,
    copied: [] as string[],
    opened: [] as string[],
    restyle: null as null | (() => void),
    tokens: {} as Record<string, string>,
    fitTo: { cols: 132, rows: 40 } as { cols: number; rows: number } | undefined,
    glAsked: [] as unknown[],
    terms: [] as FakeTerminal[],
    webgls: [] as FakeWebgl[],
    links: [] as FakeLinks[],
    /** The fonts measured on a canvas, and the glyph's height there per pixel of font size (the font's ascent + descent). */
    measured: [] as string[],
    glyph: 1.2,
  };
  class FakeTerminal {
    options: Record<string, unknown>;
    cols = 80;
    rows = 24;
    element: unknown = undefined;
    unicode = { activeVersion: "6" };
    addons: unknown[] = [];
    keys: ((e: unknown) => boolean) | null = null;
    data: ((d: string) => void) | null = null;
    written = 0;
    pasted: string[] = [];
    selection = "";
    disposed = false;
    opens: unknown[] = [];
    constructor(opts: Record<string, unknown>) { this.options = { ...opts }; s.terms.push(this); }
    loadAddon(a: unknown) { this.addons.push(a); }
    attachCustomKeyEventHandler(f: (e: unknown) => boolean) { this.keys = f; }
    onData(f: (d: string) => void) { this.data = f; return { dispose() {} }; }
    onBinary() { return { dispose() {} }; }
    write(b: Uint8Array, cb?: () => void) { this.written += b.length; cb?.(); }
    hasSelection() { return this.selection !== ""; }
    getSelection() { return this.selection; }
    clearSelection() { this.selection = ""; }
    paste(t: string) { this.pasted.push(t); }
    open(host: unknown) { this.opens.push(host); this.element ??= host; }
    resize(c: number, r: number) { this.cols = c; this.rows = r; }
    refresh() {}
    focus() {}
    dispose() { this.disposed = true; }
  }
  class FakeWebgl {
    lost: (() => void) | null = null;
    disposed = false;
    constructor() { s.webgls.push(this); }
    onContextLoss(f: () => void) { this.lost = f; return { dispose() {} }; }
    dispose() { this.disposed = true; }
  }
  class FakeLinks {
    constructor(public handler: (e: { ctrlKey: boolean; metaKey: boolean }, uri: string) => void) { s.links.push(this); }
  }
  class FakeFit { proposeDimensions() { return s.fitTo; } }
  class FakeSearch { clearDecorations() {} findNext() { return true; } findPrevious() { return true; } onDidChangeResults() { return { dispose() {} }; } }
  class FakeUnicode {}
  return { s, FakeTerminal, FakeWebgl, FakeLinks, FakeFit, FakeSearch, FakeUnicode };
});
const s = h.s;

vi.mock("react", async (orig) => ({ ...(await orig<typeof import("react")>()), useSyncExternalStore: (_sub: unknown, get: () => unknown) => get() }));
vi.mock("../api", () => ({
  terminalList: () => Promise.resolve(s.list),
  onTerminalsChanged: (cb: (list: unknown[]) => void) => { s.changed = cb; return Promise.resolve(() => {}); },
  terminalAttach: (id: string, onBytes: (b: Uint8Array) => void) => { s.attached.push([id, onBytes]); return Promise.resolve(); },
  terminalAck: (id: string, n: number) => { s.acks.push([id, n]); return Promise.resolve(); },
  terminalInput: (id: string, d: string, b?: boolean) => { s.inputs.push([id, d, b]); return Promise.resolve(); },
  terminalResize: (id: string, c: number, r: number) => { s.resizes.push([id, c, r]); return Promise.resolve(); },
  readClipboardText: () => { s.clipReads++; return Promise.resolve(s.clip); },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (u: string) => { s.opened.push(u); return Promise.resolve(); } }));
vi.mock("./appearance", () => ({ onAppearance: (f: () => void) => { s.restyle = f; return () => {}; } }));
vi.mock("@xterm/xterm", () => ({ Terminal: h.FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: h.FakeFit }));
vi.mock("@xterm/addon-search", () => ({ SearchAddon: h.FakeSearch }));
vi.mock("@xterm/addon-webgl", () => ({ WebglAddon: h.FakeWebgl }));
vi.mock("@xterm/addon-web-links", () => ({ WebLinksAddon: h.FakeLinks }));
vi.mock("@xterm/addon-unicode11", () => ({ Unicode11Addon: h.FakeUnicode }));

function el(): El {
  const e: El = {
    children: [], parentElement: null, className: "", style: {},
    appendChild(c) { c.remove(); c.parentElement = e; e.children.push(c); return c; },
    remove() { const p = e.parentElement; if (p) { p.children = p.children.filter((x) => x !== e); e.parentElement = null; } },
    getContext(kind, opts) { s.glAsked.push([kind, opts]); return kind === "webgl2" ? { getExtension: () => ({ loseContext() {} }) } : null; },
  };
  return e;
}
const TOKENS: Record<string, string> = {
  "--term-fg": "#e6e7ea", "--term-bg": "#0f1013", "--term-cursor": "#e6e7ea", "--term-cursor-text": "#0f1013",
  "--term-selection": "#283350", "--term-selection-inactive": "#1c2233", "--term-red": "#ff7b72", "--term-cyan": "#56b8f0",
  "--term-bright-white": "#fafafb", "--line-2": "#30333a", "--text-3": "#7f838d",
  "--font-mono": '"Atkinson Hyperlegible Mono", ui-monospace, monospace', "--ui-grow": "0px",
};
Object.assign(s.tokens, TOKENS);
vi.stubGlobal("document", { documentElement: {}, body: el(), createElement: () => el() });
vi.stubGlobal("getComputedStyle", () => ({ getPropertyValue: (n: string) => s.tokens[n] ?? "" }));
vi.stubGlobal("requestAnimationFrame", (f: (t: number) => void) => { f(0); return 0; });
vi.stubGlobal("devicePixelRatio", 1);
// A canvas measures "W" as xterm.js 6 does (CharSizeService's TextMetricsMeasureStrategy): ascent + descent of the font
vi.stubGlobal("OffscreenCanvas", class {
  getContext() {
    let font = "";
    return {
      get font() { return font; },
      set font(f: string) { font = f; s.measured.push(f); },
      measureText: () => {
        const size = parseFloat(font);
        return { width: size * 0.6, fontBoundingBoxAscent: size * s.glyph * 0.8, fontBoundingBoxDescent: size * s.glyph * 0.2 };
      },
    };
  }
});
/** A row as xterm.js 6 draws it (DomRenderer and WebglRenderer alike): floor(ceil(glyph × dpr) × lineHeight) device
 *  pixels, in CSS pixels. */
const rowPx = (fontSize: number, lineHeight: unknown, dpr = 1) => Math.floor(Math.ceil(fontSize * s.glyph * dpr) * Number(lineHeight)) / dpr;
vi.stubGlobal("navigator", {
  userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  clipboard: { writeText: (t: string) => { s.copied.push(t); return Promise.resolve(); } },
});

const T = await import("./terminals");
const { awayText } = await import("../components/terminal/TerminalView");

const session = (id: string, more: Partial<TerminalSession> = {}): TerminalSession => ({
  id, name: "gizai 2", folder: "/home/you/Herd/gizai", folderPath: "~/Herd/gizai", taskId: null, shell: "sh", busy: null,
  ended: null, cols: 80, rows: 24, openedAt: 1, ...more,
});
const key = (k: string, code: string, mods: { ctrl?: boolean; shift?: boolean; alt?: boolean; meta?: boolean } = {}) =>
  ({ key: k, code, ctrlKey: !!mods.ctrl, shiftKey: !!mods.shift, altKey: !!mods.alt, metaKey: !!mods.meta });
const keydown = (k: string, code: string, mods: Parameters<typeof key>[2] = {}) => {
  let prevented = false;
  return { ...key(k, code, mods), type: "keydown", preventDefault: () => { prevented = true; }, get prevented() { return prevented; } };
};
const flush = () => new Promise((r) => setTimeout(r, 0));
const PAGE: TerminalPlace = { kind: "page" };
const PANEL: TerminalPlace = { kind: "panel", folder: "/home/you/Herd/gizai" };

describe("the exports TF1 and TF2 use, word for word", () => {
  it("has TerminalPlace, useTerminals, showIn and useShownIn as the card writes them", () => {
    const lines = source.split("\n").map((l) => l.trim());
    expect(lines).toContain('export type TerminalPlace = { kind: "page" } | { kind: "panel"; folder: string };');
    expect(lines).toContain("export function useTerminals(): TerminalSession[] {");
    expect(lines).toContain("export function showIn(id: string, place: TerminalPlace): void {");
    expect(lines).toContain("export function useShownIn(id: string): TerminalPlace | null {");
  });
  it("names a place by its kind and folder", () => {
    expect(T.placeKey(PAGE)).toBe("page");
    expect(T.placeKey(PANEL)).toBe("panel:/home/you/Herd/gizai");
    expect(T.samePlace(PANEL, { kind: "panel", folder: "/home/you/Herd/gizai" })).toBe(true);
    expect(T.samePlace(PANEL, { kind: "panel", folder: "/home/you/Herd/other" })).toBe(false);
    expect(T.samePlace(PAGE, PANEL)).toBe(false);
    expect(T.samePlace(null, PAGE)).toBe(false);
  });
});

describe("the keys of research §5.2 (termKey)", () => {
  const C = (l: string) => `Key${l.toUpperCase()}`;
  it("knows the system from the web view's user agent", () => {
    expect(T.platformOf("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("mac");
    expect(T.platformOf("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/120")).toBe("windows");
    expect(T.platformOf("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("linux");
  });
  it("Linux: Ctrl+Shift+C copies, Ctrl+Shift+V and Shift+Insert paste through the plugin, Ctrl+Shift+F finds", () => {
    expect(T.termKey(key("C", C("c"), { ctrl: true, shift: true }), "linux", false)).toBe("copy");
    expect(T.termKey(key("V", C("v"), { ctrl: true, shift: true }), "linux", false)).toBe("paste");
    expect(T.termKey(key("Insert", "Insert", { shift: true }), "linux", false)).toBe("paste");
    expect(T.termKey(key("F", C("f"), { ctrl: true, shift: true }), "linux", false)).toBe("find");
    // Ctrl+C is the program's interrupt even with a selection, and Ctrl+V goes to the shell
    expect(T.termKey(key("c", C("c"), { ctrl: true }), "linux", true)).toBeNull();
    expect(T.termKey(key("v", C("v"), { ctrl: true }), "linux", false)).toBeNull();
  });
  it("Windows: Ctrl+C copies only with a selection, Ctrl+V and Shift+Insert paste, Ctrl+Shift+F finds", () => {
    expect(T.termKey(key("c", C("c"), { ctrl: true }), "windows", true)).toBe("copy");
    expect(T.termKey(key("c", C("c"), { ctrl: true }), "windows", false)).toBeNull();
    expect(T.termKey(key("C", C("c"), { ctrl: true, shift: true }), "windows", false)).toBe("copy");
    expect(T.termKey(key("v", C("v"), { ctrl: true }), "windows", false)).toBe("paste-native");
    expect(T.termKey(key("Insert", "Insert", { shift: true }), "windows", false)).toBe("paste-native");
    expect(T.termKey(key("F", C("f"), { ctrl: true, shift: true }), "windows", false)).toBe("find");
  });
  it("macOS: Cmd+C, Cmd+V and Cmd+F; Ctrl+C and Ctrl+F go to the shell", () => {
    expect(T.termKey(key("c", C("c"), { meta: true }), "mac", false)).toBe("copy");
    expect(T.termKey(key("v", C("v"), { meta: true }), "mac", false)).toBe("paste-native");
    expect(T.termKey(key("f", C("f"), { meta: true }), "mac", false)).toBe("find");
    expect(T.termKey(key("c", C("c"), { ctrl: true }), "mac", true)).toBeNull();
    expect(T.termKey(key("f", C("f"), { ctrl: true }), "mac", false)).toBeNull();
  });
  it("lets Gizai keep Ctrl+`, Ctrl+Shift+`, Ctrl+PgDn and Ctrl+PgUp, by the key's place for the backquote", () => {
    for (const p of ["linux", "windows", "mac"] as const) {
      expect(T.termKey(key("`", "Backquote", { ctrl: true }), p, false)).toBe("gizai");
      expect(T.termKey(key("~", "Backquote", { ctrl: true, shift: true }), p, false)).toBe("gizai");
      expect(T.termKey(key("½", "Backquote", { ctrl: true }), p, false)).toBe("gizai"); // another layout's character
      expect(T.termKey(key("PageDown", "PageDown", { ctrl: true }), p, false)).toBe("gizai");
      expect(T.termKey(key("PageUp", "PageUp", { ctrl: true }), p, false)).toBe("gizai");
    }
  });
  it("sends every other key to the shell: Ctrl+K, Ctrl+W, Ctrl+P, Ctrl+B, Ctrl+R, Escape, n, t and d", () => {
    for (const p of ["linux", "windows", "mac"] as const) {
      for (const l of ["k", "w", "p", "b", "r"]) expect([p, l, T.termKey(key(l, C(l), { ctrl: true }), p, false)]).toEqual([p, l, null]);
      for (const l of ["n", "t", "d"]) expect([p, l, T.termKey(key(l, C(l)), p, false)]).toEqual([p, l, null]);
      expect(T.termKey(key("Escape", "Escape"), p, false)).toBeNull();
    }
  });
  it("reads the letter by its key or its place, so Caps Lock and other layouts still copy and find", () => {
    expect(T.termKey(key("Ç", C("c"), { ctrl: true, shift: true }), "linux", false)).toBe("copy");
    expect(T.termKey(key("C", C("c"), { meta: true }), "mac", false)).toBe("copy");
  });
});

describe("links (isWebLink)", () => {
  it("opens only http and https", () => {
    expect(T.isWebLink("https://gizai.dev/docs")).toBe(true);
    expect(T.isWebLink("http://localhost:5173/")).toBe(true);
    for (const u of ["file:///etc/passwd", "javascript:alert(1)", "mailto:you@example.com", "ssh://host", "vscode://file/x", "data:text/html,x", "not a link", ""]) {
      expect([u, T.isWebLink(u)]).toEqual([u, false]);
    }
  });
});

describe("acknowledging what xterm.js wrote (ackCounter)", () => {
  it("tells the backend every 100,000 bytes, what it wrote since the last time", () => {
    const sent: number[] = [];
    const ack = T.ackCounter((n) => sent.push(n));
    ack(99_999);
    expect(sent).toEqual([]);
    ack(1);
    expect(sent).toEqual([100_000]);
    for (let i = 0; i < 7; i++) ack(30_000);
    expect(sent).toEqual([100_000, 120_000]);
    ack(250_000);
    expect(sent).toEqual([100_000, 120_000, 340_000]);
    expect(T.ACK_EVERY).toBe(100_000);
  });
});

describe("the theme from the term-* tokens (themeFrom)", () => {
  it("maps each of TD's 22 colours to xterm.js's, read resolved, and leaves out a token that is unset", () => {
    const termTokens = T.THEME_TOKENS.map(([, t]) => t).filter((t) => t.startsWith("term-"));
    expect(termTokens.length).toBe(22);
    const theme = T.themeFrom((t) => (t === "term-magenta" ? "" : ` #${t.length.toString(16).padStart(6, "0")} `));
    expect(theme.background).toBe(`#${"term-bg".length.toString(16).padStart(6, "0")}`);
    expect(theme).not.toHaveProperty("magenta");
    expect(Object.keys(theme)).toEqual(expect.arrayContaining(["foreground", "background", "cursor", "cursorAccent", "selectionBackground",
      "selectionInactiveBackground", "black", "red", "green", "yellow", "blue", "cyan", "white", "brightBlack", "brightRed", "brightGreen",
      "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"]));
  });
  it("uses the code size, 12.5px on 1.6 lines (20px), grown with the interface text; 5,000 lines of scrollback", () => {
    expect(T.fontSizeFor(0)).toBe(12.5);
    expect(T.fontSizeFor(2)).toBe(14.5);
    expect(T.LINE_HEIGHT).toBe(1.6);
    expect(T.rowHeightFor(12.5)).toBe(20);
    expect(T.rowHeightFor(14.5)).toBeCloseTo(23.2);
    expect(T.SCROLLBACK).toBe(5000);
  });
  it("lineHeightFor: xterm.js multiplies the glyph's height, so the rows are the code's lines for any font and screen", () => {
    // a row is floor(ceil(glyph × dpr) × lineHeight) device pixels; it must come out as the code's line, rounded to a pixel
    for (const dpr of [1, 1.25, 1.5, 2, 3]) {
      for (const size of [11.5, 12.5, 13.5, 14.5, 16.5]) {
        for (const ratio of [1, 1.1, 1.17, 1.2, 1.25, 1.33, 1.45]) {
          const glyph = size * ratio, row = T.rowHeightFor(size);
          const lh = T.lineHeightFor(glyph, row, dpr);
          expect(Math.floor(Math.ceil(glyph * dpr) * lh), `${size}px, glyph ${glyph}px, dpr ${dpr}`).toBe(Math.round(row * dpr));
          expect(lh, `${size}px, glyph ${glyph}px, dpr ${dpr}`).toBeGreaterThanOrEqual(1);
        }
      }
    }
    // 12.5px in a font whose glyph is 15px tall: 20px rows, not the 24px that lineHeight 1.6 would give
    expect(Math.floor(15 * T.lineHeightFor(15, 20, 1))).toBe(20);
    expect(Math.floor(15 * 1.6)).toBe(24);
    // at least 1, as xterm.js wants: a glyph taller than the line; and 1 when the glyph couldn't be measured
    expect(T.lineHeightFor(25, 20, 1)).toBe(1);
    expect(T.lineHeightFor(0, 20, 1)).toBe(1);
    expect(T.lineHeightFor(Number.NaN, 20, 1)).toBe(1);
    expect(T.lineHeightFor(15, 0, 1)).toBe(1);
  });
});

describe("the place's text (design §8)", () => {
  it("says where the terminal shows instead", () => {
    const g = session("t1", { name: "gizai 2", folderPath: "~/Herd/gizai" });
    expect(awayText(g, PAGE)).toBe("gizai 2 is on its own page.");
    expect(awayText(g, PANEL)).toBe("gizai 2 is in gizai's Editor.");
    expect(awayText(session("t2", { name: "GA-118", folderPath: "C:\\Users\\you\\.gizai\\worktrees\\GA\\GA-118\\" }), PANEL)).toBe("GA-118 is in GA-118's Editor.");
  });
});

describe("the store: one view per terminal, for the terminal's whole life", () => {
  const term = (i = 0) => s.terms[i]!;
  it("lists the terminals from terminalList and makes one view each, attached once", async () => {
    s.list = [session("t1")];
    T.startTerminals();
    T.startTerminals(); // once, however often it is called
    await vi.waitFor(() => expect(T.probeView("t1")).not.toBeNull());
    expect(T.useTerminals().map((x) => x.id)).toEqual(["t1"]);
    expect(T.useTerminalsLoaded()).toBe(true);
    expect(T.useViewMade("t1")).toBe(true);
    expect(s.terms.length).toBe(1);
    expect(s.attached.map(([id]) => id)).toEqual(["t1"]);
    // terminals-changed brings the same terminal again: no second view, no second attach
    s.changed!([session("t1", { busy: "npm" })]);
    await flush();
    expect(s.terms.length).toBe(1);
    expect(s.attached.length).toBe(1);
    expect(T.useTerminals()[0]?.busy).toBe("npm");
  });

  it("makes xterm.js as the card says: 5,000 lines, no blinking, the code font, the term-* colours, Unicode 11", () => {
    const o = term().options;
    expect(o.scrollback).toBe(5000);
    expect(o.cursorBlink).toBe(false);
    expect(o.fontSize).toBe(12.5);
    // the code's 20px lines, from the glyph measured as xterm.js measures it (15px here), not lineHeight 1.6
    expect(s.measured).toContain(`12.5px ${TOKENS["--font-mono"]}`);
    expect(rowPx(12.5, o.lineHeight)).toBe(20);
    expect(o.fontFamily).toBe(TOKENS["--font-mono"]);
    expect(o.customGlyphs).toBe(true); // box drawing lines up
    const theme = o.theme as Record<string, string>;
    expect([theme.background, theme.foreground, theme.red, theme.cyan, theme.brightWhite, theme.selectionBackground])
      .toEqual(["#0f1013", "#e6e7ea", "#ff7b72", "#56b8f0", "#fafafb", "#283350"]);
    expect(term().unicode.activeVersion).toBe("11");
    expect((o.linkHandler as { allowNonHttpProtocols: boolean }).allowNonHttpProtocols).toBe(false);
    expect(o.disableStdin).toBe(false);
  });

  it("takes in output while no place shows it, and acknowledges every 100,000 bytes xterm.js wrote", () => {
    expect(T.probeView("t1")?.renderer).toBe("none");
    const [, onBytes] = s.attached[0]!;
    for (let i = 0; i < 7; i++) onBytes(new Uint8Array(30_000));
    expect(term().written).toBe(210_000);
    expect(s.acks).toEqual([["t1", 120_000]]);
    onBytes(new Uint8Array(30_000));
    expect(s.acks).toEqual([["t1", 120_000], ["t1", 120_000]]);
  });

  it("sends what is typed to the shell", () => {
    term().data!("ls\r");
    expect(s.inputs).toContainEqual(["t1", "ls\r", undefined]);
  });

  it("showIn: the place that asked last shows the view; when it goes, the one before shows it again", () => {
    expect(T.useShownIn("t1")).toBeNull();
    T.showIn("t1", PAGE);
    expect(T.useShownIn("t1")).toEqual(PAGE);
    T.showIn("t1", PANEL);
    expect(T.useShownIn("t1")).toEqual(PANEL);
    T.showIn("t1", PAGE); // Show here on the page
    expect(T.useShownIn("t1")).toEqual(PAGE);
    T.leaveIn("t1", PAGE);
    expect(T.useShownIn("t1")).toEqual(PANEL);
    T.leaveIn("t1", PANEL);
    expect(T.useShownIn("t1")).toBeNull();
    T.showIn("t1", PAGE);
    expect(T.useShownIn("t1")).toEqual(PAGE);
  });

  let page: El, panel: El;
  it("puts the one view in its place, fits it, sends the size once, and draws it with WebGL on screen", async () => {
    page = el();
    s.glyph = 1.25; // the loaded font measures otherwise than the one there was when the view was made
    expect(await T.attachView("t1", page as unknown as HTMLElement)).toBe(true);
    expect(rowPx(12.5, term().options.lineHeight)).toBe(20); // measured again before it opens: still 20px rows
    const v = T.probeView("t1")!;
    expect(page.children).toEqual([v.host]);
    expect(term().opens).toEqual([v.host]);
    expect([term().cols, term().rows]).toEqual([132, 40]);
    expect(s.resizes).toEqual([["t1", 132, 40]]);
    T.fitView("t1");
    expect(s.resizes.length).toBe(1); // the same size isn't sent again
    expect(v.renderer).toBe("webgl");
    expect(s.webgls.length).toBe(1);
    // a hardware WebGL2 context: one with a major performance caveat (a software renderer) doesn't count
    expect(s.glAsked).toContainEqual(["webgl2", { failIfMajorPerformanceCaveat: true }]);
  });

  it("keeps 20px rows on a new pixel ratio (a zoom, another screen) when it fits again", () => {
    const before = term().options.lineHeight;
    expect(rowPx(12.5, before, 2)).not.toBe(20); // the old lineHeight would give 20.5px rows at 2×
    vi.stubGlobal("devicePixelRatio", 2);
    T.fitView("t1");
    expect(rowPx(12.5, term().options.lineHeight, 2)).toBe(20);
    vi.stubGlobal("devicePixelRatio", 1);
    T.fitView("t1");
    expect(term().options.lineHeight).toBe(before);
    expect(s.resizes.length).toBe(1); // the place kept its size
  });

  it("moves the same view to a second place, which frees the first place's WebGL context", async () => {
    T.showIn("t1", PANEL);
    panel = el();
    const v = T.probeView("t1")!;
    const gl = s.webgls[0]!;
    T.detachView("t1", page as unknown as HTMLElement); // the page's effect cleans up as it shows Show here
    expect(gl.disposed).toBe(true);
    expect(T.probeView("t1")?.renderer).toBe("none");
    s.fitTo = { cols: 60, rows: 12 };
    expect(await T.attachView("t1", panel as unknown as HTMLElement)).toBe(true);
    expect(s.terms.length).toBe(1); // the same xterm.js Terminal, with its buffer
    expect(page.children).toEqual([]);
    expect(panel.children).toEqual([v.host]);
    expect(s.resizes.at(-1)).toEqual(["t1", 60, 12]);
    expect(T.probeView("t1")?.renderer).toBe("webgl");
    expect(s.webgls.filter((g) => !g.disposed).length).toBe(1);
  });

  it("leaves the view where it is when a place that no longer has it cleans up late", async () => {
    T.showIn("t1", PAGE);
    s.fitTo = { cols: 132, rows: 40 };
    const v = T.probeView("t1")!;
    await T.attachView("t1", page as unknown as HTMLElement); // the page attaches before the panel's cleanup ran
    T.detachView("t1", panel as unknown as HTMLElement);
    expect(page.children).toEqual([v.host]);
    expect(T.probeView("t1")?.renderer).not.toBe("none");
  });

  it("goes on with the DOM renderer when the WebGL context is lost, or when WebGL is off", () => {
    const gl = s.webgls.filter((g) => !g.disposed).at(-1)!;
    expect(T.probeView("t1")?.renderer).toBe("webgl");
    gl.lost!();
    expect(gl.disposed).toBe(true);
    expect(T.probeView("t1")?.renderer).toBe("dom");
    T.setWebglAllowed(true);
    return vi.waitFor(() => expect(T.probeView("t1")?.renderer).toBe("webgl")).then(() => {
      T.setWebglAllowed(false);
      expect(T.probeView("t1")?.renderer).toBe("dom");
      expect(T.hardwareWebgl2()).toBe(false);
      T.setWebglAllowed(true);
    });
  });

  it("copies with Ctrl+Shift+C, pastes Ctrl+Shift+V through the clipboard plugin, and lets every other key through (Linux)", async () => {
    await vi.waitFor(() => expect(T.probeView("t1")?.renderer).toBe("webgl"));
    const keys = term().keys!;
    term().selection = "gizai-probe";
    const copy = keydown("C", "KeyC", { ctrl: true, shift: true });
    expect(keys(copy)).toBe(false);
    expect(copy.prevented).toBe(true);
    expect(s.copied).toEqual(["gizai-probe"]);

    const paste = keydown("V", "KeyV", { ctrl: true, shift: true });
    expect(keys(paste)).toBe(false);
    expect(paste.prevented).toBe(true); // WebKit's own paste (and its prompt) doesn't happen
    await flush();
    expect(s.clipReads).toBe(1);
    expect(term().pasted).toEqual(["pasted text"]);
    // its keypress and keyup are kept from the shell too, and don't read the clipboard again
    expect(keys({ ...key("V", "KeyV", { ctrl: true, shift: true }), type: "keypress", preventDefault() {} })).toBe(false);
    expect(s.clipReads).toBe(1);

    // find bubbles up to the place; Ctrl+C (with a selection, on Linux), Ctrl+K and n reach the shell
    expect(keys(keydown("F", "KeyF", { ctrl: true, shift: true }))).toBe(false);
    expect(keys(keydown("c", "KeyC", { ctrl: true }))).toBe(true);
    expect(keys(keydown("k", "KeyK", { ctrl: true }))).toBe(true);
    expect(keys(keydown("n", "KeyN"))).toBe(true);
    s.clip = "";
    keys(keydown("V", "KeyV", { ctrl: true, shift: true }));
    await flush();
    expect(term().pasted).toEqual(["pasted text"]); // an empty clipboard pastes nothing
  });

  it("opens http(s) links with Ctrl+click only, and nothing else", () => {
    const open = s.links[0]!.handler;
    const activate = (term().options.linkHandler as { activate: typeof open }).activate;
    for (const f of [open, activate]) {
      f({ ctrlKey: false, metaKey: false }, "https://gizai.dev");
      f({ ctrlKey: true, metaKey: false }, "file:///etc/passwd");
      f({ ctrlKey: true, metaKey: false }, "javascript:alert(1)");
    }
    expect(s.opened).toEqual([]);
    open({ ctrlKey: true, metaKey: false }, "https://gizai.dev/a");
    activate({ ctrlKey: true, metaKey: false }, "http://localhost:8000/");
    expect(s.opened).toEqual(["https://gizai.dev/a", "http://localhost:8000/"]);
  });

  it("recolours at once when the theme changes, and refonts and fits again when Settings → Appearance changes", async () => {
    s.tokens["--term-bg"] = "#ffffff";
    s.tokens["--term-red"] = "#b3261e";
    s.restyle!();
    const theme = term().options.theme as Record<string, string>;
    expect([theme.background, theme.red]).toEqual(["#ffffff", "#b3261e"]); // before any await
    s.tokens["--font-mono"] = '"JetBrains Mono", monospace';
    s.tokens["--ui-grow"] = "2px";
    s.fitTo = { cols: 110, rows: 33 };
    s.restyle!();
    await vi.waitFor(() => expect(term().options.fontSize).toBe(14.5));
    expect(term().options.fontFamily).toBe('"JetBrains Mono", monospace');
    // the rows grow with the text: 1.6 times 14.5px, measured in the new font
    expect(s.measured).toContain('14.5px "JetBrains Mono", monospace');
    expect(rowPx(14.5, term().options.lineHeight)).toBe(23);
    await vi.waitFor(() => expect(s.resizes.at(-1)).toEqual(["t1", 110, 33]));
  });

  it("puts a just-opened terminal in the list at once, and stops typing into an ended one", async () => {
    T.noteOpened(session("t2", { name: "gizai 3" }));
    expect(T.useTerminals().map((x) => x.id)).toEqual(["t1", "t2"]);
    await vi.waitFor(() => expect(T.useViewMade("t2")).toBe(true));
    expect(s.attached.map(([id]) => id)).toEqual(["t1", "t2"]);
    s.changed!([session("t1", { ended: { code: 0, at: 2, why: null } }), session("t2", { name: "gizai 3" })]);
    expect(term(0).options.disableStdin).toBe(true);
    expect(term(1).options.disableStdin).toBe(false);
  });

  it("disposes of a view, its WebGL context and its place when its terminal leaves the list", () => {
    const v = T.probeView("t1")!;
    s.changed!([session("t2", { name: "gizai 3" })]);
    expect(term(0).disposed).toBe(true);
    expect(T.probeView("t1")).toBeNull();
    expect(T.useShownIn("t1")).toBeNull();
    expect(page.children).not.toContain(v.host);
    expect(s.webgls.every((g) => g.disposed)).toBe(true);
    expect(term(1).disposed).toBe(false);
  });
});
