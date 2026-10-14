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

### 2.4 What agents read and write during a run

*(to come: test and build output, file reads, diffs; numbers for GizAi's own repository)*

### 2.5 The Team Lead: chat answers and board checks

*(to come)*

### 2.6 Models and effort

*(to come)*

### 2.7 What GizAi records, and the real numbers

*(to come)*

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
