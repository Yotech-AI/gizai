// The memory graph's canvas (GA-69). The dots and lines are drawn on a canvas and laid out by d3-force (ISC), one tick
// a frame, damped so they settle in a couple of seconds; with reduced motion the layout is worked out at once and drawn
// still. Pointing at a dot lights it and its lines up in the blue accent and dims the rest; a click opens it; a dot can
// be dragged and the layout follows; the wheel zooms and dragging the background pans; labels fade in as you zoom. The
// keyboard: the arrow keys go from dot to dot (the page lists that dot's connections), Enter opens it, + and - zoom and
// 0 fits the graph in view. Animate replays the graph growing in the order its notes were made.
import { useEffect, useImperativeHandle, useRef, type Ref } from "react";
import { forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY, type Simulation, type SimulationLinkDatum, type SimulationNodeDatum } from "d3-force";
import {
  fitTransform, growthOrder, KIND_LOOK, KIND_NAME, labelAlpha, neighbourhood, nextDot, nodeRadius, paletteColor, type Graph, type GraphLink, type GraphNode,
  type GraphSettings, type PaletteKey,
} from "../../lib/graph";

type Dot = SimulationNodeDatum & { id: string; node: GraphNode; r: number; x: number; y: number };
type Line = SimulationLinkDatum<Dot> & { link: GraphLink };
type View = { k: number; x: number; y: number };
type Colors = { theme: "dark" | "light"; dot: string; line: string; accent: string; label: string; text: string; bg: string; font: string };

/** What the canvas can be asked to do from outside. */
export type GraphHandle = { fit: () => void; zoom: (by: number) => void; animate: () => void };

/** A headless probe's view of a canvas (the self-test): every dot on screen with where it is, and whether the layout
 *  has settled. */
export type ProbeDot = { id: string; kind: string; label: string; x: number; y: number; r: number };
const probes = new WeakMap<HTMLCanvasElement, () => { dots: ProbeDot[]; settled: boolean; frameMs: number }>();
export const graphProbe = (canvas: HTMLCanvasElement | null) => (canvas ? probes.get(canvas)?.() ?? null : null);

const reducedMotion = () => typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;

function readColors(): Colors {
  const root = document.documentElement;
  const cs = getComputedStyle(root);
  const v = (k: string, d: string) => cs.getPropertyValue(k).trim() || d;
  return {
    theme: root.dataset.theme === "light" ? "light" : "dark",
    dot: v("--text-3", "#7f838d"), line: v("--text-3", "#7f838d"), accent: v("--accent", "#6f97ff"), label: v("--text-2", "#a5a8b1"),
    text: v("--text", "#ededf0"), bg: v("--bg", "#0f1013"), font: getComputedStyle(document.body).fontFamily || "sans-serif",
  };
}

const MIN_K = 0.03;
const MAX_K = 8;
const clampK = (k: number) => Math.max(MIN_K, Math.min(MAX_K, k));
const ease = (a: number, b: number, t: number) => a + (b - a) * t;

type Props = {
  graph: Graph;
  settings: GraphSettings;
  /** Each note dot's group colour. */
  colors: ReadonlyMap<string, PaletteKey>;
  /** The open note: drawn in the accent with its lines. */
  focus?: string | null;
  /** The local graph's note: the layout keeps it in the middle, and the view fits again when it changes. */
  centre?: string | null;
  /** The dot chosen with the keyboard. */
  selected?: string | null;
  onSelect?: (id: string | null) => void;
  onOpen: (n: GraphNode) => void;
  /** What a screen reader calls the canvas. */
  label: string;
  /** Closer than this the view doesn't fit itself (a local graph of a few dots). */
  maxFit?: number;
  handle?: Ref<GraphHandle>;
};

