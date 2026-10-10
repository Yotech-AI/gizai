// One note on the Memory page (GA-68): in the centre its title, the reading view or the editor (the doc page's editor,
// saving and conflict handling: useDocEditor), and on the right the panel with collapsible sections: backlinks (linked
// and unlinked mentions, with Link), outgoing links (also those that find no note yet), the outline, the properties as
// a small form, the tags (a click filters the tree) and the history.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { BookOpen, ChevronDown, ChevronRight, FileText, Hash, Link2, Pencil, Plus, X } from "lucide-react";
import { docVersionBody, docVersions, renameDoc, saveDoc } from "../../api";
import { useData } from "../../lib/useData";
import { useDocEditor } from "../../lib/useDocEditor";
import { relTime } from "../../lib/format";
import { modKey } from "../../lib/keys";
import {
  backlinks, folderOf, linkedNote, linkMention, LIST_PROPERTIES, NOTE_TYPES, outgoing, outline, properties, resolve, setProperty, tagCounts,
  tags, titleOf, titleProblem, unlinkedMentions, type Mention,
} from "../../lib/memory";
import { MarkdownEditor, type EditorHandle, type WikiSupport } from "../MarkdownEditor";
import { MarkdownView } from "../MarkdownView";
import { Drawer } from "../Drawer";
import { NoteView, type NoteLinks } from "./NoteView";
import type { MemoryNote } from "../../types";

const MODE_KEY = "gizai.memory.mode";
const SECTIONS_KEY = "gizai.memory.closed";

type Props = {
  id: string;
  /** Every note, with its text. */
  notes: MemoryNote[];
  /** The notes the page shows (its scope): the tags come from these. */
  scopeNotes: MemoryNote[];
  links: NoteLinks;
  tagFilter: string | null;
  onTag: (tag: string | null) => void;
  /** Open in the editor (a note just made). */
  editing?: boolean;
  /** A heading to show once the note is there (a link to Note#Heading). */
  heading?: string;
  /** The right panel is shown. */
  panel: boolean;
};

