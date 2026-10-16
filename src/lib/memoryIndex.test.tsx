// GA-190 QA: "With 300 notes, the Memory page builds its note index once per list." Two levels. The library: one
// noteIndex(notes) of 300 notes answers backlinks, unlinked mentions, which note a link finds and the outgoing links for
// every note exactly as the old per-note code did (copied below from origin/main as the reference), the wrappers without
// an index agree, and indexOf keeps one index per list. The note pane: React itself renders NotePane (react-dom/client)
// into a small stand-in DOM (there is no DOM library in the project); noteIndex is the real one, counted; the editor is a
// stand-in that keeps its props, so a keystroke is a call of its onChange; the api is a fake.
import { act, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Doc, MemoryNote } from "../types";
import type { NoteLinks } from "../components/memory/NoteView";
import {
  backlinks, backlinksIn, folderOf, frontmatterBlock, indexOf, noteIndex, outgoing, resolve, resolveIn, titleOf, unlinkedMentions,
  unlinkedMentionsIn, wikilinks, type Mention, type WikiLink,
} from "./memory";

// ---- a stand-in DOM, as much of it as React DOM uses (as in StreamParses.test.tsx) ----
const HTML = "http://www.w3.org/1999/xhtml";
type Listener = { f: (e: unknown) => void; capture: boolean };
class FakeNode {
  childNodes: FakeNode[] = [];
  parentNode: FakeNode | null = null;
  attrs = new Map<string, string>();
  listeners = new Map<string, Listener[]>();
  style: Record<string, unknown> = { setProperty(k: string, v: string) { (this as Record<string, unknown>)[k] = v; }, removeProperty() {} };
  nodeValue: string | null = null;
  value = "";
  constructor(public nodeType: number, public nodeName: string, public ownerDocument: FakeNode | null, public namespaceURI = HTML) {}
  get tagName() { return this.nodeName; }
  get options() { return this.all().filter((n) => n.nodeName === "OPTION"); }
  get firstChild(): FakeNode | null { return this.childNodes[0] ?? null; }
  get lastChild(): FakeNode | null { return this.childNodes[this.childNodes.length - 1] ?? null; }
  get nextSibling(): FakeNode | null { const p = this.parentNode; return p ? p.childNodes[p.childNodes.indexOf(this) + 1] ?? null : null; }
  get textContent(): string { return this.nodeType === 3 ? this.nodeValue ?? "" : this.childNodes.map((c) => c.textContent).join(""); }
  set textContent(t: string) {
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
    if (t) this.appendChild(doc.createTextNode(t));
  }
  appendChild(c: FakeNode) { return this.insertBefore(c, null); }
  insertBefore(c: FakeNode, ref: FakeNode | null) {
    c.parentNode?.removeChild(c);
    const i = ref ? this.childNodes.indexOf(ref) : -1;
    if (i < 0) this.childNodes.push(c); else this.childNodes.splice(i, 0, c);
    c.parentNode = this;
    return c;
  }
  removeChild(c: FakeNode) { this.childNodes.splice(this.childNodes.indexOf(c), 1); c.parentNode = null; return c; }
  setAttribute(k: string, v: unknown) { this.attrs.set(k, String(v)); }
  setAttributeNS(_ns: string, k: string, v: unknown) { this.attrs.set(k, String(v)); }
  removeAttribute(k: string) { this.attrs.delete(k); }
  removeAttributeNS(_ns: string, k: string) { this.attrs.delete(k); }
  getAttribute(k: string) { return this.attrs.get(k) ?? null; }
  hasAttribute(k: string) { return this.attrs.has(k); }
  addEventListener(type: string, f: (e: unknown) => void, opt?: boolean | { capture?: boolean }) {
    const capture = typeof opt === "boolean" ? opt : !!opt?.capture;
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), { f, capture }]);
  }
  removeEventListener(type: string, f: (e: unknown) => void) { this.listeners.set(type, (this.listeners.get(type) ?? []).filter((l) => l.f !== f)); }
  contains(n: FakeNode | null) { for (let x = n; x; x = x.parentNode) if (x === this) return true; return false; }
  scrollIntoView() {}
  focus() {}
  blur() {}
  getBoundingClientRect() { return { top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }; }
  /** This node and every node under it, in document order. */
  all(): FakeNode[] { return [this, ...this.childNodes.flatMap((c) => c.all())]; }
  createElement(tag: string) { return new FakeNode(1, tag.toUpperCase(), this); }
  createElementNS(ns: string, tag: string) { return new FakeNode(1, tag, this, ns); }
  createTextNode(t: string) { const n = new FakeNode(3, "#text", this); n.nodeValue = t; return n; }
}
const doc = new FakeNode(9, "#document", null);
const body = doc.createElement("body");
doc.appendChild(body);
const win = { document: doc, event: undefined, HTMLIFrameElement: class {}, devicePixelRatio: 1, addEventListener() {}, removeEventListener() {} };
Object.assign(doc, { body, documentElement: body, activeElement: body, defaultView: win });
vi.stubGlobal("window", win);
vi.stubGlobal("document", doc);
const store = new Map<string, string>();
vi.stubGlobal("localStorage", { getItem: (k: string) => store.get(k) ?? null, setItem: (k: string, v: string) => { store.set(k, v); }, removeItem: (k: string) => { store.delete(k); } });
vi.stubGlobal("requestAnimationFrame", (f: () => void) => setTimeout(f, 0));
vi.stubGlobal("cancelAnimationFrame", (t: ReturnType<typeof setTimeout>) => clearTimeout(t));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const cls = (n: FakeNode) => (n.getAttribute("class") ?? "").split(" ");
const byClass = (root: FakeNode, c: string) => root.all().filter((n) => n.nodeType === 1 && cls(n).includes(c));

