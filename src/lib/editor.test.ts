// GA-113 QA: what the Editor page works out apart from the screen (src/lib/editor.ts): the folders you opened and each
// folder's tabs as kept in localStorage, paths and typed names, the file's indentation, git's marks, the tree's rows, the
// tabs' names and order, what a tab does about an outside change, the status bar and file bar's words, search in the
// folder, Go to file, the folder picker's groups and a folder's standing in the sidebar.
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DirEntry, EditorFolder, FolderGit, SearchHit } from "../types";
import {
  diskAction, cursorLabel, endingsLabel, foldersOf, forgetFolder, hitParts, hitsByFile, holdsChanges, ignoredFolderOf, indentOf,
  keepTabs, keptTabs, loadTabs, markOf, matchFiles, MAX_OPENED, nameOf, nextAfterClose, openedOf, OPENED_KEY, parentOf,
  pickerGroups, readOnlyBar, readOnlyStatus, rememberFolder, searchSummary, sizeLabel, standingTip, standingWord, stepTab,
  tabNames, TABS_KEY, treeRows, typedPath, unreadable, WHY, withOpened, withoutOpened, withTabs, type Level,
} from "./editor";

const file = (path: string, extra: Partial<DirEntry> = {}): DirEntry =>
  ({ name: nameOf(path), path, dir: false, size: 10, ignored: false, git: null, ...extra });
const folder = (path: string, extra: Partial<DirEntry> = {}): DirEntry => file(path, { dir: true, size: 0, ...extra });
const git = (files: FolderGit["files"], uncommitted = Object.keys(files).length): FolderGit => ({ branch: "main", files, uncommitted });

/** A localStorage for the tests, kept in a Map. */
function fakeStorage() {
  const m = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => m.get(k) ?? null,
    setItem: (k: string, v: string) => { m.set(k, v); },
    removeItem: (k: string) => { m.delete(k); },
  });
  return m;
}

describe("the folders you opened, as kept in localStorage (GA-113)", () => {
  it("is kept under gizai.editor.opened, at most 8", () => {
    expect(OPENED_KEY).toBe("gizai.editor.opened");
    expect(MAX_OPENED).toBe(8);
  });
  it("is none on a computer that never opened one, and for something unreadable", () => {
    expect(openedOf(null)).toEqual([]);
    expect(openedOf("")).toEqual([]);
    expect(openedOf("{not json")).toEqual([]);
    expect(openedOf('{"a":1}')).toEqual([]);
    expect(openedOf('"/home/you/x"')).toEqual([]);
  });
  it("reads the list, leaving out what isn't a folder id", () => {
    expect(openedOf('["/a","",3,null,"/b"]')).toEqual(["/a", "/b"]);
  });
  it("reads at most 8", () => {
    const ten = Array.from({ length: 10 }, (_, i) => `/f${i}`);
    expect(openedOf(JSON.stringify(ten))).toEqual(ten.slice(0, 8));
  });
  it("puts a folder you open at the top", () => {
    expect(withOpened(null, "/a")).toBe('["/a"]');
    expect(withOpened('["/a","/b"]', "/c")).toBe('["/c","/a","/b"]');
  });
  it("lists a folder once: opening it again moves it to the top", () => {
    expect(withOpened('["/a","/b","/c"]', "/c")).toBe('["/c","/a","/b"]');
    expect(withOpened('["/a"]', "/a")).toBe('["/a"]');
  });
  it("drops the oldest beyond 8", () => {
    const eight = Array.from({ length: 8 }, (_, i) => `/f${i}`);
    expect(JSON.parse(withOpened(JSON.stringify(eight), "/new"))).toEqual(["/new", ...eight.slice(0, 7)]);
  });
  it("starts over from something unreadable", () => {
    expect(withOpened("{oops", "/a")).toBe('["/a"]');
  });
  it("takes a folder off the list on Remove from the list, the others as they were", () => {
    expect(withoutOpened('["/a","/b","/c"]', "/b")).toBe('["/a","/c"]');
    expect(withoutOpened('["/a"]', "/x")).toBe('["/a"]');
    expect(withoutOpened(null, "/a")).toBe("[]");
  });
});

