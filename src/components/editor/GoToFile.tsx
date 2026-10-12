// Go to file (Ctrl+P; GA-113, design §3): the command palette's look over the Editor, over the files git tracks and new
// files it doesn't ignore. The typed letters match in order, in the name first; the match is bold and each file's folder
// follows its name. ↑ and ↓ choose, Enter opens, Escape closes.
import { useEffect, useMemo, useRef, useState } from "react";
import { FileCode, Search } from "lucide-react";
import { editorFiles } from "../../api";
import { matchFiles } from "../../lib/editor";

export function GoToFile({ folder, name, onOpen, onClose }: { folder: string; name: string; onOpen: (path: string) => void; onClose: () => void }) {
  const [files, setFiles] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => { input.current?.focus(); editorFiles(folder).then(setFiles).catch((e) => setError(String(e))); }, [folder]);
  const found = useMemo(() => matchFiles(files ?? [], query), [files, query]);
  useEffect(() => { (list.current?.children[sel] as HTMLElement | undefined)?.scrollIntoView({ block: "nearest" }); }, [sel]);
  const choose = (path: string | undefined) => { if (!path) return; onClose(); onOpen(path); };
  return (
    <>
      <div className="pal-backdrop" onClick={onClose} />
      <div className="pal ed-goto-pal" role="dialog" aria-label="Go to file">
        <div className="pal-in">
          <Search className="icon" />
          <input ref={input} value={query} placeholder={`Go to file in ${name}`} aria-label={`Go to file in ${name}`}
            onChange={(e) => { setQuery(e.target.value); setSel(0); }}
            onKeyDown={(e) => {
              if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); }
              else if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, found.length - 1)); }
              else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
              else if (e.key === "Enter") { e.preventDefault(); choose(found[sel]?.path); }
            }} />
        </div>
        <div className="pal-list" ref={list}>
          {error ? <div className="pal-group">{error}</div>
            : !files ? <div className="pal-group">Loading…</div>
            : found.length === 0 ? <div className="pal-group">No file found</div>
            : found.map((f, i) => {
              const at = f.path.lastIndexOf("/") + 1;
              return (
                <button key={f.path} className={`pal-item${i === sel ? " active" : ""}`} onMouseEnter={() => setSel(i)} onClick={() => choose(f.path)}>
                  <FileCode className="icon" />
                  <span>{bold(f.path, f.marks, at, f.path.length)}</span>
                  {at > 0 && <span className="where">{bold(f.path, f.marks, 0, at - 1)}</span>}
                </button>
              );
            })}
        </div>
      </div>
    </>
  );
}

/** `path` from `from` to `to`, with the matched letters in bold. */
function bold(path: string, marks: number[], from: number, to: number) {
  const out: React.ReactNode[] = [];
  let run = "";
  let strong = false;
  for (let i = from; i < to; i++) {
    const m = marks.includes(i);
    if (m !== strong && run) { out.push(strong ? <b key={i}>{run}</b> : run); run = ""; }
    strong = m;
    run += path[i];
  }
  if (run) out.push(strong ? <b key="end">{run}</b> : run);
  return out;
}
