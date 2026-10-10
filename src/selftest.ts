// Self-test probes, used only when Gizai runs with GIZAI_SELFTEST (headless cage, test data).
import { EditorView } from "@codemirror/view";
import { emit } from "@tauri-apps/api/event";
import { appInfo, archiveTask, chatMessages, getTask, listChatThreads, listDocs, listLabels, listProjects, listTasks, setAgentStatus } from "./api";
import { isMac } from "./lib/keys";
import { periodDays } from "./lib/usage";
import { resetAppearance } from "./lib/appearance";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const nextFrame = () => new Promise((r) => requestAnimationFrame(() => r(null)));

function pe(type: string, x: number, y: number) {
  return new PointerEvent(type, {
    bubbles: true, cancelable: true, composed: true, pointerId: 1, pointerType: "mouse", isPrimary: true,
    button: 0, buttons: type === "pointerup" ? 0 : 1, clientX: x, clientY: y,
  });
}

async function waitFor<T>(f: () => T | null | undefined, ms = 5000): Promise<T | null> {
  const t0 = performance.now();
  while (performance.now() - t0 < ms) { const v = f(); if (v) return v; await sleep(50); }
  return null;
}

/** Drags the first "To do" card into "In progress" the way a mouse would, then reads the task back. */
export async function dragProbe(from = "To do", to = "In progress") {
  const card = await waitFor(() => document.querySelector(`[data-col="${from}"] .card`) as HTMLElement | null);
  const target = document.querySelector(`[data-col="${to}"] .col-body`) as HTMLElement | null;
  if (!card || !target) return { moved: false, error: `no card in ${from} or no ${to} column` };
  const identifier = card.dataset.card ?? "";
  const r = card.getBoundingClientRect(), cr = target.getBoundingClientRect();
  const x0 = r.left + r.width / 2, y0 = r.top + r.height / 2, x1 = cr.left + cr.width / 2, y1 = cr.top + 40;
  card.dispatchEvent(pe("pointerdown", x0, y0));
  const steps = 40;
  for (let i = 1; i <= steps; i++) { document.dispatchEvent(pe("pointermove", x0 + ((x1 - x0) * i) / steps, y0 + ((y1 - y0) * i) / steps)); await nextFrame(); }
  for (let i = 0; i < 10; i++) { document.dispatchEvent(pe("pointermove", x1, y1 + i)); await nextFrame(); }
  document.dispatchEvent(pe("pointerup", x1, y1 + 10));
  let stateName = "";
  for (let i = 0; i < 20; i++) {
    await sleep(100);
    const t = (await listTasks()).find((x) => x.identifier === identifier);
    stateName = t?.stateName ?? "";
    if (stateName === to) break;
  }
  const onScreen = !!document.querySelector(`[data-col="${to}"] [data-card="${identifier}"]`);
  return { moved: stateName === to && onScreen, identifier, from, to, stored_in: stateName, on_screen_in_target: onScreen };
}

/** Opens the task's description editor, links two items with the @ picker (GA-41), types like a keyboard would, saves with
 * Ctrl+Enter, reads the task back. `saved` also needs the picker's checks and the saved links shown as chips that open. */
export async function editorProbe(taskId: string, getDescription: (id: string) => Promise<string>, holdMs = 0) {
  const box = await waitFor(() => document.querySelector(".md-click") as HTMLElement | null);
  if (!box) return { saved: false, error: "no description block" };
  box.click();
  const content = await waitFor(() => document.querySelector(".md-edit-box .cm-content") as HTMLElement | null);
  if (!content) return { saved: false, error: "editor did not open" };
  await sleep(200);
  const dimmed = document.querySelectorAll(".md-edit-box .cm-md-mark").length;
  const picker = await taskPickerProbe(content);
  if (!picker.ok) return { saved: false, error: "the @ picker in the description editor", picker };
  const typed = " Typed by the self-test: café ✓";
  document.execCommand("insertText", false, typed);
  await sleep(holdMs);
  content.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, ctrlKey: !isMac(), metaKey: isMac(), bubbles: true, cancelable: true }));
  let stored = "";
  for (let i = 0; i < 20; i++) {
    await sleep(100);
    stored = await getDescription(taskId);
    if (stored.endsWith(typed)) break;
  }
  const closed = !document.querySelector(".md-edit-box");
  const links = /\]\(gizai:task\/[A-Z][A-Z0-9]*-\d+\)/.test(stored) && /\]\(gizai:project\/[A-Z0-9]+\)/.test(stored);
  const shown = await shownChipsProbe();
  return { saved: stored.endsWith(typed) && links && shown.ok, editor_closed: closed, dimmed_marks: dimmed, task: taskId, links_saved: links, picker, shown };
}

const docOf = (content: Element) => EditorView.findFromDOM(content as HTMLElement)?.state.doc.toString() ?? "";
const pickerRows = () => [...document.querySelectorAll(".item-picker .opt")].map((e) => (e.textContent ?? "").trim());
const pickerReady = (test: (rows: string[]) => boolean) => waitFor(() => { const r = pickerRows(); return r.length > 0 && test(r) ? r : null; }, 4000);
const KINDS = ["Task@task.", "Project@project.", "Client@client.", "Agent@agent.", "Person@person.", "Doc@doc."];
const onlyTasks = (rows: string[]) => rows.every((r) => /^[A-Z][A-Z0-9]*-\d+ - /.test(r));
const press = (el: HTMLElement, key: string, keyCode: number, more: KeyboardEventInit = {}) =>
  el.dispatchEvent(new KeyboardEvent("keydown", { key, code: key, keyCode, bubbles: true, cancelable: true, ...more }));
const write = (text: string) => document.execCommand("insertText", false, text);

/** GA-41 in the task page's description editor: @ lists the kinds, @task. only tasks; ↓ moves the selection; Escape closes
 * only the picker; Enter picks (a link, no new line, still editing); a click on a row picks too and leaves the edit box open
 * and focused; links show as chips; `@zzqq`, which finds nothing, stays a mention. */
async function taskPickerProbe(content: HTMLElement) {
  const view = EditorView.findFromDOM(content);
  if (!view) return { ok: false, error: "no CodeMirror view" };
  content.focus();
  view.dispatch({ selection: { anchor: view.state.doc.length } });
  write(" @");
  const kinds = await pickerReady((r) => r.length === 6);
  const listsKinds = JSON.stringify(kinds) === JSON.stringify(KINDS);
  write("task.");
  const tasks = await pickerReady(onlyTasks);
  press(content, "ArrowDown", 40);
  await sleep(150);
  const opts = document.querySelectorAll(".item-picker .opt");
  const moved = opts.length > 1 ? opts[1].classList.contains("active") && !opts[0].classList.contains("active") : opts.length === 1;
  press(content, "Escape", 27);
  await sleep(200);
  const escClosesPicker = !document.querySelector(".item-picker");
  const escKeepsEdit = !!document.querySelector(".md-edit-box") && docOf(content).endsWith(" @task.");

  write(" @task.");
  await pickerReady(onlyTasks);
  const lines = docOf(content).split("\n").length;
  press(content, "Enter", 13);
  await sleep(200);
  const afterEnter = docOf(content);
  const enterLinks = /\[[A-Z][A-Z0-9]*-\d+ - [^\]]+\]\(gizai:task\/[A-Z][A-Z0-9]*-\d+\) $/.test(afterEnter);
  const noNewLine = afterEnter.split("\n").length === lines;
  const stillEditing = !!document.querySelector(".md-edit-box") && !document.querySelector(".item-picker");

  write("@project.");
  const projects = await pickerReady((r) => r.every((x) => / - /.test(x)));
  const row = document.querySelector(".item-picker .opt") as HTMLElement | null;
  row?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, cancelable: true }));
  row?.click();
  await sleep(250);
  const clickLinks = /\]\(gizai:project\/[A-Z0-9]+\) $/.test(docOf(content));
  const keptOpen = !!document.querySelector(".md-edit-box") && document.activeElement === content;
  const chips = [...document.querySelectorAll(".md-edit-box .cm-item-chip")].map((e) => e.textContent ?? "");

  write("@zzqq");
  await sleep(400);
  const mention = !document.querySelector(".item-picker") && [...document.querySelectorAll(".md-edit-box .cm-chip-mention")].some((e) => e.textContent === "@zzqq");
  const ok = listsKinds && !!tasks && moved && escClosesPicker && escKeepsEdit && enterLinks && noNewLine && stillEditing && !!projects && clickLinks && keptOpen
    && chips.length === 2 && mention;
  return { ok, kinds, tasks: tasks?.slice(0, 3), arrow_moves: moved, escape_closes_picker: escClosesPicker, escape_keeps_edit: escKeepsEdit,
    enter_links: enterLinks, enter_adds_no_line: noNewLine, still_editing: stillEditing, projects: projects?.slice(0, 2), click_links: clickLinks,
    click_keeps_edit_focused: keptOpen, chips, at_name_stays_mention: mention };
}

/** The saved description shows its links as chips with the items' names; a click on the project's opens its page. */
async function shownChipsProbe() {
  const chips = await waitFor(() => { const c = [...document.querySelectorAll(".md-click a.item-chip, .prose a.item-chip")] as HTMLElement[]; return c.length >= 2 ? c : null; }, 3000);
  if (!chips) return { ok: false, error: "no chips in the saved description" };
  const kinds = chips.map((c) => [...c.classList].find((k) => k.startsWith("kind-")));
  const project = chips.find((c) => c.classList.contains("kind-project"));
  project?.click();
  const opened = await waitFor(() => (window.location.hash.startsWith("#/project/") ? window.location.hash : null), 3000);
  const page = await waitFor(() => textOf(document.querySelector(".entity-head h1")) || null, 3000);
  const ok = kinds.includes("kind-task") && !!opened && !!page && page === textOf(project);
  return { ok, kinds, chip_texts: chips.map((c) => textOf(c)), opened, page };
}

/** Doc page: type and save with Ctrl+S; then type again while "an agent" saves underneath, expect the
 * conflict banner, and keep our text with "Save mine as a new version". */
export async function docProbe(
  docId: string,
  api: { getDoc: (id: string) => Promise<{ bodyMd: string; currentVersion: number }>; saveDoc: (id: string, md: string, base: number) => Promise<number> },
) {
  const content = await waitFor(() => document.querySelector(".doc-editor .cm-content") as HTMLElement | null);
  if (!content) return { ok: false, error: "doc editor not found" };
  const v0 = (await api.getDoc(docId)).currentVersion;
  const ctrlS = () => content.dispatchEvent(new KeyboardEvent("keydown", { key: "s", code: "KeyS", keyCode: 83, ctrlKey: !isMac(), metaKey: isMac(), bubbles: true, cancelable: true }));
  const until = async (f: (d: { bodyMd: string; currentVersion: number }) => boolean) => {
    for (let i = 0; i < 30; i++) { await sleep(100); const d = await api.getDoc(docId); if (f(d)) return d; }
    return api.getDoc(docId);
  };
  content.focus(); await sleep(100);
  document.execCommand("insertText", false, "Probe one. ");
  ctrlS();
  const d1 = await until((d) => d.currentVersion === v0 + 1);
  const first = d1.bodyMd.includes("Probe one.");

  content.focus(); await sleep(100);
  document.execCommand("insertText", false, "Probe two. ");
  await api.saveDoc(docId, "Text an agent saved meanwhile.", d1.currentVersion);
  await sleep(300);
  ctrlS();
  const banner = await waitFor(() => [...document.querySelectorAll(".error-banner")].find((b) => b.textContent?.includes("saved by someone else")) as HTMLElement | undefined, 3000);
  const keep = banner?.querySelector("button") as HTMLButtonElement | null;
  keep?.click();
  const d3 = await until((d) => d.currentVersion === v0 + 3);
  const kept = d3.bodyMd.includes("Probe two.") && d3.bodyMd.includes("Probe one.");
  return { ok: first && !!banner && kept, first_save: first, conflict_shown: !!banner, kept_mine: kept, versions: [v0, d1.currentVersion, d3.currentVersion] };
}

function typeInto(el: HTMLInputElement | HTMLTextAreaElement, text: string) {
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(el, text);
  el.dispatchEvent(new Event("input", { bubbles: true }));
}
const buttonByText = (root: ParentNode, text: string) =>
  [...root.querySelectorAll("button")].find((b) => b.textContent?.trim() === text) as HTMLButtonElement | undefined;

function pickOption(el: HTMLSelectElement, value: string) {
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(el, value);
  el.dispatchEvent(new Event("change", { bubbles: true }));
}
const q = <E extends Element = HTMLElement>(sel: string, root: ParentNode = document) => root.querySelector(sel) as E | null;
const column = (name: string) => q(`[data-column="${name}"]`);
const lineOf = (name: string) => column(name)?.querySelector(".wf-line")?.textContent ?? "";
const alertIn = (name: string) => column(name)?.querySelector("[role=alert]")?.textContent ?? "";
const pressed = (name: string, button: "Auto" | "Manual") =>
  buttonByText(column(name)?.querySelector(`[aria-label="Auto or Manual for ${name}"]`) ?? document, button)?.getAttribute("aria-pressed") === "true";
