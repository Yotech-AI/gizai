// "New label…" at the end of a label picker (Properties, the New task drawer): type a name and Enter creates the label,
// with the first palette colour no label has yet, and hands it over so the card gets it. A name in use is refused with
// the backend's reason.
import { useState } from "react";
import { Plus } from "lucide-react";
import { saveLabel } from "../api";
import { nextLabelColor } from "../lib/columns";
import type { Label } from "../types";

export function NewLabel({ labels, onCreated, className = "opt" }: {
  labels: { color?: string | null }[]; onCreated: (label: Label) => void | Promise<void>;
  /** The closed button's class: a menu option (Properties) or a label pill (the drawer). */
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (!open) return <button type="button" className={`${className} new-label`} onClick={() => setOpen(true)}><Plus className="icon" />New label…</button>;
  const close = () => { setOpen(false); setName(""); setErr(null); };
  const create = async () => {
    const n = name.trim();
    if (!n) { setErr("Type the label's name, then press Enter."); return; }
    setBusy(true); setErr(null);
    const color = nextLabelColor(labels);
    try { const id = await saveLabel(null, n, color); await onCreated({ id, name: n, color }); close(); } catch (e) { setErr(String(e)); }
    setBusy(false);
  };
  return (
    <div className="new-label-form">
      <input className="input" autoFocus aria-label="New label name" placeholder="New label" maxLength={40} value={name} disabled={busy}
        onChange={(e) => { setName(e.target.value); setErr(null); }}
        // Enter creates (and doesn't submit the drawer's form); Escape closes this box only.
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); e.stopPropagation(); create(); }
          if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
        }} />
      {err ? <span className="error" role="alert">{err}</span> : <span className="hint">Enter creates it and puts it on the card</span>}
    </div>
  );
}
