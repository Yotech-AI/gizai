// GA-109: Blade's colours, on sample templates. Blade's own parts get theirs ({{-- --}} a comment, {{ }} and {!! !!}
// meta, directives keywords), PHP is coloured inside echoes, directives' brackets and @php … @endphp, and the rest is
// HTML (with CSS and JS in <style> and <script>). What Blade doesn't compile (@click, info@…, @{{, an echo without its
// end) stays HTML text.
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import type { SyntaxNode } from "@lezer/common";
import { describe, expect, it } from "vitest";
import { blade, bladeLanguage, DIRECTIVES } from "./blade";
import { highlightCode } from "./codeTheme";

/** The tokens over the `nth` `piece` in `code`, each once, in order; "" where nothing colours a character. */
function colours(code: string, piece: string, nth = 0): string[] {
  let at = -1;
  for (let i = 0; i <= nth; i++) at = code.indexOf(piece, at + 1);
  if (at < 0) throw new Error(`no ${piece} in ${code}`);
  const chars: string[] = Array.from({ length: piece.length }, () => "");
  for (const run of highlightCode(code, bladeLanguage)) {
    for (let p = Math.max(run.from, at); p < Math.min(run.to, at + piece.length); p++) chars[p - at] = run.token;
  }
  return [...new Set(chars)];
}

/** The language at the `nth` `piece` in `code`, by its grammar's top node: Program is PHP, Document HTML, Template
 *  Blade's own, StyleSheet CSS, Script JS. */
function languageAt(code: string, piece: string, nth = 0): string {
  let at = -1;
  for (let i = 0; i <= nth; i++) at = code.indexOf(piece, at + 1);
  if (at < 0) throw new Error(`no ${piece} in ${code}`);
  for (let node: SyntaxNode | null = bladeLanguage.parser.parse(code).resolveInner(at, 1); node; node = node.parent) {
    if (node.type.isTop) return node.name;
  }
  return "";
}

describe("Blade comments", () => {
  it("colours {{-- --}} as a comment, over lines and with echoes, directives and tags inside", () => {
    const code = "{{-- @if($x) {{ $y }}\n  <p>{!! $z !!}</p> --}}<p>{{ $a }}</p>";
    expect(colours(code, "{{-- @if($x) {{ $y }}\n  <p>{!! $z !!}</p> --}}")).toEqual(["comment"]);
    expect(colours(code, "p", 2)).toEqual(["tag"]);
    expect(colours(code, "$a")).toEqual(["variable"]);
  });

  it("ends a comment at its first --}}", () => {
    const code = "{{-- one --}}<b>{{-- two --}}</b>";
    expect(colours(code, "{{-- one --}}")).toEqual(["comment"]);
    expect(colours(code, "b")).toEqual(["tag"]);
    expect(colours(code, "{{-- two --}}")).toEqual(["comment"]);
  });
});