describe("the tabs kept per folder (GA-113)", () => {
  it("is kept under gizai.editor.tabs", () => {
    expect(TABS_KEY).toBe("gizai.editor.tabs");
  });
  it("is no tabs for a folder with none kept, and for something unreadable", () => {
    expect(keptTabs(null, "/a")).toEqual({ paths: [], active: null });
    expect(keptTabs('{"/b":{"paths":["x.ts"],"active":"x.ts"}}', "/a")).toEqual({ paths: [], active: null });
    expect(keptTabs("{oops", "/a")).toEqual({ paths: [], active: null });
    expect(keptTabs('{"/a":{"paths":"x.ts"}}', "/a")).toEqual({ paths: [], active: null });
    expect(keptTabs("null", "/a")).toEqual({ paths: [], active: null });
  });
  it("gives the folder's tabs in order and the one that was open", () => {
    expect(keptTabs('{"/a":{"paths":["src/a.ts","README.md"],"active":"README.md"}}', "/a")).toEqual({ paths: ["src/a.ts", "README.md"], active: "README.md" });
  });
  it("leaves out paths that aren't paths, and opens the first tab when the open one isn't among them", () => {
    expect(keptTabs('{"/a":{"paths":["a.ts","",4,"b.ts"],"active":"gone.ts"}}', "/a")).toEqual({ paths: ["a.ts", "b.ts"], active: "a.ts" });
    expect(keptTabs('{"/a":{"paths":["a.ts","b.ts"],"active":null}}', "/a")).toEqual({ paths: ["a.ts", "b.ts"], active: "a.ts" });
    expect(keptTabs('{"/a":{"paths":[],"active":"a.ts"}}', "/a")).toEqual({ paths: [], active: null });
  });
  it("keeps a folder's tabs next to the other folders'", () => {
    const one = withTabs(null, "/a", { paths: ["a.ts"], active: "a.ts" });
    expect(JSON.parse(one)).toEqual({ "/a": { paths: ["a.ts"], active: "a.ts" } });
    const two = withTabs(one, "/b", { paths: ["b.ts", "c.ts"], active: "c.ts" });
    expect(JSON.parse(two)).toEqual({ "/a": { paths: ["a.ts"], active: "a.ts" }, "/b": { paths: ["b.ts", "c.ts"], active: "c.ts" } });
    expect(keptTabs(two, "/b")).toEqual({ paths: ["b.ts", "c.ts"], active: "c.ts" });
  });
  it("leaves a folder without tabs out", () => {
    const kept = JSON.stringify({ "/a": { paths: ["a.ts"], active: "a.ts" }, "/b": { paths: ["b.ts"], active: "b.ts" } });
    expect(JSON.parse(withTabs(kept, "/a", { paths: [], active: null }))).toEqual({ "/b": { paths: ["b.ts"], active: "b.ts" } });
    expect(withTabs(null, "/a", { paths: [], active: null })).toBe("{}");
  });
  it("starts over from unreadable JSON", () => {
    expect(JSON.parse(withTabs("{oops", "/a", { paths: ["a.ts"], active: "a.ts" }))).toEqual({ "/a": { paths: ["a.ts"], active: "a.ts" } });
    expect(JSON.parse(withTabs("null", "/a", { paths: ["a.ts"], active: "a.ts" }))).toEqual({ "/a": { paths: ["a.ts"], active: "a.ts" } });
  });
  it("starts over from something kept that isn't a list of folders (an array, a number)", () => {
    expect(JSON.parse(withTabs("[]", "/a", { paths: ["a.ts"], active: "a.ts" }))).toEqual({ "/a": { paths: ["a.ts"], active: "a.ts" } });
    expect(() => withTabs("5", "/a", { paths: ["a.ts"], active: "a.ts" })).not.toThrow();
  });
});

describe("what is kept on this computer, through localStorage (GA-113)", () => {
  afterEach(() => { vi.unstubAllGlobals(); });
  it("remembers the folders you open, newest first, and forgets one on Remove from the list", () => {
    const m = fakeStorage();
    rememberFolder("/a");
    rememberFolder("/b");
    rememberFolder("/a");
    expect(openedOf(m.get(OPENED_KEY) ?? null)).toEqual(["/a", "/b"]);
    forgetFolder("/a");
    expect(openedOf(m.get(OPENED_KEY) ?? null)).toEqual(["/b"]);
  });
  it("keeps and loads each folder's tabs", () => {
    fakeStorage();
    keepTabs("/a", { paths: ["x.ts", "y.ts"], active: "y.ts" });
    keepTabs("/b", { paths: ["z.ts"], active: "z.ts" });
    expect(loadTabs("/a")).toEqual({ paths: ["x.ts", "y.ts"], active: "y.ts" });
    expect(loadTabs("/b")).toEqual({ paths: ["z.ts"], active: "z.ts" });
    keepTabs("/a", { paths: [], active: null });
    expect(loadTabs("/a")).toEqual({ paths: [], active: null });
    expect(loadTabs("/b")).toEqual({ paths: ["z.ts"], active: "z.ts" });
  });
  it("does nothing, and doesn't throw, where there is no localStorage", () => {
    vi.stubGlobal("localStorage", undefined);
    expect(() => { rememberFolder("/a"); forgetFolder("/a"); keepTabs("/a", { paths: ["x.ts"], active: "x.ts" }); }).not.toThrow();
    expect(loadTabs("/a")).toEqual({ paths: [], active: null });
  });
});

describe("paths (GA-113)", () => {
  it("gives the folder a path is in, \"\" at the top", () => {
    expect(parentOf("src/lib/editor.ts")).toBe("src/lib");
    expect(parentOf("src")).toBe("");
    expect(parentOf("README.md")).toBe("");
  });
  it("gives the last part of a path", () => {
    expect(nameOf("src/lib/editor.ts")).toBe("editor.ts");
    expect(nameOf("README.md")).toBe("README.md");
  });
  it("gives the folders a path is in, the top one first", () => {
    expect(foldersOf("a/b/c.ts")).toEqual(["a", "a/b"]);
    expect(foldersOf("vendor/laravel/framework/x.php")).toEqual(["vendor", "vendor/laravel", "vendor/laravel/framework"]);
    expect(foldersOf("c.ts")).toEqual([]);
  });
  it("makes a typed name a path in the folder it is typed in", () => {
    expect(typedPath("src", "new.ts")).toBe("src/new.ts");
    expect(typedPath("", "new.ts")).toBe("new.ts");
    expect(typedPath("src", "lib/new.ts")).toBe("src/lib/new.ts");
  });
  it("starts at the folder's top for a leading /", () => {
    expect(typedPath("src/lib", "/top.ts")).toBe("top.ts");
    expect(typedPath("src/lib", "/docs/a.md")).toBe("docs/a.md");
  });
  it("tidies slashes, backslashes and the spaces around them", () => {
    expect(typedPath("src", "  a / b.ts  ")).toBe("src/a/b.ts");
    expect(typedPath("src", "a//b.ts")).toBe("src/a/b.ts");
    expect(typedPath("src", "a\\b.ts")).toBe("src/a/b.ts");
    expect(typedPath("src", "lib/")).toBe("src/lib");
  });
  it("is null when nothing is left", () => {
    expect(typedPath("src", "")).toBeNull();
    expect(typedPath("src", "   ")).toBeNull();
    expect(typedPath("src", "/")).toBeNull();
    expect(typedPath("src", " / // ")).toBeNull();
  });
});

