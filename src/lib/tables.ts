// Which tables each read shows (GA-190; research docs/research/performance.md F1, P2). After a write the backend sends
// `rows-changed` with the table it wrote (src-tauri: commands.rs, runs.rs, chat.rs, tools/, …), and a screen reloads a
// read only when the notice names one of the read's tables. A read that isn't here reloads after any write, as before.
//
// The names are the notices' names, which aren't always the table written: "team" is an agent's MCP servers and tools
// (agent_configs, save_agent_mcp and save_agent_cli_tools), "subscription_limits" a settings row; "actors" also covers
// agent_configs and team_members, "teams" a new team's members and columns, "workflow_states" the columns' agents,
// "projects" the repositories, "docs" a doc's versions, "tasks" a card's labels. Some writes name another table than one
// they also change, so a read lists that notice too: a saved chat message also changes its chat's last activity but sends
// "chat_messages"; a run's start sets its card's branch but sends "runs"; a run's end writes the run but sends "tasks"
// and "comments"; the Team Lead's own notes can appear with a chat answer or a board check ("chat_messages", "runs").

const CARDS = ["tasks", "workflow_states", "projects", "actors", "runs", "labels"];
const CHATS = ["chat_threads", "chat_messages", "pending_actions", "tasks"];
const MEMBERS = ["actors", "team", "teams"];

/** The tables each read command's answer comes from, by command (src/api.ts). */
export const READS: Readonly<Record<string, readonly string[]>> = {
  list_tasks: CARDS,
  get_task: CARDS,
  list_archived_tasks: CARDS,
  task_activity: ["tasks", "comments", "actors", "runs"],
  list_comments: ["comments", "actors"],
  list_teams: ["teams"],
  get_team: ["teams", "team", "actors", "workflow_states", "labels"],
  get_agent: MEMBERS,
  chat_agent: MEMBERS,
  list_projects: ["projects", "clients", "tasks", "runs"],
  get_project: ["projects", "clients", "tasks", "runs"],
  list_users: ["actors", "tasks"],
  list_clients: ["clients", "contacts", "tasks", "projects"],
  get_client: ["clients", "contacts", "tasks", "projects"],
  list_contacts: ["contacts"],
  memory_notes: ["docs", "actors", "runs", "chat_messages"],
  memory_recent: ["docs", "actors", "runs", "tasks"],
  agent_notes: ["docs", "actors", "team", "teams", "tasks", "runs", "chat_messages"],
  list_chat_threads: CHATS,
  get_chat_thread: CHATS,
  list_runs: ["runs", "tasks", "actors"],
  agent_runs: ["runs", "tasks", "actors"],
  agent_stats: ["runs", "tasks"],
  list_files: ["files"],
  list_docs: ["docs"],
  get_doc: ["docs"],
  doc_versions: ["docs", "actors"],
  list_labels: ["labels", "tasks"],
  usage_summary: ["runs", "tasks", "actors", "projects", "teams"],
  subscription_limits: ["subscription_limits", "settings", "actors", "team", "teams", "chat_threads"],
};

/** Of a read's tables, those that only feed counts and sums in it: a project's open and done cards and this month's
 *  cost, a person's open cards, a client's projects and cards. A screen that doesn't show them (the sidebar, the board,
 *  a card's pickers) leaves them out (useData's `counts: false`), so a card's write doesn't reload its projects. */
export const COUNTS: Readonly<Record<string, readonly string[]>> = {
  list_projects: ["tasks", "runs"],
  get_project: ["tasks", "runs"],
  list_users: ["tasks"],
  list_clients: ["tasks", "projects"],
  get_client: ["tasks", "projects"],
};

/** The tables a fetch that called `commands` shows, or null when a write to any table counts: it called a read this
 *  map doesn't know, or none before it returned (it may call one later). `counts: false` leaves out COUNTS. */
export function tablesOf(commands: readonly string[], counts = true): Set<string> | null {
  if (commands.length === 0) return null;
  const out = new Set<string>();
  for (const c of commands) {
    const tables = READS[c];
    if (!tables) return null;
    const skip = counts ? undefined : COUNTS[c];
    for (const t of tables) if (!skip?.includes(t)) out.add(t);
  }
  return out;
}
