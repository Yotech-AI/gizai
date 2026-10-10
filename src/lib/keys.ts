// Shortcut names as each system shows them: Cmd on macOS, Ctrl on Linux and Windows. The key handlers take either
// (e.ctrlKey || e.metaKey) and the editor's Mod- keys are Cmd on macOS; a click with the modifier is Cmd+click there,
// as Ctrl+click is a right click on macOS. Read on each call, so a test can fake the system with a stubbed navigator.

/** True in macOS's web view, whose user agent names the Mac ("Macintosh; Intel Mac OS X …", on Apple Silicon too).
 *  Only the user agent: Node, where the tests run, says "MacIntel" in navigator.platform on a Mac but "Node.js/…" as
 *  its user agent, so the tests see Ctrl on every system. */
export function isMac(): boolean {
  if (typeof navigator === "undefined") return false;
  return /Macintosh|Mac OS X/.test(navigator.userAgent || "");
}

/** The shortcut modifier's name, as in `${modKey()}+Enter saves`: "Cmd" on macOS, "Ctrl" on Linux and Windows. */
export function modKey(): "Cmd" | "Ctrl" {
  return isMac() ? "Cmd" : "Ctrl";
}

/** A click with the modifier held: Cmd+click on macOS; Ctrl+click elsewhere (Super+click too, as before). */
export function modClick(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return isMac() ? e.metaKey : e.ctrlKey || e.metaKey;
}
