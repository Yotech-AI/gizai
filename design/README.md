Gizai is a desktop app for agentic software development: clients, projects, tasks and a team of
local AI coding agents, used all day by one developer. The interface is an operational console:
dense, dark, calm, and honest about what agents are doing. Paperclip and Linear are the reference;
Gizai keeps their scanning density and adds its own meaning for colour.

## Content fundamentals

- **Plain words from the user's side.** A task, a project, a run, an agent. Never "issue",
  "ticket", "job" or "worker". Columns are Backlog, To do, In progress, Testing, Review, Done.
- **Sentence case everywhere**, including buttons and headings: "New task", "Add agent",
  "Latest run". Capitals only for list group heads and sidebar section labels (`t-group`,
  `t-nav-label`), which are set uppercase in CSS, never typed in capitals.
- **Buttons say what happens:** "Create project", "Save changes", "Run", "Stop", "Add files".
  The result echoes the verb: "Saved", "Stopped".
- **Errors say what went wrong and how to fix it**, without apology:
  "Link a git repository to Kade portal first (project page → Edit)."
  "Claude Code not found: set its path in Settings."
- **Empty states invite one action:** "No agents yet. Add one per job: a Frontend, a Backend and a
  QA agent is a good start."
- **Machine values are monospace** (`t-id`, `t-code`): task IDs (`KADE-41`), run IDs, branches
  (`gizai/kade-41-export-invoices`), paths, costs in tables.
- English UI; Dutch business data (KvK, BTW, IBAN) keeps its Dutch field names.
- No emoji in the interface.

## Visual foundations

**Dark first.** The `dark` theme is the default; `light` swaps the same token names (Settings →
Appearance, or the t key). Surfaces go from dim to bright in this order: `side` → `bg` (the canvas) →
`raised` (cards, panels) → `overlay` (drawers, palette). The sidebar is dimmer than the canvas so the work stands out.
Separate regions with 1px borders in `line`; use `line-2` for controls (inputs, buttons, pills)
and the drawer edge. Shadows only on things that float: `shadow-pop`, `shadow-drawer`,
`shadow-drag`.

**Colour carries meaning, never decoration.**
- `accent` (blue) is the one interactive colour: primary buttons, links, focus, selection.
- `live` (teal) means only "an agent is working on this right now": the pulse dot, the Live badge,
  the working strip on a card, the border of a live run card. Never use teal for anything else.
- `needs` (magenta) means "a person is needed": Review, holds, the Inbox count.
- The terminal's own colours (`term-*`) are what programs print, not Gizai's states: `term-cyan` is a sky blue and
  `term-magenta` a violet, at least 35° of hue (OKLCH) from `live` and `needs`, so a program's cyan never reads as an
  agent at work, nor its magenta as "needs you".
- The mountains along the bottom of the window (`band-*`, GA-139) are the window's own light, not a state: one cool
  blue, at least 35° of hue (OKLCH) from `live` and `needs` and 30° from `accent`, low in chroma and alpha. They never
  move and never run behind code; text and controls on them keep their contrast (`design/branding/README.md`).
- `success`, `warning`, `danger` are run and form outcomes. Each badge pairs its colour with a word.
- Status is shown by glyph shape *and* hue (see StatusIcon), so it reads without colour:
  Backlog dashed circle `st-backlog`, To do open circle `st-todo`, In progress half circle
  `st-progress`, Testing three-quarter circle `st-testing`, Review circle with a dot `st-review`,
  Done filled check `st-done`, On hold circle with a minus `st-blocked`, Cancelled struck circle
  `st-cancelled`. Task status and run state are separate signals: a card can be In progress with no
  live run.
- Priority is grey bars; only Urgent is coloured (`pr-urgent`).
- Labels and projects pick from the decorative `c-*` colours, always shown as a dot next to the
  name.

**Code and diffs.** Code (the Changes view, the Editor, code in Markdown) has colours of its own: `syn-*` for syntax and
`diff-*` for changes. They appear only inside code: never on buttons, badges, status, labels or charts.

- **Changes are green and red of their own.** An added line has `diff-add-soft` behind it and a removed line
  `diff-del-soft`; the characters that changed get `diff-add-strong` or `diff-del-strong` on top. The + and − marks, a
  changed line's number, the A and D letters and the counts (`+964 −112`, in mono) are `diff-add` and `diff-del`. A fold
  bar is `diff-fold` with `text-2`. They're near `success` and `danger` but separate: those mean run and form outcomes,
  so neither stands in for the other.
- **Git's marks** on a file in the Editor's tree, tabs and status bar: A and U are `git-added` (`diff-add`), D and C
  `git-deleted` (`diff-del`), M and R gold `git-modified`, which is also the dot on a folder with changes inside.
- **Syntax uses four hues and the greys.** Violet `syn-keyword` (and `syn-meta`), blue `syn-function` (and `syn-tag`),
  gold `syn-type` (and `syn-attribute`), orange `syn-string` (and `syn-number`). Names stay `text`, operators and
  punctuation `text-2`, comments `syn-comment`. `syn-invalid` is `danger` with a wavy underline.
- **No teal or magenta in code**: they mean an agent at work and "needs you". **No green or red in syntax**: they mean
  added and removed.
- **Contrast:** every syntax and diff text colour is at least 4.5:1 on `bg` and `raised` in both themes, and on changed
  lines; on a changed word every one but comments (3.4:1 in dark) and `syn-invalid`. Check a new value against all four
  before it goes in.
