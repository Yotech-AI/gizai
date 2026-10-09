// Self-test probes, used only when Gizai runs with GIZAI_SELFTEST (headless cage, test data).
import { listChatThreads, listTasks } from "./api";

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

/** Team page: add an agent with a heartbeat through the dialog; it lands on its role's usual columns (GA-49: no routing rules). */
function pickOption(el: HTMLSelectElement, value: string) {
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")!.set!.call(el, value);
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

export async function teamProbe(getTeam: () => Promise<{ members: { actorId: string; name: string; kind: string; wakeup?: string | null; heartbeatMinutes?: number | null; instructionsMd?: string | null; model?: string | null; effort?: string | null }[]; states: { name: string; agentIds?: string[] }[] }>) {
  const open = await waitFor(() => buttonByText(document, "Add agent"));
  if (!open) return { ok: false, error: "no Add agent button" };
  open.click();
  const dialog = await waitFor(() => document.querySelector("[role=dialog]") as HTMLElement | null);
  if (!dialog) return { ok: false, error: "dialog did not open" };
  await waitFor(() => (dialog.querySelector(".cm-content")?.textContent ?? "").includes("GIZAI_RESULT") || null, 3000);
  typeInto(dialog.querySelector("input.input") as HTMLInputElement, "Frontend Agent");
  const radios = [...dialog.querySelectorAll("input[type=radio]")] as HTMLInputElement[];
  radios[2]?.click();
  await sleep(100);
  // the wake-up minutes, not the board check's (the Chat section comes first)
  const minutes = dialog.querySelector("input[type=number][aria-label=Minutes]") as HTMLInputElement | null;
  if (minutes) typeInto(minutes, "20");
  // The model list comes from Claude Code (the fake here): pick Opus, then Extra high effort.
  const modelSel = await waitFor(() => {
    const el = dialog.querySelector("#a-model") as HTMLSelectElement | null;
    return el && !el.disabled && [...el.options].some((o) => o.value === "opus") ? el : null;
  }, 8000);
  if (modelSel) pickOption(modelSel, "opus");
  await sleep(100);
  const effortSel = dialog.querySelector("#a-effort") as HTMLSelectElement | null;
  if (effortSel) pickOption(effortSel, "xhigh");
  await sleep(100);
  buttonByText(dialog, "Add agent")?.click();
  let agent: { actorId: string; wakeup?: string | null; heartbeatMinutes?: number | null; instructionsMd?: string | null; model?: string | null; effort?: string | null } | undefined;
  for (let i = 0; i < 30 && !agent; i++) { await sleep(100); agent = (await getTeam()).members.find((m) => m.name === "Frontend Agent" && m.kind === "agent"); }
  // Adding an agent opens its page. A builder lands on the team's To do and In progress columns (GA-49: no routing rules).
  const agentPage = !!(await waitFor(() => (document.querySelector(".entity-head h1")?.textContent ?? "") === "Frontend Agent" || null, 3000));
  const id = agent?.actorId;
  const columns = id ? (await getTeam()).states.filter((s) => s.agentIds?.includes(id)).map((s) => s.name) : [];
  const panelOpen = agentPage;
  const ok = !!agent && agent.wakeup === "heartbeat" && agent.heartbeatMinutes === 20 && !!agent.instructionsMd?.includes("Frontend Agent")
    && columns.join(",") === "To do,In progress" && panelOpen && agent.model === "opus" && agent.effort === "xhigh";
  return { ok, agent_added: !!agent, wakeup: agent?.wakeup, minutes: agent?.heartbeatMinutes, template: !!agent?.instructionsMd?.includes("GIZAI_RESULT"), columns, agent_page: panelOpen,
    model_list: !!modelSel, model: agent?.model, effort: agent?.effort };
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