// ---- what the fakes record ----
type EditorProps = { value: string; onChange?: (md: string) => void; wiki?: { finds: (t: string) => boolean; notes: readonly MemoryNote[] } };
const h = vi.hoisted(() => ({
  /** Each call of noteIndex through the module's export (NotePane's), with the length of the list it was given. */
  indexCalls: [] as number[],
  editor: null as EditorProps | null,
  /** What the fake api answers, by function name; a function without an answer never settles. */
  api: {} as Record<string, (...a: unknown[]) => unknown>,
  calls: [] as [string, unknown[]][],
}));

// noteIndex is the real one, counted; everything else in lib/memory is the real module.
vi.mock("./memory", async (importOriginal) => {
  const real = await importOriginal<typeof import("./memory")>();
  return { ...real, noteIndex: <N extends Pick<MemoryNote, "path">>(notes: readonly N[]) => { h.indexCalls.push(notes.length); return real.noteIndex(notes); } };
});
vi.mock("../api", async (orig) => {
  const real = await orig<Record<string, unknown>>();
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(real)) {
    out[k] = typeof v !== "function" ? v : (...a: unknown[]) => { h.calls.push([k, a]); return h.api[k] ? h.api[k](...a) : new Promise(() => {}); };
  }
  return out;
});
// The editor keeps the props it was given last.
vi.mock("../components/MarkdownEditor", () => ({ MarkdownEditor: (p: EditorProps) => { h.editor = p; return null; } }));
vi.mock("../components/memory/NoteView", () => ({ NoteView: () => null }));
vi.mock("../components/MarkdownView", () => ({ MarkdownView: () => null }));
vi.mock("../components/Drawer", () => ({ Drawer: () => null }));

const { createRoot } = await import("react-dom/client");
const { NotePane } = await import("../components/memory/NotePane");

