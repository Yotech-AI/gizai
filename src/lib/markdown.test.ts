import { describe, expect, it } from "vitest";
import { safeUrl, viewUrl } from "./markdown";
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

// GA-41: shown Markdown keeps links to Gizai items (the @ picker's), which open inside Gizai, and still drops the rest.
describe("viewUrl", () => {
  it("keeps gizai: links to the six kinds of item", () => {
    for (const url of ["gizai:task/GA-12", "gizai:project/GA", "gizai:client/c1", "gizai:agent/a1", "gizai:person/u1", "gizai:doc/d1"]) {
      expect(viewUrl(url)).toBe(url);
    }
    expect(viewUrl(" gizai:task/GA-12 ")).toBe("gizai:task/GA-12");
  });
  it("keeps what safeUrl keeps and drops the rest, other gizai: links too", () => {
    expect(viewUrl("https://gizai.ai/docs")).toBe("https://gizai.ai/docs");
    expect(viewUrl("mailto:jeffrey@example.com")).toBe("mailto:jeffrey@example.com");
    for (const url of ["javascript:alert(1)", "gizai:board/x", "gizai:task/GA-1/../x", "gizai:", "file:///etc/passwd"]) expect(viewUrl(url)).toBe("");
  });
});
