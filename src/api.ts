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
/** The archived cards (the bin) of one project, or of all: the most recently archived first. */
export const listArchivedTasks = (projectId: string | null = null) => invoke<T.Task[]>("list_archived_tasks", { projectId });
/** Archives a card in Done; fails with a plain message for another column or while an agent works on it. */
export const archiveTask = (id: string) => invoke<void>("archive_task", { id });
/** Puts an archived card back at the bottom of its Done column. */
export const restoreTask = (id: string) => invoke<void>("restore_task", { id });
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
/** An agent's own notes in Memory (the Team Lead's Team Lead/Notes is made the first time); null when it has none. */
export const agentNotes = (agentId: string) => invoke<T.MemoryNote | null>("agent_notes", { agentId });
/** Memory for every agent (Settings → Runs). */
export const memoryEnabled = () => invoke<boolean>("memory_enabled");
export const setMemoryEnabled = (on: boolean) => invoke<void>("set_memory_enabled", { on });

export const addFiles = (ownerType: T.FileOwner, ownerId: string, paths: string[]) => invoke<T.AddFilesResult>("add_files", { ownerType, ownerId, paths });
export const listFiles = (ownerType: T.FileOwner, ownerId: string) => invoke<T.FileRow[]>("list_files", { ownerType, ownerId });
export const removeFile = (id: string) => invoke<void>("remove_file", { id });
export const openFile = (id: string) => invoke<void>("open_file", { id });

export const addTeam = (name: string) => invoke<string>("add_team", { name });
export const addAgent = (teamId: string, input: T.AgentInput) => invoke<string>("add_agent", { teamId, input });
export const updateAgent = (actorId: string, input: T.AgentInput) => invoke<void>("update_agent", { actorId, input });
export const setAgentStatus = (actorId: string, status: "active" | "paused") => invoke<void>("set_agent_status", { actorId, status });
/** The agent form's Folders: why each one is refused, or a warning. */
export const checkAgentFolders = (folders: T.AgentFolder[]) => invoke<T.FolderCheck[]>("check_agent_folders", { folders });
export const renameState = (stateId: string, name: string) => invoke<void>("rename_state", { stateId, name });
/** A new column right after `afterId`, Manual and without agents; resolves with its id. `kind`: waiting (like To do), work (like In
 * progress), testing, review, deploy, done or backlog (a category works too). Fails with "this team already has a column called X". */
export const addState = (teamId: string, name: string, afterId: string, kind: string) =>
  invoke<string>("add_state", { teamId, name, afterId, category: kind });
/** Sets up a column: its agents (the full list), Auto or Manual, its next column, its name or its place (afterId). Refuses agents on
 * Backlog, Review, Done and Cancelled, a link to itself and Auto without a next column, each with a reason. */
export const setColumn = (stateId: string, input: T.ColumnInput) => invoke<void>("set_column", { stateId, input });
/** Puts an agent on a column (a drag from the organisation chart, or "+ Agent"). */
export const addColumnAgent = (stateId: string, agentId: string) => invoke<void>("add_column_agent", { stateId, agentId });
/** Takes an agent off a column (×). */
export const removeColumnAgent = (stateId: string, agentId: string) => invoke<void>("remove_column_agent", { stateId, agentId });
/** What removing a column does: its cards (archived ones included), the default target, the columns relinked or unlinked, or why not. */
export const columnRemoval = (stateId: string) => invoke<T.ColumnRemoval>("column_removal", { stateId });
/** Removes a column; its cards move to `targetId`. */
export const removeState = (stateId: string, targetId: string) => invoke<void>("remove_state", { stateId, targetId });
/** Every label with its number of cards. */
export const listLabels = () => invoke<T.LabelInfo[]>("list_labels");
/** Creates a label (id null) or renames or recolours one; resolves with its id. Fails for a name in use, ignoring case. */
export const saveLabel = (id: string | null, name: string, color?: string | null) => invoke<string>("save_label", { id, name, color: color ?? null });
/** Removes a label from every card; resolves with how many cards carried it. */
export const removeLabel = (id: string) => invoke<number>("remove_label", { id });
/** Adds a branch to the team's organisation chart (its role key is made from the name); resolves with the branches. */
export const addBranch = (teamId: string, name: string) => invoke<T.Branch[]>("add_branch", { teamId, name });
/** Removes a branch without agents; resolves with the branches. */
export const removeBranch = (teamId: string, key: string) => invoke<T.Branch[]>("remove_branch", { teamId, key });
export const getAgent = (id: string) => invoke<T.Member>("get_agent", { id });
/** The models this user's Claude Code offers (kept for half an hour; refresh asks Claude Code again). `cli`: another Claude Code
 * CLI (a second account); other kinds of CLI have no list. */
