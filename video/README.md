# The product tour (GA-149)

GizAi's product tour for the website's hero and the README, made with [Remotion](https://www.remotion.dev) 4 from the
stills `scripts/tour-capture.sh` captured (GA-148). A cursor, the board's Review glyph, clicks through every feature
in dark and in light. Remotion's free licence covers YoTech (a company of two).

This folder is its own npm project, apart from the app: the app's `package.json`, `npm ci`, `npm run build`, `npm test`
and CI don't see it.

```sh
cd video
npm install          # Remotion, React and TypeScript, in video/node_modules
npm run studio       # the tour in Remotion Studio, in your browser
npm run render       # every file in docs/tour/ again, both themes (npm run render -- dark: one theme)
npm run typecheck
```

`npm run render` needs `ffmpeg` on the PATH (for VP9 and the GIF's palette). The first render downloads Chrome Headless
Shell into `node_modules/.remotion`, so it needs the network once. The whole render takes about a quarter of an hour (16 threads); the VP9
encode is the slow part.

## What it renders

Into `docs/tour/`, for `dark` and `light`:

| File | What it is |
|---|---|
| `tour-<theme>.mp4` | H.264, 2560 × 1664 at 30 fps, for the website. Two passes, aimed at 21 MB (limit 25 MB). |
| `tour-<theme>.webm` | VP9, the same, aimed at 17 MB (limit 20 MB). |
| `tour-<theme>.gif` | The README's demo: 1280 × 832 at 15 fps, ffmpeg's palette, under 10 MB. |
| `tour-<theme>-poster.png` | The first frame: the logo. |

Remotion renders a master per theme into `out/` (git ignores it), and ffmpeg encodes the MP4 and the WebM from it. The
GIF is its own cut, `TourGif-<theme>`: the same tour without the zoom, and with cuts where the video crossfades. A GIF
redraws the whole picture for every frame that zooms, and the zoomed tour came to about 90 MB; without it, about 3 MB.

## How it is built

- `remotion.config.ts`: the public folder is `capture/`, so the stills stay where the capture wrote them.
- `src/scenes.ts`: the scenes from `capture/<theme>/scenes.json` in the website's order (its feature rows 01 to 08, so
  Bring your own CLI comes fourth, after the Team Lead's chat), each with its caption: the feature's name from the
  homepage's list (`design/website/README.md`) and one short line. Also the colours, from `design/tokens.json` and the
  mountains' tokens (`design/branding`, at strength strong, as on the website's hero).
- `src/timeline.ts`: when each scene and step starts, where the cursor is, which stills show and how far the camera
  zooms, as plain functions of the frame. The durations are in seconds, so the GIF's 15 fps keeps the same pace.
- `src/Tour.tsx`, `Cursor.tsx`, `Caption.tsx` and `Ends.tsx` draw them.

What the tour does:

- **The cursor** is a ring with a dot (the Review glyph, `src/components/StatusIcon.tsx`), 76 px in the video, about 40 px
  where the website and the GIF show it 1280 wide: white with a soft shadow on dark, dark on light. It glides from
  click to click on a gentle arc, eased, and lands in the clicked element's box (its middle, or on a wide row towards
  the label on the left). At each click it presses in and sends out a ring.
- **Each click** crossfades in 0.3 s from the still before it to the still after it.
- **The zoom** goes towards the clicked box while the cursor travels, at most 1.3× (less for a big box), and back out
  at the end of the step.
- **The captions** sit at the bottom left: the feature's number in the logo's pink and its name in Geist, one line in
  Atkinson Hyperlegible Next, both from `src/assets/fonts/`.
- **The opening** is the logo (`design/brand/logo-horizontal-on-<theme>.svg`); **the closing** is the logo and "An
  Agentic Development Application" over the mountains of the website's hero (`design/branding/mountains-hero.svg`).

New stills from `scripts/tour-capture.sh` need nothing here: render again. A scene added to the capture needs its
place in `ORDER` and a caption in `src/scenes.ts`.
