// Mirrors gizai-core models (camelCase on the wire).
export type Client = {
  id: string; name: string; legalName?: string | null; kind: string; vatNumber?: string | null; cocNumber?: string | null;
  iban?: string | null; email?: string | null; phone?: string | null; website?: string | null; street?: string | null;
  postalCode?: string | null; city?: string | null; country?: string | null; currency: string; paymentTermsDays?: number | null;
  status: string; notesMd?: string | null; mainContact?: string | null; openTasks: number; projects: number; updatedAt: number;
};
export type ClientInput = {
  name: string; legalName?: string | null; kind?: string | null; vatNumber?: string | null; cocNumber?: string | null;
  iban?: string | null; email?: string | null; phone?: string | null; website?: string | null; street?: string | null;
  postalCode?: string | null; city?: string | null; country?: string | null; paymentTermsDays?: number | null;
  status?: string | null; notesMd?: string | null;
};
export type Contact = { id: string; clientId: string; name: string; role?: string | null; email?: string | null; phone?: string | null; isPrimary: boolean };
export type Person = { id: string; name: string; handle: string; title?: string | null; email?: string | null; openTasks: number };
export type Project = {
  id: string; clientId?: string | null; clientName?: string | null; number: string; key: string; name: string; status: string;
  color?: string | null; goalMd?: string | null; repoPath?: string | null; repoUrl?: string | null; defaultBranch: string; teamId?: string | null;
  budgetAmountMinor?: number | null; budgetHours?: number | null; openTasks: number; doneTasks: number; updatedAt: number;
  /** AI usage: the API cost of the runs on its cards this month (UTC), an estimate at API prices; and how many of those runs
   *  have an unknown cost (tokens but no cost: a CLI that reports none). */
  aiCostUsdMicros: number; aiUnknownCostRuns: number;
  /** How a new worktree is prepared: paths copied from the main checkout, the install of what is missing, a setup command. */
  worktreeCopy: string[]; worktreeInstall: boolean; worktreeSetup?: string | null;
  /** Team Lead may merge (GA-86): the Team Lead may merge a pull request QA passed, once its checks are green. Off by default. */
  leadMayMerge?: boolean;
};
export type ProjectInput = {
  clientId?: string | null; name: string; key: string; status?: string | null; goalMd?: string | null; repoPath?: string | null; repoUrl?: string | null;
  defaultBranch?: string | null; color?: string | null; budgetAmountMinor?: number | null; budgetHours?: number | null;
  /** Left out (null): kept as they are. */
  worktreeCopy?: string[] | null; worktreeInstall?: boolean | null; worktreeSetup?: string | null;
  /** Left out (null): kept as it is. Only a person sets it (here, in the app). */
  leadMayMerge?: boolean | null;
};
export type Label = { id: string; name: string; color?: string | null };
export type Task = {
  id: string; identifier: string; projectId?: string | null; projectName?: string | null; projectColor?: string | null;
  title: string; descriptionMd: string; acceptanceMd?: string | null; stateId: string; stateName: string; stateCategory: string;
  priority: number; assigneeId?: string | null; assigneeName?: string | null; assigneeKind?: string | null; labels: Label[];
  hold?: string | null; holdReason?: string | null; bounceCount: number; failCount: number; sortKey: string;
  branch?: string | null;
  /** The card's pull request on GitHub or Bitbucket, and its state as Gizai last saw it. */
  prUrl?: string | null; prState?: PullState | null;
  /** On: the QA Agent tests the card before Review. Off: it goes straight to Review (a small fix). */
  testing: boolean;
  createdAt: number; updatedAt: number;
  /** Archived from Done: when, and who archived it. Null for a card on the board. */
  archivedAt?: number | null; archivedBy?: string | null;
  /** Run this for me: while the card is on hold, the commands its latest run asks you to run; Done, continue resumes that run. */
  runForMe?: string[];
  /** On hold for a decision, and the question is with the Team Lead (GA-70): it answers, or asks you. Not in the Inbox meanwhile. */
  withLead?: boolean;
};
export type PullState = "open" | "draft" | "merged" | "closed";
/** A card's pull request; `note` says something worth knowing (uncommitted changes left out, what a merge cleaned up). */
export type PullInfo = { url: string; number?: number | null; state: PullState; note?: string | null };
export type TaskInput = {
  projectId: string; title: string; descriptionMd?: string; acceptanceMd?: string | null; stateId?: string | null;
  priority?: number; assigneeId?: string | null; labelIds?: string[];
  /** The Testing switch; left out = on. */
  testing?: boolean;
};
/** Every field optional; "" clears an optional value. */
export type TaskPatch = Partial<{
  title: string; descriptionMd: string; acceptanceMd: string; priority: number; assigneeId: string; pinnedActorId: string;
  dueOn: string; hold: string; holdReason: string;
  /** The Testing switch. */
  testing: boolean;
}>;
export type TaskFilter = { projectId?: string | null; openOnly?: boolean };
export type Comment = { id: string; authorId: string; authorName: string; authorKind: string; bodyMd: string; runId?: string | null; createdAt: number };
export type ChangeEntry = { at: number; actorName?: string | null; table: string; op: string; diff: unknown };
export type TeamSummary = { id: string; name: string };
export type Member = {
  actorId: string; name: string; kind: string; roleKey: string; title?: string | null; adapter?: string | null; instructionsMd?: string | null;
  handle: string; status: string; isLead: boolean;
  model?: string | null; permissionMode?: string | null; allowedTools: string[]; wakeup?: string | null; heartbeatMinutes?: number | null;
  budgetUsdMicros?: number | null; lastHeartbeatAt?: number | null; chatEnabled: boolean;
  /** Claude Code --effort; null = Claude Code's default. */
  effort?: string | null;
  /** Cards it works on at once (1 when absent). */
  maxRuns?: number;
  /** The Team Lead checks the board this often (minutes); null = off. */
  boardCheckMinutes?: number | null;
  boardCheckedAt?: number | null;
  /** Why its board check stopped (three failed checks in a row); null = not paused. */
  boardCheckPaused?: string | null;
  /** Folders besides its worktree its file tools may read, or read and change. */
  folders?: AgentFolder[];
  /** Its MCP servers switched on or off, with the tools switched off of each (agent form → Tools). */
  tools?: AgentTools;
  /** Its CLI's own tools switched on: web search, fetching pages, built-in tools (agent form → Tools). */
  cliTools?: CliTools;
  /** Its runs get a Memory section and its learned lines are kept (on when absent). */
  useMemory?: boolean;
  /** The agent whose memory folder it shares, the group's owner (GA-96); null or absent: its own folder. */
  sharesMemoryWith?: string | null;
};
/** A folder an agent's file tools may use besides its worktree: "read", or "change" (read and change). */
export type AgentFolder = { path: string; access: "read" | "change" };
/** What the agent form shows next to a folder: why it's refused, or a warning. `path` as it would be saved. */
export type FolderCheck = { path: string; error?: string | null; warning?: string | null };
/** A coding CLI agents run on (Settings → Coding CLIs); "claude_code" is the built-in Claude Code. */
export type CliKind = "claude_code" | "codex" | "gemini" | "other";
export type Cli = {
  /** Empty for a new one: saving gives it an id. */
  id: string; name: string; kind: CliKind;
  /** The program: a path or a name on your login shell's PATH. */
  command: string;
  /** NAME=value lines, e.g. CLAUDE_CONFIG_DIR=~/.claude-2. */
  env: string[];
  /** Other only: the arguments, with {prompt} and {model}. */
  args: string;
};
/** A CLI and the program that would run; `problem` when it isn't found. */
export type CliStatus = Cli & { path?: string | null; problem?: string | null };
/** A model Claude Code offers (its /model list). */
export type ModelOption = { value: string; resolvedModel?: string | null; displayName: string; description: string; supportsEffort: boolean; effortLevels: string[] };
export type Wakeup = "manual" | "on_assign" | "heartbeat";
export type AgentInput = {
  name: string; roleKey: string; title?: string | null; adapter?: string; model?: string | null; instructionsMd?: string | null;
  permissionMode?: string; allowedTools?: string[]; wakeup?: Wakeup; heartbeatMinutes?: number | null; budgetUsdMicros?: number | null;
  /** Answers on the Chat page; null/absent leaves it unchanged on update. */
  chatEnabled?: boolean | null;
  /** Claude Code --effort; null = Claude Code's default. */
  effort?: string | null;
  /** Cards it works on at once (1–10); null leaves it unchanged on update. */
  maxRuns?: number | null;
  /** The Team Lead's board check every this many minutes (5–1440), 0 = off; null leaves it unchanged on update. */
  boardCheckMinutes?: number | null;
  /** Its folders; null/absent leaves them unchanged on update (none for a new agent). */
  folders?: AgentFolder[] | null;
  /** Memory for its runs; null/absent: on for a new agent, unchanged on update. */
  useMemory?: boolean | null;
  /** The id of the agent whose memory folder it shares (that agent's group's owner when it shares one; never the Team
   *  Lead), "" for its own folder (GA-96); null/absent: its own folder for a new agent, unchanged on update. */
  sharesMemoryWith?: string | null;
};
/** A column's category: its name can change, the gates key off this. Deploy: merged, not deployed yet (worked by you). */
export type StateCategory = "backlog" | "ready" | "in_progress" | "testing" | "review" | "deploy" | "done" | "cancelled";
/** A board column. auto: the agents on it (agentIds, in order) pick up its cards by themselves; Manual: only Run starts one.
 * nextStateId: the column its cards go to next. Backlog, Review, Done and Cancelled columns take no agents and are never Auto. */
