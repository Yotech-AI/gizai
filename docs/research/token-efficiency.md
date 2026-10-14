# GizAi: fewer tokens without losing quality, and what caveman.so does (research, 2026-10-14)

Status: research only, no code. Written by the R&D Agent for GA-171 on 2026-10-14, reading `main` at `24535e3` (GizAi 0.11.0).
Legend: **[V]** = verified today, in the code (file and line) or in the primary source named (link and date in §9). **[I]** = my judgement, or a fact I could not check in a primary source (the reason is given).
Limits of this research: the run may not read GizAi's database or run logs (they are real user data), nor start `cargo`, `npm` or any CLI, so no run was measured. The token figures for GizAi's prompt are counted from the code; the dollar figures per agent are the ones Jefsev gave on the card (Claude Code's estimate at API prices, October 1 to 14). Everything that needs a real run is an experiment in §5.

> Draft in progress: sections marked *(to come)* are filled in by later commits of this run.

---

## 1. Summary

*(to come)*

---

## 2. The project now, and where its tokens go

### 2.1 What GizAi does, in one paragraph

GizAi is a local desktop app (Tauri 2, Rust and React) for one developer or a small agency and a team of coding agents. Each card on the board is worked by an agent: GizAi starts the user's own installed CLI (Claude Code, Codex, Gemini or another) headless in the card's own git worktree, with a prompt GizAi writes, and reads the CLI's event stream for the run's log, its result line and its usage (README.md:3-5; `crates/gizai-agents/src/claude.rs:64-125`; `src-tauri/src/runs.rs:1021-1036`). The Team Lead answers in Chat and checks the board, on Claude Code only (`src-tauri/src/chat.rs:1394-1397`). The user pays the CLIs through their own subscription, so what runs out is the subscription's 5-hour and weekly limits, which GizAi reads from the CLIs' own reports and shows on Usage → Subscription (`crates/gizai-core/src/limits.rs:1-16`). [V]

### 2.2 How an agent run spends tokens

A run is a loop of turns: each turn sends the whole conversation so far to the model, and the model answers with thinking, a little text and one or more tool calls; the tool results go into the conversation, and the next turn starts. Three things follow, and they decide where savings are: [V for the cache rules, Anthropic's prompt caching and effort pages; I for the arithmetic]

1. **Everything in the context is read again on every later turn.** Claude Code caches the conversation, so a repeated read is a *cache read*, at 0.1× the input price; what is new in a turn is a *cache write*, at 1.25× (5-minute cache) or 2× (1-hour cache) the input price. "Cache prefixes are created in the following order: `tools`, `system`, then `messages`." So 10,000 tokens of test output read at turn 30 of a 150-turn run cost one cache write and 120 cache reads: about 14× their size in input-price terms. A token is cheapest when it never enters the context.
2. **A change early in the prefix makes everything after it new again.** A system prompt that differs from the last turn's turns the whole conversation after it into cache writes. This matters for the Team Lead's chat (§2.5).
3. **Output costs 5× input, and effort decides how much output there is.** Anthropic's effort page: "The effort parameter affects **all tokens** in the response", thinking, text and tool calls, and "Lower effort also means fewer and terser tool calls." On Opus 4.5 and later "thinking blocks are preserved by default", so thinking also stays in the context and is read again on later turns.

Prices, from Claude Code's own model list as GizAi's tests hold it (`src/lib/models.test.ts:7-10`): Opus 5.5 $4 in / $20 out per million tokens, Sonnet 5.5 $2 / $10, Haiku 4.5 $1 / $5. *(The pricing page check follows in §3.)* [V for the test fixture]

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

What this means [I]: the prompt itself is not where most tokens go. Even at 8,000 tokens, a 150-turn run reads it back about 150 times, 1.2 million cache-read tokens, which is the price of about 120,000 fresh input tokens: some $0.50 at Opus 5.5's input price. It counts, but the context that grows during the run (tool results, file reads, thinking) is read back the same way and is usually far bigger.

**One real run, measured.** GizAi's tests keep the event stream of a real Claude Code 2.1.289 run (Haiku 4.5, the GA-48 shell-rules check) as a fixture. Its last line says (`crates/gizai-agents/tests/fixtures/run-refused.jsonl:21`): [V]

| | Tokens | Price (Haiku 4.5) | Share of the $0.0944 |
|---|---|---|---|
| Fresh input | 185 | $1 / M | 0.2% |
| Cache writes, all with the **1-hour** lifetime (`ephemeral_1h_input_tokens: 11139`) | 11,139 | $2 / M (2×) | 24% |
| Cache reads | 604,313 | $0.10 / M (0.1×) | 64% |
| Output (832 of it thinking) | 2,300 | $5 / M | 12% |

The shares add up to Claude Code's own `total_cost_usd` of $0.09439 to the cent [V, arithmetic]. The run had 23 turns of tiny tool results (`git status`, `ls`), and its last request was 29,292 tokens, nearly all from the cache: so Claude Code's own system prompt and tools plus GizAi's prompt were already some 25,000–29,000 tokens before any work [I]. Two lessons: on a subscription, Claude Code writes its cache with the 1-hour lifetime (2× input), so a cache miss is expensive; and in a real run nearly all input tokens are cache reads, turn after turn, of a context that only grows.

### 2.4 What agents read and write during a run

- **Test and build output.** The project tells every builder to run, before it says it is done, `cargo test --workspace`, `npm test`, `npm run build` and `scripts/ui-test.sh` when the UI changed (CLAUDE.md:39-44), and QA runs the whole suite twice, "once to see where things stand and once more after your new tests" (`crates/gizai-core/roles/qa.md:10`). GizAi's own suites are big: 1,519 Rust tests whose names alone are 105,631 characters (`rg` over `#[test]` functions, test names average 66 characters), and 1,915 `it`/`test` calls in 131 test files. A passing `cargo test --workspace` prints one line per test, so roughly 130,000–150,000 characters (35,000–40,000 tokens), and a fresh worktree first prints a `Compiling` line for each of the 588 crates in `Cargo.lock` [I, estimated from the counts; not run]. Claude Code cuts long command output itself *(its limit and how it cuts: §3.6)*, so what the model reads is less, but whatever it reads stays in the context for the rest of the run.
- **Files, diffs and searches.** Read, Grep and `git diff` results enter the context the same way. GizAi doesn't shape them today; the CLIs' own limits apply *(§3.6)*.
- **Thinking and text.** On Opus 4.5 and later, thinking stays in the context (§2.2), so effort `max` grows the context too, not only the output.

### 2.5 The Team Lead: chat answers and board checks

- **Chat.** "One message is one turn: `claude -p` resumes the thread's session" (`src-tauri/src/chat.rs:1`, `:819-865`). Its system prompt is appended with `--append-system-prompt` (`chat.rs:1191`) and holds the Team Lead's fixed rules (`chat.rs:975-990`), one line per project copy of the code, its folders, its instructions, and **its Memory block**: its own notes in full (up to 6,000 characters) and a list of every other note it may read, each with its size, `- {path} ({n} characters)` (up to 4,000; `chat.rs:993`, `:1000-1003`; `src-tauri/src/memory.rs:19-31`; `crates/gizai-core/src/memory.rs:1848-1956`, the line at `:1946`). [V]
- **The chat's cache breaks more often than the code expects.** The code says the system prompt is "The same from turn to turn (only the projects change it), so the prompt cache keeps working" (`chat.rs:957`). But it also changes when the Team Lead writes to its own notes (its instructions ask it to keep Memory up to date: `crates/gizai-core/src/seed.rs:209`), when **any** note in the list changes size, which happens every time an agent's run adds a `learned` line to its Notes, and at midnight ("Today is {today}", `chat.rs:976`, `:994`). Since the system prompt comes before the messages in the cache's order, each such change makes the next chat turn write the whole conversation to the cache again, at 2× the input price on the 1-hour cache. [V for the code; I for how often it happens and what it costs: GizAi doesn't record cache writes per turn, §2.7]
- **Board checks** run only when the code finds something new (`src-tauri/src/board.rs:113-125`; `crates/gizai-core/src/board.rs:1-9`), each in a fresh session that is never resumed (`chat.rs:1420`, `:1459-1460`), with Gizai's MCP server and Read, Glob and Grep (`chat.rs:1438-1458`), on the Team Lead's own model and effort (`chat.rs:1456`, `:1461`). A check is already lean in what starts it; each one still pays the fixed prefix (Claude Code's prompt and tools, Gizai's tools, the Team Lead's prompt and memory), from the cache only if the same prefix was written within the hour. [V; I for the cost]
- The Team Lead is the team's biggest single agent: $499 of $2,771 in October so far (§2.7).

### 2.6 Models and effort

- Each agent has a model and an effort in its form, and every run and chat turn uses them: `--model` and `--effort` for Claude Code (`crates/gizai-agents/src/claude.rs:47`, `:99-104`), `-m` and `-c model_reasoning_effort=…` for Codex (`crates/gizai-agents/src/cli.rs:214-219`), from `agent.model` and `agent.effort` (`src-tauri/src/runs.rs:1031`). No card sets its own. The form offers the levels Claude Code reports for the model (`src/lib/models.ts:1-17`). [V]
- A card already has one cost switch: **Testing**. "On: a finished run sends the card to Testing for the QA Agent. Off: straight to Review (a small fix)" (`crates/gizai-core/src/model.rs:183-184`). [V]
- Most of this team runs Opus 5.5 at `max` or `xhigh` (the card). Anthropic's effort page says Opus 5.5 **defaults to `medium`**; `xhigh` is for "Long-running agentic and coding tasks (over 30 minutes) with token budgets in the millions"; and for Opus 5.5: "Run an effort sweep on your own evals rather than carrying settings over from an earlier model". Of `max` it says, for Opus 4.7: "On most workloads `max` adds significant cost for relatively small quality gains, and on some structured-output or less intelligence-sensitive tasks it can lead to overthinking." For Sonnet 5.5: "For agentic coding and multistep tool use, start with `medium` for well-specified tasks and move to `high` for harder or longer ones." [V, platform.claude.com effort page]
- Anthropic's own data point for effort, from the Opus 4.5 launch: "Set to a medium effort level, Opus 4.5 matches Sonnet 4.5's best score on SWE-bench Verified, but uses 76% fewer output tokens." [V, anthropic.com]

### 2.7 What GizAi records, and the real numbers

**Per run** (`runs` table, `crates/gizai-core/migrations/0008_board_check.sql:23-50`): the agent, card, trigger (`assigned`, `routed`, `chat`, `manual`, `approval`, `nudge`, `board_check`), role, model, outcome, start and end, input tokens, output tokens and cost. [V]
- Input tokens are fresh input + cache writes + cache reads added together (`crates/gizai-agents/src/stream.rs:116-127`); the `cache_read_tokens` and `cache_write_tokens` columns exist but "stay empty" (`crates/gizai-core/src/usage.rs:7-8`). So GizAi can't tell a cache miss from a cache hit, though the two differ 20× in price. [V]
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

About $198 a day at API prices [V, arithmetic]. Builders and QA are 72% of it, so savings in task runs matter most; the Team Lead is next. How these dollars split into cache reads, cache writes and output can't be read from GizAi today (above); if this team's runs look like the measured one and like typical long agent runs, cache reads are the largest part, then output, then cache writes [I]. Card M1 (§7) records the split, so the first week after it gives the real one.

---

## 3. caveman.so, checked

*(to come)*

## 4. The ideas, ranked, each as a setting

Ranked by what they should save for this team and how much they risk quality. Every saving is a setting the user controls (Jefsev's rule): a switch, or a value within a range, with a default proposed for freelancers and small agencies. "Settings → Tokens" is a new Settings tab (§7.1); "agent form" and "card" are the existing forms. Savings are my estimates from §2 [I]; the experiment under each idea is what turns them into numbers.

| # | Idea | Caveman's way | Expected saving | Risk to quality | Setting, default |
|---|---|---|---|---|---|
| 1 | Effort and model by card size, raised after a QA fail | picks the model | high: 10–20% of a Normal card's runs moved from `max`/`xhigh` to `high`; 30–50% on Small cards | medium, guarded | card **Size** Small · Normal · Large (Normal); agent form: model and effort per size (Normal = today's); switch **Raise effort after a QA fail** (on) |
| 2 | A stable Team Lead prompt, so its chat stays in the cache | (the rest) | medium on the Team Lead: 10–30% of its chat cost | none | switch **Keep the Team Lead's prompt stable** (on) |
| 3 | Board checks on their own model and effort | picks the model | medium on the Team Lead's checks: 30–60% of their cost | low | agent form → Chat: **Board check model** (the Team Lead's) and **effort** (`medium`) |
| 4 | Shorter test and build output, full log kept | reads less | low to medium: 5–10% of a QA or builder run | low, with its safeguards; may improve quality | switch **Shorten test and build output** (on), **Lines kept around a failure** 10–200 (40) |
| 5 | Comments and memory as ranges, cuts marked | (the rest) | small: under 2% of a run | low at the defaults | **Comments per run** 0–20 (10), **Characters per comment** 500–8,000 (2,000), **Memory per run** 1,000–12,000 (6,000), switch **List the other notes** (on) |
| 6 | Short messages between tool calls | says less | small: 1–3% of a run | low | switch **Short messages** (off) |
| 7 | Leave room in the subscription's windows | (fits one subscription) | none per run; keeps part of the window for the user | none | **Start no new runs above** 50–100% of a window (90%) |

Not proposed now, with the reason, in §4.8: a lower compaction threshold, continuing a builder's session after a QA fail, MCP servers switched per card, a model for subagents, and reordering the prompt for the cache.

### 4.1 Effort and model by card size

**What it does.** A card gets a **Size**: Small, Normal or Large. Each agent's form keeps its model and effort, now as the Normal size, and gets two optional rows: the model and effort for Small cards and for Large cards (empty = the Normal one). The Team Lead sets the size when it writes or splits a card (its instructions get one line); the user changes it on the card like Testing. With **Raise effort after a QA fail** on, a builder's run after `qa_fail` runs one size up (Small → Normal → Large), so a card that failed QA gets more thought, not the same again. Today every run of an agent uses one model and effort, whatever the card (`runs.rs:1031`), and the only per-card cost switch is Testing (§2.6). This is Caveman's "cheapest model and effort that gets it right", decided by the Team Lead, who already reads every card, and by the user, instead of by a classifier in the request path. [V for today's code; I for the design]

