// Builds the Gizai design system files under ./project from tokens.json, the hand-written README.md and
// components/bundle.css, Lucide's own icon data (lucide-react 0.577, ISC) and the app's font files.
import fs from "node:fs";
import path from "node:path";

const HERE = path.dirname(new URL(import.meta.url).pathname);
const OUT = path.join(HERE, "project");
const GZ = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const LUCIDE = `${GZ}/node_modules/lucide-react/dist/esm/icons`;

const ICONS = ["square-pen", "inbox", "list-todo", "building-2", "folder-kanban", "network", "users", "settings", "search", "list",
  "columns-3", "list-filter", "arrow-up-down", "layers", "plus", "x", "play", "pause", "square", "paperclip", "file-text", "git-branch",
  "terminal", "crown", "monitor", "server", "flask-conical", "bot", "chevron-down", "chevron-right", "external-link", "ellipsis",
  "heading", "bold", "italic", "text-quote", "code", "link", "list-ordered", "list-checks", "minus", "table", "copy", "clock", "tag",
  "user", "message-square", "activity", "upload", "sun", "moon", "check", "trash-2", "pencil", "circle-alert", "layout-dashboard",
  "messages-square", "palette", "container", "arrow-up", "info"];

const nodes = {};
for (const name of ICONS) {
  const file = `${LUCIDE}/${name}.js`;
  if (!fs.existsSync(file)) { console.warn("missing icon", name); continue; }
  nodes[name] = (await import(file)).__iconNode;
}
const attrs = (o) => Object.entries(o).filter(([k]) => k !== "key").map(([k, v]) => `${k.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())}="${v}"`).join(" ");
const inner = (name) => nodes[name].map(([tag, a]) => `<${tag} ${attrs(a)}/>`).join("");
const ic = (name, cls = "icon") =>
  `<svg class="${cls}" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${inner(name)}</svg>`;

// Status glyphs: Gizai's own, 16px grid. Shared with the app (src/components/StatusIcon.tsx draws the same paths).
const GLYPH = {
  backlog: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6" stroke-dasharray="2.4 2.1"/>',
  ready: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/>',
  in_progress: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 4.25a3.75 3.75 0 0 1 0 7.5z" fill="currentColor"/>',
  testing: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 8V4.25a3.75 3.75 0 1 1-3.75 3.75z" fill="currentColor"/>',
  review: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><circle cx="8" cy="8" r="2.4" fill="currentColor"/>',
  deploy: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M8 10.8V5.2M5.6 7.6 8 5.2l2.4 2.4" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>',
  done: '<circle cx="8" cy="8" r="7" fill="currentColor"/><path d="m5.1 8.2 2 1.9 3.8-3.9" stroke="var(--bg)" stroke-width="1.7" fill="none" stroke-linecap="round" stroke-linejoin="round"/>',
  hold: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="M5.2 8h5.6" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/>',
  cancelled: '<circle cx="8" cy="8" r="6.25" stroke="currentColor" stroke-width="1.6"/><path d="m4 12 8-8" stroke="currentColor" stroke-width="1.6"/>',
};
const st = (cat, title) => `<svg class="st st-${cat}" viewBox="0 0 16 16" fill="none" role="img" aria-label="${title}">${GLYPH[cat]}</svg>`;

const write = (rel, text) => { const p = path.join(OUT, rel); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, text); };
const card = (group, height, extra = "") => `<!-- @dsCard group="${group}" height=${height}${extra} -->\n`;
const doc = (body, style = "") => `<div class="pv" style="padding:16px">\n${body}\n</div>\n<style>.pv{font-family:var(--font-sans);color:var(--text)}${style}</style>\n`;
const row = (...xs) => `<div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap">${xs.join("")}</div>`;
const live = (t = "Live") => `<span class="badge live"><span class="pulse"></span>${t}</span>`;
const avatar = (txt, kind = "", cls = "") => `<span class="avatar ${kind} ${cls}">${txt}</span>`;
const lbl = (name, color) => `<span class="label-pill"><span class="dot" style="background:var(--${color})"></span>${name}</span>`;
const pri = (p) => p === "urgent" ? '<span class="pri urgent" title="Urgent">!</span>' : `<span class="pri ${p}" title="${p}"><i></i><i></i><i></i></span>`;

const C = {};

C.Button = {
  group: "Actions", height: 120,
  readme: `Buttons trigger one action each; their label says what happens.

- \`primary\` (accent fill) for the one main action of a page or drawer: "New task", "Create project", "Save changes".
- Default (raised) for secondary actions: "Edit", "Add files", "Assign task".
- \`ghost\` for toolbar and low-emphasis actions: "Cancel", "Filters", icon-only buttons.
- \`danger\` (outline in \`danger\`) for Stop and destructive actions; destructive actions confirm in place, never with a browser dialog.
- \`sm\` (26px) inside panels, cards and run headers; \`icon-only\` needs an \`aria-label\`.
- A keyboard shortcut may follow the label as a \`kbd\`.

The consumer supplies the label, an optional leading Lucide icon (15px) and the handler.`,
  preview: doc([
    row(`<button class="btn primary">${ic("plus")}New task <span class="kbd">N</span></button>`, `<button class="btn">${ic("pencil")}Edit</button>`,
      `<button class="btn ghost">Cancel</button>`, `<button class="btn danger">${ic("square")}Stop</button>`, `<button class="btn" disabled>Disabled</button>`),
    `<div style="height:12px"></div>`,
    row(`<button class="btn sm primary">${ic("play")}Run</button>`, `<button class="btn sm">${ic("terminal")}Open in terminal</button>`,
      `<button class="btn sm ghost icon-only" aria-label="More">${ic("ellipsis")}</button>`, `<button class="btn icon-only" aria-label="Close">${ic("x")}</button>`,
      `<button class="link">View details ${ic("chevron-right", "icon sm")}</button>`),
  ].join("\n")),
};

