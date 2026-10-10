// The conversation in one thread (or a new, empty one), the live answer, the messages queued meanwhile, and the
// composer. The composer is one box: a Markdown editor without a toolbar (@ links a Gizai item), and under the text a row
// with + (add files, or link an item; a drop on the Chat page adds files too), the tips, Runs on (the coding CLI the
// chat's answers run on) and Send, or Stop while the Team Lead answers.
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { ArrowUp, AtSign, Paperclip, Pencil, Plus, Square, X } from "lucide-react";
import { answerChatOn, chatClis, checkFiles, editQueuedChat, removeQueuedChat, sendChat, sendChatQueue, setAgentStatus, setChatCli, stopChat } from "../../api";
import { go } from "../../router";
import { chatRunsOn, groupMessages, SUGGESTIONS, toolName } from "../../lib/chat";
import { addPaths } from "../../lib/files";
import { usePending, type Pending } from "../../lib/usePending";
import { useDropZone } from "../../lib/useDropZone";
import type { ChatCli, ChatMessage, ChatThread as Thread, Member, QueuedMessage } from "../../types";
import { MarkdownView } from "../MarkdownView";
import { MarkdownEditor, type EditorHandle } from "../MarkdownEditor";
import { PendingFileList, pickFiles } from "../FileDrop";
import { Popover } from "../Popover";
import { MessageGroup, UserBubble } from "./ChatMessage";
import { Avatar } from "../Avatar";
import { BusyButton } from "../BusyButton";
import { useChat } from "./useChat";

/** Stop, Send now and Answer on <CLI>: each spins from its click until the chat shows what it did. */
type ChatButtons = Pending<"stop" | "send" | `answer:${string}`>;

/** The text box on top; under it, in the same box, + on the left, the tips, `picker` (Runs on), and Send (Stop while the
 *  Team Lead answers) on the right. */
function Composer({ value, setValue, files, onAddFiles, onRemoveFile, onSend, onStop, working, disabled, busy, pending, editor, picker }: {
  value: string; setValue: (v: string) => void; files: string[]; onAddFiles: (paths: string[]) => void; onRemoveFile: (path: string) => void;
  onSend: (text?: string) => void; onStop: () => void; working: boolean; disabled: boolean; busy: boolean; pending: ChatButtons;
  editor: React.MutableRefObject<EditorHandle | null>; picker: ReactNode;
}) {
  useEffect(() => { if (!disabled) editor.current?.focus(); }, [disabled, editor]);
  const ready = !!value.trim() || files.length > 0;
  // A click in the box around the text (its edges, the tips) puts the cursor in the text, as in one big text field.
  const toText = (e: React.MouseEvent) => {
    if (disabled || (e.target as Element).closest("button, select, label, a, .md-editor, .pop")) return;
    e.preventDefault();
    editor.current?.focus();
  };
  return (
    <div className={`composer-box${disabled ? " disabled" : ""}`} onMouseDown={toText}>
      <PendingFileList paths={files} onRemove={onRemoveFile} disabled={disabled} compact />
      {/* While the Team Lead answers, Enter queues the message: it goes when the answer is done. */}
      <MarkdownEditor className="composer-editor" toolbar={false} value={value} onChange={setValue} onEnter={(md) => onSend(md)} disabled={disabled}
        pickerUp handle={editor} ariaLabel="Message the Team Lead"
        placeholder={working ? "Queue a message for when this answer is done…" : "Message the Team Lead…"} />
      <div className="composer-foot">
        <Popover up label="Add to this message" disabled={disabled} button={(open) => (
          <button type="button" className={`btn ghost sm icon-only composer-plus${open ? " on" : ""}`} disabled={disabled}
            aria-label="Add files or link an item" title="Add files or link an item"><Plus className="icon" /></button>
        )}>
          {(close) => (
            <>
              <button type="button" role="menuitem" className="opt" onClick={() => { close(); pickFiles().then(onAddFiles).catch(() => {}); }}>
                <Paperclip className="icon sm" />Add files</button>
              <button type="button" role="menuitem" className="opt" onClick={() => { close(); editor.current?.startLink(); }}>
                <AtSign className="icon sm" />Link an item</button>
            </>
          )}
        </Popover>
        {/* The tips that don't fit the row's width are left out, the last first. */}
        <div className="composer-hint"><span><span className="kbd">Enter</span> {working ? "queues" : "sends"}</span><span><span className="kbd">Shift</span> <span className="kbd">Enter</span> new line</span>
          <span><span className="kbd">@</span> links a task, project, client or agent</span></div>
        {picker}
        {working && <BusyButton className="btn sm stop-btn" pending={pending} name="stop" busyLabel="Stopping…" icon={<Square className="icon sm" />} onClick={onStop}
          title="Stop the answer">Stop</BusyButton>}
        {(!working || ready) && (
          <button className="btn primary sm icon-only" onClick={() => onSend()} disabled={disabled || busy || !ready} aria-label={working ? "Queue" : "Send"}
            title={working ? "Queue: it goes when this answer is done (Enter)" : "Send (Enter)"}><ArrowUp className="icon" /></button>
        )}
      </div>
    </div>
  );
}