const keyOn = (el: HTMLElement, key: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key, code: key, bubbles: true, cancelable: true }));
/** Polls `f` (async) until it says yes, up to ~3 s. */
async function until(f: () => Promise<boolean> | boolean, tries = 30) {
  for (let i = 0; i < tries; i++) { if (await f()) return true; await sleep(100); }
  return false;
}
/** Drags `handle` onto the middle of `target` the way a mouse would (dnd-kit's pointer sensor). */
async function pointerDrag(handle: HTMLElement, target: HTMLElement, dy = 0) {
  const r = handle.getBoundingClientRect(), t = target.getBoundingClientRect();
  const x0 = r.left + r.width / 2, y0 = r.top + r.height / 2, x1 = t.left + t.width / 2, y1 = t.top + t.height / 2 + dy;
  handle.dispatchEvent(pe("pointerdown", x0, y0));
  for (let i = 1; i <= 30; i++) { document.dispatchEvent(pe("pointermove", x0 + ((x1 - x0) * i) / 30, y0 + ((y1 - y0) * i) / 30)); await nextFrame(); }
  for (let i = 0; i < 6; i++) { document.dispatchEvent(pe("pointermove", x1, y1 + i)); await nextFrame(); }
  document.dispatchEvent(pe("pointerup", x1, y1 + 5));
  // dnd-kit swallows the click that follows a drop for a moment; a mouse's next click comes later than that.
  await sleep(150);
}

/** Drags `handle` onto `target` the way a person would when the target is further down: towards where it is now, while
 * dnd-kit scrolls the page, and drops once the pointer has rested on it. */
async function dragOnto(handle: HTMLElement, target: () => HTMLElement | null) {
  const r = handle.getBoundingClientRect();
  let x = r.left + r.width / 2, y = r.top + r.height / 2;
  handle.dispatchEvent(pe("pointerdown", x, y));
  for (let i = 1; i <= 5; i++) { document.dispatchEvent(pe("pointermove", x, y + i * 2)); await nextFrame(); }
  y += 10;
  let still = 0;
  for (let i = 0; i < 300 && still < 5; i++) {
    const t = target()?.getBoundingClientRect();
    if (!t) break;
    const tx = t.left + t.width / 2, ty = Math.min(Math.max(t.top + t.height / 2, 10), window.innerHeight - 10);
    x += Math.sign(tx - x) * Math.min(Math.abs(tx - x), 25);
    y += Math.sign(ty - y) * Math.min(Math.abs(ty - y), 25);
    document.dispatchEvent(pe("pointermove", x, y));
    await nextFrame();
    const n = target()!.getBoundingClientRect();
    still = x >= n.left && x <= n.right && y >= n.top && y <= n.bottom && Math.abs(n.top - t.top) < 1 ? still + 1 : 0;
  }
  document.dispatchEvent(pe("pointerup", x, y));
  await sleep(150);
}

type ProbeTeam = { states: { id: string; name: string; category: string; sortKey: string; auto?: boolean; nextStateId?: string | null; agentIds?: string[] }[];
  members: { actorId: string; name: string; kind: string; roleKey: string; wakeup?: string | null; heartbeatMinutes?: number | null; model?: string | null; effort?: string | null;
    allowedTools?: string[] }[];
  branches?: { key: string; name: string; roles: string[] }[] };

/** GA-53, Team page against the demo data (no agents yet): the organisation chart's empty spot opens the agent form with the
 * branch's role and no wake-up; "+ Agent" and ×; Auto and Manual; the next column and the backend's refusals (a link to
 * itself, Auto without a next column); Review; Add column; dragging a column to a new place; the bin's confirm and removing a
 * column; a new label (and a name in use); a new branch. Then "New label…" in a task's Properties and in the New task drawer. */
