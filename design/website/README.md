# Website: the homepage, desktop (GA-144)

GizAi's homepage, designed in Figma from GA-52's brandbook, for Jefsev's review. Desktop first: the sub-pages (one per
feature) and mobile come later, and so does building the site.

- **Figma:** the file Giz-AI, page **Website: Home (desktop)** (node `37:2`):
  https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=37-2
  - `Home · Dark` (`40:27`) and `Home · Light` (`41:292`): the whole page, 1440 × 6560, top to bottom.
  - `Features · row 01 on hover · Dark` (`42:557`) and `· Light` (`42:673`): the features with row 01 open.
  - `Homepage components` (`37:16`): Cursor, Theme switch, Code block, Feature row, Tour frame and Note.
  - Blue dashed notes on the canvas: right of each page, the page each feature row opens and what its screenshot shows;
    left of the dark page, how the parts behave. Each row also carries its address as a Dev Mode annotation.
  - In Figma's prototype view, hovering a feature row opens it (an interactive component, While hovering).
- **From the brandbook** (page Brandbook, `5:2`): the Logo, Button, Badge, Card label and Mountains components, the
  `GizAi colour` variables (modes Dark and Light), the `GizAi size` radii and the `Web/…` text styles. Each page frame
  sets the colour mode, so one set of components draws both themes.

## The page, top to bottom

1. **Top bar**, 72 px, sticky: the logo; Tour, Features, ADA and Get GizAi, which scroll to their section; View on
   GitHub (https://github.com/Yotech-AI/gizai); the dark and light switch.
2. **Hero**: the mark at 72 px, the headline, one line, Get GizAi (primary) and View on GitHub (secondary), then the
   product tour in a 1280 × 832 frame.
3. **Features**: ten numbered rows, full width, split by thin lines, with the number large in low contrast.
4. **ADA**: the term, its copy and three points, on the `side` surface.
5. **Get GizAi**: the README's install commands for Linux, macOS and Windows in mono blocks. No download buttons.
6. **Footer**: the logo, Made by YoTech, GitHub and MIT licence.
7. **The mountain band** along the bottom: the Mountains component, Website hero (GA-139's `mountains-hero.svg`),
   3840 × 448 at 1:1, centred and cut at the sides, at Medium (`band-hero-line`, `band-hero-glow`).

## The words

**Headline (Display):** Your agents build it. You review.

**The line under it (Lead):** GizAi is a free desktop app where a team of AI coding agents works through your board
and hands every card back to you for review.

**Features:** eyebrow Features, title What GizAi does, lead "One app, from the card you write to the pull request you
merge." The rows are the card's ten, in its order and words; each name is an H2 without its full stop, as the
brandbook's headings are.

| # | Name (H2) | Opens | Hover shows |
|---|---|---|---|
| 01 | The board decides who works | `/features/board` | GIF: an Auto column picks up the next card, which gets its own branch |
| 02 | A Team Lead you talk to | `/features/team-lead` | GIF: the chat turns a request into cards, with a one-click approval |
| 03 | Tested before you see it | `/features/testing` | Screenshot: a card in Testing, the QA run and its pull request |
| 04 | A code editor, free | `/features/code-editor` | Screenshot: the Editor on a card worktree, git marks, search, Open in |
| 05 | Terminals, free | `/features/terminal` | GIF: a terminal in a card worktree, in the Editor's panel |
| 06 | Bring your own CLI | `/features/cli` | Screenshot: the Coding CLIs settings, accounts and usage limits |
| 07 | Memory | `/features/memory` | Screenshot: linked notes and the graph |
| 08 | Tools per agent | `/features/tools` | Screenshot: an agent's tools, each with its switch |
| 09 | The Inbox | `/features/inbox` | Screenshot: a question, an approval and a Run this for me |
| 10 | Local-first and open | `/features/local-first` | Screenshot: the same board on Linux, macOS and Windows |

**ADA**

- Eyebrow: A new kind of app. Headline (H1): **Not an IDE. An ADA.**
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

**Get GizAi:** eyebrow Install; lead "GizAi builds from source on your own computer: there are no downloads or
installers. Its install script checks what is missing, builds GizAi and installs it for you alone, without sudo or
administrator rights." Then what every system needs, and per system its extra needs, the commands from the README's
Install section, and where it installs.

## What the design decides

- **Pink only for the brand.** The logo is pink (`#f472b6` on dark, `#c0287a` on light); the one other pink on the
  first screen is the Review badge in the picture of the board, as the brandbook allows. Buttons, links, focus and the
  cursor are the accent blue.
- **The hero shows the mark, not the wordmark again.** The top bar already has the logo with its name, so the hero's
  logo is the G at 72 px: the first big picture is the Review glyph the brand is built on.
- **A feature row** (Feature row, State=Closed) is the number (Geist Bold 88, `line-2`), the name (Web/H2, one line in
  520 px) and the text (Web/Lead, `text-2`). **On hover and on keyboard focus** (State=Hover) the row grows to show a
  screenshot or GIF of 1120 × 700, drawn at 560 × 350, under the text it shows; the number brightens to `text-3`; the
  mouse cursor becomes Cursor/Kind=Open, a 56 px accent circle with a plus and a 2 px `bg` ring. One row is open at a
  time; it opens over about 200 ms, at once with reduced motion. A click opens the feature page.
- **The tour plays muted and loops**, with a Pause button (WCAG 2.2.2). With reduced motion it shows its first frame and
  a Play button. Its cursor (Cursor/Kind=Tour) is a circle with a dot.
- **Install commands** stack by system, each in a Code block with a Copy button that copies only the commands. The
  longest line fits a 880 px block without wrapping.
- **The mountain band** sits under the footer, which stands in its empty sky; nothing else sits on it.

## Placeholders

The Figma MCP can't place images or video, so these are frames that say their size, theme and content:

- **The product tour:** `Tour frame`, 2560 × 1664 drawn at 1280 × 832, a dark and a light version (the `Version`
  property). The sketch of the board behind the label shows what the tour opens on.
- **The feature screenshots and GIFs:** in Feature row, State=Hover, 1120 × 700 drawn at 560 × 350, ten features in two
  themes each (`Media label` and `Media`).