describe("how the file already indents (GA-113)", () => {
  it("is 4 spaces when nothing is indented yet", () => {
    expect(indentOf("")).toBe("    ");
    expect(indentOf("one\ntwo\n")).toBe("    ");
  });
  it("is the fewest spaces a line starts with", () => {
    expect(indentOf("a {\n  b {\n    c;\n  }\n}\n")).toBe("  ");
    expect(indentOf("a:\n    b\n        c\n")).toBe("    ");
    expect(indentOf("a\n   b\n      c\n")).toBe("   ");
  });
  it("leaves out a single space (a comment's star) and lines of only spaces", () => {
    expect(indentOf("/**\n * x\n */\nf() {\n  y;\n}\n")).toBe("  ");
    expect(indentOf("a\n  \n   b\n")).toBe("   ");
  });
  it("is a tab when the lines start with tabs", () => {
    expect(indentOf("func a() {\n\tb()\n\tc()\n}\n")).toBe("\t");
  });
  it("is a tab with 3 or more tab lines even when some start with spaces, else the spaces", () => {
    expect(indentOf("a\n\tb\n\tc\n\td\n  e\n")).toBe("\t");
    expect(indentOf("a\n\tb\n  c\n")).toBe("  ");
  });
  it("is 4 when the fewest spaces are more than 8", () => {
    expect(indentOf("a\n          b\n")).toBe("    ");
  });
  it("reads lines that end with CRLF", () => {
    expect(indentOf("a\r\n  b\r\n")).toBe("  ");
  });
});

describe("git's marks (GA-113)", () => {
  it("takes a file's mark from the folder's git status when Gizai has it", () => {
    const g = git({ "src/a.ts": "modified", "new.ts": "untracked" });
    expect(markOf("src/a.ts", g)).toBe("modified");
    expect(markOf("new.ts", g, null)).toBe("untracked");
  });
  it("trusts the git status over the listing, also when it has no mark", () => {
    const g = git({ "src/a.ts": "modified" });
    expect(markOf("src/a.ts", g, "added")).toBe("modified");
    expect(markOf("src/b.ts", g, "added")).toBeNull();
  });
  it("takes the listing's mark without a git status", () => {
    expect(markOf("src/a.ts", null, "added")).toBe("added");
    expect(markOf("src/a.ts", null, null)).toBeNull();
    expect(markOf("src/a.ts", null)).toBeNull();
  });
  it("puts a dot on a folder something inside changed, at any depth", () => {
    const g = git({ "src/lib/a.ts": "modified" });
    expect(holdsChanges("src", g)).toBe(true);
    expect(holdsChanges("src/lib", g)).toBe(true);
  });
  it("puts no dot on a folder whose name only starts the same, or on one with nothing changed", () => {
    const g = git({ "src/lib/a.ts": "modified" });
    expect(holdsChanges("sr", g)).toBe(false);
    expect(holdsChanges("src/li", g)).toBe(false);
    expect(holdsChanges("docs", g)).toBe(false);
    expect(holdsChanges("src", git({}), "modified")).toBe(false);
  });
  it("follows the listing without a git status", () => {
    expect(holdsChanges("src", null, "modified")).toBe(true);
    expect(holdsChanges("src", null, null)).toBe(false);
    expect(holdsChanges("src", null)).toBe(false);
  });
});

