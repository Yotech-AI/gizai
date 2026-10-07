import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { getCoreRowModel, getSortedRowModel, useReactTable, type ColumnDef, type SortingState } from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";

export type Col<T> = {
  key: string;
  header: string;
  cell: (row: T) => ReactNode;
  /** Value used for sorting; omit to make the column unsortable. */
  sort?: (row: T) => string | number | null | undefined;
  width?: number | string;
  align?: "right";
};

type Item<T> = { type: "group"; name: string; count: number } | { type: "row"; row: T };

export function DataTable<T>({ rows, columns, rowId, onRowClick, groupBy, groupOrder, groupIcon, initialCollapsed, keyboardNav, empty, footer, initialSort }: {
  rows: T[];
  columns: Col<T>[];
  rowId: (row: T) => string;
  onRowClick?: (row: T) => void;
  groupBy?: (row: T) => string;
  groupOrder?: string[];
  groupIcon?: (group: string) => ReactNode;
  initialCollapsed?: string[];
  /** J/K move the focused row, Enter opens it (calls onRowClick). */
  keyboardNav?: boolean;
  empty?: ReactNode;
  footer?: ReactNode;
  initialSort?: SortingState;
}) {
  const [sorting, setSorting] = useState<SortingState>(initialSort ?? []);
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set(initialCollapsed ?? []));
  const [focusId, setFocusId] = useState<string | null>(null);
  const defs = useMemo<ColumnDef<T>[]>(
    () => columns.map((c) => ({
      id: c.key,
      header: c.header,
      accessorFn: (r: T) => (c.sort ? c.sort(r) ?? "" : ""),
      enableSorting: !!c.sort,
      sortingFn: (a, b, id) => {
        const x = a.getValue(id) as string | number, y = b.getValue(id) as string | number;
        if (typeof x === "number" && typeof y === "number") return x - y;
        return String(x).localeCompare(String(y), undefined, { sensitivity: "base", numeric: true });
      },
    })),
    [columns],
  );
  const table = useReactTable({
    data: rows, columns: defs, state: { sorting }, onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(), getSortedRowModel: getSortedRowModel(),
    enableMultiSort: true, isMultiSortEvent: (e) => (e as MouseEvent).shiftKey,
  });
  const sorted = table.getRowModel().rows.map((r) => r.original);
  const items = useMemo<Item<T>[]>(() => {
    if (!groupBy) return sorted.map((row) => ({ type: "row", row }));
    const groups = new Map<string, T[]>();
    for (const r of sorted) {
      const g = groupBy(r);
      if (!groups.has(g)) groups.set(g, []);
      groups.get(g)!.push(r);
    }
    const order = [...(groupOrder ?? []), ...[...groups.keys()].filter((g) => !(groupOrder ?? []).includes(g))];
    const out: Item<T>[] = [];
    for (const g of order) {
      const rs = groups.get(g);
      if (!rs) continue;
      out.push({ type: "group", name: g, count: rs.length });
      if (!collapsed.has(g)) for (const row of rs) out.push({ type: "row", row });
    }
    return out;
  }, [sorted, groupBy, groupOrder, collapsed]);

  const scroller = useRef<HTMLDivElement>(null);
  const rowH = 34;
  const virt = useVirtualizer({ count: items.length, getScrollElement: () => scroller.current, estimateSize: () => rowH, overscan: 14 });
  const vItems = virt.getVirtualItems();
  const padTop = vItems.length ? vItems[0].start : 0;
  const padBottom = vItems.length ? virt.getTotalSize() - vItems[vItems.length - 1].end : 0;

  const live = useRef({ items, focusId, onRowClick, rowId, virt });
  live.current = { items, focusId, onRowClick, rowId, virt };
  useEffect(() => {
    if (!keyboardNav) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, select, [contenteditable], .cm-editor, [role=dialog]") || e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key !== "j" && e.key !== "k" && e.key !== "Enter") return;
      const { items, focusId, onRowClick, rowId, virt } = live.current;
      const cur = items.findIndex((it) => it.type === "row" && rowId(it.row) === focusId);
      if (e.key === "Enter") {
        const it = items[cur];
        if (it?.type === "row" && onRowClick) { e.preventDefault(); onRowClick(it.row); }
        return;
      }
      e.preventDefault();
      const dir = e.key === "j" ? 1 : -1;
      for (let i = cur + dir; i >= 0 && i < items.length; i += dir) {
        const it = items[i];
        if (it.type === "row") { setFocusId(rowId(it.row)); virt.scrollToIndex(i, { align: "auto" }); return; }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [keyboardNav]);

  return (
    <>
      <div className="content" ref={scroller}>
        {rows.length === 0 && empty ? (
          <div className="page-pad">{empty}</div>
        ) : (
          <table className="grid">
            <thead>
              <tr>
                {columns.map((c, i) => {
                  const h = table.getHeaderGroups()[0].headers[i];
                  const dir = h.column.getIsSorted();
                  return (
                    <th key={c.key} style={{ width: c.width, cursor: c.sort ? "pointer" : undefined }} className={`${dir ? "sorted" : ""} ${c.align === "right" ? "num" : ""}`}
                      onClick={h.column.getToggleSortingHandler()}>
                      {c.header}{dir && <span className="arr">{dir === "asc" ? "↑" : "↓"}</span>}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {padTop > 0 && <tr style={{ height: padTop }}><td colSpan={columns.length} style={{ padding: 0, border: 0 }} /></tr>}
              {vItems.map((v) => {
                const it = items[v.index];
                if (it.type === "group") {
                  return (
                    <tr key={`g-${it.name}`} className="group" onClick={() => setCollapsed((s) => { const n = new Set(s); n.has(it.name) ? n.delete(it.name) : n.add(it.name); return n; })} style={{ cursor: "pointer" }}>
                      <td colSpan={columns.length}>{collapsed.has(it.name) ? "▸" : "▾"}&nbsp; {groupIcon && <>{groupIcon(it.name)}&nbsp; </>}{it.name} <span className="n">{it.count}</span></td>
                    </tr>
                  );
                }
                return (
                  <tr key={rowId(it.row)} className={keyboardNav && rowId(it.row) === focusId ? "focus" : undefined} onClick={() => onRowClick?.(it.row)} style={{ cursor: onRowClick ? "pointer" : undefined }}>
                    {columns.map((c) => <td key={c.key} className={c.align === "right" ? "num" : ""}>{c.cell(it.row)}</td>)}
                  </tr>
                );
              })}
              {padBottom > 0 && <tr style={{ height: padBottom }}><td colSpan={columns.length} style={{ padding: 0, border: 0 }} /></tr>}
            </tbody>
          </table>
        )}
      </div>
      {footer && <div className="tablefoot">{footer}</div>}
    </>
  );
}
