// The terminals (GA-135, TF0; docs/research/terminal.md §5.3): the list of open terminals, and one xterm.js view per
// terminal for the terminal's whole life. Started once by App, not by a page, because pages unmount when you leave them.
// - The list comes from terminalList and the terminals-changed event (contract CT).
// - A terminal's view is made when it appears in the list and attached to its output once: it takes in everything the
//   shell prints, wherever you are, and acknowledges it every 100,000 bytes xterm.js has written (the backend's flow
//   control). It is disposed of when the terminal leaves the list.
// - A place (the Terminal page, a folder's panel in the Editor) shows a view by moving its element in. The place that
//   asked last shows it; the others show "Show here" (design §5). Moving the element keeps the buffer, the scrollback,
//   the cursor and the selection. Only a view on screen holds a WebGL context.
// - Colours are the term-* tokens, read with getComputedStyle; the font is --font-mono at the code size, on the code's
//   lines (20px at 12.5px). All follow the theme (t) and Settings → Appearance at once.
// xterm.js and its addons load with the first terminal, in a chunk of their own.
import { useSyncExternalStore } from "react";
import type { ITheme, Terminal } from "@xterm/xterm";
import type { FitAddon } from "@xterm/addon-fit";
import type { ISearchOptions, SearchAddon } from "@xterm/addon-search";
import type { WebglAddon } from "@xterm/addon-webgl";
import { openUrl } from "@tauri-apps/plugin-opener";
import { onTerminalsChanged, readClipboardText, terminalAck, terminalAttach, terminalInput, terminalList, terminalResize } from "../api";
import type { TerminalSession } from "../types";
import { onAppearance } from "./appearance";
import { modClick } from "./keys";
import { hidden, onShown } from "./windowFocus";

/** Where a terminal can show: the Terminal page, or the terminal panel of a folder in the Editor (its EditorFolder id). */
export type TerminalPlace = { kind: "page" } | { kind: "panel"; folder: string };

export const placeKey = (p: TerminalPlace) => (p.kind === "page" ? "page" : `panel:${p.folder}`);
export const samePlace = (a: TerminalPlace | null, b: TerminalPlace | null) => !!a && !!b && placeKey(a) === placeKey(b);

// ---------- pure parts (tested in Node) ----------

/** xterm.js's write callback adds up what it wrote; every 100,000 bytes the backend hears it (terminalAck). */
export const ACK_EVERY = 100_000;
export function ackCounter(send: (bytes: number) => void): (bytes: number) => void {
  let written = 0;
  return (bytes) => {
    written += bytes;
    if (written >= ACK_EVERY) { send(written); written = 0; }
  };
}

export type Platform = "mac" | "windows" | "linux";
/** The system, from the web view's user agent (WKWebView names the Mac, WebView2 Windows; WebKitGTK is the rest). */
export function platformOf(userAgent: string): Platform {
  return /Macintosh|Mac OS X/.test(userAgent) ? "mac" : /Windows/.test(userAgent) ? "windows" : "linux";
}
const platform = (): Platform => platformOf(typeof navigator === "undefined" ? "" : navigator.userAgent || "");

/** What a key in a terminal does, by the table of research §5.2:
 *  - "copy" the selection; "paste" reads the clipboard through the plugin (Linux: WebKitGTK has no paste behind
 *    Ctrl+Shift+V, and navigator.clipboard would ask first); "paste-native" lets the web view paste (Cmd+V, Ctrl+V on
 *    Windows), which xterm.js takes from its text field;
 *  - "find" opens find; "gizai" is a key Gizai keeps (Ctrl+`, Ctrl+Shift+`, Ctrl+PgDn and Ctrl+PgUp);
 *  - null: the key goes to the shell. Ctrl+C without a selection is the program's interrupt. */