C.SegmentedControl = {
  group: "Actions", height: 70,
  readme: `A segmented control switches between a few views of the same thing: list or board, light or dark.

Use icon segments with an \`aria-label\` (and \`aria-pressed\`) for views, short words for modes. One segment is always on.`,
  preview: doc(row(
    `<div class="seg" role="group" aria-label="View"><button aria-pressed="true" aria-label="List">${ic("list")}</button><button aria-pressed="false" aria-label="Board">${ic("columns-3")}</button></div>`,
    `<div class="seg" role="group" aria-label="Theme"><button aria-pressed="false" aria-label="Light">${ic("sun")}</button><button aria-pressed="true" aria-label="Dark">${ic("moon")}</button></div>`,
    `<div class="seg" role="group" aria-label="Density"><button aria-pressed="true">Comfortable</button><button aria-pressed="false">Compact</button></div>`)),
};

C.Sidebar = {
  group: "Navigation", height: 700,
  readme: `The sidebar is the app's spine: the organisation, quick actions, work areas, projects, the team's agents and company pages.

- The organisation tile lives in the rail to its left (initials on \`accent\`, a "+" below for more later).
- Order: New task (N), Search (Ctrl K), Inbox with a \`needs\` count; WORK (Chat, Tasks, Clients, Projects; Chat shows a teal "working" tag while the Team Lead answers); PROJECTS with colour dots and a "+"; AGENTS of the current team with a role icon and a teal "1 live" tag while working; COMPANY (Team, Users, Settings).
- Section labels use \`t-nav-label\` in capitals and \`text-3\`; items are 32px with a 16px icon in \`text-3\` that brightens on hover.
- The current page gets \`selected\`; nothing else is highlighted.
- Background \`side\`, dimmer than the canvas, with a \`line\` border on the right.

The consumer supplies the route, the counts and the lists of projects and agents.`,
  preview: `<div style="display:flex;height:668px"><nav style="width:56px;flex:none;display:flex;flex-direction:column;align-items:center;padding:12px 0;background:var(--rail);border-right:1px solid var(--line)"><span class="org-tile" style="width:36px;height:36px;border-radius:var(--radius-l)">GZ</span></nav><aside class="side" style="height:auto">
<a class="nav-item">${ic("square-pen")}New task<span class="meta"><span class="kbd">N</span></span></a>
<a class="nav-item">${ic("search")}Search<span class="meta"><span class="kbd">Ctrl K</span></span></a>
<a class="nav-item">${ic("inbox")}Inbox<span class="meta"><span class="count alert">2</span></span></a>
<div class="nav-section"><div class="nav-label">Work</div>
<a class="nav-item">${ic("messages-square")}Chat<span class="meta"><span class="live-tag"><span class="pulse"></span>working</span></span></a>
<a class="nav-item on" aria-current="page">${ic("list-todo")}Tasks</a><a class="nav-item">${ic("building-2")}Clients</a><a class="nav-item">${ic("folder-kanban")}Projects</a></div>
<div class="nav-section"><div class="nav-label">Projects<button aria-label="New project">${ic("plus", "icon sm")}</button></div>
<a class="nav-item"><span class="dot" style="background:var(--c-yellow)"></span>Kade portal</a><a class="nav-item"><span class="dot" style="background:var(--c-teal)"></span>Groene Fiets webshop</a></div>
<div class="nav-section"><div class="nav-label">Agents<button aria-label="Add agent">${ic("plus", "icon sm")}</button></div>
<a class="nav-item">${ic("server")}Backend Agent<span class="meta"><span class="live-tag"><span class="pulse"></span>1 live</span></span></a>
<a class="nav-item">${ic("monitor")}Frontend Agent</a><a class="nav-item">${ic("flask-conical")}QA Agent</a><a class="nav-item">${ic("crown")}Team Lead</a></div>
<div class="nav-section"><div class="nav-label">Company</div>
<a class="nav-item">${ic("network")}Team</a><a class="nav-item">${ic("users")}Users</a><a class="nav-item">${ic("settings")}Settings</a></div>
</aside><div style="flex:1;background:var(--bg)"></div></div>`,
};

C.Toolbar = {
  group: "Navigation", height: 150,
  readme: `The top bar and view toolbar sit above every list page; detail pages use the top bar only.

- Top bar (\`size-topbar\`): breadcrumbs on the left (links in \`text-2\`, the current page bold in \`text\`), page actions on the right.
- View toolbar: the page's create action on the left as a secondary button ("New task"), then on the right the list/board switch, Filters (accent and a count when active, with a clear button), Sort and Group as ghost buttons.

The consumer supplies the crumbs, the actions and the view state.`,
  preview: `<div style="background:var(--bg)"><div class="topbar"><div class="crumbs"><a href="#">Kade portal</a><span class="sep">/</span><b>Tasks</b><span class="faint">12</span></div>
<div class="actions"><button class="btn ghost sm">${ic("copy")}Copy link</button></div></div>
<div class="toolbar"><button class="btn">${ic("plus")}New task</button><span class="spacer"></span>
<div class="seg" role="group" aria-label="View"><button aria-pressed="true" aria-label="List">${ic("list")}</button><button aria-pressed="false" aria-label="Board">${ic("columns-3")}</button></div>
<button class="btn ghost on">${ic("list-filter")}Filters: 1</button><button class="btn ghost sm icon-only" aria-label="Clear filters">${ic("x")}</button>
<button class="btn ghost">${ic("arrow-up-down")}Sort</button><button class="btn ghost">${ic("layers")}Group</button></div></div>`,
};

C.Tabs = {
  group: "Navigation", height: 70,
  readme: `Tabs switch between sections of one page: Comments, Activity and Runs on a task; Overview, Runs, Instructions and Settings on an agent.

Each tab has a 16px icon, a label and an optional count in \`text-3\`; the current tab has a 2px \`text\` underline.`,
  preview: doc(`<div class="tabs" role="tablist"><button class="tab" role="tab" aria-selected="true">${ic("message-square")}Comments <span class="n">3</span></button>
<button class="tab" role="tab" aria-selected="false">${ic("activity")}Activity</button><button class="tab" role="tab" aria-selected="false">${ic("play")}Runs <span class="n">2</span></button></div>`),
};

