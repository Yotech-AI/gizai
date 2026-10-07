// The conversation in one thread (or a new, empty one), the live answer, and the composer.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowUp, Square } from "lucide-react";
import { sendChat, setAgentStatus, stopChat } from "../../api";
import { go } from "../../router";
import { groupMessages, SUGGESTIONS, toolName } from "../../lib/chat";
import type { Member } from "../../types";
import { MarkdownView } from "../MarkdownView";
import { MessageGroup } from "./ChatMessage";
import { Avatar } from "../Avatar";
import { useChat } from "./useChat";

function Composer({ value, setValue, onSend, onStop, working, disabled, busy }: {
  value: string; setValue: (v: string) => void; onSend: () => void; onStop: () => void; working: boolean; disabled: boolean; busy: boolean;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const t = ref.current;
    if (!t) return;
    t.style.height = "auto";
    t.style.height = `${Math.min(t.scrollHeight, 200)}px`;
  }, [value]);
  useEffect(() => { if (!disabled) ref.current?.focus(); }, [disabled]);
  return (
    <div className="composer-box">
      <textarea ref={ref} rows={1} value={value} disabled={disabled} aria-label="Message the Team Lead" placeholder="Message the Team Lead…"
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); if (!working) onSend(); }
        }} />
      {working
        ? <button className="btn sm stop-btn" onClick={onStop} title="Stop the answer"><Square className="icon sm" />Stop</button>
        : <button className="btn primary sm icon-only" onClick={onSend} disabled={disabled || busy || !value.trim()} aria-label="Send" title="Send (Enter)"><ArrowUp className="icon" /></button>}
    </div>
  );
}

export function ChatThread({ threadId, agent }: { threadId: string | null; agent: Member }) {
  const { messages, draft, tool, working, error, setWorking } = useChat(threadId);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const paused = agent.status !== "active";

  useEffect(() => { setSendError(null); pinned.current = true; }, [threadId]);
  // Follow the answer while you are at the bottom; leave the view alone when you scrolled up to read.
  useLayoutEffect(() => {
    const s = scroller.current;
    if (s && pinned.current) s.scrollTop = s.scrollHeight;
  }, [messages, draft, working, tool]);

  const send = async () => {
    const text = value.trim();
    if (!text || busy) return;
    setBusy(true);
    setSendError(null);
    try {
      const id = await sendChat(threadId, text);
      setValue("");
      setWorking(true);
      pinned.current = true;
      if (id !== threadId) go({ page: "chat", id });
    } catch (e) {
      setSendError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const groups = groupMessages(messages);
  const last = groups[groups.length - 1];
  const liveBits = working ? (
    <>
      {draft && <div className="chat-msg agent draft"><MarkdownView md={draft} /><span className="caret" aria-hidden /></div>}
      <div className="chat-working" role="status"><span className="pulse" />{tool ? `Using ${toolName(tool).replace(/_/g, " ")}` : draft ? "Writing" : "Thinking"}…</div>
    </>
  ) : null;

  return (
    <section className="chat-main">
      <div className="chat-scroll" ref={scroller} onScroll={(e) => { const s = e.currentTarget; pinned.current = s.scrollHeight - s.scrollTop - s.clientHeight < 80; }}>
        <div className="chat-col">
          {error && <div className="error-banner" role="alert">{error}</div>}
          {messages.length === 0 && !working && (
            <div className="chat-start">
              <Avatar name={agent.name} kind="agent" size="xl" role={agent.roleKey} />
              <h2>Ask {agent.name}</h2>
              <p className="muted">It can add clients, projects and tasks, set up agents, write docs and read your inbox. It looks things up before it changes them, and links what it made.</p>
              <div className="suggestions">
                {SUGGESTIONS.map((s) => <button key={s.label} className="btn" onClick={() => setValue(s.text)}>{s.label}</button>)}
              </div>
            </div>
          )}
          {groups.map((g, i) => <MessageGroup key={g.key} g={g} agentRole={agent.roleKey} live={i === groups.length - 1 && g.side === "agent" ? liveBits : undefined} />)}
          {working && last?.side !== "agent" && (
            <MessageGroup g={{ side: "agent", key: "live", at: Date.now(), author: agent.name, items: [] }} agentRole={agent.roleKey} live={liveBits} />
          )}
        </div>
      </div>
      <div className="chat-composer">
        <div className="chat-col">
          {paused && (
            <div className="chat-banner"><span>{agent.name} is paused, so it can't answer.</span>
              <button className="btn sm" onClick={() => setAgentStatus(agent.actorId, "active").catch((e) => setSendError(String(e)))}>Resume</button></div>
          )}
          {sendError && <div className="chat-banner bad" role="alert">{sendError}</div>}
          <Composer value={value} setValue={setValue} onSend={send} onStop={() => threadId && stopChat(threadId)} working={working} disabled={paused} busy={busy} />
          <div className="composer-hint"><span><span className="kbd">Enter</span> sends</span><span><span className="kbd">Shift</span> <span className="kbd">Enter</span> new line</span></div>
        </div>
      </div>
    </section>
  );
}