- **Never colour alone:** changed lines keep their + or −, removed lines their old number, status letters their word
  (tooltip and screen readers), and "Not committed" is written in the file's header.
- **Type:** code is `t-code` in `--font-mono`, which Settings → Appearance changes as everywhere. No italic or bold for
  syntax: the bundled fonts have no italic, and colour already does the work. In text, write arrows with Lucide's
  `arrow-right`: → isn't in Atkinson Hyperlegible's files (↑ and ↓ are).
- **In the code:** `src/lib/codeTheme.ts` maps Lezer's highlighting tags to these tokens (the table in
  `code-editor/README.md`) and gives CodeMirror the mono font; `src/lib/codeLanguages.ts` picks a file's language by
  its name, and `src/lib/blade.ts` is Blade's.

**Type.** One family, Atkinson Hyperlegible Next (`--font-sans`), chosen for long days of reading
code-adjacent text; Atkinson Hyperlegible Mono (`--font-mono`) for machine values. Use the styles,
not ad-hoc sizes: `t-body` (13.5px) for UI, `t-row` (14px, 500) for task titles in lists,
`t-prose` (15px/24px) for rendered Markdown, `t-small` for meta, `t-caption` for hints,
`t-group` for group heads, `t-entity-title` for agent/project/client names, `t-task-title` on the
task page, `t-drawer-title` in drawers.

Settings → Appearance lets the user pick another font for the whole app, each setting both
families: JetBrains Mono, Inter (code in JetBrains Mono), Geist (code in Geist Mono) or Hack. All
ship with the app. It also has three text sizes, in px of each part's main text: Chat (15 by
default), Interface (13.5, `fs`) and Tasks and docs (15, `t-prose`). Reading text grows by the whole
step, headings by about half, and small things (IDs, label pills, times, badges, keyboard hints,
group labels, avatars, icons) stay the same or grow at most 1px. Rows, the sidebar, board columns
and the chat column grow with the text. So: size text with the `fs*` tokens, and give a size that
should never grow its own px value (see TextSizes).

**Spacing and shape.** A 4px grid: `space-1` … `space-8`. List pages use a `space-5` (20px)
gutter; detail pages `space-6`–`space-8`. Rows are `size-row` (40px). Radii by role: `radius-s` for
chips and small controls, `radius` for buttons, inputs, rows and nav items, `radius-l` for cards
and panels, `radius-pill` for badges. Never round everything the same.

**Layout.** Three zones: the sidebar (`size-side`), the main column, and on detail pages a
properties panel (`size-props`) that can be closed. There is no org rail: Gizai has one
organisation. A top bar (`size-topbar`) holds breadcrumbs and page actions; list pages add a view
toolbar below it (New task on the left; view switch, Filters, Sort, Group on the right). Pages with
parts (Usage, Settings) show them as tabs, the open one in the address. The Editor adds a file
tree (`size-tree`, rows `size-tree-row`), a tab strip (`size-tab`) and a status bar
(`size-statusbar`), which grow with the text like the sidebar and rows. A terminal's tabs are
`size-term-tab`; the Editor's terminal panel starts at `size-term-panel` of the code column, within
`size-term-panel-min` and `size-term-code-min`, with a `size-handle` drag handle.

**Drawers, not dialogs.** Every create and edit form (new task, new project, edit client, add
agent, agent settings, new doc) slides in from the right in a Drawer at least `size-drawer`
(1024px) wide, over a `scrim`. Forms inside use sections: a 220px intro column (heading + one
sentence) and a two-column field grid. The footer is sticky with the primary action on the right.
Centered dialogs are only for confirmations of one sentence.

**Markdown is edited with a toolbar.** Every Markdown field (description, acceptance criteria,
comments, docs, agent instructions, project goal) uses the MarkdownEditor: a toolbar (heading,
bold, italic, quote, code, link | numbered list, bullet list, checklist | rule, table) above a
live-preview surface. Ctrl+Enter saves, Escape cancels.

**Motion.** Only the live pulse moves on its own. Drawers slide 200ms, the scrim fades 160ms.
Everything respects `prefers-reduced-motion`.

**States.** Hover = `hover` fill. Current = `selected` fill. Focus = a 2px `focus` ring, offset
1px, at least 3:1 on every surface. Disabled = 45% opacity, no pointer.

## Iconography

Lucide icons (ISC licence), drawn at 16px with a 1.75 stroke in `currentColor`; 14px inside small
buttons, 20px in empty states. The Icons asset group holds the set Gizai uses. Status glyphs are
Gizai's own (StatusIcon), drawn on a 16px grid. Map concepts consistently:

| Concept | Icon |
|---|---|
| New task | `square-pen` |
| Chat | `messages-square`; send `arrow-up`; notes `info` |
| Inbox | `inbox` |
| Tasks | `list-todo` |
| Clients | `building-2` |
| Projects | `folder-kanban` |
| Team | `network` |
| Users | `users` |
| Settings | `settings` |
| Search | `search` |
| List / board view | `list` / `columns-3` |
| Filters, sort, group | `list-filter`, `arrow-up-down`, `layers` |
| Run / pause / stop | `play`, `pause`, `square` |
| Attachments, docs | `paperclip`, `file-text` |
| Branch, terminal | `git-branch`, `terminal` |
| Agent roles | lead `crown`, frontend `monitor`, backend `server`, design `palette`, QA `flask-conical`, DevOps `container`, other `bot` |

No emoji, no illustrations. Gizai has no logo yet: the name is set in plain type.
