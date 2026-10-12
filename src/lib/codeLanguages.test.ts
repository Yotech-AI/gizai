// GA-109: a file's language by its name, loaded the first time a file needs it. Each listed type gets its language and
// colours a sample with it; another name is plain text.
import { describe, expect, it } from "vitest";
import source from "./codeLanguages.ts?raw";
import { CODE_LANGUAGES, languageFor, languageName, loadLanguage, PLAIN_TEXT } from "./codeLanguages";
import { highlightCode } from "./codeTheme";

/** The tokens over `piece` in `code` coloured as the language of `file`, each once, in order ("" where nothing colours
 *  a character). */
async function colours(file: string, code: string, piece: string): Promise<string[]> {
  const support = await loadLanguage(file);
  if (!support) throw new Error(`no language for ${file}`);
  const at = code.indexOf(piece);
  if (at < 0) throw new Error(`no ${piece} in ${code}`);
  const chars: string[] = Array.from({ length: piece.length }, () => "");
  for (const run of highlightCode(code, support.language)) {
    for (let p = Math.max(run.from, at); p < Math.min(run.to, at + piece.length); p++) chars[p - at] = run.token;
  }
  return [...new Set(chars)];
}

describe("loading a language", () => {
  // first in the file, so nothing has loaded SQL yet
  it("loads a language only when a file needs it, once", async () => {
    const sql = languageFor("database/schema.sql")!;
    expect(sql.support).toBeUndefined();
    const first = await loadLanguage("database/schema.sql");
    expect(first).not.toBeNull();
    expect(sql.support).toBe(first);
    expect(await loadLanguage("other.sql")).toBe(first);
  });

  it("imports no language at the start: each one comes in its own chunk", () => {
    const imports = [...source.matchAll(/^import .* from "([^"]+)";$/gm)].map((m) => m[1]);
    expect(imports).toEqual(["@codemirror/language"]);
    expect(source).toContain('import("@codemirror/lang-php")');
    expect(source).toContain('import("./blade")');
  });

  it("gives null for plain text", async () => {
    expect(await loadLanguage("notes.txt")).toBeNull();
    expect(await loadLanguage("Makefile")).toBeNull();
  });
});

describe("languageFor and languageName", () => {
  const NAMES: [string, string][] = [
    ["resources/views/welcome.blade.php", "Blade"],
    ["resources/views/components/task-card.blade.php", "Blade"],
    ["app/Models/Task.php", "PHP"],
    ["templates/old.phtml", "PHP"],
    ["artisan", "PHP"],
    ["src/main.ts", "TypeScript"],
    ["vite.config.mts", "TypeScript"],
    ["src/App.tsx", "TypeScript JSX"],
    ["eslint.config.js", "JavaScript"],
    ["build.mjs", "JavaScript"],
    ["babel.config.cjs", "JavaScript"],
    ["src/Button.jsx", "JavaScript JSX"],
    ["src-tauri/src/main.rs", "Rust"],
    ["src/styles/app.css", "CSS"],
    ["resources/sass/app.scss", "SCSS"],
    ["package.json", "JSON"],
    ["tsconfig.jsonc", "JSON"],
    ["composer.lock", "JSON"],
    ["docker-compose.yml", "YAML"],
    [".github/workflows/ci.yaml", "YAML"],
    ["README.md", "Markdown"],
    ["docs/notes.markdown", "Markdown"],
    ["database/schema.sql", "SQL"],
    ["index.html", "HTML"],
    ["public/old.htm", "HTML"],
    [".env", ".env"],
    [".env.example", ".env"],
    [".env.testing.local", ".env"],
    ["config/app.env", ".env"],
    ["Cargo.toml", "TOML"],
    ["Cargo.lock", "TOML"],
    ["scripts/install.sh", "Shell"],
    ["deploy.bash", "Shell"],
    ["setup.zsh", "Shell"],
    [".bashrc", "Shell"],
    [".zshrc", "Shell"],
    [".profile", "Shell"],
  ];

  it.each(NAMES)("%s is %s", (path, name) => {
    expect(languageFor(path)?.name).toBe(name);
    expect(languageName(path)).toBe(name);
  });

  it("takes Windows paths and any case", () => {
    expect(languageName("resources\\views\\Welcome.Blade.PHP")).toBe("Blade");
    expect(languageName("C:\\Code\\gizai\\README.MD")).toBe("Markdown");
    expect(languageName("SRC/APP.TSX")).toBe("TypeScript JSX");
    expect(languageName(".ENV.LOCAL")).toBe(".env");
  });

  it("shows another name as plain text", () => {
    for (const path of ["Makefile", "LICENSE", "notes.txt", "logo.png", "app.php.bak", ".envrc", "Dockerfile", "archive.", "", "src/"]) {
      expect(languageFor(path), path).toBeNull();
      expect(languageName(path), path).toBe(PLAIN_TEXT);
    }
    expect(PLAIN_TEXT).toBe("Plain text");
  });

  it("has one language per name", () => {
    const names = CODE_LANGUAGES.map((l) => l.name);
    expect(new Set(names).size).toBe(names.length);
  });
});