describe("the tree's rows (GA-113)", () => {
  it("says Loading… for a level not loaded yet or loading", () => {
    expect(treeRows(new Map(), new Set())).toEqual([{ kind: "note", dir: "", depth: 0, text: "Loading…", error: false }]);
    expect(treeRows(new Map<string, Level>([["", "loading"]]), new Set())).toEqual([{ kind: "note", dir: "", depth: 0, text: "Loading…", error: false }]);
  });
  it("says why a level can't be read, as an error", () => {
    const levels = new Map<string, Level>([["", { error: "Can't read this folder: permission denied." }]]);
    expect(treeRows(levels, new Set())).toEqual([{ kind: "note", dir: "", depth: 0, text: "Can't read this folder: permission denied.", error: true }]);
  });
  it("says Empty folder for a level with nothing in it", () => {
    expect(treeRows(new Map<string, Level>([["", []]]), new Set())).toEqual([{ kind: "note", dir: "", depth: 0, text: "Empty folder", error: false }]);
  });
  it("lists the open folders' entries under them, one deeper, and the notes of their levels", () => {
    const src = folder("src"), readme = file("README.md"), lib = folder("src/lib"), main = file("src/main.ts"), docs = folder("docs"), empty = folder("src/empty");
    const levels = new Map<string, Level>([
      ["", [src, docs, readme]],
      ["src", [empty, lib, main]],
      ["src/empty", []],
      ["docs", [file("docs/a.md")]],
    ]);
    expect(treeRows(levels, new Set(["src", "src/lib", "src/empty", "README.md"]))).toEqual([
      { kind: "entry", entry: src, depth: 0, inIgnored: false },
      { kind: "entry", entry: empty, depth: 1, inIgnored: false },
      { kind: "note", dir: "src/empty", depth: 2, text: "Empty folder", error: false },
      { kind: "entry", entry: lib, depth: 1, inIgnored: false },
      { kind: "note", dir: "src/lib", depth: 2, text: "Loading…", error: false },
      { kind: "entry", entry: main, depth: 1, inIgnored: false },
      { kind: "entry", entry: docs, depth: 0, inIgnored: false },
      { kind: "entry", entry: readme, depth: 0, inIgnored: false },
    ]);
  });
  it("greys everything inside an ignored folder, down every level, and nothing after it", () => {
    const vendor = folder("vendor", { ignored: true }), laravel = folder("vendor/laravel"), x = file("vendor/laravel/x.php"), a = file("a.ts");
    const levels = new Map<string, Level>([["", [vendor, a]], ["vendor", [laravel]], ["vendor/laravel", [x]]]);
    expect(treeRows(levels, new Set(["vendor", "vendor/laravel"]))).toEqual([
      { kind: "entry", entry: vendor, depth: 0, inIgnored: false },
      { kind: "entry", entry: laravel, depth: 1, inIgnored: true },
      { kind: "entry", entry: x, depth: 2, inIgnored: true },
      { kind: "entry", entry: a, depth: 0, inIgnored: false },
    ]);
  });
});

describe("a folder the tree can't read (GA-113)", () => {
  it("says permission denied in its own words", () => {
    expect(unreadable("Permission denied (os error 13)")).toBe("Can't read this folder: permission denied.");
    expect(unreadable("couldn't list src: PERMISSION DENIED")).toBe("Can't read this folder: permission denied.");
  });
  it("gives the backend's reason without what comes before its first colon and without a full stop of its own", () => {
    expect(unreadable("read_dir failed: Not a directory.")).toBe("Can't read this folder: Not a directory.");
    expect(unreadable("No such file or directory (os error 2)")).toBe("Can't read this folder: No such file or directory (os error 2).");
  });
});

describe("the tabs (GA-113)", () => {
  it("names each tab after its file", () => {
    expect(tabNames(["src/a.ts", "b.md"])).toEqual(new Map([
      ["src/a.ts", { name: "a.ts", dir: null }],
      ["b.md", { name: "b.md", dir: null }],
    ]));
  });
  it("adds the folder to two tabs with the same name (the two README.md)", () => {
    expect(tabNames(["docs/README.md", "src/a.ts", "app/README.md"])).toEqual(new Map([
      ["docs/README.md", { name: "README.md", dir: "docs" }],
      ["src/a.ts", { name: "a.ts", dir: null }],
      ["app/README.md", { name: "README.md", dir: "app" }],
    ]));
    expect(tabNames(["README.md", "docs/README.md"]).get("docs/README.md")).toEqual({ name: "README.md", dir: "docs" });
    expect(tabNames(["README.md", "docs/README.md"]).get("README.md")).toEqual({ name: "README.md", dir: null });
  });
  it("opens the tab after the one that closes, else the one before, and none after the last", () => {
    expect(nextAfterClose(["a", "b", "c"], "b")).toBe("c");
    expect(nextAfterClose(["a", "b", "c"], "a")).toBe("b");
    expect(nextAfterClose(["a", "b", "c"], "c")).toBe("b");
    expect(nextAfterClose(["a"], "a")).toBeNull();
    expect(nextAfterClose([], "a")).toBeNull();
  });
  it("steps to the next and the previous tab with Ctrl+Tab and Ctrl+Shift+Tab", () => {
    expect(stepTab(["a", "b", "c"], "a", 1)).toBe("b");
    expect(stepTab(["a", "b", "c"], "b", -1)).toBe("a");
  });
  it("goes round both ways", () => {
    expect(stepTab(["a", "b", "c"], "c", 1)).toBe("a");
    expect(stepTab(["a", "b", "c"], "a", -1)).toBe("c");
    expect(stepTab(["a"], "a", 1)).toBe("a");
    expect(stepTab(["a"], "a", -1)).toBe("a");
  });
  it("starts at the first tab when none is open, and is none without tabs", () => {
    expect(stepTab(["a", "b", "c"], null, 1)).toBe("a");
    expect(stepTab([], null, 1)).toBeNull();
    expect(stepTab([], "a", -1)).toBeNull();
  });
});