// ---- the old per-note code (origin/main, before GA-190), the reference ----
const lower = (s: string) => s.toLowerCase();
function oldResolve(notes: readonly Pick<MemoryNote, "path">[], target: string, folder: string): number {
  let t = lower(target.trim()).replace(/^\/+|\/+$/g, "");
  if (t.endsWith(".md")) t = t.slice(0, -3);
  if (!t) return -1;
  let cands: number[];
  if (t.includes("/")) {
    cands = notes.flatMap((n, i) => (lower(n.path) === t ? [i] : []));
    if (!cands.length) cands = notes.flatMap((n, i) => (lower(n.path).endsWith(`/${t}`) ? [i] : []));
  } else {
    cands = notes.flatMap((n, i) => (lower(titleOf(n.path)) === t ? [i] : []));
  }
  const f = lower(folder);
  cands.sort((a, b) => {
    const pa = notes[a]!.path, pb = notes[b]!.path;
    return Number(lower(folderOf(pa)) !== f) - Number(lower(folderOf(pb)) !== f) || pa.length - pb.length || (lower(pa) < lower(pb) ? -1 : lower(pa) > lower(pb) ? 1 : 0);
  });
  return cands[0] ?? -1;
}
function oldLinkedNote<N extends Pick<MemoryNote, "path">>(notes: readonly N[], target: string, from: Pick<MemoryNote, "path"> | null): N | null {
  const i = oldResolve(notes, target, from ? folderOf(from.path) : "");
  return i < 0 ? null : notes[i]!;
}
const lineAround = (text: string, start: number, end: number) => {
  const from = text.lastIndexOf("\n", start - 1) + 1;
  const to = text.indexOf("\n", end);
  return text.slice(from, to < 0 ? text.length : to).trim();
};
/** oldResolve for one list, each (target, folder) worked out once: the same answers, so the reference stays the old
 *  code while 300 notes × 300 targets × ~1500 links stay quick. */
const resolvers = new WeakMap<object, (t: string, f: string) => number>();
function oldResolver(notes: readonly Pick<MemoryNote, "path">[]) {
  let r = resolvers.get(notes);
  if (!r) {
    const memo = new Map<string, number>();
    r = (t, f) => { const k = `${t}\u0000${f}`; let v = memo.get(k); if (v === undefined) memo.set(k, (v = oldResolve(notes, t, f))); return v; };
    resolvers.set(notes, r);
  }
  return r;
}
function oldBacklinks(notes: readonly MemoryNote[], target: Pick<MemoryNote, "id">): Mention[] {
  const res = oldResolver(notes);
  const out: Mention[] = [];
  for (const n of notes) {
    if (n.id === target.id) continue;
    for (const l of wikilinks(n.bodyMd)) {
      const i = res(l.target, folderOf(n.path));
      if (i >= 0 && notes[i]!.id === target.id) out.push({ note: n, line: lineAround(n.bodyMd, l.start, l.end), start: l.start, end: l.end });
    }
  }
  return out;
}
function oldBlocked(text: string): [number, number][] {
  const out: [number, number][] = [];
  const fm = frontmatterBlock(text);
  if (fm) out.push([0, fm.end]);
  for (const re of [/(^|\n)(```|~~~)[\s\S]*?(\n(```|~~~)|$)/g, /`[^`\n]*`/g, /!?\[\[[^\]\n]*\]\]/g, /\[[^\]\n]*\]\([^)\n]*\)/g, /<[^>\n]+>/g]) {
    for (let m = re.exec(text); m; m = re.exec(text)) out.push([m.index, m.index + m[0].length]);
  }
  return out;
}
const WORD = /[\p{L}\p{N}_]/u;
function oldUnlinkedMentions(notes: readonly MemoryNote[], target: Pick<MemoryNote, "id" | "path">): Mention[] {
  const title = titleOf(target.path).trim();
  if (title.length < 2) return [];
  const want = lower(title);
  const out: Mention[] = [];
  for (const n of notes) {
    if (n.id === target.id) continue;
    const text = n.bodyMd;
    const low = lower(text);
    if (!low.includes(want)) continue;
    const skip = oldBlocked(text);
    for (let i = low.indexOf(want); i >= 0; i = low.indexOf(want, i + 1)) {
      const end = i + want.length;
      if ((i > 0 && WORD.test(text[i - 1]!)) || (end < text.length && WORD.test(text[end]!))) continue;
      if (skip.some(([a, b]) => i < b && end > a)) continue;
      out.push({ note: n, line: lineAround(text, i, end), start: i, end });
    }
  }
  return out;
}
function oldOutgoing<N extends MemoryNote>(text: string, notes: readonly N[], from: Pick<MemoryNote, "path"> | null): { link: WikiLink; note: N | null }[] {
  const seen = new Set<string>();
  const out: { link: WikiLink; note: N | null }[] = [];
  for (const l of wikilinks(text)) {
    const note = oldLinkedNote(notes, l.target, from);
    const key = note ? `id:${note.id}` : `new:${lower(l.target)}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ link: l, note });
  }
  return out;
}
/** Mentions compared by note, place and line. */
const ms = (list: Mention[]) => list.map((m) => ({ id: m.note.id, start: m.start, end: m.end, line: m.line }));
const outs = (list: { link: WikiLink; note: MemoryNote | null }[]) => list.map((o) => ({ link: o.link, id: o.note?.id ?? null }));