C.StatusIcon = {
  group: "Data display", height: 120,
  readme: `One glyph per column category, so status reads by shape as well as colour (colour-blind safe).

Backlog dashed circle, To do open circle, In progress half circle, Testing three-quarter circle, Review circle with a dot (needs you), Deploy circle with an up arrow (merged, waits for you to deploy it), Done filled check, On hold circle with a minus, Cancelled struck circle. Columns can be renamed; the glyph follows the column's category, never its name. Always 16px, with the column name as its accessible label. A card's hold overrides its category glyph.`,
  preview: doc(`<div style="display:grid;grid-template-columns:repeat(5,max-content);gap:14px 32px">${[
    ["backlog", "Backlog"], ["ready", "To do"], ["in_progress", "In progress"], ["testing", "Testing"], ["review", "Review"],
    ["deploy", "Deploy"], ["done", "Done"], ["hold", "On hold"], ["cancelled", "Cancelled"]].map(([c, t]) => `<span class="who" style="color:var(--text)">${st(c, t)}${t}</span>`).join("")}</div>`),
};

C.Badge = {
  group: "Data display", height: 150,
  readme: `Badges label a state in a word; label pills show task labels; ID chips show machine IDs.

- \`live\` (teal, with a pulse): an agent is working now. Only for that.
- \`ok\`, \`fail\`, \`warn\`: run outcomes ("succeeded", "failed", "timed out"). \`info\`: how a run started ("Heartbeat", "Manual", "Assigned"). \`needs\`: waiting for you.
- Label pills: an outlined pill with the label's colour dot and name; as a toggle (\`button[aria-pressed]\`) in pickers.
- \`chip-id\` for run IDs; \`.id\` (mono, \`text-3\`) for task IDs.
- Priority: grey bars; only Urgent is a coloured square.`,
  preview: doc([
    row(live(), `<span class="badge ok">succeeded</span>`, `<span class="badge fail">failed</span>`, `<span class="badge warn">timed out</span>`, `<span class="badge info">Heartbeat</span>`, `<span class="badge needs">Review</span>`, `<span class="badge">cancelled</span>`),
    `<div style="height:12px"></div>`,
    row(lbl("frontend", "c-blue"), lbl("backend", "c-orange"), lbl("qa", "c-violet"), lbl("bug", "c-red"), `<span class="chip-id">2fc1b863</span>`, `<span class="id">KADE-41</span>`),
    `<div style="height:12px"></div>`,
    row(pri("none"), `<span class="faint">None</span>`, pri("low"), `<span class="faint">Low</span>`, pri("medium"), `<span class="faint">Medium</span>`, pri("high"), `<span class="faint">High</span>`, pri("urgent"), `<span class="faint">Urgent</span>`),
  ].join("\n")),
};

C.Avatar = {
  group: "Data display", height: 90,
  readme: `Avatars show who: people are circles with initials, agents are rounded squares (accent tint), so a glance tells a person from an agent.

Sizes: \`sm\` (18px) on cards, default (22px) in rows, \`lg\` (32px) in comments, \`xl\` (48px) in entity headers, where an agent shows its role icon instead of initials. Pair an avatar with the name except on dense cards.`,
  preview: doc(row(avatar("JE"), `<span class="who">${avatar("SB", "", "sm")}Sanne Bakker</span>`, `<span class="who">${avatar("BA", "agent")}Backend Agent</span>`, avatar("FA", "agent", "lg"), avatar("JE", "", "lg"),
    `<span class="avatar agent xl">${ic("server")}</span>`, `<span class="avatar agent xl">${ic("crown")}</span>`)),
};

const trow = (cat, id, title, labels, who, liveOn, date, extra = "") =>
  `<a class="task-row${extra}">${st(cat, cat)}<span class="id">${id}</span><span class="title"><span>${title}</span></span><span class="labels">${labels}</span>${who}<span>${liveOn ? live() : ""}</span><span class="date">${date}</span></a>`;
C.TaskList = {
  group: "Data display", height: 330,
  readme: `The task list: rows grouped by column, in workflow order (Paperclip's issues list).

- Group head: chevron (collapses), the column's glyph, its name in \`t-group\` capitals, the count, and "+" to add a task in that column. Done and Cancelled start collapsed.
- Row (\`size-row\`): glyph, ID (\`.id\`), title (\`t-row\`, one line, ellipsis) with label pills, assignee (avatar + name, or a faint "Assignee"), a Live badge while an agent works, and the last update right-aligned in \`text-3\`.
- J/K move the focus (the focused row gets \`selected\` and an accent edge), Enter opens.

The consumer supplies tasks grouped by column, the assignee names and the live run set.`,
  preview: `<div class="task-list" style="background:var(--bg);padding-top:6px">
<div class="group-head" aria-expanded="true">${ic("chevron-down", "icon sm chev")}${st("in_progress", "In progress")}<span class="name">In progress</span><span class="n">2</span><span class="add"><button class="btn ghost sm icon-only" aria-label="New task in In progress">${ic("plus")}</button></span></div>
${trow("in_progress", "KADE-1", "Export invoices as CSV from the portal", lbl("backend", "c-orange"), `<span class="who">${avatar("BA", "agent", "sm")}Backend Agent</span>`, true, "6m ago", " focus")}
${trow("in_progress", "GFW-1", "Mollie webhook: retry failed payment updates", lbl("backend", "c-orange") + lbl("bug", "c-red"), `<span class="who none">${ic("user", "icon sm")}Assignee</span>`, false, "Oct 4")}
<div class="group-head" aria-expanded="true">${ic("chevron-down", "icon sm chev")}${st("ready", "To do")}<span class="name">To do</span><span class="n">1</span></div>
${trow("ready", "KADE-3", "Filament table: remember column order per user", lbl("frontend", "c-blue"), `<span class="who none">${ic("user", "icon sm")}Assignee</span>`, false, "Oct 3")}
<div class="group-head" aria-expanded="false">${ic("chevron-down", "icon sm chev")}${st("done", "Done")}<span class="name">Done</span><span class="n">14</span></div>
</div>`,
};