export type TermKey = "copy" | "paste" | "paste-native" | "find" | "gizai" | null;
export type KeyLike = { key: string; code: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean };
export function termKey(e: KeyLike, p: Platform, hasSelection: boolean): TermKey {
  const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const ctrl = e.ctrlKey && !e.metaKey && !e.altKey;
  const cmd = e.metaKey && !e.ctrlKey && !e.altKey;
  const letter = (c: string) => k === c || e.code === `Key${c.toUpperCase()}`;
  if (ctrl && e.code === "Backquote") return "gizai";
  if (ctrl && !e.shiftKey && (e.key === "PageDown" || e.key === "PageUp")) return "gizai";
  if (p === "mac") {
    if (cmd && !e.shiftKey && letter("c")) return "copy";
    if (cmd && !e.shiftKey && letter("v")) return "paste-native";
    if (cmd && !e.shiftKey && letter("f")) return "find";
    return null;
  }
  if (ctrl && e.shiftKey && letter("f")) return "find";
  if (ctrl && e.shiftKey && letter("c")) return "copy";
  if (p === "windows" && ctrl && !e.shiftKey && letter("c") && hasSelection) return "copy";
  const shiftInsert = e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey && e.key === "Insert";
  if (p === "linux") return (ctrl && e.shiftKey && letter("v")) || shiftInsert ? "paste" : null;
  return (ctrl && letter("v")) || shiftInsert ? "paste-native" : null;
}

