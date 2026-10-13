# The Editor and Terminal, built in

Open a project or a card's worktree inside GizAi, see its git status and edit it, with your own shell in a panel under the code or on its own page.

## Open a folder

The sidebar has an Editor section above Agents, and a Terminal section above Editor. Both open the same folders:

- a project's linked folder;
- a folder in Settings → General → Editor folders (`~/Herd` unless you change it);
- a card's worktree.

\+ on either section opens the folder picker.

## The Editor

A small code editor with syntax colours. It has:

- a file tree with git's status, tabs, and Ctrl+S to save;
- Go to file (Ctrl+P) and search in the folder (Ctrl+Shift+F);
- changes made outside GizAi, which show within a few seconds;
- **Open in**, which opens the folder in VS Code, Zed or PhpStorm when they're installed;
- a terminal panel under the code ([In the Editor](#in-the-editor)).

## Behind main

The top bar says how the folder stands against main on its remote, GitHub or another. It is fetched when the folder opens, every 10 minutes and on Fetch.

When the folder is behind, **Update** brings main up to date:

- with a fast-forward only, never over uncommitted changes and never switching branch;
- running composer install or npm ci only when a lock file changed.

The sidebar says "3 behind" on the folder's row.

## Terminals

Your own shell in any of those folders, as many as you like. Each terminal runs until you close it or quit GizAi, and closing the window keeps them running.

### In the Editor

The **Terminal** button in the Editor's top bar, between Fetch and Open in, shows a terminal panel under the code. When the folder has no terminal yet, it starts one in the folder's root.

- The panel's tabs are the folder's terminals, with +, **Own page** and **Hide**.
- Drag its edge to change its height, which is the same in every folder.
- Ctrl+\` shows or hides the panel, and Ctrl+Shift+\` starts a terminal in it.

### In the sidebar and on its own page

Each terminal has a row in the sidebar, with what runs in it on the right, or "ended". It also has its own page:

- its folder's terminals as tabs, with + for another one there, and Ctrl+PgDn and Ctrl+PgUp to switch;
- find (Ctrl+Shift+F);
- Rename (F2 or a double-click);
- Show in Editor.

### Closing a terminal

- Closing a terminal that a program runs in asks first.
- `exit` closes it.
- A shell that ends with another code says so, with **Start again**.

## A card's worktree

- While an agent works in a card's worktree, the Editor shows it read-only, and a new terminal there waits until the run ends.
- A card that isn't pinned to you stays read-only in the Editor until **Edit anyway**, which puts your changes in its worktree, uncommitted, for the agent's next run. A shell can change files too: the folder picker asks for Edit anyway before it starts a terminal there, and the Editor's terminal panel stays off until you press it.

### Work on a card yourself

A card assigned to you has **Open in editor** in its top bar. GizAi makes the card's worktree, prepared as for an agent, and pins the card to you, so agents leave it alone; from Backlog or To do it moves to In progress.

In the Editor, a bar above the code shows the card, its branch and how far it is, with three buttons:

- **Commit…**: a small box with the message and the files it commits;
- **Push**;
- **Hand over**: to Testing, or to Review when the card's Testing is off.

When QA sends the card back, the bar says so until you open the card.

## A card's changes

Every card has two tabs at the top: Card and Changes.

- **Changes** lists the files the card changed under their folders on the left, and shows every file's diff on the right, one below the other.
- The changes come from the card's worktree, uncommitted files included, since its branch left main.
- In the Runs tab, a run that committed has **Show changes**: only what that run committed.
- Keys: n and p for the next and previous file, Alt+↓ and Alt+↑ for the next and previous change, [ folds the list, and Esc goes back to Card.

After a merge, the changes stay readable. When a card's pull request is merged, GizAi first fetches main and keeps the card's range in two refs of your repository, `refs/gizai/merged/<ID>/base` and `/head`. So what the card changed can still be read after a merge commit, a squash or a rebase, offline, once its branch is gone; GizAi never pushes these refs.

## Free

The Editor, the terminals and a card's changes are free, like the rest of the app. [docs/PAID-FEATURES.md](../PAID-FEATURES.md) lists what stays free.

## Read more

- [Tested before you see it](testing.md): what happens after Hand over.
- [The board decides who works](board.md#a-worktree-for-every-card): how a card's worktree is made.
- [docs/PLATFORMS.md](../PLATFORMS.md): how the Editor reads files and finds VS Code, Zed and PhpStorm, and which shell a terminal starts and how it ends, on Linux, macOS and Windows.