export const claudeModels = (refresh = false, cli: string | null = null) => invoke<T.ModelOption[]>("claude_models", { refresh, cli });
// Settings → MCP servers and the agent form's Tools. Values of environment and header lines go in once and stay in the keychain.
export const listMcpServers = () => invoke<T.McpServerView[]>("list_mcp_servers");
export const saveMcpServer = (input: T.McpServerInput) => invoke<T.McpServerView>("save_mcp_server", { input });
export const removeMcpServer = (id: string) => invoke<void>("remove_mcp_server", { id });
/** Starts or calls the server, lists its tools and stops it (up to 2 minutes: an npx server downloads its package once). */
export const listMcpTools = (id: string) => invoke<T.McpServerView>("list_mcp_tools", { id });
/** The MCP servers in each Claude Code's config file; reads only, starts nothing. */
export const scanClaudeCodeMcp = () => invoke<T.McpScan>("scan_claude_code_mcp");
export const importMcpServers = (picks: T.McpPick[]) => invoke<T.McpServerView[]>("import_mcp_servers", { picks });
/** Opens the sign-in page in your default browser and waits (up to 10 minutes) until you come back. */
export const mcpSignIn = (id: string) => invoke<T.McpServerView>("mcp_sign_in", { id });
export const mcpSignOut = (id: string) => invoke<T.McpServerView>("mcp_sign_out", { id });
export const agentMcp = (agentId: string) => invoke<T.AgentMcpView>("agent_mcp", { agentId });
export const saveAgentMcp = (agentId: string, tools: T.AgentTools) => invoke<T.AgentMcpView>("save_agent_mcp", { agentId, tools });
export const agentCliTools = (agentId: string | null, cliId: string) => invoke<T.ToolsView>("agent_cli_tools", { agentId, cliId });
export const saveAgentCliTools = (agentId: string, tools: T.CliTools) => invoke<T.CliTools>("save_agent_cli_tools", { agentId, tools });
/** Ask Claude Code again: its tools, from a start without a login (nothing spent, nothing written in ~/.claude). */
export const askCliTools = (cliId: string) => invoke<string[]>("ask_cli_tools", { cliId });
export const browserEntry = () => invoke<T.BrowserView>("browser_entry");
export const saveBrowserEntry = (entry: T.BrowserEntry) => invoke<T.BrowserView>("save_browser_entry", { entry });
export const listBrowserTools = () => invoke<T.BrowserView>("list_browser_tools");

/** Settings → Coding CLIs. */
export const listClis = () => invoke<T.CliStatus[]>("list_clis");
export const saveClis = (clis: T.Cli[]) => invoke<T.CliStatus[]>("save_clis", { clis });
/** Known coding CLIs installed here that aren't listed yet. */
export const findClis = () => invoke<T.Cli[]>("find_clis");
export const agentStats = (id: string, days = 14) => invoke<T.DayStat[]>("agent_stats", { id, days });
export const agentRuns = (id: string, limit = 20) => invoke<T.Run[]>("agent_runs", { id, limit });
export const agentNextTask = (id: string) => invoke<string | null>("agent_next_task", { id });
/** The Usage page: tokens and API cost of all runs and chat turns in the period, in total, per day, per agent and per project. */
export const usageSummary = (period: T.UsagePeriod) => invoke<T.Usage>("usage_summary", { period });
/** The Usage page's Subscription tab: per coding CLI, the newest reading of each of its limits and the agents on it. */
export const subscriptionLimits = () => invoke<T.CliLimits[]>("subscription_limits");
export const roleTemplate = (role: string) => invoke<string>("role_template", { role });
/** The allowed commands an agent with this role starts with. */
export const roleTools = (role: string) => invoke<string[]>("role_tools", { role });

export const detectClaude = () => invoke<string | null>("detect_claude");
export const getSettings = () => invoke<T.Settings>("get_settings");
export const saveSettings = (settings: T.Settings) => invoke<void>("save_settings", { settings });
export const startRun = (taskId: string, agentId: string | null) => invoke<string>("start_run", { taskId, agentId });
export const stopRun = (runId: string) => invoke<void>("stop_run", { runId });
/** Continue with your note for the agent, if you wrote one: it goes into the continued run's prompt and on the card as your comment. */
export const continueRun = (runId: string, note?: string | null) => invoke<string>("continue_run", { runId, note: note?.trim() || null });
/** Run this for me: Done, continue. You ran the commands the card's latest run asked for; that run continues. Returns the new run's id. */
export const continueAfterRunForMe = (taskId: string) => invoke<string>("continue_after_run_for_me", { taskId });
export const listRuns = (taskId: string) => invoke<T.Run[]>("list_runs", { taskId });
/** The commits a finished run made, oldest first. */
export const runCommits = (runId: string) => invoke<T.Commit[]>("run_commits", { runId });
export const runEvents = (runId: string) => invoke<T.SeqEvent[]>("run_events", { runId });
export const liveRuns = () => invoke<T.LiveRun[]>("live_runs");
export const suggestAgent = (taskId: string) => invoke<string | null>("suggest_agent", { taskId });
export const onRunEvent = (cb: (e: { runId: string; seq: number; event: T.RunEvent }) => void): Promise<UnlistenFn> =>
  listen<{ runId: string; seq: number; event: T.RunEvent }>("run-event", (m) => cb(m.payload));
