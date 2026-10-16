# Screenshots of what was built

Agents that build a screen take screenshots of it at the end of their run, from a preview of the card's branch with demo data. The card shows them on its Screenshots tab, so you see what was built before you read the pull request.

## Who takes them, and when

- **The builder** (by default the Frontend or Design agent) takes a set at the end of its run when its change shows on a screen: after its last commit and before its result line, so the pictures show the commit GizAi pushes. It takes the screens it changed, in the states the card names, with their empty and error states when it touched them, at each of the project's sizes.
- **QA** adds only what the builder's set doesn't show: each failure it found, and at most four acceptance criteria it checked, each captioned with the criterion.
- **Only when all four hold:** the agent runs on Claude Code with the browser, its Take screenshots of what it built is on, the card's Screenshots before Review is on (until you set it, its project's Screenshots), and the project has a Preview command. Otherwise the run goes as before, with nothing about screenshots in its prompt.

The run's prompt says where to save them, the sizes, the themes and the limit. The agent saves each picture to a file in the worktree's `.gizai-tmp/screenshots/`, numbered in the order to look at them (`02-search-results-desktop-dark.png`), with a caption for each in `captions.json`. It opens them only when it must, so a set costs it almost no tokens.

### What GizAi keeps

At the end of the run, however it ended (also after Stop or a limit), GizAi checks the folder before it empties it, and keeps on the card:

- the folder's own files only: no subfolders and no links;
- PNG, JPEG and WebP images, by their content, not their name;
- at most Largest screenshot (10 MB) each, 16 to 16,384 pixels a side and at most 50 megapixels;
- the first ones by file name, up to Screenshots per run (12).

A caption is plain text of at most 200 characters. A picture without one gets one made from its name: "Board with cards, desktop, dark". The run's output and the set say what was left out and why. A run stopped by GizAi quitting keeps none.

Screenshots stay on this computer, in GizAi's data folder. They are never committed (`.gizai-tmp/` is kept out of git) or added to the pull request.

## The preview

The project's **Preview command** (its form → Preview) serves the card's branch. When a run starts whose agent takes screenshots, GizAi runs it after the worktree is prepared (New worktrees) and before the agent starts:

- with `bash -c` (Git Bash on Windows) in the card's worktree, in a process tree of its own, its output in `.gizai-tmp/preview/preview.log`;
- with `GIZAI_PREVIEW_PORT`: a free port from Settings → Agents and runs → Screenshots → Preview ports that no other run has, with the nine after it for a second server (an API, Vite). `PORT` has the same port, `GIZAI_PREVIEW_URL` is `http://127.0.0.1:<port>` and `GIZAI_PREVIEW_DIR` is `<worktree>/.gizai-tmp/preview`;
- without `CLAUDECODE`, so a server that detaches when it sees Claude Code (Astro's) stays in the tree.

The agent's own commands get the three `GIZAI_PREVIEW_*` variables too, not `PORT`, so a project script it runs to load the demo data again finds the same database.

**Demo data, never yours.** The command fills a throwaway database in `GIZAI_PREVIEW_DIR` with demo data and serves that, never the database in the worktree's `.env`, which is a copy of your checkout's. The folder goes with the temp folder when the run ends. The agent is told to shoot only the preview's address: never a `.test` site, a live site or a real account.

GizAi waits until the port answers, at most the project's Start time, then starts the agent with the address in its prompt. When the preview doesn't answer in time (or ends first, or no port is free), the run starts anyway and takes no screenshots: the prompt, the run's output and the set say why, with the last lines of its log. The card is never held for a preview.

When the run ends, GizAi ends the preview's whole tree (SIGTERM, then SIGKILL after 3 s; on Windows its Job Object), then collects the screenshots. Quitting GizAi ends every live preview at once, and a GizAi that starts again after a crash ends one the previous GizAi left running.

For example:

- a static Astro site: `npx astro dev --host 127.0.0.1 --port "$GIZAI_PREVIEW_PORT" --ignore-lock` (`--ignore-lock` keeps Astro in the foreground);
- a Laravel project: a script of its own, like `composer preview`, that points the app at an SQLite file in `$GIZAI_PREVIEW_DIR`, migrates and seeds it, builds the assets and runs `php artisan serve --no-reload` on the port;
- GizAi itself: `npm run browser` ([below](#gizai-itself-in-a-plain-browser)).

[The research, §3.5](../research/screenshots.md#35-the-page-up-per-stack), has a recipe per stack.

## The Screenshots tab

A card's page has three tabs at the top: **Card | Changes | Screenshots**. The Screenshots tab is at `#/task/<id>/screenshots`, and one run's set at `#/task/<id>/screenshots/<run id>`. It shows on a card whose project takes screenshots or whose own switch is on, and on a card that has sets.

- **Its count** is the newest round: the newest builder's set and QA's sets after it, like 4 + 2 = 6. Without any it says None.
- **One set per run, newest first**, each with the agent, its answer (like Ready for testing, QA passed or Stopped at the time limit), when, how many, the commit and **View the run**. Runs older than the newest two fold into one line, like "4 older runs".
- **Each thumbnail** shows the picture at its own proportions, with its caption, its size and theme, and its file name.
- **Against the run before** (the same agent's last set before it): when they share file names, the set says "2 changed, 1 unchanged, 1 new", and each picture says Changed, Unchanged or New. The names the run before had and this one hasn't are listed under the set: "Not taken this time".
- **A set says what was left out**, and why. A run whose preview didn't start says why, and a run that took none says so.

The card's Attachments list only the files people added.

## The larger view

A click on a thumbnail opens it over the page, fitted, with its caption in its foot.

- **Fit | 100%** (key 1); at 100%, drag or scroll to look around.
- **Previous** and **Next** (← and →) go through the set, and **Esc** closes.
- **Open** opens the picture in the system's image viewer.
- **Compare** (key C) shows it with the run before's picture of the same file name: **Side by side**, or one picture with a **Slider** to drag. On an Unchanged picture it says so and shows one picture; on a New one Compare is off. In Compare, ← and → go to the set's next pair, and C goes back to one picture.

## On the board and in the Inbox

- **The board:** a card in Review whose newest round has screenshots shows a sign with their count after its pull request sign; cards in other columns show none. A click on the sign opens the Screenshots tab.
- **The Inbox:** the card's row says "6 screenshots" and opens the tab.

## When the tab is empty

It says why, with a link to the setting:

| Why | The link |
|---|---|
| The project has no preview yet | **Set up the preview** opens the project's form at Preview |
| The agent doesn't take them: its switch is off, or it has no browser | **Open Frontend Agent settings** (with the agent's name) opens its form at Tools → Browser |
| They're off for this card | **Show properties** opens Properties, with the focus on the card's switch |
| No run has taken any yet | **Screenshot settings** opens Settings → Agents and runs → Screenshots |

## The settings

Every part is a switch or a value with a range, in four places: the project's form → Preview (Screenshots, Preview command, Start time, Themes, Sizes, Whole page), the agent's form → Tools → Browser (Take screenshots of what it built), Settings → Agents and runs → Screenshots (how many a run keeps, the largest, how long cards keep them, the preview ports) and the card's Properties (Screenshots before Review). [docs/settings.md](../settings.md#screenshots) has each one's default, range and what it does.

## Old screenshots go

When GizAi starts and once a day:

- each card keeps its newest 3 sets with pictures (Keep per card), and older runs' sets go;
- a card in Done or Cancelled loses all of them after 30 days there (Delete a finished card's screenshots after; 0 keeps them);
- then a stored file that nothing uses any more goes, a day after it was last stored.

What you attached to a card is never removed. [Your data](../install.md#your-data) lists everything GizAi clears.

## GizAi itself in a plain browser

For GizAi's developers, and the agents that work on it: `npm run browser` serves GizAi's screens in a plain browser with a fake backend. It is the Preview command of the project for GizAi's own repository, best with the sizes Window 1440 × 900 and Small 900 × 600 (the window's default and smallest size) and both themes on.

- It serves them at `http://127.0.0.1:$GIZAI_PREVIEW_PORT/` (port 1430 without that variable), so a card opens at `/#/task/<id>`. The page is `browser.html` with `src/browser/`, where Tauri's `mockIPC` stands in for the backend.
- Commands answer from `src/browser/fixtures.json`. To write it again from the demo data: `cargo run -p gizai-core --example demo -- <dir>`, then `cargo run -p gizai-core --example fixtures -- <dir> src/browser/fixtures.json`. The demo's GFW-2, in Review and in the Inbox, has six runs' screenshot sets; their pictures are drawn from each one's size and caption, as the fixtures hold no images. KADE-1, GFW-1, GFW-3 and GFW-5 show the empty tab's four reasons.
- The app's own UI test has the same six sets, with real pictures: `cargo run -p gizai-core --example prep_screenshots -- <dir>` gives them to the demo data's GFW-2, and `scripts/ui-test.sh` opens its Screenshots tab with `screenshotsProbe` (`src/selftest.ts`): the sets, the larger view, Compare, Esc, the board's sign and the Inbox row.
- A command without a fixture answers empty and logs its name in the browser's console. `gizaiBlobUrls()` in the console lists the pictures' `blob:` URLs that are alive, so you can see them freed when the pictures leave the screen.
- `t` switches the theme, as in the app (or set `gizai-theme` in localStorage).
- What differs from the app: Chrome draws it, not the app's webview, and native dialogs, file drops and the tray don't exist there. The app's own build (`npm run build`, `npm run tauri build`) contains none of it.

## Free

Screenshots and the preview are free, like the rest of the app. [docs/PAID-FEATURES.md](../PAID-FEATURES.md) lists what stays free.

## Read more

- [docs/settings.md](../settings.md#screenshots): every screenshot setting, with its default and range.
- [Tools per agent](tools.md#a-hidden-browser): the hidden browser agents take them with.
- [Tested before you see it](testing.md): QA, and the pull request.
- [docs/research/screenshots.md](../research/screenshots.md): the research behind it.