/** Only http and https links open, and only with Ctrl+click (Cmd+click on macOS). */
export function isWebLink(uri: string): boolean {
  try {
    const u = new URL(uri);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch { return false; }
}

/** xterm.js's theme from the term-* tokens (design §6 and §7); `read` gives a token's value, resolved. */
export const THEME_TOKENS: [keyof ITheme, string][] = [
  ["foreground", "term-fg"], ["background", "term-bg"], ["cursor", "term-cursor"], ["cursorAccent", "term-cursor-text"],
  ["selectionBackground", "term-selection"], ["selectionInactiveBackground", "term-selection-inactive"],
  ["black", "term-black"], ["red", "term-red"], ["green", "term-green"], ["yellow", "term-yellow"], ["blue", "term-blue"],
  ["magenta", "term-magenta"], ["cyan", "term-cyan"], ["white", "term-white"],
  ["brightBlack", "term-bright-black"], ["brightRed", "term-bright-red"], ["brightGreen", "term-bright-green"],
  ["brightYellow", "term-bright-yellow"], ["brightBlue", "term-bright-blue"], ["brightMagenta", "term-bright-magenta"],
  ["brightCyan", "term-bright-cyan"], ["brightWhite", "term-bright-white"],
  ["scrollbarSliderBackground", "line-2"], ["scrollbarSliderHoverBackground", "text-3"], ["scrollbarSliderActiveBackground", "text-3"],
];
export function themeFrom(read: (token: string) => string): ITheme {
  const theme: Record<string, string> = {};
  for (const [key, token] of THEME_TOKENS) { const v = read(token).trim(); if (v) theme[key] = v; }
  return theme as ITheme;
}

/** The code size (t-code, 12.5px on 20px lines), grown with the Interface text like the code. */
export const fontSizeFor = (uiGrow: number) => 12.5 + uiGrow;
/** The code's lines as CSS has them: 1.6 times the font size, so 20px rows at 12.5px. */
export const LINE_HEIGHT = 1.6;
export const rowHeightFor = (fontSize: number) => fontSize * LINE_HEIGHT;
/** xterm.js's lineHeight multiplies the glyph's height, not the font size: a row is floor(ceil(glyph × dpr) × lineHeight)
 *  device pixels. This is the lineHeight that makes a row `row` CSS pixels (20 at 12.5px, so the design's 4 lines fit the
 *  panel's least height); at least 1, as xterm.js wants, and 1 when the glyph couldn't be measured. */
export function lineHeightFor(glyph: number, row: number, dpr: number): number {
  const g = Math.ceil(glyph * dpr);
  if (!(g > 0) || !(row > 0)) return 1;
  return Math.max(1, (Math.round(row * dpr) + 0.5) / g); // + 0.5: xterm.js floors, so a rounding error can't cost a pixel
}
export const SCROLLBACK = 5000;

// ---------- the words of the sidebar's rows and the page (GA-136, TF1; design §1, §3 and §8) ----------

type Shown = Pick<TerminalSession, "name" | "folderPath" | "shell" | "busy" | "ended">;

/** The folder's own name, as the Editor names it ("gizai", or a card's identifier for its worktree). */
export const folderNameOf = (s: Pick<TerminalSession, "folderPath">) => s.folderPath.split(/[\\/]/).filter(Boolean).pop() ?? s.folderPath;

/** What it does now, for a tooltip: "running npm", "waiting for a command", or why it ended ("zsh ended with exit code 2"). */
export function doingWords(s: Shown): string {
  if (s.ended) return s.ended.why ?? (s.ended.code === null ? `${s.shell} was ended by a signal` : `${s.shell} ended with exit code ${s.ended.code}`);
  return s.busy ? `running ${s.busy}` : "waiting for a command";
}

/** A sidebar row's tooltip: "~/Herd/gizai · zsh · running npm"; for a card's worktree "KADE-41's worktree · …"; once ended
 *  "KADE-41's worktree · zsh ended with exit code 2". */
export function rowTitle(s: Shown, card: string | null): string {
  const where = card ? `${card}'s worktree` : s.folderPath;
  return s.ended ? `${where} · ${doingWords(s)}` : `${where} · ${s.shell} · ${doingWords(s)}`;
}

/** A tab's tooltip: "gizai 2 · running npm", "gizai · waiting for a command", "gizai 3 · ended". */
export const tabTitle = (s: Shown) => `${s.name} · ${s.ended ? "ended" : s.busy ? `running ${s.busy}` : "waiting for a command"}`;

/** Closing asks first only while a program runs in it; one that waits for a command, or has ended, closes at once. */
export const asksToClose = (s: Pick<TerminalSession, "busy" | "ended">) => !s.ended && !!s.busy;
/** The question under a row or a tab: "npm is still running in gizai 2. Close it and end npm?" */
export const closeQuestion = (s: Pick<TerminalSession, "name" | "busy">) => `${s.busy} is still running in ${s.name}. Close it and end ${s.busy}?`;

/** The bar of a terminal whose shell ended with another code than 0 (0 closes it): "The shell ended with exit code 2." */
export function endedSay(end: TerminalSession["ended"]): string {
  if (!end) return "";
  if (end.code !== null) return `The shell ended with exit code ${end.code}.`;
  return !end.why || / was ended by /.test(end.why) ? "The shell was ended by a signal." : sentence(end.why);
}

/** A message as a sentence, with its full stop ("Couldn't start zsh in ~/Herd/gizai: /usr/bin/zsh wasn't found."). */
export function sentence(text: string): string {
  const t = text.trim();
  return !t || /[.!?…]$/.test(t) ? t : `${t}.`;
}

/** Folded Terminal's heading: "3 open", with "3 terminals are open" as its tooltip. */
export const openWord = (n: number) => `${n} open`;
export const openTip = (n: number) => (n === 1 ? "1 terminal is open" : `${n} terminals are open`);

/** From this many open terminals, New terminal says how many are open, and that each keeps its shell running. */
export const MANY_OPEN = 20;
export const manyNote = (n: number) => (n >= MANY_OPEN ? `${n} terminals are open: each keeps its shell running until you close it.` : null);

/** A card's worktree shows its identifier in mono: the name as it starts there ("KADE-41", "KADE-41 2"), not one given. */
export const showsCardId = (name: string, identifier: string) => name === identifier || name.startsWith(`${identifier} `);

/** The terminal `by` tabs on from `id` among `ids` (Ctrl+PgDn and Ctrl+PgUp, ← and →), round from the last to the first. */
export function stepTerminal(ids: string[], id: string, by: 1 | -1): string | null {
  if (!ids.length) return null;
  const i = ids.indexOf(id);
  return ids[(Math.max(i, by > 0 ? -1 : 0) + by + ids.length) % ids.length] ?? null;
}

/** The tab to show when the one on the page closed: the next in the folder, else the one before it, else none. `was` is
 *  the folder's tabs when it was there, `now` the ids open now. */
export function nextAfterClose(was: string[], id: string, now: string[]): string | null {
  const i = was.indexOf(id);
  const order = i < 0 ? was : [...was.slice(i + 1), ...was.slice(0, i).reverse()];
  return order.find((x) => now.includes(x)) ?? null;
}

// ---------- the store ----------

type Xterm = {
  Terminal: typeof import("@xterm/xterm").Terminal;
  FitAddon: typeof import("@xterm/addon-fit").FitAddon;
  SearchAddon: typeof import("@xterm/addon-search").SearchAddon;
  WebglAddon: typeof import("@xterm/addon-webgl").WebglAddon;
  WebLinksAddon: typeof import("@xterm/addon-web-links").WebLinksAddon;
  Unicode11Addon: typeof import("@xterm/addon-unicode11").Unicode11Addon;
};
type View = {
  id: string;
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  /** The view's own element: this is what moves between places. */
  host: HTMLDivElement;
  /** The place's element it is in now, and the one it is on its way to (fonts loading). */
  el: HTMLElement | null;
  want: HTMLElement | null;
  webgl: WebglAddon | null;
  /** The size the backend was last told. */
  sent: { cols: number; rows: number };
  /** The screen's pixel ratio the rows were made for (a zoom or another screen changes it). */
  dpr: number;
  /** Output that came while no place showed it or the window was hidden, not written to xterm.js yet (GA-190). */
  held: Uint8Array[];
  heldBytes: number;
  /** Counts what xterm.js wrote, for the backend's flow control. */
  ack: (bytes: number) => void;
};

let sessions: TerminalSession[] = [];
let loaded = false;
const views = new Map<string, View>();
const making = new Set<string>();
/** Per terminal: the places that asked for it, oldest first; the last one shows it. */
const asked = new Map<string, TerminalPlace[]>();
const shown = new Map<string, TerminalPlace>();
const subs = new Set<() => void>();
const changed = () => subs.forEach((f) => f());
const subscribe = (f: () => void) => { subs.add(f); return () => { subs.delete(f); }; };

let xterm: Promise<Xterm> | null = null;
function loadXterm(): Promise<Xterm> {
  xterm ??= Promise.all([
    import("@xterm/xterm"), import("@xterm/addon-fit"), import("@xterm/addon-search"), import("@xterm/addon-webgl"),
    import("@xterm/addon-web-links"), import("@xterm/addon-unicode11"),
  ]).then(([x, fit, search, webgl, links, unicode]) => ({
    Terminal: x.Terminal, FitAddon: fit.FitAddon, SearchAddon: search.SearchAddon, WebglAddon: webgl.WebglAddon,
    WebLinksAddon: links.WebLinksAddon, Unicode11Addon: unicode.Unicode11Addon,
  }));
  return xterm;
}

/** The tokens and the font as they are on <html> now. */
function looks() {
  const css = getComputedStyle(document.documentElement);
  const read = (token: string) => css.getPropertyValue(`--${token}`);
  const family = read("font-mono").trim() || "monospace";
  return { theme: themeFrom(read), fontFamily: family, fontSize: fontSizeFor(parseFloat(read("ui-grow")) || 0) };
}

/** The glyph's height in CSS pixels, measured as xterm.js measures it: the font's ascent and descent on a canvas, else the
 *  height of a span of W's. 0 when it can't be measured. */
function glyphHeight(family: string, size: number): number {
  try {
    const ctx = new OffscreenCanvas(100, 100).getContext("2d");
    if (ctx) {
      ctx.font = `${size}px ${family}`;
      const m = ctx.measureText("W");
      if ("fontBoundingBoxAscent" in m && "fontBoundingBoxDescent" in m) return Math.max(0, m.fontBoundingBoxAscent + m.fontBoundingBoxDescent);
    }
  } catch { /* no OffscreenCanvas: xterm.js measures a span too */ }
  try {
    const span = document.createElement("span");
    span.textContent = "W".repeat(32);
    span.style.cssText = "position:absolute;top:0;left:-9999em;visibility:hidden;display:inline-block;line-height:normal;white-space:pre;font-kerning:none";
    span.style.fontFamily = family;
    span.style.fontSize = `${size}px`;
    document.body.appendChild(span);
    const h = Number(span.offsetHeight);
    span.remove();
    return h > 0 ? h : 0;
  } catch { return 0; }
}
const dprNow = () => globalThis.devicePixelRatio || 1;
/** The lineHeight that gives this font the code's rows on this screen. */
const lineHeightOf = (family: string, size: number) => lineHeightFor(glyphHeight(family, size), rowHeightFor(size), dprNow());

/** Waits for the mono font, so xterm.js measures its cells in it (it measures once, when it opens or the font changes). */
function fontReady(family: string, size: number): Promise<void> {
  if (typeof document === "undefined" || !document.fonts) return Promise.resolve();
  const load = (weight: string) => document.fonts.load(`${weight} ${size}px ${family}`).catch(() => []);
  return Promise.all([load("400"), load("700")]).then(() => undefined);
}

let started = false;
/** Starts the store: the list now and on every change, and the looks on every appearance change. Once, by App. */
export function startTerminals() {
  if (started) return;
  started = true;
  let heard = false;
  onTerminalsChanged((list) => { heard = true; setSessions(list); }).catch(() => {});
  terminalList().then((list) => { if (!heard) setSessions(list); }).catch(() => { if (!heard) setSessions([]); });
  onAppearance(() => restyle());
  onShown(releaseShown);
}

function setSessions(list: TerminalSession[]) {
  sessions = list;
  loaded = true;
  const ids = new Set(list.map((s) => s.id));
  for (const id of [...views.keys()]) if (!ids.has(id)) drop(id);
  for (const s of list) {
    const v = views.get(s.id);
    if (v) v.term.options.disableStdin = !!s.ended;
    else make(s);
  }
  changed();
}

/** A terminal just opened (terminalOpen's answer): in the list at once, before terminals-changed brings it. */
export function noteOpened(s: TerminalSession) {
  if (sessions.some((x) => x.id === s.id)) return;
  setSessions([...sessions, s]);
}

/** A terminal just changed (terminalRename's answer): its row, tab and page show it at once. */
export function noteChanged(s: TerminalSession) {
  if (!sessions.some((x) => x.id === s.id)) return;
  setSessions(sessions.map((x) => (x.id === s.id ? s : x)));
}

function make(s: TerminalSession) {
  if (making.has(s.id)) return;
  making.add(s.id);
  loadXterm().then((x) => {
    making.delete(s.id);
    if (views.has(s.id) || !sessions.some((y) => y.id === s.id)) return;
    const { theme, fontFamily, fontSize } = looks();
    const open = (e: MouseEvent, uri: string) => { if (modClick(e) && isWebLink(uri)) openUrl(uri).catch(() => {}); };
    const term = new x.Terminal({
      allowProposedApi: true, // the Unicode 11 widths and find's marks
      scrollback: SCROLLBACK, cursorBlink: false, cursorStyle: "block", cursorInactiveStyle: "outline",
      fontFamily, fontSize, lineHeight: lineHeightOf(fontFamily, fontSize), fontWeight: 400, fontWeightBold: 700, drawBoldTextInBrightColors: true,
      minimumContrastRatio: 4.5, customGlyphs: true, rescaleOverlappingGlyphs: true, theme,
      // OSC 8 links (a program marks text as a link): the same rule as the links found in the text
      linkHandler: { activate: open, allowNonHttpProtocols: false },
      disableStdin: !!s.ended,
    });
    const fit = new x.FitAddon();
    const search = new x.SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new x.WebLinksAddon(open));
    term.loadAddon(new x.Unicode11Addon());
    term.unicode.activeVersion = "11";
    const host = document.createElement("div");
    host.className = "term-host";
    const ack = ackCounter((n) => { terminalAck(s.id, n).catch(() => {}); });
    const v: View = { id: s.id, term, fit, search, host, el: null, want: null, webgl: null, sent: { cols: s.cols, rows: s.rows }, dpr: dprNow(), held: [], heldBytes: 0, ack };
    term.attachCustomKeyEventHandler((e) => keyIn(v, e));
    term.onData((d) => { terminalInput(s.id, d).catch(() => {}); });
    term.onBinary((d) => { terminalInput(s.id, d, true).catch(() => {}); });
    views.set(s.id, v);
    // out of sight its output waits (take), and xterm.js keeps what is written in its buffer until a place first shows it
    terminalAttach(s.id, (bytes) => take(v, bytes)).catch(() => {});
    changed();
  }).catch(() => { making.delete(s.id); });
}

