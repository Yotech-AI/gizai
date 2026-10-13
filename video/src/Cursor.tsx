// The tour's cursor: the board's Review glyph (src/components/StatusIcon.tsx), a ring with a dot, white with a soft
// shadow on dark and dark on light. At each click it presses in and sends out a ring.
import type React from "react";
import type { Point } from "./timeline";

/** About 40px where the site and the GIF show the video at 1280 wide. */
export const CURSOR_SIZE = 76;

export const Cursor: React.FC<{
  at: Point; opacity: number; press: number; pulse: number | null; color: string; shadow: string;
}> = ({ at, opacity, press, pulse, color, shadow }) => {
  const ring = (CURSOR_SIZE * 6.25) / 8; // the glyph's ring, in the cursor's pixels: its diameter
  return (
    <>
      {pulse !== null && (
        <div
          style={{
            position: "absolute",
            left: at.x,
            top: at.y,
            width: ring,
            height: ring,
            marginLeft: -ring / 2,
            marginTop: -ring / 2,
            borderRadius: "50%",
            border: `${CURSOR_SIZE * 0.07}px solid ${color}`,
            transform: `scale(${1 + 1.7 * (1 - (1 - pulse) ** 3)})`,
            opacity: opacity * 0.85 * (1 - pulse),
          }}
        />
      )}
      <svg
        width={CURSOR_SIZE}
        height={CURSOR_SIZE}
        viewBox="0 0 16 16"
        fill="none"
        style={{
          position: "absolute",
          left: at.x - CURSOR_SIZE / 2,
          top: at.y - CURSOR_SIZE / 2,
          opacity,
          transform: `scale(${press})`,
          filter: `drop-shadow(0 3px 8px ${shadow}) drop-shadow(0 0 2px ${shadow})`,
          overflow: "visible",
        }}
      >
        <circle cx="8" cy="8" r="6.25" stroke={color} strokeWidth="1.6" />
        <circle cx="8" cy="8" r="2.4" fill={color} />
      </svg>
    </>
  );
};
