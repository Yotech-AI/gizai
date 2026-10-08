// Chat with the Team Lead (design system: ChatMessage, ToolCard, ChatSetup): threads on the left, the
// conversation in the middle. Without a Team Lead, a panel explains how to set one up.
import { Crown, MessagesSquare, Plus, Settings } from "lucide-react";
import { chatAgent, listChatThreads } from "../api";
import { go, href } from "../router";
import { useData } from "../lib/useData";
import { useDrawer } from "../lib/drawers";
import { useCurrentTeam } from "../lib/team";
import { relTime } from "../lib/format";
import { chatLabel } from "../lib/inbox";
import { Avatar } from "../components/Avatar";
import { ChatThread } from "../components/chat/ChatThread";
import { useChatLive } from "../components/chat/useChat";

function ChatSetup() {
  const open = useDrawer();
  const [teamId] = useCurrentTeam();
  return (
    <div className="chat-setup">
      <div className="panel setup-card">
        <span className="setup-icon"><MessagesSquare className="icon lg" /></span>
        <h2>Chat with your Team Lead</h2>
        <p className="muted">To start chatting, set up the Team Lead: an agent with Chat turned on. It runs Claude Code with Gizai's tools, so you can ask it to add clients, projects and tasks, set up agents, write docs and read your inbox.</p>
        <button className="btn primary" onClick={() => { go({ page: "team" }); open({ kind: "agent", teamId, preset: { name: "Team Lead", role: "lead", chat: true } }); }}>
          <Crown className="icon" />Set up the Team Lead</button>
        <p className="faint small">In chat it works in a folder of its own and can read your linked repositories. It doesn't edit code or run commands: it hands that work to your agents as tasks.</p>
      </div>
    </div>
  );
}

export function ChatPage({ id }: { id?: string }) {
  const agent = useData(() => chatAgent().then((a) => ({ a })));
  const threads = useData(() => listChatThreads());
  const live = useChatLive();
  const open = useDrawer();
  const lead = agent.data?.a ?? null;
  const thread = threads.data?.find((t) => t.id === id);
  const working = live.length > 0;
  return (
    <>
      <div className="topbar">
        <div className="crumbs"><a href={href({ page: "chat" })}>Chat</a>{thread && <><span className="sep">/</span><span className="ellipsis">{thread.title}</span></>}</div>
        {lead && (
          <div className="actions">
            <a className="lead-chip" href={href({ page: "agent", id: lead.actorId })} title={`${lead.name}: open its page`}>
              <Avatar name={lead.name} kind="agent" size="sm" role={lead.roleKey} /><span>{lead.name}</span>
              {working ? <span className="live-tag"><span className="pulse" />working</span> : lead.status !== "active" ? <span className="faint">paused</span> : null}
            </a>
            <button className="btn ghost sm icon-only" title={`${lead.name} settings`} aria-label={`${lead.name} settings`} onClick={() => open({ kind: "agent", id: lead.actorId })}><Settings className="icon" /></button>
            <button className="btn sm" onClick={() => go({ page: "chat" })}><Plus className="icon" />New chat</button>
          </div>
        )}
      </div>
      {agent.error && <div className="error-banner">{agent.error}</div>}
      {agent.data && !lead && <ChatSetup />}
      {lead && (
        <div className="chat">
          <aside className="chat-threads" aria-label="Chats">
            <a className={`th${!id ? " on" : ""}`} href={href({ page: "chat" })}><span className="t"><Plus className="icon sm" />New chat</span></a>
            {(threads.data ?? []).length > 0 && <div className="nav-label">Recent</div>}
            {(threads.data ?? []).map((t) => (
              <a key={t.id} className={`th${t.id === id ? " on" : ""}`} href={href({ page: "chat", id: t.id })} aria-current={t.id === id ? "page" : undefined}>
                <span className="t">{live.some((l) => l.threadId === t.id) && <span className="pulse" />}
                  {chatLabel(t) && <span className={`badge ${t.waiting ? "needs" : "outline"}`} title={t.waiting ? "The Team Lead asks you this" : "The Team Lead started this chat"}>{chatLabel(t)}</span>}
                  <span className="ellipsis">{t.title}</span></span>
                <span className="when">{relTime(t.updatedAt)}</span>
              </a>
            ))}
          </aside>
          <ChatThread threadId={id ?? null} thread={thread} agent={lead} />
        </div>
      )}
    </>
  );
}
