# Open items (7 October 2026)

There are two parts:

- **Cards** for the Gizai project. Paste them into Gizai's Chat ("Create these cards in project Gizai: …"), or make them by hand.
- **Decisions and checks** that only Jeffrey can do.

They are ordered by how much they help dogfooding: the first ones make agents on this repository work better.

## Cards

### Runs and agents (label backend)

1. **A run that fails before it starts working doesn't count as a failure**
   - A run that dies before its first tool call (unknown model, not logged in, Claude Code missing) puts the card on hold as "blocked" with Claude Code's error. It doesn't add to the failure count.
   - Starting a run checks the agent's model against Claude Code's model list (`runs::models`).
   - *Done when:* a wrong model name holds the card at once with the error, and `fail_count` stays 0.
2. **One automatic nudge before a hold**
   - When a run that did some work ends without a `GIZAI_RESULT` line, Gizai continues it once by itself, asking only for the result line. Then the usual hold applies.
   - Use the `runs.nudged` column, and the same session as Continue.
   - *Done when:* a run without a result gets one Continue with trigger `nudge`, and a second missing result holds the card.
3. **Record where a run ended**
   - Save `runs.head_sha` when a run finishes.
   - The Runs tab shows the commits the run made (count and subjects).
   - *Done when:* a run that commits twice shows "2 commits" with their subjects.
4. **Stop agents cleanly when the system closes Gizai**
   - On SIGTERM or logout, stop the live runs and chat answers the way quitting does: mark them interrupted and end their process groups. Today they run on until the next start.
   - *Done when:* a test sends SIGTERM to a Gizai with a live fake run, and the run ends with "Stopped because Gizai quit".
5. **A Merge button on cards in Review**
   - A Review card shows its branch's commits and diff stat.
   - "Merge into main" merges locally; it refuses on conflicts and says which files. Afterwards the card moves to Done and its worktree is removed.
   - *Done when:* in a test repository, Merge lands the branch on main, removes the worktree and moves the card to Done.
6. **Clean up old worktrees**
   - Settings → Data lists worktrees of Done and Cancelled cards with their disk use, and removes them (worktree and branch) after asking.
   - *Done when:* removing frees the folder, and git no longer lists the worktree.

### Team Lead, towards replacing Chief (label backend)

7. **Team Lead memory**
   - A "Team Lead notes" doc that every chat answer reads, and that the Team Lead can update with a tool (like Chief's notes).
   - *Done when:* a fact saved in one thread is known in a new thread.
8. **Continue with a message**
   - Continue (from the run card or the Team Lead) can carry a note from you: "use the existing CSV writer", for example.
   - *Done when:* the continued run's prompt contains the note.
9. **"Run this for me" cards**
   - An agent can ask you to run a command it may not run (sudo, installs). Its result line can say so, and the card goes to your Inbox with the command and a "Done, continue" button.
   - *Done when:* such a result holds the card with the command shown, and the button continues the run.
10. **Desktop notifications**
    - A desktop notification when a card needs you: a hold, Review, or a question. Each kind can be switched off in Settings.
11. **Import projects from a folder**
    - Choose a folder such as `~/Herd`, and Gizai lists its git repositories and creates projects for the ones you tick (with the GitHub link filled in).

### Deferred minors from the v0.3 review (label backend)

12. **Chat edge cases:**
    - failed resumed turns can count their cost twice;
    - Stop in a narrow window can mark a finished answer as cancelled;
    - a Gizai crash leaves no "interrupted" note in the chat;
    - the live text snapshot can drop a few words.
13. **Team Lead tool edge cases:**
    - `update_task` applies fields before it checks labels, so a bad label half-succeeds;
    - tool results cut at 4,000 characters become invalid JSON;
    - the resolver prefers an exact key over another project's exact name;
    - `column()` falls back to a category silently.
14. **Housekeeping:**
    - prune old `api_tokens`, run logs and chat logs;
    - backup file names use UTC instead of local time;
    - the project key suggestion can give 7 letters after nine collisions.

### Screens (label frontend)

15. **Remove a contact:** the contact drawer gets "Remove contact", after a confirmation.
16. **Warn about a too-low total:** in Settings, warn when "Runs at once" is lower than the agents' cards at once added up.
17. **README update:**
    - take new screenshots (the sidebar has no rail);
    - describe cards at once, Continue, the GitHub link and run limits.
18. **Design system generator update** (`design/gen-design-system.mjs`), for:
    - the rail removal and Company at the bottom of the sidebar;
    - the opaque Working badge;
    - Runs tab rows;
    - the Continue button;
    - contact editing;
    - the GitHub field.

    Publishing the design system page to claude.ai is a step for a Claude session, not an agent.

## Decisions and checks for Jeffrey

1. **Merge:** done on 7 October: `main` is `v0.3-chat`, and it's on GitHub.

2. **GitHub:** answered: `Yotech-AI/gizai` (public). Link the Gizai project to it, so cards start from GitHub's main.
3. **Paid plan:** P1–P9 in `docs/PAID-FEATURES.md`.
4. **From the morning list, still open:**
   - no delete tools in chat;
   - chat cost counting towards the Team Lead's budget;
   - hooks and skills off for task agents;
   - the org chart from roles.

   One chat agent and "the Team Lead doesn't code" are answered: keep both.
5. **A real two-turn chat check** (it writes a session in `~/.claude`):

   ```sh
   cargo run -p gizai --example chat_probe -- /tmp/gz haiku "hi" "and again"
   ```

6. **The first real Continue:** Continue is tested only with the fake Claude Code. A stalled real card is a good first one; tell me or a card if it fails.
