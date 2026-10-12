// GA-109: how code looks. Lezer's tags get D1's syn-* tokens (design/code-editor/README.md's table), the colours are
// only CSS variables that tokens.css sets for both themes (so code follows the t key and Settings → Appearance at
// once), D1's values are the ones in design/tokens.json, and the editor is in the mono font at t-code's size.
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { tags as t, type Tag } from "@lezer/highlight";
import { describe, expect, it } from "vitest";
import { codeClasses, codeHighlightStyle, codeTheme, highlightCode, SYNTAX } from "./codeTheme";
import { tokensCss, type Tokens } from "./tokensCss";
import { php } from "@codemirror/lang-php";

// Vitest hands a .css file back empty, even with ?raw, so files are read from disk. node:fs comes in through a computed
// specifier: the build type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const read = (path: string): string => fs.readFileSync(new URL(`../../${path}`, import.meta.url), "utf8");
const tokensFile = read("src/styles/tokens.css");
const tokensJson = JSON.parse(read("design/tokens.json")) as Tokens & { color: { tokens: { name: string; value: unknown }[] } };
const d1 = JSON.parse(read("design/code-editor/new-tokens.json")) as { tokens: { name: string; value: unknown; usage: string }[] };

/** `--name: value;` declarations of the block whose selector is exactly `selector`. */
function block(css: string, selector: string): Map<string, string> {
  const at = css.indexOf(`${selector} {`);
  if (at < 0) throw new Error(`no ${selector} block`);
  const body = css.slice(css.indexOf("{", at) + 1, css.indexOf("}", at));
  return new Map([...body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1] ?? "", (m[2] ?? "").trim()]));
}
const dark = block(tokensFile, ':root, [data-theme="dark"]');
const light = block(tokensFile, '[data-theme="light"]');

/** The colour a variable has in a theme: light sets only what differs, the rest comes from :root. */
function colour(theme: "dark" | "light", name: string): string {
  const value = (theme === "light" ? light.get(name) : undefined) ?? dark.get(name);
  if (value === undefined) throw new Error(`${name} isn't in tokens.css`);
  const alias = /^var\((--[\w-]+)\)$/.exec(value);
  return alias ? colour(theme, alias[1] ?? "") : value;
}

/** WCAG 2's contrast between two #rrggbb colours. */
function contrast(a: string, b: string): number {
  const lum = (hex: string) => {
    const [r, g, bl] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (bl ?? 0);
  };
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return ((x ?? 0) + 0.05) / ((y ?? 0) + 0.05);
}

const SYN = SYNTAX.map((s) => s.token);
const DIFF_TEXT = ["diff-add", "diff-del"];

describe("which tag gets which token", () => {
  // design/code-editor/README.md: "Which Lezer tags get which token"
  const TABLE: [Tag, string][] = [
    [t.keyword, "keyword"], [t.controlKeyword, "keyword"], [t.definitionKeyword, "keyword"], [t.moduleKeyword, "keyword"],
    [t.operatorKeyword, "keyword"], [t.self, "keyword"], [t.atom, "keyword"], [t.modifier, "keyword"],
    [t.meta, "meta"], [t.processingInstruction, "meta"], [t.documentMeta, "meta"], [t.annotation, "meta"], [t.escape, "meta"],
    [t.special(t.brace), "meta"],
    [t.function(t.variableName), "function"], [t.function(t.propertyName), "function"], [t.macroName, "function"],
    [t.function(t.definition(t.variableName)), "function"],
    [t.tagName, "tag"], [t.standard(t.tagName), "tag"],
    [t.typeName, "type"], [t.className, "type"], [t.namespace, "type"], [t.special(t.variableName), "type"],
    [t.string, "string"], [t.character, "string"], [t.attributeValue, "string"], [t.docString, "string"],
    [t.special(t.string), "string"], [t.regexp, "string"], [t.url, "string"],
    [t.number, "number"], [t.integer, "number"], [t.float, "number"], [t.bool, "number"], [t.null, "number"],
    [t.comment, "comment"], [t.lineComment, "comment"], [t.blockComment, "comment"], [t.docComment, "comment"],
    [t.attributeName, "attribute"],
    [t.propertyName, "property"], [t.special(t.propertyName), "property"], [t.definition(t.propertyName), "property"],
    [t.variableName, "variable"], [t.definition(t.variableName), "variable"], [t.name, "variable"], [t.labelName, "variable"],
    [t.content, "variable"],
    [t.operator, "operator"], [t.arithmeticOperator, "operator"], [t.logicOperator, "operator"], [t.compareOperator, "operator"],
    [t.definitionOperator, "operator"], [t.function(t.punctuation), "operator"],
    [t.punctuation, "punctuation"], [t.separator, "punctuation"], [t.paren, "punctuation"], [t.brace, "punctuation"],
    [t.squareBracket, "punctuation"], [t.angleBracket, "punctuation"],
    [t.invalid, "invalid"],
  ];

  it.each(TABLE.map(([tag, token]) => [tag.toString(), tag, token] as const))("%s is syn-%s", (_name, tag, token) => {
    expect(codeClasses.style([tag])).toBe(`syn-${token}`);
  });

  it("leaves what isn't in the table without a colour, and Markdown's text as text", () => {
    expect(codeClasses.style([t.inserted])).toBeNull();
    expect(codeClasses.style([t.deleted])).toBeNull();
    expect(codeClasses.style([t.heading])).toBe("syn-variable");
  });
});

