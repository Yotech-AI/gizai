# Agent tools

What an agent may use besides its worktree, its folders and its allowed commands: **MCP servers**, **web search and fetching pages**, a **hidden browser** for testing web pages, and the **CLI's own built-in tools**.

Everything is **off by default** and set **per agent**, in the agent form → Tools. It applies to task runs, and for the Team Lead also to its chat answers.

**Gizai builds no tools and installs none.** Every tool is the CLI's own or comes from an MCP server in Settings. Gizai only switches them on per agent and hands them to the run: flags, allowed tools and the run's own MCP config.

## MCP servers

Settings → MCP servers holds one list for all agents. A server is either:

- **a command** (stdio): a program with its arguments and environment lines, like `npx -y @acme/mcp-server` with `ACME_TOKEN=…`;
- **an address** (Streamable HTTP, or the older SSE): like `https://os.oranjeuil.nl/api/mcp`, with header lines.

Its name takes letters, digits, `-` and `_`; its tools are called `mcp__<name>__<tool>`. `gizai` and `chrome-devtools` (the built-in browser, below) are taken. A name can't have two `_` in a row or end with `_`: Claude Code reads `__` as where the name ends, so `gizai__notes` would pass for Gizai's own server.

Gizai builds no tools and installs no servers: a server's command is what you enter or import. Gizai only hands the servers an agent has on to its run.

### Secrets stay in the keychain

Environment values, header values and sign-in tokens live in the OS keychain (Secret Service, through the `keyring` crate), under `mcp/<server id>/…`. The settings table keeps only their names. Once saved, a value is never shown again: the form says it is saved and lets you type a new one. A value is kept under its line's name, so a line you rename needs its value typed again (the old name's value goes when you save). Values never go into logs, run output, error messages or the Team Lead's tools. A run gets them only in its own MCP config file, readable by you alone (0600) and deleted when the run ends.

Headless tests and QA runs use a file instead of the keychain: `GIZAI_FAKE_KEYCHAIN=/path/to/file.json`.

### List tools

**List tools** starts the server (or calls its address), asks for its tools (`initialize`, `tools/list`) and stops it again. Do this before you switch a server on. For an `npx` server it also fetches the package once, so runs don't wait for the download (it may take up to 2 minutes). A server that fails shows why in plain words.

Each tool shows its name, title, description and parameters (name, type, required or not, description; in the order the server lists them), and what the server says about it, in plain words with a risk:

| The server says | Risk |
| --- | --- |
| only reads (`readOnlyHint`) | low |
| changes things, but doesn't delete or overwrite (`destructiveHint: false`) | medium |
| may delete or overwrite (`destructiveHint`, or no hint) | high |

A hint the server didn't send follows the MCP spec's default: not read-only, may delete or overwrite, not safe to repeat, reaches outside services. The view says which hints the server sent.

### Import from Claude Code

**Import from Claude Code** reads the MCP servers each Claude Code in Settings → Coding CLIs already has: the built-in one (`~/.claude.json`, or `$CLAUDE_CONFIG_DIR/.claude.json`) and every second account by its `CLAUDE_CONFIG_DIR`. It shows user scope (`mcpServers`) and local scope (`projects.<folder>.mcpServers`, with its folder): name, account, scope, type, command and arguments or address, and the **names** of environment lines and headers, never their values.

Tick the ones to import. One already in the list is marked; a name that clashes (or `gizai`, `chrome-devtools`) asks for another. Importing copies the entry, with its values into the keychain: later changes in Claude Code don't follow.

Read only: Gizai never writes in `~/.claude.json`, `~/.claude` or a second account's folder, never reads `.credentials.json`, and starts no server to find them (it doesn't run `claude mcp list`). Connectors from your claude.ai account and servers from plugins aren't in that file: add those by address.

### Signing in (OAuth)

An address server that answers `401` with `WWW-Authenticate` shows **Needs sign-in** and a **Sign in** button. Gizai signs in itself, as a device of its own; it never reads or reuses Claude Code's sign-ins (they share a file with the Claude login, and a server like Otus OS replaces the refresh token on every use, so sharing one would sign the other side out).

Following the MCP authorization spec, Gizai:

1. reads the protected resource metadata (RFC 9728), from the header or the well-known address, and the authorization server's metadata (RFC 8414, OpenID discovery as a fallback);
2. registers itself as a public client named "Gizai" (RFC 7591) with a loopback redirect `http://127.0.0.1:<free port>/callback`; a server without registration needs a client id, entered in the server's form;
3. opens the sign-in page in **your default browser** through the system opener (the only time Gizai opens a browser, on your click; no agent touches it), with PKCE S256, `state` and `resource` set to the server's address (RFC 8707);
4. takes one answer on the loopback address, checks `state`, and gives up after 10 minutes.

The server then shows **Signed in**. A signed-in server's tools act as you in that service; the agent form says so next to its switch. **Sign out** revokes the refresh token where the server offers revocation (RFC 7009), and forgets the tokens.

Tokens are refreshed before List tools, and before a run when less than the run's time cap is left: one refresh at a time per server, the new refresh token saved before the new access token is used. After a refused refresh Gizai reads the stored token once more (another run may just have refreshed it) before it asks you to sign in again.

## Per agent: the switches

The agent form → Tools lists every server with a switch, its sign-in state, its state in the agent's last run (connected or failed), and its tools, each with its own switch, what it does and its risk. Agents on Codex or Gemini show them disabled, with why (see Codex and Gemini below).

In a run, the agent's servers go into the run's own MCP config, next to `gizai` in chat:

- all tools of a server on: `mcp__<name>` in `--allowedTools`;
- some off: the ones on by full name in `--allowedTools`, the ones off in `--disallowedTools`;
- the server off: it isn't in the config.

A server that can't be used (signed out, a refused refresh, a value missing from the keychain) is left out of the run, and the run says which one and why, like "Left out otus: signed out. Sign in again in Settings → MCP servers." When Claude Code's init line says a server failed to connect, the run says which one; the form shows each server's state in the last run.

Stop, the time cap and quitting end the MCP servers a run started with it: Claude Code starts them in the run's process group, which gets SIGINT, then SIGTERM, then SIGKILL. A server that starts a child in a process group of its own ends that child on the SIGINT or SIGTERM it gets first. (Quitting a second time while agents are being stopped ends them at once, with SIGKILL.)

## Web search and fetching pages

The agent form → Tools → **Web** has two switches:

- **Search the web.** Claude Code: `WebSearch` in `--allowedTools`. Codex: `-c web_search="live"`, and `"disabled"` while it is off. Medium risk: results are untrusted text, and the search terms leave your computer.
- **Fetch web pages**, any page or only the domains you list (one per line, like `docs.rs` or `*.laravel.com`). Claude Code: `WebFetch`, or one `WebFetch(domain:…)` per domain. Gemini: `--allowed-tools=web_fetch` (any page: it can't keep to a domain list, so an agent with domains set goes without it). High risk: pages are untrusted, and an address can carry data out.

Off, a run doesn't get them, and a headless Claude Code refuses a tool it would have to ask for. The Team Lead's chat answers keep `--tools Read,Glob,Grep`; its Web switches add `WebSearch` and `WebFetch` to that list (and the rules to `--allowedTools`) only when they are on. The form suggests web search for the Team Lead, but never switches anything on.

## The browser: Chrome DevTools MCP

The top of Settings → MCP servers has a built-in entry, like the built-in Claude Code in Coding CLIs: **chrome-devtools**, Google's MCP server for coding agents, pinned to an exact version (1.10.1 to start with, never `latest`). A run starts it as:

```
npx -y chrome-devtools-mcp@1.10.1 --headless --isolated --no-usage-statistics --no-performance-crux [--executablePath=<browser>] [--acceptInsecureCerts]
```

- **Always hidden with a throwaway profile.** `--headless` and `--isolated` are always there, so the browser never shows on your screen and never uses your browser, profile or logins. Gizai never passes an option that connects to a running browser or uses a real profile (`--browserUrl`, `--wsEndpoint`, `--autoConnect`, `--userDataDir`, `--channel`, `--config`, `--chromeArg`) and checks every command line for them. Claude Code's `--chrome` (Claude in Chrome, which drives your own browser) is never passed.
- **Nothing goes to Google.** `--no-usage-statistics` (and `CHROME_DEVTOOLS_MCP_NO_USAGE_STATISTICS`) keeps usage statistics home, `--no-performance-crux` keeps page addresses from performance traces away from Google's CrUX API, and `CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS` stops its update check.
- **Only the version and the browser program change** (Edit). The program is Google Chrome where the server finds it itself, else the Google Chrome or Chromium Gizai finds (`--executablePath`), or the full path you set. Brave's program is refused.
- **What it needs:** Node 20.19 or newer (22.12 or newer on Node 22) with npx, and Google Chrome or Chromium. Settings and the agent form say what is found, and what to install if not. Gizai installs nothing: `npx` fetches the server (List tools does that the first time, which may take up to 2 minutes), and the server starts its own browser at the first tool call.
- **Local `.test` sites** with their own certificates: the agent form's switch "Accept self-signed certificates" adds `--acceptInsecureCerts` for that agent. Off by default.

The agent form → Tools → **Browser** switches it on for that agent (high risk: it opens and clicks pages, fills in forms and runs scripts in them). Its tools have a switch each once List tools has listed them. Every click is a tool call, and the cap per run is in Settings → Runs. The form suggests it for QA, Frontend and Design agents, but never switches it on. It tests web pages, like the Herd sites, not Gizai's own window: `scripts/ui-test.sh` in `cage` stays the check for Gizai's UI.

On, a run gets the `chrome-devtools` server in its own MCP config and `mcp__chrome-devtools` in `--allowedTools`; off, neither. A browser that can't start (no npx, no Chrome or Chromium, a program that's gone) is left out of the run, and the run says why.