// ---------- out of sight: the output waits (GA-190; research docs/research/performance.md F7, P9) ----------

/** How much output a terminal out of sight keeps before xterm.js takes it all the same: well under what the backend lets
 *  wait for xterm.js (500,000 bytes, terminalAck), so the program in it never waits for it. */
export const HOLD_MAX = 256_000;

/** A place shows the view, and the window isn't hidden. */
const inSight = (v: View) => !!v.el && !hidden();

const write = (v: View, bytes: Uint8Array) => v.term.write(bytes, () => v.ack(bytes.length));

/** Output from the backend: written at once while the view is in sight; else it waits, until the view shows again or
 *  HOLD_MAX bytes wait. Nothing is lost, and it comes in its order. */
function take(v: View, bytes: Uint8Array) {
  if (!v.held.length && inSight(v)) { write(v, bytes); return; }
  v.held.push(bytes);
  v.heldBytes += bytes.length;
  if (v.heldBytes >= HOLD_MAX || inSight(v)) release(v);
}

/** Writes what waited, in one piece. */
function release(v: View) {
  if (!v.held.length) return;
  const all = new Uint8Array(v.heldBytes);
  let at = 0;
  for (const b of v.held) { all.set(b, at); at += b.length; }
  v.held = [];
  v.heldBytes = 0;
  write(v, all);
}

