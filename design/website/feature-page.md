# Website: the feature page template, desktop (GA-152)

One template for the eight feature pages, at `/features/<slug>`, designed in Figma for Jefsev's review. It builds on
the homepage (GA-144, notes in `design/website/README.md` on that card's branch) and changes two things there too: the
top bar gets the page's line art as its bottom border, and the footer becomes a real footer. Desktop (1440 wide), in
dark and light. Mobile, the other pages and the website's build come later.

- **Figma:** the file Giz-AI, page **Website: Feature page (desktop)** (node `85:2`):
  https://www.figma.com/design/Q2dp2xiv8MMm90Xwj3ToCI/Giz-AI?node-id=85-2
  - `Feature page` (`89:2637`): the template, a component, with its named slots. The blue dashed notes on its left say
    how each slot is filled.
  - `Feature page · 01 The board decides who works · Dark` (`90:665`) and `· Light` (`90:1119`).
  - `Feature page · 05 The Editor and Terminal, built in · Dark` (`91:1463`) and `· Light` (`91:1908`).
    All four are instances of `Feature page`: the same template, filled with other content.
  - `Feature page components` (`85:3`): every part of the template, as components (below).
  - `About this page` (`93:2285`), above the template: the choices to check, and what the MCP couldn't do.
- **On the homepage** (page Website: Home, `37:2`): `Home · Dark` (`40:27`) and `Home · Light` (`41:292`) now use the
  components Top bar and Footer, and so do the eight windows in `In the window · Dark` (`67:2341`) and `· Light`
  (`67:2571`). The new footer is 319 px taller: the page is now 1440 × 6152, the window frames moved down by as much,
  and "At the end" is now scrolled 5252 px. GA-144's notes on the top bar and the footer say what changed.

## The template, top to bottom

| # | Part (component) | What it holds |
|---|---|---|
| 1 | Top bar | The homepage's bar, sticky, with the line art as its bottom border. |
| 2 | Feature brandbox | The breadcrumb; the number in pink under a 160 px pink line; the name as the H1; the intro; the screenshot or GIF, 1280 × 832, with a plus at each corner. |
| 3 | Feature paragraphs | A rule; one centred column of two or three paragraphs, each an H2 above its text; under the last one, the feature's doc on GitHub. |
| 4 | Feature FAQ | A rule; the label FAQ and an H2 on the left; three to six questions on the right. Left out when a feature has no questions worth asking. |
| 5 | Pager | Previous and next, two cells split by a divider. |
| 6 | Get GizAi | The install section from the homepage. |
| 7 | Footer | Shared with the homepage. |
| 8 | Sky | 360 px of empty page, where the mountains show at the end. |

Behind it all, as on the homepage: the two guides 40 px in from each side, running from the top of the window to the
footer's bottom rule, and the mountains, fixed along the bottom of the window (`position: fixed`, `aria-hidden`).

## The slots, and how each page fills them

Every feature page is the same template. A page fills these, and nothing else:

| Slot | In Figma | What goes in |
|---|---|---|
| Number | Brandbox › Number | The feature's number on the homepage, `01` to `08`. |
| Title | Brandbox › Title | The feature's name as on its homepage row, without a full stop. The page's only H1. |
| Intro | Brandbox › Intro | One or two sentences on what the feature does for you. Also the page's meta description. |
| Media | Brandbox › Media (Feature media) | The feature's screenshot or GIF, a dark and a light version (below). |
| Paragraphs | Paragraphs › the Paragraphs slot | Two or three Paragraph instances: Heading (an H2) and Text, two to five sentences each. |
| FAQ | FAQ › the FAQ slot, and FAQ title | Three to six FAQ items (Question, Answer), under the title "Questions about …". Show FAQ off leaves the section out. |

And these follow from the slots: the breadcrumb's last item (Brandbox › Breadcrumb › Current) is the Title; Docs
(Paragraphs › Docs) is `docs/features/<slug>.md`; previous and next (Previous and next › Previous, Next: Number and
Name) are the neighbours in the table below; the page's `<title>` is the Title, a dash and GizAi ("The board decides
who works – GizAi").

In Figma, Number, Title and Intro are text properties of Feature brandbox; Paragraphs and FAQ are Figma slots (a
slot takes any number of instances); Media, Breadcrumb, Previous and Next are exposed nested instances, so their
properties show on the page instance. For dark and light, set the instance's colour mode (GizAi colour: Dark or Light)
and Theme on its Top bar and Footer.

### The eight pages

| # | Title (H1) | Address | Previous | Next | Docs | Media, from GA-148's captures |
|---|---|---|---|---|---|---|
| 01 | The board decides who works | `/features/board` | 08 | 02 | `docs/features/board.md` | `01-board` |
| 02 | A Team Lead you talk to | `/features/team-lead` | 01 | 03 | `docs/features/team-lead.md` | `03-chat` |
| 03 | Bring your own CLI | `/features/cli` | 02 | 04 | `docs/features/cli.md` | `07-clis` |
| 04 | Tested before you see it | `/features/testing` | 03 | 05 | `docs/features/testing.md` | `04-review` |
| 05 | The Editor and Terminal, built in | `/features/editor-and-terminal` | 04 | 06 | `docs/features/editor-and-terminal.md` | `05-editor`, `06-terminals` |
| 06 | Memory | `/features/memory` | 05 | 07 | `docs/features/memory.md` | `08-memory` |
| 07 | Tools per agent | `/features/tools` | 06 | 08 | `docs/features/tools.md` | `09-tools` |
| 08 | The Inbox | `/features/inbox` | 07 | 01 | `docs/features/inbox.md` | `10-inbox` |

Previous and next follow the homepage's order and wrap round: 01's previous is 08, 08's next is 01. Docs links to
`https://github.com/Yotech-AI/gizai/blob/main/<path>`; the `docs/features/` files come with GA-97. Check `07-clis` for
other companies' logos before it goes on the site.

## Heading levels

One H1 per page; every section starts with an H2.

- **H1:** the feature's name (Title), in Web/Display. Nothing else on the page is an H1.
- **H2:** each paragraph's heading (Web/H2); the FAQ's title (Web/H2); Get GizAi (Web/H2 here, where the homepage
  draws it at Web/H1's 48 px).
- **H3:** each FAQ question, drawn in Web/H4, as `<h3><button aria-expanded="false" aria-controls="…">…</button></h3>`.
- **Not headings:** the breadcrumb (a `nav`, aria-label "Breadcrumb"); the number (`aria-hidden`: the H1 says the
  name); the section labels FAQ and Install; the media's label (a design note); the previous and next names (links in
  a `nav`, aria-label "Features"); the footer's column titles (the labels of their link lists, `nav` with
  aria-labelledby).