export function GraphCanvas({ graph, settings, colors, focus, centre, selected, onSelect, onOpen, label, maxFit = 1.6, handle }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const live = useRef<HTMLDivElement>(null);
  // Everything the frame loop reads lives in refs, so a render never restarts the layout.
  const st = useRef({
    dots: new Map<string, Dot>(), lines: [] as Line[], view: { k: 1, x: 0, y: 0 } as View, fit: true, size: { w: 0, h: 0 },
    hover: null as string | null, raf: 0, reduced: reducedMotion(), colors: null as Colors | null, frameMs: 0,
    // Animate: the dots shown so far, in growth order; null shows them all.
    grow: null as { order: Dot[]; shown: Set<string>; at: number } | null,
    drag: null as null | { dot: Dot | null; x0: number; y0: number; view0: View; moved: boolean; id: number },
    props: { graph, settings, colors, focus, centre, selected, onSelect, onOpen },
  });
  st.current.props = { graph, settings, colors, focus, centre, selected, onSelect, onOpen };
  const simRef = useRef<Simulation<Dot, Line> | null>(null);
  if (!simRef.current) {
    simRef.current = forceSimulation<Dot, Line>([]).stop().velocityDecay(0.45).alphaDecay(0.045)
      .force("link", forceLink<Dot, Line>([]).id((d) => d.id))
      .force("charge", forceManyBody<Dot>().distanceMax(800))
      .force("x", forceX<Dot>(0)).force("y", forceY<Dot>(0))
      .force("collide", forceCollide<Dot>().strength(0.6));
  }
  const sim = simRef.current;

  const shownDots = () => { const s = st.current; return [...s.dots.values()].filter((d) => !s.grow || s.grow.shown.has(d.id)); };
  const settled = () => sim.alpha() <= sim.alphaMin() && !st.current.grow && !st.current.drag?.moved;

  // ---- drawing --------------------------------------------------------------------------------------------------------
  const draw = () => {
    const s = st.current;
    const canvas = canvasRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const c = s.colors ?? (s.colors = readColors());
    const { settings: set, focus: open, selected: chosen } = s.props;
    const dpr = window.devicePixelRatio || 1;
    const { k, x, y } = s.view;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const shown = (d: Dot) => !s.grow || s.grow.shown.has(d.id);
    const lines = s.lines.filter((l) => shown(l.source as Dot) && shown(l.target as Dot));
    // What is lit: the dot pointed at (or chosen with the keyboard) and its neighbours; the open note and its lines.
    const hotId = s.hover ?? chosen ?? null;
    const hot = hotId ? neighbourhood({ links: lines.map((l) => l.link) }, hotId) : null;
    const openLit = open ? neighbourhood({ links: lines.map((l) => l.link) }, open) : null;
    const dim = (id: string) => !!hot && !hot.nodes.has(id);
    ctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * x, dpr * y);

    // Lines: thin and low-contrast, those of the lit dots in the accent.
    const lw = Math.max(0.6 / k, 0.8 * set.linkThickness);
    const arrow = Math.max(4 / k, 3.2 * set.linkThickness);
    const strokeLines = (ls: Line[], color: string, alpha: number, width: number) => {
      if (!ls.length) return;
      ctx.globalAlpha = alpha;
      ctx.strokeStyle = color;
      ctx.fillStyle = color;
      ctx.lineWidth = width;
      ctx.beginPath();
      for (const l of ls) { const a = l.source as Dot, b = l.target as Dot; ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); }
      ctx.stroke();
      if (!set.arrows) return;
      ctx.beginPath();
      for (const l of ls) {
        const a = l.source as Dot, b = l.target as Dot;
        const dx = b.x - a.x, dy = b.y - a.y, len = Math.hypot(dx, dy);
        if (len < b.r + arrow) continue;
        const ux = dx / len, uy = dy / len;
        const tx = b.x - ux * (b.r + 1 / k), ty = b.y - uy * (b.r + 1 / k);
        ctx.moveTo(tx, ty);
        ctx.lineTo(tx - ux * arrow - uy * arrow * 0.5, ty - uy * arrow + ux * arrow * 0.5);
        ctx.lineTo(tx - ux * arrow + uy * arrow * 0.5, ty - uy * arrow - ux * arrow * 0.5);
        ctx.closePath();
      }
      ctx.fill();
    };
    const litLine = (l: Line) => (hot ? hot.links.has(l.link) : false) || (!hot && !!openLit?.links.has(l.link));
    strokeLines(lines.filter((l) => !litLine(l)), c.line, hot ? 0.1 : 0.35, lw);
    strokeLines(lines.filter(litLine), c.accent, 0.85, lw * 1.4);

    // Dots: notes grey or their group's colour, links that find no note dim, the other kinds their own shape and colour;
    // the dot pointed at and the open note in the accent.
    const dots = [...s.dots.values()].filter(shown);
    const colorOf = (d: Dot) => {
      if (d.id === hotId || d.id === open) return c.accent;
      if (d.node.kind === "note") { const g = s.props.colors.get(d.id); return g ? paletteColor(g, c.theme) : c.dot; }
      if (d.node.kind === "missing") return c.dot;
      return paletteColor(KIND_LOOK[d.node.kind].color, c.theme);
    };
    const minR = 1.6 / k;
    for (const d of dots) {
      const r = Math.max(d.r, minR);
      ctx.globalAlpha = (dim(d.id) ? 0.18 : 1) * (d.node.kind === "missing" ? 0.45 : 1);
      ctx.fillStyle = colorOf(d);
      ctx.strokeStyle = ctx.fillStyle;
      const shape = d.node.kind === "note" || d.node.kind === "missing" ? "circle" : KIND_LOOK[d.node.kind].shape;
      ctx.beginPath();
      if (shape === "circle") ctx.arc(d.x, d.y, r, 0, Math.PI * 2);
      else if (shape === "ring") { ctx.lineWidth = Math.max(1.2 / k, r * 0.35); ctx.arc(d.x, d.y, r * 0.82, 0, Math.PI * 2); ctx.stroke(); continue; }
      else if (shape === "square") ctx.rect(d.x - r * 0.85, d.y - r * 0.85, r * 1.7, r * 1.7);
      else {
        const n = shape === "diamond" ? 4 : shape === "hexagon" ? 6 : 3;
        const rot = shape === "triangle" ? -Math.PI / 2 : shape === "diamond" ? 0 : Math.PI / 6;
        const rr = shape === "triangle" ? r * 1.2 : r * 1.1;
        for (let i = 0; i < n; i++) { const a = rot + (i * Math.PI * 2) / n; if (i) ctx.lineTo(d.x + Math.cos(a) * rr, d.y + Math.sin(a) * rr); else ctx.moveTo(d.x + Math.cos(a) * rr, d.y + Math.sin(a) * rr); }
        ctx.closePath();
      }
      ctx.fill();
    }
    // The keyboard's dot: a ring around it.
    const ring = chosen ? s.dots.get(chosen) : null;
    if (ring && shown(ring)) {
      ctx.globalAlpha = 1;
      ctx.strokeStyle = c.accent;
      ctx.lineWidth = 2 / k;
      ctx.beginPath();
      ctx.arc(ring.x, ring.y, Math.max(ring.r, minR) + 4 / k, 0, Math.PI * 2);
      ctx.stroke();
    }

    // Labels, in screen pixels in the UI font: they fade in with the zoom; the lit ones always show.
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const fade = labelAlpha(k, set.textFade);
    ctx.font = `11.5px ${c.font}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (const d of dots) {
      const lit = d.id === hotId || d.id === open || (hot ? hot.nodes.has(d.id) : false);
      const a = lit ? 1 : dim(d.id) ? fade * 0.2 : fade;
      if (a < 0.03) continue;
      const sx = d.x * k + x, sy = d.y * k + y;
      if (sx < -200 || sy < -40 || sx > s.size.w + 200 || sy > s.size.h + 40) continue;
      ctx.globalAlpha = a * (d.node.kind === "missing" ? 0.6 : 1);
      ctx.fillStyle = d.id === hotId || d.id === open ? c.text : c.label;
      const text = d.node.label.length > 42 ? `${d.node.label.slice(0, 40)}…` : d.node.label;
      ctx.fillText(text, sx, sy + Math.max(d.r, minR) * k + 3);
    }
    ctx.globalAlpha = 1;
  };

  // ---- the frame loop -------------------------------------------------------------------------------------------------
  const frame = () => {
    const s = st.current;
    s.raf = 0;
    const t0 = performance.now();
    let again = false;
    if (s.grow) again = growStep() || again;
    if (!s.reduced && (sim.alpha() > sim.alphaMin() || s.drag?.moved)) { sim.tick(); again = true; }
    if (s.fit && s.size.w > 0) {
      const target = fitTransform(shownDots(), s.size.w, s.size.h, 36, s.props.centre ? Math.max(maxFit, 2.2) : maxFit);
      const t = s.reduced ? 1 : 0.12;
      const v = { k: ease(s.view.k, target.k, t), x: ease(s.view.x, target.x, t), y: ease(s.view.y, target.y, t) };
      const close = Math.abs(v.k - target.k) < target.k * 0.002 && Math.abs(v.x - target.x) < 0.5 && Math.abs(v.y - target.y) < 0.5;
      s.view = close ? target : v;
      if (!close) again = true;
    }
    draw();
    s.frameMs = s.frameMs ? s.frameMs * 0.9 + (performance.now() - t0) * 0.1 : performance.now() - t0;
    const canvas = canvasRef.current;
    if (canvas) { const done = String(settled()); if (canvas.dataset.settled !== done) canvas.dataset.settled = done; }
    if (again) wake();
  };
  const wake = () => { if (!st.current.raf) st.current.raf = requestAnimationFrame(frame); };
  /** The layout worked out at once (reduced motion): no movement, only the end. */
  const settleNow = () => { sim.alpha(1); sim.tick(Math.ceil(Math.log(sim.alphaMin()) / Math.log(1 - sim.alphaDecay()))); sim.alpha(0); };

  // Animate: a few dots a frame in the order they came, each starting next to a neighbour already there.
  const growStep = (): boolean => {
    const s = st.current;
    const g = s.grow;
    if (!g) return false;
    const per = Math.max(1, Math.ceil(g.order.length / 240));
    for (let i = 0; i < per && g.at < g.order.length; i++, g.at++) {
      const d = g.order[g.at]!;
      if (!s.reduced) {
        const near = s.lines.map((l) => (l.source as Dot).id === d.id ? (l.target as Dot) : (l.target as Dot).id === d.id ? (l.source as Dot) : null).find((o) => o && g.shown.has(o.id));
        const a = Math.random() * Math.PI * 2;
        d.x = (near?.x ?? 0) + Math.cos(a) * 12; d.y = (near?.y ?? 0) + Math.sin(a) * 12; d.vx = 0; d.vy = 0;
      }
      g.shown.add(d.id);
    }
    if (!s.reduced) {
      sim.nodes(g.order.filter((d) => g.shown.has(d.id)));
      (sim.force("link") as ReturnType<typeof forceLink<Dot, Line>>).links(s.lines.filter((l) => g.shown.has((l.source as Dot).id) && g.shown.has((l.target as Dot).id)));
      sim.alpha(Math.max(sim.alpha(), 0.4));
    }
    if (g.at >= g.order.length) {
      s.grow = null;
      if (!s.reduced) { sim.nodes([...s.dots.values()]); (sim.force("link") as ReturnType<typeof forceLink<Dot, Line>>).links(s.lines); }
      return true;
    }
    return true;
  };

  // ---- the graph and the settings -------------------------------------------------------------------------------------
  // New data keeps every dot that stays where it is; a new dot starts next to a neighbour that has a place.
  useEffect(() => {
    const s = st.current;
    const old = s.dots;
    const first = old.size === 0;
    const next = new Map<string, Dot>();
    for (const n of graph.nodes) {
      const had = old.get(n.id);
      const r = nodeRadius(n.links, settings.nodeSize);
      if (had) { had.node = n; had.r = r; next.set(n.id, had); } else next.set(n.id, { id: n.id, node: n, r, x: NaN, y: NaN });
    }
    if (!first) {
      for (const l of graph.links) {
        const a = next.get(l.source), b = next.get(l.target);
        if (!a || !b) continue;
        const [from, to] = Number.isNaN(a.x) && !Number.isNaN(b.x) ? [b, a] : Number.isNaN(b.x) && !Number.isNaN(a.x) ? [a, b] : [null, null];
        if (from && to) { const ang = Math.random() * Math.PI * 2; to.x = from.x + Math.cos(ang) * 16; to.y = from.y + Math.sin(ang) * 16; }
      }
    }
    // d3 puts the rest (on the first load: all of them) on a spiral around the middle.
    for (const d of next.values()) if (Number.isNaN(d.x)) { delete (d as Partial<Dot>).x; delete (d as Partial<Dot>).y; }
    s.dots = next;
    s.lines = graph.links.map((link) => ({ source: link.source, target: link.target, link }));
    s.grow = null;
    if (s.hover && !next.has(s.hover)) s.hover = null;
    sim.nodes([...next.values()]);
    (sim.force("link") as ReturnType<typeof forceLink<Dot, Line>>).links(s.lines);
    applyForces();
    const changed = first || graph.nodes.length !== old.size || graph.nodes.some((n) => !old.has(n.id));
    if (s.reduced) settleNow(); else sim.alpha(Math.max(sim.alpha(), first ? 1 : changed ? 0.5 : 0.15));
    wake();
  }, [graph]); // eslint-disable-line react-hooks/exhaustive-deps

  const applyForces = () => {
    const { settings: set, centre: mid } = st.current.props;
    const degree = new Map<string, number>();
    for (const l of st.current.lines) for (const id of [typeof l.source === "string" ? l.source : (l.source as Dot).id, typeof l.target === "string" ? l.target : (l.target as Dot).id]) degree.set(id, (degree.get(id) ?? 0) + 1);
    const idOf = (e: string | number | Dot) => (typeof e === "object" ? e.id : String(e));
    (sim.force("link") as ReturnType<typeof forceLink<Dot, Line>>).distance(set.linkDistance)
      .strength((l) => set.linkForce / Math.max(1, Math.min(degree.get(idOf(l.source)) ?? 1, degree.get(idOf(l.target)) ?? 1)));
    (sim.force("charge") as ReturnType<typeof forceManyBody<Dot>>).strength(-set.repel * 12);
    const pull = (d: Dot) => (mid && d.id === mid ? 0.5 : set.centre * 0.12);
    (sim.force("x") as ReturnType<typeof forceX<Dot>>).strength(pull);
    (sim.force("y") as ReturnType<typeof forceY<Dot>>).strength(pull);
    (sim.force("collide") as ReturnType<typeof forceCollide<Dot>>).radius((d) => d.r + 2);
  };

  // A change of a force, the dot size or the local graph's note moves the layout again (and fits the view again for a
  // new note).
  useEffect(() => {
    const s = st.current;
    for (const d of s.dots.values()) d.r = nodeRadius(d.node.links, settings.nodeSize);
    applyForces();
    if (s.reduced) settleNow(); else sim.alpha(Math.max(sim.alpha(), 0.3));
    wake();
  }, [settings.centre, settings.repel, settings.linkForce, settings.linkDistance, settings.nodeSize, centre]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { st.current.fit = true; wake(); }, [centre]); // eslint-disable-line react-hooks/exhaustive-deps
  // Anything else only draws again.
  useEffect(() => { wake(); }, [colors, focus, selected, settings.arrows, settings.textFade, settings.linkThickness]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- size, theme, reduced motion, the probe -------------------------------------------------------------------------
  useEffect(() => {
    const canvas = canvasRef.current!;
    const s = st.current;
    const resize = () => {
      const r = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      s.size = { w: r.width, h: r.height };
      canvas.width = Math.max(1, Math.round(r.width * dpr));
      canvas.height = Math.max(1, Math.round(r.height * dpr));
      wake();
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    // The theme, font or text sizes changed (Settings → Appearance): read the colours again.
    const mo = new MutationObserver(() => { s.colors = null; wake(); });
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "data-font", "style", "class"] });
    const mq = typeof matchMedia === "function" ? matchMedia("(prefers-reduced-motion: reduce)") : null;
    const onMotion = () => { s.reduced = !!mq?.matches; if (s.reduced) settleNow(); wake(); };
    mq?.addEventListener("change", onMotion);
    probes.set(canvas, () => {
      const { k, x, y } = s.view;
      const r = canvas.getBoundingClientRect();
      return {
        dots: shownDots().map((d) => ({ id: d.id, kind: d.node.kind, label: d.node.label, x: r.left + d.x * k + x, y: r.top + d.y * k + y, r: Math.max(d.r, 1.6 / k) * k })),
        settled: settled(), frameMs: s.frameMs,
      };
    });
    return () => { ro.disconnect(); mo.disconnect(); mq?.removeEventListener("change", onMotion); cancelAnimationFrame(s.raf); s.raf = 0; probes.delete(canvas); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ---- what the page can ask ------------------------------------------------------------------------------------------
  const zoomAt = (by: number, px: number, py: number) => {
    const s = st.current;
    const k = clampK(s.view.k * by);
    s.view = { k, x: px - ((px - s.view.x) * k) / s.view.k, y: py - ((py - s.view.y) * k) / s.view.k };
    s.fit = false;
    wake();
  };
  useImperativeHandle(handle, () => ({
    fit: () => { st.current.fit = true; wake(); },
    zoom: (by) => zoomAt(by, st.current.size.w / 2, st.current.size.h / 2),
    animate: () => {
      const s = st.current;
      if (!s.dots.size) return;
      if (s.reduced) settleNow();
      s.grow = { order: growthOrder([...s.dots.values()].map((d) => d.node)).map((n) => s.dots.get(n.id)!), shown: new Set(), at: 0 };
      s.fit = true;
      wake();
    },
  }));

  // ---- the mouse ------------------------------------------------------------------------------------------------------
  const dotAt = (px: number, py: number): Dot | null => {
    const s = st.current;
    const wx = (px - s.view.x) / s.view.k, wy = (py - s.view.y) / s.view.k;
    let best: Dot | null = null, bestD = Infinity;
    for (const d of s.dots.values()) {
      if (s.grow && !s.grow.shown.has(d.id)) continue;
      const dist = Math.hypot(d.x - wx, d.y - wy);
      const reach = Math.max(d.r, 1.6 / s.view.k) + 4 / s.view.k;
      if (dist <= reach && dist < bestD) { best = d; bestD = dist; }
    }
    return best;
  };
  const local = (e: { clientX: number; clientY: number }) => { const r = canvasRef.current!.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top] as const; };
  const setHover = (id: string | null) => {
    const s = st.current;
    if (s.hover === id) return;
    s.hover = id;
    const canvas = canvasRef.current;
    if (canvas) canvas.style.cursor = id ? "pointer" : "";
    wake();
  };

  useEffect(() => {
    const canvas = canvasRef.current!;
    // The wheel zooms around the pointer (a listener of its own: React's is passive and can't stop the page scrolling).
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const [px, py] = local(e);
      const dy = e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * 400 : e.deltaY;
      zoomAt(Math.exp(-dy * (e.ctrlKey ? 0.01 : 0.0018)), px, py);
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (e.button !== 0) return;
    const s = st.current;
    const [px, py] = local(e);
    const dot = dotAt(px, py);
    s.drag = { dot, x0: px, y0: py, view0: { ...s.view }, moved: false, id: e.pointerId };
    e.currentTarget.setPointerCapture(e.pointerId);
    if (!dot) e.currentTarget.style.cursor = "grabbing";
  };
  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const s = st.current;
    const [px, py] = local(e);
    const g = s.drag;
    if (!g || g.id !== e.pointerId) { setHover(dotAt(px, py)?.id ?? null); return; }
    if (!g.moved && Math.hypot(px - g.x0, py - g.y0) < 4) return;
    g.moved = true;
    if (g.dot) {
      // Dragging a dot: it follows the pointer and the rest of the layout reacts (with reduced motion only it moves).
      const wx = (px - s.view.x) / s.view.k, wy = (py - s.view.y) / s.view.k;
      g.dot.fx = wx; g.dot.fy = wy;
      if (s.reduced) { g.dot.x = wx; g.dot.y = wy; } else sim.alphaTarget(0.25);
      s.fit = false;
      setHover(g.dot.id);
    } else {
      s.view = { ...g.view0, x: g.view0.x + px - g.x0, y: g.view0.y + py - g.y0 };
      s.fit = false;
    }
    wake();
  };
  const onPointerUp = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const s = st.current;
    const g = s.drag;
    if (!g || g.id !== e.pointerId) return;
    s.drag = null;
    e.currentTarget.style.cursor = g.dot ? "pointer" : "";
    if (g.dot) {
      g.dot.fx = null; g.dot.fy = null;
      sim.alphaTarget(0);
      if (!g.moved) s.props.onOpen(g.dot.node);
    }
    wake();
  };

  // ---- the keyboard ---------------------------------------------------------------------------------------------------
  const onKeyDown = (e: React.KeyboardEvent<HTMLCanvasElement>) => {
    const s = st.current;
    const { selected: chosen, onSelect: choose, focus: open, centre: mid } = s.props;
    const dir = ({ ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down" } as const)[e.key as "ArrowLeft"];
    if (dir) {
      e.preventDefault();
      const dots = shownDots();
      const from = (chosen && s.dots.get(chosen)) || null;
      const start = from ?? s.dots.get(mid ?? open ?? "") ?? [...dots].sort((a, b) => b.node.links - a.node.links)[0];
      const to = from ? nextDot(dots, from, dir) ?? from : start;
      if (to) { choose?.(to.id); say(to); keepInView(to); }
      return;
    }
    if (e.key === "Enter" && chosen) { const d = s.dots.get(chosen); if (d) { e.preventDefault(); s.props.onOpen(d.node); } return; }
    if (e.key === "Escape" && chosen) { e.preventDefault(); choose?.(null); return; }
    if (e.key === "+" || e.key === "=") { e.preventDefault(); zoomAt(1.25, s.size.w / 2, s.size.h / 2); return; }
    if (e.key === "-") { e.preventDefault(); zoomAt(0.8, s.size.w / 2, s.size.h / 2); return; }
    if (e.key === "0") { e.preventDefault(); s.fit = true; wake(); }
  };
  const keepInView = (d: Dot) => {
    const s = st.current;
    const sx = d.x * s.view.k + s.view.x, sy = d.y * s.view.k + s.view.y;
    if (sx < 30 || sy < 30 || sx > s.size.w - 30 || sy > s.size.h - 30) { s.view = { ...s.view, x: s.size.w / 2 - d.x * s.view.k, y: s.size.h / 2 - d.y * s.view.k }; s.fit = false; }
    wake();
  };
  const say = (d: Dot) => {
    const links = s2(d.node.links);
    if (live.current) live.current.textContent = `${d.node.label}, ${KIND_NAME[d.node.kind].toLowerCase()}${d.node.detail ? ` in ${d.node.detail}` : ""}, ${links}`;
  };

  return (
    <>
      <canvas ref={canvasRef} className="graph-canvas" tabIndex={0} role="application" aria-roledescription="graph" aria-label={label}
        onPointerDown={onPointerDown} onPointerMove={onPointerMove} onPointerUp={onPointerUp} onPointerCancel={onPointerUp}
        onPointerLeave={() => { if (!st.current.drag) setHover(null); }} onKeyDown={onKeyDown}
        onBlur={() => { if (live.current) live.current.textContent = ""; }}
        data-dots={graph.nodes.length} data-lines={graph.links.length} />
      <div ref={live} className="sr-only" aria-live="polite" />
    </>
  );
}

const s2 = (n: number) => `${n} ${n === 1 ? "link" : "links"}`;
