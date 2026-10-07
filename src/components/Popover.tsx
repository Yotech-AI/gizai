import { useEffect, useRef, useState, type ReactNode } from "react";

/** A button that opens a small menu below it; closes on an outside click or Escape. */
export function Popover({ button, children, align = "left", label }: { button: (open: boolean) => ReactNode; children: (close: () => void) => ReactNode; align?: "left" | "right"; label: string }) {
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
  return (
    <div className="pop-wrap" ref={ref}>
      <span onClick={() => setOpen((o) => !o)} aria-haspopup="menu" aria-expanded={open}>{button(open)}</span>
      {open && <div className={`pop${align === "right" ? " right" : ""}`} role="menu" aria-label={label}>{children(() => setOpen(false))}</div>}
    </div>
  );
}
