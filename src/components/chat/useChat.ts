// A thread's messages plus what the Team Lead is writing right now, kept live from the backend's events.
import { useCallback, useEffect, useRef, useState } from "react";
import { chatLive, chatMessages, onChatChanged, onChatEvent } from "../../api";
import type { ChatMessage, ChatStatus } from "../../types";

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

export function useChat(threadId: string | null) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [tool, setTool] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const current = useRef(threadId);
  current.current = threadId;

  const reload = useCallback(() => {
    const id = current.current;
    if (!id) { setMessages([]); setWorking(false); setDraft(""); setTool(null); return; }
    chatMessages(id).then((m) => { if (current.current === id) { setMessages(m); setError(null); } }).catch((e) => setError(String(e)));
    chatLive().then((l) => {
      if (current.current !== id) return;
      const mine = l.find((x) => x.threadId === id);
      setWorking(!!mine);
      setDraft(mine?.draft ?? "");
      setTool(mine?.tool ?? null);
    }).catch(() => {});
  }, []);

  useEffect(() => { reload(); }, [threadId, reload]);

  useEffect(() => {
    let alive = true;
    const uns: (() => void)[] = [];
    onChatEvent((e) => {
      if (e.threadId !== current.current) return;
      if (e.kind === "delta") setDraft((d) => d + e.text);
      else if (e.kind === "block") setDraft("");
      else if (e.kind === "tool") setTool(e.name);
      else if (e.kind === "message") {
        setMessages((list) => upsert(list, e.message));
        if (e.message.role === "agent") setDraft("");
        if (e.message.role === "tool" && e.message.tool && "result" in e.message.tool) setTool(null);
        if (e.message.role === "user") setWorking(true);
      }
    }).then((f) => (alive ? uns.push(f) : f())).catch(() => {});
    onChatChanged(reload).then((f) => (alive ? uns.push(f) : f())).catch(() => {});
    return () => { alive = false; uns.forEach((u) => u()); };
  }, [reload]);

  return { messages, draft, tool, working, error, setWorking, reload };
}
