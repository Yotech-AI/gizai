import { useEffect, useState } from "react";
import {
  DndContext, DragOverlay, KeyboardSensor, PointerSensor, closestCenter, pointerWithin, useSensor, useSensors,
  type CollisionDetection, type DragEndEvent, type DragOverEvent, type DragStartEvent,
} from "@dnd-kit/core";
import { arrayMove, sortableKeyboardCoordinates } from "@dnd-kit/sortable";
import { Plus } from "lucide-react";
import { addColumnAgent, addTeam, getTeam, listTeams, setColumn } from "../api";
import { go } from "../router";
import { useData } from "../lib/useData";
import { useCurrentTeam } from "../lib/team";
import { afterIdAt, boardOrder } from "../lib/columns";
import { useLiveRuns } from "../lib/useLiveRuns";
import { useDrawer } from "../lib/drawers";
import type { Member } from "../types";
import { Avatar } from "../components/Avatar";
import { Drawer } from "../components/Drawer";
import { Field, FormSection } from "../components/Form";
import { OrgChart, type AgentDrag } from "../components/OrgChart";
import { ColumnEditor, type ColumnDrag, type ColumnError } from "../components/ColumnEditor";
import { LabelsEditor } from "../components/LabelsEditor";
import { useChatLive } from "../components/chat/useChat";

function PersonCard({ m }: { m: Member }) {
  return (
    <div className="member">
      <div className="h"><Avatar name={m.name} kind={m.kind} size="lg" role={m.roleKey} />
        <div><b>{m.name}</b><span className="fn">{m.roleKey === "reviewer" ? "Reviewer · merges" : m.roleKey}</span></div></div>
    </div>
  );
}

function NewTeamDrawer({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const make = async () => { try { onCreated(await addTeam(name)); } catch (e) { setErr(String(e)); } };
  return (
    <Drawer title="New team" subtitle="A new team gets the seven usual columns and no agents. New projects still join your first team for now." onClose={onClose} dirty={!!name} error={err}
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!name.trim()} onClick={make}>Create team</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); make(); }}>
        <FormSection title="Team"><Field label="Name" htmlFor="tm-name"><input id="tm-name" className="input" autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Mobile team" /></Field></FormSection>
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
    </>
  );
}
