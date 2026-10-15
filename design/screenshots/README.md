# Screenshots on the card: the design (GA-181)

The design of GA-173's research (`docs/research/screenshots.md` §3.7, §3.8 and §5.1, on main since GA-175) for
Jefsev's review, before SF1 and SF2 build it. It lives in Figma, file **Giz-AI**, page **App: Screenshots** (`158:2`):
https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=158-2

Every frame is the window, 1440 × 900 (`src-tauri/tauri.conf.json`), drawn with the app's tokens (the GizAi colour and
size variables, Dark and Light modes), the App/* text styles and the App/* components of GA-168 (page App: Onboarding,
`135:21`), plus the components below. `use_figma` places no images, so each screenshot is a grey placeholder with its
size marked (`Shot/Picture`).

## The frames

Open one with `https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=<id with - for :>`.

| Frame | Node | What it shows |
|---|---|---|
| Screenshots tab · Dark | `166:71` | GW-8 in Review: Card \| Changes \| Screenshots 6; QA's set (2), the Frontend Agent's set (4: two changed, one unchanged phone picture, one new light one), 4 older runs folded |
| Screenshots tab · Light | `168:479` | The same in the Light mode |
| Screenshots tab · Older runs open · Dark | `168:564` | Scrolled down to the older runs: QA failed (a phone picture), a set with files left out, a run whose preview didn't start, a run where no screen changed |
| Empty tab · No preview · Dark | `169:1291` | MN-31: the project has no Preview command; Set up the preview |
| Empty tab · The agent's screenshots are off · Dark | `169:1947` | GW-9: the Frontend Agent's switch is off; Open Frontend Agent settings |
| Empty tab · Off for this card · Dark | `169:2590` | GW-10: the card's own switch is off; Show properties, and Properties open after its click, the focus on the new row |
| Empty tab · No run yet · Dark | `169:3279` | GW-11: nothing yet; Screenshot settings |
| Larger view · Dark | `170:2304` | One picture fitted, its caption, 2 of 4, Previous and Next, Fit \| 100%, Compare, Open, Close |
| Compare · Side by side · Dark | `172:2896` | The run before and this run, paired by file name; Compare pressed |
| Compare · Slider · Dark | `172:3640` | One picture with a line to drag, here dragged to 40 %: the run before left of it, this run right |
| Board · Review cards with the sign · Dark | `174:3626` | The board scrolled to its right end; GW-8 (6) and MN-29 (4) in Review carry the sign, GA-176 (no screenshots) doesn't, nor do other columns |
| Inbox · Review rows with the sign · Dark | `174:4544` | GW-8 "6 screenshots", MN-29 "4 screenshots"; the card on hold has none |
| Settings · The project: its form → Preview · Dark | `175:4306` | Edit gizai-website, the new Preview section |
| Settings · The agent: its form → Tools → Browser · Dark | `175:4545` | Frontend Agent settings, the new Take screenshots of what it built under the browser; Instructions follows, as today |
| Settings · Settings → Agents and runs → Screenshots · Dark | `175:4718` | The new Screenshots section under Runs |
| Screenshots components | `158:8` | The new components, icons and glyphs (below) |

Rows on the canvas: y 0 the tab, y 1100 the empty tab, y 2200 the larger view and Compare, y 3300 the board and the
Inbox, y 4400 the settings; components at x 9000.

## Where this follows the research instead of the card

1. **Compare pairs by file name**, not by caption: the research's agents number and name their files
   (`02-search-results-desktop-dark.png`) and a name that comes back is the pair; captions may change between runs. The
   foot says which caption the run before had. Same picture (same hash): **Unchanged**; a name the run before didn't
   have: **New**, and Compare is off for it. Compare has two ways: **Side by side** and a **Slider**.
2. **The larger view has more than Previous, Next and Close**: Fit \| 100% (key `1`), **Open** in the system's image
   viewer (as Attachments do), Compare (key `C`).
3. **No port and no demo-data field in the project**: Gizai picks a free port per run from Settings → Agents and runs →
   Screenshots → Preview ports and hands it in `GIZAI_PREVIEW_PORT` (and `PORT`); the demo data is the Preview command's
   own job, in a throwaway database in `GIZAI_PREVIEW_DIR`. The project has Screenshots, Preview command, Start time,
   Themes, Sizes and Whole page.
4. **Sizes and themes are the project's**, in its form → Preview, not in Settings: a website has Desktop and Mobile, Gizai
   Window and Small. Settings has how many a run, the largest file, how long they're kept and the ports.
5. **One more switch**, the card's own: Properties → Screenshots → Screenshots before Review (the research's table), so
   the empty tab has a fourth reason, Off for this card.
6. **A run that took none still shows**, with why (the preview didn't start; no screen changed), and a set says what was
   left out. The card asked for the commit in each run's heading; the research's contract CS has no commit: SB1's
   `ScreenshotSet` needs one (the run's head commit, 7 characters).

## Choices to check

1. **The count on the tab** is the newest round: the builder's newest set plus QA's sets after it (4 + 2 = 6), what you
   look at in Review. The research says "the newest set's number" and sketches 6 for the same two sets.
2. **The summary "2 changed, 1 unchanged, 1 new" and the badges** show only on a set that shares a file name with an
   older set (here the Frontend Agent's); QA's additions have none.
3. **Older runs fold after the newest two**, with one line: "4 older runs" and when.
4. **The sign** is an outline badge (`line-2`, no fill) so it doesn't compete with the pull request's: only on a card in
   Review whose newest round has screenshots; "6" on the board, "6 screenshots" in the Inbox. Click: the Screenshots tab.
5. **The larger view** floats over the page on the scrim, 24 px from the window's edges, like the palette (overlay,
   radius-xl, shadow-pop), not a drawer: a picture needs the width.
6. **The page behind a drawer** in the two form frames is the board, dimmed: the forms open from the project's and the
   agent's pages, which aren't drawn here.
7. **The screenshot limits are a section of their own** in Settings → Agents and runs, Screenshots under Runs, not rows
   in Runs as §3.8 has them: they're about pictures and previews rather than a run's minutes and calls, and the
   section's intro can say where the sizes and themes are.
8. **Off for this card links with Show properties**, the words of the top bar's button (`]`): it opens Properties and
   puts the focus on the Screenshots switch. It stays in the notice while Properties is open, as the frame shows after
   its click.

## Sizes and spacing

- **Window** 1440 × 900; sidebar 248 (`size-side`); top bar 52 (`size-topbar`), padding 0 20, a 1 px `line` under it.
- **Tabs** (`.tabs.task-tabs`): a row 42 high at y 52, padding 0 16, gap 4, a `line` under it. A tab: padding 10 14 11,
  gap 7, a 16 px icon, `t-button` (13.5/500) in `text-2`; the current one `text` with a 2 px `text` line at its bottom;
  the count (`.n`) in `text-3`; Changes' counts in `t-id` with `diff-add` and `diff-del`. Icons: Card `square-kanban`,
  Changes `file-diff`, Screenshots `images` (new in the icon map).
- **The tab's content**: full width under the tabs like Changes (Properties closed; `]` opens it, 320 wide): x 248, y 94,
  1192 × 806; padding 24 28; sets 28 apart (24 in the older-runs frame). At `#/task/<id>/screenshots`, as the research
  has it (`…/screenshots/<run>` for one run's set).
- **A set**: its heading (`Shot/Run head`, 28 high) then 12, then the grid: thumbnails 272 wide, 16 apart, rows 20 apart
  (four to a row at 1440). A note under the grid 12 below it.
- **A thumbnail** (`Shot/Thumbnail`): a cell 272 × 170 (`side`, 1 px `line`, radius 6) with the picture fitted in at its own
  proportions: 1280 × 800 fills it, 390 × 844 is 79 × 170 in the middle; then 8, the caption (`t-body`, `text`, wraps),
  2, the size and theme (`t-small`, `text-2`) with Changed, Unchanged or New (Neutral badge), 2, the file name (`t-id`,
  `text-3`, one line, ends in …).
- **Older runs, folded**: a row 32 high, padding 0 10 0 8, radius 6: `chevron-right` (`text-3`), the count (`t-body-strong`,
  `text-2`), when (`t-small`, `text-3`). Open: `chevron-down`.
- **Empty tab** (`.empty`): at the top of the content, padding 22 24, gap 8, a dashed 1 px `line-2` border, radius 10; a
  20 px `images` in `text-3`, the title `t-body-strong` `text`, the text `t-body` `text-2` at most 640 wide, then 6 and the
  button (Secondary, Medium; the `settings` icon, `panel-right` on Show properties) or the link (Accent, no icon).
- **Larger view**: `scrim` over the window; the viewer at 24 from each edge (1392 × 852), `overlay`, 1 px `line-2`,
  radius 14, `App/shadow-pop`. Head 52 high, padding 0 12 0 20, gap 10, a `line` under it. Stage `side`, the picture
  fitted (1280 × 800 at 1062 × 664) in the middle; Previous and Next 40 × 40, 24 from its sides, centred. Foot padding 14
  20, a `line` above: the caption `t-section`, under it size and theme, the badge and the file name; the keys on the right
  (`App/Kbd` and `t-caption` `text-3`).
- **Compare**: side by side, two panes 604 × 378 (24 apart, 80 from the sides), each with its label (Neutral "The run
  before", Info "This run"), run and commit above it. Slider: the picture at 1062 × 664, the run before left of a 2 px
  `text` line, under a 35 % `selected` tint; a 36 px round grip (`raised`, `line-2`) with `chevron-left` and
  `chevron-right`; the two labels in the top corners, 12 in. The line opens in the middle; the frame shows it dragged to
  40 % (425 of 1062). In both, the head's Compare is pressed (`selected` fill, as `aria-pressed` buttons are in
  `components.css`) and Side by side \| Slider (`App/Seg`) takes the place of Fit \| 100%.
- **Board card** (`App/Board card`): 282 in a 286 column, padding 10 12, gap 8, `raised`, 1 px `line`, radius 10; the sign
  after the pull request badge, 6 apart. Columns 12 apart; Done and Cancelled are 44 wide rails.
- **Inbox row** (`App/Task row`): 40 high, padding 0 10, gap 12, radius 6; the sign 8 after the title.
- **Forms**: drawers from the right (project form 1024, the agent form 1280 wide), `overlay`, a `line-2` left edge,
  `App/shadow-drawer`, over the `scrim`. Head padding 18 28 16; body padding 24 28; foot padding 14 28. A form section:
  a 220 intro column (`t-section`, then `t-small` `text-3`), 24, the fields in two columns 20 apart, rows 16 apart, 28 under
  it and a `line`. A field: its label (`t-label`, `text-2`), 6, the control (`App/Input` 34 high), 6, the hint
  (`t-caption`, `text-3`). The new browser switch sits in an `accent-soft` box (radius 6, padding 8 10) inside the
  browser's box. In the agent's form Instructions follows Tools, as today (its editor drawn as a box: `bg`, 1 px
  `line-2`, `radius`, padding 8 10, 260 high).
- **Settings**: the page at x 248, at most 1100 wide, padding 24 28, gap 18; its tabs as the card's.

## Tokens and components

**Colours** (GizAi colour, Dark and Light): `surface/side`, `bg`, `raised`, `overlay`, `hover`, `selected`; `line/line`,
`line-2`; `text/text`, `text-2`, `text-3`; `accent/accent`, `accent-soft`, `on-accent`, `focus`; `status/success` and
`-soft` (QA passed), `danger` and `-soft` (QA failed, High risk, the preview's note), `warning` and `-soft` (Stopped at
the time limit, Left out), `needs` and `-soft` (Needs a decision, Review, On hold); the glyphs `st-*`; labels `c-*`.
New in the file from `design/tokens.json`: `diff/diff-add`, `diff/diff-del` (the Changes tab's counts) and
`surface/scrim`; effect styles `App/shadow-pop` and `App/shadow-drawer` (dark values; the light ones are in their
descriptions). **No teal or magenta for screenshots**: nothing here is an agent at work, and the sign isn't "needs you".
**Size**: `radius-s`, `radius`, `radius-l`, `radius-xl`, `radius-pill`. **Type**: App/t-section, t-body, t-body-strong,
t-button, t-small, t-label, t-caption, t-badge, t-group, t-row, t-drawer-title, t-id, t-code, t-kbd.

**From GA-168** (`135:21`): App/Button (Primary, Secondary, Ghost × Medium, Small; Default, and Compare pressed with a
`selected` fill in the Compare frames), App/Input (Default, Value and
Placeholder), App/Checkbox (On and Off; Default, and Focus on the card's new switch), App/Badge (Neutral, Info, Ok, Fail,
Warn, Needs), App/Avatar (Agent and Person; Default and Small), App/Kbd, App/Link (Accent), App/Nav item (Default,
Current, with the Inbox count), App/Nav label, Icon/*. The sidebar and the mountains are copies of the Chat frame's
(`146:1125`).

**New** (in `158:8`):

| Component | Node | Properties and states |
|---|---|---|
| Shot/Picture | `160:34` | Format Desktop (272 × 170) \| Mobile (79 × 170), Look Dark \| Light, Size text. The greys are the picture, not tokens |
| Shot/Thumbnail | `162:27` | Caption, Meta, File, Compare badge (on/off), Desktop picture, Phone picture |
| Shot/Run head | `162:44` | Name, Role (icon), Meta, Commit, Summary, Show summary; avatar initials and the answer badge's kind and label inside |
| Shot/Sign | `160:35` | Count ("6", or "6 screenshots" in the Inbox) |
| Shot/Note | `160:83` | Kind Left out (triangle-alert, warning) \| Preview (circle-alert, danger) \| No change (info, text-3); Text |
| Shot/Nav button | `160:97` | Direction Previous \| Next |
| App/Tab | `160:64` | State Default \| Current; Label, Icon, Count, Count text, Diffstat, Added, Removed |
| App/Seg | `160:108` | Pressed First \| Second; First, Second |
| App/Board card | `163:50` | ID, Title, Pull request, Screenshots sign, Meta |
| App/Task row | `163:71` | ID, Title, Status, Screenshots sign, Label, Assignee, Badge, Date |
| App/Label pill | `160:84` | Label |
| App/Icon button | `160:87` | Icon (ghost, small, icon only) |
| Icon/* (new) | `158:9` | images, square-kanban, file-diff, panel-right, copy, chevron-left, chevron-right, columns-2, git-commit-horizontal, columns-3, list, list-filter, arrow-up-down, layers, trash-2, user, image, sliders-horizontal, bell, palette, git-pull-request |
| Status/* | `158:124` | Backlog, To do, In progress, Testing, Review, Done, On hold (StatusIcon.tsx's glyphs) |

States to build that the frames don't each show: a thumbnail's hover (`hover` behind the caption, the cell's border
`line-2`) and focus (the 2 px `focus` ring, offset 1); the folded row's hover `hover`; Previous hidden on the first
picture and Next on the last; Compare off (45 %) on a New picture, with the title "New in this run: the run before has no
04-docs-search-desktop-light.png"; on an Unchanged one Compare shows one picture and "Unchanged: the same picture as the
run before". A file name the run before had and this run hasn't (§3.7): under the set, `Shot/Note` No change, "Not taken
this time: <file name>, <file name>." Keys: ← → previous and next, `1` 100%, `C`
compare, Esc close; in 100% drag or scroll to look around. In Compare there is no `1`: ← → go to the set's next pair,
`C` goes back to one picture, Esc closes.

## Every text

The sidebar, in every frame (from the Chat frame): New task, N, Search, Ctrl K, Inbox, 2 (3 on the board and in the
Inbox, which count the cards that need you), Work, Chat, Tasks, Clients,
Projects, gizai-website, Gizai, spoorwegmuseum, Terminal, New terminal, Editor, Open a folder, Agents, Team Lead,
Backend Agent, Frontend Agent, QA Agent, DevOps Agent, Memory, Company, Usage, Team, Users, Settings.

**Screenshots tab** (`166:71`, `168:479`): Tasks / gizai-website / GW-8 · Copy ID · Card · Changes +120 −8 ·
Screenshots 6.
- QA · QA Agent · QA passed · 2 h ago · 2 screenshots · 5e1c9a7 · View the run
  - AC 2: a search with no results says what to try · Desktop · 1280 × 800 · Dark · 01-ac2-no-results-desktop-dark.png
  - AC 4: / opens the search on any docs page · Desktop · 1280 × 800 · Dark · 02-ac4-slash-opens-search-desktop-dark.png
- FA · Frontend Agent · Ready for testing · 3 h ago · 4 screenshots · 9f8e7d6 · 2 changed, 1 unchanged, 1 new · View the run
  - Docs home with the search box · Desktop · 1280 × 800 · Dark · Changed · 01-docs-search-desktop-dark.png
  - Results for “worktree”: five pages · Desktop · 1280 × 800 · Dark · Changed · 02-search-results-desktop-dark.png
  - Docs home on a phone, the search in the menu · Mobile · 390 × 844 · Dark · Unchanged · 03-docs-search-mobile-dark.png
  - Docs home with the search box, light · Desktop · 1280 × 800 · Light · New · 04-docs-search-desktop-light.png
- 4 older runs · Yesterday and 2 days ago: 2 sets of screenshots and 2 runs that took none
- On the placeholders: 1280 × 800, 390 × 844.

**Older runs open** (`168:564`), under the same top bar and tabs:
- QA · QA Agent · QA failed · yesterday · 1 screenshot · c41d2e0 · View the run
  - Failure 1: the results cover the menu · Mobile · 390 × 844 · Dark · 01-fail-results-cover-menu-mobile-dark.png
- FA · Frontend Agent · Ready for testing · yesterday · 3 screenshots · 4c3b2a1 · View the run
  - Docs home with the search box · Desktop · 1280 × 800 · Dark · 01-docs-search-desktop-dark.png
  - Results for “worktree” · Desktop · 1280 × 800 · Dark · 02-search-results-desktop-dark.png
  - Docs home on a phone · Mobile · 390 × 844 · Dark · 03-docs-search-mobile-dark.png
  - Left out 2: notes.txt isn’t an image; 05-docs-full-page-desktop-dark.png is over 16,384 pixels high.
- FA · Frontend Agent · Stopped at the time limit · 2 days ago · no screenshots · 1d9e0f3 · View the run
  - The preview didn’t start: it didn’t answer on port 47130 in 120 s, so the agent took none. Its last lines are in the run.
- FA · Frontend Agent · Needs a decision · 2 days ago · no screenshots · 0b7a6c5 · View the run
  - No screen changed, the agent says, so it took none.

**Empty tab**: the tab says Screenshots None.
- No preview (`169:1291`): Tasks / spoorwegmuseum / MN-31 · Changes +86 −12 · "No screenshots: spoorwegmuseum has no
  preview yet." · "Agents take screenshots of the card’s branch, served by the project’s Preview command with demo data.
  Without one they take none." · Set up the preview (opens Edit spoorwegmuseum at Preview).
- The agent's screenshots are off (`169:1947`): Tasks / gizai-website / GW-9 · Changes +214 −31 · "No screenshots: the
  Frontend Agent doesn’t take them." · "Its Take screenshots of what it built is off, so it builds this card’s screens
  without keeping pictures of them." · Open Frontend Agent settings (opens its form at Tools → Browser). When its browser
  is off the text is "It has no browser, so it builds this card’s screens without keeping pictures of them." with the
  same button.
- Off for this card (`169:2590`): Tasks / gizai-website / GW-10 · Changes +42 −5 · "No screenshots: they’re off for this
  card." · "Screenshots before Review is off in this card’s Properties, so its agents take none here. gizai-website has
  them on." · Show properties (opens Properties, as `]` does, with the focus on its Screenshots switch). Properties:
  Status In progress · Priority Medium · Testing Test before Review · **Screenshots Screenshots
  before Review** · "Off for this card. By default it follows gizai-website’s Screenshots switch, which is on." · Labels
  docs · Assignee FA Frontend Agent.
- No run yet (`169:3279`): Tasks / gizai-website / GW-11 · Changes No changes · "No screenshots yet." · "Agents with the
  browser save screenshots of the screens they changed at the end of their run. The Frontend Agent takes them when it
  has built this card." · Screenshot settings (Settings → Agents and runs, at Screenshots).

**Larger view** (`170:2304`): 2 of 4 · FA · Frontend Agent · Ready for testing · 3 h ago · 9f8e7d6 · Fit · 100% ·
Compare · Open · Results for “worktree”: five pages · Desktop · 1280 × 800 · Dark · Changed ·
02-search-results-desktop-dark.png · ← → previous and next · 1 100% · C compare · Esc close. Titles: Previous (←),
Next (→), Close (Esc).

**Compare, side by side** (`172:2896`): as the larger view, with Side by side · Slider in the place of Fit · 100%, and
The run before · Frontend Agent · yesterday · 4c3b2a1 · This run · Frontend Agent · 3 h ago · 9f8e7d6 · "Paired by file
name with the run before’s picture, captioned “Results for “worktree””." The keys: ← → previous and next · C compare ·
Esc close.

**Compare, slider** (`172:3640`): Side by side · Slider · The run before · yesterday · This run · 3 h ago; the keys:
drag the line · ← → previous and next · C compare · Esc close.

**Board** (`174:3626`): Tasks 14 · New task · Search ID or title · Filters · Sort. TO DO 2: GW-12 Footer: the legal links
on every page (website); MN-33 Touchscreens: a bigger font for the timeline (touchscreens). IN PROGRESS 2: GW-11
Changelog page from the GitHub releases (website); GA-182 Backend: a run’s screenshots onto its card (screenshots).
TESTING 1: GW-10 PR #43 Pricing: a table of the three plans (website). REVIEW 3: GW-8 PR #41 **6** Docs: search on every
docs page (docs); MN-29 PR #12 **4** Collection: filter by railway company (collection); GA-176 PR #143 Memory: keep each
agent’s learned lines per project (memory). DONE 23, CANCELLED 2. The sign's title: "6 screenshots: open the Screenshots
tab".

**Inbox** (`174:4544`): Inbox 3 · New task · Search ID or title · Filters · Sort · Group. REVIEW 2: GW-8 Docs: search on
every docs page **6 screenshots** docs Jefsev 2 h ago; MN-29 Collection: filter by railway company **4 screenshots**
collection Jefsev 5 h ago. IN PROGRESS 1: GA-180 Research: a preview command per project research Backend Agent On hold
1 h ago. A click on a row with the sign opens the card on its Screenshots tab.

**The project's form** (`175:4306`): Edit gizai-website · "Projects hold tasks, docs and files. Agents work on projects
linked to a git repository." Section **Preview**: "While an agent works on a card, Gizai serves the card’s branch with
demo data, so the agent can take screenshots of the screens it changed. The card shows them on its Screenshots tab."

| Field | Control (default) | Hint |
|---|---|---|
| Screenshots | ☑ Take screenshots of this project’s screens | On by default. Off: no agent takes any here, and its cards have no Screenshots tab. |
| Preview command | `npx astro dev --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --ignore-lock` (empty) | Empty by default: no preview, so no screenshots. Runs with bash -c in the card’s worktree when a run starts whose agent takes screenshots, on a free port in GIZAI_PREVIEW_PORT (and PORT), with GIZAI_PREVIEW_DIR for a throwaway database. Serve demo data, never your .env database. Gizai ends it with the run. |
| Start time (seconds) | 120 | 10 to 600; 120 by default. The agent starts once the page answers, or after this long without it. |
| Themes | ☑ Both themes, dark and light (off) | Off by default (on for Gizai). On: each changed screen in the first theme, and the card’s main screen in the other one too. |
| Sizes | Desktop 1280 × 800, Mobile 390 × 844, Add a size | 1 to 4 sizes, each 320 to 3840 wide and 320 to 2400 high. Desktop 1280 × 800 and Mobile 390 × 844 by default; Gizai’s own are Window 1440 × 900 and Small 900 × 600. |
| Whole page | ☐ The whole length of each page | Off by default: one window’s height at each size. |

Then Budget · "Optional. Shown on the project page." · Budget (€) 4800 · Budget (hours) 60; the foot: Ctrl+Enter saves ·
Cancel · Save changes.

**The agent's form** (`175:4545`): Frontend Agent settings · "Each agent runs its coding CLI headless in its own git
worktree, with the instructions and permissions below." Section **Tools**: "What Claude Code may use besides its
worktree, folders and commands: outside services through MCP servers, the web, a hidden browser and its own built-in
tools. Everything is off until you switch it on." Field **Browser**: ☑ Test web pages in a hidden browser ·
chrome-devtools 1.10.1 · High risk · "Opens, reads and clicks web pages, fills in forms and runs scripts in them. Pages
are untrusted, and they can reach any address." · "Found: Node 24.9.0, npx, Chromium (/usr/bin/chromium)." · ☐ Accept
self-signed certificates, for local .test sites (off: such a page doesn’t load) · **☑ Take screenshots of what it
built** · "On by default for Frontend, Design and QA agents, off for the rest; only with the browser on. At the end of a
run that changed a screen, it saves screenshots of it for the card’s Screenshots tab, at the project’s sizes (its form →
Preview)." · "List its tools in Settings → MCP servers → chrome-devtools to switch them one by one; until then, all its
tools are on." The field's hint: "Chrome DevTools: a hidden Chrome with a throwaway profile, never on your screen and
never your own browser, profile or logins. Every click is a tool call, and the cap per run is in Settings → Runs.
Suggested for QA, Frontend and Design agents, never switched on by Gizai." With the browser off, the new switch is off
and disabled (45 %). Then, as today: Instructions · "Sent with every run, before the task. Must end by asking for the
GIZAI_RESULT line." · Instructions (the editor shows a sample of the agent's instructions); the foot: Ctrl+Enter saves ·
Cancel · Save changes.

**Settings** (`175:4718`): Settings · Save settings · General · Appearance · Notifications · Agents and runs · MCP
servers · GitHub and Bitbucket. Runs (as today): Minutes per run 90 "5 to 480"; Tool calls per run 200 "20 to 2000. Every
file read, edit and command is one." New section **Screenshots**: "What Gizai keeps of the screenshots agents take at
the end of their runs, and the ports of the projects’ previews. Each project sets its sizes and themes in its form →
Preview."

| Field | Default | Hint (range) |
|---|---|---|
| Screenshots per run | 12 | 1 to 30; 12 by default. Gizai keeps the first ones by file name and says in the run what it left out. |
| Largest screenshot (MB) | 10 | 1 to 50; 10 by default. |
| Keep per card (sets) | 3 | 1 to 20; 3 by default. A newer run’s set removes the oldest beyond these. |
| Delete a finished card’s screenshots after (days) | 30 | 0 to 365; 30 by default, counted from Done or Cancelled. 0 keeps them. |
| Preview ports | 47100 to 47999 | From 1024 to 65535; 47100 to 47999 by default. At least ten ports for each run at once: a run’s preview gets its own port and the nine after it. |

A value outside its range is refused when saving, as Settings → Runs does, with "<Field> must be from <low> to <high>."

## For design/README.md, when SF1 builds it

Add to the icon map: **Screenshots `images`**; Compare `columns-2`; a commit `git-commit-horizontal`.
