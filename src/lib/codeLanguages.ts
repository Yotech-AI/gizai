// Which language a file is in, by its name (Code Editor F0, GA-109), for a card's changes and the Editor. Each language
// loads the first time a file needs it, in its own chunk, so the app doesn't load them all at the start. A file with
// another name shows as plain text. The names are what the Editor's status bar shows (D2).
import { LanguageDescription, LanguageSupport, StreamLanguage, type StreamParser } from "@codemirror/language";

/** What the status bar says for a file without a language. */
export const PLAIN_TEXT = "Plain text";

/** .env files: KEY=value lines, `export` before a key, # comments, values with or without quotes (double quotes can
 *  span lines), and ${NAME} in a value that isn't in single quotes. */
type EnvState = { inValue: boolean; quote: string | null };
type Stream = Parameters<StreamParser<EnvState>["token"]>[0];
/** The rest of a quoted value on this line, up to its closing quote or the next ${NAME}. */
function envQuoted(stream: Stream, state: EnvState): string {
  if (state.quote === '"' && !stream.current() && stream.match(/^\$\{[^}\n]*\}/)) return "variableName";
  while (!stream.eol()) {
    const c = stream.next();
    if (c === "\\") stream.next();
    else if (c === state.quote) { state.quote = null; break; }
    else if (state.quote === '"' && c === "$" && stream.peek() === "{" && stream.current().length > 1) { stream.backUp(1); break; }
  }
  return "string";
}
const dotenv: StreamParser<EnvState> = {
  name: "dotenv",
  startState: () => ({ inValue: false, quote: null }),
  token(stream, state) {
    if (state.quote) return envQuoted(stream, state);
    if (stream.sol()) state.inValue = false;
    if (stream.eatSpace()) return null;
    const c = stream.peek();
    if (c === "#") { stream.skipToEnd(); return "comment"; }
    if (!state.inValue) {
      if (stream.match(/^export(?=\s)/)) return "keyword";
      if (stream.match(/^[^=\s#]+/)) return "propertyName";
      stream.next();
      if (c === "=") { state.inValue = true; return "operator"; }
      return null;
    }
    if (c === '"' || c === "'") { stream.next(); state.quote = c; return envQuoted(stream, state); }
    if (stream.match(/^\$\{[^}\n]*\}/)) return "variableName";
    if (!stream.match(/^[^\s$]+/)) stream.next();
    return "string";
  },
  languageData: { commentTokens: { line: "#" } },
};

const js = (o: { typescript?: boolean; jsx?: boolean }) => async () => (await import("@codemirror/lang-javascript")).javascript(o);
// legacy-modes types its modes with @codemirror/streamparser, a copy of the same types that TypeScript tells apart.
const legacy = (parser: object) => new LanguageSupport(StreamLanguage.define(parser as StreamParser<unknown>));

/** The languages, in the order a name is matched: first by whole names (Blade before PHP), then by the extension. */
export const CODE_LANGUAGES: LanguageDescription[] = [
  LanguageDescription.of({
    name: "Blade", filename: /\.blade\.php$/,
    load: async () => (await import("./blade")).blade(),
  }),
  LanguageDescription.of({
    name: "PHP", extensions: ["php", "phtml"], filename: /^artisan$/,
    load: async () => (await import("@codemirror/lang-php")).php(),
  }),
  LanguageDescription.of({ name: "TypeScript", alias: ["ts"], extensions: ["ts", "mts", "cts"], load: js({ typescript: true }) }),
  LanguageDescription.of({ name: "TypeScript JSX", alias: ["tsx"], extensions: ["tsx"], load: js({ typescript: true, jsx: true }) }),
  LanguageDescription.of({ name: "JavaScript", alias: ["js", "node"], extensions: ["js", "mjs", "cjs"], load: js({}) }),
  LanguageDescription.of({ name: "JavaScript JSX", alias: ["jsx"], extensions: ["jsx"], load: js({ jsx: true }) }),
  LanguageDescription.of({
    name: "Rust", alias: ["rs"], extensions: ["rs"],
    load: async () => (await import("@codemirror/lang-rust")).rust(),
  }),
  LanguageDescription.of({
    name: "CSS", extensions: ["css"],
    load: async () => (await import("@codemirror/lang-css")).css(),
  }),
  LanguageDescription.of({
    name: "SCSS", extensions: ["scss"],
    load: async () => (await import("@codemirror/lang-sass")).sass({ indented: false }),
  }),
  LanguageDescription.of({
    name: "JSON", alias: ["jsonc"], extensions: ["json", "jsonc"], filename: /^composer\.lock$/,
    load: async () => (await import("@codemirror/lang-json")).json(),
  }),
  LanguageDescription.of({
    name: "YAML", alias: ["yml"], extensions: ["yaml", "yml"],
    load: async () => (await import("@codemirror/lang-yaml")).yaml(),
  }),
  LanguageDescription.of({
    name: "Markdown", alias: ["md"], extensions: ["md", "markdown"],
    load: async () => {
      const { markdown, markdownLanguage } = await import("@codemirror/lang-markdown");
      return markdown({ base: markdownLanguage, codeLanguages: CODE_LANGUAGES });
    },
  }),
  LanguageDescription.of({
    name: "SQL", extensions: ["sql"],
    load: async () => (await import("@codemirror/lang-sql")).sql(),
  }),
  LanguageDescription.of({
    name: "HTML", alias: ["htm"], extensions: ["html", "htm"],
    load: async () => (await import("@codemirror/lang-html")).html(),
  }),
  LanguageDescription.of({
    name: ".env", alias: ["env", "dotenv"], filename: /^\.env(\.[\w.-]+)?$/, extensions: ["env"],
    load: async () => legacy(dotenv),
  }),
  LanguageDescription.of({
    name: "TOML", extensions: ["toml"], filename: /^cargo\.lock$/,
    load: async () => legacy((await import("@codemirror/legacy-modes/mode/toml")).toml),
  }),
  LanguageDescription.of({
    name: "Shell", alias: ["sh", "bash", "zsh"], extensions: ["sh", "bash", "zsh"],
    filename: /^\.(bashrc|bash_profile|bash_aliases|zshrc|zshenv|zprofile|profile)$/,
    load: async () => legacy((await import("@codemirror/legacy-modes/mode/shell")).shell),
  }),
];

/** The language of the file at `path` (a name, or a path with / or \), or null for plain text. Case doesn't matter. */
export function languageFor(path: string): LanguageDescription | null {
  const name = path.split(/[\\/]/).pop()!.toLowerCase();
  return LanguageDescription.matchFilename(CODE_LANGUAGES, name);
}

/** The language's name for the file at `path`, as the status bar shows it: "Blade", "TypeScript JSX", "Plain text". */
export function languageName(path: string): string {
  return languageFor(path)?.name ?? PLAIN_TEXT;
}

/** Loads the file's language the first time one is needed (later files get the same one), or null for plain text. */
export function loadLanguage(path: string): Promise<LanguageSupport | null> {
  return languageFor(path)?.load() ?? Promise.resolve(null);
}
