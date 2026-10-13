# Onboarding on first start (GA-168)

What someone sees the first time GizAi opens: four steps (you and your team, coding CLIs, MCP servers, ready), then the
Chat page, where a first message makes the Team Lead welcome them. Today the app opens straight on the board. Designed in
Figma for Jefsev's review on 14 Oct 2026; GA-162 (the backend) and GA-164 (the screens) build it once it is approved.
Where this design shows how things look it is the reference; where GA-164 says how they behave, GA-164 is.

## In Figma

The file Giz-AI, page **App: Onboarding** (`135:2`):
https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=135-2

| Frame | Node | Link |
|---|---|---|
| Step 1 · You and your team · Dark | `139:182` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=139-182) |
| Step 2 · Coding CLIs · Dark | `140:260` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=140-260) |
| Step 3 · MCP servers · Dark | `141:345` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=141-345) |
| Step 4 · Ready · Dark | `142:457` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=142-457) |
| Step 1 · You and your team · Light | `142:10385` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=142-10385) |
| Step 2 · Coding CLIs · Light | `142:10413` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=142-10413) |
| Step 3 · MCP servers · Light | `142:10443` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=142-10443) |
| Step 4 · Ready · Light | `142:10467` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=142-10467) |
| Step 1 · Error: empty name · Dark | `143:895` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=143-895) |
| Step 2 · No Claude Code · Dark | `143:968` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=143-968) |
| Step 3 · No MCP servers · Dark | `143:1101` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=143-1101) |
| Chat · Right after · Dark | `146:1125` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=146-1125) |
| Onboarding components | `135:21` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=135-21) |
| About this page (choices to check, texts changed) | `148:1368` | [open](https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=148-1368) |

- Rows: dark steps on top (y 0), light steps under them (y 1100), the three dark states and the Chat page under those
  (y 2200), the components on the right, About this page above.
- Each frame sets the `GizAi colour` mode (Dark or Light); the components set none, so one set of components draws both.
- Prototype: Next, Back and Open the Team Lead chat link the frames (flows "Onboarding · Dark" and "Onboarding · Light").
- Every colour is a `GizAi colour` variable, every radius a `GizAi size` variable and every text an `App/…` text style.
  The `App/…` text styles are new in the file: `design/tokens.json`'s type groups (t-entity-title, t-prose, t-body, …),
  plus `t-body-strong`, `t-button`, `t-button-sm`, `t-label`, `t-badge` and `t-kbd` for what components.css sets by hand.

## The window, 1440 × 900

The onboarding takes the whole window, without the sidebar: nothing in the sidebar works yet, and the steps get the
focus. From the top:

| Part | Where | What |
|---|---|---|
| Top bar | y 0, 1440 × 52 (`size-topbar`) | Padding 0 20, 1 px `line` under it. Left (300 wide): the GizAi logo, horizontal, 20 px high (79.7 wide), one colour `text-2` (the brandbook: inside the app the logo is never pink). Middle: the step indicator, centred on the window. Right (300 wide): Skip setup. |
| Step body | x 400, y 116, 640 wide | 64 below the top bar. It scrolls between the top bar and the actions when it is taller (many MCP servers, a large text size). |
| Actions | x 400, y 700, 640 wide | A 1 px `line`, then 16, then Back (left) and Next (right), 30 high. They stay at one place on every step. Place them `calc(band height + 24px)` above the window's bottom: at 900 high the band is 126 (`clamp(size-band-min, 14vh, size-band)`), so 24 above it. |
| Mountains | the bottom 126 (drawn 128 in Figma) | The app's band (`Band.tsx`, `band-line`, `band-glow`), behind everything; the step body and top bar are see-through, the panels and fields stay solid over it. It goes under 560 px high, with forced colours and when Settings turns it off, as in the app. |

Inside the step body, top to bottom: the eyebrow ("Step 1 of 4"), 8, the heading, 8, the line under it, then 32 (step 1)
or 24 (steps 2 to 4) to the first part. Between parts 24; a list's head 8 above the list; a caption 8 (10 on step 3)
under it; a field's label 6 above its control and its hint 6 under it; two fields side by side 20 apart (`.fields`).