export function NotePane({ id, notes, scopeNotes, links, tagFilter, onTag, editing, heading, panel }: Props) {
  const ed = useDocEditor(id);
  const { doc, text, base, status } = ed;
  const { data: versions } = useData(() => docVersions(id), [id]);
  const [mode, setMode] = useState<"read" | "edit">(() => (editing || localStorage.getItem(MODE_KEY) === "edit" ? "edit" : "read"));
  const [title, setTitle] = useState("");
  const [viewing, setViewing] = useState<{ version: number; body: string } | null>(null);
  const editor = useRef<EditorHandle | null>(null);
  const reading = useRef<HTMLDivElement>(null);
  useEffect(() => { if (doc) setTitle(titleOf(doc.path ?? doc.title)); }, [doc?.path, doc?.title]); // eslint-disable-line react-hooks/exhaustive-deps

  const listed = notes.find((n) => n.id === id) ?? null;
  // The note as it is now: its place from the list (or the doc, just made), its text from the editor.
  const note: MemoryNote | null = doc && text !== null ? {
    id, path: listed?.path ?? doc.path ?? doc.title, scope: listed?.scope ?? "shared", ownerId: listed?.ownerId ?? null, bodyMd: text,
    currentVersion: base ?? doc.currentVersion, updatedAt: doc.updatedAt, updatedBy: listed?.updatedBy ?? null, chars: text.length,
  } : null;
  const all = note ? [...notes.filter((n) => n.id !== id), note] : notes;

  const show = (h: string) => {
    if (mode === "edit") { const at = outline(text ?? "").find((x) => x.text.toLowerCase() === h.toLowerCase()); if (at) editor.current?.goto(at.pos); return; }
    const el = [...(reading.current?.querySelectorAll("h1, h2, h3, h4, h5, h6") ?? [])].find((x) => !x.closest(".embed") && x.textContent?.trim().toLowerCase() === h.trim().toLowerCase());
    el?.scrollIntoView({ block: "start", behavior: "smooth" });
  };
  // A link to Note#Heading: show the heading once the text is there.
  const shown = useRef<string | null>(null);
  useEffect(() => {
    if (!heading || text === null || shown.current === heading) return;
    shown.current = heading;
    requestAnimationFrame(() => show(heading));
  }, [heading, text !== null]); // eslint-disable-line react-hooks/exhaustive-deps

  if (ed.error) return <div className="error-banner">{ed.error}</div>;
  if (!doc || text === null || !note) return null;

  const switchMode = (m: "read" | "edit") => {
    if (m === "read" && status === "unsaved") ed.save(text);
    setMode(m);
    localStorage.setItem(MODE_KEY, m);
  };
  const saveTitle = () => {
    const t = title.trim();
    const was = titleOf(note.path);
    if (t === was) return;
    const problem = titleProblem(t);
    if (problem) { ed.setErr(problem); setTitle(was); return; }
    renameDoc(id, t).then(() => ed.setErr(null)).catch((e) => { ed.setErr(String(e)); setTitle(was); });
  };
  const openVersion = async (v: number) => { try { setViewing({ version: v, body: await docVersionBody(id, v) }); } catch (e) { ed.setErr(String(e)); } };
  const statusText = { saved: `Saved · version ${base}`, unsaved: `Unsaved changes · ${modKey()}+S saves`, saving: "Saving…", conflict: "Not saved" }[status];
  const last = versions?.[0];
  const wiki: WikiSupport = {
    notes: all, current: note,
    finds: (t) => resolve(all, t, folderOf(note.path)) >= 0,
    open: (l) => { const n = l.target ? linkedNote(all, l.target, note) : note; if (n) { if (n.id === id && l.heading) show(l.heading); else links.open(n, l.heading); } else links.create(l, note); },
    hover: (l, at) => links.hover(l, note, at),
  };

  return (
    <>
      <div className="mem-note">
        <div className="mem-note-bar">
          <span className={status === "saved" ? "faint" : "muted"} role="status">{statusText}</span>
          {last && <span className="faint">· {last.authorName ?? "Someone"}, {relTime(last.createdAt)}</span>}
          <span className="grow" />
          <div className="seg" role="group" aria-label="View">
            <button className={mode === "read" ? "on" : undefined} aria-pressed={mode === "read"} title="The note as it reads, its links working" onClick={() => switchMode("read")}><BookOpen className="icon sm" />Read</button>
            <button className={mode === "edit" ? "on" : undefined} aria-pressed={mode === "edit"} title="Edit the note's Markdown" onClick={() => switchMode("edit")}><Pencil className="icon sm" />Edit</button>
          </div>
          <button className="btn sm primary" disabled={status === "saved" || status === "saving"} onClick={() => ed.save(text)}>Save</button>
        </div>
        {ed.err && <div className="error-banner" role="alert">{ed.err}</div>}
        {status === "conflict" && (
          <div className="error-banner" role="alert">
            This note was saved by someone else after you opened it (version {doc.currentVersion}). Your text is still in the editor.
            <button className="btn" onClick={() => ed.save(text, true)}>Save mine as a new version</button>
            <button className="btn ghost" onClick={() => ed.adopt(doc.bodyMd, doc.currentVersion)}>Discard mine, load theirs</button>
          </div>
        )}
        <div className="doc-in mem-note-in">
          <div className="mem-path faint">{folderOf(note.path)}</div>
          <input className="title-input" aria-label="Note title" value={title} onChange={(e) => setTitle(e.target.value)} onBlur={saveTitle}
            onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setTitle(titleOf(note.path)); (e.target as HTMLInputElement).blur(); } }} />
          {mode === "edit" ? (
            <div className="doc-editor">
              <MarkdownEditor value={text} ariaLabel="Note text" minHeight={420} hint={`[[ links a note · ${modKey()}+click opens a link · ${modKey()}+S saves`} handle={editor}
                placeholder="Write in Markdown: # headings, [[links to notes]], #tags, KADE-12 refs and @mentions."
                wiki={wiki} onChange={ed.edit} onSave={(md) => ed.save(md)} onBlur={ed.blur} />
            </div>
          ) : (
            <div ref={reading} onDoubleClick={(e) => { if (!(e.target as HTMLElement).closest("a, button")) switchMode("edit"); }}>
              {text.trim() ? <NoteView md={text} from={note} links={{ ...links, notes: all }} />
                : <p className="faint">This note is empty. <button className="link-btn" onClick={() => switchMode("edit")}>Write in it</button></p>}
            </div>
          )}
        </div>
      </div>
      {panel && (
        <aside className="mem-side" aria-label="About this note">
          <Backlinks note={note} notes={all} links={links} onError={(e) => ed.setErr(e)} />
          <Outgoing note={note} notes={all} links={links} />
          <Section id="outline" title="Outline" count={outline(text).length}>
            {outline(text).length === 0 ? <p className="faint">No headings.</p> : outline(text).map((h, i) => (
              <button key={i} className="mem-row" style={{ paddingLeft: 8 + (h.level - 1) * 12 }} onClick={() => show(h.text)}>
                <span className="ellipsis">{h.text}</span>
              </button>
            ))}
          </Section>
          <Section id="properties" title="Properties" count={properties(text).length}>
            <PropertiesForm text={text} onChange={(md) => ed.replace(md)} />
          </Section>
          <Section id="tags" title="Tags" count={tagCounts(scopeNotes).length}>
            <TagList notes={scopeNotes} mine={tags(text)} active={tagFilter} onTag={onTag} />
          </Section>
          <Section id="history" title="History" count={versions?.length ?? 0}>
            {(versions ?? []).map((v) => (
              <button key={v.version} className={`version${v.version === base ? " on" : ""}`} onClick={() => openVersion(v.version)}>
                <span className="mono">v{v.version}</span><span>{v.authorName ?? "Someone"}</span><span className="faint">{relTime(v.createdAt)}</span>
              </button>
            ))}
          </Section>
        </aside>
      )}
      {viewing && (
        <Drawer wide title={`${titleOf(note.path)}, version ${viewing.version}`} subtitle="Restoring saves this text as a new version; nothing is lost." onClose={() => setViewing(null)}
          actions={<>
            <button className="btn ghost" onClick={() => setViewing(null)}>Close</button>
            <button className="btn primary" onClick={() => { const b = viewing.body; setViewing(null); ed.replace(b, true); }}>Restore this version</button>
          </>}>
          {viewing.body.trim() ? <MarkdownView md={viewing.body} /> : <p className="faint">This version is empty.</p>}
        </Drawer>
      )}
    </>
  );
}