describe("a tab and the version on disk (GA-113)", () => {
  const clean = { version: "v1", seen: null, dirty: false };
  const typed = { version: "v1", seen: null, dirty: true };
  it("does nothing while the disk has its own version", () => {
    expect(diskAction(clean, "v1")).toBe("none");
    expect(diskAction(typed, "v1")).toBe("none");
  });
  it("reloads by itself when it has no changes here", () => {
    expect(diskAction(clean, "v2")).toBe("reload");
  });
  it("asks Reload or Keep mine when it has changes here", () => {
    expect(diskAction(typed, "v2")).toBe("ask");
  });
  it("says the file is gone when it has no changes here", () => {
    expect(diskAction(clean, "")).toBe("gone");
  });
  it("offers Save it again for a file gone with changes here", () => {
    expect(diskAction(typed, "")).toBe("gone-ask");
  });
  it("doesn't ask again after Keep mine for the version it already knows of, but does for a newer one", () => {
    const kept = { version: "v1", seen: "v2", dirty: true };
    expect(diskAction(kept, "v2")).toBe("none");
    expect(diskAction(kept, "v3")).toBe("ask");
    expect(diskAction(kept, "")).toBe("gone-ask");
  });
});

describe("the status bar (GA-113)", () => {
  it("says where the cursor is, with the selection when there is one", () => {
    expect(cursorLabel(75, 79, 0)).toBe("Ln 75, Col 79");
    expect(cursorLabel(75, 79, 12)).toBe("Ln 75, Col 79 (12 selected)");
    expect(cursorLabel(1, 1, 1)).toBe("Ln 1, Col 1 (1 selected)");
  });
  it("says the line endings, and the byte order mark when the file has one", () => {
    expect(endingsLabel(false, false)).toBe("LF");
    expect(endingsLabel(true, false)).toBe("CRLF");
    expect(endingsLabel(true, true)).toBe("CRLF · UTF-8 with BOM");
    expect(endingsLabel(false, true)).toBe("LF · UTF-8 with BOM");
  });
  it("says a file's size in bytes, KB or MB", () => {
    expect(sizeLabel(512)).toBe("512 bytes");
    expect(sizeLabel(1023)).toBe("1023 bytes");
    expect(sizeLabel(1024)).toBe("1 KB");
    expect(sizeLabel(48 * 1024)).toBe("48 KB");
    expect(sizeLabel(Math.round(3.4 * 1024 * 1024))).toBe("3.4 MB");
    expect(sizeLabel(1024 * 1024)).toBe("1.0 MB");
  });
  it("says why a file is read-only in a few words, for each of the backend's reasons", () => {
    expect(readOnlyStatus(WHY.over1)).toBe("Read-only: over 1 MB");
    expect(readOnlyStatus(WHY.over5)).toBe("Read-only: over 5 MB");
    expect(readOnlyStatus(WHY.binary)).toBe("Read-only: a binary file");
    expect(readOnlyStatus(WHY.ignored)).toBe("Read-only: ignored by git");
    expect(readOnlyStatus(WHY.notUtf8)).toBe("Read-only: not UTF-8");
    expect(readOnlyStatus(WHY.onDisk)).toBe("Read-only: read-only on disk");
  });
  it("gives an agent at work as the backend says it, and just Read-only without a reason", () => {
    expect(readOnlyStatus("QA Agent is working on GA-118")).toBe("Read-only: QA Agent is working on GA-118");
    expect(readOnlyStatus(null)).toBe("Read-only");
    expect(readOnlyStatus("")).toBe("Read-only");
  });
  it("knows the backend's reasons word for word (gizai_agents::editor)", () => {
    expect(WHY).toEqual({ over5: "Over 5 MB", binary: "A binary file", over1: "Over 1 MB", ignored: "Ignored by git", notUtf8: "Not UTF-8 text", onDisk: "Read-only on disk" });
  });
});

describe("the file bar of a read-only file (GA-113)", () => {
  it("says a file over 1 MB is, with its size when known", () => {
    expect(readOnlyBar(WHY.over1, 1258291, null)).toBe("Read-only: this file is over 1 MB (1.2 MB). Change it in your own editor.");
    expect(readOnlyBar(WHY.over1, null, null)).toBe("Read-only: this file is over 1 MB. Change it in your own editor.");
  });
  it("says a file that isn't UTF-8 is kept as it is", () => {
    expect(readOnlyBar(WHY.notUtf8, 10, null)).toBe("Read-only: this file isn't UTF-8 text, and Gizai keeps files as they are.");
  });
  it("names the ignored folder the file is in when the tree knows it", () => {
    expect(readOnlyBar(WHY.ignored, 10, "vendor")).toBe("Read-only: it's in vendor/, which git ignores.");
    expect(readOnlyBar(WHY.ignored, 10, null)).toBe("Read-only: it's in a folder git ignores.");
  });
  it("says a file read-only on disk is", () => {
    expect(readOnlyBar(WHY.onDisk, 10, null)).toBe("Read-only: this file is read-only on disk.");
  });
  it("leaves an agent at work to the folder's bar", () => {
    expect(readOnlyBar("QA Agent is working on GA-118", 10, null)).toBeNull();
    expect(readOnlyBar(null, 10, null)).toBeNull();
  });
});

describe("the ignored folder a file is in (GA-113)", () => {
  const levels = new Map<string, Level>([
    ["", [folder("src"), folder("vendor", { ignored: true }), file("debug.log", { ignored: true })]],
    ["vendor", [folder("vendor/laravel", { ignored: true })]],
    ["src", [folder("src/gen", { ignored: true }), file("src/a.ts")]],
  ]);
  it("is the top ignored folder of the path", () => {
    expect(ignoredFolderOf("vendor/laravel/x.php", levels)).toBe("vendor");
    expect(ignoredFolderOf("vendor/x.php", levels)).toBe("vendor");
    expect(ignoredFolderOf("src/gen/out.ts", levels)).toBe("src/gen");
  });
  it("is null outside an ignored folder, for an ignored file at the top, and when the tree doesn't know", () => {
    expect(ignoredFolderOf("src/a.ts", levels)).toBeNull();
    expect(ignoredFolderOf("debug.log", levels)).toBeNull();
    expect(ignoredFolderOf("node_modules/x/index.js", levels)).toBeNull();
    expect(ignoredFolderOf("vendor/x.php", new Map())).toBeNull();
  });
});

