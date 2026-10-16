// The larger view of a screenshot (GA-185, GA-181's frames 170:2304, 172:2896 and 172:3640): over the page on the scrim,
// 24 px from the window's edges. Fit | 100% (key 1), Previous and Next (← →), Compare with the run before (key C: side by
// side or a slider, paired by file name), Open in the system's image viewer, Esc closes. The focus stays inside while it
// is open.
import {
  useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent, type RefObject,
} from "react";
import { ChevronLeft, ChevronRight, Columns2, ExternalLink, X } from "lucide-react";
import { openFile } from "../../api";
import { answerOf, compare, formatOf, PAIRING_TEXT, shortCommit, sizeOf, themeOf, when } from "../../lib/screenshots";
import { useShotUrl } from "../../lib/shotUrls";
import type { Screenshot, ScreenshotSet } from "../../types";
import { Avatar } from "../Avatar";

type Size = { w: number; h: number };

/** The stage's size, kept up to date. */
function useSize<E extends HTMLElement>(): [RefObject<E | null>, Size] {
  const ref = useRef<E | null>(null);
  const [size, setSize] = useState<Size>({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const set = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    set();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(set);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, size];
}

/** A picture fitted in a box at its own proportions, never larger than 100%. */
function fit(s: Pick<Screenshot, "width" | "height">, box: Size): Size {
  const k = Math.min(box.w / s.width, box.h / s.height, 1);
  return { w: Math.max(0, Math.floor(s.width * k)), h: Math.max(0, Math.floor(s.height * k)) };
}

function Picture({ shot, size, label }: { shot: Screenshot; size?: Size; label?: string }) {
  const { url, error } = useShotUrl(shot);
  const style = size ? { width: size.w, height: size.h } : { width: shot.width, height: shot.height };
  return url ? <img className="sv-pic" src={url} alt={label ?? shot.caption} style={style} draggable={false} />
    : <span className="sv-pic sv-ph" style={style} role="img" aria-label={label ?? shot.caption}>{error ? "Couldn’t load this picture" : sizeOf(shot)}</span>;
}

/** Slider: the run before left of the line (under a tint), this run right of it; drag the line, or move it with the keys
 *  while it has the focus. */
function Slider({ before, shot, box, setWhen, beforeWhen }: { before: Screenshot; shot: Screenshot; box: Size; setWhen: string; beforeWhen: string }) {
  const [pos, setPos] = useState(50);
  const frame = useRef<HTMLDivElement>(null);
  const size = fit(shot, box);
  const moveTo = (x: number) => {
    const r = frame.current?.getBoundingClientRect();
    if (r && r.width > 0) setPos(Math.min(100, Math.max(0, ((x - r.left) / r.width) * 100)));
  };
  const down = (e: ReactPointerEvent) => {
    e.preventDefault();
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    moveTo(e.clientX);
  };
  const key = (e: ReactKeyboardEvent) => {
    const step = e.shiftKey || e.key === "PageUp" || e.key === "PageDown" ? 10 : 2;
    const next = e.key === "ArrowLeft" || e.key === "PageDown" ? pos - step : e.key === "ArrowRight" || e.key === "PageUp" ? pos + step
      : e.key === "Home" ? 0 : e.key === "End" ? 100 : null;
    if (next === null) return;
    e.preventDefault();
    e.stopPropagation();
    setPos(Math.min(100, Math.max(0, next)));
  };
  return (
    <div className="sv-slider" ref={frame} style={{ width: size.w, height: size.h }} onPointerDown={down}
      onPointerMove={(e) => { if (e.buttons & 1) moveTo(e.clientX); }}>
      <Picture shot={shot} size={size} label={`This run: ${shot.caption}`} />
      <div className="sv-before" style={{ clipPath: `inset(0 ${100 - pos}% 0 0)` }}>
        <Picture shot={before} size={size} label={`The run before: ${before.caption}`} />
        <span className="sv-tint" aria-hidden />
      </div>
      <div className="sv-line" style={{ left: `${pos}%` }}>
        <span className="sv-grip" role="slider" tabIndex={0} aria-label="The line between the run before and this run" aria-valuemin={0} aria-valuemax={100}
          aria-valuenow={Math.round(pos)} aria-valuetext={`${Math.round(pos)} % the run before`} onKeyDown={key}>
          <ChevronLeft className="icon sm" aria-hidden /><ChevronRight className="icon sm" aria-hidden />
        </span>
      </div>
      <span className="badge sv-label left">The run before · {beforeWhen}</span>
      <span className="badge info sv-label right">This run · {setWhen}</span>
    </div>
  );
}

export function ShotViewer({ sets, set, start, onClose }: { sets: ScreenshotSet[]; set: ScreenshotSet; start: number; onClose: () => void }) {
  const [i, setI] = useState(Math.min(start, set.shots.length - 1));
  const [zoom, setZoom] = useState<"fit" | "full">("fit");
  const [cmp, setCmp] = useState<"side" | "slider" | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [stage, box] = useSize<HTMLDivElement>();
  const dialog = useRef<HTMLDivElement>(null);
  const compared = useMemo(() => compare(sets, set), [sets, set]);
  const shot = set.shots[i];
  const pairing = compared?.pairing.get(shot.fileId);
  const before = compared?.pair.get(shot.fileId) ?? null;
  const n = set.shots.length;
  const answer = answerOf(set);
  const commit = shortCommit(set.commit);
  const comparing = cmp !== null && !!before;
  useEffect(() => { dialog.current?.focus(); }, []);
  // In Compare, ← and → go to the set's next pair; without one left, Compare goes back to one picture.
  const pairs = set.shots.map((s, k) => (compared?.pair.has(s.fileId) ? k : -1)).filter((k) => k >= 0);
  const go = (d: 1 | -1) => {
    setErr(null);
    if (comparing) {
      const next = d > 0 ? pairs.find((k) => k > i) : [...pairs].reverse().find((k) => k < i);
      if (next !== undefined) setI(next);
      return;
    }
    setI((k) => Math.min(n - 1, Math.max(0, k + d)));
  };
  const toggleCompare = () => { if (before) setCmp((c) => (c ? null : "side")); };
  const onKey = (e: ReactKeyboardEvent) => {
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); return; }
    if (e.key === "Tab") { // keep the focus inside the dialog
      const els = [...(dialog.current?.querySelectorAll<HTMLElement>("button:not([disabled]), [tabindex='0']") ?? [])];
      if (!els.length) return;
      const first = els[0], last = els[els.length - 1];
      if (e.shiftKey && (document.activeElement === first || document.activeElement === dialog.current)) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      return;
    }
    if (e.key === "ArrowLeft") { e.preventDefault(); go(-1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); go(1); }
    else if (e.key === "1" && !comparing) { e.preventDefault(); setZoom((z) => (z === "fit" ? "full" : "fit")); }
    else if (e.key.toLowerCase() === "c") { e.preventDefault(); toggleCompare(); }
    e.stopPropagation(); // the page's own keys (], t, n) wait until it is closed
  };
  const open = () => { setErr(null); openFile(shot.fileId).catch((e) => setErr(String(e))); };
  // 100%: drag to look around
  const pan = useRef<{ x: number; y: number; left: number; top: number } | null>(null);
  const panDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (zoom !== "full" || comparing || e.button !== 0) return;
    const el = e.currentTarget;
    pan.current = { x: e.clientX, y: e.clientY, left: el.scrollLeft, top: el.scrollTop };
    el.setPointerCapture?.(e.pointerId);
  };
  const panMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const p = pan.current;
    if (!p || !(e.buttons & 1)) return;
    e.currentTarget.scrollLeft = p.left - (e.clientX - p.x);
    e.currentTarget.scrollTop = p.top - (e.clientY - p.y);
  };
  const inner = { w: Math.max(0, box.w - 160), h: Math.max(0, box.h - 48) }; // room for Previous and Next
  const side = { w: Math.max(0, (box.w - 160 - 24) / 2), h: Math.max(0, box.h - 120) };
  const compareTitle = !compared ? "The run before has no pictures to compare with"
    : pairing === "new" ? `New in this run: the run before has no ${shot.name}` : "Compare with the run before (C)";
  return (
    <>
      <div className="scrim shot-scrim" onClick={onClose} aria-hidden />
      <div className="shot-view" role="dialog" aria-modal="true" aria-label={`Screenshot ${i + 1} of ${n}: ${shot.caption}`} tabIndex={-1} ref={dialog} onKeyDown={onKey}>
        <div className="sv-head">
          <span className="sv-count">{i + 1} of {n}</span>
          <Avatar name={set.agentName} kind="agent" role={set.role} size="sm" />
          <b>{set.agentName}</b>
          <span className={`badge ${answer.cls}`}>{answer.text}</span>
          <span className="faint">{when(set.endedAt)}</span>
          {commit && <span className="commit faint">{commit}</span>}
          <span className="grow" />
          {comparing ? (
            <div className="seg" role="group" aria-label="How to compare">
              <button type="button" aria-pressed={cmp === "side"} onClick={() => setCmp("side")}>Side by side</button>
              <button type="button" aria-pressed={cmp === "slider"} onClick={() => setCmp("slider")}>Slider</button>
            </div>
          ) : (
            <div className="seg" role="group" aria-label="Size">
              <button type="button" aria-pressed={zoom === "fit"} onClick={() => setZoom("fit")} title="Fit in the window">Fit</button>
              <button type="button" aria-pressed={zoom === "full"} onClick={() => setZoom("full")} title="At 100% (1)">100%</button>
            </div>
          )}
          <button type="button" className="btn sm" aria-pressed={comparing} disabled={!before} title={compareTitle} onClick={toggleCompare}>
            <Columns2 className="icon" />Compare</button>
          <button type="button" className="btn sm" onClick={open} title="Open it in the system’s image viewer"><ExternalLink className="icon" />Open</button>
          <button type="button" className="btn ghost sm icon-only" aria-label="Close" title="Close (Esc)" onClick={onClose}><X className="icon" /></button>
        </div>
        {err && <div className="error-banner" role="alert">{err}</div>}
        <div className={`sv-stage${zoom === "full" && !comparing ? " full" : ""}`} ref={stage} onPointerDown={panDown} onPointerMove={panMove}
          onPointerUp={() => { pan.current = null; }}>
          {comparing && before && pairing === "unchanged" ? (
            <div className="sv-center"><Picture shot={shot} size={fit(shot, inner)} /></div>
          ) : comparing && before && cmp === "side" ? (
            <div className="sv-side">
              {([["The run before", "", compared!.before, before], ["This run", "info", set, shot]] as const).map(([label, cls, s, pic]) => (
                <figure key={label} className="sv-pane">
                  <figcaption><span className={`badge ${cls}`}>{label}</span><span>{s.agentName}</span><span className="faint">{when(s.endedAt)}</span>
                    {s.commit && <span className="commit faint">{shortCommit(s.commit)}</span>}</figcaption>
                  <Picture shot={pic} size={fit(pic, side)} label={`${label}: ${pic.caption}`} />
                </figure>
              ))}
            </div>
          ) : comparing && before ? (
            <div className="sv-center"><Slider before={before} shot={shot} box={inner} setWhen={when(set.endedAt)} beforeWhen={when(compared!.before.endedAt)} /></div>
          ) : zoom === "full" ? (
            <div className="sv-full"><Picture shot={shot} /></div>
          ) : (
            <div className="sv-center"><Picture shot={shot} size={fit(shot, inner)} /></div>
          )}
          {(comparing ? pairs.some((k) => k < i) : i > 0) && (
            <button type="button" className="sv-nav prev" aria-label="Previous" title="Previous (←)" onClick={() => go(-1)}><ChevronLeft className="icon" /></button>
          )}
          {(comparing ? pairs.some((k) => k > i) : i < n - 1) && (
            <button type="button" className="sv-nav next" aria-label="Next" title="Next (→)" onClick={() => go(1)}><ChevronRight className="icon" /></button>
          )}
        </div>
        <div className="sv-foot">
          <div className="sv-cap">
            <b>{shot.caption}</b>
            <span className="sv-meta">{formatOf(shot)} · {sizeOf(shot)} · {themeOf(shot)}{pairing && <span className="badge">{PAIRING_TEXT[pairing]}</span>}
              <span className="shot-file">{shot.name}</span></span>
            {comparing && before && (pairing === "unchanged" ? <span className="sv-pair">Unchanged: the same picture as the run before</span>
              : <span className="sv-pair">Paired by file name with the run before’s picture, captioned “{before.caption}”.</span>)}
          </div>
          <div className="sv-keys" aria-hidden>
            {comparing && cmp === "slider" && <span>drag the line</span>}
            <span><span className="kbd">←</span> <span className="kbd">→</span> previous and next</span>
            {!comparing && <span><span className="kbd">1</span> 100%</span>}
            <span><span className="kbd">C</span> compare</span>
            <span><span className="kbd">Esc</span> close</span>
          </div>
        </div>
      </div>
    </>
  );
}
