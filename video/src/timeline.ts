// The tour's timeline: when each scene and step starts, where the cursor goes and which stills show, all as plain
// functions of the frame. The frame never zooms: every still fills it, at the same size, the whole tour. Durations are
// in seconds, so the GIF's lower frame rate keeps the same pace.
import { Easing, interpolate } from "remotion";
import { HEIGHT, WIDTH, type Box, type Scene, type Step } from "./scenes";

export type Point = { x: number; y: number };
/** smooth: false is the GIF's cut: cuts where the video crossfades, so its frames stay small. */
export type Options = { fps: number; smooth: boolean };

const SECONDS = {
  intro: 2, // the logo alone; frame 0 is the poster
  sceneFade: 0.4, // the crossfade into a scene's first still
  lead: 0.6, // from a scene's start to the cursor's first move
  stepFade: 0.2, // into the next step's before-still, when the step before it changed the page
  settle: 0.12, // the cursor rests on its target before it clicks
  clickDelay: 0.08, // from the click to the crossfade from before to after
  clickFade: 0.3,
  hold: 1.25, // on the after-still
  lastHold: 2.2, // a scene's last step holds longer, to read its caption
  outroFade: 0.6,
  outro: 4,
  cursorFade: 0.4,
  press: 0.25,
  pulse: 0.65,
};

const CURSOR_EASE = Easing.bezier(0.65, 0, 0.35, 1);
const clamp = { extrapolateLeft: "clamp", extrapolateRight: "clamp" } as const;

export type TimedStep = Step & {
  /** frames: the move starts, the cursor arrives, it clicks, the step ends */
  start: number; arrive: number; click: number; end: number;
  from: Point; to: Point;
};
export type TimedScene = Scene & { start: number; end: number; timed: TimedStep[] };
export type Shot = { kind: "intro" | "still" | "outro"; src: string; at: number; fade: number };
export type Timeline = {
  options: Options; total: number; introEnd: number; outroStart: number;
  scenes: TimedScene[]; steps: TimedStep[]; shots: Shot[]; cursorStart: Point;
};

/** Where the cursor lands in a clicked box: its middle, but on a wide row (a sidebar item, a file in the tree) towards
 *  the left, where its label is. */
const target = (b: Box): Point => ({ x: b.x + Math.min(b.width / 2, b.height * 2.5), y: b.y + b.height / 2 });

export function buildTimeline(scenes: Scene[], options: Options): Timeline {
  const { fps, smooth } = options;
  const fr = (seconds: number) => Math.round(seconds * fps);
  const fade = (seconds: number) => (smooth ? fr(seconds) : 0);
  const introEnd = fr(SECONDS.intro);
  const cursorStart = { x: WIDTH * 0.72, y: HEIGHT * 0.78 };
  const shots: Shot[] = [{ kind: "intro", src: "", at: 0, fade: 0 }];
  const steps: TimedStep[] = [];
  const timedScenes: TimedScene[] = [];
  let t = introEnd;
  let at = cursorStart;
  for (const scene of scenes) {
    const start = t;
    shots.push({ kind: "still", src: scene.steps[0].before, at: start, fade: fade(SECONDS.sceneFade) });
    t += fr(SECONDS.lead);
    const timed = scene.steps.map((step, i) => {
      const to = target(step.box);
      const distance = Math.hypot(to.x - at.x, to.y - at.y);
      const move = fr(Math.min(1, Math.max(0.6, 0.5 + distance / 3500)));
      const stepStart = t;
      if (i > 0) shots.push({ kind: "still", src: step.before, at: stepStart, fade: fade(SECONDS.stepFade) });
      const arrive = stepStart + move;
      const click = arrive + fr(SECONDS.settle);
      const end = click + fr(i === scene.steps.length - 1 ? SECONDS.lastHold : SECONDS.hold);
      shots.push({ kind: "still", src: step.after, at: click + fr(SECONDS.clickDelay), fade: fade(SECONDS.clickFade) });
      const timedStep: TimedStep = { ...step, start: stepStart, arrive, click, end, from: at, to };
      t = end;
      at = to;
      steps.push(timedStep);
      return timedStep;
    });
    timedScenes.push({ ...scene, start, end: t, timed });
  }
  const outroStart = t;
  shots.push({ kind: "outro", src: "", at: outroStart, fade: fade(SECONDS.outroFade) });
  return { options, total: outroStart + fr(SECONDS.outro), introEnd, outroStart, scenes: timedScenes, steps, shots, cursorStart };
}

