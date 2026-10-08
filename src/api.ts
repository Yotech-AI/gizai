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
export const removeContact = (id: string) => invoke<void>("remove_contact", { id });
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
/** A new column right after `afterId`; resolves with its id. ownerRole: null = nobody, a role key ("qa", "backend"), or "human" (you,
 * always for a Deploy column). Fails with "this team already has a column called X" for a name in use. */
export const addState = (teamId: string, name: string, afterId: string, category: T.StateCategory, ownerRole: string | null) =>
  invoke<string>("add_state", { teamId, name, afterId, category, ownerRole });
export const getAgent = (id: string) => invoke<T.Member>("get_agent", { id });
/** The models this user's Claude Code offers (kept for half an hour; refresh asks Claude Code again). `cli`: another Claude Code
 * CLI (a second account); other kinds of CLI have no list. */
export const claudeModels = (refresh = false, cli: string | null = null) => invoke<T.ModelOption[]>("claude_models", { refresh, cli });
/** Settings → Coding CLIs. */
export const listClis = () => invoke<T.CliStatus[]>("list_clis");
export const saveClis = (clis: T.Cli[]) => invoke<T.CliStatus[]>("save_clis", { clis });
/** Known coding CLIs installed here that aren't listed yet. */
export const findClis = () => invoke<T.Cli[]>("find_clis");
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
/** The commits a finished run made, oldest first. */
export const runCommits = (runId: string) => invoke<T.Commit[]>("run_commits", { runId });
export const runEvents = (runId: string) => invoke<T.SeqEvent[]>("run_events", { runId });
export const liveRuns = () => invoke<T.LiveRun[]>("live_runs");
export const suggestAgent = (taskId: string) => invoke<string | null>("suggest_agent", { taskId });
export const onRunEvent = (cb: (e: { runId: string; seq: number; event: T.RunEvent }) => void): Promise<UnlistenFn> =>
  listen<{ runId: string; seq: number; event: T.RunEvent }>("run-event", (m) => cb(m.payload));
export const onRunsChanged = (cb: () => void): Promise<UnlistenFn> => listen("runs-changed", () => cb());

/** Pushes a Review card's branch with your git login and opens its pull request with gh (keeps an open one). */
export const openPullRequest = (taskId: string) => invoke<T.PullInfo>("open_pull_request", { taskId });
/** Asks GitHub about the card's pull request now; a merge moves the card to Done. */
export const checkPullRequest = (taskId: string) => invoke<T.PullInfo | null>("check_pull_request", { taskId });
export const detectGh = () => invoke<string | null>("detect_gh");
/** Settings → GitHub: whether gh is found, the account it is logged in as, and how pushes go (gh asks GitHub). */
export const githubStatus = () => invoke<T.GithubStatus>("github_status");
/** Check connection: gh, its login, ssh to GitHub, and whether you can push to each project with a GitHub link. */
export const githubCheck = () => invoke<T.ConnectionCheck>("github_check");
/** Log in with GitHub: starts gh's login in the browser; resolves with its one-time code and link. */
export const githubLogin = () => invoke<T.GithubLoginCode>("github_login");
/** Resolves when that login has ended, with the account gh logged in as (null if it didn't say); rejects with why not. */
export const githubLoginWait = () => invoke<string | null>("github_login_wait");
export const githubLoginCancel = () => invoke<void>("github_login_cancel");

export type RepoCheck = { isGit: boolean; branch?: string | null; dirty: boolean; github?: string | null; suggestCopy?: string[] };
export const checkRepo = (path: string) => invoke<RepoCheck>("check_repo", { path });
export const listOldWorktrees = () => invoke<T.OldWorktree[]>("list_old_worktrees");
export const removeOldWorktrees = (taskIds: string[]) => invoke<T.RemovedWorktree[]>("remove_old_worktrees", { taskIds });

// ---- updates: the release check and Update to <version> ----
/** This version, what the release check found, and the update that runs (or ran). */
export const updateStatus = () => invoke<T.UpdateStatus>("update_status");
/** Asks GitHub for the latest release now. */
export const checkForUpdates = () => invoke<T.UpdateStatus>("check_for_updates");
/** Check for new releases (at start and every six hours): on or off. */
export const setUpdateAutoCheck = (on: boolean) => invoke<T.UpdateStatus>("set_update_auto_check", { on });
/** Builds `version` in the background, backs up your data and installs it; resolves at once. */
export const startUpdate = (version: string) => invoke<T.UpdateStatus>("start_update", { version });
/** Stops an update while it gets the source or builds. */
export const stopUpdate = () => invoke<T.UpdateStatus>("stop_update");
/** Quits and starts the Gizai an update installed (agents at work are stopped first, as when you quit). */
export const restartGizai = () => invoke<void>("restart_gizai");
export const onUpdateChanged = (cb: () => void): Promise<UnlistenFn> => listen("update-changed", () => cb());

/** Fires after any write; screens refetch what they show. */
export const onRowsChanged = (cb: (table: string) => void): Promise<UnlistenFn> =>
  listen<{ table: string }>("rows-changed", (e) => cb(e.payload.table));

// ---- chat with the Team Lead ----
export const listChatThreads = () => invoke<T.ChatThread[]>("list_chat_threads");
export const chatMessages = (threadId: string) => invoke<T.ChatMessage[]>("chat_messages", { threadId });
/** Sends a message (a new thread when threadId is null) and starts the answer; resolves with the thread id. */
export const sendChat = (threadId: string | null, text: string) => invoke<string>("send_chat", { threadId, text });
export const stopChat = (threadId: string) => invoke<void>("stop_chat", { threadId });
/** × on a Team Lead chat in the Inbox: it no longer waits for you. */
export const dismissChat = (threadId: string) => invoke<void>("dismiss_chat", { threadId });
export const chatLive = () => invoke<T.ChatStatus[]>("chat_live");
export const chatAgent = () => invoke<T.Member | null>("chat_agent");
export const onChatEvent = (cb: (e: T.ChatEvent) => void): Promise<UnlistenFn> => listen<T.ChatEvent>("chat-event", (e) => cb(e.payload));
export const onChatChanged = (cb: () => void): Promise<UnlistenFn> => listen("chat-changed", () => cb());