/** The window shows again: each terminal in a place draws what came meanwhile. */
function releaseShown() {
  for (const v of views.values()) if (inSight(v)) release(v);
}

function drop(id: string) {
  const v = views.get(id);
  if (!v) return;
  dropWebgl(v);
  v.term.dispose();
  v.host.remove();
  views.delete(id);
  asked.delete(id);
  shown.delete(id);
}

/** The keys of research §5.2: copy, paste and find here, Gizai's own keys let through, every other key to the shell. */
function keyIn(v: View, e: KeyboardEvent): boolean {
  const what = termKey(e, platform(), v.term.hasSelection());
  if (!what) return true;
  if (e.type !== "keydown") return false;
  if (what === "copy") {
    e.preventDefault();
    copyText(v.term.getSelection());
  } else if (what === "paste") {
    e.preventDefault();
    readClipboardText().then((text) => { if (text) v.term.paste(text); }).catch(() => {});
  }
  // paste-native: the web view pastes into xterm.js's text field; find and Gizai's keys: the place and the page take
  // them as they bubble up
  return false;
}

function copyText(text: string) {
  if (!text) return;
  navigator.clipboard?.writeText(text).catch(() => {
    const t = document.createElement("textarea");
    t.value = text;
    t.style.position = "fixed";
    t.style.opacity = "0";
    document.body.appendChild(t);
    t.select();
    try { document.execCommand("copy"); } catch { /* nothing more to try */ }
    t.remove();
  });
}

