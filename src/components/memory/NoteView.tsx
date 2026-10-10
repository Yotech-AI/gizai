// The reading view of a memory note (GA-68): its Markdown without its properties (the right panel has them), with
// [[wikilinks]] as links (dashed when they find no note yet: a click offers to make it), ![[embeds]] shown in place,
// card refs (KADE-12) as chips that open the card, and a preview when the mouse rests on a link. Raw HTML is never
// rendered; other links open in the system browser, as in MarkdownView.
import { createContext, useContext } from "react";
import Markdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { viewUrl } from "../../lib/markdown";
import { parseItemUrl } from "../../lib/itemLinks";
import { ItemChip } from "../MarkdownView";
import { linkedNote, section, TASK_REF, titleOf, wikilinks, withoutFrontmatter, type WikiLink } from "../../lib/memory";
import type { MemoryNote } from "../../types";

/** What the links in a note do, from the Memory page. */
export type NoteLinks = {
  notes: readonly MemoryNote[];
  /** The cards that exist, by identifier in capitals: only those refs become chips. */
  cards: ReadonlySet<string>;
  open: (note: MemoryNote, heading?: string) => void;
  /** A link that finds no note: offer to make it. */
  create: (link: WikiLink, from: MemoryNote | null) => void;
  /** The mouse rests on a link (its box on screen), or left it (null). */
  hover: (link: WikiLink | null, from: MemoryNote | null, at: DOMRect | null) => void;
};

type MdNode = { type: string; value?: string; url?: string; children?: MdNode[] };
const WIKI = "wiki:";
const EMBED = "wiki-embed:";

/** How a link reads: its alias, else what it names (`Note > Heading` for a heading). */
export const linkText = (l: WikiLink) => l.alias || (l.heading ? (l.target ? `${l.target} > ${l.heading}` : l.heading) : l.target);

/** A remark plugin: the wikilinks, embeds and card refs in the text (never in code or a link) become links that the
 *  view draws itself. */
function memoryLinks(cards: ReadonlySet<string>) {
  const split = (text: string): MdNode[] => {
    const found: { start: number; end: number; node: MdNode }[] = wikilinks(text, true).map((l) => ({
      start: l.start, end: l.end,
      node: { type: "link", url: (l.embed ? EMBED : WIKI) + encodeURIComponent(JSON.stringify(l)), children: [{ type: "text", value: linkText(l) }] },
    }));
    for (const m of text.matchAll(TASK_REF)) {
      const [start, end] = [m.index, m.index + m[0].length];
      if (!cards.has(m[0]) || found.some((f) => start < f.end && end > f.start)) continue;
      found.push({ start, end, node: { type: "link", url: `gizai:task/${m[0]}`, children: [{ type: "text", value: m[0] }] } });
    }
    if (!found.length) return [{ type: "text", value: text }];
    found.sort((a, b) => a.start - b.start);
    const out: MdNode[] = [];
    let at = 0;
    for (const f of found) {
      if (f.start > at) out.push({ type: "text", value: text.slice(at, f.start) });
      out.push(f.node);
      at = f.end;
    }
    if (at < text.length) out.push({ type: "text", value: text.slice(at) });
    return out;
  };
  const walk = (node: MdNode) => {
    if (!node.children) return;
    node.children = node.children.flatMap((c) => {
      if (c.type === "text" && c.value) return split(c.value);
      if (c.type !== "link" && c.type !== "linkReference") walk(c);
      return [c];
    });
  };
  return () => (tree: MdNode) => { walk(tree); };
}

const keepUrl = (url: string) => (url.startsWith(WIKI) || url.startsWith(EMBED) ? url : viewUrl(url));
const parseLink = (href: string, prefix: string): WikiLink | null => {
  try { return JSON.parse(decodeURIComponent(href.slice(prefix.length))) as WikiLink; } catch { return null; }
};

/** How deep embeds go (an embed in an embed), so two notes that embed each other end. */
const MAX_DEPTH = 2;
const Depth = createContext(0);

