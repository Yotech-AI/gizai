// The only file that talks to Tauri. Swap this file to change the shell.
import { Channel } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { shared } from "./lib/share";
// Tauri's own invoke and listen, counted in a perf run only (GA-188, lib/perf.ts)
import { invoke as tauriInvoke, listen } from "./lib/perf";
import type * as T from "./types";

// Every call: in a screen's data hook, a read another hook has on its way with the same arguments is shared (GA-190).
const invoke = <R>(cmd: string, args?: Record<string, unknown>): Promise<R> => shared(cmd, args, () => tauriInvoke<R>(cmd, args));

export const appInfo = () => invoke<T.AppInfo>("app_info");
export const selftestReport = (json: unknown) => invoke<void>("selftest_report", { json: JSON.stringify(json) });
export const exitApp = (code = 0) => invoke<void>("exit_app", { code });
// GA-148, the product tour (scripts/tour-capture.sh): a still of the screen, and the steps with what they clicked
export const tourStill = (name: string) => invoke<void>("tour_still", { name });
export const tourSteps = (json: unknown) => invoke<void>("tour_steps", { json: JSON.stringify(json) });
/** GA-222, Linux: the paste guard's self-test step (scripts/ui-test.sh). Puts `clipboard` on the clipboard when given (the
 *  primary selection for PasteGlobalSelection), then makes WebKit paste into the field with the focus, as Ctrl+V ("Paste")
 *  or middle-click ("PasteGlobalSelection") does. Resolves with how often the guard turned an empty clipboard's count of
 *  -1 into 0 before this paste, which WebKit does afterwards; with neither it only gives that count. Refused outside a
 *  self-test run; macOS and Windows have no such command. */
export const selftestPaste = (command: "Paste" | "PasteGlobalSelection" | null, clipboard?: string) =>
  invoke<number>("selftest_paste", { command, clipboard: clipboard ?? null });

// The first start's onboarding (GA-162, GA-164): it waits on a new install until it is finished or skipped.
export const onboardingPending = () => invoke<boolean>("onboarding_pending");
/** Saves your name, the team's and the agents' CLI, then starts the Team Lead's welcome chat: resolves with its thread id, or
 *  null when the Team Lead can't answer (no Claude Code). An empty name or an unknown CLI fails and changes nothing. */
export const finishOnboarding = (input: T.OnboardingInput) => invoke<string | null>("finish_onboarding", { input });
/** Skip setup: the onboarding no longer waits; nothing else changes. */
export const skipOnboarding = () => invoke<void>("skip_onboarding");

// Closing Gizai with unsaved text in an editor (GA-122). The page says when it has some; then closing the window or
// quitting waits for the page's answer: "asking" (the question shows), "close" (go on closing) or "stay".
export type CloseAnswer = "asking" | "close" | "stay";
export const setUnsavedEdits = (unsaved: boolean) => invoke<void>("set_unsaved_edits", { unsaved });
export const closeAnswer = (answer: CloseAnswer) => invoke<void>("close_answer", { answer });
/** The window's close button. Gizai hides the window itself (or waits for `closeAnswer`), so Tauri's own close never runs. */
export const onCloseRequested = (cb: () => void): Promise<UnlistenFn> =>
  getCurrentWindow().onCloseRequested((e) => { e.preventDefault(); cb(); });
/** A request to quit while the page has unsaved text: Gizai waits for `closeAnswer`. */
export const onQuitRequested = (cb: () => void): Promise<UnlistenFn> => listen("quit-requested", () => cb());

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
/** GA-228: the cards a card builds on and those that build on it; add one (by its identifier or id) or take one off. A
 *  refusal (a loop, the card itself, more than 20) says why. */