// ---------- WebGL: only where a hardware WebGL2 context exists, and only on screen ----------

let webgl2: boolean | null = null;
let webglAllowed = true;
/** A WebGL2 context without a major performance caveat (a software renderer would drop characters, research §3.2 Q2). */
export function hardwareWebgl2(): boolean {
  if (!webglAllowed) return false;
  if (webgl2 === null) {
    try {
      const gl = document.createElement("canvas").getContext("webgl2", { failIfMajorPerformanceCaveat: true });
      webgl2 = !!gl;
      gl?.getExtension("WEBGL_lose_context")?.loseContext();
    } catch { webgl2 = false; }
  }
  return webgl2;
}

/** The probe's switch: with WebGL off every view draws with the DOM renderer. */
export function setWebglAllowed(on: boolean) {
  webglAllowed = on;
  for (const v of views.values()) {
    if (!on) dropWebgl(v);
    else if (v.el) loadWebgl(v);
  }
}

function loadWebgl(v: View) {
  if (v.webgl || !v.term.element || !hardwareWebgl2()) return;
  loadXterm().then((x) => {
    if (v.webgl || !v.el || !views.has(v.id) || !hardwareWebgl2()) return;
    try {
      const gl = new x.WebglAddon();
      // a lost context (a GPU reset, the computer sleeping): the DOM renderer goes on with the same screen
      gl.onContextLoss(() => { if (v.webgl === gl) dropWebgl(v); });
      v.term.loadAddon(gl);
      v.webgl = gl;
    } catch { v.webgl = null; }
  }).catch(() => {});
}

