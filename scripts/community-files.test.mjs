// Tests for the repository's community files and one name in the docs (GA-201): the issue forms in
// .github/ISSUE_TEMPLATE keep the rules of GitHub's issue forms check (docs.github.com, "Syntax for issue forms",
// "Syntax for GitHub's form schema" and "Common validation errors when creating issue forms"), read with the YAML parser
// the Editor already has (@codemirror/lang-yaml), as GA has no other; CONTRIBUTING.md, SECURITY.md, the pull request
// template and release.yml exist, and their links reach real files and headings; docs/RELEASING.md labels the bump and
// release pull requests and makes the release with the card's notes; and "Gizai" is left in the docs only in paths,
// file names and the app's own texts they quote, while the names installs and updates find GizAi by stay as they are.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { yamlLanguage } from "@codemirror/lang-yaml";
import { describe, expect, it } from "vitest";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const read = (path) => readFileSync(join(ROOT, path), "utf8");
const git = (...args) => spawnSync("git", args, { cwd: ROOT, encoding: "utf8", maxBuffer: 64 * 1000 * 1000 });
const REPO = "https://github.com/Yotech-AI/gizai";

// YAML to a plain value, for the YAML these files use: block and flow mappings and sequences, plain, quoted and block
// scalars. A plain yes, no, on, off, true or false is a boolean, as GitHub reads it.
function parseYaml(text) {
  const tree = yamlLanguage.parser.parse(text);
  const errors = [];
  tree.iterate({ enter: (n) => { if (n.type.isError) errors.push(text.slice(0, n.from).split("\n").length); } });
  if (errors.length) throw new Error(`YAML syntax error on line ${[...new Set(errors)].join(", ")}`);
  // a node's children without its comments and punctuation
  const parts = (node) => {
    const out = [];
    for (let c = node.firstChild; c; c = c.nextSibling) if (!["Comment", "Key", ":", "-", "[", "]", ",", "{", "}"].includes(c.name)) out.push(c);
    return out;
  };
  const value = (node) => {
    if (!node) return null;
    const src = text.slice(node.from, node.to);
    switch (node.name) {
      case "BlockMapping":
      case "FlowMapping": {
        const out = {};
        for (const pair of node.getChildren("Pair")) {
          const key = scalar(pair.getChild("Key").firstChild);
          if (Object.hasOwn(out, key)) throw new Error(`the key ${key} twice`);
          out[key] = value(parts(pair)[0]);
        }
        return out;
      }
      case "BlockSequence":
      case "FlowSequence":
        return node.getChildren("Item").map((item) => value(parts(item)[0]));
      case "Literal":
      case "QuotedLiteral":
        return scalar(node);
      case "BlockLiteral": {
        const header = text.slice(node.firstChild.from, node.firstChild.to);
        const lines = src.split("\n").slice(1);
        const indent = Math.min(...lines.filter((l) => l.trim()).map((l) => l.match(/^ */)[0].length));
        const body = lines.map((l) => l.slice(indent)).join("\n").replace(/\n+$/, "");
        return header.startsWith(">") ? body.replace(/([^\n])\n(?=[^\n])/g, "$1 ") : body;
      }
      default:
        throw new Error(`no YAML ${node.name} in these files: ${src}`);
    }
  };
  const scalar = (node) => {
    const src = text.slice(node.from, node.to);
    if (node.name === "QuotedLiteral") return src.startsWith("'") ? src.slice(1, -1).replace(/''/g, "'") : JSON.parse(src);
    if (/^(y|yes|n|no|true|false|on|off)$/i.test(src)) return /^(y|yes|true|on)$/i.test(src);
    if (/^(null|~)$/.test(src)) return null;
    if (/^-?\d+$/.test(src)) return Number(src);
    return src.replace(/\s*\n\s*/g, " ");
  };
  return value(parts(tree.topNode.getChild("Document"))[0]);
}