export type WorkflowState = { id: string; name: string; category: string; wipLimit?: number | null; color?: string | null; sortKey: string;
  auto?: boolean; nextStateId?: string | null; agentIds?: string[];
  /** Gone with label routing (GA-49): Gizai no longer sends it; only older test fixtures still carry it. */
  ownerRole?: string | null };
/** A branch of the organisation chart: the team's agents with one of its roles. */
export type Branch = { key: string; name: string; roles: string[] };
export type Team = { id: string; name: string; members: Member[]; states: WorkflowState[]; labels: Label[]; branches?: Branch[];
  /** Gone with label routing (GA-49): Gizai no longer sends routing rules; only older test fixtures still carry it. */
  rules?: unknown[] };
/** Changes to a column; only what is given changes. nextStateId "" clears it; afterId "" moves it to the front. */
export type ColumnInput = { name?: string; agentIds?: string[]; auto?: boolean; nextStateId?: string; afterId?: string };
/** What removing a column does, for its confirm. blocked: why it can't be removed now (the bin's tooltip). */
export type ColumnRemoval = { cards: number; archived: number; defaultTarget?: string | null; relinked: string[]; unlinked: string[]; blocked?: string | null };
/** A label with the number of cards that carry it. */
export type LabelInfo = { id: string; name: string; color?: string | null; cards: number };
export type AppInfo = { version: string; data_dir: string; selftest: boolean; you_id: string; start_route?: string | null; selftest_mode?: string | null; data_label?: string | null };
export type Doc = { id: string; projectId?: string | null; title: string; bodyMd: string; currentVersion: number; updatedAt: number;
  /** "memory" for a memory note (GA-19), with its path like "Team Lead/Notes"; "doc" for a project's. */
  kind?: string; path?: string | null };