export const onRunsChanged = (cb: () => void): Promise<UnlistenFn> => listen("runs-changed", () => cb());

/** Pushes a Review card's branch and opens its pull request: with gh on GitHub, through Bitbucket's API on Bitbucket (keeps an open one). */
export const openPullRequest = (taskId: string) => invoke<T.PullInfo>("open_pull_request", { taskId });
/** Asks GitHub or Bitbucket about the card's pull request now; a merge moves the card to Done. */
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
/** Settings → Bitbucket: your Atlassian email, whether an API token is saved, and who it belongs to (asks Bitbucket). */
export const bitbucketStatus = () => invoke<T.BitbucketStatus>("bitbucket_status");
/** Saves your Atlassian email and API token in your keychain; fails with plain words when Bitbucket refuses them. */
export const bitbucketSaveLogin = (email: string, token: string) => invoke<void>("bitbucket_save_login", { email, token });
/** Removes the email and the token. */
export const bitbucketRemoveLogin = () => invoke<void>("bitbucket_remove_login");
/** Check connection: the token, ssh to Bitbucket, and whether you can push to each project with a Bitbucket link. */
export const bitbucketCheck = () => invoke<T.ConnectionCheck>("bitbucket_check");

/** A local folder: whether it's a git repository, its branch, and its GitHub or Bitbucket remote as a link. */
export type RepoCheck = { isGit: boolean; branch?: string | null; dirty: boolean; github?: string | null; bitbucket?: string | null; suggestCopy?: string[] };
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
/** The chats, newest activity first: the `limit` newest (Chat → Recent), or all of them. */
export const listChatThreads = (limit?: number) => invoke<T.ChatThread[]>("list_chat_threads", { limit: limit ?? null });
/** One chat, also one older than those in Recent (opened from the Archive). */
export const getChatThread = (threadId: string) => invoke<T.ChatThread>("get_chat_thread", { threadId });
/** Chat → Archive: the chats whose title or messages (yours and the Team Lead's) hold `query`, newest first; all for "". */
export const searchChatThreads = (query: string) => invoke<T.ChatHit[]>("search_chat_threads", { query });
export const chatMessages = (threadId: string) => invoke<T.ChatMessage[]>("chat_messages", { threadId });
/** Sends a message (a new thread when threadId is null) and starts the answer; resolves with the thread id. */
/** Sends a message (queued while the Team Lead answers in the chat); a new chat runs on `cli` when one was picked. `files`:
 *  paths of files added to the message, which go with it (the text may be empty then); one that can't be read fails the send. */
export const sendChat = (threadId: string | null, text: string, cli?: string | null, files: string[] = []) =>
  invoke<string>("send_chat", { threadId, text, cli: cli ?? null, files });
/** Which picked or dropped paths can be added to a chat message, and a plain sentence for each that can't (a folder, over 1 GB). */
export const checkFiles = (paths: string[]) => invoke<T.FileCheck>("check_files", { paths });
/** The id of the page a gizai: link opens: a task by identifier (GA-12), a project by key (GA); others name their id already. */
export const itemId = (kind: string, key: string) => invoke<string>("item_id", { kind, key });
export const chatQueue = (threadId: string) => invoke<T.QueuedMessage[]>("chat_queue", { threadId });
export const editQueuedChat = (id: string, text: string) => invoke<T.QueuedMessage>("edit_queued_chat", { id, text });
export const removeQueuedChat = (id: string) => invoke<void>("remove_queued_chat", { id });
/** Send now: the chat's queued messages go together. */
export const sendChatQueue = (threadId: string) => invoke<void>("send_chat_queue", { threadId });
/** Runs on under the text box; null: the Team Lead's Runs on. */
export const setChatCli = (threadId: string, cli: string | null) => invoke<T.ChatThread>("set_chat_cli", { threadId, cli });
/** Answer on <CLI> under a usage-limit note. */
export const answerChatOn = (threadId: string, cli: string, noteId: string) => invoke<void>("answer_chat_on", { threadId, cli, noteId });
export const chatClis = () => invoke<T.ChatCli[]>("chat_clis");
export const stopChat = (threadId: string) => invoke<void>("stop_chat", { threadId });
/** × on a Team Lead chat in the Inbox: it no longer waits for you. */
export const dismissChat = (threadId: string) => invoke<void>("dismiss_chat", { threadId });
export const chatLive = () => invoke<T.ChatStatus[]>("chat_live");
export const chatAgent = () => invoke<T.Member | null>("chat_agent");
export const onChatEvent = (cb: (e: T.ChatEvent) => void): Promise<UnlistenFn> => listen<T.ChatEvent>("chat-event", (e) => cb(e.payload));
export const onChatChanged = (cb: () => void): Promise<UnlistenFn> => listen("chat-changed", () => cb());