export async function teamProbe(getTeam: () => Promise<ProbeTeam>) {
  const out: Record<string, unknown> = {};
  const fail = (step: string, extra: Record<string, unknown> = {}) => ({ ok: false, failed_at: step, ...out, ...extra });
  if (!(await waitFor(() => column("To do"), 6000))) return fail("no Workflow columns");
  const st = async (name: string) => (await getTeam()).states.find((s) => s.name === name);
  const idOf = async (name: string) => (await st(name))?.id ?? "";
  const order = async () => [...(await getTeam()).states].sort((a, b) => (a.sortKey < b.sortKey ? -1 : 1)).map((s) => s.name);

  // The organisation chart: Design first, one empty spot per branch.
  out.branches = [...document.querySelectorAll("[data-branch]")].map((b) => (b as HTMLElement).dataset.branch);
  out.one_spot_each = ["Design", "Development", "Quality", "Operations"].every((b) => document.querySelectorAll(`[aria-label="Add an agent to ${b}"]`).length === 1);

  // 1. The empty spot in Development opens the agent form with its role, without a wake-up.
  q("[aria-label='Add an agent to Development']")?.click();
  const dialog = await waitFor(() => q("[role=dialog]"));
  if (!dialog) return fail("the empty spot opened no agent form");
  out.spot_form = { name: q<HTMLInputElement>("#a-name", dialog)?.value, role: q<HTMLSelectElement>("#a-role", dialog)?.value,
    wake_up: !!q("input[aria-label=Minutes]", dialog) || (dialog.textContent ?? "").includes("Wakes up"),
    // GA-39's Tools section next to this card's Work section (merged in from main)
    sections: [...dialog.querySelectorAll("h3")].map((h) => h.textContent ?? "") };
  // GA-63: the role fills in its allowed commands like its instructions, and another role replaces both, until you edit
  // the list (here with a suggestion's + button); then a role change keeps it.
  const roleSel = q<HTMLSelectElement>("#a-role", dialog);
  const hasTool = (t: string) => (q<HTMLTextAreaElement>("#a-tools", dialog)?.value ?? "").split("\n").includes(t);
  const instructions = () => q("[aria-label=Instructions]", dialog)?.textContent ?? "";
  const isFrontend = () => hasTool("Bash(git push:*)") && !hasTool("Bash(gh pr create:*)") && instructions().includes("You are the Frontend Agent in Gizai's Software team.");
  const frontendFirst = !!(await waitFor(() => isFrontend() || null, 5000));
  if (roleSel) pickOption(roleSel, "qa");
  const qaNext = !!(await waitFor(() => (hasTool("Bash(gh pr create:*)") && instructions().includes("You are the QA Agent in Gizai's Software team.")) || null, 5000));
  if (roleSel) pickOption(roleSel, "frontend");
  const frontendAgain = !!(await waitFor(() => isFrontend() || null, 5000));
  buttonByText(dialog, "+ Bash(make test:*)")?.click();
  await sleep(100);
  if (roleSel) pickOption(roleSel, "qa");
  const qaInstructions = !!(await waitFor(() => instructions().includes("You are the QA Agent") || null, 5000));
  await sleep(300);
  const keptEdit = hasTool("Bash(make test:*)") && hasTool("Bash(git push:*)") && !hasTool("Bash(gh pr create:*)");
  if (roleSel) pickOption(roleSel, "frontend");
  await waitFor(() => instructions().includes("You are the Frontend Agent") || null, 5000);
  out.role_fills = { frontend_first: frontendFirst, qa_next: qaNext, frontend_again: frontendAgain, qa_instructions: qaInstructions, kept_edit: keptEdit };
  const modelSel = await waitFor(() => { const el = q<HTMLSelectElement>("#a-model", dialog); return el && !el.disabled && [...el.options].some((o) => o.value === "opus") ? el : null; }, 8000);
  if (modelSel) pickOption(modelSel, "opus");
  await sleep(100);
  const effortSel = q<HTMLSelectElement>("#a-effort", dialog);
  if (effortSel) pickOption(effortSel, "xhigh");
  await sleep(100);
  buttonByText(dialog, "Add agent")?.click();
  let agent: ProbeTeam["members"][number] | undefined;
  await until(async () => !!(agent = (await getTeam()).members.find((m) => m.name === "Frontend Agent" && m.kind === "agent")));
  if (!agent) return fail("the agent form added no agent");
  const id = agent.actorId;
  out.agent = { role: agent.roleKey, wakeup: agent.wakeup, heartbeat: agent.heartbeatMinutes, model: agent.model, effort: agent.effort,
    agent_page: !!(await waitFor(() => (q(".entity-head h1")?.textContent ?? "") === "Frontend Agent" || null, 3000)),
    columns: (await getTeam()).states.filter((s) => s.agentIds?.includes(id)).map((s) => s.name),
    added_once: (await getTeam()).members.filter((m) => m.name === "Frontend Agent" && m.kind === "agent").length === 1,
    // GA-63: saved with the frontend role's list and the command added in the form
    tools: agent.allowedTools?.includes("Bash(git push:*)") && agent.allowedTools.includes("Bash(make test:*)") && !agent.allowedTools.includes("Bash(gh pr create:*)") };
  window.location.hash = "#/team";
  if (!(await waitFor(() => column("Testing"), 4000))) return fail("back on the Team page: no columns");

  // 2. Drag its card from the organisation chart onto Testing; × takes it off. A drop on Review does nothing (it takes no
  //    agents). "+ Agent" puts it on Deploy; × again.
  const agentCard = () => q(`[data-agent="Frontend Agent"]`);
  const box = (el: Element | null) => { const r = el?.getBoundingClientRect(); return r ? [Math.round(r.top), Math.round(r.bottom)] : null; };
  out.layout = { height: window.innerHeight, card: box(agentCard()), testing: box(column("Testing")) };
  if (agentCard() && column("Testing")) await dragOnto(agentCard()!, () => column("Testing"));
  const dropped = await until(async () => !!(await st("Testing"))?.agentIds?.includes(id));
  (out.layout as Record<string, unknown>).landed = (await getTeam()).states.filter((x) => x.agentIds?.includes(id)).map((x) => x.name);
  (out.layout as Record<string, unknown>).alerts = [...document.querySelectorAll(".wf-err")].map((e) => e.textContent);
  const chip = await waitFor(() => q<HTMLElement>("[aria-label='Take Frontend Agent off Testing']"));
  chip?.click();
  const offTesting = await until(async () => !(await st("Testing"))?.agentIds?.includes(id));
  window.scrollTo(0, 0); q(".content")?.scrollTo(0, 0);
  await sleep(100);
  if (agentCard() && column("Review")) await dragOnto(agentCard()!, () => column("Review"));
  await sleep(300);
  const reviewEmpty = !(await st("Review"))?.agentIds?.length && !column("Review")?.querySelector(".agent-chip");
  q(`[aria-label="Add an agent to Deploy"]`, column("Deploy")!)?.click();
  const menu = await waitFor(() => q("[role=menu][aria-label='Agents for Deploy']"));
  (menu && [...menu.querySelectorAll("button.opt")].find((b) => b.textContent?.includes("Frontend Agent")) as HTMLElement | undefined)?.click();
  const added = await until(async () => !!(await st("Deploy"))?.agentIds?.includes(id));
  const chip2 = await waitFor(() => q<HTMLElement>("[aria-label='Take Frontend Agent off Deploy']"));
  const noAddOn = ["Backlog", "Review", "Done"].every((n) => !column(n)?.querySelector("[aria-label^='Add an agent to']") && !column(n)?.querySelector(".seg"));
  chip2?.click();
  const removed = await until(async () => !(await st("Deploy"))?.agentIds?.includes(id));
  out.plus_agent = { dragged_on: dropped, chip: !!chip, off: offTesting, review_takes_none: reviewEmpty, menu: !!menu, added, chip2: !!chip2, removed,
    none_on_backlog_review_done: noAddOn };

  // 3. Manual, then Auto again, with the line under the column.
  buttonByText(column("In progress")!, "Manual")?.click();
  const manual = await until(async () => (await st("In progress"))?.auto === false) && await until(() => pressed("In progress", "Manual"));
  const manualLine = lineOf("In progress");
  buttonByText(column("In progress")!, "Auto")?.click();
  const auto = await until(async () => (await st("In progress"))?.auto === true) && await until(() => pressed("In progress", "Auto"));
  out.auto_manual = { manual, manual_line: manualLine, auto, auto_line: lineOf("In progress") };

  // 4. The next column: a link to itself and Auto without a next column are refused with the backend's reason.
  const deployId = await idOf("Deploy"), doneId = await idOf("Done");
  const next = () => q<HTMLSelectElement>("[aria-label='Next column after Deploy']", column("Deploy")!);
  pickOption(next()!, deployId);
  await until(() => !!alertIn("Deploy"));
  const selfLink = alertIn("Deploy");
  pickOption(next()!, "");
  const cleared = await until(async () => !(await st("Deploy"))?.nextStateId);
  buttonByText(column("Deploy")!, "Auto")?.click();
  await until(() => !!alertIn("Deploy"));
  const autoWithout = alertIn("Deploy");
  const stayedManual = (await st("Deploy"))?.auto === false;
  pickOption(next()!, doneId);
  const linked = await until(async () => (await st("Deploy"))?.nextStateId === doneId);
  out.next_column = { self_link_refused: selfLink, cleared, auto_without_next_refused: autoWithout, stayed_manual: stayedManual, linked_to_done: linked,
    error_gone: !alertIn("Deploy") };

  // 5. Review: you review and merge, and where merged cards go.
  out.review = { you: (column("Review")?.textContent ?? "").includes("You review and merge"),
    merged_to: q<HTMLSelectElement>("[aria-label='Merged cards from Review go to']", column("Review")!)?.selectedOptions[0]?.textContent };

  // 6. Add column: a name, a kind in plain words and its place.
  buttonByText(document, "Add column")?.click();
  const form = await waitFor(() => q("[role=group][aria-label='Add column']"));
  if (!form) return fail("Add column opened nothing");
  typeInto(q<HTMLInputElement>("[aria-label='Column name']", form)!, "Design review");
  pickOption(q<HTMLSelectElement>("[aria-label='Kind of column']", form)!, "testing");
  pickOption(q<HTMLSelectElement>("[aria-label='After column']", form)!, await idOf("In progress"));
  await sleep(50);
  buttonByText(form, "Add column")?.click();
  const made = await until(async () => (await order()).join(",") === "Backlog,To do,In progress,Design review,Testing,Review,Deploy,Done");
  const newCol = await st("Design review");
  out.add_column = { made, category: newCol?.category, auto: newCol?.auto, shown: !!(await waitFor(() => column("Design review"), 2000)), order: await order() };

  // 7. Drag it by its grip below Testing.
  const grip = q("[aria-label='Move Design review']");
  if (grip && column("Testing")) await pointerDrag(grip, column("Testing")!, 4);
  const moved = await until(async () => (await order()).join(",") === "Backlog,To do,In progress,Testing,Design review,Review,Deploy,Done");
  const onScreen = [...document.querySelectorAll("[data-column]")].map((c) => (c as HTMLElement).dataset.column).join(",");
  out.reorder = { moved, order: await order(), on_screen: onScreen };

  // 8. The bin: off on the last Backlog and Done; the new column goes; Review's confirm names its card, the column before
  //    (the default) and the column that gets relinked.
  const bin = (n: string) => q<HTMLButtonElement>(`[aria-label='Remove ${n}']`, column(n)!);
  const binOff = (n: string) => ({ disabled: !!bin(n)?.disabled, why: bin(n)?.parentElement?.getAttribute("title") });
  // In progress is Auto with the Frontend Agent on it: when the agent works on one of its cards, its bin is off too.
  out.bins_off = { backlog: binOff("Backlog"), done: binOff("Done"), in_progress: binOff("In progress") };
  bin("Design review")?.click();
  const ask2 = await waitFor(() => q("[role=alertdialog][aria-label='Remove Design review']"));
  const ask2Text = ask2?.textContent;
  if (ask2) buttonByText(ask2, "Remove column")?.click();
  const gone = await until(async () => !(await st("Design review")));
  out.remove = { confirm: ask2Text, removed: gone, off_screen: !!(await until(() => !column("Design review"))) };
  bin("Review")?.click();
  const ask = await waitFor(() => q("[role=alertdialog][aria-label='Remove Review']"));
  const target = q<HTMLSelectElement>("[aria-label='Cards of Review go to']", ask ?? document);
  out.confirm = { text: ask?.textContent, default_target: target?.selectedOptions[0]?.textContent };
  if (ask) buttonByText(ask, "Keep")?.click();
  (out.confirm as Record<string, unknown>).kept = !!(await until(() => !q("[role=alertdialog][aria-label='Remove Review']"))) && !!(await st("Review"));

  // 8b. A second Done column turns the old Done's bin on; its confirm counts its archived card and offers the column before
  //     (the new one) by default. Then the new one goes again.
  const doneCard = (await listTasks()).find((t) => t.stateName === "Done");
  if (doneCard) await archiveTask(doneCard.id);
  buttonByText(document, "Add column")?.click();
  const form2 = await waitFor(() => q("[role=group][aria-label='Add column']"));
  if (form2) {
    typeInto(q<HTMLInputElement>("[aria-label='Column name']", form2)!, "Shipped");
    pickOption(q<HTMLSelectElement>("[aria-label='Kind of column']", form2)!, "done");
    pickOption(q<HTMLSelectElement>("[aria-label='After column']", form2)!, deployId);
    await sleep(50);
    buttonByText(form2, "Add column")?.click();
  }
  const shipped = await until(async () => (await st("Shipped"))?.category === "done") && !!(await waitFor(() => column("Shipped"), 2000));
  const doneOn = await until(() => !!bin("Done") && !bin("Done")!.disabled);
  bin("Done")?.click();
  const ask3 = await waitFor(() => q("[role=alertdialog][aria-label='Remove Done']"));
  out.done_confirm = { archived_one: !!doneCard, shipped, bin_on: doneOn, text: ask3?.textContent,
    default_target: q<HTMLSelectElement>("[aria-label='Cards of Done go to']", ask3 ?? document)?.selectedOptions[0]?.textContent };
  if (ask3) buttonByText(ask3, "Keep")?.click();
  await until(() => !q("[role=alertdialog][aria-label='Remove Done']"));
  bin("Shipped")?.click();
  const ask4 = await waitFor(() => q("[role=alertdialog][aria-label='Remove Shipped']"));
  if (ask4) buttonByText(ask4, "Remove column")?.click();
  (out.done_confirm as Record<string, unknown>).shipped_gone = await until(async () => !(await st("Shipped")));
  (out.done_confirm as Record<string, unknown>).done_kept = !!(await st("Done"));

  // 9. Labels: a new one, a name in use (any case) refused with the backend's reason, and removing one that a card has.
  const labels = () => q("[aria-label=Labels]");
  const newName = () => q<HTMLInputElement>("[aria-label='New label name']", labels()!);
  typeInto(newName()!, "Must have");
  buttonByText(labels()!, "Create label")?.click();
  const created = await until(async () => (await listLabels()).some((l) => l.name === "Must have" && l.cards === 0));
  const row = await waitFor(() => q("[data-label='Must have']"), 2000);
  typeInto(newName()!, "must HAVE");
  buttonByText(labels()!, "Create label")?.click();
  await until(() => !!labels()?.querySelector(".label-new [role=alert]"));
  const dup = labels()?.querySelector(".label-new [role=alert]")?.textContent;
  const bugCards = (await listLabels()).find((l) => l.name === "bug")?.cards;
  q("[aria-label='Remove bug']")?.click();
  const askLabel = await waitFor(() => q("[role=alertdialog][aria-label='Remove bug']"));
  const askLabelText = askLabel?.textContent;
  if (askLabel) buttonByText(askLabel, "Remove label")?.click();
  const labelGone = await until(async () => !(await listLabels()).some((l) => l.name === "bug"));
  out.labels = { created, row: row?.textContent, duplicate_refused: dup, only_one: (await listLabels()).filter((l) => l.name.toLowerCase() === "must have").length === 1,
    bug_cards: bugCards, remove_confirm: askLabelText, removed: labelGone };

  // 10. A new branch with its own empty spot; removed again while it has no agents. Development has an agent: no ×.
  (q(".org-add-branch") as HTMLElement | null)?.click();
  const branchName = await waitFor(() => q<HTMLInputElement>("[aria-label='Branch name']"));
  if (branchName) { typeInto(branchName, "Docs"); await sleep(50); keyOn(branchName, "Enter"); }
  const branchAdded = await until(async () => !!(await getTeam()).branches?.some((b) => b.name === "Docs"));
  const docs = await waitFor(() => q("[data-branch=Docs]"), 2000);
  const docsSpot = !!docs?.querySelector("[aria-label='Add an agent to Docs']");
  q<HTMLButtonElement>("[aria-label='Remove the Docs branch']")?.click();
  const branchRemoved = await until(async () => !(await getTeam()).branches?.some((b) => b.name === "Docs"));
  out.branch = { added: branchAdded, shown: !!docs, empty_spot: docsSpot, removed: branchRemoved,
    development_locked: !!q<HTMLButtonElement>("[aria-label='Remove the Development branch']")?.disabled,
    order: (await getTeam()).branches?.map((b) => b.name) };

  // 11. "New label…" in a task's Properties: Enter creates the label and puts it on the card.
  const task = (await listTasks()).find((t) => t.identifier === "KADE-1");
  if (!task) return fail("no KADE-1");
  window.location.hash = `#/task/${task.id}`;
  const labelsRow = await waitFor(() => [...document.querySelectorAll(".props .prop-row")].find((r) => r.querySelector(".k")?.textContent === "Labels") as HTMLElement | undefined, 4000);
  (labelsRow?.querySelector("[aria-haspopup]") as HTMLElement | null)?.click();
  const labelMenu = await waitFor(() => labelsRow?.querySelector("[role=menu]") as HTMLElement | null);
  (labelMenu?.querySelector(".new-label") as HTMLElement | null)?.click();
  const propsInput = await waitFor(() => labelsRow?.querySelector("[aria-label='New label name']") as HTMLInputElement | null);
  if (propsInput) { typeInto(propsInput, "Could have"); await sleep(50); keyOn(propsInput, "Enter"); }
  const onCard = await until(async () => (await getTask(task.id)).labels.some((l) => l.name === "Could have"));
  out.props_new_label = { menu: !!labelMenu, on_card: onCard, kept_others: (await getTask(task.id)).labels.map((l) => l.name) };

  // 12. "New label…" in the New task drawer: Enter creates it and turns it on; the new card has it.
  [...document.querySelectorAll(".side button")].find((b) => b.textContent?.startsWith("New task"))?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  const drawer = await waitFor(() => q("#t-title")?.closest("[role=dialog]") as HTMLElement | null, 3000);
  let drawerCard = false, pill = false;
  if (drawer) {
    typeInto(q<HTMLInputElement>("#t-title", drawer)!, "Probe card with a new label");
    (drawer.querySelector(".new-label") as HTMLElement | null)?.click();
    const input = await waitFor(() => drawer.querySelector("[aria-label='New label name']") as HTMLInputElement | null);
    if (input) { typeInto(input, "Should have"); await sleep(50); keyOn(input, "Enter"); }
    pill = !!(await waitFor(() => [...drawer.querySelectorAll(".label-pill[aria-pressed=true]")].some((b) => b.textContent?.includes("Should have")) || null, 2000));
    buttonByText(drawer, "Create task")?.click();
    drawerCard = await until(async () => (await listTasks()).some((t) => t.title === "Probe card with a new label" && t.labels.some((l) => l.name === "Should have")));
  }
  out.drawer_new_label = { drawer: !!drawer, pill_on: pill, card_has_it: drawerCard };

  const a = out.agent as { role?: string; wakeup?: string | null; model?: string | null; effort?: string | null; agent_page?: boolean; columns?: string[]; added_once?: boolean; tools?: boolean };
  const sf = out.spot_form as { name?: string; role?: string; wake_up?: boolean; sections?: string[] };
  const pa = out.plus_agent as Record<string, boolean>, am = out.auto_manual as { manual: boolean; auto: boolean; manual_line: string; auto_line: string };
  const nc = out.next_column as { self_link_refused: string; cleared: boolean; auto_without_next_refused: string; stayed_manual: boolean; linked_to_done: boolean; error_gone: boolean };
  const ac = out.add_column as { made: boolean; category?: string; auto?: boolean; shown: boolean };
  const bo = out.bins_off as { backlog: { disabled: boolean; why?: string | null }; done: { disabled: boolean; why?: string | null } };
  const cf = out.confirm as { text?: string | null; default_target?: string | null; kept: boolean };
  const rm = out.remove as { confirm?: string | null; removed: boolean; off_screen: boolean };
  const lb = out.labels as { created: boolean; duplicate_refused?: string | null; only_one: boolean; remove_confirm?: string | null; removed: boolean };
  const br = out.branch as { added: boolean; shown: boolean; empty_spot: boolean; removed: boolean; development_locked: boolean };
  const checks: Record<string, boolean> = {
    design_first: (out.branches as string[]).slice(0, 4).join(",") === "Design,Development,Quality,Operations",
    one_spot_each: !!out.one_spot_each,
    spot_form: sf.name === "Frontend Agent" && sf.role === "frontend" && !sf.wake_up && sf.sections?.join(",") === "Agent,Chat,Work,Permissions,Tools,Instructions",
    agent: a.role === "frontend" && a.wakeup !== "heartbeat" && !!a.agent_page && a.model === "opus" && a.effort === "xhigh" && a.columns?.join(",") === "To do,In progress"
      && !!a.added_once && !!a.tools,
    role_fills: Object.values(out.role_fills as Record<string, boolean>).every(Boolean),
    plus_agent: Object.values(pa).every(Boolean),
    auto_manual: am.manual && am.auto && am.manual_line.startsWith("Manual: press Run on a card to start Frontend Agent") && am.auto_line.startsWith("Auto: Frontend Agent takes cards"),
    next_column: !!nc.self_link_refused && nc.cleared && !!nc.auto_without_next_refused && nc.stayed_manual && nc.linked_to_done && nc.error_gone,
    review: (out.review as { you: boolean; merged_to?: string }).you && (out.review as { merged_to?: string }).merged_to === "Deploy",
    add_column: ac.made && ac.category === "testing" && ac.auto === false && ac.shown,
    reorder: !!(out.reorder as { moved: boolean }).moved,
    bins_off: bo.backlog.disabled && !!bo.backlog.why?.includes("last Backlog") && bo.done.disabled && !!bo.done.why?.includes("last Done"),
    confirm: !!cf.text?.includes("Its card goes to") && cf.default_target === "Testing (the column before)" && !!cf.text?.includes("Testing links to Deploy instead.") && cf.kept,
    remove: !!rm.confirm?.includes("It has no cards.") && rm.removed && rm.off_screen,
    done_confirm: (() => { const d = out.done_confirm as { archived_one: boolean; shipped: boolean; bin_on: boolean; text?: string | null; default_target?: string | null; shipped_gone: boolean; done_kept: boolean };
      return d.archived_one && d.shipped && d.bin_on && !!d.text?.includes("Its archived card goes to") && d.default_target === "Shipped (the column before)" && d.shipped_gone && d.done_kept; })(),
    labels: lb.created && !!lb.duplicate_refused && lb.only_one && !!lb.remove_confirm?.includes("1 card loses it.") && lb.removed,
    branch: br.added && br.shown && br.empty_spot && br.removed && br.development_locked,
    props_new_label: !!(out.props_new_label as { on_card: boolean }).on_card,
    drawer_new_label: (out.drawer_new_label as { pill_on: boolean; card_has_it: boolean }).pill_on && (out.drawer_new_label as { card_has_it: boolean }).card_has_it,
  };
  const failed = Object.entries(checks).filter(([, v]) => !v).map(([k]) => k);
  return { ok: failed.length === 0, failed, ...out };
}

