import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import { safeUrl } from "../lib/markdown";

/** Renders Markdown (GFM). Raw HTML is never rendered; links are http(s)/mailto only and open in the system browser. */
export function MarkdownView({ md }: { md: string }) {
  return (
    <div className="prose">
      <Markdown remarkPlugins={[remarkGfm]} skipHtml urlTransform={safeUrl}
        components={{
          a: ({ href, children }) => (
            <a href={href || undefined} title={href || undefined}
              onClick={(e) => { e.preventDefault(); e.stopPropagation(); if (href) openUrl(href).catch(() => {}); }}>{children}</a>
          ),
          // Remote images are blocked by the content security policy; show the alt text instead.
          img: ({ alt }) => <span className="faint">[image{alt ? `: ${alt}` : ""}]</span>,
        }}>
        {md}
      </Markdown>
    </div>
  );
}