**Why it saves the most.** Effort decides all output: thinking, text and tool calls (§2.2), and on Opus 4.5 and later the thinking stays in the context and is read again on every later turn. Anthropic's own advice for Opus 5.5 is to sweep effort on one's own work rather than carry settings over, and it warns that `max` "adds significant cost for relatively small quality gains" (§2.6). This team runs most agents at `max` or `xhigh`. Moving Normal cards to `high` and keeping `xhigh`/`max` for Large ones should save 10–20% of those runs; Small cards at `medium`, or on Sonnet 5.5 at half Opus 5.5's price per token, 30–50% [I: output is a fifth to a third of a long run's cost, and thinking also grows the cache reads; the experiment measures it].

**The setting.**
- Card → Properties → **Size**: Small · Normal · Large; default Normal, which changes nothing for today's agents.
- Agent form → Agent: **Model** and **Effort** stay as they are (the Normal size); new rows **Small cards** (model, effort) and **Large cards** (model, effort), each empty by default, meaning "as Normal". The effort list is the one the form already offers for the model (`src/lib/models.ts:1-17`).
- Settings → Tokens: **Raise effort after a QA fail**, on.
- Proposed defaults for a new install (seed): Normal at the CLI's default, Small one level lower, Large at `xhigh`. Existing agents keep exactly what they have until the user fills the new rows.