C.BoardCard = {
  group: "Data display", height: 330,
  readme: `Kanban cards and columns for the board view.

- Column head: glyph, name in \`t-group\` capitals, count, "+"; an optional one-line note on who works the column. Columns are \`size-board-col\` wide; Backlog, Done and Cancelled collapse to 44px rails that still accept drops.
- Card: ID and priority, the title (\`fs\`, 500), then label pills and the assignee avatar. While an agent works the card shows a teal working strip with a pulse.
- At most 50 cards render per column, then "Show N more".`,
  preview: `<div class="board" style="background:var(--bg);padding-top:10px">
<div class="col rail"><div class="col-head">${st("backlog", "Backlog")} <span class="name">Backlog</span> <span class="n">4</span></div></div>
<div class="col"><div class="col-head">${st("ready", "To do")}<span class="name">To do</span><span class="n">2</span><span class="add"><button class="btn ghost sm icon-only" aria-label="Add">${ic("plus")}</button></span></div><div class="col-note">Auto: Frontend Agent and Backend Agent</div>
<div class="card"><div class="top"><span class="id">KADE-3</span>${pri("medium")}</div><div class="title">Filament table: remember column order per user</div><div class="meta">${lbl("frontend", "c-blue")}</div></div>
<div class="card"><div class="top"><span class="id">GFW-3</span>${pri("urgent")}</div><div class="title">Postcode and house number lookup on checkout</div><div class="meta">${lbl("frontend", "c-blue")}${avatar("SB", "", "sm")}</div></div></div>
<div class="col"><div class="col-head">${st("in_progress", "In progress")}<span class="name">In progress</span><span class="n">1</span></div><div class="col-note">Auto: Frontend Agent and Backend Agent</div>
<div class="card"><div class="top"><span class="id">KADE-1</span>${pri("high")}</div><div class="title">Export invoices as CSV from the portal</div><div class="working"><span class="pulse"></span>Backend Agent is working</div><div class="meta">${lbl("backend", "c-orange")}${avatar("BA", "agent", "sm")}</div></div></div>
</div>`,
};

const prop = (k, v, cls = "editable") => `<div class="prop-row"><span class="k">${k}</span><span class="v ${cls}">${v}</span></div>`;
C.PropertiesPanel = {
  group: "Data display", height: 430,
  readme: `The properties panel sits on the right of a task page (\`size-props\`), closable, with one row per property: a 96px label column in \`text-3\` and a value that is a picker on click.

Rows, in order: Status, Priority, Testing (a checkbox that saves at once), Labels, Assignee, Project, Branch; a separator; Started, Created, Updated. Empty values read "No labels", "Unassigned". Holds show as their own row with the reason and a "Clear hold" button.`,
  preview: `<div style="display:flex;justify-content:flex-end;background:var(--bg)"><aside class="props" style="height:400px">
<div class="props-head">Properties<button class="btn ghost sm icon-only" aria-label="Close properties">${ic("x")}</button></div>
<div class="props-body">
${prop("Status", st("in_progress", "In progress") + "In progress")}${prop("Priority", pri("medium") + "Medium")}${prop("Labels", lbl("backend", "c-orange"))}
${prop("Assignee", avatar("BA", "agent", "sm") + "Backend Agent")}${prop("Project", '<span class="dot" style="width:9px;height:9px;border-radius:50%;background:var(--c-yellow)"></span>Kade portal')}
${prop("Branch", '<span class="id" style="color:var(--text-2)">gizai/kade-1-export-invoices</span>', "")}
<div class="props-sep"></div>${prop("Started", "Oct 6, 2026", "")}${prop("Created", "Oct 2, 2026", "")}${prop("Updated", "6m ago", "")}
</div></aside></div>`,
};

C.RunCard = {
  group: "Data display", height: 420,
  readme: `Run cards show an agent run: the latest result, or the live transcript while it runs.

- Finished: outcome icon, outcome badge, the run ID chip, how it started (\`info\` badge), the time on the right, then the agent's summary.
- Live: a \`live\` border with a soft halo, "Live run" with a pulse, the agent, the run ID, Stop (danger) and Open in terminal; a monospace transcript (tool calls with the tool name in teal, errors in \`danger\`), and a footer with tokens, cost and branch.

The consumer supplies the run, its streamed events and the stop handler.`,
  preview: doc(`<div class="run-card"><div class="run-head"><span style="color:var(--success)">${ic("check")}</span><span class="badge ok">succeeded</span><span class="chip-id">2fc1b863</span><span class="badge info">Heartbeat</span><span class="right">2h ago</span></div>
<div class="run-summary">Exporter added with semicolon separators; 12 tests pass. Ready for testing.</div></div>
<div style="height:14px"></div>
<div class="run-card live"><div class="run-head"><span class="pulse"></span><span class="title">Live run</span><span class="who">${avatar("BA", "agent", "sm")}Backend Agent</span><span class="chip-id">9f7d5857</span><span class="right">4m 12s<button class="btn sm danger">${ic("square")}Stop</button><button class="btn sm ghost">${ic("terminal")}Open in terminal</button></span></div>
<div class="run-stream"><div>Reading the exporter.</div><div class="tool"><b>Read</b> app/Exports/InvoiceExporter.php</div><div class="tool"><b>Edit</b> app/Exports/InvoiceExporter.php</div><div class="tool"><b>Bash</b> php artisan test --filter=InvoiceExport</div><div class="ok">12 passed</div></div>
<div class="run-foot"><span>38k in · 4k out</span><span>$0.42</span><span class="mono">gizai/kade-1-export-invoices</span></div></div>`),
};

