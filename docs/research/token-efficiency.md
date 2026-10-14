# GizAi: fewer tokens without losing quality, and what caveman.so does (research, 2026-10-14)

Status: research only, no code. Written by the R&D Agent for GA-171 on 2026-10-14, reading `main` at `24535e3` (GizAi 0.11.0).
Legend: **[V]** = verified today, in the code (file and line) or in the primary source named (link and date in §10). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits of this research: the run may not read GizAi's database or run logs (they are real user data), nor start `cargo`, `npm` or any CLI, so no run was measured. The token figures for GizAi's prompt are counted from the code; the dollar figures per agent are the ones Jefsev gave on the card (Claude Code's estimate at API prices, October 1 to 14). Everything that needs a real run is an experiment in §5.

---

## 1. Summary

1. **Where the tokens go.** This team spent $2,771 at API prices in 14 days (§2.7): builders and QA 72%, the Team Lead 18%. In a run nearly all input is the growing conversation read again from the cache on every turn; GizAi's own prompt is only 4,000–15,000 tokens of it. Most agents run Opus 5.5 at `max` or `xhigh`, while Claude Code's own default for it is `medium`. GizAi saves only totals per run, so the split into cache reads, writes and output, and the cost per card, can't be seen yet.
2. **caveman.so** is real and open (Apache-2.0): a prompt that makes replies terse, a local proxy and hooks that shrink tool output with the originals kept, and a closed cloud router (§3). Its numbers are thinner than its site: the 33.2% comes from 6 tasks of large noisy outputs, the 65% isn't measured against a concise prompt, and the routing's 0.46× is an interim run of 85 tasks in which "Most of the saving comes from lower effort" (§3.3). GizAi can do each of the three natively.
3. **A proxy is out.** With a Claude subscription, a proxy that changes requests breaks them, and Anthropic doesn't permit third-party developers to "intermediate" a plan's session tokens; GizAi's model, the user's own unmodified Claude Code with their own login, is the one its terms allow (§3.4). Codex is looser, but the same design works for both.
4. **The ideas, ranked, each a setting with a default (§4):** effort and model by card size, raised after a QA fail; a stable Team Lead prompt so its chat stays in the cache; board checks on their own model and effort; a cap on a run's context; shorter test and build output with the full log kept; comments and memory as ranges; short messages; and room left in the subscription's windows.
5. **Recommended: build them natively in GizAi (option A, §6), measure first.** Card M1 records the token split, turns, effort and largest context per run; then every idea runs as an experiment on half the cards, with QA's pass rate, rounds and failed runs as the guard (§5). Thirteen cards (§8); 13 open questions (§9).

---

## 2. The project now, and where its tokens go

### 2.1 What GizAi does, in one paragraph

GizAi is a local desktop app (Tauri 2, Rust and React) for one developer or a small agency and a team of coding agents. Each card on the board is worked by an agent: GizAi starts the user's own installed CLI (Claude Code, Codex, Gemini or another) headless in the card's own git worktree, with a prompt GizAi writes, and reads the CLI's event stream for the run's log, its result line and its usage (README.md:3-5; `crates/gizai-agents/src/claude.rs:64-125`; `src-tauri/src/runs.rs:1021-1036`). The Team Lead answers in Chat and checks the board, on Claude Code only (`src-tauri/src/chat.rs:1394-1397`). The user pays the CLIs through their own subscription, so what runs out is the subscription's 5-hour and weekly limits, which GizAi reads from the CLIs' own reports and shows on Usage → Subscription (`crates/gizai-core/src/limits.rs:1-16`). [V]

### 2.2 How an agent run spends tokens

A run is a loop of turns: each turn sends the whole conversation so far to the model, and the model answers with thinking, a little text and one or more tool calls; the tool results go into the conversation, and the next turn starts. Three things follow, and they decide where savings are: [V for the cache rules, Anthropic's prompt caching and effort pages; I for the arithmetic]

1. **Everything in the context is read again on every later turn.** Claude Code caches the conversation, so a repeated read is a *cache read*, at 0.1× the input price, and 0.05× on Opus 5.5 and Sonnet 5.5; what is new in a turn is a *cache write*, at 1.25× (5-minute cache) or 2× (1-hour cache) the input price. On a Claude subscription, Claude Code's main conversation, `-p` runs included, uses the 1-hour cache. "Cache prefixes are created in the following order: `tools`, `system`, then `messages`." So 10,000 tokens of test output read at turn 30 of a 150-turn run cost one cache write and 120 cache reads: 2 + 120 × 0.05 = 8× their size in input-price terms on Opus 5.5, 14× on Haiku 4.5. A token is cheapest when it never enters the context.
2. **A change early in the prefix makes everything after it new again.** A system prompt that differs from the last turn's turns the whole conversation after it into cache writes. This matters for the Team Lead's chat (§2.5).
3. **Output costs 5× input, and effort decides how much output there is.** Anthropic's effort page: "The effort parameter affects **all tokens** in the response", thinking, text and tool calls, and "Lower effort also means fewer and terser tool calls." On Opus 4.5 and later "thinking blocks are preserved by default", so thinking also stays in the context and is read again on later turns.

Prices per million tokens, from Anthropic's pricing page (Claude Code's own model list, as GizAi's tests hold it, says the same for input and output: `src/lib/models.test.ts:7-10`): [V]

| Model | Input | Cache write, 5 min | Cache write, 1 hour | Cache read | Output |
|---|---|---|---|---|---|
| Opus 5.5 | $4 | $5 | $8 | $0.20 | $20 |
| Sonnet 5.5 | $2 | $2.50 | $4 | $0.10 | $10 |
| Haiku 4.5 | $1 | $1.25 | $2 | $0.10 | $5 |
| Haiku 5.5 (prompts up to 100,000 tokens) | $0.10 | $0.125 | $0.20 | $0.01 | $0.50 |

"Cache hits and refreshes on Claude Opus 5.5 and Claude Sonnet 5.5 are priced at 0.05x the base input price." Claude Code's `haiku` alias now resolves to Haiku 5.5 on the Anthropic API, so GizAi's fixture is a little behind.

### 2.3 A run's prompt, part by part

GizAi's prompt goes to the CLI on stdin as the first user message (`crates/gizai-agents/src/claude.rs:64-67`). For a new card run it is built in this order (`crates/gizai-agents/src/prompt.rs:60-97`, `:102-107`, `:206-208`; `src-tauri/src/runs.rs:1021-1029`). Sizes are characters; a token is about 4 characters of English prose and fewer for code and paths [I]. [V for the code and the sizes of files in the repository]

| # | Part | Where it comes from | Cap today | Size |
|---|---|---|---|---|
| 1 | Role instructions | the agent's own text (agent form), else the role template (`runs.rs:921`; `crates/gizai-core/src/seed.rs:190-235`) | none | templates: Backend 4,532, Frontend 4,640, QA 6,338, DevOps 12,538 (`wc -c crates/gizai-core/roles/*.md`); this team's R&D Agent about 5,500 [I, counted roughly from this run's prompt] |
| 2 | Task: id, title, project goal, description, acceptance criteria | the card and its project (`prompt.rs:63-70`) | none | this card: about 6,500 description + 1,000 criteria [I, counted roughly] |
| 3 | QA's issues from the last failed QA run | builders only (`runs.rs:918`; `prompt.rs:71-77`) | none | usually under 2,000; the same list is also in QA's comment twice (below) |
| 4 | Recent comments, oldest first | the last 10 by anyone, agents' summaries included (`runs.rs:912-913`), each cut to 2,000 characters with no mark where it was cut (`prompt.rs:37`, `:81`) | 10 × 2,000 | up to 20,000 |
| 5 | Your branch | the fetched main branch (`prompt.rs:85-92`) | | about 100–250 |
| 6 | Limits of this run | Settings → Runs (`runs.rs:914`; `prompt.rs:109-113`) | | about 330 |
| 7 | Memory | the agent's notes, then the card's shared notes, then a list of the rest (`crates/gizai-core/src/memory.rs:1857-1956`) | 6,000 in full (own notes 3,000 when there are shared notes), list 4,000 (`memory.rs:30-35`) | up to about 10,600 with its 560-character intro |
| 8 | How this run works | per CLI and permission mode (`prompt.rs:297-353`) | | on Claude Code about 2,900 with the 29 default commands (`seed.rs:245-250`), 3,100 with a builder's list, 3,600 with DevOps' 59 commands; on Codex about 1,800 |
| 9 | The untrusted-content line | when the run has MCP servers or web tools (`runs.rs:1013`, `:1029`; `crates/gizai-agents/src/mcp_run.rs:10-12`) | | 250 |

So a new run starts with roughly 15,000–30,000 characters of GizAi's own prompt, about 4,000–8,000 tokens, and up to about 15,000 tokens with a long card, ten long comments and full memory [I, from the caps]. Before it, Claude Code sends its own system prompt and its tools' definitions with every request; GizAi doesn't write those, but it chooses which tools a run gets (`--tools`, `--allowedTools`, `--mcp-config`, `--strict-mcp-config`; `claude.rs:68-125`). [V]

A continued run (Continue, an answered question, GizAi's nudge) resumes the same session and sends only a short message: its prompt and memory are already in the session (`runs.rs:1017`, `:1021-1026`; `prompt.rs:136-180`). A builder's run after QA's `qa_fail` is a new session with QA's issues in part 3 (`runs.rs:918`). [V]

What this means [I]: the prompt itself is not where most tokens go. Even at 8,000 tokens, a 150-turn run reads it back about 150 times, 1.2 million cache-read tokens, which is the price of about 60,000 fresh input tokens: some $0.24 at Opus 5.5's prices. It counts, but the context that grows during the run (tool results, file reads, thinking) is read back the same way and is usually far bigger.

**One real run, measured.** GizAi's tests keep the event stream of a real Claude Code 2.1.289 run (Haiku 4.5, the GA-48 shell-rules check) as a fixture. Its last line says (`crates/gizai-agents/tests/fixtures/run-refused.jsonl:21`): [V]

| | Tokens | Price (Haiku 4.5) | Share of the $0.0944 |
|---|---|---|---|
| Fresh input | 185 | $1 / M | 0.2% |
| Cache writes, all with the **1-hour** lifetime (`ephemeral_1h_input_tokens: 11139`) | 11,139 | $2 / M (2×) | 24% |
| Cache reads | 604,313 | $0.10 / M (0.1×) | 64% |
| Output (832 of it thinking) | 2,300 | $5 / M | 12% |

The shares add up to Claude Code's own `total_cost_usd` of $0.09439 to the cent [V, arithmetic]. The run had 23 turns of tiny tool results (`git status`, `ls`), and its last request was 29,292 tokens, nearly all from the cache: so Claude Code's own system prompt and tools plus GizAi's prompt were already some 25,000–29,000 tokens before any work [I]. Two lessons: on a subscription, Claude Code writes its cache with the 1-hour lifetime (2× input), so a cache miss is expensive; and in a real run nearly all input tokens are cache reads, turn after turn, of a context that only grows.

### 2.4 What agents read and write during a run

- **Test and build output.** The project tells every builder to run, before it says it is done, `cargo test --workspace`, `npm test`, `npm run build` and `scripts/ui-test.sh` when the UI changed (CLAUDE.md:39-44), and QA runs the whole suite twice, "once to see where things stand and once more after your new tests" (`crates/gizai-core/roles/qa.md:10`). GizAi's own suites are big: 1,519 Rust test functions whose names alone are about 99,600 characters (66 on average; `rg` over the functions after `#[test]` and `#[tokio::test]`), and 1,915 `it`/`test` calls in 131 test files (`rg` over `*.test.ts`, `*.test.tsx` and `*.test.mjs`). A passing `cargo test --workspace` prints one line per test, so roughly 130,000–150,000 characters (35,000–40,000 tokens), and a fresh worktree first prints a `Compiling` line for each of the 588 crates in `Cargo.lock` [I, estimated from the counts; not run]. Claude Code doesn't put all of that in the context (§3.6): a command that succeeds is shown in full up to about 30,000 characters, and past that the model gets the path of a file with the whole output and a preview of its first 2,000 characters; a command that fails is shown up to about 10,000 characters, and past that as "a head-and-tail excerpt of that size", with no file path. So a long passing suite costs little, but its preview is the start of the output (for cargo, the `Compiling` lines) and the agent reads or searches the file for the result; a long failing one loses its middle, where failures can be. Codex cuts long output to its head and tail too (§3.6). Whatever is read stays in the context for the rest of the run. [V for Claude Code's and Codex's docs]
- **Files, diffs and searches.** Read, Grep and `git diff` results enter the context the same way. GizAi doesn't shape them today; Claude Code's Read returns at most 25,000 tokens a call and pages a larger file (§3.6).
- **Thinking and text.** On Opus 4.5 and later, thinking stays in the context (§2.2), so effort `max` grows the context too, not only the output.

### 2.5 The Team Lead: chat answers and board checks

- **Chat.** "One message is one turn: `claude -p` resumes the thread's session" (`src-tauri/src/chat.rs:1`, `:819-865`). Its system prompt is appended with `--append-system-prompt` (`chat.rs:1191`) and holds the Team Lead's fixed rules (`chat.rs:975-990`), one line per project copy of the code, its folders, its instructions, and **its Memory block**: its own notes in full (up to 6,000 characters) and a list of every other note it may read, each with its size, `- {path} ({n} characters)` (up to 4,000; `chat.rs:993`, `:1000-1003`; `src-tauri/src/memory.rs:19-31`; `crates/gizai-core/src/memory.rs:1848-1956`, the line at `:1946`). [V]
- **The chat's cache breaks more often than the code expects.** The code says the system prompt is "The same from turn to turn (only the projects change it), so the prompt cache keeps working" (`chat.rs:957`). But it also changes when the Team Lead writes to its own notes (its instructions ask it to keep Memory up to date: `crates/gizai-core/src/seed.rs:209`), when **any** note in the list changes size, which happens every time an agent's run adds a `learned` line to its Notes, and at midnight ("Today is {today}", `chat.rs:976`, `:994`). Since the system prompt comes before the messages in the cache's order, each such change makes the next chat turn write the whole conversation to the cache again, at 2× the input price on the 1-hour cache. [V for the code; I for how often it happens and what it costs: GizAi doesn't record cache writes per turn, §2.7]
- **Board checks** run only when the code finds something new (`src-tauri/src/board.rs:113-125`; `crates/gizai-core/src/board.rs:1-9`), each in a fresh session that is never resumed (`chat.rs:1420`, `:1459-1460`), with Gizai's MCP server and Read, Glob and Grep (`chat.rs:1438-1458`), on the Team Lead's own model and effort (`chat.rs:1456`, `:1461`). A check is already lean in what starts it; each one still pays the fixed prefix (Claude Code's prompt and tools, Gizai's tools, the Team Lead's prompt and memory), from the cache only if the same prefix was written within the hour. [V; I for the cost]
- The Team Lead is the team's biggest single agent: $499 of $2,771 in October so far (§2.7).

### 2.6 Models and effort

- Each agent has a model and an effort in its form, and every run and chat turn uses them: `--model` and `--effort` for Claude Code (`crates/gizai-agents/src/claude.rs:47`, `:99-104`), `-m` and `-c model_reasoning_effort=…` for Codex (`crates/gizai-agents/src/cli.rs:214-219`), from `agent.model` and `agent.effort` (`src-tauri/src/runs.rs:1031`). No card sets its own. The form offers the levels Claude Code reports for the model (`src/lib/models.ts:1-17`). [V]
- A card already has one cost switch: **Testing**. "On: a finished run sends the card to Testing for the QA Agent. Off: straight to Review (a small fix)" (`crates/gizai-core/src/model.rs:183-184`). [V]
- Most of this team runs Opus 5.5 at `max` or `xhigh` (the card). Claude Code's own default is "`high` on every model that supports effort, except that Opus 5.5, Sonnet 5.5, and Haiku 5.5 default to `medium`", so a seeded agent without an effort runs Opus 5.5 at `medium` (`crates/gizai-core/src/seed.rs:61`: "its CLI's own model and effort"). Anthropic's effort page also says Opus 5.5 **defaults to `medium`**; `xhigh` is for "Long-running agentic and coding tasks (over 30 minutes) with token budgets in the millions"; and for Opus 5.5: "Run an effort sweep on your own evals rather than carrying settings over from an earlier model". Of `max` it says, for Opus 4.7: "On most workloads `max` adds significant cost for relatively small quality gains, and on some structured-output or less intelligence-sensitive tasks it can lead to overthinking." For Sonnet 5.5: "For agentic coding and multistep tool use, start with `medium` for well-specified tasks and move to `high` for harder or longer ones." [V, platform.claude.com effort page]
- Anthropic's own data point for effort, from the Opus 4.5 launch: "Set to a medium effort level, Opus 4.5 matches Sonnet 4.5's best score on SWE-bench Verified, but uses 76% fewer output tokens." [V, anthropic.com]

### 2.7 What GizAi records, and the real numbers

**Per run** (`runs` table, `crates/gizai-core/migrations/0008_board_check.sql:23-50`): the agent, card, trigger (`assigned`, `routed`, `chat`, `manual`, `approval`, `nudge`, `board_check`), role, model, outcome, start and end, input tokens, output tokens and cost. [V]
- Input tokens are fresh input + cache writes + cache reads added together (`crates/gizai-agents/src/stream.rs:116-127`); the `cache_read_tokens` and `cache_write_tokens` columns exist but "stay empty" (`crates/gizai-core/src/usage.rs:7-8`). So GizAi can't tell a cache miss from a cache hit, though on Opus 5.5 the two differ 40× in price (a 1-hour write at 2× the input price, a read at 0.05×). [V]
- The cost is Claude Code's `total_cost_usd`; Codex and Gemini report none, so their runs show tokens with an unknown cost (`usage.rs:5-10`). [V]
- Not saved: the number of turns (parsed, `stream.rs:127`, then dropped), the effort, the thinking tokens, and Claude Code's `modelUsage` per model (`run-refused.jsonl:21` shows it has one entry per model a run used, subagents included). [V]
- Subscription limits: the newest reading per CLI and window (5-hour, 7-day, and per model), not per run (`crates/gizai-core/src/limits.rs:1-16`). [V]

**The Usage page** (`src/pages/UsagePage.tsx:24`, `:247-273`; `usage.rs:99-119`): a Subscription tab (the limits), then totals, per day, per agent and per project, for today, 7 or 30 days or this month. Chat turns and board checks are one "chat" line; there is no view per card and no split of input into fresh, written and read. [V]

**The real numbers** this team gave on the card (Claude Code's estimate at API prices, October 1–14, 2026, subscriptions, not bills):

| Agent | API-price estimate | Share |
|---|---|---|
| Team Lead (chat and board checks) | $499 | 18.0% |
| Backend Agents ($290 + $490) | $780 | 28.1% |
| QA Agents ($360 + $359) | $719 | 25.9% |
| Frontend Agents ($152 + $166) | $318 | 11.5% |
| R&D Agent | $226 | 8.2% |
| Design Agent | $173 | 6.2% |
| DevOps Agents ($35 + $21) | $56 | 2.0% |
| **All** | **$2,771** | 100% |

About $198 a day at API prices [V, arithmetic]. Builders and QA are 72% of it, so savings in task runs matter most; the Team Lead is next. How these dollars split into cache reads, cache writes and output can't be read from GizAi today (above); if this team's runs look like the measured one and like typical long agent runs, cache reads are the largest part, then output, then cache writes [I]. Card M1 (§8) records the split, so the first week after it gives the real one.

---

## 3. caveman.so, checked

Read on 2026-10-14: the site (caveman.so, its product and pricing pages, docs.caveman.so) and the repository `JuliusBrussee/caveman` at commit `d262ab3` (`main`, tag `v4.0.1`, 2026-10-13). Paths below are in that repository. [V unless marked; the Team Lead's notes on the card were right in outline, and wrong or loose on the points marked ✱]

### 3.1 What it is

Caveman is "Viral skill + proxy for coding agents" (its GitHub description), by Julius Brussee: Apache-2.0, 111,628 stars, created 2026-04-04, pushed today, mostly Go, with JavaScript, TypeScript and Python. Its README opens: "Caveman make your AI agent say less and read less. Code stay exact. Brain still big." It comes as four things:
- **A skill and plugins** that make replies terse, for Claude Code (`claude plugin install caveman@caveman`), Codex and others; most of the "30+ agents" get only this.
- **A CLI with a local runtime** (`npx @caveman-ai/cli@latest`; Go binaries `caveman-proxy` and `caveman-engine` in `~/.caveman/bin`) that listens on `127.0.0.1:8787` between the agents and their providers. Ten agents get it as a wrapper: "claude, codex, gemini, opencode, aider, kilo, qwen, hermes, openclaw, pi" (README L259). ✱ So "reads less" reaches about ten agents, not 30+.
- **21 middleware adapters** for apps that call models themselves (8 TypeScript, 13 Python: the Vercel AI SDK, OpenAI, Anthropic, LangChain, Mastra, LiteLLM, MCP and more; docs.caveman.so/caveman/middleware).
- **Caveman Cloud**, closed and paid (§3.7), which also runs the router behind "picks the model".

### 3.2 Its three ways, as built

**Says less: a prompt, nothing more.** `skills/caveman/SKILL.md` (89 lines), injected by a Claude Code `SessionStart` hook as hidden context, or by a `hooks.json` entry in Codex. Its rules, word for word: "Respond terse like smart caveman. All technical substance stay. Only fluff die." (L11); "No greeting, hedging, pleasantries, recap, or closer." (L39); "Drop a/an/the when the sentence still reads in one pass. Fragments fine. Never drop not/never/no/only/except. Numbers and units exact." (L47). Code and errors stay exact only because the model is told so: "Code blocks unchanged. Commands, paths, API names exact. Errors quoted exact, shortest decisive line only." (L58); and the rules lift for "Anything persisted outside chat: code, comments, commits, docs, issues, PRs, tickets, memory files…" (L80). Three levels: caveman, ultracave and megacave (Classical Chinese). The rules "cost about 1,160 tokens of input" (README L125). Headless runs are left out on purpose: "Headless sessions start off. `claude -p` and Agent SDK sessions … start under the `manual` policy: nothing injected until an explicit `/caveman`. … Set `CAVEMAN_DEFAULT_MODE=<mode>` in the environment to opt a headless run back in." (`src/hooks/README.md` L49-54). GizAi's runs are exactly such sessions.

**Reads less: compressors in the local runtime, originals kept.** "Default registry contains 16 compressors" (JSON, logs, code, diffs, search results, HTML, tables, test reports, terminal output and more; `docs/technical/engine.md`). Two rules are written down: test reports "keeps the counts and every failure", code "keeps imports, signatures, and type declarations and elides function bodies"; and in doubt it sends the original: "No compressor, parse error, larger result, or recovery-store failure returns original bytes." Originals go to a local store (`~/.caveman/ccr.db`, 512 MiB) and the model gets a handle (`ccr_…`) it can fetch back with `caveman tools retrieve` or the MCP tool `mcp__caveman__caveman_retrieve` (`docs/technical/context-recovery.md`; `proxy/README.md` L64). It reaches the model three ways:
1. **A proxy in the request path:** "A base-URL-swap reverse proxy: point an agent at `http://127.0.0.1:8787` and its LLM traffic flows through Caveman with no code change." (`proxy/README.md` L3-4). For Claude Code its install leaves "`ANTHROPIC_BASE_URL` and `_CLAUDE_CODE_ASSUME_FIRST_PARTY_BASE_URL` in `~/.claude/settings.json`" (`docs/technical/install-and-update.md`); for Codex, "`caveman enable codex` (or `caveman codex`) points Codex at `/chatgpt`", where "zstd request bodies (Codex's default) are decoded, compressed and re-encoded".
2. **Claude Code hooks:** a `PostToolUse` hook returns `updatedToolOutput` and so replaces a tool's result before the model reads it (`packages/cli/src/native-hook-fast.ts`), talking to the local runtime over a socket.
3. **A wrapper command,** `caveman shrink -- pnpm test` (README L265), and an opt-in MCP server.

**Picks the model: a closed router in Caveman Cloud.** "Auto" is a model in the agent's picker (`caveman-auto[1m]` in Claude Code); for each ask, Caveman Cloud picks the model and effort. How it decides is not published. ✱ It is not local: each ask "sends Caveman Cloud your latest ask, the one before it and the end of the agent's last reply, raw (the last 128 KiB, 16 KiB and 16 KiB of each)" (`SECURITY.md`), and "On the Free plan Caveman may keep it to improve routing" (README L328). It "needs a free Caveman account; if the router is missing, slow or fails, the task runs exactly as the agent asked" (caveman.so).

**Logins.** "It runs on your machine, with your keys and your Claude Pro/Max login." (README L71); "Works in Claude Code, OpenCode, and Codex on a ChatGPT login, with API keys, Claude Pro/Max, or ChatGPT." (README L217). Where Caveman itself would call Anthropic for Auto, its CLI says: "Claude Pro/Max logins work only in Anthropic's own apps; add an Anthropic API key instead" (`packages/cli/src/modules/provider-logins.ts`). Whether a proxy in Claude Code's path is allowed on a subscription is §3.4.

### 3.3 What its numbers rest on

| Claim on caveman.so | Its source in the repository | What it measured, in the source's words |
|---|---|---|
| "Whole sessions used 33.2% fewer input tokens, with 18 of 18 answers right" | `docs/WRAP-BENCHMARK.md` | Claude Code 2.1.223 on `claude-sonnet-5`, "54 total agent runs and 18 direct/Caveman pairs" over 6 tasks (885,793 → 591,673 input tokens), answers "checked by an exact semantic JSON oracle". ✱ Not coding tasks: "Workloads are deterministic large tool outputs, not open-ended coding tasks or customer traffic"; "each test file hides one record in 61 to 72 KB of noise" (README L196); "It cannot be independently reproduced from this checkout". |
| "65% less readable output on a 10-prompt benchmark" | the v1.5.0 README's table (average 1,214 → 294 tokens) | Against default, verbose replies. `docs/HONEST-NUMBERS.md` now lists that comparison as "Not published", and measures against an "Answer concisely." control: "caveman 3%, ultracave 35%, megacave 9% at the median (n=10, single run, output length only…)". The site itself adds: "That is not a cost saving: on long-horizon coding tasks it is about 10% fewer tokens in total." |
| "Routed Claude Code cost 0.46× as much per task as always running Opus 5.5 at max effort, with the same solve rate, on 85 paired agentic coding tasks from SWE-Interact and SWE-Together (interim run, list-price estimates)" | `docs/AUTO-BENCHMARK.md` | "Interim result, 2026-10-03. The run was paused at 85 paired tasks of 169." ✱ The baseline was "Opus 5.5 at xhigh effort for every request, no routing", not max. Score gap "−0.5 [−6.8, +5.7]" points: "At 85 tasks the arms cannot be told apart: the interval is about ±7 points." "Cost is list price, from the router's price catalog, not billed amounts." **"Most of the saving comes from lower effort (Opus xhigh to Opus medium or high), not from cheaper models."** Auto chose "Opus medium 87%, Opus high 8%, Sonnet 5.5 5%". "The raw trajectories are not in this repository." |
| The calculator's combined saving | the site's "Savings" note | "The figures come from separate studies and are multiplied, one after another; they were not measured together, so the result is an estimate, not a bill." |

What this means for GizAi [I]: Caveman's strongest number, routing, is mostly a lower effort, which GizAi can set itself without a router (§4.1), and which this team, at `max` and `xhigh`, has the most room for. Its "reads less" was measured on large noisy outputs, where Claude Code already moves a long successful output to a file (§2.4); and its own site puts "says less" at about 10% of the tokens of long coding tasks, and its measured terse level at 3% against a concise prompt.

### 3.5 What GizAi may reuse under Apache-2.0, and the NOTICE

- **Licence.** "Everything in this repository is licensed under the Apache License, Version 2.0, starting with Caveman 3.0.0" (`LICENSING.md` L3-6); README: "Apache-2.0, whole repo." Contributions from before 3.0 are also under MIT (`LICENSE-MIT`). Not covered: "Caveman Cloud, the hosted service, is separate commercial software. Its source is not in this repository and this license does not cover it." (`LICENSING.md` L8-9). GizAi is Apache-2.0 too (`Cargo.toml:7`, `package.json:5`, `LICENSE`). RTK is Apache-2.0 (§6).
- **Its NOTICE file exists:** "Caveman / Copyright 2026 Julius Brussee", followed by third-party parts: a pxpipe port (MIT), the Spleen and GNU Unifont fonts, chromedp (MIT), Go modules, a port of Qwen Code's MCP server and permission-rule matchers in `packages/cli/src/index.ts` ("Copyright 2025 Google LLC / Copyright 2025 Qwen", Apache-2.0), the @clack packages (MIT) and the Geist fonts. [V, read through the fetch tool; its line numbers not cross-checked]
- **What copying would oblige GizAi to do** (Apache-2.0 §4, as in GizAi's own `LICENSE:94-118`): pass the licence on; "cause any modified files to carry prominent notices stating that You changed the files"; "retain … all copyright, patent, trademark, and attribution notices" in copied source; and, because Caveman has a NOTICE file, "include a readable copy of the attribution notices contained within such NOTICE file" that pertain to what was taken. §6 grants no use of its name (`LICENSE:138`), so no GizAi feature is called Caveman.
- **The NOTICE line GizAi would add**, only once something is copied or ported (GizAi's `NOTICE` today has its own two lines):
  ```
  This product includes software from Caveman (https://github.com/JuliusBrussee/caveman),
  Copyright 2026 Julius Brussee, licensed under the Apache License, Version 2.0.
  ```
  plus the Google/Qwen line if anything from `packages/cli/src/index.ts` were taken, and the same pattern for RTK with its own copyright line (RTK's NOTICE not checked).
- **What is worth taking** [I]: little code. GizAi's filter is Rust and Caveman's engine Go, and the useful parts are rules, not code: "keeps the counts and every failure", "returns original bytes" when in doubt, and a handle to fetch the original back. Written anew, ideas need no notice; credit them in the source anyway. If GizAi copies a sentence of `SKILL.md` into its Short messages line, that is text under the licence: credit it in NOTICE.

### 3.4 A proxy in the request path, with a subscription login

The question for any tool that sits between a CLI and the model provider (Caveman's local runtime, a gateway, a router): does it work, and is it allowed, when the user signs in with a Claude or ChatGPT plan instead of an API key?

**Claude Code: it works, but not when the proxy changes the requests.**
- Claude Code sends its requests to `ANTHROPIC_BASE_URL` when set, and a subscription login goes along: "Setting only that variable, without a gateway credential, doesn't replace the subscription. Requests still route through the gateway, but a saved claude.ai login remains the active credential, so its usage limits and billing apply." [V, code.claude.com llm-gateway]
- A proxy must pass requests on unchanged. "Capabilities that add body fields pair them with a beta header … A gateway that rewrites or redacts request bodies for content inspection breaks the pairing the same way stripping does, so inspect without modifying." With a claude.ai login the `anthropic-beta` header "also carries an OAuth capability that the upstream requires, and stripping it fails those requests with `401`." And Opus 5.5's preserved thinking "fails when `system`, `tools`, or earlier `messages` content differs from the request that produced the thinking. A gateway that rewrites any of that content can cause the rejection itself"; Claude Code then "removes earlier thinking blocks from the request, retries, and keeps them out of every later request". [V, code.claude.com llm-gateway-protocol] So a proxy that shrinks the tool results already in a conversation (Caveman's "reads less" in the request path) either fails requests or costs the model its earlier thinking [I, from those rules].
- A custom base URL also changes Claude Code's own savings: MCP tool search, which keeps tool definitions out of the context, "Falls back to loading upfront on… a non-first-party `ANTHROPIC_BASE_URL`" unless `ENABLE_TOOL_SEARCH=true` [V, code.claude.com agent-sdk tool-search].

**Claude Code: allowed for the user's own Claude Code, not for a third party in between.** Claude Code's Legal and compliance page, word for word [V, read 2026-10-14]:
- "**OAuth authentication** is intended exclusively for purchasers of Claude Free, Pro, Max, Team, and Enterprise subscription plans and is designed to support ordinary use of Claude Code and other native Anthropic applications."
- "Anthropic does not permit third-party developers to offer Claude.ai login into their own applications, or to route requests through Free, Pro, or Max plan credentials on behalf of their users. Moreover, developers may not collect, store, or intermediate Claude.ai credentials or session tokens — sign-in to a Claude account must complete through Anthropic's own flow."
- What it does allow, and what GizAi does today: "Nor does it prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription, including where a platform hosts Claude Code". And for products that run Claude Code: "The Claude Code binary must not be modified" and "Customers may not pay for, resell, or intermediate Claude usage on their end users' behalf."
- "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK." "Anthropic reserves the right to take measures to enforce these restrictions and may do so without prior notice."
- The Consumer Terms (§3) forbid access "through automated or non-human means, whether through a bot, script, or otherwise", "Except when you are accessing our Services via an Anthropic API Key or where we otherwise explicitly permit it" [V, anthropic.com consumer-terms]. Anthropic permits `claude -p` on a plan: its support article, updated in June 2026, says "Claude Agent SDK, `claude -p`, and third-party app usage still draw from your subscription limits" [V, support.claude.com, quoted through the fetch tool].
- Enforcement is real: in January 2026 an Anthropic engineer wrote that "accounts were banned for triggering abuse filters from third-party harnesses using Claude subscriptions", and in April 2026 the head of Claude Code that "Claude subscriptions will no longer cover usage on third-party tools like OpenClaw" [secondary: their posts on X, read through a mirror].

My reading [I, not legal advice]: a local proxy that holds and forwards the user's session token on every request is a third-party developer "intermediating" it, and one that rewrites requests breaks them. GizAi runs the user's own unmodified `claude` with the user's own login, which the page explicitly allows; putting Caveman's runtime, or anything like it, in that path would move GizAi's users from the allowed side to the doubtful one, and the risk would land on their accounts.

**Codex: it works, and nothing forbids it outright.**
- "If you just need to point the built-in OpenAI provider at an LLM proxy, router, or data-residency enabled project, set `openai_base_url`"; a custom provider with `requires_openai_auth = true` takes a ChatGPT sign-in [V, learn.chatgpt.com config-advanced and auth; developers.openai.com/codex now redirects there]. Whether `openai_base_url` with a ChatGPT login works for a Plus or Pro plan isn't stated; OpenAI documents a gateway with ChatGPT sign-in for enterprise workspaces, which must "proxy `/models` and `/responses` to `https://chatgpt.com/backend-api/codex`".
- OpenAI's Terms of Use forbid to "circumvent any rate limits or restrictions or bypass any protective measures" and to "share your account credentials or make your account available to anyone else" [secondary: openai.com answered 403, the text is from a terms-tracking copy]. OpenAI also invites third-party apps to use a ChatGPT plan, through its own sign-in: "Let users sign in with ChatGPT and, for eligible Plus and Pro users, use their ChatGPT plan for AI requests in your app", "in a limited trial with selected partners" [V, learn.chatgpt.com DevDay 2026].
- My reading [I]: possible and probably allowed for a user's own Codex, but undocumented with a ChatGPT login, and the proxy holds the user's ChatGPT tokens.

**A gotcha for every subscription user of GizAi** [V for the doc; V that runs don't remove it]: "`ANTHROPIC_API_KEY` … In non-interactive mode (`-p`), the key is always used when present." GizAi's runs start with Gizai's own environment plus their own lines (`crates/gizai-agents/src/claude.rs:49-50`, `:56-62`), and nothing removes that variable (`rg` finds no removal in the run code), so a key in the user's shell makes every agent run bill the API instead of the plan. §9, question 12.

**What fits one subscription.** Anthropic gives no absolute numbers: "Both Pro and Max plans have a five-hour session limit and a weekly limit", and Max gives "5x or 20x the Pro plan's usage allowance" [V, support.claude.com]. Codex's pricing page gives Plus and Business a number of local messages per five hours per model, says "Pro plans currently have no five-hour limit", and that "Weekly limits may also apply" [V, learn.chatgpt.com pricing]. So what fits is measured, not looked up: GizAi already reads each window's use (§2.7), and M1 adds what each run and card cost. For scale: this team's 14 days came to $2,771 at API prices across 12 agents (§2.7); a freelancer with one plan runs a fraction of that, which is why the effort and size settings (§4.1) and the window cap (§4.8) matter more to them than to a team with several plans. Since 7 October 2026, Max and Team plans also come with monthly API credits ($100 for Max 5x, $200 for Max 20x), but Anthropic's credits page lists "Claude Code" under "Doesn't cover" [V, platform.claude.com api-credits-for-subscribers]; its support article, updated the same day, says they cover "the Claude Agent SDK, `claude -p`, the Claude API, and Claude Managed Agents", and also that "When you're signed in with your Claude plan instead, `claude -p` and Agent SDK usage still draw from your plan's usage limits and don't use your API credits" [support.claude.com, quoted through the fetch tool]. The two disagree, so GizAi shouldn't build on the credits yet (§9, question 13).

### 3.6 What Claude Code and Codex already do

GizAi's native options build on what the CLIs already offer. Quotes from their docs, read 2026-10-14 (code.claude.com's pages; Codex's at learn.chatgpt.com, where developers.openai.com/codex now redirects) [V unless marked]:

| | Claude Code | Codex | GizAi today |
|---|---|---|---|
| Long command output | Successful: "Inline up to roughly 30,000 characters by default; past that, the path of a file saved to the session directory … plus a preview of up to the first 2,000 characters, and Claude reads or searches the file when it needs the rest". Failed: "Inline up to roughly 10,000 characters; past that, a head-and-tail excerpt of that size cut from the read-back window, with no file path". `BASH_MAX_OUTPUT_LENGTH`: "default: 30000; maximum: 150000" | `tool_output_token_limit`: "Token budget for storing individual tool/function outputs in history", no default given (the sample config shows 12000); a cut to head and tail with "…N tokens truncated…" and no file [secondary: an open issue on openai/codex, #14206] | sets neither |
| Files | Read: "at most 25,000 tokens" a call, a whole file over 256 KB refused, so larger files are read in pages | | |
| MCP tools | "MCP tool definitions are deferred by default, so only tool names and server instructions enter context until Claude uses a specific tool"; an MCP result over 25,000 tokens goes to a file | a token budget per MCP tool | only the agent's own servers (§2.3) |
| Hooks | `PreToolUse` `updatedInput` rewrites a command; `PostToolUse` "`updatedToolOutput` replaces the tool's result"; `--settings '{"disableAllHooks": true}'` turns all off, and "There is no way to disable an individual hook while keeping it in the configuration" | "Hooks are enabled by default"; `PreToolUse` can rewrite a command (`updatedInput`); "`updatedMCPToolOutput` and `suppressOutput` are parsed but not supported yet" | all hooks off in Claude Code runs (`crates/gizai-agents/src/claude.rs:39`, `:93-95`) |
| Compaction | Models with a 1M window "compact before the window fills, at about 967K tokens by default"; `CLAUDE_CODE_AUTO_COMPACT_WINDOW` from 100000 to 1000000; it "clears older tool outputs first, then summarizes the conversation if needed" | `model_auto_compact_token_limit`: "Token threshold that triggers automatic history compaction" | the CLIs' own |
| Prompt cache | One hour for the main conversation, `-p` runs included, on a subscription; five minutes for subagents and compaction. "The cache is effectively scoped to one machine and directory". Breaks on "Switching models", "Connecting or removing an MCP server", "Compacting the conversation", "Upgrading Claude Code" and more; "On Opus 5.5, Sonnet 5.5, Haiku 5.5, and Fable 5.1 … changing effort keeps the cache" | | not touched |
| Effort | `low`, `medium`, `high`, `xhigh`, `max`; default `high`, but `medium` on Opus 5.5, Sonnet 5.5 and Haiku 5.5 | `model_reasoning_effort`: "such as `low`, `medium`, `high`, `xhigh`, `max`, or `ultra`" | the agent's own (§2.6) |
| Terser text | the **Concise** output style: "Responses lead with the result and leave out preamble, narration, and recaps"; `outputStyle` as a setting | `model_verbosity`: `low`, `medium`, `high` | none |
| Subagents' model | `CLAUDE_CODE_SUBAGENT_MODEL` (and `_FORCE=1` for every subagent) | | none |

Claude Code's own advice for fewer tokens (its costs page) lists, among others: "Clear between tasks", "Choose the right model" ("Sonnet handles most coding tasks well and costs less than Opus"), "Reduce MCP server overhead", "Offload processing to hooks and skills" ("a hook can grep for `ERROR` and return only matching lines, reducing context from tens of thousands of tokens to hundreds"), "Adjust extended thinking" and "Delegate verbose operations to subagents". Its `--bare` mode skips hooks, skills, plugins and MCP, but "bare mode doesn't use your subscription login", so it is no option for GizAi.

### 3.7 Caveman Cloud, and what Caveman sends

- **Plans** (caveman.so and its pricing page): Free, "$0 for one person, no card needed": "Every Cloud feature for one person", "100,000 events a month", "600 agent minutes a month", "25,000 routing decisions a month", "$10 of Caveman AI a month". Team, "$300 / month" after "a 7-day free trial": "$300 of usage every month", "$100 of Caveman AI a month", "Semantic cache", "Project access control and audit log export", "Content sharing off", "There are no seats." Enterprise: custom, "Runs in your VPC". Features: Analytics, Traces, Team, Improvements (Autopilot), Evals and Govern ("Soft and hard caps per project, rate limits per key and workflow…"). [V, the pricing page only in short quotes through the fetch tool]
- **Data:** "On Free, Caveman may use your prompt text and recorded requests to improve its models" … "Move to Team any time to turn sharing off." The site also says "Your coding agents talk straight to your provider, with your key. Caveman gets a routing ask, never the request", while the README says "Only a request routed to a Caveman Cloud model passes through Caveman Cloud, whole (system prompt, tool results, code)" (L329).
- **Telemetry, on by default:** "The CLI sends usage stats by default: commands run, token counts, caveman-blocks commands and usage counts, a random install ID, OS, and IP. Never your prompts, code, file paths, or scripts you wrote. … Off for good, Blocks included: `caveman telemetry off` or `DO_NOT_TRACK=1`." (README L327).
- **For GizAi's users** [I]: what Caveman Cloud's analytics show (spend per agent and project, traces, what each saving did) GizAi can show from data it already keeps on the user's computer (§2.7, §7.4), for free. A freelancer's client code in a free tier that may train on it is a no-go, and the Team plan costs more than the subscription it would measure.

## 4. The ideas, ranked, each as a setting

Ranked by what they should save for this team and how much they risk quality. Every saving is a setting the user controls (Jefsev's rule): a switch, or a value within a range, with a default proposed for freelancers and small agencies. "Settings → Tokens" is a new Settings tab (§7.1); "agent form" and "card" are the existing forms. Savings are my estimates from §2 [I]; the experiment under each idea is what turns them into numbers.

| # | Idea | Caveman's way | Expected saving | Risk to quality | Setting, default |
|---|---|---|---|---|---|
| 1 | Effort and model by card size, raised after a QA fail | picks the model | high: 10–20% of a Normal card's runs moved from `max`/`xhigh` to `high`; 30–50% on Small cards | medium, guarded | card **Size** Small · Normal · Large (Normal); agent form: model and effort per size (Normal = today's); switch **Raise effort after a QA fail** (on) |
| 2 | A stable Team Lead prompt, so its chat stays in the cache | (the rest) | medium on the Team Lead: 10–30% of its chat cost | none | switch **Keep the Team Lead's prompt stable** (on) |
| 3 | Board checks on their own model and effort | picks the model | medium on the Team Lead's checks: 30–60% of their cost | low | agent form → Chat: **Board check model** (the Team Lead's) and **effort** (`medium`) |
| 4 | Cap a run's context: compact earlier than the CLI would | (the rest) | unknown until M1 records each run's largest context; high on runs that grow past a few hundred thousand tokens | medium | **Compact a run's context at** 100,000–1,000,000 tokens (the CLI's own: off) |
| 5 | Shorter test and build output, full log kept | reads less | low: 1–3% of a QA or builder run, plus fewer follow-up reads | low, with its safeguards; improves what a failing run shows | switch **Shorten test and build output** (on), **Lines kept around a failure** 10–200 (40) |
| 6 | Comments and memory as ranges, cuts marked | (the rest) | small: about 1% of a run | low at the defaults | **Comments per run** 0–20 (10), **Characters per comment** 500–8,000 (2,000), **Memory per run** 1,000–12,000 (6,000), switch **List the other notes** (on) |
| 7 | Short messages between tool calls | says less | small: 1–3% of a run | low | switch **Short messages** (off) |
| 8 | Leave room in the subscription's windows | (fits one subscription) | none per run; keeps part of the window for the user | none | **Start no new runs above** 50–100% of a window (90%) |

Not proposed now, with the reason, in §4.9: continuing a builder's session after a QA fail, MCP servers switched per card, a model for subagents, sharing Claude Code's cache between worktrees, and reordering the prompt for the cache.

### 4.1 Effort and model by card size

**What it does.** A card gets a **Size**: Small, Normal or Large. Each agent's form keeps its model and effort, now as the Normal size, and gets two optional rows: the model and effort for Small cards and for Large cards (empty = the Normal one). The Team Lead sets the size when it writes or splits a card (its instructions get one line); the user changes it on the card like Testing. With **Raise effort after a QA fail** on, a builder's run after `qa_fail` runs one size up (Small → Normal → Large), so a card that failed QA gets more thought, not the same again. Today every run of an agent uses one model and effort, whatever the card (`runs.rs:1031`), and the only per-card cost switch is Testing (§2.6). This is Caveman's "cheapest model and effort that gets it right", decided by the Team Lead, who already reads every card, and by the user, instead of by a classifier in the request path. [V for today's code; I for the design]

**Why it saves the most.** Effort decides all output: thinking, text and tool calls (§2.2), and on Opus 4.5 and later the thinking stays in the context and is read again on every later turn. Anthropic's own advice for Opus 5.5 is to sweep effort on one's own work rather than carry settings over, and of Opus 4.7 it says that `max` "adds significant cost for relatively small quality gains" (§2.6); Claude Code itself runs Opus 5.5 at `medium` unless told otherwise. This team runs most agents at `max` or `xhigh`. Moving Normal cards to `high` and keeping `xhigh`/`max` for Large ones should save 10–20% of those runs; Small cards at `medium`, or on Sonnet 5.5 at half Opus 5.5's price per token, 30–50% [I: output is a fifth to a third of a long run's cost, and thinking also grows the cache reads; the experiment measures it].

**The setting.**
- Card → Properties → **Size**: Small · Normal · Large; default Normal, which changes nothing for today's agents.
- Agent form → Agent: **Model** and **Effort** stay as they are (the Normal size); new rows **Small cards** (model, effort) and **Large cards** (model, effort), each empty by default, meaning "as Normal". The effort list is the one the form already offers for the model (`src/lib/models.ts:1-17`).
- Settings → Tokens: **Raise effort after a QA fail**, on.
- Proposed defaults for a new install (seed): Normal at the CLI's default, Small one level lower, Large at `xhigh`. Existing agents keep exactly what they have until the user fills the new rows.

**What the user sees.** In the run's facts (`src/components/RunHistory.tsx:75-93`): "Size Small: Sonnet 5.5, effort medium (Small cards, Backend Agent)", or "raised to Normal after QA's fail". On Usage → Cards: each card's size, cost, rounds and QA result; on Usage → Savings: cost per card by size and by effort.

**Risk.** A card sized too small gets too little thought: more QA fails, more rounds. Guarded by the raise after a fail, by the user's view of the size on every card, and by the experiment's guard (§5.2). A card's runs then use different models, which is fine for the cache: each run is its own session.

**Experiment.** Card parity (§5.2) on Normal cards: even cards at `high`, odd cards at the agent's current effort, two weeks; per card: cost, QA pass rate, rounds, failed runs, questions. A second one for Small cards: Sonnet 5.5 against the agent's Opus. The effort experiment is the one big enough to read with about 35 cards per group (§5.2).

### 4.2 A stable Team Lead prompt

**What it does.** The Team Lead's system prompt holds its own notes in full and a list of every other note with its size, and the date (§2.5). Any change to it, between two turns of a chat, makes the next turn write the whole conversation to the cache again (2× the input price on the 1-hour cache) instead of reading it (0.05× on Opus 5.5). With the switch on, Gizai freezes the Memory block and the date in the system prompt for the life of a chat thread (as they were at its first turn), and lists the list without sizes, so it changes only when a note is added or removed. What changed since goes, as one short line, into the bracket line Gizai already puts before a message ("A message may start with a line from Gizai in square brackets", `chat.rs:984`): "Notes changed since this chat started: Agents/Backend Agent/Notes, Projects/GA. Read them with memory_read." The Team Lead wrote its own new notes in this chat, so it has them already. [V for the prompt and its bracket line; I for the design]

**Saving.** For a chat whose context is 80,000 tokens, one avoidable miss costs about (2 − 0.05) × 80,000 × $4/M ≈ $0.62 at Opus 5.5 API prices on top of a hit. Each agent's `learned` line, each note the Team Lead writes and midnight cause one today. If a third of the turns miss for these reasons, the chat's input cost falls by roughly 10–30% [I]. M1 measures it exactly: cache writes per chat turn, before and after.

**The setting.** Settings → Tokens: **Keep the Team Lead's prompt stable**, on. Off restores today's behaviour (useful to compare).

**What the user sees.** In a chat turn's facts (the chat run in Usage → Agents): the cache written and read; on Usage → Savings: "Team Lead: cache writes per turn, last 7 days vs before". In chat itself, nothing changes.

**Risk.** None to answers: the same facts reach the Team Lead, a turn later at most, and it can always read Memory with its tools. Board checks are separate sessions and keep their fresh block.

**Experiment.** Not by card: by week, since the Team Lead is one agent. A week off then a week on, comparing cache writes per chat turn and cost per turn; quality is unchanged by design (same information).

### 4.3 Board checks on their own model and effort

**What it does.** A board check runs only when Gizai's code finds something new, in a fresh session, on the Team Lead's own model and effort (§2.5). Most checks are routine: a card waiting, a run that ended, a question to route. With the switch, a check runs on its own model and effort. [V for today's code; I for the design]

**The setting.** Agent form → Chat, beside the Board check minutes (`src/components/AgentForm.tsx:274-279`): **Board check model** (empty = the Team Lead's) and **Board check effort**, proposed default `medium`. Range: the levels the form offers for that model.

**What the user sees.** Usage → Agents splits the Team Lead's line into Chat and Board checks (the `runs.trigger` column already tells them apart: `migrations/0008_board_check.sql:30`), with cost per check. The check's run facts show its model and effort.

**Saving and risk.** The checks' share of the Team Lead's $499 can't be read today (M2 shows it); a check at `medium` instead of `max` should cost 30–60% less [I]. Risk: a check that misses or misreads something new. It is cheap to see: the check's findings are recorded (`runs.findings_json`), and a missed item comes back at the next check.

**Experiment.** Alternate checks (even and odd run numbers) between the Team Lead's effort and `medium` for two weeks; compare cost per check and, from the board, cards left waiting longer than an hour and questions answered late.

### 4.4 Cap a run's context

**What it does.** Claude Code compacts a conversation by itself only when it nears the model's window: "Models running with a native 1M window compact before the window fills, at about 967K tokens by default", Opus 4.7 and later and Sonnet 5 and later among them. Until then every request reads the whole context again. With this value set, Gizai starts a Claude Code run with `CLAUDE_CODE_AUTO_COMPACT_WINDOW` ("Set the auto-compact window in tokens, from `100000` to `1000000`") and a Codex run with `-c model_auto_compact_token_limit=<n>` ("Token threshold that triggers automatic history compaction"), so a long run compacts earlier; Claude Code "clears older tool outputs first, then summarizes the conversation if needed". [V for the CLIs' docs (§3.6); I for the design]

**Why.** On Opus 5.5 a context of 400,000 tokens costs about $0.08 to read once (400,000 × $0.20/M), and it is read on every request of the run; a run whose context grows that far spends more on reading it than on anything else [I]. GizAi doesn't know today how large its runs' contexts get (it saves totals only: §2.7). M1 records each run's largest context, and that decides whether this is the biggest lever after effort or almost nothing.

**The setting.** Settings → Tokens: **Compact a run's context at**, 100,000–1,000,000 tokens; default off (the CLI's own), until M1's numbers and the experiment say where it pays. Task runs only; chat keeps the CLI's own.

**What the user sees.** In the run, a compaction as a step of its own (Claude Code marks it in its stream [I: to check on the current version]); in the run's facts, the largest context and the number of compactions; on Usage → Savings, the cost per turn of runs with and without it.

**Risk.** Medium: a summary can drop a detail the agent needs later, and a compaction costs a summarizing request and a fresh cache write. Old tool results go first, and those the agent can read again.

**Experiment.** After M1: if a fifth or more of the runs pass 300,000 tokens, card parity at 300,000 for two weeks: cost per card, QA pass rate, rounds, and files read again after a compaction.

### 4.5 Shorter test and build output, with the full log kept

**What it does.** A small command of Gizai's, `gizai-quiet`, runs a test or build command, writes its whole output to a file in the run's temp folder, and prints only what the agent needs: the command, its exit code and time, the counts (passed, failed, ignored), every failure with its message and the lines around it, every error and warning line from compilers and linters, and the path of the full log. The exit code is the command's own, so `&&` chains keep working. Agents are told in "How this run works" to run tests and builds through it, and their allowed commands get one rule per allowed command they already have: `Bash(gizai-quiet cargo:*)` next to `Bash(cargo:*)`, so it never widens what an agent may run. After the run Gizai keeps the full logs with the run's log (`<data>/runs/<run-id>/outputs/`), because the temp folder is emptied. A hook could do the same without the agent's help (Claude Code's `PostToolUse` can replace a tool's result), but GizAi turns all hooks off on purpose, so that the user's own hooks stay out of headless runs, and Claude Code has "no way to disable an individual hook while keeping it in the configuration" (§3.6). [I: the design; Caveman's and RTK's filters do the same job by other routes: a proxy or a `PostToolUse` hook (§3.2), a rewriting `PreToolUse` hook (§6)]

**Why.** QA runs the whole suite twice (`crates/gizai-core/roles/qa.md:10`), builders run `cargo test --workspace`, `npm test` and `npm run build` before they finish (CLAUDE.md:39-44), and a passing GizAi suite prints some 130,000–150,000 characters (§2.4). Claude Code already keeps most of a long *passing* output out of the context: the model gets a file path and the first 2,000 characters (§2.4). What is left is where a filter helps. The preview is the start of the output, not its result, so the agent reads or searches the file in more calls. A long *failing* output becomes a head-and-tail excerpt of about 10,000 characters with no file, so the failures in its middle are lost. Outputs under 30,000 characters come in whole. And Codex cuts output to its head and tail and keeps no file (§3.6). Through the filter every command prints its result first, a failing one keeps its failures in full, and the whole output is on disk for every CLI. Anthropic's own cost advice says the same of a hook that filters test output: it can return "only matching lines, reducing context from tens of thousands of tokens to hundreds".

**The setting.** Settings → Tokens: **Shorten test and build output**, on; **Lines kept around a failure**, 10–200, default 40. Per agent, the agent form's Allowed commands shows the added rules (a switch in the agent form can turn it off for one agent).

**What the user sees.** In the run, each shortened command is a normal step whose output starts with `gizai-quiet: 1,519 passed, 0 failed (exit 0) · full log: .gizai-tmp/outputs/03-cargo-test.log (142,311 characters)`. In the run's facts: "Shortened output: 4 commands, 212,000 → 9,800 characters", with **Open** beside each full log. On Usage → Savings: characters kept out of the context per agent and per card, and what they would have cost, read in the turns after them (§7.4).

**Saving.** Low in tokens: up to about 10,000 characters per failing command and 2,000 per long passing one, plus the reads that follow them, which are read again in every later turn: 1–3% of a QA run [I]. Possibly more on Codex, which keeps a head-and-tail cut of each long output in the context and no file [I]. The stronger reason is quality: a failing suite shows its failures instead of its two ends.

**Risk.** A filter bug hides an error. Guarded: the exit code and counts are always shown; any line that looks like an error is kept; an output it doesn't recognise is shown as its first and last lines, never as nothing; the full log is one `rg` away; and the filter's own tests run on recorded outputs of each tool, passing and failing. Agents that forget to use it: Gizai sees every command in the stream and counts the test and build commands run without it (in the run's facts).

**Experiment.** Card parity for QA and builders, two weeks: tokens per run and per card, QA pass rate, rounds, and QA's `needs_decision` runs; plus a fidelity check on every failing run in the "on" group: the user (or a script) compares the failures in the full log with what the agent reported.

### 4.6 Comments and memory as ranges, cuts marked

**What it does.** The prompt's comments and memory have fixed caps today: the last 10 comments by anyone, each cut to 2,000 characters with no mark (`runs.rs:912-913`, `prompt.rs:37`, `:81`), and 6,000 characters of notes plus a 4,000-character list (`crates/gizai-core/src/memory.rs:30-35`). This idea makes them values the user sets, marks a cut ("… (cut at 2,000 of 5,400 characters; the full comment is on the card)"), and leaves QA's comment out of a builder's prompt when its issues are already the prompt's "QA found these issues" section (§2.3, row 3). [V for today's caps]

**The setting.** Settings → Tokens: **Comments per run** 0–20 (10); **Characters per comment** 500–8,000 (2,000); **Memory per run** 1,000–12,000 characters (6,000), which also caps the Team Lead's block; **List the other notes** on. Defaults are today's, so nothing changes until the user moves them.

**What the user sees.** The run's facts: "Comments: 10 of 14, 2 cut" and, as today, each note given and whether it was cut (`RunHistory.tsx:88-91`). Usage → Savings: characters left out of prompts per agent.

**Saving and risk.** Small: 5,000 tokens of comments read in 120 turns is about 600,000 cache-read tokens, some $0.12 a run at Opus 5.5 API prices; halving it saves about 1% [I]. The risk is a lost clarification from an early comment; the mark tells the agent something was cut.

**Experiment.** No card split needed for the saving: the characters left out are exact. For quality, a two-week period at 5 comments of 1,500 characters, watching questions (`needs_decision`) and QA fails that name a missed instruction.

### 4.7 Short messages between tool calls

**What it does.** With the switch on, each CLI is asked for terser text in its own way, and nothing rewrites an answer afterwards. Claude Code gets its built-in **Concise** output style: "Responses lead with the result and leave out preamble, narration, and recaps"; GizAi already gives every run a `--settings` JSON, and `outputStyle` would go in it [V for the style and the setting; I that the two combine, to check]. Codex gets `model_verbosity = "low"` ("Optional GPT-5 Responses API verbosity override"). Any other CLI gets one line in "How this run works": "Between tool calls, write at most one short sentence. Code, commands, file paths, error messages and your final summary and hand-over stay complete and exact." This is Caveman's "says less" for the part of an agent's output that nobody hands over. [I]

**The setting.** Settings → Tokens: **Short messages**, off by default: the saving is small and the narration is how the user follows a run live. A user who doesn't watch runs can switch it on.

**What the user sees.** The run reads terser; Usage → Savings shows output tokens per turn per agent, before and after.

**Saving and risk.** Text between tool calls is a small part of an agentic run's output; thinking and tool inputs are most of it. 1–3% [I]. The risk is a shorter hand-over: the Concise style leaves out "recaps", and a hand-over is one. The role instructions ask for a complete summary in the result line, and the experiment checks the hand-overs' length and QA's pass rate.

**Experiment.** Card parity, two weeks: output tokens per turn, hand-over length, QA pass rate.

### 4.8 Leave room in the subscription's windows

**What it does.** Gizai already reads each CLI's 5-hour and weekly windows (§2.7). With this value set, agents start no new run from an Auto column while a window of their CLI is above it; Run by hand still works, as with Pause all agents (`src/pages/SettingsPage.tsx:113`). It saves no tokens; it keeps a freelancer's own Claude Code or Codex usable during the day. [V for the readings; I for the design]

**The setting.** Settings → Tokens: **Start no new runs above** 50–100% of a window, default 90% (100% = today).

**What the user sees.** On the board, a waiting card says "Waiting: Claude Code's 5-hour window is at 92%, resets 15:00"; Usage → Subscription marks the line.

**Risk.** None to quality; cards wait longer.

### 4.9 Not proposed now

- **Continuing the builder's session after a QA fail** instead of a fresh one with QA's issues (`runs.rs:918`). The session knows what it did, but QA's run usually takes longer than the cache's hour, so the whole old context would be written again at 2×; a fresh run with the issues is often cheaper. An experiment after M1 [I].
- **MCP servers per card.** Task runs get only the agent's own servers (agent form → Tools), and Claude Code already keeps their tools out of the context until they are used: "MCP tool definitions are deferred by default, so only tool names and server instructions enter context until Claude uses a specific tool" (§3.6). Little left to save [I].
- **A model for subagents.** Claude Code's subagents (the Task and Agent tools, always on: `crates/gizai-agents/src/tool_catalog.rs:80-81`) can be pointed at a cheaper model with `CLAUDE_CODE_SUBAGENT_MODEL` (§3.6). How much agents use them is not recorded today; M1 records it (Claude Code's `modelUsage` per model), and then it can be a field in the agent form (§9, question 11).
- **Sharing Claude Code's cache between worktrees.** Claude Code's cache "is effectively scoped to one machine and directory", so each card's worktree builds its own, and every run starts by writing Claude Code's system prompt and tools to the cache. Its `--exclude-dynamic-system-prompt-sections` flag "improves prompt-cache reuse across different users and machines"; whether it lets runs in different worktrees share that prefix, and what the agent then no longer sees at the start, needs a check on the CLI. At about 20,000 tokens a run it would save some $0.15 a run on Opus 5.5 [I]. A small card after M1, if the check is good.
- **Reordering GizAi's prompt for the cache.** For the same reason (one cache per worktree), putting the parts an agent's runs share first saves nothing across runs [V for the scope; I for the conclusion].

## 5. Quality: how we'd know it holds, and the experiments

### 5.1 What GizAi can already measure

Quality signals GizAi records today, per card and per run, with no change [V]:

| Signal | Where it is | What it says |
|---|---|---|
| QA pass rate | `runs.outcome` = `qa_pass` or `qa_fail` (`crates/gizai-core/migrations/0008_board_check.sql:31`) | how often a builder's work passes QA the first time |
| Rounds per card | `tasks.bounce_count`, "QA fails; 3 = hold" (`migrations/0001_init.sql:252`; `crates/gizai-core/src/workflow.rs:482-483`) | how many times a card went back to To do |
| Failed runs | `runs.status` `failed`, `timed_out`; `runs.outcome` `error`, `no_result`; `tasks.fail_count` (`0001_init.sql:253`; `workflow.rs:509-510`) | runs that broke or ended without a result line |
| Questions | `runs.outcome` = `needs_decision` | runs that stopped to ask; more of them after a cut can mean missing context |
| Review | cards sent back from Review, and pull requests that needed changes after QA passed | what the user and CI catch that QA didn't |

What it can't measure yet, and card M1 adds (§8): the split of input into fresh, cache writes and cache reads, the turns, the largest context of a run, the effort and the efficiency settings each run had. The share of the 5-hour and weekly windows a single run used can only be estimated: Claude Code reports a window's use when it changes, for the whole account, while several runs share it (`crates/gizai-core/src/limits.rs:5-9`) [V for the readings; I for the estimate].

### 5.2 How every experiment runs

- **Measure before cutting.** M1 ships first and records a baseline week with today's settings. [I]
- **Alternate by card, not by week.** A setting is on for cards with an even number and off for odd ones (GA-170 on, GA-171 off), so both groups see the same weeks, projects and agents. GizAi records which group each run was in. The user can stop an experiment at any time; it is a switch too. [I]
- **Compare per card, not per run.** The cost of a card is all its runs (builder, QA, fixes after `qa_fail`); a setting that makes runs cheaper but adds a QA round can cost more per card. [I]
- **A guard on quality, decided up front.** A setting becomes a default only when, over at least 30 cards per group, the cost per card falls by the expected amount and: the QA pass rate falls by no more than 10 points, rounds per card rise by no more than 0.3, and failed runs and questions don't rise. Otherwise it stays available but off. [I: the thresholds are a proposal for the user to set; 30 cards per group shows a 25–30% cost difference, not a 5% one]
- **Small, by design.** With about five cards a day, a two-week experiment gives some 35 cards per group. That is enough to see the big levers (effort, model), not the small ones (comments, memory): for those, the saving is measured exactly from the characters cut, and only the quality guard needs the cards. [I]

The experiment for each idea is under the idea in §4; card T9 builds the parity split and Usage → Savings shows it (§7.1).

## 6. Options

Three ways to build "fewer tokens without losing quality" into GizAi. All three keep Jefsev's rule: each saving is a setting the user switches or sets.

**Option A: native, in GizAi (recommended).** GizAi already writes the prompt, chooses the CLI's flags and environment, sets each agent's model and effort, and reads every event of the run. The savings are made there: effort and model per card size, a stable Team Lead prompt, board checks on their own model and effort, a cap on a run's context through the CLIs' own compaction settings, a small output filter of its own that agents run their tests and builds through (`gizai-quiet`, full log kept with the run), each CLI's own terser mode, and ranges for comments and memory (§4). Plus what makes it visible and testable: per-run token detail, Usage per card, a Savings view and card-parity experiments.
- Libraries: none new for the app. The filter is a small Rust binary shipped next to Gizai like `gizai-mcp` (`crates/gizai-mcp`), with its own parsers for the formats this user's projects use (cargo, Vitest, tsc, ESLint, PHPUnit and Pest, Composer and npm), and a generic rule for the rest. Rules may be borrowed from RTK or Caveman under Apache-2.0 with a NOTICE line (§3.5). [I]
- Risks: a filter bug that hides an error (mitigated: exit code and counts always shown, every error-like line kept, full log one command away, a fidelity test suite on recorded outputs); agents that forget to use the filter (measured: GizAi sees each command in the stream); lower effort that costs quality (guarded by the experiments of §5 and by raising effort after a QA fail). [I]
- Size: thirteen cards (§8), each one agent run: M1 first, then six that can be built side by side. [I]

**Option B: bundle an existing filter (RTK) and its Claude Code hook.** RTK ("CLI proxy that reduces LLM token consumption by 60-90% on common dev commands. Single Rust binary, zero dependencies") is Apache-2.0, has 83,286 stars and 1,520 open issues, and was pushed to today (GitHub API, 2026-10-14). Its README says it hooks into Claude Code with a "PreToolUse hook (native binary)" and into Codex with a "PreToolUse hook (`updatedInput`) + AGENTS.md", both of which rewrite the agent's commands; it keeps "Failures only, passing tests collapsed to a count" for `cargo test` and `npm test`, and shows savings with `rtk gain`; it says nothing about keeping the full output. [V, its README and the GitHub API]
- What it takes: shipping or asking the user to install a second fast-moving binary, turning hooks back on for headless runs, which GizAi turns off on purpose today (`--settings {"disableAllHooks":true}`, `crates/gizai-agents/src/claude.rs:39`, `:93-95`; `cli.rs:181-184`), and adding a way to keep the full output.
- Risk: the user's own hooks and plugins would run in agents' runs again unless GizAi also stops loading user settings; a rewrite the agent never sees is hard to show in the run. Only the "reads less" part. [I]
- Size: smaller than A for the filter itself, plus the same measurement cards.

**Option C: put Caveman (or another proxy) between the CLIs and the model provider.** *(Its runtime, what it changes, and the terms for subscription logins: §3.)* It would bring all three of Caveman's ways at once, including routing. What it takes: every request of every agent passes through a third-party local service; GizAi can't show per-setting control or savings except through Caveman's own numbers; and routing would override the model the user chose in each agent's form. [I]

**Recommendation: A.** It is the only option that works the same for Claude Code and Codex on a subscription login without touching the request path, keeps each saving a GizAi setting with GizAi's own "what it did" in the run and on Usage, and starts with the measurement that tells whether quality holds. B's filter rules are worth reading (and borrowing with attribution) when building `gizai-quiet`. C's terms risk alone rules it out for a product that runs on the user's own subscription (§3.4). [I]

## 7. The design of the recommended option

Everything below is local and free: it lives in the desktop app, which stays free (`docs/PAID-FEATURES.md:30-37`), and nothing new sits between the CLIs and Anthropic or OpenAI. [V for the plan; I for the design]

### 7.1 The screens

**Settings → Tokens**, a new tab after "Agents and runs" (`src/router.ts:15`; `src/pages/SettingsPage.tsx:30-33`), built like the Runs section: `FormSection` and `Field`, number inputs with `min`/`max` and the range in the hint (`SettingsPage.tsx:105-118`). Each field's hint says what it did in the last 30 days, linking to Usage → Savings.

```
Settings   [General] [Appearance] [Notifications] [Agents and runs] [Tokens] [MCP servers] [GitHub and Bitbucket]

Tokens     Every saving is yours to switch. Usage → Savings shows what each one did.
  Read less
    Shorten test and build output     [x]        Last 30 days: 4.1 M characters kept out (about $38 at API prices)
    Lines kept around a failure       [ 40 ]     10 to 200
  Context
    Compact a run's context at        [      ]   tokens, 100,000 to 1,000,000; empty: the CLI's own
  Prompts
    Comments per run                  [ 10 ]     0 to 20
    Characters per comment            [ 2000 ]   500 to 8,000
    Memory per run (characters)       [ 6000 ]   1,000 to 12,000
    List the other notes              [x]
  Effort
    Raise effort after a QA fail      [x]        A builder's next run goes one card size up
  Team Lead
    Keep the Team Lead's prompt stable [x]       Fewer cache writes in chat
  Messages
    Short messages                    [ ]        One short sentence between tool calls; hand-overs stay complete
  Subscription
    Start no new runs above           [ 90 ] %   50 to 100 of a 5-hour or weekly window
  Compare
    Try a setting on half the cards   [ Shorten test and build output v ] [Start]
                                      Even cards on, odd cards off, until you stop it. Results: Usage → Savings
```

**The card's Size**, a Properties row above Testing (`src/components/Properties.tsx:56-57`), with what it means for the card's agent:

```
Size      [ Small v ]   Backend Agent: Sonnet 5.5, effort medium
Testing   [x] Test before Review
```

**The agent form**: under Model and Effort (`src/components/AgentForm.tsx:243-266`), the two size rows; in the Team Lead's Chat section, beside the board check minutes (`:274-282`), its model and effort.

```
Model         [ Opus 5.5 v ]      Effort [ high v ]       Normal cards
Small cards   [ Sonnet 5.5 v ]    Effort [ medium v ]     empty: as Normal
Large cards   [            v ]    Effort [ xhigh v ]
Board check   [x] every [ 30 ] minutes    Model [ the Team Lead's v ]   Effort [ medium v ]
```

**Usage → Cards and Usage → Savings**, two new tabs (`src/pages/UsagePage.tsx:24-26`), with the same tables as Agents and Projects (`UsageTable`, `:103-149`). Total gets the split of input into new, cache written and cache read; Agents splits the Team Lead into Chat and Board checks.

```
Cards     Card    Title                      Size    Runs  Rounds  QA     In / out        Cost
          GA-170  Hover on the sidebar ...   Normal  3     1       pass   4.1 M / 92 k    $14.20

Savings   Setting                          Runs  Kept out of the context      About, at API prices
          Shorten test and build output    41    5.2 M characters             $46
          Comments and memory              120   0.9 M characters             $7
          Experiment: Shorten output       on: 18 cards, $9.10 a card, QA pass 81%, 1.2 rounds
          (since 3 Nov)                    off: 17 cards, $10.40 a card, QA pass 79%, 1.3 rounds
```

**A run's facts** (`src/components/RunHistory.tsx:75-93`): the Tokens row gets the split and the turns, and two rows say what the settings did.

```
Tokens      2.1 M in (1.9 M cache read, 160 k cache written, 12 k new), 48 k out · 87 turns
Settings    Size Normal · Opus 5.5, effort high · comments 10 of 14 (2 cut) · memory 6,000 of 8,400 characters
Shortened   4 commands: 212,000 → 9,800 characters   [Open the full logs]
```

While a run is live, a shortened command and a cut comment are also Gizai's notes in its stream (`gizai_note` lines, `crates/gizai-agents/src/cli.rs:348-350`, shown by `src/components/RunPanel.tsx:34`), like today's notes about folders and tools.

### 7.2 The data

- `runs` (migration `0018_tokens.sql`): fill the existing `cache_read_tokens` and `cache_write_tokens` (§2.7) from Claude Code's `usage` (`cache_read_input_tokens`, `cache_creation_input_tokens`) and Codex's `cached_input_tokens`; `input_tokens` stays the sum, so today's Usage sums don't change. New: `turns INTEGER`, `effort TEXT`, `size TEXT`, `token_json TEXT` (the settings the run had and what they did, §7.3), `experiment TEXT` (`"<setting>:on"` or `"<setting>:off"`).
- `tasks`: `size TEXT NOT NULL DEFAULT 'normal' CHECK (size IN ('small','normal','large'))`.
- `agent_configs` (where model and effort live: `crates/gizai-core/migrations/0004_effort.sql:2`; `crates/gizai-core/src/team.rs:112-116`): `small_model`, `small_effort`, `large_model`, `large_effort`, `check_model`, `check_effort`, all `TEXT`, null = as Normal (or as the Team Lead's).
- The settings, in the key-value `settings` table (`crates/gizai-core/src/settings.rs:8-30`) under one key, `tokens`, read with defaults like the run limits (`src-tauri/src/runs.rs:216-232`) and checked like them (`:234-272`).
- Full outputs of shortened commands: `<data>/runs/<run-id>/outputs/`, copied from the worktree's `.gizai-tmp/outputs/` before Gizai empties it at the end of the run (`src-tauri/src/runs.rs:1147`; `crates/gizai-agents/src/worktree.rs:256-259`), and removed with the run's log.

### 7.3 The contract between backend and frontend

Word for word, for the cards in §8 (TypeScript in `src/types.ts`, Rust structs with `#[serde(rename_all = "camelCase")]`):

```ts
/** Settings → Tokens. Ranges are checked by save_settings; out of range is an error, as for the run limits. */
export type TokenSettings = {
  shortenOutput: boolean;      // default true
  failureLines: number;        // 10–200, default 40
  compactAtTokens: number | null; // 100000–1000000, default null (the CLI's own)
  commentsPerRun: number;      // 0–20, default 10
  commentChars: number;        // 500–8000, default 2000
  memoryChars: number;         // 1000–12000, default 6000
  listNotes: boolean;          // default true
  stableLeadPrompt: boolean;   // default true
  raiseAfterQaFail: boolean;   // default true
  shortMessages: boolean;      // default false
  windowCap: number;           // 50–100 (percent), default 90; 100 = no cap
  experiment: TokenExperiment | null; // default null
};
export type TokenSetting = "shortenOutput" | "shortMessages" | "comments" | "effortHigh" | "compactAt";
export type TokenExperiment = { setting: TokenSetting; startedAt: number };
export type CardSize = "small" | "normal" | "large";

/** Settings gets one more field: */          // export type Settings = { …; tokens: TokenSettings }
/** TaskPatch gets: */                        // size: CardSize
/** Task gets: */                             // size: CardSize
/** AgentInput and Member get (null = as Normal; check*: null = the Team Lead's own): */
//   smallModel?: string | null; smallEffort?: string | null; largeModel?: string | null; largeEffort?: string | null;
//   checkModel?: string | null; checkEffort?: string | null;

/** Run gets: */
//   cacheReadTokens: number; cacheWriteTokens: number; turns?: number | null; effort?: string | null;
//   size?: CardSize | null; tokenFacts?: TokenFacts | null; experiment?: string | null;
export type TokenFacts = {
  comments: { given: number; total: number; cut: number };
  memory: { givenChars: number; totalChars: number };
  shortened: { command: string; fullChars: number; shownChars: number; log: string }[];
  unshortened: number;         // test or build commands the agent ran without gizai-quiet
  peakContextTokens: number;   // the largest input of one request in the run (new + cache written + cache read)
  compactions: number;
  sizeReason?: "card" | "raisedAfterQaFail" | null;
  models: { model: string; inputTokens: number; outputTokens: number; costUsdMicros: number }[]; // Claude Code's modelUsage
};

/** Usage: UsageTotals gets cacheReadTokens and cacheWriteTokens; Usage gets checks (the board checks, apart from chat). */
export type CardUsage = { taskId: string; identifier: string; title: string; size: CardSize; runs: number; rounds: number;
  qa: "pass" | "fail" | null; totals: UsageTotals };
export type SavingsLine = { setting: TokenSetting | "commentsMemory"; runs: number; keptOutChars: number; aboutUsdMicros: number };
export type ExperimentGroup = { cards: number; costPerCardUsdMicros: number; qaPassRate: number | null; roundsPerCard: number;
  failedRuns: number; questions: number };
export type Savings = { lines: SavingsLine[]; experiment: { setting: TokenSetting; startedAt: number; on: ExperimentGroup;
  off: ExperimentGroup } | null };
```

Commands (`src/api.ts`, next to `usageSummary` at `:158`): `usage_cards(period: UsagePeriod) -> CardUsage[]`, `usage_savings(period: UsagePeriod) -> Savings`, `run_outputs(runId: string) -> { name: string; path: string; chars: number }[]` and `open_run_output(runId: string, name: string)`. Settings, tasks and agents go through today's `save_settings`, `update_task` and the agent save.

### 7.4 How a saving is counted

- **Exact:** characters kept out of a prompt (comments, memory) and out of the context by `gizai-quiet`: what the CLI would have shown, minus what was printed. For Claude Code that is the whole output up to about 30,000 characters, or a file path and 2,000-character preview past that, when the command succeeded, and up to about 10,000 characters when it failed (§2.4, §3.6).
- **About, in tokens:** characters ÷ 4, labelled "about".
- **About, at API prices:** what those tokens would have cost in this run: one cache write at the run's model's price, plus one cache read for every turn after the command (Gizai has the turn of each tool call from the stream). Labelled "at API prices, not a bill"; on a subscription, it is part of a window the run didn't use.
- **Effort, model and size:** no per-run counterfactual. Usage → Savings shows cost per card per size and effort, and, during an experiment, the two groups side by side with QA's pass rate, rounds, failed runs and questions (§5.2).

### 7.5 Security and privacy

- No proxy and no change to the CLIs' login, base URL or requests; nothing new leaves the computer.
- `gizai-quiet` never widens what an agent may run: its allowed rules are made from the agent's own (`Bash(gizai-quiet cargo:*)` only when `Bash(cargo:*)` is allowed), it starts the command directly (no shell, so no injection through its arguments), and it writes only in the run's temp folder. The full logs are kept like the run logs, readable only by the user, and may hold whatever the tests print, as run logs do today.
- The Team Lead's stable prompt carries the same information as today; nothing is hidden from it.
- An experiment is visible on every run it touches (its group in the run's facts) and stops with one click.

### 7.6 How it is tested

- `gizai-quiet`: recorded outputs of each tool it knows, passing and failing (cargo test and build, Vitest, `tsc`, ESLint, `npm run build`, PHPUnit and Pest, Composer), checked line by line: every failure and error line kept, counts right, exit code passed through, an unknown output shown as its first and last lines and never as nothing.
- The prompt: comments and memory at the edges of their ranges, the cut mark, QA's issues once; "How this run works" with and without Short messages and `gizai-quiet` (`crates/gizai-agents/src/prompt.rs`'s tests).
- Size: the model and effort a run gets for each size, the raise after `qa_fail`, the Team Lead's line; the board check's own model and effort.
- The Team Lead's prompt: the same system prompt across a note's change, and the bracket line naming the changed notes.
- Usage: the split from Claude Code's result line (`crates/gizai-agents/tests/fixtures/run-refused.jsonl:21` has every field) and from a Codex `turn.completed`; usage per card and per trigger; savings from recorded runs.
- Flow tests with the fake Claude Code scripts (CLAUDE.md:44), UI tests for the new tab, rows and Usage tabs.

### 7.7 Left out

No proxy, no automatic router, no rewriting of a model's answers, no Caveman or RTK install, no change to Claude Code's cache or compaction, and no cloud analytics.

## 8. Proposed cards

In order. Each is one pull request that fits in one agent run; the contract they share is §7.3, word for word. A card starting in Design gets its screens from the Design Agent first; every card goes to Testing (QA) as usual.

**Default for freelancers and small agencies**, the column the card asks for: every setting below is on by default except Short messages, the context cap and an experiment, and every value starts where today's behaviour is, except the effort of board checks and new cards' sizes (Normal changes nothing) and the window cap (§9, questions 2 and 3).

| # | Card | Role | Starts in | Its setting: default, range | Builds on |
|---|---|---|---|---|---|
| M1 | Record where each run's tokens go, and the token settings | backend | To do | none (records only, always on); adds `Settings.tokens` with every default and range of §7.3 | — |
| M2 | Usage per card, the input split, and Chat apart from Board checks | frontend | To do | none | M1 |
| T1 | Keep the Team Lead's prompt stable | backend | To do | Keep the Team Lead's prompt stable: on | M1 |
| T2 | Card size, with model and effort per size, and the board check's own | backend | To do | card Size: Normal; Small and Large rows: empty (as Normal); Raise effort after a QA fail: on; board check effort: `medium` on a new install | M1 |
| T3 | The card's Size and the agent form's size and board check rows | frontend | Design | as T2 | T2 |
| T4 | `gizai-quiet`: shorten test and build output, keep the full log | backend | To do | (used by T5) | — |
| T5 | Runs use `gizai-quiet`, and keep the full logs | backend | To do | Shorten test and build output: on; Lines kept around a failure: 40, 10–200 | M1, T4 |
| T6 | Comments, memory and Short messages as settings | backend | To do | Comments per run 10 (0–20); Characters per comment 2,000 (500–8,000); Memory per run 6,000 (1,000–12,000); List the other notes: on; Short messages: off | M1 |
| T7 | Leave room in the subscription's windows | backend | To do | Start no new runs above 90% (50–100) | M1 |
| T8 | Settings → Tokens | frontend | Design | shows every setting above | M1, T1, T5, T6, T7 |
| T9 | Savings and experiments | backend | To do | Try a setting on half the cards: none running | M1, T5, T6 |
| T10 | Usage → Savings, and what each setting saved | frontend | Design | shows the experiment and the savings | T8, T9 |
| T11 | Cap a run's context | backend | To do | Compact a run's context at: off (the CLI's own), 100,000–1,000,000 tokens | M1, and M1's first weeks of numbers |

**At the same time:** M1 first (it holds the settings every other card reads). Then T1, T2, T4, T6, T7 and M2 can be built side by side: they touch different files (`src-tauri/src/chat.rs` and `memory.rs`; the run start and `team.rs`; a new crate; `prompt.rs`; the scheduler; `UsagePage.tsx`). T5 follows T4, T3 follows T2, T9 follows T5 and T6, and T8 and T10 come last. T11 waits for M1's numbers: build it only if runs grow large contexts (§4.4); its field then joins Settings → Tokens. No setting ships before its field: T1 to T7 reach users in the release that has T8 (and T3 for the size rows).

### M1. Record where each run's tokens go, and the token settings (backend, To do)

Builds: the input split, turns, effort and models of every run and chat turn, and the settings struct the other cards use.
- [ ] Migration `0018_tokens.sql`: `runs.turns`, `runs.effort`, `runs.size`, `runs.token_json`, `runs.experiment`; `cache_read_tokens` and `cache_write_tokens` filled from now on.
- [ ] Claude Code: cache reads and writes saved from the result's `usage`; for `crates/gizai-agents/tests/fixtures/run-refused.jsonl:21`, 604,313 read, 11,139 written, 185 new, and `input_tokens` still their sum.
- [ ] Codex: `cached_input_tokens` of `turn.completed` saved as cache reads (`crates/gizai-agents/src/cli.rs:599-603`).
- [ ] Turns (already parsed, `crates/gizai-agents/src/stream.rs:127`), effort and Claude Code's `modelUsage` per model saved per run, chat turn and board check.
- [ ] The run's largest context (the biggest input of one request, from each assistant message's `usage`) and its compactions saved in `token_json`.
- [ ] `Run`, `UsageTotals` and `Usage` carry the new fields (§7.3); `Usage.checks` holds board checks apart from `chat`; `usage_cards(period)` returns each card's runs, rounds (`tasks.bounce_count`), QA result and totals.
- [ ] `Settings.tokens` (`TokenSettings`, §7.3) with its defaults and ranges, saved by `save_settings` and refused out of range, like the run limits (`src-tauri/src/runs.rs:234-272`).
- [ ] `cargo test --workspace` and `npm test` pass.

### M2. Usage per card, the input split, and Chat apart from Board checks (frontend, To do)

Builds: Usage → Cards; Total's input as new, cache written and cache read; the Team Lead as two lines; a run's Tokens row with the split and turns. It reuses the Usage page's tabs and table, so it needs no design first.
- [ ] Usage → Cards lists each card of the period: identifier (a link), title, size, runs, rounds, QA result, tokens in and out, cost; sorted by cost.
- [ ] Total shows input split three ways, with the cost note as today (`src/pages/UsagePage.tsx:30-42`).
- [ ] Agents shows the Team Lead's Chat and Board checks on two lines.
- [ ] A run's facts: "2.1 M in (1.9 M cache read, 160 k cache written, 12 k new), 48 k out · 87 turns" (`src/components/RunHistory.tsx:80`).
- [ ] UI tests for the tab and the facts.

### T1. Keep the Team Lead's prompt stable (backend, To do)

Builds: §4.2. Setting: **Keep the Team Lead's prompt stable**, switch, on.
- [ ] With it on, a chat thread's system prompt (`src-tauri/src/chat.rs:954-996`) is the same at every turn: the date and the Memory block as at the thread's first turn, the list of other notes without sizes. Only a change in the projects' copies renews it, as today (`chat.rs:957`).
- [ ] The bracket line before a message names the notes added, removed or changed since the thread began (at most ten, then "and N more").
- [ ] Off: today's prompt, word for word.
- [ ] Board checks unchanged.
- [ ] Tests: a note's size change, a note the Team Lead writes, and a new day leave the system prompt unchanged when on, and change it when off.

### T2. Card size, with model and effort per size, and the board check's own (backend, To do)

Builds: §4.1 and §4.3. Settings: card **Size** (Small · Normal · Large, default Normal); agent **Small cards** and **Large cards** model and effort (default empty, as Normal); **Raise effort after a QA fail** (on); **Board check model** (default the Team Lead's) and **effort** (`medium` on a new install).
- [ ] `tasks.size` and the six `agent_configs` columns (§7.2); `Task`, `TaskPatch`, `AgentInput` and `Member` carry them; the effort is checked against the CLI's levels like today's (`crates/gizai-core/src/team.rs:319-321`).
- [ ] A run's model and effort come from its card's size and the agent's rows, on Claude Code (`--model`, `--effort`) and Codex (`-m`, `model_reasoning_effort`), and are recorded on the run with why ("card" or "raisedAfterQaFail").
- [ ] With Raise on, a builder's first run after `qa_fail` goes one size up; Large stays Large.
- [ ] Board checks use the check model and effort when set (`src-tauri/src/chat.rs:1456`, `:1461`).
- [ ] Gizai's task tools for the Team Lead take and return `size`; the lead's template (`crates/gizai-core/src/seed.rs:198-215`) gets one line: "Give each card a size: small for a small fix, large for a hard or wide change, normal otherwise."
- [ ] Seed: a new install's builders get Small one effort level below Normal.

### T3. The card's Size and the agent form's size and board check rows (frontend, Design)

Builds: §7.1's Size row and agent form rows.
- [ ] Properties has Size above Testing, and says what it means for the card's agent ("Backend Agent: Sonnet 5.5, effort medium").
- [ ] The agent form has Small cards and Large cards rows (model and effort, empty = as Normal), with the levels the form offers for each model; the Team Lead's Chat section has Board check model and effort.
- [ ] Read-only cards and agents show them read-only.
- [ ] UI tests.

### T4. `gizai-quiet`: shorten test and build output, keep the full log (backend, To do)

Builds: the command of §4.5, its own crate next to `crates/gizai-mcp`, shipped next to Gizai like `gizai-mcp` (`install.sh:3`; `src-tauri/src/mcp.rs:65`).
- [ ] `gizai-quiet <command> [args…]` starts the command without a shell, writes the whole output to `$TMPDIR/outputs/<nn>-<command>.log`, and passes on its exit code.
- [ ] It prints the command, exit code, time, counts, every failure with its message and up to `GIZAI_QUIET_LINES` lines around it (default 40), every error and warning line, and the full log's path and size.
- [ ] Recognises cargo test and build, Vitest and Jest, `tsc`, ESLint, `npm run build`, PHPUnit and Pest, Composer; anything else as its first and last lines, never as nothing.
- [ ] Tests on recorded outputs, passing and failing, for each format.
- [ ] If a rule is taken from RTK or Caveman, the file says so and NOTICE gets their line (§3.5).

### T5. Runs use `gizai-quiet`, and keep the full logs (backend, To do)

Builds: §4.5 in the runs. Settings: **Shorten test and build output** (on), **Lines kept around a failure** (40, 10–200).
- [ ] With it on, each `Bash(X:*)` in a Claude Code agent's allowed commands gets `Bash(gizai-quiet X:*)` beside it, never more; the run finds `gizai-quiet` (on its PATH or by full path, both tested on Claude Code and Codex).
- [ ] "How this run works" gets one line: run tests and builds through `gizai-quiet`, and read the full log with `rg` or `tail` when needed.
- [ ] The run's `outputs/` is copied to `<data>/runs/<run-id>/outputs/` before the temp folder is emptied; `run_outputs` and `open_run_output` (§7.3).
- [ ] `TokenFacts.shortened` and `unshortened` saved; a Gizai note in the stream for each shortened command.
- [ ] Off: no rules, no line, as today.

### T6. Comments, memory and Short messages as settings (backend, To do)

Builds: §4.6 and §4.7. Settings: **Comments per run** (10, 0–20), **Characters per comment** (2,000, 500–8,000), **Memory per run** (6,000, 1,000–12,000), **List the other notes** (on), **Short messages** (off).
- [ ] The prompt takes the last N comments, each cut at the setting with the mark "… (cut at 2,000 of 5,400 characters; the full comment is on the card)" (`crates/gizai-agents/src/prompt.rs:37`, `:78-84`; `src-tauri/src/runs.rs:912-913`).
- [ ] QA's latest comment is left out of a builder's prompt when its issues are in "QA found these issues last time" (`prompt.rs:71-77`).
- [ ] Memory's caps come from the setting (own notes half of it, as `OWN_CAP` is now, `crates/gizai-core/src/memory.rs:30-35`); the list only with List the other notes on.
- [ ] Short messages on gives a Claude Code run the Concise output style (`"outputStyle":"Concise"` in the `--settings` JSON it already gets, `crates/gizai-agents/src/claude.rs:93-95`), a Codex run `-c model_verbosity="low"`, and any other CLI §4.7's line in "How this run works" (`prompt.rs:297-353`); each checked on the CLI's current version first.
- [ ] `TokenFacts.comments` and `memory` saved; tests at each range's edges.

### T7. Leave room in the subscription's windows (backend, To do)

Builds: §4.8. Setting: **Start no new runs above**, 90% (50–100).
- [ ] No automatic start for an agent while the newest reading of its CLI account's 5-hour or weekly window (`crates/gizai-core/src/limits.rs`) is above the setting and not yet reset; Run by hand still starts.
- [ ] The waiting card says which window, its percentage and when it resets.
- [ ] 100 = today's behaviour. Tests with readings above, below and after a reset.

### T8. Settings → Tokens (frontend, Design)

Builds: the tab of §7.1, every field of `TokenSettings` but `experiment` (T10), saved with Save settings.
- [ ] A Tokens tab after Agents and runs, address `#/settings/tokens`.
- [ ] Each field with its range in the hint; an out-of-range value says so, as the run limits do.
- [ ] `docs/settings.md` gets the tab.
- [ ] UI tests.

### T9. Savings and experiments (backend, To do)

Builds: §5.2's card parity and §7.4's counting.
- [ ] Starting an experiment saves `tokens.experiment`; every run of a card is put in its group by the card's number (even on, odd off) and records it (`runs.experiment`); stopping clears it.
- [ ] `usage_savings(period)`: per setting, runs, characters kept out and their price at API prices (one cache write plus a read per later turn, at the run's model's prices), and the two groups' cost per card, QA pass rate, rounds, failed runs and questions.
- [ ] Tests on recorded runs.

### T10. Usage → Savings, and what each setting saved (frontend, Design)

Builds: Usage → Savings (§7.1), Start and Stop under Settings → Tokens → Compare, and the "last 30 days" line under each setting.
- [ ] The tab shows each setting's line and, during an experiment, the two groups side by side.
- [ ] Only one experiment at a time; Stop asks once.
- [ ] UI tests.

### T11. Cap a run's context (backend, To do; after M1's numbers)

Builds: §4.4. Setting: **Compact a run's context at**, off (the CLI's own), 100,000–1,000,000 tokens.
- [ ] With a value set, a Claude Code task run gets `CLAUDE_CODE_AUTO_COMPACT_WINDOW=<n>` in its environment and a Codex run `-c model_auto_compact_token_limit=<n>`; chat and board checks keep the CLI's own.
- [ ] Each compaction is a Gizai note in the run's stream and counts in `TokenFacts.compactions`.
- [ ] Empty: nothing is set, as today. Tests for both CLIs' arguments and the range.

## 9. Open questions

1. **This team's effort, now.** Most agents run at `max` or `xhigh`. Run the first experiment on it (even cards at `high`, odd cards as now, two weeks, after M1's baseline week)? *Recommendation: yes; it is the biggest lever and needs no new feature but M1 and T9, and until T9 the parity can be set by hand on two agents.*
2. **Board checks' effort on existing installs.** T2 proposes `medium` for a new install. For an existing Team Lead, set `medium` too, or leave it empty (the Team Lead's own) with a hint in the form? *Recommendation: leave it empty and show the hint; Jefsev's rule is that the user decides, and an upgrade shouldn't change how a check thinks.*
3. **The window cap's default.** 90% for a new install. For existing installs too, or 100% (today) until the user sets it? *Recommendation: 90% for everyone, said in the release notes: it only makes cards wait, the card says why, and it protects a freelancer's own use of the same subscription.*
4. **Short messages off by default?** *Recommendation: off; the saving is small and the narration is how you follow a run. Revisit after its experiment.*
5. **The quality guard (§5.2):** a setting becomes a default only if QA's pass rate falls by no more than 10 points, rounds per card rise by no more than 0.3, failed runs and questions don't rise, over at least 30 cards per group. Are these your thresholds? *Recommendation: accept them for the first experiment and tighten them once M1 shows the normal spread.*
6. **Should Size also set Testing** (Small → Testing off)? *Recommendation: no; keep the two apart. The Team Lead sets both when it writes a card, and you see both on the card.*
7. **Who sets the size?** *Recommendation: the Team Lead sets it when it writes or splits a card, and you change it like any property; the card's history shows who changed it.*
8. **Borrow filter rules from RTK or Caveman, or write them?** Both are Apache-2.0 (§3.5). *Recommendation: write `gizai-quiet`'s parsers for the formats GizAi's users run, and borrow a rule only where it is clearly better, with the file's attribution and a NOTICE line.*
9. **A proxy in the request path (Caveman's runtime or similar):** confirm it stays out. *Recommendation: yes, out (§3.4).*
10. **Experiments by card parity** run half the cards with a setting off for two weeks. Acceptable on client work? *Recommendation: yes, one setting at a time, with Stop always one click away; or limit an experiment to one project.*
11. **Subagents' model.** After a month of M1 data, add a "Subagents" model to the agent form if agents use subagents a lot? *Recommendation: decide on M1's numbers (Claude Code's `modelUsage` per run).*
12. **An `ANTHROPIC_API_KEY` in Gizai's environment** makes every Claude Code run bill the API instead of the plan (§3.4). Warn about it, or keep it out of runs? *Recommendation: a warning in Settings → Coding CLIs and in the run's facts when the variable is set, now (a small card); a switch "Runs use my plan, not an API key from my environment" only if users ask, since some want API billing.*
13. **Max plans' monthly API credits** (§3.4): Anthropic's two pages disagree on whether they cover `claude -p`. *Recommendation: don't build on them until the credits page says so; then a second Claude Code account on an API key (Settings → Coding CLIs already takes more accounts) could run board checks or DevOps on the credits.*

## 10. Sources

All read on 2026-10-14. Quotes from code.claude.com's and platform.claude.com's `.md` pages are word for word; quotes from other pages came through a fetch tool asked to quote word for word, and are marked where only a copy or a secondary source could be read.

**GizAi** (main at `24535e3`): the files named in the text, with their lines.

**caveman.so and RTK**
- caveman.so: https://caveman.so , https://caveman.so/caveman , https://caveman.so/pricing ; adapters: https://docs.caveman.so/caveman/middleware.md
- Repository data: https://api.github.com/repos/JuliusBrussee/caveman
- Repository files at commit `d262ab3` (tag `v4.0.1`), each at https://github.com/JuliusBrussee/caveman/blob/d262ab3/ followed by its path: `README.md`, `LICENSE`, `LICENSE-MIT`, `LICENSING.md`, `NOTICE`, `SECURITY.md`, `INSTALL.md`, `CLAUDE.md`, `skills/caveman/SKILL.md`, `src/rules/caveman-activate.md`, `src/hooks/README.md`, `proxy/README.md`, `packages/cli/src/native-hook-fast.ts`, `packages/cli/src/modules/provider-logins.ts`, `docs/technical/engine.md`, `docs/technical/context-recovery.md`, `docs/technical/install-and-update.md`, `docs/WRAP-BENCHMARK.md`, `docs/AUTO-BENCHMARK.md`, `docs/HONEST-NUMBERS.md`, `evals/README.md`; and the old README with the 65% table: https://raw.githubusercontent.com/JuliusBrussee/caveman/v1.5.0/README.md
- RTK repository data: https://api.github.com/repos/rtk-ai/rtk ; README: https://raw.githubusercontent.com/rtk-ai/rtk/develop/README.md

**Anthropic: Claude Code docs**
- Legal and compliance: https://code.claude.com/docs/en/legal-and-compliance.md
- LLM gateway, and its protocol guide: https://code.claude.com/docs/en/llm-gateway.md , https://code.claude.com/docs/en/llm-gateway-protocol.md
- Environment variables: https://code.claude.com/docs/en/env-vars.md
- Tools reference (Bash output, Read): https://code.claude.com/docs/en/tools-reference.md
- Hooks: https://code.claude.com/docs/en/hooks.md ; SDK hooks: https://code.claude.com/docs/en/agent-sdk/hooks.md
- Costs: https://code.claude.com/docs/en/costs.md
- Prompt caching in Claude Code: https://code.claude.com/docs/en/prompt-caching.md
- Model configuration (effort, compaction window): https://code.claude.com/docs/en/model-config.md
- How Claude Code works (compaction): https://code.claude.com/docs/en/how-claude-code-works.md
- Output styles: https://code.claude.com/docs/en/output-styles.md
- Subagents: https://code.claude.com/docs/en/sub-agents.md
- MCP: https://code.claude.com/docs/en/mcp.md ; tool search: https://code.claude.com/docs/en/agent-sdk/tool-search.md ; context window: https://code.claude.com/docs/en/context-window.md
- CLI reference: https://code.claude.com/docs/en/cli-reference.md

**Anthropic: API docs, terms and support**
- Pricing: https://platform.claude.com/docs/en/about-claude/pricing
- Effort: https://platform.claude.com/docs/en/build-with-claude/effort
- Prompt caching: https://platform.claude.com/docs/en/build-with-claude/prompt-caching.md
- Context editing: https://platform.claude.com/docs/en/build-with-claude/context-editing
- API credits for Max and Team plans: https://platform.claude.com/docs/en/about-claude/api-credits-for-subscribers.md
- Opus 4.5 launch (effort and SWE-bench): https://www.anthropic.com/news/claude-opus-4-5
- Consumer Terms of Service: https://www.anthropic.com/legal/consumer-terms
- Support: https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan , https://support.claude.com/en/articles/11647753-how-do-usage-and-length-limits-work , https://support.claude.com/en/articles/11049741-what-is-the-max-plan , https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan , https://support.claude.com/en/articles/17154008-monthly-api-credits-for-max-and-team-plans
- Secondary (posts on X by Anthropic staff, read through the api.fxtwitter.com mirror because x.com refused the fetch): https://x.com/trq212/status/2009689809875591565 (2026-01-09), https://x.com/bcherny/status/2040206440556826908 (2026-04-03)

**OpenAI: Codex**
- Authentication: https://learn.chatgpt.com/docs/auth
- Advanced configuration (`openai_base_url`, providers): https://learn.chatgpt.com/docs/config-file/config-advanced
- Configuration reference: https://learn.chatgpt.com/docs/config-file/config-reference ; sample: https://learn.chatgpt.com/docs/config-file/config-sample.md
- Hooks: https://learn.chatgpt.com/docs/hooks
- Sign in with ChatGPT through a gateway: https://learn.chatgpt.com/docs/enterprise/sign-in-with-chatgpt-through-a-gateway
- Pricing and plan limits: https://learn.chatgpt.com/docs/pricing
- DevDay 2026 (ChatGPT sign-in for apps): https://learn.chatgpt.com/docs/whats-new/devday-2026
- Secondary: OpenAI's Terms of Use as copied by https://tostracker.app/document/openai (openai.com answered 403); Codex's output cut as described in the open issue https://github.com/openai/codex/issues/14206

(developers.openai.com/codex pages now redirect to learn.chatgpt.com.)
