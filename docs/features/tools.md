# Tools per agent

Each agent gets the commands its role allows, the folders you give it, and the tools you switch on for it.

## Off until you switch it on

The agent form → Tools has MCP servers, web search, fetching pages, a hidden browser for testing web pages, and the CLI's own built-in tools. Everything is off until you switch it on per agent, with a switch per server and per tool.

GizAi builds no tools of its own: each one is the CLI's own or comes from an MCP server. A web or built-in tool named in an agent's allowed commands is left out of its runs.

## MCP servers

Agents can use outside services through MCP servers, like Otus OS. Settings → MCP servers holds one list for all agents.

- Add a server by hand (a command, or an address), or **Import from Claude Code**, which reads the servers your Claude Code accounts already have: read only, with the values going into the keychain.
- **List tools** shows what each tool does, its parameters and its risk, before you switch anything on.
- A server that asks for it gets **Sign in**: GizAi signs in as a device of its own, in your default browser.
- Environment values, headers and sign-in tokens stay in the OS keychain, never in GizAi's database.

## The web

**Web** has two switches: search the web, and fetch pages (any, or only the domains you list).

- Claude Code gets `WebSearch` and `WebFetch`.
- Codex gets its own web search.
- Gemini fetches pages; its web search is on by its own policy.

## A hidden browser

**Browser** is a hidden Chrome for testing web pages, through Chrome DevTools MCP, the built-in entry at the top of Settings → MCP servers.

- It always runs headless with a throwaway profile: never on your screen, never your own browser, profile or logins.
- It needs Node 20.19 or newer with npx, and Google Chrome or Chromium (never Brave). The form says what is missing.
- GizAi installs none of it: `npx` fetches the pinned server, and the server starts the browser.

MCP servers and the browser work on Claude Code. Codex and Gemini agents show them disabled, with why.

## Built-in tools

**Built-in tools** lists what the agent's CLI offers: GizAi's catalog merged with what the CLI itself reports. For Claude Code that is its list from the agent's last run, or **Ask Claude Code again**, which starts it without a login. Each tool says what it allows and how risky it is; a few, like Read and Grep, are always on.

**Slash commands and skills**, in Built-in tools, lets a Claude Code agent's task runs use your slash commands and skills, like `/design`, plugins' too.

- Medium risk: they can steer the run, so switch it on only for an agent that needs one.
- On, its runs go without `--disable-slash-commands` and get `SlashCommand` and `Skill`. Hooks stay off, and settings still come only from your user settings.
- The Team Lead's chat never gets them, and Codex and Gemini agents show the switch disabled, with why.

## Allowed commands

A new agent starts with its role's allowed commands.

- GizAi's default list has the usual git, package manager and test commands, the read-only helpers agents use in pipes (`head`, `tail`, `wc`, `sort`, `uniq`, `cut`, `diff`, `grep`, `jq`, `pwd`, `which` and `tree`), and `sleep`, to wait between checks. The Team Lead gets that list.
- Builders (Backend, Frontend, Design and your own roles) also get `git push`, `git pull`, `git fetch`, a few read-only git commands, `node`, `echo` and `printf`.
- QA gets the builders' list plus `gh pr create`, `list`, `view` and `edit`.
- DevOps gets a list of its own for releases.

An agent without a list runs with GizAi's default list. An agent with its own list needs `Bash(sleep:*)` added to it to wait between checks.

## Folders

Besides its worktree, an agent can have folders of its own, each set to read or to read and change (agent form → Permissions). They limit the agent's file tools, not the commands it runs. `/`, your home folder, GizAi's data and folders with keys are refused.

## The rules of a run

Agent runs are headless: nobody is there to approve anything while one runs, so a command or tool call that needs approval is refused.

### A temp folder per card

Each card's worktree has a `.gizai-tmp/` folder. Before every run GizAi makes it, empty, and sets `TMPDIR`, `TMP` and `TEMP` to it for every CLI. So `mktemp`, PHP's `sys_get_temp_dir()`, Node's `os.tmpdir()` and Python's `tempfile` write inside the worktree, where the agent may write.

- GizAi adds `/.gizai-tmp/` once to the repository's `.git/info/exclude`, which your checkout and every worktree share. So the folder never shows in `git status`, and `.gitignore` is left alone.
- The folder is emptied when the run ends, also after Stop or a limit, and goes with the worktree.

### How this run works

Every task prompt, new or continued, ends with "How this run works": nobody can approve anything, the commands the agent may run (its allowed list), the shell rules of its CLI, the temp folder's path, and that GizAi pushes the branch when the run ends.

For Claude Code in acceptEdits mode (the default), checked against Claude Code 2.1.289:

- commands and file tools reach only the worktree and the agent's folders: `ls /tmp`, `ls ..`, a redirect to `/tmp` and the Write tool on `/tmp` are refused, also for an allowed command;
- `$(…)`, backticks, variables such as `$TMPDIR` and heredocs with an unquoted delimiter (`<<EOF`) are refused;
- pipes, `2>&1`, `&&` and `;` between allowed commands work, and so does a redirect to a file in the worktree.

Codex and Gemini hear only what holds for them. Codex asks for nothing and its sandbox blocks what it doesn't allow; Gemini hears its allowed commands.

### Waiting in a run

Ending its message ends an agent's run, and nothing wakes it up later. So "How this run works" also says how to wait for something outside the run, like a CI run, a release or deploy workflow or a pull request's checks.

- Check it in the foreground about once a minute: one check, then `sleep 45`, in one command, and again.
- When it won't be done before the run's limit, end with the result line and say what is left.
- `sleep` is named only for an agent that may run it.

Checked against Claude Code 2.1.289: a command that starts with a sleep of more than 20 seconds is blocked, and most shell loops are refused. A command that runs more than 2 minutes (or its own timeout, at most 10) is moved to the background, and a background command is stopped when the message ends.

### Refused in this run

Claude Code reports every tool call it refused, with the reason. GizAi saves them on the run:

- the Run panel lists them under "Refused in this run";
- Show output marks each one where it happened;
- the Team Lead's `get_task` and `get_agent` return them for each run.

Codex and Gemini don't report refusals, so their runs list none.

## Safety

After a chat answer used an outside tool (an MCP server, the web, the browser), the Team Lead's calls that start runs, change agents or columns, save to memory or merge wait under that answer for your Approve, each with a preview. See [Approve with one click](team-lead.md#approve-with-one-click).

## Read more

- [docs/agent-tools.md](../agent-tools.md): MCP servers, signing in, the web, the browser, built-in tools and every safety rule, in full.
- [Bring your own CLI](cli.md): what each coding CLI gets.
- [docs/PLATFORMS.md](../PLATFORMS.md): how the browser and the agents' folders differ on Linux, macOS and Windows.