const bars = (days) => `<div class="bars">${days.map((d) => `<div class="day">${d.map(([k, h]) => `<div class="seg-${k}" style="height:${h}%"></div>`).join("")}</div>`).join("")}</div>`;
C.StatCard = {
  group: "Data display", height: 230,
  readme: `Metric cards with a 14-day bar chart, for agent pages and the dashboard: Run activity (succeeded vs failed), Success rate, Tasks by status.

Title in \`fs\` 600, the window in \`t-caption\`, bars that grow from the baseline (one column per day, stacked by outcome), a three-tick date axis and a legend when there is more than one series. Days without runs show a thin \`line\` stub so the axis stays readable.`,
  preview: doc(`<div class="stats"><div class="stat-card"><h4>Run activity</h4><span class="sub">Last 14 days</span>
${bars([[["none", 3]], [["ok", 30]], [["ok", 20], ["fail", 10]], [["none", 3]], [["ok", 45]], [["ok", 25]], [["ok", 35], ["fail", 15]], [["ok", 55]], [["ok", 30]], [["none", 3]], [["ok", 40], ["fail", 8]], [["ok", 60]], [["ok", 35]], [["ok", 50]]])}
<div class="axis"><span>9/23</span><span>9/30</span><span>10/6</span></div><div class="legend"><span><i style="background:var(--success)"></i>Succeeded</span><span><i style="background:var(--danger)"></i>Failed</span></div></div>
<div class="stat-card"><h4>Success rate</h4><span class="sub">Last 14 days</span>
${bars([[["none", 3]], [["ok", 80]], [["warn", 60]], [["none", 3]], [["ok", 100]], [["ok", 90]], [["warn", 70]], [["ok", 100]], [["ok", 85]], [["none", 3]], [["fail", 40]], [["ok", 100]], [["ok", 95]], [["ok", 100]]])}
<div class="axis"><span>9/23</span><span>9/30</span><span>10/6</span></div></div></div>`),
};

C.EntityHeader = {
  group: "Data display", height: 100,
  readme: `The header of an agent, project or client page: an \`xl\` avatar (an agent shows its role icon), the name in \`t-entity-title\`, one meta line in \`text-2\`, and the page's actions on the right.

For agents: Assign task, Run, Pause/Resume and a state badge (\`state\`: idle in \`warning\`, running in \`live\`, paused neutral).`,
  preview: doc(`<div class="entity-head"><span class="avatar agent xl">${ic("server")}</span><div class="names"><h1>Backend Agent</h1><p>Backend · Claude Code · every 15 min</p></div>
<div class="actions"><button class="btn">${ic("plus")}Assign task</button><button class="btn">${ic("play")}Run</button><button class="btn">${ic("pause")}Pause</button><span class="state">idle</span><button class="btn ghost icon-only" aria-label="More">${ic("ellipsis")}</button></div></div>`),
};

C.Field = {
  group: "Forms", height: 400,
  readme: `Fields and form sections, used inside drawers.

- A form is a stack of sections: a 220px intro column (\`t-section\` heading and one sentence in \`text-3\`) beside a two-column field grid. \`wide\` fields span both columns.
- A field is a label (\`fs-sm\`, \`text-2\`), the control (34px, \`bg\` with a \`line-2\` border, accent border and halo on focus) and an optional hint (\`text-3\`), warning (\`warning\`) or error (\`danger\`) below it.
- Native selects get the chevron; machine values (keys, paths, branches) use \`input mono\`.

The consumer supplies labels, values, validation messages and handlers.`,
  preview: doc(`<div class="form"><section class="form-section"><header><h3>Project</h3><p>Its name and task key. Tasks become KADE-1, KADE-2…</p></header>
<div class="fields"><div class="field"><label>Name</label><input class="input" value="Kade portal"></div>
<div class="field"><label>Task key</label><input class="input mono" value="KADE"><span class="hint">Fixed after creation</span></div>
<div class="field"><label>Client</label><select class="select"><option>Kade Logistics B.V.</option></select></div>
<div class="field"><label>Status</label><select class="select"><option>Active</option></select></div>
<div class="field"><label>Colour</label><div class="swatches"><button class="swatch" style="background:var(--c-blue)"></button><button class="swatch" aria-pressed="true" style="background:var(--c-yellow)"></button><button class="swatch" style="background:var(--c-teal)"></button><button class="swatch" style="background:var(--c-violet)"></button></div></div>
<div class="field"><label>Budget (€)</label><input class="input" value="12000"><span class="warn">Over the client's usual range</span></div></div></section>
<section class="form-section"><header><h3>Repository</h3><p>Agents work in their own git worktree of this repository.</p></header>
<div class="fields"><div class="field wide"><label>Git repository</label><div class="input-group"><input class="input mono" value="/home/jeffrey/Code/kade-portal"><button class="btn">Choose…</button></div><span class="hint">git repository · on branch main</span></div>
<label class="check"><input type="checkbox" checked>Agents may push this branch</label></div></section></div>`),
};

C.MarkdownEditor = {
  group: "Forms", height: 300,
  readme: `Every Markdown field uses this editor: a toolbar above a live-preview surface (CodeMirror 6).

Toolbar, in groups: heading, bold, italic, quote | code, link | numbered list, bullet list, checklist | rule, table. Buttons are 30×28 icon buttons with a title and keyboard hint (Ctrl+B, Ctrl+I, Ctrl+K for a link); a button shows pressed when the cursor sits in that format. The right edge shows the save hint. The surface dims Markdown marks off the cursor line, renders checklists as clickable boxes, and shows task IDs and @mentions as chips. Ctrl+Enter or Ctrl+S saves, Escape cancels.

The consumer supplies the text, onChange, onSave and an optional placeholder and minimum height.`,
  preview: doc(`<div class="md-editor"><div class="md-toolbar" role="toolbar" aria-label="Formatting">
<button title="Heading" aria-label="Heading">${ic("heading")}</button><button title="Bold (Ctrl+B)" aria-label="Bold" aria-pressed="true">${ic("bold")}</button><button title="Italic (Ctrl+I)" aria-label="Italic">${ic("italic")}</button><button title="Quote" aria-label="Quote">${ic("text-quote")}</button><span class="sep"></span>
<button title="Code" aria-label="Code">${ic("code")}</button><button title="Link (Ctrl+K)" aria-label="Link">${ic("link")}</button><span class="sep"></span>
<button title="Numbered list" aria-label="Numbered list">${ic("list-ordered")}</button><button title="Bullet list" aria-label="Bullet list">${ic("list")}</button><button title="Checklist" aria-label="Checklist">${ic("list-checks")}</button><span class="sep"></span>
<button title="Divider" aria-label="Divider">${ic("minus")}</button><button title="Table" aria-label="Table">${ic("table")}</button>
<span class="right">Ctrl+Enter saves · Esc cancels</span></div>
<div class="md-surface prose"><h3>Context</h3><p>Export invoices as CSV with <b>semicolons</b>, so they open in Excel NL. See <code>KADE-12</code>.</p>
<ul><li class="task-list-item"><input type="checkbox" checked>List and filter invoices</li><li class="task-list-item"><input type="checkbox">CSV export with semicolons</li></ul></div></div>`),
};

