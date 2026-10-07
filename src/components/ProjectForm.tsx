import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { checkRepo, getProject, listClients, saveProject, type RepoCheck } from "../api";
import { go } from "../router";
import type { Project, ProjectInput } from "../types";
import { suggestKey } from "../lib/projectKey";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";

/** The design system's decorative colours (c-blue … c-grey). */
export const PROJECT_COLORS = ["#6f97ff", "#3fb8a0", "#e3bd3c", "#a98bfa", "#f29a4a", "#e77ab3", "#f2706b", "#8a8e98"];

export function toProjectInput(p?: Project | null): ProjectInput {
  return {
    name: p?.name ?? "", key: p?.key ?? "", clientId: p?.clientId ?? null, status: p?.status ?? "active", goalMd: p?.goalMd ?? "",
    repoPath: p?.repoPath ?? "", repoUrl: p?.repoUrl ?? "", defaultBranch: p?.defaultBranch ?? "main", color: p?.color ?? PROJECT_COLORS[0],
    budgetAmountMinor: p?.budgetAmountMinor ?? null, budgetHours: p?.budgetHours ?? null,
  };
}

/** New project (no id) or edit one, in a drawer. */
export function ProjectDrawer({ id, onClose }: { id?: string; onClose: () => void }) {
  const isNew = !id;
  const [initial, setInitial] = useState<ProjectInput | null>(isNew ? toProjectInput(null) : null);
  const [v, setV] = useState<ProjectInput | null>(initial);
  const [clients, setClients] = useState<{ id: string; name: string }[]>([]);
  const [repo, setRepo] = useState<RepoCheck | null>(null);
  const [keyTouched, setKeyTouched] = useState(!isNew);
  const [urlTouched, setUrlTouched] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => { listClients().then((c) => setClients(c.map((x) => ({ id: x.id, name: x.name })))).catch(() => {}); }, []);
  useEffect(() => { if (id) getProject(id).then((p) => { const i = toProjectInput(p); setInitial(i); setV(i); }).catch((e) => setErr(String(e))); }, [id]);
  const repoPath = v?.repoPath?.trim();
  useEffect(() => {
    if (!repoPath) { setRepo(null); return; }
    const t = setTimeout(() => checkRepo(repoPath).then(setRepo).catch(() => setRepo(null)), 300);
    return () => clearTimeout(t);
  }, [repoPath]);
  // Offer the folder's GitHub remote as the link until you type one yourself.
  useEffect(() => {
    if (repo?.github && v && !urlTouched && !v.repoUrl?.trim()) setV({ ...v, repoUrl: repo.github });
  }, [repo?.github]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!v || !initial) return null;
  const set = (k: keyof ProjectInput, val: unknown) => setV({ ...v, [k]: val });
  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Choose the project's git repository" });
    if (typeof dir === "string") set("repoPath", dir);
  };
  const save = async () => {
    setBusy(true);
    try { const saved = await saveProject(id ?? null, v); onClose(); if (isNew) go({ page: "project", id: saved }); }
    catch (e) { setErr(String(e)); setBusy(false); }
  };
  const euros = v.budgetAmountMinor == null ? "" : String(v.budgetAmountMinor / 100);
  const repoNote = repo && !repo.isGit ? { warn: "This folder isn't a git repository" } : repo?.dirty ? { warn: `On branch ${repo.branch ?? "?"}, with uncommitted changes` }
    : repo?.isGit ? { hint: `Git repository · on branch ${repo.branch ?? "?"}` } : { hint: "Agents work in their own git worktree of it." };
  return (
    <Drawer title={isNew ? "New project" : `Edit ${initial.name}`} subtitle="Projects hold tasks, docs and files. Agents work on projects linked to a git repository."
      onClose={onClose} dirty={JSON.stringify(v) !== JSON.stringify(initial)} error={err} hint="Ctrl+Enter saves"
      actions={<><button className="btn ghost" onClick={onClose}>Cancel</button><button className="btn primary" disabled={busy || !v.name.trim()} onClick={save}>{isNew ? "Create project" : "Save changes"}</button></>}>
      <form className="form" onSubmit={(e) => { e.preventDefault(); save(); }} onKeyDown={(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) { e.preventDefault(); save(); } }}>
        <FormSection title="Project" text={<>Its name, client and task key. Tasks become {v.key || "KEY"}-1, {v.key || "KEY"}-2…</>}>
          <Field label="Name" htmlFor="p-name"><input id="p-name" className="input" autoFocus value={v.name}
            onChange={(e) => setV({ ...v, name: e.target.value, key: keyTouched ? v.key : suggestKey(e.target.value) })} placeholder="Kade portal" /></Field>
          <Field label="Task key" htmlFor="p-key" hint={isNew ? "2–6 letters or digits" : "Fixed after creation"}>
            <input id="p-key" className="input mono" maxLength={6} disabled={!isNew} value={v.key} onChange={(e) => { setKeyTouched(true); set("key", e.target.value.toUpperCase()); }} /></Field>
          <Field label="Client" htmlFor="p-client"><select id="p-client" className="select" value={v.clientId ?? ""} onChange={(e) => set("clientId", e.target.value || null)}>
            <option value="">Internal (no client)</option>{clients.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}</select></Field>
          <Field label="Status" htmlFor="p-status"><select id="p-status" className="select" value={v.status ?? "active"} onChange={(e) => set("status", e.target.value)}>
            {["planned", "active", "paused", "done", "archived"].map((s) => <option key={s} value={s}>{s[0].toUpperCase() + s.slice(1)}</option>)}</select></Field>
          <Field label="Colour" wide><div className="swatches">{PROJECT_COLORS.map((c) => (
            <button key={c} type="button" className="swatch" aria-label={`Colour ${c}`} aria-pressed={v.color === c} style={{ background: c }} onClick={() => set("color", c)} />))}</div></Field>
        </FormSection>
        <FormSection title="Repository" text="Agents work in their own git worktree of this repository, on a branch per task.">
          <Field label="Git repository" htmlFor="p-repo" wide {...repoNote}>
            <div className="input-group"><input id="p-repo" className="input mono" placeholder="/home/you/Code/project" value={v.repoPath ?? ""} onChange={(e) => set("repoPath", e.target.value)} />
              <button type="button" className="btn" onClick={chooseFolder}>Choose…</button></div></Field>
          <Field label="GitHub repository" htmlFor="p-url" wide
            hint={v.repoUrl?.trim() ? "New cards start from the main branch fetched from here; agents hear when it moves on"
              : repo?.isGit && !repo.github ? "This folder has no GitHub remote: paste the link, or leave it empty to start from the local branch"
              : "Optional. With a link, new cards start from the main branch fetched from GitHub"}>
            <input id="p-url" className="input mono" placeholder="https://github.com/owner/name" value={v.repoUrl ?? ""}
              onChange={(e) => { setUrlTouched(true); set("repoUrl", e.target.value); }} /></Field>
          <Field label="Main branch" htmlFor="p-branch" hint="New task branches start here"><input id="p-branch" className="input mono" value={v.defaultBranch ?? "main"} onChange={(e) => set("defaultBranch", e.target.value)} /></Field>
        </FormSection>
        <FormSection title="Budget" text="Optional. Shown on the project page.">
          <Field label="Budget (€)" htmlFor="p-eur"><input id="p-eur" className="input" type="number" min={0} value={euros} onChange={(e) => set("budgetAmountMinor", e.target.value === "" ? null : Math.round(Number(e.target.value) * 100))} /></Field>
          <Field label="Budget (hours)" htmlFor="p-hours"><input id="p-hours" className="input" type="number" min={0} value={v.budgetHours ?? ""} onChange={(e) => set("budgetHours", e.target.value === "" ? null : Number(e.target.value))} /></Field>
        </FormSection>
        <FormSection title="Goal" text="What this project must achieve. Agents read it with every task.">
          <Field label="Goal" wide><MarkdownEditor value={v.goalMd ?? ""} onChange={(md) => setV((x) => (x ? { ...x, goalMd: md } : x))} ariaLabel="Goal" minHeight={140}
            placeholder="Give Kade's office one place for shipments, invoices and drivers." /></Field>
        </FormSection>
      </form>
    </Drawer>
  );
}
