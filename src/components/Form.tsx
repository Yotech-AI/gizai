import type { ReactNode } from "react";

/** A form section: a short intro column (heading + one sentence) beside a two-column field grid. `id` is the section's,
 *  for a link that opens the form there. */
export function FormSection({ id, title, text, children }: { id?: string; title: string; text?: ReactNode; children: ReactNode }) {
  return (
    <section className="form-section" id={id}>
      <header><h3>{title}</h3>{text && <p>{text}</p>}</header>
      <div className="fields">{children}</div>
    </section>
  );
}

/** One field: label, control, then a hint, warning or error. `wide` spans both columns; `id` is the field's own. */
export function Field({ id, label, hint, warn, error, wide, htmlFor, children }: {
  id?: string; label: string; hint?: ReactNode; warn?: string | null; error?: string | null; wide?: boolean; htmlFor?: string; children: ReactNode;
}) {
  return (
    <div className={`field${wide ? " wide" : ""}`} id={id}>
      <label htmlFor={htmlFor}>{label}</label>
      {children}
      {error ? <span className="error">{error}</span> : warn ? <span className="warn">{warn}</span> : hint ? <span className="hint">{hint}</span> : null}
    </div>
  );
}