describe("Blade echoes", () => {
  it("colours {{ }} and {!! !!} as meta and what's inside as PHP", () => {
    const code = `<p>{{ $user->name }} {{ route('tasks.show', ['id' => $task->id]) }} {!! $html !!} {{ __("Save") }}</p>`;
    expect(colours(code, "{{")).toEqual(["meta"]);
    expect(colours(code, "}}")).toEqual(["meta"]);
    expect(colours(code, "{!!")).toEqual(["meta"]);
    expect(colours(code, "!!}")).toEqual(["meta"]);
    expect(colours(code, "route")).toEqual(["function"]);
    expect(colours(code, "'tasks.show'")).toEqual(["string"]);
    expect(colours(code, "'id'")).toEqual(["string"]);
    expect(colours(code, "=>")).toEqual(["punctuation"]);
    expect(colours(code, "__")).toEqual(["function"]);
    expect(colours(code, `"Save"`)).toEqual(["string"]);
    expect(colours(code, "$html")).toEqual(["variable"]);
    expect(colours(code, "name")).toEqual(["property"]);
    expect(languageAt(code, "$user")).toBe("Program");
    expect(languageAt(code, "route")).toBe("Program");
    expect(languageAt(code, "$html")).toBe("Program");
    expect(languageAt(code, "p")).toBe("Document");
  });

  it("colours PHP's numbers, keywords and operators in an echo", () => {
    const code = "{{ $count > 0 ? 'many' : null }} {{ new Carbon($at) }}";
    expect(colours(code, "0")).toEqual(["number"]);
    expect(colours(code, ">")).toEqual(["operator"]);
    expect(colours(code, "'many'")).toEqual(["string"]);
    expect(colours(code, "null")).toEqual(["number"]);
    expect(colours(code, "new")).toEqual(["keyword"]);
    expect(languageAt(code, "Carbon")).toBe("Program");
  });

  it("puts an echo inside an attribute's value", () => {
    const code = `<div class="card {{ $active ? 'on' : '' }}" id="t-{{ $id }}">x</div>`;
    expect(colours(code, "class")).toEqual(["attribute"]);
    expect(colours(code, `"card `)).toEqual(["string"]);
    expect(colours(code, "{{")).toEqual(["meta"]);
    expect(colours(code, "'on'")).toEqual(["string"]);
    expect(colours(code, "$id")).toEqual(["variable"]);
    expect(colours(code, "id", 0)).toEqual(["attribute"]);
    expect(colours(code, "div", 1)).toEqual(["tag"]);
  });
});

describe("Blade's @php blocks", () => {
  it("colours @php and @endphp as keywords and the code between as PHP, over lines", () => {
    const code = "@php\n    $total = $items->sum('price') + 2;\n    // the total\n@endphp\n<p>{{ $total }}</p>";
    expect(colours(code, "@php")).toEqual(["keyword"]);
    expect(colours(code, "@endphp")).toEqual(["keyword"]);
    expect(colours(code, "$total")).toEqual(["variable"]);
    expect(colours(code, "=")).toEqual(["operator"]);
    expect(colours(code, "sum")).toEqual(["function"]);
    expect(colours(code, "'price'")).toEqual(["string"]);
    expect(colours(code, "2")).toEqual(["number"]);
    expect(colours(code, "// the total")).toEqual(["comment"]);
    expect(languageAt(code, "$items")).toBe("Program");
    expect(colours(code, "<p>")).toEqual(["punctuation", "tag"]);
  });

  it("colours @php(…) on one line as a directive with PHP in its brackets", () => {
    const code = "@php($open = true)\n<p>x</p>";
    expect(colours(code, "@php")).toEqual(["keyword"]);
    expect(colours(code, "(")).toEqual(["punctuation"]);
    expect(colours(code, "$open")).toEqual(["variable"]);
    expect(colours(code, "true")).toEqual(["number"]);
    expect(colours(code, "<p>")).toEqual(["punctuation", "tag"]);
  });

  it("keeps HTML in an @php block as PHP, not as tags", () => {
    const code = "@php $a = '<b>'; @endphp<i>x</i>";
    expect(colours(code, "'<b>'")).toEqual(["string"]);
    expect(colours(code, "i")).toEqual(["tag"]);
  });
});

