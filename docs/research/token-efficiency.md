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
| 3 | QA's issues from the last failed QA run | builders only (`runs.rs:918`; `prompt.rs:71-77`) | none | usually under 2,000 |
| 4 | Recent comments, oldest first | the last 10 (`runs.rs:912-913`), each cut to 2,000 characters (`prompt.rs:37`, `:81`) | 10 × 2,000 | up to 20,000 |
| 5 | Your branch | the fetched main branch (`prompt.rs:85-92`) | | about 100–250 |
| 6 | Limits of this run | Settings → Runs (`runs.rs:914`; `prompt.rs:109-113`) | | about 330 |
| 7 | Memory | the agent's notes, then the card's shared notes, then a list of the rest (`crates/gizai-core/src/memory.rs:1857-1956`) | 6,000 in full (own notes 3,000 when there are shared notes), list 4,000 (`memory.rs:30-35`) | up to about 10,600 with its 560-character intro |
| 8 | How this run works | per CLI and permission mode (`prompt.rs:297-353`) | | about 2,000–2,900 on Claude Code with the default list (29 commands, `seed.rs:245-250`) |
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

*(to come)*

## 5. Experiments

*(to come)*

## 6. The design of the recommended option

*(to come)*

## 7. Proposed cards

*(to come)*

## 8. Open questions

*(to come)*

## 9. Sources

*(to come)*