export type DocVersion = { version: number; authorName?: string | null; createdAt: number };
export type FileRow = { id: string; name: string; mime?: string | null; sizeBytes: number; sha256: string; createdAt: number };
export type FileOwner = "client" | "project" | "task" | "comment" | "doc";
export type AddFilesResult = { added: FileRow[]; failed: string[] };

export type RunEvent =
  | { kind: "init"; session_id: string; model: string }
  | { kind: "text"; text: string }
  | { kind: "tool_use"; name: string; summary: string }
  | { kind: "tool_result"; is_error: boolean; preview: string }
  | { kind: "result"; is_error: boolean; subtype: string; text: string; cost_usd?: number | null; input_tokens: number; output_tokens: number; num_turns: number }
  | { kind: "other"; raw_type: string }
  /** A note from Gizai, such as a folder the run goes without. */
  | { kind: "note"; text: string }
  /** The MCP servers Claude Code's init line names, with their state (connected, failed, needs-auth). */
  | { kind: "mcp_servers"; servers: { name: string; status: string }[] }
  /** A tool call the CLI refused: it needed an approval nobody can give in a headless run. */
  | { kind: "refused"; tool: string; input: string; reason: string };
export type SeqEvent = { seq: number; event: RunEvent };
/** How a run ended, from its GIZAI_RESULT line; `deployed` is the DevOps Agent's. */
export type RunOutcome = "ready_for_testing" | "qa_pass" | "qa_fail" | "needs_decision" | "deployed" | "no_result" | "error";
export type Run = {
  id: string; agentId: string; agentName: string; taskId?: string | null; roleKey?: string | null; trigger: string; status: string;
  outcome?: RunOutcome | null; summaryMd?: string | null; createdAt: number; startedAt?: number | null; endedAt?: number | null;
  costUsdMicros: number; inputTokens: number; outputTokens: number; branch?: string | null; worktreePath?: string | null;
  sessionId?: string | null; error?: string | null; logPath: string;
  /** The commit its worktree was at when it started. */
  baseSha?: string | null;
  /** The id of the coding CLI it ran on. */
  adapter?: string | null;
  /** The commit its worktree was at when it ended; null while it runs and for runs from before Gizai saved it. */
  headSha?: string | null;
  /** The tool calls its CLI refused (Refused in this run): Claude Code reports them, other CLIs don't. */
  refused?: Refusal[];
  /** Gizai's own nudge after a run ended without its result line (trigger result_nudge; before GA-31 it was a nudge too). */
  nudged?: boolean;
  /** Run this for me: the commands its needs_decision result asks you to run for it. */
  runForMe?: string[];
  /** The memory notes its prompt was given (GA-19). */
  memory?: GivenNote[];
  /** It ended asking for a decision and the Team Lead took the question (GA-70): what it did with it. */
  lead?: LeadAnswer | null;
  /** A Team Lead's run on a question (trigger question, GA-70): the card the question is on. */
  questionTaskId?: string | null;
};
/** What the Team Lead did with a run's question (GA-70). state: asking (it looks at it now), answering (it answered and Gizai
 *  continues the agent), answered, escalated (it asked you: the Inbox), dropped (the card moved on before it was done),
 *  skipped (it can't run here, reason says why: the question went to you as before) or limit (the limits sent it to you,
 *  reason says which). */