/** Task page Run panel against the fake Claude Code: Run (it hangs), see it live, Stop, then Run again
 * without the hang and watch the card move to Testing. */
export async function runProbe(taskId: string, api: {
  getTask: (id: string) => Promise<{ stateName: string }>;
  clearHang: (id: string) => Promise<void>;
  lastRunStatus: (id: string) => Promise<string | undefined>;
}) {
  const runButton = () => [...document.querySelectorAll(".run-card button")].find((b) => b.textContent?.trim() === "Run") as HTMLButtonElement | undefined;
  const first = await waitFor(runButton, 6000);
  if (!first) return { ok: false, error: "no Run button" };
  first.click();
  const live = await waitFor(() => document.querySelector(".run-card.live") as HTMLElement | null, 6000);
  const started = !!(await waitFor(() => (document.querySelector(".run-card.live .run-stream")?.textContent ?? "").includes("Started") || null, 4000));
  buttonByText(document.querySelector(".run-card.live") ?? document, "Stop")?.click();
  let stopped: string | undefined;
  for (let i = 0; i < 60 && stopped !== "cancelled"; i++) { await sleep(100); stopped = await api.lastRunStatus(taskId); }
  await api.clearHang(taskId);
  const again = await waitFor(() => (!document.querySelector(".run-card.live") ? runButton() : undefined), 6000);
  again?.click();
  let state = "";
  for (let i = 0; i < 80 && state !== "Testing"; i++) { await sleep(100); state = (await api.getTask(taskId)).stateName; }
  const shown = !!(await waitFor(() => (document.querySelector(".run-card")?.textContent ?? "").includes("Ready for testing") || null, 4000));
  const ok = !!live && started && stopped === "cancelled" && state === "Testing" && shown;
  return { ok, live_panel: !!live, stream_started: started, stopped, state, result_shown: shown };
}

/** Chat page against the fake Claude Code: the setup panel opens the agent form with Chat on, saving it
 * makes the Team Lead, then a message streams an answer whose tool card links the task it created. */
export async function chatProbe(listTitles: () => Promise<string[]>) {
  const setup = await waitFor(() => document.querySelector(".chat-setup .btn.primary") as HTMLButtonElement | null, 6000);
  if (!setup) return { ok: false, error: "no setup panel" };
  setup.click();
  const dialog = await waitFor(() => document.querySelector("[role=dialog]") as HTMLElement | null);
  if (!dialog) return { ok: false, error: "the agent form did not open" };
  const onTeam = window.location.hash.startsWith("#/team");
  const name = (dialog.querySelector("#a-name") as HTMLInputElement | null)?.value;
  const chatOn = !!(dialog.querySelector("#a-chat") as HTMLInputElement | null)?.checked;
  await waitFor(() => (dialog.querySelector(".cm-content")?.textContent ?? "").includes("Chat page") || null, 3000);
  buttonByText(dialog, "Add agent")?.click();
  const agentPage = !!(await waitFor(() => (document.querySelector(".entity-head h1")?.textContent ?? "") === "Team Lead" || null, 4000));
  window.location.hash = "#/chat";
  // GA-41: the text box is a Markdown editor without a toolbar.
  const box = await waitFor(composer, 4000);
  if (!box) return { ok: false, error: "no composer", name, chatOn, onTeam, agentPage };
  typeIn(box, "create task Chat probe task in KADE");
  await sleep(50);
  enter(box);
  const card = await waitFor(() => document.querySelector('.tool-card a[href^="#/task/"]') as HTMLAnchorElement | null, 15000);
  const reply = !!(await waitFor(() => [...document.querySelectorAll(".chat-msg.agent")].some((e) => (e.textContent ?? "").includes("Done:")) || null, 15000));
  const made = (await listTitles()).includes("Chat probe task");
  const thread = window.location.hash.startsWith("#/chat/");
  const listed = !!document.querySelector(".chat-threads .th.on");
  const first = onTeam && name === "Team Lead" && chatOn && agentPage && !!card && reply && made && thread && listed;
  const runsOn = await runsOnAndQueueProbe();
  const linksAndFiles = await chatPickerAndFilesProbe();
  const ok = first && runsOn.ok && linksAndFiles.ok;
  return { ok, on_team: onTeam, name, chat_on: chatOn, agent_page: agentPage, tool_card: card?.textContent, reply, task_made: made, thread_url: thread,
    thread_listed: listed, runs_on: runsOn, links_and_files: linksAndFiles };
}

const enter = (el: HTMLElement) => el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, bubbles: true, cancelable: true }));
const composer = () => document.querySelector(".composer-box .cm-content") as HTMLElement | null;
/** Types into a CodeMirror editor the way a keyboard would: focused, at the cursor. */
function typeIn(el: HTMLElement, text: string) {
  el.focus();
  document.execCommand("insertText", false, text);
}
const sentCount = () => document.querySelectorAll(".chat-msg.user:not(.queued)").length;

/** GA-41 in a chat that is not answering: + opens a menu upward with Add files and Link an item; Link an item types @ and
 * opens the picker above the text box with the kinds; @task. lists tasks; Enter links one without sending; Shift+Enter adds
 * a line; Enter sends, and the sent message shows the link as a chip. Then files: Tauri's drag-and-drop events (sent
 * through Tauri's event system, as the native drop sends them; a real OS drop can't be made headless) show the drop state
 * and add the file as a removable chip, refusing a folder with why; sending sends the file, the message shows it and the
 * Team Lead's prompt names it; after reopening the chat the chips and the file are still there. */
async function chatPickerAndFilesProbe() {
  const idle = await waitFor(() => { const c = composer(); return c && !document.querySelector(".composer-box .stop-btn") ? c : null; }, 15000);
  const plus = document.querySelector(".composer-box .composer-plus") as HTMLButtonElement | null;
  if (!idle || !plus) return { ok: false, error: "no idle composer or no + button" };
  const threadId = window.location.hash.slice("#/chat/".length);
  plus.click();
  const menu = await waitFor(() => document.querySelector('.composer-box .pop.up[role="menu"]') as HTMLElement | null, 2000);
  const items = menu ? textsOf("button", menu) : [];
  const menuUp = !!menu && menu.getBoundingClientRect().bottom <= plus.getBoundingClientRect().top + 1;
  if (menu) buttonByText(menu, "Link an item")?.click();
  const kinds = await pickerReady((r) => r.length === 6);
  const menuClosed = !document.querySelector('.composer-box .pop[role="menu"]');
  const content = composer()!;
  const typedAt = docOf(content) === "@";
  const pk = document.querySelector(".item-picker")?.getBoundingClientRect();
  const pickerUp = !!pk && pk.bottom <= content.getBoundingClientRect().bottom && pk.top < content.getBoundingClientRect().top;
  typeIn(content, "task.");
  const tasks = await pickerReady(onlyTasks);
  const before = sentCount();
  enter(content);
  await sleep(300);
  const linked = docOf(content);
  const enterLinks = /^\[[A-Z][A-Z0-9]*-\d+ - [^\]]+\]\(gizai:task\/[A-Z][A-Z0-9]*-\d+\) $/.test(linked) && sentCount() === before;
  const chipInBox = !!content.querySelector(".cm-item-chip");
  press(content, "Enter", 13, { shiftKey: true });
  typeIn(content, "please look");
  await sleep(100);
  const twoLines = docOf(content).split("\n").length === 2 && sentCount() === before;
  enter(content);
  const sentChip = await waitFor(() => [...document.querySelectorAll(".chat-msg.user:not(.queued) .bubble a.item-chip.kind-task")].pop() as HTMLElement | null, 4000);
  const cleared = !!(await waitFor(() => (docOf(composer()!) === "" ? true : null), 3000));
  await waitFor(() => (!document.querySelector(".composer-box .stop-btn") && !document.querySelector(".chat-working") ? true : null), 15000);

  // Files, dropped the way Tauri reports a native drop: physical pixels over the Chat page.
  const data = (await appInfo()).data_dir;
  const file = `${data}/gizai.db`;
  const main = document.querySelector(".chat-main")!.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const position = { x: Math.round((main.left + main.width / 2) * dpr), y: Math.round((main.top + main.height / 3) * dpr) };
  let emitError: string | null = null;
  try {
    await emit("tauri://drag-enter", { paths: [file, data], position });
    await emit("tauri://drag-over", { position });
  } catch (e) { emitError = String(e); }
  if (emitError) return { ok: false, error: `couldn't send Tauri's drag events: ${emitError}` };
  const dropState = textOf(await waitFor(() => document.querySelector(".chat-main.dropping .chat-drop"), 2000));
  await emit("tauri://drag-drop", { paths: [file, data], position });
  const chips = await waitFor(() => { const n = textsOf(".composer-box .files.compact li b"); return n.length ? n : null; }, 4000);
  const dropGone = !document.querySelector(".chat-drop");
  const refused = textOf(await waitFor(() => document.querySelector(".chat-composer .chat-banner.warn"), 2000));
  (document.querySelector('.composer-box button[aria-label="Remove gizai.db"]') as HTMLButtonElement | null)?.click();
  const removed = !!(await waitFor(() => (!document.querySelector(".composer-box .files.compact") ? true : null), 2000));
  await emit("tauri://drag-enter", { paths: [file], position });
  await emit("tauri://drag-drop", { paths: [file], position });
  const again = await waitFor(() => { const n = textsOf(".composer-box .files.compact li b"); return n.length ? n : null; }, 4000);
  typeIn(composer()!, "here is a file");
  enter(composer()!);
  const sentFile = await waitFor(() => [...document.querySelectorAll(".chat-msg.user:not(.queued)")].pop()?.querySelector(".msg-files b") ?? null, 5000);
  const chipsCleared = !!(await waitFor(() => (!document.querySelector(".composer-box .files.compact") ? true : null), 3000));
  await waitFor(() => (!document.querySelector(".composer-box .stop-btn") && !document.querySelector(".chat-working") ? true : null), 15000);
  const saved = (await chatMessages(threadId)).filter((m) => m.role === "user").pop();

  // The text box grows with its lines up to 200 px, then scrolls; emptied again by hand.
  const box = composer()!;
  const cm = () => document.querySelector(".composer-editor .cm-editor") as HTMLElement;
  const oneLine = cm().getBoundingClientRect().height;
  typeIn(box, "line");
  for (let i = 0; i < 14; i++) { press(box, "Enter", 13, { shiftKey: true }); typeIn(box, "line"); }
  await sleep(200);
  const tall = cm().getBoundingClientRect().height;
  const scroller = document.querySelector(".composer-editor .cm-scroller") as HTMLElement;
  const grows = oneLine < 60 && tall > 150 && tall <= 201 && scroller.scrollHeight > scroller.clientHeight + 20 && sentCount() === before + 2;
  const view = EditorView.findFromDOM(box)!;
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: "" } });

  // While the Team Lead is paused: the text box, + and drops are off.
  const leadId = (await listChatThreads()).find((t) => t.id === threadId)?.agentId ?? "";
  await setAgentStatus(leadId, "paused");
  const off = !!(await waitFor(() => document.querySelector(".composer-box.disabled"), 4000));
  const plusOff = !!(document.querySelector(".composer-box .composer-plus") as HTMLButtonElement | null)?.disabled;
  (document.querySelector(".composer-box .composer-plus") as HTMLElement | null)?.click();
  await sleep(150);
  const noMenu = !document.querySelector('.composer-box .pop[role="menu"]');
  await emit("tauri://drag-enter", { paths: [file], position });
  await emit("tauri://drag-over", { position });
  await sleep(300);
  const noDropState = !document.querySelector(".chat-drop");
  await emit("tauri://drag-drop", { paths: [file], position });
  await sleep(500);
  const noDrop = !document.querySelector(".composer-box .files.compact");
  await setAgentStatus(leadId, "active");
  const on = !!(await waitFor(() => (document.querySelector(".composer-box") && !document.querySelector(".composer-box.disabled") ? true : null), 4000));
  const pausedOk = off && plusOff && noMenu && noDropState && noDrop && on;

  // Reopened: the link and the file are still shown.
  window.location.hash = "#/chat";
  await waitFor(() => (!document.querySelector(".chat-msg.user") ? true : null), 3000);
  window.location.hash = `#/chat/${threadId}`;
  const reopenedChip = await waitFor(() => document.querySelector(".chat-msg.user .bubble a.item-chip.kind-task") as HTMLElement | null, 5000);
  const reopenedFile = textOf(await waitFor(() => document.querySelector(".chat-msg.user .msg-files b"), 5000));
  // A click on the chip opens the task.
  reopenedChip?.click();
  const opened = await waitFor(() => (window.location.hash.startsWith("#/task/") ? window.location.hash : null), 3000);

  const ok = JSON.stringify(items) === JSON.stringify(["Add files", "Link an item"]) && menuUp && menuClosed && JSON.stringify(kinds) === JSON.stringify(KINDS)
    && typedAt && pickerUp && !!tasks && enterLinks && chipInBox && twoLines && !!sentChip && cleared
    && dropState === "Drop to add to this message" && JSON.stringify(chips) === '["gizai.db"]' && dropGone && refused.includes("is a folder, not a file") && removed
    && JSON.stringify(again) === '["gizai.db"]' && textOf(sentFile) === "gizai.db" && chipsCleared && saved?.files?.[0]?.name === "gizai.db"
    && !!reopenedChip && reopenedFile === "gizai.db" && !!opened && grows && pausedOk;
  return { ok, grows: { ok: grows, one_line: oneLine, tall }, paused: { ok: pausedOk, off, plus_off: plusOff, no_menu: noMenu, no_drop_state: noDropState,
    no_drop: noDrop, on_again: on }, menu: items, menu_up: menuUp, menu_closed: menuClosed, kinds, typed_at: typedAt, picker_up: pickerUp, tasks: tasks?.slice(0, 3),
    enter_links_without_sending: enterLinks, chip_in_box: chipInBox, shift_enter_new_line: twoLines, sent_chip: sentChip?.textContent, cleared,
    drop_state: dropState, chips, drop_state_gone: dropGone, refused, removed, dropped_again: again, sent_file: textOf(sentFile), chips_cleared: chipsCleared,
    saved_files: saved?.files?.map((f) => f.name), reopened_chip: reopenedChip?.textContent, reopened_file: reopenedFile, chip_opens: opened };
}
const picker = () => document.querySelector('.composer-foot select[aria-label="Runs on"]') as HTMLSelectElement | null;
const userSaid = (text: string) => [...document.querySelectorAll(".chat-msg.user:not(.queued)")].some((e) => (e.textContent ?? "").includes(text));