/** A note's text as the reading view shows it: `from` is the note it is in (its folder decides what a title finds).
 *  `frontmatter`: the text has no properties block to take off (taken off already, or a part of a note). */
export function NoteView({ md, from, links, frontmatter = false }: { md: string; from: MemoryNote | null; links: NoteLinks; frontmatter?: boolean }) {
  const components: Components = {
    a: ({ href, children }) => {
      const url = href ?? "";
      if (url.startsWith(WIKI) || url.startsWith(EMBED)) {
        const embed = url.startsWith(EMBED);
        const l = parseLink(url, embed ? EMBED : WIKI);
        if (!l) return <>{children}</>;
        const note = l.target ? linkedNote(links.notes, l.target, from) : from;
        if (embed) return <Embed link={l} note={note} from={from} links={links} />;
        return (
          <a href="#" className={`wikilink${note ? "" : " missing"}`} title={note ? note.path : `No note called ${l.target} yet: click to make it`}
            onClick={(e) => { e.preventDefault(); e.stopPropagation(); links.hover(null, null, null); if (note) links.open(note, l.heading); else links.create(l, from); }}
            onMouseEnter={(e) => links.hover(l, from, e.currentTarget.getBoundingClientRect())} onMouseLeave={() => links.hover(null, null, null)}>
            {children}
          </a>
        );
      }
      const item = parseItemUrl(url);
      if (item) return <ItemChip kind={item.kind} itemKey={item.key}>{children}</ItemChip>;
      return (
        <a href={url || undefined} title={url || undefined}
          onClick={(e) => { e.preventDefault(); e.stopPropagation(); if (url) openUrl(url).catch(() => {}); }}>{children}</a>
      );
    },
    // A paragraph that is only an embed is the embed's box.
    p: ({ node, children }) => {
      const kids = (node?.children ?? []).filter((c) => !(c.type === "text" && !c.value.trim()));
      const lone = kids.length === 1 && kids[0]?.type === "element" && kids[0].tagName === "a" && String(kids[0].properties?.href ?? "").startsWith(EMBED);
      return lone ? <div className="embed-para">{children}</div> : <p>{children}</p>;
    },
    img: ({ alt }) => <span className="faint">[image{alt ? `: ${alt}` : ""}]</span>,
  };
  return (
    <div className="prose note-view">
      <Markdown remarkPlugins={[remarkGfm, memoryLinks(links.cards)]} skipHtml urlTransform={keepUrl} components={components}>
        {frontmatter ? md : withoutFrontmatter(md)}
      </Markdown>
    </div>
  );
}

/** `![[Note]]` (or `![[Note#Heading]]`): the note's text in a box, with its title as a link; a link that finds no
 *  note offers to make it. */
function Embed({ link, note, from, links }: { link: WikiLink; note: MemoryNote | null; from: MemoryNote | null; links: NoteLinks }) {
  const depth = useContext(Depth);
  if (!note) {
    return (
      <span className="embed missing">
        No note called {link.target} yet. <a href="#" onClick={(e) => { e.preventDefault(); links.create(link, from); }}>Make it</a>
      </span>
    );
  }
  const body = link.heading ? section(note.bodyMd, link.heading) : withoutFrontmatter(note.bodyMd);
  return (
    <span className="embed" role="group" aria-label={`Embedded: ${note.path}`}>
      <a href="#" className="embed-title" title={`Open ${note.path}`} onClick={(e) => { e.preventDefault(); links.open(note, link.heading); }}>
        {titleOf(note.path)}{link.heading ? ` > ${link.heading}` : ""}
      </a>
      {depth >= MAX_DEPTH || (from && note.id === from.id)
        ? <span className="faint">(Embedded again here: open it to read it.)</span>
        : body === null ? <span className="faint">{titleOf(note.path)} has no heading {link.heading}.</span>
        : <Depth.Provider value={depth + 1}><NoteView md={body} from={note} links={links} frontmatter /></Depth.Provider>}
    </span>
  );
}