export const getBuildsOn = (id: string) => invoke<T.BuildsOn>("get_builds_on", { id });
export const addBuildsOn = (id: string, on: string) => invoke<void>("add_builds_on", { id, on });
export const removeBuildsOn = (id: string, on: string) => invoke<void>("remove_builds_on", { id, on });
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
/** The Memory page (GA-68), as you: every note by path, with their text when `text`. */
export const memoryNotes = (text = false) => invoke<T.MemoryNote[]>("memory_notes", { text });
/** A new note at `path` (folder and title); refused when the path is taken or the folder isn't a memory folder. */
export const memoryCreate = (path: string, bodyMd: string) => invoke<T.MemorySaved>("memory_create", { path, bodyMd });
/** Moves or renames a note: `to` is a whole path, or a folder ending in / that keeps its title. Links to it follow. */
export const memoryMove = (id: string, to: string) => invoke<T.MemoryNote>("memory_move", { id, to });
/** Words, "phrases", path:Folder and tag:name. */
export const memorySearch = (query: string, limit?: number) => invoke<T.MemoryHit[]>("memory_search", { query, limit: limit ?? null });
/** Notes by their last saved version, newest first, with who wrote it. */
export const memoryRecent = (limit?: number) => invoke<T.MemoryChange[]>("memory_recent", { limit: limit ?? null });
/** Memory for every agent (Settings → Runs). */
export const memoryEnabled = () => invoke<boolean>("memory_enabled");
export const setMemoryEnabled = (on: boolean) => invoke<void>("set_memory_enabled", { on });
/** Agents ask the Team Lead before you (Settings → Runs, GA-70). */
export const askLeadEnabled = () => invoke<boolean>("ask_lead_enabled");
export const setAskLeadEnabled = (on: boolean) => invoke<void>("set_ask_lead_enabled", { on });

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
// Team templates (GA-226, contract TT): the screens pick a path with the dialog's open or save, these read or write the file.
/** Export's check: each place in the team's template that looks private, with the agent it is in. Writes nothing. */
export const checkTeamExport = (teamId: string) => invoke<T.TemplateWarning[]>("check_team_export", { teamId });
/** Writes the team to `path` as a template, format 1 (`<team name>.gizai-team.json`). */
export const saveTeamTemplate = (teamId: string, path: string) => invoke<void>("save_team_template", { teamId, path });
/** The built-in templates. */
export const teamTemplates = () => invoke<{ key: string; name: string; description: string }[]>("team_templates");
/** What importing the template makes; rejects with why when it is refused. Writes nothing. */
export const previewTeamTemplate = (source: T.TemplateSource) => invoke<T.TemplatePlan>("preview_team_template", { source });
/** Makes a new team called `name` from the template; resolves with the new team's id. */
export const importTeamTemplate = (source: T.TemplateSource, name: string) => invoke<string>("import_team_template", { source, name });
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
export const saveAgentCliTools = (agentId: string, tools: T.CliToolsSave) => invoke<T.CliTools>("save_agent_cli_tools", { agentId, tools });
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
/** The run's events numbered after `afterSeq` (all of them without it), at most the last 500. */
export const runEvents = (runId: string, afterSeq: number | null = null) => invoke<T.SeqEvent[]>("run_events", { runId, afterSeq });
export const liveRuns = () => invoke<T.LiveRun[]>("live_runs");
export const suggestAgent = (taskId: string) => invoke<string | null>("suggest_agent", { taskId });
/** Each live run event, one call per event: they come in batches (`run-events`, at most one every 50 ms per run), in
 *  seq order. */
export const onRunEvent = (cb: (e: { runId: string; seq: number; event: T.RunEvent }) => void): Promise<UnlistenFn> =>
  listen<{ runId: string; events: T.SeqEvent[] }>("run-events", (m) => {
    for (const e of m.payload.events) cb({ runId: m.payload.runId, seq: e.seq, event: e.event });
  });
export const onRunsChanged = (cb: () => void): Promise<UnlistenFn> => listen("runs-changed", () => cb());

/** A card's changed files with their counts: its worktree (uncommitted changes included) against its merge base with main,
 *  else its branch, else the range saved at its merge, else its merge commit on main, else the pull request's diff.
 *  Each file's texts and hunks come from cardFileDiff. */
export const cardChanges = (taskId: string) => invoke<T.CardChanges>("card_changes", { taskId });
/** One changed file: its text at `from` and at `to` (null: the worktree as it is on disk), and git's hunks between them. */
export const cardFileDiff = (taskId: string, path: string, oldPath: string | null, from: string, to: string | null) =>
  invoke<T.FileDiff>("card_file_diff", { taskId, path, oldPath, from, to });
/** The changes one run committed: from the commit it started at to the one it ended at. */
export const runChanges = (runId: string) => invoke<T.CardChanges>("run_changes", { runId });

// ---- Screenshots (GA-182, contract CS) ----
/** The card's screenshot sets, newest run first: every run of the card that got the Screenshots section, also one that
 *  saved none (its `shots` is empty), with what it left out and why its preview didn't start. They arrive when a run ends,
 *  before "runs-changed": reload on onRunsChanged. */
export const listScreenshots = (taskId: string) => invoke<T.ScreenshotSet[]>("list_screenshots", { taskId });
/** One screenshot's bytes (PNG, JPEG or WebP), for a blob: URL. Any id that isn't a run's screenshot is refused. */
export const screenshotBytes = (fileId: string) => invoke<ArrayBuffer>("screenshot_bytes", { fileId });

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
/** Remove anyway: puts the worktree's uncommitted changes in git's stash, then removes it. */
export const removeOldWorktreeAnyway = (taskId: string) => invoke<T.RemovedWorktree>("remove_old_worktree_anyway", { taskId });

