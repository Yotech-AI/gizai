// Laravel's Blade templates for CodeMirror (Code Editor F0, GA-109), on the model of @codemirror/lang-liquid: a small
// parser finds Blade's own parts, HTML is parsed over the text between them (an overlay, so a tag can hold an echo or a
// directive), and PHP (php({ plain: true })) inside {{ }}, {!! !!}, a directive's brackets and @php … @endphp. The
// parts follow Blade's compiler: {{-- --}} is a comment; @{{ and @@if stay text; a directive needs a known name after
// something that isn't a letter or digit (so `info@example.com` and Alpine's `@click` stay HTML); an echo, comment or
// block without its end is text, as Blade leaves it.
import { html } from "@codemirror/lang-html";
import { php } from "@codemirror/lang-php";
import { defineLanguageFacet, foldNodeProp, Language, languageDataProp, LanguageSupport } from "@codemirror/language";
import { NodeProp, NodeSet, NodeType, parseMixed, Parser, Tree, type Input, type PartialParse, type TreeFragment } from "@lezer/common";
import { styleTags, tags as t } from "@lezer/highlight";

/** Blade's directives (Laravel 12), and Livewire's, Vite's and Inertia's. Others stay text, as Blade leaves a directive
 *  it doesn't know: CSS's `@media` in a <style>, or Alpine's `@click` on a tag. */
export const DIRECTIVES = new Set(`
  if elseif else endif unless endunless isset endisset empty endempty auth elseauth endauth guest elseguest endguest
  production endproduction env endenv hasSection sectionMissing switch case break default endswitch
  for endfor foreach endforeach forelse endforelse while endwhile continue
  class style checked selected disabled readonly required
  include includeIf includeWhen includeUnless includeFirst each once endonce
  push endpush pushOnce endPushOnce pushIf endPushIf prepend endprepend prependOnce endPrependOnce stack
  extends section endsection show stop overwrite append yield parent
  component endcomponent componentFirst endComponentFirst slot endslot props aware fragment endfragment
  csrf method error enderror inject json js lang endlang choice dd dump use
  can elsecan endcan cannot elsecannot endcannot canany elsecanany endcanany session endsession context endcontext
  php endphp verbatim endverbatim
  vite viteReactRefresh livewire livewireStyles livewireScripts livewireScriptConfig persist endpersist script endscript
  assets endassets teleport endteleport entangle this inertia inertiaHead
`.trim().split(/\s+/));

const NAMES = ["Template", "Text", "Comment", "Echo", "RawEcho", "{{", "}}", "{!!", "!!}", "PhpCode", "Directive", "DirectiveName",
  "(", ")", "PhpBlock", "Verbatim"] as const;
const ID = Object.fromEntries(NAMES.map((n, i) => [n, i])) as Record<(typeof NAMES)[number], number>;

const data = defineLanguageFacet({ commentTokens: { block: { open: "{{--", close: "--}}" } } });

const nodeSet = new NodeSet(NAMES.map((name, id) => NodeType.define({
  id, name, top: id === 0, props: id === 0 ? [[languageDataProp, data]] : [],
}))).extend(
  styleTags({
    DirectiveName: t.keyword,
    '"{{" "}}" "{!!" "!!}"': t.special(t.brace), // quoted: ! means something in styleTags
    Comment: t.blockComment,
    "( )": t.paren,
  }),
  NodeProp.closedBy.add({ "{{": ["}}"], "{!!": ["!!}"], "(": [")"] }),
  NodeProp.openedBy.add({ "}}": ["{{"], "!!}": ["{!!"], ")": ["("] }),
  foldNodeProp.add({
    "PhpBlock Verbatim": (node) => node.firstChild && node.lastChild ? { from: node.firstChild.to, to: node.lastChild.from } : null,
    Comment: (node) => ({ from: node.from + 4, to: node.to - 4 }),
  }),
);

const word = (c: string | undefined) => c !== undefined && /\w/.test(c);

/** Where the bracket that closes the one at `open` is, skipping PHP's strings; -1 when it isn't closed. */
function closingParen(s: string, open: number): number {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    const c = s[i];
    if (c === "'" || c === '"') {
      for (i++; i < s.length && s[i] !== c; i++) if (s[i] === "\\") i++;
    } else if (c === "(") depth++;
    else if (c === ")" && --depth === 0) return i;
  }
  return -1;
}

/** The syntax tree of Blade's parts in `s` (which starts at `from` in the document), in Lezer's buffer form: each node
 *  is its type, start, end and size, after its children. */