describe("search in the folder (GA-113)", () => {
  const hit = (path: string, line: number, column = 1, text = "x"): SearchHit => ({ path, line, column, text });
  it("groups the hits by file, in the order git found them", () => {
    const a1 = hit("b.ts", 1), b1 = hit("a.ts", 3), a2 = hit("b.ts", 9), c1 = hit("c.ts", 2), b2 = hit("a.ts", 4);
    expect(hitsByFile([a1, b1, a2, c1, b2])).toEqual([
      { path: "b.ts", hits: [a1, a2] },
      { path: "a.ts", hits: [b1, b2] },
      { path: "c.ts", hits: [c1] },
    ]);
    expect(hitsByFile([])).toEqual([]);
  });
  it("says there are no results, and which files aren't searched", () => {
    expect(searchSummary({ hits: [], truncated: false })).toBe("No results. Ignored files (node_modules/, vendor/, .env) aren't searched.");
  });
  it("says the first 2000 are shown when the search was cut there", () => {
    const hits = Array.from({ length: 2000 }, (_, i) => hit(`f${i % 7}.ts`, i));
    expect(searchSummary({ hits, truncated: true })).toBe("2,000 results shown: narrow the search for more.");
  });
  it("says the search stopped after 10 seconds when it was cut before 2000", () => {
    const hits = Array.from({ length: 1500 }, (_, i) => hit("a.ts", i));
    expect(searchSummary({ hits, truncated: true })).toBe("Search stopped after 10 seconds, at 1,500 results: narrow it for the rest.");
  });
  it("counts the results and the files, one or more", () => {
    expect(searchSummary({ hits: [hit("a.ts", 1)], truncated: false })).toBe("1 result in 1 file");
    expect(searchSummary({ hits: [hit("a.ts", 1), hit("a.ts", 2)], truncated: false })).toBe("2 results in 1 file");
    expect(searchSummary({ hits: [hit("a.ts", 1), hit("b.ts", 2), hit("a.ts", 5)], truncated: false })).toBe("3 results in 2 files");
    const many = Array.from({ length: 1234 }, (_, i) => hit(`f${i % 2}.ts`, i));
    expect(searchSummary({ hits: many, truncated: false })).toBe("1,234 results in 2 files");
  });
});