export type LeadAnswer = {
  state: "asking" | "answering" | "answered" | "escalated" | "dropped" | "skipped" | "limit" | string;
  leadId?: string | null;
  /** The Team Lead's run on the question. */
  runId?: string | null;
  /** Escalated: why you decide. */
  reason?: string | null;
  /** Answered: the start of its answer (the card's comment has it all). */
  answer?: string | null;
  /** The memory note the answer was kept in. */
  note?: string | null;
  /** What the Team Lead's run on it cost (it counts toward the Team Lead's budget). */
  costUsdMicros: number;
  /** You answered after it escalated, and the Team Lead kept your answer in memory. */
  learned?: boolean;
  /** Escalated: the Team Lead's comment that asks you. */
  commentId?: string | null;
};
/** A memory note a run's prompt was given: its path, its length and how much of it the prompt showed (less when cut). */
export type GivenNote = { path: string; chars: number; shown: number };
/** A note in Gizai's Memory (GA-19): a doc of kind memory, with a path like "Team Lead/Notes". */
export type MemoryNote = { id: string; path: string; scope: "shared" | "agent"; ownerId?: string | null; bodyMd: string; currentVersion: number;
  updatedAt: number; updatedBy?: string | null; chars: number };
/** What saving a memory note did. */
export type MemorySaved = { id: string; path: string; version: number; created: boolean };
/** A memory search result: the note (without its text) and the line that matched. */
export type MemoryHit = { note: MemoryNote; snippet: string };
/** A note's last saved version (Memory → Recently changed): who wrote it, the run and its card when a run did, and when. */
export type MemoryChange = { note: MemoryNote; version: number; at: number; authorId?: string | null; authorName?: string | null;
  authorKind?: string | null; runId?: string | null; taskId?: string | null; taskIdentifier?: string | null };