// ---- the Editor (GA-107, contract C2): `folder` is always an EditorFolder.id, `path` always relative to it, with / ----
/** The folders the Editor may open: projects' linked folders, the folders under Settings → Editor folders, card worktrees. */
export const editorFolders = () => invoke<T.EditorFolder[]>("editor_folders");
/** One folder level, folders first; `dir` "" for the top. */
export const editorList = (folder: string, dir: string) => invoke<T.DirEntry[]>("editor_list", { folder, dir });
export const editorRead = (folder: string, path: string) => invoke<T.OpenedFile>("editor_read", { folder, path });
/** Saves; refuses with "changed on disk" when the file's version isn't `version` any more. Resolves with the new version. */
export const editorWrite = (folder: string, path: string, text: string, version: string) =>
  invoke<string>("editor_write", { folder, path, text, version });
export const editorCreate = (folder: string, path: string, dir: boolean) => invoke<void>("editor_create", { folder, path, dir });
export const editorRename = (folder: string, from: string, to: string) => invoke<void>("editor_rename", { folder, from, to });
export const editorDelete = (folder: string, path: string) => invoke<void>("editor_delete", { folder, path });
/** Go to file: tracked and untracked files git doesn't ignore. */
export const editorFiles = (folder: string) => invoke<string[]>("editor_files", { folder });
export const editorSearch = (folder: string, query: string, caseSensitive: boolean, wholeWord: boolean, regex: boolean) =>
  invoke<T.SearchResult>("editor_search", { folder, query, caseSensitive, wholeWord, regex });
/** The branch and each changed file's git status (local, no fetch). */
export const editorGit = (folder: string) => invoke<T.FolderGit>("editor_git", { folder });
/** The current version of each path ("" for a file that is gone), to notice outside changes. */
export const editorVersions = (folder: string, paths: string[]) => invoke<string[]>("editor_versions", { folder, paths });
/** VS Code, Zed and PhpStorm, those found on this computer (under any of their names, such as zeditor). */
export const editorApps = () => invoke<T.EditorApp[]>("editor_apps");
/** Opens the folder, or a file in it (at a line when given), in that editor. */
export const openInApp = (app: T.EditorApp["id"], folder: string, path: string | null, line: number | null) =>
  invoke<void>("open_in_app", { app, folder, path, line });

// ---- behind main in the Editor (GA-110, contract C3): how a folder stands against main, Fetch and Update ----
/** How the folder stands against main as last fetched (local only). */
export const editorMain = (folder: string) => invoke<T.FolderMain>("editor_main", { folder });
/** Fetches main first (at most 60 s; not again within a minute), then as editorMain. */
export const editorFetch = (folder: string) => invoke<T.FolderMain>("editor_fetch", { folder });
/** Starts Update in the background: resolves with what it will do, or rejects with why not (nothing changed). */
export const editorUpdate = (folder: string) => invoke<string>("editor_update", { folder });
export const onEditorUpdated = (cb: (e: T.EditorUpdated) => void): Promise<UnlistenFn> =>
  listen<T.EditorUpdated>("editor-updated", (m) => cb(m.payload));

// ---- a person's card (GA-111, contract C4): a card you do by hand, in the Editor or your own editor ----
/** Open in editor, on a card assigned to you: makes and prepares its worktree as for an agent, pins the card to you and moves
 *  it on from To do. Resolves with where it is (its folder is an EditorFolder id). */
export const startByHand = (taskId: string) => invoke<T.HandCard>("start_by_hand", { taskId });
export const handCard = (taskId: string) => invoke<T.HandCard>("hand_card", { taskId });
/** Commits every change in the card's worktree with this message. */
export const handCommit = (taskId: string, message: string) => invoke<T.Commit>("hand_commit", { taskId, message });
/** Pushes the card's branch like Gizai's push after a run; resolves with what it did ("Pushed gizai/ga-123-… (2 commits)."). */
export const handPush = (taskId: string) => invoke<string>("hand_push", { taskId });
/** Refuses uncommitted changes; pushes, moves the card to Testing (Review with its Testing switch off) and takes the pin off.
 *  Resolves with the column's name. */
export const handOver = (taskId: string) => invoke<string>("hand_over", { taskId });

// ---- terminals (GA-133, contract CT): your shell in one of the Editor's folders, kept in memory while Gizai runs ----
/** Every open terminal, in the order they were opened. */
export const terminalList = () => invoke<T.TerminalSession[]>("terminal_list");
/** Starts your shell in one of the Editor's folders (an EditorFolder id), at this size. Refuses, with the Editor's reason,
 *  a folder the Editor can't open or a card's worktree an agent is working on. `name` null: the folder's name, numbered. */
export const terminalOpen = (folder: string, cols: number, rows: number, name: string | null) =>
  invoke<T.TerminalSession>("terminal_open", { folder, cols, rows, name });
