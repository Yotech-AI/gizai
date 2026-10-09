import { Fragment } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { viewUrl } from "../lib/markdown";
import { KIND_NAME, parseItemUrl, splitItemLinks, type ItemKind } from "../lib/itemLinks";
import { openItem } from "../lib/openItem";

/** Plain text (a message you sent in the chat) with its links to Gizai items shown as chips. */
export function LinkedText({ text }: { text: string }) {
  return <>{splitItemLinks(text).map((p, i) => "link" in p
    ? <ItemChip key={i} kind={p.link.kind} itemKey={p.link.key}>{p.link.label}</ItemChip>
    : <Fragment key={i}>{p.text}</Fragment>)}</>;
}

/** A link to a Gizai item (the @ picker's): a chip with its name that opens the item's page in Gizai. */
export function ItemChip({ kind, itemKey, children }: { kind: ItemKind; itemKey: string; children: React.ReactNode }) {
  return (
    <a className={`item-chip kind-${kind}`} href={`gizai:${kind}/${encodeURIComponent(itemKey)}`} title={`Open ${KIND_NAME[kind].toLowerCase()}`}
      onClick={(e) => { e.preventDefault(); e.stopPropagation(); openItem(kind, itemKey).catch(() => {}); }}>{children}</a>
  );
}

/** Renders Markdown (GFM). Raw HTML is never rendered; links are http(s)/mailto, which open in the system browser, or gizai:
 *  links to items, which show as chips and open in Gizai. */
export function MarkdownView({ md }: { md: string }) {
  return (
    <div className="prose">
      <Markdown remarkPlugins={[remarkGfm]} skipHtml urlTransform={viewUrl}
        components={{
          a: ({ href, children }) => {
            const item = parseItemUrl(href);
            if (item) return <ItemChip kind={item.kind} itemKey={item.key}>{children}</ItemChip>;
            return (
              <a href={href || undefined} title={href || undefined}
                onClick={(e) => { e.preventDefault(); e.stopPropagation(); if (href) openUrl(href).catch(() => {}); }}>{children}</a>
            );
          },
          // Remote images are blocked by the content security policy; show the alt text instead.
          img: ({ alt }) => <span className="faint">[image{alt ? `: ${alt}` : ""}]</span>,
        }}>
        {md}
      </Markdown>
    </div>
  );
}