/** A tool call a run's CLI refused: the tool, what it asked for (the command, the file) and why, when the CLI said. */
export type Refusal = { tool: string; input: string; reason?: string };
/** A commit a run made: its id and the first line of its message. */
export type Commit = { sha: string; subject: string };
export type LiveRun = { runId: string; taskId: string; agentId: string };
export type Settings = { claudeBin?: string | null; dataDir: string; maxConcurrentRuns: number; agentsPaused: boolean; maxRunUsd?: number | null; maxRunMinutes: number; maxRunToolCalls: number;
  /** The GitHub CLI; null = found when needed. */
  ghBin?: string | null;
  /** How Open pull request and Push branch reach GitHub. */
  pushOver: PushOver;
  /** Settings → Notifications: a desktop notification per kind, on or off. */
  notifications: NotificationSwitches };
/** The kinds of desktop notification: a card on hold, a card waiting for your review or deploy, the Team Lead asking
 *  (a Question or Approval chat), and the Team Lead's answer in a chat while Gizai is out of sight. All on by default. */
export type NotificationSwitches = { hold: boolean; waiting: boolean; leadAsks: boolean; leadAnswered: boolean };
/** A Done or Cancelled card's worktree (Settings → Data); `bytes` is its disk use. */
export type OldWorktree = {
  taskId: string; identifier: string; title: string; category: "done" | "cancelled"; projectName: string; branch: string; path: string;
  bytes: number; uncommitted: number; live: boolean;
};
export type RemovedWorktree = { taskId: string; identifier: string; removed: boolean; note: string };

// ---- Settings → GitHub ----
/** ssh: your SSH keys (the default); https: the GitHub CLI's login. */
export type PushOver = "ssh" | "https";
/** What went wrong talking to GitHub, and what to do about it. */
export type GithubProblem = { what: string; fix?: string | null };
/** Log in with GitHub: the one-time code to enter on GitHub, and where. */
export type GithubLoginCode = { code: string; url: string };
/** Whether Gizai can use GitHub: the GitHub CLI, the account it is logged in as, and how pushes go. */
export type GithubStatus = {
  ghPath?: string | null; ghVersion?: string | null; ghProblem?: GithubProblem | null;
  account?: string | null; accountProblem?: GithubProblem | null;
  pushOver: PushOver;
  /** A login in the browser that waits for its code to be entered. */
  login?: GithubLoginCode | null;
  /** The command that logs gh in from a terminal. */
  loginCommand?: string | null;
};
/** One line of Check connection. Skipped: not needed now, or it needs something that failed. */
export type ConnectionCheckItem = {
  name: string; result: "ok" | "failed" | "skipped"; text: string; fix?: string | null; projectId?: string | null; repo?: string | null;
};
export type ConnectionCheck = { ok: boolean; pushOver: PushOver; checks: ConnectionCheckItem[] };

// ---- Settings → Bitbucket ----
/** Whether Gizai can use Bitbucket: your Atlassian email, whether an API token is saved (the token itself never comes back),
 * and the account it belongs to. Pushes go over SSH with your own keys. */
export type BitbucketStatus = {
  email?: string | null;
  hasToken: boolean;
  /** Who the token belongs to. */
  account?: string | null;
  /** Why there is none, with what to do. */
  accountProblem?: GithubProblem | null;
};