C.Drawer = {
  group: "Overlays", height: 600, extra: " width=1200",
  readme: `Every create and edit form opens in a drawer from the right: at least \`size-drawer\` (1024px) wide, full height, over a \`scrim\`.

- Head: the title (\`t-drawer-title\`), one sentence of context, and a close button. Escape or a click on the scrim closes it; unsaved input asks in place before closing.
- Body: scrolls; holds a \`form\` of sections (see Field).
- Foot: sticky; a keyboard hint on the left, then Cancel (ghost) and the primary action, named for what it does ("Create project", "Save changes", "Add agent").
- \`wide\` (1280px) for editors that need room: agent settings, docs.

The consumer supplies the title, the form and the actions.`,
  preview: `<div style="position:relative;height:568px;overflow:hidden;background:var(--bg)">
<div style="padding:20px 24px;color:var(--text-3)">Projects</div>
<div class="scrim" style="position:absolute"></div>
<div class="drawer" style="position:absolute;width:1024px">
<div class="drawer-head"><div class="titles"><h2>New project</h2><p>Projects hold tasks, docs and files. Agents work on projects linked to a git repository.</p></div><button class="btn ghost icon-only" aria-label="Close">${ic("x")}</button></div>
<div class="drawer-body"><div class="form"><section class="form-section"><header><h3>Project</h3><p>Its name, client and task key.</p></header>
<div class="fields"><div class="field"><label>Name</label><input class="input" value="Stapsgewijs"></div><div class="field"><label>Task key</label><input class="input mono" value="STAP"><span class="hint">Tasks become STAP-1, STAP-2…</span></div>
<div class="field"><label>Client</label><select class="select"><option>Kade Logistics</option></select></div><div class="field"><label>Status</label><select class="select"><option>Active</option></select></div></div></section>
<section class="form-section"><header><h3>Repository</h3><p>Agents work in their own git worktree of it.</p></header>
<div class="fields"><div class="field wide"><label>Git repository</label><div class="input-group"><input class="input mono" value="/home/jeffrey/Herd/stapsgewijs"><button class="btn">Choose…</button></div><span class="hint">git repository · on branch feature/fortify-auth</span></div></div></section></div></div>
<div class="drawer-foot"><span class="hint">Ctrl+Enter creates</span><button class="btn ghost">Cancel</button><button class="btn primary">Create project</button></div></div></div>
<style>.drawer{animation:none}.scrim{animation:none}</style>`,
};

C.EmptyState = {
  group: "Feedback", height: 230,
  readme: `Empty states invite one action: a 20px icon, a bold first phrase, one sentence on what goes here, and the button that fills it.

Never "No data". Use the noun the user knows ("No agents yet", "Nothing needs you").`,
  preview: doc(`<div class="empty">${ic("bot")}<span><b>No agents yet.</b> Add one per job: a Frontend, a Backend and a QA agent is a good start. Each runs Claude Code in its own git worktree.</span><button class="btn primary">${ic("plus")}Add agent</button></div>
<div style="height:12px"></div><div class="empty">${ic("inbox")}<span><b>Nothing needs you.</b> Cards on hold and cards waiting for your review or deploy show up here.</span></div>`),
};

C.Toast = {
  group: "Feedback", height: 80,
  readme: `A toast confirms or reports the result of an action that happened elsewhere (a drag on the board, a background run), bottom right, for 5 seconds.

Lead with the outcome, name the thing: "Moved KADE-3 to In progress", "Couldn't move KADE-3: the task has an active run".`,
  preview: `<div style="position:relative;height:64px;background:var(--bg)"><div class="toast" style="position:absolute">${ic("circle-alert")}<span>Couldn't move KADE-3: the task has an active run</span></div></div>`,
};

C.ChatMessage = {
  group: "Chat", height: 360,
  readme: `A conversation with the Team Lead: your messages on the right, its answers on the left, its tool calls in between.

- Your messages are a \`raised\` bubble with a \`line\` border, plain text with line breaks, at most 560px wide, the time under the last one.
- The Team Lead's run of messages shares one header: a small agent avatar, its name in bold, the time in \`text-3\`. Text renders as Markdown (\`prose\`), indented 30px under the header.
- While it writes, the text so far streams in with a teal caret, and a teal "Thinking… / Writing… / Using create task…" row with a pulse sits at the bottom: teal means an agent is working right now.
- System notes (stopped, errors) are centred \`text-3\` lines with an icon; errors are \`danger\` and get \`role="alert"\`.
- A new speaker or five quiet minutes start a new group.

The consumer supplies the messages, the live draft and the running tool.`,
  preview: `<div style="padding:20px 24px;background:var(--bg)">
<div class="chat-group user"><div class="chat-msg user"><div class="bubble">Add a task to Kade portal for the backend agent: drivers want to export their invoices as CSV.</div></div><div class="chat-meta">2m ago</div></div>
<div class="chat-group agent"><div class="chat-head">${avatar("TL", "agent")}<b>Team Lead</b><span class="faint">2m ago</span></div>
<div class="chat-body"><div class="chat-msg agent"><div class="prose"><p>I'll add it to Kade portal with the backend label, so routing hands it to the Backend Agent.</p></div></div>
<div class="tool-card ok"><div class="tc-row"><span class="tc-state">${ic("check", "icon sm")}</span><span class="tc-verb">Created task</span><a class="tc-label" href="#">KADE-5 Export invoices as CSV</a><button class="tc-more" aria-label="Show details">${ic("chevron-right", "icon sm")}</button></div></div>
<div class="chat-msg agent draft"><div class="prose"><p>Done: it is in To do with three acceptance</p></div><span class="caret"></span></div>
<div class="chat-working"><span class="pulse"></span>Writing…</div></div></div>
<div class="chat-note">${ic("info", "icon sm")}<span>Stopped.</span></div>
</div>`,
};