function dropWebgl(v: View) {
  const gl = v.webgl;
  v.webgl = null;
  try { gl?.dispose(); } catch { /* already gone with its context */ }
}

// ---------- places ----------
// (The hooks below give the same snapshot on a server render, so a page rendered to HTML, as tests do, has the list.)

/** The place that asked last shows the view (design §5): a place asks when it shows, and Show here asks again. */
export function showIn(id: string, place: TerminalPlace): void {
  const list = (asked.get(id) ?? []).filter((p) => !samePlace(p, place));
  list.push(place);
  asked.set(id, list);
  if (!samePlace(shown.get(id) ?? null, place)) { shown.set(id, place); changed(); }
}

/** A place goes (the page is left, the panel hidden): the place that asked before it shows the view again, if any. */
export function leaveIn(id: string, place: TerminalPlace): void {
  const list = (asked.get(id) ?? []).filter((p) => !samePlace(p, place));
  if (list.length) asked.set(id, list);
  else asked.delete(id);
  if (!samePlace(shown.get(id) ?? null, place)) return;
  const back = list[list.length - 1];
  if (back) shown.set(id, back);
  else shown.delete(id);
  changed();
}

/** Every open terminal, in the order they were opened. */
export function useTerminals(): TerminalSession[] {
  return useSyncExternalStore(subscribe, () => sessions, () => sessions);
}
/** Whether the list has come from the backend yet (before that, an address of a terminal can't be called closed). */
export function useTerminalsLoaded(): boolean {
  return useSyncExternalStore(subscribe, () => loaded, () => loaded);
}
/** Where the terminal's view shows now, or null. */
export function useShownIn(id: string): TerminalPlace | null {
  return useSyncExternalStore(subscribe, () => shown.get(id) ?? null, () => shown.get(id) ?? null);
}
/** The terminals a place shows now, one id per line ("" for none): a string, so it compares by value. The Editor's panel
 *  opens on a terminal Show in Editor put there (GA-137). */
export function useShownHere(place: TerminalPlace): string {
  const key = placeKey(place);
  return useSyncExternalStore(subscribe, () => [...shown].filter(([, p]) => placeKey(p) === key).map(([id]) => id).join("\n"));
}
/** Whether the terminal's view is made (xterm.js loaded and attached), so a place can put it in. */
export function useViewMade(id: string): boolean {
  return useSyncExternalStore(subscribe, () => views.has(id), () => views.has(id));
}

/** Puts the view in the place's element: the first time it opens there (after the font has loaded); then it only moves.
 *  WebGL comes with it, the size is fitted and sent. */
export function attachView(id: string, el: HTMLElement): Promise<boolean> {
  const v = views.get(id);
  if (!v) return Promise.resolve(false);
  v.want = el;
  const { fontFamily, fontSize } = looks();
  return fontReady(fontFamily, fontSize).then(() => {
    if (v.want !== el || views.get(id) !== v) return false;
    if (v.host.parentElement !== el) el.appendChild(v.host);
    v.el = el;
    setRows(v); // the font has loaded now, so the glyph measures as xterm.js will measure it
    v.term.open(v.host); // the first time builds the view; later it only picks up the new place, as VS Code does
    release(v);
    loadWebgl(v);
    fitView(id);
    v.term.refresh(0, v.term.rows - 1);
    return true;
  });
}

/** Takes the view out of the place's element, unless another place has it already. Its output waits from now on (take). */
export function detachView(id: string, el: HTMLElement) {
  const v = views.get(id);
  if (!v) return;
  if (v.want === el) v.want = null;
  if (v.el !== el) return;
  dropWebgl(v); // frees the WebGL context; the buffer stays
  v.host.remove();
  v.el = null;
}

