import { useEffect, useRef, useState } from "react";
import { Plus, RefreshCw, X } from "lucide-react";
import { addAgent, chatAgent, checkAgentFolders, claudeModels, getAgent, getTeam, listClis, roleTemplate, roleTools, saveAgentMcp, updateAgent } from "../api";
import { go } from "../router";
import { DEFAULT_TOOLS, draftFrom, foldersFrom, inputFrom, parseTools, ROLES, roleLabel, type AgentDraft, type AgentPreset } from "../lib/agents";
import { CLAUDE_CODE, EFFORTS_BY_KIND, FOLDERS_NOTE, KIND_LABEL, kindOf, modeFor, PERMISSIONS, RISKY, usesAllowedTools } from "../lib/clis";
import { cleanSwitches, sameSwitches } from "../lib/mcp";
import { effortChoices, findModel, modelHint } from "../lib/models";
import type { AgentFolder, CliKind, CliStatus, FolderCheck, Member, ModelOption } from "../types";
export type { AgentPreset } from "../lib/agents";
import { AgentToolsField } from "./AgentTools";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";

const EFFORT_LABEL: Record<string, string> = { minimal: "Minimal", low: "Low", medium: "Medium", high: "High", xhigh: "Extra high", max: "Max" };
const MODEL_EXAMPLE: Record<string, string> = { codex: "gpt-5-codex", gemini: "gemini-2.5-pro", other: "provider/model" };

/** The model picker: Claude Code's own list, an "Other model id…" escape, and a refresh. */
function ModelField({ value, onChange, models, error, onRefresh }: {
  value: string; onChange: (v: string) => void; models: ModelOption[] | null; error: string | null; onRefresh: () => void;
}) {
  const [other, setOther] = useState(false);
  const found = findModel(value, models);
  const unknown = !!models && !!value.trim() && !found;
  if (error || other) {
    return (
      <Field label="Model" htmlFor="a-model"
        hint={error ? `Couldn't ask Claude Code for its models (${error}). Type an alias like opus or a full id.` : "A full model id, like claude-opus-5-5"}>
        <div className="input-group">
          <input id="a-model" className="input" value={value} onChange={(e) => onChange(e.target.value)} placeholder="opus" />
          <button type="button" className="btn ghost sm" onClick={() => { setOther(false); if (error) onRefresh(); }}>{error ? "Try again" : "Back to the list"}</button>
        </div>
      </Field>
    );
  }
  return (
    <Field label="Model" htmlFor="a-model" hint={unknown ? null : modelHint(value, models)} warn={unknown ? modelHint(value, models) : null}>
      <div className="input-group">
        <select id="a-model" className="select" disabled={!models} value={unknown ? value : found?.value ?? "default"}
          onChange={(e) => e.target.value === "__other" ? setOther(true) : onChange(e.target.value === "default" ? "" : e.target.value)}>
          {!models && <option>Asking Claude Code for its models…</option>}
          {models?.map((m) => <option key={m.value} value={m.value}>{m.displayName}{m.resolvedModel && m.value !== "default" ? ` · ${m.resolvedModel}` : ""}</option>)}
          {unknown && <option value={value}>{value} (not offered)</option>}
          {models && <option value="__other">Other model id…</option>}
        </select>
        <button type="button" className="btn ghost sm icon-only" aria-label="Ask Claude Code again" title="Ask Claude Code again" onClick={onRefresh}><RefreshCw className="icon" /></button>
      </div>
    </Field>
  );
}

