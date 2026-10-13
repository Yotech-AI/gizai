# Website: the homepage, desktop (GA-144)

GizAi's homepage, designed in Figma from GA-52's brandbook, for Jefsev's review. Desktop first: the sub-pages (one per
feature) and mobile come later, and so does building the site.

**Round 2** follows Jefsev's review of 13 Oct 2026 ("the basics are there, but it lacks personality"): ADA moves up
after the tour; eight features in a new order, with row 01 open by default; one line to install; the logo's pink as the
site's accent; the mountains' wireframe lines through the page; and the mountains fixed along the bottom of the window.

- **Figma:** the file Giz-AI, page **Website: Home (desktop)** (node `37:2`):
  https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=37-2
  - `Home · Dark` (`40:27`) and `Home · Light` (`41:292`): the whole page, 1440 × 5833, top to bottom.
  - `Features · row 03 on hover · Dark` (`42:557`) and `· Light` (`42:673`): row 03 open under the pink plus cursor,
    row 01 closed.
  - `In the window · Dark` (`67:2341`) and `· Light` (`67:2571`), under the pages,: the page in a 1440 × 900 window at four scroll
    positions, with the mountains fixed along the bottom.
  - `Homepage components` (`37:16`): Cursor, Theme switch, Tour frame, Note, Plus mark, Rule, Section label, Install
    line, Point glyph and Feature row.
  - `About this page` (`43:721`), above the pages: what changed in round 2, and the website's pink rule.
  - Blue dashed notes: left of the dark page, how the parts behave; right of it, the page each feature row opens and
    what its screenshot or GIF shows. Each row also carries its address as a Dev Mode annotation.
  - In Figma's prototype view, hovering a closed feature row opens it (an interactive component, While hovering).
- **From the brandbook** (page Brandbook, `5:2`): the Logo, Button, Link, Badge, Card label and Mountains components,
  the `GizAi colour` variables (modes Dark and Light) and the `Web/…` text styles. Each page frame sets the colour mode,
  so one set of components draws both themes.

## The page, top to bottom

1. **Top bar**, 72 px, sticky, with a 1 px line under it: the logo; Tour, ADA, Features and Get GizAi, which scroll to
   their section; View on GitHub (https://github.com/Yotech-AI/gizai); the dark and light switch.
2. **Hero**: the mark at 72 px, the headline, one line, Get GizAi (primary) and View on GitHub (secondary), then the
   product tour in a 1280 × 832 frame, with a plus at each corner.
3. **ADA**, right after the tour: the outlined ADA and what the letters stand for; the headline and the three
   paragraphs; the three points in a strip of lines.
4. **Features**: eight numbered rows, guide to guide, split by 1 px lines. Row 01 is open.
5. **Get GizAi**: one line to install per system, each with Copy, and Or build from source.
6. **Footer**: the logo, Made by YoTech, GitHub and Apache-2.0 licence; under it the mountains' empty sky.
7. **The mountain band**, fixed along the bottom of the window, behind everything.

## The words

**Headline (Display):** Your agents build it. You review. ("review" in pink)

**The line under it (Lead):** GizAi is a free desktop app where a team of AI coding agents works through your board
and hands every card back to you for review.

**ADA** (copy kept from round 1)

- Label: A new kind of app. Headline (H1): **Not an IDE. An ADA.** ("ADA" in pink)
- An Agentic Development Application (ADA) is an application where a team of AI agents builds your software through
  your board. You write the cards. The agents pick them up, do the work and hand it back, and you review and decide.
- An IDE with an AI assistant is built around you writing code. The assistant helps in the file you have open, one
  request at a time. An ADA is built around the work: agents take whole cards, side by side, and you look at what comes
  back.
- GizAi is an ADA. Its board decides which agent works on what. A Team Lead plans the work with you, and QA tests every
  card before it comes back to you.
- The three points: **The board leads.** Cards and columns decide what gets done, and by whom. **Agents work side by
  side.** Every card gets its own agent, git worktree and branch. **You review and decide.** Nothing reaches your main
  branch until you merge it.

**Features:** label Features, title What GizAi does, lead "One app, from the card you write to the pull request you
merge." Each name is an H2 without its full stop, as the brandbook's headings are.

| # | Name (H2) | Text | Opens | Open shows |
|---|---|---|---|---|
| 01 | The board decides who works | Agents sit on columns, and Auto columns pick up cards by priority. Every card gets its own git worktree and branch. | `/features/board` | GIF: an Auto column picks up the next card, which gets its own worktree and branch |
| 02 | A Team Lead you talk to | A chat that turns your requests into cards, starts agents and checks the board. It asks you only what it can't decide, and you approve in one click. | `/features/team-lead` | GIF: the chat turns a request into cards and starts agents; a one-click approval |
| 03 | Bring your own CLI | Claude Code, Codex, Gemini or another coding CLI, on your own machine, with your own logins and accounts. See each account's usage limits and move agents to one with room. | `/features/cli` | Screenshot: Settings, Coding CLIs, the accounts and their usage limits |
| 04 | Tested before you see it | QA runs the tests, adds missing ones and opens the pull request. You review and merge, and DevOps releases. | `/features/testing` | Screenshot: a card in Testing, the QA run, its tests and pull request |
| 05 | The Editor and Terminal, built in | Open a project or a card's worktree, see its git status and edit, with your own shell in the panel under the code. Free, like the rest of GizAi. | `/features/editor-and-terminal` | GIF: the Editor on a card's worktree, git marks, a shell under the code |
| 06 | Memory | Decisions and gotchas as linked notes, with a graph. Agents learn on every card, and one client's notes never reach another's. | `/features/memory` | Screenshot: linked notes and their graph |
| 07 | Tools per agent | MCP servers, web search and a hidden browser for testing web pages. Each stays off until you switch it on. | `/features/tools` | Screenshot: an agent's tools, each with its switch |
| 08 | The Inbox | Questions, approvals and cards to review in one place. "Run this for me" for the commands an agent may not run. | `/features/inbox` | Screenshot: a question, an approval and a Run this for me |

There is no "Local-first and open" row: open source isn't a feature, and local-first is part of row 03.

**Get GizAi:** label Install; lead "One line in a terminal installs GizAi on your computer. It's free, for Linux,
macOS and Windows." Then:

- Linux and macOS: `curl -fsSL https://gizai.app/install.sh | sh`
- Windows · PowerShell: `irm https://gizai.app/install.ps1 | iex`
- Or build from source: the README's Install steps (https://github.com/Yotech-AI/gizai#install).

## What the design decides

- **Pink on the website (new rule).** The brandbook's "at most one pink thing per website screen" is lifted for the
  website. The logo's pink (`#f472b6` on dark, `#c0287a` on light) is the site's accent: the logo, the section labels
  and their 40 px line, one key word in a headline (review, ADA), the open feature row's number and its 160 px line, both
  cursors, the install prompts and the Review glyph in ADA's third point. Never on buttons, links, focus or body text,
  never behind text, and never a fill bigger than the cursor: no pink walls. Pink text is 7.2:1 on the dark `bg` and
  5.5:1 on white. Inside the app, pink still means only "needs you".
- **Why pink says "review".** The mark is the board's Review glyph, so the site's pink lands where you come in: the
  headline's "review", the third ADA point (You review and decide) with the Review glyph, the open row and the cursor.
- **The lines**, from the mountains' wireframe, all 1 px: two guides 40 px in from the window's sides run the whole page
  (`line`); a rule across the window starts each section; where lines cross, an 11 px plus in `text-3` (the plus the
  cursor carries). The feature numbers and the big ADA are outlined in 1 px `text-3`. The mesh itself is the mountain
  band. herdr.dev's bold numbers, rhythm and contrast, kept cleaner: no background grid, no pink fills.
- **The mountains, fixed.** GA-139's website hero (`mountains-hero.svg`, 3840 × 448 at 1:1, centred and cut at the
  sides, `band-hero-line` and `band-hero-glow`) is the page's lowest layer, `position: fixed` along the bottom of the
  window, like the app's band. The sections are see-through; what holds content stays solid and covers it: the tour,
  the screenshots, the install lines, the top bar. At the end of the page the footer stands in the band's empty sky.
  `aria-hidden`, `pointer-events: none`, no motion; it goes with forced colours, on paper and under 560 px high.