**What the user sees.** In the run's facts (`src/components/RunHistory.tsx:75-93`): "Size Small: Sonnet 5.5, effort medium (Small cards, Backend Agent)", or "raised to Normal after QA's fail". On Usage → Cards: each card's size, cost, rounds and QA result; on Usage → Savings: cost per card by size and by effort.

**Risk.** A card sized too small gets too little thought: more QA fails, more rounds. Guarded by the raise after a fail, by the user's view of the size on every card, and by the experiment's guard (§5.2). A card's runs then use different models, which is fine for the cache: each run is its own session.

**Experiment.** Card parity (§5.2) on Normal cards: even cards at `high`, odd cards at the agent's current effort, two weeks; per card: cost, QA pass rate, rounds, failed runs, questions. A second one for Small cards: Sonnet 5.5 against the agent's Opus. The effort experiment is the one big enough to read with about 35 cards per group (§5.2).

### 4.2 A stable Team Lead prompt

**What it does.** The Team Lead's system prompt holds its own notes in full and a list of every other note with its size, and the date (§2.5). Any change to it, between two turns of a chat, makes the next turn write the whole conversation to the cache again (2× the input price on the 1-hour cache) instead of reading it (0.1×). With the switch on, Gizai freezes the Memory block and the date in the system prompt for the life of a chat thread (as they were at its first turn), and lists the list without sizes, so it changes only when a note is added or removed. What changed since goes, as one short line, into the bracket line Gizai already puts before a message ("A message may start with a line from Gizai in square brackets", `chat.rs:984`): "Notes changed since this chat started: Agents/Backend Agent/Notes, Projects/GA. Read them with memory_read." The Team Lead wrote its own new notes in this chat, so it has them already. [V for the prompt and its bracket line; I for the design]

