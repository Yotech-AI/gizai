// A row of tabs above a page's panels, each an icon and a label: the Usage page's and Settings' (GA-42).
import type { LucideIcon } from "lucide-react";

export type TabDef<T extends string> = readonly [T, LucideIcon, string];

/** `open` is the tab whose panel the page shows; `onOpen` is called with the tab clicked. */
export function Tabs<T extends string>({ tabs, open, onOpen, label }: { tabs: readonly TabDef<T>[]; open: T; onOpen: (tab: T) => void; label?: string }) {
  return (
    <div className="tabs" role="tablist" aria-label={label}>
      {tabs.map(([k, I, l]) => (
        <button key={k} className="tab" role="tab" aria-selected={open === k} onClick={() => onOpen(k)}><I className="icon" />{l}</button>
      ))}
    </div>
  );
}