function bladeTree(s: string, from: number): Tree {
  const buf: number[] = [];
  const leaf = (type: number, a: number, b: number) => { if (b > a) buf.push(type, from + a, from + b, 4); };
  const node = (type: number, a: number, b: number, mark: number) => buf.push(type, from + a, from + b, buf.length - mark + 4);
  let text = 0;
  const part = (a: number) => { leaf(ID.Text, text, a); return buf.length; };

  let i = 0;
  while (i < s.length) {
    const c = s[i];
    if (c === "{" && s[i + 1] === "{" && s.startsWith("--", i + 2)) {
      const end = s.indexOf("--}}", i + 4);
      if (end < 0) { i += 4; continue; }
      part(i);
      leaf(ID.Comment, i, end + 4);
      i = text = end + 4;
    } else if (c === "{" && (s.startsWith("{{", i) || s.startsWith("{!!", i))) {
      const raw = s[i + 1] === "!", size = raw ? 3 : 2;
      const end = s.indexOf(raw ? "!!}" : "}}", i + size);
      if (end < 0) { i += size; continue; }
      const mark = part(i);
      leaf(raw ? ID["{!!"] : ID["{{"], i, i + size);
      leaf(ID.PhpCode, i + size, end);
      leaf(raw ? ID["!!}"] : ID["}}"], end, end + size);
      node(raw ? ID.RawEcho : ID.Echo, i, end + size, mark);
      i = text = end + size;
    } else if (c === "@") {
      // @{{ and @{!! are an echo written out as it is; @@if a directive's name
      if (s[i + 1] === "{") { i += s.startsWith("{!!", i + 1) ? 4 : 3; continue; }
      const name = /^@?\w+/.exec(s.slice(i + 1, i + 64))?.[0] ?? "";
      if (word(s[i - 1]) || name.startsWith("@") || !DIRECTIVES.has(name)) { i += 1 + name.length; continue; }
      const nameEnd = i + 1 + name.length;
      const block = name === "php" || name === "verbatim" ? `@end${name}` : null;
      const open = /^[ \t]*\(/.exec(s.slice(nameEnd, nameEnd + 64)) ? s.indexOf("(", nameEnd) : -1;
      const close = open >= 0 ? closingParen(s, open) : -1;
      const end = block && open < 0 ? s.indexOf(block, nameEnd) : -1;
      const mark = part(i);
      leaf(ID.DirectiveName, i, nameEnd);
      if (end >= 0) {
        leaf(name === "php" ? ID.PhpCode : ID.Text, nameEnd, end);
        leaf(ID.DirectiveName, end, end + block!.length);
        node(name === "php" ? ID.PhpBlock : ID.Verbatim, i, end + block!.length, mark);
        i = text = end + block!.length;
      } else if (close >= 0) {
        leaf(ID["("], open, open + 1);
        leaf(ID.PhpCode, open + 1, close);
        leaf(ID[")"], close, close + 1);
        node(ID.Directive, i, close + 1, mark);
        i = text = close + 1;
      } else {
        node(ID.Directive, i, nameEnd, mark);
        i = text = nameEnd;
      }
    } else i++;
  }
  part(s.length);
  return Tree.build({ buffer: buf, nodeSet, topID: ID.Template, start: from, length: s.length });
}

/** Parses the whole template at once: Blade's parts are quick to find, and the HTML and PHP parsers inside reuse their
 *  earlier work as the text changes. */
class BladeParse implements PartialParse {
  parsedPos: number;
  stoppedAt: number | null = null;
  constructor(readonly input: Input, readonly from: number, readonly to: number) { this.parsedPos = from; }
  advance(): Tree {
    const tree = bladeTree(this.input.read(this.from, this.to), this.from);
    this.parsedPos = this.to;
    return tree;
  }
  stopAt(pos: number) { this.stoppedAt = pos; }
}

const htmlSupport = html({ matchClosingTags: false, selfClosingTags: true });
const phpParser = php({ plain: true, baseLanguage: null }).language.parser;
const mixed = parseMixed((node) => {
  if (node.type.isTop) return { parser: htmlSupport.language.parser, overlay: (n) => n.name === "Text" };
  return node.name === "PhpCode" ? { parser: phpParser } : null;
});

class BladeParser extends Parser {
  createParse(input: Input, fragments: readonly TreeFragment[], ranges: readonly { from: number; to: number }[]): PartialParse {
    const parse = new BladeParse(input, ranges[0].from, ranges[ranges.length - 1].to);
    return mixed(parse, input, fragments, ranges);
  }
}

/** Blade as a language: the Blade parser, with HTML and PHP inside. */
export const bladeLanguage = new Language(data, new BladeParser(), [], "blade");

/** Blade for an editor, with HTML's support (closing tags, completion) for the HTML in it. */
export function blade(): LanguageSupport {
  return new LanguageSupport(bladeLanguage, [htmlSupport.support]);
}
