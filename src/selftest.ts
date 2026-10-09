// Self-test probes, used only when Gizai runs with GIZAI_SELFTEST (headless cage, test data).
import { getTask, listChatThreads, listLabels, listTasks } from "./api";

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

/** Opens the task's description editor, types like a keyboard would, saves with Ctrl+Enter, reads the task back. */
export async function editorProbe(taskId: string, getDescription: (id: string) => Promise<string>, holdMs = 0) {
  const box = await waitFor(() => document.querySelector(".md-click") as HTMLElement | null);
  if (!box) return { saved: false, error: "no description block" };
  box.click();
  const content = await waitFor(() => document.querySelector(".md-edit-box .cm-content") as HTMLElement | null);
  if (!content) return { saved: false, error: "editor did not open" };
  await sleep(200);
  const dimmed = document.querySelectorAll(".md-edit-box .cm-md-mark").length;
  const typed = " Typed by the self-test: café ✓";
  document.execCommand("insertText", false, typed);
  await sleep(holdMs);
  content.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, ctrlKey: true, bubbles: true, cancelable: true }));
  let stored = "";
  for (let i = 0; i < 20; i++) {
    await sleep(100);
    stored = await getDescription(taskId);
    if (stored.endsWith(typed)) break;
  }
  const closed = !document.querySelector(".md-edit-box");
  return { saved: stored.endsWith(typed), editor_closed: closed, dimmed_marks: dimmed, task: taskId };
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
  const ctrlS = () => content.dispatchEvent(new KeyboardEvent("keydown", { key: "s", code: "KeyS", keyCode: 83, ctrlKey: true, bubbles: true, cancelable: true }));
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
  members: { actorId: string; name: string; kind: string; roleKey: string; wakeup?: string | null; heartbeatMinutes?: number | null; model?: string | null; effort?: string | null }[];
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
    wake_up: !!q("input[aria-label=Minutes]", dialog) || (dialog.textContent ?? "").includes("Wakes up") };
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
    columns: (await getTeam()).states.filter((s) => s.agentIds?.includes(id)).map((s) => s.name) };
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

  const a = out.agent as { role?: string; wakeup?: string | null; model?: string | null; effort?: string | null; agent_page?: boolean; columns?: string[] };
  const sf = out.spot_form as { name?: string; role?: string; wake_up?: boolean };
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
    spot_form: sf.name === "Frontend Agent" && sf.role === "frontend" && !sf.wake_up,
    agent: a.role === "frontend" && a.wakeup !== "heartbeat" && !!a.agent_page && a.model === "opus" && a.effort === "xhigh" && a.columns?.join(",") === "To do,In progress",
    plus_agent: Object.values(pa).every(Boolean),
    auto_manual: am.manual && am.auto && am.manual_line.startsWith("Manual: press Run on a card to start Frontend Agent") && am.auto_line.startsWith("Auto: Frontend Agent takes cards"),
    next_column: !!nc.self_link_refused && nc.cleared && !!nc.auto_without_next_refused && nc.stayed_manual && nc.linked_to_done && nc.error_gone,
    review: (out.review as { you: boolean; merged_to?: string }).you && (out.review as { merged_to?: string }).merged_to === "Deploy",
    add_column: ac.made && ac.category === "testing" && ac.auto === false && ac.shown,
    reorder: !!(out.reorder as { moved: boolean }).moved,
    bins_off: bo.backlog.disabled && !!bo.backlog.why?.includes("last Backlog") && bo.done.disabled && !!bo.done.why?.includes("last Done"),
    confirm: !!cf.text?.includes("Its card goes to") && cf.default_target === "Testing (the column before)" && !!cf.text?.includes("Testing links to Deploy instead.") && cf.kept,
    remove: !!rm.confirm?.includes("It has no cards.") && rm.removed && rm.off_screen,
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
  const box = await waitFor(() => document.querySelector(".composer-box textarea") as HTMLTextAreaElement | null, 4000);
  if (!box) return { ok: false, error: "no composer", name, chatOn, onTeam, agentPage };
  typeInto(box, "create task Chat probe task in KADE");
  await sleep(50);
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, bubbles: true, cancelable: true }));
  const card = await waitFor(() => document.querySelector('.tool-card a[href^="#/task/"]') as HTMLAnchorElement | null, 15000);
  const reply = !!(await waitFor(() => [...document.querySelectorAll(".chat-msg.agent")].some((e) => (e.textContent ?? "").includes("Done:")) || null, 15000));
  const made = (await listTitles()).includes("Chat probe task");
  const thread = window.location.hash.startsWith("#/chat/");
  const listed = !!document.querySelector(".chat-threads .th.on");
  const first = onTeam && name === "Team Lead" && chatOn && agentPage && !!card && reply && made && thread && listed;
  const runsOn = await runsOnAndQueueProbe();
  const ok = first && runsOn.ok;
  return { ok, on_team: onTeam, name, chat_on: chatOn, agent_page: agentPage, tool_card: card?.textContent, reply, task_made: made, thread_url: thread,
    thread_listed: listed, runs_on: runsOn };
}

const enter = (el: HTMLElement) => el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, bubbles: true, cancelable: true }));
const composer = () => document.querySelector(".composer-box textarea") as HTMLTextAreaElement | null;
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
  typeInto(box, "FAKE_CHAT_SLOW what is next?");
  await sleep(50);
  enter(box);
  const answering = !!(await waitFor(() => document.querySelector(".composer-box .stop-btn"), 4000));
  const locked = !!picker()?.disabled;
  const box2 = composer();
  if (box2) { typeInto(box2, "and one more thing"); await sleep(50); enter(box2); }
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