// What GitHub's issue forms check refuses, as a list of problems (empty when the form passes).
const TOP_KEYS = ["name", "description", "title", "labels", "assignees", "projects", "type", "body"];
const ELEMENT_KEYS = ["type", "id", "attributes", "validations"];
const ATTRIBUTES = {
  markdown: ["value"],
  textarea: ["label", "description", "placeholder", "value", "render"],
  input: ["label", "description", "placeholder", "value"],
  dropdown: ["label", "description", "multiple", "options", "default"],
  checkboxes: ["label", "description", "options"],
  upload: ["label", "description"],
};
const VALIDATIONS = { markdown: [], textarea: ["required", "min_length"], input: ["required", "min_length"], dropdown: ["required"], checkboxes: ["required"], upload: ["required", "accept"] };
const isText = (v) => typeof v === "string" && v.trim() !== "";
const isMap = (v) => v !== null && typeof v === "object" && !Array.isArray(v);
const parameterize = (s) => s.toLowerCase().replace(/[^a-z0-9_-]+/g, "-").replace(/^-|-$/g, "");

function formProblems(form) {
  const problems = [];
  if (!isMap(form)) return ["the form isn't a mapping"];
  for (const key of Object.keys(form)) if (!TOP_KEYS.includes(key)) problems.push(`${key} is not a permitted key`);
  for (const key of ["name", "description"]) if (!isText(form[key])) problems.push(`${key} must be a string`);
  if (form.labels !== undefined && !isText(form.labels) && !(Array.isArray(form.labels) && form.labels.every(isText))) problems.push("labels must be strings");
  if (!Array.isArray(form.body) || form.body.length === 0) return [...problems, "body cannot be empty"];
  const ids = [];
  const labels = [];
  form.body.forEach((el, i) => {
    const at = `body[${i}]`;
    if (!isMap(el)) return problems.push(`${at} isn't a mapping`);
    for (const key of Object.keys(el)) if (!ELEMENT_KEYS.includes(key)) problems.push(`${at}: ${key} is not a permitted key`);
    if (!(el.type in ATTRIBUTES)) return problems.push(`${at}: ${el.type} is not a valid input type`);
    const attrs = el.attributes;
    if (!isMap(attrs)) return problems.push(`${at}: attributes are missing`);
    for (const key of Object.keys(attrs)) if (!ATTRIBUTES[el.type].includes(key)) problems.push(`${at}: ${key} is not a permitted attribute`);
    for (const key of Object.keys(el.validations ?? {})) if (!VALIDATIONS[el.type].includes(key)) problems.push(`${at}: ${key} is not a permitted validation`);
    if (el.validations?.required !== undefined && typeof el.validations.required !== "boolean") problems.push(`${at}: required must be a boolean`);
    if (el.type === "markdown") {
      if (!isText(attrs.value)) problems.push(`${at}: required attribute key value is missing`);
      if (el.id !== undefined) problems.push(`${at}: markdown takes no id`);
      return;
    }
    if (el.id !== undefined) {
      if (!/^[A-Za-z0-9_-]+$/.test(String(el.id))) problems.push(`${at}: id can only contain numbers, letters, -, _`);
      ids.push(String(el.id));
    }
    if (!isText(attrs.label)) problems.push(`${at}: label must be a string`);
    else {
      labels.push(attrs.label);
      if (/password/i.test(attrs.label) && ["input", "textarea"].includes(el.type)) problems.push(`${at}: label contains forbidden word`);
    }
    if (el.type === "dropdown") {
      const options = attrs.options;
      if (!Array.isArray(options) || options.length === 0) return problems.push(`${at}: options cannot be empty`);
      if (options.some((o) => typeof o === "boolean")) problems.push(`${at}: options must not include booleans`);
      if (options.some((o) => !isText(o))) problems.push(`${at}: options must be strings`);
      if (new Set(options).size !== options.length) problems.push(`${at}: options must be unique`);
      if (options.some((o) => typeof o === "string" && o.trim().toLowerCase() === "none")) problems.push(`${at}: options must not include the reserved word, none`);
      if (attrs.multiple !== undefined && typeof attrs.multiple !== "boolean") problems.push(`${at}: multiple must be a boolean`);
      if (attrs.default !== undefined) {
        if (!Number.isInteger(attrs.default) || attrs.default < 0 || attrs.default >= options.length) problems.push(`${at}: default must be an option's index`);
        if (options.some((o) => typeof o === "string" && o.trim().toLowerCase() === "n/a")) problems.push(`${at}: with a default, options must not include n/a`);
      }
    }
    if (el.type === "checkboxes") {
      if (!Array.isArray(attrs.options) || attrs.options.length === 0) return problems.push(`${at}: options cannot be empty`);
      attrs.options.forEach((o, j) => {
        if (!isMap(o) || !isText(o.label)) return problems.push(`${at}: options[${j}] needs a label`);
        for (const key of Object.keys(o)) if (!["label", "required"].includes(key)) problems.push(`${at}: options[${j}]: ${key} is not a permitted key`);
        if (o.required !== undefined && typeof o.required !== "boolean") problems.push(`${at}: options[${j}]: required must be a boolean`);
        labels.push(o.label);
      });
    }
  });
  if (!form.body.some((el) => isMap(el) && el.type !== "markdown")) problems.push("body must contain at least one non-markdown field");
  if (new Set(ids).size !== ids.length) problems.push("body must have unique ids");
  if (new Set(labels).size !== labels.length) problems.push("body must have unique labels");
  else if (new Set(labels.map(parameterize)).size !== labels.length) problems.push("labels are too similar");
  return problems;
}

