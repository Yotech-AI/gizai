// Stands in for `claude` in os_test.rs, on every system (Windows starts it through an npm .cmd shim, as npm installs a
// CLI there). Prints the run-ok fixture as a stream, with the prompt read from stdin.
// - FAKE_ARGS_OUT=<file> in its environment: writes its arguments there as JSON first.
// - A prompt with "hang": prints the init line, starts a child that writes the time into FAKE_BEAT=<file> every 100 ms
//   (a dev server the agent left running), and waits until it is ended. "hang detached": the child is detached, as
//   `npm run dev &` or a daemon would be.
const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

if (process.env.FAKE_ARGS_OUT) fs.writeFileSync(process.env.FAKE_ARGS_OUT, JSON.stringify(process.argv.slice(2)));
let prompt = "";
process.stdin.on("data", (d) => (prompt += d));
process.stdin.on("end", () => {
  const lines = fs.readFileSync(path.join(__dirname, "fixtures", "run-ok.jsonl"), "utf8").split("\n").filter(Boolean);
  if (!prompt.includes("hang")) {
    for (const l of lines) console.log(l);
    console.error("fake claude done");
    return;
  }
  console.log(lines[0]);
  const beat = `setInterval(() => require("fs").writeFileSync(${JSON.stringify(process.env.FAKE_BEAT)}, String(Date.now())), 100)`;
  const detached = prompt.includes("detached");
  const child = spawn(process.execPath, ["-e", beat], { detached, stdio: "ignore", windowsHide: true });
  if (detached) child.unref();
  setInterval(() => {}, 1000);
});
