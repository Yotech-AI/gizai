# The Inbox

Everything that needs you waits in one list: the Team Lead's questions, calls that wait for your OK, commands to run, held cards and cards to review.

## What waits there

Sidebar → Inbox, with a count of what waits. From the top:

1. **The Team Lead's chats that wait for you:** its questions, labelled Question or Approval, and chats with calls that wait for your OK ([Approve with one click](team-lead.md#approve-with-one-click)).
2. **Run this for me:** cards whose agent asks you to run commands for it, with the commands.
3. **Cards that need you:** cards on hold, and cards waiting for your review or deploy.

With nothing there, it says "Nothing needs you."

## Why a card is on hold

- **Blocked:** a start that couldn't work ([When agents start](board.md#when-agents-start)), or a push that failed ([GizAi pushes after every run](testing.md#gizai-pushes-after-every-run)).
- **Stalled:** a run that ended twice without its result line ([One nudge](board.md#one-nudge)).
- **Needs a decision:** an agent asked a question, or QA sent the card back three times. The Team Lead looks at an agent's question first ([Agents ask the Team Lead first](team-lead.md#agents-ask-the-team-lead-first)), so the card reaches you only when the Team Lead escalates it.
- **Waiting for your OK:** the Team Lead put the card in an Auto column, or changed it there, after it used a tool from outside GizAi ([docs/agent-tools.md](../agent-tools.md#safety-rules)).

Dragging a held card back to To do or In progress takes the hold off.

## Run this for me

An agent that needs a command it may not run (sudo, an install, a command its list refuses) asks you for it. It ends with `needs_decision` and names the commands in `run_for_me` on its result line:

```
GIZAI_RESULT: {"outcome":"needs_decision","summary":"…","issues":[],"run_for_me":["sudo pacman -S libayatana-appindicator"]}
```

Older result lines still work. "How this run works" and the role instructions tell agents how to ask.

1. The card goes on hold and waits at the top of your Inbox, under the Team Lead's chats, and on its Run panel: each command exactly as written, with a copy button.
2. Run the commands in a terminal and press **Done, continue**.
3. The run continues with your note that the commands were run, also saved on the card, so the agent checks that they worked and carries on.

The commands still show when GizAi's push after that run failed.

## Notifications

GizAi sends a desktop notification when:

- a card goes on hold;
- a card waits for your review or deploy;
- the Team Lead asks you something (a Question or Approval chat), or waits for your OK on a call it couldn't make after a tool from outside GizAi;
- the Team Lead answered while the GizAi window was hidden or in the background.

Each kind has a switch in Settings → Notifications, all on by default. Clicking a notification while it shows opens the card or the chat, where the desktop's notifications support it (Omarchy's do). Otherwise Open GizAi in the tray brings the window back ([The window and the tray](../settings.md#the-window-and-the-tray)).

## Read more

- [A Team Lead you talk to](team-lead.md): questions, approvals and the board check.
- [The board decides who works](board.md): how cards move, and when agents start.
- [docs/memory.md](../memory.md#agents-ask-the-team-lead-first-ga-70): how the Team Lead answers agents' questions, in full.
- [docs/agent-tools.md](../agent-tools.md#safety-rules): when the Team Lead's calls wait for your OK.