/** Sends the terminal's output to `onBytes`: first what it kept in memory (at most 256 KB), then everything new, in order.
 *  One listener per terminal: attaching again replaces the last one. */
export function terminalAttach(id: string, onBytes: (bytes: Uint8Array) => void): Promise<void> {
  const output = new Channel<ArrayBuffer>((b) => onBytes(new Uint8Array(b)));
  return invoke<void>("terminal_attach", { id, output });
}
/** What you typed or pasted (xterm's onData), or with `binary` xterm's onBinary: one byte per character. */
export const terminalInput = (id: string, data: string, binary = false) => invoke<void>("terminal_input", { id, data, binary });
/** The place showing the terminal has this size now; 0 columns or rows keeps the size it has. */
export const terminalResize = (id: string, cols: number, rows: number) => invoke<void>("terminal_resize", { id, cols, rows });
/** xterm.js has written this many more bytes (its write callback; send it every 100,000 bytes or so). Above 500,000 bytes
 *  not yet acknowledged the backend stops reading until it is back under 200,000, and the program waits, as in any terminal. */
export const terminalAck = (id: string, bytes: number) => invoke<void>("terminal_ack", { id, bytes });
/** Trimmed, at most 40 characters; an empty name gives the default back. */
export const terminalRename = (id: string, name: string) => invoke<T.TerminalSession>("terminal_rename", { id, name });
/** Ends the shell and everything it started; resolves when they are gone (at most about 3 s; on Windows at once). */
export const terminalClose = (id: string) => invoke<void>("terminal_close", { id });
/** After an open, a rename, a close, a shell's end and a change of `busy`: every open terminal. */
export const onTerminalsChanged = (cb: (list: T.TerminalSession[]) => void): Promise<UnlistenFn> =>
  listen<T.TerminalSession[]>("terminals-changed", (m) => cb(m.payload));
/** The clipboard's text, through the clipboard plugin (its one right: clipboard-manager:allow-read-text), so pasting
 *  into a terminal with Ctrl+Shift+V on Linux doesn't show WebKit's own Paste prompt. */
export const readClipboardText = () => readText();

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

// ---- backups (GA-216, contract BK): snapshots in a backups folder, on a schedule ----
/** The settings, the backups folder in use and why it can't be used now, the snapshot that runs, the last one, the last
 *  error and when the next scheduled one is due. */
export const backupStatus = () => invoke<T.BackupStatus>("backup_status");
/** Rejects Keep outside 1..100 and a folder that can't be written, saying why. */
export const saveBackupSettings = (settings: T.BackupSettings) => invoke<T.BackupStatus>("save_backup_settings", { settings });
/** The snapshots in a folder, newest first; null: the folder in Settings. */
export const listBackups = (folder: string | null) => invoke<T.BackupEntry[]>("list_backups", { folder });
/** Starts one in the background; rejected without a folder or while one runs. */
export const backUpNow = () => invoke<T.BackupStatus>("back_up_now");
/** A snapshot started, took a step, ended or failed, or the settings changed. */
export const onBackupsChanged = (cb: (s: T.BackupStatus) => void): Promise<UnlistenFn> =>
  listen<T.BackupStatus>("backups-changed", (m) => cb(m.payload));

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
/** GA-102: the Team Lead's calls that wait for your approval in a chat (after a tool from outside Gizai), each with its preview. */
export const pendingActions = (threadId: string) => invoke<T.PendingAction[]>("pending_actions", { threadId });
/** Approve: runs exactly that call, once, with every other rule in force; the chat gets a note with what happened. Fails for a call
 *  that no longer waits (approved, discarded, or replaced by a newer one). */
export const approvePendingAction = (id: string) => invoke<T.PendingAction>("approve_pending_action", { id });
/** Approve all: the calls you saw, one after the other in the order given. */
export const approvePendingActions = (ids: string[]) => invoke<T.PendingAction[]>("approve_pending_actions", { ids });
/** Discard: nothing runs; the chat gets a note with what was discarded. */
export const discardPendingAction = (id: string) => invoke<T.PendingAction>("discard_pending_action", { id });
export const chatLive = () => invoke<T.ChatStatus[]>("chat_live");
export const chatAgent = () => invoke<T.Member | null>("chat_agent");
/** Each event of a chat's answer, one call per event: they come in batches (`chat-events`, at most one every 50 ms per
 *  chat, pieces of text one after the other joined into one). */
export const onChatEvent = (cb: (e: T.ChatEvent) => void): Promise<UnlistenFn> =>
  listen<{ threadId: string; events: T.ChatEvent[] }>("chat-events", (m) => { for (const e of m.payload.events) cb(e); });
export const onChatChanged = (cb: () => void): Promise<UnlistenFn> => listen("chat-changed", () => cb());
