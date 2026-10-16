# Settings

Where each setting lives, how GizAi looks, what the screenshot settings do, and what happens when you close its window.

## The tabs

| Tab | What it holds |
|---|---|
| General | Quit, Updates, the Editor's folders, and Data: where GizAi keeps your data, and the worktrees of finished cards |
| Appearance | The font, text sizes, theme, density and the mountains |
| Notifications | A switch for each kind of notification |
| Agents and runs | Coding CLIs; Runs: limits, runs at once, memory for agent runs, Pause all agents, memory and Ask the Team Lead first; and Screenshots: what GizAi keeps of agents' screenshots, and the previews' ports |
| MCP servers | The outside services agents can use, and the built-in browser |
| GitHub and Bitbucket | gh and how pushes go, Check connection, and your Bitbucket login |

Each feature's doc says which settings it uses: [Updates](install.md#updates), [the Editor's folders](features/editor-and-terminal.md#open-a-folder), [Data](features/board.md#a-worktree-for-every-card), [Notifications](features/inbox.md#notifications), [Coding CLIs](features/cli.md), [Runs](features/board.md#when-agents-start) and [their limits](features/board.md#runs), [Screenshots](features/screenshots.md), [MCP servers](features/tools.md#mcp-servers) and [GitHub and Bitbucket](features/testing.md#review-on-github).

## Appearance

- **Font:** one font for the whole app: Atkinson Hyperlegible, JetBrains Mono, Inter, Geist or Hack, all bundled.
- **Text size:** one for the chat, one for the interface, and one for tasks and docs. Reading text grows, headings half as much, and IDs and labels stay put; the chat column gets wider with it.
- **Theme and density**, also with the t and d keys.
- **Mountains:** whether GizAi's wireframe mountains show along the bottom of the window, behind text and controls. They are on by default.

Changes show at once and are kept on this computer.

## Screenshots

Agents take [screenshots of what they built](features/screenshots.md), and the card shows them on its Screenshots tab. Their settings live in four places. A run takes screenshots only when its agent's browser and Take screenshots of what it built are on, the card's Screenshots before Review is on (until you set it, its project's Screenshots), and the project has a Preview command.

Each value has a default and a range. A value outside its range is refused when you save, with why, like "Screenshots per run must be from 1 to 30."

### Settings → Agents and runs → Screenshots

What GizAi keeps of the screenshots, for every project, and the ports the projects' previews get.

| Setting | Default | Range | What it does |
|---|---|---|---|
| Screenshots per run | 12 | 1 to 30 | GizAi keeps a run's first ones by file name and says in the run what it left out. The agent is told the limit. |
| Largest screenshot (MB) | 10 | 1 to 50 | A bigger file is left out, and the run says so. |
| Keep per card (sets) | 3 | 1 to 20 | A card keeps its newest sets with pictures. Older sets go, and so do older runs that took none. |
| Delete a finished card's screenshots after (days) | 30 | 0 to 365 | Counted from Done or Cancelled. 0 keeps them. |
| Preview ports | 47100 to 47999 | from 1024 to 65535, the lower port first | A run's preview gets a free port from this range and the nine after it, so the range must hold ten ports for each run at once (Runs → Runs at once): 30 with the default 3. |

Old sets go when GizAi starts and once a day, not the moment a newer set arrives ([Old screenshots go](features/screenshots.md#old-screenshots-go)).

### The project's form → Preview

Each project's own (its page → Edit → Preview): whether its cards get screenshots, the preview that serves a card's branch, and the sizes and themes the agents take.

| Setting | Default | Range | What it does |
|---|---|---|---|
| Screenshots: Take screenshots of this project's screens | on | on or off | Off: agents take none on its cards, except on a card whose own switch is on. |
| Preview command | empty | a command | Serves the card's branch with demo data while an agent that takes screenshots works on it. Empty: no preview, so no screenshots. GizAi runs it with `bash -c` (Git Bash on Windows) in the card's worktree, on a free port in `GIZAI_PREVIEW_PORT` (and `PORT`), with `GIZAI_PREVIEW_DIR` for a throwaway database, never your `.env` one, and ends it with the run ([The preview](features/screenshots.md#the-preview)). |
| Start time (seconds) | 120 | 10 to 600 | The agent starts once the page answers, or after this long without it, and then takes none. |
| Themes: Both themes, dark and light | off | on or off | On: each changed screen in the first theme, and the card's main screen in the other one too. Off: the theme the page opens in. |
| Sizes | Desktop 1280 × 800 and Mobile 390 × 844 | 1 to 4 sizes, each 320 to 3840 wide and 320 to 2400 high | Each changed screen is taken at every size. A size's name has at most 40 characters. |
| Whole page: The whole length of each page | off | on or off | Off: one window's height at each size. |

For GizAi's own project, use Window 1440 × 900 and Small 900 × 600 (its window's default and smallest size), with both themes on.

### The agent's form → Tools → Browser

| Setting | Default | Range | What it does |
|---|---|---|---|
| Take screenshots of what it built | on for Frontend, Design and QA agents (also ones made before it existed), off for the rest | on or off, only with the browser on | At the end of a run that changed a screen, the agent saves screenshots of it for the card's Screenshots tab, at the project's sizes. With the browser off it is off and can't be switched. |

The browser works on Claude Code only, so Codex and Gemini agents take none. Once you have listed the browser's tools, its `take_screenshot` must be on too.

### The card's Properties

| Setting | Default | Range | What it does |
|---|---|---|---|
| Screenshots: Screenshots before Review | its project's Screenshots switch | on or off | Off: the card's agents take none. Until you set it, it follows the project's switch; once set, Follow <project> under it goes back to that. |

## The window and the tray

GizAi keeps running when you close its window (Super+W on Omarchy, or the X button): the agents, the board check and the chat go on.

- A tray icon (top right in Omarchy's Waybar, the menu bar on macOS) has Open GizAi and Quit GizAi completely.
- Starting GizAi again also brings the window back.
- Settings → Quit quits too, and says how many runs and chat answers it stops.
