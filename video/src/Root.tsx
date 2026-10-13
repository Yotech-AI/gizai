// The compositions: the tour in dark and in light for the website (Tour-<theme>, 30 fps), and the README's GIF cut
// (TourGif-<theme>, rendered at --scale 0.5): the same tour with cuts for crossfades, so a GIF of it stays under 10 MB.
import type React from "react";
import { Composition } from "remotion";
import { HEIGHT, WIDTH, type Theme } from "./scenes";
import { Tour, tourTimeline } from "./Tour";

export const VIDEO_FPS = 30;
export const GIF_FPS = 15; // render.mjs encodes the GIF at this rate too
const THEMES: Theme[] = ["dark", "light"];

export const Root: React.FC = () => (
  <>
    {THEMES.map((theme) => (
      <Composition
        key={`video-${theme}`}
        id={`Tour-${theme}`}
        component={Tour}
        width={WIDTH}
        height={HEIGHT}
        fps={VIDEO_FPS}
        durationInFrames={tourTimeline(theme, VIDEO_FPS, true).total}
        defaultProps={{ theme, smooth: true }}
      />
    ))}
    {THEMES.map((theme) => (
      <Composition
        key={`gif-${theme}`}
        id={`TourGif-${theme}`}
        component={Tour}
        width={WIDTH}
        height={HEIGHT}
        fps={GIF_FPS}
        durationInFrames={tourTimeline(theme, GIF_FPS, false).total}
        defaultProps={{ theme, smooth: false }}
      />
    ))}
  </>
);
