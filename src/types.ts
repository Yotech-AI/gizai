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
  /** How a new worktree is prepared: paths copied from the main checkout, the install of what is missing, a setup command. */
  worktreeCopy: string[]; worktreeInstall: boolean; worktreeSetup?: string | null;
};
export type ProjectInput = {
  clientId?: string | null; name: string; key: string; status?: string | null; goalMd?: string | null; repoPath?: string | null; repoUrl?: string | null;
  defaultBranch?: string | null; color?: string | null; budgetAmountMinor?: number | null; budgetHours?: number | null;
  /** Left out (null): kept as they are. */
  worktreeCopy?: string[] | null; worktreeInstall?: boolean | null; worktreeSetup?: string | null;
};
export type Label = { id: string; name: string; color?: string | null };
export type Task = {
  id: string; identifier: string; projectId?: string | null; projectName?: string | null; projectColor?: string | null;
  title: string; descriptionMd: string; acceptanceMd?: string | null; stateId: string; stateName: string; stateCategory: string;
  priority: number; assigneeId?: string | null; assigneeName?: string | null; assigneeKind?: string | null; labels: Label[];
  hold?: string | null; holdReason?: string | null; bounceCount: number; failCount: number; sortKey: string;
  branch?: string | null;
  /** The card's pull request on GitHub, and its state as Gizai last saw it. */
  prUrl?: string | null; prState?: PullState | null;
  /** On: the QA Agent tests the card before Review. Off: it goes straight to Review (a small fix). */
  testing: boolean;
  createdAt: number; updatedAt: number;
  /** Archived from Done: when, and who archived it. Null for a card on the board. */
  archivedAt?: number | null; archivedBy?: string | null;
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
};
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
};
export type RuleInput = { kind: "label" | "column"; matchName: string; targetRole: string; priority: number };
/** A column's category: its name can change, the gates key off this. Deploy: merged, not deployed yet (worked by you). */
export type StateCategory = "backlog" | "ready" | "in_progress" | "testing" | "review" | "deploy" | "done" | "cancelled";
export type WorkflowState = { id: string; name: string; category: string; ownerRole?: string | null; wipLimit?: number | null; color?: string | null; sortKey: string };
export type RoutingRule = { id: string; kind: string; matchLabelId?: string | null; matchStateId?: string | null; targetRole?: string | null; targetActorId?: string | null; priority: number; enabled: boolean };
export type Team = { id: string; name: string; members: Member[]; states: WorkflowState[]; labels: Label[]; rules: RoutingRule[] };
export type AppInfo = { version: string; data_dir: string; selftest: boolean; you_id: string; start_route?: string | null; selftest_mode?: string | null; data_label?: string | null };
export type Doc = { id: string; projectId?: string | null; title: string; bodyMd: string; currentVersion: number; updatedAt: number };
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
  | { kind: "other"; raw_type: string };
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
};
/** A commit a run made: its id and the first line of its message. */
export type Commit = { sha: string; subject: string };
export type LiveRun = { runId: string; taskId: string; agentId: string };
export type Settings = { claudeBin?: string | null; dataDir: string; maxConcurrentRuns: number; agentsPaused: boolean; maxRunUsd?: number | null; maxRunMinutes: number; maxRunToolCalls: number;
  /** The GitHub CLI; null = found when needed. */
  ghBin?: string | null;
  /** How Open pull request and Push branch reach GitHub. */
  pushOver: PushOver };
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

// ---- chat with the Team Lead ----
export type ChatThread = { id: string; agentId: string; title: string; sessionId?: string | null; createdAt: number; updatedAt: number;
  costUsdMicros: number; inputTokens: number; outputTokens: number };
/** role: user | agent | tool | system. Tool messages carry `tool` = {id, input, result?, isError?}. */
export type ChatMessage = { id: string; threadId: string; role: string; authorId?: string | null; authorName?: string | null;
  bodyMd?: string | null; runId?: string | null; toolName?: string | null; tool?: Record<string, unknown> | null; createdAt: number };
export type ChatStatus = { threadId: string; runId: string; draft: string; tool?: string | null };
export type ChatEvent =
  | { kind: "delta"; threadId: string; text: string }
  | { kind: "block"; threadId: string }
  | { kind: "tool"; threadId: string; name: string }
  | { kind: "message"; threadId: string; message: ChatMessage };