## The step indicator (`Onboarding/Step`, in `Onboarding/Top bar`)

Four steps, 12 apart, with a 32 × 1 connector in `line-2` between them. Each is a 20 px circle and its name (t-body), 8
apart.

| State | Circle | Name | Does |
|---|---|---|---|
| Done | `selected` fill, a 12 px check in `text` | `text-2` | A button: opens that step again. |
| Current | `accent-soft` fill, 1 px `accent` ring, the number in `accent` (t-badge) | `text`, 600 (t-body-strong) | `aria-current="step"`. |
| Upcoming | 1 px `line-2` ring, the number in `text-3` | `text-3` | Nothing (not a button). |

The state is in the shape and the weight too (check, ring, bold), not in colour alone. Screen readers also get the step's
eyebrow ("Step 2 of 4").

## The steps

### 1 · You and your team

- Heading **Welcome to GizAi**, then the line under it.
- Two fields side by side (each 310): **Your name**, filled in with the login name (`display_name()` in
  `src-tauri/src/lib.rs`: `$USER` with a capital, `%USERNAME%` on Windows, for example `Jefsev`), and **Team name**,
  empty with the placeholder `My team`. The cursor waits in Team name (Focus in the frame), since the name is there.
- Under them: "Your team starts with five agents" and the five agents the seed adds (`DEFAULT_AGENTS` in
  `crates/gizai-core/src/seed.rs`) as `.agent-chip`s.
- Back is off (45 %). Next is on. Pressed with an empty field, that field turns red and says why under it, the cursor goes
  there and the step stays (frame *Step 1 · Error: empty name*). Enter in either field does what Next does.

### 2 · Coding CLIs

- "Found on this computer" with **Look again** on the right (ghost, small, `refresh-cw`), then a panel with a row per CLI
  found: the tick, its name, its program in t-code on the right. Claude Code, Codex, Gemini, OpenCode, Cursor Agent and
  Crush are the ones GizAi knows (`KNOWN` in `src-tauri/src/clis.rs`, plus the built-in Claude Code).
- Claude Code's tick is on and can't change ("· Built in"): Settings → Coding CLIs always has it. The others found start
  ticked; Next adds the ticked ones to Settings → Coding CLIs.
- Under the list, the known CLIs not found, in one line.
- **Your agents build and test with**: a select (310) of the ticked CLIs, Claude Code first. The Backend, Frontend, QA
  and DevOps agents get it (today the seed picks Claude Code, or Codex when only Codex is installed).
- An info line: the Team Lead's chat needs Claude Code.
- **Without Claude Code** (frame *Step 2 · No Claude Code*): a warning above the list with GA-164's sentence, "How to
  install Claude Code" (opens https://docs.claude.com/en/docs/claude-code in the browser, the page `docs/install.md`
  links) and **Look again**, which looks again and, once Claude Code is there, shows the normal step. The list's own Look
  again and the info line go (the warning says both). Next stays on: the agents can work on Codex or Gemini, and the
  chat waits for Claude Code. Proposal for GA-164: then step 3 is skipped (there is nothing to import) and step 4's
  button reads "Open the board".
- With no CLI found at all, the panel is the `.empty` box: "No coding CLIs found on this computer." under the warning
  (proposal, not drawn).

### 3 · MCP servers (optional)

- Heading **MCP servers** with an outline **Optional** badge.
- "In your Claude Code", and on the right how many are ticked and what Next does with them.
- A panel with Settings → MCP servers → Import from Claude Code's rows (`ImportPanel` in
  `src/components/McpSettings.tsx`), unchanged: the tick, the name (600), `· {account} · {scope} scope[: {folder}] ·
  {type}` in `text-3`, **Already in the list** (badge outline) for a server whose command or address GizAi has, the
  command or address in t-code, and the environment and header names when it has them. None start ticked, as in Settings
  (the frame shows two the user ticked).
  A name that clashes asks for another name in the row, as in Settings.
- Under it, Settings' read-only note, shorter.
- Next imports the ticked servers and goes on; with none ticked it just goes on.
- **With none found** (frame *Step 3 · No MCP servers*): the `.empty` box (dashed `line-2`, `plug` icon in `text-3`)
  with GA-164's two sentences, the first in 600 (`.empty b`). Next goes on.

