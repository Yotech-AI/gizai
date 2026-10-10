// Stands in for `claude` in ask_claude_tree_test.rs on every system (Windows starts it through an npm .cmd shim, Linux
// and macOS through fake-claude-tools-node.sh), never the real one. Like Claude Code without a login, it prints its init
// line (with its tools), then a "Not logged in" result, and exits 1.
// $HOME/mode (the test writes it in the scratch home before it asks):
//   hang      first starts a child that writes the time into $HOME/beat every 100 ms (a server it left running), then
//             prints its init line and waits until it is ended
//   stubborn  as hang, but it and its child ignore SIGTERM: only SIGKILL (on Windows, ending its job) ends them
// Both give up by themselves after a minute, so a failing test leaves nothing running.
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const home = process.env.HOME;
let mode = "";
try {
  mode = fs.readFileSync(path.join(home, "mode"), "utf8").trim();
} catch {}
const init = {
  type: "system", subtype: "init", cwd: home, session_id: "S", model: "claude-opus-5-5",
  tools: ["Task", "Bash", "Read", "WebSearch", "WebFetch", "mcp__gizai__get_overview"], mcp_servers: [],
};
process.stdin.on("data", () => {});
process.stdin.on("end", () => {
  if (mode !== "hang" && mode !== "stubborn") {
    console.log(JSON.stringify(init));
    console.log(JSON.stringify({ type: "result", subtype: "success", is_error: true, result: "Not logged in · Please run /login", session_id: "S" }));
    process.exitCode = 1;
    return;
  }
  const stubborn = mode === "stubborn";
  if (stubborn) process.on("SIGTERM", () => {});
  const beat = path.join(home, "beat");
  const child = (stubborn ? 'process.on("SIGTERM", () => {});' : "")
    + `setInterval(() => require("fs").writeFileSync(${JSON.stringify(beat)}, String(Date.now())), 100);`
    + "setTimeout(() => process.exit(0), 60000);";
  spawn(process.execPath, ["-e", child], { stdio: "ignore", windowsHide: true });
  // its init line only once the child runs: Gizai may end everything as soon as it reads it
  const started = setInterval(() => {
    if (!fs.existsSync(beat)) return;
    clearInterval(started);
    console.log(JSON.stringify(init));
  }, 20);
  setTimeout(() => process.exit(0), 60000);
});
