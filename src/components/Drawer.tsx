// Every create and edit form opens here: from the right, at least 1024px wide (design system: Drawer).
// Escape or a click on the scrim closes it; with unsaved changes it asks first, in place.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { X } from "lucide-react";

export function Drawer({ title, subtitle, onClose, children, actions, hint, wide, dirty, error }: {
  title: string;
  subtitle?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  /** Footer buttons, primary last. */
  actions?: ReactNode;
  /** Footer hint on the left, e.g. "Ctrl+Enter creates". */
  hint?: ReactNode;
  wide?: boolean;
  /** True when closing would lose input. */
  dirty?: boolean;
  error?: string | null;
}) {
  const [confirm, setConfirm] = useState(false);
  const live = useRef({ dirty, onClose });
  live.current = { dirty, onClose };
  const tryClose = () => { if (live.current.dirty) setConfirm(true); else live.current.onClose(); };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if ((e.target as HTMLElement).closest?.(".cm-editor")) return; // the editor's own Escape
      e.preventDefault();
      if (live.current.dirty) setConfirm(true); else live.current.onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return (
    <>
      <div className="scrim" onClick={tryClose} />
      <aside className={`drawer${wide ? " wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
        <header className="drawer-head">
          <div className="titles"><h2>{title}</h2>{subtitle && <p>{subtitle}</p>}</div>
          <button className="btn ghost icon-only" aria-label="Close" onClick={tryClose}><X className="icon" /></button>
        </header>
        {error && <div className="error-banner" role="alert" style={{ margin: "14px 28px 0" }}>{error}</div>}
        <div className="drawer-body">{children}</div>
        <footer className="drawer-foot">
          {confirm ? (
            <>
              <span className="hint" style={{ color: "var(--text)" }}>Discard what you entered?</span>
              <button className="btn ghost" onClick={() => setConfirm(false)}>Keep editing</button>
              <button className="btn danger" onClick={() => onClose()}>Discard</button>
            </>
          ) : (
            <>{hint ? <span className="hint">{hint}</span> : <span className="hint" />}{actions}</>
          )}
        </footer>
      </aside>
    </>
  );
}