/** GA-50, in a chat that has answered once: Runs on under the text box (right of the hints) lists the Claude Code
 * accounts and shows Codex disabled with why; picking Claude Code 2 saves it on the chat. Then, while a slow answer is
 * written, the picker is disabled and Enter queues a message, which shows as queued and goes by itself afterwards. */
async function runsOnAndQueueProbe() {
  // The first answer's text shows before its turn has ended: Runs on can change once it has.
  const sel = await waitFor(() => { const p = picker(); return p && !p.disabled && !document.querySelector(".composer-box .stop-btn") ? p : null; }, 8000);
  if (!sel) return { ok: false, error: "no usable Runs on under the text box", disabled: picker()?.disabled };
  const hints = document.querySelector(".composer-foot .composer-hint")?.getBoundingClientRect();
  const right = !!hints && sel.getBoundingClientRect().left > hints.right;
  const options = [...sel.options].map((o) => ({ name: o.textContent ?? "", value: o.value, disabled: o.disabled }));
  const onLead = sel.value === "claude_code" && !sel.disabled;
  const cc2 = options.find((o) => o.name === "Claude Code 2" && !o.disabled);
  const codexOff = options.some((o) => o.name.startsWith("Codex") && o.disabled && o.name.includes("the chat runs on Claude Code only"));
  sel.focus();
  sel.click();
  if (cc2) pickOption(sel, cc2.value);
  const threadId = window.location.hash.slice("#/chat/".length);
  let saved = false;
  for (let i = 0; i < 40 && !saved && cc2; i++) { await sleep(100); saved = (await listChatThreads()).find((t) => t.id === threadId)?.cli === cc2.value; }
  const shows = picker()?.value === cc2?.value;

  const box = composer();
  if (!box) return { ok: false, error: "no composer" };
  typeIn(box, "FAKE_CHAT_SLOW what is next?");
  await sleep(50);
  enter(box);
  const answering = !!(await waitFor(() => document.querySelector(".composer-box .stop-btn"), 4000));
  const locked = !!picker()?.disabled;
  const box2 = composer();
  if (box2) { typeIn(box2, "and one more thing"); await sleep(50); enter(box2); }
  const queued = !!(await waitFor(() => [...document.querySelectorAll(".chat-queue .chat-msg.queued")].find((e) =>
    (e.textContent ?? "").includes("and one more thing") && (e.textContent ?? "").includes("Queued: goes when this answer is done")) || null, 3000));
  const notYet = !userSaid("and one more thing");
  const went = !!(await waitFor(() => (!document.querySelector(".chat-queue") && userSaid("and one more thing") ? true : null), 15000));
  const done = !!(await waitFor(() => (!document.querySelector(".composer-box .stop-btn") ? true : null), 10000));
  const note = [...document.querySelectorAll(".chat-note")].some((e) => (e.textContent ?? "").includes("Now on Claude Code 2."));
  const ok = right && onLead && !!cc2 && codexOff && saved && shows && answering && locked && queued && notYet && went && done && note;
  return { ok, right_of_hints: right, on_lead: onLead, options, codex_disabled: codexOff, saved, shows, answering, locked_while_answering: locked,
    queued, not_sent_yet: notYet, went_after: went, answer_done: done, switch_note: note };
}

const DAY = 86_400_000;
const textOf = (el: Element | null | undefined) => (el?.textContent ?? "").trim();
const textsOf = (sel: string, root: ParentNode = document) => [...root.querySelectorAll(sel)].map((e) => textOf(e));
const cellsOf = (tr: Element) => [...tr.querySelectorAll("td")].map((td) => textOf(td));

/** GA-33, against prep_usage's runs (today: $0.57 of the Backend Agent on KADE and GFW, a Codex run on KADE with tokens but
 * no cost, a Team Lead chat turn of $0.03; 20 days ago: $1.00 on KADE). Company lists Usage above Team and it is the page
 * shown. GA-62: it opens on Subscription, the first tab, with a block per coding CLI entry and the limits prep_usage kept
 * (Claude Code, Claude Code 2, Codex, and Gemini, which can't be read) and the agents on each, no period switch and no
 * table cut off. Then Total: the period switch and the labels; today's numbers on each tab, which agree with each other;
 * 30 days takes in the older run; then the Projects list's AI usage column, sorted by a click on its header. */
