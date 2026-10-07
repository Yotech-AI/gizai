import { useEffect, useRef, useState } from "react";
import { RefreshCw } from "lucide-react";
import { addAgent, chatAgent, claudeModels, getAgent, getTeam, roleTemplate, updateAgent } from "../api";
import { go } from "../router";
import { draftFrom, inputFrom, parseTools, ROLES, roleLabel, type AgentDraft, type AgentPreset } from "../lib/agents";
import { effortChoices, findModel, modelHint } from "../lib/models";
import type { Member, ModelOption } from "../types";
export type { AgentPreset } from "../lib/agents";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";

const EFFORT_LABEL: Record<string, string> = { low: "Low", medium: "Medium", high: "High", xhigh: "Extra high", max: "Max" };

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

const TOOL_SUGGESTIONS = ["Bash(pnpm:*)", "Bash(yarn:*)", "Bash(go test:*)", "Bash(make test:*)", "Bash(python -m pytest:*)"];
export const PERMISSION_HELP: Record<string, string> = {
  acceptEdits: "Edits files freely; runs only the commands allowed below. Recommended.",
  dontAsk: "Never asks; anything not allowed below is refused.",
  auto: "Claude decides what is safe to run on its own.",
  plan: "Plans only; changes nothing.",
  manual: "Asks for every action; a background agent can't answer, so most actions are refused.",
  bypassPermissions: "Runs anything without asking. Only for a sandboxed machine.",
};