/** The shots to draw at a frame, bottom first: the one fading in sits over the one before it. */
export function shotsAt(tl: Timeline, frame: number): { shot: Shot; index: number; opacity: number }[] {
  let i = 0;
  while (i + 1 < tl.shots.length && tl.shots[i + 1].at <= frame) i++;
  const shot = tl.shots[i];
  if (i > 0 && shot.fade > 0 && frame < shot.at + shot.fade) {
    const opacity = interpolate(frame, [shot.at, shot.at + shot.fade], [0, 1], { ...clamp, easing: Easing.inOut(Easing.quad) });
    return [{ shot: tl.shots[i - 1], index: i - 1, opacity: 1 }, { shot, index: i, opacity }];
  }
  return [{ shot, index: i, opacity: 1 }];
}

/** The step a frame belongs to: the last one that has started, or null before the first. */
export function stepAt(tl: Timeline, frame: number): TimedStep | null {
  let found: TimedStep | null = null;
  for (const step of tl.steps) if (step.start <= frame) found = step;
  return found;
}

export function sceneAt(tl: Timeline, frame: number): TimedScene | null {
  return tl.scenes.find((s) => s.start <= frame && frame < s.end) ?? null;
}

/** The cursor, in the still's pixels and the frame's (they are the same): it glides on a gentle arc, eased, from one click to the next. */
export function cursorAt(tl: Timeline, frame: number): Point {
  const step = stepAt(tl, frame);
  if (!step) return tl.cursorStart;
  if (frame >= step.arrive) return step.to;
  const k = interpolate(frame, [step.start, step.arrive], [0, 1], { ...clamp, easing: CURSOR_EASE });
  const { from, to } = step;
  const dx = to.x - from.x, dy = to.y - from.y;
  const length = Math.hypot(dx, dy) || 1;
  // The arc bows upwards, by a tenth of the distance.
  let nx = -dy / length, ny = dx / length;
  if (ny > 0) { nx = -nx; ny = -ny; }
  const bow = length * 0.1;
  const control = { x: (from.x + to.x) / 2 + nx * bow, y: (from.y + to.y) / 2 + ny * bow };
  const u = 1 - k;
  return {
    x: u * u * from.x + 2 * u * k * control.x + k * k * to.x,
    y: u * u * from.y + 2 * u * k * control.y + k * k * to.y,
  };
}

/** The cursor's opacity, its size while it presses, and the click's ring pulse (0 to 1, or null). */
export function cursorStateAt(tl: Timeline, frame: number): { opacity: number; press: number; pulse: number | null } {
  const fps = tl.options.fps;
  const first = tl.scenes[0]?.start ?? 0;
  const fadeFrames = Math.round(SECONDS.cursorFade * fps);
  const opacity = Math.min(
    interpolate(frame, [first, first + fadeFrames], [0, 1], clamp),
    interpolate(frame, [tl.outroStart, tl.outroStart + fadeFrames], [1, 0], clamp),
  );
  const step = stepAt(tl, frame);
  if (!step) return { opacity, press: 1, pulse: null };
  const half = Math.max(1, Math.round((SECONDS.press * fps) / 2));
  const press = interpolate(frame, [step.click - half, step.click, step.click + half], [1, 0.8, 1], clamp);
  const pulseFrames = Math.round(SECONDS.pulse * fps);
  const pulse = frame >= step.click && frame < step.click + pulseFrames ? (frame - step.click) / pulseFrames : null;
  return { opacity, press, pulse };
}

/** A caption's opacity and lift (in pixels) at a frame, for its scene. */
export function captionStateAt(tl: Timeline, scene: TimedScene, frame: number): { opacity: number; lift: number } {
  if (!tl.options.smooth) return { opacity: 1, lift: 0 };
  const fr = (seconds: number) => Math.round(seconds * tl.options.fps);
  const inStart = scene.start + fr(0.2), inEnd = inStart + fr(0.35);
  const k = interpolate(frame, [inStart, inEnd], [0, 1], { ...clamp, easing: Easing.out(Easing.cubic) });
  const out = interpolate(frame, [scene.end - fr(0.3), scene.end], [1, 0], clamp);
  return { opacity: Math.min(k, out), lift: (1 - k) * 18 };
}
