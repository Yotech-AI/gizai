// One speaker's run of messages: your text on the right, the Team Lead's text and tool calls on the left.
import { useState } from "react";
import { Check, ChevronRight, CircleAlert, Info } from "lucide-react";
import { MarkdownView } from "../MarkdownView";
import { Avatar } from "../Avatar";
import { relTime } from "../../lib/format";
import { toolCard, type Group } from "../../lib/chat";
import type { ChatMessage } from "../../types";

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

export function MessageGroup({ g, agentRole, live }: { g: Group; agentRole?: string | null; live?: React.ReactNode }) {
  if (g.side === "system") {
    const m = g.items[0];
    const bad = (m.bodyMd ?? "").startsWith("The Team Lead couldn't") || (m.bodyMd ?? "").startsWith("Gizai's tools didn't");
    return <div className={`chat-note${bad ? " bad" : ""}`} role={bad ? "alert" : undefined}>{bad ? <CircleAlert className="icon sm" /> : <Info className="icon sm" />}<span>{m.bodyMd}</span></div>;
  }
  if (g.side === "user") {
    return (
      <div className="chat-group user">
        {g.items.map((m) => <div key={m.id} className="chat-msg user"><div className="bubble">{m.bodyMd}</div></div>)}
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
