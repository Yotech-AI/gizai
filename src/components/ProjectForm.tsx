import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Plus, X } from "lucide-react";
import { checkRepo, getProject, listClients, saveProject, type RepoCheck } from "../api";
import { go } from "../router";
import type { Project, ProjectInput, ProjectScreenshots } from "../types";
import { modKey } from "../lib/keys";
import { suggestKey } from "../lib/projectKey";
import {
  HEIGHT, nextSize, projectShotsFrom, projectShotsProblem, shownNumber, SIZES, START_SECS, typedNumber, WIDTH, type ShotSize, type ShotsProblem,
} from "../lib/screenshotSettings";
import { TEST_RUNS, testRunsProblem, testRunsToSave } from "../lib/testRunsAtOnce";
import { Drawer } from "./Drawer";
import { Field, FormSection } from "./Form";
import { MarkdownEditor } from "./MarkdownEditor";

/** The design system's decorative colours (c-blue … c-grey). */
export const PROJECT_COLORS = ["#6f97ff", "#3fb8a0", "#e3bd3c", "#a98bfa", "#f29a4a", "#e77ab3", "#f2706b", "#8a8e98"];

/** The form's values for project `p`, or a new project's. Preview (GA-185): Screenshots on, Desktop 1280 × 800 and Mobile
 *  390 × 844, one theme, one window's height, no Preview command (saved as "", which leaves none) and 120 seconds. Test
 *  runs at once (GA-212): empty, no limit (saved as 0, which leaves none). */
export function toProjectInput(p?: Project | null): ProjectInput {
  return {
    name: p?.name ?? "", key: p?.key ?? "", clientId: p?.clientId ?? null, status: p?.status ?? "active", goalMd: p?.goalMd ?? "",
    repoPath: p?.repoPath ?? "", repoUrl: p?.repoUrl ?? "", defaultBranch: p?.defaultBranch ?? "main", color: p?.color ?? PROJECT_COLORS[0],
    budgetAmountMinor: p?.budgetAmountMinor ?? null, budgetHours: p?.budgetHours ?? null,
    worktreeCopy: p?.worktreeCopy ?? [], worktreeInstall: p?.worktreeInstall ?? true, worktreeSetup: p?.worktreeSetup ?? "",
    testRunsAtOnce: p?.testRunsAtOnce ?? null,
    leadMayMerge: p?.leadMayMerge ?? false,
    screenshots: projectShotsFrom(p?.screenshots), previewCommand: p?.previewCommand ?? "", previewStartSecs: p?.previewStartSecs ?? START_SECS.default,
  };
}

/** The copy list as typed: one path per line. */
export function copyPaths(text: string): string[] {
  return text.split("\n").map((l) => l.trim()).filter(Boolean);
}

/** The input of a size's name, width or height in Preview → Sizes. */
const sizeInput = (i: number, part: "name" | "width" | "height") => `p-size-${i}-${part}`;
/** The input that holds Preview's problem. */
const problemInput = (p: ShotsProblem) => (p.at === "start" ? "p-start" : p.at === "sizes" ? sizeInput(0, "name") : sizeInput(p.size, p.at));

