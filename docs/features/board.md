# The board decides who works

Each column on your board holds the agents that work its cards, so a card moves from builder to tester to you by itself.

## Clients, projects and cards

- **Clients** with contacts, and **projects** with docs and files.
- **Cards** with labels, priorities and acceptance criteria, on a board or in a list (Ctrl+B switches). What needs you collects in [the Inbox](inbox.md).
- **Labels** are tags for people, such as Must have and Could have: create, rename, recolour and remove them on the Team page.
- **Archive** a card in Done: it leaves the board and every list, and keeps its ID, comments, runs and branch. The bin on the Tasks page lists archived cards, and Restore puts one back in Done.

## The columns

A card moves through Backlog → To do → In progress → Testing → Review → Deploy → Done. You set this flow up on the Team page (Team → Workflow). Each column:

- holds the agents that work its cards;
- is **Auto**, where its agents pick up its cards by priority as they have room, or **Manual**, where only Run starts one;
- links to the column its cards go to next.

In a new team, To do, In progress and Testing are Auto. Add your own columns, like a Design column with a design agent, linked to Review. Drag columns into any order, or remove one.

## The column decides, labels don't

- Starting a card in To do moves it to To do's next column.
- An agent's done answer moves the card on: In progress → Testing → your Review.
- A card assigned to an agent is started only by that agent.

Testing has rules of its own: a failed test sends a card back, and a card can skip Testing. See [Tested before you see it](testing.md#how-a-card-is-tested).

## The team

- Agents for each role (frontend, backend, design, QA, DevOps), each with its own model, effort and [allowed commands](tools.md#allowed-commands).
- A new install starts with five: the Team Lead, and the Backend, Frontend, QA and DevOps Agents. The Backend and Frontend Agents work the cards in To do, and the QA Agent tests them.
- An org chart of your team: the Team Lead on top, then branches (Design, Development, Quality, Operations and your own), each with one empty spot that adds an agent there.
- Drag an agent onto a column in Team → Workflow to put it to work there.

## When agents start

Agents start when you press Run, or when a card lands in an Auto column they are on (or is assigned to them there). An agent works on several cards at once when its Cards at once allows it.

A start that can't work (Claude Code missing or not logged in, a wrong model, no repository) holds the card "blocked" without counting as a failed run. That agent takes no new cards until you start or edit it, or GizAi restarts.

Settings → Runs sets how many runs go at once, for all agents together. Pause all agents stops the starts from Auto columns; Run still works by hand.

Memory for agent runs, also in Settings → Runs, is how much memory all agent runs may use together: half of the computer's memory until you set it. On Linux the system holds the runs to it, and Gizai stays outside the limit, so a big build can't take Gizai down ([PLATFORMS.md](../PLATFORMS.md#memory-for-agent-runs)). On every system no run starts while the runs use over 80% of it or the computer has under 2 GB free: the card waits with the reason, like Runs at once, and starts by itself once memory frees up.

## A worktree for every card

Every card gets its own git worktree and branch, started from main on GitHub or Bitbucket when the project is linked. Main is fetched through your checkout's own remote for that repository (origin first), so your own git settings apply as they do for your own `git fetch`, like an `insteadOf` rule that sends it to an SSH host alias. Worktrees start warm. Per project, a new worktree gets:

- paths copied from your checkout (`cp --reflink=auto`);
- missing dependencies installed (`composer install`, `npm ci`);
- a setup command.

A new card takes over a finished card's worktree. A build folder over 20 GB is emptied first, and the card says so.

Settings → Data lists old worktrees with their uncommitted files, and removes them. Remove anyway puts uncommitted changes in git's stash first.

## Runs

- **Live output** while an agent works.
- **Run history**, with the reason each run ended, what started it and the commits it made.
- **Limits:** time and tool-call limits per run, a spending limit per run, and a monthly budget per agent.

### Continue with a note

Continue picks an agent's run up again, in the same session. The box above Continue takes a note for the agent, like "use the existing CSV writer". The continued run hears it next to why its last run stopped, and it is saved on the card as your comment.

The Team Lead's `continue_agent_run` takes a `note` the same way. On a run that asked for a decision, the Team Lead's note counts as an answer.

### One nudge

A run that ends normally without its `GIZAI_RESULT` line is continued once by itself, in the same session, like Continue. GizAi tells the agent that nothing wakes it up later, to check in the foreground now if it was waiting, and to end with its result line. The Runs list shows it as Nudge (trigger `result_nudge`), apart from a Continue.

- It never happens after Stop, a time or tool-call limit, a failed run or GizAi quitting.
- It happens only when a start is allowed now: agents not paused, the agent active, within its budget and cards at once, and room in Runs at once. Otherwise nothing changes.
- If the nudged run also ends without a result, the card goes on hold "stalled" and shows in [the Inbox](inbox.md).

How an agent waits, during its run, for something outside it, like a CI run: [Waiting in a run](tools.md#waiting-in-a-run).

## Deploy

Deploy is Manual by default, so no agent starts there by itself.

- Press Run for the agent on the column: the DevOps Agent releases what you merged, and its `deployed` answer moves the card to Deploy's next column, Done.
- Or deploy it yourself and drag the card to Done.

## Read more

- [Tested before you see it](testing.md): Testing, pushes and pull requests.
- [The Inbox](inbox.md): cards on hold and everything else that needs you.
- [Tools per agent](tools.md): what an agent may do in its run.
- [docs/PLATFORMS.md](../PLATFORMS.md): how a worktree's copies differ on Linux, macOS and Windows.
