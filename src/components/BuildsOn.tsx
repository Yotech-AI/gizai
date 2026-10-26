// Builds on (GA-228), in the card's properties: the cards it builds on, each a link with its column and a check mark once
// it is merged (in Deploy or Done), and × to take it off; Add a card finds one with the @ picker's card list
// (src/lib/pickItems.ts). Under it Needed by, read-only: the cards that build on this one. A refusal (the card itself, a
// loop, more than 20 cards) shows its message under the field.
import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Plus, X } from "lucide-react";
import { addBuildsOn, getBuildsOn, removeBuildsOn } from "../api";
import { href } from "../router";
import { useData } from "../lib/useData";
import { loadPickItems } from "../lib/pickItems";
import { pickRows, type PickItem } from "../lib/itemLinks";
import { buildsOnChoices, linkedTip } from "../lib/buildsOn";
import { ItemPicker, type PickerAt } from "./ItemPicker";
import { StatusIcon } from "./StatusIcon";
import type { BuildsOnCard, Task } from "../types";

/** One linked card: its link (ID and column, a check mark once merged), and × when it can be taken off. */
function Linked({ card, onRemove }: { card: BuildsOnCard; onRemove?: () => void }) {
  return (
    <span className="builds-on-card" data-card={card.identifier}>
      <a href={href({ page: "task", id: card.id })} title={linkedTip(card)}>
        <StatusIcon category={card.category} hold={card.hold} title={card.column} />
        <span className="id">{card.identifier}</span><span className="col">{card.column}</span>
        {card.merged && <Check className="icon sm merged" aria-label="Merged" />}
      </a>
      {onRemove && (
        <button type="button" className="btn ghost sm icon-only" aria-label={`Take ${card.identifier} off Builds on`} title="Take it off Builds on"
          onClick={onRemove}><X className="icon" /></button>
      )}
    </span>
  );
}

/** Add a card: a search box with the @ picker's list of cards under it (by column, as @task. shows them). ↑ and ↓ move,
 *  Enter or a click adds the card, Escape or leaving the box closes it. */
function AddCard({ task, taken, onAdd }: { task: Task; taken: string[]; onAdd: (identifier: string) => void }) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<PickItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [chosen, setChosen] = useState(0);
  const [at, setAt] = useState<PickerAt | null>(null);
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    let alive = true;
    loadPickItems().then((list) => { if (alive) { setItems(list); setError(null); } }).catch((e) => { if (alive) setError(String(e)); });
    const r = box.current?.getBoundingClientRect();
    if (r) setAt({ left: r.left + 8, top: r.top, bottom: r.bottom });
    return () => { alive = false; };
  }, [open]);
  const search = `task.${query}`;
  const takenKey = taken.join(",");
  const rows = useMemo(() => pickRows(search, buildsOnChoices(items ?? [], task.identifier, taken)),
    [search, items, task.identifier, takenKey]); // eslint-disable-line react-hooks/exhaustive-deps
  const selected = Math.min(chosen, Math.max(0, rows.length - 1));
  const close = () => { setOpen(false); setQuery(""); setChosen(0); setAt(null); };
  const pick = (i: number) => {
    const row = rows[i];
    if (row?.type !== "item") return;
    close();
    onAdd(row.item.key);
  };
  if (!open) {
    return <button type="button" className="btn ghost sm builds-on-add" onClick={() => setOpen(true)}><Plus className="icon" />Add a card</button>;
  }
  return (
    <>
      <input ref={box} className="input builds-on-find" autoFocus aria-label="Find a card it builds on" placeholder="ID or title"
        value={query} onChange={(e) => { setQuery(e.target.value); setChosen(0); }} onBlur={close}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") { e.preventDefault(); setChosen(Math.min(selected + 1, rows.length - 1)); }
          else if (e.key === "ArrowUp") { e.preventDefault(); setChosen(Math.max(selected - 1, 0)); }
          else if (e.key === "Enter") { e.preventDefault(); pick(selected); }
          else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); close(); }
        }} />
      {at && <ItemPicker at={at} query={search} rows={rows} loading={items === null} error={error} selected={selected} onSelect={setChosen} onPick={pick} />}
    </>
  );
}

/** The Builds on and Needed by rows. `readOnly` (an archived card): they only show. */
export function BuildsOnRows({ task, readOnly }: { task: Task; readOnly?: boolean }) {
  const { data } = useData(() => getBuildsOn(task.id), [task.id]);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => setErr(null), [task.id]);
  const on = data?.buildsOn ?? [];
  const neededBy = data?.neededBy ?? [];
  const add = (identifier: string) => { setErr(null); addBuildsOn(task.id, identifier).catch((e) => setErr(String(e))); };
  const remove = (card: BuildsOnCard) => { setErr(null); removeBuildsOn(task.id, card.id).catch((e) => setErr(String(e))); };
  return (
    <>
      <div className="prop-row prop-builds-on">
        <span className="k">Builds on</span>
        <div className="v builds-on">
          {on.map((c) => <Linked key={c.id} card={c} onRemove={readOnly ? undefined : () => remove(c)} />)}
          {readOnly ? on.length === 0 && <span className="none">None</span> : <AddCard task={task} taken={on.map((c) => c.identifier)} onAdd={add} />}
        </div>
        {err && <div className="prop-note builds-on-error" role="alert">{err}</div>}
      </div>
      <div className="prop-row prop-builds-on">
        <span className="k">Needed by</span>
        <div className="v builds-on">
          {neededBy.map((c) => <Linked key={c.id} card={c} />)}
          {neededBy.length === 0 && <span className="none">None</span>}
        </div>
      </div>
    </>
  );
}