/** A collapsible section of the right panel; which are closed is kept. */
function Section({ id, title, count, children }: { id: string; title: string; count?: number; children: ReactNode }) {
  const [closed, setClosed] = useState(() => (localStorage.getItem(SECTIONS_KEY) ?? "").split(",").includes(id));
  const toggle = () => {
    const now = !closed;
    setClosed(now);
    const list = (localStorage.getItem(SECTIONS_KEY) ?? "").split(",").filter((x) => x && x !== id);
    localStorage.setItem(SECTIONS_KEY, (now ? [...list, id] : list).join(","));
  };
  return (
    <section className="mem-section">
      <button className="mem-section-head" aria-expanded={!closed} onClick={toggle}>
        {closed ? <ChevronRight className="icon sm" /> : <ChevronDown className="icon sm" />}<span>{title}</span>
        {count !== undefined && count > 0 && <span className="faint">{count}</span>}
      </button>
      {!closed && <div className="mem-section-body">{children}</div>}
    </section>
  );
}

/** A mention's line with the name marked. */
function MentionLine({ m }: { m: Mention }) {
  const line = m.line;
  return <span className="mem-context">{line.length > 160 ? `${line.slice(0, 160)}…` : line}</span>;
}

function Backlinks({ note, notes, links, onError }: { note: MemoryNote; notes: MemoryNote[]; links: NoteLinks; onError: (e: string) => void }) {
  const linked = backlinks(notes, note);
  const unlinked = unlinkedMentions(notes, note);
  const [busy, setBusy] = useState<string | null>(null);
  const link = async (m: Mention) => {
    const key = `${m.note.id}:${m.start}`;
    setBusy(key);
    try { await saveDoc(m.note.id, linkMention(m.note.bodyMd, m.start, m.end, notes, note, folderOf(m.note.path)), m.note.currentVersion); }
    catch (e) { onError(String(e)); } finally { setBusy(null); }
  };
  const byNote = (ms: Mention[]) => [...new Map(ms.map((m) => [m.note.id, ms.filter((x) => x.note.id === m.note.id)])).values()];
  return (
    <Section id="backlinks" title="Backlinks" count={linked.length + unlinked.length}>
      <div className="mem-sub">Linked mentions <span className="faint">{linked.length}</span></div>
      {linked.length === 0 && <p className="faint">No note links here yet.</p>}
      {byNote(linked).map((ms) => (
        <div key={ms[0]!.note.id} className="mem-mention">
          <button className="mem-row" title={ms[0]!.note.path} onClick={() => links.open(ms[0]!.note)}><FileText className="icon sm" /><span className="ellipsis">{titleOf(ms[0]!.note.path)}</span></button>
          {ms.map((m, i) => <MentionLine key={i} m={m} />)}
        </div>
      ))}
      <div className="mem-sub">Unlinked mentions <span className="faint">{unlinked.length}</span></div>
      {unlinked.length === 0 && <p className="faint">No note names it without a link.</p>}
      {unlinked.map((m) => (
        <div key={`${m.note.id}:${m.start}`} className="mem-mention">
          <div className="mem-row-line">
            <button className="mem-row" title={m.note.path} onClick={() => links.open(m.note)}><FileText className="icon sm" /><span className="ellipsis">{titleOf(m.note.path)}</span></button>
            <button className="btn sm" disabled={busy !== null} title={`Make this a link to ${titleOf(note.path)}`} onClick={() => link(m)}><Link2 className="icon" />Link</button>
          </div>
          <MentionLine m={m} />
        </div>
      ))}
    </Section>
  );
}