describe("a hit's line as shown (GA-113)", () => {
  const plain = { regex: false, caseSensitive: false };
  const regex = { regex: true, caseSensitive: false };
  it("marks the query's length from the hit's column in a plain search", () => {
    expect(hitParts({ path: "a.ts", line: 1, column: 7, text: "const foo = bar;" }, "foo", plain)).toEqual({ before: "const ", match: "foo", after: " = bar;" });
    expect(hitParts({ path: "a.ts", line: 1, column: 1, text: "foo()" }, "foo", plain)).toEqual({ before: "", match: "foo", after: "()" });
  });
  it("shows the line's own letters as the match, also when the query's case differs", () => {
    expect(hitParts({ path: "a.ts", line: 1, column: 7, text: "const Foo = 1;" }, "FOO", plain)).toEqual({ before: "const ", match: "Foo", after: " = 1;" });
  });
  it("marks what the regular expression finds at the column", () => {
    expect(hitParts({ path: "a.ts", line: 1, column: 3, text: "x fooo y" }, "fo+", regex)).toEqual({ before: "x ", match: "fooo", after: " y" });
    expect(hitParts({ path: "a.ts", line: 1, column: 3, text: "x FOOO y" }, "fo+", regex)).toEqual({ before: "x ", match: "FOOO", after: " y" });
  });
  it("marks nothing when a case-sensitive regular expression doesn't find it there, or doesn't compile", () => {
    expect(hitParts({ path: "a.ts", line: 1, column: 3, text: "x FOOO y" }, "fo+", { regex: true, caseSensitive: true })).toEqual({ before: "x ", match: "", after: "FOOO y" });
    expect(hitParts({ path: "a.ts", line: 1, column: 3, text: "x (a y" }, "(a", regex)).toEqual({ before: "x ", match: "", after: "(a y" });
  });
  it("starts a long line a little before its match, with …", () => {
    const text = `${"a".repeat(60)}needle${"z".repeat(5)}`;
    expect(hitParts({ path: "a.ts", line: 1, column: 61, text }, "needle", plain)).toEqual({ before: `…${"a".repeat(24)}`, match: "needle", after: "zzzzz" });
  });
  it("shows the whole start of a line whose match is in its first 40 letters", () => {
    const text = `${"a".repeat(40)}needle`;
    expect(hitParts({ path: "a.ts", line: 1, column: 41, text }, "needle", plain)).toEqual({ before: "a".repeat(40), match: "needle", after: "" });
  });
  it("shows a tab after the match as two spaces", () => {
    expect(hitParts({ path: "a.go", line: 1, column: 1, text: "foo\tbar" }, "foo", plain)).toEqual({ before: "", match: "foo", after: "  bar" });
  });
  it("marks the match itself on a line indented with tabs (git's column counts a tab as one)", () => {
    // git grep --column: in "\tfoo()" foo starts at column 2.
    expect(hitParts({ path: "a.go", line: 1, column: 2, text: "\tfoo()" }, "foo", plain)).toEqual({ before: "  ", match: "foo", after: "()" });
    expect(hitParts({ path: "a.go", line: 1, column: 3, text: "\t\tbar := foo" }, "bar", plain)).toEqual({ before: "    ", match: "bar", after: " := foo" });
  });
  it("marks what a regular expression finds on a line indented with tabs, widening a tab inside the match too", () => {
    expect(hitParts({ path: "a.php", line: 1, column: 3, text: "\t\treturn $invoice;" }, "ret\\w+", regex)).toEqual({ before: "    ", match: "return", after: " $invoice;" });
    expect(hitParts({ path: "a.php", line: 1, column: 3, text: "\t\tRETURN $x;" }, "return", { regex: true, caseSensitive: true })).toEqual({ before: "    ", match: "", after: "RETURN $x;" });
    // the match itself holds a tab: "=>\t'x'" from column 6
    expect(hitParts({ path: "a.php", line: 1, column: 6, text: "\t'a' =>\t'x'," }, "=>\\s+'x'", regex)).toEqual({ before: "  'a' ", match: "=>  'x'", after: "," });
  });
  it("marks the word with Match case and Words on a line indented with tabs (the toggles only change what git finds)", () => {
    expect(hitParts({ path: "a.go", line: 1, column: 2, text: "\tfoo := foobar" }, "foo", { regex: false, caseSensitive: true })).toEqual({ before: "  ", match: "foo", after: " := foobar" });
    expect(hitParts({ path: "a.go", line: 1, column: 9, text: "\tfoo := foobar" }, "foobar", plain)).toEqual({ before: "  foo := ", match: "foobar", after: "" });
  });
  it("starts a long line indented with tabs a little before its match, with …, counting each tab as one", () => {
    // "needle" is at index 53: 2 tabs, 50 x's, a tab; 24 characters before it are 23 x's and the tab.
    const text = `\t\t${"x".repeat(50)}\tneedle()`;
    expect(hitParts({ path: "a.go", line: 1, column: 54, text }, "needle", plain)).toEqual({ before: `…${"x".repeat(23)}  `, match: "needle", after: "()" });
    // only tabs before the match: the 24 kept are widened, so 48 spaces
    expect(hitParts({ path: "a.go", line: 1, column: 46, text: `${"\t".repeat(45)}needle` }, "needle", plain)).toEqual({ before: `…${" ".repeat(48)}`, match: "needle", after: "" });
  });
  it("marks the match after letters outside the basic plane, git's column counting them as JavaScript does (UTF-16)", () => {
    // the backend turns git's byte column into UTF-16 units: "😀\t" is 3, so needle is at column 4
    expect(hitParts({ path: "a.md", line: 1, column: 4, text: "😀\tneedle" }, "needle", plain)).toEqual({ before: "😀  ", match: "needle", after: "" });
    expect(hitParts({ path: "a.md", line: 1, column: 7, text: "héllo needle" }, "needle", plain)).toEqual({ before: "héllo ", match: "needle", after: "" });
  });
  it("keeps the column within the line", () => {
    expect(hitParts({ path: "a.ts", line: 1, column: 99, text: "short" }, "x", plain)).toEqual({ before: "short", match: "", after: "" });
  });
});

describe("Go to file (GA-113)", () => {
  it("lists the files as they are, unmarked, for an empty query, at most max", () => {
    expect(matchFiles(["b.ts", "a.ts"], "")).toEqual([{ path: "b.ts", marks: [] }, { path: "a.ts", marks: [] }]);
    expect(matchFiles(["b.ts", "a.ts", "c.ts"], "   ", 2)).toEqual([{ path: "b.ts", marks: [] }, { path: "a.ts", marks: [] }]);
  });
  it("leaves out the files that don't hold the letters in order", () => {
    expect(matchFiles(["src/a.ts", "src/b.ts"], "xyz")).toEqual([]);
    expect(matchFiles(["tes.ts"], "set").map((f) => f.path)).toEqual([]);
  });
  it("puts letters in the name before letters in the folders", () => {
    expect(matchFiles(["editor/x.ts", "src/lib/editor.ts"], "editor").map((f) => f.path)).toEqual(["src/lib/editor.ts", "editor/x.ts"]);
  });
  it("puts letters together before letters apart, also in a longer path", () => {
    expect(matchFiles(["lib/a_b_c.ts", "long/folder/name/abc.ts"], "abc").map((f) => f.path)).toEqual(["long/folder/name/abc.ts", "lib/a_b_c.ts"]);
  });
  it("puts shorter paths first when they match as well, then by path", () => {
    expect(matchFiles(["src/components/app.ts", "src/app.ts"], "app").map((f) => f.path)).toEqual(["src/app.ts", "src/components/app.ts"]);
    expect(matchFiles(["b/app.ts", "a/app.ts"], "app").map((f) => f.path)).toEqual(["a/app.ts", "b/app.ts"]);
  });
  it("marks the matched positions in the path, in the name when they are there", () => {
    expect(matchFiles(["src/editor.ts"], "edt")).toEqual([{ path: "src/editor.ts", marks: [4, 5, 7] }]);
    expect(matchFiles(["src/lib/editor.ts"], "sle")).toEqual([{ path: "src/lib/editor.ts", marks: [0, 4, 8] }]);
  });
  it("ignores case and spaces in the query", () => {
    expect(matchFiles(["src/Editor.ts"], " E dT ")).toEqual([{ path: "src/Editor.ts", marks: [4, 5, 7] }]);
  });
  it("gives at most max", () => {
    expect(matchFiles(["a1.ts", "a2.ts", "a3.ts"], "a", 2)).toHaveLength(2);
    const many = Array.from({ length: 100 }, (_, i) => `f${i}.ts`);
    expect(matchFiles(many, "f")).toHaveLength(60);
  });
});

