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
};
export type ProjectInput = {
  clientId?: string | null; name: string; key: string; status?: string | null; goalMd?: string | null; repoPath?: string | null; repoUrl?: string | null;
  defaultBranch?: string | null; color?: string | null; budgetAmountMinor?: number | null; budgetHours?: number | null;
};
export type Label = { id: string; name: string; color?: string | null };
export type Task = {
  id: string; identifier: string; projectId?: string | null; projectName?: string | null; projectColor?: string | null;
  title: string; descriptionMd: string; acceptanceMd?: string | null; stateId: string; stateName: string; stateCategory: string;
  priority: number; assigneeId?: string | null; assigneeName?: string | null; assigneeKind?: string | null; labels: Label[];
  hold?: string | null; holdReason?: string | null; bounceCount: number; failCount: number; sortKey: string;
  branch?: string | null; createdAt: number; updatedAt: number;
};
export type TaskInput = {
  projectId: string; title: string; descriptionMd?: string; acceptanceMd?: string | null; stateId?: string | null;
  priority?: number; assigneeId?: string | null; labelIds?: string[];
};
/** Every field optional; "" clears an optional value. */
export type TaskPatch = Partial<{
  title: string; descriptionMd: string; acceptanceMd: string; priority: number; assigneeId: string; pinnedActorId: string;
  dueOn: string; hold: string; holdReason: string;
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
export type Run = {
  id: string; agentId: string; agentName: string; taskId?: string | null; roleKey?: string | null; trigger: string; status: string;
  outcome?: string | null; summaryMd?: string | null; createdAt: number; startedAt?: number | null; endedAt?: number | null;
  costUsdMicros: number; inputTokens: number; outputTokens: number; branch?: string | null; worktreePath?: string | null;
  sessionId?: string | null; error?: string | null; logPath: string;
  /** The commit its worktree was at when it started. */
  baseSha?: string | null;
};
export type LiveRun = { runId: string; taskId: string; agentId: string };
export type Settings = { claudeBin?: string | null; dataDir: string; maxConcurrentRuns: number; agentsPaused: boolean; maxRunUsd?: number | null; maxRunMinutes: number; maxRunToolCalls: number };
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
