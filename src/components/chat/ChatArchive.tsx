// Chat → Archive (#/chats): every chat, the newest activity first, and a search through their titles and the messages you
// and the Team Lead wrote. It takes the conversation's place; Recent stays on the left. Nothing here hides or deletes a chat.
import { Fragment, useEffect, useState } from "react";
import { onRowsChanged, searchChatThreads } from "../../api";
import { href } from "../../router";
import { relTime } from "../../lib/format";
import { chatLabel } from "../../lib/inbox";
import { highlight, hitAuthor, snippet } from "../../lib/chat";
import type { ChatHit, ChatStatus } from "../../types";

/** How long typing pauses before the search runs. */
const PAUSE_MS = 250;

/** The chats found for `query` (all of them for ""), asked again when a chat changes. `found.query`: the search they are for. */
function useFound(query: string) {
  const [found, setFound] = useState<{ query: string; hits: ChatHit[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    // A search that was overtaken by the next one is dropped when it answers.
    let alive = true;
    let un: (() => void) | undefined;
    const load = () => searchChatThreads(query).then((hits) => { if (alive) { setFound({ query, hits }); setError(null); } })
      .catch((e) => { if (alive) setError(String(e)); });
    load();
    onRowsChanged((table) => { if (alive && table.startsWith("chat")) load(); }).then((f) => (alive ? (un = f) : f())).catch(() => {});
    return () => { alive = false; un?.(); };
  }, [query]);
  return { found, error };
}

/** `text` with the matches of `query` highlighted. */
function Marked({ text, query }: { text: string; query: string }) {
  return <>{highlight(text, query).map((p, i) => (p.hit ? <mark key={i}>{p.text}</mark> : <Fragment key={i}>{p.text}</Fragment>))}</>;
}

export function ChatArchive({ live }: { live: ChatStatus[] }) {
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  useEffect(() => { const t = setTimeout(() => setQuery(text.trim()), PAUSE_MS); return () => clearTimeout(t); }, [text]);
  const { found, error } = useFound(query);
  const hits = found?.hits ?? [];
  const q = found?.query ?? "";
  return (
    <section className="chat-main chat-archive" aria-label="Archive">
      <div className="archive-head">
        <div className="chat-col">
          <input className="input archive-search" type="search" autoFocus aria-label="Search all chats" placeholder="Search chat titles and messages"
            value={text} onChange={(e) => setText(e.target.value)} />
        </div>
      </div>
      <div className="chat-scroll">
        <div className="chat-col">
          {error && <div className="error-banner" role="alert">{error}</div>}
          {found && hits.length === 0 && (q
            ? <div className="empty"><b>No chats match “{q}”.</b><span>The search looks in chat titles and in your messages and the Team Lead's, not in tool calls or Gizai's notes.</span></div>
            : <div className="empty"><b>No chats yet.</b><span>Start one with New chat. Every chat stays here, also after it has left Recent.</span></div>)}
          {hits.length > 0 && (
            <div className="archive-list" role="list" aria-label={q ? "Chats found" : "All chats"}>
              {hits.map(({ thread: t, message: m }) => (
                <a key={t.id} role="listitem" className="archive-row" href={href({ page: "chat", id: t.id })}>
                  <span className="top">
                    {live.some((l) => l.threadId === t.id) && <span className="pulse" />}
                    {chatLabel(t) && <span className={`badge ${t.waiting ? "needs" : "outline"}`} title={t.waiting ? "The Team Lead asks you this" : "The Team Lead started this chat"}>{chatLabel(t)}</span>}
                    <span className="title" title={t.title}><Marked text={t.title} query={q} /></span>
                    <span className="when" title={new Date(t.updatedAt).toLocaleString("en-GB")}>{relTime(t.updatedAt)}</span>
                  </span>
                  {m && <span className="snippet"><span className="who">{hitAuthor(m)}:</span> <Marked text={snippet(m.bodyMd ?? "", q)} query={q} /></span>}
                </a>
              ))}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
