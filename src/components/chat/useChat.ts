// A thread's messages plus what the Team Lead is writing right now, kept live from the backend's events, and the
// messages queued while it answers.
import { useCallback, useEffect, useRef, useState } from "react";
import { chatLive, chatMessages, chatQueue, onChatChanged, onChatEvent } from "../../api";
import { applyDraft, withSnapshot, type DraftChange, type LiveDraft } from "../../lib/chat";
import type { ChatMessage, ChatStatus, QueuedMessage } from "../../types";

/** Every chat turn running right now (any thread), refreshed when one starts or ends. */
export function useChatLive(): ChatStatus[] {
  const [live, setLive] = useState<ChatStatus[]>([]);
  useEffect(() => {
    let alive = true;
    let un: (() => void) | undefined;
    const load = () => { chatLive().then((l) => { if (alive) setLive(l); }).catch(() => {}); };
    load();
    onChatChanged(load).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, []);
  return live;
}

function upsert(list: ChatMessage[], m: ChatMessage): ChatMessage[] {
  const i = list.findIndex((x) => x.id === m.id);
  if (i < 0) return [...list, m];
  const next = list.slice();
  next[i] = m;
  return next;
}

const EMPTY: LiveDraft = { text: "", seq: 0 };

export function useChat(threadId: string | null) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [live, setLive] = useState<LiveDraft>(EMPTY);
  const [tool, setTool] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [queue, setQueue] = useState<QueuedMessage[]>([]);
  const [error, setError] = useState<string | null>(null);
  const current = useRef(threadId);
  current.current = threadId;
  // The changes to the text being written heard since its last block: a snapshot gets those after it added.
  const heard = useRef<DraftChange[]>([]);

  const reload = useCallback(() => {
    const id = current.current;
    if (!id) { setMessages([]); setWorking(false); setLive(EMPTY); setTool(null); setQueue([]); return; }
    chatMessages(id).then((m) => { if (current.current === id) { setMessages(m); setError(null); } }).catch((e) => setError(String(e)));
    chatQueue(id).then((q) => { if (current.current === id) setQueue(q); }).catch(() => {});
    chatLive().then((l) => {
      if (current.current !== id) return;
      const mine = l.find((x) => x.threadId === id);
      setWorking(!!mine);
      setTool(mine?.tool ?? null);
      setLive((d) => {
        if (!mine) return { text: "", seq: d.seq };
        // Words that arrived while this was asked for are in `heard`, not in the snapshot: keep them.
        const joined = withSnapshot({ text: mine.draft, seq: mine.seq }, heard.current);
        return joined.seq >= d.seq ? joined : d;
      });
    }).catch(() => {});
  }, []);

  useEffect(() => { heard.current = []; setLive(EMPTY); reload(); }, [threadId, reload]);

  useEffect(() => {
    let alive = true;
    const uns: (() => void)[] = [];
    onChatEvent((e) => {
      if (e.threadId !== current.current) return;
      if (e.kind === "delta" || e.kind === "block") {
        const c: DraftChange = e.kind === "delta" ? { kind: "delta", text: e.text, seq: e.seq } : { kind: "block", seq: e.seq };
        if (c.kind === "block") heard.current = [c];
        else heard.current.push(c);
        setLive((d) => applyDraft(d, c));
      }
      else if (e.kind === "tool") setTool(e.name);
      else if (e.kind === "message") {
        setMessages((list) => upsert(list, e.message));
        if (e.message.role === "tool" && e.message.tool && "result" in e.message.tool) setTool(null);
        if (e.message.role === "user") setWorking(true);
      }
    }).then((f) => (alive ? uns.push(f) : f())).catch(() => {});
    onChatChanged(reload).then((f) => (alive ? uns.push(f) : f())).catch(() => {});
    return () => { alive = false; uns.forEach((u) => u()); };
  }, [reload]);

  return { messages, draft: live.text, tool, working, queue, error, setWorking, reload };
}
