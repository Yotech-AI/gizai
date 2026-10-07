import { useState } from "react";
import { Plus } from "lucide-react";
import { listProjects } from "../api";
import { go } from "../router";
import { useData } from "../lib/useData";
import { useDrawer } from "../lib/drawers";
import type { Project } from "../types";
import { DataTable, type Col } from "../components/DataTable";

const STATUS: Record<string, string> = { active: "ok", planned: "info", paused: "warn", done: "", archived: "" };

export function ProjectsPage() {
  const { data, error } = useData(() => listProjects());
  const open = useDrawer();
  const [status, setStatus] = useState<string | null>("active");
  const rows = (data ?? []).filter((p) => !status || p.status === status);
  const cols: Col<Project>[] = [
    { key: "number", header: "Nr", width: 100, sort: (p) => p.number, cell: (p) => <span className="id">{p.number}</span> },
    { key: "name", header: "Project", sort: (p) => p.name, cell: (p) => <span className="who" style={{ color: "var(--text)" }}><span className="dot" style={{ width: 9, height: 9, borderRadius: "50%", background: p.color ?? "var(--text-3)" }} />{p.name}<span className="id">{p.key}</span></span> },
    { key: "client", header: "Client", sort: (p) => p.clientName ?? "", cell: (p) => p.clientName ?? <span className="faint">Internal</span> },
    { key: "progress", header: "Tasks done", sort: (p) => p.doneTasks / Math.max(1, p.doneTasks + p.openTasks), cell: (p) => {
      const total = p.doneTasks + p.openTasks;
      return <span className="who"><span style={{ width: 90, height: 5, borderRadius: 3, background: "var(--line)", overflow: "hidden", display: "inline-block" }}><i style={{ display: "block", height: "100%", width: `${total ? (100 * p.doneTasks) / total : 0}%`, background: "var(--success)" }} /></span>{p.doneTasks} / {total}</span>;
    } },
    { key: "repo", header: "Repository", cell: (p) => p.repoPath ? <span className="mono muted">{p.repoPath.split("/").slice(-2).join("/")}</span> : <span className="faint">Not linked</span> },
    { key: "status", header: "Status", sort: (p) => p.status, cell: (p) => <span className={`badge ${STATUS[p.status] ?? ""}`}>{p.status[0].toUpperCase() + p.status.slice(1)}</span> },
  ];
  return (
    <>
      <div className="topbar"><div className="crumbs"><b>Projects</b><span className="faint">{data?.length ?? ""}</span></div></div>
      <div className="toolbar">
        <button className="btn" onClick={() => open({ kind: "project" })}><Plus className="icon" />New project</button>
        <span className="spacer" />
        <div className="chips">{[null, "planned", "active", "paused", "done"].map((s) => (
          <button key={s ?? "all"} className={`chip${status === s ? " on" : ""}`} onClick={() => setStatus(s)}>{s ? s[0].toUpperCase() + s.slice(1) : "All"}</button>))}</div>
      </div>
      {error && <div className="error-banner">{error}</div>}
      <DataTable rows={rows} columns={cols} rowId={(p) => p.id} onRowClick={(p) => go({ page: "project", id: p.id })} keyboardNav
        initialSort={[{ id: "number", desc: true }]}
        empty={<div className="empty"><b>No projects here.</b><span>Projects hold tasks, docs and files. Link one to a git repository so agents can work on it.</span><button className="btn primary" onClick={() => open({ kind: "project" })}><Plus className="icon" />New project</button></div>}
        footer={<span>{rows.length} of {data?.length ?? 0} projects</span>} />
    </>
  );
}