/** The agent's folders: a row each (path, read or read and change, remove), checked by Gizai as you type. */
function FoldersField({ value, onChange, kind, lead }: { value: AgentFolder[]; onChange: (v: AgentFolder[]) => void; kind: CliKind; lead: boolean }) {
  const [checks, setChecks] = useState<FolderCheck[]>([]);
  const filled = value.map((f, i) => (f.path.trim() ? i : -1)).filter((i) => i >= 0);
  const key = JSON.stringify(foldersFrom(value));
  useEffect(() => {
    const list: AgentFolder[] = JSON.parse(key);
    if (list.length === 0) { setChecks([]); return; }
    let alive = true;
    const t = setTimeout(() => { checkAgentFolders(list).then((c) => { if (alive) setChecks(c); }).catch(() => {}); }, 300);
    return () => { alive = false; clearTimeout(t); };
  }, [key]);
  const put = (i: number, f: AgentFolder) => onChange(value.map((x, j) => (j === i ? f : x)));
  return (
    <Field label="Folders" wide hint={<>
      Folders besides its card's worktree that its file tools may read, or read and change. They limit the file tools, not the commands it may run: an allowed command can still reach any folder.
      {" "}Never /, your home folder, Gizai's data folder or folders with keys (~/.ssh, ~/.gnupg, ~/.config, …).
      {" "}{FOLDERS_NOTE[kind]}
      {lead && " In chat the Team Lead only reads them; read and change also lets it update that folder (update_checkout) after you say yes in the chat."}
    </>}>
      <div className="folder-list">
        {value.map((f, i) => {
          const c = filled.includes(i) ? checks[filled.indexOf(i)] : undefined;
          return (
            <div key={i} className="folder-row">
              <div className="input-group">
                <input className="input mono" aria-label={`Folder ${i + 1}`} value={f.path} placeholder="~/Herd/shared" onChange={(e) => put(i, { ...f, path: e.target.value })} />
                <select className="select" style={{ flex: "0 0 170px" }} aria-label={`Access to folder ${i + 1}`} value={f.access}
                  onChange={(e) => put(i, { ...f, access: e.target.value as AgentFolder["access"] })}>
                  <option value="read">Read</option>
                  <option value="change">Read and change</option>
                </select>
                <button type="button" className="btn ghost sm icon-only" aria-label={`Remove folder ${i + 1}`} title="Remove" onClick={() => onChange(value.filter((_, j) => j !== i))}><X className="icon" /></button>
              </div>
              {c?.error ? <span className="error">{c.error}</span> : c?.warning ? <span className="warn">{c.warning}</span> : null}
            </div>
          );
        })}
        <div><button type="button" className="btn sm" onClick={() => onChange([...value, { path: "", access: "read" }])}><Plus className="icon" />Add folder</button></div>
      </div>
    </Field>
  );
}

const TOOL_SUGGESTIONS = ["Bash(pnpm:*)", "Bash(yarn:*)", "Bash(go test:*)", "Bash(make test:*)", "Bash(python -m pytest:*)"];
export const PERMISSION_HELP: Record<string, string> = PERMISSIONS.claude_code;

