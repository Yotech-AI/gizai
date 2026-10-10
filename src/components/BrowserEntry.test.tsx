// GA-79: Settings → MCP servers, the built-in browser's Browser program field. Its hint gives a full path as the system
// Gizai runs on writes one: Google Chrome's usual place on Windows, told by the web view's user agent (as programHint in
// CliSettings.tsx does), and /usr/bin/chromium on Linux and macOS, as before. The same example as the Rust side's message
// (EXAMPLE_PROGRAM in browser.rs, read as text with Vite's ?raw). Rendered to HTML on the server with the form open: the
// component's state is handed in, in the order of its useState calls (v, edit, busy, err); no data loads.
import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { BrowserView } from "../types";
import browserRs from "../../crates/gizai-agents/src/browser.rs?raw";

const queue: unknown[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init: unknown) => R.useState(queue.length ? queue.shift() : init)) as typeof R.useState;
  return { ...R, default: { ...R, useState }, useState };
});

const { BrowserEntryRow, browserProgramHint } = await import("./BrowserEntry");

// The user agents of each system's web view: WebKitGTK on Linux, WKWebView on macOS, WebView2 on Windows.
const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WINDOWS = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

const WINDOWS_HINT = "A full path, like C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe. Brave's is refused.";
const BEFORE = "A full path, like /usr/bin/chromium. Brave's is refused.";

const view: BrowserView = {
  id: "chrome-devtools", version: "1.10.1", program: "", command: "npx -y chrome-devtools-mcp@1.10.1 --headless --isolated",
  needs: { node: "/usr/bin/node", nodeVersion: "v24.1.0", npx: "/usr/bin/npx", browser: "/usr/bin/chromium", browserName: "Chromium", missing: [] },
  usedBy: [],
};

function on(userAgent: string) {
  vi.stubGlobal("navigator", { userAgent });
}

/** The hint under Browser program, with the form open. */
function hint() {
  queue.push(view, { version: "1.10.1", program: "" }, "", null);
  let html: string;
  try { html = renderToStaticMarkup(<BrowserEntryRow />); } finally { queue.length = 0; }
  const m = /<label>Browser program<input[^>]*\/?>\s*<span class="hint">([^<]*)<\/span>/.exec(html);
  return m?.[1].replace(/&#x27;/g, "'");
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("the Browser program hint (GA-79)", () => {
  it("gives Google Chrome's Windows path with a Windows user agent", () => {
    on(WINDOWS);
    expect(hint()).toBe(WINDOWS_HINT);
    expect(browserProgramHint()).toBe(WINDOWS_HINT);
  });

  it("gives /usr/bin/chromium on Linux and macOS, as before", () => {
    for (const ua of [LINUX, MAC]) {
      on(ua);
      expect(hint()).toBe(BEFORE);
      expect(browserProgramHint()).toBe(BEFORE);
    }
  });

  it("gives /usr/bin/chromium where there is no web view, as in the tests", () => {
    vi.stubGlobal("navigator", undefined);
    expect(browserProgramHint()).toBe(BEFORE);
    on("");
    expect(browserProgramHint()).toBe(BEFORE);
  });

  it("gives the same example as the message for a path that isn't full, on each system", () => {
    expect(browserRs).toContain('#[cfg(windows)]\npub const EXAMPLE_PROGRAM: &str = r"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";');
    expect(browserRs).toContain('#[cfg(not(windows))]\npub const EXAMPLE_PROGRAM: &str = "/usr/bin/chromium";');
    expect(browserRs).toContain("give the browser program as a full path, like {EXAMPLE_PROGRAM}");
  });
});