export async function usageProbe() {
  const company = [...document.querySelectorAll(".side .nav-section")].find((s) => textOf(s.querySelector(".nav-label")) === "Company");
  const companyItems = company ? textsOf("a.nav-item", company) : [];
  const usageOn = textOf(company?.querySelector('a.nav-item[aria-current="page"]')) === "Usage";
  if (!(await waitFor(() => q(".usage-tab"), 6000))) return { ok: false, error: "the Usage page shows no tab", company: companyItems, page: textOf(q(".main")).slice(0, 300) };

  const tabs = textsOf(".tabs button.tab");
  const selectedTab = () => textOf(q('.tabs button.tab[aria-selected="true"]'));
  const pressedChip = () => textOf(q('[aria-label="Period"] .chip[aria-pressed="true"]'));
  const shownDays = () => textOf(q(".topbar .crumbs .faint"));
  const now = new Date();
  const today = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
  const sinceOf: Record<string, number> = { Today: today, "7 days": today - 6 * DAY, "30 days": today - 29 * DAY, "This month": Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1) };
  const daysOf = (label: string) => periodDays(sinceOf[label], today + DAY);
  const period = async (label: string) => {
    buttonByText(q('[aria-label="Period"]') ?? document, label)?.click();
    return !!(await waitFor(() => (pressedChip() === label && shownDays() === daysOf(label) ? true : null), 4000));
  };
  const tab = async (label: string) => {
    buttonByText(q(".tabs") ?? document, label)?.click();
    return !!(await waitFor(() => (selectedTab() === label ? true : null), 2000));
  };
  const table = (label: string) => q(`table[aria-label="${label}"]`);
  const rows = (label: string) => [...(table(label)?.querySelectorAll("tbody tr") ?? [])].map(cellsOf);
  const total = (label: string) => [...(table(label)?.querySelectorAll("tfoot td") ?? [])].map((td) => textOf(td));
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

  // GA-62: the page opens on Subscription, the first tab, with a block per coding CLI entry (prep_usage's four) and no period
  // switch. Each block shows its own limits, or why it has none; no table is cut off at the block's edge.
  const blocks = (await waitFor(() => {
    const all = [...document.querySelectorAll(".limits .limits-block")];
    return all.length >= 4 ? all : null;
  }, 4000)) ?? [];
  const blockOf = (name: string) => blocks.find((b) => b.getAttribute("aria-label") === name);
  const limitRows = (name: string) => [...(blockOf(name)?.querySelectorAll("tbody tr") ?? [])].map(cellsOf);
  const subscription = {
    tab: selectedTab(), chips: document.querySelectorAll('[aria-label="Period"]').length, days: shownDays(),
    blocks: blocks.map((b) => b.getAttribute("aria-label")),
    claude: limitRows("Claude Code"), claude2: limitRows("Claude Code 2"), codex: limitRows("Codex"),
    gemini: textOf(blockOf("Gemini")), gemini_tables: blockOf("Gemini")?.querySelectorAll("table").length,
    agents: blocks.map((b) => textsOf(".limits-agent", b)),
    chats: textsOf(".limits-note", blockOf("Claude Code 2") ?? document),
    states: blocks.map((b) => [...b.querySelectorAll("tbody tr")].map((tr) => tr.className)),
    clipped: blocks.filter((b) => b.scrollWidth > b.clientWidth + 1).map((b) => b.getAttribute("aria-label")),
    foot: textOf(q(".usage-foot")),
  };
  const asOf = (r: string[] | undefined) => !!r && r[3]?.startsWith("as of ") === true;
  const subscriptionOk = subscription.tab === "Subscription" && subscription.chips === 0 && subscription.days === ""
    && same(subscription.blocks, ["Claude Code", "Claude Code 2", "Codex", "Gemini"])
    && same(subscription.claude.map((r) => r.slice(0, 2)), [["Session limit", "42%"], ["Weekly limit", "85%"], ["Fable limit", "Not reported yet"]])
    && asOf(subscription.claude[0]) && asOf(subscription.claude[1]) && subscription.claude[0]?.[2] !== "" && subscription.claude[2]?.[3] === ""
    && same(subscription.claude2.map((r) => r.slice(0, 3)), [["Session limit", "Limit reached", "3pm (Europe/Amsterdam)"], ["Weekly limit", "35%", subscription.claude2[1]?.[2]], ["Fable limit", "Not reported yet", ""]])
    && asOf(subscription.claude2[0]) && asOf(subscription.claude2[1])
    && same(subscription.codex.map((r) => r.slice(0, 2)), [["5-hour limit", "24%"], ["Weekly limit", "41%"]]) && asOf(subscription.codex[0])
    && subscription.gemini.includes("Gizai can't read Gemini's limits yet.") && subscription.gemini_tables === 0
    && same(subscription.agents, [["Backend Agent (paused)"], ["Team Lead (paused)"], ["Codex Agent (paused)"], []])
    && subscription.chats.includes("The Team Lead's chat runs here.")
    && same(subscription.states, [["limit-ok", "limit-near", "limit-unread"], ["limit-reached", "limit-ok", "limit-unread"], ["limit-ok", "limit-ok"], []])
    && subscription.clipped.length === 0 && subscription.foot.includes("it never asks Anthropic or OpenAI");

  // Then the Total tab, with the period switch.
  const toTotal = await tab("Total");
  await sleep(200);
  const chips = textsOf('[aria-label="Period"] .chip');
  const first = { tab: selectedTab(), period: pressedChip(), days: shownDays(), want_days: daysOf("This month"), bars: q(".usage-bars")?.children.length,
    want_bars: now.getUTCDate() };
  const labels = textsOf(".usage-tab .stat-card h4");
  const subs = textsOf(".usage-tab .stat-card .sub");
  const foot = textOf(q(".usage-foot"));
  const labelled = labels.includes("API cost") && labels.includes("Input tokens (incl. cache)") && labels.includes("Output tokens")
    && subs.includes("An estimate at API prices, not a bill") && foot.includes("API cost: what these tokens would cost at API prices")
    && foot.includes("Input tokens include cache reads and writes");

  // Today, on each tab: [name, runs, input tokens, output tokens, API cost, share].
  const toToday = await period("Today");
  await sleep(300);
  const metrics = textsOf(".usage-tab .usage-metric");
  const unknownLine = textOf(q(".usage-tab .stat-card .sub.usage-unknown"));
  const totalOk = same(metrics, ["$0.60", "23K", "4K", "4"]) && unknownLine === "+ an unknown cost for 1 run";
  const wantTotal = ["Total", "3 runs · 1 chat turn", "23K", "4K", "$0.60 + unknown", ""];
  await tab("Agents");
  const agents = rows("Usage per agent");
  const agentsTotal = total("Usage per agent");
  const agentsOk = same(agents, [
    ["Backend Agent", "2 runs", "16K", "3.4K", "$0.57", "95%"],
    ["Team Lead", "1 chat turn", "2K", "100", "$0.03", "5%"],
    ["Codex Agent", "1 run", "5K", "500", "Unknown", "0%"],
  ]) && same(agentsTotal, wantTotal);
  await tab("Projects");
  const projects = rows("Usage per project");
  const projectsTotal = total("Usage per project");
  const projectsOk = same(projects, [
    ["Kade portalKADE", "2 runs", "17K", "3.5K", "$0.42 + unknown", "70%"],
    ["Groene Fiets webshopGFW", "1 run", "4K", "400", "$0.15", "25%"],
    ["Chat (no project)", "1 chat turn", "2K", "100", "$0.03", "5%"],
  ]) && same(projectsTotal, wantTotal);
  // The other periods: the dates change; 30 days takes in the run of 20 days ago.
  const to7 = await period("7 days");
  const to30 = await period("30 days");
  await sleep(300);
  const projects30 = rows("Usage per project");
  const total30 = total("Usage per project");
  const thirtyOk = projects30[0]?.[0] === "Kade portalKADE" && projects30[0]?.[4] === "$1.42 + unknown" && same(total30, ["Total", "4 runs · 1 chat turn", "24K", "4.1K", "$1.60 + unknown", ""]);
  const toMonth = await period("This month");
  const periodsOk = toToday && to7 && to30 && toMonth;

  // The Projects list: AI usage this month, sortable.
  window.location.hash = "#/projects";
  const th = await waitFor(() => [...document.querySelectorAll("table.grid th")].find((h) => textOf(h).startsWith("AI usage")) as HTMLElement | undefined, 4000);
  if (!th) return { ok: false, error: "no AI usage column on the Projects list", company: companyItems, first, labelled, totalOk, agents, projects };
  const heads = textsOf("table.grid thead th");
  const at = heads.findIndex((h) => h.startsWith("AI usage")), nameAt = heads.findIndex((h) => h.startsWith("Project"));
  const list = () => [...document.querySelectorAll("table.grid tbody tr")].map(cellsOf).filter((c) => c.length === heads.length).map((c) => [c[nameAt], c[at]]);
  const oldInMonth = new Date(Date.now() - 20 * DAY).getUTCMonth() === now.getUTCMonth();
  const kade = list().find(([n]) => n.startsWith("Kade portal"))?.[1], gfw = list().find(([n]) => n.startsWith("Groene Fiets"))?.[1];
  const column = { kade, gfw, title: th.title, footer: textOf(q(".tablefoot")) };
  const columnOk = kade === (oldInMonth ? "$1.42 + unknown" : "$0.42 + unknown") && gfw === "$0.15" && th.title.includes("API cost this month")
    && column.footer.includes("AI usage: the API cost this month, an estimate at API prices, not a bill");
  const sorted = async () => {
    th.click();
    await sleep(200);
    const arrow = textOf(th.querySelector(".arr"));
    const names = list().map(([n]) => n);
    const want = ["Kade portal", "Groene Fiets"];
    const firsts = (arrow === "↓" ? names : [...names].reverse()).slice(0, 2);
    return { arrow, names, ok: (arrow === "↓" || arrow === "↑") && want.every((w, i) => firsts[i]?.startsWith(w)) };
  };
  const sort1 = await sorted(), sort2 = await sorted();
  const sortOk = sort1.ok && sort2.ok && sort1.arrow !== sort2.arrow;

  const companyOk = companyItems.indexOf("Usage") >= 0 && companyItems.indexOf("Usage") + 1 === companyItems.indexOf("Team") && usageOn;
  const firstOk = same(tabs, ["Subscription", "Total", "Agents", "Projects"]) && same(chips, ["Today", "7 days", "30 days", "This month"])
    && toTotal && first.tab === "Total" && first.period === "This month" && first.days === first.want_days && first.bars === first.want_bars;
  const ok = companyOk && subscriptionOk && firstOk && labelled && totalOk && agentsOk && projectsOk && periodsOk && thirtyOk && columnOk && sortOk;
  return { ok, company: companyItems, usage_on: usageOn, tabs, subscription, subscription_ok: subscriptionOk, chips, first, labelled, metrics,
    unknown_line: unknownLine, total_ok: totalOk,
    agents, agents_total: agentsTotal, projects, projects_total: projectsTotal, periods: { today: toToday, d7: to7, d30: to30, month: toMonth },
    projects_30: projects30, total_30: total30, column, column_ok: columnOk, sort: [sort1, sort2] };
}

/** GA-46, against prep_chats' 36 chats (the newest "Which database?", a Team Lead question; then "Chat 1" … "Chat 35", the
 * oldest last). Recent shows the 30 newest with Archive under them, which stays in sight when Recent is scrolled to its end;
 * Archive opens #/chats with Recent still there, the crumbs Chat / Archive and the search box focused, over all 36 chats.
 * A word from the Team Lead's message in the oldest chat finds that chat alone (the same word in Chat 33's tool call and
 * note doesn't), with the word marked; "0%" finds only "50% done"; a word nowhere says so; clicking the hit opens the chat. */
export async function chatArchiveProbe() {
  const recentTitles = () => textsOf(".chat-threads-list a.th .ellipsis");
  if (!(await waitFor(() => (recentTitles().length > 0 ? true : null), 6000))) return { ok: false, error: "Recent shows no chats", page: textOf(q(".main")).slice(0, 300) };
  const want = ["Which database?", ...Array.from({ length: 35 }, (_, i) => `Chat ${i + 1}`)];
  const recent = recentTitles();
  const recentOk = recent.length === 30 && JSON.stringify(recent) === JSON.stringify(want.slice(0, 30));
  const labelOk = textOf(q(".chat-threads-list a.th .badge")) === "Question";

  // Archive sits under the list, outside the part that scrolls, and stays in sight when Recent is scrolled to its end.
  const scroller = q(".chat-threads-list");
  const button = q<HTMLAnchorElement>(".chat-threads-foot a.th");
  const aside = q(".chat-threads");
  const inSight = () => {
    const b = button?.getBoundingClientRect(), a = aside?.getBoundingClientRect();
    return !!b && !!a && b.height > 0 && b.top >= a.top && b.bottom <= a.bottom + 0.5 && b.bottom <= window.innerHeight + 0.5;
  };
  const scrolls = !!scroller && scroller.scrollHeight > scroller.clientHeight;
  const before = inSight();
  if (scroller) scroller.scrollTop = scroller.scrollHeight;
  await sleep(100);
  const atEnd = !!scroller && scroller.scrollTop > 0 && inSight();
  const lastRowAbove = (q(".chat-threads-list a.th:last-child")?.getBoundingClientRect().bottom ?? 1e9) <= (button?.getBoundingClientRect().top ?? 0) + 0.5;
  const buttonOk = !!button && textOf(button) === "Archive" && button.getAttribute("href") === "#/chats" && !!button.querySelector("svg.lucide-history")
    && !button.classList.contains("on") && !scroller?.contains(button);

  // The Archive page.
  button?.click();
  const opened = !!(await waitFor(() => (location.hash === "#/chats" && q(".chat-archive") ? true : null), 4000));
  const rows = () => [...document.querySelectorAll(".archive-row")];
  const rowTitles = () => rows().map((r) => textOf(r.querySelector(".title")));
  const all = !!(await waitFor(() => (rows().length > 0 ? true : null), 4000)) ? rowTitles() : [];
  const allOk = JSON.stringify(all) === JSON.stringify(want);
  const firstRow = rows()[0];
  // "Which database?" was asked half an hour before the seed ran
  const rowOk = textOf(firstRow?.querySelector(".badge")) === "Question" && /^3\dm ago$/.test(textOf(firstRow?.querySelector(".when")))
    && !firstRow?.querySelector(".snippet");
  const crumbs = textOf(q(".topbar .crumbs"));
  const search = q<HTMLInputElement>(".chat-archive input.archive-search");
  const focused = !!search && document.activeElement === search;
  const focusedEl = `${document.activeElement?.tagName} ${document.activeElement?.className}`;
  const highlighted = !!q('.chat-threads-foot a.th.on[aria-current="page"]');
  const chatNavOn = textOf(q('.side a.nav-item[aria-current="page"]')) === "Chat";
  const recentStays = recentTitles().length === 30;
  const searchOnTop = !!search && !!firstRow && search.getBoundingClientRect().bottom <= firstRow.getBoundingClientRect().top;

  const find = async (text: string, done: () => boolean) => {
    if (search) typeInto(search, text);
    return !!(await waitFor(() => (done() ? true : null), 4000));
  };
  const flamingo = await find("FLAMINGO", () => JSON.stringify(rowTitles()) === JSON.stringify(["Chat 35"]));
  await sleep(400); // a later answer would add Chat 33 if the search took tool calls or notes
  const hit = rows()[0];
  const found = { titles: rowTitles(), who: textOf(hit?.querySelector(".snippet .who")), snippet: textOf(hit?.querySelector(".snippet")),
    marks: textsOf(".snippet mark", hit ?? document) };
  const hitOk = flamingo && found.titles.length === 1 && found.who === "Team Lead:" && found.snippet === "Team Lead: The old flamingo plan is in the docs."
    && JSON.stringify(found.marks) === JSON.stringify(["flamingo"]);
  const percent = await find("0%", () => JSON.stringify(rowTitles()) === JSON.stringify(["Chat 34"]));
  await sleep(400);
  const percentOk = percent && JSON.stringify(rowTitles()) === JSON.stringify(["Chat 34"]) && JSON.stringify(textsOf(".snippet mark")) === JSON.stringify(["0%"]);
  const none = await find("zebra-crossing", () => textOf(q(".chat-archive .empty b")) === "No chats match “zebra-crossing”." && rows().length === 0);
  await find("flamingo", () => JSON.stringify(rowTitles()) === JSON.stringify(["Chat 35"]));

  // Clicking the hit opens the chat, older than the 30 in Recent: its title in the crumbs, its messages.
  q<HTMLAnchorElement>(".archive-row")?.click();
  const openedChat = !!(await waitFor(() => (location.hash.startsWith("#/chat/") && textOf(q(".topbar .crumbs")).includes("Chat 35")
    && [...document.querySelectorAll(".chat-msg")].some((m) => textOf(m).includes("flamingo plan")) ? true : null), 5000));
  const chatCrumbs = textOf(q(".topbar .crumbs"));
  const notInRecent = !recentTitles().includes("Chat 35") && !q(".chat-threads-list a.th.on") && !q(".chat-threads-foot a.th.on");

  const ok = recentOk && labelOk && scrolls && before && atEnd && lastRowAbove && buttonOk && opened && allOk && rowOk && crumbs === "Chat/Archive" && focused
    && highlighted && chatNavOn && recentStays && searchOnTop && hitOk && percentOk && none && openedChat && notInRecent;
  return { ok, recent_ok: recentOk, recent: recent.length, label_ok: labelOk, scrolls, in_sight: before, in_sight_at_end: atEnd, last_row_above: lastRowAbove,
    button_ok: buttonOk, opened, all: all.length, all_ok: allOk, row_ok: rowOk, crumbs, focused, focused_el: focusedEl, highlighted,
    chat_nav_on: chatNavOn, recent_stays: recentStays, search_on_top: searchOnTop, found, hit_ok: hitOk, percent_ok: percentOk, no_match: none,
    opened_chat: openedChat, chat_crumbs: chatCrumbs, not_in_recent: notInRecent };
}

