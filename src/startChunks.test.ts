// GA-178 QA: what a start loads. Vite builds the app as `npm run build` does (vite.config.ts), in memory so dist/ stays as
// it is. The start is the file index.html runs and every file it imports in turn; index.html preloads only those. Every
// page but TasksPage (the board and the Inbox), the self-test (src/selftest.ts), the Memory graph, the Editor's CodeMirror
// parts (merge, search and the language packs the Markdown editor doesn't use) and xterm are outside it, each page and the
// self-test in a file of its own. The start is at most three files (CodeMirror's part, the rest and Rolldown's runtime)
// and smaller than before GA-178: main's index was 1,665 kB with a 328 kB CodeMirror file preloaded beside it.
import { describe, expect, it } from "vitest";
import { build } from "vite";

type Chunk = { type: "chunk"; fileName: string; isEntry: boolean; isDynamicEntry: boolean; facadeModuleId: string | null; moduleIds: string[]; imports: string[]; code: string };
type Asset = { type: "asset"; fileName: string; source: string | Uint8Array };

// node:fs and node:url come in through a computed specifier: the build type-checks tests without Node's types.
const fs = await import(/* @vite-ignore */ ["node", "fs"].join(":"));
const url = await import(/* @vite-ignore */ ["node", "url"].join(":"));
const root: string = url.fileURLToPath(new URL("..", import.meta.url));
const configFile: string = url.fileURLToPath(new URL("../vite.config.ts", import.meta.url));

const BEFORE_BYTES = 1_993_000;
// every page in src/pages but TasksPage, which opens with Gizai
const LAZY_PAGES: string[] = (fs.readdirSync(new URL("./pages", import.meta.url)) as string[])
  .filter((f) => /^[A-Z]\w*\.tsx$/.test(f) && f !== "TasksPage.tsx").map((f) => `src/pages/${f}`);

// module ids with forward slashes, so the checks read the same on Windows
const slash = (id: string) => id.replace(/\\/g, "/");
const isModule = (id: string, path: string) => slash(id).endsWith(`/${path}`);

const out = (await build({ root, configFile, logLevel: "silent", build: { write: false } })) as unknown as { output: (Chunk | Asset)[] } | { output: (Chunk | Asset)[] }[];
const output = (Array.isArray(out) ? out : [out]).flatMap((o) => o.output);
const chunks = output.filter((o): o is Chunk => o.type === "chunk");
const byFile = new Map(chunks.map((c) => [c.fileName, c]));
const entry = chunks.find((c) => c.isEntry)!;
const start = new Set<string>();
const walk = (file: string) => { if (start.has(file)) return; start.add(file); byFile.get(file)?.imports.forEach(walk); };
walk(entry.fileName);
const startModules = [...start].flatMap((f) => byFile.get(f)!.moduleIds.map(slash));
const html = output.find((o): o is Asset => o.type === "asset" && o.fileName === "index.html")!;
const htmlText = typeof html.source === "string" ? html.source : new TextDecoder().decode(html.source);
const chunkOf = (path: string) => chunks.find((c) => c.moduleIds.some((id) => isModule(id, path)));

describe("the start (npm run build)", () => {
  it("index.html runs the entry and preloads only files of the start", () => {
    expect(htmlText).toContain(`src="/${entry.fileName}"`);
    const preloads = [...htmlText.matchAll(/rel="modulepreload"[^>]*href="\/([^"]+)"/g)].map((m) => m[1]);
    expect(preloads.length).toBeGreaterThan(0);
    for (const file of preloads) expect(start, file).toContain(file);
    expect(htmlText).not.toMatch(/selftest/);
  });

  it("holds the board and the Inbox (TasksPage), and not the self-test", () => {
    expect(startModules.some((id) => isModule(id, "src/pages/TasksPage.tsx"))).toBe(true);
    expect(startModules.some((id) => isModule(id, "src/App.tsx"))).toBe(true);
    expect(startModules.filter((id) => isModule(id, "src/selftest.ts"))).toEqual([]);
  });

  it("puts the self-test in a file of its own, loaded only by import()", () => {
    const own = chunkOf("src/selftest.ts")!;
    expect(own).toBeDefined();
    expect(start.has(own.fileName)).toBe(false);
    expect(own.isDynamicEntry).toBe(true);
    expect(isModule(own.facadeModuleId ?? "", "src/selftest.ts")).toBe(true);
    // no file imports it statically
    expect(chunks.filter((c) => c.imports.includes(own.fileName)).map((c) => c.fileName)).toEqual([]);
  });

  it.each(LAZY_PAGES)("puts %s in a file of its own, outside the start", (page) => {
    expect(startModules.filter((id) => isModule(id, page))).toEqual([]);
    const own = chunkOf(page)!;
    expect(own).toBeDefined();
    expect(own.isDynamicEntry).toBe(true);
    expect(isModule(own.facadeModuleId ?? "", page)).toBe(true);
  });

  it("leaves out the Memory graph, the Editor's CodeMirror parts and xterm", () => {
    const left = ["src/components/memory/GraphCanvas.tsx", "/node_modules/d3-force/", "/node_modules/@codemirror/merge/",
      "/node_modules/@codemirror/search/", "/node_modules/@codemirror/lang-php/", "/node_modules/@codemirror/lang-rust/", "/node_modules/@codemirror/lang-sql/",
      "/node_modules/@codemirror/lang-yaml/", "/node_modules/@codemirror/legacy-modes/", "/node_modules/@xterm/xterm/"];
    for (const part of left) expect(startModules.filter((id) => (part.startsWith("src/") ? isModule(id, part) : id.includes(part))), part).toEqual([]);
    expect(start.has(chunkOf("src/components/memory/GraphCanvas.tsx")!.fileName)).toBe(false);
  });

  // GA-178 QA: splitting the drawers was out of scope, so Ctrl+K's palette and every drawer open as before, with no file to load
  it("keeps the sidebar, the command palette, the close guard and every drawer in it", () => {
    for (const part of ["src/components/Sidebar.tsx", "src/components/CommandPalette.tsx", "src/components/CloseGuard.tsx", "src/lib/drawers.tsx",
      "src/components/NewTaskDrawer.tsx", "src/components/ProjectForm.tsx", "src/components/ClientForm.tsx", "src/components/PersonDrawer.tsx",
      "src/components/AgentForm.tsx", "src/components/DocDrawer.tsx", "src/router.ts"]) expect(startModules.some((id) => isModule(id, part)), part).toBe(true);
  });

  it("is at most three files, smaller than before GA-178", () => {
    expect(start.size).toBeLessThanOrEqual(3);
    const bytes = [...start].reduce((n, f) => n + new TextEncoder().encode(byFile.get(f)!.code).length, 0);
    expect(bytes).toBeLessThan(BEFORE_BYTES);
  });
});
