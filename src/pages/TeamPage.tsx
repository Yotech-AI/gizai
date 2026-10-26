import { useEffect, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import {
  DndContext, DragOverlay, KeyboardSensor, PointerSensor, closestCenter, pointerWithin, useSensor, useSensors,
  type CollisionDetection, type DragEndEvent, type DragOverEvent, type DragStartEvent,
} from "@dnd-kit/core";
import { arrayMove, sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { Plus } from "lucide-react";
import {
  addColumnAgent, addTeam, checkTeamExport, getTeam, importTeamTemplate, listTeams, previewTeamTemplate, saveTeamTemplate, setColumn, teamTemplates,
} from "../api";
import { go } from "../router";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { afterIdAt, boardOrder } from "../lib/columns";
import { useLiveRuns } from "../lib/useLiveRuns";
import { useDrawer } from "../lib/drawers";
import type { Member, TemplatePlan, TemplateSource, TemplateWarning } from "../types";
import { Avatar } from "../components/Avatar";
import { Drawer } from "../components/Drawer";
import { Field, FormSection } from "../components/Form";
import { OrgChart, type AgentDrag } from "../components/OrgChart";
import { ColumnEditor, type ColumnDrag, type ColumnError } from "../components/ColumnEditor";
import { LabelsEditor } from "../components/LabelsEditor";
import { ExportWarnings, TemplateCheck, baseName, templateFileName } from "../components/TeamTemplate";
import { useChatLive } from "../components/chat/useChat";

function PersonCard({ m }: { m: Member }) {
  return (
    <div className="member">
      <div className="h"><Avatar name={m.name} kind={m.kind} size="lg" role={m.roleKey} />
        <div><b>{m.name}</b><span className="fn">{m.roleKey === "reviewer" ? "Reviewer · merges" : m.roleKey}</span></div></div>
    </div>
  );
}

/** Where a new team starts from: empty (the seven usual columns, no agents), a built-in template or a template file. */
type Start = { kind: "empty" } | { kind: "builtin"; key: string } | { kind: "file"; path: string };
const sourceOf = (s: Start): TemplateSource | null => (s.kind === "builtin" ? { builtin: s.key } : s.kind === "file" ? { path: s.path } : null);

function NewTeamDrawer({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const { data: builtins } = useData(() => teamTemplates());
  const [start, setStart] = useState<Start>({ kind: "empty" });
  // The check of the picked template; a refusal is in err and leaves no plan, so Create team stays off.
  const [plan, setPlan] = useState<TemplatePlan | null>(null);
  const [checking, setChecking] = useState(false);
  const [making, setMaking] = useState(false);
  const asked = useRef(0); // only the last pick's check counts
  // The name the last check filled in: a new check replaces it, but not a name the person typed.
  const filled = useRef("");
  const typed = useRef(name);
  typed.current = name;
  const pick = async (next: Start) => {
    const n = ++asked.current;
    setStart(next); setErr(null); setPlan(null);
    const source = sourceOf(next);
    if (!source) {
      setChecking(false);
      if (typed.current === filled.current) { setName(""); filled.current = ""; }
      return;
    }
    setChecking(true);
    try {
      const p = await previewTeamTemplate(source);
      if (n !== asked.current) return;
      setPlan(p);
      if (!typed.current.trim() || typed.current === filled.current) { setName(p.teamName); filled.current = p.teamName; }
    } catch (e) { if (n === asked.current) setErr(String(e)); }
    finally { if (n === asked.current) setChecking(false); }
  };
  const pickFile = async () => {
    const path = await openDialog({ multiple: false, directory: false, title: "Start from a template file",
      filters: [{ name: "Team template", extensions: ["json"] }] }).catch((e) => { setErr(String(e)); return null; });
    if (typeof path === "string") await pick({ kind: "file", path });
  };
  const source = sourceOf(start);
  const ready = !!name.trim() && !making && (!source || (!!plan && !checking));
  const make = async () => {
    if (!ready) return;
    setMaking(true); setErr(null);
    try { onCreated(source ? await importTeamTemplate(source, name.trim()) : await addTeam(name)); }
    catch (e) { setErr(String(e)); setMaking(false); }
  };
  const subtitle = start.kind === "empty" ? "A new team gets the seven usual columns and no agents. Move a project to it in the project's form."
    : "A new team gets the template's columns and agents. Check them below, then move a project to it in the project's form.";
  return (
    <Drawer title="New team" subtitle={subtitle} onClose={onClose} dirty={!!name || start.kind !== "empty"} error={err}
      hint={checking ? "Checking the template…" : undefined}
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!ready} onClick={make}>{making ? "Creating…" : "Create team"}</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); make(); }}>
        <FormSection title="Team"><Field label="Name" htmlFor="tm-name"><input id="tm-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Mobile team" /></Field></FormSection>
        <FormSection title="Start from" text="A built-in team or a template file brings its columns and agents. You see them here before the team is made.">
          <div className="tt-starts tt-wide" role="radiogroup" aria-label="Start from">
            <label className={`tt-start${start.kind === "empty" ? " on" : ""}`}>
              <input type="radio" name="tm-start" checked={start.kind === "empty"} onChange={() => pick({ kind: "empty" })} />
              <span className="tt-name">Empty</span>
              <span className="tt-note">The seven usual columns, no agents.</span>
            </label>
            {(builtins ?? []).map((b) => (
              <label key={b.key} className={`tt-start${start.kind === "builtin" && start.key === b.key ? " on" : ""}`}>
                <input type="radio" name="tm-start" checked={start.kind === "builtin" && start.key === b.key} onChange={() => pick({ kind: "builtin", key: b.key })} />
                <span className="tt-name">{b.name}</span>
                <span className="tt-note">{b.description}</span>
              </label>
            ))}
            <label className={`tt-start${start.kind === "file" ? " on" : ""}`}>
              <input type="radio" name="tm-start" checked={start.kind === "file"} onChange={pickFile} />
              <span className="tt-name">A template file…</span>
              <span className="tt-note">{start.kind === "file" ? <span className="mono" title={start.path}>{baseName(start.path)}</span>
                : "A .json file: a team someone exported and shared."}</span>
            </label>
          </div>
          {start.kind === "file" && <div className="tt-wide"><button type="button" className="btn ghost sm" onClick={pickFile}>Choose another file…</button></div>}
        </FormSection>
        {plan && source && <TemplateCheck plan={plan} fromFile={start.kind === "file"} />}
        <button type="submit" hidden />
      </form>
    </Drawer>
  );
}

