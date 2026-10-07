import { useMemo, useState } from "react";
import { Plus } from "lucide-react";
import { listClients } from "../api";
import { go } from "../router";
import { useData } from "../lib/useData";
import { fold } from "../lib/palette";
import { companyInitials } from "../lib/format";
import { useDrawer } from "../lib/drawers";
import type { Client } from "../types";
import { DataTable, type Col } from "../components/DataTable";

const STATUS: Record<string, string> = { active: "ok", lead: "warn", inactive: "" };

export function ClientsPage() {
  const { data, error } = useData(() => listClients());
  const open = useDrawer();
  const [status, setStatus] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const rows = useMemo(() => (data ?? []).filter((c) => (!status || c.status === status) &&
    (!q || fold([c.name, c.city, c.cocNumber, c.vatNumber, c.mainContact].filter(Boolean).join(" ")).includes(fold(q)))), [data, status, q]);
  const cols: Col<Client>[] = [
    { key: "name", header: "Client", sort: (c) => c.name, cell: (c) => <span className="who" style={{ color: "var(--text)" }}><span className="avatar" style={{ borderRadius: "var(--radius-s)" }}>{companyInitials(c.name)}</span>{c.name}</span> },
    { key: "contact", header: "Main contact", sort: (c) => c.mainContact ?? "", cell: (c) => c.mainContact ?? <span className="faint">—</span> },
    { key: "city", header: "City", sort: (c) => c.city ?? "", cell: (c) => c.city ?? "" },
    { key: "kvk", header: "KvK", cell: (c) => <span className="mono faint">{c.cocNumber ?? ""}</span> },
    { key: "vat", header: "BTW-nummer", cell: (c) => <span className="mono faint">{c.vatNumber ?? ""}</span> },
    { key: "projects", header: "Projects", align: "right", sort: (c) => c.projects, cell: (c) => c.projects },
    { key: "open", header: "Open tasks", align: "right", sort: (c) => c.openTasks, cell: (c) => c.openTasks },
    { key: "status", header: "Status", sort: (c) => c.status, cell: (c) => <span className={`badge ${STATUS[c.status] ?? ""}`}>{c.status[0].toUpperCase() + c.status.slice(1)}</span> },
  ];
  return (
    <>
      <div className="topbar"><div className="crumbs"><b>Clients</b><span className="faint">{data?.length ?? ""}</span></div></div>
      <div className="toolbar">
        <button className="btn" onClick={() => open({ kind: "client" })}><Plus className="icon" />New client</button>
        <input className="input search-input" type="search" aria-label="Filter clients" placeholder="Name, city, KvK, BTW, contact" value={q} onChange={(e) => setQ(e.target.value)} />
        <span className="spacer" />
        <div className="chips">{[null, "active", "lead", "inactive"].map((s) => (
          <button key={s ?? "all"} className={`chip${status === s ? " on" : ""}`} onClick={() => setStatus(s)}>{s ? s[0].toUpperCase() + s.slice(1) : "All"}</button>))}</div>
      </div>
      {error && <div className="error-banner">{error}</div>}
      <DataTable rows={rows} columns={cols} rowId={(c) => c.id} onRowClick={(c) => go({ page: "client", id: c.id })} keyboardNav
        initialSort={[{ id: "name", desc: false }]}
        empty={<div className="empty"><b>No clients yet.</b><span>Clients own projects. Add your first one.</span><button className="btn primary" onClick={() => open({ kind: "client" })}><Plus className="icon" />New client</button></div>}
        footer={<span>{rows.length} of {data?.length ?? 0} clients · click a header to sort, Shift+click for a second sort</span>} />
    </>
  );
}
