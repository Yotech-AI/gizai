// One speaker's run of messages: your text on the right (with its links to Gizai items as chips, and the files you added),
// the Team Lead's text and tool calls on the left.
import { useState } from "react";
import { Check, ChevronRight, CircleAlert, Info } from "lucide-react";
import { openFile } from "../../api";
import { LinkedText, MarkdownView } from "../MarkdownView";
import { Avatar } from "../Avatar";
import { fileExt } from "../../lib/files";
import { formatBytes, relTime } from "../../lib/format";
import { toolCard, type Group } from "../../lib/chat";
import type { ChatMessage, FileRow } from "../../types";

/** The files added to a message you sent (or queued): each opens with the system's app. */
export function MessageFiles({ files }: { files?: FileRow[] | null }) {
  const [error, setError] = useState<string | null>(null);
  if (!files?.length) return null;
  return (
    <>
      <ul className="files compact msg-files" aria-label="Files">
        {files.map((f) => (
          <li key={f.id}>
            <button type="button" className="file" title={`Open ${f.name}`} onClick={() => openFile(f.id).catch((e) => setError(String(e)))}>
              <span className="ext">{fileExt(f.name)}</span>
              <span className="fname"><b>{f.name}</b><span>{formatBytes(f.sizeBytes)}</span></span>
            </button>
          </li>
        ))}
      </ul>
      {error && <div className="msg-files-error">{error}</div>}
    </>
  );
}

/** Your message: its text with item chips, and its files. */
export function UserBubble({ text, files }: { text: string; files?: FileRow[] | null }) {
  return (
    <>
      {text.trim() && <div className="bubble"><LinkedText text={text} /></div>}
      <MessageFiles files={files} />
    </>
  );
}

function pretty(v: unknown): string {
  if (typeof v === "string") { try { return JSON.stringify(JSON.parse(v), null, 2); } catch { return v; } }
  return JSON.stringify(v, null, 2);
}

export function ToolCard({ m }: { m: ChatMessage }) {
  const [open, setOpen] = useState(false);
  const c = toolCard(m);
  const t = (m.tool ?? {}) as Record<string, unknown>;
  return (
    <div className={`tool-card ${c.state}`}>
      <div className="tc-row">
        <span className="tc-state" aria-hidden>{c.state === "running" ? <span className="pulse" /> : c.state === "error" ? <CircleAlert className="icon sm" /> : <Check className="icon sm" />}</span>
        <span className="tc-verb">{c.verb}</span>
        {c.label && (c.href ? <a className="tc-label" href={c.href}>{c.label}</a> : <span className="tc-label">{c.label}</span>)}
        <button className="tc-more" aria-expanded={open} aria-label={open ? "Hide details" : "Show details"} title="Details" onClick={() => setOpen((o) => !o)}>
          <ChevronRight className="icon sm" /></button>
      </div>
      {c.state === "error" && c.detail && <div className="tc-error">{c.detail}</div>}
      {open && (
        <div className="tc-detail">
          <div className="tc-h">Input</div><pre>{pretty(t.input ?? {})}</pre>
          {t.result !== undefined && <><div className="tc-h">Result</div><pre>{pretty(t.result)}</pre></>}
        </div>
      )}
    </div>
  );
}

/** `noteActions`: what a note of Gizai's offers under it (Answer on another CLI after a usage limit). */
export function MessageGroup({ g, agentRole, live, noteActions }: { g: Group; agentRole?: string | null; live?: React.ReactNode;
  noteActions?: (m: ChatMessage) => React.ReactNode }) {
  if (g.side === "system") {
    const m = g.items[0];
    const bad = m.meta?.kind === "limit" || (m.bodyMd ?? "").startsWith("The Team Lead couldn't") || (m.bodyMd ?? "").startsWith("Gizai's tools didn't");
    const actions = noteActions?.(m);
    const note = <div className={`chat-note${bad ? " bad" : ""}${actions ? " has-actions" : ""}`} role={bad ? "alert" : undefined}>{bad ? <CircleAlert className="icon sm" /> : <Info className="icon sm" />}<span>{m.bodyMd}</span></div>;
    return actions ? <>{note}<div className="chat-note-actions">{actions}</div></> : note;
  }
  if (g.side === "user") {
    return (
      <div className="chat-group user">
        {g.items.map((m) => <div key={m.id} className="chat-msg user"><UserBubble text={m.bodyMd ?? ""} files={m.files} /></div>)}
        <div className="chat-meta">{relTime(g.items[g.items.length - 1].createdAt)}</div>
      </div>
    );
  }
  return (
    <div className="chat-group agent">
      <div className="chat-head"><Avatar name={g.author ?? "Team Lead"} kind="agent" role={agentRole} /><b>{g.author ?? "Team Lead"}</b><span className="faint">{relTime(g.at)}</span></div>
      <div className="chat-body">
        {g.items.map((m) => m.role === "tool" ? <ToolCard key={m.id} m={m} /> : <div key={m.id} className="chat-msg agent"><MarkdownView md={m.bodyMd ?? ""} /></div>)}
        {live}
      </div>
    </div>
  );
}