// ---- updates: Settings → Updates and the notice above Company ----
/** A release on GitHub: `version` "0.1.6" from the tag "v0.1.6"; `url` is its page, `notes` its Markdown notes. */
export type Release = { version: string; tag: string; name?: string | null; url?: string | null; publishedAt?: string | null; notes?: string | null };
/** While an update runs: source (getting it), build, backup, install. When it has ended: installed, failed, stopped. */
export type UpdateStep = "source" | "build" | "backup" | "install" | "installed" | "failed" | "stopped";
export type UpdateJob = {
  version: string; step: UpdateStep; startedAt: number; endedAt?: number | null;
  /** Its log: every command and what it said. */
  log: string;
  /** The backup of your data made before installing. */
  backup?: string | null;
  /** Why it failed, and the end of what the failed command said. For an installed update: what the installer said
   * went wrong after the new version was in place. */
  problem?: string | null; output?: string | null;
  /** When it failed: whether the Gizai installed before is still in place, as it was (unless the install failed partway). */
  unchanged: boolean;
};
export type UpdateStatus = {
  /** This Gizai's version. */
  current: string;
  /** Check for new releases (at start and every six hours). */
  autoCheck: boolean;
  checking: boolean;
  /** The last check: when, the latest release it found, and why it didn't work. */
  checkedAt?: number | null; latest?: Release | null; problem?: GithubProblem | null;
  /** The latest release when it is newer than this Gizai. */
  available?: Release | null;
  /** A newer version that is installed already: a restart starts it. */
  installed?: string | null;
  /** Where an update installs, or why this Gizai can't update itself (a dev build). */
  installTo?: string | null; cannotInstall?: string | null;
  /** The update that runs now, or the last one since Gizai started. */
  job?: UpdateJob | null;
  /** Where releases come from. */
  repo: string;
};
export type DayStat = { dayStart: number; succeeded: number; failed: number; other: number };

// ---- the Usage page (gizai-core usage.rs) ----
export type UsagePeriod = "today" | "7d" | "30d" | "month";
/** What a set of runs used. `runs` includes the chat turns; input tokens include cache reads and writes; the cost leaves out the
 *  `unknownCostRuns` (tokens but no cost: a CLI that reports none) and is an estimate at API prices, not a bill. */
export type UsageTotals = { runs: number; chatTurns: number; inputTokens: number; outputTokens: number; costUsdMicros: number; unknownCostRuns: number };
export type UsageDay = { dayStart: number; totals: UsageTotals };
export type AgentUsage = { agentId: string; name: string; roleKey?: string | null; totals: UsageTotals };
export type ProjectUsage = { projectId: string; number: string; key: string; name: string; color?: string | null; totals: UsageTotals };
/** One period (UTC days, from `since` up to `until`). The agents add up to `total`; so do the projects with `chat` (runs without a
 *  card: the Team Lead's chat turns and board checks) and `noProject` (runs on cards without a project). */
export type Usage = { since: number; until: number; total: UsageTotals; days: UsageDay[]; agents: AgentUsage[]; projects: ProjectUsage[];
  chat: UsageTotals; noProject: UsageTotals };

// ---- the Usage page's Subscription tab (gizai-core limits.rs) ----
/** One reading of a subscription limit, as the coding CLI reported it in a run or chat turn. `usedPercent`: 0 to 100 (more past the
 *  cap), null when the CLI only said the limit was reached; `status`: allowed, allowed_warning (near it) or rejected (reached);
 *  `resetsAt`, `observedAt`: Unix ms; `resetsText`: the reset as the CLI wrote it, when it gave no time stamp. */
export type LimitReading = { key: string; usedPercent?: number | null; status?: string | null; resetsAt?: number | null; resetsText?: string | null;
  windowMinutes?: number | null; observedAt: number; runId?: string | null };
/** A limit of a coding CLI: Claude Code's session (five_hour), weekly (seven_day) and Fable (seven_day_overage_included) limits, or a
 *  Codex window (primary, secondary); `reading` null until a run on that CLI reports it. */