### 4 · Ready

- Heading **You're all set**, the line under it, then the summary: your name, the team, the agents' CLI and the MCP
  servers imported ("None" when none), each with **Change**, which opens that step again (and Next there comes back to
  Ready).
- "The chat opens with this message from you:" and the first message, quoted (2 px `line-2` on its left), so nothing is
  sent in your name unseen.
- Back, and **Open the Team Lead chat** in Next's place (primary, `messages-square`): GizAi starts a chat with the Team
  Lead called "Welcome to GizAi", sends the first message as yours and opens it.

### Right after: the Chat page (frame *Chat · Right after · Dark*)

The app as it is today, nothing new but the chat's title: the sidebar (Chat current, with "working"), the top bar (Chat /
Welcome to GizAi; the Team Lead chip "working"), Recent with the one chat (its pulse), your message on the right and the
Team Lead's answer starting under its name: the text so far, the teal caret, "Writing…". The composer waits with "Queue a
message for when this answer is done…" and Stop. Teal here is right: an agent is at work.

### On every step

- **Skip setup** (top bar, ghost): opens the board. What earlier steps saved stays; the rest keeps the seed's defaults.
  Its tooltip says "Open the board. You can set things up later in Settings."
- **Back** goes one step back, keeping what you filled in. **Next** saves the step and goes on.
- **An error from a step** (it couldn't save) shows under the step's body, above the actions, as `Onboarding/Notice`,
  Kind=Error (`.error-banner`'s border and fill, its words in `text`: danger on danger-soft is 4.49:1 in light, under
  AA), with the error's own words; the step stays. A field's problem shows under that
  field (`.field .error`).
- Between steps nothing slides: the next step is there at once and the focus goes to its heading.
- The theme and text size are Settings → Appearance's (dark first); everything grows with the Interface size as in the
  app.

## Components and their states

All on the frame *Onboarding components* (`135:21`). `App/…` are `src/styles/components.css` as Figma components;
`Onboarding/…` are the onboarding's own.

| Component | Node | From | States and properties |
|---|---|---|---|
| Icon/… (36) | in `135:22` | Lucide, 16 px, stroke 1.75 (`.icon`) | Stroke `text-2`; set to the text colour beside it. |
| App/Button | `136:194` | `.btn`, `.btn.primary`, `.btn.ghost`, `.btn.sm` | Kind Primary · Secondary · Ghost × Size Medium (30) · Small (26) × State Default · Hover · Focus · Disabled; Label, leading and trailing icon. |
| App/Input | `136:215` | `.input` | State Default · Hover · Focus · Error · Disabled × Content Value · Placeholder. **Error is new** (see below). |
| App/Select | `136:232` | `.select` | Default · Hover · Focus · Disabled. |
| App/Checkbox | `136:245` | `.check input` | Checked On · Off × Default · Focus · Disabled. |
| App/Badge | `136:262` | `.badge` | Neutral · Outline · Info · Ok · Warn · Fail · Needs · Live. Used: Outline ("Optional", "Already in the list"). |
| App/Avatar | `136:271` | `.avatar` | Agent · Person × Default (22) · Small (18). |
| App/Kbd | `136:272` | `.kbd` | Key. |
| App/Link | `136:298` | `.link-btn` (Accent), `.link` (Plain) | Default · Hover; external-link icon on or off. |
| App/Agent chip | `137:359` | `.agent-chip` | Name. |
| App/Live tag | `144:1121` | `.live-tag` + `.pulse` | Words. |
| App/Nav item | `144:1150` | `.nav-item` | Default · Current; Icon, Kbd, Live, Count. |
| App/Nav label | `144:1151` | `.nav-label` | Plus, Caret. |
| Onboarding/Step | `137:35` | new | Done · Current · Upcoming; Number, Label. |
| Onboarding/Top bar | `137:184` | new | Step 1 · 2 · 3 · 4. |
| Onboarding/Actions | `137:271` | new | First (Back off) · Middle · Last (Open the Team Lead chat). |
| Onboarding/CLI row | `140:253` | `label.check` in a `.panel` | Title, "· Built in" on or off, Detail (the program); the checkbox inside takes its own state. |
| Onboarding/Check row | `137:272` | `ImportPanel`'s row | Title, Meta, Detail, Extra on or off, badge on or off. |
| Onboarding/Summary row | `137:285` | like `.kv` | Key, Value, Change. |
| Onboarding/Notice | `137:358` | new; Error is `.error-banner` with its words in `text` | Warning · Error; Text, actions on or off. |

Sizes inside them:

- Rows: CLI row 40 high (padding 10 14, gap 10; name and "· Built in" 6 apart; the program one line, cut with …). Check
  row: padding 10 14, gap 10, its lines 2 apart, the tick in a 15 × 20 box so it sits on the first line. Summary row: at
  least 44 (padding 8 8 8 16, gap 12; the key 190 wide). A 1 px `line` above every row but the first, in a `.panel`
  (`raised`, 1 px `line`, `radius-l`, clipped).
- Notice: padding 10 12, gap 10, `radius`, 1 px border; its icon in a 16 × 20 box; its actions 10 under the text, 16 apart.
- Empty box: `.empty` (padding 22 24, gap 8, dashed `line-2`, `radius-l`), its icon 20.
- Quote: 2 px `line-2` on the left, padding 2 0 2 12.

### New CSS for GA-164

- `.input[aria-invalid="true"], .input.invalid { border-color: var(--danger); }` (and the same for `.select`): the Error
  state. Focus keeps the accent halo; the message goes in `.field .error` under it, tied with `aria-describedby`.
- The onboarding's layout (proposed names): `.onb` (the window), `.onb-top` (the top bar's three zones: `1fr auto 1fr`
  would centre the steps), `.onb-steps` / `.onb-step` (`.done`, `.on`), `.onb-body` (640 wide, centred, scrolls),
  `.onb-actions` (sticky, 640, `border-top: 1px solid var(--line)`, `padding-top: 16px`), `.onb-notice` (`.warn` and
  `.bad`). Everything else is components.css as it is: `.btn`, `.input`, `.select`, `.check`, `.field`, `.fields`,
  `.panel`, `.badge.outline`, `.agent-chip`, `.empty`, `.link-btn`.