function Outgoing({ note, notes, links }: { note: MemoryNote; notes: MemoryNote[]; links: NoteLinks }) {
  const out = outgoing(note.bodyMd, notes, note).filter((o) => o.note?.id !== note.id);
  return (
    <Section id="outgoing" title="Outgoing links" count={out.length}>
      {out.length === 0 && <p className="faint">This note links to no note.</p>}
      {out.map(({ link, note: to }) => to ? (
        <button key={to.id} className="mem-row" title={to.path} onClick={() => links.open(to, link.heading)}>
          <FileText className="icon sm" /><span className="ellipsis">{titleOf(to.path)}</span><span className="faint ellipsis">{folderOf(to.path)}</span>
        </button>
      ) : (
        <div key={`new:${link.target}`} className="mem-row-line">
          <span className="mem-row missing" title={`No note called ${link.target} yet`}><FileText className="icon sm" /><span className="ellipsis">{link.target}</span></span>
          <button className="btn sm" onClick={() => links.create(link, note)}><Plus className="icon" />Make it</button>
        </div>
      ))}
    </Section>
  );
}

/** The note's properties (its frontmatter) as a small form: each change rewrites that line in the text and saves. */
function PropertiesForm({ text, onChange }: { text: string; onChange: (md: string) => void }) {
  const props = properties(text);
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const set = (k: string, raw: string | null) => {
    const vals = raw === null ? null : LIST_PROPERTIES.includes(k) ? raw.split(",").map((x) => x.trim()).filter(Boolean) : [raw.trim()];
    const md = setProperty(text, k, vals);
    if (md !== text) onChange(md);
  };
  const add = () => {
    const k = key.trim().toLowerCase().replace(/\s+/g, "_");
    if (!k || /[:#]/.test(k)) return;
    set(k, value);
    setKey(""); setValue("");
  };
  return (
    <div className="mem-props">
      {props.length === 0 && <p className="faint">No properties yet.</p>}
      {props.map(([k, vals]) => (
        <div key={k} className="mem-prop">
          <label htmlFor={`prop-${k}`}>{k}</label>
          {k === "type" ? (
            <select id={`prop-${k}`} className="select" value={vals[0] ?? ""} onChange={(e) => set(k, e.target.value)}>
              {!(NOTE_TYPES as readonly string[]).includes(vals[0] ?? "") && <option value={vals[0] ?? ""}>{vals[0] || "—"}</option>}
              {NOTE_TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
            </select>
          ) : (
            <PropInput id={`prop-${k}`} value={vals.join(", ")} onCommit={(v) => set(k, v)} />
          )}
          <button className="btn ghost sm icon-only" aria-label={`Remove ${k}`} title={`Remove ${k}`} onClick={() => set(k, null)}><X className="icon" /></button>
        </div>
      ))}
      <div className="mem-prop add">
        <input className="input" aria-label="New property" placeholder="property" value={key} onChange={(e) => setKey(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") add(); }} />
        <input className="input" aria-label="Its value" placeholder="value" value={value} onChange={(e) => setValue(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") add(); }} />
        <button className="btn ghost sm icon-only" aria-label="Add the property" title="Add the property" disabled={!key.trim()} onClick={add}><Plus className="icon" /></button>
      </div>
    </div>
  );
}

/** A property's value, saved when you leave it or press Enter (Escape puts it back). */
function PropInput({ id, value, onCommit }: { id: string; value: string; onCommit: (v: string) => void }) {
  const [v, setV] = useState(value);
  useEffect(() => setV(value), [value]);
  return <input id={id} className="input" value={v} onChange={(e) => setV(e.target.value)} onBlur={() => { if (v !== value) onCommit(v); }}
    onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setV(value); } }} />;
}

/** Every tag of the page's notes with how many notes have it; this note's first. A click filters the tree. */
function TagList({ notes, mine, active, onTag }: { notes: MemoryNote[]; mine: string[]; active: string | null; onTag: (t: string | null) => void }) {
  const counts = tagCounts(notes);
  if (counts.length === 0 && mine.length === 0) return <p className="faint">No tags yet. Add them to a note's tags property, or write #tag in it.</p>;
  const first = [...counts.filter((c) => mine.includes(c.tag)), ...counts.filter((c) => !mine.includes(c.tag))];
  return (
    <div className="mem-tags">
      {first.map((c) => (
        <button key={c.tag} className={`mem-tag${active === c.tag ? " on" : ""}${mine.includes(c.tag) ? " mine" : ""}`} aria-pressed={active === c.tag}
          title={active === c.tag ? "Show every note again" : `Show only the notes tagged ${c.tag}`} onClick={() => onTag(active === c.tag ? null : c.tag)}>
          <Hash className="icon sm" />{c.tag}<span className="faint">{c.count}</span>
        </button>
      ))}
    </div>
  );
}