// ---------- GA-42: Settings → Appearance ----------
type Box = { fs: number | null; h: number | null; w: number | null; family: string };
const box = (el: Element | null | undefined): Box => {
  if (!el) return { fs: null, h: null, w: null, family: "" };
  const cs = getComputedStyle(el), r = el.getBoundingClientRect();
  return { fs: parseFloat(cs.fontSize), h: Math.round(r.height * 10) / 10, w: Math.round(r.width * 10) / 10,
    family: (cs.fontFamily.split(",")[0] ?? "").replace(/["']/g, "").trim() };
};
const near = (a: number | null | undefined, b: number, by = 0.6) => a != null && Math.abs(a - b) <= by;
const goTo = async (hash: string, ready: string, ms = 6000) => { location.hash = hash; const ok = !!(await waitFor(() => q(ready), ms)); await sleep(250); return ok; };
/** The task list or the board. The page keeps its List/Board switch in localStorage, which the next start (and the board
 * probe) shares: the probe sets it for a moment and puts it back (restoreView). */
const VIEW_PREF = "gizai-tasks-view";
let view0: string | null | undefined;
const showTasks = (view: "list" | "board") => {
  if (view0 === undefined) view0 = localStorage.getItem(VIEW_PREF);
  localStorage.setItem(VIEW_PREF, JSON.stringify(view));
  return view === "list" ? goTo("#/tasks", ".task-row") : goTo("#/board", ".col:not(.rail) .card");
};
const restoreView = () => {
  if (view0 === undefined) return;
  if (view0 === null) localStorage.removeItem(VIEW_PREF); else localStorage.setItem(VIEW_PREF, view0);
};
const appearancePanel = () => q('.settings-panel[aria-label="Appearance"]');
const pick = async (group: string, label: string) => {
  const b = [...(appearancePanel()?.querySelectorAll(`[aria-label="${group}"] button`) ?? [])].find((x) => textOf(x) === label) as HTMLElement | undefined;
  b?.click();
  await sleep(150);
  return !!b;
};
const pressedIn = (group: string) => textsOf(`[aria-label="${group}"] button[aria-pressed="true"]`, appearancePanel() ?? document);
const SIZE_KEYS = ["gizai-font", "gizai-size-chat", "gizai-size-ui", "gizai-size-docs", "gizai-theme", "gizai-density"];
const kept = () => Object.fromEntries(SIZE_KEYS.map((k) => [k, localStorage.getItem(k)]).filter(([, v]) => v !== null));
const htmlVars = () => {
  const s = document.documentElement.style;
  return Object.fromEntries([...Array(s.length).keys()].map((i) => s.item(i)).filter((n) => n.startsWith("--")).map((n) => [n, s.getPropertyValue(n).trim()]));
};
/** Elements wider than their box (they would scroll sideways or be cut off at the side); the page itself first. */
function sideways(sels: string[]): string[] {
  const out: string[] = [];
  if (document.documentElement.scrollWidth > window.innerWidth + 1) out.push(`page ${document.documentElement.scrollWidth}>${window.innerWidth}`);
  for (const sel of sels) for (const el of document.querySelectorAll(sel)) {
    if (el.scrollWidth > el.clientWidth + 1) out.push(`${sel} "${textOf(el).slice(0, 24)}" ${el.scrollWidth}>${el.clientWidth}`);
  }
  return out;
}
/** Rows and controls whose text is taller than they are (cut off at the top or bottom). */
function cutOff(sels: string[]): string[] {
  return sels.flatMap((sel) => [...document.querySelectorAll(sel)].filter((el) => el.scrollHeight > el.clientHeight + 1)
    .map((el) => `${sel} "${textOf(el).slice(0, 24)}" ${el.scrollHeight}>${el.clientHeight}`));
}
const ROWS = [".side .nav-item", ".topbar", ".btn", ".tab", ".task-row", ".group-head", ".panel-row", ".label-pill", ".badge", ".input", ".select"];
/** The small things on the task list and in the sidebar: IDs, label pills, dates, avatars, group labels, badges, hints, icons. */
const smallThings = () => ({
  id: box(q(".task-row .id")).fs, pill: box(q(".label-pill")).fs, pill_h: box(q(".label-pill")).h, date: box(q(".task-row .date")).fs,
  avatar: box(q(".task-row .avatar")).w, nav_label: box(q(".side .nav-label")).fs, badge: box(q(".badge")).fs, kbd: box(q(".kbd")).fs,
  icon: box(q(".side .nav-item svg")).w, row_icon: box(q(".task-row svg")).w,
});
const FAMILIES: Record<string, [string, string]> = {
  atkinson: ["Atkinson Hyperlegible Next", "Atkinson Hyperlegible Mono"], "jetbrains-mono": ["JetBrains Mono", "JetBrains Mono"],
  inter: ["Inter", "JetBrains Mono"], geist: ["Geist", "Geist Mono"], hack: ["Hack", "Hack"],
};
/** The text font and the code font the app uses now (a code element is added for a moment). */
function appFonts(): [string, string] {
  const code = document.createElement("span");
  code.className = "mono";
  code.textContent = "KADE-1";
  q(".main")?.appendChild(code);
  const fams: [string, string] = [box(q(".side .nav-item")).family, box(code).family];
  code.remove();
  return fams;
}

/** GA-42, against prep_chats' data (the demo plus a paused Team Lead and 36 chats). Phase "set": Settings → Appearance opens
 * on its own tab with Font, the three text sizes, Theme, Density and Reset to defaults; at the defaults nothing is on <html> and
 * the sizes are the design system's; each font choice shows in its own font and every font loads from the app (bundled).
 * Then the largest sizes (chat 20, interface 16.5, tasks and docs 20), picked with clicks: they show at once and are kept;
 * reading text grows fully, titles half, small things at most 1px; rows grow so nothing is cut off; the chat column, the
 * sidebar and board columns get wider; in a 1280 px window nothing scrolls sideways on the task list, board, task page and
 * chat, in dark and in light. Then Compact, and each font everywhere at once. It leaves Geist, light, compact and the
 * largest sizes kept. Phase "kept" (the next start): they are all still there, and Reset to defaults brings everything back. */
export async function appearanceProbe(phase: "set" | "kept") {
  if (!(await waitFor(() => appearancePanel(), 6000))) return { ok: false, error: "Settings → Appearance did not open", page: textOf(q(".main")).slice(0, 300) };
  await sleep(300);
  if (phase === "kept") return appearanceKeptProbe();

  // The tab, its controls, and the default look.
  const tabs = { labels: textsOf('.tabs[role="tablist"] button[role="tab"]'), selected: textOf(q('.tabs button[aria-selected="true"]')),
    shown: [...document.querySelectorAll(".settings-panel")].filter((p) => p.getClientRects().length > 0).map((p) => p.getAttribute("aria-label")),
    address: location.hash };
  const panel = appearancePanel()!;
  const controls = { sections: textsOf(".form-section > header h3", panel), groups: [...panel.querySelectorAll('[role="group"], [role="radiogroup"]')].map((g) => g.getAttribute("aria-label")),
    fonts: textsOf(".font-choice .fc-name", panel), reset_disabled: (q<HTMLButtonElement>("button.link", panel))?.disabled ?? null,
    reset_text: textOf(q("button.link", panel)) };
  const defaults = { html_vars: htmlVars(), font_attr: document.documentElement.dataset.font ?? null, theme: document.documentElement.dataset.theme ?? null,
    density: document.documentElement.dataset.density ?? null, kept: kept(),
    nav: box(q(".side .nav-item")), side: box(q(".side")).w, topbar: box(q(".topbar")).h, save: box(q(".topbar .btn.primary")).h, tab: box(q(".tabs .tab")).fs,
    bubble: box(q(".chat-sample .bubble")).fs, agent: box(q(".chat-sample .prose")).fs, docs: box(q(".docs-sample .prose")).fs, docs_h3: box(q(".docs-sample h3")).fs,
    pressed: { chat: pressedIn("Chat size"), ui: pressedIn("Interface size"), docs: pressedIn("Tasks and docs size"), theme: pressedIn("Theme"), density: pressedIn("Density") } };
  const tabOk = JSON.stringify(tabs.labels) === JSON.stringify(["General", "Appearance", "Notifications", "Agents and runs", "MCP servers", "GitHub and Bitbucket"])
    && tabs.selected === "Appearance" && JSON.stringify(tabs.shown) === '["Appearance"]';
  const controlsOk = JSON.stringify(controls.groups) === JSON.stringify(["Font", "Chat size", "Interface size", "Tasks and docs size", "Theme", "Density"])
    && JSON.stringify(controls.fonts) === JSON.stringify(["Atkinson Hyperlegible", "JetBrains Mono", "Inter", "Geist", "Hack"])
    && controls.reset_disabled === true && controls.reset_text === "Reset to defaults";
  const defaultsOk = Object.keys(defaults.html_vars).length === 0 && defaults.font_attr === null && defaults.theme === "dark" && defaults.density === null
    && Object.keys(defaults.kept).length === 0 && defaults.nav.fs === 13.5 && defaults.nav.h === 32 && defaults.side === 248 && defaults.topbar === 52
    && defaults.save === 30 && defaults.tab === 13.5 && defaults.bubble === 13.5 && defaults.agent === 15 && defaults.docs === 15 && defaults.docs_h3 === 15.5
    && JSON.stringify(defaults.pressed) === JSON.stringify({ chat: ["15"], ui: ["13.5"], docs: ["15"], theme: ["Dark"], density: ["Comfortable"] })
    && defaults.nav.family === "Atkinson Hyperlegible Next";

  // Tabs work like the Usage page's, in the address; an edit not saved yet survives a switch to another tab and back.
  const tabTo = async (label: string) => {
    [...document.querySelectorAll('.tabs button[role="tab"]')].find((b) => textOf(b) === label)?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    return !!(await waitFor(() => (textOf(q('.tabs button[aria-selected="true"]')) === label ? true : null), 3000));
  };
  const shownPanels = () => [...document.querySelectorAll(".settings-panel")].filter((p) => p.getClientRects().length > 0).map((p) => p.getAttribute("aria-label"));
  await tabTo("Agents and runs");
  const onAgents = { address: location.hash, shown: shownPanels() };
  const runsAtOnce = q<HTMLInputElement>("#s-max");
  const before = runsAtOnce?.value ?? "";
  const edited = before === "9" ? "8" : "9";
  if (runsAtOnce) typeInto(runsAtOnce, edited);
  await sleep(150);
  await tabTo("General");
  const onGeneral = { address: location.hash, shown: shownPanels(), quit_first: textOf(q('.settings-panel[aria-label="General"] .form-section > header h3')),
    value: q<HTMLInputElement>("#s-max")?.value };
  await tabTo("Agents and runs");
  const back = q<HTMLInputElement>("#s-max")?.value;
  if (runsAtOnce) typeInto(runsAtOnce, before); // nothing was saved; put the field back as it was
  await tabTo("Appearance");
  const switching = { on_agents: onAgents, on_general: onGeneral, back, edited, address: location.hash };
  const switchingOk = onAgents.address === "#/settings/agents" && JSON.stringify(onAgents.shown) === '["Agents and runs"]' && !!runsAtOnce
    && onGeneral.address === "#/settings/general" && JSON.stringify(onGeneral.shown) === '["General"]' && onGeneral.quit_first === "Quit"
    && onGeneral.value === edited && back === edited && switching.address === "#/settings/appearance" && !!appearancePanel()?.getClientRects().length;

  // Each font choice in its own font, and every font loads from the app.
  const choices = [...panel.querySelectorAll(".font-choice")].map((c) => [c.getAttribute("data-font") ?? "", box(c.querySelector(".fc-name")).family, box(c.querySelector(".fc-sample .mono")).family]);
  const loads: Record<string, number> = {};
  for (const fam of new Set(Object.values(FAMILIES).flat())) {
    try { loads[fam] = (await document.fonts.load(`16px "${fam}"`, "Export KADE-41")).filter((f) => f.status === "loaded").length; } catch { loads[fam] = -1; }
  }
  const choicesOk = choices.length === 5 && choices.every(([k = "", sans, mono]) => FAMILIES[k]?.[0] === sans && FAMILIES[k]?.[1] === mono)
    && Object.values(loads).every((n) => n > 0);

  // The small things and rows at the defaults, on the task list; the board's columns.
  await showTasks("list");
  const small0 = smallThings();
  const row0 = { row: box(q(".task-row")).h, title: box(q(".task-row .title")).fs };
  await showTasks("board");
  const col0 = box(q(".col:not(.rail)")).w;

  // The largest sizes, picked with clicks on Settings → Appearance: at once, and kept.
  await goTo("#/settings/appearance", '.settings-panel[aria-label="Appearance"]');
  const clicked = [await pick("Chat size", "20"), await pick("Interface size", "16.5"), await pick("Tasks and docs size", "20")];
  await sleep(200);
  const big = { html_vars: htmlVars(), kept: kept(), reset_disabled: (q<HTMLButtonElement>("button.link", appearancePanel() ?? document))?.disabled ?? null,
    nav: box(q(".side .nav-item")), side: box(q(".side")).w, topbar: box(q(".topbar")).h, save: box(q(".topbar .btn.primary")).h, tab: box(q(".tabs .tab")).fs,
    bubble: box(q(".chat-sample .bubble")).fs, agent: box(q(".chat-sample .prose")).fs, docs: box(q(".docs-sample .prose")).fs, docs_h3: box(q(".docs-sample h3")).fs,
    section: box(q(".settings-panel:not([hidden]) .form-section > header h3")).fs, settings_cut: cutOff(ROWS), settings_wide: sideways([".side", ".main", ".content"]) };
  const bigOk = clicked.every(Boolean) && big.html_vars["--chat-grow"] === "5px" && big.html_vars["--ui-grow"] === "3px" && big.html_vars["--docs-grow"] === "5px"
    && JSON.stringify(big.kept) === JSON.stringify({ "gizai-size-chat": "20", "gizai-size-ui": "16.5", "gizai-size-docs": "20" }) && big.reset_disabled === false
    && big.nav.fs === 16.5 && big.nav.h === 37 && big.side === 266 && big.topbar === 57 && big.save === 35 && big.tab === 16.5
    && big.bubble === 18.5 && big.agent === 20 && big.docs === 20 && big.docs_h3 === 18 && near(big.section, 16.5, 0.01)
    && big.settings_cut.length === 0 && big.settings_wide.length === 0;

  // Dark, then light: the task list, the board, a task page with its editor and the New task drawer, a doc, a chat.
  const tasks = await listTasks();
  // a task with a description and acceptance criteria (and comments, if the demo has one)
  let task = tasks.find((t) => t.identifier === "KADE-1") ?? tasks[0];
  for (const t of tasks.slice(0, 20)) { const full = await getTask(t.id); if (full.descriptionMd.trim() && full.acceptanceMd?.trim()) { task = t; break; } }
  let docId = "";
  for (const p of await listProjects()) { const d = (await listDocs(p.id)).find((x) => x.title === "Requirements"); if (d) { docId = d.id; break; } }
  const chat = (await listChatThreads()).find((t) => t.title === "Chat 35");
  const pages = async () => {
    const out: Record<string, unknown> = {};
    await showTasks("list");
    out.tasks = { small: smallThings(), row: box(q(".task-row")).h, title: box(q(".task-row .title")).fs, cut: cutOff(ROWS), wide: sideways([".side", ".side .nav-item", ".main", ".content", ".task-list", ".task-row"]) };
    await showTasks("board");
    out.board = { col: box(q(".col:not(.rail)")).w, card: box(q(".col .card")).fs, cut: cutOff([...ROWS, ".col-head"]), wide: sideways([".side", ".main", ".col", ".col .card", ".col-head"]) };
    if (task) {
      await goTo(`#/task/${task.id}`, ".md-click");
      const blocks = [...document.querySelectorAll(".md-click")];
      out.task = { identifier: task.identifier, description: box(blocks[0]?.querySelector(".prose")).fs, acceptance: box(blocks[1]?.querySelector(".prose")).fs, title: box(q(".title-input")).fs,
        comment: box(q(".comment .prose")).fs, id: box(q(".topbar .id, .task-head .id, .id")).fs, cut: cutOff(ROWS), wide: sideways([".side", ".main", ".content", ".split", ".split > *", ".topbar"]) };
    }
    if (docId) {
      await goTo(`#/doc/${docId}`, ".cm-editor, .prose");
      out.doc = { text: box(q(".main .cm-editor") ?? q(".main .prose")).fs, wide: sideways([".main", ".content", ".split", ".split > *"]) };
    }
    if (chat) {
      await goTo(`#/chat/${chat.id}`, ".chat-msg.agent .prose");
      const col = q(".chat-scroll .chat-col") ?? q(".chat-col");
      out.chat = { user: box(q(".chat-msg.user .bubble")).fs, agent: box(q(".chat-msg.agent .prose")).fs, composer: box(q(".composer-editor .cm-editor")).fs,
        col_max: col ? parseFloat(getComputedStyle(col).maxWidth) : null, col_w: box(col).w, meta: box(q(".chat-meta, .chat-head")).fs,
        threads: box(q(".chat-threads")).w, cut: cutOff([...ROWS, ".chat-threads-list a.th"]), wide: sideways([".side", ".main", ".chat-threads", ".chat-scroll", ".chat-col", ".composer-box", ".chat-msg"]) };
    }
    return out;
  };
  const dark = await pages();
  // The editors: the description's and the acceptance criteria's on the task page (opened, then Escape: nothing saved), and
  // the New task drawer's.
  let editors: Record<string, unknown> = {};
  if (task) {
    await goTo(`#/task/${task.id}`, ".md-click");
    const edit = async (i: number) => {
      (document.querySelectorAll(".md-click")[i] as HTMLElement | undefined)?.click();
      const cm = await waitFor(() => q<HTMLElement>(".md-edit-box .cm-content"), 4000);
      const fs = box(q(".md-edit-box .cm-editor")).fs;
      if (cm) { cm.focus(); press(cm, "Escape", 27); }
      const closed = !!(await waitFor(() => (q(".md-edit-box") ? null : true), 3000));
      await sleep(200);
      return { fs, closed };
    };
    const description = await edit(0);
    const acceptance = await edit(1);
    location.hash = "#/inbox";
    await waitFor(() => q(".topbar"), 4000);
    await sleep(300);
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "n", code: "KeyN", bubbles: true, cancelable: true }));
    const drawer = await waitFor(() => q('[role="dialog"] .cm-editor'), 4000);
    const inDrawer = box(drawer).fs;
    const drawerTitle = box(q('[role="dialog"] h2, [role="dialog"] .t-drawer-title')).fs;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }));
    const drawerClosed = !!(await waitFor(() => (q('[role="dialog"]') ? null : true), 3000));
    editors = { description: description.fs, description_closed: description.closed, acceptance: acceptance.fs, acceptance_closed: acceptance.closed,
      drawer: inDrawer, drawer_title: drawerTitle, drawer_closed: drawerClosed };
  }
  await goTo("#/settings/appearance", '.settings-panel[aria-label="Appearance"]');
  await pick("Theme", "Light");
  const light = { theme: document.documentElement.dataset.theme, bg: getComputedStyle(document.body).backgroundColor, kept: localStorage.getItem("gizai-theme") };
  const lightPages = await pages();

  const growsOk = (p: Record<string, any>) => {
    const s = p.tasks?.small ?? {};
    const smallOk = Object.entries(small0).every(([k, v]) => v == null || (s[k] != null && s[k] - v >= -0.01 && s[k] - v <= 1.01));
    return smallOk && near(p.tasks?.title, (row0.title ?? 14) + 3, 0.01) && (p.tasks?.row ?? 0) >= (row0.row ?? 40) + 5
      && near(p.board?.col, (col0 ?? 286) + 24, 0.5)
      && (!task || (p.task?.description === 20 && (p.task?.acceptance == null || p.task?.acceptance === 20) && p.task?.title === 24.5 && (p.task?.comment == null || p.task?.comment === 18.5)))
      && (!docId || p.doc?.text === 20)
      && (!chat || (p.chat?.user === 18.5 && p.chat?.agent === 20 && (p.chat?.composer == null || p.chat?.composer === 18.5) && near(p.chat?.col_max, 760 * 1.333, 0.5)))
      && ["tasks", "board", "task", "doc", "chat"].every((k) => !p[k] || ((p[k].cut ?? []).length === 0 && (p[k].wide ?? []).length === 0));
  };
  const editorsOk = !task || (editors.description === 20 && editors.description_closed === true && editors.acceptance === 20 && editors.acceptance_closed === true
    && editors.drawer === 20 && editors.drawer_closed === true);
  const lightOk = light.theme === "light" && light.bg === "rgb(255, 255, 255)" && light.kept === "light";

  // Compact, then each font everywhere at once; Geist stays for the next start.
  await goTo("#/settings/appearance", '.settings-panel[aria-label="Appearance"]');
  await pick("Density", "Compact");
  const compact = { density: document.documentElement.dataset.density, nav_h: box(q(".side .nav-item")).h, box: htmlVars()["--ui-box"], kept: localStorage.getItem("gizai-density") };
  const compactOk = compact.density === "compact" && compact.nav_h === 33 && compact.box === "1px" && compact.kept === "compact";
  const fonts: Record<string, unknown> = {};
  for (const key of ["jetbrains-mono", "inter", "hack", "atkinson", "geist"]) {
    q<HTMLInputElement>(`.font-choice[data-font="${key}"] input`, appearancePanel() ?? document)?.click();
    await sleep(200);
    fonts[key] = { attr: document.documentElement.dataset.font ?? null, fams: appFonts(), kept: localStorage.getItem("gizai-font"),
      checked: q(".font-choice.on", appearancePanel() ?? document)?.getAttribute("data-font") };
  }
  const fontsOk = Object.entries(fonts).every(([key, f]: [string, any]) => JSON.stringify(f.fams) === JSON.stringify(FAMILIES[key]) && f.checked === key
    && (key === "atkinson" ? f.attr === null && f.kept === null : f.attr === key && f.kept === key));
  restoreView();
  await sleep(1500); // WebKit writes localStorage to disk a moment later

  const ok = tabOk && switchingOk && controlsOk && defaultsOk && choicesOk && bigOk && growsOk(dark) && growsOk(lightPages) && editorsOk && lightOk && compactOk && fontsOk;
  return { ok, phase, tab_ok: tabOk, switching_ok: switchingOk, controls_ok: controlsOk, defaults_ok: defaultsOk, choices_ok: choicesOk, big_ok: bigOk, dark_ok: growsOk(dark),
    light_pages_ok: growsOk(lightPages), editors_ok: editorsOk, light_ok: lightOk, compact_ok: compactOk, fonts_ok: fontsOk,
    tabs, switching, controls, defaults, choices, loads, small0, row0, col0, big, dark, light, light_pages: lightPages, editors, compact, fonts,
    found: { task: task?.identifier ?? null, doc: !!docId, chat: !!chat } };
}