export type SubscriptionLimit = { key: string; name: string; windowMinutes?: number | null; reading?: LimitReading | null };
export type LimitAgent = { agentId: string; name: string; roleKey: string; status: string; isLead: boolean };
/** One coding CLI's block (Settings → Coding CLIs): `readable` for Claude Code and Codex; `accountDir` holds the account (Claude
 *  Code's CLAUDE_CONFIG_DIR, Codex's CODEX_HOME with its session logs); `leadChat`: the Team Lead's chat runs on it; `chats`: chats
 *  whose own Runs on it is. */
export type CliLimits = { cliId: string; name: string; kind: CliKind; readable: boolean; accountDir?: string | null; limits: SubscriptionLimit[];
  agents: LimitAgent[]; leadChat: boolean; chats: number };

// ---- chat with the Team Lead ----
export type ChatThread = { id: string; agentId: string; title: string; sessionId?: string | null; createdAt: number; updatedAt: number;
  costUsdMicros: number; inputTokens: number; outputTokens: number;
  /** A chat the Team Lead started during a board check: question | approval; null = your own chat. */
  kind?: "question" | "approval" | null;
  /** The identifiers of the cards a Team Lead chat is about. */
  tasks?: string[];
  answeredAt?: number | null; dismissedAt?: number | null;
  /** A Team Lead chat that still waits for you (it is in the Inbox). */
  waiting?: boolean;
  /** The chat's own Runs on (a coding CLI's id); null: it follows the Team Lead's. */
  cli?: string | null;
  /** The coding CLI whose account holds the chat's session. */
  sessionCli?: string | null };
/** role: user | agent | tool | system. Tool messages carry `tool` = {id, input, result?, isError?}. */
export type ChatMessage = { id: string; threadId: string; role: string; authorId?: string | null; authorName?: string | null;
  bodyMd?: string | null; runId?: string | null; toolName?: string | null; tool?: Record<string, unknown> | null; createdAt: number;
  /** A note's details: {kind: "switch", cli, cliName} where the chat moved to another CLI, {kind: "limit", cli, cliName, limit, resets?, messageIds}
   *  where an answer hit a usage limit. */
  meta?: ChatNoteMeta | null;
  /** The files you added to the message. */
  files?: FileRow[] };
/** Paths picked or dropped for a chat message: those that can be added, and why each other one can't. */
export type FileCheck = { ok: string[]; failed: string[] };
/** A chat the Archive found, with the newest of its messages (yours or the Team Lead's) whose text matches; none when only
 *  its title matches, or for an empty search. */
export type ChatHit = { thread: ChatThread; message?: ChatMessage | null };
export type ChatNoteMeta = { kind: "switch" | "limit" | string; cli?: string; cliName?: string; limit?: string; resets?: string | null; messageIds?: string[] };
/** `seq`: the last change to the text being written that `draft` holds. */
export type ChatStatus = { threadId: string; runId: string; draft: string; tool?: string | null; seq: number };
/** A message sent while the Team Lead answers; `held`: it waits for Send now instead of going when the answer is done. */
export type QueuedMessage = { id: string; threadId: string; bodyMd: string; createdAt: number; updatedAt: number; held: boolean;
  /** The files added to it; they go with it. */
  files?: FileRow[] };
/** A coding CLI in Runs on under the chat's text box; `problem`: why it can't run the chat. */
export type ChatCli = { id: string; name: string; kind: CliKind; problem?: string | null };
export type ChatEvent =
  | { kind: "delta"; threadId: string; text: string; seq: number }
  | { kind: "block"; threadId: string; seq: number }
  | { kind: "tool"; threadId: string; name: string }
  | { kind: "message"; threadId: string; message: ChatMessage };

// ---- MCP servers (Settings → MCP servers, agent form → Tools) ----