**Saving.** For a chat whose context is 80,000 tokens, one avoidable miss costs about (2 − 0.1) × 80,000 × $4/M ≈ $0.61 at Opus 5.5 API prices on top of a hit. Each agent's `learned` line, each note the Team Lead writes and midnight cause one today. If a third of the turns miss for these reasons, the chat's input cost falls by roughly 10–30% [I]. M1 measures it exactly: cache writes per chat turn, before and after.

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

### 4.4 Shorter test and build output, with the full log kept

**What it does.** A small command of Gizai's, `gizai-quiet`, runs a test or build command, writes its whole output to a file in the run's temp folder, and prints only what the agent needs: the command, its exit code and time, the counts (passed, failed, ignored), every failure with its message and the lines around it, every error and warning line from compilers and linters, and the path of the full log. The exit code is the command's own, so `&&` chains keep working. Agents are told in "How this run works" to run tests and builds through it, and their allowed commands get one rule per allowed command they already have: `Bash(gizai-quiet cargo:*)` next to `Bash(cargo:*)`, so it never widens what an agent may run. After the run Gizai keeps the full logs with the run's log (`<data>/runs/<run-id>/outputs/`), because the temp folder is emptied. [I: the design; Caveman's and RTK's filters in §3 work the same way]

**Why.** QA runs the whole suite twice (`crates/gizai-core/roles/qa.md:10`), builders run `cargo test --workspace`, `npm test` and `npm run build` before they finish (CLAUDE.md:39-44), and a passing GizAi suite prints some 130,000–150,000 characters (§2.4). Claude Code cuts what it shows at its own limit (§3.6), so what enters the context is that much, read again on every later turn. Through the filter a passing suite is a few hundred characters, and a failing one keeps its failures in full instead of losing them to a cut in the middle.

