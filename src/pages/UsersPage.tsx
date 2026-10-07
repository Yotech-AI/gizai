import { Plus } from "lucide-react";
import { listUsers } from "../api";
import { useData } from "../lib/useData";
import { useDrawer } from "../lib/drawers";
import type { Person } from "../types";
import { DataTable, type Col } from "../components/DataTable";
import { Avatar } from "../components/Avatar";

export function UsersPage() {
  const { data, error } = useData(() => listUsers());
  const open = useDrawer();
  const cols: Col<Person>[] = [
    { key: "name", header: "Name", sort: (p) => p.name, cell: (p) => <span className="who" style={{ color: "var(--text)" }}><Avatar name={p.name} />{p.name}</span> },
    { key: "handle", header: "Handle", sort: (p) => p.handle, cell: (p) => <span className="mono faint">@{p.handle}</span> },
    { key: "email", header: "Email", sort: (p) => p.email ?? "", cell: (p) => p.email ?? <span className="faint">—</span> },
    { key: "open", header: "Open tasks", align: "right", sort: (p) => p.openTasks, cell: (p) => p.openTasks },
  ];
  return (
    <>
      <div className="topbar"><div className="crumbs"><b>Users</b><span className="faint">{data?.length ?? ""} people</span></div></div>
      <div className="toolbar">
        <button className="btn" onClick={() => open({ kind: "person" })}><Plus className="icon" />Add person</button>
        <span className="spacer" /><span className="faint" style={{ fontSize: "var(--fs-sm)" }}>People can be assigned tasks and mentioned. Agents are set up on the Team page.</span>
      </div>
      {error && <div className="error-banner">{error}</div>}
      <DataTable rows={data ?? []} columns={cols} rowId={(p) => p.id} initialSort={[{ id: "name", desc: false }]}
        footer={<span>Only you sign in for now; the shared (paid) version adds logins for others.</span>} />
    </>
  );
}