- **A feature row** (Feature row, State=Closed) is the number (Geist Bold 96, outlined in 1 px `text-3`), the name
  (Web/H2, in 520 px), one to three sentences (Web/Lead, `text-2`) and the feature page's address (Web/Code, `text-3`,
  with an arrow). **Open** (State=Open): the number fills pink under a 160 px pink line, the address takes the accent,
  and the screenshot or GIF (1120 × 700, drawn at 560 × 350) shows under the text, in the site theme.
- **Opening rows.** Row 01 is open when the page loads. Hover (after 150 ms at rest, so passing over rows doesn't open
  each one) or keyboard focus opens a row over 200 ms and closes the open one; with reduced motion it opens at once.
  When a row above closes, the page scrolls by the same amount, so the row under the pointer stays put. The whole row is
  one link to its address; over it the pointer becomes the cursor (Cursor/Kind=Open): a 56 px pink circle with a plus,
  ringed in `bg`. Keyboards and touch keep the accent focus ring.
- **The tour plays muted and loops**, with a Pause button (WCAG 2.2.2). With reduced motion it shows its first frame and
  a Play button. Its cursor (Cursor/Kind=Tour) is a pink ring with a dot, the Review glyph again: a suggestion for the
  tour cards (GA-148, GA-149).
- **Install lines** (Install line) show one command on one line: the system above it, the prompt in pink, the command in
  Web/Code, and Copy, which copies the command without the prompt and says Copied for 2 s.

## Placeholders

The Figma MCP can't place images or video, so these are frames that say their size, theme and content:

- **The product tour:** `Tour frame`, 2560 × 1664 drawn at 1280 × 832, a dark and a light version (the `Version`
  property). The sketch of the board behind the label shows what the tour opens on.
- **The feature screenshots and GIFs:** in Feature row, State=Open, 1120 × 700 drawn at 560 × 350, eight features in two
  themes each (`Media label` and `Media`), drawn as a wireframe of the app.

## What the MCP couldn't do

- Place images, GIFs or video: the tour and the screenshots are placeholders.
- Mark the band fixed and the top bar sticky for the prototype (`scrollBehavior` isn't in its plugin API). On the
  canvas the band sits at the end of the page; `In the window` shows it fixed. In Figma, Prototype → Scroll behavior →
  Fixed on the band (and Sticky on the top bar) would add it to the prototype.
- Close row 01 when another row opens in the prototype: an interactive component only changes itself.