const TEMPLATES = ".github/ISSUE_TEMPLATE";
const FORMS = { bug: "bug.yml", feature: "feature.yml", install: "install.yml" };
const form = (name) => parseYaml(read(`${TEMPLATES}/${FORMS[name]}`));
const field = (f, id) => f.body.find((el) => el.id === id);
const required = (f) => f.body.filter((el) => el.validations?.required).map((el) => el.id);

describe("the issue forms check, as these tests read it", () => {
  const good = { name: "A", description: "B", body: [{ type: "textarea", id: "a", attributes: { label: "A" } }] };

  it("passes a small good form, and reads YAML's booleans, quotes and folded text as GitHub does", () => {
    expect(formProblems(good)).toEqual([]);
    expect(parseYaml("a: yes\nb: 'it''s'\nc: \"x: y\"\nd: >-\n  one\n  two\ne: [x, \"yes\", no]\n")).toEqual({ a: true, b: "it's", c: "x: y", d: "one two", e: ["x", "yes", false] });
    expect(() => parseYaml("a: [x\nb: y\n")).toThrow(/YAML syntax error/);
  });

  it("refuses what GitHub refuses", () => {
    const el = (type, attributes, more = {}) => ({ type, attributes, ...more });
    const with_ = (...body) => ({ ...good, body });
    expect(formProblems({ description: "B", body: good.body })).toContain("name must be a string");
    expect(formProblems({ ...good, input: 1 })).toContain("input is not a permitted key");
    expect(formProblems({ ...good, body: [] })).toContain("body cannot be empty");
    expect(formProblems(with_(el("markdown", { value: "x" })))).toContain("body must contain at least one non-markdown field");
    expect(formProblems(with_(el("input", { label: "A" }, { id: "x" }), el("input", { label: "B" }, { id: "x" })))).toContain("body must have unique ids");
    expect(formProblems(with_(el("input", { label: "A" }), el("textarea", { label: "A" })))).toContain("body must have unique labels");
    expect(formProblems(with_(el("input", { label: "A b" }), el("textarea", { label: "a-b" })))).toContain("labels are too similar");
    expect(formProblems(with_(el("input", { label: "A" }, { id: "a b" })))).toContain("body[0]: id can only contain numbers, letters, -, _");
    expect(formProblems(with_(el("input", { label: "A", options: ["x"] })))).toContain("body[0]: options is not a permitted attribute");
    expect(formProblems(with_(el("dropdown", { label: "A", options: ["x", "None"] })))).toContain("body[0]: options must not include the reserved word, none");
    expect(formProblems(with_(el("dropdown", { label: "A", options: ["x", true] })))).toContain("body[0]: options must not include booleans");
    expect(formProblems(with_(el("dropdown", { label: "A", options: ["x", "x"] })))).toContain("body[0]: options must be unique");
    expect(formProblems(with_(el("checkboxes", { label: "A", options: [{ required: true }] })))).toContain("body[0]: options[0] needs a label");
    expect(formProblems(with_(el("input", { label: "Your password" })))).toContain("body[0]: label contains forbidden word");
    expect(formProblems(with_(el("select", { label: "A" })))).toContain("body[0]: select is not a valid input type");
  });
});

