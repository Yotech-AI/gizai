# Settings

Where each setting lives, how GizAi looks, and what happens when you close its window.

## The tabs

| Tab | What it holds |
|---|---|
| General | Quit, Updates, the Editor's folders, and Data: where GizAi keeps your data, and the worktrees of finished cards |
| Appearance | The font, text sizes, theme, density and the mountains |
| Notifications | A switch for each kind of notification |
| Agents and runs | Coding CLIs, and Runs: limits, runs at once, memory for agent runs, Pause all agents, memory and Ask the Team Lead first |
| MCP servers | The outside services agents can use, and the built-in browser |
| GitHub and Bitbucket | gh and how pushes go, Check connection, and your Bitbucket login |

Each feature's doc says which settings it uses: [Updates](install.md#updates), [the Editor's folders](features/editor-and-terminal.md#open-a-folder), [Data](features/board.md#a-worktree-for-every-card), [Notifications](features/inbox.md#notifications), [Coding CLIs](features/cli.md), [Runs](features/board.md#when-agents-start) and [their limits](features/board.md#runs), [MCP servers](features/tools.md#mcp-servers) and [GitHub and Bitbucket](features/testing.md#review-on-github).

## Appearance

- **Font:** one font for the whole app: Atkinson Hyperlegible, JetBrains Mono, Inter, Geist or Hack, all bundled.
- **Text size:** one for the chat, one for the interface, and one for tasks and docs. Reading text grows, headings half as much, and IDs and labels stay put; the chat column gets wider with it.
- **Theme and density**, also with the t and d keys.
- **Mountains:** whether GizAi's wireframe mountains show along the bottom of the window, behind text and controls. They are on by default.

Changes show at once and are kept on this computer.

## The window and the tray

GizAi keeps running when you close its window (Super+W on Omarchy, or the X button): the agents, the board check and the chat go on.

- A tray icon (top right in Omarchy's Waybar, the menu bar on macOS) has Open GizAi and Quit GizAi completely.
- Starting GizAi again also brings the window back.
- Settings → Quit quits too, and says how many runs and chat answers it stops.
