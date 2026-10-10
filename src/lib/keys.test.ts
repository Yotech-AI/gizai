import { afterEach, describe, expect, it, vi } from "vitest";
import { isMac, modClick, modKey } from "./keys";

// The user agents of each system's web view: WebKitGTK on Linux, WKWebView on macOS (Apple Silicon says Intel too),
// WebView2 on Windows.
const LINUX = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const MAC = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)";
const WINDOWS = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

function on(userAgent: string) {
  vi.stubGlobal("navigator", { userAgent });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("shortcut names per system (GA-51)", () => {
  it("says Cmd on macOS and Ctrl on Linux and Windows", () => {
    on(MAC);
    expect([isMac(), modKey()]).toEqual([true, "Cmd"]);
    on(LINUX);
    expect([isMac(), modKey()]).toEqual([false, "Ctrl"]);
    on(WINDOWS);
    expect([isMac(), modKey()]).toEqual([false, "Ctrl"]);
  });

  it("says Ctrl where there is no web view, as in the tests", () => {
    vi.stubGlobal("navigator", undefined);
    expect(modKey()).toBe("Ctrl");
  });

  it("takes Cmd+click on macOS, where Ctrl+click is a right click, and Ctrl+click or Super+click elsewhere", () => {
    const ctrl = { ctrlKey: true, metaKey: false };
    const meta = { ctrlKey: false, metaKey: true };
    const none = { ctrlKey: false, metaKey: false };
    on(MAC);
    expect([modClick(ctrl), modClick(meta), modClick(none)]).toEqual([false, true, false]);
    for (const ua of [LINUX, WINDOWS]) {
      on(ua);
      expect([modClick(ctrl), modClick(meta), modClick(none)]).toEqual([true, true, false]);
    }
  });
});