describe("the issue forms in .github/ISSUE_TEMPLATE", () => {
  it("are bug.yml, feature.yml and install.yml, with config.yml", () => {
    expect(readdirSync(join(ROOT, TEMPLATES)).sort()).toEqual(["bug.yml", "config.yml", "feature.yml", "install.yml"]);
  });

  it.each(Object.keys(FORMS))("%s passes GitHub's issue forms check, with a unique name", (name) => {
    expect(formProblems(form(name))).toEqual([]);
    expect(new Set(Object.keys(FORMS).map((n) => form(n).name)).size).toBe(3);
  });

  it("the bug report: label bug; what happened, the steps, the version and the system required; the CLIs; logs as text; a required checkbox for private data", () => {
    const f = form("bug");
    expect([f.name, f.labels]).toEqual(["Bug report", ["bug"]]);
    expect(required(f)).toEqual(expect.arrayContaining(["what-happened", "steps", "version", "system"]));
    expect(field(f, "version").attributes.description).toMatch(/Settings → Updates.*`gizai --version`/);
    expect(field(f, "system").attributes.options).toEqual(["Linux", "macOS (Apple silicon)", "Windows 11"]);
    expect(field(f, "cli").attributes.options).toEqual(["Claude Code", "Codex CLI", "Gemini CLI", "Another CLI"]);
    expect(field(f, "logs").attributes.render).toBe("text");
    expect(field(f, "private").attributes.options).toEqual([{ label: "I removed tokens, keys, client names and private code from what I pasted", required: true }]);
  });

  it("the feature request: label enhancement; the problem required; who it helps; the eight features, install and other; the free app's line", () => {
    const f = form("feature");
    expect([f.name, f.labels]).toEqual(["Feature request", ["enhancement"]]);
    expect(required(f)).toEqual(["problem"]);
    expect(field(f, "who").attributes.options).toEqual(["A freelancer", "A team", "Both"]);
    const features = read("README.md").split("\n## Features\n")[1].split("\n## ")[0].match(/^- \*\*/gm);
    expect(field(f, "part").attributes.options).toHaveLength(features.length + 2);
    expect(field(f, "part").attributes.options.slice(-2)).toEqual(["Install and updates", "Other"]);
    expect(f.body[0].attributes.value).toMatch(/The app stays free; the paid plan is hosted services only/);
    expect(f.body[0].attributes.value).toMatch(/docs\/PAID-FEATURES\.md/);
  });

  it("the install or update problem: label bug; the system, how it was installed and the install check required; the update log, Rust, Node.js and WebKitGTK", () => {
    const f = form("install");
    expect([f.name, f.labels]).toEqual(["Install or update problem", ["bug"]]);
    expect(required(f)).toEqual(["system", "system-version", "how", "check"]);
    expect(field(f, "how").attributes.options).toHaveLength(4);
    expect(field(f, "how").attributes.options.join("\n")).toMatch(/one-line install[\s\S]*install\.sh[\s\S]*install\.ps1[\s\S]*update from inside GizAi/);
    expect(field(f, "check").attributes.description).toMatch(/`\.\/install\.sh --check`[\s\S]*`\.\\install\.ps1 -Check`/);
    expect(field(f, "output").attributes.description).toMatch(/update\/update\.log/);
    expect(field(f, "tools").attributes.description).toMatch(/`rustc --version` and `node --version`/);
    expect(field(f, "webkitgtk").attributes.label).toMatch(/WebKitGTK/);
  });

  it("the one-line install's check, as the install form gives it, is an option get.sh and get.ps1 take", () => {
    const check = field(form("install"), "check").attributes.description;
    expect(check).toContain("curl -fsSL https://gizai.app/install.sh | sh -s -- --check");
    expect(read("scripts/get.sh")).toMatch(/--check\) check=1/);
    expect(check).toContain("& ([scriptblock]::Create((irm https://gizai.app/install.ps1))) -Check");
    expect(read("scripts/get.ps1")).toMatch(/param\(\s*\[switch\]\$Check/);
  });

  it("config.yml turns blank issues off and sends questions to Q&A, security problems to the policy and readers to the docs", () => {
    const config = parseYaml(read(`${TEMPLATES}/config.yml`));
    expect(Object.keys(config).sort()).toEqual(["blank_issues_enabled", "contact_links"]);
    expect(config.blank_issues_enabled).toBe(false);
    for (const link of config.contact_links) {
      expect(Object.keys(link).sort()).toEqual(["about", "name", "url"]);
      expect(isText(link.name) && isText(link.about)).toBe(true);
      expect(link.url).toMatch(/^https:\/\//);
    }
    expect(config.contact_links.map((l) => l.url)).toEqual([`${REPO}/discussions/categories/q-a`, `${REPO}/security/policy`, "https://gizai.app/docs"]);
  });
});

describe("release.yml", () => {
  it("leaves the pull requests labelled release out of GitHub's list in a release's notes", () => {
    expect(parseYaml(read(".github/release.yml"))).toEqual({ changelog: { exclude: { labels: ["release"] } } });
  });
});

// GitHub's heading anchors: lower case, without punctuation, a hyphen for each space.
const slug = (heading) => heading.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").replace(/\s/g, "-");
// a Markdown file without its code blocks, which keeps its line numbers
const withoutCode = (md) => md.replace(/^ *```[\s\S]*?^ *```/gm, (block) => block.replace(/[^\n]/g, ""));
const anchors = (path) => withoutCode(read(path)).split("\n").filter((l) => /^#{1,6} /.test(l)).map((l) => slug(l.replace(/^#+ /, "")));

// Every link in a file that points into this repository: a relative one, or one to its files on GitHub's main.
function repoLinks(path) {
  const links = [];
  for (const [, target] of withoutCode(read(path)).matchAll(/\]\(([^)\s]+)\)/g)) {
    const blob = target.match(/^https:\/\/github\.com\/Yotech-AI\/gizai\/blob\/main\/(.+)$/);
    if (blob) links.push({ target, file: blob[1].split("#")[0], anchor: blob[1].split("#")[1] });
    else if (!/^[a-z]+:/.test(target)) {
      const [file, anchor] = target.split("#");
      links.push({ target, file: file ? normalize(join(dirname(path), file)) : path, anchor });
    }
  }
  return links;
}

