// The product tour: the opening, the eight scenes (each step's still before its click and after it, the cursor and the
// caption), and the closing. Every still fills the frame at its own size: the frame never zooms.
import type React from "react";
import { useMemo } from "react";
import { AbsoluteFill, Img, interpolate, staticFile, useCurrentFrame, useVideoConfig } from "remotion";
import "./fonts";
import { Caption } from "./Caption";
import { Cursor } from "./Cursor";
import { Closing, Opening } from "./Ends";
import { COLORS, HEIGHT, WIDTH, scenesFor, type Theme } from "./scenes";
import { buildTimeline, captionStateAt, cursorAt, cursorStateAt, sceneAt, shotsAt } from "./timeline";

export type TourProps = { theme: Theme; smooth: boolean };

export function tourTimeline(theme: Theme, fps: number, smooth: boolean) {
  return buildTimeline(scenesFor(theme), { fps, smooth });
}

const Still: React.FC<{ src: string }> = ({ src }) => (
  <AbsoluteFill style={{ overflow: "hidden" }}>
    <Img src={staticFile(src)} style={{ position: "absolute", left: 0, top: 0, width: WIDTH, height: HEIGHT }} />
  </AbsoluteFill>
);

export const Tour: React.FC<TourProps> = ({ theme, smooth }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const tl = useMemo(() => tourTimeline(theme, fps, smooth), [theme, fps, smooth]);
  const c = COLORS[theme];
  const scene = sceneAt(tl, frame);
  const cursor = cursorStateAt(tl, frame);
  const at = cursorAt(tl, frame);
  const introProgress = smooth ? interpolate(frame, [0, tl.introEnd], [0, 1], { extrapolateRight: "clamp" }) : 0;
  const outroProgress = smooth
    ? interpolate(frame, [tl.outroStart, tl.outroStart + Math.round(0.9 * fps)], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" })
    : 1;

  return (
    <AbsoluteFill style={{ background: c.bg }}>
      {shotsAt(tl, frame).map(({ shot, index, opacity }) => (
        <AbsoluteFill key={index} style={{ opacity }}>
          {shot.kind === "intro" && <Opening theme={theme} progress={introProgress} />}
          {shot.kind === "still" && <Still src={shot.src} />}
          {shot.kind === "outro" && <Closing theme={theme} progress={outroProgress} />}
        </AbsoluteFill>
      ))}
      {scene && <Caption caption={scene.caption} theme={theme} {...captionStateAt(tl, scene, frame)} />}
      {cursor.opacity > 0 && (
        <Cursor at={at} opacity={cursor.opacity} press={cursor.press} pulse={cursor.pulse} color={c.cursor} shadow={c.cursorShadow} />
      )}
    </AbsoluteFill>
  );
};