/** A coding CLI as Runs on lists it: its name, and why it can't run the chat. */
const cliLabel = (c: ChatCli) => `${c.name}${c.problem ? ` (${c.problem})` : ""}`;

/** Runs on, in the composer's bottom row: the coding CLIs from Settings; those that can't run the chat are listed but
 *  disabled, with why. */
function RunsOn({ value, clis, disabled, onChange }: { value: string; clis: ChatCli[] | null; disabled: boolean; onChange: (id: string) => void }) {
  const picked = clis?.find((c) => c.id === value);
  const shown = picked ? cliLabel(picked) : clis ? value : "Loading…";
  return (
    <label className="runs-on" title={disabled ? "Runs on can change when this answer is done; a change applies from the next message" : "The coding CLI this chat's answers run on"}>
      <span>Runs on</span>
      {/* A select is as wide as its longest option; the hidden copy of the picked one's name makes it as wide as that. */}
      <span className="runs-on-pick">
        <span className="runs-on-size" aria-hidden="true">{shown}</span>
        <select className="select" aria-label="Runs on" value={value} disabled={disabled || !clis} onChange={(e) => onChange(e.target.value)}>
          {!picked && <option value={value}>{shown}</option>}
          {clis?.map((c) => <option key={c.id} value={c.id} disabled={!!c.problem}>{cliLabel(c)}</option>)}
        </select>
      </span>
    </label>
  );
}