## Tokens

- **Colour:** `bg` (canvas), `raised` (panels, the Secondary button), `side` (the Chat page's columns), `hover`,
  `selected` (done steps, the current nav item and chat), `line`, `line-2`, `text`, `text-2`, `text-3`, `accent`,
  `accent-hover`, `accent-soft`, `on-accent`, `focus`, `warning` and `warning-soft` (no Claude Code), `danger` and
  `danger-soft` (errors), `live` (only on the Chat page: the Team Lead at work), `band-line` and `band-glow` (the
  mountains). No `needs`: nothing in the onboarding waits for you.
- **Radius:** `radius-s` (avatars, kbd), `radius` (buttons, fields, notices), `radius-l` (panels, the bubble, the empty
  box), `radius-xl` (the composer), `radius-pill` (badges, the lead chip).
- **Size:** `size-topbar` 52, `size-side` 248 (Chat page), `size-band` 128 and `size-band-min` 80.
- **Type (App/… text styles):** t-entity-title (headings), t-prose (the line under them, the Team Lead's answer), t-body,
  t-body-strong (names in rows), t-label (field labels, list heads), t-small (eyebrow, counts, the info line, summary
  keys), t-caption (hints, errors, captions), t-button and t-button-sm, t-badge, t-code (programs, commands, addresses),
  t-kbd, t-nav-label.
- **Spacing:** the 4 px grid: 2, 6, 8, 10, 12, 16, 20, 24, 32 and 64 (`space-1` to `space-8` where they match).
- **Contrast:** `text-3` is 5.0:1 on `bg` and 4.7:1 on `raised` in dark, 5.7:1 in light; a field error (`danger`) 6.6:1
  and 5.2:1 on `bg`; the notices' words are `text` (13:1 and more), since `danger` on `danger-soft` is 4.49:1 in light;
  the link on `warning-soft` 5.5:1 and 5.1:1; the current step's number (`accent` on `accent-soft`) 5.6:1 and 5.0:1. The
  unchecked box's border is `text-3` (3:1 and more on every surface); focus rings are `focus`, 2 px, 1 px off.

## Every text, word for word

Marks: **GA-164** is GA-164's text as GA-168 quotes it, word for word. **Changed**: GA-164 gives a text and this design
proposes other words (why in the last column). **New**: GA-164 gives no text; this design proposes it. **App**: a text
Gizai already has, unchanged. **Example**: a value that depends on the computer or what the user typed.

### On every step

| Where | Text | Mark |
|---|---|---|
| Step indicator | You and your team | GA-164 |
| Step indicator | Coding CLIs | GA-164 |
| Step indicator | MCP servers | Changed: GA-164's step is "MCP servers (optional)"; the indicator keeps it short and step 3's heading has the Optional badge. |
| Step indicator | Ready | GA-164 |
| Step indicator | 1, 2, 3, 4 | GA-164 |
| Top bar, right | Skip setup | GA-164 |
| Skip setup's tooltip | Open the board. You can set things up later in Settings. | New |
| A done step's label for screen readers | Back to step 1: You and your team | New |
| Actions | Back | GA-164 |
| Actions | Next | GA-164 |
| Eyebrow | Step 1 of 4 (2, 3, 4 likewise) | New |

### 1 · You and your team

| Where | Text | Mark |
|---|---|---|
| Heading | Welcome to GizAi | Changed: GA-164 names the step "You and your team" (kept in the indicator); this is the first screen of GizAi anyone sees. |
| Under the heading | Four short steps, then your Team Lead says hello. First, you and your team. | New |
| Field label | Your name | New (GA-164: "your name") |
| Field value | Jefsev | Example: the login name, as GA-164 says |
| Field hint | From your login name. The Team Lead calls you by it. | New |
| Field label | Team name | New (GA-164: "the team's name") |
| Field placeholder | My team | GA-164 |
| Field hint | Your agents work in this team. | New |
| Label | Your team starts with five agents | New |
| Chips | Team Lead · Backend Agent · Frontend Agent · QA Agent · DevOps Agent | App (the seed's agents) |
| Error under Your name | Type your name to go on. | New |
| Error under Team name (not drawn) | Type a name for your team to go on. | New |
| Team name in the error frame | YoTech | Example |

### 2 · Coding CLIs

| Where | Text | Mark |
|---|---|---|
| Heading | Coding CLIs | GA-164 |
| Under the heading | Your agents work through the coding CLIs on this computer, with your own logins. | New |
| List head | Found on this computer | New |
| List head, button | Look again | GA-164 (its words for the no-Claude-Code state, used here too) |
| Row | Claude Code · Built in | App (the name); New ("Built in") |
| Rows | Codex · Gemini · OpenCode (also Cursor Agent, Crush) | App (`KNOWN` in `src-tauri/src/clis.rs`) |
| Rows, right | /home/jefsev/.local/bin/claude, /home/jefsev/.npm-global/bin/codex, /home/jefsev/.npm-global/bin/gemini, /home/jefsev/.opencode/bin/opencode | Example (the program found) |
| Under the list | Not found: Cursor Agent and Crush. You can add a CLI later in Settings → Coding CLIs. | New (the names are the ones not found, joined with commas and "and") |
| Field label | Your agents build and test with | New (GA-164: "pick the one the agents build and test with") |
| Select | Claude Code | Example (the first ticked CLI) |
| Field hint | The Backend, Frontend, QA and DevOps agents run on it. Each can change it in its settings. | New |
| Info line | The Team Lead's chat needs Claude Code, whichever CLI your agents use. | Changed: GA-164 asks for "a plain line that the Team Lead's chat needs Claude Code"; these words also say the agents' pick doesn't change that. |
| No CLI at all (not drawn) | No coding CLIs found on this computer. | New |

**Without Claude Code**

| Where | Text | Mark |
|---|---|---|
| Warning | Claude Code isn't installed yet. The Team Lead's chat needs it. | GA-164 |
| Warning, link | How to install Claude Code | New (opens https://docs.claude.com/en/docs/claude-code) |
| Warning, button | Look again | GA-164 |
| Under the list | Not found: OpenCode, Cursor Agent and Crush. You can add a CLI later in Settings → Coding CLIs. | New |
| Select | Codex | Example |

### 3 · MCP servers

| Where | Text | Mark |
|---|---|---|
| Heading | MCP servers | Changed: GA-164 says "MCP servers (optional)"; brackets in a heading read as an afterthought, a badge is the app's way to mark it. |
| Badge | Optional | Changed (see the heading) |
| Under the heading | Bring the MCP servers your Claude Code has. Each stays off until you switch it on for an agent. | New |
| List head | In your Claude Code | New |
| List head, right | 2 ticked: Next imports them (1 ticked: Next imports it; none: None ticked: Next goes on) | New |
| Row, name | figma · github · chrome-devtools · linear · context7 | Example |
| Row, after the name | · Claude Code · user scope · Address (HTTP) | App (the import row: `· {account} · {scope} scope[: {folder}] · {type}`; types Command, Address (HTTP), Address (SSE)) |
| Row, after the name | · Claude Code · local scope: /home/jefsev/code/kade · Address (SSE) | App, with an example folder |
| Row, badge | Already in the list | App |
| Row, second line | https://mcp.figma.com/mcp · npx -y @modelcontextprotocol/server-github · npx chrome-devtools-mcp@latest · https://mcp.linear.app/sse · https://mcp.context7.com/mcp | Example |
| Row, third line | Environment: GITHUB_PERSONAL_ACCESS_TOKEN (values go to your keychain) | App (the format), Example (the name) |
| Under the list | Read only: GizAi doesn't change Claude Code's files or read its sign-ins. An import is a copy: later changes in Claude Code don't follow. | New here (Settings' note, shorter) |

**With no MCP servers**

| Where | Text | Mark |
|---|---|---|
| Empty box | **Your Claude Code has no MCP servers yet.** You can add them later in Settings → MCP servers. | GA-164 (the first sentence in 600) |

### 4 · Ready

| Where | Text | Mark |
|---|---|---|
| Heading | You're all set | Changed: GA-164 names the step "Ready" (kept in the indicator); a sentence welcomes, one word labels. |
| Under the heading | Here's what you set up. Your Team Lead is ready to say hello. | New |
| Summary, keys | Your name · Team · Your agents build and test with · MCP servers imported | New (GA-164: "the name, the team, the agents' CLI, the servers imported") |
| Summary, values | Jefsev · YoTech · Claude Code · figma, github | Example |
| Summary, no servers | None | New |
| Summary, button | Change | New |
| Label | The chat opens with this message from you: | New |
| The message | Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else. | GA-164 |
| Next's place | Open the Team Lead chat | GA-164 |
| Proposal without Claude Code (not drawn) | Open the board | New |

### Right after: the Chat page

| Where | Text | Mark |
|---|---|---|
| The chat's title (top bar, Recent) | Welcome to GizAi | New: today a chat takes its first message's first 60 characters ("Hi! I just installed GizAi and set up my team. Please…"); the onboarding names it. |
| Your message | Hi! I just installed GizAi and set up my team. Please welcome me, tell me in a few lines what you and the team can do, and ask whether I'd like help setting up agents, clients, projects, tasks or anything else. | GA-164 |
| The answer starting | Welcome to GizAi, Jefsev! I'm your Team Lead. I plan the work with you and turn it into cards, and your Backend, Frontend, QA and DevOps agents pick them up, each on a branch of its own. QA tests every card before it comes back to you for review. | Example (the Team Lead writes its own) |
| Everything else | Chat, New chat, Recent, just now, Archive, Team Lead, working, Writing…, Queue a message for when this answer is done…, Enter queues, Shift Enter new line, Runs on, Claude Code, Stop, and the sidebar | App |

### Step errors

| Where | Text | Mark |
|---|---|---|
| Under the step (`Onboarding/Notice`, Error) | The backend's own message, for example: Couldn't save your team: the database is read only. Try again, or quit and start GizAi again. | Example |

## What Figma can't show

- The pulse ringing and the caret blinking on the Chat page (both stop with reduced motion, as in the app).
- The band's real height at 900 px (126; the component is 128) and the step body scrolling.
- Hover and focus on every control: the components hold those states (App/Button, App/Input, App/Select, App/Checkbox).
