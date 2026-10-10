// The memory graph's settings panel (GA-69), over the graph's top right corner: Filters (the search query with path: and
// tag:, tags, orphans, existing notes only and the kinds of dot; the local graph's depth and directions), Groups (a
// colour per query, from the calm palette), Display (arrows, text fade, node size, link thickness and Animate) and
// Forces. Its sections fold; every change is kept at once (MemoryPage keeps them with the user's other Memory settings).
import { useId, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, Play, Plus, RotateCcw, X } from "lucide-react";
import {
  EXTRA_KINDS, EXTRA_NAME, KIND_LOOK, PALETTE, paletteColor, RANGES, type ExtraKind, type GraphGroup, type GraphSettings, type NodeKind,
  type PaletteKey, type RangeKey,
} from "../../lib/graph";

type Props = {
  settings: GraphSettings;
  onChange: (s: GraphSettings) => void;
  /** The groups as shown: the kept ones, or one per top folder until they are changed. */
  groups: GraphGroup[];
  /** The local graph: depth and the link directions too. */
  local?: boolean;
  onAnimate: () => void;
  onReset: () => void;
  onClose: () => void;
};

export function GraphPanel({ settings: s, onChange, groups, local, onAnimate, onReset, onClose }: Props) {
  const set = (patch: Partial<GraphSettings>) => onChange({ ...s, ...patch });
  const id = useId();
  const toggleSection = (key: string) => set({ open: s.open.includes(key) ? s.open.filter((x) => x !== key) : [...s.open, key] });
  const setGroups = (gs: GraphGroup[]) => set({ groups: gs });
  return (
    <div className="graph-panel" role="region" aria-label={local ? "Local graph settings" : "Graph settings"}>
      <div className="graph-panel-head">
        <b>{local ? "Local graph" : "Graph"} settings</b>
        <button className="btn ghost sm icon-only" aria-label="Restore the default settings" title="Restore the default settings" onClick={onReset}><RotateCcw className="icon" /></button>
        <button className="btn ghost sm icon-only" aria-label="Close the settings" title="Close" onClick={onClose}><X className="icon" /></button>
      </div>

      <Section title="Filters" open={s.open.includes("filters")} onToggle={() => toggleSection("filters")}>
        <input className="input" type="search" aria-label="Search the graph" placeholder="Search, path: or tag:" value={s.query}
          title={'Only the notes with every word and "phrase"; path:Folder keeps a folder\'s notes, tag:name those with a tag'}
          onChange={(e) => set({ query: e.target.value })} onKeyDown={(e) => { if (e.key === "Escape" && s.query) { e.stopPropagation(); set({ query: "" }); } }} />
        {local && <>
          <Slider id={`${id}-depth`} label="Depth" k="depth" value={s.depth} onChange={(depth) => set({ depth })} />
          <Check label="Incoming links" hint="Notes that link to it" on={s.incoming} onChange={(incoming) => set({ incoming })} />
          <Check label="Outgoing links" hint="What it links to" on={s.outgoing} onChange={(outgoing) => set({ outgoing })} />
        </>}
        <Check label="Tags" hint="Each tag as a dot, linked to its notes" on={s.tags} onChange={(tags) => set({ tags })} kind="tag" />
        <Check label="Orphans" hint="Notes without any link" on={s.orphans} onChange={(orphans) => set({ orphans })} />
        <Check label="Existing notes only" hint="Leave out links to notes that aren't there yet" on={s.existingOnly} onChange={(existingOnly) => set({ existingOnly })} />
        <div className="graph-sub">What notes name</div>
        {EXTRA_KINDS.map((k) => (
          <Check key={k} label={EXTRA_NAME[k]} on={!s.hidden.includes(k)} kind={k}
            onChange={(on) => set({ hidden: on ? s.hidden.filter((x) => x !== k) : [...s.hidden, k] as ExtraKind[] })} />
        ))}
      </Section>

      <Section title="Groups" open={s.open.includes("groups")} onToggle={() => toggleSection("groups")}>
        <p className="faint graph-hint">A note gets the colour of the first group whose query it matches.</p>
        {groups.map((g, i) => (
          <GroupRow key={i} n={i + 1} group={g} onChange={(ng) => setGroups(groups.map((x, j) => (j === i ? ng : x)))}
            onRemove={() => setGroups(groups.filter((_, j) => j !== i))} />
        ))}
        <button className="btn sm graph-add" onClick={() => setGroups([...groups, { query: "", color: PALETTE.find((p) => !groups.some((g) => g.color === p.key))?.key ?? PALETTE[0].key }])}>
          <Plus className="icon" />New group
        </button>
      </Section>

      <Section title="Display" open={s.open.includes("display")} onToggle={() => toggleSection("display")}>
        <Check label="Arrows" hint="Which way each link goes" on={s.arrows} onChange={(arrows) => set({ arrows })} />
        <Slider id={`${id}-fade`} label="Text fade threshold" k="textFade" value={s.textFade} onChange={(textFade) => set({ textFade })} />
        <Slider id={`${id}-size`} label="Node size" k="nodeSize" value={s.nodeSize} onChange={(nodeSize) => set({ nodeSize })} />
        <Slider id={`${id}-thick`} label="Link thickness" k="linkThickness" value={s.linkThickness} onChange={(linkThickness) => set({ linkThickness })} />
        <button className="btn sm graph-add" title="Replays the graph growing in the order the notes were made" onClick={onAnimate}><Play className="icon" />Animate</button>
      </Section>

      <Section title="Forces" open={s.open.includes("forces")} onToggle={() => toggleSection("forces")}>
        <Slider id={`${id}-centre`} label="Centre force" k="centre" value={s.centre} onChange={(centre) => set({ centre })} />
        <Slider id={`${id}-repel`} label="Repel force" k="repel" value={s.repel} onChange={(repel) => set({ repel })} />
        <Slider id={`${id}-link`} label="Link force" k="linkForce" value={s.linkForce} onChange={(linkForce) => set({ linkForce })} />
        <Slider id={`${id}-dist`} label="Link distance" k="linkDistance" value={s.linkDistance} onChange={(linkDistance) => set({ linkDistance })} />
      </Section>
    </div>
  );
}