## The parts

All are in `Feature page components` (`85:3`). Their colours are the GizAi colour variables, so they follow the
mode of the frame they sit in.

- **Top bar** (`85:84`; Theme = Dark, Light): the homepage's bar. Its bottom border is the page's rule (the Rule
  component): the 1 px `line` with an 11 px plus where it crosses each guide; the plus hangs 5 px under the bar. The
  guides run up through the bar to the top of the window. On a feature page, Tour, ADA and Features go to the
  homepage's sections (`/#tour`, `/#ada`, `/#features`); Get GizAi scrolls to the page's own install section. Theme
  sets the logo, the switch's lit half and the colour mode.
- **Footer** (`85:271`; Theme = Dark, Light): see "The footer" below.
- **Feature brandbox** (`89:175`): see "The brandbox" below.
- **Breadcrumb** (`86:139`): Home › Features › Current, in Web/Small. Home (`/`) and Features (`/#features`) in
  `text-3`, underlined on hover; the current page in `text-2`, not a link, `aria-current="page"`.
- **Feature media** (`86:145`): 1280 × 832, `surface/side` with a 1 px `line-2` border. Kind, Shows and Size fill the
  label, a design note that isn't on the site; Still turns on the capture itself. With Still off it is a wireframe of
  the app, as on the homepage.
- **Paragraph** (`86:201`): an H2 (Web/H2) and, 16 px under it, its text (Web/Lead, `text-2`), both as wide as the
  column. 72 px under each; no lines between them.
