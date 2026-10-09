import { useEffect, useState } from "react";
import { createTask, getTeam, listProjects, listUsers } from "../api";
import { go } from "../router";
import type { Person, Project, Team } from "../types";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";
import { NewLabel } from "./NewLabel";
import { PRIORITY_NAMES } from "./StatusIcon";

function readPref(k: string): string | null { try { return localStorage.getItem(k); } catch { return null; } }

/** N anywhere. Title, project, column (default Backlog), priority, assignee, labels, description and acceptance criteria. */
export function NewTaskDrawer({ onClose, stateId: presetState, projectId: presetProject, assigneeId: presetAssignee }: {
  onClose: () => void; stateId?: string | null; projectId?: string | null; assigneeId?: string | null;
}) {
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [team, setTeam] = useState<Team | null>(null);
  const [people, setPeople] = useState<Person[]>([]);
  const [projectId, setProjectId] = useState<string>(presetProject ?? "");
  const [title, setTitle] = useState("");
  const [labelIds, setLabelIds] = useState<string[]>([]);
  const [stateId, setStateId] = useState<string>(presetState ?? "");
  const [priority, setPriority] = useState(0);
  const [assigneeId, setAssigneeId] = useState<string>(presetAssignee ?? "");
  const [testing, setTesting] = useState(true);
  const [description, setDescription] = useState("");
  const [acceptance, setAcceptance] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    listProjects().then((ps) => {
      const open = ps.filter((p) => p.status !== "archived" && p.status !== "done");
      setProjects(open);
      const remembered = readPref("gizai-task-project");
      setProjectId((cur) => cur || (open.find((p) => p.id === remembered)?.id ?? open[0]?.id ?? ""));
    }).catch((e) => setErr(String(e)));
    getTeam().then((t) => { setTeam(t); setStateId((s) => s || [...t.states].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1))[0]?.id || ""); }).catch((e) => setErr(String(e)));
    listUsers().then(setPeople).catch(() => {});
  }, []);

  const create = async () => {
    if (!title.trim()) { setErr("Give the task a title."); return; }
    if (!projectId) { setErr("Pick a project."); return; }
    setBusy(true);
    try {
      const id = await createTask({ projectId, title: title.trim(), stateId: stateId || null, labelIds, priority, assigneeId: assigneeId || null,
        descriptionMd: description, acceptanceMd: acceptance.trim() ? acceptance : null, testing });
      onClose();
      go({ page: "task", id });
    } catch (e) { setErr(String(e)); setBusy(false); }
  };
  const agents = (team?.members ?? []).filter((m) => m.kind === "agent");
  const states = [...(team?.states ?? [])].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1));
  const noProjects = projects !== null && projects.length === 0;
  const dirty = !!(title.trim() || description.trim() || acceptance.trim());

  return (
    <Drawer title="New task" subtitle="Its column decides which agents pick it up, and when." onClose={onClose} dirty={dirty} error={err}
      hint={noProjects ? undefined : "Ctrl+Enter creates"}
      actions={noProjects ? <button className="btn ghost" onClick={onClose}>Close</button>
        : <><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={busy} onClick={create}>Create task</button></>}>
      {noProjects ? (
        <div className="empty"><span><b>No project yet.</b> Tasks live in a project. <a href="#/projects" onClick={onClose}>Create a project</a> first.</span></div>
      ) : (
        <form className="form" onSubmit={(e) => { e.preventDefault(); create(); }} onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); create(); } }}>
          <FormSection title="Task" text="What needs to happen, where it starts and who picks it up.">
            <Field label="Title" htmlFor="t-title" wide><input id="t-title" className="input" autoFocus value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Export invoices as CSV from the portal" /></Field>
            <Field label="Project" htmlFor="t-project"><select id="t-project" className="select" value={projectId} onChange={(e) => setProjectId(e.target.value)}>
              {(projects ?? []).map((p) => <option key={p.id} value={p.id}>{p.name} ({p.key})</option>)}</select></Field>
            <Field label="Column" htmlFor="t-column"><select id="t-column" className="select" value={stateId} onChange={(e) => setStateId(e.target.value)}>
              {states.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}</select></Field>
            <Field label="Priority" htmlFor="t-priority"><select id="t-priority" className="select" value={priority} onChange={(e) => setPriority(Number(e.target.value))}>
              {[0, 1, 2, 3, 4].map((p) => <option key={p} value={p}>{PRIORITY_NAMES[p]}</option>)}</select></Field>
            <Field label="Assignee" htmlFor="t-assignee" hint={assigneeId ? undefined : "Leave empty: the agents on its column take it"}>
              <select id="t-assignee" className="select" value={assigneeId} onChange={(e) => setAssigneeId(e.target.value)}>
                <option value="">Unassigned</option>
                <optgroup label="People">{people.map((p) => <option key={p.id} value={p.id}>{p.name}</option>)}</optgroup>
                {agents.length > 0 && <optgroup label="Agents">{agents.map((a) => <option key={a.actorId} value={a.actorId}>{a.name}</option>)}</optgroup>}
              </select></Field>
            <Field label="Labels" wide hint="Tags for people, like Must have: they don't start or assign anything">
              <div className="chips">{(team?.labels ?? []).map((l) => {
                const on = labelIds.includes(l.id);
                return <button key={l.id} type="button" className="label-pill" aria-pressed={on} onClick={() => setLabelIds((ids) => on ? ids.filter((x) => x !== l.id) : [...ids, l.id])}>
                  <span className="dot" style={{ background: l.color ?? "var(--text-3)" }} />{l.name}</button>;
              })}
              <NewLabel className="label-pill" labels={team?.labels ?? []} onCreated={(l) => {
                setTeam((t) => (t ? { ...t, labels: [...t.labels, l] } : t));
                setLabelIds((ids) => [...ids, l.id]);
              }} /></div></Field>
            <Field label="Testing" htmlFor="t-testing" wide
              hint="On: the QA Agent tests the card before Review. Turn it off for a small UI fix or a bug fix that doesn't need a full test sweep.">
              <label className="check"><input id="t-testing" type="checkbox" checked={testing} onChange={(e) => setTesting(e.target.checked)} />Test before Review</label>
            </Field>
          </FormSection>
          <FormSection title="Description" text="Context for whoever picks it up: people and agents read the same text.">
            <Field label="Description" wide><MarkdownEditor value={description} onChange={setDescription} ariaLabel="Description" minHeight={180}
              placeholder="What and why. Mention a task like KADE-12 or a person like @sanne." /></Field>
          </FormSection>
          <FormSection title="Acceptance criteria" text="What must be true when it's done. The QA agent checks these one by one.">
            <Field label="Acceptance criteria" wide><MarkdownEditor value={acceptance} onChange={setAcceptance} ariaLabel="Acceptance criteria" minHeight={100}
              placeholder="- [ ] Opens in Excel with semicolons" /></Field>
          </FormSection>
        </form>
      )}
    </Drawer>
  );
}
