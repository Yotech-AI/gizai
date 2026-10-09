import { useEffect, useRef, useState, type ReactNode } from "react";

/** A button that opens a small menu below it (above it with `up`, for a button at the bottom of the window); closes on an
 *  outside click or Escape. `disabled`: it doesn't open. */
export function Popover({ button, children, align = "left", label, up = false, disabled = false }: {
  button: (open: boolean) => ReactNode; children: (close: () => void) => ReactNode; align?: "left" | "right"; label: string; up?: boolean; disabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); setOpen(false); } };
    window.addEventListener("mousedown", down);
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("mousedown", down); window.removeEventListener("keydown", key); };
  }, [open]);
  useEffect(() => { if (disabled) setOpen(false); }, [disabled]);
  return (
    <div className="pop-wrap" ref={ref}>
      <span onClick={() => { if (!disabled) setOpen((o) => !o); }} aria-haspopup="menu" aria-expanded={open}>{button(open)}</span>
      {open && <div className={`pop${align === "right" ? " right" : ""}${up ? " up" : ""}`} role="menu" aria-label={label}>{children(() => setOpen(false))}</div>}
    </div>
  );
}