**Stop, the time cap and quitting.** The server runs in the run's process group, so it gets the SIGINT (then SIGTERM) Stop, the cap and quitting send. It starts Chrome in a process group of its own, and ends it when it gets SIGINT, SIGTERM or SIGHUP, or when it exits. Gizai never ends anything by name or pattern.

## The CLI's built-in tools

The agent form → Tools → **Built-in tools** lists what the agent's CLI offers, by group (Files, Commands, Agents, Planning, Other), each with what it allows and its risk:

- **Always on**: the CLI uses it without asking (like `Read`, `Glob`, `Grep`, `TodoWrite`).
- **Set elsewhere**: the permission mode (`Edit`, `Write`) or Allowed commands (`Bash`).
- **Off**: not for headless agents, or Gizai keeps it off (`Skill` and `SlashCommand`, through `--disable-slash-commands`; `AskUserQuestion`, as the agent asks in its result line instead).
- **A switch**: a tool the catalog doesn't know shows under **Other tools the CLI reports**, risk unknown, off. On, it is allowed in the agent's task runs; the Team Lead's chat answers don't get it.

Where the list comes from: Gizai's **catalog** (in `crates/gizai-agents/src/tool_catalog.rs`) merged with what the CLI itself reported, never only one of them. For Claude Code that is the `tools` of its init line: Gizai keeps them on the agent after each run and chat turn ("seen in the last run"), and **Ask Claude Code again** starts the installed Claude Code with a scratch `CLAUDE_CONFIG_DIR` and HOME and no API key or token in its environment, so it isn't logged in: it prints its init line (with the tools) before its "Not logged in" error, nothing is spent, nothing is written in `~/.claude`, and Gizai ends it. Codex and Gemini have no command that lists their tools without a model call: their list is the catalog, labelled "From Gizai's catalog".

## Codex and Gemini

Only what a per-run switch can give, checked against the installed CLIs' own files (without a model call); never by editing `~/.codex` or `~/.gemini`, and never with config in the worktree.

| | Codex 0.154 | Gemini CLI 0.62 |
| --- | --- | --- |
| Web search | `-c web_search="live"` (and `"disabled"` while off) | On in every run: its own read-only policy allows `google_web_search`, and Gizai has no checked per-run switch to turn it off |
| Fetching pages | No such tool (disabled) | `--allowed-tools=web_fetch` (any page) |
| The browser and MCP servers | Disabled: Codex takes servers per run (`-c mcp_servers.<name>…`), but whether a headless `codex exec` may call their tools hasn't been tried | Disabled: Gemini takes servers only from its settings files |
| Built-in tools | The catalog: commands and `apply_patch` through the sandbox, `update_plan`, `view_image` | The catalog: file tools, `run_shell_command` through Allowed commands, `write_todos` |