/** The next start: what the "set" phase left is still there, then Reset to defaults brings everything back. */
async function appearanceKeptProbe() {
  const html = document.documentElement;
  const start = { font: html.dataset.font ?? null, theme: html.dataset.theme ?? null, density: html.dataset.density ?? null, vars: htmlVars(), kept: kept(),
    pressed: { chat: pressedIn("Chat size"), ui: pressedIn("Interface size"), docs: pressedIn("Tasks and docs size"), theme: pressedIn("Theme"), density: pressedIn("Density") },
    checked: q(".font-choice.on", appearancePanel() ?? document)?.getAttribute("data-font") ?? null, fams: appFonts(), nav: box(q(".side .nav-item")) };
  const startOk = start.font === "geist" && start.theme === "light" && start.density === "compact" && start.vars["--chat-grow"] === "5px"
    && start.vars["--ui-grow"] === "3px" && start.vars["--docs-grow"] === "5px" && start.vars["--ui-box"] === "1px"
    && JSON.stringify(start.pressed) === JSON.stringify({ chat: ["20"], ui: ["16.5"], docs: ["20"], theme: ["Light"], density: ["Compact"] })
    && start.checked === "geist" && JSON.stringify(start.fams) === JSON.stringify(FAMILIES.geist) && start.nav.fs === 16.5 && start.nav.h === 33;
  const reset = q<HTMLButtonElement>("button.link", appearancePanel() ?? document);
  const resetEnabled = !!reset && !reset.disabled;
  reset?.click();
  await sleep(300);
  const after = { font: html.dataset.font ?? null, theme: html.dataset.theme ?? null, density: html.dataset.density ?? null, vars: htmlVars(), kept: kept(),
    pressed: { chat: pressedIn("Chat size"), ui: pressedIn("Interface size"), docs: pressedIn("Tasks and docs size"), theme: pressedIn("Theme"), density: pressedIn("Density") },
    reset_disabled: reset?.disabled ?? null, fams: appFonts(), nav: box(q(".side .nav-item")), bg: getComputedStyle(document.body).backgroundColor };
  const afterOk = after.font === null && after.theme === "dark" && after.density === null && Object.keys(after.vars).length === 0 && Object.keys(after.kept).length === 0
    && JSON.stringify(after.pressed) === JSON.stringify({ chat: ["15"], ui: ["13.5"], docs: ["15"], theme: ["Dark"], density: ["Comfortable"] })
    && after.reset_disabled === true && JSON.stringify(after.fams) === JSON.stringify(FAMILIES.atkinson) && after.nav.fs === 13.5 && after.nav.h === 32
    && after.bg !== "rgb(255, 255, 255)";
  // whatever happened, leave the defaults for the next test's start
  if (Object.keys(kept()).length) resetAppearance();
  await sleep(1500);
  return { ok: startOk && resetEnabled && afterOk, phase: "kept", start_ok: startOk, reset_enabled: resetEnabled, after_ok: afterOk, start, after };
}