/** A server in Settings → MCP servers: names of its lines, never their values (those live in the keychain). */
export type McpServer = {
  id: string; name: string;
  /** stdio (a command) | http | sse (an address) */
  transport: string;
  command: string; args: string[]; envNames: string[];
  url: string; headerNames: string[];
  /** A client id for sign-in, for a server that doesn't let Gizai register itself. */
  clientId: string;
  /** Where it was imported from; empty when added by hand. */
  source: string;
};
/** A secret line as the form saves it: value only when typed in now; null keeps the one in the keychain. */
export type SecretLine = { name: string; value: string | null };
export type McpServerInput = { server: McpServer; env: SecretLine[]; headers: SecretLine[] };
export type McpParam = { name: string; ty: string; required: boolean; description: string };
export type McpHints = { readOnly: boolean; destructive: boolean; idempotent: boolean; openWorld: boolean };
/** One tool in plain words: what it does, its parameters, what the server says about it, and its risk. */
export type McpToolView = {
  name: string; title?: string | null; description: string; params: McpParam[];
  hints: McpHints; hintsSent: string[];
  /** low | medium | high */
  risk: string; summary: string; notes: string[];
};
export type McpListed = { serverName: string; serverVersion: string; listedAt: number; tools: McpToolView[] };
export type McpServerView = McpServer & {
  /** signed_in | needs_sign_in | "" */
  signIn: string;
  problem?: string | null;
  /** Lines whose value isn't in the keychain. */
  missing: string[];
  listed?: McpListed | null;
  usedBy: string[];
};
export type McpCandidate = {
  key: string; name: string; account: string; scope: string; folder?: string | null; transport: string;
  command: string; args: string[]; url: string; envNames: string[]; headerNames: string[];
  already: boolean; clash?: string | null;
};
export type McpScan = { servers: McpCandidate[]; problems: string[] };
export type McpPick = { key: string; name: string };
export type AgentServer = { serverId: string; on: boolean; toolsOff: string[] };
export type AgentTools = { mcp: AgentServer[] };
export type AgentServerView = {
  serverId: string; name: string; transport: string; on: boolean; toolsOff: string[]; signIn: string;
  actsAsYou?: string | null; lastRun?: { status: string; at: number } | null;
  tools: McpToolView[]; summary: string; risk: string;
};
export type AgentMcpView = { disabled?: string | null; warning?: string | null; servers: AgentServerView[] };
/** The CLI's own tools an agent has on (agent form → Tools; saved apart from the rest). Everything is off until switched on. */
export type CliTools = {
  webSearch: boolean; webFetch: boolean;
  /** Only these domains for fetching; none = any page. */
  fetchDomains: string[];
  /** The browser accepts self-signed certificates (local .test sites). */
  insecureCerts: boolean;
  /** The CLI's other tools switched on, by name. */
  builtin: string[];
};
/** One of a CLI's own tools: Gizai's catalog merged with what the CLI reported. `how`: web | switch | always | elsewhere | off. */
export type CatalogTool = { id: string; label: string; group: string; description: string; risk: string; how: string; note: string; reported: boolean };
/** What the hidden browser needs, found or not; `missing` says what to install. */
export type BrowserNeeds = { node?: string | null; nodeVersion?: string | null; npx?: string | null; browser?: string | null; browserName?: string | null; missing: string[] };
/** The agent form's Web, Browser and Built-in tools for the CLI picked in the form. */
export type ToolsView = {
  kind: CliKind;
  /** Why the CLI can't take each Web switch; null = it can. */
  web: { search?: string | null; fetch?: string | null; domains?: string | null };
  browser: { disabled?: string | null; needs: BrowserNeeds; version: string; lastRun?: { status: string; at: number } | null;
    tools: McpToolView[]; summary: string; risk: string };
  builtin: { tools: CatalogTool[]; source: string; canAsk: boolean };
  saved?: CliTools | null;
};
/** The built-in browser in Settings → MCP servers: only its version and browser program change. */
export type BrowserEntry = { version: string; program: string };
export type BrowserView = BrowserEntry & { id: string; command: string; needs: BrowserNeeds; problem?: string | null; listed?: McpListed | null; usedBy: string[] };