C.ToolCard = {
  group: "Chat", height: 230,
  readme: `One line per tool the Team Lead calls: what it did, linked to what it made, so every change in a chat is one click from the item.

- A verb in \`text-2\` ("Created task", "Read the inbox", "Moved task") and the target in \`text\`, bold: the item's identifier and title from the tool's result, or what the input named. Writes link to the item; reads show a count ("3 items").
- State on the left: a teal pulse while it runs, a \`text-3\` check when done, a \`danger\` alert icon and the reason under the line when it failed.
- The chevron on the right opens the input and the result as formatted JSON in \`font-mono\` at \`fs-xs\`.
- \`bg\` surface, \`line\` border, \`radius\`, at most 560px wide.`,
  preview: doc([
    `<div class="tool-card running"><div class="tc-row"><span class="tc-state"><span class="pulse"></span></span><span class="tc-verb">Opened task</span><span class="tc-label">KADE-3</span><button class="tc-more" aria-label="Show details">${ic("chevron-right", "icon sm")}</button></div></div>`,
    `<div class="tool-card ok"><div class="tc-row"><span class="tc-state">${ic("check", "icon sm")}</span><span class="tc-verb">Read the inbox</span><span class="tc-label">3 items</span><button class="tc-more" aria-label="Show details">${ic("chevron-right", "icon sm")}</button></div></div>`,
    `<div class="tool-card error"><div class="tc-row"><span class="tc-state">${ic("circle-alert", "icon sm")}</span><span class="tc-verb">Moved task</span><span class="tc-label">KADE-3</span><button class="tc-more" aria-label="Show details">${ic("chevron-right", "icon sm")}</button></div><div class="tc-error">No column called "Doing". Columns: Backlog, To do, In progress, Testing, Review, Done.</div></div>`,
    `<div class="tool-card ok"><div class="tc-row"><span class="tc-state">${ic("check", "icon sm")}</span><span class="tc-verb">Created project</span><a class="tc-label" href="#">Kade portal (KP)</a><button class="tc-more" aria-expanded="true" aria-label="Hide details">${ic("chevron-right", "icon sm")}</button></div><div class="tc-detail"><div class="tc-h">Input</div><pre>{ "name": "Kade portal", "client": "Kade Logistics" }</pre></div></div>`,
  ].join('<div style="height:8px"></div>')),
};

C.ChatComposer = {
  group: "Chat", height: 150,
  readme: `Where you write to the Team Lead, pinned under the conversation.

- A \`raised\` box with \`radius-l\`; it grows with the text up to 200px; focus shows the \`accent\` ring.
- Enter sends, Shift+Enter adds a line (shown under the box as \`kbd\` hints). The send button is a small \`primary\` icon button, disabled while empty.
- While the Team Lead answers, Send becomes **Stop**: outlined in \`live\`, because it stops work in progress.
- A banner above the box explains why it can't send (the Team Lead is paused, Claude Code is missing) and offers the fix.`,
  preview: `<div style="padding:16px 24px;background:var(--bg)"><div class="chat-banner"><span>Team Lead is paused, so it can't answer.</span><button class="btn sm">Resume</button></div>
<div class="composer-box"><textarea rows="1" aria-label="Message">Plan the Kade portal as tasks</textarea><button class="btn primary sm icon-only" aria-label="Send">${ic("arrow-up")}</button></div>
<div class="composer-hint"><span><span class="kbd">Enter</span> sends</span><span><span class="kbd">Shift</span> <span class="kbd">Enter</span> new line</span></div></div>`,
};

C.ChatSetup = {
  group: "Feedback", height: 330,
  readme: `What the Chat page shows before there is a Team Lead: one centred panel that explains what chat is for and opens the agent form with Chat turned on.

- A 44px \`accent-soft\` tile with the chat icon, a heading in \`fs-lg\`, two lines in \`text-2\`, one \`primary\` button with a crown, and a \`text-3\` line on what the Team Lead can't do.
- The copy names what the user gets ("add clients, projects and tasks, set up agents, write docs, read your inbox"), never the mechanism.`,
  preview: `<div class="chat-setup" style="background:var(--bg)"><div class="panel setup-card"><span class="setup-icon">${ic("messages-square", "icon lg")}</span><h2>Chat with your Team Lead</h2>
<p class="muted">To start chatting, set up the Team Lead: an agent with Chat turned on. You can ask it to add clients, projects and tasks, set up agents, write docs and read your inbox.</p>
<button class="btn primary">${ic("crown")}Set up the Team Lead</button><p class="faint small">It hands code work to your agents as tasks; it doesn't edit code itself.</p></div></div>`,
};

C.OrgChart = {
  group: "Data display", height: 330,
  readme: `The team as an org chart: the Team Lead on top, the team's branches below (Design, Development, Quality, Operations and the ones you add, then Specialists for other roles), drawn from the agents' roles.

- Nodes are \`radius-pill\` cards on \`raised\`: a round role icon, the name in bold, a status dot and "Claude Code" (or "Chat · Claude Code" for the Team Lead). Grey dot when idle, \`warning\` when paused (and the node dims), \`live\` when working.
- An agent working right now gets a teal ring and a teal "Working" badge on its top edge; teal still means only that.
- Every branch ends in one empty spot: a dashed, transparent node ("Frontend · Add an agent"); hovering turns it \`accent\`; a click opens the agent form with the branch's role. Add branch, a dashed node at the end, asks a name; a branch's × removes it while it has no agents.
- Agent nodes are what you drag onto a column in Team → Workflow; the dragged agent follows the pointer as a chip with \`shadow-drag\`.
- Connectors are 1px \`line-2\` with \`radius\` corners; branch names sit on the line as small \`text-2\` labels.
- Wider than the panel, it scrolls sideways; it never shrinks the nodes.`,
  preview: `<div style="background:var(--bg)"><div class="org-scroll"><div class="org">
<div class="org-top"><a class="org-node live"><span class="badge live org-badge"><span class="pulse"></span>Working</span><span class="org-icon">${ic("crown")}</span><span class="org-text"><b>Team Lead</b><span><i class="org-dot live"></i>Chat · Claude Code</span></span></a></div>
<div class="org-stem"></div><div class="org-depts">
<div class="org-dept"><div class="org-dept-name">Development</div><div class="org-dept-nodes"><a class="org-node"><span class="org-icon">${ic("server")}</span><span class="org-text"><b>Backend Agent</b><span><i class="org-dot"></i>Claude Code</span></span></a><button class="org-node ghost"><span class="org-icon">${ic("plus")}</span><span class="org-text"><b>Frontend</b><span>Add an agent</span></span></button></div></div>
<div class="org-dept"><div class="org-dept-name">Quality</div><div class="org-dept-nodes"><a class="org-node paused"><span class="org-icon">${ic("flask-conical")}</span><span class="org-text"><b>QA Agent</b><span><i class="org-dot paused"></i>Paused</span></span></a></div></div>
</div></div></div></div>`,
};