/** An agent card dragged from the organisation chart lands only on columns that take agents; a column dragged by its grip
 * goes between the other columns. */
const collision: CollisionDetection = (args) => {
  const type = (args.active.data.current as AgentDrag | ColumnDrag | undefined)?.type;
  const columns = args.droppableContainers.filter((c) => {
    const d = c.data.current as ColumnDrag | undefined;
    return d?.type === "column" && (type !== "agent" || d.takesAgents);
  });
  return type === "agent" ? pointerWithin({ ...args, droppableContainers: columns }) : closestCenter({ ...args, droppableContainers: columns });
};

export function TeamPage() {
  const [teamId, setTeamId] = useCurrentTeam();
  const { data: teams } = useData(() => listTeams());
  const { data: team, error } = useData(() => getTeam(teamId), [teamId]);
  const open = useDrawer();
  const live = useLiveRuns();
  const chatLive = useChatLive();
  const [newTeam, setNewTeam] = useState(false);
  const [addingColumn, setAddingColumn] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [colErr, setColErr] = useState<ColumnError>(null);
  // The columns in board order. A column dropped in a new place keeps it while it saves, until the saved order is back.
  const stateKey = team ? boardOrder(team.states).map((s) => s.id).join(",") : "";
  const [moved, setMoved] = useState<string[] | null>(null);
  useEffect(() => setMoved(null), [stateKey]);
  const order = moved ?? (stateKey ? stateKey.split(",") : []);
  const [dragging, setDragging] = useState<AgentDrag | ColumnDrag | null>(null);
  const [overId, setOverId] = useState<string | null>(null);
  // Export template: the check, its warnings when there are some, then the save dialog and a toast.
  const exporting = useRef(false); // from the click until the file is saved or the person stops
  const [warnings, setWarnings] = useState<TemplateWarning[] | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  useEffect(() => { if (!toast) return; const t = setTimeout(() => setToast(null), 5000); return () => clearTimeout(t); }, [toast]);
  const exportBtn = useRef<HTMLButtonElement>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  if (error) return <div className="error-banner">{error}</div>;
  if (!team) return null;
  const agents = team.members.filter((m) => m.kind === "agent");
  const people = team.members.filter((m) => m.kind !== "agent");

  const onDragStart = ({ active }: DragStartEvent) => { setDragging((active.data.current as AgentDrag | ColumnDrag) ?? null); setColErr(null); };
  const onDragOver = ({ active, over }: DragOverEvent) => {
    setOverId((active.data.current as AgentDrag | undefined)?.type === "agent" && over ? String(over.id) : null);
  };
  const onDragEnd = ({ active, over }: DragEndEvent) => {
    const drag = active.data.current as AgentDrag | ColumnDrag | undefined;
    setDragging(null);
    setOverId(null);
    if (!drag || !over) return;
    const to = String(over.id);
    if (drag.type === "agent") {
      addColumnAgent(to, drag.agentId).catch((e) => setColErr({ id: to, text: String(e) }));
      return;
    }
    const id = String(active.id);
    const from = order.indexOf(id), at = order.indexOf(to);
    if (from < 0 || at < 0 || from === at) return;
    const next = arrayMove(order, from, at);
    setMoved(next);
    setColumn(id, { afterId: afterIdAt(next, at) }).catch((e) => { setMoved(null); setColErr({ id, text: String(e) }); });
  };

  const saveTemplate = async () => {
    setWarnings(null);
    exporting.current = true;
    try {
      const path = await saveDialog({ title: "Export template", defaultPath: templateFileName(team.name), filters: [{ name: "Team template", extensions: ["json"] }] });
      if (!path) return;
      await saveTeamTemplate(team.id, path);
      setToast(`Saved ${baseName(path)}`);
    } catch (e) { setErr(String(e)); }
    finally { exporting.current = false; exportBtn.current?.focus(); }
  };
  const exportTemplate = async () => {
    if (exporting.current) return;
    exporting.current = true;
    setErr(null);
    try {
      const found = await checkTeamExport(team.id);
      if (found.length) { exporting.current = false; setWarnings(found); return; }
    } catch (e) { exporting.current = false; setErr(String(e)); return; }
    await saveTemplate();
  };
  // Cancel saves nothing.
  const cancelExport = () => { setWarnings(null); exportBtn.current?.focus(); };

  return (
    <>
      <div className="topbar">
        <div className="crumbs"><span>Team</span><span className="sep">/</span>
          <select className="chip" aria-label="Team" value={team.id} onChange={(e) => { setTeamId(e.target.value); go({ page: "team" }); }}>
            {(teams ?? []).map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
          </select>
          <span className="faint">{people.length} {people.length === 1 ? "person" : "people"}, {agents.length} {agents.length === 1 ? "agent" : "agents"}</span>
        </div>
        <div className="actions">
          <button ref={exportBtn} className="btn ghost" onClick={exportTemplate}
            title="Save this team's columns, agents and labels as a file to share">Export template</button>
          <button className="btn ghost" onClick={() => setNewTeam(true)}>New team</button>
          <button className="btn primary" onClick={() => open({ kind: "agent", teamId: team.id })}><Plus className="icon" />Add agent</button>
        </div>
      </div>
      {err && <div className="error-banner" role="alert">{err}<button className="btn ghost sm" onClick={() => setErr(null)}>Dismiss</button></div>}
      <div className="content">
        <div className="page">
          <DndContext sensors={sensors} collisionDetection={collision} onDragStart={onDragStart} onDragOver={onDragOver} onDragEnd={onDragEnd}
            onDragCancel={() => { setDragging(null); setOverId(null); }}>
            <section>
              <div className="section-head"><h3>Organisation</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>
                {agents.length === 0 ? "Start with the Team Lead: the agent you chat with. Click an empty spot to add an agent there."
                  : "Click an agent to open it, an empty spot to add one; drag an agent onto a column below to put it there"}</span></div>
              <div className="panel org-panel">
                <OrgChart members={team.members} teamId={team.id} branches={team.branches} onError={setErr}
                  working={(id) => live.some((r) => r.agentId === id) || (chatLive.length > 0 && !!agents.find((a) => a.actorId === id)?.chatEnabled)} />
              </div>
            </section>
            <section>
              <div className="section-head"><h3>Workflow</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>
                The columns in board order: who takes their cards, and where cards go next. Drag a column by its grip to move it.</span>
                {!addingColumn && <button className="btn ghost sm" style={{ marginLeft: "auto" }} onClick={() => setAddingColumn(true)}><Plus className="icon" />Add column</button>}</div>
              <ColumnEditor team={team} order={order} error={colErr} onError={setColErr} overId={overId}
                adding={addingColumn} onAddingDone={() => setAddingColumn(false)} />
            </section>
            <DragOverlay dropAnimation={null}>
              {dragging?.type === "agent" ? <span className="agent-chip drag-chip"><Avatar name={dragging.name} kind="agent" size="sm" /><span className="nm">{dragging.name}</span></span> : null}
            </DragOverlay>
          </DndContext>
          <section>
            <div className="section-head"><h3>Labels</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>
              Tags for people, like Must have and Could have: they don't start or assign anything</span></div>
            <LabelsEditor />
          </section>
          <section>
            <div className="section-head"><h3>People</h3></div>
            <div className="members">
              {people.map((m) => <PersonCard key={m.actorId} m={m} />)}
            </div>
          </section>
        </div>
      </div>
      {newTeam && <NewTeamDrawer onClose={() => setNewTeam(false)} onCreated={(id) => { setNewTeam(false); setTeamId(id); }} />}
      {warnings && <ExportWarnings warnings={warnings} onExport={saveTemplate} onCancel={cancelExport} />}
      {toast && <div className="toast" role="status">{toast}</div>}
    </>
  );
}