// ---- 300 notes ----
/** A small seeded random (mulberry32): the same notes on every run. */
function seeded(seed: number) {
  let s = seed;
  return () => {
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const FOLDERS = ["Projects", "Clients", "Lessons", "Decisions", "Agents/Ann", "Agents/Bob/Sub", "Team Lead"];
/** Ten notes called Overview, four of them in a folder called Sub (so [[Sub/Overview]] finds four, by the end of a path). */
const OVERVIEWS = ["Projects/Part 0", "Clients/Sub", "Agents/Bob/Sub", "Agents/Ann/Sub", "Lessons/Part 4", "Decisions/Sub", "Team Lead", "Projects", "Agents/Ann", "Lessons"];
function pathOf(i: number): string {
  if (i % 30 === 0) return `${OVERVIEWS[i / 30]}/Overview`;
  if (i >= 280) return `${FOLDERS[(i - 280 + 3) % FOLDERS.length]}/Topic ${i - 280}`; // Topic 0 to 19 again, in another folder
  if (i === 279) return "Clients/TOPIC 21"; // Topic 21 again, its case changed
  return `${FOLDERS[i % FOLDERS.length]}/Topic ${i}`;
}
function bodyOf(i: number, r: () => number, paths: string[]): string {
  const pick = () => paths[Math.floor(r() * paths.length)]!;
  const t = (p: string) => titleOf(p);
  const out: string[] = [];
  if (i % 3 === 0) out.push("---", "type: note", `related: ${t(pick())}`, `tags: [memo, t${i % 5}]`, "---");
  out.push(`# ${titleOf(paths[i]!)}`, "");
  for (let k = 0; k < 5; k++) {
    const p = pick();
    const kinds = [
      `[[${t(p)}]]`, `[[${p}]]`, `[[${p.slice(p.indexOf("/") + 1)}]]`, `[[${t(p)}|alias ${k}]]`, `[[${t(p)}#Plan]]`, `[[${t(p).toUpperCase()}]]`,
      `[[${p}.md]]`, `[[Missing ${Math.floor(r() * 20)}]]`, `[[Nowhere/${t(p)}]]`, `![[${t(p)}]]`, `[[  ${t(p)}  |spaced]]`, `[[/${p}/]]`,
      "[[Sub/Overview]]", "[[Overview]]", `[[${t(p)}#Plan|both]]`,
    ];
    out.push(`Line ${k}: see ${kinds[Math.floor(r() * kinds.length)]} for more.`);
  }
  out.push("[[#Plan]] is here.");
  out.push(`We talked about ${t(pick())} today, and ${lower(t(pick()))} too.`);
  out.push(`Not a word: ${t(pick())}s and x${t(pick())}.`);
  out.push(`In code: \`${t(pick())}\` and a [${t(pick())}](https://example.com) link and <b title="${t(pick())}">x</b>.`);
  if (i % 4 === 0) out.push("```", `${t(pick())} in a fence`, "```");
  out.push("## Plan", `${t(pick())}: the end.`);
  return out.join("\n");
}
function makeNotes(seed = 190): MemoryNote[] {
  const r = seeded(seed);
  const paths = Array.from({ length: 300 }, (_, i) => pathOf(i));
  return paths.map((path, i) => {
    const bodyMd = bodyOf(i, r, paths);
    return { id: `n${i}`, path, scope: "shared", ownerId: null, bodyMd, currentVersion: 1, updatedAt: 1_760_000_000_000 + i, updatedBy: null, chars: bodyMd.length };
  });
}

describe("one note index for 300 notes (GA-190), against the old per-note code", () => {
  const notes = makeNotes();
  const ix = noteIndex(notes);

  it("has 300 notes with links of every kind: found, missing, by path, by the end of a path, and titles that several notes have", () => {
    expect(notes).toHaveLength(300);
    expect(new Set(notes.map((n) => lower(n.path))).size).toBe(300);
    const links = notes.flatMap((n) => wikilinks(n.bodyMd).map((l) => ({ l, n })));
    expect(links.length).toBeGreaterThanOrEqual(1500);
    const found = links.filter(({ l, n }) => oldResolve(notes, l.target, folderOf(n.path)) >= 0);
    expect(found.length).toBeGreaterThan(1000);
    expect(links.length - found.length).toBeGreaterThan(100); // links to notes that don't exist yet
    expect(links.filter(({ l }) => l.alias !== undefined).length).toBeGreaterThan(50);
    expect(links.filter(({ l }) => l.heading !== undefined).length).toBeGreaterThan(50);
    expect(links.filter(({ l }) => l.embed).length).toBeGreaterThan(50);
    expect(links.filter(({ l }) => l.target.includes("/")).length).toBeGreaterThan(300);
    // Titles more than one note has: a link to one picks the note in its own folder, else the shortest path.
    const many = links.filter(({ l }) => !l.target.includes("/") && notes.filter((x) => lower(titleOf(x.path)) === lower(l.target.replace(/\.md$/, ""))).length > 1);
    expect(many.length).toBeGreaterThan(100);
    // Plain mentions are there, also in places that can't take a link (properties, code, fences, Markdown links, tags).
    expect(notes.filter((n) => frontmatterBlock(n.bodyMd)).length).toBe(100);
    expect(notes.filter((n) => n.bodyMd.includes("```")).length).toBe(75);
  });

  it("finds the same note for every link of every note as the old resolve, from the note's folder and from every other folder", () => {
    const folders = [...new Set(notes.map((n) => folderOf(n.path))), ""];
    let checked = 0;
    for (const n of notes) {
      for (const l of wikilinks(n.bodyMd, true)) {
        const from = folderOf(n.path);
        expect(resolveIn(ix, l.target, from), `${n.path}: [[${l.target}]]`).toBe(oldResolve(notes, l.target, from));
        checked++;
      }
    }
    const targets = [...new Set(notes.flatMap((n) => wikilinks(n.bodyMd).map((l) => l.target)))];
    for (const t of [...targets, "Sub/Overview", "ann/sub/overview", "/Overview/", "overview.md", "TOPIC 21", "Topic 5", "Bob/Sub/Topic 5", "Nowhere", "", "  ", "/"]) {
      for (const f of folders) {
        expect(resolveIn(ix, t, f), `[[${t}]] from ${f}`).toBe(oldResolve(notes, t, f));
        checked++;
      }
    }
    expect(checked).toBeGreaterThan(10_000);
    // without an index of its own: the same
    for (const n of notes) for (const l of wikilinks(n.bodyMd)) expect(resolve(notes, l.target, folderOf(n.path))).toBe(oldResolve(notes, l.target, folderOf(n.path)));
  });

  it("gives every one of the 300 notes the same backlinks as the old backlinks, with the index and without", () => {
    let total = 0;
    for (const n of notes) {
      const want = ms(oldBacklinks(notes, n));
      expect(ms(backlinksIn(ix, n)), n.path).toEqual(want);
      expect(ms(backlinks(notes, n)), n.path).toEqual(want);
      total += want.length;
    }
    expect(total).toBeGreaterThan(1000);
    expect(notes.filter((n) => oldBacklinks(notes, n).length > 0).length).toBeGreaterThan(150);
  });

  it("gives every one of the 300 notes the same unlinked mentions as the old unlinkedMentions, with the index and without, none in properties or code", () => {
    let total = 0;
    for (const n of notes) {
      const old = oldUnlinkedMentions(notes, n);
      const want = ms(old);
      expect(ms(unlinkedMentionsIn(ix, n)), n.path).toEqual(want);
      expect(ms(unlinkedMentions(notes, n)), n.path).toEqual(want);
      for (const m of old) {
        expect(m.line.startsWith("related:"), m.line).toBe(false);
        expect(m.line.startsWith("In code:"), m.line).toBe(false);
        expect(m.line.endsWith("in a fence"), m.line).toBe(false);
        expect(m.line.startsWith("Not a word:"), m.line).toBe(false);
      }
      total += want.length;
    }
    expect(total).toBeGreaterThan(500);
  });

  it("gives every one of the 300 notes the same outgoing links as the old outgoing, with the index and without", () => {
    let found = 0, missing = 0;
    for (const n of notes) {
      const want = outs(oldOutgoing(n.bodyMd, notes, n));
      expect(outs(outgoing(n.bodyMd, notes, n, ix)), n.path).toEqual(want);
      expect(outs(outgoing(n.bodyMd, notes, n)), n.path).toEqual(want);
      expect(outs(outgoing(n.bodyMd, notes, null, ix)), n.path).toEqual(outs(oldOutgoing(n.bodyMd, notes, null)));
      for (const o of want) if (o.id) found++; else missing++;
    }
    expect(found).toBeGreaterThan(1000);
    expect(missing).toBeGreaterThan(100);
  });

  it("keeps one index per list: the same list twice is the same index, a new array of the same notes a new one", () => {
    const list = makeNotes(7);
    const a = indexOf(list);
    expect(indexOf(list)).toBe(a);
    expect(a.notes).toBe(list);
    const copy = [...list];
    const b = indexOf(copy);
    expect(b).not.toBe(a);
    expect(b.notes).toBe(copy);
    expect(indexOf(copy)).toBe(b);
    expect(indexOf(list)).toBe(a);
    // noteIndex itself makes a new index on each call (only indexOf keeps them)
    expect(noteIndex(list)).not.toBe(a);
  });

  it("builds the index once for 300 resolves, backlinks and unlinked mentions on one list: the wrappers use the list's index", () => {
    const list = makeNotes(11);
    // Each read of a note's path is counted: building an index reads every path; a lookup in it reads none (a title more
    // than one note has: the candidates' paths).
    let reads = 0;
    const counted = list.map((n) => {
      const c = { ...n } as MemoryNote;
      Object.defineProperty(c, "path", { get: () => { reads++; return n.path; }, enumerable: true });
      return c;
    });
    const unique = counted.map((n) => n.path).filter((p) => counted.filter((x) => lower(titleOf(x.path)) === lower(titleOf(p))).length === 1);
    expect(unique.length).toBe(250); // 50 notes share a title: 10 Overviews, Topic 1 to 19 twice, Topic 21 and TOPIC 21
    reads = 0;
    const answers1 = unique.map((p) => resolve(counted, titleOf(p), "Projects"));
    const firstRound = reads;
    const ix1 = indexOf(counted);
    reads = 0;
    const answers2 = [...unique, ...unique].map((p) => resolve(counted, titleOf(p), "Lessons"));
    expect(reads).toBe(0); // 2 × ~290 more resolves on the same list: no note's path read again, no index built again
    expect(indexOf(counted)).toBe(ix1);
    expect(firstRound).toBeLessThanOrEqual(2 * counted.length); // one build: each path read at most twice, not 300 × 300
    expect(answers1.every((i, k) => i >= 0 && counted[i]!.path === unique[k])).toBe(true);
    expect(answers2.every((i) => i >= 0)).toBe(true);

    // backlinks and unlinkedMentions without an index fill the list's own index, once.
    expect(ix1.linksTo).toBeUndefined();
    for (const n of counted) backlinks(counted, n);
    const linksTo = ix1.linksTo;
    expect(linksTo).toBeInstanceOf(Map);
    for (const n of counted) backlinks(counted, n);
    expect(ix1.linksTo).toBe(linksTo);
    expect(ix1.low).toBeUndefined();
    for (const n of counted) unlinkedMentions(counted, n);
    const low = ix1.low;
    expect(low).toBeDefined();
    for (const n of counted) unlinkedMentions(counted, n);
    expect(ix1.low).toBe(low);
    expect(indexOf(counted)).toBe(ix1);
  });
});

// ---- the note pane ----
let container: FakeNode;
let root: ReturnType<typeof createRoot> | null = null;
async function mount(el: ReactNode) {
  container = doc.createElement("div");
  body.appendChild(container);
  root = createRoot(container as unknown as HTMLElement);
  await act(async () => { root!.render(el); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}
async function rerender(el: ReactNode) {
  await act(async () => { root!.render(el); });
  await act(async () => { await new Promise((r) => setTimeout(r, 0)); });
}
/** A keystroke in the editor: its onChange with the text it now has. */
async function type(text: string) { await act(async () => { h.editor!.onChange!(text); }); }

/** The Backlinks section: linked mentions as [path, lines] by note, unlinked ones as [path, line] each. */
function backlinksShown() {
  const section = byClass(container, "mem-section").find((s) => s.firstChild?.textContent.startsWith("Backlinks"))!;
  const mentions = byClass(section, "mem-mention");
  const linked = mentions.filter((m) => m.firstChild?.nodeName === "BUTTON")
    .map((m) => [m.firstChild!.getAttribute("title"), byClass(m, "mem-context").map((c) => c.textContent)]);
  const unlinked = mentions.filter((m) => cls(m.firstChild!).includes("mem-row-line"))
    .map((m) => [byClass(m, "mem-row")[0]!.getAttribute("title"), byClass(m, "mem-context")[0]!.textContent]);
  return { linked, unlinked, head: section.firstChild!.textContent };
}
/** The Outgoing links section: the paths of the notes it finds, and the links that find none. */
function outgoingShown() {
  const section = byClass(container, "mem-section").find((s) => s.firstChild?.textContent.startsWith("Outgoing links"))!;
  return {
    found: byClass(section, "mem-row").filter((n) => n.nodeName === "BUTTON").map((n) => n.getAttribute("title")),
    missing: byClass(section, "missing").map((n) => n.textContent),
  };
}
/** What the pane should show, by the old code, for `open` with its text `text` among `notes`. */
function expected(notes: MemoryNote[], open: MemoryNote, text: string) {
  const note = { ...open, bodyMd: text };
  const all = [...notes.filter((n) => n.id !== open.id), note];
  const groups = new Map<string, Mention[]>();
  for (const m of oldBacklinks(all, note)) groups.set(m.note.id, [...(groups.get(m.note.id) ?? []), m]);
  const out = oldOutgoing(text, all, note).filter((o) => o.note?.id !== note.id);
  return {
    linked: [...groups.values()].map((g) => [g[0]!.note.path, g.map((m) => m.line)]),
    unlinked: oldUnlinkedMentions(all, note).map((m) => [m.note.path, m.line]),
    found: out.filter((o) => o.note).map((o) => o.note!.path),
    missing: out.filter((o) => !o.note).map((o) => o.link.target),
  };
}

describe("NotePane with 300 notes builds its note index once per list (GA-190)", () => {
  const links: NoteLinks = { notes: [], cards: new Set<string>(), open: () => {}, create: () => {}, hover: () => {} };
  const docOf = (n: MemoryNote): Doc => ({ id: n.id, title: titleOf(n.path), bodyMd: n.bodyMd, currentVersion: n.currentVersion, updatedAt: n.updatedAt, kind: "memory", path: n.path });
  const pane = (notes: MemoryNote[], id: string) => (
    <NotePane id={id} notes={notes} scopeNotes={notes} links={links} tagFilter={null} onTag={() => {}} editing panel />
  );

  beforeEach(() => {
    h.indexCalls.length = 0;
    h.calls.length = 0;
    h.editor = null;
    store.clear();
    for (const k of Object.keys(h.api)) delete h.api[k];
    h.api.docVersions = () => Promise.resolve([]);
    h.api.saveDoc = () => Promise.resolve(2);
  });
  afterEach(async () => {
    if (root) await act(async () => { root!.unmount(); });
    root = null;
    container?.parentNode?.removeChild(container);
  });

  it("calls noteIndex once for its list over 20 keystrokes, and once more for a new list; the Backlinks panel shows what the old code found", async () => {
    const notes = makeNotes();
    // The note with the most linked mentions is open.
    const count = new Map(notes.map((n) => [n.id, oldBacklinks(notes, n).length]));
    const open = [...notes].sort((a, b) => count.get(b.id)! - count.get(a.id)!)[0]!;
    h.api.getDoc = (id) => Promise.resolve(docOf(notes.find((n) => n.id === id)!));
    await mount(pane(notes, open.id));
    expect(h.editor, "the editor is open").not.toBeNull();
    expect(h.editor!.value).toBe(open.bodyMd);
    // Mounting: once for the list before the note's text is there, once for the list with it.
    const afterMount = h.indexCalls.length;
    expect(h.indexCalls).toEqual([300, 300]);

    const before = expected(notes, open, open.bodyMd);
    expect(before.linked.length).toBeGreaterThan(3);
    const shown = backlinksShown();
    expect(shown.linked).toEqual(before.linked);
    expect(shown.unlinked).toEqual(before.unlinked);
    expect(shown.head).toBe(`Backlinks${before.linked.reduce((s, [, l]) => s + l.length, 0) + before.unlinked.length}`);
    expect(outgoingShown()).toEqual({ found: before.found, missing: before.missing });

    // 20 keystrokes: a link to a note the open note doesn't link to yet, typed a character at a time.
    const target = notes.find((n) => /^Topic \d\d$/.test(titleOf(n.path)) && n.id !== open.id && !before.found.includes(n.path)
      && notes.filter((x) => lower(titleOf(x.path)) === lower(titleOf(n.path))).length === 1)!;
    const typed = ` see [[${titleOf(target.path)}]] ok`;
    expect(typed).toHaveLength(20);
    let text = open.bodyMd;
    for (const c of typed) { text += c; await type(text); }
    expect(h.editor!.value).toBe(text);
    expect(h.indexCalls.length - afterMount).toBe(0);

    // The panel follows the text (the new link is an outgoing link), and its backlinks are still right.
    const after = expected(notes, open, text);
    expect(after.found).toContain(target.path);
    expect(outgoingShown()).toEqual({ found: after.found, missing: after.missing });
    expect(backlinksShown().linked).toEqual(after.linked);
    expect(backlinksShown().unlinked).toEqual(after.unlinked);
    // The editor's dashed links ask the same index.
    expect(h.editor!.wiki!.finds(titleOf(target.path))).toBe(true);
    expect(h.editor!.wiki!.finds("Missing Zed")).toBe(false);
    expect(h.indexCalls.length - afterMount).toBe(0);

    // A new list (a note saved elsewhere links to the open one): one more index, and the panel shows the new backlink.
    const linker: MemoryNote = { ...notes[1]!, id: "new1", path: "Lessons/Fresh note", bodyMd: `# Fresh note\n\nAbout [[${open.path}]] now.` };
    const notes2 = [...notes, linker];
    await rerender(pane(notes2, open.id));
    expect(h.indexCalls.slice(afterMount)).toEqual([301]);
    const withNew = expected(notes2, open, text);
    expect(withNew.linked.map(([p]) => p)).toContain("Lessons/Fresh note");
    expect(backlinksShown().linked).toEqual(withNew.linked);
    expect(backlinksShown().unlinked).toEqual(withNew.unlinked);

    // 20 more keystrokes on the new list: no new index.
    for (const c of "abcdefghijklmnopqrst") { text += c; await type(text); }
    expect(h.indexCalls.slice(afterMount)).toEqual([301]);
    expect(backlinksShown().linked).toEqual(withNew.linked);
  });

  it("builds no index on re-renders with the same list, and one when the open note's place changes", async () => {
    const notes = makeNotes();
    const open = notes[42]!;
    h.api.getDoc = (id) => Promise.resolve(docOf(notes.find((n) => n.id === id)!));
    await mount(pane(notes, open.id));
    const afterMount = h.indexCalls.length;
    // The parent renders again with the same list (as MemoryPage does on its own changes): no index.
    await rerender(pane(notes, open.id));
    await rerender(pane(notes, open.id));
    expect(h.indexCalls.length).toBe(afterMount);
    // The note is renamed: the list has it at its new path, a new list, one index.
    const renamed = notes.map((n) => (n.id === open.id ? { ...n, path: "Lessons/Renamed note" } : n));
    await rerender(pane(renamed, open.id));
    expect(h.indexCalls.length).toBe(afterMount + 1);
    const panel = backlinksShown();
    const want = expected(renamed, renamed[42]!, open.bodyMd);
    expect(panel.linked).toEqual(want.linked);
    expect(panel.unlinked).toEqual(want.unlinked);
  });
});