- **Feature paragraphs** (`89:259`): a rule, then one column, 680 wide and centred on the page (x 380 to 1060), 120 px
  under the rule and 120 px above the next section. In it, the Paragraphs slot, then Docs, 72 px under the last
  paragraph: "In the docs" (Web/Eyebrow, `text-3`) over the address (Web/Code, accent, with an arrow), like the
  address on a feature row. The column is centred; its text is left-aligned. 680 is the FAQ's column width, about 70
  characters a line in Web/Lead.
- **FAQ item** (`86:223`; State = Closed, Open): see "The FAQ" below.
- **Feature FAQ** (`89:290`): a rule; on the left the Section label FAQ and the title (H2); on the right the FAQ slot,
  680 wide, with a 1 px line under the last question.
- **Pager link** (`87:148`; Direction = Previous, Next; State = Default, Hover) and **Pager** (`87:151`): see
  "Previous and next" below.
- **Get GizAi** (`87:199`): the homepage's install section: the label Install, Get GizAi (H2), the line on what it
  costs and runs on, an Install line per system with Copy, and Or build from source.
- **Leaves the site** (`86:106`): a 12 px arrow, up and to the right, in `text-3`, after a footer link that leaves the
  site.
- From GA-144, unchanged: Rule, Plus mark, Section label, Install line, Logo, Button, Link, Theme switch, Mountains.

## The brandbox

The open feature row from the homepage, grown into a page, so the row you clicked opens into its page:

- The breadcrumb, 40 px under the top bar.
- 64 px under it, the 160 px pink line (2 px), and 40 px under the line the number: Geist Bold 96, `brand/pink`, the
  same as the open row's. The name (H1, Web/Display, `text`) starts at x 240, its cap height level with the top of the
  number, at most 1000 wide; the intro (Web/Lead, `text-2`, 640 wide) 24 px under it.
- 96 px under the intro, the media: 1280 × 832 from x 80, guide to guide less 40 px a side, with an 11 px plus outside
  each corner, as the tour on the homepage.
- No buttons: the top bar's Get GizAi and the install section at the end do that job.

The media is the feature's screenshot or GIF from the product tour's captures (GA-148): 2560 × 1664 at scale 2, drawn
at 1280 × 832, in a dark and a light version that swap with the site theme. Build a GIF as a muted, looping video
(WebM or MP4) with a Pause button, as the tour (WCAG 2.2.2); with reduced motion it shows its first frame. Alt text
says what it shows (the Shows line). It is solid, so it covers the mountains.

## The FAQ

- Every question starts closed. A click, Enter or Space opens it over 200 ms (at once with reduced motion); the same
  closes it. Several can be open at once.
- Closed: the question (Web/H4, `text`) and a 16 px plus in `text-2`, with a 1 px `line` on top.
- Open: the answer (Web/Body, `text-2`, 600 wide) 12 px under the question; the plus becomes a minus in `text`; a 40 px
  pink line (2 px) on the item's top line marks it open, as the open feature row.
- In Figma's prototype, a click opens and closes an item (an interactive component).

## Previous and next

- Two cells, guide to guide, under a rule with a plus on each guide and on the divider; the 1 px divider runs down to
  the next section's rule, with a plus there too.
- Each cell is one link: "← Previous" or "Next →" (Web/Eyebrow, `text-3`), then the number (Geist Bold 48, outlined
  in 1 px `text-3`, as the closed feature rows) and the name (Web/H3, `text`). Next is right-aligned.
- Hover and keyboard focus fill the number pink under a 160 px pink line, as the open row; the label turns `text-2`.
  The focus ring stays the accent. In the prototype, hover shows it.

## The footer (every page)

The logo and one line on what GizAi is, then three columns of links; the page's lines close here.

- **Line art:** a rule above and below, a 1 px divider between the columns, and a plus wherever two lines cross. The
  guides end on the bottom rule. The footer is see-through: the mountains, fixed along the bottom of the window, stand
  behind it, and under it is the band's empty sky (360 px).
