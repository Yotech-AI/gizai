# A code editor, free

Read what a card changed and edit a project's files inside GizAi, with no extra app and nothing to pay.

## A card's changes

Every card has two tabs at the top: Card and Changes.

- **Changes** lists the files the card changed under their folders on the left, and shows every file's diff on the right, one below the other.
- The changes come from the card's worktree, uncommitted files included, against main.
- In the Runs tab, a run that committed has **Show changes**: only what that run committed.
- Keys: n and p for the next and previous file, Alt+↓ and Alt+↑ for the next and previous change, [ folds the list, and Esc goes back to Card.

After a merge, the changes stay readable. When a card's pull request is merged, GizAi first fetches main and keeps the card's range in two refs of your repository, `refs/gizai/merged/<ID>/base` and `/head`. So what the card changed can still be read after a merge commit, a squash or a rebase, offline, once its branch is gone; GizAi never pushes these refs.

## The Editor

Sidebar → Editor, above Agents: a small code editor with syntax colours. It opens

- a project's linked folder;
- a folder in Settings → General → Editor folders (`~/Herd` unless you change it);
- a card's worktree.

What it has:

- a file tree with git's status, tabs, and Ctrl+S to save;
- Go to file (Ctrl+P) and search in the folder (Ctrl+Shift+F);
- changes made outside GizAi, which show within a few seconds;
- **Open in**, which opens the folder in VS Code, Zed or PhpStorm when they're installed;
- a terminal panel under the code ([Terminals, free](terminal.md#in-the-editor)).

A worktree is read-only while an agent works in it. Another's card stays read-only here until **Edit anyway**, which puts your changes in its worktree, uncommitted, for the agent's next run.

## Behind main

The top bar says how the folder stands against main on its remote, GitHub or another. It is fetched when the folder opens, every 10 minutes and on Fetch.

When the folder is behind, **Update** brings main up to date:

- with a fast-forward only, never over uncommitted changes and never switching branch;
- running composer install or npm ci only when a lock file changed.

The sidebar says "3 behind" on the folder's row.

## Work on a card yourself

A card assigned to you has **Open in editor** in its top bar. GizAi makes the card's worktree, prepared as for an agent, and pins the card to you, so agents leave it alone; from Backlog or To do it moves to In progress.

In the Editor, a bar above the code shows the card, its branch and how far it is, with three buttons:

- **Commit…**: a small box with the message and the files it commits;
- **Push**;
- **Hand over**: to Testing, or to Review when the card's Testing is off.

When QA sends the card back, the bar says so until you open the card.

## Free

The code editor and a card's changes are free, like the rest of the app. [docs/PAID-FEATURES.md](../PAID-FEATURES.md) lists what stays free.

## Read more

- [Terminals, free](terminal.md): a shell in the same folders.
- [Tested before you see it](testing.md): what happens after Hand over.
- [docs/PLATFORMS.md](../PLATFORMS.md): how the Editor reads files and finds VS Code, Zed and PhpStorm on Linux, macOS and Windows.
