# A Team Lead you talk to

The Team Lead is the agent you chat with: it looks after your board for you, and answers the other agents' questions before they reach you.

## What it can do

The Team Lead manages clients, projects, cards and agents with GizAi's own tools. In a chat it can:

- make and change clients, projects, cards and docs, and comment on and move cards;
- start and continue agents' runs, and add or change agents and columns;
- attach a file you gave it to a card, project or client;
- read and keep notes in [memory](memory.md);
- update a project's linked folder, after your yes;
- merge a pull request that QA passed, in a project where you allow it ([The Team Lead may merge](testing.md#the-team-lead-may-merge)).

Its chat runs on Claude Code ([Bring your own CLI](cli.md)).

## The chat

- **Runs on**, under the text box, moves one chat to another Claude Code account (Settings → Coding CLIs), for example when one is near its weekly limit. The new account gets the conversation handed over, and the chat notes the switch. An answer that hits a usage limit offers Answer on another account.
- **A queue:** messages you send while the Team Lead answers wait in a queue and go together when the answer is done. After a stopped or failed answer they wait for Send now.
- **Recent and Archive:** Recent shows the 30 chats with the newest activity. Archive, under the list, shows every chat, newest first, and searches their titles and the messages you and the Team Lead wrote. No chat is hidden or deleted: a new message in an old chat moves it back to the top of Recent.
- **Files:** + next to the text box adds files to a message (Add files, or drop them on the Chat page) or links an item. A message's files are kept with it; the Team Lead reads a copy of each in its own folder and can attach one to a task, project or client.

## Link anything with @

@ in the chat and in every Markdown text box links a task, project, client, agent, person or doc.

- Only @ lists the kinds, `@task.` (and the others) one kind, and typing searches by ID, key or name.
- `@task.` shows the tasks under a heading per column, in the board's order.
- The link shows as a chip with the item's name, which opens the item (Ctrl+click while you edit).

## Approve with one click

After an answer used a tool from outside GizAi (an MCP server, the web, the browser), its calls that start or continue runs, change agents or columns, attach a file, update a linked folder, save to memory or merge a pull request aren't run. They wait under that answer instead:

- each with a line about what it does and a preview: a note's new text and what changes, the card and agent, the settings that change, the file and where it goes;
- each with **Approve** and **Discard**, and **Approve all** when there are several.

Approve runs exactly that call once, with every other rule in force; Discard runs nothing. The chat notes what happened and the Team Lead hears it in its next answer. Until you decide, the chat waits for you at the top of [the Inbox](inbox.md), also after a restart.

The rules behind it: [docs/agent-tools.md](../agent-tools.md#safety-rules).

## The board check

Agent settings → Chat → Check the board every 15 min, off until you turn it on. It looks for answered questions, held and stuck cards, and cards no agent will start.

Only something new starts it. It gets agents going again (Continue or Run), and asks you what it can't decide in a chat of its own, labelled Question or Approval, at the top of your Inbox.

## Agents ask the Team Lead first

An agent that ends its run asking for a decision (`needs_decision`) asks the Team Lead before you. Settings → Runs → Ask the Team Lead first switches this; it is on by default.

- The card stays on hold but out of your Inbox ("With the Team Lead").
- A short run of the Team Lead reads the question, and looks in memory, the card, the docs and its copies of the code.
- It answers: its comment on the card. The agent's session continues with it as the Continue note, and the answer is kept in memory, linked to the card.
- Or it asks you: a comment "Needs <you>: why, the options, my advice", and the card lands in the Inbox with "Team Lead escalated to you: why".

Money, scope, deadlines, client messages, security, deleting and anything it can't find always come to you. So does any error, timeout or run without an answer. At most one try per question and two per card, and the question right after a Team Lead answer comes to you.

Run this for me requests and failed pushes skip the Team Lead. When you answer a question it escalated, the Team Lead keeps your answer in memory as the agent starts again. The Runs tab shows who answered and what the Team Lead's look cost (on its budget).

More: [docs/memory.md](../memory.md#agents-ask-the-team-lead-first-ga-70).

## It reads your code

The Team Lead reads its own read-only copy of each active project's code: `<data dir>/code/<KEY>`, a worktree without a branch. The copy is kept at the commit a new card starts from and refreshed before each answer. Your own checkouts are never passed to it.

## It updates your linked folders, after your yes

When a project's linked folder has dependencies behind main (`vendor/` or `node_modules/` missing, or a lock file that differs from main's), the Team Lead tells you what an update would do. After your yes it updates the folder:

- a fast-forward to main, switching branch only if you agreed;
- then `composer install` or `npm ci`;
- never a merge, a reset or the setup command.

## Limits per answer

Each answer of the Team Lead has a time and a tool-call limit, also its board check and its look at an agent's question. Set them in Settings → Runs: Minutes and Tool calls per chat answer, 15 and 60 unless you change them (5 to 240 minutes, 20 to 500 tool calls).

A change counts from the next answer, and an answer that stops at a limit says which one.

## Read more

- [The Inbox](inbox.md): the Team Lead's questions and the calls that wait for your OK.
- [Memory](memory.md): the notes the Team Lead keeps.
- [docs/agent-tools.md](../agent-tools.md): the Team Lead's safety rules and how it merges pull requests.
- [docs/memory.md](../memory.md): the Team Lead's memory tools.