- **Columns** (x 40 to 520, 960, 1180 and 1400; 56 px above the content, 64 below): column titles in Web/Eyebrow,
  `text-3`; links in Web/Small, `text-2`, 10 px apart, `text` and underlined on hover.

| Column | Link | Goes to |
|---|---|---|
| GizAi | the logo (32 px), then "A free desktop app where a team of AI coding agents works through your board." | the logo links to `/` |
| Features | 01 The board decides who works | `/features/board` |
| | 02 A Team Lead you talk to | `/features/team-lead` |
| | 03 Bring your own CLI | `/features/cli` |
| | 04 Tested before you see it | `/features/testing` |
| | 05 The Editor and Terminal, built in | `/features/editor-and-terminal` |
| | 06 Memory | `/features/memory` |
| | 07 Tools per agent | `/features/tools` |
| | 08 The Inbox | `/features/inbox` |
| Get started | Get GizAi | `/#get-gizai`, the install section on the homepage |
| | Docs ↗ | `https://github.com/Yotech-AI/gizai/tree/main/docs` |
| | Releases ↗ | `https://github.com/Yotech-AI/gizai/releases` |
| | View on GitHub ↗ | `https://github.com/Yotech-AI/gizai` |
| YoTech | Made by YoTech ↗ | YoTech's website (the address is Jefsev's to give) |
| | Apache-2.0 licence ↗ | `https://github.com/Yotech-AI/gizai/blob/main/LICENSE` |

The numbers in Features are Web/Code, `text-3`. ↗ (the component Leaves the site) marks a link that leaves the site.
In Figma each link carries its address as an annotation.

## The words: the two examples

### 01 The board decides who works (`/features/board`)

- **Breadcrumb:** Home › Features › The board decides who works
- **Number:** 01. **Title (H1):** The board decides who works
- **Intro:** Each column on your board has its own agents. Cards move from the agent that builds to the agent that
  tests, and then to you.
- **Media:** a GIF, dark and light. Shows: "The board: an Auto column picks up the next card, and the card gets its own
  git worktree and branch." In Figma: the still `01-board/open-card-before.png`.
- **Paragraphs:**
  - **Columns hold the agents** (H2): Your board's columns run from Backlog to Done. Each column holds the agents that
    work its cards. In an Auto column, they pick up cards by priority as soon as they have room. In a Manual column,
    nothing starts until you press Run.
  - **A worktree for every card** (H2): When an agent picks up a card, the card gets its own git worktree and branch,
    started from main. Agents work side by side, never in each other's files or in yours. A new worktree starts ready:
    the files you choose copied from your checkout, dependencies installed and your setup command run.
  - **The flow is yours** (H2): When an agent is done, its card moves on to the next column: from In progress to
    Testing, and from Testing to your Review. On the Team page you add your own columns, like Design, link each one to
    the next and drag agents onto them.
- **Docs:** `docs/features/board.md`
- **FAQ title (H2):** Questions about the board
- **FAQ:**
  1. **What is the difference between Auto and Manual columns?** In an Auto column, the agents pick up cards by
     priority whenever they have room. In a Manual column, a card waits until you press Run. Deploy is Manual from
     the start, so nothing is released without you.
  2. **Can I change the columns?** Yes. On the Team page, under Workflow, you add, remove and reorder columns, choose
     where each one sends its cards, and drag agents onto them. A Design column with a design agent, linked to Review,
     is one example.
  3. **Why does every card get its own worktree?** So several agents can work at the same time without getting in each
     other's way. Each card has its own folder and branch, started from main, and your own checkout stays as it was.
  4. **How many agents work at once?** You decide. Each agent has a number of cards it may take at once, and in
     Settings, Runs sets how many runs go at the same time, for all agents together.
  5. **Can I stop agents from starting new work?** Yes. Pause all agents stops new starts from Auto columns. Run still
     starts an agent when you press it.
- **Previous:** 08 The Inbox. **Next:** 02 A Team Lead you talk to.

### 05 The Editor and Terminal, built in (`/features/editor-and-terminal`)