describe("the community files", () => {
  const FILES = ["CONTRIBUTING.md", "SECURITY.md", ".github/pull_request_template.md", ".github/release.yml"];

  it("CONTRIBUTING.md, SECURITY.md, the pull request template and release.yml exist and are in git", () => {
    for (const file of FILES) expect(existsSync(join(ROOT, file))).toBe(true);
    expect(git("ls-files", "--error-unmatch", ...FILES).status).toBe(0);
  });

  it.each(["README.md", "CONTRIBUTING.md", "SECURITY.md", ".github/pull_request_template.md", "docs/RELEASING.md", ...Object.values(FORMS).map((f) => `${TEMPLATES}/${f}`)])(
    "every link in %s to this repository reaches a file and its heading",
    (path) => {
      const broken = repoLinks(path).filter(({ file, anchor }) => !existsSync(join(ROOT, file)) || (anchor && !anchors(file).includes(anchor)));
      expect(broken.map((l) => l.target)).toEqual([]);
    },
  );

  it("SECURITY.md: the latest release only, reports through GitHub's private vulnerability reporting only, a first reply within 7 days", () => {
    const security = read("SECURITY.md");
    expect(security).toMatch(/Only the latest release gets security fixes/);
    expect(security).toMatch(/private vulnerability reporting/);
    expect(security).toContain(`${REPO}/security/advisories/new`);
    expect(security).toMatch(/the only way to report a security problem\. Never report one in a public issue/);
    expect(security).toMatch(/a first reply within 7 days/);
    expect(security).not.toMatch(/@[a-z0-9-]+\.[a-z]{2,}/i);
  });

  it("CONTRIBUTING.md: set up and test, never against your own GizAi, the code's rules, pull requests into main, Apache-2.0 without a CLA", () => {
    const contributing = read("CONTRIBUTING.md");
    const headings = anchors("CONTRIBUTING.md");
    expect(headings).toEqual(expect.arrayContaining(["set-up", "never-test-against-your-own-gizai", "pull-requests", "licence"]));
    expect(contributing).toMatch(/source scripts\/env\.sh[\s\S]*cargo test --workspace && npm test/);
    expect(contributing).toMatch(/scripts\/ui-test\.sh/);
    expect(contributing).toMatch(/GIZAI_DATA_DIR/);
    expect(contributing).toMatch(/Open them into `main`/);
    expect(contributing).toMatch(/Apache License 2\.0[\s\S]*no CLA/);
    for (const script of contributing.match(/scripts\/[a-z-]+\.sh/g)) expect(existsSync(join(ROOT, script))).toBe(true);
  });

  it("the pull request template asks what changed, how it was tested and on which systems", () => {
    expect(anchors(".github/pull_request_template.md")).toEqual(["what-changed", "how-it-was-tested", "systems-tried"]);
    expect(read(".github/pull_request_template.md")).toMatch(/- \[ \] Linux\n- \[ \] macOS \(Apple silicon\)\n- \[ \] Windows 11/);
  });
});