describe("Blade directives", () => {
  it("colours directives as keywords and their arguments as PHP", () => {
    const code = [
      "@extends('layouts.app')",
      "@section('title', __('Tasks'))",
      "@if($tasks->isEmpty())",
      "  <p>None</p>",
      "@elseif ($tasks->count() > 10)",
      "  @include('tasks.list', ['tasks' => $tasks])",
      "@else",
      "  @foreach ($tasks as $task)",
      "    <x-task :task=\"$task\" />",
      "  @endforeach",
      "@endif",
      "@endsection",
    ].join("\n");
    for (const d of ["@extends", "@section", "@if", "@elseif", "@include", "@else", "@foreach", "@endforeach", "@endif", "@endsection"]) {
      expect(colours(code, d), d).toEqual(["keyword"]);
    }
    expect(colours(code, "'layouts.app'")).toEqual(["string"]);
    expect(colours(code, "'title'")).toEqual(["string"]);
    expect(colours(code, "__")).toEqual(["function"]);
    expect(colours(code, "isEmpty")).toEqual(["function"]);
    expect(colours(code, "count")).toEqual(["function"]);
    expect(colours(code, "10")).toEqual(["number"]);
    expect(colours(code, "'tasks.list'")).toEqual(["string"]);
    expect(colours(code, "as $task")).toEqual(["keyword", "", "variable"]);
    expect(colours(code, "$task)")).toEqual(["variable", "punctuation"]);
    expect(languageAt(code, "$tasks")).toBe("Program");
    expect(languageAt(code, "$tasks->count")).toBe("Program");
    // the brackets around the arguments are Blade's
    expect(colours(code, "(", 0)).toEqual(["punctuation"]);
    // the HTML between directives
    expect(colours(code, "<p>")).toEqual(["punctuation", "tag"]);
    expect(colours(code, "x-task")).toEqual(["tag"]);
    expect(colours(code, ":task")).toEqual(["attribute"]);
    expect(languageAt(code, "None")).toBe("Document");
  });

  it("ends the arguments at their own closing bracket, past nested brackets and brackets in strings", () => {
    const code = "@if(in_array($role, [')', 'admin(1)']) && fn() => (1))<b>yes</b>@endif";
    expect(colours(code, "in_array")).toEqual(["function"]);
    expect(colours(code, "')'")).toEqual(["string"]);
    expect(colours(code, "'admin(1)'")).toEqual(["string"]);
    expect(languageAt(code, "1))")).toBe("Program");
    expect(colours(code, "b")).toEqual(["tag"]);
    expect(languageAt(code, "yes")).toBe("Document");
    expect(colours(code, "@endif")).toEqual(["keyword"]);
  });

  it("colours a directive without arguments, and one inside a tag", () => {
    const code = `<form method="POST">@csrf @method('PUT')<input @disabled($locked) @checked(old('done', $task->done))></form>`;
    expect(colours(code, "@csrf")).toEqual(["keyword"]);
    expect(colours(code, "@method")).toEqual(["keyword"]);
    expect(colours(code, "'PUT'")).toEqual(["string"]);
    expect(colours(code, "input")).toEqual(["tag"]);
    expect(colours(code, "@disabled")).toEqual(["keyword"]);
    expect(colours(code, "$locked")).toEqual(["variable"]);
    expect(colours(code, "@checked")).toEqual(["keyword"]);
    expect(colours(code, "old")).toEqual(["function"]);
    expect(colours(code, "form", 1)).toEqual(["tag"]);
  });

  it("knows each of its directives", () => {
    for (const d of DIRECTIVES) expect(highlightCode(`@${d}`, bladeLanguage), d).toEqual([{ from: 0, to: d.length + 1, token: "keyword" }]);
    for (const d of ["if", "endif", "foreach", "forelse", "include", "csrf", "auth", "can", "error", "php", "endphp", "livewire", "vite"]) {
      expect(DIRECTIVES.has(d), d).toBe(true);
    }
  });
});