/** Fits the view to its place and tells the backend the size when it changed. */
export function fitView(id: string) {
  const v = views.get(id);
  if (!v?.el || !v.term.element) return;
  if (v.dpr !== dprNow()) setRows(v);
  const d = v.fit.proposeDimensions();
  if (!d || !Number.isFinite(d.cols) || !Number.isFinite(d.rows) || d.cols < 2 || d.rows < 1) return;
  if (d.cols !== v.term.cols || d.rows !== v.term.rows) v.term.resize(d.cols, d.rows);
  const { cols, rows } = v.term;
  if (cols !== v.sent.cols || rows !== v.sent.rows) {
    v.sent = { cols, rows };
    terminalResize(id, cols, rows).catch(() => {});
    changed(); // the page's top bar says the size
  }
}

/** The terminal's size now (columns × rows), as last sent to the backend; null before its view is made. */
export function useTerminalSize(id: string): { cols: number; rows: number } | null {
  return useSyncExternalStore(subscribe, () => views.get(id)?.sent ?? null, () => views.get(id)?.sent ?? null);
}

/** Clear, in the page's ⋯ menu: empties the scrollback and the screen but for the line the prompt is on. */
export function clearTerminal(id: string) {
  views.get(id)?.term.clear();
}

/** Gives the view the code's rows for its font on this screen. */
function setRows(v: View) {
  v.dpr = dprNow();
  const lh = lineHeightOf(String(v.term.options.fontFamily), Number(v.term.options.fontSize));
  if (v.term.options.lineHeight !== lh) v.term.options.lineHeight = lh;
}

/** The theme or Settings → Appearance changed: every view takes the new colours and font; one on screen fits again. */
function restyle() {
  if (!views.size) return;
  const { theme, fontFamily, fontSize } = looks();
  for (const v of views.values()) v.term.options.theme = theme;
  fontReady(fontFamily, fontSize).then(() => {
    for (const v of views.values()) {
      if (v.term.options.fontFamily !== fontFamily) v.term.options.fontFamily = fontFamily;
      if (v.term.options.fontSize !== fontSize) v.term.options.fontSize = fontSize;
      setRows(v);
    }
    // xterm.js measures the new cell, then the views on screen fit again (their places kept their size)
    requestAnimationFrame(() => { for (const v of views.values()) if (v.el) fitView(v.id); });
  });
}

export function focusTerminal(id: string) {
  views.get(id)?.term.focus();
}

// ---------- find (design §3) ----------

export type FindOptions = Pick<ISearchOptions, "caseSensitive" | "wholeWord" | "regex">;
/** Finds the next (or the previous) match; matches are marked as the Editor marks them. */
export function findIn(id: string, query: string, opts: FindOptions, how: "next" | "previous" | "typed" = "next"): boolean {
  const v = views.get(id);
  if (!v) return false;
  if (!query) { v.search.clearDecorations(); return false; }
  const css = getComputedStyle(document.documentElement);
  const read = (token: string) => css.getPropertyValue(`--${token}`).trim();
  const decorations = {
    matchBackground: read("term-selection"), activeMatchBackground: read("term-selection"), activeMatchBorder: read("accent"),
    matchOverviewRuler: read("text-3"), activeMatchColorOverviewRuler: read("accent"),
  };
  // typing keeps the match it has while it still matches; Enter and Shift+Enter move on
  const o = { ...opts, decorations, incremental: how === "typed" };
  try { return how === "previous" ? v.search.findPrevious(query, o) : v.search.findNext(query, o); } catch { return false; }
}
/** Calls `cb` with find's place and count ("3 of 12"); returns the unsubscribe. */
export function onFindResults(id: string, cb: (r: { index: number; count: number }) => void): () => void {
  const d = views.get(id)?.search.onDidChangeResults((r) => cb({ index: r.resultIndex, count: r.resultCount }));
  return () => d?.dispose();
}
export function endFind(id: string) {
  const v = views.get(id);
  if (!v) return;
  v.search.clearDecorations();
  v.term.clearSelection();
}

// ---------- for the UI probe (src/selftest.ts) ----------

/** The view of a terminal as the probe sees it: its xterm.js Terminal and what draws it now. */
export function probeView(id: string): { term: Terminal; renderer: "webgl" | "dom" | "none"; host: HTMLElement } | null {
  const v = views.get(id);
  if (!v) return null;
  return { term: v.term, renderer: !v.el ? "none" : v.webgl ? "webgl" : "dom", host: v.host };
}
