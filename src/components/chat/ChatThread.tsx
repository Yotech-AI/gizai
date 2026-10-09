// The conversation in one thread (or a new, empty one), the live answer, the messages queued meanwhile, and the
// composer with Runs on (the coding CLI the chat's answers run on).
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowUp, Pencil, Square, X } from "lucide-react";
import { answerChatOn, chatClis, editQueuedChat, removeQueuedChat, sendChat, sendChatQueue, setAgentStatus, setChatCli, stopChat } from "../../api";
import { go } from "../../router";
import { chatRunsOn, groupMessages, SUGGESTIONS, toolName } from "../../lib/chat";
import type { ChatCli, ChatMessage, ChatThread as Thread, Member, QueuedMessage } from "../../types";
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
      <textarea ref={ref} rows={1} value={value} disabled={disabled} aria-label="Message the Team Lead"
        placeholder={working ? "Queue a message for when this answer is done…" : "Message the Team Lead…"}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          // While the Team Lead answers, Enter queues the message: it goes when the answer is done.
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); onSend(); }
        }} />
      {working && <button className="btn sm stop-btn" onClick={onStop} title="Stop the answer"><Square className="icon sm" />Stop</button>}
      {(!working || value.trim()) && (
        <button className="btn primary sm icon-only" onClick={onSend} disabled={disabled || busy || !value.trim()} aria-label={working ? "Queue" : "Send"}
          title={working ? "Queue: it goes when this answer is done (Enter)" : "Send (Enter)"}><ArrowUp className="icon" /></button>
      )}
    </div>
  );
}

/** Runs on, under the text box: the coding CLIs from Settings; those that can't run the chat are listed but disabled, with why. */
function RunsOn({ value, clis, disabled, onChange }: { value: string; clis: ChatCli[] | null; disabled: boolean; onChange: (id: string) => void }) {
  const known = clis?.some((c) => c.id === value);
  return (
    <label className="runs-on" title={disabled ? "Runs on can change when this answer is done; a change applies from the next message" : "The coding CLI this chat's answers run on"}>
      <span>Runs on</span>
      <select className="select" aria-label="Runs on" value={value} disabled={disabled || !clis} onChange={(e) => onChange(e.target.value)}>
        {(!clis || !known) && <option value={value}>{clis ? value : "Loading…"}</option>}
        {clis?.map((c) => <option key={c.id} value={c.id} disabled={!!c.problem}>{c.name}{c.problem ? ` (${c.problem})` : ""}</option>)}
      </select>
    </label>
  );
}

/** A message queued while the Team Lead answers: it goes when the answer is done, or waits for Send now. */
function Queued({ q, waiting, onError }: { q: QueuedMessage; waiting: boolean; onError: (e: string) => void }) {
  const [edit, setEdit] = useState<string | null>(null);
  const save = () => {
    if (edit === null || !edit.trim()) return;
    editQueuedChat(q.id, edit).then(() => setEdit(null)).catch((e) => onError(String(e)));
  };
  return (
    <div className="chat-msg user queued">
      {edit === null ? <div className="bubble">{q.bodyMd}</div> : (
        <div className="bubble editing">
          <textarea className="input" aria-label="Edit the queued message" value={edit} autoFocus rows={Math.min(8, edit.split("\n").length + 1)}
            onChange={(e) => setEdit(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); save(); } if (e.key === "Escape") setEdit(null); }} />
          <div className="q-edit"><button className="btn sm" onClick={() => setEdit(null)}>Cancel</button><button className="btn sm primary" onClick={save} disabled={!edit.trim()}>Save</button></div>
        </div>
      )}
      <div className="q-meta">
        <span>{waiting ? "Waiting: the answer before it didn't finish" : "Queued: goes when this answer is done"}</span>
        {edit === null && <button className="btn ghost sm icon-only" aria-label="Edit" title="Edit" onClick={() => setEdit(q.bodyMd)}><Pencil className="icon sm" /></button>}
        <button className="btn ghost sm icon-only" aria-label="Remove" title="Remove" onClick={() => removeQueuedChat(q.id).catch((e) => onError(String(e)))}><X className="icon sm" /></button>
      </div>
    </div>
  );
}