/** A message queued while the Team Lead answers: it goes when the answer is done, or waits for Send now. */
function Queued({ q, waiting, onError }: { q: QueuedMessage; waiting: boolean; onError: (e: string) => void }) {
  const [edit, setEdit] = useState<string | null>(null);
  // With files, the text may be empty.
  const empty = (text: string) => !text.trim() && !q.files?.length;
  const save = () => {
    if (edit === null || empty(edit)) return;
    editQueuedChat(q.id, edit).then(() => setEdit(null)).catch((e) => onError(String(e)));
  };
  return (
    <div className="chat-msg user queued">
      {edit === null ? <UserBubble text={q.bodyMd} files={q.files} /> : (
        <div className="bubble editing">
          <textarea className="input" aria-label="Edit the queued message" value={edit} autoFocus rows={Math.min(8, edit.split("\n").length + 1)}
            onChange={(e) => setEdit(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); save(); } if (e.key === "Escape") setEdit(null); }} />
          <div className="q-edit"><button className="btn sm" onClick={() => setEdit(null)}>Cancel</button><button className="btn sm primary" onClick={save} disabled={empty(edit)}>Save</button></div>
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
  // Files added to the message (+ → Add files, or dropped on the Chat page): their paths, until it is sent.
  const [files, setFiles] = useState<string[]>([]);
  const [fileNote, setFileNote] = useState<string | null>(null);
  const editor = useRef<EditorHandle | null>(null);
  const zone = useRef<HTMLElement>(null);
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
  // The queue goes by itself when the answer is done; after a stopped or failed answer (or a restart) it waits.
  const waiting = queue.filter((q) => !working || q.held);
  // Stop spins until the answer has stopped, Send now until its messages have gone, Answer on until the answer starts.
  const pending: ChatButtons = usePending((k) => k === "stop" ? !working : k === "send" ? waiting.length === 0 : working, threadId);
  const fail = (e: unknown) => setSendError(String(e));

  const runsOn = chatRunsOn(threadId ? thread?.cli : newCli, agent.adapter, clis);
  const pickCli = (id: string) => {
    setSendError(null);
    if (!threadId) { setNewCli(id); return; }
    setChatCli(threadId, id).catch((e) => setSendError(String(e)));
  };

  // A folder or a file over 1 GB is refused with why; the other files of the same pick or drop are added.
  const addFiles = async (paths: string[]) => {
    if (paths.length === 0) return;
    try {
      const r = await checkFiles(paths);
      if (r.ok.length) setFiles((f) => addPaths(f, r.ok));
      setFileNote(r.failed.length ? `Not added: ${r.failed.join("; ")}` : null);
    } catch (e) {
      setFileNote(String(e));
    }
  };
  // Files dropped anywhere on the Chat page go to the message (a drawer's Files box, open over it, takes them first).
  const dropping = useDropZone(zone, (paths) => { addFiles(paths); }, { on: !paused });

  // `typed`: the text as Enter saw it.
  const send = async (typed?: string) => {
    const text = (typed ?? value).trim();
    if ((!text && files.length === 0) || busy) return;
    setBusy(true);
    setSendError(null);
    try {
      const id = await sendChat(threadId, text, threadId ? null : newCli, files);
      setValue("");
      editor.current?.clear();
      setFiles([]);
      setFileNote(null);
      if (!working) setWorking(true);
      pinned.current = true;
      if (id !== threadId) go({ page: "chat", id });
    } catch (e) {
      setSendError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const queueBits = queue.length > 0 && threadId ? (
    <div className="chat-queue" aria-label="Queued messages">
      {queue.map((q) => <Queued key={q.id} q={q} waiting={!working || q.held} onError={setSendError} />)}
      {waiting.length > 0 && (
        <div className="q-actions">
          <BusyButton className="btn sm primary" pending={pending} name="send" busyLabel="Sending…" onClick={() => pending.act("send", () => sendChatQueue(threadId), fail)}
            title={working ? "They go when this answer is done" : "They go together, in the order written"}>Send now</BusyButton>
          <button className="btn sm" disabled={!!pending.busy} onClick={() => Promise.all(waiting.map((q) => removeQueuedChat(q.id))).catch((e) => setSendError(String(e)))}>Remove</button>
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
      <BusyButton key={c.id} className="btn sm" pending={pending} name={`answer:${c.id}`} busyLabel="Starting…"
        onClick={() => { pinned.current = true; pending.act(`answer:${c.id}`, () => answerChatOn(threadId, c.id, m.id), fail); }}>
        Answer on {c.name}</BusyButton>
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
    <section className={`chat-main${dropping ? " dropping" : ""}`} ref={zone}>
      {dropping && <div className="chat-drop" role="status"><span>Drop to add to this message</span></div>}
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
          {fileNote && (
            <div className="chat-banner warn" role="alert"><span>{fileNote}</span>
              <button className="btn ghost sm icon-only" aria-label="Close" title="Close" onClick={() => setFileNote(null)}><X className="icon sm" /></button></div>
          )}
          <Composer value={value} setValue={setValue} files={files} onAddFiles={addFiles} onRemoveFile={(p) => setFiles((f) => f.filter((x) => x !== p))}
            onSend={send} onStop={() => { if (threadId) pending.act("stop", () => stopChat(threadId), fail); }}
            working={working} disabled={paused} busy={busy} pending={pending} editor={editor}
            picker={<RunsOn value={runsOn} clis={clis} disabled={working} onChange={pickCli} />} />
        </div>
      </div>
    </section>
  );
}
