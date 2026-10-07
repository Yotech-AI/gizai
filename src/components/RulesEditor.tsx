import { useState } from "react";
import { Plus, X } from "lucide-react";
import { addRule, deleteRule } from "../api";
import { ruleSentence } from "../lib/agents";
import type { Team } from "../types";

const USUAL: { kind: "label" | "column"; matchName: string; targetRole: string; priority: number }[] = [
  { kind: "label", matchName: "frontend", targetRole: "frontend", priority: 10 },
  { kind: "label", matchName: "backend", targetRole: "backend", priority: 10 },
  { kind: "column", matchName: "Testing", targetRole: "qa", priority: 20 },
];

/** Routing rules: which role picks a card up, by label or by the column it enters. */
export function RulesEditor({ team, onError }: { team: Team; onError: (m: string) => void }) {
  const roles = [...new Set(team.members.filter((m) => m.kind === "agent").map((m) => m.roleKey))];
  const [kind, setKind] = useState<"label" | "column">("label");
  const [match, setMatch] = useState("");
  const [role, setRole] = useState("");
  const [priority, setPriority] = useState("10");
  const names = kind === "label" ? team.labels.map((l) => l.name) : team.states.map((s) => s.name);
  const add = async () => {
    try { await addRule(team.id, { kind, matchName: match || names[0], targetRole: role || roles[0] || "", priority: Number(priority) || 0 }); setMatch(""); setRole(""); }
    catch (e) { onError(String(e)); }
  };
  const addUsual = async () => {
    for (const r of USUAL) {
      if (r.kind === "label" && !team.labels.some((l) => l.name === r.matchName)) continue;
      if (r.kind === "column" && !team.states.some((s) => s.name === r.matchName)) continue;
      try { await addRule(team.id, r); } catch (e) { onError(String(e)); return; }
    }
  };
  return (
    <section>
      <div className="section-head"><h3>Routing rules</h3><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>Lowest priority number wins; a column rule beats a label rule</span></div>
      {team.rules.length === 0 && (
        <div className="empty" style={{ marginBottom: 12 }}>
          <b>No rules yet.</b><span>No card goes to an agent by itself. Add your own below, or start with the usual three: frontend label → frontend, backend label → backend, Testing column → qa.</span>
          <button className="btn" onClick={addUsual}>Add the usual rules</button>
        </div>
      )}
      <div className="panel">
        {team.rules.map((r) => (
          <div className="rule" key={r.id}>
            <span className="dot" style={{ width: 8, height: 8, borderRadius: "50%", background: r.enabled ? "var(--live)" : "var(--text-3)" }} />
            <span>{ruleSentence(r, team)}</span>
            <span className="faint">priority {r.priority}</span>
            <button className="btn ghost sm icon-only" aria-label="Delete rule" title="Delete rule" onClick={() => deleteRule(r.id).catch((e) => onError(String(e)))}><X className="icon" /></button>
          </div>
        ))}
        <div className="rule-add">
          <span>When a card</span>
          <select className="select" aria-label="Rule kind" value={kind} onChange={(e) => { setKind(e.target.value as "label" | "column"); setMatch(""); }}>
            <option value="label">has label</option><option value="column">enters column</option>
          </select>
          <select className="select" aria-label="Label or column" value={match || names[0] || ""} onChange={(e) => setMatch(e.target.value)}>
            {names.map((n) => <option key={n} value={n}>{n}</option>)}
          </select>
          <span>→ role</span>
          <input className="input" aria-label="Role" list="gizai-roles" style={{ width: 130 }} value={role} placeholder={roles[0] ?? "frontend"} onChange={(e) => setRole(e.target.value)} />
          <datalist id="gizai-roles">{[...new Set([...roles, "frontend", "backend", "qa"])].map((r) => <option key={r} value={r} />)}</datalist>
          <span>priority</span>
          <input className="input" aria-label="Priority" type="number" style={{ width: 80 }} value={priority} onChange={(e) => setPriority(e.target.value)} />
          <button className="btn" onClick={add} disabled={!(role || roles[0])}><Plus className="icon" />Add rule</button>
        </div>
      </div>
    </section>
  );
}