describe("docs/RELEASING.md", () => {
  const releasing = read("docs/RELEASING.md");
  const commands = (cmd) => releasing.split("\n").map((l) => l.trim()).filter((l) => l.startsWith(cmd));

  it("steps 1 and 2 open their pull requests with the label release.yml leaves out", () => {
    const label = parseYaml(read(".github/release.yml")).changelog.exclude.labels[0];
    const prs = commands("gh pr create");
    expect(prs).toHaveLength(2);
    expect(prs[0]).toMatch(/--base main --title "Bump version to X\.Y\.Z" .* --label release$/);
    expect(prs[1]).toMatch(/--base production --head main --title "Release vX\.Y\.Z" .* --label release$/);
    expect(label).toBe("release");
  });

  it("step 3 makes the release with the notes file above GitHub's list, posted in Announcements, never a draft or a pre-release", () => {
    expect(commands("gh release create")).toEqual([
      'gh release create vX.Y.Z --target production --title "vX.Y.Z" --notes-file <notes.md> --generate-notes --discussion-category "Announcements"',
    ]);
    const step3 = releasing.split("\n3. **Create the release**")[1].split("\n4. **")[0];
    expect(step3).toMatch(/only works once Jeffrey has turned Discussions on/);
    expect(step3).toMatch(/Don't add `--draft` or `--prerelease`/);
  });

  it("Release notes: the format, a sample with at most six bullets of what's new, and who writes them", () => {
    const notes = withoutCode(releasing).split("\n## Release notes\n")[1].split("\n## ")[0];
    expect(notes).toMatch(/\*\*A share line:\*\*[\s\S]*\*\*What's new:\*\* at most six bullets[\s\S]*\*\*Update:\*\*[\s\S]*\*\*All changes:\*\*/);
    const sample = releasing.match(/```markdown\n([\s\S]*?)```/)[1];
    expect(sample).toMatch(/^GizAi \d+\.\d+\.\d+ .+\n\n## What's new\n/);
    const bullets = sample.split("## What's new")[1].split("\n**Update:**")[0].match(/^- \*\*/gm);
    expect(bullets.length).toBeGreaterThan(0);
    expect(bullets.length).toBeLessThanOrEqual(6);
    expect(sample).toMatch(/\*\*Update:\*\* .+\n\n## All changes\n$/);
    expect(notes).toMatch(/\*\*Who writes them:\*\*[\s\S]*The Team Lead drafts[\s\S]*starts the DevOps Agent[\s\S]*The DevOps Agent[\s\S]*needs_decision/);
  });
});

// The "Gizai"s the docs keep: file names, and the app's own texts they quote, each still in the app's code (when the
// app's rebrand changes one, the doc that quotes it shows up here).
const QUOTES = ['"Gizai"', "From Gizai's catalog", "a tool from outside Gizai", "Gizai keeps the call", "because Gizai quit", "Open Gizai", "Quit Gizai completely"];
const FILE_NAMES = /Gizai\.(app|lnk)/g;
// Records of their day, which keep the name they had.
const DATED = /^docs\/(research|superpowers)\/|^docs\/(HANDOFF-|MORNING-)|^docs\/(OPEN-ITEMS|v0\.1-notes|v0\.3-notes)\.md$/;

describe('one name: GizAi in the docs, and "Gizai" only where it is kept', () => {
  const docs = git("ls-files", "README.md", "CLAUDE.md", "CONTRIBUTING.md", "SECURITY.md", ".github/ISSUE_TEMPLATE", ".github/pull_request_template.md", "docs/*.md")
    .stdout.split("\n").filter((p) => p && !DATED.test(p));

  it("the docs it covers include the README, CLAUDE.md, the community files and the docs in the research's table", () => {
    for (const doc of ["README.md", "CLAUDE.md", "CONTRIBUTING.md", "SECURITY.md", "docs/PLATFORMS.md", "docs/agent-tools.md", "docs/RELEASING.md", "docs/memory.md", "docs/PAID-FEATURES.md", "docs/features/board.md"])
      expect(docs).toContain(doc);
    expect(docs.filter((d) => DATED.test(d))).toEqual([]);
  });

  it('"Gizai" is left only in code and paths (in backticks or a code block), file names and the app\'s own texts', () => {
    const left = [];
    for (const doc of docs) {
      const lines = withoutCode(read(doc)).split("\n");
      lines.forEach((line, i) => {
        let rest = line.replace(/`[^`]*`/g, "").replace(FILE_NAMES, "");
        for (const quote of QUOTES) rest = rest.split(quote).join("");
        if (rest.includes("Gizai")) left.push(`${doc}:${i + 1}: ${line.trim().slice(0, 120)}`);
      });
    }
    expect(left).toEqual([]);
  });

  it("each text the docs quote with Gizai is still the app's own", () => {
    for (const quote of QUOTES) {
      const r = git("grep", "-lF", quote, "--", "src", "src-tauri/src", "crates/*/src");
      expect(r.stdout, quote).not.toBe("");
    }
  });

  it("the names installs and updates find GizAi by stay: productName, the window title, the identifier, the data folders and the desktop entry", () => {
    const conf = JSON.parse(read("src-tauri/tauri.conf.json"));
    expect(conf.productName).toBe("Gizai");
    expect(conf.identifier).toBe("ai.gizai.app");
    expect(conf.app.windows[0].title).toBe("Gizai");
    expect(read("install.sh")).toContain(`grep -q '"productName": "Gizai"'`);
    expect(read("install.sh")).toMatch(/^Name=Gizai$/m);
    expect(read("src-tauri/src/lib.rs")).toContain('join("Library/Application Support/Gizai")');
    expect(read("src-tauri/src/lib.rs")).toMatch(/\.join\("Gizai"\)/);
  });
});