/** Add an agent to a team (no `agent`) or edit one, in a wide drawer. */
export function AgentDrawer({ teamId, agentId, preset, onClose }: { teamId?: string | null; agentId?: string; preset?: AgentPreset; onClose: () => void }) {
  const isNew = !agentId;
  const [initial, setInitial] = useState<AgentDraft | null>(isNew ? draftFrom(null, preset) : null);
  const [chatOwner, setChatOwner] = useState<Member | null>(null);
  const [models, setModels] = useState<ModelOption[] | null>(null);
  const [modelsErr, setModelsErr] = useState<string | null>(null);
  const loadModels = (refresh = false) => {
    setModelsErr(null);
    if (refresh) setModels(null);
    claudeModels(refresh).then(setModels).catch((e) => setModelsErr(String(e)));
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => loadModels(), []);
  const [d, setD] = useState<AgentDraft | null>(initial);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const edited = useRef(!isNew); // once the instructions are edited, a role change no longer replaces them
  const lastTemplate = useRef("");
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
  if (!d || !initial) return null;
  const set = <K extends keyof AgentDraft>(k: K, v: AgentDraft[K]) => {
    if (k === "instructions" && v !== lastTemplate.current) edited.current = true;
    setD((x) => (x ? { ...x, [k]: v } : x));
  };
  const custom = !ROLES.includes(d.role);
  const addTool = (t: string) => { const list = parseTools(d.tools); if (!list.includes(t)) set("tools", [...list, t].join("\n")); };
  const save = async () => {
    setBusy(true);
    try {
      // Opened from outside the Team page (Chat's setup panel) without a team: the first team.
      if (isNew) { const id = await addAgent(teamId || (await getTeam(null)).id, inputFrom(d)); onClose(); go({ page: "agent", id }); }
      else { await updateAgent(agentId!, inputFrom(d)); onClose(); }
    } catch (e) { setErr(String(e)); setBusy(false); }
  };
  return (
    <Drawer wide title={isNew ? "Add agent" : `${initial.name} settings`} subtitle="Each agent runs Claude Code headless in its own git worktree, with the instructions and permissions below."
      onClose={onClose} dirty={JSON.stringify(d) !== JSON.stringify(initial)} error={err} hint="Ctrl+Enter saves"
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={busy || !d.name.trim()} onClick={save}>{isNew ? "Add agent" : "Save changes"}</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save(); }} onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); } }}>
        <FormSection title="Agent" text="Its name and the job it does. The role decides which cards it takes.">
          <Field label="Name" htmlFor="a-name"><input id="a-name" className="input" autoFocus={isNew} value={d.name} onChange={(e) => set("name", e.target.value)} placeholder="Frontend Agent" /></Field>
          <Field label="Role" htmlFor="a-role">
            <div className="input-group">
              <select id="a-role" className="select" value={custom ? "custom" : d.role} onChange={(e) => set("role", e.target.value === "custom" ? "" : e.target.value)}>
                {ROLES.map((r) => <option key={r} value={r}>{roleLabel(r)}</option>)}
                <option value="custom">Other…</option>
              </select>
              {custom && <input className="input" aria-label="Custom role" value={d.role} onChange={(e) => set("role", e.target.value)} placeholder="docs" />}
            </div></Field>
          <Field label="Runs on" htmlFor="a-cli" hint="Codex and Gemini come later"><select id="a-cli" className="select" value="claude_code" onChange={() => {}}><option value="claude_code">Claude Code</option></select></Field>
          <ModelField value={d.model} models={models} error={modelsErr} onRefresh={() => loadModels(true)}
            onChange={(v) => { set("model", v); if (d.effort && models && !effortChoices(v, models).includes(d.effort)) set("effort", ""); }} />
          {(() => {
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
            warn={d.chat && chatOwner && chatOwner.actorId !== agentId ? `Chat moves from ${chatOwner.name} to this agent.` : null}
            hint="In chat it uses Gizai's tools: clients, projects, tasks, agents, docs, files and your inbox. It can read your linked repositories but doesn't edit files or run commands there. The settings below are for its work on tasks.">
            <label className="check"><input id="a-chat" type="checkbox" checked={d.chat} onChange={(e) => set("chat", e.target.checked)} /> Talk to this agent on the Chat page</label>
          </Field>
        </FormSection>
        <FormSection title="Wakes up" text="When the agent starts work without you pressing Run, and on how many cards at once.">
          <Field label="Wake-up" wide>
            <div className="radios" role="radiogroup" aria-label="Wakes up">
              {([["manual", "Only when I press Run"], ["on_assign", "When a card is routed or assigned to it"], ["heartbeat", "On a heartbeat"]] as const).map(([k, l]) => (
                <label key={k}><input type="radio" name="wake" checked={d.wakeup === k} onChange={() => set("wakeup", k)} /> {l}</label>
              ))}
              {d.wakeup === "heartbeat" && (
                <span className="inline">every <input className="input" aria-label="Minutes" type="number" min={1} max={1440} style={{ width: 90 }} value={d.minutes} onChange={(e) => set("minutes", e.target.value)} /> minutes it looks for its next card</span>
              )}
            </div></Field>
          <Field label="Cards at once" htmlFor="a-runs" hint="1 to 10. Each card gets its own git worktree; Settings sets the limit for all agents together."
            warn={d.maxRuns.trim() !== "" && !(Number(d.maxRuns) >= 1 && Number(d.maxRuns) <= 10) ? "Pick a number from 1 to 10" : null}>
            <input id="a-runs" className="input" type="number" min={1} max={10} style={{ width: 90 }} value={d.maxRuns} onChange={(e) => set("maxRuns", e.target.value)} /></Field>
        </FormSection>
        <FormSection title="Permissions" text="What Claude Code may do without asking. A background agent can't answer questions.">
          <Field label="Permission mode" htmlFor="a-perm" wide warn={d.permissionMode === "bypassPermissions" ? PERMISSION_HELP.bypassPermissions : null} hint={PERMISSION_HELP[d.permissionMode]}>
            <select id="a-perm" className="select" value={d.permissionMode} onChange={(e) => set("permissionMode", e.target.value)}>
              {Object.keys(PERMISSION_HELP).map((m) => <option key={m} value={m}>{m}</option>)}</select></Field>
          <Field label="Allowed commands" htmlFor="a-tools" wide hint="One per line. Everything else is refused in acceptEdits mode.">
            <textarea id="a-tools" className="textarea mono" rows={6} value={d.tools} onChange={(e) => set("tools", e.target.value)} />
            <div className="tool-sugg">{TOOL_SUGGESTIONS.filter((t) => !parseTools(d.tools).includes(t)).map((t) => <button key={t} type="button" className="label-pill" onClick={() => addTool(t)}>+ {t}</button>)}</div></Field>
          <Field label="Monthly budget ($)" htmlFor="a-budget" hint="Once its runs this calendar month (UTC) cost this much, it starts no new runs"><input id="a-budget" className="input" inputMode="decimal" value={d.budget} onChange={(e) => set("budget", e.target.value)} placeholder="No limit" /></Field>
        </FormSection>
        <FormSection title="Instructions" text="Sent with every run, before the task. Must end by asking for the GIZAI_RESULT line.">
          <Field label="Instructions" wide><MarkdownEditor value={d.instructions} onChange={(md) => set("instructions", md)} ariaLabel="Instructions" minHeight={260} /></Field>
        </FormSection>
      </form>
    </Drawer>
  );
}