/** Add an agent to a team (no `agent`) or edit one, in a wide drawer. */
export function AgentDrawer({ teamId, agentId, preset, onClose }: { teamId?: string | null; agentId?: string; preset?: AgentPreset; onClose: () => void }) {
  const isNew = !agentId;
  const [initial, setInitial] = useState<AgentDraft | null>(isNew ? draftFrom(null, preset) : null);
  const [chatOwner, setChatOwner] = useState<Member | null>(null);
  const [models, setModels] = useState<ModelOption[] | null>(null);
  const [modelsErr, setModelsErr] = useState<string | null>(null);
  const [clis, setClis] = useState<CliStatus[] | null>(null);
  useEffect(() => {
    listClis().then(setClis).catch(() => setClis([{ id: CLAUDE_CODE, name: "Claude Code", kind: "claude_code", command: "", env: [], args: "" }]));
  }, []);
  const [d, setD] = useState<AgentDraft | null>(initial);
  const cliId = d?.cli ?? CLAUDE_CODE;
  const kind = kindOf(cliId, clis);
  // Claude Code's model list, asked from the CLI the agent runs on (another account can offer other models).
  const asked = useRef("");
  const loadModels = (refresh = false) => {
    setModelsErr(null);
    setModels(null);
    asked.current = cliId;
    const mine = (f: () => void) => { if (asked.current === cliId) f(); }; // a slower answer for another CLI is dropped
    claudeModels(refresh, cliId === CLAUDE_CODE ? null : cliId).then((m) => mine(() => setModels(m))).catch((e) => mine(() => setModelsErr(String(e))));
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { if (kind === "claude_code") loadModels(); }, [cliId, kind]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const edited = useRef(!isNew); // once the instructions are edited, a role change no longer replaces them
  const toolsEdited = useRef(!isNew); // the same for the allowed commands
  const added = useRef<string | null>(null);
  const lastTemplate = useRef("");
  const lastTools = useRef("");
  useEffect(() => {
    if (isNew) return;
    getAgent(agentId!).then((m) => { const dr = draftFrom(m); setInitial(dr); setD(dr); }).catch((e) => setErr(String(e)));
  }, [agentId, isNew]);
  useEffect(() => { chatAgent().then(setChatOwner).catch(() => {}); }, []);
  const role = d?.role;
  useEffect(() => {
    if (edited.current || role === undefined) return;
    roleTemplate(role || "agent").then((t) => { lastTemplate.current = t; setD((x) => (x ? { ...x, instructions: t } : x)); setInitial((x) => (x && isNew ? { ...x, instructions: t } : x)); }).catch(() => {});
  }, [role, isNew]);
  useEffect(() => {
    if (toolsEdited.current || role === undefined) return;
    roleTools(role || "agent").then((list) => { const t = list.join("\n"); lastTools.current = t; setD((x) => (x ? { ...x, tools: t } : x)); setInitial((x) => (x && isNew ? { ...x, tools: t } : x)); }).catch(() => {});
  }, [role, isNew]);
  if (!d || !initial) return null;
  const set = <K extends keyof AgentDraft>(k: K, v: AgentDraft[K]) => {
    if (k === "instructions" && v !== lastTemplate.current) edited.current = true;
    if (k === "tools" && v !== lastTools.current) toolsEdited.current = true;
    setD((x) => (x ? { ...x, [k]: v } : x));
  };
  const custom = !ROLES.includes(d.role);
  const cli = clis?.find((c) => c.id === d.cli);
  const cliName = cli?.name ?? KIND_LABEL[kind];
  /** Moving to another CLI keeps what still fits it: the permission mode and effort of its kind, the model of the same kind. */
  const pickCli = (id: string) => setD((x) => {
    if (!x) return x;
    const k = kindOf(id, clis);
    return { ...x, cli: id, permissionMode: modeFor(k, x.permissionMode), effort: EFFORTS_BY_KIND[k].includes(x.effort) ? x.effort : "",
      model: k === kind ? x.model : "", chat: k === "claude_code" ? x.chat : false };
  });
  const addTool = (t: string) => { const list = parseTools(d.tools); if (!list.includes(t)) set("tools", [...list, t].join("\n")); };
  const save = async () => {
    setBusy(true);
    try {
      // A new agent added already, when saving its tools failed: saving again changes it rather than adding another.
      let id = agentId ?? added.current;
      if (id) await updateAgent(id, inputFrom(d));
      // Opened from outside the Team page (Chat's setup panel) without a team: the first team.
      else { id = await addAgent(teamId || (await getTeam(null)).id, inputFrom(d)); added.current = id; }
      // Its MCP switches are saved on their own (only you change them); on another CLI they wait until it runs on Claude Code.
      if (kind === "claude_code" && !sameSwitches(d.mcp, initial.mcp)) await saveAgentMcp(id, { mcp: cleanSwitches(d.mcp) });
      onClose();
      if (isNew) go({ page: "agent", id });
    } catch (e) { setErr(String(e)); setBusy(false); }
  };
  return (
    <Drawer wide title={isNew ? "Add agent" : `${initial.name} settings`} subtitle="Each agent runs its coding CLI headless in its own git worktree, with the instructions and permissions below."
      onClose={onClose} dirty={JSON.stringify(d) !== JSON.stringify(initial)} error={err} hint="Ctrl+Enter saves"
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={busy || !d.name.trim()} onClick={save}>{isNew ? "Add agent" : "Save changes"}</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save(); }} onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); } }}>
        <FormSection title="Agent" text="Its name and the job it does. The role gives it its starting instructions, its branch in the organisation chart and its usual columns.">
          <Field label="Name" htmlFor="a-name"><input id="a-name" className="input" autoFocus={isNew} value={d.name} onChange={(e) => set("name", e.target.value)} placeholder="Frontend Agent" /></Field>
          <Field label="Role" htmlFor="a-role">
            <div className="input-group">
              <select id="a-role" className="select" value={custom ? "custom" : d.role} onChange={(e) => set("role", e.target.value === "custom" ? "" : e.target.value)}>
                {ROLES.map((r) => <option key={r} value={r}>{roleLabel(r)}</option>)}
                <option value="custom">Other…</option>
              </select>
              {custom && <input className="input" aria-label="Custom role" value={d.role} onChange={(e) => set("role", e.target.value)} placeholder="docs" />}
            </div></Field>
          <Field label="Runs on" htmlFor="a-cli" hint={cli?.problem ? null : "Settings → Coding CLIs adds Codex, Gemini, other CLIs and more accounts"}
            warn={clis && !cli ? "This CLI is no longer in Settings: pick another" : cli?.problem ?? null}>
            <select id="a-cli" className="select" value={d.cli} disabled={!clis} onChange={(e) => pickCli(e.target.value)}>
              {!clis && <option value={d.cli}>Loading…</option>}
              {clis?.map((c) => <option key={c.id} value={c.id}>{c.name}{c.id !== CLAUDE_CODE && c.name !== KIND_LABEL[c.kind] ? ` (${KIND_LABEL[c.kind]})` : ""}</option>)}
              {clis && !cli && <option value={d.cli}>{d.cli} (missing)</option>}
            </select></Field>
          {kind === "claude_code" ? (
            <ModelField value={d.model} models={models} error={modelsErr} onRefresh={() => loadModels(true)}
              onChange={(v) => { set("model", v); if (d.effort && models && !effortChoices(v, models).includes(d.effort)) set("effort", ""); }} />
          ) : (
            <Field label="Model" htmlFor="a-model" hint={kind === "other" ? `Empty: ${cliName}'s default. Used where its arguments say {model}.` : `Empty: ${cliName}'s default model`}>
              <input id="a-model" className="input" value={d.model} onChange={(e) => set("model", e.target.value)} placeholder={MODEL_EXAMPLE[kind]} /></Field>
          )}
          {kind === "codex" && (
            <Field label="Effort" htmlFor="a-effort" hint="How hard it reasons: higher is slower and uses more of your plan">
              <select id="a-effort" className="select" value={d.effort} onChange={(e) => set("effort", e.target.value)}>
                <option value="">Default (Codex decides)</option>
                {EFFORTS_BY_KIND.codex.map((l) => <option key={l} value={l}>{EFFORT_LABEL[l] ?? l}</option>)}
              </select></Field>
          )}
          {kind === "claude_code" && (() => {
            const levels = effortChoices(d.model, models);
            const bad = !!d.effort && !levels.includes(d.effort);
            return (
              <Field label="Effort" htmlFor="a-effort" hint={levels.length === 0 ? "This model takes no effort level" : "How hard it thinks: higher is slower and costs more"}
                warn={bad ? `This model doesn't take ${d.effort}; pick another level` : null}>
                <select id="a-effort" className="select" value={d.effort} disabled={levels.length === 0 && !d.effort} onChange={(e) => set("effort", e.target.value)}>
                  <option value="">Default (Claude Code decides)</option>
                  {levels.map((l) => <option key={l} value={l}>{EFFORT_LABEL[l] ?? l}</option>)}
                  {bad && <option value={d.effort}>{d.effort} (not offered)</option>}
                </select>
              </Field>
            );
          })()}
        </FormSection>
        <FormSection title="Chat" text="The agent you talk to on the Chat page: your Team Lead. Only one agent has Chat.">
          <Field label="Chat" wide htmlFor="a-chat"
            warn={kind !== "claude_code" ? "Chat runs on Claude Code: pick a Claude Code CLI under Runs on to turn it on." : d.chat && chatOwner && chatOwner.actorId !== agentId ? `Chat moves from ${chatOwner.name} to this agent.` : null}
            hint="In chat it uses Gizai's tools: clients, projects, tasks, agents, docs, files and your inbox. It can read your linked repositories but doesn't edit files or run commands there. The settings below are for its work on tasks.">
            <label className="check"><input id="a-chat" type="checkbox" checked={d.chat} disabled={kind !== "claude_code"} onChange={(e) => set("chat", e.target.checked)} /> Talk to this agent on the Chat page</label>
          </Field>
          <Field label="Board check" wide htmlFor="a-board"
            warn={d.chat && d.boardCheck && !(Number(d.boardMinutes) >= 5 && Number(d.boardMinutes) <= 1440) ? "Pick a number from 5 to 1440" : null}
            hint="It looks for held, answered, stuck and waiting cards. Only something new starts it: an answered question gets its agent going again, and what needs you comes as a chat at the top of your Inbox.">
            <span className="inline">
              <label className="check"><input id="a-board" type="checkbox" checked={d.chat && d.boardCheck} disabled={!d.chat} onChange={(e) => set("boardCheck", e.target.checked)} /> Check the board every</label>
              <input className="input" aria-label="Board check minutes" type="number" min={5} max={1440} style={{ width: 90 }} value={d.boardMinutes}
                disabled={!d.chat || !d.boardCheck} onChange={(e) => set("boardMinutes", e.target.value)} /> min
            </span>
          </Field>
        </FormSection>
        <FormSection title="Work" text={isNew
          ? "The columns it is on decide when it works (Team → Workflow). A new agent goes on its role's usual columns: builders on To do and In progress, QA on Testing, DevOps on Deploy, the Team Lead on none."
          : "The columns it is on decide when it works (Team → Workflow): on an Auto column it takes cards by itself, on a Manual one Run starts it."}>
          <Field label="Cards at once" htmlFor="a-runs" hint="1 to 10. Each card gets its own git worktree; Settings sets the limit for all agents together."
            warn={d.maxRuns.trim() !== "" && !(Number(d.maxRuns) >= 1 && Number(d.maxRuns) <= 10) ? "Pick a number from 1 to 10" : null}>
            <input id="a-runs" className="input" type="number" min={1} max={10} style={{ width: 90 }} value={d.maxRuns} onChange={(e) => set("maxRuns", e.target.value)} /></Field>
        </FormSection>
        <FormSection title="Permissions" text={`What ${cliName} may do without asking. A background agent can't answer questions.`}>
          {kind === "other" ? (
            <Field label="Permission mode" wide><span>{cliName} runs with its own settings: Gizai passes it no permissions. Put the options it needs in its arguments (Settings → Coding CLIs).</span></Field>
          ) : (
            <Field label="Permission mode" htmlFor="a-perm" wide warn={RISKY.has(d.permissionMode) ? PERMISSIONS[kind][d.permissionMode] : null} hint={RISKY.has(d.permissionMode) ? null : PERMISSIONS[kind][d.permissionMode]}>
              <select id="a-perm" className="select" value={d.permissionMode} onChange={(e) => set("permissionMode", e.target.value)}>
                {Object.keys(PERMISSIONS[kind]).map((m) => <option key={m} value={m}>{m}</option>)}
                {!(d.permissionMode in PERMISSIONS[kind]) && <option value={d.permissionMode}>{d.permissionMode || "(none)"} (not offered)</option>}
              </select></Field>
          )}
          {usesAllowedTools(kind) && (
            <Field label="Allowed commands" htmlFor="a-tools" wide hint={kind === "gemini" ? "One per line, like Bash(npm test:*). Gemini gets each Bash(…) line as a shell command it may run; everything else is refused in auto_edit mode." : "One per line. Everything else is refused in acceptEdits mode."}>
              <textarea id="a-tools" className="textarea mono" rows={6} value={d.tools} onChange={(e) => set("tools", e.target.value)} />
              <div className="tool-sugg">{TOOL_SUGGESTIONS.filter((t) => !parseTools(d.tools).includes(t)).map((t) => <button key={t} type="button" className="label-pill" onClick={() => addTool(t)}>+ {t}</button>)}</div></Field>
          )}
          <FoldersField value={d.folders} onChange={(v) => set("folders", v)} kind={kind} lead={d.chat} />
          <Field label="Monthly budget ($)" htmlFor="a-budget" hint={kind === "claude_code" ? "Once its runs this calendar month (UTC) cost this much, it starts no new runs" : `${cliName} doesn't report what a run costs, so its runs count as $0 here`}><input id="a-budget" className="input" inputMode="decimal" value={d.budget} onChange={(e) => set("budget", e.target.value)} placeholder="No limit" /></Field>
        </FormSection>
        <FormSection title="Tools" text={`What ${cliName} may use besides its worktree, folders and commands: outside services through MCP servers. Everything is off until you switch it on.`}>
          <AgentToolsField agentId={agentId} kind={kind} allowedTools={!usesAllowedTools(kind) ? [] : parseTools(d.tools).length > 0 ? parseTools(d.tools) : DEFAULT_TOOLS} value={d.mcp} onChange={(v) => set("mcp", v)} />
        </FormSection>
        <FormSection title="Instructions" text="Sent with every run, before the task. Must end by asking for the GIZAI_RESULT line.">
          <Field label="Instructions" wide><MarkdownEditor value={d.instructions} onChange={(md) => set("instructions", md)} ariaLabel="Instructions" minHeight={260} /></Field>
        </FormSection>
      </form>
    </Drawer>
  );
}
