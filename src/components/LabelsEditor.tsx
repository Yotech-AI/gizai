// Team → Labels: every label with its colour and number of cards. Labels are tags for people, such as Must have and
// Could have: they start, route and assign nothing. Create (a name and a colour from the palette), rename, recolour,
// and remove after a confirm that names the cards that carry it. The backend refuses a name in use, ignoring case.
import { useEffect, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { listLabels, removeLabel, saveLabel } from "../api";
import { useData } from "../lib/useData";
import { cardCount, LABEL_COLORS, nextLabelColor } from "../lib/columns";
import type { LabelInfo } from "../types";
import { Popover } from "./Popover";

/** The palette as a row of round swatches. */
function Swatches({ value, onPick, name }: { value?: string | null; onPick: (c: string) => void; name: string }) {
  return (
    <div className="swatches" role="group" aria-label={`Colour of ${name}`}>
      {LABEL_COLORS.map((c) => (
        <button key={c} type="button" className="swatch" aria-label={`Colour ${c}`} aria-pressed={value?.toLowerCase() === c} style={{ background: c }} onClick={() => onPick(c)} />
      ))}
    </div>
  );
}

function LabelRow({ l }: { l: LabelInfo }) {
  const [name, setName] = useState(l.name);
  useEffect(() => setName(l.name), [l.name]);
  const [err, setErr] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const rename = () => {
    const n = name.trim();
    if (!n) { setName(l.name); return; }
    if (n === l.name) return;
    setErr(null);
    saveLabel(l.id, n, null).catch((e) => { setName(l.name); setErr(String(e)); });
  };
  const recolour = (c: string) => { setErr(null); saveLabel(l.id, l.name, c).catch((e) => setErr(String(e))); };
  const remove = async () => {
    setBusy(true);
    try { await removeLabel(l.id); } catch (e) { setErr(String(e)); setBusy(false); setAsking(false); }
  };
  return (
    <div className="label-row" data-label={l.name}>
      <div className="label-main">
        <Popover label={`Colour of ${l.name}`} button={() => (
          <button className="label-dot-btn" aria-label={`Recolour ${l.name}`} title="Pick a colour"><span className="dot" style={{ background: l.color ?? "var(--text-3)" }} /></button>)}>
          {(close) => <div className="pop-swatches"><Swatches value={l.color} name={l.name} onPick={(c) => { close(); recolour(c); }} /></div>}
        </Popover>
        <input className="stage-name label-name" aria-label={`Rename ${l.name}`} value={name} maxLength={40} onChange={(e) => setName(e.target.value)} onBlur={rename}
          onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); if (e.key === "Escape") { setName(l.name); (e.target as HTMLInputElement).blur(); } }} />
        <span className="label-cards">{cardCount(l.cards)}</span>
        <button className="btn ghost sm icon-only" aria-label={`Remove ${l.name}`} title={`Remove ${l.name}`} disabled={asking} onClick={() => { setErr(null); setAsking(true); }}><Trash2 className="icon" /></button>
      </div>
      {asking && (
        <div className="wf-confirm" role="alertdialog" aria-label={`Remove ${l.name}`}>
          <span>Remove {l.name}? {l.cards === 0 ? "No card has it." : `${l.cards === 1 ? "1 card loses" : `${l.cards} cards lose`} it.`}</span>
          <span className="wf-confirm-actions"><button className="btn ghost sm" disabled={busy} onClick={() => setAsking(false)}>Keep</button>
            <button className="btn danger sm" disabled={busy} onClick={remove}>{busy ? "Removing…" : "Remove label"}</button></span>
        </div>
      )}
      {err && <div className="wf-err" role="alert">{err}</div>}
    </div>
  );
}

/** Create a label: a name and a colour (the first one no label has yet). */
function NewLabelRow({ labels }: { labels: LabelInfo[] }) {
  const [name, setName] = useState("");
  const [color, setColor] = useState(() => nextLabelColor(labels));
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const create = async () => {
    if (!name.trim()) { setErr("Give the label a name, like Must have."); return; }
    setBusy(true); setErr(null);
    try { await saveLabel(null, name.trim(), color); setName(""); setColor(nextLabelColor([...labels, { color }])); } catch (e) { setErr(String(e)); }
    setBusy(false);
  };
  return (
    <div className="label-row label-new">
      <div className="label-main">
        <span className="dot" style={{ background: color }} aria-hidden />
        <input className="input" aria-label="New label name" placeholder="Must have" maxLength={40} value={name} disabled={busy}
          onChange={(e) => { setName(e.target.value); setErr(null); }} onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); create(); } }} />
        <Swatches value={color} name="the new label" onPick={setColor} />
        <button className="btn sm" disabled={busy} onClick={create}><Plus className="icon" />Create label</button>
      </div>
      {err && <div className="wf-err" role="alert">{err}</div>}
    </div>
  );
}

export function LabelsEditor() {
  const { data: labels, error } = useData(() => listLabels());
  if (error) return <div className="error-banner" role="alert">{error}</div>;
  if (!labels) return null;
  return (
    <div className="panel labels" aria-label="Labels">
      {labels.length === 0 && <div className="label-row faint">No labels yet. Create one below, like Must have or Could have.</div>}
      {labels.map((l) => <LabelRow key={l.id} l={l} />)}
      <NewLabelRow labels={labels} />
    </div>
  );
}