**The setting.** Settings → Tokens: **Shorten test and build output**, on; **Lines kept around a failure**, 10–200, default 40. Per agent, the agent form's Allowed commands shows the added rules (a switch in the agent form can turn it off for one agent).

**What the user sees.** In the run, each shortened command is a normal step whose output starts with `gizai-quiet: 1,519 passed, 0 failed (exit 0) · full log: .gizai-tmp/outputs/03-cargo-test.log (142,311 characters)`. In the run's facts: "Shortened output: 4 commands, 212,000 → 9,800 characters", with **Open** beside each full log. On Usage → Savings: characters kept out of the context per agent and per card, and what they would have cost, read in the turns after them (§7.4).

**Saving.** About $0.50–$1 of a QA run, 5–10% [I: some 25,000 tokens of output kept out, which would have been read again in the run's later turns; §2.2]. Smaller for builders that run fewer suites.

**Risk.** A filter bug hides an error. Guarded: the exit code and counts are always shown; any line that looks like an error is kept; an output it doesn't recognise is shown as its first and last lines, never as nothing; the full log is one `rg` away; and the filter's own tests run on recorded outputs of each tool, passing and failing. Agents that forget to use it: Gizai sees every command in the stream and counts the test and build commands run without it (in the run's facts).

**Experiment.** Card parity for QA and builders, two weeks: tokens per run and per card, QA pass rate, rounds, and QA's `needs_decision` runs; plus a fidelity check on every failing run in the "on" group: the user (or a script) compares the failures in the full log with what the agent reported.

### 4.5 Comments and memory as ranges, cuts marked

**What it does.** The prompt's comments and memory have fixed caps today: the last 10 comments by anyone, each cut to 2,000 characters with no mark (`runs.rs:912-913`, `prompt.rs:37`, `:81`), and 6,000 characters of notes plus a 4,000-character list (`crates/gizai-core/src/memory.rs:30-35`). This idea makes them values the user sets, marks a cut ("… (cut at 2,000 of 5,400 characters; the full comment is on the card)"), and leaves QA's comment out of a builder's prompt when its issues are already the prompt's "QA found these issues" section (§2.3, row 3). [V for today's caps]

**The setting.** Settings → Tokens: **Comments per run** 0–20 (10); **Characters per comment** 500–8,000 (2,000); **Memory per run** 1,000–12,000 characters (6,000), which also caps the Team Lead's block; **List the other notes** on. Defaults are today's, so nothing changes until the user moves them.

**What the user sees.** The run's facts: "Comments: 10 of 14, 2 cut" and, as today, each note given and whether it was cut (`RunHistory.tsx:88-91`). Usage → Savings: characters left out of prompts per agent.

**Saving and risk.** Small: 5,000 tokens of comments read in 120 turns is about 600,000 cache-read tokens, some $0.25 a run at Opus 5.5 API prices; halving it saves about 1–2% [I]. The risk is a lost clarification from an early comment; the mark tells the agent something was cut.

**Experiment.** No card split needed for the saving: the characters left out are exact. For quality, a two-week period at 5 comments of 1,500 characters, watching questions (`needs_decision`) and QA fails that name a missed instruction.

### 4.6 Short messages between tool calls

**What it does.** One line in "How this run works", only with the switch on: "Between tool calls, write at most one short sentence. Code, commands, file paths, error messages and your final summary and hand-over stay complete and exact." The same line for every CLI; no proxy, no rewriting of answers. This is Caveman's "says less" for the part of an agent's output that nobody hands over. [I]

**The setting.** Settings → Tokens: **Short messages**, off by default: the saving is small and the narration is how the user follows a run live. A user who doesn't watch runs can switch it on.

**What the user sees.** The run reads terser; Usage → Savings shows output tokens per turn per agent, before and after.

**Saving and risk.** Text between tool calls is a small part of an agentic run's output; thinking and tool inputs are most of it. 1–3% [I]. Hand-overs stay complete by the rule's own words; QA's pass rate and the builders' hand-over length are the checks.

**Experiment.** Card parity, two weeks: output tokens per turn, hand-over length, QA pass rate.

### 4.7 Leave room in the subscription's windows

**What it does.** Gizai already reads each CLI's 5-hour and weekly windows (§2.7). With this value set, agents start no new run from an Auto column while a window of their CLI is above it; Run by hand still works, as with Pause all agents (`src/pages/SettingsPage.tsx:113`). It saves no tokens; it keeps a freelancer's own Claude Code or Codex usable during the day. [V for the readings; I for the design]

**The setting.** Settings → Tokens: **Start no new runs above** 50–100% of a window, default 90% (100% = today).

**What the user sees.** On the board, a waiting card says "Waiting: Claude Code's 5-hour window is at 92%, resets 15:00"; Usage → Subscription marks the line.

**Risk.** None to quality; cards wait longer.

### 4.8 Not proposed now

- **A lower compaction threshold.** Compacting earlier shrinks what each turn reads, but a summary loses details an agent may need, and the summary itself costs a call. Claude Code compacts on its own near its limit (§3.6). Worth an experiment after M1, not a setting yet [I].
- **Continuing the builder's session after a QA fail** instead of a fresh one with QA's issues (`runs.rs:918`). The session knows what it did, but QA's run usually takes longer than the cache's hour, so the whole old context would be written again at 2×; a fresh run with the issues is often cheaper. An experiment after M1 [I].
- **MCP servers per card.** Task runs get only the agent's own servers (agent form → Tools); Claude Code loads MCP tools on demand where its tool search applies (§3.6). Little left to save [I].
- **A model for subagents.** Claude Code's subagents (the Task and Agent tools, always on: `crates/gizai-agents/src/tool_catalog.rs:80-81`) can be pointed at a cheaper model (§3.6). How much agents use them is not recorded today; M1 records it (Claude Code's `modelUsage` per model), and then it can be a field in the agent form.
- **Reordering the prompt for the cache.** Putting the parts an agent's runs share first would turn a few thousand tokens per run from cache writes into reads, some $0.03 a run, while moving "How this run works" away from the end, where it is read last. Not worth it [I].

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

What it can't measure yet, and card M1 adds (§8): the split of input into fresh, cache writes and cache reads, the turns, the effort and the efficiency settings each run had, and the share of the 5-hour and weekly windows each run used.

### 5.2 How every experiment runs

- **Measure before cutting.** M1 ships first and records a baseline week with today's settings. [I]
- **Alternate by card, not by week.** A setting is on for cards with an even number and off for odd ones (GA-170 on, GA-171 off), so both groups see the same weeks, projects and agents. GizAi records which group each run was in. The user can stop an experiment at any time; it is a switch too. [I]
- **Compare per card, not per run.** The cost of a card is all its runs (builder, QA, fixes after `qa_fail`); a setting that makes runs cheaper but adds a QA round can cost more per card. [I]
- **A guard on quality, decided up front.** A setting becomes a default only when, over at least 30 cards per group, the cost per card falls by the expected amount and: the QA pass rate falls by no more than 10 points, rounds per card rise by no more than 0.3, and failed runs and questions don't rise. Otherwise it stays available but off. [I: the thresholds are a proposal for the user to set; 30 cards per group shows a 25–30% cost difference, not a 5% one]
- **Small, by design.** With about five cards a day, a two-week experiment gives some 35 cards per group. That is enough to see the big levers (effort, model), not the small ones (comments, memory): for those, the saving is measured exactly from the characters cut, and only the quality guard needs the cards. [I]

*(The experiment for each idea follows in §4, with the idea.)*

## 6. Options

Three ways to build "fewer tokens without losing quality" into GizAi. All three keep Jefsev's rule: each saving is a setting the user switches or sets.

**Option A: native, in GizAi (recommended).** GizAi already writes the prompt, chooses the CLI's flags and environment, sets each agent's model and effort, and reads every event of the run. The savings are made there: effort and model per role and per card, a small output filter of its own that agents run their tests and builds through (`gizai-quiet`, full log kept with the run), a stable Team Lead prompt, a "short messages" line in "How this run works", and ranges for comments and memory. Plus what makes it visible and testable: per-run token detail, Usage per card, a Savings view and card-parity experiments.
- Libraries: none new for the app. The filter is a small Rust binary shipped next to Gizai like `gizai-mcp` (`crates/gizai-mcp`), with its own parsers for the formats this user's projects use (cargo, Vitest, tsc, ESLint, PHPUnit and Pest, Composer and npm), and a generic rule for the rest. Rules may be borrowed from RTK or Caveman under Apache-2.0 with a NOTICE line (§3.5). [I]
- Risks: a filter bug that hides an error (mitigated: exit code and counts always shown, every error-like line kept, full log one command away, a fidelity test suite on recorded outputs); agents that forget to use the filter (measured: GizAi sees each command in the stream); lower effort that costs quality (guarded by the experiments of §5 and by raising effort after a QA fail). [I]
- Size: about ten cards (§8), each one agent run; the first three are independent. [I]

**Option B: bundle an existing filter (RTK) and its Claude Code hook.** RTK ("CLI proxy that reduces LLM token consumption by 60-90% on common dev commands. Single Rust binary, zero dependencies") is Apache-2.0, has 83,281 stars and was pushed to today (GitHub API, 2026-10-14). Its README says it hooks into Claude Code with a "PreToolUse hook" that rewrites commands, keeps "Failures only, passing tests collapsed to a count" for `cargo test` and `npm test`, and measures savings with `rtk gain`; it says nothing about keeping the full output. [V, its README and the GitHub API]
- What it takes: shipping or asking the user to install a second fast-moving binary (1,520 open issues), turning hooks back on for headless runs, which GizAi turns off on purpose today (`--settings {"disableAllHooks":true}`, `crates/gizai-agents/src/claude.rs:39`, `:93-95`; `cli.rs:181-184`), and adding a way to keep the full output.
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
export type TokenSetting = "shortenOutput" | "shortMessages" | "comments" | "effortHigh";
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

- **Exact:** characters kept out of a prompt (comments, memory) and out of the context by `gizai-quiet` (the full output, or Claude Code's own cut of it when smaller (§3.6), minus what was printed).
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

*(to come)*

## 9. Open questions

*(to come)*

## 10. Sources

*(to come)*