describe("Blade's HTML", () => {
  it("colours the rest as HTML, with CSS in <style> and JavaScript in <script>", () => {
    const code = [
      "<!DOCTYPE html>",
      "<!-- a note -->",
      `<html lang="{{ app()->getLocale() }}">`,
      "<style>.card { color: red; } @media (min-width: 600px) { .card { margin: 0; } }</style>",
      "<script>const open = true;</script>",
      `<a href="/tasks" class="link">Tasks</a>`,
      "</html>",
    ].join("\n");
    expect(colours(code, "<!-- a note -->")).toEqual(["comment"]);
    expect(colours(code, "html", 1)).toEqual(["tag"]);
    expect(colours(code, "lang")).toEqual(["attribute"]);
    expect(colours(code, "getLocale")).toEqual(["function"]);
    expect(colours(code, "color")).toEqual(["property"]);
    expect(colours(code, "@media")).toEqual(["keyword"]);
    expect(languageAt(code, "@media")).toBe("StyleSheet");
    expect(colours(code, "const")).toEqual(["keyword"]);
    expect(languageAt(code, "const")).toBe("Script");
    expect(colours(code, "href")).toEqual(["attribute"]);
    expect(colours(code, `"/tasks"`)).toEqual(["string"]);
    expect(colours(code, "a href")).toEqual(["tag", "", "attribute"]);
    expect(languageAt(code, "Tasks")).toBe("Document");
  });

  it("keeps what Blade doesn't compile as HTML text: e-mail addresses, Alpine's @, @{{, @@ and unknown names", () => {
    const code = `<p>Mail info@example.com</p><button @click="open = !open" x-on:keyup="go">@{{ name }} @{!! raw !!} @@if @unknown(1)</button>`;
    const tokens = new Set(highlightCode(code, bladeLanguage).map((r) => r.token));
    expect(tokens.has("meta")).toBe(false);
    expect(tokens.has("keyword")).toBe(false);
    expect(colours(code, "@click")).toEqual(["attribute"]);
    expect(colours(code, `"open = !open"`)).toEqual(["string"]);
    expect(languageAt(code, "@example")).toBe("Document");
    expect(languageAt(code, "name")).toBe("Document");
    expect(languageAt(code, "@if")).toBe("Document");
    expect(languageAt(code, "@unknown")).toBe("Document");
  });

  it("leaves an echo, raw echo or comment without its end as text, and goes on with the HTML", () => {
    for (const open of ["{{ $x", "{!! $x", "{{-- note"]) {
      const code = `<p>${open}</p><b>y</b>`;
      expect(colours(code, open), open).toEqual(["variable"]);
      expect(colours(code, "b"), open).toEqual(["tag"]);
    }
  });

  it("leaves @verbatim … @endverbatim as text", () => {
    const code = "@verbatim\n<div>{{ name }}</div>\n@endverbatim\n{{ $x }}";
    expect(colours(code, "@verbatim")).toEqual(["keyword"]);
    expect(colours(code, "@endverbatim")).toEqual(["keyword"]);
    expect(colours(code, "{{")).toEqual(["variable"]);
    expect(colours(code, "div")).toEqual(["tag"]);
    expect(colours(code, "{{", 1)).toEqual(["meta"]);
  });
});

describe("Blade in an editor", () => {
  /** The editor's syntax tree, parsed to the end. */
  const parsed = (state: EditorState) => {
    const tree = ensureSyntaxTree(state, state.doc.length, 5000);
    if (!tree) throw new Error("not parsed");
    return tree;
  };

  it("is the Blade language with HTML's support", () => {
    const support = blade();
    expect(support.language).toBe(bladeLanguage);
    expect(bladeLanguage.name).toBe("blade");
  });

  it("parses again as the template changes", () => {
    let state = EditorState.create({ doc: "<p>{{ $a }}</p>\n@if($b)\n<i>x</i>\n@endif", extensions: blade() });
    expect(parsed(state).resolveInner(state.doc.toString().indexOf("$b"), 1).name).toBe("VariableName");
    // turn the paragraph's text into a raw echo and the @if's argument into a call
    const at = state.doc.toString().indexOf("$b");
    state = state.update({ changes: [{ from: 3, to: 11, insert: "{!! e($a) !!}" }, { from: at, to: at + 2, insert: "can('x')" }] }).state;
    const code = state.doc.toString();
    expect(code).toBe("<p>{!! e($a) !!}</p>\n@if(can('x'))\n<i>x</i>\n@endif");
    const tree = parsed(state);
    expect(tree.resolveInner(code.indexOf("e("), 1).name).toBe("Name");
    expect(tree.resolveInner(code.indexOf("'x'") + 1, 1).name).toBe("String");
    expect(tree.resolveInner(code.indexOf("{!!"), 1).name).toBe("{!!");
    expect(tree.resolveInner(code.indexOf("i>"), 1).name).toBe("TagName");
  });
});
