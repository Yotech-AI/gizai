# Tested before you see it

A QA agent tests a card in its own worktree before it reaches your Review, and opens its pull request on GitHub.

## How a card is tested

1. A builder's done answer moves the card from In progress to Testing, where the QA Agent picks it up.
2. QA runs the project's tests, lint and typecheck, writes tests for what isn't covered yet, and checks each acceptance criterion.
3. Passed: QA pushes its tests and opens the pull request on GitHub, and the card moves to your Review.
4. Failed: the card goes back to the column it came from, to its builder, with a numbered list of what is wrong.

What QA checks and writes comes from the QA Agent's starting instructions, which you can change in its form. After three rounds back, GizAi holds the card for you.

- A card with Testing off skips Testing columns: a done answer sends it on to the column after them, Review in the usual flow.
- A DevOps run never sends a card to QA.
- A card you build yourself is tested the same way after you hand it over, and a failed test sends it back to you ([Work on a card yourself](editor-and-terminal.md#work-on-a-card-yourself)).

## Test runs at once

A test run builds and runs the project's whole test suite in the card's own worktree. On a big project (Rust, a large monorepo) a few at the same time can fill the disk and slow your computer down. So a project can keep them to a number: project form → New worktrees → **Test runs at once**, 1 to 20. Empty, the default, is no limit of its own.

- A test run is a run on a card in a Testing column. Runs on cards in other columns never count and never wait.
- While the project has that many test runs live or starting, the queue passes over its other cards in Testing and the agent takes its next card, so other projects' cards aren't held up. A card that waits starts by itself once one of those runs has ended.
- Run, Continue and the Team Lead's `start_agent_run` and `continue_agent_run` wait too, and say why: "Giz AI tests 1 card at a time (project settings): wait for GA-211 to finish testing, or raise the limit on the project page."
- Runs at once (Settings → Runs), each agent's cards at once and the wait for memory still hold: this is one more limit.
- A card that has waited over an hour only for it is a finding of the Team Lead's board check (`test_runs_full`).
- The Team Lead's `get_project` and `list_projects` show it, and `update_project` sets it (`test_runs_at_once`; 0 clears it).

## GizAi pushes after every run

When a run on a card ends (also after Stop or a limit) and its project is linked to GitHub, Bitbucket or a git URL, GizAi pushes the card's branch itself when it has commits the remote doesn't have. It pushes before the card moves on, so the next agent (QA, for its pull request) finds the branch.

- It pushes to the same remote and over the same Push over setting as Open pull request, never by force.
- Only commits go: uncommitted changes stay in the worktree, and the run's output says how many there are.
- The run's output and the Run panel say "GizAi pushed <branch> (3 commits)", or why it couldn't.
- A push that fails (the branch on GitHub has commits this one doesn't, no access, no network) is not forced or tried again. The card stays where it is, on hold "blocked" with the reason, and shows in [the Inbox](inbox.md).

So a `git push` the agent's CLI refuses no longer holds a card up, and "How this run works" tells agents so. They keep `git push` in their list.

## An agent's own push

A run of a project linked to GitHub or Bitbucket gets git settings in its environment only. So a plain `git push origin HEAD` from the worktree reaches the project's repository the way GizAi's push does, and QA can push its branch and open its pull request during its run.

- The settings are `GIT_CONFIG_COUNT` with its keys and values; Codex also gets them in its shell environment policy.
- The push goes over SSH with your keys or over HTTPS with gh's login (Settings → GitHub → Push over), and to Bitbucket over SSH.
- They touch only the project's own repository. Each address it goes by (its link, and each remote that is the same repository) gets a `pushInsteadOf` rule to its SSH or HTTPS address, and over HTTPS gh's login is the only credential helper for github.com.
- Fetches, other repositories and `git remote get-url` without `--push` stay as they are, and no git config file or remote changes.
- An address that a rewrite of your own already sends somewhere (an `insteadOf` for a mirror, or a test's local repository) is left to yours, as it is for GizAi's push.
- With Push over HTTPS and gh missing, a run starts without the settings and its log says why. GizAi's push after the run needs gh too.
- New and continued runs get them alike. A project with another git URL gets none, as its address says where a push goes.

## Review on GitHub

- **Open pull request** pushes a card's branch over SSH with your keys (or HTTPS with gh's login) and opens its pull request with gh.
- **A merge on GitHub** moves the card to Review's next column (Deploy; Done for a team without a Deploy column) and removes its worktree. What the card changed stays readable in [its Changes](editor-and-terminal.md#a-cards-changes).
- **Settings → GitHub** shows whether gh is found and logged in and how pushes go, with Check connection for every linked project and Log in with GitHub. GizAi never stores a token or password.

## Review on Bitbucket Cloud

Review on Bitbucket Cloud works the same way. The push goes over SSH with your keys, and the pull request through Bitbucket's API with your email and API token. Declined and superseded pull requests show as closed.

What you need for it: [docs/install.md](../install.md#what-every-system-needs).

## The Team Lead may merge

In a project you switch on, the Team Lead may merge a card's pull request once QA passed it and CI is green, in chat and in its board checks.

- The switch: project form → Repository → **Team Lead may merge**, off by default. Only you set it.
- It merges only when QA passed exactly the pull request's latest commit, the pull request can be merged, every check passed and no release is under way. GitHub only.
- It makes a merge commit and deletes no branch: never `--admin`, `--auto`, squash or rebase.
- Releases, tags and deploys stay yours.

Every rule: [docs/agent-tools.md](../agent-tools.md#the-team-lead-merges-pull-requests).

## Read more

- [The board decides who works](board.md): how cards move between columns.
- [The Inbox](inbox.md): held cards and cards waiting for your review.
- [docs/agent-tools.md](../agent-tools.md): the merge rules in full, and the hidden browser QA can test web pages with.