// ---------- write components ----------
for (const [name, c] of Object.entries(C)) {
  write(`components/${name}/README.md`, `${c.readme}\n`);
  write(`components/${name}/preview.html`, card(c.group, c.height, c.extra ?? "") + c.preview);
}

// ---------- cover ----------
write("components/Cover/preview.html", `<!-- @dsCard height=288 -->
<div class="cover">
<svg viewBox="0 0 960 288" width="960" height="288" aria-hidden="true">
<!-- blocks: accent 216x152, live 112x240, needs 96x96, st-progress 96x128, selected 216x72 (spacing multiples of space-2), corners radius-l
     arrangement: a flush modular grid from x=480 to the right edge, gutters space-4
     pattern: the status ring row (literal motif): Gizai is read through its status glyphs, so rings, half and three-quarter discs cut out of the blocks in bg
     steps and radii: ring radius 18 = space-4 + 2, pitch 48 = space-8 + space-4, stroke 4, block radius radius-l -->
<rect class="b-accent tile" x="480" y="24" width="216" height="152"/>
<rect class="b-live tile" x="712" y="24" width="112" height="240"/>
<rect class="b-needs tile" x="840" y="24" width="96" height="96"/>
<rect class="b-progress tile" x="840" y="136" width="96" height="128"/>
<rect class="b-tint tile" x="480" y="192" width="216" height="72"/>
<g class="cut">
<circle cx="520" cy="100" r="18" stroke-dasharray="7 6"/><circle cx="568" cy="100" r="18"/>
<circle cx="616" cy="100" r="18"/><path class="fill" d="M616 88a12 12 0 0 1 0 24z"/>
<circle cx="664" cy="100" r="18"/><path class="fill" d="M664 100V88a12 12 0 1 1-12 12z"/>
<circle cx="768" cy="72" r="18"/><circle class="fill" cx="768" cy="72" r="7"/>
<circle cx="768" cy="216" r="18"/><path d="M760 216.5l6 6 11-11"/>
<circle cx="888" cy="72" r="18"/><circle class="fill" cx="888" cy="72" r="7"/>
<circle cx="888" cy="200" r="18"/><path class="fill" d="M888 188a12 12 0 0 1 0 24z"/>
</g>
</svg>
<div class="txt"><div class="name">Gizai</div><div class="tag">Tasks, projects and the agents that work on them.</div></div>
</div>
<style>
.cover{position:relative;width:960px;height:288px;background:var(--bg);overflow:hidden}
.cover svg{position:absolute;inset:0}
.tile{rx:var(--radius-l)}
.b-accent{fill:var(--accent)}.b-live{fill:var(--live)}.b-needs{fill:var(--needs)}.b-progress{fill:var(--st-progress)}.b-tint{fill:var(--selected)}
.cut circle,.cut path{fill:none;stroke:var(--bg);stroke-width:4;stroke-linecap:round;stroke-linejoin:round}
.cut .fill{fill:var(--bg);stroke:none}
.txt{position:absolute;left:32px;bottom:28px;max-width:440px}
.name{font-family:var(--font-sans);font-weight:700;font-size:120px;line-height:.92;letter-spacing:-.03em;color:var(--text)}
.tag{margin-top:14px;font-family:var(--font-sans);font-size:14px;color:var(--text-2)}
</style>
`);

// ---------- icons (asset uploads; single ink) ----------
const INK = "#8a8e98";
for (const name of Object.keys(nodes)) {
  write(`assets/Icons/${name}.svg`, `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="${INK}" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">${inner(name)}</svg>\n`);
}
for (const [cat, g] of Object.entries(GLYPH)) {
  const colors = { backlog: "#8a8e98", ready: "#60a5fa", in_progress: "#f0b429", testing: "#a98bfa", review: "#f472b6", deploy: "#f472b6", done: "#4cc38a", hold: "#f2706b", cancelled: "#6c707a" };
  write(`assets/Icons/status-${cat.replace("_", "-")}.svg`, `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" style="color:${colors[cat]}">${g.replaceAll("currentColor", colors[cat]).replace("var(--bg)", "#0f1013")}</svg>\n`);
}
write("assets/Icons/README.md", `Lucide icons (lucide-react 0.577, ISC licence), copied from Lucide's own icon data, plus Gizai's nine status glyphs (\`status-*.svg\`).

These files are drawn in one ink, \`#8a8e98\` (\`c-grey\`), so they show on both themes here. In the app every icon uses \`currentColor\` at a 1.75 stroke, 16px, and takes its colour from the text it sits in; status glyphs take their \`st-*\` colour. See the Iconography section of the README for which icon means what.
`);

// ---------- fonts ----------
fs.mkdirSync(path.join(OUT, "fonts"), { recursive: true });
for (const f of ["AtkinsonHyperlegibleNext-400-latin.woff2", "AtkinsonHyperlegibleNext-500-latin.woff2", "AtkinsonHyperlegibleNext-600-latin.woff2",
  "AtkinsonHyperlegibleNext-700-latin.woff2", "AtkinsonHyperlegibleMono-400-latin.woff2", "AtkinsonHyperlegibleMono-500-latin.woff2"]) {
  fs.copyFileSync(`${GZ}/src/assets/fonts/${f}`, path.join(OUT, "fonts", f));
}
fs.copyFileSync(path.join(HERE, "tokens.json"), path.join(OUT, "tokens.json"));

// Export the glyphs and icon list for the app.
fs.writeFileSync(path.join(HERE, "glyphs.json"), JSON.stringify(GLYPH, null, 2));
console.log("components:", Object.keys(C).length, "icons:", Object.keys(nodes).length, `+ ${Object.keys(GLYPH).length} status glyphs`);