describe("the colours", () => {
  const rules = codeHighlightStyle.module!.getRules();

  it("are only the syn-* variables, one for each token", () => {
    const colours = [...rules.matchAll(/color:\s*([^;}]+)/g)].map((m) => (m[1] ?? "").trim());
    expect(new Set(colours)).toEqual(new Set(SYN.map((s) => `var(--syn-${s})`)));
    expect(rules).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(/i);
  });

  it("underline syn-invalid, so it isn't colour alone, and have no italic or bold", () => {
    expect(rules).toMatch(/color:\s*var\(--syn-invalid\);\s*text-decoration:\s*underline wavy/);
    expect(rules).not.toMatch(/font-style|font-weight/);
  });

  it("are set in tokens.css for both themes, and differ between them", () => {
    for (const token of [...SYN.map((s) => `syn-${s}`), ...DIFF_TEXT, "diff-add-soft", "diff-del-soft", "diff-add-strong", "diff-del-strong", "diff-fold"]) {
      const d = colour("dark", `--${token}`), l = colour("light", `--${token}`);
      expect(d, token).toMatch(/^(#[0-9a-f]{6}|rgba\([\d., ]+\))$/);
      expect(l, token).toMatch(/^(#[0-9a-f]{6}|rgba\([\d., ]+\))$/);
      expect(l, token).not.toBe(d);
    }
  });

  it("are D1's values, after c-grey in design/tokens.json, and tokens.css is made from it", () => {
    const names = tokensJson.color.tokens.map((x) => x.name);
    const at = names.indexOf("c-grey") + 1;
    expect(tokensJson.color.tokens.slice(at, at + d1.tokens.length)).toEqual(d1.tokens);
    expect(d1.tokens.map((x) => x.name).sort()).toEqual([...SYN.map((s) => `syn-${s}`), ...DIFF_TEXT, "diff-add-soft", "diff-del-soft", "diff-add-strong", "diff-del-strong", "diff-fold"].sort());
    expect(tokensFile).toBe(tokensCss(tokensJson));
  });

  it("have at least 4.5:1 on bg and raised in both themes", () => {
    for (const theme of ["dark", "light"] as const) {
      for (const token of [...SYN.map((s) => `syn-${s}`), ...DIFF_TEXT]) {
        for (const surface of ["--bg", "--raised"]) {
          expect(contrast(colour(theme, `--${token}`), colour(theme, surface)), `${token} on ${surface} (${theme})`).toBeGreaterThanOrEqual(4.5);
        }
      }
    }
  });

  it("use no teal or magenta in code, and no green or red in syntax but syn-invalid", () => {
    for (const theme of ["dark", "light"] as const) {
      const syntax = SYN.filter((s) => s !== "invalid").map((s) => colour(theme, `--syn-${s}`));
      for (const taken of ["--live", "--needs", "--diff-add", "--diff-del", "--success", "--danger"]) {
        expect(syntax, `${taken} (${theme})`).not.toContain(colour(theme, taken));
      }
      expect(colour(theme, "--syn-invalid")).toBe(colour(theme, "--danger"));
    }
  });
});

describe("the editor", () => {
  const rules = EditorState.create({ extensions: codeTheme }).facet(EditorView.styleModule).map((m) => m.getRules()).join("\n");
  const tCode = tokensJson.type.groups.flatMap((g) => g.styles).find((s) => s.name === "t-code")!;

  it("is in the mono font at t-code's size and line height, growing with the interface size", () => {
    expect(rules).toMatch(/font-family:\s*var\(--font-mono\)/);
    expect(rules).toContain(`font-size: calc(${tCode.fontSize} + var(--ui-grow, 0px))`);
    const lineHeight = Number(/line-height:\s*([\d.]+)\s*[;}]/.exec(rules)?.[1]);
    expect(parseFloat(String(tCode.fontSize)) * lineHeight).toBe(parseFloat(String(tCode.lineHeight)));
  });

  it("takes all its colours from the app's variables", () => {
    expect(rules).not.toMatch(/#[0-9a-f]{3,8}\b|rgba?\(/i);
    for (const [, name] of rules.matchAll(/var\((--[\w-]+)/g)) {
      if (name !== "--ui-grow") expect(tokensFile, name).toContain(`  ${name}: `);
    }
  });

  it("has the code colours with it", () => {
    expect(rules).toContain("var(--syn-keyword)");
  });
});

describe("highlightCode", () => {
  it("lists coloured stretches in order, joins neighbours of one token and skips what has none", () => {
    const code = "<?php\n$a = [1, 2];\n";
    const runs = highlightCode(code, php().language);
    expect(runs.map((r) => [code.slice(r.from, r.to), r.token])).toEqual([
      ["<?php", "meta"], ["$a", "variable"], ["=", "operator"], ["[", "punctuation"], ["1", "number"], [",", "punctuation"],
      ["2", "number"], ["];", "punctuation"],
    ]);
  });
});