function Section({ title, open, onToggle, children }: { title: string; open: boolean; onToggle: () => void; children: ReactNode }) {
  return (
    <section className="graph-section">
      <button className="graph-section-head" aria-expanded={open} onClick={onToggle}>
        {open ? <ChevronDown className="icon sm" /> : <ChevronRight className="icon sm" />}<span>{title}</span>
      </button>
      {open && <div className="graph-section-body">{children}</div>}
    </section>
  );
}

function Check({ label, hint, on, onChange, kind }: { label: string; hint?: string; on: boolean; onChange: (on: boolean) => void; kind?: NodeKind }) {
  return (
    <label className="check graph-check" title={hint}>
      <input type="checkbox" checked={on} onChange={(e) => onChange(e.target.checked)} />
      {kind && <KindIcon kind={kind} />}
      <span>{label}</span>
    </label>
  );
}

function Slider({ id, label, k, value, onChange }: { id: string; label: string; k: RangeKey; value: number; onChange: (v: number) => void }) {
  const r = RANGES[k];
  const shown = r.step >= 1 ? String(value) : value.toFixed(r.step < 0.1 ? 2 : 1);
  return (
    <div className="graph-slider">
      <label htmlFor={id}>{label}</label>
      <output htmlFor={id} className="faint">{shown}</output>
      <input id={id} type="range" min={r.min} max={r.max} step={r.step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
    </div>
  );
}

function GroupRow({ n, group, onChange, onRemove }: { n: number; group: GraphGroup; onChange: (g: GraphGroup) => void; onRemove: () => void }) {
  const [picking, setPicking] = useState(false);
  const name = PALETTE.find((p) => p.key === group.color)?.name ?? "";
  return (
    <div className="graph-group">
      <div className="graph-group-row">
        <button className="graph-swatch" style={{ background: paletteColor(group.color) }} aria-expanded={picking}
          aria-label={`Group ${n}'s colour: ${name}. Change it`} title={`${name}: change the colour`} onClick={() => setPicking(!picking)} />
        <input className="input" aria-label={`Group ${n}'s query`} placeholder="path:Folder, tag:name or words" value={group.query}
          onChange={(e) => onChange({ ...group, query: e.target.value })} />
        <button className="btn ghost sm icon-only" aria-label={`Remove group ${n}`} title="Remove this group" onClick={onRemove}><X className="icon" /></button>
      </div>
      {picking && (
        <div className="swatches graph-swatches" role="radiogroup" aria-label={`Group ${n}'s colour`}>
          {PALETTE.map((p) => (
            <button key={p.key} className="swatch" role="radio" aria-checked={p.key === group.color} aria-pressed={p.key === group.color} aria-label={p.name} title={p.name}
              style={{ background: p.dark }} onClick={() => { onChange({ ...group, color: p.key as PaletteKey }); setPicking(false); }} />
          ))}
        </div>
      )}
    </div>
  );
}

/** A kind's dot as a small icon: its shape in its colour (a legend next to its switch). */
export function KindIcon({ kind }: { kind: NodeKind }) {
  const look = kind === "note" || kind === "missing" ? { shape: "circle", color: null } : KIND_LOOK[kind];
  const fill = look.color ? paletteColor(look.color) : "var(--text-3)";
  const shapes: Record<string, ReactNode> = {
    circle: <circle cx="6" cy="6" r="4" fill={fill} opacity={kind === "missing" ? 0.45 : 1} />,
    ring: <circle cx="6" cy="6" r="3.4" fill="none" stroke={fill} strokeWidth="1.8" />,
    square: <rect x="2.4" y="2.4" width="7.2" height="7.2" fill={fill} />,
    diamond: <path d="M6 1.4 10.6 6 6 10.6 1.4 6Z" fill={fill} />,
    hexagon: <path d="M6 1.4 10 3.7v4.6L6 10.6 2 8.3V3.7Z" fill={fill} />,
    triangle: <path d="M6 1.6 10.6 10H1.4Z" fill={fill} />,
  };
  return <svg className="graph-kind" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">{shapes[look.shape]}</svg>;
}
