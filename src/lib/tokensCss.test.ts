import { describe, expect, it } from "vitest";
import { tokensCss } from "./tokensCss";

const t = {
  color: {
    themes: [{ id: "dark" }, { id: "light" }],
    tokens: [
      { name: "bg", value: { dark: "#0f1013", light: "#ffffff" } },
      { name: "needs", value: { dark: "#f472b6", light: "#c0287a" } },
      { name: "st-review", value: "{needs}" },
      { name: "c-blue", value: "#6f97ff" },
    ],
  },
  type: {
    fonts: [], families: { sans: "\"Atkinson Hyperlegible Next\", system-ui, sans-serif" },
    groups: [{ name: "Text", family: "sans", styles: [{ name: "t-body", fontSize: "13.5px", lineHeight: "20px", fontWeight: 400 }] }],
  },
  spacing: { tokens: [{ name: "space-2", value: "8px" }] },
  shadow: { tokens: [{ name: "shadow-pop", value: { dark: "0 12px 32px rgba(0, 0, 0, 0.45)", light: "0 12px 32px rgba(15, 17, 23, 0.14)" } }] },
};

describe("tokensCss", () => {
  const css = tokensCss(t);
  it("puts the first theme on :root and the others under data-theme", () => {
    expect(css).toMatch(/:root, \[data-theme="dark"\] \{[^}]*--bg: #0f1013;/);
    expect(css).toMatch(/\[data-theme="light"\] \{[^}]*--bg: #ffffff;/);
    expect(css).toContain("color-scheme: dark");
    expect(css).toContain("color-scheme: light");
  });
  it("turns aliases into var() and keeps single-value colours on the first theme only", () => {
    expect(css).toContain("--st-review: var(--needs);");
    expect(css).toMatch(/:root, \[data-theme="dark"\] \{[^}]*--c-blue: #6f97ff;/);
    expect(css).not.toMatch(/\[data-theme="light"\] \{[^}]*--c-blue/);
  });
  it("emits lengths, shadows per theme, font families and type classes", () => {
    expect(css).toContain("--space-2: 8px;");
    expect(css).toMatch(/\[data-theme="light"\] \{[^}]*--shadow-pop: 0 12px 32px rgba\(15, 17, 23, 0.14\);/);
    expect(css).toContain('--font-sans: "Atkinson Hyperlegible Next", system-ui, sans-serif;');
    expect(css).toContain(".t-body { font-family: var(--font-sans); font-size: 13.5px; line-height: 20px; font-weight: 400; }");
  });
});
