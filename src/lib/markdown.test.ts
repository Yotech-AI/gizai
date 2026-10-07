import { describe, expect, it } from "vitest";
import { safeUrl } from "./markdown";
describe("safeUrl", () => {
  it("keeps http(s) and mailto links", () => {
    expect(safeUrl("https://gizai.ai/docs")).toBe("https://gizai.ai/docs");
    expect(safeUrl("http://localhost:3000")).toBe("http://localhost:3000");
    expect(safeUrl("mailto:jeffrey@example.com")).toBe("mailto:jeffrey@example.com");
  });
  it("drops every other scheme, including tricks", () => {
    expect(safeUrl("javascript:alert(1)")).toBe("");
    expect(safeUrl(" JaVaScRiPt:alert(1)")).toBe("");
    expect(safeUrl("data:text/html,<b>x</b>")).toBe("");
    expect(safeUrl("file:///etc/passwd")).toBe("");
    expect(safeUrl("tauri://localhost")).toBe("");
    expect(safeUrl("/relative/path")).toBe("");
  });
});
