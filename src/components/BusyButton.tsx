// A button that starts something slow (Run, Continue, Stop). From the click until usePending says the screen shows
// what it started, it shows a spinner and a short label ("Starting…"), and neither it nor the other buttons of the same
// `pending` can be clicked, so nothing starts twice.
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { LoaderCircle } from "lucide-react";
import type { Pending } from "../lib/usePending";

export function BusyButton<K extends string>({ pending, name, busyLabel, icon, disabled, children, ...rest }: ButtonHTMLAttributes<HTMLButtonElement> & {
  pending: Pending<K>; name: K; busyLabel: string; icon?: ReactNode;
}) {
  const busy = pending.busy === name;
  return (
    <button {...rest} disabled={disabled || pending.busy !== null} aria-busy={busy || undefined}>
      {busy ? <><LoaderCircle className="icon spin" />{busyLabel}</> : <>{icon}{children}</>}
    </button>
  );
}
