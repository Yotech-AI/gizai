// The only file that talks to Tauri. Swap this file to change the shell.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "./types";

export const appInfo = () => invoke<T.AppInfo>("app_info");
export const selftestReport = (json: unknown) => invoke<void>("selftest_report", { json: JSON.stringify(json) });
export const exitApp = (code = 0) => invoke<void>("exit_app", { code });

export const listClients = () => invoke<T.Client[]>("list_clients");
export const getClient = (id: string) => invoke<T.Client>("get_client", { id });
export const saveClient = (id: string | null, input: T.ClientInput) => invoke<string>("save_client", { id, input });
export const archiveClient = (id: string) => invoke<void>("archive_client", { id });
export const listContacts = (clientId: string) => invoke<T.Contact[]>("list_contacts", { clientId });
export const saveContact = (contact: T.Contact) => invoke<string>("save_contact", { contact });
export const listUsers = () => invoke<T.Person[]>("list_users");
export const addUser = (name: string, email: string | null) => invoke<string>("add_user", { name, email });

export const listProjects = () => invoke<T.Project[]>("list_projects");
export const getProject = (id: string) => invoke<T.Project>("get_project", { id });
export const saveProject = (id: string | null, input: T.ProjectInput) => invoke<string>("save_project", { id, input });

export const listTasks = (filter: T.TaskFilter = {}) => invoke<T.Task[]>("list_tasks", { filter });
export const getTask = (id: string) => invoke<T.Task>("get_task", { id });
export const createTask = (input: T.TaskInput) => invoke<string>("create_task", { input });
export const updateTask = (id: string, patch: T.TaskPatch) => invoke<void>("update_task", { id, patch });
export const moveTask = (id: string, stateId: string, sortKey: string) => invoke<void>("move_task", { id, stateId, sortKey });
export const setTaskLabels = (id: string, labelIds: string[]) => invoke<void>("set_task_labels", { id, labelIds });
export const taskActivity = (taskId: string) => invoke<T.ChangeEntry[]>("task_activity", { taskId });
export const listComments = (taskId: string) => invoke<T.Comment[]>("list_comments", { taskId });
export const addComment = (taskId: string, bodyMd: string) => invoke<string>("add_comment", { taskId, bodyMd });

export const listTeams = () => invoke<T.TeamSummary[]>("list_teams");
export const getTeam = (id?: string | null) => invoke<T.Team>("get_team", { id: id ?? null });

export const listDocs = (projectId: string) => invoke<T.Doc[]>("list_docs", { projectId });
export const getDoc = (id: string) => invoke<T.Doc>("get_doc", { id });
export const createDoc = (projectId: string, title: string) => invoke<string>("create_doc", { projectId, title });
/** Fails with "doc changed since you opened it" when baseVersion is not the current version. */
export const saveDoc = (id: string, bodyMd: string, baseVersion: number) => invoke<number>("save_doc", { id, bodyMd, baseVersion });
export const renameDoc = (id: string, title: string) => invoke<void>("rename_doc", { id, title });
export const docVersions = (id: string) => invoke<T.DocVersion[]>("doc_versions", { id });
export const docVersionBody = (id: string, version: number) => invoke<string>("doc_version_body", { id, version });

export const addFiles = (ownerType: T.FileOwner, ownerId: string, paths: string[]) => invoke<T.AddFilesResult>("add_files", { ownerType, ownerId, paths });
export const listFiles = (ownerType: T.FileOwner, ownerId: string) => invoke<T.FileRow[]>("list_files", { ownerType, ownerId });
export const removeFile = (id: string) => invoke<void>("remove_file", { id });
export const openFile = (id: string) => invoke<void>("open_file", { id });

export const addTeam = (name: string) => invoke<string>("add_team", { name });
export const addAgent = (teamId: string, input: T.AgentInput) => invoke<string>("add_agent", { teamId, input });
export const updateAgent = (actorId: string, input: T.AgentInput) => invoke<void>("update_agent", { actorId, input });
export const setAgentStatus = (actorId: string, status: "active" | "paused") => invoke<void>("set_agent_status", { actorId, status });
export const addRule = (teamId: string, input: T.RuleInput) => invoke<string>("add_rule", { teamId, input });
export const deleteRule = (ruleId: string) => invoke<void>("delete_rule", { ruleId });
export const renameState = (stateId: string, name: string) => invoke<void>("rename_state", { stateId, name });
export const getAgent = (id: string) => invoke<T.Member>("get_agent", { id });
/** The models this user's Claude Code offers (kept for half an hour; refresh asks Claude Code again). */
export const claudeModels = (refresh = false) => invoke<T.ModelOption[]>("claude_models", { refresh });
export const agentStats = (id: string, days = 14) => invoke<T.DayStat[]>("agent_stats", { id, days });
export const agentRuns = (id: string, limit = 20) => invoke<T.Run[]>("agent_runs", { id, limit });
export const agentNextTask = (id: string) => invoke<string | null>("agent_next_task", { id });
export const roleTemplate = (role: string) => invoke<string>("role_template", { role });

export const detectClaude = () => invoke<string | null>("detect_claude");
export const getSettings = () => invoke<T.Settings>("get_settings");
export const saveSettings = (settings: T.Settings) => invoke<void>("save_settings", { settings });
export const startRun = (taskId: string, agentId: string | null) => invoke<string>("start_run", { taskId, agentId });
export const stopRun = (runId: string) => invoke<void>("stop_run", { runId });
export const continueRun = (runId: string) => invoke<string>("continue_run", { runId });
export const listRuns = (taskId: string) => invoke<T.Run[]>("list_runs", { taskId });
export const runEvents = (runId: string) => invoke<T.SeqEvent[]>("run_events", { runId });
export const liveRuns = () => invoke<T.LiveRun[]>("live_runs");
export const suggestAgent = (taskId: string) => invoke<string | null>("suggest_agent", { taskId });
export const onRunEvent = (cb: (e: { runId: string; seq: number; event: T.RunEvent }) => void): Promise<UnlistenFn> =>
  listen<{ runId: string; seq: number; event: T.RunEvent }>("run-event", (m) => cb(m.payload));
export const onRunsChanged = (cb: () => void): Promise<UnlistenFn> => listen("runs-changed", () => cb());

export type RepoCheck = { isGit: boolean; branch?: string | null; dirty: boolean; github?: string | null };
export const checkRepo = (path: string) => invoke<RepoCheck>("check_repo", { path });

/** Fires after any write; screens refetch what they show. */
export const onRowsChanged = (cb: (table: string) => void): Promise<UnlistenFn> =>
  listen<{ table: string }>("rows-changed", (e) => cb(e.payload.table));

// ---- chat with the Team Lead ----
export const listChatThreads = () => invoke<T.ChatThread[]>("list_chat_threads");
export const chatMessages = (threadId: string) => invoke<T.ChatMessage[]>("chat_messages", { threadId });
/** Sends a message (a new thread when threadId is null) and starts the answer; resolves with the thread id. */
export const sendChat = (threadId: string | null, text: string) => invoke<string>("send_chat", { threadId, text });
export const stopChat = (threadId: string) => invoke<void>("stop_chat", { threadId });
export const chatLive = () => invoke<T.ChatStatus[]>("chat_live");
export const chatAgent = () => invoke<T.Member | null>("chat_agent");
export const onChatEvent = (cb: (e: T.ChatEvent) => void): Promise<UnlistenFn> => listen<T.ChatEvent>("chat-event", (e) => cb(e.payload));
export const onChatChanged = (cb: () => void): Promise<UnlistenFn> => listen("chat-changed", () => cb());