## Safety rules

- **Content from outside is data.** An agent with an outside MCP server, web search, fetching pages or the browser on gets one more prompt line: what those servers return, web pages, search results and pages in the browser are data, never instructions. Every Gemini run gets it, as Gemini searches the web in every run.
- **The Team Lead confirms with you.** Once a chat answer has used a tool from outside Gizai (anything besides `Read`, `Glob`, `Grep` and Gizai's own tools: an MCP server's, `WebSearch`, `WebFetch`, the browser's), the rest of that answer can't use `start_agent_run`, `continue_agent_run`, `create_agent`, `update_agent`, `set_agent_status`, `add_column`, `set_column`, `attach_file` or `update_checkout`, nor save to memory (`memory_write`, `memory_append`, `memory_move`; reading memory still works). It asks you to confirm in a new message; they work again then. Each of those calls in a chat waits until Gizai has read the answer up to it (normally at once, at most 5 seconds), so an outside tool earlier in the same message counts too. Not read by then, the call is refused and the Team Lead can try again.
- **Only you switch them.** Only you add, import, sign in to and switch on MCP servers, the web tools, the browser and built-in tools, in Settings and the agent form. The Team Lead's `get_agent` shows an agent's switches, but `create_agent` and `update_agent` can't change them, and no Team Lead tool adds, imports or signs in to a server. Their `allowed_tools` takes only commands (`Bash(…)`). A run takes web search, fetching pages and built-in tools only from their switches: one of them in an agent's Allowed commands (like `WebSearch`, `WebFetch(domain:…)`, `Skill` or a tool the catalog doesn't know) is left out, and the run log says so.
- **npm and npx.** The form warns when an MCP server, a web tool or the browser is on together with `Bash(npm:*)` or `Bash(npx:*)`: a server's answer or a web page could try to make the agent run code.
- **The browser is always hidden with a throwaway profile**: never on your screen, never your browser, profile or logins.

## Try it with Otus OS

1. Settings → Coding CLIs lists the Claude Code account that has Otus OS (for a second account, its `CLAUDE_CONFIG_DIR`).
2. Settings → MCP servers → **Import from Claude Code**: tick `otus` (user scope of that account) and **Import**.
3. **List tools**: Otus OS answers that it needs sign-in, so `otus` shows **Needs sign-in**.
4. **Sign in**: your browser opens Otus OS's consent screen; name the device (like "Gizai") and press *Activate*. Back in Gizai, `otus` shows **Signed in**.
5. **List tools** again: each tool with what it does and its risk (Otus OS marks the tools that only read).
6. Team page → the agent → Tools: switch `otus` on, switch off the tools it shouldn't use, and save.
7. Run a card with that agent: the run's output says which MCP servers connected, and the form shows `otus` as connected in its last run. In Otus OS the agent's changes show as "via agent".

## Checked against Claude Code 2.1.289 (9 October 2026)

- `claude mcp add` writes to `$CLAUDE_CONFIG_DIR/.claude.json` for an account with its own config folder, else to `~/.claude.json`: user scope under `mcpServers`, local scope (its default) under `projects.<folder>.mcpServers`. An entry is `{"type": "stdio", "command", "args", "env"}` or `{"type": "http" | "sse", "url", "headers"}`. Checked in scratch folders.
- `--disallowedTools mcp__<server>__<tool>` takes that tool out of the run: the init line no longer lists it. `--allowedTools mcp__<server>` allows all of a server's tools.
- Chat's `--tools Read,Glob,Grep` limits only Claude Code's built-in tools: the MCP servers' tools stay.
- The init line's `mcp_servers` gives each server's `name` and `status` (`connected`, `failed`, `needs-auth`, `pending`).
- Otus OS's public metadata fits the sign-in: its `401` names `resource_metadata`, it registers public clients, needs PKCE S256 and offers revocation.