- **Breadcrumb:** Home › Features › The Editor and Terminal, built in
- **Number:** 05. **Title (H1):** The Editor and Terminal, built in
- **Intro:** A code editor and your own terminals, inside GizAi. Open a project or a card's worktree, check its git
  status, edit, and run commands in a shell under the code.
- **Media:** a GIF, dark and light. Shows: "The Editor on a project: git's marks in the file tree, and a shell in the
  panel under the code." In Figma: the still `06-terminals/show-in-editor-after.png`.
- **Paragraphs:**
  - **A small editor that knows git** (H2): The Editor opens a project, a folder from your Editor folders, or a card's
    worktree. The file tree shows git's status, and the top bar says when the folder is behind main. Update catches up
    with a fast-forward, never over your own changes. For a full IDE, Open in hands the folder to VS Code, Zed or
    PhpStorm.
  - **Your own shell, as many as you like** (H2): Terminal opens a panel under the code, in the folder's root. Start
    more terminals as tabs, give one its own page, or rename it. A terminal keeps running until you close it or quit
    GizAi, even when you close the window.
  - **Work on a card yourself** (H2): A card assigned to you has Open in editor. GizAi prepares the card's worktree as
    it would for an agent and pins the card to you, so agents leave it alone. A bar above the code lets you commit,
    push and hand the card over to Testing.
- **Docs:** `docs/features/editor-and-terminal.md`
- **FAQ title (H2):** Questions about the Editor and Terminal
- **FAQ:**
  1. **Is the Editor free?** Yes. The Editor, the terminals and a card's changes are free, like the rest of GizAi.
  2. **Can I edit a card while an agent works on it?** Not while it works: the Editor shows the worktree read-only, and
     a new terminal there waits until the run ends. When no agent is working on it, Edit anyway lets you change the
     card's files, and the agent finds your changes on its next run.
  3. **Does it replace my IDE?** It isn't meant to. It's for quick edits and for checking an agent's work. Open in hands
     the folder to VS Code, Zed or PhpStorm when you need more.
  4. **What happens to my terminals when I close the window?** They keep running. A terminal stops when you close it,
     type exit, or quit GizAi.
- **Previous:** 04 Tested before you see it. **Next:** 06 Memory.

The copy is written from `docs/features/board.md` and `docs/features/editor-and-terminal.md` (GA-97's branch), in
new sentences. In the examples the first question is open, to show both states; on the site every question starts
closed.

## What the design decides

- **The brandbox continues the open row.** The pink number under its pink line, the name and the media are what the
  open feature row shows on the homepage, at page size; the page is the row you clicked.
- **The top bar sits in the grid.** Its bottom border is the rule every section starts with, and the guides run up
  through it, so the bar is the first cell of the page's lines.
- **The footer closes the grid.** A second rule ends the guides, and the column dividers make it a strip of cells like
  ADA's three points. The mountains stay behind it, then show alone in the sky.
- **Paragraphs in one centred column.** Each H2 sits above its text and the doc's address closes the column, so the
  paragraphs read top to bottom. The FAQ under them keeps its title on the left, which sets the two sections apart.
  The paragraphs belong together, so space parts them, not lines.
- **Pink, as on the homepage:** the number and its 160 px line, the section label FAQ, an open question's 40 px line,
  a hovered previous or next. Never on links, buttons or body text.
- **One H2 size.** Every section heading on a feature page is Web/H2, Get GizAi included, so the H1 in Web/Display
  stands alone.
- **The full names in the footer**, numbered as on the homepage, so a feature has one name everywhere.
- **Real stills.** The examples show GA-148's captures, the scenes the GIFs will be made from; the template and the
  homepage keep the labelled wireframe.

## What the MCP couldn't do

- Play a GIF or video: in the examples, a still from the same scene stands in for each GIF.
- Make the top bar sticky and the mountains fixed in the prototype: the plugin API has no scroll behaviour. In Figma,
  Prototype → Scroll behavior → Sticky on the bar and Fixed on the band adds them; the homepage's In the window frames
  show both.
- It could place images after all: `upload_assets` puts a PNG in the file, and its image hash then fills any layer.
  GA-144's placeholders on the homepage could get their stills the same way.