describe("the folder picker's groups (GA-113)", () => {
  const ef = (kind: EditorFolder["kind"], path: string, name = nameOf(path)): EditorFolder =>
    ({ id: path.replace(/^~/, "/home/you"), path, name, kind, projectId: null, taskId: null, readOnly: false, readOnlyWhy: null });
  const kade = ef("project", "~/Herd/kade-portal");
  const invoice = ef("root", "~/Herd/invoice-api");
  const site = ef("root", "~/Herd/gizai-site");
  const tool = ef("root", "~/Code/tool");
  const thing = ef("root", "~/Other/thing");
  const ga12 = ef("card", "~/.gizai/worktrees/GA/GA-12", "GA-12");
  const ga13 = ef("card", "~/.gizai/worktrees/GA/GA-13", "GA-13");
  const ga14 = ef("card", "~/.gizai/worktrees/GA/GA-14", "GA-14");
  const all = [ga12, invoice, kade, thing, ga13, site, tool, ga14];
  const titles = (gs: { title: string }[]) => gs.map((g) => g.title);

  it("lists Projects, one In <folder> per Editor folder, the folders under another parent, then Cards", () => {
    expect(pickerGroups(all, ["~/Herd", "~/Code"], "")).toEqual([
      { title: "Projects", folders: [kade] },
      { title: "In ~/Herd", folders: [invoice, site] },
      { title: "In ~/Code", folders: [tool] },
      { title: "In ~/Other", folders: [thing] },
      { title: "Cards", folders: [ga12, ga13, ga14] },
    ]);
  });
  it("writes an Editor folder without the slashes at its end", () => {
    expect(titles(pickerGroups([invoice], ["~/Herd/"], ""))).toEqual(["In ~/Herd"]);
    expect(pickerGroups([invoice], ["~/Herd//"], "")[0]!.folders).toEqual([invoice]);
  });
  it("places folders under a Windows Editor folder written with backslashes", () => {
    const win = ef("root", "C:\\Users\\you\\Herd\\app");
    expect(pickerGroups([win], ["C:\\Users\\you\\Herd\\"], "")).toEqual([{ title: "In C:\\Users\\you\\Herd", folders: [win] }]);
  });
  it("puts a folder under another parent with others of that parent", () => {
    const other2 = ef("root", "~/Other/more");
    expect(pickerGroups([thing, other2], [], "")).toEqual([{ title: "In ~/Other", folders: [thing, other2] }]);
  });
  it("lists the cards to list first first, the others after in their order", () => {
    const cards = pickerGroups(all, [], "", (f) => f === ga14).find((g) => g.title === "Cards");
    expect(cards!.folders).toEqual([ga14, ga12, ga13]);
  });
  it("filters every group by name and path, whatever the case", () => {
    expect(pickerGroups(all, ["~/Herd", "~/Code"], "KADE")).toEqual([{ title: "Projects", folders: [kade] }]);
    expect(pickerGroups(all, ["~/Herd", "~/Code"], " herd ")).toEqual([
      { title: "Projects", folders: [kade] },
      { title: "In ~/Herd", folders: [invoice, site] },
    ]);
    expect(pickerGroups(all, ["~/Herd"], "ga-13")).toEqual([{ title: "Cards", folders: [ga13] }]);
    expect(pickerGroups(all, ["~/Herd"], "worktrees/GA")).toEqual([{ title: "Cards", folders: [ga12, ga13, ga14] }]);
  });
  it("drops the groups with nothing in them, also an Editor folder with no folders", () => {
    expect(titles(pickerGroups([tool], ["~/Herd", "~/Code"], ""))).toEqual(["In ~/Code"]);
    expect(pickerGroups([], ["~/Herd"], "")).toEqual([]);
    expect(pickerGroups(all, ["~/Herd"], "nothing like it")).toEqual([]);
  });
});

describe("a folder's standing in the sidebar (GA-113)", () => {
  it("says how many files changed in one word", () => {
    expect(standingWord(git({ "a.ts": "modified" }, 4))).toBe("4 changed");
    expect(standingWord(git({}, 1234))).toBe("1,234 changed");
  });
  it("says nothing with nothing changed or no git status", () => {
    expect(standingWord(git({}, 0))).toBe("");
    expect(standingWord(null)).toBe("");
  });
  it("tells the files changed, not committed, one or more, in its tooltip", () => {
    expect(standingTip(git({}, 4))).toBe("4 files changed, not committed");
    expect(standingTip(git({}, 1))).toBe("1 file changed, not committed");
    expect(standingTip(git({}, 0))).toBe("");
    expect(standingTip(null)).toBe("");
  });
});