export function ChatThread({ threadId, thread, agent }: { threadId: string | null; thread?: Thread; agent: Member }) {
  const { messages, draft, tool, working, queue, error, setWorking } = useChat(threadId);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [clis, setClis] = useState<ChatCli[] | null>(null);
  // Runs on picked for a new chat, before its first message (null: the Team Lead's).
  const [newCli, setNewCli] = useState<string | null>(null);
  const scroller = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  const paused = agent.status !== "active";

  // The CLIs are asked once per chat page: finding their programs asks your login shell.
  useEffect(() => { let alive = true; chatClis().then((c) => alive && setClis(c)).catch(() => alive && setClis([])); return () => { alive = false; }; }, []);
  useEffect(() => { setSendError(null); setNewCli(null); pinned.current = true; }, [threadId]);
  // Follow the answer while you are at the bottom; leave the view alone when you scrolled up to read.
  useLayoutEffect(() => {
    const s = scroller.current;
    if (s && pinned.current) s.scrollTop = s.scrollHeight;
  }, [messages, draft, working, tool, queue]);

  const runsOn = chatRunsOn(threadId ? thread?.cli : newCli, agent.adapter, clis);
  const pickCli = (id: string) => {
    setSendError(null);
    if (!threadId) { setNewCli(id); return; }
    setChatCli(threadId, id).catch((e) => setSendError(String(e)));
  };

  const send = async () => {
    const text = value.trim();
    if (!text || busy) return;
    setBusy(true);
    setSendError(null);
    try {
      const id = await sendChat(threadId, text, threadId ? null : newCli);
      setValue("");
      if (!working) setWorking(true);
      pinned.current = true;
      if (id !== threadId) go({ page: "chat", id });
    } catch (e) {
      setSendError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // The queue goes by itself when the answer is done; after a stopped or failed answer (or a restart) it waits.
  const waiting = queue.filter((q) => !working || q.held);
  const queueBits = queue.length > 0 && threadId ? (
    <div className="chat-queue" aria-label="Queued messages">
      {queue.map((q) => <Queued key={q.id} q={q} waiting={!working || q.held} onError={setSendError} />)}
      {waiting.length > 0 && (
        <div className="q-actions">
          <button className="btn sm primary" onClick={() => sendChatQueue(threadId).catch((e) => setSendError(String(e)))}
            title={working ? "They go when this answer is done" : "They go together, in the order written"}>Send now</button>
          <button className="btn sm" onClick={() => Promise.all(waiting.map((q) => removeQueuedChat(q.id))).catch((e) => setSendError(String(e)))}>Remove</button>
        </div>
      )}
    </div>
  ) : null;

  // Under a usage-limit note that is the chat's last word: answer on another Claude Code entry instead.
  const lastId = messages[messages.length - 1]?.id;
  const noteActions = (m: ChatMessage) => {
    if (m.meta?.kind !== "limit" || m.id !== lastId || working || !threadId) return null;
    const others = (clis ?? []).filter((c) => c.kind === "claude_code" && !c.problem && c.id !== m.meta?.cli);
    if (!clis) return null;
    if (others.length === 0) return <span className="faint small">Add another Claude Code account in Settings → Coding CLIs to answer on it.</span>;
    return others.map((c) => (
      <button key={c.id} className="btn sm" onClick={() => { pinned.current = true; answerChatOn(threadId, c.id, m.id).catch((e) => setSendError(String(e))); }}>
        Answer on {c.name}</button>
    ));
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
          {groups.map((g, i) => <MessageGroup key={g.key} g={g} agentRole={agent.roleKey} noteActions={noteActions}
            live={i === groups.length - 1 && g.side === "agent" ? liveBits : undefined} />)}
          {working && last?.side !== "agent" && (
            <MessageGroup g={{ side: "agent", key: "live", at: Date.now(), author: agent.name, items: [] }} agentRole={agent.roleKey} live={liveBits} />
          )}
          {queueBits}
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
          <div className="composer-foot">
            <div className="composer-hint"><span><span className="kbd">Enter</span> {working ? "queues" : "sends"}</span><span><span className="kbd">Shift</span> <span className="kbd">Enter</span> new line</span></div>
            <RunsOn value={runsOn} clis={clis} disabled={working} onChange={pickCli} />
          </div>
        </div>
      </div>
    </section>
  );
}