describe("each language colours its code", () => {
  const SAMPLES: [file: string, code: string, piece: string, tokens: string[]][] = [
    ["welcome.blade.php", "@if($open) <p>{{ $task->title }}</p> @endif", "@if", ["keyword"]],
    ["welcome.blade.php", "@if($open) <p>{{ $task->title }}</p> @endif", "{{", ["meta"]],
    ["Task.php", "<?php\nfunction total(): int { return 1; }", "<?php", ["meta"]],
    ["Task.php", "<?php\nfunction total(): int { return 1; }", "total", ["function"]],
    ["page.php", "<div class=\"x\"><?= $x ?></div>", "class", ["attribute"]],
    ["main.ts", "const n: number = 1; // one", "number", ["type"]],
    ["main.ts", "const n: number = 1; // one", "// one", ["comment"]],
    ["App.tsx", "const a = <Sidebar open={true} />;", "Sidebar", ["tag"]],
    ["App.tsx", "const a = <Sidebar open={true} />;", "open", ["attribute"]],
    ["a.js", "function f() { return 'x'; }", "f()", ["function", "punctuation"]],
    ["a.js", "function f() { return 'x'; }", "'x'", ["string"]],
    ["Button.jsx", "export default () => <div className=\"b\" />;", "className", ["attribute"]],
    ["main.rs", "pub fn f<'a>(x: &'a str) { println!(\"{}\", x); }", "fn", ["keyword"]],
    ["main.rs", "pub fn f<'a>(x: &'a str) { println!(\"{}\", x); }", "'a", ["type"]],
    ["main.rs", "pub fn f<'a>(x: &'a str) { println!(\"{}\", x); }", "println!", ["function"]],
    ["app.css", ".card { color: #fff; }", "color", ["property"]],
    ["app.scss", ".card { .title { margin: 0; } }", "margin", ["property"]],
    ["package.json", "{ \"name\": \"gizai\", \"private\": true }", "\"name\"", ["property"]],
    ["package.json", "{ \"name\": \"gizai\", \"private\": true }", "true", ["number"]],
    ["ci.yml", "on: push # when\njobs: {}", "# when", ["comment"]],
    ["ci.yml", "on: push # when\njobs: {}", "jobs", ["property"]],
    ["README.md", "# Title\n\n```php\n<?php echo $x;\n```\n", "echo", ["keyword"]],
    ["README.md", "```blade\n@foreach($a as $b)\n{{ $b }}\n@endforeach\n```\n", "@foreach", ["keyword"]],
    ["README.md", "```ts\nconst x = 1;\n```\n", "const", ["keyword"]],
    ["schema.sql", "SELECT id FROM tasks WHERE title = 'x'; -- all", "SELECT", ["keyword"]],
    ["schema.sql", "SELECT id FROM tasks WHERE title = 'x'; -- all", "-- all", ["comment"]],
    ["index.html", "<!-- c --><p class=\"x\">hi</p>", "<!-- c -->", ["comment"]],
    ["index.html", "<!-- c --><p class=\"x\">hi</p>", "p", ["tag"]],
    [".env", "APP_NAME=Gizai # the name", "APP_NAME", ["property"]],
    ["Cargo.toml", "[package]\nname = \"gizai\" # c", "\"gizai\"", ["string"]],
    ["Cargo.toml", "[package]\nname = \"gizai\" # c", "# c", ["comment"]],
    ["install.sh", "if [ -f x ]; then echo \"$HOME\"; fi # c", "if", ["keyword"]],
    ["install.sh", "if [ -f x ]; then echo \"$HOME\"; fi # c", "# c", ["comment"]],
  ];

  it.each(SAMPLES)("%s: %j colours %s", async (file, code, piece, tokens) => {
    expect(await colours(file, code, piece)).toEqual(tokens);
  });
});

describe(".env files", () => {
  const env = (code: string, piece: string) => colours(".env", code, piece);

  it("colours keys, =, values and comments", async () => {
    const code = "# the app\nAPP_NAME=Gizai\nAPP_DEBUG=true # for now\nEMPTY=\n";
    expect(await env(code, "# the app")).toEqual(["comment"]);
    expect(await env(code, "APP_NAME")).toEqual(["property"]);
    expect(await env(code, "=")).toEqual(["operator"]);
    expect(await env(code, "Gizai")).toEqual(["string"]);
    expect(await env(code, "true")).toEqual(["string"]);
    expect(await env(code, "# for now")).toEqual(["comment"]);
    expect(await env(code, "EMPTY")).toEqual(["property"]);
  });

  it("colours export as a keyword", async () => {
    expect(await env("export DB_HOST=127.0.0.1", "export")).toEqual(["keyword"]);
    expect(await env("export DB_HOST=127.0.0.1", "DB_HOST")).toEqual(["property"]);
    expect(await env("exported=1", "exported")).toEqual(["property"]);
  });

  it("colours ${NAME} in a value, but not in single quotes", async () => {
    const code = "URL=\"https://${HOST}/api\"\nRAW='${HOST}'\nBARE=${HOST}/x";
    expect(await env(code, "\"https://")).toEqual(["string"]);
    expect(await env(code, "${HOST}")).toEqual(["variable"]);
    expect(await env(code, "/api\"")).toEqual(["string"]);
    expect(await env(code, "'${HOST}'")).toEqual(["string"]);
    expect(await env(code.slice(code.indexOf("BARE")), "${HOST}")).toEqual(["variable"]);
  });

  it("keeps a double-quoted value over lines, and # inside quotes", async () => {
    const code = "KEY=\"-----BEGIN KEY-----\nabc # not a comment\n-----END KEY-----\"\nNEXT=1";
    expect(await env(code, "abc # not a comment")).toEqual(["string"]);
    expect(await env(code, "-----END KEY-----\"")).toEqual(["string"]);
    expect(await env(code, "NEXT")).toEqual(["property"]);
  });
});