/** Puts the focus on the input `id` once the form has drawn it, its text selected with `select`. */
function focusSoon(id: string, select = false) {
  setTimeout(() => {
    if (typeof document === "undefined") return;
    const el = document.getElementById(id) as HTMLInputElement | null;
    el?.focus();
    if (select) el?.select();
  }, 0);
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
  const [copyText, setCopyText] = useState<string | null>(null);
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
  // Offer the folder's GitHub or Bitbucket remote as the link until you type one yourself.
  const remote = repo?.github || repo?.bitbucket;
  useEffect(() => {
    if (remote && v && !urlTouched && !v.repoUrl?.trim()) setV({ ...v, repoUrl: remote });
  }, [remote]); // eslint-disable-line react-hooks/exhaustive-deps
  if (!v || !initial) return null;
  const set = (k: keyof ProjectInput, val: unknown) => setV({ ...v, [k]: val });
  const chooseFolder = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Choose the project's git repository" });
    if (typeof dir === "string") set("repoPath", dir);
  };
  // Preview (GA-185): the project's screenshots, its Preview command and Start time
  const shots = v.screenshots ?? projectShotsFrom(null);
  const setShots = (s: Partial<ProjectScreenshots>) => setV({ ...v, screenshots: { ...shots, ...s } });
  const setSize = (i: number, s: Partial<ShotSize>) => setShots({ sizes: shots.sizes.map((x, j) => (j === i ? { ...x, ...s } : x)) });
  const addSize = () => { setShots({ sizes: [...shots.sizes, nextSize(shots.sizes)] }); focusSoon(sizeInput(shots.sizes.length, "name"), true); };
  const removeSize = (i: number) => {
    setShots({ sizes: shots.sizes.filter((_, j) => j !== i) });
    focusSoon(sizeInput(Math.min(i, shots.sizes.length - 2), "name"));
  };
  const save = async () => {
    // A value outside its range is refused before saving, with why, as the backend would refuse it.
    const testRuns = testRunsProblem(v.testRunsAtOnce);
    if (testRuns) { setErr(testRuns); focusSoon("p-test-runs"); return; }
    const problem = projectShotsProblem(v);
    if (problem) { setErr(problem.message); focusSoon(problemInput(problem)); return; }
    setBusy(true);
    // An empty Test runs at once is saved as 0, which removes the limit (left out, it would be kept).
    const input = { ...v, testRunsAtOnce: testRunsToSave(v.testRunsAtOnce) };
    try { const saved = await saveProject(id ?? null, input); onClose(); if (isNew) go({ page: "project", id: saved }); }
    catch (e) { setErr(String(e)); setBusy(false); }
  };
  const euros = v.budgetAmountMinor == null ? "" : String(v.budgetAmountMinor / 100);
  const copy = v.worktreeCopy ?? [];
  const setCopy = (text: string) => { setCopyText(text); setV({ ...v, worktreeCopy: copyPaths(text) }); };
  const suggested = (repo?.suggestCopy ?? []).filter((p) => !copy.some((c) => c.replace(/\/$/, "") === p.replace(/\/$/, "")));
  const repoNote = repo && !repo.isGit ? { warn: "This folder isn't a git repository" } : repo?.dirty ? { warn: `On branch ${repo.branch ?? "?"}, with uncommitted changes` }
    : repo?.isGit ? { hint: `Git repository · on branch ${repo.branch ?? "?"}` } : { hint: "Agents work in their own git worktree of it." };
  return (
    <Drawer title={isNew ? "New project" : `Edit ${initial.name}`} subtitle="Projects hold tasks, docs and files. Agents work on projects linked to a git repository."
      onClose={onClose} dirty={JSON.stringify(v) !== JSON.stringify(initial)} error={err} hint={`${modKey()}+Enter saves`}
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
          <Field label="Repository" htmlFor="p-url" wide
            hint={v.repoUrl?.trim() ? "New cards start from the main branch fetched from here; agents hear when it moves on"
              : repo?.isGit && !remote ? "This folder has no GitHub or Bitbucket remote: paste the link, or leave it empty to start from the local branch"
              : "Optional. With a GitHub or Bitbucket link, new cards start from the main branch fetched from there"}>
            <input id="p-url" className="input mono" placeholder="https://github.com/owner/name or https://bitbucket.org/workspace/name" value={v.repoUrl ?? ""}
              onChange={(e) => { setUrlTouched(true); set("repoUrl", e.target.value); }} /></Field>
          <Field label="Main branch" htmlFor="p-branch" hint="New task branches start here"><input id="p-branch" className="input mono" value={v.defaultBranch ?? "main"} onChange={(e) => set("defaultBranch", e.target.value)} /></Field>
          <Field label="Merging" wide hint="On GitHub only. The Team Lead merges a card in Review with a merge commit once QA passed its latest commit and every check succeeded, in chat and in its board checks. Releases and deploys stay yours.">
            <label className="check"><input type="checkbox" checked={v.leadMayMerge ?? false} onChange={(e) => set("leadMayMerge", e.target.checked)} />Team Lead may merge</label>
          </Field>
        </FormSection>
        <FormSection title="New worktrees" text="Each card gets its own git worktree. Before the agent starts in a new one, Gizai copies these paths, installs what is still missing, then runs the setup command.">
          <Field label="Copy from the main checkout" htmlFor="p-copy" wide
            hint="One file or folder per line, relative to the repository. Copied with cp --reflink=auto (instant on btrfs), so agents start with a warm build. A path the main checkout doesn't have is skipped.">
            <textarea id="p-copy" className="textarea mono" rows={4} value={copyText ?? copy.join("\n")} onChange={(e) => setCopy(e.target.value)}
              placeholder={".env\nnode_modules/\nvendor/\ntarget/"} />
            {suggested.length > 0 && <div><button type="button" className="btn sm" onClick={() => setCopy([...copy, ...suggested].join("\n"))}>
              Add {suggested.join(", ")}</button> <span className="faint">found in the repository</span></div>}
          </Field>
          <Field label="Install" wide hint="composer install when there is a composer.json but no vendor/; npm ci (npm install without a package-lock.json) when there is a package.json but no node_modules/. A folder that is already there is left alone.">
            <label className="check"><input type="checkbox" checked={v.worktreeInstall ?? true} onChange={(e) => set("worktreeInstall", e.target.checked)} />Install missing dependencies</label>
          </Field>
          <Field label="Setup command" htmlFor="p-setup" wide hint="Optional. Runs in each new worktree after the install, before the agent starts: a migration, a build. If it fails, the card goes on hold with its output.">
            <input id="p-setup" className="input mono" value={v.worktreeSetup ?? ""} onChange={(e) => set("worktreeSetup", e.target.value)} placeholder="php artisan migrate --seed" />
          </Field>
          <Field label="Test runs at once" htmlFor="p-test-runs"
            hint="How many of this project's cards are tested at the same time. A test run builds and runs the whole test suite; on a big project (Rust, a large monorepo) more than one at a time can fill the disk and slow your computer down. Empty: no limit.">
            <input id="p-test-runs" className="input" type="number" min={TEST_RUNS.low} max={TEST_RUNS.high} step={1} value={shownNumber(v.testRunsAtOnce)}
              onChange={(e) => set("testRunsAtOnce", typedNumber(e.target.value))} />
          </Field>
        </FormSection>
        <FormSection id="project-preview" title="Preview" text="While an agent works on a card, Gizai serves the card’s branch with demo data, so the agent can take screenshots of the screens it changed. The card shows them on its Screenshots tab.">
          <Field label="Screenshots" htmlFor="p-shots" wide hint="On by default. Off: no agent takes any here, and its cards have no Screenshots tab.">
            <label className="check"><input id="p-shots" type="checkbox" checked={shots.on} onChange={(e) => setShots({ on: e.target.checked })} />Take screenshots of this project’s screens</label>
          </Field>
          <Field label="Preview command" htmlFor="p-preview" wide
            hint="Empty by default: no preview, so no screenshots. Runs with bash -c in the card’s worktree when a run starts whose agent takes screenshots, on a free port in GIZAI_PREVIEW_PORT (and PORT), with GIZAI_PREVIEW_DIR for a throwaway database. Serve demo data, never your .env database. Gizai ends it with the run.">
            <input id="p-preview" className="input mono" spellCheck={false} autoComplete="off" value={v.previewCommand ?? ""} onChange={(e) => set("previewCommand", e.target.value)}
              placeholder={'npx astro dev --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --ignore-lock'} />
          </Field>
          <Field label="Start time (seconds)" htmlFor="p-start" hint="10 to 600; 120 by default. The agent starts once the page answers, or after this long without it.">
            <input id="p-start" className="input" type="number" min={START_SECS.low} max={START_SECS.high} step={1} value={shownNumber(v.previewStartSecs)}
              onChange={(e) => set("previewStartSecs", typedNumber(e.target.value))} />
          </Field>
          <Field label="Themes" htmlFor="p-themes" hint="Off by default (on for Gizai). On: each changed screen in the first theme, and the card’s main screen in the other one too.">
            <label className="check"><input id="p-themes" type="checkbox" checked={shots.bothThemes} onChange={(e) => setShots({ bothThemes: e.target.checked })} />Both themes, dark and light</label>
          </Field>
          <Field label="Sizes" htmlFor={sizeInput(0, "name")} wide
            hint="1 to 4 sizes, each 320 to 3840 wide and 320 to 2400 high. Desktop 1280 × 800 and Mobile 390 × 844 by default; Gizai’s own are Window 1440 × 900 and Small 900 × 600.">
            <div className="shot-sizes" role="group" aria-label="Sizes">
              {shots.sizes.map((size, i) => (
                <div key={i} className="shot-size">
                  <input id={sizeInput(i, "name")} className="input shot-name" aria-label={`Name of size ${i + 1}`} value={size.name} placeholder="Desktop"
                    onChange={(e) => setSize(i, { name: e.target.value })} />
                  <input id={sizeInput(i, "width")} className="input shot-px" type="number" min={WIDTH.low} max={WIDTH.high} step={1} aria-label={`Width of size ${i + 1}`}
                    value={shownNumber(size.width)} onChange={(e) => setSize(i, { width: typedNumber(e.target.value) })} />
                  <span className="faint" aria-hidden="true">×</span>
                  <input id={sizeInput(i, "height")} className="input shot-px" type="number" min={HEIGHT.low} max={HEIGHT.high} step={1} aria-label={`Height of size ${i + 1}`}
                    value={shownNumber(size.height)} onChange={(e) => setSize(i, { height: typedNumber(e.target.value) })} />
                  {shots.sizes.length > SIZES.low && (
                    <button type="button" className="btn ghost sm icon-only" aria-label={`Remove size ${i + 1}`} title="Remove" onClick={() => removeSize(i)}><X className="icon" /></button>
                  )}
                </div>
              ))}
              {shots.sizes.length < SIZES.high && <button type="button" className="btn ghost sm" onClick={addSize}><Plus className="icon" />Add a size</button>}
            </div>
          </Field>
          <Field label="Whole page" htmlFor="p-fullpage" wide hint="Off by default: one window’s height at each size.">
            <label className="check"><input id="p-fullpage" type="checkbox" checked={shots.fullPage} onChange={(e) => setShots({ fullPage: e.target.checked })} />The whole length of each page</label>
          </Field>
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
